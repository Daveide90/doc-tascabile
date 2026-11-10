//! Dove finiscono i file dell'app (segnalazione dell'autore del 10/11/2026: «non si capisce bene
//! dove finiscono le cose create o salvate dall'app, deve essere trovabile se uno usa un file
//! manager di terze parti»). La cartella sul telefono si chiama «Documents/Tascabile»: è il nome
//! che mostra qualsiasi gestore di file, e l'app la deve chiamare così. Un tempo la chiamava con
//! «Documenti» davanti, una cartella che fuori dall'app non esiste. E i file nuovi vanno detti
//! ad Android, che altrimenti non li mette tutti nel suo indice (sul telefono dell'autore 7 su 22
//! mancavano): le app che cercano i documenti passando dall'indice non li trovavano.

use std::fs;
use std::path::{Path, PathBuf};

fn progetto() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Tutti i file `.rs` sotto `cartella`, senza `target` e senza il codice di terzi.
fn sorgenti(cartella: &Path, trovati: &mut Vec<PathBuf>) {
    let Ok(voci) = fs::read_dir(cartella) else { return };
    for v in voci.flatten() {
        let p = v.path();
        let nome = v.file_name().to_string_lossy().into_owned();
        if p.is_dir() {
            if !matches!(nome.as_str(), "target" | "vendor" | ".git" | "android") {
                sorgenti(&p, trovati);
            }
        } else if nome.ends_with(".rs") && nome != "dove_vanno_i_file.rs" {
            trovati.push(p);
        }
    }
}

/// Il nome di prima, spezzato perché questa prova non trovi sé stessa.
fn nome_di_prima() -> String {
    ["Documenti", "Tascabile"].join("/")
}

#[test]
fn la_cartella_dei_file_ha_ovunque_il_suo_nome_vero() {
    let mut file = Vec::new();
    for c in ["app", "crates"] {
        sorgenti(&progetto().join(c), &mut file);
    }
    assert!(file.len() > 50, "pochi sorgenti letti: {}", file.len());
    let mut sbagliati = Vec::new();
    for f in &file {
        let testo = fs::read_to_string(f).unwrap();
        for (n, riga) in testo.lines().enumerate() {
            if riga.contains(&nome_di_prima()) {
                sbagliati.push(format!("{}:{}", f.strip_prefix(progetto()).unwrap_or(f).display(), n + 1));
            }
        }
    }
    assert!(sbagliati.is_empty(), "«{}» non esiste sul telefono, la cartella è Documents/Tascabile:\n{}", nome_di_prima(), sbagliati.join("\n"));
}

/// Uscendo dall'app (per aprire un gestore di file, per esempio) i file nuovi di
/// Documents/Tascabile vanno detti ad Android con `MediaScannerConnection.scanFile`, fuori dal
/// thread dello schermo, e senza scrivere i nomi nel registro.
#[test]
fn uscendo_dall_app_i_file_nuovi_vanno_nell_indice_di_android() {
    let java = fs::read_to_string(progetto().join("android/app/src/main/java/it/tascabile/app/MainActivity.java")).unwrap();
    assert!(java.contains("import android.media.MediaScannerConnection;"), "manca l'import");
    let pausa = java.find("protected void onPause()").expect("manca onPause");
    let corpo = &java[pausa..pausa + java[pausa..].find("\n    }\n").expect("onPause non finisce")];
    assert!(corpo.contains("super.onPause();"), "{corpo}");
    assert!(corpo.contains("segnalaIFileNuovi();"), "{corpo}");
    let inizio = java.find("private void segnalaIFileNuovi()").expect("manca segnalaIFileNuovi");
    let metodo = &java[inizio..inizio + java[inizio..].find("\n    }\n").expect("segnalaIFileNuovi non finisce")];
    assert!(metodo.contains("Environment.DIRECTORY_DOCUMENTS"), "{metodo}");
    assert!(metodo.contains("\"Tascabile\""), "{metodo}");
    assert!(metodo.contains("new Thread("), "la cartella si legge fuori dal thread dello schermo:\n{metodo}");
    assert!(metodo.contains("MediaScannerConnection.scanFile(getApplicationContext()"), "{metodo}");
    // I file nascosti (i temporanei dei salvataggi) e le cartelle non si segnalano.
    assert!(metodo.contains("isFile()") && metodo.contains("startsWith(\".\")"), "{metodo}");
    // Nel registro quanti, mai quali.
    assert!(!metodo.contains("getName())") || !metodo.contains("Log."), "{metodo}");
}
