//! Le due viste del documento, disegnate a fasce: Scrittura (una pagina unica larga quanto lo
//! schermo, il testo a capo sulla sua larghezza) e Pagina (le pagine vere una sotto l'altra, in
//! sola lettura).

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tascabile_platform::diagnosi;
use tascabile_touch::{
    BarraContesto, Bordo, Maniglia, ManigliaCursore, ManigliaMisura, Maniglie, Trascinamento, TrascinamentoCursore, TrascinamentoMisura, icone,
    lente_con, tema,
};
use wordcraft_engine::Session;
use wordcraft_engine::cmd::table_mouse;
use wordcraft_engine::doc::Pos;
use wordcraft_engine::layout::display::{Draw, page_display};
use wordcraft_engine::layout::{DocLayout, Page, Placed, ViewMode};
use wordcraft_engine::render::{RenderOptions, Rendered, render_items};

use crate::documento::DP_PER_PT;

/// Altezza di una fascia in pixel fisici (come le fasce di WordCraft desktop).
const FASCIA_PX: f32 = 1024.0;
/// Fasce nuove disegnate al massimo in un frame: le altre al frame dopo.
const NUOVE_PER_FRAME: usize = 2;
/// Il cursore, le gocce delle maniglie e la sottolineatura della parola in composizione.
const BLU: egui::Color32 = tema::BLU;
/// Con la maniglia a meno di tanto (dp) dal bordo della vista, la pagina scorre da sola.
const BORDO_SCORRIMENTO: f32 = 48.0;
/// Di quanto scorre in un frame, in dp.
const PASSO_SCORRIMENTO: f32 = 8.0;
/// Il colore tenue della selezione, sotto il testo (`.sel` nei bozzetti), con gli angoli appena tondi.
pub(crate) const SELEZIONE: egui::Color32 = tema::SELEZIONE;
const RAGGIO_SELEZIONE: f32 = 2.0;
/// Quanto si allarga attorno al cursore il rettangolo da tenere a schermo.
const ATTORNO_AL_CURSORE: f32 = 32.0;
/// Lo zoom del pizzico, fra questi due valori.
pub const ZOOM_MINIMO: f32 = 0.7;
pub const ZOOM_MASSIMO: f32 = 2.5;
/// Lo spazio fra una pagina e l'altra nella vista Pagina, in dp (`.pagina` del bozzetto 7: 20).
pub const SPAZIO_PAGINE: f32 = 20.0;
/// Le pagine dai lati della vista, in dp, con lo zoom a 1: sulla carta come i fogli del bozzetto
/// 7 (`.pagina { left: 16px }`).
pub const MARGINE_PAGINE: f32 = 16.0;
/// La prima pagina dalla cima della vista, e l'ultima dal fondo, in dp.
pub const CIMA_PAGINE: f32 = 14.0;
/// Quanto resta la goccia del cursore senza che il dito la tocchi, in secondi: come in Android
/// (`DELAY_BEFORE_HANDLE_FADES_OUT` dell'Editor di TextView, 4 s).
const DURATA_GOCCIA: f64 = 4.0;
/// Il tempo fra un tocco e il successivo di un doppio (o triplo) tocco, in secondi, da quando il
/// dito si alza a quando si alza di nuovo: i 300 ms di Android fra l'alzata e il tocco dopo, più
/// il tocco stesso.
const TEMPO_TOCCHI: f64 = 0.45;
/// Quanto può stare lontano un tocco dal precedente per contare nel doppio tocco, in dp: un dito.
const DISTANZA_TOCCHI: f32 = 48.0;

/// Quale vista mostra il documento.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Modo {
    /// Una pagina unica larga quanto lo schermo, per scrivere.
    #[default]
    Scrittura,
    /// Le pagine vere (`ViewMode::Print`) una sotto l'altra, larghe quanto lo schermo, in sola lettura.
    Pagina,
}

/// Il colore della pagina del documento (`design.pageColor`), bianco se non ne ha uno.
pub fn colore_carta(s: &Session) -> egui::Color32 {
    s.doc.settings.page_color.map_or(egui::Color32::WHITE, |c| egui::Color32::from_rgb(c.0, c.1, c.2))
}

/// Le pagine vere del documento, come le stampa il motore.
pub fn layout_pagine(s: &mut Session) -> Arc<DocLayout> {
    s.view.mode = ViewMode::Print;
    s.view.read_mode = false;
    s.layout()
}

/// Pagina unica larga `larghezza_dp`: il testo va a capo sulla larghezza dello schermo.
pub fn layout_scrittura(s: &mut Session, larghezza_dp: f32, zoom: f32) -> Arc<DocLayout> {
    s.view.mode = ViewMode::Web;
    s.view.read_mode = false;
    s.view.web_width = (larghezza_dp / (DP_PER_PT * zoom)).clamp(144.0, 20_000.0);
    s.layout()
}

/// Fascia `indice` (alta `fascia_pt` punti) della pagina unica, a `px_per_pt` pixel per punto.
/// Comoda per le prove: rifà ogni volta la lista di disegno della pagina intera.
pub fn rendi_fascia(s: &Session, l: &DocLayout, indice: usize, fascia_pt: f32, px_per_pt: f32) -> Option<Rendered> {
    let pagina = l.pages.first()?;
    let lista = page_display(&s.doc, pagina, &RenderOptions::default().display);
    rendi_fascia_da(s, pagina, &lista, indice, fascia_pt, px_per_pt)
}

/// Fascia `indice` dalla lista di disegno della pagina già pronta (`page_display`), con un solo
/// thread. Sfondo trasparente: niente carta, filigrana o immagine di sfondo della pagina, il
/// colore della pagina lo dipinge la vista.
pub fn rendi_fascia_da(s: &Session, pagina: &Page, lista: &[Draw], indice: usize, fascia_pt: f32, px_per_pt: f32) -> Option<Rendered> {
    rendi_fascia_larga(s, pagina, lista, indice, fascia_pt, px_per_pt, pagina.w)
}

/// Come `rendi_fascia_da`, larga `larghezza_pt` invece della pagina: una tabella più larga dello
/// schermo esce dalla pagina della vista Scrittura (`larghezza_del_contenuto`).
pub fn rendi_fascia_larga(
    s: &Session,
    pagina: &Page,
    lista: &[Draw],
    indice: usize,
    fascia_pt: f32,
    px_per_pt: f32,
    larghezza_pt: f32,
) -> Option<Rendered> {
    let y = indice as f32 * fascia_pt;
    if y >= pagina.h {
        return None;
    }
    let h = fascia_pt.min(pagina.h - y);
    let nella_fascia = nella_fascia(lista, y, y + h);
    Some(render_items(&s.doc, &nella_fascia, 0.0, y, larghezza_pt, h, px_per_pt, &RenderOptions::default()))
}

/// Le voci della lista di disegno che toccano la fascia fra `y0` e `y1` (punti della pagina),
/// con gli stessi margini che il disegnatore usa per il testo: 40 punti sopra e sotto, e per il
/// testo la linea di base fino a 240 punti sotto. Il disegnatore salta le lettere fuori dalla
/// fascia, ma non le linee: in un documento lungo le sottolineature a onda dell'ortografia sono
/// decine di migliaia, e ogni fascia le ridisegnava tutte (7/11: un secondo a fascia sul PC con
/// 6.000 paragrafi, 5 ms con la lista ridotta). Le voci di altro genere, poche, restano tutte.
fn nella_fascia(lista: &[Draw], y0: f32, y1: f32) -> Vec<Draw> {
    let (sopra, sotto) = (y0 - 40.0, y1 + 40.0);
    lista
        .iter()
        .filter(|d| match d {
            Draw::Glyphs { glyphs, .. } => glyphs.iter().any(|g| g.2.is_nan() || (g.2 >= sopra && g.2 <= sotto + 200.0)),
            Draw::Fill { rect, .. } => rect.bottom() >= sopra && rect.y <= sotto,
            Draw::Line { y0: a, y1: b, width, .. } => a.max(*b) + width >= sopra && a.min(*b) - width <= sotto,
            _ => true,
        })
        .cloned()
        .collect()
}

/// Quanto è larga, in punti, la pagina unica della vista Scrittura con quello che ci sta: la
/// pagina, o più, se una tabella o un'immagine esce a destra (una tabella di un file con la sua
/// larghezza, che il motore tiene anche oltre i margini, come Word; un'immagine larga quanto il
/// testo della pagina), col margine destro della vista dopo di lei. La pagina allora scorre di
/// lato col dito; il testo resta a capo sullo schermo.
pub fn larghezza_del_contenuto(pagina: &Page) -> f32 {
    let destra = pagina
        .items
        .iter()
        .filter_map(|it| match it {
            Placed::Cell { rect, .. } | Placed::Image { rect, .. } => Some(rect.x + rect.w),
            _ => None,
        })
        .fold(0.0f32, f32::max);
    if destra > pagina.w { destra + MARGINE_SCRITTURA } else { pagina.w }
}

/// Il margine della pagina unica di Scrittura ai lati, in punti (quello del motore nella vista
/// Web, `web_sect.margin_right`).
const MARGINE_SCRITTURA: f32 = 18.0;

/// La cella del cursore nella pagina, in punti; None fuori da una tabella.
fn cella_del_cursore(s: &Session, pagina: &Page) -> Option<egui::Rect> {
    let f = &s.sel.focus;
    let (tp, r, c) = f.path.cell()?;
    pagina.items.iter().find_map(|it| match it {
        Placed::Cell { rect, table, row, cell, story, covered: false, .. } if *story == f.story && *table == tp && *row == r && *cell == c => {
            Some(egui::Rect::from_min_size(egui::pos2(rect.x, rect.y), egui::vec2(rect.w, rect.h)))
        }
        _ => None,
    })
}

/// Fasce da `prima` a `ultima` nell'ordine in cui disegnarle: prima quelle della riga del cursore
/// (da `cima` a `fondo`, in punti), così la lettera appena battuta non aspetta il frame dopo.
fn ordine_fasce(prima: usize, ultima: usize, riga: Option<(f32, f32)>, fascia_pt: f32) -> Vec<usize> {
    let mut ordine = Vec::with_capacity(ultima.saturating_sub(prima) + 1);
    if let Some((cima, fondo)) = riga {
        for y in [cima, fondo] {
            let i = (y / fascia_pt).floor().max(0.0) as usize;
            if (prima..=ultima).contains(&i) && !ordine.contains(&i) {
                ordine.push(i);
            }
        }
    }
    for i in prima..=ultima {
        if !ordine.contains(&i) {
            ordine.push(i);
        }
    }
    ordine
}

/// Tempi misurati, per la riga di stato della Fase 0.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Misure {
    pub layout_ms: f32,
    /// Lista di disegno della pagina intera, rifatta a ogni modifica: per i documenti lunghi è
    /// il costo principale dopo una lettera.
    pub lista_ms: f32,
    pub fascia_ms: f32,
    /// Dall'apertura del documento alla prima fascia disegnata.
    pub prima_fascia_ms: Option<f32>,
    /// Quante liste di disegno (`page_display`) la vista ha fatto finora, in tutte e due le viste.
    pub liste: u64,
}

/// Tocco sul testo, in punti della pagina.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tocco {
    pub x_pt: f32,
    pub y_pt: f32,
}

/// I bottoni della barra sopra la selezione.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Azione {
    Taglia,
    Copia,
    Incolla,
    SelezionaTutto,
    /// Un commento sulla selezione.
    Commenta,
}

impl Azione {
    /// L'etichetta sulla barra, come nel bozzetto 5.
    pub fn etichetta(self) -> &'static str {
        match self {
            Azione::Taglia => "Taglia",
            Azione::Copia => "Copia",
            Azione::Incolla => "Incolla",
            Azione::SelezionaTutto => "Tutto",
            Azione::Commenta => "Commenta",
        }
    }

    pub fn icona(self) -> &'static str {
        match self {
            Azione::Taglia => icone::CONTENT_CUT,
            Azione::Copia => icone::CONTENT_COPY,
            Azione::Incolla => icone::CONTENT_PASTE,
            Azione::SelezionaTutto => icone::SELECT_ALL,
            Azione::Commenta => icone::ADD_COMMENT,
        }
    }
}

/// La scelta nella barra della parola sottolineata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceltaOrtografia {
    /// Il suggerimento con questo numero al posto della parola.
    Correggi(usize),
    /// La parola nel dizionario personale.
    Aggiungi,
    /// La parola non si sottolinea più, finché l'app è aperta.
    Ignora,
}

/// Le etichette delle due azioni della barra dell'ortografia.
pub const AGGIUNGI_AL_DIZIONARIO: &str = "Aggiungi al dizionario";
pub const IGNORA: &str = "Ignora";

/// Che cosa ha fatto il dito sulla vista in questo frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Gesto {
    /// Un tocco sul testo: il cursore va lì e la selezione si chiude.
    Tocco(Tocco),
    /// Una pressione lunga: si seleziona la parola.
    PressioneLunga(Tocco),
    /// Una maniglia trascinata: quel bordo della selezione va al punto (`presa` nel primo frame).
    Maniglia { maniglia: Maniglia, punto: Tocco, presa: bool },
    /// Il dito ha lasciato la maniglia.
    ManigliaLasciata,
    /// Due tocchi vicini e svelti: si seleziona la parola.
    DoppioTocco(Tocco),
    /// Tre tocchi vicini e svelti: si seleziona il paragrafo.
    TriploTocco(Tocco),
    /// La goccia del cursore trascinata: il cursore va al punto (`presa` nel primo frame).
    Cursore { punto: Tocco, presa: bool },
    /// Il dito ha lasciato la goccia del cursore.
    CursoreLasciato,
    /// Un bottone della barra della selezione.
    Azione(Azione),
    /// Un tocco su una pagina della vista Pagina, in punti di quella pagina.
    ToccoPagina { pagina: usize, punto: Tocco },
    /// Il dito trascinato dalla cella del cursore: la selezione va fin lì, a celle intere fra due
    /// celle della tabella.
    Celle(Tocco),
    /// Il dito si è alzato dalle celle.
    CelleLasciate,
    /// La maniglia dell'immagine scelta lasciata: la larghezza nuova, in punti.
    MisuraImmagine(f32),
    /// Una scelta della barra della parola sottolineata.
    Ortografia(SceltaOrtografia),
}

impl Gesto {
    /// Il tocco semplice, se il gesto è quello.
    pub fn tocco(self) -> Option<Tocco> {
        match self {
            Gesto::Tocco(t) => Some(t),
            _ => None,
        }
    }
}

fn ms(da: Instant) -> f32 {
    da.elapsed().as_secs_f32() * 1000.0
}

/// Numero della prossima vista: ogni vista ha la sua ScrollArea, e un documento nuovo non
/// parte dallo scorrimento di quello di prima.
static PROSSIMA_VISTA: AtomicU64 = AtomicU64::new(0);

/// La chiave di una fascia: il numero del layout e la scala (`px_per_pt` in bit).
type ChiaveFascia = (u64, u32);

pub struct Vista {
    /// Lo zoom del documento (resta per documento, perché la vista è dell'Editor): si cambia
    /// solo col pizzico, così non si separa dallo zoom vivo.
    zoom: f32,
    pub misure: Misure,
    id: u64,
    /// L'ultimo layout visto, tenuto con il suo `Arc`, e il suo numero (vedi `numero_del_layout`).
    layout: Option<(Arc<DocLayout>, u64)>,
    fasce: HashMap<usize, (ChiaveFascia, egui::TextureHandle)>,
    /// Lista di disegno della pagina con il numero del layout per cui è stata fatta: si rifà
    /// solo quando il layout cambia, non per ogni fascia né per la scala.
    lista: Option<(u64, Arc<Vec<Draw>>)>,
    /// I rettangoli della selezione, in punti della pagina, con la chiave del layout e della
    /// selezione per cui sono stati fatti: con «Seleziona tutto» su un documento lungo non si
    /// rifanno a ogni frame.
    rettangoli_selezione: Option<(u64, Arc<Vec<egui::Rect>>)>,
    scorri_al_cursore: bool,
    /// Altezza della vista al frame prima: se cala (si apre la tastiera) il cursore torna a schermo.
    ultima_altezza: f32,
    /// Il cursore era a schermo prima che la vista cominciasse ad accorciarsi: solo allora la
    /// vista lo segue mentre si accorcia.
    segui_cursore: bool,
    /// Ultima volta (tempo di egui) che la vista ha inseguito il cursore mentre si accorciava:
    /// durante l'animazione della tastiera un frame senza altezza nuova non deve far perdere
    /// la presa sul cursore.
    seguito_alle: f64,
    maniglie: Maniglie,
    /// Dove sta la lente, sullo schermo, mentre si trascina una maniglia.
    punto_lente: Option<egui::Pos2>,
    /// La goccia sotto il cursore, da trascinare.
    maniglia_cursore: ManigliaCursore,
    /// La goccia del cursore si vede: dove sta il cursore e da quando (tempo di egui) nessuno la
    /// tocca. Se ne va quando il cursore si sposta da solo (si scrive), con una selezione e dopo
    /// `DURATA_GOCCIA`.
    goccia: Option<(Pos, f64)>,
    /// Un tocco ha appena messo il cursore: al frame dopo la goccia si mostra.
    goccia_chiesta: bool,
    /// L'ultimo tocco sul testo: quando (tempo di egui), dove (in punti della pagina) e quanti
    /// tocchi di fila ha fatto (fino a 3), per il doppio e il triplo tocco.
    ultimo_tocco: Option<(f64, Tocco, u8)>,
    /// Dopo una pressione lunga fuori da una parola (uno spazio, una riga vuota) resta il
    /// cursore: la barra si mostra lo stesso, con Incolla e Seleziona tutto, finché il cursore
    /// resta lì.
    barra_al_cursore: Option<Pos>,
    /// Un tocco su una parola sottolineata: dove ha messo il cursore e la parola. Sopra, la
    /// barra coi suggerimenti, finché il cursore resta lì.
    ortografia: Option<(Pos, crate::ortografia::Sbagliata)>,
    pub modo: Modo,
    /// La barra Taglia, Copia… non si mostra: la tastiera è di un campo (Trova), e la
    /// selezione è il risultato trovato, che la barra coprirebbe.
    pub senza_barra: bool,
    /// Quanti dp in fondo alla vista copre quello che l'app disegna sopra (la barra di formato):
    /// il cursore e la barra della selezione restano sopra, e la pagina ha quello spazio in più
    /// in fondo, perché anche l'ultima riga possa salire oltre la barra.
    pub coperto_sotto: f32,
    /// La vista dipinge il colore della pagina sotto il testo; falso se lo dipinge l'app (la
    /// pagina come foglio con gli angoli tondi, `colore_carta`).
    pub dipingi_carta: bool,
    /// Lo zoom mentre le dita pizzicano: le fasce già pronte si scalano, e solo al rilascio
    /// diventa `zoom` e il documento si reimpagina.
    zoom_vivo: f32,
    /// Le fasce della vista Pagina, per (pagina, fascia), con la loro chiave.
    fasce_pagine: HashMap<(usize, usize), (ChiaveFascia, egui::TextureHandle)>,
    /// Le liste di disegno della vista Pagina, una per pagina, con il numero del layout.
    liste_pagine: HashMap<usize, (u64, Arc<Vec<Draw>>)>,
    /// Lo zoom dell'altra vista: in Pagina 1,0 è la pagina larga quanto lo schermo, in Scrittura
    /// il testo a 96/72 dp per punto, quindi ognuna tiene il suo.
    zoom_dell_altra: f32,
    /// Due dita sono state giù in questa pressione: finché non si alzano tutte non c'è né tocco
    /// né pressione lunga (egui li ricava dal primo dito, che in un pizzico resta spesso fermo).
    dita_del_pizzico: bool,
    /// Il centro delle dita al frame prima, sullo schermo.
    centro_pizzico: Option<egui::Pos2>,
    /// Lo scorrimento della vista al frame prima.
    scorrimento: egui::Vec2,
    /// L'angolo in alto a sinistra della vista sullo schermo, al frame prima.
    origine: egui::Pos2,
    /// In Scrittura, durante un pizzico: la posizione del documento fra le dita e la y sullo
    /// schermo della cima della sua riga, da rimettere lì quando il layout si rifà al rilascio.
    ancora: Option<(Pos, f32)>,
    /// La cella del cursore sullo schermo, dove il dito trascinato sceglie le celle: resta quella
    /// della presa finché il dito non si alza.
    zona_celle: Option<egui::Rect>,
    /// Il dito sta scegliendo le celle.
    celle_prese: bool,
    /// La larghezza del contenuto (`larghezza_del_contenuto`) col numero del layout.
    contenuto: Option<(u64, f32)>,
    /// La maniglia della misura dell'immagine scelta.
    misura: ManigliaMisura,
    /// La larghezza del testo della vista Scrittura con lo zoom a 1, in punti, all'ultimo frame.
    testo_pt: f32,
}

impl Default for Vista {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            misure: Misure::default(),
            id: PROSSIMA_VISTA.fetch_add(1, Ordering::Relaxed),
            layout: None,
            fasce: HashMap::new(),
            lista: None,
            rettangoli_selezione: None,
            scorri_al_cursore: false,
            ultima_altezza: 0.0,
            segui_cursore: false,
            seguito_alle: f64::NEG_INFINITY,
            maniglie: Maniglie::default(),
            punto_lente: None,
            maniglia_cursore: ManigliaCursore::default(),
            goccia: None,
            goccia_chiesta: false,
            ultimo_tocco: None,
            barra_al_cursore: None,
            ortografia: None,
            modo: Modo::Scrittura,
            senza_barra: false,
            coperto_sotto: 0.0,
            dipingi_carta: true,
            zoom_vivo: 1.0,
            fasce_pagine: HashMap::new(),
            liste_pagine: HashMap::new(),
            zoom_dell_altra: 1.0,
            dita_del_pizzico: false,
            centro_pizzico: None,
            scorrimento: egui::Vec2::ZERO,
            origine: egui::Pos2::ZERO,
            ancora: None,
            zona_celle: None,
            celle_prese: false,
            contenuto: None,
            misura: ManigliaMisura::default(),
            testo_pt: 0.0,
        }
    }
}

impl Vista {
    /// Il pizzico: `fattore` è quanto si sono allargate le dita in questo frame, `in_corso` se le
    /// dita sono ancora giù. Durante il gesto cambia solo lo zoom vivo (le fasce si scalano); al
    /// rilascio diventa lo zoom del documento e il layout si rifà alla larghezza nuova.
    pub fn pizzica(&mut self, fattore: f32, in_corso: bool) {
        if fattore.is_finite() && fattore > 0.0 && fattore != 1.0 {
            self.zoom_vivo = (self.zoom_vivo * fattore).clamp(ZOOM_MINIMO, ZOOM_MASSIMO);
        }
        if !in_corso && self.zoom_vivo != self.zoom {
            self.zoom = self.zoom_vivo;
            diagnosi(format!("zoom: {:.2}", self.zoom));
        }
    }

    /// Lo zoom del documento.
    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    /// Lo zoom che si vede adesso (durante un pizzico è diverso da `zoom`).
    pub fn zoom_vivo(&self) -> f32 {
        self.zoom_vivo
    }

    /// Il pizzico di questo frame, dagli eventi di egui (due dita, o Ctrl e la rotella sul PC).
    /// Restituisce lo scorrimento che tiene fermo fra le dita il punto del documento che c'era,
    /// anche se le dita si spostano: le fasce si scalano attorno al centro delle dita, non
    /// dall'angolo in alto a sinistra.
    fn pizzico(&mut self, ui: &egui::Ui) -> Option<egui::Vec2> {
        let (fattore, dita) = ui.input(|i| (i.zoom_delta(), i.multi_touch()));
        if dita.is_some() {
            self.dita_del_pizzico = true;
        }
        let prima = self.zoom_vivo;
        let centro_prima = self.centro_pizzico;
        self.centro_pizzico = dita.map(|d| d.center_pos);
        self.pizzica(fattore, dita.is_some());
        let centro = self.centro_pizzico.or(centro_prima)?;
        let da = centro_prima.unwrap_or(centro);
        if self.zoom_vivo == prima && da == centro {
            return None;
        }
        let k = self.zoom_vivo / prima;
        let nuovo = (self.scorrimento + (da - self.origine)) * k - (centro - self.origine);
        Some(nuovo.max(egui::Vec2::ZERO))
    }

    /// A fine frame: con tutte le dita alzate il pizzico è finito.
    fn fine_pizzico(&mut self, ui: &egui::Ui) {
        if self.dita_del_pizzico && !ui.input(|i| i.any_touches() || i.pointer.any_down()) {
            self.dita_del_pizzico = false;
            self.centro_pizzico = None;
        }
    }

    /// Il numero del layout `l`, la chiave di liste, fasce e selezione: resta quello finché il
    /// motore dà lo stesso `Arc` e cresce a ogni layout nuovo. Il motore rifà il layout anche
    /// senza una revisione nuova (la parola in scrittura è nella chiave della sua cache, e cambia
    /// quando il cursore la lascia), quindi la revisione non basta. Si tiene l'`Arc` e non il suo
    /// indirizzo come numero, che un layout liberato può lasciare al successivo; le fasce tengono
    /// solo il numero, così non tengono vivi i layout vecchi.
    fn numero_del_layout(&mut self, l: &Arc<DocLayout>) -> u64 {
        match &self.layout {
            Some((tenuto, n)) if Arc::ptr_eq(tenuto, l) => *n,
            vecchio => {
                let n = vecchio.as_ref().map_or(0, |(_, n)| n + 1);
                self.layout = Some((Arc::clone(l), n));
                n
            }
        }
    }

    /// Passa alla vista `modo`.
    pub fn cambia_modo(&mut self, modo: Modo) {
        if self.modo != modo {
            diagnosi(format!("vista: {modo:?}"));
            self.modo = modo;
            std::mem::swap(&mut self.zoom, &mut self.zoom_dell_altra);
            self.zoom_vivo = self.zoom;
            self.ancora = None;
            self.barra_al_cursore = None;
            self.maniglie.lascia();
            self.maniglia_cursore.lascia();
            self.misura.lascia();
            self.goccia = None;
            self.punto_lente = None;
            if modo == Modo::Scrittura {
                self.fasce_pagine.clear();
                self.liste_pagine.clear();
                self.scorri_al_cursore = true;
            } else {
                self.fasce.clear();
                self.lista = None;
            }
        }
    }

    /// Al prossimo frame la pagina scorre fino al cursore.
    pub fn scorri_al_cursore(&mut self) {
        self.scorri_al_cursore = true;
    }

    /// Dopo una pressione lunga rimasta un cursore: la barra si mostra lì.
    pub fn barra_al_cursore(&mut self, pos: Pos) {
        self.barra_al_cursore = Some(pos);
    }

    /// Un tocco ha messo il cursore in `cursore`, in una parola sottolineata: sopra si mostra la
    /// barra coi suggerimenti (con Aggiungi al dizionario e Ignora per l'ortografia).
    pub fn mostra_ortografia(&mut self, cursore: Pos, parola: crate::ortografia::Sbagliata) {
        self.ortografia = Some((cursore, parola));
    }

    /// La parola sottolineata della barra, se c'è.
    pub fn ortografia(&self) -> Option<&crate::ortografia::Sbagliata> {
        self.ortografia.as_ref().map(|(_, p)| p)
    }

    /// Un tocco ha messo il cursore: sotto si mostra la goccia da trascinare.
    pub fn mostra_maniglia_cursore(&mut self) {
        self.goccia_chiesta = true;
    }

    /// Quanti tocchi di fila fa il tocco in `t` adesso (`ora`, tempo di egui): 2 se è vicino e
    /// svelto dopo il tocco di prima, 3 dopo un doppio; dopo un triplo si riparte da 1. La
    /// distanza è nella pagina, non sullo schermo: fra un tocco e l'altro la pagina può scorrere
    /// (la tastiera che si apre).
    fn conta_tocchi(&mut self, ora: f64, t: Tocco, scala: f32) -> u8 {
        let n = match self.ultimo_tocco {
            Some((quando, dove, n))
                if n < 3
                    && ora - quando <= TEMPO_TOCCHI
                    && egui::vec2(t.x_pt - dove.x_pt, t.y_pt - dove.y_pt).length() * scala <= DISTANZA_TOCCHI =>
            {
                n + 1
            }
            _ => 1,
        };
        self.ultimo_tocco = Some((ora, t, n));
        n
    }

    /// Dove sta sullo schermo, in Scrittura, il punto (`x_pt`, `y_pt`) della pagina, con lo
    /// scorrimento e lo zoom dell'ultimo frame (per le prove dell'app).
    pub fn sullo_schermo(&self, x_pt: f32, y_pt: f32) -> egui::Pos2 {
        self.origine - self.scorrimento + egui::vec2(x_pt, y_pt) * DP_PER_PT * self.zoom
    }

    /// La larghezza del testo sullo schermo nella vista Scrittura, con lo zoom a 1, in punti:
    /// un'immagine inserita non è più larga (così si vede tutta, con la sua maniglia). Infinita
    /// prima del primo frame di Scrittura: allora conta solo il testo della pagina.
    pub fn larghezza_del_testo(&self) -> f32 {
        if self.testo_pt > 0.0 { self.testo_pt } else { f32::INFINITY }
    }

    /// La barra si mostra anche senza selezione (dopo una pressione lunga fuori da una parola, o
    /// coi suggerimenti di una parola sottolineata).
    pub fn ha_barra_al_cursore(&self) -> bool {
        self.barra_al_cursore.is_some() || self.ortografia.is_some()
    }

    /// Indietro: la barra al cursore (o quella dei suggerimenti) si chiude.
    pub fn chiudi_barra(&mut self) {
        self.barra_al_cursore = None;
        self.ortografia = None;
    }

    /// Le ultime misure del disegno, per la diagnosi (quando il documento si chiude).
    pub fn riga_misure(&self) -> String {
        let m = &self.misure;
        format!("misure: layout {:.1} ms, lista {:.1} ms, fascia {:.0} ms, {} liste", m.layout_ms, m.lista_ms, m.fascia_ms, m.liste)
    }

    /// La prima fascia del documento è a schermo: quanto ci è voluto, nella diagnosi.
    fn prima_fascia(&mut self, aperto_alle: Instant) {
        if self.misure.prima_fascia_ms.is_none() {
            let ms_apertura = ms(aperto_alle);
            self.misure.prima_fascia_ms = Some(ms_apertura);
            let m = &self.misure;
            diagnosi(format!(
                "pagina: prima fascia dopo {ms_apertura:.0} ms dall'apertura (layout {:.1} ms, lista {:.1} ms, fascia {:.0} ms)",
                m.layout_ms, m.lista_ms, m.fascia_ms
            ));
        }
    }

    /// Un tocco ha chiesto la goccia del cursore (per le prove dell'editor).
    #[cfg(test)]
    pub(crate) fn goccia_chiesta(&self) -> bool {
        self.goccia_chiesta
    }

    /// La pagina scorrerà fino al cursore al prossimo frame (per le prove dell'editor).
    #[cfg(test)]
    pub(crate) fn scorrimento_chiesto(&self) -> bool {
        self.scorri_al_cursore
    }

    /// Disegna il documento, la selezione con le maniglie e la barra. Restituisce il gesto del
    /// dito, se c'è stato.
    pub fn mostra(&mut self, ui: &mut egui::Ui, s: &mut Session, composizione: Option<&(Pos, Pos)>, aperto_alle: Instant) -> Option<Gesto> {
        let zoom_prima = self.zoom;
        let mut forza = self.pizzico(ui);
        let gesto = if self.modo == Modo::Pagina {
            self.mostra_pagine(ui, s, aperto_alle, forza)
        } else {
            let larghezza = ui.available_width();
            let t = Instant::now();
            let l = layout_scrittura(s, larghezza, self.zoom);
            let layout_ms = ms(t);
            // Al rilascio il layout si rifà a un'altra larghezza e le righe cambiano: il punto fra
            // le dita torna dov'era sullo schermo dalla sua posizione nel documento.
            if self.zoom != zoom_prima
                && let Some((pos, y)) = self.ancora.take()
                && let Some(c) = l.caret_on(&pos, s.page_hint)
            {
                let x = forza.map_or(self.scorrimento.x, |f| f.x);
                forza = Some(egui::vec2(x, (c.top * DP_PER_PT * self.zoom - (y - self.origine.y)).max(0.0)));
            }
            self.mostra_scrittura(ui, s, &l, layout_ms, composizione, aperto_alle, forza)
        };
        // Durante un pizzico, e al rilascio, nessun tocco né pressione lunga.
        let pizzico = self.dita_del_pizzico;
        self.fine_pizzico(ui);
        if pizzico {
            return gesto.filter(|g| !matches!(g, Gesto::Tocco(_) | Gesto::PressioneLunga(_) | Gesto::ToccoPagina { .. }));
        }
        gesto
    }

    #[allow(clippy::too_many_arguments)]
    fn mostra_scrittura(
        &mut self,
        ui: &mut egui::Ui,
        s: &Session,
        l: &Arc<DocLayout>,
        layout_ms: f32,
        composizione: Option<&(Pos, Pos)>,
        aperto_alle: Instant,
        forza: Option<egui::Vec2>,
    ) -> Option<Gesto> {
        let larghezza = ui.available_width();
        let pagina = l.pages.first()?;
        self.testo_pt = (larghezza / DP_PER_PT).clamp(144.0, 20_000.0) - 2.0 * MARGINE_SCRITTURA;
        // Il layout e le fasce sono allo zoom del documento; durante un pizzico si disegnano allo
        // zoom vivo, scalate.
        let scala_layout = DP_PER_PT * self.zoom;
        let scala = DP_PER_PT * self.zoom_vivo;
        let ppp = ui.ctx().pixels_per_point();
        let px_per_pt = scala_layout * ppp;
        let fascia_pt = FASCIA_PX / px_per_pt;
        let n_fasce = (pagina.h / fascia_pt).ceil().max(1.0) as usize;
        // La larghezza del testo e lo zoom sono già nel layout: una larghezza nuova dà un layout
        // nuovo. La scala da sola (i pixel per punto) cambia le fasce, non la lista.
        let n_layout = self.numero_del_layout(l);
        let chiave: ChiaveFascia = (n_layout, px_per_pt.to_bits());
        let contenuto = match self.contenuto {
            Some((n, w)) if n == n_layout => w,
            _ => {
                let w = larghezza_del_contenuto(pagina);
                self.contenuto = Some((n_layout, w));
                w
            }
        };
        // Le fasce larghe quanto il contenuto, fin dove la GPU accetta una texture.
        let lato = ui.input(|i| i.max_texture_side).max(1) as f32;
        let larghezza_fasce = contenuto.min(lato / px_per_pt);
        let lista = match &self.lista {
            Some((n, lista)) if *n == n_layout => Arc::clone(lista),
            _ => {
                // Solo qui il layout è nuovo: con lo stesso layout arriva dalla cache e il suo
                // tempo non misura nulla.
                self.misure.layout_ms = layout_ms;
                let t = Instant::now();
                let lista = Arc::new(page_display(&s.doc, pagina, &RenderOptions::default().display));
                self.misure.lista_ms = ms(t);
                self.misure.liste += 1;
                self.lista = Some((n_layout, Arc::clone(&lista)));
                lista
            }
        };
        let scorri = std::mem::take(&mut self.scorri_al_cursore);
        let mut gesto = None;
        let (sel_a, sel_b) = s.sel.ordered();
        let collassata = sel_a == sel_b;
        // L'immagine scelta, col suo posto nella pagina: al posto della selezione, delle maniglie e
        // della barra, il riquadro con la maniglia della misura.
        let immagine = crate::immagine::immagine_scelta(s).and_then(|(pos, info)| l.object(&pos, s.page_hint).map(|o| (o.rect, info)));
        if immagine.is_none() {
            self.misura.lascia();
        }
        if self.barra_al_cursore.as_ref().is_some_and(|p| !collassata || *p != s.sel.focus) {
            self.barra_al_cursore = None;
        }
        if self.ortografia.as_ref().is_some_and(|(p, _)| !collassata || *p != s.sel.focus) {
            self.ortografia = None;
        }
        // La goccia del cursore: chiesta da un tocco, segue il cursore mentre la si trascina, se ne
        // va col cursore che si sposta da solo, con una selezione e dopo `DURATA_GOCCIA`.
        let ora = ui.input(|i| i.time);
        if std::mem::take(&mut self.goccia_chiesta) && collassata {
            self.goccia = Some((s.sel.focus.clone(), ora));
        }
        if self.maniglia_cursore.trascinata()
            && let Some((pos, dal)) = self.goccia.as_mut()
        {
            *pos = s.sel.focus.clone();
            *dal = ora;
        }
        if self.goccia.as_ref().is_some_and(|(pos, dal)| !collassata || *pos != s.sel.focus || ora - dal > DURATA_GOCCIA) {
            self.goccia = None;
            self.maniglia_cursore.lascia();
        }
        if let Some((_, dal)) = &self.goccia
            && !self.maniglia_cursore.trascinata()
        {
            ui.ctx().request_repaint_after(Duration::from_secs_f64((DURATA_GOCCIA - (ora - dal)).max(0.0) + 0.01));
        }
        let area = egui::ScrollArea::vertical()
            .id_salt(("vista", self.id))
            // La pagina può essere più larga dello schermo: durante un pizzico, e allo zoom alto
            // su uno schermo stretto, dove la larghezza del testo si ferma a 144 punti.
            .hscroll(contenuto * scala > larghezza + 0.5)
            // Col dito si scorre dal primo tocco: con il valore di egui il trascinamento si accende
            // solo dopo aver visto un tocco, e il primo dopo l'avvio non scorre. Durante un
            // pizzico lo scorrimento lo decidono le dita (`pizzico`), non il primo dito da solo.
            .scroll_source(if self.dita_del_pizzico { egui::scroll_area::ScrollSource::NONE } else { egui::scroll_area::ScrollSource::ALL })
            .auto_shrink([false, false]);
        let area = match forza {
            Some(f) => area.scroll_offset(f),
            None => area,
        };
        let cursore = l.caret_on(&s.sel.focus, s.page_hint);
        // Dove sta la vista sullo schermo (la pagina dell'app): la barra della selezione resta dentro.
        let finestra = ui.max_rect();
        area.show_viewport(ui, |ui, viewport| {
            // La tastiera che si apre accorcia la vista qualche frame dopo il tocco: il cursore
            // va riportato a schermo anche allora, a ogni frame finché la vista cala. Solo se
            // prima era a schermo: all'avvio la vista si accorcia anche quando le barre prendono
            // la loro misura, e la pagina non deve saltare a un cursore lontano.
            let accorciata = viewport.height() < self.ultima_altezza - 0.5;
            self.ultima_altezza = viewport.height();
            // Si decide di nuovo se seguire il cursore solo a vista ferma da più di 0,35 s, più
            // dell'animazione di scorrimento di egui (0,3 s): sul telefono l'altezza della tastiera
            // arriva a scatti, e un frame senza altezza nuova a metà animazione è normale.
            if accorciata && self.segui_cursore {
                self.seguito_alle = ora;
            }
            // Il cursore conta come a schermo solo sopra la parte coperta in fondo.
            let coperto = self.coperto_sotto.max(0.0);
            if !accorciata && ora - self.seguito_alle > 0.35 {
                self.segui_cursore =
                    cursore.as_ref().is_some_and(|c| c.top * scala < viewport.max.y - coperto && (c.top + c.height) * scala > viewport.min.y);
            }
            // La pagina riempie almeno lo schermo: un tocco sotto un documento corto arriva al
            // motore con una y oltre la pagina, e il motore sceglie la posizione più vicina. In
            // fondo, lo spazio coperto dalla barra di formato.
            // Alta come la vista e non come lo spazio libero, che può superarla di una frazione
            // di dp (l'angolo sui pixel): egui mostrerebbe la barra di scorrimento senza niente
            // da scorrere.
            let alto = (pagina.h * scala + coperto).max(viewport.height());
            let (rect, risposta) = ui.allocate_exact_size(egui::vec2(larghezza.max(contenuto * scala), alto), egui::Sense::click());
            self.scorrimento = viewport.min.to_vec2();
            self.origine = rect.min + viewport.min.to_vec2();
            // Durante il pizzico, il punto del documento fra le dita e la cima della sua riga.
            if let Some(c) = self.centro_pizzico {
                let (x_pt, y_pt) = ((c.x - rect.min.x) / scala, (c.y - rect.min.y) / scala);
                self.ancora = l
                    .hit(0, x_pt, y_pt, wordcraft_engine::doc::StoryRef::Body)
                    .and_then(|p| l.caret_on(&p, s.page_hint).map(|r| (p, rect.min.y + r.top * scala)));
            }
            let painter = ui.painter();
            // La carta: le fasce arrivano su sfondo trasparente.
            let carta = colore_carta(s);
            if self.dipingi_carta {
                painter.rect_filled(rect, 0.0, carta);
            }
            // La selezione, sotto il testo: le fasce sono trasparenti fuori dalle lettere.
            let in_punti = if collassata || immagine.is_some() {
                Arc::new(Vec::new())
            } else {
                let chiave_sel = {
                    let mut h = DefaultHasher::new();
                    (n_layout, format!("{sel_a:?}{sel_b:?}{:?}", s.cell_range()), s.page_hint).hash(&mut h);
                    h.finish()
                };
                match &self.rettangoli_selezione {
                    Some((k, r)) if *k == chiave_sel => Arc::clone(r),
                    _ => {
                        // Come WordCraft: le celle intere scelte si colorano tutte (anche vuote);
                        // il testo che entra in una tabella, o la attraversa, prende le celle intere
                        // delle righe che tocca.
                        let rettangoli = match s.cell_range() {
                            Some(celle) => table_mouse::cell_rects(l, &s.doc, celle),
                            None => table_mouse::selection_rects(l, &s.doc, &sel_a, &sel_b, s.page_hint),
                        };
                        let r: Arc<Vec<egui::Rect>> = Arc::new(
                            rettangoli
                                .into_iter()
                                .filter(|(pagina, _)| *pagina == 0)
                                .map(|(_, r)| egui::Rect::from_min_size(egui::pos2(r.x, r.y), egui::vec2(r.w, r.h)))
                                .collect(),
                        );
                        self.rettangoli_selezione = Some((chiave_sel, Arc::clone(&r)));
                        r
                    }
                }
            };
            let evidenza: Vec<(egui::Rect, egui::Color32)> = in_punti
                .iter()
                .map(|r| (egui::Rect::from_min_max(rect.min + r.min.to_vec2() * scala, rect.min + r.max.to_vec2() * scala), SELEZIONE))
                .collect();
            for (r, colore) in &evidenza {
                painter.rect_filled(*r, RAGGIO_SELEZIONE, *colore);
            }
            let mut immagini = Vec::new();
            let prima = (viewport.min.y / scala / fascia_pt).floor().max(0.0) as usize;
            // Mai oltre l'ultima fascia della pagina, anche se la vista è più alta del documento.
            let ultima = ((viewport.max.y / scala / fascia_pt).floor().max(0.0) as usize).min(n_fasce - 1);
            let mut nuove = 0;
            for i in ordine_fasce(prima, ultima, cursore.as_ref().map(|c| (c.top, c.top + c.height)), fascia_pt) {
                let pronta = matches!(self.fasce.get(&i), Some((k, _)) if *k == chiave);
                if !pronta && nuove < NUOVE_PER_FRAME {
                    let t = Instant::now();
                    match rendi_fascia_larga(s, pagina, &lista, i, fascia_pt, px_per_pt, larghezza_fasce) {
                        Some(img) => {
                            self.misure.fascia_ms = ms(t);
                            nuove += 1;
                            let ci = egui::ColorImage::from_rgba_premultiplied([img.width as usize, img.height as usize], &img.pixels);
                            match self.fasce.get_mut(&i) {
                                Some((k, tex)) => {
                                    tex.set(ci, egui::TextureOptions::LINEAR);
                                    *k = chiave;
                                }
                                None => {
                                    let tex = ui.ctx().load_texture(format!("fascia{i}"), ci, egui::TextureOptions::LINEAR);
                                    self.fasce.insert(i, (chiave, tex));
                                }
                            }
                            self.prima_fascia(aperto_alle);
                        }
                        // Fascia senza pagina (arrotondamenti al bordo): niente texture vecchia a schermo.
                        None => {
                            self.fasce.remove(&i);
                        }
                    }
                } else if !pronta {
                    ui.ctx().request_repaint(); // la fascia vecchia resta a schermo fino al frame dopo
                }
                if let Some((_, tex)) = self.fasce.get(&i) {
                    // Un pixel della texture su un pixel dello schermo: misura della texture e
                    // angolo sui pixel fisici, se no il testo si sfoca.
                    let sui_pixel = |v: f32| (v * ppp).round() / ppp;
                    let cima = rect.min.y + i as f32 * fascia_pt * scala;
                    let dest =
                        egui::Rect::from_min_size(egui::pos2(sui_pixel(rect.min.x), sui_pixel(cima)), tex.size_vec2() / ppp * (scala / scala_layout));
                    let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                    painter.image(tex.id(), dest, uv, egui::Color32::WHITE);
                    immagini.push((tex.id(), dest));
                }
            }
            if let Some((a, b)) = composizione {
                for (_, r) in l.selection_rects(&s.doc, a, b, s.page_hint) {
                    let y = rect.min.y + (r.y + r.h) * scala;
                    let x0 = rect.min.x + r.x * scala;
                    painter.line_segment([egui::pos2(x0, y), egui::pos2(x0 + r.w * scala, y)], egui::Stroke::new(1.5, BLU));
                }
            }
            let rettangolo_cursore = cursore.as_ref().map(|c| {
                let x = rect.min.x + c.x * scala;
                let y = rect.min.y + c.top * scala;
                egui::Rect::from_min_max(egui::pos2(x - 1.0, y), egui::pos2(x + 1.0, y + c.height * scala))
            });
            if let Some(r) = rettangolo_cursore {
                // Con una selezione il cursore non si disegna: i bordi li segnano le maniglie.
                if collassata {
                    painter.rect_filled(r, 0.0, BLU);
                }
                if scorri || (accorciata && self.segui_cursore) {
                    // Attorno al cursore, e sotto anche la parte coperta.
                    let a = ATTORNO_AL_CURSORE;
                    ui.scroll_to_rect(egui::Rect::from_min_max(r.min - egui::vec2(a, a), r.max + egui::vec2(a, a + coperto)), None);
                }
            }
            let a_tocco = |p: egui::Pos2| Tocco { x_pt: (p.x - rect.min.x) / scala, y_pt: (p.y - rect.min.y) / scala };
            // Le celle col dito: dalla cella del cursore il dito trascinato sceglie le celle (fuori
            // da lei la pagina scorre). La cella sente anche i tocchi, che egui non passerebbe
            // alla pagina sotto: un tocco fermo resta un tocco e una pressione lunga la pressione
            // lunga. Sotto maniglie e goccia, che vengono dopo e restano sopra.
            let zona = if self.celle_prese {
                self.zona_celle
            } else {
                cella_del_cursore(s, pagina).map(|c| egui::Rect::from_min_max(rect.min + c.min.to_vec2() * scala, rect.min + c.max.to_vec2() * scala))
            };
            self.zona_celle = zona;
            let cella = zona.map(|z| ui.interact(z, egui::Id::new(("celle", self.id)), egui::Sense::click_and_drag()));
            if cella.is_none() {
                self.celle_prese = false;
            }
            let risposte = || [Some(&risposta), cella.as_ref()].into_iter().flatten();
            // Col dito fermo non arrivano eventi: senza un frame la pressione lunga non si vedrebbe
            // finché il dito non si muove o si alza.
            if risposte().any(egui::Response::is_pointer_button_down_on) {
                ui.ctx().request_repaint_after(Duration::from_millis(50));
            }
            if let Some(p) = risposte().find(|r| r.long_touched()).and_then(egui::Response::interact_pointer_pos) {
                self.ultimo_tocco = None;
                gesto = Some(Gesto::PressioneLunga(a_tocco(p)));
            } else if let Some(p) = risposte().find(|r| r.clicked()).and_then(egui::Response::interact_pointer_pos) {
                self.barra_al_cursore = None;
                let t = a_tocco(p);
                gesto = Some(match self.conta_tocchi(ora, t, scala) {
                    2 => Gesto::DoppioTocco(t),
                    3 => Gesto::TriploTocco(t),
                    _ => Gesto::Tocco(t),
                });
            }
            if let Some(r) = &cella {
                if r.dragged()
                    && let Some(p) = r.interact_pointer_pos()
                {
                    self.celle_prese = true;
                    gesto = Some(Gesto::Celle(a_tocco(p)));
                }
                if r.drag_stopped() && std::mem::take(&mut self.celle_prese) {
                    gesto = Some(Gesto::CelleLasciate);
                }
            }
            // Le maniglie, sopra la pagina; finché una è presa anche se la selezione si è chiusa,
            // perché egui tenga il trascinamento.
            let bordo = |pos: &Pos| {
                l.caret_on(pos, s.page_hint).map(|c| Bordo {
                    x: rect.min.x + c.x * scala,
                    cima: rect.min.y + c.top * scala,
                    fondo: rect.min.y + (c.top + c.height) * scala,
                })
            };
            let bordi =
                if (!collassata && immagine.is_none()) || self.maniglie.trascinata().is_some() { bordo(&sel_a).zip(bordo(&sel_b)) } else { None };
            match bordi {
                Some((inizio, fine)) => match self.maniglie.mostra(ui, egui::Id::new(("maniglie", self.id)), inizio, fine, BLU) {
                    Some(Trascinamento::Sposta { maniglia, punto, presa }) => {
                        self.punto_lente = Some(punto);
                        gesto = Some(Gesto::Maniglia { maniglia, punto: a_tocco(punto), presa });
                    }
                    Some(Trascinamento::Lasciata(_)) => {
                        self.punto_lente = None;
                        gesto = Some(Gesto::ManigliaLasciata);
                    }
                    None => {}
                },
                None => {
                    self.maniglie.lascia();
                    self.punto_lente = None;
                }
            }
            // La goccia del cursore, sopra la pagina; un tocco su di lei mostra la barra con Incolla.
            match (self.goccia.is_some() && collassata).then(|| bordo(&s.sel.focus)).flatten() {
                Some(b) => match self.maniglia_cursore.mostra(ui, egui::Id::new(("goccia", self.id)), b, BLU) {
                    Some(TrascinamentoCursore::Sposta { punto, presa }) => {
                        self.punto_lente = Some(punto);
                        gesto = Some(Gesto::Cursore { punto: a_tocco(punto), presa });
                    }
                    Some(TrascinamentoCursore::Lasciata) => {
                        self.punto_lente = None;
                        gesto = Some(Gesto::CursoreLasciato);
                    }
                    Some(TrascinamentoCursore::Toccata) => self.barra_al_cursore = Some(s.sel.focus.clone()),
                    None => {}
                },
                None => self.maniglia_cursore.lascia(),
            }
            // Il riquadro dell'immagine scelta e la sua maniglia, larga da `LATO_MINIMO_PT` al testo
            // della pagina; lasciata, la misura nuova.
            if let Some((r, info)) = &immagine {
                let schermo = egui::Rect::from_min_size(rect.min + egui::vec2(r.x, r.y) * scala, egui::vec2(r.w, r.h) * scala);
                let larghezze = (crate::immagine::LATO_MINIMO_PT * scala, info.larghezza_massima * scala);
                if let Some(TrascinamentoMisura::Lasciata { misura }) =
                    self.misura.mostra(ui, egui::Id::new(("misura", self.id)), schermo, larghezze, BLU)
                {
                    gesto = Some(Gesto::MisuraImmagine(misura.x / scala));
                }
            }
            let trascina = self.maniglie.trascinata().is_some() || self.maniglia_cursore.trascinata() || self.celle_prese;
            if trascina && let Some(punto) = self.punto_lente {
                // Nella lente anche il cursore, che non sta nelle fasce.
                let sopra: Vec<(egui::Rect, egui::Color32)> = rettangolo_cursore.filter(|_| collassata).map(|r| (r, BLU)).into_iter().collect();
                lente_con(ui.painter(), ui.clip_rect(), punto, carta, &evidenza, &immagini, &sopra);
                // Con la maniglia vicino al bordo la pagina scorre da sola, anche col dito fermo,
                // per allargare la selezione oltre lo schermo.
                let vista = ui.clip_rect();
                let passo = if punto.y > vista.max.y - BORDO_SCORRIMENTO {
                    -PASSO_SCORRIMENTO
                } else if punto.y < vista.min.y + BORDO_SCORRIMENTO {
                    PASSO_SCORRIMENTO
                } else {
                    0.0
                };
                if passo != 0.0 {
                    ui.scroll_with_delta_animation(egui::vec2(0.0, passo), egui::style::ScrollAnimation::none());
                    ui.ctx().request_repaint();
                }
            }
            // La barra sopra la selezione (o sotto, se sopra non c'è posto), solo col dito fermo,
            // dentro la vista e sopra la parte coperta in fondo.
            let barra = if trascina || self.senza_barra || immagine.is_some() {
                None
            } else if !collassata {
                let selezione = evidenza.iter().map(|(r, _)| *r).reduce(|a, b| a.union(b));
                selezione.map(|r| (r, QualeBarra::Selezione))
            } else if let Some((_, parola)) = &self.ortografia {
                // Sopra la parola sottolineata.
                l.selection_rects(&s.doc, &parola.inizio, &parola.fine, s.page_hint)
                    .into_iter()
                    .filter(|(pagina, _)| *pagina == 0)
                    .map(|(_, r)| egui::Rect::from_min_size(rect.min + egui::vec2(r.x, r.y) * scala, egui::vec2(r.w, r.h) * scala))
                    .reduce(|a, b| a.union(b))
                    .map(|r| (r, QualeBarra::Ortografia))
            } else if self.barra_al_cursore.is_some()
                && let Some(b) = bordo(&s.sel.focus)
            {
                Some((egui::Rect::from_min_max(egui::pos2(b.x, b.cima), egui::pos2(b.x, b.fondo)), QualeBarra::Cursore))
            } else {
                None
            };
            if let Some((selezione, quale)) = barra {
                let vista = finestra.intersect(ui.clip_rect());
                let zona = egui::Rect::from_min_max(vista.min, egui::pos2(vista.max.x, (vista.max.y - coperto).max(vista.min.y)));
                let id = egui::Id::new(("barra", self.id));
                // Fuori dallo schermo non serve.
                if selezione.max.y >= zona.min.y && selezione.min.y <= zona.max.y {
                    match (quale, &self.ortografia) {
                        (QualeBarra::Ortografia, Some((_, parola))) => {
                            if let Some(scelta) = barra_ortografia(ui.ctx(), id, selezione, zona, parola) {
                                gesto = Some(Gesto::Ortografia(scelta));
                            }
                        }
                        _ => {
                            if let Some(a) = barra_selezione(ui.ctx(), id, selezione, zona, quale == QualeBarra::Selezione) {
                                gesto = Some(Gesto::Azione(a));
                            }
                        }
                    }
                }
            }
            // Fasce lontane dallo schermo o oltre la fine della pagina: via dalla memoria della GPU.
            self.fasce.retain(|i, _| *i < n_fasce && *i + 2 >= prima && *i <= ultima + 2);
        });
        gesto
    }

    /// La vista Pagina: le pagine vere una sotto l'altra sulla carta, con l'ombra, `SPAZIO_PAGINE`
    /// fra loro e `MARGINE_PAGINE` dai lati, larghe quanto lo spazio che resta per lo zoom; in
    /// sola lettura. Fasce e liste di disegno come in Scrittura, per pagina. Un tocco dà la
    /// pagina e il punto in punti di quella pagina (sulla carta, il punto più vicino).
    fn mostra_pagine(&mut self, ui: &mut egui::Ui, s: &mut Session, aperto_alle: Instant, forza: Option<egui::Vec2>) -> Option<Gesto> {
        let vista = ui.available_width();
        let larghezza = (vista - 2.0 * MARGINE_PAGINE).max(1.0);
        let l = layout_pagine(s);
        let s: &Session = s;
        let w_max = l.pages.iter().map(|p| p.w).fold(0.0f32, f32::max);
        if w_max <= 0.0 {
            return None;
        }
        let ppp = ui.ctx().pixels_per_point();
        // Una fascia è larga quanto la pagina: col telefono in orizzontale e lo zoom alto
        // supererebbe il lato più lungo che la GPU accetta per una texture, e allora le fasce
        // restano a quella risoluzione e si scalano.
        let lato = ui.input(|i| i.max_texture_side).max(1) as f32;
        let px_per_pt = (larghezza / w_max * self.zoom * ppp).min(lato / w_max);
        let scala_layout = px_per_pt / ppp;
        let scala = larghezza / w_max * self.zoom_vivo;
        let fascia_pt = FASCIA_PX / px_per_pt;
        // Le liste per pagina vanno col layout; le fasce col layout e la scala: un pizzico rifà
        // le fasce, non le liste.
        let n_layout = self.numero_del_layout(&l);
        let chiave: ChiaveFascia = (n_layout, px_per_pt.to_bits());
        // La cima di ogni pagina, in dp dall'inizio della vista.
        let mut cime = Vec::with_capacity(l.pages.len());
        let mut y = CIMA_PAGINE;
        for p in &l.pages {
            cime.push(y);
            y += p.h * scala + SPAZIO_PAGINE;
        }
        let alto = y - SPAZIO_PAGINE + CIMA_PAGINE;
        let carta = colore_carta(s);
        let mut gesto = None;
        let sorgente = if self.dita_del_pizzico { egui::scroll_area::ScrollSource::NONE } else { egui::scroll_area::ScrollSource::ALL };
        let area = egui::ScrollArea::both().id_salt(("pagine", self.id)).scroll_source(sorgente).auto_shrink([false, false]);
        let area = match forza {
            Some(f) => area.scroll_offset(f),
            None => area,
        };
        area.show_viewport(ui, |ui, viewport| {
            // Come in Scrittura: almeno alta quanto la vista, non quanto lo spazio libero.
            let alto = alto.max(viewport.height());
            let (rect, risposta) = ui.allocate_exact_size(egui::vec2(vista.max(w_max * scala + 2.0 * MARGINE_PAGINE), alto), egui::Sense::click());
            self.scorrimento = viewport.min.to_vec2();
            self.origine = rect.min + viewport.min.to_vec2();
            let painter = ui.painter();
            painter.rect_filled(rect, 0.0, tema::CARTA);
            let sinistra = rect.min.x + MARGINE_PAGINE;
            let mut nuove = 0;
            let mut tenute = Vec::new();
            for (k, pagina) in l.pages.iter().enumerate() {
                let cima = cime[k];
                let h = pagina.h * scala;
                if cima > viewport.max.y || cima + h < viewport.min.y {
                    continue;
                }
                let foglio = egui::Rect::from_min_size(egui::pos2(sinistra, rect.min.y + cima), egui::vec2(pagina.w * scala, h));
                tema::dipingi_ombra(painter, foglio, 0, &tema::OMBRA);
                painter.rect_filled(foglio, 0.0, carta);
                let lista = match self.liste_pagine.get(&k) {
                    Some((n, lista)) if *n == n_layout => Arc::clone(lista),
                    _ => {
                        let lista = Arc::new(page_display(&s.doc, pagina, &RenderOptions::default().display));
                        self.misure.liste += 1;
                        self.liste_pagine.insert(k, (n_layout, Arc::clone(&lista)));
                        lista
                    }
                };
                let n_fasce = (pagina.h / fascia_pt).ceil().max(1.0) as usize;
                let prima = ((viewport.min.y - cima).max(0.0) / scala / fascia_pt).floor() as usize;
                let ultima = (((viewport.max.y - cima).max(0.0) / scala / fascia_pt).floor() as usize).min(n_fasce - 1);
                for i in prima..=ultima {
                    tenute.push((k, i));
                    let pronta = matches!(self.fasce_pagine.get(&(k, i)), Some((c, _)) if *c == chiave);
                    if !pronta && nuove < NUOVE_PER_FRAME {
                        let t = Instant::now();
                        match rendi_fascia_da(s, pagina, &lista, i, fascia_pt, px_per_pt) {
                            Some(img) => {
                                self.misure.fascia_ms = ms(t);
                                nuove += 1;
                                let ci = egui::ColorImage::from_rgba_premultiplied([img.width as usize, img.height as usize], &img.pixels);
                                match self.fasce_pagine.get_mut(&(k, i)) {
                                    Some((c, tex)) => {
                                        tex.set(ci, egui::TextureOptions::LINEAR);
                                        *c = chiave;
                                    }
                                    None => {
                                        let tex = ui.ctx().load_texture(format!("pagina{k}fascia{i}"), ci, egui::TextureOptions::LINEAR);
                                        self.fasce_pagine.insert((k, i), (chiave, tex));
                                    }
                                }
                                self.prima_fascia(aperto_alle);
                            }
                            None => {
                                self.fasce_pagine.remove(&(k, i));
                            }
                        }
                    } else if !pronta {
                        ui.ctx().request_repaint();
                    }
                    if let Some((_, tex)) = self.fasce_pagine.get(&(k, i)) {
                        let sui_pixel = |v: f32| (v * ppp).round() / ppp;
                        let y = foglio.min.y + i as f32 * fascia_pt * scala;
                        let dest = egui::Rect::from_min_size(
                            egui::pos2(sui_pixel(foglio.min.x), sui_pixel(y)),
                            tex.size_vec2() / ppp * (scala / scala_layout),
                        );
                        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                        painter.image(tex.id(), dest, uv, egui::Color32::WHITE);
                    }
                }
            }
            if risposta.clicked()
                && let Some(p) = risposta.interact_pointer_pos()
            {
                let y = p.y - rect.min.y;
                // La pagina sotto il dito; nello spazio fra due pagine, quella sopra; sopra la
                // prima, la prima.
                let k = cime.iter().rposition(|c| *c <= y).unwrap_or(0);
                let pagina = &l.pages[k];
                let x_pt = ((p.x - sinistra) / scala).clamp(0.0, pagina.w);
                let punto = Tocco { x_pt, y_pt: ((y - cime[k]) / scala).clamp(0.0, pagina.h) };
                gesto = Some(Gesto::ToccoPagina { pagina: k, punto });
            }
            // Fasce e liste delle pagine lontane dallo schermo: via dalla memoria.
            let vicine = |k: usize| tenute.iter().any(|(t, _)| t.abs_diff(k) <= 1);
            self.fasce_pagine.retain(|(k, i), _| *k < l.pages.len() && tenute.iter().any(|(t, j)| t == k && j.abs_diff(*i) <= 2));
            self.liste_pagine.retain(|k, _| vicine(*k));
        });
        gesto
    }
}

/// La barra del contesto scura (bozzetto 5) con Taglia, Copia, Incolla e Tutto, sopra tutto il
/// resto; con un cursore (`con_selezione` falso) solo Incolla e Tutto.
/// Quale barra del contesto sta sopra il testo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QualeBarra {
    /// Quella della selezione: Taglia, Copia, Incolla, Tutto, Commenta.
    Selezione,
    /// Quella al cursore: Incolla e Tutto.
    Cursore,
    /// Quella della parola sottolineata: i suggerimenti, poi Aggiungi al dizionario e Ignora.
    Ortografia,
}

/// La barra della parola sottolineata: i suggerimenti (solo parole) in una fila; per l'ortografia,
/// nella fila sotto, Aggiungi al dizionario e Ignora. Una parola della grammatica senza
/// suggerimenti non ha barra.
fn barra_ortografia(
    ctx: &egui::Context,
    id: egui::Id,
    parola: egui::Rect,
    zona: egui::Rect,
    sb: &crate::ortografia::Sbagliata,
) -> Option<SceltaOrtografia> {
    let n = sb.suggerimenti.len();
    if n == 0 && !sb.ortografia {
        return None;
    }
    let mut barra = sb.suggerimenti.iter().fold(BarraContesto::new(), |b, p| b.azione("", p));
    if sb.ortografia {
        barra = barra.a_capo().azione(icone::LIBRARY_ADD, AGGIUNGI_AL_DIZIONARIO).azione(icone::VISIBILITY_OFF, IGNORA);
    }
    match barra.mostra_in_primo_piano(ctx, id, parola, zona)? {
        i if i < n => Some(SceltaOrtografia::Correggi(i)),
        i if i == n => Some(SceltaOrtografia::Aggiungi),
        _ => Some(SceltaOrtografia::Ignora),
    }
}

fn barra_selezione(ctx: &egui::Context, id: egui::Id, selezione: egui::Rect, zona: egui::Rect, con_selezione: bool) -> Option<Azione> {
    let azioni: &[Azione] = if con_selezione {
        &[Azione::Taglia, Azione::Copia, Azione::Incolla, Azione::SelezionaTutto, Azione::Commenta]
    } else {
        &[Azione::Incolla, Azione::SelezionaTutto]
    };
    let barra = azioni.iter().fold(BarraContesto::new(), |b, a| b.azione(a.icona(), a.etichetta()));
    barra.mostra_in_primo_piano(ctx, id, selezione, zona).and_then(|i| azioni.get(i).copied())
}

#[cfg(test)]
mod test {
    use super::*;

    /// La lista ridotta alla fascia disegna la stessa fascia, pixel per pixel, anche con le
    /// sottolineature dell'ortografia e una riga a cavallo fra due fasce (7/11).
    #[test]
    fn la_fascia_con_la_lista_ridotta_e_uguale() {
        let testo: Vec<String> = (0..80).map(|i| format!("Riga {i} con parole sbagliatte e qualche errorre da sottolineare.")).collect();
        let mut s = crate::documento::apri_byte("prova.txt", testo.join("\n").as_bytes()).unwrap();
        let l = layout_scrittura(&mut s, 372.0, 1.0);
        let pagina = l.pages.first().unwrap();
        let lista = page_display(&s.doc, pagina, &RenderOptions::default().display);
        assert!(lista.iter().any(|d| matches!(d, Draw::Line { .. })), "la prova vuole le sottolineature");
        for indice in [0, 1, 2, 5] {
            let y = indice as f32 * 100.0;
            if y >= pagina.h {
                continue;
            }
            let h = 100.0f32.min(pagina.h - y);
            let intera = render_items(&s.doc, &lista, 0.0, y, pagina.w, h, 2.0, &RenderOptions::default());
            let ridotta = rendi_fascia_da(&s, pagina, &lista, indice, 100.0, 2.0).unwrap();
            assert!(intera.pixels == ridotta.pixels, "fascia {indice} diversa");
            assert!(nella_fascia(&lista, y, y + h).len() < lista.len());
        }
    }

    /// Misura, non prova (7/11, «se apro docx alcuni lagga»): quanto costano apertura,
    /// impaginazione allo zoom nuovo, lista di disegno e una fascia per un documento di circa 200
    /// pagine. Si lancia a mano, compilata come per il telefono:
    /// `cargo test --release -p tascabile-word -- --ignored misura_un_documento_lungo --nocapture`.
    #[test]
    #[ignore]
    fn misura_un_documento_lungo() {
        let testo: Vec<String> = (0..6000).map(|i| format!("Paragrafo {i}: l'operatore socio-sanitario svolge attività di assistenza diretta alla persona, in collaborazione con gli altri operatori.")).collect();
        let s = crate::documento::apri_byte("prova.txt", testo.join("\n").as_bytes()).unwrap();
        let byte = wordcraft_engine::io::save_bytes("prova.docx", &s.doc).unwrap();
        let t = Instant::now();
        let mut s = crate::documento::apri_byte("prova.docx", &byte).unwrap();
        let apertura = ms(t);
        let t = Instant::now();
        let l1 = layout_scrittura(&mut s, 372.0, 1.0);
        let layout_1 = ms(t);
        let t = Instant::now();
        let l2 = layout_scrittura(&mut s, 372.0, 1.3);
        let layout_zoom = ms(t);
        let pagina = l2.pages.first().unwrap();
        let t = Instant::now();
        let lista = page_display(&s.doc, pagina, &RenderOptions::default().display);
        let lista_ms = ms(t);
        let t = Instant::now();
        let _ = rendi_fascia_da(&s, pagina, &lista, 3, 300.0, 3.0);
        let fascia = ms(t);
        let t = Instant::now();
        let _ = rendi_fascia_da(&s, pagina, &lista, 400, 300.0, 3.0);
        let fascia_dopo = ms(t);
        let t = Instant::now();
        let _ = layout_scrittura(&mut s, 372.0, 1.3);
        let layout_di_nuovo = ms(t);
        eprintln!("MISURA seconda fascia {fascia_dopo:.0} ms, stesso layout di nuovo {layout_di_nuovo:.0} ms");
        // La riduzione della lista da sola, e la fascia con la lista già ridotta.
        let (y0, y1) = (400.0 * 300.0, 401.0 * 300.0);
        let t = Instant::now();
        let ridotta = nella_fascia(&lista, y0, y1);
        let filtro = ms(t);
        let t = Instant::now();
        let _ = rendi_fascia_da(&s, pagina, &ridotta, 400, 300.0, 3.0);
        let fascia_ridotta = ms(t);
        let conta = |f: fn(&Draw) -> bool| lista.iter().filter(|d| f(d)).count();
        eprintln!(
            "MISURA lista {} voci (testo {}, riempimenti {}, linee {}), ridotta {} voci in {filtro:.0} ms: fascia {fascia_ridotta:.0} ms",
            lista.len(),
            conta(|d| matches!(d, Draw::Glyphs { .. })),
            conta(|d| matches!(d, Draw::Fill { .. })),
            conta(|d| matches!(d, Draw::Line { .. })),
            ridotta.len()
        );
        eprintln!(
            "MISURA {} byte, {} paragrafi, alta {:.0} pt: apertura {apertura:.0} ms, layout {layout_1:.0} ms, layout allo zoom nuovo {layout_zoom:.0} ms, lista {lista_ms:.0} ms, una fascia {fascia:.0} ms",
            byte.len(),
            s.doc.paragraph_count(),
            l1.pages.first().map_or(0.0, |p| p.h)
        );
    }
    use crate::prova::documento;
    use serde_json::json;
    use tascabile_touch::{Bordo, tema};
    use wordcraft_engine::doc::StoryRef;
    use wordcraft_engine::layout::Placed;
    use wordcraft_engine::render::render_area;

    /// Telefono finto: egui senza schermo, come nelle prove della revisione.
    struct Telefono {
        ctx: egui::Context,
        dim: egui::Vec2,
        ppp: f32,
        t: f64,
    }

    /// Quello che serve di un frame.
    struct Frame {
        tocco: Option<Tocco>,
        gesto: Option<Gesto>,
        /// Texture caricate o rifatte nel frame.
        caricate: Vec<egui::TextureId>,
        forme: Vec<egui::epaint::ClippedShape>,
        ridisegna: bool,
    }

    impl Frame {
        /// Il cursore a schermo: il rettangolo blu sottile (le gocce delle maniglie sono larghe).
        fn cursore(&self) -> Option<egui::Rect> {
            self.forme.iter().find_map(|f| match &f.shape {
                egui::Shape::Rect(r) if r.fill == tema::BLU && r.rect.width() < 3.0 => Some(r.rect),
                _ => None,
            })
        }

        /// La carta: il rettangolo pieno largo quanto la vista (la cornice di egui è trasparente).
        fn carta(&self) -> Option<&egui::epaint::RectShape> {
            self.forme.iter().find_map(|f| match &f.shape {
                egui::Shape::Rect(r) if r.rect.width() > 400.0 && r.fill != egui::Color32::TRANSPARENT => Some(r),
                _ => None,
            })
        }

        /// Dove è disegnata la texture `id`.
        fn immagine(&self, id: egui::TextureId) -> Option<egui::Rect> {
            self.forme.iter().find_map(|f| match &f.shape {
                egui::Shape::Mesh(m) if m.texture_id == id => Some(m.calc_bounds()),
                _ => None,
            })
        }
    }

    fn dito(phase: egui::TouchPhase, pos: egui::Pos2) -> egui::Event {
        egui::Event::Touch { device_id: egui::TouchDeviceId(1), id: egui::TouchId(0), phase, pos, force: None }
    }

    fn tasto(pressed: bool, pos: egui::Pos2) -> egui::Event {
        egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE }
    }

    impl Telefono {
        /// 412 × 800 dp a 2,625 pixel per punto, come un telefono da 1080 pixel di larghezza.
        fn new() -> Self {
            Self { ctx: egui::Context::default(), dim: egui::vec2(412.0, 800.0), ppp: 2.625, t: 0.0 }
        }

        fn passo(&mut self, v: &mut Vista, s: &mut Session, eventi: Vec<egui::Event>) -> Frame {
            let mut ri = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, self.dim)),
                max_texture_side: Some(4096),
                time: Some(self.t),
                predicted_dt: 1.0 / 60.0,
                events: eventi,
                ..Default::default()
            };
            ri.viewports.entry(egui::ViewportId::ROOT).or_default().native_pixels_per_point = Some(self.ppp);
            self.t += 1.0 / 60.0;
            let mut gesto = None;
            let mut out = self.ctx.run_ui(ri, |ui| gesto = v.mostra(ui, s, None, Instant::now()));
            let tocco = gesto.and_then(Gesto::tocco);
            let caricate = out.textures_delta.set.keys().copied().collect();
            // Un delta buttato senza svuotarlo manda epaint in panico (debug_assert nel Drop).
            out.textures_delta.clear();
            let ridisegna = out.viewport_output.get(&egui::ViewportId::ROOT).is_some_and(|o| o.repaint_delay.is_zero());
            Frame { tocco, gesto, caricate, forme: std::mem::take(&mut out.shapes), ridisegna }
        }

        /// Frame vuoti finché egui non chiede più di ridisegnare (al massimo 300: dopo un colpo di
        /// dito lo scorrimento per inerzia dura quasi due secondi).
        fn calma(&mut self, v: &mut Vista, s: &mut Session) -> Frame {
            let mut f = self.passo(v, s, vec![]);
            for _ in 0..300 {
                if !f.ridisegna {
                    break;
                }
                f = self.passo(v, s, vec![]);
            }
            f
        }

        /// Dito giù e su nello stesso punto, in due frame, un secondo dopo il frame di prima (mai
        /// un doppio tocco): restituisce il tocco.
        fn tocca(&mut self, v: &mut Vista, s: &mut Session, p: egui::Pos2) -> Option<Tocco> {
            self.t += 1.0;
            self.tocca_subito(v, s, p).and_then(Gesto::tocco)
        }

        /// Dito giù e su nello stesso punto, nei due frame subito dopo: restituisce il gesto.
        fn tocca_subito(&mut self, v: &mut Vista, s: &mut Session, p: egui::Pos2) -> Option<Gesto> {
            let giu = self.passo(v, s, vec![dito(egui::TouchPhase::Start, p), egui::Event::PointerMoved(p), tasto(true, p)]);
            let su = self.passo(v, s, vec![dito(egui::TouchPhase::End, p), tasto(false, p), egui::Event::PointerGone]);
            giu.gesto.or(su.gesto)
        }

        /// Dito che scorre di `dy` dp in dieci frame partendo da `da`: restituisce il tocco, se c'è stato.
        fn trascina(&mut self, v: &mut Vista, s: &mut Session, da: egui::Pos2, dy: f32) -> Option<Tocco> {
            let mut tocco = self.passo(v, s, vec![dito(egui::TouchPhase::Start, da), egui::Event::PointerMoved(da), tasto(true, da)]).tocco;
            for k in 1..=10 {
                let p = da + egui::vec2(0.0, dy * k as f32 / 10.0);
                tocco = tocco.or(self.passo(v, s, vec![dito(egui::TouchPhase::Move, p), egui::Event::PointerMoved(p)]).tocco);
            }
            let fine = da + egui::vec2(0.0, dy);
            tocco.or(self.passo(v, s, vec![dito(egui::TouchPhase::End, fine), tasto(false, fine), egui::Event::PointerGone]).tocco)
        }

        fn fascia_pt(&self) -> f32 {
            FASCIA_PX / (DP_PER_PT * self.ppp)
        }
    }

    impl Frame {
        /// I testi scritti da egui nel frame (etichette dei bottoni).
        fn testi(&self) -> Vec<String> {
            self.forme
                .iter()
                .filter_map(|f| match &f.shape {
                    egui::Shape::Text(t) => Some(t.galley.text().to_string()),
                    _ => None,
                })
                .collect()
        }

        fn rettangoli_di(&self, colore: egui::Color32) -> usize {
            self.forme.iter().filter(|f| matches!(&f.shape, egui::Shape::Rect(r) if r.fill == colore)).count()
        }

        /// Le gocce blu delle maniglie.
        fn gocce(&self) -> usize {
            let goccia = egui::Vec2::splat(tascabile_touch::GOCCIA);
            self.forme.iter().filter(|f| matches!(&f.shape, egui::Shape::Rect(r) if r.fill == tema::BLU && r.rect.size() == goccia)).count()
        }

        /// La goccia del cursore (il poligono blu con la punta in alto): la sua punta.
        fn goccia_cursore(&self) -> Option<egui::Pos2> {
            let mut gocce = self.forme.iter().filter_map(|f| match &f.shape {
                egui::Shape::Path(p) if p.fill == tema::BLU && p.closed => p.points.first().copied(),
                _ => None,
            });
            let prima = gocce.next();
            assert!(gocce.next().is_none(), "più di una goccia del cursore");
            prima
        }

        /// La lente è a schermo.
        fn lente(&self) -> bool {
            self.forme.iter().any(|f| matches!(&f.shape, egui::Shape::Rect(r) if (r.rect.size() - tascabile_touch::LENTE).length() < 0.5))
        }
    }

    /// Il punto sullo schermo davanti alla posizione `off` del paragrafo `par`, a metà della riga,
    /// con la vista non scorsa e lo zoom 1.
    fn sullo_schermo(s: &mut Session, par: usize, off: usize) -> egui::Pos2 {
        let c = layout_scrittura(s, 412.0, 1.0).caret_on(&Pos::body(par, off), 0).unwrap();
        egui::pos2(c.x * DP_PER_PT, (c.top + c.height / 2.0) * DP_PER_PT)
    }

    /// Dove sta il testo `etichetta` disegnato da egui nel frame (il suo rettangolo).
    fn testo_a_schermo(f: &Frame, etichetta: &str) -> Option<egui::Rect> {
        f.forme.iter().find_map(|x| match &x.shape {
            egui::Shape::Text(t) if t.galley.text() == etichetta => Some(t.galley.rect.translate(t.pos.to_vec2())),
            _ => None,
        })
    }

    #[test]
    fn sopra_la_parola_sottolineata_la_barra_coi_suggerimenti() {
        crate::ortografia::test::con_il_dizionario();
        // Più in basso, perché sopra ci sia posto per la barra.
        let mut s = documento(&["Uno", "Due", "Tre", "Quattro", "Il letto perche la terapia"]);
        let mut v = Vista::default();
        let mut tel = Telefono::new();
        tel.calma(&mut v, &mut s);
        s.run("caret.set", &json!({ "pos": Pos::body(4, 11) })).unwrap();
        let parola = crate::ortografia::al_cursore(&mut s).unwrap();
        v.mostra_ortografia(s.sel.focus.clone(), parola);
        let f = tel.calma(&mut v, &mut s);
        let perche = testo_a_schermo(&f, "perché").expect("il suggerimento nella barra");
        let aggiungi = testo_a_schermo(&f, AGGIUNGI_AL_DIZIONARIO).expect("Aggiungi al dizionario");
        assert!(testo_a_schermo(&f, IGNORA).is_some());
        // Le parole in una fila, le azioni sotto; tutto sopra la parola sottolineata.
        let riga = sullo_schermo(&mut s, 4, 11);
        assert!(aggiungi.top() > perche.bottom() && aggiungi.bottom() < riga.y, "{perche:?} {aggiungi:?} {riga:?}");
        let g = tel.tocca_subito(&mut v, &mut s, perche.center());
        assert_eq!(g, Some(Gesto::Ortografia(SceltaOrtografia::Correggi(0))));
        // Il cursore altrove: la barra se ne va.
        s.run("caret.set", &json!({ "pos": Pos::body(4, 2) })).unwrap();
        let f = tel.calma(&mut v, &mut s);
        assert!(testo_a_schermo(&f, IGNORA).is_none() && v.ortografia().is_none());
    }

    #[test]
    fn la_barra_della_selezione_ha_commenta() {
        let mut s = documento(&["Ciao mondo, questa è una prova"]);
        let mut v = Vista::default();
        let mut tel = Telefono::new();
        tel.calma(&mut v, &mut s);
        s.run("select.range", &json!({ "anchor": Pos::body(0, 0), "focus": Pos::body(0, 10) })).unwrap();
        let f = tel.calma(&mut v, &mut s);
        let commenta = testo_a_schermo(&f, "Commenta").expect("Commenta nella barra");
        let tutto = testo_a_schermo(&f, "Tutto").unwrap();
        assert!(commenta.right() <= 412.0 && tutto.right() <= 412.0, "{commenta:?} {tutto:?}");
        assert_eq!(tel.tocca_subito(&mut v, &mut s, commenta.center()), Some(Gesto::Azione(Azione::Commenta)));
    }

    #[test]
    fn il_tocco_sul_bordo_delle_lettere_sceglie_il_lato_piu_vicino() {
        let testo = "Ciao mondo, questa è una prova: il 3,5% già";
        let mut s = documento(&[testo]);
        let mut v = Vista::default();
        let mut tel = Telefono::new();
        tel.calma(&mut v, &mut s);
        let l = layout_scrittura(&mut s, 412.0, 1.0);
        let posti: Vec<usize> = testo.char_indices().map(|(i, _)| i).chain([testo.len()]).collect();
        for coppia in posti.windows(2) {
            let (a, b) = (coppia[0], coppia[1]);
            let (pa, pb) = (sullo_schermo(&mut s, 0, a), sullo_schermo(&mut s, 0, b));
            let mezzo = (pa.x + pb.x) / 2.0;
            // Poco prima della metà della lettera il cursore va prima, poco dopo va dopo; anche
            // col dito alto o basso sulla riga.
            let c = l.caret_on(&Pos::body(0, a), 0).unwrap();
            for y in [c.top + 0.5, c.top + c.height / 2.0, c.top + c.height - 0.5].map(|y| y * DP_PER_PT) {
                for (dx, atteso) in [(-0.6, a), (0.6, b)] {
                    let t = tel.tocca(&mut v, &mut s, egui::pos2(mezzo + dx, y)).expect("tocco");
                    let pos = l.hit(0, t.x_pt, t.y_pt, StoryRef::Body);
                    assert_eq!(pos, Some(Pos::body(0, atteso)), "fra {a} e {b}, {dx} dp dalla metà, y {y}");
                }
            }
        }
    }

    #[test]
    fn il_tocco_fra_due_righe_va_sulla_riga_sotto_il_dito() {
        let testo = "parola ".repeat(30);
        let mut s = documento(&[testo.trim_end()]);
        let mut v = Vista::default();
        let mut tel = Telefono::new();
        tel.calma(&mut v, &mut s);
        let l = layout_scrittura(&mut s, 412.0, 1.0);
        // La prima parola della seconda riga.
        let a_capo = (0..testo.len()).find(|off| l.caret_on(&Pos::body(0, *off), 0).unwrap().top > 1.0).unwrap();
        let sopra = l.caret_on(&Pos::body(0, 0), 0).unwrap();
        let sotto = l.caret_on(&Pos::body(0, a_capo), 0).unwrap();
        let x = 30.0;
        for (y, riga) in [(sopra.top + sopra.height - 0.5, sopra.top), (sotto.top + 0.5, sotto.top)] {
            let t = tel.tocca(&mut v, &mut s, egui::pos2(x, y * DP_PER_PT)).expect("tocco");
            let pos = l.hit(0, t.x_pt, t.y_pt, StoryRef::Body).unwrap();
            assert_eq!(l.caret_on(&pos, 0).unwrap().top, riga, "y {y} pt: {pos:?}");
        }
        // A sinistra dell'inizio della seconda riga: il cursore all'inizio di quella riga.
        let t = tel.tocca(&mut v, &mut s, egui::pos2(1.0, (sotto.top + sotto.height / 2.0) * DP_PER_PT)).unwrap();
        let pos = l.hit(0, t.x_pt, t.y_pt, StoryRef::Body).unwrap();
        assert_eq!((pos.off, l.caret_on(&pos, 0).unwrap().top), (a_capo, sotto.top));
    }

    #[test]
    fn doppio_e_triplo_tocco() {
        let mut s = documento(&["Ciao mondo, questa è una prova"]);
        let mut v = Vista::default();
        let mut tel = Telefono::new();
        tel.calma(&mut v, &mut s);
        let p = sullo_schermo(&mut s, 0, 7);
        tel.t += 1.0;
        let visti: Vec<Option<Gesto>> = (0..4).map(|_| tel.tocca_subito(&mut v, &mut s, p)).collect();
        let a_tocco = |p: egui::Pos2| Tocco { x_pt: p.x / DP_PER_PT, y_pt: p.y / DP_PER_PT };
        let t = a_tocco(p);
        // Dopo il triplo si riparte da un tocco.
        assert_eq!(visti, [Some(Gesto::Tocco(t)), Some(Gesto::DoppioTocco(t)), Some(Gesto::TriploTocco(t)), Some(Gesto::Tocco(t))]);
        // Lontani un dito l'uno dall'altro, o con troppo tempo fra loro, sono tocchi semplici.
        let lontano = p + egui::vec2(60.0, 0.0);
        tel.t += 1.0;
        let due = [tel.tocca_subito(&mut v, &mut s, p), tel.tocca_subito(&mut v, &mut s, lontano)];
        assert_eq!(due, [Some(Gesto::Tocco(t)), Some(Gesto::Tocco(a_tocco(lontano)))]);
        tel.tocca_subito(&mut v, &mut s, p);
        tel.t += 0.6;
        assert_eq!(tel.tocca_subito(&mut v, &mut s, p), Some(Gesto::Tocco(t)));
        // Un dito vicino, mezza lettera più in là, conta per il doppio tocco.
        tel.t += 1.0;
        tel.tocca_subito(&mut v, &mut s, p);
        let vicino = tel.tocca_subito(&mut v, &mut s, p + egui::vec2(8.0, 3.0));
        assert_eq!(vicino, Some(Gesto::DoppioTocco(a_tocco(p + egui::vec2(8.0, 3.0)))));
    }

    #[test]
    fn la_goccia_del_cursore_sotto_il_cursore_fino_a_quando_serve() {
        let mut s = documento(&["Ciao mondo, questa è una prova"]);
        s.run("caret.set", &json!({ "pos": Pos::body(0, 4) })).unwrap();
        let mut v = Vista::default();
        let mut tel = Telefono::new();
        assert_eq!(tel.calma(&mut v, &mut s).goccia_cursore(), None, "senza tocco niente goccia");
        // Dopo il tocco che ha messo il cursore: la punta sul fondo della riga, sotto il cursore.
        v.mostra_maniglia_cursore();
        let f = tel.passo(&mut v, &mut s, vec![]);
        let c = layout_scrittura(&mut s, 412.0, 1.0).caret_on(&Pos::body(0, 4), 0).unwrap();
        let punta = f.goccia_cursore().expect("la goccia del cursore");
        assert!((punta.x - c.x * DP_PER_PT).abs() < 0.01 && (punta.y - (c.top + c.height) * DP_PER_PT).abs() < 0.01, "{punta:?}");
        assert!(f.cursore().is_some(), "il cursore resta");
        assert_eq!(f.gocce(), 0, "niente gocce della selezione");
        // Passati quattro secondi senza toccarla se ne va, come in Android.
        tel.t += 3.5;
        assert!(tel.passo(&mut v, &mut s, vec![]).goccia_cursore().is_some());
        tel.t += 0.6;
        assert!(tel.calma(&mut v, &mut s).goccia_cursore().is_none());
        // Se ne va anche quando si scrive (il cursore si sposta) e con una selezione.
        v.mostra_maniglia_cursore();
        assert!(tel.passo(&mut v, &mut s, vec![]).goccia_cursore().is_some());
        s.run("text.insert", &json!({ "text": "x", "raw": true })).unwrap();
        assert!(tel.passo(&mut v, &mut s, vec![]).goccia_cursore().is_none());
        v.mostra_maniglia_cursore();
        tel.passo(&mut v, &mut s, vec![]);
        s.run("select.range", &json!({ "anchor": Pos::body(0, 0), "focus": Pos::body(0, 4) })).unwrap();
        let f = tel.passo(&mut v, &mut s, vec![]);
        assert_eq!((f.goccia_cursore(), f.gocce()), (None, 2));
    }

    #[test]
    fn la_goccia_del_cursore_trascinata_da_il_gesto_e_la_lente() {
        let mut s = documento(&["Ciao mondo, questa è una prova"]);
        s.run("caret.set", &json!({ "pos": Pos::body(0, 4) })).unwrap();
        let mut v = Vista::default();
        let mut tel = Telefono::new();
        tel.calma(&mut v, &mut s);
        v.mostra_maniglia_cursore();
        tel.passo(&mut v, &mut s, vec![]);
        let c = layout_scrittura(&mut s, 412.0, 1.0).caret_on(&Pos::body(0, 4), 0).unwrap();
        let b = Bordo { x: c.x * DP_PER_PT, cima: c.top * DP_PER_PT, fondo: (c.top + c.height) * DP_PER_PT };
        let presa = tascabile_touch::ManigliaCursore::centro(b);
        let mut visti: Vec<Gesto> = Vec::new();
        visti.extend(
            tel.passo(&mut v, &mut s, vec![dito(egui::TouchPhase::Start, presa), egui::Event::PointerMoved(presa), tasto(true, presa)]).gesto,
        );
        let mut lente_vista = false;
        for k in 1..=5 {
            let p = presa + egui::vec2(12.0 * k as f32, 0.0);
            let f = tel.passo(&mut v, &mut s, vec![dito(egui::TouchPhase::Move, p), egui::Event::PointerMoved(p)]);
            lente_vista |= f.lente();
            assert!(f.goccia_cursore().is_some(), "la goccia resta sotto il dito");
            visti.extend(f.gesto);
        }
        assert!(lente_vista, "la lente non si è vista");
        assert!(matches!(visti.first(), Some(Gesto::Cursore { presa: true, .. })), "{visti:?}");
        let Some(Gesto::Cursore { punto, presa: false }) = visti.last() else { panic!("{visti:?}") };
        // Il cursore segue il dito, a metà della riga.
        assert!((punto.x_pt - (c.x + 60.0 / DP_PER_PT)).abs() < 0.1 && (punto.y_pt - (c.top + c.height / 2.0)).abs() < 0.1, "{punto:?}");
        let p = presa + egui::vec2(60.0, 0.0);
        let su = tel.passo(&mut v, &mut s, vec![dito(egui::TouchPhase::End, p), tasto(false, p), egui::Event::PointerGone]);
        assert_eq!(su.gesto, Some(Gesto::CursoreLasciato));
        assert!(!su.lente());
        // Un tocco sulla goccia mostra la barra con Incolla, non sposta il cursore.
        tel.t += 1.0;
        let toccato = tel.tocca_subito(&mut v, &mut s, presa);
        assert_eq!(toccato, None, "{toccato:?}");
        assert!(v.ha_barra_al_cursore());
    }

    #[test]
    fn pressione_lunga_col_dito_fermo() {
        let mut s = documento(&["Ciao mondo, questa è una prova"]);
        let mut v = Vista::default();
        let mut tel = Telefono::new();
        tel.calma(&mut v, &mut s);
        let p = egui::pos2(100.0, 12.0);
        let mut visti = vec![tel.passo(&mut v, &mut s, vec![dito(egui::TouchPhase::Start, p), egui::Event::PointerMoved(p), tasto(true, p)]).gesto];
        // Un secondo col dito fermo, senza eventi: la vista chiede i frame da sé.
        for _ in 0..60 {
            let f = tel.passo(&mut v, &mut s, vec![]);
            visti.push(f.gesto);
        }
        visti.push(tel.passo(&mut v, &mut s, vec![dito(egui::TouchPhase::End, p), tasto(false, p), egui::Event::PointerGone]).gesto);
        let visti: Vec<Gesto> = visti.into_iter().flatten().collect();
        assert!(matches!(visti.as_slice(), [Gesto::PressioneLunga(t)] if (t.x_pt - 75.0).abs() < 0.05 && (t.y_pt - 9.0).abs() < 0.05), "{visti:?}");
    }

    #[test]
    fn la_selezione_si_vede_con_maniglie_e_barra() {
        let mut s = documento(&["Ciao mondo, questa è una prova"]);
        let mut v = Vista::default();
        let mut tel = Telefono::new();
        let f = tel.calma(&mut v, &mut s);
        assert_eq!((f.rettangoli_di(SELEZIONE), f.gocce()), (0, 0), "senza selezione niente evidenza né maniglie");
        assert!(f.cursore().is_some());
        assert!(!f.testi().iter().any(|t| t == "Copia"));
        s.run("select.range", &json!({ "anchor": Pos::body(0, 5), "focus": Pos::body(0, 10) })).unwrap();
        let f = tel.calma(&mut v, &mut s);
        // Il fondo blu tenue del bozzetto, con gli angoli appena tondi, e le due gocce blu.
        assert_eq!(f.rettangoli_di(tema::SELEZIONE), 1);
        assert!(
            f.forme
                .iter()
                .any(|f| matches!(&f.shape, egui::Shape::Rect(r) if r.fill == tema::SELEZIONE && r.corner_radius == egui::CornerRadius::same(2))),
            "angoli della selezione"
        );
        assert_eq!(f.gocce(), 2);
        assert!(f.cursore().is_none(), "con la selezione il cursore non si disegna");
        // La barra del contesto scura, con icone ed etichette, sopra la selezione.
        let testi = f.testi();
        for b in ["Taglia", "Copia", "Incolla", "Tutto"] {
            assert!(testi.iter().any(|t| t == b), "manca «{b}» in {testi:?}");
        }
        assert!(testi.iter().any(|t| t == tascabile_touch::icone::CONTENT_COPY), "manca l'icona di Copia in {testi:?}");
        assert_eq!(f.rettangoli_di(tema::SCURO), 1, "la barra scura");
        // Un cursore dopo una pressione lunga su uno spazio: solo Incolla e Seleziona tutto.
        s.run("caret.set", &json!({ "pos": Pos::body(0, 4) })).unwrap();
        v.barra_al_cursore(Pos::body(0, 4));
        let testi = tel.calma(&mut v, &mut s).testi();
        assert!(testi.iter().any(|t| t == "Incolla") && !testi.iter().any(|t| t == "Copia"), "{testi:?}");
        // Il cursore si sposta: la barra se ne va.
        s.run("caret.set", &json!({ "pos": Pos::body(0, 8) })).unwrap();
        assert!(!tel.calma(&mut v, &mut s).testi().iter().any(|t| t == "Incolla"));
    }

    #[test]
    fn la_maniglia_trascinata_da_il_gesto_e_la_lente() {
        let mut s = documento(&["Ciao mondo, questa è una prova"]);
        s.run("select.range", &json!({ "anchor": Pos::body(0, 5), "focus": Pos::body(0, 10) })).unwrap();
        let mut v = Vista::default();
        let mut tel = Telefono::new();
        tel.calma(&mut v, &mut s);
        let l = layout_scrittura(&mut s, 412.0, 1.0);
        let c = l.caret_on(&Pos::body(0, 10), 0).unwrap();
        let fine = Bordo { x: c.x * DP_PER_PT, cima: c.top * DP_PER_PT, fondo: (c.top + c.height) * DP_PER_PT };
        let presa = Maniglie::centro(Maniglia::Fine, fine);
        let mut visti: Vec<Gesto> = Vec::new();
        visti.extend(
            tel.passo(&mut v, &mut s, vec![dito(egui::TouchPhase::Start, presa), egui::Event::PointerMoved(presa), tasto(true, presa)]).gesto,
        );
        let mut lente_vista = false;
        for k in 1..=5 {
            let p = presa + egui::vec2(12.0 * k as f32, 0.0);
            let f = tel.passo(&mut v, &mut s, vec![dito(egui::TouchPhase::Move, p), egui::Event::PointerMoved(p)]);
            lente_vista |=
                f.forme.iter().any(|f| matches!(&f.shape, egui::Shape::Rect(r) if (r.rect.size() - tascabile_touch::LENTE).length() < 0.5));
            visti.extend(f.gesto);
        }
        assert!(lente_vista, "la lente non si è vista");
        assert!(matches!(visti.first(), Some(Gesto::Maniglia { maniglia: Maniglia::Fine, presa: true, .. })), "{visti:?}");
        let Some(Gesto::Maniglia { punto, presa: false, .. }) = visti.last() else { panic!("{visti:?}") };
        // Il bordo segue il dito, a metà della riga.
        assert!((punto.x_pt - (c.x + 60.0 / DP_PER_PT)).abs() < 0.1 && (punto.y_pt - (c.top + c.height / 2.0)).abs() < 0.1, "{punto:?}");
        let p = presa + egui::vec2(60.0, 0.0);
        let su = tel.passo(&mut v, &mut s, vec![dito(egui::TouchPhase::End, p), tasto(false, p), egui::Event::PointerGone]);
        assert_eq!(su.gesto, Some(Gesto::ManigliaLasciata));
    }

    #[test]
    fn la_maniglia_vicino_al_bordo_fa_scorrere_la_pagina() {
        let mut s = crate::esempi::lungo(30);
        let mut v = Vista::default();
        let mut tel = Telefono::new();
        tel.calma(&mut v, &mut s);
        // Una selezione su una riga a metà schermo.
        let l = layout_scrittura(&mut s, 412.0, 1.0);
        let a = l.hit(0, 20.0, 400.0 / DP_PER_PT, StoryRef::Body).unwrap();
        let b = l.hit(0, 120.0, 400.0 / DP_PER_PT, StoryRef::Body).unwrap();
        s.run("select.range", &json!({ "anchor": a, "focus": b })).unwrap();
        tel.calma(&mut v, &mut s);
        let c = l.caret_on(&b, 0).unwrap();
        let presa = Maniglie::centro(Maniglia::Fine, Bordo { x: c.x * DP_PER_PT, cima: c.top * DP_PER_PT, fondo: (c.top + c.height) * DP_PER_PT });
        tel.passo(&mut v, &mut s, vec![dito(egui::TouchPhase::Start, presa), egui::Event::PointerMoved(presa), tasto(true, presa)]);
        // Il dito scende fino in fondo allo schermo e resta lì.
        let giu = egui::pos2(presa.x, 795.0);
        let mut punti = Vec::new();
        let f = tel.passo(&mut v, &mut s, vec![dito(egui::TouchPhase::Move, giu), egui::Event::PointerMoved(giu)]);
        punti.extend(f.gesto);
        for _ in 0..30 {
            punti.extend(tel.passo(&mut v, &mut s, vec![]).gesto);
        }
        let ys: Vec<f32> = punti.iter().filter_map(|g| if let Gesto::Maniglia { punto, .. } = g { Some(punto.y_pt) } else { None }).collect();
        assert!(ys.len() > 20, "la maniglia ferma non dà più il punto: {ys:?}");
        // 30 frame da 8 dp: il punto è sceso di circa 240 dp nella pagina.
        let sceso = (ys[ys.len() - 1] - ys[0]) * DP_PER_PT;
        assert!(sceso > 150.0, "la pagina non è scesa: {sceso} dp");
    }

    #[test]
    fn lo_zoom_resta_fra_i_bordi() {
        let mut v = Vista::default();
        v.pizzica(10.0, false);
        assert_eq!(v.zoom, ZOOM_MASSIMO);
        v.pizzica(0.01, false);
        assert_eq!(v.zoom, ZOOM_MINIMO);
        // Fattori che non hanno senso non cambiano niente.
        v.pizzica(f32::NAN, false);
        v.pizzica(0.0, false);
        assert_eq!(v.zoom, ZOOM_MINIMO);
    }

    #[test]
    fn il_pizzico_con_due_dita_e_dopo_il_rilascio_la_larghezza_nuova() {
        let mut s = documento(&["Ciao mondo, questa è una prova"]);
        let mut v = Vista::default();
        let mut tel = Telefono::new();
        tel.calma(&mut v, &mut s);
        let prima = s.view.web_width;
        assert!((prima - 412.0 / DP_PER_PT).abs() < 0.5, "{prima}");
        let due = |phase, d: f32| {
            vec![
                egui::Event::Touch { device_id: egui::TouchDeviceId(1), id: egui::TouchId(0), phase, pos: egui::pos2(200.0 - d, 400.0), force: None },
                egui::Event::PointerMoved(egui::pos2(200.0 - d, 400.0)),
                egui::Event::Touch { device_id: egui::TouchDeviceId(1), id: egui::TouchId(1), phase, pos: egui::pos2(200.0 + d, 400.0), force: None },
            ]
        };
        tel.passo(&mut v, &mut s, due(egui::TouchPhase::Start, 50.0));
        // Le dita si allargano (egui conta il gesto dal primo spostamento): durante il gesto il
        // layout resta quello di prima.
        for k in 1..=5 {
            tel.passo(&mut v, &mut s, due(egui::TouchPhase::Move, 50.0 + 10.0 * k as f32));
        }
        let vivo = v.zoom_vivo();
        assert!(vivo > 1.5 && v.zoom == 1.0, "zoom vivo {vivo} zoom {}", v.zoom);
        assert_eq!(s.view.web_width, prima);
        // Le dita si alzano: il documento si reimpagina alla larghezza divisa per lo zoom.
        tel.passo(&mut v, &mut s, due(egui::TouchPhase::End, 100.0));
        tel.calma(&mut v, &mut s);
        assert_eq!(v.zoom, vivo, "dopo il rilascio");
        assert!((s.view.web_width - prima / vivo).abs() < 0.5, "{}", s.view.web_width);
    }

    /// Un pizzico col dito 0 fermo in `fermo` e il dito 1 che si allontana per `frame` frame;
    /// restituisce i gesti visti, compreso il rilascio. Con il dito 0 arrivano anche gli eventi
    /// del puntatore, come su Android.
    fn pizzico_col_pollice_fermo(tel: &mut Telefono, v: &mut Vista, s: &mut Session, fermo: egui::Pos2, frame: usize) -> Vec<Gesto> {
        let uno = |phase, pos| egui::Event::Touch { device_id: egui::TouchDeviceId(1), id: egui::TouchId(1), phase, pos, force: None };
        let altro = |k: usize| fermo + egui::vec2(100.0 + 3.0 * k as f32, 0.0);
        let mut visti = vec![tel.passo(v, s, vec![dito(egui::TouchPhase::Start, fermo), egui::Event::PointerMoved(fermo), tasto(true, fermo)]).gesto];
        visti.push(tel.passo(v, s, vec![uno(egui::TouchPhase::Start, altro(0))]).gesto);
        for k in 1..=frame {
            visti.push(tel.passo(v, s, vec![dito(egui::TouchPhase::Move, fermo), uno(egui::TouchPhase::Move, altro(k))]).gesto);
        }
        visti.push(tel.passo(v, s, vec![uno(egui::TouchPhase::End, altro(frame))]).gesto);
        visti.push(tel.passo(v, s, vec![dito(egui::TouchPhase::End, fermo), tasto(false, fermo), egui::Event::PointerGone]).gesto);
        visti.into_iter().flatten().collect()
    }

    #[test]
    fn un_pizzico_col_pollice_fermo_non_e_un_tocco_ne_una_pressione_lunga() {
        for frame in [20, 60] {
            let mut s = documento(&["Ciao mondo, questa è una prova"]);
            let mut v = Vista::default();
            let mut tel = Telefono::new();
            tel.calma(&mut v, &mut s);
            let visti = pizzico_col_pollice_fermo(&mut tel, &mut v, &mut s, egui::pos2(100.0, 12.0), frame);
            assert!(v.zoom() > 1.2, "il pizzico non ha ingrandito: {}", v.zoom());
            assert!(visti.is_empty(), "{frame} frame: {visti:?}");
            // Dopo il pizzico un tocco torna a essere un tocco.
            assert!(tel.tocca(&mut v, &mut s, egui::pos2(100.0, 12.0)).is_some());
        }
        // Anche nella vista Pagina.
        let mut s = crate::esempi::lungo(20);
        let mut v = Vista::default();
        v.cambia_modo(Modo::Pagina);
        let mut tel = Telefono::new();
        tel.calma(&mut v, &mut s);
        let visti = pizzico_col_pollice_fermo(&mut tel, &mut v, &mut s, egui::pos2(100.0, 200.0), 20);
        assert!(visti.is_empty(), "{visti:?}");
    }

    /// Il paragrafo sotto questo punto dello schermo, con un tocco.
    fn paragrafo_sotto(tel: &mut Telefono, v: &mut Vista, s: &mut Session, p: egui::Pos2) -> u32 {
        let t = tel.tocca(v, s, p).expect("tocco");
        let l = layout_scrittura(s, 412.0, v.zoom());
        l.hit(0, t.x_pt, t.y_pt, StoryRef::Body).expect("posizione").path.0[0]
    }

    #[test]
    fn il_pizzico_tiene_il_punto_del_documento_fra_le_dita() {
        let mut s = crate::esempi::lungo(200);
        let mut v = Vista::default();
        let mut tel = Telefono::new();
        // A metà del documento.
        s.run("select.range", &json!({ "anchor": Pos::body(110, 0), "focus": Pos::body(110, 0) })).unwrap();
        v.scorri_al_cursore();
        tel.calma(&mut v, &mut s);
        let centro = egui::pos2(206.0, 400.0);
        let prima = paragrafo_sotto(&mut tel, &mut v, &mut s, centro);
        assert!(prima > 90, "la vista non è a metà: {prima}");
        let due = |phase, d: f32| {
            vec![
                egui::Event::Touch { device_id: egui::TouchDeviceId(1), id: egui::TouchId(0), phase, pos: centro - egui::vec2(d, 0.0), force: None },
                egui::Event::PointerMoved(centro - egui::vec2(d, 0.0)),
                egui::Event::Touch { device_id: egui::TouchDeviceId(1), id: egui::TouchId(1), phase, pos: centro + egui::vec2(d, 0.0), force: None },
            ]
        };
        tel.passo(&mut v, &mut s, due(egui::TouchPhase::Start, 50.0));
        for k in 1..=5 {
            tel.passo(&mut v, &mut s, due(egui::TouchPhase::Move, 50.0 + 7.0 * k as f32));
        }
        tel.passo(&mut v, &mut s, due(egui::TouchPhase::End, 85.0));
        tel.calma(&mut v, &mut s);
        assert!(v.zoom() > 1.4, "{}", v.zoom());
        let dopo = paragrafo_sotto(&mut tel, &mut v, &mut s, centro);
        assert!(dopo.abs_diff(prima) <= 1, "prima il paragrafo {prima}, dopo il {dopo}");
    }

    #[test]
    fn lo_zoom_della_pagina_non_passa_alla_scrittura() {
        let mut s = crate::esempi::lungo(20);
        let mut v = Vista::default();
        let mut tel = Telefono::new();
        tel.calma(&mut v, &mut s);
        let larghezza = s.view.web_width;
        v.cambia_modo(Modo::Pagina);
        tel.calma(&mut v, &mut s);
        v.pizzica(2.5, false);
        assert_eq!(v.zoom(), 2.5);
        v.cambia_modo(Modo::Scrittura);
        tel.calma(&mut v, &mut s);
        assert_eq!(v.zoom(), 1.0);
        assert!((s.view.web_width - larghezza).abs() < 0.5, "{} invece di {larghezza}", s.view.web_width);
        // E tornando in Pagina c'è il suo.
        v.cambia_modo(Modo::Pagina);
        assert_eq!(v.zoom(), 2.5);
    }

    #[test]
    fn la_vista_pagina_mostra_piu_pagine_e_il_tocco_dice_quale() {
        let mut s = crate::esempi::lungo(60);
        let mut v = Vista::default();
        v.cambia_modo(Modo::Pagina);
        let mut tel = Telefono::new();
        let f = tel.calma(&mut v, &mut s);
        let l = layout_pagine(&mut s);
        assert!(l.pages.len() >= 2, "pagine: {}", l.pages.len());
        // Le pagine sulla carta, come i fogli del bozzetto 7: 16 dp dai lati (larghe 380), la
        // prima 14 dp sotto la cima, l'ombra sotto ognuna.
        let rettangoli = |colore: egui::Color32| -> Vec<egui::Rect> {
            f.forme
                .iter()
                .filter_map(|f| match &f.shape {
                    egui::Shape::Rect(r) if r.fill == colore && r.blur_width == 0.0 => Some(r.rect),
                    _ => None,
                })
                .collect()
        };
        assert!(rettangoli(tema::CARTA).iter().any(|r| r.width() >= 412.0 && r.height() >= 800.0), "manca il fondo carta");
        let fogli: Vec<egui::Rect> = rettangoli(egui::Color32::WHITE).into_iter().filter(|r| (r.width() - 380.0).abs() < 0.5).collect();
        assert!(!fogli.is_empty());
        assert_eq!((fogli[0].left(), fogli[0].top()), (MARGINE_PAGINE, CIMA_PAGINE));
        assert!(f.forme.iter().any(|f| matches!(&f.shape, egui::Shape::Rect(r) if r.blur_width > 0.0)), "pagine senza ombra");
        let scala = 380.0 / l.pages[0].w;
        let alto = l.pages[0].h * scala;
        assert!(alto < 800.0, "la prima pagina è alta {alto} dp");
        // Con la prima pagina più corta dello schermo si vede anche la seconda.
        assert!(fogli.len() >= 2, "fogli: {fogli:?}");
        assert!((fogli[1].min.y - (CIMA_PAGINE + alto + SPAZIO_PAGINE)).abs() < 0.5, "{fogli:?}");
        assert!(v.fasce_pagine.keys().any(|(k, _)| *k == 1), "fasce: {:?}", v.fasce_pagine.keys());
        // Un tocco sulla seconda pagina, 20 dp sotto la sua cima e 100 dal suo bordo.
        let p = egui::pos2(MARGINE_PAGINE + 100.0, fogli[1].min.y + 20.0);
        let giu = tel.passo(&mut v, &mut s, vec![dito(egui::TouchPhase::Start, p), egui::Event::PointerMoved(p), tasto(true, p)]);
        let su = tel.passo(&mut v, &mut s, vec![dito(egui::TouchPhase::End, p), tasto(false, p), egui::Event::PointerGone]);
        let g = giu.gesto.or(su.gesto);
        let Some(Gesto::ToccoPagina { pagina, punto }) = g else { panic!("{g:?}") };
        assert_eq!(pagina, 1);
        assert!((punto.x_pt - 100.0 / scala).abs() < 0.05 && (punto.y_pt - 20.0 / scala).abs() < 0.05, "{punto:?}");
        // Sulla carta fuori dalle pagine: il punto più vicino della pagina accanto.
        let fuori = egui::pos2(4.0, 6.0);
        let giu = tel.passo(&mut v, &mut s, vec![dito(egui::TouchPhase::Start, fuori), egui::Event::PointerMoved(fuori), tasto(true, fuori)]);
        let su = tel.passo(&mut v, &mut s, vec![dito(egui::TouchPhase::End, fuori), tasto(false, fuori), egui::Event::PointerGone]);
        let g = giu.gesto.or(su.gesto);
        let Some(Gesto::ToccoPagina { pagina, punto }) = g else { panic!("{g:?}") };
        assert_eq!((pagina, punto.x_pt, punto.y_pt), (0, 0.0, 0.0));
    }

    #[test]
    fn in_pagina_le_fasce_non_superano_la_texture_piu_grande() {
        let mut s = crate::esempi::lungo(60);
        let mut v = Vista::default();
        v.cambia_modo(Modo::Pagina);
        v.pizzica(ZOOM_MASSIMO, false);
        // Il telefono in orizzontale: 915 dp per 2,625 per 2,5 farebbero 6000 pixel.
        let mut tel = Telefono { dim: egui::vec2(915.0, 412.0), ..Telefono::new() };
        tel.calma(&mut v, &mut s);
        assert!(!v.fasce_pagine.is_empty());
        for (_, tex) in v.fasce_pagine.values() {
            assert!(tex.size()[0] <= 4096, "texture larga {} pixel", tex.size()[0]);
        }
    }

    #[test]
    fn tocco_mappato_sul_testo() {
        let mut s = documento(&["Ciao mondo, questa è una prova"]);
        let mut v = Vista::default();
        let mut tel = Telefono::new();
        tel.calma(&mut v, &mut s);
        // La vista parte dall'angolo in alto a sinistra: 100 × 60 dp sono 75 × 45 punti.
        let t = tel.tocca(&mut v, &mut s, egui::pos2(100.0, 60.0)).expect("tocco");
        assert!((t.x_pt - 75.0).abs() < 0.05 && (t.y_pt - 45.0).abs() < 0.05, "{t:?}");
        // A metà della riga, subito dopo «Ciao»: il motore ci trova la posizione 4.
        let c = layout_scrittura(&mut s, 412.0, 1.0).caret_on(&Pos::body(0, 4), 0).unwrap();
        let p = egui::pos2((c.x + 0.3) * DP_PER_PT, (c.top + c.height / 2.0) * DP_PER_PT);
        let t = tel.tocca(&mut v, &mut s, p).expect("tocco sul testo");
        let l = layout_scrittura(&mut s, 412.0, 1.0);
        assert_eq!(l.hit(0, t.x_pt, t.y_pt, StoryRef::Body), Some(Pos::body(0, 4)), "{t:?}");
    }

    #[test]
    fn niente_fascia_oltre_la_fine() {
        let mut s = crate::esempi::lungo(30);
        let mut v = Vista::default();
        let mut tel = Telefono::new();
        tel.calma(&mut v, &mut s);
        assert!(v.fasce.len() >= 2, "fasce a schermo {:?}", v.fasce.keys());
        // Tutto il testo diventa una parola: la pagina è più corta della prima fascia.
        s.run("select.all", &json!({})).unwrap();
        s.run("text.insert", &json!({ "text": "Corto", "raw": true })).unwrap();
        for _ in 0..3 {
            tel.passo(&mut v, &mut s, vec![]);
        }
        let h = layout_scrittura(&mut s, 412.0, 1.0).pages[0].h;
        let n_fasce = (h / tel.fascia_pt()).ceil().max(1.0) as usize;
        assert_eq!(n_fasce, 1, "pagina alta {h} pt");
        assert!(v.fasce.keys().all(|i| *i < n_fasce), "fasce tenute {:?} con {n_fasce} fasce", v.fasce.keys());
    }

    /// Documento lungo col cursore su una riga in basso, a schermo poco sopra il bordo (come dopo
    /// un tocco lì).
    fn cursore_in_basso() -> (Session, Vista, Telefono) {
        let mut s = crate::esempi::lungo(30);
        let mut v = Vista::default();
        let mut tel = Telefono::new();
        tel.calma(&mut v, &mut s);
        let l = layout_scrittura(&mut s, 412.0, 1.0);
        let pos = l.hit(0, 20.0, 740.0 / DP_PER_PT, StoryRef::Body).unwrap();
        s.run("caret.set", &json!({ "pos": pos })).unwrap();
        v.scorri_al_cursore();
        let c = tel.calma(&mut v, &mut s).cursore().unwrap();
        assert!(c.min.y > 500.0 && c.max.y <= 800.0, "cursore {c:?}");
        (s, v, tel)
    }

    #[test]
    fn cursore_visibile_quando_la_tastiera_si_apre() {
        let (mut s, mut v, mut tel) = cursore_in_basso();
        // Si apre la tastiera: la vista si accorcia di 350 dp, qualche frame dopo il tocco.
        tel.dim.y = 450.0;
        let c = tel.calma(&mut v, &mut s).cursore().unwrap();
        assert!(c.min.y >= 0.0 && c.max.y <= 450.0, "cursore {c:?} fuori dalla vista alta 450 dp");
    }

    #[test]
    fn il_cursore_resta_sopra_la_parte_coperta_in_fondo() {
        // La barra di formato copre gli ultimi 72 dp della vista: il cursore che ci va sotto
        // torna sopra, anche all'ultima riga del documento.
        let (mut s, mut v, mut tel) = cursore_in_basso();
        v.coperto_sotto = 72.0;
        v.scorri_al_cursore();
        let c = tel.calma(&mut v, &mut s).cursore().unwrap();
        assert!(c.min.y >= 0.0 && c.max.y <= 800.0 - 72.0, "cursore {c:?} sotto la parte coperta");
        s.run("caret.set", &json!({ "pos": Pos::body(s.doc.body.len() - 1, 0) })).unwrap();
        v.scorri_al_cursore();
        let c = tel.calma(&mut v, &mut s).cursore().unwrap();
        assert!(c.max.y <= 800.0 - 72.0, "ultima riga {c:?} sotto la parte coperta");
        // Anche quando la vista si accorcia (la tastiera): segue il cursore che era sopra la barra.
        tel.dim.y = 450.0;
        let c = tel.calma(&mut v, &mut s).cursore().unwrap();
        assert!(c.max.y <= 450.0 - 72.0, "cursore {c:?} con la vista alta 450 dp");
    }

    #[test]
    fn la_barra_della_selezione_non_va_nella_parte_coperta() {
        // Una selezione in cima: la barra va sotto, ma mai dentro gli ultimi 300 dp coperti.
        let mut s = documento(&["Ciao mondo, questa è una prova"]);
        s.run("select.range", &json!({ "anchor": Pos::body(0, 5), "focus": Pos::body(0, 10) })).unwrap();
        let mut v = Vista { coperto_sotto: 300.0, ..Vista::default() };
        let mut tel = Telefono { dim: egui::vec2(412.0, 400.0), ..Telefono::new() };
        let f = tel.calma(&mut v, &mut s);
        let barra = f
            .forme
            .iter()
            .find_map(|f| match &f.shape {
                egui::Shape::Rect(r) if r.fill == tema::SCURO => Some(r.rect),
                _ => None,
            })
            .expect("la barra della selezione");
        assert!(barra.max.y <= 100.0, "{barra:?}");
    }

    #[test]
    fn la_barra_della_selezione_resta_dentro_la_vista() {
        // La vista in una pagina larga 392 dp a 10 dal bordo, come nell'app: la barra, larga, sta
        // a 8 dp dai bordi della pagina e non dello schermo.
        let mut s = documento(&["Ciao mondo, questa è una prova"]);
        s.run("select.range", &json!({ "anchor": Pos::body(0, 0), "focus": Pos::body(0, 4) })).unwrap();
        let mut v = Vista::default();
        let ctx = egui::Context::default();
        let pagina = egui::Rect::from_min_max(egui::pos2(10.0, 64.0), egui::pos2(402.0, 800.0));
        let mut barra = None;
        // Qualche frame col tempo che avanza: la barra compare in dissolvenza.
        for k in 0..10 {
            let ri = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(412.0, 800.0))),
                time: Some(f64::from(k) * 0.1),
                ..Default::default()
            };
            let mut out = ctx.run_ui(ri, |ui| {
                ui.scope_builder(egui::UiBuilder::new().max_rect(pagina), |ui| v.mostra(ui, &mut s, None, Instant::now()));
            });
            out.textures_delta.clear();
            barra = out.shapes.iter().find_map(|c| match &c.shape {
                egui::Shape::Rect(r) if r.fill == tema::SCURO => Some(r.rect),
                _ => None,
            });
        }
        let barra = barra.expect("la barra della selezione");
        assert!(barra.left() >= pagina.left() + 8.0 - 0.01 && barra.right() <= pagina.right() - 8.0 + 0.01, "{barra:?}");
    }

    #[test]
    fn senza_carta_la_pagina_la_dipinge_l_app() {
        let mut s = documento(&["Ciao"]);
        let mut v = Vista { dipingi_carta: false, ..Vista::default() };
        let f = Telefono::new().calma(&mut v, &mut s);
        assert!(f.carta().is_none(), "la vista ha dipinto la carta");
        assert!(f.cursore().is_some());
        assert_eq!(colore_carta(&s), egui::Color32::WHITE);
        s.run("design.pageColor", &json!({ "color": "FFF2CC" })).unwrap();
        assert_eq!(colore_carta(&s), egui::Color32::from_rgb(0xFF, 0xF2, 0xCC));
    }

    #[test]
    fn le_misure_vanno_nella_diagnosi() {
        let mut s = documento(&["Ciao"]);
        let mut v = Vista::default();
        Telefono::new().calma(&mut v, &mut s);
        let d = tascabile_platform::diagnosi_testo();
        assert!(d.contains("] pagina: prima fascia dopo ") && d.contains(" ms dall'apertura (layout "), "{d}");
        let riga = v.riga_misure();
        assert!(riga.starts_with("misure: layout ") && riga.contains(" ms, lista ") && riga.contains(" liste"), "{riga}");
    }

    #[test]
    fn cursore_visibile_con_la_tastiera_che_sale_piano() {
        let (mut s, mut v, mut tel) = cursore_in_basso();
        // La tastiera sale di 20 dp a frame, più in fretta dell'animazione di scorrimento.
        for _ in 0..15 {
            tel.dim.y -= 20.0;
            tel.passo(&mut v, &mut s, vec![]);
        }
        let c = tel.calma(&mut v, &mut s).cursore().unwrap();
        assert!(c.min.y >= 0.0 && c.max.y <= 500.0, "cursore {c:?} fuori dalla vista alta 500 dp");
    }

    #[test]
    fn cursore_visibile_con_la_tastiera_animata_e_un_frame_fermo() {
        // Come Android: la tastiera sale di 300 dp in 17 frame, rallentando, e a metà animazione un
        // frame non porta un'altezza nuova (l'altezza arriva dal thread Java, non a ogni frame).
        let curva = |x: f32| 1.0 - (1.0 - x).powi(3);
        for fermo in [4usize, 5, 6] {
            let (mut s, mut v, mut tel) = cursore_in_basso();
            let alto = tel.dim.y;
            let (mut k, mut passi) = (0usize, 0usize);
            while k < 17 {
                passi += 1;
                if passi != fermo {
                    k += 1;
                    tel.dim.y = alto - 48.0f32.max(300.0 * curva(k as f32 / 17.0));
                }
                tel.passo(&mut v, &mut s, vec![]);
            }
            let c = tel.calma(&mut v, &mut s).cursore().unwrap();
            assert!(c.min.y >= 0.0 && c.max.y <= tel.dim.y, "frame fermo al passo {fermo}: cursore {c:?}, vista alta {}", tel.dim.y);
        }
    }

    #[test]
    fn vista_che_si_accorcia_col_cursore_lontano() {
        // Gli esempi lasciano il cursore in fondo, e all'avvio la vista si accorcia appena le
        // barre prendono la loro misura: la pagina resta in cima e non salta al cursore.
        let mut s = crate::esempi::lungo(30);
        let mut v = Vista::default();
        let mut tel = Telefono::new();
        tel.passo(&mut v, &mut s, vec![]);
        tel.dim.y = 770.0;
        let cima = tel.calma(&mut v, &mut s).carta().unwrap().rect.min.y;
        assert_eq!(cima, 0.0);
    }

    #[test]
    fn un_frame_fermo_non_carica_texture() {
        let mut s = crate::esempi::lungo(30);
        let mut v = Vista::default();
        let mut tel = Telefono::new();
        let primo = tel.passo(&mut v, &mut s, vec![]);
        assert!(!primo.caricate.is_empty(), "il primo frame carica le fasce");
        tel.calma(&mut v, &mut s);
        let misure = v.misure;
        let f = tel.passo(&mut v, &mut s, vec![]);
        assert!(f.caricate.is_empty(), "texture caricate in un frame fermo: {:?}", f.caricate);
        assert_eq!(v.misure, misure, "misure riscritte senza un layout nuovo");
    }

    /// Un documento con «parola» appena battuta, la vista in `modo` ferma, e poi il cursore che
    /// lascia la parola: la revisione resta quella, ma la parola in scrittura è nella chiave della
    /// cache del motore e `Session::layout()` dà un layout nuovo. Restituisce il frame dopo e
    /// quante liste ha fatto la vista prima.
    fn cursore_fuori_dalla_parola(modo: Modo) -> (Session, Vista, Telefono, Frame, u64) {
        let mut s = documento(&["Ciao "]);
        s.run("text.insert", &json!({ "text": "parola", "raw": true })).unwrap();
        let mut v = Vista::default();
        v.cambia_modo(modo);
        let mut tel = Telefono::new();
        tel.calma(&mut v, &mut s);
        // La vista ha già messo la sua larghezza: qui il layout arriva dalla cache.
        let prima = s.layout();
        let (rev, liste) = (s.rev(), v.misure.liste);
        assert!(liste > 0, "nessuna lista fatta");
        s.run("caret.set", &json!({ "pos": Pos::body(0, 0) })).unwrap();
        let f = tel.passo(&mut v, &mut s, vec![]);
        assert_eq!(s.rev(), rev, "caret.set ha cambiato la revisione");
        assert!(!Arc::ptr_eq(&prima, &s.layout()), "il motore ha dato lo stesso layout");
        (s, v, tel, f, liste)
    }

    #[test]
    fn la_lista_si_rifa_quando_il_layout_cambia_senza_revisione() {
        let (mut s, mut v, mut tel, f, liste) = cursore_fuori_dalla_parola(Modo::Scrittura);
        assert_eq!(v.misure.liste, liste + 1, "lista non rifatta col layout nuovo");
        assert!(!f.caricate.is_empty(), "fasce non rifatte col layout nuovo");
        // Due frame fermi: nessuna lista nuova (nessun `layout()` con un'altra vista nel frame).
        for _ in 0..2 {
            let f = tel.passo(&mut v, &mut s, vec![]);
            assert!(f.caricate.is_empty(), "texture caricate in un frame fermo: {:?}", f.caricate);
        }
        assert_eq!(v.misure.liste, liste + 1, "lista rifatta senza un layout nuovo");
    }

    #[test]
    fn in_pagina_le_liste_si_rifanno_quando_il_layout_cambia_senza_revisione() {
        let (mut s, mut v, mut tel, f, liste) = cursore_fuori_dalla_parola(Modo::Pagina);
        assert_eq!(v.misure.liste, liste * 2, "liste delle pagine non rifatte col layout nuovo");
        assert!(!f.caricate.is_empty(), "fasce non rifatte col layout nuovo");
        for _ in 0..2 {
            let f = tel.passo(&mut v, &mut s, vec![]);
            assert!(f.caricate.is_empty(), "texture caricate in un frame fermo: {:?}", f.caricate);
        }
        assert_eq!(v.misure.liste, liste * 2, "liste rifatte senza un layout nuovo");
    }

    #[test]
    fn texture_alla_sua_misura_e_sui_pixel() {
        let mut s = crate::esempi::lungo(30);
        let mut v = Vista::default();
        let mut tel = Telefono::new();
        tel.calma(&mut v, &mut s);
        // In fondo al documento: lo scorrimento si ferma a una frazione di pixel qualsiasi.
        s.run("caret.set", &json!({ "pos": Pos::body(s.doc.body.len() - 1, 0) })).unwrap();
        v.scorri_al_cursore();
        let f = tel.calma(&mut v, &mut s);
        let mut viste = 0;
        for (_, tex) in v.fasce.values() {
            let Some(dest) = f.immagine(tex.id()) else { continue };
            viste += 1;
            let px = dest.size() * tel.ppp;
            assert!((px - tex.size_vec2()).length() < 0.01, "disegnata a {px:?} pixel, texture da {:?}", tex.size_vec2());
            let angolo = dest.min.to_vec2() * tel.ppp;
            assert!((angolo - angolo.round()).length() < 0.01, "angolo a {angolo:?} pixel");
        }
        assert!(viste >= 2, "fasce disegnate: {viste}");
    }

    #[test]
    fn ogni_vista_parte_dall_alto() {
        let mut tel = Telefono::new();
        let mut s = crate::esempi::lungo(30);
        let mut v = Vista::default();
        tel.calma(&mut v, &mut s);
        s.run("caret.set", &json!({ "pos": Pos::body(s.doc.body.len() - 1, 0) })).unwrap();
        v.scorri_al_cursore();
        let cima = tel.calma(&mut v, &mut s).carta().unwrap().rect.min.y;
        assert!(cima < -1000.0, "il primo documento non è sceso: cima a {cima}");
        // Un documento nuovo nella stessa finestra parte dalla cima, non da dove era l'altro.
        let mut s2 = crate::esempi::lungo(30);
        let mut v2 = Vista::default();
        let cima = tel.passo(&mut v2, &mut s2, vec![]).carta().unwrap().rect.min.y;
        assert_eq!(cima, 0.0);
    }

    #[test]
    fn il_primo_trascinamento_scorre() {
        let mut s = crate::esempi::lungo(30);
        let mut v = Vista::default();
        let mut tel = Telefono::new();
        tel.calma(&mut v, &mut s);
        // Il primo dito dopo l'avvio, senza tocchi prima: la pagina sale e il testo non si tocca.
        let tocco = tel.trascina(&mut v, &mut s, egui::pos2(200.0, 600.0), -300.0);
        let cima = tel.calma(&mut v, &mut s).carta().unwrap().rect.min.y;
        assert_eq!(tocco, None);
        assert!(cima < -250.0, "pagina ferma: cima a {cima}");
        // Un tocco fermo resta un tocco, anche con lo scorrimento col dito sempre acceso.
        let t = tel.tocca(&mut v, &mut s, egui::pos2(100.0, 400.0)).expect("tocco dopo lo scorrimento");
        assert!((t.y_pt - (400.0 - cima) / DP_PER_PT).abs() < 0.05, "{t:?} con la cima a {cima}");
    }

    #[test]
    fn tocco_sotto_il_testo() {
        let mut s = documento(&["Ciao"]);
        let mut v = Vista::default();
        let mut tel = Telefono::new();
        let f = tel.calma(&mut v, &mut s);
        // La carta riempie lo schermo anche con un documento corto.
        assert_eq!(f.carta().unwrap().rect.height(), 800.0);
        let h = layout_scrittura(&mut s, 412.0, 1.0).pages[0].h;
        let t = tel.tocca(&mut v, &mut s, egui::pos2(200.0, 600.0)).expect("tocco sotto il testo");
        assert!(t.y_pt > h, "{t:?} con la pagina alta {h} pt");
        // Il motore porta il cursore nel punto più vicino: la fine di «Ciao».
        let l = layout_scrittura(&mut s, 412.0, 1.0);
        assert_eq!(l.hit(0, t.x_pt, t.y_pt, StoryRef::Body), Some(Pos::body(0, 4)));
    }

    #[test]
    fn piu_stretto_vuol_dire_piu_alto() {
        let testo = "parola ".repeat(200);
        let mut s = documento(&[testo.as_str()]);
        let stretto = layout_scrittura(&mut s, 412.0, 1.0);
        let (w1, h1) = (stretto.pages[0].w, stretto.pages[0].h);
        let largo = layout_scrittura(&mut s, 1200.0, 1.0);
        assert_eq!(stretto.pages.len(), 1);
        assert!((w1 - 412.0 / DP_PER_PT).abs() < 0.5, "larghezza {w1}");
        assert!(h1 > largo.pages[0].h * 1.5, "{h1} contro {}", largo.pages[0].h);
    }

    #[test]
    fn una_fascia_ha_del_testo() {
        let mut s = documento(&["Ciao mondo, questa è una prova"]);
        let l = layout_scrittura(&mut s, 412.0, 1.0);
        let img = rendi_fascia(&s, &l, 0, 100.0, 2.0).unwrap();
        assert!((img.width as f32 - l.pages[0].w * 2.0).abs() <= 1.0);
        // Scuro e coprente: un pixel trasparente (fuori dal testo) ha anche lui il rosso a zero.
        assert!(img.pixels.chunks(4).any(|p| p[3] > 128 && p[0] < 128), "nessun pixel scuro: niente testo");
        assert!(rendi_fascia(&s, &l, 1000, 100.0, 2.0).is_none());
    }

    #[test]
    fn fascia_dalla_lista_come_quella_intera() {
        let mut s = crate::esempi::checklist();
        let l = layout_scrittura(&mut s, 412.0, 1.0);
        let pagina = &l.pages[0];
        let lista = page_display(&s.doc, pagina, &RenderOptions::default().display);
        let (fascia_pt, px_per_pt) = (150.0, 3.5);
        for i in 0..2 {
            let dalla_lista = rendi_fascia_da(&s, pagina, &lista, i, fascia_pt, px_per_pt).unwrap();
            let y = i as f32 * fascia_pt;
            let intera = render_area(&s.doc, pagina, 0.0, y, pagina.w, fascia_pt.min(pagina.h - y), px_per_pt, &RenderOptions::default());
            assert_eq!((dalla_lista.width, dalla_lista.height), (intera.width, intera.height));
            // Fuori dal testo la fascia dalla lista è trasparente: sulla carta bianca è la stessa.
            let sulla_carta = |p: &[u8], k: usize| p[k] as i32 + 255 - p[3] as i32;
            let diff = dalla_lista
                .pixels
                .chunks(4)
                .zip(intera.pixels.chunks(4))
                .map(|(a, b)| (0..3).map(|k| (sulla_carta(a, k) - b[k] as i32).abs()).max().unwrap_or(0))
                .max()
                .unwrap_or(0);
            assert!(diff <= 2, "fascia {i}: differenza massima {diff}");
        }
        assert!(rendi_fascia_da(&s, pagina, &lista, 1000, fascia_pt, px_per_pt).is_none());
    }

    #[test]
    fn la_carta_ha_il_colore_della_pagina() {
        let mut s = crate::esempi::checklist();
        s.run("design.pageColor", &json!({ "color": "FFF2CC" })).unwrap();
        let mut v = Vista::default();
        let f = Telefono::new().calma(&mut v, &mut s);
        assert_eq!(f.carta().map(|r| r.fill), Some(egui::Color32::from_rgb(0xFF, 0xF2, 0xCC)));
    }

    #[test]
    fn prima_le_fasce_del_cursore() {
        assert_eq!(ordine_fasce(0, 3, None, 100.0), [0, 1, 2, 3]);
        assert_eq!(ordine_fasce(0, 3, Some((250.0, 270.0)), 100.0), [2, 0, 1, 3]);
        // Riga a cavallo di due fasce: per prime tutte e due.
        assert_eq!(ordine_fasce(0, 3, Some((290.0, 310.0)), 100.0), [2, 3, 0, 1]);
        // Cursore fuori dallo schermo: l'ordine di sempre.
        assert_eq!(ordine_fasce(1, 2, Some((50.0, 70.0)), 100.0), [1, 2]);
    }

    #[test]
    fn la_fascia_del_cursore_si_rifa_subito() {
        let mut s = crate::esempi::lungo(30);
        let mut v = Vista::default();
        // A 4 pixel per punto le fasce sono alte 256 dp: quattro a schermo.
        let mut tel = Telefono { ppp: 4.0, ..Telefono::new() };
        tel.calma(&mut v, &mut s);
        let l = layout_scrittura(&mut s, 412.0, 1.0);
        let pos = l.hit(0, 20.0, 600.0 / DP_PER_PT, StoryRef::Body).unwrap();
        s.run("caret.set", &json!({ "pos": pos })).unwrap();
        tel.calma(&mut v, &mut s);
        let fascia = (l.caret_on(&pos, 0).unwrap().top / tel.fascia_pt()).floor() as usize;
        assert!(fascia >= NUOVE_PER_FRAME, "il cursore deve stare dopo le prime fasce: fascia {fascia}");
        assert!(v.fasce.len() > NUOVE_PER_FRAME, "fasce a schermo {:?}", v.fasce.keys());
        // Una lettera: già nel primo frame la fascia del cursore è fra quelle rifatte.
        s.run("text.insert", &json!({ "text": "x", "raw": true })).unwrap();
        let f = tel.passo(&mut v, &mut s, vec![]);
        assert!(f.caricate.contains(&v.fasce[&fascia].1.id()), "fascia {fascia} lasciata al frame dopo");
        assert!(v.misure.lista_ms > 0.0, "{:?}", v.misure);
    }

    /// «Sopra», poi una tabella di 3 righe e 3 colonne larga quanto il testo col cursore nella
    /// prima cella, poi tanti paragrafi (la pagina scorre).
    /// Il riquadro dell'immagine scelta: il rettangolo blu del contorno (largo più di una goccia).
    fn riquadro(f: &Frame) -> Option<egui::Rect> {
        f.forme.iter().find_map(|f| match &f.shape {
            egui::Shape::Rect(r) if r.stroke.color == tema::BLU && r.stroke.width == tascabile_touch::RIQUADRO => Some(r.rect),
            _ => None,
        })
    }

    #[test]
    fn l_immagine_scelta_ha_il_riquadro_e_la_maniglia_che_ne_cambia_la_misura() {
        let mut e = crate::immagine::test::con_immagine();
        let (mut v, mut tel) = (Vista::default(), Telefono::new());
        let f = tel.calma(&mut v, &mut e.s);
        assert!((v.larghezza_del_testo() - (412.0 / DP_PER_PT - 36.0)).abs() < 0.01, "{}", v.larghezza_del_testo());
        let l = layout_scrittura(&mut e.s, 412.0, 1.0);
        let o = l.find_object(0, |o| o.text_box.is_none()).unwrap().rect;
        let schermo = egui::Rect::from_min_size(egui::pos2(o.x, o.y) * DP_PER_PT, egui::vec2(o.w, o.h) * DP_PER_PT);
        let r = riquadro(&f).expect("il riquadro dell'immagine");
        assert!((r.min - schermo.min).length() < 0.5 && (r.max - schermo.max).length() < 0.5, "{r:?} {schermo:?}");
        // Niente maniglie della selezione né barra Taglia, Copia.
        assert_eq!(f.gocce(), 0);
        assert!(!f.testi().iter().any(|t| t == "Copia"), "{:?}", f.testi());
        // La maniglia tirata in diagonale verso l'alto a sinistra di un quarto: la misura nuova.
        let presa = schermo.max;
        let mut gesti =
            vec![tel.passo(&mut v, &mut e.s, vec![dito(egui::TouchPhase::Start, presa), egui::Event::PointerMoved(presa), tasto(true, presa)]).gesto];
        let verso = -schermo.size() / 4.0;
        for k in 1..=8 {
            let p = presa + verso * (k as f32 / 8.0);
            let f = tel.passo(&mut v, &mut e.s, vec![dito(egui::TouchPhase::Move, p), egui::Event::PointerMoved(p)]);
            gesti.push(f.gesto);
            if k == 8 {
                let r = riquadro(&f).unwrap();
                assert!((r.width() - schermo.width() * 0.75).abs() < 0.5, "il riquadro segue il dito: {r:?}");
            }
        }
        let fine = presa + verso;
        gesti.push(tel.passo(&mut v, &mut e.s, vec![dito(egui::TouchPhase::End, fine), tasto(false, fine), egui::Event::PointerGone]).gesto);
        let gesti: Vec<Gesto> = gesti.into_iter().flatten().collect();
        let [Gesto::MisuraImmagine(w)] = gesti.as_slice() else { panic!("{gesti:?}") };
        assert!((w - o.w * 0.75).abs() < 0.5, "{w} invece di {}", o.w * 0.75);
        // L'editor la fa larga così, e la pagina non è scorsa.
        e.gesto(Gesto::MisuraImmagine(*w));
        let (_, info) = crate::immagine::immagine_scelta(&e.s).unwrap();
        assert!((info.larghezza - o.w * 0.75).abs() < 0.5, "{info:?}");
        assert_eq!(v.scorrimento, egui::Vec2::ZERO);
        // Il cursore altrove: niente riquadro.
        e.s.run("caret.set", &json!({ "pos": Pos::body(2, 1) })).unwrap();
        assert!(riquadro(&tel.calma(&mut v, &mut e.s)).is_none());
    }

    fn con_tabella() -> Session {
        let mut righe = vec!["Sopra".to_string()];
        righe.extend((1..=40).map(|i| format!("Paragrafo {i}")));
        let righe: Vec<&str> = righe.iter().map(String::as_str).collect();
        let mut s = documento(&righe);
        s.run("caret.set", &json!({ "pos": Pos::body(0, 5) })).unwrap();
        s.run("insert.table", &json!({ "rows": 3, "cols": 3 })).unwrap();
        s.run("table.autofit", &json!({ "mode": "window" })).unwrap();
        s
    }

    /// Il centro della cella (`riga`, `cella`) sullo schermo, con la vista non scorsa e lo zoom 1.
    fn centro_cella(s: &mut Session, riga: usize, cella: usize) -> egui::Pos2 {
        let l = layout_scrittura(s, 412.0, 1.0);
        let r = l.pages[0]
            .items
            .iter()
            .find_map(|it| match it {
                Placed::Cell { rect, row, cell, covered: false, .. } if *row == riga && *cell == cella => Some(*rect),
                _ => None,
            })
            .expect("la cella");
        egui::pos2((r.x + r.w / 2.0) * DP_PER_PT, (r.y + r.h / 2.0) * DP_PER_PT)
    }

    /// Il dito da `da` ad `a` in dieci frame, poi su: i gesti visti.
    fn trascina_fino(tel: &mut Telefono, v: &mut Vista, s: &mut Session, da: egui::Pos2, a: egui::Pos2) -> Vec<Gesto> {
        let mut visti = Vec::new();
        visti.extend(tel.passo(v, s, vec![dito(egui::TouchPhase::Start, da), egui::Event::PointerMoved(da), tasto(true, da)]).gesto);
        for k in 1..=10 {
            let p = da + (a - da) * (k as f32 / 10.0);
            visti.extend(tel.passo(v, s, vec![dito(egui::TouchPhase::Move, p), egui::Event::PointerMoved(p)]).gesto);
        }
        visti.extend(tel.passo(v, s, vec![dito(egui::TouchPhase::End, a), tasto(false, a), egui::Event::PointerGone]).gesto);
        visti
    }

    #[test]
    fn il_dito_trascinato_dalla_cella_del_cursore_sceglie_le_celle() {
        let mut s = con_tabella();
        let mut v = Vista::default();
        let mut tel = Telefono::new();
        tel.calma(&mut v, &mut s);
        let (da, a) = (centro_cella(&mut s, 0, 0), centro_cella(&mut s, 1, 1));
        assert_eq!(v.sullo_schermo(da.x / DP_PER_PT, da.y / DP_PER_PT), da, "da punti della pagina allo schermo");
        let visti = trascina_fino(&mut tel, &mut v, &mut s, da, a);
        let celle: Vec<Tocco> = visti.iter().filter_map(|g| if let Gesto::Celle(t) = g { Some(*t) } else { None }).collect();
        let ultimo = celle.last().unwrap_or_else(|| panic!("nessun gesto delle celle: {visti:?}"));
        assert!((ultimo.x_pt - a.x / DP_PER_PT).abs() < 0.1 && (ultimo.y_pt - a.y / DP_PER_PT).abs() < 0.1, "{ultimo:?}");
        assert_eq!(visti.last(), Some(&Gesto::CelleLasciate), "{visti:?}");
        assert!(!visti.iter().any(|g| matches!(g, Gesto::Tocco(_))), "{visti:?}");
        assert_eq!(tel.calma(&mut v, &mut s).carta().unwrap().rect.min.y, 0.0, "la pagina non è scorsa");
        // Un tocco fermo nella cella del cursore resta un tocco, e la pressione lunga la sua.
        assert!(tel.tocca(&mut v, &mut s, da).is_some());
        tel.t += 1.0;
        let mut visti =
            vec![tel.passo(&mut v, &mut s, vec![dito(egui::TouchPhase::Start, da), egui::Event::PointerMoved(da), tasto(true, da)]).gesto];
        visti.extend((0..60).map(|_| tel.passo(&mut v, &mut s, vec![]).gesto));
        visti.push(tel.passo(&mut v, &mut s, vec![dito(egui::TouchPhase::End, da), tasto(false, da), egui::Event::PointerGone]).gesto);
        let visti: Vec<Gesto> = visti.into_iter().flatten().collect();
        assert!(matches!(visti.as_slice(), [Gesto::PressioneLunga(_)]), "{visti:?}");
        // Il dito da un'altra cella fa scorrere la pagina, come fuori dalla tabella.
        let altra = centro_cella(&mut s, 2, 2);
        let visti = trascina_fino(&mut tel, &mut v, &mut s, altra, altra - egui::vec2(0.0, 300.0));
        assert!(!visti.iter().any(|g| matches!(g, Gesto::Celle(_) | Gesto::CelleLasciate)), "{visti:?}");
        assert!(tel.calma(&mut v, &mut s).carta().unwrap().rect.min.y < -100.0, "la pagina non è scorsa");
    }

    #[test]
    fn le_celle_scelte_si_vedono_intere() {
        let mut s = con_tabella();
        let l = layout_scrittura(&mut s, 412.0, 1.0);
        let cella = |r: usize, c: usize| {
            let mut p = s.sel.focus.path.0.clone();
            let n = p.len();
            (p[n - 3], p[n - 2]) = (r as u32, c as u32);
            Pos { story: StoryRef::Body, path: wordcraft_engine::doc::Path(p), off: 0 }
        };
        let (a, b) = (cella(0, 1), cella(1, 1));
        s.run("caret.set", &json!({ "pos": a })).unwrap();
        s.run("caret.set", &json!({ "pos": b, "extend": true })).unwrap();
        assert!(s.cell_range().is_some_and(|c| !c.stream), "una parte della colonna");
        let mut v = Vista::default();
        let f = Telefono::new().calma(&mut v, &mut s);
        let attese: Vec<egui::Rect> = l.pages[0]
            .items
            .iter()
            .filter_map(|it| match it {
                Placed::Cell { rect, row, cell: 1, covered: false, .. } if *row < 2 => {
                    Some(egui::Rect::from_min_size(egui::pos2(rect.x, rect.y) * DP_PER_PT, egui::vec2(rect.w, rect.h) * DP_PER_PT))
                }
                _ => None,
            })
            .collect();
        let viste: Vec<egui::Rect> = f
            .forme
            .iter()
            .filter_map(|f| match &f.shape {
                egui::Shape::Rect(r) if r.fill == SELEZIONE => Some(r.rect),
                _ => None,
            })
            .collect();
        assert_eq!(viste.len(), 2, "{viste:?}");
        for (vista, attesa) in viste.iter().zip(&attese) {
            assert!((vista.min - attesa.min).length() < 0.5 && (vista.max - attesa.max).length() < 0.5, "{vista:?} invece di {attesa:?}");
        }
    }

    #[test]
    fn una_tabella_piu_larga_dello_schermo_si_vede_scorrendo_di_lato() {
        let mut s = con_tabella();
        // Una tabella di un file, larga 700 punti: il motore le lascia la sua larghezza.
        s.run("table.properties", &json!({ "width": 700.0 })).unwrap();
        let l = layout_scrittura(&mut s, 412.0, 1.0);
        let pagina = &l.pages[0];
        let destra = larghezza_del_contenuto(pagina);
        assert!(destra > 700.0 && pagina.w < 320.0, "contenuto {destra}, pagina {}", pagina.w);
        assert_eq!(larghezza_del_contenuto(&layout_scrittura(&mut documento(&["Ciao"]), 412.0, 1.0).pages[0]), pagina.w);
        let mut v = Vista::default();
        let mut tel = Telefono::new();
        let f = tel.calma(&mut v, &mut s);
        let larga = v.fasce.values().filter_map(|(_, tex)| f.immagine(tex.id())).map(|r| r.width()).fold(0.0f32, f32::max);
        assert!((larga - destra * DP_PER_PT).abs() < 1.0, "fasce larghe {larga} dp, contenuto {}", destra * DP_PER_PT);
        // Il dito verso sinistra porta in vista la destra della tabella.
        let da = egui::pos2(300.0, 600.0);
        trascina_fino(&mut tel, &mut v, &mut s, da, da - egui::vec2(300.0, 0.0));
        let sinistra = tel.calma(&mut v, &mut s).carta().unwrap().rect.min.x;
        assert!(sinistra < -250.0, "la pagina non è scorsa di lato: {sinistra}");
    }
}
