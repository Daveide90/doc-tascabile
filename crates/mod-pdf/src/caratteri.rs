//! I caratteri per riscrivere le parole di una scansione (seconda segnalazione dell'autore del 3/11:
//! «se modifico un file scansionato cambia il font ed è diverso dal resto»). Le parole corrette si
//! disegnano dentro la foto della pagina, con il carattere che somiglia di più a quello stampato:
//! si sceglie fra quelli che l'APK ha già per le presentazioni, che hanno le misure dei caratteri
//! di Office e dei PDF più comuni. Carlito come Calibri, Liberation Sans come Arial e Helvetica,
//! Liberation Serif come Times, Caladea come Cambria; ognuno tondo, grassetto e corsivo.
//!
//! Qui la forma delle lettere (contorni e composizione di deckcraft-fonts) e il loro disegno a
//! pixel, con i bordi sfumati come quelli di una foto: la copertura di ogni pixel, fra 0 e 1.

use std::sync::{Arc, OnceLock};

use deckcraft_fonts::{FontDb, FontFace, ShapeParams};
use kurbo::PathEl;

/// Le famiglie fra cui si cerca quella della scansione.
const FAMIGLIE: [&str; 4] = ["Carlito", "Liberation Sans", "Liberation Serif", "Caladea"];
/// Gli stili di ogni famiglia: nome, grassetto, corsivo.
const STILI: [(&str, bool, bool); 4] = [("Regular", false, false), ("Bold", true, false), ("Italic", false, true), ("Bold Italic", true, true)];
/// Margine intorno al disegno di una parola, in pixel: i bordi sfumati non escono dalla tela.
const MARGINE: f64 = 5.0;

/// Un carattere fra cui scegliere.
pub(crate) struct Carattere {
    pub(crate) famiglia: &'static str,
    pub(crate) grassetto: bool,
    pub(crate) corsivo: bool,
    faccia: Arc<FontFace>,
}

impl std::fmt::Debug for Carattere {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}{}", self.famiglia, if self.grassetto { " grassetto" } else { "" }, if self.corsivo { " corsivo" } else { "" })
    }
}

/// I caratteri incorporati, senza quelli del sistema: sul PC delle prove e sul telefono si
/// sceglie fra gli stessi.
fn archivio() -> &'static FontDb {
    static ARCHIVIO: OnceLock<FontDb> = OnceLock::new();
    ARCHIVIO.get_or_init(|| {
        let db = FontDb::with_font_dirs(Vec::new());
        db.set_system_fallback(false);
        db
    })
}

/// I caratteri fra cui scegliere, nell'ordine di [`FAMIGLIE`] e [`STILI`]. Uno che manca (una
/// build senza i caratteri delle presentazioni) non c'è: la lista può essere vuota.
pub(crate) fn caratteri() -> &'static [Carattere] {
    static CARATTERI: OnceLock<Vec<Carattere>> = OnceLock::new();
    CARATTERI.get_or_init(|| {
        let db = archivio();
        let mut v = Vec::new();
        for famiglia in FAMIGLIE {
            for (stile, grassetto, corsivo) in STILI {
                let faccia = db.face(famiglia, stile);
                // Lo stile più vicino, se quello chiesto manca: va bene solo se è proprio lui.
                if faccia.family == famiglia && faccia.italic == corsivo && (faccia.weight >= 600.0) == grassetto {
                    v.push(Carattere { famiglia, grassetto, corsivo, faccia });
                }
            }
        }
        v
    })
}

/// Una parola disegnata: la copertura di ogni pixel (0 la carta, 1 l'inchiostro pieno), riga per
/// riga, e dove stanno la penna e la linea di base.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Disegno {
    pub(crate) largo: usize,
    pub(crate) alto: usize,
    pub(crate) copertura: Vec<f32>,
    /// Il punto da cui parte la penna sulla linea di base, in pixel del disegno.
    pub(crate) penna: [f64; 2],
    /// Quanto avanza la penna dopo la parola, in pixel.
    pub(crate) avanza: f64,
    /// Il riquadro dell'inchiostro [x0, y0, x1, y1] (x1 e y1 esclusi): i pixel coperti almeno a
    /// metà. None se la parola non ha inchiostro (spazi).
    pub(crate) inchiostro: Option<[usize; 4]>,
}

impl Disegno {
    pub(crate) fn a(&self, x: usize, y: usize) -> f32 {
        if x < self.largo && y < self.alto { self.copertura.get(y * self.largo + x).copied().unwrap_or(0.0) } else { 0.0 }
    }
}

impl Carattere {
    /// `testo` disegnato con un em di `em` pixel, stirato in orizzontale di `stira` (1 com'è).
    pub(crate) fn disegna(&self, testo: &str, em: f64, stira: f64) -> Disegno {
        let db = archivio();
        let f = &*self.faccia;
        let glifi = deckcraft_fonts::shape_range(f, testo, 0..testo.len(), &ShapeParams::default(), |c| c);
        let scala = em / f.upem.max(1.0);
        let mut segmenti: Vec<[f64; 4]> = Vec::new();
        let mut penna = 0.0;
        for g in &glifi {
            let (ox, oy) = (penna + f64::from(g.x_offset), -f64::from(g.y_offset));
            let punto = |p: kurbo::Point| [(ox + p.x) * scala * stira, (oy + p.y) * scala];
            appiattisci(db.outline(f, g.gid).elements(), punto, &mut segmenti);
            penna += f64::from(g.x_advance);
        }
        let avanza = penna * scala * stira;
        let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for s in &segmenti {
            x0 = x0.min(s[0]).min(s[2]);
            x1 = x1.max(s[0]).max(s[2]);
            y0 = y0.min(s[1]).min(s[3]);
            y1 = y1.max(s[1]).max(s[3]);
        }
        if segmenti.is_empty() || ![x0, y0, x1, y1].iter().all(|v| v.is_finite()) {
            return Disegno { largo: 0, alto: 0, copertura: Vec::new(), penna: [0.0, 0.0], avanza, inchiostro: None };
        }
        let (dx, dy) = (MARGINE - x0.floor(), MARGINE - y0.floor());
        let largo = (x1.ceil() - x0.floor() + 2.0 * MARGINE) as usize;
        let alto = (y1.ceil() - y0.floor() + 2.0 * MARGINE) as usize;
        let mut tela = Tela::new(largo, alto);
        for s in &segmenti {
            tela.linea([(s[0] + dx) as f32, (s[1] + dy) as f32], [(s[2] + dx) as f32, (s[3] + dy) as f32]);
        }
        let copertura = tela.copertura();
        let inchiostro = riquadro(&copertura, largo, alto);
        Disegno { largo, alto, copertura, penna: [dx, dy], avanza, inchiostro }
    }
}

/// Il riquadro dei pixel coperti almeno a metà.
fn riquadro(copertura: &[f32], largo: usize, alto: usize) -> Option<[usize; 4]> {
    let mut r: Option<[usize; 4]> = None;
    for y in 0..alto {
        for x in 0..largo {
            if copertura.get(y * largo + x).is_some_and(|c| *c >= 0.5) {
                r = Some(match r {
                    None => [x, y, x + 1, y + 1],
                    Some(r) => [r[0].min(x), r[1].min(y), r[2].max(x + 1), r[3].max(y + 1)],
                });
            }
        }
    }
    r
}

/// I contorni di una lettera come segmenti [x0, y0, x1, y1] in pixel: le curve divise in tratti
/// dritti abbastanza corti da non vedersi. `punto` porta un punto del contorno nei pixel.
fn appiattisci(elementi: &[PathEl], punto: impl Fn(kurbo::Point) -> [f64; 2], segmenti: &mut Vec<[f64; 4]>) {
    let mut inizio = [0.0, 0.0];
    let mut qui = [0.0, 0.0];
    let tratto = |a: [f64; 2], b: [f64; 2], segmenti: &mut Vec<[f64; 4]>| segmenti.push([a[0], a[1], b[0], b[1]]);
    // Quanti tratti per una curva: dalla sua curvatura, fra 1 e 32.
    let quanti = |d: f64| (d.sqrt() * 1.5).ceil().clamp(1.0, 32.0) as usize;
    for el in elementi {
        match *el {
            PathEl::MoveTo(p) => {
                if qui != inizio {
                    tratto(qui, inizio, segmenti);
                }
                inizio = punto(p);
                qui = inizio;
            }
            PathEl::LineTo(p) => {
                let p = punto(p);
                tratto(qui, p, segmenti);
                qui = p;
            }
            PathEl::QuadTo(c, p) => {
                let (c, p) = (punto(c), punto(p));
                let d = (qui[0] - 2.0 * c[0] + p[0]).hypot(qui[1] - 2.0 * c[1] + p[1]);
                let n = quanti(d);
                let a = qui;
                for i in 1..=n {
                    let t = i as f64 / n as f64;
                    let u = 1.0 - t;
                    let q = [u * u * a[0] + 2.0 * u * t * c[0] + t * t * p[0], u * u * a[1] + 2.0 * u * t * c[1] + t * t * p[1]];
                    tratto(qui, q, segmenti);
                    qui = q;
                }
            }
            PathEl::CurveTo(c1, c2, p) => {
                let (c1, c2, p) = (punto(c1), punto(c2), punto(p));
                let d1 = (qui[0] - 2.0 * c1[0] + c2[0]).hypot(qui[1] - 2.0 * c1[1] + c2[1]);
                let d2 = (c1[0] - 2.0 * c2[0] + p[0]).hypot(c1[1] - 2.0 * c2[1] + p[1]);
                let n = quanti(1.5 * d1.max(d2));
                let a = qui;
                for i in 1..=n {
                    let t = i as f64 / n as f64;
                    let u = 1.0 - t;
                    let (b0, b1, b2, b3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                    let q = [b0 * a[0] + b1 * c1[0] + b2 * c2[0] + b3 * p[0], b0 * a[1] + b1 * c1[1] + b2 * c2[1] + b3 * p[1]];
                    tratto(qui, q, segmenti);
                    qui = q;
                }
            }
            PathEl::ClosePath => {
                if qui != inizio {
                    tratto(qui, inizio, segmenti);
                }
                qui = inizio;
            }
        }
    }
    if qui != inizio {
        tratto(qui, inizio, segmenti);
    }
}

/// La tela su cui si disegnano i contorni: per ogni pixel l'area coperta, accumulata lungo la riga
/// (il metodo di font-rs). I contorni chiusi sommano a zero a fine riga.
struct Tela {
    largo: usize,
    alto: usize,
    a: Vec<f32>,
}

impl Tela {
    fn new(largo: usize, alto: usize) -> Tela {
        Tela { largo, alto, a: vec![0.0; largo * alto + 4] }
    }

    fn somma(&mut self, i: isize, v: f32) {
        if let Some(c) = usize::try_from(i).ok().and_then(|i| self.a.get_mut(i)) {
            *c += v;
        }
    }

    /// Un tratto del contorno, da `p0` a `p1` (pixel).
    fn linea(&mut self, p0: [f32; 2], p1: [f32; 2]) {
        if (p0[1] - p1[1]).abs() <= f32::EPSILON {
            return;
        }
        let (verso, p0, p1) = if p0[1] < p1[1] { (1.0, p0, p1) } else { (-1.0, p1, p0) };
        let dxdy = (p1[0] - p0[0]) / (p1[1] - p0[1]);
        let mut x = p0[0];
        if p0[1] < 0.0 {
            x -= p0[1] * dxdy;
        }
        let y_da = p0[1].max(0.0) as usize;
        let y_a = self.alto.min(p1[1].ceil().max(0.0) as usize);
        for y in y_da..y_a {
            let riga = (y * self.largo) as isize;
            let dy = ((y + 1) as f32).min(p1[1]) - (y as f32).max(p0[1]);
            let xdopo = x + dxdy * dy;
            let d = dy * verso;
            let (x0, x1) = if x < xdopo { (x, xdopo) } else { (xdopo, x) };
            let x0f = x0.floor();
            let x0i = x0f as isize;
            let x1c = x1.ceil();
            let x1i = x1c as isize;
            if x1i <= x0i + 1 {
                let xmf = 0.5 * (x + xdopo) - x0f;
                self.somma(riga + x0i, d - d * xmf);
                self.somma(riga + x0i + 1, d * xmf);
            } else {
                let s = (x1 - x0).recip();
                let x0r = x0 - x0f;
                let a0 = 0.5 * s * (1.0 - x0r) * (1.0 - x0r);
                let x1r = x1 - x1c + 1.0;
                let am = 0.5 * s * x1r * x1r;
                self.somma(riga + x0i, d * a0);
                if x1i == x0i + 2 {
                    self.somma(riga + x0i + 1, d * (1.0 - a0 - am));
                } else {
                    let a1 = s * (1.5 - x0r);
                    self.somma(riga + x0i + 1, d * (a1 - a0));
                    for xi in x0i + 2..x1i - 1 {
                        self.somma(riga + xi, d * s);
                    }
                    let a2 = a1 + (x1i - x0i - 3) as f32 * s;
                    self.somma(riga + x1i - 1, d * (1.0 - a2 - am));
                }
                self.somma(riga + x1i, d * am);
            }
            x = xdopo;
        }
    }

    fn copertura(self) -> Vec<f32> {
        let mut somma = 0.0;
        self.a.iter().take(self.largo * self.alto).map(|v| {
            somma += v;
            somma.abs().min(1.0)
        }).collect()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn cerca(famiglia: &str, grassetto: bool, corsivo: bool) -> &'static Carattere {
        caratteri().iter().find(|c| c.famiglia == famiglia && c.grassetto == grassetto && c.corsivo == corsivo).expect("il carattere c'è")
    }

    #[test]
    fn ci_sono_tutti_i_caratteri() {
        assert_eq!(caratteri().len(), 16, "{:?}", caratteri());
    }

    #[test]
    fn una_parola_si_disegna_alla_sua_misura() {
        let c = cerca("Liberation Sans", false, false);
        let d = c.disegna("Hi", 100.0, 1.0);
        let r = d.inchiostro.expect("ha inchiostro");
        // L'H di Arial: alta quanto le maiuscole (0,716 em), sopra la linea di base.
        let alto = (r[3] - r[1]) as f64;
        assert!((alto - 71.6).abs() <= 2.0, "{alto}");
        assert!((r[3] as f64 - d.penna[1]).abs() <= 1.0, "la base è sotto le lettere: {r:?} {:?}", d.penna);
        // Larga quanto le due lettere di Arial (H 0,722 em, i 0,222 em), meno i margini laterali.
        assert!((d.avanza - 94.4).abs() < 0.5, "{}", d.avanza);
        // Dentro l'H c'è inchiostro pieno, fra le aste no.
        let y = (r[1] + r[3]) / 2 + 10;
        assert!(d.a(r[0] + 3, y) > 0.99);
        assert!(d.a(r[0] + 30, y) < 0.01);
        // Stirata a metà, larga la metà.
        assert!((c.disegna("Hi", 100.0, 0.5).avanza - 47.2).abs() < 0.5);
        // Uno spazio non ha inchiostro.
        assert_eq!(c.disegna(" ", 100.0, 1.0).inchiostro, None);
    }

    #[test]
    fn il_grassetto_ha_piu_inchiostro() {
        let inchiostro = |c: &Carattere| c.disegna("lavoro", 60.0, 1.0).copertura.iter().map(|v| f64::from(*v)).sum::<f64>();
        assert!(inchiostro(cerca("Carlito", true, false)) > 1.3 * inchiostro(cerca("Carlito", false, false)));
        // Carlito (Calibri) è più stretto di Liberation Sans (Arial).
        let (calibri, arial) = (cerca("Carlito", false, false).disegna("lavoro", 60.0, 1.0).avanza, cerca("Liberation Sans", false, false).disegna("lavoro", 60.0, 1.0).avanza);
        assert!(calibri < 0.95 * arial, "{calibri} {arial}");
    }
}
