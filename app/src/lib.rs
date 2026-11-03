//! Doc Tascabile: lo stesso guscio eframe su Linux e su Android. Quattro schermate: la Home dei
//! file (con le cartelle), l'Editor del documento, le Impostazioni e la Diagnosi.

#[cfg(target_os = "android")]
mod android_main;
mod blocco;
mod dizionario;
mod fissa_data;
mod foglio_aa;
mod foglio_immagine;
mod foglio_pagina;
mod foglio_revisioni;
mod foglio_tabella;
mod guida;
mod home;
mod impostazioni;
mod pdf_annota;
mod pdf_compila;
mod pdf_firma;
mod pdf_lavoro;
mod pdf_modifica;
mod pdf_ocr;
mod pdf_pagine;
mod pdf_recupero;
mod quando;
mod recupero;
mod schermo_editor;
mod schermo_foglio;
mod schermo_pdf;
mod schermo_slide;

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use eframe::egui;
use home::{Azione, Home};
use impostazioni::{Impostazioni, NOME_APP};
use recupero::{Copia, Recupero, Risveglio};
use tascabile_files::{Recente, Tipo, tipo_da_nome};
pub use tascabile_platform::Piattaforma;
use tascabile_platform::{ErroreLettura, Fissa, OpenRequest, TastoCrudo, TipoTastiera, diagnosi, diagnosi_fissa, diagnosi_testo};
use tascabile_touch::tema::{self, Carattere};
use tascabile_touch::{
    AvvisoBreve, BarraAlta, BarraFlottante, BarraVoci, CampoTesto, Dialogo, FoglioDalFondo, Peso, PulsanteIcona, RispostaDialogo, Scheda, icone,
};
use tascabile_word::editor::{Editor, OpzioniRicerca, Ricerca, Titolo, titoli};
use tascabile_word::ponte::{Ponte, StatoIme};
use tascabile_word::salva::{self, ErroreSalva, Origine, Salvato};
use tascabile_word::vista::Modo;
use tascabile_word::{documento, esempi};

pub const VERSIONE: &str = env!("CARGO_PKG_VERSION");

/// Quanto aspettare dopo l'ultima modifica prima di scrivere la nostra copia privata: su Android,
/// dopo onPause, non gira più nessun frame, quindi la copia non può aspettare il fuoco perso.
const RITARDO_COPIA: Duration = Duration::from_secs(3);

/// Chi riceve gli stati della tastiera: il documento o il campo di testo della schermata (Trova
/// nell'Editor, la ricerca nella Home).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fuoco {
    Documento,
    Campo,
}

/// Il campo di un foglio dell'Editor: la descrizione dell'immagine, l'intestazione e il piè di
/// pagina del foglio «Pagina».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum QualeCampo {
    Descrizione,
    Intestazione,
    Pie,
    /// Il commento sulla selezione, o la risposta a un commento.
    Commento,
}

impl QualeCampo {
    /// Per la diagnosi.
    fn nome(self) -> &'static str {
        match self {
            QualeCampo::Descrizione => "della descrizione",
            QualeCampo::Intestazione => "dell'intestazione",
            QualeCampo::Pie => "del piè di pagina",
            QualeCampo::Commento => "del commento",
        }
    }
}

/// Le schermate dell'app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Schermata {
    Home,
    Editor,
    /// Il lettore dei PDF.
    Pdf,
    /// Le presentazioni.
    Slide,
    /// I fogli di calcolo, in sola lettura.
    Foglio,
    Impostazioni,
    Diagnosi,
}

/// I documenti delle Prove della Fase 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Prova {
    Checklist,
    Lungo,
}

/// Il documento dell'Editor.
struct Aperto {
    editor: Editor,
    /// Uno dei documenti delle Prove: uscendo non si chiede di salvare (la checklist resta da
    /// parte con le risposte, il documento lungo si butta). Le prove non hanno recupero.
    prova: Option<Prova>,
    /// Il recupero del motore è acceso per questo documento.
    recupero: bool,
    /// La nostra copia privata, scritta poco dopo l'ultima modifica non salvata (e quando l'app
    /// perde il fuoco) e tolta quando il documento è salvato, pulito o chiuso.
    copia: Option<Copia>,
    /// La revisione del motore che la copia ha dentro.
    rev_copia: Option<u64>,
    /// L'ultima revisione vista e da quando: la copia si scrive `ritardo_copia` dopo.
    rev_vista: Option<(u64, Instant)>,
    /// Le copie del motore che c'erano già quando il suo recupero si è acceso per questo
    /// documento: le altre le ha scritte lui, e la nostra copia le segna come sue gemelle.
    motore_altri: Option<Vec<String>>,
}

impl Aperto {
    fn prova(editor: Editor, prova: Prova) -> Self {
        Self { editor, prova: Some(prova), recupero: false, copia: None, rev_copia: None, rev_vista: None, motore_altri: None }
    }

    fn documento(editor: Editor) -> Self {
        Self { editor, prova: None, recupero: false, copia: None, rev_copia: None, rev_vista: None, motore_altri: None }
    }

    /// Le copie che il motore ha scritto per questo documento (vuoto finché il recupero non è
    /// pronto).
    fn id_motore(&self) -> Vec<String> {
        let Some(altri) = &self.motore_altri else { return Vec::new() };
        self.editor.s.recovery_documents().into_iter().map(|r| r.id).filter(|id| !altri.contains(id)).collect()
    }
}

/// I comandi del menu dell'Editor che scrivono un file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comando {
    Salva,
    SalvaCopia,
    EsportaPdf,
    Condividi,
}

impl Comando {
    fn etichetta(self) -> &'static str {
        match self {
            Comando::Salva => "salva",
            Comando::SalvaCopia => "salva una copia",
            Comando::EsportaPdf => "esporta pdf",
            Comando::Condividi => "condividi",
        }
    }
}

/// Che cosa fare dopo aver chiuso il documento dell'Editor.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Dopo {
    Home,
    ApriCon(OpenRequest),
    /// Il file `percorso` col nome `nome`: il PDF di una scansione appena fatta.
    ApriFile(PathBuf, String),
}

/// Le scelte del dialogo «Salvare le modifiche?».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scelta {
    Salva,
    NonSalvare,
    Annulla,
}

/// Un errore da dire col dialogo: un file che non si apre, un salvataggio o una condivisione che
/// non riescono, una copia di recupero che non si riapre.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Errore {
    titolo: &'static str,
    testo: String,
}

/// I titoli dei dialoghi degli errori e il loro pulsante.
const NON_SI_APRE: &str = "Il file non si apre";
pub(crate) const NON_SALVATO: &str = "Salvataggio non riuscito";
/// «Apri con» con un indirizzo che non si apre (`tascabile_platform::uri_da_aprire`).
pub(crate) const APRI_CON_RIFIUTATO: &str =
    "Per sicurezza Tascabile non apre i file passati in questo modo: cercalo nella Home con «Cerca nei documenti».";
const NON_CONDIVISO: &str = "Condivisione non riuscita";
const NON_RECUPERATO: &str = "Recupero non riuscito";
const HO_CAPITO: &str = "Ho capito";
/// L'azione dell'avviso «PDF pronto».
const CONDIVIDI_PDF: &str = "Condividi";

impl Errore {
    fn apertura(testo: impl Into<String>) -> Self {
        Self { titolo: NON_SI_APRE, testo: testo.into() }
    }
}

/// Da dove si apre un documento: decide l'inizio delle righe della diagnosi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Da {
    Cartella,
    Recenti,
    ApriCon,
    /// Una copia di recupero di un PDF, riaperta dalla Home.
    Recupero,
}

impl Da {
    fn etichetta(self) -> &'static str {
        match self {
            Da::Cartella => "apri dalla cartella",
            Da::Recenti => "apri dai recenti",
            Da::ApriCon => "apri con",
            Da::Recupero => "apri dal recupero",
        }
    }

    /// Una riga della diagnosi; quella dell'ultimo «Apri con» resta anche in testa al registro.
    fn scrivi(self, riga: String) {
        match self {
            Da::ApriCon => diagnosi_fissa(Fissa::ApriCon, riga),
            Da::Cartella | Da::Recenti | Da::Recupero => diagnosi(riga),
        }
    }
}

pub struct TascabileApp {
    piattaforma: Piattaforma,
    schermata: Schermata,
    /// Da dove si è arrivati alla Diagnosi: Indietro torna lì.
    prima_della_diagnosi: Schermata,
    /// Il registro com'era quando si è aperta la Diagnosi: i tocchi sulla schermata stessa non lo
    /// fanno scorrere sotto il dito.
    testo_diagnosi: String,
    home: Home,
    impostazioni: Impostazioni,
    /// Il documento dell'Editor; None nella Home.
    aperto: Option<Aperto>,
    /// La checklist messa da parte, con le risposte già scritte, quando non è nell'Editor.
    checklist: Option<Editor>,
    /// Un ponte mai aperto che tiene l'ultima versione data alla tastiera dai documenti chiusi: il
    /// documento che si apre riparte da lì, e uno stato in ritardo di uno di quelli si scarta,
    /// anche passando dalla Home.
    versioni: Ponte,
    /// Il dialogo «Salvare le modifiche?» aperto, con quello che va fatto dopo.
    dialogo: Option<Dopo>,
    /// L'app aveva il fuoco al passo prima: quando lo ritrova si ricontrolla l'accesso ai file.
    aveva_il_fuoco: bool,
    avvio: Instant,
    avvio_home_ms: Option<f32>,
    /// L'errore da dire col dialogo, finché non lo si chiude.
    errore: Option<Errore>,
    /// L'esito di un comando che non è un errore (salvato, copia salvata, PDF pronto), in basso.
    avviso_breve: AvvisoBreve,
    /// Il PDF appena esportato, da condividere con l'azione del suo avviso.
    pdf_pronto: Option<PathBuf>,
    /// Il recupero all'avvio: le copie da proporre nella Home.
    recupero: Recupero,
    /// Il contesto di egui, per il risveglio del recupero (dal thread del motore).
    contesto: Arc<OnceLock<egui::Context>>,
    risveglio: Risveglio,
    /// Quante copie private sono state scritte in questo avvio, per i loro nomi.
    copie_scritte: u64,
    /// Quanto aspettare dopo l'ultima modifica prima di scrivere la nostra copia privata.
    ritardo_copia: Duration,
    /// Il tema è stato controllato al primo `disegna` (e installato, se `prepara` non c'era stata).
    stile_pronto: bool,
    /// Il foglio «Aa» dell'Editor, con la tavolozza aperta sotto la fila del testo.
    foglio_aa: FoglioDalFondo,
    tavolozza_aa: Option<foglio_aa::QualeTavolozza>,
    /// Il foglio dell'Indice, con i titoli presi quando si è aperto.
    foglio_indice: FoglioDalFondo,
    indice: Vec<Titolo>,
    /// Il menu dell'Editor è aperto.
    menu_editor: bool,
    /// Il foglio «Inserisci» dell'Editor, con la misura della tabella quando mostra la griglia.
    foglio_inserisci: FoglioDalFondo,
    tabella_scelta: Option<(usize, usize)>,
    /// Il menu di Elimina della barra della tabella, aperto sopra questo punto.
    menu_tabella: Option<egui::Pos2>,
    /// Il menu di Allinea della barra dell'immagine, aperto in questo punto; il lato di adesso.
    menu_allinea: Option<(egui::Pos2, tascabile_word::immagine::Lato)>,
    /// Il foglio «Bordi e sfondo» della barra della tabella.
    foglio_bordi: FoglioDalFondo,
    /// Il foglio «Inserisci» mostra da dove prendere l'immagine.
    fonti_immagine: bool,
    /// La foto per il documento, chiesta ad Android o che si prepara.
    immagine_word: Option<foglio_immagine::AttesaImmagine>,
    /// Il foglio della descrizione dell'immagine scelta, col suo campo.
    foglio_descrizione: FoglioDalFondo,
    descrizione: Option<CampoTesto>,
    /// Il foglio «Pagina», coi campi della sua seconda pagina quando è aperta.
    foglio_pagina: FoglioDalFondo,
    campi_pagina: Option<foglio_pagina::CampiPagina>,
    /// Il foglio «Revisioni», con le modifiche e i commenti presi quando si è aperto (e dopo ogni
    /// comando), e quello del commento col suo campo.
    foglio_revisioni: FoglioDalFondo,
    revisioni: Option<tascabile_word::revisioni::StatoRevisioni>,
    foglio_commento: FoglioDalFondo,
    commento: Option<foglio_revisioni::CampoCommento>,
    /// Il foglio «Informazioni sul documento», coi conteggi presi quando si è aperto.
    foglio_info: FoglioDalFondo,
    informazioni: Option<schermo_editor::Informazioni>,
    /// La barra di Trova, se è aperta, col suo campo di testo.
    trova: Option<CampoTesto>,
    /// La seconda riga di Trova, col campo «Sostituisci con», se è aperta.
    sostituisci_con: Option<CampoTesto>,
    /// Chi riceve gli stati della tastiera.
    fuoco: Fuoco,
    /// Il dito giù, per la sua riga nella diagnosi quando si alza.
    dito: Option<DitoGiu>,
    /// Istante (`InputState::time`) dell'ultimo input visto da `dita_nella_diagnosi`.
    ultimo_input: Option<f64>,
    /// Il PDF del lettore; None fuori dal lettore.
    pdf: Option<schermo_pdf::PdfAperto>,
    /// Un PDF che si sta aprendo sul suo thread.
    apertura_pdf: Option<schermo_pdf::AperturaPdf>,
    /// Il menu del lettore è aperto.
    menu_pdf: bool,
    /// Il menu del lettore ha Ripeti in cima (telefono stretto, qualcosa da ripetere): si decide
    /// quando si apre, così le voci non si spostano sotto il dito se lo stato del motore cambia.
    ripeti_nel_menu_pdf: bool,
    /// I fogli del lettore: miniature, indice, «Vai a pagina», informazioni.
    fogli_pdf: schermo_pdf::FogliPdf,
    /// La firma e le iniziali salvate nella memoria privata dell'app.
    firme: pdf_firma::FirmeSalvate,
    /// L'impronta e il blocco dell'app (`blocco`).
    pub(crate) blocco: blocco::Blocco,
    /// Un PDF che chiede la password, col suo dialogo.
    password_pdf: Option<schermo_pdf::Password>,
    /// La posizione di un PDF che si sta scrivendo sul suo thread, con pagina e pagine per la
    /// riga della diagnosi.
    scrittura_posizioni: Option<schermo_pdf::ScritturaPosizione>,
    /// Il dizionario italiano dell'ortografia, caricato al primo documento Word.
    dizionario: dizionario::Dizionario,
    /// «Riconosci testo» che lavora, «Esporta in Word» che scrive, la scansione aperta (P8).
    riconosci: Option<pdf_ocr::Lavoro<Result<tascabile_pdf::ocr::EsitoOcr, tascabile_pdf::ocr::ErroreOcr>>>,
    /// Il .docx di Esporta in Word: dove, quanti byte, se viene dal testo riconosciuto.
    word: Option<pdf_ocr::Lavoro<Result<(PathBuf, usize, bool), &'static str>>>,
    scansione: Option<pdf_ocr::Scansione>,
    /// «Timestamp con blockchain» e «Controlla il timestamp»: il foglio e il lavoro in corso.
    data: fissa_data::DataDelDocumento,
    /// Estrai, Dividi, Comprimi o il PDF da unire che si legge, sul thread `pdf-file` (P6).
    file_pdf: Option<pdf_pagine::LavoroFile>,
    /// La presentazione delle Slide; None fuori dalle Slide.
    slide: Option<schermo_slide::SlideAperte>,
    /// Una presentazione che si sta aprendo sul suo thread.
    apertura_slide: Option<schermo_slide::AperturaSlide>,
    /// Il foglio di calcolo aperto, e quello che si apre.
    foglio: Option<schermo_foglio::FoglioAperto>,
    apertura_foglio: Option<schermo_foglio::AperturaFoglio>,
    /// I fogli delle Slide: miniature e note.
    fogli_slide: schermo_slide::FogliSlide,
}

/// Un dito appoggiato (o il tasto del mouse premuto), in punti egui (dp sul telefono).
struct DitoGiu {
    dove: egui::Pos2,
    alle: f64,
    /// La distanza più grande dal punto d'appoggio finora: egui non prende per un clic un dito
    /// che si è allontanato di più di `max_click_dist`.
    spostato: f32,
}

fn concesso(accesso: bool) -> &'static str {
    if accesso { "concesso" } else { "negato" }
}

/// «alle 09:30 del 24/10» nell'ora del telefono, per le copie di recupero.
fn ora_locale_breve(secondi_utc: u64) -> String {
    let (_, mese, giorno, ora, minuti) = salva::ora_locale(secondi_utc as i64);
    format!("salvato alle {ora:02}:{minuti:02} del {giorno:02}/{mese:02}")
}

/// Le foto scelte dalla galleria arrivate a chi non le aspettava (le chiede solo la scansione):
/// si cancellano dalla cache. Nella diagnosi `chi` e quante.
pub(crate) fn butta_le_foto(file: &[std::path::PathBuf], chi: &str) {
    for f in file {
        butta_la_foto(f);
    }
    diagnosi(format!("{chi}: {} foto arrivate senza attesa, cancellate", file.len()));
}

/// Una foto chiesta ad Android, letta o lasciata: il file e, se resta vuota, la cartella della sua
/// richiesta.
pub(crate) fn butta_la_foto(file: &std::path::Path) {
    let _ = std::fs::remove_file(file);
    if let Some(cartella) = file.parent()
        && cartella.file_name().is_some_and(|n| n.to_string_lossy().starts_with("scelte-"))
    {
        // Solo se è vuota: le altre foto della stessa scelta aspettano ancora.
        let _ = std::fs::remove_dir(cartella);
    }
}

/// Secondi UTC da quando comincia il 1970, per i recenti.
fn adesso() -> u64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Legge un file della memoria del telefono, con lo stesso tetto di `read_uri`. Nel genere
/// dell'errore solo il tipo dell'errore di I/O; il messaggio, per lo schermo, ha il percorso.
fn leggi_file(percorso: &Path) -> Result<Vec<u8>, ErroreLettura> {
    let dove = percorso.display().to_string();
    let file = std::fs::File::open(percorso).map_err(|e| ErroreLettura { genere: format!("{:?}", e.kind()), messaggio: format!("{dove}: {e}") })?;
    tascabile_platform::leggi_con_tetto(file, &dove)
}

/// Un recente con un percorso che non si trova: resta nei recenti (`apri_recente`).
const NON_TROVATO: &str =
    "Non trovo il file: forse è stato spostato o cancellato. Riprova tra poco; se non c'è più, toglilo dai recenti dal suo menu.";

/// Un URI che non si legge più: il permesso di un «Apri con» è scaduto (SecurityException) o il
/// file non c'è più (FileNotFoundException); sul PC, dove l'URI è un percorso, NotFound.
fn non_si_apre_piu(genere: &str) -> bool {
    genere.contains("SecurityException") || genere.contains("FileNotFoundException") || genere == "NotFound"
}

/// Il nome di un file arrivato con «Apri con»: quello vero (DISPLAY_NAME) se l'app che lo manda
/// lo dice, con l'estensione del tipo MIME se gli manca; altrimenti dal percorso o dal tipo MIME
/// (`nome_per`). Il motore sceglie il lettore dall'estensione.
fn nome_di_apri_con(r: &OpenRequest) -> String {
    if r.mime.as_deref() == Some("application/pdf") || r.nome.as_deref().is_some_and(|n| tipo_da_nome(n.trim()) == Tipo::Pdf) {
        return schermo_pdf::nome_del_pdf(r.nome.as_deref().unwrap_or(""), &r.uri);
    }
    // Una presentazione tiene il suo nome vero, con l'estensione del tipo MIME se gli manca;
    // senza nome, «presentazione.pptx» (o .ppsx, .potx) dal tipo MIME.
    let vero = r.nome.as_deref().map(str::trim).filter(|n| !n.is_empty());
    if let Some(n) = vero.filter(|n| tipo_da_nome(n) == Tipo::Presentazione) {
        return n.to_string();
    }
    // Un foglio di calcolo, lo stesso: col suo nome, o «foglio.xlsx» (.xls, .ods, .csv) dal tipo.
    if let Some(n) = vero.filter(|n| tipo_da_nome(n) == Tipo::Foglio) {
        return n.to_string();
    }
    if let Some(dal_mime) = foglio_da_mime(r.mime.as_deref()) {
        return match (vero, dal_mime.rsplit_once('.')) {
            (Some(n), Some((_, estensione))) => format!("{n}.{estensione}"),
            _ => dal_mime.to_string(),
        };
    }
    if let Some(dal_mime) = schermo_slide::nome_da_mime(r.mime.as_deref()) {
        return match (vero, dal_mime.rsplit_once('.')) {
            (Some(n), Some((_, estensione))) => format!("{n}.{estensione}"),
            _ => dal_mime.to_string(),
        };
    }
    let ripiego = documento::nome_per(&r.uri, r.mime.as_deref());
    let Some(vero) = vero else { return ripiego };
    if documento::estensione_nota(vero).is_some() {
        return vero.to_string();
    }
    match documento::estensione_nota(&ripiego) {
        Some(ext) => format!("{vero}.{ext}"),
        None => ripiego,
    }
}

/// Il nome di ripiego di un foglio di calcolo dal suo tipo MIME.
fn foglio_da_mime(mime: Option<&str>) -> Option<&'static str> {
    Some(match mime? {
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => "foglio.xlsx",
        "application/vnd.ms-excel" => "foglio.xls",
        "application/vnd.ms-excel.sheet.macroenabled.12" => "foglio.xlsm",
        "application/vnd.oasis.opendocument.spreadsheet" => "foglio.ods",
        "text/csv" | "text/comma-separated-values" => "foglio.csv",
        _ => return None,
    })
}

/// La riga della diagnosi di un salvataggio, anche in testa al registro: com'è andato, il genere
/// dell'errore, tipo e misura del file. Mai il nome né il messaggio dell'errore.
fn scrivi_esito(azione: &str, tipo: &str, esito: &Result<Salvato, ErroreSalva>) {
    let riga = match esito {
        Ok(r) => format!("{azione}: riuscito, tipo {}, {} byte", r.tipo, r.byte),
        Err(e) => format!("{azione}: non riuscito ({}), tipo {tipo}", e.genere),
    };
    diagnosi_fissa(Fissa::Salvataggio, riga);
}

/// Il nome del file scritto, per il messaggio sullo schermo.
fn file_di(r: &Salvato) -> String {
    r.percorso.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Il tempo di una misura, o «…» se non c'è ancora.
fn tempo(v: Option<f32>) -> String {
    v.map_or_else(|| "…".to_string(), |v| format!("{v:.0} ms"))
}

/// «2 di 5», «Nessun risultato», o niente col campo vuoto.
fn conteggio_ricerca(r: Ricerca, vuoto: bool) -> String {
    match (r.conteggio, r.attuale) {
        _ if vuoto => String::new(),
        (0, _) => "Nessun risultato".to_string(),
        (n, Some(i)) => format!("{} di {n}", i + 1),
        (n, None) => format!("{n}"),
    }
}

/// Prepara il contesto di egui per l'app: il tema dei bozzetti (caratteri, colori chiari, bersagli
/// da 48 dp) e la pressione lunga più vicina ad Android (in egui è 0,8 s). Va chiamata prima del
/// primo frame, nella creazione di eframe; se non c'è stata, la chiama il primo `disegna`.
pub fn prepara(ctx: &egui::Context) {
    // Coi caratteri delle celle dei fogli di calcolo (Carlito, già nell'APK per le Slide).
    tema::installa_con(ctx, &tascabile_foglio::caratteri_delle_celle());
    ctx.options_mut(|o| o.input_options.max_click_duration = 0.5);
}

/// Il contesto ha già il tema dell'app (`prepara`).
fn preparato(ctx: &egui::Context) -> bool {
    let chiaro = ctx.options(|o| o.theme_preference == egui::ThemePreference::Light && o.input_options.max_click_duration == 0.5);
    chiaro && ctx.style_of(egui::Theme::Light).visuals.panel_fill == tema::CARTA
}

impl TascabileApp {
    pub fn new(piattaforma: Piattaforma, avvio: Instant) -> Self {
        // Le foto rimaste nella cache da prima non le aspetta nessuno (0.8.3: una scelta della
        // galleria mai letta, se Android ha tolto l'app mentre la galleria era aperta); le copie
        // condivise più vecchie di un'ora nemmeno (0.8.4, audit di sicurezza del 30/10).
        piattaforma.pulisci_le_foto();
        piattaforma.pulisci_i_condivisi();
        // I temporanei dei salvataggi e le copie di recupero a metà lasciati da un'app chiusa di
        // colpo: non li propone nessuno e restavano in chiaro (revisione del 31/10, punto 12).
        tascabile_files::temporanei::pulisci(&piattaforma.cartella_app(), 4);
        recupero::togli_le_meta(&piattaforma.cartella_app());
        let accesso = piattaforma.accesso_file();
        diagnosi_fissa(Fissa::AccessoFile, format!("accesso ai file all'avvio: {}", concesso(accesso)));
        let home = Home::nuova(piattaforma.radice_archivio(), &piattaforma.cartella_app(), accesso);
        // L'autore dei commenti nei PDF: quello scritto nelle Impostazioni, mai preso dal telefono.
        let autore = impostazioni::leggi_autore(&piattaforma.cartella_app());
        let ortografia = impostazioni::leggi_ortografia(&piattaforma.cartella_app());
        // La modalità riservata scelta nelle Impostazioni, da subito (prima del primo frame).
        let riservata = impostazioni::leggi_riservata(&piattaforma.cartella_app());
        let rete_timestamp = impostazioni::leggi_rete_timestamp(&piattaforma.cartella_app());
        let impronta_firma = impostazioni::leggi_impronta_firma(&piattaforma.cartella_app());
        let blocco = impostazioni::leggi_blocco(&piattaforma.cartella_app());
        piattaforma.riservata(riservata);
        diagnosi(format!("modalità riservata: {}", if riservata { "accesa" } else { "spenta" }));
        // Le parole aggiunte al dizionario, nella cartella privata dell'app.
        let parole =
            tascabile_word::ortografia::usa_dizionario_personale(piattaforma.cartella_app().join(tascabile_word::ortografia::FILE_PERSONALE));
        diagnosi(format!("ortografia: {}, {parole} parole nel dizionario personale", if ortografia { "accesa" } else { "spenta" }));
        let contesto: Arc<OnceLock<egui::Context>> = Arc::default();
        let risveglio: Risveglio = {
            let contesto = contesto.clone();
            Arc::new(move || {
                if let Some(ctx) = contesto.get() {
                    ctx.request_repaint();
                }
            })
        };
        let recupero = Recupero::avvia(&piattaforma.cartella_app(), risveglio.clone());
        Self {
            piattaforma,
            schermata: Schermata::Home,
            prima_della_diagnosi: Schermata::Home,
            testo_diagnosi: String::new(),
            home,
            impostazioni: Impostazioni::con_autore(autore, ortografia).con_sicurezza(blocco, riservata, rete_timestamp, impronta_firma),
            aperto: None,
            checklist: None,
            versioni: Ponte::default(),
            dialogo: None,
            aveva_il_fuoco: true,
            avvio,
            avvio_home_ms: None,
            errore: None,
            avviso_breve: AvvisoBreve::new(egui::Id::new("avviso_breve")),
            pdf_pronto: None,
            recupero,
            contesto,
            risveglio,
            copie_scritte: 0,
            ritardo_copia: RITARDO_COPIA,
            stile_pronto: false,
            foglio_aa: FoglioDalFondo::new(egui::Id::new("foglio_aa")),
            tavolozza_aa: None,
            foglio_indice: FoglioDalFondo::new(egui::Id::new("foglio_indice")),
            indice: Vec::new(),
            menu_editor: false,
            foglio_inserisci: FoglioDalFondo::new(egui::Id::new("foglio_inserisci")),
            tabella_scelta: None,
            menu_tabella: None,
            menu_allinea: None,
            foglio_bordi: FoglioDalFondo::new(egui::Id::new("foglio_bordi")),
            fonti_immagine: false,
            immagine_word: None,
            foglio_descrizione: FoglioDalFondo::new(egui::Id::new("foglio_descrizione")),
            descrizione: None,
            foglio_pagina: FoglioDalFondo::new(egui::Id::new("foglio_pagina")),
            campi_pagina: None,
            foglio_revisioni: FoglioDalFondo::new(egui::Id::new("foglio_revisioni")),
            revisioni: None,
            foglio_commento: FoglioDalFondo::new(egui::Id::new("foglio_commento")),
            commento: None,
            foglio_info: FoglioDalFondo::new(egui::Id::new("foglio_info")),
            informazioni: None,
            trova: None,
            sostituisci_con: None,
            fuoco: Fuoco::Documento,
            dito: None,
            ultimo_input: None,
            pdf: None,
            apertura_pdf: None,
            menu_pdf: false,
            ripeti_nel_menu_pdf: false,
            fogli_pdf: schermo_pdf::FogliPdf::new(),
            firme: pdf_firma::FirmeSalvate::default(),
            blocco: blocco::Blocco::all_avvio(blocco),
            password_pdf: None,
            scrittura_posizioni: None,
            dizionario: dizionario::Dizionario::default(),
            riconosci: None,
            word: None,
            data: fissa_data::DataDelDocumento::nuova(),
            scansione: None,
            file_pdf: None,
            slide: None,
            apertura_slide: None,
            foglio: None,
            apertura_foglio: None,
            fogli_slide: schermo_slide::FogliSlide::new(),
        }
    }

    fn editor(&self) -> Option<&Editor> {
        self.aperto.as_ref().map(|a| &a.editor)
    }

    fn editor_mut(&mut self) -> Option<&mut Editor> {
        self.aperto.as_mut().map(|a| &mut a.editor)
    }

    /// Dopo ogni chiamata al ponte: lo stato nuovo va alla tastiera; se il ponte si è chiuso
    /// (paragrafo troppo lungo, finestra impossibile) o non c'è un documento, si chiude anche la
    /// tastiera, e la testata mostra `ponte.avviso`.
    fn dopo_ponte(&self, stato: Option<StatoIme>) {
        if let Some(st) = stato {
            self.piattaforma.set_ime_state(st.testo, st.sel.0, st.sel.1, st.versione);
        }
        let attivo = self.fuoco == Fuoco::Campo || self.editor().is_some_and(|e| e.ponte.attivo());
        if !attivo && self.piattaforma.keyboard_visible() {
            self.piattaforma.hide_keyboard();
        }
    }

    fn vai(&mut self, schermata: Schermata) {
        if self.schermata == schermata {
            return;
        }
        diagnosi(format!("schermata: {schermata:?}"));
        match self.schermata {
            // Il foglio di «Nuovo», il menu di un recente e la tastiera della ricerca restano nella Home.
            Schermata::Home => {
                self.home.chiudi_sopra();
                if self.fuoco == Fuoco::Campo {
                    self.fuoco_al_documento();
                    if self.piattaforma.keyboard_visible() {
                        self.piattaforma.hide_keyboard();
                    }
                }
            }
            Schermata::Impostazioni => self.impostazioni.chiudi(),
            Schermata::Pdf => self.menu_pdf = false,
            Schermata::Editor | Schermata::Slide | Schermata::Foglio | Schermata::Diagnosi => {}
        }
        self.schermata = schermata;
    }

    /// Mette `nuovo` nell'Editor (o niente) e restituisce quello che c'era. La tastiera si chiude,
    /// e con lei il ponte di chi esce: se torna nell'Editor (la checklist), non accetta stati della
    /// tastiera prima di un tocco. Le versioni della tastiera continuano da un documento all'altro:
    /// uno stato in ritardo del documento di prima ha una versione più vecchia della prima finestra
    /// di questo, e si scarta.
    fn sostituisci(&mut self, nuovo: Option<Aperto>) -> Option<Aperto> {
        self.piattaforma.hide_keyboard();
        self.fuoco_al_documento();
        self.trova = None;
        self.sostituisci_con = None;
        self.chiudi_sopra_l_editor();
        self.errore = None;
        // Una foto chiesta per il documento di prima non va in quello nuovo.
        self.immagine_word = None;
        let mut prima = self.aperto.take();
        if let Some(p) = &mut prima {
            // Le misure del disegno, che non stanno più sullo schermo.
            diagnosi(format!("documento chiuso, {}", p.editor.vista.riga_misure()));
            p.editor.ponte.chiudi();
            self.versioni.continua_da(&p.editor.ponte);
            // Chi esce è stato salvato o lasciato con «Non salvare»: le sue copie di recupero non
            // servono più.
            if p.recupero {
                recupero::spegni(&mut p.editor.s);
                p.recupero = false;
            }
            if let Some(c) = p.copia.take() {
                c.togli();
            }
        }
        self.aperto = nuovo.map(|mut n| {
            n.editor.ponte.continua_da(&self.versioni);
            if n.prova.is_none() && !n.recupero {
                recupero::accendi(&mut n.editor.s, &self.piattaforma.cartella_app(), &self.risveglio);
                n.recupero = true;
            }
            n
        });
        prima
    }

    /// Chiude quello che sta sopra l'Editor: menu e fogli.
    fn chiudi_sopra_l_editor(&mut self) {
        self.menu_editor = false;
        self.menu_pdf = false;
        self.foglio_aa.chiudi();
        self.foglio_indice.chiudi();
        self.foglio_inserisci.chiudi();
        self.tabella_scelta = None;
        self.menu_tabella = None;
        self.menu_allinea = None;
        self.foglio_bordi.chiudi();
        self.fonti_immagine = false;
        self.foglio_descrizione.chiudi();
        self.descrizione = None;
        self.foglio_pagina.chiudi();
        self.campi_pagina = None;
        self.foglio_revisioni.chiudi();
        self.revisioni = None;
        self.foglio_commento.chiudi();
        self.commento = None;
        self.foglio_info.chiudi();
        self.informazioni = None;
    }

    /// Il documento che esce dall'Editor: la checklist resta da parte con le risposte scritte, gli
    /// altri si chiudono.
    fn metti_da_parte(&mut self, uscito: Option<Aperto>) {
        if let Some(Aperto { editor, prova: Some(Prova::Checklist), .. }) = uscito {
            self.checklist = Some(editor);
        }
    }

    /// Porta `aperto` nell'Editor; il PDF del lettore e la presentazione, se c'erano, si chiudono.
    fn apri(&mut self, mut aperto: Aperto) {
        self.chiudi_pdf();
        self.chiudi_slide();
        self.chiudi_foglio();
        // L'autore dei commenti e delle revisioni e il controllo ortografico, dalle Impostazioni.
        aperto.editor.s.author = self.impostazioni.autore.clone();
        aperto.editor.controllo_ortografico = self.impostazioni.ortografia;
        self.carica_dizionario();
        let uscito = self.sostituisci(Some(aperto));
        self.metti_da_parte(uscito);
        self.vai(Schermata::Editor);
    }

    /// Apre nell'Editor un documento che non è una prova.
    fn cambia_documento(&mut self, editor: Editor) {
        self.apri(Aperto::documento(editor));
    }

    /// «Documento lungo»: il documento lungo delle misure, nuovo ogni volta.
    fn apri_lungo(&mut self) {
        // «apertura➡pagina» conta anche la costruzione del documento.
        let inizio = Instant::now();
        let mut editor = Editor::new(esempi::lungo(300), "Documento lungo");
        editor.aperto_alle = inizio;
        self.apri(Aperto::prova(editor, Prova::Lungo));
    }

    /// «Checklist tastiera»: la checklist con le risposte già scritte, nuova solo la prima volta.
    /// Se è già nell'Editor non succede niente: ricominciarla le perderebbe.
    fn apri_checklist(&mut self) {
        if self.aperto.as_ref().is_some_and(|a| a.prova == Some(Prova::Checklist)) {
            self.vai(Schermata::Editor);
            return;
        }
        let editor = self.checklist.take().unwrap_or_else(|| {
            let inizio = Instant::now();
            let mut editor = Editor::new(esempi::checklist(), "Checklist");
            editor.aperto_alle = inizio;
            editor
        });
        self.apri(Aperto::prova(editor, Prova::Checklist));
    }

    fn nuovo_documento(&mut self) {
        diagnosi("nuovo documento");
        self.cambia_documento(Editor::new(documento::nuovo(), "Documento nuovo"));
    }

    /// Un documento nuovo da un modello del «Nuovo»: com'è un documento nuovo (Salva chiede dove,
    /// col nome dal titolo), col cursore nel primo campo da riempire.
    fn nuovo_da_modello(&mut self, m: tascabile_word::modelli::Modello) {
        match m.apri() {
            Ok(s) => {
                diagnosi(format!("nuovo documento dal modello {}", m.nome()));
                self.cambia_documento(Editor::new(s, m.nome()));
            }
            Err(e) => {
                diagnosi(format!("modello {}: non si apre ({})", m.nome(), e.genere));
                self.errore = Some(Errore::apertura(e.messaggio));
            }
        }
    }

    /// «Diagnosi»: il registro com'è adesso.
    fn apri_diagnosi(&mut self) {
        if self.piattaforma.keyboard_visible() {
            self.piattaforma.hide_keyboard();
        }
        self.fuoco_al_documento();
        if let Some(e) = self.editor_mut() {
            e.ponte.chiudi();
        }
        self.chiudi_sopra_l_editor();
        self.prima_della_diagnosi = self.schermata;
        self.testo_diagnosi = diagnosi_testo();
        self.vai(Schermata::Diagnosi);
    }

    /// Il campo di Trova prende la tastiera, e la apre.
    fn fuoco_al_campo(&mut self) {
        self.fuoco_a_trova(false, true);
    }

    /// Il campo «Sostituisci con» prende la tastiera, e la apre.
    fn fuoco_al_sostituisci(&mut self) {
        self.fuoco_a_trova(true, true);
    }

    /// Uno dei due campi di Trova (con `sostituisci` quello di «Sostituisci con») prende la
    /// tastiera: il ponte del documento si chiude, l'altro campo lascia il fuoco, e le versioni
    /// continuano da uno all'altro in tutti i versi, così uno stato in ritardo di chi aveva la
    /// tastiera prima si scarta. Con `apri` la tastiera si apre.
    fn fuoco_a_trova(&mut self, sostituisci: bool, apri: bool) {
        let Some(a) = self.aperto.as_mut() else { return };
        let mut dopo = a.editor.ponte.versione();
        for c in [self.trova.as_ref(), self.sostituisci_con.as_ref()].into_iter().flatten() {
            dopo = dopo.max(c.versione());
        }
        let (campo, altro, nome) = if sostituisci {
            (self.sostituisci_con.as_mut(), self.trova.as_mut(), "Sostituisci con")
        } else {
            (self.trova.as_mut(), self.sostituisci_con.as_mut(), "di Trova")
        };
        let Some(c) = campo else { return };
        a.editor.ponte.chiudi();
        if let Some(altro) = altro {
            altro.lascia_fuoco();
        }
        let gia = self.fuoco == Fuoco::Campo && c.ha_il_fuoco();
        let st = c.prendi_fuoco(dopo);
        a.editor.ponte.continua_dalla_versione(st.versione);
        if !gia {
            diagnosi(format!("fuoco: campo {nome}, finestra v{}", st.versione));
        }
        self.fuoco = Fuoco::Campo;
        self.piattaforma.tipo_tastiera(TipoTastiera::Testo);
        self.piattaforma.set_ime_state(st.testo, st.sel.0, st.sel.1, st.versione);
        if apri {
            self.piattaforma.show_keyboard();
        }
    }

    /// Il campo «Sostituisci con» ha la tastiera.
    fn al_sostituisci(&self) -> bool {
        self.sostituisci_con.as_ref().is_some_and(CampoTesto::ha_il_fuoco)
    }

    /// Il nome del campo dell'Editor che ha la tastiera, per la diagnosi.
    fn campo_di_trova(&self) -> &'static str {
        if let Some(q) = self.quale_campo_word() {
            return q.nome();
        }
        if self.al_sostituisci() { "di Sostituisci" } else { "di Trova" }
    }

    /// Un foglio dell'Editor col suo campo è aperto: la descrizione dell'immagine o la seconda
    /// pagina del foglio «Pagina». Il suo campo riceve la tastiera al posto di Trova.
    fn campo_word_aperto(&self) -> bool {
        self.descrizione.is_some() || self.campi_pagina.is_some() || self.commento.is_some()
    }

    /// Il campo di un foglio dell'Editor che riceve la tastiera.
    fn quale_campo_word(&self) -> Option<QualeCampo> {
        if self.descrizione.is_some() {
            return Some(QualeCampo::Descrizione);
        }
        if self.commento.is_some() {
            return Some(QualeCampo::Commento);
        }
        let c = self.campi_pagina.as_ref()?;
        Some(if c.pie.ha_il_fuoco() { QualeCampo::Pie } else { QualeCampo::Intestazione })
    }

    fn campo_word(&mut self, q: QualeCampo) -> Option<&mut CampoTesto> {
        match q {
            QualeCampo::Descrizione => self.descrizione.as_mut(),
            QualeCampo::Intestazione => self.campi_pagina.as_mut().map(|c| &mut c.intestazione),
            QualeCampo::Pie => self.campi_pagina.as_mut().map(|c| &mut c.pie),
            QualeCampo::Commento => self.commento.as_mut().map(|c| &mut c.campo),
        }
    }

    /// Il campo `q` di un foglio dell'Editor prende la tastiera, e la apre: il ponte del documento
    /// si chiude, l'altro campo del foglio lascia il fuoco, e le versioni continuano oltre tutte
    /// quelle date prima, come fra i due campi di Trova.
    fn fuoco_al_campo_word(&mut self, q: QualeCampo) {
        let mut dopo = self.versioni.versione();
        if let Some(a) = self.aperto.as_mut() {
            dopo = dopo.max(a.editor.ponte.versione());
            a.editor.ponte.chiudi();
        }
        for c in [
            self.descrizione.as_ref(),
            self.campi_pagina.as_ref().map(|c| &c.intestazione),
            self.campi_pagina.as_ref().map(|c| &c.pie),
            self.commento.as_ref().map(|c| &c.campo),
        ]
        .into_iter()
        .flatten()
        {
            dopo = dopo.max(c.versione());
        }
        let altro = match q {
            QualeCampo::Intestazione => Some(QualeCampo::Pie),
            QualeCampo::Pie => Some(QualeCampo::Intestazione),
            QualeCampo::Descrizione | QualeCampo::Commento => None,
        };
        if let Some(c) = altro.and_then(|a| self.campo_word(a)) {
            c.lascia_fuoco();
        }
        let gia = self.fuoco == Fuoco::Campo && self.campo_word(q).is_some_and(|c| c.ha_il_fuoco());
        let Some(st) = self.campo_word(q).map(|c| c.prendi_fuoco(dopo)) else { return };
        if let Some(e) = self.editor_mut() {
            e.ponte.continua_dalla_versione(st.versione);
        }
        self.versioni.continua_dalla_versione(st.versione);
        if !gia {
            diagnosi(format!("fuoco: campo {}, finestra v{}", q.nome(), st.versione));
        }
        self.fuoco = Fuoco::Campo;
        self.piattaforma.tipo_tastiera(TipoTastiera::Testo);
        self.piattaforma.set_ime_state(st.testo, st.sel.0, st.sel.1, st.versione);
        self.piattaforma.show_keyboard();
    }

    /// I campi dei fogli dell'Editor se ne vanno: il fuoco torna al documento e la tastiera, se era
    /// loro, si chiude.
    fn chiudi_campi_word(&mut self) {
        if self.fuoco == Fuoco::Campo && self.campo_word_aperto() {
            self.fuoco_al_documento();
            if self.piattaforma.keyboard_visible() {
                self.piattaforma.hide_keyboard();
            }
        }
        self.descrizione = None;
        self.campi_pagina = None;
        self.commento = None;
    }

    /// Il campo di testo che riceve la tastiera col fuoco al campo: la password di un PDF, se il
    /// suo dialogo è aperto; se no Trova nell'Editor, la ricerca nella Home, i campi del lettore.
    fn campo_mut(&mut self) -> Option<&mut CampoTesto> {
        if self.password_pdf.is_some() {
            return self.password_pdf.as_mut().map(|p| &mut p.campo);
        }
        match self.schermata {
            Schermata::Editor if self.campo_word_aperto() => match self.quale_campo_word() {
                Some(q) => self.campo_word(q),
                None => None,
            },
            Schermata::Editor if self.al_sostituisci() => self.sostituisci_con.as_mut(),
            Schermata::Editor => self.trova.as_mut(),
            Schermata::Home => Some(self.home.ricerca_mut()),
            Schermata::Pdf => self.campo_pdf_mut(),
            Schermata::Impostazioni => self.impostazioni.campo_autore_mut(),
            Schermata::Slide | Schermata::Foglio | Schermata::Diagnosi => None,
        }
    }

    /// Il campo della ricerca della Home prende la tastiera, con una versione più nuova di tutte
    /// quelle date dai documenti; i documenti che si apriranno continuano da lì.
    fn fuoco_alla_ricerca(&mut self) {
        let dopo = self.versioni.versione();
        let st = self.home.ricerca_mut().prendi_fuoco(dopo);
        self.versioni.continua_dalla_versione(st.versione);
        if self.fuoco != Fuoco::Campo {
            diagnosi(format!("fuoco: ricerca della Home, finestra v{}", st.versione));
        }
        self.fuoco = Fuoco::Campo;
        self.piattaforma.tipo_tastiera(TipoTastiera::Testo);
        self.piattaforma.set_ime_state(st.testo, st.sel.0, st.sel.1, st.versione);
        self.piattaforma.show_keyboard();
    }

    /// Il fuoco torna al documento (che riprende la tastiera solo con un tocco sul testo).
    fn fuoco_al_documento(&mut self) {
        if self.fuoco == Fuoco::Documento {
            return;
        }
        self.fuoco = Fuoco::Documento;
        diagnosi("fuoco: documento");
        // La tastiera torna quella del testo: cifre e password valgono solo per il loro campo.
        self.piattaforma.tipo_tastiera(TipoTastiera::Testo);
        if let Some(c) = self.campo_mut() {
            c.lascia_fuoco();
            let v = c.versione();
            if let Some(e) = self.editor_mut() {
                e.ponte.continua_dalla_versione(v);
            }
            // Anche il documento che si aprirà dopo (dalla Home) parte oltre il campo.
            self.versioni.continua_dalla_versione(v);
        }
    }

    /// «Trova» dal menu: la barra in alto col campo, che prende la tastiera. Trova lavora in
    /// Scrittura, dove si vede la selezione.
    fn apri_trova(&mut self) {
        let Some(e) = self.editor_mut() else { return };
        e.vista.cambia_modo(Modo::Scrittura);
        if self.trova.is_none() {
            diagnosi("trova: barra aperta");
            self.trova = Some(CampoTesto::new());
        }
        self.fuoco_al_campo();
    }

    fn chiudi_trova(&mut self) {
        if self.trova.is_none() {
            return;
        }
        let campo = self.fuoco == Fuoco::Campo;
        self.fuoco_al_documento();
        self.trova = None;
        self.sostituisci_con = None;
        if campo && self.piattaforma.keyboard_visible() {
            self.piattaforma.hide_keyboard();
        }
        diagnosi("trova: barra chiusa");
    }

    /// Cerca il testo del campo nel documento (nel lettore: nel PDF).
    fn cerca(&mut self) {
        // La password: si riscrive, e «Password sbagliata» se ne va.
        if let Some(p) = self.password_pdf.as_mut() {
            p.sbagliata = false;
            return;
        }
        if self.schermata == Schermata::Pdf {
            self.campo_pdf_cambiato();
            return;
        }
        // Le lettere del campo «Sostituisci con» non cambiano quello che si cerca, né quelle dei
        // campi dei fogli dell'Editor.
        if self.al_sostituisci() || self.campo_word_aperto() {
            return;
        }
        let Some(q) = self.trova.as_ref().map(|c| c.testo().to_string()) else { return };
        if let Some(e) = self.editor_mut() {
            e.trova(&q);
        }
    }

    /// Il pulsante di Trova e sostituisci: apre la seconda riga, col campo «Sostituisci con» che
    /// prende la tastiera, o la chiude.
    fn apri_o_chiudi_sostituisci(&mut self) {
        if self.sostituisci_con.is_some() {
            self.chiudi_sostituisci();
            return;
        }
        diagnosi("sostituisci: riga aperta");
        self.sostituisci_con = Some(CampoTesto::new());
        self.fuoco_al_sostituisci();
    }

    /// Chiude la seconda riga di Trova; se il suo campo aveva la tastiera, passa al campo di Trova
    /// (aperta solo se lo era già).
    fn chiudi_sostituisci(&mut self) {
        if self.sostituisci_con.is_none() {
            return;
        }
        if self.al_sostituisci() {
            let aperta = self.piattaforma.keyboard_visible();
            self.fuoco_a_trova(false, aperta);
        }
        self.sostituisci_con = None;
        diagnosi("sostituisci: riga chiusa");
    }

    /// Il testo cercato e quello che lo sostituisce.
    fn testi_di_sostituisci(&self) -> Option<(String, String)> {
        Some((self.trova.as_ref()?.testo().to_string(), self.sostituisci_con.as_ref()?.testo().to_string()))
    }

    /// «Sostituisci»: il risultato selezionato, poi quello dopo.
    fn sostituisci_uno(&mut self) {
        let Some((q, con)) = self.testi_di_sostituisci() else { return };
        if let Some(e) = self.editor_mut() {
            e.sostituisci(&q, &con);
        }
    }

    /// «Tutti»: ogni risultato, con l'avviso di quanti.
    fn sostituisci_tutti(&mut self) {
        let Some((q, con)) = self.testi_di_sostituisci() else { return };
        let Some(e) = self.editor_mut() else { return };
        let n = e.sostituisci_tutti(&q, &con);
        self.avviso_breve.avvisa(match n {
            0 => "Nessun risultato".to_string(),
            1 => "Sostituito 1".to_string(),
            n => format!("Sostituiti {n}"),
        });
    }

    /// Maiuscole o Parola intera, acceso o spento: si cerca di nuovo.
    fn cambia_opzioni_di_trova(&mut self, o: OpzioniRicerca) {
        let q = self.trova.as_ref().map(|c| c.testo().to_string()).unwrap_or_default();
        if let Some(e) = self.editor_mut() {
            e.cambia_opzioni(&q, o);
        }
    }

    /// Il risultato dopo o prima.
    fn trova_altro(&mut self, avanti: bool) {
        if let Some(e) = self.editor_mut() {
            e.trova_altro(avanti);
        }
    }

    /// «Indice» dal menu: il foglio coi titoli di adesso.
    fn apri_indice(&mut self) {
        let Some(e) = self.editor() else { return };
        self.indice = titoli(&e.s);
        diagnosi(format!("indice: aperto, {} titoli", self.indice.len()));
        self.foglio_aa.chiudi();
        self.foglio_indice.apri();
    }

    /// «Pagina» dal menu, e «Scrittura» per tornare: la vista Pagina è in sola lettura, la
    /// tastiera si chiude.
    fn cambia_vista(&mut self) {
        let Some(e) = self.editor_mut() else { return };
        let modo = if e.vista.modo == Modo::Pagina { Modo::Scrittura } else { Modo::Pagina };
        e.vista.cambia_modo(modo);
        if modo == Modo::Pagina {
            e.ponte.chiudi();
            self.chiudi_trova();
            self.foglio_aa.chiudi();
            self.foglio_inserisci.chiudi();
            if self.piattaforma.keyboard_visible() {
                self.piattaforma.hide_keyboard();
            }
        }
    }

    /// Il documento dell'Editor, o il PDF del lettore, ha modifiche che si perderebbero
    /// chiudendolo: non è una prova e il motore lo dà per modificato.
    fn modifiche_da_salvare(&self) -> bool {
        // Del PDF anche una modifica appena mandata al motore, che l'istantanea non dice ancora.
        let pdf = self.pdf.as_ref().is_some_and(|p| p.motore.stato().modificato || p.motore.modifica_in_corso() || p.copia.recuperato);
        self.aperto.as_ref().is_some_and(|a| a.prova.is_none() && a.editor.s.dirty) || pdf
    }

    /// Chiude il documento dell'Editor e fa `dopo`; se ci sono modifiche da salvare, prima chiede
    /// con il dialogo. Una richiesta nuova col dialogo aperto prende il posto di quella di prima.
    fn chiudi_poi(&mut self, dopo: Dopo) {
        // Il campo di un modulo che si sta scrivendo va al motore prima di chiedere.
        self.conferma_scrittura_pdf(true);
        if !self.modifiche_da_salvare() {
            self.fai(dopo);
            return;
        }
        self.chiudi_sopra_l_editor();
        // Dietro il dialogo la tastiera non scrive nel documento né nel campo di Trova.
        self.fuoco_al_documento();
        if self.piattaforma.keyboard_visible() {
            self.piattaforma.hide_keyboard();
        }
        if let Some(e) = self.editor_mut() {
            e.ponte.chiudi();
        }
        if self.dialogo.replace(dopo).is_none() {
            diagnosi("salvare le modifiche: chiesto");
        }
    }

    fn fai(&mut self, dopo: Dopo) {
        match dopo {
            Dopo::Home => self.torna_alla_home(),
            // Il documento di prima esce solo se quello nuovo si apre.
            Dopo::ApriCon(r) => self.apri_con(&r),
            Dopo::ApriFile(percorso, nome) => self.apri_file(&percorso, &nome),
        }
    }

    fn torna_alla_home(&mut self) {
        let uscito = self.sostituisci(None);
        self.metti_da_parte(uscito);
        self.chiudi_pdf();
        self.chiudi_slide();
        self.chiudi_foglio();
        // Nel frattempo può essere arrivato un file nella cartella aperta.
        self.home.aggiorna();
        self.vai(Schermata::Home);
    }

    fn scegli(&mut self, scelta: Scelta) {
        let Some(dopo) = self.dialogo.take() else { return };
        match scelta {
            Scelta::Salva => {
                diagnosi("salvare le modifiche: salva");
                // Il PDF si salva sul thread del suo motore: si esce quando il salvataggio
                // riesce (`pdf_lavoro::scritto_pdf`); se non riesce si resta, con l'errore.
                if self.pdf.is_some() {
                    self.salva_pdf(pdf_lavoro::AzioneSalva::Salva, Some(dopo));
                    return;
                }
                // Se non riesce si resta nel documento, con l'errore nella testata.
                if self.esegui(Comando::Salva) {
                    self.fai(dopo);
                }
            }
            Scelta::NonSalvare => {
                diagnosi("salvare le modifiche: non salvare");
                self.fai(dopo);
            }
            Scelta::Annulla => diagnosi("salvare le modifiche: annullato"),
        }
    }

    /// Indietro (Esc sul PC): col dialogo aperto vale Annulla; poi chiude il menu e i fogli
    /// dell'Editor; poi la tastiera; poi la selezione; poi esce dalla schermata: dall'Editor alla
    /// Home (chiedendo, se ci sono modifiche), dalla Diagnosi a dov'era, dalla Home manda l'app in
    /// secondo piano.
    fn indietro(&mut self) {
        if self.errore.take().is_some() {
            diagnosi("indietro: errore chiuso");
            return;
        }
        if self.dialogo.is_some() {
            self.scegli(Scelta::Annulla);
            return;
        }
        // L'avviso delle firme di un PDF: vale Annulla.
        if self.annulla_firme_pdf() {
            return;
        }
        // Il riconoscimento del testo che lavora si ferma; la scansione si chiude (o si ferma il
        // PDF che crea).
        if self.ferma_riconosci_pdf() || self.indietro_scansione() {
            return;
        }
        // Il timestamp: il lavoro si lascia andare, il foglio si chiude.
        if self.indietro_data() {
            return;
        }
        // La firma: il foglio col dito, l'anteprima della foto, il foglio della firma.
        if self.indietro_firma_pdf() {
            return;
        }
        // La proposta di accendere i calcoli di un modulo: vale «Non ora».
        if let Some(p) = self.pdf.as_mut()
            && std::mem::take(&mut p.compila.chiedi_calcoli)
        {
            diagnosi("pdf: calcoli, non ora");
            return;
        }
        // Il dialogo della password: prima la tastiera, poi lui.
        if self.password_pdf.is_some() {
            if self.piattaforma.keyboard_visible() {
                self.piattaforma.hide_keyboard();
                diagnosi("indietro: tastiera della password chiusa");
            } else {
                self.annulla_password();
            }
            return;
        }
        // Un PDF che si sta aprendo: Indietro lo lascia andare e si resta dove si era.
        if self.apertura_pdf.take().is_some() {
            diagnosi("indietro: apertura del pdf annullata");
            return;
        }
        if std::mem::take(&mut self.menu_editor) {
            diagnosi("indietro: menu dell'editor chiuso");
            return;
        }
        if self.menu_tabella.take().is_some() {
            diagnosi("indietro: menu della tabella chiuso");
            return;
        }
        if self.menu_allinea.take().is_some() {
            diagnosi("indietro: menu dell'allineamento chiuso");
            return;
        }
        if std::mem::take(&mut self.menu_pdf) {
            diagnosi("indietro: menu del pdf chiuso");
            return;
        }
        if let Some(p) = self.pdf.as_mut()
            && p.link_esterno.take().is_some()
        {
            diagnosi("indietro: link esterno non aperto");
            return;
        }
        // Il modo Compila: il campo che si scrive si conferma e la tastiera si chiude; poi i fogli
        // delle scelte e della data, la proposta di Fill & Sign, il segno scelto.
        if self.pdf.as_ref().is_some_and(|p| p.compila.scrittura.is_some()) {
            self.conferma_scrittura_pdf(true);
            diagnosi("indietro: scrittura del campo confermata");
            return;
        }
        // Il modo Modifica: l'avviso del carattere, i suoi fogli, Oscura, l'oggetto scelto.
        if self.indietro_modifica_pdf() {
            return;
        }
        // Il modo Annota: i suoi fogli, l'annotazione scelta, la penna o la gomma.
        if self.indietro_annota_pdf() {
            return;
        }
        // Pagine e file: il lavoro che si ferma, i dialoghi, i fogli, la pagina presa, la griglia.
        if self.indietro_pagine_pdf() {
            return;
        }
        if self.fogli_pdf.scelta.indietro() || self.fogli_pdf.data.indietro() {
            diagnosi("indietro: foglio del campo chiuso");
            return;
        }
        if let Some(p) = self.pdf.as_mut()
            && (p.compila.proposta.take().is_some() | p.compila.segno.take().is_some())
        {
            diagnosi("indietro: fill & sign chiuso");
            return;
        }
        // I fogli del lettore; «Vai a pagina» lascia anche la tastiera.
        if self.fogli_pdf.vai.aperto() {
            self.fogli_pdf.vai.chiudi();
            diagnosi("indietro: vai a pagina chiuso");
            return;
        }
        if self.fogli_pdf.miniature.indietro() {
            diagnosi("indietro: miniature chiuse");
            return;
        }
        if self.fogli_pdf.indice.indietro() {
            diagnosi("indietro: indice del pdf chiuso");
            return;
        }
        if self.fogli_pdf.riconosci.indietro() {
            diagnosi("indietro: riconosci testo chiuso");
            return;
        }
        if self.fogli_pdf.info.indietro() {
            diagnosi("indietro: informazioni del pdf chiuse");
            return;
        }
        if self.fogli_pdf.definitiva.indietro() {
            diagnosi("indietro: copia definitiva chiusa");
            return;
        }
        if self.fogli_pdf.strumenti.indietro() {
            diagnosi("indietro: strumenti del pdf chiusi");
            return;
        }
        if self.indietro_slide() {
            return;
        }
        if self.foglio_inserisci.indietro() {
            self.tabella_scelta = None;
            self.fonti_immagine = false;
            diagnosi("indietro: foglio inserisci chiuso");
            return;
        }
        // I fogli dell'Editor con un campo: prima la tastiera, poi il foglio (la seconda pagina del
        // foglio «Pagina» torna alla prima).
        if self.campo_word_aperto() && self.fuoco == Fuoco::Campo && self.piattaforma.keyboard_visible() {
            self.piattaforma.hide_keyboard();
            self.fuoco_al_documento();
            diagnosi("indietro: tastiera del campo chiusa");
            return;
        }
        if self.foglio_descrizione.indietro() {
            self.chiudi_campi_word();
            diagnosi("indietro: descrizione chiusa");
            return;
        }
        if self.foglio_commento.indietro() {
            self.chiudi_campi_word();
            diagnosi("indietro: commento chiuso");
            return;
        }
        if self.foglio_revisioni.indietro() {
            diagnosi("indietro: revisioni chiuse");
            return;
        }
        if self.foglio_pagina.aperto() && self.campi_pagina.is_some() {
            self.chiudi_campi_word();
            diagnosi("indietro: intestazione e piè di pagina chiusi");
            return;
        }
        if self.foglio_pagina.indietro() {
            diagnosi("indietro: foglio pagina chiuso");
            return;
        }
        if self.foglio_bordi.indietro() {
            diagnosi("indietro: bordi e sfondo chiuso");
            return;
        }
        if self.foglio_info.indietro() {
            diagnosi("indietro: informazioni chiuse");
            return;
        }
        // Il foglio «Aa» si chiude e l'app resta dov'è, anche con la tastiera aperta.
        if self.foglio_aa.indietro() {
            diagnosi("indietro: foglio del formato chiuso");
            return;
        }
        if self.foglio_indice.indietro() {
            diagnosi("indietro: indice chiuso");
            return;
        }
        // Nella Home il menu di un recente e il foglio di «Nuovo», nelle Impostazioni il foglio
        // della licenza.
        if self.schermata == Schermata::Home && self.home.chiudi_sopra() {
            diagnosi("indietro: menu o foglio della Home chiuso");
            return;
        }
        if self.schermata == Schermata::Impostazioni && self.impostazioni.indietro() {
            diagnosi("indietro: licenza chiusa");
            return;
        }
        if self.piattaforma.keyboard_visible() {
            self.piattaforma.hide_keyboard();
            self.fuoco_al_documento();
            if let Some(e) = self.editor_mut() {
                e.ponte.chiudi();
            }
            return;
        }
        // Poi la selezione del lettore, come la barra della selezione su Android.
        if self.schermata == Schermata::Pdf
            && let Some(p) = self.pdf.as_mut()
            && p.lettore.ha_selezione()
        {
            p.lettore.chiudi_selezione();
            diagnosi("indietro: selezione del pdf chiusa");
            return;
        }
        // Poi la seconda riga di Trova, la barra di Trova, o la ricerca del lettore.
        if self.schermata == Schermata::Editor && self.sostituisci_con.is_some() {
            self.chiudi_sostituisci();
            return;
        }
        if self.schermata == Schermata::Editor && self.trova.is_some() {
            self.chiudi_trova();
            return;
        }
        if self.schermata == Schermata::Pdf && self.pdf.as_ref().is_some_and(|p| p.trova.is_some()) {
            self.chiudi_ricerca_pdf();
            return;
        }
        // Poi la vista Pagina, che torna in Scrittura.
        if self.schermata == Schermata::Editor
            && let Some(e) = self.editor_mut()
            && e.vista.modo == Modo::Pagina
        {
            e.vista.cambia_modo(Modo::Scrittura);
            return;
        }
        // Poi la selezione (o la barra al cursore), come la barra della selezione su Android.
        if self.schermata == Schermata::Editor
            && let Some(e) = self.editor_mut()
            && e.ha_selezione()
        {
            let st = e.chiudi_selezione();
            self.dopo_ponte(st);
            return;
        }
        match self.schermata {
            Schermata::Editor => self.chiudi_poi(Dopo::Home),
            Schermata::Pdf => self.esci_dal_pdf(),
            Schermata::Slide => self.esci_dalle_slide(),
            Schermata::Foglio => self.esci_dal_foglio(),
            Schermata::Diagnosi => self.vai(self.prima_della_diagnosi),
            Schermata::Impostazioni => self.vai(Schermata::Home),
            // Da una cartella si torna a quella di prima, poi alla Home; con una ricerca la si
            // svuota; solo dalla Home così com'è l'app va in secondo piano.
            Schermata::Home => {
                if !self.home.indietro() {
                    self.piattaforma.move_to_background();
                }
            }
        }
    }

    /// Legge, apre e porta nell'Editor un documento (`nome` decide il lettore del motore); se si
    /// apre e ha una `chiave`, va in cima ai recenti. Nella diagnosi tipo e misura del file e
    /// com'è andata, mai il nome, l'URI o il messaggio d'errore, che di solito riporta il nome. Un
    /// errore del lettore del documento va nell'avviso; un errore di lettura torna a chi chiama,
    /// che sa che cosa dire.
    fn apri_documento(
        &mut self,
        da: Da,
        leggi: impl FnOnce(&Piattaforma) -> Result<Vec<u8>, ErroreLettura>,
        nome: &str,
        chiave: Option<String>,
        origine: Origine,
    ) -> Result<(), ErroreLettura> {
        // «apertura➡pagina» conta anche lettura e conversione del file.
        let inizio = Instant::now();
        // Un altro documento al posto del PDF che chiedeva la password.
        self.annulla_password();
        let etichetta = da.etichetta();
        let tipo = documento::estensione_nota(nome).unwrap_or_else(|| "sconosciuto".to_string());
        let byte = match leggi(&self.piattaforma) {
            Ok(byte) => byte,
            Err(e) => {
                da.scrivi(format!("{etichetta}: errore di lettura ({}), tipo {tipo}", e.genere));
                return Err(e);
            }
        };
        // Anche un file che si chiama PDF (o arriva col tipo del PDF) ma non comincia con %PDF va
        // al lettore PDF, che dice in italiano che è vuoto o rovinato: Word apriva un documento
        // vuoto, o il codice di una pagina d'errore del web (analisi del 27/10, lettura I4).
        if tascabile_pdf::sembra_pdf(&byte) || tipo_da_nome(nome) == Tipo::Pdf {
            self.apri_pdf(da, byte, nome, chiave, inizio);
            return Ok(());
        }
        // I fogli di calcolo: per nome, o per contenuto (un pacchetto di Excel, un .xls, un
        // OpenDocument di fogli) se il nome non lo dice.
        if tipo_da_nome(nome) == Tipo::Foglio || (tipo_da_nome(nome) != Tipo::Documento && tascabile_foglio::sembra_foglio(&byte)) {
            self.apri_foglio(da, byte, nome, chiave, inizio);
            return Ok(());
        }
        // Le presentazioni alle Slide: per nome (anche .ppt e .odp, che così dicono perché non si
        // aprono) o per contenuto, se il nome non lo dice.
        if tipo_da_nome(nome) == Tipo::Presentazione || tascabile_slide::sembra_presentazione(&byte) {
            self.apri_slide(da, byte, nome, chiave, inizio);
            return Ok(());
        }
        match documento::apri_byte(nome, &byte) {
            Ok(s) => {
                da.scrivi(format!(
                    "{etichetta}: {} byte, tipo {tipo}, aperto in {:.0} ms, {} paragrafi",
                    byte.len(),
                    inizio.elapsed().as_secs_f64() * 1000.0,
                    s.doc.paragraph_count()
                ));
                let mut editor = Editor::con_origine(s, nome, origine);
                editor.aperto_alle = inizio;
                self.cambia_documento(editor);
                // Il motore salta commenti e note di .odt e .rtf, e Salva li scrive in un .docx
                // accanto: va detto subito, prima che si cerchino.
                if matches!(tipo.as_str(), "odt" | "rtf") {
                    self.avviso_breve.avvisa("Di questo file qui non si vedono tutte le parti, come commenti e note: Salva fa un .docx accanto.");
                }
                match chiave {
                    Some(chiave) => {
                        self.home.ricorda(Recente { chiave, nome: nome.to_string(), tipo: tipo_da_nome(nome), aperto_alle: adesso(), pagine: None })
                    }
                    None => diagnosi("recenti: percorso che non è UTF-8, non ricordato"),
                }
            }
            Err(e) => {
                // Il genere sì (zip troppo grande, protetto da password), il messaggio no: quello
                // del motore può riportare il nome del file.
                da.scrivi(format!("{etichetta}: {} byte, tipo {tipo}, errore di apertura ({})", byte.len(), e.genere));
                self.errore = Some(Errore::apertura(format!("Non riesco ad aprire il file: {e}")));
            }
        }
        Ok(())
    }

    fn apri_con(&mut self, r: &OpenRequest) {
        let nome = nome_di_apri_con(r);
        let letti = self.piattaforma.read_uri(&r.uri);
        // Lo stesso file nelle cartelle del telefono (WhatsApp, Telegram, Download): si apre da
        // lì, così resta nei recenti anche quando il permesso di «Apri con» se ne va, dopo un
        // aggiornamento dell'app (segnalazione dell'autore del 3/11), e Salva scrive nel file.
        let sul_telefono = match &letti {
            Ok(byte) if self.home.accesso() => tascabile_files::lo_stesso_file(&self.piattaforma.radice_archivio(), &nome, byte),
            _ => None,
        };
        let esito = match sul_telefono {
            Some(percorso) => {
                diagnosi("apri con: lo stesso file è nelle cartelle del telefono, aperto da lì");
                let nome = percorso.file_name().and_then(|n| n.to_str()).map_or(nome.clone(), str::to_string);
                let chiave = percorso.to_str().map(str::to_string);
                self.apri_documento(Da::ApriCon, move |_| letti, &nome, chiave, Origine::File(percorso))
            }
            None => {
                let origine = Origine::Uri { uri: r.uri.clone(), nome: nome.clone() };
                self.apri_documento(Da::ApriCon, move |_| letti, &nome, Some(r.uri.clone()), origine)
            }
        };
        if let Err(e) = esito {
            self.errore = Some(Errore::apertura(format!("Non riesco ad aprire il file: {e}")));
        }
    }

    /// Un documento toccato nel navigatore.
    fn apri_file(&mut self, percorso: &Path, nome: &str) {
        let chiave = percorso.to_str().map(str::to_string);
        let origine = Origine::File(percorso.to_path_buf());
        if let Err(e) = self.apri_documento(Da::Cartella, |_| leggi_file(percorso), nome, chiave, origine) {
            self.errore = Some(Errore::apertura(format!("Non riesco ad aprire il file: {e}")));
        }
    }

    /// Un recente toccato nella Home. Un URI che non si legge più (il permesso di un «Apri con»
    /// non dura) esce dai recenti, e l'avviso dice da dove riaprirlo. Un file che non si trova
    /// invece resta: «non trovato» arriva anche per un file che c'è, senza l'accesso ai file o
    /// subito dopo un aggiornamento dell'app (segnalato dall'autore il 1/11: i file «non esistevano
    /// più» e al tocco dopo si aprivano). Si riprova dopo un attimo, e lo si toglie dal suo menu.
    fn apri_recente(&mut self, r: Recente) {
        let uri = home::e_uri(&r.chiave);
        let esito = if uri {
            let origine = Origine::Uri { uri: r.chiave.clone(), nome: r.nome.clone() };
            self.apri_documento(Da::Recenti, |p| p.read_uri(&r.chiave), &r.nome, Some(r.chiave.clone()), origine)
        } else {
            let origine = Origine::File(PathBuf::from(&r.chiave));
            let leggi = |_: &Piattaforma| {
                leggi_file(Path::new(&r.chiave)).or_else(|e| {
                    if e.genere != "NotFound" {
                        return Err(e);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(250));
                    leggi_file(Path::new(&r.chiave))
                })
            };
            self.apri_documento(Da::Recenti, leggi, &r.nome, Some(r.chiave.clone()), origine)
        };
        let Err(e) = esito else { return };
        if uri && non_si_apre_piu(&e.genere) {
            self.home.dimentica(&r.chiave);
            // Lo stesso nome nelle cartelle del telefono (WhatsApp, Download…): si apre da lì, e il
            // recente diventa il file (segnalazione dell'autore del 3/11).
            if self.home.accesso()
                && let Some(percorso) = tascabile_files::per_nome(&self.piattaforma.radice_archivio(), &r.nome)
            {
                diagnosi("recenti: un documento arrivato da un'altra app ritrovato nelle cartelle del telefono");
                self.apri_file(&percorso, &r.nome);
                return;
            }
            diagnosi("recenti: tolto un documento arrivato da un'altra app, che non si apre più");
            self.errore = Some(Errore::apertura(match home::provenienza(&r.chiave) {
                Some(app) => format!("Non si apre più: riaprilo da {app}."),
                None => "Non si apre più: riaprilo dall'app da cui era arrivato.".to_string(),
            }));
        } else if !uri && e.genere == "NotFound" {
            diagnosi("recenti: file non trovato, lasciato nei recenti");
            self.errore = Some(Errore::apertura(NON_TROVATO));
        } else {
            self.errore = Some(Errore::apertura(format!("Non riesco ad aprire il file: {e}")));
        }
    }

    fn azione_della_home(&mut self, azione: Azione) {
        match azione {
            Azione::Nuovo => self.nuovo_documento(),
            Azione::NuovoDaModello(m) => self.nuovo_da_modello(m),
            Azione::Scansiona => self.apri_scansione(),
            Azione::ApriFile { percorso, nome } => self.apri_file(&percorso, &nome),
            Azione::ApriRecente(r) => self.apri_recente(r),
            Azione::CondividiRecente(r) => self.condividi_recente(&r),
            Azione::ConcediAccesso => self.piattaforma.chiedi_accesso_file(),
            Azione::Impostazioni => self.vai(Schermata::Impostazioni),
            Azione::Cerca => self.fuoco_alla_ricerca(),
            Azione::SvuotaRicerca => {
                if let Some(st) = self.home.ricerca_mut().svuota() {
                    self.versioni.continua_dalla_versione(st.versione);
                    self.piattaforma.set_ime_state(st.testo, st.sel.0, st.sel.1, st.versione);
                }
            }
        }
    }

    /// «Condividi» nel menu di un recente: il file com'è sul disco (o com'è arrivato da un'altra
    /// app) in `condivisi/` e al foglio di sistema. Nella diagnosi tipo e misura, mai il nome.
    fn condividi_recente(&mut self, r: &Recente) {
        let cache = self.piattaforma.cartella_cache();
        let tipo = documento::estensione_nota(&r.nome).unwrap_or_else(|| "sconosciuto".to_string());
        let esito = if home::e_uri(&r.chiave) {
            match self.piattaforma.read_uri(&r.chiave) {
                Ok(byte) => salva::byte_in_condivisi(&r.nome, &byte, &cache),
                Err(e) => Err(ErroreSalva { genere: e.genere, messaggio: e.messaggio }),
            }
        } else {
            salva::copia_in_condivisi(Path::new(&r.chiave), &cache)
        };
        scrivi_esito("condividi dai recenti", &tipo, &esito);
        match esito {
            Ok(c) => self.piattaforma.condividi_file(&c.percorso, salva::mime(&c.tipo), "Condividi il documento"),
            Err(e) => self.errore = Some(Errore { titolo: NON_CONDIVISO, testo: format!("Non riesco a condividere il file: {e}") }),
        }
    }

    /// Una scelta delle Impostazioni.
    fn scelta_delle_impostazioni(&mut self, scelta: impostazioni::Scelta) {
        use impostazioni::Scelta;
        match scelta {
            Scelta::Indietro => self.vai(Schermata::Home),
            Scelta::Registro => self.apri_diagnosi(),
            Scelta::CondividiDiagnosi => self.piattaforma.condividi_testo(&diagnosi_testo(), &format!("Diagnosi di {NOME_APP}")),
            Scelta::Checklist => self.apri_checklist(),
            Scelta::Lungo => self.apri_lungo(),
            Scelta::CancellaFirma => self.cancella_firma_salvata(),
            Scelta::Autore => {
                diagnosi("impostazioni: autore aperto");
                self.impostazioni.campo_autore = CampoTesto::new().con_testo(&self.impostazioni.autore);
                self.impostazioni.foglio_autore.apri();
                self.impostazioni.autore_aperto = true;
                self.fuoco_al_campo_autore();
            }
            Scelta::CampoAutore => self.fuoco_al_campo_autore(),
            Scelta::AutoreFatto => {
                let nome = impostazioni::pulito(self.impostazioni.campo_autore.testo());
                self.scrivi_l_autore(nome);
            }
            Scelta::TogliAutore => self.scrivi_l_autore(String::new()),
            Scelta::Riservata => {
                let accesa = !self.impostazioni.riservata;
                match impostazioni::scrivi_riservata(&self.piattaforma.cartella_app(), accesa) {
                    Ok(()) => {
                        diagnosi(if accesa { "impostazioni: modalità riservata accesa" } else { "impostazioni: modalità riservata spenta" });
                        self.impostazioni.riservata = accesa;
                        self.piattaforma.riservata(accesa);
                    }
                    Err(e) => {
                        diagnosi(format!("impostazioni: modalità riservata non scritta ({:?})", e.kind()));
                        self.errore = Some(Errore { titolo: NON_SALVATO, testo: "Non riesco a salvare la scelta nelle Impostazioni.".to_string() });
                    }
                }
            }
            Scelta::ImprontaFirma => self.tocca_impronta_firma(),
            Scelta::Blocco => self.tocca_blocco(),
            Scelta::ReteTimestamp => {
                let accesa = !self.impostazioni.rete_timestamp;
                match impostazioni::scrivi_rete_timestamp(&self.piattaforma.cartella_app(), accesa) {
                    Ok(()) => {
                        diagnosi(if accesa { "impostazioni: rete per il timestamp accesa" } else { "impostazioni: rete per il timestamp spenta" });
                        self.impostazioni.rete_timestamp = accesa;
                    }
                    Err(e) => {
                        diagnosi(format!("impostazioni: rete per il timestamp non scritta ({:?})", e.kind()));
                        self.errore = Some(Errore { titolo: NON_SALVATO, testo: "Non riesco a salvare la scelta nelle Impostazioni.".to_string() });
                    }
                }
            }
            Scelta::Ortografia => {
                let acceso = !self.impostazioni.ortografia;
                match impostazioni::scrivi_ortografia(&self.piattaforma.cartella_app(), acceso) {
                    Ok(()) => {
                        diagnosi(if acceso { "impostazioni: ortografia accesa" } else { "impostazioni: ortografia spenta" });
                        self.impostazioni.ortografia = acceso;
                        if let Some(e) = self.editor_mut() {
                            e.controllo_ortografico = acceso;
                        }
                        if let Some(c) = self.checklist.as_mut() {
                            c.controllo_ortografico = acceso;
                        }
                    }
                    Err(e) => {
                        diagnosi(format!("impostazioni: ortografia non scritta ({:?})", e.kind()));
                        self.errore = Some(Errore { titolo: NON_SALVATO, testo: "Non riesco a salvare la scelta nelle Impostazioni.".to_string() });
                    }
                }
            }
        }
    }

    /// Il campo dell'autore prende la tastiera, come la ricerca della Home.
    fn fuoco_al_campo_autore(&mut self) {
        let dopo = self.versioni.versione();
        let st = self.impostazioni.campo_autore.prendi_fuoco(dopo);
        self.versioni.continua_dalla_versione(st.versione);
        if self.fuoco != Fuoco::Campo {
            diagnosi(format!("fuoco: autore dei commenti, finestra v{}", st.versione));
        }
        self.fuoco = Fuoco::Campo;
        self.piattaforma.tipo_tastiera(TipoTastiera::Testo);
        self.piattaforma.set_ime_state(st.testo, st.sel.0, st.sel.1, st.versione);
        self.piattaforma.show_keyboard();
    }

    /// L'autore dei commenti scritto (vuoto: tolto), nella cartella dell'app; il foglio si chiude.
    /// Nella diagnosi mai il nome.
    fn scrivi_l_autore(&mut self, nome: String) {
        let esito = impostazioni::scrivi_autore(&self.piattaforma.cartella_app(), &nome);
        match &esito {
            Ok(()) => diagnosi(if nome.is_empty() { "impostazioni: autore tolto" } else { "impostazioni: autore scritto" }),
            Err(e) => diagnosi(format!("impostazioni: autore non scritto ({:?})", e.kind())),
        }
        if esito.is_ok() {
            // Anche nel documento Word aperto (o messo da parte), per i commenti da qui in poi.
            for e in [self.aperto.as_mut().map(|a| &mut a.editor), self.checklist.as_mut()].into_iter().flatten() {
                e.s.author = nome.clone();
            }
            self.impostazioni.autore = nome;
        } else {
            self.errore = Some(Errore { titolo: NON_SALVATO, testo: "Non riesco a salvare il nome nelle Impostazioni.".to_string() });
        }
        self.chiudi_foglio_autore();
    }

    fn chiudi_foglio_autore(&mut self) {
        let campo = self.fuoco == Fuoco::Campo;
        self.fuoco_al_documento();
        self.impostazioni.foglio_autore.chiudi();
        self.impostazioni.autore_aperto = false;
        if campo && self.piattaforma.keyboard_visible() {
            self.piattaforma.hide_keyboard();
        }
    }

    /// «Riapri» di una copia di recupero proposta nella Home.
    fn riapri_recupero(&mut self, i: usize) {
        if let Some(c) = self.recupero.prendi_pdf(i) {
            self.riapri_pdf_recuperato(c);
            return;
        }
        match self.recupero.riapri(i) {
            Ok(r) => {
                diagnosi(format!("recupero: riaperto, {} paragrafi", r.s.doc.paragraph_count()));
                let mut editor = Editor::con_origine(r.s, r.nome, r.origine);
                // Il lavoro recuperato non è salvato da nessuna parte.
                editor.s.dirty = true;
                // La nostra copia ha già dentro questa revisione: non si riscrive finché non cambia.
                let rev_copia = r.copia.is_some().then(|| editor.s.rev());
                // La copia del motore riaperta il motore la continua: è del documento.
                let motore_altri = r.id_motore.map(|id| editor.s.recovery_documents().into_iter().map(|d| d.id).filter(|d| *d != id).collect());
                self.apri(Aperto { editor, prova: None, recupero: r.recupero_acceso, copia: r.copia, rev_copia, rev_vista: None, motore_altri });
            }
            Err(genere) => {
                diagnosi(format!("recupero: non riaperto ({genere})"));
                self.errore = Some(Errore { titolo: NON_RECUPERATO, testo: "Non riesco a riaprire la copia di recupero.".to_string() });
            }
        }
    }

    fn scarta_recupero(&mut self, i: usize) {
        self.recupero.scarta(i);
    }

    /// Il formato che scriverà `comando`, per la riga della diagnosi anche quando non riesce.
    fn tipo_di(&self, comando: Comando) -> String {
        match (comando, self.editor()) {
            (Comando::EsportaPdf, _) => "pdf".to_string(),
            (_, Some(e)) => salva::formato_della_copia(&e.origine),
            (_, None) => "docx".to_string(),
        }
    }

    /// Esegue un comando che scrive un file e lo dice nella diagnosi (esito, genere dell'errore,
    /// tipo e misura, mai il nome) e sullo schermo. Vero se è riuscito.
    fn esegui(&mut self, comando: Comando) -> bool {
        let radice = self.piattaforma.radice_archivio();
        let cache = self.piattaforma.cartella_cache();
        let tipo = self.tipo_di(comando);
        let ripiego = salva::nome_di_ripiego_adesso();
        let Some(a) = self.aperto.as_mut() else { return false };
        let e = &mut a.editor;
        let prima = e.origine.clone();
        let esito = match comando {
            Comando::Salva => salva::salva(&mut e.s, &mut e.origine, &radice, &ripiego),
            Comando::SalvaCopia => salva::salva_copia(&mut e.s, &mut e.origine, &radice, &ripiego),
            Comando::EsportaPdf => salva::esporta_pdf(&e.s, &e.origine, &radice, &ripiego),
            Comando::Condividi => salva::prepara_condivisione(&e.s, &e.origine, &cache, &ripiego),
        };
        tascabile_word::editor::svuota_richieste(&mut e.s);
        scrivi_esito(comando.etichetta(), &tipo, &esito);
        let r = match esito {
            Ok(r) => r,
            Err(err) => {
                self.errore = Some(Errore { titolo: NON_SALVATO, testo: format!("Non riesco a salvare: {err}") });
                return false;
            }
        };
        // Un file nuovo (la copia, il .docx al posto di un .doc) diventa il documento: nome nella
        // testata e nei recenti al posto di quello di prima, se era arrivato da un'altra app.
        if matches!(r.come, salva::Come::Copia | salva::Come::ComeDocx) {
            let nome = r.percorso.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            e.nome = nome.clone();
            e.s.path = Some(r.percorso.clone());
            if let Origine::Uri { uri, .. } = &prima {
                self.home.dimentica(uri);
            }
            match r.percorso.to_str() {
                Some(chiave) => {
                    self.home.ricorda(Recente { chiave: chiave.to_string(), tipo: tipo_da_nome(&nome), nome, aperto_alle: adesso(), pagine: None })
                }
                None => diagnosi("recenti: percorso che non è UTF-8, non ricordato"),
            }
        }
        if let Some(a) = self.aperto.as_mut()
            && !a.editor.s.dirty
            && let Some(c) = a.copia.take()
        {
            c.togli();
        }
        match r.come {
            salva::Come::SulFile => self.avviso_breve.avvisa("Salvato"),
            salva::Come::ComeDocx => self.avviso_breve.avvisa(format!("Salvato come documento Word nella stessa cartella: {}", file_di(&r))),
            salva::Come::Copia if comando == Comando::SalvaCopia => {
                self.avviso_breve.avvisa(format!("Copia salvata in Documenti/Tascabile: {}", file_di(&r)));
            }
            salva::Come::Copia => self.avviso_breve.avvisa(format!("Salvato in Documenti/Tascabile: {}", file_di(&r))),
            salva::Come::Pdf => {
                self.avviso_breve.avvisa_con_azione(format!("PDF pronto in Documenti/Tascabile: {}", file_di(&r)), CONDIVIDI_PDF);
                self.pdf_pronto = Some(r.percorso.clone());
            }
            // Il foglio di condivisione del sistema è già l'esito.
            salva::Come::Condivisione => self.piattaforma.condividi_file(&r.percorso, salva::mime(&r.tipo), "Condividi il documento"),
        }
        true
    }

    /// «Condividi il PDF» dopo «Esporta PDF»: il PDF va in una sottocartella nuova di `condivisi/`
    /// e al foglio di sistema.
    fn condividi_pdf(&mut self) {
        let Some(pdf) = self.pdf_pronto.take() else { return };
        let esito = salva::copia_in_condivisi(&pdf, &self.piattaforma.cartella_cache());
        scrivi_esito("condividi pdf", "pdf", &esito);
        match esito {
            Ok(r) => self.piattaforma.condividi_file(&r.percorso, "application/pdf", "Condividi il PDF"),
            Err(e) => self.errore = Some(Errore { titolo: NON_CONDIVISO, testo: format!("Non riesco a condividere il PDF: {e}") }),
        }
    }

    /// I numeri delle richieste di foto che qualcuno aspetta ancora: la firma, l'immagine e il PDF
    /// da unire del lettore, l'immagine di Word, la scansione.
    fn foto_attese(&self) -> [Option<i32>; 5] {
        let pdf = self.pdf.as_ref();
        [
            pdf.and_then(|p| p.compila.firma.foto.map(|(_, r)| r)),
            pdf.and_then(|p| p.modifica.foto.map(|(_, r)| r)),
            pdf.and_then(|p| p.pagine.attende_pdf),
            match self.immagine_word {
                Some(foglio_immagine::AttesaImmagine::Foto(r)) => Some(r),
                _ => None,
            },
            self.scansione.as_ref().and_then(|s| s.attende_foto),
        ]
    }

    /// L'esito della richiesta di foto in corso, se nessuno lo aspetta più (chi l'aveva chiesta se
    /// n'è andato: il PDF chiuso, la X della scansione): via le sue foto. Dopo tutti quelli che
    /// aspettano: un esito arrivato in mezzo al passo resta a chi lo aspetta, per il passo dopo.
    fn butta_le_foto_di_nessuno(&mut self) {
        let in_corso = self.piattaforma.richiesta_foto_in_corso();
        if in_corso <= 0 || self.foto_attese().contains(&Some(in_corso)) {
            return;
        }
        match self.piattaforma.prendi_foto_firma(in_corso) {
            Some(tascabile_platform::EsitoFoto::Pronta(file)) => {
                butta_la_foto(&file);
                diagnosi("foto: arrivata senza attesa, cancellata");
            }
            Some(tascabile_platform::EsitoFoto::Pronte { file, .. }) => butta_le_foto(&file, "foto"),
            _ => {}
        }
    }

    /// Quando l'app perde il fuoco con modifiche non salvate: la nostra copia privata, perché la
    /// copia del motore può avere fino a 10 secondi di ritardo. Il file dell'utente non si tocca.
    fn copia_privata(&mut self) {
        if !self.modifiche_da_salvare() {
            return;
        }
        let cartella = self.piattaforma.cartella_app().join(recupero::CARTELLA_COPIE);
        let Some(a) = self.aperto.as_mut() else { return };
        // Già scritta con questa revisione e le stesse gemelle: niente da rifare (al ritorno
        // nell'app non blocca il frame).
        let rev = a.editor.s.rev();
        let id_motore = a.id_motore();
        if a.rev_copia == Some(rev) && a.copia.as_ref().is_some_and(|c| c.id_motore == id_motore) {
            return;
        }
        let id = match &a.copia {
            Some(c) => c.file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
            None => {
                self.copie_scritte += 1;
                format!("{}-{}-{}", adesso(), std::process::id(), self.copie_scritte)
            }
        };
        let inizio = Instant::now();
        // Il nome che il documento aveva (vuoto se nuovo): torna nell'origine del recuperato.
        let nome = match &a.editor.origine {
            Origine::File(p) => p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            Origine::Uri { nome, .. } | Origine::Recuperato { nome } => nome.clone(),
            Origine::Nuovo => String::new(),
        };
        match Copia::scrivi(&cartella, &id, &nome, &id_motore, &a.editor.s) {
            Ok(c) => {
                let byte = std::fs::metadata(&c.file).map_or(0, |m| m.len());
                diagnosi(format!("recupero: copia dell'app scritta, {byte} byte in {:.0} ms", inizio.elapsed().as_secs_f64() * 1000.0));
                a.copia = Some(c);
                a.rev_copia = Some(rev);
            }
            Err(genere) => diagnosi(format!("recupero: copia dell'app non scritta ({genere})")),
        }
    }

    /// A ogni passo: il recupero all'avvio, quello del motore per il documento aperto (con un
    /// risveglio quando scade la prossima copia) e la nostra copia, tolta quando il documento è
    /// tornato pulito.
    fn passo_recupero(&mut self, ctx: &egui::Context) {
        self.recupero.passo();
        self.copia_dopo_le_modifiche(ctx);
        let Some(a) = self.aperto.as_mut() else { return };
        if a.recupero {
            let errore_prima = a.editor.s.recovery_error().is_some();
            a.editor.s.recovery_tick();
            if !errore_prima && a.editor.s.recovery_error().is_some() {
                diagnosi("recupero: errore del motore nella copia");
            }
            if let Some(d) = a.editor.s.recovery_wait() {
                ctx.request_repaint_after(d);
            }
        }
        if !a.editor.s.dirty
            && let Some(c) = a.copia.take()
        {
            c.togli();
            a.rev_copia = None;
        }
    }

    /// La nostra copia `ritardo_copia` dopo l'ultima modifica non salvata, con l'app in primo piano:
    /// dopo onPause su Android non gira più nessun frame, e la copia del motore può avere fino a 10
    /// secondi di ritardo.
    fn copia_dopo_le_modifiche(&mut self, ctx: &egui::Context) {
        let ritardo = self.ritardo_copia;
        let Some(a) = self.aperto.as_mut() else { return };
        // Le copie che c'erano già, appena il motore le ha lette: prima di ogni modifica, quindi
        // prima che il motore scriva la sua.
        if a.recupero && a.motore_altri.is_none() && a.editor.s.recovery_ready() && !a.editor.s.recovery_busy() {
            a.motore_altri = Some(a.editor.s.recovery_documents().into_iter().map(|r| r.id).collect());
        }
        if a.prova.is_some() || !a.editor.s.dirty {
            a.rev_vista = None;
            return;
        }
        let rev = a.editor.s.rev();
        if a.rev_copia == Some(rev) {
            // Il motore ha scritto la sua prima copia dopo la nostra: la nostra la segna.
            if a.copia.as_ref().is_some_and(|c| c.id_motore != a.id_motore()) {
                self.copia_privata();
            }
            return;
        }
        let adesso = Instant::now();
        let da = match a.rev_vista {
            Some((r, t)) if r == rev => t,
            _ => {
                a.rev_vista = Some((rev, adesso));
                adesso
            }
        };
        let passato = adesso.saturating_duration_since(da);
        if passato < ritardo {
            ctx.request_repaint_after(ritardo - passato);
            return;
        }
        self.copia_privata();
    }

    /// Quando l'app ritrova il fuoco (di ritorno dalle impostazioni, da WhatsApp, dalla tendina)
    /// si ricontrolla l'accesso ai file e, se è sullo schermo, si rilegge la Home; dalle altre
    /// schermate la rilegge il ritorno nella Home. La riga in testa al registro cambia solo se è
    /// cambiato l'accesso.
    fn ritrova_il_fuoco(&mut self, ctx: &egui::Context) {
        let fuoco = ctx.input(|i| i.focused);
        let ritrovato = fuoco && !self.aveva_il_fuoco;
        let perso = !fuoco && self.aveva_il_fuoco;
        self.aveva_il_fuoco = fuoco;
        self.fuoco_e_blocco(perso, ritrovato);
        if perso {
            self.copia_privata();
            // Il campo che si scrive va al motore (la scrittura resta aperta), poi la copia di
            // recupero del PDF e la posizione.
            self.conferma_scrittura_pdf(false);
            self.copia_pdf_subito();
            self.ricorda_posizione_pdf(true);
        }
        if !ritrovato {
            return;
        }
        let accesso = self.piattaforma.accesso_file();
        if accesso != self.home.accesso() {
            diagnosi_fissa(Fissa::AccessoFile, format!("accesso ai file al ritorno nell'app: {}", concesso(accesso)));
        }
        self.home.imposta_accesso(accesso);
        if self.schermata == Schermata::Home {
            self.home.aggiorna();
        }
    }

    /// Una riga per ogni dito che si alza: dove si è appoggiato, quanto si è spostato, quanto è
    /// rimasto giù; o che il sistema ha annullato il tocco. Anche per i tocchi che egui non prende
    /// per un clic (spostati troppo, troppo lunghi, sulla pagina che scorre ancora), che
    /// `Editor::tocca` non vede: se dopo questa riga non c'è quella del tocco sul testo, il tocco
    /// non è arrivato all'editor.
    fn dita_nella_diagnosi(&mut self, ctx: &egui::Context) {
        // Fuori dai passi (finestra nascosta) eframe chiama `logic` con l'input dell'ultimo passo,
        // che non va contato di nuovo: il passo si riconosce dal suo istante, come in `tasti_crudi`.
        let istante = ctx.input(|i| i.time);
        if self.ultimo_input.replace(istante) == Some(istante) {
            return;
        }
        // Solo gli eventi del puntatore: degli altri (anche il testo battuto) qui non serve niente.
        let eventi: Vec<egui::Event> = ctx.input(|i| {
            let del_puntatore =
                |e: &&egui::Event| matches!(e, egui::Event::PointerButton { .. } | egui::Event::PointerMoved(_) | egui::Event::PointerGone);
            i.events.iter().filter(del_puntatore).cloned().collect()
        });
        for evento in eventi {
            match evento {
                egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, .. } => {
                    self.dito = Some(DitoGiu { dove: pos, alle: istante, spostato: 0.0 });
                }
                egui::Event::PointerMoved(pos) => {
                    if let Some(d) = &mut self.dito {
                        d.spostato = d.spostato.max(d.dove.distance(pos));
                    }
                }
                egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: false, .. } => match self.dito.take() {
                    Some(d) => diagnosi(format!(
                        "dito: x {:.0} dp, y {:.0} dp, spostato fino a {:.0} dp, per {:.2} s",
                        d.dove.x,
                        d.dove.y,
                        d.spostato.max(d.dove.distance(pos)),
                        istante - d.alle
                    )),
                    None => diagnosi(format!("dito: alzato a x {:.0} dp, y {:.0} dp (appoggio non visto)", pos.x, pos.y)),
                },
                egui::Event::PointerGone => {
                    if let Some(d) = self.dito.take() {
                        diagnosi(format!("dito: x {:.0} dp, y {:.0} dp, annullato dopo {:.2} s", d.dove.x, d.dove.y, istante - d.alle));
                    }
                }
                _ => {}
            }
        }
    }

    /// Tutto ciò che non disegna: tocchi per la diagnosi, fuoco, «Apri con» e tastiera.
    pub fn passo_logica(&mut self, ctx: &egui::Context) {
        let _ = self.contesto.set(ctx.clone());
        self.dita_nella_diagnosi(ctx);
        self.ritrova_il_fuoco(ctx);
        self.passo_blocco(ctx);
        self.passo_recupero(ctx);
        self.passo_dizionario();
        self.passo_apertura_pdf();
        self.passo_apertura_slide();
        self.passo_apertura_foglio();
        self.passo_pdf_slide();
        self.passo_motore_pdf();
        self.passo_copia_pdf(ctx);
        // Le foto chieste ad Android: ognuno prende l'esito della sua richiesta; dopo tutti, via
        // quello che non aspetta più nessuno.
        self.passo_immagine_pdf(ctx);
        self.passo_immagine_word(ctx);
        self.passo_unisci_pdf(ctx);
        self.passo_scansione(ctx);
        self.passo_firma_pdf(ctx);
        self.butta_le_foto_di_nessuno();
        self.passo_riconosci_pdf();
        self.passo_data();
        self.passo_word_pdf();
        self.passo_file_pdf();
        self.passo_posizioni();
        // App bloccata: «Apri con» aspetta lo sblocco, e i tasti e la tastiera non vanno a nessuno.
        if self.blocco.bloccata {
            let _ = self.piattaforma.tasti_crudi(ctx);
            let _ = self.piattaforma.take_ime_snapshot();
            return;
        }
        match self.piattaforma.take_open_request() {
            Some(Ok(r)) => {
                // Il nome vero (DISPLAY_NAME) mai: solo se c'è.
                let nome = if r.nome.is_some() { ", col nome" } else { "" };
                diagnosi(format!("apri con: richiesta, tipo {}{nome}", r.mime.as_deref().unwrap_or("non indicato")));
                self.chiudi_poi(Dopo::ApriCon(r));
            }
            // Rifiutata (un file://, il nostro FileProvider): prima lo diceva solo la diagnosi.
            Some(Err(_)) => self.avviso_breve.avvisa(APRI_CON_RIFIUTATO),
            None => {}
        }
        // Sul PC questa chiamata fa scrivere la tastiera finta (e prepara lo stato); su Android
        // raccoglie i tasti che GameTextInput lascia passare.
        let crudi = self.piattaforma.tasti_crudi(ctx);
        // Prima lo stato della tastiera, poi i tasti crudi: al contrario il tasto crudo aprirebbe
        // una finestra nuova e renderebbe vecchio lo stato già arrivato.
        if let Some(snap) = self.piattaforma.take_ime_snapshot() {
            let quale = match self.schermata {
                _ if self.password_pdf.is_some() => "della password",
                Schermata::Home => "della ricerca",
                Schermata::Pdf => "del pdf",
                _ => self.campo_di_trova(),
            };
            if self.fuoco == Fuoco::Campo
                && let Some(c) = self.campo_mut()
            {
                // Al campo, non al documento. Nella diagnosi solo misure e versioni; di una
                // password nemmeno quanto è lunga.
                let misura = if c.e_segreto() { String::new() } else { format!(", {} unità", snap.text.encode_utf16().count()) };
                let r = c.ricevi(&snap.text, snap.sel, snap.version);
                let com_e = if r.scartato {
                    "scartato"
                } else if r.cambiato {
                    "testo cambiato"
                } else {
                    "applicato"
                };
                let invio = if r.invio { ", invio" } else { "" };
                diagnosi(format!("tastiera: stato v{} al campo {quale}{misura}, {com_e}{invio}", snap.version));
                if let Some(st) = r.stato {
                    let v = st.versione;
                    self.piattaforma.set_ime_state(st.testo, st.sel.0, st.sel.1, v);
                    if let Some(e) = self.editor_mut() {
                        e.ponte.continua_dalla_versione(v);
                    }
                    self.versioni.continua_dalla_versione(v);
                }
                if r.cambiato {
                    self.cerca();
                }
                if r.invio {
                    self.invio_nel_campo();
                }
            } else {
                match self.editor_mut() {
                    Some(e) => {
                        let st = e.ricevi(&snap);
                        self.dopo_ponte(st);
                    }
                    None => diagnosi(format!("tastiera: stato v{} senza un documento aperto, scartato", snap.version)),
                }
            }
        }
        for t in crudi {
            self.tasto_crudo(t);
        }
    }

    /// Invio nel campo: in Trova il risultato dopo, nella ricerca della Home «fatto» (la tastiera
    /// si chiude e lascia vedere quello che si è trovato).
    fn invio_nel_campo(&mut self) {
        if self.password_pdf.is_some() {
            self.apri_con_la_password();
        } else if self.schermata == Schermata::Home {
            self.fuoco_al_documento();
            self.piattaforma.hide_keyboard();
        } else if self.schermata == Schermata::Pdf {
            self.invio_nel_pdf();
        } else if self.schermata == Schermata::Impostazioni {
            let nome = impostazioni::pulito(self.impostazioni.campo_autore.testo());
            self.scrivi_l_autore(nome);
        } else if self.descrizione.is_some() {
            self.conferma_descrizione();
        } else if self.commento.is_some() {
            self.conferma_commento();
        } else if self.campi_pagina.is_some() {
            // Dall'intestazione al piè di pagina; dal piè di pagina la tastiera si chiude.
            if self.quale_campo_word() == Some(QualeCampo::Intestazione) {
                self.fuoco_al_campo_word(QualeCampo::Pie);
            } else {
                self.fuoco_al_documento();
                self.piattaforma.hide_keyboard();
            }
        } else {
            self.trova_altro(true);
        }
    }

    fn tasto_crudo(&mut self, t: TastoCrudo) {
        if self.fuoco == Fuoco::Campo {
            // Nel campo Invio è il risultato dopo (o «fatto»); Backspace e Canc a vuoto non fanno niente.
            let quale = match self.schermata {
                _ if self.password_pdf.is_some() => "della password",
                Schermata::Home => "della ricerca",
                _ => self.campo_di_trova(),
            };
            diagnosi(format!("tastiera: tasto {t:?} al campo {quale}"));
            if t == TastoCrudo::Invio {
                self.invio_nel_campo();
            }
            return;
        }
        match self.editor_mut() {
            Some(e) => {
                let st = e.tasto_crudo(t);
                self.dopo_ponte(st);
            }
            None => diagnosi(format!("tastiera: tasto {t:?} senza un documento aperto, ignorato")),
        }
    }

    pub fn disegna(&mut self, ui: &mut egui::Ui) {
        if !self.stile_pronto {
            if !preparato(ui.ctx()) {
                prepara(ui.ctx());
            }
            self.stile_pronto = true;
        }
        // App bloccata: solo la schermata del blocco, color carta, niente fogli né avvisi.
        if self.blocco.bloccata {
            ui.painter().rect_filled(ui.ctx().viewport_rect(), 0.0, tema::CARTA);
            self.mostra_bloccata(ui);
            return;
        }
        if ui.input(|i| i.key_pressed(egui::Key::BrowserBack) || i.key_pressed(egui::Key::Escape)) {
            self.indietro();
        }
        // Tutto lo schermo color carta, anche sotto la barra di stato e quella di navigazione.
        ui.painter().rect_filled(ui.ctx().viewport_rect(), 0.0, tema::CARTA);
        // egui dà alla Ui radice tutto lo schermo: si resta dentro la zona libera da barre e tastiera.
        let zona = ui.ctx().content_rect();
        ui.scope_builder(egui::UiBuilder::new().max_rect(zona), |ui| match self.schermata {
            Schermata::Home => self.mostra_home(ui),
            Schermata::Editor => self.mostra_editor(ui),
            Schermata::Pdf => self.mostra_pdf(ui),
            Schermata::Slide => self.mostra_slide(ui),
            Schermata::Foglio => self.mostra_foglio(ui),
            Schermata::Impostazioni => self.mostra_impostazioni(ui),
            Schermata::Diagnosi => self.mostra_diagnosi(ui),
        });
        // «Organizza pagine» a tutto schermo sopra il lettore, sotto i suoi fogli e i dialoghi.
        if self.schermata == Schermata::Pdf {
            self.mostra_organizza_pdf(ui.ctx());
        }
        self.mostra_foglio_aa(ui.ctx());
        self.mostra_foglio_indice(ui.ctx());
        self.mostra_foglio_inserisci(ui.ctx());
        self.mostra_foglio_bordi(ui.ctx());
        self.mostra_foglio_descrizione(ui.ctx());
        self.mostra_foglio_pagina(ui.ctx());
        self.mostra_foglio_revisioni(ui.ctx());
        self.mostra_foglio_commento(ui.ctx());
        self.mostra_foglio_info(ui.ctx());
        self.mostra_fogli_pdf(ui.ctx());
        self.mostra_fogli_slide(ui.ctx());
        self.mostra_riconosci_pdf(ui.ctx());
        self.mostra_data(ui.ctx());
        self.mostra_scansione(ui.ctx());
        self.mostra_link_esterno(ui.ctx());
        self.mostra_firme_pdf(ui.ctx());
        self.mostra_calcoli_pdf(ui.ctx());
        self.mostra_salva_firma_pdf(ui.ctx());
        self.mostra_recupero(ui.ctx());
        self.mostra_apertura_pdf(ui.ctx());
        self.mostra_apertura_slide(ui.ctx());
        self.mostra_apertura_foglio(ui.ctx());
        self.mostra_password_pdf(ui.ctx());
        if self.dialogo.is_some() {
            self.mostra_dialogo(ui.ctx());
        }
        self.mostra_errore(ui.ctx());
        let fondo = self.fondo_degli_avvisi(ui.ctx());
        if self.avviso_breve.mostra(ui.ctx(), fondo) {
            self.condividi_pdf();
        }
        if self.avvio_home_ms.is_none() {
            let ms = self.avvio.elapsed().as_secs_f32() * 1000.0;
            self.avvio_home_ms = Some(ms);
            diagnosi_fissa(Fissa::PrimoDisegno, format!("avvio: primo disegno dopo {ms:.0} ms, schermata {:?}", self.schermata));
        }
    }

    fn mostra_home(&mut self, ui: &mut egui::Ui) {
        let contesto = home::Contesto { tastiera: self.piattaforma.keyboard_visible(), adesso: adesso() };
        let home = &mut self.home;
        let azione = egui::CentralPanel::no_frame().show(ui, |ui| home.mostra(ui, &contesto)).inner;
        if let Some(a) = azione {
            self.azione_della_home(a);
        }
    }

    fn mostra_impostazioni(&mut self, ui: &mut egui::Ui) {
        self.impostazioni.firma_salvata = self.firme().qualcuna();
        let scelta = egui::CentralPanel::no_frame().show(ui, |ui| self.impostazioni.mostra(ui)).inner;
        if let Some(s) = scelta {
            self.scelta_delle_impostazioni(s);
        }
        // Il foglio dell'autore chiuso toccando fuori o con Indietro: il campo lascia la tastiera.
        if self.impostazioni.autore_aperto && !self.impostazioni.foglio_autore.aperto() {
            self.chiudi_foglio_autore();
        }
    }

    /// La Diagnosi: la barra alta con la versione, la misura dell'avvio e la condivisione, poi il
    /// registro com'era quando si è aperta.
    fn mostra_diagnosi(&mut self, ui: &mut egui::Ui) {
        let sotto = format!("{NOME_APP} {VERSIONE} · avvio fino al primo disegno {}", tempo(self.avvio_home_ms));
        let (mut indietro, mut condividi) = (false, false);
        egui::CentralPanel::no_frame().show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            let barra = BarraAlta::new("Diagnosi")
                .indietro("Indietro")
                .sottotitolo(&sotto)
                .azione(PulsanteIcona::new(icone::SHARE, "Condividi diagnosi"))
                .mostra(ui);
            indietro = barra.indietro;
            condividi = barra.azione == Some(0);
            egui::Frame::NONE.inner_margin(egui::Margin { left: 22, right: 22, top: 4, bottom: 8 }).show(ui, |ui| {
                ui.label(
                    egui::RichText::new(
                        "Il registro degli eventi dell'app: tempi, tocchi, tastiera, file aperti. Non contiene il testo dei documenti né i nomi dei file.",
                    )
                    .font(Carattere::Testo.font(ui.ctx(), 14.0))
                    .color(tema::MATITA),
                );
            });
            egui::Frame::NONE.inner_margin(egui::Margin::symmetric(tema::MARGINE as i8, 0)).show(ui, |ui| {
                Scheda::new().margine(egui::Margin::same(12)).show(ui, |ui| {
                    egui::ScrollArea::vertical().scroll_source(egui::scroll_area::ScrollSource::ALL).auto_shrink([false, false]).show(ui, |ui| {
                        ui.add(egui::Label::new(egui::RichText::new(&self.testo_diagnosi).monospace().size(11.0).color(tema::INCHIOSTRO)).wrap());
                    });
                });
            });
        });
        if condividi {
            self.piattaforma.condividi_testo(&diagnosi_testo(), &format!("Diagnosi di {NOME_APP}"));
        }
        if indietro {
            self.vai(self.prima_della_diagnosi);
        }
    }

    /// Il dialogo «Salvare le modifiche?» (Annulla, Non salvare, Salva). Un tocco fuori vale
    /// Annulla, come su Android; Indietro lo gestisce già `indietro`.
    fn mostra_dialogo(&mut self, ctx: &egui::Context) {
        let risposta = Dialogo::new(egui::Id::new("salvare_le_modifiche"), "Salvare le modifiche?")
            .testo("Il documento ha modifiche che non sono state salvate.")
            .pulsante("Annulla", Peso::Testo)
            .pulsante("Non salvare", Peso::Testo)
            .pulsante("Salva", Peso::Principale)
            .mostra(ctx);
        match risposta {
            Some(RispostaDialogo::Pulsante(2)) => self.scegli(Scelta::Salva),
            Some(RispostaDialogo::Pulsante(1)) => self.scegli(Scelta::NonSalvare),
            Some(RispostaDialogo::Pulsante(_) | RispostaDialogo::Fuori) => self.scegli(Scelta::Annulla),
            None => {}
        }
    }

    /// Nella Home, il lavoro da recuperare, una copia alla volta, dalla più recente: il dialogo
    /// con Scarta e Riapri. Un tocco fuori non lo chiude: si sceglie (Indietro fa quello che fa
    /// nella Home, e il dialogo resta).
    fn mostra_recupero(&mut self, ctx: &egui::Context) {
        if self.schermata != Schermata::Home || self.errore.is_some() {
            return;
        }
        let Some(p) = self.recupero.proposte.first() else { return };
        let quante = self.recupero.proposte.len();
        // Il nome come nella Home, senza l'estensione.
        let nome = p.fonte.nome();
        let nome = home::nome_da_mostrare(&nome, tipo_da_nome(&nome));
        let mut testo =
            format!("L'app si è chiusa con modifiche non salvate a «{nome}» ({}). Riaprile, oppure scartale.", ora_locale_breve(p.fonte.quando()));
        match quante {
            0 | 1 => {}
            2 => testo.push_str(" Poi ce n'è un'altra."),
            _ => testo.push_str(&format!(" Poi ce ne sono altre {}.", quante - 1)),
        }
        let risposta = Dialogo::new(egui::Id::new("lavoro_da_recuperare"), "Lavoro da recuperare")
            .testo(&testo)
            .pulsante("Scarta", Peso::Testo)
            .pulsante("Riapri", Peso::Principale)
            .mostra(ctx);
        match risposta {
            Some(RispostaDialogo::Pulsante(0)) => self.scarta_recupero(0),
            Some(RispostaDialogo::Pulsante(_)) => self.riapri_recupero(0),
            Some(RispostaDialogo::Fuori) | None => {}
        }
    }

    /// L'errore, se c'è, nel dialogo con «Ho capito»; si chiude anche toccando fuori o con Indietro.
    fn mostra_errore(&mut self, ctx: &egui::Context) {
        let Some(e) = &self.errore else { return };
        let risposta = Dialogo::new(egui::Id::new("errore"), e.titolo).testo(&e.testo).pulsante(HO_CAPITO, Peso::Principale).mostra(ctx);
        if risposta.is_some() {
            self.errore = None;
        }
    }

    /// La y sopra cui stanno gli avvisi brevi: la barra di formato nell'Editor (anche sulla
    /// tastiera), il «Nuovo» nella Home, altrimenti il fondo della zona libera.
    fn fondo_degli_avvisi(&self, ctx: &egui::Context) -> f32 {
        let zona = ctx.content_rect();
        let in_pagina = self.editor().is_some_and(|e| e.vista.modo == Modo::Pagina);
        match self.schermata {
            Schermata::Editor if !in_pagina && self.piattaforma.keyboard_height() > 0.0 => BarraFlottante::posto_sulla_tastiera(zona).top(),
            Schermata::Editor if !in_pagina => BarraFlottante::posto(zona).top(),
            Schermata::Home => zona.bottom() - home::SOPRA_IL_NUOVO,
            Schermata::Pdf if self.piattaforma.keyboard_height() <= 0.0 => BarraVoci::posto(zona).top(),
            _ => zona.bottom(),
        }
    }
}

impl eframe::App for TascabileApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.passo_logica(ctx);
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        // Barre di sistema, tastiera e tema vengono dalla piattaforma, non da winit.
        self.piattaforma.apply_raw_input(ctx, raw);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.disegna(ui);
    }
}

#[cfg(test)]
pub(crate) mod prova {
    use std::path::{Path, PathBuf};

    /// Cartella vuota per una prova, dentro target/ (mai in /tmp).
    pub fn cartella_di_prova(nome: &str) -> PathBuf {
        let d = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/prove-app").join(nome);
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }
}

#[cfg(test)]
mod test {
    use std::path::PathBuf;

    use super::*;
    use crate::prova::cartella_di_prova;
    use egui::accesskit::Role;
    use egui_kittest::kittest::{NodeT, Queryable};
    use tascabile_files::Recenti;
    use tascabile_platform::{ImeSnapshot, OpenRequest};
    use wordcraft_engine::doc::Pos;

    /// Piattaforma con le cartelle di una prova in target/: dati dell'app (i recenti), cache e
    /// radice dei documenti. Mai quelle vere del PC.
    fn piattaforma_di_prova(nome: &str) -> (Piattaforma, PathBuf) {
        tascabile_platform::ascolta_diagnosi();
        let d = cartella_di_prova(nome);
        std::fs::create_dir_all(d.join("radice")).unwrap();
        (Piattaforma::con_cartelle(d.join("app"), d.join("cache"), d.join("radice")), d)
    }

    fn app_di_prova(nome: &str) -> TascabileApp {
        TascabileApp::new(piattaforma_di_prova(nome).0, Instant::now())
    }

    /// Il documento dell'Editor, che nella prova deve esserci.
    fn editor(app: &TascabileApp) -> &Editor {
        app.editor().expect("nessun documento nell'Editor")
    }

    /// Come un tocco sul testo: finestra nuova attorno al cursore e tastiera finta aperta.
    fn apri_tastiera(app: &mut TascabileApp) {
        let e = app.editor_mut().expect("nessun documento nell'Editor");
        let st = e.ponte.apri(&e.s);
        assert!(st.is_some(), "il ponte non si apre");
        app.dopo_ponte(st);
        app.piattaforma.show_keyboard();
    }

    /// L'app su un documento vuoto, col ponte aperto e la tastiera finta visibile.
    fn app_che_scrive(nome: &str) -> TascabileApp {
        let mut app = app_di_prova(nome);
        app.cambia_documento(Editor::new(documento::nuovo(), "prova"));
        apri_tastiera(&mut app);
        app
    }

    /// Un documento di testo semplice, un paragrafo per riga.
    fn testo_semplice(nome: &str, testo: &str) -> Editor {
        Editor::new(documento::apri_byte(&format!("{nome}.txt"), testo.as_bytes()).unwrap(), nome)
    }

    /// Lo stato che la tastiera manderebbe aggiungendo «!» in fondo all'ultimo testo ricevuto.
    fn esclamazione(app: &TascabileApp) -> ImeSnapshot {
        let (testo, _, _, versione) = app.piattaforma.ultimo_stato().unwrap();
        let n = testo.encode_utf16().count() as i32 + 1;
        ImeSnapshot { text: format!("{testo}!"), sel: (n, n), comp: (-1, -1), version: versione }
    }

    /// Un passo di egui con questi eventi, solo la logica (niente disegno).
    fn passo(app: &mut TascabileApp, ctx: &egui::Context, eventi: Vec<egui::Event>) {
        let mut out = ctx.run_ui(egui::RawInput { events: eventi, ..Default::default() }, |ui| app.passo_logica(ui.ctx()));
        out.textures_delta.clear();
    }

    fn tasto(key: egui::Key) -> egui::Event {
        egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }
    }

    /// Testi dei paragrafi del documento nell'Editor.
    fn testi(app: &TascabileApp) -> Vec<String> {
        editor(app).s.doc.body.iter().filter_map(|b| b.as_para()).map(|p| p.text.clone()).collect()
    }

    /// Cursore del documento nell'Editor: (percorso del paragrafo, byte nel paragrafo).
    fn cursore(app: &TascabileApp) -> (Vec<u32>, usize) {
        let f = &editor(app).s.sel.focus;
        (f.path.0.clone(), f.off)
    }

    type Telefono = egui_kittest::Harness<'static, TascabileApp>;

    /// L'app disegnata senza GPU su uno schermo di telefono (412 × 915 dp), dopo qualche frame.
    fn telefono(app: TascabileApp) -> Telefono {
        telefono_di(app, egui::vec2(412.0, 915.0), 2.625)
    }

    /// Un telefono di questa misura (dp) e densità: quello dell'autore è 372 × 827 a 3 (1116 × 2480
    /// pixel a 480 dpi).
    fn telefono_di(app: TascabileApp, misura: egui::Vec2, densita: f32) -> Telefono {
        let mut h = egui_kittest::Harness::builder().with_size(misura).with_pixels_per_point(densita).build_ui_state(
            |ui, app: &mut TascabileApp| {
                let ctx = ui.ctx().clone();
                app.passo_logica(&ctx);
                app.disegna(ui);
            },
            app,
        );
        h.run_steps(4);
        h
    }

    /// Tocca il bottone (o la riga della Home) con questa etichetta, poi qualche frame.
    fn tocca(h: &mut Telefono, etichetta: &str) {
        h.get_by_role_and_label(Role::Button, etichetta).click();
        h.run_steps(2);
    }

    /// Dalla Home, «Nuovo» e poi «Documento» nel foglio che sale.
    fn nuovo_dalla_home(h: &mut Telefono) {
        tocca(h, "Nuovo");
        h.run_steps(4);
        tocca(h, home::DOCUMENTO_VUOTO);
        assert_eq!(h.state().schermata, Schermata::Editor, "«Documento vuoto» non ha aperto l'Editor");
    }

    /// Dalla Home le Impostazioni, poi la riga `voce`.
    /// La Guida rapida (tappa I, I8): la voce in cima alle Impostazioni apre il foglio con le parti
    /// della guida; Indietro lo chiude e si resta nelle Impostazioni.
    #[test]
    fn la_guida_rapida_si_apre_dalle_impostazioni() {
        let mut h = telefono(app_di_prova("guida"));
        dalle_impostazioni(&mut h, guida::GUIDA);
        h.run_steps(12);
        for (titolo, _) in guida::PARTI {
            h.get_by_label(titolo);
        }
        indietro(&mut h);
        h.run_steps(12);
        assert_eq!(h.state().schermata, Schermata::Impostazioni);
        assert!(h.query_by_label(guida::PARTI[0].0).is_none(), "la guida è rimasta aperta");
        indietro(&mut h);
        assert_eq!(h.state().schermata, Schermata::Home);
        assert!(diagnosi_della_prova().contains("] impostazioni: guida rapida aperta\n"));
    }

    fn dalle_impostazioni(h: &mut Telefono, voce: &str) {
        tocca(h, "Impostazioni");
        assert_eq!(h.state().schermata, Schermata::Impostazioni);
        // La voce in vista: la sezione della sicurezza spinge giù quelle sotto (lo scorrimento è
        // animato, qualche frame).
        h.get_by_role_and_label(Role::Button, voce).scroll_to_me();
        h.run_steps(12);
        tocca(h, voce);
    }

    /// Roadmap della sicurezza, Fase 1: la modalità riservata (FLAG_SECURE sul telefono) è spenta di
    /// base, si accende nelle Impostazioni, resta accesa alla volta dopo e si rispegne.
    #[test]
    fn la_modalita_riservata_si_accende_e_resta() {
        let (piattaforma, d) = piattaforma_di_prova("riservata");
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        assert!(!h.state().piattaforma.riservata_accesa());
        dalle_impostazioni(&mut h, impostazioni::RISERVATA);
        assert!(h.state().impostazioni.riservata && h.state().piattaforma.riservata_accesa());
        assert!(impostazioni::leggi_riservata(&d.join("app")));
        let acceso = h.get_by_role_and_label(Role::Button, impostazioni::RISERVATA).accesskit_node().toggled();
        assert_eq!(acceso, Some(egui::accesskit::Toggled::True));
        // Un'altra volta dell'app: accesa da subito.
        let piattaforma = Piattaforma::con_cartelle(d.join("app"), d.join("cache"), d.join("radice"));
        let mut altra = telefono(TascabileApp::new(piattaforma, Instant::now()));
        assert!(altra.state().piattaforma.riservata_accesa());
        dalle_impostazioni(&mut altra, impostazioni::RISERVATA);
        assert!(!altra.state().piattaforma.riservata_accesa() && !impostazioni::leggi_riservata(&d.join("app")));
        let diag = diagnosi_della_prova();
        for riga in ["] impostazioni: modalità riservata accesa\n", "] modalità riservata: accesa\n", "] impostazioni: modalità riservata spenta\n"] {
            assert!(diag.contains(riga), "manca «{riga}»:\n{diag}");
        }
    }

    /// Indietro di Android (Esc sul PC), poi qualche frame.
    fn indietro(h: &mut Telefono) {
        h.key_press(egui::Key::BrowserBack);
        h.run_steps(2);
    }

    /// Scrive con la tastiera finta, già aperta.
    fn scrivi(h: &mut Telefono, testo: &str) {
        h.event(egui::Event::Text(testo.into()));
        h.run_steps(2);
    }

    /// Un .docx vero con questi paragrafi, scritto dal motore.
    fn docx(percorso: &Path, paragrafi: &[&str]) {
        let s = documento::apri_byte("prova.txt", paragrafi.join("\n").as_bytes()).unwrap();
        std::fs::write(percorso, wordcraft_engine::io::save_bytes("prova.docx", &s.doc).unwrap()).unwrap();
    }

    /// Le righe della diagnosi scritte da questa prova (dal suo thread, da `piattaforma_di_prova`),
    /// col «] » davanti come nel registro. Le prove in parallelo, che sono tante, fanno uscire le
    /// righe dal giro delle 300 del registro prima che la prova le legga; queste restano tutte.
    fn diagnosi_della_prova() -> String {
        tascabile_platform::righe_ascoltate().lines().map(|r| format!("] {r}\n")).collect()
    }

    /// Le righe fisse in testa al registro (prima degli ultimi eventi).
    fn testa_della_diagnosi() -> String {
        let d = tascabile_platform::diagnosi_testo();
        d.split_once("Ultimi eventi, dal più vecchio:\n").map(|(testa, _)| testa.to_string()).unwrap_or_default()
    }

    /// Seleziona nel documento dell'Editor da `a` a `b` (byte del primo paragrafo).
    fn seleziona(h: &mut Telefono, a: usize, b: usize) {
        let e = h.state_mut().editor_mut().unwrap();
        e.s.run("select.range", &serde_json::json!({ "anchor": Pos::body(0, a), "focus": Pos::body(0, b) })).unwrap();
        h.run_steps(3);
    }

    #[test]
    fn copia_taglia_e_incolla_dalla_barra_della_selezione() {
        let mut app = app_di_prova("appunti");
        app.cambia_documento(testo_semplice("prova", "Ciao mondo bello"));
        apri_tastiera(&mut app);
        let mut h = telefono(app);
        seleziona(&mut h, 5, 10);
        let versione = h.state().piattaforma.ultimo_stato().unwrap().3;
        tocca(&mut h, "Copia");
        assert_eq!(h.state().piattaforma.incolla_testo().as_deref(), Some("mondo"));
        assert_eq!(testi(h.state()), ["Ciao mondo bello"]);
        let dopo_copia = h.state().piattaforma.ultimo_stato().unwrap().3;
        assert!(dopo_copia > versione, "dopo Copia la tastiera non ha una finestra nuova");
        // Incolla al posto di «bello».
        seleziona(&mut h, 11, 16);
        tocca(&mut h, "Incolla");
        assert_eq!(testi(h.state()), ["Ciao mondo mondo"]);
        // Taglia toglie «Ciao» e lo mette negli appunti.
        seleziona(&mut h, 0, 4);
        tocca(&mut h, "Taglia");
        assert_eq!(testi(h.state()), [" mondo mondo"]);
        assert_eq!(h.state().piattaforma.incolla_testo().as_deref(), Some("Ciao"));
        // Da un'altra app arrivano due righe: due paragrafi.
        h.state().piattaforma.copia_testo("uno\ndue");
        seleziona(&mut h, 0, 0);
        h.state_mut().editor_mut().unwrap().vista.barra_al_cursore(Pos::body(0, 0));
        h.run_steps(3);
        assert!(h.query_by_role_and_label(Role::Button, "Copia").is_none(), "con un cursore niente Copia");
        tocca(&mut h, "Incolla");
        assert_eq!(testi(h.state()), ["uno", "due mondo mondo"]);
        // Il cursore si è spostato: la barra se ne va.
        assert!(h.query_by_role_and_label(Role::Button, "Incolla").is_none());
        let d = diagnosi_della_prova();
        assert!(d.contains("] comando: edit.cut\n") && d.contains("] comando: edit.pasteText\n"), "{d}");
        assert!(!d.contains("mondo") && !d.contains("uno"), "testo nella diagnosi:\n{d}");
    }

    #[test]
    fn indietro_chiude_prima_la_selezione() {
        let mut app = app_di_prova("indietro-selezione");
        app.cambia_documento(testo_semplice("prova", "Ciao mondo"));
        let mut h = telefono(app);
        seleziona(&mut h, 0, 4);
        assert!(h.query_by_role_and_label(Role::Button, "Copia").is_some());
        indietro(&mut h);
        assert_eq!(h.state().schermata, Schermata::Editor);
        assert!(editor(h.state()).s.sel.is_collapsed());
        assert_eq!(cursore(h.state()), (vec![0], 4));
        assert!(h.query_by_role_and_label(Role::Button, "Copia").is_none());
        indietro(&mut h);
        assert_eq!(h.state().schermata, Schermata::Home);
    }

    #[test]
    fn seleziona_tutto_dalla_barra() {
        let mut app = app_di_prova("seleziona-tutto");
        app.cambia_documento(testo_semplice("prova", "Uno\nDue"));
        let mut h = telefono(app);
        seleziona(&mut h, 0, 2);
        tocca(&mut h, "Tutto");
        let (a, b) = editor(h.state()).s.sel.ordered();
        assert_eq!((a, b), (Pos::body(0, 0), Pos::body(1, 3)));
    }

    #[test]
    fn tastiera_chiusa_quando_il_ponte_si_chiude() {
        let mut app = app_che_scrive("ponte-chiuso");
        let ctx = egui::Context::default();
        // Un incolla oltre il limite del ponte: per la tastiera il paragrafo è troppo lungo.
        let incolla = "a".repeat(tascabile_word::finestra::LIMITE_UTF16 + 1);
        passo(&mut app, &ctx, vec![egui::Event::Text(incolla)]);
        assert!(!editor(&app).ponte.attivo() && editor(&app).ponte.avviso.is_some(), "il ponte doveva chiudersi con un avviso");
        assert!(!app.piattaforma.keyboard_visible(), "tastiera aperta col ponte chiuso");
    }

    #[test]
    fn stato_della_tastiera_prima_dei_tasti_crudi() {
        let mut app = app_che_scrive("prima-lo-stato");
        let ctx = egui::Context::default();
        // Nello stesso passo un Backspace a inizio finestra (la tastiera lo lascia all'app) e una
        // lettera. Lo stato con la lettera va consegnato per primo: al contrario il tasto crudo
        // aprirebbe una finestra nuova, e lo stato, più vecchio di quella, verrebbe scartato con
        // la lettera. Dopo la lettera il cursore della tastiera non è più sul bordo: il tasto
        // crudo si perde e non riporta il cursore prima della «x».
        passo(&mut app, &ctx, vec![tasto(egui::Key::Backspace), egui::Event::Text("x".into())]);
        assert_eq!(testi(&app), ["x"]);
        assert_eq!(cursore(&app), (vec![0], 1));
    }

    #[test]
    fn stato_in_ritardo_del_documento_di_prima_scartato() {
        let mut app = app_di_prova("in-ritardo");
        app.cambia_documento(testo_semplice("A", "uno\ndue\ntre"));
        for _ in 0..5 {
            apri_tastiera(&mut app);
        }
        // Uno stato della tastiera scritto sul documento A, che arriva quando c'è già B.
        let vecchio = esclamazione(&app);
        app.cambia_documento(testo_semplice("B", "alfa\nbeta\ngamma"));
        apri_tastiera(&mut app);
        let prima = testi(&app);
        let st = app.editor_mut().unwrap().ricevi(&vecchio);
        app.dopo_ponte(st);
        assert_eq!(testi(&app), prima, "il testo di A è finito in B");
    }

    #[test]
    fn stato_in_ritardo_scartato_anche_passando_dalla_home() {
        // A si chiude tornando nella Home, B si apre dopo: le versioni della tastiera continuano
        // anche senza un documento in mezzo.
        let mut app = app_di_prova("in-ritardo-home");
        app.cambia_documento(testo_semplice("A", "uno\ndue\ntre"));
        for _ in 0..5 {
            apri_tastiera(&mut app);
        }
        let vecchio = esclamazione(&app);
        app.chiudi_poi(Dopo::Home);
        assert_eq!((app.schermata, app.editor().is_none()), (Schermata::Home, true));
        // Arriva nella Home: nessun documento, si scarta.
        app.piattaforma.simula_testo("?");
        passo(&mut app, &egui::Context::default(), vec![]);
        assert!(diagnosi_della_prova().contains(" senza un documento aperto, scartato\n"));
        app.cambia_documento(testo_semplice("B", "alfa\nbeta\ngamma"));
        apri_tastiera(&mut app);
        let prima = testi(&app);
        let st = app.editor_mut().unwrap().ricevi(&vecchio);
        app.dopo_ponte(st);
        assert_eq!(testi(&app), prima, "il testo di A è finito in B");
    }

    #[test]
    fn la_checklist_tiene_le_risposte() {
        let mut app = app_di_prova("checklist");
        let ctx = egui::Context::default();
        app.apri_checklist();
        // All'apertura il cursore è nell'ultimo paragrafo, sotto «Spazio per scrivere».
        apri_tastiera(&mut app);
        passo(&mut app, &ctx, vec![egui::Event::Text("sì".into())]);
        let risposte = testi(&app);
        assert_eq!(risposte.last().map(String::as_str), Some("sì"));
        app.apri_lungo();
        assert_ne!(testi(&app), risposte);
        // Uno stato della tastiera scritto sul documento lungo, che arriva quando c'è già la checklist.
        apri_tastiera(&mut app);
        apri_tastiera(&mut app);
        let vecchio = esclamazione(&app);
        app.apri_checklist();
        assert_eq!(testi(&app), risposte, "la checklist ha perso le risposte");
        // Lo stato in ritardo arriva prima di un tocco sulla checklist e dopo: tutte e due le volte
        // si scarta.
        let st = app.editor_mut().unwrap().ricevi(&vecchio);
        app.dopo_ponte(st);
        apri_tastiera(&mut app);
        let st = app.editor_mut().unwrap().ricevi(&vecchio);
        app.dopo_ponte(st);
        assert_eq!(testi(&app), risposte, "il testo del documento lungo è finito nella checklist");
        // «Checklist» con la checklist già sullo schermo non la ricomincia.
        app.apri_checklist();
        assert_eq!(testi(&app), risposte);
        // Nemmeno passando dalla Home.
        app.chiudi_poi(Dopo::Home);
        app.apri_checklist();
        assert_eq!(testi(&app), risposte);
    }

    /// L'app con la checklist nell'Editor, aperta dalla Home come farebbe l'autore.
    fn telefono_con_la_checklist(nome: &str) -> Telefono {
        let mut h = telefono(app_di_prova(nome));
        dalle_impostazioni(&mut h, "Checklist della tastiera");
        assert_eq!(h.state().schermata, Schermata::Editor);
        // Le barre prendono la loro misura nei primi frame, come all'avvio.
        h.run_steps(4);
        h
    }

    #[test]
    fn barra_intera_sullo_schermo() {
        let h = telefono_con_la_checklist("barra");
        let schermo = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(412.0, 915.0));
        let etichette = [schermo_editor::FORMATO, schermo_editor::INSERISCI].into_iter().chain(schermo_editor::BARRA.map(|(_, _, e)| e));
        for etichetta in etichette {
            let r = h.get_by_role_and_label(Role::Button, etichetta).rect();
            assert!(schermo.contains_rect(r), "«{etichetta}» fuori dallo schermo: {r:?}");
            assert!(r.width() >= 48.0 && r.height() >= 48.0, "«{etichetta}» piccolo: {r:?}");
        }
    }

    #[test]
    fn testata_con_un_nome_lungo() {
        let mut app = app_di_prova("nome-lungo");
        app.cambia_documento(Editor::new(documento::nuovo(), "Relazione finale del progetto di ottobre 2026, versione definitiva.docx"));
        let h = telefono(app);
        let indietro = h.get_by_label(schermo_editor::INDIETRO).rect();
        let annulla = h.get_by_label(schermo_editor::ANNULLA).rect();
        let nome = h.get_by_label_contains("Relazione finale").rect();
        // Il nome, senza l'estensione, si accorcia con «…» fra indietro e le azioni.
        assert!(nome.min.x >= indietro.max.x && nome.max.x <= annulla.min.x, "il nome esce dal suo posto: {nome:?}, {indietro:?}");
        assert!(indietro.height() >= 48.0 && indietro.width() >= 48.0, "indietro piccolo: {indietro:?}");
        assert!(h.query_by_label_contains(".docx").is_none());
    }

    #[test]
    fn apri_con_lascia_tipo_e_misura_senza_nome_ne_testo() {
        let (piattaforma, dir) = piattaforma_di_prova("apri-con");
        let percorso = dir.join("Relazione Zanzibar.txt");
        let testo = "Kilimangiaro\nseconda riga di prova, lunga apposta: 1234567";
        std::fs::write(&percorso, testo).unwrap();
        let mut app = TascabileApp::new(piattaforma, Instant::now());
        app.piattaforma.simula_apri_con(percorso.to_str().unwrap(), Some("text/plain"));
        passo(&mut app, &egui::Context::default(), vec![]);
        assert_eq!(testi(&app)[0], "Kilimangiaro");
        assert_eq!(app.schermata, Schermata::Editor);
        let d = diagnosi_della_prova();
        assert!(d.contains("] apri con: richiesta, tipo text/plain\n"), "{d}");
        assert!(d.contains(&format!("] apri con: {} byte, tipo txt, aperto in ", testo.len())), "{d}");
        for parola in ["Zanzibar", "Relazione", "Kilimangiaro", "lunga apposta", "prove-app"] {
            assert!(!d.contains(parola), "«{parola}» nella diagnosi:\n{d}");
        }
        // L'ultimo «Apri con» resta in testa. Quale, dipende dalle altre prove che girano insieme.
        let testa = testa_della_diagnosi();
        assert!(testa.contains("] apri con: "), "{testa}");
        // E il documento va nei recenti, anche su disco.
        let chiave = percorso.to_str().unwrap();
        assert_eq!(app.home.recenti()[0].chiave, chiave);
        assert_eq!(Recenti::carica(&dir.join("app").join(home::FILE_RECENTI)).voci()[0].chiave, chiave);
    }

    /// Passi di logica finché il PDF che si apre non è arrivato (o non ha dato errore), al massimo 20 s.
    fn aspetta_il_pdf(app: &mut TascabileApp, ctx: &egui::Context) {
        let fine = Instant::now() + Duration::from_secs(20);
        passo(app, ctx, vec![]);
        while app.apertura_pdf.is_some() {
            assert!(Instant::now() < fine, "il PDF non si è aperto in 20 s");
            std::thread::sleep(Duration::from_millis(5));
            passo(app, ctx, vec![]);
        }
    }

    /// Roadmap della sicurezza, Fase 1: «Apri con» con un indirizzo che non si apre (un file://,
    /// il nostro FileProvider) lo dice anche a chi usa l'app, non solo alla diagnosi. Nella
    /// diagnosi il perché, l'indirizzo mai.
    #[test]
    fn apri_con_un_indirizzo_rifiutato_lo_dice() {
        let (piattaforma, _dir) = piattaforma_di_prova("apri-con-rifiutato");
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        for uri in ["file:///data/data/it.tascabile.app/files/Zanzibar.docx", "content://it.tascabile.app.condivisione/condivisi/Zanzibar.pdf"] {
            h.state().piattaforma.simula_apri_con(uri, None);
            h.run_steps(3);
            assert_eq!(h.state().schermata, Schermata::Home);
            assert!(h.state().errore.is_none() && h.state().pdf.is_none() && h.state().aperto.is_none());
            h.get_by_label(APRI_CON_RIFIUTATO);
        }
        let d = diagnosi_della_prova();
        for riga in ["] apri con: indirizzo non aperto (file)\n", "] apri con: indirizzo non aperto (nostro FileProvider)\n"] {
            assert!(d.contains(riga), "manca «{riga}»:\n{d}");
        }
        assert!(!d.contains("Zanzibar") && !d.contains("condivisi/"), "{d}");
    }

    #[test]
    fn un_pdf_si_riconosce_dal_contenuto_e_si_apre_nel_lettore() {
        let (piattaforma, dir) = piattaforma_di_prova("pdf");
        // Ha il nome di un documento Word, ma è un PDF: lo dice il contenuto.
        let percorso = dir.join("Bolletta Zanzibar.docx");
        let byte = tascabile_pdf::prova::pdf(&[tascabile_pdf::prova::Pagina::bianca(595.0, 842.0); 2]);
        std::fs::write(&percorso, &byte).unwrap();
        let mut app = TascabileApp::new(piattaforma, Instant::now());
        let ctx = egui::Context::default();
        app.piattaforma.simula_apri_con(percorso.to_str().unwrap(), None);
        aspetta_il_pdf(&mut app, &ctx);
        assert_eq!(app.schermata, Schermata::Pdf);
        assert!(app.errore.is_none());
        let p = app.pdf.as_ref().expect("nessun PDF nel lettore");
        assert_eq!(p.lettore.pagine(), 2);
        let d = diagnosi_della_prova();
        assert!(d.contains(&format!("] apri con: {} byte, tipo pdf, aperto in ", byte.len())), "{d}");
        assert!(d.contains(" ms, 2 pagine\n"), "{d}");
        for parola in ["Zanzibar", "Bolletta", "prove-app"] {
            assert!(!d.contains(parola), "«{parola}» nella diagnosi:\n{d}");
        }
        // Nei recenti, col tipo PDF anche se il nome dice altro, e le pagine.
        assert_eq!(app.home.recenti()[0].tipo, Tipo::Pdf);
        assert_eq!(app.home.recenti()[0].pagine, Some(2));
        assert_eq!(app.home.recenti()[0].chiave, percorso.to_str().unwrap());
        // Indietro: alla Home, con le misure dei disegni nella diagnosi.
        app.indietro();
        assert_eq!(app.schermata, Schermata::Home);
        assert!(app.pdf.is_none());
        assert!(diagnosi_della_prova().contains("] pdf chiuso: "));
        // Un PDF rovinato: nella diagnosi il genere dell'errore, e si resta dove si era.
        std::fs::write(&percorso, b"%PDF-1.7\nniente altro").unwrap();
        app.piattaforma.simula_apri_con(percorso.to_str().unwrap(), None);
        aspetta_il_pdf(&mut app, &ctx);
        assert!(diagnosi_della_prova().contains("] apri con: 21 byte, tipo pdf, errore di apertura (PDF illeggibile)\n"));
        assert!(app.errore.as_ref().is_some_and(|e| e.testo.contains("non è un PDF che si legge")));
        assert_eq!(app.schermata, Schermata::Home);
    }

    #[test]
    fn un_pdf_da_apri_con_prende_il_nome_vero_e_chiude_il_documento_di_prima() {
        let (piattaforma, dir) = piattaforma_di_prova("pdf-apri-con");
        let percorso = dir.join("x");
        std::fs::write(&percorso, tascabile_pdf::prova::pdf(&[tascabile_pdf::prova::Pagina::bianca(300.0, 400.0)])).unwrap();
        let mut app = TascabileApp::new(piattaforma, Instant::now());
        let ctx = egui::Context::default();
        app.cambia_documento(Editor::new(documento::nuovo(), "prova"));
        apri_tastiera(&mut app);
        let r = OpenRequest { uri: percorso.to_str().unwrap().into(), mime: Some("application/pdf".into()), nome: Some("Consenso informato".into()) };
        assert_eq!(nome_di_apri_con(&r), "Consenso informato.pdf");
        app.apri_con(&r);
        // Finché il PDF non è pronto si resta nell'Editor, che però non riceve più la tastiera.
        assert_eq!(app.schermata, Schermata::Editor);
        assert!(!app.piattaforma.keyboard_visible());
        assert!(!editor(&app).ponte.attivo());
        aspetta_il_pdf(&mut app, &ctx);
        assert_eq!(app.schermata, Schermata::Pdf);
        assert!(app.aperto.is_none(), "il documento di prima è rimasto aperto");
        assert_eq!(app.pdf.as_ref().unwrap().nome, "Consenso informato.pdf");
        assert_eq!(app.home.recenti()[0].nome, "Consenso informato.pdf");
        // Indietro mentre si apre: si lascia andare, e il lettore resta com'era.
        let altro = OpenRequest { uri: percorso.to_str().unwrap().into(), mime: None, nome: Some("Altro".into()) };
        app.apri_con(&altro);
        app.indietro();
        assert!(app.apertura_pdf.is_none());
        aspetta_il_pdf(&mut app, &ctx);
        assert_eq!(app.pdf.as_ref().unwrap().nome, "Consenso informato.pdf");
        assert_eq!(app.schermata, Schermata::Pdf);
        // Senza nome e senza tipo: «documento.pdf».
        let r = OpenRequest { uri: percorso.to_str().unwrap().into(), mime: None, nome: None };
        app.apri_con(&r);
        aspetta_il_pdf(&mut app, &ctx);
        assert_eq!(app.pdf.as_ref().unwrap().nome, "documento.pdf");
        // Un documento Word aperto dopo chiude il PDF.
        app.cambia_documento(Editor::new(documento::nuovo(), "prova"));
        assert!(app.pdf.is_none());
        assert_eq!(app.schermata, Schermata::Editor);
    }

    /// Segnalazione dell'autore del 3/11: un PDF aperto da WhatsApp con «Apri con», dopo
    /// l'aggiornamento dell'app, «Non si apre più: riaprilo da WhatsApp». Se lo stesso file è nelle
    /// cartelle del telefono, si apre da lì: il recente è il file, che resta.
    #[test]
    fn apri_con_un_file_che_e_sul_telefono_lo_apre_da_li() {
        let (piattaforma, dir) = piattaforma_di_prova("apri-con-sul-telefono");
        let pdf = tascabile_pdf::prova::pdf(&[tascabile_pdf::prova::Pagina::bianca(300.0, 400.0)]);
        let ricevuti = dir.join("radice/Android/media/com.whatsapp/WhatsApp/Media/WhatsApp Documents/Private");
        std::fs::create_dir_all(&ricevuti).unwrap();
        std::fs::write(ricevuti.join("Referto.pdf"), &pdf).unwrap();
        // Quello che passa WhatsApp: un indirizzo suo (qui un file fuori dalla radice), coi byte.
        let passato = dir.join("passato-da-whatsapp");
        std::fs::write(&passato, &pdf).unwrap();
        let mut app = TascabileApp::new(piattaforma, Instant::now());
        let ctx = egui::Context::default();
        assert!(app.home.accesso(), "la prova vuole l'accesso ai file");
        let r = OpenRequest { uri: passato.to_str().unwrap().into(), mime: Some("application/pdf".into()), nome: Some("Referto.pdf".into()) };
        app.apri_con(&r);
        aspetta_il_pdf(&mut app, &ctx);
        assert_eq!(app.pdf.as_ref().unwrap().nome, "Referto.pdf");
        assert_eq!(app.home.recenti()[0].chiave, ricevuti.join("Referto.pdf").to_str().unwrap());
        // Un file che sul telefono non c'è si apre dall'indirizzo, come prima.
        std::fs::write(&passato, tascabile_pdf::prova::pdf(&[tascabile_pdf::prova::Pagina::bianca(200.0, 200.0)])).unwrap();
        app.apri_con(&r);
        aspetta_il_pdf(&mut app, &ctx);
        assert_eq!(app.home.recenti()[0].chiave, passato.to_str().unwrap());
    }

    #[test]
    fn il_lettore_a_schermo_col_nome_le_pagine_e_il_menu() {
        let (piattaforma, dir) = piattaforma_di_prova("pdf-schermo");
        let percorso = dir.join("radice/Verbale Zanzibar.pdf");
        let pagine = [tascabile_pdf::prova::Pagina::bianca(595.0, 842.0); 3];
        std::fs::write(&percorso, tascabile_pdf::prova::pdf(&pagine)).unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        h.state_mut().apri_file(&percorso, "Verbale Zanzibar.pdf");
        let fine = Instant::now() + Duration::from_secs(20);
        while h.state().schermata != Schermata::Pdf {
            assert!(Instant::now() < fine, "il lettore non si è aperto");
            std::thread::sleep(Duration::from_millis(5));
            h.step();
        }
        // L'indicatore della pagina all'apertura (i passi di kittest sono di un quarto di secondo:
        // fra poco sparisce).
        h.get_by_label("1 / 3");
        h.get_by_label("Verbale Zanzibar");
        h.get_by_label("PDF · 3 pagine");
        // La lente e il menu nella barra alta, senza Annulla e Ripeti finché non c'è una modifica;
        // nel menu Salva, Salva una copia, Condividi, Miniature, Vai a pagina e Indice; in fondo la
        // barra dei modi con Leggi acceso (un PDF senza campi), Compila, Annota, Modifica e
        // Strumenti.
        h.get_by_role_and_label(Role::Button, schermo_pdf::CERCA);
        assert!(h.query_by_label(pdf_lavoro::ANNULLA_MODIFICA).is_none() && h.query_by_label(pdf_lavoro::RIPETI_MODIFICA).is_none());
        assert!(acceso(&h, "Leggi") && !acceso(&h, "Compila") && !acceso(&h, "Annota") && !acceso(&h, "Modifica"));
        h.get_by_role_and_label(Role::Button, pdf_lavoro::STRUMENTI);
        tocca(&mut h, schermo_pdf::MENU);
        for voce in [
            pdf_lavoro::SALVA,
            pdf_lavoro::SALVA_COPIA,
            schermo_pdf::CONDIVIDI,
            schermo_pdf::COPIA_DEFINITIVA,
            schermo_pdf::MINIATURE,
            schermo_pdf::INDICE,
            schermo_pdf::VAI_A_PAGINA,
        ] {
            assert_eq!(h.query_all_by_label(voce).count(), 1, "{voce} nel menu");
        }
        indietro(&mut h);
        assert!(h.query_by_label(schermo_pdf::CONDIVIDI).is_none());
        assert_eq!(h.state().schermata, Schermata::Pdf);
        // Condividi: il file in condivisi/, col suo nome, e nella diagnosi l'esito. I byte li
        // prepara il motore sul suo thread.
        h.state_mut().condividi_pdf_aperto();
        h.state_mut().aspetta_il_motore_pdf();
        assert!(dir.join("cache/condivisi").exists());
        assert!(diagnosi_della_prova().contains("] condividi pdf aperto: riuscito, tipo pdf, "));
        indietro(&mut h);
        assert_eq!(h.state().schermata, Schermata::Home);
    }

    /// Passi finché il lettore non è aperto e non aspetta più niente dal pool, al massimo 20 s.
    fn aspetta_il_lettore(h: &mut Telefono) {
        let fine = Instant::now() + Duration::from_secs(20);
        h.step();
        while h.state().schermata != Schermata::Pdf || h.state().pdf_in_attesa() != Some(false) {
            assert!(Instant::now() < fine, "il lettore non ha finito in 20 s");
            std::thread::sleep(Duration::from_millis(5));
            h.step();
        }
        h.run_steps(2);
    }

    /// Il PDF `byte` aperto nel lettore, dalle cartelle, con le pagine sullo schermo disegnate.
    fn telefono_col_pdf(nome: &str, byte: Vec<u8>) -> Telefono {
        let (piattaforma, dir) = piattaforma_di_prova(nome);
        let percorso = dir.join("radice/Modulo Zanzibar.pdf");
        std::fs::write(&percorso, byte).unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        h.state_mut().apri_file(&percorso, "Modulo Zanzibar.pdf");
        aspetta_il_lettore(&mut h);
        h
    }

    /// Passi finché la presentazione che si apre non è arrivata (nelle Slide o col dialogo
    /// dell'errore), al massimo 20 s.
    fn aspetta_le_slide(h: &mut Telefono) {
        let fine = Instant::now() + Duration::from_secs(20);
        h.step();
        while h.state().slide_in_apertura() {
            assert!(Instant::now() < fine, "la presentazione non si è aperta in 20 s");
            std::thread::sleep(Duration::from_millis(5));
            h.step();
        }
        h.run_steps(2);
    }

    /// Un file scritto nella radice e aperto dalle cartelle, fino al suo arrivo.
    fn telefono_col_file(prova: &str, nome: &str, byte: &[u8]) -> Telefono {
        let (piattaforma, dir) = piattaforma_di_prova(prova);
        let percorso = dir.join("radice").join(nome);
        std::fs::write(&percorso, byte).unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        h.state_mut().apri_file(&percorso, nome);
        aspetta_le_slide(&mut h);
        h
    }

    /// Passi finché il foglio di calcolo che si apre non è arrivato, al massimo 20 s.
    fn aspetta_il_foglio(h: &mut Telefono) {
        let fine = Instant::now() + Duration::from_secs(20);
        h.step();
        while h.state().foglio_in_apertura() {
            assert!(Instant::now() < fine, "il foglio di calcolo non si è aperto in 20 s");
            std::thread::sleep(Duration::from_millis(5));
            h.step();
        }
        h.run_steps(2);
    }

    /// I fogli di calcolo in sola lettura (0.9.0, l'autore il 3/11): un .xlsx dalle cartelle si apre
    /// nella sua schermata, con la griglia, le schede dei fogli visibili (non quello nascosto), il
    /// tocco su una cella che mostra testo e formula; va nei recenti coi suoi fogli; Indietro
    /// chiude prima la cella, poi torna alla Home. Nella diagnosi niente nomi né celle.
    #[test]
    fn un_foglio_di_calcolo_si_apre_in_sola_lettura() {
        let (piattaforma, dir) = piattaforma_di_prova("foglio-apertura");
        let percorso = dir.join("radice/Turni Zanzibar.xlsx");
        std::fs::write(&percorso, tascabile_foglio::prova::cartella_di_prova()).unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        h.state_mut().apri_file(&percorso, "Turni Zanzibar.xlsx");
        aspetta_il_foglio(&mut h);
        assert_eq!(h.state().schermata, Schermata::Foglio);
        assert_eq!(h.state().foglio_aperto(), Some((2, 0, 1.0)));
        h.get_by_label("Turni Zanzibar");
        h.get_by_label("Foglio di calcolo · 2 fogli · sola lettura");
        assert!(h.query_by_label("Nascosto").is_none());
        // Un tocco sulla cella C5 (la somma): il pannello col testo e la formula.
        let griglia = h.get_by_label(schermo_foglio::GRIGLIA).rect();
        // A 1:1, le colonne larghe 64 dp e le righe alte 20: C5 sta a (2,5 × 64, 4,5 × 20).
        tocca_in(&mut h, griglia.min + egui::vec2(2.5 * 64.0, 4.5 * 20.0));
        h.run_steps(2);
        let d = h.state().cella_del_foglio().expect("il pannello della cella");
        assert_eq!((d.dove.as_str(), d.testo.as_str(), d.formula.as_deref()), ("C5", "74,3", Some("=SUM(C3:C4)")));
        h.get_by_label("74,3");
        // La seconda scheda.
        tocca(&mut h, "Turni");
        assert_eq!(h.state().foglio_aperto().map(|f| f.1), Some(1));
        assert!(h.state().cella_del_foglio().is_none(), "la cella resta aperta cambiando foglio");
        // Indietro: prima la cella, poi la Home.
        tocca_in(&mut h, griglia.min + egui::vec2(10.0, 10.0));
        h.run_steps(2);
        assert!(h.state().cella_del_foglio().is_some());
        indietro(&mut h);
        h.run_steps(2);
        assert!(h.state().cella_del_foglio().is_none() && h.state().schermata == Schermata::Foglio);
        indietro(&mut h);
        h.run_steps(2);
        assert_eq!(h.state().schermata, Schermata::Home);
        assert!(h.state().foglio_aperto().is_none());
        let r = &h.state().home.recenti()[0];
        assert_eq!((r.tipo, r.pagine), (Tipo::Foglio, Some(2)));
        assert_eq!(home::riga_del_recente("Download", "ora", r), "Download · ora · 2 fogli");
        let d = diagnosi_della_prova();
        assert!(d.contains("foglio di calcolo, 2 fogli da mostrare su 3"), "{d}");
        for parola in ["Zanzibar", "Pazienti", "Rossi", "74,3", "SUM"] {
            assert!(!d.contains(parola), "«{parola}» nella diagnosi:\n{d}");
        }
    }

    /// Un foglio che non si apre lo dice; un .xlsx col nome sbagliato si riconosce dal contenuto;
    /// «Apri con» dà il nome giusto dal tipo MIME.
    #[test]
    fn i_fogli_di_calcolo_che_non_vanno_e_il_nome_da_apri_con() {
        let (piattaforma, dir) = piattaforma_di_prova("foglio-errori");
        let rotto = dir.join("radice/Rotto.xlsx");
        std::fs::write(&rotto, b"PK\x03\x04 non uno zip vero").unwrap();
        let senza_nome = dir.join("radice/allegato");
        std::fs::write(&senza_nome, tascabile_foglio::prova::cartella_di_prova()).unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        h.state_mut().apri_file(&rotto, "Rotto.xlsx");
        aspetta_il_foglio(&mut h);
        assert!(h.state().errore.is_some() && h.state().schermata == Schermata::Home);
        tocca(&mut h, HO_CAPITO);
        h.state_mut().apri_file(&senza_nome, "allegato");
        aspetta_il_foglio(&mut h);
        assert_eq!(h.state().schermata, Schermata::Foglio);
        let r = |mime: &str, nome: Option<&str>| OpenRequest { uri: "content://x/1".into(), mime: Some(mime.into()), nome: nome.map(Into::into) };
        let xlsx = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";
        assert_eq!(nome_di_apri_con(&r(xlsx, None)), "foglio.xlsx");
        assert_eq!(nome_di_apri_con(&r(xlsx, Some("Turni"))), "Turni.xlsx");
        assert_eq!(nome_di_apri_con(&r("text/csv", Some("Elenco.csv"))), "Elenco.csv");
        assert_eq!(nome_di_apri_con(&r("application/vnd.ms-excel", Some("Vecchio"))), "Vecchio.xls");
    }

    #[test]
    fn una_presentazione_dalle_cartelle_si_apre_nelle_slide() {
        let lezione = tascabile_slide::prova::LEZIONE;
        let mut h = telefono_col_file("slide-apertura", "Lezione Zanzibar.pptx", lezione);
        assert_eq!(h.state().schermata, Schermata::Slide);
        h.get_by_label("Lezione Zanzibar");
        h.get_by_label("Slide · 4 slide");
        let r = &h.state().home.recenti()[0];
        assert_eq!((r.tipo, r.pagine), (Tipo::Presentazione, Some(4)));
        let d = diagnosi_della_prova();
        assert!(d.contains(&format!("] apri dalla cartella: {} byte, tipo pptx, aperta in ", lezione.len())), "{d}");
        assert!(d.contains(" ms, 4 slide\n"), "{d}");
        assert!(!d.contains("Zanzibar"), "{d}");
        indietro(&mut h);
        assert_eq!(h.state().schermata, Schermata::Home);
        assert!(h.state().slide.is_none());
    }

    #[test]
    fn una_presentazione_vecchia_dice_perche_non_si_apre() {
        let h = telefono_col_file("slide-vecchia", "Vecchia Zanzibar.ppt", &tascabile_slide::prova::ppt_vecchio());
        let messaggio = tascabile_slide::Genere::PowerPointVecchio.messaggio();
        assert_eq!(h.state().errore.as_ref().map(|e| e.testo.clone()), Some(format!("Non riesco ad aprire il file: {messaggio}")));
        assert_eq!(h.state().schermata, Schermata::Home);
        assert!(h.state().home.recenti().is_empty());
        let d = diagnosi_della_prova();
        assert!(d.contains("] apri dalla cartella: 1024 byte, tipo ppt, errore di apertura (formato vecchio di PowerPoint)\n"), "{d}");
    }

    #[test]
    fn il_nome_di_una_presentazione_arrivata_da_un_altra_app() {
        let nome = |nome: Option<&str>, mime: Option<&str>| {
            nome_di_apri_con(&OpenRequest { uri: "content://x/42".into(), mime: mime.map(Into::into), nome: nome.map(Into::into) })
        };
        let ppsx = "application/vnd.openxmlformats-officedocument.presentationml.slideshow";
        assert_eq!(nome(Some("Lezione.pptx"), None), "Lezione.pptx");
        assert_eq!(nome(Some("Lezione.PPTX"), Some(schermo_slide::MIME_PPTX)), "Lezione.PPTX");
        assert_eq!(nome(Some("Lezione"), Some(schermo_slide::MIME_PPTX)), "Lezione.pptx");
        assert_eq!(nome(Some("Lezione"), Some(ppsx)), "Lezione.ppsx");
        assert_eq!(nome(None, Some(ppsx)), "presentazione.ppsx");
        // Un documento resta com'era.
        assert_eq!(nome(Some("Verbale.docx"), None), "Verbale.docx");
    }

    #[test]
    fn una_presentazione_da_un_altra_app_senza_nome_si_apre_nelle_slide() {
        let (piattaforma, dir) = piattaforma_di_prova("slide-apri-con");
        let arrivato = dir.join("arrivato");
        std::fs::write(&arrivato, tascabile_slide::prova::LEZIONE).unwrap();
        piattaforma.simula_apri_con(arrivato.to_str().unwrap(), Some(schermo_slide::MIME_PPTX));
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        aspetta_le_slide(&mut h);
        assert_eq!(h.state().schermata, Schermata::Slide);
        h.get_by_label("Slide · 4 slide");
        assert_eq!(h.state().home.recenti()[0].nome, "presentazione.pptx");
    }

    /// La lezione aperta nelle Slide, con le slide sullo schermo disegnate.
    fn telefono_con_le_slide(prova: &str) -> Telefono {
        let mut h = telefono_col_file(prova, "Lezione Zanzibar.pptx", tascabile_slide::prova::LEZIONE);
        aspetta_le_slide_disegnate(&mut h);
        h
    }

    /// Passi finché le slide sullo schermo non sono disegnate, al massimo 20 s.
    fn aspetta_le_slide_disegnate(h: &mut Telefono) {
        let fine = Instant::now() + Duration::from_secs(20);
        h.step();
        while h.state().slide_in_attesa() != Some(false) {
            assert!(Instant::now() < fine, "le slide non sono state disegnate in 20 s");
            std::thread::sleep(Duration::from_millis(5));
            h.step();
        }
        h.run_steps(2);
    }

    #[test]
    fn le_slide_si_leggono_una_sotto_l_altra() {
        let mut h = telefono_con_le_slide("slide-lettura");
        // L'indicatore «1 / 4» sfuma dopo un attimo, e le prove del lettore lo guardano da vicino.
        assert_eq!(h.state().slide_corrente(), Some(0));
        indietro(&mut h);
        assert_eq!(h.state().schermata, Schermata::Home);
        assert!(diagnosi_della_prova().contains("] slide chiuse: 4 slide disegnate, in media "), "{}", diagnosi_della_prova());
    }

    #[test]
    fn le_note_sono_quelle_della_slide_che_si_legge() {
        let mut h = telefono_con_le_slide("slide-note");
        tocca(&mut h, schermo_slide::NOTE);
        h.run_steps(4);
        h.get_by_label(schermo_slide::NESSUNA_NOTA);
        // Indietro chiude il foglio e si resta nelle Slide.
        indietro(&mut h);
        assert!(!h.state().fogli_slide.note.aperto());
        assert_eq!(h.state().schermata, Schermata::Slide);
        h.state_mut().slide.as_mut().unwrap().lettore.vai_alla_slide(1);
        aspetta_le_slide_disegnate(&mut h);
        assert_eq!(h.state().slide_corrente(), Some(1));
        tocca(&mut h, schermo_slide::NOTE);
        h.run_steps(4);
        h.get_by_label("Nota per il relatore");
        let d = diagnosi_della_prova();
        assert!(d.contains("] slide: note della slide 2 di 4, 20 caratteri\n"), "{d}");
        assert!(!d.contains("Nota per il relatore"), "{d}");
    }

    #[test]
    fn una_miniatura_porta_alla_sua_slide() {
        let mut h = telefono_con_le_slide("slide-miniature");
        tocca(&mut h, schermo_slide::MINIATURE);
        h.run_steps(4);
        assert!(h.state().fogli_slide.miniature.aperto());
        tocca(&mut h, "Slide 3");
        aspetta_le_slide_disegnate(&mut h);
        assert_eq!(h.state().slide_corrente(), Some(2));
        assert!(!h.state().fogli_slide.miniature.aperto());
        assert!(diagnosi_della_prova().contains("] slide: miniatura, salto alla slide 3 di 4\n"));
    }

    /// «Presenta» dalla barra in fondo delle Slide, fino al primo fotogramma disegnato.
    fn presenta(h: &mut Telefono) {
        tocca(h, schermo_slide::PRESENTA);
        let fine = Instant::now() + Duration::from_secs(20);
        while h.state().presentazione_in_attesa() != Some(false) {
            assert!(Instant::now() < fine, "la presentazione non è partita in 20 s");
            std::thread::sleep(Duration::from_millis(5));
            h.step();
        }
        h.run_steps(2);
    }

    #[test]
    fn si_presenta_dalla_slide_che_si_legge_e_si_torna_li() {
        let mut h = telefono_con_le_slide("slide-presenta");
        h.state_mut().slide.as_mut().unwrap().lettore.vai_alla_slide(1);
        aspetta_le_slide_disegnate(&mut h);
        presenta(&mut h);
        // Schermo acceso, barre di sistema via, tasti del volume presi dal Java.
        assert!(h.state().piattaforma.presentazione_attiva());
        assert_eq!(h.state().slide_presentata(), Some((1, 0)));
        // Pagina giù, che il Java manda al posto del volume, va avanti.
        h.key_press(egui::Key::PageDown);
        h.run_steps(2);
        assert_eq!(h.state().slide_presentata(), Some((1, 1)));
        // Indietro esce: la lettura torna sulla slide presentata.
        indietro(&mut h);
        assert_eq!(h.state().slide_presentata(), None);
        assert!(!h.state().piattaforma.presentazione_attiva());
        assert_eq!(h.state().schermata, Schermata::Slide);
        assert_eq!(h.state().slide_corrente(), Some(1));
        let d = diagnosi_della_prova();
        assert!(d.contains("] presentazione: dalla slide 2 di 4\n"), "{d}");
        assert!(d.contains("] presentazione finita alla slide 2 di 4: "), "{d}");
    }

    #[test]
    fn la_barra_della_presentazione_ha_penna_nero_ed_esci() {
        let mut h = telefono_con_le_slide("slide-barra");
        presenta(&mut h);
        // Un tocco nella fascia in fondo apre la barra; un tocco su Penna non la chiude.
        tocca_in(&mut h, egui::pos2(200.0, 900.0));
        h.run_steps(3);
        tocca(&mut h, schermo_slide::PENNA);
        assert_eq!(h.state().penna_della_presentazione(), Some(true));
        tocca(&mut h, schermo_slide::NERO);
        assert_eq!(h.state().nero_della_presentazione(), Some(true));
        // Il nero chiude la barra; un tocco toglie il nero senza andare avanti.
        tocca_in(&mut h, egui::pos2(300.0, 400.0));
        h.run_steps(3);
        assert_eq!(h.state().nero_della_presentazione(), Some(false));
        assert_eq!(h.state().slide_presentata(), Some((0, 0)));
        tocca_in(&mut h, egui::pos2(200.0, 900.0));
        h.run_steps(3);
        tocca(&mut h, schermo_slide::ESCI);
        assert_eq!(h.state().slide_presentata(), None);
    }

    /// Nella presentazione, un tocco nella fascia in fondo: la barra si apre o si chiude.
    fn barra_della_presentazione(h: &mut Telefono) {
        tocca_in(h, egui::pos2(200.0, 900.0));
        h.run_steps(3);
    }

    #[test]
    fn il_relatore_vede_le_note_e_la_slide_che_viene_dopo() {
        let mut h = telefono_con_le_slide("slide-relatore");
        presenta(&mut h);
        barra_della_presentazione(&mut h);
        tocca(&mut h, schermo_slide::NOTE);
        h.get_by_label(schermo_slide::NESSUNA_NOTA);
        h.get_by_label(schermo_slide::DOPO);
        // Le note seguono la slide che si presenta.
        h.key_press(egui::Key::PageDown);
        h.run_steps(3);
        assert_eq!(h.state().slide_presentata(), Some((1, 0)));
        h.get_by_label("Nota per il relatore");
        // Le note restano aperte anche chiudendo la barra; si chiudono dal loro pulsante.
        barra_della_presentazione(&mut h);
        h.get_by_label("Nota per il relatore");
        barra_della_presentazione(&mut h);
        tocca(&mut h, schermo_slide::NOTE);
        assert!(h.query_by_label("Nota per il relatore").is_none());
    }

    #[test]
    fn dalla_griglia_della_presentazione_si_salta_a_una_slide() {
        let mut h = telefono_con_le_slide("slide-salto");
        presenta(&mut h);
        barra_della_presentazione(&mut h);
        tocca(&mut h, schermo_slide::SLIDE);
        h.run_steps(4);
        // Anche alla nascosta: chi presenta può mostrarla apposta.
        tocca(&mut h, "Slide 4");
        h.run_steps(3);
        assert_eq!(h.state().slide_presentata().map(|(s, _)| s), Some(3));
        assert!(!h.state().fogli_slide.miniature.aperto());
        assert!(diagnosi_della_prova().contains("] presentazione: salto alla slide 4 di 4\n"));
    }

    #[test]
    fn col_proiettore_le_slide_vanno_li_e_il_relatore_resta_sul_telefono() {
        let mut h = telefono_con_le_slide("slide-proiettore");
        h.state().piattaforma.simula_schermo_esterno(Some((1920, 1080)));
        presenta(&mut h);
        let fine = Instant::now() + Duration::from_secs(20);
        while h.state().piattaforma.proiettate().is_empty() {
            assert!(Instant::now() < fine, "niente sullo schermo esterno in 20 s");
            std::thread::sleep(Duration::from_millis(5));
            h.step();
        }
        assert_eq!(h.state().piattaforma.proiettate(), vec![(1920, 1080)]);
        // Il relatore si apre da sé: le note e quello che viene dopo.
        h.get_by_label(schermo_slide::DOPO);
        // Un passo avanti va anche allo schermo esterno.
        h.key_press(egui::Key::PageDown);
        while h.state().piattaforma.proiettate().len() < 2 {
            assert!(Instant::now() < fine, "il passo dopo non è andato allo schermo esterno");
            std::thread::sleep(Duration::from_millis(5));
            h.step();
        }
        // Staccato lo schermo, la presentazione continua sul telefono.
        h.state().piattaforma.simula_schermo_esterno(None);
        h.run_steps(3);
        h.key_press(egui::Key::PageDown);
        h.run_steps(10);
        assert_eq!(h.state().piattaforma.proiettate().len(), 2);
        // Il secondo passo c'è stato anche senza lo schermo esterno: la slide 2 col primo punto.
        assert_eq!(h.state().slide_presentata(), Some((1, 1)));
        let d = diagnosi_della_prova();
        assert!(d.contains("] presentazione: schermo esterno 1920 × 1080\n"), "{d}");
        assert!(d.contains("] presentazione: schermo esterno staccato\n"), "{d}");
    }

    #[test]
    fn un_documento_protetto_lo_dice_sotto_la_barra() {
        let (piattaforma, dir) = piattaforma_di_prova("word-protetto");
        let mut s = documento::apri_byte("prova.txt", b"Verbale della riunione").unwrap();
        s.run("review.restrict", &serde_json::json!({ "mode": "readOnly" })).unwrap();
        let percorso = dir.join("radice/Verbale Zanzibar.docx");
        std::fs::write(&percorso, wordcraft_engine::io::save_bytes("prova.docx", &s.doc).unwrap()).unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        h.state_mut().apri_file(&percorso, "Verbale Zanzibar.docx");
        h.run_steps(3);
        h.get_by_label(tascabile_word::editor::PROTETTO_SOLA_LETTURA);
    }

    /// Il menu delle Slide, poi la voce `voce`.
    fn dal_menu_delle_slide(h: &mut Telefono, voce: &str) {
        tocca(h, schermo_slide::MENU);
        h.run_steps(2);
        tocca(h, voce);
    }

    /// Passi finché il PDF delle slide non è scritto, al massimo 20 s.
    fn aspetta_il_pdf_delle_slide(h: &mut Telefono) {
        let fine = Instant::now() + Duration::from_secs(20);
        h.step();
        while h.state().pdf_delle_slide_in_corso() {
            assert!(Instant::now() < fine, "il PDF delle slide non è pronto in 20 s");
            std::thread::sleep(Duration::from_millis(5));
            h.step();
        }
        h.run_steps(2);
    }

    #[test]
    fn il_pdf_delle_slide_va_in_documenti_tascabile_e_si_condivide() {
        let (piattaforma, dir) = piattaforma_di_prova("slide-pdf");
        let percorso = dir.join("radice/Lezione Zanzibar.pptx");
        std::fs::write(&percorso, tascabile_slide::prova::LEZIONE).unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        h.state_mut().apri_file(&percorso, "Lezione Zanzibar.pptx");
        aspetta_le_slide(&mut h);
        dal_menu_delle_slide(&mut h, schermo_slide::ESPORTA_PDF);
        aspetta_il_pdf_delle_slide(&mut h);
        let pdf = dir.join("radice/Documents/Tascabile/Lezione Zanzibar.pdf");
        assert_eq!(tascabile_pdf::Pdf::apri(std::fs::read(&pdf).unwrap(), None).unwrap().pagine(), 3);
        h.get_by_label("PDF pronto in Documenti/Tascabile: Lezione Zanzibar.pdf");
        tocca(&mut h, CONDIVIDI_PDF);
        assert!(dir.join("cache/condivisi").exists());
        // Con le note: un altro file, accanto.
        dal_menu_delle_slide(&mut h, schermo_slide::PDF_CON_NOTE);
        aspetta_il_pdf_delle_slide(&mut h);
        assert!(dir.join("radice/Documents/Tascabile/Lezione Zanzibar - note.pdf").is_file());
        let d = diagnosi_della_prova();
        assert!(d.contains("] slide: pdf esportato, "), "{d}");
        assert!(d.contains("] slide: pdf con le note esportato, "), "{d}");
        assert!(!d.contains("Zanzibar"), "{d}");
    }

    /// Il ripasso delle Slide (tappa I, I7): una presentazione in 4:3, una con le slide tutte
    /// nascoste e una senza slide, aperte, lette, presentate ed esportate dall'interfaccia.
    #[test]
    fn le_presentazioni_dei_casi_limite_si_leggono_si_presentano_e_si_esportano() {
        // 4:3: si legge, si presenta dall'inizio fino alla fine, il PDF ha due pagine.
        let mut h = telefono_col_file("slide-quattro-terzi", "Quattro terzi.pptx", tascabile_slide::prova::QUATTRO_TERZI);
        aspetta_le_slide_disegnate(&mut h);
        h.get_by_label("Slide · 2 slide");
        presenta(&mut h);
        assert_eq!(h.state().slide_presentata(), Some((0, 0)));
        h.key_press(egui::Key::PageDown);
        h.key_press(egui::Key::PageDown);
        h.run_steps(2);
        h.get_by_label(tascabile_slide::FINE);
        h.key_press(egui::Key::PageDown);
        h.run_steps(2);
        assert_eq!(h.state().slide_presentata(), None);
        dal_menu_delle_slide(&mut h, schermo_slide::ESPORTA_PDF);
        aspetta_il_pdf_delle_slide(&mut h);
        h.get_by_label("PDF pronto in Documenti/Tascabile: Quattro terzi.pdf");
        // Tutte nascoste: si leggono velate, si presenta dall'inizio (la prima, nascosta), il PDF
        // dice perché non si fa.
        let mut h = telefono_col_file("slide-nascoste", "Nascoste.pptx", tascabile_slide::prova::TUTTE_NASCOSTE);
        aspetta_le_slide_disegnate(&mut h);
        dal_menu_delle_slide(&mut h, schermo_slide::PRESENTA_DALL_INIZIO);
        assert_eq!(h.state().slide_presentata(), Some((0, 0)));
        indietro(&mut h);
        dal_menu_delle_slide(&mut h, schermo_slide::ESPORTA_PDF);
        aspetta_il_pdf_delle_slide(&mut h);
        assert!(h.state().errore.as_ref().is_some_and(|e| e.testo.contains("tutte le slide sono nascoste")));
        assert!(diagnosi_della_prova().contains("] slide: pdf non esportato (niente da esportare)\n"));
        // Senza slide: la barra lo dice, la presentazione è subito alla fine e non aspetta, il PDF
        // dice che non ci sono slide.
        let mut h = telefono_col_file("slide-vuota", "Vuota.pptx", tascabile_slide::prova::SENZA_SLIDE);
        aspetta_le_slide_disegnate(&mut h);
        h.get_by_label("Slide · 0 slide");
        presenta(&mut h);
        h.get_by_label(tascabile_slide::FINE);
        assert_eq!(h.state().presentazione_in_attesa(), Some(false));
        indietro(&mut h);
        dal_menu_delle_slide(&mut h, schermo_slide::ESPORTA_PDF);
        aspetta_il_pdf_delle_slide(&mut h);
        assert!(h.state().errore.as_ref().is_some_and(|e| e.testo.contains("la presentazione non ha slide")), "{:?}", h.state().errore);
        assert!(diagnosi_della_prova().contains("] slide: pdf non esportato (senza slide)\n"));
    }

    #[test]
    fn presenta_dall_inizio_e_un_ppsx_parte_gia_in_presentazione() {
        let mut h = telefono_con_le_slide("slide-inizio");
        h.state_mut().slide.as_mut().unwrap().lettore.vai_alla_slide(2);
        aspetta_le_slide_disegnate(&mut h);
        dal_menu_delle_slide(&mut h, schermo_slide::PRESENTA_DALL_INIZIO);
        assert_eq!(h.state().slide_presentata(), Some((0, 0)));
        // Una presentazione salvata come .ppsx si apre già presentata, dalla prima slide.
        let h = telefono_col_file("slide-ppsx", "Mostra Zanzibar.ppsx", tascabile_slide::prova::LEZIONE);
        assert_eq!(h.state().slide_presentata(), Some((0, 0)));
    }

    /// «Strumenti» nella barra in fondo, poi la voce `voce` del foglio.
    fn dagli_strumenti(h: &mut Telefono, voce: &str) {
        tocca(h, pdf_lavoro::STRUMENTI);
        h.run_steps(6);
        // Il foglio è più alto dello schermo: la voce si porta in vista, come col dito.
        h.get_by_role_and_label(Role::Button, voce).scroll_to_me();
        h.run_steps(4);
        tocca(h, voce);
        assert!(!h.state().fogli_pdf.strumenti.aperto(), "il foglio degli strumenti è rimasto aperto");
    }

    fn pagina_corrente(h: &Telefono) -> usize {
        h.state().pdf.as_ref().expect("nessun PDF nel lettore").lettore.pagina_corrente()
    }

    #[test]
    fn la_ricerca_nel_pdf_come_trova_di_word() {
        let mut h = telefono_col_pdf("pdf-ricerca", tascabile_pdf::prova::modulo());
        tocca(&mut h, schermo_pdf::CERCA);
        assert_eq!(h.state().fuoco, Fuoco::Campo);
        assert!(h.state().piattaforma.keyboard_visible(), "la tastiera si apre per il campo");
        h.get_by_role_and_label(Role::Button, schermo_pdf::CHIUDI_CERCA);
        // In maiuscolo: «trattamento» c'è 4 volte nella prima pagina e 12 in ognuna delle note.
        scrivi(&mut h, "TRATTAMENTO");
        assert_eq!(h.state().pdf.as_ref().unwrap().trova.as_ref().unwrap().testo(), "TRATTAMENTO");
        aspetta_il_lettore(&mut h);
        h.get_by_label("1 di 28");
        tocca(&mut h, schermo_editor::DOPO);
        h.get_by_label("2 di 28");
        h.key_press(egui::Key::Enter);
        h.run_steps(2);
        h.get_by_label("3 di 28");
        tocca(&mut h, schermo_editor::PRIMA);
        tocca(&mut h, schermo_editor::PRIMA);
        tocca(&mut h, schermo_editor::PRIMA);
        // Prima del primo si gira all'ultimo, nella terza pagina.
        h.get_by_label("28 di 28");
        h.run_steps(2);
        assert_eq!(pagina_corrente(&h), 2);
        // Una parola che non c'è.
        scrivi(&mut h, "zz");
        aspetta_il_lettore(&mut h);
        h.get_by_label("Nessun risultato");
        let d = diagnosi_della_prova();
        assert!(
            d.contains(
                "] pdf: ricerca aperta
"
            ),
            "{d}"
        );
        assert!(d.contains("] pdf: ricerca finita, 28 risultati in 3 pagine lette, "), "{d}");
        assert!(
            d.contains(
                "] pdf: ricerca di 13 caratteri
"
            ),
            "{d}"
        );
        for parola in ["TRATTAMENTO", "trattamento", "Zanzibar"] {
            assert!(!d.contains(parola), "«{parola}» nella diagnosi:\n{d}");
        }
        // Indietro chiude la tastiera, poi la ricerca, e torna la barra alta.
        indietro(&mut h);
        assert!(!h.state().piattaforma.keyboard_visible());
        assert!(h.state().pdf.as_ref().unwrap().trova.is_some());
        indietro(&mut h);
        assert!(h.state().pdf.as_ref().unwrap().trova.is_none());
        assert_eq!(h.state().pdf.as_ref().unwrap().lettore.conteggio(), None);
        h.get_by_role_and_label(Role::Button, schermo_pdf::CERCA);
        assert_eq!(h.state().schermata, Schermata::Pdf);
    }

    #[test]
    fn indice_vai_a_pagina_e_miniature_portano_alla_pagina() {
        let mut h = telefono_col_pdf("pdf-orientarsi", tascabile_pdf::prova::modulo());
        assert_eq!(pagina_corrente(&h), 0);
        // L'indice dal foglio Strumenti: i segnalibri rientrati, col numero della pagina.
        dagli_strumenti(&mut h, schermo_pdf::INDICE);
        h.run_steps(2);
        h.get_by_label("Consenso informato al trattamento, pagina 1");
        h.get_by_label("Firma, pagina 1");
        tocca(&mut h, "Note informative, pagina 2");
        h.run_steps(2);
        assert!(h.query_by_label("Firma, pagina 1").is_none(), "l'indice è rimasto aperto");
        assert_eq!(pagina_corrente(&h), 1);
        // «Vai a pagina»: il campo prende la tastiera e tiene solo le cifre.
        dagli_strumenti(&mut h, schermo_pdf::VAI_A_PAGINA);
        h.run_steps(2);
        assert_eq!(h.state().fuoco, Fuoco::Campo);
        assert!(h.state().piattaforma.keyboard_visible());
        h.get_by_label("Il PDF ha 3 pagine.");
        // La tastiera dei numeri, solo mentre il campo ha il fuoco.
        assert_eq!(h.state().piattaforma.tipo_della_tastiera(), TipoTastiera::Numeri);
        scrivi(&mut h, "x0");
        assert_eq!(h.state().pdf.as_ref().unwrap().vai.testo(), "0");
        tocca(&mut h, schermo_pdf::VAI);
        h.get_by_label("Scrivi un numero da 1 a 3.");
        scrivi(&mut h, "3");
        h.key_press(egui::Key::Enter);
        h.run_steps(3);
        assert!(h.query_by_label("Il PDF ha 3 pagine.").is_none(), "il foglio è rimasto aperto");
        assert!(!h.state().piattaforma.keyboard_visible(), "la tastiera è rimasta aperta");
        assert_eq!(h.state().fuoco, Fuoco::Documento);
        assert_eq!(h.state().piattaforma.tipo_della_tastiera(), TipoTastiera::Testo);
        assert_eq!(pagina_corrente(&h), 2);
        // Le miniature: un tocco sulla prima porta alla prima pagina.
        dagli_strumenti(&mut h, schermo_pdf::MINIATURE);
        aspetta_il_lettore(&mut h);
        assert_eq!(h.state().pdf.as_ref().unwrap().lettore.miniature_pronte(), [0, 1, 2]);
        tocca(&mut h, "Pagina 1");
        h.run_steps(2);
        assert!(h.query_by_label("Pagina 2").is_none(), "le miniature sono rimaste aperte");
        assert_eq!(pagina_corrente(&h), 0);
        let d = diagnosi_della_prova();
        for riga in [
            "] pdf: indice aperto, 6 segnalibri
",
            "] pdf: indice, salto alla pagina 2
",
            "] pdf: vai a pagina, numero che non va
",
            "] pdf: vai alla pagina 3 di 3
",
            "] pdf: miniature aperte, 3 pagine
",
            "] pdf: miniatura, salto alla pagina 1
",
        ] {
            assert!(d.contains(riga), "manca «{riga}»:\n{d}");
        }
        for parola in ["Note informative", "Consenso", "Zanzibar"] {
            assert!(!d.contains(parola), "«{parola}» nella diagnosi:\n{d}");
        }
    }

    #[test]
    fn un_pdf_senza_segnalibri_lo_dice_nell_indice() {
        let mut h = telefono_col_pdf("pdf-senza-indice", tascabile_pdf::prova::pdf(&[tascabile_pdf::prova::Pagina::bianca(595.0, 842.0)]));
        dagli_strumenti(&mut h, schermo_pdf::INDICE);
        h.run_steps(2);
        h.get_by_label("Nessun segnalibro");
        indietro(&mut h);
        assert!(h.query_by_label("Nessun segnalibro").is_none());
        assert_eq!(h.state().schermata, Schermata::Pdf);
    }

    /// Il motore del PDF ha finito i comandi mandati, e qualche frame dopo.
    fn aspetta_il_motore(h: &mut Telefono) {
        h.state_mut().aspetta_il_motore_pdf();
        h.run_steps(3);
    }

    /// «Organizza pagine» dal foglio Strumenti (chiuso prima, se era aperto), la prima pagina
    /// scelta e «Ruota a destra»; poi la griglia si chiude, se l'avviso delle firme non aspetta.
    fn ruota_dagli_strumenti(h: &mut Telefono) {
        chiudi_organizza(h);
        dagli_strumenti(h, pdf_lavoro::Strumento::OrganizzaPagine.nome());
        h.run_steps(4);
        h.get_by_role_and_label(Role::CheckBox, "Pagina 1").click();
        h.run_steps(2);
        assert_eq!(h.state().pagine_scelte(), Some(vec![0]));
        tocca(h, pdf_pagine::RUOTA_A_DESTRA);
        if h.query_by_label(pdf_lavoro::PDF_FIRMATO).is_none() {
            chiudi_organizza(h);
        }
    }

    /// «Chiudi» nella barra alta di «Organizza pagine», se è aperto.
    fn chiudi_organizza(h: &mut Telefono) {
        if h.state().pagine_scelte().is_some() {
            tocca(h, pdf_pagine::CHIUDI);
            assert!(h.state().pagine_scelte().is_none(), "«Organizza pagine» è rimasto aperto");
        }
    }

    /// Il modulo di prova di tre pagine `byte` aperto dalla cartella, con la sua misura di pagina.
    fn misura_della_pagina(h: &Telefono, pagina: usize) -> (f32, f32) {
        let m = h.state().pdf.as_ref().unwrap().motore.stato().misure[pagina];
        (m.larghezza, m.altezza)
    }

    /// Il giro completo: una rotazione dal foglio Strumenti, Annulla e Ripeti nella barra alta con
    /// «Modifiche non salvate», Salva sul file (riscritto tutto), e il PDF riaperto ha la pagina girata.
    #[test]
    fn ruotare_annullare_ripetere_salvare_e_riaprire() {
        let byte = tascabile_pdf::prova::modulo();
        let (piattaforma, dir) = piattaforma_di_prova("pdf-giro");
        let percorso = dir.join("radice/Modulo Zanzibar.pdf");
        std::fs::write(&percorso, &byte).unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        h.state_mut().apri_file(&percorso, "Modulo Zanzibar.pdf");
        aspetta_il_lettore(&mut h);
        assert_eq!(misura_della_pagina(&h, 0), (595.0, 842.0));
        ruota_dagli_strumenti(&mut h);
        aspetta_il_motore(&mut h);
        assert_eq!(misura_della_pagina(&h, 0), (842.0, 595.0));
        assert_eq!(h.state().pdf_modificato(), Some(true));
        h.get_by_label(pdf_lavoro::MODIFICHE_NON_SALVATE);
        assert!(!h.get_by_role_and_label(Role::Button, pdf_lavoro::ANNULLA_MODIFICA).accesskit_node().is_disabled());
        assert!(h.get_by_role_and_label(Role::Button, pdf_lavoro::RIPETI_MODIFICA).accesskit_node().is_disabled());
        // Il lettore ridisegna la pagina girata: larga.
        aspetta_il_lettore(&mut h);
        assert_eq!(h.state().pdf.as_ref().unwrap().lettore.pagine_pronte()[0], 0);
        // Annulla: di nuovo com'era, e pulito; Ripeti la rimette.
        tocca(&mut h, pdf_lavoro::ANNULLA_MODIFICA);
        aspetta_il_motore(&mut h);
        assert_eq!((misura_della_pagina(&h, 0), h.state().pdf_modificato()), ((595.0, 842.0), Some(false)));
        assert!(h.get_by_role_and_label(Role::Button, pdf_lavoro::ANNULLA_MODIFICA).accesskit_node().is_disabled());
        tocca(&mut h, pdf_lavoro::RIPETI_MODIFICA);
        aspetta_il_motore(&mut h);
        assert_eq!((misura_della_pagina(&h, 0), h.state().pdf_modificato()), ((842.0, 595.0), Some(true)));
        // Salva dal menu: sul file, riscritto tutto in una revisione sola.
        tocca(&mut h, schermo_pdf::MENU);
        tocca(&mut h, pdf_lavoro::SALVA);
        aspetta_il_motore(&mut h);
        assert_eq!(h.state().pdf_modificato(), Some(false));
        let salvati = std::fs::read(&percorso).unwrap();
        assert_eq!(salvati.windows(5).filter(|w| w == b"%%EOF").count(), 1, "il salvataggio ha lasciato le revisioni di prima");
        assert_eq!(tascabile_pdf::Pdf::apri(salvati, None).unwrap().misura(0), Some(tascabile_pdf::Misura { larghezza: 842.0, altezza: 595.0 }));
        h.get_by_label_contains("Salvato · ");
        h.get_by_label("Salvato");
        // Uscendo non chiede niente: è salvato.
        indietro(&mut h);
        assert_eq!(h.state().schermata, Schermata::Home);
        let d = diagnosi_della_prova();
        for riga in ["] pdf: organizza, ruota 1 pagina\n", "] pdf: modifica (pagine) chiesta\n", "] salva pdf: riuscito, tipo pdf, "] {
            assert!(d.contains(riga), "manca «{riga}»:\n{d}");
        }
        assert!(
            d.contains("] pdf: modifica (pagine) applicata in ") && d.contains("] pdf: annullato in ") && d.contains("] pdf: ripetuto in "),
            "{d}"
        );
        assert!(!d.contains("Zanzibar") && !d.contains("prove-app"), "{d}");
    }

    /// Un PDF arrivato con «Apri con» si salva in una copia in Documenti/Tascabile, che diventa il
    /// documento; l'originale resta com'è. Condividi dà i byte con le modifiche senza salvarle.
    #[test]
    fn un_pdf_arrivato_da_un_altra_app_si_salva_in_una_copia() {
        let byte = tascabile_pdf::prova::modulo();
        let (piattaforma, dir) = piattaforma_di_prova("pdf-copia");
        let arrivato = dir.join("arrivato.pdf");
        std::fs::write(&arrivato, &byte).unwrap();
        piattaforma.simula_apri_con_col_nome(arrivato.to_str().unwrap(), Some("application/pdf"), "Referto Zanzibar.pdf");
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        aspetta_il_lettore(&mut h);
        ruota_dagli_strumenti(&mut h);
        aspetta_il_motore(&mut h);
        // Condividi: il PDF con la pagina girata, e il documento resta da salvare.
        tocca(&mut h, schermo_pdf::MENU);
        tocca(&mut h, schermo_pdf::CONDIVIDI);
        aspetta_il_motore(&mut h);
        let condiviso = std::fs::read_dir(dir.join("cache/condivisi")).unwrap().flatten().next().unwrap().path().join("Referto Zanzibar.pdf");
        assert_eq!(tascabile_pdf::Pdf::apri(std::fs::read(&condiviso).unwrap(), None).unwrap().misura(0).unwrap().larghezza, 842.0);
        assert_eq!(h.state().pdf_modificato(), Some(true));
        // Salva: una copia in Documenti/Tascabile, col nome del PDF.
        tocca(&mut h, schermo_pdf::MENU);
        tocca(&mut h, pdf_lavoro::SALVA);
        aspetta_il_motore(&mut h);
        let copia = dir.join("radice/Documents/Tascabile/Referto Zanzibar.pdf");
        assert_eq!(tascabile_pdf::Pdf::apri(std::fs::read(&copia).unwrap(), None).unwrap().misura(0).unwrap().larghezza, 842.0);
        assert_eq!(std::fs::read(&arrivato).unwrap(), byte, "l'originale è cambiato");
        h.get_by_label("Salvato in Documenti/Tascabile: Referto Zanzibar.pdf");
        assert_eq!(h.state().home.recenti()[0].chiave, copia.to_str().unwrap());
        // Da qui Salva scrive sulla copia, ed è un altro giro: «Salva una copia» ne fa un'altra.
        ruota_dagli_strumenti(&mut h);
        aspetta_il_motore(&mut h);
        tocca(&mut h, schermo_pdf::MENU);
        tocca(&mut h, pdf_lavoro::SALVA);
        aspetta_il_motore(&mut h);
        h.get_by_label("Salvato");
        let misure = h.state().pdf.as_ref().unwrap().motore.stato().misure.clone();
        assert_eq!(tascabile_pdf::Pdf::apri(std::fs::read(&copia).unwrap(), None).unwrap().misure(), misure);
        tocca(&mut h, schermo_pdf::MENU);
        tocca(&mut h, pdf_lavoro::SALVA_COPIA);
        aspetta_il_motore(&mut h);
        assert!(dir.join("radice/Documents/Tascabile/Referto Zanzibar (2).pdf").exists());
        h.get_by_label("Copia salvata in Documenti/Tascabile: Referto Zanzibar (2).pdf");
        let d = diagnosi_della_prova();
        assert!(d.contains("] condividi pdf aperto: riuscito, tipo pdf, ") && d.contains("] salva una copia pdf: riuscito, tipo pdf, "), "{d}");
        assert!(!d.contains("Zanzibar"), "{d}");
    }

    /// Indietro con modifiche: «Salvare le modifiche?». Annulla resta, Non salvare esce senza
    /// toccare il file, Salva esce quando il salvataggio è finito.
    #[test]
    fn uscendo_con_modifiche_chiede_di_salvare() {
        let byte = tascabile_pdf::prova::modulo();
        let (piattaforma, dir) = piattaforma_di_prova("pdf-uscita");
        let percorso = dir.join("radice/Modulo.pdf");
        std::fs::write(&percorso, &byte).unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        h.state_mut().apri_file(&percorso, "Modulo.pdf");
        aspetta_il_lettore(&mut h);
        // Indietro subito dopo la modifica, prima che il motore risponda: chiede lo stesso.
        ruota_dagli_strumenti(&mut h);
        assert!(h.state().pdf.as_ref().unwrap().motore.modifica_in_corso() || h.state().pdf_modificato() == Some(true));
        indietro(&mut h);
        h.get_by_label("Salvare le modifiche?");
        aspetta_il_motore(&mut h);
        tocca(&mut h, "Annulla");
        assert_eq!(h.state().schermata, Schermata::Pdf);
        // La freccia della barra alta chiede lo stesso.
        tocca(&mut h, schermo_pdf::INDIETRO);
        tocca(&mut h, "Non salvare");
        assert_eq!(h.state().schermata, Schermata::Home);
        assert!(h.state().pdf.is_none());
        assert_eq!(std::fs::read(&percorso).unwrap(), byte);
        // Di nuovo, con Salva: si esce quando il motore ha scritto il file.
        h.state_mut().apri_file(&percorso, "Modulo.pdf");
        aspetta_il_lettore(&mut h);
        ruota_dagli_strumenti(&mut h);
        aspetta_il_motore(&mut h);
        indietro(&mut h);
        tocca(&mut h, "Salva");
        aspetta_il_motore(&mut h);
        assert_eq!(h.state().schermata, Schermata::Home);
        assert!(h.state().pdf.is_none());
        assert_eq!(tascabile_pdf::Pdf::apri(std::fs::read(&percorso).unwrap(), None).unwrap().misura(0).unwrap().larghezza, 842.0);
    }

    /// Un PDF firmato: la prima modifica che renderebbe non valida la firma lo dice e propone una
    /// copia. Annulla lascia tutto com'era; «Lavora su una copia» modifica, e Salva scrive una
    /// copia, mai l'originale firmato; le modifiche dopo non chiedono più.
    #[test]
    fn un_pdf_firmato_avvisa_prima_della_prima_modifica() {
        let firmato = tascabile_pdf::prova::firmato(tascabile_pdf::prova::modulo(), None);
        let (piattaforma, dir) = piattaforma_di_prova("pdf-firmato");
        let percorso = dir.join("radice/Consenso firmato.pdf");
        std::fs::write(&percorso, &firmato).unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        h.state_mut().apri_file(&percorso, "Consenso firmato.pdf");
        aspetta_il_lettore(&mut h);
        h.get_by_label("PDF · 3 pagine · firmato");
        ruota_dagli_strumenti(&mut h);
        h.run_steps(2);
        h.get_by_label(pdf_lavoro::PDF_FIRMATO);
        h.get_by_label(pdf_lavoro::MODIFICA_ORIGINALE);
        h.get_by_label(pdf_lavoro::LAVORA_SU_COPIA);
        tocca(&mut h, schermo_pdf::ANNULLA);
        aspetta_il_motore(&mut h);
        assert!(h.query_by_label(pdf_lavoro::PDF_FIRMATO).is_none());
        assert_eq!((misura_della_pagina(&h, 0), h.state().pdf_modificato()), ((595.0, 842.0), Some(false)));
        // Indietro vale Annulla.
        ruota_dagli_strumenti(&mut h);
        h.run_steps(2);
        indietro(&mut h);
        assert!(h.query_by_label(pdf_lavoro::PDF_FIRMATO).is_none());
        assert_eq!(h.state().schermata, Schermata::Pdf);
        // Su una copia: la modifica si fa; la seconda non chiede più.
        ruota_dagli_strumenti(&mut h);
        h.run_steps(2);
        tocca(&mut h, pdf_lavoro::LAVORA_SU_COPIA);
        aspetta_il_motore(&mut h);
        assert_eq!(misura_della_pagina(&h, 0), (842.0, 595.0));
        ruota_dagli_strumenti(&mut h);
        h.run_steps(2);
        assert!(h.query_by_label(pdf_lavoro::PDF_FIRMATO).is_none(), "l'avviso torna a ogni modifica");
        aspetta_il_motore(&mut h);
        chiudi_organizza(&mut h);
        tocca(&mut h, schermo_pdf::MENU);
        tocca(&mut h, pdf_lavoro::SALVA);
        aspetta_il_motore(&mut h);
        assert_eq!(std::fs::read(&percorso).unwrap(), firmato, "l'originale firmato è cambiato");
        assert!(dir.join("radice/Documents/Tascabile/Consenso firmato.pdf").exists());
        let d = diagnosi_della_prova();
        for riga in [
            "] pdf: firmato\n",
            "] pdf: modifica (pagine) in un pdf firmato, si avvisa\n",
            "] pdf: firme, modifica annullata\n",
            "] pdf: firme, si lavora su una copia\n",
        ] {
            assert!(d.contains(riga), "manca «{riga}»:\n{d}");
        }
    }

    /// Una certificazione «nessuna modifica»: il PDF si può solo leggere. Nel foglio Strumenti
    /// solo le voci per orientarsi, e perché.
    #[test]
    fn un_pdf_certificato_senza_modifiche_si_puo_solo_leggere() {
        let mut h = telefono_col_pdf("pdf-certificato", tascabile_pdf::prova::firmato(tascabile_pdf::prova::modulo(), Some(1)));
        h.get_by_label("PDF · 3 pagine · solo lettura");
        tocca(&mut h, pdf_lavoro::STRUMENTI);
        h.run_steps(6);
        assert!(h.query_by_label(pdf_lavoro::Strumento::OrganizzaPagine.nome()).is_none());
        h.get_by_role_and_label(Role::Button, schermo_pdf::INDICE);
        h.get_by_label_contains("si può solo leggere");
        indietro(&mut h);
        // Una modifica che arrivasse lo stesso: il dialogo, e niente cambia.
        h.state_mut().modifica_pdf_di_prova(tascabile_pdf::Modifica::ruota(vec![0], 90));
        h.run_steps(2);
        h.get_by_label(pdf_lavoro::SOLO_LETTURA);
        assert_eq!(h.state().pdf_modificato(), Some(false));
    }

    /// Quattro pagine A4 con scritto «Pagina 1», «Pagina 2» e così via.
    fn quattro_pagine() -> Vec<u8> {
        use tascabile_pdf::prova::{Pagina, pdf};
        let c = [
            "BT /F1 24 Tf 60 700 Td (Pagina 1) Tj ET",
            "BT /F1 24 Tf 60 700 Td (Pagina 2) Tj ET",
            "BT /F1 24 Tf 60 700 Td (Pagina 3) Tj ET",
            "BT /F1 24 Tf 60 700 Td (Pagina 4) Tj ET",
        ];
        pdf(&c.map(|contenuto| Pagina { larghezza: 595.0, altezza: 842.0, rotazione: 0, contenuto }))
    }

    /// Il testo di ogni pagina del PDF `byte`, in ordine.
    fn ordine_delle_pagine(byte: Vec<u8>, password: Option<&str>) -> Vec<String> {
        let p = tascabile_pdf::Pdf::apri(byte, password).unwrap();
        (0..p.pagine()).map(|i| p.testo(i).map(|t| t.glyphs.iter().map(|g| g.text.as_str()).collect::<String>()).unwrap_or_default()).collect()
    }

    /// Tocca la pagina `n` (da 1) nella griglia di «Organizza pagine».
    fn tocca_pagina(h: &mut Telefono, n: usize) {
        h.get_by_role_and_label(Role::CheckBox, &format!("Pagina {n}")).click();
        h.run_steps(2);
    }

    /// La pagina `da` (da 1) presa col dito (la pressione lunga) e lasciata davanti alla pagina
    /// `davanti_a`.
    fn trascina_pagina(h: &mut Telefono, da: usize, davanti_a: usize) {
        let p = h.get_by_role_and_label(Role::CheckBox, &format!("Pagina {da}")).rect().center();
        let a = h.get_by_role_and_label(Role::CheckBox, &format!("Pagina {davanti_a}")).rect();
        let q = egui::pos2(a.left() + 8.0, a.center().y);
        let tocco = |fase, pos| egui::Event::Touch { device_id: egui::TouchDeviceId(1), id: egui::TouchId(0), phase: fase, pos, force: None };
        let tasto =
            |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        h.event(tocco(egui::TouchPhase::Start, p));
        h.event(egui::Event::PointerMoved(p));
        h.event(tasto(p, true));
        h.run_steps(4);
        for i in 1..=8 {
            let x = p + (q - p) * (i as f32 / 8.0);
            h.event(tocco(egui::TouchPhase::Move, x));
            h.event(egui::Event::PointerMoved(x));
            h.run_steps(1);
        }
        h.event(tocco(egui::TouchPhase::End, q));
        h.event(tasto(q, false));
        h.run_steps(1);
        h.event(egui::Event::PointerGone);
        h.run_steps(2);
    }

    /// Passi finché il lavoro di «Pagine e file» e il motore non hanno finito.
    fn aspetta_il_lavoro(h: &mut Telefono) {
        finche(h, "il lavoro delle pagine", |h| !h.state().lavoro_file_in_corso());
        aspetta_il_motore(h);
    }

    #[test]
    fn un_comando_di_organizza_dato_mentre_il_motore_lavora_aspetta() {
        // Analisi del 27/10, pagine I1: con la griglia ancora vecchia, un secondo Elimina dato in
        // fretta toglieva la pagina sbagliata (i numeri di pagina erano quelli di prima).
        let tante = tascabile_pdf::prova::pdf(&vec![tascabile_pdf::prova::Pagina::bianca(595.0, 842.0); 400]);
        let mut h = telefono_col_pdf("organizza-in-fretta", tante);
        dagli_strumenti(&mut h, pdf_lavoro::Strumento::OrganizzaPagine.nome());
        h.run_steps(4);
        tocca_pagina(&mut h, 2);
        tocca(&mut h, pdf_pagine::ELIMINA);
        assert!(h.state().pdf.as_ref().unwrap().motore.modifica_in_corso(), "il motore ha già finito: la prova non prova niente");
        h.get_by_label(pdf_pagine::UN_ATTIMO);
        tocca_pagina(&mut h, 3);
        tocca(&mut h, pdf_pagine::ELIMINA);
        aspetta_il_motore(&mut h);
        h.run_steps(3);
        assert_eq!(h.state().misure_del_pdf().len(), 399, "il secondo Elimina è andato su una pagina vecchia");
        assert!(h.query_by_label(pdf_pagine::UN_ATTIMO).is_none());
    }

    /// «Organizza pagine» col motore vero: le azioni spente senza pagine scelte; una pagina girata
    /// a sinistra, duplicata, la copia eliminata (l'avviso non rimanda più a Salva pulito: Salva
    /// riscrive già tutto il file), l'ultima portata
    /// in cima col dito, una pagina bianca grande come la vicina, due pagine estratte in un PDF
    /// nuovo, Annulla nella barra della griglia; salvato, il PDF ha l'ordine nuovo.
    #[test]
    fn organizzare_le_pagine_a_schermo() {
        let (piattaforma, dir) = piattaforma_di_prova("pdf-organizza");
        let percorso = dir.join("radice/Lettera Zanzibar.pdf");
        std::fs::write(&percorso, quattro_pagine()).unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        h.state_mut().apri_file(&percorso, "Lettera Zanzibar.pdf");
        aspetta_il_lettore(&mut h);
        dagli_strumenti(&mut h, pdf_lavoro::Strumento::OrganizzaPagine.nome());
        h.run_steps(4);
        assert_eq!(h.state().pagine_scelte(), Some(vec![]));
        h.get_by_label("Tocca per scegliere, tieni premuto per spostare");
        assert!(h.get_by_role_and_label(Role::Button, pdf_pagine::ELIMINA).accesskit_node().is_disabled());
        // La seconda girata a sinistra.
        tocca_pagina(&mut h, 2);
        h.get_by_label("1 pagina scelta su 4");
        tocca(&mut h, pdf_pagine::RUOTA_A_SINISTRA);
        aspetta_il_motore(&mut h);
        assert_eq!((h.state().misure_del_pdf()[1].larghezza, h.state().misure_del_pdf()[1].altezza), (842.0, 595.0));
        // Duplicata: la copia dopo di lei; poi la copia eliminata.
        tocca(&mut h, pdf_pagine::DUPLICA);
        aspetta_il_motore(&mut h);
        assert_eq!(h.state().misure_del_pdf().len(), 5);
        assert_eq!(h.state().pagine_scelte(), Some(vec![]));
        tocca_pagina(&mut h, 3);
        tocca(&mut h, pdf_pagine::ELIMINA);
        aspetta_il_motore(&mut h);
        assert_eq!(h.state().misure_del_pdf().len(), 4);
        h.get_by_label_contains("1 pagina eliminata");
        assert!(h.query_by_label_contains("Salva pulito").is_none(), "Salva riscrive già tutto il file");
        // L'ultima presa col dito e portata davanti alla prima.
        trascina_pagina(&mut h, 4, 1);
        aspetta_il_motore(&mut h);
        assert_eq!(h.state().pagine_scelte(), Some(vec![0]), "la pagina spostata resta scelta");
        // Una pagina bianca dopo la prima, grande come lei.
        tocca(&mut h, pdf_pagine::PAGINA_BIANCA);
        h.run_steps(4);
        tocca(&mut h, "Dopo la pagina 1");
        aspetta_il_motore(&mut h);
        let misure = h.state().misure_del_pdf();
        assert_eq!((misure.len(), misure[1]), (5, misure[0]));
        // Le prime due estratte in un PDF nuovo in Documenti/Tascabile.
        tocca_pagina(&mut h, 1);
        assert_eq!(h.state().pagine_scelte(), Some(vec![0, 1]));
        tocca(&mut h, pdf_pagine::ESTRAI);
        aspetta_il_lavoro(&mut h);
        let estratto = dir.join("radice/Documents/Tascabile/Lettera Zanzibar pagine 1-2.pdf");
        assert_eq!(ordine_delle_pagine(std::fs::read(&estratto).unwrap(), None), ["Pagina 4", ""]);
        h.get_by_label_contains("Salvato in Documenti/Tascabile: Lettera Zanzibar pagine 1-2.pdf");
        // Annulla nella barra della griglia: la pagina bianca se ne va.
        tocca(&mut h, pdf_lavoro::ANNULLA_MODIFICA);
        aspetta_il_motore(&mut h);
        assert_eq!(h.state().misure_del_pdf().len(), 4);
        // Indietro chiude la griglia; salvato, il PDF ha l'ordine nuovo.
        indietro(&mut h);
        assert_eq!(h.state().pagine_scelte(), None);
        tocca(&mut h, schermo_pdf::MENU);
        tocca(&mut h, pdf_lavoro::SALVA);
        aspetta_il_motore(&mut h);
        let salvato = std::fs::read(&percorso).unwrap();
        assert_eq!(ordine_delle_pagine(salvato.clone(), None), ["Pagina 4", "Pagina 1", "Pagina 2", "Pagina 3"]);
        assert_eq!(tascabile_pdf::Pdf::apri(salvato, None).unwrap().misura(2), Some(tascabile_pdf::Misura { larghezza: 842.0, altezza: 595.0 }));
        let d = diagnosi_della_prova();
        for riga in [
            "] pdf: organizza aperto, 4 pagine\n",
            "] pdf: organizza, prese col dito: 1 pagina\n",
            "] pdf: organizza, spostate: 1 pagina\n",
            "] pdf: estrai fatto, 1 file in ",
        ] {
            assert!(d.contains(riga), "manca «{riga}»:\n{d}");
        }
        assert!(!d.contains("Zanzibar") && !d.contains("prove-app"), "{d}");
    }

    /// Unisci (da Documenti/Tascabile e dalla scelta dei file), Dividi per intervalli, Comprimi
    /// su una copia, Proteggi con la password e Togli la protezione, Salva pulito: col motore vero.
    /// La password non va mai nella diagnosi.
    #[test]
    fn unire_dividere_comprimere_proteggere_e_salvare_pulito() {
        use tascabile_pdf::prova::{Pagina, pdf};
        let (piattaforma, dir) = piattaforma_di_prova("pdf-file");
        let percorso = dir.join("radice/Lettera Zanzibar.pdf");
        std::fs::write(&percorso, quattro_pagine()).unwrap();
        let tascabile = dir.join("radice/Documents/Tascabile");
        std::fs::create_dir_all(&tascabile).unwrap();
        let altro = |testo: &'static str| pdf(&[Pagina { larghezza: 300.0, altezza: 400.0, rotazione: 0, contenuto: testo }]);
        std::fs::write(tascabile.join("Allegato Zanzibar.pdf"), altro("BT /F1 24 Tf 20 300 Td (Allegato) Tj ET")).unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        h.state_mut().apri_file(&percorso, "Lettera Zanzibar.pdf");
        aspetta_il_lettore(&mut h);
        // Unisci in fondo, dalla cartella di Tascabile.
        dagli_strumenti(&mut h, pdf_pagine::UNISCI);
        h.run_steps(4);
        tocca(&mut h, "Allegato Zanzibar");
        aspetta_il_lavoro(&mut h);
        assert_eq!(h.state().misure_del_pdf().len(), 5);
        // Dalla scelta dei file, dopo la pagina che si legge (la prima).
        dagli_strumenti(&mut h, pdf_pagine::UNISCI);
        h.run_steps(4);
        tocca(&mut h, "Dopo la pagina 1");
        tocca(&mut h, pdf_pagine::DAL_TELEFONO);
        assert_eq!(h.state().piattaforma.foto_chieste().last(), Some(&tascabile_platform::FonteFoto::Pdf));
        h.state().piattaforma.simula_foto_firma(Some(&altro("BT /F1 24 Tf 20 300 Td (Scelto) Tj ET")));
        h.run_steps(2);
        aspetta_il_lavoro(&mut h);
        assert_eq!(h.state().misure_del_pdf().len(), 6);
        assert_eq!(rimasto_fra_le_foto(&dir.join("cache")), Vec::<String>::new(), "il PDF scelto è rimasto nella cache");
        // Dividi per intervalli: due file nuovi.
        dagli_strumenti(&mut h, pdf_pagine::DIVIDI);
        h.run_steps(4);
        scrivi(&mut h, "1-2, 3-");
        tocca(&mut h, pdf_pagine::DIVIDI);
        aspetta_il_lavoro(&mut h);
        assert_eq!(ordine_delle_pagine(std::fs::read(tascabile.join("Lettera Zanzibar pagine 1-2.pdf")).unwrap(), None), ["Pagina 1", "Scelto"]);
        assert_eq!(ordine_delle_pagine(std::fs::read(tascabile.join("Lettera Zanzibar pagine 3-6.pdf")).unwrap(), None).len(), 4);
        // Un intervallo che non va lo dice, e il foglio resta.
        dagli_strumenti(&mut h, pdf_pagine::DIVIDI);
        h.run_steps(4);
        scrivi(&mut h, "4-2");
        tocca(&mut h, pdf_pagine::DIVIDI);
        h.get_by_label_contains("Scrivi gli intervalli come 1-3, 4-8");
        indietro(&mut h);
        assert!(!h.state().fogli_pdf.dividi.aperto());
        // Proteggi con la password: messa, salvata, il file chiede la password.
        dagli_strumenti(&mut h, pdf_pagine::PROTEGGI);
        h.run_steps(4);
        h.get_by_label_contains("non si recupera");
        scrivi(&mut h, "Segreta Zanzibar");
        tocca(&mut h, pdf_pagine::PROTEGGI);
        aspetta_il_motore(&mut h);
        assert_eq!(h.state().pdf_cifrato(), Some(true));
        tocca(&mut h, schermo_pdf::MENU);
        tocca(&mut h, pdf_lavoro::SALVA);
        aspetta_il_motore(&mut h);
        let salvato = std::fs::read(&percorso).unwrap();
        assert!(tascabile_pdf::Pdf::apri(salvato.clone(), None).err().unwrap().chiede_password());
        assert_eq!(ordine_delle_pagine(salvato, Some("Segreta Zanzibar")).len(), 6);
        // Togli la protezione, al posto di Proteggi.
        dagli_strumenti(&mut h, "Togli la protezione");
        h.run_steps(2);
        tocca(&mut h, pdf_pagine::TOGLI_PASSWORD);
        aspetta_il_motore(&mut h);
        assert_eq!(h.state().pdf_cifrato(), Some(false));
        // Salva pulito dal menu: una revisione sola, senza password.
        tocca(&mut h, schermo_pdf::MENU);
        tocca(&mut h, pdf_pagine::SALVA_PULITO);
        h.get_by_label_contains("senza le versioni di prima");
        tocca(&mut h, pdf_pagine::SALVA_PULITO);
        aspetta_il_motore(&mut h);
        let pulito = std::fs::read(&percorso).unwrap();
        assert_eq!(String::from_utf8_lossy(&pulito).matches("%%EOF").count(), 1);
        assert_eq!(ordine_delle_pagine(pulito, None).len(), 6);
        h.get_by_label("Salvato pulito");
        // Comprimi una foto a 320 dpi: la copia è più piccola e il dialogo dice quanto.
        let foto = dir.join("radice/Foto Zanzibar.pdf");
        std::fs::write(&foto, tascabile_pdf::prova::pdf_con_foto()).unwrap();
        h.state_mut().apri_file(&foto, "Foto Zanzibar.pdf");
        finche(&mut h, "il PDF della foto", |h| h.state().misure_del_pdf().len() == 1);
        aspetta_il_lettore(&mut h);
        dagli_strumenti(&mut h, pdf_pagine::COMPRIMI);
        h.run_steps(4);
        tocca(&mut h, "Media · Come «Riduci dimensioni» di Acrobat");
        aspetta_il_lavoro(&mut h);
        h.get_by_label(pdf_pagine::PDF_COMPRESSO);
        h.get_by_label_contains("in meno. La copia è in Documenti/Tascabile: «Foto Zanzibar compresso.pdf»");
        let compresso = std::fs::read(tascabile.join("Foto Zanzibar compresso.pdf")).unwrap();
        assert!(compresso.len() < std::fs::read(&foto).unwrap().len());
        tocca(&mut h, "Ho capito");
        let d = diagnosi_della_prova();
        for riga in [
            "] pdf: unisci 1 pagina in fondo\n",
            "] pdf da unire: chiesto alla scelta dei file\n",
            "] pdf: dividi in 2 parti\n",
            "] pdf: proteggi con password\n",
            "] salva pulito pdf: riuscito",
            "] pdf: compresso da ",
        ] {
            assert!(d.contains(riga), "manca «{riga}»:\n{d}");
        }
        assert!(!d.contains("Zanzibar") && !d.contains("Segreta") && !d.contains("prove-app"), "{d}");
    }

    /// Il campo del modulo di prova `c` nel PDF del lettore.
    fn campo_del_lettore(h: &Telefono, c: tascabile_pdf::prova::CampoDiProva) -> tascabile_pdf::Campo {
        let p = h.state().pdf.as_ref().expect("nessun PDF nel lettore");
        p.motore.stato().campi.iter().find(|x| x.suggerimento.as_deref() == Some(c.suggerimento())).expect("campo che non c'è").clone()
    }

    /// Un modulo sul telefono dell'autore (372 dp): nella barra alta Annulla, Cerca e il menu, e il
    /// sottotitolo dice prima quanti campi restano, intero (0.8.3: con quattro icone restavano
    /// 116 dp e si leggeva «Non salvato · 2 da c…»). Ripeti è in cima al menu quando c'è da
    /// ripetere; a 412 dp resta nella barra.
    #[test]
    fn sul_telefono_stretto_ripeti_va_nel_menu_e_il_sottotitolo_si_legge() {
        use tascabile_pdf::prova::CampoDiProva as C;
        let (piattaforma, dir) = piattaforma_di_prova("pdf-stretto");
        let percorso = dir.join("radice/Consenso Zanzibar.pdf");
        std::fs::write(&percorso, tascabile_pdf::prova::modulo_da_compilare()).unwrap();
        let mut h = telefono_di(TascabileApp::new(piattaforma, Instant::now()), egui::vec2(372.0, 827.0), 3.0);
        h.state_mut().apri_file(&percorso, "Consenso Zanzibar.pdf");
        aspetta_il_lettore(&mut h);
        assert!(acceso(&h, "Compila"));
        h.get_by_label(C::NonAcconsente.suggerimento()).click();
        h.run_steps(2);
        aspetta_il_motore(&mut h);
        let sotto = "2 da compilare · non salvato";
        h.get_by_label(sotto);
        assert_eq!(tagliato(&h, sotto), Some(false), "il sottotitolo finisce coi puntini");
        assert_eq!(tagliato(&h, "Consenso Zanzibar"), Some(false), "il nome finisce coi puntini");
        assert!(!h.get_by_role_and_label(Role::Button, pdf_lavoro::ANNULLA_MODIFICA).accesskit_node().is_disabled());
        assert!(h.query_by_label(pdf_lavoro::RIPETI_MODIFICA).is_none(), "Ripeti è nella barra");
        // Niente da ripetere: nel menu Ripeti non c'è. E non compare col menu aperto, anche se lo
        // stato del motore cambia (le voci si sposterebbero sotto il dito).
        tocca(&mut h, schermo_pdf::MENU);
        h.run_steps(2);
        let salva = h.get_by_role_and_label(Role::Button, pdf_lavoro::SALVA).rect();
        assert!(h.query_by_label(pdf_lavoro::RIPETI_MODIFICA).is_none(), "Ripeti nel menu senza niente da ripetere");
        h.state_mut().annulla_pdf();
        aspetta_il_motore(&mut h);
        assert!(h.state().pdf.as_ref().unwrap().motore.stato().ripeti);
        assert!(h.query_by_label(pdf_lavoro::RIPETI_MODIFICA).is_none(), "Ripeti è comparso col menu aperto");
        assert_eq!(h.get_by_role_and_label(Role::Button, pdf_lavoro::SALVA).rect(), salva, "le voci si sono spostate");
        indietro(&mut h);
        // Rimessa la casella col menu chiuso, come prima.
        h.state_mut().ripeti_pdf();
        aspetta_il_motore(&mut h);
        assert!(campo_del_lettore(&h, C::NonAcconsente).acceso(0));
        assert!(!h.state().menu_pdf);
        // Annullata la casella, Ripeti è in cima al menu e la rimette.
        tocca(&mut h, pdf_lavoro::ANNULLA_MODIFICA);
        aspetta_il_motore(&mut h);
        assert!(!campo_del_lettore(&h, C::NonAcconsente).acceso(0));
        tocca(&mut h, schermo_pdf::MENU);
        h.run_steps(2);
        let ripeti = h.get_by_role_and_label(Role::Button, pdf_lavoro::RIPETI_MODIFICA).rect();
        assert!(ripeti.bottom() <= h.get_by_role_and_label(Role::Button, pdf_lavoro::SALVA).rect().top(), "Ripeti non è in cima: {ripeti:?}");
        tocca(&mut h, pdf_lavoro::RIPETI_MODIFICA);
        aspetta_il_motore(&mut h);
        assert!(!h.state().menu_pdf, "il menu è rimasto aperto");
        assert!(campo_del_lettore(&h, C::NonAcconsente).acceso(0), "Ripeti non ha rimesso la casella");
        assert!(diagnosi_della_prova().contains("] pdf: ripeti chiesto\n"));
        // Le altre voci del menu vanno ancora al loro posto: Informazioni apre il suo foglio.
        h.get_by_label(C::NonAcconsente.suggerimento()).click();
        h.run_steps(2);
        aspetta_il_motore(&mut h);
        tocca(&mut h, pdf_lavoro::ANNULLA_MODIFICA);
        aspetta_il_motore(&mut h);
        tocca(&mut h, schermo_pdf::MENU);
        h.run_steps(2);
        tocca(&mut h, schermo_pdf::INFORMAZIONI);
        h.run_steps(6);
        assert!(h.state().fogli_pdf.info.aperto(), "Informazioni non ha aperto il suo foglio");
    }

    /// Il modulo coi campi veri: si apre nel modo Compila coi campi da compilare nel sottotitolo;
    /// un campo di testo si scrive con la tastiera e il valore va al motore solo alla conferma
    /// (Invio, che passa al campo dopo, «Fatto»); casella e radio con un tocco; menu ed elenco in
    /// un foglio; la data nel suo foglio. Salvato e riaperto, ha tutti i valori.
    #[test]
    fn compilare_un_modulo_a_schermo_salvarlo_e_riaprirlo() {
        use tascabile_pdf::prova::CampoDiProva as C;
        let byte = tascabile_pdf::prova::modulo_da_compilare();
        let (piattaforma, dir) = piattaforma_di_prova("pdf-compila");
        let percorso = dir.join("radice/Consenso Zanzibar.pdf");
        std::fs::write(&percorso, &byte).unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        h.state_mut().apri_file(&percorso, "Consenso Zanzibar.pdf");
        aspetta_il_lettore(&mut h);
        assert!(acceso(&h, "Compila"), "un modulo si apre nel modo Compila");
        h.get_by_label("PDF · 3 pagine · 2 campi da compilare");
        // «residente in»: la tastiera vera, la barra con campo prima, campo dopo e Fatto.
        h.get_by_label(C::Residente.suggerimento()).click();
        h.run_steps(2);
        assert_eq!(h.state().fuoco, Fuoco::Campo);
        assert!(h.state().piattaforma.keyboard_visible());
        h.get_by_role_and_label(Role::Button, pdf_compila::CAMPO_DOPO);
        h.get_by_role_and_label(Role::Button, pdf_compila::FATTO);
        scrivi(&mut h, "Via dei Mille 12");
        assert_eq!(h.state().pdf_in_scrittura().as_deref(), Some("Via dei Mille 12"));
        assert_eq!(h.state().pdf_modificato(), Some(false), "il valore è andato al motore prima della conferma");
        // Invio: il valore va al motore e si passa al campo dopo (la data, con la tastiera).
        h.key_press(egui::Key::Enter);
        h.run_steps(2);
        aspetta_il_motore(&mut h);
        assert_eq!(campo_del_lettore(&h, C::Residente).testo(), "Via dei Mille 12");
        assert_eq!(h.state().pdf_in_scrittura().as_deref(), Some("24/10/2026"));
        h.get_by_role_and_label(Role::Button, pdf_compila::SCEGLI_LA_DATA);
        tocca(&mut h, pdf_compila::CAMPO_DOPO);
        scrivi(&mut h, "Roma");
        tocca(&mut h, pdf_compila::FATTO);
        aspetta_il_motore(&mut h);
        assert_eq!(campo_del_lettore(&h, C::Luogo).testo(), "Roma");
        assert!(h.state().pdf_in_scrittura().is_none() && !h.state().piattaforma.keyboard_visible());
        assert_eq!(h.state().fuoco, Fuoco::Documento);
        h.get_by_label("tutto compilato · non salvato");
        // La casella e il radio, con un tocco.
        h.get_by_label(C::NonAcconsente.suggerimento()).click();
        h.run_steps(2);
        h.get_by_label(&format!("{}: {}", C::Ricovero.suggerimento(), tascabile_pdf::prova::RICOVERI[1])).click();
        h.run_steps(2);
        aspetta_il_motore(&mut h);
        assert!(campo_del_lettore(&h, C::NonAcconsente).acceso(0));
        assert!(campo_del_lettore(&h, C::Ricovero).acceso(1));
        // Il menu nel suo foglio: una scelta e il foglio si chiude.
        h.get_by_label(C::Reparto.suggerimento()).click();
        h.run_steps(6);
        h.get_by_label("Cardiologia").click();
        h.run_steps(2);
        h.run_steps(4);
        assert!(h.query_by_label("Ortopedia").is_none(), "il foglio del menu è rimasto aperto");
        // L'elenco a scelta multipla: due tocchi e Fatto.
        h.get_by_label(C::Allergie.suggerimento()).click();
        h.run_steps(6);
        h.get_by_label("Lattice").click();
        h.run_steps(2);
        h.get_by_label("Farmaci").click();
        h.run_steps(2);
        tocca(&mut h, pdf_compila::FATTO);
        h.run_steps(4);
        // La data di nascita: il giorno dopo e «Metti la data».
        h.get_by_label(C::DataDiNascita.suggerimento()).click();
        h.run_steps(6);
        h.get_by_label("12/03/1958");
        tocca(&mut h, "Giorno dopo");
        h.get_by_label("13/03/1958");
        tocca(&mut h, pdf_compila::METTI_LA_DATA);
        h.run_steps(4);
        aspetta_il_motore(&mut h);
        assert_eq!(campo_del_lettore(&h, C::Reparto).mostrato(), "Cardiologia");
        assert_eq!(campo_del_lettore(&h, C::Allergie).valore, ["Nessuna", "Farmaci", "Lattice"]);
        assert_eq!(campo_del_lettore(&h, C::DataDiNascita).testo(), "13/03/1958");
        // Salva dal menu, poi riaperto: tutti i valori.
        tocca(&mut h, schermo_pdf::MENU);
        tocca(&mut h, pdf_lavoro::SALVA);
        aspetta_il_motore(&mut h);
        let salvati = std::fs::read(&percorso).unwrap();
        assert_eq!(salvati.windows(5).filter(|w| w == b"%%EOF").count(), 1, "il salvataggio ha lasciato le revisioni di prima");
        let campi = tascabile_pdf::Pdf::apri(salvati, None).unwrap().campi();
        let valore = |c: C| campi.iter().find(|x| x.suggerimento.as_deref() == Some(c.suggerimento())).unwrap().clone();
        assert_eq!(valore(C::Residente).testo(), "Via dei Mille 12");
        assert_eq!(valore(C::Luogo).testo(), "Roma");
        assert!(valore(C::NonAcconsente).acceso(0) && valore(C::Acconsente).acceso(0));
        assert!(valore(C::Ricovero).acceso(1));
        assert_eq!(valore(C::Reparto).valore, ["Cardiologia"]);
        assert_eq!(valore(C::DataDiNascita).testo(), "13/03/1958");
        assert_eq!(tascabile_pdf::da_compilare(&campi), 0);
        let d = diagnosi_della_prova();
        for riga in [
            "] pdf: modulo con 13 campi, 2 da compilare\n",
            "] pdf: scrittura di un campo di testo\n",
            "] pdf: campo confermato\n",
            "] pdf: casella toccata\n",
        ] {
            assert!(d.contains(riga), "manca «{riga}»:\n{d}");
        }
        for parola in ["Mille", "Roma", "Cardiologia", "Lattice", "Residente", "residenza", "1958", "Zanzibar"] {
            assert!(!d.contains(parola), "«{parola}» nella diagnosi:\n{d}");
        }
    }

    /// Indietro durante la scrittura conferma il valore; «Campo prima» torna indietro; un tocco
    /// fuori dai campi conferma e chiude.
    #[test]
    fn indietro_e_un_tocco_fuori_confermano_il_campo() {
        use tascabile_pdf::prova::CampoDiProva as C;
        let mut h = telefono_col_pdf("pdf-compila-indietro", tascabile_pdf::prova::modulo_da_compilare());
        h.get_by_label(C::Luogo.suggerimento()).click();
        h.run_steps(2);
        scrivi(&mut h, "Napoli");
        indietro(&mut h);
        aspetta_il_motore(&mut h);
        assert_eq!(campo_del_lettore(&h, C::Luogo).testo(), "Napoli");
        assert!(h.state().pdf_in_scrittura().is_none() && !h.state().piattaforma.keyboard_visible());
        assert_eq!(h.state().schermata, Schermata::Pdf);
        // Di nuovo nel luogo: «Campo prima» porta alla data della firma, col suo testo.
        h.get_by_label(C::Luogo.suggerimento()).click();
        h.run_steps(2);
        assert_eq!(h.state().pdf_in_scrittura().as_deref(), Some("Napoli"));
        tocca(&mut h, pdf_compila::CAMPO_PRIMA);
        assert_eq!(h.state().pdf_in_scrittura().as_deref(), Some("24/10/2026"));
        // Un tocco sulla pagina lontano dai campi: si chiude, e niente proposta di Fill & Sign.
        tocca_in(&mut h, egui::pos2(216.0, 78.0 + 470.0));
        h.run_steps(3);
        assert!(h.state().pdf_in_scrittura().is_none());
        assert!(h.state().pdf.as_ref().unwrap().compila.proposta.is_none());
        assert_eq!(h.state().pdf_modificato(), Some(true));
    }

    /// Un PDF senza campi (Fill & Sign): nel modo Compila un tocco su un punto vuoto propone Testo,
    /// Spunta, Croce e Data; il segno posato resta scelto, si sposta col dito e si toglie.
    #[test]
    fn fill_and_sign_su_un_pdf_senza_campi() {
        let mut h = telefono_col_pdf("pdf-fill-sign", tascabile_pdf::prova::modulo());
        assert!(acceso(&h, "Leggi"));
        tocca(&mut h, "Compila");
        assert!(acceso(&h, "Compila"));
        // Un punto vuoto sotto il riquadro della firma (px del bozzetto = dp a zoom 1).
        let dove = egui::pos2(16.0 + 200.0, 78.0 + 450.0);
        tocca_in(&mut h, dove);
        h.run_steps(3);
        for voce in [pdf_compila::TESTO, pdf_compila::SPUNTA, pdf_compila::CROCE, pdf_compila::DATA] {
            h.get_by_role_and_label(Role::Button, voce);
        }
        tocca(&mut h, pdf_compila::SPUNTA);
        aspetta_il_motore(&mut h);
        let segni = h.state().pdf.as_ref().unwrap().motore.stato().segni.clone();
        assert_eq!(segni.len(), 1);
        assert_eq!(h.state().pdf.as_ref().unwrap().compila.segno, Some((0, segni[0].indice)), "il segno posato non è scelto");
        // Spostato col dito di 40 dp a destra.
        let prima = segni[0].rettangolo;
        let partenza = h.get_by_label("Segno posato").rect().center();
        h.event(egui::Event::PointerMoved(partenza));
        h.event(egui::Event::PointerButton { pos: partenza, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
        h.run_steps(1);
        for i in 1..=8 {
            h.event(egui::Event::PointerMoved(partenza + egui::vec2(5.0 * i as f32, 0.0)));
            h.run_steps(1);
        }
        let arrivo = partenza + egui::vec2(40.0, 0.0);
        h.event(egui::Event::PointerButton { pos: arrivo, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
        h.run_steps(2);
        aspetta_il_motore(&mut h);
        let dopo = h.state().pdf.as_ref().unwrap().motore.stato().segni[0].rettangolo;
        let k = 380.0 / 595.0;
        assert!((dopo[0] - prima[0] - 40.0 / k).abs() < 3.0, "spostato di {} pt:\n{}", dopo[0] - prima[0], diagnosi_della_prova());
        // Il testo: proposto, scritto con la tastiera, posato.
        tocca_in(&mut h, egui::pos2(16.0 + 60.0, 78.0 + 420.0));
        h.run_steps(3);
        if h.query_by_label(pdf_compila::TESTO).is_none() {
            // Il primo tocco ha chiuso il segno scelto: il secondo propone.
            tocca_in(&mut h, egui::pos2(16.0 + 60.0, 78.0 + 420.0));
            h.run_steps(3);
        }
        tocca(&mut h, pdf_compila::TESTO);
        assert!(h.state().piattaforma.keyboard_visible());
        scrivi(&mut h, "Mario Bianchi");
        tocca(&mut h, pdf_compila::FATTO);
        aspetta_il_motore(&mut h);
        let segni = h.state().pdf.as_ref().unwrap().motore.stato().segni.clone();
        assert!(segni.iter().any(|s| s.testo == "Mario Bianchi"), "il testo non è stato posato");
        // Tolto dalla barra del segno scelto (il testo appena posato).
        tocca(&mut h, pdf_compila::TOGLI);
        aspetta_il_motore(&mut h);
        assert_eq!(h.state().pdf.as_ref().unwrap().motore.stato().segni.len(), 1);
        let d = diagnosi_della_prova();
        for riga in ["] pdf: fill & sign, spunta\n", "] pdf: segno spostato\n", "] pdf: fill & sign, testo posato\n", "] pdf: segno tolto\n"] {
            assert!(d.contains(riga), "manca «{riga}»:\n{d}");
        }
        assert!(!d.contains("Bianchi"), "{d}");
    }

    /// Le annotazioni del PDF aperto, come le dà il motore.
    #[test]
    fn riscrivere_con_un_simbolo_lo_dice_e_non_mette_punti_di_domanda() {
        // Analisi del 27/10, modifica I6: riscrivendo un testo libero con «SpO₂ ≥ 94%» il file
        // aveva «SpO? ? 94%» senza avviso, e un testo aggiunto dava l'errore generico del motore.
        let mut h = telefono_col_pdf("riscrivi-simbolo", tascabile_pdf::prova::pdf(&[tascabile_pdf::prova::Pagina::bianca(595.0, 842.0)]));
        // Un testo libero posato, poi riscritto col simbolo.
        h.state_mut().conferma_testo_annota(&pdf_compila::Bersaglio::TestoLibero { pagina: 0, punto: [100.0, 200.0] }, "Saturazione");
        aspetta_il_motore(&mut h);
        let libero = annotazioni_pdf(&h)
            .into_iter()
            .find(|a| matches!(a.genere, tascabile_pdf::GenereAnnotazione::TestoLibero { .. }))
            .expect("il testo libero");
        h.state_mut().conferma_testo_annota(&pdf_compila::Bersaglio::Annotazione { pagina: libero.pagina, indice: libero.indice }, "SpO₂ ≥ 94%");
        aspetta_il_motore(&mut h);
        let errore = h.state_mut().errore.take().expect("il dialogo che dice perché");
        assert!(errore.testo.contains("«₂»"), "{}", errore.testo);
        let dopo = annotazioni_pdf(&h).into_iter().find(|a| matches!(a.genere, tascabile_pdf::GenereAnnotazione::TestoLibero { .. })).unwrap();
        assert_eq!(dopo.testo, "Saturazione", "il testo coi «?» è stato scritto");
        // Un testo aggiunto, poi riscritto col simbolo: lo stesso messaggio, non quello del motore.
        tocca(&mut h, "Modifica");
        h.state_mut().conferma_testo_modifica(&pdf_compila::Bersaglio::TestoNuovo { pagina: 0, punto: [100.0, 400.0] }, "Saturazione");
        aspetta_il_motore(&mut h);
        let aggiunto = |h: &Telefono| oggetti_pdf(h, 0).into_iter().find(|o| matches!(o.genere, tascabile_pdf::GenereOggetto::TestoAggiunto { .. }));
        finche(&mut h, "il testo aggiunto", |h| aggiunto(h).is_some());
        let aggiunto = aggiunto(&h).unwrap();
        h.state_mut().conferma_testo_modifica(&pdf_compila::Bersaglio::Oggetto(aggiunto.chiave()), "SpO₂ ≥ 94%");
        aspetta_il_motore(&mut h);
        let errore = h.state_mut().errore.take().expect("il dialogo che dice perché");
        assert!(errore.testo.contains("«₂»"), "{}", errore.testo);
    }

    fn annotazioni_pdf(h: &Telefono) -> Vec<tascabile_pdf::Annotazione> {
        h.state().pdf.as_ref().unwrap().motore.stato().annotazioni.as_ref().clone()
    }

    fn di_genere(h: &Telefono, g: tascabile_pdf::GenereAnnotazione) -> Vec<tascabile_pdf::Annotazione> {
        annotazioni_pdf(h).into_iter().filter(|a| a.genere == g).collect()
    }

    /// Il centro sullo schermo del riquadro di un'annotazione.
    fn centro_dell_annotazione(h: &Telefono, a: &tascabile_pdf::Annotazione) -> egui::Pos2 {
        h.state().pdf.as_ref().unwrap().lettore.sullo_schermo(a.pagina, a.rettangolo).expect("l'annotazione sullo schermo").center()
    }

    /// Una lettera di due pagine per annotarla.
    fn lettera_da_annotare(dir: &Path) -> PathBuf {
        let contenuto = "BT /F1 18 Tf 60 700 Td (Relazione di dimissione) Tj ET\nBT /F1 12 Tf 60 600 Td (Terapia da proseguire) Tj ET";
        let pagina = tascabile_pdf::prova::Pagina { larghezza: 595.0, altezza: 842.0, rotazione: 0, contenuto };
        let byte = tascabile_pdf::prova::pdf(&[pagina, tascabile_pdf::prova::Pagina::bianca(595.0, 842.0)]);
        let percorso = dir.join("radice/Lettera Zanzibar.pdf");
        std::fs::write(&percorso, byte).unwrap();
        percorso
    }

    /// Il modo Annota col motore vero: la parola scelta con la pressione lunga (il tasto destro
    /// sul PC) si evidenzia col colore toccato nella barra; la penna disegna col dito senza
    /// scorrere la pagina, la gomma toglie il tratto; una nota col testo scritto, riletta nel suo
    /// foglio e riscritta; un testo libero, scelto e tolto, Annulla lo rimette. L'autore viene
    /// dalle Impostazioni. Salvato e riaperto, il PDF ha tutto; la copia definitiva (il foglio
    /// spiega prima che cosa fa) in Documenti/Tascabile non ha più annotazioni, e il documento
    /// resta com'era. Nella diagnosi nessun testo né l'autore.
    #[test]
    fn annotare_un_pdf_e_farne_la_copia_definitiva() {
        use tascabile_pdf::{GenereAnnotazione, Marca};
        let (piattaforma, dir) = piattaforma_di_prova("pdf-annota");
        std::fs::create_dir_all(dir.join("app")).unwrap();
        std::fs::write(dir.join("app/autore.txt"), "Mario Zanzibar\n").unwrap();
        let percorso = lettera_da_annotare(&dir);
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        h.state_mut().apri_file(&percorso, "Lettera Zanzibar.pdf");
        aspetta_il_lettore(&mut h);
        tocca(&mut h, "Annota");
        assert!(acceso(&h, "Annota") && acceso(&h, pdf_annota::SCORRI));
        // La parola «Terapia» scelta col tasto destro (la pressione lunga del telefono).
        let k = 380.0 / 595.0;
        let parola = egui::pos2(16.0 + 80.0 * k, 78.0 + 238.0 * k);
        h.event(egui::Event::PointerMoved(parola));
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton { pos: parola, button: egui::PointerButton::Secondary, pressed, modifiers: egui::Modifiers::NONE });
        }
        finche(&mut h, "la parola scelta con la sua barra", |h| {
            h.state().pdf.as_ref().unwrap().lettore.ha_selezione() && h.query_by_label(pdf_annota::EVIDENZIA).is_some()
        });
        for voce in [pdf_annota::EVIDENZIA, pdf_annota::SOTTOLINEA, pdf_annota::BARRA] {
            h.get_by_role_and_label(Role::Button, voce);
        }
        tocca(&mut h, "Azzurro");
        assert_eq!(h.state().pdf.as_ref().unwrap().annota.colore_marca, 2);
        assert!(h.state().pdf.as_ref().unwrap().lettore.ha_selezione(), "il colore ha chiuso la selezione");
        tocca(&mut h, pdf_annota::EVIDENZIA);
        finche(&mut h, "l'evidenziatura", |h| !di_genere(h, GenereAnnotazione::Marca(Marca::Evidenzia)).is_empty());
        let ev = di_genere(&h, GenereAnnotazione::Marca(Marca::Evidenzia)).remove(0);
        assert_eq!(ev.autore, "Mario Zanzibar");
        assert!((ev.colore.unwrap()[2] - pdf_annota::COLORI_MARCA[2].0[2]).abs() < 0.02, "{:?}", ev.colore);
        // La penna: il dito disegna e la pagina resta ferma.
        tocca(&mut h, pdf_annota::PENNA);
        assert!(acceso(&h, pdf_annota::PENNA));
        tocca(&mut h, "Rosso");
        let ferma = h.state().pdf.as_ref().unwrap().lettore.scorrimento();
        trascina_col_dito(&mut h, egui::pos2(100.0, 450.0), egui::vec2(160.0, 60.0));
        finche(&mut h, "il tratto della penna", |h| di_genere(h, GenereAnnotazione::Penna).len() == 1);
        assert_eq!(h.state().pdf.as_ref().unwrap().lettore.scorrimento(), ferma, "la penna ha scorso la pagina");
        let t = di_genere(&h, GenereAnnotazione::Penna).remove(0);
        assert!((t.colore.unwrap()[0] - pdf_annota::COLORI[2].0[0]).abs() < 0.02 && t.autore == "Mario Zanzibar");
        // La gomma lungo tutto il tratto lo toglie.
        tocca(&mut h, pdf_annota::GOMMA);
        trascina_col_dito(&mut h, egui::pos2(100.0, 450.0), egui::vec2(160.0, 60.0));
        finche(&mut h, "il tratto tolto", |h| di_genere(h, GenereAnnotazione::Penna).is_empty());
        // Una nota: il tocco dopo Nota apre la tastiera; il testo va al motore con Fatto.
        tocca(&mut h, pdf_annota::NOTA);
        let qui = egui::pos2(16.0 + 450.0 * k, 78.0 + 150.0 * k);
        tocca_in(&mut h, qui);
        h.run_steps(3);
        assert!(h.state().piattaforma.keyboard_visible(), "{}", diagnosi_della_prova());
        scrivi(&mut h, "Chiamare il medico");
        tocca(&mut h, pdf_compila::FATTO);
        finche(&mut h, "la nota", |h| di_genere(h, GenereAnnotazione::Nota).iter().any(|a| a.testo == "Chiamare il medico"));
        assert!(acceso(&h, pdf_annota::SCORRI), "dopo la nota si torna a scorrere");
        // Toccata, la nota apre il suo foglio; Riscrivi.
        let n = di_genere(&h, GenereAnnotazione::Nota).remove(0);
        let c = centro_dell_annotazione(&h, &n);
        assert!((c - qui).length() < 4.0, "{c:?} {qui:?}");
        tocca_in(&mut h, c);
        h.run_steps(8);
        assert!(h.state().fogli_pdf.nota.aperto());
        h.get_by_label("Chiamare il medico");
        h.get_by_label("Scritta da Mario Zanzibar");
        tocca(&mut h, pdf_compila::RISCRIVI);
        h.run_steps(3);
        scrivi(&mut h, " di turno");
        tocca(&mut h, pdf_compila::FATTO);
        finche(&mut h, "la nota riscritta", |h| di_genere(h, GenereAnnotazione::Nota).iter().any(|a| a.testo == "Chiamare il medico di turno"));
        // Un testo libero, scelto col tocco e tolto dalla sua barra; Annulla lo rimette.
        tocca(&mut h, pdf_annota::TESTO);
        h.run_steps(2);
        tocca(&mut h, pdf_annota::PIU_GRANDE);
        tocca_in(&mut h, egui::pos2(16.0 + 60.0 * k, 78.0 + 400.0 * k));
        h.run_steps(3);
        scrivi(&mut h, "Visto il 25 ottobre");
        tocca(&mut h, pdf_compila::FATTO);
        let libero = |h: &Telefono| annotazioni_pdf(h).into_iter().find(|a| matches!(a.genere, GenereAnnotazione::TestoLibero { .. }));
        finche(&mut h, "il testo libero", |h| libero(h).is_some());
        assert!(matches!(libero(&h).unwrap().genere, GenereAnnotazione::TestoLibero { misura } if (misura - 16.0).abs() < 0.01));
        let c = centro_dell_annotazione(&h, &libero(&h).unwrap());
        tocca_in(&mut h, c);
        h.run_steps(4);
        assert!(h.state().pdf.as_ref().unwrap().annota.scelta.is_some());
        tocca(&mut h, pdf_compila::TOGLI);
        finche(&mut h, "il testo libero tolto", |h| libero(h).is_none());
        tocca(&mut h, pdf_lavoro::ANNULLA_MODIFICA);
        finche(&mut h, "il testo libero rimesso", |h| libero(h).is_some());
        // La copia definitiva dal menu: prima il foglio che spiega, poi un PDF nuovo senza
        // annotazioni; il documento resta com'era.
        tocca(&mut h, schermo_pdf::MENU);
        tocca(&mut h, schermo_pdf::COPIA_DEFINITIVA);
        h.run_steps(12);
        h.get_by_label(schermo_pdf::SPIEGA_DEFINITIVA[0]);
        let definitiva = dir.join("radice/Documents/Tascabile/Lettera Zanzibar definitivo.pdf");
        assert!(!definitiva.exists(), "la copia prima del sì");
        tocca(&mut h, schermo_pdf::CREA_DEFINITIVA);
        aspetta_il_motore(&mut h);
        assert!(definitiva.is_file(), "{}", diagnosi_della_prova());
        assert!(tascabile_pdf::Pdf::apri(std::fs::read(&definitiva).unwrap(), None).unwrap().annotazioni().is_empty());
        h.get_by_label_contains("Copia definitiva in Documenti/Tascabile: Lettera Zanzibar definitivo.pdf");
        assert_eq!(h.state().pdf_modificato(), Some(true));
        assert_eq!(annotazioni_pdf(&h).len(), 3);
        // Salva e riapri: le tre annotazioni con l'autore.
        tocca(&mut h, schermo_pdf::MENU);
        tocca(&mut h, pdf_lavoro::SALVA);
        aspetta_il_motore(&mut h);
        let riletto = tascabile_pdf::Pdf::apri(std::fs::read(&percorso).unwrap(), None).unwrap().annotazioni();
        assert_eq!(riletto.len(), 3);
        assert!(riletto.iter().all(|a| a.autore == "Mario Zanzibar"));
        let d = diagnosi_della_prova();
        for riga in [
            "] pdf: modo annota\n",
            "] pdf: testo evidenziato in 1 righe, colore 3\n",
            "] pdf: annota, penna\n",
            "] pdf: gomma nella pagina 1\n",
            "] pdf: nota posata\n",
            "] pdf: nota aperta\n",
            "] pdf: testo riscritto (nota)\n",
            "] pdf: testo libero posato, 16 pt, colore 1\n",
            "] pdf: annotazione tolta (testo libero)\n",
            "] pdf: copia definitiva spiegata\n",
            "] copia definitiva pdf: chiesto\n",
        ] {
            assert!(d.contains(riga), "manca «{riga}»:\n{d}");
        }
        assert!(!d.contains("Zanzibar") && !d.contains("medico") && !d.contains("ottobre"), "{d}");
    }

    /// Un indirizzo cliccabile su una riga: il primo tocco sceglie il link (Cambia, Togli), il
    /// secondo apre la scrittura della riga sotto. Prima ogni tocco sceglieva di nuovo il link e
    /// la riga non si riscriveva mai (analisi del 27/10, modifica I7).
    #[test]
    fn il_testo_sotto_un_link_si_riscrive_al_secondo_tocco() {
        use tascabile_pdf::prova::{Bersaglio, Collegamento, Pagina, pdf_completo};
        let contenuto = "BT /F1 14 Tf 60 700 Td (Scrivere a reparto@esempio.it) Tj ET";
        let link =
            [Collegamento { pagina: 0, rettangolo: [140.0, 695.0, 290.0, 715.0], bersaglio: Bersaglio::Indirizzo("mailto:reparto@esempio.it") }];
        let byte = pdf_completo(&[Pagina { larghezza: 595.0, altezza: 842.0, rotazione: 0, contenuto }], &[], &link);
        let mut h = telefono_col_pdf("pdf-link-testo", byte);
        tocca(&mut h, "Modifica");
        finche(&mut h, "la riga", |h| oggetti_pdf(h, 0).iter().any(|o| o.testo.starts_with("Scrivere")));
        let riga = oggetti_pdf(&h, 0).into_iter().find(|o| o.testo.starts_with("Scrivere")).unwrap();
        let sul_link = h
            .state()
            .pdf
            .as_ref()
            .unwrap()
            .lettore
            .sullo_schermo(0, h.state().pdf.as_ref().unwrap().motore.stato().collegamenti[0].rettangolo)
            .unwrap()
            .center();
        assert!(centro_dell_oggetto(&h, &riga).y - sul_link.y < 4.0, "il link non è sulla riga");
        tocca_in(&mut h, sul_link);
        h.run_steps(4);
        h.get_by_label(pdf_modifica::CAMBIA);
        assert!(!h.state().piattaforma.keyboard_visible());
        tocca_in(&mut h, sul_link);
        h.run_steps(4);
        assert!(h.state().piattaforma.keyboard_visible(), "il secondo tocco non apre la scrittura:\n{}", diagnosi_della_prova());
        assert!(h.query_by_label(pdf_modifica::CAMBIA).is_none());
        scrivi(&mut h, " o al 06");
        tocca(&mut h, pdf_compila::FATTO);
        finche(&mut h, "la riga riscritta", |h| oggetti_pdf(h, 0).iter().any(|o| o.testo.ends_with("o al 06")));
        assert_eq!(h.state().pdf.as_ref().unwrap().motore.stato().collegamenti.len(), 1, "il link resta");
    }

    /// Le forme e i link del modo Modifica: un rettangolo e una freccia col dito dal foglio
    /// Strumenti, il rettangolo scelto col tocco e ricolorato dal suo Formato; un link disegnato
    /// che porta alla pagina 2, cambiato in un indirizzo scritto con la tastiera e tolto.
    #[test]
    fn forme_e_link_nel_modo_modifica() {
        use tascabile_pdf::{Destinazione, Forma, GenereAnnotazione};
        let (piattaforma, dir) = piattaforma_di_prova("pdf-forme");
        let percorso = lettera_da_annotare(&dir);
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        h.state_mut().apri_file(&percorso, "Lettera Zanzibar.pdf");
        aspetta_il_lettore(&mut h);
        dagli_strumenti(&mut h, pdf_lavoro::Strumento::Forme.nome());
        h.run_steps(8);
        assert!(acceso(&h, "Modifica") && acceso(&h, pdf_modifica::RETTANGOLO));
        trascina_col_dito(&mut h, egui::pos2(80.0, 330.0), egui::vec2(140.0, 60.0));
        finche(&mut h, "il rettangolo", |h| di_genere(h, GenereAnnotazione::Forma(Forma::Rettangolo)).len() == 1);
        tocca(&mut h, pdf_modifica::FRECCIA);
        trascina_col_dito(&mut h, egui::pos2(80.0, 450.0), egui::vec2(200.0, 40.0));
        finche(&mut h, "la freccia", |h| di_genere(h, GenereAnnotazione::Forma(Forma::Freccia)).len() == 1);
        tocca(&mut h, pdf_modifica::FINE_FORME);
        assert!(h.state().pdf.as_ref().unwrap().modifica.forme.is_none());
        // Il rettangolo toccato si sceglie; Formato, Blu.
        let r = di_genere(&h, GenereAnnotazione::Forma(Forma::Rettangolo)).remove(0);
        let c = centro_dell_annotazione(&h, &r);
        tocca_in(&mut h, c);
        h.run_steps(4);
        assert_eq!(h.state().pdf.as_ref().unwrap().annota.scelta, Some(r.chiave()));
        tocca(&mut h, pdf_annota::FORMATO);
        h.run_steps(10);
        tocca(&mut h, "Blu");
        finche(&mut h, "il rettangolo blu", |h| {
            di_genere(h, GenereAnnotazione::Forma(Forma::Rettangolo))
                .iter()
                .any(|a| a.colore.is_some_and(|c| (c[2] - pdf_annota::COLORI[1].0[2]).abs() < 0.02))
        });
        indietro(&mut h);
        indietro(&mut h);
        // Un link sul titolo che porta alla pagina 2.
        dagli_strumenti(&mut h, pdf_lavoro::Strumento::Link.nome());
        h.run_steps(8);
        let k = 380.0 / 595.0;
        trascina_col_dito(&mut h, egui::pos2(16.0 + 55.0 * k, 78.0 + 125.0 * k), egui::vec2(230.0 * k, 25.0 * k));
        h.run_steps(10);
        assert!(h.state().fogli_pdf.link.aperto(), "{}", diagnosi_della_prova());
        tocca(&mut h, "Pagina dopo");
        h.get_by_label("Pagina 2 di 2");
        tocca(&mut h, pdf_modifica::METTI_LINK);
        let link = |h: &Telefono| h.state().pdf.as_ref().unwrap().motore.stato().collegamenti.clone();
        finche(&mut h, "il link", |h| link(h).iter().any(|c| matches!(c.destinazione, Destinazione::Pagina { pagina: 1, .. })));
        // Toccato si sceglie (non si segue): Cambia, un indirizzo.
        let l = link(&h).remove(0);
        let c = h.state().pdf.as_ref().unwrap().lettore.sullo_schermo(0, l.rettangolo).unwrap().center();
        tocca_in(&mut h, c);
        h.run_steps(4);
        assert_eq!(pagina_corrente(&h), 0);
        tocca(&mut h, pdf_modifica::CAMBIA);
        h.run_steps(10);
        tocca(&mut h, pdf_modifica::UN_INDIRIZZO);
        h.run_steps(10);
        tocca(&mut h, pdf_modifica::SCRIVI_INDIRIZZO);
        h.run_steps(3);
        assert!(h.state().piattaforma.keyboard_visible(), "{}", diagnosi_della_prova());
        scrivi(&mut h, "www.esempio.it");
        tocca(&mut h, pdf_compila::FATTO);
        h.run_steps(10);
        tocca(&mut h, pdf_modifica::CAMBIA_LINK);
        finche(&mut h, "il link a un indirizzo", |h| {
            link(h).iter().any(|c| c.destinazione == Destinazione::Indirizzo("https://www.esempio.it".into()))
        });
        // Resta scelto, con la sua barra (un altro tocco andrebbe al titolo sotto): Togli.
        h.run_steps(4);
        tocca(&mut h, pdf_compila::TOGLI);
        finche(&mut h, "il link tolto", |h| link(h).is_empty());
        let d = diagnosi_della_prova();
        for riga in [
            "] pdf: forma disegnata (rettangolo)\n",
            "] pdf: forma disegnata (freccia)\n",
            "] pdf: link messo a una pagina\n",
            "] pdf: link cambiato a un indirizzo\n",
            "] pdf: link tolto\n",
        ] {
            assert!(d.contains(riga), "manca «{riga}»:\n{d}");
        }
        assert!(!d.contains("esempio"), "{d}");
    }

    /// L'autore dei commenti nelle Impostazioni: vuoto all'inizio, scritto con la tastiera e
    /// «Fatto», ritrovato all'avvio dopo, tolto. Nella diagnosi mai il nome.
    #[test]
    fn l_autore_dei_commenti_si_sceglie_nelle_impostazioni() {
        let (piattaforma, dir) = piattaforma_di_prova("autore-commenti");
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        h.run_steps(2);
        assert_eq!(h.state().impostazioni.autore, "");
        dalle_impostazioni(&mut h, impostazioni::AUTORE);
        h.run_steps(10);
        assert!(h.state().piattaforma.keyboard_visible());
        scrivi(&mut h, "  Anna Zanzibar ");
        tocca(&mut h, impostazioni::FATTO);
        h.run_steps(4);
        assert_eq!(h.state().impostazioni.autore, "Anna Zanzibar");
        assert_eq!(std::fs::read_to_string(dir.join("app/autore.txt")).unwrap(), "Anna Zanzibar");
        assert!(!h.state().impostazioni.foglio_autore.aperto() && !h.state().piattaforma.keyboard_visible());
        let piattaforma = Piattaforma::con_cartelle(dir.join("app"), dir.join("cache"), dir.join("radice"));
        let mut altra = telefono(TascabileApp::new(piattaforma, Instant::now()));
        altra.run_steps(2);
        assert_eq!(altra.state().impostazioni.autore, "Anna Zanzibar");
        dalle_impostazioni(&mut altra, impostazioni::AUTORE);
        altra.run_steps(10);
        tocca(&mut altra, impostazioni::TOGLI_NOME);
        altra.run_steps(4);
        assert_eq!(altra.state().impostazioni.autore, "");
        assert!(!dir.join("app/autore.txt").exists());
        let d = diagnosi_della_prova();
        assert!(d.contains("] impostazioni: autore scritto\n") && d.contains("] impostazioni: autore tolto\n"), "{d}");
        assert!(!d.contains("Zanzibar"), "{d}");
    }

    /// Il dito appoggiato in `da`, mosso di `delta` in otto passi e alzato.
    fn trascina_col_dito(h: &mut Telefono, da: egui::Pos2, delta: egui::Vec2) {
        h.event(egui::Event::PointerMoved(da));
        h.event(egui::Event::PointerButton { pos: da, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
        h.run_steps(1);
        for i in 1..=8 {
            h.event(egui::Event::PointerMoved(da + delta * (i as f32 / 8.0)));
            h.run_steps(1);
        }
        let a = da + delta;
        h.event(egui::Event::PointerButton { pos: a, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
        h.run_steps(2);
    }

    /// Gli oggetti della pagina `pagina` nel modo Modifica, come li ha dati il motore.
    fn oggetti_pdf(h: &Telefono, pagina: usize) -> Vec<tascabile_pdf::Oggetto> {
        h.state().pdf.as_ref().unwrap().modifica.oggetti.get(&pagina).map(|v| v.as_ref().clone()).unwrap_or_default()
    }

    /// Passi e attese finché `fatto` non è vero, al massimo 20 s.
    fn finche(h: &mut Telefono, cosa: &str, fatto: impl Fn(&Telefono) -> bool) {
        let fine = Instant::now() + Duration::from_secs(20);
        while !fatto(h) {
            assert!(Instant::now() < fine, "{cosa}: non è successo in 20 s\n{}", diagnosi_della_prova());
            h.state_mut().aspetta_il_motore_pdf();
            std::thread::sleep(Duration::from_millis(5));
            h.step();
        }
        h.run_steps(2);
    }

    /// Il centro sullo schermo di un oggetto della pagina.
    fn centro_dell_oggetto(h: &Telefono, o: &tascabile_pdf::Oggetto) -> egui::Pos2 {
        h.state().pdf.as_ref().unwrap().lettore.sullo_schermo(o.pagina, o.rettangolo).expect("l'oggetto sullo schermo").center()
    }

    /// Un'immagine chiesta alla galleria per il PDF, con una copia lenta (una foto nel cloud), e
    /// intanto la scansione chiede la fotocamera: la foto della scansione va alla scansione, non
    /// nella pagina del PDF; l'immagine arrivata tardi non entra nel PDF e non resta nella cache;
    /// l'attesa dell'immagine si spegne (revisione del codice della 0.8.3). Poi, con la X della
    /// scansione mentre aspetta, la foto che arriva non la aspetta nessuno e se ne va.
    #[test]
    fn una_foto_superata_non_finisce_nel_pdf_e_la_scansione_ha_la_sua() {
        let contenuto = "BT /F1 24 Tf 60 700 Td (Pagina di prova) Tj ET";
        let byte = tascabile_pdf::prova::pdf(&[tascabile_pdf::prova::Pagina { larghezza: 595.0, altezza: 842.0, rotazione: 0, contenuto }]);
        let (piattaforma, dir) = piattaforma_di_prova("foto-superata");
        let percorso = dir.join("radice/Lettera Zanzibar.pdf");
        std::fs::write(&percorso, &byte).unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        h.state_mut().apri_file(&percorso, "Lettera Zanzibar.pdf");
        aspetta_il_lettore(&mut h);
        tocca(&mut h, "Modifica");
        finche(&mut h, "gli oggetti della pagina", |h| oggetti_pdf(h, 0).len() == 1);
        dagli_strumenti(&mut h, pdf_modifica::IMMAGINE);
        h.run_steps(12);
        let k = 380.0 / 595.0;
        tocca_in(&mut h, egui::pos2(16.0 + 300.0 * k, 78.0 + 600.0 * k));
        h.run_steps(3);
        tocca(&mut h, pdf_modifica::DALLA_GALLERIA);
        let immagine = h.state().piattaforma.richiesta_foto_in_corso();
        assert!(h.state().pdf.as_ref().unwrap().modifica.foto.is_some(), "l'immagine non aspetta la sua foto");
        // Mentre la copia va avanti: Strumenti, Scansiona, Fotocamera.
        dagli_strumenti(&mut h, "Scansiona");
        h.run_steps(4);
        tocca(&mut h, pdf_ocr::FOTOCAMERA);
        let scansione = h.state().piattaforma.richiesta_foto_in_corso();
        assert_ne!(scansione, immagine);
        // L'immagine arriva tardi, poi la foto della scansione.
        h.state().piattaforma.simula_foto_di(immagine, Some(&tascabile_pdf::prova::foto_di_una_firma(400, 200, 80)));
        h.state().simula_foto_firma_di_prova(Some(&tascabile_pdf::prova::foto_di_un_foglio()));
        finche(&mut h, "la foto della scansione nei bordi", |h| h.state().bordi_aperti());
        h.run_steps(10);
        assert!(h.state().pdf.as_ref().unwrap().modifica.foto.is_none(), "l'immagine aspetta ancora");
        assert!(
            !oggetti_pdf(&h, 0).iter().any(|o| o.genere == tascabile_pdf::GenereOggetto::ImmagineAggiunta),
            "una foto è finita nella pagina del PDF"
        );
        assert!(!tascabile_platform::cartella_delle_scelte(&dir.join("cache"), immagine).exists(), "la foto superata è rimasta nella cache");
        let d = diagnosi_della_prova();
        assert!(
            d.contains("] pdf: immagine, richiesta superata da un'altra\n") && d.contains("] foto: esito di una richiesta superata, lasciato\n"),
            "{d}"
        );
        // La foto della scansione diventa una pagina.
        tocca(&mut h, pdf_ocr::FATTO);
        finche(&mut h, "la pagina scansionata", |h| h.state().pagine_scansionate() == Some(1) && !h.state().lavoro_in_corso());
        // La X mentre si aspetta la fotocamera: la foto arriva, non la aspetta nessuno, se ne va.
        tocca(&mut h, pdf_ocr::FOTOCAMERA);
        indietro(&mut h);
        assert!(!h.state().scansione_attende());
        h.state().simula_foto_firma_di_prova(Some(&tascabile_pdf::prova::foto_di_un_foglio()));
        h.run_steps(4);
        assert_eq!(rimasto_fra_le_foto(&dir.join("cache")), Vec::<String>::new());
        assert!(!h.state().bordi_aperti() && h.state().pagine_scansionate() == Some(1));
        assert!(diagnosi_della_prova().contains("] foto: arrivata senza attesa, cancellata\n"));
    }

    /// Il modo Modifica col motore vero: un paragrafo riscritto con la tastiera (e l'avviso del
    /// carattere con Continua), un testo nuovo con Aggiungi testo spostato col dito, un'immagine
    /// dalla galleria inserita e spostata, l'oscuramento col dito e la conferma, la filigrana e i
    /// numeri di pagina dai loro fogli; salvato e riaperto, il PDF ha tutto e il testo oscurato
    /// non c'è più nei byte. Nella diagnosi nessun testo.
    #[test]
    fn modificare_testi_immagini_oscurare_e_numerare() {
        let contenuto = "BT /F1 24 Tf 60 700 Td (Pagina di prova) Tj ET\nBT /F1 12 Tf 60 450 Td (Codice segreto 98765) Tj ET";
        let pagina = tascabile_pdf::prova::Pagina { larghezza: 595.0, altezza: 842.0, rotazione: 0, contenuto };
        let seconda =
            tascabile_pdf::prova::Pagina { larghezza: 595.0, altezza: 842.0, rotazione: 0, contenuto: "BT /F1 12 Tf 60 700 Td (Seconda) Tj ET" };
        let byte = tascabile_pdf::prova::pdf(&[pagina, seconda]);
        let (piattaforma, dir) = piattaforma_di_prova("pdf-modifica");
        let percorso = dir.join("radice/Lettera Zanzibar.pdf");
        std::fs::write(&percorso, &byte).unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        h.state_mut().apri_file(&percorso, "Lettera Zanzibar.pdf");
        aspetta_il_lettore(&mut h);
        tocca(&mut h, "Modifica");
        assert!(acceso(&h, "Modifica"));
        finche(&mut h, "gli oggetti della prima pagina", |h| oggetti_pdf(h, 0).len() == 2);
        // Un tocco sul paragrafo: la scrittura con la tastiera vera, il testo va al motore con Fatto.
        let o = oggetti_pdf(&h, 0).into_iter().find(|o| o.testo == "Pagina di prova").unwrap();
        let c = centro_dell_oggetto(&h, &o);
        tocca_in(&mut h, c);
        h.run_steps(3);
        assert!(h.state().piattaforma.keyboard_visible(), "il tocco sul paragrafo non apre la tastiera");
        scrivi(&mut h, " bis");
        tocca(&mut h, pdf_compila::FATTO);
        finche(&mut h, "il paragrafo riscritto", |h| oggetti_pdf(h, 0).iter().any(|o| o.testo == "Pagina di prova bis"));
        // Una lettera che il carattere non ha: l'avviso, e Continua.
        let o = oggetti_pdf(&h, 0).into_iter().find(|o| o.testo == "Pagina di prova bis").unwrap();
        let c = centro_dell_oggetto(&h, &o);
        tocca_in(&mut h, c);
        h.run_steps(3);
        scrivi(&mut h, " ≥");
        tocca(&mut h, pdf_compila::FATTO);
        finche(&mut h, "l'avviso del carattere", |h| h.query_by_label(pdf_modifica::CAMBIA_CARATTERE).is_some());
        tocca(&mut h, pdf_modifica::CONTINUA);
        finche(&mut h, "il paragrafo col simbolo", |h| oggetti_pdf(h, 0).iter().any(|o| o.testo.ends_with('≥')));
        // Aggiungi testo dal foglio Strumenti, dove si tocca; resta scelto e si sposta col dito.
        dagli_strumenti(&mut h, pdf_modifica::AGGIUNGI_TESTO);
        h.run_steps(12);
        let k = 380.0 / 595.0;
        tocca_in(&mut h, egui::pos2(16.0 + 300.0 * k, 78.0 + 300.0 * k));
        h.run_steps(3);
        assert!(h.state().piattaforma.keyboard_visible(), "{}", diagnosi_della_prova());
        scrivi(&mut h, "Visto, si approva");
        tocca(&mut h, pdf_compila::FATTO);
        let aggiunto = |h: &Telefono| oggetti_pdf(h, 0).into_iter().find(|o| o.testo == "Visto, si approva");
        finche(&mut h, "il testo aggiunto scelto", |h| {
            aggiunto(h).is_some_and(|o| h.state().pdf.as_ref().unwrap().modifica.scelto == Some(o.chiave()))
        });
        let prima = aggiunto(&h).unwrap();
        let c = centro_dell_oggetto(&h, &prima);
        trascina_col_dito(&mut h, c, egui::vec2(0.0, 40.0));
        finche(&mut h, "il testo spostato", |h| aggiunto(h).is_some_and(|o| (o.rettangolo[1] - prima.rettangolo[1] - 40.0 / k).abs() < 3.0));
        // Un'immagine dalla galleria, dove si tocca; poi spostata.
        dagli_strumenti(&mut h, pdf_modifica::IMMAGINE);
        h.run_steps(12);
        tocca_in(&mut h, egui::pos2(16.0 + 300.0 * k, 78.0 + 600.0 * k));
        h.run_steps(3);
        tocca(&mut h, pdf_modifica::DALLA_GALLERIA);
        h.state().simula_immagine_di_prova(&tascabile_pdf::prova::foto_di_una_firma(400, 200, 80));
        let immagine = |h: &Telefono| oggetti_pdf(h, 0).into_iter().find(|o| o.genere == tascabile_pdf::GenereOggetto::ImmagineAggiunta);
        finche(&mut h, "l'immagine inserita", |h| immagine(h).is_some());
        let prima = immagine(&h).unwrap();
        assert!(((prima.rettangolo[0] + prima.rettangolo[2]) / 2.0 - 300.0).abs() < 2.0, "{:?}", prima.rettangolo);
        assert_eq!(h.state().pdf.as_ref().unwrap().modifica.scelto, Some(prima.chiave()));
        let c = centro_dell_oggetto(&h, &prima);
        trascina_col_dito(&mut h, c, egui::vec2(-30.0, 0.0));
        finche(&mut h, "l'immagine spostata", |h| immagine(h).is_some_and(|o| (prima.rettangolo[0] - o.rettangolo[0] - 30.0 / k).abs() < 3.0));
        // Sostituisci con una foto in verticale: entra nel riquadro con le sue proporzioni, senza
        // stirarsi (analisi del 27/10, modifica I3).
        tocca(&mut h, pdf_modifica::SOSTITUISCI);
        h.run_steps(3);
        tocca(&mut h, pdf_modifica::DALLA_GALLERIA);
        h.state().simula_immagine_di_prova(&tascabile_pdf::prova::foto_di_una_firma(100, 200, 80));
        let in_verticale =
            |o: &tascabile_pdf::Oggetto| ((o.rettangolo[2] - o.rettangolo[0]) / (o.rettangolo[3] - o.rettangolo[1]) - 0.5).abs() < 0.02;
        finche(&mut h, "la foto in verticale", |h| immagine(h).is_some_and(|o| in_verticale(&o)));
        // Oscura: un rettangolo col dito sul codice, Applica, la conferma.
        dagli_strumenti(&mut h, pdf_modifica::OSCURA);
        h.run_steps(12);
        let segreto = oggetti_pdf(&h, 0).into_iter().find(|o| o.testo.contains("segreto")).unwrap();
        let r = h.state().pdf.as_ref().unwrap().lettore.sullo_schermo(0, segreto.rettangolo).unwrap().expand(4.0);
        trascina_col_dito(&mut h, r.left_top(), r.size());
        tocca(&mut h, pdf_modifica::APPLICA);
        h.run_steps(4);
        h.get_by_label(pdf_modifica::OSCURARE);
        tocca(&mut h, pdf_modifica::OSCURA);
        finche(&mut h, "il codice oscurato", |h| {
            let o = oggetti_pdf(h, 0);
            !o.is_empty() && !o.iter().any(|o| o.testo.contains("segreto"))
        });
        // Filigrana e numeri di pagina dai loro fogli.
        dagli_strumenti(&mut h, pdf_modifica::FILIGRANA);
        h.run_steps(8);
        tocca(&mut h, pdf_modifica::METTI_FILIGRANA);
        finche(&mut h, "la filigrana", |h| h.state().pdf.as_ref().unwrap().motore.stato().filigrana);
        dagli_strumenti(&mut h, pdf_lavoro::Strumento::NumeriDiPagina.nome());
        h.run_steps(8);
        tocca(&mut h, pdf_modifica::METTI_NUMERI);
        finche(&mut h, "i numeri", |h| h.state().pdf.as_ref().unwrap().motore.stato().numeri);
        // Annulla toglie i numeri, Ripeti li rimette.
        tocca(&mut h, pdf_lavoro::ANNULLA_MODIFICA);
        finche(&mut h, "i numeri annullati", |h| !h.state().pdf.as_ref().unwrap().motore.stato().numeri);
        tocca(&mut h, pdf_lavoro::RIPETI_MODIFICA);
        finche(&mut h, "i numeri rimessi", |h| h.state().pdf.as_ref().unwrap().motore.stato().numeri);
        // Salva e riapri: tutto c'è, il codice no, nemmeno nei byte.
        tocca(&mut h, schermo_pdf::MENU);
        tocca(&mut h, pdf_lavoro::SALVA);
        aspetta_il_motore(&mut h);
        assert_eq!(h.state().pdf_modificato(), Some(false));
        let salvati = std::fs::read(&percorso).unwrap();
        assert!(!salvati.windows(7).any(|w| w == b"segreto") && !salvati.windows(5).any(|w| w == b"98765"));
        let riaperto = tascabile_pdf::Pdf::apri(salvati, None).unwrap();
        let oggetti = riaperto.oggetti(0);
        assert!(oggetti.iter().any(|o| o.testo == "Visto, si approva"));
        assert!(oggetti.iter().any(|o| o.genere == tascabile_pdf::GenereOggetto::ImmagineAggiunta));
        assert!(oggetti.iter().any(|o| o.testo.starts_with("Pagina di prova bis")));
        indietro(&mut h);
        assert_eq!(h.state().schermata, Schermata::Home);
        let d = diagnosi_della_prova();
        for riga in [
            "] pdf: testo riscritto (paragrafo)\n",
            "] pdf: il paragrafo cambierebbe carattere, si avvisa\n",
            "] pdf: carattere, si continua\n",
            "] pdf: testo nuovo posato\n",
            "] pdf: oggetto spostato (testo aggiunto)\n",
            "] pdf: immagine inserita\n",
            "] pdf: oggetto spostato (immagine aggiunta)\n",
            "] pdf: immagine sostituita\n",
            "] pdf: oscura, 1 rettangoli applicati nella pagina 1\n",
            "] pdf: filigrana, opacità leggera\n",
            "] pdf: numeri di pagina, formato 1\n",
        ] {
            assert!(d.contains(riga), "manca «{riga}»:\n{d}");
        }
        for testo in ["Visto", "segreto", "98765", "prova bis", "Zanzibar"] {
            assert!(!d.contains(testo), "«{testo}» nella diagnosi:\n{d}");
        }
    }

    /// Una firma col dito sul foglio a tutto schermo: un'onda da `da`, verso destra.
    fn firma_col_dito(h: &mut Telefono, da: egui::Pos2) {
        h.event(egui::Event::PointerMoved(da));
        h.event(egui::Event::PointerButton { pos: da, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
        h.run_steps(1);
        let mut p = da;
        for i in 1..=24 {
            p = da + egui::vec2(10.0 * i as f32, 30.0 * (i as f32 * 0.6).sin());
            h.event(egui::Event::PointerMoved(p));
            h.run_steps(1);
        }
        h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
        h.run_steps(2);
    }

    /// La firma col dito: dal campo firma («Tocca per firmare»), il foglio a tutto schermo con
    /// Cancella e Fatto; la firma va nel campo, resta salvata nella memoria dell'app, si annulla;
    /// poi dalla proposta di Fill & Sign la firma salvata va dove si tocca; salvato e riaperto, il
    /// PDF ha la firma; riaperta l'app la firma salvata c'è ancora, e «Cancella firma salvata»
    /// delle Impostazioni la toglie.
    #[test]
    fn la_firma_col_dito_si_posa_si_salva_e_si_ritrova() {
        let (piattaforma, dir) = piattaforma_di_prova("pdf-firma-dito");
        let percorso = dir.join("radice/Consenso Zanzibar.pdf");
        std::fs::write(&percorso, tascabile_pdf::prova::modulo_da_compilare()).unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        h.state_mut().apri_file(&percorso, "Consenso Zanzibar.pdf");
        aspetta_il_lettore(&mut h);
        assert!(acceso(&h, "Compila"));
        let campo = "Firma del paziente o del tutore";
        tocca(&mut h, campo);
        h.run_steps(6);
        // Niente firme salvate: solo le righe per farne una.
        assert!(h.query_by_label(pdf_firma::METTI_LA_FIRMA).is_none());
        tocca(&mut h, pdf_firma::FIRMA_COL_DITO);
        h.run_steps(2);
        firma_col_dito(&mut h, egui::pos2(60.0, 400.0));
        assert_eq!(h.state().tratti_della_firma_pdf(), Some(1));
        tocca(&mut h, pdf_firma::CANCELLA);
        assert_eq!(h.state().tratti_della_firma_pdf(), Some(0));
        // Cancella si annulla: Rimetti, finché non si ricomincia a firmare (0.8.3).
        tocca(&mut h, pdf_firma::RIMETTI);
        assert_eq!(h.state().tratti_della_firma_pdf(), Some(1));
        tocca(&mut h, pdf_firma::CANCELLA);
        assert_eq!(h.state().tratti_della_firma_pdf(), Some(0));
        firma_col_dito(&mut h, egui::pos2(60.0, 380.0));
        firma_col_dito(&mut h, egui::pos2(80.0, 500.0));
        tocca(&mut h, pdf_firma::FATTO);
        aspetta_il_motore(&mut h);
        assert_eq!(h.state().tratti_della_firma_pdf(), None);
        let stato = h.state().pdf.as_ref().unwrap().motore.stato().clone();
        let riquadro = stato.campi.iter().find(|c| c.genere == tascabile_pdf::GenereCampo::Firma).unwrap().posti[0].rettangolo;
        assert_eq!(stato.segni.len(), 1);
        let s = stato.segni[0].clone();
        assert_eq!(s.genere, tascabile_pdf::GenereSegno::Firma);
        assert!(s.rettangolo[0] > riquadro[0] && s.rettangolo[2] < riquadro[2] && s.rettangolo[1] >= riquadro[1] - 1.0, "{:?}", s.rettangolo);
        // La firma posata è scelta e il campo non dice più «Tocca per firmare».
        assert_eq!(h.state().pdf.as_ref().unwrap().compila.segno, Some((0, s.indice)));
        assert!(h.query_by_label(campo).is_none());
        assert!(dir.join("app/firme/firma.txt").exists(), "la firma non è salvata");
        // Annullata, il campo torna da firmare.
        tocca(&mut h, pdf_lavoro::ANNULLA_MODIFICA);
        aspetta_il_motore(&mut h);
        assert!(h.state().pdf.as_ref().unwrap().motore.stato().segni.is_empty());
        h.get_by_label(campo);
        // Dalla proposta, in un punto vuoto della seconda metà della pagina: la firma salvata.
        let dove = egui::pos2(16.0 + 200.0, 78.0 + 450.0);
        tocca_in(&mut h, dove);
        h.run_steps(3);
        if h.query_by_label(pdf_firma::FIRMA).is_none() {
            tocca_in(&mut h, dove);
            h.run_steps(3);
        }
        tocca(&mut h, pdf_firma::FIRMA);
        h.run_steps(6);
        tocca(&mut h, pdf_firma::METTI_LA_FIRMA);
        aspetta_il_motore(&mut h);
        let s = h.state().pdf.as_ref().unwrap().motore.stato().segni[0].clone();
        let k = h.state().pdf.as_ref().unwrap().lettore.dp_per_pt(s.pagina);
        let centro = egui::pos2((s.rettangolo[0] + s.rettangolo[2]) / 2.0, (s.rettangolo[1] + s.rettangolo[3]) / 2.0);
        let a_schermo = h.state().pdf.as_ref().unwrap().lettore.sullo_schermo(0, [centro.x, centro.y, centro.x, centro.y]).unwrap().center();
        assert!((a_schermo - dove).length() < 4.0, "la firma è a {a_schermo:?}, toccato {dove:?}");
        assert!(
            s.rettangolo[2] - s.rettangolo[0] <= tascabile_pdf::firma::LARGA + 4.0
                && s.rettangolo[3] - s.rettangolo[1] <= tascabile_pdf::firma::ALTA + 4.0
        );
        assert!(k > 0.0);
        // Salva e riapri: la firma è nel PDF.
        tocca(&mut h, schermo_pdf::MENU);
        tocca(&mut h, pdf_lavoro::SALVA);
        aspetta_il_motore(&mut h);
        let segni = tascabile_pdf::Pdf::apri(std::fs::read(&percorso).unwrap(), None).unwrap().segni();
        assert_eq!(segni.iter().filter(|s| s.genere == tascabile_pdf::GenereSegno::Firma).count(), 1);
        // Un'altra volta dell'app: la firma salvata c'è ancora; le Impostazioni la tolgono.
        let piattaforma = Piattaforma::con_cartelle(dir.join("app"), dir.join("cache"), dir.join("radice"));
        let mut altra = telefono(TascabileApp::new(piattaforma, Instant::now()));
        assert!(matches!(altra.state_mut().firme().di(false), Some(tascabile_pdf::Firma::Tratti(d)) if d.tratti.len() == 2));
        dalle_impostazioni(&mut altra, impostazioni::CANCELLA_FIRMA);
        assert!(!dir.join("app/firme/firma.txt").exists() && !altra.state_mut().firme().qualcuna());
        altra.run_steps(2);
        assert!(altra.query_by_label(impostazioni::CANCELLA_FIRMA).is_none());
        let d = diagnosi_della_prova();
        for riga in [
            "] pdf: campo firma toccato nella pagina 1\n",
            "] pdf: firma col dito fatta, 2 tratti,",
            "] firma: firma salvata, Tratti(2 tratti, stilografica, nero)\n",
            "] pdf: firma posate, col dito, nella pagina 1\n",
            "] firma: firma e iniziali salvate cancellate\n",
        ] {
            assert!(d.contains(riga), "manca «{riga}»:\n{d}");
        }
        assert!(!d.contains("Zanzibar") && !d.contains("firme/"), "{d}");
    }

    /// Revisione del 31/10, punto 3: una firma nuova che non si riesce a salvare si posa lo stesso,
    /// lei e non quella salvata prima. La prima firma, un tratto solo, è larga; la seconda, due
    /// tratti uno sopra l'altro, è alta: nel campo il segno ha le proporzioni della firma posata.
    #[test]
    fn una_firma_nuova_che_non_si_salva_si_posa_lo_stesso() {
        let (piattaforma, dir) = piattaforma_di_prova("pdf-firma-non-salvata");
        let percorso = dir.join("radice/Consenso Zanzibar.pdf");
        std::fs::write(&percorso, tascabile_pdf::prova::modulo_da_compilare()).unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        h.state_mut().apri_file(&percorso, "Consenso Zanzibar.pdf");
        aspetta_il_lettore(&mut h);
        let campo = "Firma del paziente o del tutore";
        let proporzione = |h: &Telefono| {
            let r = h.state().pdf.as_ref().unwrap().motore.stato().segni[0].rettangolo;
            (r[2] - r[0]) / (r[3] - r[1])
        };
        tocca(&mut h, campo);
        h.run_steps(6);
        tocca(&mut h, pdf_firma::FIRMA_COL_DITO);
        h.run_steps(2);
        firma_col_dito(&mut h, egui::pos2(60.0, 400.0));
        tocca(&mut h, pdf_firma::FATTO);
        aspetta_il_motore(&mut h);
        let prima = proporzione(&h);
        tocca(&mut h, pdf_lavoro::ANNULLA_MODIFICA);
        aspetta_il_motore(&mut h);
        // Al posto del file della firma salvata, una cartella: la prossima non si salva.
        let file = dir.join("app/firme/firma.txt");
        std::fs::remove_file(&file).unwrap();
        std::fs::create_dir(&file).unwrap();
        tocca(&mut h, campo);
        h.run_steps(6);
        tocca(&mut h, pdf_firma::FIRMA_COL_DITO);
        h.run_steps(2);
        firma_col_dito(&mut h, egui::pos2(60.0, 300.0));
        firma_col_dito(&mut h, egui::pos2(60.0, 600.0));
        tocca(&mut h, pdf_firma::FATTO);
        aspetta_il_motore(&mut h);
        let dopo = proporzione(&h);
        assert!(prima > dopo * 1.5, "si è posata la firma salvata prima: proporzioni {prima} e {dopo}");
        // C'era una firma salvata: si chiede se sostituirla, e il salvataggio non riesce.
        tocca(&mut h, pdf_firma::SALVA_LA_NUOVA);
        assert!(diagnosi_della_prova().contains("] firma: firma non salvata ("));
    }

    /// Revisione del 31/10, punto 3: con una firma già salvata, la nuova va sul documento e poi si
    /// chiede se salvarla al posto di quella; «Tieni quella di prima» la lascia, «Salva la nuova» la
    /// cambia. Un collega col telefono in mano non sostituisce più la firma dell'autore senza domande.
    #[test]
    fn prima_di_sostituire_la_firma_salvata_si_chiede() {
        let (piattaforma, dir) = piattaforma_di_prova("pdf-firma-sostituita");
        let percorso = dir.join("radice/Consenso Zanzibar.pdf");
        std::fs::write(&percorso, tascabile_pdf::prova::modulo_da_compilare()).unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        h.state_mut().apri_file(&percorso, "Consenso Zanzibar.pdf");
        aspetta_il_lettore(&mut h);
        let campo = "Firma del paziente o del tutore";
        let firma_nuova = |h: &mut Telefono, da: &[egui::Pos2]| {
            tocca(h, campo);
            h.run_steps(6);
            tocca(h, pdf_firma::FIRMA_COL_DITO);
            h.run_steps(2);
            for p in da {
                firma_col_dito(h, *p);
            }
            tocca(h, pdf_firma::FATTO);
            aspetta_il_motore(h);
        };
        // La prima si salva senza domande.
        firma_nuova(&mut h, &[egui::pos2(60.0, 400.0)]);
        assert!(h.query_by_label(pdf_firma::SALVARE_LA_NUOVA).is_none());
        let salvata = dir.join("app/firme/firma.txt");
        let prima = std::fs::read(&salvata).unwrap();
        tocca(&mut h, pdf_lavoro::ANNULLA_MODIFICA);
        aspetta_il_motore(&mut h);
        // La seconda va sul documento; la salvata resta, se si sceglie così.
        firma_nuova(&mut h, &[egui::pos2(60.0, 300.0), egui::pos2(60.0, 600.0)]);
        h.get_by_label(pdf_firma::SALVARE_LA_NUOVA);
        assert_eq!(h.state().pdf.as_ref().unwrap().motore.stato().segni.len(), 1);
        tocca(&mut h, pdf_firma::TIENI_QUELLA_DI_PRIMA);
        assert_eq!(std::fs::read(&salvata).unwrap(), prima);
        tocca(&mut h, pdf_lavoro::ANNULLA_MODIFICA);
        aspetta_il_motore(&mut h);
        // La terza prende il suo posto.
        firma_nuova(&mut h, &[egui::pos2(60.0, 300.0), egui::pos2(60.0, 600.0)]);
        tocca(&mut h, pdf_firma::SALVA_LA_NUOVA);
        assert_ne!(std::fs::read(&salvata).unwrap(), prima);
        let d = diagnosi_della_prova();
        for riga in ["] firma: firma nuova, si chiede se sostituire la salvata\n", "] firma: firma salvata lasciata com'era\n"] {
            assert!(d.contains(riga), "manca «{riga}»:\n{d}");
        }
    }

    /// Roadmap della sicurezza, Fase 1: la firma salvata si posa solo dopo l'impronta o il PIN
    /// (la finestra di Android; sul PC un finto che risponde quello che si sceglie). Una firma
    /// fatta adesso col dito no: è di chi ha il dito. Annullata l'impronta, o su un telefono senza
    /// blocco, niente firma; riuscita, la firma va nel campo e per cinque minuti non si richiede.
    /// Spenta nelle Impostazioni, non si chiede più, anche alla volta dopo dell'app.
    #[test]
    fn la_firma_salvata_si_mette_dopo_l_impronta() {
        use tascabile_platform::EsitoImpronta;
        let (piattaforma, dir) = piattaforma_di_prova("pdf-firma-impronta");
        let percorso = dir.join("radice/Consenso Zanzibar.pdf");
        std::fs::write(&percorso, tascabile_pdf::prova::modulo_da_compilare()).unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        assert!(h.state().impostazioni.impronta_firma, "accesa di base");
        h.state_mut().apri_file(&percorso, "Consenso Zanzibar.pdf");
        aspetta_il_lettore(&mut h);
        let campo = "Firma del paziente o del tutore";
        let segni = |h: &Telefono| h.state().pdf.as_ref().unwrap().motore.stato().segni.len();
        tocca(&mut h, campo);
        h.run_steps(6);
        tocca(&mut h, pdf_firma::FIRMA_COL_DITO);
        h.run_steps(2);
        firma_col_dito(&mut h, egui::pos2(60.0, 400.0));
        tocca(&mut h, pdf_firma::FATTO);
        aspetta_il_motore(&mut h);
        assert_eq!((segni(&h), h.state().piattaforma.impronte_chieste()), (1, 0), "la firma appena fatta non chiede l'impronta");
        tocca(&mut h, pdf_lavoro::ANNULLA_MODIFICA);
        aspetta_il_motore(&mut h);
        let metti_la_salvata = |h: &mut Telefono, esito: EsitoImpronta| {
            h.state().piattaforma.simula_impronta(esito);
            tocca(h, campo);
            h.run_steps(6);
            tocca(h, pdf_firma::METTI_LA_FIRMA);
            h.run_steps(3);
            aspetta_il_motore(h);
        };
        // Annullata: niente firma, e il campo resta da firmare.
        metti_la_salvata(&mut h, EsitoImpronta::Annullata);
        assert_eq!((segni(&h), h.state().piattaforma.impronte_chieste()), (0, 1));
        assert!(h.state().pdf.as_ref().unwrap().compila.firma.dove.is_none());
        h.get_by_label(campo);
        // Un telefono senza blocco dello schermo: niente firma, e l'avviso dice perché.
        metti_la_salvata(&mut h, EsitoImpronta::SenzaBlocco);
        assert_eq!((segni(&h), h.state().piattaforma.impronte_chieste()), (0, 2));
        h.get_by_label(blocco::SENZA_BLOCCO_FIRMA);
        // Riuscita: la firma va nel campo.
        metti_la_salvata(&mut h, EsitoImpronta::Riuscita);
        assert_eq!((segni(&h), h.state().piattaforma.impronte_chieste()), (1, 3));
        assert!(h.query_by_label(campo).is_none());
        // Subito dopo non si richiede; dopo cinque minuti sì.
        tocca(&mut h, pdf_lavoro::ANNULLA_MODIFICA);
        aspetta_il_motore(&mut h);
        metti_la_salvata(&mut h, EsitoImpronta::Annullata);
        assert_eq!((segni(&h), h.state().piattaforma.impronte_chieste()), (1, 3));
        tocca(&mut h, pdf_lavoro::ANNULLA_MODIFICA);
        aspetta_il_motore(&mut h);
        h.state_mut().blocco.riuscita_alle = Some(std::time::SystemTime::now() - blocco::VALE_UN_IMPRONTA);
        metti_la_salvata(&mut h, EsitoImpronta::Annullata);
        assert_eq!((segni(&h), h.state().piattaforma.impronte_chieste()), (0, 4));
        // Spenta nelle Impostazioni: la firma salvata va senza domande, anche la volta dopo.
        assert!(impostazioni::leggi_impronta_firma(&dir.join("app")));
        impostazioni::scrivi_impronta_firma(&dir.join("app"), false).unwrap();
        let piattaforma = Piattaforma::con_cartelle(dir.join("app"), dir.join("cache"), dir.join("radice"));
        let mut altra = telefono(TascabileApp::new(piattaforma, Instant::now()));
        assert!(!altra.state().impostazioni.impronta_firma);
        altra.state_mut().apri_file(&percorso, "Consenso Zanzibar.pdf");
        aspetta_il_lettore(&mut altra);
        metti_la_salvata(&mut altra, EsitoImpronta::Annullata);
        assert_eq!((segni(&altra), altra.state().piattaforma.impronte_chieste()), (1, 0));
        let d = diagnosi_della_prova();
        for riga in [
            "] impronta: chiesta per la firma salvata\n",
            "] impronta: annullata per la firma salvata\n",
            "] impronta: telefono senza blocco per la firma salvata\n",
            "] impronta: riuscita per la firma salvata\n",
        ] {
            assert!(d.contains(riga), "manca «{riga}»:\n{d}");
        }
    }

    /// L'interruttore «Impronta per la firma» nella sezione Sicurezza e privacy: acceso di base; si
    /// spegne solo dopo l'impronta (altrimenti chi ha il telefono sbloccato in mano la spegnerebbe e
    /// poi userebbe la firma), si riaccende subito; spento resta spento alla volta dopo dell'app.
    #[test]
    fn l_impronta_per_la_firma_si_spegne_solo_con_l_impronta() {
        use tascabile_platform::EsitoImpronta;
        let (piattaforma, d) = piattaforma_di_prova("impronta-firma");
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        let chieste = |h: &Telefono| h.state().piattaforma.impronte_chieste();
        h.state().piattaforma.simula_impronta(EsitoImpronta::Annullata);
        dalle_impostazioni(&mut h, impostazioni::IMPRONTA_FIRMA);
        h.run_steps(2);
        assert!(h.state().impostazioni.impronta_firma && impostazioni::leggi_impronta_firma(&d.join("app")), "annullata: resta accesa");
        assert_eq!(chieste(&h), 1);
        h.state().piattaforma.simula_impronta(EsitoImpronta::Riuscita);
        tocca(&mut h, impostazioni::IMPRONTA_FIRMA);
        h.run_steps(2);
        assert!(!h.state().impostazioni.impronta_firma && !impostazioni::leggi_impronta_firma(&d.join("app")));
        assert_eq!(chieste(&h), 2);
        let acceso = h.get_by_role_and_label(Role::Button, impostazioni::IMPRONTA_FIRMA).accesskit_node().toggled();
        assert_eq!(acceso, Some(egui::accesskit::Toggled::False));
        // Riaccesa subito; rispenta entro cinque minuti dall'impronta, senza richiederla.
        tocca(&mut h, impostazioni::IMPRONTA_FIRMA);
        h.run_steps(2);
        assert!(h.state().impostazioni.impronta_firma);
        tocca(&mut h, impostazioni::IMPRONTA_FIRMA);
        h.run_steps(2);
        assert!(!h.state().impostazioni.impronta_firma);
        assert_eq!(chieste(&h), 2);
        let piattaforma = Piattaforma::con_cartelle(d.join("app"), d.join("cache"), d.join("radice"));
        let altra = telefono(TascabileApp::new(piattaforma, Instant::now()));
        assert!(!altra.state().impostazioni.impronta_firma);
        let diag = diagnosi_della_prova();
        for riga in [
            "] impronta: annullata per spegnere l'impronta per la firma\n",
            "] impostazioni: impronta per la firma spenta\n",
            "] impostazioni: impronta per la firma accesa\n",
        ] {
            assert!(diag.contains(riga), "manca «{riga}»:\n{diag}");
        }
    }

    /// Il blocco dell'app (roadmap della sicurezza, Fase 1): spento di base; si accende dopo
    /// l'impronta, e su un telefono senza blocco dello schermo no. Fuori dall'app per poco non
    /// succede niente; fuori per due minuti, al ritorno si vede solo la schermata del blocco e
    /// l'impronta si chiede da sola. All'avvio l'app parte bloccata; annullata, resta bloccata. Se
    /// il telefono non ha più un blocco dello schermo, l'app si sblocca e il blocco si spegne.
    #[test]
    fn il_blocco_dell_app_chiede_l_impronta_all_avvio_e_al_ritorno() {
        use tascabile_platform::EsitoImpronta;
        let (piattaforma, d) = piattaforma_di_prova("blocco-app");
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        let chieste = |h: &Telefono| h.state().piattaforma.impronte_chieste();
        assert!(!h.state().impostazioni.blocco && !h.state().blocco.bloccata);
        h.state().piattaforma.simula_impronta(EsitoImpronta::SenzaBlocco);
        dalle_impostazioni(&mut h, impostazioni::BLOCCO_APP);
        h.run_steps(2);
        assert!(!h.state().impostazioni.blocco && !impostazioni::leggi_blocco(&d.join("app")));
        h.get_by_label(blocco::BLOCCO_SENZA_BLOCCO);
        h.state().piattaforma.simula_impronta(EsitoImpronta::Riuscita);
        tocca(&mut h, impostazioni::BLOCCO_APP);
        h.run_steps(2);
        assert!(h.state().impostazioni.blocco && impostazioni::leggi_blocco(&d.join("app")));
        // Fuori per poco: niente.
        h.input_mut().focused = false;
        h.run_steps(2);
        h.input_mut().focused = true;
        h.run_steps(2);
        assert!(!h.state().blocco.bloccata);
        // Fuori per due minuti: bloccata, e l'impronta si chiede da sola.
        h.state().piattaforma.simula_impronta(EsitoImpronta::Annullata);
        h.input_mut().focused = false;
        h.run_steps(2);
        // Come se fossero passati due minuti (anche dall'impronta che ha acceso il blocco).
        let indietro_di = |t: &mut Option<std::time::SystemTime>| *t = t.map(|t| t - blocco::BLOCCO_DOPO);
        indietro_di(&mut h.state_mut().blocco.fuori_dal);
        indietro_di(&mut h.state_mut().blocco.riuscita_alle);
        let prima = chieste(&h);
        h.input_mut().focused = true;
        h.run_steps(3);
        assert!(h.state().blocco.bloccata);
        assert_eq!(chieste(&h), prima + 1);
        h.get_by_label(blocco::BLOCCATA);
        assert!(h.query_by_label(impostazioni::BLOCCO_APP).is_none(), "sotto il blocco non si vede niente");
        // Indietro non toglie il blocco.
        indietro(&mut h);
        assert!(h.state().blocco.bloccata);
        h.state().piattaforma.simula_impronta(EsitoImpronta::Riuscita);
        tocca(&mut h, blocco::SBLOCCA);
        h.run_steps(2);
        assert!(!h.state().blocco.bloccata);
        h.get_by_label(impostazioni::BLOCCO_APP);
        // Un'altra volta dell'app: parte bloccata, e annullata resta bloccata.
        let piattaforma = Piattaforma::con_cartelle(d.join("app"), d.join("cache"), d.join("radice"));
        piattaforma.simula_impronta(EsitoImpronta::Annullata);
        let mut altra = telefono(TascabileApp::new(piattaforma, Instant::now()));
        altra.run_steps(2);
        assert!(altra.state().blocco.bloccata);
        assert_eq!(chieste(&altra), 1);
        altra.get_by_label(blocco::BLOCCATA);
        assert!(altra.query_by_label("Impostazioni").is_none());
        // Il telefono non ha più un blocco dello schermo: l'app si sblocca e il blocco si spegne.
        altra.state().piattaforma.simula_impronta(EsitoImpronta::SenzaBlocco);
        tocca(&mut altra, blocco::SBLOCCA);
        altra.run_steps(2);
        assert!(!altra.state().blocco.bloccata && !altra.state().impostazioni.blocco && !impostazioni::leggi_blocco(&d.join("app")));
        altra.get_by_label(blocco::BLOCCO_SPENTO);
        let diag = diagnosi_della_prova();
        for riga in [
            "] impronta: telefono senza blocco per accendere il blocco dell'app\n",
            "] impostazioni: blocco dell'app acceso\n",
            "] blocco: app bloccata\n",
            "] impronta: annullata per sbloccare l'app\n",
            "] blocco: app sbloccata\n",
            "] impostazioni: blocco dell'app spento\n",
        ] {
            assert!(diag.contains(riga), "manca «{riga}»:\n{diag}");
        }
    }

    /// Il tratto e il colore della firma col dito (ritocchi della 0.8.2, R2): si comincia con la
    /// stilografica nera; cambiati dopo aver firmato, la firma fatta resta e si ridisegna; Fatto la
    /// salva con la penna blu, e la volta dopo il foglio comincia da lì. (Gli spessori e il colore
    /// nel PDF disegnato li prova il crate, `firma::test`.)
    #[test]
    fn la_firma_col_dito_col_tratto_e_il_colore_scelti() {
        let (piattaforma, dir) = piattaforma_di_prova("pdf-firma-tratto");
        let percorso = dir.join("radice/Modulo Zanzibar.pdf");
        std::fs::write(&percorso, tascabile_pdf::prova::modulo_da_compilare()).unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        h.state_mut().apri_file(&percorso, "Modulo Zanzibar.pdf");
        aspetta_il_lettore(&mut h);
        tocca(&mut h, "Firma del paziente o del tutore");
        h.run_steps(6);
        tocca(&mut h, pdf_firma::FIRMA_COL_DITO);
        h.run_steps(2);
        assert!(acceso(&h, "Stilografica") && acceso(&h, "Nero") && !acceso(&h, "Penna") && !acceso(&h, "Blu"));
        firma_col_dito(&mut h, egui::pos2(60.0, 400.0));
        tocca(&mut h, "Penna");
        tocca(&mut h, "Blu");
        assert!(acceso(&h, "Penna") && acceso(&h, "Blu") && !acceso(&h, "Stilografica"));
        assert_eq!(h.state().tratti_della_firma_pdf(), Some(1), "la firma fatta è rimasta");
        tocca(&mut h, pdf_firma::FATTO);
        aspetta_il_motore(&mut h);
        let Some(tascabile_pdf::Firma::Tratti(d)) = h.state_mut().firme().di(false).cloned() else { panic!("niente firma col dito salvata") };
        assert_eq!((d.stile, d.colore, d.tratti.len()), (tascabile_pdf::tratto::Stile::Penna, tascabile_pdf::tratto::Colore::Blu, 1));
        // Nella pagina è una firma di Fill & Sign.
        let s = h.state().pdf.as_ref().unwrap().motore.stato().segni[0].clone();
        assert_eq!(s.genere, tascabile_pdf::GenereSegno::Firma);
        // La volta dopo, le iniziali col dito cominciano dalla penna blu della firma.
        tocca(&mut h, pdf_lavoro::ANNULLA_MODIFICA);
        aspetta_il_motore(&mut h);
        tocca(&mut h, "Firma del paziente o del tutore");
        h.run_steps(6);
        tocca(&mut h, pdf_firma::INIZIALI_COL_DITO);
        h.run_steps(2);
        assert!(acceso(&h, "Penna") && acceso(&h, "Blu"));
        let d = diagnosi_della_prova();
        for riga in [
            "] pdf: firma col dito, stilografica nero\n",
            "] pdf: firma col dito, tratto penna\n",
            "] pdf: firma col dito, colore blu\n",
            "] pdf: firma col dito fatta, 1 tratti,",
            "] firma: firma salvata, Tratti(1 tratti, penna, blu)\n",
            "] pdf: iniziali col dito, penna blu\n",
        ] {
            assert!(d.contains(riga), "manca «{riga}»:\n{d}");
        }
    }

    /// La firma da una foto su carta: chiesta alla fotocamera di sistema, arrivata nel file della
    /// cache, pulita dal motore sul suo thread, guardata e usata: va dove si è toccato, salvata
    /// come immagine; il file della foto non resta. Indietro dalla galleria: niente.
    #[test]
    fn la_firma_da_una_foto_si_pulisce_si_guarda_e_si_posa() {
        let (piattaforma, dir) = piattaforma_di_prova("pdf-firma-foto");
        let percorso = dir.join("radice/Modulo Zanzibar.pdf");
        std::fs::write(&percorso, tascabile_pdf::prova::modulo()).unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        h.state_mut().apri_file(&percorso, "Modulo Zanzibar.pdf");
        aspetta_il_lettore(&mut h);
        // Dal foglio Strumenti: la firma va dove si tocca dopo.
        dagli_strumenti(&mut h, pdf_firma::FIRMA);
        h.run_steps(6);
        assert!(acceso(&h, "Compila"));
        tocca(&mut h, pdf_firma::FOTO_DELLA_FIRMA);
        assert_eq!(h.state().piattaforma.foto_chieste(), [tascabile_platform::FonteFoto::Fotocamera]);
        h.state().piattaforma.simula_foto_firma(Some(&tascabile_pdf::prova::foto_di_una_firma(2400, 1800, 85)));
        let fine = Instant::now() + Duration::from_secs(20);
        while h.query_by_label(pdf_firma::USA_QUESTA_FIRMA).is_none() {
            assert!(Instant::now() < fine, "la foto non è stata pulita in 20 s:\n{}", diagnosi_della_prova());
            std::thread::sleep(Duration::from_millis(10));
            h.step();
        }
        assert_eq!(rimasto_fra_le_foto(&dir.join("cache")), Vec::<String>::new(), "la foto è rimasta nella cache");
        h.run_steps(10);
        tocca(&mut h, pdf_firma::USA_QUESTA_FIRMA);
        h.run_steps(4);
        assert!(dir.join("app/firme/firma.png").exists(), "{}", diagnosi_della_prova());
        assert!(h.state().pdf.as_ref().unwrap().motore.stato().segni.is_empty(), "posata prima del tocco");
        let dove = egui::pos2(16.0 + 190.0, 78.0 + 470.0);
        tocca_in(&mut h, dove);
        h.run_steps(3);
        aspetta_il_motore(&mut h);
        let segni = h.state().pdf.as_ref().unwrap().motore.stato().segni.clone();
        assert_eq!(segni.len(), 1);
        assert_eq!(segni[0].genere, tascabile_pdf::GenereSegno::Firma);
        // Dalla galleria, poi indietro senza immagine: nessuna firma nuova.
        tocca(&mut h, "Compila");
        dagli_strumenti(&mut h, pdf_firma::FIRMA);
        h.run_steps(6);
        tocca(&mut h, pdf_firma::FIRMA_DA_IMMAGINE);
        h.state().piattaforma.simula_foto_firma(None);
        h.run_steps(4);
        assert!(h.query_by_label(pdf_firma::USA_QUESTA_FIRMA).is_none());
        let d = diagnosi_della_prova();
        for riga in [
            "] foto della firma: chiesta alla fotocamera\n",
            "] foto della firma: arrivata dalla fotocamera\n",
            "] pdf: firma dalla foto usata\n",
            "] pdf: firma posate, da un'immagine, nella pagina 1\n",
            "] foto della firma: annullata\n",
        ] {
            assert!(d.contains(riga), "manca «{riga}»:\n{d}");
        }
    }

    /// Un modulo coi calcoli veri: la proposta di accendere gli script, una volta.
    #[test]
    fn un_modulo_coi_calcoli_propone_di_accenderli() {
        let mut h = telefono_col_pdf("pdf-calcoli", tascabile_pdf::prova::modulo_coi_calcoli());
        h.get_by_label(pdf_compila::CALCOLI);
        tocca(&mut h, pdf_compila::ACCENDI_I_CALCOLI);
        aspetta_il_motore(&mut h);
        assert!(h.state().pdf.as_ref().unwrap().motore.stato().javascript);
        assert!(h.query_by_label(pdf_compila::CALCOLI).is_none());
        // Tornando a Leggi e poi a Compila non chiede più.
        tocca(&mut h, "Leggi");
        tocca(&mut h, "Compila");
        assert!(h.query_by_label(pdf_compila::CALCOLI).is_none());
        assert!(diagnosi_della_prova().contains("] pdf: calcoli del modulo accesi\n"));
        // Indietro vale «Non ora»: in un altro modulo coi calcoli il dialogo si chiude e si resta.
        let mut altro = telefono_col_pdf("pdf-calcoli-indietro", tascabile_pdf::prova::modulo_coi_calcoli());
        altro.get_by_label(pdf_compila::CALCOLI);
        indietro(&mut altro);
        assert!(altro.query_by_label(pdf_compila::CALCOLI).is_none());
        assert_eq!(altro.state().schermata, Schermata::Pdf);
        assert!(!altro.state().pdf.as_ref().unwrap().motore.stato().javascript);
    }

    /// Dove sta sullo schermo il centro della parola `parola` della prima pagina di `byte`, col
    /// PDF appena aperto nel lettore: la pagina sotto la barra alta (64 dp), a 16 dp dal lato e 14
    /// dalla cima, larga 380 dp per 595 pt.
    fn parola_sullo_schermo(byte: &[u8], parola: &str) -> egui::Pos2 {
        let pdf = tascabile_pdf::Pdf::apri(byte.to_vec(), None).unwrap();
        let t = pdf.testo(0).unwrap();
        let caratteri: String = t.glyphs.iter().map(|g| g.text.as_str()).collect();
        let i = caratteri[..caratteri.find(parola).expect("parola nella pagina")].chars().count();
        let (a, b) = (t.glyphs[i].rect, t.glyphs[i + parola.chars().count() - 1].rect);
        let k = 380.0 / 595.0;
        egui::pos2(16.0 + (a[0] + b[2]) / 2.0 * k, 64.0 + 14.0 + (a[1] + a[3]) / 2.0 * k)
    }

    /// Una pressione lunga col dito in `dove` (eventi del tocco: egui la vede solo così), poi
    /// passi finché la parola non è selezionata (il testo della pagina arriva dal pool).
    fn pressione_lunga(h: &mut Telefono, dove: egui::Pos2) {
        let tocco = |fase| egui::Event::Touch { device_id: egui::TouchDeviceId(1), id: egui::TouchId(0), phase: fase, pos: dove, force: None };
        h.event(tocco(egui::TouchPhase::Start));
        h.event(egui::Event::PointerMoved(dove));
        h.event(egui::Event::PointerButton { pos: dove, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
        h.run_steps(4);
        h.event(tocco(egui::TouchPhase::End));
        h.event(egui::Event::PointerButton { pos: dove, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
        h.event(egui::Event::PointerGone);
        let fine = Instant::now() + Duration::from_secs(20);
        h.step();
        while h.state().pdf.as_ref().is_some_and(|p| p.lettore.ha_selezione() && p.lettore.selezione().is_none()) {
            assert!(Instant::now() < fine, "il testo della pagina non è arrivato");
            std::thread::sleep(Duration::from_millis(5));
            h.step();
        }
        h.run_steps(2);
    }

    #[test]
    fn la_parola_selezionata_si_copia_negli_appunti_e_indietro_chiude_la_selezione() {
        let modulo = tascabile_pdf::prova::modulo();
        let dove = parola_sullo_schermo(&modulo, "dichiara");
        let mut h = telefono_col_pdf("pdf-selezione", modulo);
        pressione_lunga(&mut h, dove);
        let testo = |h: &Telefono| h.state().pdf.as_ref().unwrap().lettore.testo_selezionato();
        assert_eq!(testo(&h).as_deref(), Some("dichiara"));
        // La barra scura con Copia e Tutto; Copia mette la parola negli appunti e chiude.
        tocca(&mut h, tascabile_pdf::lettore::COPIA);
        assert_eq!(h.state().piattaforma.incolla_testo().as_deref(), Some("dichiara"));
        assert!(!h.state().pdf.as_ref().unwrap().lettore.ha_selezione());
        assert!(h.query_by_label(tascabile_pdf::lettore::COPIA).is_none());
        // Di nuovo, poi Tutto: il testo della pagina; Indietro chiude la selezione, poi il lettore.
        pressione_lunga(&mut h, dove);
        tocca(&mut h, tascabile_pdf::lettore::TUTTO);
        assert!(testo(&h).unwrap().starts_with("CONSENSO INFORMATO AL TRATTAMENTO\n"), "{:?}", testo(&h));
        indietro(&mut h);
        assert!(!h.state().pdf.as_ref().unwrap().lettore.ha_selezione());
        assert_eq!(h.state().schermata, Schermata::Pdf);
        let d = diagnosi_della_prova();
        for riga in [
            "] pdf: selezione di 8 caratteri\n",
            "] pdf: selezione copiata\n",
            "] appunti: copiati 8 caratteri\n",
            "] indietro: selezione del pdf chiusa\n",
        ] {
            assert!(d.contains(riga), "manca «{riga}»:\n{d}");
        }
        assert!(d.contains("] pdf: tutta la pagina selezionata, "), "{d}");
        for parola in ["dichiara", "CONSENSO", "Zanzibar"] {
            assert!(!d.contains(parola), "«{parola}» nella diagnosi:\n{d}");
        }
    }

    #[test]
    fn una_pagina_senza_testo_lo_dice_con_un_avviso() {
        let mut h = telefono_col_pdf("pdf-senza-testo", tascabile_pdf::prova::pdf(&[tascabile_pdf::prova::Pagina::bianca(595.0, 842.0)]));
        pressione_lunga(&mut h, egui::pos2(200.0, 300.0));
        let fine = Instant::now() + Duration::from_secs(20);
        while h.query_by_label(schermo_pdf::SENZA_TESTO).is_none() {
            assert!(Instant::now() < fine, "nessun avviso per la pagina senza testo");
            std::thread::sleep(Duration::from_millis(5));
            h.step();
        }
        assert!(diagnosi_della_prova().contains("] pdf: pressione lunga su una pagina senza testo\n"));
    }

    /// Passi finché c'è il dialogo della password (o il lettore), al massimo 20 s.
    fn aspetta_la_password(h: &mut Telefono) {
        let fine = Instant::now() + Duration::from_secs(20);
        h.step();
        while h.state().password_pdf.is_none() && h.state().schermata != Schermata::Pdf {
            assert!(Instant::now() < fine, "né la password né il lettore in 20 s");
            std::thread::sleep(Duration::from_millis(5));
            h.step();
        }
        h.run_steps(2);
    }

    #[test]
    fn un_pdf_con_password_la_chiede_e_non_la_scrive_mai() {
        let (piattaforma, dir) = piattaforma_di_prova("pdf-password");
        let percorso = dir.join("radice/Referto Zanzibar.pdf");
        std::fs::write(&percorso, tascabile_pdf::prova::cifrato("Kilimangiaro9")).unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        h.state_mut().apri_file(&percorso, "Referto Zanzibar.pdf");
        aspetta_la_password(&mut h);
        // Il dialogo col nome, il campo che ha la tastiera delle password, Annulla e Apri.
        h.get_by_label(schermo_pdf::PDF_PROTETTO);
        h.get_by_label("«Referto Zanzibar» si apre con una password.");
        assert_eq!(h.state().fuoco, Fuoco::Campo);
        assert!(h.state().piattaforma.keyboard_visible());
        assert_eq!(h.state().piattaforma.tipo_della_tastiera(), TipoTastiera::Password);
        assert!(h.query_by_label(schermo_pdf::PASSWORD_SBAGLIATA).is_none());
        // Una password sbagliata: il dialogo torna, vuoto, con «Password sbagliata».
        scrivi(&mut h, "kilimangiaro9");
        h.get_by_value(&"\u{2022}".repeat(13));
        h.key_press(egui::Key::Enter);
        aspetta_la_password(&mut h);
        aspetta_la_password(&mut h);
        h.get_by_label(schermo_pdf::PASSWORD_SBAGLIATA);
        assert_eq!(h.state().schermata, Schermata::Home);
        // Riscrivendo l'avviso se ne va; l'occhio mostra le lettere.
        scrivi(&mut h, "Kilimangiaro9");
        assert!(h.query_by_label(schermo_pdf::PASSWORD_SBAGLIATA).is_none());
        tocca(&mut h, tascabile_touch::MOSTRA);
        assert!(h.state().password_pdf.as_ref().unwrap().visibile);
        // Nell'accessibilità restano i punti.
        h.get_by_value(&"\u{2022}".repeat(13));
        tocca(&mut h, schermo_pdf::APRI);
        // La tastiera non tiene più la password.
        assert_eq!(h.state().piattaforma.ultimo_stato().map(|s| s.0), Some(String::new()));
        aspetta_il_lettore(&mut h);
        assert!(h.state().password_pdf.is_none());
        assert_eq!(h.state().pdf.as_ref().unwrap().lettore.pagine(), 1);
        assert_eq!(h.state().piattaforma.tipo_della_tastiera(), TipoTastiera::Testo);
        assert!(!h.state().piattaforma.keyboard_visible());
        let d = diagnosi_della_prova();
        for riga in [
            ", tipo pdf, protetto da password, si chiede\n",
            ", tipo pdf, password sbagliata, si chiede di nuovo\n",
            "] pdf: password data, si riapre\n",
            ", 1 pagine, con la password\n",
            "] tastiera: finestra v",
        ] {
            assert!(d.contains(riga), "manca «{riga}»:\n{d}");
        }
        // Mai la password, nemmeno quanto è lunga; mai il nome del file.
        for parola in ["Kilimangiaro", "kilimangiaro", "13 unità", "Zanzibar", "Referto"] {
            assert!(!d.contains(parola), "«{parola}» nella diagnosi:\n{d}");
        }
        assert!(d.lines().filter(|r| r.contains("campo della password")).all(|r| !r.contains("unità") && !r.contains("sel ")), "{d}");
        // Né nei file dell'app.
        for file in [home::FILE_RECENTI, schermo_pdf::FILE_POSIZIONI] {
            if let Ok(scritto) = std::fs::read_to_string(dir.join("app").join(file)) {
                assert!(!scritto.contains("Kilimangiaro"), "{file}: {scritto}");
            }
        }
    }

    #[test]
    fn annulla_e_indietro_lasciano_il_pdf_con_password_chiuso() {
        let (piattaforma, dir) = piattaforma_di_prova("pdf-password-annulla");
        let percorso = dir.join("radice/Referto.pdf");
        std::fs::write(&percorso, tascabile_pdf::prova::cifrato("Kilimangiaro9")).unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        h.state_mut().apri_file(&percorso, "Referto.pdf");
        aspetta_la_password(&mut h);
        // Indietro chiude prima la tastiera, poi il dialogo.
        indietro(&mut h);
        assert!(!h.state().piattaforma.keyboard_visible());
        assert!(h.state().password_pdf.is_some());
        indietro(&mut h);
        assert!(h.state().password_pdf.is_none());
        assert_eq!(h.state().schermata, Schermata::Home);
        // Di nuovo, con qualche lettera e Annulla: la tastiera non le tiene.
        h.state_mut().apri_file(&percorso, "Referto.pdf");
        aspetta_la_password(&mut h);
        scrivi(&mut h, "Kili");
        tocca(&mut h, schermo_pdf::ANNULLA);
        assert_eq!(h.state().piattaforma.ultimo_stato().map(|s| s.0), Some(String::new()));
        assert!(h.state().password_pdf.is_none() && h.state().pdf.is_none());
        assert_eq!(h.state().fuoco, Fuoco::Documento);
        assert_eq!(h.state().piattaforma.tipo_della_tastiera(), TipoTastiera::Testo);
        assert!(diagnosi_della_prova().contains("] pdf: password non data, il PDF non si apre\n"));
        // Un altro documento mentre il dialogo è aperto lo chiude.
        h.state_mut().apri_file(&percorso, "Referto.pdf");
        aspetta_la_password(&mut h);
        let altro = dir.join("radice/Altro.pdf");
        std::fs::write(&altro, tascabile_pdf::prova::pdf(&[tascabile_pdf::prova::Pagina::bianca(595.0, 842.0)])).unwrap();
        h.state_mut().apri_file(&altro, "Altro.pdf");
        assert!(h.state().password_pdf.is_none());
        aspetta_il_lettore(&mut h);
        assert_eq!(h.state().pdf.as_ref().unwrap().nome, "Altro.pdf");
    }

    /// La rete simulata per il timestamp: ogni impronta mandata ha un'attesa verso Alice, le
    /// attese non sono ancora pronte (404), gli esploratori danno il blocco 358391 dell'esempio
    /// pubblico; il resto non si collega.
    fn rete_dei_server(p: &Piattaforma) {
        use tascabile_platform::RispostaRete;
        use tascabile_prova_data::prova::{BLOCCO_358391, HASH_358391, risposta_in_attesa};
        let attesa = risposta_in_attesa("https://alice.btc.calendar.opentimestamps.org");
        p.simula_rete(move |indirizzo, corpo| {
            if corpo.is_some() && indirizzo.ends_with("/digest") {
                RispostaRete::Ok(attesa.clone())
            } else if indirizzo.contains("/timestamp/") {
                RispostaRete::Stato(404)
            } else if indirizzo.ends_with("/block-height/358391") {
                RispostaRete::Ok(HASH_358391.as_bytes().to_vec())
            } else if indirizzo.ends_with(&format!("/block/{HASH_358391}/header")) {
                RispostaRete::Ok(BLOCCO_358391.as_bytes().to_vec())
            } else {
                RispostaRete::Collegamento
            }
        });
    }

    /// Passi finché il lavoro della data non è finito, poi il foglio che sale.
    fn aspetta_la_data(h: &mut Telefono) {
        finche(h, "la data", |h| !h.state().data_in_corso());
        h.run_steps(12);
    }

    /// Il timestamp di un PDF (piano 2026-10-28-fissa-la-data, F4; i nomi del 28/10 sera): il
    /// foglio spiega, la prova va accanto al file e ha la sua impronta; poi il menu dice «Controlla
    /// il timestamp», e la conferma non è ancora arrivata. Nella diagnosi né il nome né gli indirizzi.
    #[test]
    fn fissare_la_data_di_un_pdf_e_poi_controllarla() {
        let (piattaforma, dir) = piattaforma_di_prova("data-pdf");
        rete_dei_server(&piattaforma);
        let percorso = dir.join("radice/Contratto Zanzibar.pdf");
        let byte = tascabile_pdf::prova::pdf(&[tascabile_pdf::prova::Pagina::bianca(595.0, 842.0)]);
        std::fs::write(&percorso, &byte).unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        h.state_mut().apri_file(&percorso, "Contratto Zanzibar.pdf");
        aspetta_il_lettore(&mut h);
        tocca(&mut h, schermo_pdf::MENU);
        h.run_steps(2);
        tocca(&mut h, fissa_data::TIMESTAMP_CON_BLOCKCHAIN);
        h.run_steps(12);
        h.get_by_label(fissa_data::SPIEGAZIONE[1]);
        assert!(!dir.join("radice/Contratto Zanzibar.pdf.ots").exists(), "la prova prima del sì");
        tocca(&mut h, fissa_data::CREA_IL_TIMESTAMP);
        aspetta_la_data(&mut h);
        h.get_by_label(fissa_data::TIMESTAMP_CREATO);
        h.get_by_label_contains("Contratto Zanzibar.pdf.ots, accanto al file");
        let prova = tascabile_prova_data::Prova::leggi(&std::fs::read(dir.join("radice/Contratto Zanzibar.pdf.ots")).expect("la prova")).unwrap();
        assert_eq!(prova.impronta(), tascabile_prova_data::Prova::nuova(&byte, [0; 16]).0.impronta());
        assert_eq!(prova.attese().len(), 1);
        // Adesso il menu controlla: la conferma non c'è ancora.
        indietro(&mut h);
        h.run_steps(12);
        tocca(&mut h, schermo_pdf::MENU);
        h.run_steps(2);
        tocca(&mut h, fissa_data::CONTROLLA_IL_TIMESTAMP);
        tocca(&mut h, fissa_data::CONTROLLA);
        aspetta_la_data(&mut h);
        h.get_by_label(fissa_data::IN_ATTESA);
        let d = diagnosi_della_prova();
        // Le righe «rete:» le scrive il thread di lavoro: le prova la piattaforma (`riga_rete`).
        for riga in ["] data: fissata\n", "] data: in attesa\n"] {
            assert!(d.contains(riga), "manca «{riga}»:\n{d}");
        }
        assert!(!d.contains("Zanzibar") && !d.contains("opentimestamps") && !d.contains("/digest"), "{d}");
    }

    /// Un file con la sua prova accanto, già confermata: Word dice da quando esiste, nell'ora
    /// italiana (l'esempio pubblico, «Hello World!», nel blocco del 28/05/2015 alle 15:41 UTC).
    #[test]
    fn la_data_confermata_dice_da_quando_esiste_il_file() {
        let (piattaforma, dir) = piattaforma_di_prova("data-word");
        rete_dei_server(&piattaforma);
        let percorso = dir.join("radice/hello-world.txt");
        std::fs::write(&percorso, tascabile_prova_data::prova::ESEMPIO_TXT).unwrap();
        std::fs::write(dir.join("radice/hello-world.txt.ots"), tascabile_prova_data::prova::ESEMPIO_OTS).unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        h.state_mut().apri_file(&percorso, "hello-world.txt");
        h.run_steps(4);
        assert_eq!(h.state().schermata, Schermata::Editor);
        tocca(&mut h, schermo_editor::MENU);
        h.run_steps(2);
        tocca(&mut h, fissa_data::CONTROLLA_IL_TIMESTAMP);
        tocca(&mut h, fissa_data::CONTROLLA);
        aspetta_la_data(&mut h);
        h.get_by_label(fissa_data::TIMESTAMP_CONFERMATO);
        h.get_by_label_contains("esisteva già così com'è il 28/05/2015 alle 17:41");
        assert!(diagnosi_della_prova().contains("] data: confermata nel blocco 358391\n"));
    }

    /// Roadmap della sicurezza, livello 1: la rete parte chiusa e si apre solo per un'operazione a
    /// cui si è appena dato il consenso; «No» non va in rete; finito il controllo si richiude; con
    /// la rete per il timestamp spenta nelle Impostazioni non si va mai.
    #[test]
    fn la_rete_si_apre_solo_col_consenso() {
        let (piattaforma, dir) = piattaforma_di_prova("rete-consenso");
        rete_dei_server(&piattaforma);
        let percorso = dir.join("radice/hello-world.txt");
        std::fs::write(&percorso, tascabile_prova_data::prova::ESEMPIO_TXT).unwrap();
        std::fs::write(dir.join("radice/hello-world.txt.ots"), tascabile_prova_data::prova::ESEMPIO_OTS).unwrap();
        use tascabile_platform::RispostaRete;
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        let chiusa = |h: &Telefono| matches!(h.state().piattaforma.rete().chiedi("https://blockstream.info/api/blocks/tip/height", None), RispostaRete::Collegamento);
        assert!(chiusa(&h), "la rete deve partire chiusa");
        h.state_mut().apri_file(&percorso, "hello-world.txt");
        h.run_steps(4);
        tocca(&mut h, schermo_editor::MENU);
        h.run_steps(2);
        tocca(&mut h, fissa_data::CONTROLLA_IL_TIMESTAMP);
        h.get_by_label(fissa_data::CONTROLLARE);
        tocca(&mut h, fissa_data::NO);
        h.run_steps(4);
        assert!(h.query_by_label(fissa_data::CONTROLLARE).is_none() && chiusa(&h));
        tocca(&mut h, schermo_editor::MENU);
        h.run_steps(2);
        tocca(&mut h, fissa_data::CONTROLLA_IL_TIMESTAMP);
        tocca(&mut h, fissa_data::CONTROLLA);
        aspetta_la_data(&mut h);
        h.get_by_label(fissa_data::TIMESTAMP_CONFERMATO);
        assert!(chiusa(&h), "finito il controllo la rete si richiude");
        // Spenta nelle Impostazioni: non si chiede nemmeno.
        indietro(&mut h);
        indietro(&mut h);
        assert_eq!(h.state().schermata, Schermata::Home);
        dalle_impostazioni(&mut h, impostazioni::RETE_TIMESTAMP);
        assert!(!h.state().impostazioni.rete_timestamp);
        indietro(&mut h);
        h.state_mut().apri_file(&percorso, "hello-world.txt");
        h.run_steps(4);
        tocca(&mut h, schermo_editor::MENU);
        h.run_steps(2);
        tocca(&mut h, fissa_data::CONTROLLA_IL_TIMESTAMP);
        h.get_by_label(fissa_data::RETE_SPENTA);
        assert!(h.query_by_label(fissa_data::CONTROLLARE).is_none());
        let d = diagnosi_della_prova();
        for riga in ["] data: niente rete, controllo lasciato\n", "] data: consenso alla rete per il controllo\n", "] data: rete spenta nelle Impostazioni\n", "] rete: chiusa, nessun consenso\n"] {
            assert!(d.contains(riga), "manca «{riga}»:\n{d}");
        }
    }

    /// Senza internet il timestamp non si fa e non resta una prova a metà; un documento nuovo, mai
    /// salvato, non ha un file di cui fare il timestamp.
    #[test]
    fn senza_internet_o_senza_file_la_data_non_si_fissa() {
        let mut h = telefono_col_file("data-slide", "Lezione Zanzibar.pptx", tascabile_slide::prova::LEZIONE);
        tocca(&mut h, schermo_slide::MENU);
        h.run_steps(2);
        tocca(&mut h, fissa_data::TIMESTAMP_CON_BLOCKCHAIN);
        h.run_steps(12);
        tocca(&mut h, fissa_data::CREA_IL_TIMESTAMP);
        aspetta_la_data(&mut h);
        h.get_by_label(fissa_data::SENZA_RETE);
        h.get_by_label_contains("Hanno risposto 0 server su 4");
        let radice = h.state().piattaforma.radice_archivio();
        assert!(!radice.join("Lezione Zanzibar.pptx.ots").exists());
        assert!(diagnosi_della_prova().contains("] data: non riuscita (0 server su 4 hanno risposto)\n"));
        let mut h = telefono(app_di_prova("data-nuovo"));
        nuovo_dalla_home(&mut h);
        tocca(&mut h, schermo_editor::MENU);
        h.run_steps(2);
        tocca(&mut h, fissa_data::TIMESTAMP_CON_BLOCKCHAIN);
        h.run_steps(2);
        h.get_by_label("Salva prima il documento: il timestamp si fa su un file.");
        assert!(!h.state().data_in_corso());
    }

    /// Esporta in Word di una scansione riconosciuta: un .docx coi paragrafi del testo, da
    /// modificare. Prima le parole dell'OCR andavano in caselle sopra l'immagine e si leggevano a
    /// pezzi («ci / tt / à»; analisi del 27/10, pagine I6).
    #[test]
    fn esportare_in_word_una_scansione_riconosciuta_da_il_testo_a_paragrafi() {
        let (piattaforma, dir) = piattaforma_di_prova("pdf-scansione-word");
        let percorso = dir.join("radice/Verbale Zanzibar.pdf");
        std::fs::write(&percorso, tascabile_pdf::prova::scansione_riconosciuta()).unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        h.state_mut().apri_file(&percorso, "Verbale Zanzibar.pdf");
        aspetta_il_lettore(&mut h);
        let ctx = h.ctx.clone();
        h.state_mut().esporta_word_pdf(&ctx);
        for _ in 0..1000 {
            if h.state().word.is_none() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
            h.run_steps(1);
        }
        h.run_steps(4);
        h.get_by_label_contains("solo il testo riconosciuto");
        let docx = std::fs::read(dir.join("radice/Documents/Tascabile/Verbale Zanzibar.docx")).expect("il .docx");
        let s = tascabile_word::documento::apri_byte("Verbale Zanzibar.docx", &docx).expect("si apre");
        let testo = s.doc.plain_text(wordcraft_engine::doc::StoryRef::Body);
        for riga in tascabile_pdf::prova::RIGHE {
            assert!(testo.contains(riga), "manca «{riga}» in {testo:?}");
        }
        let d = diagnosi_della_prova();
        assert!(d.contains("] pdf: word esportato dal testo riconosciuto, "), "{d}");
        assert!(!d.contains("Zanzibar") && !d.contains("città"), "{d}");
    }

    #[test]
    fn esportare_in_word_un_pdf_con_password_avvisa_che_il_docx_non_la_chiede() {
        // Analisi del 27/10, critico 3: il .docx esce in chiaro (è voluto), ma va detto.
        let (piattaforma, dir) = piattaforma_di_prova("pdf-password-word");
        let percorso = dir.join("radice/Referto.pdf");
        std::fs::write(&percorso, tascabile_pdf::prova::cifrato("Kilimangiaro9")).unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        h.state_mut().apri_file(&percorso, "Referto.pdf");
        aspetta_la_password(&mut h);
        scrivi(&mut h, "Kilimangiaro9");
        tocca(&mut h, schermo_pdf::APRI);
        aspetta_il_lettore(&mut h);
        let ctx = h.ctx.clone();
        h.state_mut().esporta_word_pdf(&ctx);
        for _ in 0..1000 {
            if h.state().word.is_none() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
            h.run_steps(1);
        }
        h.run_steps(4);
        h.get_by_label_contains("senza la password del PDF");
    }

    #[test]
    fn ogni_pdf_riprende_dal_punto_e_dallo_zoom_dell_ultima_volta() {
        let (piattaforma, dir) = piattaforma_di_prova("pdf-ripresa");
        let byte = tascabile_pdf::prova::pdf(&[tascabile_pdf::prova::Pagina::bianca(595.0, 842.0); 6]);
        let percorso = dir.join("radice/Turni Zanzibar.pdf");
        std::fs::write(&percorso, &byte).unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        h.state_mut().apri_file(&percorso, "Turni Zanzibar.pdf");
        aspetta_il_lettore(&mut h);
        {
            let l = &mut h.state_mut().pdf.as_mut().unwrap().lettore;
            l.imposta_zoom(1.5);
            l.vai_al_punto_xy(4, 0.6, 200.0);
        }
        h.run_steps(2);
        // Ferma più di un attimo: la posizione si scrive senza aspettare la chiusura, su un thread
        // suo (il file con sync_all sul thread dell'interfaccia poteva fermare un frame).
        std::thread::sleep(schermo_pdf_ricorda_dopo());
        h.run_steps(2);
        assert!(h.state().posizioni_in_scrittura(), "la posizione non si scrive su un thread");
        h.state_mut().aspetta_le_posizioni();
        let file = dir.join("app").join(schermo_pdf::FILE_POSIZIONI);
        let scritte = tascabile_files::Posizioni::carica(&file);
        assert_eq!(scritte.voci().len(), 1);
        assert_eq!((scritte.voci()[0].pagina, scritte.voci()[0].zoom), (4, 1.5));
        assert!((scritte.voci()[0].y - 200.0).abs() < 0.5, "{:?}", scritte.voci()[0]);
        assert!(scritte.voci()[0].x.is_some_and(|x| (x - 0.6).abs() < 0.01), "{:?}", scritte.voci()[0]);
        assert!(!std::fs::read_to_string(&file).unwrap().contains("Zanzibar"));
        // L'app perde il fuoco (un'altra app davanti): si scrive subito.
        h.state_mut().pdf.as_mut().unwrap().lettore.vai_al_punto(2, Some(50.0));
        h.run_steps(2);
        h.input_mut().focused = false;
        h.run_steps(1);
        h.input_mut().focused = true;
        h.run_steps(1);
        h.state_mut().aspetta_le_posizioni();
        assert_eq!(tascabile_files::Posizioni::carica(&file).voci()[0].pagina, 2);
        // Più avanti, poi indietro: alla chiusura si scrive l'ultimo punto.
        h.state_mut().pdf.as_mut().unwrap().lettore.vai_al_punto_xy(5, 0.6, 100.0);
        h.run_steps(2);
        indietro(&mut h);
        assert_eq!(h.state().schermata, Schermata::Home);
        h.state_mut().aspetta_le_posizioni();
        assert_eq!(tascabile_files::Posizioni::carica(&file).voci()[0].pagina, 5);
        // Lo stesso contenuto con un altro nome riprende da lì, allo stesso zoom.
        let copia = dir.join("radice/Copia.pdf");
        std::fs::write(&copia, &byte).unwrap();
        h.state_mut().apri_file(&copia, "Copia.pdf");
        aspetta_il_lettore(&mut h);
        assert_eq!(pagina_corrente(&h), 5);
        assert_eq!(h.state().pdf.as_ref().unwrap().lettore.zoom(), 1.5);
        let x = h.state().pdf.as_ref().unwrap().lettore.posizione_x().unwrap();
        assert!((x - 0.6).abs() < 0.01, "la ripresa non ha rimesso il punto in orizzontale: {x}");
        let d = diagnosi_della_prova();
        assert!(d.contains("] pdf: ripresa dalla pagina 6 di 6, zoom 1,50, chiave "), "{d}");
        assert!(d.contains("] pdf: posizione ricordata, pagina 6 di 6\n"), "{d}");
        assert!(!d.contains("Zanzibar") && !d.contains("Copia"), "{d}");
    }

    /// Quanto deve restare ferma la vista perché la posizione si scriva, più un poco.
    fn schermo_pdf_ricorda_dopo() -> Duration {
        Duration::from_millis(1600)
    }

    #[test]
    fn le_informazioni_sul_documento_dal_menu() {
        let mut h = telefono_col_pdf("pdf-informazioni", tascabile_pdf::prova::modulo());
        tocca(&mut h, schermo_pdf::MENU);
        tocca(&mut h, schermo_pdf::INFORMAZIONI);
        h.run_steps(4);
        for (etichetta, valore) in [
            ("Pagine", "3"),
            ("Titolo", "Consenso informato al trattamento"),
            ("Autore", "Ambulatorio di prova"),
            ("Firme digitali", "Nessuna"),
            ("Cartella", "Memoria del telefono"),
        ] {
            h.get_by_label(etichetta);
            assert!(h.query_all_by_label(valore).count() >= 1, "manca «{valore}»");
        }
        h.get_by_label_contains(" KB");
        let d = diagnosi_della_prova();
        assert!(d.contains("] pdf: informazioni, 3 pagine, "), "{d}");
        for parola in ["Consenso", "Ambulatorio", "Zanzibar"] {
            assert!(!d.contains(parola), "«{parola}» nella diagnosi:\n{d}");
        }
        indietro(&mut h);
        assert!(h.query_by_label("Firme digitali").is_none());
        assert_eq!(h.state().schermata, Schermata::Pdf);
    }

    #[test]
    fn un_link_esterno_chiede_prima_di_aprire_il_browser() {
        use tascabile_pdf::prova::{Bersaglio, Collegamento, Pagina, pdf_completo};
        let link = [
            Collegamento { pagina: 0, rettangolo: [50.0, 700.0, 250.0, 720.0], bersaglio: Bersaglio::Indirizzo("https://esempio.it/Zanzibar") },
            Collegamento { pagina: 0, rettangolo: [50.0, 600.0, 250.0, 620.0], bersaglio: Bersaglio::Indirizzo("javascript:Zanzibar()") },
        ];
        let mut h = telefono_col_pdf("pdf-link", pdf_completo(&[Pagina::bianca(595.0, 842.0); 2], &[], &link));
        // La pagina sotto la barra alta (64 dp), a 16 dp dal lato e 14 dalla barra, 380 / 595 dp per pt.
        let k = 380.0 / 595.0;
        let punto = |y_pt: f32| egui::pos2(16.0 + 150.0 * k, 64.0 + 14.0 + y_pt * k);
        tocca_in(&mut h, punto(132.0));
        h.run_steps(2);
        h.get_by_label(schermo_pdf::APRIRE_IL_LINK);
        // Annulla: niente.
        tocca(&mut h, schermo_pdf::ANNULLA);
        assert!(h.query_by_label(schermo_pdf::APRIRE_IL_LINK).is_none());
        assert!(h.state().piattaforma.indirizzi_aperti().is_empty());
        h.run_steps(2);
        tocca_in(&mut h, punto(132.0));
        h.run_steps(2);
        tocca(&mut h, schermo_pdf::APRI);
        assert_eq!(h.state().piattaforma.indirizzi_aperti(), ["https://esempio.it/Zanzibar"]);
        h.run_steps(2);
        // Un indirizzo che non è una pagina web né una mail non si apre.
        tocca_in(&mut h, punto(232.0));
        h.run_steps(2);
        assert!(h.state().errore.as_ref().is_some_and(|e| e.testo.contains("solo pagine web")));
        assert_eq!(h.state().piattaforma.indirizzi_aperti().len(), 1);
        let d = diagnosi_della_prova();
        assert!(
            d.contains(
                "] pdf: link esterno toccato (pagina web), si chiede
"
            ),
            "{d}"
        );
        assert!(
            d.contains(
                "] pdf: link esterno non aperto
"
            ),
            "{d}"
        );
        assert!(
            d.contains(
                "] link esterno: pagina web aperta fuori dall'app
"
            ),
            "{d}"
        );
        assert!(
            d.contains(
                "] pdf: link esterno che non si apre
"
            ),
            "{d}"
        );
        for parola in ["esempio", "Zanzibar", "javascript", "https"] {
            assert!(!d.contains(parola), "«{parola}» nella diagnosi:\n{d}");
        }
    }

    #[test]
    fn errori_di_apertura_senza_nome_del_file() {
        let (piattaforma, dir) = piattaforma_di_prova("errori");
        let rotto = dir.join("Segreto Zanzibar.docx");
        std::fs::write(&rotto, [0u8, 1, 2, 3, 255, 254].repeat(111)).unwrap(); // 666 byte
        let mut app = TascabileApp::new(piattaforma, Instant::now());
        let ctx = egui::Context::default();
        app.piattaforma.simula_apri_con(rotto.to_str().unwrap(), None);
        passo(&mut app, &ctx, vec![]);
        assert!(app.errore.is_some(), "il file rotto si è aperto");
        assert_eq!(app.schermata, Schermata::Home);
        app.piattaforma.simula_apri_con(dir.join("Mancante Zanzibar.docx").to_str().unwrap(), None);
        passo(&mut app, &ctx, vec![]);
        // Due errori di lettura diversi lasciano righe diverse: il genere dell'errore sì, il
        // messaggio (che ha il percorso) no.
        let cartella = dir.join("Cartella Zanzibar.odt");
        std::fs::create_dir_all(&cartella).unwrap();
        app.piattaforma.simula_apri_con(cartella.to_str().unwrap(), None);
        passo(&mut app, &ctx, vec![]);
        let d = diagnosi_della_prova();
        assert!(d.contains("] apri con: richiesta, tipo non indicato\n"), "{d}");
        assert!(d.contains("] apri con: 666 byte, tipo docx, errore di apertura (errore del lettore)\n"), "{d}");
        assert!(d.contains("] apri con: errore di lettura (NotFound), tipo docx\n"), "{d}");
        assert!(d.contains("] apri con: errore di lettura (IsADirectory), tipo odt\n"), "{d}");
        for parola in ["Zanzibar", "Segreto", "Mancante", "Cartella", "prove-app"] {
            assert!(!d.contains(parola), "«{parola}» nella diagnosi:\n{d}");
        }
        // Un file che non si apre non va nei recenti.
        assert!(app.home.recenti().is_empty());
    }

    #[test]
    fn l_avvio_dice_se_c_e_l_accesso_ai_file() {
        let _app = app_di_prova("avvio");
        assert!(diagnosi_della_prova().contains("] accesso ai file all'avvio: concesso\n"));
        // Anche in testa, dove resta dopo che il giro delle righe l'ha fatta uscire.
        let testa = testa_della_diagnosi();
        assert!(testa.contains("] accesso ai file all'avvio: concesso\n"), "{testa}");
    }

    /// Un passo di egui all'istante `t` (secondi), solo la logica.
    fn passo_alle(app: &mut TascabileApp, ctx: &egui::Context, t: f64, eventi: Vec<egui::Event>) {
        let mut out = ctx.run_ui(egui::RawInput { time: Some(t), events: eventi, ..Default::default() }, |ui| app.passo_logica(ui.ctx()));
        out.textures_delta.clear();
    }

    fn dito(pos: egui::Pos2, giu: bool) -> egui::Event {
        egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: giu, modifiers: egui::Modifiers::NONE }
    }

    #[test]
    fn ogni_dito_che_si_alza_lascia_una_riga() {
        // Anche i tocchi che egui non prende per un clic, e che l'editor non vede: qui il dito
        // scivola di 7 dp (egui ne tollera 6) e resta giù 0,6 s (l'app ne tollera 0,5).
        let mut app = app_di_prova("dita");
        let ctx = egui::Context::default();
        let (giu, scivolato) = (egui::pos2(123.0, 456.0), egui::pos2(130.0, 456.0));
        passo_alle(&mut app, &ctx, 10.0, vec![egui::Event::PointerMoved(giu), dito(giu, true)]);
        passo_alle(&mut app, &ctx, 10.3, vec![egui::Event::PointerMoved(scivolato)]);
        passo_alle(&mut app, &ctx, 10.6, vec![dito(giu, false), egui::Event::PointerGone]);
        // Finestra nascosta: eframe chiama la logica con l'input dell'ultimo passo, che non va
        // contato di nuovo.
        for _ in 0..3 {
            let _ = ctx.run_logic(&egui::RawInput::default(), |ctx| app.passo_logica(ctx));
        }
        // Un tocco che il sistema annulla (gesto di sistema): il dito sparisce senza alzarsi.
        let altro = egui::pos2(321.0, 654.0);
        passo_alle(&mut app, &ctx, 20.0, vec![egui::Event::PointerMoved(altro), dito(altro, true)]);
        passo_alle(&mut app, &ctx, 20.25, vec![egui::Event::PointerGone]);
        let d = diagnosi_della_prova();
        let righe: Vec<&str> = d.lines().filter(|r| r.contains("] dito: x 123 dp, y 456 dp") || r.contains("] dito: x 321 dp")).collect();
        assert_eq!(righe.len(), 2, "{d}");
        assert!(righe[0].ends_with("] dito: x 123 dp, y 456 dp, spostato fino a 7 dp, per 0.60 s"), "{d}");
        assert!(righe[1].ends_with("] dito: x 321 dp, y 654 dp, annullato dopo 0.25 s"), "{d}");
    }

    #[test]
    fn tocco_sul_testo_prima_il_dito_poi_il_cursore() {
        // In un frame vero la riga del dito viene dalla logica, quella del tocco sul testo dal
        // disegno, dopo: chi legge vede prima dove si è appoggiato il dito, poi dove è finito il
        // cursore. In questo crate è l'unica prova che tocca la pagina.
        let mut h = telefono_con_la_checklist("tocco");
        let p = egui::pos2(211.0, 517.0);
        h.event(egui::Event::PointerMoved(p));
        h.event(dito(p, true));
        h.step();
        h.event(dito(p, false));
        h.event(egui::Event::PointerGone);
        h.step();
        let d = diagnosi_della_prova();
        let (_, dopo) = d.split_once("] dito: x 211 dp, y 517 dp, spostato fino a 0 dp, per 0.25 s\n").expect("manca la riga del dito");
        assert!(dopo.contains("] tocco: x "), "{d}");
    }

    #[test]
    fn diagnosi_senza_il_testo_battuto_nell_app() {
        let mut app = app_che_scrive("testo-battuto");
        let ctx = egui::Context::default();
        passo(&mut app, &ctx, vec![egui::Event::Text("Zanzibar".into())]);
        passo(&mut app, &ctx, vec![tasto(egui::Key::Enter), egui::Event::Text("Kilimangiaro".into())]);
        assert_eq!(testi(&app), ["Zanzibar", "Kilimangiaro"]);
        let d = diagnosi_della_prova();
        assert!(d.contains("] tastiera: richiesta di aprirla\n"), "{d}");
        for parola in ["Zanzibar", "Kilimangiaro"] {
            assert!(!d.contains(parola), "«{parola}» nella diagnosi:\n{d}");
        }
    }

    #[test]
    fn testata_con_tutti_e_due_gli_avvisi() {
        let mut app = app_di_prova("avvisi");
        app.cambia_documento(Editor::new(documento::nuovo(), "prova"));
        app.errore = Some(Errore { titolo: NON_SI_APRE, testo: "Non riesco ad aprire il file: prova".into() });
        app.editor_mut().unwrap().ponte.avviso = Some("Paragrafo troppo lungo: prova".into());
        let h = telefono(app);
        // L'errore nel dialogo al centro, l'avviso del ponte sotto la barra alta: non si coprono.
        h.get_by_label(NON_SI_APRE);
        let a = h.get_by_label("Non riesco ad aprire il file: prova").rect();
        let b = h.get_by_label("Paragrafo troppo lungo: prova").rect();
        assert!(!a.intersects(b) && b.min.y >= tema::BARRA_ALTA - 0.5, "avvisi sovrapposti: {a:?}, {b:?}");
    }

    // Task 3: Home, Editor e Diagnosi.

    #[test]
    fn si_parte_dalla_home() {
        let h = telefono(app_di_prova("parte-dalla-home"));
        assert_eq!(h.state().schermata, Schermata::Home);
        assert!(h.state().editor().is_none());
        h.get_by_label("Doc Tascabile");
        h.get_by_label("Recenti");
        h.get_by_value(home::CERCA);
        for chip in ["Tutti", "PDF", "Documenti", "Slide"] {
            assert!(h.get_by_role_and_label(Role::Button, chip).rect().height() >= tema::CHIP, "{chip}");
        }
        for (b, alto) in [("Nuovo", tema::NUOVO), ("Impostazioni", tema::PULSANTE)] {
            let r = h.get_by_role_and_label(Role::Button, b).rect();
            assert!(r.height() >= alto, "«{b}» alto {} dp", r.height());
        }
        // Le prove e le misure non stanno più nella Home: sono nelle Impostazioni e nella diagnosi.
        for via in ["Checklist tastiera", "Checklist della tastiera", "Documento lungo", "Diagnosi"] {
            assert!(h.query_by_label(via).is_none(), "«{via}» nella Home");
        }
        assert!(h.query_by_label_contains("avvio").is_none() && h.query_by_label_contains("ms").is_none());
        // Con l'accesso ai file niente riquadro.
        assert!(h.query_by_label("Concedi").is_none());
    }

    #[test]
    fn il_tema_e_installato_e_il_primo_disegno_e_nella_diagnosi() {
        let h = telefono(app_di_prova("tema"));
        let ctx = h.ctx.clone();
        assert_eq!(ctx.theme(), egui::Theme::Light);
        assert_eq!(ctx.global_style().visuals.panel_fill, tema::CARTA);
        assert!(Carattere::Titolo.pronto(&ctx) && Carattere::Icona.pronto(&ctx));
        assert!(preparato(&ctx));
        let testa = testa_della_diagnosi();
        assert!(testa.contains("] avvio: primo disegno dopo "), "{testa}");
    }

    #[test]
    fn home_con_i_recenti() {
        let (piattaforma, d) = piattaforma_di_prova("app-con-recenti");
        std::fs::create_dir_all(d.join("radice/Download")).unwrap();
        let mut finti = Recenti::default();
        let recente = |chiave: String, nome: &str| tascabile_files::Recente {
            chiave,
            nome: nome.into(),
            tipo: tascabile_files::Tipo::Documento,
            aperto_alle: 1_792_738_800,
            pagine: None,
        };
        finti.aggiungi(recente(d.join("radice/Download/Verbale di ottobre.docx").to_string_lossy().into_owned(), "Verbale di ottobre.docx"));
        finti.aggiungi(recente("content://com.whatsapp.provider.media/item/123".into(), "documento.docx"));
        finti.salva(&d.join("app").join(home::FILE_RECENTI)).unwrap();
        let h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        let whatsapp = h.get_by_role_and_label(Role::Button, "documento").rect();
        let verbale = h.get_by_role_and_label(Role::Button, "Verbale di ottobre").rect();
        // Il più recente in cima, voci da 72 dp col loro menu.
        assert!(whatsapp.max.y <= verbale.min.y, "{whatsapp:?}, {verbale:?}");
        assert!(whatsapp.height() >= tema::VOCE && verbale.height() >= tema::VOCE);
        for nome in ["documento", "Verbale di ottobre"] {
            let menu = h.get_by_role_and_label(Role::Button, &home::etichetta_menu(nome)).rect();
            assert!(menu.width() >= 48.0 && menu.height() >= 48.0, "{nome}: {menu:?}");
        }
        assert!(h.query_by_label("Qui compaiono i documenti aperti di recente.").is_none());
        // Due recenti: niente «Vedi tutti».
        assert!(h.query_by_label("Vedi tutti").is_none());
    }

    /// Un telefono con `n` recenti finti (documenti in Download, che non ci sono) e, in Download,
    /// i file `nomi`.
    fn telefono_con_recenti(prova: &str, n: usize, nomi: &[&str]) -> (Telefono, PathBuf) {
        let (piattaforma, d) = piattaforma_di_prova(prova);
        let download = d.join("radice/Download");
        std::fs::create_dir_all(&download).unwrap();
        for nome in nomi {
            if nome.ends_with(".docx") {
                docx(&download.join(nome), &["Verbale"]);
            } else {
                std::fs::write(download.join(nome), b"x").unwrap();
            }
        }
        let mut finti = Recenti::default();
        for i in (0..n).rev() {
            finti.aggiungi(tascabile_files::Recente {
                chiave: download.join(format!("Recente {i}.docx")).to_string_lossy().into_owned(),
                nome: format!("Recente {i}.docx"),
                tipo: tascabile_files::Tipo::Documento,
                aperto_alle: adesso(),
                pagine: None,
            });
        }
        finti.salva(&d.join("app").join(home::FILE_RECENTI)).unwrap();
        (telefono(TascabileApp::new(piattaforma, Instant::now())), d)
    }

    #[test]
    fn vedi_tutti_mostra_tutti_i_recenti() {
        let (mut h, _) = telefono_con_recenti("vedi-tutti", 6, &[]);
        let visti = |h: &Telefono| (0..6).filter(|i| h.query_by_role_and_label(Role::Button, &format!("Recente {i}")).is_some()).count();
        assert_eq!(visti(&h), home::RECENTI_IN_HOME);
        tocca(&mut h, "Vedi tutti");
        assert_eq!(visti(&h), 6);
        tocca(&mut h, "Vedi meno");
        assert_eq!(visti(&h), home::RECENTI_IN_HOME);
    }

    #[test]
    fn la_ricerca_della_home_prende_la_tastiera_e_filtra() {
        let (mut h, d) = telefono_con_recenti("ricerca-home", 3, &["Verbale Kilimangiaro.docx", "Bolletta.pdf", "Lezione.pptx"]);
        h.get_by_value(home::CERCA).click();
        h.run_steps(2);
        assert_eq!(h.state().fuoco, Fuoco::Campo);
        assert!(h.state().piattaforma.keyboard_visible(), "la tastiera si apre per la ricerca");
        // Il «Nuovo» non copre i risultati con la tastiera aperta.
        assert!(h.query_by_role_and_label(Role::Button, "Nuovo").is_none());
        scrivi(&mut h, "kilim");
        assert_eq!(h.state().home.ricerca().testo(), "kilim");
        h.get_by_role_and_label(Role::Button, "Verbale Kilimangiaro");
        assert!(h.query_by_role_and_label(Role::Button, "Recente 0").is_none());
        assert!(h.query_by_role_and_label(Role::Button, "Bolletta").is_none());
        // Due parole, in un altro ordine e con le maiuscole: «recente 1» trova solo quel recente.
        h.get_by_label("Svuota la ricerca").click();
        h.run_steps(2);
        assert_eq!(h.state().home.ricerca().testo(), "");
        scrivi(&mut h, "1 RECENTE");
        h.get_by_role_and_label(Role::Button, "Recente 1");
        assert!(h.query_by_role_and_label(Role::Button, "Recente 0").is_none());
        // Invio è «fatto»: la tastiera si chiude, quello che si è trovato resta.
        h.key_press(egui::Key::Enter);
        h.run_steps(2);
        assert!(!h.state().piattaforma.keyboard_visible());
        assert_eq!(h.state().fuoco, Fuoco::Documento);
        h.get_by_role_and_label(Role::Button, "Recente 1");
        // Indietro svuota la ricerca.
        indietro(&mut h);
        assert_eq!(h.state().home.ricerca().testo(), "");
        assert_eq!(h.state().schermata, Schermata::Home);
        assert_eq!(h.state().piattaforma.volte_in_secondo_piano(), 0);
        // Il chip PDF: i PDF delle cartelle, che si aprono (dalla tappa D).
        tocca(&mut h, "PDF");
        let pdf = h.get_by_role_and_label(Role::Button, "Bolletta");
        assert!(!pdf.accesskit_node().is_disabled());
        assert!(h.query_by_role_and_label(Role::Button, "Lezione").is_none());
        assert!(h.query_by_role_and_label(Role::Button, "Recente 0").is_none());
        // Un documento trovato si apre.
        tocca(&mut h, "Documenti");
        tocca(&mut h, "Verbale Kilimangiaro");
        assert_eq!(h.state().schermata, Schermata::Editor);
        let diag = diagnosi_della_prova();
        assert!(diag.contains("] fuoco: ricerca della Home, finestra v"), "{diag}");
        assert!(diag.contains("] tastiera: stato v") && diag.contains(" al campo della ricerca, "), "{diag}");
        for parola in ["kilim", "Kilimangiaro", "RECENTE", "Bolletta"] {
            assert!(!diag.contains(parola), "«{parola}» nella diagnosi:\n{diag}");
        }
        let _ = d;
    }

    #[test]
    fn uno_stato_in_ritardo_della_ricerca_non_entra_nel_documento() {
        let (mut h, _) = telefono_con_recenti("ricerca-in-ritardo", 0, &[]);
        h.get_by_value(home::CERCA).click();
        h.run_steps(2);
        scrivi(&mut h, "cane");
        let v_campo = h.state().home.ricerca().versione();
        let in_ritardo = ImeSnapshot { text: "canex".into(), sel: (5, 5), comp: (-1, -1), version: v_campo };
        // Indietro chiude la tastiera; poi un documento nuovo, col suo ponte aperto.
        indietro(&mut h);
        nuovo_dalla_home(&mut h);
        apri_tastiera(h.state_mut());
        let (_, _, _, v) = h.state().piattaforma.ultimo_stato().unwrap();
        assert!(v > v_campo, "la finestra del documento v{v} deve venire dopo quella del campo v{v_campo}");
        let app = h.state_mut();
        let st = app.editor_mut().unwrap().ricevi(&in_ritardo);
        app.dopo_ponte(st);
        assert_eq!(testi(h.state()), [""], "il testo della ricerca è finito nel documento");
    }

    #[test]
    fn il_menu_di_un_recente_condivide_e_toglie() {
        let (mut h, d) = telefono_con_recenti("menu-recente", 2, &["Recente 0.docx"]);
        tocca(&mut h, &home::etichetta_menu("Recente 0"));
        for voce in ["Apri", "Condividi", "Togli dai recenti"] {
            h.get_by_role_and_label(Role::Button, voce);
        }
        // Indietro chiude il menu e basta.
        indietro(&mut h);
        assert!(h.query_by_role_and_label(Role::Button, "Togli dai recenti").is_none());
        assert_eq!(h.state().schermata, Schermata::Home);
        tocca(&mut h, &home::etichetta_menu("Recente 0"));
        tocca(&mut h, "Condividi");
        let condivisi: Vec<_> = std::fs::read_dir(d.join("cache/condivisi")).unwrap().flatten().collect();
        assert!(condivisi.len() == 1 && condivisi[0].path().join("Recente 0.docx").is_file());
        // Un recente che non c'è più sul disco: l'avviso, e nessun file condiviso.
        tocca(&mut h, &home::etichetta_menu("Recente 1"));
        tocca(&mut h, "Condividi");
        h.get_by_label_contains("Non riesco a condividere il file: ");
        h.get_by_label(NON_CONDIVISO);
        tocca(&mut h, HO_CAPITO);
        tocca(&mut h, &home::etichetta_menu("Recente 1"));
        tocca(&mut h, "Togli dai recenti");
        assert!(h.query_by_role_and_label(Role::Button, "Recente 1").is_none());
        let su_disco = Recenti::carica(&d.join("app").join(home::FILE_RECENTI));
        assert_eq!(su_disco.voci().len(), 1);
        // «Apri» apre.
        tocca(&mut h, &home::etichetta_menu("Recente 0"));
        tocca(&mut h, "Apri");
        assert_eq!(h.state().schermata, Schermata::Editor);
        assert_eq!(testi(h.state()), ["Verbale"]);
        let diag = diagnosi_della_prova();
        let byte = std::fs::metadata(d.join("radice/Download/Recente 0.docx")).unwrap().len();
        assert!(diag.contains(&format!("] condividi dai recenti: riuscito, tipo docx, {byte} byte\n")), "{diag}");
        assert!(diag.contains("] condividi dai recenti: non riuscito (NotFound), tipo docx\n"), "{diag}");
        assert!(diag.contains("] recenti: tolto dal menu\n"), "{diag}");
        assert!(!diag.contains("Recente 0") && !diag.contains("Recente 1"), "{diag}");
    }

    /// «Scansiona» apre la scansione vuota con Fotocamera e Galleria (0.8.3, l'autore: «dai la
    /// possibilità di prendere la foto per lo scan dalla galleria e non farla e basta»). Tornare
    /// indietro dalla fotocamera o dalla galleria senza foto lascia la scansione aperta; Indietro la
    /// chiude.
    #[test]
    fn scansiona_dal_nuovo_fa_scegliere_fotocamera_o_galleria() {
        use tascabile_platform::FonteFoto;
        let mut h = telefono(app_di_prova("scansiona-nuovo"));
        tocca(&mut h, "Nuovo");
        h.run_steps(15);
        tocca(&mut h, "Scansiona");
        h.run_steps(4);
        assert_eq!(h.state().pagine_scansionate(), Some(0));
        assert!(h.state().piattaforma.foto_chieste().is_empty(), "la fotocamera è partita da sola");
        h.get_by_label_contains("scegli dalla galleria le foto che hai già");
        assert!(h.query_by_label(pdf_ocr::CREA_PDF).is_none() && h.query_by_label(pdf_ocr::TOGLI).is_none(), "comandi delle pagine senza pagine");
        tocca(&mut h, pdf_ocr::FOTOCAMERA);
        assert_eq!(h.state().piattaforma.foto_chieste(), [FonteFoto::Fotocamera]);
        h.state().piattaforma.simula_foto_firma(None);
        h.run_steps(4);
        assert_eq!(h.state().pagine_scansionate(), Some(0), "annullata la fotocamera, la scansione resta");
        tocca(&mut h, pdf_ocr::GALLERIA);
        assert_eq!(h.state().piattaforma.foto_chieste(), [FonteFoto::Fotocamera, FonteFoto::GalleriaPiu]);
        h.state().piattaforma.simula_foto_galleria(&[]);
        h.run_steps(4);
        assert_eq!(h.state().pagine_scansionate(), Some(0));
        indietro(&mut h);
        h.run_steps(3);
        assert_eq!(h.state().pagine_scansionate(), None);
        assert_eq!(h.state().schermata, Schermata::Home);
    }

    #[test]
    fn nuovo_apre_il_foglio_col_documento_vuoto_e_i_modelli() {
        let mut h = telefono(app_di_prova("foglio-nuovo"));
        tocca(&mut h, "Nuovo");
        h.run_steps(15);
        for voce in [home::DOCUMENTO_VUOTO, "Verbale", "Relazione", "Lettera", "Lezione", "Scansiona"] {
            h.get_by_role_and_label(Role::Button, voce);
        }
        for no in ["PDF", "Presentazione", "Slide"] {
            assert!(h.query_by_role_and_label(Role::Button, no).is_none_or(|n| n.rect().max.y < 400.0), "«{no}» nel foglio di Nuovo");
        }
        // Indietro chiude il foglio, l'app resta nella Home.
        indietro(&mut h);
        assert!(h.query_by_role_and_label(Role::Button, home::DOCUMENTO_VUOTO).is_none());
        assert_eq!(h.state().piattaforma.volte_in_secondo_piano(), 0);
        nuovo_dalla_home(&mut h);
        assert_eq!(editor(h.state()).nome, "Documento nuovo");
        // Tornando nella Home il foglio è chiuso.
        indietro(&mut h);
        assert_eq!(h.state().schermata, Schermata::Home);
        assert!(h.query_by_role_and_label(Role::Button, home::DOCUMENTO_VUOTO).is_none());
    }

    #[test]
    fn un_modello_si_apre_come_documento_nuovo_col_cursore_nel_primo_campo() {
        let mut h = telefono(app_di_prova("modello"));
        tocca(&mut h, "Nuovo");
        h.run_steps(15);
        tocca(&mut h, "Verbale");
        h.run_steps(4);
        assert_eq!(h.state().schermata, Schermata::Editor);
        let e = editor(h.state());
        assert_eq!(e.nome, "Verbale");
        assert_eq!(e.origine, Origine::Nuovo);
        assert!(!e.s.dirty);
        let p = e.s.doc.para_at(&e.s.sel.focus).unwrap();
        assert_eq!((p.text.as_str(), e.s.sel.focus.off), ("Data: ", 6));
        h.get_by_label("Non ancora salvato");
        assert!(diagnosi_della_prova().contains("] nuovo documento dal modello Verbale\n"));
        // Si scrive nel campo e si salva come documento nuovo, col nome dal titolo.
        apri_tastiera(h.state_mut());
        scrivi(&mut h, "lunedì");
        assert_eq!(editor(h.state()).s.doc.para_at(&editor(h.state()).s.sel.focus).unwrap().text, "Data: lunedì");
    }

    #[test]
    fn l_ortografia_si_carica_col_primo_documento_e_si_spegne_nelle_impostazioni() {
        let (piattaforma, d) = piattaforma_di_prova("ortografia-app");
        // Il dizionario personale della prova, nella cartella privata dell'app.
        std::fs::create_dir_all(d.join("app")).unwrap();
        std::fs::write(d.join("app").join(tascabile_word::ortografia::FILE_PERSONALE), "zanzibarottino\n").unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        assert!(!h.state().dizionario_pronto(), "si carica solo col primo documento Word");
        assert!(diagnosi_della_prova().contains("] ortografia: accesa, 1 parole nel dizionario personale\n"));
        nuovo_dalla_home(&mut h);
        finche(&mut h, "il dizionario pronto", |h| h.state().dizionario_pronto());
        let d_prova = diagnosi_della_prova();
        assert!(d_prova.contains("] ortografia: dizionario pronto, "), "{d_prova}");
        assert!(editor(h.state()).controllo_ortografico);
        assert!(tascabile_word::ortografia::al_cursore(&mut h.state_mut().editor_mut().unwrap().s).is_none());
        // Spento nelle Impostazioni: resta spento, anche per i documenti dopo.
        indietro(&mut h);
        dalle_impostazioni(&mut h, impostazioni::ORTOGRAFIA);
        assert!(!h.state().impostazioni.ortografia);
        assert!(!impostazioni::leggi_ortografia(&d.join("app")));
        assert_eq!(h.get_by_role_and_label(Role::Button, impostazioni::ORTOGRAFIA).accesskit_node().toggled(), Some(egui::accesskit::Toggled::False));
        assert!(diagnosi_della_prova().contains("] impostazioni: ortografia spenta\n"));
        tocca(&mut h, "Indietro");
        nuovo_dalla_home(&mut h);
        assert!(!editor(h.state()).controllo_ortografico);
        // L'autore delle Impostazioni (vuoto) nel documento, mai quello del motore.
        assert_eq!(editor(h.state()).s.author, "");
    }

    #[test]
    fn le_impostazioni_hanno_diagnosi_prove_e_informazioni() {
        let mut h = telefono(app_di_prova("impostazioni"));
        tocca(&mut h, "Impostazioni");
        assert_eq!(h.state().schermata, Schermata::Impostazioni);
        for voce in ["Registro degli eventi", "Condividi la diagnosi", "Checklist della tastiera", "Documento lungo", "Fraunces", "Material Symbols"]
        {
            h.get_by_role_and_label(Role::Button, voce);
        }
        h.get_by_label("Doc Tascabile");
        h.get_by_label(&format!("Versione {VERSIONE}"));
        // Come è fatta l'app: con l'ausilio dell'intelligenza artificiale, che però l'app non usa
        // per funzionare (0.8.4; scelta di trasparenza, non un obbligo dell'AI Act).
        h.get_by_label(guida::SVILUPPO_CON_IA);
        assert!(guida::SVILUPPO_CON_IA.contains("ausilio di intelligenza artificiale") && guida::SVILUPPO_CON_IA.contains("senza rete"));
        assert!(guida::PARTI.iter().any(|(_, testo)| *testo == guida::SVILUPPO_CON_IA), "la Guida rapida non lo dice");
        // Una licenza sale in un foglio; Indietro chiude il foglio, poi torna alla Home. Le
        // licenze stanno in fondo: prima si scorre fin lì.
        h.get_by_role_and_label(Role::Button, "Fraunces").scroll_to_me();
        h.run_steps(15);
        tocca(&mut h, "Fraunces");
        h.run_steps(15);
        h.get_by_label_contains("SIL OPEN FONT LICENSE Version 1.1");
        indietro(&mut h);
        assert!(h.query_by_label_contains("SIL OPEN FONT LICENSE").is_none());
        assert_eq!(h.state().schermata, Schermata::Impostazioni);
        h.get_by_role_and_label(Role::Button, "Condividi la diagnosi").scroll_to_me();
        h.run_steps(15);
        tocca(&mut h, "Condividi la diagnosi");
        assert!(diagnosi_della_prova().contains("] condividi testo: "));
        indietro(&mut h);
        assert_eq!(h.state().schermata, Schermata::Home);
        // Anche con la freccia della barra.
        tocca(&mut h, "Impostazioni");
        tocca(&mut h, "Indietro");
        assert_eq!(h.state().schermata, Schermata::Home);
        // Il documento lungo, dalle prove.
        dalle_impostazioni(&mut h, "Documento lungo");
        assert_eq!(h.state().schermata, Schermata::Editor);
        assert_eq!(editor(h.state()).nome, "Documento lungo");
    }

    /// Arriva la foto di prova di un foglio: «Sistema i bordi» si apre (ritocchi della 0.8.2, R3).
    fn foto_ai_bordi(h: &mut Telefono) {
        h.state().simula_foto_firma_di_prova(Some(&tascabile_pdf::prova::foto_di_un_foglio()));
        finche(h, "i bordi da sistemare", |h| h.state().bordi_aperti() && !h.state().lavoro_in_corso());
        h.run_steps(2);
    }

    /// Dalla Home «Nuovo», «Scansiona» e `n` pagine fotografate (la foto di prova di un foglio), coi
    /// bordi come li ha trovati il motore.
    fn telefono_con_la_scansione(prova: &str, n: usize) -> Telefono {
        scansione_su(telefono(app_di_prova(prova)), n)
    }

    /// `n` pagine fotografate nella scansione aperta da «Nuovo» su questo telefono.
    fn scansione_su(mut h: Telefono, n: usize) -> Telefono {
        tocca(&mut h, "Nuovo");
        h.run_steps(15);
        // In orizzontale «Scansiona» è sotto il bordo: il foglio scorre.
        h.get_by_role_and_label(Role::Button, "Scansiona").scroll_to_me();
        h.run_steps(15);
        tocca(&mut h, "Scansiona");
        h.run_steps(4);
        assert_eq!(h.state().pagine_scansionate(), Some(0), "«Scansiona» non ha aperto la scansione");
        for i in 1..=n {
            tocca(&mut h, pdf_ocr::FOTOCAMERA);
            h.run_steps(3);
            foto_ai_bordi(&mut h);
            tocca(&mut h, pdf_ocr::FATTO);
            finche(&mut h, "la pagina pulita", |h| h.state().pagine_scansionate() == Some(i) && !h.state().lavoro_in_corso());
        }
        h
    }

    /// Un dito che trascina da `da` ad `a`, a piccoli passi.
    fn trascina(h: &mut Telefono, da: egui::Pos2, a: egui::Pos2) {
        h.event(egui::Event::PointerMoved(da));
        h.event(egui::Event::PointerButton { pos: da, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
        h.run_steps(1);
        for i in 1..=10 {
            h.event(egui::Event::PointerMoved(da + (a - da) * (i as f32 / 10.0)));
            h.run_steps(1);
        }
        h.event(egui::Event::PointerButton { pos: a, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
        h.run_steps(2);
    }

    /// Dopo la foto, «Sistema i bordi» (ritocchi della 0.8.2, R3): gli angoli partono da quelli
    /// trovati dal motore, poco dentro il foglio; trascinati col dito si spostano; incrociati, la
    /// spiegazione lo dice e Fatto non va; Tutta la foto fa la pagina grande quanto la foto.
    #[test]
    fn dopo_la_foto_si_sistemano_i_bordi() {
        let mut h = telefono(app_di_prova("scansione-bordi"));
        tocca(&mut h, "Nuovo");
        h.run_steps(15);
        tocca(&mut h, "Scansiona");
        h.run_steps(4);
        tocca(&mut h, pdf_ocr::FOTOCAMERA);
        h.run_steps(3);
        foto_ai_bordi(&mut h);
        h.get_by_label(pdf_ocr::SISTEMA_I_BORDI);
        // Il foglio della foto di prova va da (120, 140) a (1360, 1895): gli angoli poco dentro.
        let angoli = h.state().angoli_dei_bordi().unwrap();
        for (a, v) in angoli.iter().zip([[120.0, 140.0], [1360.0, 140.0], [1360.0, 1895.0], [120.0, 1895.0]]) {
            assert!((a[0] - v[0]).abs() < 25.0 && (a[1] - v[1]).abs() < 25.0, "{angoli:?}");
        }
        // L'angolo in alto a sinistra, trascinato giù e a destra di 40 dp, si sposta di tanto.
        let dove = h.get_by_label(pdf_ocr::ANGOLI[0]).rect().center();
        trascina(&mut h, dove + egui::vec2(6.0, 6.0), dove + egui::vec2(46.0, 46.0));
        let spostati = h.state().angoli_dei_bordi().unwrap();
        assert!(spostati[0][0] > angoli[0][0] + 100.0 && spostati[0][1] > angoli[0][1] + 100.0, "{spostati:?}");
        assert_eq!(spostati[1..], angoli[1..]);
        // Portato oltre l'angolo in alto a destra si incrocia: lo dice, e Fatto non va.
        let dove = h.get_by_label(pdf_ocr::ANGOLI[0]).rect().center();
        let destra = h.get_by_label(pdf_ocr::ANGOLI[1]).rect().center();
        trascina(&mut h, dove, destra + egui::vec2(30.0, 0.0));
        h.get_by_label("Gli angoli si incrociano: rimettili ognuno al suo posto.");
        tocca(&mut h, pdf_ocr::FATTO);
        assert!(h.state().bordi_aperti() && h.state().pagine_scansionate() == Some(0), "Fatto con gli angoli incrociati");
        // Tutta la foto: la pagina è la foto intera.
        tocca(&mut h, pdf_ocr::TUTTA_LA_FOTO);
        tocca(&mut h, pdf_ocr::FATTO);
        finche(&mut h, "la pagina", |h| h.state().pagine_scansionate() == Some(1) && !h.state().lavoro_in_corso());
        assert_eq!(h.state().misure_scansionate(), [(1500, 2055)]);
        let d = diagnosi_della_prova();
        // (Le righe della foto e della pagina le scrive il thread di lavoro: la prova non le vede.)
        for riga in ["] scansione: tutta la foto\n", "] scansione: bordi fatti, tutta la foto\n"] {
            assert!(d.contains(riga), "manca «{riga}»:\n{d}");
        }
    }

    /// L'aspetto si sceglie sotto la pagina e la rifà dalla sua foto; vale anche per le pagine
    /// dopo. Bordi, dalla miniatura della prima pagina, rifà gli angoli di quella sola.
    #[test]
    fn l_aspetto_e_i_bordi_di_una_pagina_fatta() {
        use tascabile_pdf::scansione::Aspetto;
        let mut h = telefono_con_la_scansione("scansione-aspetto", 1);
        assert_eq!(h.state().aspetti_scansionati(), (vec![Aspetto::Documento], 0));
        // Aspetto apre il foglio dei quattro, con la spiegazione; Documento è quello scelto.
        tocca(&mut h, pdf_ocr::ASPETTO);
        h.run_steps(2);
        h.get_by_label(pdf_ocr::ASPETTO_DELLA_PAGINA);
        h.get_by_label("Solo nero sul bianco: per i testi, il file più leggero");
        assert!(acceso(&h, "Documento") && !acceso(&h, "Bianco e nero"));
        // La X lo chiude senza cambiare niente; poi di nuovo Aspetto.
        tocca(&mut h, pdf_ocr::CHIUDI_ASPETTO);
        assert!(h.query_by_label(pdf_ocr::ASPETTO_DELLA_PAGINA).is_none(), "la X non ha chiuso il foglio");
        assert_eq!(h.state().aspetti_scansionati(), (vec![Aspetto::Documento], 0));
        tocca(&mut h, pdf_ocr::ASPETTO);
        h.run_steps(2);
        tocca(&mut h, "Bianco e nero");
        assert!(h.query_by_label(pdf_ocr::ASPETTO_DELLA_PAGINA).is_none(), "il foglio resta aperto");
        finche(&mut h, "la pagina in bianco e nero", |h| !h.state().lavoro_in_corso() && h.state().aspetti_scansionati().0 == [Aspetto::BiancoNero]);
        tocca(&mut h, pdf_ocr::FOTOCAMERA);
        h.run_steps(3);
        foto_ai_bordi(&mut h);
        tocca(&mut h, pdf_ocr::FATTO);
        finche(&mut h, "la seconda pagina", |h| h.state().pagine_scansionate() == Some(2) && !h.state().lavoro_in_corso());
        assert_eq!(h.state().aspetti_scansionati(), (vec![Aspetto::BiancoNero, Aspetto::BiancoNero], 1));
        // La prima, dalla miniatura: Bordi con gli angoli di quella pagina, poi tutta la foto.
        let prima = h.state().misure_scansionate();
        tocca(&mut h, "Pagina 1");
        assert_eq!(h.state().aspetti_scansionati().1, 0);
        tocca(&mut h, pdf_ocr::BORDI);
        finche(&mut h, "i bordi della prima", |h| h.state().bordi_aperti() && !h.state().lavoro_in_corso());
        h.run_steps(2);
        tocca(&mut h, pdf_ocr::TUTTA_LA_FOTO);
        tocca(&mut h, pdf_ocr::FATTO);
        finche(&mut h, "la prima rifatta", |h| !h.state().lavoro_in_corso() && h.state().misure_scansionate()[0] == (1500, 2055));
        assert_eq!(h.state().misure_scansionate()[1], prima[1]);
        assert_eq!(h.state().aspetti_scansionati(), (vec![Aspetto::BiancoNero, Aspetto::BiancoNero], 0));
        assert!(diagnosi_della_prova().contains("] scansione: aspetto bianco e nero\n"));
    }

    /// Il formato del foglio in «Sistema i bordi» (l'autore, 3/11): Automatico all'inizio; scelto
    /// A5, la pagina è un A5 e le foto dopo partono da A5; Bordi su una pagina fatta riparte dal
    /// suo formato.
    #[test]
    fn il_formato_del_foglio_si_sceglie_nei_bordi() {
        use tascabile_pdf::scansione::Formato;
        let mut h = telefono_con_la_scansione("scansione-formato", 1);
        assert_eq!(h.state().ritocchi_scansionati()[0].formato, Formato::Automatico);
        tocca(&mut h, pdf_ocr::FOTOCAMERA);
        h.run_steps(3);
        foto_ai_bordi(&mut h);
        assert!(acceso(&h, "Automatico") && !acceso(&h, "A5"));
        tocca(&mut h, "A5");
        assert!(acceso(&h, "A5"));
        tocca(&mut h, pdf_ocr::FATTO);
        finche(&mut h, "la seconda pagina", |h| h.state().pagine_scansionate() == Some(2) && !h.state().lavoro_in_corso());
        assert_eq!(h.state().ritocchi_scansionati()[1].formato, Formato::A5);
        let (w, a) = h.state().misure_scansionate()[1];
        assert!((f64::from(w) / f64::from(a) - 148.0 / 210.0).abs() < 0.01, "{w} × {a}");
        // La foto dopo parte dall'ultimo formato scelto.
        tocca(&mut h, pdf_ocr::FOTOCAMERA);
        h.run_steps(3);
        foto_ai_bordi(&mut h);
        assert!(acceso(&h, "A5"));
        tocca(&mut h, "Biglietto da visita");
        tocca(&mut h, pdf_ocr::FATTO);
        finche(&mut h, "la terza pagina", |h| h.state().pagine_scansionate() == Some(3) && !h.state().lavoro_in_corso());
        assert_eq!(h.state().ritocchi_scansionati()[2].formato, Formato::Biglietto);
        let d = diagnosi_della_prova();
        assert!(d.contains("] scansione: formato a5\n") && d.contains("] scansione: formato biglietto\n"), "{d}");
    }

    /// La gomma (il cancellino chiesto dall'autore il 3/11): col dito sulla pagina si fanno le
    /// passate, Annulla toglie l'ultima, «Fatto» rifà la pagina con le passate, che restano anche
    /// girandola. Indietro la chiude senza toccare la pagina.
    #[test]
    fn la_gomma_sbianca_la_pagina_e_resta() {
        let mut h = telefono_con_la_scansione("scansione-gomma", 1);
        tocca(&mut h, pdf_ocr::GOMMA);
        h.run_steps(2);
        for voce in ["Sottile", "Media", "Larga", pdf_ocr::ANNULLA_PASSATA] {
            h.get_by_label(voce);
        }
        assert!(acceso(&h, "Media") && !acceso(&h, "Larga"));
        let pagina = h.get_by_label("Passa il dito sulle macchie da togliere").rect();
        let riga = |y: f32| (egui::pos2(pagina.left() + 20.0, pagina.top() + y), egui::pos2(pagina.right() - 20.0, pagina.top() + y));
        let (da, a) = riga(60.0);
        trascina(&mut h, da, a);
        tocca(&mut h, pdf_ocr::ANNULLA_PASSATA);
        tocca(&mut h, "Larga");
        assert!(acceso(&h, "Larga"));
        let (da, a) = riga(90.0);
        trascina(&mut h, da, a);
        // Indietro chiude senza rifare; poi di nuovo, e Fatto.
        indietro(&mut h);
        h.run_steps(2);
        assert!(h.query_by_label("Larga").is_none() && h.state().ritocchi_scansionati()[0].cancellature.is_empty());
        tocca(&mut h, pdf_ocr::GOMMA);
        h.run_steps(2);
        trascina(&mut h, da, a);
        tocca(&mut h, pdf_ocr::FATTO);
        finche(&mut h, "la pagina rifatta", |h| !h.state().lavoro_in_corso() && h.state().ritocchi_scansionati()[0].cancellature.len() == 1);
        let passata = &h.state().ritocchi_scansionati()[0].cancellature[0];
        assert!(passata.punti.len() > 2 && passata.raggio > 0.0 && passata.raggio < 0.2, "{passata:?}");
        // Girata, la passata resta.
        tocca(&mut h, pdf_ocr::RUOTA);
        finche(&mut h, "la pagina girata", |h| !h.state().lavoro_in_corso());
        assert_eq!(h.state().ritocchi_scansionati()[0].cancellature.len(), 1);
        let d = diagnosi_della_prova();
        for r in ["] scansione: gomma aperta\n", "] scansione: gomma chiusa senza passate\n", "] scansione: gomma, 1 passate\n"] {
            assert!(d.contains(r), "manca «{r}»:\n{d}");
        }
    }

    /// I punti in mezzo ai lati di «Sistema i bordi» (3/11, foglio piegato o lente): trascinati,
    /// il lato si curva e la pagina si stende fra le curve; un angolo trascinato si porta dietro
    /// la metà dei suoi due lati.
    #[test]
    fn i_lati_del_foglio_si_curvano_col_dito() {
        let mut h = telefono_con_la_scansione("scansione-lati", 1);
        tocca(&mut h, pdf_ocr::FOTOCAMERA);
        h.run_steps(3);
        foto_ai_bordi(&mut h);
        let prima = h.state().lati_dei_bordi().unwrap();
        let dove = h.get_by_label(pdf_ocr::LATI[2]).rect().center();
        trascina(&mut h, dove, dove + egui::vec2(0.0, 25.0));
        let dopo = h.state().lati_dei_bordi().unwrap();
        assert!(dopo[2][1] > prima[2][1] + 40.0 && dopo[0] == prima[0], "{prima:?} → {dopo:?}");
        // L'angolo in basso a destra, verso destra: i lati destro e basso lo seguono a metà.
        let angoli = h.state().angoli_dei_bordi().unwrap();
        let dove = h.get_by_label(pdf_ocr::ANGOLI[2]).rect().center();
        trascina(&mut h, dove + egui::vec2(-4.0, -4.0), dove + egui::vec2(-24.0, -4.0));
        let spostati = h.state().angoli_dei_bordi().unwrap();
        let lati = h.state().lati_dei_bordi().unwrap();
        let dx = spostati[2][0] - angoli[2][0];
        assert!(dx < -40.0, "{angoli:?} → {spostati:?}");
        assert!((lati[1][0] - dopo[1][0] - dx / 2.0).abs() < 1.0 && (lati[2][0] - dopo[2][0] - dx / 2.0).abs() < 1.0, "{lati:?}");
        tocca(&mut h, pdf_ocr::FATTO);
        finche(&mut h, "la seconda pagina", |h| h.state().pagine_scansionate() == Some(2) && !h.state().lavoro_in_corso());
        assert!(h.state().ritocchi_scansionati()[1].lati.is_some());
        assert!(diagnosi_della_prova().contains("] scansione: lati curvi\n"));
    }

    /// Indietro da «Sistema i bordi» lascia la foto: si torna all'anteprima com'era, o alla
    /// scansione vuota.
    #[test]
    fn indietro_dai_bordi_lascia_la_foto() {
        let mut h = telefono_con_la_scansione("scansione-lascia", 1);
        tocca(&mut h, pdf_ocr::FOTOCAMERA);
        h.run_steps(3);
        foto_ai_bordi(&mut h);
        indietro(&mut h);
        h.run_steps(3);
        assert!(!h.state().bordi_aperti());
        assert_eq!(h.state().pagine_scansionate(), Some(1));
        let mut h = telefono(app_di_prova("scansione-lascia-vuota"));
        tocca(&mut h, "Nuovo");
        h.run_steps(15);
        tocca(&mut h, "Scansiona");
        h.run_steps(4);
        tocca(&mut h, pdf_ocr::FOTOCAMERA);
        h.run_steps(3);
        foto_ai_bordi(&mut h);
        tocca(&mut h, pdf_ocr::LASCIA_LA_FOTO);
        h.run_steps(3);
        assert_eq!(h.state().pagine_scansionate(), Some(0), "la scansione vuota resta, con Fotocamera e Galleria");
        h.get_by_role_and_label(Role::Button, pdf_ocr::GALLERIA);
        assert!(diagnosi_della_prova().contains("] scansione: foto lasciata\n"));
    }

    /// Quanti file restano nella cartella delle foto della cache (anche nelle cartelle delle scelte).
    fn foto_nella_cache(h: &Telefono) -> usize {
        fn conta(d: &std::path::Path) -> usize {
            std::fs::read_dir(d).map_or(0, |v| v.filter_map(Result::ok).map(|e| if e.path().is_dir() { conta(&e.path()) } else { 1 }).sum())
        }
        conta(&h.state().piattaforma.cartella_cache().join("foto"))
    }

    /// Quello che resta nella cartella delle foto della cache: file e cartelle delle richieste.
    fn rimasto_fra_le_foto(cache: &std::path::Path) -> Vec<String> {
        std::fs::read_dir(cache.join("foto"))
            .map_or(Vec::new(), |v| v.filter_map(Result::ok).map(|e| e.file_name().to_string_lossy().into_owned()).collect())
    }

    /// Più foto dalla galleria (0.8.3): una dopo l'altra in «Sistema i bordi», con «2 di 3»; Salta
    /// lascia una foto e passa alla prossima; ogni foto letta si cancella dalla cache.
    #[test]
    fn dalla_galleria_piu_foto_una_dopo_l_altra() {
        let mut h = telefono(app_di_prova("scansione-galleria"));
        tocca(&mut h, "Nuovo");
        h.run_steps(15);
        tocca(&mut h, "Scansiona");
        h.run_steps(4);
        tocca(&mut h, pdf_ocr::GALLERIA);
        let foglio = tascabile_pdf::prova::foto_di_un_foglio();
        h.state().piattaforma.simula_foto_galleria(&[&foglio, &foglio, &foglio]);
        finche(&mut h, "la prima ai bordi", |h| h.state().bordi_aperti() && !h.state().lavoro_in_corso());
        h.run_steps(2);
        h.get_by_label("Sistema i bordi · 1 di 3");
        assert_eq!(h.state().foto_in_coda(), 2);
        // Per una foto della galleria c'è Salta, non Rifai; Indietro le lascerebbe tutte.
        h.get_by_role_and_label(Role::Button, pdf_ocr::SALTA);
        h.get_by_role_and_label(Role::Button, pdf_ocr::LASCIA_LE_FOTO);
        assert!(h.query_by_label(pdf_ocr::RIFAI).is_none());
        tocca(&mut h, pdf_ocr::FATTO);
        finche(&mut h, "la seconda ai bordi", |h| {
            h.state().bordi_aperti() && !h.state().lavoro_in_corso() && h.state().pagine_scansionate() == Some(1)
        });
        h.run_steps(2);
        h.get_by_label("Sistema i bordi · 2 di 3");
        tocca(&mut h, pdf_ocr::SALTA);
        finche(&mut h, "la terza ai bordi", |h| h.state().bordi_aperti() && !h.state().lavoro_in_corso());
        h.run_steps(2);
        h.get_by_label("Sistema i bordi · 3 di 3");
        h.get_by_role_and_label(Role::Button, pdf_ocr::LASCIA_LA_FOTO);
        tocca(&mut h, pdf_ocr::FATTO);
        finche(&mut h, "due pagine", |h| h.state().pagine_scansionate() == Some(2) && !h.state().lavoro_in_corso());
        assert_eq!(h.state().foto_in_coda(), 0);
        assert_eq!(foto_nella_cache(&h), 0, "foto rimaste nella cache");
        assert!(diagnosi_della_prova().contains("] scansione: foto saltata\n"));
    }

    /// Indietro dai bordi della prima foto scelta dalla galleria lascia anche le altre, e le cancella
    /// dalla cache; la scansione resta, vuota.
    #[test]
    fn lascia_dai_bordi_della_galleria_lascia_tutte_le_foto() {
        let mut h = telefono(app_di_prova("scansione-galleria-lascia"));
        tocca(&mut h, "Nuovo");
        h.run_steps(15);
        tocca(&mut h, "Scansiona");
        h.run_steps(4);
        tocca(&mut h, pdf_ocr::GALLERIA);
        let foglio = tascabile_pdf::prova::foto_di_un_foglio();
        h.state().piattaforma.simula_foto_galleria(&[&foglio, &foglio, &foglio]);
        finche(&mut h, "la prima ai bordi", |h| h.state().bordi_aperti() && !h.state().lavoro_in_corso());
        h.run_steps(2);
        tocca(&mut h, pdf_ocr::LASCIA_LE_FOTO);
        h.run_steps(3);
        assert!(!h.state().bordi_aperti());
        assert_eq!((h.state().pagine_scansionate(), h.state().foto_in_coda()), (Some(0), 0));
        assert_eq!(foto_nella_cache(&h), 0, "foto rimaste nella cache");
        assert!(diagnosi_della_prova().contains("] scansione: foto lasciata, con altre 2 della galleria\n"));
    }

    /// Più di 30 foto dalla galleria: arrivano le prime 30 e un avviso lo dice; Indietro dai bordi
    /// della prima le lascia tutte.
    #[test]
    fn oltre_trenta_foto_dalla_galleria_lo_dice() {
        let mut h = telefono(app_di_prova("scansione-galleria-tante"));
        tocca(&mut h, "Nuovo");
        h.run_steps(15);
        tocca(&mut h, "Scansiona");
        h.run_steps(4);
        tocca(&mut h, pdf_ocr::GALLERIA);
        let foglio = tascabile_pdf::prova::foto_di_un_foglio();
        let tante: Vec<&[u8]> = (0..33).map(|_| &foglio[..]).collect();
        h.state().piattaforma.simula_foto_galleria(&tante);
        h.run_steps(3);
        h.get_by_label_contains("Ho preso le prime 30 delle 33 foto scelte");
        finche(&mut h, "la prima ai bordi", |h| h.state().bordi_aperti() && !h.state().lavoro_in_corso());
        h.run_steps(2);
        h.get_by_label("Sistema i bordi · 1 di 30");
        assert_eq!(h.state().foto_in_coda(), 29);
        tocca(&mut h, pdf_ocr::LASCIA_LE_FOTO);
        h.run_steps(3);
        assert_eq!((h.state().pagine_scansionate(), h.state().foto_in_coda(), foto_nella_cache(&h)), (Some(0), 0, 0));
    }

    /// Le miniature scorrono: con tante pagine l'ultima fatta viene in vista, e la prima si
    /// raggiunge scorrendo e si sceglie.
    #[test]
    fn le_miniature_scorrono_fino_a_ogni_pagina() {
        let mut h = telefono_di(app_di_prova("scansione-miniature"), egui::vec2(372.0, 827.0), 3.0);
        tocca(&mut h, "Nuovo");
        h.run_steps(15);
        tocca(&mut h, "Scansiona");
        h.run_steps(4);
        tocca(&mut h, pdf_ocr::GALLERIA);
        let foglio = tascabile_pdf::prova::foto_di_un_foglio();
        h.state().piattaforma.simula_foto_galleria(&[&foglio[..]; 8]);
        for n in 1..=8 {
            finche(&mut h, "la foto ai bordi", |h| h.state().bordi_aperti() && !h.state().lavoro_in_corso());
            h.run_steps(2);
            tocca(&mut h, pdf_ocr::FATTO);
            finche(&mut h, "la pagina", |h| h.state().pagine_scansionate() == Some(n) && !h.state().lavoro_in_corso());
        }
        h.run_steps(6);
        let schermo = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(372.0, 827.0));
        assert!(schermo.contains_rect(h.get_by_label("Pagina 8").rect()), "l'ultima pagina fatta non è in vista");
        assert!(!schermo.contains_rect(h.get_by_label("Pagina 1").rect()), "con 8 pagine la striscia dovrebbe scorrere");
        h.get_by_label("Pagina 1").scroll_to_me();
        h.run_steps(6);
        tocca(&mut h, "Pagina 1");
        assert_eq!(h.state().aspetti_scansionati().1, 0);
    }

    /// La X mentre si aspetta la foto (se la fotocamera non ha risposto) smette di aspettare e
    /// lascia la scansione aperta; la foto che arriva dopo non la aspetta nessuno e si cancella.
    #[test]
    fn la_x_mentre_si_aspetta_la_foto_smette_di_aspettare() {
        let mut h = telefono(app_di_prova("scansione-x-attesa"));
        tocca(&mut h, "Nuovo");
        h.run_steps(15);
        tocca(&mut h, "Scansiona");
        h.run_steps(4);
        tocca(&mut h, pdf_ocr::FOTOCAMERA);
        assert!(h.state().scansione_attende());
        tocca(&mut h, pdf_ocr::CHIUDI);
        assert!(!h.state().scansione_attende());
        assert_eq!(h.state().pagine_scansionate(), Some(0));
        h.state().simula_foto_firma_di_prova(Some(&tascabile_pdf::prova::foto_di_un_foglio()));
        h.run_steps(4);
        assert!(!h.state().bordi_aperti() && foto_nella_cache(&h) == 0);
        tocca(&mut h, pdf_ocr::CHIUDI);
        assert_eq!(h.state().pagine_scansionate(), None);
        assert!(diagnosi_della_prova().contains("] scansione: non aspetto più la foto\n"));
    }

    /// Togli, nella barra della pagina, chiede prima; poi toglie la pagina che si vede.
    #[test]
    fn togli_chiede_e_toglie_la_pagina() {
        let mut h = telefono_con_la_scansione("scansione-togli", 2);
        tocca(&mut h, "Pagina 1");
        tocca(&mut h, pdf_ocr::TOGLI);
        h.run_steps(2);
        h.get_by_label("Togliere la pagina 1?");
        tocca(&mut h, pdf_ocr::ANNULLA_SCANSIONE);
        assert_eq!(h.state().pagine_scansionate(), Some(2));
        tocca(&mut h, pdf_ocr::TOGLI);
        h.run_steps(2);
        tocca(&mut h, pdf_ocr::TOGLI_LA_PAGINA);
        assert_eq!(h.state().pagine_scansionate(), Some(1));
        assert_eq!(h.state().aspetti_scansionati().1, 0);
        assert!(diagnosi_della_prova().contains("] scansione: pagina tolta, ne restano 1\n"));
    }

    /// I rettangoli di questi pulsanti stanno nello schermo e non si sovrappongono.
    fn tutti_visibili(h: &Telefono, etichette: &[&str], schermo: egui::Rect) {
        let posti: Vec<(&str, egui::Rect)> = etichette.iter().map(|e| (*e, h.get_by_role_and_label(Role::Button, e).rect())).collect();
        for (e, r) in &posti {
            assert!(schermo.contains_rect(*r), "«{e}» esce dallo schermo: {r:?} in {schermo:?}");
            // Le chip sono alte 34 dp.
            assert!(r.width() >= 44.0 && r.height() >= 32.0, "«{e}» troppo piccolo da toccare: {r:?}");
        }
        for (i, (a, ra)) in posti.iter().enumerate() {
            for (b, rb) in &posti[i + 1..] {
                assert!(!ra.shrink(0.5).intersects(rb.shrink(0.5)), "«{a}» {ra:?} e «{b}» {rb:?} si sovrappongono");
            }
        }
    }

    /// La firma col dito sul telefono dell'autore, su uno stretto e girata (0.8.3: a 372 dp Cancella
    /// toccava quasi il nero, in orizzontale le chip del tratto finivano sopra Cancella): ogni
    /// comando si vede, intero e senza sovrapposizioni, e fra Cancella, i colori e Fatto restano
    /// almeno 8 dp.
    #[test]
    fn i_comandi_della_firma_col_dito_si_vedono_su_ogni_telefono() {
        for (nome, misura, densita) in
            [("davide", egui::vec2(372.0, 827.0), 3.0), ("stretto", egui::vec2(360.0, 760.0), 3.0), ("girato", egui::vec2(827.0, 372.0), 3.0)]
        {
            let (piattaforma, dir) = piattaforma_di_prova(&format!("firma-misure-{nome}"));
            let percorso = dir.join("radice/Modulo.pdf");
            std::fs::write(&percorso, tascabile_pdf::prova::modulo()).unwrap();
            let mut h = telefono_di(TascabileApp::new(piattaforma, Instant::now()), misura, densita);
            h.state_mut().apri_file(&percorso, "Modulo.pdf");
            aspetta_il_lettore(&mut h);
            dagli_strumenti(&mut h, pdf_firma::FIRMA);
            h.run_steps(6);
            tocca(&mut h, pdf_firma::FIRMA_COL_DITO);
            h.run_steps(2);
            let schermo = egui::Rect::from_min_size(egui::Pos2::ZERO, misura);
            let comandi = ["Stilografica", "Penna", "Pennarello", "Nero", "Blu", pdf_firma::CANCELLA, pdf_firma::FATTO, pdf_firma::CHIUDI];
            tutti_visibili(&h, &comandi, schermo);
            let r = |e: &str| h.get_by_role_and_label(Role::Button, e).rect();
            // La distanza fra due bersagli: lo spazio vuoto fra i due, in orizzontale o in verticale.
            let distanza =
                |a: egui::Rect, b: egui::Rect| (b.left() - a.right()).max(a.left() - b.right()).max((b.top() - a.bottom()).max(a.top() - b.bottom()));
            for (a, b) in [
                (pdf_firma::CANCELLA, "Nero"),
                (pdf_firma::CANCELLA, "Blu"),
                ("Nero", pdf_firma::FATTO),
                ("Blu", pdf_firma::FATTO),
                (pdf_firma::CANCELLA, pdf_firma::FATTO),
            ] {
                let d = distanza(r(a), r(b));
                assert!(d >= 8.0, "{nome}: fra «{a}» {:?} e «{b}» {:?} solo {d} dp", r(a), r(b));
            }
            // Dopo Cancella, al suo posto Rimetti, che si vede.
            firma_col_dito(&mut h, egui::pos2(misura.x * 0.2, misura.y * 0.4));
            tocca(&mut h, pdf_firma::CANCELLA);
            tutti_visibili(&h, &[pdf_firma::RIMETTI, pdf_firma::FATTO], schermo);
        }
    }

    /// La scansione e «Sistema i bordi» sul telefono dell'autore (372 dp, dove Rifai finiva sotto
    /// Altra pagina, 28/10), su uno stretto da 360 e girati: ogni pulsante si vede, intero.
    #[test]
    fn i_comandi_della_scansione_si_vedono_su_ogni_telefono() {
        for (nome, misura, densita) in
            [("davide", egui::vec2(372.0, 827.0), 3.0), ("stretto", egui::vec2(360.0, 760.0), 3.0), ("girato", egui::vec2(827.0, 372.0), 3.0)]
        {
            let mut h = scansione_su(telefono_di(app_di_prova(&format!("scansione-misure-{nome}")), misura, densita), 2);
            let schermo = egui::Rect::from_min_size(egui::Pos2::ZERO, misura);
            let pagina = [
                pdf_ocr::CREA_PDF,
                pdf_ocr::BORDI,
                pdf_ocr::RUOTA,
                pdf_ocr::ASPETTO,
                pdf_ocr::GOMMA,
                pdf_ocr::RIFAI,
                pdf_ocr::TOGLI,
                pdf_ocr::FOTOCAMERA,
                pdf_ocr::GALLERIA,
            ];
            tutti_visibili(&h, &pagina, schermo);
            h.get_by_label("Pagina 2");
            // Il titolo e quante pagine, interi accanto a Crea PDF.
            assert_eq!((tagliato(&h, pdf_ocr::SCANSIONE), tagliato(&h, "2 pagine")), (Some(false), Some(false)), "{nome}");
            tocca(&mut h, pdf_ocr::FOTOCAMERA);
            h.run_steps(3);
            foto_ai_bordi(&mut h);
            tutti_visibili(&h, &[pdf_ocr::LASCIA_LA_FOTO, pdf_ocr::TUTTA_LA_FOTO, pdf_ocr::RIFAI, pdf_ocr::FATTO], schermo);
            for angolo in pdf_ocr::ANGOLI.iter().chain(&pdf_ocr::LATI) {
                assert!(schermo.contains_rect(h.get_by_label(angolo).rect()), "{nome}: {angolo}");
            }
            tocca(&mut h, pdf_ocr::FATTO);
            finche(&mut h, "la terza pagina", |h| h.state().pagine_scansionate() == Some(3) && !h.state().lavoro_in_corso());
            tocca(&mut h, pdf_ocr::ASPETTO);
            h.run_steps(2);
            tutti_visibili(&h, &["Documento", "Grigi", "Bianco e nero", "Originale"], schermo);
        }
    }

    #[test]
    fn indietro_nella_scansione_chiede_prima_di_buttare_le_pagine() {
        // Analisi del 27/10, pagine I2: un Indietro (anche il gesto dal bordo, per sbaglio)
        // buttava le pagine fotografate.
        let mut h = telefono_con_la_scansione("scansione-indietro", 2);
        indietro(&mut h);
        h.run_steps(3);
        assert_eq!(h.state().pagine_scansionate(), Some(2));
        h.get_by_label(pdf_ocr::BUTTARE);
        // Annulla: si resta con le pagine.
        tocca(&mut h, pdf_ocr::ANNULLA_SCANSIONE);
        assert_eq!(h.state().pagine_scansionate(), Some(2));
        assert!(h.query_by_label(pdf_ocr::BUTTARE).is_none());
        // La X chiede lo stesso; Butta chiude.
        tocca(&mut h, pdf_ocr::CHIUDI);
        h.run_steps(3);
        tocca(&mut h, pdf_ocr::BUTTA);
        assert_eq!(h.state().pagine_scansionate(), None);
    }

    #[test]
    fn rifai_tiene_la_pagina_finche_non_arriva_quella_nuova() {
        // Analisi del 27/10, pagine I3: Rifai toglieva la pagina prima della foto nuova, e con la
        // fotocamera annullata la pagina era persa (con una pagina sola, tutta la scansione).
        let mut h = telefono_con_la_scansione("scansione-rifai", 2);
        tocca(&mut h, pdf_ocr::RIFAI);
        h.run_steps(3);
        assert_eq!(h.state().pagine_scansionate(), Some(2), "durante la fotocamera la pagina c'è ancora");
        h.state().simula_foto_firma_di_prova(None);
        h.run_steps(6);
        assert_eq!(h.state().pagine_scansionate(), Some(2), "fotocamera annullata: la pagina resta");
        // Rifai con la foto nuova: sempre due pagine, la seconda cambiata (dopo i bordi).
        tocca(&mut h, pdf_ocr::RIFAI);
        h.run_steps(3);
        foto_ai_bordi(&mut h);
        assert_eq!(h.state().pagine_scansionate(), Some(2), "durante i bordi la pagina c'è ancora");
        tocca(&mut h, pdf_ocr::TUTTA_LA_FOTO);
        tocca(&mut h, pdf_ocr::FATTO);
        finche(&mut h, "la pagina rifatta", |h| !h.state().lavoro_in_corso() && h.state().misure_scansionate()[1] == (1500, 2055));
        assert_eq!(h.state().pagine_scansionate(), Some(2));
    }

    #[test]
    fn ruota_gira_la_pagina_fotografata() {
        // Analisi del 27/10, pagine I5: nell'anteprima non c'era Ruota, e il foglio fotografato di
        // traverso finiva coricato nel PDF, col testo riconosciuto sbagliato.
        let mut h = telefono_con_la_scansione("scansione-ruota", 2);
        let prima = h.state().misure_scansionate();
        assert!(prima[1].0 < prima[1].1, "la pagina di prova è in piedi: {prima:?}");
        tocca(&mut h, pdf_ocr::RUOTA);
        finche(&mut h, "la pagina girata", |h| !h.state().lavoro_in_corso() && h.state().misure_scansionate()[1] == (prima[1].1, prima[1].0));
        // Gira l'ultima, quella che si vede; la prima resta com'era.
        assert_eq!(h.state().misure_scansionate()[0], prima[0]);
        tocca(&mut h, pdf_ocr::RUOTA);
        finche(&mut h, "di nuovo in piedi", |h| !h.state().lavoro_in_corso() && h.state().misure_scansionate()[1] == prima[1]);
        assert!(diagnosi_della_prova().contains("] scansione: pagina girata\n"));
    }

    #[test]
    fn la_scansione_pronta_chiede_prima_di_lasciare_un_pdf_modificato() {
        // Analisi del 27/10, critico 6: la scansione si apriva al posto del PDF modificato senza
        // chiedere, e la copia di recupero spariva.
        let tre = tascabile_pdf::prova::pdf(&[tascabile_pdf::prova::Pagina::bianca(595.0, 842.0); 3]);
        let mut h = telefono_col_pdf("scansione-pronta", tre);
        h.state_mut().modifica_pdf(tascabile_pdf::Modifica::ruota(vec![0], 90));
        aspetta_il_motore(&mut h);
        assert_eq!(h.state().pdf_modificato(), Some(true));
        let scansione = cartella_di_prova("scansione-pronta-file").join("Scansione.pdf");
        std::fs::write(&scansione, tascabile_pdf::prova::pdf(&[tascabile_pdf::prova::Pagina::bianca(595.0, 842.0)])).unwrap();
        h.state_mut().apri_la_scansione(scansione, "Scansione.pdf".into());
        h.run_steps(4);
        h.get_by_label("Salvare le modifiche?");
        assert_eq!(h.state().misure_del_pdf().len(), 3, "il PDF modificato non è più aperto");
        // «Non salvare»: si apre la scansione.
        tocca(&mut h, "Non salvare");
        let fine = Instant::now() + Duration::from_secs(30);
        while h.state().misure_del_pdf().len() != 1 && Instant::now() < fine {
            std::thread::sleep(Duration::from_millis(10));
            h.run_steps(1);
        }
        assert_eq!(h.state().misure_del_pdf().len(), 1, "la scansione non si è aperta");
    }

    #[test]
    fn le_lettere_che_il_pdf_non_ha_si_dicono_e_non_diventano_punti_di_domanda() {
        // Analisi del 27/10, critico 12: «Ionuț Ștefănescu, Łódź» in un campo o in un testo di
        // Fill & Sign si vedeva «Ionu? ?tef?nescu, ?ód?», senza un avviso.
        use tascabile_pdf::prova::CampoDiProva::Residente;
        let nome = "Ionuț Ștefănescu, Łódź";
        let mut h = telefono_col_pdf("lettere-che-mancano", tascabile_pdf::prova::modulo_da_compilare());
        // Nel campo il valore resta giusto, ma il disegno no: si salva e si avvisa.
        let c = campo_del_lettore(&h, Residente);
        h.state_mut().apri_scrittura_pdf(pdf_compila::Bersaglio::Campo(c.nome.clone()), "", false);
        h.run_steps(2);
        scrivi(&mut h, nome);
        h.state_mut().conferma_scrittura_pdf(true);
        aspetta_il_motore(&mut h);
        h.run_steps(2);
        h.get_by_label_contains("non c'è nel carattere del modulo");
        assert_eq!(campo_del_lettore(&h, Residente).testo(), nome);
        // Un testo di Fill & Sign si disegnerebbe coi «?» per sempre: non si posa, e si dice perché.
        let segni = h.state().pdf.as_ref().unwrap().motore.stato().segni.len();
        h.state_mut().apri_scrittura_pdf(pdf_compila::Bersaglio::Nuovo { pagina: 0, punto: [100.0, 300.0] }, "", false);
        h.run_steps(2);
        scrivi(&mut h, nome);
        h.state_mut().conferma_scrittura_pdf(true);
        aspetta_il_motore(&mut h);
        assert_eq!(h.state().pdf.as_ref().unwrap().motore.stato().segni.len(), segni, "il testo coi «?» è stato posato");
        assert!(h.state().errore.as_ref().is_some_and(|e| e.testo.contains("«ț»")), "{:?}", h.state().errore);
    }

    #[test]
    fn il_calendario_dopo_una_data_scritta_parte_da_quella() {
        // Analisi del 27/10, critico 10: dopo una data scritta con la tastiera, il calendario della
        // barra mostrava, e «Metti la data» rimetteva, quella di prima.
        use tascabile_pdf::prova::CampoDiProva::DataDiNascita;
        let mut h = telefono_col_pdf("calendario-dopo-la-tastiera", tascabile_pdf::prova::modulo_da_compilare());
        let c = campo_del_lettore(&h, DataDiNascita);
        h.state_mut().modifica_pdf(tascabile_pdf::Modifica::valore(&c, tascabile_pdf::FieldValue::Text("12/03/1958".into())));
        aspetta_il_motore(&mut h);
        // Scritta con la tastiera (il motore ha ancora quella di prima), poi il calendario.
        h.state_mut().apri_scrittura_pdf(pdf_compila::Bersaglio::Campo(c.nome.clone()), "01/01/1960", false);
        h.run_steps(2);
        tocca(&mut h, pdf_compila::SCEGLI_LA_DATA);
        aspetta_il_motore(&mut h);
        tocca(&mut h, pdf_compila::METTI_LA_DATA);
        aspetta_il_motore(&mut h);
        assert_eq!(campo_del_lettore(&h, DataDiNascita).testo(), "01/01/1960");
    }

    #[test]
    fn un_campo_a_piu_righe_del_modulo_tiene_gli_a_capo() {
        // Analisi del 27/10, critico 9: un valore su due righe, al primo tasto, si incollava e il
        // campo si confermava da solo.
        let mut h = telefono_col_pdf("campo-a-piu-righe", tascabile_pdf::prova::modulo_da_compilare());
        let note = campo_del_lettore(&h, tascabile_pdf::prova::CampoDiProva::Note);
        assert!(note.multilinea);
        h.state_mut().apri_scrittura_pdf(pdf_compila::Bersaglio::Campo(note.nome.clone()), "Allergia al lattice\nPortatore di pacemaker", false);
        h.run_steps(2);
        scrivi(&mut h, ".");
        let p = h.state().pdf.as_ref().unwrap();
        let s = p.compila.scrittura.as_ref().expect("la scrittura si è chiusa da sola");
        assert_eq!(s.campo.testo(), "Allergia al lattice\nPortatore di pacemaker.");
    }

    /// Bug segnalato dall'autore il 1/11: in un documento fotografato e riconosciuto le lettere non
    /// si cambiavano. Nel modo Modifica un tocco sul testo della scansione apre la scrittura col
    /// testo riconosciuto; a «Fatto» una domanda spiega che cosa succede: «Lascia com'era» non
    /// cambia niente, «Correggi» riscrive il paragrafo, che diventa un paragrafo come gli altri.
    /// Nella diagnosi né il testo di prima né quello nuovo.
    #[test]
    fn il_testo_di_una_scansione_si_corregge_dal_modo_modifica() {
        use tascabile_pdf::GenereOggetto;
        let mut h = telefono_col_pdf("correggi-scansione", tascabile_pdf::prova::scansione_riconosciuta_su_carta());
        tocca(&mut h, "Modifica");
        finche(&mut h, "gli oggetti della prima pagina", |h| !oggetti_pdf(h, 0).is_empty());
        let scansione = |h: &Telefono| oggetti_pdf(h, 0).into_iter().find(|o| o.genere == GenereOggetto::Scansione && o.testo == "Nove giorni fa");
        let o = scansione(&h).expect("il testo della scansione");
        let correggi_con = |h: &mut Telefono, testo: &str| {
            let o = scansione(h).expect("il testo della scansione");
            tocca_in(h, centro_dell_oggetto(h, &o));
            h.run_steps(3);
            let s = h.state().pdf.as_ref().unwrap().compila.scrittura.as_ref().expect("la scrittura del testo della scansione");
            assert_eq!(s.campo.testo(), "Nove giorni fa");
            // Il testo nuovo meno l'ultima lettera, che si batte: la scrittura vede un cambiamento.
            let (inizio, ultima) = testo.split_at(testo.len() - 1);
            h.state_mut().apri_scrittura_pdf(pdf_compila::Bersaglio::Oggetto(o.chiave()), inizio, false);
            h.run_steps(2);
            scrivi(h, ultima);
            tocca(h, pdf_compila::FATTO);
            h.run_steps(2);
        };
        // «Fatto» corregge subito, senza domande: il testo resta della scansione, invisibile
        // sopra la foto rifatta, e si corregge ancora.
        correggi_con(&mut h, "Dieci giorni fa");
        finche(&mut h, "il paragrafo corretto", |h| oggetti_pdf(h, 0).iter().any(|o| o.genere == GenereOggetto::Scansione && o.testo == "Dieci giorni fa"));
        assert!(scansione(&h).is_none());
        assert!(!oggetti_pdf(&h, 0).iter().any(|o| o.genere == GenereOggetto::Testo), "un testo che si vede sopra la foto");
        assert!(h.state().errore.is_none());
        // Il testo di una scansione non si sposta col dito.
        assert!(!o.genere.si_muove());
        let d = diagnosi_della_prova();
        for riga in [
            "] pdf: testo da riscrivere (testo della scansione)\n",
            "] pdf: scansione da correggere nella pagina 1\n",
        ] {
            assert!(d.contains(riga), "manca «{riga}»:\n{d}");
        }
        for testo in ["Nove giorni", "Dieci", "giorni fa"] {
            assert!(!d.contains(testo), "«{testo}» nella diagnosi:\n{d}");
        }
    }

    /// Segnalazione dell'autore del 3/11: il cursore della casella non si spostava col dito, e
    /// sarebbe stato meglio scrivere direttamente sul file. Il paragrafo si scrive lì dov'è, senza
    /// la casella sopra la tastiera; il cursore va dove si tocca o si trascina il dito, e la
    /// tastiera scrive da lì; «Fatto» sta accanto al testo.
    #[test]
    fn un_paragrafo_si_scrive_sulla_pagina_col_cursore_dove_si_tocca() {
        let contenuto = "BT /F1 24 Tf 60 700 Td (Pagina di prova) Tj ET";
        let byte = tascabile_pdf::prova::pdf(&[tascabile_pdf::prova::Pagina { larghezza: 595.0, altezza: 842.0, rotazione: 0, contenuto }]);
        let mut h = telefono_col_pdf("scrivi-sulla-pagina", byte);
        tocca(&mut h, "Modifica");
        finche(&mut h, "gli oggetti della prima pagina", |h| !oggetti_pdf(h, 0).is_empty());
        let o = oggetti_pdf(&h, 0).into_iter().find(|o| o.testo == "Pagina di prova").unwrap();
        let c = centro_dell_oggetto(&h, &o);
        tocca_in(&mut h, c);
        h.run_steps(3);
        let testo = |h: &Telefono| h.state().pdf.as_ref().unwrap().compila.scrittura.as_ref().map(|s| s.campo.testo().to_string());
        assert_eq!(testo(&h).as_deref(), Some("Pagina di prova"));
        // Niente casella sopra la tastiera: si scrive sulla pagina, sopra il paragrafo.
        assert!(h.query_by_label(pdf_compila::VUOTO_TESTO).is_none());
        let scrittura = h.get_by_label(pdf_modifica::SCRITTURA_SULLA_PAGINA).rect();
        let paragrafo = h.state().pdf.as_ref().unwrap().lettore.sullo_schermo(0, o.rettangolo).unwrap();
        assert!(scrittura.contains(paragrafo.center()), "{scrittura:?} {paragrafo:?}");
        // Il testo è alto come il paragrafo, non di più (0.8.7: enorme e a capo).
        assert!(scrittura.height() < paragrafo.height() * 1.6, "{scrittura:?} {paragrafo:?}");
        // Un tocco all'inizio: il cursore va lì e la lettera nuova pure.
        tocca_in(&mut h, egui::pos2(scrittura.left() + 2.0, scrittura.center().y));
        h.run_steps(2);
        scrivi(&mut h, "X");
        assert_eq!(testo(&h).as_deref(), Some("XPagina di prova"));
        // Il dito trascinato verso la fine porta il cursore in fondo.
        let scrittura = h.get_by_label(pdf_modifica::SCRITTURA_SULLA_PAGINA).rect();
        trascina_col_dito(&mut h, egui::pos2(scrittura.left() + 4.0, scrittura.center().y), egui::vec2(scrittura.width() - 6.0, 0.0));
        scrivi(&mut h, "!");
        assert_eq!(testo(&h).as_deref(), Some("XPagina di prova!"));
        tocca(&mut h, pdf_compila::FATTO);
        finche(&mut h, "il paragrafo riscritto", |h| oggetti_pdf(h, 0).iter().any(|o| o.testo == "XPagina di prova!"));
        assert!(testo(&h).is_none());
    }

    #[test]
    fn dopo_fatto_scorrere_dal_paragrafo_riscritto_non_lo_sposta() {
        // Analisi del 27/10, modifica I8: il paragrafo riscritto restava scelto dopo «Fatto», e uno
        // scorrimento che partiva da lì lo spostava di centinaia di punti.
        let contenuto = "BT /F1 24 Tf 60 700 Td (Pagina di prova) Tj ET";
        let byte = tascabile_pdf::prova::pdf(&[tascabile_pdf::prova::Pagina { larghezza: 595.0, altezza: 842.0, rotazione: 0, contenuto }; 3]);
        let mut h = telefono_col_pdf("fatto-poi-scorri", byte);
        tocca(&mut h, "Modifica");
        finche(&mut h, "gli oggetti della prima pagina", |h| !oggetti_pdf(h, 0).is_empty());
        let o = oggetti_pdf(&h, 0).into_iter().find(|o| o.testo == "Pagina di prova").unwrap();
        let c = centro_dell_oggetto(&h, &o);
        tocca_in(&mut h, c);
        h.run_steps(3);
        scrivi(&mut h, " bis");
        tocca(&mut h, pdf_compila::FATTO);
        let riscritto = |h: &Telefono| oggetti_pdf(h, 0).into_iter().find(|o| o.testo == "Pagina di prova bis");
        finche(&mut h, "il paragrafo riscritto", |h| riscritto(h).is_some());
        let prima = riscritto(&h).unwrap();
        let c = centro_dell_oggetto(&h, &prima);
        trascina_col_dito(&mut h, c, egui::vec2(0.0, -150.0));
        aspetta_il_motore(&mut h);
        h.run_steps(3);
        assert_eq!(riscritto(&h).unwrap().rettangolo, prima.rettangolo, "lo scorrimento ha spostato il paragrafo");
    }

    #[test]
    fn scorrere_vicino_a_una_spunta_non_scelta_non_la_sposta() {
        // Analisi del 27/10, compila 9: ogni segno di Fill & Sign si trascinava, e scorrere la pagina
        // col dito vicino a una spunta la spostava di centinaia di punti.
        let mut h = telefono_col_pdf("scorri-vicino-spunta", tascabile_pdf::prova::modulo_da_compilare());
        let info = h.state().pdf.as_ref().unwrap().motore.stato().geometrie[0].clone();
        h.state_mut().modifica_pdf(tascabile_pdf::Modifica::segno(0, &info, [450.0, 700.0], tascabile_pdf::NuovoSegno::Spunta, "").unwrap());
        aspetta_il_motore(&mut h);
        h.state_mut().pdf.as_mut().unwrap().compila.segno = None;
        h.run_steps(2);
        let spunta = |h: &Telefono| h.state().pdf.as_ref().unwrap().motore.stato().segni[0].clone();
        let prima = spunta(&h);
        let r = h.state().pdf.as_ref().unwrap().lettore.sullo_schermo(prima.pagina, prima.rettangolo).unwrap();
        trascina_col_dito(&mut h, egui::pos2(r.right() + 12.0, r.center().y), egui::vec2(0.0, -150.0));
        aspetta_il_motore(&mut h);
        assert_eq!(spunta(&h).rettangolo, prima.rettangolo, "la spunta non scelta si è spostata");
    }

    #[test]
    fn tornare_subito_a_un_campo_confermato_lo_riapre_col_testo_nuovo() {
        // Analisi del 27/10, compila 8: tornando al campo prima che il motore rispondesse, si
        // riapriva col testo di prima e quello nuovo si perdeva.
        use tascabile_pdf::prova::CampoDiProva::Sottoscritto;
        let mut h = telefono_col_pdf("tornare-subito", tascabile_pdf::prova::modulo_da_compilare());
        let c = campo_del_lettore(&h, Sottoscritto);
        h.state_mut().apri_scrittura_pdf(pdf_compila::Bersaglio::Campo(c.nome.clone()), "", false);
        h.run_steps(2);
        scrivi(&mut h, "Esposito");
        // Confermato e riaperto subito, senza frame in mezzo: il motore non ha ancora risposto, e
        // l'istantanea ha ancora il testo di prima.
        let vecchio = campo_del_lettore(&h, Sottoscritto).testo();
        let app = h.state_mut();
        app.conferma_scrittura_pdf(true);
        app.apri_scrittura_pdf(pdf_compila::Bersaglio::Campo(c.nome.clone()), &vecchio, false);
        let s = h.state().pdf.as_ref().unwrap().compila.scrittura.as_ref().unwrap();
        assert_eq!(s.campo.testo(), "Esposito");
    }

    #[test]
    fn la_casella_a_due_stati_non_accende_il_posto_sbagliato_e_lo_dice() {
        // Analisi del 27/10, compila 6: toccando «Due» si accendeva «Uno».
        let mut h = telefono_col_pdf("casella-due-stati", tascabile_pdf::prova::casella_a_due_stati());
        let due = h.get_all_by_label("Scelta").map(|n| n.rect()).max_by(|a, b| a.center().x.total_cmp(&b.center().x)).unwrap();
        tocca_in(&mut h, due.center());
        h.run_steps(3);
        aspetta_il_motore(&mut h);
        h.get_by_label_contains("non si può ancora spuntare");
        let stato = h.state().pdf.as_ref().unwrap().motore.stato();
        let c = stato.campi.iter().find(|c| c.suggerimento.as_deref() == Some("Scelta")).unwrap();
        assert!(c.posti.iter().all(|p| !p.acceso), "si è acceso un posto: {:?}", c.posti.iter().map(|p| p.acceso).collect::<Vec<_>>());
    }

    #[test]
    fn il_tocco_su_una_casella_piccola_accende_lei_e_non_la_vicina() {
        // Analisi del 27/10, critico 8: la presa di ogni campo (almeno 32 dp) copriva i vicini, ed
        // egui dava il tocco all'ultimo creato: si toccava il 5 della scala del dolore e si
        // accendeva il 6.
        let mut h = telefono_col_pdf("caselle-vicine", tascabile_pdf::prova::caselle_vicine());
        let (si, no) = (h.get_by_label("Si").rect(), h.get_by_label("No").rect());
        assert!(si.max.x <= no.min.x, "le prese si coprono: {si:?}, {no:?}");
        tocca_in(&mut h, si.center());
        h.run_steps(3);
        aspetta_il_motore(&mut h);
        let acceso = |h: &Telefono, nome: &str| {
            let stato = h.state().pdf.as_ref().unwrap().motore.stato();
            stato.campi.iter().find(|c| c.suggerimento.as_deref() == Some(nome)).unwrap().posti[0].acceso
        };
        assert!(acceso(&h, "Si") && !acceso(&h, "No"), "toccata «Si», accesa {}", if acceso(&h, "No") { "«No»" } else { "nessuna" });
    }

    #[test]
    fn un_pdf_troncato_si_apre_ma_lo_dice() {
        // Analisi del 27/10, lettura I6: un PDF scaricato a metà si apriva con meno pagine, come se
        // fosse completo; il motore lo ripara e lo scrive nel suo registro, che l'app non guardava.
        let intero = tascabile_pdf::prova::pdf(&[tascabile_pdf::prova::Pagina::bianca(595.0, 842.0); 3]);
        let troncato = intero[..intero.len() * 6 / 10].to_vec();
        let mut h = telefono_col_pdf("pdf-troncato", troncato);
        h.run_steps(3);
        h.get_by_label_contains("era rovinato");
        assert!(diagnosi_della_prova().contains(", riparato"), "{}", diagnosi_della_prova());
    }

    #[test]
    fn un_finto_pdf_da_l_errore_del_pdf_e_non_apre_word() {
        // Analisi del 27/10, lettura I4: un «PDF» vuoto (download interrotto) o una pagina d'errore
        // del web salvata col nome .pdf aprivano l'editor di Word.
        let (piattaforma, dir) = piattaforma_di_prova("finto-pdf");
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        let casi: [(&str, &[u8], &str); 2] =
            [("Referto.pdf", b"", "il file è vuoto"), ("Esito.pdf", b"<html><body>Sessione scaduta</body></html>", "non è un PDF che si legge")];
        for (nome, byte, atteso) in casi {
            let percorso = dir.join("radice").join(nome);
            std::fs::write(&percorso, byte).unwrap();
            h.state_mut().apri_file(&percorso, nome);
            finche(&mut h, "l'errore di apertura", |h| h.state().errore.is_some() || h.state().schermata == Schermata::Editor);
            assert_ne!(h.state().schermata, Schermata::Editor, "{nome}: si è aperto Word");
            let e = h.state_mut().errore.take().unwrap();
            assert!(e.testo.contains(atteso), "{nome}: {}", e.testo);
        }
    }

    #[test]
    fn un_odt_aperto_avvisa_che_non_si_vede_tutto() {
        // Analisi del 27/10, critico 4: il motore salta commenti e note di .odt e .rtf.
        let (piattaforma, d) = piattaforma_di_prova("apri-odt");
        let file = d.join("radice/Relazione.odt");
        let s = documento::apri_byte("prova.txt", b"Paziente stabile").unwrap();
        std::fs::write(&file, wordcraft_engine::io::save_bytes("prova.odt", &s.doc).unwrap()).unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        h.state_mut().apri_file(&file, "Relazione.odt");
        h.run_steps(4);
        assert_eq!(h.state().schermata, Schermata::Editor);
        h.get_by_label_contains("non si vedono tutte le parti");
    }

    #[test]
    fn apre_un_docx_da_una_cartella_di_prova() {
        let (piattaforma, d) = piattaforma_di_prova("apri-dalla-cartella");
        let cartella = d.join("radice/Download");
        std::fs::create_dir_all(&cartella).unwrap();
        let file = cartella.join("Verbale Zanzibar.docx");
        docx(&file, &["Primo paragrafo", "Kilimangiaro"]);
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        tocca(&mut h, "Download");
        tocca(&mut h, "Verbale Zanzibar.docx");
        let app = h.state();
        assert_eq!(app.schermata, Schermata::Editor);
        assert_eq!(testi(app), ["Primo paragrafo", "Kilimangiaro"]);
        assert_eq!(editor(app).nome, "Verbale Zanzibar.docx");
        assert!(!editor(app).s.dirty);
        // Nei recenti, in memoria e su disco.
        let chiave = file.to_str().unwrap();
        assert_eq!(app.home.recenti()[0].chiave, chiave);
        assert_eq!(Recenti::carica(&d.join("app").join(home::FILE_RECENTI)).voci()[0].nome, "Verbale Zanzibar.docx");
        let diag = diagnosi_della_prova();
        let byte = std::fs::metadata(&file).unwrap().len();
        assert!(diag.contains(&format!("] apri dalla cartella: {byte} byte, tipo docx, aperto in ")), "{diag}");
        for parola in ["Zanzibar", "Kilimangiaro", "Verbale"] {
            assert!(!diag.contains(parola), "«{parola}» nella diagnosi:\n{diag}");
        }
        // Uscendo dal documento si torna nella cartella; da lì, nella Home, è fra i recenti.
        indietro(&mut h);
        assert_eq!(h.state().schermata, Schermata::Home);
        assert_eq!(h.state().home.cartella_aperta(), Some(cartella.as_path()));
        indietro(&mut h);
        assert_eq!(h.state().home.cartella_aperta(), None);
        h.get_by_role_and_label(Role::Button, "Verbale Zanzibar");
        h.get_by_role_and_label(Role::Button, &home::etichetta_menu("Verbale Zanzibar"));
    }

    #[test]
    fn il_navigatore_scende_e_risale() {
        let (piattaforma, d) = piattaforma_di_prova("navigatore");
        let sotto = d.join("radice/Download/Lavoro");
        std::fs::create_dir_all(&sotto).unwrap();
        docx(&sotto.join("Relazione.docx"), &["Relazione"]);
        std::fs::write(d.join("radice/Download/Bolletta.pdf"), b"%PDF-1.7").unwrap();
        std::fs::write(d.join("radice/Download/Lezione.pptx"), b"x").unwrap();
        std::fs::write(d.join("radice/Download/foto.jpg"), b"x").unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        tocca(&mut h, "Download");
        // La barra alta col percorso e quello che c'è dentro.
        h.get_by_label("Download");
        h.get_by_label("1 cartella · 2 file");
        // I PDF e le presentazioni si aprono; quello che l'app non legge non si vede (l'autore, 3/11).
        for etichetta in ["Bolletta.pdf", "Lezione.pptx"] {
            assert!(!h.get_by_role_and_label(Role::Button, etichetta).accesskit_node().is_disabled(), "«{etichetta}» è spenta");
        }
        assert!(h.query_by_label("foto.jpg").is_none(), "«foto.jpg» si vede");
        tocca(&mut h, "Lavoro");
        tocca(&mut h, "..");
        assert_eq!(h.state().home.cartella_aperta(), Some(d.join("radice/Download").as_path()));
        // Nella radice non c'è «..»: sopra non si va.
        tocca(&mut h, "..");
        assert_eq!(h.state().home.cartella_aperta(), Some(d.join("radice").as_path()));
        h.get_by_label("Memoria del telefono");
        assert!(h.query_by_role_and_label(Role::Button, "..").is_none());
        tocca(&mut h, "Download");
        tocca(&mut h, "Lavoro");
        tocca(&mut h, "Relazione.docx");
        assert_eq!(testi(h.state()), ["Relazione"]);
    }

    /// La Home sul telefono dell'autore (372 dp): col chip «Fogli» la fila non stava più e allargava
    /// tutta la Home oltre lo schermo (screenshot del 3/11). Ora la fila dei chip scorre e il resto
    /// sta dentro.
    #[test]
    fn la_home_sta_nello_schermo_di_davide() {
        let misura = egui::vec2(372.0, 827.0);
        let (piattaforma, d) = piattaforma_di_prova("home-372");
        std::fs::create_dir_all(d.join("radice/Download")).unwrap();
        std::fs::write(d.join("radice/Download/Bolletta.pdf"), b"%PDF-1.7").unwrap();
        let h = telefono_di(TascabileApp::new(piattaforma, Instant::now()), misura, 3.0);
        let schermo = egui::Rect::from_min_size(egui::Pos2::ZERO, misura);
        for b in ["Impostazioni", "Nuovo", "Tutti", "PDF", "Download"] {
            let r = h.get_by_role_and_label(Role::Button, b).rect();
            assert!(schermo.contains_rect(r), "«{b}» esce dallo schermo: {r:?}");
        }
        assert!(h.get_by_value(home::CERCA).rect().right() <= 372.0);
    }

    /// Le due viste di una cartella e il Cerca (l'autore, 3/11): «Tutti i file» mostra anche quelli
    /// delle sottocartelle, con la sottocartella; la scelta resta per le volte dopo; il Cerca
    /// apre il campo e trova anche nelle sottocartelle; Indietro lo chiude e lo svuota.
    #[test]
    fn una_cartella_con_tutti_i_file_insieme_e_il_cerca() {
        let (piattaforma, d) = piattaforma_di_prova("cartella-viste");
        let sotto = d.join("radice/Download/Lavoro");
        std::fs::create_dir_all(&sotto).unwrap();
        std::fs::write(sotto.join("Relazione.docx"), b"x").unwrap();
        std::fs::write(d.join("radice/Download/Bolletta.pdf"), b"%PDF-1.7").unwrap();
        std::fs::write(d.join("radice/Download/foto.jpg"), b"x").unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        tocca(&mut h, "Download");
        assert!(acceso(&h, home::VISTA_CARTELLE) && !acceso(&h, home::VISTA_TUTTI));
        assert!(h.query_by_label("Relazione.docx").is_none());
        tocca(&mut h, home::VISTA_TUTTI);
        h.get_by_role_and_label(Role::Button, "Relazione.docx");
        assert!(h.query_by_label("Lavoro").is_none(), "nella vista di tutti i file le cartelle non ci sono");
        assert!(h.query_by_label("foto.jpg").is_none());
        assert_eq!(std::fs::read_to_string(d.join("app").join(home::FILE_VISTA)).unwrap(), "tutti");
        // Il Cerca: il campo, una parola, solo quello che la contiene.
        tocca(&mut h, home::CERCA_PULSANTE);
        h.run_steps(2);
        h.get_by_value(home::CERCA_IN_CARTELLA);
        scrivi(&mut h, "rel");
        h.get_by_role_and_label(Role::Button, "Relazione.docx");
        assert!(h.query_by_label("Bolletta.pdf").is_none());
        // Indietro: prima la tastiera, poi la ricerca, poi la cartella.
        indietro(&mut h);
        indietro(&mut h);
        h.run_steps(2);
        assert!(h.query_by_value(home::CERCA_IN_CARTELLA).is_none());
        h.get_by_role_and_label(Role::Button, "Bolletta.pdf");
        // Uscendo e tornando, la vista resta «Tutti i file».
        indietro(&mut h);
        tocca(&mut h, "Download");
        assert!(acceso(&h, home::VISTA_TUTTI));
        let diag = diagnosi_della_prova();
        assert!(diag.contains("] cartella: vista tutti i file\n") && diag.contains("] cartella: ricerca aperta\n"), "{diag}");
        assert!(!diag.contains("Relazione") && !diag.contains("rel\n"), "{diag}");
    }

    #[test]
    fn dialogo_delle_modifiche_non_salvate() {
        let mut h = telefono(app_di_prova("dialogo"));
        nuovo_dalla_home(&mut h);
        assert_eq!(h.state().schermata, Schermata::Editor);
        apri_tastiera(h.state_mut());
        scrivi(&mut h, "Zanzibar");
        assert_eq!(testi(h.state()), ["Zanzibar"]);
        // Il primo Indietro chiude la tastiera, come prima; il secondo chiede.
        indietro(&mut h);
        assert!(!h.state().piattaforma.keyboard_visible());
        assert!(h.query_by_label("Salvare le modifiche?").is_none());
        indietro(&mut h);
        h.get_by_label("Salvare le modifiche?");
        assert_eq!(famiglia_del_testo(&h, "Salvare le modifiche?"), Some(Carattere::Titolo.famiglia()), "non è il dialogo del tema");
        // Salva c'è e si tocca (la prova del salvataggio dal dialogo è più sotto).
        assert!(!h.get_by_role_and_label(Role::Button, "Salva").accesskit_node().is_disabled());
        tocca(&mut h, "Annulla");
        assert_eq!(h.state().schermata, Schermata::Editor);
        assert!(h.query_by_label("Salvare le modifiche?").is_none());
        assert_eq!(testi(h.state()), ["Zanzibar"]);
        // Indietro col dialogo aperto vale Annulla, e non manda l'app in secondo piano.
        indietro(&mut h);
        indietro(&mut h);
        assert_eq!((h.state().schermata, h.query_by_label("Salvare le modifiche?").is_none()), (Schermata::Editor, true));
        // Il bottone indietro della testata chiede anche lui; Non salvare porta alla Home.
        tocca(&mut h, schermo_editor::INDIETRO);
        tocca(&mut h, "Non salvare");
        assert_eq!(h.state().schermata, Schermata::Home);
        assert!(h.state().editor().is_none());
        let d = diagnosi_della_prova();
        assert!(d.contains("] salvare le modifiche: chiesto\n") && d.contains("] salvare le modifiche: non salvare\n"), "{d}");
        assert!(!d.contains("Zanzibar"), "{d}");
    }

    #[test]
    fn gli_errori_si_dicono_con_un_dialogo() {
        let (piattaforma, d) = piattaforma_di_prova("errore-dialogo");
        std::fs::create_dir_all(d.join("radice/Download")).unwrap();
        std::fs::write(d.join("radice/Download/Rotto.docx"), [0u8, 1, 2, 3].repeat(50)).unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        tocca(&mut h, "Download");
        tocca(&mut h, "Rotto.docx");
        // Il dialogo del tema: titolo in Fraunces, il messaggio, «Ho capito», sopra il velo scuro.
        assert_eq!(famiglia_del_testo(&h, NON_SI_APRE), Some(Carattere::Titolo.famiglia()));
        h.get_by_label_contains("Non riesco ad aprire il file: ");
        assert!(!riempiti(&h, tema::VELO_SCURO).is_empty());
        // Indietro lo chiude e resta nella cartella; «Ho capito» anche.
        indietro(&mut h);
        assert!(h.query_by_label(NON_SI_APRE).is_none() && h.state().errore.is_none());
        assert_eq!(h.state().piattaforma.volte_in_secondo_piano(), 0);
        assert!(h.state().home.cartella_aperta().is_some());
        tocca(&mut h, "Rotto.docx");
        tocca(&mut h, HO_CAPITO);
        assert!(h.query_by_label(NON_SI_APRE).is_none());
        assert_eq!(h.state().schermata, Schermata::Home);
    }

    #[test]
    fn gli_esiti_di_salva_copia_e_pdf_negli_avvisi_brevi() {
        let (mut h, d) = telefono_con_un_docx_modificato("avvisi-brevi", "Verbale.docx");
        tocca(&mut h, schermo_editor::MENU);
        tocca(&mut h, "Salva");
        // In basso, sopra la barra di formato, nella pillola scura.
        let avviso = h.get_by_label("Salvato").rect();
        let barra = h.get_by_role_and_label(Role::Button, "Grassetto").rect();
        assert!(avviso.bottom() <= barra.top() - 8.0 && avviso.top() > 600.0, "{avviso:?} {barra:?}");
        assert!(riempiti(&h, tema::SCURO).iter().any(|r| r.contains_rect(avviso)));
        // Non si prende i tocchi: sotto, la pagina e la barra restano usabili.
        assert_eq!(colore_del_testo(&h, "Salvato"), Some(tema::FOGLIO));
        tocca(&mut h, schermo_editor::MENU);
        tocca(&mut h, "Esporta in PDF");
        h.get_by_label("PDF pronto in Documenti/Tascabile: Verbale.pdf");
        assert!(h.query_by_label("Salvato").is_none(), "l'avviso nuovo prende il posto di quello di prima");
        tocca(&mut h, CONDIVIDI_PDF);
        let condivisi: Vec<_> = std::fs::read_dir(d.join("cache/condivisi")).unwrap().flatten().collect();
        assert!(condivisi.len() == 1 && condivisi[0].path().join("Verbale.pdf").is_file());
        assert!(h.query_by_label_contains("PDF pronto").is_none(), "toccata l'azione, l'avviso se ne va");
        tocca(&mut h, schermo_editor::MENU);
        tocca(&mut h, "Salva una copia");
        h.get_by_label("Copia salvata in Documenti/Tascabile: Verbale.docx");
        // Sotto la barra alta niente righe di messaggi.
        assert!(h.query_by_label_contains("Salvato in").is_none());
    }

    #[test]
    fn senza_modifiche_si_esce_senza_chiedere() {
        let mut h = telefono(app_di_prova("senza-modifiche"));
        nuovo_dalla_home(&mut h);
        indietro(&mut h);
        assert_eq!(h.state().schermata, Schermata::Home);
        // Le prove non chiedono mai: la checklist resta da parte con le risposte, il documento
        // lungo si butta.
        dalle_impostazioni(&mut h, "Checklist della tastiera");
        apri_tastiera(h.state_mut());
        scrivi(&mut h, "sì");
        indietro(&mut h);
        indietro(&mut h);
        assert_eq!(h.state().schermata, Schermata::Home);
        dalle_impostazioni(&mut h, "Checklist della tastiera");
        assert_eq!(testi(h.state()).last().map(String::as_str), Some("sì"));
        indietro(&mut h);
        dalle_impostazioni(&mut h, "Documento lungo");
        apri_tastiera(h.state_mut());
        scrivi(&mut h, "x");
        indietro(&mut h);
        indietro(&mut h);
        assert_eq!(h.state().schermata, Schermata::Home);
    }

    #[test]
    fn apri_con_su_un_documento_modificato_chiede_prima() {
        let (piattaforma, d) = piattaforma_di_prova("apri-con-modificato");
        let arrivato = d.join("Arrivato.txt");
        std::fs::write(&arrivato, "arrivato da WhatsApp").unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        nuovo_dalla_home(&mut h);
        apri_tastiera(h.state_mut());
        scrivi(&mut h, "da non perdere");
        h.state().piattaforma.simula_apri_con(arrivato.to_str().unwrap(), Some("text/plain"));
        h.run_steps(2);
        h.get_by_label("Salvare le modifiche?");
        assert_eq!(testi(h.state()), ["da non perdere"]);
        tocca(&mut h, "Non salvare");
        assert_eq!(testi(h.state()), ["arrivato da WhatsApp"]);
        assert_eq!(h.state().home.recenti()[0].chiave, arrivato.to_str().unwrap());
        // Con Annulla il documento resta e l'«Apri con» si lascia perdere.
        apri_tastiera(h.state_mut());
        scrivi(&mut h, "!");
        h.state().piattaforma.simula_apri_con(arrivato.to_str().unwrap(), Some("text/plain"));
        h.run_steps(2);
        tocca(&mut h, "Annulla");
        // Il cursore di un documento appena aperto è all'inizio.
        assert_eq!(testi(h.state()), ["!arrivato da WhatsApp"]);
        assert_eq!(h.state().schermata, Schermata::Editor);
    }

    #[test]
    fn permesso_mancante_mostra_il_riquadro() {
        let (piattaforma, d) = piattaforma_di_prova("permesso");
        std::fs::create_dir_all(d.join("radice/Download")).unwrap();
        piattaforma.simula_accesso_file(false);
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        h.get_by_label("Serve l'accesso ai file");
        assert!(h.query_by_label("Cartelle").is_none() && h.query_by_role_and_label(Role::Button, "Download").is_none());
        tocca(&mut h, "Concedi");
        assert!(diagnosi_della_prova().contains("] accesso ai file: chiesto; sul PC non c'è la schermata delle impostazioni\n"));
        // L'utente lo concede nelle impostazioni: l'app perde il fuoco e lo ritrova.
        h.input_mut().focused = false;
        h.run_steps(2);
        h.state().piattaforma.simula_accesso_file(true);
        assert!(h.query_by_label("Cartelle").is_none(), "ricontrollato prima del ritorno");
        h.input_mut().focused = true;
        h.run_steps(2);
        assert!(h.query_by_label("Serve l'accesso ai file").is_none());
        h.get_by_label("Cartelle");
        h.get_by_role_and_label(Role::Button, "Download");
        // Fra gli eventi, non nella testa: la testa è di tutto il processo e le altre prove, in
        // parallelo, ci scrivono il loro accesso all'avvio (che resti in testa lo prova la
        // piattaforma).
        let d = diagnosi_della_prova();
        assert!(d.contains("] accesso ai file al ritorno nell'app: concesso\n"), "{d}");
    }

    #[test]
    fn la_cartella_si_rilegge_solo_nella_home() {
        // Ritrovare il fuoco nell'Editor (dalla tendina, dal foglio di condivisione) non rilegge
        // la cartella aperta, che può essere grande; tornando nella Home sì.
        let (piattaforma, d) = piattaforma_di_prova("rilettura");
        std::fs::create_dir_all(d.join("radice/Download")).unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        tocca(&mut h, "Download");
        h.state_mut().nuovo_documento();
        h.run_steps(2);
        docx(&d.join("radice/Download/Arrivato.docx"), &["arrivato"]);
        h.input_mut().focused = false;
        h.run_steps(1);
        h.input_mut().focused = true;
        h.run_steps(1);
        assert!(h.state().home.voci().unwrap().is_empty(), "cartella riletta nell'Editor");
        indietro(&mut h);
        h.get_by_role_and_label(Role::Button, "Arrivato.docx");
    }

    #[test]
    fn un_recente_che_non_si_apre_piu_si_toglie() {
        let (piattaforma, d) = piattaforma_di_prova("recente-scaduto");
        let file_recenti = d.join("app").join(home::FILE_RECENTI);
        let mut finti = Recenti::default();
        for (chiave, nome) in
            [("content://com.whatsapp.provider.media/item/404", "Zanzibar.docx"), ("content://com.example.posta/allegato/9", "Kilimangiaro.docx")]
        {
            finti.aggiungi(tascabile_files::Recente {
                chiave: chiave.into(),
                nome: nome.into(),
                tipo: tascabile_files::Tipo::Documento,
                aperto_alle: 1,
                pagine: None,
            });
        }
        finti.salva(&file_recenti).unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        // Sul PC l'URI non si legge (NotFound), come sul telefono un permesso scaduto.
        tocca(&mut h, "Zanzibar");
        h.get_by_label("Non si apre più: riaprilo da WhatsApp.");
        assert!(h.query_by_label_contains("Zanzibar").is_none());
        tocca(&mut h, HO_CAPITO);
        tocca(&mut h, "Kilimangiaro");
        h.get_by_label("Non si apre più: riaprilo dall'app da cui era arrivato.");
        assert!(Recenti::carica(&file_recenti).voci().is_empty());
        assert_eq!(h.state().schermata, Schermata::Home);
        let diag = diagnosi_della_prova();
        assert!(diag.contains("] apri dai recenti: errore di lettura (NotFound), tipo docx\n"), "{diag}");
        for parola in ["Zanzibar", "Kilimangiaro", "whatsapp", "example"] {
            assert!(!diag.contains(parola), "«{parola}» nella diagnosi:\n{diag}");
        }
    }

    /// Segnalazione dell'autore del 3/11: un recente arrivato da WhatsApp non si apriva più. Se un
    /// file con lo stesso nome è nelle cartelle del telefono, si apre da lì e il recente diventa
    /// il file.
    #[test]
    fn un_recente_che_non_si_apre_piu_si_ritrova_per_nome() {
        let (piattaforma, d) = piattaforma_di_prova("recente-ritrovato");
        let ricevuti = d.join("radice/Android/media/com.whatsapp/WhatsApp/Media/WhatsApp Documents/Private");
        std::fs::create_dir_all(&ricevuti).unwrap();
        docx(&ricevuti.join("Zanzibar.docx"), &["Ritrovato"]);
        let file_recenti = d.join("app").join(home::FILE_RECENTI);
        let mut finti = Recenti::default();
        finti.aggiungi(tascabile_files::Recente {
            chiave: "content://com.whatsapp.provider.media/item/404".into(),
            nome: "Zanzibar.docx".into(),
            tipo: tascabile_files::Tipo::Documento,
            aperto_alle: 1,
            pagine: None,
        });
        finti.salva(&file_recenti).unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        tocca(&mut h, "Zanzibar");
        assert_eq!(h.state().schermata, Schermata::Editor);
        assert!(h.state().errore.is_none());
        let voci = Recenti::carica(&file_recenti);
        assert_eq!(voci.voci().len(), 1);
        assert_eq!(voci.voci()[0].chiave, ricevuti.join("Zanzibar.docx").to_str().unwrap());
        assert!(diagnosi_della_prova().contains("] recenti: un documento arrivato da un'altra app ritrovato nelle cartelle del telefono\n"));
    }

    /// Segnalato dall'autore il 1/11: dopo l'aggiornamento dell'app dei file «non esistevano più» e al
    /// tocco dopo si aprivano. Un file che non si trova resta nei recenti, con o senza l'accesso ai
    /// file; si toglie dal suo menu.
    #[test]
    fn un_file_recente_che_non_si_trova_resta_nei_recenti() {
        let (piattaforma, d) = piattaforma_di_prova("recente-sparito");
        let file_recenti = d.join("app").join(home::FILE_RECENTI);
        let sparito = d.join("radice/Zanzibar.docx");
        let mut finti = Recenti::default();
        finti.aggiungi(tascabile_files::Recente {
            chiave: sparito.to_string_lossy().into_owned(),
            nome: "Zanzibar.docx".into(),
            tipo: tascabile_files::Tipo::Documento,
            aperto_alle: 1,
            pagine: None,
        });
        finti.salva(&file_recenti).unwrap();
        piattaforma.simula_accesso_file(false);
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        tocca(&mut h, "Zanzibar");
        h.get_by_label(NON_TROVATO);
        tocca(&mut h, HO_CAPITO);
        assert_eq!(Recenti::carica(&file_recenti).voci().len(), 1);
        h.state().piattaforma.simula_accesso_file(true);
        h.input_mut().focused = false;
        h.run_steps(1);
        h.input_mut().focused = true;
        h.run_steps(1);
        tocca(&mut h, "Zanzibar");
        h.get_by_label(NON_TROVATO);
        assert_eq!(Recenti::carica(&file_recenti).voci().len(), 1);
        let d = tascabile_platform::diagnosi_testo();
        assert!(d.contains("] recenti: file non trovato, lasciato nei recenti\n") && !d.contains("Zanzibar"), "{d}");
    }

    #[test]
    fn la_diagnosi_si_legge_e_si_condivide() {
        let mut h = telefono(app_di_prova("diagnosi"));
        dalle_impostazioni(&mut h, "Registro degli eventi");
        assert_eq!(h.state().schermata, Schermata::Diagnosi);
        h.get_by_label_contains("Tempi: secondi dall'avvio");
        // La misura dell'avvio, che prima stava nella Home, è qui.
        h.get_by_label_contains("avvio fino al primo disegno ");
        tocca(&mut h, "Condividi diagnosi");
        assert!(diagnosi_della_prova().contains("] condividi testo: "));
        // Indietro torna alle Impostazioni, da dove si era arrivati.
        indietro(&mut h);
        assert_eq!(h.state().schermata, Schermata::Impostazioni);
        // Anche con la freccia della barra.
        tocca(&mut h, "Registro degli eventi");
        tocca(&mut h, "Indietro");
        assert_eq!(h.state().schermata, Schermata::Impostazioni);
        indietro(&mut h);
        assert_eq!(h.state().schermata, Schermata::Home);
    }

    /// Un .docx in Download, aperto dalla cartella nell'Editor e modificato con la tastiera finta.
    fn telefono_con_un_docx_modificato(nome: &str, file: &str) -> (Telefono, PathBuf) {
        let (piattaforma, d) = piattaforma_di_prova(nome);
        let cartella = d.join("radice/Download");
        std::fs::create_dir_all(&cartella).unwrap();
        docx(&cartella.join(file), &["Verbale", "Kilimangiaro"]);
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        tocca(&mut h, "Download");
        tocca(&mut h, file);
        apri_tastiera(h.state_mut());
        scrivi(&mut h, " Zanzibar");
        assert!(editor(h.state()).s.dirty);
        (h, d)
    }

    #[test]
    fn salva_dal_dialogo_e_si_riapre_uguale() {
        let (mut h, d) = telefono_con_un_docx_modificato("salva-dal-dialogo", "Segreto.docx");
        let prima = testi(h.state());
        assert!(prima.concat().contains("Zanzibar"));
        indietro(&mut h);
        indietro(&mut h);
        tocca(&mut h, "Salva");
        assert_eq!(h.state().schermata, Schermata::Home);
        let file = d.join("radice/Download/Segreto.docx");
        let s = documento::apri_byte("Segreto.docx", &std::fs::read(&file).unwrap()).unwrap();
        let testi: Vec<String> = s.doc.body.iter().filter_map(|b| b.as_para()).map(|p| p.text.clone()).collect();
        assert_eq!(testi, prima);
        // Solo il file, niente temporanei accanto.
        assert_eq!(std::fs::read_dir(d.join("radice/Download")).unwrap().count(), 1);
        let diag = diagnosi_della_prova();
        let byte = std::fs::metadata(&file).unwrap().len();
        assert!(diag.contains(&format!("] salva: riuscito, tipo docx, {byte} byte\n")), "{diag}");
        for parola in ["Segreto", "Zanzibar", "Kilimangiaro"] {
            assert!(!diag.contains(parola), "«{parola}» nella diagnosi:\n{diag}");
        }
    }

    #[test]
    fn il_menu_salva_copia_pdf_e_condivide() {
        let (mut h, d) = telefono_con_un_docx_modificato("menu", "Riservato.docx");
        tocca(&mut h, schermo_editor::MENU);
        tocca(&mut h, "Esporta in PDF");
        let pdf = d.join("radice/Documents/Tascabile/Riservato.pdf");
        assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF"));
        // Ogni condivisione nella sua sottocartella di condivisi/, che resta una sola.
        let condiviso = |nome: &str| {
            let voci: Vec<_> = std::fs::read_dir(d.join("cache/condivisi")).unwrap().flatten().collect();
            voci.len() == 1 && voci[0].path().join(nome).is_file()
        };
        tocca(&mut h, CONDIVIDI_PDF);
        assert!(condiviso("Riservato.pdf"));
        tocca(&mut h, schermo_editor::MENU);
        tocca(&mut h, "Condividi");
        assert!(condiviso("Riservato.docx"));
        assert!(editor(h.state()).s.dirty, "condividere non salva");
        tocca(&mut h, schermo_editor::MENU);
        tocca(&mut h, "Salva una copia");
        let copia = d.join("radice/Documents/Tascabile/Riservato.docx");
        assert!(copia.is_file());
        assert_eq!(editor(h.state()).origine, Origine::File(copia.clone()));
        assert!(!editor(h.state()).s.dirty);
        assert_eq!(h.state().home.recenti()[0].chiave, copia.to_str().unwrap());
        let diag = diagnosi_della_prova();
        for riga in [
            "] esporta pdf: riuscito, tipo pdf, ",
            "] condividi pdf: riuscito, tipo pdf, ",
            "] condividi: riuscito, tipo docx, ",
            "] condividi file: ",
        ] {
            assert!(diag.contains(riga), "manca «{riga}»:\n{diag}");
        }
        assert!(!diag.contains("Riservato"), "{diag}");
    }

    #[test]
    fn un_salvataggio_che_non_riesce_e_uno_che_riesce_nella_diagnosi() {
        let (piattaforma, d) = piattaforma_di_prova("salvataggi");
        // Un file al posto della cartella Documents: la copia di un documento nuovo non si scrive.
        std::fs::write(d.join("radice/Documents"), b"").unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        nuovo_dalla_home(&mut h);
        apri_tastiera(h.state_mut());
        scrivi(&mut h, "Relazione Zanzibar");
        tocca(&mut h, schermo_editor::MENU);
        tocca(&mut h, "Salva");
        h.get_by_label_contains("Non riesco a salvare: ");
        h.get_by_label(NON_SALVATO);
        tocca(&mut h, HO_CAPITO);
        assert!(editor(h.state()).s.dirty);
        // Le righe di questa prova, non la testa del registro: quella è una sola per tutte le prove
        // in parallelo, e un'altra che salva o condivide ci scrive sopra (la riga fissa in testa la
        // provano le prove della diagnosi).
        let diag = diagnosi_della_prova();
        assert!(
            diag.contains("] salva: non riuscito (NotADirectory), tipo docx\n") || diag.contains("non riuscito (AlreadyExists), tipo docx"),
            "{diag}"
        );
        std::fs::remove_file(d.join("radice/Documents")).unwrap();
        tocca(&mut h, schermo_editor::MENU);
        tocca(&mut h, "Salva");
        let file = d.join("radice/Documents/Tascabile/Relazione Zanzibar.docx");
        let byte = std::fs::metadata(&file).unwrap().len();
        let diag = diagnosi_della_prova();
        assert!(diag.contains(&format!("] salva: riuscito, tipo docx, {byte} byte\n")), "{diag}");
        assert!(!diag.contains("Zanzibar") && !diag.contains("Relazione"), "{diag}");
    }

    #[test]
    fn apri_con_prende_il_nome_vero() {
        let (piattaforma, d) = piattaforma_di_prova("nome-vero");
        let file = d.join("item-123");
        docx(&file, &["Referto"]);
        let mut app = TascabileApp::new(piattaforma, Instant::now());
        let docx_mime = "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
        app.piattaforma.simula_apri_con_col_nome(file.to_str().unwrap(), Some(docx_mime), "Referto Zanzibar");
        let ctx = egui::Context::default();
        passo(&mut app, &ctx, vec![]);
        assert_eq!(editor(&app).nome, "Referto Zanzibar.docx");
        assert_eq!(app.home.recenti()[0].nome, "Referto Zanzibar.docx");
        // La copia prende quel nome.
        assert!(app.esegui(Comando::SalvaCopia));
        assert!(d.join("radice/Documents/Tascabile/Referto Zanzibar.docx").is_file());
        // Al posto dell'URI, nei recenti c'è la copia.
        assert_eq!(app.home.recenti().len(), 1);
        let d = diagnosi_della_prova();
        assert!(d.contains("] apri con: richiesta, tipo application/vnd.openxmlformats-officedocument.wordprocessingml.document, col nome\n"), "{d}");
        assert!(!d.contains("Zanzibar"), "{d}");
    }

    #[test]
    fn indietro_nella_home_ripercorre_le_cartelle() {
        let (piattaforma, d) = piattaforma_di_prova("indietro-cartelle");
        std::fs::create_dir_all(d.join("radice/Download/Lavoro")).unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        tocca(&mut h, "Download");
        tocca(&mut h, "Lavoro");
        indietro(&mut h);
        assert_eq!(h.state().home.cartella_aperta(), Some(d.join("radice/Download").as_path()));
        // Dalla cartella aperta dalla Home si torna alla Home.
        indietro(&mut h);
        assert_eq!((h.state().schermata, h.state().home.cartella_aperta()), (Schermata::Home, None));
        assert_eq!(h.state().piattaforma.volte_in_secondo_piano(), 0);
        // Dalla Home va in secondo piano (sul PC si conta e basta) e resta lì.
        indietro(&mut h);
        assert_eq!(h.state().piattaforma.volte_in_secondo_piano(), 1);
        assert_eq!(h.state().schermata, Schermata::Home);
    }

    /// Fa girare `app` finché il recupero all'avvio non ha letto le copie.
    fn aspetta_il_recupero(app: &mut TascabileApp, ctx: &egui::Context) {
        let fine = Instant::now() + std::time::Duration::from_secs(10);
        while !app.recupero.pronto() {
            assert!(Instant::now() < fine, "il recupero non ha letto le copie");
            passo(app, ctx, vec![]);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    #[test]
    fn la_copia_dell_app_si_ritrova_dopo_un_uscita_a_forza() {
        let (piattaforma, d) = piattaforma_di_prova("recupero-app");
        let ctx = egui::Context::default();
        let mut app = TascabileApp::new(piattaforma, Instant::now());
        aspetta_il_recupero(&mut app, &ctx);
        assert!(app.recupero.proposte.is_empty());
        app.nuovo_documento();
        apri_tastiera(&mut app);
        app.piattaforma.simula_testo("Lavoro Zanzibar");
        passo(&mut app, &ctx, vec![]);
        assert_eq!(testi(&app), ["Lavoro Zanzibar"]);
        // L'app perde il fuoco: la copia privata; poi il processo muore senza salvare.
        let mut out = ctx.run_ui(egui::RawInput { focused: false, ..Default::default() }, |ui| app.passo_logica(ui.ctx()));
        out.textures_delta.clear();
        assert_eq!(std::fs::read_dir(d.join("app/ripristino")).unwrap().count(), 2, "copia e dati");
        drop(app);
        std::thread::sleep(std::time::Duration::from_millis(200));
        let piattaforma = Piattaforma::con_cartelle(d.join("app"), d.join("cache"), d.join("radice"));
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        let fine = Instant::now() + std::time::Duration::from_secs(10);
        while !h.state().recupero.pronto() {
            assert!(Instant::now() < fine);
            h.run_steps(1);
        }
        h.run_steps(2);
        // Il dialogo del tema sopra la Home: il nome del documento, Scarta e Riapri; un tocco
        // fuori non lo chiude, si sceglie.
        h.get_by_label("Lavoro da recuperare");
        assert_eq!(famiglia_del_testo(&h, "Lavoro da recuperare"), Some(Carattere::Titolo.famiglia()));
        h.get_by_label_contains("«Documento nuovo»");
        h.get_by_role_and_label(Role::Button, "Scarta");
        tocca_in(&mut h, egui::pos2(20.0, 30.0));
        h.run_steps(2);
        h.get_by_label("Lavoro da recuperare");
        tocca(&mut h, "Riapri");
        assert_eq!(h.state().schermata, Schermata::Editor);
        assert_eq!(testi(h.state()), ["Lavoro Zanzibar"]);
        assert!(editor(h.state()).s.dirty);
        // Salvato, la copia privata se ne va.
        assert!(h.state_mut().esegui(Comando::Salva));
        h.run_steps(2);
        assert_eq!(std::fs::read_dir(d.join("app/ripristino")).unwrap().count(), 0);
        assert!(d.join("radice/Documents/Tascabile/Lavoro Zanzibar.docx").is_file());
        assert!(!tascabile_platform::diagnosi_testo().contains("Zanzibar"));
    }

    /// Un PDF modificato e l'app chiusa a forza: quando perde il fuoco il campo che si scrive va al
    /// motore e la copia di recupero coi byte modificati va nella cartella dell'app (scritta dal
    /// thread del motore). Alla riapertura la Home la propone; riaperto, il PDF ha la modifica e
    /// va salvato (in una copia in Documenti/Tascabile, l'originale resta com'era); salvato, la
    /// copia se ne va.
    #[test]
    fn un_pdf_modificato_si_ritrova_dopo_un_uscita_a_forza() {
        use tascabile_pdf::prova::CampoDiProva as C;
        let byte = tascabile_pdf::prova::modulo_da_compilare();
        let (piattaforma, d) = piattaforma_di_prova("recupero-pdf");
        let percorso = d.join("radice/Consenso Zanzibar.pdf");
        std::fs::write(&percorso, &byte).unwrap();
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        h.state_mut().apri_file(&percorso, "Consenso Zanzibar.pdf");
        aspetta_il_lettore(&mut h);
        h.get_by_label(C::Luogo.suggerimento()).click();
        h.run_steps(2);
        scrivi(&mut h, "Torino");
        // L'app perde il fuoco: il valore va al motore (la scrittura resta), poi la copia.
        h.input_mut().focused = false;
        h.run_steps(1);
        h.input_mut().focused = true;
        aspetta_il_motore(&mut h);
        assert!(h.state().pdf_ha_la_copia(), "la copia non c'è");
        assert_eq!(h.state().pdf_in_scrittura().as_deref(), Some("Torino"));
        let copie = d.join("app/ripristino-pdf");
        assert_eq!(std::fs::read_dir(&copie).unwrap().count(), 2, "copia e dati");
        // Il processo muore senza salvare.
        drop(h);
        let piattaforma = Piattaforma::con_cartelle(d.join("app"), d.join("cache"), d.join("radice"));
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        let fine = Instant::now() + std::time::Duration::from_secs(10);
        while !h.state().recupero.pronto() {
            assert!(Instant::now() < fine);
            h.run_steps(1);
        }
        h.run_steps(2);
        h.get_by_label("Lavoro da recuperare");
        h.get_by_label_contains("«Consenso Zanzibar»");
        tocca(&mut h, "Riapri");
        aspetta_il_lettore(&mut h);
        assert_eq!(h.state().schermata, Schermata::Pdf);
        assert_eq!(campo_del_lettore(&h, C::Luogo).testo(), "Torino");
        h.get_by_label("1 da compilare · non salvato");
        assert!(h.state().modifiche_da_salvare(), "un pdf recuperato si chiude senza chiedere");
        // Salva: una copia in Documenti/Tascabile; l'originale com'era; la copia di recupero via.
        tocca(&mut h, schermo_pdf::MENU);
        tocca(&mut h, pdf_lavoro::SALVA);
        aspetta_il_motore(&mut h);
        h.run_steps(2);
        let salvato = d.join("radice/Documents/Tascabile/Consenso Zanzibar.pdf");
        let campi = tascabile_pdf::Pdf::apri(std::fs::read(&salvato).unwrap(), None).unwrap().campi();
        assert_eq!(campi.iter().find(|c| c.suggerimento.as_deref() == Some(C::Luogo.suggerimento())).unwrap().testo(), "Torino");
        assert_eq!(std::fs::read(&percorso).unwrap(), byte, "l'originale è cambiato");
        assert_eq!(std::fs::read_dir(&copie).unwrap().count(), 0, "la copia è rimasta");
        assert!(!h.state().modifiche_da_salvare());
        let d = diagnosi_della_prova();
        assert!(d.contains("] recupero: copia del pdf scritta, ") && d.contains("] recupero: 1 copie di pdf\n"), "{d}");
        assert!(!d.contains("Zanzibar") && !d.contains("Torino"), "{d}");
    }

    /// La copia del PDF si scrive anche con l'app in primo piano, poco dopo l'ultima modifica; se
    /// ne va annullando fino al PDF di prima e uscendo con «Non salvare».
    #[test]
    fn la_copia_del_pdf_poco_dopo_la_modifica_e_via_con_non_salvare() {
        let byte = tascabile_pdf::prova::modulo();
        let (piattaforma, d) = piattaforma_di_prova("recupero-pdf-intervalli");
        let percorso = d.join("radice/Modulo.pdf");
        std::fs::write(&percorso, &byte).unwrap();
        let mut app = TascabileApp::new(piattaforma, Instant::now());
        app.ritardo_copia = std::time::Duration::from_millis(50);
        let mut h = telefono(app);
        h.state_mut().apri_file(&percorso, "Modulo.pdf");
        aspetta_il_lettore(&mut h);
        let copie = d.join("app/ripristino-pdf");
        let aspetta_la_copia = |h: &mut Telefono, c_e: bool| {
            let fine = Instant::now() + std::time::Duration::from_secs(10);
            while h.state().pdf_ha_la_copia() != c_e || std::fs::read_dir(&copie).map_or(0, |v| v.count()) != if c_e { 2 } else { 0 } {
                assert!(Instant::now() < fine, "la copia non è {}", if c_e { "arrivata" } else { "andata via" });
                std::thread::sleep(std::time::Duration::from_millis(10));
                h.state_mut().aspetta_il_motore_pdf();
                h.run_steps(1);
            }
        };
        ruota_dagli_strumenti(&mut h);
        aspetta_la_copia(&mut h, true);
        // Annullando, il PDF torna com'era: la copia non serve.
        tocca(&mut h, pdf_lavoro::ANNULLA_MODIFICA);
        aspetta_la_copia(&mut h, false);
        // Di nuovo, poi «Non salvare».
        ruota_dagli_strumenti(&mut h);
        aspetta_la_copia(&mut h, true);
        indietro(&mut h);
        tocca(&mut h, "Non salvare");
        assert_eq!(h.state().schermata, Schermata::Home);
        assert_eq!(std::fs::read_dir(&copie).unwrap().count(), 0, "la copia è rimasta dopo «Non salvare»");
        assert_eq!(std::fs::read(&percorso).unwrap(), byte);
    }

    #[test]
    fn la_copia_dell_app_si_scrive_anche_col_fuoco_poco_dopo_l_ultima_modifica() {
        // Su Android dopo onPause non gira più nessun frame: la copia non può aspettare il fuoco
        // perso, si scrive poco dopo l'ultima modifica.
        let (piattaforma, d) = piattaforma_di_prova("recupero-intervalli");
        let ctx = egui::Context::default();
        let mut app = TascabileApp::new(piattaforma, Instant::now());
        app.ritardo_copia = std::time::Duration::from_millis(100);
        app.nuovo_documento();
        apri_tastiera(&mut app);
        app.piattaforma.simula_testo("Prima Zanzibar");
        passo(&mut app, &ctx, vec![]);
        let copie = d.join("app/ripristino");
        assert!(!copie.exists() || std::fs::read_dir(&copie).unwrap().count() == 0, "subito no, si aspetta il ritardo");
        let scritta = |app: &mut TascabileApp, ctx: &egui::Context| {
            let fine = Instant::now() + std::time::Duration::from_secs(5);
            loop {
                passo(app, ctx, vec![]);
                if let Some(c) = app.aperto.as_ref().and_then(|a| a.copia.clone()) {
                    let s = documento::apri_byte("c.docx", &std::fs::read(&c.file).unwrap()).unwrap();
                    return s.doc.body[0].as_para().unwrap().text.clone();
                }
                assert!(Instant::now() < fine, "la copia non si è scritta");
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        };
        assert_eq!(scritta(&mut app, &ctx), "Prima Zanzibar");
        assert_eq!(std::fs::read_dir(&copie).unwrap().count(), 2, "copia e dati");
        // Una modifica dopo: la stessa copia si riscrive.
        app.piattaforma.simula_testo(" poi");
        passo(&mut app, &ctx, vec![]);
        let fine = Instant::now() + std::time::Duration::from_secs(5);
        while scritta(&mut app, &ctx) != "Prima Zanzibar poi" {
            assert!(Instant::now() < fine, "la copia non si è riscritta");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(std::fs::read_dir(&copie).unwrap().count(), 2);
        assert!(!tascabile_platform::diagnosi_testo().contains("Zanzibar"));
    }

    #[test]
    fn la_copia_del_motore_si_ritrova_e_si_scarta() {
        let (piattaforma, d) = piattaforma_di_prova("recupero-motore");
        let ctx = egui::Context::default();
        let mut app = TascabileApp::new(piattaforma, Instant::now());
        app.nuovo_documento();
        apri_tastiera(&mut app);
        app.piattaforma.simula_testo("Bozza Zanzibar");
        passo(&mut app, &ctx, vec![]);
        // Il motore scrive la sua copia dopo 10 secondi di modifiche.
        let fine = Instant::now() + std::time::Duration::from_secs(20);
        while editor(&app).s.recovery_documents().is_empty() {
            assert!(Instant::now() < fine, "il motore non ha scritto la copia");
            passo(&mut app, &ctx, vec![]);
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        // Al frame dopo la nostra copia (scritta prima, dopo la pausa) segna quella del motore come
        // sua gemella: all'avvio dopo se ne propone una sola.
        passo(&mut app, &ctx, vec![]);
        let a = app.aperto.as_ref().unwrap();
        assert_eq!(a.copia.as_ref().map(|c| c.id_motore.clone()), Some(a.id_motore()));
        assert_eq!(a.id_motore().len(), 1);
        drop(app);
        std::thread::sleep(std::time::Duration::from_millis(200));
        let piattaforma = Piattaforma::con_cartelle(d.join("app"), d.join("cache"), d.join("radice"));
        let mut app = TascabileApp::new(piattaforma, Instant::now());
        aspetta_il_recupero(&mut app, &ctx);
        assert_eq!(app.recupero.proposte.len(), 1);
        assert_eq!(app.recupero.proposte[0].fonte.nome(), "Documento nuovo");
        app.riapri_recupero(0);
        assert_eq!(testi(&app), ["Bozza Zanzibar"]);
        // «Non salvare»: la copia del motore se ne va, e al prossimo avvio non c'è più niente.
        app.indietro();
        app.scegli(Scelta::NonSalvare);
        assert_eq!(app.schermata, Schermata::Home);
        drop(app);
        std::thread::sleep(std::time::Duration::from_millis(200));
        let piattaforma = Piattaforma::con_cartelle(d.join("app"), d.join("cache"), d.join("radice"));
        let mut app = TascabileApp::new(piattaforma, Instant::now());
        aspetta_il_recupero(&mut app, &ctx);
        assert!(app.recupero.proposte.is_empty());
    }

    /// L'app sul documento «Ciao mondo» con «Ciao» selezionato, sullo schermo del telefono.
    fn telefono_con_ciao_selezionato(nome: &str) -> Telefono {
        let mut app = app_di_prova(nome);
        ciao_selezionato(&mut app);
        telefono(app)
    }

    /// Nell'Editor un documento nuovo, «Ciao mondo», con «Ciao» selezionato.
    fn ciao_selezionato(app: &mut TascabileApp) {
        app.cambia_documento(testo_semplice("prova", "Ciao mondo\nSeconda riga"));
        let e = app.editor_mut().unwrap();
        e.s.run("select.range", &serde_json::json!({ "anchor": Pos::body(0, 0), "focus": Pos::body(0, 4) })).unwrap();
        e.formato = tascabile_word::editor::Formato::di(&e.s);
    }

    #[test]
    fn commenta_sulla_selezione_col_nome_delle_impostazioni() {
        let (piattaforma, d) = piattaforma_di_prova("commenta");
        impostazioni::scrivi_autore(&d.join("app"), "Anna Bianchi").unwrap();
        let mut app = TascabileApp::new(piattaforma, Instant::now());
        ciao_selezionato(&mut app);
        assert_eq!(editor(&app).s.author, "Anna Bianchi", "l'autore delle Impostazioni");
        let mut h = telefono(app);
        h.run_steps(4);
        tocca(&mut h, "Commenta");
        h.run_steps(10);
        assert!(h.state().foglio_commento.aperto() && h.state().piattaforma.keyboard_visible());
        h.get_by_label_contains("col nome Anna Bianchi");
        // Il campo vuoto non scrive niente; Indietro chiude prima la tastiera, poi il foglio.
        assert!(h.get_by_role_and_label(Role::Button, foglio_revisioni::FATTO).accesskit_node().is_disabled());
        indietro(&mut h);
        assert!(h.state().foglio_commento.aperto() && !h.state().piattaforma.keyboard_visible());
        indietro(&mut h);
        assert!(!h.state().foglio_commento.aperto() && h.state().commento.is_none());
        assert!(editor(h.state()).s.doc.comments.is_empty());
        // Di nuovo, con Fatto: il commento sul testo scelto, col nome.
        tocca(&mut h, "Commenta");
        h.run_steps(10);
        scrivi(&mut h, "Da rivedere");
        tocca(&mut h, foglio_revisioni::FATTO);
        h.run_steps(4);
        assert!(!h.state().foglio_commento.aperto() && h.state().fuoco == Fuoco::Documento);
        let e = editor(h.state());
        let c = e.s.doc.comments.values().next().expect("il commento");
        assert_eq!(c.author, "Anna Bianchi");
        assert_eq!(e.s.doc.plain_text(wordcraft_engine::doc::StoryRef::Part(c.part)), "Da rivedere");
        assert!(e.s.sel.is_collapsed());
        // Annulla lo toglie.
        tocca(&mut h, schermo_editor::ANNULLA);
        assert!(editor(h.state()).s.doc.comments.is_empty());
        let d = diagnosi_della_prova();
        assert!(d.contains("] revisioni: commento scritto\n"), "{d}");
        assert!(!d.contains("Da rivedere") && !d.contains("Anna"), "{d}");
    }

    #[test]
    fn il_foglio_delle_revisioni_accende_la_traccia_accetta_rifiuta_risponde_e_risolve() {
        let mut h = telefono(app_di_prova("revisioni"));
        h.state_mut().cambia_documento(testo_semplice("prova", "Ciao mondo"));
        h.run_steps(4);
        tocca(&mut h, schermo_editor::MENU);
        h.run_steps(4);
        tocca(&mut h, foglio_revisioni::REVISIONI);
        h.run_steps(10);
        h.get_by_label_contains("Nessuna modifica né commento");
        assert_eq!(
            h.get_by_role_and_label(Role::Button, foglio_revisioni::TRACCIA).accesskit_node().toggled(),
            Some(egui::accesskit::Toggled::False)
        );
        tocca(&mut h, foglio_revisioni::TRACCIA);
        assert!(editor(h.state()).s.doc.settings.track_changes);
        assert_eq!(h.get_by_role_and_label(Role::Button, foglio_revisioni::TRACCIA).accesskit_node().toggled(), Some(egui::accesskit::Toggled::True));
        indietro(&mut h);
        assert!(!h.state().foglio_revisioni.aperto());
        // Due modifiche e un commento, scritti dal motore come dalla tastiera.
        {
            let e = h.state_mut().editor_mut().unwrap();
            e.s.run("caret.set", &serde_json::json!({ "pos": Pos::body(0, 10) })).unwrap();
            e.s.run("text.insert", &serde_json::json!({ "text": " bello", "raw": true })).unwrap();
            e.s.run("select.range", &serde_json::json!({ "anchor": Pos::body(0, 0), "focus": Pos::body(0, 5) })).unwrap();
            e.s.run("text.delete", &serde_json::json!({})).unwrap();
            e.s.run("select.range", &serde_json::json!({ "anchor": Pos::body(0, 5), "focus": Pos::body(0, 10) })).unwrap();
            e.commenta("Quale mondo?");
        }
        tocca(&mut h, schermo_editor::MENU);
        h.run_steps(4);
        tocca(&mut h, foglio_revisioni::REVISIONI);
        h.run_steps(10);
        h.get_by_label("Quale mondo?");
        assert_eq!(h.get_all_by_label(foglio_revisioni::ACCETTA).count(), 2);
        // Rifiutata la prima (la cancellazione di «Ciao »): resta una modifica.
        h.get_all_by_label(foglio_revisioni::RIFIUTA).next().unwrap().click();
        h.run_steps(4);
        assert_eq!(h.state().revisioni.as_ref().map(|r| r.modifiche.len()), Some(1));
        // Rispondi: il foglio della risposta, poi di nuovo le revisioni con la risposta.
        tocca(&mut h, foglio_revisioni::RISPONDI);
        h.run_steps(10);
        assert!(h.state().foglio_commento.aperto() && !h.state().foglio_revisioni.aperto());
        scrivi(&mut h, "Il nostro");
        tocca(&mut h, foglio_revisioni::FATTO);
        h.run_steps(10);
        assert!(h.state().foglio_revisioni.aperto());
        h.get_by_label("Il nostro");
        tocca(&mut h, foglio_revisioni::RISOLVI);
        assert!(editor(h.state()).s.doc.comments.values().any(|c| c.resolved));
        h.get_by_role_and_label(Role::Button, foglio_revisioni::RIAPRI);
        tocca(&mut h, foglio_revisioni::ACCETTA_TUTTO);
        assert_eq!(h.state().revisioni.as_ref().map(|r| r.modifiche.len()), Some(0));
        assert_eq!(primo(h.state()).text.replace('\u{fffc}', ""), "Ciao mondo bello");
        // Un tocco sul commento porta il cursore lì e chiude il foglio.
        h.run_steps(10);
        let commento = h.get_by_label("Quale mondo?").rect().center();
        tocca_in(&mut h, commento);
        h.run_steps(4);
        assert!(!h.state().foglio_revisioni.aperto(), "{}", diagnosi_della_prova());
        assert_eq!(editor(h.state()).s.sel.focus.off, 5, "il cursore dove comincia il commento");
        let d = diagnosi_della_prova();
        assert!(d.contains("] revisioni: modifica rifiutata\n") && d.contains("] revisioni: commento risolto\n"), "{d}");
        assert!(!d.contains("Quale mondo") && !d.contains("nostro"), "{d}");
    }

    /// Il foglio delle revisioni con una modifica e un commento, sul telefono dell'autore e su uno
    /// stretto (0.8.3: a 372 dp Rispondi, Risolvi ed Elimina allargavano il foglio, che usciva dai
    /// due lati): i pulsanti, il commento e la X stanno nello schermo.
    #[test]
    fn il_foglio_delle_revisioni_sta_nello_schermo_di_ogni_telefono() {
        for (nome, misura) in [("davide", egui::vec2(372.0, 827.0)), ("stretto", egui::vec2(360.0, 760.0))] {
            let mut h = telefono_di(app_di_prova(&format!("revisioni-{nome}")), misura, 3.0);
            h.state_mut().cambia_documento(testo_semplice("prova", "Ciao mondo"));
            h.run_steps(4);
            {
                let e = h.state_mut().editor_mut().unwrap();
                e.s.doc.settings.track_changes = true;
                e.s.run("caret.set", &serde_json::json!({ "pos": Pos::body(0, 10) })).unwrap();
                e.s.run("text.insert", &serde_json::json!({ "text": " bello", "raw": true })).unwrap();
                e.s.run("select.range", &serde_json::json!({ "anchor": Pos::body(0, 5), "focus": Pos::body(0, 10) })).unwrap();
                e.commenta("Quale mondo?");
            }
            tocca(&mut h, schermo_editor::MENU);
            h.run_steps(4);
            tocca(&mut h, foglio_revisioni::REVISIONI);
            h.run_steps(10);
            assert_eq!(h.state().revisioni.as_ref().map(|r| (r.modifiche.len(), r.commenti.len())), Some((1, 1)), "{nome}");
            let schermo = egui::Rect::from_min_size(egui::Pos2::ZERO, misura);
            tutti_visibili(
                &h,
                &[
                    foglio_revisioni::ACCETTA_TUTTO,
                    foglio_revisioni::RIFIUTA_TUTTO,
                    foglio_revisioni::RISPONDI,
                    foglio_revisioni::RISOLVI,
                    foglio_revisioni::ELIMINA,
                    "Chiudi",
                ],
                schermo,
            );
            let commento = h.get_by_label("Quale mondo?").rect();
            assert!(schermo.contains_rect(commento), "{nome}: il commento esce dallo schermo: {commento:?}");
        }
    }

    /// La riga dei colori della penna sul telefono dell'autore e su uno stretto (0.8.3: a 372 dp il
    /// tratto spesso usciva dalla pillola): i cinque colori e i tre spessori stanno nella pillola,
    /// senza sovrapporsi, e l'ultimo spessore sta dal bordo destro quanto il primo colore dal
    /// sinistro.
    #[test]
    fn la_riga_dei_colori_della_penna_sta_nella_pillola_su_ogni_telefono() {
        for (nome, misura) in [("davide", egui::vec2(372.0, 827.0)), ("stretto", egui::vec2(360.0, 760.0))] {
            let (piattaforma, dir) = piattaforma_di_prova(&format!("penna-{nome}"));
            let percorso = lettera_da_annotare(&dir);
            let mut h = telefono_di(TascabileApp::new(piattaforma, Instant::now()), misura, 3.0);
            h.state_mut().apri_file(&percorso, "Lettera Zanzibar.pdf");
            aspetta_il_lettore(&mut h);
            tocca(&mut h, "Annota");
            tocca(&mut h, pdf_annota::PENNA);
            h.run_steps(2);
            // La pillola: larga quanto lo schermo meno 14 dp per parte.
            let (sinistra, destra) = (tema::MARGINE, misura.x - tema::MARGINE);
            let nomi: Vec<&str> = pdf_annota::COLORI.iter().map(|c| c.1).chain(pdf_annota::SPESSORI.iter().map(|s| s.1)).collect();
            let posti: Vec<egui::Rect> = nomi.iter().map(|n| h.get_by_role_and_label(Role::Button, n).rect()).collect();
            for (n, r) in nomi.iter().zip(&posti) {
                assert!(r.left() >= sinistra + 2.0 && r.right() <= destra - 2.0, "{nome}: «{n}» esce dalla pillola: {r:?}");
                assert!(r.width() >= 36.0, "{nome}: «{n}» troppo stretto: {r:?}");
            }
            for (i, a) in posti.iter().enumerate() {
                for b in &posti[i + 1..] {
                    assert!(!a.shrink(0.5).intersects(b.shrink(0.5)), "{nome}: {a:?} e {b:?} si sovrappongono");
                }
            }
            let a_sinistra = posti[0].center().x - sinistra;
            let a_destra = destra - posti[posti.len() - 1].center().x;
            assert!((a_destra - a_sinistra).abs() <= 6.0, "{nome}: {a_sinistra} dp a sinistra e {a_destra} a destra");
        }
    }

    /// All'avvio l'app toglie le copie condivise più vecchie di un'ora e tiene quelle di adesso
    /// (audit di sicurezza del 30/10, problema 5).
    #[test]
    fn all_avvio_le_copie_condivise_vecchie_se_ne_vanno() {
        let (piattaforma, dir) = piattaforma_di_prova("condivisi-vecchi");
        let adesso = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
        let condivisi = dir.join("cache/condivisi");
        let (vecchia, nuova) = (condivisi.join(format!("{}-0", adesso - 3 * 3_600_000)), condivisi.join(format!("{adesso}-1")));
        for c in [&vecchia, &nuova] {
            std::fs::create_dir_all(c).unwrap();
            std::fs::write(c.join("Relazione.docx"), b"documento").unwrap();
        }
        let _app = TascabileApp::new(piattaforma, Instant::now());
        assert!(!vecchia.exists() && nuova.join("Relazione.docx").exists());
    }

    /// Revisione del 31/10, punto 12: all'avvio se ne vanno i temporanei dei salvataggi di un'app
    /// chiusa di colpo e le copie di recupero a metà; le copie intere restano, da proporre.
    #[test]
    fn all_avvio_temporanei_e_copie_a_meta_se_ne_vanno() {
        let (piattaforma, dir) = piattaforma_di_prova("temporanei-vecchi");
        let pdf = dir.join("app").join(recupero::CARTELLA_COPIE_PDF);
        let word = dir.join("app").join(recupero::CARTELLA_COPIE);
        std::fs::create_dir_all(&pdf).unwrap();
        std::fs::create_dir_all(&word).unwrap();
        let intera = [pdf.join("a.pdf"), pdf.join("a.json")];
        let a_meta = [pdf.join("b.pdf"), word.join("c.json"), pdf.join(".salvataggio-1-0.tmp"), dir.join("app/.salvataggio-2-5.tmp")];
        for f in intera.iter().chain(&a_meta) {
            std::fs::write(f, b"{}").unwrap();
        }
        let _app = TascabileApp::new(piattaforma, Instant::now());
        assert!(intera.iter().all(|f| f.exists()));
        for f in &a_meta {
            assert!(!f.exists(), "{}", f.display());
        }
    }

    /// Vero se il bottone con questa etichetta è acceso (`Button::selected`).
    fn acceso(h: &Telefono, etichetta: &str) -> bool {
        h.get_by_role_and_label(Role::Button, etichetta).accesskit_node().toggled() == Some(egui::accesskit::Toggled::True)
    }

    /// Il primo paragrafo del documento nell'Editor.
    fn primo(app: &TascabileApp) -> wordcraft_engine::doc::Paragraph {
        editor(app).s.doc.body.iter().find_map(|b| b.as_para()).unwrap().clone()
    }

    /// La famiglia del carattere con cui è disegnato il testo `testo`, se è sullo schermo.
    fn famiglia_del_testo(h: &Telefono, testo: &str) -> Option<egui::FontFamily> {
        fn cerca(s: &egui::Shape, testo: &str) -> Option<egui::FontFamily> {
            match s {
                egui::Shape::Vec(v) => v.iter().find_map(|s| cerca(s, testo)),
                egui::Shape::Text(t) if t.galley.job.text == testo => t.galley.job.sections.first().map(|s| s.format.font_id.family.clone()),
                _ => None,
            }
        }
        h.output().shapes.iter().find_map(|c| cerca(&c.shape, testo))
    }

    /// Il colore con cui è disegnato il testo `testo`, se è sullo schermo.
    fn colore_del_testo(h: &Telefono, testo: &str) -> Option<egui::Color32> {
        fn cerca(s: &egui::Shape, testo: &str) -> Option<egui::Color32> {
            match s {
                egui::Shape::Vec(v) => v.iter().find_map(|s| cerca(s, testo)),
                egui::Shape::Text(t) if t.galley.job.text == testo => {
                    let c = t.galley.job.sections.first().map_or(t.fallback_color, |s| s.format.color);
                    Some(if c == egui::Color32::PLACEHOLDER { t.fallback_color } else { c })
                }
                _ => None,
            }
        }
        h.output().shapes.iter().find_map(|c| cerca(&c.shape, testo))
    }

    /// Se il testo `testo` è disegnato coi puntini perché non ci sta (None se non è sullo
    /// schermo).
    fn tagliato(h: &Telefono, testo: &str) -> Option<bool> {
        fn cerca(s: &egui::Shape, testo: &str) -> Option<bool> {
            match s {
                egui::Shape::Vec(v) => v.iter().find_map(|s| cerca(s, testo)),
                egui::Shape::Text(t) if t.galley.job.text == testo => Some(t.galley.elided),
                _ => None,
            }
        }
        h.output().shapes.iter().find_map(|c| cerca(&c.shape, testo))
    }

    /// Il rettangolo del testo `testo` disegnato, se è sullo schermo.
    fn rettangolo_del_testo(h: &Telefono, testo: &str) -> Option<egui::Rect> {
        fn cerca(s: &egui::Shape, testo: &str) -> Option<egui::Rect> {
            match s {
                egui::Shape::Vec(v) => v.iter().find_map(|s| cerca(s, testo)),
                egui::Shape::Text(t) if t.galley.job.text == testo => Some(t.visual_bounding_rect()),
                _ => None,
            }
        }
        h.output().shapes.iter().find_map(|c| cerca(&c.shape, testo))
    }

    /// I rettangoli pieni di questo colore sullo schermo, senza le ombre.
    fn riempiti(h: &Telefono, colore: egui::Color32) -> Vec<egui::Rect> {
        fn cerca(s: &egui::Shape, colore: egui::Color32, fuori: &mut Vec<egui::Rect>) {
            match s {
                egui::Shape::Vec(v) => v.iter().for_each(|s| cerca(s, colore, fuori)),
                egui::Shape::Rect(r) if r.fill == colore && r.blur_width == 0.0 => fuori.push(r.rect),
                _ => {}
            }
        }
        let mut fuori = Vec::new();
        h.output().shapes.iter().for_each(|c| cerca(&c.shape, colore, &mut fuori));
        fuori
    }

    /// Il cursore del documento sullo schermo: il rettangolo blu sottile.
    fn cursore_a_schermo(h: &Telefono) -> Option<egui::Rect> {
        riempiti(h, tema::BLU).into_iter().find(|r| r.width() < 3.0)
    }

    #[test]
    fn le_icone_dell_editor_sono_quelle_del_tema() {
        let h = telefono_con_ciao_selezionato("icone-editor");
        for icona in [icone::ARROW_BACK, icone::UNDO, icone::REDO, icone::MORE_VERT, icone::ADD_CIRCLE]
            .into_iter()
            .chain(schermo_editor::BARRA.map(|(icona, _, _)| icona))
        {
            assert_eq!(famiglia_del_testo(&h, icona), Some(Carattere::Icona.famiglia()), "{icona:?}");
        }
        assert_eq!(famiglia_del_testo(&h, "Aa"), Some(Carattere::Titolo.famiglia()));
    }

    #[test]
    fn trova_prende_il_posto_della_barra_alta() {
        let mut h = telefono_con_trova("trova-barra");
        // Al posto della barra alta: il campo a pillola, le frecce e la X, con le icone del tema.
        assert!(h.query_by_label(schermo_editor::ANNULLA).is_none() && h.query_by_label(schermo_editor::MENU).is_none());
        let campo = h.get_by_value(schermo_editor::VUOTO_TROVA).rect();
        assert!(campo.top() >= 0.0 && campo.bottom() <= tema::BARRA_ALTA && campo.height() == tema::PULSANTE, "{campo:?}");
        let mut destra = campo.right();
        for etichetta in [schermo_editor::PRIMA, schermo_editor::DOPO, schermo_editor::CHIUDI_TROVA] {
            let r = h.get_by_role_and_label(Role::Button, etichetta).rect();
            assert!(r.left() >= destra && (r.center().y - campo.center().y).abs() < 0.5, "«{etichetta}» {r:?}");
            destra = r.right();
        }
        assert!(destra <= 412.0);
        for icona in [icone::KEYBOARD_ARROW_UP, icone::KEYBOARD_ARROW_DOWN, icone::CLOSE, icone::SEARCH] {
            assert_eq!(famiglia_del_testo(&h, icona), Some(Carattere::Icona.famiglia()), "{icona:?}");
        }
        // Col campo vuoto le frecce sono spente e non c'è il conteggio.
        assert!(h.get_by_label(schermo_editor::DOPO).accesskit_node().is_disabled());
        scrivi(&mut h, "gatto");
        assert!(!h.get_by_label(schermo_editor::DOPO).accesskit_node().is_disabled());
        assert!(campo.contains_rect(h.get_by_label("1 di 3").rect()));
        scrivi(&mut h, "x");
        h.get_by_label("Nessun risultato");
        // La X chiude Trova e torna la barra alta.
        tocca(&mut h, schermo_editor::CHIUDI_TROVA);
        assert!(h.state().trova.is_none());
        h.get_by_role_and_label(Role::Button, schermo_editor::MENU);
    }

    /// Un tocco del dito in questo punto, giù e su nello stesso frame.
    fn tocca_in(h: &mut Telefono, pos: egui::Pos2) {
        h.event(egui::Event::PointerMoved(pos));
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
        }
    }

    /// Il telefono con «gatto» tre volte nel documento e la barra di Trova aperta dal menu.
    fn telefono_con_trova(nome: &str) -> Telefono {
        let mut app = app_di_prova(nome);
        app.cambia_documento(testo_semplice("prova", "gatto e cane\nun gatto nero\nil gatto"));
        let e = app.editor_mut().unwrap();
        e.s.run("caret.set", &serde_json::json!({ "pos": Pos::body(0, 0) })).unwrap();
        let mut h = telefono(app);
        tocca(&mut h, schermo_editor::MENU);
        tocca(&mut h, "Trova");
        h
    }

    #[test]
    fn il_campo_di_trova_riceve_le_lettere_e_il_documento_no() {
        let mut h = telefono_con_trova("trova-campo");
        assert_eq!(h.state().fuoco, Fuoco::Campo);
        assert!(h.state().piattaforma.keyboard_visible(), "la tastiera si apre per il campo");
        scrivi(&mut h, "gatto");
        assert_eq!(h.state().trova.as_ref().unwrap().testo(), "gatto");
        assert_eq!(testi(h.state()), ["gatto e cane", "un gatto nero", "il gatto"], "il documento non riceve le lettere");
        assert_eq!(editor(h.state()).ricerca(), Ricerca { conteggio: 3, attuale: Some(0) });
        h.get_by_label("1 di 3");
        // Il risultato è selezionato, ma col fuoco al campo la barra Taglia, Copia… non copre il testo.
        assert!(h.query_by_role_and_label(Role::Button, "Copia").is_none(), "la barra della selezione è sopra il risultato");
        // Avanti col bottone e con l'Invio della tastiera, poi indietro.
        tocca(&mut h, schermo_editor::DOPO);
        assert_eq!(editor(h.state()).ricerca().attuale, Some(1));
        h.key_press(egui::Key::Enter);
        h.run_steps(2);
        assert_eq!(editor(h.state()).ricerca().attuale, Some(2));
        assert_eq!(h.state().trova.as_ref().unwrap().testo(), "gatto", "l'Invio non entra nel campo");
        tocca(&mut h, schermo_editor::PRIMA);
        assert_eq!(editor(h.state()).ricerca().attuale, Some(1));
        assert_eq!(editor(h.state()).s.sel.ordered(), (Pos::body(1, 3), Pos::body(1, 8)));
        let d = diagnosi_della_prova();
        assert!(d.contains("] trova: 5 caratteri, 3 risultati\n"), "{d}");
        assert!(!d.contains("gatto"), "{d}");
    }

    #[test]
    fn un_tocco_sul_testo_riporta_la_tastiera_al_documento() {
        let mut h = telefono_con_trova("trova-documento");
        scrivi(&mut h, "cane");
        let v_campo = h.state().trova.as_ref().unwrap().versione();
        // Lo stato che il campo riceverebbe con una «x» in più, se arrivasse in ritardo.
        let in_ritardo = ImeSnapshot { text: "canex".into(), sel: (5, 5), comp: (-1, -1), version: v_campo };
        // Un tocco sul documento, sotto le barre: la tastiera ora scrive lì.
        let sotto = h.get_by_role_and_label(Role::Button, schermo_editor::CHIUDI_TROVA).rect().max.y + 200.0;
        tocca_in(&mut h, egui::pos2(300.0, sotto));
        h.run_steps(3);
        assert_eq!(h.state().fuoco, Fuoco::Documento);
        let (_, _, _, v) = h.state().piattaforma.ultimo_stato().unwrap();
        assert!(v > v_campo, "la finestra del documento v{v} deve venire dopo quella del campo v{v_campo}");
        scrivi(&mut h, "X");
        assert_eq!(h.state().trova.as_ref().unwrap().testo(), "cane");
        assert!(testi(h.state()).concat().contains('X'), "{:?}", testi(h.state()));
        // Uno stato in ritardo del campo non entra nel documento.
        let prima = testi(h.state());
        let app = h.state_mut();
        let st = app.editor_mut().unwrap().ricevi(&in_ritardo);
        app.dopo_ponte(st);
        assert_eq!(testi(h.state()), prima);
    }

    #[test]
    fn indietro_chiude_tastiera_poi_trova() {
        let mut h = telefono_con_trova("trova-indietro");
        indietro(&mut h);
        assert!(!h.state().piattaforma.keyboard_visible());
        assert!(h.state().trova.is_some());
        indietro(&mut h);
        assert!(h.state().trova.is_none());
        assert_eq!(h.state().schermata, Schermata::Editor);
    }

    #[test]
    fn sostituisci_dalla_seconda_riga_di_trova() {
        let mut h = telefono_con_trova("trova-sostituisci");
        scrivi(&mut h, "gatto");
        assert!(h.query_by_value(schermo_editor::VUOTO_SOSTITUISCI).is_none(), "la seconda riga parte chiusa");
        tocca(&mut h, schermo_editor::TROVA_E_SOSTITUISCI);
        // Sotto la barra di Trova: il campo, Sostituisci e Tutti, poi Maiuscole e Parola intera.
        let campo = h.get_by_value(schermo_editor::VUOTO_SOSTITUISCI).rect();
        assert!(campo.top() >= tema::BARRA_ALTA - 0.5 && campo.height() == tema::PULSANTE, "{campo:?}");
        let sostituisci = h.get_by_role_and_label(Role::Button, schermo_editor::SOSTITUISCI).rect();
        let tutti = h.get_by_role_and_label(Role::Button, schermo_editor::TUTTI).rect();
        assert!(campo.right() <= sostituisci.left() && sostituisci.right() <= tutti.left() && tutti.right() <= 412.0, "{sostituisci:?} {tutti:?}");
        let maiuscole = h.get_by_role_and_label(Role::Button, schermo_editor::MAIUSCOLE).rect();
        let parola = h.get_by_role_and_label(Role::Button, schermo_editor::PAROLA_INTERA).rect();
        assert!(maiuscole.top() >= campo.bottom() && maiuscole.right() <= parola.left(), "{maiuscole:?} {parola:?}");
        let acceso = h.get_by_role_and_label(Role::Button, schermo_editor::TROVA_E_SOSTITUISCI).accesskit_node().toggled();
        assert_eq!(acceso, Some(egui::accesskit::Toggled::True), "il pulsante della riga è acceso");
        // Il campo nuovo prende la tastiera: le lettere vanno lì e Trova resta com'è.
        assert!(h.state().sostituisci_con.as_ref().unwrap().ha_il_fuoco() && !h.state().trova.as_ref().unwrap().ha_il_fuoco());
        scrivi(&mut h, "micio");
        assert_eq!(h.state().sostituisci_con.as_ref().unwrap().testo(), "micio");
        assert_eq!(h.state().trova.as_ref().unwrap().testo(), "gatto");
        assert_eq!(testi(h.state()), ["gatto e cane", "un gatto nero", "il gatto"]);
        h.get_by_label("1 di 3");
        // Sostituisci: il primo, poi il risultato dopo.
        tocca(&mut h, schermo_editor::SOSTITUISCI);
        assert_eq!(testi(h.state()), ["micio e cane", "un gatto nero", "il gatto"]);
        h.get_by_label("1 di 2");
        // Tutti: gli altri due, e l'avviso.
        tocca(&mut h, schermo_editor::TUTTI);
        assert_eq!(testi(h.state()), ["micio e cane", "un micio nero", "il micio"]);
        h.get_by_label("Sostituiti 2");
        h.get_by_label("Nessun risultato");
        assert!(h.get_by_role_and_label(Role::Button, schermo_editor::TUTTI).accesskit_node().is_disabled());
        assert!(h.get_by_role_and_label(Role::Button, schermo_editor::SOSTITUISCI).accesskit_node().is_disabled());
        let d = diagnosi_della_prova();
        assert!(d.contains("] fuoco: campo Sostituisci con, finestra v"), "{d}");
        assert!(d.contains("] sostituisci: 1 sostituito, ne restano 2\n") && d.contains("] sostituisci tutti: 2 sostituiti\n"), "{d}");
        assert!(d.contains(" al campo di Sostituisci, "), "{d}");
        assert!(!d.contains("micio") && !d.contains("gatto"), "{d}");
    }

    #[test]
    fn maiuscole_e_parola_intera_cambiano_i_risultati() {
        let mut h = telefono_con_trova("trova-opzioni");
        scrivi(&mut h, "Gatt");
        h.get_by_label("1 di 3");
        tocca(&mut h, schermo_editor::TROVA_E_SOSTITUISCI);
        tocca(&mut h, schermo_editor::MAIUSCOLE);
        h.get_by_label("Nessun risultato");
        assert_eq!(h.get_by_role_and_label(Role::Button, schermo_editor::MAIUSCOLE).accesskit_node().toggled(), Some(egui::accesskit::Toggled::True));
        tocca(&mut h, schermo_editor::MAIUSCOLE);
        h.get_by_label("1 di 3");
        tocca(&mut h, schermo_editor::PAROLA_INTERA);
        h.get_by_label("Nessun risultato");
        assert_eq!(editor(h.state()).opzioni_ricerca(), OpzioniRicerca { maiuscole: false, parola_intera: true });
        // Il tocco sul campo di Trova gli riporta la tastiera: la parola intera, «Gatto», c'è.
        let campo = h.get_by_value("Gatt").rect();
        tocca_in(&mut h, campo.center());
        h.run_steps(2);
        assert!(h.state().trova.as_ref().unwrap().ha_il_fuoco() && !h.state().sostituisci_con.as_ref().unwrap().ha_il_fuoco());
        scrivi(&mut h, "o");
        assert_eq!(h.state().trova.as_ref().unwrap().testo(), "Gatto");
        h.get_by_label("1 di 3");
    }

    #[test]
    fn indietro_chiude_tastiera_poi_sostituisci_poi_trova() {
        let mut h = telefono_con_trova("trova-sostituisci-indietro");
        tocca(&mut h, schermo_editor::TROVA_E_SOSTITUISCI);
        indietro(&mut h);
        assert!(!h.state().piattaforma.keyboard_visible());
        assert!(h.state().sostituisci_con.is_some());
        indietro(&mut h);
        assert!(h.state().sostituisci_con.is_none() && h.state().trova.is_some());
        assert!(!h.state().piattaforma.keyboard_visible(), "chiudere la riga non riapre la tastiera");
        indietro(&mut h);
        assert!(h.state().trova.is_none());
        // Il pulsante la apre e la chiude; chiusa, la tastiera torna al campo di Trova.
        tocca(&mut h, schermo_editor::MENU);
        tocca(&mut h, "Trova");
        tocca(&mut h, schermo_editor::TROVA_E_SOSTITUISCI);
        tocca(&mut h, schermo_editor::TROVA_E_SOSTITUISCI);
        assert!(h.state().sostituisci_con.is_none());
        assert!(h.state().trova.as_ref().unwrap().ha_il_fuoco() && h.state().piattaforma.keyboard_visible());
    }

    #[test]
    fn indice_dal_menu_con_tre_titoli_e_salto() {
        let mut app = app_di_prova("indice");
        app.cambia_documento(testo_semplice("prova", "Primo\ntesto\nSecondo\naltro\nTerzo"));
        let e = app.editor_mut().unwrap();
        for (par, id) in [(0, "para.heading1"), (2, "para.heading2"), (4, "para.heading1")] {
            e.s.run("caret.set", &serde_json::json!({ "pos": Pos::body(par, 0) })).unwrap();
            e.s.run(id, &serde_json::json!({})).unwrap();
        }
        let mut h = telefono(app);
        tocca(&mut h, schermo_editor::MENU);
        tocca(&mut h, "Indice");
        h.run_steps(15);
        // Righe da 48 dp larghe quanto il foglio; i titoli rientrati per livello, il primo
        // livello in grassetto: «Secondo» più a destra di «Primo».
        let primo = h.get_by_role_and_label(Role::Button, "Primo").rect();
        let secondo = h.get_by_role_and_label(Role::Button, "Secondo").rect();
        h.get_by_role_and_label(Role::Button, "Terzo");
        assert!(primo.height() >= tema::PULSANTE && primo.width() > 300.0, "{primo:?}");
        let (testo_primo, testo_secondo) = (rettangolo_del_testo(&h, "Primo").unwrap(), rettangolo_del_testo(&h, "Secondo").unwrap());
        assert!(testo_secondo.min.x > testo_primo.min.x + 16.0, "{testo_primo:?} {testo_secondo:?}");
        assert_eq!(famiglia_del_testo(&h, "Primo"), Some(Carattere::Grassetto.famiglia()));
        assert_eq!(famiglia_del_testo(&h, "Secondo"), Some(egui::FontFamily::Proportional));
        assert!(secondo.min.y >= primo.max.y);
        tocca(&mut h, "Secondo");
        assert_eq!(cursore(h.state()), (vec![2], 0));
        assert!(!h.state().foglio_indice.aperto());
    }

    #[test]
    fn l_indice_senza_titoli_lo_dice() {
        let mut app = app_di_prova("indice-vuoto");
        app.cambia_documento(testo_semplice("prova", "Solo testo"));
        let mut h = telefono(app);
        tocca(&mut h, schermo_editor::MENU);
        tocca(&mut h, "Indice");
        h.run_steps(15);
        h.get_by_label("Nessun titolo");
        assert_eq!(colore_del_testo(&h, icone::TOC), Some(tema::MATITA));
    }

    #[test]
    fn pagina_dal_menu_e_un_tocco_torna_in_scrittura() {
        let mut app = app_di_prova("pagina");
        app.cambia_documento(Editor::new(esempi::lungo(60), "lungo"));
        let mut h = telefono(app);
        tocca(&mut h, schermo_editor::MENU);
        tocca(&mut h, "Vista pagina");
        h.run_steps(4);
        assert_eq!(editor(h.state()).vista.modo, Modo::Pagina);
        assert!(h.query_by_role_and_label(Role::Button, schermo_editor::FORMATO).is_none(), "in Pagina niente barra del formato");
        // In Pagina Annulla e Ripeti sono spenti.
        assert!(h.get_by_label(schermo_editor::ANNULLA).accesskit_node().is_disabled());
        // Dal menu, «Vista scrittura» torna indietro; poi di nuovo in Pagina.
        tocca(&mut h, schermo_editor::MENU);
        tocca(&mut h, "Vista scrittura");
        assert_eq!(editor(h.state()).vista.modo, Modo::Scrittura);
        tocca(&mut h, schermo_editor::MENU);
        tocca(&mut h, "Vista pagina");
        h.run_steps(4);
        // Un tocco sulla pagina: si torna in Scrittura, col cursore lì vicino e la tastiera chiusa.
        tocca_in(&mut h, egui::pos2(150.0, 600.0));
        h.run_steps(3);
        assert_eq!(editor(h.state()).vista.modo, Modo::Scrittura);
        assert!(!h.state().piattaforma.keyboard_visible());
        let (par, _) = cursore(h.state());
        assert!(par[0] > 0, "il cursore è rimasto in cima: {par:?}");
    }

    #[test]
    fn testata_con_annulla_ripeti_e_il_menu_intero() {
        let mut h = telefono_con_ciao_selezionato("testata-intera");
        let schermo = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(412.0, 915.0));
        let indietro = h.get_by_label(schermo_editor::INDIETRO).rect();
        for etichetta in [schermo_editor::ANNULLA, schermo_editor::RIPETI, schermo_editor::MENU] {
            let r = h.get_by_role_and_label(Role::Button, etichetta).rect();
            assert!(schermo.contains_rect(r) && r.center().y < indietro.max.y + 1.0, "«{etichetta}» non è nella testata: {r:?}");
            assert!(r.width() >= 48.0 && r.height() >= 48.0, "«{etichetta}» piccolo: {r:?}");
        }
        // Appena aperto non c'è niente da annullare né da ripetere.
        for etichetta in [schermo_editor::ANNULLA, schermo_editor::RIPETI] {
            assert!(h.get_by_label(etichetta).accesskit_node().is_disabled(), "«{etichetta}» acceso");
        }
        tocca(&mut h, schermo_editor::MENU);
        let voci = ["Salva", "Salva una copia", "Esporta in PDF", "Condividi", "Trova", "Indice", "Vista pagina", "Informazioni sul documento"];
        let mut sopra = 0.0;
        for voce in voci {
            let r = h.get_by_role_and_label(Role::Button, voce).rect();
            assert!(schermo.contains_rect(r), "«{voce}» fuori dallo schermo: {r:?}");
            assert!(r.min.y >= sopra, "«{voce}» fuori ordine");
            sopra = r.max.y;
        }
        // La Diagnosi è nelle Impostazioni.
        assert!(h.query_by_label("Diagnosi").is_none());
    }

    #[test]
    fn annulla_e_ripeti_dalla_testata() {
        let mut h = telefono_con_ciao_selezionato("annulla-testata");
        tocca(&mut h, "Grassetto");
        assert!(primo(h.state()).runs.iter().any(|r| r.props.bold == Some(true)));
        assert!(!h.get_by_label(schermo_editor::ANNULLA).accesskit_node().is_disabled(), "Annulla spento dopo una modifica");
        assert!(h.get_by_label(schermo_editor::RIPETI).accesskit_node().is_disabled());
        tocca(&mut h, schermo_editor::ANNULLA);
        assert!(!primo(h.state()).runs.iter().any(|r| r.props.bold == Some(true)), "Annulla non ha tolto il grassetto");
        assert!(!h.get_by_label(schermo_editor::RIPETI).accesskit_node().is_disabled(), "Ripeti spento dopo Annulla");
        tocca(&mut h, schermo_editor::RIPETI);
        assert!(primo(h.state()).runs.iter().any(|r| r.props.bold == Some(true)), "Ripeti non ha rimesso il grassetto");
    }

    #[test]
    fn barra_di_formato_e_bottoni_accesi() {
        let mut h = telefono_con_ciao_selezionato("barra-accesa");
        for (_, _, etichetta) in schermo_editor::BARRA {
            assert!(!acceso(&h, etichetta), "«{etichetta}» acceso prima di tutto");
        }
        tocca(&mut h, "Grassetto");
        assert!(acceso(&h, "Grassetto"), "dopo Grassetto il bottone è spento");
        assert!(!acceso(&h, "Corsivo") && !acceso(&h, "Sottolineato"));
        tocca(&mut h, "Sottolineato");
        assert!(acceso(&h, "Grassetto") && acceso(&h, "Sottolineato"));
        // Gli elenchi, letti dal paragrafo.
        tocca(&mut h, "Elenco numerato");
        assert!(acceso(&h, "Elenco numerato") && !acceso(&h, "Elenco puntato"));
        tocca(&mut h, "Elenco puntato");
        assert!(acceso(&h, "Elenco puntato") && !acceso(&h, "Elenco numerato"));
        // Il cursore nella seconda riga, che non ha formato: si spegne tutto.
        let e = h.state_mut().editor_mut().unwrap();
        let st = e.tocca(tascabile_word::vista::Tocco { x_pt: 5.0, y_pt: 200.0 });
        h.state().dopo_ponte(st);
        h.run_steps(2);
        for (_, _, etichetta) in schermo_editor::BARRA {
            assert!(!acceso(&h, etichetta), "«{etichetta}» acceso: {:?}", editor(h.state()).formato);
        }
    }

    /// L'app con un .docx in Download aperto dalla cartella, senza modifiche.
    fn telefono_con_un_docx(nome: &str, file: &str) -> (Telefono, PathBuf) {
        let (piattaforma, d) = piattaforma_di_prova(nome);
        let cartella = d.join("radice/Download");
        std::fs::create_dir_all(&cartella).unwrap();
        docx(&cartella.join(file), &["Verbale", "Kilimangiaro"]);
        let mut h = telefono(TascabileApp::new(piattaforma, Instant::now()));
        tocca(&mut h, "Download");
        tocca(&mut h, file);
        (h, d)
    }

    #[test]
    fn la_barra_alta_dice_se_il_documento_e_salvato_e_dove() {
        let (mut h, _) = telefono_con_un_docx("stato-salvato", "Verbale di ottobre.docx");
        h.get_by_label("Verbale di ottobre");
        h.get_by_label("Salvato · Download");
        assert_eq!(famiglia_del_testo(&h, icone::CHECK_CIRCLE), Some(Carattere::IconaPiena.famiglia()), "manca la spunta verde");
        apri_tastiera(h.state_mut());
        scrivi(&mut h, "x");
        h.get_by_label("Modifiche non salvate");
        assert!(famiglia_del_testo(&h, icone::CHECK_CIRCLE).is_none(), "spunta con le modifiche");
        tocca(&mut h, schermo_editor::MENU);
        tocca(&mut h, "Salva");
        h.get_by_label("Salvato · Download");
        // Un documento nuovo non è ancora da nessuna parte.
        h.state_mut().chiudi_poi(Dopo::Home);
        h.state_mut().nuovo_documento();
        h.run_steps(2);
        h.get_by_label("Non ancora salvato");
    }

    #[test]
    fn la_riga_delle_misure_non_c_e_piu() {
        let h = telefono_con_la_checklist("senza-misure");
        for via in ["apertura", "layout", "fascia", VERSIONE] {
            assert!(h.query_by_label_contains(via).is_none(), "«{via}» nell'Editor");
        }
        let d = diagnosi_della_prova();
        assert!(d.contains("] pagina: prima fascia dopo "), "{d}");
    }

    #[test]
    fn il_menu_si_chiude_con_indietro_e_toccando_fuori() {
        let mut h = telefono_con_ciao_selezionato("menu-chiuso");
        tocca(&mut h, schermo_editor::MENU);
        h.get_by_label("Informazioni sul documento");
        indietro(&mut h);
        assert!(h.query_by_label("Informazioni sul documento").is_none());
        assert_eq!(h.state().schermata, Schermata::Editor);
        assert!(!editor(h.state()).s.sel.is_collapsed(), "Indietro ha chiuso anche la selezione");
        tocca(&mut h, schermo_editor::MENU);
        tocca_in(&mut h, egui::pos2(60.0, 800.0));
        h.run_steps(3);
        assert!(h.query_by_label("Informazioni sul documento").is_none());
        assert!(!editor(h.state()).s.sel.is_collapsed(), "il tocco fuori dal menu è arrivato al documento");
    }

    /// La prima tabella del documento nell'Editor.
    fn tabella(app: &TascabileApp) -> wordcraft_engine::doc::Table {
        editor(app).s.doc.body.iter().find_map(|b| b.as_table()).expect("nessuna tabella").clone()
    }

    /// Il testo della cella (`riga`, `cella`) della prima tabella.
    fn testo_cella(app: &TascabileApp, riga: usize, cella: usize) -> String {
        tabella(app).rows[riga].cells[cella].blocks.iter().filter_map(|b| b.as_para()).map(|p| p.text.clone()).collect::<Vec<_>>().join("|")
    }

    /// Il dito che scorre la barra della tabella verso sinistra fino in fondo, per le ultime voci.
    fn scorri_la_barra_della_tabella(h: &mut Telefono) {
        for _ in 0..3 {
            if h.get_by_label(foglio_tabella::INTESTAZIONE).rect().right() < 400.0 {
                return;
            }
            scorri_la_barra_una_volta(h);
        }
    }

    fn scorri_la_barra_una_volta(h: &mut Telefono) {
        let y = h.get_by_label(foglio_tabella::RIGA_SOPRA).rect().center().y;
        let da = egui::pos2(360.0, y);
        h.event(egui::Event::PointerMoved(da));
        h.event(egui::Event::PointerButton { pos: da, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
        h.step();
        for k in 1..=10 {
            h.event(egui::Event::PointerMoved(da - egui::vec2(32.0 * k as f32, 0.0)));
            h.step();
        }
        let a = da - egui::vec2(320.0, 0.0);
        h.event(egui::Event::PointerButton { pos: a, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
        h.event(egui::Event::PointerGone);
        h.run_steps(40);
    }

    #[test]
    fn tabella_dal_foglio_inserisci_scritta_e_cambiata_dalla_sua_barra() {
        let mut app = app_di_prova("tabella");
        app.cambia_documento(testo_semplice("prova", "Prima\nDopo"));
        app.editor_mut().unwrap().s.run("caret.set", &serde_json::json!({ "pos": Pos::body(0, 5) })).unwrap();
        let mut h = telefono(app);
        assert!(h.query_by_label(foglio_tabella::RIGA_SOPRA).is_none(), "la barra della tabella senza tabella");
        // Inserisci, Tabella: la griglia parte da 3 per 3; si sceglie 2 righe e 4 colonne.
        tocca(&mut h, schermo_editor::INSERISCI);
        h.run_steps(10);
        tocca(&mut h, foglio_tabella::TABELLA);
        h.run_steps(4);
        h.get_by_role_and_label(Role::Label, "3 righe e 3 colonne");
        let quadretto = h.get_by_role_and_label(Role::Button, "3 righe e 3 colonne");
        assert_eq!(quadretto.accesskit_node().toggled(), Some(egui::accesskit::Toggled::True));
        tocca(&mut h, "2 righe e 4 colonne");
        h.get_by_role_and_label(Role::Label, "2 righe e 4 colonne");
        tocca(&mut h, foglio_tabella::INSERISCI_TABELLA);
        h.run_steps(4);
        assert!(!h.state().foglio_inserisci.aperto());
        let t = tabella(h.state());
        assert_eq!((t.rows.len(), t.cols()), (2, 4));
        // Il cursore nella prima cella, la tastiera aperta: si scrive lì.
        assert!(h.state().piattaforma.keyboard_visible(), "la tastiera non si è aperta");
        scrivi(&mut h, "Ora");
        assert_eq!(testo_cella(h.state(), 0, 0), "Ora");
        // La barra della tabella, sopra quella di formato.
        let barra = h.get_by_label(schermo_editor::FORMATO).rect();
        let riga = h.get_by_label(foglio_tabella::RIGA_SOPRA).rect();
        assert!(riga.bottom() < barra.top(), "{riga:?} sopra {barra:?}");
        tocca(&mut h, foglio_tabella::RIGA_SOTTO);
        assert_eq!(tabella(h.state()).rows.len(), 3);
        tocca(&mut h, foglio_tabella::COLONNA_DESTRA);
        assert_eq!(tabella(h.state()).cols(), 5);
        // Unisci è spento con una cella sola; le voci in fondo si raggiungono scorrendo la barra.
        scorri_la_barra_della_tabella(&mut h);
        assert!(h.get_by_label(foglio_tabella::UNISCI).accesskit_node().is_disabled());
        tocca(&mut h, foglio_tabella::INTESTAZIONE);
        assert!(tabella(h.state()).rows[0].props.header);
        assert_eq!(h.get_by_label(foglio_tabella::INTESTAZIONE).accesskit_node().toggled(), Some(egui::accesskit::Toggled::True));
        // Elimina: il menu sale dalla barra; Indietro lo chiude, poi la riga.
        tocca(&mut h, foglio_tabella::ELIMINA);
        h.get_by_label(foglio_tabella::ELIMINA_TABELLA);
        indietro(&mut h);
        assert!(h.query_by_label(foglio_tabella::ELIMINA_TABELLA).is_none());
        assert_eq!(h.state().schermata, Schermata::Editor);
        tocca(&mut h, foglio_tabella::ELIMINA);
        tocca(&mut h, foglio_tabella::ELIMINA_COLONNA);
        assert_eq!(tabella(h.state()).cols(), 4);
        // Bordi e sfondo: il foglio resta aperto per il comando dopo.
        tocca(&mut h, foglio_tabella::BORDI_E_SFONDO);
        h.run_steps(10);
        tocca(&mut h, "Contorno");
        tocca(&mut h, "Giallo chiaro");
        assert!(h.state().foglio_bordi.aperto());
        let t = tabella(h.state());
        assert!(t.props.borders.is_some_and(|b| b.between.is_some_and(|x| !x.is_visible())), "{:?}", t.props.borders);
        let (_, r, c) = editor(h.state()).s.sel.focus.path.cell().unwrap();
        assert_eq!(t.rows[r].cells[c].props.shading.map(|x| (x.0, x.1, x.2)), Some((0xFF, 0xF2, 0xCC)));
        assert_eq!(h.get_by_label("Giallo chiaro").accesskit_node().toggled(), Some(egui::accesskit::Toggled::True));
        indietro(&mut h);
        assert!(!h.state().foglio_bordi.aperto());
        // Tutto si annulla dalla barra alta, un passo alla volta.
        tocca(&mut h, schermo_editor::ANNULLA);
        assert_eq!(tabella(h.state()).rows[r].cells[c].props.shading, None);
        // Elimina la tabella: la barra se ne va.
        tocca(&mut h, foglio_tabella::ELIMINA);
        tocca(&mut h, foglio_tabella::ELIMINA_TABELLA);
        h.run_steps(2);
        assert!(editor(h.state()).s.doc.body.iter().all(|b| b.as_table().is_none()));
        assert!(h.query_by_label(foglio_tabella::RIGA_SOPRA).is_none(), "la barra è rimasta");
        let d = diagnosi_della_prova();
        for riga in [
            "] inserisci: tabella di 2 righe e 4 colonne\n",
            "] comando: table.insertRowBelow\n",
            "] comando: table.deleteColumn\n",
            "] tabella: intestazione ripetuta accesa\n",
            "] comando: table.deleteTable\n",
        ] {
            assert!(d.contains(riga), "manca «{riga}»:\n{d}");
        }
        assert!(!d.contains("Ora"), "{d}");
    }

    /// Una foto di prova: `w` per `h` pixel a righe colorate, in PNG.
    fn foto_png(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbImage::from_fn(w, h, |x, y| image::Rgb([(x * 255 / w) as u8, (y * 255 / h) as u8, 0x80]));
        let mut out = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
        out
    }

    /// Qualche frame finché la foto per il documento non è pronta (il suo thread), al massimo 10 s.
    fn aspetta_l_immagine(h: &mut Telefono) {
        let inizio = Instant::now();
        while h.state().immagine_in_arrivo() && inizio.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(10));
            h.run_steps(1);
        }
        h.run_steps(4);
    }

    fn immagine(app: &TascabileApp) -> Option<tascabile_word::immagine::InfoImmagine> {
        tascabile_word::immagine::immagine_scelta(&editor(app).s).map(|(_, i)| i)
    }

    #[test]
    fn immagine_dalla_galleria_scelta_e_cambiata_dalla_sua_barra() {
        let mut app = app_di_prova("immagine");
        app.cambia_documento(testo_semplice("prova", "Prima\nDopo"));
        app.editor_mut().unwrap().s.run("caret.set", &serde_json::json!({ "pos": Pos::body(0, 5) })).unwrap();
        apri_tastiera(&mut app);
        let mut h = telefono(app);
        assert!(h.query_by_label(foglio_immagine::ALLINEA).is_none(), "la barra dell'immagine senza immagine");
        // Inserisci, Immagine: da dove; la galleria chiede la foto ad Android.
        tocca(&mut h, schermo_editor::INSERISCI);
        h.run_steps(10);
        tocca(&mut h, foglio_immagine::IMMAGINE);
        h.run_steps(4);
        h.get_by_role_and_label(Role::Button, foglio_immagine::DALLA_FOTOCAMERA);
        tocca(&mut h, foglio_immagine::DALLA_GALLERIA);
        assert!(!h.state().foglio_inserisci.aperto());
        assert_eq!(h.state().piattaforma.foto_chieste(), [tascabile_platform::FonteFoto::Galleria]);
        assert!(!h.state().piattaforma.keyboard_visible(), "per la foto la tastiera si chiude");
        // Arriva una foto di 800 per 400 pixel: entra al cursore, larga al massimo come il testo
        // dello schermo, e resta scelta, con la sua barra; la foto non resta nella cache.
        h.state().simula_foto_di_prova(&foto_png(800, 400));
        aspetta_l_immagine(&mut h);
        let i = immagine(h.state()).expect("l'immagine nuova è scelta");
        let testo = editor(h.state()).vista.larghezza_del_testo();
        assert!((i.larghezza - testo).abs() < 0.5 && (i.altezza * 2.0 - i.larghezza).abs() < 0.5, "{i:?}, testo di {testo} pt");
        assert_eq!(rimasto_fra_le_foto(&h.state().piattaforma.cartella_cache()), Vec::<String>::new(), "la foto è rimasta nella cache");
        assert!(!h.state().piattaforma.keyboard_visible());
        let barra = h.get_by_label(schermo_editor::FORMATO).rect();
        let voce = h.get_by_label(foglio_immagine::ALLINEA).rect();
        assert!(voce.bottom() < barra.top(), "{voce:?} sopra {barra:?}");
        // Allinea apre il suo menu sopra la voce, col lato di adesso in grassetto; Al centro, poi
        // il testo intorno: resta al centro.
        tocca(&mut h, foglio_immagine::ALLINEA);
        let centro = h.get_by_role_and_label(Role::Button, foglio_immagine::AL_CENTRO).rect();
        assert!(centro.bottom() <= voce.top(), "il menu non sta sopra Allinea: {centro:?}");
        assert_eq!(famiglia_del_testo(&h, foglio_immagine::A_SINISTRA), Some(Carattere::Grassetto.famiglia()));
        assert_eq!(famiglia_del_testo(&h, foglio_immagine::AL_CENTRO), Some(Carattere::Testo.famiglia()));
        tocca(&mut h, foglio_immagine::AL_CENTRO);
        assert_eq!(immagine(h.state()).map(|i| i.lato), Some(tascabile_word::immagine::Lato::Centro));
        assert!(h.state().menu_allinea.is_none() && h.query_by_label(foglio_immagine::A_DESTRA).is_none(), "il menu è rimasto aperto");
        // Indietro chiude il menu e basta.
        tocca(&mut h, foglio_immagine::ALLINEA);
        assert_eq!(famiglia_del_testo(&h, foglio_immagine::AL_CENTRO), Some(Carattere::Grassetto.famiglia()));
        indietro(&mut h);
        assert!(h.state().menu_allinea.is_none() && immagine(h.state()).is_some());
        tocca(&mut h, foglio_immagine::TESTO_INTORNO);
        let i = immagine(h.state()).unwrap();
        assert_eq!((i.intorno, i.lato), (tascabile_word::immagine::Intorno::Quadrato, tascabile_word::immagine::Lato::Centro));
        assert_eq!(h.get_by_label(foglio_immagine::TESTO_INTORNO).accesskit_node().toggled(), Some(egui::accesskit::Toggled::True));
        tocca(&mut h, foglio_immagine::TESTO_INTORNO);
        assert_eq!(immagine(h.state()).map(|i| i.intorno), Some(tascabile_word::immagine::Intorno::InLinea));
        // La descrizione: il foglio col campo che prende la tastiera; Indietro chiude prima la
        // tastiera, poi il foglio, senza cambiare niente.
        tocca(&mut h, foglio_immagine::DESCRIZIONE);
        h.run_steps(10);
        assert!(h.state().foglio_descrizione.aperto() && h.state().piattaforma.keyboard_visible());
        scrivi(&mut h, "Bozza");
        indietro(&mut h);
        assert!(!h.state().piattaforma.keyboard_visible() && h.state().foglio_descrizione.aperto());
        indietro(&mut h);
        assert!(!h.state().foglio_descrizione.aperto() && h.state().descrizione.is_none());
        assert_eq!(immagine(h.state()).map(|i| i.descrizione), Some(String::new()));
        // Di nuovo, con Fatto.
        tocca(&mut h, foglio_immagine::DESCRIZIONE);
        h.run_steps(10);
        scrivi(&mut h, "Medicazione del tallone");
        tocca(&mut h, foglio_immagine::FATTO);
        h.run_steps(4);
        assert!(!h.state().foglio_descrizione.aperto() && !h.state().piattaforma.keyboard_visible());
        assert_eq!(immagine(h.state()).map(|i| i.descrizione), Some("Medicazione del tallone".to_string()));
        assert_eq!(h.state().fuoco, Fuoco::Documento);
        // Annulla dalla barra alta: la descrizione se ne va, l'immagine resta scelta.
        tocca(&mut h, schermo_editor::ANNULLA);
        assert_eq!(immagine(h.state()).map(|i| i.descrizione), Some(String::new()));
        // Elimina: l'immagine e la sua barra se ne vanno.
        tocca(&mut h, foglio_tabella::ELIMINA);
        h.run_steps(2);
        assert_eq!(testi(h.state()), ["Prima", "", "Dopo"]);
        assert!(h.query_by_label(foglio_immagine::ALLINEA).is_none(), "la barra è rimasta");
        let d = diagnosi_della_prova();
        for riga in [
            "] word: immagine dalla galleria\n",
            "] word: immagine arrivata\n",
            "] immagine: menu dell'allineamento\n",
            "] comando: para.alignCenter\n",
            "] indietro: menu dell'allineamento chiuso\n",
            "] immagine: testo intorno, quadrato\n",
            "] immagine: descrizione di 23 caratteri\n",
            "] immagine: eliminata\n",
        ] {
            assert!(d.contains(riga), "manca «{riga}»:\n{d}");
        }
        assert!(!d.contains("Medicazione") && !d.contains("Bozza"), "{d}");
    }

    /// La barra dell'immagine sul telefono dell'autore e su uno stretto (0.8.3: a 372 dp Elimina
    /// restava fuori, da scorrere): le quattro voci si vedono intere, dentro la barra.
    #[test]
    fn la_barra_dell_immagine_si_vede_tutta_su_ogni_telefono() {
        for (nome, misura) in [("davide", egui::vec2(372.0, 827.0)), ("stretto", egui::vec2(360.0, 760.0))] {
            let mut app = app_di_prova(&format!("immagine-{nome}"));
            app.cambia_documento(testo_semplice("prova", "Prima\nDopo"));
            app.editor_mut().unwrap().s.run("caret.set", &serde_json::json!({ "pos": Pos::body(0, 5) })).unwrap();
            let mut h = telefono_di(app, misura, 3.0);
            tocca(&mut h, schermo_editor::INSERISCI);
            h.run_steps(10);
            tocca(&mut h, foglio_immagine::IMMAGINE);
            h.run_steps(4);
            tocca(&mut h, foglio_immagine::DALLA_GALLERIA);
            h.state().simula_foto_di_prova(&foto_png(800, 400));
            aspetta_l_immagine(&mut h);
            assert!(immagine(h.state()).is_some(), "{nome}");
            let schermo = egui::Rect::from_min_size(egui::Pos2::ZERO, misura);
            let voci = [foglio_immagine::ALLINEA, foglio_immagine::TESTO_INTORNO, foglio_immagine::DESCRIZIONE, foglio_tabella::ELIMINA];
            tutti_visibili(&h, &voci, schermo);
            // Dentro la pillola, larga quanto la barra di formato, coi suoi 10 dp di margine.
            let pillola = tascabile_touch::BarraFlottante::posto(schermo);
            for v in voci {
                let r = h.get_by_role_and_label(Role::Button, v).rect();
                assert!(
                    r.left() >= pillola.left() + 9.5 && r.right() <= pillola.right() - 9.5,
                    "{nome}: «{v}» esce dalla barra: {r:?} in {pillola:?}"
                );
            }
        }
    }

    #[test]
    fn il_tocco_sull_immagine_la_sceglie_e_la_maniglia_ne_cambia_la_misura() {
        let mut app = app_di_prova("immagine-tocco");
        app.cambia_documento(testo_semplice("prova", "Prima\nDopo"));
        let e = app.editor_mut().unwrap();
        e.s.run("caret.set", &serde_json::json!({ "pos": Pos::body(0, 5) })).unwrap();
        let foto = tascabile_word::immagine::prepara(&foto_png(400, 200)).unwrap();
        e.inserisci_immagine(&foto, 200.0);
        e.s.run("caret.set", &serde_json::json!({ "pos": Pos::body(2, 2) })).unwrap();
        let mut h = telefono(app);
        assert!(immagine(h.state()).is_none());
        let r = h.state_mut().prima_immagine_di_prova().expect("l'immagine sullo schermo");
        tocca_in(&mut h, r.center());
        h.run_steps(4);
        let r = h.state_mut().immagine_sullo_schermo_di_prova().expect("il tocco la sceglie");
        // La maniglia nell'angolo tirata di 50 dp in diagonale verso l'interno: più piccola.
        let da = r.max;
        h.event(egui::Event::PointerMoved(da));
        h.event(egui::Event::PointerButton { pos: da, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
        h.step();
        let verso = egui::vec2(-40.0, -20.0);
        for k in 1..=10 {
            h.event(egui::Event::PointerMoved(da + verso * (k as f32 / 10.0)));
            h.step();
        }
        h.event(egui::Event::PointerButton {
            pos: da + verso,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        h.event(egui::Event::PointerGone);
        h.run_steps(4);
        let i = immagine(h.state()).expect("resta scelta");
        assert!((i.larghezza - (200.0 - 40.0 / tascabile_word::documento::DP_PER_PT)).abs() < 1.0, "{i:?}");
        // Un Annulla: torna larga 200 punti.
        tocca(&mut h, schermo_editor::ANNULLA);
        assert_eq!(immagine(h.state()).map(|i| i.larghezza), Some(200.0));
    }

    #[test]
    fn il_foglio_pagina_dal_menu_con_intestazione_e_numeri() {
        let mut app = app_di_prova("pagina");
        app.cambia_documento(testo_semplice("prova", "Uno\nDue"));
        apri_tastiera(&mut app);
        let mut h = telefono(app);
        tocca(&mut h, schermo_editor::MENU);
        tocca(&mut h, foglio_pagina::PAGINA);
        h.run_steps(10);
        assert!(h.state().foglio_pagina.aperto() && !h.state().piattaforma.keyboard_visible());
        assert_eq!(editor(h.state()).vista.modo, Modo::Pagina, "dietro il foglio la vista Pagina");
        let info = |h: &Telefono| tascabile_word::pagina::info_pagina(&editor(h.state()).s);
        assert_eq!(h.get_by_role_and_label(Role::Button, "A4").accesskit_node().toggled(), Some(egui::accesskit::Toggled::True));
        tocca(&mut h, "A5");
        assert_eq!(info(&h).formato, Some(tascabile_word::pagina::Formato::A5));
        tocca(&mut h, "A4");
        tocca(&mut h, foglio_pagina::ORIZZONTALE);
        assert!(info(&h).orizzontale);
        h.get_by_label("29,7 × 21 cm");
        tocca(&mut h, "Stretti");
        assert_eq!(info(&h).margini, Some(tascabile_word::pagina::Margini::Stretti));
        // Intestazione e piè di pagina: i campi, i numeri, il totale.
        tocca(&mut h, foglio_pagina::INTESTAZIONE_E_PIE);
        h.run_steps(10);
        h.get_by_value(foglio_pagina::VUOTO_INTESTAZIONE).click();
        h.run_steps(4);
        assert!(h.state().piattaforma.keyboard_visible());
        scrivi(&mut h, "Reparto");
        // Invio: al piè di pagina.
        h.event(egui::Event::Text("\n".into()));
        h.run_steps(2);
        assert!(h.state().campi_pagina.as_ref().is_some_and(|c| c.pie.ha_il_fuoco()), "Invio non porta al piè di pagina");
        scrivi(&mut h, "Riservato");
        assert!(h.query_by_label(foglio_pagina::DI_TOTALE).is_none(), "il totale senza i numeri");
        tocca(&mut h, "A destra");
        // Tutto il foglio sta sullo schermo, Applica compreso.
        h.run_steps(10);
        assert!(h.get_by_label(foglio_pagina::APPLICA).rect().bottom() <= 915.0, "{:?}", h.get_by_label(foglio_pagina::APPLICA).rect());
        tocca(&mut h, foglio_pagina::DI_TOTALE);
        assert_eq!(h.state().campi_pagina.as_ref().unwrap().intestazione.testo(), "Reparto");
        tocca(&mut h, foglio_pagina::APPLICA);
        h.run_steps(4);
        assert!(!h.state().foglio_pagina.aperto() && !h.state().piattaforma.keyboard_visible());
        assert_eq!(h.state().fuoco, Fuoco::Documento);
        let ip = tascabile_word::pagina::intestazione_e_pie(&editor(h.state()).s);
        let attesa = tascabile_word::pagina::IntestazioneEPie {
            intestazione: "Reparto".into(),
            pie: "Riservato".into(),
            numeri: tascabile_word::pagina::NumeriPagina::ADestra,
            di_totale: true,
            ..Default::default()
        };
        assert_eq!(ip, attesa);
        assert_eq!(editor(h.state()).vista.modo, Modo::Pagina);
        // Riaperta: la voce dice che c'è; Indietro dalla seconda pagina torna alla prima.
        tocca(&mut h, schermo_editor::MENU);
        tocca(&mut h, foglio_pagina::PAGINA);
        h.run_steps(10);
        assert!(rettangolo_del_testo(&h, "Intestazione, piè di pagina, N di M a destra").is_some(), "manca il riassunto sotto la voce");
        tocca(&mut h, foglio_pagina::INTESTAZIONE_E_PIE);
        h.run_steps(4);
        h.get_by_value("Reparto");
        indietro(&mut h);
        assert!(h.state().foglio_pagina.aperto() && h.state().campi_pagina.is_none());
        indietro(&mut h);
        assert!(!h.state().foglio_pagina.aperto());
        // Un Annulla toglie intestazione, piè di pagina e numeri insieme (in Pagina è spento:
        // si torna in Scrittura).
        indietro(&mut h);
        assert_eq!(editor(h.state()).vista.modo, Modo::Scrittura);
        tocca(&mut h, schermo_editor::ANNULLA);
        assert_eq!(tascabile_word::pagina::intestazione_e_pie(&editor(h.state()).s), tascabile_word::pagina::IntestazioneEPie::default());
        let d = diagnosi_della_prova();
        for riga in [
            "] pagina: formato A5\n",
            "] pagina: orizzontale\n",
            "] pagina: margini stretti\n",
            "] pagina: intestazione di 7 caratteri, piè di pagina di 9 caratteri, numeri in basso a destra, con il totale\n",
        ] {
            assert!(d.contains(riga), "manca «{riga}»:\n{d}");
        }
        assert!(!d.contains("Reparto") && !d.contains("Riservato"), "{d}");
    }

    #[test]
    fn inserisci_dal_foglio() {
        let mut h = telefono_con_ciao_selezionato("inserisci");
        tocca(&mut h, schermo_editor::INSERISCI);
        h.run_steps(10);
        for voce in ["Data e ora", "Interruzione di pagina", "Linea orizzontale"] {
            h.get_by_role_and_label(Role::Button, voce);
        }
        // Indietro chiude il foglio e basta.
        indietro(&mut h);
        assert!(!h.state().foglio_inserisci.aperto());
        assert_eq!(h.state().schermata, Schermata::Editor);
        tocca(&mut h, schermo_editor::INSERISCI);
        h.run_steps(10);
        tocca(&mut h, "Linea orizzontale");
        assert!(!h.state().foglio_inserisci.aperto(), "il foglio resta aperto");
        assert!(primo(h.state()).props.borders.as_ref().is_some_and(|b| b.bottom.is_some()), "niente linea");
        tocca(&mut h, schermo_editor::INSERISCI);
        h.run_steps(10);
        tocca(&mut h, "Data e ora");
        let testo = primo(h.state()).text;
        assert!(testo.contains('/') && testo.contains(':') && testo.ends_with(" mondo"), "{testo:?}");
        tocca(&mut h, schermo_editor::INSERISCI);
        h.run_steps(10);
        tocca(&mut h, "Interruzione di pagina");
        assert!(testi(h.state()).concat().contains('\u{000C}'));
        let d = diagnosi_della_prova();
        assert!(d.contains("] inserisci: data e ora\n") && d.contains("] comando: insert.pageBreak\n"), "{d}");
    }

    #[test]
    fn informazioni_sul_documento() {
        let (mut h, _) = telefono_con_un_docx("informazioni", "Verbale.docx");
        tocca(&mut h, schermo_editor::MENU);
        tocca(&mut h, "Informazioni sul documento");
        h.run_steps(10);
        for riga in ["Parole", "Caratteri", "Paragrafi", "Pagine", "Cartella"] {
            h.get_by_label(riga);
        }
        // Due parole in due paragrafi, una pagina, il file in Download.
        assert_eq!(h.get_all_by_label("2").count(), 2);
        for valore in ["19, 19 senza spazi", "1", "Download"] {
            h.get_by_label(valore);
        }
        indietro(&mut h);
        assert!(!h.state().foglio_info.aperto());
        assert_eq!(h.state().schermata, Schermata::Editor);
    }

    #[test]
    fn i_numeri_all_italiana() {
        assert_eq!(schermo_editor::numero(7), "7");
        assert_eq!(schermo_editor::numero(1234), "1.234");
        assert_eq!(schermo_editor::numero(12_345_678), "12.345.678");
    }

    /// La zona libera del telefono con la tastiera aperta (alta come quella finta del PC).
    fn con_la_tastiera(h: &mut Telefono) {
        let alta = h.state().piattaforma.keyboard_height();
        h.input_mut().safe_area_insets = Some(egui::SafeAreaInsets(egui::epaint::MarginF32 { left: 0.0, right: 0.0, top: 0.0, bottom: alta }));
        h.run_steps(10);
    }

    #[test]
    fn con_la_tastiera_la_barra_sta_sulla_tastiera_e_il_cursore_sopra_la_barra() {
        let mut app = app_di_prova("barra-sulla-tastiera");
        app.cambia_documento(Editor::new(esempi::lungo(40), "lungo"));
        let mut h = telefono(app);
        // Un tocco in basso, vicino alla barra: il cursore lì e la tastiera aperta.
        tocca_in(&mut h, egui::pos2(200.0, 820.0));
        h.run_steps(4);
        assert!(h.state().piattaforma.keyboard_visible());
        con_la_tastiera(&mut h);
        let fondo = 915.0 - h.state().piattaforma.keyboard_height();
        let grassetto = h.get_by_role_and_label(Role::Button, "Grassetto").rect();
        // La barra appoggiata 8 dp sopra la tastiera: alta 52, i pulsanti da 48 al centro.
        let centro = fondo - tema::SOPRA_LA_TASTIERA - tema::FORMATO_SULLA_TASTIERA / 2.0;
        assert!((grassetto.center().y - centro).abs() < 0.5, "{grassetto:?} col fondo a {fondo}");
        let cima_barra = fondo - tema::SOPRA_LA_TASTIERA - tema::FORMATO_SULLA_TASTIERA;
        let c = cursore_a_schermo(&h).expect("il cursore a schermo");
        assert!(c.max.y <= cima_barra && c.min.y > tema::BARRA_ALTA, "cursore {c:?} non visibile sopra la barra (cima {cima_barra})");
        // Altre righe battute in fondo: il cursore resta sopra la barra.
        scrivi(&mut h, "\n\n\n\n");
        h.run_steps(10);
        let c = cursore_a_schermo(&h).expect("il cursore a schermo");
        assert!(c.max.y <= cima_barra, "cursore {c:?} sotto la barra (cima {cima_barra})");
    }

    #[test]
    fn senza_tastiera_l_ultima_riga_sale_sopra_la_barra() {
        let mut app = app_di_prova("ultima-riga");
        app.cambia_documento(Editor::new(esempi::lungo(40), "lungo"));
        let e = app.editor_mut().unwrap();
        let ultimo = e.s.doc.body.len() - 1;
        e.s.run("caret.set", &serde_json::json!({ "pos": Pos::body(ultimo, 0) })).unwrap();
        e.vista.scorri_al_cursore();
        let mut h = telefono(app);
        h.run_steps(10);
        let barra = h.get_by_role_and_label(Role::Button, "Grassetto").rect();
        let c = cursore_a_schermo(&h).expect("il cursore a schermo");
        assert!(c.max.y <= barra.min.y - 6.0, "cursore {c:?} sotto la barra {barra:?}");
    }

    #[test]
    fn ogni_bottone_del_foglio_fa_il_suo_comando() {
        use wordcraft_engine::doc::Align;
        type Prova = (&'static str, Option<&'static str>, fn(&TascabileApp) -> bool);
        let prove: [Prova; 14] = [
            ("Normale", Some("para.heading1"), |a| primo(a).props.style.as_deref().is_none_or(|s| s == "Normal")),
            ("Titolo 1", None, |a| primo(a).props.style.as_deref() == Some("Heading1")),
            ("Titolo 2", None, |a| primo(a).props.style.as_deref() == Some("Heading2")),
            ("Titolo 3", None, |a| primo(a).props.style.as_deref() == Some("Heading3")),
            ("A sinistra", Some("para.alignCenter"), |a| primo(a).props.align == Some(Align::Left)),
            ("Al centro", None, |a| primo(a).props.align == Some(Align::Center)),
            ("A destra", None, |a| primo(a).props.align == Some(Align::Right)),
            ("Giustificato", None, |a| primo(a).props.align == Some(Align::Justify)),
            ("Più rientro", None, |a| primo(a).props.indent_left.is_some_and(|x| x > 0.0)),
            ("Meno rientro", Some("para.indent"), |a| primo(a).props.indent_left.is_none_or(|x| x == 0.0)),
            ("Più grande", None, |a| wordcraft_engine::cmd::format::state(&editor(a).s)["size"].as_f64().is_some_and(|x| x > 12.0)),
            ("Più piccolo", None, |a| wordcraft_engine::cmd::format::state(&editor(a).s)["size"].as_f64().is_some_and(|x| x < 12.0)),
            ("Puntato", None, |a| primo(a).props.numbering.is_some()),
            ("Numerato", None, |a| primo(a).props.numbering.is_some()),
        ];
        let mut h = telefono_con_ciao_selezionato("foglio-comandi");
        for (etichetta, prima, giusto) in prove {
            ciao_selezionato(h.state_mut());
            h.run_steps(2);
            let dim_prima = wordcraft_engine::cmd::format::state(&editor(h.state()).s)["size"].as_f64();
            assert_eq!(dim_prima, Some(12.0), "la prova conta su un testo a 12 punti");
            if let Some(id) = prima {
                h.state_mut().editor_mut().unwrap().comando(id);
            }
            assert!(!giusto(h.state()), "«{etichetta}»: il documento era già così");
            tocca(&mut h, schermo_editor::FORMATO);
            tocca(&mut h, etichetta);
            assert!(giusto(h.state()), "«{etichetta}» non ha fatto il suo comando");
            // Il foglio resta aperto per il comando dopo.
            assert!(h.state().foglio_aa.aperto(), "«{etichetta}» ha chiuso il foglio");
        }
    }

    #[test]
    fn il_foglio_accende_lo_stile_del_paragrafo() {
        let mut h = telefono_con_ciao_selezionato("foglio-stile");
        tocca(&mut h, schermo_editor::FORMATO);
        assert!(acceso(&h, "Normale") && !acceso(&h, "Titolo 2"));
        tocca(&mut h, "Titolo 2");
        assert!(acceso(&h, "Titolo 2") && !acceso(&h, "Normale"));
    }

    #[test]
    fn il_foglio_legge_misura_allineamento_ed_elenchi_dal_paragrafo() {
        let mut h = telefono_con_ciao_selezionato("foglio-stato");
        tocca(&mut h, schermo_editor::FORMATO);
        h.run_steps(10);
        // La misura del testo fra meno e più, l'allineamento a sinistra, nessun elenco.
        h.get_by_label("12");
        assert!(acceso(&h, "A sinistra") && !acceso(&h, "Al centro"));
        assert!(!acceso(&h, "Puntato") && !acceso(&h, "Numerato"));
        tocca(&mut h, "Più piccolo");
        h.get_by_label("11");
        tocca(&mut h, "Al centro");
        assert!(acceso(&h, "Al centro") && !acceso(&h, "A sinistra"));
        // Toccato di nuovo resta al centro: è una scelta, non un interruttore come in Word.
        tocca(&mut h, "Al centro");
        assert!(acceso(&h, "Al centro"));
        tocca(&mut h, "Numerato");
        assert!(acceso(&h, "Numerato") && !acceso(&h, "Puntato"));
        // I campioni degli stili nell'aspetto del documento: il titolo col colore del suo stile.
        let titolo = tascabile_word::editor::aspetto_stile(&editor(h.state()).s, "heading 1").unwrap();
        let [r, g, b] = titolo.colore.unwrap();
        assert_eq!(colore_del_testo(&h, "Titolo 1"), Some(egui::Color32::from_rgb(r, g, b)));
    }

    #[test]
    fn colore_evidenziatore_e_togli_formato_dal_foglio() {
        use wordcraft_engine::doc::props::{Highlight, Rgb, TextColor};
        let mut h = telefono_con_ciao_selezionato("foglio-colori");
        tocca(&mut h, schermo_editor::FORMATO);
        h.run_steps(10);
        assert!(h.query_by_label("Rosso scuro").is_none(), "la tavolozza si apre col tocco");
        tocca(&mut h, "Colore del testo");
        h.run_steps(4);
        // Aperta: il pulsante del colore acceso, «Automatico» cerchiato (il colore di adesso).
        assert!(acceso(&h, "Colore del testo") && acceso(&h, "Automatico"));
        // Il tocco di kittest va dove il pulsante era al frame prima: si lascia assestare il
        // foglio, che con la tavolozza chiusa si accorcia.
        tocca(&mut h, "Rosso scuro");
        h.run_steps(4);
        let ciao = |a: &TascabileApp| primo(a).runs.first().unwrap().props.clone();
        assert_eq!(ciao(h.state()).color, Some(TextColor::Rgb(Rgb(0xC0, 0, 0))));
        assert!(h.query_by_label("Rosso scuro").is_none(), "scelto il colore, la tavolozza si chiude");
        assert!(h.state().foglio_aa.aperto(), "il foglio resta aperto");
        tocca(&mut h, "Evidenziatore");
        h.run_steps(4);
        assert!(acceso(&h, "Nessuno"));
        tocca(&mut h, "Giallo");
        h.run_steps(4);
        assert_eq!(ciao(h.state()).highlight, Some(Highlight::Yellow));
        // Riaperta, la tavolozza cerchia il colore di adesso.
        tocca(&mut h, "Colore del testo");
        h.run_steps(4);
        assert!(acceso(&h, "Rosso scuro") && !acceso(&h, "Automatico"));
        // Toccato di nuovo, il pulsante richiude la tavolozza.
        tocca(&mut h, "Colore del testo");
        h.run_steps(4);
        assert!(h.query_by_label("Rosso scuro").is_none());
        tocca(&mut h, "Togli il formato");
        let p = ciao(h.state());
        assert!(p.color.is_none() && p.highlight.is_none(), "{p:?}");
        let d = diagnosi_della_prova();
        assert!(d.contains("] comando: format.color\n") && d.contains("] comando: format.resetChar\n"), "{d}");
    }

    #[test]
    fn il_foglio_si_chiude_con_indietro_e_si_resta_nell_editor() {
        let mut h = telefono_con_ciao_selezionato("foglio-indietro");
        tocca(&mut h, schermo_editor::FORMATO);
        assert!(h.state().foglio_aa.aperto());
        assert!(h.query_by_label("Titolo 1").is_some());
        indietro(&mut h);
        assert!(!h.state().foglio_aa.aperto(), "Indietro non ha chiuso il foglio");
        assert!(h.query_by_label("Titolo 1").is_none());
        assert_eq!(h.state().schermata, Schermata::Editor);
        assert!(h.state().dialogo.is_none());
        assert_eq!(h.state().piattaforma.volte_in_secondo_piano(), 0);
        // Il secondo Indietro chiude la selezione di «Ciao», il terzo esce dall'Editor come sempre.
        indietro(&mut h);
        assert_eq!(h.state().schermata, Schermata::Editor);
        indietro(&mut h);
        assert_eq!(h.state().schermata, Schermata::Home);
        assert_eq!(h.state().piattaforma.volte_in_secondo_piano(), 0);
    }

    #[test]
    fn il_foglio_si_chiude_toccando_fuori() {
        let mut h = telefono_con_ciao_selezionato("foglio-fuori");
        tocca(&mut h, schermo_editor::FORMATO);
        let pos = egui::pos2(200.0, 150.0);
        h.event(egui::Event::PointerMoved(pos));
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
        }
        h.run_steps(3);
        assert!(!h.state().foglio_aa.aperto());
        assert_eq!(h.state().schermata, Schermata::Editor);
        // Il tocco fuori chiude il foglio e basta: il cursore non si sposta.
        assert!(!editor(h.state()).s.sel.is_collapsed(), "il tocco è arrivato al documento");
    }

    #[test]
    fn uscendo_dall_editor_il_foglio_si_chiude() {
        let mut h = telefono_con_ciao_selezionato("foglio-uscita");
        tocca(&mut h, schermo_editor::FORMATO);
        h.state_mut().chiudi_poi(Dopo::Home);
        h.run_steps(2);
        assert_eq!(h.state().schermata, Schermata::Home);
        assert!(!h.state().foglio_aa.aperto());
        h.state_mut().nuovo_documento();
        h.run_steps(2);
        assert!(h.query_by_label("Titolo 1").is_none(), "il foglio è ricomparso nel documento dopo");
    }
}
