//! Date e numeri all'italiana. Il formattatore di GridCraft conosce solo l'inglese americano
//! (ricerca, sezione 4): il formato di data «di sistema» (id 14) esce `m/d/yyyy`, quindi il 3
//! novembre si leggerebbe «11/3/2026», e `1234,5` con `#,##0.0` esce `1,234.5`. Qui il codice del
//! formato di data di sistema diventa `dd/mm/yyyy` prima di formattare, e il testo uscito prende
//! i separatori, i mesi, i giorni, VERO e FALSO e gli errori italiani.

use gridcraft_core::{CellError, DateSystem, Value};
use gridcraft_numfmt::{NumberFormat, format_general_fit, format_value};

/// I codici di data che Excel scrive per la data breve del sistema (e che in italiano mostra
/// giorno, mese, anno), con quello che si mostra.
const DATE_DI_SISTEMA: [(&str, &str); 6] = [
    ("m/d/yyyy", "dd/mm/yyyy"),
    ("m/d/yy", "dd/mm/yy"),
    ("mm-dd-yy", "dd/mm/yyyy"),
    ("m/d/yyyy h:mm", "dd/mm/yyyy hh:mm"),
    ("m/d/yy h:mm", "dd/mm/yy hh:mm"),
    ("mm/dd/yyyy", "dd/mm/yyyy"),
];

const MESI: [(&str, &str); 24] = [
    ("January", "gennaio"),
    ("February", "febbraio"),
    ("March", "marzo"),
    ("April", "aprile"),
    ("May", "maggio"),
    ("June", "giugno"),
    ("July", "luglio"),
    ("August", "agosto"),
    ("September", "settembre"),
    ("October", "ottobre"),
    ("November", "novembre"),
    ("December", "dicembre"),
    ("Jan", "gen"),
    ("Feb", "feb"),
    ("Mar", "mar"),
    ("Apr", "apr"),
    ("May", "mag"),
    ("Jun", "giu"),
    ("Jul", "lug"),
    ("Aug", "ago"),
    ("Sep", "set"),
    ("Oct", "ott"),
    ("Nov", "nov"),
    ("Dec", "dic"),
];

const GIORNI: [(&str, &str); 14] = [
    ("Monday", "lunedì"),
    ("Tuesday", "martedì"),
    ("Wednesday", "mercoledì"),
    ("Thursday", "giovedì"),
    ("Friday", "venerdì"),
    ("Saturday", "sabato"),
    ("Sunday", "domenica"),
    ("Mon", "lun"),
    ("Tue", "mar"),
    ("Wed", "mer"),
    ("Thu", "gio"),
    ("Fri", "ven"),
    ("Sat", "sab"),
    ("Sun", "dom"),
];

/// Il testo di una cella col formato `codice`, all'italiana: il testo, il colore del formato
/// (`[Red]`) e se è un numero (allineato a destra quando l'allineamento è «Generale»). `cifre`: le
/// cifre che stanno nella colonna, per i numeri senza formato (oltre si accorciano, o `#`).
pub fn testo(v: &Value, codice: &str, sistema: DateSystem, cifre: usize) -> (String, Option<[u8; 3]>, bool) {
    match v {
        Value::Empty => (String::new(), None, false),
        Value::Bool(b) => ((if *b { "VERO" } else { "FALSO" }).into(), None, false),
        Value::Error(e) => (errore(*e).into(), None, false),
        Value::Number(n) if codice == "General" || codice.is_empty() => {
            let massimo = cifre.clamp(1, 11);
            match format_general_fit(*n, massimo) {
                Some(t) => (separatori(&t), None, true),
                None => ("#".repeat(massimo), None, true),
            }
        }
        Value::Number(_) => {
            let codice = DATE_DI_SISTEMA.iter().find(|(en, _)| codice.eq_ignore_ascii_case(en)).map_or(codice, |(_, it)| *it);
            let formato = NumberFormat::parse_cached(codice);
            let f = format_value(v, &formato, sistema);
            let t = if formato.is_date() { nomi(&f.text) } else { separatori(&f.text) };
            (t, f.color.and_then(|c| c.rgb()), f.numeric)
        }
        _ => {
            let f = format_value(v, &NumberFormat::parse_cached(codice), sistema);
            (f.text, f.color.and_then(|c| c.rgb()), false)
        }
    }
}

/// Il nome italiano di un errore di Excel.
pub fn errore(e: CellError) -> &'static str {
    match e.as_str() {
        "#VALUE!" => "#VALORE!",
        "#NAME?" => "#NOME?",
        "#REF!" => "#RIF!",
        "#N/A" => "#N/D",
        "#NULL!" => "#NULLO!",
        "#SPILL!" => "#ESPANSIONE!",
        "#CALC!" => "#CALC!",
        altro => altro,
    }
}

/// Virgola e punto scambiati fra due cifre (o fra una cifra e la fine di una parte decimale):
/// `1,234.5` diventa `1.234,5`.
pub fn separatori(t: &str) -> String {
    let c: Vec<char> = t.chars().collect();
    (0..c.len())
        .map(|i| {
            let fra_cifre = i > 0 && i + 1 < c.len() && c[i - 1].is_ascii_digit() && c[i + 1].is_ascii_digit();
            match c[i] {
                ',' if fra_cifre => '.',
                '.' if fra_cifre => ',',
                x => x,
            }
        })
        .collect()
}

/// I nomi inglesi dei mesi e dei giorni in una data, in italiano (le parole intere, prima le
/// lunghe).
fn nomi(t: &str) -> String {
    let mut fuori = String::with_capacity(t.len());
    let mut parola = String::new();
    let traduci = |p: &str| -> String {
        MESI.iter().chain(&GIORNI).find(|(en, _)| *en == p).map_or_else(|| p.to_string(), |(_, it)| it.to_string())
    };
    for ch in t.chars() {
        if ch.is_ascii_alphabetic() {
            parola.push(ch);
        } else {
            fuori.push_str(&traduci(&parola));
            parola.clear();
            fuori.push(ch);
        }
    }
    fuori.push_str(&traduci(&parola));
    fuori
}

#[cfg(test)]
mod test {
    use super::*;

    fn d(t: &str) -> (String, Option<[u8; 3]>, bool) {
        // Il 3 novembre 2026: 46329 giorni dal 1900.
        testo(&Value::Number(46329.0), t, DateSystem::default(), 11)
    }

    #[test]
    fn le_date_di_sistema_escono_giorno_mese_anno() {
        assert_eq!(d("m/d/yyyy").0, "03/11/2026");
        assert_eq!(d("mm-dd-yy").0, "03/11/2026");
        assert_eq!(d("d mmmm yyyy").0, "3 novembre 2026");
        assert_eq!(d("ddd d mmm").0, "mar 3 nov");
        assert_eq!(d("dddd").0, "martedì");
    }

    #[test]
    fn i_numeri_hanno_la_virgola_decimale() {
        let n = |v: f64, f: &str| testo(&Value::Number(v), f, DateSystem::default(), 11);
        assert_eq!(n(1234.5, "#,##0.00").0, "1.234,50");
        assert_eq!(n(0.125, "0.0%").0, "12,5%");
        assert_eq!(n(2.5, "General").0, "2,5");
        assert!(n(2.5, "General").2);
        assert_eq!(n(1234567.891, "General").0, "1234567,891");
        // Non c'è posto: Excel scrive i cancelletti.
        assert!(n(123456789.0, "General").0.len() <= 11);
        // Il colore del formato.
        assert_eq!(n(-3.0, "0;[Red]0").1, Some([0xFF, 0, 0]));
        assert_eq!(separatori("v1.2 e 3,4"), "v1,2 e 3.4");
    }

    #[test]
    fn vero_falso_e_gli_errori_in_italiano() {
        assert_eq!(testo(&Value::Bool(true), "General", DateSystem::default(), 11).0, "VERO");
        assert_eq!(testo(&Value::Error(CellError::NA), "General", DateSystem::default(), 11).0, "#N/D");
        assert_eq!(testo(&Value::Error(CellError::Div0), "General", DateSystem::default(), 11).0, "#DIV/0!");
        // Il testo resta com'è, anche coi punti e le virgole.
        assert_eq!(testo(&Value::Text("1.500,00 €".into()), "General", DateSystem::default(), 11).0, "1.500,00 €");
    }
}
