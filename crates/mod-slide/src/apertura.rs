//! Aprire una presentazione dai suoi byte coi crate di DeckCraft, senza il motore: sono le righe
//! di `open_other` e `install_embedded_fonts` del motore rifatte qui (ricerca DeckCraft, sezione
//! 13), da riallineare quando si cambia il commit fissato.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use deckcraft_model::Presentation;

/// Il tetto di una presentazione da aprire, come per i PDF: l'import tiene in memoria tutto il
/// pacchetto decompresso, gli alberi XML e una copia di ogni immagine (ricerca DeckCraft,
/// sezione 2).
pub const LIMITE_MB: usize = 200;

const MB: u64 = 1024 * 1024;
/// I tetti del pacchetto decompresso, controllati prima di DeckCraft (`tascabile_files::archivio`,
/// audit del 30/10, problema 1): al più 10.000 parti; una parte di testo (gli XML, che il lettore
/// fa diventare alberi molte volte più grandi) al più 32 MB e 96 MB in tutto; una binaria
/// (immagini, video) al più 256 MB; tutto al più 320 MB; 2 milioni di elementi XML.
pub const TETTI: tascabile_files::archivio::Tetti = tascabile_files::archivio::Tetti {
    voci: 10_000,
    voce_testo: 32 * MB,
    voce_binaria: 256 * MB,
    totale_testo: 96 * MB,
    totale: 320 * MB,
    elementi: 2_000_000,
};

/// I modi in cui una presentazione non si apre. Nella diagnosi va solo `diagnosi()`, sullo
/// schermo il messaggio: tutti e due fissi, mai il messaggio del lettore, che può riportare nomi
/// di parti e pezzi del file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Genere {
    Vuoto,
    TroppoGrande,
    /// Il pacchetto decompresso supera i tetti (`TETTI`): costruito per gonfiarsi, o enorme.
    Gonfiata,
    /// Cifrata con la password di PowerPoint: DeckCraft non la decifra.
    Password,
    /// .ppt, .pps, .pot: il formato binario di PowerPoint di prima del 2007.
    PowerPointVecchio,
    /// .odp di LibreOffice e OpenOffice.
    LibreOffice,
    /// Un altro pacchetto di Office (un .docx, un .xlsx) o un file qualunque.
    NonPresentazione,
    /// Una presentazione che non si legge: troncata, rovinata.
    Illeggibile,
    PanicoDelLettore,
}

impl Genere {
    /// Il genere per la diagnosi.
    pub fn diagnosi(self) -> &'static str {
        match self {
            Genere::Vuoto => "file vuoto",
            Genere::TroppoGrande => "oltre 200 MB",
            Genere::Gonfiata => "oltre i tetti decompressa",
            Genere::Password => "protetta da password",
            Genere::PowerPointVecchio => "formato vecchio di PowerPoint",
            Genere::LibreOffice => "presentazione di LibreOffice",
            Genere::NonPresentazione => "non è una presentazione",
            Genere::Illeggibile => "presentazione illeggibile",
            Genere::PanicoDelLettore => "panico del lettore",
        }
    }

    /// Il messaggio per lo schermo, che segue «Non riesco ad aprire il file: ».
    pub fn messaggio(self) -> &'static str {
        match self {
            Genere::Vuoto => "il file è vuoto.",
            Genere::TroppoGrande => "la presentazione supera i 200 MB: in questa versione non si apre.",
            Genere::Gonfiata => {
                "decompressa, la presentazione supera i limiti di questa versione (10.000 parti, 32 MB di testo per parte, 320 MB in tutto): non si apre."
            }
            Genere::Password => "la presentazione è protetta da password, e qui non si può ancora aprire.",
            Genere::PowerPointVecchio => "è nel formato vecchio di PowerPoint (.ppt), che qui non si legge: va salvata come .pptx.",
            Genere::LibreOffice => "è una presentazione di LibreOffice (.odp), che qui non si legge: va salvata come .pptx.",
            Genere::NonPresentazione => "non è una presentazione.",
            Genere::Illeggibile => "non è una presentazione che si legge, forse è rovinata.",
            Genere::PanicoDelLettore => "il lettore delle presentazioni si è fermato con un errore interno.",
        }
    }
}

/// Una presentazione che non si apre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErroreApertura {
    pub genere: Genere,
}

impl std::fmt::Display for ErroreApertura {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.genere.messaggio())
    }
}

impl std::error::Error for ErroreApertura {}

impl From<Genere> for ErroreApertura {
    fn from(genere: Genere) -> Self {
        Self { genere }
    }
}

/// I punti per pollice dei disegni nel PDF delle slide: nitidi stampati, non troppo pesanti.
const DPI_DEL_PDF: f64 = 150.0;
/// L'A4 verticale in punti, per le pagine con le note.
const A4: (f64, f64) = (595.28, 841.89);

/// Un PDF delle slide che non si scrive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorePdf {
    /// Tutte le slide sono nascoste.
    NienteDaEsportare,
    /// La presentazione non ha slide.
    SenzaSlide,
    Scrittura,
    Panico,
}

impl ErrorePdf {
    /// Il genere per la diagnosi.
    pub fn diagnosi(self) -> &'static str {
        match self {
            ErrorePdf::NienteDaEsportare => "niente da esportare",
            ErrorePdf::SenzaSlide => "senza slide",
            ErrorePdf::Scrittura => "errore di scrittura",
            ErrorePdf::Panico => "panico dell'esportazione",
        }
    }
}

impl std::fmt::Display for ErrorePdf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ErrorePdf::NienteDaEsportare => "tutte le slide sono nascoste.",
            ErrorePdf::SenzaSlide => "la presentazione non ha slide.",
            ErrorePdf::Scrittura => "la scrittura del PDF non è riuscita.",
            ErrorePdf::Panico => "l'esportazione si è fermata con un errore interno.",
        })
    }
}

/// L'inizio di un contenitore OLE (CFB): i .ppt vecchi e i .pptx cifrati.
const CFB: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

/// Il file è un pacchetto di PowerPoint (.pptx, .ppsx, .potx), qualunque sia il nome: uno zip
/// con `ppt/presentation.xml`, la parte che PowerPoint, LibreOffice e Google scrivono sempre lì.
/// Serve a mandare al lettore delle Slide anche un «Apri con» senza nome; non dice se poi si apre.
/// Prima passa dal controllo dei tetti: `ZipArchive` prenota memoria per le voci che il file
/// dichiara.
pub fn sembra_presentazione(byte: &[u8]) -> bool {
    byte.starts_with(b"PK")
        && tascabile_files::archivio::controlla(byte, &TETTI).is_ok()
        && zip::ZipArchive::new(std::io::Cursor::new(byte)).is_ok_and(|z| z.index_for_name("ppt/presentation.xml").is_some())
}

/// Una presentazione aperta. Il modello sta in un `Arc`: il thread del disegno ne tiene una copia,
/// e una copia di `Presentazione` costa solo quella dell'`Arc`.
#[derive(Clone)]
pub struct Presentazione {
    doc: Arc<Presentation>,
    /// Le slide che il file elenca ma che non si sono potute leggere: solo quante, i nomi delle
    /// parti restano fuori dalla diagnosi.
    perse: usize,
}

impl Presentazione {
    /// Apre i byte di una presentazione. Un errore o un panico del lettore diventano `Err`, mai
    /// un'app che si chiude. Lavoro pesante (decomprime tutto il pacchetto): fuori dal thread
    /// dell'interfaccia. I font incorporati nel file entrano nel database dei font di DeckCraft,
    /// e ci restano finché l'app è aperta, come nel motore.
    pub fn apri(byte: &[u8]) -> Result<Presentazione, ErroreApertura> {
        if byte.is_empty() {
            return Err(Genere::Vuoto.into());
        }
        if byte.len() > LIMITE_MB * 1024 * 1024 {
            return Err(Genere::TroppoGrande.into());
        }
        // Ogni file passa prima dai tetti, poi da DeckCraft (anche il suo riconoscimento apre lo zip
        // con `ZipArchive`). Il controllo legge con lo stesso zip di DeckCraft: uno che non si apre
        // come zip non lo apre nemmeno DeckCraft, e il perché lo dice lui (revisione del 31/10).
        use tascabile_files::archivio::ErroreZip;
        match tascabile_files::archivio::controlla(byte, &TETTI) {
            Ok(()) | Err(ErroreZip::NonValido) => {}
            // Un archivio che si legge in due modi non va a DeckCraft (CVE-2026-115905).
            Err(ErroreZip::CompressioneIgnota | ErroreZip::Rovinato | ErroreZip::Incoerente) => return Err(Genere::Illeggibile.into()),
            Err(ErroreZip::TroppeVoci | ErroreZip::VoceTroppoGrande | ErroreZip::TroppoGrande | ErroreZip::TroppiElementi) => {
                return Err(Genere::Gonfiata.into());
            }
        }
        if deckcraft_pptx::sniff(byte) {
            let letto = catch_unwind(AssertUnwindSafe(|| {
                deckcraft_pptx::import_checked(byte).map(|(mut p, perse)| {
                    deckcraft_format::repair(&mut p);
                    (p, perse.len())
                })
            }));
            let (p, perse) = match letto {
                Ok(Ok(letto)) => letto,
                Ok(Err(deckcraft_pptx::PptxError::NotPptx(_))) => return Err(Genere::NonPresentazione.into()),
                Ok(Err(_)) => return Err(Genere::Illeggibile.into()),
                Err(_) => return Err(Genere::PanicoDelLettore.into()),
            };
            carica_i_font(&p);
            return Ok(Presentazione { doc: Arc::new(p), perse });
        }
        Err(Self::perche_no(byte).into())
    }

    /// Che cosa è un file che non è un pacchetto di PowerPoint.
    fn perche_no(byte: &[u8]) -> Genere {
        if deckcraft_pptx::is_encrypted(byte) {
            Genere::Password
        } else if byte.starts_with(&CFB) {
            Genere::PowerPointVecchio
        } else if e_odp(byte) {
            Genere::LibreOffice
        } else if byte.starts_with(b"PK") {
            // Uno zip senza [Content_Types].xml leggibile: di solito un .pptx scaricato a metà,
            // che ha perso la directory in fondo.
            Genere::Illeggibile
        } else {
            Genere::NonPresentazione
        }
    }

    /// Il modello di DeckCraft, da passare al disegno e alla presentazione.
    pub fn documento(&self) -> &Arc<Presentation> {
        &self.doc
    }

    /// Quante slide ha, nascoste comprese.
    pub fn slide(&self) -> usize {
        self.doc.slides.len()
    }

    /// Quante slide il file elencava ma non si sono potute leggere, e mancano.
    pub fn perse(&self) -> usize {
        self.perse
    }

    /// Il PDF della presentazione, da chiamare su un thread a parte (disegna ogni slide): una slide
    /// per pagina, grande come la slide, a 150 punti per pollice per i disegni e col testo vero
    /// sopra (si cerca e si copia); `con_note`, ogni slide su un A4 verticale con le sue note
    /// sotto. Le slide nascoste restano fuori. Nell'errore solo il genere, mai il messaggio di
    /// DeckCraft.
    pub fn pdf(&self, con_note: bool) -> Result<Vec<u8>, ErrorePdf> {
        if self.doc.slides.is_empty() {
            return Err(ErrorePdf::SenzaSlide);
        }
        let opzioni = deckcraft_pdf::PdfOptions {
            layout: if con_note { deckcraft_pdf::PageLayout::Notes } else { deckcraft_pdf::PageLayout::Slides },
            dpi: DPI_DEL_PDF,
            paper: A4,
            ..Default::default()
        };
        match catch_unwind(AssertUnwindSafe(|| deckcraft_pdf::export(&self.doc, &opzioni))) {
            Ok(Ok(byte)) => Ok(byte),
            Ok(Err(deckcraft_pdf::PdfError::NoPages)) => Err(ErrorePdf::NienteDaEsportare),
            Ok(Err(deckcraft_pdf::PdfError::Write(_))) => Err(ErrorePdf::Scrittura),
            Err(_) => Err(ErrorePdf::Panico),
        }
    }

    /// La prima slide che si presenta: la prima non nascosta (o quella della presentazione
    /// personalizzata del file).
    pub fn prima_da_presentare(&self) -> usize {
        deckcraft_anim::show_order(&self.doc).first().copied().unwrap_or(0)
    }

    /// Le note per il relatore della slide `indice`, in testo semplice coi paragrafi a capo; vuote
    /// se non ce ne sono (o se la slide non c'è).
    pub fn note(&self, indice: usize) -> String {
        self.doc.slides.get(indice).map(|s| s.notes.text()).unwrap_or_default()
    }

    /// Disegna la slide `indice` (da 0) a `scala` pixel per punto, su questo thread: lavoro da
    /// 10 a 80 ms sul telefono, fuori dal thread dell'interfaccia. Un panico dentro il disegno dà
    /// una slide bianca, mai un'app che si chiude (`render_slide`, ricerca DeckCraft, sezione 3).
    pub fn disegna(&self, indice: usize, scala: f64) -> Immagine {
        let opzioni = deckcraft_render::RenderOpts { scale: scala, ..Default::default() };
        let img = deckcraft_render::render_slide(&self.doc, indice, &opzioni);
        Immagine { larghezza: img.width, altezza: img.height, pixel: img.pixels }
    }
}

/// Una slide disegnata: RGBA8 premoltiplicato, come `egui::Color32`.
pub struct Immagine {
    pub larghezza: u32,
    pub altezza: u32,
    pub pixel: Vec<u8>,
}

/// Un .odp: lo zip di OpenDocument comincia con la parte `mimetype` non compressa, e il suo
/// contenuto subito dopo il nome (OpenDocument 1.3, parte 2, 3.3).
fn e_odp(byte: &[u8]) -> bool {
    const TIPO: &[u8] = b"mimetypeapplication/vnd.oasis.opendocument.presentation";
    byte.starts_with(b"PK") && byte.get(30..30 + TIPO.len()) == Some(TIPO)
}

/// I font incorporati nel file nel database dei font, così il testo in quella famiglia si
/// disegna col font del file: `install_embedded_fonts` del motore (`crates/engine/src/fonts.rs`).
/// Quelli MicroType Express non hanno un TrueType dentro e restano fuori. Il database è uno solo
/// per tutta l'app e non si svuota: entra solo un font che non può cambiare come si vedono le
/// presentazioni aperte dopo (`font_ammesso`).
fn carica_i_font(p: &Presentation) {
    let db = deckcraft_fonts::FontDb::global();
    for font in &p.embedded_fonts {
        for id in font.faces().into_iter().flatten() {
            let Some(parte) = p.media(id) else { continue };
            let Some(sfnt) = deckcraft_fonts::embedded_sfnt(&parte.data) else { continue };
            if font_ammesso(sfnt, db) {
                db.add_font(sfnt.to_vec());
            }
        }
    }
}

/// Le famiglie che i documenti chiedono di solito e quelle che DeckCraft usa al loro posto: le
/// chiavi e i sostituti di `deckcraft_fonts::substitutes` (che si chiede anche a ogni nome, per
/// quelle aggiunte dopo), Calibri (`MISSING_FONT_FAMILY`), con cui si disegnano tutte le famiglie
/// che mancano, e i ripieghi.
const FAMIGLIE_COMUNI: &[&str] = &[
    "Calibri",
    "Calibri Light",
    "Cambria",
    "Cambria Math",
    "Arial",
    "Arial Nova",
    "Arial Black",
    "Arial Narrow",
    "Helvetica",
    "Helvetica Neue",
    "Times New Roman",
    "Times",
    "Courier New",
    "Courier",
    "Consolas",
    "Menlo",
    "Aptos",
    "Aptos Display",
    "Aptos Narrow",
    "Segoe UI",
    "Segoe UI Light",
    "Segoe UI Semibold",
    "Tahoma",
    "Verdana",
    "Georgia",
    "Garamond",
    "Book Antiqua",
    "Palatino",
    "Palatino Linotype",
    "Constantia",
    "Century Gothic",
    "Avenir",
    "Avenir Next",
    "Futura",
    "Gill Sans",
    "Gill Sans MT",
    "Trebuchet MS",
    "Corbel",
    "Candara",
    "Franklin Gothic",
    "Franklin Gothic Book",
    "Franklin Gothic Medium",
    "Univers",
    "Open Sans",
    "Lato",
    "Roboto",
    "Montserrat",
    "Poppins",
    "Nunito Sans",
    "Playfair Display",
    "Merriweather",
    "Source Serif 4",
    "Source Serif Pro",
    "Source Sans 3",
    "Source Sans Pro",
    "Impact",
    "Carlito",
    "Caladea",
    "Liberation Sans",
    "Liberation Serif",
    "Liberation Mono",
    "Inter",
    "Ubuntu",
    "DejaVu Sans",
    "Noto Sans",
    "Symbol",
    "Wingdings",
];

/// Un font incorporato entra nel database se porta solo famiglie nuove, che non sono già nel
/// database (incorporate da una presentazione di prima, o del sistema) e non sono famiglie comuni,
/// nemmeno come nome vecchio del font: un file ostile con un falso «Calibri», con le cifre
/// disegnate scambiate, cambierebbe i numeri di tutte le presentazioni aperte dopo (audit del
/// 30/10, problema 1). Le famiglie si leggono come le legge DeckCraft, in un database di prova.
fn font_ammesso(sfnt: &[u8], db: &deckcraft_fonts::FontDb) -> bool {
    let comune = |f: &str| {
        FAMIGLIE_COMUNI.iter().any(|c| c.eq_ignore_ascii_case(f.trim()))
            || f.trim().eq_ignore_ascii_case(deckcraft_fonts::MISSING_FONT_FAMILY)
            || !deckcraft_fonts::substitutes(f.trim()).is_empty()
    };
    let prova = deckcraft_fonts::FontDb::with_font_dirs(Vec::new());
    let prima: Vec<String> = prova.families();
    let comuni_prima: Vec<bool> = FAMIGLIE_COMUNI.iter().map(|c| prova.has_family(c)).collect();
    if prova.add_font(sfnt.to_vec()) == 0 {
        return false;
    }
    let nuove: Vec<String> = prova.families().into_iter().filter(|f| !prima.iter().any(|p| p.eq_ignore_ascii_case(f))).collect();
    let prende_un_nome_comune = FAMIGLIE_COMUNI.iter().zip(&comuni_prima).any(|(c, prima)| !prima && prova.has_family(c));
    !nuove.is_empty() && !prende_un_nome_comune && nuove.iter().all(|f| !comune(f) && !db.has_family(f))
}

#[cfg(test)]
pub(crate) mod test {
    use super::*;
    use std::io::{Read, Write};

    pub(crate) const LEZIONE: &[u8] = include_bytes!("../tests/fixtures/lezione.pptx");

    /// Uno zip con queste parti, compresse.
    fn zip_con(parti: &[(&str, &[u8])]) -> Vec<u8> {
        let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (nome, byte) in parti {
            z.start_file(*nome, zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(byte).unwrap();
        }
        z.finish().unwrap().into_inner()
    }

    /// La lezione con una parte cambiata.
    fn lezione_con(nome: &str, nuova: &[u8]) -> Vec<u8> {
        let mut a = zip::ZipArchive::new(std::io::Cursor::new(LEZIONE)).unwrap();
        let mut parti = Vec::new();
        for i in 0..a.len() {
            let mut f = a.by_index(i).unwrap();
            let mut byte = Vec::new();
            f.read_to_end(&mut byte).unwrap();
            let nome_voce = f.name().unwrap().into_owned();
            let byte = if nome_voce == nome { nuova.to_vec() } else { byte };
            parti.push((nome_voce, byte));
        }
        let parti: Vec<(&str, &[u8])> = parti.iter().map(|(n, b)| (n.as_str(), b.as_slice())).collect();
        zip_con(&parti)
    }

    /// Un contenitore OLE finto: l'intestazione, un settore vuoto e, se serve, il nome dello
    /// stream della cifratura in UTF-16, dove `is_encrypted` lo cerca quando la directory non si
    /// legge.
    fn cfb(cifrato: bool) -> Vec<u8> {
        let mut byte = CFB.to_vec();
        byte.resize(1024, 0);
        if cifrato {
            byte.extend("EncryptedPackage".encode_utf16().flat_map(u16::to_le_bytes));
        }
        byte
    }

    fn genere(byte: &[u8]) -> Genere {
        Presentazione::apri(byte).err().expect("non deve aprirsi").genere
    }

    /// La lezione con un font incorporato (ECMA-376, 19.2.1.9): la parte del font, la sua
    /// relazione, il tipo e l'elenco nella presentazione.
    fn lezione_col_font(font: &[u8]) -> Vec<u8> {
        let mut a = zip::ZipArchive::new(std::io::Cursor::new(LEZIONE)).unwrap();
        let mut parti: Vec<(String, Vec<u8>)> = Vec::new();
        for i in 0..a.len() {
            let mut f = a.by_index(i).unwrap();
            let mut byte = Vec::new();
            f.read_to_end(&mut byte).unwrap();
            let nome = f.name().unwrap().into_owned();
            let testo = || String::from_utf8(byte.clone()).unwrap();
            let byte = match nome.as_str() {
                "ppt/presentation.xml" => {
                    let t = testo();
                    let dopo = t.find("<p:notesSz").map(|i| i + t[i..].find("/>").unwrap() + 2).unwrap();
                    let lista = r#"<p:embeddedFontLst><p:embeddedFont><p:font typeface="Prova"/><p:regular r:id="rIdFontProva"/></p:embeddedFont></p:embeddedFontLst>"#;
                    format!("{}{lista}{}", &t[..dopo], &t[dopo..]).into_bytes()
                }
                "ppt/_rels/presentation.xml.rels" => testo()
                    .replace(
                        "</Relationships>",
                        r#"<Relationship Id="rIdFontProva" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/font" Target="fonts/font1.fntdata"/></Relationships>"#,
                    )
                    .into_bytes(),
                "[Content_Types].xml" => testo()
                    .replacen("<Default ", r#"<Default Extension="fntdata" ContentType="application/x-fontdata"/><Default "#, 1)
                    .into_bytes(),
                _ => byte,
            };
            parti.push((nome, byte));
        }
        parti.push(("ppt/fonts/font1.fntdata".into(), font.to_vec()));
        let parti: Vec<(&str, &[u8])> = parti.iter().map(|(n, b)| (n.as_str(), b.as_slice())).collect();
        zip_con(&parti)
    }

    /// Carlito con un altro nome di famiglia (di sette lettere, come «Carlito»: le misure della
    /// tabella dei nomi non cambiano).
    fn carlito_chiamato(nome: &str) -> Vec<u8> {
        assert_eq!(nome.len(), 7);
        let byte = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/fonts/carlito/Carlito-Regular.ttf")).unwrap();
        let (vecchio, nuovo): (Vec<u8>, Vec<u8>) =
            ("Carlito".encode_utf16().flat_map(u16::to_be_bytes).collect(), nome.encode_utf16().flat_map(u16::to_be_bytes).collect());
        let mut fuori = byte.clone();
        let mut i = 0;
        while let Some(j) = fuori[i..].windows(vecchio.len()).position(|w| w == vecchio.as_slice()) {
            fuori[i + j..i + j + nuovo.len()].copy_from_slice(&nuovo);
            i += j + nuovo.len();
        }
        assert_ne!(fuori, byte);
        fuori
    }

    /// Un font incorporato con il nome di una famiglia che i documenti chiedono di solito (qui
    /// Calibri, con cui DeckCraft disegna anche le famiglie che mancano) non entra nel database
    /// dei font, che è uno per tutta l'app: cambierebbe come si vedono le presentazioni aperte
    /// dopo (audit del 30/10, problema 1). Uno col suo nome entra, e la presentazione si apre.
    #[test]
    fn un_font_incorporato_non_cambia_le_presentazioni_dopo() {
        let db = deckcraft_fonts::FontDb::global();
        assert_eq!(db.document_face("Calibri", "Regular").family, "Carlito");
        let p = Presentazione::apri(&lezione_col_font(&carlito_chiamato("Calibri"))).expect("si apre");
        assert_eq!(p.doc.embedded_fonts.len(), 1, "la prova non ha incorporato il font");
        assert_eq!(db.document_face("Calibri", "Regular").family, "Carlito", "il font incorporato ha preso il posto di Calibri");
        assert!(!db.has_family("Pippoli"));
        Presentazione::apri(&lezione_col_font(&carlito_chiamato("Pippoli"))).expect("si apre");
        assert!(db.has_family("Pippoli"), "un font col suo nome deve entrare");
    }

    /// Una presentazione costruita per gonfiarsi (una slide di 33 MB di XML in pochi KB) non arriva
    /// a DeckCraft: lo stesso controllo dei documenti Word, coi tetti delle presentazioni.
    #[test]
    fn una_presentazione_che_si_gonfia_non_si_apre() {
        let mut slide = vec![b' '; 33 * 1024 * 1024];
        slide[0] = b'<';
        let bomba = lezione_con("ppt/slides/slide1.xml", &slide);
        assert!(bomba.len() < 1024 * 1024, "compressa è piccola: {}", bomba.len());
        assert_eq!(genere(&bomba), Genere::Gonfiata);
        assert!(!sembra_presentazione(&bomba));
        assert_eq!(Genere::Gonfiata.diagnosi(), "oltre i tetti decompressa");
        // Con byte davanti il vecchio controllo diceva «non valido» e la lasciava a DeckCraft, che
        // trova da solo dove comincia l'archivio (revisione del 31/10, punto 1).
        let mut davanti = b"PK\x07\x08 qualcosa davanti".to_vec();
        davanti.extend(&bomba);
        assert_eq!(genere(&davanti), Genere::Gonfiata);
    }

    #[test]
    fn la_lezione_si_apre_con_tutte_le_slide() {
        let p = Presentazione::apri(LEZIONE).unwrap();
        assert_eq!((p.slide(), p.perse()), (4, 0));
        assert!(sembra_presentazione(LEZIONE));
    }

    #[test]
    fn le_note_di_ogni_slide() {
        let p = Presentazione::apri(LEZIONE).unwrap();
        assert_eq!(p.note(1), "Nota per il relatore");
        assert_eq!(p.note(0), "");
        assert_eq!(p.note(99), "");
    }

    #[test]
    fn il_pdf_ha_le_slide_da_presentare_e_quello_con_le_note_le_pagine_a4() {
        let p = Presentazione::apri(LEZIONE).unwrap();
        // Le slide: tre pagine (la nascosta resta fuori), grandi come la slide.
        let pdf = tascabile_pdf::Pdf::apri(p.pdf(false).unwrap(), None).unwrap();
        assert_eq!(pdf.pagine(), 3);
        let m = pdf.misura(0).unwrap();
        assert_eq!((m.larghezza.round(), m.altezza.round()), (960.0, 540.0));
        // Con le note: le stesse tre slide, ognuna su un A4 verticale con le sue note sotto.
        let pdf = tascabile_pdf::Pdf::apri(p.pdf(true).unwrap(), None).unwrap();
        assert_eq!(pdf.pagine(), 3);
        let m = pdf.misura(1).unwrap();
        assert_eq!((m.larghezza.round(), m.altezza.round()), (595.0, 842.0));
    }

    #[test]
    fn una_slide_rovinata_manca_e_si_conta() {
        let byte = lezione_con("ppt/slides/slide3.xml", b"<p:sld><non chiuso");
        let p = Presentazione::apri(&byte).unwrap();
        assert_eq!((p.slide(), p.perse()), (3, 1));
    }

    #[test]
    fn ogni_file_sbagliato_ha_il_suo_genere() {
        assert_eq!(genere(b""), Genere::Vuoto);
        assert_eq!(genere(&cfb(true)), Genere::Password);
        assert_eq!(genere(&cfb(false)), Genere::PowerPointVecchio);
        let mut odp = Vec::new();
        {
            let mut z = zip::ZipWriter::new(std::io::Cursor::new(&mut odp));
            let senza = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            z.start_file("mimetype", senza).unwrap();
            z.write_all(b"application/vnd.oasis.opendocument.presentation").unwrap();
            z.finish().unwrap();
        }
        assert_eq!(genere(&odp), Genere::LibreOffice);
        let docx = zip_con(&[("[Content_Types].xml", b"<Types/>".as_slice()), ("word/document.xml", b"<w:document/>".as_slice())]);
        assert_eq!(genere(&docx), Genere::NonPresentazione);
        assert!(!sembra_presentazione(&docx));
        assert_eq!(genere(b"%PDF-1.7 un pdf"), Genere::NonPresentazione);
        // Scaricata a metà: lo zip ha perso la sua directory, in fondo.
        assert_eq!(genere(&LEZIONE[..LEZIONE.len() / 2]), Genere::Illeggibile);
    }

    #[test]
    fn la_prima_slide_si_disegna_a_due_pixel_per_punto() {
        let img = Presentazione::apri(LEZIONE).unwrap().disegna(0, 2.0);
        assert_eq!((img.larghezza, img.altezza, img.pixel.len()), (1920, 1080, 1920 * 1080 * 4));
        // Bianca nell'angolo, e il titolo scuro nella fascia del suo segnaposto (da 88 a 276 punti).
        assert_eq!(&img.pixel[..4], &[255, 255, 255, 255]);
        let fascia = &img.pixel[176 * 1920 * 4..552 * 1920 * 4];
        let scuri = fascia.as_chunks::<4>().0.iter().filter(|p| p[0] < 100 && p[3] == 255).count();
        assert!(scuri > 500, "il titolo non c'è: {scuri} pixel scuri");
    }

    #[test]
    fn le_presentazioni_dei_casi_limite_si_aprono_e_si_esportano() {
        use crate::prova::{QUATTRO_TERZI, SENZA_SLIDE, TUTTE_NASCOSTE, VERTICALE};
        let misura = |byte: Vec<u8>, pagina: usize| {
            let pdf = tascabile_pdf::Pdf::apri(byte, None).unwrap();
            let m = pdf.misura(pagina).unwrap();
            (pdf.pagine(), m.larghezza.round(), m.altezza.round())
        };
        // 4:3 e in verticale: il PDF ha le loro misure, e con le note un A4.
        let p = Presentazione::apri(QUATTRO_TERZI).unwrap();
        assert_eq!(p.slide(), 2);
        assert_eq!(misura(p.pdf(false).unwrap(), 0), (2, 720.0, 540.0));
        let p = Presentazione::apri(VERTICALE).unwrap();
        assert_eq!(misura(p.pdf(false).unwrap(), 0), (1, 540.0, 960.0));
        assert_eq!(misura(p.pdf(true).unwrap(), 0), (1, 595.0, 842.0));
        let img = p.disegna(0, 0.5);
        assert_eq!((img.larghezza, img.altezza), (270, 480));
        // Tutte nascoste: si leggono, ma il PDF non ha niente da mettere e lo dice.
        let p = Presentazione::apri(TUTTE_NASCOSTE).unwrap();
        assert_eq!(p.slide(), 2);
        assert_eq!(p.pdf(false).err(), Some(ErrorePdf::NienteDaEsportare));
        assert_eq!(p.pdf(true).err(), Some(ErrorePdf::NienteDaEsportare));
        assert_eq!(p.prima_da_presentare(), 0);
        // Senza slide: si apre, niente note, niente PDF, e l'errore non parla di slide nascoste.
        let p = Presentazione::apri(SENZA_SLIDE).unwrap();
        assert_eq!((p.slide(), p.perse()), (0, 0));
        assert_eq!(p.note(0), "");
        assert_eq!(p.pdf(false).err(), Some(ErrorePdf::SenzaSlide));
        assert_eq!(ErrorePdf::SenzaSlide.to_string(), "la presentazione non ha slide.");
        assert_eq!(p.prima_da_presentare(), 0);
    }

    #[test]
    fn oltre_il_tetto_non_si_apre() {
        let grande = vec![0u8; LIMITE_MB * 1024 * 1024 + 1];
        assert_eq!(genere(&grande), Genere::TroppoGrande);
    }
}
