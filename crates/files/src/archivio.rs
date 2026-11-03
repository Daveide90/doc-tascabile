//! Gli archivi zip dei documenti (docx, docm, dotx, odt, pptx), guardati prima di darli ai
//! motori: un archivio gonfiato apposta non deve finire la memoria del telefono (rapporti di
//! sicurezza del 24/10, problema 2, e del 30/10, problema 1; revisione del 31/10, punto 1).
//!
//! Si conta con lo stesso lettore dei motori, la `zip` del workspace (9.2.3, la stessa dei crate dei
//! motori in vendor/), così il controllo vede le stesse voci e gli stessi byte che vedranno loro.
//! Fino alla 0.8.4 l'archivio si leggeva a mano, e un archivio fatto apposta (la misura della
//! directory a zero, byte davanti o in fondo, il campo zip64) si leggeva in un modo nel controllo e
//! in un altro nel motore.
//!
//! Prima di aprirlo si guardano le voci dichiarate: `ZipArchive` prenota memoria per quelle che la
//! fine dell'archivio dichiara, fino all'inizio dichiarato della directory centrale (qualche
//! centinaio di byte per voce). Poi le due copie di ogni voce devono coincidere: l'intestazione
//! locale davanti ai dati e la riga della directory centrale in fondo (nome, compressione, misure,
//! date), senza nomi doppi, voci sovrapposte o voci fuori dalla directory. I motori credono alla
//! directory, chi legge in fila alle intestazioni locali: se non coincidono, lo stesso file è due
//! documenti diversi (CVE-2026-115905, «zip cloaking»). Poi ogni voce si decomprime in streaming
//! contando i byte veri, senza scrivere su disco. Le voci di testo (XML: cominciano con «<», anche
//! dopo spazi o un BOM) hanno tetti più bassi di quelle binarie (immagini, video) e un tetto agli
//! elementi: un lettore ne fa un albero in memoria molte volte più grande, e milioni di paragrafi
//! vuoti stanno in pochi MB. Si guarda il contenuto, non il nome: un XML chiamato come un'immagine
//! resta testo.

use std::io::Read;

/// I tetti di un archivio: voci, byte decompressi ed elementi XML.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tetti {
    /// Voci al massimo, dichiarate o vere.
    pub voci: u64,
    /// Byte decompressi al massimo in una voce di testo.
    pub voce_testo: u64,
    /// Byte decompressi al massimo in una voce binaria.
    pub voce_binaria: u64,
    /// Byte decompressi al massimo in tutte le voci di testo.
    pub totale_testo: u64,
    /// Byte decompressi al massimo in tutto l'archivio.
    pub totale: u64,
    /// Elementi XML al massimo in tutte le voci di testo, contati dai «<».
    pub elementi: u64,
}

/// Perché un archivio non passa. Il genere va nella diagnosi; il messaggio sullo schermo lo
/// scrive chi controlla, coi suoi tetti.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroreZip {
    /// Non si apre come zip: non lo apriranno nemmeno i motori, che usano lo stesso lettore.
    NonValido,
    /// Si apre, ma una voce non si legge fino in fondo.
    Rovinato,
    TroppeVoci,
    VoceTroppoGrande,
    TroppoGrande,
    TroppiElementi,
    CompressioneIgnota,
    /// L'intestazione locale di una voce (davanti ai suoi dati) non dice quello che dice la
    /// directory centrale (in fondo), oppure due voci hanno lo stesso nome, si sovrappongono o una
    /// non è nella directory: lettori diversi vedrebbero archivi diversi (CVE-2026-115905, «zip
    /// cloaking»).
    Incoerente,
}

impl ErroreZip {
    pub fn genere(self) -> &'static str {
        match self {
            ErroreZip::NonValido => "zip non valido",
            ErroreZip::Rovinato => "zip con una voce che non si legge",
            ErroreZip::TroppeVoci => "zip con troppe voci",
            ErroreZip::VoceTroppoGrande => "zip con una voce troppo grande",
            ErroreZip::TroppoGrande => "zip troppo grande decompresso",
            ErroreZip::TroppiElementi => "zip con troppi elementi XML",
            ErroreZip::CompressioneIgnota => "zip con una compressione non gestita",
            ErroreZip::Incoerente => "zip con intestazioni che non coincidono",
        }
    }
}

/// I byte sembrano un archivio zip: cominciano come uno zip, oppure ne hanno la fine negli ultimi
/// 64 KB (uno zip con qualcosa davanti, che i lettori dei motori accettano). Non guarda
/// l'estensione: i motori aprono per contenuto anche un .docx chiamato .txt. Un contenitore OLE
/// (un .doc, un .docx cifrato) non è uno zip anche se dentro ne porta uno, per esempio un oggetto
/// incorporato.
pub fn sembra_zip(byte: &[u8]) -> bool {
    if byte.starts_with(b"PK\x03\x04") || byte.starts_with(b"PK\x05\x06") {
        return true;
    }
    if byte.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]) {
        return false;
    }
    let da = byte.len().saturating_sub(22 + 0xFFFF);
    byte[da..].windows(4).any(|w| w == FINE)
}

fn u16_a(b: &[u8], i: usize) -> Option<u64> {
    Some(u16::from_le_bytes(b.get(i..i + 2)?.try_into().ok()?) as u64)
}

fn u64_a(b: &[u8], i: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(i..i + 8)?.try_into().ok()?))
}

const FINE: [u8; 4] = [0x50, 0x4b, 0x05, 0x06];
const FINE64: [u8; 4] = [0x50, 0x4b, 0x06, 0x06];

/// Le voci dichiarate da tutte le fini d'archivio del file, anche a metà: `zip` cerca la fine in
/// tutto il file e legge il numero a 8 byte dall'inizio della fine, o a 24 e 32 in quella zip64. Un
/// numero a 0xFFFF rimanda alla fine zip64, guardata a parte. Pubblica per chi apre uno zip solo
/// per riconoscerlo (`tascabile_foglio::sembra_foglio`).
pub fn voci_dichiarate(b: &[u8], tetto: u64) -> Result<(), ErroreZip> {
    let mut da = 0;
    while let Some(p) = b[da..].iter().position(|&c| c == b'P') {
        let i = da + p;
        da = i + 1;
        let campi: &[(usize, bool)] = match b.get(i..i + 4) {
            Some(w) if w == FINE => &[(8, false), (10, false)],
            Some(w) if w == FINE64 => &[(24, true), (32, true)],
            _ => continue,
        };
        for &(k, zip64) in campi {
            let voci = if zip64 { u64_a(b, i + k) } else { u16_a(b, i + k).filter(|&v| v != 0xFFFF) };
            if voci.is_some_and(|v| v > tetto) {
                return Err(ErroreZip::TroppeVoci);
            }
        }
    }
    Ok(())
}

/// Quanti byte iniziali bastano per dire se una voce è testo.
const INIZIO_DA_GUARDARE: usize = 4096;

/// Se i primi byte di una voce sono di testo (XML): dopo un eventuale BOM e degli spazi, «<», in
/// UTF-8 o in UTF-16 (anche UTF-16 senza BOM, che i lettori riconoscono da «<» e uno zero). None
/// se non si può ancora dire (sono arrivati pochi byte e non è la fine). Una voce vuota, o di soli
/// spazi, è testo: il tetto più basso.
fn e_testo(inizio: &[u8], tutto: bool) -> Option<bool> {
    let non_si_sa = || if tutto || inizio.len() >= INIZIO_DA_GUARDARE { Some(true) } else { None };
    if inizio.starts_with(&[0x00, b'<']) {
        return Some(true);
    }
    for (bom, be) in [([0xFE, 0xFF], true), ([0xFF, 0xFE], false)] {
        if let Some(resto) = inizio.strip_prefix(&bom[..]) {
            for coppia in resto.chunks(2) {
                let [a, b] = *coppia else { return non_si_sa() };
                let c = if be { u16::from_be_bytes([a, b]) } else { u16::from_le_bytes([a, b]) };
                if c < 0x80 && (c as u8).is_ascii_whitespace() {
                    continue;
                }
                return Some(c == u16::from(b'<'));
            }
            return non_si_sa();
        }
    }
    if inizio.len() < 3 && !tutto && [0xEF, 0xBB, 0xBF].starts_with(inizio) {
        return None;
    }
    let dopo_il_bom = inizio.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(inizio);
    match dopo_il_bom.iter().find(|c| !c.is_ascii_whitespace()) {
        Some(c) => Some(*c == b'<'),
        None => non_si_sa(),
    }
}

/// Controlla un archivio zip prima di un motore: le voci dichiarate, poi, con lo stesso lettore dei
/// motori, che le due intestazioni di ogni voce coincidano, i byte veri di ogni voce nei tetti del
/// suo genere e in quelli di tutto l'archivio, e gli elementi XML.
pub fn controlla(b: &[u8], tetti: &Tetti) -> Result<(), ErroreZip> {
    voci_dichiarate(b, tetti.voci)?;
    let mut archivio = zip::ZipArchive::new(std::io::Cursor::new(b)).map_err(|_| ErroreZip::NonValido)?;
    if archivio.len() as u64 > tetti.voci {
        return Err(ErroreZip::TroppeVoci);
    }
    // CVE-2026-115905: la verifica che zip aggiunge dalla 9.0.5 e che `ZipArchive` non fa da sé.
    // Legge solo le intestazioni (e cerca intestazioni locali negli spazi fra le voci), non
    // decomprime niente: va prima del conto.
    archivio.validate_header_consistency().map_err(|e| match e {
        zip::result::ZipError::Io(_) => ErroreZip::Rovinato,
        _ => ErroreZip::Incoerente,
    })?;
    let mut conto = Conto::default();
    for i in 0..archivio.len() {
        let voce = archivio.by_index(i).map_err(|e| match e {
            zip::result::ZipError::UnsupportedArchive(_) => ErroreZip::CompressioneIgnota,
            _ => ErroreZip::Rovinato,
        })?;
        conto.voce(voce, tetti)?;
    }
    Ok(())
}

/// Quello che si è contato finora in un archivio.
#[derive(Default)]
struct Conto {
    totale: u64,
    di_testo: u64,
    elementi: u64,
}

impl Conto {
    /// Decomprime una voce in streaming, contando byte ed elementi; si ferma appena un tetto è
    /// superato.
    fn voce(&mut self, mut voce: impl Read, tetti: &Tetti) -> Result<(), ErroreZip> {
        let mut buffer = vec![0u8; 64 * 1024];
        let mut inizio: Vec<u8> = Vec::new();
        let mut testo: Option<bool> = None;
        let mut n = 0u64;
        loop {
            let k = match voce.read(&mut buffer) {
                Ok(0) => break,
                Ok(k) => k,
                Err(_) => return Err(ErroreZip::Rovinato),
            };
            let letti = &buffer[..k];
            if testo.is_none() {
                inizio.extend_from_slice(&letti[..k.min(INIZIO_DA_GUARDARE.saturating_sub(inizio.len()))]);
                testo = e_testo(&inizio, false);
            }
            n += k as u64;
            // Finché non si sa, vale il testo: il tetto più basso, e gli elementi contano.
            let genere = testo.unwrap_or(true);
            if genere {
                self.elementi += letti.iter().filter(|&&c| c == b'<').count() as u64;
            }
            self.oltre(n, genere, tetti)?;
        }
        let genere = testo.or_else(|| e_testo(&inizio, true)).unwrap_or(true);
        self.totale += n;
        if genere {
            self.di_testo += n;
        }
        Ok(())
    }

    /// Una voce di `n` byte del genere dato, sopra quello già contato, supera un tetto?
    fn oltre(&self, n: u64, testo: bool, tetti: &Tetti) -> Result<(), ErroreZip> {
        if n > if testo { tetti.voce_testo } else { tetti.voce_binaria } {
            return Err(ErroreZip::VoceTroppoGrande);
        }
        if self.totale + n > tetti.totale || (testo && self.di_testo + n > tetti.totale_testo) {
            return Err(ErroreZip::TroppoGrande);
        }
        if self.elementi > tetti.elementi {
            return Err(ErroreZip::TroppiElementi);
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::io::Write;

    const MB: u64 = 1024 * 1024;
    /// I tetti dei documenti Word: tutte le voci come testo.
    const WORD: Tetti =
        Tetti { voci: 2_000, voce_testo: 32 * MB, voce_binaria: 32 * MB, totale_testo: 64 * MB, totale: 64 * MB, elementi: 2_000_000 };
    /// I tetti delle presentazioni: le immagini e i video possono essere grandi, il testo no.
    const SLIDE: Tetti =
        Tetti { voci: 10_000, voce_testo: 32 * MB, voce_binaria: 256 * MB, totale_testo: 96 * MB, totale: 320 * MB, elementi: 2_000_000 };
    const CENTRALE: [u8; 4] = [0x50, 0x4b, 0x01, 0x02];

    /// Un archivio con queste voci, compresse.
    fn archivio(voci: &[(&str, &[u8])]) -> Vec<u8> {
        archivio_con(voci, zip::CompressionMethod::Deflated)
    }

    fn archivio_con(voci: &[(&str, &[u8])], metodo: zip::CompressionMethod) -> Vec<u8> {
        let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (nome, byte) in voci {
            let opzioni = zip::write::SimpleFileOptions::default().compression_method(metodo).large_file(byte.len() as u64 >= 0xFFFF_FFFF);
            z.start_file(*nome, opzioni).unwrap();
            z.write_all(byte).unwrap();
        }
        z.finish().unwrap().into_inner()
    }

    /// `n` byte di testo XML: «<» e poi spazi.
    fn xml(n: usize) -> Vec<u8> {
        let mut v = vec![b' '; n];
        v[0] = b'<';
        v
    }

    #[test]
    fn un_archivio_normale_passa() {
        let b = archivio(&[("a.xml", b"<a/>"), ("b.xml", &[b'x'; 10_000])]);
        assert_eq!(controlla(&b, &WORD), Ok(()));
        assert!(sembra_zip(&b));
    }

    #[test]
    fn troppe_voci_dichiarate() {
        let mut b = archivio(&[("a.xml", b"<a/>")]);
        // Fine dell'archivio che dichiara 60000 voci (e la directory centrale com'è).
        let n = b.len();
        b[n - 22 + 8..n - 22 + 12].copy_from_slice(&[0x60, 0xEA, 0x60, 0xEA]);
        assert_eq!(controlla(&b, &WORD), Err(ErroreZip::TroppeVoci));
        assert_eq!(controlla(&b, &SLIDE), Err(ErroreZip::TroppeVoci));
    }

    #[test]
    fn troppe_voci_vere() {
        let nomi: Vec<String> = (0..2_001).map(|i| format!("v{i}")).collect();
        let voci: Vec<(&str, &[u8])> = nomi.iter().map(|n| (n.as_str(), &b""[..])).collect();
        assert_eq!(controlla(&archivio(&voci), &WORD), Err(ErroreZip::TroppeVoci));
        assert_eq!(controlla(&archivio(&voci[..2_000]), &WORD), Ok(()));
        assert_eq!(controlla(&archivio(&voci), &SLIDE), Ok(()));
    }

    #[test]
    fn una_voce_che_si_gonfia() {
        let zeri = vec![0u8; 33 * 1024 * 1024];
        let b = archivio(&[("word/document.xml", &zeri)]);
        assert!(b.len() < 1024 * 1024, "compressa è piccola: {}", b.len());
        assert_eq!(controlla(&b, &WORD), Err(ErroreZip::VoceTroppoGrande));
    }

    #[test]
    fn tante_voci_che_si_gonfiano() {
        let zeri = vec![0u8; 30 * 1024 * 1024];
        let b = archivio(&[("a", &zeri), ("b", &zeri), ("c", &zeri)]);
        assert_eq!(controlla(&b, &WORD), Err(ErroreZip::TroppoGrande));
    }

    /// Una voce che dichiara 10 byte ma ne dà 33 MB non passa. Se lo dice solo la directory, le due
    /// intestazioni non coincidono; se lo dicono tutte e due, zip 9 si ferma ai byte dichiarati con
    /// un errore, e così i motori.
    #[test]
    fn la_misura_dichiarata_non_conta() {
        let zeri = vec![0u8; 33 * 1024 * 1024];
        let mut b = archivio(&[("a", &zeri)]);
        let cd = b.windows(4).rposition(|w| w == CENTRALE).unwrap();
        b[cd + 24..cd + 28].copy_from_slice(&10u32.to_le_bytes());
        assert!(il_motore_la_apre(&b));
        assert_eq!(controlla(&b, &WORD), Err(ErroreZip::Incoerente));
        b[22..26].copy_from_slice(&10u32.to_le_bytes());
        assert!(il_motore_la_apre(&b));
        assert_eq!(controlla(&b, &WORD), Err(ErroreZip::Rovinato));
    }

    /// Un archivio che decompresso fa 33 MB di XML: oltre i tetti di una voce di testo di Word e
    /// delle presentazioni.
    fn bomba() -> Vec<u8> {
        archivio(&[("word/document.xml", &xml(33 * 1024 * 1024))])
    }

    fn il_motore_la_apre(b: &[u8]) -> bool {
        zip::ZipArchive::new(std::io::Cursor::new(b)).is_ok()
    }

    // Revisione del 31/10 (un documento interno di lavoro, punto 1): il controllo e il
    // lettore dei motori devono vedere lo stesso archivio.

    /// La misura della directory centrale (nella fine dell'archivio) a zero: zip non la usa.
    #[test]
    fn la_misura_della_directory_a_zero_non_salta_il_conto() {
        let mut b = bomba();
        let n = b.len();
        b[n - 22 + 12..n - 22 + 16].copy_from_slice(&0u32.to_le_bytes());
        assert!(il_motore_la_apre(&b));
        assert_eq!(controlla(&b, &WORD), Err(ErroreZip::VoceTroppoGrande));
        assert_eq!(controlla(&b, &SLIDE), Err(ErroreZip::VoceTroppoGrande));
    }

    /// Byte davanti all'archivio: zip trova da solo dove comincia.
    #[test]
    fn i_byte_davanti_non_saltano_il_conto() {
        let mut b = b"PK\x07\x08 qualcosa davanti".to_vec();
        b.extend(bomba());
        assert!(il_motore_la_apre(&b));
        assert_eq!(controlla(&b, &WORD), Err(ErroreZip::VoceTroppoGrande));
        assert_eq!(controlla(&b, &SLIDE), Err(ErroreZip::VoceTroppoGrande));
    }

    /// Byte dopo la fine dell'archivio, più dei 64 KB di un commento: se zip lo apre, si conta.
    #[test]
    fn i_byte_in_fondo_non_saltano_il_conto() {
        let mut b = bomba();
        b.extend(vec![b'x'; 70_000]);
        let atteso = if il_motore_la_apre(&b) { ErroreZip::VoceTroppoGrande } else { ErroreZip::NonValido };
        assert_eq!(controlla(&b, &WORD), Err(atteso));
    }

    /// zip legge il numero di voci a 8 byte dalla fine dell'archivio, non solo a 10.
    #[test]
    fn le_voci_dichiarate_si_leggono_dove_le_legge_zip() {
        let mut b = archivio(&[("a.xml", b"<a/>")]);
        let n = b.len();
        b[n - 22 + 8..n - 22 + 10].copy_from_slice(&60_000u16.to_le_bytes());
        assert_eq!(controlla(&b, &WORD), Err(ErroreZip::TroppeVoci));
    }

    const LOCALE: [u8; 4] = [0x50, 0x4b, 0x03, 0x04];

    fn u16_in(b: &[u8], i: usize) -> usize {
        u16::from_le_bytes([b[i], b[i + 1]]) as usize
    }

    fn u32_in(b: &[u8], i: usize) -> usize {
        u32::from_le_bytes(b[i..i + 4].try_into().unwrap()) as usize
    }

    /// Le voci della directory centrale di un archivio senza commento, dove le dice la fine
    /// dell'archivio: il primo byte e quello dopo l'ultimo di ognuna.
    fn voci_centrali(b: &[u8]) -> Vec<std::ops::Range<usize>> {
        let mut p = u32_in(b, b.len() - 22 + 16);
        let mut voci = Vec::new();
        while b[p..p + 4] == CENTRALE {
            let n = 46 + u16_in(b, p + 28) + u16_in(b, p + 30) + u16_in(b, p + 32);
            voci.push(p..p + n);
            p += n;
        }
        voci
    }

    /// Dove comincia l'intestazione locale della voce centrale `v`.
    fn locale(b: &[u8], v: &std::ops::Range<usize>) -> usize {
        let p = u32_in(b, v.start + 42);
        assert_eq!(b[p..p + 4], LOCALE);
        p
    }

    /// CVE-2026-115905 («zip cloaking», corretta in zip 9.0.5): ogni voce è scritta due volte,
    /// nell'intestazione locale davanti ai dati e nella directory centrale in fondo. `ZipArchive`,
    /// quello dei motori e del controllo, crede alla directory; chi legge l'archivio in fila (un
    /// antivirus, un'altra app a cui il documento si manda) crede alle intestazioni locali. Se le
    /// due non coincidono, lo stesso file è due documenti diversi. Tutti questi archivi il lettore
    /// dei motori li apre: il controllo li deve fermare prima.
    #[test]
    fn le_intestazioni_che_non_coincidono_non_passano() {
        let xml = b"<w:document><w:body>Dose 10 mg</w:body></w:document>";
        let normale = archivio_con(&[("word/document.xml", xml), ("word/settings.xml", b"<w:settings/>")], zip::CompressionMethod::Stored);
        assert_eq!(controlla(&normale, &WORD), Ok(()));
        let voci = voci_centrali(&normale);
        let (prima, seconda) = (&voci[0], &voci[1]);
        let mut casi: Vec<(&str, Vec<u8>)> = Vec::new();

        // Il nome nella directory dice una cosa, quello locale un'altra (lo stesso numero di byte,
        // così i dati restano dove sono).
        let mut b = normale.clone();
        let p = locale(&b, prima);
        b[p + 30..p + 30 + 17].copy_from_slice(b"word/malevolo.xml");
        casi.push(("nome locale diverso", b));
        let mut b = normale.clone();
        b[prima.start + 46..prima.start + 46 + 17].copy_from_slice(b"word/malevolo.xml");
        casi.push(("nome centrale diverso", b));

        // Il metodo di compressione e le misure: chi legge in fila decomprimerebbe altri byte.
        let mut b = normale.clone();
        let p = locale(&b, prima);
        b[p + 8..p + 10].copy_from_slice(&8u16.to_le_bytes());
        casi.push(("compressione locale diversa", b));
        let mut b = normale.clone();
        let p = locale(&b, prima);
        let corta = (u32_in(&b, p + 18) - 10) as u32;
        b[p + 18..p + 22].copy_from_slice(&corta.to_le_bytes());
        b[p + 22..p + 26].copy_from_slice(&corta.to_le_bytes());
        casi.push(("misura locale diversa", b));

        // Due voci con lo stesso nome, in tutte e due le intestazioni: `ZipArchive` ne tiene una.
        let mut b = normale.clone();
        let p = locale(&b, seconda);
        b[p + 30..p + 30 + 17].copy_from_slice(b"word/document.xml");
        b[seconda.start + 46..seconda.start + 46 + 17].copy_from_slice(b"word/document.xml");
        casi.push(("nome doppio", b));

        // Una voce che c'è nel file ma non nella directory: si toglie la sua riga dalla directory e
        // la fine dell'archivio conta una voce in meno.
        let mut b = normale.clone();
        let n = seconda.len();
        b.drain(seconda.clone());
        let fine = b.len() - 22;
        b[fine + 8..fine + 10].copy_from_slice(&1u16.to_le_bytes());
        b[fine + 10..fine + 12].copy_from_slice(&1u16.to_le_bytes());
        let misura = (u32_in(&b, fine + 12) - n) as u32;
        b[fine + 12..fine + 16].copy_from_slice(&misura.to_le_bytes());
        casi.push(("voce fuori dalla directory", b));

        let mut passano = Vec::new();
        for (caso, b) in &casi {
            assert!(il_motore_la_apre(b), "{caso}: il lettore dei motori non la apre");
            let (word, slide) = (controlla(b, &WORD), controlla(b, &SLIDE));
            if word != Err(ErroreZip::Incoerente) || slide != Err(ErroreZip::Incoerente) {
                passano.push(format!("{caso}: {word:?}, {slide:?}"));
            }
        }
        assert!(passano.is_empty(), "il controllo non le ferma: {passano:#?}");
    }

    /// Milioni di paragrafi vuoti stanno in pochi MB e in memoria diventano gigabyte.
    #[test]
    fn troppi_elementi() {
        let pochi = b"<p/>".repeat(1_000_000);
        assert_eq!(controlla(&archivio(&[("word/document.xml", &pochi)]), &WORD), Ok(()));
        let troppi = b"<p/>".repeat(2_000_001);
        assert_eq!(controlla(&archivio(&[("word/document.xml", &troppi)]), &WORD), Err(ErroreZip::TroppiElementi));
    }

    #[test]
    fn spazzatura_non_e_un_archivio() {
        assert_eq!(controlla(b"PK\x03\x04 niente", &WORD), Err(ErroreZip::NonValido));
        assert_eq!(controlla(&[], &WORD), Err(ErroreZip::NonValido));
        assert!(!sembra_zip(b"niente"));
    }

    /// Nelle presentazioni un'immagine o un video grandi passano, il testo grande no: un XML si
    /// riconosce dal contenuto, anche chiamato come un'immagine, anche non compresso.
    #[test]
    fn testo_e_binario_hanno_tetti_diversi() {
        let video = vec![0u8; 40 * 1024 * 1024];
        assert_eq!(controlla(&archivio(&[("ppt/media/media1.mp4", &video)]), &SLIDE), Ok(()));
        let grande = xml(33 * 1024 * 1024);
        assert_eq!(controlla(&archivio(&[("ppt/slides/slide1.xml", &grande)]), &SLIDE), Err(ErroreZip::VoceTroppoGrande));
        assert_eq!(controlla(&archivio(&[("ppt/media/image1.png", &grande)]), &SLIDE), Err(ErroreZip::VoceTroppoGrande));
        assert_eq!(
            controlla(&archivio_con(&[("ppt/media/image2.png", &grande)], zip::CompressionMethod::Stored), &SLIDE),
            Err(ErroreZip::VoceTroppoGrande)
        );
        // Quattro parti di testo da 30 MB: ognuna va, tutte insieme superano i 96 MB.
        let trenta = xml(30 * 1024 * 1024);
        let quattro: Vec<(String, &[u8])> = (0..4).map(|i| (format!("ppt/slides/slide{i}.xml"), trenta.as_slice())).collect();
        let quattro: Vec<(&str, &[u8])> = quattro.iter().map(|(n, b)| (n.as_str(), *b)).collect();
        assert_eq!(controlla(&archivio(&quattro), &SLIDE), Err(ErroreZip::TroppoGrande));
        assert_eq!(controlla(&archivio(&quattro[..3]), &SLIDE), Ok(()));
    }

    #[test]
    fn il_testo_si_riconosce_dai_primi_byte() {
        assert_eq!(e_testo(b"<?xml version", false), Some(true));
        assert_eq!(e_testo(b"  \r\n\t<a/>", false), Some(true));
        assert_eq!(e_testo(&[0xEF, 0xBB, 0xBF, b'<'], false), Some(true));
        assert_eq!(e_testo(&[0xFE, 0xFF, 0x00, b'<'], false), Some(true));
        assert_eq!(e_testo(&[0xFF, 0xFE, b' ', 0x00, b'<', 0x00], false), Some(true));
        assert_eq!(e_testo(&[0x00, b'<', 0x00, b'?'], false), Some(true));
        // Un audio che comincia come un BOM UTF-16 ma poi non ha «<».
        assert_eq!(e_testo(&[0xFF, 0xFE, 0x12, 0x34], false), Some(false));
        assert_eq!(e_testo(&[0xFF, 0xFE, 0x20], false), None);
        assert_eq!(e_testo(&[0x89, b'P', b'N', b'G'], false), Some(false));
        assert_eq!(e_testo(&[0xFF, 0xD8, 0xFF], false), Some(false));
        // Pochi byte che potrebbero essere l'inizio di un BOM, o soli spazi: non si sa ancora.
        assert_eq!(e_testo(&[0xEF, 0xBB], false), None);
        assert_eq!(e_testo(b"   ", false), None);
        assert_eq!(e_testo(b"   ", true), Some(true));
        assert_eq!(e_testo(b"", true), Some(true));
    }
}
