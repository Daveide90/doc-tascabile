# gridcraft-xlsx (copia di GridCraft)

**Da dove viene.** `crates/xlsx` di GridCraft (https://github.com/storytold/gridcraft) al commit `178f3eb745a589c63b61ef97dd7b6b2d6fd48515`, lo stesso che il workspace usa per il resto del motore. Copiati solo `src/`, le licenze (`LICENSE-APACHE`, `LICENSE-MIT`) e il `NOTICE` del progetto; `Cargo.toml` riscritto con le versioni del workspace di GridCraft al posto di `workspace = true`.

**Perché.** Il motore originale legge gli archivi con zip 3.0.0, colpita da CVE-2026-115905 (critica, intestazioni che non coincidono, corretta in zip 9.0.5), CVE-2026-115906, CVE-2026-115909 e dagli avvisi RUSTSEC-2026-0367, 0368, 0369 e 0429 (corretti in zip 9.0.0). Il controllo degli archivi dell'app (`crates/files/src/archivio.rs`) deve leggere con la stessa zip dei motori, così vedono gli stessi byte: questa copia legge con zip 9.2.3, come il controllo. Entra al posto dell'originale con `[patch."https://github.com/storytold/gridcraft"]` nel `Cargo.toml` del workspace.

**Le modifiche** (cercare «Tascabile» nei sorgenti):

1. `Cargo.toml`: `zip = "=9.2.3"` al posto di `zip = "3"`, con le stesse feature (solo `deflate`).
2. `src/package.rs`, `Package::open`: `ZipArchive::name_for_index` ora dà un `Result`; una voce col nome illeggibile resta fuori dall'elenco delle parti.
3. `src/package.rs`, `oversized_part`: `ZipFile::name()` ora dà un `Result`; illeggibile, nel messaggio il nome resta vuoto.
4. `src/ods.rs`, entità XML con nome: `match &*r` al posto di `match r.as_ref()`. typed-path, che zip 9 porta con sé, aggiunge a `[u8]` altre implementazioni di `AsRef` e il compilatore non deduce più il tipo; `&*r` è lo stesso slice di byte.

Le prove del crate (`#[cfg(test)]`, in `src/tests.rs` e `src/tests/`) non sono adattate: questa copia non è un membro del workspace e non si compilano.

**Da togliere** quando il motore passa da sé a zip 9 o più (aggiornando il commit nel workspace); da rifare se il commit del motore cambia.
