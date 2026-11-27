//! Le date della Home nella lingua dell'app, nell'ora del telefono: in testa «Sabato 24 ottobre»,
//! accanto ai recenti quando sono stati aperti («oggi 08:12», «ieri 19:40», «21 ottobre», «3 marzo
//! 2025»). In inglese il giorno viene prima del mese, come nel resto dell'app (variante
//! internazionale: «Saturday 24 October»); in spagnolo «Sábado, 24 de octubre».
//! I conti si fanno sui giorni del calendario, non sulle 24 ore: il giorno del cambio dell'ora ne
//! ha 23 o 25, e «ieri» resta ieri.

use tascabile_touch::Lingua;
use tascabile_word::salva;

const GIORNI: [&str; 7] = ["Lunedì", "Martedì", "Mercoledì", "Giovedì", "Venerdì", "Sabato", "Domenica"];
const MESI: [&str; 12] =
    ["gennaio", "febbraio", "marzo", "aprile", "maggio", "giugno", "luglio", "agosto", "settembre", "ottobre", "novembre", "dicembre"];
const DAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const DIAS: [&str; 7] = ["Lunes", "Martes", "Miércoles", "Jueves", "Viernes", "Sábado", "Domingo"];
const MESES: [&str; 12] =
    ["enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto", "septiembre", "octubre", "noviembre", "diciembre"];

/// Un istante nell'ora del telefono.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Locale {
    pub anno: i32,
    /// Da 1 a 12.
    pub mese: u32,
    /// Da 1 a 31.
    pub giorno: u32,
    pub ora: u32,
    pub minuti: u32,
}

impl Locale {
    /// L'istante `secondi_utc` (secondi dal 1970) nell'ora del telefono.
    pub fn da_utc(secondi_utc: u64) -> Self {
        let (anno, mese, giorno, ora, minuti) = salva::ora_locale(secondi_utc.min(i64::MAX as u64) as i64);
        Self { anno, mese, giorno, ora, minuti }
    }

    /// Il giorno del calendario come numero: giorni dal 1° gennaio 1970 (`days_from_civil` di
    /// Howard Hinnant). Due giorni di fila hanno numeri di fila, anche fra un mese e l'altro.
    fn numero_del_giorno(&self) -> i64 {
        let (m, d) = (i64::from(self.mese), i64::from(self.giorno));
        let y = i64::from(self.anno) - i64::from(m <= 2);
        let era = y.div_euclid(400);
        let anno_dell_era = y - era * 400;
        let giorno_dell_anno = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
        let giorno_dell_era = anno_dell_era * 365 + anno_dell_era / 4 - anno_dell_era / 100 + giorno_dell_anno;
        era * 146_097 + giorno_dell_era - 719_468
    }

    fn mese(&self, lingua: Lingua) -> &'static str {
        let i = (self.mese.clamp(1, 12) - 1) as usize;
        match lingua {
            Lingua::Italiano => MESI[i],
            Lingua::Inglese => MONTHS[i],
            Lingua::Spagnolo => MESES[i],
        }
    }
}

/// La data in testa alla Home, nella lingua dell'app: «Sabato 24 ottobre».
pub fn data_lunga(oggi: &Locale) -> String {
    data_lunga_in(oggi, tascabile_touch::lingua::attiva())
}

/// La data in testa alla Home in `lingua`: «Sabato 24 ottobre», «Saturday 24 October», «Sábado,
/// 24 de octubre».
pub fn data_lunga_in(oggi: &Locale, lingua: Lingua) -> String {
    // Il 1° gennaio 1970 era un giovedì.
    let g = (oggi.numero_del_giorno() + 3).rem_euclid(7) as usize;
    match lingua {
        Lingua::Italiano => format!("{} {} {}", GIORNI[g], oggi.giorno, oggi.mese(lingua)),
        Lingua::Inglese => format!("{} {} {}", DAYS[g], oggi.giorno, oggi.mese(lingua)),
        Lingua::Spagnolo => format!("{}, {} de {}", DIAS[g], oggi.giorno, oggi.mese(lingua)),
    }
}

/// Quando è stato aperto un documento, visto da `oggi`, nella lingua dell'app.
pub fn quando(aperto: &Locale, oggi: &Locale) -> String {
    quando_in(aperto, oggi, tascabile_touch::lingua::attiva())
}

/// Quando è stato aperto un documento, visto da `oggi`: «oggi 08:12», «ieri 19:40», «21 ottobre»
/// quest'anno, «3 marzo 2025» gli anni prima (e se l'orologio del telefono è andato indietro).
pub fn quando_in(aperto: &Locale, oggi: &Locale, lingua: Lingua) -> String {
    let (oggi_detto, ieri_detto, di) = match lingua {
        Lingua::Italiano => ("oggi", "ieri", ""),
        Lingua::Inglese => ("today", "yesterday", ""),
        Lingua::Spagnolo => ("hoy", "ayer", "de "),
    };
    let mese = aperto.mese(lingua);
    match oggi.numero_del_giorno() - aperto.numero_del_giorno() {
        0 => format!("{oggi_detto} {:02}:{:02}", aperto.ora, aperto.minuti),
        1 => format!("{ieri_detto} {:02}:{:02}", aperto.ora, aperto.minuti),
        _ if aperto.anno == oggi.anno => format!("{} {di}{mese}", aperto.giorno),
        _ => format!("{} {di}{mese} {di}{}", aperto.giorno, aperto.anno),
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn l(anno: i32, mese: u32, giorno: u32, ora: u32, minuti: u32) -> Locale {
        Locale { anno, mese, giorno, ora, minuti }
    }

    #[test]
    fn la_data_lunga_col_giorno_della_settimana() {
        assert_eq!(data_lunga(&l(2026, 10, 24, 10, 52)), "Sabato 24 ottobre");
        assert_eq!(data_lunga(&l(2026, 3, 1, 0, 0)), "Domenica 1 marzo");
        assert_eq!(data_lunga(&l(2024, 2, 29, 12, 0)), "Giovedì 29 febbraio");
        assert_eq!(data_lunga(&l(1970, 1, 1, 0, 0)), "Giovedì 1 gennaio");
        assert_eq!(data_lunga(&l(2027, 1, 4, 8, 0)), "Lunedì 4 gennaio");
    }

    #[test]
    fn oggi_ieri_e_poi_la_data() {
        let oggi = l(2026, 10, 24, 10, 52);
        assert_eq!(quando(&l(2026, 10, 24, 8, 12), &oggi), "oggi 08:12");
        assert_eq!(quando(&l(2026, 10, 23, 19, 40), &oggi), "ieri 19:40");
        assert_eq!(quando(&l(2026, 10, 21, 9, 0), &oggi), "21 ottobre");
        assert_eq!(quando(&l(2025, 3, 3, 9, 0), &oggi), "3 marzo 2025");
        // L'orologio andato indietro: la data, mai «domani».
        assert_eq!(quando(&l(2026, 10, 25, 9, 0), &oggi), "25 ottobre");
    }

    #[test]
    fn ieri_anche_fra_due_mesi_e_due_anni() {
        assert_eq!(quando(&l(2026, 10, 31, 23, 59), &l(2026, 11, 1, 0, 5)), "ieri 23:59");
        assert_eq!(quando(&l(2026, 12, 31, 22, 0), &l(2027, 1, 1, 7, 0)), "ieri 22:00");
        assert_eq!(quando(&l(2024, 2, 28, 22, 0), &l(2024, 3, 1, 7, 0)), "28 febbraio", "il 2024 ha il 29 febbraio");
        // Il giorno del cambio dell'ora ha 23 ore: conta il calendario.
        assert_eq!(quando(&l(2026, 3, 29, 0, 30), &l(2026, 3, 30, 0, 10)), "ieri 00:30");
    }

    #[test]
    fn le_date_in_inglese_e_in_spagnolo() {
        let oggi = l(2026, 10, 24, 10, 52);
        assert_eq!(data_lunga_in(&oggi, Lingua::Inglese), "Saturday 24 October");
        assert_eq!(data_lunga_in(&oggi, Lingua::Spagnolo), "Sábado, 24 de octubre");
        assert_eq!(quando_in(&l(2026, 10, 24, 8, 12), &oggi, Lingua::Inglese), "today 08:12");
        assert_eq!(quando_in(&l(2026, 10, 23, 19, 40), &oggi, Lingua::Spagnolo), "ayer 19:40");
        assert_eq!(quando_in(&l(2026, 10, 21, 9, 0), &oggi, Lingua::Inglese), "21 October");
        assert_eq!(quando_in(&l(2026, 10, 21, 9, 0), &oggi, Lingua::Spagnolo), "21 de octubre");
        assert_eq!(quando_in(&l(2025, 3, 3, 9, 0), &oggi, Lingua::Inglese), "3 March 2025");
        assert_eq!(quando_in(&l(2025, 3, 3, 9, 0), &oggi, Lingua::Spagnolo), "3 de marzo de 2025");
        assert_eq!(quando_in(&l(2025, 3, 3, 9, 0), &oggi, Lingua::Italiano), "3 marzo 2025");
    }

    #[test]
    fn dall_ora_utc_all_ora_del_telefono() {
        // Senza conoscere il fuso della macchina: lo stesso istante dà sempre lo stesso giorno,
        // e un'ora dopo l'ora è avanti di uno (o il giorno è cambiato).
        let a = Locale::da_utc(1_792_800_000);
        let b = Locale::da_utc(1_792_800_000 + 3600);
        assert!(b.numero_del_giorno() >= a.numero_del_giorno());
        assert!((1..=12).contains(&a.mese) && (1..=31).contains(&a.giorno) && a.ora < 24 && a.minuti < 60);
        assert_eq!(a.minuti, b.minuti);
    }
}
