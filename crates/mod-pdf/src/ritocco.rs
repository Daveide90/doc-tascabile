//! Riscrivere una riga di una scansione nei pixel della sua foto: le parole cambiate si
//! ridisegnano col carattere più simile a quello stampato, alla sua misura, sulla sua linea di
//! base e col colore del suo inchiostro; le parole rimaste sono i pixel di prima, spostati solo se
//! serve a far posto. Il resto della pagina non si tocca.
//!
//! I riquadri delle parole vengono dallo strato dell'OCR, che non sempre stringe l'inchiostro:
//! prima si cercano le lettere vere nella foto ([`affina_riga`]).

use image::RgbImage;

use crate::caratteri::{Carattere, Disegno, caratteri};

/// Una parola di una riga della foto: il testo e il riquadro nei pixel [x0, y0, x1, y1] (x1 e y1
/// esclusi).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParolaFoto {
    pub(crate) testo: String,
    pub(crate) r: [u32; 4],
}

/// Una parola della riga corretta: una di prima (il suo numero nella riga) o una nuova.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Voce {
    Tenuta(usize),
    Nuova(String),
}

/// Perché una riga non si riscrive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Intoppo {
    /// Il testo nuovo non entra nello spazio della riga.
    NonSta,
    /// Nella foto non si trovano le lettere della riga (troppo chiare, o niente sotto il testo).
    SenzaLettere,
}

/// L'inchiostro e la carta di una riga.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Inchiostro {
    pub(crate) carta: [u8; 3],
    pub(crate) colore: [u8; 3],
    luce_carta: f64,
    luce_inchiostro: f64,
}

impl Inchiostro {
    fn soglia(&self) -> f64 {
        (self.luce_carta + self.luce_inchiostro) / 2.0
    }

    /// Quanto è scuro il pixel `p`: 0 la carta, 1 l'inchiostro pieno.
    fn scuro(&self, p: [u8; 3]) -> f32 {
        ((self.luce_carta - luce(p)) / (self.luce_carta - self.luce_inchiostro)).clamp(0.0, 1.0) as f32
    }
}

fn luce(p: [u8; 3]) -> f64 {
    (299.0 * f64::from(p[0]) + 587.0 * f64::from(p[1]) + 114.0 * f64::from(p[2])) / 1000.0
}

/// La differenza minima di luce fra carta e inchiostro perché le lettere si riconoscano.
const CONTRASTO_MINIMO: f64 = 40.0;

/// L'inchiostro di una riga dai suoi riquadri: la carta intorno, il pixel più scuro (il 3° percentile,
/// contro i puntini di rumore) e il colore delle lettere (la metà più scura dei pixel d'inchiostro).
fn inchiostro_della_riga(foto: &RgbImage, riquadri: &[[u32; 4]]) -> Option<Inchiostro> {
    let carta = crate::correzione::colore_della_carta(foto, riquadri);
    let (w, h) = foto.dimensions();
    let mut pixel: Vec<[u8; 3]> = Vec::new();
    for r in riquadri {
        for y in r[1]..r[3].min(h) {
            for x in r[0]..r[2].min(w) {
                pixel.push(foto.get_pixel(x, y).0);
            }
        }
    }
    if pixel.is_empty() {
        return None;
    }
    pixel.sort_by(|a, b| luce(*a).total_cmp(&luce(*b)));
    let luce_carta = luce(carta);
    let luce_inchiostro = luce(pixel[pixel.len() * 3 / 100]);
    if luce_carta - luce_inchiostro < CONTRASTO_MINIMO {
        return None;
    }
    let soglia = (luce_carta + luce_inchiostro) / 2.0;
    let scuri: Vec<[u8; 3]> = pixel.into_iter().take_while(|p| luce(*p) < soglia).collect();
    let meta = &scuri[..scuri.len().div_ceil(2)];
    let mut somma = [0u64; 3];
    for p in meta {
        for (s, v) in somma.iter_mut().zip(p) {
            *s += u64::from(*v);
        }
    }
    let colore = somma.map(|s| (s / meta.len().max(1) as u64) as u8);
    Some(Inchiostro { carta, colore, luce_carta, luce_inchiostro })
}

/// I riquadri dell'inchiostro delle parole di una riga, dai riquadri dell'OCR, e l'inchiostro della
/// riga. None se nella foto le lettere non si vedono.
pub(crate) fn affina_riga(foto: &RgbImage, parole: &[ParolaFoto]) -> Option<(Vec<ParolaFoto>, Inchiostro)> {
    let riquadri: Vec<[u32; 4]> = parole.iter().map(|p| p.r).collect();
    let inchiostro = inchiostro_della_riga(foto, &riquadri)?;
    let w = foto.width();
    let affinate = parole
        .iter()
        .enumerate()
        .map(|(i, p)| {
            // Non oltre la metà dello spazio fra questa parola e le vicine.
            let sinistra = if i > 0 { (parole[i - 1].r[2] + p.r[0]) / 2 } else { 0 };
            let destra = parole.get(i + 1).map_or(w, |q| (p.r[2] + q.r[0]).div_ceil(2));
            ParolaFoto { testo: p.testo.clone(), r: affina(foto, inchiostro.soglia(), p.r, sinistra, destra) }
        })
        .collect();
    Some((per_spazi(foto, inchiostro.soglia(), parole, affinate), inchiostro))
}

/// Le parole dagli spazi bianchi della riga: nella fascia della riga le colonne con inchiostro
/// fanno dei pezzi, uniti quando sono separati meno di un quinto dell'altezza (lo spazio fra due
/// lettere); ogni pezzo va alla parola dell'OCR il cui mezzo gli sta più vicino, fra quelle che
/// tocca. Così una parola che l'OCR ha messo un po' più in là (segnalazione del 3/11: la M di
/// «MEDICA» restava mezza nella foto) si prende tutta. Una parola senza pezzi, o un pezzo che
/// tiene due parole, resta com'era.
fn per_spazi(foto: &RgbImage, soglia: f64, ocr: &[ParolaFoto], mut affinate: Vec<ParolaFoto>) -> Vec<ParolaFoto> {
    let (w, _) = foto.dimensions();
    let (Some(su), Some(giu)) = (affinate.iter().map(|p| p.r[1]).min(), affinate.iter().map(|p| p.r[3]).max()) else { return affinate };
    let mut altezze: Vec<u32> = affinate.iter().map(|p| p.r[3].saturating_sub(p.r[1])).collect();
    altezze.sort_unstable();
    let alto = altezze[altezze.len() / 2].max(4);
    let da = ocr.iter().chain(&affinate).map(|p| p.r[0]).min().unwrap_or(0).saturating_sub(alto);
    let a = (ocr.iter().chain(&affinate).map(|p| p.r[2]).max().unwrap_or(w) + alto).min(w);
    let scuro = |x: u32| (su..giu).any(|y| luce(foto.get_pixel(x, y).0) < soglia);
    // I pezzi d'inchiostro, uniti sopra gli spazi fra le lettere.
    let unisci = (alto / 5).max(1);
    let mut pezzi: Vec<[u32; 2]> = Vec::new();
    for x in da..a {
        if !scuro(x) {
            continue;
        }
        match pezzi.last_mut() {
            Some(p) if x - p[1] <= unisci => p[1] = x + 1,
            _ => pezzi.push([x, x + 1]),
        }
    }
    let mut di_chi: Vec<Vec<[u32; 2]>> = vec![Vec::new(); ocr.len()];
    let mut doppi = vec![false; ocr.len()];
    for p in &pezzi {
        let tocca = |r: [u32; 4]| p[0] < r[2] + alto / 2 && r[0] < p[1] + alto / 2;
        let toccate: Vec<usize> = (0..ocr.len()).filter(|i| tocca(ocr[*i].r) || tocca(affinate[*i].r)).collect();
        let mezzo = f64::from(p[0] + p[1]) / 2.0;
        let Some(&chi) = toccate.iter().min_by(|i, j| {
            let d = |k: usize| (f64::from(ocr[k].r[0] + ocr[k].r[2]) / 2.0 - mezzo).abs();
            d(**i).total_cmp(&d(**j))
        }) else {
            continue;
        };
        // Un pezzo che copre il mezzo di due parole le tiene tutte e due: non si sa dove dividerle.
        let coperte = toccate.iter().filter(|k| {
            let m = (ocr[**k].r[0] + ocr[**k].r[2]) / 2;
            m >= p[0] && m < p[1]
        });
        if coperte.count() > 1 {
            for k in toccate {
                doppi[k] = true;
            }
            continue;
        }
        di_chi[chi].push(*p);
    }
    for (i, pezzi) in di_chi.iter().enumerate() {
        if let (false, Some(x0), Some(x1)) = (doppi[i], pezzi.iter().map(|p| p[0]).min(), pezzi.iter().map(|p| p[1]).max()) {
            affinate[i].r[0] = x0;
            affinate[i].r[2] = x1;
        }
    }
    affinate
}

/// Il riquadro dell'inchiostro della parola che l'OCR ha messo in `r`: si cerca un po' fuori dal
/// riquadro (un terzo dell'altezza sopra e sotto, un quarto ai lati, fra `sinistra` e `destra`),
/// tenendo le righe di pixel con inchiostro attaccate a quella di mezzo (con buchi di un ottavo
/// dell'altezza: il punto della i). Senza inchiostro, il riquadro dell'OCR.
fn affina(foto: &RgbImage, soglia: f64, r: [u32; 4], sinistra: u32, destra: u32) -> [u32; 4] {
    let (w, h) = foto.dimensions();
    let alto = r[3].saturating_sub(r[1]).max(1);
    let (mx, my) = (alto / 4, (alto / 3).max(1));
    let (x0, x1) = (r[0].saturating_sub(mx).max(sinistra), (r[2] + mx).min(destra).min(w));
    let (y0, y1) = (r[1].saturating_sub(my), (r[3] + my).min(h));
    if x1 <= x0 || y1 <= y0 {
        return r;
    }
    let scuro = |x: u32, y: u32| luce(foto.get_pixel(x, y).0) < soglia;
    let con_inchiostro: Vec<bool> = (y0..y1).map(|y| (x0..x1).any(|x| scuro(x, y))).collect();
    // La riga di pixel da cui partire: quella di mezzo, o la più vicina con inchiostro dentro il
    // riquadro dell'OCR.
    let mezzo = ((r[1] + r[3]) / 2).clamp(y0, y1 - 1);
    let dentro = |y: u32| y >= r[1].max(y0) && y < r[3].min(y1);
    let Some(partenza) = (0..(y1 - y0)).flat_map(|d| [mezzo.checked_sub(d), mezzo.checked_add(d)]).flatten().find(|y| dentro(*y) && con_inchiostro[(*y - y0) as usize])
    else {
        return r;
    };
    let buco = (alto / 8).max(1);
    let (mut su, mut giu) = (partenza, partenza);
    let mut vuote = 0;
    for y in (y0..partenza).rev() {
        if con_inchiostro[(y - y0) as usize] {
            su = y;
            vuote = 0;
        } else {
            vuote += 1;
            if vuote > buco {
                break;
            }
        }
    }
    vuote = 0;
    for y in partenza + 1..y1 {
        if con_inchiostro[(y - y0) as usize] {
            giu = y;
            vuote = 0;
        } else {
            vuote += 1;
            if vuote > buco {
                break;
            }
        }
    }
    let colonne: Vec<u32> = (x0..x1).filter(|x| (su..=giu).any(|y| scuro(*x, y))).collect();
    match (colonne.first(), colonne.last()) {
        (Some(a), Some(b)) => [*a, su, b + 1, giu + 1],
        _ => r,
    }
}

/// Come una parola della foto somiglia a un carattere.
#[derive(Debug, Clone, Copy)]
struct Confronto {
    /// 0 uguale, 1 niente in comune.
    errore: f64,
    /// L'em (pixel) a cui il carattere fa la parola alta come nella foto.
    em: f64,
    /// La linea di base nella foto (pixel).
    base: f64,
}

/// Quanto è scuro ogni pixel della parola `p` nella foto (0 la carta, 1 l'inchiostro), riga per
/// riga, e la sua misura. None per una parola troppo piccola o senza testo.
fn osservata(foto: &RgbImage, ink: &Inchiostro, p: &ParolaFoto) -> Option<(Vec<f32>, usize, usize)> {
    let (wo, ho) = ((p.r[2] - p.r[0]) as usize, (p.r[3] - p.r[1]) as usize);
    if ho < 4 || wo < 2 || p.testo.trim().is_empty() {
        return None;
    }
    let pixel = (0..ho).flat_map(|y| (0..wo).map(move |x| (x, y))).map(|(x, y)| ink.scuro(foto.get_pixel(p.r[0] + x as u32, p.r[1] + y as u32).0)).collect();
    Some((pixel, wo, ho))
}

/// L'em che fa la parola `testo` di `c` grande come nella foto (`wo` × `ho` pixel d'inchiostro), in
/// altezza e in larghezza insieme (in pixel interi una misura sola sbaglia di un poco),
/// ricontrollato una volta.
fn em_della_parola(c: &Carattere, testo: &str, wo: usize, ho: usize) -> Option<f64> {
    let mut em = ho as f64 * 1.4;
    for _ in 0..2 {
        let r = c.disegna(testo, em, 1.0).inchiostro?;
        em *= ((wo as f64 / (r[2] - r[0]).max(1) as f64) * (ho as f64 / (r[3] - r[1]).max(1) as f64)).sqrt();
    }
    Some(em)
}

/// La parola `p` della foto contro `c`: disegnata alta come lei, con l'inchiostro allineato in alto
/// a sinistra (a meno di un pixel), la differenza pixel per pixel su quella di tutti e due.
fn confronta(foto: &RgbImage, ink: &Inchiostro, p: &ParolaFoto, c: &Carattere) -> Option<Confronto> {
    let (osservato, wo, ho) = osservata(foto, ink, p)?;
    let em = em_della_parola(c, &p.testo, wo, ho)?;
    let d = c.disegna(&p.testo, em, 1.0);
    let rr = d.inchiostro?;
    let migliore = differenza(&osservato, wo, ho, &d, rr);
    // La base dall'alto e dal basso dell'inchiostro, a metà: l'errore di un pixel si dimezza.
    let base = (f64::from(p.r[1]) + d.penna[1] - rr[1] as f64 + f64::from(p.r[3]) - (rr[3] as f64 - d.penna[1])) / 2.0;
    migliore.is_finite().then_some(Confronto { errore: migliore, em, base })
}

/// La differenza fra la parola della foto (`osservato`, `wo` × `ho`) e il disegno `d` col suo
/// inchiostro in `rr`, allineati in alto a sinistra a meno di un pixel: 0 uguali, 1 niente in
/// comune.
fn differenza(osservato: &[f32], wo: usize, ho: usize, d: &Disegno, rr: [usize; 4]) -> f64 {
    let (wr, hr) = (rr[2] - rr[0], rr[3] - rr[1]);
    let (largo, alto) = (wo.max(wr) + 2, ho.max(hr) + 2);
    let mut migliore = f64::INFINITY;
    for sy in -1i64..=1 {
        for sx in -1i64..=1 {
            let (mut diversi, mut tutti) = (0.0f64, 0.0f64);
            for v in 0..alto {
                for u in 0..largo {
                    let o = if u < wo && v < ho { osservato[v * wo + u] } else { 0.0 };
                    let (x, y) = (rr[0] as i64 + u as i64 + sx, rr[1] as i64 + v as i64 + sy);
                    let k = if x >= 0 && y >= 0 { d.a(x as usize, y as usize) } else { 0.0 };
                    diversi += f64::from((o - k).abs());
                    tutti += f64::from(o + k);
                }
            }
            if tutti > 0.0 {
                migliore = migliore.min(diversi / tutti);
            }
        }
    }
    migliore
}

/// Le sfocature fra cui si sceglie quella della scansione (deviazione del filtro, pixel).
const SFOCATURE: [f32; 6] = [0.0, 0.4, 0.7, 1.0, 1.4, 1.9];

/// Il disegno `d` sfocato come una foto: un filtro gaussiano di deviazione `sigma` pixel, prima per
/// righe e poi per colonne. Stessa misura (il margine del disegno tiene la sfumatura).
pub(crate) fn sfoca(d: &Disegno, sigma: f32) -> Disegno {
    if sigma <= 0.0 || d.largo == 0 {
        return d.clone();
    }
    let raggio = (2.5 * sigma).ceil() as i64;
    let pesi: Vec<f32> = (-raggio..=raggio).map(|i| (-((i * i) as f32) / (2.0 * sigma * sigma)).exp()).collect();
    let somma: f32 = pesi.iter().sum();
    let pesi: Vec<f32> = pesi.iter().map(|p| p / somma).collect();
    let (w, h) = (d.largo as i64, d.alto as i64);
    let passata = |sorgente: &[f32], orizzontale: bool| -> Vec<f32> {
        let mut fuori = vec![0f32; sorgente.len()];
        for y in 0..h {
            for x in 0..w {
                let mut v = 0.0;
                for (k, p) in pesi.iter().enumerate() {
                    let o = k as i64 - raggio;
                    let (sx, sy) = if orizzontale { (x + o, y) } else { (x, y + o) };
                    if sx >= 0 && sy >= 0 && sx < w && sy < h {
                        v += p * sorgente[(sy * w + sx) as usize];
                    }
                }
                fuori[(y * w + x) as usize] = v;
            }
        }
        fuori
    };
    let copertura = passata(&passata(&d.copertura, true), false);
    Disegno { copertura, ..d.clone() }
}

/// La sfocatura della scansione, dalle parole `campioni` col carattere `c`: quella fra
/// [`SFOCATURE`] che rende il disegno più simile alla foto. Le lettere nuove così non sono più
/// nette delle altre (segnalazione del 3/11: «un font diverso e più definito»).
fn sfocatura(foto: &RgbImage, ink: &Inchiostro, campioni: &[&ParolaFoto], c: &Carattere) -> f32 {
    let mut errori = [0.0f64; SFOCATURE.len()];
    let mut contate = 0;
    for p in campioni.iter().take(5) {
        let Some((osservato, wo, ho)) = osservata(foto, ink, p) else { continue };
        let Some(em) = em_della_parola(c, &p.testo, wo, ho) else { continue };
        let d = c.disegna(&p.testo, em, 1.0);
        let Some(rr) = d.inchiostro else { continue };
        for (e, s) in errori.iter_mut().zip(SFOCATURE) {
            *e += differenza(&osservato, wo, ho, &sfoca(&d, s), rr);
        }
        contate += 1;
    }
    if contate == 0 {
        return 0.0;
    }
    SFOCATURE.iter().zip(errori).min_by(|a, b| a.1.total_cmp(&b.1)).map_or(0.0, |(s, _)| *s)
}

/// Il carattere che somiglia di più alle parole `campioni` (con il loro peso) e l'em della riga.
fn scegli(foto: &RgbImage, ink: &Inchiostro, campioni: &[(&ParolaFoto, f64)]) -> Option<(&'static Carattere, f64)> {
    let mut migliore: Option<(&'static Carattere, f64, f64)> = None;
    for c in caratteri() {
        let (mut errore, mut pesi, mut em) = (0.0, 0.0, Vec::new());
        for (p, peso) in campioni {
            let Some(conf) = confronta(foto, ink, p, c) else { continue };
            errore += peso * conf.errore;
            pesi += peso;
            em.push(conf.em);
        }
        if pesi <= 0.0 {
            continue;
        }
        let errore = errore / pesi;
        em.sort_by(f64::total_cmp);
        let em = em[em.len() / 2];
        if migliore.is_none_or(|(_, e, _)| errore < e) {
            migliore = Some((c, errore, em));
        }
    }
    migliore.map(|(c, _, em)| (c, em))
}

/// Riscrive nella foto la riga `vecchie` (le parole di prima, coi riquadri di [`affina_riga`])
/// come dicono `nuove`. Si ridisegna dal primo punto che cambia: le parole rimaste dopo si
/// spostano coi loro pixel, tenendo gli spazi di prima fra loro; quelle nuove si disegnano. La
/// riga non va oltre `destra` (pixel): se serve, gli spazi dopo il cambio si stringono fino alla
/// metà di uno spazio normale, poi [`Intoppo::NonSta`]. `fascia`: il fondo della riga sopra e
/// l'alto di quella sotto (pixel), che la pulizia non tocca. Le parole della riga dopo.
pub(crate) fn riscrivi_riga(
    foto: &mut RgbImage,
    vecchie: &[ParolaFoto],
    ink: &Inchiostro,
    nuove: &[Voce],
    destra: u32,
    fascia: (u32, u32),
) -> Result<Vec<ParolaFoto>, Intoppo> {
    let primo = (0..nuove.len()).find(|k| nuove[*k] != Voce::Tenuta(*k)).unwrap_or(nuove.len());
    if primo == nuove.len() && nuove.len() == vecchie.len() {
        return Ok(vecchie.to_vec());
    }
    let ultima = vecchie.last().ok_or(Intoppo::SenzaLettere)?;
    // Il carattere dalle parole tolte (contano di più) e dalle due rimaste più vicine per parte.
    let tenute: Vec<usize> = nuove.iter().filter_map(|v| if let Voce::Tenuta(j) = v { Some(*j) } else { None }).collect();
    let tolte: Vec<usize> = (0..vecchie.len()).filter(|j| !tenute.contains(j)).collect();
    let (da, a) = match (tolte.first(), tolte.last()) {
        (Some(x), Some(y)) => (*x, *y + 1),
        _ => (primo.min(vecchie.len()), primo.min(vecchie.len())),
    };
    let mut campioni: Vec<(&ParolaFoto, f64)> = tolte.iter().map(|j| (&vecchie[*j], 3.0)).collect();
    campioni.extend((da.saturating_sub(2)..da).chain(a..(a + 2).min(vecchie.len())).filter(|j| tenute.contains(j)).map(|j| (&vecchie[j], 1.0)));
    if campioni.is_empty() {
        campioni = vecchie.iter().map(|p| (p, 1.0)).collect();
    }
    let (carattere, em) = scegli(foto, ink, &campioni).ok_or(Intoppo::SenzaLettere)?;
    // La linea di base: la mediana di quelle delle parole della riga, col carattere scelto.
    let mut basi: Vec<f64> = vecchie.iter().filter_map(|p| confronta(foto, ink, p, carattere)).map(|c| c.base).collect();
    if basi.is_empty() {
        return Err(Intoppo::SenzaLettere);
    }
    basi.sort_by(f64::total_cmp);
    let base = basi[basi.len() / 2];
    // I margini ai lati dell'inchiostro di una parola, come li disegna il carattere scelto: fra due
    // parole c'è lo spazio della penna più il margine destro della prima e il sinistro della
    // seconda. Così una parola nuova parte dalla penna di quella che sostituisce.
    let lati_di = |d: &Disegno| d.inchiostro.map_or((0.0, d.avanza), |r| (r[0] as f64 - d.penna[0], d.penna[0] + d.avanza - r[2] as f64));
    let lati_vecchie: Vec<(f64, f64)> = vecchie.iter().map(|p| lati_di(&carattere.disegna(&p.testo, em, 1.0))).collect();
    // Lo spazio della penna fra le parole: il più comune della riga, o quello del carattere.
    let mut spazi: Vec<f64> =
        vecchie.windows(2).zip(lati_vecchie.windows(2)).map(|(w, l)| f64::from(w[1].r[0].saturating_sub(w[0].r[2])) - l[0].1 - l[1].0).collect();
    spazi.sort_by(f64::total_cmp);
    let spazio = spazi.get(spazi.len() / 2).copied().unwrap_or_else(|| carattere.disegna(" ", em, 1.0).avanza).max(em * 0.1);
    // Le parole nuove disegnate, sfocate come la scansione.
    let sigma = sfocatura(foto, ink, &campioni.iter().map(|(p, _)| *p).collect::<Vec<_>>(), carattere);
    let disegni: Vec<Option<Disegno>> =
        nuove.iter().map(|v| if let Voce::Nuova(t) = v { Some(sfoca(&carattere.disegna(t, em, 1.0), sigma)) } else { None }).collect();
    let lati = |k: usize| match (&nuove[k], &disegni[k]) {
        (Voce::Tenuta(j), _) => lati_vecchie[*j],
        (_, Some(d)) => lati_di(d),
        _ => (0.0, 0.0),
    };
    let larghezza = |k: usize| match (&nuove[k], &disegni[k]) {
        (Voce::Tenuta(j), _) => f64::from(vecchie[*j].r[2] - vecchie[*j].r[0]),
        (_, Some(d)) => d.inchiostro.map_or(0.0, |r| (r[2] - r[0]) as f64),
        _ => 0.0,
    };
    // La penna da cui riparte la riga: quella della prima parola cambiata, o dopo l'ultima. Prima
    // di ogni parola, lo spazio d'inchiostro da quella di prima (o dalla penna): fra due parole
    // rimaste vicine quello di prima.
    let inizio = match vecchie.get(primo) {
        Some(p) => f64::from(p.r[0]) - lati_vecchie[primo].0,
        None => f64::from(ultima.r[2]) + lati_vecchie.last().map_or(0.0, |l| l.1) + spazio,
    };
    let mut spazi_dopo: Vec<f64> = (primo..nuove.len())
        .map(|k| match (k.checked_sub(1).map(|i| &nuove[i]), &nuove[k]) {
            _ if k == primo => lati(k).0,
            (Some(Voce::Tenuta(x)), Voce::Tenuta(y)) if *y == x + 1 => f64::from(vecchie[*y].r[0].saturating_sub(vecchie[*x].r[2])),
            _ => lati(k - 1).1 + spazio + lati(k).0,
        })
        .collect();
    let fine = |spazi: &[f64]| inizio + (primo..nuove.len()).zip(spazi).map(|(k, s)| s + larghezza(k)).sum::<f64>();
    let troppo = fine(&spazi_dopo) - f64::from(destra);
    if troppo > 0.0 {
        let stringibile: f64 = spazi_dopo.iter().skip(1).map(|s| (s - spazio * 0.5).max(0.0)).sum();
        if stringibile < troppo {
            return Err(Intoppo::NonSta);
        }
        for s in spazi_dopo.iter_mut().skip(1) {
            *s -= (*s - spazio * 0.5).max(0.0) * troppo / stringibile;
        }
    }
    // Dove va ogni parola, da sinistra.
    let mut x = inizio;
    let mut posti = Vec::new();
    for (k, s) in (primo..nuove.len()).zip(&spazi_dopo) {
        x += s;
        posti.push((k, x.round() as i64));
        x += larghezza(k);
    }
    let fine_nuova = x.ceil() as i64;
    // La fascia da ripulire: dall'alto al basso delle parole, vecchie e nuove, con un margine,
    // senza toccare le righe vicine.
    let (w, h) = foto.dimensions();
    let mut alto = vecchie.iter().map(|p| i64::from(p.r[1])).min().unwrap_or(0);
    let mut basso = vecchie.iter().map(|p| i64::from(p.r[3])).max().unwrap_or(0);
    for (k, _) in &posti {
        if let Some(Some(d)) = disegni.get(*k)
            && let Some(r) = d.inchiostro
        {
            alto = alto.min((base - (d.penna[1] - r[1] as f64)).floor() as i64);
            basso = basso.max((base + (r[3] as f64 - d.penna[1])).ceil() as i64);
        }
    }
    let alto = (alto - 2).max(i64::from(fascia.0) + 1).max(0) as u32;
    let basso = (basso + 2).min(i64::from(fascia.1)).min(i64::from(h)) as u32;
    // I pixel delle parole che restano, prima di ripulire.
    let ritagli: Vec<(usize, i64, RgbImage)> = posti
        .iter()
        .filter_map(|(k, x)| if let Voce::Tenuta(j) = nuove[*k] { Some((j, *x)) } else { None })
        .map(|(j, x)| {
            let r = vecchie[j].r;
            let x0 = r[0].saturating_sub(1);
            let x1 = (r[2] + 1).min(w);
            (j, x - i64::from(r[0] - x0), image::imageops::crop_imm(foto, x0, alto, x1 - x0, basso.saturating_sub(alto)).to_image())
        })
        .collect();
    // Si ripulisce dal primo cambio alla fine, la vecchia o la nuova.
    let prima_fine = vecchie.get(primo.wrapping_sub(1)).filter(|_| primo > 0).map_or(0, |p| i64::from(p.r[2]));
    let primo_inchiostro = posti.first().map_or(i64::MAX, |(_, x)| *x).min(vecchie.get(primo).map_or(i64::MAX, |p| i64::from(p.r[0])));
    let da_x = (primo_inchiostro.min(fine_nuova) - 1).max(prima_fine).max(0);
    let a_x = (vecchie.iter().skip(primo).map(|p| i64::from(p.r[2])).max().unwrap_or(0).max(fine_nuova) + 2).min(i64::from(w));
    for y in alto..basso {
        for x in da_x..a_x {
            foto.put_pixel(x as u32, y, image::Rgb(ink.carta));
        }
    }
    for (_, x, ritaglio) in &ritagli {
        for (u, v, p) in ritaglio.enumerate_pixels() {
            let x = x + i64::from(u);
            if x >= 0 && x < i64::from(w) {
                foto.put_pixel(x as u32, alto + v, *p);
            }
        }
    }
    // Le parole nuove, sfumate sulla carta come le lettere della foto.
    let mut finali: Vec<ParolaFoto> = vecchie[..primo.min(vecchie.len())].to_vec();
    for (k, x) in &posti {
        match (&nuove[*k], &disegni[*k]) {
            (Voce::Tenuta(j), _) => {
                let r = vecchie[*j].r;
                let dx = *x - i64::from(r[0]);
                finali.push(ParolaFoto { testo: vecchie[*j].testo.clone(), r: [(i64::from(r[0]) + dx) as u32, r[1], (i64::from(r[2]) + dx) as u32, r[3]] });
            }
            (Voce::Nuova(t), Some(d)) => {
                let Some(ri) = d.inchiostro else { continue };
                let ox = *x - ri[0] as i64;
                let oy = (base - d.penna[1]).round() as i64;
                for v in 0..d.alto {
                    for u in 0..d.largo {
                        let c = d.a(u, v);
                        let (px, py) = (ox + u as i64, oy + v as i64);
                        if c <= 0.0 || px < 0 || py < 0 || px >= i64::from(w) || py >= i64::from(h) {
                            continue;
                        }
                        let q = foto.get_pixel(px as u32, py as u32).0;
                        let m = |a: u8, b: u8| (f32::from(a) * (1.0 - c) + f32::from(b) * c).round() as u8;
                        foto.put_pixel(px as u32, py as u32, image::Rgb([m(q[0], ink.colore[0]), m(q[1], ink.colore[1]), m(q[2], ink.colore[2])]));
                    }
                }
                let r = [ox + ri[0] as i64, oy + ri[1] as i64, ox + ri[2] as i64, oy + ri[3] as i64].map(|v| v.clamp(0, i64::from(w.max(h))) as u32);
                finali.push(ParolaFoto { testo: t.clone(), r });
            }
            _ => {}
        }
    }
    Ok(finali)
}

#[cfg(test)]
pub(crate) mod test {
    use super::*;

    /// Una riga di parole scritta con `c` su carta avorio, inchiostro grigio scuro come una foto:
    /// la foto e le parole coi loro riquadri d'inchiostro esatti.
    pub(crate) fn riga_scritta(c: &Carattere, parole: &[&str], em: f64) -> (RgbImage, Vec<ParolaFoto>) {
        riga_scritta_sfocata(c, parole, em, 0.0)
    }

    /// Come [`riga_scritta`], con le lettere sfocate di `sigma` pixel come in una foto.
    pub(crate) fn riga_scritta_sfocata(c: &Carattere, parole: &[&str], em: f64, sigma: f32) -> (RgbImage, Vec<ParolaFoto>) {
        let carta = [245u8, 237, 219];
        let colore = [40u8, 38, 45];
        let mut foto = RgbImage::from_pixel(900, 160, image::Rgb(carta));
        let base = 90.0;
        let mut x = 30.0;
        let spazio = c.disegna(" ", em, 1.0).avanza;
        let mut fatte = Vec::new();
        for p in parole {
            let netta = c.disegna(p, em, 1.0);
            let r = netta.inchiostro.unwrap();
            let d = sfoca(&netta, sigma);
            let (ox, oy) = ((x - d.penna[0]).round() as i64, (base - d.penna[1]).round() as i64);
            for v in 0..d.alto {
                for u in 0..d.largo {
                    let k = d.a(u, v);
                    let (px, py) = ((ox + u as i64) as u32, (oy + v as i64) as u32);
                    let m = |a: u8, b: u8| (f32::from(a) * (1.0 - k) + f32::from(b) * k).round() as u8;
                    foto.put_pixel(px, py, image::Rgb([m(carta[0], colore[0]), m(carta[1], colore[1]), m(carta[2], colore[2])]));
                }
            }
            fatte.push(ParolaFoto { testo: p.to_string(), r: [(ox + r[0] as i64) as u32, (oy + r[1] as i64) as u32, (ox + r[2] as i64) as u32, (oy + r[3] as i64) as u32] });
            x += d.avanza + spazio;
        }
        (foto, fatte)
    }

    fn cerca(famiglia: &str, grassetto: bool, corsivo: bool) -> &'static Carattere {
        caratteri().iter().find(|c| c.famiglia == famiglia && c.grassetto == grassetto && c.corsivo == corsivo).unwrap()
    }

    /// I riquadri dell'OCR, larghi e spostati come li dà un riconoscitore.
    fn come_l_ocr(parole: &[ParolaFoto]) -> Vec<ParolaFoto> {
        parole.iter().map(|p| ParolaFoto { testo: p.testo.clone(), r: [p.r[0] + 2, p.r[1].saturating_sub(5), p.r[2] + 3, p.r[3] + 4] }).collect()
    }

    #[test]
    fn i_riquadri_dell_ocr_si_stringono_sull_inchiostro() {
        let (foto, vere) = riga_scritta(cerca("Carlito", true, false), &["Area", "lavoro/istruzione", "di"], 40.0);
        let (affinate, ink) = affina_riga(&foto, &come_l_ocr(&vere)).unwrap();
        for (a, v) in affinate.iter().zip(&vere) {
            assert!(a.r.iter().zip(v.r).all(|(x, y)| x.abs_diff(y) <= 1), "{a:?} {v:?}");
        }
        assert!(ink.colore.iter().zip([40u8, 38, 45]).all(|(a, b)| a.abs_diff(b) <= 12), "{:?}", ink.colore);
        assert!(ink.carta.iter().zip([245u8, 237, 219]).all(|(a, b)| a.abs_diff(b) <= 2), "{:?}", ink.carta);
        // Una foto senza lettere: niente.
        let bianca = RgbImage::from_pixel(100, 100, image::Rgb([250, 250, 250]));
        assert_eq!(affina_riga(&bianca, &[ParolaFoto { testo: "x".into(), r: [10, 10, 30, 30] }]), None);
    }

    #[test]
    fn si_riconosce_il_carattere_della_scansione() {
        for (famiglia, grassetto, corsivo) in [("Carlito", true, false), ("Liberation Serif", false, false), ("Liberation Sans", false, false), ("Carlito", false, true)] {
            let c = cerca(famiglia, grassetto, corsivo);
            let (foto, vere) = riga_scritta(c, &["Tempo", "libero/hobby", "Autostima"], 38.0);
            let (affinate, ink) = affina_riga(&foto, &come_l_ocr(&vere)).unwrap();
            let campioni: Vec<(&ParolaFoto, f64)> = affinate.iter().map(|p| (p, 1.0)).collect();
            let (scelto, em) = scegli(&foto, &ink, &campioni).unwrap();
            assert_eq!((scelto.famiglia, scelto.grassetto, scelto.corsivo), (famiglia, grassetto, corsivo));
            assert!((em - 38.0).abs() < 1.5, "{famiglia}: {em}");
        }
    }

    #[test]
    fn una_parola_cambiata_si_riscrive_nel_suo_carattere_e_il_resto_scorre() {
        let c = cerca("Carlito", true, false);
        let (mut foto, vere) = riga_scritta(c, &["Area", "lavoro/istruzione", "fine"], 40.0);
        let (affinate, ink) = affina_riga(&foto, &come_l_ocr(&vere)).unwrap();
        // «lavoro/istruzione» diventa «studio»: «fine» scorre a sinistra.
        let nuove = [Voce::Tenuta(0), Voce::Nuova("studio".into()), Voce::Tenuta(2)];
        let finali = riscrivi_riga(&mut foto, &affinate, &ink, &nuove, 880, (0, 160)).unwrap();
        let (attesa, attese) = riga_scritta(c, &["Area", "studio", "fine"], 40.0);
        assert_eq!(finali.iter().map(|p| p.testo.as_str()).collect::<Vec<_>>(), ["Area", "studio", "fine"]);
        for (f, a) in finali.iter().zip(&attese) {
            assert!(f.r.iter().zip(a.r).all(|(x, y)| x.abs_diff(y) <= 3), "{f:?} {a:?}");
            // I pixel della parola come nella riga scritta così fin dall'inizio, allineati
            // sull'inchiostro: stesso carattere, stessa misura, stesso colore.
            let (w, h) = (a.r[2] - a.r[0], a.r[3] - a.r[1]);
            let mut diversi = 0;
            for v in 0..h {
                for u in 0..w {
                    // Mezzo per cento di misura sposta i bordi di mezzo pixel: va bene un vicino.
                    let p = foto.get_pixel(f.r[0] + u, f.r[1] + v).0;
                    let vicino = (a.r[0] + u).saturating_sub(1)..=a.r[0] + u + 1;
                    if !vicino.into_iter().any(|x| attesa.get_pixel(x, a.r[1] + v).0.iter().zip(p).all(|(x, y)| x.abs_diff(y) <= 40)) {
                        diversi += 1;
                    }
                }
            }
            assert!(diversi * 20 < w * h, "{}: {diversi} pixel diversi su {}", f.testo, w * h);
        }
        // Dove finiva «istruzione» ora c'è la carta.
        let r = vere[1].r;
        let qui = foto.get_pixel(r[2] - 3, (r[1] + r[3]) / 2).0;
        assert!(qui.iter().zip([245u8, 237, 219]).all(|(a, b)| a.abs_diff(b) <= 2), "{qui:?}");
    }

    /// Il caso della «M» di «MEDICA» (3/11): l'OCR mette il riquadro di una parola mezza lettera
    /// più in là. Dagli spazi bianchi della riga la parola si prende tutta, e corretta non resta
    /// niente della lettera di prima.
    #[test]
    fn una_parola_spostata_dall_ocr_si_prende_tutta() {
        let c = cerca("Carlito", false, false);
        let (mut foto, vere) = riga_scritta(c, &["VISITA", "MEDICA", "ORE"], 40.0);
        let alto = vere[1].r[3] - vere[1].r[1];
        // Il riquadro di MEDICA comincia quasi a metà della M e finisce oltre la A.
        let mut ocr = vere.clone();
        ocr[1].r = [vere[1].r[0] + alto * 2 / 5, vere[1].r[1] + 3, vere[1].r[2] + alto / 3, vere[1].r[3] - 2];
        let (affinate, ink) = affina_riga(&foto, &ocr).unwrap();
        for (a, v) in affinate.iter().zip(&vere) {
            assert!(a.r[0].abs_diff(v.r[0]) <= 1 && a.r[2].abs_diff(v.r[2]) <= 1, "{a:?} invece di {v:?}");
        }
        let nuove = [Voce::Tenuta(0), Voce::Nuova("CLINICA".into()), Voce::Tenuta(2)];
        riscrivi_riga(&mut foto, &affinate, &ink, &nuove, 880, (0, 160)).unwrap();
        // Dove stava la metà sinistra della M c'è quello che c'è nella riga scritta così fin
        // dall'inizio (l'inizio della C), a meno di un pixel: niente asta della M.
        let (attesa, _) = riga_scritta(c, &["VISITA", "CLINICA", "ORE"], 40.0);
        let (x0, x1) = (vere[1].r[0] - 2, vere[1].r[0] + alto / 2);
        let mut diversi = 0;
        for y in vere[1].r[1]..vere[1].r[3] {
            for x in x0..x1 {
                let p = foto.get_pixel(x, y).0;
                if !(x - 1..=x + 1).any(|x| attesa.get_pixel(x, y).0.iter().zip(p).all(|(a, b)| a.abs_diff(b) <= 50)) {
                    diversi += 1;
                }
            }
        }
        assert!(diversi * 20 < (x1 - x0) * alto, "resta la M di prima: {diversi} pixel diversi");
    }

    /// Le lettere nuove sono sfocate quanto quelle della scansione: non più nette.
    #[test]
    fn le_lettere_nuove_sono_sfocate_come_la_scansione() {
        let c = cerca("Liberation Serif", false, false);
        let (foto, vere) = riga_scritta_sfocata(c, &["Lettera", "di", "dimissione"], 36.0, 1.0);
        let (affinate, ink) = affina_riga(&foto, &vere).unwrap();
        let campioni: Vec<&ParolaFoto> = affinate.iter().collect();
        let s = sfocatura(&foto, &ink, &campioni, c);
        assert!((0.7..=1.4).contains(&s), "sfocatura {s}");
        // Una foto netta: niente sfocatura.
        let (netta, vere) = riga_scritta(c, &["Lettera", "di", "dimissione"], 36.0);
        let (affinate, ink) = affina_riga(&netta, &vere).unwrap();
        let campioni: Vec<&ParolaFoto> = affinate.iter().collect();
        assert!(sfocatura(&netta, &ink, &campioni, c) <= 0.4);
    }

    #[test]
    fn un_testo_troppo_lungo_non_sta() {
        let c = cerca("Liberation Sans", false, false);
        let (mut foto, vere) = riga_scritta(c, &["Uno", "due"], 40.0);
        let prima = foto.clone();
        let (affinate, ink) = affina_riga(&foto, &vere).unwrap();
        let nuove = [Voce::Tenuta(0), Voce::Nuova("lunghissimamente".into()), Voce::Tenuta(1)];
        assert_eq!(riscrivi_riga(&mut foto, &affinate, &ink, &nuove, 300, (0, 160)), Err(Intoppo::NonSta));
        assert!(foto == prima, "la foto non si tocca");
        // Con lo spazio fino al bordo sta.
        assert!(riscrivi_riga(&mut foto, &affinate, &ink, &nuove, 880, (0, 160)).is_ok());
    }
}
