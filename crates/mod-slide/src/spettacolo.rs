//! La presentazione a schermo intero: dove si è con `ShowState` di DeckCraft, la slide disegnata
//! allo stato dei passi fatti (le animazioni a scatti: ogni passo subito alla sua fine), la
//! dissolvenza fra una slide e l'altra, il laser e la penna col dito, lo schermo nero, il tempo
//! dal primo «avanti». Le slide le disegna il thread di disegno, che prepara anche il passo dopo.

use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use std::time::Duration;

use deckcraft_anim::{ShowAction, ShowState};
use deckcraft_model::Presentation;
use egui::{Align2, Color32, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use tascabile_touch::tema::{self, Carattere};

use crate::apertura::{Immagine, Presentazione};
use crate::disegno::{Disegnatore, Richiesta, Uso, larghezza_entro};

/// La schermata dopo l'ultima slide.
pub const FINE: &str = "Fine della presentazione";
/// Sotto la fine, piccolo.
const TOCCA_PER_USCIRE: &str = "Tocca per uscire";
/// La fascia in fondo allo schermo che apre la barra dei comandi invece di andare avanti, in dp.
const FASCIA_DELLA_BARRA: f32 = 72.0;
/// Il tocco nel terzo sinistro dello schermo va indietro, nel resto avanti.
const PARTE_INDIETRO: f32 = 1.0 / 3.0;
/// Uno scorrimento orizzontale lungo almeno tanto (dp), più che verticale, cambia passo.
const SCORRIMENTO: f32 = 60.0;
/// Il dito fermo (entro tanto, in dp) per tanto (secondi) accende il laser.
const FERMO: f32 = 8.0;
const ATTESA_LASER: f64 = 0.45;
/// Il punto del laser e il suo alone, in dp.
const RAGGIO_LASER: f32 = 7.0;
const ALONE_LASER: f32 = 16.0;
/// Lo spessore della penna, in dp.
const PENNA: f32 = 3.5;
/// Il lato più lungo di una slide disegnata, in pixel, e al più quello delle texture della scheda
/// grafica (`max_texture_side`).
const LATO_MASSIMO: f32 = 4096.0;
/// Il lato più lungo dell'immagine per lo schermo esterno, in pixel: un proiettore 4K riceve una
/// slide Full HD, che allarga da sé.
const LATO_PROIETTORE: u32 = 1920;
/// La dissolvenza più lunga, in secondi: una transizione lenta del file non deve fermare chi parla.
const DISSOLVENZA_MASSIMA: f64 = 1.0;
const ATTESA: Duration = Duration::from_millis(16);

/// Quello che è successo nella presentazione in un frame.
#[derive(Debug, Default)]
pub struct EsitoSpettacolo {
    /// Si esce: Esc, un «avanti» sulla schermata della fine.
    pub esci: bool,
    /// Un tocco nella fascia in fondo: la barra dei comandi si apre o si chiude.
    pub barra: bool,
}

/// Una slide a un passo, larga tanti pixel: quello che si chiede al thread e si tiene in texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Fotogramma {
    indice: usize,
    passo: usize,
    larghezza: u32,
}

impl Fotogramma {
    fn richiesta(self) -> Richiesta {
        self.richiesta_per(Uso::Vista)
    }

    fn richiesta_per(self, uso: Uso) -> Richiesta {
        Richiesta { indice: self.indice, larghezza: self.larghezza, uso, passo: Some(self.passo) }
    }
}

/// Quello che deve vedere lo schermo esterno: una slide a un passo, o il nero (lo schermo nero, la
/// fine della presentazione).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Proiezione {
    Slide(Fotogramma),
    Nero,
}

/// La slide di prima che sfuma sotto quella nuova; il tempo parte quando la nuova è pronta.
struct Dissolvenza {
    vecchia: egui::TextureHandle,
    durata: f64,
    inizio: Option<f64>,
}

/// Il dito sullo schermo: dove è sceso e quando, dov'è adesso, se ha acceso il laser, se è sceso
/// nella fascia in fondo (che è della barra, anche con la penna).
struct Dito {
    inizio: Pos2,
    t0: f64,
    adesso: Pos2,
    laser: bool,
    fascia: bool,
}

/// Una presentazione a schermo intero.
pub struct Spettacolo {
    doc: Arc<Presentation>,
    stato: ShowState,
    disegnatore: Disegnatore,
    texture: HashMap<Fotogramma, egui::TextureHandle>,
    /// Il fotogramma sullo schermo, e quelli chiesti al thread e non arrivati.
    mostrato: Option<Fotogramma>,
    chieste: Vec<Richiesta>,
    dissolvenza: Option<Dissolvenza>,
    nero: bool,
    penna: bool,
    /// I tratti della penna sulla slide, in coordinate da 0 a 1; si cancellano a ogni passo.
    tratti: Vec<Vec<Pos2>>,
    dito: Option<Dito>,
    laser: Option<Pos2>,
    /// La zona dei comandi aperti sopra la presentazione: i tocchi che cominciano lì sono loro, e
    /// un tocco altrove li chiude invece di andare avanti.
    esclusa: Option<Rect>,
    /// Le zone di quello che sta sopra la presentazione senza essere i comandi (le note del
    /// relatore, un foglio): i tocchi che cominciano lì sono loro e basta.
    ignorate: Vec<Rect>,
    /// Lo schermo esterno (proiettore, televisore) in pixel, se c'è; quello che ha già avuto e
    /// l'immagine pronta da mandargli.
    proiettore: Option<(u32, u32)>,
    proiettato: Option<Proiezione>,
    per_il_proiettore: Option<(Proiezione, Immagine)>,
    /// Quando si è andati avanti la prima volta (`InputState::time`).
    partito: Option<f64>,
    /// Il posto della slide sullo schermo nell'ultimo frame, e la larghezza in pixel.
    posto: Rect,
    larghezza: u32,
    disegnate: usize,
    ms_totali: u64,
    ms_massimo: u32,
}

impl Spettacolo {
    /// La presentazione di `p` dalla slide `da` (anche nascosta: «da qui»), col suo thread di
    /// disegno, che chiama `sveglia` a ogni slide pronta.
    pub fn new(p: &Presentazione, da: usize, sveglia: impl Fn() + Send + 'static) -> io::Result<Self> {
        let doc = p.documento().clone();
        let mut stato = ShowState::start_at(&doc, da);
        // Un primo passo che parte da solo è già finito: le animazioni vanno a scatti.
        if stato.playing {
            stato.step_done(&doc);
        }
        let disegnatore = Disegnatore::avvia(doc.clone(), sveglia)?;
        Ok(Self {
            doc,
            stato,
            disegnatore,
            texture: HashMap::new(),
            mostrato: None,
            chieste: Vec::new(),
            dissolvenza: None,
            nero: false,
            penna: false,
            tratti: Vec::new(),
            dito: None,
            laser: None,
            esclusa: None,
            ignorate: Vec::new(),
            proiettore: None,
            proiettato: None,
            per_il_proiettore: None,
            partito: None,
            posto: Rect::NOTHING,
            larghezza: 0,
            disegnate: 0,
            ms_totali: 0,
            ms_massimo: 0,
        })
    }

    /// La slide che si presenta (indice nella presentazione).
    pub fn slide(&self) -> usize {
        self.stato.slide
    }

    /// Quanti passi della slide sono fatti.
    pub fn passo(&self) -> usize {
        self.stato.step
    }

    /// Dopo l'ultima slide: la schermata della fine.
    pub fn finita(&self) -> bool {
        self.stato.ended
    }

    /// Avanti: il passo dopo, o la slide dopo, o la fine; dalla fine si esce (falso). Con lo
    /// schermo nero toglie solo il nero.
    pub fn avanti(&mut self, adesso: f64) -> bool {
        if std::mem::take(&mut self.nero) {
            return true;
        }
        self.partito.get_or_insert(adesso);
        let azione = self.stato.next(&self.doc);
        self.dopo(azione, true)
    }

    /// Indietro: il passo di prima, o la slide di prima com'era stata lasciata (tutta costruita, se
    /// non la si era vista). Con lo schermo nero toglie solo il nero.
    pub fn indietro(&mut self, _adesso: f64) {
        if std::mem::take(&mut self.nero) {
            return;
        }
        let azione = self.stato.prev(&self.doc);
        self.dopo(azione, false);
    }

    /// Salta alla slide `i`, anche nascosta.
    pub fn vai(&mut self, i: usize) {
        self.nero = false;
        let azione = self.stato.goto(&self.doc, i);
        self.dopo(azione, true);
    }

    /// Quello che segue un'azione di `ShowState`: il passo che parte va subito alla fine, i tratti
    /// della penna si cancellano, una slide nuova con la sua transizione sfuma sopra quella di
    /// prima (solo andando avanti). Falso quando si esce.
    fn dopo(&mut self, azione: ShowAction, avanti: bool) -> bool {
        match azione {
            ShowAction::Exit => return false,
            ShowAction::None => return true,
            ShowAction::PlayStep(_) => {
                self.stato.step_done(&self.doc);
            }
            ShowAction::Slide(i) => {
                if self.stato.playing {
                    self.stato.step_done(&self.doc);
                }
                let durata = self.durata_della_transizione(i);
                self.dissolvenza = match self.mostrato.and_then(|m| self.texture.get(&m)) {
                    Some(vecchia) if avanti && durata > 0.0 => Some(Dissolvenza { vecchia: vecchia.clone(), durata, inizio: None }),
                    _ => None,
                };
            }
            ShowAction::FinishStep(_) | ShowAction::StepBack(_) | ShowAction::End => {}
        }
        self.tratti.clear();
        true
    }

    /// La durata della transizione della slide `i`, in secondi (0 senza transizione).
    fn durata_della_transizione(&self, i: usize) -> f64 {
        if self.doc.show.without_animation {
            return 0.0;
        }
        match self.doc.slides.get(i).and_then(|s| s.transition.as_ref()) {
            Some(t) if t.kind != "none" => (f64::from(t.duration_ms) / 1000.0).min(DISSOLVENZA_MASSIMA),
            _ => 0.0,
        }
    }

    pub fn nero(&self) -> bool {
        self.nero
    }

    pub fn alterna_nero(&mut self) {
        self.nero = !self.nero;
    }

    pub fn penna(&self) -> bool {
        self.penna
    }

    /// La penna si accende o si spegne; i tratti restano fino al passo dopo.
    pub fn alterna_penna(&mut self) {
        self.penna = !self.penna;
    }

    /// Cancella i tratti della penna sulla slide.
    pub fn cancella_tratti(&mut self) {
        self.tratti.clear();
    }

    /// La zona dei comandi aperti sopra la presentazione (None se sono chiusi): i tocchi che
    /// cominciano lì vanno a loro, e un tocco altrove li chiude (l'esito dice `barra`) senza
    /// andare avanti.
    pub fn escludi(&mut self, zona: Option<Rect>) {
        self.esclusa = zona;
    }

    /// Le zone di quello che sta sopra la presentazione senza essere i comandi (le note del
    /// relatore, un foglio): i tocchi che cominciano lì vanno a loro e la presentazione non li
    /// vede.
    pub fn ignora(&mut self, zone: Vec<Rect>) {
        self.ignorate = zone;
    }

    /// Lo schermo esterno della presentazione, in pixel (None se non c'è o si è staccato): da qui
    /// ogni passo va anche a lui, con `da_proiettare`.
    pub fn proiettore(&mut self, misura: Option<(u32, u32)>) {
        if misura != self.proiettore {
            self.proiettore = misura;
            self.proiettato = None;
            self.per_il_proiettore = None;
        }
    }

    /// L'immagine nuova per lo schermo esterno, una volta sola: la slide al passo di adesso, alla
    /// sua misura (al più `LATO_PROIETTORE`), o il nero. None se non è cambiato niente o non è
    /// ancora disegnata.
    pub fn da_proiettare(&mut self) -> Option<Immagine> {
        let voluta = self.proiezione()?;
        if self.proiettato == Some(voluta) {
            return None;
        }
        let img = match voluta {
            Proiezione::Nero => Immagine { larghezza: 16, altezza: 9, pixel: [0, 0, 0, 255].repeat(16 * 9) },
            Proiezione::Slide(_) => match self.per_il_proiettore.take() {
                Some((p, img)) if p == voluta => img,
                altro => {
                    self.per_il_proiettore = altro;
                    return None;
                }
            },
        };
        self.proiettato = Some(voluta);
        Some(img)
    }

    /// Quello che lo schermo esterno deve vedere adesso, se c'è.
    fn proiezione(&self) -> Option<Proiezione> {
        let (w, h) = self.proiettore?;
        if self.nero || self.stato.ended {
            return Some(Proiezione::Nero);
        }
        let (sw, sh) = (self.doc.slide_size.width, self.doc.slide_size.height);
        let adatta = (f64::from(w)).min(f64::from(h) * sw / sh.max(1.0)).round().max(1.0) as u32;
        let larghezza = larghezza_entro(sw, sh, adatta, LATO_PROIETTORE);
        self.fotogramma(larghezza).map(Proiezione::Slide)
    }

    /// Quanti tratti della penna ci sono sulla slide.
    pub fn tratti(&self) -> usize {
        self.tratti.len()
    }

    /// Il punto del laser sulla slide, da 0 a 1, mentre c'è.
    pub fn laser(&self) -> Option<Pos2> {
        self.laser
    }

    /// I secondi dal primo «avanti» (None prima).
    pub fn tempo(&self, adesso: f64) -> Option<f64> {
        self.partito.map(|p| (adesso - p).max(0.0))
    }

    /// Quello che si vedrà col prossimo «avanti» (il passo dopo o la slide dopo), se è già
    /// disegnato: per il relatore. None dopo l'ultima slide.
    pub fn anteprima(&self) -> Option<egui::TextureId> {
        self.prossimo(self.larghezza).and_then(|f| self.texture.get(&f)).map(egui::TextureHandle::id)
    }

    /// Il fotogramma sullo schermo o quello dopo non sono ancora disegnati. Alla fine non c'è
    /// niente da disegnare: senza slide si aspettava per sempre, ridisegnando ogni 16 ms.
    pub fn in_attesa(&self) -> bool {
        (self.mostrato.is_none() && !self.stato.ended) || !self.chieste.is_empty()
    }

    /// Una riga di misure per la diagnosi: quante slide disegnate, quanto tempo.
    pub fn riga_misure(&self) -> String {
        let media = if self.disegnate > 0 { self.ms_totali as f64 / self.disegnate as f64 } else { 0.0 };
        format!("{} fotogrammi disegnati, in media {media:.0} ms, al massimo {} ms", self.disegnate, self.ms_massimo)
    }

    /// Il fotogramma di adesso, alla larghezza `larghezza`; None alla fine.
    fn fotogramma(&self, larghezza: u32) -> Option<Fotogramma> {
        (!self.stato.ended && self.stato.slide < self.doc.slides.len()).then_some(Fotogramma {
            indice: self.stato.slide,
            passo: self.stato.step,
            larghezza,
        })
    }

    /// Il fotogramma che verrà col prossimo «avanti», da preparare.
    fn prossimo(&self, larghezza: u32) -> Option<Fotogramma> {
        if self.stato.ended {
            return None;
        }
        let passi = ShowState::steps(&self.doc, self.stato.slide);
        if self.stato.step < passi {
            return Some(Fotogramma { indice: self.stato.slide, passo: self.stato.step + 1, larghezza });
        }
        let i = self.stato.peek_next(&self.doc)?;
        let automatico = !self.doc.show.without_animation && ShowState::timeline(&self.doc, i).step_is_auto(0);
        Some(Fotogramma { indice: i, passo: usize::from(automatico), larghezza })
    }

    /// Il posto della slide nella zona `rect`: tutta dentro, in mezzo, nelle proporzioni giuste.
    fn adatta(&self, rect: Rect) -> Rect {
        let (w, h) = (self.doc.slide_size.width as f32, self.doc.slide_size.height as f32);
        if w <= 0.0 || h <= 0.0 {
            return rect;
        }
        let k = (rect.width() / w).min(rect.height() / h);
        Rect::from_center_size(rect.center(), vec2(w * k, h * k))
    }

    /// Disegna la presentazione in tutto lo spazio di `ui` e prende tasti e dita.
    pub fn mostra(&mut self, ui: &mut Ui) -> EsitoSpettacolo {
        let mut esito = EsitoSpettacolo::default();
        let rect = ui.max_rect();
        let adesso = ui.input(|i| i.time);
        let ppp = ui.ctx().pixels_per_point();
        // Tutta la zona è della presentazione: i tocchi non vanno a quello che c'è sotto.
        ui.allocate_rect(rect, Sense::click_and_drag());
        // Il posto della slide (prima del dito, che lo usa) e la larghezza a cui disegnarla, col
        // lato più lungo al più `LATO_MASSIMO` pixel.
        self.posto = self.adatta(rect);
        let lato_massimo = (ui.input(|i| i.max_texture_side) as f32).min(LATO_MASSIMO) as u32;
        let voluta = (self.posto.width() * ppp).round().max(1.0) as u32;
        self.larghezza = larghezza_entro(self.doc.slide_size.width, self.doc.slide_size.height, voluta, lato_massimo);
        self.tasti(ui, adesso, &mut esito);
        self.dita(ui, rect, adesso, &mut esito);
        self.ricevi(ui.ctx());
        self.chiedi();
        self.dipingi(ui, rect, adesso);
        if self.in_attesa() || self.dissolvenza.is_some() || self.dito.is_some() {
            ui.ctx().request_repaint_after(ATTESA);
        }
        esito
    }

    /// I tasti di una tastiera o di un telecomando: avanti, indietro, nero, uscita.
    fn tasti(&mut self, ui: &Ui, adesso: f64, esito: &mut EsitoSpettacolo) {
        use egui::Key;
        let premuti: Vec<Key> = ui.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Key { key, pressed: true, .. } => Some(*key),
                    _ => None,
                })
                .collect()
        });
        for k in premuti {
            match k {
                Key::PageDown | Key::ArrowRight | Key::ArrowDown | Key::Space | Key::Enter | Key::N => {
                    if !self.avanti(adesso) {
                        esito.esci = true;
                    }
                }
                Key::PageUp | Key::ArrowLeft | Key::ArrowUp | Key::Backspace | Key::P => self.indietro(adesso),
                Key::B | Key::Period => self.alterna_nero(),
                Key::Escape => esito.esci = true,
                _ => {}
            }
        }
    }

    /// Il dito: un tocco va avanti (o indietro nel terzo sinistro, o apre la barra nella fascia in
    /// fondo), uno scorrimento orizzontale cambia passo, il dito fermo accende il laser; con la
    /// penna il dito disegna, ma non nella fascia in fondo, che apre sempre la barra (se no con la
    /// penna accesa non si tornerebbe ai comandi). Sullo schermo nero il dito toglie il nero e
    /// basta.
    fn dita(&mut self, ui: &Ui, rect: Rect, adesso: f64, esito: &mut EsitoSpettacolo) {
        let (premuto, giu, alzato, dove) =
            ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down(), i.pointer.primary_released(), i.pointer.latest_pos()));
        if premuto
            && let Some(p) = dove
            && !self.esclusa.is_some_and(|z| z.contains(p))
            && !self.ignorate.iter().any(|z| z.contains(p))
        {
            let fascia = p.y >= rect.bottom() - FASCIA_DELLA_BARRA;
            self.dito = Some(Dito { inizio: p, t0: adesso, adesso: p, laser: false, fascia });
            // La penna comincia sulla slide; con la barra aperta il tocco è per chiuderla.
            if self.penna && !self.nero && !fascia && self.esclusa.is_none() && self.posto.contains(p) {
                self.tratti.push(vec![sulla_slide(self.posto, p)]);
            }
        }
        if giu
            && !self.nero
            && let (Some(d), Some(p)) = (self.dito.as_mut(), dove)
        {
            d.adesso = p;
            if d.fascia || self.esclusa.is_some() {
                // La fascia della barra, o la barra aperta: né penna né laser.
            } else if self.penna {
                let punto = sulla_slide(self.posto, p);
                if let Some(t) = self.tratti.last_mut()
                    && t.last() != Some(&punto)
                {
                    t.push(punto);
                }
            } else if !d.laser && (p - d.inizio).length() <= FERMO && adesso - d.t0 >= ATTESA_LASER {
                d.laser = true;
            }
            if d.laser {
                self.laser = Some(sulla_slide(self.posto, p));
            }
        }
        if alzato && let Some(d) = self.dito.take() {
            self.laser = None;
            if std::mem::take(&mut self.nero) {
                return;
            }
            let spostamento = d.adesso - d.inizio;
            let breve = spostamento.length() <= FERMO * 2.0;
            if (d.fascia || self.esclusa.is_some()) && breve {
                esito.barra = true;
                return;
            }
            if self.penna || d.laser {
                return;
            }
            if spostamento.x.abs() >= SCORRIMENTO && spostamento.x.abs() > spostamento.y.abs() {
                if spostamento.x < 0.0 {
                    if !self.avanti(adesso) {
                        esito.esci = true;
                    }
                } else {
                    self.indietro(adesso);
                }
            } else if breve {
                if d.inizio.x < rect.left() + rect.width() * PARTE_INDIETRO {
                    self.indietro(adesso);
                } else if !self.avanti(adesso) {
                    esito.esci = true;
                }
            }
        }
    }

    /// Chiede al thread il fotogramma di adesso e quello dopo, se non ci sono già; solo quando
    /// l'elenco cambia.
    fn chiedi(&mut self) {
        // Prima lo schermo del telefono, poi quello esterno, poi il passo dopo.
        let proiettore = match self.proiezione() {
            Some(p @ Proiezione::Slide(f)) if self.proiettato != Some(p) && self.per_il_proiettore.as_ref().is_none_or(|(q, _)| *q != p) => {
                Some(f.richiesta_per(Uso::Proiettore))
            }
            _ => None,
        };
        let vista = self.fotogramma(self.larghezza).filter(|f| !self.texture.contains_key(f)).map(Fotogramma::richiesta);
        let prossimo = self.prossimo(self.larghezza).filter(|f| !self.texture.contains_key(f)).map(Fotogramma::richiesta);
        let voluti: Vec<Richiesta> = [vista, proiettore, prossimo].into_iter().flatten().collect();
        if voluti != self.chieste {
            self.disegnatore.chiedi(voluti.iter().copied());
            self.chieste = voluti;
        }
    }

    /// I fotogrammi disegnati dal thread, in texture; via quelli che non servono più.
    fn ricevi(&mut self, ctx: &egui::Context) {
        while let Some(d) = self.disegnatore.prendi() {
            self.disegnate += 1;
            self.ms_totali += u64::from(d.ms);
            self.ms_massimo = self.ms_massimo.max(d.ms);
            self.chieste.retain(|c| *c != d.richiesta);
            let r = d.richiesta;
            let Some(passo) = r.passo else { continue };
            let img = d.immagine;
            if r.uso == Uso::Proiettore {
                let p = Proiezione::Slide(Fotogramma { indice: r.indice, passo, larghezza: r.larghezza });
                if self.proiezione() == Some(p) {
                    self.per_il_proiettore = Some((p, img));
                }
                continue;
            }
            let colori = egui::ColorImage::from_rgba_premultiplied([img.larghezza as usize, img.altezza as usize], &img.pixel);
            let texture = ctx.load_texture(format!("presentazione {} {}", r.indice, passo), colori, egui::TextureOptions::LINEAR);
            self.texture.insert(Fotogramma { indice: r.indice, passo, larghezza: r.larghezza }, texture);
        }
        let tieni = [self.fotogramma(self.larghezza), self.prossimo(self.larghezza), self.mostrato];
        self.texture.retain(|f, _| tieni.contains(&Some(*f)));
    }

    /// Il nero, la slide (con la dissolvenza da quella di prima), la penna, il laser; alla fine la
    /// sua schermata.
    fn dipingi(&mut self, ui: &Ui, rect: Rect, adesso: f64) {
        let painter = ui.painter();
        painter.rect_filled(rect, 0.0, Color32::BLACK);
        if self.nero {
            return;
        }
        if self.stato.ended {
            let titolo = Carattere::Grassetto.font(ui.ctx(), 20.0);
            let dove = painter.text(rect.center() - vec2(0.0, 14.0), Align2::CENTER_CENTER, FINE, titolo, Color32::from_gray(200));
            let sotto = Carattere::Testo.font(ui.ctx(), 15.0);
            painter.text(rect.center() + vec2(0.0, 16.0), Align2::CENTER_CENTER, TOCCA_PER_USCIRE, sotto, Color32::from_gray(140));
            // Per chi usa il lettore dello schermo (e per le prove dell'app).
            ui.interact(dove, ui.id().with("fine"), Sense::hover()).widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, FINE));
            return;
        }
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        let voluto = self.fotogramma(self.larghezza);
        // Il fotogramma nuovo appena c'è; finché manca resta quello di prima.
        if let Some(f) = voluto
            && self.texture.contains_key(&f)
        {
            self.mostrato = Some(f);
        }
        let nuova = self.mostrato.and_then(|f| self.texture.get(&f));
        let mut sfumata = false;
        match (self.dissolvenza.as_mut(), nuova) {
            (Some(d), Some(nuova)) if voluto == self.mostrato => {
                let inizio = *d.inizio.get_or_insert(adesso);
                let p = ((adesso - inizio) / d.durata).clamp(0.0, 1.0) as f32;
                sfumata = p >= 1.0;
                if !sfumata {
                    painter.image(d.vecchia.id(), self.posto, uv, Color32::WHITE.gamma_multiply(1.0 - p));
                }
                painter.image(nuova.id(), self.posto, uv, Color32::WHITE.gamma_multiply(p));
            }
            (Some(d), _) => {
                painter.image(d.vecchia.id(), self.posto, uv, Color32::WHITE);
            }
            (None, Some(nuova)) => {
                painter.image(nuova.id(), self.posto, uv, Color32::WHITE);
            }
            (None, None) => {}
        }
        if sfumata {
            self.dissolvenza = None;
        }
        let sullo_schermo = |p: Pos2| self.posto.min + vec2(p.x * self.posto.width(), p.y * self.posto.height());
        for t in &self.tratti {
            let punti: Vec<Pos2> = t.iter().map(|p| sullo_schermo(*p)).collect();
            if punti.len() == 1 {
                painter.circle_filled(punti[0], PENNA / 2.0, tema::ROSSO);
            } else {
                painter.add(egui::Shape::line(punti, Stroke::new(PENNA, tema::ROSSO)));
            }
        }
        if let Some(l) = self.laser {
            let c = sullo_schermo(l);
            painter.circle_filled(c, ALONE_LASER, Color32::from_rgba_unmultiplied(255, 40, 30, 60));
            painter.circle_filled(c, RAGGIO_LASER, Color32::from_rgb(255, 40, 30));
        }
    }
}

/// Il punto dello schermo `p` nella slide al posto `posto`, da 0 a 1 (fuori dalla slide, sul bordo).
fn sulla_slide(posto: Rect, p: Pos2) -> Pos2 {
    let w = posto.width().max(1.0);
    let h = posto.height().max(1.0);
    pos2(((p.x - posto.left()) / w).clamp(0.0, 1.0), ((p.y - posto.top()) / h).clamp(0.0, 1.0))
}

#[cfg(test)]
mod test {
    use super::*;
    use egui::{Rect, pos2, vec2};
    use std::time::{Duration, Instant};

    /// Un telefono finto in verticale, a schermo intero: 412 × 915 dp a 2,625 pixel per dp.
    struct Telefono {
        ctx: egui::Context,
        t: f64,
        /// Il lato più lungo di una texture che la scheda grafica accetta.
        lato_texture: usize,
        /// La misura dello schermo, in dp.
        schermo: egui::Vec2,
    }

    struct Frame {
        esito: EsitoSpettacolo,
        forme: Vec<egui::epaint::ClippedShape>,
    }

    impl Frame {
        fn testi(&self) -> Vec<String> {
            self.forme
                .iter()
                .filter_map(|f| match &f.shape {
                    egui::Shape::Text(t) => Some(t.galley.text().to_string()),
                    _ => None,
                })
                .collect()
        }

        fn immagini(&self) -> Vec<Rect> {
            self.forme
                .iter()
                .filter_map(|f| match &f.shape {
                    egui::Shape::Mesh(m) if m.texture_id != egui::TextureId::default() => Some(m.calc_bounds()),
                    _ => None,
                })
                .collect()
        }
    }

    impl Telefono {
        fn new() -> Self {
            Self { ctx: egui::Context::default(), t: 0.0, lato_texture: 4096, schermo: vec2(412.0, 915.0) }
        }

        /// La presentazione della lezione, dalla slide `da`.
        fn spettacolo(&self, da: usize) -> Spettacolo {
            self.spettacolo_di(crate::prova::LEZIONE, da)
        }

        /// La presentazione dei byte `byte`, dalla slide `da`.
        fn spettacolo_di(&self, byte: &[u8], da: usize) -> Spettacolo {
            let p = Presentazione::apri(byte).unwrap();
            let ctx = self.ctx.clone();
            Spettacolo::new(&p, da, move || ctx.request_repaint()).unwrap()
        }

        /// Un frame `dt` secondi dopo il precedente.
        fn passo_dopo(&mut self, s: &mut Spettacolo, dt: f64, eventi: Vec<egui::Event>) -> Frame {
            self.t += dt;
            let mut ri = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.schermo)),
                max_texture_side: Some(self.lato_texture),
                time: Some(self.t),
                predicted_dt: dt as f32,
                events: eventi,
                ..Default::default()
            };
            ri.viewports.entry(egui::ViewportId::ROOT).or_default().native_pixels_per_point = Some(2.625);
            let mut esito = EsitoSpettacolo::default();
            let mut out = self.ctx.run_ui(ri, |ui| esito = s.mostra(ui));
            out.textures_delta.clear();
            Frame { esito, forme: std::mem::take(&mut out.shapes) }
        }

        fn passo(&mut self, s: &mut Spettacolo, eventi: Vec<egui::Event>) -> Frame {
            self.passo_dopo(s, 1.0 / 60.0, eventi)
        }

        /// Frame finché la presentazione non aspetta più slide dal thread, al massimo 20 s.
        fn aspetta(&mut self, s: &mut Spettacolo) -> Frame {
            let fine = Instant::now() + Duration::from_secs(20);
            while s.in_attesa() {
                assert!(Instant::now() < fine, "la presentazione non ha finito in 20 s");
                self.passo(s, vec![]);
                std::thread::sleep(Duration::from_millis(5));
            }
            self.passo(s, vec![])
        }
    }

    fn tasto(key: egui::Key) -> egui::Event {
        egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }
    }

    fn dito(pressed: bool, pos: Pos2) -> egui::Event {
        egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE }
    }

    /// Un tocco breve in `pos`.
    fn tocca(tel: &mut Telefono, s: &mut Spettacolo, pos: Pos2) -> Frame {
        tel.passo(s, vec![egui::Event::PointerMoved(pos), dito(true, pos)]);
        tel.passo(s, vec![dito(false, pos)])
    }

    /// Il dito giù in `da`, trascinato fino ad `a` in qualche frame, poi su.
    fn trascina(tel: &mut Telefono, s: &mut Spettacolo, da: Pos2, a: Pos2) -> Frame {
        tel.passo(s, vec![egui::Event::PointerMoved(da), dito(true, da)]);
        for i in 1..=5 {
            let p = da + (a - da) * (i as f32 / 5.0);
            tel.passo(s, vec![egui::Event::PointerMoved(p)]);
        }
        tel.passo(s, vec![dito(false, a)])
    }

    #[test]
    fn i_tre_punti_entrano_uno_alla_volta_poi_la_slide_dopo() {
        let tel = Telefono::new();
        let mut s = tel.spettacolo(1);
        assert_eq!((s.slide(), s.passo()), (1, 0));
        for passo in 1..=3 {
            assert!(s.avanti(1.0));
            assert_eq!((s.slide(), s.passo()), (1, passo));
        }
        assert!(s.avanti(2.0));
        assert_eq!((s.slide(), s.passo()), (2, 0));
    }

    #[test]
    fn la_nascosta_si_salta_e_dopo_l_ultima_c_e_la_fine() {
        let tel = Telefono::new();
        let mut s = tel.spettacolo(2);
        assert!(s.avanti(1.0));
        assert!(s.finita(), "la slide nascosta non va presentata");
        // Ancora avanti dalla fine: si esce.
        assert!(!s.avanti(2.0));
    }

    #[test]
    fn indietro_torna_alla_slide_di_prima_gia_costruita() {
        let tel = Telefono::new();
        let mut s = tel.spettacolo(2);
        s.indietro(1.0);
        assert_eq!((s.slide(), s.passo()), (1, 3));
        s.indietro(2.0);
        assert_eq!((s.slide(), s.passo()), (1, 2));
    }

    #[test]
    fn i_tasti_del_telecomando_vanno_avanti_e_indietro() {
        let mut tel = Telefono::new();
        let mut s = tel.spettacolo(1);
        tel.passo(&mut s, vec![tasto(egui::Key::PageDown)]);
        tel.passo(&mut s, vec![tasto(egui::Key::ArrowRight)]);
        tel.passo(&mut s, vec![tasto(egui::Key::Space)]);
        assert_eq!((s.slide(), s.passo()), (1, 3));
        tel.passo(&mut s, vec![tasto(egui::Key::PageUp)]);
        tel.passo(&mut s, vec![tasto(egui::Key::ArrowLeft)]);
        assert_eq!((s.slide(), s.passo()), (1, 1));
        // B: lo schermo nero; un altro tasto lo toglie senza andare avanti.
        tel.passo(&mut s, vec![tasto(egui::Key::B)]);
        assert!(s.nero());
        tel.passo(&mut s, vec![tasto(egui::Key::PageDown)]);
        assert!(!s.nero());
        assert_eq!((s.slide(), s.passo()), (1, 1));
        // Esc esce.
        assert!(tel.passo(&mut s, vec![tasto(egui::Key::Escape)]).esito.esci);
    }

    #[test]
    fn un_tocco_a_destra_va_avanti_a_sinistra_indietro_in_fondo_la_barra() {
        let mut tel = Telefono::new();
        let mut s = tel.spettacolo(1);
        tocca(&mut tel, &mut s, pos2(300.0, 400.0));
        assert_eq!(s.passo(), 1);
        tocca(&mut tel, &mut s, pos2(60.0, 400.0));
        assert_eq!(s.passo(), 0);
        // La fascia in fondo non va né avanti né indietro: apre la barra.
        let f = tocca(&mut tel, &mut s, pos2(300.0, 890.0));
        assert!(f.esito.barra);
        assert_eq!(s.passo(), 0);
    }

    #[test]
    fn un_tocco_nella_zona_esclusa_non_fa_niente() {
        let mut tel = Telefono::new();
        let mut s = tel.spettacolo(1);
        // La barra dei comandi sopra la presentazione: i suoi tocchi sono suoi.
        s.escludi(Some(Rect::from_min_max(pos2(0.0, 800.0), pos2(412.0, 915.0))));
        let f = tocca(&mut tel, &mut s, pos2(300.0, 890.0));
        assert!(!f.esito.barra);
        assert_eq!(s.passo(), 0);
        s.escludi(None);
        assert!(tocca(&mut tel, &mut s, pos2(300.0, 890.0)).esito.barra);
    }

    #[test]
    fn con_i_comandi_aperti_un_tocco_fuori_li_chiude_e_non_va_avanti() {
        let mut tel = Telefono::new();
        let mut s = tel.spettacolo(1);
        // Anche con la penna: il tocco chiude la barra e non lascia segni.
        s.alterna_penna();
        s.escludi(Some(Rect::from_min_max(pos2(0.0, 800.0), pos2(412.0, 915.0))));
        let f = tocca(&mut tel, &mut s, pos2(300.0, 400.0));
        assert!(f.esito.barra);
        assert_eq!((s.passo(), s.tratti()), (0, 0));
    }

    #[test]
    fn la_penna_disegna_solo_cominciando_sulla_slide() {
        let mut tel = Telefono::new();
        let mut s = tel.spettacolo(1);
        s.alterna_penna();
        // La slide va da 341,6 a 573,4 dp: sopra c'è il nero.
        trascina(&mut tel, &mut s, pos2(100.0, 200.0), pos2(300.0, 210.0));
        assert_eq!(s.tratti(), 0);
        trascina(&mut tel, &mut s, pos2(100.0, 400.0), pos2(300.0, 410.0));
        assert_eq!(s.tratti(), 1);
    }

    #[test]
    fn i_tocchi_sulle_zone_ignorate_non_sono_della_presentazione() {
        let mut tel = Telefono::new();
        let mut s = tel.spettacolo(1);
        // Il pannello delle note del relatore: lì si scorre il testo, non le slide.
        s.ignora(vec![Rect::from_min_max(pos2(0.0, 500.0), pos2(412.0, 800.0))]);
        let f = tocca(&mut tel, &mut s, pos2(300.0, 600.0));
        assert!(!f.esito.barra);
        trascina(&mut tel, &mut s, pos2(320.0, 600.0), pos2(100.0, 610.0));
        assert_eq!(s.passo(), 0);
        // Fuori dal pannello si va avanti come sempre.
        tocca(&mut tel, &mut s, pos2(300.0, 400.0));
        assert_eq!(s.passo(), 1);
    }

    #[test]
    fn uno_scorrimento_verso_sinistra_va_avanti_verso_destra_indietro() {
        let mut tel = Telefono::new();
        let mut s = tel.spettacolo(1);
        trascina(&mut tel, &mut s, pos2(320.0, 400.0), pos2(100.0, 410.0));
        assert_eq!(s.passo(), 1);
        trascina(&mut tel, &mut s, pos2(100.0, 400.0), pos2(320.0, 390.0));
        assert_eq!(s.passo(), 0);
    }

    #[test]
    fn il_laser_c_e_solo_mentre_il_dito_e_fermo_e_giu() {
        let mut tel = Telefono::new();
        let mut s = tel.spettacolo(1);
        let p = pos2(200.0, 450.0);
        tel.passo(&mut s, vec![egui::Event::PointerMoved(p), dito(true, p)]);
        assert!(s.laser().is_none(), "il laser è venuto subito");
        tel.passo_dopo(&mut s, 0.6, vec![]);
        tel.passo(&mut s, vec![]);
        let laser = s.laser().expect("niente laser col dito fermo");
        // In coordinate della slide, da 0 a 1: il punto è a metà in larghezza.
        assert!((laser.x - 200.0 / 412.0).abs() < 0.01, "{laser:?}");
        tel.passo(&mut s, vec![dito(false, p)]);
        assert!(s.laser().is_none());
        // Il dito alzato dopo il laser non va avanti.
        assert_eq!(s.passo(), 0);
    }

    #[test]
    fn la_penna_disegna_e_il_cambio_di_passo_la_cancella() {
        let mut tel = Telefono::new();
        let mut s = tel.spettacolo(1);
        s.alterna_penna();
        trascina(&mut tel, &mut s, pos2(100.0, 400.0), pos2(300.0, 420.0));
        assert_eq!(s.tratti(), 1);
        // Con la penna il trascinamento disegna e non va avanti.
        assert_eq!(s.passo(), 0);
        s.avanti(1.0);
        assert_eq!(s.tratti(), 0);
    }

    #[test]
    fn sullo_schermo_nero_il_dito_toglie_il_nero_anche_con_la_penna() {
        let mut tel = Telefono::new();
        let mut s = tel.spettacolo(1);
        s.alterna_penna();
        s.alterna_nero();
        tocca(&mut tel, &mut s, pos2(300.0, 400.0));
        assert!(!s.nero());
        assert_eq!((s.passo(), s.tratti()), (0, 0));
    }

    #[test]
    fn con_la_penna_la_fascia_in_fondo_apre_ancora_la_barra() {
        let mut tel = Telefono::new();
        let mut s = tel.spettacolo(1);
        s.alterna_penna();
        let f = tocca(&mut tel, &mut s, pos2(200.0, 900.0));
        assert!(f.esito.barra);
        assert_eq!(s.tratti(), 0);
    }

    #[test]
    fn il_tempo_parte_al_primo_avanti() {
        let tel = Telefono::new();
        let mut s = tel.spettacolo(0);
        assert_eq!(s.tempo(10.0), None);
        s.avanti(12.0);
        assert_eq!(s.tempo(72.0), Some(60.0));
    }

    #[test]
    fn la_slide_non_supera_il_lato_massimo_delle_texture() {
        // Col telefono girato la slide è larga 732 dp, 1922 pixel: con texture fino a 1024 si
        // disegna a 1024 e si allarga.
        let mut tel = Telefono { lato_texture: 1024, schermo: vec2(915.0, 412.0), ..Telefono::new() };
        let mut s = tel.spettacolo(0);
        let f = tel.aspetta(&mut s);
        assert_eq!(f.immagini().len(), 1);
        assert!(s.riga_misure().starts_with("2 fotogrammi"), "{}", s.riga_misure());
    }

    #[test]
    fn la_slide_con_la_transizione_sfuma_sopra_quella_di_prima() {
        let mut tel = Telefono::new();
        let mut s = tel.spettacolo(0);
        // Finché aspetta, prepara anche il passo dopo: la slide 2, che entra in dissolvenza.
        tel.aspetta(&mut s);
        s.avanti(tel.t);
        let f = tel.passo(&mut s, vec![]);
        assert_eq!(f.immagini().len(), 2, "durante la dissolvenza si vedono le due slide");
        tel.passo_dopo(&mut s, 1.5, vec![]);
        let f = tel.passo(&mut s, vec![]);
        assert_eq!(f.immagini().len(), 1, "finita la dissolvenza resta la slide nuova");
        // Indietro niente dissolvenza.
        s.indietro(tel.t);
        tel.aspetta(&mut s);
        let f = tel.passo(&mut s, vec![]);
        assert_eq!(f.immagini().len(), 1);
    }

    #[test]
    fn l_anteprima_e_quello_che_viene_col_prossimo_avanti() {
        let mut tel = Telefono::new();
        let mut s = tel.spettacolo(1);
        tel.aspetta(&mut s);
        let anteprima = s.anteprima().expect("il passo dopo non è pronto");
        // È il fotogramma della slide 2 col primo punto: si mostra quando si va avanti.
        s.avanti(tel.t);
        let f = tel.passo(&mut s, vec![]);
        assert_eq!((s.slide(), s.passo()), (1, 1));
        assert!(f.forme.iter().any(|c| matches!(&c.shape, egui::Shape::Mesh(m) if m.texture_id == anteprima)));
        // Dall'ultima slide non c'è un dopo.
        let mut s = tel.spettacolo(2);
        tel.aspetta(&mut s);
        assert!(s.anteprima().is_none());
    }

    /// Frame finché la presentazione non ha un'immagine nuova per il proiettore, al massimo 20 s.
    fn da_proiettare(tel: &mut Telefono, s: &mut Spettacolo) -> crate::apertura::Immagine {
        let fine = Instant::now() + Duration::from_secs(20);
        loop {
            tel.passo(s, vec![]);
            if let Some(img) = s.da_proiettare() {
                return img;
            }
            assert!(Instant::now() < fine, "niente per il proiettore in 20 s");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn col_proiettore_ogni_passo_va_anche_allo_schermo_esterno() {
        let mut tel = Telefono::new();
        let mut s = tel.spettacolo(1);
        s.proiettore(Some((1920, 1080)));
        let img = da_proiettare(&mut tel, &mut s);
        assert_eq!((img.larghezza, img.altezza, img.pixel.len()), (1920, 1080, 1920 * 1080 * 4));
        // Senza cambiare passo non si rimanda niente.
        for _ in 0..5 {
            tel.passo(&mut s, vec![]);
        }
        assert!(s.da_proiettare().is_none());
        // Un passo avanti: l'immagine nuova.
        s.avanti(tel.t);
        let img = da_proiettare(&mut tel, &mut s);
        assert_eq!(img.larghezza, 1920);
        // Uno schermo 4:3 grande: la slide larga quanto lui, entro 1920 pixel.
        s.proiettore(Some((2048, 1536)));
        let img = da_proiettare(&mut tel, &mut s);
        assert_eq!((img.larghezza, img.altezza), (1920, 1080));
        // Lo schermo nero va anche al proiettore: è per chi guarda.
        s.alterna_nero();
        let img = da_proiettare(&mut tel, &mut s);
        assert!(img.pixel.as_chunks::<4>().0.iter().all(|p| *p == [0, 0, 0, 255]));
        s.alterna_nero();
        assert_eq!(da_proiettare(&mut tel, &mut s).larghezza, 1920);
        // Staccato lo schermo, niente più.
        s.proiettore(None);
        s.avanti(tel.t);
        tel.aspetta(&mut s);
        assert!(s.da_proiettare().is_none());
    }

    #[test]
    fn la_slide_sta_tutta_nello_schermo_sul_nero() {
        let mut tel = Telefono::new();
        let mut s = tel.spettacolo(0);
        let f = tel.aspetta(&mut s);
        // 412 dp di larghezza e 231,75 di altezza, in mezzo ai 915 dp dello schermo.
        let immagini = f.immagini();
        assert_eq!(immagini.len(), 1, "{immagini:?}");
        let r = immagini[0];
        assert!((r.width() - 412.0).abs() < 0.5 && (r.height() - 231.75).abs() < 0.5, "{r:?}");
        assert!((r.center().y - 457.5).abs() < 0.5, "{r:?}");
        // Dopo l'ultima slide, la fine.
        let mut s = tel.spettacolo(2);
        s.avanti(1.0);
        let f = tel.passo(&mut s, vec![]);
        assert!(f.testi().iter().any(|t| t == FINE), "{:?}", f.testi());
    }

    #[test]
    fn le_slide_in_quattro_terzi_e_in_verticale_stanno_tutte_nello_schermo() {
        // In verticale (412 × 915 dp) e girato (915 × 412): la slide intera, in mezzo, nelle sue
        // proporzioni.
        for (schermo, byte, attesa) in [
            (vec2(412.0, 915.0), crate::prova::QUATTRO_TERZI, vec2(412.0, 309.0)),
            (vec2(915.0, 412.0), crate::prova::QUATTRO_TERZI, vec2(412.0 * 720.0 / 540.0, 412.0)),
            (vec2(412.0, 915.0), crate::prova::VERTICALE, vec2(412.0, 412.0 * 960.0 / 540.0)),
            (vec2(915.0, 412.0), crate::prova::VERTICALE, vec2(412.0 * 540.0 / 960.0, 412.0)),
        ] {
            let mut tel = Telefono { schermo, ..Telefono::new() };
            let mut s = tel.spettacolo_di(byte, 0);
            let f = tel.aspetta(&mut s);
            let r = f.immagini()[0];
            assert!((r.size() - attesa).length() < 0.5 && (r.center() - (schermo / 2.0).to_pos2()).length() < 0.5, "{schermo:?}: {r:?}");
        }
    }

    #[test]
    fn tutte_nascoste_si_presenta_da_dove_si_chiede_poi_c_e_la_fine() {
        let mut tel = Telefono::new();
        let mut s = tel.spettacolo_di(crate::prova::TUTTE_NASCOSTE, 0);
        let f = tel.aspetta(&mut s);
        assert_eq!((s.slide(), f.immagini().len()), (0, 1));
        assert!(s.avanti(tel.t));
        let f = tel.passo(&mut s, vec![]);
        assert!(s.finita() && f.testi().iter().any(|t| t == FINE), "{:?}", f.testi());
        assert!(!s.in_attesa());
        assert!(!s.avanti(tel.t), "dalla fine si esce");
    }

    #[test]
    fn senza_slide_la_presentazione_e_alla_fine_e_non_aspetta_niente() {
        // Senza niente da disegnare la presentazione aspettava per sempre un fotogramma, e
        // ridisegnava lo schermo ogni 16 ms.
        let mut tel = Telefono::new();
        let mut s = tel.spettacolo_di(crate::prova::SENZA_SLIDE, 0);
        let f = tel.passo(&mut s, vec![]);
        assert!(s.finita() && f.testi().iter().any(|t| t == FINE), "{:?}", f.testi());
        assert!(!s.in_attesa(), "aspetta un fotogramma che non arriverà mai");
        assert!(s.anteprima().is_none());
        let f = tocca(&mut tel, &mut s, pos2(300.0, 400.0));
        assert!(f.esito.esci);
    }
}
