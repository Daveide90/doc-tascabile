//! Calcoli del ponte tastiera: niente Android qui, si prova con `cargo test` su Linux.
//!
//! La tastiera di Android (GameTextInput) misura selezione e composizione in unità UTF-16
//! (indici delle String Java); il motore di WordCraft lavora in byte UTF-8.

/// Indice UTF-16 → indice in byte UTF-8 di `s` (arrotondato al carattere successivo).
pub fn utf16_to_byte(s: &str, utf16: usize) -> usize {
    let mut unita = 0usize;
    for (byte, c) in s.char_indices() {
        if unita >= utf16 {
            return byte;
        }
        unita += c.len_utf16();
    }
    s.len()
}

/// Indice in byte UTF-8 → indice UTF-16. Un indice a metà di un carattere vale l'inizio del
/// carattere, uno oltre la fine vale la fine: niente panico per una posizione vecchia.
pub fn byte_to_utf16(s: &str, byte: usize) -> usize {
    s[..s.floor_char_boundary(byte)].encode_utf16().count()
}

/// Una modifica: sostituire `da..a` (byte del testo vecchio) con `inserisci`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Modifica {
    pub da: usize,
    pub a: usize,
    pub inserisci: String,
}

/// Differenza tra il testo che la tastiera aveva (`vecchio`) e quello che manda ora (`nuovo`):
/// la sostituzione più piccola che porta dal primo al secondo.
///
/// `cursore` (byte nel testo nuovo) decide solo i casi ambigui: in «aa» → «aaa» col cursore a 2
/// l'inserimento è in posizione 1, perché la parte cambiata non finisce prima del cursore.
/// Una correzione lontana dal cursore (per esempio nel paragrafo prima) resta dov'è.
pub fn differenza(vecchio: &str, nuovo: &str, cursore: usize) -> Option<Modifica> {
    if vecchio == nuovo {
        return None;
    }
    let cursore = cursore.min(nuovo.len());
    // 1. coda comune, senza superare il cursore
    let mut coda = 0usize;
    for (a, b) in vecchio.chars().rev().zip(nuovo.chars().rev()) {
        if a != b || nuovo.len() - coda - b.len_utf8() < cursore {
            break;
        }
        coda += a.len_utf8();
    }
    // 2. testa comune, senza sovrapporsi alla coda
    let fine_vecchio = vecchio.len() - coda;
    let fine_nuovo = nuovo.len() - coda;
    let mut testa = 0usize;
    for (a, b) in vecchio[..fine_vecchio].chars().zip(nuovo[..fine_nuovo].chars()) {
        if a != b {
            break;
        }
        testa += a.len_utf8();
    }
    // 3. coda comune rimasta dentro la parte cambiata: una correzione lontana dal cursore
    //    non si allarga fino a lui
    let resto: usize = vecchio[testa..fine_vecchio]
        .chars()
        .rev()
        .zip(nuovo[testa..fine_nuovo].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum();
    Some(Modifica { da: testa, a: fine_vecchio - resto, inserisci: nuovo[testa..fine_nuovo - resto].to_string() })
}

#[cfg(test)]
mod test {
    use super::*;

    fn applica(vecchio: &str, m: &Modifica) -> String {
        let mut s = vecchio.to_string();
        s.replace_range(m.da..m.a, &m.inserisci);
        s
    }

    fn terna(m: &Modifica) -> (usize, usize, &str) {
        (m.da, m.a, m.inserisci.as_str())
    }

    #[test]
    fn utf16_e_byte() {
        let s = "è😀a"; // è: 2 byte e 1 unità; 😀: 4 byte e 2 unità
        assert_eq!(utf16_to_byte(s, 0), 0);
        assert_eq!(utf16_to_byte(s, 1), 2);
        assert_eq!(utf16_to_byte(s, 3), 6);
        assert_eq!(utf16_to_byte(s, 4), 7);
        assert_eq!(byte_to_utf16(s, 6), 3);
    }

    #[test]
    fn oltre_la_fine() {
        assert_eq!(utf16_to_byte("abc", 99), 3);
        assert_eq!(byte_to_utf16("abc", 99), 3);
    }

    #[test]
    fn testi_uguali() {
        assert_eq!(differenza("ciao", "ciao", 2), None);
    }

    #[test]
    fn lettera_in_mezzo_a_lettere_uguali() {
        assert_eq!(terna(&differenza("aa", "aaa", 2).unwrap()), (1, 1, "a"));
    }

    #[test]
    fn lettera_in_mezzo() {
        assert_eq!(terna(&differenza("Cao", "Ciao", 2).unwrap()), (1, 1, "i"));
    }

    #[test]
    fn cancellazione() {
        assert_eq!(terna(&differenza("abc", "ab", 2).unwrap()), (2, 3, ""));
    }

    #[test]
    fn autocorrezione() {
        let m = differenza("teh ", "the ", 4).unwrap();
        assert_eq!(applica("teh ", &m), "the ");
        assert_eq!(terna(&m), (1, 3, "he"));
        // correzione vera, sullo spazio: «teh» diventa «the »
        assert_eq!(terna(&differenza("teh", "the ", 4).unwrap()), (1, 3, "he "));
    }

    #[test]
    fn a_capo() {
        assert_eq!(terna(&differenza("Ciao", "Ciao\n", 5).unwrap()), (4, 4, "\n"));
    }

    #[test]
    fn emoji_inserita() {
        assert_eq!(terna(&differenza("ab", "a😀b", 5).unwrap()), (1, 1, "😀"));
    }

    #[test]
    fn emoji_cancellata() {
        assert_eq!(terna(&differenza("a😀b", "ab", 1).unwrap()), (1, 5, ""));
    }

    #[test]
    fn dettatura() {
        assert_eq!(terna(&differenza("Oggi ", "Oggi piove forte", 16).unwrap()), (5, 5, "piove forte"));
    }

    #[test]
    fn tutto_cancellato() {
        assert_eq!(terna(&differenza("abc", "", 0).unwrap()), (0, 3, ""));
    }

    #[test]
    fn correzione_lontana_dal_cursore() {
        // Una tastiera che corregge il paragrafo prima e rimette il cursore in fondo.
        assert_eq!(terna(&differenza("Uno\nDue", "Una\nDue", 7).unwrap()), (2, 3, "a"));
    }

    #[test]
    fn cursore_prima_della_modifica() {
        assert_eq!(terna(&differenza("abc", "ac", 1).unwrap()), (1, 2, ""));
    }

    #[test]
    fn cancellazione_dentro_lettere_uguali() {
        assert_eq!(terna(&differenza("aaa", "aa", 1).unwrap()), (1, 2, ""));
    }

    #[test]
    fn lettere_con_lo_stesso_primo_byte() {
        assert_eq!(terna(&differenza("è", "é", 2).unwrap()), (0, 2, "é"));
    }

    #[test]
    fn utf16_in_mezzo_a_una_coppia_surrogata() {
        assert_eq!(utf16_to_byte("😀a", 1), 4);
    }

    #[test]
    fn byte_fuori_confine_senza_panico() {
        assert_eq!(byte_to_utf16("è", 1), 0);
        assert_eq!(byte_to_utf16("😀a", 2), 0);
    }

    #[test]
    fn tutte_le_coppie_brevi() {
        // Ogni coppia di testi fino a 3 simboli (multibyte, coppie surrogate, a capo), ogni cursore:
        // niente panico, intervalli su confini di carattere, e la modifica porta sempre al testo nuovo.
        let alfabeto = ["a", "è", "😀", "\n"];
        let mut testi = vec![String::new()];
        let mut livello = vec![String::new()];
        for _ in 0..3 {
            livello = livello.iter().flat_map(|s| alfabeto.iter().map(move |c| format!("{s}{c}"))).collect();
            testi.extend(livello.iter().cloned());
        }
        for v in &testi {
            for n in &testi {
                for c in 0..=n.len() + 1 {
                    match differenza(v, n, c) {
                        None => assert_eq!(v, n),
                        Some(m) => {
                            assert!(m.da <= m.a && m.a <= v.len(), "{v:?} → {n:?} @{c}");
                            assert!(v.is_char_boundary(m.da) && v.is_char_boundary(m.a), "{v:?} → {n:?} @{c}");
                            assert_eq!(&applica(v, &m), n, "{v:?} → {n:?} @{c}");
                        }
                    }
                }
            }
        }
    }
}
