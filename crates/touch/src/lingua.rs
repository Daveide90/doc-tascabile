//! Le lingue dell'app (0.10.0): italiano, inglese e spagnolo.
//!
//! Il testo italiano scritto nel codice è anche la chiave della traduzione: i componenti di
//! `tascabile-touch` traducono da soli quello che ricevono ([`t`]), e chi disegna un testo
//! direttamente con egui lo passa da [`t`] o, se ha dei numeri o dei nomi dentro, da [`tf!`]
//! (un modello con `{}` al posto dei valori). Le traduzioni stanno nelle tabelle di
//! `lingua/testi_*.rs`, una riga per frase: (italiano, inglese, spagnolo).
//!
//! In italiano il testo resta quello del codice: le prove dell'app cercano i pulsanti per le
//! etichette italiane e non cambiano. Con `TASCABILE_MANCANTI=<file>` ogni testo italiano che passa
//! da qui senza una traduzione nelle tabelle si scrive in quel file, una riga per testo: le prove
//! girano in italiano e intanto dicono che cosa manca alle altre lingue.
//!
//! La lingua scelta vale per il thread che disegna, come il tema: le prove girano in parallelo,
//! ognuna sul suo thread, e non si toccano.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::sync::{LazyLock, Mutex};

mod testi;

/// Le lingue dell'app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Lingua {
    #[default]
    Italiano,
    Inglese,
    Spagnolo,
}

impl Lingua {
    pub const TUTTE: [Lingua; 3] = [Lingua::Italiano, Lingua::Inglese, Lingua::Spagnolo];

    /// La lingua dell'app per la lingua del telefono (codice ISO 639, «it», «en-GB», «es_419»):
    /// italiano e spagnolo se il telefono è in italiano o in spagnolo, altrimenti inglese.
    pub fn del_telefono(codice: &str) -> Lingua {
        let base = codice.trim().split(['-', '_']).next().unwrap_or("").to_ascii_lowercase();
        match base.as_str() {
            "it" => Lingua::Italiano,
            "es" => Lingua::Spagnolo,
            _ => Lingua::Inglese,
        }
    }

    /// Il codice ISO 639.
    pub fn codice(self) -> &'static str {
        match self {
            Lingua::Italiano => "it",
            Lingua::Inglese => "en",
            Lingua::Spagnolo => "es",
        }
    }

    /// Dal codice ISO 639 scritto da [`Lingua::codice`].
    pub fn da_codice(codice: &str) -> Option<Lingua> {
        Lingua::TUTTE.into_iter().find(|l| l.codice() == codice.trim())
    }

    /// Il nome della lingua nella lingua stessa, come lo cerca chi la parla: non si traduce.
    pub fn nome(self) -> &'static str {
        match self {
            Lingua::Italiano => "Italiano",
            Lingua::Inglese => "English",
            Lingua::Spagnolo => "Español",
        }
    }
}

thread_local! {
    static ATTIVA: Cell<Lingua> = const { Cell::new(Lingua::Italiano) };
}

/// La lingua dell'app.
pub fn attiva() -> Lingua {
    ATTIVA.with(|l| l.get())
}

/// Cambia la lingua dell'app (per il thread che disegna).
pub fn imposta(lingua: Lingua) {
    ATTIVA.with(|l| l.set(lingua));
}

/// Le traduzioni: dal testo italiano, (inglese, spagnolo).
static TABELLA: LazyLock<HashMap<&'static str, (&'static str, &'static str)>> =
    LazyLock::new(|| testi::tutte().map(|(it, en, es)| (*it, (*en, *es))).collect());

/// Il testo `s` nella lingua dell'app: la sua traduzione, se c'è; altrimenti com'è.
pub fn t(s: &str) -> &str {
    match attiva() {
        Lingua::Italiano => {
            controlla(s);
            s
        }
        l => match TABELLA.get(s) {
            Some((en, es)) => {
                if l == Lingua::Inglese {
                    en
                } else {
                    es
                }
            }
            None => {
                controlla(s);
                s
            }
        },
    }
}

/// Come [`t`], per un testo che si possiede.
pub fn t_string(s: String) -> String {
    match attiva() {
        Lingua::Italiano => {
            controlla(&s);
            s
        }
        _ => t(&s).to_string(),
    }
}

/// Il testo italiano `s` in una lingua data, senza cambiare quella dell'app.
pub fn in_lingua(s: &str, lingua: Lingua) -> &str {
    match (lingua, TABELLA.get(s)) {
        (Lingua::Inglese, Some((en, _))) => en,
        (Lingua::Spagnolo, Some((_, es))) => es,
        _ => s,
    }
}

/// Il testo italiano `s` ha una traduzione nelle tabelle.
pub fn tradotto(s: &str) -> bool {
    TABELLA.contains_key(s)
}

/// Il modello `modello` tradotto, coi `valori` al posto dei segnaposto: `{}` prende il valore
/// dopo, `{0}`, `{1}`… quello di quel posto (una traduzione può cambiare l'ordine), `{{` e `}}`
/// sono le graffe. Si usa con la macro [`tf!`].
pub fn tf(modello: &str, valori: &[&dyn Display]) -> String {
    riempi(t(modello), valori)
}

/// Il modello coi valori al posto dei segnaposto (vedi [`tf`]).
pub fn riempi(modello: &str, valori: &[&dyn Display]) -> String {
    let mut fuori = String::with_capacity(modello.len() + 16);
    let mut dopo = 0;
    let mut c = modello.char_indices().peekable();
    while let Some((i, ch)) = c.next() {
        match ch {
            '{' if c.peek().map(|(_, d)| *d) == Some('{') => {
                c.next();
                fuori.push('{');
            }
            '}' if c.peek().map(|(_, d)| *d) == Some('}') => {
                c.next();
                fuori.push('}');
            }
            '{' => {
                let resto = &modello[i + 1..];
                match resto.find('}') {
                    Some(fine) => {
                        let dentro = &resto[..fine];
                        let posto = if dentro.is_empty() {
                            dopo += 1;
                            Some(dopo - 1)
                        } else {
                            dentro.parse::<usize>().ok()
                        };
                        match posto.and_then(|p| valori.get(p)) {
                            Some(v) => fuori.push_str(&v.to_string()),
                            None => {
                                fuori.push('{');
                                fuori.push_str(dentro);
                                fuori.push('}');
                            }
                        }
                        for _ in 0..=dentro.chars().count() {
                            c.next();
                        }
                    }
                    None => fuori.push('{'),
                }
            }
            _ => fuori.push(ch),
        }
    }
    fuori
}

/// Un testo con dei valori dentro, tradotto: `tf!("Pagina {} di {}", corrente, totale)`. Il
/// modello è il testo italiano, chiave della traduzione.
#[macro_export]
macro_rules! tf {
    ($modello:expr $(, $valore:expr)* $(,)?) => {
        $crate::lingua::tf($modello, &[$(&$valore as &dyn ::std::fmt::Display),*])
    };
}

/// Dove scrivere i testi senza traduzione (`TASCABILE_MANCANTI`), e quelli già scritti.
static MANCANTI: LazyLock<Option<(std::path::PathBuf, Mutex<HashSet<String>>)>> =
    LazyLock::new(|| std::env::var_os("TASCABILE_MANCANTI").map(|f| (f.into(), Mutex::new(HashSet::new()))));

/// Il testo è da tradurre: ha delle lettere (non è un'icona, un numero, un segno).
fn da_tradurre(s: &str) -> bool {
    s.chars().any(char::is_alphabetic) && !s.chars().all(|c| ('\u{e000}'..='\u{f8ff}').contains(&c) || c.is_whitespace())
}

/// Con `TASCABILE_MANCANTI`: scrive il testo se non ha traduzione.
fn controlla(s: &str) {
    let Some((file, visti)) = MANCANTI.as_ref() else { return };
    if !da_tradurre(s) || TABELLA.contains_key(s) {
        return;
    }
    let mut visti = visti.lock().unwrap_or_else(|e| e.into_inner());
    if visti.insert(s.to_string()) {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(file) {
            let _ = writeln!(f, "{}", s.replace('\n', "\\n"));
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn la_lingua_del_telefono_sceglie_fra_le_tre_e_altrimenti_l_inglese() {
        assert_eq!(Lingua::del_telefono("it"), Lingua::Italiano);
        assert_eq!(Lingua::del_telefono("it-CH"), Lingua::Italiano);
        assert_eq!(Lingua::del_telefono("es_419"), Lingua::Spagnolo);
        assert_eq!(Lingua::del_telefono("ES"), Lingua::Spagnolo);
        assert_eq!(Lingua::del_telefono("en-GB"), Lingua::Inglese);
        assert_eq!(Lingua::del_telefono("de"), Lingua::Inglese);
        assert_eq!(Lingua::del_telefono("fr"), Lingua::Inglese);
        assert_eq!(Lingua::del_telefono(""), Lingua::Inglese);
        for l in Lingua::TUTTE {
            assert_eq!(Lingua::da_codice(l.codice()), Some(l));
        }
        assert_eq!(Lingua::da_codice("de"), None);
    }

    #[test]
    fn i_segnaposto_si_riempiono_anche_in_un_altro_ordine() {
        assert_eq!(riempi("Pagina {} di {}", &[&3, &12]), "Pagina 3 di 12");
        assert_eq!(riempi("{1} pages, page {0}", &[&3, &12]), "12 pages, page 3");
        assert_eq!(riempi("{{graffe}} e {}", &[&"x"]), "{graffe} e x");
        assert_eq!(riempi("manca {} e {}", &[&1]), "manca 1 e {}");
        assert_eq!(riempi("àèì {} ù", &[&"ò"]), "àèì ò ù");
    }

    #[test]
    fn si_traduce_nella_lingua_attiva_e_l_italiano_resta_com_e() {
        imposta(Lingua::Italiano);
        assert_eq!(t("Annulla"), "Annulla");
        imposta(Lingua::Inglese);
        assert_eq!(t("Annulla"), "Cancel");
        assert_eq!(t("Un testo che non c'è"), "Un testo che non c'è");
        imposta(Lingua::Spagnolo);
        assert_eq!(t("Annulla"), "Cancelar");
        imposta(Lingua::Italiano);
        assert_eq!(in_lingua("Annulla", Lingua::Inglese), "Cancel");
    }

    /// Le tabelle: ogni testo italiano una volta sola, con le due traduzioni piene e gli stessi
    /// segnaposto (un `{}` perso o in più cambierebbe i numeri sullo schermo).
    #[test]
    fn le_tabelle_sono_complete_e_coi_segnaposto_giusti() {
        fn segnaposto(s: &str) -> usize {
            let pulito = s.replace("{{", "").replace("}}", "");
            pulito.matches('{').count()
        }
        let mut visti = HashSet::new();
        for (it, en, es) in testi::tutte() {
            assert!(visti.insert(*it), "due volte: «{it}»");
            assert!(!en.trim().is_empty() && !es.trim().is_empty(), "traduzione vuota: «{it}»");
            assert_eq!(segnaposto(it), segnaposto(en), "segnaposto in inglese: «{it}» → «{en}»");
            assert_eq!(segnaposto(it), segnaposto(es), "segnaposto in spagnolo: «{it}» → «{es}»");
            assert_eq!(it.trim(), *it, "spazi ai lati: «{it}»");
        }
    }
}
