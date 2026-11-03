//! I fogli di calcolo in sola lettura (richiesta dell'autore, 3/11: «inserisci fogli di calcolo per ora solo in
//! visualizzazione»), coi lettori e il modello di GridCraft (ricerca 2026-11-03-gridcraft-api).
//!
//! Si apre da byte, coi tetti per un telefono ([`apertura`]); i valori sono quelli salvati da
//! Excel, senza ricalcolo, quindi uguali a Excel; date e numeri si mostrano all'italiana
//! ([`italiano`]), e la griglia dà per una zona del foglio le celle da disegnare ([`vista`]).

pub mod apertura;
pub mod italiano;
pub mod vista;

#[doc(hidden)]
pub mod prova;

pub use apertura::{Cartella, Genere, LIMITE_MB, sembra_foglio};

/// I caratteri delle celle per egui: Carlito tondo, grassetto, corsivo e grassetto corsivo (le
/// misure di Calibri, il carattere di Excel), coi nomi delle loro famiglie. Dai caratteri delle
/// Slide, già nell'APK; vuoto se la build non li ha.
pub fn caratteri_delle_celle() -> Vec<(&'static str, &'static [u8])> {
    let nomi = [("Regular", CARLITO[0]), ("Bold", CARLITO[1]), ("Italic", CARLITO[2]), ("Bold Italic", CARLITO[3])];
    nomi.iter()
        .filter_map(|(stile, nome)| deckcraft_fonts::CRAFT_FONTS.iter().find(|f| f.family == "Carlito" && f.style == *stile).map(|f| (*nome, f.bytes)))
        .collect()
}

/// Le famiglie dei caratteri delle celle: tondo, grassetto, corsivo, grassetto corsivo.
pub const CARLITO: [&str; 4] = ["carlito", "carlito-grassetto", "carlito-corsivo", "carlito-grassetto-corsivo"];
pub use vista::{Allinea, Assi, Bordo, Casella, Dettaglio, dettaglio, nome_colonna};
/// I tipi di GridCraft che servono all'app per la griglia.
pub use gridcraft_core::{CellRef, RangeRef};
pub use gridcraft_model::Sheet;

#[cfg(test)]
mod test {
    #[test]
    fn ci_sono_i_caratteri_delle_celle() {
        assert_eq!(super::caratteri_delle_celle().len(), 4);
    }
}
