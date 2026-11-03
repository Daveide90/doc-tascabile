//! Il foglio nella foto di una scansione, trovato dalla sua luce (segnalazione dell'autore del 3/11:
//! un foglio bianco su un fondo scuro non si trovava da solo, e i bordi andavano messi a mano).
//!
//! Il riconoscitore del motore cerca le righe dritte dei bordi; qui invece si cerca la macchia
//! chiara più grande: la foto rimpicciolita, la soglia che separa meglio chiaro e scuro (Otsu), la
//! parte chiara più grande, il suo contorno convesso, i quattro angoli che ne tengono più area. Va
//! bene proprio quando il foglio è chiaro e il fondo scuro; con un fondo chiaro come il foglio non
//! trova niente e resta il riconoscitore del motore.

/// La foto si guarda col lato lungo al più di tanti pixel.
const LATO: usize = 400;
/// Il foglio è almeno l'8% della foto e non tutta (il 97%).
const AREA: (f32, f32) = (0.08, 0.97);
/// Fra la luce media del foglio e quella del fondo almeno tanto (0-255).
const CONTRASTO: f32 = 40.0;
/// Il quadrilatero tiene almeno l'85% dell'area del contorno: un foglio, non una macchia storta.
const QUADRATO: f32 = 0.85;

/// Il foglio trovato: gli angoli in senso orario dall'alto a sinistra (pixel della foto, la y in
/// giù) e, se i lati sono curvi (foglio piegato, lente), i punti in mezzo ai lati alto, destro,
/// basso e sinistro.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Trovato {
    pub(crate) angoli: [[f32; 2]; 4],
    pub(crate) lati: Option<[[f32; 2]; 4]>,
}

/// Un lato che si scosta dalla sua corda meno di così (frazione della sua lunghezza) è dritto.
const CURVA_MINIMA: f32 = 0.006;
/// Il punto in mezzo a un lato si cerca fino a tanto (frazione della sua lunghezza) dalla corda.
const CURVA_MASSIMA: f32 = 0.15;

/// Il foglio nella foto `rgba` (`w` × `h`), se è chiaro su un fondo più scuro.
pub(crate) fn trova_il_foglio(rgba: &[u8], w: u32, h: u32) -> Option<Trovato> {
    let (w, h) = (w as usize, h as usize);
    if w < 16 || h < 16 || rgba.len() != w * h * 4 {
        return None;
    }
    // La foto piccola, in grigi: la media di ogni quadretto k × k.
    let k = w.max(h).div_ceil(LATO).max(1);
    let (pw, ph) = (w / k, h / k);
    let mut grigi = vec![0f32; pw * ph];
    for (i, g) in grigi.iter_mut().enumerate() {
        let (px, py) = (i % pw, i / pw);
        let mut somma = 0u32;
        for y in py * k..(py + 1) * k {
            for x in px * k..(px + 1) * k {
                let p = &rgba[(y * w + x) * 4..(y * w + x) * 4 + 3];
                somma += (299 * u32::from(p[0]) + 587 * u32::from(p[1]) + 114 * u32::from(p[2])) / 1000;
            }
        }
        *g = somma as f32 / (k * k) as f32;
    }
    let soglia = otsu(&grigi);
    // La parte chiara più grande, a quattro vicini.
    let mut etichetta = vec![0u32; pw * ph];
    let (mut migliore, mut area_migliore, mut prossima) = (0u32, 0usize, 1u32);
    let mut pila = Vec::new();
    for i in 0..pw * ph {
        if etichetta[i] != 0 || grigi[i] <= soglia {
            continue;
        }
        let mut area = 0;
        etichetta[i] = prossima;
        pila.push(i);
        while let Some(j) = pila.pop() {
            area += 1;
            let (x, y) = (j % pw, j / pw);
            let vicini = [(x > 0).then(|| j - 1), (x + 1 < pw).then(|| j + 1), (y > 0).then(|| j - pw), (y + 1 < ph).then(|| j + pw)];
            for v in vicini.into_iter().flatten() {
                if etichetta[v] == 0 && grigi[v] > soglia {
                    etichetta[v] = prossima;
                    pila.push(v);
                }
            }
        }
        if area > area_migliore {
            (migliore, area_migliore) = (prossima, area);
        }
        prossima += 1;
    }
    let quota = area_migliore as f32 / (pw * ph) as f32;
    if !(AREA.0..=AREA.1).contains(&quota) {
        return None;
    }
    // Il bordo della parte chiara; il foglio non è il fondo (non tocca quasi tutto il giro della foto).
    let dentro = |x: usize, y: usize| etichetta[y * pw + x] == migliore;
    let mut bordo = Vec::new();
    let mut sul_giro = 0;
    for y in 0..ph {
        for x in 0..pw {
            if !dentro(x, y) {
                continue;
            }
            let al_giro = x == 0 || y == 0 || x + 1 == pw || y + 1 == ph;
            sul_giro += usize::from(al_giro);
            if al_giro || !dentro(x - 1, y) || !dentro(x + 1, y) || !dentro(x, y - 1) || !dentro(x, y + 1) {
                bordo.push([x as f32 + 0.5, y as f32 + 0.5]);
            }
        }
    }
    if sul_giro as f32 > 0.6 * (2 * (pw + ph)) as f32 {
        return None;
    }
    // Il fondo più scuro del foglio, di qualcosa: la luce media fuori dal contorno convesso.
    let contorno = involucro(bordo);
    if contorno.len() < 4 {
        return None;
    }
    let (mut luce_dentro, mut n_dentro, mut luce_fuori, mut n_fuori) = (0.0, 0usize, 0.0, 0usize);
    for (i, g) in grigi.iter().enumerate() {
        let p = [(i % pw) as f32 + 0.5, (i / pw) as f32 + 0.5];
        if nel_poligono(&contorno, p) {
            luce_dentro += g;
            n_dentro += 1;
        } else {
            luce_fuori += g;
            n_fuori += 1;
        }
    }
    if n_dentro == 0 || n_fuori == 0 || luce_dentro / (n_dentro as f32) - luce_fuori / (n_fuori as f32) < CONTRASTO {
        return None;
    }
    let angoli = quattro_angoli(&contorno)?;
    if area(&angoli) < QUADRATO * area(&contorno) {
        return None;
    }
    // I lati curvi: lungo la perpendicolare dal mezzo di ogni lato dritto, da fuori verso dentro,
    // il primo punto del foglio (con altri due dietro: un granello non conta).
    let centro = [angoli.iter().map(|p| p[0]).sum::<f32>() / 4.0, angoli.iter().map(|p| p[1]).sum::<f32>() / 4.0];
    let nel_foglio = |p: [f32; 2]| {
        let (x, y) = (p[0].floor(), p[1].floor());
        x >= 0.0 && y >= 0.0 && (x as usize) < pw && (y as usize) < ph && dentro(x as usize, y as usize)
    };
    let mut curvi = false;
    let lati: [[f32; 2]; 4] = std::array::from_fn(|i| {
        let (a, b) = (angoli[i], angoli[(i + 1) % 4]);
        let m = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
        let l = (b[0] - a[0]).hypot(b[1] - a[1]).max(1e-3);
        let mut n = [-(b[1] - a[1]) / l, (b[0] - a[0]) / l];
        if n[0] * (m[0] - centro[0]) + n[1] * (m[1] - centro[1]) < 0.0 {
            n = [-n[0], -n[1]];
        }
        let lontano = CURVA_MASSIMA * l;
        let mut s = lontano;
        while s >= -lontano {
            let p = |d: f32| [m[0] + n[0] * (s - d), m[1] + n[1] * (s - d)];
            if nel_foglio(p(0.0)) && nel_foglio(p(1.0)) && nel_foglio(p(2.0)) {
                if s.abs() >= (CURVA_MINIMA * l).max(1.0) {
                    curvi = true;
                    return [m[0] + n[0] * s, m[1] + n[1] * s];
                }
                return m;
            }
            s -= 0.5;
        }
        m
    });
    let a_foto = |p: [f32; 2]| [p[0] * k as f32, p[1] * k as f32];
    Some(Trovato { angoli: angoli.map(a_foto), lati: curvi.then(|| lati.map(a_foto)) })
}

/// La soglia di Otsu: quella che separa meglio i grigi in due gruppi.
fn otsu(grigi: &[f32]) -> f32 {
    let mut istogramma = [0usize; 256];
    for g in grigi {
        istogramma[(*g as usize).min(255)] += 1;
    }
    let n = grigi.len() as f64;
    let totale: f64 = istogramma.iter().enumerate().map(|(i, c)| i as f64 * *c as f64).sum();
    let (mut sotto, mut somma_sotto, mut migliore, mut soglia) = (0.0, 0.0, -1.0, 128.0);
    for (t, c) in istogramma.iter().enumerate() {
        sotto += *c as f64;
        somma_sotto += t as f64 * *c as f64;
        let sopra = n - sotto;
        if sotto == 0.0 || sopra == 0.0 {
            continue;
        }
        let (m0, m1) = (somma_sotto / sotto, (totale - somma_sotto) / sopra);
        let fra = sotto * sopra * (m0 - m1) * (m0 - m1);
        if fra > migliore {
            (migliore, soglia) = (fra, t as f32 + 0.5);
        }
    }
    soglia
}

/// Il contorno convesso dei punti (catena monotona), in senso orario con la y in giù.
pub(crate) fn involucro(mut punti: Vec<[f32; 2]>) -> Vec<[f32; 2]> {
    punti.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    punti.dedup();
    if punti.len() < 3 {
        return punti;
    }
    let giro = |o: [f32; 2], a: [f32; 2], b: [f32; 2]| (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
    let mut sotto: Vec<[f32; 2]> = Vec::new();
    for p in &punti {
        while sotto.len() >= 2 && giro(sotto[sotto.len() - 2], sotto[sotto.len() - 1], *p) <= 0.0 {
            sotto.pop();
        }
        sotto.push(*p);
    }
    let mut sopra: Vec<[f32; 2]> = Vec::new();
    for p in punti.iter().rev() {
        while sopra.len() >= 2 && giro(sopra[sopra.len() - 2], sopra[sopra.len() - 1], *p) <= 0.0 {
            sopra.pop();
        }
        sopra.push(*p);
    }
    sotto.pop();
    sopra.pop();
    sotto.extend(sopra);
    sotto
}

/// L'area di un poligono (valore assoluto).
fn area(p: &[[f32; 2]]) -> f32 {
    let n = p.len();
    (0..n).map(|i| p[i][0] * p[(i + 1) % n][1] - p[(i + 1) % n][0] * p[i][1]).sum::<f32>().abs() / 2.0
}

/// Il punto sta dentro il poligono convesso.
fn nel_poligono(p: &[[f32; 2]], q: [f32; 2]) -> bool {
    let n = p.len();
    let segni: Vec<f32> = (0..n).map(|i| (p[(i + 1) % n][0] - p[i][0]) * (q[1] - p[i][1]) - (p[(i + 1) % n][1] - p[i][1]) * (q[0] - p[i][0])).collect();
    segni.iter().all(|s| *s >= 0.0) || segni.iter().all(|s| *s <= 0.0)
}

/// I quattro vertici del contorno che tengono più area. Prima si tolgono i punti quasi dritti fra
/// i loro vicini (i gradini dei pixel lungo i lati), poi si provano tutte le diagonali: per ognuna
/// il vertice più lontano da una parte e dall'altra. In senso orario dall'alto a sinistra.
fn quattro_angoli(contorno: &[[f32; 2]]) -> Option<[[f32; 2]; 4]> {
    let mut v = contorno.to_vec();
    // Un punto a meno di un pixel dalla retta dei suoi vicini non è un angolo.
    loop {
        let n = v.len();
        if n <= 4 {
            break;
        }
        let distanza = |i: usize| {
            let (a, p, b) = (v[(i + n - 1) % n], v[i], v[(i + 1) % n]);
            2.0 * area(&[a, p, b]) / (b[0] - a[0]).hypot(b[1] - a[1]).max(1e-3)
        };
        let Some(via) = (0..n).filter(|i| distanza(*i) < 1.0).min_by(|a, b| distanza(*a).total_cmp(&distanza(*b))) else { break };
        v.remove(via);
    }
    let n = v.len();
    if n < 4 {
        return None;
    }
    let tri = |a: usize, b: usize, c: usize| area(&[v[a % n], v[b % n], v[c % n]]);
    let mut migliore: Option<(f32, [usize; 4])> = None;
    for i in 0..n {
        for k in i + 2..n {
            let Some(j) = (i + 1..k).max_by(|x, y| tri(i, *x, k).total_cmp(&tri(i, *y, k))) else { continue };
            let Some(l) = (k + 1..i + n).max_by(|x, y| tri(k, *x, i).total_cmp(&tri(k, *y, i))) else { continue };
            let tot = tri(i, j, k) + tri(k, l, i);
            if migliore.is_none_or(|(m, _)| tot > m) {
                migliore = Some((tot, [i, j, k, l % n]));
            }
        }
    }
    let mut a = migliore?.1.map(|i| v[i]);
    // Dall'alto a sinistra (x + y più piccolo), in senso orario sullo schermo.
    let centro = [a.iter().map(|p| p[0]).sum::<f32>() / 4.0, a.iter().map(|p| p[1]).sum::<f32>() / 4.0];
    a.sort_by(|p, q| (p[1] - centro[1]).atan2(p[0] - centro[0]).total_cmp(&(q[1] - centro[1]).atan2(q[0] - centro[0])));
    let primo = (0..4).min_by(|i, j| (a[*i][0] + a[*i][1]).total_cmp(&(a[*j][0] + a[*j][1])))?;
    a.rotate_left(primo);
    Some(a)
}

#[cfg(test)]
pub(crate) mod test {
    use super::*;

    /// Una foto finta: un foglio chiaro con le righe di testo, coi suoi `angoli`, su un fondo
    /// `fondo` con la grana (un tavolo), con un'ombra che scende da sinistra.
    pub(crate) fn foto(w: u32, h: u32, angoli: [[f32; 2]; 4], fondo: [u8; 3], carta: [u8; 3]) -> Vec<u8> {
        let mut semi = 12345u32;
        let mut caso = || {
            semi = semi.wrapping_mul(1_103_515_245).wrapping_add(12345);
            (semi >> 16) % 40
        };
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                let p = [x as f32 + 0.5, y as f32 + 0.5];
                let c = if nel_poligono(&angoli, p) {
                    // Le righe di testo: ogni 30 pixel una riga scura alta 8, nel mezzo del foglio.
                    let riga = (y / 30) % 2 == 0 && (y % 30) < 8 && x % 40 < 32;
                    let ombra = 1.0 - 0.15 * (x as f32 / w as f32);
                    if riga && p[0] > angoli[0][0] + 60.0 && p[0] < angoli[1][0] - 60.0 && p[1] > angoli[0][1] + 60.0 && p[1] < angoli[3][1] - 60.0 {
                        [40, 40, 40]
                    } else {
                        carta.map(|v| (v as f32 * ombra) as u8)
                    }
                } else {
                    let g = caso() as u8;
                    fondo.map(|v| v.saturating_add(g))
                };
                rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
        rgba
    }

    fn vicini(a: [[f32; 2]; 4], b: [[f32; 2]; 4], quanto: f32) -> bool {
        a.iter().zip(b).all(|(p, q)| (p[0] - q[0]).abs() <= quanto && (p[1] - q[1]).abs() <= quanto)
    }

    #[test]
    fn un_foglio_bianco_su_un_tavolo_scuro_si_trova() {
        // Dritto, storto in prospettiva, girato.
        let casi = [
            [[150.0, 120.0], [850.0, 120.0], [850.0, 1100.0], [150.0, 1100.0]],
            [[220.0, 160.0], [820.0, 110.0], [900.0, 1150.0], [120.0, 1080.0]],
            [[400.0, 80.0], [940.0, 420.0], [600.0, 1180.0], [60.0, 820.0]],
        ];
        for angoli in casi {
            let rgba = foto(1000, 1250, angoli, [45, 35, 30], [240, 238, 230]);
            let t = trova_il_foglio(&rgba, 1000, 1250).expect("il foglio");
            assert!(vicini(t.angoli, angoli, 12.0), "{:?} invece di {angoli:?}", t.angoli);
        }
    }

    #[test]
    fn un_foglio_piegato_ha_i_lati_curvi() {
        // I lati alto e basso curvi in giù di 40 pixel nel mezzo; gli altri dritti.
        let (w, h) = (1000u32, 1250u32);
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                let u = (x as f32 - 150.0) / 700.0;
                let piega = 40.0 * 4.0 * u * (1.0 - u);
                let dentro = (0.0..=1.0).contains(&u) && (y as f32) >= 120.0 + piega && (y as f32) <= 1100.0 + piega;
                let c = if dentro { [240, 238, 230] } else { [40, 35, 30] };
                rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
        let t = trova_il_foglio(&rgba, w, h).expect("il foglio");
        let lati = t.lati.expect("i lati curvi");
        let attesi = [[500.0, 160.0], [850.0, 610.0], [500.0, 1140.0], [150.0, 610.0]];
        for (l, a) in lati.iter().zip(attesi) {
            assert!((l[0] - a[0]).abs() <= 14.0 && (l[1] - a[1]).abs() <= 14.0, "{lati:?} invece di {attesi:?}");
        }
        // Un foglio coi lati dritti non ha lati curvi.
        let dritto = foto(1000, 1250, [[150.0, 120.0], [850.0, 120.0], [850.0, 1100.0], [150.0, 1100.0]], [45, 35, 30], [240, 238, 230]);
        assert_eq!(trova_il_foglio(&dritto, 1000, 1250).unwrap().lati, None);
    }

    #[test]
    fn senza_contrasto_o_senza_foglio_niente() {
        let angoli = [[150.0, 120.0], [850.0, 120.0], [850.0, 1100.0], [150.0, 1100.0]];
        // Un foglio bianco su un tavolo bianco: il riconoscitore del motore, non questo.
        let rgba = foto(1000, 1250, angoli, [215, 215, 210], [240, 238, 230]);
        assert_eq!(trova_il_foglio(&rgba, 1000, 1250), None);
        // Tutto foglio, o un foglio piccolissimo.
        let tutto = foto(400, 500, [[-5.0, -5.0], [405.0, -5.0], [405.0, 505.0], [-5.0, 505.0]], [40, 40, 40], [240, 238, 230]);
        assert_eq!(trova_il_foglio(&tutto, 400, 500), None);
        let piccolo = foto(400, 500, [[190.0, 240.0], [210.0, 240.0], [210.0, 260.0], [190.0, 260.0]], [40, 40, 40], [240, 238, 230]);
        assert_eq!(trova_il_foglio(&piccolo, 400, 500), None);
        assert_eq!(trova_il_foglio(&[0; 12], 1, 3), None);
    }
}
