//! Il dialogo coi server di OpenTimestamps e con gli esploratori dei blocchi, attraverso una
//! [`Rete`] che dà l'app (su Android il Java del MainActivity). Fissare: l'impronta col numero a
//! caso va ai quattro aggregatori del client ufficiale, in parallelo, e servono almeno due
//! risposte, come nel client. Controllare: se il file è quello della prova, i server fidati danno
//! il seguito delle attese, e il blocco si controlla con l'intestazione presa da un esploratore.

use sha2::{Digest, Sha256};

use crate::blocco::{BloccoControllato, ErroreBlocco, controlla_intestazione_all_altezza};
use crate::formato::{ErroreFormato, Prova, esadecimale, leggi_albero};

/// I quattro aggregatori del client ufficiale (`DEFAULT_AGGREGATORS`).
pub const AGGREGATORI: [&str; 4] =
    ["https://a.pool.opentimestamps.org", "https://b.pool.opentimestamps.org", "https://a.pool.eternitywall.com", "https://ots.btc.catallaxy.com"];
/// Gli esploratori dei blocchi, con la stessa interfaccia: si interpellano tutti per confrontarli.
pub const ESPLORATORI: [&str; 2] = ["https://blockstream.info/api", "https://mempool.space/api"];
/// Al più tante attese chieste per un controllo, una per server, e al più tante richieste insieme:
/// un .ots costruito apposta può averne migliaia (revisione del 31/10, punto 7).
pub const ATTESE_MASSIME: usize = 8;
const RICHIESTE_INSIEME: usize = 4;
/// Al più tante attese tenute per ogni risposta di aggregatore: con 4 aggregatori restano dentro
/// `ATTESE_MASSIME` (8), così un solo server ostile non può escludere i calendari degli altri.
const ATTESE_PER_AGGREGATORE: usize = 2;
/// La misura massima di una prova `.ots` letta dall'app: una fusione non deve mai superarla.
const PROVA_MASSIMA: usize = 64 * 1024;

/// Le attese da chiedere: una per server, le prime `ATTESE_MASSIME`.
fn da_chiedere(attese: Vec<(Vec<u8>, String)>) -> Vec<(Vec<u8>, String)> {
    let mut server = std::collections::HashSet::new();
    attese.into_iter().filter(|(_, u)| server.insert(u.clone())).take(ATTESE_MASSIME).collect()
}

/// Quanti aggregatori devono rispondere perché la prova valga (il `-m 2` del client).
pub const RISPOSTE_MINIME: usize = 2;
/// Una risposta più lunga di così non si legge (come nel client).
pub const RISPOSTA_MASSIMA: usize = 10_000;
/// Come si presenta l'app ai server (il client usa l'Accept del formato).
pub const ACCETTA: &str = "application/vnd.opentimestamps.v1";

/// Che cosa non è andato nella rete. Mai l'indirizzo: ha l'impronta dentro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroreRete {
    /// Niente collegamento, o nessuna risposta in tempo.
    Collegamento,
    /// Il server non ha quello che si chiede (404): per un'attesa, non è ancora pronta.
    NonTrovato,
    /// Un'altra risposta che non è 200.
    Risposta(u16),
    /// Più lunga di [`RISPOSTA_MASSIMA`].
    TroppoLunga,
}

/// La risposta di un indirizzo: i byte o perché non ci sono.
pub type Risposta = Result<Vec<u8>, ErroreRete>;

/// Chiedere byte a un indirizzo https: lo fa l'app (su Android il Java, con la sua connessione
/// sicura). Si chiama da più thread insieme.
pub trait Rete: Sync {
    /// Manda `corpo` con POST e dà la risposta.
    fn post(&self, indirizzo: &str, corpo: &[u8]) -> Result<Vec<u8>, ErroreRete>;
    /// Chiede con GET e dà la risposta.
    fn get(&self, indirizzo: &str) -> Result<Vec<u8>, ErroreRete>;
}

/// Perché fissare o controllare non è riuscito.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroreData {
    /// Hanno risposto troppo pochi server (quanti), di solito perché manca internet.
    PocheRisposte(usize),
    /// La prova non si legge o non si scrive.
    Formato(ErroreFormato),
    /// Il blocco non regge: la prova non è quella che dice di essere.
    Blocco(ErroreBlocco),
}

impl std::fmt::Display for ErroreData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErroreData::PocheRisposte(n) => write!(f, "{n} server su {} hanno risposto", AGGREGATORI.len()),
            ErroreData::Formato(e) => e.fmt(f),
            ErroreData::Blocco(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for ErroreData {}

impl From<ErroreFormato> for ErroreData {
    fn from(e: ErroreFormato) -> Self {
        ErroreData::Formato(e)
    }
}

/// La prova nuova dei byte `byte`, coi server della rete `rete` e il numero a caso `numero`: i byte
/// del file `.ots`.
pub fn fissa(byte: &[u8], rete: &dyn Rete, numero: [u8; 16]) -> Result<Vec<u8>, ErroreData> {
    let (mut prova, mandato) = Prova::nuova(byte, numero);
    let risposte: Vec<Risposta> = std::thread::scope(|s| {
        let mandato = &mandato;
        let lavori: Vec<_> = AGGREGATORI
            .iter()
            .map(|a| {
                let indirizzo = format!("{a}/digest");
                s.spawn(move || rete.post(&indirizzo, mandato))
            })
            .collect();
        lavori.into_iter().map(|l| l.join().unwrap_or(Err(ErroreRete::Collegamento))).collect()
    });
    let mut unite = 0;
    for r in risposte.into_iter().flatten() {
        if r.len() > RISPOSTA_MASSIMA {
            continue;
        }
        let Ok(albero) = leggi_albero(&r, &mandato) else { continue };
        let Some(albero) = albero.solo_attese_fidate(ATTESE_PER_AGGREGATORE) else { continue };
        let mut copia = prova.clone();
        let valida = copia.albero.trova_mut(&mandato).is_some_and(|n| n.unisci(albero).is_ok())
            && copia.scrivi().is_ok_and(|b| b.len() <= PROVA_MASSIMA && Prova::leggi(&b).is_ok());
        if valida {
            prova = copia;
            unite += 1;
        }
    }
    if unite < RISPOSTE_MINIME {
        return Err(ErroreData::PocheRisposte(unite));
    }
    Ok(prova.scrivi()?)
}

/// Com'è la prova di un file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stato {
    /// Il file non è quello della prova: è cambiato dopo, o è un altro.
    Diverso,
    /// I server non hanno ancora legato la prova a un blocco.
    InAttesa,
    /// Confermata: il file esisteva così all'ora del blocco (secondi dal 1970, UTC).
    Confermata { ora: u32, altezza: u64 },
    /// Non si è potuto chiedere niente a nessuno: manca internet.
    SenzaRete,
}

/// Il controllo: lo stato, e la prova aggiornata da salvare se i server ne hanno dato il seguito.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Controllo {
    pub stato: Stato,
    pub aggiornata: Option<Vec<u8>>,
}

/// Esito della verifica di un singolo blocco presso tutti gli esploratori.
enum EsitoBlocco {
    Confermato(u32),
    Errore(ErroreBlocco),
    NessunaRisposta,
}

/// Chiede hash e intestazione del blocco a tutti gli esploratori: se due esploratori danno hash
/// diversi è errore (`EsploratoriDiscordi`); se almeno uno conferma con hash e intestazione validi
/// vale quella conferma; se nessuno conferma, torna il primo errore o `NessunaRisposta`.
fn controlla_blocco_agli_esploratori(altezza: u64, radice: &[u8], rete: &dyn Rete) -> EsitoBlocco {
    let mut hash_visto: Option<String> = None;
    let mut confermato: Option<BloccoControllato> = None;
    let mut primo_errore: Option<ErroreBlocco> = None;
    for e in ESPLORATORI {
        let Ok(hash) = rete.get(&format!("{e}/block-height/{altezza}")) else { continue };
        let Some(hash) = esadecimale_valido(&hash, 64) else { continue };
        if let Some(ref altro) = hash_visto
            && altro != &hash
        {
            return EsitoBlocco::Errore(ErroreBlocco::EsploratoriDiscordi);
        }
        hash_visto = Some(hash.clone());
        let Ok(intestazione) = rete.get(&format!("{e}/block/{hash}/header")) else { continue };
        let Some(intestazione) = esadecimale_valido(&intestazione, 160).and_then(|h| da_esadecimale(&h)) else { continue };
        match controlla_intestazione_all_altezza(&intestazione, radice, altezza) {
            Ok(b) if b.hash == hash => {
                if confermato.is_none() {
                    confermato = Some(b);
                }
            }
            Ok(_) => {
                if primo_errore.is_none() {
                    primo_errore = Some(ErroreBlocco::LavoroNonTorna);
                }
            }
            Err(err) => {
                if primo_errore.is_none() {
                    primo_errore = Some(err);
                }
            }
        }
    }
    match confermato {
        Some(b) => EsitoBlocco::Confermato(b.ora),
        None => match primo_errore {
            Some(err) => EsitoBlocco::Errore(err),
            None => EsitoBlocco::NessunaRisposta,
        },
    }
}

/// Controlla la prova `prova` (i byte del `.ots`) del file `byte`.
pub fn controlla(byte: &[u8], prova: &[u8], rete: &dyn Rete) -> Result<Controllo, ErroreData> {
    let mut p = Prova::leggi(prova)?;
    if Sha256::digest(byte).as_slice() != p.impronta() {
        return Ok(Controllo { stato: Stato::Diverso, aggiornata: None });
    }
    let mut aggiornata = None;
    let mut qualcuno = false;
    let attese = da_chiedere(p.attese());
    if !attese.is_empty() {
        let risposte: Vec<(Vec<u8>, Risposta)> = attese
            .chunks(RICHIESTE_INSIEME)
            .flat_map(|gruppo| {
                std::thread::scope(|s| {
                    let lavori: Vec<_> = gruppo
                        .iter()
                        .map(|(m, u)| {
                            let indirizzo = format!("{u}/timestamp/{}", esadecimale(m));
                            (m.clone(), s.spawn(move || rete.get(&indirizzo)))
                        })
                        .collect();
                    lavori.into_iter().map(|(m, l)| (m, l.join().unwrap_or(Err(ErroreRete::Collegamento)))).collect::<Vec<_>>()
                })
            })
            .collect();
        let mut cambiata = false;
        for (m, r) in risposte {
            match r {
                Ok(r) => {
                    qualcuno = true;
                    if r.len() <= RISPOSTA_MASSIMA {
                        let mut copia = p.clone();
                        if copia.unisci_risposta(&m, &r).is_ok() && copia.scrivi().is_ok_and(|b| b.len() <= PROVA_MASSIMA && Prova::leggi(&b).is_ok())
                        {
                            p = copia;
                            cambiata = true;
                        }
                    }
                }
                Err(ErroreRete::Collegamento) => {}
                Err(_) => qualcuno = true,
            }
        }
        if cambiata {
            aggiornata = Some(p.scrivi()?);
        }
    }
    let mut blocchi = p.blocchi();
    if blocchi.is_empty() {
        let stato = if attese.is_empty() || qualcuno { Stato::InAttesa } else { Stato::SenzaRete };
        return Ok(Controllo { stato, aggiornata: None });
    }
    // Tutti i blocchi, dal più vecchio: il primo che regge dimostra la data più presto.
    blocchi.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
    blocchi.dedup();
    let mut primo_errore = None;
    for (radice, altezza) in blocchi {
        match controlla_blocco_agli_esploratori(altezza, &radice, rete) {
            EsitoBlocco::Confermato(ora) => {
                return Ok(Controllo { stato: Stato::Confermata { ora, altezza }, aggiornata });
            }
            EsitoBlocco::Errore(e) => {
                if primo_errore.is_none() {
                    primo_errore = Some(e);
                }
            }
            EsitoBlocco::NessunaRisposta => {}
        }
    }
    if let Some(e) = primo_errore {
        return Err(ErroreData::Blocco(e));
    }
    Ok(Controllo { stato: Stato::SenzaRete, aggiornata: None })
}

/// Il testo di una risposta, se è esadecimale minuscolo della lunghezza `n` (spazi ai lati tolti).
fn esadecimale_valido(byte: &[u8], n: usize) -> Option<String> {
    let t = std::str::from_utf8(byte).ok()?.trim();
    (t.len() == n && t.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))).then(|| t.to_string())
}

fn da_esadecimale(t: &str) -> Option<Vec<u8>> {
    (0..t.len()).step_by(2).map(|i| u8::from_str_radix(t.get(i..i + 2)?, 16).ok()).collect()
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::formato::{Albero, Attestazione, scrivi_albero};
    use std::collections::HashMap;
    use std::sync::Mutex;

    const IN_ATTESA_TXT: &[u8] = include_bytes!("../tests/fixtures/in-attesa.txt");
    const IN_ATTESA_OTS: &[u8] = include_bytes!("../tests/fixtures/in-attesa.txt.ots");
    const ESEMPIO_TXT: &[u8] = include_bytes!("../tests/fixtures/hello-world.txt");
    const ESEMPIO_OTS: &[u8] = include_bytes!("../tests/fixtures/hello-world.txt.ots");
    const HASH_358391: &str = "000000000000000003e892881a8cdcdc117c06d444057c98b6f04a9ee75a2319";
    const NUMERO_IN_ATTESA: [u8; 16] = [0xc8, 0x2b, 0xe9, 0xcf, 0x8b, 0x02, 0x69, 0x40, 0x34, 0xdc, 0xb7, 0x58, 0xe4, 0x09, 0x7b, 0x35];

    /// Una rete finta: per ogni indirizzo la sua risposta; quello che non c'è non si collega. Tiene
    /// le chiamate.
    #[derive(Default)]
    struct ReteFinta {
        risposte: HashMap<String, Result<Vec<u8>, ErroreRete>>,
        chiamate: Mutex<Vec<String>>,
    }

    impl ReteFinta {
        fn con(mut self, indirizzo: &str, risposta: Result<Vec<u8>, ErroreRete>) -> Self {
            self.risposte.insert(indirizzo.to_string(), risposta);
            self
        }

        fn rispondi(&self, indirizzo: &str) -> Result<Vec<u8>, ErroreRete> {
            self.chiamate.lock().unwrap().push(indirizzo.to_string());
            self.risposte.get(indirizzo).cloned().unwrap_or(Err(ErroreRete::Collegamento))
        }

        fn chiamate(&self) -> Vec<String> {
            self.chiamate.lock().unwrap().clone()
        }
    }

    impl Rete for ReteFinta {
        fn post(&self, indirizzo: &str, _corpo: &[u8]) -> Result<Vec<u8>, ErroreRete> {
            self.rispondi(indirizzo)
        }

        fn get(&self, indirizzo: &str) -> Result<Vec<u8>, ErroreRete> {
            self.rispondi(indirizzo)
        }
    }

    /// Le quattro risposte vere dei server nella prova in attesa, una per ramo dopo la punta, e il
    /// messaggio mandato.
    fn risposte_vere() -> (Vec<u8>, Vec<Vec<u8>>) {
        let p = Prova::leggi(IN_ATTESA_OTS).unwrap();
        let punta = &p.albero.rami[0].1.rami[0].1;
        let risposte = punta
            .rami
            .iter()
            .map(|r| scrivi_albero(&Albero { messaggio: punta.messaggio.clone(), attestazioni: Vec::new(), rami: vec![r.clone()] }).unwrap())
            .collect();
        (punta.messaggio.clone(), risposte)
    }

    fn rete_coi_server(quanti: usize) -> ReteFinta {
        let (_, risposte) = risposte_vere();
        AGGREGATORI.iter().zip(risposte).take(quanti).fold(ReteFinta::default(), |r, (a, b)| r.con(&format!("{a}/digest"), Ok(b)))
    }

    #[test]
    fn fissare_coi_quattro_server_da_la_prova_del_client() {
        let rete = rete_coi_server(4);
        assert_eq!(fissa(IN_ATTESA_TXT, &rete, NUMERO_IN_ATTESA).unwrap(), IN_ATTESA_OTS);
        let mut chiamate = rete.chiamate();
        chiamate.sort();
        let mut attese: Vec<String> = AGGREGATORI.iter().map(|a| format!("{a}/digest")).collect();
        attese.sort();
        assert_eq!(chiamate, attese);
    }

    #[test]
    fn bastano_due_server_ma_non_uno() {
        let due = fissa(IN_ATTESA_TXT, &rete_coi_server(2), NUMERO_IN_ATTESA).unwrap();
        assert_eq!(Prova::leggi(&due).unwrap().attese().len(), 2);
        assert_eq!(fissa(IN_ATTESA_TXT, &rete_coi_server(1), NUMERO_IN_ATTESA), Err(ErroreData::PocheRisposte(1)));
        assert_eq!(fissa(IN_ATTESA_TXT, &ReteFinta::default(), NUMERO_IN_ATTESA), Err(ErroreData::PocheRisposte(0)));
        // Una risposta rovinata non conta.
        let rovinata = rete_coi_server(1).con(&format!("{}/digest", AGGREGATORI[3]), Ok(b"spazzatura".to_vec()));
        assert_eq!(fissa(IN_ATTESA_TXT, &rovinata, NUMERO_IN_ATTESA), Err(ErroreData::PocheRisposte(1)));
    }

    #[test]
    fn un_file_cambiato_non_e_quello_della_prova() {
        let rete = ReteFinta::default();
        let c = controlla(b"un altro file", IN_ATTESA_OTS, &rete).unwrap();
        assert_eq!(c, Controllo { stato: Stato::Diverso, aggiornata: None });
        assert!(rete.chiamate().is_empty(), "per un file diverso non si chiede niente");
    }

    #[test]
    fn una_prova_in_attesa_resta_in_attesa_o_dice_che_manca_la_rete() {
        let attese = Prova::leggi(IN_ATTESA_OTS).unwrap().attese();
        let rete =
            attese.iter().fold(ReteFinta::default(), |r, (m, u)| r.con(&format!("{u}/timestamp/{}", esadecimale(m)), Err(ErroreRete::NonTrovato)));
        let c = controlla(IN_ATTESA_TXT, IN_ATTESA_OTS, &rete).unwrap();
        assert_eq!(c, Controllo { stato: Stato::InAttesa, aggiornata: None });
        assert_eq!(rete.chiamate().len(), 4);
        let c = controlla(IN_ATTESA_TXT, IN_ATTESA_OTS, &ReteFinta::default()).unwrap();
        assert_eq!(c.stato, Stato::SenzaRete);
    }

    /// L'esempio pubblico con la strada verso il blocco tolta da un punto in giù e al suo posto
    /// un'attesa: la strada tolta è quello che il server darà. Dà la prova in attesa, il messaggio
    /// del punto e la risposta del server.
    fn esempio_in_attesa(server: &str) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let mut p = Prova::leggi(ESEMPIO_OTS).unwrap();
        // Il punto: tre passi dopo l'impronta, sulla strada verso il blocco.
        let mut punto = &mut p.albero;
        for _ in 0..3 {
            punto = &mut punto.rami[0].1;
        }
        let tolta = Albero { messaggio: punto.messaggio.clone(), attestazioni: Vec::new(), rami: std::mem::take(&mut punto.rami) };
        punto.aggiungi(Attestazione::InAttesa(server.into()));
        let messaggio = punto.messaggio.clone();
        (p.scrivi().unwrap(), messaggio, scrivi_albero(&tolta).unwrap())
    }

    fn rete_dell_esempio(esploratore: &str) -> ReteFinta {
        ReteFinta::default()
            .con(&format!("{esploratore}/block-height/358391"), Ok(format!("{HASH_358391}\n").into_bytes()))
            .con(&format!("{esploratore}/block/{HASH_358391}/header"), Ok(include_bytes!("../tests/fixtures/blocco-358391.hex").to_vec()))
    }

    #[test]
    fn una_prova_si_completa_e_si_conferma_col_blocco() {
        let server = "https://alice.btc.calendar.opentimestamps.org";
        let (in_attesa, messaggio, seguito) = esempio_in_attesa(server);
        let rete = rete_dell_esempio(ESPLORATORI[0]).con(&format!("{server}/timestamp/{}", esadecimale(&messaggio)), Ok(seguito));
        let c = controlla(ESEMPIO_TXT, &in_attesa, &rete).unwrap();
        assert_eq!(c.stato, Stato::Confermata { ora: 1_432_827_678, altezza: 358391 });
        let aggiornata = Prova::leggi(c.aggiornata.as_ref().expect("la prova aggiornata")).unwrap();
        assert_eq!(aggiornata.blocchi().len(), 1);
        // La prova aggiornata si conferma da sola, senza chiedere ai server.
        let rete = rete_dell_esempio(ESPLORATORI[0]);
        let c = controlla(ESEMPIO_TXT, c.aggiornata.as_ref().unwrap(), &rete).unwrap();
        assert_eq!(c, Controllo { stato: Stato::Confermata { ora: 1_432_827_678, altezza: 358391 }, aggiornata: None });
        // Solo esploratori, nessun calendario (dalla 0.10.1 si sentono tutti e due gli esploratori).
        assert!(rete.chiamate().iter().all(|u| ESPLORATORI.iter().any(|e| u.starts_with(e))), "{:?}", rete.chiamate());
    }

    #[test]
    fn se_il_primo_esploratore_non_risponde_si_chiede_al_secondo() {
        let rete = rete_dell_esempio(ESPLORATORI[1]);
        let c = controlla(ESEMPIO_TXT, ESEMPIO_OTS, &rete).unwrap();
        assert_eq!(c.stato, Stato::Confermata { ora: 1_432_827_678, altezza: 358391 });
        // Nessuno dei due: manca la rete.
        assert_eq!(controlla(ESEMPIO_TXT, ESEMPIO_OTS, &ReteFinta::default()).unwrap().stato, Stato::SenzaRete);
    }

    #[test]
    fn un_esploratore_che_imbroglia_non_passa() {
        // L'intestazione con un byte cambiato.
        let mut finta = include_str!("../tests/fixtures/blocco-358391.hex").trim().to_string();
        finta.replace_range(158..160, "00");
        let rete = ReteFinta::default()
            .con(&format!("{}/block-height/358391", ESPLORATORI[0]), Ok(HASH_358391.as_bytes().to_vec()))
            .con(&format!("{}/block/{HASH_358391}/header", ESPLORATORI[0]), Ok(finta.into_bytes()));
        assert_eq!(controlla(ESEMPIO_TXT, ESEMPIO_OTS, &rete), Err(ErroreData::Blocco(ErroreBlocco::LavoroNonTorna)));
        // Un hash che non è quello dell'intestazione.
        let altro = format!("1{}", &HASH_358391[1..]);
        let rete = ReteFinta::default()
            .con(&format!("{}/block-height/358391", ESPLORATORI[0]), Ok(altro.clone().into_bytes()))
            .con(&format!("{}/block/{altro}/header", ESPLORATORI[0]), Ok(include_bytes!("../tests/fixtures/blocco-358391.hex").to_vec()));
        assert_eq!(controlla(ESEMPIO_TXT, ESEMPIO_OTS, &rete), Err(ErroreData::Blocco(ErroreBlocco::LavoroNonTorna)));
    }

    #[test]
    fn un_server_non_fidato_nella_prova_non_si_chiama() {
        let (in_attesa, _, _) = esempio_in_attesa("https://example.com");
        let rete = ReteFinta::default();
        let c = controlla(ESEMPIO_TXT, &in_attesa, &rete).unwrap();
        assert_eq!(c.stato, Stato::InAttesa);
        assert!(rete.chiamate().is_empty(), "{:?}", rete.chiamate());
    }

    /// La rete vera del PC, con curl: per le prove a mano sui server veri.
    struct ReteCurl;

    impl ReteCurl {
        fn curl(argomenti: &[&str]) -> Result<Vec<u8>, ErroreRete> {
            let fuori = std::process::Command::new("curl")
                .args(["-sS", "--max-time", "15", "-w", "\n%{http_code}", "-H", &format!("Accept: {ACCETTA}"), "-A", "DocTascabile"])
                .args(argomenti)
                .output()
                .map_err(|_| ErroreRete::Collegamento)?;
            let mut corpo = fuori.stdout;
            let fine = corpo.iter().rposition(|b| *b == b'\n').ok_or(ErroreRete::Collegamento)?;
            let codice: u16 = std::str::from_utf8(&corpo[fine + 1..]).ok().and_then(|c| c.trim().parse().ok()).ok_or(ErroreRete::Collegamento)?;
            corpo.truncate(fine);
            match codice {
                200 => Ok(corpo),
                404 => Err(ErroreRete::NonTrovato),
                0 => Err(ErroreRete::Collegamento),
                c => Err(ErroreRete::Risposta(c)),
            }
        }
    }

    impl Rete for ReteCurl {
        fn post(&self, indirizzo: &str, corpo: &[u8]) -> Result<Vec<u8>, ErroreRete> {
            let file = std::env::temp_dir().join(format!("corpo-ots-{}", std::process::id()));
            std::fs::write(&file, corpo).map_err(|_| ErroreRete::Collegamento)?;
            let r = Self::curl(&["--data-binary", &format!("@{}", file.display()), indirizzo]);
            let _ = std::fs::remove_file(&file);
            r
        }

        fn get(&self, indirizzo: &str) -> Result<Vec<u8>, ErroreRete> {
            Self::curl(&[indirizzo])
        }
    }

    /// A mano, coi server veri: `cargo test -p tascabile-prova-data -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn coi_server_veri() {
        let byte = format!("Doc Tascabile, prova coi server veri, {:?}", std::time::SystemTime::now()).into_bytes();
        let prova = fissa(&byte, &ReteCurl, *b"numero a caso 16").expect("fissata");
        let p = Prova::leggi(&prova).unwrap();
        eprintln!("fissata con {} attese", p.attese().len());
        let c = controlla(&byte, &prova, &ReteCurl).unwrap();
        eprintln!("subito dopo: {:?}", c.stato);
        assert_eq!(c.stato, Stato::InAttesa);
        let c = controlla(ESEMPIO_TXT, ESEMPIO_OTS, &ReteCurl).unwrap();
        eprintln!("l'esempio pubblico: {:?}", c.stato);
        assert_eq!(c.stato, Stato::Confermata { ora: 1_432_827_678, altezza: 358391 });
    }

    /// Revisione del 31/10, punto 7: un .ots con migliaia di attese fa al più 8 richieste, una per
    /// server.
    #[test]
    fn migliaia_di_attese_fanno_al_piu_otto_richieste() {
        let tante: Vec<(Vec<u8>, String)> =
            (0..5_000).map(|i: u32| (i.to_le_bytes().to_vec(), format!("https://a{}.calendar.opentimestamps.org", i % 20))).collect();
        let scelte = da_chiedere(tante);
        assert_eq!(scelte.len(), ATTESE_MASSIME);
        assert_eq!(scelte.iter().map(|(_, u)| u).collect::<std::collections::HashSet<_>>().len(), ATTESE_MASSIME);
        let stesso: Vec<(Vec<u8>, String)> =
            (0..5_000).map(|i: u32| (i.to_le_bytes().to_vec(), "https://alice.btc.calendar.opentimestamps.org".to_string())).collect();
        assert_eq!(da_chiedere(stesso).len(), 1);
    }

    /// Audit B4: i due esploratori vanno sentiti entrambi e devono concordare sul blocco; un'intestazione
    /// rovinata del primo fa ripiegare sul secondo se concordano sull'hash; e un blocco di un'epoca
    /// vecchia (67 bit di lavoro) non passa per un'altezza recente come 900.000.
    #[test]
    fn audit_b4_i_due_esploratori_devono_dare_lo_stesso_blocco() {
        // (A) Disaccordo fra i due esploratori sull'hash del blocco: si sente il secondo e non si conferma.
        let altro = format!("1{}", &HASH_358391[1..]);
        let rete_discordi = rete_dell_esempio(ESPLORATORI[0]).con(&format!("{}/block-height/358391", ESPLORATORI[1]), Ok(altro.into_bytes()));
        let esito = controlla(ESEMPIO_TXT, ESEMPIO_OTS, &rete_discordi);
        assert!(
            rete_discordi.chiamate().iter().any(|u| u.starts_with(ESPLORATORI[1])),
            "si deve sentire anche il secondo esploratore: {:?}",
            rete_discordi.chiamate()
        );
        assert!(esito.is_err(), "due esploratori in disaccordo non possono confermare: {esito:?}");

        // (B) Stesso hash, ma il primo manda un'intestazione rovinata: si ripiega sul secondo e si conferma.
        let mut rovinata = include_str!("../tests/fixtures/blocco-358391.hex").trim().to_string();
        rovinata.replace_range(158..160, "00");
        let rete_ripiego = rete_dell_esempio(ESPLORATORI[1])
            .con(&format!("{}/block-height/358391", ESPLORATORI[0]), Ok(HASH_358391.as_bytes().to_vec()))
            .con(&format!("{}/block/{HASH_358391}/header", ESPLORATORI[0]), Ok(rovinata.into_bytes()));
        let esito = controlla(ESEMPIO_TXT, ESEMPIO_OTS, &rete_ripiego);
        assert!(rete_ripiego.chiamate().iter().any(|u| u.starts_with(ESPLORATORI[1])), "{:?}", rete_ripiego.chiamate());
        assert_eq!(esito, Ok(Controllo { stato: Stato::Confermata { ora: 1_432_827_678, altezza: 358391 }, aggiornata: None }));

        // (C) L'intestazione del blocco 358391 (del 2015, 67 bit di lavoro) non può reggere all'altezza 900.000.
        fn cambia_altezza(a: &mut Albero, nuova: u64) {
            for att in &mut a.attestazioni {
                if let Attestazione::Blocco(h) = att {
                    *h = nuova;
                }
            }
            for (_, sotto) in &mut a.rami {
                cambia_altezza(sotto, nuova);
            }
        }
        let mut p = Prova::leggi(ESEMPIO_OTS).unwrap();
        cambia_altezza(&mut p.albero, 900_000);
        let ots_900k = p.scrivi().unwrap();
        let rete_900k = ESPLORATORI.iter().fold(ReteFinta::default(), |r, e| {
            r.con(&format!("{e}/block-height/900000"), Ok(HASH_358391.as_bytes().to_vec()))
                .con(&format!("{e}/block/{HASH_358391}/header"), Ok(include_bytes!("../tests/fixtures/blocco-358391.hex").to_vec()))
        });
        assert!(
            matches!(controlla(ESEMPIO_TXT, &ots_900k, &rete_900k), Err(ErroreData::Blocco(_))),
            "un'intestazione del 2015 con 67 bit di lavoro non deve valere per l'altezza 900.000"
        );
    }

    /// Audit B5: un singolo aggregatore o calendario ostile non può avvelenare il `.ots` con un
    /// blocco falso o futuro, con una catena oltre la profondità massima o riempiendo tutte le 8
    /// attese per escludere i calendari onesti.
    #[test]
    fn audit_b5_un_server_ostile_non_rovina_la_prova() {
        use crate::formato::Operazione;

        let rete_due_esploratori = || {
            ESPLORATORI.iter().fold(ReteFinta::default(), |r, e| {
                r.con(&format!("{e}/block-height/358391"), Ok(format!("{HASH_358391}\n").into_bytes()))
                    .con(&format!("{e}/block/{HASH_358391}/header"), Ok(include_bytes!("../tests/fixtures/blocco-358391.hex").to_vec()))
            })
        };

        // (1) Iniezione di un blocco durante `fissa`: va scartata, restano le attese dei server onesti.
        let (_, vere) = risposte_vere();
        let (_, mandato) = Prova::nuova(IN_ATTESA_TXT, NUMERO_IN_ATTESA);
        let blocco_finto =
            scrivi_albero(&Albero { messaggio: mandato.clone(), attestazioni: vec![Attestazione::Blocco(358391)], rami: Vec::new() }).unwrap();
        let rete = AGGREGATORI[..3]
            .iter()
            .zip(&vere[..3])
            .fold(ReteFinta::default(), |r, (a, b)| r.con(&format!("{a}/digest"), Ok(b.clone())))
            .con(&format!("{}/digest", AGGREGATORI[3]), Ok(blocco_finto));
        let ots = fissa(IN_ATTESA_TXT, &rete, NUMERO_IN_ATTESA).unwrap();
        let p = Prova::leggi(&ots).unwrap();
        assert!(p.blocchi().is_empty(), "fissa non deve accettare blocchi dagli aggregatori");
        assert!(!p.attese().is_empty(), "fissa deve tenere le attese dei server onesti");

        // Prova di partenza con due calendari (alice onesto, bob ostile) sullo stesso punto.
        let alice = "https://alice.btc.calendar.opentimestamps.org";
        let bob = "https://bob.btc.calendar.opentimestamps.org";
        let (in_attesa_alice, m, seguito) = esempio_in_attesa(alice);
        let mut p = Prova::leggi(&in_attesa_alice).unwrap();
        p.albero.trova_mut(&m).unwrap().aggiungi(Attestazione::InAttesa(bob.into()));
        let in_attesa = p.scrivi().unwrap();

        // (2) Blocco falso a un'altezza più bassa da bob: si ripiega sul blocco vero 358391 di alice.
        let blocco_basso = scrivi_albero(&Albero {
            messaggio: m.clone(),
            attestazioni: Vec::new(),
            rami: vec![(
                Operazione::Sha256,
                Albero { messaggio: Sha256::digest(&m).to_vec(), attestazioni: vec![Attestazione::Blocco(1)], rami: Vec::new() },
            )],
        })
        .unwrap();
        let rete = rete_due_esploratori()
            .con(&format!("{alice}/timestamp/{}", esadecimale(&m)), Ok(seguito.clone()))
            .con(&format!("{bob}/timestamp/{}", esadecimale(&m)), Ok(blocco_basso));
        let c = controlla(ESEMPIO_TXT, &in_attesa, &rete).unwrap();
        assert_eq!(c.stato, Stato::Confermata { ora: 1_432_827_678, altezza: 358391 });

        // (3) Blocco futuro non verificato da bob: non deve svuotare le attese per il controllo dopo.
        let blocco_futuro = scrivi_albero(&Albero {
            messaggio: m.clone(),
            attestazioni: Vec::new(),
            rami: vec![(
                Operazione::Sha256,
                Albero { messaggio: Sha256::digest(&m).to_vec(), attestazioni: vec![Attestazione::Blocco(999_999_999)], rami: Vec::new() },
            )],
        })
        .unwrap();
        let rete1 = ReteFinta::default()
            .con(&format!("{alice}/timestamp/{}", esadecimale(&m)), Err(ErroreRete::NonTrovato))
            .con(&format!("{bob}/timestamp/{}", esadecimale(&m)), Ok(blocco_futuro));
        let c1 = controlla(ESEMPIO_TXT, &in_attesa, &rete1).unwrap();
        let dopo = c1.aggiornata.as_deref().unwrap_or(&in_attesa);
        let rete2 = rete_due_esploratori().con(&format!("{alice}/timestamp/{}", esadecimale(&m)), Ok(seguito));
        let c2 = controlla(ESEMPIO_TXT, dopo, &rete2).unwrap();
        assert_eq!(c2.stato, Stato::Confermata { ora: 1_432_827_678, altezza: 358391 });

        // (4) Catena di 255 Sha256 da bob: la prova aggiornata, se c'è, deve restare leggibile.
        let mut passi = vec![m.clone()];
        for _ in 0..255 {
            passi.push(Sha256::digest(passi.last().unwrap()).to_vec());
        }
        let mut profonda = Albero { messaggio: passi.pop().unwrap(), attestazioni: vec![Attestazione::InAttesa(bob.into())], rami: Vec::new() };
        while let Some(msg) = passi.pop() {
            profonda = Albero { messaggio: msg, attestazioni: Vec::new(), rami: vec![(Operazione::Sha256, profonda)] };
        }
        let rete_profonda = ReteFinta::default()
            .con(&format!("{alice}/timestamp/{}", esadecimale(&m)), Err(ErroreRete::NonTrovato))
            .con(&format!("{bob}/timestamp/{}", esadecimale(&m)), Ok(scrivi_albero(&profonda).unwrap()));
        let c = controlla(ESEMPIO_TXT, &in_attesa, &rete_profonda).unwrap();
        if let Some(a) = &c.aggiornata {
            assert!(Prova::leggi(a).is_ok(), "la prova aggiornata non deve superare il limite di profondità");
        }

        // (5) Un aggregatore con 8 attese finte non deve escludere i calendari degli altri aggregatori.
        let mut tante = Albero::nuovo(mandato.clone());
        for i in 1..=8 {
            tante.aggiungi(Attestazione::InAttesa(format!("https://finto{i}.calendar.opentimestamps.org")));
        }
        let rete_fissa = AGGREGATORI[1..]
            .iter()
            .zip(&vere[1..])
            .fold(ReteFinta::default().con(&format!("{}/digest", AGGREGATORI[0]), Ok(scrivi_albero(&tante).unwrap())), |r, (a, b)| {
                r.con(&format!("{a}/digest"), Ok(b.clone()))
            });
        let ots = fissa(IN_ATTESA_TXT, &rete_fissa, NUMERO_IN_ATTESA).unwrap();
        let rete_controllo = ReteFinta::default();
        let _ = controlla(IN_ATTESA_TXT, &ots, &rete_controllo).unwrap();
        let chiamate = rete_controllo.chiamate();
        assert!(
            chiamate.iter().any(|u| !u.starts_with("https://finto")),
            "un solo aggregatore con 8 attese non deve escludere i calendari onesti: {chiamate:?}"
        );
    }
}
