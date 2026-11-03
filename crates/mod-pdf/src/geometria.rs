//! La geometria del lettore, senza disegnare: le pagine una sotto l'altra sulla carta come nel
//! bozzetto 7, lo zoom attorno a un punto, la pagina che si sta leggendo e le tessere che servono
//! per la parte visibile. Misure in dp di egui (punti), in pixel del dispositivo e in punti del
//! PDF, che qui si chiamano «pt» per non confonderli con i punti di egui.

use egui::{Pos2, Rect, Vec2, pos2, vec2};
use pdfcraft_render::device_pixels;

use crate::apertura::Misura;
use crate::disegno::Rettangolo;

/// Le pagine dai lati della vista, in dp, con lo zoom a 1 (`.pagina { left: 16px }`).
pub const MARGINE: f32 = 16.0;
/// La prima pagina sotto la barra alta, e l'ultima sopra il fondo, in dp (`top: 14px`).
pub const CIMA: f32 = 14.0;
/// Fra due pagine, in dp (la seconda pagina del bozzetto a 571 = 14 + 537 + 20).
pub const SPAZIO: f32 = 20.0;
/// Lo zoom del pizzico, fra questi due valori: 1 è ogni pagina larga quanto lo schermo meno i
/// margini.
pub const ZOOM_MINIMO: f32 = 0.5;
pub const ZOOM_MASSIMO: f32 = 6.0;
/// Il doppio tocco ingrandisce fin qui; da ingrandito torna a 1.
pub const ZOOM_DOPPIO_TOCCO: f32 = 2.5;
/// Il lato di una tessera, in pixel del dispositivo.
pub const TESSERA: u32 = 1024;
/// Il lato più lungo dell'immagine intera di una pagina, in pixel: oltre, la pagina intera resta a
/// questa risoluzione (sfondo un po' sfocato) e la parte visibile si disegna a tessere nitide.
/// Una A4 larga quanto il telefono (circa 1000 × 1410 pixel) ci sta, fino a circa 1,4 volte.
pub const LATO_PAGINA: f32 = 2048.0;

/// Le pagine di un PDF impaginate per una vista larga `larghezza_vista` dp, a uno zoom.
#[derive(Debug, Clone, PartialEq)]
pub struct Impaginazione {
    pagine: Vec<Misura>,
    larghezza_vista: f32,
    zoom: f32,
    /// dp per pt di ogni pagina: a zoom 1 ognuna è larga quanto la vista meno i margini. Con una
    /// scala sola, quella della pagina più larga, una pagina orizzontale in un referto A4
    /// rimpiccioliva tutte le altre (analisi del 27/10, lettura I5).
    k: Vec<f32>,
    /// La cima di ogni pagina, in dp dall'inizio del contenuto.
    cime: Vec<f32>,
    contenuto: Vec2,
    /// Lo spazio in più dopo l'ultima pagina, per la barra flottante sopra il lettore.
    sotto: f32,
}

impl Impaginazione {
    /// Le pagine `pagine` (in pt, come si vedono) per una vista larga `larghezza_vista` dp, a
    /// `zoom` (tenuto fra `ZOOM_MINIMO` e `ZOOM_MASSIMO`).
    pub fn new(pagine: Vec<Misura>, larghezza_vista: f32, zoom: f32) -> Self {
        let zoom = if zoom.is_finite() { zoom.clamp(ZOOM_MINIMO, ZOOM_MASSIMO) } else { 1.0 };
        // Ogni pagina larga così, in dp; una larghezza impossibile vale 1 pt.
        let largo = (larghezza_vista - 2.0 * MARGINE).max(1.0) * zoom;
        let k: Vec<f32> = pagine.iter().map(|p| largo / if p.larghezza.is_finite() && p.larghezza > 0.0 { p.larghezza } else { 1.0 }).collect();
        let mut cime = Vec::with_capacity(pagine.len());
        let mut y = CIMA;
        for (p, k) in pagine.iter().zip(&k) {
            cime.push(y);
            y += p.altezza.max(0.0) * k + SPAZIO;
        }
        let alto = if pagine.is_empty() { 2.0 * CIMA } else { y - SPAZIO + CIMA };
        let contenuto = vec2(larghezza_vista.max(largo + 2.0 * MARGINE), alto);
        Self { pagine, larghezza_vista, zoom, k, cime, contenuto, sotto: 0.0 }
    }

    /// La stessa impaginazione con `dp` in più dopo l'ultima pagina, che così può salire sopra una
    /// barra flottante.
    pub fn con_spazio_sotto(mut self, dp: f32) -> Self {
        let dp = if dp.is_finite() { dp.max(0.0) } else { 0.0 };
        self.contenuto.y += dp - self.sotto;
        self.sotto = dp;
        self
    }

    pub fn spazio_sotto(&self) -> f32 {
        self.sotto
    }

    /// La stessa impaginazione a un altro zoom.
    pub fn con_zoom(&self, zoom: f32) -> Self {
        Self::new(self.pagine.clone(), self.larghezza_vista, zoom).con_spazio_sotto(self.sotto)
    }

    /// La misura della pagina `i` in pt.
    pub fn misura(&self, i: usize) -> Misura {
        self.pagine[i]
    }

    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    pub fn pagine(&self) -> usize {
        self.pagine.len()
    }

    pub fn larghezza_vista(&self) -> f32 {
        self.larghezza_vista
    }

    /// dp per pt della pagina `i` a questo zoom (fuori dalle pagine, quelli della prima).
    pub fn dp_per_pt(&self, i: usize) -> f32 {
        self.k.get(i).or(self.k.first()).copied().unwrap_or(1.0)
    }

    /// La misura di tutto il contenuto, in dp: almeno larga quanto la vista.
    pub fn contenuto(&self) -> Vec2 {
        self.contenuto
    }

    /// Il rettangolo della pagina `i` nel contenuto, in dp, centrata in larghezza.
    pub fn rettangolo(&self, i: usize) -> Rect {
        let p = self.pagine[i];
        let k = self.dp_per_pt(i);
        let w = p.larghezza.max(0.0) * k;
        Rect::from_min_size(pos2((self.contenuto.x - w) / 2.0, self.cime[i]), vec2(w, p.altezza.max(0.0) * k))
    }

    /// L'ultima pagina con la cima sopra `y` (dp nel contenuto); sopra la prima, la prima.
    fn pagina_a(&self, y: f32) -> usize {
        self.cime.partition_point(|c| *c <= y).saturating_sub(1)
    }

    /// Le pagine che si vedono, anche solo in parte, nella `vista` (in dp nel contenuto).
    pub fn visibili(&self, vista: Rect) -> std::ops::Range<usize> {
        if self.pagine.is_empty() {
            return 0..0;
        }
        let prima = self.pagina_a(vista.top());
        // La prima può finire sopra la vista (la vista comincia nello spazio fra due pagine).
        let prima = if self.rettangolo(prima).bottom() < vista.top() { prima + 1 } else { prima };
        let ultima = self.pagina_a(vista.bottom());
        prima.min(ultima + 1)..ultima + 1
    }

    /// La pagina che si sta leggendo: quella che occupa più spazio nella vista, a parità la più in
    /// alto; in cima al documento la prima, in fondo l'ultima se si vede tutta. Con la pagina a
    /// metà della vista, con pagine basse (slide, zoom piccolo) in cima si leggeva la seconda e in
    /// fondo la penultima (analisi del 27/10, lettura I1).
    pub fn corrente(&self, vista: Rect) -> usize {
        let n = self.pagine.len();
        if n == 0 || vista.top() <= 0.0 {
            return 0;
        }
        if vista.bottom() >= self.contenuto.y - 0.5 && self.rettangolo(n - 1).top() >= vista.top() {
            return n - 1;
        }
        let quanto = |i: usize| {
            let r = self.rettangolo(i);
            (r.bottom().min(vista.bottom()) - r.top().max(vista.top())).max(0.0)
        };
        let meglio = self.visibili(vista).fold(None, |meglio: Option<(usize, f32)>, i| match meglio {
            Some((_, m)) if m >= quanto(i) => meglio,
            _ => Some((i, quanto(i))),
        });
        meglio.map_or_else(|| self.pagina_a(vista.center().y), |(i, _)| i)
    }

    /// Il punto del contenuto `p` (dp) come pagina e punto in pt dentro la pagina, tenuto sulla
    /// pagina (nello spazio fra due pagine vale il bordo più vicino di quella sopra).
    pub fn punto(&self, p: Pos2) -> Option<(usize, Pos2)> {
        if self.pagine.is_empty() {
            return None;
        }
        let i = self.pagina_a(p.y);
        let r = self.rettangolo(i);
        let m = self.pagine[i];
        let k = self.dp_per_pt(i);
        Some((i, pos2(((p.x - r.left()) / k).clamp(0.0, m.larghezza), ((p.y - r.top()) / k).clamp(0.0, m.altezza))))
    }

    /// Dove sta nel contenuto (dp) il punto `pt` della pagina `i`.
    pub fn posizione(&self, i: usize, pt: Pos2) -> Pos2 {
        let r = self.rettangolo(i);
        r.min + pt.to_vec2() * self.dp_per_pt(i)
    }

    /// Lo scorrimento tenuto fra 0 e la fine del contenuto per una vista di misura `vista`.
    pub fn tieni(&self, scorrimento: Vec2, vista: Vec2) -> Vec2 {
        let massimo = (self.contenuto - vista).max(Vec2::ZERO);
        scorrimento.max(Vec2::ZERO).min(massimo)
    }

    /// Lo zoom cambia a `zoom` attorno a un punto: il punto del documento che stava sullo schermo
    /// a `da` (dp dall'angolo della vista) va a `a`, anche se le dita si sono spostate.
    /// Restituisce l'impaginazione nuova e lo scorrimento che fa tornare i conti.
    pub fn zoom_attorno(&self, zoom: f32, da: Vec2, a: Vec2, scorrimento: Vec2, vista: Vec2) -> (Impaginazione, Vec2) {
        let nuova = self.con_zoom(zoom);
        let Some((i, pt)) = self.punto((scorrimento + da).to_pos2()) else { return (nuova, Vec2::ZERO) };
        let dove = nuova.posizione(i, pt).to_vec2() - a;
        let tenuto = nuova.tieni(dove, vista);
        (nuova, tenuto)
    }

    /// Lo zoom dopo un doppio tocco: dalla larghezza dello schermo (o meno) ingrandisce, da
    /// ingrandito torna alla larghezza dello schermo.
    pub fn zoom_doppio_tocco(&self) -> f32 {
        if self.zoom > 1.05 { 1.0 } else { ZOOM_DOPPIO_TOCCO }
    }

    /// I pixel del dispositivo per pt della pagina `i` a questo zoom, con `ppp` pixel per dp.
    pub fn scala(&self, i: usize, ppp: f32) -> f32 {
        self.dp_per_pt(i) * ppp
    }

    /// La scala dell'immagine intera della pagina `i`: quella dello zoom, o meno se il lato più
    /// lungo supererebbe `LATO_PAGINA` pixel.
    pub fn scala_intera(&self, i: usize, ppp: f32) -> f32 {
        let p = self.pagine[i];
        let lato = p.larghezza.max(p.altezza).max(1.0);
        self.scala(i, ppp).min(LATO_PAGINA / lato)
    }

    /// La pagina `i` a questo zoom è più grande dell'immagine intera: servono le tessere.
    pub fn servono_tessere(&self, i: usize, ppp: f32) -> bool {
        self.scala(i, ppp) > self.scala_intera(i, ppp) * 1.01
    }

    /// La misura in pixel della pagina `i` a `scala`, come la disegna il motore.
    pub fn pixel(&self, i: usize, scala: f32) -> (u32, u32) {
        let p = self.pagine[i];
        (device_pixels(p.larghezza, scala), device_pixels(p.altezza, scala))
    }

    /// Le tessere della pagina `i` che toccano la `vista` (dp nel contenuto), alla scala dello zoom: (colonna, riga, pixel della tessera), dalla più vicina al
    /// centro della vista. Le tessere sul bordo destro e in fondo sono più piccole.
    pub fn tessere(&self, i: usize, vista: Rect, ppp: f32) -> Vec<(u32, u32, Rettangolo)> {
        let scala = self.scala(i, ppp);
        let (w, h) = self.pixel(i, scala);
        let pagina = self.rettangolo(i);
        let dentro = vista.intersect(pagina);
        if !dentro.is_positive() {
            return Vec::new();
        }
        // Dp dalla cima della pagina in pixel: k × ppp = scala.
        let px = |dp: f32| (dp.max(0.0) * ppp) as u32;
        let (x0, x1) = (px(dentro.left() - pagina.left()) / TESSERA, (px(dentro.right() - pagina.left()).min(w.saturating_sub(1))) / TESSERA);
        let (y0, y1) = (px(dentro.top() - pagina.top()) / TESSERA, (px(dentro.bottom() - pagina.top()).min(h.saturating_sub(1))) / TESSERA);
        let centro = vista.center() - pagina.min;
        let mut fuori = Vec::new();
        for ty in y0..=y1 {
            for tx in x0..=x1 {
                let (x, y) = (tx * TESSERA, ty * TESSERA);
                if x >= w || y >= h {
                    continue;
                }
                fuori.push((tx, ty, Rettangolo { x, y, larghezza: TESSERA.min(w - x), altezza: TESSERA.min(h - y) }));
            }
        }
        let distanza = |r: &Rettangolo| {
            let c = vec2(r.x as f32 + r.larghezza as f32 / 2.0, r.y as f32 + r.altezza as f32 / 2.0) / ppp;
            (c - centro).length_sq()
        };
        fuori.sort_by(|a, b| distanza(&a.2).total_cmp(&distanza(&b.2)));
        fuori
    }

    /// Dove va sullo schermo (dp nel contenuto) un pezzo della pagina `i` disegnato a `scala` coi
    /// pixel `r`: con lo zoom di adesso, anche se l'immagine è di uno zoom di prima.
    pub fn posto_dei_pixel(&self, i: usize, scala: f32, r: Rettangolo) -> Rect {
        let pagina = self.rettangolo(i);
        let dp = self.dp_per_pt(i) / scala;
        Rect::from_min_size(pagina.min + vec2(r.x as f32, r.y as f32) * dp, vec2(r.larghezza as f32, r.altezza as f32) * dp)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const A4: Misura = Misura { larghezza: 595.0, altezza: 842.0 };

    fn tre_a4(larghezza: f32, zoom: f32) -> Impaginazione {
        Impaginazione::new(vec![A4; 3], larghezza, zoom)
    }

    fn vicino(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn pagine_come_il_bozzetto_7() {
        // Il telefono del bozzetto è largo 412 dp: la pagina A4 è larga 380 e alta circa 537,
        // a 16 dp dai lati e a 14 dalla barra; la seconda comincia 20 dp sotto.
        let imp = tre_a4(412.0, 1.0);
        let r = imp.rettangolo(0);
        assert!(vicino(r.left(), 16.0) && vicino(r.width(), 380.0) && vicino(r.top(), 14.0), "{r:?}");
        assert!(vicino(r.height(), 380.0 * 842.0 / 595.0), "{r:?}");
        assert!(vicino(imp.rettangolo(1).top(), r.bottom() + 20.0));
        assert!(vicino(imp.contenuto().x, 412.0));
        assert!(vicino(imp.contenuto().y, imp.rettangolo(2).bottom() + 14.0));
    }

    #[test]
    fn lo_spazio_sotto_allunga_il_contenuto_anche_a_un_altro_zoom() {
        let imp = tre_a4(412.0, 1.0);
        let alto = imp.contenuto().y;
        let con = imp.clone().con_spazio_sotto(88.0);
        assert!(vicino(con.contenuto().y, alto + 88.0));
        // Messo due volte vale una.
        assert!(vicino(con.clone().con_spazio_sotto(88.0).contenuto().y, alto + 88.0));
        let doppio = con.con_zoom(2.0);
        assert!(vicino(doppio.contenuto().y, tre_a4(412.0, 2.0).contenuto().y + 88.0));
        assert_eq!(doppio.rettangolo(1), tre_a4(412.0, 2.0).rettangolo(1));
        assert_eq!(imp.con_spazio_sotto(f32::NAN).spazio_sotto(), 0.0);
    }

    #[test]
    fn ogni_pagina_e_larga_quanto_lo_schermo_anche_fra_pagine_di_misure_diverse() {
        // Analisi del 27/10, lettura I5: la scala era una per tutte, quella della pagina più larga,
        // e una pagina orizzontale in un referto A4 rimpiccioliva tutte le altre (268 dp invece di
        // 380); una MediaBox assurda le faceva sparire.
        let imp = Impaginazione::new(vec![A4, Misura { larghezza: 842.0, altezza: 595.0 }, Misura { larghezza: 300.0, altezza: 300.0 }], 412.0, 1.0);
        for i in 0..3 {
            let r = imp.rettangolo(i);
            assert!(vicino(r.left(), 16.0) && vicino(r.right(), 396.0), "{i}: {r:?}");
        }
        assert!(vicino(imp.rettangolo(0).height(), 380.0 * 842.0 / 595.0));
        assert!(vicino(imp.rettangolo(1).height(), 380.0 * 595.0 / 842.0));
        // Una MediaBox enorme resta una pagina sua, larga come le altre.
        let enorme = Impaginazione::new(vec![A4, Misura { larghezza: 1e9, altezza: 1e9 }, A4], 412.0, 1.0);
        for i in 0..3 {
            assert!(vicino(enorme.rettangolo(i).width(), 380.0), "{i}: {:?}", enorme.rettangolo(i));
        }
        // Ingrandite, tutte insieme.
        let doppio = imp.con_zoom(2.0);
        assert!(vicino(doppio.rettangolo(1).width(), 760.0) && vicino(doppio.contenuto().x, 792.0));
    }

    #[test]
    fn lo_zoom_resta_fra_i_bordi_e_allarga_il_contenuto() {
        assert_eq!(tre_a4(412.0, 100.0).zoom(), ZOOM_MASSIMO);
        assert_eq!(tre_a4(412.0, 0.01).zoom(), ZOOM_MINIMO);
        assert_eq!(tre_a4(412.0, f32::NAN).zoom(), 1.0);
        let piccola = tre_a4(412.0, 0.5);
        // Rimpicciolita la pagina sta al centro, e non si scorre di lato.
        assert!(vicino(piccola.contenuto().x, 412.0));
        assert!(vicino(piccola.rettangolo(0).center().x, 206.0));
        let grande = tre_a4(412.0, 2.0);
        assert!(vicino(grande.contenuto().x, 760.0 + 32.0));
        assert!(vicino(grande.rettangolo(0).width(), 760.0));
    }

    #[test]
    fn pagine_visibili_e_pagina_corrente() {
        let imp = tre_a4(412.0, 1.0);
        let vista = |y: f32| Rect::from_min_size(pos2(0.0, y), vec2(412.0, 851.0));
        assert_eq!(imp.visibili(vista(0.0)), 0..2);
        assert_eq!(imp.corrente(vista(0.0)), 0);
        // Scorsa di una pagina: la seconda a metà schermo.
        let seconda = imp.rettangolo(1).top();
        assert_eq!(imp.corrente(vista(seconda)), 1);
        assert_eq!(imp.visibili(vista(seconda)), 1..3);
        // La vista che comincia nello spazio fra la prima e la seconda non conta la prima.
        let r0 = imp.rettangolo(0);
        assert_eq!(imp.visibili(Rect::from_min_size(pos2(0.0, r0.bottom() + 5.0), vec2(412.0, 100.0))), 1..2);
        assert!(imp.visibili(Rect::from_min_size(pos2(0.0, 1e6), vec2(412.0, 100.0))).is_empty());
        let vuota = Impaginazione::new(Vec::new(), 412.0, 1.0);
        assert_eq!(vuota.visibili(vista(0.0)), 0..0);
        assert_eq!(vuota.punto(pos2(10.0, 10.0)), None);
    }

    #[test]
    fn la_pagina_corrente_e_quella_che_si_vede_di_piu() {
        // Analisi del 27/10, lettura I1: era la pagina a metà della vista, e con pagine basse
        // (slide, zoom piccolo) in cima si leggeva «2 / 10» e in fondo «9 / 10».
        let slide = Impaginazione::new(vec![Misura { larghezza: 960.0, altezza: 540.0 }; 10], 392.0, 1.0);
        let vista = vec2(392.0, 700.0);
        let in_fondo = |imp: &Impaginazione| Rect::from_min_size(imp.tieni(vec2(0.0, f32::MAX), vista).to_pos2(), vista);
        assert_eq!(slide.corrente(Rect::from_min_size(Pos2::ZERO, vista)), 0);
        assert_eq!(slide.corrente(in_fondo(&slide)), 9);
        // In mezzo, la slide che si vede di più: la quinta tutta, pezzi della quarta e della sesta.
        assert_eq!(slide.corrente(Rect::from_center_size(slide.rettangolo(4).center(), vec2(392.0, 400.0))), 4);
        // Dodici A4 rimpicciolite col pizzico.
        let a4 = Impaginazione::new(vec![A4; 12], 392.0, ZOOM_MINIMO);
        assert_eq!(a4.corrente(Rect::from_min_size(Pos2::ZERO, vista)), 0);
        assert_eq!(a4.corrente(in_fondo(&a4)), 11);
    }

    #[test]
    fn punto_e_posizione_si_ritrovano() {
        let imp = tre_a4(412.0, 1.7);
        let pt = pos2(100.0, 700.0);
        let p = imp.posizione(2, pt);
        let (i, ritrovato) = imp.punto(p).unwrap();
        assert_eq!(i, 2);
        assert!(vicino(ritrovato.x, 100.0) && vicino(ritrovato.y, 700.0), "{ritrovato:?}");
        // Nello spazio fra due pagine: il fondo di quella sopra.
        let (i, sotto) = imp.punto(pos2(200.0, imp.rettangolo(0).bottom() + 10.0)).unwrap();
        assert_eq!(i, 0);
        assert!(vicino(sotto.y, 842.0));
    }

    #[test]
    fn il_pizzico_tiene_fermo_il_punto_fra_le_dita() {
        let vista = vec2(412.0, 851.0);
        let imp = tre_a4(412.0, 1.0);
        let scorrimento = vec2(0.0, 300.0);
        let dita = vec2(150.0, 400.0);
        let prima = imp.punto((scorrimento + dita).to_pos2()).unwrap();
        let (nuova, s) = imp.zoom_attorno(3.0, dita, dita, scorrimento, vista);
        assert_eq!(nuova.zoom(), 3.0);
        let dopo = nuova.punto((s + dita).to_pos2()).unwrap();
        assert_eq!(dopo.0, prima.0);
        assert!(vicino(dopo.1.x, prima.1.x) && vicino(dopo.1.y, prima.1.y), "{prima:?} {dopo:?}");
        // Le dita si spostano mentre pizzicano: il punto segue le dita.
        let spostate = dita + vec2(30.0, -50.0);
        let (nuova, s) = imp.zoom_attorno(2.0, dita, spostate, scorrimento, vista);
        let dopo = nuova.punto((s + spostate).to_pos2()).unwrap();
        assert!(vicino(dopo.1.x, prima.1.x) && vicino(dopo.1.y, prima.1.y), "{prima:?} {dopo:?}");
        // Rimpicciolendo vicino all'inizio lo scorrimento non va sotto zero, né oltre la fine.
        let (nuova, s) = imp.zoom_attorno(0.5, vec2(10.0, 10.0), vec2(10.0, 10.0), Vec2::ZERO, vista);
        assert!(s.x == 0.0 && s.y >= 0.0, "{s:?}");
        assert!(s.y <= nuova.contenuto().y - vista.y, "{s:?}");
    }

    #[test]
    fn il_doppio_tocco_alterna_larghezza_e_ingrandimento() {
        assert_eq!(tre_a4(412.0, 1.0).zoom_doppio_tocco(), ZOOM_DOPPIO_TOCCO);
        assert_eq!(tre_a4(412.0, 0.6).zoom_doppio_tocco(), ZOOM_DOPPIO_TOCCO);
        assert_eq!(tre_a4(412.0, 2.5).zoom_doppio_tocco(), 1.0);
        assert_eq!(tre_a4(412.0, 5.0).zoom_doppio_tocco(), 1.0);
        // Tornando alla larghezza dello schermo non si scorre più di lato.
        let imp = tre_a4(412.0, 2.5);
        let (nuova, s) = imp.zoom_attorno(1.0, vec2(300.0, 200.0), vec2(300.0, 200.0), vec2(500.0, 900.0), vec2(412.0, 851.0));
        assert_eq!(s.x, 0.0);
        assert_eq!(nuova.zoom(), 1.0);
    }

    #[test]
    fn immagine_intera_fino_al_tetto_poi_tessere() {
        // Telefono a 2,625 pixel per dp: alla larghezza dello schermo la A4 è di circa 1000 ×
        // 1410 pixel, intera e senza tessere.
        let ppp = 2.625;
        let imp = tre_a4(412.0, 1.0);
        let (w, h) = imp.pixel(0, imp.scala_intera(0, ppp));
        assert!((997..=999).contains(&w) && (1410..=1413).contains(&h), "{w} × {h}");
        assert!(!imp.servono_tessere(0, ppp));
        // Ingrandita tre volte la pagina intera resta a 2048 pixel e servono le tessere.
        let imp = tre_a4(412.0, 3.0);
        assert!(imp.servono_tessere(0, ppp));
        let (_, h) = imp.pixel(0, imp.scala_intera(0, ppp));
        assert!((2047..=2049).contains(&h), "{h}");
    }

    #[test]
    fn tessere_solo_della_parte_visibile_dal_centro() {
        let ppp = 2.625;
        let imp = tre_a4(412.0, 4.0);
        let vista = Rect::from_min_size(pos2(600.0, 900.0), vec2(412.0, 851.0));
        let tessere = imp.tessere(0, vista, ppp);
        assert!(!tessere.is_empty());
        let (w, h) = imp.pixel(0, imp.scala(0, ppp));
        let pagina = imp.rettangolo(0);
        for (tx, ty, r) in &tessere {
            assert_eq!((r.x, r.y), (tx * TESSERA, ty * TESSERA));
            assert!(r.x + r.larghezza <= w && r.y + r.altezza <= h, "{r:?} fuori da {w} × {h}");
            // Ogni tessera tocca la vista.
            let posto = imp.posto_dei_pixel(0, imp.scala(0, ppp), *r);
            assert!(posto.intersects(vista), "{posto:?}");
            assert!(pagina.expand(1.0).contains_rect(posto), "{posto:?} fuori dalla pagina {pagina:?}");
        }
        // La prima è quella che contiene il centro della vista.
        let prima = imp.posto_dei_pixel(0, imp.scala(0, ppp), tessere[0].2);
        assert!(prima.contains(vista.center()), "{prima:?}");
        // Poche: al massimo 3 × 4 per uno schermo di 1081 × 2234 pixel, mai tutta la pagina (a 4
        // volte ne servirebbero 24 per coprirla).
        assert!(tessere.len() <= 12, "{} tessere", tessere.len());
        // Una pagina che non si vede non ha tessere.
        assert!(imp.tessere(2, vista, ppp).is_empty());
        // Sul bordo destro e in fondo le tessere sono più corte.
        let tutta = imp.rettangolo(0).expand(10.0);
        let ultime = imp.tessere(0, tutta, ppp);
        assert!(ultime.iter().any(|(_, _, r)| r.larghezza < TESSERA));
        assert!(ultime.iter().any(|(_, _, r)| r.altezza < TESSERA));
    }

    #[test]
    fn un_immagine_vecchia_va_al_posto_giusto_anche_a_un_altro_zoom() {
        let ppp = 2.0;
        let prima = tre_a4(412.0, 2.0);
        let scala = prima.scala(0, ppp);
        let r = Rettangolo { x: 1024, y: 2048, larghezza: 1024, altezza: 1024 };
        let posto = prima.posto_dei_pixel(1, scala, r);
        // Allo zoom doppio lo stesso pezzo è grande il doppio, nello stesso punto della pagina.
        let dopo = prima.con_zoom(4.0);
        let nuovo = dopo.posto_dei_pixel(1, scala, r);
        assert!(vicino(nuovo.width(), posto.width() * 2.0));
        let pt = |imp: &Impaginazione, p: Pos2| imp.punto(p).unwrap().1;
        let (a, b) = (pt(&prima, posto.min), pt(&dopo, nuovo.min));
        assert!(vicino(a.x, b.x) && vicino(a.y, b.y), "{a:?} {b:?}");
    }
}
