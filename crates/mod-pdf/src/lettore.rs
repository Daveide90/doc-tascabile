//! Il lettore del bozzetto 7: le pagine bianche con l'ombra sul tavolo, una sotto l'altra, che
//! scorrono in verticale; il pizzico da 0,5 a 6 volte attorno al punto fra le dita e il doppio
//! tocco fra la larghezza dello schermo e l'ingrandimento; l'indicatore «2 / 12» che compare
//! scorrendo.
//!
//! Le immagini delle pagine le disegna un pool nostro di due thread (`RenderPool` del motore,
//! creato sui byte da mostrare: il pool del motore resta fermo, ricerca PDFCraft, verifica). A ogni
//! frame si chiede quello che manca, in ordine di urgenza: le pagine sullo schermo dalla più
//! vicina al centro, le loro tessere nitide se lo zoom le vuole, poi le pagine vicine. Il pool
//! consegna solo quello che è ancora nell'ultima coda; le tessere portano nel tag la generazione
//! dello zoom, e quelle di uno zoom passato si scartano. Durante il pizzico si ingrandiscono le
//! immagini che ci sono; finché le nuove non arrivano restano quelle vecchie, stirate. Le texture
//! lontane si liberano, con un tetto di memoria per le pagine e uno per le tessere.
//!
//! Per orientarsi (D3): le miniature in una griglia, disegnate a bassa scala dallo stesso pool e
//! tenute con un tetto loro; il salto a una pagina o a un punto di una pagina (indice, link, vai a
//! pagina); la ricerca, coi testi delle pagine chiesti allo stesso pool dopo le immagini (la coda
//! è una sola: chi la manda senza i pixel attesi li perde, ricerca PDFCraft, verifica), i
//! risultati in giallo tenue e l'attuale più forte; i link toccati, che portano alla pagina o
//! tornano a chi usa il lettore.
//!
//! Per prendere il testo (D4): la pressione lunga su una parola la seleziona in azzurro, con le
//! maniglie a goccia da trascinare (la lente ingrandisce sotto il dito) e la barra del contesto
//! scura con Copia e Tutto (il testo della pagina); il testo da copiare torna a chi usa il
//! lettore, che lo mette negli appunti. Il testo della pagina viene dal pool, come per la ricerca.
//! Per la ripresa: lo zoom e il punto che si leggeva, da ricordare e da rimettere.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{
    Align, Color32, Id, Pos2, Rect, Sense, Stroke, StrokeKind, TextureHandle, TextureId, TextureOptions, Ui, Vec2, WidgetInfo, WidgetType, pos2, vec2,
};
use pdfcraft_render::{PageText, RenderPool, RenderRequest, RequestKind, Tile};
use tascabile_touch::tema::{self, Carattere};
use tascabile_touch::{BarraContesto, Maniglia, Maniglie, Trascinamento, icone, lente_con};

use crate::apertura::Misura;
#[cfg(test)]
use crate::apertura::Pdf;
use crate::disegno::{Rettangolo, immagine};
use crate::geometria::{CIMA, Impaginazione, ZOOM_MASSIMO, ZOOM_MINIMO};
use crate::modifica::Toccate;
use crate::motore::Istantanea;
use crate::navigazione::{Collegamento, Destinazione};
use crate::ricerca::{RettangoloPt, Ricerca, TestoNormale};
use crate::selezione::{Bordo, BordoPt, Selezione};

/// I thread del pool nostro.
pub const THREAD: usize = 2;
/// Le pagine sopra e sotto quelle sullo schermo che si disegnano prima, per lo scorrimento.
pub const VICINE: usize = 2;
/// Il tetto della memoria delle immagini intere delle pagine (circa undici A4 larghe quanto il
/// telefono), e quello delle tessere, vecchie comprese.
pub const TETTO_PAGINE: usize = 64 << 20;
pub const TETTO_TESSERE: usize = 64 << 20;
/// I risultati presi dal pool in un frame al massimo, e i loro byte: caricare una texture costa.
const RISULTATI_PER_FRAME: usize = 4;
const BYTE_PER_FRAME: usize = 24 << 20;
/// Dopo l'ultimo cambio di zoom senza dita (la rotella sul PC), tanto prima delle immagini
/// nitide; col dito vale il momento in cui si alzano le dita.
const CALMA_ZOOM: f64 = 0.25;
/// L'indicatore della pagina resta tanto dopo l'ultimo movimento, poi sfuma.
const DURATA_INDICATORE: f64 = 1.2;
const SFUMATURA_INDICATORE: f32 = 0.3;
/// Il tag delle richieste delle pagine intere; quello delle tessere è la generazione dello zoom.
const INTERA: u64 = 1 << 62;
/// Il tag delle miniature e quello dei testi (non `1 << 60`, che il motore tiene per la stampa).
const MINIATURA: u64 = 1 << 61;
const TESTO: u64 = 1 << 59;
/// Le miniature: il tetto della loro memoria, le colonne della griglia, lo spazio fra due celle e
/// l'altezza del numero sotto.
pub const TETTO_MINIATURE: usize = 24 << 20;
pub const COLONNE_MINIATURE: usize = 3;
const SPAZIO_MINIATURE: f32 = 12.0;
const NUMERO_MINIATURA: f32 = 30.0;
/// Attorno alla griglia, perché l'ombra e il bordo blu non finiscano tagliati dal foglio.
const ATTORNO_MINIATURE: Vec2 = vec2(4.0, 6.0);
/// I testi delle pagine tenuti per la ricerca, con un tetto stimato (circa 100 byte per
/// carattere, ricerca PDFCraft sezione 4), e quanti se ne chiedono al pool alla volta.
pub const TETTO_TESTI: usize = 48 << 20;
const BYTE_PER_CARATTERE: usize = 100;
const TESTI_IN_CODA: usize = 32;
/// Un link si prende anche col dito un poco fuori dal suo rettangolo, in dp.
const BORDO_LINK: f32 = 8.0;
/// Un salto a un punto di una pagina lo lascia tanto sotto la cima della vista, in dp.
const SOPRA_IL_PUNTO: f32 = 16.0;
/// La pressione lunga prende una parola anche col dito un poco fuori dalle lettere, in dp.
const BORDO_PAROLA: f32 = 6.0;
/// Col dito fermo non arrivano eventi: il frame dopo arriva entro tanto, perché la pressione
/// lunga si veda senza muovere il dito.
const ATTESA_PRESSIONE: Duration = Duration::from_millis(50);
/// Con una maniglia vicino al bordo della vista la pagina scorre da sola: la fascia del bordo e il
/// passo per frame, in dp (come in Word).
const BORDO_SCORRIMENTO: f32 = 48.0;
const PASSO_SCORRIMENTO: f32 = 8.0;
/// Le etichette della barra della selezione.
pub const COPIA: &str = "Copia";
pub const TUTTO: &str = "Tutto";

/// Dove portare la vista al frame dopo.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Salto {
    /// La pagina, in cima o al punto in pt dall'alto.
    Pagina(usize, Option<f32>),
    /// Il punto della pagina, con la x in frazione della sua larghezza (al centro della vista) e
    /// la y in pt dall'alto (poco sotto la cima della vista): la ripresa.
    Punto(usize, f32, f32),
    /// Il punto della pagina (x in frazione della larghezza, y in pt dall'alto) al centro della
    /// vista: il telefono girato, che tiene sullo schermo quello che si guardava.
    Centro(usize, f32, f32),
    /// In fondo al documento: il telefono girato quando si era in fondo.
    Fondo,
    /// Il risultato attuale della ricerca.
    Risultato,
    /// Il rettangolo della pagina (in pt), da vedere tutto se non si vede già: il campo che si
    /// compila.
    Rettangolo(usize, [f32; 4]),
}

/// Il conto della ricerca per la barra: il risultato attuale (da 0), quanti finora, se tutte le
/// pagine sono state lette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Conteggio {
    pub attuale: Option<usize>,
    pub totale: usize,
    pub finita: bool,
}

/// Una ricerca appena finita, per la diagnosi: quanti risultati in quante pagine, in quanto tempo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RicercaFinita {
    pub risultati: usize,
    pub pagine: usize,
    pub ms: u64,
}
/// Mentre si aspetta il pool, il frame dopo arriva entro tanto.
const ATTESA: Duration = Duration::from_millis(16);

/// Un'immagine sullo schermo: la texture e quale parte della pagina copre, a quale scala.
struct Immagine {
    tex: TextureHandle,
    /// Pixel del dispositivo per pt.
    scala: f32,
    px: Rettangolo,
    byte: usize,
}

/// La prima pagina disegnata dopo l'apertura, per la diagnosi.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrimaPagina {
    /// Dalla creazione del lettore a quando l'immagine arriva, in ms.
    pub dopo_ms: f32,
    /// Quanto ci ha messo il motore a disegnarla, in ms.
    pub disegno_ms: u32,
    pub larghezza: u32,
    pub altezza: u32,
}

/// Quello che è successo in un frame del lettore, per la diagnosi dell'app.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Esito {
    pub prima_pagina: Option<PrimaPagina>,
    /// Lo zoom nuovo, quando diventa nitido.
    pub zoom: Option<f32>,
    /// Il motore non ha disegnato una pagina: il genere, mai il messaggio.
    pub errore: Option<&'static str>,
    /// Un link toccato: uno a una pagina il lettore l'ha già seguito, gli altri li decide chi
    /// usa il lettore.
    pub link: Option<Destinazione>,
    pub ricerca_finita: Option<RicercaFinita>,
    /// «Copia» nella barra della selezione: il testo, che chi usa il lettore mette negli appunti.
    pub copia: Option<String>,
    /// Una selezione nuova o cambiata (pressione lunga, maniglia lasciata, «Tutto»): quanti
    /// caratteri, per la diagnosi.
    pub selezionati: Option<usize>,
    /// «Tutto»: la selezione è tutto il testo della pagina.
    pub tutta: bool,
    /// Una pressione lunga su una pagina senza testo (una scansione).
    pub senza_testo: bool,
    /// Un tocco su una pagina che non ha chiuso una selezione né seguito un link: la pagina e il
    /// punto in pt nella pagina come si vede (per compilare).
    pub tocco: Option<(usize, Pos2)>,
    /// Una delle azioni date con [`Lettore::azioni_della_selezione`] toccata sulla barra della
    /// selezione, o un suo colore.
    pub scelta_selezione: Option<SceltaSelezione>,
}

/// Le azioni della barra della selezione al posto di Copia e Tutto (il modo Annota: Evidenzia,
/// Sottolinea, Barra) e, sotto, i colori.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AzioniSelezione {
    /// (icona, nome).
    pub azioni: Vec<(&'static str, &'static str)>,
    /// (colore, nome).
    pub colori: Vec<(Color32, &'static str)>,
    pub colore: Option<usize>,
}

/// Che cosa si è toccato sulla barra della selezione con le azioni date.
#[derive(Debug, Clone, PartialEq)]
pub enum SceltaSelezione {
    /// L'azione `indice` sul testo scelto: la pagina e le righe (pt della pagina come si vede).
    /// La selezione si chiude.
    Azione { indice: usize, pagina: usize, righe: Vec<RettangoloPt> },
    /// Il colore `indice`: la selezione resta.
    Colore(usize),
}

/// I conti dei disegni, per la diagnosi alla chiusura.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Conti {
    pub pagine: u32,
    pub tessere: u32,
    pub miniature: u32,
    /// I testi delle pagine letti per la ricerca.
    pub testi: u32,
    /// I testi portati alla forma del confronto della ricerca: una volta per pagina.
    pub normalizzati: u32,
    pub ms_totali: u64,
    pub ms_massimo: u32,
    /// La memoria delle texture più alta vista, in byte.
    pub byte_massimi: usize,
}

/// Lo stato della griglia di «Organizza pagine»: le pagine scelte e quelle prese col dito.
#[derive(Debug, Default)]
pub struct Organizza {
    /// Le pagine scelte (da 0), in ordine.
    pub scelte: std::collections::BTreeSet<usize>,
    presa: Option<Presa>,
}

/// Le pagine prese con la pressione lunga, dove sta il dito e il posto sotto di lui.
#[derive(Debug)]
struct Presa {
    pagine: Vec<usize>,
    dito: Pos2,
    posto: Option<usize>,
}

impl Organizza {
    /// Si tiene una pagina col dito: l'area della griglia non deve scorrere col dito.
    pub fn tiene(&self) -> bool {
        self.presa.is_some()
    }

    /// Lascia la pagina presa senza spostarla (Indietro): vero se ce n'era una.
    pub fn lascia(&mut self) -> bool {
        self.presa.take().is_some()
    }
}

/// Quello che è successo nella griglia in un frame.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct EsitoGriglia {
    /// La pagina toccata.
    pub toccata: Option<usize>,
    /// Quante pagine si sono prese col dito adesso.
    pub prese: Option<usize>,
    /// Le pagine lasciate in un posto nuovo, e il posto (nell'ordine di adesso).
    pub spostate: Option<(Vec<usize>, usize)>,
}

/// Il segno della scelta nell'angolo in alto a destra della pagina: il tondo blu con la spunta, o
/// vuoto col bordo grigio.
fn dipingi_spunta(painter: &egui::Painter, ctx: &egui::Context, pagina: Rect, scelta: bool) {
    let centro = pagina.right_top() + vec2(-14.0, 14.0);
    if scelta {
        painter.circle_filled(centro, 11.0, tema::BLU);
        let g = painter.layout_no_wrap(icone::CHECK.to_string(), Carattere::Icona.font(ctx, 16.0), Color32::WHITE);
        painter.galley(centro - g.size() / 2.0, g, Color32::PLACEHOLDER);
    } else {
        painter.circle(centro, 10.0, Color32::WHITE.gamma_multiply(0.9), Stroke::new(1.5, tema::MATITA));
    }
}

pub struct Lettore {
    id: Id,
    pool: RenderPool,
    /// I byte che disegna il pool: quando l'istantanea ne porta di nuovi, il pool si rifà.
    display: Arc<Vec<u8>>,
    /// Le pagine cambiate da una modifica, con la misura di prima: l'immagine che c'è resta
    /// finché non arriva quella nuova, che si chiede anche se la scala è giusta.
    da_rifare: HashSet<usize>,
    misure: Vec<Misura>,
    /// L'impaginazione dell'ultimo frame, rifatta quando cambiano zoom o larghezza.
    imp: Option<Impaginazione>,
    /// Lo zoom che si vede.
    zoom: f32,
    /// Lo zoom delle tessere nitide, e la sua generazione.
    zoom_nitido: f32,
    generazione: u64,
    /// Quando è cambiato lo zoom l'ultima volta (`InputState::time`).
    ultimo_zoom: f64,
    /// Due dita sullo schermo: il pizzico è in corso, e lo scorrimento di egui è fermo.
    dita: bool,
    centro_dita: Option<Pos2>,
    scorrimento: Vec2,
    /// Lo scorrimento da dare all'area al frame dopo (pizzico, doppio tocco, salto).
    forza: Option<Vec2>,
    intere: HashMap<usize, Immagine>,
    tessere: HashMap<(usize, u32, u32), Immagine>,
    /// Le tessere di uno zoom passato, stirate finché arrivano le nuove.
    vecchie: Vec<(usize, Immagine)>,
    /// L'ultima coda data al pool.
    coda: Vec<RenderRequest>,
    /// Le pagine che il motore non ha disegnato: non si richiedono più.
    fallite: HashSet<usize>,
    /// Le pagine con tessere che mancano, nell'ultimo frame.
    incomplete: HashSet<usize>,
    /// Quando si è mossa la vista l'ultima volta, per l'indicatore; all'apertura si mostra.
    ultimo_movimento: Option<f64>,
    nato: Instant,
    prima_vista: bool,
    conti: Conti,
    /// La misura della vista nell'ultimo frame.
    vista: Vec2,
    /// Lo spazio in più dopo l'ultima pagina, per la barra flottante.
    sotto: f32,
    salto: Option<Salto>,
    miniature: HashMap<usize, Immagine>,
    /// Le miniature chieste dalla griglia nell'ultimo frame, la pagina in mezzo alla griglia e le
    /// pagine delle celle che la griglia vuole (a vista, più una fila sopra e sotto), che non si
    /// liberano anche oltre il tetto.
    miniature_volute: Vec<RenderRequest>,
    centro_miniature: Option<usize>,
    miniature_attorno: std::ops::Range<usize>,
    scorri_miniature: bool,
    /// I testi delle pagine letti dal pool, e le pagine il cui testo non si legge.
    testi: HashMap<usize, Arc<PageText>>,
    /// I testi nella forma del confronto, fatti una volta per pagina alla prima ricerca che li vuole.
    normali: HashMap<usize, TestoNormale>,
    testi_falliti: HashSet<usize>,
    ricerca: Option<Ricerca>,
    ricerca_dal: Instant,
    ricerca_detta: bool,
    collegamenti: Vec<Collegamento>,
    /// Il testo selezionato.
    selezione: Option<Selezione>,
    /// Una pressione lunga su una pagina il cui testo non è ancora arrivato: la pagina, il punto
    /// e la tolleranza in pt.
    selezione_attesa: Option<(usize, Pos2, f32)>,
    maniglie: Maniglie,
    /// Dove va la lente, sullo schermo, mentre si trascina una maniglia.
    punto_lente: Option<Pos2>,
    /// Dove stava sullo schermo l'angolo del contenuto nell'ultimo frame, e la zona del lettore.
    origine: Pos2,
    zona: Rect,
    /// Le azioni della barra della selezione al posto di Copia e Tutto.
    azioni_selezione: Option<AzioniSelezione>,
    /// Un dito solo scorre le pagine (no con la penna: disegna); il pizzico sempre.
    dito_scorre: bool,
    /// Un tocco su un link lo segue (no nel modo Modifica: il link si sceglie).
    segue_link: bool,
}

/// Chi scorre le pagine: niente durante il pizzico (lo fa lui), il dito se scorre (non con la
/// penna), sempre la barra e la rotella.
fn sorgente_dello_scorrimento(pizzico: bool, dito: bool) -> egui::scroll_area::ScrollSource {
    use egui::scroll_area::ScrollSource;
    match (pizzico, dito) {
        (true, _) => ScrollSource::NONE,
        (false, true) => ScrollSource::ALL,
        (false, false) => ScrollSource::SCROLL_BAR | ScrollSource::MOUSE_WHEEL,
    }
}

impl Lettore {
    /// Il lettore del PDF dell'istantanea, col pool nostro di due thread sui byte da mostrare.
    pub fn new(ist: &Istantanea, id: Id) -> Self {
        let pool = RenderPool::new(ist.display.clone(), THREAD, ist.configurazione.clone()).without_replacements();
        Self {
            id,
            pool,
            display: ist.display.clone(),
            da_rifare: HashSet::new(),
            misure: ist.misure.clone(),
            imp: None,
            zoom: 1.0,
            zoom_nitido: 1.0,
            generazione: 0,
            ultimo_zoom: f64::NEG_INFINITY,
            dita: false,
            centro_dita: None,
            scorrimento: Vec2::ZERO,
            forza: None,
            intere: HashMap::new(),
            tessere: HashMap::new(),
            vecchie: Vec::new(),
            coda: Vec::new(),
            fallite: HashSet::new(),
            incomplete: HashSet::new(),
            ultimo_movimento: None,
            nato: Instant::now(),
            prima_vista: false,
            conti: Conti::default(),
            vista: Vec2::ZERO,
            sotto: 0.0,
            salto: None,
            miniature: HashMap::new(),
            miniature_volute: Vec::new(),
            centro_miniature: None,
            miniature_attorno: 0..0,
            scorri_miniature: false,
            testi: HashMap::new(),
            normali: HashMap::new(),
            testi_falliti: HashSet::new(),
            ricerca: None,
            ricerca_dal: Instant::now(),
            ricerca_detta: false,
            collegamenti: ist.collegamenti.clone(),
            selezione: None,
            selezione_attesa: None,
            maniglie: Maniglie::default(),
            punto_lente: None,
            origine: Pos2::ZERO,
            zona: Rect::NOTHING,
            azioni_selezione: None,
            dito_scorre: true,
            segue_link: true,
        }
    }

    /// Le azioni della barra della selezione (None: Copia e Tutto), da dare prima di
    /// [`mostra`](Self::mostra).
    pub fn azioni_della_selezione(&mut self, azioni: Option<AzioniSelezione>) {
        self.azioni_selezione = azioni;
    }

    /// Se un dito solo scorre le pagine: no mentre la penna disegna. Il pizzico resta.
    pub fn dito_scorre(&mut self, si: bool) {
        self.dito_scorre = si;
    }

    /// Se un tocco su un link lo segue: no nel modo Modifica, dove il tocco arriva come gli altri.
    pub fn segue_link(&mut self, si: bool) {
        self.segue_link = si;
    }

    /// Due dita sullo schermo: il pizzico è in corso.
    pub fn pizzica(&self) -> bool {
        self.dita
    }

    /// La pagina `pagina` è cambiata e la sua immagine nuova non è ancora arrivata: quella che si
    /// vede è di prima.
    pub fn da_rifare(&self, pagina: usize) -> bool {
        self.da_rifare.contains(&pagina)
    }

    /// Lo zoom da cui partire (la ripresa), tenuto fra 0,5 e 6.
    pub fn imposta_zoom(&mut self, zoom: f32) {
        if zoom.is_finite() {
            self.zoom = zoom.clamp(ZOOM_MINIMO, ZOOM_MASSIMO);
        }
    }

    /// Il punto che si legge, da ricordare: la pagina (da 0) e il punto in pt dall'alto che sta
    /// poco sotto la cima della vista, dove lo rimette [`vai_al_punto`](Self::vai_al_punto). None
    /// prima del primo frame.
    pub fn posizione(&self) -> Option<(usize, f32)> {
        self.punto_letto().map(|(p, _, y)| (p, y))
    }

    /// Il punto in orizzontale che sta al centro della vista, in frazione della larghezza della
    /// pagina (da 0 a 1), dove lo rimette [`vai_al_punto_xy`](Self::vai_al_punto_xy).
    pub fn posizione_x(&self) -> Option<f32> {
        self.punto_letto().map(|(_, x, _)| x)
    }

    /// La pagina, la x in frazione della larghezza e la y in pt dall'alto del punto che si legge.
    fn punto_letto(&self) -> Option<(usize, f32, f32)> {
        let imp = self.imp.as_ref()?;
        let (p, pt) = imp.punto(pos2(self.scorrimento.x + self.vista.x / 2.0, self.scorrimento.y + SOPRA_IL_PUNTO))?;
        let larghezza = imp.misura(p).larghezza;
        let x = if larghezza > 0.0 { (pt.x / larghezza).clamp(0.0, 1.0) } else { 0.5 };
        Some((p, x, pt.y))
    }

    /// La pagina, la x in frazione della larghezza e la y in pt dall'alto del punto al centro della
    /// vista.
    fn punto_al_centro(&self) -> Option<(usize, f32, f32)> {
        let imp = self.imp.as_ref()?;
        let (p, pt) = imp.punto(self.scorrimento.to_pos2() + self.vista / 2.0)?;
        let larghezza = imp.misura(p).larghezza;
        let x = if larghezza > 0.0 { (pt.x / larghezza).clamp(0.0, 1.0) } else { 0.5 };
        Some((p, x, pt.y))
    }

    /// C'è una selezione (o una pressione lunga che aspetta il testo della pagina).
    pub fn ha_selezione(&self) -> bool {
        self.selezione.is_some() || self.selezione_attesa.is_some()
    }

    /// Via la selezione, con le maniglie e la barra.
    pub fn chiudi_selezione(&mut self) {
        self.selezione = None;
        self.selezione_attesa = None;
    }

    /// La selezione di adesso.
    pub fn selezione(&self) -> Option<Selezione> {
        self.selezione
    }

    /// Il testo selezionato, come lo copierebbe «Copia».
    pub fn testo_selezionato(&self) -> Option<String> {
        let s = self.selezione?;
        Some(s.testo(self.testi.get(&s.pagina)?))
    }

    /// La pagina che si sta leggendo (da 0), con la vista dell'ultimo frame.
    pub fn pagina_corrente(&self) -> usize {
        self.corrente(self.vista)
    }

    /// Lo spazio in più dopo l'ultima pagina (dp), che così sale sopra una barra flottante.
    pub fn spazio_sotto(&mut self, dp: f32) {
        self.sotto = dp;
    }

    /// Porta la vista alla cima della pagina `pagina` (da 0) al frame dopo.
    pub fn vai_alla_pagina(&mut self, pagina: usize) {
        self.vai_al_punto(pagina, None);
    }

    /// Porta la vista al punto `y` (pt dall'alto) della pagina `pagina`, o alla sua cima.
    pub fn vai_al_punto(&mut self, pagina: usize, y: Option<f32>) {
        if pagina < self.misure.len() {
            self.salto = Some(Salto::Pagina(pagina, y));
        }
    }

    /// Porta la vista a vedere tutto il rettangolo `r` (pt) della pagina `pagina`, se non si vede
    /// già, sopra lo spazio in fondo (la barra sulla tastiera): il campo che si compila.
    pub fn mostra_rettangolo(&mut self, pagina: usize, r: [f32; 4]) {
        if pagina < self.misure.len() && r.iter().all(|v| v.is_finite()) {
            self.salto = Some(Salto::Rettangolo(pagina, r));
        }
    }

    /// Dove sta sullo schermo il rettangolo `r` (pt) della pagina `pagina`, con la vista
    /// dell'ultimo frame; None prima del primo frame.
    pub fn sullo_schermo(&self, pagina: usize, r: [f32; 4]) -> Option<Rect> {
        let imp = self.imp.as_ref()?;
        if pagina >= imp.pagine() {
            return None;
        }
        let a = imp.posizione(pagina, pos2(r[0], r[1]));
        let b = imp.posizione(pagina, pos2(r[2], r[3]));
        Some(Rect::from_two_pos(a, b).translate(self.origine.to_vec2()))
    }

    /// La pagina e il punto in pt sotto il punto `p` dello schermo, se è su una pagina.
    pub fn sulla_pagina(&self, p: Pos2) -> Option<(usize, Pos2)> {
        let imp = self.imp.as_ref()?;
        let nel_contenuto = p - self.origine.to_vec2();
        let (i, pt) = imp.punto(nel_contenuto)?;
        imp.rettangolo(i).contains(nel_contenuto).then_some((i, pt))
    }

    /// dp dello schermo per pt della pagina `pagina`, con lo zoom dell'ultimo frame: ogni pagina ha
    /// la sua scala, perché a zoom 1 ognuna è larga quanto lo schermo.
    pub fn dp_per_pt(&self, pagina: usize) -> f32 {
        self.imp.as_ref().map_or(1.0, |imp| imp.dp_per_pt(pagina))
    }

    /// La zona dello schermo del lettore nell'ultimo frame.
    pub fn zona(&self) -> Rect {
        self.zona
    }

    /// Porta la vista al punto della pagina `pagina` con la x `x` in frazione della larghezza (al
    /// centro della vista, se la pagina è più larga) e la y `y` in pt dall'alto (poco sotto la
    /// cima): la ripresa. Una x impossibile vale il mezzo.
    pub fn vai_al_punto_xy(&mut self, pagina: usize, x: f32, y: f32) {
        let x = if x.is_finite() { x.clamp(0.0, 1.0) } else { 0.5 };
        if pagina < self.misure.len() && y.is_finite() {
            self.salto = Some(Salto::Punto(pagina, x, y));
        }
    }

    /// Il documento è cambiato (una modifica, annulla, ripeti, un salvataggio): l'istantanea nuova
    /// e le pagine toccate. Byte nuovi vogliono un pool nuovo (quello di prima e le sue richieste
    /// se ne vanno). Una pagina che ha cambiato misura (girata) perde la sua immagine; le altre
    /// toccate tengono quella di prima finché non arriva la nuova (niente pagina bianca per un
    /// attimo), le tessere diventano vecchie; testi, miniature e selezione delle pagine toccate se
    /// ne vanno, e la ricerca le rilegge. Le pagine non toccate restano come sono.
    pub fn aggiorna(&mut self, ist: &Istantanea, toccate: &Toccate) {
        if !Arc::ptr_eq(&self.display, &ist.display) {
            self.pool = RenderPool::new(ist.display.clone(), THREAD, ist.configurazione.clone()).without_replacements();
            self.display = ist.display.clone();
            self.coda.clear();
        }
        self.collegamenti = ist.collegamenti.clone();
        let tutte = matches!(toccate, Toccate::Tutte) || ist.misure.len() != self.misure.len();
        let mut pagine: HashSet<usize> = match toccate {
            Toccate::Alcune(v) => v.iter().copied().collect(),
            _ => HashSet::new(),
        };
        // Una pagina che ha cambiato misura è toccata anche se nessuno lo dice.
        let girate: HashSet<usize> = (0..ist.misure.len().min(self.misure.len())).filter(|p| ist.misure[*p] != self.misure[*p]).collect();
        pagine.extend(&girate);
        if ist.misure != self.misure {
            self.misure = ist.misure.clone();
            self.imp = None;
        }
        if tutte {
            self.intere.clear();
            self.tessere.clear();
            self.vecchie.clear();
            self.miniature.clear();
            self.testi.clear();
            self.normali.clear();
            self.fallite.clear();
            self.testi_falliti.clear();
            self.da_rifare.clear();
            self.chiudi_selezione();
        } else {
            for &p in &pagine {
                if girate.contains(&p) {
                    self.intere.remove(&p);
                    self.vecchie.retain(|(q, _)| *q != p);
                } else if self.intere.contains_key(&p) {
                    self.da_rifare.insert(p);
                }
                let tessere: Vec<(usize, u32, u32)> = self.tessere.keys().copied().filter(|(q, _, _)| *q == p).collect();
                for k in tessere {
                    if let Some(im) = self.tessere.remove(&k)
                        && !girate.contains(&p)
                    {
                        self.vecchie.push((p, im));
                    }
                }
                self.miniature.remove(&p);
                self.testi.remove(&p);
                self.normali.remove(&p);
                self.fallite.remove(&p);
                self.testi_falliti.remove(&p);
            }
            if self.selezione.is_some_and(|s| pagine.contains(&s.pagina)) {
                self.selezione = None;
            }
        }
        if let Some(r) = self.ricerca.as_mut() {
            let rileggere: Option<Vec<usize>> = (!tutte).then(|| pagine.iter().copied().collect());
            r.da_rileggere(rileggere.as_deref(), self.misure.len());
            self.ricerca_detta = false;
            self.ricerca_dal = Instant::now();
        }
    }

    /// Cerca `testo` dalla pagina che si legge, senza maiuscole né accenti; un testo vuoto chiude
    /// la ricerca. La stessa parola (anche con altre maiuscole o accenti) non ricomincia. I testi
    /// già letti valgono subito, gli altri si chiedono al pool.
    pub fn cerca(&mut self, testo: &str) {
        if self.ricerca.as_ref().is_some_and(|r| r.cerca_gia(testo)) {
            return;
        }
        self.ricerca = Ricerca::new(testo, self.misure.len(), self.pagina_corrente());
        self.ricerca_dal = Instant::now();
        self.ricerca_detta = false;
        let Some(mut r) = self.ricerca.take() else { return };
        for p in r.mancanti() {
            let salto = match self.testi.get(&p).cloned() {
                Some(t) => {
                    let normale = self.normale(p, &t);
                    r.leggi_normale(p, &t, normale)
                }
                None if self.testi_falliti.contains(&p) => r.illeggibile(p),
                None => None,
            };
            if salto.is_some() {
                self.salto = Some(Salto::Risultato);
            }
        }
        self.ricerca = Some(r);
    }

    /// Il testo della pagina `p` nella forma del confronto: fatto la prima volta, poi tenuto.
    fn normale(&mut self, p: usize, testo: &PageText) -> &TestoNormale {
        self.normali.entry(p).or_insert_with(|| {
            self.conti.normalizzati += 1;
            TestoNormale::new(testo)
        })
    }

    /// Via la ricerca e i suoi risultati sulla pagina.
    pub fn chiudi_ricerca(&mut self) {
        self.ricerca = None;
    }

    /// Il conto della ricerca, se ce n'è una.
    pub fn conteggio(&self) -> Option<Conteggio> {
        self.ricerca.as_ref().map(|r| Conteggio { attuale: r.numero_attuale(), totale: r.totale(), finita: r.finita() })
    }

    /// Il risultato dopo (o prima), e la vista fin lì.
    pub fn risultato_altro(&mut self, avanti: bool) {
        if let Some(r) = self.ricerca.as_mut()
            && r.altro(avanti).is_some()
        {
            self.salto = Some(Salto::Risultato);
        }
    }

    /// La griglia delle miniature si apre: al primo frame scorre fino alla pagina che si legge.
    pub fn apri_miniature(&mut self) {
        self.scorri_miniature = true;
    }

    /// Le miniature pronte, in ordine.
    pub fn miniature_pronte(&self) -> Vec<usize> {
        let mut v: Vec<usize> = self.miniature.keys().copied().collect();
        v.sort_unstable();
        v
    }

    pub fn pagine(&self) -> usize {
        self.misure.len()
    }

    /// Lo zoom che si vede: 1 è la pagina più larga larga quanto lo schermo meno i margini.
    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    /// Lo zoom delle immagini nitide.
    pub fn zoom_nitido(&self) -> f32 {
        self.zoom_nitido
    }

    pub fn conti(&self) -> Conti {
        self.conti
    }

    /// La memoria delle texture di adesso, in byte: pagine intere e tessere.
    pub fn byte_texture(&self) -> (usize, usize) {
        let intere = self.intere.values().map(|i| i.byte).sum();
        let tessere = self.tessere.values().chain(self.vecchie.iter().map(|(_, i)| i)).map(|i| i.byte).sum();
        (intere, tessere)
    }

    /// Le pagine con l'immagine intera pronta, in ordine.
    pub fn pagine_pronte(&self) -> Vec<usize> {
        let mut v: Vec<usize> = self.intere.keys().copied().collect();
        v.sort_unstable();
        v
    }

    /// Quante tessere ci sono: (dello zoom di adesso, vecchie).
    pub fn quante_tessere(&self) -> (usize, usize) {
        (self.tessere.len(), self.vecchie.len())
    }

    /// Ci sono immagini chieste al pool e non ancora arrivate.
    pub fn in_attesa(&self) -> bool {
        !self.coda.is_empty()
    }

    /// Lo scorrimento di adesso, in dp.
    pub fn scorrimento(&self) -> Vec2 {
        self.scorrimento
    }

    /// Porta la vista a `scorrimento` (dp) al frame dopo.
    pub fn scorri_a(&mut self, scorrimento: Vec2) {
        self.forza = Some(scorrimento);
    }

    /// La pagina che si sta leggendo (da 0), con una vista di misura `vista`.
    pub fn corrente(&self, vista: Vec2) -> usize {
        self.imp.as_ref().map_or(0, |imp| imp.corrente(Rect::from_min_size(self.scorrimento.to_pos2(), vista)))
    }

    /// Una riga di misure per la diagnosi: quanti disegni, quanto tempo, quanta memoria.
    pub fn riga_misure(&self) -> String {
        let c = self.conti;
        let n = c.pagine + c.tessere + c.miniature;
        let media = if n > 0 { c.ms_totali as f64 / f64::from(n) } else { 0.0 };
        format!(
            "{} pagine intere, {} tessere e {} miniature disegnate, in media {media:.0} ms, al massimo {} ms; texture fino a {:.1} MB; {} testi di pagina letti",
            c.pagine,
            c.tessere,
            c.miniature,
            c.ms_massimo,
            c.byte_massimi as f64 / (1024.0 * 1024.0),
            c.testi
        )
    }

    fn impaginazione(&mut self, larghezza: f32) -> Impaginazione {
        match &self.imp {
            Some(imp) if imp.zoom() == self.zoom && imp.larghezza_vista() == larghezza && imp.spazio_sotto() == self.sotto => imp.clone(),
            _ => {
                let imp = Impaginazione::new(self.misure.clone(), larghezza, self.zoom).con_spazio_sotto(self.sotto);
                self.imp = Some(imp.clone());
                imp
            }
        }
    }

    /// Disegna il lettore in tutto lo spazio di `ui`.
    pub fn mostra(&mut self, ui: &mut Ui) -> Esito {
        let mut esito = Esito::default();
        let rect = ui.max_rect();
        let ppp = ui.ctx().pixels_per_point();
        let adesso = ui.input(|i| i.time);
        if self.ultimo_movimento.is_none() {
            self.ultimo_movimento = Some(adesso);
        }
        ui.painter().rect_filled(rect, 0.0, tema::TAVOLO);
        self.zona = rect;
        let vista = rect.size();
        // Il telefono girato: l'impaginazione si rifà con l'altra larghezza, ma lo scorrimento è in
        // dp e porterebbe a un'altra pagina. Si rimette al centro il punto che stava al centro, e
        // chi era in fondo resta in fondo: col punto in cima alla vista, con le slide si finiva su
        // un'altra pagina (analisi del 27/10, lettura I2).
        if let Some(imp) = self.imp.as_ref()
            && imp.larghezza_vista() != vista.x
            && self.salto.is_none()
        {
            let massimo = imp.tieni(vec2(0.0, f32::MAX), self.vista).y;
            if massimo > 0.0 && self.scorrimento.y >= massimo - 0.5 {
                self.salto = Some(Salto::Fondo);
            } else if let Some((p, x, y)) = self.punto_al_centro() {
                self.salto = Some(Salto::Centro(p, x, y));
            }
        }
        self.vista = vista;
        let miniature = std::mem::take(&mut self.miniature_volute);
        let imp = self.impaginazione(vista.x);
        self.pizzico(ui, rect, &imp, adesso);
        let imp = self.impaginazione(vista.x);
        if let Some(s) = self.salto.take()
            && let Some(f) = self.scorrimento_per(s, &imp, vista)
        {
            self.forza = Some(f);
            self.ultimo_movimento = Some(adesso);
        }
        // Le immagini nitide dello zoom nuovo, a dita alzate e dopo un attimo di calma.
        let calmo = !self.dita && adesso - self.ultimo_zoom >= CALMA_ZOOM;
        if !calmo {
            let resta = (CALMA_ZOOM - (adesso - self.ultimo_zoom)).max(0.0);
            ui.ctx().request_repaint_after(Duration::from_secs_f64(resta).max(ATTESA));
        }
        if calmo && self.zoom != self.zoom_nitido {
            self.zoom_nitido = self.zoom;
            self.generazione += 1;
            self.vecchie.extend(self.tessere.drain().map(|((p, _, _), im)| (p, im)));
            esito.zoom = Some(self.zoom);
        }
        // Le pagine nitide fino al bordo, come nel bozzetto: niente velo sui lati che scorrono.
        ui.spacing_mut().scroll.fade.strength = 0.0;
        let sorgente = sorgente_dello_scorrimento(self.dita, self.dito_scorre);
        let mut area = egui::ScrollArea::both().id_salt(self.id).scroll_source(sorgente).auto_shrink([false, false]);
        if let Some(f) = self.forza.take() {
            area = area.scroll_offset(imp.tieni(f, vista));
        }
        let mut doppio = None;
        let mut tocco = None;
        let mut lunga = None;
        // La barra della selezione resta sopra lo spazio in fondo (la barra flottante).
        let zona_barra = Rect::from_min_max(rect.min, pos2(rect.max.x, (rect.max.y - self.sotto).max(rect.min.y)));
        area.show_viewport(ui, |ui, viewport| {
            let (contenuto, risposta) = ui.allocate_exact_size(imp.contenuto().max(viewport.size()), Sense::click());
            let scorrimento = viewport.min.to_vec2();
            self.origine = contenuto.min;
            if scorrimento != self.scorrimento {
                self.ultimo_movimento = Some(adesso);
            }
            self.scorrimento = scorrimento;
            self.ricevi(ui.ctx(), &mut esito);
            if calmo {
                self.chiedi(&imp, viewport, ppp, &miniature);
            }
            let immagini = self.dipingi(ui, &imp, contenuto.min, viewport, ppp);
            self.libera(&imp, viewport, ppp, calmo);
            if risposta.is_pointer_button_down_on() {
                ui.ctx().request_repaint_after(ATTESA_PRESSIONE);
            }
            // La pressione lunga (col tasto destro sul PC) prima del tocco: egui le dà tutte e due.
            // In un pizzico niente: il primo dito resta spesso fermo, ma non è una pressione lunga.
            let dito = (!self.dita).then(|| risposta.interact_pointer_pos()).flatten();
            if let Some(p) = dito {
                if risposta.long_touched() || risposta.secondary_clicked() {
                    lunga = Some(p - contenuto.min);
                } else if risposta.double_clicked() {
                    doppio = Some(p);
                } else if risposta.clicked() {
                    tocco = Some(p - contenuto.min);
                }
            }
            self.mostra_selezione(ui, &imp, contenuto.min, zona_barra, &immagini, &mut esito);
        });
        if let Some(p) = lunga {
            self.pressione_lunga(&imp, p.to_pos2(), &mut esito);
            ui.ctx().request_repaint();
        }
        // Un tocco con una selezione la chiude e basta, come su Android.
        let chiude = tocco.is_some() && self.ha_selezione();
        if chiude {
            self.chiudi_selezione();
        }
        if let Some(t) = tocco.filter(|_| !chiude) {
            match self.link_a(&imp, t.to_pos2()).filter(|_| self.segue_link) {
                Some(d) => {
                    if let Destinazione::Pagina { pagina, y } = d {
                        self.vai_al_punto(pagina, y);
                        ui.ctx().request_repaint();
                    }
                    esito.link = Some(d);
                }
                None => {
                    esito.tocco = imp.punto(t.to_pos2()).filter(|(i, _)| imp.rettangolo(*i).contains(t.to_pos2()));
                }
            }
        }
        if let Some(r) = &self.ricerca
            && r.finita()
            && !self.ricerca_detta
        {
            self.ricerca_detta = true;
            let ms = self.ricerca_dal.elapsed().as_millis() as u64;
            esito.ricerca_finita = Some(RicercaFinita { risultati: r.totale(), pagine: r.lette(), ms });
        }
        if let Some(p) = doppio {
            let a = p - rect.min;
            let (nuova, s) = imp.zoom_attorno(imp.zoom_doppio_tocco(), a, a, self.scorrimento, vista);
            self.zoom = nuova.zoom();
            self.forza = Some(s);
            // Niente pizzico da aspettare: le immagini nitide subito.
            self.ultimo_zoom = adesso - CALMA_ZOOM;
            self.ultimo_movimento = Some(adesso);
            ui.ctx().request_repaint();
        }
        self.fine_pizzico(ui);
        self.indicatore(ui, rect, adesso);
        if !self.coda.is_empty() {
            ui.ctx().request_repaint_after(ATTESA);
        }
        let (a, b) = self.byte_texture();
        let m: usize = self.miniature.values().map(|i| i.byte).sum();
        self.conti.byte_massimi = self.conti.byte_massimi.max(a + b + m);
        // La griglia, se è aperta, li rimette dopo il lettore.
        self.centro_miniature = None;
        self.miniature_attorno = 0..0;
        esito
    }

    /// Lo scorrimento per un salto: la cima della pagina (o il punto un poco sotto la cima della
    /// vista); il risultato attuale a un terzo della vista, se non si vede già bene.
    fn scorrimento_per(&self, salto: Salto, imp: &Impaginazione, vista: Vec2) -> Option<Vec2> {
        match salto {
            Salto::Pagina(p, y) => {
                let cima = match y {
                    Some(y) => imp.posizione(p, pos2(0.0, y)).y - SOPRA_IL_PUNTO,
                    None => imp.rettangolo(p).top() - CIMA,
                };
                Some(imp.tieni(vec2(self.scorrimento.x, cima), vista))
            }
            Salto::Punto(p, x, y) => {
                let punto = imp.posizione(p, pos2(x * imp.misura(p).larghezza, y));
                Some(imp.tieni(vec2(punto.x - vista.x / 2.0, punto.y - SOPRA_IL_PUNTO), vista))
            }
            Salto::Centro(p, x, y) => {
                let punto = imp.posizione(p, pos2(x * imp.misura(p).larghezza, y));
                Some(imp.tieni(punto.to_vec2() - vista / 2.0, vista))
            }
            Salto::Fondo => Some(imp.tieni(vec2(self.scorrimento.x, f32::MAX), vista)),
            Salto::Rettangolo(p, [x0, y0, x1, y1]) => {
                let dove = Rect::from_min_max(imp.posizione(p, pos2(x0, y0)), imp.posizione(p, pos2(x1, y1)));
                let ora = Rect::from_min_max(
                    self.scorrimento.to_pos2() + vec2(8.0, 8.0),
                    (self.scorrimento + vista).to_pos2() - vec2(8.0, self.sotto.max(8.0)),
                );
                if ora.contains_rect(dove) {
                    return None;
                }
                let alto = (ora.height() - 8.0).max(0.0);
                // In verticale il campo a un terzo della vista (sotto, se è più alto di quanto si
                // vede); in orizzontale si sposta solo se esce.
                let y = if dove.height() > alto { dove.top() - 8.0 } else { dove.top() - (alto - dove.height()) / 3.0 - 8.0 };
                let x = if dove.left() < ora.left() || dove.right() > ora.right() {
                    if dove.width() > ora.width() { dove.left() - 8.0 } else { dove.center().x - vista.x / 2.0 }
                } else {
                    self.scorrimento.x
                };
                Some(imp.tieni(vec2(x, y), vista))
            }
            Salto::Risultato => {
                let r = self.ricerca.as_ref()?;
                let (p, i) = r.attuale()?;
                let [x0, y0, x1, y1] = r.rettangolo((p, i))?;
                let dove = Rect::from_min_max(imp.posizione(p, pos2(x0, y0)), imp.posizione(p, pos2(x1, y1)));
                let ora = Rect::from_min_max(
                    self.scorrimento.to_pos2() + vec2(8.0, 8.0),
                    (self.scorrimento + vista).to_pos2() - vec2(8.0, self.sotto.max(8.0)),
                );
                if ora.contains_rect(dove) {
                    return None;
                }
                let x = if imp.contenuto().x > vista.x { dove.center().x - vista.x / 2.0 } else { 0.0 };
                Some(imp.tieni(vec2(x, dove.top() - vista.y / 3.0), vista))
            }
        }
    }

    /// Il link sotto il punto `p` del contenuto (dp), anche un poco fuori dal suo rettangolo: il
    /// più vicino, se ce n'è più di uno.
    fn link_a(&self, imp: &Impaginazione, p: Pos2) -> Option<Destinazione> {
        let (i, pt) = imp.punto(p)?;
        if !imp.rettangolo(i).expand(BORDO_LINK).contains(p) {
            return None;
        }
        let bordo = BORDO_LINK / imp.dp_per_pt(i).max(1e-3);
        let distanza = |c: &&Collegamento| {
            let [x0, y0, x1, y1] = c.rettangolo;
            let dx = (x0 - pt.x).max(pt.x - x1).max(0.0);
            let dy = (y0 - pt.y).max(pt.y - y1).max(0.0);
            dx.max(dy)
        };
        self.collegamenti
            .iter()
            .filter(|c| c.pagina == i && distanza(c) <= bordo)
            .min_by(|a, b| distanza(a).total_cmp(&distanza(b)))
            .map(|c| c.destinazione.clone())
    }

    /// Il testo di una pagina arrivato dal pool: va alla ricerca e alla pressione lunga che lo
    /// aspettava, e resta per le prossime.
    fn testo_arrivato(&mut self, ctx: &egui::Context, pagina: usize, testo: Option<Arc<PageText>>, esito: &mut Esito) {
        self.conti.testi += 1;
        let attesa = self.selezione_attesa.filter(|(p, _, _)| *p == pagina);
        if attesa.is_some() {
            self.selezione_attesa = None;
        }
        let salto = match testo {
            Some(t) => {
                let s = match self.ricerca.take() {
                    Some(mut r) => {
                        let normale = self.normale(pagina, &t);
                        let s = r.leggi_normale(pagina, &t, normale);
                        self.ricerca = Some(r);
                        s
                    }
                    None => None,
                };
                if let Some((_, pt, tolleranza)) = attesa {
                    self.seleziona_parola(pagina, &t, pt, tolleranza, esito);
                }
                self.testi.insert(pagina, t);
                s
            }
            None => {
                self.testi_falliti.insert(pagina);
                esito.senza_testo |= attesa.is_some();
                self.ricerca.as_mut().and_then(|r| r.illeggibile(pagina))
            }
        };
        if salto.is_some() {
            self.salto = Some(Salto::Risultato);
        }
        ctx.request_repaint();
    }

    /// Una pressione lunga nel punto `p` del contenuto (dp): la parola sotto il dito, se il testo
    /// della pagina c'è; se no lo si chiede al pool e la parola arriva con lui.
    fn pressione_lunga(&mut self, imp: &Impaginazione, p: Pos2, esito: &mut Esito) {
        self.chiudi_selezione();
        let Some((pagina, pt)) = imp.punto(p) else { return };
        // Fuori dalle pagine (sul tavolo fra due), niente.
        if !imp.rettangolo(pagina).contains(p) {
            return;
        }
        let tolleranza = BORDO_PAROLA / imp.dp_per_pt(pagina).max(1e-3);
        match self.testi.get(&pagina).cloned() {
            Some(t) => self.seleziona_parola(pagina, &t, pt, tolleranza, esito),
            None if self.testi_falliti.contains(&pagina) => esito.senza_testo = true,
            None => self.selezione_attesa = Some((pagina, pt, tolleranza)),
        }
    }

    /// La parola della pagina `pagina` al punto `pt` (in pt) diventa la selezione.
    fn seleziona_parola(&mut self, pagina: usize, testo: &PageText, pt: Pos2, tolleranza: f32, esito: &mut Esito) {
        if testo.is_empty() {
            esito.senza_testo = true;
            return;
        }
        if let Some(s) = Selezione::parola(pagina, testo, pt.x, pt.y, tolleranza) {
            esito.selezionati = Some(s.testo(testo).chars().count());
            self.selezione = Some(s);
        }
    }

    /// La selezione sulla pagina: le righe in azzurro sopra il testo, le due gocce da trascinare
    /// (con la lente mentre il dito le porta, e la pagina che scorre se il dito è al bordo) e, a
    /// dito fermo, la barra del contesto con Copia e Tutto sopra la selezione, dentro `zona`.
    fn mostra_selezione(&mut self, ui: &mut Ui, imp: &Impaginazione, origine: Pos2, zona: Rect, immagini: &[(TextureId, Rect)], esito: &mut Esito) {
        let Some(sel) = self.selezione else {
            self.maniglie.lascia();
            self.punto_lente = None;
            return;
        };
        let Some(testo) = self.testi.get(&sel.pagina).cloned() else {
            self.selezione = None;
            return;
        };
        let a_schermo = |x: f32, y: f32| origine + imp.posizione(sel.pagina, pos2(x, y)).to_vec2();
        let evidenza: Vec<(Rect, Color32)> = sel
            .rettangoli(&testo)
            .iter()
            .map(|&[x0, y0, x1, y1]| (Rect::from_min_max(a_schermo(x0, y0), a_schermo(x1, y1)), tema::SELEZIONE))
            .collect();
        for (r, colore) in &evidenza {
            ui.painter().rect_filled(*r, 2.0, *colore);
        }
        let Some((a, b)) = sel.bordi(&testo) else { return };
        let bordo =
            |b: BordoPt| tascabile_touch::Bordo { x: a_schermo(b.x, 0.0).x, cima: a_schermo(0.0, b.cima).y, fondo: a_schermo(0.0, b.fondo).y };
        match self.maniglie.mostra(ui, self.id.with("maniglie"), bordo(a), bordo(b), tema::BLU) {
            Some(Trascinamento::Sposta { maniglia, punto, .. }) => {
                self.punto_lente = Some(punto);
                let pt = (punto - a_schermo(0.0, 0.0)) / imp.dp_per_pt(sel.pagina).max(1e-3);
                let quale = match maniglia {
                    Maniglia::Inizio => Bordo::Inizio,
                    Maniglia::Fine => Bordo::Fine,
                };
                self.selezione = Some(sel.sposta(&testo, quale, pt.x, pt.y));
            }
            Some(Trascinamento::Lasciata(_)) => {
                self.punto_lente = None;
                esito.selezionati = Some(sel.testo(&testo).chars().count());
            }
            None => {}
        }
        if self.maniglie.trascinata().is_some() {
            if let Some(punto) = self.punto_lente {
                let vista = ui.clip_rect();
                lente_con(ui.painter(), vista, punto, tema::FOGLIO, &[], immagini, &evidenza);
                let passo = if punto.y > vista.max.y - BORDO_SCORRIMENTO {
                    -PASSO_SCORRIMENTO
                } else if punto.y < vista.min.y + BORDO_SCORRIMENTO {
                    PASSO_SCORRIMENTO
                } else {
                    0.0
                };
                if passo != 0.0 {
                    ui.scroll_with_delta_animation(vec2(0.0, passo), egui::style::ScrollAnimation::none());
                    ui.ctx().request_repaint();
                }
            }
            return;
        }
        // La barra, solo se la selezione si vede.
        let Some(dove) = evidenza.iter().map(|(r, _)| *r).reduce(|a, b| a.union(b)) else { return };
        if !dove.intersects(zona) {
            return;
        }
        if let Some(a) = self.azioni_selezione.clone() {
            let mut barra = BarraContesto::new();
            for (icona, nome) in &a.azioni {
                barra = barra.azione(icona, nome);
            }
            match barra.colori(&a.colori, a.colore).mostra_in_primo_piano(ui.ctx(), self.id.with("barra della selezione"), dove.intersect(zona), zona)
            {
                Some(i) if i < a.azioni.len() => {
                    esito.scelta_selezione = Some(SceltaSelezione::Azione { indice: i, pagina: sel.pagina, righe: sel.rettangoli(&testo) });
                    self.selezione = None;
                }
                Some(i) => esito.scelta_selezione = Some(SceltaSelezione::Colore(i - a.azioni.len())),
                None => {}
            }
            return;
        }
        let barra = BarraContesto::new().azione(icone::CONTENT_COPY, COPIA).azione(icone::SELECT_ALL, TUTTO);
        match barra.mostra_in_primo_piano(ui.ctx(), self.id.with("barra della selezione"), dove.intersect(zona), zona) {
            Some(0) => {
                esito.copia = Some(sel.testo(&testo));
                self.selezione = None;
            }
            Some(_) => {
                if let Some(tutta) = Selezione::tutta(sel.pagina, &testo) {
                    esito.selezionati = Some(tutta.testo(&testo).chars().count());
                    esito.tutta = true;
                    self.selezione = Some(tutta);
                }
            }
            None => {}
        }
    }

    /// La griglia delle miniature, larga quanto `ui`, in un'area che scorre (il foglio dal fondo):
    /// tre colonne, ogni pagina rimpicciolita col suo numero sotto, la pagina che si legge col
    /// bordo blu. Si disegnano solo le celle a vista; le loro miniature (e quelle di una fila sopra
    /// e sotto) si chiedono al pool al frame dopo. Restituisce la pagina toccata.
    pub fn mostra_miniature(&mut self, ui: &mut Ui) -> Option<usize> {
        self.griglia(ui, None).toccata
    }

    /// La griglia di «Organizza pagine» (P6): le stesse miniature, le pagine scelte col bordo blu
    /// e il segno di spunta, un tocco che dice la pagina (chi chiama la sceglie o la toglie).
    /// La pressione lunga prende la pagina (o tutte le scelte, se è fra loro): finché il dito resta
    /// giù la pagina presa lo segue, una barra blu dice dove andrà e la griglia scorre vicino ai
    /// bordi; lasciata, l'esito dice le pagine e il posto (nell'ordine di adesso). Mentre si tiene
    /// una pagina chi chiama ferma lo scorrimento col dito dell'area ([`Organizza::tiene`]).
    pub fn mostra_organizza(&mut self, ui: &mut Ui, org: &mut Organizza) -> EsitoGriglia {
        self.griglia(ui, Some(org))
    }

    fn griglia(&mut self, ui: &mut Ui, mut org: Option<&mut Organizza>) -> EsitoGriglia {
        let mut esito = EsitoGriglia::default();
        let n = self.misure.len();
        if n == 0 {
            return esito;
        }
        let ppp = ui.ctx().pixels_per_point();
        let largo = ui.available_width();
        let colonne = COLONNE_MINIATURE;
        let interno = largo - 2.0 * ATTORNO_MINIATURE.x;
        let cella_w = ((interno - SPAZIO_MINIATURE * (colonne - 1) as f32) / colonne as f32).max(1.0);
        // Celle alte come una A4 verticale: le pagine stanno dentro, in fondo, col numero sotto.
        let cella_h = cella_w * 842.0 / 595.0;
        let riga = cella_h + NUMERO_MINIATURA + SPAZIO_MINIATURE;
        let righe = n.div_ceil(colonne);
        let (fuori, _) = ui.allocate_exact_size(vec2(largo, righe as f32 * riga + 2.0 * ATTORNO_MINIATURE.y), Sense::hover());
        let tutto = fuori.shrink2(ATTORNO_MINIATURE);
        let corrente = self.pagina_corrente();
        if std::mem::take(&mut self.scorri_miniature) {
            let r = (corrente / colonne) as f32;
            ui.scroll_to_rect(Rect::from_min_size(pos2(tutto.left(), tutto.top() + r * riga), vec2(interno, riga)), Some(Align::Center));
        }
        let cella_di = |p: usize| {
            let (r, c) = (p / colonne, p % colonne);
            Rect::from_min_size(pos2(tutto.left() + c as f32 * (cella_w + SPAZIO_MINIATURE), tutto.top() + r as f32 * riga), vec2(cella_w, cella_h))
        };
        let clip = ui.clip_rect();
        let prima = (((clip.top() - tutto.top()) / riga).floor().max(0.0) as usize).min(righe);
        let ultima = ((((clip.bottom() - tutto.top()) / riga).ceil().max(0.0)) as usize).min(righe);
        self.centro_miniature = Some(((prima + ultima) / 2 * colonne).min(n - 1));
        self.miniature_attorno = (prima.saturating_sub(1) * colonne).min(n)..((ultima + 1).min(righe) * colonne).min(n);
        let (mut volute, mut margine) = (Vec::new(), Vec::new());
        let ctx = ui.ctx().clone();
        let uv = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
        let organizza = org.is_some();
        let tiene = org.as_ref().is_some_and(|o| o.presa.is_some());
        for r in prima.saturating_sub(1)..(ultima + 1).min(righe) {
            let a_vista = (prima..ultima).contains(&r);
            for c in 0..colonne {
                let p = r * colonne + c;
                if p >= n {
                    break;
                }
                let cella = cella_di(p);
                let m = self.misure[p];
                let k = (cella_w / m.larghezza.max(1.0)).min(cella_h / m.altezza.max(1.0));
                let (w, h) = (m.larghezza.max(1.0) * k, m.altezza.max(1.0) * k);
                let sul_pixel = |v: f32| (v * ppp).round() / ppp;
                let pagina = Rect::from_min_size(pos2(sul_pixel(cella.center().x - w / 2.0), sul_pixel(cella.bottom() - h)), vec2(w, h));
                let scala = k * ppp;
                let pronta = self.miniature.get(&p).is_some_and(|im| (im.scala - scala).abs() <= scala * 0.05);
                if !pronta && !self.fallite.contains(&p) {
                    let richiesta = RenderRequest { page: p, kind: RequestKind::Pixels, tile: None, scale: scala, tag: MINIATURA, preview: true };
                    if a_vista { volute.push(richiesta) } else { margine.push(richiesta) }
                }
                if !a_vista {
                    continue;
                }
                let numero = Rect::from_min_size(pos2(cella.left(), cella.bottom()), vec2(cella_w, NUMERO_MINIATURA));
                let risposta = ui.interact(cella.union(numero), Id::new("miniatura").with(p), Sense::click());
                let etichetta = format!("Pagina {}", p + 1);
                let scelta = org.as_ref().is_some_and(|o| o.scelte.contains(&p));
                if organizza {
                    risposta.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, true, scelta, &etichetta));
                } else {
                    risposta.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, &etichetta));
                }
                let presa = org.as_ref().and_then(|o| o.presa.as_ref()).is_some_and(|pr| pr.pagine.contains(&p));
                let painter = ui.painter();
                tema::dipingi_ombra(painter, pagina, 0, &tema::OMBRA_PAGINA);
                painter.rect_filled(pagina, 0.0, tema::FOGLIO);
                if let Some(im) = self.miniature.get(&p).or_else(|| self.intere.get(&p)) {
                    painter.image(im.tex.id(), pagina, uv, Color32::WHITE);
                }
                if presa {
                    // La pagina presa lascia il suo posto velato: è sotto il dito.
                    painter.rect_filled(pagina, 0.0, tema::CARTA.gamma_multiply(0.75));
                } else if risposta.is_pointer_button_down_on() && !tiene {
                    painter.rect_filled(pagina, 0.0, tema::SELEZIONE);
                }
                let evidenziata = if organizza { scelta } else { p == corrente };
                let (colore, carattere) = if evidenziata { (tema::BLU, Carattere::Grassetto) } else { (tema::MATITA, Carattere::Testo) };
                if evidenziata {
                    painter.rect_stroke(pagina, 2.0, Stroke::new(2.5, tema::BLU), StrokeKind::Outside);
                }
                if organizza {
                    dipingi_spunta(painter, &ctx, pagina, scelta);
                }
                let g = painter.layout_no_wrap((p + 1).to_string(), carattere.font(&ctx, 14.0), colore);
                painter.galley(pos2(numero.center().x - g.size().x / 2.0, numero.top() + 6.0), g, Color32::PLACEHOLDER);
                match org.as_deref_mut() {
                    Some(o) if o.presa.is_none() && risposta.long_touched() => {
                        let pagine: Vec<usize> = if scelta { o.scelte.iter().copied().collect() } else { vec![p] };
                        esito.prese = Some(pagine.len());
                        let dito = risposta.interact_pointer_pos().unwrap_or(cella.center());
                        o.presa = Some(Presa { pagine, dito, posto: None });
                    }
                    Some(o) if o.presa.is_none() && risposta.clicked() => esito.toccata = Some(p),
                    None if risposta.clicked() => esito.toccata = Some(p),
                    _ => {}
                }
            }
        }
        if let Some(o) = org
            && let Some(presa) = o.presa.as_mut()
        {
            let (giu, dito) = ui.input(|i| (i.pointer.any_down(), i.pointer.interact_pos().or(i.pointer.latest_pos())));
            if let Some(d) = dito {
                presa.dito = d;
            }
            // Il posto davanti alla cella sotto il dito, o dopo, se il dito ne passa la metà.
            let d = presa.dito;
            let r = (((d.y - tutto.top()) / riga).floor().max(0.0) as usize).min(righe - 1);
            let c = (((d.x - tutto.left()) / (cella_w + SPAZIO_MINIATURE)).floor().max(0.0) as usize).min(colonne - 1);
            let i = r * colonne + c;
            let posto = if i >= n {
                n
            } else if d.x > cella_di(i).center().x {
                i + 1
            } else {
                i
            };
            presa.posto = Some(posto);
            if giu {
                let segno = if posto < n {
                    let cella = cella_di(posto);
                    Rect::from_center_size(pos2(cella.left() - SPAZIO_MINIATURE / 2.0, cella.center().y), vec2(4.0, cella_h))
                } else {
                    let cella = cella_di(n - 1);
                    Rect::from_center_size(pos2(cella.right() + SPAZIO_MINIATURE / 2.0, cella.center().y), vec2(4.0, cella_h))
                };
                ui.painter().rect_filled(segno, 2.0, tema::BLU);
                // La pagina presa sotto il dito, sopra tutto, col numero delle pagine se sono più.
                let sopra = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, self.id.with("pagina presa")));
                let prima_presa = presa.pagine[0];
                let m = self.misure[prima_presa];
                let k = (cella_w * 0.9 / m.larghezza.max(1.0)).min(cella_h * 0.9 / m.altezza.max(1.0));
                let foglio = Rect::from_center_size(d - vec2(0.0, cella_h * 0.35), vec2(m.larghezza.max(1.0) * k, m.altezza.max(1.0) * k));
                tema::dipingi_ombra(&sopra, foglio, 0, &tema::OMBRA_FLOTTANTE);
                sopra.rect_filled(foglio, 0.0, tema::FOGLIO);
                if let Some(im) = self.miniature.get(&prima_presa).or_else(|| self.intere.get(&prima_presa)) {
                    sopra.image(im.tex.id(), foglio, uv, Color32::WHITE);
                }
                sopra.rect_stroke(foglio, 2.0, Stroke::new(2.5, tema::BLU), StrokeKind::Outside);
                if presa.pagine.len() > 1 {
                    let tondo = Rect::from_center_size(foglio.right_top(), vec2(28.0, 28.0));
                    sopra.circle_filled(tondo.center(), 14.0, tema::BLU);
                    let g = sopra.layout_no_wrap(presa.pagine.len().to_string(), Carattere::Grassetto.font(&ctx, 14.0), Color32::WHITE);
                    sopra.galley(tondo.center() - g.size() / 2.0, g, Color32::PLACEHOLDER);
                }
                // Vicino ai bordi la griglia scorre da sola.
                let passo = if d.y > clip.bottom() - BORDO_SCORRIMENTO {
                    -PASSO_SCORRIMENTO
                } else if d.y < clip.top() + BORDO_SCORRIMENTO {
                    PASSO_SCORRIMENTO
                } else {
                    0.0
                };
                if passo != 0.0 {
                    ui.scroll_with_delta(vec2(0.0, passo));
                }
                ui.ctx().request_repaint();
            } else {
                let presa = o.presa.take().expect("presa c'è");
                if crate::pagine::sposta_qualcosa(&presa.pagine, posto) {
                    esito.spostate = Some((presa.pagine, posto));
                }
            }
        }
        volute.extend(margine);
        if !volute.is_empty() {
            ui.ctx().request_repaint_after(ATTESA);
        }
        self.miniature_volute = volute;
        esito
    }

    /// Il pizzico di questo frame, dalle dita (o da Ctrl e la rotella sul PC): lo zoom vivo
    /// cambia e lo scorrimento tiene fermo fra le dita il punto della pagina che c'era, anche se
    /// le dita si spostano.
    fn pizzico(&mut self, ui: &Ui, rect: Rect, imp: &Impaginazione, adesso: f64) {
        let (fattore, dita, sopra) = ui.input(|i| (i.zoom_delta(), i.multi_touch(), i.pointer.hover_pos()));
        if dita.is_some() {
            self.dita = true;
        }
        let centro_prima = self.centro_dita;
        self.centro_dita = dita.map(|d| d.center_pos);
        let centro = self.centro_dita.or(centro_prima).or(sopra).unwrap_or(rect.center());
        let da = centro_prima.unwrap_or(centro);
        let spostate = dita.is_some() && da != centro;
        if fattore == 1.0 && !spostate {
            return;
        }
        let zoom = self.zoom * fattore;
        let (nuova, s) = imp.zoom_attorno(zoom, da - rect.min, centro - rect.min, self.scorrimento, rect.size());
        if nuova.zoom() != self.zoom {
            self.ultimo_zoom = adesso;
        }
        self.zoom = nuova.zoom();
        self.forza = Some(s);
        self.ultimo_movimento = Some(adesso);
    }

    /// A fine frame: con tutte le dita alzate il pizzico è finito.
    fn fine_pizzico(&mut self, ui: &Ui) {
        if self.dita && !ui.input(|i| i.any_touches() || i.pointer.any_down()) {
            self.dita = false;
            self.centro_dita = None;
            ui.ctx().request_repaint();
        }
    }

    /// I risultati del pool, al massimo `RISULTATI_PER_FRAME` e `BYTE_PER_FRAME`, in texture.
    fn ricevi(&mut self, ctx: &egui::Context, esito: &mut Esito) {
        let (mut byte, mut immagini) = (0, 0);
        // I testi sono piccoli: per frame si prendono più risultati, ma al massimo
        // `RISULTATI_PER_FRAME` immagini.
        for _ in 0..RISULTATI_PER_FRAME * 4 {
            if byte >= BYTE_PER_FRAME || immagini >= RISULTATI_PER_FRAME {
                ctx.request_repaint();
                break;
            }
            let Some(r) = self.pool.try_recv() else { break };
            let (req, millis) = (r.request, r.millis);
            if req.kind == RequestKind::Text {
                self.testo_arrivato(ctx, req.page, r.text, esito);
                continue;
            }
            immagini += 1;
            let intera = req.tag == INTERA;
            let miniatura = req.tag == MINIATURA;
            // Una tessera di uno zoom passato: non serve più.
            if !intera && !miniatura && req.tag != self.generazione {
                continue;
            }
            let img = match immagine(r) {
                Ok(img) => img,
                Err(e) => {
                    self.fallite.insert(req.page);
                    esito.errore = Some(e.diagnosi());
                    continue;
                }
            };
            let px = match req.tile {
                Some(t) => Rettangolo { x: t.x, y: t.y, larghezza: t.w, altezza: t.h },
                None => Rettangolo { x: 0, y: 0, larghezza: img.size[0] as u32, altezza: img.size[1] as u32 },
            };
            let (w, h) = (img.size[0] as u32, img.size[1] as u32);
            let n = img.size[0] * img.size[1] * 4;
            byte += n;
            let nome = if intera {
                format!("pdf-pagina-{}", req.page)
            } else if miniatura {
                format!("pdf-miniatura-{}", req.page)
            } else {
                format!("pdf-tessera-{}-{}-{}", req.page, px.x, px.y)
            };
            let tex = ctx.load_texture(nome, img, TextureOptions::LINEAR);
            let im = Immagine { tex, scala: req.scale, px, byte: n };
            self.conti.ms_totali += u64::from(millis);
            self.conti.ms_massimo = self.conti.ms_massimo.max(millis);
            if intera {
                self.conti.pagine += 1;
                self.da_rifare.remove(&req.page);
                self.intere.insert(req.page, im);
                if !self.prima_vista {
                    self.prima_vista = true;
                    let dopo_ms = self.nato.elapsed().as_secs_f32() * 1000.0;
                    esito.prima_pagina = Some(PrimaPagina { dopo_ms, disegno_ms: millis, larghezza: w, altezza: h });
                }
            } else if miniatura {
                self.conti.miniature += 1;
                self.miniature.insert(req.page, im);
            } else {
                self.conti.tessere += 1;
                self.tessere.insert((req.page, px.x / crate::geometria::TESSERA, px.y / crate::geometria::TESSERA), im);
            }
        }
    }

    /// La richiesta dell'immagine intera della pagina `p`, se manca o è di una scala lontana.
    fn intera_che_manca(&self, imp: &Impaginazione, p: usize, ppp: f32) -> Option<RenderRequest> {
        if self.fallite.contains(&p) {
            return None;
        }
        let scala = imp.scala_intera(p, ppp);
        if self.intere.get(&p).is_some_and(|im| (im.scala - scala).abs() <= scala * 0.02) && !self.da_rifare.contains(&p) {
            return None;
        }
        // Sotto le tessere basta un'anteprima, senza le immagini minuscole.
        let anteprima = imp.servono_tessere(p, ppp);
        Some(RenderRequest { page: p, kind: RequestKind::Pixels, tile: None, scale: scala, tag: INTERA, preview: anteprima })
    }

    /// La coda per il pool, dalla cosa più urgente: le pagine sullo schermo dalla più vicina al
    /// centro, le loro tessere, poi le pagine vicine sopra e sotto. Al pool va solo se è cambiata.
    fn chiedi(&mut self, imp: &Impaginazione, vista: Rect, ppp: f32, miniature: &[RenderRequest]) {
        let visibili = imp.visibili(vista);
        let centro = imp.corrente(vista);
        let mut sullo_schermo: Vec<usize> = visibili.clone().collect();
        sullo_schermo.sort_by_key(|p| p.abs_diff(centro));
        let mut voluti = Vec::new();
        voluti.extend(sullo_schermo.iter().filter_map(|&p| self.intera_che_manca(imp, p, ppp)));
        // Il testo della pagina di una pressione lunga, subito dopo le pagine sullo schermo: chi
        // ha il dito giù aspetta lui.
        let attesa = self.selezione_attesa.map(|(p, _, _)| p).filter(|p| !self.testi.contains_key(p) && !self.testi_falliti.contains(p));
        if let Some(page) = attesa {
            voluti.push(RenderRequest { page, kind: RequestKind::Text, tile: None, scale: 1.0, tag: TESTO, preview: false });
        }
        self.incomplete.clear();
        for &p in &sullo_schermo {
            if self.fallite.contains(&p) || !imp.servono_tessere(p, ppp) {
                continue;
            }
            for (tx, ty, r) in imp.tessere(p, vista, ppp) {
                if self.tessere.contains_key(&(p, tx, ty)) {
                    continue;
                }
                self.incomplete.insert(p);
                let tile = Tile { x: r.x, y: r.y, w: r.larghezza, h: r.altezza };
                voluti.push(RenderRequest {
                    page: p,
                    kind: RequestKind::Pixels,
                    tile: Some(tile),
                    scale: imp.scala(p, ppp),
                    tag: self.generazione,
                    preview: false,
                });
            }
        }
        if !visibili.is_empty() {
            for d in 1..=VICINE {
                let sopra = visibili.start.checked_sub(d);
                let sotto = Some(visibili.end - 1 + d).filter(|p| *p < imp.pagine());
                voluti.extend([sopra, sotto].into_iter().flatten().filter_map(|p| self.intera_che_manca(imp, p, ppp)));
            }
        }
        // Poi le miniature della griglia, se è aperta, e i testi che mancano alla ricerca: dopo
        // le immagini, nella stessa coda.
        // Una miniatura arrivata in questo frame era ancora nella lista della griglia, fatta al
        // frame prima: chiederla di nuovo la farebbe ridisegnare.
        let pronta = |m: &RenderRequest| self.miniature.get(&m.page).is_some_and(|im| (im.scala - m.scale).abs() <= m.scale * 0.05);
        voluti.extend(miniature.iter().filter(|m| !self.fallite.contains(&m.page) && !pronta(m)).copied());
        if let Some(r) = &self.ricerca {
            let testi = r.mancanti().into_iter().filter(|p| !self.testi_falliti.contains(p) && !self.testi.contains_key(p) && Some(*p) != attesa);
            voluti.extend(testi.take(TESTI_IN_CODA).map(|page| RenderRequest {
                page,
                kind: RequestKind::Text,
                tile: None,
                scale: 1.0,
                tag: TESTO,
                preview: false,
            }));
        }
        if voluti != self.coda {
            self.pool.set_queue(voluti.clone());
            self.coda = voluti;
        }
    }

    /// Le pagine sullo schermo: l'ombra, il foglio bianco, l'immagine intera (anche di un'altra
    /// scala, stirata), le tessere vecchie e quelle nitide. Ogni pagina comincia su un pixel.
    /// Restituisce le immagini disegnate e dove, per la lente.
    fn dipingi(&self, ui: &Ui, imp: &Impaginazione, origine: Pos2, vista: Rect, ppp: f32) -> Vec<(TextureId, Rect)> {
        let mut disegnate = Vec::new();
        let painter = ui.painter();
        let sul_pixel = |v: f32| (v * ppp).round() / ppp;
        let uv = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
        for p in imp.visibili(vista) {
            let esatta = imp.rettangolo(p).translate(origine.to_vec2());
            let spostamento = pos2(sul_pixel(esatta.left()), sul_pixel(esatta.top())) - esatta.min;
            let pagina = esatta.translate(spostamento);
            tema::dipingi_ombra(painter, pagina, 0, &tema::OMBRA_PAGINA);
            painter.rect_filled(pagina, 0.0, tema::FOGLIO);
            let dentro = painter.with_clip_rect(pagina.intersect(painter.clip_rect()));
            let posto = |im: &Immagine| imp.posto_dei_pixel(p, im.scala, im.px).translate(origine.to_vec2() + spostamento);
            let immagini = self
                .intere
                .get(&p)
                .into_iter()
                .chain(self.vecchie.iter().filter(|(q, _)| *q == p).map(|(_, im)| im))
                .chain(self.tessere.iter().filter(|((q, _, _), _)| *q == p).map(|(_, im)| im));
            for im in immagini {
                // Tagliata alla pagina, come la disegna `dentro`.
                let r = posto(im);
                dentro.image(im.tex.id(), r, uv, Color32::WHITE);
                disegnate.push((im.tex.id(), r));
            }
            // I risultati della ricerca, in giallo tenue; l'attuale più forte.
            if let Some(r) = &self.ricerca {
                let k = imp.dp_per_pt(p);
                for (i, righe) in r.risultati(p).iter().enumerate() {
                    let colore = if r.attuale() == Some((p, i)) { tema::TROVATO_ATTUALE } else { tema::TROVATO };
                    for &[x0, y0, x1, y1] in righe {
                        let riga = Rect::from_min_max(pagina.min + vec2(x0, y0) * k, pagina.min + vec2(x1, y1) * k).expand(1.0);
                        dentro.rect_filled(riga, 2.0, colore);
                    }
                }
            }
        }
        disegnate
    }

    /// Via le texture che non servono: le pagine intere lontane (e, oltre il tetto, le più
    /// lontane dalla pagina corrente), le tessere fuori dallo schermo, quelle vecchie di una
    /// pagina già completa allo zoom nuovo.
    fn libera(&mut self, imp: &Impaginazione, vista: Rect, ppp: f32, calmo: bool) {
        let visibili = imp.visibili(vista);
        let centro = imp.corrente(vista);
        let lontana = |p: usize| p + VICINE + 1 < visibili.start || p > visibili.end + VICINE;
        self.intere.retain(|p, _| *p < imp.pagine() && !lontana(*p));
        let mut byte: usize = self.intere.values().map(|i| i.byte).sum();
        if byte > TETTO_PAGINE {
            // Oltre il tetto si toglie solo quello che `chiedi` non vuole (le pagine sullo schermo
            // e le `VICINE` sopra e sotto): a zoom alto con due pagine sullo schermo quelle volute
            // superano il tetto da sole, e toglierne una la farebbe ridisegnare per sempre.
            let voluta = |p: usize| p + VICINE >= visibili.start && p < visibili.end + VICINE;
            let mut fuori: Vec<usize> = self.intere.keys().copied().filter(|p| !voluta(*p)).collect();
            fuori.sort_by_key(|p| std::cmp::Reverse(p.abs_diff(centro)));
            for p in fuori {
                if byte <= TETTO_PAGINE {
                    break;
                }
                if let Some(im) = self.intere.remove(&p) {
                    byte -= im.byte;
                }
            }
        }
        let si_vede = |p: usize, im: &Immagine| visibili.contains(&p) && imp.posto_dei_pixel(p, im.scala, im.px).intersects(vista);
        self.tessere.retain(|(p, _, _), im| si_vede(*p, im));
        self.vecchie.retain(|(p, im)| si_vede(*p, im));
        if calmo {
            // Una pagina completa allo zoom nuovo non ha più bisogno delle tessere vecchie.
            let completa = |p: usize| {
                let scala = imp.scala_intera(p, ppp);
                let intera = self.intere.get(&p).is_some_and(|im| (im.scala - scala).abs() <= scala * 0.02);
                intera && !self.incomplete.contains(&p) && !self.da_rifare.contains(&p)
            };
            let complete: HashSet<usize> = self.vecchie.iter().map(|(p, _)| *p).filter(|p| completa(*p)).collect();
            self.vecchie.retain(|(p, _)| !complete.contains(p));
        }
        // Oltre il tetto, via prima le vecchie (le più vecchie per prime).
        let mut byte: usize = self.tessere.values().chain(self.vecchie.iter().map(|(_, i)| i)).map(|i| i.byte).sum();
        while byte > TETTO_TESSERE && !self.vecchie.is_empty() {
            byte -= self.vecchie.remove(0).1.byte;
        }
        // Miniature e testi oltre i loro tetti: via i più lontani dalla griglia (o dalla pagina
        // che si legge).
        let attorno = self.centro_miniature.unwrap_or(centro);
        let mut byte: usize = self.miniature.values().map(|i| i.byte).sum();
        if byte > TETTO_MINIATURE {
            // Mai quelle che la griglia vuole: col telefono girato le celle sono grandi, e quelle
            // a vista con una fila sopra e sotto superano il tetto da sole.
            let volute = self.miniature_attorno.clone();
            let mut lontane: Vec<usize> = self.miniature.keys().copied().filter(|p| !volute.contains(p)).collect();
            lontane.sort_by_key(|p| std::cmp::Reverse(p.abs_diff(attorno)));
            for p in lontane {
                if byte <= TETTO_MINIATURE {
                    break;
                }
                if let Some(im) = self.miniature.remove(&p) {
                    byte -= im.byte;
                }
            }
        }
        let peso = |p: &usize, t: &Arc<PageText>, normali: &HashMap<usize, TestoNormale>| {
            t.glyphs.len() * BYTE_PER_CARATTERE + normali.get(p).map_or(0, TestoNormale::byte)
        };
        let mut byte: usize = self.testi.iter().map(|(p, t)| peso(p, t, &self.normali)).sum();
        if byte > TETTO_TESTI {
            // Mai il testo della pagina con la selezione.
            let selezionata = self.selezione.map(|s| s.pagina);
            let mut lontane: Vec<usize> = self.testi.keys().copied().filter(|p| Some(*p) != selezionata).collect();
            lontane.sort_by_key(|p| std::cmp::Reverse(p.abs_diff(centro)));
            for p in lontane {
                if byte <= TETTO_TESTI {
                    break;
                }
                if let Some(t) = self.testi.remove(&p) {
                    byte -= peso(&p, &t, &self.normali);
                    self.normali.remove(&p);
                }
            }
        }
    }

    /// «2 / 12» in una pillola scura a due terzi del lettore: compare quando la vista si muove
    /// (e all'apertura) e sfuma poco dopo.
    fn indicatore(&self, ui: &Ui, rect: Rect, adesso: f64) {
        let n = self.misure.len();
        let Some(imp) = self.imp.as_ref() else { return };
        if n == 0 {
            return;
        }
        let passato = adesso - self.ultimo_movimento.unwrap_or(adesso);
        let visibile = passato < DURATA_INDICATORE;
        if visibile {
            ui.ctx().request_repaint_after(Duration::from_secs_f64(DURATA_INDICATORE - passato));
        }
        let alfa = ui.ctx().animate_bool_with_time(self.id.with("indicatore"), visibile, SFUMATURA_INDICATORE);
        if alfa <= 0.0 {
            return;
        }
        let corrente = imp.corrente(Rect::from_min_size(self.scorrimento.to_pos2(), rect.size()));
        let testo = format!("{} / {n}", corrente + 1);
        let font = Carattere::Grassetto.font(ui.ctx(), 13.0);
        let g = ui.painter().layout_no_wrap(testo.clone(), font, Color32::WHITE.gamma_multiply(alfa));
        // `.numero-pag`: 5 dp sopra e sotto, 12 ai lati, angoli da 14.
        let misura = vec2(g.size().x + 24.0, (g.size().y + 10.0).max(28.0));
        let centro = pos2(rect.center().x, rect.top() + rect.height() * 2.0 / 3.0);
        let pillola = Rect::from_center_size(centro, misura);
        ui.painter().rect_filled(pillola, misura.y / 2.0, tema::INDICATORE.gamma_multiply(alfa));
        ui.painter().galley(pillola.center() - g.size() / 2.0, g, Color32::PLACEHOLDER);
        ui.interact(pillola, self.id.with("pagina"), Sense::hover()).widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, &testo));
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::geometria::{TESSERA, ZOOM_DOPPIO_TOCCO};
    use crate::prova::{Bersaglio, Collegamento as Link, Pagina, pdf, pdf_completo};

    /// Un telefono finto: 412 × 851 dp (lo schermo sotto la barra alta) a 2,625 pixel per dp.
    struct Telefono {
        ctx: egui::Context,
        t: f64,
    }

    struct Frame {
        esito: Esito,
        forme: Vec<egui::epaint::ClippedShape>,
    }

    impl Frame {
        /// I testi disegnati.
        fn testi(&self) -> Vec<String> {
            self.forme
                .iter()
                .filter_map(|f| match &f.shape {
                    egui::Shape::Text(t) => Some(t.galley.text().to_string()),
                    _ => None,
                })
                .collect()
        }

        /// Dove sono disegnate le immagini.
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

    fn lettore(pagine: usize) -> Lettore {
        let pagina = Pagina { larghezza: 595.0, altezza: 842.0, rotazione: 0, contenuto: "0 0 1 rg 50 50 495 742 re f" };
        let p = Pdf::apri(pdf(&vec![pagina; pagine]), None).unwrap();
        Lettore::new(&p.istantanea(), Id::new("lettore di prova"))
    }

    fn tasto(pressed: bool, pos: Pos2) -> egui::Event {
        egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE }
    }

    impl Telefono {
        fn new() -> Self {
            Self { ctx: egui::Context::default(), t: 0.0 }
        }

        fn passo(&mut self, l: &mut Lettore, eventi: Vec<egui::Event>) -> Frame {
            let mut ri = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(412.0, 851.0))),
                max_texture_side: Some(4096),
                time: Some(self.t),
                predicted_dt: 1.0 / 60.0,
                events: eventi,
                ..Default::default()
            };
            ri.viewports.entry(egui::ViewportId::ROOT).or_default().native_pixels_per_point = Some(2.625);
            self.t += 1.0 / 60.0;
            let mut esito = Esito::default();
            let mut out = self.ctx.run_ui(ri, |ui| esito = l.mostra(ui));
            // Un delta buttato senza svuotarlo manda epaint in panico (debug_assert nel Drop).
            out.textures_delta.clear();
            Frame { esito, forme: std::mem::take(&mut out.shapes) }
        }

        /// Frame finché `fatto` non è vero (il pool disegna sui suoi thread), al massimo 20 s;
        /// restituisce gli esiti visti.
        fn aspetta(&mut self, l: &mut Lettore, fatto: impl Fn(&Lettore) -> bool) -> Vec<Esito> {
            let fine = Instant::now() + Duration::from_secs(20);
            let mut esiti = Vec::new();
            while !fatto(l) {
                assert!(Instant::now() < fine, "il lettore non ha finito in 20 s");
                esiti.push(self.passo(l, vec![]).esito);
                std::thread::sleep(Duration::from_millis(5));
            }
            esiti.push(self.passo(l, vec![]).esito);
            esiti
        }

        /// Un frame col lettore e, sopra, la griglia delle miniature in un'area che scorre larga
        /// 376 dp dal punto (18, 300), alta 500: restituisce la pagina toccata nella griglia.
        fn passo_con_griglia(&mut self, l: &mut Lettore, eventi: Vec<egui::Event>) -> Option<usize> {
            let mut ri = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(412.0, 851.0))),
                max_texture_side: Some(4096),
                time: Some(self.t),
                predicted_dt: 1.0 / 60.0,
                events: eventi,
                ..Default::default()
            };
            ri.viewports.entry(egui::ViewportId::ROOT).or_default().native_pixels_per_point = Some(2.625);
            self.t += 1.0 / 60.0;
            let mut scelta = None;
            let mut out = self.ctx.run_ui(ri, |ui| {
                l.mostra(ui);
                egui::Area::new(Id::new("griglia")).fixed_pos(pos2(18.0, 300.0)).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
                    egui::ScrollArea::vertical().max_height(500.0).show(ui, |ui| {
                        ui.set_width(376.0);
                        scelta = l.mostra_miniature(ui);
                    });
                });
            });
            out.textures_delta.clear();
            scelta
        }

        /// Come [`passo_con_griglia`](Self::passo_con_griglia), con la griglia di «Organizza
        /// pagine»: l'area non scorre col dito mentre si tiene una pagina.
        fn passo_con_organizza(&mut self, l: &mut Lettore, org: &mut Organizza, eventi: Vec<egui::Event>) -> EsitoGriglia {
            let mut ri = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(412.0, 851.0))),
                max_texture_side: Some(4096),
                time: Some(self.t),
                predicted_dt: 1.0 / 60.0,
                events: eventi,
                ..Default::default()
            };
            ri.viewports.entry(egui::ViewportId::ROOT).or_default().native_pixels_per_point = Some(2.625);
            self.t += 1.0 / 60.0;
            let mut esito = EsitoGriglia::default();
            let mut out = self.ctx.run_ui(ri, |ui| {
                l.mostra(ui);
                egui::Area::new(Id::new("griglia")).fixed_pos(pos2(18.0, 300.0)).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
                    let sorgente = sorgente_dello_scorrimento(false, !org.tiene());
                    egui::ScrollArea::vertical().max_height(500.0).scroll_source(sorgente).show(ui, |ui| {
                        ui.set_width(376.0);
                        esito = l.mostra_organizza(ui, org);
                    });
                });
            });
            out.textures_delta.clear();
            esito
        }

        /// Frame vuoti per `secondi` del tempo di egui.
        fn passa(&mut self, l: &mut Lettore, secondi: f64) -> Frame {
            let fine = self.t + secondi;
            let mut f = self.passo(l, vec![]);
            while self.t < fine {
                f = self.passo(l, vec![]);
            }
            f
        }
    }

    #[test]
    fn disegna_le_pagine_sullo_schermo_e_le_vicine() {
        let mut l = lettore(6);
        let mut tel = Telefono::new();
        let esiti = tel.aspetta(&mut l, |l| l.pagine_pronte() == [0, 1, 2, 3] && l.coda.is_empty());
        // La prima pagina si dice una volta, larga quanto lo schermo meno i margini, in pixel.
        let prime: Vec<PrimaPagina> = esiti.iter().filter_map(|e| e.prima_pagina).collect();
        assert_eq!(prime.len(), 1);
        assert_eq!((prime[0].larghezza, prime[0].altezza), (998, 1412));
        // Due pagine sullo schermo, due pronte per lo scorrimento; le altre no.
        let f = tel.passo(&mut l, vec![]);
        let immagini = f.immagini();
        assert_eq!(immagini.len(), 2, "{immagini:?}");
        let prima = immagini.iter().find(|r| r.top() < 100.0).expect("la prima pagina");
        assert!((prima.left() - 16.0).abs() < 0.5 && (prima.top() - 14.0).abs() < 0.5, "{prima:?}");
        // A pixel pieni: l'immagine larga 998 pixel occupa 998 / 2,625 dp.
        assert!((prima.width() - 998.0 / 2.625).abs() < 0.01, "{prima:?}");
        assert_eq!(l.quante_tessere(), (0, 0));
        assert!(l.riga_misure().starts_with("4 pagine intere, 0 tessere e 0 miniature disegnate"), "{}", l.riga_misure());
    }

    #[test]
    fn il_pizzico_ingrandisce_e_poi_arrivano_le_tessere_nitide() {
        let mut l = lettore(3);
        let mut tel = Telefono::new();
        tel.aspetta(&mut l, |l| l.pagine_pronte().contains(&0));
        let dita = pos2(200.0, 300.0);
        let f = tel.passo(&mut l, vec![egui::Event::PointerMoved(dita), egui::Event::Zoom(2.0)]);
        assert_eq!(l.zoom(), 2.0);
        // Subito si ingrandisce l'immagine che c'è: lo zoom nitido resta quello di prima.
        assert_eq!(l.zoom_nitido(), 1.0);
        assert!(f.esito.zoom.is_none());
        // Dopo un attimo di calma lo zoom diventa nitido, una volta.
        let esiti = tel.aspetta(&mut l, |l| l.zoom_nitido() == 2.0 && l.coda.is_empty());
        assert_eq!(esiti.iter().filter_map(|e| e.zoom).collect::<Vec<_>>(), [2.0]);
        // A 2 volte la A4 è di 1995 × 2823 pixel: intera fino a 2048, il resto a tessere.
        let (tessere, vecchie) = l.quante_tessere();
        assert!((1..=12).contains(&tessere) && vecchie == 0, "{tessere} tessere, {vecchie} vecchie");
        let (_, byte) = l.byte_texture();
        assert!(byte <= tessere * (TESSERA * TESSERA * 4) as usize && byte <= TETTO_TESSERE);
        // Ancora più vicino: le tessere di prima diventano vecchie, poi spariscono quando la
        // pagina è completa allo zoom nuovo; e le tessere di quello di prima non entrano più.
        tel.passo(&mut l, vec![egui::Event::PointerMoved(dita), egui::Event::Zoom(1.5)]);
        let f = tel.passa(&mut l, CALMA_ZOOM + 0.05);
        assert_eq!(f.esito.zoom.or(Some(l.zoom_nitido())), Some(3.0));
        tel.aspetta(&mut l, |l| l.coda.is_empty() && l.quante_tessere().1 == 0);
        assert!(l.tessere.values().all(|im| (im.scala - 3.0 * 380.0 / 595.0 * 2.625).abs() < 0.01));
        assert!(l.conti().tessere as usize >= l.quante_tessere().0);
    }

    #[test]
    fn il_doppio_tocco_ingrandisce_e_poi_torna_alla_larghezza() {
        let mut l = lettore(2);
        let mut tel = Telefono::new();
        tel.passo(&mut l, vec![]);
        let doppio = |tel: &mut Telefono, l: &mut Lettore, p: Pos2| {
            tel.passo(l, vec![egui::Event::PointerMoved(p), tasto(true, p)]);
            tel.passo(l, vec![tasto(false, p)]);
            tel.passo(l, vec![tasto(true, p)]);
            tel.passo(l, vec![tasto(false, p)]);
            tel.passo(l, vec![egui::Event::PointerGone]);
        };
        let p = pos2(300.0, 200.0);
        doppio(&mut tel, &mut l, p);
        assert_eq!(l.zoom(), ZOOM_DOPPIO_TOCCO);
        tel.passo(&mut l, vec![]);
        // Il punto toccato resta sotto il dito.
        let imp = l.imp.clone().unwrap();
        let (pagina, pt) = imp.punto((l.scorrimento() + p.to_vec2()).to_pos2()).unwrap();
        let prima = Impaginazione::new(l.misure.clone(), 412.0, 1.0).punto(p).unwrap();
        assert_eq!(pagina, prima.0);
        assert!((pt - prima.1).length() < 1.0, "{pt:?} {:?}", prima.1);
        // Un attimo dopo (subito sarebbe un triplo tocco), di nuovo: si torna alla larghezza.
        tel.passa(&mut l, 0.5);
        doppio(&mut tel, &mut l, p);
        assert_eq!(l.zoom(), 1.0);
        tel.passo(&mut l, vec![]);
        assert_eq!(l.scorrimento().x, 0.0);
    }

    #[test]
    fn le_pagine_lontane_si_liberano() {
        let mut l = lettore(40);
        let mut tel = Telefono::new();
        tel.aspetta(&mut l, |l| l.pagine_pronte() == [0, 1, 2, 3] && l.coda.is_empty());
        // Un salto alla pagina 30: restano solo le immagini attorno.
        let cima = Impaginazione::new(l.misure.clone(), 412.0, 1.0).rettangolo(30).top();
        l.scorri_a(vec2(0.0, cima));
        tel.aspetta(&mut l, |l| l.pagine_pronte().contains(&30) && l.coda.is_empty());
        let pronte = l.pagine_pronte();
        assert!(pronte.iter().all(|p| (27..=33).contains(p)), "{pronte:?}");
        let (byte, _) = l.byte_texture();
        assert!(byte <= TETTO_PAGINE);
        assert_eq!(l.corrente(vec2(412.0, 851.0)), 30);
    }

    /// Cinque pagine A4: «Perché» due volte nella quarta, «PERCHÉ» nella quinta.
    fn lettore_con_testo() -> Lettore {
        let quarta = "BT /F1 14 Tf 60 700 Td (Perch\\351 no) Tj 0 -40 Td (e perche si) Tj ET";
        let quinta = "BT /F1 14 Tf 60 300 Td (PERCH\\311 forse) Tj ET";
        let pagina = |contenuto| Pagina { larghezza: 595.0, altezza: 842.0, rotazione: 0, contenuto };
        let p = Pdf::apri(pdf(&[pagina(""), pagina(""), pagina(""), pagina(quarta), pagina(quinta)]), None).unwrap();
        Lettore::new(&p.istantanea(), Id::new("lettore col testo"))
    }

    fn rettangoli(f: &Frame, colore: Color32) -> Vec<Rect> {
        f.forme
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Rect(r) if r.fill == colore => Some(r.rect),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn la_ricerca_conta_salta_al_risultato_e_lo_evidenzia() {
        let mut l = lettore_con_testo();
        let mut tel = Telefono::new();
        tel.aspetta(&mut l, |l| l.pagine_pronte().contains(&0) && !l.in_attesa());
        l.cerca("perche");
        let esiti = tel.aspetta(&mut l, |l| l.conteggio().is_some_and(|c| c.finita) && !l.in_attesa());
        assert_eq!(l.conteggio(), Some(Conteggio { attuale: Some(0), totale: 3, finita: true }));
        // La ricerca finita si dice una volta, coi numeri.
        let finite: Vec<RicercaFinita> = esiti.iter().filter_map(|e| e.ricerca_finita).collect();
        assert_eq!(finite.len(), 1);
        assert_eq!((finite[0].risultati, finite[0].pagine), (3, 5));
        // La vista è andata al primo risultato, nella quarta pagina.
        let f = tel.passa(&mut l, 0.1);
        assert_eq!(l.pagina_corrente(), 3);
        let attuale = rettangoli(&f, tema::TROVATO_ATTUALE);
        // Gli altri sullo schermo: la quinta pagina comincia sotto.
        let altri: Vec<Rect> = rettangoli(&f, tema::TROVATO).into_iter().filter(|r| r.top() < 851.0).collect();
        assert_eq!((attuale.len(), altri.len()), (1, 1), "{attuale:?} {altri:?}");
        // Il risultato attuale sta sopra l'altro, entrambi sullo schermo.
        assert!(attuale[0].top() < altri[0].top() && attuale[0].top() > 0.0 && altri[0].bottom() < 851.0);
        // Avanti due volte: la quinta pagina.
        l.risultato_altro(true);
        l.risultato_altro(true);
        tel.passa(&mut l, 0.1);
        assert_eq!(l.conteggio().unwrap().attuale, Some(2));
        let f = tel.passo(&mut l, vec![]);
        let attuale = rettangoli(&f, tema::TROVATO_ATTUALE);
        assert_eq!(attuale.len(), 1);
        assert!(attuale[0].top() > 0.0 && attuale[0].bottom() < 851.0, "{attuale:?}");
        // La stessa parola con l'accento non ricomincia; una vuota chiude.
        l.cerca("PERCHÉ");
        assert_eq!(l.conteggio().unwrap().attuale, Some(2));
        l.cerca("");
        assert_eq!(l.conteggio(), None);
        let f = tel.passo(&mut l, vec![]);
        assert!(rettangoli(&f, tema::TROVATO).is_empty() && rettangoli(&f, tema::TROVATO_ATTUALE).is_empty());
        // Una seconda ricerca usa i testi già letti, senza chiederli di nuovo.
        let letti = l.conti().testi;
        l.cerca("forse");
        assert_eq!(l.conteggio(), Some(Conteggio { attuale: Some(0), totale: 1, finita: true }));
        tel.passa(&mut l, 0.1);
        assert_eq!(l.conti().testi, letti);
        assert_eq!(l.pagina_corrente(), 4);
    }

    /// Ogni testo si porta alla forma del confronto una volta: le ricerche dopo, a ogni tasto, non
    /// lo rifanno (revisione della 0.3.0: nei PDF lunghi costava 100 ms per tasto).
    #[test]
    fn una_ricerca_nuova_non_rinormalizza_i_testi_gia_letti() {
        let mut l = lettore_con_testo();
        let mut tel = Telefono::new();
        l.cerca("perche");
        tel.aspetta(&mut l, |l| l.conteggio().is_some_and(|c| c.finita) && !l.in_attesa());
        assert_eq!(l.conti().normalizzati, 5);
        for parola in ["f", "fo", "for", "forse"] {
            l.cerca(parola);
        }
        assert_eq!(l.conteggio(), Some(Conteggio { attuale: Some(0), totale: 1, finita: true }));
        assert_eq!(l.conti().normalizzati, 5);
    }

    #[test]
    fn le_miniature_arrivano_e_un_tocco_porta_alla_pagina() {
        let mut l = lettore(9);
        let mut tel = Telefono::new();
        l.apri_miniature();
        let fine = Instant::now() + Duration::from_secs(20);
        // Si vedono due file e mezza: le miniature delle prime nove pagine (una fila oltre).
        while l.miniature_pronte().len() < 9 || l.in_attesa() {
            assert!(Instant::now() < fine, "miniature non arrivate: {:?}", l.miniature_pronte());
            tel.passo_con_griglia(&mut l, vec![]);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(l.conti().miniature >= 9);
        // Piccole: larghe quanto la cella (117 dp), non quanto la pagina.
        let (_, im) = l.miniature.iter().next().unwrap();
        assert!(im.px.larghezza < 330, "{:?}", im.px);
        // La quinta è nella seconda fila, al centro: (18 + 4 + 127 + 57, 300 + 6 + 204 + 80).
        let dove = pos2(205.0, 588.0);
        tel.passo_con_griglia(&mut l, vec![egui::Event::PointerMoved(dove), tasto(true, dove)]);
        let scelta = tel.passo_con_griglia(&mut l, vec![tasto(false, dove)]);
        assert_eq!(scelta, Some(4));
        l.vai_alla_pagina(4);
        tel.passa(&mut l, 0.1);
        assert_eq!(l.pagina_corrente(), 4);
        assert!(l.riga_misure().contains(" miniature disegnate"), "{}", l.riga_misure());
    }

    /// «Organizza pagine»: il tocco dice la pagina, la pressione lunga la prende e, col dito
    /// portato oltre la metà della quinta pagina, lasciata dice il posto dopo di lei; presa una
    /// pagina scelta, si prendono tutte le scelte.
    #[test]
    fn nella_griglia_da_organizzare_si_sceglie_e_si_sposta_col_dito() {
        let mut l = lettore(9);
        let mut tel = Telefono::new();
        let mut org = Organizza::default();
        for _ in 0..4 {
            tel.passo_con_organizza(&mut l, &mut org, vec![]);
        }
        // La terza: prima fila, a destra.
        let terza = pos2(332.0, 387.0);
        tel.passo_con_organizza(&mut l, &mut org, vec![egui::Event::PointerMoved(terza), tasto(true, terza)]);
        let e = tel.passo_con_organizza(&mut l, &mut org, vec![tasto(false, terza)]);
        assert_eq!(e.toccata, Some(2));
        tel.passo_con_organizza(&mut l, &mut org, vec![egui::Event::PointerGone]);
        // La prima presa col dito e portata a destra della quinta: va dopo di lei.
        let prima = pos2(79.0, 387.0);
        let mut esiti = vec![tel.passo_con_organizza(
            &mut l,
            &mut org,
            vec![dito(egui::TouchPhase::Start, prima), egui::Event::PointerMoved(prima), tasto(true, prima)],
        )];
        for _ in 0..60 {
            esiti.push(tel.passo_con_organizza(&mut l, &mut org, vec![]));
        }
        assert!(esiti.iter().any(|e| e.prese == Some(1)), "la pagina non si è presa");
        assert!(org.tiene());
        let quinta_a_destra = pos2(240.0, 591.0);
        for i in 1..=10 {
            let p = prima + (quinta_a_destra - prima) * (i as f32 / 10.0);
            tel.passo_con_organizza(&mut l, &mut org, vec![dito(egui::TouchPhase::Move, p), egui::Event::PointerMoved(p)]);
        }
        let e = tel.passo_con_organizza(&mut l, &mut org, vec![dito(egui::TouchPhase::End, quinta_a_destra), tasto(false, quinta_a_destra)]);
        assert_eq!(e.spostate, Some((vec![0], 5)));
        assert!(!org.tiene());
        tel.passo_con_organizza(&mut l, &mut org, vec![egui::Event::PointerGone]);
        // Con due pagine scelte, prendere una delle due le prende tutte e due; lasciata dove
        // era, niente da spostare. Indietro lascia una pagina presa.
        org.scelte.extend([1, 2]);
        let seconda = pos2(206.0, 387.0);
        let mut esiti = vec![tel.passo_con_organizza(
            &mut l,
            &mut org,
            vec![dito(egui::TouchPhase::Start, seconda), egui::Event::PointerMoved(seconda), tasto(true, seconda)],
        )];
        for _ in 0..60 {
            esiti.push(tel.passo_con_organizza(&mut l, &mut org, vec![]));
        }
        assert!(esiti.iter().any(|e| e.prese == Some(2)));
        let e = tel.passo_con_organizza(&mut l, &mut org, vec![dito(egui::TouchPhase::End, seconda), tasto(false, seconda)]);
        assert_eq!(e.spostate, None);
        tel.passo_con_organizza(&mut l, &mut org, vec![egui::Event::PointerGone]);
        assert!(!org.lascia());
    }

    #[test]
    fn un_link_porta_alla_pagina_e_uno_esterno_torna_all_app() {
        let link = [
            Link { pagina: 0, rettangolo: [50.0, 700.0, 250.0, 720.0], bersaglio: Bersaglio::Pagina(2, Some(442.0)) },
            Link { pagina: 0, rettangolo: [50.0, 600.0, 250.0, 620.0], bersaglio: Bersaglio::Indirizzo("https://esempio.it") },
        ];
        let a4 = Pagina::bianca(595.0, 842.0);
        let p = Pdf::apri(pdf_completo(&[a4; 6], &[], &link), None).unwrap();
        let mut l = Lettore::new(&p.istantanea(), Id::new("lettore coi link"));
        let mut tel = Telefono::new();
        tel.passo(&mut l, vec![]);
        let tocca = |tel: &mut Telefono, l: &mut Lettore, dove: Pos2| {
            tel.passo(l, vec![egui::Event::PointerMoved(dove), tasto(true, dove)]);
            let f = tel.passo(l, vec![tasto(false, dove)]);
            tel.passo(l, vec![egui::Event::PointerGone]);
            f.esito.link
        };
        // A zoom 1 la pagina è a 16 dp dal lato e 14 dalla cima, 380 / 595 dp per pt; il link
        // esterno a 232 pt dall'alto, anche toccato 4 dp sotto il suo bordo.
        let k = 380.0 / 595.0;
        let esterno = tocca(&mut tel, &mut l, pos2(16.0 + 150.0 * k, 14.0 + 242.0 * k + 4.0));
        assert_eq!(esterno, Some(Destinazione::Indirizzo("https://esempio.it".into())));
        assert_eq!(l.pagina_corrente(), 0);
        tel.passa(&mut l, 0.5);
        // Un tocco fuori dai link non fa niente.
        assert_eq!(tocca(&mut tel, &mut l, pos2(300.0, 400.0)), None);
        tel.passa(&mut l, 0.5);
        let interno = tocca(&mut tel, &mut l, pos2(16.0 + 150.0 * k, 14.0 + 132.0 * k));
        assert!(matches!(interno, Some(Destinazione::Pagina { pagina: 2, .. })), "{interno:?}");
        tel.passa(&mut l, 0.1);
        // La pagina 3, col punto della destinazione (400 pt dall'alto) poco sotto la cima.
        let imp = l.imp.clone().unwrap();
        assert_eq!(imp.punto(l.scorrimento().to_pos2() + vec2(200.0, SOPRA_IL_PUNTO)).unwrap().0, 2);
        let punto = imp.posizione(2, pos2(0.0, 400.0)).y - l.scorrimento().y;
        assert!((punto - SOPRA_IL_PUNTO).abs() < 1.0, "{punto}");
    }

    /// Per compilare: un tocco fuori dai link dice la pagina e il punto; il punto toccato si
    /// ritrova sullo schermo; un rettangolo in fondo alla pagina 3 si porta sopra lo spazio in
    /// fondo, uno che si vede già non sposta niente.
    #[test]
    fn il_tocco_sulla_pagina_e_il_rettangolo_da_vedere() {
        let mut l = lettore(4);
        let mut tel = Telefono::new();
        tel.passo(&mut l, vec![]);
        let k = 380.0 / 595.0;
        let dove = pos2(16.0 + 100.0 * k, 14.0 + 200.0 * k);
        tel.passo(&mut l, vec![egui::Event::PointerMoved(dove), tasto(true, dove)]);
        let f = tel.passo(&mut l, vec![tasto(false, dove)]);
        tel.passo(&mut l, vec![egui::Event::PointerGone]);
        let (pagina, pt) = f.esito.tocco.expect("il tocco non è arrivato");
        assert_eq!(pagina, 0);
        assert!((pt.x - 100.0).abs() < 0.5 && (pt.y - 200.0).abs() < 0.5, "{pt:?}");
        assert_eq!(l.sulla_pagina(dove).map(|(p, _)| p), Some(0));
        let r = l.sullo_schermo(0, [100.0, 200.0, 150.0, 220.0]).unwrap();
        assert!((r.left() - dove.x).abs() < 0.5 && (r.top() - dove.y).abs() < 0.5 && (r.width() - 50.0 * k).abs() < 0.5, "{r:?}");
        // Sul tavolo fra due pagine niente.
        assert_eq!(l.sulla_pagina(pos2(5.0, 300.0)), None);
        // Un campo in fondo alla terza pagina, con 300 dp in fondo coperti (la tastiera e la barra).
        l.spazio_sotto(300.0);
        l.mostra_rettangolo(2, [100.0, 780.0, 300.0, 800.0]);
        tel.passa(&mut l, 0.2);
        let r = l.sullo_schermo(2, [100.0, 780.0, 300.0, 800.0]).unwrap();
        assert!(r.top() >= 8.0 && r.bottom() <= 851.0 - 300.0 - 8.0, "il campo non si vede: {r:?}");
        let prima = l.scorrimento();
        l.mostra_rettangolo(2, [100.0, 780.0, 300.0, 800.0]);
        tel.passa(&mut l, 0.2);
        assert_eq!(l.scorrimento(), prima, "un campo che si vede ha spostato la vista");
    }

    #[test]
    fn vai_alla_pagina_e_lo_spazio_sotto() {
        let mut l = lettore(6);
        let mut tel = Telefono::new();
        l.spazio_sotto(88.0);
        l.vai_alla_pagina(5);
        tel.passa(&mut l, 0.1);
        // L'ultima pagina, in fondo: sale sopra lo spazio della barra.
        let imp = l.imp.clone().unwrap();
        let fondo = imp.rettangolo(5).bottom() - l.scorrimento().y;
        assert!(fondo <= 851.0 - 88.0, "{fondo}");
        assert_eq!(l.pagina_corrente(), 5);
        l.vai_alla_pagina(1);
        tel.passa(&mut l, 0.1);
        assert!((imp.rettangolo(1).top() - l.scorrimento().y - CIMA).abs() < 0.5);
        // Una pagina che non c'è: niente.
        l.vai_alla_pagina(60);
        tel.passa(&mut l, 0.1);
        assert_eq!(l.pagina_corrente(), 1);
    }

    /// Due pagine A4: nella prima «Ciao bel mondo» e sotto «seconda riga», la seconda bianca.
    fn lettore_da_selezionare() -> Lettore {
        let contenuto = "BT /F1 14 Tf 60 700 Td (Ciao bel mondo) Tj 0 -40 Td (seconda riga) Tj ET";
        let p = Pdf::apri(pdf(&[Pagina { larghezza: 595.0, altezza: 842.0, rotazione: 0, contenuto }, Pagina::bianca(595.0, 842.0)]), None).unwrap();
        Lettore::new(&p.istantanea(), Id::new("lettore da selezionare"))
    }

    fn dito(fase: egui::TouchPhase, pos: Pos2) -> egui::Event {
        egui::Event::Touch { device_id: egui::TouchDeviceId(1), id: egui::TouchId(0), phase: fase, pos, force: None }
    }

    /// Il dito giù in `p`, fermo un secondo (più della pressione lunga di egui), poi su: gli esiti.
    fn pressione_lunga(tel: &mut Telefono, l: &mut Lettore, p: Pos2) -> Vec<Esito> {
        let mut esiti = vec![tel.passo(l, vec![dito(egui::TouchPhase::Start, p), egui::Event::PointerMoved(p), tasto(true, p)]).esito];
        for _ in 0..60 {
            esiti.push(tel.passo(l, vec![]).esito);
        }
        esiti.push(tel.passo(l, vec![dito(egui::TouchPhase::End, p), tasto(false, p)]).esito);
        esiti.push(tel.passo(l, vec![egui::Event::PointerGone]).esito);
        esiti
    }

    /// Dove sta sullo schermo il punto `pt` della pagina `p` (il lettore prende tutto lo schermo).
    fn sullo_schermo(l: &Lettore, p: usize, pt: Pos2) -> Pos2 {
        l.imp.as_ref().unwrap().posizione(p, pt) - l.scorrimento()
    }

    /// Il centro del primo carattere di `parola` nella prima pagina, sullo schermo.
    fn centro_di(l: &Lettore, parola: &str) -> Pos2 {
        let t = &l.testi[&0];
        let caratteri: String = t.glyphs.iter().map(|g| g.text.as_str()).collect();
        let i = caratteri[..caratteri.find(parola).unwrap()].chars().count();
        let r = t.glyphs[i].rect;
        sullo_schermo(l, 0, pos2((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0))
    }

    /// Il rettangolo del testo `testo` disegnato nel frame.
    fn posto_del_testo(f: &Frame, testo: &str) -> Option<Rect> {
        f.forme.iter().find_map(|c| match &c.shape {
            egui::Shape::Text(t) if t.galley.text() == testo => Some(Rect::from_min_size(t.pos, t.galley.size())),
            _ => None,
        })
    }

    fn tocca(tel: &mut Telefono, l: &mut Lettore, dove: Pos2) -> Esito {
        tel.passo(l, vec![egui::Event::PointerMoved(dove), tasto(true, dove)]);
        let f = tel.passo(l, vec![tasto(false, dove)]);
        tel.passo(l, vec![egui::Event::PointerGone]);
        f.esito
    }

    #[test]
    fn la_pressione_lunga_seleziona_la_parola_con_maniglie_e_barra() {
        let mut l = lettore_da_selezionare();
        let mut tel = Telefono::new();
        tel.aspetta(&mut l, |l| l.pagine_pronte().contains(&0) && !l.in_attesa());
        // Il testo della pagina non c'è ancora: arriva dal pool, poi la parola.
        assert!(l.testi.is_empty());
        let k = 380.0 / 595.0;
        // «bel»: dopo «Ciao » (32,7 pt di Helvetica 14), sulla riga a 142 pt dall'alto.
        let p = pos2(16.0 + (60.0 + 38.0) * k, 14.0 + 137.0 * k);
        let mut esiti = pressione_lunga(&mut tel, &mut l, p);
        esiti.extend(tel.aspetta(&mut l, |l| l.selezione().is_some()));
        assert_eq!(l.testo_selezionato().as_deref(), Some("bel"));
        assert_eq!(esiti.iter().filter_map(|e| e.selezionati).collect::<Vec<_>>(), [3]);
        let f = tel.passa(&mut l, 0.1);
        // La selezione azzurra, le due gocce blu, la barra scura con Copia e Tutto sopra la parola.
        assert_eq!(rettangoli(&f, tema::SELEZIONE).len(), 1);
        assert_eq!(rettangoli(&f, tema::BLU).len(), 2);
        let testi = f.testi();
        assert!(testi.iter().any(|t| t == COPIA) && testi.iter().any(|t| t == TUTTO), "{testi:?}");
        let sel = rettangoli(&f, tema::SELEZIONE)[0];
        assert!(posto_del_testo(&f, COPIA).unwrap().bottom() < sel.top(), "la barra sopra la selezione");
        // «Tutto»: il testo della pagina; «Copia» lo dà a chi usa il lettore e chiude.
        let e = tocca(&mut tel, &mut l, posto_del_testo(&f, TUTTO).unwrap().center());
        assert!(e.tutta && e.selezionati == Some("Ciao bel mondo\nseconda riga".chars().count()));
        assert_eq!(l.testo_selezionato().as_deref(), Some("Ciao bel mondo\nseconda riga"));
        let f = tel.passa(&mut l, 0.1);
        assert_eq!(rettangoli(&f, tema::SELEZIONE).len(), 2);
        let e = tocca(&mut tel, &mut l, posto_del_testo(&f, COPIA).unwrap().center());
        assert_eq!(e.copia.as_deref(), Some("Ciao bel mondo\nseconda riga"));
        assert!(!l.ha_selezione());
        let f = tel.passa(&mut l, 0.1);
        assert!(rettangoli(&f, tema::SELEZIONE).is_empty() && !f.testi().iter().any(|t| t == COPIA));
    }

    #[test]
    fn con_le_azioni_date_la_barra_della_selezione_le_mostra_coi_colori() {
        let mut l = lettore_da_selezionare();
        let azioni = AzioniSelezione {
            azioni: vec![(icone::INK_HIGHLIGHTER, "Evidenzia"), (icone::FORMAT_UNDERLINED, "Sottolinea")],
            colori: vec![(Color32::YELLOW, "Giallo"), (Color32::RED, "Rosso")],
            colore: Some(0),
        };
        l.azioni_della_selezione(Some(azioni));
        let mut tel = Telefono::new();
        tel.aspetta(&mut l, |l| l.pagine_pronte().contains(&0) && !l.in_attesa());
        let k = 380.0 / 595.0;
        pressione_lunga(&mut tel, &mut l, pos2(16.0 + 98.0 * k, 14.0 + 137.0 * k));
        tel.aspetta(&mut l, |l| l.selezione().is_some());
        let f = tel.passa(&mut l, 0.1);
        let testi = f.testi();
        assert!(testi.iter().any(|t| t == "Evidenzia") && !testi.iter().any(|t| t == COPIA), "{testi:?}");
        // Un colore: la selezione resta; un'azione: le righe scelte, e la selezione si chiude.
        let sel = rettangoli(&f, tema::SELEZIONE)[0];
        let evidenzia = posto_del_testo(&f, "Evidenzia").unwrap();
        let rosso = pos2(evidenzia.right() + 10.0, evidenzia.bottom() + 26.0);
        let mut visti = Vec::new();
        for x in [rosso.x - 60.0, rosso.x - 20.0, rosso.x + 20.0, rosso.x + 60.0] {
            if let Some(s) = tocca(&mut tel, &mut l, pos2(x, rosso.y)).scelta_selezione {
                visti.push(s);
            }
        }
        assert!(visti.iter().any(|s| matches!(s, SceltaSelezione::Colore(_))), "{visti:?}");
        assert!(l.ha_selezione());
        let e = tocca(&mut tel, &mut l, evidenzia.center());
        let Some(SceltaSelezione::Azione { indice: 0, pagina: 0, righe }) = e.scelta_selezione else { panic!("{:?}", e.scelta_selezione) };
        assert_eq!(righe.len(), 1);
        let r = sullo_schermo(&l, 0, pos2(righe[0][0], righe[0][1]));
        assert!((r.x - sel.left()).abs() < 1.0 && (r.y - sel.top()).abs() < 1.0, "{r:?} {sel:?}");
        assert!(!l.ha_selezione());
    }

    #[test]
    fn senza_lo_scorrimento_del_dito_la_pagina_resta_e_i_link_non_si_seguono() {
        let link = [Link { pagina: 0, rettangolo: [50.0, 700.0, 250.0, 720.0], bersaglio: Bersaglio::Pagina(2, None) }];
        let a4 = Pagina::bianca(595.0, 842.0);
        let p = Pdf::apri(pdf_completo(&[a4; 4], &[], &link), None).unwrap();
        let mut l = Lettore::new(&p.istantanea(), Id::new("lettore fermo"));
        let mut tel = Telefono::new();
        tel.passo(&mut l, vec![]);
        let trascina = |tel: &mut Telefono, l: &mut Lettore| {
            let da = pos2(200.0, 700.0);
            tel.passo(l, vec![dito(egui::TouchPhase::Start, da), egui::Event::PointerMoved(da), tasto(true, da)]);
            for i in 1..=20 {
                let qui = da - vec2(0.0, 20.0 * i as f32);
                tel.passo(l, vec![dito(egui::TouchPhase::Move, qui), egui::Event::PointerMoved(qui)]);
            }
            let fine = da - vec2(0.0, 400.0);
            tel.passo(l, vec![dito(egui::TouchPhase::End, fine), tasto(false, fine), egui::Event::PointerGone]);
            tel.passa(l, 0.5);
        };
        l.dito_scorre(false);
        trascina(&mut tel, &mut l);
        assert_eq!(l.scorrimento().y, 0.0);
        l.segue_link(false);
        let k = 380.0 / 595.0;
        let dove = pos2(16.0 + 150.0 * k, 14.0 + 132.0 * k);
        tel.passo(&mut l, vec![egui::Event::PointerMoved(dove), tasto(true, dove)]);
        let f = tel.passo(&mut l, vec![tasto(false, dove)]);
        assert_eq!(f.esito.link, None);
        assert!(matches!(f.esito.tocco, Some((0, _))));
        tel.passa(&mut l, 0.5);
        assert_eq!(l.pagina_corrente(), 0);
        use egui::scroll_area::ScrollSource;
        assert_eq!(sorgente_dello_scorrimento(false, true), ScrollSource::ALL);
        assert_eq!(sorgente_dello_scorrimento(false, false), ScrollSource::SCROLL_BAR | ScrollSource::MOUSE_WHEEL);
        assert_eq!(sorgente_dello_scorrimento(true, true), ScrollSource::NONE);
    }

    #[test]
    fn un_pizzico_col_pollice_fermo_non_seleziona() {
        let mut l = lettore_da_selezionare();
        let mut tel = Telefono::new();
        tel.aspetta(&mut l, |l| l.pagine_pronte().contains(&0) && !l.in_attesa());
        let k = 380.0 / 595.0;
        let fermo = pos2(16.0 + 98.0 * k, 14.0 + 137.0 * k);
        let altro = |id, fase, pos| egui::Event::Touch { device_id: egui::TouchDeviceId(1), id: egui::TouchId(id), phase: fase, pos, force: None };
        tel.passo(&mut l, vec![altro(0, egui::TouchPhase::Start, fermo), egui::Event::PointerMoved(fermo), tasto(true, fermo)]);
        tel.passo(&mut l, vec![altro(1, egui::TouchPhase::Start, pos2(300.0, 500.0))]);
        // Un secondo col pollice fermo e l'altro dito che si allarga.
        for i in 1..=60 {
            tel.passo(&mut l, vec![altro(1, egui::TouchPhase::Move, pos2(300.0 + i as f32, 500.0 + i as f32))]);
        }
        tel.passo(&mut l, vec![altro(1, egui::TouchPhase::End, pos2(360.0, 560.0))]);
        tel.passo(&mut l, vec![altro(0, egui::TouchPhase::End, fermo), tasto(false, fermo), egui::Event::PointerGone]);
        tel.passa(&mut l, 0.3);
        assert!(l.zoom() > 1.0, "il pizzico non ha ingrandito");
        assert!(!l.ha_selezione(), "il pizzico ha selezionato");
    }

    #[test]
    fn la_maniglia_allarga_la_selezione_con_la_lente_e_un_tocco_la_chiude() {
        let mut l = lettore_da_selezionare();
        let mut tel = Telefono::new();
        tel.aspetta(&mut l, |l| l.pagine_pronte().contains(&0) && !l.in_attesa());
        let k = 380.0 / 595.0;
        pressione_lunga(&mut tel, &mut l, pos2(16.0 + 98.0 * k, 14.0 + 137.0 * k));
        tel.aspetta(&mut l, |l| l.selezione().is_some());
        // La goccia della fine, appesa sotto la riga a destra di «bel»: portata fino a «mondo».
        let sel = l.selezione().unwrap();
        let (_, fine) = sel.bordi(&l.testi[&0]).unwrap();
        let b = tascabile_touch::Bordo {
            x: sullo_schermo(&l, 0, pos2(fine.x, 0.0)).x,
            cima: sullo_schermo(&l, 0, pos2(0.0, fine.cima)).y,
            fondo: sullo_schermo(&l, 0, pos2(0.0, fine.fondo)).y,
        };
        let presa = Maniglie::centro(Maniglia::Fine, b);
        // Oltre la metà dell'ultima «o» (da 35 a 42,8 pt dopo l'inizio di «mondo»).
        let arrivo = centro_di(&l, "mondo") + vec2(36.0 * k, 0.0) - (b.punto() - presa);
        tel.passo(&mut l, vec![dito(egui::TouchPhase::Start, presa), egui::Event::PointerMoved(presa), tasto(true, presa)]);
        let mut lente = false;
        for i in 1..=8 {
            let p = presa + (arrivo - presa) * (i as f32 / 8.0);
            let f = tel.passo(&mut l, vec![dito(egui::TouchPhase::Move, p), egui::Event::PointerMoved(p)]);
            lente |= f.forme.iter().any(|f| matches!(&f.shape, egui::Shape::Rect(r) if (r.rect.size() - tascabile_touch::LENTE).length() < 0.5));
            // Mentre il dito porta la goccia la barra non c'è.
            assert!(!f.testi().iter().any(|t| t == COPIA));
        }
        assert!(lente, "la lente non si è vista");
        let mut esiti = vec![tel.passo(&mut l, vec![dito(egui::TouchPhase::End, arrivo), tasto(false, arrivo)]).esito];
        esiti.push(tel.passo(&mut l, vec![egui::Event::PointerGone]).esito);
        assert_eq!(l.testo_selezionato().as_deref(), Some("bel mondo"));
        assert_eq!(esiti.iter().filter_map(|e| e.selezionati).collect::<Vec<_>>(), [9]);
        // Un tocco sulla pagina chiude la selezione.
        tel.passa(&mut l, 0.5);
        let e = tocca(&mut tel, &mut l, pos2(200.0, 600.0));
        assert!(!l.ha_selezione() && e.link.is_none());
    }

    #[test]
    fn una_pagina_senza_testo_lo_dice_e_fuori_dalle_pagine_niente() {
        let mut l = lettore_da_selezionare();
        let mut tel = Telefono::new();
        // La seconda pagina, bianca: nessuna parola, e l'avviso per chi usa il lettore.
        l.vai_alla_pagina(1);
        tel.aspetta(&mut l, |l| l.pagine_pronte().contains(&1) && !l.in_attesa());
        let mut esiti = pressione_lunga(&mut tel, &mut l, pos2(200.0, 400.0));
        esiti.extend(tel.aspetta(&mut l, |l| !l.ha_selezione()));
        tel.aspetta(&mut l, |l| l.testi.contains_key(&1));
        let esiti: Vec<Esito> = esiti.into_iter().chain([tel.passo(&mut l, vec![]).esito]).collect();
        assert!(esiti.iter().any(|e| e.senza_testo), "nessun avviso per la pagina senza testo");
        assert!(l.selezione().is_none());
        // Sul tavolo, a sinistra della pagina: niente.
        let esiti = pressione_lunga(&mut tel, &mut l, pos2(6.0, 400.0));
        assert!(!l.ha_selezione() && !esiti.iter().any(|e| e.senza_testo));
    }

    #[test]
    fn la_posizione_si_ricorda_e_si_rimette_con_lo_zoom() {
        let mut l = lettore(8);
        let mut tel = Telefono::new();
        assert_eq!(l.posizione(), None);
        l.imposta_zoom(1.5);
        l.vai_al_punto(5, Some(300.0));
        tel.passa(&mut l, 0.1);
        let (pagina, y) = l.posizione().unwrap();
        assert_eq!(pagina, 5);
        assert!((y - 300.0).abs() < 0.5, "{y}");
        // Un lettore nuovo dello stesso PDF torna lì, allo stesso zoom.
        let mut di_nuovo = lettore(8);
        let mut tel = Telefono::new();
        di_nuovo.imposta_zoom(l.zoom());
        di_nuovo.vai_al_punto(pagina, Some(y));
        tel.passa(&mut di_nuovo, 0.1);
        assert_eq!(di_nuovo.zoom(), 1.5);
        assert_eq!(di_nuovo.scorrimento(), l.scorrimento());
        assert_eq!(di_nuovo.pagina_corrente(), 5);
        // Uno zoom fuori dai bordi si tiene dentro.
        di_nuovo.imposta_zoom(40.0);
        assert_eq!(di_nuovo.zoom(), ZOOM_MASSIMO);
        di_nuovo.imposta_zoom(f32::NAN);
        assert_eq!(di_nuovo.zoom(), ZOOM_MASSIMO);
    }

    /// Un frame su uno schermo di misura `schermo` (il telefono girato), col lettore e, se c'è,
    /// la griglia delle miniature in un'area che scorre larga `griglia.width()` dal suo angolo,
    /// alta al massimo `griglia.height()` e scorsa in fondo.
    fn passo_su(tel: &mut Telefono, l: &mut Lettore, schermo: Vec2, griglia: Option<Rect>) {
        let mut ri = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, schermo)),
            max_texture_side: Some(4096),
            time: Some(tel.t),
            predicted_dt: 1.0 / 60.0,
            ..Default::default()
        };
        ri.viewports.entry(egui::ViewportId::ROOT).or_default().native_pixels_per_point = Some(2.625);
        tel.t += 1.0 / 60.0;
        let mut out = tel.ctx.run_ui(ri, |ui| {
            l.mostra(ui);
            if let Some(g) = griglia {
                egui::Area::new(Id::new("griglia")).fixed_pos(g.min).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
                    egui::ScrollArea::vertical().max_height(g.height()).vertical_scroll_offset(1000.0).show(ui, |ui| {
                        ui.set_width(g.width());
                        l.mostra_miniature(ui);
                    });
                });
            }
        });
        out.textures_delta.clear();
    }

    /// Frame con l'attesa vera (il pool disegna sui suoi thread) finché il lettore non aspetta
    /// più niente, al massimo 20 s; poi frame fermi per mezzo secondo vero. Restituisce i conti
    /// di prima e di dopo i frame fermi.
    fn fermo_dopo_l_attesa(tel: &mut Telefono, l: &mut Lettore, mut frame: impl FnMut(&mut Telefono, &mut Lettore)) -> (Conti, Conti) {
        let fine = Instant::now() + Duration::from_secs(20);
        frame(tel, l);
        while l.in_attesa() {
            assert!(Instant::now() < fine, "il pool lavora ancora dopo 20 s: {}", l.riga_misure());
            std::thread::sleep(Duration::from_millis(5));
            frame(tel, l);
        }
        let prima = l.conti();
        let fine = Instant::now() + Duration::from_millis(500);
        while Instant::now() < fine {
            frame(tel, l);
            std::thread::sleep(Duration::from_millis(16));
        }
        (prima, l.conti())
    }

    /// A zoom alto, con la vista a cavallo di due pagine, le pagine intere volute superano il
    /// tetto: quelle che il lettore chiede non si liberano, se no il pool le ridisegna per sempre
    /// (revisione della 0.3.0).
    #[test]
    fn a_zoom_alto_fra_due_pagine_le_intere_non_si_ridisegnano_all_infinito() {
        let mut l = lettore(12);
        let mut tel = Telefono::new();
        l.imposta_zoom(2.5);
        let imp = Impaginazione::new(l.misure.clone(), 412.0, 2.5);
        l.scorri_a(vec2(0.0, imp.rettangolo(5).top() - 400.0));
        tel.passa(&mut l, 0.5);
        let (prima, dopo) = fermo_dopo_l_attesa(&mut tel, &mut l, |tel, l| {
            tel.passo(l, vec![]);
        });
        assert_eq!(dopo.pagine, prima.pagine, "a vista ferma il pool ridisegna le pagine intere");
        assert!(!l.in_attesa());
    }

    /// Col telefono girato la griglia delle miniature è larga: quelle volute si tengono e quelle
    /// pronte non si richiedono, così la coda si svuota (revisione della 0.3.0).
    #[test]
    fn le_miniature_col_telefono_girato_non_si_ridisegnano_all_infinito() {
        let mut l = lettore(30);
        let mut tel = Telefono::new();
        let griglia = Rect::from_min_size(pos2(18.0, 170.0), vec2(879.0, 230.0));
        let (prima, dopo) = fermo_dopo_l_attesa(&mut tel, &mut l, |tel, l| passo_su(tel, l, vec2(915.0, 412.0), Some(griglia)));
        assert_eq!(dopo.miniature, prima.miniature, "a griglia ferma il pool ridisegna le miniature");
        assert!(dopo.miniature > 0);
    }

    /// Dieci slide orizzontali (960 × 540 pt), come una lezione esportata da PowerPoint.
    fn lettore_di_slide() -> Lettore {
        let slide = Pagina { larghezza: 960.0, altezza: 540.0, rotazione: 0, contenuto: "0 0 1 rg 50 50 860 440 re f" };
        Lettore::new(&Pdf::apri(pdf(&vec![slide; 10]), None).unwrap().istantanea(), Id::new("slide di prova"))
    }

    /// Analisi del 27/10, lettura I2: girando il telefono resta sullo schermo la slide che si
    /// guardava; in fondo al documento si resta in fondo. Prima si teneva il punto in cima alla
    /// vista, e con le slide si finiva su un'altra.
    #[test]
    fn girando_il_telefono_resta_la_slide_che_si_guardava() {
        let (verticale, orizzontale) = (vec2(412.0, 851.0), vec2(915.0, 380.0));
        let mut l = lettore_di_slide();
        let mut tel = Telefono::new();
        passo_su(&mut tel, &mut l, verticale, None);
        // La quinta slide al centro della vista.
        let imp = l.imp.clone().unwrap();
        l.forza = Some(vec2(0.0, imp.rettangolo(4).center().y - verticale.y / 2.0));
        for _ in 0..3 {
            passo_su(&mut tel, &mut l, verticale, None);
        }
        let al_centro = |l: &Lettore| l.imp.as_ref().unwrap().punto(l.scorrimento.to_pos2() + l.vista / 2.0).unwrap().0;
        assert_eq!(al_centro(&l), 4);
        for _ in 0..10 {
            passo_su(&mut tel, &mut l, orizzontale, None);
        }
        assert_eq!((al_centro(&l), l.pagina_corrente()), (4, 4), "girato, la slide che si guardava non c'è più");
        // In fondo, sull'ultima: girato, si resta in fondo.
        for _ in 0..10 {
            passo_su(&mut tel, &mut l, verticale, None);
        }
        l.vai_al_punto(9, None);
        for _ in 0..5 {
            passo_su(&mut tel, &mut l, verticale, None);
        }
        assert_eq!(l.pagina_corrente(), 9);
        for _ in 0..10 {
            passo_su(&mut tel, &mut l, orizzontale, None);
        }
        assert_eq!(l.pagina_corrente(), 9, "girato in fondo, si è tornati indietro");
    }

    /// Girando il telefono l'impaginazione cambia larghezza: il punto al centro della vista resta al
    /// centro, in orizzontale e tornando in verticale (revisione della 0.3.0; dal 27/10 il punto al
    /// centro e non quello in cima, lettura I2).
    #[test]
    fn girando_il_telefono_si_resta_alla_stessa_pagina() {
        let mut l = lettore(60);
        let mut tel = Telefono::new();
        l.vai_al_punto(30, Some(300.0));
        for _ in 0..10 {
            passo_su(&mut tel, &mut l, vec2(412.0, 851.0), None);
        }
        let al_centro = |l: &Lettore| {
            let (p, pt) = l.imp.as_ref().unwrap().punto(l.scorrimento.to_pos2() + l.vista / 2.0).unwrap();
            (p, pt.y)
        };
        let (prima, y_prima) = al_centro(&l);
        for (schermo, verso) in [(vec2(915.0, 380.0), "in orizzontale"), (vec2(412.0, 851.0), "tornando in verticale")] {
            for _ in 0..10 {
                passo_su(&mut tel, &mut l, schermo, None);
            }
            let (p, y) = al_centro(&l);
            assert!(
                p == prima && (y - y_prima).abs() < 1.0,
                "{verso} al centro c'è la pagina {} a {y} pt, prima la {} a {y_prima}",
                p + 1,
                prima + 1
            );
        }
    }

    /// Ingranditi a destra della pagina: la ripresa ricorda anche il punto in orizzontale, in
    /// frazione della larghezza della pagina, e lo rimette (revisione della 0.3.0).
    #[test]
    fn la_ripresa_ricorda_anche_il_punto_in_orizzontale() {
        let mut l = lettore(4);
        let mut tel = Telefono::new();
        l.imposta_zoom(3.0);
        l.vai_al_punto_xy(2, 0.8, 300.0);
        tel.passa(&mut l, 0.1);
        let (pagina, y) = l.posizione().unwrap();
        let x = l.posizione_x().unwrap();
        assert_eq!(pagina, 2);
        assert!((y - 300.0).abs() < 0.5 && (x - 0.8).abs() < 0.01, "{x} {y}");
        assert!(l.scorrimento().x > 412.0, "la vista non è andata a destra: {:?}", l.scorrimento());
        let mut di_nuovo = lettore(4);
        let mut tel = Telefono::new();
        di_nuovo.imposta_zoom(l.zoom());
        di_nuovo.vai_al_punto_xy(pagina, x, y);
        tel.passa(&mut di_nuovo, 0.1);
        assert!((di_nuovo.scorrimento() - l.scorrimento()).length() < 1.0, "{:?} {:?}", di_nuovo.scorrimento(), l.scorrimento());
        // A zoom 1 la pagina sta tutta: la x non conta e la vista resta a sinistra.
        let mut largo = lettore(4);
        largo.vai_al_punto_xy(1, 0.9, 100.0);
        tel.passa(&mut largo, 0.1);
        assert_eq!(largo.scorrimento().x, 0.0);
        // Una x impossibile vale il mezzo della pagina.
        l.vai_al_punto_xy(2, f32::NAN, 300.0);
        tel.passa(&mut l, 0.1);
        assert!((l.posizione_x().unwrap() - 0.5).abs() < 0.01);
    }

    type Motore = crate::motore::Motore<Result<usize, String>>;

    /// Il PDF `byte` sul thread del motore e il suo lettore, con le pagine sullo schermo disegnate.
    fn col_motore(byte: Vec<u8>, tel: &mut Telefono) -> (Motore, Lettore) {
        let m = Motore::avvia(Pdf::apri(byte, None).unwrap(), Arc::new(|| {})).unwrap();
        let mut l = Lettore::new(m.stato(), Id::new("lettore col motore"));
        tel.aspetta(&mut l, |l| l.pagine_pronte().contains(&1) && !l.in_attesa());
        (m, l)
    }

    /// Manda la modifica, aspetta il motore e passa al lettore le pagine toccate.
    fn modifica(m: &mut Motore, l: &mut Lettore, modifica: crate::Modifica) -> crate::Toccate {
        m.applica(modifica, false);
        let fatti = m.aspetta(Duration::from_secs(20));
        let crate::motore::Evento::Applicata { toccate, .. } = &fatti[0].evento else { panic!("modifica non applicata") };
        l.aggiorna(m.stato(), toccate);
        toccate.clone()
    }

    #[test]
    fn una_pagina_girata_si_ridisegna_larga_e_le_altre_restano() {
        let mut tel = Telefono::new();
        let (mut m, mut l) =
            col_motore(pdf(&[Pagina { larghezza: 595.0, altezza: 842.0, rotazione: 0, contenuto: "0 0 1 rg 50 50 495 742 re f" }; 3]), &mut tel);
        let seconda = l.intere[&1].tex.id();
        let display = l.display.clone();
        modifica(&mut m, &mut l, crate::Modifica::ruota(vec![0], 90));
        // Il pool è nuovo, sui byte nuovi; la pagina girata ha la misura nuova e la sua immagine
        // di prima (stretta) non c'è più; quella della seconda pagina resta.
        assert!(!Arc::ptr_eq(&display, &l.display) && Arc::ptr_eq(&l.display, &m.stato().display));
        assert_eq!(l.misure[0], Misura { larghezza: 842.0, altezza: 595.0 });
        assert!(!l.intere.contains_key(&0));
        assert_eq!(l.intere[&1].tex.id(), seconda);
        tel.aspetta(&mut l, |l| l.pagine_pronte().contains(&0) && !l.in_attesa());
        let im = &l.intere[&0];
        assert!(im.px.larghezza > im.px.altezza, "la pagina girata non è larga: {:?}", im.px);
        // Annullata: di nuovo stretta.
        m.annulla();
        let fatti = m.aspetta(Duration::from_secs(20));
        let crate::motore::Evento::Annullata { toccate } = &fatti[0].evento else { panic!("non annullata") };
        l.aggiorna(m.stato(), toccate);
        tel.aspetta(&mut l, |l| l.pagine_pronte().contains(&0) && !l.in_attesa());
        assert!(l.intere[&0].px.larghezza < l.intere[&0].px.altezza);
    }

    #[test]
    fn una_pagina_cambiata_resta_com_era_finche_arriva_quella_nuova() {
        let mut tel = Telefono::new();
        let (mut m, mut l) = col_motore(pdf(&[Pagina::bianca(595.0, 842.0); 3]), &mut tel);
        let (prima, seconda) = (l.intere[&0].tex.id(), l.intere[&1].tex.id());
        let shape = pdfcraft_engine::Shape::Note { at: [100.0, 700.0], icon: pdfcraft_engine::NoteIcon::Comment };
        let nota = pdfcraft_engine::Edit::AddAnnotation(pdfcraft_engine::NewAnnotation {
            page: 1,
            style: pdfcraft_engine::Style::default_for(&shape),
            shape,
            contents: String::new(),
            author: String::new(),
        });
        let toccate = modifica(&mut m, &mut l, crate::Modifica::del_motore(nota));
        assert_eq!(toccate, crate::Toccate::Alcune(vec![1]));
        // Finché la nuova non arriva si vede quella di prima: niente pagina bianca per un attimo.
        assert_eq!(l.intere[&1].tex.id(), seconda);
        assert!(l.da_rifare.contains(&1));
        tel.aspetta(&mut l, |l| l.da_rifare.is_empty() && !l.in_attesa());
        assert_ne!(l.intere[&1].tex.id(), seconda, "la pagina con la nota non si è ridisegnata");
        assert_eq!(l.intere[&0].tex.id(), prima, "la pagina senza modifiche si è ridisegnata");
    }

    #[test]
    fn dopo_una_modifica_la_ricerca_rilegge_le_pagine_toccate_e_la_selezione_si_chiude() {
        let mut tel = Telefono::new();
        let contenuto = "BT /F1 14 Tf 60 700 Td (Ciao bel mondo) Tj ET";
        let (mut m, mut l) = col_motore(pdf(&[Pagina { larghezza: 595.0, altezza: 842.0, rotazione: 0, contenuto }; 2]), &mut tel);
        l.cerca("mondo");
        tel.aspetta(&mut l, |l| l.conteggio().is_some_and(|c| c.finita) && !l.in_attesa());
        assert_eq!(l.conteggio().unwrap().totale, 2);
        l.selezione = Some(Selezione { pagina: 0, inizio: 0, fine: 4 });
        let letti = l.conti().testi;
        modifica(&mut m, &mut l, crate::Modifica::ruota(vec![0], 90));
        assert!(!l.ha_selezione());
        // La pagina girata si rilegge (il testo ha le posizioni nuove), l'altra no.
        assert!(!l.conteggio().unwrap().finita);
        tel.aspetta(&mut l, |l| l.conteggio().is_some_and(|c| c.finita) && !l.in_attesa());
        assert_eq!(l.conteggio().unwrap().totale, 2);
        assert_eq!(l.conti().testi, letti + 1);
    }

    #[test]
    fn l_indicatore_compare_all_apertura_e_scorrendo_poi_sparisce() {
        let mut l = lettore(5);
        let mut tel = Telefono::new();
        let f = tel.passo(&mut l, vec![]);
        assert!(f.testi().contains(&"1 / 5".to_string()), "{:?}", f.testi());
        let f = tel.passa(&mut l, DURATA_INDICATORE + f64::from(SFUMATURA_INDICATORE) + 0.1);
        assert!(!f.testi().iter().any(|t| t.contains(" / ")), "{:?}", f.testi());
        // Scorrendo fino alla quarta pagina ricompare, col numero nuovo.
        let cima = Impaginazione::new(l.misure.clone(), 412.0, 1.0).rettangolo(3).top();
        l.scorri_a(vec2(0.0, cima));
        tel.passo(&mut l, vec![]);
        let f = tel.passo(&mut l, vec![]);
        assert!(f.testi().contains(&"4 / 5".to_string()), "{:?}", f.testi());
    }
}
