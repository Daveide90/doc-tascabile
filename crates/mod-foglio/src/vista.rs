//! La griglia di un foglio: le misure delle righe e delle colonne ([`Assi`], pixel a 96 dpi come
//! nel file) e, per una zona, le celle da disegnare col loro testo all'italiana, i colori, i bordi,
//! l'allineamento ([`caselle`]). Il disegno lo fa l'app con egui: qui niente egui, solo dati
//! (ricerca, sezioni 6 e 7, dal disegno della griglia di GridCraft).

use gridcraft_core::{CellRef, MAX_COLS, MAX_ROWS, RangeRef, Value};
use gridcraft_model::{BorderLine, HAlign, LineIndex, PatternType, Sheet, Underline, VAlign, Workbook};

/// La larghezza di una cifra del carattere standard (Calibri 11) a 96 dpi: con lei si contano le
/// cifre che stanno in una colonna.
const CIFRA: f64 = 7.0;
/// Il testo di una cella sborda nelle vuote accanto fino a tante colonne.
const SBORDA: u32 = 20;
/// Le righe e le colonne visibili che si disegnano al più in un pezzo di griglia. Allo zoom più
/// piccolo un telefono ne mostra un centinaio di righe e una ventina di colonne: il tetto ferma
/// un file costruito apposta (righe alte un pixel, centomila righe bloccate) che farebbe girare
/// il disegno su milioni di righe a ogni fotogramma (revisione del 3/11, M1).
pub const RIGHE_DISEGNATE: usize = 400;
pub const COLONNE_DISEGNATE: usize = 150;
/// Il testo di una cella sullo schermo, e nel pannello del tocco, al più: una cella può avere un
/// testo di decine di MB, che si copierebbe e comporrebbe a ogni fotogramma (M2).
const TESTO_IN_CELLA: usize = 2_000;
const TESTO_IN_DETTAGLIO: usize = 20_000;

/// Le misure di un foglio: dove comincia ogni riga e ogni colonna, in pixel a 96 dpi; le
/// nascoste larghe zero. E le corse di righe e di colonne nascoste, [prima, ultima] in ordine,
/// per saltarle senza contarle una per una.
pub struct Assi {
    pub righe: LineIndex,
    pub colonne: LineIndex,
    righe_vuote: Vec<(u32, u32)>,
    colonne_vuote: Vec<(u32, u32)>,
}

/// Le corse delle linee nascoste o larghe zero.
fn corse_vuote(linee: &std::collections::BTreeMap<u32, gridcraft_model::LineInfo>) -> Vec<(u32, u32)> {
    let mut corse: Vec<(u32, u32)> = Vec::new();
    for (&l, info) in linee {
        if !(info.hidden || info.size.is_some_and(|s| s <= 0.0)) {
            continue;
        }
        match corse.last_mut() {
            Some(c) if c.1.checked_add(1) == Some(l) => c.1 = l,
            _ => corse.push((l, l)),
        }
    }
    corse
}

/// Le linee visibili (larghe più di zero) da `da` ad `a` comprese, al più `massimo`. Le corse
/// nascoste si saltano intere; le linee larghe zero fuori dalle corse (un file con la misura
/// standard zero) si contano, ma al più quattro volte `massimo`.
fn visibili(indice: &LineIndex, vuote: &[(u32, u32)], da: u32, a: u32, massimo: usize) -> Vec<u32> {
    let mut fuori = Vec::new();
    let mut l = da;
    let mut giri = 0;
    while l <= a && fuori.len() < massimo && giri < 4 * massimo {
        giri += 1;
        let i = vuote.partition_point(|c| c.1 < l);
        if let Some(&(prima, ultima)) = vuote.get(i)
            && prima <= l
        {
            let Some(dopo) = ultima.checked_add(1) else { break };
            l = dopo;
            continue;
        }
        if indice.start(l + 1) > indice.start(l) {
            fuori.push(l);
        }
        l += 1;
    }
    fuori
}

impl Assi {
    pub fn di(sh: &Sheet) -> Assi {
        Assi {
            righe: LineIndex::new(&sh.rows, sh.default_row_height, MAX_ROWS),
            colonne: LineIndex::new(&sh.cols, sh.default_col_width, MAX_COLS),
            righe_vuote: corse_vuote(&sh.rows),
            colonne_vuote: corse_vuote(&sh.cols),
        }
    }

    /// Le righe visibili da `da` ad `a`, al più [`RIGHE_DISEGNATE`].
    pub fn righe_visibili(&self, da: u32, a: u32) -> Vec<u32> {
        visibili(&self.righe, &self.righe_vuote, da, a.min(MAX_ROWS - 1), RIGHE_DISEGNATE)
    }

    /// Le colonne visibili da `da` ad `a`, al più [`COLONNE_DISEGNATE`].
    pub fn colonne_visibili(&self, da: u32, a: u32) -> Vec<u32> {
        visibili(&self.colonne, &self.colonne_vuote, da, a.min(MAX_COLS - 1), COLONNE_DISEGNATE)
    }

    /// La riga e la colonna che contengono il punto (x, y) del foglio.
    pub fn cella_a(&self, x: f64, y: f64) -> CellRef {
        CellRef::new(self.righe.at(y), self.colonne.at(x))
    }

    /// Il rettangolo [x0, y0, x1, y1] di una zona.
    pub fn rettangolo(&self, r: RangeRef) -> [f64; 4] {
        [self.colonne.start(r.start.col), self.righe.start(r.start.row), self.colonne.start(r.end.col + 1), self.righe.start(r.end.row + 1)]
    }
}

/// L'allineamento del testo in una cella.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Allinea {
    Inizio,
    Mezzo,
    Fine,
}

/// Un bordo: spessore (pixel a 96 dpi) e colore.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bordo {
    pub spessore: f32,
    pub colore: [u8; 3],
}

/// Una cella da disegnare.
#[derive(Debug, Clone, PartialEq)]
pub struct Casella {
    pub dove: CellRef,
    /// Il rettangolo [x0, y0, x1, y1] (pixel a 96 dpi dall'angolo del foglio): per una cella unita,
    /// tutta l'unione.
    pub rett: [f64; 4],
    /// Fin dove può andare il testo che non va a capo (le celle vuote accanto), a sinistra e a
    /// destra.
    pub spazio: [f64; 2],
    pub testo: String,
    /// Un numero (o una data): se non sta nella colonna diventa «###», come in Excel; un testo
    /// no, anche se ha delle cifre.
    pub numero: bool,
    pub sfondo: Option<[u8; 3]>,
    pub colore: [u8; 3],
    pub carattere: String,
    /// Punti tipografici del carattere.
    pub misura: f32,
    pub grassetto: bool,
    pub corsivo: bool,
    pub sottolineato: bool,
    pub barrato: bool,
    pub orizzontale: Allinea,
    pub verticale: Allinea,
    pub a_capo: bool,
    pub rientro: u8,
    /// Sinistra, alto, destra, basso.
    pub bordi: [Option<Bordo>; 4],
}

/// Un testo lungo accorciato a `massimo` caratteri, coi puntini; `None` se ci sta.
fn accorcia(t: &str, massimo: usize) -> Option<String> {
    let (fine, _) = t.char_indices().nth(massimo).filter(|_| t.len() > massimo)?;
    Some(format!("{}…", &t[..fine]))
}

/// Il valore di una cella col testo accorciato a `massimo` caratteri.
fn accorciato(v: Value, massimo: usize) -> Value {
    match &v {
        Value::Text(t) => accorcia(t, massimo).map_or(v, |a| Value::Text(a.into())),
        _ => v,
    }
}

fn bordo(b: &BorderLine, wb: &Workbook) -> Option<Bordo> {
    (!b.is_none()).then(|| Bordo { spessore: b.style.width(), colore: b.color.resolve(&wb.theme).unwrap_or([0, 0, 0]) })
}

/// Le celle da disegnare nelle righe `righe` e nelle colonne `colonne` (le une e le altre
/// comprese): solo quelle con qualcosa da vedere (testo, sfondo, bordi). Le unioni si danno una
/// volta, intere, se toccano la zona. Solo le righe e le colonne visibili, al più
/// [`RIGHE_DISEGNATE`] e [`COLONNE_DISEGNATE`].
pub fn caselle(wb: &Workbook, sh: &Sheet, assi: &Assi, righe: (u32, u32), colonne: (u32, u32)) -> Vec<Casella> {
    let righe = assi.righe_visibili(righe.0, righe.1);
    let colonne = assi.colonne_visibili(colonne.0, colonne.1);
    let (Some(&r0), Some(&r1), Some(&c0), Some(&c1)) = (righe.first(), righe.last(), colonne.first(), colonne.last()) else {
        return Vec::new();
    };
    let zona = RangeRef::new(CellRef::new(r0, c0.saturating_sub(SBORDA)), CellRef::new(r1, (c1 + SBORDA).min(MAX_COLS - 1)));
    let unioni: Vec<RangeRef> = sh.merges.iter().filter(|m| m.intersects(&zona)).copied().collect();
    let unione_di = |c: CellRef| unioni.iter().find(|m| m.contains(c)).copied();
    let piena = |c: CellRef| !sh.value(c).is_empty() || unione_di(c).is_some_and(|m| m.start != c || m.end != c);
    let mut fuori = Vec::new();
    let mut fatte: Vec<RangeRef> = Vec::new();
    for &r in &righe {
        // Anche le celle a sinistra fuori dalla zona il cui testo sborda dentro.
        for c in (c0.saturating_sub(SBORDA)..c0).chain(colonne.iter().copied()) {
            let dove = CellRef::new(r, c);
            let area = match unione_di(dove) {
                Some(m) if fatte.contains(&m) => continue,
                Some(m) => {
                    fatte.push(m);
                    m
                }
                None => RangeRef::cell(dove),
            };
            if sh.col_width(area.start.col) <= 0.0 && area.start == area.end {
                continue;
            }
            let stile = wb.styles.get(sh.style_id(area.start));
            let valore = accorciato(sh.value(area.start), TESTO_IN_CELLA);
            let sfondo = match stile.fill.pattern {
                PatternType::None => None,
                PatternType::Solid => stile.fill.fg.resolve(&wb.theme),
                _ => stile.fill.bg.resolve(&wb.theme).or_else(|| stile.fill.fg.resolve(&wb.theme)),
            };
            let b = &stile.borders;
            let bordi = [bordo(&b.left, wb), bordo(&b.top, wb), bordo(&b.right, wb), bordo(&b.bottom, wb)];
            // Fuori dalla zona se l'area intera è a sinistra: un'unione colorata che comincia
            // prima e arriva dentro si disegna.
            let fuori_zona = area.end.col < c0;
            if valore.is_empty() && (fuori_zona || (sfondo.is_none() && bordi.iter().all(Option::is_none))) {
                continue;
            }
            let rett = assi.rettangolo(area);
            let cifre = (((rett[2] - rett[0]) - 4.0) / CIFRA).floor().max(1.0) as usize;
            let (testo, colore_formato, numero) = if !sh.show_zeros && valore.as_f64() == Some(0.0) {
                (String::new(), None, false)
            } else {
                crate::italiano::testo(&valore, stile.num_fmt.as_str(), wb.date_system, cifre)
            };
            let orizzontale = match stile.align.h {
                HAlign::Center | HAlign::CenterAcross => Allinea::Mezzo,
                HAlign::Right => Allinea::Fine,
                HAlign::General if numero => Allinea::Fine,
                HAlign::General if matches!(valore, Value::Bool(_) | Value::Error(_)) => Allinea::Mezzo,
                _ => Allinea::Inizio,
            };
            let verticale = match stile.align.v {
                VAlign::Top => Allinea::Inizio,
                VAlign::Center | VAlign::Justify | VAlign::Distributed => Allinea::Mezzo,
                _ => Allinea::Fine,
            };
            // Lo spazio per il testo che sborda: le celle vuote accanto, verso dove va il testo.
            let mut spazio = [rett[0], rett[2]];
            if !stile.align.wrap && area.start == area.end && !numero && !testo.is_empty() {
                if orizzontale != Allinea::Fine {
                    let mut k = c + 1;
                    while k < (c + SBORDA).min(MAX_COLS) && !piena(CellRef::new(r, k)) {
                        k += 1;
                    }
                    spazio[1] = assi.colonne.start(k);
                }
                if orizzontale != Allinea::Inizio {
                    let mut k = c;
                    while k > c.saturating_sub(SBORDA) && !piena(CellRef::new(r, k - 1)) {
                        k -= 1;
                    }
                    spazio[0] = assi.colonne.start(k);
                }
            }
            if fuori_zona && spazio[1] <= assi.colonne.start(c0) {
                continue;
            }
            let f = &stile.font;
            fuori.push(Casella {
                dove: area.start,
                rett,
                spazio,
                testo,
                numero,
                sfondo,
                colore: colore_formato.or_else(|| f.color.resolve(&wb.theme)).unwrap_or([0, 0, 0]),
                carattere: f.name.clone(),
                misura: if f.size > 0.0 { f.size } else { 11.0 },
                grassetto: f.bold,
                corsivo: f.italic,
                sottolineato: f.underline != Underline::None,
                barrato: f.strike,
                orizzontale,
                verticale,
                a_capo: stile.align.wrap,
                rientro: stile.align.indent,
                bordi,
            });
        }
    }
    fuori
}

/// Il nome di una colonna: A, B, …, Z, AA.
pub fn nome_colonna(c: u32) -> String {
    gridcraft_core::col_to_letters(c)
}

/// Quello che si legge toccando una cella: dove sta (B7), il testo intero come si vede e, se c'è,
/// la formula.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dettaglio {
    pub dove: String,
    pub testo: String,
    pub formula: Option<String>,
}

/// Il dettaglio della cella `c` (della sua unione, se è unita).
pub fn dettaglio(wb: &Workbook, sh: &Sheet, c: CellRef) -> Dettaglio {
    let c = sh.merge_at(c).map_or(c, |m| m.start);
    let stile = wb.styles.get(sh.style_id(c));
    let (testo, _, _) = crate::italiano::testo(&accorciato(sh.value(c), TESTO_IN_DETTAGLIO), stile.num_fmt.as_str(), wb.date_system, 30);
    let formula = sh.cell(c).and_then(|cella| cella.formula.as_ref().map(|f| f.text.trim_start_matches('=')));
    let formula = formula.map(|f| format!("={}", accorcia(f, TESTO_IN_DETTAGLIO).as_deref().unwrap_or(f)));
    Dettaglio { dove: c.a1(), testo, formula }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn le_celle_di_una_zona_col_loro_aspetto() {
        let c = crate::Cartella::apri(&crate::prova::cartella_di_prova()).unwrap();
        let wb = &c.cartella;
        let sh = wb.sheet(c.schede[0]).unwrap();
        let assi = Assi::di(sh);
        let celle = caselle(wb, sh, &assi, (0, 10), (0, 5));
        let cella = |a: &str| celle.iter().find(|k| k.dove.a1() == a).unwrap_or_else(|| panic!("{a} manca"));
        // L'intestazione unita su tre colonne, in grassetto, con lo sfondo.
        let titolo = cella("A1");
        assert_eq!(titolo.testo, "Reparto di medicina");
        assert!(titolo.grassetto && titolo.sfondo.is_some());
        assert_eq!(titolo.rett[2], assi.colonne.start(3));
        assert!(!celle.iter().any(|k| k.dove.a1() == "B1"), "una cella unita si disegna una volta");
        // Una data di sistema all'italiana, a destra; un numero con la virgola.
        assert_eq!(cella("B3").testo, "03/11/2026");
        assert_eq!(cella("B3").orizzontale, Allinea::Fine);
        assert_eq!(cella("C3").testo, "37,5");
        // Il testo lungo sborda nelle vuote accanto.
        let nota = cella("A6");
        assert!(nota.spazio[1] > nota.rett[2]);
        // I bordi.
        assert!(cella("A2").bordi[3].is_some());
        // Il dettaglio di una formula.
        let d = dettaglio(wb, sh, CellRef::new(4, 2));
        assert_eq!((d.dove.as_str(), d.formula.as_deref()), ("C5", Some("=SUM(C3:C4)")));
        assert_eq!(nome_colonna(27), "AB");
    }

    #[test]
    fn le_righe_nascoste_si_saltano_e_i_giri_hanno_un_tetto() {
        use gridcraft_model::LineInfo;
        let nascosta = LineInfo { hidden: true, ..Default::default() };
        // Le righe da 5 a 199.999 nascoste, come un gruppo chiuso: dopo la 4 viene la 200.000.
        let mut sh = Sheet::new("Prova");
        for r in 5..200_000 {
            sh.rows.insert(r, nascosta);
        }
        sh.set_value(CellRef::new(200_000, 0), Value::Text("dopo il gruppo".into()));
        let assi = Assi::di(&sh);
        let righe = assi.righe_visibili(0, MAX_ROWS - 1);
        assert_eq!(righe.len(), RIGHE_DISEGNATE);
        assert_eq!(&righe[..6], &[0, 1, 2, 3, 4, 200_000]);
        let wb = Workbook::new();
        let celle = caselle(&wb, &sh, &assi, (0, MAX_ROWS - 1), (0, MAX_COLS - 1));
        assert!(celle.iter().any(|k| k.testo == "dopo il gruppo"));
        // Le colonne, al più COLONNE_DISEGNATE anche se il foglio ne mostra 16.384.
        assert_eq!(assi.colonne_visibili(0, MAX_COLS - 1).len(), COLONNE_DISEGNATE);
        // Un file con la misura standard zero: quasi tutto largo zero, i giri finiscono presto.
        let mut zero = Sheet::new("Zero");
        zero.default_row_height = 0.0;
        zero.rows.insert(3, LineInfo { size: Some(20.0), ..Default::default() });
        let assi = Assi::di(&zero);
        let inizio = std::time::Instant::now();
        assert_eq!(assi.righe_visibili(0, MAX_ROWS - 1), vec![3]);
        assert!(inizio.elapsed() < std::time::Duration::from_millis(50));
    }

    #[test]
    fn un_testo_enorme_si_accorcia() {
        let mut sh = Sheet::new("Prova");
        sh.set_value(CellRef::new(0, 0), Value::Text("è".repeat(100_000).into()));
        let wb = Workbook::new();
        let assi = Assi::di(&sh);
        let celle = caselle(&wb, &sh, &assi, (0, 5), (0, 5));
        assert_eq!(celle[0].testo.chars().count(), TESTO_IN_CELLA + 1);
        assert!(celle[0].testo.ends_with('…'));
        let d = dettaglio(&wb, &sh, CellRef::new(0, 0));
        assert_eq!(d.testo.chars().count(), TESTO_IN_DETTAGLIO + 1);
        // Un testo corto resta com'è.
        assert_eq!(accorcia("Rossi", 3), Some("Ros…".into()));
        assert_eq!(accorcia("Ros", 3), None);
        assert_eq!(accorcia("èèè", 3), None);
    }
}
