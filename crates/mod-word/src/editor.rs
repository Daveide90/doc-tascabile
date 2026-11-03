//! Un documento aperto: sessione del motore, vista e ponte tastiera insieme.

use std::time::Instant;

use serde_json::{Value, json};
use tascabile_platform::{ImeSnapshot, TastoCrudo, diagnosi};
use tascabile_touch::Maniglia;
use wordcraft_engine::Session;
use wordcraft_engine::doc::props::{Align, Rgb, TextColor};
use wordcraft_engine::doc::{Block, Cell, CharProps, Document, Pos, StoryRef, Table};

use crate::ponte::{Ponte, StatoIme, genere};
use crate::salva::{Origine, come_si_legge, ora_locale};
use crate::vista::{Azione, Gesto, Modo, SceltaOrtografia, Tocco, Vista};

/// Quello che l'Editor chiede all'app dopo un frame.
#[derive(Debug, Default, PartialEq)]
pub struct Esito {
    /// Lo stato da dare alla tastiera.
    pub stato: Option<StatoIme>,
    /// Un tocco sul testo: la tastiera si apre.
    pub apri_tastiera: bool,
    /// Gli appunti di sistema, che ha l'app.
    pub appunti: Option<Appunti>,
    /// «Commenta» sulla selezione: l'app apre il foglio del commento.
    pub commenta: bool,
}

/// Che cosa fare con gli appunti di sistema.
#[derive(Debug, PartialEq)]
pub enum Appunti {
    /// Copiare questo testo (da Copia o Taglia, già fatti nel documento).
    Copia(String),
    /// Leggere gli appunti e darli a `Editor::incolla`.
    Incolla,
}

/// Il testo semplice della selezione, come si legge: senza il testo cancellato con le revisioni e
/// senza quello nascosto (anche da uno stile), che negli appunti di un'altra app riapparirebbe,
/// per esempio un nome tolto da un documento anonimizzato. Un paragrafo per riga, le celle di
/// una riga di tabella separate da una tabulazione, come `Document::plain_text` del motore. Con
/// delle celle scelte, solo il loro rettangolo: fra i due capi della selezione ci sono anche le
/// celle delle righe di mezzo fuori dal rettangolo (analisi del 27/10, word I3).
pub fn testo_della_selezione(s: &Session) -> String {
    let (a, b) = s.sel.ordered();
    if a == b {
        return String::new();
    }
    if let Some(cr) = s.cell_range()
        && let Some(t) = s.doc.table(cr.story, &cr.table)
    {
        return testo_delle_celle(&s.doc, t, cr.rows, cr.cols);
    }
    let pezzo = s.doc.copy_range(&a, &b);
    let mut out = String::new();
    testo_dei_blocchi(&s.doc, &mut pezzo.blocks.iter(), &mut out, 0);
    if out.ends_with('\n') {
        out.pop();
    }
    out
}

/// Il testo delle celle delle righe `righe` che toccano le colonne della griglia `colonne` (estremi
/// compresi, come `CellRange`): una riga di tabella per riga, le celle separate da tabulazioni.
fn testo_delle_celle(doc: &Document, t: &Table, righe: (usize, usize), colonne: (usize, usize)) -> String {
    let mut out = Vec::new();
    for r in righe.0..=righe.1 {
        let Some(riga) = t.rows.get(r) else { continue };
        let celle: Vec<String> = riga
            .cells
            .iter()
            .enumerate()
            .filter(|(c, cella)| {
                let inizio = t.grid_col(r, *c);
                let fine = inizio + (cella.span() as usize).max(1) - 1;
                inizio <= colonne.1 && fine >= colonne.0
            })
            .map(|(_, cella)| testo_della_cella(doc, cella, 0))
            .collect();
        out.push(celle.join("\t"));
    }
    out.join("\n")
}

/// Il testo di una cella su una riga: i suoi paragrafi uniti da spazi.
fn testo_della_cella(doc: &Document, cella: &Cell, profondita: usize) -> String {
    let mut testo = String::new();
    testo_dei_blocchi(doc, &mut cella.blocks.iter().map(|b| &**b), &mut testo, profondita + 1);
    testo.trim_end_matches('\n').replace('\n', " ")
}

fn testo_dei_blocchi(doc: &Document, blocchi: &mut dyn Iterator<Item = &Block>, out: &mut String, profondita: usize) {
    // Come il motore: tabelle dentro tabelle oltre 16 livelli non si leggono.
    if profondita > 16 {
        return;
    }
    for b in blocchi {
        match b {
            Block::Para(p) => {
                out.push_str(&come_si_legge(doc, p));
                out.push('\n');
            }
            Block::Table(t) => {
                for riga in &t.rows {
                    let celle: Vec<String> = riga.cells.iter().map(|c| testo_della_cella(doc, c, profondita)).collect();
                    out.push_str(&celle.join("\t"));
                    out.push('\n');
                }
            }
        }
    }
}

/// Le opzioni di Trova e sostituisci: le maiuscole contano, solo le parole intere. Stanno nella
/// sessione del motore (`FindState`), che le usa anche per le frecce e per sostituire.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OpzioniRicerca {
    pub maiuscole: bool,
    pub parola_intera: bool,
}

/// I risultati di Trova: quanti sono e quale è selezionato (da 0).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Ricerca {
    pub conteggio: usize,
    pub attuale: Option<usize>,
}

/// Un titolo del documento, per l'Indice.
#[derive(Debug, Clone, PartialEq)]
pub struct Titolo {
    /// L'inizio del titolo.
    pub pos: Pos,
    /// Il livello di struttura (0 per Titolo 1).
    pub livello: u8,
    /// Il testo come si legge.
    pub testo: String,
}

/// I titoli del documento: i paragrafi con un livello di struttura (`outline_level`), come il
/// riquadro di navigazione di WordCraft (`ui-egui/src/panes.rs:180-196`), senza quelli vuoti e
/// quelli degli indici (stili «TOC…»). Il testo è quello che si legge, senza le cancellazioni
/// delle revisioni e il testo nascosto.
pub fn titoli(s: &Session) -> Vec<Titolo> {
    let mut out = Vec::new();
    for path in s.doc.para_paths(StoryRef::Body) {
        let Some(p) = s.doc.para(StoryRef::Body, &path) else { continue };
        let rp = s.doc.styles.resolve_para(&p.props);
        let Some(livello) = rp.outline_level else { continue };
        if rp.style.starts_with("TOC") {
            continue;
        }
        let testo = come_si_legge(&s.doc, p).trim().to_string();
        if testo.is_empty() {
            continue;
        }
        out.push(Titolo { pos: Pos { story: StoryRef::Body, path, off: 0 }, livello, testo });
    }
    out
}

/// Il posto di una posizione per la diagnosi: paragrafo e byte, mai il testo.
fn posto(p: &Pos) -> String {
    format!("paragrafo {:?}, byte {}", p.path.0, p.off)
}

/// Il formato dove sta il cursore (o la selezione), per accendere i bottoni della barra e del
/// foglio «Aa»: da `format.state` del motore, aggiornato dopo ogni comando, tocco e stato della
/// tastiera.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Formato {
    pub grassetto: bool,
    pub corsivo: bool,
    pub sottolineato: bool,
    /// Il paragrafo del cursore è una voce di un elenco puntato.
    pub puntato: bool,
    /// Il paragrafo del cursore è una voce di un elenco numerato.
    pub numerato: bool,
    /// L'id dello stile del paragrafo del cursore («Normal», «Heading1»…).
    pub stile: String,
    /// Il nome di quello stile («Normal», «Heading 1»…, le maiuscole cambiano da un documento
    /// all'altro), uguale anche dove l'id cambia (il Word italiano chiama «Titolo1» lo stile
    /// «heading 1»).
    pub nome_stile: String,
    /// La misura del testo in mezzi punti, come la tiene Word (21 è 10,5 punti); None se la
    /// selezione ne ha più d'una.
    pub mezzi_punti: Option<u32>,
    /// L'allineamento del paragrafo del cursore, com'è sulla pagina.
    pub allineamento: Allineamento,
    /// Il colore del testo: «auto» o RRGGBB; None se la selezione ne ha più d'uno.
    pub colore: Option<String>,
    /// Il colore dell'evidenziatore, RRGGBB; None senza evidenziatore o con più d'uno.
    pub evidenziatore: Option<String>,
}

/// L'allineamento di un paragrafo com'è sulla pagina: i quattro del foglio «Aa» (il distribuito di
/// Word conta come giustificato).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Allineamento {
    #[default]
    Sinistra,
    Centro,
    Destra,
    Giustificato,
}

impl Formato {
    /// Chiama la funzione di `format.state` (`engine/src/cmd/format.rs`) e non `Session::run`,
    /// che toglierebbe `join_next` e così spezzerebbe il gruppo di Annulla della scrittura.
    /// Gli elenchi come li accende WordCraft (`list_buttons`, `engine/src/cmd/para.rs`): dal
    /// livello del paragrafo, puntato o numerato. L'allineamento dal paragrafo del cursore, girato
    /// come si vede nei paragrafi da destra a sinistra (`Align::visual`, come i pulsanti del motore).
    pub fn di(s: &Session) -> Self {
        let v = wordcraft_engine::cmd::format::state(s);
        let si = |k: &str| v.get(k).and_then(Value::as_bool).unwrap_or(false);
        let paragrafo = s.doc.para_at(&s.sel.focus).map(|p| s.doc.styles.resolve_para(&p.props));
        let numerazione = paragrafo.as_ref().and_then(|p| p.numbering);
        let elenchi = wordcraft_engine::cmd::para::list_buttons(&s.doc, numerazione);
        let allineamento = match paragrafo.map(|p| p.align.visual(p.bidi)) {
            Some(Align::Center) => Allineamento::Centro,
            Some(Align::Right) => Allineamento::Destra,
            Some(Align::Justify | Align::Distribute) => Allineamento::Giustificato,
            Some(Align::Left) | None => Allineamento::Sinistra,
        };
        let testo = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
        Self {
            grassetto: si("bold"),
            corsivo: si("italic"),
            sottolineato: si("underline"),
            puntato: elenchi.bullets,
            numerato: elenchi.numbering,
            stile: testo("style").unwrap_or_else(|| "Normal".to_string()),
            nome_stile: testo("styleName").unwrap_or_else(|| "Normal".to_string()),
            mezzi_punti: v.get("size").and_then(Value::as_f64).map(|p| (p * 2.0).round() as u32),
            allineamento,
            colore: testo("color"),
            evidenziatore: testo("highlight"),
        }
    }

    /// La misura del testo da mostrare: «11», «10,5»; vuota se la selezione ne ha più d'una.
    pub fn dimensione(&self) -> String {
        match self.mezzi_punti {
            Some(m) if m % 2 == 1 => format!("{},5", m / 2),
            Some(m) => format!("{}", m / 2),
            None => String::new(),
        }
    }
}

/// Come appare uno stile di paragrafo nel documento: il campione del foglio «Aa».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AspettoStile {
    /// Il colore del testo; None se è automatico (nero sulla pagina bianca).
    pub colore: Option<[u8; 3]>,
    pub grassetto: bool,
    pub corsivo: bool,
    /// La misura del testo, in mezzi punti.
    pub mezzi_punti: u32,
}

/// L'aspetto dello stile `nome` (l'id, come «Heading1», o il nome, come «heading 1»), dagli stili
/// del documento; None se il documento non l'ha.
pub fn aspetto_stile(s: &Session, nome: &str) -> Option<AspettoStile> {
    let id = s.doc.styles.find(nome)?.id.clone();
    let c = s.doc.styles.resolve_char(Some(&id), &CharProps::default());
    let colore = match c.color {
        TextColor::Auto => None,
        TextColor::Rgb(Rgb(r, g, b)) => Some([r, g, b]),
    };
    Some(AspettoStile { colore, grassetto: c.bold, corsivo: c.italic, mezzi_punti: (c.size * 2.0).round() as u32 })
}

/// Che cosa si inserisce dal foglio «Inserisci».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Inserimento {
    /// La data e l'ora di adesso, nell'ora del telefono.
    DataOra,
    /// Il testo dopo il cursore va su una pagina nuova (`insert.pageBreak`).
    InterruzionePagina,
    /// Una linea sotto il paragrafo del cursore (`insert.horizontalLine`, un bordo in basso).
    LineaOrizzontale,
}

/// «24/10/2026 09:05».
pub fn data_e_ora(anno: i32, mese: u32, giorno: u32, ora: u32, minuti: u32) -> String {
    format!("{giorno:02}/{mese:02}/{anno} {ora:02}:{minuti:02}")
}

/// La data e l'ora di adesso nell'ora del telefono. Non `insert.dateTime` del motore, che usa
/// l'ora UTC (in Italia una o due ore indietro) e i mesi in inglese.
pub fn data_e_ora_adesso() -> String {
    let adesso = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (anno, mese, giorno, ora, minuti) = ora_locale(adesso as i64);
    data_e_ora(anno, mese, giorno, ora, minuti)
}

/// I conteggi di «Informazioni sul documento».
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Conteggi {
    pub parole: usize,
    /// Spazi compresi, senza gli a capo fra i paragrafi.
    pub caratteri: usize,
    pub caratteri_senza_spazi: usize,
    /// I paragrafi con del testo.
    pub paragrafi: usize,
    /// Le pagine vere, come le stampa il motore.
    pub pagine: usize,
}

/// Conta il testo del documento (sempre tutto, anche con una selezione), come «Conteggio parole»
/// di WordCraft (`review.wordCount`): le parole del motore, i caratteri senza il testo cancellato
/// con le revisioni. Per le pagine fa il layout delle pagine vere: la vista Scrittura rimette il
/// suo al frame dopo.
pub fn conteggi(s: &mut Session) -> Conteggi {
    let testo = s.doc.final_text(StoryRef::Body);
    let pagine = crate::vista::layout_pagine(s).pages.len();
    Conteggi {
        parole: s.doc.story_word_count(StoryRef::Body),
        caratteri: testo.chars().filter(|c| *c != '\n' && *c != '\r').count(),
        caratteri_senza_spazi: testo.chars().filter(|c| !c.is_whitespace()).count(),
        paragrafi: testo.split('\n').filter(|r| !r.trim().is_empty()).count(),
        pagine,
    }
}

pub struct Editor {
    pub s: Session,
    /// Il formato al cursore.
    pub formato: Formato,
    pub nome: String,
    /// Da dove viene il documento: decide che cosa fa «Salva».
    pub origine: Origine,
    pub vista: Vista,
    pub ponte: Ponte,
    pub aperto_alle: Instant,
    /// Il controllo ortografico delle Impostazioni: le parole sbagliate si sottolineano nella vista
    /// Scrittura (la vista Pagina, in sola lettura, resta pulita).
    pub controllo_ortografico: bool,
}

/// I messaggi dei documenti protetti con «Limita modifiche» di Word: il motore rifiuta le
/// modifiche che la protezione non permette, e le lettere battute sparivano senza dire perché
/// (analisi del 27/10, word I4).
pub const PROTETTO_SOLA_LETTURA: &str = "Documento protetto con «Limita modifiche»: si può solo leggere.";
pub const PROTETTO_COMMENTI: &str = "Documento protetto con «Limita modifiche»: si possono solo aggiungere commenti.";
pub const PROTETTO_MODULI: &str = "Documento protetto con «Limita modifiche»: si possono solo compilare i campi.";

/// Svuota le richieste che i comandi del motore lasciano all'interfaccia (`Session::ui_requests`:
/// `edit.find` senza testo, `edit.goto` con la pagina, `file.save` senza percorso,
/// `para.dialog`…), che qui nessuno apre e che altrimenti crescerebbero per sempre. Nella
/// diagnosi solo le chiavi, mai i valori.
pub fn svuota_richieste(s: &mut Session) {
    if s.ui_requests.is_empty() {
        return;
    }
    let chiavi: Vec<String> = s
        .ui_requests
        .drain(..)
        .map(|r| match r {
            Value::Object(m) => m.keys().cloned().collect::<Vec<_>>().join("+"),
            _ => "?".to_string(),
        })
        .collect();
    diagnosi(format!("richieste all'interfaccia lasciate cadere: {}", chiavi.join(", ")));
}

impl Editor {
    pub fn new(s: Session, nome: impl Into<String>) -> Self {
        let formato = Formato::di(&s);
        Self {
            s,
            formato,
            nome: nome.into(),
            origine: Origine::Nuovo,
            vista: Vista::default(),
            ponte: Ponte::default(),
            aperto_alle: Instant::now(),
            controllo_ortografico: true,
        }
    }

    /// Il messaggio della protezione del documento («Limita modifiche» di Word), se limita la
    /// scrittura: sola lettura, solo commenti, solo moduli. Le revisioni obbligatorie lasciano
    /// scrivere (le modifiche restano segnate), e la sola formattazione bloccata non si dice.
    pub fn protezione(&self) -> Option<&'static str> {
        match self.s.doc.settings.protection.as_deref()? {
            "readOnly" => Some(PROTETTO_SOLA_LETTURA),
            "comments" => Some(PROTETTO_COMMENTI),
            "forms" => Some(PROTETTO_MODULI),
            _ => None,
        }
    }

    /// Come `new`, con l'origine.
    pub fn con_origine(s: Session, nome: impl Into<String>, origine: Origine) -> Self {
        let mut e = Self::new(s, nome);
        if let Origine::File(p) = &origine {
            // Il recupero del motore chiama la sua copia col nome del file.
            e.s.path = Some(p.clone());
        }
        e.origine = origine;
        e
    }

    /// Disegna il documento e risponde ai gesti del dito: tocco, pressione lunga, maniglie e
    /// barra della selezione.
    pub fn mostra(&mut self, ui: &mut egui::Ui) -> Esito {
        self.s.view.proofing = self.controllo_ortografico && self.vista.modo == Modo::Scrittura;
        let gesto = self.vista.mostra(ui, &mut self.s, self.ponte.composizione.as_ref(), self.aperto_alle);
        match gesto {
            None => Esito::default(),
            Some(g) => self.gesto(g),
        }
    }

    /// Un gesto della vista.
    pub fn gesto(&mut self, g: Gesto) -> Esito {
        match g {
            Gesto::Tocco(t) => {
                let stato = self.tocca(t);
                Esito { apri_tastiera: stato.is_some(), stato, ..Esito::default() }
            }
            Gesto::ToccoPagina { pagina, punto } => Esito { stato: self.tocca_pagina(pagina, punto), ..Esito::default() },
            Gesto::PressioneLunga(t) => Esito { stato: self.pressione_lunga(t), ..Esito::default() },
            Gesto::Maniglia { maniglia, punto, presa } => {
                self.sposta_maniglia(maniglia, punto, presa);
                Esito::default()
            }
            Gesto::ManigliaLasciata => Esito { stato: self.lascia_maniglia(), ..Esito::default() },
            Gesto::DoppioTocco(t) => Esito { stato: self.seleziona_parola(t, "doppio tocco"), ..Esito::default() },
            Gesto::TriploTocco(t) => Esito { stato: self.seleziona_paragrafo(t), ..Esito::default() },
            Gesto::Cursore { punto, .. } => {
                self.sposta_cursore(punto);
                Esito::default()
            }
            Gesto::CursoreLasciato => Esito { stato: self.lascia_cursore(), ..Esito::default() },
            Gesto::Celle(punto) => {
                self.sposta_celle(punto);
                Esito::default()
            }
            Gesto::CelleLasciate => Esito { stato: self.lascia_celle(), ..Esito::default() },
            Gesto::MisuraImmagine(larghezza) => Esito { stato: self.misura_immagine(larghezza), ..Esito::default() },
            Gesto::Azione(Azione::Copia) => {
                let testo = self.copia();
                Esito { stato: self.ponte.dopo_comando(&self.s), appunti: testo.map(Appunti::Copia), ..Esito::default() }
            }
            Gesto::Azione(Azione::Taglia) => {
                let (testo, stato) = self.taglia();
                Esito { stato, appunti: testo.map(Appunti::Copia), ..Esito::default() }
            }
            Gesto::Azione(Azione::Incolla) => Esito { appunti: Some(Appunti::Incolla), ..Esito::default() },
            Gesto::Azione(Azione::Commenta) => {
                diagnosi("commenta: foglio chiesto");
                Esito { commenta: true, ..Esito::default() }
            }
            Gesto::Ortografia(SceltaOrtografia::Correggi(i)) => Esito { stato: self.correggi(i), ..Esito::default() },
            Gesto::Ortografia(SceltaOrtografia::Aggiungi) => {
                self.aggiungi_al_dizionario();
                Esito::default()
            }
            Gesto::Ortografia(SceltaOrtografia::Ignora) => {
                self.ignora_parola();
                Esito::default()
            }
            // Non da `comando`, che porterebbe la pagina alla fine del documento, dove va il cursore.
            Gesto::Azione(Azione::SelezionaTutto) => {
                if self.esegui("select.all", json!({})) {
                    diagnosi("comando: select.all");
                }
                Esito { stato: self.dopo_selezione(), ..Esito::default() }
            }
        }
    }

    /// Esegue un comando del motore e lascia nella diagnosi solo il nome e l'esito.
    pub(crate) fn esegui(&mut self, id: &str, params: Value) -> bool {
        match self.s.run(id, &params) {
            Ok(_) => true,
            Err(e) => {
                log::warn!("{id}: {}", genere(&e));
                diagnosi(format!("comando: {id}, non riuscito ({})", genere(&e)));
                false
            }
        }
    }

    /// Dopo un'azione sulla selezione: richieste svuotate, formato e tastiera aggiornati.
    pub(crate) fn dopo_selezione(&mut self) -> Option<StatoIme> {
        svuota_richieste(&mut self.s);
        self.formato = Formato::di(&self.s);
        self.ponte.dopo_comando(&self.s)
    }

    /// Pressione lunga: si seleziona la parola sotto il dito. Fuori da una parola resta il
    /// cursore, con la barra per incollare.
    pub fn pressione_lunga(&mut self, t: Tocco) -> Option<StatoIme> {
        self.seleziona_parola(t, "pressione lunga")
    }

    /// La parola sotto il dito, dalla pressione lunga o dal doppio tocco (`cosa`, per la diagnosi).
    fn seleziona_parola(&mut self, t: Tocco, cosa: &str) -> Option<StatoIme> {
        // Su un'immagine si sceglie lei, non una parola.
        if let Some(pos) = self.immagine_sotto(t) {
            return self.scegli_immagine(pos);
        }
        let l = self.s.layout();
        let Some(pos) = l.hit(0, t.x_pt, t.y_pt, StoryRef::Body) else {
            diagnosi(format!("{cosa}: fuori dal testo"));
            return None;
        };
        if self.esegui("caret.set", json!({ "pos": pos })) && self.esegui("select.word", json!({})) {
            self.togli_spazi_in_coda();
        }
        let (a, b) = self.s.sel.ordered();
        if a == b {
            self.vista.barra_al_cursore(a.clone());
        }
        diagnosi(format!("{cosa}: {}, selezione di {} byte", posto(&pos), if a.path == b.path { b.off.saturating_sub(a.off) } else { 0 }));
        self.dopo_selezione()
    }

    /// Triplo tocco: si seleziona il paragrafo sotto il dito, il suo testo senza il segno di
    /// paragrafo (`select.paragraph` lo prenderebbe, e la maniglia della fine andrebbe sulla riga
    /// dopo), come in Android.
    pub fn seleziona_paragrafo(&mut self, t: Tocco) -> Option<StatoIme> {
        if let Some(pos) = self.immagine_sotto(t) {
            return self.scegli_immagine(pos);
        }
        let l = self.s.layout();
        let Some(pos) = l.hit(0, t.x_pt, t.y_pt, StoryRef::Body) else {
            diagnosi("triplo tocco: fuori dal testo");
            return None;
        };
        let lungo = self.s.doc.para_at(&pos).map_or(pos.off, |p| p.text.len());
        let (inizio, fine) = (Pos { off: 0, ..pos.clone() }, Pos { off: lungo, ..pos.clone() });
        self.esegui("select.range", json!({ "anchor": inizio, "focus": fine }));
        let (a, b) = self.s.sel.ordered();
        diagnosi(format!(
            "triplo tocco: paragrafo {:?}, selezione di {} byte",
            pos.path.0,
            if a.path == b.path { b.off.saturating_sub(a.off) } else { 0 }
        ));
        self.dopo_selezione()
    }

    /// La goccia del cursore trascinata: il cursore va dove dice il layout sotto `t`, senza
    /// selezione. La tastiera aspetta che il dito si alzi (`lascia_cursore`), come con le maniglie.
    pub fn sposta_cursore(&mut self, t: Tocco) {
        let l = self.s.layout();
        let Some(pos) = l.hit(0, t.x_pt, t.y_pt, StoryRef::Body) else { return };
        if self.s.sel.ordered() != (pos.clone(), pos.clone()) {
            self.esegui("caret.set", json!({ "pos": pos }));
        }
        svuota_richieste(&mut self.s);
    }

    /// Il dito ha lasciato la goccia del cursore: la tastiera riceve il cursore nuovo.
    pub fn lascia_cursore(&mut self) -> Option<StatoIme> {
        diagnosi(format!("goccia del cursore lasciata: {}", posto(&self.s.sel.focus)));
        self.dopo_selezione()
    }

    /// `select.word` prende la parola con gli spazi dopo, come Word sul PC; sul telefono si
    /// seleziona la parola sola, come in Android.
    fn togli_spazi_in_coda(&mut self) {
        let (a, b) = self.s.sel.ordered();
        if a.path != b.path || a.story != b.story {
            return;
        }
        let Some(testo) = self.s.doc.para_at(&a).and_then(|p| p.text.get(a.off..b.off)) else { return };
        let fine = a.off + testo.trim_end().len();
        if fine > a.off && fine < b.off {
            let focus = Pos { off: fine, ..b };
            self.esegui("select.range", json!({ "anchor": a, "focus": focus }));
        }
    }

    /// Una maniglia trascinata: il suo bordo va dove dice il layout sotto `t`, l'altro resta
    /// fermo. Nel primo frame l'àncora passa all'altro bordo. La tastiera aspetta che il dito si
    /// alzi (`lascia_maniglia`): una finestra nuova a ogni frame la farebbe ripartire di continuo.
    pub fn sposta_maniglia(&mut self, m: Maniglia, t: Tocco, presa: bool) {
        let l = self.s.layout();
        let Some(pos) = l.hit(0, t.x_pt, t.y_pt, StoryRef::Body) else { return };
        let ancora = if presa {
            let (a, b) = self.s.sel.ordered();
            match m {
                Maniglia::Inizio => b,
                Maniglia::Fine => a,
            }
        } else {
            self.s.sel.anchor.clone()
        };
        if self.s.sel.anchor != ancora || self.s.sel.focus != pos {
            self.esegui("select.range", json!({ "anchor": ancora, "focus": pos }));
        }
        svuota_richieste(&mut self.s);
    }

    /// Il dito ha lasciato la maniglia: la tastiera riceve la selezione nuova.
    pub fn lascia_maniglia(&mut self) -> Option<StatoIme> {
        let (a, b) = self.s.sel.ordered();
        diagnosi(format!("maniglia lasciata: da {} a {}", posto(&a), posto(&b)));
        self.dopo_selezione()
    }

    /// Copia: il testo della selezione da mettere negli appunti, poi il cursore alla fine della
    /// selezione, come in Android (`TextView`, Copia chiude la barra e la selezione). None senza
    /// selezione o se la selezione non ha testo da leggere (tutta nascosta o cancellata). Non
    /// passa da `edit.copy`: Incolla usa il testo degli appunti di sistema, e il motore terrebbe
    /// solo un'altra copia del testo in memoria (con la storia degli appunti) e farebbe l'HTML
    /// della selezione a ogni Copia.
    pub fn copia(&mut self) -> Option<String> {
        if self.s.sel.is_collapsed() {
            return None;
        }
        let testo = testo_della_selezione(&self.s);
        self.esegui("select.collapse", json!({ "end": true }));
        diagnosi("comando: copia");
        svuota_richieste(&mut self.s);
        self.formato = Formato::di(&self.s);
        (!testo.is_empty()).then_some(testo)
    }

    /// C'è una selezione, o la barra al cursore: Indietro la chiude prima di uscire.
    pub fn ha_selezione(&self) -> bool {
        !self.s.sel.is_collapsed() || self.vista.ha_barra_al_cursore()
    }

    /// Indietro con una selezione: si chiude, col cursore alla fine, come Indietro su Android
    /// chiude la barra della selezione.
    pub fn chiudi_selezione(&mut self) -> Option<StatoIme> {
        self.vista.chiudi_barra();
        if !self.s.sel.is_collapsed() {
            self.esegui("select.collapse", json!({ "end": true }));
        }
        diagnosi("indietro: selezione chiusa");
        self.dopo_selezione()
    }

    /// Taglia: il testo della selezione, che esce dal documento, e lo stato per la tastiera.
    pub fn taglia(&mut self) -> (Option<String>, Option<StatoIme>) {
        if self.s.sel.is_collapsed() {
            return (None, None);
        }
        let testo = testo_della_selezione(&self.s);
        let tagliato = self.esegui("edit.cut", json!({}));
        if tagliato {
            diagnosi("comando: edit.cut");
        }
        self.vista.scorri_al_cursore();
        let stato = self.dopo_selezione();
        ((tagliato && !testo.is_empty()).then_some(testo), stato)
    }

    /// Incolla il testo degli appunti di sistema al posto della selezione, come testo semplice con
    /// il formato di dove arriva: `edit.pasteText` fa un paragrafo per ogni riga (cambia `\n` e
    /// `\r\n` in `\r`, e `paste_text` del motore divide lì).
    pub fn incolla(&mut self, testo: &str) -> Option<StatoIme> {
        if testo.is_empty() {
            return None;
        }
        if self.esegui("edit.pasteText", json!({ "text": testo })) {
            diagnosi("comando: edit.pasteText");
        }
        self.vista.scorri_al_cursore();
        self.dopo_selezione()
    }

    /// Un tocco nella vista Pagina: si torna in Scrittura col cursore nel punto del testo più
    /// vicino al dito, sulla pagina toccata (`DocLayout::hit` del layout delle pagine).
    pub fn tocca_pagina(&mut self, pagina: usize, t: Tocco) -> Option<StatoIme> {
        let l = crate::vista::layout_pagine(&mut self.s);
        match l.hit(pagina, t.x_pt, t.y_pt, StoryRef::Body) {
            Some(pos) => {
                diagnosi(format!("tocco sulla pagina {}: {}", pagina + 1, posto(&pos)));
                self.esegui("caret.set", json!({ "pos": pos }));
            }
            None => diagnosi(format!("tocco sulla pagina {}: fuori dal testo", pagina + 1)),
        }
        self.vista.cambia_modo(Modo::Scrittura);
        self.vista.scorri_al_cursore();
        self.dopo_selezione()
    }

    /// Indice: il cursore all'inizio del titolo, e la pagina scorre fin lì (in Scrittura).
    pub fn vai_al_titolo(&mut self, pos: &Pos) -> Option<StatoIme> {
        if self.esegui("caret.set", json!({ "pos": pos })) {
            diagnosi(format!("indice: salto al {}", posto(pos)));
        }
        self.vista.chiudi_barra();
        self.vista.cambia_modo(Modo::Scrittura);
        self.vista.scorri_al_cursore();
        self.dopo_selezione()
    }

    /// I risultati dell'ultima ricerca.
    pub fn ricerca(&self) -> Ricerca {
        let n = self.s.find.results.len();
        Ricerca { conteggio: n, attuale: (n > 0).then(|| self.s.find.current.min(n - 1)) }
    }

    /// Le opzioni di adesso.
    pub fn opzioni_ricerca(&self) -> OpzioniRicerca {
        OpzioniRicerca { maiuscole: self.s.find.match_case, parola_intera: self.s.find.whole_word }
    }

    /// Maiuscole o Parola intera cambiate: si cerca di nuovo `testo` con le opzioni nuove.
    pub fn cambia_opzioni(&mut self, testo: &str, o: OpzioniRicerca) -> Ricerca {
        self.s.find.match_case = o.maiuscole;
        self.s.find.whole_word = o.parola_intera;
        let si = |b: bool| if b { "sì" } else { "no" };
        diagnosi(format!("trova: maiuscole {}, parola intera {}", si(o.maiuscole), si(o.parola_intera)));
        self.trova(testo)
    }

    /// I parametri di ricerca dei comandi del motore: `testo` così com'è (`special: false`,
    /// niente codici come «^p», né nel testo cercato né in quello che lo sostituisce) e le opzioni.
    /// I parametri di `edit.find` per `testo`. Un testo con apostrofi o virgolette si cerca con
    /// un'espressione regolare (il resto protetto, letterale) che prende quelli della tastiera e
    /// quelli tipografici: «dell'ospedale» battuto trova «dell’ospedale» dei documenti fatti con
    /// Word, e viceversa (analisi del 27/10, word I6). Col regex il motore prende il testo
    /// sostitutivo così com'è.
    fn parametri_ricerca(&self, testo: &str) -> Value {
        let mut v = json!({ "text": testo, "special": false, "matchCase": self.s.find.match_case, "wholeWord": self.s.find.whole_word });
        if let Some(re) = regex_con_apostrofi(testo) {
            v["text"] = json!(re);
            v["regex"] = json!(true);
        }
        v
    }

    /// Trova: cerca `testo` così com'è e seleziona il primo risultato dal cursore in poi. Sempre
    /// col testo: senza, `edit.find` chiederebbe il riquadro di navigazione
    /// (`{"focus":"navSearch"}` in `ui_requests`). Un testo vuoto toglie i risultati. Nella
    /// diagnosi solo quanti caratteri e quanti risultati.
    pub fn trova(&mut self, testo: &str) -> Ricerca {
        if testo.is_empty() {
            self.s.find.results.clear();
            self.s.find.current = 0;
            return Ricerca::default();
        }
        // Si cerca dall'inizio della selezione: battendo una lettera in più il risultato di
        // prima, se va ancora bene, resta quello selezionato.
        let (a, _) = self.s.sel.ordered();
        self.esegui("caret.set", json!({ "pos": a }));
        if self.esegui("edit.find", self.parametri_ricerca(testo)) {
            diagnosi(format!("trova: {} caratteri, {} risultati", testo.chars().count(), self.s.find.results.len()));
        }
        self.dopo_ricerca()
    }

    /// Il risultato dopo (o prima, con `avanti` falso), dal fondo si riparte dall'inizio.
    pub fn trova_altro(&mut self, avanti: bool) -> Ricerca {
        if self.s.find.query.is_empty() || self.s.find.results.is_empty() {
            return self.ricerca();
        }
        let id = if avanti { "edit.findNext" } else { "edit.findPrevious" };
        if self.esegui(id, json!({})) {
            diagnosi(format!("comando: {id}"));
        }
        self.dopo_ricerca()
    }

    /// Sostituisci: se il risultato selezionato è uno di quelli di `testo`, diventa `con` e si
    /// seleziona il risultato dopo (`edit.replace`, un passo di Annulla); se no, come in Word, si va
    /// solo al risultato dopo, senza un passo di Annulla vuoto. I risultati si rifanno prima con
    /// `edit.advancedFind`, che non sposta la selezione. Nella diagnosi solo i conteggi.
    pub fn sostituisci(&mut self, testo: &str, con: &str) -> Ricerca {
        if testo.is_empty() {
            return self.ricerca();
        }
        let parametri = self.parametri_ricerca(testo);
        self.esegui("edit.advancedFind", parametri.clone());
        if self.s.find.results.is_empty() {
            diagnosi("sostituisci: nessun risultato");
        } else if self.s.find.results.contains(&self.s.sel.ordered()) {
            let mut parametri = parametri;
            parametri["with"] = json!(con);
            if self.esegui("edit.replace", parametri) {
                diagnosi(format!("sostituisci: 1 sostituito, ne restano {}", self.s.find.results.len()));
            }
        } else if self.esegui("edit.findNext", json!({})) {
            diagnosi(format!("sostituisci: niente da sostituire al cursore, ne restano {}", self.s.find.results.len()));
        }
        self.dopo_ricerca()
    }

    /// Sostituisci tutti: ogni risultato di `testo` diventa `con`, in un passo di Annulla solo
    /// (`edit.replaceAll`); poi il cursore dove cominciava la selezione e i risultati di adesso.
    /// Restituisce quanti ne ha sostituiti. Senza risultati non si fa niente (nemmeno un passo di
    /// Annulla vuoto). Nella diagnosi solo il numero.
    pub fn sostituisci_tutti(&mut self, testo: &str, con: &str) -> usize {
        if testo.is_empty() {
            return 0;
        }
        let mut parametri = self.parametri_ricerca(testo);
        self.esegui("edit.advancedFind", parametri.clone());
        if self.s.find.results.is_empty() {
            diagnosi("sostituisci tutti: nessun risultato");
            self.dopo_ricerca();
            return 0;
        }
        parametri["with"] = json!(con);
        let n = match self.s.run("edit.replaceAll", &parametri) {
            Ok(v) => v.get("replaced").and_then(Value::as_u64).unwrap_or(0) as usize,
            Err(e) => {
                log::warn!("edit.replaceAll: {}", genere(&e));
                diagnosi(format!("comando: edit.replaceAll, non riuscito ({})", genere(&e)));
                0
            }
        };
        diagnosi(format!("sostituisci tutti: {n} sostituiti"));
        let (a, _) = self.s.sel.ordered();
        self.esegui("caret.set", json!({ "pos": a }));
        self.trova(testo);
        n
    }

    fn dopo_ricerca(&mut self) -> Ricerca {
        self.vista.chiudi_barra();
        self.vista.cambia_modo(Modo::Scrittura);
        self.vista.scorri_al_cursore();
        svuota_richieste(&mut self.s);
        self.formato = Formato::di(&self.s);
        self.ricerca()
    }

    /// Tocco sul testo: cursore lì e finestra nuova per la tastiera.
    pub fn tocca(&mut self, t: Tocco) -> Option<StatoIme> {
        let posto = format!("tocco: x {:.0} pt, y {:.0} pt", t.x_pt, t.y_pt);
        // Un tocco su un'immagine la sceglie: riquadro e maniglia, niente tastiera.
        if let Some(pos) = self.immagine_sotto(t) {
            diagnosi(format!("{posto}, su un'immagine"));
            return self.scegli_immagine(pos);
        }
        let l = self.s.layout();
        let Some(pos) = l.hit(0, t.x_pt, t.y_pt, StoryRef::Body) else {
            diagnosi(format!("{posto}, fuori dal testo"));
            return None;
        };
        diagnosi(format!("{posto}, paragrafo {:?}, byte {}", pos.path.0, pos.off));
        if let Err(e) = self.s.run("caret.set", &json!({ "pos": pos })) {
            log::warn!("caret.set: {}", genere(&e));
        }
        svuota_richieste(&mut self.s);
        self.formato = Formato::di(&self.s);
        self.vista.scorri_al_cursore();
        self.vista.mostra_maniglia_cursore();
        // Su una parola sottolineata, sopra di lei la barra coi suggerimenti.
        if let Some(parola) = crate::ortografia::al_cursore(&mut self.s) {
            diagnosi(format!(
                "ortografia: parola sottolineata ({}), {} suggerimenti",
                if parola.ortografia { "ortografia" } else { "grammatica" },
                parola.suggerimenti.len()
            ));
            self.vista.mostra_ortografia(self.s.sel.focus.clone(), parola);
        }
        self.ponte.apri(&self.s)
    }

    /// Il suggerimento `i` della barra al posto della parola sottolineata, in un passo di Annulla.
    pub fn correggi(&mut self, i: usize) -> Option<StatoIme> {
        let testo = self.vista.ortografia().and_then(|p| p.suggerimenti.get(i).cloned())?;
        self.vista.chiudi_barra();
        self.comando_con("review.applySuggestion", json!({ "text": testo }), "ortografia: parola corretta")
    }

    /// La parola sottolineata nel dizionario personale (nella memoria privata dell'app): non si
    /// sottolinea più, in nessun documento.
    pub fn aggiungi_al_dizionario(&mut self) {
        self.vista.chiudi_barra();
        if self.esegui("review.addToDictionary", json!({})) {
            diagnosi("ortografia: parola aggiunta al dizionario");
        }
    }

    /// La parola sottolineata non si sottolinea più finché l'app resta aperta (non va nel
    /// dizionario).
    pub fn ignora_parola(&mut self) {
        self.vista.chiudi_barra();
        if self.esegui("review.ignoreAll", json!({})) {
            diagnosi("ortografia: parola ignorata");
        }
    }

    /// Stato mandato dalla tastiera.
    pub fn ricevi(&mut self, snap: &ImeSnapshot) -> Option<StatoIme> {
        self.vista.scorri_al_cursore();
        let st = match self.ponte.ricevi(&mut self.s, snap) {
            Ok(st) => st,
            Err(_) => {
                // Il messaggio del motore può contenere il testo: in logcat solo che è successo
                // (il genere è già nella diagnosi, scritto dal ponte).
                log::warn!("tastiera: modifica non applicata");
                self.ponte.apri(&self.s)
            }
        };
        svuota_richieste(&mut self.s);
        self.formato = Formato::di(&self.s);
        st
    }

    /// Tasto lasciato all'app dalla tastiera.
    pub fn tasto_crudo(&mut self, t: TastoCrudo) -> Option<StatoIme> {
        self.vista.scorri_al_cursore();
        let st = match self.ponte.tasto_crudo(&mut self.s, t) {
            Ok(st) => st,
            Err(_) => {
                log::warn!("tasto: non applicato");
                self.ponte.apri(&self.s)
            }
        };
        svuota_richieste(&mut self.s);
        self.formato = Formato::di(&self.s);
        st
    }

    /// Comando della barra (formato, annulla). Dopo Annulla e Ripeti la selezione si comprime
    /// anche ora che si vede: il motore rimette la selezione di prima della modifica, che può
    /// andare oltre la finestra della tastiera (una lettera battuta lì non la sostituirebbe) e
    /// farebbe comparire maniglie e barra senza che il dito abbia selezionato niente; Android,
    /// dopo Annulla in un campo di testo, lascia un cursore. Un'immagine scelta resta scelta,
    /// col suo riquadro: Annulla ha tolto una sua modifica.
    pub fn comando(&mut self, id: &str) -> Option<StatoIme> {
        self.comando_con(id, json!({}), &format!("comando: {id}"))
    }

    /// Un comando del foglio «Aa» col suo valore (`format.color` col colore, `format.highlight`
    /// con l'evidenziatore). Nella diagnosi solo il nome del comando, come `comando`.
    pub fn comando_con_valore(&mut self, id: &str, valore: Value) -> Option<StatoIme> {
        self.comando_con(id, valore, &format!("comando: {id}"))
    }

    /// Uno stile del foglio «Aa» per nome: `heading 1` è il nome che Word scrive sempre, anche
    /// quando l'id è un altro (il Word italiano lo chiama «Titolo1», e `para.heading1` del motore
    /// cerca l'id «Heading1»: non faceva niente, analisi del 27/10, word I2). Se il documento non
    /// ha uno stile con quel nome, il comando del motore. Nella diagnosi il comando, mai il nome.
    pub fn stile(&mut self, nome: &str, comando: &str) -> Option<StatoIme> {
        if self.s.doc.styles.find(nome).is_some() {
            self.comando_con("para.style", json!({ "style": nome }), &format!("comando: {comando}"))
        } else {
            self.comando(comando)
        }
    }

    /// Dal foglio «Inserisci».
    pub fn inserisci(&mut self, cosa: Inserimento) -> Option<StatoIme> {
        match cosa {
            // Come il testo battuto (`raw`: niente correzione automatica). Nella diagnosi solo che
            // cosa si è inserito.
            Inserimento::DataOra => self.comando_con("text.insert", json!({ "text": data_e_ora_adesso(), "raw": true }), "inserisci: data e ora"),
            Inserimento::InterruzionePagina => self.comando("insert.pageBreak"),
            Inserimento::LineaOrizzontale => self.comando("insert.horizontalLine"),
        }
    }

    /// Come `comando`, con i parametri; `riga` va nella diagnosi se riesce (mai i parametri, che
    /// possono avere del testo).
    pub(crate) fn comando_con(&mut self, id: &str, params: Value, riga: &str) -> Option<StatoIme> {
        match self.s.run(id, &params) {
            Ok(_) => diagnosi(riga),
            Err(e) => {
                log::warn!("{id}: {}", genere(&e));
                diagnosi(format!("comando: {id}, non riuscito ({})", genere(&e)));
            }
        }
        // Annulla e Ripeti possono portare il cursore fuori dallo schermo.
        self.vista.scorri_al_cursore();
        if matches!(id, "edit.undo" | "edit.redo")
            && !self.s.sel.is_collapsed()
            && crate::immagine::immagine_scelta(&self.s).is_none()
            && let Err(e) = self.s.run("select.collapse", &json!({ "end": true }))
        {
            log::warn!("select.collapse: {}", genere(&e));
        }
        svuota_richieste(&mut self.s);
        self.formato = Formato::di(&self.s);
        self.ponte.dopo_comando(&self.s)
    }
}

/// Gli apostrofi e le virgolette che si equivalgono nella ricerca.
const APOSTROFI: &[char] = &['\'', '\u{2019}', '\u{2018}', '\u{201B}', '\u{02BC}'];
const VIRGOLETTE: &[char] = &['"', '\u{201C}', '\u{201D}', '\u{201E}', '\u{201F}', '\u{00AB}', '\u{00BB}'];

/// `testo` come espressione regolare, con ogni apostrofo e ogni virgoletta che vale tutti quelli
/// del suo genere e il resto protetto; None se non ce ne sono.
fn regex_con_apostrofi(testo: &str) -> Option<String> {
    if !testo.chars().any(|c| APOSTROFI.contains(&c) || VIRGOLETTE.contains(&c)) {
        return None;
    }
    let classe = |cs: &[char]| format!("[{}]", cs.iter().collect::<String>());
    let mut re = String::new();
    for c in testo.chars() {
        if APOSTROFI.contains(&c) {
            re.push_str(&classe(APOSTROFI));
        } else if VIRGOLETTE.contains(&c) {
            re.push_str(&classe(VIRGOLETTE));
        } else {
            if r"\.+*?()|[]{}^$#&-~".contains(c) {
                re.push('\\');
            }
            re.push(c);
        }
    }
    Some(re)
}

#[cfg(test)]
mod test {
    use super::*;
    use wordcraft_engine::doc::Pos;

    #[test]
    fn un_documento_protetto_lo_dice_e_non_cambia() {
        // «Limita modifiche» di Word, sola lettura: il motore rifiuta ogni modifica, e le lettere
        // battute sparivano senza un messaggio (analisi del 27/10, word I4).
        let mut s = crate::prova::documento(&["Verbale della riunione"]);
        s.run("review.restrict", &json!({ "mode": "readOnly" })).unwrap();
        let byte = wordcraft_engine::io::save_bytes("protetto.docx", &s.doc).unwrap();
        let s = crate::documento::apri_byte("protetto.docx", &byte).unwrap();
        let mut e = Editor::new(s, "protetto.docx");
        assert_eq!(e.protezione(), Some(PROTETTO_SOLA_LETTURA));
        e.comando_con("text.insert", json!({ "text": "x", "raw": true }), "prova");
        assert_eq!(crate::prova::testi(&e.s), ["Verbale della riunione"]);
        // Solo commenti, solo moduli; un documento libero niente.
        let mut s = crate::prova::documento(&["Verbale"]);
        s.run("review.restrict", &json!({ "mode": "comments" })).unwrap();
        assert_eq!(Editor::new(s, "c.docx").protezione(), Some(PROTETTO_COMMENTI));
        let mut s = crate::prova::documento(&["Modulo"]);
        s.run("review.restrict", &json!({ "mode": "forms" })).unwrap();
        assert_eq!(Editor::new(s, "m.docx").protezione(), Some(PROTETTO_MODULI));
        assert_eq!(Editor::new(crate::prova::documento(&["Libero"]), "l.docx").protezione(), None);
    }

    #[test]
    fn copia_di_celle_scelte_porta_solo_quelle_celle() {
        // Tabella 3 per 3 (a…i): scelte col dito le quattro celle in alto a sinistra, come in Word.
        // Negli appunti vanno quelle quattro, una riga per riga e le celle separate da una
        // tabulazione (analisi del 27/10, word I3: andava «a\nb\nc\nd\ne»).
        let mut s = crate::prova::documento(&["Prima"]);
        s.run("insert.table", &json!({ "rows": 3, "cols": 3 })).unwrap();
        for (i, n) in ["a", "b", "c", "d", "e", "f", "g", "h", "i"].iter().enumerate() {
            if i > 0 {
                s.run("text.tab", &json!({})).unwrap();
            }
            s.run("text.insert", &json!({ "text": n, "raw": true })).unwrap();
        }
        let tabella = s.doc.body.iter().position(|b| b.as_table().is_some()).unwrap() as u32;
        let cella = |r: u32, c: u32| Pos { story: StoryRef::Body, path: wordcraft_engine::doc::Path(vec![tabella, r, c, 0]), off: 0 };
        s.run("caret.set", &json!({ "pos": cella(0, 0) })).unwrap();
        s.run("caret.set", &json!({ "pos": cella(1, 1), "extend": true })).unwrap();
        assert!(s.cell_range().is_some(), "il motore non ha scelto le celle");
        assert_eq!(testo_della_selezione(&s), "a\tb\nd\te");
        // Taglia svuota quelle celle e porta lo stesso testo.
        let mut e = Editor::new(s, "prova");
        assert_eq!(e.taglia().0.as_deref(), Some("a\tb\nd\te"));
    }

    #[test]
    fn i_titoli_si_mettono_anche_nei_documenti_del_word_italiano() {
        // Il Word italiano chiama «Titolo1» l'id dello stile che ha il nome «heading 1».
        let mut s = crate::prova::documento(&["Relazione"]);
        for (id, italiano) in [("Heading1", "Titolo1"), ("Heading2", "Titolo2")] {
            s.doc.styles.get_mut(id).unwrap().id = italiano.into();
        }
        let mut e = Editor::new(s, "prova");
        e.stile("heading 1", "para.heading1");
        assert_eq!(e.s.doc.body[0].as_para().unwrap().props.style.as_deref(), Some("Titolo1"));
        e.stile("heading 2", "para.heading2");
        assert_eq!(e.s.doc.body[0].as_para().unwrap().props.style.as_deref(), Some("Titolo2"));
        // Nei documenti coi nomi e gli id inglesi va come prima.
        let mut e = Editor::new(crate::prova::documento(&["Relazione"]), "prova");
        e.stile("heading 3", "para.heading3");
        assert_eq!(e.s.doc.body[0].as_para().unwrap().props.style.as_deref(), Some("Heading3"));
    }

    #[test]
    fn il_tocco_mette_il_cursore() {
        let mut e = Editor::new(crate::prova::documento(&["Ciao mondo"]), "prova");
        let l = crate::vista::layout_scrittura(&mut e.s, 412.0, 1.0);
        let c = l.caret_on(&Pos::body(0, 4), 0).unwrap();
        let st = e.tocca(Tocco { x_pt: c.x + 0.3, y_pt: c.top + c.height / 2.0 }).unwrap();
        assert_eq!(e.s.sel.focus, Pos::body(0, 4));
        assert_eq!((st.testo.as_str(), st.sel), ("Ciao mondo", (4, 4)));
    }

    /// Il tocco a metà della posizione `off` del paragrafo 0.
    fn tocca_in(e: &mut Editor, off: usize) -> Option<StatoIme> {
        let l = crate::vista::layout_scrittura(&mut e.s, 412.0, 1.0);
        let c = l.caret_on(&Pos::body(0, off), 0).unwrap();
        e.tocca(Tocco { x_pt: c.x + 0.3, y_pt: c.top + c.height / 2.0 })
    }

    #[test]
    fn il_tocco_su_una_parola_sottolineata_porta_i_suggerimenti_e_la_correzione() {
        crate::ortografia::test::con_il_dizionario();
        let mut e = Editor::new(crate::prova::documento(&["Il letto perche la terapia"]), "prova");
        tocca_in(&mut e, 4);
        assert!(e.vista.ortografia().is_none(), "«letto» è giusta");
        tocca_in(&mut e, 11).expect("la tastiera");
        let p = e.vista.ortografia().expect("la barra dei suggerimenti").clone();
        assert!(p.ortografia && e.ha_selezione(), "Indietro chiude la barra");
        assert_eq!(p.suggerimenti.first().map(String::as_str), Some("perché"));
        let esito = e.gesto(Gesto::Ortografia(SceltaOrtografia::Correggi(0)));
        assert_eq!(crate::prova::testi(&e.s), ["Il letto perché la terapia"]);
        let st = esito.stato.expect("la finestra nuova per la tastiera");
        assert_eq!(st.testo, "Il letto perché la terapia");
        assert!(e.vista.ortografia().is_none());
        e.comando("edit.undo");
        assert_eq!(crate::prova::testi(&e.s), ["Il letto perche la terapia"], "un Annulla solo");
        let d = tascabile_platform::diagnosi_testo();
        assert!(d.contains("] ortografia: parola sottolineata (ortografia), "), "{d}");
        assert!(d.contains("] ortografia: parola corretta\n"), "{d}");
        assert!(!d.contains("perche") && !d.contains("perché"), "{d}");
        // Indietro chiude la barra.
        tocca_in(&mut e, 11);
        e.chiudi_selezione();
        assert!(e.vista.ortografia().is_none());
    }

    #[test]
    fn aggiungi_al_dizionario_e_ignora() {
        crate::ortografia::test::con_il_dizionario();
        let cartella = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/prove-ortografia");
        let _ = std::fs::remove_dir_all(&cartella);
        let file = cartella.join(crate::ortografia::FILE_PERSONALE);
        crate::ortografia::usa_dizionario_personale(file.clone());
        let mut e = Editor::new(crate::prova::documento(&["Zanzibarotto e frigorifro"]), "prova");
        tocca_in(&mut e, 3);
        assert!(e.vista.ortografia().is_some_and(|p| p.ortografia));
        e.gesto(Gesto::Ortografia(SceltaOrtografia::Aggiungi));
        assert!(e.vista.ortografia().is_none());
        tocca_in(&mut e, 3);
        assert!(e.vista.ortografia().is_none(), "nel dizionario personale non è più sottolineata");
        // Nel file della memoria privata, una parola per riga in minuscolo.
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "zanzibarotto\n");
        // Ignora: non si sottolinea più, ma non va nel file.
        tocca_in(&mut e, 18);
        assert!(e.vista.ortografia().is_some());
        e.gesto(Gesto::Ortografia(SceltaOrtografia::Ignora));
        tocca_in(&mut e, 18);
        assert!(e.vista.ortografia().is_none());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "zanzibarotto\n");
        assert_eq!(crate::prova::testi(&e.s), ["Zanzibarotto e frigorifro"], "il testo non cambia");
        let d = tascabile_platform::diagnosi_testo();
        assert!(d.contains("] ortografia: parola aggiunta al dizionario\n") && d.contains("] ortografia: parola ignorata\n"), "{d}");
        assert!(!d.to_lowercase().contains("zanzibarotto") && !d.contains("frigorifro"), "{d}");
    }

    #[test]
    fn commenta_sulla_selezione_lo_chiede_all_app() {
        let mut e = Editor::new(crate::prova::documento(&["Ciao mondo"]), "prova");
        e.s.run("select.range", &json!({ "anchor": Pos::body(0, 0), "focus": Pos::body(0, 4) })).unwrap();
        let esito = e.gesto(Gesto::Azione(Azione::Commenta));
        assert!(esito.commenta);
        assert!(!e.s.sel.is_collapsed(), "la selezione resta per il commento");
    }

    #[test]
    fn senza_il_controllo_ortografico_niente_sottolineature() {
        crate::ortografia::test::con_il_dizionario();
        let mut e = Editor::new(crate::prova::documento(&["Il letto perche la terapia"]), "prova");
        e.controllo_ortografico = false;
        e.s.view.proofing = false;
        tocca_in(&mut e, 11);
        assert!(e.vista.ortografia().is_none());
    }

    #[test]
    fn il_tocco_resta_nella_diagnosi_con_la_posizione() {
        let mut e = Editor::new(crate::prova::documento(&["Uno", "Ciao mondo e dintorni"]), "prova");
        let l = crate::vista::layout_scrittura(&mut e.s, 412.0, 1.0);
        let c = l.caret_on(&Pos::body(1, 7), 0).unwrap();
        let t = Tocco { x_pt: c.x + 0.3, y_pt: c.top + c.height / 2.0 };
        e.tocca(t);
        let attesa = format!("] tocco: x {:.0} pt, y {:.0} pt, paragrafo [1], byte 7\n", t.x_pt, t.y_pt);
        let d = tascabile_platform::diagnosi_testo();
        assert!(d.contains(&attesa), "manca «{attesa}»:\n{d}");
        assert!(!d.contains("dintorni"), "{d}");
    }

    #[test]
    fn i_comandi_restano_nella_diagnosi() {
        let mut e = Editor::new(crate::prova::documento(&["Ciao"]), "prova");
        e.comando("para.heading3");
        e.comando("comando.che.non.esiste");
        let d = tascabile_platform::diagnosi_testo();
        assert!(d.contains("] comando: para.heading3\n"), "{d}");
        assert!(d.contains("] comando: comando.che.non.esiste, non riuscito (sconosciuto)\n"), "{d}");
    }

    #[test]
    fn le_richieste_all_interfaccia_si_svuotano_con_le_sole_chiavi() {
        let mut e = Editor::new(crate::prova::documento(&["Ciao Zanzibar"]), "prova");
        e.comando("edit.find");
        e.comando("para.dialog");
        assert!(e.s.ui_requests.is_empty(), "{:?}", e.s.ui_requests);
        let d = tascabile_platform::diagnosi_testo();
        assert!(d.contains("] richieste all'interfaccia lasciate cadere: open\n"), "{d}");
        assert!(!d.contains("\"find\"") && !d.contains("Zanzibar"), "{d}");
    }

    #[test]
    fn il_formato_dopo_grassetto_e_titolo() {
        let mut e = Editor::new(crate::prova::documento(&["Ciao mondo"]), "prova");
        let iniziale =
            Formato { stile: "Normal".into(), nome_stile: "Normal".into(), mezzi_punti: Some(24), colore: Some("auto".into()), ..Formato::default() };
        assert_eq!(e.formato, iniziale);
        e.s.run("select.range", &json!({ "anchor": Pos::body(0, 0), "focus": Pos::body(0, 4) })).unwrap();
        e.comando("format.bold");
        assert!(e.formato.grassetto && !e.formato.corsivo && !e.formato.sottolineato, "{:?}", e.formato);
        e.comando("format.italic");
        e.comando("format.underline");
        assert!(e.formato.corsivo && e.formato.sottolineato, "{:?}", e.formato);
        e.comando("para.heading1");
        assert_eq!(e.formato.stile, "Heading1");
        assert!(e.formato.nome_stile.eq_ignore_ascii_case("heading 1"), "{:?}", e.formato);
    }

    #[test]
    fn il_formato_dice_gli_elenchi() {
        let mut e = Editor::new(crate::prova::documento(&["Ciao", "mondo"]), "prova");
        assert!(!e.formato.puntato && !e.formato.numerato);
        e.comando("para.bullets");
        assert!(e.formato.puntato && !e.formato.numerato, "{:?}", e.formato);
        e.comando("para.numbering");
        assert!(!e.formato.puntato && e.formato.numerato, "{:?}", e.formato);
        e.comando("para.numbering");
        assert!(!e.formato.puntato && !e.formato.numerato, "{:?}", e.formato);
    }

    #[test]
    fn il_formato_dice_dimensione_allineamento_e_colori() {
        let mut e = Editor::new(crate::prova::documento(&["Ciao mondo"]), "prova");
        assert_eq!((e.formato.dimensione(), e.formato.allineamento), ("12".to_string(), Allineamento::Sinistra));
        assert_eq!((e.formato.colore.as_deref(), e.formato.evidenziatore.as_deref()), (Some("auto"), None));
        e.s.run("select.range", &json!({ "anchor": Pos::body(0, 0), "focus": Pos::body(0, 4) })).unwrap();
        e.comando("format.shrinkFont");
        assert_eq!(e.formato.dimensione(), "11");
        e.comando_con_valore("format.size", json!({ "size": 10.5 }));
        assert_eq!((e.formato.mezzi_punti, e.formato.dimensione()), (Some(21), "10,5".to_string()));
        e.comando_con_valore("format.color", json!({ "color": "C00000" }));
        assert_eq!(e.formato.colore.as_deref(), Some("C00000"));
        e.comando_con_valore("format.highlight", json!({ "color": "yellow" }));
        assert_eq!(e.formato.evidenziatore.as_deref(), Some("FFFF00"));
        // Una selezione con due misure e due colori: né l'una né l'altro.
        e.s.run("select.range", &json!({ "anchor": Pos::body(0, 0), "focus": Pos::body(0, 10) })).unwrap();
        let misto = Formato::di(&e.s);
        assert_eq!((misto.mezzi_punti, misto.dimensione(), misto.colore), (None, String::new(), None));
        // Togli formato: misura, colore ed evidenziatore tornano quelli dello stile, che resta.
        e.comando("para.heading2");
        e.comando("format.resetChar");
        let titolo = aspetto_stile(&e.s, "Heading2").unwrap();
        let [r, g, b] = titolo.colore.unwrap();
        let f = &e.formato;
        assert_eq!((f.colore.clone(), f.evidenziatore.as_deref(), f.stile.as_str()), (Some(format!("{r:02X}{g:02X}{b:02X}")), None, "Heading2"));
        assert_eq!(f.mezzi_punti, Some(titolo.mezzi_punti), "{f:?}");
        // L'allineamento del paragrafo, com'è sulla pagina.
        for (id, atteso) in [
            ("para.alignCenter", Allineamento::Centro),
            ("para.alignRight", Allineamento::Destra),
            ("para.justify", Allineamento::Giustificato),
            ("para.alignLeft", Allineamento::Sinistra),
        ] {
            e.comando(id);
            assert_eq!(e.formato.allineamento, atteso, "{id}");
        }
        let d = tascabile_platform::diagnosi_testo();
        assert!(d.contains("] comando: format.color\n") && d.contains("] comando: format.highlight\n"), "{d}");
    }

    #[test]
    fn l_aspetto_degli_stili_viene_dal_documento() {
        let s = crate::prova::documento(&["Ciao"]);
        let normale = aspetto_stile(&s, "Normal").expect("lo stile Normale");
        let titolo = aspetto_stile(&s, "heading 1").expect("il Titolo 1, cercato per nome");
        assert_eq!(aspetto_stile(&s, "Heading1"), Some(titolo), "per id è lo stesso");
        assert!(titolo.colore.is_some() && titolo.colore != normale.colore, "{normale:?} {titolo:?}");
        assert!(titolo.mezzi_punti > normale.mezzi_punti, "{normale:?} {titolo:?}");
        assert!(aspetto_stile(&s, "Uno stile che non c'è").is_none());
    }

    #[test]
    fn data_e_ora_all_italiana() {
        assert_eq!(data_e_ora(2026, 10, 24, 9, 5), "24/10/2026 09:05");
        assert_eq!(data_e_ora(2027, 1, 3, 23, 59), "03/01/2027 23:59");
    }

    #[test]
    fn inserisci_data_e_ora_interruzione_e_linea() {
        let mut e = Editor::new(crate::prova::documento(&["Ciao"]), "prova");
        e.s.run("caret.set", &json!({ "pos": Pos::body(0, 4) })).unwrap();
        e.s.reset_history();
        let t = punto(&mut e, 0, 4);
        e.tocca(t).expect("la tastiera si collega");
        let st = e.inserisci(Inserimento::DataOra);
        let testo = crate::prova::testi(&e.s)[0].clone();
        // «Ciao24/10/2026 10:52»: la data, poi l'ora, nell'ora del telefono.
        let data = testo.strip_prefix("Ciao").expect(&testo);
        let cifre: String = data.chars().filter(char::is_ascii_digit).collect();
        assert_eq!((data.len(), cifre.len(), &data[2..3], &data[10..11], &data[13..14]), (16, 12, "/", " ", ":"), "{data}");
        assert!(st.is_some(), "la tastiera riceve la finestra nuova");
        assert!(e.s.can_undo());
        e.inserisci(Inserimento::InterruzionePagina);
        assert!(crate::prova::testi(&e.s).concat().contains('\u{000C}'), "{:?}", crate::prova::testi(&e.s));
        e.inserisci(Inserimento::LineaOrizzontale);
        let p = e.s.doc.para_at(&e.s.sel.focus).unwrap();
        assert!(p.props.borders.as_ref().is_some_and(|b| b.bottom.is_some()), "{:?}", p.props.borders);
        let d = tascabile_platform::diagnosi_testo();
        for riga in ["] inserisci: data e ora\n", "] comando: insert.pageBreak\n", "] comando: insert.horizontalLine\n"] {
            assert!(d.contains(riga), "manca «{riga}»:\n{d}");
        }
    }

    #[test]
    fn i_conteggi_del_documento() {
        let mut e = Editor::new(crate::prova::documento(&["Uno due tre", "", "Quattro"]), "prova");
        // Anche con una selezione si conta tutto il documento.
        e.s.run("select.range", &json!({ "anchor": Pos::body(0, 0), "focus": Pos::body(0, 3) })).unwrap();
        let c = conteggi(&mut e.s);
        assert_eq!(c, Conteggi { parole: 4, caratteri: 18, caratteri_senza_spazi: 16, paragrafi: 2, pagine: 1 });
        let mut lungo = crate::esempi::lungo(60);
        assert!(conteggi(&mut lungo).pagine >= 2);
    }

    #[test]
    fn il_formato_segue_il_tocco_e_la_tastiera() {
        let mut e = Editor::new(crate::prova::documento(&["Ciao mondo"]), "prova");
        e.s.run("select.range", &json!({ "anchor": Pos::body(0, 0), "focus": Pos::body(0, 4) })).unwrap();
        e.comando("format.bold");
        assert!(e.formato.grassetto);
        // Un tocco dentro «mondo», che non è in grassetto.
        let l = crate::vista::layout_scrittura(&mut e.s, 412.0, 1.0);
        let c = l.caret_on(&Pos::body(0, 7), 0).unwrap();
        let st = e.tocca(Tocco { x_pt: c.x + 0.3, y_pt: c.top + c.height / 2.0 }).unwrap();
        assert!(!e.formato.grassetto, "dopo il tocco su «mondo»: {:?}", e.formato);
        // La tastiera riporta il cursore dentro «Ciao».
        let snap = ImeSnapshot { text: st.testo.clone(), sel: (2, 2), comp: (-1, -1), version: st.versione };
        e.ricevi(&snap);
        assert_eq!(e.s.sel.focus, Pos::body(0, 2));
        assert!(e.formato.grassetto, "dopo lo stato della tastiera su «Ciao»: {:?}", e.formato);
    }

    /// Il punto a metà della riga davanti alla posizione `off` del paragrafo `par`.
    fn punto(e: &mut Editor, par: usize, off: usize) -> Tocco {
        let l = crate::vista::layout_scrittura(&mut e.s, 412.0, 1.0);
        let c = l.caret_on(&Pos::body(par, off), 0).unwrap();
        Tocco { x_pt: c.x + 0.3, y_pt: c.top + c.height / 2.0 }
    }

    /// «mondo» selezionato con la pressione lunga, con la tastiera già collegata da un tocco.
    fn mondo_selezionato() -> Editor {
        let mut e = Editor::new(crate::prova::documento(&["Ciao mondo bello"]), "prova");
        let t = punto(&mut e, 0, 1);
        e.tocca(t).expect("la tastiera si collega");
        let t = punto(&mut e, 0, 7);
        e.pressione_lunga(t);
        e
    }

    #[test]
    fn la_pressione_lunga_seleziona_la_parola() {
        let e = mondo_selezionato();
        assert_eq!(e.s.sel.ordered(), (Pos::body(0, 5), Pos::body(0, 10)));
        assert_eq!(testo_della_selezione(&e.s), "mondo");
        let d = tascabile_platform::diagnosi_testo();
        assert!(d.contains("] pressione lunga: paragrafo [0], byte 7, selezione di 5 byte\n"), "{d}");
        assert!(!d.contains("mondo"), "{d}");
    }

    #[test]
    fn il_doppio_tocco_seleziona_la_parola_e_il_triplo_il_paragrafo() {
        let mut e = Editor::new(crate::prova::documento(&["Uno", "Ciao mondo bello", "Tre"]), "prova");
        let t = punto(&mut e, 1, 7);
        assert!(e.gesto(Gesto::Tocco(t)).apri_tastiera, "il primo tocco mette il cursore e apre la tastiera");
        assert_eq!(e.s.sel.focus, Pos::body(1, 7));
        let es = e.gesto(Gesto::DoppioTocco(t));
        assert_eq!(e.s.sel.ordered(), (Pos::body(1, 5), Pos::body(1, 10)), "la parola sola, senza lo spazio dopo");
        assert_eq!(testo_della_selezione(&e.s), "mondo");
        assert!(es.stato.is_some() && !es.apri_tastiera, "la tastiera riceve la selezione: {es:?}");
        let es = e.gesto(Gesto::TriploTocco(t));
        assert_eq!(e.s.sel.ordered(), (Pos::body(1, 0), Pos::body(1, 16)), "il paragrafo, senza il segno di paragrafo");
        assert_eq!(es.stato.map(|s| s.sel), Some((4, 20)), "nella finestra della tastiera «Uno\\n» viene prima");
        let d = tascabile_platform::diagnosi_testo();
        assert!(d.contains("] doppio tocco: paragrafo [1], byte 7, selezione di 5 byte\n"), "{d}");
        assert!(d.contains("] triplo tocco: paragrafo [1], selezione di 16 byte\n"), "{d}");
        assert!(!d.contains("mondo"), "{d}");
    }

    #[test]
    fn la_goccia_del_cursore_sposta_il_cursore() {
        let mut e = Editor::new(crate::prova::documento(&["Ciao mondo bello"]), "prova");
        let t = punto(&mut e, 0, 1);
        e.tocca(t).expect("la tastiera si collega");
        assert!(e.vista.goccia_chiesta(), "dopo il tocco la goccia sotto il cursore");
        let v = e.ponte.versione();
        let t = punto(&mut e, 0, 10);
        assert_eq!(e.gesto(Gesto::Cursore { punto: t, presa: true }), Esito::default());
        let t = punto(&mut e, 0, 16);
        e.gesto(Gesto::Cursore { punto: t, presa: false });
        assert_eq!(e.s.sel.ordered(), (Pos::body(0, 16), Pos::body(0, 16)), "il cursore segue la goccia, senza selezione");
        assert_eq!(e.ponte.versione(), v, "mentre si trascina la tastiera aspetta");
        let es = e.gesto(Gesto::CursoreLasciato);
        assert_eq!(es.stato.map(|s| s.sel), Some((16, 16)), "lasciata la goccia, la tastiera ha il cursore nuovo");
        let d = tascabile_platform::diagnosi_testo();
        assert!(d.contains("] goccia del cursore lasciata: paragrafo [0], byte 16\n"), "{d}");
    }

    #[test]
    fn trascinare_la_maniglia_allarga_la_selezione() {
        let mut e = mondo_selezionato();
        let t = punto(&mut e, 0, 10);
        e.sposta_maniglia(Maniglia::Fine, t, true);
        let t = punto(&mut e, 0, 16);
        e.sposta_maniglia(Maniglia::Fine, t, false);
        assert_eq!(e.s.sel.ordered(), (Pos::body(0, 5), Pos::body(0, 16)));
        assert_eq!(testo_della_selezione(&e.s), "mondo bello");
        // L'inizio, trascinato oltre la fine: la fine resta ferma e l'inizio passa dall'altra parte.
        let t = punto(&mut e, 0, 5);
        e.sposta_maniglia(Maniglia::Inizio, t, true);
        let t = punto(&mut e, 0, 0);
        e.sposta_maniglia(Maniglia::Inizio, t, false);
        assert_eq!(e.s.sel.ordered(), (Pos::body(0, 0), Pos::body(0, 16)));
        assert!(e.lascia_maniglia().is_some(), "la tastiera riceve la selezione nuova");
    }

    #[test]
    fn copia_da_il_testo_della_selezione_e_lascia_il_cursore_alla_fine() {
        let mut e = mondo_selezionato();
        let es = e.gesto(Gesto::Azione(Azione::Copia));
        assert_eq!(es.appunti, Some(Appunti::Copia("mondo".into())));
        assert_eq!(crate::prova::testi(&e.s), ["Ciao mondo bello"]);
        assert_eq!(e.s.sel.focus, Pos::body(0, 10));
        assert!(e.s.sel.is_collapsed());
    }

    #[test]
    fn taglia_toglie_il_testo_e_lo_da_agli_appunti() {
        let mut e = mondo_selezionato();
        let es = e.gesto(Gesto::Azione(Azione::Taglia));
        assert_eq!(es.appunti, Some(Appunti::Copia("mondo".into())));
        assert_eq!(crate::prova::testi(&e.s), ["Ciao  bello"]);
        assert!(es.stato.is_some(), "la tastiera riceve una finestra nuova");
    }

    #[test]
    fn incolla_due_righe_fa_due_paragrafi() {
        for testo in ["uno\ndue", "uno\r\ndue"] {
            let mut e = mondo_selezionato();
            assert_eq!(e.gesto(Gesto::Azione(Azione::Incolla)).appunti, Some(Appunti::Incolla));
            let st = e.incolla(testo);
            assert_eq!(crate::prova::testi(&e.s), ["Ciao uno", "due bello"], "{testo:?}");
            assert!(st.is_some(), "la tastiera riceve una finestra nuova");
        }
    }

    #[test]
    fn seleziona_tutto_e_ogni_azione_danno_una_finestra_nuova_alla_tastiera() {
        let mut e = mondo_selezionato();
        let v0 = e.ponte.dopo_comando(&e.s).map(|s| s.versione);
        assert_eq!(v0, None, "dopo la pressione lunga la tastiera ha già la selezione");
        let es = e.gesto(Gesto::Azione(Azione::SelezionaTutto));
        let st = es.stato.expect("finestra nuova dopo Seleziona tutto");
        assert_eq!(st.sel, (0, 16));
        let st2 = e.gesto(Gesto::Azione(Azione::Copia)).stato.expect("finestra nuova dopo Copia");
        assert!(st2.versione > st.versione);
        assert_eq!(st2.sel, (16, 16));
    }

    #[test]
    fn seleziona_tutto_non_porta_la_pagina_alla_fine() {
        let mut e = Editor::new(crate::prova::documento(&["Uno", "Due"]), "prova");
        e.gesto(Gesto::Azione(Azione::SelezionaTutto));
        assert_eq!(e.s.sel.ordered(), (Pos::body(0, 0), Pos::body(1, 3)));
        assert!(!e.vista.scorrimento_chiesto());
    }

    #[test]
    fn il_testo_copiato_lascia_fuori_cancellazioni_e_testo_nascosto() {
        let mut e = Editor::new(crate::prova::documento(&["Relazione su Mario Rossi", "Riga due"]), "prova");
        // «Mario Rossi» cancellato con le revisioni, «paziente A» al suo posto.
        e.s.run("review.trackChanges", &json!({ "value": true })).unwrap();
        e.s.run("select.range", &json!({ "anchor": Pos::body(0, 13), "focus": Pos::body(0, 24) })).unwrap();
        e.s.run("text.insert", &json!({ "text": "paziente A", "raw": true })).unwrap();
        e.s.run("review.trackChanges", &json!({ "value": false })).unwrap();
        // «due» nascosto.
        e.s.run("select.range", &json!({ "anchor": Pos::body(1, 5), "focus": Pos::body(1, 8) })).unwrap();
        e.s.run("format.hidden", &json!({})).unwrap();
        e.s.run("select.all", &json!({})).unwrap();
        assert_eq!(testo_della_selezione(&e.s), "Relazione su paziente A\nRiga ");
        assert!(e.s.selected_text().contains("Mario"), "il testo del motore lo tiene: {:?}", e.s.selected_text());
    }

    #[test]
    fn trova_tre_occorrenze_e_scorre_fra_esse() {
        let mut e = Editor::new(crate::prova::documento(&["gatto e cane", "un gatto nero", "il gatto"]), "prova");
        e.s.run("caret.set", &json!({ "pos": Pos::body(0, 0) })).unwrap();
        e.s.dirty = false;
        // Una lettera alla volta, come dalla tastiera: il primo risultato resta quello.
        for q in ["g", "ga", "gatto"] {
            e.trova(q);
        }
        assert!(e.s.ui_requests.is_empty(), "{:?}", e.s.ui_requests);
        assert_eq!(e.ricerca(), Ricerca { conteggio: 3, attuale: Some(0) });
        assert_eq!(e.s.sel.ordered(), (Pos::body(0, 0), Pos::body(0, 5)));
        assert!(e.vista.scorrimento_chiesto());
        assert_eq!(e.trova_altro(true).attuale, Some(1));
        assert_eq!(e.s.sel.ordered(), (Pos::body(1, 3), Pos::body(1, 8)));
        assert_eq!(e.trova_altro(true).attuale, Some(2));
        // Dopo l'ultimo si riparte dal primo, e all'indietro dal primo si va all'ultimo.
        assert_eq!(e.trova_altro(true).attuale, Some(0));
        assert_eq!(e.trova_altro(false).attuale, Some(2));
        assert_eq!(e.s.sel.ordered(), (Pos::body(2, 3), Pos::body(2, 8)));
        assert!(!e.s.dirty, "cercare non è una modifica da salvare");
        // Il testo vuoto toglie i risultati; un testo che non c'è ne dà zero.
        assert_eq!(e.trova(""), Ricerca::default());
        assert_eq!(e.trova("zebra").conteggio, 0);
        assert_eq!(e.trova_altro(true), Ricerca::default());
        let d = tascabile_platform::diagnosi_testo();
        assert!(d.contains("] trova: 5 caratteri, 3 risultati\n"), "{d}");
        assert!(!d.contains("gatto") && !d.contains("zebra"), "{d}");
    }

    #[test]
    fn sostituisci_uno_alla_volta_poi_il_risultato_dopo() {
        let mut e = Editor::new(crate::prova::documento(&["gatto e cane", "un gatto nero", "il gatto"]), "prova");
        e.s.run("caret.set", &json!({ "pos": Pos::body(0, 0) })).unwrap();
        e.s.reset_history();
        assert_eq!(e.trova("gatto"), Ricerca { conteggio: 3, attuale: Some(0) });
        // Il primo, selezionato, diventa «micio»; resta selezionato il risultato dopo.
        assert_eq!(e.sostituisci("gatto", "micio"), Ricerca { conteggio: 2, attuale: Some(0) });
        assert_eq!(crate::prova::testi(&e.s), ["micio e cane", "un gatto nero", "il gatto"]);
        assert_eq!(e.s.sel.ordered(), (Pos::body(1, 3), Pos::body(1, 8)));
        // Il cursore spostato altrove: Sostituisci prima porta al risultato, poi lo sostituisce.
        e.s.run("caret.set", &json!({ "pos": Pos::body(2, 0) })).unwrap();
        assert_eq!(e.sostituisci("gatto", "micio").conteggio, 2, "niente sostituito");
        assert_eq!(e.s.sel.ordered(), (Pos::body(2, 3), Pos::body(2, 8)));
        e.sostituisci("gatto", "micio");
        // L'ultimo: il primo di quelli che restano, da capo.
        assert_eq!(e.sostituisci("gatto", "micio"), Ricerca::default());
        assert_eq!(crate::prova::testi(&e.s), ["micio e cane", "un micio nero", "il micio"]);
        // Ogni sostituzione è un passo di Annulla.
        e.comando("edit.undo");
        assert_eq!(crate::prova::testi(&e.s), ["micio e cane", "un gatto nero", "il micio"]);
        let d = tascabile_platform::diagnosi_testo();
        assert!(d.contains("] sostituisci: 1 sostituito, ne restano 2\n"), "{d}");
        assert!(d.contains("] sostituisci: niente da sostituire al cursore, ne restano 2\n"), "{d}");
        assert!(!d.contains("micio") && !d.contains("gatto"), "{d}");
    }

    #[test]
    fn sostituisci_tutti_in_un_passo_solo() {
        let mut e = Editor::new(crate::prova::documento(&["gatto e cane", "un gatto nero", "il gatto"]), "prova");
        e.s.reset_history();
        e.trova("gatto");
        // La sostituzione col testo cercato dentro non ritrova sé stessa.
        assert_eq!(e.sostituisci_tutti("gatto", "gattone"), 3);
        assert_eq!(crate::prova::testi(&e.s), ["gattone e cane", "un gattone nero", "il gattone"]);
        assert_eq!(e.ricerca().conteggio, 3, "i risultati di adesso, dentro «gattone»");
        assert_eq!(e.sostituisci_tutti("zebra", "x"), 0);
        e.comando("edit.undo");
        assert_eq!(crate::prova::testi(&e.s), ["gatto e cane", "un gatto nero", "il gatto"], "un Annulla solo");
        // Un testo con dei codici si sostituisce com'è, e col testo vuoto si cancella.
        assert_eq!(e.sostituisci_tutti("cane", "^p"), 1);
        assert_eq!(crate::prova::testi(&e.s)[0], "gatto e ^p");
        assert_eq!(e.sostituisci_tutti("gatto ", ""), 2);
        assert_eq!(crate::prova::testi(&e.s), ["e ^p", "un nero", "il gatto"]);
        let d = tascabile_platform::diagnosi_testo();
        assert!(d.contains("] sostituisci tutti: 3 sostituiti\n"), "{d}");
        assert!(!d.contains("gattone"), "{d}");
    }

    #[test]
    fn maiuscole_e_parola_intera() {
        let mut e = Editor::new(crate::prova::documento(&["Gatto e gattone", "il gatto"]), "prova");
        assert_eq!(e.opzioni_ricerca(), OpzioniRicerca::default());
        assert_eq!(e.trova("gatto").conteggio, 3);
        let maiuscole = OpzioniRicerca { maiuscole: true, parola_intera: false };
        assert_eq!(e.cambia_opzioni("gatto", maiuscole).conteggio, 2);
        assert_eq!(e.cambia_opzioni("gatto", OpzioniRicerca { maiuscole: true, parola_intera: true }).conteggio, 1);
        assert_eq!(e.opzioni_ricerca(), OpzioniRicerca { maiuscole: true, parola_intera: true });
        // Le opzioni valgono anche per le frecce e per sostituire.
        assert_eq!(e.trova_altro(true).conteggio, 1);
        assert_eq!(e.sostituisci_tutti("gatto", "micio"), 1);
        assert_eq!(crate::prova::testi(&e.s), ["Gatto e gattone", "il micio"]);
        assert_eq!(e.cambia_opzioni("gatto", OpzioniRicerca::default()).conteggio, 2);
        let d = tascabile_platform::diagnosi_testo();
        assert!(d.contains("] trova: maiuscole sì, parola intera sì\n"), "{d}");
    }

    #[test]
    fn trova_gli_apostrofi_della_tastiera_e_quelli_tipografici() {
        // Analisi del 27/10, word I6: «dell'ospedale» battuto con la tastiera non trovava
        // «dell’ospedale» dei documenti fatti con Word, e viceversa.
        let mut e = Editor::new(crate::prova::documento(&["Il letto dell’ospedale", "«Visto» dal medico, e l'altro"]), "prova");
        assert_eq!(e.trova("dell'ospedale").conteggio, 1);
        assert_eq!(e.trova("\"visto\"").conteggio, 1);
        assert_eq!(e.trova("l’altro").conteggio, 1);
        // Il resto resta testo: il punto è un punto.
        assert_eq!(e.trova("l'a.tro").conteggio, 0);
        // Sostituisci tutti passa di lì: anche un «$» resta com'è.
        assert_eq!(e.sostituisci_tutti("dell'ospedale", "della casa $1"), 1);
        assert_eq!(crate::prova::testi(&e.s)[0], "Il letto della casa $1");
    }

    #[test]
    fn trova_prende_il_testo_com_e() {
        let mut e = Editor::new(crate::prova::documento(&["costo ^p 10", "x"]), "prova");
        assert_eq!(e.trova("^p").conteggio, 1, "«^p» è un testo, non l'a capo");
    }

    #[test]
    fn indice_con_tre_titoli_e_salto() {
        let mut e = Editor::new(crate::prova::documento(&["Primo", "testo", "Secondo", "altro", "Terzo"]), "prova");
        for (par, id) in [(0, "para.heading1"), (2, "para.heading2"), (4, "para.heading1")] {
            e.s.run("caret.set", &json!({ "pos": Pos::body(par, 0) })).unwrap();
            e.s.run(id, &json!({})).unwrap();
        }
        let t = titoli(&e.s);
        let visti: Vec<(&str, u8)> = t.iter().map(|t| (t.testo.as_str(), t.livello)).collect();
        assert_eq!(visti, [("Primo", 0), ("Secondo", 1), ("Terzo", 0)]);
        e.s.run("caret.set", &json!({ "pos": Pos::body(1, 2) })).unwrap();
        e.vai_al_titolo(&t[1].pos);
        assert_eq!(e.s.sel.focus, Pos::body(2, 0));
        assert!(e.s.sel.is_collapsed());
        assert!(e.vista.scorrimento_chiesto());
        let d = tascabile_platform::diagnosi_testo();
        assert!(d.contains("] indice: salto al paragrafo [2], byte 0\n"), "{d}");
    }

    #[test]
    fn il_tocco_su_una_pagina_torna_in_scrittura_col_cursore_li() {
        let mut e = Editor::new(crate::esempi::lungo(60), "prova");
        e.vista.cambia_modo(Modo::Pagina);
        let l = crate::vista::layout_pagine(&mut e.s);
        assert!(l.pages.len() >= 2, "pagine: {}", l.pages.len());
        // Il punto del testo all'inizio della seconda pagina.
        let pos = l.hit(1, 100.0, 100.0, StoryRef::Body).unwrap();
        let c = l.caret_on(&pos, 1).unwrap();
        e.gesto(Gesto::ToccoPagina { pagina: 1, punto: Tocco { x_pt: c.x + 0.3, y_pt: c.top + c.height / 2.0 } });
        assert_eq!(e.vista.modo, Modo::Scrittura);
        assert_eq!(e.s.sel.focus, pos);
        assert!(e.vista.scorrimento_chiesto());
    }

    #[test]
    fn annulla_lascia_un_cursore() {
        let mut e = Editor::new(crate::prova::documento(&["Ciao"]), "prova");
        e.s.run("select.range", &json!({ "anchor": Pos::body(0, 1), "focus": Pos::body(0, 3) })).unwrap();
        e.s.run("text.insert", &json!({ "text": "X", "raw": true })).unwrap();
        assert_eq!(crate::prova::testi(&e.s), ["CXo"]);
        e.comando("edit.undo");
        assert_eq!(crate::prova::testi(&e.s), ["Ciao"]);
        assert!(e.s.sel.is_collapsed(), "dopo Annulla niente selezione: {:?}", e.s.sel);
    }

    #[test]
    fn annulla_e_ripeti_riportano_il_cursore_a_schermo() {
        for id in ["edit.undo", "edit.redo"] {
            let mut e = Editor::new(crate::prova::documento(&["Ciao"]), "prova");
            e.s.run("text.insert", &json!({ "text": "X", "raw": true })).unwrap();
            if id == "edit.redo" {
                e.s.run("edit.undo", &json!({})).unwrap();
            }
            assert!(!e.vista.scorrimento_chiesto());
            e.comando(id);
            assert!(e.vista.scorrimento_chiesto(), "dopo {id} la pagina non torna al cursore");
        }
    }
}
