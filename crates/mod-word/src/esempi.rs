//! Documenti di prova della Fase 0: la checklist della tastiera e un documento lungo per le misure.

use serde_json::{Value, json};
use wordcraft_engine::Session;

/// Voci della checklist (design, sezione 11, più le prove di sistema).
pub const VOCI: [&str; 14] = [
    "Lettere accentate: à è é ì ò ù",
    "Suggerimenti e autocorrezione",
    "Scrittura a scorrimento",
    "Dettatura con il microfono",
    "Emoji",
    "Cursore spostato scorrendo sulla barra spaziatrice",
    "Cancellazione tenendo premuto",
    "Maiuscola automatica a inizio frase",
    "La barra di formato resta sopra la tastiera",
    "Tre paragrafi di fila senza ritardi",
    "«Apri con» da WhatsApp",
    "«Apri con» con l'app già aperta",
    "Indietro, poi di nuovo nell'app",
    "Telefono ruotato: il testo va a capo sulla nuova larghezza",
];

const FRASE: &str = "La biblioteca del quartiere apre alle nove e chiude alle diciannove; il sabato \
    ospita un laboratorio di lettura per bambini e, una volta al mese, un incontro con chi scrive.";

fn esegui(s: &mut Session, id: &str, p: Value) {
    if let Err(e) = s.run(id, &p) {
        log::warn!("esempi: {id}: {e}");
    }
}

/// Paragrafo nuovo con lo stile dato (`para.normal`, `para.heading1`...).
fn paragrafo(s: &mut Session, stile: &str, testo: &str) {
    esegui(s, "text.newParagraph", json!({}));
    esegui(s, stile, json!({}));
    if !testo.is_empty() {
        esegui(s, "text.insert", json!({ "text": testo, "raw": true }));
    }
}

/// La checklist della Fase 0: chi prova scrive «sì» o «no» accanto a ogni voce.
pub fn checklist() -> Session {
    let mut s = crate::documento::nuovo();
    esegui(&mut s, "para.heading1", json!({}));
    esegui(&mut s, "text.insert", json!({ "text": "Tascabile · prova della tastiera", "raw": true }));
    paragrafo(&mut s, "para.normal", "Tocca il testo, scrivi e prova ogni voce. Accanto a ogni voce scrivi sì o no.");
    for (i, voce) in VOCI.iter().enumerate() {
        paragrafo(&mut s, "para.normal", &format!("{}. {voce}: ", i + 1));
    }
    paragrafo(&mut s, "para.heading2", "Spazio per scrivere");
    paragrafo(&mut s, "para.normal", "");
    // I comandi che l'hanno costruita non sono modifiche di chi scrive: Annulla non deve smontarla.
    s.reset_history();
    s.dirty = false;
    s
}

/// Documento lungo per misurare impaginazione e disegno: un capitolo ogni dieci paragrafi.
pub fn lungo(paragrafi: usize) -> Session {
    let mut s = crate::documento::nuovo();
    esegui(&mut s, "para.heading1", json!({}));
    esegui(&mut s, "text.insert", json!({ "text": "Documento lungo di prova", "raw": true }));
    for i in 0..paragrafi {
        if i % 10 == 0 {
            paragrafo(&mut s, "para.heading2", &format!("Capitolo {}", i / 10 + 1));
        }
        paragrafo(&mut s, "para.normal", &format!("{}. {} {} {}", i + 1, FRASE, FRASE, FRASE));
    }
    s.reset_history();
    s.dirty = false;
    s
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::prova::testi;

    #[test]
    fn checklist_ha_tutte_le_voci() {
        let s = checklist();
        let t = testi(&s);
        assert_eq!(t[0], "Tascabile · prova della tastiera");
        assert!(t.iter().any(|p| p.starts_with("14. Telefono ruotato")));
        assert_eq!(t.len(), 18);
        assert!(!s.dirty);
        // Annulla sulla checklist appena aperta non la smonta: niente cronologia della costruzione.
        assert!(!s.can_undo());
    }

    #[test]
    fn lungo_ha_i_paragrafi_chiesti() {
        // titolo, 3 capitoli, 30 paragrafi
        let s = lungo(30);
        assert_eq!(testi(&s).len(), 34);
        assert!(!s.dirty);
        assert!(!s.can_undo());
    }
}
