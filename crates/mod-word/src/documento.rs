//! Aprire e creare documenti Word con il motore di WordCraft.

use wordcraft_engine::{Region, Session};

use crate::pacchetto;

/// Punti egui (dp) per punto tipografico al 100%: lo stesso 96/72 di WordCraft.
pub const DP_PER_PT: f32 = 96.0 / 72.0;

fn prepara(s: &mut Session, regione: Region) {
    s.system_region = regione;
    // Il testo senza una lingua sua è nella lingua di chi scrive, come in Word: per il motore
    // sarebbe inglese, e l'ortografia sottolineerebbe ogni parola italiana. Una lingua scritta nel
    // file resta quella.
    if s.doc.styles.default_chr.lang.as_deref().is_none_or(|l| l.trim().is_empty()) {
        s.doc.styles.default_chr.lang = Some("it-IT".into());
        s.relayout();
    }
    s.set_editing_language(Some("it-IT"));
    // Le cancellazioni delle revisioni restano nel testo, barrate, anche nella vista Pagina: qui
    // non ci sono i fumetti a lato della pagina dove il motore le sposterebbe.
    s.prefs.markup.balloons = wordcraft_engine::layout::display::BalloonMode::Inline;
    // L'autore dei commenti e delle revisioni lo mette l'app, dalle Impostazioni: mai quello del
    // motore («WordCraft User»), vuoto finché chi usa l'app non lo scrive.
    s.author = String::new();
    crate::ortografia::prepara(s);
    // Le correzioni le fa la tastiera del telefono: quelle del motore cambierebbero il testo
    // alle spalle della tastiera (ricerca WordCraft, sezione 5).
    s.prefs.autocorrect.enabled = false;
    // Il motore non scrive mai da solo sul file dell'utente: è solo un segnale per l'interfaccia
    // di WordCraft, ma qui si salva solo con «Salva».
    s.autosave = false;
}

/// Un documento che non si apre: il genere va nella diagnosi, il messaggio (`Display`) sullo
/// schermo, perché quello del motore può riportare il nome del file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErroreApertura {
    pub genere: &'static str,
    pub messaggio: String,
}

impl std::fmt::Display for ErroreApertura {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.messaggio)
    }
}

fn errore(genere: &'static str, messaggio: impl Into<String>) -> ErroreApertura {
    ErroreApertura { genere, messaggio: messaggio.into() }
}

/// Apre dei byte; il formato lo decide l'estensione di `nome` (con controllo del contenuto).
/// Un file vuoto diventa un documento nuovo, come in WordCraft; un errore o un panico del
/// lettore diventano `Err`, mai un'app che si chiude. Un archivio zip (docx, odt) passa prima dal
/// controllo di `pacchetto` (voci e byte decompressi) e il motore lo legge senza i font
/// incorporati, che terrebbe per tutto il processo anche per i documenti aperti dopo; poi le
/// parti dei font si rimettono nel documento così come sono (`rimetti_i_font`), perché un
/// salvataggio sul file dell'utente non li perda.
pub fn apri_byte(nome: &str, byte: &[u8]) -> Result<Session, ErroreApertura> {
    if byte.is_empty() {
        return Ok(nuovo());
    }
    let originale = byte;
    let mut byte = std::borrow::Cow::Borrowed(byte);
    // Il controllo degli archivi gira su ogni file, non solo su quelli che sembrano zip: il lettore
    // dei motori trova un archivio anche con byte in fondo (revisione del 31/10, punto 1). Un file
    // che non si apre come zip è un altro formato (doc, rtf, testo) e lo giudica il motore; se
    // sembrava uno zip, è rovinato.
    let no = |e: pacchetto::ErroreZip| errore(e.genere(), pacchetto::messaggio(e));
    match pacchetto::controlla(&byte) {
        Ok(()) => {
            let pulito = match pacchetto::senza_font_incorporati(&byte).map_err(no)? {
                std::borrow::Cow::Owned(v) => Some(v),
                std::borrow::Cow::Borrowed(_) => None,
            };
            if let Some(v) = pulito {
                byte = std::borrow::Cow::Owned(v);
            }
        }
        Err(pacchetto::ErroreZip::NonValido) if !pacchetto::sembra_zip(&byte) => {}
        Err(e) => return Err(no(e)),
    }
    // Il lettore dei Word è ricorsivo, fino a 256 livelli di elementi annidati: lettura e prima
    // impaginazione girano con lo stack grande di un thread loro, non con quello del thread
    // dell'interfaccia (circa 1 MB su Android), dove uno stack finito chiude l'app senza un panico
    // da prendere (revisione del 31/10, punto 2). Il chiamante aspetta.
    let leggi = || {
        let regione = Region::from_tag("it-IT");
        let letto = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| wordcraft_engine::io::open_bytes_in(nome, &byte, None, &regione)));
        let (mut doc, _cifrato) = match letto {
            Ok(Ok(letto)) => letto,
            Ok(Err(e)) if wordcraft_engine::io::needs_password(&e) => {
                return Err(errore("protetto da password", "Il documento è protetto da password: questa versione non lo apre."));
            }
            Ok(Err(e)) => return Err(errore("errore del lettore", e)),
            Err(_) => return Err(errore("panico del lettore", "Il lettore del documento si è fermato con un errore interno.")),
        };
        if let std::borrow::Cow::Owned(_) = byte {
            rimetti_i_font(&mut doc, originale);
        }
        let mut s = Session::new(doc);
        let preparato = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| prepara(&mut s, regione)));
        preparato.map_err(|_| errore("panico del lettore", "Il lettore del documento si è fermato con un errore interno."))?;
        Ok(s)
    };
    std::thread::scope(|ambito| {
        let lettore = std::thread::Builder::new().name("word-lettura".into()).stack_size(STACK_DEL_LETTORE).spawn_scoped(ambito, leggi);
        match lettore {
            Ok(lettore) => lettore
                .join()
                .unwrap_or_else(|_| Err(errore("panico del lettore", "Il lettore del documento si è fermato con un errore interno."))),
            Err(_) => Err(errore("thread non partito", "Il telefono non ha memoria per aprire il documento.")),
        }
    })
}

/// Lo stack del thread che legge un Word: memoria riservata, non usata finché non serve.
const STACK_DEL_LETTORE: usize = 64 << 20;

/// Rimette in `doc` (letto senza i font incorporati) la tabella dei font e le parti dei font
/// dell'archivio `originale`, rilette col solo lettore .docx, che non le carica nel database dei
/// font del processo. Così il motore le riscrive come parti opache quando si salva, come avrebbe
/// fatto da solo (`docx/src/fonts.rs`).
fn rimetti_i_font(doc: &mut wordcraft_engine::doc::Document, originale: &[u8]) {
    // I nomi delle due parti nel passthrough del lettore (`docx/src/package.rs:62-64`, privati):
    // la tabella dei font e l'elenco delle parti tenute con le loro relazioni.
    const TABELLA_DEI_FONT: &str = "word/fontTable.xml";
    const PARTI_TENUTE: &str = "wordcraft:embedded.parts";
    let letto = std::panic::catch_unwind(|| wordcraft_docx::read(originale));
    let Ok(Ok(con_i_font)) = letto else {
        tascabile_platform::diagnosi("apertura: i font incorporati non si rimettono (il salvataggio li perde)");
        return;
    };
    for (parte, byte) in con_i_font.passthrough {
        let dei_font = parte == TABELLA_DEI_FONT || parte == PARTI_TENUTE;
        if dei_font || !doc.passthrough.contains_key(&parte) {
            doc.passthrough.insert(parte, byte);
        }
    }
}

/// Documento vuoto, A4, in italiano.
pub fn nuovo() -> Session {
    let regione = Region::from_tag("it-IT");
    let mut s = Session::new(wordcraft_engine::blank_document(&regione));
    prepara(&mut s, regione);
    s
}

/// I nomi che `nome_da_mime` può dare: non vengono dal file.
pub const NOMI_DA_MIME: &[&str] = &["documento.docx", "documento.doc", "documento.odt", "documento.rtf", "documento.txt", "documento"];

/// Nome con estensione per un file arrivato con «Apri con» (il motore sceglie il lettore dall'estensione).
pub fn nome_da_mime(mime: Option<&str>) -> &'static str {
    match mime {
        Some("application/vnd.openxmlformats-officedocument.wordprocessingml.document") => "documento.docx",
        Some("application/msword") => "documento.doc",
        Some("application/vnd.oasis.opendocument.text") => "documento.odt",
        Some("application/rtf") | Some("text/rtf") => "documento.rtf",
        Some("text/plain") => "documento.txt",
        _ => "documento",
    }
}

/// Estensione di `nome`, in minuscolo, se è una di quelle che il motore apre. Serve anche alla
/// diagnosi: dice il tipo del file senza mai dirne il nome.
pub fn estensione_nota(nome: &str) -> Option<String> {
    let (base, ext) = nome.rsplit_once('.')?;
    let ext = ext.to_ascii_lowercase();
    (!base.is_empty() && wordcraft_engine::io::OPEN_EXTS.contains(&ext.as_str())).then_some(ext)
}

/// Nome per il motore: dal percorso se finisce con un'estensione che il motore apre,
/// altrimenti dal tipo MIME.
pub fn nome_per(uri: &str, mime: Option<&str>) -> String {
    let ultimo = uri.rsplit('/').next().unwrap_or(uri);
    if estensione_nota(ultimo).is_some() { ultimo.to_string() } else { nome_da_mime(mime).to_string() }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::salva::come_si_legge;
    use serde_json::json;
    use tascabile_files::nome_pulito;
    use wordcraft_engine::doc::StoryRef;

    #[test]
    fn nuovo_e_italiano_senza_autocorrezione() {
        let s = nuovo();
        assert_eq!(s.doc.paragraph_count(), 1);
        assert!(!s.prefs.autocorrect.enabled);
    }

    #[test]
    fn un_documento_senza_lingua_si_controlla_in_italiano() {
        crate::ortografia::test::con_il_dizionario();
        // Un .docx senza lingua da nessuna parte: per il motore sarebbe inglese.
        let mut s = nuovo();
        s.doc.styles.default_chr.lang = None;
        s.run("text.insert", &json!({ "text": "Il letto perche la terapia", "raw": true })).unwrap();
        let byte = wordcraft_engine::io::save_bytes("prova.docx", &s.doc).unwrap();
        let mut s = apri_byte("prova.docx", &byte).unwrap();
        assert_eq!(s.doc.styles.default_chr.lang.as_deref(), Some("it-IT"));
        s.run("caret.set", &json!({ "pos": wordcraft_engine::doc::Pos::body(0, 11) })).unwrap();
        let sb = crate::ortografia::al_cursore(&mut s).expect("solo «perche» è sbagliata");
        assert_eq!((sb.inizio.off, sb.fine.off), (9, 15));
        s.run("caret.set", &json!({ "pos": wordcraft_engine::doc::Pos::body(0, 4) })).unwrap();
        assert_eq!(crate::ortografia::al_cursore(&mut s), None, "«letto» è italiano, non inglese");
        // Uno in inglese resta in inglese.
        let mut s = nuovo();
        s.doc.styles.default_chr.lang = Some("en-US".into());
        s.run("text.insert", &json!({ "text": "The bed", "raw": true })).unwrap();
        let byte = wordcraft_engine::io::save_bytes("prova.docx", &s.doc).unwrap();
        assert_eq!(apri_byte("prova.docx", &byte).unwrap().doc.styles.default_chr.lang.as_deref(), Some("en-US"));
    }

    #[test]
    fn le_cancellazioni_restano_nel_testo_anche_in_pagina() {
        let s = nuovo();
        assert_eq!(s.prefs.markup.balloons, wordcraft_engine::layout::display::BalloonMode::Inline);
        assert!(!s.revisions_in_balloons());
    }

    #[test]
    fn docx_avanti_e_indietro() {
        let mut s = nuovo();
        s.run("text.insert", &json!({ "text": "Perché è così", "raw": true })).unwrap();
        let byte = wordcraft_engine::io::save_bytes("prova.docx", &s.doc).unwrap();
        let s2 = apri_byte("prova.docx", &byte).unwrap();
        assert_eq!(crate::prova::testi(&s2), ["Perché è così"]);
    }

    #[test]
    fn nomi() {
        assert_eq!(nome_per("/home/x/Lettera.docx", None), "Lettera.docx");
        let docx = "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
        assert_eq!(nome_per("content://com.whatsapp.provider.media/item/123", Some(docx)), "documento.docx");
        assert_eq!(nome_per("content://x/item/123", None), "documento");
    }

    #[test]
    fn file_vuoto_e_un_documento_nuovo() {
        let s = apri_byte("Nuovo.docx", &[]).unwrap();
        assert_eq!(s.doc.paragraph_count(), 1);
    }

    #[test]
    fn spazzatura_da_errore_senza_panico() {
        assert!(apri_byte("documento", &[0u8, 1, 2, 3, 255, 254]).is_err());
    }

    #[test]
    fn password_con_messaggio_italiano() {
        let s = nuovo();
        let opts = wordcraft_engine::io::ExportOptions::default();
        let cifrato = wordcraft_engine::io::save_bytes_with("x.docx", &s.doc, Some("segreta"), &opts).unwrap();
        let errore = apri_byte("x.docx", &cifrato).err().unwrap();
        assert!(errore.to_string().contains("protetto da password"), "{errore}");
        assert_eq!(errore.genere, "protetto da password");
    }

    #[test]
    fn estensione_per_la_diagnosi() {
        assert_eq!(estensione_nota("Verbale Rossi.DOCX").as_deref(), Some("docx"));
        assert_eq!(estensione_nota("relazione.v2.odt").as_deref(), Some("odt"));
        // Solo estensioni che il motore conosce: il resto del nome non esce mai.
        assert_eq!(estensione_nota("Rossi.Mario"), None);
        assert_eq!(estensione_nota("documento"), None);
        assert_eq!(estensione_nota(".docx"), None);
    }

    #[test]
    fn i_documenti_della_home_li_apre_il_motore() {
        // Un file che tascabile-files chiama Documento la Home lo apre con questo modulo.
        for e in tascabile_files::ESTENSIONI_DOCUMENTO {
            assert!(wordcraft_engine::io::OPEN_EXTS.contains(e), "il motore non apre .{e}");
        }
    }

    #[test]
    fn ogni_formato_del_motore_ha_il_suo_tipo() {
        // Questi il motore li apre, ma nella Home non sono documenti da scrivere. Un formato nuovo
        // del motore fa fallire la prova: si decide se è un Documento o se va qui.
        let fuori = ["tex", "latex", "ltx", "xml", "json", "wcraft.json"];
        for e in wordcraft_engine::io::OPEN_EXTS {
            let documento = tascabile_files::tipo_da_nome(&format!("prova.{e}")) == tascabile_files::Tipo::Documento;
            assert_ne!(documento, fuori.contains(e), ".{e}");
        }
    }

    /// Salvato in .docx e riaperto: il testo passa dal lettore .docx, come un file arrivato da
    /// WhatsApp.
    fn riaperto_da_docx(s: &Session) -> Session {
        let byte = wordcraft_engine::io::save_bytes("prova.docx", &s.doc).unwrap();
        apri_byte("prova.docx", &byte).unwrap()
    }

    #[test]
    fn il_nome_non_ha_il_testo_cancellato_con_le_revisioni() {
        // Un titolo anonimizzato con le revisioni attive: «Mario Rossi» cancellato e «paziente
        // A» scritto al suo posto.
        let mut s = nuovo();
        s.run("text.insert", &json!({ "text": "Relazione su Mario Rossi", "raw": true })).unwrap();
        s.run("review.trackChanges", &json!({ "value": true })).unwrap();
        s.run("select.text", &json!({ "text": "Mario Rossi" })).unwrap();
        s.run("text.insert", &json!({ "text": "paziente A", "raw": true })).unwrap();
        let s = riaperto_da_docx(&s);
        let p = s.doc.body[0].as_para().unwrap();
        // Il lettore .docx tiene il testo cancellato nel testo del paragrafo: da qui il nome del
        // file avrebbe il nome cancellato.
        for testo in [p.text.clone(), p.plain_text(), p.shown_text(), s.doc.plain_text(StoryRef::Body), s.doc.shown_text(StoryRef::Body)] {
            assert!(nome_pulito(&testo).contains("Mario Rossi"), "{testo:?}");
        }
        assert_eq!(nome_pulito(&p.final_text()), "Relazione su paziente A");
        assert_eq!(nome_pulito(&s.doc.final_text(StoryRef::Body)), "Relazione su paziente A");
        assert_eq!(nome_pulito(&come_si_legge(&s.doc, p)), "Relazione su paziente A");
    }

    #[test]
    fn il_nome_non_ha_il_testo_nascosto() {
        // «di Mario» nascosto a mano e «Rossi» da uno stile di carattere nascosto: si legge
        // «Verbale», ma `final_text` li tiene tutti e due.
        let mut s = nuovo();
        s.run("text.insert", &json!({ "text": "Verbale di Mario Rossi", "raw": true })).unwrap();
        s.run("select.text", &json!({ "text": "di Mario" })).unwrap();
        s.run("format.hidden", &json!({ "value": true })).unwrap();
        s.run("select.text", &json!({ "text": "Rossi" })).unwrap();
        s.run("format.charStyle", &json!({ "style": "Strong" })).unwrap();
        s.run("styles.modify", &json!({ "style": "Strong", "chr": { "hidden": true } })).unwrap();
        let s = riaperto_da_docx(&s);
        let p = s.doc.body[0].as_para().unwrap();
        assert_eq!(nome_pulito(&p.final_text()), "Verbale di Mario Rossi");
        assert_eq!(nome_pulito(&s.doc.final_text(StoryRef::Body)), "Verbale di Mario Rossi");
        // Per «Rossi» il nascosto viene dallo stile, non dalla run: guardando solo
        // `CharProps::hidden` resterebbe.
        let a_mano: Vec<_> = p.run_ranges().filter(|(_, c)| c.hidden == Some(true)).map(|(intervallo, _)| intervallo).collect();
        assert_eq!(nome_pulito(&p.text_without(&a_mano)), "Verbale Rossi");
        assert_eq!(nome_pulito(&come_si_legge(&s.doc, p)), "Verbale");
    }

    /// JetBrains Mono del motore con la famiglia «Calibri» nella tabella dei nomi (famiglia e
    /// famiglia tipografica, ogni codifica): un font che il telefono non ha.
    fn falso_calibri() -> Vec<u8> {
        let percorso = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../upstream/wordcraft/assets/fonts/JetBrainsMono-Regular.ttf");
        let mut f = std::fs::read(percorso).unwrap();
        let u16_a = |f: &[u8], i: usize| u16::from_be_bytes([f[i], f[i + 1]]) as usize;
        let u32_a = |f: &[u8], i: usize| u32::from_be_bytes([f[i], f[i + 1], f[i + 2], f[i + 3]]) as usize;
        let tabella = (0..u16_a(&f, 4)).map(|t| 12 + 16 * t).find(|&r| &f[r..r + 4] == b"name").unwrap();
        let nomi = u32_a(&f, tabella + 8);
        let (quanti, testi) = (u16_a(&f, nomi + 2), nomi + u16_a(&f, nomi + 4));
        for r in (0..quanti).map(|i| nomi + 6 + 12 * i) {
            let (piattaforma, id, inizio) = (u16_a(&f, r), u16_a(&f, r + 6), testi + u16_a(&f, r + 10));
            if id != 1 && id != 16 {
                continue;
            }
            let nuovo: Vec<u8> = if piattaforma == 1 { b"Calibri".to_vec() } else { "Calibri".encode_utf16().flat_map(u16::to_be_bytes).collect() };
            f[inizio..inizio + nuovo.len()].copy_from_slice(&nuovo);
            f[r + 8..r + 10].copy_from_slice(&(nuovo.len() as u16).to_be_bytes());
        }
        f
    }

    /// Un .docx scritto in Calibri che incorpora `font` dalla tabella dei font, senza chiave
    /// (come il .ttf in chiaro delle prove del motore, `engine/src/tests_fonts.rs`).
    fn docx_che_incorpora(font: &[u8]) -> Vec<u8> {
        use std::io::Write;
        const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
        const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
        const PR: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
        let rels = |t: &str, g: &str| {
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="{PR}"><Relationship Id="rId{t}" Type="{R}/{t}" Target="{g}"/></Relationships>"#
            )
        };
        let voci: Vec<(&str, Vec<u8>)> = vec![
            ("[Content_Types].xml", br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="ttf" ContentType="application/x-font-ttf"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/fontTable.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.fontTable+xml"/></Types>"#.to_vec()),
            ("_rels/.rels", rels("officeDocument", "word/document.xml").into_bytes()),
            ("word/_rels/document.xml.rels", rels("fontTable", "fontTable.xml").into_bytes()),
            ("word/document.xml", format!(r#"<w:document xmlns:w="{W}"><w:body><w:p><w:r><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/></w:rPr><w:t>Dose 10 mg</w:t></w:r></w:p></w:body></w:document>"#).into_bytes()),
            ("word/fontTable.xml", format!(r#"<w:fonts xmlns:w="{W}" xmlns:r="{R}"><w:font w:name="Calibri"><w:embedRegular r:id="rIdfont" w:fontKey="{{00000000-0000-0000-0000-000000000000}}"/></w:font></w:fonts>"#).into_bytes()),
            ("word/_rels/fontTable.xml.rels", rels("font", "fonts/font1.ttf").into_bytes()),
            ("word/fonts/font1.ttf", font.to_vec()),
        ];
        let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (nome, byte) in voci {
            z.start_file(nome, zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(&byte).unwrap();
        }
        z.finish().unwrap().into_inner()
    }

    #[test]
    fn un_font_incorporato_non_resta_per_i_documenti_dopo() {
        let db = wordcraft_fonts::FontDb::global();
        if db.has_family("Calibri") {
            eprintln!("questo sistema ha già un Calibri: la prova non dice niente");
            return;
        }
        let font = falso_calibri();
        // Il font finto è buono: aggiunto a un database a parte, porta la famiglia «Calibri».
        let a_parte = wordcraft_fonts::FontDb::with_font_dirs(Vec::new());
        assert!(a_parte.add_font(font.clone()) > 0);
        assert!(a_parte.has_family("Calibri"), "il font finto non si chiama Calibri: {:?}", a_parte.families());
        let s = apri_byte("ostile.docx", &docx_che_incorpora(&font)).unwrap();
        assert_eq!(crate::prova::testi(&s), ["Dose 10 mg"]);
        assert!(!db.has_family("Calibri"), "il Calibri del documento è rimasto nel processo");
        // Salvato di nuovo, il documento tiene i suoi font (il file dell'utente non perde niente) e
        // si riapre, sempre senza caricarli nel processo.
        let mut s = s;
        s.run("text.insert", &serde_json::json!({ "text": "!", "raw": true })).unwrap();
        let byte = wordcraft_engine::io::save_bytes("x.docx", &s.doc).unwrap();
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(&byte)).unwrap();
        let mut voci: Vec<String> =
            z.file_names().map(Result::unwrap).filter(|n| n.to_ascii_lowercase().contains("font")).map(|n| n.into_owned()).collect();
        voci.sort();
        assert_eq!(voci, ["word/_rels/fontTable.xml.rels", "word/fontTable.xml", "word/fonts/font1.ttf"]);
        let mut tabella = String::new();
        std::io::Read::read_to_string(&mut z.by_name("word/fontTable.xml").unwrap(), &mut tabella).unwrap();
        assert!(tabella.contains("embedRegular"), "{tabella}");
        let mut file_del_font = Vec::new();
        std::io::Read::read_to_end(&mut z.by_name("word/fonts/font1.ttf").unwrap(), &mut file_del_font).unwrap();
        assert_eq!(file_del_font, font);
        assert_eq!(crate::prova::testi(&apri_byte("x.docx", &byte).unwrap()), ["!Dose 10 mg"]);
        assert!(!db.has_family("Calibri"));
    }

    /// Revisione del 31/10, punto 2: il lettore dei Word è ricorsivo, e un documento annidato a 250
    /// livelli finiva lo stack del thread dell'interfaccia (circa 1 MB su Android): l'app si
    /// chiudeva di colpo, senza un panico da prendere. Qui si apre da un thread con quello stack.
    #[test]
    fn un_documento_annidato_non_finisce_lo_stack() {
        let annidato = format!("{}<w:p><w:r><w:t>In fondo</w:t></w:r></w:p>{}", "<w:customXml>".repeat(250), "</w:customXml>".repeat(250));
        let byte = crate::prova::docx(&annidato, "", &[]);
        let aperto = std::thread::Builder::new()
            .stack_size(1 << 20)
            .spawn(move || apri_byte("annidato.docx", &byte).map(|s| crate::prova::testi(&s)).map_err(|e| e.to_string()))
            .unwrap()
            .join()
            .unwrap();
        assert!(aperto.is_ok(), "{aperto:?}");
    }

    #[test]
    fn un_archivio_gonfiato_non_si_apre() {
        use std::io::Write;
        let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        z.start_file("word/document.xml", zip::write::SimpleFileOptions::default()).unwrap();
        z.write_all(&vec![b' '; 40 * 1024 * 1024]).unwrap();
        let byte = z.finish().unwrap().into_inner();
        let e = apri_byte("bomba.docx", &byte).err().unwrap();
        assert_eq!(e.genere, "zip con una voce troppo grande");
        assert!(e.to_string().contains("32 MB"), "{e}");
        // Anche chiamato .txt: il motore lo aprirebbe per contenuto.
        assert_eq!(apri_byte("bomba.txt", &byte).err().unwrap().genere, "zip con una voce troppo grande");
        // Con 70 KB in fondo la fine dell'archivio non è negli ultimi 64 KB: zip 3 lo apriva lo
        // stesso e si controllava (revisione del 31/10, punto 1), zip 9 non lo apre più, né nel
        // controllo né nei motori. In tutti e due i casi non si apre.
        let mut in_fondo = byte.clone();
        in_fondo.extend(vec![b'x'; 70_000]);
        let atteso = if zip::ZipArchive::new(std::io::Cursor::new(&in_fondo)).is_ok() { "zip con una voce troppo grande" } else { "zip non valido" };
        assert_eq!(apri_byte("bomba.docx", &in_fondo).err().unwrap().genere, atteso);
    }

    /// CVE-2026-115905 («zip cloaking»): un .docx la cui intestazione locale del testo dice un altro
    /// nome rispetto alla directory centrale. Il motore crede alla directory e lo aprirebbe; chi
    /// legge in fila (un antivirus, l'app a cui lo si manda) vede un'altra parte. Non si apre.
    #[test]
    fn un_docx_con_le_intestazioni_che_non_coincidono_non_si_apre() {
        let mut byte = crate::prova::docx("<w:p><w:r><w:t>Dose 10 mg</w:t></w:r></w:p>", "", &[]);
        assert_eq!(crate::prova::testi(&apri_byte("buono.docx", &byte).unwrap()), ["Dose 10 mg"]);
        let nome = b"word/document.xml";
        let p = (0..byte.len() - 30 - nome.len())
            .find(|&p| byte[p..p + 4] == [0x50, 0x4b, 0x03, 0x04] && &byte[p + 30..p + 30 + nome.len()] == nome)
            .unwrap();
        byte[p + 30..p + 30 + nome.len()].copy_from_slice(b"word/malevolo.xml");
        let e = apri_byte("mascherato.docx", &byte).err().expect("il documento mascherato si è aperto");
        assert_eq!(e.genere, "zip con intestazioni che non coincidono");
        assert!(e.to_string().contains("due modi diversi"), "{e}");
    }

    #[test]
    fn punto_che_non_e_un_estensione() {
        let odt = "application/vnd.oasis.opendocument.text";
        assert_eq!(nome_per("content://x/item/versione1.2", Some(odt)), "documento.odt");
        assert_eq!(nome_per("/home/x/Verbale.ODT", None), "Verbale.ODT");
    }
}
