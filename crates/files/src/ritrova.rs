//! Ritrovare un file dai suoi byte (7/11/2026, segnalazione dell'autore: «ogni tanto non mi fa
//! aprire i file perché dice che non esistono», e la sua domanda: «non si potrebbe cercare e
//! indicizzare i file per hash?»). Un recente tiene l'impronta SHA-256 del file com'era quando si
//! è aperto, con la sua misura. Se il percorso non risponde più, perché un'altra app ha spostato o
//! rinominato il file, o se il permesso di un «Apri con» è scaduto, si cercano nella memoria del
//! telefono i file della stessa misura e, solo di quelli, l'impronta: con gli stessi byte è lo
//! stesso documento, e si può riaprire senza il rischio di un omonimo (revisione del 3/11, M3).
//! Niente indice di tutti i file: leggere la misura costa poco, e si legge un file intero solo
//! quando la misura coincide, a blocchi (dal 10/11: prima stava tutto in memoria).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// L'impronta di un file: SHA-256 dei byte, in esadecimale, e la misura.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImprontaFile {
    pub sha256: String,
    pub byte: u64,
}

impl ImprontaFile {
    pub fn di(byte: &[u8]) -> ImprontaFile {
        ImprontaFile { sha256: esadecimale(&Sha256::digest(byte)), byte: byte.len() as u64 }
    }

    /// L'impronta di un file letto a blocchi: in memoria ne sta uno alla volta, mai il file
    /// intero (un PDF può essere di centinaia di MB). Serve dopo un salvataggio, quando i byte
    /// del file sono altri, e nella ricerca.
    pub fn del_file(file: &Path) -> std::io::Result<ImprontaFile> {
        use std::io::Read;
        let mut f = std::fs::File::open(file)?;
        let mut hash = Sha256::new();
        let mut blocco = vec![0u8; BLOCCO];
        let mut byte = 0u64;
        loop {
            match f.read(&mut blocco) {
                Ok(0) => break,
                Ok(n) => {
                    hash.update(&blocco[..n]);
                    byte += n as u64;
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        Ok(ImprontaFile { sha256: esadecimale(&hash.finalize()), byte })
    }
}

/// Quanti byte di un file si leggono alla volta per la sua impronta.
const BLOCCO: usize = 64 * 1024;

fn esadecimale(hash: &[u8]) -> String {
    hash.iter().map(|b| format!("{b:02x}")).collect()
}

/// Le voci che la ricerca guarda al più, e quanti livelli di cartelle scende.
const VOCI_VISTE: usize = 200_000;
const LIVELLI: usize = 8;
/// Le cartelle private delle app, che Android comunque non mostra.
const SALTATE: [&str; 2] = ["Android/data", "Android/obb"];

/// Il file sotto `radice` con questa impronta, se c'è. Salta le cartelle e i file nascosti (col
/// punto davanti, come il cestino del telefono), i collegamenti e le cartelle private delle app.
/// Legge il disco a lungo: va chiamata su un thread.
pub fn cerca_per_impronta(radice: &Path, impronta: &ImprontaFile) -> Option<PathBuf> {
    let mut viste = 0usize;
    let mut da_vedere = vec![(radice.to_path_buf(), 0usize)];
    while let Some((cartella, livello)) = da_vedere.pop() {
        let Ok(voci) = std::fs::read_dir(&cartella) else { continue };
        for voce in voci.flatten() {
            viste += 1;
            if viste > VOCI_VISTE {
                return None;
            }
            if voce.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            // file_type non segue i collegamenti: un collegamento non è né cartella né file.
            let Ok(tipo) = voce.file_type() else { continue };
            let percorso = voce.path();
            if tipo.is_dir() {
                let privata = percorso.strip_prefix(radice).is_ok_and(|r| SALTATE.iter().any(|s| r == Path::new(s)));
                if livello < LIVELLI && !privata {
                    da_vedere.push((percorso, livello + 1));
                }
            } else if tipo.is_file()
                && voce.metadata().is_ok_and(|m| m.len() == impronta.byte)
                && ImprontaFile::del_file(&percorso).is_ok_and(|i| i == *impronta)
            {
                return Some(percorso);
            }
        }
    }
    None
}

#[cfg(test)]
mod test {
    use std::fs;

    use super::*;
    use crate::prova::cartella_di_prova;

    #[test]
    fn un_file_spostato_si_ritrova_dai_suoi_byte() {
        let r = cartella_di_prova("ritrova");
        let impronta = ImprontaFile::di(b"il referto di prova");
        assert_eq!(impronta.byte, 19);
        assert_eq!(impronta.sha256.len(), 64);
        assert_eq!(ImprontaFile::di(b"il referto di prova"), impronta);
        // Spostato e rinominato in una sottocartella: si ritrova.
        fs::create_dir_all(r.join("Documents/Lavoro")).unwrap();
        fs::write(r.join("Documents/Lavoro/Referto rinominato.pdf"), b"il referto di prova").unwrap();
        // Stessa misura, altri byte: non è lui.
        fs::create_dir_all(r.join("Download")).unwrap();
        fs::write(r.join("Download/Referto.pdf"), b"il referto di altro").unwrap();
        assert_eq!(cerca_per_impronta(&r, &impronta), Some(r.join("Documents/Lavoro/Referto rinominato.pdf")));
        // Nelle cartelle nascoste e in quelle private delle app no.
        fs::remove_file(r.join("Documents/Lavoro/Referto rinominato.pdf")).unwrap();
        fs::create_dir_all(r.join(".trashed")).unwrap();
        fs::write(r.join(".trashed/Referto.pdf"), b"il referto di prova").unwrap();
        fs::create_dir_all(r.join("Android/data/com.app")).unwrap();
        fs::write(r.join("Android/data/com.app/Referto.pdf"), b"il referto di prova").unwrap();
        assert_eq!(cerca_per_impronta(&r, &impronta), None);
        // In Android/media sì (WhatsApp).
        fs::create_dir_all(r.join("Android/media/com.whatsapp")).unwrap();
        fs::write(r.join("Android/media/com.whatsapp/Referto.pdf"), b"il referto di prova").unwrap();
        assert_eq!(cerca_per_impronta(&r, &impronta), Some(r.join("Android/media/com.whatsapp/Referto.pdf")));
    }

    /// L'impronta letta dal file, a blocchi, è quella dei suoi byte: serve dopo un salvataggio e
    /// nella ricerca, senza tenere in memoria il file intero (revisione del 10/11).
    #[test]
    fn l_impronta_di_un_file_e_quella_dei_suoi_byte() {
        let d = cartella_di_prova("impronta-del-file");
        // Più lungo di un blocco, e non un multiplo.
        let byte: Vec<u8> = (0..300_001u32).map(|i| (i % 251) as u8).collect();
        let file = d.join("Grande.pdf");
        fs::write(&file, &byte).unwrap();
        assert_eq!(ImprontaFile::del_file(&file).unwrap(), ImprontaFile::di(&byte));
        fs::write(&file, b"").unwrap();
        assert_eq!(ImprontaFile::del_file(&file).unwrap(), ImprontaFile::di(b""));
        assert_eq!(ImprontaFile::del_file(&d.join("Manca.pdf")).unwrap_err().kind(), std::io::ErrorKind::NotFound);
    }
}
