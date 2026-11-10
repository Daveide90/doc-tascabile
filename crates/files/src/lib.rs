//! File di Tascabile: il tipo di un file dal suo nome, le cartelle del telefono, i recenti, i
//! nomi per i file nuovi, dove si era arrivati a leggere i PDF, le foto senza i dati nascosti e il
//! controllo degli archivi zip prima dei motori.
//! Rust puro, senza JNI e senza motori: si prova tutto su Linux.

pub mod archivio;
mod cartelle;
mod metadati;
mod nomi;
mod posizioni;
mod recenti;
mod ritrova;
pub mod temporanei;

pub use cartelle::{Ordine, Scorciatoia, Voce, cartella_dell_omonimo, cartella_tascabile, elenca, elenca_tutto, lo_stesso_file, nome_contiene, ordina, scorciatoie};
pub use metadati::senza_metadati;
pub use nomi::{ErroreRinomina, MASSIMO_CARATTERI_NOME, NOME_DI_RIPIEGO, nome_libero, nome_pulito, rinomina, sposta_senza_sostituire};
pub use posizioni::{MASSIMO_POSIZIONI, Posizione, Posizioni, impronta};
pub use recenti::{MASSIMO_RECENTI, Recente, Recenti};
pub use ritrova::{ImprontaFile, cerca_per_impronta};

use serde::{Deserialize, Serialize};

/// Che cosa c'è in un file: decide l'icona e il modulo che lo apre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tipo {
    Documento,
    Pdf,
    Presentazione,
    /// Un foglio di calcolo (in sola lettura, dalla 0.9.0).
    Foglio,
    /// Tutto il resto, cartelle comprese. Nei recenti salvati vale anche per un tipo scritto da
    /// una versione più nuova dell'app, così la lista non va persa.
    #[serde(other)]
    Altro,
}

/// Estensioni dei documenti: tutte le apre il motore di WordCraft (lo controlla una prova in
/// tascabile-word, contro `wordcraft_engine::io::OPEN_EXTS`).
pub const ESTENSIONI_DOCUMENTO: &[&str] = &["docx", "docm", "dotx", "dotm", "doc", "dot", "odt", "rtf", "txt", "md", "markdown", "html", "htm"];

/// Estensioni dei PDF.
pub const ESTENSIONI_PDF: &[&str] = &["pdf"];

/// Estensioni delle presentazioni: quelle che apre DeckCraft (pptx, potx, ppsx) più i formati
/// vecchi e di LibreOffice, che restano presentazioni anche se il motore non li apre.
pub const ESTENSIONI_PRESENTAZIONE: &[&str] = &["pptx", "potx", "ppsx", "ppt", "odp"];

/// Estensioni dei fogli di calcolo: quelle che leggono i lettori di GridCraft (Excel, il binario
/// di Excel, LibreOffice, il testo separato da virgole o da tabulazioni).
pub const ESTENSIONI_FOGLIO: &[&str] = &["xlsx", "xlsm", "xltx", "xltm", "xls", "xlt", "xlsb", "ods", "csv", "tsv"];

/// Il tipo di un file dalla sua estensione, senza guardare maiuscole e minuscole. Conta solo
/// l'ultima estensione («Verbale.docx.zip» è un archivio); un nome che è solo un'estensione
/// («.docx») non ne ha.
pub fn tipo_da_nome(nome: &str) -> Tipo {
    let ultimo = nome.rsplit(['/', '\\']).next().unwrap_or(nome);
    let Some((base, estensione)) = ultimo.rsplit_once('.') else {
        return Tipo::Altro;
    };
    if base.is_empty() {
        return Tipo::Altro;
    }
    let estensione = estensione.to_ascii_lowercase();
    let e = estensione.as_str();
    if ESTENSIONI_DOCUMENTO.contains(&e) {
        Tipo::Documento
    } else if ESTENSIONI_PDF.contains(&e) {
        Tipo::Pdf
    } else if ESTENSIONI_PRESENTAZIONE.contains(&e) {
        Tipo::Presentazione
    } else if ESTENSIONI_FOGLIO.contains(&e) {
        Tipo::Foglio
    } else {
        Tipo::Altro
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn tipi_dalle_estensioni() {
        for nome in [
            "Verbale.docx",
            "vecchio.doc",
            "macro.docm",
            "modello.dotx",
            "modello con macro.dotm",
            "modello vecchio.dot",
            "libre.odt",
            "testo.rtf",
            "appunti.txt",
            "note.md",
            "note.markdown",
            "pagina.html",
            "pagina.htm",
        ] {
            assert_eq!(tipo_da_nome(nome), Tipo::Documento, "{nome}");
        }
        assert_eq!(tipo_da_nome("Bolletta.pdf"), Tipo::Pdf);
        for nome in ["Lezione.pptx", "modello.potx", "mostra.ppsx", "vecchia.ppt", "libre.odp"] {
            assert_eq!(tipo_da_nome(nome), Tipo::Presentazione, "{nome}");
        }
    }

    #[test]
    fn estensione_senza_maiuscole() {
        assert_eq!(tipo_da_nome("VERBALE.DOCX"), Tipo::Documento);
        assert_eq!(tipo_da_nome("Bolletta.PdF"), Tipo::Pdf);
        assert_eq!(tipo_da_nome("Lezione.PPTX"), Tipo::Presentazione);
        for nome in ["Turni.xlsx", "Conti.XLS", "Dati.ods", "Elenco.csv"] {
            assert_eq!(tipo_da_nome(nome), Tipo::Foglio, "{nome}");
        }
    }

    #[test]
    fn conta_solo_l_ultima_estensione() {
        assert_eq!(tipo_da_nome("relazione.v2.pdf"), Tipo::Pdf);
        assert_eq!(tipo_da_nome("Verbale.docx.zip"), Tipo::Altro);
        assert_eq!(tipo_da_nome("Verbale.docx.crdownload"), Tipo::Altro);
    }

    #[test]
    fn senza_estensione_nota_e_altro() {
        for nome in ["Verbale", "Lettera.", ".docx", "", "foto.jpg", "archivio.zip", "cartella.docx/Verbale"] {
            assert_eq!(tipo_da_nome(nome), Tipo::Altro, "{nome:?}");
        }
    }
}

#[cfg(test)]
pub(crate) mod prova {
    use std::path::{Path, PathBuf};

    /// Cartella vuota per una prova, dentro target/ (mai in /tmp).
    pub fn cartella_di_prova(nome: &str) -> PathBuf {
        let d = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/prove-files").join(nome);
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }
}
