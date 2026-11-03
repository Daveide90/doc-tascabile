//! La firma nel modo Compila (piano «PDF completo», P3).
//!
//! «Firma» nella proposta di Fill & Sign, sul riquadro di un campo firma («Tocca per firmare»,
//! bozzetto 7) e nel foglio Strumenti apre il foglio della firma: la firma e le iniziali salvate,
//! da mettere con un tocco; una firma nuova col dito, nel foglio a tutto schermo (in verticale o in
//! orizzontale, tratti lisci, Cancella e Fatto; dalla 0.8.2 il tratto, stilografica, penna o
//! pennarello, e il colore, nero o blu, che ridisegnano subito la firma fatta e restano con la
//! firma salvata); oppure la foto di una firma su carta, dalla
//! fotocamera o da un'immagine, che il motore pulisce (via carta, ombre e tavolo) e che si guarda
//! prima di usarla. La firma va dove si è toccato (o nel campo), resta scelta come gli altri segni
//! di Fill & Sign (si sposta, si ingrandisce, si toglie) ed è una modifica che si annulla.
//!
//! La firma e le iniziali si salvano solo nella memoria privata dell'app (`firme/`), per le volte
//! dopo; «Cancella firma salvata» nelle Impostazioni le toglie. La foto passa da un file della
//! cache, che si cancella appena letta. Nella diagnosi solo generi e misure: mai tratti, pixel o
//! nomi di file.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError};

use eframe::egui::{self, Color32, Id, Order, Pos2, Rect, Sense, Stroke, StrokeKind, UiBuilder, WidgetInfo, WidgetType, pos2, vec2};
use tascabile_pdf::firma::{ALTA, ALTE_INIZIALI, LARGA, LARGHE_INIZIALI};
use tascabile_pdf::tratto::{Colore, Pezzo, Stile, contorni_del_tratto, liscia, punti_del_dito};
use tascabile_pdf::{ErroreFoto, Firma, Modifica};
use tascabile_platform::{EsitoFoto, FonteFoto, diagnosi};
use tascabile_touch::tema::{self, Carattere};
use tascabile_touch::{BarraAlta, Chip, Dialogo, Peso, Pulsante, RispostaDialogo, Tavolozza, Tinta, icone};

use crate::TascabileApp;
use crate::pdf_lavoro::ModoPdf;

pub(crate) const FIRMA: &str = "Firma";
pub(crate) const METTI_LA_FIRMA: &str = "Metti la firma";
pub(crate) const METTI_LE_INIZIALI: &str = "Metti le iniziali";
pub(crate) const FIRMA_COL_DITO: &str = "Firma col dito";
pub(crate) const INIZIALI_COL_DITO: &str = "Iniziali col dito";
pub(crate) const FOTO_DELLA_FIRMA: &str = "Foto della firma su carta";
pub(crate) const FIRMA_DA_IMMAGINE: &str = "Firma da un'immagine";
pub(crate) const CANCELLA: &str = "Cancella";
/// Dopo Cancella, finché non si ricomincia a firmare: rimette la firma cancellata (0.8.3).
pub(crate) const RIMETTI: &str = "Rimetti";
pub(crate) const FATTO: &str = "Fatto";
pub(crate) const CHIUDI: &str = "Chiudi";
pub(crate) const USA_QUESTA_FIRMA: &str = "Usa questa firma";
pub(crate) const RIFAI: &str = "Rifai";
pub(crate) const TOCCA_PER_FIRMARE: &str = "Tocca per firmare";
pub(crate) const TOCCA_DOVE: &str = "Tocca dove mettere la firma";
pub(crate) const FIRMA_DALLA_FOTO: &str = "La firma dalla foto";
/// La domanda prima di sostituire la firma salvata, e le sue risposte (revisione del 31/10, punto 3).
pub(crate) const SALVARE_LA_NUOVA: &str = "Salvare la firma nuova?";
pub(crate) const TIENI_QUELLA_DI_PRIMA: &str = "Tieni quella di prima";
pub(crate) const SALVA_LA_NUOVA: &str = "Salva la nuova";
const SPIEGA_SALVARE: &str = "In Tascabile c'è già una firma salvata. La nuova è sul documento: vuoi che prenda anche il posto di \
     quella salvata, per le volte dopo?";
const SPIEGA_SALVARE_INIZIALI: &str = "In Tascabile ci sono già delle iniziali salvate. Le nuove sono sul documento: vuoi che \
     prendano anche il posto di quelle salvate, per le volte dopo?";
/// Detto la prima volta che una firma si salva.
pub(crate) const FIRMA_SALVATA: &str = "La firma resta salvata in Tascabile per la prossima volta: la togli nelle Impostazioni.";
const SI_PULISCE: &str = "Tolgo carta, ombre e tavolo…";
const FIRMA_QUI: &str = "Firma qui col dito";

/// L'immagine più grande che si legge dalla cache (Java ne copia al massimo 64 MiB).
const MASSIMO_FOTO: u64 = 64 << 20;
/// I lati di un capo tondo sullo schermo (mezzo cerchio).
const PASSI_DEL_CAPO: usize = 8;
/// Da questa larghezza (dp) il tratto, il colore, Cancella e Fatto stanno su una fila sola; più
/// stretto (in verticale) tratto e colore hanno una fila loro, alta tanto.
const LARGO_PER_UNA_FILA: f32 = 820.0;
const FILA_DEL_TRATTO: f32 = 52.0;
/// Il fondo del riquadro «Tocca per firmare»: bianco con un velo di rosso, come `.firma-box`.
const FONDO_FIRMA: Color32 = Color32::from_rgb(0xFC, 0xF3, 0xF1);

/// Le firme salvate nella memoria privata dell'app: una firma e le iniziali, lette la prima volta
/// che servono.
#[derive(Default)]
pub(crate) struct FirmeSalvate {
    lette: bool,
    firma: Option<Firma>,
    iniziali: Option<Firma>,
    /// Le immagini da mostrare delle due, fatte quando servono.
    texture: [Option<egui::TextureHandle>; 2],
}

/// Il nome del file di una firma salvata, senza estensione.
fn nome(iniziali: bool) -> &'static str {
    if iniziali { "iniziali" } else { "firma" }
}

impl FirmeSalvate {
    /// Le firme della cartella `cartella` (`firme/` dei dati dell'app).
    pub(crate) fn leggi(cartella: &Path) -> FirmeSalvate {
        let leggi = |iniziali: bool| {
            ["txt", "png"].iter().find_map(|est| {
                let p = cartella.join(format!("{}.{est}", nome(iniziali)));
                let corto = std::fs::metadata(&p).is_ok_and(|m| m.len() <= 8 << 20);
                corto.then(|| std::fs::read(&p).ok()).flatten().and_then(|b| Firma::da_byte(&b))
            })
        };
        let s = FirmeSalvate { lette: true, firma: leggi(false), iniziali: leggi(true), texture: [None, None] };
        diagnosi(format!("firma: salvate lette, firma {}, iniziali {}", s.firma.is_some(), s.iniziali.is_some()));
        s
    }

    pub(crate) fn di(&self, iniziali: bool) -> Option<&Firma> {
        if iniziali { self.iniziali.as_ref() } else { self.firma.as_ref() }
    }

    pub(crate) fn qualcuna(&self) -> bool {
        self.firma.is_some() || self.iniziali.is_some()
    }

    /// Salva `firma` come firma o come iniziali, al posto di quella di prima.
    pub(crate) fn salva(&mut self, cartella: &Path, iniziali: bool, firma: Firma) -> std::io::Result<()> {
        std::fs::create_dir_all(cartella)?;
        let p = cartella.join(format!("{}.{}", nome(iniziali), firma.estensione()));
        tascabile_word::salva::scrivi_atomico(&p, &firma.in_byte())?;
        let altra = if firma.estensione() == "txt" { "png" } else { "txt" };
        let _ = std::fs::remove_file(cartella.join(format!("{}.{altra}", nome(iniziali))));
        self.texture[usize::from(iniziali)] = None;
        if iniziali {
            self.iniziali = Some(firma);
        } else {
            self.firma = Some(firma);
        }
        Ok(())
    }

    /// Toglie la firma e le iniziali salvate: tutta la cartella, anche un temporaneo rimasto da un
    /// salvataggio a metà (revisione del 31/10, punto 12).
    pub(crate) fn cancella(&mut self, cartella: &Path) {
        let _ = std::fs::remove_dir_all(cartella);
        *self = FirmeSalvate { lette: true, ..Default::default() };
    }
}

/// Dove va la firma che si sceglie.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Dove {
    /// Dove si è toccato: la pagina e il punto (pt), al centro della firma.
    Punto(usize, Pos2),
    /// Nel riquadro di un campo firma: la pagina e il riquadro (pt).
    Riquadro(usize, [f32; 4]),
    /// Si tocca dopo (dal foglio Strumenti).
    DaToccare,
}

/// Il foglio a tutto schermo per firmare col dito: i tratti finiti (già lisci, con la velocità
/// del dito) e quello che si disegna, in dp dall'angolo del riquadro; il tratto e il colore scelti.
#[derive(Default)]
pub(crate) struct Blocco {
    pub(crate) iniziali: bool,
    pub(crate) tratti: Vec<Vec<tascabile_pdf::tratto::Punto>>,
    /// Quello che si disegna: i punti come arrivano, [x, y, ms dall'inizio], e quando è
    /// cominciato (il tempo di egui, in secondi).
    in_corso: Option<(Vec<[f32; 3]>, f64)>,
    /// La firma tolta da Cancella, che Rimetti rimette finché non si ricomincia a firmare.
    cancellati: Option<Vec<Vec<tascabile_pdf::tratto::Punto>>>,
    pub(crate) stile: Stile,
    pub(crate) colore: Colore,
}

/// La firma nel modo Compila.
#[derive(Default)]
pub(crate) struct FirmaPdf {
    /// Dove va la firma che si sta scegliendo o facendo.
    pub(crate) dove: Option<Dove>,
    /// Il foglio per firmare col dito, aperto.
    pub(crate) blocco: Option<Blocco>,
    /// La foto chiesta ad Android, che non è ancora arrivata: da dove, e il numero della
    /// richiesta (solo il suo esito è per la firma).
    pub(crate) foto: Option<(FonteFoto, i32)>,
    /// La foto che il motore pulisce, sul suo thread.
    lavoro: Option<Receiver<Result<Firma, ErroreFoto>>>,
    /// La firma dalla foto, da guardare prima di usarla, con la sua immagine.
    pub(crate) anteprima: Option<(Firma, Option<egui::TextureHandle>)>,
    /// La prossima firma (o le iniziali, se vero) va dove si tocca la pagina: questa.
    pub(crate) da_toccare: Option<(bool, Firma)>,
    /// Una firma nuova (o le iniziali, se vero) già posata, da salvare al posto di quella salvata
    /// se si risponde di sì.
    pub(crate) da_salvare: Option<(bool, Firma)>,
}

/// La firma disegnata nel riquadro `r`, centrata e con le sue proporzioni: i contorni pieni dei
/// tratti, col loro tratto e colore (y verso l'alto), o l'immagine `texture`.
fn disegna_firma(painter: &egui::Painter, r: Rect, firma: &Firma, texture: Option<&egui::TextureHandle>) {
    match firma {
        Firma::Tratti(d) => {
            let Some([x0, y0, x1, y1]) = d.inchiostro() else { return };
            let s = (r.width() / (x1 - x0).max(1e-3)).min(r.height() / (y1 - y0).max(1e-3));
            let dentro = Rect::from_center_size(r.center(), vec2((x1 - x0) * s, (y1 - y0) * s));
            let px = 1.0 / painter.ctx().pixels_per_point();
            let a_schermo = |p: [f32; 2]| pos2(dentro.left() + (p[0] - x0) * s, dentro.bottom() - (p[1] - y0) * s);
            dipingi_pezzi(painter, &d.pezzi_meno(px / s), a_schermo, colore_di(d.colore));
        }
        Firma::Immagine(_) => {
            let k = r.width().min(r.height() / firma.proporzione().max(1e-3));
            let dentro = Rect::from_center_size(r.center(), vec2(k, k * firma.proporzione()));
            if let Some(t) = texture {
                painter.image(t.id(), dentro, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            }
        }
    }
}

/// Il colore dell'inchiostro sullo schermo.
fn colore_di(c: Colore) -> Color32 {
    let [r, g, b] = c.rgb();
    Color32::from_rgb(r, g, b)
}

/// I pezzi pieni sullo schermo: i triangoli fra i due bordi e i ventagli dei capi, e attorno un
/// filo di un pixel dello stesso colore che egui smussa (i triangoli no). Gli spessori dei pezzi
/// sono già meno quel pixel (`meno_il_filo`).
fn dipingi_pezzi(painter: &egui::Painter, pezzi: &[Pezzo], a_schermo: impl Fn([f32; 2]) -> Pos2, colore: Color32) {
    let px = 1.0 / painter.ctx().pixels_per_point();
    let mut mesh = egui::Mesh::default();
    for p in pezzi {
        let base = mesh.vertices.len() as u32;
        for (s, d) in p.sinistra.iter().zip(&p.destra) {
            mesh.colored_vertex(a_schermo(*s), colore);
            mesh.colored_vertex(a_schermo(*d), colore);
        }
        for k in 0..p.sinistra.len().saturating_sub(1) as u32 {
            let (a, b) = (base + 2 * k, base + 2 * k + 2);
            mesh.add_triangle(a, a + 1, b + 1);
            mesh.add_triangle(a, b + 1, b);
        }
        for fine in [false, true] {
            // Il ventaglio del capo, dal bordo dove arriva il contorno a quello da dove riparte.
            let (primo, ultimo) = if fine { (p.sinistra[p.sinistra.len() - 1], p.destra[p.destra.len() - 1]) } else { (p.destra[0], p.sinistra[0]) };
            let arco: Vec<[f32; 2]> = std::iter::once(primo).chain(p.arco(fine, PASSI_DEL_CAPO)).chain(std::iter::once(ultimo)).collect();
            let centro = mesh.vertices.len() as u32;
            mesh.colored_vertex(a_schermo(p.capi[usize::from(fine)].centro), colore);
            for q in &arco {
                mesh.colored_vertex(a_schermo(*q), colore);
            }
            for k in 0..arco.len() as u32 - 1 {
                mesh.add_triangle(centro, centro + 1 + k, centro + 2 + k);
            }
        }
        painter.add(egui::Shape::closed_line(p.contorno(PASSI_DEL_CAPO).into_iter().map(&a_schermo).collect(), Stroke::new(px, colore)));
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// Il tratto e il colore con cui si comincia a firmare col dito: quelli della firma (o delle
/// iniziali) salvata col dito, se c'è; se no quelli dell'altra; se no la stilografica nera.
fn tratto_di_partenza(questa: Option<&Firma>, altra: Option<&Firma>) -> (Stile, Colore) {
    [questa, altra]
        .into_iter()
        .flatten()
        .find_map(|f| match f {
            Firma::Tratti(d) => Some((d.stile, d.colore)),
            Firma::Immagine(_) => None,
        })
        .unwrap_or_default()
}

/// L'immagine da mostrare di una firma da immagine.
fn texture_di(ctx: &egui::Context, firma: &Firma, nome: &str) -> Option<egui::TextureHandle> {
    let Firma::Immagine(i) = firma else { return None };
    let img = egui::ColorImage::from_rgba_unmultiplied(i.size(), i.rgba());
    Some(ctx.load_texture(nome, img, egui::TextureOptions::LINEAR))
}

/// Una riga del foglio della firma, alta 56 dp: l'icona e il testo, o (con `anteprima`) la firma
/// salvata al posto dell'icona.
pub(crate) fn riga(
    ui: &mut egui::Ui,
    icona: &str,
    testo: &str,
    anteprima: Option<(&Firma, Option<&egui::TextureHandle>)>,
    ultima: bool,
) -> egui::Response {
    let ctx = ui.ctx().clone();
    let largo = ui.available_width();
    let (rect, r) = ui.allocate_exact_size(vec2(largo, 56.0), Sense::click());
    r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, testo));
    if r.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, egui::CornerRadius::same(tema::RAGGIO_SEGMENTO), tema::PREMUTO);
    }
    let mut x = rect.left() + 48.0;
    match anteprima {
        Some((f, t)) => {
            let box_ = Rect::from_min_size(pos2(rect.left() + 8.0, rect.top() + 6.0), vec2(96.0, 44.0));
            ui.painter().rect(box_, 8.0, tema::FOGLIO, Stroke::new(1.0, tema::FILO), StrokeKind::Inside);
            disegna_firma(ui.painter(), box_.shrink(5.0), f, t);
            x = box_.right() + 14.0;
        }
        None => {
            ui.painter().text(
                pos2(rect.left() + 8.0 + tema::ICONA / 2.0, rect.center().y),
                egui::Align2::CENTER_CENTER,
                icona,
                Carattere::Icona.font(&ctx, tema::ICONA),
                tema::MATITA,
            );
        }
    }
    let colore = if anteprima.is_some() { tema::BLU } else { tema::INCHIOSTRO };
    let carattere = if anteprima.is_some() { Carattere::Grassetto } else { Carattere::Testo };
    let g = ui.painter().layout_no_wrap(testo.to_string(), carattere.font(&ctx, 16.0), colore);
    ui.painter().galley(pos2(x, rect.center().y - g.size().y / 2.0), g, colore);
    if !ultima {
        ui.painter().rect_filled(Rect::from_min_max(pos2(rect.left(), rect.bottom() - 1.0), rect.max), 0.0, tema::FILO);
    }
    r
}

/// Quello che si è scelto nel foglio della firma.
enum Scelta {
    Metti(bool),
    Dito(bool),
    Foto(FonteFoto),
}

/// Il riquadro «Tocca per firmare» del bozzetto 7 (`.firma-box`) sopra un campo firma: bordo
/// rosso tratteggiato, l'icona e il testo rossi in mezzo.
pub(crate) fn tocca_per_firmare(painter: &egui::Painter, ctx: &egui::Context, r: Rect) {
    painter.rect_filled(r, 8.0, FONDO_FIRMA);
    let punti = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
    painter.extend(egui::Shape::dashed_line(&punti, Stroke::new(1.5, tema::ROSSO), 5.0, 3.0));
    let misura = (r.height() * 0.24).clamp(8.0, 15.0);
    let g = painter.layout_no_wrap(TOCCA_PER_FIRMARE.to_string(), Carattere::Grassetto.font(ctx, misura), tema::ROSSO);
    let icona = painter.layout_no_wrap(icone::SIGNATURE.to_string(), Carattere::Icona.font(ctx, misura * 1.5), tema::ROSSO);
    let largo = icona.size().x + 6.0 + g.size().x;
    let x = r.center().x - largo / 2.0;
    painter.galley(pos2(x, r.center().y - icona.size().y / 2.0), icona.clone(), tema::ROSSO);
    painter.galley(pos2(x + icona.size().x + 6.0, r.center().y - g.size().y / 2.0), g, tema::ROSSO);
}

impl TascabileApp {
    /// La cartella delle firme salvate, nella memoria privata dell'app.
    fn cartella_firme(&self) -> PathBuf {
        self.piattaforma.cartella_app().join("firme")
    }

    /// Le firme salvate, lette la prima volta.
    pub(crate) fn firme(&mut self) -> &mut FirmeSalvate {
        if !self.firme.lette {
            self.firme = FirmeSalvate::leggi(&self.cartella_firme());
        }
        &mut self.firme
    }

    /// «Cancella firma salvata» delle Impostazioni.
    pub(crate) fn cancella_firma_salvata(&mut self) {
        let cartella = self.cartella_firme();
        self.firme.cancella(&cartella);
        diagnosi("firma: firma e iniziali salvate cancellate");
        self.avviso_breve.avvisa("Firma salvata cancellata");
    }

    /// Apre il foglio della firma per mettere la firma `dove`.
    pub(crate) fn apri_firma_pdf(&mut self, dove: Dove) {
        self.conferma_scrittura_pdf(true);
        if self.pdf.as_ref().is_some_and(|p| p.modo != ModoPdf::Compila) {
            self.scegli_modo_pdf(ModoPdf::Compila);
        }
        let salvate = self.firme().qualcuna();
        let Some(p) = self.pdf.as_mut() else { return };
        p.compila.proposta = None;
        p.compila.segno = None;
        p.compila.firma.dove = Some(dove);
        p.compila.firma.da_toccare = None;
        diagnosi(format!("pdf: foglio della firma aperto, firme salvate {salvate}"));
        self.fogli_pdf.firma.apri();
    }

    /// La firma (o le iniziali) salvata va dove si è scelto; dal foglio Strumenti si tocca dopo.
    fn metti_firma_pdf(&mut self, iniziali: bool) {
        if self.firme().di(iniziali).is_none() {
            return;
        }
        // Prima l'impronta o il PIN (roadmap della sicurezza, Fase 1): un collega col telefono
        // sbloccato in mano non mette la tua firma. Per qualche minuto non si richiede (`blocco`).
        if self.impostazioni.impronta_firma {
            self.con_l_impronta(crate::blocco::PerCosa::Firma(iniziali));
        } else {
            self.posa_la_firma_salvata(iniziali);
        }
    }

    /// La firma (o le iniziali) salvata va dove si è scelto, dopo l'impronta se serve.
    pub(crate) fn posa_la_firma_salvata(&mut self, iniziali: bool) {
        let Some(firma) = self.firme().di(iniziali).cloned() else { return };
        self.posa_firma_pdf(iniziali, firma);
    }

    /// Questa firma (o queste iniziali) va dove si è scelto: una salvata, o una appena fatta, anche
    /// se non si è potuta salvare. Fino alla 0.8.4 la firma nuova passava dalle salvate, e se non
    /// si salvava si posava la vecchia (revisione del 31/10, punto 3).
    fn posa_firma_pdf(&mut self, iniziali: bool, firma: Firma) {
        let autore = self.impostazioni.autore.clone();
        let Some(p) = self.pdf.as_mut() else { return };
        let Some(dove) = p.compila.firma.dove.take() else { return };
        let (pagina, modifica) = match dove {
            Dove::DaToccare => {
                p.compila.firma.da_toccare = Some((iniziali, firma));
                diagnosi("pdf: firma da posare dove si tocca");
                self.avviso_breve.avvisa(TOCCA_DOVE);
                return;
            }
            Dove::Punto(pagina, pt) => {
                let Some(info) = p.motore.stato().geometrie.get(pagina).cloned() else { return };
                let (largo, alto) = if iniziali { (LARGHE_INIZIALI, ALTE_INIZIALI) } else { (LARGA, ALTA) };
                let w = largo.min(alto / firma.proporzione().max(1e-3));
                (pagina, Modifica::firma(pagina, &info, [pt.x - w / 2.0, pt.y], &firma, largo, alto, iniziali, &autore))
            }
            Dove::Riquadro(pagina, r) => {
                let Some(info) = p.motore.stato().geometrie.get(pagina).cloned() else { return };
                (pagina, Modifica::firma_nel_riquadro(pagina, &info, r, &firma, iniziali, &autore))
            }
        };
        let Some(m) = modifica else { return };
        let genere = if matches!(firma, Firma::Tratti(_)) { "col dito" } else { "da un'immagine" };
        diagnosi(format!("pdf: {} posate, {genere}, nella pagina {}", if iniziali { "iniziali" } else { "firma" }, pagina + 1));
        p.compila.da_scegliere = Some(pagina);
        self.modifica_pdf(m);
    }

    /// Un tocco sulla pagina con una firma da posare dove si tocca: la firma va lì (vero).
    pub(crate) fn tocco_per_la_firma_pdf(&mut self, pagina: usize, pt: Pos2) -> bool {
        let Some((iniziali, firma)) = self.pdf.as_mut().and_then(|p| p.compila.firma.da_toccare.take()) else { return false };
        if let Some(p) = self.pdf.as_mut() {
            p.compila.firma.dove = Some(Dove::Punto(pagina, pt));
        }
        self.posa_firma_pdf(iniziali, firma);
        true
    }

    /// Una firma nuova è pronta: va dove si è scelto e si salva per le volte dopo. Se ce n'è già
    /// una salvata, prima di sostituirla si chiede (revisione del 31/10, punto 3: un collega col
    /// telefono in mano metteva la sua al posto di quella di chi usa l'app, senza domande).
    fn firma_nuova_pdf(&mut self, iniziali: bool, firma: Firma) {
        if self.firme().di(iniziali).is_some() {
            diagnosi(format!("firma: {} nuova, si chiede se sostituire la salvata", nome(iniziali)));
            if let Some(p) = self.pdf.as_mut() {
                p.compila.firma.da_salvare = Some((iniziali, firma.clone()));
            }
        } else if self.salva_firma(iniziali, firma.clone()) {
            self.avviso_breve.avvisa(FIRMA_SALVATA);
        }
        self.posa_firma_pdf(iniziali, firma);
    }

    /// Salva la firma (o le iniziali) per le volte dopo: vero se è salvata.
    fn salva_firma(&mut self, iniziali: bool, firma: Firma) -> bool {
        let cartella = self.cartella_firme();
        let detta = format!("{firma:?}");
        match self.firme().salva(&cartella, iniziali, firma) {
            Ok(()) => {
                diagnosi(format!("firma: {} salvata, {detta}", nome(iniziali)));
                true
            }
            Err(e) => {
                diagnosi(format!("firma: {} non salvata ({:?})", nome(iniziali), e.kind()));
                self.avviso_breve.avvisa("La firma non si è salvata: si mette lo stesso, ma la prossima volta va rifatta.");
                false
            }
        }
    }

    /// La domanda prima di sostituire la firma salvata: «Tieni quella di prima» (o un tocco fuori, o
    /// Indietro) la lascia; «Salva la nuova» la sostituisce. La firma nuova è già sul documento.
    pub(crate) fn mostra_salva_firma_pdf(&mut self, ctx: &egui::Context) {
        let Some((iniziali, _)) = self.pdf.as_ref().and_then(|p| p.compila.firma.da_salvare.as_ref()) else { return };
        let risposta = Dialogo::new(egui::Id::new("salva_firma_pdf"), SALVARE_LA_NUOVA)
            .testo(if *iniziali { SPIEGA_SALVARE_INIZIALI } else { SPIEGA_SALVARE })
            .pulsante(TIENI_QUELLA_DI_PRIMA, Peso::Testo)
            .pulsante(SALVA_LA_NUOVA, Peso::Principale)
            .mostra(ctx);
        let Some(risposta) = risposta else { return };
        let Some((iniziali, firma)) = self.pdf.as_mut().and_then(|p| p.compila.firma.da_salvare.take()) else { return };
        if risposta == RispostaDialogo::Pulsante(1) {
            self.salva_firma(iniziali, firma);
        } else {
            diagnosi(format!("firma: {} salvata lasciata com'era", nome(iniziali)));
        }
    }

    /// Indietro coi fogli della firma: chiude il foglio col dito, l'anteprima della foto, il
    /// foglio della firma o la firma da posare (vero).
    pub(crate) fn indietro_firma_pdf(&mut self) -> bool {
        let Some(p) = self.pdf.as_mut() else { return false };
        if p.compila.firma.blocco.take().is_some() {
            diagnosi("indietro: firma col dito chiusa");
            return true;
        }
        if self.fogli_pdf.foto.indietro() {
            p.compila.firma.anteprima = None;
            p.compila.firma.lavoro = None;
            diagnosi("indietro: firma dalla foto chiusa");
            return true;
        }
        if self.fogli_pdf.firma.indietro() {
            diagnosi("indietro: foglio della firma chiuso");
            return true;
        }
        if p.compila.firma.da_toccare.take().is_some() {
            diagnosi("indietro: firma da posare lasciata");
            return true;
        }
        false
    }

    /// A ogni frame: la foto chiesta ad Android, se è arrivata, va al motore su un thread suo; la
    /// firma pulita va nell'anteprima.
    pub(crate) fn passo_firma_pdf(&mut self, ctx: &egui::Context) {
        if let Some((fonte, richiesta)) = self.pdf.as_ref().and_then(|p| p.compila.firma.foto)
            && let Some(esito) = self.piattaforma.prendi_foto_firma(richiesta)
        {
            if let Some(p) = self.pdf.as_mut() {
                p.compila.firma.foto = None;
            }
            match esito {
                EsitoFoto::Pronta(file) => {
                    diagnosi(format!("foto della firma: arrivata dalla {}", fonte.nome()));
                    let (tx, rx) = std::sync::mpsc::channel();
                    let ctx2 = ctx.clone();
                    let avviato = std::thread::Builder::new().name("firma-foto".into()).spawn(move || {
                        let inizio = std::time::Instant::now();
                        let letta = std::fs::File::open(&file).and_then(|f| {
                            let mut v = Vec::new();
                            std::io::Read::read_to_end(&mut std::io::Read::take(f, MASSIMO_FOTO + 1), &mut v).map(|_| v)
                        });
                        crate::butta_la_foto(&file);
                        let esito = match letta {
                            Ok(b) if b.len() as u64 <= MASSIMO_FOTO => tascabile_pdf::firma::firma_da_foto(&b),
                            Ok(_) => Err(ErroreFoto::Grande),
                            Err(_) => Err(ErroreFoto::Lettura),
                        };
                        let ms = inizio.elapsed().as_millis();
                        match &esito {
                            Ok(f) => diagnosi(format!("foto della firma: pulita in {ms} ms, {f:?}")),
                            Err(e) => diagnosi(format!("foto della firma: niente firma ({}) in {ms} ms", e.genere())),
                        }
                        let _ = tx.send(esito);
                        ctx2.request_repaint();
                    });
                    match (avviato, self.pdf.as_mut()) {
                        (Ok(_), Some(p)) => {
                            p.compila.firma.lavoro = Some(rx);
                            p.compila.firma.anteprima = None;
                            self.fogli_pdf.foto.apri();
                        }
                        _ => diagnosi("foto della firma: thread non avviato"),
                    }
                }
                EsitoFoto::Pronte { file, .. } => crate::butta_le_foto(&file, "foto della firma"),
                EsitoFoto::Annullata => diagnosi("foto della firma: annullata"),
                EsitoFoto::Superata => diagnosi("foto della firma: superata da un'altra richiesta"),
                EsitoFoto::Errore => {
                    diagnosi("foto della firma: non arrivata");
                    self.avviso_breve.avvisa("La foto non è arrivata.");
                }
            }
        }
        let Some(p) = self.pdf.as_mut() else { return };
        let Some(rx) = p.compila.firma.lavoro.as_ref() else { return };
        match rx.try_recv() {
            Ok(Ok(firma)) => {
                let t = texture_di(ctx, &firma, "firma_dalla_foto");
                p.compila.firma.anteprima = Some((firma, t));
                p.compila.firma.lavoro = None;
            }
            Ok(Err(e)) => {
                p.compila.firma.lavoro = None;
                self.fogli_pdf.foto.chiudi();
                self.avviso_breve.avvisa(e.messaggio());
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => p.compila.firma.lavoro = None,
        }
    }

    /// Per le prove a schermo: arriva la foto della firma `byte` chiesta ad Android (sul PC).
    #[doc(hidden)]
    #[cfg(not(target_os = "android"))]
    pub fn simula_foto_firma_di_prova(&self, byte: Option<&[u8]>) {
        self.piattaforma.simula_foto_firma(byte);
    }

    /// Per le prove: la firma del foglio col dito, come la vede chi la fa (tratti in dp).
    #[doc(hidden)]
    pub fn tratti_della_firma_pdf(&self) -> Option<usize> {
        self.pdf.as_ref()?.compila.firma.blocco.as_ref().map(|b| b.tratti.len())
    }

    /// Il foglio della firma, l'anteprima della foto e il foglio a tutto schermo per firmare col
    /// dito; da mostrare a ogni frame.
    pub(crate) fn mostra_fogli_firma(&mut self, ctx: &egui::Context) {
        let (firma, iniziali_salvate) = {
            let f = self.firme();
            (f.firma.clone(), f.iniziali.clone())
        };
        for (i, f) in [&firma, &iniziali_salvate].into_iter().enumerate() {
            if let Some(f) = f
                && self.firme.texture[i].is_none()
            {
                self.firme.texture[i] = texture_di(ctx, f, if i == 0 { "firma_salvata" } else { "iniziali_salvate" });
            }
        }
        let texture = self.firme.texture.clone();
        let scelta = self
            .fogli_pdf
            .firma
            .mostra(ctx, FIRMA, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let mut scelta = None;
                if let Some(f) = &firma
                    && riga(ui, icone::SIGNATURE, METTI_LA_FIRMA, Some((f, texture[0].as_ref())), false).clicked()
                {
                    scelta = Some(Scelta::Metti(false));
                }
                if let Some(f) = &iniziali_salvate
                    && riga(ui, icone::SIGNATURE, METTI_LE_INIZIALI, Some((f, texture[1].as_ref())), false).clicked()
                {
                    scelta = Some(Scelta::Metti(true));
                }
                if riga(ui, icone::GESTURE, FIRMA_COL_DITO, None, false).clicked() {
                    scelta = Some(Scelta::Dito(false));
                }
                if riga(ui, icone::DRAW, INIZIALI_COL_DITO, None, false).clicked() {
                    scelta = Some(Scelta::Dito(true));
                }
                if riga(ui, icone::PHOTO_CAMERA, FOTO_DELLA_FIRMA, None, false).clicked() {
                    scelta = Some(Scelta::Foto(FonteFoto::Fotocamera));
                }
                if riga(ui, icone::IMAGE, FIRMA_DA_IMMAGINE, None, true).clicked() {
                    scelta = Some(Scelta::Foto(FonteFoto::Galleria));
                }
                scelta
            })
            .flatten();
        match scelta {
            Some(Scelta::Metti(iniziali)) => {
                self.fogli_pdf.firma.chiudi();
                self.metti_firma_pdf(iniziali);
            }
            Some(Scelta::Dito(iniziali)) => {
                self.fogli_pdf.firma.chiudi();
                let (questa, altra) = if iniziali { (&iniziali_salvate, &firma) } else { (&firma, &iniziali_salvate) };
                let (stile, colore) = tratto_di_partenza(questa.as_ref(), altra.as_ref());
                diagnosi(format!("pdf: {} col dito, {} {}", if iniziali { "iniziali" } else { "firma" }, stile.chiave(), colore.chiave()));
                if let Some(p) = self.pdf.as_mut() {
                    p.compila.firma.blocco = Some(Blocco { iniziali, stile, colore, ..Default::default() });
                }
            }
            Some(Scelta::Foto(fonte)) => {
                self.fogli_pdf.firma.chiudi();
                let richiesta = self.piattaforma.chiedi_foto_firma(fonte);
                if let Some(p) = self.pdf.as_mut() {
                    p.compila.firma.foto = Some((fonte, richiesta));
                }
            }
            None => {}
        }
        self.mostra_anteprima_foto(ctx);
        self.mostra_blocco_firma(ctx);
    }

    /// L'anteprima della firma dalla foto: mentre il motore pulisce, la riga che lo dice; poi la
    /// firma su bianco, «Usa questa firma» e «Rifai».
    fn mostra_anteprima_foto(&mut self, ctx: &egui::Context) {
        enum Tocco {
            Usa,
            Rifai,
        }
        let Some(p) = self.pdf.as_ref() else {
            self.fogli_pdf.foto.chiudi();
            return;
        };
        let anteprima = p.compila.firma.anteprima.as_ref().map(|(f, t)| (f.clone(), t.clone()));
        let tocco = self
            .fogli_pdf
            .foto
            .mostra(ctx, FIRMA_DALLA_FOTO, |ui| {
                let largo = ui.available_width();
                let (r, _) = ui.allocate_exact_size(vec2(largo, 150.0), Sense::hover());
                ui.painter().rect(r, 12.0, tema::FOGLIO, Stroke::new(1.0, tema::FILO), StrokeKind::Inside);
                let Some((f, t)) = &anteprima else {
                    ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, SI_PULISCE, Carattere::Testo.font(ui.ctx(), 15.0), tema::MATITA);
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
                    return None;
                };
                disegna_firma(ui.painter(), r.shrink(16.0), f, t.as_ref());
                ui.add_space(14.0);
                let mut tocco = None;
                ui.horizontal(|ui| {
                    if ui.add(Pulsante::new(RIFAI, Peso::Testo).larghezza(110.0)).clicked() {
                        tocco = Some(Tocco::Rifai);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(Pulsante::new(USA_QUESTA_FIRMA, Peso::Principale).larghezza(190.0)).clicked() {
                            tocco = Some(Tocco::Usa);
                        }
                    });
                });
                ui.add_space(6.0);
                tocco
            })
            .flatten();
        if !self.fogli_pdf.foto.aperto()
            && let Some(p) = self.pdf.as_mut()
        {
            p.compila.firma.anteprima = None;
            p.compila.firma.lavoro = None;
        }
        match tocco {
            Some(Tocco::Usa) => {
                self.fogli_pdf.foto.chiudi();
                let firma = self.pdf.as_mut().and_then(|p| p.compila.firma.anteprima.take()).map(|(f, _)| f);
                if let Some(f) = firma {
                    diagnosi("pdf: firma dalla foto usata");
                    self.firma_nuova_pdf(false, f);
                }
            }
            Some(Tocco::Rifai) => {
                self.fogli_pdf.foto.chiudi();
                diagnosi("pdf: firma dalla foto da rifare");
                if let Some(p) = self.pdf.as_mut() {
                    p.compila.firma.anteprima = None;
                }
                self.fogli_pdf.firma.apri();
            }
            None => {}
        }
    }

    /// Il foglio a tutto schermo per firmare col dito: la barra alta con Chiudi, il riquadro bianco
    /// con la riga, i tratti pieni; sotto il tratto (stilografica, penna, pennarello) e il colore
    /// (nero, blu), che ridisegnano subito la firma fatta, poi Cancella e Fatto. In verticale il
    /// tratto ha una fila sua, sopra; il colore sta fra Cancella e Fatto.
    fn mostra_blocco_firma(&mut self, ctx: &egui::Context) {
        let Some(b) = self.pdf.as_mut().and_then(|p| p.compila.firma.blocco.as_mut()) else { return };
        let schermo = ctx.content_rect();
        let titolo = if b.iniziali { INIZIALI_COL_DITO } else { FIRMA_COL_DITO };
        let una_fila = schermo.width() >= LARGO_PER_UNA_FILA;
        let fondo = if una_fila { 76.0 } else { 76.0 + FILA_DEL_TRATTO };
        let mut chiudi = false;
        let mut fatto = false;
        egui::Area::new(Id::new("firma_col_dito")).order(Order::Foreground).fixed_pos(schermo.min).constrain(false).show(ctx, |ui| {
            ui.set_min_size(schermo.size());
            // Tutto lo schermo: niente arriva alla pagina sotto.
            let _ = ui.interact(schermo, Id::new("firma_col_dito_fondo"), Sense::click_and_drag());
            ui.painter().rect_filled(schermo, 0.0, tema::CARTA);
            let barra = Rect::from_min_size(schermo.min, vec2(schermo.width(), tema::BARRA_ALTA));
            let r = ui.scope_builder(UiBuilder::new().max_rect(barra), |ui| BarraAlta::new(titolo).indietro(CHIUDI).mostra(ui)).inner;
            chiudi = r.indietro;
            let blocco =
                Rect::from_min_max(pos2(schermo.left() + 12.0, barra.bottom() + 4.0), pos2(schermo.right() - 12.0, schermo.bottom() - fondo));
            ui.painter().rect(blocco, 16.0, tema::FOGLIO, Stroke::new(1.0, tema::FILO), StrokeKind::Inside);
            // La riga su cui si firma, con la croce davanti, a due terzi dell'altezza.
            let y = blocco.top() + blocco.height() * 0.66;
            ui.painter().line_segment([pos2(blocco.left() + 28.0, y), pos2(blocco.right() - 28.0, y)], Stroke::new(1.5, tema::LINEETTA));
            ui.painter().text(pos2(blocco.left() + 36.0, y - 16.0), egui::Align2::CENTER_CENTER, "×", Carattere::Testo.font(ctx, 22.0), tema::MATITA);
            if b.tratti.is_empty() && b.in_corso.is_none() {
                ui.painter().text(
                    pos2(blocco.center().x, y + 28.0),
                    egui::Align2::CENTER_CENTER,
                    FIRMA_QUI,
                    Carattere::Testo.font(ctx, 15.0),
                    tema::MATITA,
                );
            }
            // Il dito: ogni movimento è un punto del tratto, col suo tempo (i movimenti dello
            // stesso frame si dividono il tempo dal frame prima).
            let risposta = ui.interact(blocco, Id::new("firma_col_dito_blocco"), Sense::click_and_drag());
            risposta.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, titolo));
            let giu = risposta.is_pointer_button_down_on() || risposta.dragged();
            if giu {
                let dentro = |p: Pos2| {
                    let p = blocco.clamp(p);
                    [p.x - blocco.left(), p.y - blocco.top()]
                };
                let mut nuovi: Vec<[f32; 2]> = ui.input(|i| {
                    i.events
                        .iter()
                        .filter_map(|e| match e {
                            egui::Event::PointerMoved(p) => Some(dentro(*p)),
                            _ => None,
                        })
                        .collect()
                });
                if let Some(p) = ui.input(|i| i.pointer.interact_pos()) {
                    nuovi.push(dentro(p));
                }
                let ora = ui.input(|i| i.time);
                // Si ricomincia a firmare: la firma cancellata non torna più.
                b.cancellati = None;
                let (tratto, inizio) = b.in_corso.get_or_insert_with(|| {
                    let primo = ui.input(|i| i.pointer.press_origin()).map(dentro);
                    (primo.map(|p| vec![[p[0], p[1], 0.0]]).unwrap_or_default(), ora)
                });
                let ms = ((ora - *inizio) * 1000.0) as f32;
                let prima = tratto.last().map_or(ms, |p| p[2]);
                let quanti = nuovi.len() as f32;
                for (k, p) in nuovi.into_iter().enumerate() {
                    if tratto.last().is_some_and(|u| (u[0], u[1]) == (p[0], p[1])) {
                        continue;
                    }
                    tratto.push([p[0], p[1], prima + (ms - prima) * (k + 1) as f32 / quanti]);
                }
            } else if let Some((t, _)) = b.in_corso.take()
                && !t.is_empty()
            {
                b.tratti.push(liscia(&punti_del_dito(&t)));
            }
            // I tratti pieni, col tratto e il colore di adesso.
            let px = 1.0 / ctx.pixels_per_point();
            let colore = colore_di(b.colore);
            let pittore = ui.painter().with_clip_rect(blocco);
            let a_schermo = |p: [f32; 2]| pos2(blocco.left() + p[0], blocco.top() + p[1]);
            for t in &b.tratti {
                dipingi_pezzi(&pittore, &contorni_del_tratto(t, b.stile, 1.0, px), a_schermo, colore);
            }
            if let Some((t, _)) = &b.in_corso {
                dipingi_pezzi(&pittore, &contorni_del_tratto(&liscia(&punti_del_dito(t)), b.stile, 1.0, px), a_schermo, colore);
            }
            // In fondo il tratto, il colore, Cancella e Fatto. In verticale: le chip del tratto su
            // una fila loro; sotto Cancella, i colori in mezzo e Fatto, larghi quanto c'è posto
            // (sul telefono da 372 dp, a 120 dp si toccavano quasi: 0.8.3). In orizzontale
            // una fila sola: le chip, i colori, poi Cancella e Fatto a destra.
            let fila =
                Rect::from_min_max(pos2(schermo.left() + 16.0, schermo.bottom() - 64.0), pos2(schermo.right() - 16.0, schermo.bottom() - 12.0));
            const FRA: f32 = 12.0;
            const TINTE: f32 = 96.0;
            let largo_pulsante = if una_fila { 120.0 } else { ((fila.width() - TINTE - 2.0 * FRA) / 2.0).min(120.0) };
            let alto = tema::PULSANTE;
            let y = fila.center().y - alto / 2.0;
            let fatto_qui = Rect::from_min_size(pos2(fila.right() - largo_pulsante, y), vec2(largo_pulsante, alto));
            let cancella = if una_fila {
                fatto_qui.translate(vec2(-(largo_pulsante + FRA), 0.0))
            } else {
                Rect::from_min_size(pos2(fila.left(), y), vec2(largo_pulsante, alto))
            };
            let tratti_fila = if una_fila {
                Rect::from_min_max(fila.min, pos2(cancella.left() - FRA - TINTE - FRA, fila.bottom()))
            } else {
                Rect::from_min_size(pos2(fila.left(), fila.top() - FILA_DEL_TRATTO), vec2(fila.width(), 48.0))
            };
            let fine_chip = ui
                .scope_builder(UiBuilder::new().max_rect(tratti_fila), |ui| {
                    ui.horizontal_centered(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        for stile in Stile::TUTTI {
                            if ui.add(Chip::new(stile.nome()).acceso(b.stile == stile)).clicked() && b.stile != stile {
                                b.stile = stile;
                                diagnosi(format!("pdf: firma col dito, tratto {}", stile.chiave()));
                            }
                        }
                    })
                    .response
                    .rect
                    .right()
                })
                .inner;
            let posto_tinte = if una_fila {
                Rect::from_min_size(pos2(fine_chip + FRA, y), vec2(TINTE, alto))
            } else {
                Rect::from_center_size(fila.center(), vec2(TINTE, alto))
            };
            let tinte = Colore::TUTTI.map(|c| (Tinta::Colore(colore_di(c)), c.nome()));
            ui.scope_builder(UiBuilder::new().max_rect(posto_tinte), |ui| {
                let acceso = Colore::TUTTI.iter().position(|c| *c == b.colore);
                if let Some(i) = Tavolozza::new(&tinte).acceso(acceso).mostra(ui)
                    && b.colore != Colore::TUTTI[i]
                {
                    b.colore = Colore::TUTTI[i];
                    diagnosi(format!("pdf: firma col dito, colore {}", b.colore.chiave()));
                }
            });
            // Cancella toglie la firma ma la tiene da parte: finché non si ricomincia, Rimetti.
            let rimetti = b.tratti.is_empty() && b.in_corso.is_none() && b.cancellati.is_some();
            let (nome, icona) = if rimetti { (RIMETTI, icone::UNDO) } else { (CANCELLA, icone::DELETE) };
            if ui.put(cancella, Pulsante::new(nome, Peso::Tenue).icona(icona).larghezza(largo_pulsante)).clicked() {
                if rimetti {
                    b.tratti = b.cancellati.take().unwrap_or_default();
                    diagnosi("pdf: firma col dito rimessa");
                } else if !b.tratti.is_empty() {
                    b.cancellati = Some(std::mem::take(&mut b.tratti));
                    b.in_corso = None;
                    diagnosi("pdf: firma col dito cancellata");
                }
            }
            if ui.put(fatto_qui, Pulsante::new(FATTO, Peso::Principale).icona(icone::CHECK).larghezza(largo_pulsante)).clicked() {
                fatto = true;
            }
            if giu {
                ctx.request_repaint();
            }
        });
        if chiudi {
            diagnosi("pdf: firma col dito chiusa");
            if let Some(p) = self.pdf.as_mut() {
                p.compila.firma.blocco = None;
            }
            return;
        }
        if !fatto {
            return;
        }
        let Some(b) = self.pdf.as_mut().and_then(|p| p.compila.firma.blocco.take()) else { return };
        let punti: usize = b.tratti.iter().map(Vec::len).sum();
        match Firma::da_tratti(&b.tratti, b.stile, b.colore) {
            Some(f) => {
                diagnosi(format!("pdf: firma col dito fatta, {} tratti, {punti} punti, {} {}", b.tratti.len(), b.stile.chiave(), b.colore.chiave()));
                self.firma_nuova_pdf(b.iniziali, f);
            }
            None => {
                // Niente inchiostro: il foglio resta aperto.
                if let Some(p) = self.pdf.as_mut() {
                    p.compila.firma.blocco = Some(b);
                }
                self.avviso_breve.avvisa("Firma col dito nel riquadro bianco, poi tocca Fatto.");
            }
        }
    }
}
