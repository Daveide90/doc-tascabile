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

/// Le misure di un foglio: dove comincia ogni riga e ogni colonna, in pixel a 96 dpi; le
/// nascoste larghe zero.
pub struct Assi {
    pub righe: LineIndex,
    pub colonne: LineIndex,
}

impl Assi {
    pub fn di(sh: &Sheet) -> Assi {
        Assi { righe: LineIndex::new(&sh.rows, sh.default_row_height, MAX_ROWS), colonne: LineIndex::new(&sh.cols, sh.default_col_width, MAX_COLS) }
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

fn bordo(b: &BorderLine, wb: &Workbook) -> Option<Bordo> {
    (!b.is_none()).then(|| Bordo { spessore: b.style.width(), colore: b.color.resolve(&wb.theme).unwrap_or([0, 0, 0]) })
}

/// Le celle da disegnare nelle righe `righe` e nelle colonne `colonne` (le une e le altre
/// comprese): solo quelle con qualcosa da vedere (testo, sfondo, bordi). Le unioni si danno una
/// volta, intere, se toccano la zona.
pub fn caselle(wb: &Workbook, sh: &Sheet, assi: &Assi, righe: (u32, u32), colonne: (u32, u32)) -> Vec<Casella> {
    let (r0, r1) = (righe.0, righe.1.min(MAX_ROWS - 1));
    let (c0, c1) = (colonne.0, colonne.1.min(MAX_COLS - 1));
    let zona = RangeRef::new(CellRef::new(r0, c0.saturating_sub(SBORDA)), CellRef::new(r1, (c1 + SBORDA).min(MAX_COLS - 1)));
    let unioni: Vec<RangeRef> = sh.merges.iter().filter(|m| m.intersects(&zona)).copied().collect();
    let unione_di = |c: CellRef| unioni.iter().find(|m| m.contains(c)).copied();
    let piena = |c: CellRef| !sh.value(c).is_empty() || unione_di(c).is_some_and(|m| m.start != c || m.end != c);
    let mut fuori = Vec::new();
    let mut fatte: Vec<RangeRef> = Vec::new();
    for r in r0..=r1 {
        if sh.row_height(r) <= 0.0 {
            continue;
        }
        // Anche le celle a sinistra fuori dalla zona il cui testo sborda dentro.
        for c in c0.saturating_sub(SBORDA)..=c1 {
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
            let valore = sh.value(area.start);
            let sfondo = match stile.fill.pattern {
                PatternType::None => None,
                PatternType::Solid => stile.fill.fg.resolve(&wb.theme),
                _ => stile.fill.bg.resolve(&wb.theme).or_else(|| stile.fill.fg.resolve(&wb.theme)),
            };
            let b = &stile.borders;
            let bordi = [bordo(&b.left, wb), bordo(&b.top, wb), bordo(&b.right, wb), bordo(&b.bottom, wb)];
            let fuori_zona = c < c0;
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
    let (testo, _, _) = crate::italiano::testo(&sh.value(c), stile.num_fmt.as_str(), wb.date_system, 30);
    let formula = sh.cell(c).and_then(|cella| cella.formula.as_ref().map(|f| format!("={}", f.text.trim_start_matches('='))));
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
}
