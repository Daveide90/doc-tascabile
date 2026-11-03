//! Il campo di testo con la tastiera vera: quando ha il fuoco riceve lui gli stati della
//! tastiera al posto del documento. Chi lo usa (l'app) tiene il fuoco e smista gli stati.

use egui::{Color32, Rect, Response, Sense, Stroke, StrokeKind, Ui, vec2};

use crate::icone;
use crate::pulsanti::PulsanteIcona;
use crate::tema::{self, Carattere};

/// Testo e selezione da dare alla tastiera, con la selezione in unità UTF-16 come la vuole
/// Android, e la versione dello stato.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatoCampo {
    pub testo: String,
    pub sel: (i32, i32),
    pub versione: i64,
}

/// Com'è andato uno stato della tastiera dato al campo.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ricevuto {
    /// Il testo del campo è cambiato.
    pub cambiato: bool,
    /// La tastiera ha mandato un a capo (il tasto Invio): il campo non lo tiene, chi lo usa ne fa
    /// la sua azione (Trova: il risultato dopo).
    pub invio: bool,
    /// Lo stato da rimandare alla tastiera, quando il campo non ha tenuto tutto quello che ha
    /// mandato (l'a capo).
    pub stato: Option<StatoCampo>,
    /// Lo stato è stato scartato: senza fuoco, o costruito su un testo più vecchio.
    pub scartato: bool,
}

/// Un campo di testo di una riga (Trova, il nome di un file, la password di un PDF).
#[derive(Default)]
pub struct CampoTesto {
    testo: String,
    /// La selezione in byte del testo.
    sel: (usize, usize),
    /// La versione dell'ultimo stato dato alla tastiera.
    versione: i64,
    fuoco: bool,
    /// Tiene solo le cifre (il numero di una pagina).
    cifre: bool,
    /// Una password: sullo schermo punti al posto delle lettere (salvo chiedere di vederla), e il
    /// testo mai nell'accessibilità né in `{:?}`.
    segreto: bool,
    /// Gli a capo restano nel testo e non sono Invio (un campo a più righe di un modulo, una
    /// nota): si conferma con «Fatto».
    a_piu_righe: bool,
}

/// Senza il testo, che può essere una password o una parola cercata.
impl std::fmt::Debug for CampoTesto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CampoTesto")
            .field("caratteri", &self.testo.chars().count())
            .field("versione", &self.versione)
            .field("fuoco", &self.fuoco)
            .field("cifre", &self.cifre)
            .field("segreto", &self.segreto)
            .finish()
    }
}

/// Il punto che prende il posto di ogni lettera di una password.
pub const PUNTO_SEGRETO: char = '\u{2022}';

fn utf16_a_byte(s: &str, u: usize) -> usize {
    let mut n = 0;
    for (i, c) in s.char_indices() {
        if n >= u {
            return i;
        }
        n += c.len_utf16();
    }
    s.len()
}

fn byte_a_utf16(s: &str, b: usize) -> usize {
    s.get(..b.min(s.len())).map_or(0, |p| p.encode_utf16().count())
}

impl CampoTesto {
    pub fn new() -> Self {
        Self::default()
    }

    /// Un campo per un numero: tiene solo le cifre da 0 a 9 e rimanda alla tastiera il testo
    /// senza il resto.
    pub fn solo_cifre() -> Self {
        Self { cifre: true, ..Self::default() }
    }

    /// Un campo per una password: vedi [`CampoTesto::mostra_segreto`].
    pub fn segreto() -> Self {
        Self { segreto: true, ..Self::default() }
    }

    /// Un campo a più righe: vedi [`CampoTesto::ricevi`].
    pub fn a_piu_righe() -> Self {
        Self { a_piu_righe: true, ..Self::default() }
    }

    /// Lo stesso campo con dentro `testo` e il cursore in fondo: un valore da cambiare (il campo
    /// di un modulo PDF già compilato).
    pub fn con_testo(mut self, testo: &str) -> Self {
        self.testo = testo.to_string();
        self.sel = (self.testo.len(), self.testo.len());
        self
    }

    /// Il campo tiene una password.
    pub fn e_segreto(&self) -> bool {
        self.segreto
    }

    pub fn testo(&self) -> &str {
        &self.testo
    }

    pub fn ha_il_fuoco(&self) -> bool {
        self.fuoco
    }

    /// La versione dell'ultimo stato dato alla tastiera: il documento, quando riprende il fuoco,
    /// deve partire da qui, così uno stato del campo in ritardo non entra nel documento.
    pub fn versione(&self) -> i64 {
        self.versione
    }

    /// Il campo prende il fuoco: lo stato da dare alla tastiera, con una versione più nuova di
    /// `dopo` (l'ultima data alla tastiera da chi l'aveva prima) e di quelle del campo.
    pub fn prendi_fuoco(&mut self, dopo: i64) -> StatoCampo {
        self.fuoco = true;
        self.versione = self.versione.max(dopo) + 1;
        self.stato()
    }

    /// Il fuoco passa ad altri: gli stati che arrivano dopo non sono più del campo.
    pub fn lascia_fuoco(&mut self) {
        self.fuoco = false;
    }

    fn stato(&self) -> StatoCampo {
        let (a, b) = self.sel;
        StatoCampo {
            testo: self.testo.clone(),
            sel: (byte_a_utf16(&self.testo, a) as i32, byte_a_utf16(&self.testo, b) as i32),
            versione: self.versione,
        }
    }

    /// Uno stato della tastiera. Si scarta senza il fuoco o se è costruito su un testo più
    /// vecchio dell'ultimo dato alla tastiera. Di solito il campo è di una riga: gli a capo escono
    /// dal testo, con lo stato da rimandare alla tastiera, e uno nuovo, battuto ora, è il tasto
    /// Invio e torna come `invio`. Quelli che il campo aveva già (un valore su più righe di un
    /// altro programma) non sono Invio. Un campo [`a_piu_righe`](Self::a_piu_righe) li tiene
    /// tutti, e Invio non c'è.
    pub fn ricevi(&mut self, testo: &str, sel: (i32, i32), versione: i64) -> Ricevuto {
        if !self.fuoco || versione < self.versione {
            return Ricevuto { scartato: true, ..Ricevuto::default() };
        }
        let a = utf16_a_byte(testo, sel.0.max(0) as usize);
        let b = utf16_a_byte(testo, sel.1.max(0) as usize);
        let a_capo = |t: &str| t.chars().filter(|c| matches!(c, '\n' | '\r')).count();
        let invio = !self.a_piu_righe && a_capo(testo) > a_capo(&self.testo);
        // Fuori gli a capo (in un campo di una riga) e, in un campo di cifre, tutto quello che
        // non è una cifra.
        let via = |c: char| (!self.a_piu_righe && matches!(c, '\n' | '\r')) || (self.cifre && !c.is_ascii_digit());
        let tolti = testo.chars().any(via);
        // Le posizioni dopo i caratteri tolti arretrano dei loro byte.
        let senza = |p: usize| p - testo[..p].chars().filter(|c| via(*c)).map(char::len_utf8).sum::<usize>();
        let nuovo: String = testo.chars().filter(|c| !via(*c)).collect();
        let (a, b) = (senza(a), senza(b));
        self.sel = (a.min(b), a.max(b));
        let cambiato = nuovo != self.testo;
        self.testo = nuovo;
        let stato = tolti.then(|| {
            self.versione += 1;
            self.stato()
        });
        Ricevuto { cambiato, invio, stato, scartato: false }
    }

    /// La selezione, in byte del testo: inizio e fine. Il cursore sta alla fine.
    pub fn selezione(&self) -> (usize, usize) {
        self.sel
    }

    /// Il cursore al byte `b` del testo (all'inizio della lettera, se cade dentro): dove il dito
    /// ha toccato il testo. Con il fuoco e se il cursore si è mosso, lo stato nuovo da dare alla
    /// tastiera, che scrive da lì.
    pub fn metti_cursore(&mut self, b: usize) -> Option<StatoCampo> {
        let mut b = b.min(self.testo.len());
        while !self.testo.is_char_boundary(b) {
            b -= 1;
        }
        if self.sel == (b, b) {
            return None;
        }
        self.sel = (b, b);
        if !self.fuoco {
            return None;
        }
        self.versione += 1;
        Some(self.stato())
    }

    /// Svuota il campo; con il fuoco, lo stato nuovo da dare alla tastiera.
    pub fn svuota(&mut self) -> Option<StatoCampo> {
        self.testo.clear();
        self.sel = (0, 0);
        if !self.fuoco {
            return None;
        }
        self.versione += 1;
        Some(self.stato())
    }

    /// Disegna il campo, largo quanto lo spazio che resta e alto 48 dp, col testo e, con il
    /// fuoco, il cursore; `vuoto` è il testo grigio da mostrare quando non c'è niente. Un tocco
    /// è un `clicked` della risposta: chi lo usa dà il fuoco al campo.
    pub fn mostra(&self, ui: &mut Ui, vuoto: &str) -> Response {
        let (rect, risposta) = ui.allocate_exact_size(vec2(ui.available_width().max(48.0), 48.0), Sense::click());
        let v = ui.visuals();
        let bordo = if self.fuoco { v.selection.stroke } else { v.widgets.inactive.bg_stroke };
        ui.painter().rect(rect, 4.0, v.extreme_bg_color, Stroke::new(bordo.width.max(1.0), bordo.color), StrokeKind::Inside);
        let font = egui::TextStyle::Body.resolve(ui.style());
        let (testo, vuoto_colore, cursore) = (v.text_color(), v.weak_text_color(), v.selection.stroke.color);
        self.dipingi_testo(ui, rect.shrink2(vec2(10.0, 0.0)), font, [testo, vuoto_colore, cursore], vuoto);
        self.info(&risposta, vuoto);
        risposta
    }

    /// Il campo della ricerca della Home (`.cerca`): una pillola bianca alta 52 dp con l'ombra, la
    /// lente grigia a sinistra e, quando c'è del testo, la X per svuotarlo. La X non conta come
    /// tocco sul campo: chi lo usa, se `svuota`, chiama [`CampoTesto::svuota`].
    pub fn mostra_ricerca(&self, ui: &mut Ui, vuoto: &str) -> RispostaRicerca {
        let (rect, campo) = ui.allocate_exact_size(vec2(ui.available_width().max(52.0), 52.0), Sense::click());
        let sinistra = pillola(ui, rect, 18.0);
        let con_x = !self.testo.is_empty();
        let destra = if con_x { rect.right() - 4.0 - tema::PULSANTE } else { rect.right() - 18.0 };
        let dentro = Rect::from_min_max(egui::pos2(sinistra, rect.top()), egui::pos2(destra, rect.bottom()));
        self.dipingi_testo(ui, dentro, Carattere::Testo.font(ui.ctx(), 16.0), [tema::INCHIOSTRO, tema::MATITA, tema::BLU], vuoto);
        self.info(&campo, vuoto);
        let svuota = con_x && {
            let posto = Rect::from_min_size(egui::pos2(destra, rect.center().y - tema::PULSANTE / 2.0), egui::Vec2::splat(tema::PULSANTE));
            PulsanteIcona::new(icone::CLOSE, "Svuota la ricerca").colore(tema::MATITA).mostra_in(ui, posto, (tema::PULSANTE / 2.0) as u8).clicked()
        };
        RispostaRicerca { campo, svuota }
    }

    /// Il campo di Trova nella barra in alto: la pillola della ricerca alta 48 dp, con la lente e,
    /// a destra, `conteggio` in grigio («2 di 5», «Nessun risultato»; vuoto, niente). Un tocco è
    /// un `clicked` della risposta, come in [`CampoTesto::mostra`].
    pub fn mostra_trova(&self, ui: &mut Ui, vuoto: &str, conteggio: &str) -> Response {
        let (rect, campo) = ui.allocate_exact_size(vec2(ui.available_width().max(tema::PULSANTE), tema::PULSANTE), Sense::click());
        let sinistra = pillola(ui, rect, 14.0);
        let ctx = ui.ctx().clone();
        let mut destra = rect.right() - 16.0;
        if !conteggio.is_empty() {
            let g = ui.painter().layout_no_wrap(conteggio.to_owned(), Carattere::Testo.font(&ctx, 14.0), tema::MATITA);
            let posto = Rect::from_min_size(egui::pos2(destra - g.size().x, rect.center().y - g.size().y / 2.0), g.size());
            ui.painter().galley(posto.min, g, tema::MATITA);
            let r = ui.interact(posto, ui.id().with(("conteggio", rect.min.x as i32)), Sense::hover());
            r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, conteggio));
            destra = posto.left() - 8.0;
        }
        let dentro = Rect::from_min_max(egui::pos2(sinistra, rect.top()), egui::pos2(destra.max(sinistra), rect.bottom()));
        self.dipingi_testo(ui, dentro, Carattere::Testo.font(&ctx, 16.0), [tema::INCHIOSTRO, tema::MATITA, tema::BLU], vuoto);
        self.info(&campo, vuoto);
        campo
    }

    /// Il campo «Sostituisci con» sotto Trova: la stessa pillola di [`CampoTesto::mostra_trova`],
    /// con la matita al posto della lente e senza conteggio. Un tocco è un `clicked` della risposta.
    pub fn mostra_sostituisci(&self, ui: &mut Ui, vuoto: &str) -> Response {
        let (rect, campo) = ui.allocate_exact_size(vec2(ui.available_width().max(tema::PULSANTE), tema::PULSANTE), Sense::click());
        let sinistra = pillola_con(ui, rect, 14.0, icone::EDIT);
        let dentro = Rect::from_min_max(egui::pos2(sinistra, rect.top()), egui::pos2((rect.right() - 16.0).max(sinistra), rect.bottom()));
        self.dipingi_testo(ui, dentro, Carattere::Testo.font(ui.ctx(), 16.0), [tema::INCHIOSTRO, tema::MATITA, tema::BLU], vuoto);
        self.info(&campo, vuoto);
        campo
    }

    /// Il campo di una password (`CampoTesto::segreto`), alto 52 dp, bianco con gli angoli tondi e
    /// il bordo (blu col fuoco): punti al posto delle lettere, o le lettere con `visibile`; a destra
    /// l'occhio che le mostra o le nasconde. Un tocco sul campo è `campo.clicked()`, l'occhio è
    /// `occhio`.
    pub fn mostra_segreto(&self, ui: &mut Ui, vuoto: &str, visibile: bool) -> RispostaSegreto {
        let (rect, campo) = ui.allocate_exact_size(vec2(ui.available_width().max(tema::PULSANTE), 52.0), Sense::click());
        let bordo = if self.fuoco { Stroke::new(2.0, tema::BLU) } else { Stroke::new(1.0, tema::LINEETTA) };
        ui.painter().rect(rect, 12.0, tema::FOGLIO, bordo, StrokeKind::Inside);
        let occhio = Rect::from_min_size(
            egui::pos2(rect.right() - 2.0 - tema::PULSANTE, rect.center().y - tema::PULSANTE / 2.0),
            egui::Vec2::splat(tema::PULSANTE),
        );
        let dentro = Rect::from_min_max(egui::pos2(rect.left() + 16.0, rect.top()), egui::pos2(occhio.left(), rect.bottom()));
        let mostrato = if visibile { self.testo.clone() } else { self.testo.chars().map(|_| PUNTO_SEGRETO).collect() };
        // Il cursore dopo lo stesso numero di caratteri anche fra i punti.
        let prima = self.testo.get(..self.sel.1).unwrap_or(&self.testo).chars().count();
        let cursore = mostrato.char_indices().nth(prima).map_or(mostrato.len(), |(i, _)| i);
        let colori = [tema::INCHIOSTRO, tema::MATITA, tema::BLU];
        self.dipingi_come(ui, dentro, Carattere::Testo.font(ui.ctx(), 17.0), colori, vuoto, &mostrato, cursore);
        self.info(&campo, vuoto);
        let (icona, etichetta) = if visibile { (icone::VISIBILITY_OFF, NASCONDI) } else { (icone::VISIBILITY, MOSTRA) };
        let occhio = PulsanteIcona::new(icona, etichetta).colore(tema::MATITA).mostra_in(ui, occhio, (tema::PULSANTE / 2.0) as u8).clicked();
        RispostaSegreto { campo, occhio }
    }

    /// Il testo (o `vuoto`, se non c'è) in `dentro`, centrato in altezza, e col fuoco il cursore.
    /// `colori`: testo, testo vuoto, cursore.
    fn dipingi_testo(&self, ui: &Ui, dentro: Rect, font: egui::FontId, colori: [Color32; 3], vuoto: &str) {
        self.dipingi_come(ui, dentro, font, colori, vuoto, &self.testo, self.sel.1);
    }

    /// Come `dipingi_testo`, mostrando `mostrato` (il testo, o i suoi punti) col cursore al byte
    /// `cursore` di `mostrato`.
    #[allow(clippy::too_many_arguments)]
    fn dipingi_come(&self, ui: &Ui, dentro: Rect, font: egui::FontId, colori: [Color32; 3], vuoto: &str, mostrato: &str, cursore: usize) {
        let [colore_testo, colore_vuoto, colore_cursore] = colori;
        let (testo, colore) = if mostrato.is_empty() { (vuoto, colore_vuoto) } else { (mostrato, colore_testo) };
        let galley = ui.painter().layout_no_wrap(testo.to_string(), font.clone(), colore);
        // Il testo lungo scorre verso sinistra perché resti a vista la fine, dove si scrive.
        let largo = galley.size().x;
        let dx = if largo > dentro.width() { dentro.width() - largo } else { 0.0 };
        let origine = egui::pos2(dentro.min.x + dx, dentro.center().y - galley.size().y / 2.0);
        let painter = ui.painter_at(dentro.expand2(vec2(2.0, 0.0)));
        painter.galley(origine, galley.clone(), colore);
        if self.fuoco {
            let x = if mostrato.is_empty() {
                origine.x
            } else {
                let fine = mostrato.get(..cursore).unwrap_or(mostrato);
                origine.x + ui.painter().layout_no_wrap(fine.to_string(), font, Color32::PLACEHOLDER).size().x
            };
            let alto = galley.size().y.max(20.0);
            let r = Rect::from_min_max(egui::pos2(x - 1.0, dentro.center().y - alto / 2.0), egui::pos2(x + 1.0, dentro.center().y + alto / 2.0));
            painter.rect_filled(r, 0.0, colore_cursore);
        }
    }

    /// Per l'accessibilità (e le prove): il campo è un campo di testo con quello che contiene; una
    /// password coi punti.
    fn info(&self, risposta: &Response, vuoto: &str) {
        let etichetta = match () {
            _ if self.testo.is_empty() => vuoto.to_string(),
            _ if self.segreto => self.testo.chars().map(|_| PUNTO_SEGRETO).collect(),
            _ => self.testo.clone(),
        };
        risposta.widget_info(|| egui::WidgetInfo::text_edit(true, String::new(), etichetta.clone(), vuoto));
    }
}

/// La pillola bianca con l'ombra (`.cerca`) in `rect` e la lente grigia a `bordo` dp dal lato
/// sinistro; restituisce dove comincia il testo, 12 dp dopo la lente.
fn pillola(ui: &Ui, rect: Rect, bordo: f32) -> f32 {
    pillola_con(ui, rect, bordo, icone::SEARCH)
}

/// Come [`pillola`], con l'icona `icona` al posto della lente.
fn pillola_con(ui: &Ui, rect: Rect, bordo: f32, icona: &str) -> f32 {
    let raggio = egui::CornerRadius::same((rect.height() / 2.0) as u8);
    tema::dipingi_ombra(ui.painter(), rect, raggio, &tema::OMBRA);
    ui.painter().rect_filled(rect, raggio, tema::FOGLIO);
    let posto = egui::pos2(rect.left() + bordo + tema::ICONA / 2.0, rect.center().y);
    ui.painter().text(posto, egui::Align2::CENTER_CENTER, icona, Carattere::Icona.font(ui.ctx(), tema::ICONA), tema::MATITA);
    rect.left() + bordo + tema::ICONA + 12.0
}

/// Le etichette dell'occhio del campo di una password.
pub const MOSTRA: &str = "Mostra la password";
pub const NASCONDI: &str = "Nascondi la password";

/// Che cosa è stato toccato nel campo di una password.
#[derive(Debug)]
pub struct RispostaSegreto {
    /// Il campo: un tocco gli dà il fuoco.
    pub campo: Response,
    /// L'occhio: le lettere si mostrano o si nascondono.
    pub occhio: bool,
}

/// Che cosa è stato toccato nel campo della ricerca.
#[derive(Debug)]
pub struct RispostaRicerca {
    /// Il campo: un tocco gli dà il fuoco.
    pub campo: Response,
    /// La X: il testo va svuotato.
    pub svuota: bool,
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn prende_il_fuoco_con_una_versione_piu_nuova() {
        let mut c = CampoTesto::new();
        assert!(!c.ha_il_fuoco());
        let st = c.prendi_fuoco(7);
        assert_eq!(st, StatoCampo { testo: String::new(), sel: (0, 0), versione: 8 });
        assert!(c.ha_il_fuoco());
        // Una seconda volta, con chi c'era prima più indietro: la versione cresce lo stesso.
        c.lascia_fuoco();
        assert_eq!(c.prendi_fuoco(3).versione, 9);
    }

    #[test]
    fn col_testo_il_cursore_sta_in_fondo() {
        let mut c = CampoTesto::new().con_testo("Città");
        assert!(!c.ha_il_fuoco());
        // In unità UTF-16, come le vuole Android: cinque lettere.
        assert_eq!(c.prendi_fuoco(0), StatoCampo { testo: "Città".into(), sel: (5, 5), versione: 1 });
        let c = CampoTesto::segreto().con_testo("x");
        assert!(c.e_segreto() && c.testo() == "x");
    }

    #[test]
    fn riceve_le_lettere_solo_col_fuoco() {
        let mut c = CampoTesto::new();
        assert!(c.ricevi("ciao", (4, 4), 0).scartato, "senza fuoco non scrive");
        assert_eq!(c.testo(), "");
        let st = c.prendi_fuoco(0);
        assert!(c.ricevi("ciao", (4, 4), st.versione).cambiato);
        assert_eq!(c.testo(), "ciao");
        // Lo stesso testo, cursore spostato: niente di nuovo da cercare.
        assert_eq!(c.ricevi("ciao", (2, 2), st.versione), Ricevuto::default());
        c.lascia_fuoco();
        assert!(c.ricevi("ciao!", (5, 5), st.versione).scartato);
        assert_eq!(c.testo(), "ciao");
    }

    #[test]
    fn un_campo_a_piu_righe_tiene_gli_a_capo_e_non_ha_invio() {
        // Analisi del 27/10, critico 9: un valore su due righe, al primo tasto, perdeva gli a capo
        // («Allergia al latticePortatore di pacemaker.») e il campo si confermava da solo.
        let valore = "Allergia al lattice\nPortatore di pacemaker";
        let mut c = CampoTesto::a_piu_righe().con_testo(valore);
        let v = c.prendi_fuoco(0).versione;
        let dopo = format!("{valore}.");
        let fine = dopo.len() as i32;
        let r = c.ricevi(&dopo, (fine, fine), v);
        assert!(r.cambiato && !r.invio && r.stato.is_none(), "{r:?}");
        assert_eq!(c.testo(), dopo);
        // Un a capo battuto è una riga nuova.
        let tre = format!("{dopo}\nIn terapia");
        let fine = tre.len() as i32;
        assert!(!c.ricevi(&tre, (fine, fine), v).invio);
        assert_eq!(c.testo(), tre);
        // In un campo di una riga un a capo che c'era già non è Invio; uno battuto sì.
        let mut c = CampoTesto::new().con_testo("a\nb");
        let v = c.prendi_fuoco(0).versione;
        let r = c.ricevi("a\nbc", (4, 4), v);
        assert!(!r.invio, "{r:?}");
        let v = r.stato.expect("senza l'a capo, da rimandare alla tastiera").versione;
        assert!(c.ricevi("abc\n", (4, 4), v).invio);
    }

    #[test]
    fn il_cursore_va_dove_si_tocca() {
        let mut c = CampoTesto::new().con_testo("città bella");
        // Senza il fuoco si sposta, ma alla tastiera non va niente.
        assert_eq!(c.metti_cursore(0), None);
        assert_eq!(c.selezione(), (0, 0));
        let st = c.prendi_fuoco(4);
        // Dentro la «à» (due byte): all'inizio della lettera, in UTF-16 per la tastiera.
        let nuovo = c.metti_cursore(5).unwrap();
        assert_eq!(c.selezione(), (4, 4));
        assert_eq!((nuovo.testo.as_str(), nuovo.sel, nuovo.versione), ("città bella", (4, 4), st.versione + 1));
        // Dove già sta: niente da mandare. Oltre la fine: in fondo.
        assert_eq!(c.metti_cursore(4), None);
        assert_eq!(c.metti_cursore(100).unwrap().sel, (11, 11));
        // La tastiera scrive da lì: uno stato costruito sulla versione nuova passa.
        let r = c.ricevi("città bellaX", (12, 12), st.versione + 2);
        assert!(r.cambiato && !r.scartato);
    }

    #[test]
    fn uno_stato_vecchio_si_scarta() {
        let mut c = CampoTesto::new();
        let st = c.prendi_fuoco(4);
        assert!(c.ricevi("vecchio", (7, 7), st.versione - 1).scartato);
        assert_eq!(c.testo(), "");
    }

    #[test]
    fn una_riga_sola_e_selezione_in_utf16() {
        let mut c = CampoTesto::new();
        let v = c.prendi_fuoco(0).versione;
        // L'Invio battuto dopo l'emoji: esce dal testo, e la tastiera riceve il testo senza.
        let r = c.ricevi("è😀\nx", (4, 4), v);
        assert_eq!(c.testo(), "è😀x");
        assert!(r.cambiato && r.invio);
        let st = r.stato.unwrap();
        assert_eq!((st.testo.as_str(), st.sel), ("è😀x", (3, 3)));
        assert!(st.versione > v);
        let v = st.versione;
        let st = c.svuota().unwrap();
        assert_eq!(c.testo(), "");
        assert!(st.versione > v);
        c.ricevi("😀a", (3, 3), st.versione);
        c.lascia_fuoco();
        let st = c.prendi_fuoco(0);
        assert_eq!(st.sel, (3, 3), "il cursore dopo l'emoji (2 unità) e la a");
    }

    #[test]
    fn un_campo_di_cifre_tiene_solo_le_cifre() {
        let mut c = CampoTesto::solo_cifre();
        let v = c.prendi_fuoco(0).versione;
        // Le cifre passano come sono, senza stato da rimandare.
        let r = c.ricevi("12", (2, 2), v);
        assert!(r.cambiato && r.stato.is_none());
        // Una lettera e uno spazio escono, e la tastiera riceve il testo senza, col cursore giusto.
        let r = c.ricevi("1a2 3", (5, 5), v);
        assert_eq!(c.testo(), "123");
        assert!(r.cambiato && !r.invio);
        let st = r.stato.unwrap();
        assert_eq!((st.testo.as_str(), st.sel), ("123", (3, 3)));
        // L'Invio resta l'Invio.
        let r = c.ricevi("123\n", (4, 4), st.versione);
        assert!(r.invio && !r.cambiato);
        assert_eq!(c.testo(), "123");
        // Un campo normale tiene le lettere.
        let mut t = CampoTesto::new();
        let v = t.prendi_fuoco(0).versione;
        assert!(t.ricevi("1a", (2, 2), v).stato.is_none());
        assert_eq!(t.testo(), "1a");
    }

    #[test]
    fn si_vede_col_testo_e_si_tocca() {
        use egui_kittest::Harness;
        use egui_kittest::kittest::Queryable;
        let mut c = CampoTesto::new();
        let v = c.prendi_fuoco(0).versione;
        c.ricevi("parola", (6, 6), v);
        assert_eq!(c.testo(), "parola");
        let mut toccato = false;
        let mut h = Harness::new_ui(|ui| {
            if c.mostra(ui, "Cerca nel documento").clicked() {
                toccato = true;
            }
        });
        h.get_by_value("parola").click();
        h.run();
        drop(h);
        assert!(toccato);
    }

    /// Lo stato della prova della ricerca: il campo e che cosa è stato toccato.
    #[derive(Default)]
    struct Ricerca {
        campo: CampoTesto,
        toccato: u32,
        svuotato: u32,
    }

    fn ricerca(ui: &mut Ui, r: &mut Ricerca) {
        ui.add_space(20.0);
        let risposta = r.campo.mostra_ricerca(ui, "Cerca nei documenti");
        if risposta.campo.clicked() {
            r.toccato += 1;
        }
        if risposta.svuota {
            r.svuotato += 1;
        }
    }

    #[test]
    fn la_ricerca_e_una_pillola_bianca_con_la_lente() {
        use crate::prova::{banco, colore_del_testo, forme, rettangolo_del_testo};
        use crate::tema;
        use egui_kittest::kittest::Queryable;
        let mut h = banco(ricerca, Ricerca::default());
        let campo = h.get_by_value("Cerca nei documenti").rect();
        assert_eq!(campo.height(), 52.0);
        let tutte = forme(&h);
        let pillola = egui::CornerRadius::same(26);
        assert!(tutte.iter().any(|s| matches!(s, egui::Shape::Rect(r) if r.fill == tema::FOGLIO && r.rect == campo && r.corner_radius == pillola)));
        assert_eq!(tutte.iter().filter(|s| matches!(s, egui::Shape::Rect(r) if r.blur_width > 0.0)).count(), 2, "due strati d'ombra");
        // La lente grigia a 18 dp dal bordo, il testo vuoto grigio 12 dp dopo.
        assert_eq!(colore_del_testo(&h, crate::icone::SEARCH), Some(tema::MATITA));
        let lente = rettangolo_del_testo(&h, crate::icone::SEARCH).unwrap();
        let vuoto = rettangolo_del_testo(&h, "Cerca nei documenti").unwrap();
        assert!(lente.left() >= campo.left() + 18.0 && vuoto.left() >= campo.left() + 18.0 + 24.0 + 10.0, "{lente:?} {vuoto:?}");
        assert_eq!(colore_del_testo(&h, "Cerca nei documenti"), Some(tema::MATITA));
        assert!(h.query_by_label("Svuota la ricerca").is_none(), "vuota, niente X");
        h.get_by_value("Cerca nei documenti").click();
        h.run_steps(2);
        assert_eq!(h.state().toccato, 1);
    }

    fn trova(ui: &mut Ui, r: &mut Ricerca) {
        ui.add_space(8.0);
        let conteggio = if r.campo.testo().is_empty() { "" } else { "2 di 5" };
        if r.campo.mostra_trova(ui, "Trova nel documento", conteggio).clicked() {
            r.toccato += 1;
        }
    }

    #[test]
    fn il_campo_di_trova_e_una_pillola_col_conteggio_a_destra() {
        use crate::prova::{banco, colore_del_testo, forme, rettangolo_del_testo};
        use crate::tema;
        use egui_kittest::kittest::Queryable;
        let mut stato = Ricerca::default();
        let v = stato.campo.prendi_fuoco(0).versione;
        stato.campo.ricevi("letto", (5, 5), v);
        let mut h = banco(trova, stato);
        let campo = h.get_by_value("letto").rect();
        assert_eq!(campo.height(), tema::PULSANTE);
        let tutte = forme(&h);
        let pillola = egui::CornerRadius::same(24);
        assert!(tutte.iter().any(|s| matches!(s, egui::Shape::Rect(r) if r.fill == tema::FOGLIO && r.rect == campo && r.corner_radius == pillola)));
        assert!(tutte.iter().any(|s| matches!(s, egui::Shape::Rect(r) if r.blur_width > 0.0)), "senza ombra");
        assert_eq!(colore_del_testo(&h, crate::icone::SEARCH), Some(tema::MATITA));
        // Il conteggio grigio a destra, dentro la pillola, dopo il testo.
        let conteggio = h.get_by_label("2 di 5").rect();
        assert!(campo.contains_rect(conteggio), "{campo:?} {conteggio:?}");
        assert!((conteggio.right() - (campo.right() - 16.0)).abs() < 1.0, "{campo:?} {conteggio:?}");
        assert_eq!(colore_del_testo(&h, "2 di 5"), Some(tema::MATITA));
        assert!(rettangolo_del_testo(&h, "letto").unwrap().right() < conteggio.left());
        // Col fuoco il cursore blu dopo il testo.
        assert!(tutte.iter().any(|s| matches!(s, egui::Shape::Rect(r) if r.fill == tema::BLU && r.rect.width() == 2.0)));
        h.get_by_value("letto").click();
        h.run_steps(2);
        assert_eq!(h.state().toccato, 1);
        // Vuoto: il testo grigio e niente conteggio.
        let h = banco(trova, Ricerca::default());
        assert_eq!(colore_del_testo(&h, "Trova nel documento"), Some(tema::MATITA));
        assert!(h.query_by_label("2 di 5").is_none());
    }

    #[test]
    fn il_campo_sostituisci_e_una_pillola_con_la_matita() {
        use crate::prova::{banco, colore_del_testo, forme};
        use crate::tema;
        use egui_kittest::kittest::Queryable;
        fn sostituisci(ui: &mut Ui, r: &mut Ricerca) {
            ui.add_space(8.0);
            if r.campo.mostra_sostituisci(ui, "Sostituisci con").clicked() {
                r.toccato += 1;
            }
        }
        let mut h = banco(sostituisci, Ricerca::default());
        let campo = h.get_by_value("Sostituisci con").rect();
        assert_eq!(campo.height(), tema::PULSANTE);
        let pillola = egui::CornerRadius::same(24);
        assert!(
            forme(&h).iter().any(|s| matches!(s, egui::Shape::Rect(r) if r.fill == tema::FOGLIO && r.rect == campo && r.corner_radius == pillola))
        );
        assert_eq!(colore_del_testo(&h, crate::icone::EDIT), Some(tema::MATITA));
        assert_eq!(colore_del_testo(&h, crate::icone::SEARCH), None, "niente lente");
        assert_eq!(colore_del_testo(&h, "Sostituisci con"), Some(tema::MATITA));
        h.get_by_value("Sostituisci con").click();
        h.run_steps(2);
        assert_eq!(h.state().toccato, 1);
    }

    /// Lo stato della prova della password: il campo, se si vede e che cosa è stato toccato.
    #[derive(Default)]
    struct Password {
        campo: CampoTesto,
        visibile: bool,
        toccato: u32,
    }

    fn password(ui: &mut Ui, p: &mut Password) {
        ui.add_space(8.0);
        let r = p.campo.mostra_segreto(ui, "Password", p.visibile);
        if r.campo.clicked() {
            p.toccato += 1;
        }
        if r.occhio {
            p.visibile = !p.visibile;
        }
    }

    #[test]
    fn la_password_si_vede_a_punti_e_l_occhio_la_mostra() {
        use crate::prova::{banco, forme};
        use crate::tema;
        use egui_kittest::kittest::Queryable;
        let mut stato = Password { campo: CampoTesto::segreto(), ..Password::default() };
        assert!(stato.campo.e_segreto());
        let v = stato.campo.prendi_fuoco(0).versione;
        stato.campo.ricevi("Zanzibar7", (9, 9), v);
        // Mai le lettere, nemmeno in `{:?}`.
        assert!(!format!("{:?}", stato.campo).contains("Zanzibar"));
        let mut h = banco(password, stato);
        let punti = "\u{2022}".repeat(9);
        let testi: Vec<String> = forme(&h)
            .iter()
            .filter_map(|s| match s {
                egui::Shape::Text(t) => Some(t.galley.text().to_string()),
                _ => None,
            })
            .collect();
        assert!(testi.contains(&punti) && !testi.iter().any(|t| t.contains("Zanzibar")), "{testi:?}");
        // Nell'accessibilità i punti, col bordo blu del fuoco.
        let campo = h.get_by_value(&punti).rect();
        assert_eq!(campo.height(), 52.0);
        assert!(forme(&h).iter().any(|s| matches!(s, egui::Shape::Rect(r) if r.stroke.color == tema::BLU && r.rect == campo)));
        // L'occhio mostra le lettere, poi le nasconde di nuovo.
        h.get_by_label(MOSTRA).click();
        h.run_steps(2);
        assert!(h.state().visibile);
        assert!(forme(&h).iter().any(|s| matches!(s, egui::Shape::Text(t) if t.galley.text() == "Zanzibar7")));
        h.get_by_label(NASCONDI).click();
        h.run_steps(2);
        assert!(!h.state().visibile);
        assert_eq!(h.state().toccato, 0, "l'occhio non è un tocco sul campo");
        h.get_by_value(&punti).click();
        h.run_steps(2);
        assert_eq!(h.state().toccato, 1);
    }

    #[test]
    fn col_testo_la_ricerca_ha_la_x_per_svuotarla() {
        use crate::prova::{banco, colore_del_testo};
        use crate::tema;
        use egui_kittest::kittest::Queryable;
        let mut stato = Ricerca::default();
        let v = stato.campo.prendi_fuoco(0).versione;
        stato.campo.ricevi("turno", (5, 5), v);
        let mut h = banco(ricerca, stato);
        assert_eq!(colore_del_testo(&h, "turno"), Some(tema::INCHIOSTRO));
        let x = h.get_by_label("Svuota la ricerca");
        assert!(h.get_by_value("turno").rect().contains_rect(x.rect()));
        x.click();
        h.run_steps(2);
        assert_eq!((h.state().svuotato, h.state().toccato), (1, 0), "la X non è un tocco sul campo");
    }
}
