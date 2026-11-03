//! Il foglio fotografato steso dritto anche quando i suoi lati sono curvi, e il cancellino
//! (segnalazioni dell'autore del 3/11: «la possibilità di correggere le distorsioni da lente o
//! posizionamento del telefono rispetto al foglio o foglio leggermente piegato» e «il cancellino
//! per sbiancare manualmente eventuali artefatti»).
//!
//! La prospettiva del telefono storto la raddrizzano i quattro angoli (un'omografia). Un foglio
//! piegato o la curvatura dell'obiettivo fanno i lati curvi: ognuno passa per un punto in mezzo,
//! e la pagina dritta si stende fra le quattro curve (una superficie di Coons) sopra la
//! prospettiva dei quattro angoli. Con i punti in mezzo ai lati dritti è la prospettiva e basta.

use image::RgbImage;

/// Un punto (x, y).
type Punto = [f64; 2];

/// Il lato della pagina stesa più lungo di così (pixel) si rimpicciolisce: come la foto.
const LATO_MASSIMO: f64 = 6000.0;

/// L'omografia dal quadrato unitario ai quattro punti `a` (in senso orario dall'alto a sinistra),
/// come matrice 3 × 3 per righe. None se i punti sono in fila.
fn omografia(a: [Punto; 4]) -> Option<[f64; 9]> {
    let da = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    // Otto equazioni in h0..h7 (h8 = 1).
    let mut m = [[0.0f64; 9]; 8];
    for i in 0..4 {
        let ([u, v], [x, y]) = (da[i], a[i]);
        m[2 * i] = [u, v, 1.0, 0.0, 0.0, 0.0, -u * x, -v * x, x];
        m[2 * i + 1] = [0.0, 0.0, 0.0, u, v, 1.0, -u * y, -v * y, y];
    }
    for c in 0..8 {
        let perno = (c..8).max_by(|r, s| m[*r][c].abs().total_cmp(&m[*s][c].abs()))?;
        if m[perno][c].abs() < 1e-12 {
            return None;
        }
        m.swap(c, perno);
        let perno = m[c];
        for (r, riga) in m.iter_mut().enumerate() {
            if r != c {
                let f = riga[c] / perno[c];
                for (v, p) in riga.iter_mut().zip(perno).skip(c) {
                    *v -= f * p;
                }
            }
        }
    }
    let h: Vec<f64> = (0..8).map(|r| m[r][8] / m[r][r]).collect();
    Some([h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7], 1.0])
}

fn applica(h: &[f64; 9], p: Punto) -> Option<Punto> {
    let w = h[6] * p[0] + h[7] * p[1] + h[8];
    (w.abs() > 1e-12).then(|| [(h[0] * p[0] + h[1] * p[1] + h[2]) / w, (h[3] * p[0] + h[4] * p[1] + h[5]) / w])
}

fn inversa(h: &[f64; 9]) -> Option<[f64; 9]> {
    let [a, b, c, d, e, f, g, k, l] = *h;
    let det = a * (e * l - f * k) - b * (d * l - f * g) + c * (d * k - e * g);
    if det.abs() < 1e-15 {
        return None;
    }
    Some([e * l - f * k, c * k - b * l, b * f - c * e, f * g - d * l, a * l - c * g, c * d - a * f, d * k - e * g, b * g - a * k, a * e - b * d].map(|v| v / det))
}

/// La lunghezza del lato curvo da `a` a `b` che passa per `m` (una curva di secondo grado).
fn lunghezza(a: Punto, m: Punto, b: Punto) -> f64 {
    let c = [2.0 * m[0] - (a[0] + b[0]) / 2.0, 2.0 * m[1] - (a[1] + b[1]) / 2.0];
    let punto = |t: f64| {
        let u = 1.0 - t;
        [u * u * a[0] + 2.0 * u * t * c[0] + t * t * b[0], u * u * a[1] + 2.0 * u * t * c[1] + t * t * b[1]]
    };
    (0..16).map(|i| {
        let (p, q) = (punto(i as f64 / 16.0), punto((i + 1) as f64 / 16.0));
        (q[0] - p[0]).hypot(q[1] - p[1])
    })
    .sum()
}

/// Quanto sono lunghi nella foto il più lungo dei lati alto e basso e il più lungo dei lati di
/// fianco, curvi se passano dai punti `lati`.
pub(crate) fn lunghezze(angoli: [[f32; 2]; 4], lati: [[f32; 2]; 4]) -> (f64, f64) {
    let a = angoli.map(|p| [f64::from(p[0]), f64::from(p[1])]);
    let m = lati.map(|p| [f64::from(p[0]), f64::from(p[1])]);
    let [tl, tr, br, bl] = a;
    (lunghezza(tl, m[0], tr).max(lunghezza(bl, m[2], br)), lunghezza(tl, m[3], bl).max(lunghezza(tr, m[1], br)))
}

/// La distanza focale tipica di un telefono, in lati lunghi della foto (un grandangolo da 26 mm
/// equivalenti su un sensore 4:3): quella da usare quando la foto non la dice e la prospettiva del
/// foglio non basta a misurarla.
const FOCALE_TIPICA: f64 = 0.75;

/// Il rapporto vero fra larghezza e altezza del foglio fotografato coi quattro `angoli` (in senso
/// orario dall'alto a sinistra), in una foto larga `w` e alta `h` con il centro dell'obiettivo nel
/// mezzo (segnalazione dell'autore del 3/11: col telefono inclinato la pagina usciva schiacciata,
/// quasi quadrata). I lati del foglio e le loro fughe danno due direzioni nello spazio, ad angolo
/// retto; tolta la distanza focale, il rapporto fra le loro lunghezze è quello del foglio (Zhang e
/// He, «Whiteboard scanning and image enhancement», 2007). La distanza focale (pixel) è `focale`,
/// quella scritta dal telefono nella foto; senza, si ricava dalle due fughe quando ci sono tutte e
/// due; con una sola (il telefono inclinato solo all'indietro) o nessuna (il foglio di fronte) si
/// usa quella tipica di un telefono, che di fronte non conta.
pub(crate) fn proporzione_vera(angoli: [[f32; 2]; 4], w: f64, h: f64, focale: Option<f64>) -> Option<f64> {
    let p = |q: [f32; 2]| [f64::from(q[0]), f64::from(q[1]), 1.0];
    // m1 in alto a sinistra, m2 in alto a destra, m3 in basso a sinistra, m4 in basso a destra.
    let (m1, m2, m3, m4) = (p(angoli[0]), p(angoli[1]), p(angoli[3]), p(angoli[2]));
    let x = |a: [f64; 3], b: [f64; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let s = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let (d2, d3) = (s(x(m2, m4), m3), s(x(m3, m4), m2));
    if d2.abs() < 1e-9 || d3.abs() < 1e-9 {
        return None;
    }
    let (k2, k3) = (s(x(m1, m4), m3) / d2, s(x(m1, m4), m2) / d3);
    let n2 = [k2 * m2[0] - m1[0], k2 * m2[1] - m1[1], k2 - 1.0];
    let n3 = [k3 * m3[0] - m1[0], k3 * m3[1] - m1[1], k3 - 1.0];
    let lato = w.max(h).max(1.0);
    let (u0, v0) = (w / 2.0, h / 2.0);
    // Con due fughe vere la distanza focale si misura; una misura strana non vale.
    let fughe = (n2[2] * lato).abs() > 1e-3 * n2[0].hypot(n2[1]) && (n3[2] * lato).abs() > 1e-3 * n3[0].hypot(n3[1]);
    let misurata = fughe
        .then(|| {
            -((n2[0] * n3[0] - (n2[0] * n3[2] + n2[2] * n3[0]) * u0 + n2[2] * n3[2] * u0 * u0)
                + (n2[1] * n3[1] - (n2[1] * n3[2] + n2[2] * n3[1]) * v0 + n2[2] * n3[2] * v0 * v0))
                / (n2[2] * n3[2])
        })
        .filter(|f2| f2.is_finite() && *f2 > (0.3 * lato).powi(2) && *f2 < (5.0 * lato).powi(2))
        .map(f64::sqrt);
    let f = focale.filter(|f| f.is_finite() && *f > 0.0).or(misurata).unwrap_or(FOCALE_TIPICA * lato);
    let q = |n: [f64; 3]| ((n[0] - u0 * n[2]) / f).powi(2) + ((n[1] - v0 * n[2]) / f).powi(2) + n[2] * n[2];
    let r = (q(n2) / q(n3)).sqrt();
    (r.is_finite() && r > 0.0).then_some(r)
}

/// La pagina dritta dalla foto `foto`, fra i quattro `angoli` (in senso orario dall'alto a
/// sinistra) e coi lati che passano per i punti `lati` (in mezzo ai lati alto, destro, basso,
/// sinistro): i pixel RGB, la larghezza e l'altezza. Grande `misura`, o, senza, larga quanto il
/// più lungo dei lati alto e basso e alta quanto il più lungo dei lati di fianco; fuori dalla
/// foto bianca. None se gli angoli non fanno un foglio.
pub(crate) fn stendi(foto: &RgbImage, angoli: [[f32; 2]; 4], lati: [[f32; 2]; 4], misura: Option<(u32, u32)>) -> Option<(Vec<u8>, u32, u32)> {
    let a = angoli.map(|p| [f64::from(p[0]), f64::from(p[1])]);
    let m = lati.map(|p| [f64::from(p[0]), f64::from(p[1])]);
    let (mut ow, mut oh) = match misura {
        Some((w, h)) => (f64::from(w), f64::from(h)),
        None => lunghezze(angoli, lati),
    };
    let k = (LATO_MASSIMO / ow.max(oh).max(1.0)).min(1.0);
    (ow, oh) = ((ow * k).floor().max(1.0), (oh * k).floor().max(1.0));
    if !(ow.is_finite() && oh.is_finite()) {
        return None;
    }
    let h = omografia(a)?;
    let hi = inversa(&h)?;
    // Di quanto i punti in mezzo ai lati si scostano, nel quadrato raddrizzato, dal mezzo del lato dritto.
    let scarto = |p: Punto, dritto: Punto| applica(&hi, p).map(|q| [q[0] - dritto[0], q[1] - dritto[1]]);
    let su = scarto(m[0], [0.5, 0.0])?;
    let destra = scarto(m[1], [1.0, 0.5])?;
    let giu = scarto(m[2], [0.5, 1.0])?;
    let sinistra = scarto(m[3], [0.0, 0.5])?;
    let (w, hh) = (foto.width() as usize, foto.height() as usize);
    let raw = foto.as_raw();
    let (ow, oh) = (ow as usize, oh as usize);
    let mut out = vec![255u8; ow * oh * 3];
    for y in 0..oh {
        let v = (y as f64 + 0.5) / oh as f64;
        for x in 0..ow {
            let u = (x as f64 + 0.5) / ow as f64;
            let (bu, bv) = (4.0 * u * (1.0 - u), 4.0 * v * (1.0 - v));
            let du = (1.0 - v) * bu * su[0] + v * bu * giu[0] + (1.0 - u) * bv * sinistra[0] + u * bv * destra[0];
            let dv = (1.0 - v) * bu * su[1] + v * bu * giu[1] + (1.0 - u) * bv * sinistra[1] + u * bv * destra[1];
            let Some([sx, sy]) = applica(&h, [u + du, v + dv]) else { continue };
            let (fx, fy) = (sx - 0.5, sy - 0.5);
            if !(fx > -1.0 && fy > -1.0 && fx < w as f64 && fy < hh as f64) {
                continue;
            }
            let (x0, y0) = (fx.floor(), fy.floor());
            let (ax, ay) = (fx - x0, fy - y0);
            let dentro = |v: f64, n: usize| v.clamp(0.0, (n - 1) as f64) as usize;
            let (xa, xb, ya, yb) = (dentro(x0, w), dentro(x0 + 1.0, w), dentro(y0, hh), dentro(y0 + 1.0, hh));
            for c in 0..3 {
                let p = |x: usize, y: usize| f64::from(raw[(y * w + x) * 3 + c]);
                let val = (1.0 - ay) * ((1.0 - ax) * p(xa, ya) + ax * p(xb, ya)) + ay * ((1.0 - ax) * p(xa, yb) + ax * p(xb, yb));
                out[(y * ow + x) * 3 + c] = val.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    Some((out, ow as u32, oh as u32))
}

/// Il punto in mezzo al lato dritto da `a` a `b`: un lato senza curva.
pub(crate) fn mezzo(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]
}

/// I punti in mezzo ai quattro lati dritti degli `angoli`.
pub(crate) fn lati_dritti(angoli: [[f32; 2]; 4]) -> [[f32; 2]; 4] {
    [mezzo(angoli[0], angoli[1]), mezzo(angoli[1], angoli[2]), mezzo(angoli[2], angoli[3]), mezzo(angoli[3], angoli[0])]
}

/// Le passate del cancellino sui pixel `pixel` (`canali` per pixel, `w` × `h`): ogni passata è
/// una fila di punti (0..1 della larghezza e dell'altezza) col suo raggio (frazione della
/// larghezza), e dove passa la pagina prende il colore della carta: la media dei pixel più chiari.
pub(crate) fn cancella(pixel: &mut [u8], canali: usize, w: u32, h: u32, passate: &[(Vec<[f32; 2]>, f32)]) {
    let (w, h) = (w as usize, h as usize);
    if passate.is_empty() || w == 0 || h == 0 || pixel.len() != w * h * canali {
        return;
    }
    // La carta: la media dei pixel fra il 90° e il 98° percentile della luce, presi a passi.
    let luce = |i: usize| -> u32 {
        let p = &pixel[i * canali..i * canali + canali];
        if canali >= 3 { (299 * u32::from(p[0]) + 587 * u32::from(p[1]) + 114 * u32::from(p[2])) / 1000 } else { u32::from(p[0]) }
    };
    let passo = ((w * h) / 20_000).max(1);
    let mut campioni: Vec<usize> = (0..w * h).step_by(passo).collect();
    campioni.sort_by_key(|i| luce(*i));
    let n = campioni.len();
    let scelti = &campioni[n * 9 / 10..(n * 98 / 100).max(n * 9 / 10 + 1).min(n)];
    let mut carta = vec![0u64; canali];
    for i in scelti {
        for (c, s) in carta.iter_mut().enumerate() {
            *s += u64::from(pixel[i * canali + c]);
        }
    }
    let carta: Vec<u8> = carta.iter().map(|s| (s / scelti.len().max(1) as u64) as u8).collect();
    for (punti, raggio) in passate {
        let r = f64::from(*raggio) * w as f64;
        if punti.is_empty() || !(r.is_finite() && r > 0.0) {
            continue;
        }
        let in_pixel: Vec<[f64; 2]> = punti.iter().map(|p| [f64::from(p[0]) * w as f64, f64::from(p[1]) * h as f64]).collect();
        // Ogni tratto fra due punti è una striscia con le punte tonde.
        let tratti: Vec<([f64; 2], [f64; 2])> = if in_pixel.len() == 1 { vec![(in_pixel[0], in_pixel[0])] } else { in_pixel.windows(2).map(|t| (t[0], t[1])).collect() };
        for (a, b) in tratti {
            let (x0, x1) = ((a[0].min(b[0]) - r).floor().max(0.0) as usize, ((a[0].max(b[0]) + r).ceil() as usize).min(w));
            let (y0, y1) = ((a[1].min(b[1]) - r).floor().max(0.0) as usize, ((a[1].max(b[1]) + r).ceil() as usize).min(h));
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let l2 = dx * dx + dy * dy;
            for y in y0..y1 {
                for x in x0..x1 {
                    let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
                    let t = if l2 > 0.0 { (((px - a[0]) * dx + (py - a[1]) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
                    let (qx, qy) = (a[0] + t * dx, a[1] + t * dy);
                    if (px - qx).hypot(py - qy) <= r {
                        pixel[(y * w + x) * canali..(y * w + x) * canali + canali].copy_from_slice(&carta);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn l_omografia_porta_il_quadrato_sugli_angoli_e_torna() {
        let a = [[100.0, 80.0], [900.0, 120.0], [950.0, 1200.0], [60.0, 1100.0]];
        let h = omografia(a).unwrap();
        for (q, p) in [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]].iter().zip(a) {
            let r = applica(&h, *q).unwrap();
            assert!((r[0] - p[0]).abs() < 1e-6 && (r[1] - p[1]).abs() < 1e-6, "{r:?} {p:?}");
        }
        let hi = inversa(&h).unwrap();
        let r = applica(&hi, applica(&h, [0.3, 0.7]).unwrap()).unwrap();
        assert!((r[0] - 0.3).abs() < 1e-9 && (r[1] - 0.7).abs() < 1e-9);
        assert_eq!(omografia([[0.0, 0.0], [1.0, 1.0], [2.0, 2.0], [3.0, 3.0]]), None);
    }

    /// Un foglio a quadretti (righe ogni 50 pixel) piegato: i lati alto e basso curvi in giù di
    /// 40 pixel nel mezzo, come una pagina che si incurva. Steso coi punti in mezzo ai lati
    /// giusti, le righe tornano dritte; coi lati dritti restano curve.
    #[test]
    fn un_foglio_piegato_si_stende_dritto() {
        let (w, h) = (900u32, 1100u32);
        let angoli = [[100.0f32, 100.0], [800.0, 100.0], [800.0, 1000.0], [100.0, 1000.0]];
        let curva = 40.0f32;
        // La foto: il punto (u, v) della pagina va in (x, y) con la piega in giù.
        let mut foto = RgbImage::from_pixel(w, h, image::Rgb([30, 30, 30]));
        for y in 0..h {
            for x in 0..w {
                let u = (x as f32 - 100.0) / 700.0;
                if !(0.0..=1.0).contains(&u) {
                    continue;
                }
                let piega = curva * 4.0 * u * (1.0 - u);
                let v = (y as f32 - 100.0 - piega) / 900.0;
                if !(0.0..=1.0).contains(&v) {
                    continue;
                }
                let riga = ((v * 900.0) as i32 % 50) < 3;
                foto.put_pixel(x, y, image::Rgb(if riga { [0, 0, 0] } else { [250, 250, 250] }));
            }
        }
        let lati = [[450.0, 100.0 + curva], [800.0, 550.0], [450.0, 1000.0 + curva], [100.0, 550.0]];
        let (px, ow, _) = stendi(&foto, angoli, lati, None).unwrap();
        let scura = |x: usize, y: usize| px[(y * ow as usize + x) * 3] < 100;
        // Una riga nel mezzo della pagina: alla stessa altezza a sinistra, nel mezzo e a destra.
        let altezza_della_riga = |x: usize| (400..500).find(|y| scura(x, *y));
        let (a, b, c) = (altezza_della_riga(40), altezza_della_riga(ow as usize / 2), altezza_della_riga(ow as usize - 40));
        let (a, b, c) = (a.unwrap(), b.unwrap(), c.unwrap());
        assert!(a.abs_diff(b) <= 2 && b.abs_diff(c) <= 2, "la riga non è dritta: {a} {b} {c}");
        // Coi lati dritti la stessa riga è più bassa nel mezzo.
        let (px, ow, _) = stendi(&foto, angoli, lati_dritti(angoli), None).unwrap();
        let scura = |x: usize, y: usize| px[(y * ow as usize + x) * 3] < 100;
        let a = (400..500).find(|y| scura(40, *y)).unwrap();
        let b = (400..520).find(|y| scura(ow as usize / 2, *y)).unwrap();
        assert!(b > a + 20, "coi lati dritti la piega resta: {a} {b}");
    }

    /// I quattro angoli di un foglio `w` × `h` fotografato inclinato di `beccheggio` gradi in avanti
    /// e girato di `imbardata` gradi, con la distanza focale `f`, in una foto 4000 × 3000.
    fn fotografato(w: f64, h: f64, beccheggio: f64, imbardata: f64, f: f64) -> [[f32; 2]; 4] {
        let (a, b) = (beccheggio.to_radians(), imbardata.to_radians());
        let distanza = 1.3 * w.max(h);
        [(-w / 2.0, -h / 2.0), (w / 2.0, -h / 2.0), (w / 2.0, h / 2.0), (-w / 2.0, h / 2.0)].map(|(x, y)| {
            // Prima il beccheggio (attorno all'asse x), poi l'imbardata (attorno all'asse y).
            let (y1, z1) = (y * a.cos(), y * a.sin());
            let (x2, z2) = (x * b.cos() + z1 * b.sin(), -x * b.sin() + z1 * b.cos() + distanza);
            [(2000.0 + f * x2 / z2) as f32, (1500.0 + f * y1 / z2) as f32]
        })
    }

    #[test]
    fn le_proporzioni_vere_di_un_foglio_fotografato_storto() {
        let a4 = 210.0 / 297.0;
        let vicino = |r: f64, atteso: f64, quanto: f64| (r - atteso).abs() / atteso < quanto;
        // Di fronte: il rapporto dei lati.
        let r = proporzione_vera(fotografato(210.0, 297.0, 0.0, 0.0, 3200.0), 4000.0, 3000.0, None).unwrap();
        assert!(vicino(r, a4, 0.005), "{r}");
        // Inclinato di 55 gradi e girato di 15: le due fughe misurano la distanza focale.
        let r = proporzione_vera(fotografato(210.0, 297.0, 55.0, 15.0, 3000.0), 4000.0, 3000.0, None).unwrap();
        assert!(vicino(r, a4, 0.02), "{r}");
        // Solo inclinato all'indietro, come la foto del registro del 3/11: una fuga sola, la
        // distanza focale tipica (qui quella vera è un poco diversa).
        for f in [2880.0, 3000.0, 3120.0] {
            let angoli = fotografato(210.0, 297.0, 60.0, 0.0, f);
            let r = proporzione_vera(angoli, 4000.0, 3000.0, None).unwrap();
            assert!(vicino(r, a4, 0.06), "f {f}: {r}");
            // Con la distanza focale scritta nella foto, quella giusta.
            let r = proporzione_vera(angoli, 4000.0, 3000.0, Some(f)).unwrap();
            assert!(vicino(r, a4, 0.005), "f {f} dalla foto: {r}");
            // I lati come si vedono nella foto davano un foglio quasi quadrato.
            let (largo, alto) = lunghezze(angoli, lati_dritti(angoli));
            assert!(largo / alto > 1.0, "{largo} {alto}");
        }
        // Un biglietto da visita orizzontale.
        let r = proporzione_vera(fotografato(85.0, 55.0, 40.0, -10.0, 3200.0), 4000.0, 3000.0, None).unwrap();
        assert!(vicino(r, 85.0 / 55.0, 0.03), "{r}");
    }

    #[test]
    fn il_cancellino_sbianca_dove_passa() {
        // Una pagina bianca con una macchia nera nel mezzo e una in alto a sinistra.
        let (w, h) = (200u32, 300u32);
        let mut pixel = vec![250u8; (w * h * 3) as usize];
        for y in 140..160 {
            for x in 90..110 {
                pixel[((y * w + x) * 3) as usize..((y * w + x) * 3 + 3) as usize].copy_from_slice(&[0, 0, 0]);
            }
        }
        for y in 10..20 {
            for x in 10..20 {
                pixel[((y * w + x) * 3) as usize..((y * w + x) * 3 + 3) as usize].copy_from_slice(&[0, 0, 0]);
            }
        }
        // Una passata in orizzontale sulla macchia di mezzo, larga 15 pixel per parte.
        cancella(&mut pixel, 3, w, h, &[(vec![[0.4, 0.5], [0.6, 0.5]], 0.075)]);
        let a = |x: u32, y: u32| pixel[((y * w + x) * 3) as usize];
        assert_eq!(a(100, 150), 250);
        assert_eq!(a(90, 141), 250);
        assert_eq!(a(15, 15), 0, "la macchia lontana resta");
        // In grigi, un punto solo.
        let mut grigi = vec![255u8; 100];
        grigi[55] = 0;
        cancella(&mut grigi, 1, 10, 10, &[(vec![[0.55, 0.55]], 0.1)]);
        assert_eq!(grigi[55], 255);
    }
}
