//! Modulo PDF di Tascabile: il motore di PDFCraft per aprire, misurare e disegnare i PDF.

pub mod annotazioni;
pub mod apertura;
mod caratteri;
mod ritocco;
pub mod contenuto;
pub mod correzione;
pub mod disegno;
pub mod firma;
mod foglio;
pub mod geometria;
pub mod informazioni;
pub mod lettore;
pub mod modifica;
pub mod moduli;
pub mod motore;
pub mod navigazione;
pub mod ocr;
pub mod pagine;
pub mod prova;
pub mod ricerca;
pub mod scansione;
pub mod selezione;
mod stendi;
pub mod tratto;

pub use annotazioni::{Annotazione, Forma, GenereAnnotazione, Marca, Porta, annotazione_sotto, indirizzo_completo};
pub use apertura::{ErroreApertura, Genere, LIMITE_MB, Misura, Pdf, sembra_pdf};
pub use contenuto::{GenereOggetto, Oggetto, carattere_che_manca};
pub use disegno::{Disegnatore, ErroreDisegno, LATO_MASSIMO, Rettangolo, immagine};
pub use firma::{ErroreFoto, Firma};
pub use informazioni::{Firme, Informazioni};
pub use lettore::{AzioniSelezione, EsitoGriglia, Organizza, SceltaSelezione};
pub use modifica::{GenereModifica, Modifica, Protezione, Toccate};
pub use moduli::{Campo, Data, GenereCampo, GenereSegno, NuovoSegno, ParteData, PostoCampo, Segno, da_compilare, lettera_fuori_da_winansi};
pub use motore::{Evento, Fatto, Istantanea, LavoroCopia, Motore, Risveglio, Scritto, Scrittura};
pub use navigazione::{Collegamento, Destinazione, Segnalibro};
pub use pagine::{Compressione, Livello};
/// I valori dei campi, come li vuole il motore.
pub use pdfcraft_engine::FieldValue;
/// Le pagine come le vede il motore: rotazione e riquadro.
pub use pdfcraft_render::PageInfo;
pub use ricerca::Ricerca;
pub use selezione::Selezione;
