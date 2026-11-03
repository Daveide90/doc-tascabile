//! I fogli di calcolo delle prove, scritti con GridCraft: dati finti, nessun dato vero.

use std::sync::Arc;

use gridcraft_core::{CellRef, RangeRef, Value};
use gridcraft_model::{BorderLine, Cell, Color, Fill, Formula, NumFmt, PatternType, Sheet, Style, Visibility, Workbook};

/// Un foglio di prova in xlsx: «Pazienti» con l'intestazione unita in grassetto su sfondo
/// azzurro, date di sistema, temperature, una somma, una nota lunga; «Turni»; un foglio nascosto.
pub fn cartella_di_prova() -> Vec<u8> {
    let mut wb = Workbook::new();
    wb.sheets.clear();
    let titolo = wb.styles.intern(Style {
        font: gridcraft_model::Font { bold: true, size: 14.0, ..Default::default() },
        fill: Fill { pattern: PatternType::Solid, fg: Color::Rgb(0xD6E4F0), bg: Color::Auto },
        ..Default::default()
    });
    let intestazione = wb.styles.intern(Style {
        font: gridcraft_model::Font { bold: true, ..Default::default() },
        borders: gridcraft_model::Borders { bottom: BorderLine::thin(), ..Default::default() },
        ..Default::default()
    });
    let data = wb.styles.intern(Style { num_fmt: NumFmt::new("m/d/yyyy"), ..Default::default() });
    let mut p = Sheet::new("Pazienti");
    let a = |s: &str| {
        let b = s.as_bytes();
        CellRef::new(s[1..].parse::<u32>().unwrap_or(1) - 1, u32::from(b[0] - b'A'))
    };
    p.set_value(a("A1"), Value::Text("Reparto di medicina".into()));
    p.set_style(a("A1"), titolo);
    p.merges.push(RangeRef::new(a("A1"), a("C1")));
    for (c, t) in [("A2", "Paziente"), ("B2", "Data"), ("C2", "Temperatura")] {
        p.set_value(a(c), Value::Text(t.into()));
        p.set_style(a(c), intestazione);
    }
    p.set_value(a("A3"), Value::Text("Rossi".into()));
    p.set_value(a("B3"), Value::Number(46329.0));
    p.set_style(a("B3"), data);
    p.set_value(a("C3"), Value::Number(37.5));
    p.set_value(a("A4"), Value::Text("Bianchi".into()));
    p.set_value(a("B4"), Value::Number(46330.0));
    p.set_style(a("B4"), data);
    p.set_value(a("C4"), Value::Number(36.8));
    p.set_cell(a("C5"), Cell { value: Value::Number(74.3), formula: Some(Arc::new(Formula::new("SUM(C3:C4)"))), ..Default::default() });
    p.set_value(a("A6"), Value::Text("Nota: controllare la pressione ogni due ore".into()));
    // La colonna delle date larga quanto serve, come la allarga Excel; la prima riga bloccata.
    p.cols.insert(1, gridcraft_model::LineInfo { size: Some(84.0), custom: true, ..Default::default() });
    p.freeze = Some((2, 0));
    let mut t = Sheet::new("Turni");
    t.set_value(a("A1"), Value::Text("Mattina".into()));
    t.set_value(a("B1"), Value::Bool(true));
    let mut n = Sheet::new("Nascosto");
    n.visibility = Visibility::Hidden;
    n.set_value(a("A1"), Value::Text("non si vede".into()));
    wb.sheets = vec![Arc::new(p), Arc::new(t), Arc::new(n)];
    gridcraft_xlsx::write_xlsx(&wb).unwrap_or_default()
}
