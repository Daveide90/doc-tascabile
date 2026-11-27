//! Le tabelle delle traduzioni: (italiano, inglese, spagnolo), una riga per testo. L'italiano è
//! quello scritto nel codice, parola per parola; `{}` sono i valori di [`crate::tf!`].
//!
//! Le scelte delle parole stanno nel glossario `docs/glossario-lingue.md`: firma elettronica,
//! timestamp, copia definitiva e gli altri termini fissi si traducono sempre allo stesso modo.

type Riga = (&'static str, &'static str, &'static str);

mod app;
mod comuni;

/// Tutte le righe delle tabelle.
pub(super) fn tutte() -> impl Iterator<Item = &'static Riga> {
    comuni::TESTI.iter().chain(app::TESTI)
}
