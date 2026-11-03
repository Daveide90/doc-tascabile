//! I temporanei dei salvataggi (`.salvataggio-<pid>-<n>.tmp`, scritti accanto al file e poi
//! rinominati al suo posto): un'app chiusa a metà, da Android o da una batteria finita, li lascia
//! lì, magari con dentro un documento intero (revisione del 31/10, punto 12).

use std::path::Path;

/// Toglie dalla cartella, e dalle sue sottocartelle fino a `profondita` livelli, i temporanei dei
/// salvataggi lasciati da altri processi: all'avvio non ce n'è nessuno in corso. Non segue i
/// collegamenti simbolici.
pub fn pulisci(cartella: &Path, profondita: usize) {
    let Ok(voci) = std::fs::read_dir(cartella) else { return };
    for v in voci.flatten() {
        let Ok(tipo) = v.file_type() else { continue };
        if tipo.is_dir() {
            if profondita > 0 {
                pulisci(&v.path(), profondita - 1);
            }
        } else if tipo.is_file() && di_un_altro_processo(&v.file_name().to_string_lossy()) {
            let _ = std::fs::remove_file(v.path());
        }
    }
}

/// Il nome è di un temporaneo di salvataggio scritto da un processo diverso da questo.
fn di_un_altro_processo(nome: &str) -> bool {
    let Some(resto) = nome.strip_prefix(".salvataggio-").and_then(|r| r.strip_suffix(".tmp")) else { return false };
    let Some((pid, n)) = resto.split_once('-') else { return false };
    pid.parse::<u32>().is_ok_and(|p| p != std::process::id()) && n.parse::<u64>().is_ok()
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn i_temporanei_di_altri_processi_se_ne_vanno() {
        let dir = std::env::temp_dir().join(format!("tascabile-temporanei-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let sotto = dir.join("ripristino-pdf");
        std::fs::create_dir_all(&sotto).unwrap();
        let nostro = format!(".salvataggio-{}-0.tmp", std::process::id());
        let restano = [dir.join("documento.docx"), dir.join(&nostro), dir.join("salvataggio-1-0.tmp"), dir.join(".salvataggio-x-0.tmp")];
        let vanno = [dir.join(".salvataggio-1-0.tmp"), sotto.join(".salvataggio-2-3.tmp")];
        for f in restano.iter().chain(&vanno) {
            std::fs::write(f, b"x").unwrap();
        }
        pulisci(&dir, 4);
        for f in &restano {
            assert!(f.exists(), "{}", f.display());
        }
        for f in &vanno {
            assert!(!f.exists(), "{}", f.display());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
