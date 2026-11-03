//! Il formato delle prove `.ots` di OpenTimestamps, come lo scrive e lo legge il client ufficiale
//! (python-opentimestamps 0.4.5, `core/serialize.py`, `core/timestamp.py`, `core/op.py`,
//! `core/notary.py`): l'intestazione, la versione 1, l'impronta del file e l'albero dei passi
//! dall'impronta alle attestazioni. Tutto quello che arriva da un file o da un server è da non
//! fidarsi: profondità, lunghezze e caratteri hanno gli stessi limiti del client.

use ripemd::Ripemd160;
use sha1::Sha1;
use sha2::{Digest, Sha256};

/// L'inizio di ogni prova (`DetachedTimestampFile.HEADER_MAGIC`).
pub const INTESTAZIONE: &[u8] = b"\x00OpenTimestamps\x00\x00Proof\x00\xbf\x89\xe2\xe8\x84\xe8\x92\x94";
/// L'unica versione del formato.
const VERSIONE: u64 = 1;
/// I codici delle operazioni; SHA-256 è anche quella dell'impronta del file.
const SHA1: u8 = 0x02;
const RIPEMD160: u8 = 0x03;
const SHA256: u8 = 0x08;
const ACCODA: u8 = 0xf0;
const ANTEPONI: u8 = 0xf1;
const ROVESCIA: u8 = 0xf2;
const ESADECIMALE: u8 = 0xf3;
/// Il segno di un'attestazione, e quello di «c'è un altro ramo dopo».
const ATTESTAZIONE: u8 = 0x00;
const ALTRO_RAMO: u8 = 0xff;
/// I codici delle attestazioni (8 byte).
const IN_ATTESA: [u8; 8] = [0x83, 0xdf, 0xe3, 0x0d, 0x2e, 0xf9, 0x0c, 0x8e];
const BLOCCO: [u8; 8] = [0x05, 0x88, 0x96, 0x0d, 0x73, 0xd7, 0x19, 0x01];
/// I limiti del client: messaggi e argomenti, dati di un'attestazione, indirizzi, profondità.
pub const MESSAGGIO_MASSIMO: usize = 4096;
const DATI_MASSIMI: usize = 8192;
const INDIRIZZO_MASSIMO: usize = 1000;
const PROFONDITA_MASSIMA: usize = 256;
/// I caratteri ammessi negli indirizzi dei server (`PendingAttestation.ALLOWED_URI_CHARS`).
const CARATTERI_DEGLI_INDIRIZZI: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._/:";

/// Perché una prova non si legge. Senza i byte della prova: possono avere impronte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroreFormato {
    /// Non comincia come una prova di OpenTimestamps.
    NonEUnaProva,
    /// Una versione o un'impronta che qui non si leggono.
    NonSupportata(&'static str),
    /// Finisce prima del tempo.
    Troncata,
    /// Qualcosa che il formato non ammette: il genere.
    Rovinata(&'static str),
    /// Byte in più dopo la fine.
    ByteInPiu,
}

impl std::fmt::Display for ErroreFormato {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErroreFormato::NonEUnaProva => f.write_str("non è una prova della data"),
            ErroreFormato::NonSupportata(che) => write!(f, "prova non supportata ({che})"),
            ErroreFormato::Troncata => f.write_str("prova troncata"),
            ErroreFormato::Rovinata(che) => write!(f, "prova rovinata ({che})"),
            ErroreFormato::ByteInPiu => f.write_str("byte in più dopo la prova"),
        }
    }
}

impl std::error::Error for ErroreFormato {}

/// Un passo da un messaggio al successivo.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Operazione {
    Sha1,
    Ripemd160,
    Sha256,
    /// Accoda questi byte al messaggio.
    Accoda(Vec<u8>),
    /// Mette questi byte prima del messaggio.
    Anteponi(Vec<u8>),
    /// Rovescia l'ordine dei byte.
    Rovescia,
    /// Il messaggio scritto in esadecimale.
    Esadecimale,
}

impl Operazione {
    fn codice(&self) -> u8 {
        match self {
            Operazione::Sha1 => SHA1,
            Operazione::Ripemd160 => RIPEMD160,
            Operazione::Sha256 => SHA256,
            Operazione::Accoda(_) => ACCODA,
            Operazione::Anteponi(_) => ANTEPONI,
            Operazione::Rovescia => ROVESCIA,
            Operazione::Esadecimale => ESADECIMALE,
        }
    }

    /// L'ordine del client: per codice, poi per argomento (`Op.__lt__`).
    fn chiave(&self) -> (u8, &[u8]) {
        match self {
            Operazione::Accoda(a) | Operazione::Anteponi(a) => (self.codice(), a),
            _ => (self.codice(), &[]),
        }
    }

    /// Il messaggio dopo il passo, coi limiti del client.
    pub fn applica(&self, messaggio: &[u8]) -> Result<Vec<u8>, ErroreFormato> {
        if messaggio.len() > MESSAGGIO_MASSIMO {
            return Err(ErroreFormato::Rovinata("messaggio troppo lungo"));
        }
        let fuori = match self {
            Operazione::Sha1 => Sha1::digest(messaggio).to_vec(),
            Operazione::Ripemd160 => Ripemd160::digest(messaggio).to_vec(),
            Operazione::Sha256 => Sha256::digest(messaggio).to_vec(),
            Operazione::Accoda(a) => [messaggio, a].concat(),
            Operazione::Anteponi(a) => [a.as_slice(), messaggio].concat(),
            Operazione::Rovescia if messaggio.is_empty() => return Err(ErroreFormato::Rovinata("rovescia un messaggio vuoto")),
            Operazione::Rovescia => messaggio.iter().rev().copied().collect(),
            Operazione::Esadecimale if messaggio.is_empty() => return Err(ErroreFormato::Rovinata("esadecimale di un messaggio vuoto")),
            Operazione::Esadecimale if messaggio.len() > MESSAGGIO_MASSIMO / 2 => {
                return Err(ErroreFormato::Rovinata("messaggio troppo lungo"));
            }
            Operazione::Esadecimale => esadecimale(messaggio).into_bytes(),
        };
        if fuori.len() > MESSAGGIO_MASSIMO {
            return Err(ErroreFormato::Rovinata("risultato troppo lungo"));
        }
        Ok(fuori)
    }
}

/// Chi attesta che un messaggio esisteva.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Attestazione {
    /// Un server che lo legherà a un blocco: il suo indirizzo, da chiedere più tardi.
    InAttesa(String),
    /// Il messaggio è la radice del blocco a questa altezza.
    Blocco(u64),
    /// Un'attestazione che qui non si conosce, tenuta com'era.
    Altra { codice: [u8; 8], dati: Vec<u8> },
}

impl Attestazione {
    fn codice(&self) -> [u8; 8] {
        match self {
            Attestazione::InAttesa(_) => IN_ATTESA,
            Attestazione::Blocco(_) => BLOCCO,
            Attestazione::Altra { codice, .. } => *codice,
        }
    }

    /// I dati come li scrive il client (`_serialize_payload`).
    fn dati(&self) -> Vec<u8> {
        let mut v = Vec::new();
        match self {
            Attestazione::InAttesa(indirizzo) => scrivi_byte_variabili(&mut v, indirizzo.as_bytes()),
            Attestazione::Blocco(altezza) => scrivi_varuint(&mut v, *altezza),
            Attestazione::Altra { dati, .. } => v.extend_from_slice(dati),
        }
        v
    }

    /// L'ordine del client: per codice, poi l'indirizzo, l'altezza o i dati (`__lt__`).
    fn precede(&self, altra: &Attestazione) -> std::cmp::Ordering {
        match (self, altra) {
            (Attestazione::InAttesa(a), Attestazione::InAttesa(b)) => a.cmp(b),
            (Attestazione::Blocco(a), Attestazione::Blocco(b)) => a.cmp(b),
            (Attestazione::Altra { codice: c1, dati: d1 }, Attestazione::Altra { codice: c2, dati: d2 }) => (c1, d1).cmp(&(c2, d2)),
            _ => self.codice().cmp(&altra.codice()),
        }
    }
}

/// Un messaggio, le attestazioni che lo riguardano e i rami verso i messaggi dopo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Albero {
    pub messaggio: Vec<u8>,
    /// In ordine, senza doppioni.
    pub attestazioni: Vec<Attestazione>,
    /// In ordine di operazione, una sola volta ciascuna.
    pub rami: Vec<(Operazione, Albero)>,
}

impl Albero {
    pub fn nuovo(messaggio: Vec<u8>) -> Albero {
        Albero { messaggio, attestazioni: Vec::new(), rami: Vec::new() }
    }

    /// L'albero dopo l'operazione `op`, creato se non c'è.
    pub fn ramo(&mut self, op: Operazione) -> Result<&mut Albero, ErroreFormato> {
        let i = match self.rami.binary_search_by(|(o, _)| o.chiave().cmp(&op.chiave())) {
            Ok(i) => i,
            Err(i) => {
                let messaggio = op.applica(&self.messaggio)?;
                self.rami.insert(i, (op, Albero::nuovo(messaggio)));
                i
            }
        };
        Ok(&mut self.rami[i].1)
    }

    pub fn aggiungi(&mut self, a: Attestazione) {
        if let Err(i) = self.attestazioni.binary_search_by(|x| x.precede(&a)) {
            self.attestazioni.insert(i, a);
        }
    }

    /// Unisce un albero dello stesso messaggio: le sue attestazioni e i suoi rami (`merge`).
    pub fn unisci(&mut self, altro: Albero) -> Result<(), ErroreFormato> {
        if altro.messaggio != self.messaggio {
            return Err(ErroreFormato::Rovinata("unione di messaggi diversi"));
        }
        for a in altro.attestazioni {
            self.aggiungi(a);
        }
        for (op, sotto) in altro.rami {
            self.ramo(op)?.unisci(sotto)?;
        }
        Ok(())
    }

    /// Tutte le attestazioni, ognuna col messaggio a cui si riferisce.
    pub fn tutte(&self) -> Vec<(&[u8], &Attestazione)> {
        let mut v: Vec<(&[u8], &Attestazione)> = self.attestazioni.iter().map(|a| (self.messaggio.as_slice(), a)).collect();
        for (_, sotto) in &self.rami {
            v.extend(sotto.tutte());
        }
        v
    }

    /// L'albero più in fondo col messaggio `messaggio`, se c'è.
    pub fn trova_mut(&mut self, messaggio: &[u8]) -> Option<&mut Albero> {
        if self.messaggio == messaggio {
            return Some(self);
        }
        self.rami.iter_mut().find_map(|(_, sotto)| sotto.trova_mut(messaggio))
    }

    /// Scrive l'albero come `Timestamp.serialize`: prima le attestazioni, poi i rami, con 0xff
    /// davanti a ogni voce che non è l'ultima.
    fn scrivi(&self, v: &mut Vec<u8>) -> Result<(), ErroreFormato> {
        let voci = self.attestazioni.len() + self.rami.len();
        if voci == 0 {
            return Err(ErroreFormato::Rovinata("albero vuoto"));
        }
        let mut fatte = 0;
        for a in &self.attestazioni {
            fatte += 1;
            if fatte < voci {
                v.push(ALTRO_RAMO);
            }
            v.push(ATTESTAZIONE);
            v.extend_from_slice(&a.codice());
            scrivi_byte_variabili(v, &a.dati());
        }
        for (op, sotto) in &self.rami {
            fatte += 1;
            if fatte < voci {
                v.push(ALTRO_RAMO);
            }
            v.push(op.codice());
            if let Operazione::Accoda(a) | Operazione::Anteponi(a) = op {
                scrivi_byte_variabili(v, a);
            }
            sotto.scrivi(v)?;
        }
        Ok(())
    }

    /// Legge un albero del messaggio `messaggio` (`Timestamp.deserialize`).
    fn leggi(l: &mut Lettore<'_>, messaggio: Vec<u8>, profondita: usize) -> Result<Albero, ErroreFormato> {
        if profondita == 0 {
            return Err(ErroreFormato::Rovinata("troppo profonda"));
        }
        let mut albero = Albero::nuovo(messaggio);
        loop {
            let mut segno = l.byte()?;
            let altri = segno == ALTRO_RAMO;
            if altri {
                segno = l.byte()?;
            }
            if segno == ATTESTAZIONE {
                let a = leggi_attestazione(l)?;
                albero.aggiungi(a);
            } else {
                let op = match segno {
                    SHA1 => Operazione::Sha1,
                    RIPEMD160 => Operazione::Ripemd160,
                    SHA256 => Operazione::Sha256,
                    ACCODA => Operazione::Accoda(l.byte_variabili(MESSAGGIO_MASSIMO, 1)?),
                    ANTEPONI => Operazione::Anteponi(l.byte_variabili(MESSAGGIO_MASSIMO, 1)?),
                    ROVESCIA => Operazione::Rovescia,
                    ESADECIMALE => Operazione::Esadecimale,
                    _ => return Err(ErroreFormato::NonSupportata("operazione")),
                };
                let dopo = op.applica(&albero.messaggio)?;
                let sotto = Albero::leggi(l, dopo, profondita - 1)?;
                albero.ramo(op)?.unisci(sotto)?;
            }
            if !altri {
                return Ok(albero);
            }
        }
    }
}

fn leggi_attestazione(l: &mut Lettore<'_>) -> Result<Attestazione, ErroreFormato> {
    let mut codice = [0u8; 8];
    codice.copy_from_slice(l.byte_fissi(8)?);
    let dati = l.byte_variabili(DATI_MASSIMI, 0)?;
    let mut d = Lettore::nuovo(&dati);
    let a = match codice {
        IN_ATTESA => {
            let indirizzo = d.byte_variabili(INDIRIZZO_MASSIMO, 0)?;
            if !indirizzo.iter().all(|c| CARATTERI_DEGLI_INDIRIZZI.contains(c)) {
                return Err(ErroreFormato::Rovinata("indirizzo con caratteri non ammessi"));
            }
            Attestazione::InAttesa(String::from_utf8(indirizzo).map_err(|_| ErroreFormato::Rovinata("indirizzo"))?)
        }
        BLOCCO => Attestazione::Blocco(d.varuint()?),
        _ => return Ok(Attestazione::Altra { codice, dati }),
    };
    if !d.finito() {
        return Err(ErroreFormato::Rovinata("dati dell'attestazione"));
    }
    Ok(a)
}

/// Una prova della data: l'impronta SHA-256 del file e l'albero che parte da lì.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prova {
    pub albero: Albero,
}

impl Prova {
    /// L'impronta del file di cui è la prova.
    pub fn impronta(&self) -> &[u8] {
        &self.albero.messaggio
    }

    /// Legge i byte di un file `.ots`.
    pub fn leggi(byte: &[u8]) -> Result<Prova, ErroreFormato> {
        let mut l = Lettore::nuovo(byte);
        if l.byte_fissi(INTESTAZIONE.len()).ok() != Some(INTESTAZIONE) {
            return Err(ErroreFormato::NonEUnaProva);
        }
        if l.varuint()? != VERSIONE {
            return Err(ErroreFormato::NonSupportata("versione"));
        }
        if l.byte()? != SHA256 {
            return Err(ErroreFormato::NonSupportata("impronta che non è SHA-256"));
        }
        let impronta = l.byte_fissi(32)?.to_vec();
        let albero = Albero::leggi(&mut l, impronta, PROFONDITA_MASSIMA)?;
        if !l.finito() {
            return Err(ErroreFormato::ByteInPiu);
        }
        Ok(Prova { albero })
    }

    /// I byte del file `.ots`.
    pub fn scrivi(&self) -> Result<Vec<u8>, ErroreFormato> {
        let mut v = INTESTAZIONE.to_vec();
        scrivi_varuint(&mut v, VERSIONE);
        v.push(SHA256);
        v.extend_from_slice(&self.albero.messaggio);
        self.albero.scrivi(&mut v)?;
        Ok(v)
    }
}

impl Prova {
    /// Una prova nuova per i byte di un file: l'impronta, il numero a caso accodato (così i server
    /// non vedono l'impronta) e SHA-256. Dà la prova, ancora senza attestazioni, e il messaggio da
    /// mandare ai server.
    pub fn nuova(byte: &[u8], numero: [u8; 16]) -> (Prova, Vec<u8>) {
        let mut albero = Albero::nuovo(Sha256::digest(byte).to_vec());
        let mandato = Operazione::Sha256.applica(&[albero.messaggio.as_slice(), &numero].concat()).unwrap_or_default();
        // Due passi che non possono fallire: 32 + 16 byte, poi l'impronta di 48 byte.
        if let Ok(dopo) = albero.ramo(Operazione::Accoda(numero.to_vec())) {
            let _ = dopo.ramo(Operazione::Sha256);
        }
        (Prova { albero }, mandato)
    }

    /// Unisce la risposta di un server (i byte del suo albero) al messaggio `mandato`.
    pub fn unisci_risposta(&mut self, mandato: &[u8], risposta: &[u8]) -> Result<(), ErroreFormato> {
        let albero = leggi_albero(risposta, mandato)?;
        self.albero.trova_mut(mandato).ok_or(ErroreFormato::Rovinata("messaggio che la prova non ha"))?.unisci(albero)
    }
}

/// I server a cui si chiede il seguito di un'attesa: quelli dell'elenco fidato del client
/// ufficiale (`DEFAULT_CALENDAR_WHITELIST`). Un indirizzo scritto in una prova non si chiama se non
/// è fra questi: una prova arriva da fuori.
const SERVER_FIDATI: [&str; 3] = [".calendar.opentimestamps.org", ".calendar.eternitywall.com", ".calendar.catallaxy.com"];

/// L'indirizzo è di un server fidato: https, un nome che finisce come uno dei loro, niente porta
/// né percorso (come `UrlWhitelist` del client).
pub fn fidato(indirizzo: &str) -> bool {
    let Some(nome) = indirizzo.strip_prefix("https://") else { return false };
    !nome.contains(['/', ':']) && SERVER_FIDATI.iter().any(|f| nome.len() > f.len() && nome.ends_with(f))
}

impl Prova {
    /// Le attese da chiedere ai server fidati: (messaggio, indirizzo). Vuoto se la prova ha già un
    /// blocco.
    pub fn attese(&self) -> Vec<(Vec<u8>, String)> {
        if !self.blocchi().is_empty() {
            return Vec::new();
        }
        self.albero
            .tutte()
            .into_iter()
            .filter_map(|(m, a)| match a {
                Attestazione::InAttesa(u) if fidato(u) => Some((m.to_vec(), u.clone())),
                _ => None,
            })
            .collect()
    }

    /// I blocchi della prova: (radice, altezza), con la radice di 32 byte.
    pub fn blocchi(&self) -> Vec<(Vec<u8>, u64)> {
        self.albero
            .tutte()
            .into_iter()
            .filter_map(|(m, a)| match a {
                Attestazione::Blocco(h) if m.len() == 32 => Some((m.to_vec(), *h)),
                _ => None,
            })
            .collect()
    }
}

/// I byte di un albero da solo, come li manda un server.
pub fn scrivi_albero(albero: &Albero) -> Result<Vec<u8>, ErroreFormato> {
    let mut v = Vec::new();
    albero.scrivi(&mut v)?;
    Ok(v)
}

/// Legge un albero mandato da un server per il messaggio `messaggio`: i byte sono solo l'albero,
/// senza intestazione (`Timestamp.deserialize` del client).
pub fn leggi_albero(byte: &[u8], messaggio: &[u8]) -> Result<Albero, ErroreFormato> {
    let mut l = Lettore::nuovo(byte);
    let albero = Albero::leggi(&mut l, messaggio.to_vec(), PROFONDITA_MASSIMA)?;
    if !l.finito() {
        return Err(ErroreFormato::ByteInPiu);
    }
    Ok(albero)
}

/// I byte in esadecimale minuscolo.
pub fn esadecimale(byte: &[u8]) -> String {
    byte.iter().map(|b| format!("{b:02x}")).collect()
}

/// Un numero senza segno in LEB128, come `write_varuint`.
fn scrivi_varuint(v: &mut Vec<u8>, mut n: u64) {
    loop {
        let b = (n & 0x7f) as u8;
        n >>= 7;
        if n == 0 {
            v.push(b);
            return;
        }
        v.push(b | 0x80);
    }
}

fn scrivi_byte_variabili(v: &mut Vec<u8>, byte: &[u8]) {
    scrivi_varuint(v, byte.len() as u64);
    v.extend_from_slice(byte);
}

/// Legge i byte uno dopo l'altro, senza mai uscire.
struct Lettore<'a> {
    byte: &'a [u8],
    posto: usize,
}

impl<'a> Lettore<'a> {
    fn nuovo(byte: &'a [u8]) -> Self {
        Lettore { byte, posto: 0 }
    }

    fn finito(&self) -> bool {
        self.posto == self.byte.len()
    }

    fn byte(&mut self) -> Result<u8, ErroreFormato> {
        let b = *self.byte.get(self.posto).ok_or(ErroreFormato::Troncata)?;
        self.posto += 1;
        Ok(b)
    }

    fn byte_fissi(&mut self, n: usize) -> Result<&'a [u8], ErroreFormato> {
        let fine = self.posto.checked_add(n).ok_or(ErroreFormato::Troncata)?;
        let pezzo = self.byte.get(self.posto..fine).ok_or(ErroreFormato::Troncata)?;
        self.posto = fine;
        Ok(pezzo)
    }

    /// Un numero LEB128 di al più 64 bit.
    fn varuint(&mut self) -> Result<u64, ErroreFormato> {
        let mut n: u64 = 0;
        for spostamento in (0..64).step_by(7) {
            let b = self.byte()?;
            let pezzo = u64::from(b & 0x7f);
            if spostamento == 63 && pezzo > 1 {
                return Err(ErroreFormato::Rovinata("numero troppo grande"));
            }
            n |= pezzo << spostamento;
            if b & 0x80 == 0 {
                return Ok(n);
            }
        }
        Err(ErroreFormato::Rovinata("numero troppo grande"))
    }

    fn byte_variabili(&mut self, massimo: usize, minimo: usize) -> Result<Vec<u8>, ErroreFormato> {
        let n = self.varuint()?;
        let n = usize::try_from(n).map_err(|_| ErroreFormato::Rovinata("lunghezza"))?;
        if n > massimo || n < minimo {
            return Err(ErroreFormato::Rovinata("lunghezza"));
        }
        Ok(self.byte_fissi(n)?.to_vec())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const IN_ATTESA_TXT: &[u8] = include_bytes!("../tests/fixtures/in-attesa.txt");
    const IN_ATTESA_OTS: &[u8] = include_bytes!("../tests/fixtures/in-attesa.txt.ots");
    const ESEMPIO_TXT: &[u8] = include_bytes!("../tests/fixtures/hello-world.txt");
    const ESEMPIO_OTS: &[u8] = include_bytes!("../tests/fixtures/hello-world.txt.ots");

    #[test]
    fn la_prova_del_client_ufficiale_si_legge_e_si_riscrive_uguale() {
        let p = Prova::leggi(IN_ATTESA_OTS).expect("si legge");
        assert_eq!(p.impronta(), Sha256::digest(IN_ATTESA_TXT).as_slice());
        let attese: Vec<&str> =
            p.albero.tutte().into_iter().filter_map(|(_, a)| if let Attestazione::InAttesa(u) = a { Some(u.as_str()) } else { None }).collect();
        assert_eq!(attese.len(), 4, "{attese:?}");
        assert!(attese.iter().all(|u| u.starts_with("https://")), "{attese:?}");
        assert_eq!(p.scrivi().unwrap(), IN_ATTESA_OTS, "riscritta non è uguale");
    }

    #[test]
    fn l_esempio_pubblico_ha_il_blocco_e_si_riscrive_uguale() {
        let p = Prova::leggi(ESEMPIO_OTS).expect("si legge");
        assert_eq!(p.impronta(), Sha256::digest(ESEMPIO_TXT).as_slice());
        let blocchi: Vec<(&[u8], u64)> =
            p.albero.tutte().into_iter().filter_map(|(m, a)| if let Attestazione::Blocco(h) = a { Some((m, *h)) } else { None }).collect();
        assert_eq!(blocchi.len(), 1);
        assert_eq!(blocchi[0].1, 358391);
        // La radice del blocco, nell'ordine dell'intestazione (rovesciato rispetto a quello che si legge).
        assert_eq!(esadecimale(blocchi[0].0), "007ee445d23ad061af4a36b809501fab1ac4f2d7e7a739817dd0cbb7ec661b8a");
        assert_eq!(p.scrivi().unwrap(), ESEMPIO_OTS);
    }

    #[test]
    fn una_prova_rovinata_da_errore_senza_panico() {
        assert_eq!(Prova::leggi(b"non una prova"), Err(ErroreFormato::NonEUnaProva));
        for n in 0..IN_ATTESA_OTS.len() {
            assert!(Prova::leggi(&IN_ATTESA_OTS[..n]).is_err(), "troncata a {n} byte si legge");
        }
        let mut in_piu = IN_ATTESA_OTS.to_vec();
        in_piu.push(0);
        assert_eq!(Prova::leggi(&in_piu), Err(ErroreFormato::ByteInPiu));
        // Un'operazione sconosciuta subito dopo l'impronta.
        let mut strana = IN_ATTESA_OTS[..INTESTAZIONE.len() + 2 + 32].to_vec();
        strana.push(0x67);
        assert_eq!(Prova::leggi(&strana), Err(ErroreFormato::NonSupportata("operazione")));
        // Un indirizzo con un carattere non ammesso.
        let mut v = INTESTAZIONE.to_vec();
        v.extend([1, SHA256]);
        v.extend([0u8; 32]);
        v.push(ATTESTAZIONE);
        v.extend(IN_ATTESA);
        let mut dati = Vec::new();
        scrivi_byte_variabili(&mut dati, b"https://cattivo.it/?x=1");
        scrivi_byte_variabili(&mut v, &dati);
        assert_eq!(Prova::leggi(&v), Err(ErroreFormato::Rovinata("indirizzo con caratteri non ammessi")));
    }

    /// Il numero a caso della prova in attesa, il primo passo dopo l'impronta.
    const NUMERO_IN_ATTESA: [u8; 16] = [0xc8, 0x2b, 0xe9, 0xcf, 0x8b, 0x02, 0x69, 0x40, 0x34, 0xdc, 0xb7, 0x58, 0xe4, 0x09, 0x7b, 0x35];

    #[test]
    fn una_prova_nuova_con_le_risposte_dei_server_e_quella_del_client() {
        // La prova nuova dello stesso file con lo stesso numero a caso, e come risposte dei server
        // i rami della prova del client: deve venire uguale byte per byte.
        let (mut nuova, mandato) = Prova::nuova(IN_ATTESA_TXT, NUMERO_IN_ATTESA);
        let client = Prova::leggi(IN_ATTESA_OTS).unwrap();
        let (_, dopo_il_numero) = &client.albero.rami[0];
        let (_, punta) = &dopo_il_numero.rami[0];
        assert_eq!(punta.messaggio, mandato, "il messaggio mandato ai server");
        for ramo in &punta.rami {
            let risposta = Albero { messaggio: mandato.clone(), attestazioni: Vec::new(), rami: vec![ramo.clone()] };
            let byte = scrivi_albero(&risposta).unwrap();
            nuova.unisci_risposta(&mandato, &byte).expect("risposta unita");
        }
        assert_eq!(nuova.scrivi().unwrap(), IN_ATTESA_OTS);
        // Una risposta per un altro messaggio non si unisce.
        assert!(nuova.unisci_risposta(&[0u8; 32], &scrivi_albero(&punta.rami[0].1).unwrap()).is_err());
    }

    #[test]
    fn le_attese_solo_verso_i_server_fidati_e_i_blocchi() {
        let mut p = Prova::leggi(IN_ATTESA_OTS).unwrap();
        let attese = p.attese();
        assert_eq!(attese.len(), 4);
        assert!(p.blocchi().is_empty());
        // Un indirizzo che non è dei server fidati, scritto nella prova: non si chiede.
        let m = attese[0].0.clone();
        p.albero.trova_mut(&m).unwrap().aggiungi(Attestazione::InAttesa("https://example.com".into()));
        assert_eq!(p.attese().len(), 4);
        let e = Prova::leggi(ESEMPIO_OTS).unwrap();
        assert_eq!(e.blocchi().len(), 1);
        assert_eq!(e.blocchi()[0].1, 358391);
        assert!(e.attese().is_empty());
    }

    #[test]
    fn i_server_fidati() {
        for si in ["https://alice.btc.calendar.opentimestamps.org", "https://finney.calendar.eternitywall.com", "https://btc.calendar.catallaxy.com"]
        {
            assert!(fidato(si), "{si}");
        }
        for no in [
            "http://alice.btc.calendar.opentimestamps.org",
            "https://alice.btc.calendar.opentimestamps.org/",
            "https://alice.btc.calendar.opentimestamps.org:8080",
            "https://calendar.opentimestamps.org.cattivo.it",
            "https://cattivo.it/.calendar.opentimestamps.org",
            "https://.calendar.opentimestamps.org",
            "https://a.pool.opentimestamps.org",
        ] {
            assert!(!fidato(no), "{no}");
        }
    }

    #[test]
    fn senza_risposte_una_prova_non_si_scrive() {
        let (nuova, _) = Prova::nuova(b"qualcosa", [7; 16]);
        assert_eq!(nuova.scrivi(), Err(ErroreFormato::Rovinata("albero vuoto")));
    }

    /// Il client ufficiale, se c'è sul PC, legge una prova fatta qui.
    #[test]
    fn il_client_ufficiale_legge_una_prova_fatta_qui() {
        let ots = std::path::Path::new(env!("HOME")).join(".local/bin/ots");
        if !ots.exists() {
            eprintln!("ots non c'è: prova saltata");
            return;
        }
        let (mut p, mandato) = Prova::nuova(b"Doc Tascabile", [1; 16]);
        p.albero.trova_mut(&mandato).unwrap().aggiungi(Attestazione::InAttesa("https://alice.btc.calendar.opentimestamps.org".into()));
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/prove-prova-data");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("fatta-qui.ots");
        std::fs::write(&file, p.scrivi().unwrap()).unwrap();
        let fuori = std::process::Command::new(ots).arg("info").arg(&file).output().expect("ots parte");
        let testo = String::from_utf8_lossy(&fuori.stdout);
        assert!(fuori.status.success(), "{}", String::from_utf8_lossy(&fuori.stderr));
        assert!(testo.contains(&esadecimale(&Sha256::digest(b"Doc Tascabile"))), "{testo}");
        assert!(testo.contains("PendingAttestation('https://alice.btc.calendar.opentimestamps.org')"), "{testo}");
    }

    #[test]
    fn i_numeri_si_scrivono_e_si_leggono_come_nel_client() {
        for n in [0u64, 1, 127, 128, 300, 358391, u64::from(u32::MAX), u64::MAX] {
            let mut v = Vec::new();
            scrivi_varuint(&mut v, n);
            assert_eq!(Lettore::nuovo(&v).varuint(), Ok(n), "{n}");
        }
        let mut v = Vec::new();
        scrivi_varuint(&mut v, 300);
        assert_eq!(v, [0xac, 0x02]);
    }
}
