//! La selezione del testo di una pagina, senza disegno: la parola sotto il dito (pressione
//! lunga), i due bordi che le maniglie spostano, i rettangoli delle righe da colorare, il testo da
//! copiare e tutta la pagina. Tutto sui caratteri del motore (`PageText`, ricerca PDFCraft,
//! sezione 4), nell'ordine di lettura, in pt nella pagina come si vede (dall'alto a sinistra).
//! La selezione resta in una pagina: «Tutto» è il testo della pagina.

use pdfcraft_render::PageText;

use crate::ricerca::RettangoloPt;

/// Una selezione in una pagina: dal carattere `inizio` al carattere `fine`, compresi, nell'ordine
/// di lettura del motore.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selezione {
    pub pagina: usize,
    pub inizio: usize,
    pub fine: usize,
}

/// Quale bordo della selezione si sposta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bordo {
    Inizio,
    Fine,
}

/// Un bordo della selezione nella pagina, in pt: la x e la riga, dalla cima al fondo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BordoPt {
    pub x: f32,
    pub cima: f32,
    pub fondo: f32,
}

/// Quanto il punto `(x, y)` sta fuori dal rettangolo `r`, in pt (0 dentro).
fn fuori_da(r: [f32; 4], x: f32, y: f32) -> f32 {
    let dx = (r[0] - x).max(x - r[2]).max(0.0);
    let dy = (r[1] - y).max(y - r[3]).max(0.0);
    dx.max(dy)
}

/// Un carattere bianco (uno spazio scritto nel PDF, un a capo): non fa parola.
fn bianco(testo: &PageText, i: usize) -> bool {
    testo.glyphs.get(i).is_none_or(|g| g.text.trim().is_empty())
}

/// La parola sotto il punto `(x, y)` della pagina, se il punto sta su un carattere o a non più di
/// `tolleranza` pt da uno: il primo e l'ultimo carattere. Lontano dal testo, o su uno spazio,
/// niente.
pub fn parola_al_punto(testo: &PageText, x: f32, y: f32, tolleranza: f32) -> Option<(usize, usize)> {
    let i = testo.nearest(x, y)?;
    if fuori_da(testo.glyphs[i].rect, x, y) > tolleranza || bianco(testo, i) {
        return None;
    }
    testo.word_at(i)
}

/// Il carattere a cui va un bordo portato al punto `(x, y)`: quello più vicino, o quello dopo
/// (per l'inizio) o prima (per la fine) se il punto ha passato la sua metà, come un cursore fra
/// due lettere.
fn carattere_per_bordo(testo: &PageText, bordo: Bordo, x: f32, y: f32) -> Option<usize> {
    let i = testo.nearest(x, y)?;
    let g = &testo.glyphs[i];
    let meta = (g.rect[0] + g.rect[2]) / 2.0;
    let n = testo.glyphs.len();
    Some(match bordo {
        Bordo::Inizio if x > meta && i + 1 < n => i + 1,
        Bordo::Fine if x < meta && i > 0 => i - 1,
        _ => i,
    })
}

impl Selezione {
    /// La parola sotto il punto, come selezione (vedi [`parola_al_punto`]).
    pub fn parola(pagina: usize, testo: &PageText, x: f32, y: f32, tolleranza: f32) -> Option<Selezione> {
        parola_al_punto(testo, x, y, tolleranza).map(|(inizio, fine)| Selezione { pagina, inizio, fine })
    }

    /// Tutto il testo della pagina; None se la pagina non ha testo (una scansione).
    pub fn tutta(pagina: usize, testo: &PageText) -> Option<Selezione> {
        let n = testo.glyphs.len();
        (n > 0).then(|| Selezione { pagina, inizio: 0, fine: n - 1 })
    }

    /// È tutta la pagina.
    pub fn e_tutta(&self, testo: &PageText) -> bool {
        self.inizio == 0 && self.fine + 1 >= testo.glyphs.len()
    }

    /// La selezione col bordo `bordo` portato al punto `(x, y)` della pagina; l'altro resta. Un
    /// bordo non passa l'altro: la selezione tiene almeno un carattere.
    pub fn sposta(&self, testo: &PageText, bordo: Bordo, x: f32, y: f32) -> Selezione {
        let Some(i) = carattere_per_bordo(testo, bordo, x, y) else { return *self };
        match bordo {
            Bordo::Inizio => Selezione { inizio: i.min(self.fine), ..*self },
            Bordo::Fine => Selezione { fine: i.max(self.inizio), ..*self },
        }
    }

    /// I rettangoli delle righe selezionate, in pt.
    pub fn rettangoli(&self, testo: &PageText) -> Vec<RettangoloPt> {
        testo.line_rects(self.inizio..self.fine + 1)
    }

    /// I due bordi, dove si appendono le maniglie: a sinistra del primo carattere e a destra
    /// dell'ultimo, alti quanto la loro riga.
    pub fn bordi(&self, testo: &PageText) -> Option<(BordoPt, BordoPt)> {
        let a = testo.glyphs.get(self.inizio)?.rect;
        let b = testo.glyphs.get(self.fine)?.rect;
        Some((BordoPt { x: a[0], cima: a[1], fondo: a[3] }, BordoPt { x: b[2], cima: b[1], fondo: b[3] }))
    }

    /// Il testo da copiare, con gli spazi fra le parole e gli a capo fra le righe.
    pub fn testo(&self, testo: &PageText) -> String {
        testo.text_of(self.inizio..self.fine + 1)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::Pdf;
    use crate::prova::{Pagina, pdf};

    /// Due righe in Helvetica da 14 su una A4: «Ciao bel mondo» con la linea di base a 142 pt
    /// dall'alto e «seconda riga» 40 pt sotto; la seconda pagina senza testo.
    fn testo() -> PageText {
        let contenuto = "BT /F1 14 Tf 60 700 Td (Ciao bel mondo) Tj 0 -40 Td (seconda riga) Tj ET";
        let p = Pdf::apri(pdf(&[Pagina { larghezza: 595.0, altezza: 842.0, rotazione: 0, contenuto }]), None).unwrap();
        (*p.testo(0).unwrap()).clone()
    }

    /// Il centro del carattere `i`.
    fn centro(t: &PageText, i: usize) -> (f32, f32) {
        let r = t.glyphs[i].rect;
        ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0)
    }

    /// Il primo carattere di `parola` nel testo della pagina.
    fn indice(t: &PageText, parola: &str) -> usize {
        let caratteri: String = t.glyphs.iter().map(|g| g.text.as_str()).collect();
        let byte = caratteri.find(parola).unwrap();
        caratteri[..byte].chars().count()
    }

    #[test]
    fn la_pressione_lunga_prende_la_parola_sotto_il_dito() {
        let t = testo();
        let (x, y) = centro(&t, indice(&t, "bel") + 1);
        let s = Selezione::parola(0, &t, x, y, 4.0).unwrap();
        assert_eq!(s.testo(&t), "bel");
        // Un poco sopra la riga, dentro la tolleranza: la stessa parola.
        let r = t.glyphs[s.inizio].rect;
        assert_eq!(Selezione::parola(0, &t, x, r[1] - 3.0, 4.0), Some(s));
        // Lontano dal testo, niente.
        assert_eq!(Selezione::parola(0, &t, 300.0, 600.0, 4.0), None);
        // Una pagina senza testo non ha né parole né «Tutto».
        assert_eq!(Selezione::parola(0, &PageText::default(), 10.0, 10.0, 4.0), None);
        assert_eq!(Selezione::tutta(0, &PageText::default()), None);
    }

    #[test]
    fn i_bordi_stanno_ai_lati_della_parola_e_i_rettangoli_sulle_righe() {
        let t = testo();
        let (x, y) = centro(&t, indice(&t, "mondo"));
        let s = Selezione::parola(0, &t, x, y, 4.0).unwrap();
        let (a, b) = s.bordi(&t).unwrap();
        assert_eq!(a.x, t.glyphs[s.inizio].rect[0]);
        assert_eq!(b.x, t.glyphs[s.fine].rect[2]);
        // La riga: la linea di base a 142 pt dall'alto sta fra cima e fondo.
        assert!(a.cima < 142.0 && a.fondo >= 141.0 && a.cima == b.cima, "{a:?} {b:?}");
        assert_eq!(s.rettangoli(&t).len(), 1);
    }

    #[test]
    fn le_maniglie_allargano_e_stringono_senza_passarsi() {
        let t = testo();
        let (x, y) = centro(&t, indice(&t, "bel"));
        let s = Selezione::parola(0, &t, x, y, 4.0).unwrap();
        // La fine portata a destra della «d» di «mondo»: «bel mond».
        let d = t.glyphs[indice(&t, "mondo") + 3].rect;
        let s2 = s.sposta(&t, Bordo::Fine, d[2] - 0.5, (d[1] + d[3]) / 2.0);
        assert_eq!(s2.testo(&t), "bel mond");
        // Poco prima della metà della «d»: la fine si ferma prima, «bel mon».
        let s3 = s.sposta(&t, Bordo::Fine, d[0] + 1.0, (d[1] + d[3]) / 2.0);
        assert_eq!(s3.testo(&t), "bel mon");
        // L'inizio portato all'inizio della riga: tutta la prima riga fino a «mond».
        let c = t.glyphs[0].rect;
        assert_eq!(s2.sposta(&t, Bordo::Inizio, c[0] - 5.0, (c[1] + c[3]) / 2.0).testo(&t), "Ciao bel mond");
        // La fine sulla seconda riga: il testo ha l'a capo.
        let r = t.glyphs[indice(&t, "riga") + 3].rect;
        assert_eq!(s.sposta(&t, Bordo::Fine, r[2], (r[1] + r[3]) / 2.0).testo(&t), "bel mondo\nseconda riga");
        // L'inizio oltre la fine: si ferma sulla fine, un carattere resta.
        let oltre = s.sposta(&t, Bordo::Inizio, r[2], (r[1] + r[3]) / 2.0);
        assert_eq!((oltre.inizio, oltre.fine), (s.fine, s.fine));
    }

    #[test]
    fn tutto_e_il_testo_della_pagina() {
        let t = testo();
        let s = Selezione::tutta(3, &t).unwrap();
        assert_eq!(s.pagina, 3);
        assert!(s.e_tutta(&t));
        assert_eq!(s.testo(&t), "Ciao bel mondo\nseconda riga");
        assert_eq!(s.rettangoli(&t).len(), 2);
        let (x, y) = centro(&t, 0);
        assert!(!Selezione::parola(3, &t, x, y, 4.0).unwrap().e_tutta(&t));
    }
}
