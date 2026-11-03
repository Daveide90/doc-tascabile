//! Componenti per il dito, comuni ai moduli di Tascabile: il tema dei bozzetti (colori, misure,
//! ombre, caratteri, icone e stile di egui), i componenti della nuova grafica (pulsanti, barre,
//! schede ed elenchi, menu, controlli dei fogli, dialoghi e avvisi), il foglio che sale dal fondo,
//! le maniglie della selezione con le gocce e la lente, il campo di testo con la tastiera.

mod barre;
mod campo;
mod controlli;
mod dialogo;
mod disegno;
mod elenco;
mod foglio;
pub mod icone;
mod icone_tabella;
mod maniglie;
#[cfg(test)]
mod prova;
mod pulsanti;
mod strumenti;
pub mod tema;

pub use barre::{BarraAlta, BarraContesto, BarraFlottante, BarraScorrevole, BarraVoci, RispostaBarraAlta};
pub use campo::{CampoTesto, MOSTRA, NASCONDI, PUNTO_SEGRETO, Ricevuto, RispostaRicerca, RispostaSegreto, StatoCampo};
pub use controlli::{
    Campione, GrigliaTabella, GruppoSegmentato, Passo, Tavolozza, Tinta, Verso, etichetta_sezione, fila_scorrevole, misura_tabella, titoletto,
};
pub use dialogo::{AvvisoBreve, DURATA_AVVISO, DURATA_CON_AZIONE, Dialogo, RispostaDialogo};
pub use elenco::{Menu, RispostaVoce, SceltaMenu, Scheda, SchedaCartella, Tipo, Voce, premuto};
pub use foglio::FoglioDalFondo;
pub use maniglie::{
    Bordo, CENTRO_GOCCIA_CURSORE, GOCCIA, INGRANDIMENTO, LENTE, Maniglia, ManigliaCursore, ManigliaMisura, Maniglie, PRESA, RAGGIO, RAGGIO_MISURA,
    RIQUADRO, Trascinamento, TrascinamentoCursore, TrascinamentoMisura, goccia, goccia_cursore, lente, lente_con, rettangolo_goccia, riquadro_misura,
};
pub use pulsanti::{Chip, Peso, Pulsante, PulsanteIcona, PulsanteNuovo};
pub use strumenti::{GrigliaStrumenti, Tono};
