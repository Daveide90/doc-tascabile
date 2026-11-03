//! I documenti che sono archivi zip (docx, docm, dotx, dotm, odt), guardati prima di darli al
//! motore: un archivio gonfiato apposta non deve finire la memoria del telefono, e i font
//! incorporati in un documento non devono restare nel processo per i documenti aperti dopo
//! (rapporto di sicurezza del 24/10, problemi 1 e 2). Il controllo degli archivi è nel crate dei
//! file (`tascabile_files::archivio`), con i tetti dei documenti Word; qui la tabella dei font.

use std::borrow::Cow;
use std::io::{Read, Write};

use tascabile_files::archivio::Tetti;
pub use tascabile_files::archivio::{ErroreZip, sembra_zip};

const MB: u64 = 1024 * 1024;

/// I tetti di un documento: al più 2.000 voci, 32 MB decompressi per voce e 64 MB in tutto, 2
/// milioni di elementi XML (un documento di mille pagine ne ha qualche centinaio di migliaia).
pub const TETTI: Tetti =
    Tetti { voci: 2_000, voce_testo: 32 * MB, voce_binaria: 32 * MB, totale_testo: 64 * MB, totale: 64 * MB, elementi: 2_000_000 };

/// Il messaggio sullo schermo per un archivio che non passa.
pub fn messaggio(e: ErroreZip) -> &'static str {
    match e {
        ErroreZip::NonValido => "il file è rovinato: l'archivio del documento non si legge.",
        ErroreZip::Rovinato => "il file è rovinato: una parte del documento non si legge.",
        ErroreZip::TroppeVoci => "il documento ha più di 2000 parti: in questa versione non si apre.",
        ErroreZip::VoceTroppoGrande => "una parte del documento supera i 32 MB una volta decompressa: in questa versione non si apre.",
        ErroreZip::TroppoGrande => "il documento supera i 64 MB una volta decompresso: in questa versione non si apre.",
        ErroreZip::TroppiElementi => "il documento ha più di 2 milioni di elementi: in questa versione non si apre.",
        ErroreZip::CompressioneIgnota => "il documento usa una compressione che questa versione non conosce.",
        ErroreZip::Incoerente => "l'archivio del documento si legge in due modi diversi: per sicurezza non si apre.",
    }
}

/// Controlla un archivio coi tetti dei documenti (`tascabile_files::archivio::controlla`).
pub fn controlla(b: &[u8]) -> Result<(), ErroreZip> {
    tascabile_files::archivio::controlla(b, &TETTI)
}

/// Gli elementi della tabella dei font che incorporano un font (ECMA-376, 17.8.3).
const INCORPORA: &[&str] = &["embedRegular", "embedBold", "embedItalic", "embedBoldItalic"];

/// La tabella dei font senza gli elementi che incorporano un font, con qualunque prefisso; None
/// se non ce n'erano.
fn senza_incorporati(xml: &str) -> Option<String> {
    let mut fuori = String::with_capacity(xml.len());
    let mut resto = xml;
    let mut tolti = false;
    while let Some(lt) = resto.find('<') {
        let dopo = &resto[lt + 1..];
        let fine_nome = dopo.find(|c: char| c.is_whitespace() || c == '/' || c == '>').unwrap_or(dopo.len());
        let nome = &dopo[..fine_nome];
        let locale = nome.rsplit(':').next().unwrap_or(nome);
        if !INCORPORA.contains(&locale) {
            fuori.push_str(&resto[..lt + 1]);
            resto = dopo;
            continue;
        }
        let Some(gt) = dopo.find('>') else { break };
        fuori.push_str(&resto[..lt]);
        tolti = true;
        let aperto = !dopo[..gt].ends_with('/');
        resto = &dopo[gt + 1..];
        if aperto {
            let chiusura = format!("</{nome}>");
            match resto.find(&chiusura) {
                Some(c) => resto = &resto[c + chiusura.len()..],
                None => break,
            }
        }
    }
    fuori.push_str(resto);
    tolti.then_some(fuori)
}

/// L'archivio senza i font incorporati: dalla tabella dei font (`word/fontTable.xml`) si tolgono
/// gli elementi che li incorporano e si toglie il file delle sue relazioni, così il motore non li
/// trova (le parti le rimette poi `documento::rimetti_i_font`, senza caricarle, perché un
/// salvataggio non le perda). Le altre voci si copiano come sono, senza decomprimerle.
/// Da chiamare dopo `controlla`. Un archivio senza font incorporati torna com'è.
pub fn senza_font_incorporati(b: &[u8]) -> Result<Cow<'_, [u8]>, ErroreZip> {
    fn no<E>(_: E) -> ErroreZip {
        ErroreZip::NonValido
    }
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(b)).map_err(no)?;
    let tabella = |n: &str| n.to_ascii_lowercase().ends_with("fonttable.xml");
    let relazioni = |n: &str| n.to_ascii_lowercase().ends_with("fonttable.xml.rels");
    let mut nuove: Vec<(usize, Option<String>)> = Vec::new();
    let mut cambia = false;
    for i in 0..zip.len() {
        let nome = zip.name_for_index(i).and_then(Result::ok).unwrap_or_default().to_string();
        if relazioni(&nome) {
            nuove.push((i, None));
            cambia = true;
        } else if tabella(&nome) {
            let mut xml = String::new();
            zip.by_index(i).map_err(no)?.take(TETTI.voce_testo).read_to_string(&mut xml).map_err(no)?;
            match senza_incorporati(&xml) {
                Some(pulito) => {
                    nuove.push((i, Some(pulito)));
                    cambia = true;
                }
                None => nuove.push((i, Some(xml))),
            }
        }
    }
    if !cambia {
        return Ok(Cow::Borrowed(b));
    }
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::with_capacity(b.len())));
    for i in 0..zip.len() {
        match nuove.iter().find(|(j, _)| *j == i) {
            Some((_, None)) => {}
            Some((_, Some(xml))) => {
                let nome = zip.name_for_index(i).and_then(Result::ok).unwrap_or_default().to_string();
                let opzioni = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
                out.start_file(nome, opzioni).map_err(no)?;
                out.write_all(xml.as_bytes()).map_err(no)?;
            }
            None => {
                let voce = zip.by_index_raw(i).map_err(no)?;
                out.raw_copy_file(voce).map_err(no)?;
            }
        }
    }
    Ok(Cow::Owned(out.finish().map_err(no)?.into_inner()))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn toglie_gli_elementi_che_incorporano() {
        let xml = r#"<w:fonts><w:font w:name="Calibri"><w:embedRegular r:id="rId1" w:fontKey="{0}"/><w:embedBold r:id="rId2"></w:embedBold><w:panose1/></w:font></w:fonts>"#;
        assert_eq!(senza_incorporati(xml).unwrap(), r#"<w:fonts><w:font w:name="Calibri"><w:panose1/></w:font></w:fonts>"#);
        assert_eq!(senza_incorporati("<w:fonts><w:font/></w:fonts>"), None);
    }
}
