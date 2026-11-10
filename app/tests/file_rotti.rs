//! File rotti apposta (10/11/2026, revisione prima della 0.9.8: «il fuzzing dei file in ingresso
//! non è mai stato fatto»). Dai documenti di prova nascono migliaia di varianti rovinate, sempre
//! le stesse (un generatore col seme fisso): tagliate, con un pezzo tolto o raddoppiato, con un
//! numero enorme al posto di uno vero, con un byte cambiato, con migliaia di elementi annidati.
//! Per i formati zip (Word, Excel, PowerPoint) si rovina una parte alla volta dentro l'archivio,
//! che poi si richiude: così la variante passa il controllo dello zip e arriva al lettore.
//!
//! Ogni variante si apre come nell'app, su un thread con lo stesso stack, e poi si disegna la
//! prima pagina o la prima slide. Aprirsi o non aprirsi vanno bene tutti e due. Non vanno bene:
//! un panico che esce dal lettore, un thread di disegno che muore, un'apertura che non finisce.
//!
//! Sono circa 13 000 varianti e dura qualche minuto: è fuori dal giro normale (`--ignored` per farla).

use std::io::{Cursor, Read, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

/// Lo stack dei thread di apertura dell'app (`app/src/apertura.rs`).
const STACK: usize = 16 << 20;
/// Oltre questo tempo un'apertura è «ferma».
const TEMPO_MASSIMO: Duration = Duration::from_secs(40);

/// Numeri a caso sempre uguali (xorshift64).
struct Caso(u64);

impl Caso {
    fn nuovo(seme: u64) -> Caso {
        Caso(seme.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn prossimo(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn fino_a(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.prossimo() % n as u64) as usize }
    }
}

const NUMERI: [&str; 8] = ["0", "-1", "4294967295", "2147483648", "99999999999999999999", "1e308", "-0", "65536"];

/// Una variante rovinata di `byte`; `genere` sceglie come.
fn rovina(byte: &[u8], genere: usize, c: &mut Caso) -> Vec<u8> {
    let n = byte.len();
    if n == 0 {
        return vec![b'<'];
    }
    let a = c.fino_a(n);
    let lungo = 1 + c.fino_a(200.min(n - a));
    match genere % 8 {
        // Tagliato.
        0 => byte[..a].to_vec(),
        // Un pezzo tolto.
        1 => [&byte[..a], &byte[a + lungo..]].concat(),
        // Un pezzo raddoppiato.
        2 => [&byte[..a + lungo], &byte[a..]].concat(),
        // Un numero al posto di un altro.
        3 => {
            let inizio = (a..n).find(|i| byte[*i].is_ascii_digit()).or_else(|| (0..a).find(|i| byte[*i].is_ascii_digit()));
            let Some(inizio) = inizio else { return byte[..a].to_vec() };
            let fine = (inizio..n).find(|i| !byte[*i].is_ascii_digit()).unwrap_or(n);
            let numero = NUMERI[c.fino_a(NUMERI.len())].as_bytes();
            [&byte[..inizio], numero, &byte[fine..]].concat()
        }
        // Un byte cambiato.
        4 => {
            let mut v = byte.to_vec();
            v[a] = (c.prossimo() & 0xff) as u8;
            v
        }
        // Migliaia di elementi annidati, mai chiusi.
        5 => [&byte[..a], "<a>".repeat(20_000).as_bytes(), &byte[a..]].concat(),
        // Un valore lunghissimo.
        6 => [&byte[..a], "A".repeat(100_000).as_bytes(), &byte[a..]].concat(),
        // Un pezzo spostato altrove.
        _ => {
            let b = c.fino_a(n);
            let mut v = byte.to_vec();
            let pezzo: Vec<u8> = v.drain(a..a + lungo).collect();
            let b = b.min(v.len());
            v.splice(b..b, pezzo);
            v
        }
    }
}

/// Dall'ottava variante in poi, una su due ha due danni insieme.
fn rovina_una_o_due_volte(byte: &[u8], k: usize, c: &mut Caso) -> Vec<u8> {
    let una = rovina(byte, k, c);
    if (k / 8) % 2 == 1 { rovina(&una, k / 8 + k, c) } else { una }
}

/// Le voci di un archivio zip: nome e contenuto.
fn voci(zip: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut a = zip::ZipArchive::new(Cursor::new(zip)).expect("il documento di prova è uno zip");
    (0..a.len())
        .map(|i| {
            let mut v = a.by_index(i).expect("la voce si legge");
            let nome = v.name().expect("il nome della voce").into_owned();
            let mut b = Vec::new();
            v.read_to_end(&mut b).expect("la voce si decomprime");
            (nome, b)
        })
        .collect()
}

/// L'archivio rifatto da `voci`.
fn richiudi(voci: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opzioni = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (nome, byte) in voci {
        z.start_file(nome.as_str(), opzioni).expect("la voce si apre");
        z.write_all(byte).expect("la voce si scrive");
    }
    z.finish().expect("l'archivio si chiude").into_inner()
}

/// Le varianti di un documento zip: `per_parte` per ogni parte di testo (XML e relazioni), più
/// qualcuna dell'archivio intero, rovinato da fuori.
fn varianti_zip(nome: &str, zip: &[u8], per_parte: usize, seme: u64) -> Vec<(String, Vec<u8>)> {
    let originali = voci(zip);
    let mut out = Vec::new();
    for (i, (parte, byte)) in originali.iter().enumerate() {
        let testo = parte.ends_with(".xml") || parte.ends_with(".rels") || parte.ends_with(".vml");
        if !testo || byte.len() > 2 << 20 {
            continue;
        }
        for k in 0..per_parte {
            let mut c = Caso::nuovo(seme ^ ((i as u64) << 20) ^ k as u64);
            let mut copia = originali.clone();
            copia[i].1 = rovina_una_o_due_volte(byte, k, &mut c);
            out.push((format!("{nome} · {parte} · {k}"), richiudi(&copia)));
        }
    }
    for k in 0..24 {
        let mut c = Caso::nuovo(seme ^ 0xABCD_0000 ^ k as u64);
        out.push((format!("{nome} · archivio · {k}"), rovina(zip, k, &mut c)));
    }
    out
}

/// Le varianti di un file che non è uno zip (PDF): rovinato da fuori.
fn varianti(nome: &str, byte: &[u8], quante: usize, seme: u64) -> Vec<(String, Vec<u8>)> {
    (0..quante)
        .map(|k| {
            let mut c = Caso::nuovo(seme ^ k as u64);
            (format!("{nome} · {k}"), rovina_una_o_due_volte(byte, k, &mut c))
        })
        .collect()
}

/// Com'è andata una variante.
#[derive(Debug, PartialEq, Eq)]
enum Esito {
    Aperto,
    NonAperto,
    /// Un panico uscito dal lettore o dal disegno.
    Panico(String),
    /// Il thread di disegno non ha risposto.
    DisegnoFermo,
}

fn del_panico(p: Box<dyn std::any::Any + Send>) -> String {
    p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_else(|| "panico".into())
}

fn apri_documento(byte: Vec<u8>) -> Esito {
    match catch_unwind(AssertUnwindSafe(|| tascabile_word::documento::apri_byte("rotto.docx", &byte).is_ok())) {
        Ok(true) => Esito::Aperto,
        Ok(false) => Esito::NonAperto,
        Err(p) => Esito::Panico(del_panico(p)),
    }
}

fn apri_foglio(byte: Vec<u8>) -> Esito {
    match catch_unwind(AssertUnwindSafe(|| tascabile_foglio::Cartella::apri(&byte).is_ok())) {
        Ok(true) => Esito::Aperto,
        Ok(false) => Esito::NonAperto,
        Err(p) => Esito::Panico(del_panico(p)),
    }
}

fn apri_pdf(byte: Vec<u8>) -> Esito {
    let aperto = catch_unwind(AssertUnwindSafe(|| {
        let Ok(pdf) = tascabile_pdf::Pdf::apri(byte, None) else { return false };
        if pdf.pagine() > 0 {
            let _ = pdf.disegnatore().disegna(0, 0.5);
        }
        true
    }));
    match aperto {
        Ok(true) => Esito::Aperto,
        Ok(false) => Esito::NonAperto,
        Err(p) => Esito::Panico(del_panico(p)),
    }
}

fn apri_slide(byte: Vec<u8>) -> Esito {
    use tascabile_slide::{Disegnatore, Presentazione, Richiesta, Uso};
    let aperta = match catch_unwind(AssertUnwindSafe(|| Presentazione::apri(&byte))) {
        Ok(Ok(p)) => p,
        Ok(Err(_)) => return Esito::NonAperto,
        Err(p) => return Esito::Panico(del_panico(p)),
    };
    if aperta.slide() == 0 {
        return Esito::Aperto;
    }
    // Il disegno come nel lettore: sul thread «slide-disegno», che deve rispondere.
    let Ok(d) = Disegnatore::avvia(aperta.documento().clone(), || {}) else { return Esito::Aperto };
    d.chiedi([Richiesta { indice: 0, larghezza: 240, uso: Uso::Miniatura, passo: None }]);
    let inizio = Instant::now();
    while inizio.elapsed() < Duration::from_secs(20) {
        if d.prendi().is_some() {
            return Esito::Aperto;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    Esito::DisegnoFermo
}

/// Apre ogni variante sul suo thread, con lo stack dell'app, e conta. Il nome della variante in
/// corso sta in un file: se il processo cade (uno stack finito non si ferma), si sa quale era.
fn prova(genere: &str, varianti: Vec<(String, Vec<u8>)>, apri: fn(Vec<u8>) -> Esito) -> Vec<String> {
    let cartella = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("file-rotti");
    std::fs::create_dir_all(&cartella).unwrap();
    let in_corso = cartella.join(format!("{genere}-in-corso.txt"));
    let (mut aperti, mut non_aperti, mut piu_lento) = (0usize, 0usize, Duration::ZERO);
    let mut guai = Vec::new();
    let totale = varianti.len();
    for (nome, byte) in varianti {
        std::fs::write(&in_corso, &nome).unwrap();
        let (manda, arriva) = channel();
        let inizio = Instant::now();
        std::thread::Builder::new()
            .stack_size(STACK)
            .spawn(move || {
                let _ = manda.send(apri(byte));
            })
            .unwrap();
        let esito = arriva.recv_timeout(TEMPO_MASSIMO);
        piu_lento = piu_lento.max(inizio.elapsed());
        match esito {
            Ok(Esito::Aperto) => aperti += 1,
            Ok(Esito::NonAperto) => non_aperti += 1,
            Ok(Esito::Panico(p)) => guai.push(format!("{nome}: panico uscito dal lettore ({p})")),
            Ok(Esito::DisegnoFermo) => guai.push(format!("{nome}: il thread di disegno non risponde")),
            Err(_) => guai.push(format!("{nome}: non finisce in {} s (o il thread è caduto)", TEMPO_MASSIMO.as_secs())),
        }
    }
    let _ = std::fs::remove_file(&in_corso);
    eprintln!("{genere}: {totale} varianti, {aperti} aperte, {non_aperti} rifiutate, {} guai, la più lenta {:.1} s", guai.len(), piu_lento.as_secs_f32());
    guai
}

fn nessun_guaio(guai: Vec<String>) {
    assert!(guai.is_empty(), "{} varianti con guai:\n{}", guai.len(), guai.join("\n"));
}

#[test]
#[ignore = "dura un paio di minuti: cargo test -p tascabile --test file_rotti -- --ignored"]
fn i_documenti_di_word_rotti_non_fermano_l_app() {
    let mut v = Vec::new();
    for (i, (nome, byte)) in [
        ("lettera", &include_bytes!("../../crates/mod-word/modelli/lettera.docx")[..]),
        ("lezione", &include_bytes!("../../crates/mod-word/modelli/lezione.docx")[..]),
        ("relazione", &include_bytes!("../../crates/mod-word/modelli/relazione.docx")[..]),
        ("verbale", &include_bytes!("../../crates/mod-word/modelli/verbale.docx")[..]),
    ]
    .into_iter()
    .enumerate()
    {
        v.extend(varianti_zip(nome, byte, 64, 0x1000 + i as u64));
    }
    nessun_guaio(prova("word", v, apri_documento));
}

#[test]
#[ignore = "dura un paio di minuti: cargo test -p tascabile --test file_rotti -- --ignored"]
fn i_fogli_di_calcolo_rotti_non_fermano_l_app() {
    let v = varianti_zip("cartella", &tascabile_foglio::prova::cartella_di_prova(), 160, 0x2000);
    nessun_guaio(prova("fogli", v, apri_foglio));
}

#[test]
#[ignore = "dura un paio di minuti: cargo test -p tascabile --test file_rotti -- --ignored"]
fn le_presentazioni_rotte_non_fermano_l_app() {
    use tascabile_slide::prova::{LEZIONE, QUATTRO_TERZI, VERTICALE};
    let mut v = Vec::new();
    for (i, (nome, byte)) in [("lezione", LEZIONE), ("quattro-terzi", QUATTRO_TERZI), ("verticale", VERTICALE)].into_iter().enumerate() {
        v.extend(varianti_zip(nome, byte, 32, 0x3000 + i as u64));
    }
    nessun_guaio(prova("slide", v, apri_slide));
}

#[test]
#[ignore = "dura un paio di minuti: cargo test -p tascabile --test file_rotti -- --ignored"]
fn i_pdf_rotti_non_fermano_l_app() {
    use tascabile_pdf::prova;
    let mut v = Vec::new();
    for (i, (nome, byte)) in [
        ("modulo", prova::modulo()),
        ("modulo da compilare", prova::modulo_da_compilare()),
        ("modulo coi calcoli", prova::modulo_coi_calcoli()),
        ("pagina italiana", prova::pagina_italiana()),
        ("con foto", prova::pdf_con_foto()),
        ("scansione", prova::scansione_riconosciuta()),
        ("cifrato", prova::cifrato("segreta")),
    ]
    .into_iter()
    .enumerate()
    {
        v.extend(varianti(nome, &byte, 640, 0x4000 + i as u64));
    }
    nessun_guaio(prova("pdf", v, apri_pdf));
}
