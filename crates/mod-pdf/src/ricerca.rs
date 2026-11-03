//! La ricerca nel testo delle pagine, senza maiuscole e senza accenti su tutti e due i lati del
//! confronto: «perche» trova «Perché» e «PERCHÉ», e anche un PDF che scrive la e e l'accento come
//! due caratteri (ricerca PDFCraft, sezione 4 e verifica). Il motore dà il testo di una pagina
//! (`PageText`) e, con `find_with`, la stringa dei suoi caratteri con gli spazi fra parole e
//! righe; qui la si porta alla forma del confronto tenendo, per ogni carattere, quello di
//! partenza, e i risultati tornano al motore come intervalli di caratteri, che li riporta ai glifi
//! e ai rettangoli delle righe.
//!
//! La ricerca tiene i risultati pagina per pagina, mentre i testi arrivano dal pool nell'ordine
//! che parte dalla pagina che si leggeva; il risultato attuale è il primo da lì in avanti, e lo si
//! sceglie solo quando le pagine prima di lui sono già state lette, così non salta.

use std::ops::Range;

use pdfcraft_render::PageText;
use pdfcraft_render::text::fold_case;
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

/// Un rettangolo in pt nella pagina come si vede, dall'alto a sinistra: [x0, y0, x1, y1].
pub type RettangoloPt = [f32; 4];

/// I caratteri `testo` nella forma del confronto: compatibilità aperta (NFKD: le legature
/// diventano lettere, le lettere accentate lettera e segno), via i segni combinanti, maiuscole
/// come il motore (`fold_case`), gli spazi di fila come uno solo e niente spazi in testa.
/// Restituisce i caratteri e, per ognuno, la posizione del carattere di `testo` da cui viene.
pub fn normalizza(testo: &[char]) -> (Vec<char>, Vec<usize>) {
    let mut fuori = Vec::with_capacity(testo.len());
    let mut da = Vec::with_capacity(testo.len());
    let spazio = |fuori: &mut Vec<char>, da: &mut Vec<usize>, i: usize| {
        if fuori.last().is_some_and(|c| *c != ' ') {
            fuori.push(' ');
            da.push(i);
        }
    };
    let mut buf = [0u8; 4];
    for (i, &c) in testo.iter().enumerate() {
        if c.is_whitespace() {
            spazio(&mut fuori, &mut da, i);
            continue;
        }
        // Apostrofi e virgolette tipografici come quelli della tastiera, un carattere per uno:
        // «dell'acqua» battuto trova «dell’acqua» dei PDF fatti con Word, e viceversa (analisi del
        // 27/10, lettura I3).
        let c = match c {
            '\u{2019}' | '\u{2018}' | '\u{201B}' | '\u{02BC}' | '\u{2032}' | '`' => '\'',
            '\u{00AB}' | '\u{00BB}' | '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' | '\u{2033}' => '"',
            c => c,
        };
        // La strada corta per l'ASCII, quasi tutto il testo: NFKD lo lascia com'è, non ha segni
        // combinanti e `fold_case` fa come `to_ascii_lowercase`.
        if c.is_ascii() {
            fuori.push(c.to_ascii_lowercase());
            da.push(i);
            continue;
        }
        for d in c.encode_utf8(&mut buf).nfkd() {
            if is_combining_mark(d) {
                continue;
            }
            if d.is_whitespace() {
                spazio(&mut fuori, &mut da, i);
                continue;
            }
            let mut b = [0u8; 4];
            for f in fold_case(d.encode_utf8(&mut b)).chars() {
                fuori.push(f);
                da.push(i);
            }
        }
    }
    (fuori, da)
}

/// La parola cercata nella forma del confronto, senza spazi in fondo; None se non resta niente.
pub fn ago(testo: &str) -> Option<Vec<char>> {
    let caratteri: Vec<char> = testo.chars().collect();
    let (mut ago, _) = normalizza(&caratteri);
    while ago.last() == Some(&' ') {
        ago.pop();
    }
    (!ago.is_empty()).then_some(ago)
}

/// Dove sta `ago` in `pagliaio`, senza sovrapposizioni, da sinistra.
fn posti(pagliaio: &[char], ago: &[char]) -> Vec<Range<usize>> {
    let mut fuori = Vec::new();
    if ago.is_empty() || ago.len() > pagliaio.len() {
        return fuori;
    }
    let mut i = 0;
    while i + ago.len() <= pagliaio.len() {
        if pagliaio[i..i + ago.len()] == *ago {
            fuori.push(i..i + ago.len());
            i += ago.len();
        } else {
            i += 1;
        }
    }
    fuori
}

/// Il risultato `r` della forma del confronto riportato ai caratteri di partenza `testo`, con `da`
/// che dice da quale carattere viene ognuno.
fn ai_caratteri(testo: &[char], da: &[usize], r: &Range<usize>) -> Range<usize> {
    // I segni che seguono l'ultima lettera (l'accento scritto a parte) sono suoi.
    let mut fine = da[r.end - 1] + 1;
    while testo.get(fine).is_some_and(|c| is_combining_mark(*c)) {
        fine += 1;
    }
    da[r.start]..fine
}

/// I caratteri di `testo` (come li dà `find_with`) dove si trova `ago`, già nella forma del
/// confronto: intervalli dei caratteri di partenza.
pub fn trova_nei_caratteri(testo: &[char], ago: &[char]) -> Vec<Range<usize>> {
    let (normale, da) = normalizza(testo);
    posti(&normale, ago).iter().map(|r| ai_caratteri(testo, &da, r)).collect()
}

/// I risultati di `ago` in una pagina, ognuno coi rettangoli delle sue righe.
pub fn trova_nella_pagina(testo: &PageText, ago: &[char]) -> Vec<Vec<RettangoloPt>> {
    trova_nella_pagina_normale(testo, &TestoNormale::new(testo), ago)
}

/// Il testo di una pagina nella forma del confronto, fatto una volta sola e tenuto accanto al
/// testo: a ogni tasto la ricerca confronta solo questo, e il motore rifà la stringa dei caratteri
/// (per riportare i risultati ai glifi) solo nelle pagine dove c'è qualcosa. Rinormalizzare tutti i
/// testi a ogni tasto, sul thread dell'interfaccia, costava 100 ms e più nei PDF di migliaia di
/// pagine (revisione della 0.3.0).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TestoNormale {
    caratteri: Vec<char>,
    /// Per ogni carattere di `caratteri`, la posizione del carattere da cui viene nella stringa di
    /// `find_with`.
    da: Vec<usize>,
}

impl TestoNormale {
    pub fn new(testo: &PageText) -> Self {
        let fatto = std::cell::RefCell::new(Self::default());
        testo.find_with(|caratteri| {
            let (caratteri, da) = normalizza(caratteri);
            *fatto.borrow_mut() = Self { caratteri, da };
            Vec::new()
        });
        fatto.into_inner()
    }

    /// Quanta memoria tiene, per il tetto dei testi.
    pub fn byte(&self) -> usize {
        self.caratteri.len() * std::mem::size_of::<char>() + self.da.len() * std::mem::size_of::<usize>()
    }
}

/// I risultati di `ago` in una pagina di cui si ha già la forma del confronto.
pub fn trova_nella_pagina_normale(testo: &PageText, normale: &TestoNormale, ago: &[char]) -> Vec<Vec<RettangoloPt>> {
    let trovati = posti(&normale.caratteri, ago);
    if trovati.is_empty() {
        return Vec::new();
    }
    testo
        .find_with(|caratteri| trovati.iter().map(|r| ai_caratteri(caratteri, &normale.da, r)).collect())
        .into_iter()
        .map(|glifi| testo.line_rects(glifi))
        .collect()
}

/// Una ricerca in corso o finita in un PDF.
#[derive(Debug, Clone)]
pub struct Ricerca {
    ago: Vec<char>,
    /// Per ogni pagina i risultati, ognuno coi rettangoli delle righe; None finché il testo della
    /// pagina non è stato letto.
    pagine: Vec<Option<Vec<Vec<RettangoloPt>>>>,
    /// La pagina da cui si comincia: quella che si leggeva.
    da: usize,
    /// Il risultato attuale: pagina e posto fra quelli della pagina.
    attuale: Option<(usize, usize)>,
    /// Le frecce sono state toccate: il risultato attuale lo sceglie chi legge.
    scelto: bool,
}

impl Ricerca {
    /// Una ricerca di `testo` in `pagine` pagine, dalla pagina `da`; None se `testo` non ha niente
    /// da cercare (vuoto, solo spazi o solo segni).
    pub fn new(testo: &str, pagine: usize, da: usize) -> Option<Self> {
        let ago = ago(testo)?;
        Some(Self { ago, pagine: vec![None; pagine], da: da.min(pagine.saturating_sub(1)), attuale: None, scelto: false })
    }

    /// La stessa parola, nella forma del confronto: «Perché» e «perche» sono la stessa ricerca.
    pub fn cerca_gia(&self, testo: &str) -> bool {
        ago(testo).is_some_and(|a| a == self.ago)
    }

    /// Il documento è cambiato: le pagine `pagine` (o tutte, con None, anche se ora sono
    /// `quante`) si rileggono; il risultato attuale, se stava lì, si sceglie di nuovo.
    pub fn da_rileggere(&mut self, pagine: Option<&[usize]>, quante: usize) {
        match pagine {
            Some(pagine) if quante == self.pagine.len() => {
                for &p in pagine {
                    if let Some(posto) = self.pagine.get_mut(p) {
                        *posto = None;
                    }
                }
                if self.attuale.is_some_and(|(p, _)| pagine.contains(&p)) {
                    self.attuale = None;
                    self.scelto = false;
                }
            }
            _ => {
                self.pagine = vec![None; quante];
                self.da = self.da.min(quante.saturating_sub(1));
                self.attuale = None;
                self.scelto = false;
            }
        }
    }

    /// Quanti caratteri ha la parola cercata, per la diagnosi (mai la parola).
    pub fn lunghezza(&self) -> usize {
        self.ago.len()
    }

    /// Le pagine nell'ordine della ricerca: da `da` in fondo, poi da capo.
    fn in_ordine(&self) -> impl Iterator<Item = usize> + '_ {
        let n = self.pagine.len();
        (0..n).map(move |k| (self.da + k) % n)
    }

    /// Le pagine il cui testo manca ancora, nell'ordine della ricerca.
    pub fn mancanti(&self) -> Vec<usize> {
        self.in_ordine().filter(|p| self.pagine[*p].is_none()).collect()
    }

    /// Tutte le pagine sono state lette.
    pub fn finita(&self) -> bool {
        self.pagine.iter().all(Option::is_some)
    }

    /// Le pagine lette.
    pub fn lette(&self) -> usize {
        self.pagine.iter().filter(|p| p.is_some()).count()
    }

    /// Il testo della pagina `pagina` è arrivato. Restituisce la pagina del risultato attuale se è
    /// stato appena scelto da solo (da mostrare).
    pub fn leggi(&mut self, pagina: usize, testo: &PageText) -> Option<usize> {
        let trovati = trova_nella_pagina(testo, &self.ago);
        self.metti(pagina, trovati)
    }

    /// Come [`leggi`](Self::leggi), col testo già nella forma del confronto.
    pub fn leggi_normale(&mut self, pagina: usize, testo: &PageText, normale: &TestoNormale) -> Option<usize> {
        let trovati = trova_nella_pagina_normale(testo, normale, &self.ago);
        self.metti(pagina, trovati)
    }

    /// La pagina `pagina` non ha un testo che si legge: vale come una pagina senza risultati.
    pub fn illeggibile(&mut self, pagina: usize) -> Option<usize> {
        self.metti(pagina, Vec::new())
    }

    fn metti(&mut self, pagina: usize, trovati: Vec<Vec<RettangoloPt>>) -> Option<usize> {
        let posto = self.pagine.get_mut(pagina)?;
        if posto.is_some() {
            return None;
        }
        *posto = Some(trovati);
        if self.scelto || self.attuale.is_some() {
            return None;
        }
        // Il primo risultato dalla pagina di partenza, se le pagine prima di lui ci sono tutte.
        let ordine: Vec<usize> = self.in_ordine().collect();
        for p in ordine {
            match &self.pagine[p] {
                None => return None,
                Some(v) if !v.is_empty() => {
                    self.attuale = Some((p, 0));
                    return Some(p);
                }
                Some(_) => {}
            }
        }
        None
    }

    /// Quanti risultati, finora.
    pub fn totale(&self) -> usize {
        self.pagine.iter().flatten().map(Vec::len).sum()
    }

    /// Il risultato attuale: pagina e posto nella pagina.
    pub fn attuale(&self) -> Option<(usize, usize)> {
        self.attuale
    }

    /// Il numero del risultato attuale fra tutti (da 0), nell'ordine delle pagine.
    pub fn numero_attuale(&self) -> Option<usize> {
        let (p, i) = self.attuale?;
        Some(self.pagine[..p].iter().flatten().map(Vec::len).sum::<usize>() + i)
    }

    /// I risultati della pagina `pagina`, coi rettangoli delle righe.
    pub fn risultati(&self, pagina: usize) -> &[Vec<RettangoloPt>] {
        self.pagine.get(pagina).and_then(|p| p.as_deref()).unwrap_or(&[])
    }

    /// Il rettangolo che contiene tutto il risultato (pagina, posto).
    pub fn rettangolo(&self, (p, i): (usize, usize)) -> Option<RettangoloPt> {
        let righe = self.risultati(p).get(i)?;
        righe.iter().copied().reduce(|a, b| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])])
    }

    /// Il risultato dopo (o prima), girando dalla fine all'inizio; salta le pagine non ancora
    /// lette. Da qui in poi il risultato attuale non cambia più da solo.
    pub fn altro(&mut self, avanti: bool) -> Option<(usize, usize)> {
        let n = self.pagine.len();
        if self.totale() == 0 || n == 0 {
            return None;
        }
        self.scelto = true;
        let quanti = |p: usize| self.pagine[p].as_ref().map_or(0, Vec::len);
        let nuovo = match self.attuale {
            Some((p, i)) if avanti && i + 1 < quanti(p) => (p, i + 1),
            Some((p, i)) if !avanti && i > 0 => (p, i - 1),
            Some((p, _)) => {
                let mut q = p;
                loop {
                    q = if avanti { (q + 1) % n } else { (q + n - 1) % n };
                    if quanti(q) > 0 {
                        break (q, if avanti { 0 } else { quanti(q) - 1 });
                    }
                }
            }
            None => {
                // Il primo dalla pagina di partenza in avanti (o l'ultimo prima, all'indietro).
                let ordine: Vec<usize> =
                    if avanti { self.in_ordine().collect() } else { self.in_ordine().collect::<Vec<_>>().into_iter().rev().collect() };
                let q = ordine.into_iter().find(|q| quanti(*q) > 0)?;
                (q, if avanti { 0 } else { quanti(q) - 1 })
            }
        };
        self.attuale = Some(nuovo);
        Some(nuovo)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::prova::{Pagina, pdf};

    fn caratteri(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    fn trova(testo: &str, cercato: &str) -> Vec<String> {
        let c = caratteri(testo);
        trova_nei_caratteri(&c, &ago(cercato).unwrap()).into_iter().map(|r| c[r].iter().collect()).collect()
    }

    #[test]
    fn senza_maiuscole_e_senza_accenti_da_tutti_e_due_i_lati() {
        assert_eq!(trova("Perché sì, PERCHÉ no, perche forse", "perche"), ["Perché", "PERCHÉ", "perche"]);
        assert_eq!(trova("Perché sì, PERCHÉ no, perche forse", "PERCHÉ"), ["Perché", "PERCHÉ", "perche"]);
        assert_eq!(trova("la città di Forlì", "citta"), ["città"]);
        // La e scritta come e più accento combinante (U+0301): si trova lo stesso, coi due caratteri.
        assert_eq!(trova("perche\u{301} no", "perché"), ["perche\u{301}"]);
        assert_eq!(trova("perche\u{301} no", "perche"), ["perche\u{301}"]);
        // La legatura fi è due lettere.
        assert_eq!(trova("il \u{fb01}glio", "figlio"), ["\u{fb01}glio"]);
        // La i turca e la sigma finale come il motore.
        assert_eq!(trova("İSTANBUL", "istanbul"), ["İSTANBUL"]);
        assert_eq!(trova("λόγος", "ΛΟΓΟΣ"), ["λόγος"]);
    }

    /// La forma del confronto di un carattere per la strada lunga (NFKD, via i segni, `fold_case`).
    fn per_la_strada_lunga(c: char) -> Vec<char> {
        let mut b = [0u8; 4];
        c.encode_utf8(&mut b)
            .nfkd()
            .filter(|d| !is_combining_mark(*d))
            .flat_map(|d| fold_case(d.encode_utf8(&mut [0u8; 4])).chars().collect::<Vec<_>>())
            .collect()
    }

    #[test]
    fn apostrofi_e_virgolette_tipografici_come_quelli_della_tastiera() {
        // Analisi del 27/10, lettura I3: «dell'acqua» battuto con la tastiera non trovava
        // «dell’acqua» dei PDF fatti con Word, e viceversa.
        assert_eq!(normalizza(&caratteri("dell’acqua")).0, normalizza(&caratteri("dell'acqua")).0);
        assert_eq!(normalizza(&caratteri("l‘ora l`ora lʼora")).0, caratteri("l'ora l'ora l'ora"));
        assert_eq!(normalizza(&caratteri("«Visto» “sì”")).0, caratteri("\"visto\" \"si\""));
        // Un carattere per uno: il posto nel testo di prima resta giusto.
        assert_eq!(normalizza(&caratteri("a’b")).1, vec![0, 1, 2]);
    }

    #[test]
    fn l_ascii_per_la_strada_corta_come_per_quella_lunga() {
        for b in 0u8..128 {
            let c = char::from(b);
            // L'accento grave vale un apostrofo, apposta: lo prova quella degli apostrofi.
            if c.is_whitespace() || c == '`' {
                continue;
            }
            let (fuori, da) = normalizza(&['x', c]);
            assert_eq!(fuori[1..], per_la_strada_lunga(c), "{c:?}");
            assert!(da[1..].iter().all(|i| *i == 1), "{c:?}");
        }
        assert_eq!(normalizza(&caratteri("A\tB  c")).0, caratteri("a b c"));
    }

    #[test]
    fn col_testo_normale_tenuto_gli_stessi_risultati() {
        let p = tre_pagine();
        for i in 0..3 {
            let t = p.testo(i).unwrap();
            let normale = TestoNormale::new(&t);
            assert!(normale.byte() > 0);
            for cercato in ["perche", "PERCHÉ si", "niente", "zebra"] {
                let a = ago(cercato).unwrap();
                assert_eq!(trova_nella_pagina_normale(&t, &normale, &a), trova_nella_pagina(&t, &a), "pagina {i}, {cercato}");
            }
        }
        // La e scritta come e più accento combinante: il segno resta del risultato anche così.
        let mut r = Ricerca::new("perche", 1, 0).unwrap();
        let t = p.testo(0).unwrap();
        assert_eq!(r.leggi_normale(0, &t, &TestoNormale::new(&t)), Some(0));
        assert_eq!(r.totale(), 2);
    }

    #[test]
    fn gli_spazi_valgono_uno_e_i_risultati_non_si_sovrappongono() {
        assert_eq!(trova("due  spazi\u{a0}e  basta", "spazi e"), ["spazi\u{a0}e"]);
        assert_eq!(trova("aaaa", "aa"), ["aa", "aa"]);
        assert_eq!(trova("niente", "zebra"), Vec::<String>::new());
        assert!(ago("   ").is_none());
        assert!(ago("\u{301}").is_none());
        assert_eq!(ago(" Città ").unwrap(), caratteri("citta"));
    }

    /// Tre pagine col testo in Helvetica: la prima e la terza con «Perché», la seconda senza.
    fn tre_pagine() -> crate::Pdf {
        let p1 = "BT /F1 12 Tf 50 700 Td (Perch\\351 no) Tj 0 -20 Td (PERCH\\311 si) Tj ET";
        let p2 = "BT /F1 12 Tf 50 700 Td (niente qui) Tj ET";
        let p3 = "BT /F1 12 Tf 50 700 Td (ancora perche) Tj ET";
        let pagina = |contenuto| Pagina { larghezza: 595.0, altezza: 842.0, rotazione: 0, contenuto };
        crate::Pdf::apri(pdf(&[pagina(p1), pagina(p2), pagina(p3)]), None).unwrap()
    }

    #[test]
    fn nel_testo_vero_di_una_pagina_coi_rettangoli() {
        let p = tre_pagine();
        let testi: Vec<_> = (0..3).map(|i| p.testo(i).unwrap()).collect();
        let trovati = trova_nella_pagina(&testi[0], &ago("perche").unwrap());
        assert_eq!(trovati.len(), 2);
        // Una riga per risultato, la prima più in alto (y dall'alto), a 50 pt dal bordo sinistro.
        assert_eq!(trovati[0].len(), 1);
        let (a, b) = (trovati[0][0], trovati[1][0]);
        assert!(a[1] < b[1], "{a:?} {b:?}");
        assert!((a[0] - 50.0).abs() < 1.0 && a[2] > a[0] + 30.0 && (a[3] - a[1]) > 5.0, "{a:?}");
        assert!((842.0 - 700.0 - a[3]).abs() < 6.0, "la riga sta a 700 pt dal fondo: {a:?}");
        assert!(trova_nella_pagina(&testi[1], &ago("perche").unwrap()).is_empty());
    }

    #[test]
    fn la_ricerca_conta_sceglie_il_primo_dalla_pagina_di_partenza_e_gira() {
        let p = tre_pagine();
        let testi: Vec<_> = (0..3).map(|i| p.testo(i).unwrap()).collect();
        // Si leggeva la seconda pagina: si comincia da lì.
        let mut r = Ricerca::new("PERCHE", 3, 1).unwrap();
        assert_eq!(r.mancanti(), [1, 2, 0]);
        assert_eq!(r.lunghezza(), 6);
        assert!(r.cerca_gia("perché") && !r.cerca_gia("perch"));
        // La terza arriva prima della seconda: il suo risultato non è ancora sicuro.
        assert_eq!(r.leggi(2, &testi[2]), None);
        assert_eq!(r.attuale(), None);
        assert_eq!(r.leggi(1, &testi[1]), Some(2));
        assert_eq!(r.attuale(), Some((2, 0)));
        assert!(!r.finita());
        assert_eq!((r.totale(), r.numero_attuale()), (1, Some(0)));
        // La prima arriva per ultima: il risultato attuale resta, il suo numero cresce.
        assert_eq!(r.leggi(0, &testi[0]), None);
        assert!(r.finita());
        assert_eq!((r.totale(), r.numero_attuale()), (3, Some(2)));
        assert_eq!(r.risultati(0).len(), 2);
        // Avanti gira all'inizio, indietro torna.
        assert_eq!(r.altro(true), Some((0, 0)));
        assert_eq!(r.altro(true), Some((0, 1)));
        assert_eq!(r.altro(true), Some((2, 0)));
        assert_eq!(r.altro(false), Some((0, 1)));
        assert_eq!(r.numero_attuale(), Some(1));
        let rett = r.rettangolo((0, 1)).unwrap();
        assert!(rett[1] > r.rettangolo((0, 0)).unwrap()[1]);
        // Una pagina letta due volte non conta due volte.
        assert_eq!(r.leggi(0, &testi[0]), None);
        assert_eq!(r.totale(), 3);
    }

    #[test]
    fn dopo_una_modifica_si_rileggono_le_pagine_toccate() {
        let p = tre_pagine();
        let testi: Vec<_> = (0..3).map(|i| p.testo(i).unwrap()).collect();
        let mut r = Ricerca::new("perche", 3, 0).unwrap();
        for (i, t) in testi.iter().enumerate() {
            r.leggi(i, t);
        }
        assert_eq!((r.totale(), r.attuale()), (3, Some((0, 0))));
        // La terza pagina da rileggere: il risultato attuale resta nella prima.
        r.da_rileggere(Some(&[2]), 3);
        assert_eq!((r.finita(), r.totale(), r.mancanti(), r.attuale()), (false, 2, vec![2], Some((0, 0))));
        r.leggi(2, &testi[2]);
        assert_eq!(r.totale(), 3);
        // La prima: il risultato attuale si sceglie di nuovo quando arriva.
        r.da_rileggere(Some(&[0]), 3);
        assert_eq!(r.attuale(), None);
        assert_eq!(r.leggi(0, &testi[0]), Some(0));
        // Tutte, con le pagine che cambiano: da capo.
        r.da_rileggere(None, 4);
        assert_eq!((r.mancanti().len(), r.totale(), r.attuale()), (4, 0, None));
    }

    #[test]
    fn senza_risultati_e_pagine_illeggibili() {
        let mut r = Ricerca::new("zebra", 2, 0).unwrap();
        assert_eq!(r.illeggibile(0), None);
        assert_eq!(r.illeggibile(1), None);
        assert!(r.finita());
        assert_eq!((r.totale(), r.attuale(), r.altro(true)), (0, None, None));
        assert!(Ricerca::new("  ", 3, 0).is_none());
        // Pagina fuori dal PDF: niente.
        assert_eq!(r.illeggibile(7), None);
    }
}
