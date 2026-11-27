//! L'intestazione di un blocco (80 byte): che la sua radice sia il messaggio della prova, che il
//! lavoro del blocco torni coi suoi bit e che non sia poco, e la sua ora. Così l'intestazione si
//! può prendere da un esploratore pubblico senza fidarsi di lui: un'intestazione finta col lavoro
//! che torna costerebbe miliardi di miliardi di tentativi.

use sha2::{Digest, Sha256};

/// Il lavoro minimo richiesto a seconda dell'altezza del blocco, in bit a zero in testa
/// all'obiettivo (256 meno la lunghezza in bit): valori qualche bit sotto la difficoltà reale di
/// quelle epoche, così un'intestazione su misura costa quanto un blocco vero (il blocco 358391 del
/// 2015 ha 67 bit).
const SOGLIE_LAVORO: [(u64, u32); 4] = [(850_000, 76), (700_000, 74), (500_000, 70), (0, 64)];

/// Perché un'intestazione non va.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroreBlocco {
    /// Non è lunga 80 byte.
    Lunghezza,
    /// La radice del blocco non è il messaggio della prova.
    RadiceDiversa,
    /// L'impronta del blocco non sta sotto l'obiettivo dei suoi bit, o i bit non hanno senso.
    LavoroNonTorna,
    /// L'obiettivo è troppo facile per un blocco vero.
    PocoLavoro,
    /// Gli esploratori danno blocchi diversi per la stessa altezza.
    EsploratoriDiscordi,
}

impl std::fmt::Display for ErroreBlocco {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ErroreBlocco::Lunghezza => "intestazione del blocco della lunghezza sbagliata",
            ErroreBlocco::RadiceDiversa => "il blocco non contiene la prova",
            ErroreBlocco::LavoroNonTorna => "il lavoro del blocco non torna",
            ErroreBlocco::PocoLavoro => "il blocco ha troppo poco lavoro",
            ErroreBlocco::EsploratoriDiscordi => "gli esploratori danno blocchi diversi",
        })
    }
}

impl std::error::Error for ErroreBlocco {}

/// Un'intestazione controllata: l'ora del blocco (secondi dal 1970, UTC) e la sua impronta come la
/// scrivono gli esploratori (esadecimale, rovesciata).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BloccoControllato {
    pub ora: u32,
    pub hash: String,
}

/// L'obiettivo dei bit compatti di un blocco, in 32 byte dal più significativo; None se i bit non
/// hanno senso (segno, zero, troppo grande).
fn obiettivo(bit: u32) -> Option<[u8; 32]> {
    let esponente = (bit >> 24) as i64;
    let mantissa = bit & 0x007f_ffff;
    if bit & 0x0080_0000 != 0 || mantissa == 0 {
        return None;
    }
    let mut t = [0u8; 32];
    for (i, b) in [(mantissa >> 16) as u8, (mantissa >> 8) as u8, mantissa as u8].into_iter().enumerate() {
        // Il byte i della mantissa vale 256^(esponente - 1 - i).
        let potenza = esponente - 1 - i as i64;
        if potenza < 0 {
            continue;
        }
        if potenza >= 32 {
            if b != 0 {
                return None;
            }
            continue;
        }
        t[31 - potenza as usize] = b;
    }
    Some(t)
}

/// Bit a zero in testa all'obiettivo `t` (256 meno la lunghezza in bit di `t`).
fn bit_a_zero(t: &[u8; 32]) -> u32 {
    let mut n = 0u32;
    for &b in t {
        if b == 0 {
            n += 8;
        } else {
            n += b.leading_zeros();
            break;
        }
    }
    n
}

fn bit_minimi(altezza: u64) -> u32 {
    SOGLIE_LAVORO.iter().find_map(|&(da, bit)| (altezza >= da).then_some(bit)).unwrap_or(64)
}

/// Controlla l'intestazione `intestazione` per la radice `radice` (il messaggio della prova,
/// nell'ordine dell'intestazione), con la soglia minima base di 64 bit.
pub fn controlla_intestazione(intestazione: &[u8], radice: &[u8]) -> Result<BloccoControllato, ErroreBlocco> {
    controlla_intestazione_all_altezza(intestazione, radice, 0)
}

/// Controlla l'intestazione `intestazione` per la radice `radice` all'altezza `altezza`, chiedendo
/// un lavoro minimo adeguato all'epoca del blocco.
pub fn controlla_intestazione_all_altezza(intestazione: &[u8], radice: &[u8], altezza: u64) -> Result<BloccoControllato, ErroreBlocco> {
    let h: &[u8; 80] = intestazione.try_into().map_err(|_| ErroreBlocco::Lunghezza)?;
    if &h[36..68] != radice {
        return Err(ErroreBlocco::RadiceDiversa);
    }
    let bit = u32::from_le_bytes([h[72], h[73], h[74], h[75]]);
    let t = obiettivo(bit).ok_or(ErroreBlocco::LavoroNonTorna)?;
    let mut impronta: [u8; 32] = Sha256::digest(Sha256::digest(h)).into();
    // L'impronta è un numero scritto dal byte meno significativo: rovesciata si confronta.
    impronta.reverse();
    if impronta > t {
        return Err(ErroreBlocco::LavoroNonTorna);
    }
    if bit_a_zero(&t) < bit_minimi(altezza) {
        return Err(ErroreBlocco::PocoLavoro);
    }
    let ora = u32::from_le_bytes([h[68], h[69], h[70], h[71]]);
    Ok(BloccoControllato { ora, hash: crate::formato::esadecimale(&impronta) })
}

#[cfg(test)]
mod test {
    use super::*;

    fn intestazione_358391() -> Vec<u8> {
        let hex = include_str!("../tests/fixtures/blocco-358391.hex").trim();
        (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect()
    }

    fn radice_dell_esempio() -> Vec<u8> {
        let p = crate::Prova::leggi(include_bytes!("../tests/fixtures/hello-world.txt.ots")).unwrap();
        p.albero.tutte().into_iter().find(|(_, a)| matches!(a, crate::Attestazione::Blocco(_))).unwrap().0.to_vec()
    }

    #[test]
    fn il_blocco_dell_esempio_regge_e_dice_il_28_maggio_2015() {
        let b = controlla_intestazione(&intestazione_358391(), &radice_dell_esempio()).expect("regge");
        // 1432827678: giovedì 28 maggio 2015, 15:41:18 UTC, come nella guida del client ufficiale.
        assert_eq!(b.ora, 1_432_827_678);
        assert_eq!(b.hash, "000000000000000003e892881a8cdcdc117c06d444057c98b6f04a9ee75a2319");
    }

    #[test]
    fn un_intestazione_sbagliata_non_regge() {
        let h = intestazione_358391();
        let radice = radice_dell_esempio();
        assert_eq!(controlla_intestazione(&h[..79], &radice), Err(ErroreBlocco::Lunghezza));
        assert_eq!(controlla_intestazione(&h, &[0u8; 32]), Err(ErroreBlocco::RadiceDiversa));
        // Un byte del nonce cambiato: l'impronta del blocco non sta più sotto l'obiettivo.
        let mut cambiata = h.clone();
        cambiata[79] ^= 1;
        assert_eq!(controlla_intestazione(&cambiata, &radice), Err(ErroreBlocco::LavoroNonTorna));
        // Un'intestazione inventata con l'obiettivo facilissimo: il lavoro torna, ma è troppo poco.
        let mut finta = h.clone();
        finta[72..76].copy_from_slice(&0x207f_ffffu32.to_le_bytes());
        let mut nonce = 0u32;
        loop {
            finta[76..80].copy_from_slice(&nonce.to_le_bytes());
            if controlla_intestazione(&finta, &radice) != Err(ErroreBlocco::LavoroNonTorna) {
                break;
            }
            nonce += 1;
        }
        assert_eq!(controlla_intestazione(&finta, &radice), Err(ErroreBlocco::PocoLavoro));
    }

    #[test]
    fn l_obiettivo_dei_bit_compatti() {
        // Il blocco 0: 0x1d00ffff, cioè 0x00000000ffff seguito da zeri.
        let t = obiettivo(0x1d00_ffff).unwrap();
        assert_eq!(&t[..6], &[0, 0, 0, 0, 0xff, 0xff]);
        assert!(t[6..].iter().all(|b| *b == 0));
        // Col segno, a zero o fuori misura non ha senso.
        assert_eq!(obiettivo(0x1d80_ffff), None);
        assert_eq!(obiettivo(0x1d00_0000), None);
        assert_eq!(obiettivo(0x2300_ffff), None);
        // Esponente piccolo: la mantissa scivola a destra.
        let t = obiettivo(0x0200_ffff).unwrap();
        assert_eq!(t[31], 0xff);
        assert_eq!(t[30], 0x00);
    }
}
