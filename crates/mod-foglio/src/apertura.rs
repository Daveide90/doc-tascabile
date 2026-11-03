//! Aprire un foglio di calcolo dai suoi byte: il genere dal contenuto (`gridcraft_xlsx::sniff`),
//! i tetti per un telefono prima del lettore, il lettore giusto, le schede da mostrare.

use std::sync::Arc;

use gridcraft_model::{Visibility, Workbook};
use gridcraft_xlsx::{CsvOptions, Format};

/// Il tetto di un foglio da aprire. Il lettore tiene tutto in memoria, cella per cella
/// (ricerca, sezione 11): un foglio di 40 MB compressi ha già milioni di celle.
pub const LIMITE_MB: usize = 40;
/// Un CSV si decodifica tutto e non ha tetti suoi: più basso.
const LIMITE_CSV_MB: usize = 20;
/// Le celle al più, in tutti i fogli: oltre, sul telefono la memoria non basta.
const CELLE_MASSIME: usize = 2_000_000;

const MB: u64 = 1024 * 1024;
/// I tetti del pacchetto decompresso (xlsx, xlsb, ods), controllati prima del lettore come per le
/// Slide: al più 10.000 parti, una parte di testo 48 MB e 128 MB in tutto, 256 MB in tutto,
/// 2 milioni di elementi XML (circa mezzo milione di celle).
pub const TETTI: tascabile_files::archivio::Tetti = tascabile_files::archivio::Tetti {
    voci: 10_000,
    voce_testo: 48 * MB,
    voce_binaria: 128 * MB,
    totale_testo: 128 * MB,
    totale: 256 * MB,
    elementi: 2_000_000,
};

/// I modi in cui un foglio non si apre. Nella diagnosi va `diagnosi()`, sullo schermo
/// `messaggio()`: tutti e due fissi, mai il messaggio del lettore, che può riportare nomi e pezzi
/// del file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Genere {
    Vuoto,
    TroppoGrande,
    /// Il pacchetto decompresso supera i tetti, o le celle sono troppe.
    Gonfiato,
    /// Protetto da password: non si decifra ancora.
    Password,
    /// Un altro file di Office (un .doc, un .ppt) o un file qualunque.
    NonFoglio,
    /// Un foglio che non si legge: troncato, rovinato, un .xls di Excel 5.
    Illeggibile,
    /// Nessun foglio da mostrare (tutti nascosti).
    SenzaFogli,
    /// Una pagina web salvata col nome di un foglio: tanti portali esportano così l'«Excel».
    PaginaWeb,
}

impl Genere {
    pub fn diagnosi(self) -> &'static str {
        match self {
            Genere::Vuoto => "file vuoto",
            Genere::TroppoGrande => "oltre il tetto",
            Genere::Gonfiato => "oltre i tetti decompresso",
            Genere::Password => "protetto da password",
            Genere::NonFoglio => "non è un foglio di calcolo",
            Genere::Illeggibile => "illeggibile",
            Genere::SenzaFogli => "senza fogli da mostrare",
            Genere::PaginaWeb => "pagina web col nome di un foglio",
        }
    }

    pub fn messaggio(self) -> &'static str {
        match self {
            Genere::Vuoto => "il file è vuoto.",
            Genere::TroppoGrande => "il foglio di calcolo è troppo grande per il telefono.",
            Genere::Gonfiato => "il foglio di calcolo, aperto, sarebbe troppo grande per il telefono.",
            Genere::Password => "il foglio di calcolo è protetto da password: qui non si apre ancora.",
            Genere::NonFoglio => "non è un foglio di calcolo che so leggere.",
            Genere::Illeggibile => "il foglio di calcolo è rovinato o di un formato troppo vecchio.",
            Genere::SenzaFogli => "il foglio di calcolo ha solo fogli nascosti.",
            Genere::PaginaWeb => "è una pagina web salvata come foglio di calcolo: aprila col browser.",
        }
    }
}

impl std::fmt::Display for Genere {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.messaggio())
    }
}

/// Un foglio di calcolo aperto: la cartella di GridCraft, i fogli da mostrare (gli indici di
/// quelli visibili, nell'ordine del file), quello da cui partire e se il lettore ha avvisato di
/// parti che non ha letto.
#[derive(Clone)]
pub struct Cartella {
    pub cartella: Arc<Workbook>,
    pub schede: Vec<usize>,
    pub iniziale: usize,
    pub avvisi: usize,
    /// Qualche formula a matrice: il lettore tiene solo il valore della prima cella.
    pub matrici: bool,
}

/// Senza il contenuto: nella diagnosi niente nomi di fogli né celle.
impl std::fmt::Debug for Cartella {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cartella").field("fogli", &self.cartella.sheets.len()).field("schede", &self.schede.len()).field("avvisi", &self.avvisi).finish()
    }
}

impl Cartella {
    /// Il foglio di calcolo dai suoi byte. Il CSV resta testo: «1,500» non diventa 1500.
    pub fn apri(byte: &[u8]) -> Result<Cartella, Genere> {
        if byte.is_empty() {
            return Err(Genere::Vuoto);
        }
        if byte.len() > LIMITE_MB * 1024 * 1024 {
            return Err(Genere::TroppoGrande);
        }
        let formato = gridcraft_xlsx::sniff(byte);
        if matches!(formato, Format::Xlsx | Format::Xlsb | Format::Ods) {
            tascabile_files::archivio::controlla(byte, &TETTI).map_err(|e| match e {
                tascabile_files::archivio::ErroreZip::NonValido | tascabile_files::archivio::ErroreZip::Rovinato => Genere::Illeggibile,
                _ => Genere::Gonfiato,
            })?;
        }
        let letto = match formato {
            Format::Xlsx => gridcraft_xlsx::read_xlsx(byte),
            Format::Xlsb => gridcraft_xlsx::read_xlsb(byte),
            Format::Ods => gridcraft_xlsx::read_ods(byte),
            Format::Xls => gridcraft_xlsx::read_xls(byte),
            Format::Csv if byte.len() > LIMITE_CSV_MB * 1024 * 1024 => return Err(Genere::TroppoGrande),
            Format::Csv if campi_del_csv(byte) > CELLE_MASSIME => return Err(Genere::Gonfiato),
            Format::Csv if pagina_web(byte) => return Err(Genere::PaginaWeb),
            Format::Csv => gridcraft_xlsx::read_csv_with_report(byte, &CsvOptions { delimiter: 0, parse_values: false, ..CsvOptions::default() }),
            Format::Encrypted => return Err(Genere::Password),
            Format::LegacyBinary | Format::Unknown => return Err(Genere::NonFoglio),
        };
        let (cartella, avvisi) = letto.map_err(|e| match e {
            gridcraft_xlsx::IoError::TooLarge(_) => Genere::Gonfiato,
            _ => Genere::Illeggibile,
        })?;
        let celle: usize = cartella.sheets.iter().map(|s| s.cells.len()).sum();
        if celle > CELLE_MASSIME {
            return Err(Genere::Gonfiato);
        }
        let schede: Vec<usize> = (0..cartella.sheets.len()).filter(|i| cartella.sheet(*i).is_some_and(|s| s.visibility == Visibility::Visible)).collect();
        if schede.is_empty() {
            return Err(Genere::SenzaFogli);
        }
        let attiva = cartella.active_sheet();
        let iniziale = schede.iter().position(|s| *s == attiva).unwrap_or(0);
        let matrici = cartella.sheets.iter().any(|s| s.cells.iter().any(|(_, c)| c.formula.as_ref().is_some_and(|f| f.array.is_some() || f.dynamic)));
        Ok(Cartella { cartella: Arc::new(cartella), schede, iniziale, avvisi: avvisi.warnings.len(), matrici })
    }

    /// Il nome della scheda `i` (fra quelle da mostrare).
    pub fn nome_scheda(&self, i: usize) -> &str {
        self.schede.get(i).and_then(|s| self.cartella.sheet(*s)).map_or("", |s| s.name.as_str())
    }
}

/// Una pagina web e non un testo separato da virgole: comincia con «<» e ha una tabella o
/// l'intestazione di una pagina (revisione del 3/11: gestionali e portali esportano l'«Excel»
/// così, e si leggeva come CSV coi tag nelle celle).
fn pagina_web(byte: &[u8]) -> bool {
    let inizio = byte.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(byte);
    let testa = String::from_utf8_lossy(&inizio[..inizio.len().min(64 * 1024)]).to_ascii_lowercase();
    testa.trim_start().starts_with('<') && ["<table", "<html", "<!doctype html"].iter().any(|t| testa.contains(t))
}

/// I campi non vuoti di un CSV, al più: il lettore crea una cella per campo e il tetto delle celle
/// si controlla solo dopo, quando un CSV da 20 MB di virgole ne ha già fatte dieci milioni, un
/// GB (revisione del 3/11, B2). Si contano sui byte per ognuno dei separatori che il lettore può
/// scegliere (virgola, punto e virgola, tabulazione), e vale il conto più alto. Le virgolette come
/// le legge il lettore: aprono solo all'inizio di un campo. In UTF-16 una «"» può essere metà di
/// un altro carattere: lì le virgolette non si guardano, e il conto può solo crescere.
fn campi_del_csv(byte: &[u8]) -> usize {
    let utf16 = byte.starts_with(&[0xFF, 0xFE]) || byte.starts_with(&[0xFE, 0xFF]);
    let campi = |separatore: u8| {
        let (mut n, mut pieno, mut tra_virgolette, mut gia_tra_virgolette) = (0usize, false, false, false);
        let mut i = 0;
        while let Some(&c) = byte.get(i) {
            i += 1;
            if tra_virgolette {
                if c == b'"' && byte.get(i) == Some(&b'"') {
                    i += 1;
                    pieno = true;
                } else if c == b'"' {
                    tra_virgolette = false;
                } else {
                    pieno = true;
                }
            } else if c == b'"' && !utf16 && !pieno && !gia_tra_virgolette {
                (tra_virgolette, gia_tra_virgolette) = (true, true);
            } else if c == separatore || c == b'\r' || c == b'\n' {
                if c == b'\r' && byte.get(i) == Some(&b'\n') {
                    i += 1;
                }
                n += usize::from(pieno);
                (pieno, gia_tra_virgolette) = (false, false);
            } else {
                pieno = true;
            }
        }
        n + usize::from(pieno)
    };
    b",;\t".iter().copied().map(campi).max().unwrap_or(0)
}

/// I byte sono un foglio di calcolo anche senza il nome giusto: un pacchetto con
/// `xl/workbook.xml` (o `.bin`), un OpenDocument di fogli, un .xls. Uno zip qualunque no: anche
/// un .docx è uno zip. Uno zip con più voci dichiarate dei tetti no, prima di aprirlo: il
/// riconoscimento leggerebbe tutta la directory, fino a tre volte (revisione del 3/11, B3).
pub fn sembra_foglio(byte: &[u8]) -> bool {
    if byte.starts_with(b"PK") && tascabile_files::archivio::voci_dichiarate(byte, TETTI.voci).is_err() {
        return false;
    }
    match gridcraft_xlsx::sniff(byte) {
        Format::Xls => true,
        Format::Xlsx | Format::Xlsb | Format::Ods => {
            let Ok(mut z) = zip::ZipArchive::new(std::io::Cursor::new(byte)) else { return false };
            if z.file_names().any(|n| n == "xl/workbook.xml" || n == "xl/workbook.bin") {
                return true;
            }
            let Ok(mut tipo) = z.by_name("mimetype") else { return false };
            let mut t = String::new();
            std::io::Read::read_to_string(&mut std::io::Read::take(&mut tipo, 200), &mut t).is_ok() && t.trim() == "application/vnd.oasis.opendocument.spreadsheet"
        }
        _ => false,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn un_foglio_di_excel_si_apre_con_le_sue_schede() {
        let c = Cartella::apri(&crate::prova::cartella_di_prova()).unwrap();
        // Il foglio nascosto non è fra le schede.
        assert_eq!(c.schede.len(), 2);
        assert_eq!((c.nome_scheda(0), c.nome_scheda(1)), ("Pazienti", "Turni"));
        assert!(sembra_foglio(&crate::prova::cartella_di_prova()));
        assert!(format!("{c:?}").contains("schede: 2") && !format!("{c:?}").contains("Pazienti"));
    }

    #[test]
    fn i_file_che_non_vanno_lo_dicono() {
        assert_eq!(Cartella::apri(b"").unwrap_err(), Genere::Vuoto);
        assert!(!Cartella::apri(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1, 0, 0]).unwrap_err().diagnosi().is_empty());
        // Un docx non è un foglio.
        let mut docx = Vec::new();
        {
            let mut z = zip::ZipWriter::new(std::io::Cursor::new(&mut docx));
            z.start_file("word/document.xml", zip::write::SimpleFileOptions::default()).unwrap();
            std::io::Write::write_all(&mut z, b"<w:document/>").unwrap();
            z.finish().unwrap();
        }
        assert!(!sembra_foglio(&docx));
        assert!(Cartella::apri(&docx).is_err());
        // Un CSV resta testo: «1,500» non diventa 1500.
        let c = Cartella::apri(b"Nome;Dose\nRossi;1,500\n").unwrap();
        let foglio = c.cartella.sheet(0).unwrap();
        assert_eq!(foglio.value(gridcraft_core::CellRef::new(1, 1)), gridcraft_core::Value::Text("1,500".into()));
        assert!(!sembra_foglio(b"Nome;Dose\n"));
        // Una pagina web col nome .xls lo dice; un CSV con un «<» no.
        let html = b"\n  <html><body><table><tr><td>Rossi</td></tr></table></body></html>";
        assert_eq!(Cartella::apri(html).unwrap_err(), Genere::PaginaWeb);
        assert!(Cartella::apri(b"Valore;Nota\n<5;basso\n").is_ok());
    }

    #[test]
    fn un_csv_con_troppi_campi_si_ferma_prima_del_lettore() {
        // Tre milioni di campi in 6 MB: oltre il tetto delle celle, senza crearle.
        let virgole = b"a,".repeat(3_000_000);
        assert_eq!(campi_del_csv(&virgole), 3_000_000);
        let inizio = std::time::Instant::now();
        assert_eq!(Cartella::apri(&virgole).unwrap_err(), Genere::Gonfiato);
        assert!(inizio.elapsed() < std::time::Duration::from_secs(2));
        // Una virgoletta in mezzo al primo campo non apre le virgolette, come nel lettore.
        let mut furbo = b"x\"".to_vec();
        furbo.extend(b"a,".repeat(10));
        assert_eq!(campi_del_csv(&furbo), 10);
        // Le virgolette all'inizio di un campo sì, con «""» dentro; i campi vuoti non contano.
        assert_eq!(campi_del_csv(b"\"a,\"\"b\";c;;\r\nd\n"), 3);
        assert_eq!(campi_del_csv(b"1,5;2,5;3,5\n"), 4);
        // In UTF-16 le virgolette non si guardano.
        assert_eq!(campi_del_csv(&[0xFF, 0xFE, b'"', 0, b',', 0, b'a', 0]), 2);
    }
}
