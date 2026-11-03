# deckcraft-pptx (copia di DeckCraft)

**Da dove viene.** `crates/pptx` di DeckCraft (https://github.com/storytold/deckcraft) al commit `b47195feec0f0fe4220d8943ad1c8add5dad8072`, lo stesso che il workspace usa per il resto del motore. Copiati solo `src/`, le licenze (`LICENSE-APACHE`, `LICENSE-MIT`) e il `NOTICE` del progetto; `Cargo.toml` riscritto con le versioni del workspace di DeckCraft al posto di `workspace = true`.

**Perché.** Il motore originale legge gli archivi con zip 3.0.0, colpita da CVE-2026-115905 (critica, intestazioni che non coincidono, corretta in zip 9.0.5), CVE-2026-115906, CVE-2026-115909 e dagli avvisi RUSTSEC-2026-0367, 0368, 0369 e 0429 (corretti in zip 9.0.0). Il controllo degli archivi dell'app (`crates/files/src/archivio.rs`) deve leggere con la stessa zip dei motori, così vedono gli stessi byte: questa copia legge con zip 9.2.3, come il controllo. Entra al posto dell'originale con `[patch."https://github.com/storytold/deckcraft"]` nel `Cargo.toml` del workspace.

**Le modifiche** (cercare «Tascabile» nei sorgenti):

1. `Cargo.toml`: `zip = "=9.2.3"` al posto di `zip = "3"`, con le stesse feature (solo `deflate`).
2. `src/opc.rs`, `Package::open_within`: in zip 9 `ZipFile::name()` restituisce un `Result`. Una voce col nome illeggibile si salta con un avviso nel registro, come già le voci che non si leggono.

Le prove del crate (`#[cfg(test)]` e `tests/`) non sono copiate o adattate: questa copia non è un membro del workspace e non si compilano.

**Da togliere** quando il motore passa da sé a zip 9 o più (aggiornando il commit nel workspace); da rifare se il commit del motore cambia.
