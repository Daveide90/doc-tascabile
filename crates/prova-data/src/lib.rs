//! Il «Timestamp con blockchain» dell'app: la prova pubblica che un file esisteva in un momento, così com'era, col
//! protocollo aperto OpenTimestamps (piano `docs/superpowers/plans/2026-10-28-fissa-la-data.md`).

pub mod blocco;
pub mod formato;
pub mod server;

pub use blocco::{BloccoControllato, ErroreBlocco, controlla_intestazione};
pub use formato::{Albero, Attestazione, ErroreFormato, Operazione, Prova};
pub use server::{Controllo, ErroreData, ErroreRete, Rete, Stato, controlla, fissa};

/// Per le prove dell'app: l'esempio pubblico confermato e il suo blocco, e la risposta di un
/// server che mette un'attesa.
#[doc(hidden)]
pub mod prova {
    use crate::formato::{Albero, Attestazione, scrivi_albero};

    /// L'esempio pubblico del client OpenTimestamps: «Hello World!» e la sua prova, confermata nel
    /// blocco 358391 il 28/05/2015 alle 15:41:18 UTC.
    pub const ESEMPIO_TXT: &[u8] = include_bytes!("../tests/fixtures/hello-world.txt");
    pub const ESEMPIO_OTS: &[u8] = include_bytes!("../tests/fixtures/hello-world.txt.ots");
    /// L'intestazione del blocco 358391, in esadecimale, e la sua impronta.
    pub const BLOCCO_358391: &str = include_str!("../tests/fixtures/blocco-358391.hex");
    pub const HASH_358391: &str = "000000000000000003e892881a8cdcdc117c06d444057c98b6f04a9ee75a2319";

    /// La risposta di un server a un'impronta qualunque: un'attesa verso `server`, subito sul
    /// messaggio mandato.
    pub fn risposta_in_attesa(server: &str) -> Vec<u8> {
        let mut a = Albero::nuovo(Vec::new());
        a.aggiungi(Attestazione::InAttesa(server.into()));
        scrivi_albero(&a).unwrap_or_default()
    }
}
