//! Salvare, salvare una copia, esportare in PDF e preparare la condivisione; il nome dei file
//! nuovi dal testo come si legge.
//!
//! Il file dell'utente si sostituisce solo con «Salva» su un formato che il motore scrive senza
//! perdite (`save_path`, già atomico). Ogni file nuovo (copia, «Salva come .docx», PDF) prende un
//! nome libero e un ultimo passo che non sostituisce mai: il nome si prenota creandolo vuoto in
//! esclusiva, il contenuto va in un temporaneo accanto e il rename lo mette al posto di quello
//! vuoto, che è nostro.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use tascabile_files::{NOME_DI_RIPIEGO, cartella_tascabile, nome_libero, nome_pulito};
use wordcraft_engine::Session;
use wordcraft_engine::doc::{CharProps, Document, Paragraph, para_block};

/// Da dove viene il documento dell'Editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origine {
    /// Un file della memoria del telefono.
    File(PathBuf),
    /// Arrivato con «Apri con»: l'URI e il nome (quello vero, se l'app che lo manda lo dice).
    Uri { uri: String, nome: String },
    /// Scritto qui, mai salvato.
    Nuovo,
    /// Riaperto dal recupero dopo un'interruzione, col nome che aveva (vuoto se era nuovo): non
    /// si salva sul file di prima senza chiederlo, come nel recupero del motore, ma in una copia.
    Recuperato { nome: String },
}

/// I formati che il motore scrive senza perdere niente (`ui-egui/src/lib.rs`, `keeps_everything`,
/// senza json e xml, che non sono documenti dell'utente). Nemmeno odt e rtf: il motore ne salta
/// commenti, note, intestazioni e piè di pagina, e il file dell'utente uscirebbe senza (analisi
/// del 27/10, critico 4); come per il .doc, si salva un .docx accanto.
pub const SENZA_PERDITE: &[&str] = &["docx", "docm", "dotx", "dotm"];

/// L'estensione di un nome, in minuscolo.
pub fn estensione(nome: &str) -> Option<String> {
    let (base, ext) = nome.rsplit_once('.')?;
    (!base.is_empty() && !ext.is_empty()).then(|| ext.to_ascii_lowercase())
}

/// Il motore riscrive questo formato senza perdite.
pub fn senza_perdite(nome: &str) -> bool {
    estensione(nome).is_some_and(|e| SENZA_PERDITE.contains(&e.as_str()))
}

/// Il tipo MIME per condividere un file con questa estensione.
pub fn mime(ext: &str) -> &'static str {
    match ext {
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "docm" => "application/vnd.ms-word.document.macroEnabled.12",
        "dotx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.template",
        "dotm" => "application/vnd.ms-word.template.macroEnabled.12",
        "odt" => "application/vnd.oasis.opendocument.text",
        "rtf" => "application/rtf",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

/// Il testo di `p` come si legge: senza le run che la vista senza revisioni lascia fuori, cioè
/// quelle cancellate con le revisioni e quelle nascoste, anche da uno stile, e con i caratteri dei
/// font simbolo (Symbol, Wingdings) resi col loro carattere Unicode, come fa `shown_text`: una µ
/// in Symbol non diventa «m» né un carattere ad uso privato. È il testo da dare a `nome_pulito`:
/// mai `Paragraph::text`, `plain_text()` o `shown_text()`, e mai `final_text()` da solo, che
/// tiene il testo nascosto.
pub fn come_si_legge(doc: &Document, p: &Paragraph) -> String {
    use wordcraft_engine::doc::symbol_fonts::symbol_char;
    let stile = p.props.style.as_deref();
    // Per ogni run: dove sta, se resta fuori e il suo font risolto.
    let run: Vec<(std::ops::Range<usize>, bool, String)> = p
        .run_ranges()
        .map(|(intervallo, c)| {
            let c = doc.styles.resolve_char(stile, c);
            (intervallo, c.del.is_some() || c.hidden, c.font)
        })
        .collect();
    let mut out = String::with_capacity(p.text.len());
    let (mut j, mut k) = (0, 0);
    for (i, ch) in p.text.char_indices() {
        while run.get(j).is_some_and(|r| r.0.end <= i) {
            j += 1;
        }
        let r = run.get(j).filter(|r| r.0.start <= i);
        let fuori = r.is_some_and(|r| r.1);
        if ch == wordcraft_engine::doc::para::OBJ {
            if !fuori && let Some(o) = p.objects.get(k) {
                out.push_str(o.plain_text());
            }
            k += 1;
        } else if !fuori {
            out.push(r.and_then(|r| symbol_char(&r.2, ch)).unwrap_or(ch));
        }
    }
    out
}

/// Il nome dal testo: il primo titolo che dice qualcosa, altrimenti il primo paragrafo che dice
/// qualcosa, letti come si leggono. None se nessun paragrafo dice niente.
pub fn nome_dal_testo(doc: &Document) -> Option<String> {
    let paragrafi: Vec<&Paragraph> = doc.body.iter().filter_map(|b| b.as_para()).collect();
    let nome = |p: &&Paragraph| {
        let n = nome_pulito(&come_si_legge(doc, p));
        (n != NOME_DI_RIPIEGO).then_some(n)
    };
    let titolo = |p: &&&Paragraph| doc.styles.resolve_para(&p.props).outline_level.is_some();
    paragrafi.iter().filter(titolo).find_map(nome).or_else(|| paragrafi.iter().find_map(nome))
}

/// «Documento» con data e ora locali, per un documento nuovo che non dice niente.
pub fn nome_di_ripiego_adesso() -> String {
    let adesso = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    let (anno, mese, giorno, ora, minuti) = ora_locale(adesso);
    format!("{NOME_DI_RIPIEGO} {giorno:02}-{mese:02}-{anno} {ora:02}.{minuti:02}")
}

/// Anno, mese, giorno, ora e minuti locali di un istante (secondi UTC), con `localtime_r`, che su
/// Android legge il fuso del telefono.
pub fn ora_locale(secondi: i64) -> (i32, u32, u32, u32, u32) {
    let t: libc::time_t = secondi as libc::time_t;
    // SAFETY: `tm` è tutto zeri, un valore valido; localtime_r scrive solo lì e non tiene i
    // puntatori.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let ok = unsafe { !libc::localtime_r(&t, &mut tm).is_null() };
    if !ok {
        return (1970, 1, 1, 0, 0);
    }
    (tm.tm_year + 1900, (tm.tm_mon + 1) as u32, tm.tm_mday as u32, tm.tm_hour as u32, tm.tm_min as u32)
}

/// La base del nome di un file nuovo fatto da questo documento: il nome del file o quello vero
/// arrivato con «Apri con», senza estensione; per un documento nuovo (o un «Apri con» senza il
/// nome vero, `documento.docx`) il nome dal testo, altrimenti `ripiego`.
pub fn base_del_nome(origine: &Origine, doc: &Document, ripiego: &str) -> String {
    let senza_ext = |n: &str| n.rsplit_once('.').map_or(n, |(b, _)| b).to_string();
    let dal_testo = || nome_dal_testo(doc).unwrap_or_else(|| ripiego.to_string());
    match origine {
        Origine::File(p) => p.file_stem().map(|s| s.to_string_lossy().into_owned()).filter(|s| !s.is_empty()).unwrap_or_else(dal_testo),
        Origine::Uri { nome, .. } if !nome_generico(nome) => senza_ext(nome),
        Origine::Recuperato { nome } if !nome.is_empty() => senza_ext(nome),
        Origine::Uri { .. } | Origine::Recuperato { .. } | Origine::Nuovo => dal_testo(),
    }
}

/// Un nome che non viene dal file ma dal tipo MIME (`documento::nome_da_mime`).
fn nome_generico(nome: &str) -> bool {
    crate::documento::NOMI_DA_MIME.contains(&nome)
}

/// Il formato in cui scrivono «Salva», «Salva una copia» e «Condividi»: quello del documento se il
/// motore lo scrive senza perdite, altrimenti docx.
pub fn formato_della_copia(origine: &Origine) -> String {
    let nome = match origine {
        Origine::File(p) => p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        Origine::Uri { nome, .. } | Origine::Recuperato { nome } => nome.clone(),
        Origine::Nuovo => String::new(),
    };
    if senza_perdite(&nome) { estensione(&nome).unwrap_or_else(|| "docx".into()) } else { "docx".to_string() }
}

/// Come è stato salvato.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Come {
    /// Sul file dell'utente.
    SulFile,
    /// Un formato che si perde qualcosa: un .docx nuovo nella stessa cartella.
    ComeDocx,
    /// Una copia in Documents/Tascabile.
    Copia,
    /// Un PDF in Documents/Tascabile.
    Pdf,
    /// Un file da condividere in `condivisi/` della cache.
    Condivisione,
}

/// Un file scritto: dove, quanti byte e di che tipo (l'estensione, per la diagnosi).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Salvato {
    pub percorso: PathBuf,
    pub byte: u64,
    pub tipo: String,
    pub come: Come,
}

/// Un salvataggio non riuscito. Il genere va nella diagnosi (il genere dell'errore di I/O, o
/// «errore del motore»), il messaggio sullo schermo: può avere il nome del file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErroreSalva {
    pub genere: String,
    pub messaggio: String,
}

impl std::fmt::Display for ErroreSalva {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.messaggio)
    }
}

fn da_io(e: io::Error) -> ErroreSalva {
    ErroreSalva { genere: format!("{:?}", e.kind()), messaggio: e.to_string() }
}

fn dal_motore(e: String) -> ErroreSalva {
    ErroreSalva { genere: "errore del motore".to_string(), messaggio: e }
}

/// I byte del documento nel formato di `ext`; un panico del motore diventa un errore.
fn byte_del_documento(doc: &Document, ext: &str) -> Result<Vec<u8>, ErroreSalva> {
    let nome = format!("documento.{ext}");
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| wordcraft_engine::io::save_bytes(&nome, doc))) {
        Ok(r) => r.map_err(dal_motore),
        Err(_) => {
            Err(ErroreSalva { genere: "panico del motore".to_string(), messaggio: "Il motore si è fermato con un errore interno.".to_string() })
        }
    }
}

/// Un `.docx` coi paragrafi di `pagine`, in Normale su un A4 italiano: ogni pagina dopo la prima
/// comincia su una pagina nuova.
pub fn docx_dai_paragrafi(pagine: &[Vec<String>]) -> Result<Vec<u8>, ErroreSalva> {
    let mut doc = crate::documento::nuovo().doc;
    doc.body = pagine
        .iter()
        .enumerate()
        .flat_map(|(i, paragrafi)| {
            paragrafi.iter().enumerate().map(move |(j, testo)| {
                let mut p = Paragraph::with_text(testo, CharProps::default());
                if i > 0 && j == 0 {
                    p.props.page_break_before = Some(true);
                }
                para_block(p)
            })
        })
        .collect();
    doc.ensure_nonempty();
    byte_del_documento(&doc, "docx")
}

static TEMPORANEI: AtomicU64 = AtomicU64::new(0);

/// Un temporaneo nascosto e corto accanto a `percorso`.
fn temporaneo(cartella: &Path) -> PathBuf {
    let n = TEMPORANEI.fetch_add(1, Ordering::Relaxed);
    cartella.join(format!(".salvataggio-{}-{n}.tmp", std::process::id()))
}

/// Scrive `byte` in `percorso` passando da un temporaneo accanto, sincronizzato, e un rename. Il
/// rename sostituisce: chi chiama sa che il posto è suo.
pub fn scrivi_atomico(percorso: &Path, byte: &[u8]) -> io::Result<()> {
    let cartella = percorso.parent().unwrap_or(Path::new("."));
    let tmp = temporaneo(cartella);
    let esito = (|| {
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        f.write_all(byte)?;
        f.sync_all()?;
        std::fs::rename(&tmp, percorso)
    })();
    if esito.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    esito
}

/// Scrive un file nuovo in `cartella` con un nome libero da `base` ed `ext`, senza mai sostituire
/// un file che c'è: il nome si prenota con `create_new`, poi `scrivi_atomico` ci mette il
/// contenuto. Se intanto il nome è stato preso si prova il prossimo.
pub fn scrivi_nuovo(cartella: &Path, base: &str, ext: &str, byte: &[u8]) -> io::Result<PathBuf> {
    let mut ultimo = io::Error::from(io::ErrorKind::AlreadyExists);
    for _ in 0..20 {
        let percorso = nome_libero(cartella, base, ext);
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&percorso) {
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                ultimo = e;
                continue;
            }
            Err(e) => return Err(e),
        }
        return match scrivi_atomico(&percorso, byte) {
            Ok(()) => Ok(percorso),
            Err(e) => {
                let _ = std::fs::remove_file(&percorso);
                Err(e)
            }
        };
    }
    Err(ultimo)
}

/// La cartella Documents/Tascabile sotto `radice`, creata se manca.
fn documenti_tascabile(radice: &Path) -> Result<PathBuf, ErroreSalva> {
    let c = cartella_tascabile(radice);
    std::fs::create_dir_all(&c).map_err(da_io)?;
    Ok(c)
}

fn salvato(percorso: PathBuf, byte: usize, ext: &str, come: Come) -> Salvato {
    Salvato { percorso, byte: byte as u64, tipo: ext.to_string(), come }
}

/// I metadati del salvataggio come li mette Word (`stamp_save`: data, revisione, zoom), tranne
/// «ultima modifica di», che resta com'era: il nome delle Impostazioni va solo nei commenti e
/// nelle revisioni, non nelle proprietà del file.
fn timbra(s: &mut Session) {
    let di_prima = std::mem::take(&mut s.doc.core.last_modified_by);
    s.stamp_save();
    s.doc.core.last_modified_by = di_prima;
}

/// Dopo un salvataggio riuscito del documento (i metadati come Word li ha già messi
/// `stamp_save`): non ha più modifiche da salvare, e il recupero del motore toglie la sua copia.
fn segna_salvato(s: &mut Session) {
    s.dirty = false;
}

/// «Salva». Un file in un formato senza perdite si sostituisce (`save_path`, atomico); un formato
/// che perde qualcosa (.doc, .txt, .md…) diventa un .docx nuovo nella stessa cartella; un
/// documento arrivato con «Apri con» o nuovo diventa una copia in Documents/Tascabile. Dopo il
/// .docx nuovo e la copia l'origine è quel file. `radice` è la radice dei documenti, `ripiego` la
/// base del nome se il testo non dice niente.
pub fn salva(s: &mut Session, origine: &mut Origine, radice: &Path, ripiego: &str) -> Result<Salvato, ErroreSalva> {
    let Origine::File(percorso) = origine.clone() else {
        return salva_copia(s, origine, radice, ripiego);
    };
    let nome = percorso.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if senza_perdite(&nome) {
        timbra(s);
        let ext = estensione(&nome).unwrap_or_default();
        let esito = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| wordcraft_engine::io::save_path(&percorso, &s.doc)));
        match esito {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(dal_motore(e)),
            Err(_) => {
                return Err(ErroreSalva { genere: "panico del motore".into(), messaggio: "Il motore si è fermato con un errore interno.".into() });
            }
        }
        segna_salvato(s);
        let byte = std::fs::metadata(&percorso).map_or(0, |m| m.len());
        return Ok(Salvato { percorso, byte, tipo: ext, come: Come::SulFile });
    }
    let cartella = percorso.parent().map(Path::to_path_buf).unwrap_or_default();
    let base = base_del_nome(origine, &s.doc, ripiego);
    timbra(s);
    let byte = byte_del_documento(&s.doc, "docx")?;
    let nuovo = scrivi_nuovo(&cartella, &base, "docx", &byte).map_err(da_io)?;
    segna_salvato(s);
    *origine = Origine::File(nuovo.clone());
    Ok(salvato(nuovo, byte.len(), "docx", Come::ComeDocx))
}

/// «Salva una copia» in Documents/Tascabile (creata se manca), nello stesso formato se il motore
/// lo scrive senza perdite, altrimenti in .docx; da lì l'origine è la copia.
pub fn salva_copia(s: &mut Session, origine: &mut Origine, radice: &Path, ripiego: &str) -> Result<Salvato, ErroreSalva> {
    let ext = formato_della_copia(origine);
    let base = base_del_nome(origine, &s.doc, ripiego);
    let cartella = documenti_tascabile(radice)?;
    timbra(s);
    let byte = byte_del_documento(&s.doc, &ext)?;
    let copia = scrivi_nuovo(&cartella, &base, &ext, &byte).map_err(da_io)?;
    segna_salvato(s);
    *origine = Origine::File(copia.clone());
    Ok(salvato(copia, byte.len(), &ext, Come::Copia))
}

/// «Esporta PDF» in Documents/Tascabile (creata se manca). Il documento resta com'è.
pub fn esporta_pdf(s: &Session, origine: &Origine, radice: &Path, ripiego: &str) -> Result<Salvato, ErroreSalva> {
    let base = base_del_nome(origine, &s.doc, ripiego);
    let cartella = documenti_tascabile(radice)?;
    let byte = byte_del_documento(&s.doc, "pdf")?;
    let pdf = scrivi_nuovo(&cartella, &base, "pdf", &byte).map_err(da_io)?;
    Ok(salvato(pdf, byte.len(), "pdf", Come::Pdf))
}

/// Numero della prossima sottocartella di `condivisi/` in questo processo.
static CONDIVISIONI: AtomicU64 = AtomicU64::new(0);

/// La cartella di una condivisione nuova, `cartella_cache()/condivisi/<ora in ms>-<numero>/`.
/// Ogni condivisione ha la sua: il permesso di lettura che Android dà a chi riceve vale per l'URI,
/// e un URI che indicasse un altro documento con lo stesso nome lo farebbe leggere a chi non l'ha
/// ricevuto. Quelle di prima si tolgono: chi le ha ricevute ne ha già fatto una copia.
fn condivisi(cache: &Path) -> Result<PathBuf, ErroreSalva> {
    let c = cache.join("condivisi");
    std::fs::create_dir_all(&c).map_err(da_io)?;
    if let Ok(voci) = std::fs::read_dir(&c) {
        for v in voci.flatten() {
            let p = v.path();
            let _ = if p.is_dir() { std::fs::remove_dir_all(&p) } else { std::fs::remove_file(&p) };
        }
    }
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
    let dentro = c.join(format!("{ms}-{}", CONDIVISIONI.fetch_add(1, Ordering::Relaxed)));
    std::fs::create_dir_all(&dentro).map_err(da_io)?;
    Ok(dentro)
}

/// «Condividi»: il documento com'è adesso, anche con le modifiche non salvate, in
/// `cartella_cache()/condivisi/`, nel suo formato se senza perdite, altrimenti in .docx.
pub fn prepara_condivisione(s: &Session, origine: &Origine, cache: &Path, ripiego: &str) -> Result<Salvato, ErroreSalva> {
    let ext = formato_della_copia(origine);
    let base = base_del_nome(origine, &s.doc, ripiego);
    let cartella = condivisi(cache)?;
    let byte = byte_del_documento(&s.doc, &ext)?;
    let file = nome_libero(&cartella, &base, &ext);
    scrivi_atomico(&file, &byte).map_err(da_io)?;
    Ok(salvato(file, byte.len(), &ext, Come::Condivisione))
}

/// Un file già scritto (il PDF esportato, un documento dei recenti) copiato in `condivisi/` per
/// condividerlo.
pub fn copia_in_condivisi(file: &Path, cache: &Path) -> Result<Salvato, ErroreSalva> {
    let byte = std::fs::read(file).map_err(da_io)?;
    let nome = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "documento.pdf".into());
    byte_in_condivisi(&nome, &byte, cache)
}

/// I byte di un documento già letti (uno dei recenti arrivato da un'altra app) in `condivisi/`, col
/// nome `nome`: solo l'ultimo pezzo, che non esca dalla sottocartella.
pub fn byte_in_condivisi(nome: &str, byte: &[u8], cache: &Path) -> Result<Salvato, ErroreSalva> {
    let nome = nome.rsplit(['/', '\\']).next().map(str::trim).filter(|n| !n.is_empty() && *n != "." && *n != "..").unwrap_or("documento");
    let cartella = condivisi(cache)?;
    let ext = estensione(nome).unwrap_or_default();
    let dentro = cartella.join(nome);
    scrivi_atomico(&dentro, byte).map_err(da_io)?;
    Ok(salvato(dentro, byte.len(), &ext, Come::Condivisione))
}

#[cfg(test)]
mod test {
    use super::*;
    use serde_json::json;

    #[test]
    fn un_docx_dai_paragrafi_con_una_pagina_per_pagina() {
        let pagine = vec![vec!["Primo paragrafo, con le accentate: più città.".to_string(), "Secondo".to_string()], vec!["Pagina due".to_string()]];
        let docx = docx_dai_paragrafi(&pagine).expect("il docx");
        let s = crate::documento::apri_byte("riconosciuto.docx", &docx).expect("si riapre");
        let paragrafi: Vec<&Paragraph> =
            s.doc.body.iter().filter_map(|b| if let wordcraft_engine::doc::Block::Para(p) = &**b { Some(p) } else { None }).collect();
        let testi: Vec<String> = paragrafi.iter().map(|p| p.plain_text()).collect();
        assert_eq!(testi, ["Primo paragrafo, con le accentate: più città.", "Secondo", "Pagina due"]);
        let salti: Vec<bool> = paragrafi.iter().map(|p| p.props.page_break_before == Some(true)).collect();
        assert_eq!(salti, [false, false, true]);
        // Un A4.
        assert!((s.doc.last_section.page_w - 595.3).abs() < 1.0, "{}", s.doc.last_section.page_w);
    }

    fn cartella(nome: &str) -> PathBuf {
        let d = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/prove-salva").join(nome);
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn documento(titolo: &str, testo: &str) -> Session {
        let mut s = crate::documento::nuovo();
        s.run("text.insert", &json!({ "text": titolo, "raw": true })).unwrap();
        s.run("para.heading1", &json!({})).unwrap();
        s.run("text.newParagraph", &json!({})).unwrap();
        s.run("para.normal", &json!({})).unwrap();
        s.run("text.insert", &json!({ "text": testo, "raw": true })).unwrap();
        s
    }

    fn titoli_e_testi(s: &Session) -> Vec<(bool, String)> {
        s.doc.body.iter().filter_map(|b| b.as_para()).map(|p| (s.doc.styles.resolve_para(&p.props).outline_level.is_some(), p.text.clone())).collect()
    }

    #[test]
    fn salva_e_riapre_con_testo_e_titoli() {
        let d = cartella("salva");
        let file = d.join("Verbale.docx");
        let mut s = documento("Verbale", "Testo del verbale");
        let mut o = Origine::File(file.clone());
        let r = salva(&mut s, &mut o, &d, "Documento").unwrap();
        assert_eq!((r.come, r.tipo.as_str(), r.percorso.as_path()), (Come::SulFile, "docx", file.as_path()));
        assert!(!s.dirty);
        assert_eq!(r.byte, std::fs::metadata(&file).unwrap().len());
        let riaperto = crate::documento::apri_byte("Verbale.docx", &std::fs::read(&file).unwrap()).unwrap();
        assert_eq!(titoli_e_testi(&riaperto), titoli_e_testi(&s));
        assert_eq!(titoli_e_testi(&riaperto), [(true, "Verbale".to_string()), (false, "Testo del verbale".to_string())]);
        // Salvato di nuovo, sullo stesso file.
        s.run("text.insert", &json!({ "text": "!", "raw": true })).unwrap();
        salva(&mut s, &mut o, &d, "Documento").unwrap();
        assert_eq!(std::fs::read_dir(&d).unwrap().count(), 1, "niente temporanei né copie");
    }

    #[test]
    fn il_nome_dell_autore_va_nei_commenti_e_non_nelle_proprieta() {
        let d = cartella("autore");
        let file = d.join("Verbale.docx");
        let mut s = documento("Verbale", "Testo del verbale");
        assert_eq!(s.author, "", "vuoto finché non lo si scrive nelle Impostazioni");
        s.author = "Anna Bianchi".into();
        s.run("select.range", &json!({ "anchor": wordcraft_engine::doc::Pos::body(1, 0), "focus": wordcraft_engine::doc::Pos::body(1, 5) })).unwrap();
        s.run("review.newComment", &json!({ "text": "Da rivedere" })).unwrap();
        let mut o = Origine::File(file.clone());
        salva(&mut s, &mut o, &d, "Documento").unwrap();
        let riaperto = crate::documento::apri_byte("Verbale.docx", &std::fs::read(&file).unwrap()).unwrap();
        let c = riaperto.doc.comments.values().next().expect("il commento nel file");
        assert_eq!(c.author, "Anna Bianchi");
        assert_eq!((riaperto.doc.core.last_modified_by.as_str(), riaperto.doc.core.creator.as_str()), ("", ""));
        assert!(riaperto.doc.core.revision >= 1 && !riaperto.doc.core.modified.is_empty(), "gli altri metadati come Word");
    }

    #[test]
    fn un_doc_si_salva_come_copia_docx() {
        let d = cartella("doc");
        let vecchio = d.join("Lettera.doc");
        std::fs::write(&vecchio, b"originale").unwrap();
        // Un .docx con lo stesso nome c'è già: non si tocca.
        std::fs::write(d.join("Lettera.docx"), b"altro").unwrap();
        let mut s = documento("Lettera", "Gentile");
        let mut o = Origine::File(vecchio.clone());
        let r = salva(&mut s, &mut o, &d, "Documento").unwrap();
        assert_eq!(r.come, Come::ComeDocx);
        assert_eq!(r.percorso, d.join("Lettera (2).docx"));
        assert_eq!(o, Origine::File(d.join("Lettera (2).docx")));
        assert_eq!(std::fs::read(&vecchio).unwrap(), b"originale");
        assert_eq!(std::fs::read(d.join("Lettera.docx")).unwrap(), b"altro");
        assert!(!s.dirty);
    }

    #[test]
    fn un_odt_e_un_rtf_si_salvano_come_docx_accanto_e_l_originale_resta() {
        // Analisi del 27/10, critico 4: il motore salta commenti, note, intestazioni e piè di
        // pagina di .odt e .rtf, e «Salva» riscriveva il file dell'utente senza.
        for ext in ["odt", "rtf"] {
            let d = cartella(&format!("salva-{ext}"));
            let vecchio = d.join(format!("Relazione.{ext}"));
            std::fs::write(&vecchio, b"originale").unwrap();
            let mut s = documento("Relazione", "Paziente stabile");
            let mut o = Origine::File(vecchio.clone());
            let r = salva(&mut s, &mut o, &d, "Documento").unwrap();
            assert_eq!((r.come, r.percorso), (Come::ComeDocx, d.join("Relazione.docx")), "{ext}");
            assert_eq!(std::fs::read(&vecchio).unwrap(), b"originale", "{ext}: il file dell'utente è stato riscritto");
            // Copie e condivisione in .docx.
            assert_eq!(formato_della_copia(&Origine::File(vecchio)), "docx", "{ext}");
        }
    }

    #[test]
    fn un_nuovo_prende_il_nome_dal_primo_titolo() {
        let d = cartella("nuovo");
        let mut s = crate::documento::nuovo();
        s.run("text.insert", &json!({ "text": "Gentile dottore", "raw": true })).unwrap();
        s.run("text.newParagraph", &json!({})).unwrap();
        s.run("text.insert", &json!({ "text": "Relazione 12/10/2026", "raw": true })).unwrap();
        s.run("para.heading1", &json!({})).unwrap();
        let mut o = Origine::Nuovo;
        let r = salva(&mut s, &mut o, &d, "Documento").unwrap();
        assert_eq!(r.come, Come::Copia);
        assert_eq!(r.percorso, d.join("Documents/Tascabile/Relazione 12 10 2026.docx"));
        assert_eq!(o, Origine::File(r.percorso.clone()));
        // Senza titoli, il primo paragrafo che dice qualcosa; senza niente, il ripiego.
        let mut s = crate::prova::documento(&["", "---", "Appunti di oggi"]);
        assert_eq!(salva(&mut s, &mut Origine::Nuovo, &d, "Documento").unwrap().percorso.file_name().unwrap(), "Appunti di oggi.docx");
        let mut s = crate::prova::documento(&[""]);
        let r = salva(&mut s, &mut Origine::Nuovo, &d, "Documento 24-10-2026 09.30").unwrap();
        assert_eq!(r.percorso.file_name().unwrap(), "Documento 24-10-2026 09.30.docx");
    }

    #[test]
    fn un_apri_con_usa_il_nome_vero() {
        let d = cartella("uri");
        let mut s = documento("Titolo", "x");
        let mut o = Origine::Uri { uri: "content://com.whatsapp.provider.media/item/1".into(), nome: "Referto Rossi.odt".into() };
        let r = salva(&mut s, &mut o, &d, "Documento").unwrap();
        // Un .odt diventa .docx: il motore non lo scrive senza perdere commenti e note.
        assert_eq!(r.percorso, d.join("Documents/Tascabile/Referto Rossi.docx"));
        // Senza il nome vero (documento.docx dal tipo MIME), il nome dal testo.
        let mut o = Origine::Uri { uri: "content://x/1".into(), nome: "documento.docx".into() };
        assert_eq!(salva(&mut s, &mut o, &d, "Documento").unwrap().percorso, d.join("Documents/Tascabile/Titolo.docx"));
    }

    #[test]
    fn il_nome_della_copia_non_ha_testo_cancellato_ne_nascosto() {
        let d = cartella("nascosto");
        let mut s = crate::documento::nuovo();
        s.run("text.insert", &json!({ "text": "Relazione su Mario Rossi di Bianchi Verdi", "raw": true })).unwrap();
        s.run("select.text", &json!({ "text": "di Bianchi" })).unwrap();
        s.run("format.hidden", &json!({ "value": true })).unwrap();
        s.run("select.text", &json!({ "text": "Verdi" })).unwrap();
        s.run("format.charStyle", &json!({ "style": "Strong" })).unwrap();
        s.run("styles.modify", &json!({ "style": "Strong", "chr": { "hidden": true } })).unwrap();
        s.run("review.trackChanges", &json!({ "value": true })).unwrap();
        s.run("select.text", &json!({ "text": "Mario Rossi" })).unwrap();
        s.run("text.insert", &json!({ "text": "paziente A", "raw": true })).unwrap();
        // Passato dal lettore .docx, come un file arrivato da fuori.
        let byte = wordcraft_engine::io::save_bytes("x.docx", &s.doc).unwrap();
        let mut s = crate::documento::apri_byte("x.docx", &byte).unwrap();
        let r = salva_copia(&mut s, &mut Origine::Nuovo, &d, "Documento").unwrap();
        let nome = r.percorso.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(nome, "Relazione su paziente A.docx");
        for parola in ["Mario", "Rossi", "Bianchi", "Verdi"] {
            assert!(!nome.contains(parola), "{nome}");
        }
        let pdf = esporta_pdf(&s, &Origine::Nuovo, &d, "Documento").unwrap();
        assert_eq!(pdf.percorso.file_name().unwrap(), "Relazione su paziente A.pdf");
    }

    #[test]
    fn i_font_simbolo_si_leggono_come_si_vedono() {
        // «Dose 10 µg e 5 µg»: la prima µ da insert.symbol (U+F06D in Symbol), la seconda una «m»
        // in Symbol. Negli appunti e nei nomi deve arrivare µ, mai «m» (5 mg) né un carattere ad
        // uso privato.
        let mut s = crate::documento::nuovo();
        s.run("text.insert", &json!({ "text": "Dose 10 ", "raw": true })).unwrap();
        s.run("insert.symbol", &json!({ "char": "\u{F06D}", "font": "Symbol" })).unwrap();
        s.run("text.insert", &json!({ "text": "g e 5 mg", "raw": true })).unwrap();
        let m = s.doc.body[0].as_para().unwrap().text.rfind('m').unwrap();
        let pos = |off| wordcraft_engine::doc::Pos::body(0, off);
        s.run("select.range", &json!({ "anchor": pos(m), "focus": pos(m + 1) })).unwrap();
        s.run("format.font", &json!({ "name": "Symbol" })).unwrap();
        let byte = wordcraft_engine::io::save_bytes("x.docx", &s.doc).unwrap();
        let s = crate::documento::apri_byte("x.docx", &byte).unwrap();
        let p = s.doc.body[0].as_para().unwrap();
        assert_eq!(p.shown_text(), "Dose 10 \u{3BC}g e 5 \u{3BC}g");
        assert_eq!(come_si_legge(&s.doc, p), "Dose 10 \u{3BC}g e 5 \u{3BC}g");
    }

    /// Un .docx minimo con `corpo` dentro `<w:body>`: le tre parti che servono, scritte a mano.
    fn docx_con(corpo: &str) -> Vec<u8> {
        const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
        let voci = [
            ("[Content_Types].xml", r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#.to_string()),
            ("_rels/.rels", r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.to_string()),
            ("word/document.xml", format!(r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="{W}"><w:body>{corpo}</w:body></w:document>"#)),
        ];
        let mut z = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
        for (nome, testo) in voci {
            z.start_file(nome, zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(testo.as_bytes()).unwrap();
        }
        z.finish().unwrap().into_inner()
    }

    #[test]
    fn il_simbolo_di_wingdings_nel_nome_e_quello_che_si_vede() {
        // Il segno di spunta di Word: il carattere F0FC in Wingdings (`w:sym`), che si vede «✓»
        // (U+2713). Nel nome arriva «✓», mai il carattere ad uso privato, anche quando il font
        // del simbolo sta a parte da quello della run (in Calibri, qui la seconda).
        let sym = r#"<w:sym w:font="Wingdings" w:char="F0FC"/><w:t xml:space="preserve"> Fatto</w:t>"#;
        let calibri = r#"<w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/></w:rPr>"#;
        for run in [format!("<w:r>{sym}</w:r>"), format!("<w:r>{calibri}{sym}</w:r>")] {
            let s = crate::documento::apri_byte("Spunta.docx", &docx_con(&format!("<w:p>{run}</w:p>"))).unwrap();
            assert_eq!(nome_dal_testo(&s.doc).as_deref(), Some("\u{2713} Fatto"), "{run}");
        }
    }

    #[test]
    fn pdf_non_vuoto() {
        let d = cartella("pdf");
        let s = documento("Verbale", "Testo");
        let o = Origine::File(d.join("Verbale.docx"));
        let r = esporta_pdf(&s, &o, &d, "Documento").unwrap();
        assert_eq!((r.come, r.tipo.as_str()), (Come::Pdf, "pdf"));
        let byte = std::fs::read(&r.percorso).unwrap();
        assert!(byte.starts_with(b"%PDF"), "{:?}", &byte[..8.min(byte.len())]);
        assert!(byte.len() > 500);
        assert_eq!(r.percorso, d.join("Documents/Tascabile/Verbale.pdf"));
        // Il secondo non sostituisce il primo.
        assert_eq!(esporta_pdf(&s, &o, &d, "Documento").unwrap().percorso, d.join("Documents/Tascabile/Verbale (2).pdf"));
        // E si condivide da condivisi/.
        let c = copia_in_condivisi(&r.percorso, &d.join("cache")).unwrap();
        assert_eq!(c.percorso.file_name().unwrap(), "Verbale.pdf");
        assert_eq!(c.percorso.parent().unwrap().parent().unwrap(), d.join("cache/condivisi"));
    }

    #[test]
    fn condividi_prepara_il_documento_in_condivisi() {
        let d = cartella("condividi");
        let mut s = documento("Verbale", "Testo");
        s.run("text.insert", &json!({ "text": " non salvato", "raw": true })).unwrap();
        let r = prepara_condivisione(&s, &Origine::Nuovo, &d.join("cache"), "Documento").unwrap();
        assert_eq!(r.percorso.file_name().unwrap(), "Verbale.docx");
        assert_eq!(r.percorso.parent().unwrap().parent().unwrap(), d.join("cache/condivisi"));
        let riaperto = crate::documento::apri_byte("x.docx", &std::fs::read(&r.percorso).unwrap()).unwrap();
        assert_eq!(crate::prova::testi(&riaperto)[1], "Testo non salvato");
        assert!(s.dirty, "condividere non salva");
        // Il secondo prende il posto del primo, ma in un'altra sottocartella: un'app che ha ancora
        // il permesso sull'URI del primo non legge il secondo, che ha lo stesso nome.
        let altro = documento("Verbale", "Un altro paziente");
        let r2 = prepara_condivisione(&altro, &Origine::Nuovo, &d.join("cache"), "Documento").unwrap();
        assert_eq!(r2.percorso.file_name().unwrap(), "Verbale.docx");
        assert_ne!(r2.percorso, r.percorso);
        assert!(!r.percorso.exists());
        assert_eq!(std::fs::read_dir(d.join("cache/condivisi")).unwrap().count(), 1);
        let c = copia_in_condivisi(&r2.percorso, &d.join("cache")).unwrap();
        assert_ne!(c.percorso, r2.percorso);
        assert_eq!(std::fs::read_dir(d.join("cache/condivisi")).unwrap().count(), 1);
    }

    #[test]
    fn i_byte_di_un_recente_vanno_in_condivisi_col_suo_nome() {
        let d = cartella("byte-condivisi");
        let r = byte_in_condivisi("Referto.docx", b"PK", &d.join("cache")).unwrap();
        assert_eq!(r.percorso.file_name().unwrap(), "Referto.docx");
        assert_eq!(r.percorso.parent().unwrap().parent().unwrap(), d.join("cache/condivisi"));
        assert_eq!((r.byte, r.tipo.as_str(), r.come), (2, "docx", Come::Condivisione));
        assert_eq!(std::fs::read(&r.percorso).unwrap(), b"PK");
        // Un nome con le barre o vuoto non esce dalla sottocartella.
        for (nome, atteso) in [("../../fuori.docx", "fuori.docx"), ("a\\b.odt", "b.odt"), ("", "documento"), ("..", "documento")] {
            let r = byte_in_condivisi(nome, b"x", &d.join("cache")).unwrap();
            assert_eq!(r.percorso.file_name().unwrap(), atteso, "{nome:?}");
            assert_eq!(r.percorso.parent().unwrap().parent().unwrap(), d.join("cache/condivisi"));
        }
    }

    #[test]
    fn un_salvataggio_che_fallisce_lascia_il_documento_modificato() {
        let d = cartella("fallisce");
        // Un file al posto della cartella Documents.
        std::fs::write(d.join("Documents"), b"").unwrap();
        let mut s = documento("Verbale", "Testo");
        let mut o = Origine::Nuovo;
        let e = salva(&mut s, &mut o, &d, "Documento").unwrap_err();
        assert!(e.genere == "AlreadyExists" || e.genere == "NotADirectory", "{e:?}");
        assert!(s.dirty);
        assert_eq!(o, Origine::Nuovo);
    }

    #[test]
    fn scrivi_nuovo_non_sostituisce_mai() {
        let d = cartella("nuovo-file");
        std::fs::write(d.join("A.docx"), b"vecchio").unwrap();
        let p = scrivi_nuovo(&d, "A", "docx", b"nuovo").unwrap();
        assert_eq!(p, d.join("A (2).docx"));
        assert_eq!(std::fs::read(d.join("A.docx")).unwrap(), b"vecchio");
        assert_eq!(std::fs::read(&p).unwrap(), b"nuovo");
    }

    #[test]
    fn ripiego_con_data_e_ora() {
        let n = nome_di_ripiego_adesso();
        assert!(n.starts_with("Documento ") && n.len() == "Documento 24-10-2026 09.30".len(), "{n}");
    }
}
