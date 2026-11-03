//! Le Slide (tappa G): la barra alta col nome e «Slide · N slide», indietro; sotto, le slide una
//! sotto l'altra (`tascabile_slide::LettoreSlide`); in fondo la barra con Presenta, Miniature e
//! Note. La presentazione si apre su un thread suo e arriva al thread dell'interfaccia quando è
//! pronta; intanto, dopo un attimo, «Apro la presentazione…».
//!
//! «Presenta» parte dalla slide che si legge, a tutto schermo (`tascabile_slide::Spettacolo`):
//! il Java tiene lo schermo acceso, nasconde le barre di sistema e fa dei tasti del volume avanti
//! e indietro. Un tocco nella fascia in fondo apre la barra con Esci, Penna, Cancella, Nero, Note
//! e Slide, e sopra il tempo dal primo «avanti» e l'ora. Note apre il pannello del relatore (le
//! note della slide e quello che viene col prossimo «avanti»), Slide la griglia per saltare a una
//! slide, anche nascosta. Indietro esce, e la lettura torna sulla slide presentata.
//!
//! Nella diagnosi solo misure: byte, tipo, tempi, slide, generi d'errore; mai il nome del file,
//! l'URI o il testo delle slide.

use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use eframe::egui::{self, Rect, RichText, UiBuilder, pos2, vec2};
use tascabile_files::{ESTENSIONI_PRESENTAZIONE, Recente, Tipo};
use tascabile_platform::diagnosi;
use tascabile_slide::{ErroreApertura, ErrorePdf, Genere, LettoreSlide, Presentazione, Spettacolo};
use tascabile_touch::tema::{self, Carattere};
use tascabile_touch::{BarraAlta, BarraVoci, FoglioDalFondo, Menu, PulsanteIcona, SceltaMenu, icone};
use tascabile_word::salva;

use crate::quando::Locale;
use crate::{Da, Errore, Schermata, TascabileApp, adesso, home};

/// Le etichette dei pulsanti della barra alta (per l'accessibilità e per le prove).
pub(crate) const INDIETRO: &str = "Indietro";
pub(crate) const MENU: &str = "Menu";
/// Le voci del menu.
pub(crate) const CONDIVIDI: &str = "Condividi";
pub(crate) const ESPORTA_PDF: &str = "Esporta in PDF";
pub(crate) const PDF_CON_NOTE: &str = "PDF con le note";
pub(crate) const PRESENTA_DALL_INIZIO: &str = "Presenta dall'inizio";
/// Mentre si prepara il PDF delle slide.
const PREPARO_IL_PDF: &str = "Preparo il PDF…";
/// L'angolo del menu, come quello del PDF.
const MENU_DESTRA: f32 = 8.0;
const MENU_SOTTO: f32 = 44.0;
/// I tipi MIME delle presentazioni che si aprono: .pptx, .ppsx, .potx.
pub(crate) const MIME_PPTX: &str = "application/vnd.openxmlformats-officedocument.presentationml.presentation";
const MIME_PPSX: &str = "application/vnd.openxmlformats-officedocument.presentationml.slideshow";
const MIME_POTX: &str = "application/vnd.openxmlformats-officedocument.presentationml.template";
/// Il testo mentre la presentazione si apre.
pub(crate) const APRO: &str = "Apro la presentazione…";
/// Le voci della barra in fondo, e i titoli dei loro fogli.
pub(crate) const PRESENTA: &str = "Presenta";
pub(crate) const MINIATURE: &str = "Miniature";
pub(crate) const NOTE: &str = "Note";
/// Le voci della barra della presentazione.
pub(crate) const ESCI: &str = "Esci";
pub(crate) const PENNA: &str = "Penna";
pub(crate) const CANCELLA: &str = "Cancella";
pub(crate) const NERO: &str = "Nero";
pub(crate) const SLIDE: &str = "Slide";
/// Nel pannello del relatore, sopra quello che viene col prossimo «avanti».
pub(crate) const DOPO: &str = "Dopo";
/// Al posto dell'anteprima, dopo l'ultima slide.
const FINE_DOPO: &str = "Fine";
/// Il pannello del relatore: alto al più questa parte dello schermo, l'anteprima larga tanto.
const ALTEZZA_RELATORE: f32 = 0.45;
const LARGHEZZA_ANTEPRIMA: f32 = 128.0;
/// Il foglio delle note di una slide che non ne ha.
pub(crate) const NESSUNA_NOTA: &str = "Nessuna nota per questa slide.";
/// La barra in fondo lascia tanto spazio sopra di sé dopo l'ultima slide.
const SOPRA_LA_BARRA: f32 = 8.0;
/// Dopo quanto si dice che la presentazione si sta aprendo: prima sarebbe solo un lampo.
const DOPO_QUANTO_SI_DICE: Duration = Duration::from_millis(300);

/// Il nome di una presentazione arrivata con «Apri con» senza nome: dal suo tipo MIME, None se
/// il tipo non è di una presentazione.
pub(crate) fn nome_da_mime(mime: Option<&str>) -> Option<&'static str> {
    match mime? {
        MIME_PPTX => Some("presentazione.pptx"),
        MIME_PPSX => Some("presentazione.ppsx"),
        MIME_POTX => Some("presentazione.potx"),
        _ => None,
    }
}

/// Il tipo per la diagnosi: l'estensione, se è di una presentazione.
fn tipo_detto(nome: &str) -> String {
    let estensione = nome.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    if ESTENSIONI_PRESENTAZIONE.contains(&estensione.as_str()) { estensione } else { "sconosciuto".to_string() }
}

/// La presentazione aperta, o perché non si apre.
type Esito = Result<Presentazione, ErroreApertura>;

/// Una presentazione che si apre sul suo thread.
pub(crate) struct AperturaSlide {
    esito: Receiver<Esito>,
    da: Da,
    misura: usize,
    inizio: Instant,
    /// Quando lo si è chiesto: dopo `DOPO_QUANTO_SI_DICE` lo si dice sullo schermo.
    chiesto: Instant,
    nome: String,
    chiave: Option<String>,
}

/// La presentazione delle Slide.
pub(crate) struct SlideAperte {
    presentazione: Presentazione,
    pub(crate) lettore: LettoreSlide,
    nome: String,
    /// La slide delle note, fissata quando il loro foglio si apre.
    note_di: usize,
    /// La presentazione a tutto schermo, mentre c'è, con la sua barra e il pannello del relatore
    /// aperti o no.
    spettacolo: Option<Spettacolo>,
    barra: bool,
    relatore: bool,
    /// Lo schermo esterno della presentazione visto nell'ultimo frame, per dirlo quando cambia.
    esterno: Option<(u32, u32)>,
    /// Il file da cui è aperta (percorso o URI), per Condividi.
    chiave: Option<String>,
    menu: bool,
    /// Il PDF delle slide che si prepara sul suo thread.
    esportazione: Option<Esportazione>,
}

impl SlideAperte {
    /// Il percorso o l'URI del file, se si sa.
    pub(crate) fn chiave_del_file(&self) -> Option<String> {
        self.chiave.clone()
    }

    /// Il nome del file.
    pub(crate) fn nome_del_file(&self) -> &str {
        &self.nome
    }
}

/// Il PDF delle slide che si prepara: l'esito dal thread, se ha le note, quando è cominciato.
struct Esportazione {
    esito: Receiver<Result<Vec<u8>, ErrorePdf>>,
    con_note: bool,
    inizio: Instant,
}

/// I fogli delle Slide.
pub(crate) struct FogliSlide {
    pub(crate) miniature: FoglioDalFondo,
    pub(crate) note: FoglioDalFondo,
}

impl FogliSlide {
    pub(crate) fn new() -> Self {
        Self {
            miniature: FoglioDalFondo::new(egui::Id::new("foglio_miniature_slide")),
            note: FoglioDalFondo::new(egui::Id::new("foglio_note_slide")),
        }
    }

    fn chiudi(&mut self) {
        self.miniature.chiudi();
        self.note.chiudi();
    }
}

impl TascabileApp {
    /// Comincia ad aprire una presentazione sul suo thread: la lettura del pacchetto non ferma
    /// l'interfaccia. Un'apertura chiesta prima e non finita si lascia andare.
    pub(crate) fn apri_slide(&mut self, da: Da, byte: Vec<u8>, nome: &str, chiave: Option<String>, inizio: Instant) {
        let misura = byte.len();
        let (manda, esito) = std::sync::mpsc::channel();
        let risveglio = self.risveglio.clone();
        let lavoro = move || {
            let esito = Presentazione::apri(&byte);
            drop(byte);
            let _ = manda.send(esito);
            risveglio();
        };
        if let Err(e) = std::thread::Builder::new().name("slide-apertura".into()).spawn(lavoro) {
            da.scrivi(format!("{}: {misura} byte, tipo {}, thread di apertura non partito ({:?})", da.etichetta(), tipo_detto(nome), e.kind()));
            self.errore = Some(Errore::apertura("Non riesco ad aprire il file: il telefono non ha lasciato partire la lettura."));
            return;
        }
        // Come per il PDF: mentre si apre, il documento dell'Editor non riceve più niente (la
        // tastiera si chiude e i tocchi li ferma il velo di `mostra_apertura_slide`).
        self.piattaforma.hide_keyboard();
        self.fuoco_al_documento();
        if let Some(e) = self.editor_mut() {
            e.ponte.chiudi();
        }
        self.apertura_slide = Some(AperturaSlide { esito, da, misura, inizio, chiesto: Instant::now(), nome: nome.to_string(), chiave });
    }

    /// A ogni passo: se la presentazione che si apre è pronta, va nelle Slide (il documento
    /// dell'Editor e il PDF si chiudono, come aprendo un documento); se non si apre, lo dice il
    /// dialogo.
    pub(crate) fn passo_apertura_slide(&mut self) {
        let Some(a) = &self.apertura_slide else { return };
        let esito = match a.esito.try_recv() {
            Ok(esito) => esito,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err(Genere::PanicoDelLettore.into()),
        };
        let Some(a) = self.apertura_slide.take() else { return };
        let etichetta = a.da.etichetta();
        let tipo = tipo_detto(&a.nome);
        match esito {
            Ok(presentazione) => {
                let risveglio = self.risveglio.clone();
                let lettore = match LettoreSlide::new(&presentazione, egui::Id::new("lettore_slide"), move || risveglio()) {
                    Ok(l) => l,
                    Err(e) => {
                        a.da.scrivi(format!("{etichetta}: {} byte, tipo {tipo}, thread del disegno non partito ({:?})", a.misura, e.kind()));
                        self.errore =
                            Some(Errore::apertura("Non riesco ad aprire il file: il telefono non ha lasciato partire il disegno delle slide."));
                        return;
                    }
                };
                let ms = a.inizio.elapsed().as_secs_f64() * 1000.0;
                let slide = presentazione.slide();
                let perse = presentazione.perse();
                let dette = if perse > 0 { format!(", {perse} perse") } else { String::new() };
                a.da.scrivi(format!("{etichetta}: {} byte, tipo {tipo}, aperta in {ms:.0} ms, {slide} slide{dette}", a.misura));
                match perse {
                    0 => {}
                    1 => self.avviso_breve.avvisa("Una slide della presentazione non si legge e manca: forse il file è rovinato."),
                    n => self.avviso_breve.avvisa(format!("{n} slide della presentazione non si leggono e mancano: forse il file è rovinato.")),
                }
                let uscito = self.sostituisci(None);
                self.metti_da_parte(uscito);
                self.chiudi_pdf();
                self.chiudi_slide();
                self.chiudi_foglio();
                let ppsx = tipo == "ppsx";
                self.slide = Some(SlideAperte {
                    presentazione,
                    lettore,
                    nome: a.nome.clone(),
                    note_di: 0,
                    spettacolo: None,
                    barra: false,
                    relatore: false,
                    esterno: None,
                    chiave: a.chiave.clone(),
                    menu: false,
                    esportazione: None,
                });
                self.vai(Schermata::Slide);
                match a.chiave {
                    Some(chiave) => {
                        self.home.ricorda(Recente { chiave, nome: a.nome, tipo: Tipo::Presentazione, aperto_alle: adesso(), pagine: Some(slide) })
                    }
                    None => diagnosi("recenti: percorso che non è UTF-8, non ricordato"),
                }
                // Un .ppsx è salvato per presentare: parte già presentato, dall'inizio.
                if ppsx {
                    self.presenta(true);
                }
            }
            Err(e) => {
                a.da.scrivi(format!("{etichetta}: {} byte, tipo {tipo}, errore di apertura ({})", a.misura, e.genere.diagnosi()));
                self.errore = Some(Errore::apertura(format!("Non riesco ad aprire il file: {e}")));
            }
        }
    }

    /// Per le prove: una presentazione si sta aprendo.
    #[doc(hidden)]
    pub fn slide_in_apertura(&self) -> bool {
        self.apertura_slide.is_some()
    }

    /// Per le prove: le slide sullo schermo aspettano ancora il disegno (None senza Slide).
    #[doc(hidden)]
    pub fn slide_in_attesa(&self) -> Option<bool> {
        self.slide.as_ref().map(|s| s.lettore.in_attesa())
    }

    /// Per le prove: la slide che si legge (None senza Slide).
    #[doc(hidden)]
    pub fn slide_corrente(&self) -> Option<usize> {
        self.slide.as_ref().map(|s| s.lettore.corrente())
    }

    /// Per le prove: la slide che si presenta e i suoi passi fatti (None senza presentazione).
    #[doc(hidden)]
    pub fn slide_presentata(&self) -> Option<(usize, usize)> {
        self.slide.as_ref()?.spettacolo.as_ref().map(|sp| (sp.slide(), sp.passo()))
    }

    /// Per le prove: la presentazione aspetta ancora fotogrammi dal disegno.
    #[doc(hidden)]
    pub fn presentazione_in_attesa(&self) -> Option<bool> {
        self.slide.as_ref()?.spettacolo.as_ref().map(Spettacolo::in_attesa)
    }

    /// Per le prove: la penna della presentazione è accesa.
    #[doc(hidden)]
    pub fn penna_della_presentazione(&self) -> Option<bool> {
        self.slide.as_ref()?.spettacolo.as_ref().map(Spettacolo::penna)
    }

    /// Per le prove: lo schermo della presentazione è nero.
    #[doc(hidden)]
    pub fn nero_della_presentazione(&self) -> Option<bool> {
        self.slide.as_ref()?.spettacolo.as_ref().map(Spettacolo::nero)
    }

    /// Chiude la presentazione delle Slide, se c'è, coi suoi fogli e le misure dei disegni nella
    /// diagnosi.
    pub(crate) fn chiudi_slide(&mut self) {
        self.esci_dalla_presentazione();
        self.fogli_slide.chiudi();
        if let Some(s) = self.slide.take() {
            diagnosi(format!("slide chiuse: {}", s.lettore.riga_misure()));
        }
    }

    /// «Presenta»: a tutto schermo dalla slide che si legge (o dall'inizio). Il telefono tiene lo
    /// schermo acceso, nasconde le barre e passa i tasti del volume.
    fn presenta(&mut self, dall_inizio: bool) {
        let Some(s) = self.slide.as_mut() else { return };
        let da = if dall_inizio { s.presentazione.prima_da_presentare() } else { s.lettore.corrente() };
        let risveglio = self.risveglio.clone();
        match Spettacolo::new(&s.presentazione, da, move || risveglio()) {
            Ok(sp) => {
                diagnosi(format!("presentazione: dalla slide {} di {}", da + 1, s.presentazione.slide()));
                s.spettacolo = Some(sp);
                s.barra = false;
                s.relatore = false;
                self.fogli_slide.chiudi();
                self.piattaforma.presentazione(true);
            }
            Err(e) => {
                diagnosi(format!("presentazione: thread del disegno non partito ({:?})", e.kind()));
                self.errore = Some(Errore::apertura("Non riesco a presentare: il telefono non ha lasciato partire il disegno delle slide."));
            }
        }
    }

    /// Esce dalla presentazione, se c'è: la lettura torna sulla slide presentata, il telefono
    /// torna normale. Vero se c'era.
    pub(crate) fn esci_dalla_presentazione(&mut self) -> bool {
        let Some(s) = self.slide.as_mut() else { return false };
        let Some(sp) = s.spettacolo.take() else { return false };
        s.barra = false;
        s.relatore = false;
        let i = sp.slide();
        s.lettore.vai_alla_slide(i);
        diagnosi(format!("presentazione finita alla slide {} di {}: {}", i + 1, s.presentazione.slide(), sp.riga_misure()));
        self.piattaforma.presentazione(false);
        true
    }

    /// Indietro nelle Slide: una presentazione che si apre si lascia andare, poi i fogli si
    /// chiudono. Vero se Indietro è stato usato qui.
    pub(crate) fn indietro_slide(&mut self) -> bool {
        if self.apertura_slide.take().is_some() {
            diagnosi("indietro: apertura delle slide annullata");
            return true;
        }
        if self.fogli_slide.miniature.indietro() {
            diagnosi("indietro: miniature delle slide chiuse");
            return true;
        }
        if self.fogli_slide.note.indietro() {
            diagnosi("indietro: note chiuse");
            return true;
        }
        if let Some(s) = self.slide.as_mut()
            && std::mem::take(&mut s.menu)
        {
            diagnosi("indietro: menu delle slide chiuso");
            return true;
        }
        // Nella presentazione il pannello del relatore, poi la barra, poi la presentazione stessa.
        if let Some(s) = self.slide.as_mut()
            && s.spettacolo.is_some()
        {
            if std::mem::take(&mut s.relatore) {
                diagnosi("indietro: note del relatore chiuse");
                return true;
            }
            if std::mem::take(&mut s.barra) {
                diagnosi("indietro: barra della presentazione chiusa");
                return true;
            }
        }
        self.esci_dalla_presentazione()
    }

    /// Indietro dalle Slide: alla Home.
    pub(crate) fn esci_dalle_slide(&mut self) {
        self.chiudi_poi(crate::Dopo::Home);
    }

    /// La schermata delle Slide, dentro la zona libera da barre di sistema: la barra alta, le
    /// slide e la barra in fondo con Presenta, Miniature e Note; o la presentazione.
    pub(crate) fn mostra_slide(&mut self, ui: &mut egui::Ui) {
        if self.slide.as_ref().is_some_and(|s| s.spettacolo.is_some()) {
            self.mostra_presentazione(ui.ctx());
            return;
        }
        let Some(s) = self.slide.as_mut() else {
            self.vai(Schermata::Home);
            return;
        };
        let zona = ui.max_rect();
        let barra = Rect::from_min_size(zona.min, vec2(zona.width(), tema::BARRA_ALTA));
        let nome = home::nome_da_mostrare(&s.nome, Tipo::Presentazione).to_string();
        let sotto = format!("Slide · {} slide", s.presentazione.slide());
        let r = ui
            .scope_builder(UiBuilder::new().max_rect(barra), |ui| {
                BarraAlta::new(&nome).indietro(INDIETRO).sottotitolo(&sotto).azione(PulsanteIcona::new(icone::MORE_VERT, MENU)).mostra(ui)
            })
            .inner;
        if r.azione.is_some() {
            diagnosi("menu delle slide: aperto");
            s.menu = true;
        }
        let voci = BarraVoci::posto(zona);
        s.lettore.spazio_sotto(zona.bottom() - voci.top() + SOPRA_LA_BARRA);
        let slide = Rect::from_min_max(pos2(zona.left(), barra.bottom()), zona.max);
        ui.scope_builder(UiBuilder::new().max_rect(slide), |ui| s.lettore.mostra(ui));
        let scelta = BarraVoci::new().voce(icone::SLIDESHOW, PRESENTA).voce(icone::GRID_VIEW, MINIATURE).voce(icone::NOTES, NOTE).mostra(ui, voci);
        if s.esportazione.is_some() {
            pillola(ui, zona.center(), PREPARO_IL_PDF);
        }
        match scelta {
            Some(0) => self.presenta(false),
            Some(1) => self.apri_miniature_slide(),
            Some(_) => self.apri_note(),
            None => {}
        }
        self.mostra_menu_slide(ui.ctx(), zona);
        if r.indietro {
            self.esci_dalle_slide();
        }
    }

    /// Il menu delle Slide: Presenta dall'inizio, Condividi, il timestamp, i due PDF.
    fn mostra_menu_slide(&mut self, ctx: &egui::Context, zona: Rect) {
        if !self.slide.as_ref().is_some_and(|s| s.menu) {
            return;
        }
        let data = self.voce_della_data();
        let scelta = Menu::new(egui::Id::new("menu_slide"))
            .primo(icone::SLIDESHOW, PRESENTA_DALL_INIZIO)
            .voce(icone::SHARE, CONDIVIDI)
            .voce(icone::SCHEDULE, data)
            .separatore()
            .voce(icone::PICTURE_AS_PDF, ESPORTA_PDF)
            .voce(icone::NOTES, PDF_CON_NOTE)
            .mostra(ctx, pos2(zona.right() - MENU_DESTRA, zona.top() + MENU_SOTTO));
        let Some(scelta) = scelta else { return };
        if let Some(s) = self.slide.as_mut() {
            s.menu = false;
        }
        match scelta {
            SceltaMenu::Voce(0) => self.presenta(true),
            SceltaMenu::Voce(1) => self.condividi_presentazione(),
            SceltaMenu::Voce(2) => self.data_dal_menu(ctx),
            SceltaMenu::Voce(3) => self.esporta_pdf_slide(false),
            SceltaMenu::Voce(4) => self.esporta_pdf_slide(true),
            _ => {}
        }
    }

    /// «Condividi»: il file della presentazione com'è sul telefono, come dai recenti.
    fn condividi_presentazione(&mut self) {
        let Some(s) = self.slide.as_ref() else { return };
        match s.chiave.clone() {
            Some(chiave) => {
                let r = Recente { chiave, nome: s.nome.clone(), tipo: Tipo::Presentazione, aperto_alle: 0, pagine: None };
                self.condividi_recente(&r);
            }
            None => {
                let testo = "Non riesco a condividere la presentazione: non so più da dove era arrivata.".to_string();
                self.errore = Some(Errore { titolo: crate::NON_CONDIVISO, testo });
            }
        }
    }

    /// «Esporta in PDF» e «PDF con le note»: il PDF si prepara sul thread `slide-pdf`; quando è
    /// pronto va in Documenti/Tascabile (`passo_pdf_slide`).
    fn esporta_pdf_slide(&mut self, con_note: bool) {
        let Some(s) = self.slide.as_mut() else { return };
        if s.esportazione.is_some() {
            return;
        }
        let (manda, esito) = std::sync::mpsc::channel();
        let doc = s.presentazione.clone();
        let risveglio = self.risveglio.clone();
        let lavoro = move || {
            let _ = manda.send(doc.pdf(con_note));
            risveglio();
        };
        match std::thread::Builder::new().name("slide-pdf".into()).spawn(lavoro) {
            Ok(_) => s.esportazione = Some(Esportazione { esito, con_note, inizio: Instant::now() }),
            Err(e) => diagnosi(format!("slide: pdf non esportato, thread non partito ({:?})", e.kind())),
        }
    }

    /// A ogni passo: il PDF delle slide pronto va in Documenti/Tascabile, col nome della
    /// presentazione («… - note» quello con le note), e l'avviso lo dice con Condividi.
    pub(crate) fn passo_pdf_slide(&mut self) {
        let Some(s) = self.slide.as_mut() else { return };
        let Some(e) = &s.esportazione else { return };
        let esito = match e.esito.try_recv() {
            Ok(esito) => esito,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err(ErrorePdf::Panico),
        };
        let Some(e) = s.esportazione.take() else { return };
        let cosa = if e.con_note { "pdf con le note" } else { "pdf" };
        let byte = match esito {
            Ok(byte) => byte,
            Err(errore) => {
                diagnosi(format!("slide: {cosa} non esportato ({})", errore.diagnosi()));
                self.errore = Some(Errore { titolo: crate::NON_SALVATO, testo: format!("Non riesco a fare il PDF: {errore}") });
                return;
            }
        };
        let base = home::nome_da_mostrare(&s.nome, Tipo::Presentazione).to_string();
        let base = if e.con_note { format!("{base} - note") } else { base };
        let cartella = tascabile_files::cartella_tascabile(&self.piattaforma.radice_archivio());
        let scritto = std::fs::create_dir_all(&cartella).and_then(|_| salva::scrivi_nuovo(&cartella, &base, "pdf", &byte));
        match scritto {
            Ok(percorso) => {
                let ms = e.inizio.elapsed().as_millis();
                diagnosi(format!("slide: {cosa} esportato, {} byte, in {ms} ms", byte.len()));
                let file = percorso.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                self.avviso_breve.avvisa_con_azione(format!("PDF pronto in Documenti/Tascabile: {file}"), crate::CONDIVIDI_PDF);
                self.pdf_pronto = Some(percorso);
            }
            Err(errore) => {
                diagnosi(format!("slide: {cosa} non scritto ({:?})", errore.kind()));
                let testo = "Non riesco a salvare il PDF in Documenti/Tascabile.".to_string();
                self.errore = Some(Errore { titolo: crate::NON_SALVATO, testo });
            }
        }
    }

    /// Per le prove: il PDF delle slide si sta preparando.
    #[doc(hidden)]
    pub fn pdf_delle_slide_in_corso(&self) -> bool {
        self.slide.as_ref().is_some_and(|s| s.esportazione.is_some())
    }

    /// La presentazione su tutto lo schermo, sopra il resto; con la barra aperta, la barra in fondo
    /// e sopra di lei il tempo e l'ora; col relatore, il suo pannello.
    fn mostra_presentazione(&mut self, ctx: &egui::Context) {
        let schermo = ctx.content_rect();
        let griglia = self.fogli_slide.miniature.aperto();
        let esterno = self.piattaforma.schermo_esterno();
        let Some(s) = self.slide.as_mut() else { return };
        let Some(sp) = s.spettacolo.as_mut() else { return };
        // Uno schermo esterno: le slide vanno lì, e sul telefono resta chi parla, col pannello del
        // relatore aperto.
        if esterno != s.esterno {
            match esterno {
                Some((w, h)) => {
                    diagnosi(format!("presentazione: schermo esterno {w} × {h}"));
                    s.relatore = true;
                }
                None => diagnosi("presentazione: schermo esterno staccato"),
            }
            s.esterno = esterno;
        }
        sp.proiettore(esterno);
        let voci = BarraVoci::posto(schermo);
        // Il pannello del relatore sopra la fascia della barra (e sopra il tempo, con la barra).
        let sotto_il_pannello = if s.barra { voci.top() - 56.0 } else { schermo.bottom() - 80.0 };
        let alto = (schermo.height() * ALTEZZA_RELATORE).min(sotto_il_pannello - schermo.top() - 12.0).max(0.0);
        let pannello = Rect::from_min_max(pos2(schermo.left() + 12.0, sotto_il_pannello - alto), pos2(schermo.right() - 12.0, sotto_il_pannello));
        sp.escludi(s.barra.then_some(voci));
        // Con la griglia aperta i tocchi sono tutti del foglio.
        let ignorate = if griglia {
            vec![schermo]
        } else if s.relatore {
            vec![pannello]
        } else {
            Vec::new()
        };
        sp.ignora(ignorate);
        let note = s.relatore.then(|| s.presentazione.note(sp.slide()));
        let (w, h) = (s.presentazione.documento().slide_size.width as f32, s.presentazione.documento().slide_size.height as f32);
        let mut scelta = None;
        let esito = egui::Area::new(egui::Id::new("presentazione"))
            .order(egui::Order::Foreground)
            .fixed_pos(schermo.min)
            .show(ctx, |ui| {
                let esito = ui.scope_builder(UiBuilder::new().max_rect(schermo), |ui| sp.mostra(ui)).inner;
                if let Some(note) = &note {
                    pannello_del_relatore(ui, pannello, note, sp.anteprima(), w / h.max(1.0));
                }
                if s.barra {
                    let adesso = ui.input(|i| i.time);
                    let tempo = sp.tempo(adesso).map_or_else(|| "0:00".to_string(), |t| minuti_e_secondi(t as u64));
                    let ora = Locale::da_utc(adesso_utc());
                    let testo = format!("{tempo} · ore {}:{:02}", ora.ora, ora.minuti);
                    let font = Carattere::Grassetto.font(ui.ctx(), 15.0);
                    let g = ui.painter().layout_no_wrap(testo.clone(), font, egui::Color32::WHITE);
                    let pillola = Rect::from_center_size(pos2(voci.center().x, voci.top() - 30.0), vec2(g.size().x + 28.0, 36.0));
                    ui.painter().rect_filled(pillola, 18.0, tema::INDICATORE);
                    ui.painter().galley(pillola.center() - g.size() / 2.0, g, egui::Color32::PLACEHOLDER);
                    ui.interact(pillola, egui::Id::new("tempo_presentazione"), egui::Sense::hover())
                        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &testo));
                    // Il tempo cambia ogni secondo.
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
                    let penna = sp.penna().then_some(1);
                    scelta = BarraVoci::new()
                        .accesa(penna)
                        .voce(icone::CLOSE, ESCI)
                        .voce(icone::DRAW, PENNA)
                        .voce(icone::INK_ERASER, CANCELLA)
                        .voce(icone::DARK_MODE, NERO)
                        .voce(icone::NOTES, NOTE)
                        .voce(icone::GRID_VIEW, SLIDE)
                        .mostra(ui, voci);
                }
                esito
            })
            .inner;
        if let Some(img) = sp.da_proiettare() {
            self.piattaforma.proietta(img.pixel, img.larghezza, img.altezza);
        }
        if esito.barra {
            s.barra = !s.barra;
        }
        match scelta {
            Some(0) => {
                self.esci_dalla_presentazione();
                return;
            }
            Some(1) => sp.alterna_penna(),
            Some(2) => sp.cancella_tratti(),
            Some(3) => {
                sp.alterna_nero();
                s.barra = false;
            }
            Some(4) => {
                s.relatore = !s.relatore;
                if s.relatore {
                    diagnosi(format!("presentazione: note del relatore aperte alla slide {}", sp.slide() + 1));
                }
            }
            Some(_) => {
                // La griglia segna la slide che si presenta.
                s.lettore.vai_alla_slide(sp.slide());
                s.barra = false;
                self.fogli_slide.miniature.apri();
            }
            None => {}
        }
        if esito.esci {
            self.esci_dalla_presentazione();
        }
    }

    fn apri_miniature_slide(&mut self) {
        let Some(s) = self.slide.as_ref() else { return };
        diagnosi(format!("slide: miniature aperte, {} slide", s.presentazione.slide()));
        self.fogli_slide.miniature.apri();
    }

    /// Il foglio delle note della slide che si legge; nella diagnosi quanti caratteri, mai il testo.
    fn apri_note(&mut self) {
        let Some(s) = self.slide.as_mut() else { return };
        s.note_di = s.lettore.corrente();
        let caratteri = s.presentazione.note(s.note_di).chars().count();
        diagnosi(format!("slide: note della slide {} di {}, {caratteri} caratteri", s.note_di + 1, s.presentazione.slide()));
        self.fogli_slide.note.apri();
    }

    /// I fogli delle Slide: le miniature (un tocco porta alla slide e chiude il foglio) e le note.
    /// Vanno chiamati a ogni frame.
    pub(crate) fn mostra_fogli_slide(&mut self, ctx: &egui::Context) {
        if self.schermata != Schermata::Slide || self.slide.is_none() {
            self.fogli_slide.chiudi();
        }
        let scelta = match self.slide.as_mut() {
            Some(s) => {
                let scelta = self.fogli_slide.miniature.mostra(ctx, MINIATURE, |ui| s.lettore.mostra_miniature(ui)).flatten();
                // Nella presentazione il lettore non si mostra: le miniature le chiede la griglia.
                if s.spettacolo.is_some() && self.fogli_slide.miniature.aperto() {
                    s.lettore.aggiorna_miniature(ctx);
                }
                scelta
            }
            None => self.fogli_slide.miniature.mostra(ctx, MINIATURE, |_| None).flatten(),
        };
        if let Some(i) = scelta
            && let Some(s) = self.slide.as_mut()
        {
            match s.spettacolo.as_mut() {
                Some(sp) => {
                    sp.vai(i);
                    diagnosi(format!("presentazione: salto alla slide {} di {}", i + 1, s.presentazione.slide()));
                }
                None => {
                    s.lettore.vai_alla_slide(i);
                    diagnosi(format!("slide: miniatura, salto alla slide {} di {}", i + 1, s.presentazione.slide()));
                }
            }
            self.fogli_slide.miniature.chiudi();
        }
        let (titolo, testo) = match self.slide.as_ref() {
            Some(s) => (format!("{NOTE} · slide {}", s.note_di + 1), s.presentazione.note(s.note_di)),
            None => (NOTE.to_string(), String::new()),
        };
        self.fogli_slide.note.mostra(ctx, &titolo, |ui| {
            if testo.trim().is_empty() {
                ui.label(RichText::new(NESSUNA_NOTA).font(Carattere::Testo.font(ui.ctx(), 16.0)).color(tema::MATITA));
            } else {
                ui.label(RichText::new(testo.as_str()).font(Carattere::Testo.font(ui.ctx(), 18.0)).color(tema::INCHIOSTRO));
            }
        });
    }

    /// Mentre la presentazione si apre, un velo trasparente ferma i tocchi sullo schermo di
    /// prima; se l'apertura dura più di un attimo, «Apro la presentazione…» in una pillola scura.
    pub(crate) fn mostra_apertura_slide(&self, ctx: &egui::Context) {
        let Some(a) = &self.apertura_slide else { return };
        let zona = ctx.content_rect();
        egui::Area::new(egui::Id::new("apertura_slide")).order(egui::Order::Foreground).fixed_pos(zona.min).show(ctx, |ui| {
            ui.allocate_rect(Rect::from_min_size(zona.min, zona.size()), egui::Sense::click_and_drag());
            let passato = a.chiesto.elapsed();
            if passato < DOPO_QUANTO_SI_DICE {
                ctx.request_repaint_after(DOPO_QUANTO_SI_DICE - passato);
                return;
            }
            let font = Carattere::Grassetto.font(ui.ctx(), 15.0);
            let g = ui.painter().layout_no_wrap(APRO.to_string(), font, egui::Color32::WHITE);
            let pillola = Rect::from_center_size(zona.center(), vec2(g.size().x + 32.0, 44.0));
            ui.painter().rect_filled(pillola, 22.0, tema::INDICATORE);
            ui.painter().galley(pillola.center() - g.size() / 2.0, g, egui::Color32::PLACEHOLDER);
            ui.interact(pillola, egui::Id::new("apertura_slide_testo"), egui::Sense::hover())
                .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, APRO));
        });
    }
}

/// Una pillola scura con un testo, in mezzo a `centro`: «Preparo il PDF…».
fn pillola(ui: &egui::Ui, centro: egui::Pos2, testo: &str) {
    let font = Carattere::Grassetto.font(ui.ctx(), 15.0);
    let g = ui.painter().layout_no_wrap(testo.to_string(), font, egui::Color32::WHITE);
    let r = Rect::from_center_size(centro, vec2(g.size().x + 32.0, 44.0));
    ui.painter().rect_filled(r, 22.0, tema::INDICATORE);
    ui.painter().galley(r.center() - g.size() / 2.0, g, egui::Color32::PLACEHOLDER);
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
}

/// Il pannello del relatore, scuro sopra la presentazione: in alto «Dopo» con quello che viene
/// col prossimo «avanti» (o «Fine»), sotto le note della slide, grandi, che scorrono.
fn pannello_del_relatore(ui: &mut egui::Ui, pannello: Rect, note: &str, dopo: Option<egui::TextureId>, rapporto: f32) {
    ui.painter().rect_filled(pannello, 16.0, tema::SCURO.gamma_multiply(0.94));
    ui.allocate_rect(pannello, egui::Sense::click_and_drag());
    let interno = pannello.shrink(14.0);
    let font = Carattere::Grassetto.font(ui.ctx(), 13.0);
    let g = ui.painter().layout_no_wrap(DOPO.to_string(), font, egui::Color32::from_gray(170));
    let etichetta = Rect::from_min_size(interno.min, g.size());
    ui.painter().galley(etichetta.min, g, egui::Color32::PLACEHOLDER);
    ui.interact(etichetta, egui::Id::new("relatore_dopo"), egui::Sense::hover())
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, DOPO));
    let anteprima =
        Rect::from_min_size(pos2(interno.left(), etichetta.bottom() + 6.0), vec2(LARGHEZZA_ANTEPRIMA, LARGHEZZA_ANTEPRIMA / rapporto.max(0.1)));
    match dopo {
        Some(t) => {
            ui.painter().image(t, anteprima, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), egui::Color32::WHITE);
        }
        None => {
            ui.painter().rect_filled(anteprima, 4.0, egui::Color32::BLACK);
            let font = Carattere::Testo.font(ui.ctx(), 13.0);
            ui.painter().text(anteprima.center(), egui::Align2::CENTER_CENTER, FINE_DOPO, font, egui::Color32::from_gray(170));
        }
    }
    // Le note a destra dell'anteprima, fino in fondo al pannello.
    let testo = Rect::from_min_max(pos2(anteprima.right() + 14.0, interno.top()), interno.max);
    ui.scope_builder(UiBuilder::new().max_rect(testo), |ui| {
        egui::ScrollArea::vertical().id_salt("note_del_relatore").auto_shrink([false, false]).show(ui, |ui| {
            let (parole, colore) = if note.trim().is_empty() { (NESSUNA_NOTA, egui::Color32::from_gray(170)) } else { (note, egui::Color32::WHITE) };
            ui.label(RichText::new(parole).font(Carattere::Testo.font(ui.ctx(), 19.0)).color(colore));
        });
    });
}

/// «3:05», «1:02:40»: il tempo della presentazione.
fn minuti_e_secondi(secondi: u64) -> String {
    let (ore, minuti, secondi) = (secondi / 3600, secondi / 60 % 60, secondi % 60);
    if ore > 0 { format!("{ore}:{minuti:02}:{secondi:02}") } else { format!("{minuti}:{secondi:02}") }
}

/// I secondi UTC di adesso, per l'ora sulla barra della presentazione.
fn adesso_utc() -> u64 {
    adesso()
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn il_tempo_della_presentazione() {
        assert_eq!(minuti_e_secondi(0), "0:00");
        assert_eq!(minuti_e_secondi(185), "3:05");
        assert_eq!(minuti_e_secondi(3760), "1:02:40");
    }
}
