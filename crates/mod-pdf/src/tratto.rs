//! Il tratto della firma col dito (ritocchi della 0.8.2, R2). L'autore il 28/10: «la firma con il
//! dito non è bella, il tratto è troppo omogeneo, usa un tratto tipo stilografico o fammi
//! scegliere più tratti».
//!
//! Di ogni punto del dito si tiene la velocità, e lo spessore viene da lì e dal tratto scelto: la
//! stilografica è più sottile dove il dito corre e più piena dove rallenta, come l'inchiostro; ha
//! il pennino tagliato e tenuto inclinato, quindi i tratti che scendono verso destra sono pieni e
//! quelli che salgono verso destra sottili; e i suoi capi si assottigliano. La penna è sottile e
//! uguale, il pennarello spesso e uguale. La velocità è filtrata come in signature_pad (0,7 della
//! nuova, 0,3 della vecchia) e lo spessore della stilografica è il massimo diviso (velocità + 1),
//! mai sotto il minimo.
//!
//! Un tratto diventa pezzi pieni: i bordi a sinistra e a destra a mezzo spessore, punto per punto,
//! e i capi tondi. Dove il tratto gira stretto (più di 50 gradi fra due lati, o tanto che un bordo
//! tornerebbe indietro) un pezzo finisce e il prossimo comincia lì: i due capi tondi fanno la
//! giunta. Così i contorni girano tutti nello stesso verso e, riempiti con la regola del non-zero,
//! non lasciano buchi dove si sovrappongono. Gli stessi contorni vanno sullo schermo e nel PDF.

/// Due punti del dito più vicini di così (dp) sono uno solo.
pub const PASSO_MINIMO: f32 = 1.0;
/// Il filtro della velocità: quanto pesa quella nuova.
const PESO_NUOVA: f32 = 0.7;
/// Fra due punti del dito si contano almeno 4 ms: due eventi dello stesso frame non fanno una
/// velocità enorme.
const TEMPO_MINIMO: f32 = 4.0;
/// La stilografica: lo spessore col dito fermo e il più sottile (dp).
const STILO_PIENO: f32 = 5.0;
const STILO_SOTTILE: f32 = 1.0;
/// Il taglio del pennino della stilografica, inclinato di 40 gradi verso l'alto a destra (come si
/// tiene la penna): un tratto di traverso al taglio è pieno, uno lungo il taglio scende a 45
/// centesimi; mai sotto 0,6 dp.
const PENNINO: f32 = 40.0;
const LUNGO_IL_TAGLIO: f32 = 0.45;
const STILO_FILO: f32 = 0.6;
/// I capi della stilografica: lunghi fino a tanti dp (al più un terzo del tratto), e quanto
/// sottili cominciano (una parte dello spessore).
const CAPO_STILO: f32 = 14.0;
const PUNTA_STILO: f32 = 0.3;
/// La penna e il pennarello (dp).
const PENNA: f32 = 2.0;
const PENNARELLO: f32 = 4.6;
/// Un angolo più stretto di così fra due lati (il coseno: circa 50 gradi) chiude il pezzo.
const COS_SPIGOLO: f32 = 0.64;

/// Il tratto scelto per firmare.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Stile {
    /// Più sottile dove il dito corre, coi capi sottili.
    #[default]
    Stilografica,
    /// Sottile e uguale.
    Penna,
    /// Spesso e uguale.
    Pennarello,
}

impl Stile {
    pub const TUTTI: [Stile; 3] = [Stile::Stilografica, Stile::Penna, Stile::Pennarello];

    /// Il nome per chi usa l'app.
    pub fn nome(self) -> &'static str {
        match self {
            Stile::Stilografica => "Stilografica",
            Stile::Penna => "Penna",
            Stile::Pennarello => "Pennarello",
        }
    }

    /// Il nome nel file della firma e nella diagnosi.
    pub fn chiave(self) -> &'static str {
        match self {
            Stile::Stilografica => "stilografica",
            Stile::Penna => "penna",
            Stile::Pennarello => "pennarello",
        }
    }

    pub fn da_chiave(chiave: &str) -> Option<Stile> {
        Stile::TUTTI.into_iter().find(|s| s.chiave() == chiave)
    }

    /// Lo spessore in dp col dito alla velocità `v` (dp al millisecondo).
    pub fn spessore(self, v: f32) -> f32 {
        match self {
            Stile::Stilografica => (STILO_PIENO / (v.max(0.0) + 1.0)).max(STILO_SOTTILE),
            Stile::Penna => PENNA,
            Stile::Pennarello => PENNARELLO,
        }
    }

    /// Lo spessore di ogni punto di un tratto, nella misura dei punti (`scala` dp per unità, y
    /// verso il basso come sullo schermo): dalla velocità e, per la stilografica, dal verso del
    /// tratto sul taglio del pennino e coi capi che si assottigliano.
    pub fn spessori(self, punti: &[Punto], scala: f32) -> Vec<f32> {
        let scala = if scala.is_finite() && scala > 0.0 { scala } else { 1.0 };
        let mut spessori: Vec<f32> = punti.iter().map(|p| self.spessore(p.v) / scala).collect();
        if self == Stile::Stilografica && punti.len() > 1 {
            let n = punti.len();
            for (i, w) in spessori.iter_mut().enumerate() {
                let (a, b) = (punti[i.saturating_sub(1)], punti[(i + 1).min(n - 1)]);
                *w = (*w * taglio(b.x - a.x, b.y - a.y)).max(STILO_FILO / scala);
            }
            // La strada fatta dall'inizio, in dp.
            let mut strada = Vec::with_capacity(punti.len());
            let mut fatta = 0.0;
            for (i, p) in punti.iter().enumerate() {
                if i > 0 {
                    fatta += (p.x - punti[i - 1].x).hypot(p.y - punti[i - 1].y) * scala;
                }
                strada.push(fatta);
            }
            let capo = CAPO_STILO.min(fatta / 3.0);
            if capo > 1e-3 {
                for (w, s) in spessori.iter_mut().zip(&strada) {
                    let t = (s.min(fatta - s) / capo).clamp(0.0, 1.0);
                    *w *= PUNTA_STILO + (1.0 - PUNTA_STILO) * t * t * (3.0 - 2.0 * t);
                }
            }
        }
        spessori
    }
}

/// Quanto è pieno col pennino della stilografica un tratto che va verso `(dx, dy)` (y verso il
/// basso): 1 di traverso al taglio, `LUNGO_IL_TAGLIO` lungo il taglio.
fn taglio(dx: f32, dy: f32) -> f32 {
    let l = dx.hypot(dy);
    if l < 1e-12 {
        return 1.0;
    }
    let (s, c) = PENNINO.to_radians().sin_cos();
    // Il taglio va a destra e in su: (c, -s) con la y verso il basso.
    let traverso = ((dx * -s - dy * c) / l).abs();
    LUNGO_IL_TAGLIO + (1.0 - LUNGO_IL_TAGLIO) * traverso
}

/// Il colore dell'inchiostro.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Colore {
    #[default]
    Nero,
    Blu,
}

impl Colore {
    pub const TUTTI: [Colore; 2] = [Colore::Nero, Colore::Blu];

    /// Il nome per chi usa l'app.
    pub fn nome(self) -> &'static str {
        match self {
            Colore::Nero => "Nero",
            Colore::Blu => "Blu",
        }
    }

    /// Il nome nel file della firma e nella diagnosi.
    pub fn chiave(self) -> &'static str {
        match self {
            Colore::Nero => "nero",
            Colore::Blu => "blu",
        }
    }

    pub fn da_chiave(chiave: &str) -> Option<Colore> {
        Colore::TUTTI.into_iter().find(|c| c.chiave() == chiave)
    }

    /// Il colore in RGB: il nero di una penna (non il nero pieno della stampa) e il blu di una
    /// stilografica.
    pub fn rgb(self) -> [u8; 3] {
        match self {
            Colore::Nero => [0x1B, 0x1B, 0x1F],
            Colore::Blu => [0x14, 0x2D, 0x8C],
        }
    }
}

/// Un punto di un tratto: dove (dp, o nella misura della firma) e la velocità del dito lì (dp al
/// millisecondo).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Punto {
    pub x: f32,
    pub y: f32,
    pub v: f32,
}

/// I punti del dito come arrivano, `[x, y, ms]` (dp, e millisecondi dall'inizio del tratto): i
/// punti più vicini di un dp diventano uno, e ognuno ha la velocità filtrata del dito. Il primo
/// prende la velocità del secondo (il dito parte già in moto) e l'ultimo sta dov'è finito il dito.
pub fn punti_del_dito(grezzi: &[[f32; 3]]) -> Vec<Punto> {
    let mut punti: Vec<Punto> = Vec::with_capacity(grezzi.len());
    let (mut tempo, mut v) = (0.0, 0.0);
    let buoni = || grezzi.iter().filter(|g| g.iter().all(|c| c.is_finite()));
    for g in buoni() {
        let Some(u) = punti.last() else {
            punti.push(Punto { x: g[0], y: g[1], v: 0.0 });
            tempo = g[2];
            continue;
        };
        let strada = (g[0] - u.x).hypot(g[1] - u.y);
        if strada < PASSO_MINIMO {
            continue;
        }
        v = PESO_NUOVA * strada / (g[2] - tempo).max(TEMPO_MINIMO) + (1.0 - PESO_NUOVA) * v;
        punti.push(Punto { x: g[0], y: g[1], v });
        tempo = g[2];
    }
    if let Some(g) = buoni().next_back()
        && punti.len() > 1
        && let Some(u) = punti.last_mut()
        && (u.x, u.y) != (g[0], g[1])
    {
        // Più vicino di un dp all'ultimo tenuto: l'ultimo si sposta lì.
        (u.x, u.y) = (g[0], g[1]);
    }
    if punti.len() > 1 {
        punti[0].v = punti[1].v;
    }
    // Una media fra vicini: la velocità di un frame solo non fa un nodo.
    if punti.len() > 2 {
        let v: Vec<f32> = punti.iter().map(|p| p.v).collect();
        for i in 1..punti.len() - 1 {
            punti[i].v = 0.25 * v[i - 1] + 0.5 * v[i] + 0.25 * v[i + 1];
        }
        punti[0].v = punti[1].v;
    }
    punti
}

/// Un tratto più liscio: due giri di Chaikin (ogni lato tagliato a un quarto e a tre quarti), coi
/// due capi dove sono; la velocità si taglia come i punti.
pub fn liscia(punti: &[Punto]) -> Vec<Punto> {
    let fra = |a: Punto, b: Punto, t: f32| Punto { x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t, v: a.v + (b.v - a.v) * t };
    let mut v = punti.to_vec();
    for _ in 0..2 {
        if v.len() < 3 {
            break;
        }
        let mut n = Vec::with_capacity(v.len() * 2);
        n.push(v[0]);
        for w in v.windows(2) {
            n.push(fra(w[0], w[1], 0.25));
            n.push(fra(w[0], w[1], 0.75));
        }
        n.push(v[v.len() - 1]);
        v = n;
    }
    v
}

/// Un tratto della penna del modo Annota, senza velocità: i punti troppo vicini (meno di un
/// `PASSO_MINIMO`) diventano uno, l'ultimo resta, poi due giri di Chaikin come in [`liscia`].
pub fn liscia_xy(punti: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let mut v: Vec<[f32; 2]> = Vec::with_capacity(punti.len());
    for p in punti.iter().filter(|p| p[0].is_finite() && p[1].is_finite()) {
        match v.last() {
            Some(u) if (p[0] - u[0]).hypot(p[1] - u[1]) < PASSO_MINIMO => {}
            _ => v.push(*p),
        }
    }
    if let (Some(ultimo), Some(u)) = (punti.last(), v.last())
        && ultimo != u
        && ultimo[0].is_finite()
        && ultimo[1].is_finite()
    {
        v.push(*ultimo);
    }
    let punti: Vec<Punto> = v.iter().map(|p| Punto { x: p[0], y: p[1], v: 0.0 }).collect();
    liscia(&punti).iter().map(|p| [p.x, p.y]).collect()
}

/// Un capo tondo di un pezzo: dove, quanto grande (mezzo spessore) e il verso del tratto lì.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Capo {
    pub centro: [f32; 2],
    pub raggio: f32,
    pub verso: [f32; 2],
}

/// Un pezzo di tratto pieno: i bordi a sinistra e a destra del tratto, punto per punto, e i capi
/// tondi all'inizio e alla fine.
#[derive(Debug, Clone, PartialEq)]
pub struct Pezzo {
    pub sinistra: Vec<[f32; 2]>,
    pub destra: Vec<[f32; 2]>,
    pub capi: [Capo; 2],
}

fn meno(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn piu(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] + b[0], a[1] + b[1]]
}

fn per(a: [f32; 2], k: f32) -> [f32; 2] {
    [a[0] * k, a[1] * k]
}

fn scalare(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}

fn lungo(a: [f32; 2]) -> f32 {
    a[0].hypot(a[1])
}

/// Il verso a sinistra di `t`, girato di un quarto.
fn normale(t: [f32; 2]) -> [f32; 2] {
    [-t[1], t[0]]
}

impl Pezzo {
    /// Un punto solo: un cerchio di raggio `raggio`.
    fn punto(centro: [f32; 2], raggio: f32) -> Pezzo {
        let capo = Capo { centro, raggio, verso: [1.0, 0.0] };
        Pezzo { sinistra: vec![[centro[0], centro[1] + raggio]], destra: vec![[centro[0], centro[1] - raggio]], capi: [capo, capo] }
    }

    /// L'arco di un capo in `passi` lati, senza i due estremi (stanno sui bordi): alla fine
    /// (`fine`) davanti, dal bordo sinistro al destro; all'inizio dietro, dal destro al sinistro.
    pub fn arco(&self, fine: bool, passi: usize) -> Vec<[f32; 2]> {
        let capo = &self.capi[usize::from(fine)];
        let (t, n) = (capo.verso, normale(capo.verso));
        let r = if fine { capo.raggio } else { -capo.raggio };
        let passi = passi.max(2);
        (1..passi)
            .map(|k| {
                let (s, c) = (std::f32::consts::PI * k as f32 / passi as f32).sin_cos();
                piu(capo.centro, per(piu(per(n, c), per(t, s)), r))
            })
            .collect()
    }

    /// Il contorno chiuso: il bordo sinistro avanti, il capo della fine, il bordo destro
    /// indietro, il capo dell'inizio. I capi in `passi` lati.
    pub fn contorno(&self, passi: usize) -> Vec<[f32; 2]> {
        let mut c = Vec::with_capacity(self.sinistra.len() + self.destra.len() + 2 * passi);
        c.extend_from_slice(&self.sinistra);
        c.extend(self.arco(true, passi));
        c.extend(self.destra.iter().rev());
        c.extend(self.arco(false, passi));
        c
    }
}

/// Il verso a sinistra nel punto `k` del pezzo da `a` a `b`: ai capi quello del lato, in mezzo la
/// media dei due lati.
fn normale_in(lati: &[[f32; 2]], a: usize, b: usize, k: usize) -> [f32; 2] {
    if k == a {
        return normale(lati[a]);
    }
    if k == b {
        return normale(lati[b - 1]);
    }
    let s = piu(normale(lati[k - 1]), normale(lati[k]));
    let l = lungo(s);
    if l < 1e-6 { normale(lati[k]) } else { per(s, 1.0 / l) }
}

/// Il pezzo che va da `s` a `e` si può allungare fino a `e + 1`: l'angolo in `e` non è uno
/// spigolo e, coi versi nuovi, nessun bordo dei due lati toccati torna indietro.
fn si_allunga(p: &[[f32; 2]], w: &[f32], lati: &[[f32; 2]], s: usize, e: usize) -> bool {
    if scalare(lati[e - 1], lati[e]) < COS_SPIGOLO {
        return false;
    }
    for k in e - 1..=e {
        let (n0, n1) = (normale_in(lati, s, e + 1, k), normale_in(lati, s, e + 1, k + 1));
        for lato in [0.5, -0.5] {
            let o0 = piu(p[k], per(n0, lato * w[k]));
            let o1 = piu(p[k + 1], per(n1, lato * w[k + 1]));
            if scalare(meno(o1, o0), lati[k]) <= 0.0 {
                return false;
            }
        }
    }
    true
}

/// I pezzi pieni di un tratto coi suoi spessori (nella stessa misura dei punti).
pub fn pezzi(punti: &[[f32; 2]], spessori: &[f32]) -> Vec<Pezzo> {
    // I punti doppi no: un lato lungo zero non ha verso.
    let (mut p, mut w): (Vec<[f32; 2]>, Vec<f32>) = (Vec::with_capacity(punti.len()), Vec::with_capacity(punti.len()));
    for (q, s) in punti.iter().zip(spessori) {
        if !(q[0].is_finite() && q[1].is_finite() && s.is_finite()) {
            continue;
        }
        if p.last().is_some_and(|u| lungo(meno(*q, *u)) < 1e-6 * (1.0 + q[0].abs().max(q[1].abs()))) {
            continue;
        }
        p.push(*q);
        w.push(s.max(0.0));
    }
    match p.len() {
        0 => return Vec::new(),
        1 => return vec![Pezzo::punto(p[0], w[0] / 2.0)],
        _ => {}
    }
    let lati: Vec<[f32; 2]> = p.windows(2).map(|c| per(meno(c[1], c[0]), 1.0 / lungo(meno(c[1], c[0])))).collect();
    let mut fatti = Vec::new();
    let mut s = 0;
    while s + 1 < p.len() {
        let mut e = s + 1;
        while e + 1 < p.len() && si_allunga(&p, &w, &lati, s, e) {
            e += 1;
        }
        let (mut sinistra, mut destra) = (Vec::with_capacity(e - s + 1), Vec::with_capacity(e - s + 1));
        for k in s..=e {
            let n = normale_in(&lati, s, e, k);
            sinistra.push(piu(p[k], per(n, w[k] / 2.0)));
            destra.push(piu(p[k], per(n, -w[k] / 2.0)));
        }
        let capi = [Capo { centro: p[s], raggio: w[s] / 2.0, verso: lati[s] }, Capo { centro: p[e], raggio: w[e] / 2.0, verso: lati[e - 1] }];
        fatti.push(Pezzo { sinistra, destra, capi });
        s = e;
    }
    fatti
}

/// Gli spessori meno `togli`, mai sotto tre decimi del loro: per lo schermo, che attorno ai pezzi
/// mette un filo sottile che smussa i bordi.
pub fn meno_il_filo(spessori: &mut [f32], togli: f32) {
    if togli > 0.0 {
        for w in spessori {
            *w = (*w - togli).max(*w * 0.3);
        }
    }
}

/// I pezzi pieni di un tratto del dito (y verso il basso, `scala` dp per unità dei punti) col
/// tratto scelto, con gli spessori meno `togli` (vedi [`meno_il_filo`]).
pub fn contorni_del_tratto(punti: &[Punto], stile: Stile, scala: f32, togli: f32) -> Vec<Pezzo> {
    let mut spessori = stile.spessori(punti, scala);
    meno_il_filo(&mut spessori, togli);
    let xy: Vec<[f32; 2]> = punti.iter().map(|p| [p.x, p.y]).collect();
    pezzi(&xy, &spessori)
}

/// La distanza di `p` dal lato da `a` a `b`.
fn dal_lato(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let ab = meno(b, a);
    let l2 = scalare(ab, ab);
    if l2 < 1e-20 {
        return lungo(meno(p, a));
    }
    let t = (scalare(meno(p, a), ab) / l2).clamp(0.0, 1.0);
    lungo(meno(p, piu(a, per(ab, t))))
}

/// Una spezzata con meno punti (Ramer, Douglas e Peucker): via quelli che stanno a meno di
/// `tolleranza` dalla spezzata che resta. Il primo e l'ultimo restano.
pub fn semplifica(punti: &[[f32; 2]], tolleranza: f32) -> Vec<[f32; 2]> {
    if punti.len() < 3 {
        return punti.to_vec();
    }
    let mut tieni = vec![false; punti.len()];
    tieni[0] = true;
    tieni[punti.len() - 1] = true;
    let mut pila = vec![(0, punti.len() - 1)];
    while let Some((a, b)) = pila.pop() {
        let (mut lontano, mut distanza) = (a, -1.0);
        for i in a + 1..b {
            let d = dal_lato(punti[i], punti[a], punti[b]);
            if d > distanza {
                (lontano, distanza) = (i, d);
            }
        }
        if distanza > tolleranza {
            tieni[lontano] = true;
            pila.push((a, lontano));
            pila.push((lontano, b));
        }
    }
    punti.iter().zip(tieni).filter(|(_, t)| *t).map(|(p, _)| *p).collect()
}

#[cfg(test)]
mod test {
    use super::*;

    /// Un tratto dritto verso destra: `n` punti ogni `passo` dp, uno ogni `ms` millisecondi.
    fn dritto(n: usize, passo: f32, ms: f32) -> Vec<[f32; 3]> {
        (0..n).map(|i| [i as f32 * passo, 0.0, i as f32 * ms]).collect()
    }

    /// L'area con segno di un contorno: il segno dice il verso.
    fn area(c: &[[f32; 2]]) -> f32 {
        (0..c.len()).map(|i| (c[i][0] * c[(i + 1) % c.len()][1] - c[(i + 1) % c.len()][0] * c[i][1]) / 2.0).sum()
    }

    fn limiti(c: &[[f32; 2]]) -> [f32; 4] {
        c.iter().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |l, p| [l[0].min(p[0]), l[1].min(p[1]), l[2].max(p[0]), l[3].max(p[1])])
    }

    #[test]
    fn la_velocita_viene_dalla_strada_e_dal_tempo() {
        // 10 dp ogni 10 ms: un dp al millisecondo, appena il filtro si assesta.
        let p = punti_del_dito(&dritto(20, 10.0, 10.0));
        assert_eq!(p.len(), 20);
        assert!(p[8..].iter().all(|q| (q.v - 1.0).abs() < 0.01), "{:?}", p.iter().map(|q| q.v).collect::<Vec<_>>());
        // Piano: un quarto.
        let p = punti_del_dito(&dritto(20, 2.5, 10.0));
        assert!((p[15].v - 0.25).abs() < 0.01, "{}", p[15].v);
        // Il primo ha la velocità del secondo.
        assert_eq!(p[0].v, p[1].v);
        // Due punti dello stesso frame: la velocità è grande ma non infinita.
        let p = punti_del_dito(&[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]]);
        assert!(p[1].v.is_finite() && p[1].v > 1.0);
        // I punti più vicini di un dp sono uno; l'ultimo sta dov'è finito il dito.
        let p = punti_del_dito(&[[0.0, 0.0, 0.0], [0.3, 0.0, 8.0], [0.6, 0.0, 16.0], [5.0, 0.0, 24.0], [5.4, 0.0, 32.0]]);
        assert_eq!(p.iter().map(|q| q.x).collect::<Vec<_>>(), vec![0.0, 5.4]);
        // Un tocco solo è un punto; i numeri che non vanno si saltano.
        assert_eq!(punti_del_dito(&[[3.0, 4.0, 0.0], [3.2, 4.1, 16.0]]).len(), 1);
        assert_eq!(punti_del_dito(&[[f32::NAN, 0.0, 0.0], [1.0, 1.0, 0.0]]).len(), 1);
    }

    #[test]
    fn i_tratti_della_penna_lisci_tengono_i_capi_e_tolgono_gli_angoli() {
        let zigzag = [[0.0, 0.0], [10.0, 10.0], [20.0, 0.0], [30.0, 10.0], [30.2, 10.1]];
        let l = liscia_xy(&zigzag);
        assert_eq!((l[0], *l.last().unwrap()), ([0.0, 0.0], [30.2, 10.1]));
        assert!(l.len() > zigzag.len());
        // Gli angoli del dito non ci sono più: la punta in [10, 10] si arrotonda sotto.
        assert!(!l.contains(&[10.0, 10.0]) && l.iter().all(|p| p[1] < 9.9 || p[0] > 29.0));
        // Un tocco solo resta un punto.
        assert_eq!(liscia_xy(&[[5.0, 5.0], [5.2, 5.1]]), vec![[5.0, 5.0], [5.2, 5.1]]);
    }

    #[test]
    fn i_tratti_lisci_tengono_i_capi_e_la_velocita() {
        let zigzag = [[0.0, 0.0, 0.0], [10.0, 10.0, 10.0], [20.0, 0.0, 20.0], [30.0, 10.0, 30.0]];
        let p = punti_del_dito(&zigzag);
        let l = liscia(&p);
        assert_eq!((l[0], *l.last().unwrap()), (p[0], *p.last().unwrap()));
        assert!(l.len() > p.len());
        // Gli angoli del dito non ci sono più: la punta in [10, 10] si arrotonda sotto.
        assert!(l.iter().all(|q| q.y < 9.9 || q.x > 29.0));
        // La velocità sta fra quelle dei punti vicini.
        let (lenta, veloce) = p.iter().fold((f32::MAX, f32::MIN), |(a, b), q| (a.min(q.v), b.max(q.v)));
        assert!(l.iter().all(|q| q.v >= lenta - 1e-6 && q.v <= veloce + 1e-6));
    }

    #[test]
    fn lo_spessore_viene_dalla_velocita_e_dal_tratto() {
        let s = Stile::Stilografica;
        assert_eq!(s.spessore(0.0), STILO_PIENO);
        assert!(s.spessore(1.0) < s.spessore(0.5) && s.spessore(0.5) < s.spessore(0.0));
        assert_eq!(s.spessore(100.0), STILO_SOTTILE);
        assert_eq!(Stile::Penna.spessore(0.0), Stile::Penna.spessore(3.0));
        assert!(Stile::Pennarello.spessore(1.0) > 2.0 * Stile::Penna.spessore(1.0));
        // La stilografica coi capi sottili: in mezzo lo spessore della velocità sul taglio del
        // pennino (il tratto va a destra), ai capi tre decimi.
        let p = punti_del_dito(&dritto(30, 4.0, 4.0));
        let w = s.spessori(&p, 1.0);
        let destra = taglio(1.0, 0.0);
        assert!((w[15] - s.spessore(p[15].v) * destra).abs() < 1e-4);
        assert!((w[0] - PUNTA_STILO * s.spessore(p[0].v) * destra).abs() < 1e-5, "{w:?}");
        assert!((w[29] - PUNTA_STILO * s.spessore(p[29].v) * destra).abs() < 1e-5, "{w:?}");
        assert!(w[0] < w[1] && w[1] < w[2] && w[2] < w[3], "{w:?}");
        // La penna no; nella misura della firma lo spessore si divide per la scala, e anche i capi
        // si contano in dp.
        assert!(Stile::Penna.spessori(&p, 1.0).iter().all(|v| (*v - PENNA).abs() < 1e-6));
        assert!((Stile::Penna.spessori(&p, 100.0)[0] - PENNA / 100.0).abs() < 1e-7);
        let piccolo: Vec<Punto> = p.iter().map(|q| Punto { x: q.x / 100.0, y: q.y / 100.0, v: q.v }).collect();
        let w100 = s.spessori(&piccolo, 100.0);
        assert!(w.iter().zip(&w100).all(|(a, b)| (a / 100.0 - b).abs() < 1e-6));
        // Il pennino: alla stessa velocità, giù verso destra è pieno, su verso destra sottile; in
        // giù e in su è lo stesso tratto all'incontrario.
        assert!((taglio(1.0, 1.0) - 0.996).abs() < 0.01 && taglio(1.0, -1.0) < 0.55, "{} {}", taglio(1.0, 1.0), taglio(1.0, -1.0));
        assert_eq!(taglio(0.0, 1.0), taglio(0.0, -1.0));
        let scende: Vec<[f32; 3]> = (0..30).map(|i| [i as f32 * 3.0, i as f32 * 3.0, i as f32 * 4.0]).collect();
        let sale: Vec<[f32; 3]> = (0..30).map(|i| [i as f32 * 3.0, -(i as f32) * 3.0, i as f32 * 4.0]).collect();
        let (pieno, sottile) = (s.spessori(&punti_del_dito(&scende), 1.0)[15], s.spessori(&punti_del_dito(&sale), 1.0)[15]);
        assert!(pieno > 1.8 * sottile, "{pieno} {sottile}");
        // Mai sotto il filo, nemmeno veloce lungo il taglio.
        let veloce: Vec<[f32; 3]> = (0..30).map(|i| [i as f32 * 30.0, -(i as f32) * 25.0, i as f32 * 4.0]).collect();
        assert!(s.spessori(&punti_del_dito(&veloce), 1.0)[15] >= STILO_FILO - 1e-6);
        // I nomi e le chiavi.
        assert!(Stile::TUTTI.iter().all(|s| Stile::da_chiave(s.chiave()) == Some(*s)));
        assert!(Colore::TUTTI.iter().all(|c| Colore::da_chiave(c.chiave()) == Some(*c)));
        assert_eq!((Stile::da_chiave("matita"), Colore::da_chiave("rosso")), (None, None));
    }

    #[test]
    fn il_contorno_di_un_tratto_dritto_e_di_un_punto() {
        let punti: Vec<[f32; 2]> = (0..=10).map(|i| [10.0 * i as f32, 0.0]).collect();
        let fatti = pezzi(&punti, &[4.0; 11]);
        assert_eq!(fatti.len(), 1);
        let p = &fatti[0];
        assert!(p.sinistra.iter().chain(&p.destra).all(|q| (q[1].abs() - 2.0).abs() < 1e-5));
        let c = p.contorno(16);
        let [x0, y0, x1, y1] = limiti(&c);
        assert!((x0 + 2.0).abs() < 1e-3 && (x1 - 102.0).abs() < 1e-3 && (y0 + 2.0).abs() < 1e-3 && (y1 - 2.0).abs() < 1e-3, "{:?}", [x0, y0, x1, y1]);
        // Un rettangolo di 100 per 4 e due mezzi cerchi di raggio 2.
        let a = area(&c);
        assert!((a.abs() - (400.0 + std::f32::consts::PI * 4.0)).abs() < 0.2, "{a}");
        // All'indietro gira nello stesso verso; un punto solo è un cerchio, nello stesso verso.
        let indietro: Vec<[f32; 2]> = punti.iter().rev().copied().collect();
        assert_eq!(area(&pezzi(&indietro, &[4.0; 11])[0].contorno(16)).signum(), a.signum());
        let giu: Vec<[f32; 2]> = punti.iter().map(|q| [q[1], q[0]]).collect();
        assert_eq!(area(&pezzi(&giu, &[4.0; 11])[0].contorno(16)).signum(), a.signum());
        let punto = pezzi(&[[5.0, 5.0]], &[3.0]);
        assert_eq!(punto.len(), 1);
        let b = area(&punto[0].contorno(32));
        assert!((b.abs() - std::f32::consts::PI * 2.25).abs() < 0.1 && b.signum() == a.signum(), "{b}");
        // I punti doppi e quelli che non vanno si saltano.
        assert_eq!(pezzi(&[[0.0, 0.0], [0.0, 0.0], [f32::NAN, 1.0], [10.0, 0.0]], &[2.0; 4])[0].sinistra.len(), 2);
        assert!(pezzi(&[], &[]).is_empty());
    }

    /// In ogni pezzo i due bordi vanno avanti come il tratto (il tratto è a metà fra i due).
    fn bordi_avanti(p: &Pezzo) -> bool {
        let mezzo: Vec<[f32; 2]> = p.sinistra.iter().zip(&p.destra).map(|(a, b)| per(piu(*a, *b), 0.5)).collect();
        [&p.sinistra, &p.destra]
            .iter()
            .all(|bordo| (1..bordo.len()).all(|k| scalare(meno(bordo[k], bordo[k - 1]), meno(mezzo[k], mezzo[k - 1])) > 0.0))
    }

    #[test]
    fn uno_spigolo_divide_il_tratto_e_nessun_bordo_torna_indietro() {
        // Una V: due pezzi, che si toccano nella punta coi capi tondi.
        let fatti = pezzi(&[[0.0, 0.0], [10.0, 10.0], [20.0, 0.0]], &[2.0; 3]);
        assert_eq!(fatti.len(), 2);
        assert_eq!((fatti[0].capi[1].centro, fatti[1].capi[0].centro), ([10.0, 10.0], [10.0, 10.0]));
        // Una curva dolce: un pezzo solo, coi bordi che vanno avanti.
        let arco: Vec<[f32; 2]> = (0..=20).map(|i| (i as f32 * 0.05).sin_cos()).map(|(s, c)| [100.0 * c, 100.0 * s]).collect();
        let fatti = pezzi(&arco, &[3.0; 21]);
        assert_eq!(fatti.len(), 1);
        assert!(bordi_avanti(&fatti[0]));
        // Una curva più stretta del mezzo spessore: il bordo di dentro tornerebbe indietro, quindi
        // si divide, e in ogni pezzo i bordi vanno avanti.
        let stretta: Vec<[f32; 2]> = (0..=40).map(|i| (i as f32 * 0.1).sin_cos()).map(|(s, c)| [c, s]).collect();
        let fatti = pezzi(&stretta, &[4.0; 41]);
        assert!(fatti.len() > 1);
        assert!(fatti.iter().all(bordi_avanti));
        // Anche col contorno: tutti i pezzi girano nello stesso verso.
        let versi: Vec<f32> = fatti.iter().map(|p| area(&p.contorno(8)).signum()).collect();
        assert!(versi.iter().all(|v| *v == versi[0]), "{versi:?}");
    }

    #[test]
    fn la_semplificazione_toglie_i_punti_che_non_si_vedono() {
        let quasi: Vec<[f32; 2]> = (0..=100).map(|i| [i as f32, 0.001 * (i % 2) as f32]).collect();
        assert_eq!(semplifica(&quasi, 0.01), vec![[0.0, 0.0], [100.0, 0.0]]);
        let angolo = [[0.0, 0.0], [5.0, 0.0], [10.0, 0.0], [10.0, 5.0], [10.0, 10.0]];
        assert_eq!(semplifica(&angolo, 0.01), vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]]);
        assert_eq!(semplifica(&angolo[..2], 0.01), angolo[..2].to_vec());
    }
}
