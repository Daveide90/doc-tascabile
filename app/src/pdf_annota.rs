//! Annotare un PDF (piano «PDF completo», P5): il modo Annota della barra del bozzetto 7 e le voci
//! Evidenzia, Penna e Nota del foglio Strumenti del bozzetto 8.
//!
//! Sopra la barra dei modi una striscia con gli attrezzi: Scorri (il dito scorre le pagine e
//! sceglie le annotazioni), Penna, Gomma, Nota e Testo; sotto, per la penna i cinque colori e i
//! tre spessori, per il testo i colori e la misura. Con la penna il dito disegna e non scorre; il
//! pizzico resta libero (due dita fermano il tratto). La gomma toglie i tratti della penna dove il
//! dito passa. Il testo si evidenzia, si sottolinea o si barra dalla selezione della pressione
//! lunga (D4): nella barra del contesto Evidenzia, Sottolinea e Barra, con i colori sotto.
//!
//! Un tocco su un'annotazione la sceglie: riquadro blu, si sposta col dito, la sua barra ha
//! Riscrivi (il testo libero), Formato (colore, spessore, misura) e Togli; una nota apre il suo
//! foglio col testo. Le forme del modo Modifica si scelgono allo stesso modo. Tutto va al motore
//! come una modifica che si annulla; l'autore in `/T` viene dalle Impostazioni.
//!
//! Nella diagnosi solo generi e conti: mai un testo, un autore, un tratto.

use eframe::egui::{self, Color32, CornerRadius, Id, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Vec2, WidgetInfo, WidgetType, pos2, vec2};
use tascabile_pdf::{Annotazione, AzioniSelezione, Evento, GenereAnnotazione, Marca, Modifica, SceltaSelezione, annotazione_sotto};
use tascabile_platform::diagnosi;
use tascabile_touch::tema::{self, Carattere};
use tascabile_touch::{BarraContesto, BarraFlottante, Chip, Passo, Peso, Pulsante, PulsanteIcona, Tavolozza, Tinta, Verso, etichetta_sezione, icone};

use crate::pdf_compila::{Bersaglio, RISCRIVI, TOGLI};
use crate::pdf_lavoro::ModoPdf;
use crate::{Errore, TascabileApp};

pub(crate) const EVIDENZIA: &str = "Evidenzia";
pub(crate) const SOTTOLINEA: &str = "Sottolinea";
pub(crate) const BARRA: &str = "Barra";
pub(crate) const SCORRI: &str = "Scorri";
pub(crate) const PENNA: &str = "Penna";
pub(crate) const GOMMA: &str = "Gomma";
pub(crate) const NOTA: &str = "Nota";
pub(crate) const TESTO: &str = "Testo";
pub(crate) const FORMATO: &str = "Formato";
pub(crate) const PIU_PICCOLO: &str = "Testo più piccolo";
pub(crate) const PIU_GRANDE: &str = "Testo più grande";
pub(crate) const ANNOTAZIONE: &str = "Annotazione scelta";
const DISEGNA_CON_IL_DITO: &str = "Disegna col dito sulla pagina";

/// I colori della penna, dei testi e delle forme.
pub(crate) const COLORI: [([f32; 3], &str); 5] = [
    ([0.0, 0.0, 0.0], "Nero"),
    ([0.14, 0.27, 0.63], "Blu"),
    ([0.80, 0.11, 0.11], "Rosso"),
    ([0.09, 0.50, 0.20], "Verde"),
    ([0.95, 0.55, 0.05], "Arancione"),
];
/// I colori dell'evidenziatore, della sottolineatura e della barra.
pub(crate) const COLORI_MARCA: [([f32; 3], &str); 5] = [
    ([1.0, 0.90, 0.10], "Giallo"),
    ([0.45, 0.85, 0.35], "Verde chiaro"),
    ([0.40, 0.75, 1.0], "Azzurro"),
    ([1.0, 0.55, 0.75], "Rosa"),
    ([0.85, 0.12, 0.12], "Rosso scuro"),
];
/// Gli spessori della penna e delle forme, in pt.
pub(crate) const SPESSORI: [(f32, &str); 3] = [(1.0, "Sottile"), (2.5, "Medio"), (5.0, "Spesso")];
/// La misura dei testi liberi, in pt, e i suoi passi.
const MISURA: f32 = 14.0;
const PASSO_MISURA: f32 = 2.0;
/// La gomma prende tanti dp attorno al dito.
const RAGGIO_GOMMA: f32 = 14.0;
/// Il tocco prende un'annotazione anche un poco fuori: tanti dp.
const BORDO: f32 = 8.0;
/// L'altezza di una riga della striscia degli attrezzi, e lo spazio fra le righe e sopra la barra.
pub(crate) const RIGA: f32 = 52.0;
const FRA: f32 = 8.0;
const RAGGIO_COLORE: f32 = 11.0;

pub(crate) fn colore32(c: [f32; 3]) -> Color32 {
    Color32::from_rgb((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8)
}

/// Che cosa fa il dito sulle pagine nel modo Annota.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Attrezzo {
    /// Scorre e sceglie le annotazioni; la pressione lunga seleziona il testo da segnare.
    #[default]
    Dito,
    Penna,
    Gomma,
    /// Il tocco dopo mette una nota (o un testo libero), poi si torna al dito.
    Nota,
    Testo,
}

impl Attrezzo {
    const TUTTI: [(Attrezzo, &'static str, &'static str); 5] = [
        (Attrezzo::Dito, icone::PAN_TOOL, SCORRI),
        (Attrezzo::Penna, icone::DRAW, PENNA),
        (Attrezzo::Gomma, icone::INK_ERASER, GOMMA),
        (Attrezzo::Nota, icone::STICKY_NOTE_2, NOTA),
        (Attrezzo::Testo, icone::TEXT_FIELDS, TESTO),
    ];

    fn nome(self) -> &'static str {
        Self::TUTTI.iter().find(|a| a.0 == self).map_or("", |a| a.2)
    }

    /// Il dito disegna e non scorre le pagine.
    fn disegna(self) -> bool {
        matches!(self, Attrezzo::Penna | Attrezzo::Gomma)
    }
}

/// Un tratto mandato al motore: si disegna qui finché la pagina rifatta non lo mostra.
struct InVolo {
    pagina: usize,
    punti: Vec<[f32; 2]>,
    colore: [f32; 3],
    spessore: f32,
    /// Il motore ha risposto: resta finché arriva l'immagine nuova della pagina.
    risposto: bool,
}

/// Lo stato del modo Annota di un PDF.
pub(crate) struct StatoAnnota {
    pub(crate) attrezzo: Attrezzo,
    pub(crate) colore_penna: usize,
    pub(crate) spessore: usize,
    pub(crate) colore_marca: usize,
    pub(crate) colore_testo: usize,
    pub(crate) misura_testo: f32,
    /// Il tratto della penna o il percorso della gomma che si disegna: la pagina e i punti (pt).
    tratto: Option<(usize, Vec<[f32; 2]>)>,
    /// Due dita: il tratto è finito lì, fino a dita alzate.
    pizzico: bool,
    in_volo: Vec<InVolo>,
    /// L'annotazione scelta (pagina, posizione), e di quanto si trascina (dp).
    pub(crate) scelta: Option<(usize, usize)>,
    trascina: Option<Vec2>,
    /// L'annotazione nel foglio della nota.
    pub(crate) nota: Option<(usize, usize)>,
}

impl Default for StatoAnnota {
    fn default() -> Self {
        Self {
            attrezzo: Attrezzo::Dito,
            colore_penna: 1,
            spessore: 1,
            colore_marca: 0,
            colore_testo: 0,
            misura_testo: MISURA,
            tratto: None,
            pizzico: false,
            in_volo: Vec::new(),
            scelta: None,
            trascina: None,
            nota: None,
        }
    }
}

impl StatoAnnota {
    /// Chiude quello che è aperto sulla pagina e torna al dito.
    pub(crate) fn chiudi(&mut self) {
        self.attrezzo = Attrezzo::Dito;
        self.tratto = None;
        self.scelta = None;
        self.trascina = None;
    }

    /// Il dito disegna sulle pagine (penna o gomma) nel modo `modo`.
    pub(crate) fn disegna(&self, modo: ModoPdf) -> bool {
        modo == ModoPdf::Annota && self.attrezzo.disegna()
    }

    /// Quanto spazio prende la striscia degli attrezzi sopra la barra dei modi.
    pub(crate) fn altezza_striscia(&self) -> f32 {
        let righe = if matches!(self.attrezzo, Attrezzo::Penna | Attrezzo::Testo) { 2.0 } else { 1.0 };
        righe * (RIGA + FRA)
    }
}

/// Le azioni della barra della selezione nel modo Annota, coi colori dell'evidenziatore.
pub(crate) fn azioni_marca(colore: usize) -> AzioniSelezione {
    AzioniSelezione {
        azioni: vec![(icone::FORMAT_INK_HIGHLIGHTER, EVIDENZIA), (icone::FORMAT_UNDERLINED, SOTTOLINEA), (icone::FORMAT_STRIKETHROUGH, BARRA)],
        colori: COLORI_MARCA.iter().map(|(c, n)| (colore32(*c), *n)).collect(),
        colore: Some(colore),
    }
}

/// Un'annotazione si sceglie col tocco in questo modo: nel modo Annota tutte, nel modo Modifica
/// le forme.
fn toccabile(modo: ModoPdf, g: GenereAnnotazione) -> bool {
    match modo {
        ModoPdf::Annota => true,
        ModoPdf::Modifica => matches!(g, GenereAnnotazione::Forma(_)),
        _ => false,
    }
}

/// Il nome di un'annotazione per l'accessibilità: il suo genere, mai il testo.
fn nome_dell_annotazione(a: &Annotazione) -> String {
    format!("{ANNOTAZIONE}: {}", a.genere.nome())
}

/// Il riquadro di quello che si vede di un'annotazione (pt): i pezzi di testo per le marche.
fn riquadro(a: &Annotazione) -> [f32; 4] {
    a.pezzi.iter().copied().reduce(|x, y| [x[0].min(y[0]), x[1].min(y[1]), x[2].max(y[2]), x[3].max(y[3])]).unwrap_or(a.rettangolo)
}

/// Un cerchio di colore da toccare, cerchiato di blu se è quello scelto.
fn colore_da_toccare(ui: &mut egui::Ui, c: Pos2, colore: Color32, nome: &str, acceso: bool, id: Id, largo: f32) -> bool {
    let r = ui.interact(Rect::from_center_size(c, vec2(largo.min(44.0), 44.0)), id, Sense::click());
    r.widget_info(|| WidgetInfo::selected(WidgetType::Button, true, acceso, nome));
    let p = ui.painter();
    if r.is_pointer_button_down_on() {
        p.circle_filled(c, RAGGIO_COLORE + 7.0, tema::PREMUTO);
    }
    if acceso {
        p.circle_stroke(c, RAGGIO_COLORE + 4.0, Stroke::new(2.5, tema::BLU));
    }
    p.circle(c, RAGGIO_COLORE, colore, Stroke::new(1.0, tema::FILO));
    r.clicked()
}

/// Una pillola bianca con l'ombra, come la barra di formato.
fn pillola(ui: &egui::Ui, r: Rect) {
    let raggio = CornerRadius::same((r.height() / 2.0) as u8);
    tema::dipingi_ombra(ui.painter(), r, raggio, &tema::OMBRA_FLOTTANTE);
    ui.painter().rect_filled(r, raggio, tema::FOGLIO);
}

/// Quello che si tocca nella seconda riga della striscia.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scelta {
    Colore(usize),
    Spessore(usize),
    Misura(Verso),
}

/// La seconda riga della striscia: i colori e, con `spessore`, i tre spessori, con `misura` la
/// misura del testo coi passi. Tutto in una pillola bianca in `r`. Gli spessori (o la misura)
/// stanno attaccati al lato destro della pillola e i colori si dividono lo spazio che resta, al
/// più 40 dp l'uno: così la riga sta anche in una pillola stretta (0.8.3: a 372 dp lo spessore
/// grosso usciva dal bordo).
pub(crate) fn riga_dei_colori(ui: &mut egui::Ui, r: Rect, colore: usize, spessore: Option<usize>, misura: Option<f32>, id: Id) -> Option<Scelta> {
    let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(r).id_salt(id));
    pillola(&ui, r);
    ui.interact(r, id.with("fondo"), Sense::click());
    let mut scelta = None;
    let sinistra = if r.width() < 360.0 { 24.0 } else { 30.0 };
    // Il primo della parte destra: il primo spessore, o il «meno» della misura. L'ultimo spessore
    // sta dal bordo destro quanto il primo colore dal sinistro (il tratto spesso è un poco più
    // largo di un colore).
    let destra = match (spessore.is_some(), misura.is_some()) {
        (true, _) => Some(r.right() - sinistra - 4.0 - 2.0 * 44.0),
        (false, true) => Some(r.right() - 38.0 - 76.0),
        (false, false) => None,
    };
    let fine_colori = destra.map_or(r.right() - sinistra, |d| d - 30.0 - 16.0);
    let passo = ((fine_colori - (r.left() + sinistra)) / (COLORI.len() - 1).max(1) as f32).min(40.0);
    let mut x = r.left() + sinistra;
    for (i, (c, nome)) in COLORI.iter().enumerate() {
        if colore_da_toccare(&mut ui, pos2(x, r.center().y), colore32(*c), nome, colore == i, id.with(("colore", i)), passo) {
            scelta = Some(Scelta::Colore(i));
        }
        x += passo;
    }
    let mut x = destra.unwrap_or(x);
    if destra.is_some() {
        ui.painter().rect_filled(Rect::from_min_size(pos2(x - 30.0, r.center().y - 14.0), vec2(1.0, 28.0)), 0.0, tema::FILO);
    }
    if let Some(s) = spessore {
        for (i, (w, nome)) in SPESSORI.iter().enumerate() {
            let c = pos2(x, r.center().y);
            let risposta = ui.interact(Rect::from_center_size(c, Vec2::splat(44.0)), id.with(("spessore", i)), Sense::click());
            risposta.widget_info(|| WidgetInfo::selected(WidgetType::Button, true, s == i, *nome));
            if s == i {
                ui.painter().circle_filled(c, 19.0, tema::BLU_TENUE);
            }
            let colore = if s == i { tema::BLU } else { tema::INCHIOSTRO };
            ui.painter().line_segment([c - vec2(11.0, 0.0), c + vec2(11.0, 0.0)], Stroke::new(w * 1.6, colore));
            if risposta.clicked() {
                scelta = Some(Scelta::Spessore(i));
            }
            x += 44.0;
        }
    }
    if let Some(m) = misura {
        let meno = Rect::from_center_size(pos2(x, r.center().y), Vec2::splat(tema::PULSANTE));
        if ui.put(meno, PulsanteIcona::new(icone::TEXT_DECREASE, PIU_PICCOLO).spento(m <= 6.0)).clicked() {
            scelta = Some(Scelta::Misura(Verso::Meno));
        }
        let testo = format!("{} pt", m.round());
        ui.painter().text(
            pos2(x + 38.0, r.center().y),
            egui::Align2::CENTER_CENTER,
            testo,
            Carattere::Grassetto.font(ui.ctx(), 14.0),
            tema::INCHIOSTRO,
        );
        let piu = Rect::from_center_size(pos2(x + 76.0, r.center().y), Vec2::splat(tema::PULSANTE));
        if ui.put(piu, PulsanteIcona::new(icone::TEXT_INCREASE, PIU_GRANDE).spento(m >= 72.0)).clicked() {
            scelta = Some(Scelta::Misura(Verso::Piu));
        }
    }
    scelta
}

/// Quello da fare dopo aver disegnato, in un frame.
type Dopo = Box<dyn FnOnce(&mut TascabileApp)>;

impl TascabileApp {
    /// Il modo Annota sopra il lettore (`pagine`, la sua zona; `sotto`, lo spazio della barra in
    /// fondo); `tocco` e `selezione` sono quello che il lettore ha visto in questo frame.
    pub(crate) fn mostra_annota(
        &mut self,
        ui: &mut egui::Ui,
        pagine: Rect,
        sotto: f32,
        tocco: Option<(usize, Pos2)>,
        selezione: Option<SceltaSelezione>,
    ) {
        if let Some(s) = selezione {
            self.marca_dalla_selezione(s);
        }
        let ctx = ui.ctx().clone();
        let zona = Rect::from_min_max(pagine.min, pos2(pagine.right(), (pagine.bottom() - sotto).max(pagine.top())));
        self.disegna_in_volo(ui, pagine);
        self.disegna_scrittura_annota(ui, pagine);
        let attrezzo = self.pdf.as_ref().map_or(Attrezzo::Dito, |p| p.annota.attrezzo);
        let mut azione: Option<Dopo> = None;
        if attrezzo.disegna() {
            azione = self.mostra_penna(ui, pagine);
        } else {
            azione = self.mostra_annotazione_scelta(ui, pagine).or(azione);
        }
        let scrive = self.pdf.as_ref().is_some_and(|p| p.compila.scrittura.is_some());
        if !scrive {
            self.mostra_striscia_annota(ui, pagine, sotto);
        }
        if let Some(a) = azione {
            a(self);
        } else if let Some((pagina, pt)) = tocco {
            self.tocco_annota_pdf(pagina, pt);
        }
        self.mostra_barra_annotazione(&ctx, zona);
    }

    /// Il testo che si scrive per una nota o un testo libero, sulla pagina: il testo libero dove
    /// andrà, col suo colore e la sua misura; la nota come un'icona gialla col testo accanto.
    fn disegna_scrittura_annota(&self, ui: &egui::Ui, pagine: Rect) {
        let Some(p) = self.pdf.as_ref() else { return };
        let Some(s) = p.compila.scrittura.as_ref() else { return };
        let (pagina, punto, nota) = match s.bersaglio {
            Bersaglio::NotaNuova { pagina, punto } => (pagina, punto, true),
            Bersaglio::TestoLibero { pagina, punto } => (pagina, punto, false),
            Bersaglio::Annotazione { pagina, indice } => {
                let Some(a) = p.motore.stato().annotazioni.iter().find(|a| a.chiave() == (pagina, indice)) else { return };
                (pagina, [a.rettangolo[0], a.rettangolo[1]], a.genere == GenereAnnotazione::Nota)
            }
            _ => return,
        };
        let Some(a) = p.lettore.sullo_schermo(pagina, [punto[0], punto[1], punto[0], punto[1]]) else { return };
        let k = p.lettore.dp_per_pt(pagina);
        let painter = ui.painter_at(pagine);
        let vuoto = s.campo.testo().is_empty();
        let testo = if vuoto { crate::pdf_compila::VUOTO_TESTO } else { s.campo.testo() };
        if nota {
            let icona = Rect::from_center_size(a.min, Vec2::splat(20.0 * k));
            painter.rect(icona, 3.0, Color32::from_rgb(0xFF, 0xD1, 0x00), Stroke::new(1.5, tema::BLU), StrokeKind::Outside);
            let g =
                painter.layout(testo.to_string(), Carattere::Testo.font(ui.ctx(), 15.0), if vuoto { tema::MATITA } else { tema::INCHIOSTRO }, 220.0);
            let r = Rect::from_min_size(icona.right_top() + vec2(8.0, 0.0), g.size()).expand2(vec2(6.0, 4.0));
            painter.rect(r, 4.0, tema::FOGLIO, Stroke::new(1.0, tema::FILO), StrokeKind::Outside);
            painter.galley(r.min + vec2(6.0, 4.0), g, tema::INCHIOSTRO);
            return;
        }
        let colore = if vuoto { tema::MATITA } else { colore32(COLORI[p.annota.colore_testo].0) };
        let g =
            painter.layout(testo.to_string(), Carattere::Testo.font(ui.ctx(), (p.annota.misura_testo * k).max(6.0)), colore, (260.0 * k).max(40.0));
        let r = Rect::from_min_size(a.min, g.size()).expand2(vec2(3.0, 2.0));
        painter.rect(r, 2.0, tema::FOGLIO, Stroke::new(1.5, tema::BLU), StrokeKind::Outside);
        painter.galley(r.min + vec2(3.0, 2.0), g, colore);
    }

    /// I tratti mandati al motore, finché la pagina rifatta non li mostra.
    fn disegna_in_volo(&mut self, ui: &egui::Ui, pagine: Rect) {
        let Some(p) = self.pdf.as_mut() else { return };
        let lettore = &p.lettore;
        p.annota.in_volo.retain(|t| !(t.risposto && !lettore.da_rifare(t.pagina)));
        let painter = ui.painter_at(pagine);
        for t in &p.annota.in_volo {
            let k = p.lettore.dp_per_pt(t.pagina);
            let punti: Vec<Pos2> =
                t.punti.iter().filter_map(|q| p.lettore.sullo_schermo(t.pagina, [q[0], q[1], q[0], q[1]]).map(|r| r.min)).collect();
            painter.add(egui::Shape::line(punti, Stroke::new((t.spessore * k).max(1.0), colore32(t.colore))));
        }
        if !p.annota.in_volo.is_empty() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(30));
        }
    }

    /// La penna e la gomma: il dito disegna sulla pagina dove si è appoggiato; due dita fermano il
    /// tratto (è un pizzico). Alla fine il tratto (o la gomma) va al motore.
    fn mostra_penna(&mut self, ui: &mut egui::Ui, pagine: Rect) -> Option<Dopo> {
        let p = self.pdf.as_mut()?;
        let risposta = ui.interact(pagine, Id::new("penna_pdf"), Sense::click_and_drag());
        risposta.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, DISEGNA_CON_IL_DITO));
        let dita = ui.input(|i| i.multi_touch().is_some()) || p.lettore.pizzica();
        if dita {
            if p.annota.tratto.take().is_some() {
                diagnosi("pdf: tratto lasciato, è un pizzico");
            }
            p.annota.pizzico = true;
        }
        if !ui.input(|i| i.pointer.any_down() || i.any_touches()) {
            p.annota.pizzico = false;
        }
        let gomma = p.annota.attrezzo == Attrezzo::Gomma;
        if risposta.drag_started()
            && !p.annota.pizzico
            && let Some(da) = ui.input(|i| i.pointer.press_origin())
            && let Some((pagina, pt)) = p.lettore.sulla_pagina(da)
        {
            p.annota.tratto = Some((pagina, vec![[pt.x, pt.y]]));
        }
        if risposta.dragged()
            && let Some(qui) = risposta.interact_pointer_pos()
            && let Some((pagina, punti)) = p.annota.tratto.as_mut()
            && let Some((q, pt)) = p.lettore.sulla_pagina(qui)
            && q == *pagina
        {
            punti.push([pt.x, pt.y]);
        }
        let k = p.annota.tratto.as_ref().map_or(1.0, |(pagina, _)| p.lettore.dp_per_pt(*pagina)).max(1e-3);
        let painter = ui.painter_at(pagine);
        if let Some((pagina, punti)) = &p.annota.tratto {
            let schermo: Vec<Pos2> = punti.iter().filter_map(|q| p.lettore.sullo_schermo(*pagina, [q[0], q[1], q[0], q[1]]).map(|r| r.min)).collect();
            if gomma {
                for c in &schermo {
                    painter.circle_filled(*c, RAGGIO_GOMMA, Color32::from_rgba_unmultiplied(0x23, 0x46, 0xA0, 24));
                }
            } else {
                let colore = colore32(COLORI[p.annota.colore_penna].0);
                painter.add(egui::Shape::line(schermo, Stroke::new((SPESSORI[p.annota.spessore].0 * k).max(1.0), colore)));
            }
        }
        if !risposta.drag_stopped() {
            return None;
        }
        let (pagina, punti) = p.annota.tratto.take()?;
        let autore = self.impostazioni.autore.clone();
        Some(Box::new(move |app: &mut TascabileApp| {
            let Some(p) = app.pdf.as_mut() else { return };
            let stato = p.motore.stato();
            let Some(info) = stato.geometrie.get(pagina).cloned() else { return };
            if gomma {
                let annotazioni = stato.annotazioni.clone();
                match Modifica::gomma(pagina, &info, &annotazioni, &punti, RAGGIO_GOMMA / k) {
                    Some(m) => {
                        diagnosi(format!("pdf: gomma nella pagina {}", pagina + 1));
                        app.modifica_pdf(m);
                    }
                    None => diagnosi("pdf: gomma su niente"),
                }
                return;
            }
            let (colore, spessore) = (COLORI[p.annota.colore_penna].0, SPESSORI[p.annota.spessore].0);
            if let Some(m) = Modifica::tratto(pagina, &info, &punti, colore, spessore, &autore) {
                diagnosi(format!(
                    "pdf: tratto della penna, {} punti, colore {}, spessore {}",
                    punti.len(),
                    p.annota.colore_penna + 1,
                    p.annota.spessore + 1
                ));
                p.annota.in_volo.push(InVolo { pagina, punti, colore, spessore, risposto: false });
                app.modifica_pdf(m);
            }
        }))
    }

    /// L'annotazione scelta: il riquadro blu e il dito che la sposta (non le marche, che seguono il
    /// testo). Vale nel modo Annota e, per le forme, nel modo Modifica.
    pub(crate) fn mostra_annotazione_scelta(&mut self, ui: &mut egui::Ui, pagine: Rect) -> Option<Dopo> {
        let p = self.pdf.as_mut()?;
        let (pagina, indice) = p.annota.scelta?;
        let Some(a) = p.motore.stato().annotazioni.iter().find(|a| a.chiave() == (pagina, indice)).cloned() else {
            p.annota.scelta = None;
            return None;
        };
        let r = p.lettore.sullo_schermo(a.pagina, riquadro(&a))?;
        let painter = ui.painter_at(pagine);
        let mobile = !matches!(a.genere, GenereAnnotazione::Marca(_));
        let mut azione: Option<Dopo> = None;
        if mobile {
            let presa = r.expand(BORDO).intersect(pagine);
            let risposta = ui.interact(presa, Id::new(("annotazione_pdf", pagina, indice)), Sense::click_and_drag());
            let nome = nome_dell_annotazione(&a);
            risposta.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, &nome));
            let origine = ui.input(|i| i.pointer.press_origin());
            let fin_qui = match (risposta.interact_pointer_pos(), origine) {
                (Some(x), Some(o)) => x - o,
                _ => Vec2::ZERO,
            };
            if risposta.drag_started() {
                p.annota.trascina = Some(Vec2::ZERO);
            }
            if risposta.dragged()
                && let Some(d) = p.annota.trascina.as_mut()
            {
                *d = fin_qui;
            }
            if risposta.drag_stopped()
                && let Some(d) = p.annota.trascina.take()
            {
                let a = a.clone();
                let k = p.lettore.dp_per_pt(a.pagina).max(1e-3);
                azione = Some(Box::new(move |app: &mut TascabileApp| {
                    let Some(info) = app.pdf.as_ref().and_then(|p| p.motore.stato().geometrie.get(a.pagina).cloned()) else { return };
                    if d.length() >= 2.0 {
                        diagnosi(format!("pdf: annotazione spostata ({})", a.genere.nome()));
                        app.modifica_pdf(Modifica::sposta_annotazione(&a, &info, d.x / k, d.y / k));
                    }
                }));
            }
            if risposta.clicked() && a.genere.con_testo() {
                let a = a.clone();
                azione = Some(Box::new(move |app: &mut TascabileApp| app.apri_testo_annotazione(&a)));
            }
        }
        let dove = r.translate(p.annota.trascina.unwrap_or(Vec2::ZERO));
        if dove != r {
            painter.rect_filled(dove, 2.0, Color32::from_rgba_unmultiplied(35, 70, 160, 30));
        }
        painter.rect_stroke(dove.expand(3.0), 3.0, Stroke::new(1.5, tema::BLU), StrokeKind::Outside);
        azione
    }

    /// Un tocco sulla pagina nel modo Annota: chiude la scrittura; con Nota o Testo li mette lì;
    /// sceglie l'annotazione sotto il dito (una nota apre il suo foglio); su un punto vuoto
    /// toglie la scelta.
    pub(crate) fn tocco_annota_pdf(&mut self, pagina: usize, pt: Pos2) {
        if self.pdf.as_ref().is_some_and(|p| p.compila.scrittura.is_some()) {
            self.conferma_scrittura_pdf(true);
            return;
        }
        let Some(p) = self.pdf.as_mut() else { return };
        match p.annota.attrezzo {
            Attrezzo::Nota => {
                p.annota.attrezzo = Attrezzo::Dito;
                diagnosi("pdf: nota nuova");
                self.apri_scrittura_pdf(Bersaglio::NotaNuova { pagina, punto: [pt.x, pt.y] }, "", false);
                return;
            }
            Attrezzo::Testo => {
                p.annota.attrezzo = Attrezzo::Dito;
                diagnosi("pdf: testo libero nuovo");
                self.apri_scrittura_pdf(Bersaglio::TestoLibero { pagina, punto: [pt.x, pt.y] }, "", false);
                return;
            }
            _ => {}
        }
        self.tocco_annotazione_pdf(pagina, pt, ModoPdf::Annota);
    }

    /// Sceglie l'annotazione sotto il punto (quelle che si toccano nel `modo`); una nota apre il
    /// suo foglio. Vero se ne ha presa una o ne ha lasciata una scelta.
    pub(crate) fn tocco_annotazione_pdf(&mut self, pagina: usize, pt: Pos2, modo: ModoPdf) -> bool {
        let Some(p) = self.pdf.as_mut() else { return false };
        let k = p.lettore.dp_per_pt(pagina).max(1e-3);
        let stato = p.motore.stato();
        let toccabili: Vec<Annotazione> = stato.annotazioni.iter().filter(|a| toccabile(modo, a.genere)).cloned().collect();
        match annotazione_sotto(&toccabili, pagina, [pt.x, pt.y], BORDO / k).cloned() {
            Some(a) if a.genere == GenereAnnotazione::Nota => {
                diagnosi("pdf: nota aperta");
                p.annota.scelta = Some(a.chiave());
                p.annota.nota = Some(a.chiave());
                self.fogli_pdf.nota.apri();
                true
            }
            Some(a) => {
                diagnosi(format!("pdf: annotazione scelta ({})", a.genere.nome()));
                p.annota.scelta = Some(a.chiave());
                true
            }
            None => p.annota.scelta.take().is_some(),
        }
    }

    /// La scrittura del testo di una nota o di un testo libero.
    fn apri_testo_annotazione(&mut self, a: &Annotazione) {
        diagnosi(format!("pdf: testo da riscrivere ({})", a.genere.nome()));
        self.fogli_pdf.nota.chiudi();
        self.apri_scrittura_pdf(Bersaglio::Annotazione { pagina: a.pagina, indice: a.indice }, &a.testo, false);
    }

    /// Evidenzia, Sottolinea o Barra sulla barra della selezione: il testo scelto si segna col
    /// colore di adesso; un colore toccato diventa quello di adesso.
    fn marca_dalla_selezione(&mut self, s: SceltaSelezione) {
        let autore = self.impostazioni.autore.clone();
        let Some(p) = self.pdf.as_mut() else { return };
        match s {
            SceltaSelezione::Colore(i) => {
                p.annota.colore_marca = i.min(COLORI_MARCA.len() - 1);
                diagnosi(format!("pdf: colore per segnare il testo {}", i + 1));
            }
            SceltaSelezione::Azione { indice, pagina, righe } => {
                let marca = [Marca::Evidenzia, Marca::Sottolinea, Marca::Barra].get(indice).copied().unwrap_or(Marca::Evidenzia);
                let Some(info) = p.motore.stato().geometrie.get(pagina).cloned() else { return };
                if let Some(m) = Modifica::marca(pagina, &info, &righe, marca, COLORI_MARCA[p.annota.colore_marca].0, &autore) {
                    diagnosi(format!(
                        "pdf: testo {} in {} righe, colore {}",
                        GenereAnnotazione::Marca(marca).nome(),
                        righe.len(),
                        p.annota.colore_marca + 1
                    ));
                    self.modifica_pdf(m);
                }
            }
        }
    }

    /// La striscia degli attrezzi sopra la barra dei modi e, per la penna e il testo, la riga dei
    /// colori.
    fn mostra_striscia_annota(&mut self, ui: &mut egui::Ui, pagine: Rect, sotto: f32) {
        let Some(p) = self.pdf.as_ref() else { return };
        let a = p.annota.attrezzo;
        let fondo = pagine.bottom() - sotto + 8.0;
        let larga = |alto: f32, y: f32| Rect::from_min_max(pos2(pagine.left() + tema::MARGINE, y - alto), pos2(pagine.right() - tema::MARGINE, y));
        let prima = larga(RIGA, fondo - FRA);
        let mut barra = BarraFlottante::new();
        for (i, (attrezzo, icona, nome)) in Attrezzo::TUTTI.iter().enumerate() {
            if i == 3 {
                barra = barra.separatore();
            }
            barra = barra.pulsante(PulsanteIcona::new(icona, nome).acceso(a == *attrezzo));
        }
        let scelto = barra.mostra(ui, prima);
        let seconda = matches!(a, Attrezzo::Penna | Attrezzo::Testo).then(|| larga(RIGA, prima.top() - FRA));
        let scelta = seconda.and_then(|r| match a {
            Attrezzo::Penna => riga_dei_colori(ui, r, p.annota.colore_penna, Some(p.annota.spessore), None, Id::new("riga_penna")),
            _ => riga_dei_colori(ui, r, p.annota.colore_testo, None, Some(p.annota.misura_testo), Id::new("riga_testo")),
        });
        let Some(p) = self.pdf.as_mut() else { return };
        if let Some(i) = scelto {
            let nuovo = Attrezzo::TUTTI[i].0;
            p.annota.attrezzo = nuovo;
            p.annota.tratto = None;
            p.annota.scelta = None;
            diagnosi(format!("pdf: annota, {}", nuovo.nome().to_lowercase()));
            match nuovo {
                Attrezzo::Nota => self.avviso_breve.avvisa("Tocca dove mettere la nota"),
                Attrezzo::Testo => self.avviso_breve.avvisa("Tocca dove scrivere il testo"),
                _ => {}
            }
        }
        let Some(p) = self.pdf.as_mut() else { return };
        match (a, scelta) {
            (Attrezzo::Penna, Some(Scelta::Colore(i))) => p.annota.colore_penna = i,
            (Attrezzo::Penna, Some(Scelta::Spessore(i))) => p.annota.spessore = i,
            (_, Some(Scelta::Colore(i))) => p.annota.colore_testo = i,
            (_, Some(Scelta::Misura(v))) => {
                let m = p.annota.misura_testo + if v == Verso::Piu { PASSO_MISURA } else { -PASSO_MISURA };
                p.annota.misura_testo = m.clamp(6.0, 72.0);
            }
            _ => {}
        }
    }

    /// La barra scura sopra l'annotazione scelta: Riscrivi (testo libero), Formato e Togli.
    pub(crate) fn mostra_barra_annotazione(&mut self, ctx: &egui::Context, zona: Rect) {
        let Some(p) = self.pdf.as_ref() else { return };
        if p.annota.trascina.is_some() || p.compila.scrittura.is_some() || p.annota.attrezzo.disegna() || self.fogli_pdf.nota.aperto() {
            return;
        }
        let Some(a) = p.annota.scelta.and_then(|c| p.motore.stato().annotazioni.iter().find(|a| a.chiave() == c).cloned()) else { return };
        let Some(r) = p.lettore.sullo_schermo(a.pagina, riquadro(&a)) else { return };
        let mut voci: Vec<(&str, &str)> = Vec::new();
        if matches!(a.genere, GenereAnnotazione::TestoLibero { .. }) {
            voci.push((icone::EDIT, RISCRIVI));
        }
        if a.genere.con_colore() {
            voci.push((icone::PALETTE, FORMATO));
        }
        voci.push((icone::DELETE, TOGLI));
        let mut barra = BarraContesto::new();
        for (icona, nome) in &voci {
            barra = barra.azione(icona, nome);
        }
        match barra.mostra_in_primo_piano(ctx, Id::new("annotazione_scelta"), r.expand(6.0), zona).and_then(|i| voci.get(i)).map(|v| v.1) {
            Some(RISCRIVI) => self.apri_testo_annotazione(&a),
            Some(FORMATO) => self.fogli_pdf.annotazione.apri(),
            Some(_) => self.togli_annotazione_pdf(&a),
            None => {}
        }
    }

    fn togli_annotazione_pdf(&mut self, a: &Annotazione) {
        diagnosi(format!("pdf: annotazione tolta ({})", a.genere.nome()));
        if let Some(p) = self.pdf.as_mut() {
            p.annota.scelta = None;
            p.annota.nota = None;
        }
        self.fogli_pdf.nota.chiudi();
        self.fogli_pdf.annotazione.chiudi();
        self.modifica_pdf(Modifica::togli_annotazione(a));
    }

    /// Il testo scritto per una nota nuova, un testo libero nuovo o un'annotazione: va al motore
    /// alla conferma della scrittura.
    pub(crate) fn conferma_testo_annota(&mut self, bersaglio: &Bersaglio, testo: &str) {
        let autore = self.impostazioni.autore.clone();
        let Some(p) = self.pdf.as_mut() else { return };
        let stato = p.motore.stato();
        let m = match bersaglio {
            Bersaglio::NotaNuova { pagina, punto } => {
                let info = stato.geometrie.get(*pagina).cloned();
                let m = info.and_then(|i| Modifica::nota(*pagina, &i, *punto, testo, [1.0, 0.82, 0.0], &autore));
                if m.is_some() {
                    diagnosi("pdf: nota posata");
                }
                m
            }
            Bersaglio::TestoLibero { pagina, punto } => {
                if let Some(c) = tascabile_pdf::carattere_che_manca(testo) {
                    self.errore = Some(Errore {
                        titolo: "Testo non aggiunto",
                        testo: format!("Il carattere «{c}» non si può scrivere in un testo libero: le lettere italiane e «€» sì, i simboli no."),
                    });
                    return;
                }
                let info = stato.geometrie.get(*pagina).cloned();
                let (misura, colore) = (f64::from(p.annota.misura_testo), COLORI[p.annota.colore_testo].0);
                let m = info.and_then(|i| Modifica::testo_libero_nuovo(*pagina, &i, *punto, testo, misura, colore, &autore));
                if m.is_some() {
                    diagnosi(format!("pdf: testo libero posato, {} pt, colore {}", p.annota.misura_testo, p.annota.colore_testo + 1));
                }
                m
            }
            Bersaglio::Annotazione { pagina, indice } => {
                let a = stato.annotazioni.iter().find(|a| a.chiave() == (*pagina, *indice)).cloned();
                // Un testo libero si disegna nel carattere standard: un simbolo diventerebbe «?»
                // per sempre, senza avviso (analisi del 27/10, modifica I6). Come per uno nuovo.
                if a.as_ref().is_some_and(|a| matches!(a.genere, tascabile_pdf::GenereAnnotazione::TestoLibero { .. }))
                    && let Some(c) = tascabile_pdf::carattere_che_manca(testo)
                {
                    diagnosi("pdf: testo libero riscritto con un carattere che non si scrive");
                    self.errore = Some(Errore {
                        titolo: "Testo non riscritto",
                        testo: format!("Il carattere «{c}» non si può scrivere in un testo libero: le lettere italiane e «€» sì, i simboli no."),
                    });
                    return;
                }
                let info = stato.geometrie.get(*pagina).cloned();
                a.zip(info).map(|(a, info)| {
                    diagnosi(format!("pdf: testo riscritto ({})", a.genere.nome()));
                    Modifica::testo_dell_annotazione(&a, &info, testo)
                })
            }
            _ => None,
        };
        if let Some(m) = m {
            self.modifica_pdf(m);
        }
    }

    /// Le risposte del motore per il modo Annota: i tratti che aspettavano la risposta, la scelta
    /// che dopo annulla e ripeti (gli indici cambiano) se ne va.
    pub(crate) fn annota_dopo_il_motore(&mut self, evento: &Evento<crate::pdf_lavoro::ScrittoPdf>) {
        let Some(p) = self.pdf.as_mut() else { return };
        match evento {
            Evento::Applicata { .. } | Evento::Rifiutata { .. } | Evento::NonApplicata { .. } => {
                if let Some(t) = p.annota.in_volo.iter_mut().find(|t| !t.risposto) {
                    t.risposto = true;
                }
            }
            Evento::Annullata { .. } | Evento::Ripetuta { .. } => {
                p.annota.scelta = None;
                p.annota.nota = None;
                p.annota.trascina = None;
            }
            _ => {}
        }
        let annotazioni = p.motore.stato().annotazioni.clone();
        if p.annota.scelta.is_some_and(|c| !annotazioni.iter().any(|a| a.chiave() == c)) {
            p.annota.scelta = None;
        }
        if p.annota.nota.is_some_and(|c| !annotazioni.iter().any(|a| a.chiave() == c)) {
            p.annota.nota = None;
            self.fogli_pdf.nota.chiudi();
        }
    }

    /// Uno strumento del modo Annota dal foglio Strumenti: Evidenzia, Penna, Nota.
    pub(crate) fn strumento_annota_pdf(&mut self, s: crate::pdf_lavoro::Strumento) {
        use crate::pdf_lavoro::Strumento;
        self.scegli_modo_pdf(ModoPdf::Annota);
        let Some(p) = self.pdf.as_mut() else { return };
        p.annota.chiudi();
        match s {
            Strumento::Evidenzia => self.avviso_breve.avvisa("Tieni premuto sul testo da evidenziare"),
            Strumento::Penna => p.annota.attrezzo = Attrezzo::Penna,
            Strumento::Nota => {
                p.annota.attrezzo = Attrezzo::Nota;
                self.avviso_breve.avvisa("Tocca dove mettere la nota");
            }
            _ => {}
        }
    }

    /// I fogli del modo Annota: la nota col suo testo, il formato dell'annotazione scelta.
    pub(crate) fn mostra_fogli_annota(&mut self, ctx: &egui::Context) {
        self.mostra_foglio_nota(ctx);
        self.mostra_foglio_formato_annotazione(ctx);
    }

    /// Il foglio di una nota: il testo, chi l'ha scritta se il PDF lo dice, Riscrivi e Togli.
    fn mostra_foglio_nota(&mut self, ctx: &egui::Context) {
        let a = self.pdf.as_ref().and_then(|p| p.annota.nota.and_then(|c| p.motore.stato().annotazioni.iter().find(|a| a.chiave() == c).cloned()));
        #[derive(Clone, Copy)]
        enum Scelta {
            Riscrivi,
            Togli,
        }
        let scelta = self
            .fogli_pdf
            .nota
            .mostra(ctx, NOTA, |ui| {
                let a = a.as_ref()?;
                let mut s = None;
                let testo = if a.testo.trim().is_empty() { "La nota non ha testo." } else { a.testo.as_str() };
                ui.label(RichText::new(testo).font(Carattere::Testo.font(ui.ctx(), 17.0)).color(tema::INCHIOSTRO));
                if !a.autore.trim().is_empty() {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(format!("Scritta da {}", a.autore.trim())).font(Carattere::Testo.font(ui.ctx(), 14.0)).color(tema::MATITA),
                    );
                }
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    if ui.add(Pulsante::new(RISCRIVI, Peso::Principale)).clicked() {
                        s = Some(Scelta::Riscrivi);
                    }
                    if ui.add(Pulsante::new(TOGLI, Peso::Testo)).clicked() {
                        s = Some(Scelta::Togli);
                    }
                });
                ui.add_space(12.0);
                s
            })
            .flatten();
        if !self.fogli_pdf.nota.aperto()
            && let Some(p) = self.pdf.as_mut()
        {
            p.annota.nota = None;
        }
        let (Some(s), Some(a)) = (scelta, a) else { return };
        match s {
            Scelta::Riscrivi => self.apri_testo_annotazione(&a),
            Scelta::Togli => self.togli_annotazione_pdf(&a),
        }
    }

    /// Il formato dell'annotazione scelta: il colore, lo spessore per disegni e forme, la misura
    /// per un testo libero. Ogni tocco va al motore.
    fn mostra_foglio_formato_annotazione(&mut self, ctx: &egui::Context) {
        let a = self.pdf.as_ref().and_then(|p| p.annota.scelta.and_then(|c| p.motore.stato().annotazioni.iter().find(|a| a.chiave() == c).cloned()));
        let marca = a.as_ref().is_some_and(|a| matches!(a.genere, GenereAnnotazione::Marca(_)));
        let tavolozza: &[([f32; 3], &str)] = if marca { &COLORI_MARCA } else { &COLORI };
        let scelta = self
            .fogli_pdf
            .annotazione
            .mostra(ctx, FORMATO, |ui| {
                let a = a.as_ref()?;
                let mut s = None;
                etichetta_sezione(ui, "Colore");
                let tinte: Vec<(Tinta, &str)> = tavolozza.iter().map(|(c, n)| (Tinta::Colore(colore32(*c)), *n)).collect();
                let acceso = a.colore.and_then(|c| tavolozza.iter().position(|(t, _)| t.iter().zip(c).all(|(x, y)| (x - y).abs() < 0.03)));
                if let Some(i) = Tavolozza::new(&tinte).acceso(acceso).mostra(ui) {
                    s = Some(Scelta::Colore(i));
                }
                if a.genere.con_spessore() {
                    ui.add_space(12.0);
                    etichetta_sezione(ui, "Spessore");
                    let ora = a.spessore.and_then(|w| SPESSORI.iter().position(|(v, _)| (v - w).abs() < 0.3));
                    ui.horizontal_wrapped(|ui| {
                        for (i, (_, nome)) in SPESSORI.iter().enumerate() {
                            if ui.add(Chip::new(nome).acceso(ora == Some(i))).clicked() {
                                s = Some(Scelta::Spessore(i));
                            }
                        }
                    });
                }
                if let GenereAnnotazione::TestoLibero { misura } = a.genere {
                    ui.add_space(12.0);
                    etichetta_sezione(ui, "Misura");
                    let valore = format!("{} pt", misura.round());
                    if let Some(v) = Passo::new(&valore, "Più piccolo", "Più grande").meno_spento(misura <= 6.0).mostra(ui) {
                        s = Some(Scelta::Misura(v));
                    }
                }
                ui.add_space(12.0);
                s
            })
            .flatten();
        let (Some(s), Some(a)) = (scelta, a) else { return };
        let m = match s {
            Scelta::Colore(i) => {
                diagnosi(format!("pdf: annotazione col colore {} ({})", i + 1, a.genere.nome()));
                Modifica::colore_dell_annotazione(&a, tavolozza[i].0)
            }
            Scelta::Spessore(i) => {
                diagnosi(format!("pdf: annotazione con lo spessore {} ({})", i + 1, a.genere.nome()));
                Modifica::spessore_dell_annotazione(&a, SPESSORI[i].0)
            }
            Scelta::Misura(v) => {
                let GenereAnnotazione::TestoLibero { misura } = a.genere else { return };
                let Some(info) = self.pdf.as_ref().and_then(|p| p.motore.stato().geometrie.get(a.pagina).cloned()) else { return };
                let nuova = (misura + if v == Verso::Piu { PASSO_MISURA } else { -PASSO_MISURA }).clamp(6.0, 72.0);
                diagnosi(format!("pdf: testo libero a {nuova} pt"));
                Modifica::misura_dell_annotazione(&a, &info, f64::from(nuova))
            }
        };
        self.modifica_pdf(m);
    }

    /// Indietro nel modo Annota: i fogli, la penna o la gomma (si torna al dito), l'annotazione
    /// scelta, la nota o il testo che aspettano il tocco.
    pub(crate) fn indietro_annota_pdf(&mut self) -> bool {
        if self.fogli_pdf.nota.indietro() || self.fogli_pdf.annotazione.indietro() {
            diagnosi("indietro: foglio dell'annotazione chiuso");
            return true;
        }
        let Some(p) = self.pdf.as_mut() else { return false };
        if p.annota.scelta.take().is_some() {
            p.annota.trascina = None;
            diagnosi("indietro: annotazione lasciata");
            return true;
        }
        if p.modo == ModoPdf::Annota && p.annota.attrezzo != Attrezzo::Dito {
            p.annota.attrezzo = Attrezzo::Dito;
            p.annota.tratto = None;
            diagnosi("indietro: annota, si torna a scorrere");
            return true;
        }
        false
    }

    /// Per le prove: le annotazioni del PDF aperto, col genere, il testo e il posto sullo schermo
    /// (aspettando il motore).
    #[doc(hidden)]
    pub fn annotazioni_pdf_di_prova(&mut self) -> Vec<(GenereAnnotazione, String, Rect)> {
        self.aspetta_il_motore_pdf();
        let Some(p) = self.pdf.as_ref() else { return Vec::new() };
        p.motore
            .stato()
            .annotazioni
            .iter()
            .filter_map(|a| Some((a.genere, a.testo.clone(), p.lettore.sullo_schermo(a.pagina, riquadro(a))?)))
            .collect()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn attrezzi_colori_e_azioni() {
        assert!(Attrezzo::Penna.disegna() && Attrezzo::Gomma.disegna() && !Attrezzo::Dito.disegna() && !Attrezzo::Nota.disegna());
        let mut s = StatoAnnota::default();
        assert_eq!((s.colore_penna, s.spessore, s.misura_testo), (1, 1, MISURA));
        assert!(!s.disegna(ModoPdf::Annota));
        s.attrezzo = Attrezzo::Penna;
        assert!(s.disegna(ModoPdf::Annota) && !s.disegna(ModoPdf::Leggi));
        assert_eq!(s.altezza_striscia(), 2.0 * (RIGA + FRA));
        s.chiudi();
        assert_eq!((s.attrezzo, s.altezza_striscia()), (Attrezzo::Dito, RIGA + FRA));
        let a = azioni_marca(2);
        assert_eq!(a.azioni.iter().map(|x| x.1).collect::<Vec<_>>(), [EVIDENZIA, SOTTOLINEA, BARRA]);
        assert_eq!((a.colori.len(), a.colore), (5, Some(2)));
        // Nel modo Modifica si toccano solo le forme.
        assert!(toccabile(ModoPdf::Annota, GenereAnnotazione::Penna) && !toccabile(ModoPdf::Modifica, GenereAnnotazione::Penna));
        assert!(toccabile(ModoPdf::Modifica, GenereAnnotazione::Forma(tascabile_pdf::Forma::Freccia)));
        assert!(!toccabile(ModoPdf::Leggi, GenereAnnotazione::Nota));
        let nomi: std::collections::HashSet<&str> = COLORI.iter().chain(COLORI_MARCA.iter()).map(|c| c.1).collect();
        assert_eq!(nomi.len(), 10);
    }
}
