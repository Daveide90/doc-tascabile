//! Modificare un PDF (piano «PDF completo», P7): il modo Modifica del bozzetto 7 e le voci del
//! foglio Strumenti del bozzetto 8 (Modifica testo, Aggiungi testo, Immagine, Oscura, Filigrana,
//! Numeri di pagina).
//!
//! Gli oggetti di una pagina (paragrafi, immagini, testi e immagini aggiunti) li dà il motore sul
//! suo thread, pagina per pagina, e si rileggono quando una modifica tocca la pagina. Sulla pagina
//! hanno un riquadro tratteggiato leggero. Un tocco su un paragrafo apre la scrittura con la
//! tastiera vera (la barra della scrittura del modo Compila): il testo va al motore alla
//! conferma; se il carattere del PDF non ha le lettere nuove, il motore lo dice e un avviso
//! propone di continuare o di lasciare com'era. Un tocco su un'immagine o un testo aggiunto lo
//! sceglie: riquadro blu e maniglia come i segni di Fill & Sign, si sposta col dito, si ingrandisce
//! con la maniglia (un paragrafo scelto si allarga), e la sua barra ha le azioni (Riscrivi,
//! Formato, Sostituisci, Togli). Un tocco su un punto vuoto propone Aggiungi testo e Immagine.
//!
//! Oscura: rettangoli disegnati col dito, poi «Applica», con l'avviso che quello che c'è sotto si
//! toglie davvero e non si recupera; il salvataggio dopo riscrive tutto il file.
//!
//! Nella diagnosi solo generi e conti: mai un testo, un'immagine, un nome di file.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError};

use eframe::egui::{self, Color32, Id, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2, WidgetInfo, WidgetType, pos2, vec2};
use tascabile_pdf::{Collegamento, Destinazione, ErroreFoto, Evento, Forma, GenereOggetto, Modifica, Oggetto, Porta, Toccate, carattere_che_manca};
use tascabile_platform::{EsitoFoto, FonteFoto, diagnosi};
use tascabile_touch::tema::{self, Carattere};
use tascabile_touch::{
    BarraContesto, BarraFlottante, Chip, Dialogo, Passo, Peso, Pulsante, PulsanteIcona, RispostaDialogo, Tavolozza, Tinta, Verso, etichetta_sezione,
    icone,
};

use crate::pdf_compila::{Bersaglio, RISCRIVI, TOGLI};
use crate::pdf_lavoro::ModoPdf;
use crate::{Errore, TascabileApp};

pub(crate) const AGGIUNGI_TESTO: &str = "Aggiungi testo";
pub(crate) const IMMAGINE: &str = "Immagine";
pub(crate) const FORMATO: &str = "Formato";
pub(crate) const SOSTITUISCI: &str = "Sostituisci";
pub(crate) const DALLA_GALLERIA: &str = "Dalla galleria";
pub(crate) const DALLA_FOTOCAMERA: &str = "Dalla fotocamera";
pub(crate) const APPLICA: &str = "Applica";
pub(crate) const ANNULLA_OSCURA: &str = "Lascia stare";
pub(crate) const OSCURA: &str = "Oscura";
pub(crate) const OSCURARE: &str = "Oscurare per sempre?";
pub(crate) const OSCURARE_TESTO: &str = "Quello che sta sotto i rettangoli neri si toglie davvero dal file e non si recupera, nemmeno dalle versioni di prima: al salvataggio il PDF si riscrive tutto. Con Annulla, finché non salvi, torna com'era.";
pub(crate) const DISEGNA: &str = "Disegna col dito i rettangoli da oscurare";
pub(crate) const CAMBIA_CARATTERE: &str = "La riga cambierà carattere";
pub(crate) const CAMBIA_CARATTERE_TESTO: &str =
    "Il carattere del PDF non ha tutte le lettere nuove: la riga si scriverà in un carattere standard simile, e la differenza si può vedere.";
pub(crate) const CONTINUA: &str = "Continua";
pub(crate) const LASCIA_COM_ERA: &str = "Lascia com'era";
pub(crate) const FILIGRANA: &str = "Filigrana";
pub(crate) const METTI_FILIGRANA: &str = "Metti la filigrana";
pub(crate) const TOGLI_FILIGRANA: &str = "Togli la filigrana";
pub(crate) const NUMERI: &str = "Numeri di pagina";
pub(crate) const METTI_NUMERI: &str = "Metti i numeri";
pub(crate) const TOGLI_NUMERI: &str = "Togli numeri e intestazione";
pub(crate) const SCRIVI_ALTRO: &str = "Scrivi un altro testo";
pub(crate) const SCRIVI_INTESTAZIONE: &str = "Scrivi l'intestazione";
pub(crate) const TESTO_DEL_PDF: &str = "Testo del PDF";
/// Il testo che si scrive sulla pagina, per l'accessibilità e le prove.
pub(crate) const SCRITTURA_SULLA_PAGINA: &str = "Testo che scrivi";
pub(crate) const TESTO_AGGIUNTO: &str = "Testo aggiunto";
pub(crate) const IMMAGINE_DEL_PDF: &str = "Immagine del PDF";
pub(crate) const IMMAGINE_AGGIUNTA: &str = "Immagine aggiunta";
/// La correzione del testo di una scansione (bug del 1/11): il testo riconosciuto, invisibile
/// sopra la foto, si riscrive visibile e la scritta di prima si toglie anche dalla foto.
pub(crate) const TESTO_DELLA_SCANSIONE: &str = "Testo della scansione";
/// Le forme e i link (P5, i dubbi di P7).
pub(crate) const RETTANGOLO: &str = "Rettangolo";
pub(crate) const ELLISSE: &str = "Ellisse";
pub(crate) const LINEA: &str = "Linea";
pub(crate) const FRECCIA: &str = "Freccia";
pub(crate) const FINE_FORME: &str = "Fine delle forme";
pub(crate) const DISEGNA_FORMA: &str = "Disegna la forma col dito";
pub(crate) const DISEGNA_LINK: &str = "Disegna col dito il riquadro del link";
pub(crate) const LINK: &str = "Link";
pub(crate) const UNA_PAGINA: &str = "Una pagina";
pub(crate) const UN_INDIRIZZO: &str = "Un indirizzo";
pub(crate) const SCRIVI_INDIRIZZO: &str = "Scrivi l'indirizzo";
pub(crate) const METTI_LINK: &str = "Metti il link";
pub(crate) const CAMBIA_LINK: &str = "Cambia il link";
pub(crate) const TOGLI_LINK: &str = "Togli il link";
pub(crate) const CAMBIA: &str = "Cambia";
const FORME: [(Forma, &str, &str); 4] = [
    (Forma::Rettangolo, icone::RECTANGLE, RETTANGOLO),
    (Forma::Ellisse, icone::CIRCLE, ELLISSE),
    (Forma::Linea, icone::HORIZONTAL_RULE, LINEA),
    (Forma::Freccia, icone::ARROW_RIGHT_ALT, FRECCIA),
];

/// I colori dei testi nuovi.
const COLORI: [([f32; 3], &str); 5] = [
    ([0.0, 0.0, 0.0], "Nero"),
    ([0.14, 0.27, 0.63], "Blu"),
    ([0.75, 0.11, 0.11], "Rosso"),
    ([0.09, 0.45, 0.20], "Verde"),
    ([0.45, 0.45, 0.45], "Grigio"),
];
/// I testi pronti della filigrana e le opacità.
const FILIGRANE: [&str; 4] = ["BOZZA", "COPIA", "RISERVATO", "ANNULLATO"];
const OPACITA: [(f64, &str); 3] = [(0.15, "Leggera"), (0.35, "Media"), (0.6, "Forte")];
/// I piè di pagina coi numeri: come li vuole il motore e come si leggono.
const PIE: [(&str, &str); 3] = [("Pagina <<1>> di <<n>>", "Pagina 1 di 5"), ("<<1>> di <<n>>", "1 di 5"), ("<<1>>", "1")];
/// La misura dei testi nuovi, in pt, e i suoi passi.
const MISURA: f32 = 14.0;
const PASSO_MISURA: f32 = 2.0;
/// La presa sul bordo destro del paragrafo che si scrive, per allargarlo.
pub(crate) const ALLARGA_SCRITTURA: &str = "Allarga il riquadro";
/// La maniglia dell'oggetto scelto, come quella dei segni di Fill & Sign.
const MANIGLIA: f32 = 7.0;
const FUORI_DALL_ANGOLO: f32 = 12.0;
const PRESA_MANIGLIA: f32 = 28.0;
/// Il tocco prende un oggetto anche un poco fuori dal suo riquadro: tanti dp per lato.
const BORDO: f32 = 6.0;
/// Il tratteggio leggero degli oggetti e il fondo dell'oggetto che si sposta.
const TRATTO: Color32 = Color32::from_rgba_unmultiplied_const(0x23, 0x46, 0xA0, 70);
const FONDO_SPOSTA: Color32 = Color32::from_rgba_unmultiplied_const(35, 70, 160, 30);
/// I segni dell'oscuramento prima di applicarlo: il nero e il bordo rosso del motore.
const NERO: Color32 = Color32::from_rgba_unmultiplied_const(0, 0, 0, 200);
const ROSSO: Color32 = Color32::from_rgb(0xE3, 0x21, 0x21);
const MASSIMO_FOTO: u64 = 64 << 20;

/// L'immagine che si riduce sul suo thread: i byte e la proporzione.
type ImmagineInArrivo = Receiver<Result<(Vec<u8>, f32), ErroreFoto>>;
/// Quello da fare dopo aver disegnato gli oggetti, in un frame.
type Dopo = Box<dyn FnOnce(&mut TascabileApp)>;

/// Chi è un oggetto in una pagina (`Oggetto::chiave`).
pub(crate) type Chiave = (usize, u8, usize);

/// Per che cosa è la foto chiesta ad Android.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum PerFoto {
    /// Un'immagine nuova centrata in questo punto (pt) della pagina.
    Nuova(usize, Pos2),
    /// Al posto dell'immagine scelta.
    Sostituisci(Chiave),
}

/// Uno strumento del foglio che aspetta il tocco sulla pagina.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Attesa {
    Testo,
    Immagine,
}

/// L'oggetto scelto che si trascina col dito: di quanto, in dp.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Trascina {
    Sposta(Vec2),
    Misura(Vec2),
}

/// Oscura: la pagina dei rettangoli (pt), quello che si disegna (da, a, sullo schermo) e se si
/// chiede la conferma.
#[derive(Debug, Default)]
pub(crate) struct Oscura {
    pub(crate) pagina: Option<usize>,
    pub(crate) rettangoli: Vec<[f32; 4]>,
    disegna: Option<(Pos2, Pos2)>,
    pub(crate) conferma: bool,
}

/// Le forme col dito: quella scelta, il colore e lo spessore, quella che si disegna (da, a,
/// sullo schermo).
#[derive(Debug)]
pub(crate) struct Forme {
    pub(crate) forma: Forma,
    pub(crate) colore: usize,
    pub(crate) spessore: usize,
    disegna: Option<(Pos2, Pos2)>,
}

impl Default for Forme {
    fn default() -> Self {
        Self { forma: Forma::Rettangolo, colore: 2, spessore: 1, disegna: None }
    }
}

/// Un link: il riquadro che si disegna, quello disegnato (pagina, pt) o il link da cambiare, e
/// dove porta.
#[derive(Debug, Default)]
pub(crate) struct NuovoLink {
    disegna: Option<(Pos2, Pos2)>,
    pub(crate) riquadro: Option<(usize, [f32; 4])>,
    pub(crate) cambia: Option<(usize, usize)>,
    pub(crate) a_indirizzo: bool,
    pub(crate) pagina: usize,
    pub(crate) indirizzo: String,
}

/// Lo stato del modo Modifica di un PDF.
#[derive(Default)]
pub(crate) struct StatoModifica {
    /// Gli oggetti delle pagine come li ha dati il motore, e le pagine chieste.
    pub(crate) oggetti: HashMap<usize, Arc<Vec<Oggetto>>>,
    chieste: HashSet<usize>,
    /// Un tocco che aspetta gli oggetti della sua pagina.
    tocco: Option<(usize, Pos2)>,
    pub(crate) scelto: Option<Chiave>,
    trascina: Option<Trascina>,
    /// La proposta su un punto vuoto: la pagina e il punto (pt).
    pub(crate) proposta: Option<(usize, Pos2)>,
    pub(crate) attesa: Option<Attesa>,
    /// Un oggetto aggiunto appena chiesto su questa pagina: quando arriva, è lui lo scelto.
    da_scegliere: Option<usize>,
    pub(crate) oscura: Option<Oscura>,
    /// La foto chiesta ad Android (per dove, e il numero della richiesta), e quella che si riduce
    /// sul suo thread.
    pub(crate) foto: Option<(PerFoto, i32)>,
    per_foto: Option<PerFoto>,
    lavoro: Option<(PerFoto, ImmagineInArrivo)>,
    /// La riscrittura che cambierebbe carattere, in attesa della risposta all'avviso.
    pub(crate) carattere: Option<Modifica>,
    /// La correzione di una scansione mandata al motore: il paragrafo e il testo scritto. Se non
    /// riesce (il testo non sta nella riga), la scrittura si riapre con quel testo, che prima si
    /// perdeva (revisione del 3/11).
    pub(crate) correzione: Option<(Chiave, String)>,
    /// Quanto è allargato il paragrafo che si scrive (1 com'era), con la presa sul bordo destro
    /// del riquadro: va al motore con il testo, a «Fatto».
    pub(crate) allargata: Option<f32>,
    /// La misura (pt) e il colore dei testi nuovi.
    misura: Option<f32>,
    colore: usize,
    /// La filigrana: un testo scritto e la scelta (dei pronti, o lo scritto con `FILIGRANE.len()`),
    /// l'opacità.
    filigrana_scritta: String,
    filigrana: usize,
    opacita: usize,
    /// I numeri di pagina: il piè scelto e l'intestazione scritta.
    pie: usize,
    pub(crate) intestazione: String,
    /// Le forme col dito, il link nuovo o da cambiare, il link scelto (pagina, posizione).
    pub(crate) forme: Option<Forme>,
    pub(crate) link: Option<NuovoLink>,
    pub(crate) link_scelto: Option<(usize, usize)>,
}

impl StatoModifica {
    /// Il dito disegna (forme, il riquadro di un link, Oscura) e non scorre le pagine.
    pub(crate) fn disegna(&self, modo: ModoPdf) -> bool {
        modo == ModoPdf::Modifica
            && (self.forme.is_some() || self.link.as_ref().is_some_and(|l| l.riquadro.is_none() && l.cambia.is_none()) || self.oscura.is_some())
    }

    /// Lo spazio della striscia delle forme sopra la barra dei modi.
    pub(crate) fn altezza_striscia(&self) -> f32 {
        if self.forme.is_some() { 2.0 * (crate::pdf_annota::RIGA + 8.0) } else { 0.0 }
    }

    pub(crate) fn oggetto(&self, chiave: Chiave) -> Option<Oggetto> {
        self.oggetti.get(&chiave.0)?.iter().find(|o| o.chiave() == chiave).cloned()
    }

    fn misura(&self) -> f32 {
        self.misura.unwrap_or(MISURA)
    }

    /// Chiude quello che è aperto sulla pagina (non la scrittura).
    pub(crate) fn chiudi(&mut self) {
        self.scelto = None;
        self.trascina = None;
        self.proposta = None;
        self.attesa = None;
        self.oscura = None;
        self.tocco = None;
        self.forme = None;
        self.link = None;
        self.link_scelto = None;
    }
}

/// L'oggetto sotto il punto `pt` (pt), entro `bordo` pt: prima le aggiunte, che stanno sopra, poi
/// il più piccolo.
fn oggetto_sotto(oggetti: &[Oggetto], pt: Pos2, bordo: f32) -> Option<&Oggetto> {
    let distanza = |r: &[f32; 4]| (r[0] - pt.x).max(pt.x - r[2]).max(0.0).max((r[1] - pt.y).max(pt.y - r[3]).max(0.0));
    let aggiunto = |o: &Oggetto| matches!(o.genere, GenereOggetto::TestoAggiunto { .. } | GenereOggetto::ImmagineAggiunta);
    oggetti.iter().filter(|o| distanza(&o.rettangolo) <= bordo).min_by(|a, b| {
        let area = |o: &Oggetto| (o.rettangolo[2] - o.rettangolo[0]) * (o.rettangolo[3] - o.rettangolo[1]);
        (!aggiunto(a), distanza(&a.rettangolo), area(a))
            .partial_cmp(&(!aggiunto(b), distanza(&b.rettangolo), area(b)))
            .unwrap_or(std::cmp::Ordering::Equal)
    })
}

/// Il nome di un oggetto per l'accessibilità: il suo genere, mai il testo.
fn nome_dell_oggetto(o: &Oggetto) -> &'static str {
    match o.genere {
        GenereOggetto::Testo => TESTO_DEL_PDF,
        GenereOggetto::Immagine { .. } => IMMAGINE_DEL_PDF,
        GenereOggetto::TestoAggiunto { .. } => TESTO_AGGIUNTO,
        GenereOggetto::ImmagineAggiunta => IMMAGINE_AGGIUNTA,
        GenereOggetto::Scansione => TESTO_DELLA_SCANSIONE,
    }
}

/// Il riquadro dell'oggetto scelto con la maniglia mossa di `delta`: con le proporzioni per le
/// immagini e i testi aggiunti, soltanto più largo o più stretto per un paragrafo.
fn ingrandito(r: Rect, delta: Vec2, proporzioni: bool) -> Rect {
    if proporzioni {
        let k = ((r.width() + delta.x) / r.width().max(1.0)).max((r.height() + delta.y) / r.height().max(1.0)).clamp(0.1, 10.0);
        Rect::from_min_size(r.min, r.size() * k)
    } else {
        Rect::from_min_size(r.min, vec2((r.width() + delta.x).max(24.0), r.height()))
    }
}

fn tratteggia(painter: &egui::Painter, r: Rect) {
    let tratto = Stroke::new(1.0, TRATTO);
    for (a, b) in
        [(r.left_top(), r.right_top()), (r.right_top(), r.right_bottom()), (r.right_bottom(), r.left_bottom()), (r.left_bottom(), r.left_top())]
    {
        painter.extend(egui::Shape::dashed_line(&[a, b], tratto, 4.0, 3.0));
    }
}

/// Un rettangolo coi lati in ordine da due punti.
fn da_due_punti(a: Pos2, b: Pos2) -> Rect {
    Rect::from_two_pos(a, b)
}

/// Il link `indice` della pagina `pagina`, come è adesso.
fn link_di(collegamenti: &[Collegamento], pagina: usize, indice: usize) -> Option<Collegamento> {
    collegamenti.iter().find(|c| c.pagina == pagina && c.indice == indice).cloned()
}

impl TascabileApp {
    /// Le pagine vicine a quella che si legge che non hanno ancora gli oggetti: si chiedono al
    /// motore.
    fn chiedi_oggetti_pdf(&mut self) {
        let Some(p) = self.pdf.as_mut() else { return };
        let corrente = p.lettore.pagina_corrente();
        let pagine = p.lettore.pagine();
        for pagina in corrente.saturating_sub(1)..(corrente + 2).min(pagine) {
            if !p.modifica.oggetti.contains_key(&pagina) && !p.modifica.chieste.contains(&pagina) && p.motore.oggetti(pagina) {
                p.modifica.chieste.insert(pagina);
            }
        }
    }

    /// Le pagine e gli oggetti del modo Modifica sopra il lettore (`pagine`, la sua zona); `tocco`
    /// è il tocco sulla pagina del lettore in questo frame.
    pub(crate) fn mostra_modifica(&mut self, ui: &mut egui::Ui, pagine: Rect, sotto: f32, tocco: Option<(usize, Pos2)>) {
        self.chiedi_oggetti_pdf();
        let ctx = ui.ctx().clone();
        let zona = Rect::from_min_max(pagine.min, pos2(pagine.right(), (pagine.bottom() - sotto).max(pagine.top())));
        if self.pdf.as_ref().is_some_and(|p| p.modifica.oscura.is_some()) {
            self.mostra_oscura(ui, pagine, zona);
            return;
        }
        if self.pdf.as_ref().is_some_and(|p| p.modifica.forme.is_some()) {
            self.disegna_i_link(ui, pagine);
            self.mostra_forme(ui, pagine, sotto);
            return;
        }
        if self.pdf.as_ref().is_some_and(|p| p.modifica.link.as_ref().is_some_and(|l| l.riquadro.is_none() && l.cambia.is_none())) {
            self.mostra_disegno_link(ui, pagine, zona);
            return;
        }
        let Some(p) = self.pdf.as_mut() else { return };
        let painter = ui.painter_at(pagine);
        let scrive = match p.compila.scrittura.as_ref().map(|s| &s.bersaglio) {
            Some(Bersaglio::Oggetto(c)) => Some(*c),
            _ => None,
        };
        let mut azione: Option<Dopo> = None;
        let visibili: Vec<Oggetto> = p.modifica.oggetti.values().flat_map(|v| v.iter().cloned()).collect();
        for o in &visibili {
            let Some(r) = p.lettore.sullo_schermo(o.pagina, o.rettangolo) else { continue };
            if !r.intersects(pagine) || p.modifica.scelto == Some(o.chiave()) || scrive == Some(o.chiave()) {
                continue;
            }
            tratteggia(&painter, r.expand(2.0));
        }
        // L'oggetto scelto: riquadro blu, si sposta col dito, la maniglia nell'angolo.
        if let Some(o) = p.modifica.scelto.and_then(|c| p.modifica.oggetto(c)).filter(|o| scrive != Some(o.chiave()))
            && let Some(r) = p.lettore.sullo_schermo(o.pagina, o.rettangolo)
        {
            let presa = r.expand(BORDO).intersect(pagine);
            let risposta = ui.interact(presa, Id::new(("oggetto_pdf", o.chiave())), Sense::click_and_drag());
            risposta.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, nome_dell_oggetto(&o)));
            let origine = ui.input(|i| i.pointer.press_origin());
            let fin_qui = |r: &egui::Response| match (r.interact_pointer_pos(), origine) {
                (Some(a), Some(o)) => a - o,
                _ => Vec2::ZERO,
            };
            // Il testo di una scansione si corregge soltanto: niente trascinamento né maniglia.
            let si_muove = o.genere.si_muove();
            if si_muove && risposta.drag_started() {
                p.modifica.trascina = Some(Trascina::Sposta(Vec2::ZERO));
            }
            if risposta.dragged()
                && let Some(Trascina::Sposta(d)) = p.modifica.trascina.as_mut()
            {
                *d = fin_qui(&risposta);
            }
            if risposta.drag_stopped()
                && let Some(Trascina::Sposta(d)) = p.modifica.trascina.take()
            {
                let o = o.clone();
                azione = Some(Box::new(move |app| app.sposta_oggetto_pdf(&o, d)));
            }
            if risposta.clicked() && o.genere.testo() {
                let o = o.clone();
                azione = Some(Box::new(move |app| app.riscrivi_oggetto_pdf(&o)));
            }
            let dove = match p.modifica.trascina {
                Some(Trascina::Sposta(d)) => r.translate(d),
                Some(Trascina::Misura(d)) => ingrandito(r, d, o.con_proporzioni()),
                None => r,
            };
            if dove != r {
                painter.rect_filled(dove, 2.0, FONDO_SPOSTA);
            }
            painter.rect_stroke(dove, 2.0, Stroke::new(1.5, tema::BLU), StrokeKind::Outside);
            if si_muove {
                let centro = dove.right_bottom() + Vec2::splat(FUORI_DALL_ANGOLO);
                painter.line_segment([dove.right_bottom(), centro], Stroke::new(1.5, tema::BLU));
                painter.circle(centro, MANIGLIA, tema::BLU, Stroke::new(2.0, tema::FOGLIO));
                let m =
                    ui.interact(Rect::from_center_size(centro, Vec2::splat(PRESA_MANIGLIA)), Id::new(("maniglia_oggetto", o.chiave())), Sense::drag());
                m.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, if o.con_proporzioni() { "Ingrandisci" } else { "Allarga" }));
                if m.drag_started() {
                    p.modifica.trascina = Some(Trascina::Misura(Vec2::ZERO));
                }
                if m.dragged()
                    && let Some(Trascina::Misura(d)) = p.modifica.trascina.as_mut()
                {
                    *d = fin_qui(&m);
                }
                if m.drag_stopped()
                    && let Some(Trascina::Misura(d)) = p.modifica.trascina.take()
                {
                    let nuovo = ingrandito(r, d, o.con_proporzioni());
                    let o = o.clone();
                    azione = Some(Box::new(move |app| app.misura_oggetto_pdf(&o, r, nuovo)));
                }
            }
        }
        // Il testo che si scrive, sulla pagina dove sta (segnalazione dell'autore del 3/11: niente
        // casella a parte, e il cursore va dove si tocca o si trascina il dito).
        if let Some(posto) = posto_sulla_pagina(p, &ctx, pagine)
            && let Some(s) = p.compila.scrittura.as_ref()
        {
            let testo = s.campo.testo();
            let font = Carattere::Testo.font(&ctx, posto.misura);
            let (mostrato, colore) = if testo.is_empty() { (crate::pdf_compila::VUOTO_TESTO, tema::MATITA) } else { (testo, posto.colore) };
            let g = painter.layout(mostrato.to_string(), font, colore, posto.largo);
            // Un paragrafo allargabile è largo quanto la sua larghezza di adesso: si vede fin dove
            // arriva il testo prima di andare a capo.
            let largo_del_riquadro = if posto.allargabile.is_some() { posto.largo } else { posto.riquadro.width() };
            let dentro = Rect::from_min_size(posto.riquadro.min, vec2(largo_del_riquadro, posto.riquadro.height()).max(g.size())).expand2(vec2(4.0, 3.0));
            painter.rect(dentro, 2.0, tema::FOGLIO, Stroke::new(1.5, tema::BLU), StrokeKind::Outside);
            let origine = dentro.min + vec2(4.0, 3.0);
            painter.galley(origine, g.clone(), colore);
            let cursore = if testo.is_empty() { 0 } else { testo.get(..s.campo.selezione().1).map_or(0, |t| t.chars().count()) };
            let c = g.pos_from_cursor(egui::text::CCursor::new(cursore)).translate(origine.to_vec2());
            painter.rect_filled(Rect::from_min_max(pos2(c.left() - 1.0, c.top()), pos2(c.left() + 1.0, c.bottom())), 0.0, tema::BLU);
            let risposta = ui.interact(dentro, Id::new("scrittura_sulla_pagina"), Sense::click_and_drag());
            risposta.widget_info(|| WidgetInfo::labeled(WidgetType::TextEdit, true, SCRITTURA_SULLA_PAGINA));
            if (risposta.clicked() || risposta.drag_started() || risposta.dragged())
                && let Some(dove) = risposta.interact_pointer_pos()
            {
                let lettera = if testo.is_empty() { 0 } else { g.cursor_from_pos(dove - origine).index.0 };
                let byte = testo.char_indices().nth(lettera).map_or(testo.len(), |(b, _)| b);
                azione = Some(Box::new(move |app| app.metti_cursore_pdf(byte)));
            }
            // La presa per allargare il paragrafo, sul bordo destro del riquadro: trascinata, il
            // riquadro si allarga o si stringe, fino al bordo della pagina (richiesta dell'autore, 3/11).
            let mut dopo_la_presa = 0.0;
            if let Some((base, massimo)) = posto.allargabile {
                let centro = pos2(dentro.right() + 8.0, dentro.center().y);
                painter.rect(Rect::from_center_size(centro, vec2(7.0, 26.0)), 3.5, tema::BLU, Stroke::new(1.5, tema::FOGLIO), StrokeKind::Outside);
                let presa = ui.interact(Rect::from_center_size(centro, vec2(PRESA_MANIGLIA, PRESA_MANIGLIA.max(dentro.height()))), Id::new("allarga_scrittura"), Sense::drag());
                presa.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, ALLARGA_SCRITTURA));
                if presa.dragged() {
                    let k = posto.largo / base.max(1.0) + presa.drag_delta().x / base.max(1.0);
                    p.modifica.allargata = Some(k.clamp(0.3, massimo));
                }
                dopo_la_presa = PRESA_MANIGLIA / 2.0 + 8.0;
            }
            // «Fatto» accanto al testo, a destra (dopo la presa); se lì non c'è posto, sopra
            // l'angolo, o sotto se sopra c'è la barra. Sempre dentro le pagine che si vedono: con
            // la pagina ingrandita finiva fuori dallo schermo (revisione del 3/11).
            let lato = tema::PULSANTE;
            let angolo = if dentro.right() + dopo_la_presa + lato + 6.0 <= pagine.right() {
                pos2(dentro.right() + dopo_la_presa + 6.0, dentro.center().y - lato / 2.0)
            } else if dentro.top() - lato - 6.0 >= pagine.top() {
                pos2(dentro.right() - lato, dentro.top() - lato - 6.0)
            } else {
                pos2(dentro.right() - lato, dentro.bottom() + 6.0)
            };
            let x = angolo.x.min(pagine.right() - lato - 4.0).max(pagine.left() + 4.0);
            let y = angolo.y.min(pagine.bottom() - lato - 4.0).max(pagine.top() + 4.0);
            let quadro = Rect::from_min_size(pos2(x, y), Vec2::splat(lato));
            painter.circle(quadro.center(), lato / 2.0, tema::FOGLIO, Stroke::new(1.0, TRATTO));
            if ui.put(quadro, PulsanteIcona::new(icone::CHECK, crate::pdf_compila::FATTO).colore(tema::BLU)).clicked() {
                azione = Some(Box::new(|app| app.conferma_scrittura_pdf(true)));
            }
        }
        if let Some((pagina, pt)) = p.modifica.proposta
            && let Some(r) = p.lettore.sullo_schermo(pagina, [pt.x, pt.y, pt.x, pt.y])
        {
            painter.circle(r.center(), 9.0, FONDO_SPOSTA, Stroke::new(1.5, tema::BLU));
            painter.circle_filled(r.center(), 3.5, tema::BLU);
        }
        self.disegna_i_link(ui, pagine);
        let azione = match azione {
            Some(a) => Some(a),
            None => self.mostra_annotazione_scelta(ui, pagine),
        };
        if let Some(a) = azione {
            a(self);
        } else if let Some((pagina, pt)) = tocco {
            self.tocco_modifica_pdf(pagina, pt);
        }
        self.mostra_barra_modifica(&ctx, zona);
        self.mostra_barra_annotazione(&ctx, zona);
        self.mostra_barra_link(&ctx, zona);
    }

    /// Un tocco sulla pagina nel modo Modifica: chiude la scrittura; fa lo strumento che aspetta;
    /// apre la scrittura di un paragrafo; sceglie un'immagine o un'aggiunta; su un punto vuoto
    /// chiude quello che è scelto o propone Aggiungi testo e Immagine.
    pub(crate) fn tocco_modifica_pdf(&mut self, pagina: usize, pt: Pos2) {
        if self.pdf.as_ref().is_some_and(|p| p.compila.scrittura.is_some()) {
            self.conferma_scrittura_pdf(true);
            return;
        }
        let Some(p) = self.pdf.as_mut() else { return };
        match p.modifica.attesa.take() {
            Some(Attesa::Testo) => {
                diagnosi("pdf: aggiungi testo");
                self.apri_scrittura_pdf(Bersaglio::TestoNuovo { pagina, punto: [pt.x, pt.y] }, "", false);
                return;
            }
            Some(Attesa::Immagine) => {
                p.modifica.per_foto = Some(PerFoto::Nuova(pagina, pt));
                self.fogli_pdf.immagine.apri();
                return;
            }
            None => {}
        }
        if p.modifica.proposta.take().is_some() {
            return;
        }
        // Prima le forme e i link, che stanno sopra il contenuto.
        if self.tocco_annotazione_pdf(pagina, pt, ModoPdf::Modifica) {
            if let Some(p) = self.pdf.as_mut() {
                p.modifica.scelto = None;
                p.modifica.link_scelto = None;
            }
            return;
        }
        let Some(p) = self.pdf.as_mut() else { return };
        let k = p.lettore.dp_per_pt(pagina).max(1e-3);
        let link = p
            .motore
            .stato()
            .collegamenti
            .iter()
            .filter(|c| c.pagina == pagina)
            .find(|c| {
                let r = c.rettangolo;
                let b = BORDO / k;
                pt.x >= r[0] - b && pt.x <= r[2] + b && pt.y >= r[1] - b && pt.y <= r[3] + b
            })
            .map(|c| (c.pagina, c.indice));
        // Un secondo tocco sul link scelto va a quello che copre: senza, un indirizzo nel testo o
        // le righe di un indice non si riscrivevano mai (analisi del 27/10, modifica I7).
        let di_nuovo = link.is_some() && p.modifica.link_scelto == link;
        if let Some(c) = link
            && !di_nuovo
        {
            diagnosi("pdf: link scelto");
            p.modifica.scelto = None;
            p.modifica.link_scelto = Some(c);
            return;
        }
        if !di_nuovo && p.modifica.link_scelto.take().is_some() {
            return;
        }
        let Some(oggetti) = p.modifica.oggetti.get(&pagina).cloned() else {
            // Gli oggetti di questa pagina non ci sono ancora: il tocco li aspetta.
            p.modifica.tocco = Some((pagina, pt));
            if !p.modifica.chieste.contains(&pagina) && p.motore.oggetti(pagina) {
                p.modifica.chieste.insert(pagina);
            }
            return;
        };
        p.modifica.link_scelto = None;
        let k = p.lettore.dp_per_pt(pagina).max(1e-3);
        match oggetto_sotto(&oggetti, pt, BORDO / k).cloned() {
            Some(o) if matches!(o.genere, GenereOggetto::Testo | GenereOggetto::Scansione) || (p.modifica.scelto == Some(o.chiave()) && o.genere.testo()) => {
                p.modifica.scelto = Some(o.chiave());
                self.riscrivi_oggetto_pdf(&o);
            }
            Some(o) => {
                diagnosi(format!("pdf: oggetto scelto ({})", o.genere.nome()));
                p.modifica.scelto = Some(o.chiave());
            }
            None if di_nuovo || p.modifica.scelto.take().is_some() => {}
            None => p.modifica.proposta = Some((pagina, pt)),
        }
    }

    /// La scrittura di un paragrafo o di un testo aggiunto, col testo di adesso.
    fn riscrivi_oggetto_pdf(&mut self, o: &Oggetto) {
        diagnosi(format!("pdf: testo da riscrivere ({})", o.genere.nome()));
        if let Some(p) = self.pdf.as_mut() {
            p.modifica.scelto = Some(o.chiave());
            p.modifica.allargata = None;
        }
        self.apri_scrittura_pdf(Bersaglio::Oggetto(o.chiave()), &o.testo, false);
    }

    /// Il dito ha toccato (o trascina sopra) il testo che si scrive sulla pagina: il cursore va
    /// lì, e la tastiera scrive da lì. Chiusa, la tastiera si riapre.
    pub(crate) fn metti_cursore_pdf(&mut self, byte: usize) {
        if self.fuoco != crate::Fuoco::Campo {
            self.fuoco_al_campo_pdf();
        } else if !self.piattaforma.keyboard_visible() {
            self.piattaforma.show_keyboard();
        }
        let Some(st) = self.pdf.as_mut().and_then(|p| p.compila.scrittura.as_mut()).and_then(|s| s.campo.metti_cursore(byte)) else { return };
        self.versioni.continua_dalla_versione(st.versione);
        self.piattaforma.set_ime_state(st.testo, st.sel.0, st.sel.1, st.versione);
    }

    fn sposta_oggetto_pdf(&mut self, o: &Oggetto, delta: Vec2) {
        let Some(p) = self.pdf.as_ref() else { return };
        let k = p.lettore.dp_per_pt(o.pagina).max(1e-3);
        let Some(info) = p.motore.stato().geometrie.get(o.pagina).cloned() else { return };
        if delta.length() < 2.0 {
            return;
        }
        diagnosi(format!("pdf: oggetto spostato ({})", o.genere.nome()));
        self.modifica_pdf(Modifica::sposta_oggetto(o, &info, delta.x / k, delta.y / k));
    }

    fn misura_oggetto_pdf(&mut self, o: &Oggetto, prima: Rect, dopo: Rect) {
        let Some(p) = self.pdf.as_ref() else { return };
        let Some(info) = p.motore.stato().geometrie.get(o.pagina).cloned() else { return };
        let scala = dopo.width() / prima.width().max(1.0);
        if (scala - 1.0).abs() < 0.02 {
            return;
        }
        let [x0, y0, x1, y1] = o.rettangolo;
        let alto = if o.con_proporzioni() { (y1 - y0) * scala } else { y1 - y0 };
        let nuovo = [x0, y0, x0 + (x1 - x0) * scala, y0 + alto];
        if let Some(m) = Modifica::misura_oggetto(o, &info, nuovo) {
            diagnosi(format!("pdf: oggetto ingrandito di {:.0} % ({})", (scala - 1.0) * 100.0, o.genere.nome()));
            self.modifica_pdf(m);
        }
    }

    /// La barra scura sopra la proposta (Aggiungi testo, Immagine) o sopra l'oggetto scelto.
    fn mostra_barra_modifica(&mut self, ctx: &egui::Context, zona: Rect) {
        let Some(p) = self.pdf.as_ref() else { return };
        if let Some((pagina, pt)) = p.modifica.proposta {
            let Some(r) = p.lettore.sullo_schermo(pagina, [pt.x, pt.y, pt.x, pt.y]) else { return };
            let scelta = BarraContesto::new()
                .azione(icone::TEXT_FIELDS, AGGIUNGI_TESTO)
                .azione(icone::ADD_PHOTO_ALTERNATE, IMMAGINE)
                .mostra_in_primo_piano(ctx, Id::new("proposta_modifica"), r.expand(10.0), zona);
            if let Some(i) = scelta {
                if let Some(p) = self.pdf.as_mut() {
                    p.modifica.proposta = None;
                    p.modifica.attesa = Some(if i == 0 { Attesa::Testo } else { Attesa::Immagine });
                }
                self.tocco_modifica_pdf(pagina, pt);
            }
            return;
        }
        if p.modifica.trascina.is_some() || p.compila.scrittura.is_some() {
            return;
        }
        let Some(o) = p.modifica.scelto.and_then(|c| p.modifica.oggetto(c)) else { return };
        let Some(r) = p.lettore.sullo_schermo(o.pagina, o.rettangolo) else { return };
        let mut voci: Vec<(&str, &str)> = Vec::new();
        if o.genere.testo() {
            voci.push((icone::EDIT, RISCRIVI));
        }
        if matches!(o.genere, GenereOggetto::TestoAggiunto { .. }) {
            voci.push((icone::FORMAT_SIZE, FORMATO));
        }
        if o.genere.sostituibile() {
            voci.push((icone::IMAGE, SOSTITUISCI));
        }
        if o.genere.si_toglie() {
            voci.push((icone::DELETE, TOGLI));
        }
        let mut barra = BarraContesto::new();
        for (icona, nome) in &voci {
            barra = barra.azione(icona, nome);
        }
        let scelta = barra.mostra_in_primo_piano(ctx, Id::new("oggetto_scelto"), r.expand(MANIGLIA + FUORI_DALL_ANGOLO), zona);
        match scelta.and_then(|i| voci.get(i)).map(|v| v.1) {
            Some(RISCRIVI) => self.riscrivi_oggetto_pdf(&o),
            Some(FORMATO) => self.fogli_pdf.testo.apri(),
            Some(SOSTITUISCI) => {
                if let Some(p) = self.pdf.as_mut() {
                    p.modifica.per_foto = Some(PerFoto::Sostituisci(o.chiave()));
                }
                self.fogli_pdf.immagine.apri();
            }
            Some(_) => {
                diagnosi(format!("pdf: oggetto tolto ({})", o.genere.nome()));
                if let Some(p) = self.pdf.as_mut() {
                    p.modifica.scelto = None;
                }
                if let Some(m) = Modifica::togli_oggetto(&o) {
                    self.modifica_pdf(m);
                }
            }
            None => {}
        }
    }

    /// Oscura: i rettangoli disegnati col dito sulla pagina (neri col bordo rosso), quello che si
    /// disegna, e la barra con «Lascia stare» e «Applica».
    fn mostra_oscura(&mut self, ui: &mut egui::Ui, pagine: Rect, zona: Rect) {
        let ctx = ui.ctx().clone();
        let Some(p) = self.pdf.as_mut() else { return };
        let painter = ui.painter_at(pagine);
        let risposta = ui.interact(pagine, Id::new("oscura_pdf"), Sense::drag());
        risposta.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, DISEGNA));
        let Some(o) = p.modifica.oscura.as_mut() else { return };
        if risposta.drag_started()
            && let Some(da) = ui.input(|i| i.pointer.press_origin())
        {
            o.disegna = Some((da, da));
        }
        if risposta.dragged()
            && let (Some((_, a)), Some(qui)) = (o.disegna.as_mut(), risposta.interact_pointer_pos())
        {
            *a = qui;
        }
        if risposta.drag_stopped()
            && let Some((da, a)) = o.disegna.take()
            && let (Some((pagina, x)), Some((pagina_b, y))) = (p.lettore.sulla_pagina(da), p.lettore.sulla_pagina(a))
            && pagina == pagina_b
            && o.pagina.is_none_or(|q| q == pagina)
        {
            let r = [x.x.min(y.x), x.y.min(y.y), x.x.max(y.x), x.y.max(y.y)];
            if r[2] - r[0] >= 2.0 && r[3] - r[1] >= 2.0 {
                o.pagina = Some(pagina);
                o.rettangoli.push(r);
                diagnosi(format!("pdf: oscura, {} rettangoli", o.rettangoli.len()));
            }
        }
        if let Some(pagina) = o.pagina {
            for r in &o.rettangoli {
                if let Some(s) = p.lettore.sullo_schermo(pagina, *r) {
                    painter.rect(s, 0.0, NERO, Stroke::new(1.5, ROSSO), StrokeKind::Outside);
                }
            }
        }
        if let Some((da, a)) = o.disegna {
            painter.rect(da_due_punti(da, a), 0.0, NERO, Stroke::new(1.5, ROSSO), StrokeKind::Outside);
        }
        let pronti = !o.rettangoli.is_empty();
        let sotto = Rect::from_center_size(pos2(zona.center().x, zona.bottom() - 40.0), vec2(1.0, 1.0));
        let g = painter.layout(DISEGNA.to_string(), Carattere::Testo.font(&ctx, 14.0), tema::FOGLIO, zona.width() - 48.0);
        let fondo = Rect::from_center_size(pos2(zona.center().x, zona.top() + 28.0), g.size() + vec2(24.0, 14.0));
        painter.rect_filled(fondo, 8.0, tema::INCHIOSTRO);
        painter.galley(fondo.min + vec2(12.0, 7.0), g, tema::FOGLIO);
        let mut barra = BarraContesto::new().azione(icone::CLOSE, ANNULLA_OSCURA);
        if pronti {
            barra = barra.azione(icone::CHECK, APPLICA);
        }
        match barra.mostra_in_primo_piano(&ctx, Id::new("barra_oscura"), sotto, zona) {
            Some(0) => {
                diagnosi("pdf: oscura, lasciato stare");
                p.modifica.oscura = None;
            }
            Some(_) => {
                if let Some(o) = p.modifica.oscura.as_mut() {
                    o.conferma = true;
                }
            }
            None => {}
        }
    }

    /// Il testo scritto per un oggetto, un testo nuovo, la filigrana o l'intestazione: va al motore
    /// (o nel foglio), alla conferma della scrittura.
    pub(crate) fn conferma_testo_modifica(&mut self, bersaglio: &Bersaglio, testo: &str) {
        let Some(p) = self.pdf.as_mut() else { return };
        match bersaglio {
            Bersaglio::Oggetto(c) => {
                let Some(o) = p.modifica.oggetto(*c) else { return };
                // Un testo aggiunto si scrive nel carattere standard: un simbolo dava l'errore
                // generico del motore (analisi del 27/10, modifica I6). Come per un testo nuovo.
                if matches!(o.genere, GenereOggetto::TestoAggiunto { .. })
                    && let Some(c) = carattere_che_manca(testo)
                {
                    diagnosi("pdf: testo aggiunto riscritto con un carattere che non si scrive");
                    self.errore = Some(Errore {
                        titolo: "Testo non riscritto",
                        testo: format!("Il carattere «{c}» non si può scrivere in un testo aggiunto: le lettere italiane e «€» sì, i simboli no."),
                    });
                    return;
                }
                // Il testo di una scansione si corregge subito, nella foto: le parole cambiate col
                // carattere della scansione (dalla 0.8.8 senza la domanda di prima, che avvisava
                // dell'Helvetica). Annulla torna indietro. Uguale a prima, niente.
                if o.genere == GenereOggetto::Scansione {
                    p.modifica.scelto = None;
                    if testo.trim() != o.testo.trim()
                        && let Some(m) = Modifica::correggi_scansione(&o, testo)
                    {
                        diagnosi(format!("pdf: scansione da correggere nella pagina {}", o.pagina + 1));
                        p.modifica.correzione = Some((o.chiave(), testo.to_string()));
                        self.modifica_pdf(m);
                    }
                    return;
                }
                let Some(info) = p.motore.stato().geometrie.get(o.pagina).cloned() else { return };
                // Allargato con la presa: testo e larghezza nuova insieme.
                let allargata = p.modifica.allargata.take().filter(|k| (k - 1.0).abs() > 0.01 && o.genere == GenereOggetto::Testo);
                let m = if testo.trim().is_empty() && o.genere != GenereOggetto::Testo {
                    Modifica::togli_oggetto(&o)
                } else if let Some(k) = allargata {
                    Modifica::testo_largo(&o, &info, testo, k)
                } else {
                    Modifica::testo_dell_oggetto(&o, &info, testo)
                };
                if let Some(m) = m {
                    let allargato = if allargata.is_some() { ", allargato" } else { "" };
                    diagnosi(format!("pdf: testo riscritto ({}){allargato}", o.genere.nome()));
                    self.modifica_pdf(m);
                }
                // Un paragrafo del PDF riscritto non resta scelto: scorrendo la pagina col dito
                // appoggiato lì si spostava (analisi del 27/10, modifica I8). Un testo aggiunto sì,
                // per metterlo a posto col dito.
                if o.genere == GenereOggetto::Testo
                    && let Some(p) = self.pdf.as_mut()
                {
                    p.modifica.scelto = None;
                }
            }
            Bersaglio::TestoNuovo { pagina, punto } => {
                if let Some(c) = carattere_che_manca(testo) {
                    diagnosi("pdf: testo nuovo con un carattere che non si scrive");
                    self.errore = Some(Errore {
                        titolo: "Testo non aggiunto",
                        testo: format!("Il carattere «{c}» non si può scrivere in un testo nuovo: le lettere italiane e «€» sì, i simboli no."),
                    });
                    return;
                }
                let Some(info) = p.motore.stato().geometrie.get(*pagina).cloned() else { return };
                let colore = COLORI[p.modifica.colore].0;
                if let Some(m) = Modifica::testo_nuovo(*pagina, &info, *punto, testo, f64::from(p.modifica.misura()), colore) {
                    diagnosi("pdf: testo nuovo posato");
                    p.modifica.da_scegliere = Some(*pagina);
                    self.modifica_pdf(m);
                }
            }
            Bersaglio::Filigrana => {
                p.modifica.filigrana_scritta = testo.trim().to_string();
                p.modifica.filigrana = FILIGRANE.len();
                self.fogli_pdf.filigrana.apri();
            }
            Bersaglio::Intestazione => {
                p.modifica.intestazione = testo.trim().to_string();
                self.fogli_pdf.numeri.apri();
            }
            Bersaglio::Indirizzo => {
                if let Some(l) = p.modifica.link.as_mut() {
                    l.indirizzo = testo.trim().to_string();
                    l.a_indirizzo = true;
                }
                self.fogli_pdf.link.apri();
            }
            _ => {}
        }
    }

    /// Le risposte del motore per il modo Modifica: gli oggetti di una pagina (e il tocco che li
    /// aspettava), le pagine da rileggere dopo una modifica, l'avviso del carattere.
    pub(crate) fn modifica_dopo_il_motore(&mut self, evento: &Evento<crate::pdf_lavoro::ScrittoPdf>) {
        let Some(p) = self.pdf.as_mut() else { return };
        let m = &mut p.modifica;
        let via = |m: &mut StatoModifica, t: &Toccate| match t {
            Toccate::Tutte => {
                m.oggetti.clear();
                m.chieste.clear();
            }
            Toccate::Alcune(v) => {
                for pagina in v {
                    m.oggetti.remove(pagina);
                    m.chieste.remove(pagina);
                }
            }
            Toccate::Nessuna => {}
        };
        match evento {
            Evento::Applicata { toccate, .. } => via(m, toccate),
            Evento::Annullata { toccate } | Evento::Ripetuta { toccate } => {
                via(m, toccate);
                m.scelto = None;
                m.trascina = None;
            }
            Evento::Rifiutata { .. } | Evento::NonApplicata { .. } => m.da_scegliere = None,
            Evento::CambiaCarattere { modifica, .. } => {
                diagnosi("pdf: il paragrafo cambierebbe carattere, si avvisa");
                m.carattere = Some((**modifica).clone());
            }
            Evento::Oggetti { pagina, oggetti } => {
                let pagina = *pagina;
                diagnosi(format!("pdf: oggetti della pagina {}, {}", pagina + 1, oggetti.len()));
                m.chieste.remove(&pagina);
                m.oggetti.insert(pagina, oggetti.clone());
                if m.da_scegliere == Some(pagina) {
                    m.da_scegliere = None;
                    let nuovo = oggetti
                        .iter()
                        .filter(|o| matches!(o.genere, GenereOggetto::TestoAggiunto { .. } | GenereOggetto::ImmagineAggiunta))
                        .max_by_key(|o| o.chiave());
                    m.scelto = nuovo.map(Oggetto::chiave);
                }
                if m.scelto.is_some_and(|c| c.0 == pagina && !oggetti.iter().any(|o| o.chiave() == c)) {
                    m.scelto = None;
                }
                if let Some((t, pt)) = m.tocco.filter(|(t, _)| *t == pagina) {
                    m.tocco = None;
                    self.tocco_modifica_pdf(t, pt);
                }
            }
            _ => {}
        }
    }

    /// Uno strumento del modo Modifica dal foglio Strumenti.
    pub(crate) fn strumento_modifica_pdf(&mut self, s: crate::pdf_lavoro::Strumento) {
        use crate::pdf_lavoro::Strumento;
        match s {
            Strumento::Filigrana => {
                self.fogli_pdf.filigrana.apri();
                return;
            }
            Strumento::NumeriDiPagina => {
                self.fogli_pdf.numeri.apri();
                return;
            }
            _ => {}
        }
        self.scegli_modo_pdf(ModoPdf::Modifica);
        let Some(p) = self.pdf.as_mut() else { return };
        p.modifica.chiudi();
        match s {
            Strumento::ModificaTesto => self.avviso_breve.avvisa("Tocca il testo da modificare"),
            Strumento::AggiungiTesto => {
                p.modifica.attesa = Some(Attesa::Testo);
                self.avviso_breve.avvisa("Tocca dove scrivere il testo");
            }
            Strumento::Immagine => {
                p.modifica.attesa = Some(Attesa::Immagine);
                self.avviso_breve.avvisa("Tocca dove mettere l'immagine");
            }
            Strumento::Oscura => p.modifica.oscura = Some(Oscura::default()),
            Strumento::Forme => p.modifica.forme = Some(Forme::default()),
            Strumento::Link => {
                p.modifica.link = Some(NuovoLink { pagina: p.lettore.pagina_corrente(), ..Default::default() });
            }
            _ => {}
        }
    }

    /// A ogni frame: la foto chiesta per un'immagine, se è arrivata, si riduce sul suo thread; poi
    /// va nella pagina (nuova o al posto di quella scelta).
    pub(crate) fn passo_immagine_pdf(&mut self, ctx: &egui::Context) {
        if let Some((per, richiesta)) = self.pdf.as_ref().and_then(|p| p.modifica.foto)
            && let Some(esito) = self.piattaforma.prendi_foto_firma(richiesta)
        {
            let Some(p) = self.pdf.as_mut() else { return };
            p.modifica.foto = None;
            match esito {
                EsitoFoto::Pronta(file) => {
                    diagnosi("pdf: immagine arrivata");
                    let (tx, rx) = std::sync::mpsc::channel();
                    let ctx2 = ctx.clone();
                    let avviato = std::thread::Builder::new().name("pdf-immagine".into()).spawn(move || {
                        let letta = std::fs::File::open(&file).and_then(|f| {
                            let mut v = Vec::new();
                            std::io::Read::read_to_end(&mut std::io::Read::take(f, MASSIMO_FOTO + 1), &mut v).map(|_| v)
                        });
                        crate::butta_la_foto(&file);
                        let esito = match letta {
                            Ok(b) if b.len() as u64 <= MASSIMO_FOTO => tascabile_pdf::firma::immagine_da_foto(&b),
                            Ok(_) => Err(ErroreFoto::Grande),
                            Err(_) => Err(ErroreFoto::Lettura),
                        };
                        match &esito {
                            Ok((b, _)) => diagnosi(format!("pdf: immagine pronta, {} byte", b.len())),
                            Err(e) => diagnosi(format!("pdf: immagine non pronta ({})", e.genere())),
                        }
                        let _ = tx.send(esito);
                        ctx2.request_repaint();
                    });
                    if avviato.is_ok() {
                        p.modifica.lavoro = Some((per, rx));
                    } else {
                        diagnosi("pdf: thread dell'immagine non avviato");
                    }
                }
                EsitoFoto::Pronte { file, .. } => crate::butta_le_foto(&file, "pdf: immagine"),
                EsitoFoto::Annullata => diagnosi("pdf: immagine annullata"),
                EsitoFoto::Superata => diagnosi("pdf: immagine, richiesta superata da un'altra"),
                EsitoFoto::Errore => {
                    diagnosi("pdf: immagine non arrivata");
                    self.avviso_breve.avvisa("L'immagine non è arrivata.");
                }
            }
        }
        let Some(p) = self.pdf.as_mut() else { return };
        let Some((per, rx)) = p.modifica.lavoro.as_ref() else { return };
        let per = *per;
        match rx.try_recv() {
            Ok(Ok((byte, proporzione))) => {
                p.modifica.lavoro = None;
                let byte = Arc::new(byte);
                let m = match per {
                    PerFoto::Nuova(pagina, pt) => {
                        let info = p.motore.stato().geometrie.get(pagina).cloned();
                        p.modifica.da_scegliere = Some(pagina);
                        info.and_then(|i| Modifica::immagine_nuova(pagina, &i, [pt.x, pt.y], byte, proporzione))
                    }
                    PerFoto::Sostituisci(c) => p.modifica.oggetto(c).and_then(|o| Modifica::sostituisci_immagine(&o, byte, proporzione)),
                };
                if let Some(m) = m {
                    diagnosi(if matches!(per, PerFoto::Nuova(..)) { "pdf: immagine inserita" } else { "pdf: immagine sostituita" });
                    self.modifica_pdf(m);
                }
            }
            Ok(Err(e)) => {
                p.modifica.lavoro = None;
                self.avviso_breve.avvisa(match e {
                    ErroreFoto::Inchiostro => "L'immagine non si legge.",
                    e => e.messaggio(),
                });
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => p.modifica.lavoro = None,
        }
    }

    /// Per le prove a schermo: gli oggetti della pagina `pagina` che il motore ha dato, col
    /// genere, il testo e il posto sullo schermo (aspettando il motore, al massimo 20 s).
    #[doc(hidden)]
    pub fn oggetti_pdf_di_prova(&mut self, pagina: usize) -> Vec<(GenereOggetto, String, Rect)> {
        self.aspetta_il_motore_pdf();
        let Some(p) = self.pdf.as_ref() else { return Vec::new() };
        let Some(oggetti) = p.modifica.oggetti.get(&pagina) else { return Vec::new() };
        oggetti.iter().filter_map(|o| Some((o.genere, o.testo.clone(), p.lettore.sullo_schermo(o.pagina, o.rettangolo)?))).collect()
    }

    /// Per le prove a schermo e sul PC: arriva l'immagine `byte` chiesta ad Android.
    #[doc(hidden)]
    #[cfg(not(target_os = "android"))]
    pub fn simula_immagine_di_prova(&self, byte: &[u8]) {
        self.piattaforma.simula_foto_firma(Some(byte));
    }

    /// I fogli e i dialoghi del modo Modifica: da dove viene l'immagine, il formato di un testo
    /// aggiunto, la filigrana, i numeri di pagina, l'avviso del carattere, la conferma di Oscura.
    pub(crate) fn mostra_fogli_modifica(&mut self, ctx: &egui::Context) {
        let fonte = self
            .fogli_pdf
            .immagine
            .mostra(ctx, IMMAGINE, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let mut fonte = None;
                if crate::pdf_firma::riga(ui, icone::IMAGE, DALLA_GALLERIA, None, false).clicked() {
                    fonte = Some(FonteFoto::Galleria);
                }
                if crate::pdf_firma::riga(ui, icone::PHOTO_CAMERA, DALLA_FOTOCAMERA, None, true).clicked() {
                    fonte = Some(FonteFoto::Fotocamera);
                }
                fonte
            })
            .flatten();
        if let Some(fonte) = fonte {
            self.fogli_pdf.immagine.chiudi();
            diagnosi(format!("pdf: immagine dalla {}", fonte.nome()));
            let richiesta = self.piattaforma.chiedi_foto_firma(fonte);
            if let Some(p) = self.pdf.as_mut() {
                p.modifica.foto = p.modifica.per_foto.take().map(|per| (per, richiesta));
            }
        }
        if !self.fogli_pdf.immagine.aperto()
            && let Some(p) = self.pdf.as_mut()
            && p.modifica.foto.is_none()
        {
            p.modifica.per_foto = None;
        }
        self.mostra_foglio_testo_pdf(ctx);
        self.mostra_foglio_filigrana(ctx);
        self.mostra_foglio_numeri(ctx);
        self.mostra_foglio_link(ctx);
        self.mostra_dialoghi_modifica(ctx);
    }

    /// Il formato del testo aggiunto scelto: la misura coi passi e il colore. Vale anche per i
    /// testi nuovi dopo.
    fn mostra_foglio_testo_pdf(&mut self, ctx: &egui::Context) {
        let Some(p) = self.pdf.as_ref() else { return };
        let (misura, colore) = match p.modifica.scelto.and_then(|c| p.modifica.oggetto(c)).map(|o| o.genere) {
            Some(GenereOggetto::TestoAggiunto { misura, colore }) => {
                (misura, COLORI.iter().position(|(c, _)| c.iter().zip(colore).all(|(a, b)| (a - b).abs() < 0.02)))
            }
            _ => (p.modifica.misura(), Some(p.modifica.colore)),
        };
        let scelta = self
            .fogli_pdf
            .testo
            .mostra(ctx, FORMATO, |ui| {
                let mut scelta = None;
                etichetta_sezione(ui, "Misura");
                let valore = format!("{} pt", misura.round());
                match Passo::new(&valore, "Più piccolo", "Più grande").meno_spento(misura <= 6.0).mostra(ui) {
                    Some(Verso::Meno) => scelta = Some(((misura - PASSO_MISURA).max(6.0), colore.unwrap_or(0))),
                    Some(Verso::Piu) => scelta = Some(((misura + PASSO_MISURA).min(96.0), colore.unwrap_or(0))),
                    None => {}
                }
                ui.add_space(12.0);
                etichetta_sezione(ui, "Colore");
                let tinte: Vec<(Tinta, &str)> = COLORI
                    .iter()
                    .map(|(c, n)| (Tinta::Colore(Color32::from_rgb((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8)), *n))
                    .collect();
                if let Some(i) = Tavolozza::new(&tinte).acceso(colore).mostra(ui) {
                    scelta = Some((misura, i));
                }
                ui.add_space(12.0);
                scelta
            })
            .flatten();
        let Some((misura, colore)) = scelta else { return };
        let Some(p) = self.pdf.as_mut() else { return };
        p.modifica.misura = Some(misura);
        p.modifica.colore = colore;
        if let Some(o) = p.modifica.scelto.and_then(|c| p.modifica.oggetto(c))
            && let Some(info) = p.motore.stato().geometrie.get(o.pagina).cloned()
            && let Some(m) = Modifica::stile_del_testo(&o, &info, f64::from(misura), COLORI[colore].0)
        {
            diagnosi(format!("pdf: testo aggiunto a {misura} pt, colore {}", colore + 1));
            self.modifica_pdf(m);
        }
    }

    /// Il foglio della filigrana: il testo (pronti o scritto), l'opacità, «Metti la filigrana» e,
    /// se ce n'è una, «Togli la filigrana».
    fn mostra_foglio_filigrana(&mut self, ctx: &egui::Context) {
        let Some(p) = self.pdf.as_ref() else { return };
        let (scritta, scelta, opacita, c_e) =
            (p.modifica.filigrana_scritta.clone(), p.modifica.filigrana, p.modifica.opacita, p.motore.stato().filigrana);
        #[derive(Clone, Copy)]
        enum Scelta {
            Testo(usize),
            Scrivi,
            Opacita(usize),
            Metti,
            Togli,
        }
        let risposta = self
            .fogli_pdf
            .filigrana
            .mostra(ctx, FILIGRANA, |ui| {
                let mut r = None;
                etichetta_sezione(ui, "Testo");
                ui.horizontal_wrapped(|ui| {
                    for (i, t) in FILIGRANE.iter().enumerate() {
                        if ui.add(Chip::new(t).acceso(scelta == i)).clicked() {
                            r = Some(Scelta::Testo(i));
                        }
                    }
                    if !scritta.is_empty() && ui.add(Chip::new(&scritta).acceso(scelta == FILIGRANE.len())).clicked() {
                        r = Some(Scelta::Testo(FILIGRANE.len()));
                    }
                    if ui.add(Chip::new(SCRIVI_ALTRO).icona(icone::EDIT)).clicked() {
                        r = Some(Scelta::Scrivi);
                    }
                });
                ui.add_space(12.0);
                etichetta_sezione(ui, "Opacità");
                ui.horizontal_wrapped(|ui| {
                    for (i, (_, nome)) in OPACITA.iter().enumerate() {
                        if ui.add(Chip::new(nome).acceso(opacita == i)).clicked() {
                            r = Some(Scelta::Opacita(i));
                        }
                    }
                });
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    if ui.add(Pulsante::new(METTI_FILIGRANA, Peso::Principale)).clicked() {
                        r = Some(Scelta::Metti);
                    }
                    if c_e && ui.add(Pulsante::new(TOGLI_FILIGRANA, Peso::Testo)).clicked() {
                        r = Some(Scelta::Togli);
                    }
                });
                ui.add_space(12.0);
                r
            })
            .flatten();
        let Some(r) = risposta else { return };
        let Some(p) = self.pdf.as_mut() else { return };
        match r {
            Scelta::Testo(i) => p.modifica.filigrana = i,
            Scelta::Opacita(i) => p.modifica.opacita = i,
            Scelta::Scrivi => {
                self.fogli_pdf.filigrana.chiudi();
                let testo = p.modifica.filigrana_scritta.clone();
                self.apri_scrittura_pdf(Bersaglio::Filigrana, &testo, false);
            }
            Scelta::Metti => {
                let testo = FILIGRANE.get(p.modifica.filigrana).map_or(p.modifica.filigrana_scritta.clone(), |t| t.to_string());
                if let Some(c) = carattere_che_manca(&testo) {
                    self.errore =
                        Some(Errore { titolo: "Filigrana non messa", testo: format!("Il carattere «{c}» non si può scrivere in una filigrana.") });
                    return;
                }
                self.fogli_pdf.filigrana.chiudi();
                diagnosi(format!("pdf: filigrana, opacità {}", OPACITA[p.modifica.opacita].1.to_lowercase()));
                let m = Modifica::filigrana(p.lettore.pagine(), &testo, OPACITA[p.modifica.opacita].0);
                self.modifica_pdf(m);
            }
            Scelta::Togli => {
                self.fogli_pdf.filigrana.chiudi();
                diagnosi("pdf: filigrana tolta");
                self.modifica_pdf(Modifica::togli_marche(false));
            }
        }
    }

    /// Il foglio dei numeri di pagina: il piè di pagina («Pagina 1 di 5», «1 di 5», «1»),
    /// l'intestazione scritta, «Metti i numeri» e, se ci sono, «Togli numeri e intestazione».
    fn mostra_foglio_numeri(&mut self, ctx: &egui::Context) {
        let Some(p) = self.pdf.as_ref() else { return };
        let (pie, intestazione, c_e) = (p.modifica.pie, p.modifica.intestazione.clone(), p.motore.stato().numeri);
        #[derive(Clone, Copy)]
        enum Scelta {
            Pie(usize),
            Scrivi,
            Metti,
            Togli,
        }
        let risposta = self
            .fogli_pdf
            .numeri
            .mostra(ctx, NUMERI, |ui| {
                let mut r = None;
                etichetta_sezione(ui, "Piè di pagina");
                ui.horizontal_wrapped(|ui| {
                    for (i, (_, nome)) in PIE.iter().enumerate() {
                        if ui.add(Chip::new(nome).acceso(pie == i)).clicked() {
                            r = Some(Scelta::Pie(i));
                        }
                    }
                });
                ui.add_space(12.0);
                etichetta_sezione(ui, "Intestazione");
                let nome = if intestazione.is_empty() { SCRIVI_INTESTAZIONE } else { intestazione.as_str() };
                if ui.add(Chip::new(nome).icona(icone::EDIT).acceso(!intestazione.is_empty())).clicked() {
                    r = Some(Scelta::Scrivi);
                }
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    if ui.add(Pulsante::new(METTI_NUMERI, Peso::Principale)).clicked() {
                        r = Some(Scelta::Metti);
                    }
                    if c_e && ui.add(Pulsante::new(TOGLI_NUMERI, Peso::Testo)).clicked() {
                        r = Some(Scelta::Togli);
                    }
                });
                ui.add_space(12.0);
                r
            })
            .flatten();
        let Some(r) = risposta else { return };
        let Some(p) = self.pdf.as_mut() else { return };
        match r {
            Scelta::Pie(i) => p.modifica.pie = i,
            Scelta::Scrivi => {
                self.fogli_pdf.numeri.chiudi();
                let testo = p.modifica.intestazione.clone();
                self.apri_scrittura_pdf(Bersaglio::Intestazione, &testo, false);
            }
            Scelta::Metti => {
                if let Some(c) = carattere_che_manca(&p.modifica.intestazione) {
                    self.errore =
                        Some(Errore { titolo: "Numeri non messi", testo: format!("Il carattere «{c}» non si può scrivere nell'intestazione.") });
                    return;
                }
                self.fogli_pdf.numeri.chiudi();
                diagnosi(format!("pdf: numeri di pagina, formato {}", p.modifica.pie + 1));
                let m = Modifica::numeri_di_pagina(p.lettore.pagine(), &p.modifica.intestazione, PIE[p.modifica.pie].0);
                self.modifica_pdf(m);
            }
            Scelta::Togli => {
                self.fogli_pdf.numeri.chiudi();
                diagnosi("pdf: numeri di pagina tolti");
                self.modifica_pdf(Modifica::togli_marche(true));
            }
        }
    }

    /// L'avviso del carattere (Continua o Lascia com'era) e la conferma di Oscura.
    fn mostra_dialoghi_modifica(&mut self, ctx: &egui::Context) {
        let Some(p) = self.pdf.as_ref() else { return };
        if p.modifica.carattere.is_some() {
            let r = Dialogo::new(Id::new("pdf_carattere"), CAMBIA_CARATTERE)
                .testo(CAMBIA_CARATTERE_TESTO)
                .pulsante(LASCIA_COM_ERA, Peso::Testo)
                .pulsante(CONTINUA, Peso::Principale)
                .mostra(ctx);
            match r {
                Some(RispostaDialogo::Pulsante(1)) => {
                    if let Some(m) = self.pdf.as_mut().and_then(|p| p.modifica.carattere.take()) {
                        diagnosi("pdf: carattere, si continua");
                        self.modifica_pdf(m.anche_se_cambia_carattere());
                    }
                }
                Some(_) => {
                    self.lascia_il_carattere_pdf();
                }
                None => {}
            }
            return;
        }
        if p.modifica.oscura.as_ref().is_some_and(|o| o.conferma) {
            let testo = crate::pdf_lavoro::con_la_copia(OSCURARE_TESTO, p);
            let r = Dialogo::new(Id::new("pdf_oscura"), OSCURARE)
                .testo(&testo)
                .pulsante(crate::schermo_pdf::ANNULLA, Peso::Testo)
                .pulsante(OSCURA, Peso::Principale)
                .mostra(ctx);
            match r {
                Some(RispostaDialogo::Pulsante(1)) => self.applica_oscura_pdf(),
                Some(_) => {
                    if let Some(o) = self.pdf.as_mut().and_then(|p| p.modifica.oscura.as_mut()) {
                        o.conferma = false;
                    }
                }
                None => {}
            }
        }
    }

    /// «Lascia com'era» all'avviso del carattere (o Indietro): la riscrittura non si fa.
    pub(crate) fn lascia_il_carattere_pdf(&mut self) -> bool {
        let tolta = self.pdf.as_mut().and_then(|p| p.modifica.carattere.take()).is_some();
        if tolta {
            diagnosi("pdf: carattere, lasciato com'era");
        }
        tolta
    }

    fn applica_oscura_pdf(&mut self) {
        let Some(p) = self.pdf.as_mut() else { return };
        let Some(o) = p.modifica.oscura.take() else { return };
        let Some(pagina) = o.pagina else { return };
        let Some(info) = p.motore.stato().geometrie.get(pagina).cloned() else { return };
        if let Some(m) = Modifica::oscura(pagina, &info, &o.rettangoli) {
            diagnosi(format!("pdf: oscura, {} rettangoli applicati nella pagina {}", o.rettangoli.len(), pagina + 1));
            self.modifica_pdf(m);
        }
    }

    /// I link delle pagine: tratteggiati, quello scelto col riquadro blu.
    fn disegna_i_link(&self, ui: &egui::Ui, pagine: Rect) {
        let Some(p) = self.pdf.as_ref() else { return };
        let painter = ui.painter_at(pagine);
        for c in p.motore.stato().collegamenti.iter() {
            let Some(r) = p.lettore.sullo_schermo(c.pagina, c.rettangolo) else { continue };
            if !r.intersects(pagine) {
                continue;
            }
            if p.modifica.link_scelto == Some((c.pagina, c.indice)) {
                painter.rect_stroke(r.expand(2.0), 2.0, Stroke::new(1.5, tema::BLU), StrokeKind::Outside);
            } else {
                tratteggia(&painter, r.expand(1.0));
            }
        }
    }

    /// Le forme col dito: la striscia con le quattro forme e Fine sopra la barra dei modi, sotto i
    /// colori e gli spessori; il dito disegna sulla pagina dove si appoggia, e alla fine la forma
    /// va al motore. Il pizzico resta libero.
    fn mostra_forme(&mut self, ui: &mut egui::Ui, pagine: Rect, sotto: f32) {
        let autore = self.impostazioni.autore.clone();
        let Some(p) = self.pdf.as_mut() else { return };
        let Some(f) = p.modifica.forme.as_mut() else { return };
        let risposta = ui.interact(pagine, Id::new("forme_pdf"), Sense::click_and_drag());
        risposta.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, DISEGNA_FORMA));
        let dita = ui.input(|i| i.multi_touch().is_some()) || p.lettore.pizzica();
        if dita {
            f.disegna = None;
        }
        if risposta.drag_started()
            && !dita
            && let Some(da) = ui.input(|i| i.pointer.press_origin())
        {
            f.disegna = Some((da, da));
        }
        if risposta.dragged()
            && let (Some((_, a)), Some(qui)) = (f.disegna.as_mut(), risposta.interact_pointer_pos())
        {
            *a = qui;
        }
        // La scala della pagina dove comincia la forma.
        let k = f.disegna.and_then(|(da, _)| p.lettore.sulla_pagina(da)).map_or(1.0, |(pagina, _)| p.lettore.dp_per_pt(pagina)).max(1e-3);
        let (colore, spessore) = (crate::pdf_annota::COLORI[f.colore].0, crate::pdf_annota::SPESSORI[f.spessore].0);
        if let Some((da, a)) = f.disegna {
            let tratto = Stroke::new((spessore * k).max(1.0), crate::pdf_annota::colore32(colore));
            let painter = ui.painter_at(pagine);
            match f.forma {
                Forma::Rettangolo => {
                    painter.rect_stroke(Rect::from_two_pos(da, a), 0.0, tratto, StrokeKind::Middle);
                }
                Forma::Ellisse => {
                    let r = Rect::from_two_pos(da, a);
                    painter.add(egui::Shape::ellipse_stroke(r.center(), r.size() / 2.0, tratto));
                }
                Forma::Linea => {
                    painter.line_segment([da, a], tratto);
                }
                Forma::Freccia => {
                    painter.arrow(da, a - da, tratto);
                }
            }
        }
        let fatta = if risposta.drag_stopped() { f.disegna.take() } else { None };
        let forma = f.forma;
        if let Some((da, a)) = fatta
            && let (Some((pagina, x)), Some((pagina_b, y))) = (p.lettore.sulla_pagina(da), p.lettore.sulla_pagina(a))
            && pagina == pagina_b
            && let Some(info) = p.motore.stato().geometrie.get(pagina).cloned()
        {
            match Modifica::forma(pagina, &info, forma, [[x.x, x.y], [y.x, y.y]], colore, spessore, &autore) {
                Some(m) => {
                    diagnosi(format!("pdf: forma disegnata ({})", tascabile_pdf::GenereAnnotazione::Forma(forma).nome()));
                    self.modifica_pdf(m);
                }
                None => diagnosi("pdf: forma troppo piccola"),
            }
        }
        // La striscia: le quattro forme e Fine, sotto i colori e gli spessori.
        let Some(p) = self.pdf.as_mut() else { return };
        let Some(f) = p.modifica.forme.as_ref() else { return };
        let riga = crate::pdf_annota::RIGA;
        let fondo = pagine.bottom() - sotto + 8.0 - 8.0;
        let prima = Rect::from_min_max(pos2(pagine.left() + tema::MARGINE, fondo - riga), pos2(pagine.right() - tema::MARGINE, fondo));
        let mut barra = BarraFlottante::new();
        for (forma, icona, nome) in FORME {
            barra = barra.pulsante(PulsanteIcona::new(icona, nome).acceso(f.forma == forma));
        }
        let scelto = barra.separatore().pulsante(PulsanteIcona::new(icone::CHECK, FINE_FORME)).mostra(ui, prima);
        let seconda = prima.translate(vec2(0.0, -(riga + 8.0)));
        let scelta = crate::pdf_annota::riga_dei_colori(ui, seconda, f.colore, Some(f.spessore), None, Id::new("riga_forme"));
        let Some(p) = self.pdf.as_mut() else { return };
        match scelto {
            Some(i) if i < FORME.len() => {
                if let Some(f) = p.modifica.forme.as_mut() {
                    f.forma = FORME[i].0;
                }
            }
            Some(_) => {
                diagnosi("pdf: forme finite");
                p.modifica.forme = None;
            }
            None => {}
        }
        if let Some(f) = p.modifica.forme.as_mut() {
            match scelta {
                Some(crate::pdf_annota::Scelta::Colore(i)) => f.colore = i,
                Some(crate::pdf_annota::Scelta::Spessore(i)) => f.spessore = i,
                _ => {}
            }
        }
    }

    /// Il riquadro di un link nuovo, disegnato col dito; poi il foglio di dove porta.
    fn mostra_disegno_link(&mut self, ui: &mut egui::Ui, pagine: Rect, zona: Rect) {
        let ctx = ui.ctx().clone();
        let Some(p) = self.pdf.as_mut() else { return };
        let painter = ui.painter_at(pagine);
        let risposta = ui.interact(pagine, Id::new("link_pdf"), Sense::click_and_drag());
        risposta.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, DISEGNA_LINK));
        let Some(l) = p.modifica.link.as_mut() else { return };
        if risposta.drag_started()
            && let Some(da) = ui.input(|i| i.pointer.press_origin())
        {
            l.disegna = Some((da, da));
        }
        if risposta.dragged()
            && let (Some((_, a)), Some(qui)) = (l.disegna.as_mut(), risposta.interact_pointer_pos())
        {
            *a = qui;
        }
        if let Some((da, a)) = l.disegna {
            let r = da_due_punti(da, a);
            painter.rect_filled(r, 2.0, FONDO_SPOSTA);
            tratteggia(&painter, r);
        }
        let mut apri = false;
        if risposta.drag_stopped()
            && let Some((da, a)) = l.disegna.take()
            && let (Some((pagina, x)), Some((pagina_b, y))) = (p.lettore.sulla_pagina(da), p.lettore.sulla_pagina(a))
            && pagina == pagina_b
        {
            let r = [x.x.min(y.x), x.y.min(y.y), x.x.max(y.x), x.y.max(y.y)];
            if r[2] - r[0] >= 4.0 && r[3] - r[1] >= 4.0 {
                diagnosi(format!("pdf: riquadro del link nella pagina {}", pagina + 1));
                l.riquadro = Some((pagina, r));
                apri = true;
            }
        }
        let g = painter.layout(DISEGNA_LINK.to_string(), Carattere::Testo.font(&ctx, 14.0), tema::FOGLIO, zona.width() - 48.0);
        let fondo = Rect::from_center_size(pos2(zona.center().x, zona.top() + 28.0), g.size() + vec2(24.0, 14.0));
        painter.rect_filled(fondo, 8.0, tema::INCHIOSTRO);
        painter.galley(fondo.min + vec2(12.0, 7.0), g, tema::FOGLIO);
        let sotto = Rect::from_center_size(pos2(zona.center().x, zona.bottom() - 40.0), vec2(1.0, 1.0));
        if BarraContesto::new().azione(icone::CLOSE, ANNULLA_OSCURA).mostra_in_primo_piano(&ctx, Id::new("barra_link"), sotto, zona).is_some() {
            diagnosi("pdf: link lasciato stare");
            p.modifica.link = None;
        }
        if apri {
            self.fogli_pdf.link.apri();
        }
    }

    /// La barra scura sopra il link scelto: Cambia e Togli.
    fn mostra_barra_link(&mut self, ctx: &egui::Context, zona: Rect) {
        let Some(p) = self.pdf.as_ref() else { return };
        if p.compila.scrittura.is_some() || self.fogli_pdf.link.aperto() {
            return;
        }
        let Some(c) = p.modifica.link_scelto.and_then(|(pagina, indice)| link_di(&p.motore.stato().collegamenti, pagina, indice)) else { return };
        let Some(r) = p.lettore.sullo_schermo(c.pagina, c.rettangolo) else { return };
        let scelta = BarraContesto::new().azione(icone::EDIT, CAMBIA).azione(icone::LINK_OFF, TOGLI).mostra_in_primo_piano(
            ctx,
            Id::new("link_scelto"),
            r.expand(6.0),
            zona,
        );
        let Some(p) = self.pdf.as_mut() else { return };
        match scelta {
            Some(0) => {
                let (a_indirizzo, pagina, indirizzo) = match &c.destinazione {
                    Destinazione::Pagina { pagina, .. } => (false, *pagina, String::new()),
                    Destinazione::Indirizzo(i) => (true, p.lettore.pagina_corrente(), i.clone()),
                    Destinazione::Altro => (false, p.lettore.pagina_corrente(), String::new()),
                };
                p.modifica.link = Some(NuovoLink { cambia: Some((c.pagina, c.indice)), a_indirizzo, pagina, indirizzo, ..Default::default() });
                self.fogli_pdf.link.apri();
            }
            Some(_) => {
                diagnosi("pdf: link tolto");
                p.modifica.link_scelto = None;
                self.modifica_pdf(Modifica::togli_link(&c));
            }
            None => {}
        }
    }

    /// Il foglio di dove porta un link: una pagina (coi passi) o un indirizzo (scritto con la
    /// tastiera), poi «Metti il link» (o «Cambia il link» e «Togli il link»).
    fn mostra_foglio_link(&mut self, ctx: &egui::Context) {
        let Some(p) = self.pdf.as_ref() else {
            self.fogli_pdf.link.mostra(ctx, LINK, |_| ());
            return;
        };
        let pagine = p.lettore.pagine();
        let Some(l) = p.modifica.link.as_ref() else {
            self.fogli_pdf.link.mostra(ctx, LINK, |_| ());
            return;
        };
        let (a_indirizzo, pagina, indirizzo, cambia) = (l.a_indirizzo, l.pagina, l.indirizzo.clone(), l.cambia.is_some());
        #[derive(Clone, Copy)]
        enum Scelta {
            Pagina,
            Indirizzo,
            Passo(Verso),
            Scrivi,
            Metti,
            Togli,
        }
        let r = self
            .fogli_pdf
            .link
            .mostra(ctx, LINK, |ui| {
                let mut r = None;
                etichetta_sezione(ui, "Porta a");
                ui.horizontal_wrapped(|ui| {
                    if ui.add(Chip::new(UNA_PAGINA).acceso(!a_indirizzo)).clicked() {
                        r = Some(Scelta::Pagina);
                    }
                    if ui.add(Chip::new(UN_INDIRIZZO).acceso(a_indirizzo)).clicked() {
                        r = Some(Scelta::Indirizzo);
                    }
                });
                ui.add_space(12.0);
                if a_indirizzo {
                    let nome = if indirizzo.is_empty() { SCRIVI_INDIRIZZO } else { indirizzo.as_str() };
                    if ui.add(Chip::new(nome).icona(icone::EDIT).acceso(!indirizzo.is_empty())).clicked() {
                        r = Some(Scelta::Scrivi);
                    }
                } else {
                    let valore = format!("Pagina {} di {pagine}", pagina + 1);
                    if let Some(v) =
                        Passo::new(&valore, "Pagina prima", "Pagina dopo").meno_spento(pagina == 0).piu_spento(pagina + 1 >= pagine).mostra(ui)
                    {
                        r = Some(Scelta::Passo(v));
                    }
                }
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    let pronto = !a_indirizzo || !indirizzo.trim().is_empty();
                    if pronto && ui.add(Pulsante::new(if cambia { CAMBIA_LINK } else { METTI_LINK }, Peso::Principale)).clicked() {
                        r = Some(Scelta::Metti);
                    }
                    if cambia && ui.add(Pulsante::new(TOGLI_LINK, Peso::Testo)).clicked() {
                        r = Some(Scelta::Togli);
                    }
                });
                ui.add_space(12.0);
                r
            })
            .flatten();
        // Chiuso senza mettere il link (non per scrivere l'indirizzo): il link se ne va.
        let scrive = self.pdf.as_ref().and_then(|p| p.compila.scrittura.as_ref()).is_some_and(|s| s.bersaglio == Bersaglio::Indirizzo);
        if r.is_none() && !self.fogli_pdf.link.aperto() && !scrive {
            if let Some(p) = self.pdf.as_mut()
                && p.modifica.link.as_ref().is_some_and(|l| l.riquadro.is_some() || l.cambia.is_some())
            {
                p.modifica.link = None;
            }
            return;
        }
        let Some(r) = r else { return };
        let Some(p) = self.pdf.as_mut() else { return };
        let Some(l) = p.modifica.link.as_mut() else { return };
        match r {
            Scelta::Pagina => l.a_indirizzo = false,
            Scelta::Indirizzo => l.a_indirizzo = true,
            Scelta::Passo(Verso::Meno) => l.pagina = l.pagina.saturating_sub(1),
            Scelta::Passo(Verso::Piu) => l.pagina = (l.pagina + 1).min(pagine.saturating_sub(1)),
            Scelta::Scrivi => {
                let testo = l.indirizzo.clone();
                self.fogli_pdf.link.chiudi();
                self.apri_scrittura_pdf(Bersaglio::Indirizzo, &testo, false);
            }
            Scelta::Metti => {
                let porta = if l.a_indirizzo { Porta::Indirizzo(l.indirizzo.clone()) } else { Porta::Pagina(l.pagina) };
                let m = match (l.cambia, l.riquadro) {
                    (Some((pagina, indice)), _) => {
                        link_di(&p.motore.stato().collegamenti, pagina, indice).and_then(|c| Modifica::cambia_link(&c, &porta))
                    }
                    (None, Some((pagina, r))) => p.motore.stato().geometrie.get(pagina).cloned().and_then(|i| Modifica::link(pagina, &i, r, &porta)),
                    (None, None) => None,
                };
                let cambiato = l.cambia.is_some();
                p.modifica.link = None;
                self.fogli_pdf.link.chiudi();
                if let Some(m) = m {
                    diagnosi(format!(
                        "pdf: link {} a {}",
                        if cambiato { "cambiato" } else { "messo" },
                        if matches!(porta, Porta::Pagina(_)) { "una pagina" } else { "un indirizzo" }
                    ));
                    self.modifica_pdf(m);
                }
            }
            Scelta::Togli => {
                let c = l.cambia.and_then(|(pagina, indice)| link_di(&p.motore.stato().collegamenti, pagina, indice));
                p.modifica.link = None;
                p.modifica.link_scelto = None;
                self.fogli_pdf.link.chiudi();
                if let Some(c) = c {
                    diagnosi("pdf: link tolto");
                    self.modifica_pdf(Modifica::togli_link(&c));
                }
            }
        }
    }

    /// Indietro nel modo Modifica: l'avviso del carattere, la conferma di Oscura, Oscura, la
    /// proposta, lo strumento che aspetta, l'oggetto scelto, i fogli.
    pub(crate) fn indietro_modifica_pdf(&mut self) -> bool {
        if self.lascia_il_carattere_pdf() {
            return true;
        }
        if self.fogli_pdf.immagine.indietro()
            || self.fogli_pdf.testo.indietro()
            || self.fogli_pdf.filigrana.indietro()
            || self.fogli_pdf.numeri.indietro()
            || self.fogli_pdf.link.indietro()
        {
            diagnosi("indietro: foglio della modifica chiuso");
            return true;
        }
        let Some(p) = self.pdf.as_mut() else { return false };
        let m = &mut p.modifica;
        if let Some(o) = m.oscura.as_mut() {
            if o.conferma {
                o.conferma = false;
            } else {
                m.oscura = None;
            }
            diagnosi("indietro: oscura chiuso");
            return true;
        }
        if m.forme.take().is_some() | m.link.take().is_some() | m.link_scelto.take().is_some() {
            diagnosi("indietro: forme o link chiusi");
            return true;
        }
        if m.proposta.take().is_some() | m.attesa.take().is_some() | m.scelto.take().is_some() {
            m.trascina = None;
            diagnosi("indietro: modifica chiusa");
            return true;
        }
        false
    }
}

/// Il testo che si scrive sulla pagina, nel modo Modifica: un paragrafo del PDF o della
/// scansione, un testo aggiunto, un testo nuovo.
pub(crate) struct PostoSullaPagina {
    /// Dove comincia e quanto è grande il testo di prima, sullo schermo.
    pub(crate) riquadro: Rect,
    /// La misura del carattere (dp).
    pub(crate) misura: f32,
    /// Quanto può andare largo prima di andare a capo (dp).
    pub(crate) largo: f32,
    pub(crate) colore: Color32,
    /// Per un paragrafo del PDF: la sua larghezza (dp) e quante volte si può allargare prima del
    /// bordo della pagina. La presa sul bordo destro del riquadro lo allarga.
    pub(crate) allargabile: Option<(f32, f32)>,
}

/// Dove si scrive sulla pagina, se la scrittura è di un testo della pagina nel modo Modifica. La
/// misura di un paragrafo del PDF è quella che fa stare la sua riga più lunga nella sua larghezza,
/// non più alta di una riga: viene giusta anche per il testo di una scansione, che dell'OCR ha
/// misure strane (la 0.8.7 lo disegnava enorme).
pub(crate) fn posto_sulla_pagina(p: &crate::schermo_pdf::PdfAperto, ctx: &egui::Context, pagine: Rect) -> Option<PostoSullaPagina> {
    if p.modo != ModoPdf::Modifica {
        return None;
    }
    let s = p.compila.scrittura.as_ref()?;
    match s.bersaglio {
        Bersaglio::Oggetto(c) => {
            let o = p.modifica.oggetto(c)?;
            let r = p.lettore.sullo_schermo(o.pagina, o.rettangolo)?;
            let k = p.lettore.dp_per_pt(o.pagina).max(1e-3);
            let righe: Vec<&str> = o.righe.lines().filter(|r| !r.trim().is_empty()).collect();
            let quante = righe.len().max(1);
            let (misura, colore) = match o.genere {
                GenereOggetto::TestoAggiunto { misura, colore } => (misura * k, Color32::from_rgb((colore[0] * 255.0) as u8, (colore[1] * 255.0) as u8, (colore[2] * 255.0) as u8)),
                GenereOggetto::Testo | GenereOggetto::Scansione => {
                    let alta = r.height() / quante as f32 * 0.85;
                    let prova = 20.0;
                    let piu_larga = righe
                        .iter()
                        .map(|t| ctx.fonts_mut(|f| f.layout_no_wrap(t.to_string(), Carattere::Testo.font(ctx, prova), Color32::PLACEHOLDER).size().x))
                        .fold(0.0f32, f32::max);
                    let larga = if piu_larga > 0.0 { prova * r.width() / piu_larga } else { alta };
                    (alta.min(larga), tema::INCHIOSTRO)
                }
                _ => return None,
            };
            // Un paragrafo del PDF va a capo dove ci andrà nel PDF, nella sua larghezza per quanto
            // è allargato: una riga sola che andava avanti fino al bordo dello schermo, nel PDF
            // andava a capo sopra la riga sotto (segnalazione dell'autore, 3/11). Gli altri: una riga sola va
            // avanti fino al bordo, un paragrafo va a capo nella sua larghezza.
            let (largo, allargabile) = if o.genere == GenereOggetto::Testo {
                let base = r.width() * 1.02;
                let info = p.motore.stato().geometrie.get(o.pagina).cloned();
                let pagina = info.and_then(|i| p.lettore.sullo_schermo(o.pagina, [0.0, 0.0, i.width, i.height]));
                let massimo = pagina.map_or(1.0, |pg| ((pg.right() - 4.0 - r.left()) / base.max(1.0)).max(1.0));
                let k = p.modifica.allargata.unwrap_or(1.0).clamp(0.3, massimo);
                (base * k, Some((base, massimo)))
            } else if quante == 1 {
                ((pagine.right() - r.left() - tema::PULSANTE - 16.0).max(r.width()), None)
            } else {
                (r.width() * 1.02, None)
            };
            Some(PostoSullaPagina { riquadro: r, misura: misura.clamp(8.0, 120.0), largo, colore, allargabile })
        }
        Bersaglio::TestoNuovo { pagina, punto } => {
            let a = p.lettore.sullo_schermo(pagina, [punto[0], punto[1], punto[0], punto[1]])?;
            let k = p.lettore.dp_per_pt(pagina).max(1e-3);
            let colore = COLORI[p.modifica.colore].0;
            let colore = Color32::from_rgb((colore[0] * 255.0) as u8, (colore[1] * 255.0) as u8, (colore[2] * 255.0) as u8);
            let misura = (p.modifica.misura() * k).max(6.0);
            Some(PostoSullaPagina {
                riquadro: Rect::from_min_size(a.min, vec2(0.0, misura)),
                misura,
                largo: (pagine.right() - a.left() - tema::PULSANTE - 16.0).max(80.0),
                colore,
                allargabile: None,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn il_tocco_prende_l_aggiunta_sopra_e_poi_il_piu_vicino() {
        let pdf = tascabile_pdf::Pdf::apri(
            tascabile_pdf::prova::pdf(&[tascabile_pdf::prova::Pagina {
                larghezza: 595.0,
                altezza: 842.0,
                rotazione: 0,
                contenuto: "BT /F1 24 Tf 60 700 Td (Pagina di prova) Tj ET\nBT /F1 12 Tf 60 500 Td (Seconda riga) Tj ET",
            }]),
            None,
        )
        .unwrap();
        let oggetti = pdf.oggetti(0);
        assert_eq!(oggetti.len(), 2);
        let primo = oggetti.iter().find(|o| o.testo == "Pagina di prova").unwrap();
        let [x0, y0, x1, y1] = primo.rettangolo;
        let centro = pos2((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        assert_eq!(oggetto_sotto(&oggetti, centro, 4.0).map(|o| o.testo.as_str()), Some("Pagina di prova"));
        assert_eq!(oggetto_sotto(&oggetti, pos2(x1 + 3.0, centro.y), 4.0).map(|o| o.testo.as_str()), Some("Pagina di prova"));
        assert!(oggetto_sotto(&oggetti, pos2(500.0, 800.0), 4.0).is_none());
        // Ingrandire: un paragrafo si allarga soltanto, un'immagine tiene le proporzioni.
        let r = Rect::from_min_size(pos2(10.0, 10.0), vec2(100.0, 50.0));
        assert_eq!(ingrandito(r, vec2(50.0, 0.0), true).size(), vec2(150.0, 75.0));
        assert_eq!(ingrandito(r, vec2(50.0, 30.0), false).size(), vec2(150.0, 50.0));
        assert_eq!(nome_dell_oggetto(primo), TESTO_DEL_PDF);
    }
}
