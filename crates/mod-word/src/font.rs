//! Font di sistema che il motore non trova da solo su Android (cerca solo le cartelle di Linux).

use std::path::Path;

use wordcraft_fonts::FontDb;

/// Aggiunge i font emoji di una cartella (su Android `/system/fonts`). Restituisce i volti aggiunti.
pub fn aggiungi_font_emoji(cartella: &Path) -> usize {
    let Ok(voci) = std::fs::read_dir(cartella) else {
        return 0;
    };
    let nomi: Vec<String> = voci.flatten().map(|voce| voce.file_name().to_string_lossy().into_owned()).collect();
    let mut aggiunti = 0;
    for nome in da_caricare(&nomi) {
        match std::fs::read(cartella.join(&nome)) {
            Ok(byte) => aggiunti += FontDb::global().add_font(byte),
            Err(e) => log::warn!("font {nome}: {e}"),
        }
    }
    aggiunti
}

/// I file emoji da caricare fra i `nomi` di una cartella, in ordine alfabetico. Su Android 14 e
/// 15 ci sono NotoColorEmoji.ttf (COLRv1) e NotoColorEmojiLegacy.ttf (CBDT) con la stessa
/// famiglia, e vincerebbe quello letto per primo: il Legacy si carica solo se è l'unico.
fn da_caricare(nomi: &[String]) -> Vec<String> {
    let emoji = |n: &str| {
        let n = n.to_lowercase();
        (n.ends_with(".ttf") || n.ends_with(".otf") || n.ends_with(".ttc")) && n.contains("emoji")
    };
    let legacy = |n: &str| n.to_lowercase().contains("legacy");
    let mut scelti: Vec<String> = nomi.iter().filter(|n| emoji(n)).cloned().collect();
    if scelti.iter().any(|n| !legacy(n)) {
        scelti.retain(|n| !legacy(n));
    }
    scelti.sort();
    scelti
}

#[cfg(test)]
mod test {
    use super::*;

    fn nomi(n: &[&str]) -> Vec<String> {
        n.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn il_legacy_solo_se_e_l_unico() {
        // Android 14 e 15: tre file emoji, due con la famiglia «Noto Color Emoji».
        let android = nomi(&["NotoColorEmoji.ttf", "NotoColorEmojiLegacy.ttf", "NotoColorEmojiFlags.ttf", "Roboto.ttf"]);
        assert_eq!(da_caricare(&android), ["NotoColorEmoji.ttf", "NotoColorEmojiFlags.ttf"]);
        // Telefono vecchio: c'è solo quello.
        assert_eq!(da_caricare(&nomi(&["NotoColorEmojiLegacy.ttf", "Roboto.ttf"])), ["NotoColorEmojiLegacy.ttf"]);
        // Niente emoji, o file che non sono font.
        assert!(da_caricare(&nomi(&["Roboto.ttf", "emoji.xml", "fonts.xml"])).is_empty());
        assert_eq!(da_caricare(&nomi(&["SamsungColorEmoji.TTF"])), ["SamsungColorEmoji.TTF"]);
    }

    #[test]
    fn cartella_che_non_esiste() {
        assert_eq!(aggiungi_font_emoji(Path::new("/non/esiste/davvero")), 0);
    }

    #[test]
    fn file_rotto_ignorato() {
        let dir = std::env::temp_dir().join(format!("tascabile-prova-font-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("FintoEmoji.ttf"), b"non sono un font").unwrap();
        std::fs::write(dir.join("Altro.ttf"), b"ignorato per nome").unwrap();
        assert_eq!(aggiungi_font_emoji(&dir), 0);
        std::fs::remove_dir_all(&dir).unwrap(); // solo la cartella creata da questo test
    }
}
