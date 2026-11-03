//! Ponte tra Tascabile e il sistema: tastiera, file, «Apri con», appunti, condivisione.
//! `ime` e il registro della diagnosi sono Rust puro e si provano su Linux; il resto ha una
//! versione Android e una finta per il PC.

pub mod ime;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "android")]
pub use android::{Piattaforma, Rete, ricevi_foto_firma, ricevi_impronta, ricevi_insets, ricevi_nuovo_intent, ricevi_schermo_esterno, ricevi_stato_ime};

#[cfg(not(target_os = "android"))]
mod desktop;
#[cfg(not(target_os = "android"))]
pub use desktop::{Piattaforma, Rete};

/// La risposta della rete a una richiesta https del timestamp (l'unica cosa per cui l'app va
/// in rete). Mai l'indirizzo: ha dentro l'impronta di un file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RispostaRete {
    /// 200, coi byte della risposta.
    Ok(Vec<u8>),
    /// Un'altra risposta del server (404: quello che si chiede non c'è ancora).
    Stato(u16),
    /// Niente collegamento, o nessuna risposta in tempo.
    Collegamento,
    /// Più lunga di [`RISPOSTA_MASSIMA`] byte.
    TroppoLunga,
}

/// Una risposta più lunga di così non si legge (come nel client di OpenTimestamps).
pub const RISPOSTA_MASSIMA: usize = 10_000;

/// La riga della diagnosi di una richiesta: com'è andata e quanto ci è voluto, mai l'indirizzo.
pub(crate) fn riga_rete(r: &RispostaRete, durata: Duration) -> String {
    let ms = durata.as_millis();
    match r {
        RispostaRete::Ok(b) => format!("rete: 200 in {ms} ms, {} byte", b.len()),
        RispostaRete::Stato(s) => format!("rete: {s} in {ms} ms"),
        RispostaRete::Collegamento => format!("rete: nessun collegamento dopo {ms} ms"),
        RispostaRete::TroppoLunga => format!("rete: risposta troppo lunga dopo {ms} ms"),
    }
}

/// La risposta del Java di `tascabileRete`: lo stato in due byte (0 senza collegamento, 1 se troppo
/// lunga), poi i byte della risposta.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub(crate) fn risposta_dal_java(byte: &[u8]) -> RispostaRete {
    let Some((stato, corpo)) = byte.split_first_chunk::<2>() else { return RispostaRete::Collegamento };
    match u16::from_be_bytes(*stato) {
        0 => RispostaRete::Collegamento,
        1 => RispostaRete::TroppoLunga,
        200 if corpo.len() <= RISPOSTA_MASSIMA => RispostaRete::Ok(corpo.to_vec()),
        200 => RispostaRete::TroppoLunga,
        s => RispostaRete::Stato(s),
    }
}

/// Stato della tastiera come lo manda Android (indici in unità UTF-16).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImeSnapshot {
    pub text: String,
    /// (inizio, fine) della selezione; il cursore è la fine. Java può mandare -1 (nessuna
    /// selezione): su Android lo sistema solo `ricevi_stato_ime`, col cursore alla fine del testo.
    pub sel: (i32, i32),
    /// Regione in composizione, (-1, -1) se assente.
    pub comp: (i32, i32),
    /// Versione dell'ultimo stato dato dall'app quando la tastiera ha prodotto questo.
    pub version: i64,
}

/// Una richiesta «Apri con».
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenRequest {
    pub uri: String,
    pub mime: Option<String>,
    /// Il nome vero del file (`OpenableColumns.DISPLAY_NAME`), se l'app che lo manda lo dice: per
    /// i recenti e come base di «Salva una copia». Mai nella diagnosi.
    pub nome: Option<String>,
}

/// Da dove viene la foto della firma su carta (piano «PDF completo», P3): la fotocamera di
/// sistema (`ACTION_IMAGE_CAPTURE`) o un'immagine scelta (`ACTION_GET_CONTENT`). Lo stesso Intent
/// porta anche un PDF scelto da unire (P6) e le foto delle pagine di una scansione scelte dalla
/// galleria, anche più d'una (0.8.3). Ogni richiesta ha il suo numero e la sua cartella nella
/// cache ([`cartella_delle_scelte`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FonteFoto {
    Fotocamera,
    Galleria,
    /// Un PDF scelto con `ACTION_GET_CONTENT` (`application/pdf`), fino a 200 MB.
    Pdf,
    /// Una o più immagini scelte insieme (`ACTION_GET_CONTENT` con `EXTRA_ALLOW_MULTIPLE`), per le
    /// pagine di una scansione: copiate nei file delle pagine nella cartella della richiesta
    /// (`file_della_pagina`), nell'ordine, al più [`MASSIMO_PAGINE_SCELTE`].
    GalleriaPiu,
}

/// Le foto scelte insieme dalla galleria, al più.
pub const MASSIMO_PAGINE_SCELTE: usize = 30;

impl FonteFoto {
    /// Il numero per Java (`MainActivity.tascabileFotoFirma`).
    pub fn codice(self) -> i32 {
        match self {
            FonteFoto::Fotocamera => 0,
            FonteFoto::Galleria => 1,
            FonteFoto::Pdf => 2,
            FonteFoto::GalleriaPiu => 3,
        }
    }

    /// La fonte dal suo numero.
    pub fn da_codice(codice: i32) -> Option<FonteFoto> {
        [FonteFoto::Fotocamera, FonteFoto::Galleria, FonteFoto::Pdf, FonteFoto::GalleriaPiu].into_iter().find(|f| f.codice() == codice)
    }

    /// Per la diagnosi.
    pub fn nome(self) -> &'static str {
        match self {
            FonteFoto::Fotocamera => "fotocamera",
            FonteFoto::Galleria | FonteFoto::GalleriaPiu => "galleria",
            FonteFoto::Pdf => "scelta dei file",
        }
    }

    /// La riga della diagnosi quando la si chiede.
    pub fn chiesta(self) -> String {
        match self {
            FonteFoto::Pdf => "pdf da unire: chiesto alla scelta dei file".to_string(),
            FonteFoto::GalleriaPiu => "foto: chieste alla galleria, anche più d'una".to_string(),
            f => format!("foto della firma: chiesta alla {}", f.nome()),
        }
    }
}

/// Com'è andata la foto della firma chiesta ad Android.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EsitoFoto {
    /// La foto è in questo file della cache dell'app: chi la legge la cancella.
    Pronta(std::path::PathBuf),
    /// Le foto scelte dalla galleria con [`FonteFoto::GalleriaPiu`], in questi file della cache,
    /// nell'ordine della scelta: chi le legge le cancella. `scelte` dice quante ne aveva scelte chi
    /// usa l'app: di più, se erano oltre le 30 o se qualcuna non si è letta.
    Pronte { file: Vec<std::path::PathBuf>, scelte: usize },
    /// Chi usa l'app è tornato indietro senza foto.
    Annullata,
    /// Nessuna app per la foto, o la foto non si è letta.
    Errore,
    /// Nel frattempo è partita un'altra richiesta: questa non risponderà più, e chi la aspettava
    /// smette di aspettarla.
    Superata,
}

/// La cartella delle foto della richiesta `richiesta`, nella cache dell'app `cache`: una per
/// richiesta, così due richieste non scrivono mai sugli stessi file.
pub fn cartella_delle_scelte(cache: &std::path::Path, richiesta: i32) -> std::path::PathBuf {
    cache.join("foto").join(format!("scelte-{richiesta}"))
}

/// Il file `i` (da 1) della richiesta `richiesta`: la foto della fotocamera (che la scrive lì
/// tramite il FileProvider), l'immagine o il PDF scelto (sempre il primo), le foto scelte dalla
/// galleria per la scansione (copiate lì da MainActivity).
pub fn file_della_pagina(cache: &std::path::Path, richiesta: i32, i: usize) -> std::path::PathBuf {
    cartella_delle_scelte(cache, richiesta).join(format!("pagina-{i}"))
}

/// L'esito della foto dai numeri che manda Java (`nativeFotoFirma`) per la richiesta `richiesta`
/// della fonte `fonte`: `n` da 1 in su pronte (per più foto dalla galleria, tante quante `n`, su
/// `scelte` scelte), 0 annullata, sotto zero errore.
pub fn esito_dal_java(n: i32, scelte: i32, fonte: FonteFoto, richiesta: i32, cache: &std::path::Path) -> EsitoFoto {
    match n {
        n if n >= 1 && fonte == FonteFoto::GalleriaPiu => {
            let file: Vec<std::path::PathBuf> =
                (1..=(n as usize).min(MASSIMO_PAGINE_SCELTE)).map(|i| file_della_pagina(cache, richiesta, i)).collect();
            let scelte = (scelte.max(n)) as usize;
            EsitoFoto::Pronte { file, scelte }
        }
        n if n >= 1 => EsitoFoto::Pronta(file_della_pagina(cache, richiesta, 1)),
        0 => EsitoFoto::Annullata,
        _ => EsitoFoto::Errore,
    }
}

/// Le foto chieste ad Android, una richiesta alla volta (sul telefono dietro un Mutex: l'esito
/// arriva dal thread di Java, la domanda da quello dell'interfaccia). Ogni richiesta ha il suo
/// numero: l'esito di una richiesta superata da una nuova non risponde più e le sue foto si
/// buttano; chi aspettava quella vecchia, chiedendola, sa che è superata. Così una funzione che
/// aspetta ancora non prende la foto chiesta da un'altra (0.8.3, revisione del codice).
#[derive(Debug, Default)]
pub struct CanaleFoto {
    richiesta: i32,
    fonte: Option<FonteFoto>,
    esito: Option<(i32, i32)>,
}

impl CanaleFoto {
    /// Nessuna richiesta ancora (per uno `static`).
    pub const NUOVO: CanaleFoto = CanaleFoto { richiesta: 0, fonte: None, esito: None };

    /// Una richiesta nuova da `fonte`: il suo numero, il primo da `seme` (perché non torni uguale
    /// a uno di prima che Android togliesse l'app), poi uno in più; sempre positivo.
    pub fn chiedi(&mut self, fonte: FonteFoto, seme: impl FnOnce() -> i32) -> i32 {
        let n = match self.richiesta {
            r if r <= 0 => seme().max(1),
            r => r.checked_add(1).unwrap_or(1),
        };
        *self = CanaleFoto { richiesta: n, fonte: Some(fonte), esito: None };
        n
    }

    /// La richiesta in corso (0 prima della prima).
    pub fn in_corso(&self) -> i32 {
        self.richiesta
    }

    /// L'esito che manda Java per `richiesta`: vero se è quella in corso, e allora resta per
    /// [`CanaleFoto::prendi`]; falso se è superata (o non c'è mai stata), e chi lo riceve butta
    /// la sua cartella.
    pub fn ricevi(&mut self, richiesta: i32, esito: i32, scelte: i32) -> bool {
        if richiesta <= 0 || richiesta != self.richiesta || self.fonte.is_none() {
            return false;
        }
        self.esito = Some((esito, scelte));
        true
    }

    /// L'esito della richiesta `richiesta`, una volta sola: None se non è ancora arrivato,
    /// [`EsitoFoto::Superata`] se nel frattempo ne è partita un'altra.
    pub fn prendi(&mut self, richiesta: i32, cache: &std::path::Path) -> Option<EsitoFoto> {
        if richiesta != self.richiesta {
            return Some(EsitoFoto::Superata);
        }
        let (n, scelte) = self.esito.take()?;
        Some(esito_dal_java(n, scelte, self.fonte?, richiesta, cache))
    }
}

/// Via tutte le foto della cache (`foto/`): all'avvio non ne aspetta nessuno, e una scelta della
/// galleria mai letta (Android ha tolto l'app mentre era aperta) non resta sul telefono.
pub fn pulisci_le_foto(cache: &std::path::Path) {
    let _ = std::fs::remove_dir_all(cache.join("foto"));
}

/// Via le copie condivise (`condivisi/` della cache) più vecchie di `eta`: all'avvio, perché
/// l'ultimo documento condiviso non resti nella cache (audit del 30/10, problema 5). Una cartella
/// `<millisecondi>-<n>` (`salva::condivisi`) conta per l'ora del suo nome, il resto per la data
/// di modifica; una condivisa da poco resta, perché l'app che la riceve può star ancora leggendola.
pub fn pulisci_i_condivisi(cache: &std::path::Path, eta: std::time::Duration) {
    let Ok(voci) = std::fs::read_dir(cache.join("condivisi")) else { return };
    let adesso = std::time::SystemTime::now();
    for v in voci.flatten() {
        let dal_nome = v
            .file_name()
            .to_str()
            .and_then(|n| n.split('-').next()?.parse::<u64>().ok())
            .map(|ms| std::time::UNIX_EPOCH + std::time::Duration::from_millis(ms));
        let quando = dal_nome.or_else(|| v.metadata().and_then(|m| m.modified()).ok());
        let vecchia = quando.is_none_or(|q| adesso.duration_since(q).is_ok_and(|d| d > eta));
        if vecchia {
            let p = v.path();
            let _ = if p.is_dir() { std::fs::remove_dir_all(&p) } else { std::fs::remove_file(&p) };
        }
    }
}

/// Dopo quanto una copia condivisa se ne va, all'avvio dell'app.
pub const ETA_DEI_CONDIVISI: std::time::Duration = std::time::Duration::from_secs(3_600);

/// L'esito della finestra dell'impronta di Android (BiometricPrompt): impronta, PIN, sequenza o
/// password del telefono (roadmap della sicurezza, Fase 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EsitoImpronta {
    Riuscita,
    /// Annullata da chi usa l'app (Indietro, «Annulla», un tocco fuori).
    Annullata,
    /// Il telefono non ha un blocco dello schermo: non c'è niente da chiedere.
    SenzaBlocco,
    Errore,
}

impl EsitoImpronta {
    /// Dal codice di MainActivity: 1 riuscita, 0 annullata, -2 senza blocco, altro errore.
    pub fn da_codice(codice: i32) -> Self {
        match codice {
            1 => EsitoImpronta::Riuscita,
            0 => EsitoImpronta::Annullata,
            -2 => EsitoImpronta::SenzaBlocco,
            _ => EsitoImpronta::Errore,
        }
    }

    pub fn genere(self) -> &'static str {
        match self {
            EsitoImpronta::Riuscita => "riuscita",
            EsitoImpronta::Annullata => "annullata",
            EsitoImpronta::SenzaBlocco => "telefono senza blocco",
            EsitoImpronta::Errore => "errore",
        }
    }
}

/// Al più tanti caratteri incollati: un testo vero, non un file (revisione del 31/10, punto 6;
/// lo stesso tetto c'è in MainActivity.tascabilePaste).
pub const MASSIMO_INCOLLA: usize = 1_000_000;

/// Il testo degli appunti, se sta nel tetto dell'incolla; se no niente, e una riga di diagnosi.
pub(crate) fn entro_il_tetto_dell_incolla(testo: Option<String>) -> Option<String> {
    match testo {
        Some(t) if t.chars().count() > MASSIMO_INCOLLA => {
            diagnosi("appunti: testo oltre il milione di caratteri, non incollato");
            None
        }
        t => t,
    }
}

/// L'autorità del nostro FileProvider (AndroidManifest.xml).
pub const AUTORITA_NOSTRA: &str = "it.tascabile.app.condivisione";

/// Un URI di «Apri con» da aprire: solo content://, che le altre app danno col permesso di
/// leggerlo, e mai del nostro FileProvider. Il filtro del manifest accetta solo content://, ma
/// un'altra app può chiamare l'Activity con un Intent esplicito e un file://, anche di un file
/// privato di Tascabile (audit del 30/10, problema 4). Il perché va nella diagnosi, l'URI mai.
pub fn uri_da_aprire(uri: &str) -> Result<(), &'static str> {
    let Some((schema, resto)) = uri.trim().split_once(':') else { return Err("senza schema") };
    if !schema.eq_ignore_ascii_case("content") {
        return Err(if schema.eq_ignore_ascii_case("file") { "file" } else { "altro schema" });
    }
    // L'autorità come la legge Android (`Uri.getAuthority`), che poi sceglie il provider: finisce
    // anche alla «\» ed è decodificata, «%2E» è un punto (revisione del 31/10, punto 5).
    let autorita = resto.strip_prefix("//").and_then(|r| r.split(['/', '\\', '?', '#']).next()).unwrap_or_default();
    let autorita = decodificata(autorita);
    // Un URI di un altro utente del telefono: «10@media».
    let autorita = autorita.rsplit('@').next().unwrap_or_default();
    if autorita.is_empty() {
        return Err("senza autorità");
    }
    if autorita.eq_ignore_ascii_case(AUTORITA_NOSTRA) {
        return Err("nostro FileProvider");
    }
    Ok(())
}

/// Un pezzo di URI con le sequenze «%XX» decodificate, come `Uri.decode` di Android.
fn decodificata(s: &str) -> String {
    let b = s.as_bytes();
    let mut v = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let cifre = b.get(i + 1..i + 3).and_then(|c| std::str::from_utf8(c).ok()).and_then(|c| u8::from_str_radix(c, 16).ok());
        match cifre {
            Some(c) if b[i] == b'%' => {
                v.push(c);
                i += 3;
            }
            _ => {
                v.push(b[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&v).into_owned()
}

/// Un file che `read_uri` non è riuscita a leggere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErroreLettura {
    /// Per la diagnosi: la classe dell'eccezione Java, il genere dell'errore di lettura o il
    /// tetto superato. Mai il nome del file né l'URI.
    pub genere: String,
    /// Per lo schermo (`Display`): può contenere l'URI o il percorso.
    pub messaggio: String,
}

impl std::fmt::Display for ErroreLettura {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.messaggio)
    }
}

/// Il tetto di un file da aprire, da «Apri con», dalla cartella o dai recenti: un file enorme non
/// deve finire tutto in memoria (rapporto di sicurezza del 24/10, problema 2; prima 200 MB). La
/// regola era per gli archivi zip dei documenti Word: i PDF hanno `LIMITE_PDF_MB`.
pub const LIMITE_APERTURA_MB: u64 = 50;

/// Il tetto di un PDF, lo stesso del motore PDF (`tascabile_pdf::LIMITE_MB`): una scansione di
/// cento pagine passa facilmente i 50 MB.
pub const LIMITE_PDF_MB: u64 = 200;

/// Dove si cerca l'intestazione di un PDF: `%PDF-` nei primi 1024 byte, come
/// `tascabile_pdf::sembra_pdf`.
const TESTA_PDF: usize = 1024 + 4;

/// Il file comincia come un PDF (`%PDF-` nei primi 1024 byte): decide il tetto di lettura.
pub fn comincia_come_pdf(byte: &[u8]) -> bool {
    byte[..byte.len().min(TESTA_PDF)].windows(5).any(|w| w == b"%PDF-")
}

/// Legge tutto `sorgente`, al massimo `LIMITE_APERTURA_MB`, o `LIMITE_PDF_MB` se comincia come un
/// PDF (lo si guarda dai primi byte, prima di leggere il resto). Nel genere dell'errore solo il
/// tipo dell'errore di I/O o il tetto; il messaggio, per lo schermo, comincia con `dove` (l'URI o
/// il percorso, che nella diagnosi non vanno).
pub fn leggi_con_tetto(sorgente: impl std::io::Read, dove: &str) -> Result<Vec<u8>, ErroreLettura> {
    use std::io::Read;
    let mut sorgente = sorgente;
    let errore = |e: std::io::Error| ErroreLettura { genere: format!("{:?}", e.kind()), messaggio: format!("{dove}: {e}") };
    let mut byte = Vec::new();
    (&mut sorgente).take(TESTA_PDF as u64).read_to_end(&mut byte).map_err(errore)?;
    let pdf = comincia_come_pdf(&byte);
    let mb = if pdf { LIMITE_PDF_MB } else { LIMITE_APERTURA_MB };
    let limite = mb * 1024 * 1024;
    sorgente.take(limite + 1 - byte.len() as u64).read_to_end(&mut byte).map_err(errore)?;
    if byte.len() as u64 > limite {
        let messaggio = if pdf {
            format!("il PDF supera i {mb} MB: in questa versione non si apre")
        } else {
            format!("file più grande di {mb} MB: in questa versione non si apre")
        };
        return Err(ErroreLettura { genere: format!("oltre {mb} MB"), messaggio });
    }
    Ok(byte)
}

/// Che genere di indirizzo è un link esterno che il telefono può aprire fuori dall'app: una
/// pagina web (http, https) o una mail (mailto). Gli altri (file, javascript, intent, tel e ogni
/// schema sconosciuto) no: None. Il genere va nella diagnosi, l'indirizzo mai.
pub fn genere_indirizzo(indirizzo: &str) -> Option<&'static str> {
    let indirizzo = indirizzo.trim();
    let (schema, resto) = indirizzo.split_once(':')?;
    if resto.is_empty() || indirizzo.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return None;
    }
    match schema.to_ascii_lowercase().as_str() {
        "http" | "https" if resto.starts_with("//") && resto.len() > 2 => Some("pagina web"),
        "mailto" => Some("posta"),
        _ => None,
    }
}

/// La riga di un panico per il log e la diagnosi: il thread e il punto del codice (file e riga),
/// mai il messaggio, che può riportare pezzi del documento o il nome di un file.
pub fn riga_del_panico(thread: Option<&str>, dove: Option<&std::panic::Location<'_>>) -> String {
    let thread = thread.unwrap_or("senza nome");
    match dove {
        Some(d) => format!("panico nel thread {thread} in {}:{}", d.file(), d.line()),
        None => format!("panico nel thread {thread} in un punto sconosciuto"),
    }
}

/// Il gancio dei panici di tutta l'app: al posto di quello di Rust, che scrive il messaggio su
/// stderr (e su Android stderr finisce in logcat), scrive nel log e nella diagnosi solo
/// `riga_del_panico`. Una volta, all'avvio; mai nelle prove, dove i messaggi servono.
pub fn installa_gancio_dei_panici() {
    std::panic::set_hook(Box::new(|info| {
        let riga = riga_del_panico(std::thread::current().name(), info.location());
        log::error!("{riga}");
        diagnosi_dal_gancio(&riga);
    }));
}

/// Tasti che la tastiera non applica al suo testo e lascia all'app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TastoCrudo {
    Backspace,
    Canc,
    Invio,
}

/// Altezza della tastiera simulata sul PC, in punti.
pub const TASTIERA_FINTA_PT: f32 = 280.0;

/// Che tastiera vuole il campo che ha il fuoco: sul telefono decide l'EditorInfo di GameTextInput
/// (MainActivity.tascabileTipoTastiera, con restartInput), solo mentre quel campo ha il fuoco.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TipoTastiera {
    /// Il documento e i campi di testo: frasi con la maiuscola, correzioni e suggerimenti.
    #[default]
    Testo,
    /// Il numero di una pagina: la tastiera delle cifre.
    Numeri,
    /// Una password: niente suggerimenti, correzioni né maiuscole, e la tastiera non la impara.
    Password,
}

impl TipoTastiera {
    /// Il numero che riceve MainActivity.tascabileTipoTastiera.
    pub fn codice(self) -> i32 {
        match self {
            TipoTastiera::Testo => 0,
            TipoTastiera::Numeri => 1,
            TipoTastiera::Password => 2,
        }
    }

    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    pub(crate) fn da_codice(codice: u8) -> TipoTastiera {
        match codice {
            1 => TipoTastiera::Numeri,
            2 => TipoTastiera::Password,
            _ => TipoTastiera::Testo,
        }
    }

    /// Per la diagnosi.
    pub fn nome(self) -> &'static str {
        match self {
            TipoTastiera::Testo => "testo",
            TipoTastiera::Numeri => "numeri",
            TipoTastiera::Password => "password",
        }
    }
}

// Diagnosi: registro degli eventi in memoria, da mostrare e da condividere.
//
// Regola: mai il testo dell'utente (documento, tastiera, appunti, contenuto dei file) e mai nomi
// di file o URI, che possono essere personali. Solo misure, versioni, posizioni, comandi, tipi di
// file ed errori che non riportano contenuti.

/// Quanti eventi tiene il registro: oltre, esce il più vecchio.
pub const RIGHE_DIAGNOSI: usize = 300;

/// Le righe che restano sempre in testa al registro (`diagnosi_fissa`), in quest'ordine. Sono
/// scritte una volta sola, e il giro delle `RIGHE_DIAGNOSI` righe le farebbe uscire presto: ogni
/// lettera battuta ne lascia almeno una.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fissa {
    /// Versione dell'app e piattaforma.
    Avvio,
    /// Quanto ci ha messo l'app, dall'avvio, a disegnare la prima schermata (di solito la Home).
    PrimoDisegno,
    /// Accesso a tutti i file: all'avvio, poi dopo ogni richiesta.
    AccessoFile,
    /// L'ultimo «Apri con»: tipo e misura del file, com'è andato.
    ApriCon,
    /// L'ultimo salvataggio (Salva, Salva una copia, Esporta PDF, Condividi): com'è andato, il
    /// genere dell'errore, tipo e misura del file.
    Salvataggio,
}

impl Fissa {
    /// Quante sono: l'indice dell'ultima più uno (da cambiare se se ne aggiunge una in fondo).
    const QUANTE: usize = Fissa::Salvataggio as usize + 1;
}

/// Caratteri al massimo per evento: anche un errore lunghissimo resta una riga corta.
const CARATTERI_PER_EVENTO: usize = 300;

/// Quando è successo un evento.
#[derive(Debug, Clone, Copy)]
struct Quando {
    /// Dall'avvio (`Instant`): ha i millesimi e non torna mai indietro, ma si ferma quando il
    /// telefono dorme (CLOCK_MONOTONIC).
    dopo: Duration,
    /// L'ora vera: dice quanto tempo è passato davvero fra due righe, anche a telefono addormentato,
    /// e a che ora del giorno è successo.
    ora: SystemTime,
}

struct Riga {
    quando: Quando,
    /// Tipo degli eventi a raffica (`diagnosi_raffica`), None per gli altri.
    tipo: Option<&'static str>,
    testo: String,
}

/// Gli ultimi eventi, nell'ordine in cui sono arrivati, e le righe fisse.
struct Registro {
    righe: VecDeque<Riga>,
    tetto: usize,
    /// Una riga per ogni `Fissa`, fuori dal giro delle `tetto` righe.
    fisse: [Option<Riga>; Fissa::QUANTE],
}

impl Registro {
    const fn nuovo(tetto: usize) -> Self {
        Self { righe: VecDeque::new(), tetto, fisse: [const { None }; Fissa::QUANTE] }
    }

    /// Un evento a raffica (`tipo`) prende il posto dell'ultima riga se è dello stesso tipo:
    /// resta l'ultimo valore, col suo tempo.
    fn scrivi(&mut self, quando: Quando, tipo: Option<&'static str>, evento: &str) {
        let riga = Riga { quando, tipo, testo: una_riga(evento) };
        if let Some(t) = tipo
            && let Some(ultima) = self.righe.back_mut()
            && ultima.tipo == Some(t)
        {
            *ultima = riga;
            return;
        }
        if self.righe.len() >= self.tetto {
            self.righe.pop_front();
        }
        self.righe.push_back(riga);
    }

    /// Una riga fissa (`Fissa`): prende il posto della sua vecchia in testa e va anche fra gli
    /// ultimi eventi, al suo posto.
    fn scrivi_fissa(&mut self, quando: Quando, quale: Fissa, evento: &str) {
        self.fisse[quale as usize] = Some(Riga { quando, tipo: None, testo: una_riga(evento) });
        self.scrivi(quando, None, evento);
    }

    /// Le righe fisse, se ce ne sono, poi gli ultimi eventi.
    fn testo(&self) -> String {
        let riga = |r: &Riga| format!("[{:>8.3} {}] {}\n", r.quando.dopo.as_secs_f64(), ora_utc(r.quando.ora), r.testo);
        let mut testo = String::new();
        if self.fisse.iter().any(Option::is_some) {
            testo.push_str("In evidenza:\n");
            testo.extend(self.fisse.iter().flatten().map(riga));
            testo.push_str("Ultimi eventi, dal più vecchio:\n");
        }
        testo.extend(self.righe.iter().map(riga));
        testo
    }
}

/// «gg/mm hh:mm:ss» in UTC: il fuso orario no, la libreria standard non lo conosce.
fn ora_utc(ora: SystemTime) -> String {
    let Ok(da_epoca) = ora.duration_since(SystemTime::UNIX_EPOCH) else {
        return "??/?? ??:??:??".to_string();
    };
    let secondi = da_epoca.as_secs();
    let (giorni, nel_giorno) = (secondi / 86_400, secondi % 86_400);
    let (_, mese, giorno) = data_civile(giorni);
    format!("{giorno:02}/{mese:02} {:02}:{:02}:{:02}", nel_giorno / 3600, nel_giorno / 60 % 60, nel_giorno % 60)
}

/// (anno, mese, giorno) del giorno `giorni` dopo l'1/1/1970, nel calendario gregoriano
/// (`civil_from_days` di Howard Hinnant, con le ere di 400 anni).
fn data_civile(giorni: u64) -> (u64, u64, u64) {
    let z = giorni + 719_468; // giorni dall'1/3/0000
    let era = z / 146_097;
    let giorno_dell_era = z % 146_097;
    let anno_dell_era = (giorno_dell_era - giorno_dell_era / 1460 + giorno_dell_era / 36_524 - giorno_dell_era / 146_096) / 365;
    let giorno_dell_anno = giorno_dell_era - (365 * anno_dell_era + anno_dell_era / 4 - anno_dell_era / 100); // dall'1/3
    let m = (5 * giorno_dell_anno + 2) / 153; // 0 = marzo
    let giorno = giorno_dell_anno - (153 * m + 2) / 5 + 1;
    let mese = if m < 10 { m + 3 } else { m - 9 };
    (era * 400 + anno_dell_era + u64::from(mese <= 2), mese, giorno)
}

/// Un evento è sempre una riga: niente a capo né altri caratteri di controllo, al massimo
/// `CARATTERI_PER_EVENTO` caratteri (poi «…»).
fn una_riga(evento: &str) -> String {
    let mut riga: String = evento.chars().take(CARATTERI_PER_EVENTO).map(|c| if c.is_control() { ' ' } else { c }).collect();
    if evento.chars().nth(CARATTERI_PER_EVENTO).is_some() {
        riga.push('…');
    }
    riga
}

static REGISTRO: Mutex<Registro> = Mutex::new(Registro::nuovo(RIGHE_DIAGNOSI));

thread_local! {
    /// Le righe scritte da questo thread da quando una prova le ascolta (`ascolta_diagnosi`).
    static ASCOLTATE: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Per le prove: da adesso le righe della diagnosi scritte da questo thread si tengono anche a
/// parte, fuori dal giro delle `RIGHE_DIAGNOSI` righe del registro, che le prove in parallelo
/// riempiono in fretta. [`righe_ascoltate`] le restituisce. Ricomincia da capo a ogni chiamata.
pub fn ascolta_diagnosi() {
    ASCOLTATE.with(|a| *a.borrow_mut() = Some(String::new()));
}

/// Le righe scritte da questo thread dopo `ascolta_diagnosi`, senza i tempi, una per riga.
pub fn righe_ascoltate() -> String {
    ASCOLTATE.with(|a| a.borrow().clone().unwrap_or_default())
}
/// Istante zero del registro: il primo evento del processo, cioè l'avvio dell'app.
static ORIGINE: OnceLock<Instant> = OnceLock::new();

/// Scrive `evento` nel registro con `scrivi`, che riceve il momento dell'evento.
fn registra(evento: &str, scrivi: impl FnOnce(&mut Registro, Quando)) {
    let origine = *ORIGINE.get_or_init(Instant::now);
    // Copia nel log solo a livello debug (sul telefono non si vede): `RUST_LOG=diagnosi=debug`.
    log::debug!(target: "diagnosi", "{evento}");
    ASCOLTATE.with(|a| {
        if let Some(righe) = a.borrow_mut().as_mut() {
            righe.push_str(&una_riga(evento));
            righe.push('\n');
        }
    });
    let mut registro = REGISTRO.lock().unwrap_or_else(|e| e.into_inner());
    scrivi_quelle_da_parte(&mut registro);
    // Il tempo si prende dentro il lucchetto: così le righe restano in ordine di tempo anche
    // quando scrivono più thread (quello di android_main e quello principale di Java).
    scrivi(&mut registro, Quando { dopo: origine.elapsed(), ora: SystemTime::now() });
}

/// Le righe dei panici arrivate col registro occupato, col loro momento: si scrivono appena
/// qualcuno prende il registro.
static DA_PARTE: Mutex<Vec<(Quando, String)>> = Mutex::new(Vec::new());

/// Scrive nel registro le righe messe da parte dal gancio dei panici. Mai in attesa: se le tiene
/// il gancio in questo momento, aspettano la prossima volta.
fn scrivi_quelle_da_parte(registro: &mut Registro) {
    if let Ok(mut da_parte) = DA_PARTE.try_lock() {
        for (quando, riga) in da_parte.drain(..) {
            registro.scrivi(quando, None, &riga);
        }
    }
}

/// Come `diagnosi`, dal gancio dei panici: il panico può essere arrivato col registro bloccato
/// da questo stesso thread, quindi niente attesa: se è occupato, la riga va da parte e si scrive
/// con la prossima riga o quando si legge il registro (prima si perdeva).
fn diagnosi_dal_gancio(evento: &str) {
    let origine = *ORIGINE.get_or_init(Instant::now);
    let quando = Quando { dopo: origine.elapsed(), ora: SystemTime::now() };
    match REGISTRO.try_lock() {
        Ok(mut registro) => {
            scrivi_quelle_da_parte(&mut registro);
            registro.scrivi(quando, None, evento);
        }
        Err(_) => {
            if let Ok(mut da_parte) = DA_PARTE.try_lock() {
                da_parte.push((quando, evento.to_string()));
            }
        }
    }
}

/// Aggiunge un evento al registro della diagnosi (ultimi `RIGHE_DIAGNOSI`, con i secondi
/// dall'avvio e l'ora UTC). Da qualunque thread. Mai testo dell'utente né nomi di file.
pub fn diagnosi(evento: impl Into<String>) {
    let evento = evento.into();
    registra(&evento, |r, quando| r.scrivi(quando, None, &evento));
}

/// Come `diagnosi`, per le righe che servono sempre (`Fissa`): oltre che fra gli ultimi eventi,
/// la riga resta in testa al registro, la più recente per ogni `quale`, anche quando il giro delle
/// righe l'ha fatta uscire.
pub fn diagnosi_fissa(quale: Fissa, evento: impl Into<String>) {
    let evento = evento.into();
    registra(&evento, |r, quando| r.scrivi_fissa(quando, quale, &evento));
}

/// Come `diagnosi`, per gli eventi che arrivano a raffica (gli insets, che cambiano più volte per
/// una sola apertura della tastiera): se anche l'ultima riga del registro è di questo `tipo`, la
/// sostituisce invece di aggiungerne una. Così resta una riga, col valore finale.
pub fn diagnosi_raffica(tipo: &'static str, evento: impl Into<String>) {
    let evento = evento.into();
    registra(&evento, |r, quando| r.scrivi(quando, Some(tipo), &evento));
}

/// In cima al registro condiviso: come leggere i due tempi di ogni riga.
const LEGENDA: &str = "Tempi: secondi dall'avvio (si fermano quando il telefono dorme) e ora UTC (in Italia un'ora in più, due con l'ora legale).";

/// Il registro della diagnosi: la legenda dei tempi, le righe fisse (`diagnosi_fissa`), poi una
/// riga per evento, dal più vecchio:
/// `[  12.345 23/10 07:00:12] tastiera: stato v7, 23 unità, sel 5..5, applicato`.
pub fn diagnosi_testo() -> String {
    let mut registro = REGISTRO.lock().unwrap_or_else(|e| e.into_inner());
    scrivi_quelle_da_parte(&mut registro);
    format!("{LEGENDA}\n{}", registro.testo())
}

/// Le richieste di aprire e chiudere la tastiera, nella diagnosi. Su Android la tastiera resta
/// visibile per tutta l'animazione di chiusura, e intanto l'app la chiede chiusa a ogni frame
/// (`dopo_ponte`, finché il ponte è chiuso): si scrive solo la prima richiesta di una serie. La
/// serie finisce quando si chiede di aprirla, o quando la tastiera si riapre da sé (`riaperta`).
pub(crate) struct RichiesteTastiera {
    chiusura_chiesta: AtomicBool,
}

impl RichiesteTastiera {
    pub(crate) const fn nuove() -> Self {
        Self { chiusura_chiesta: AtomicBool::new(false) }
    }

    pub(crate) fn apri(&self) {
        self.chiusura_chiesta.store(false, Ordering::Relaxed);
        diagnosi(righe::TASTIERA_APRI);
    }

    pub(crate) fn chiudi(&self) {
        if !self.chiusura_chiesta.swap(true, Ordering::Relaxed) {
            diagnosi(righe::TASTIERA_CHIUDI);
        }
    }

    /// La tastiera si è aperta senza una richiesta dell'app (su Android: onApplyWindowInsets con
    /// la tastiera visibile): la prossima richiesta di chiuderla va scritta.
    #[cfg(any(target_os = "android", test))]
    pub(crate) fn riaperta(&self) {
        self.chiusura_chiesta.store(false, Ordering::Relaxed);
    }
}

/// Barre di sistema e tastiera come le manda MainActivity, in pixel fisici.
#[cfg(any(target_os = "android", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Insets {
    pub(crate) sinistra: i32,
    pub(crate) sopra: i32,
    pub(crate) destra: i32,
    pub(crate) sotto: i32,
    pub(crate) tastiera: i32,
    pub(crate) visibile: bool,
}

/// La riga della diagnosi per gli insets appena arrivati, col suo tipo per `diagnosi_raffica`;
/// None se non va scritta. `finale` vuol dire che vengono da onApplyWindowInsets; `ultimi` sono
/// quelli dell'ultima riga.
///
/// Solo onApplyWindowInsets lascia una riga, e solo se cambia qualcosa: prima di un'animazione
/// della tastiera manda lo stato di fine animazione. I frame dell'animazione (onProgress) no:
/// arrivano a ogni frame, e la tastiera vi risulta chiusa nel primo frame di un'apertura e
/// visibile in tutti i frame di una chiusura tranne l'ultimo. Il tipo dice se la tastiera è
/// visibile: righe di fila con lo stesso tipo diventano una, col valore finale.
#[cfg(any(target_os = "android", test))]
pub(crate) fn riga_insets(ultimi: &mut Option<Insets>, nuovi: Insets, finale: bool) -> Option<(&'static str, String)> {
    if !finale || *ultimi == Some(nuovi) {
        return None;
    }
    *ultimi = Some(nuovi);
    let (tipo, stato) = if nuovi.visibile { ("insets, tastiera visibile", "visibile") } else { ("insets, tastiera chiusa", "chiusa") };
    let Insets { sinistra, sopra, destra, sotto, tastiera, .. } = nuovi;
    Some((tipo, format!("insets: barre sopra {sopra} px, sotto {sotto} px, ai lati {sinistra} e {destra} px; tastiera {tastiera} px, {stato}")))
}

/// Righe della diagnosi uguali sulle due piattaforme, così le prove sul PC valgono anche per il
/// telefono: solo misure, mai il testo.
mod righe {
    use std::path::{Path, PathBuf};

    pub(crate) const TASTIERA_APRI: &str = "tastiera: richiesta di aprirla";
    pub(crate) const TASTIERA_CHIUDI: &str = "tastiera: richiesta di chiuderla";
    /// Un file da condividere che non sta in `cartella_cache()/condivisi/`: non si condivide.
    pub(crate) const CONDIVIDI_FUORI: &str = "condividi file: fuori dalla cartella condivisi";

    /// Come FileProvider (res/xml/percorsi_condivisi.xml), che lancia IllegalArgumentException
    /// per tutto il resto: si condivide solo da `condivisi/` dentro la cache. I percorsi si
    /// confrontano canonici, come fa FileProvider con `File.getCanonicalPath()`: né «..» né un
    /// collegamento portano fuori.
    pub(crate) fn dentro_condivisi(cache: &Path, percorso: &Path) -> bool {
        match (canonico(&cache.join("condivisi")), canonico(percorso)) {
            (Some(radice), Some(file)) => file.starts_with(radice),
            _ => false,
        }
    }

    /// Il percorso canonico, come `File.getCanonicalPath()` di Java: si risolve la parte più
    /// lunga che esiste, collegamenti compresi, e il resto si attacca dopo. None se non si
    /// risolve niente.
    fn canonico(percorso: &Path) -> Option<PathBuf> {
        let mut esiste = percorso.to_path_buf();
        let mut resto = Vec::new();
        loop {
            if let Ok(mut c) = std::fs::canonicalize(&esiste) {
                c.extend(resto.iter().rev());
                return Some(c);
            }
            // `file_name` non è mai «.» né «..»: un percorso che finisce con «..» e non esiste
            // resta senza risposta, e il file non si condivide.
            resto.push(esiste.file_name()?.to_owned());
            esiste.pop();
        }
    }

    /// La finestra data alla tastiera: misura e selezione, salvo per una password, di cui non si
    /// dice nemmeno quanto è lunga.
    pub(crate) fn finestra(testo: &str, sel_start: i32, sel_end: i32, versione: i64, tipo: super::TipoTastiera) -> String {
        if tipo == super::TipoTastiera::Password {
            return format!("tastiera: finestra v{versione} inviata, campo della password");
        }
        format!("tastiera: finestra v{versione} inviata, {} unità, sel {sel_start}..{sel_end}", testo.encode_utf16().count())
    }

    /// Uno stato della tastiera senza selezione (il -1 di Java): il cursore va alla fine. Di una
    /// password non si dice nemmeno quanto è lunga (revisione della 0.3.0).
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    pub(crate) fn senza_selezione(versione: i64, sel: (i32, i32), fine: i32, tipo: super::TipoTastiera) -> String {
        if tipo == super::TipoTastiera::Password {
            return format!("tastiera: stato v{versione} senza selezione, cursore alla fine, campo della password");
        }
        format!("tastiera: stato v{versione} senza selezione {sel:?}, cursore alla fine ({fine} unità)")
    }

    pub(crate) fn tipo_tastiera(tipo: super::TipoTastiera) -> String {
        format!("tastiera: tipo {}", tipo.nome())
    }

    /// Tipo e misura del file: il nome (nel percorso) no, può essere personale.
    pub(crate) fn condividi_file(percorso: &Path, mime: &str) -> String {
        let misura = match std::fs::metadata(percorso) {
            Ok(m) => format!("{} byte", m.len()),
            Err(e) => format!("file non leggibile ({:?})", e.kind()),
        };
        format!("condividi file: {mime}, {misura}")
    }

    pub(crate) fn condividi_testo(testo: &str) -> String {
        format!("condividi testo: {} caratteri", testo.chars().count())
    }

    pub(crate) fn copia(testo: &str) -> String {
        format!("appunti: copiati {} caratteri", testo.chars().count())
    }

    pub(crate) fn incolla(testo: Option<&str>) -> String {
        match testo {
            Some(t) => format!("appunti: incollati {} caratteri", t.chars().count()),
            None => "appunti: niente da incollare".to_string(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    /// Il numero che manda Java per la foto: da 1 in su pronte (per più foto dalla galleria, tante
    /// quante il numero, al più 30, nella cartella della richiesta), 0 annullata, sotto zero errore.
    #[test]
    fn l_esito_della_foto_dal_numero_del_java() {
        let c = std::path::Path::new("/cache");
        // Una foto sola (fotocamera, immagine o PDF scelto) è il primo file della sua richiesta.
        assert_eq!(esito_dal_java(1, 1, FonteFoto::Fotocamera, 7, c), EsitoFoto::Pronta(file_della_pagina(c, 7, 1)));
        assert_eq!(esito_dal_java(1, 1, FonteFoto::Galleria, 8, c), EsitoFoto::Pronta(file_della_pagina(c, 8, 1)));
        assert_eq!(esito_dal_java(1, 1, FonteFoto::Pdf, 9, c), EsitoFoto::Pronta(std::path::PathBuf::from("/cache/foto/scelte-9/pagina-1")));
        assert_eq!(
            esito_dal_java(3, 5, FonteFoto::GalleriaPiu, 7, c),
            EsitoFoto::Pronte { file: vec![file_della_pagina(c, 7, 1), file_della_pagina(c, 7, 2), file_della_pagina(c, 7, 3)], scelte: 5 }
        );
        assert_eq!(file_della_pagina(c, 7, 2), std::path::Path::new("/cache/foto/scelte-7/pagina-2"));
        let EsitoFoto::Pronte { file, scelte } = esito_dal_java(500, 500, FonteFoto::GalleriaPiu, 7, c) else { panic!() };
        assert_eq!((file.len(), scelte), (MASSIMO_PAGINE_SCELTE, 500));
        // Le scelte non sono mai meno delle foto arrivate.
        assert!(matches!(esito_dal_java(2, 0, FonteFoto::GalleriaPiu, 7, c), EsitoFoto::Pronte { scelte: 2, .. }));
        assert_eq!(esito_dal_java(0, 0, FonteFoto::GalleriaPiu, 7, c), EsitoFoto::Annullata);
        assert_eq!(esito_dal_java(-1, 0, FonteFoto::Fotocamera, 7, c), EsitoFoto::Errore);
        for f in [FonteFoto::Fotocamera, FonteFoto::Galleria, FonteFoto::GalleriaPiu, FonteFoto::Pdf] {
            assert_eq!(FonteFoto::da_codice(f.codice()), Some(f));
        }
        assert_eq!((FonteFoto::GalleriaPiu.codice(), FonteFoto::da_codice(9)), (3, None));
    }

    /// All'avvio le foto rimaste nella cache da prima se ne vanno: una scelta della galleria mai
    /// letta, se Android ha tolto l'app mentre era aperta.
    #[test]
    fn all_avvio_le_foto_vecchie_se_ne_vanno() {
        let d = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/prove-platform/foto-vecchie");
        let _ = std::fs::remove_dir_all(&d);
        let pagina = file_della_pagina(&d, 3, 1);
        std::fs::create_dir_all(pagina.parent().unwrap()).unwrap();
        std::fs::write(&pagina, b"foglio").unwrap();
        std::fs::write(d.join("altro"), b"resta").unwrap();
        pulisci_le_foto(&d);
        assert!(!d.join("foto").exists() && d.join("altro").exists());
    }

    /// Il canale delle foto: ogni richiesta ha il suo numero (il primo dal seme, poi uno in più);
    /// l'esito di una richiesta superata non si tiene, e chi aspetta quella vecchia lo sa invece
    /// di prendere la foto di quella nuova (0.8.3, revisione del codice).
    #[test]
    fn una_richiesta_di_foto_superata_non_prende_la_foto_della_nuova() {
        let c = std::path::Path::new("/cache");
        let mut canale = CanaleFoto::default();
        assert_eq!(canale.in_corso(), 0);
        // Un esito prima di ogni richiesta non si tiene.
        assert!(!canale.ricevi(0, 1, 1) && !canale.ricevi(5, 1, 1));
        let immagine = canale.chiedi(FonteFoto::Galleria, || 41);
        assert_eq!((immagine, canale.in_corso()), (41, 41));
        assert_eq!(canale.prendi(immagine, c), None, "l'esito non è ancora arrivato");
        // Mentre la copia dell'immagine va avanti, la scansione chiede la fotocamera.
        let scansione = canale.chiedi(FonteFoto::Fotocamera, || panic!("il seme serve solo la prima volta"));
        assert_eq!(scansione, 42);
        // L'immagine arriva tardi: non si tiene, e chi la aspettava sa che è superata.
        assert!(!canale.ricevi(immagine, 1, 1));
        assert_eq!(canale.prendi(immagine, c), Some(EsitoFoto::Superata));
        assert_eq!(canale.prendi(scansione, c), None, "l'immagine non risponde alla scansione");
        // La foto della scansione arriva per lei, una volta sola.
        assert!(canale.ricevi(scansione, 1, 1));
        assert_eq!(canale.prendi(scansione, c), Some(EsitoFoto::Pronta(file_della_pagina(c, 42, 1))));
        assert_eq!(canale.prendi(scansione, c), None);
        // Più foto dalla galleria, con la loro fonte; il numero non diventa mai negativo.
        let galleria = canale.chiedi(FonteFoto::GalleriaPiu, || 1);
        assert!(canale.ricevi(galleria, 2, 4));
        assert!(matches!(canale.prendi(galleria, c), Some(EsitoFoto::Pronte { ref file, scelte: 4 }) if file.len() == 2));
        let mut alla_fine = CanaleFoto::default();
        let ultima = alla_fine.chiedi(FonteFoto::Fotocamera, || i32::MAX);
        assert_eq!((ultima, alla_fine.chiedi(FonteFoto::Fotocamera, || 1)), (i32::MAX, 1));
        assert_eq!(CanaleFoto::default().chiedi(FonteFoto::Pdf, || -5), 1);
    }

    /// «Apri con» apre solo URI content:// di altre app: un file:// mandato con un Intent
    /// esplicito, o un URI del nostro FileProvider, farebbe aprire i file privati dell'app (audit
    /// del 30/10, problema 4). Il perché va nella diagnosi, l'URI mai.
    #[test]
    fn apri_con_accetta_solo_i_content_di_altre_app() {
        for buono in [
            "content://com.whatsapp.provider.media/item/42",
            "content://media/external/file/1000",
            "CONTENT://com.android.providers.downloads.documents/document/7",
            "content://10@media/external/file/3",
        ] {
            assert_eq!(uri_da_aprire(buono), Ok(()), "{buono}");
        }
        for (cattivo, perche) in [
            ("file:///data/user/0/it.tascabile.app/files/ripristino/copia.docx", "file"),
            ("FILE:///sdcard/Download/x.pdf", "file"),
            ("content://it.tascabile.app.condivisione/condivisi/1/x.pdf", "nostro FileProvider"),
            ("content://0@IT.Tascabile.App.Condivisione/foto/x", "nostro FileProvider"),
            // Android decodifica l'autorità e la chiude anche alla «\» (revisione del 31/10, punto 5).
            ("content://it%2Etascabile%2Eapp%2Econdivisione/condivisi/1-0/x.pdf", "nostro FileProvider"),
            ("content://it.tascabile.app.condivision%65/foto/scelte-3/pagina-1", "nostro FileProvider"),
            ("content://it.tascabile.app.condivisione\\condivisi/1-0/x.pdf", "nostro FileProvider"),
            ("https://esempio.it/x.pdf", "altro schema"),
            ("/sdcard/Download/x.pdf", "senza schema"),
            ("content:", "senza autorità"),
            ("content:///x", "senza autorità"),
        ] {
            assert_eq!(uri_da_aprire(cattivo), Err(perche), "{cattivo}");
        }
    }

    /// All'avvio se ne vanno le copie condivise più vecchie di un'ora: l'ultimo documento
    /// condiviso non resta nella cache (audit del 30/10, problema 5); uno condiviso poco fa resta,
    /// perché l'app che lo riceve può star ancora leggendolo.
    #[test]
    fn all_avvio_le_copie_condivise_vecchie_se_ne_vanno() {
        let d = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/prove-platform/condivisi-vecchi");
        let _ = std::fs::remove_dir_all(&d);
        let adesso = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
        let vecchia = d.join("condivisi").join(format!("{}-0", adesso - 2 * 3_600_000));
        let nuova = d.join("condivisi").join(format!("{}-1", adesso - 60_000));
        for c in [&vecchia, &nuova] {
            std::fs::create_dir_all(c).unwrap();
            std::fs::write(c.join("Verbale.docx"), b"documento").unwrap();
        }
        // Una voce col nome di un altro genere conta per la sua data: appena scritta, resta.
        std::fs::write(d.join("condivisi").join("strano"), b"x").unwrap();
        pulisci_i_condivisi(&d, std::time::Duration::from_secs(3_600));
        assert!(!vecchia.exists(), "la copia vecchia è rimasta");
        assert!(nuova.join("Verbale.docx").exists() && d.join("condivisi/strano").exists());
        // Senza la cartella non succede niente.
        pulisci_i_condivisi(&d.join("non-c-e"), std::time::Duration::from_secs(3_600));
    }

    #[test]
    fn la_risposta_del_java_e_la_riga_della_diagnosi() {
        assert_eq!(risposta_dal_java(&[0, 200, 1, 2, 3]), RispostaRete::Ok(vec![1, 2, 3]));
        assert_eq!(risposta_dal_java(&[0x01, 0x94]), RispostaRete::Stato(404));
        assert_eq!(risposta_dal_java(&[0, 0]), RispostaRete::Collegamento);
        assert_eq!(risposta_dal_java(&[0, 1]), RispostaRete::TroppoLunga);
        assert_eq!(risposta_dal_java(&[]), RispostaRete::Collegamento);
        let lunga = [&[0u8, 200][..], &vec![7u8; RISPOSTA_MASSIMA + 1]].concat();
        assert_eq!(risposta_dal_java(&lunga), RispostaRete::TroppoLunga);
        assert_eq!(riga_rete(&RispostaRete::Ok(vec![0; 5]), Duration::from_millis(812)), "rete: 200 in 812 ms, 5 byte");
        assert_eq!(riga_rete(&RispostaRete::Stato(404), Duration::from_millis(90)), "rete: 404 in 90 ms");
    }

    /// L'ora vera dell'avvio nelle prove: 23/10/2026 alle 7 in punto, UTC.
    const AVVIO_PROVA: u64 = 1_792_738_800;

    /// `n` millesimi dopo l'avvio, col telefono sempre sveglio.
    fn ms(n: u64) -> Quando {
        let dopo = Duration::from_millis(n);
        Quando { dopo, ora: SystemTime::UNIX_EPOCH + Duration::from_secs(AVVIO_PROVA) + dopo }
    }

    #[test]
    fn si_aprono_solo_pagine_web_e_mail() {
        assert_eq!(genere_indirizzo("https://esempio.it/a?b=c"), Some("pagina web"));
        assert_eq!(genere_indirizzo(" HTTP://esempio.it "), Some("pagina web"));
        assert_eq!(genere_indirizzo("mailto:ufficio@esempio.it"), Some("posta"));
        for no in
            ["javascript:alert(1)", "file:///sdcard/x.pdf", "intent://x#Intent;end", "tel:123", "https:", "http:x", "esempio.it", "", "https://a b"]
        {
            assert_eq!(genere_indirizzo(no), None, "{no}");
        }
    }

    #[test]
    fn uno_stato_senza_selezione_non_dice_quanto_e_lunga_una_password() {
        let riga = righe::senza_selezione(7, (-1, -1), 12, TipoTastiera::Testo);
        assert_eq!(riga, "tastiera: stato v7 senza selezione (-1, -1), cursore alla fine (12 unità)");
        let riga = righe::senza_selezione(7, (-1, -1), 12, TipoTastiera::Password);
        assert!(!riga.contains("12") && riga.contains("password"), "{riga}");
    }

    #[test]
    fn la_riga_del_panico_ha_file_e_riga_e_mai_il_messaggio() {
        let dove = std::panic::Location::caller();
        let riga = riga_del_panico(Some("pdf-disegno"), Some(dove));
        assert_eq!(riga, format!("panico nel thread pdf-disegno in {}:{}", dove.file(), dove.line()));
        assert!(riga.contains("lib.rs:"), "{riga}");
        assert_eq!(riga_del_panico(None, None), "panico nel thread senza nome in un punto sconosciuto");
        // Il gancio scrive nella diagnosi senza aspettare; col registro bloccato da un altro la
        // riga resta da parte e si scrive con la prossima, o quando si legge il registro.
        diagnosi_dal_gancio("panico nel thread prova in a.rs:1");
        let preso = REGISTRO.lock().unwrap();
        diagnosi_dal_gancio("panico nel thread prova in b.rs:2");
        drop(preso);
        let d = diagnosi_testo();
        assert!(d.contains("] panico nel thread prova in a.rs:1\n"), "{d}");
        assert!(d.contains("] panico nel thread prova in b.rs:2\n"), "{d}");
    }

    #[test]
    fn il_tetto_dipende_dal_contenuto() {
        use std::io::Read;
        assert!(comincia_come_pdf(b"%PDF-1.7\n"));
        let mut dopo = vec![b' '; 1000];
        dopo.extend_from_slice(b"%PDF-1.4");
        assert!(comincia_come_pdf(&dopo));
        let mut troppo_dopo = vec![b' '; 1024];
        troppo_dopo.extend_from_slice(b"%PDF-1.4");
        assert!(!comincia_come_pdf(&troppo_dopo));
        assert!(!comincia_come_pdf(b"PK\x03\x04"));
        // Un PDF da 60 MB si legge; un file qualunque da 60 MB no; un PDF oltre 200 MB no.
        let pdf = |n: usize| std::io::Cursor::new([b"%PDF-1.7\n".as_slice(), &vec![0u8; n - 9]].concat());
        assert_eq!(leggi_con_tetto(pdf(60 << 20), "x").unwrap().len(), 60 << 20);
        let e = leggi_con_tetto(std::io::Cursor::new(vec![0u8; 60 << 20]), "x").unwrap_err();
        assert_eq!(e.genere, "oltre 50 MB");
        let e = leggi_con_tetto(std::io::repeat(b'%').take((LIMITE_PDF_MB << 20) + 1), "x");
        assert_eq!(e.unwrap_err().genere, "oltre 50 MB", "senza intestazione vale il tetto di Word");
        let lungo = std::io::Read::chain(&b"%PDF-1.7\n"[..], std::io::repeat(0).take(LIMITE_PDF_MB << 20));
        let e = leggi_con_tetto(lungo, "x").unwrap_err();
        assert_eq!(e.genere, "oltre 200 MB");
        assert!(e.messaggio.contains("il PDF supera i 200 MB"), "{}", e.messaggio);
        // Un file corto, più corto della testa, si legge tutto.
        assert_eq!(leggi_con_tetto(&b"ciao"[..], "x").unwrap(), b"ciao");
    }

    #[test]
    fn ora_vera_accanto_ai_secondi_dall_avvio() {
        // Tascabile aperta alle 7, telefono addormentato fino alle 12: dall'avvio risultano
        // 300 secondi, ma la riga deve dire che sono passate cinque ore.
        let mut r = Registro::nuovo(10);
        r.scrivi(ms(0), None, "avvio");
        let dopo_cinque_ore = Quando { dopo: Duration::from_secs(300), ora: SystemTime::UNIX_EPOCH + Duration::from_secs(1_792_756_800) };
        r.scrivi(dopo_cinque_ore, None, "apri con: errore di lettura");
        assert_eq!(r.testo(), "[   0.000 23/10 07:00:00] avvio\n[ 300.000 23/10 12:00:00] apri con: errore di lettura\n");
    }

    #[test]
    fn riga_con_i_secondi_dall_avvio() {
        let mut r = Registro::nuovo(10);
        r.scrivi(ms(12_345), None, "tastiera: stato v7, 23 unità, sel 5..5, applicato");
        assert_eq!(r.testo(), "[  12.345 23/10 07:00:12] tastiera: stato v7, 23 unità, sel 5..5, applicato\n");
    }

    #[test]
    fn le_righe_di_un_thread_si_ascoltano_a_parte() {
        // Prima dell'ascolto niente; poi le righe di questo thread, anche le fisse, e non quelle
        // degli altri thread.
        diagnosi("prima dell'ascolto");
        ascolta_diagnosi();
        diagnosi("una riga");
        diagnosi_fissa(Fissa::ApriCon, "una riga fissa");
        std::thread::spawn(|| diagnosi("da un altro thread")).join().unwrap();
        diagnosi("ultima riga");
        // Poche righe: le altre prove leggono il registro comune, e tante lo farebbero girare.
        assert_eq!(righe_ascoltate(), "una riga\nuna riga fissa\nultima riga\n");
        // Un ascolto nuovo ricomincia da capo.
        ascolta_diagnosi();
        assert_eq!(righe_ascoltate(), "");
    }

    #[test]
    fn tiene_le_ultime_300_righe_in_ordine() {
        let mut r = Registro::nuovo(RIGHE_DIAGNOSI);
        for i in 0..350 {
            r.scrivi(ms(i), None, &format!("evento {i}"));
        }
        let testo = r.testo();
        let righe: Vec<&str> = testo.lines().collect();
        assert_eq!(righe.len(), RIGHE_DIAGNOSI);
        // Escono i 50 più vecchi; gli altri restano nel loro ordine.
        for (k, riga) in righe.iter().enumerate() {
            assert!(riga.ends_with(&format!("] evento {}", k + 50)), "riga {k}: {riga}");
        }
    }

    #[test]
    fn un_evento_e_sempre_una_riga_corta() {
        let mut r = Registro::nuovo(10);
        r.scrivi(ms(0), None, "errore:\nseconda riga\r\tfine");
        r.scrivi(ms(0), None, &"x".repeat(1000));
        let testo = r.testo();
        let righe: Vec<&str> = testo.lines().collect();
        assert_eq!(righe.len(), 2, "{testo}");
        assert_eq!(righe[0], "[   0.000 23/10 07:00:00] errore: seconda riga  fine");
        // «[   0.000 23/10 07:00:00] », il tetto di caratteri e «…»
        assert_eq!(righe[1].chars().count(), 26 + CARATTERI_PER_EVENTO + 1);
        assert!(righe[1].ends_with('…'));
    }

    #[test]
    fn righe_fisse_in_testa_anche_dopo_il_giro() {
        let mut r = Registro::nuovo(RIGHE_DIAGNOSI);
        r.scrivi_fissa(ms(0), Fissa::Avvio, "avvio: Tascabile v9.9.9 su prova");
        r.scrivi(ms(2), None, "apri con: richiesta, tipo text/plain");
        r.scrivi_fissa(ms(3), Fissa::PrimoDisegno, "avvio: primo disegno dopo 350 ms, schermata Home");
        r.scrivi_fissa(ms(5), Fissa::ApriCon, "apri con: 4321 byte, tipo docx");
        r.scrivi_fissa(ms(7), Fissa::AccessoFile, "accesso ai file all'avvio: concesso");
        // Ogni lettera battuta lascia almeno una riga: dopo 400 le prime righe escono dal giro.
        for i in 0..400 {
            r.scrivi(ms(100 + i), None, &format!("tastiera: stato v{i}"));
        }
        // Un «Apri con» nuovo prende il posto del vecchio.
        r.scrivi_fissa(ms(600), Fissa::ApriCon, "apri con: 1234 byte, tipo odt");
        let testo = r.testo();
        let (testa, eventi) = testo.split_once("Ultimi eventi, dal più vecchio:\n").expect("manca la testa del registro");
        assert_eq!(
            testa,
            "In evidenza:\n[   0.000 23/10 07:00:00] avvio: Tascabile v9.9.9 su prova\n[   0.003 23/10 07:00:00] avvio: primo disegno dopo 350 ms, schermata Home\n[   0.007 23/10 07:00:00] accesso ai file all'avvio: concesso\n[   0.600 23/10 07:00:00] apri con: 1234 byte, tipo odt\n"
        );
        // Fra gli ultimi eventi le righe fisse stanno al loro posto, finché il giro le tiene.
        assert_eq!(eventi.lines().count(), RIGHE_DIAGNOSI);
        assert!(eventi.ends_with("] apri con: 1234 byte, tipo odt\n"), "{eventi}");
        assert!(!eventi.contains("avvio:") && !eventi.contains("4321 byte"), "{eventi}");
    }

    #[test]
    fn senza_righe_fisse_niente_testa() {
        let mut r = Registro::nuovo(10);
        r.scrivi(ms(1), None, "tocco");
        assert_eq!(r.testo(), "[   0.001 23/10 07:00:00] tocco\n");
    }

    #[test]
    fn raffica_lascia_solo_l_ultima_riga_di_fila() {
        let mut r = Registro::nuovo(10);
        r.scrivi(ms(1000), None, "tocco");
        r.scrivi(ms(2000), Some("insets"), "insets 1");
        r.scrivi(ms(3000), Some("insets"), "insets 2");
        r.scrivi(ms(3500), Some("altro"), "altro 1");
        r.scrivi(ms(4000), None, "finestra");
        r.scrivi(ms(5000), Some("insets"), "insets 3");
        assert_eq!(
            r.testo(),
            "[   1.000 23/10 07:00:01] tocco\n[   3.000 23/10 07:00:03] insets 2\n[   3.500 23/10 07:00:03] altro 1\n[   4.000 23/10 07:00:04] finestra\n[   5.000 23/10 07:00:05] insets 3\n"
        );
    }

    #[test]
    fn raffica_non_supera_il_tetto() {
        let mut r = Registro::nuovo(3);
        for i in 0..5 {
            r.scrivi(ms(i), None, &format!("evento {i}"));
            r.scrivi(ms(i), Some("insets"), &format!("insets {i}"));
        }
        assert_eq!(r.testo().lines().count(), 3);
        assert!(r.testo().ends_with("insets 4\n"));
    }

    /// Secondi dall'avvio di una riga del registro: «[  12.345 23/10 07:00:12] …» → 12,345.
    fn secondi(riga: &str) -> f64 {
        let (tempi, _) = riga.strip_prefix('[').and_then(|r| r.split_once(']')).expect("riga senza tempo");
        tempi.split_whitespace().next().expect("riga senza tempo").parse().expect("tempo non numerico")
    }

    #[test]
    fn ora_utc_con_giorno_e_mese() {
        let a = |secondi: u64| ora_utc(SystemTime::UNIX_EPOCH + Duration::from_secs(secondi));
        assert_eq!(a(0), "01/01 00:00:00");
        assert_eq!(a(AVVIO_PROVA + 12), "23/10 07:00:12");
        assert_eq!(a(1_798_761_599), "31/12 23:59:59");
        // Bisestili: il 2028 e il 2000 sì, il 2100 no.
        assert_eq!(a(1_835_481_599), "29/02 23:59:59");
        assert_eq!(a(951_868_800 - 1), "29/02 23:59:59");
        assert_eq!(a(951_868_800), "01/03 00:00:00");
        assert_eq!(a(4_107_542_400 - 1), "28/02 23:59:59");
        assert_eq!(a(4_107_542_400), "01/03 00:00:00");
        assert_eq!(data_civile(4_107_542_400 / 86_400), (2100, 3, 1));
        // Un orologio prima del 1970: niente panico.
        assert_eq!(ora_utc(SystemTime::UNIX_EPOCH - Duration::from_secs(1)), "??/?? ??:??:??");
    }

    #[test]
    fn registro_comune_in_ordine_anche_fra_thread() {
        // Il registro è uno per processo e gli altri test ci scrivono in parallelo: qui si guardano
        // solo le righe con il proprio segno, più la regola che vale per tutte (tempi in ordine).
        let fili: Vec<_> = (0..4)
            .map(|f| {
                std::thread::spawn(move || {
                    for i in 0..50 {
                        diagnosi(format!("prova-fili {f}: {i}"));
                    }
                })
            })
            .collect();
        for filo in fili {
            filo.join().unwrap();
        }
        let testo = diagnosi_testo();
        // Legenda e righe fisse in testa: qui contano gli ultimi eventi.
        let eventi = testo.split_once("Ultimi eventi, dal più vecchio:\n").map_or(testo.as_str(), |(_, eventi)| eventi);
        let righe: Vec<&str> = eventi.lines().filter(|r| r.starts_with('[')).collect();
        assert!(righe.len() <= RIGHE_DIAGNOSI);
        let tempi: Vec<f64> = righe.iter().map(|r| secondi(r)).collect();
        assert!(tempi.windows(2).all(|t| t[0] <= t[1]), "tempi all'indietro:\n{testo}");
        for f in 0..4 {
            let segno = format!("prova-fili {f}: ");
            let mie: Vec<u32> = righe.iter().filter_map(|r| r.split_once(segno.as_str())).map(|(_, i)| i.parse().unwrap()).collect();
            assert!(mie.windows(2).all(|i| i[0] < i[1]), "filo {f} fuori ordine:\n{testo}");
            assert_eq!(mie.last(), Some(&49), "manca l'ultimo evento del filo {f}:\n{testo}");
        }
    }

    /// Manda a `riga_insets` una serie di (tastiera in px, tastiera visibile, da
    /// onApplyWindowInsets) con le barre fisse, come MainActivity, e restituisce le righe del
    /// registro senza il tempo.
    fn righe_degli_insets(serie: &[(i32, bool, bool)]) -> Vec<String> {
        let mut r = Registro::nuovo(RIGHE_DIAGNOSI);
        let mut ultimi = None;
        for (k, &(tastiera, visibile, finale)) in serie.iter().enumerate() {
            let nuovi = Insets { sinistra: 0, sopra: 100, destra: 0, sotto: 60, tastiera, visibile };
            if let Some((tipo, riga)) = riga_insets(&mut ultimi, nuovi, finale) {
                r.scrivi(ms(16 * k as u64), Some(tipo), &riga);
            }
        }
        r.testo().lines().map(|riga| riga.split_once("] ").expect("riga senza tempo").1.to_string()).collect()
    }

    fn riga_insets_attesa(tastiera: i32, stato: &str) -> String {
        format!("insets: barre sopra 100 px, sotto 60 px, ai lati 0 e 0 px; tastiera {tastiera} px, {stato}")
    }

    #[test]
    fn insets_una_riga_per_apertura_e_una_per_chiusura() {
        // La sequenza di AOSP (mirror aosp-mirror/platform_frameworks_base, commit 1cdfff55):
        // prima di ogni animazione onApplyWindowInsets manda lo stato di fine animazione
        // (InsetsController.startAnimation, documentazione di WindowInsetsAnimation.Callback.onPrepare),
        // poi arrivano i frame di onProgress, il primo a frazione 0. Nei frame la tastiera è
        // visibile se `fraction == 0 ? tipo != SHOW : fraction < 1 || shownOnFinish`
        // (InsetsAnimationControlImpl.updateLeashesForSide): in apertura il primo frame la dà
        // chiusa, in chiusura tutti meno l'ultimo la danno visibile.
        let serie = [
            // avvio
            (0, false, true),
            // apertura
            (900, true, true),
            (0, false, false),
            (150, true, false),
            (400, true, false),
            (700, true, false),
            (880, true, false),
            (900, true, false),
            // chiusura
            (0, false, true),
            (900, true, false),
            (600, true, false),
            (300, true, false),
            (50, true, false),
            (0, false, false),
        ];
        assert_eq!(
            righe_degli_insets(&serie),
            [riga_insets_attesa(0, "chiusa"), riga_insets_attesa(900, "visibile"), riga_insets_attesa(0, "chiusa")]
        );
    }

    #[test]
    fn insets_chiusura_che_si_sa_solo_alla_fine() {
        // Un'animazione che parte senza uno stato finale nuovo (caso possibile, non visto sul
        // telefono: una chiusura guidata dal dito, con la disposizione tenuta a tastiera aperta
        // finché il gesto non finisce), con onApplyWindowInsets che arriva solo alla fine e con
        // gli stessi valori dell'ultimo frame: la chiusura ha comunque la sua riga.
        let serie = [(900, true, true), (700, true, false), (300, true, false), (0, false, false), (0, false, true)];
        assert_eq!(righe_degli_insets(&serie), [riga_insets_attesa(900, "visibile"), riga_insets_attesa(0, "chiusa")]);
    }

    #[test]
    fn il_registro_condiviso_spiega_i_tempi() {
        diagnosi("prova-legenda");
        let d = diagnosi_testo();
        assert!(d.starts_with("Tempi: secondi dall'avvio (si fermano quando il telefono dorme) e ora UTC"), "{d}");
        let riga = d.lines().find(|r| r.ends_with("] prova-legenda")).expect("manca la riga");
        // «[  12.345 23/10 07:00:12] prova-legenda»: secondi, giorno e mese, ora.
        let (tempi, _) = riga[1..].split_once(']').unwrap();
        let pezzi: Vec<&str> = tempi.split_whitespace().collect();
        assert_eq!(pezzi.len(), 3, "{riga}");
        assert_eq!((pezzi[1].len(), pezzi[2].len()), (5, 8), "{riga}");
    }

    #[test]
    fn raffica_nel_registro_comune() {
        diagnosi_raffica("prova-raffica", "prova-raffica: ultima");
        assert!(diagnosi_testo().contains("] prova-raffica: ultima\n"));
    }
}
