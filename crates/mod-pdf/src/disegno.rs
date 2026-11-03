//! Disegnare le pagine di un PDF in immagini RGBA8 pronte per le texture di egui.

use egui::ColorImage;
use pdfcraft_render::{PageRenderer, RenderRequest, RenderedPage, RequestKind, Tile};

use crate::apertura::Pdf;

/// Il lato più lungo di un rettangolo da disegnare, in pixel: oltre, il motore lo taglierebbe
/// (tessere da 1 a 4096 pixel per lato, `pdfcraft-render`, `raster.rs`).
pub const LATO_MASSIMO: u32 = 4096;

/// Un pezzo della pagina già portata alla scala, in pixel dall'alto a sinistra.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rettangolo {
    pub x: u32,
    pub y: u32,
    pub larghezza: u32,
    pub altezza: u32,
}

/// Una pagina che non si disegna. Nella diagnosi va solo `diagnosi()`, mai il messaggio del
/// motore.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroreDisegno {
    /// Scala zero, negativa o non finita.
    Scala,
    /// Rettangolo vuoto o con un lato oltre `LATO_MASSIMO`.
    Rettangolo,
    /// La pagina non c'è.
    Pagina,
    /// Il motore non l'ha disegnata (file rovinato, pagina senza misura, panico fermato).
    Motore,
    /// I pixel del motore non tornano con la misura dell'immagine.
    Pixel,
}

impl ErroreDisegno {
    /// Il genere per la diagnosi.
    pub fn diagnosi(self) -> &'static str {
        match self {
            ErroreDisegno::Scala => "scala non valida",
            ErroreDisegno::Rettangolo => "rettangolo non valido",
            ErroreDisegno::Pagina => "pagina che non c'è",
            ErroreDisegno::Motore => "errore del motore",
            ErroreDisegno::Pixel => "pixel che non tornano",
        }
    }
}

/// Disegna le pagine di un PDF sul thread che lo chiama: legge il file una volta sola, quando
/// nasce. Si può mandare a un thread di lavoro.
pub struct Disegnatore {
    motore: PageRenderer,
}

impl Pdf {
    /// Un disegnatore sui byte da mostrare del documento (con gli aspetti dei commenti che non li
    /// hanno) e con la sua configurazione (password, livelli).
    pub fn disegnatore(&self) -> Disegnatore {
        let configurazione = self.sessione.render_config(self.id).unwrap_or_default();
        Disegnatore { motore: PageRenderer::new(self.documento().display.clone(), configurazione) }
    }
}

impl Disegnatore {
    /// La pagina `pagina` (da 0) intera, con `scala` pixel per punto. Una pagina enorme a una
    /// scala alta esce più piccola, entro 8192 pixel per lato e 64 milioni di pixel
    /// (`pdfcraft_render::MAX_SIDE`, `MAX_PIXELS`): la misura vera è in `size`. Per ingrandire
    /// oltre, `disegna_rettangolo`.
    pub fn disegna(&mut self, pagina: usize, scala: f32) -> Result<ColorImage, ErroreDisegno> {
        self.richiesta(pagina, scala, None)
    }

    /// Solo il rettangolo `r` della pagina `pagina` portata a `scala`.
    pub fn disegna_rettangolo(&mut self, pagina: usize, scala: f32, r: Rettangolo) -> Result<ColorImage, ErroreDisegno> {
        let lato = 1..=LATO_MASSIMO;
        if !lato.contains(&r.larghezza) || !lato.contains(&r.altezza) {
            return Err(ErroreDisegno::Rettangolo);
        }
        self.richiesta(pagina, scala, Some(Tile { x: r.x, y: r.y, w: r.larghezza, h: r.altezza }))
    }

    fn richiesta(&mut self, pagina: usize, scala: f32, tile: Option<Tile>) -> Result<ColorImage, ErroreDisegno> {
        if !(scala.is_finite() && scala > 0.0) {
            return Err(ErroreDisegno::Scala);
        }
        if pagina >= self.motore.page_count() {
            return Err(ErroreDisegno::Pagina);
        }
        // `render` non va mai in panico (lo dice il motore); il risultato dice se è andata.
        immagine(self.motore.render(RenderRequest { page: pagina, kind: RequestKind::Pixels, tile, scale: scala, tag: 0, preview: false }))
    }
}

/// Un risultato del motore (anche di un `RenderPool`) come immagine di egui, senza copiare i
/// pixel: il motore li dà in parole da 32 bit con i byte R, G, B, A premoltiplicati, come
/// `Color32`.
pub fn immagine(r: RenderedPage) -> Result<ColorImage, ErroreDisegno> {
    if r.error.is_some() {
        return Err(ErroreDisegno::Motore);
    }
    let misura = [r.width as usize, r.height as usize];
    let parole = r.rgba.into_words();
    if parole.is_empty() || parole.len() != misura[0].saturating_mul(misura[1]) {
        return Err(ErroreDisegno::Pixel);
    }
    // Stessa misura e stesso allineamento di u32 (`#[repr(C, align(4))]`): non fallisce.
    let pixel = bytemuck::allocation::try_cast_vec(parole).map_err(|_| ErroreDisegno::Pixel)?;
    Ok(ColorImage::new(misura, pixel))
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::prova::{Pagina, pdf};
    use egui::Color32;

    const ROSSO: Color32 = Color32::from_rgb(255, 0, 0);

    /// 100 × 200 punti con un quadrato rosso di 50 punti in basso a sinistra.
    fn con_quadrato() -> Pdf {
        let pagina = Pagina { larghezza: 100.0, altezza: 200.0, rotazione: 0, contenuto: "1 0 0 rg 0 0 50 50 re f" };
        Pdf::apri(pdf(&[pagina]), None).unwrap()
    }

    fn pixel(img: &ColorImage, x: usize, y: usize) -> Color32 {
        img.pixels[y * img.size[0] + x]
    }

    #[test]
    fn disegna_la_pagina_alla_scala() {
        let img = con_quadrato().disegnatore().disegna(0, 2.0).unwrap();
        assert_eq!(img.size, [200, 400]);
        assert_eq!(pixel(&img, 20, 380), ROSSO);
        assert_eq!(pixel(&img, 99, 301), ROSSO);
        assert_eq!(pixel(&img, 101, 380), Color32::WHITE);
        assert_eq!(pixel(&img, 180, 20), Color32::WHITE);
    }

    #[test]
    fn disegna_un_rettangolo() {
        let mut d = con_quadrato().disegnatore();
        let img = d.disegna_rettangolo(0, 2.0, Rettangolo { x: 0, y: 300, larghezza: 100, altezza: 100 }).unwrap();
        assert_eq!(img.size, [100, 100]);
        assert!(img.pixels.iter().all(|&p| p == ROSSO), "il quadrato non riempie il rettangolo");
        let img = d.disegna_rettangolo(0, 2.0, Rettangolo { x: 100, y: 0, larghezza: 64, altezza: 32 }).unwrap();
        assert_eq!(img.size, [64, 32]);
        assert!(img.pixels.iter().all(|&p| p == Color32::WHITE));
    }

    #[test]
    fn pagina_girata_come_si_vede() {
        // 200 × 100 girata di 90 gradi: l'angolo in basso a sinistra va in alto a sinistra.
        let pagina = Pagina { larghezza: 200.0, altezza: 100.0, rotazione: 90, contenuto: "1 0 0 rg 0 0 50 50 re f" };
        let img = Pdf::apri(pdf(&[pagina]), None).unwrap().disegnatore().disegna(0, 1.0).unwrap();
        assert_eq!(img.size, [100, 200]);
        assert_eq!(pixel(&img, 10, 10), ROSSO);
        assert_eq!(pixel(&img, 90, 190), Color32::WHITE);
    }

    #[test]
    fn pagina_scala_e_rettangolo_sbagliati() {
        let mut d = con_quadrato().disegnatore();
        assert_eq!(d.disegna(1, 1.0), Err(ErroreDisegno::Pagina));
        for scala in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(d.disegna(0, scala), Err(ErroreDisegno::Scala), "{scala}");
        }
        let r = Rettangolo { x: 0, y: 0, larghezza: 0, altezza: 10 };
        assert_eq!(d.disegna_rettangolo(0, 1.0, r), Err(ErroreDisegno::Rettangolo));
        let r = Rettangolo { x: 0, y: 0, larghezza: 10, altezza: LATO_MASSIMO + 1 };
        assert_eq!(d.disegna_rettangolo(0, 1.0, r), Err(ErroreDisegno::Rettangolo));
        let r = Rettangolo { x: 0, y: 0, larghezza: 10, altezza: 10 };
        assert_eq!(d.disegna_rettangolo(3, 1.0, r), Err(ErroreDisegno::Pagina));
        assert_eq!(d.disegna_rettangolo(0, -2.0, r), Err(ErroreDisegno::Scala));
    }

    #[test]
    fn pdf_con_password_si_disegna() {
        let p = Pdf::apri(crate::prova::cifrato("Segreta1"), Some("Segreta1")).unwrap();
        let img = p.disegnatore().disegna(0, 0.5).unwrap();
        // Una A4 a mezza scala, bianca sotto la scritta.
        assert!(img.size[0].abs_diff(298) <= 1 && img.size[1].abs_diff(421) <= 1, "{:?}", img.size);
        assert_eq!(pixel(&img, 150, 300), Color32::WHITE);
    }

    #[test]
    fn errore_del_motore_e_pixel_che_non_tornano() {
        let r = RenderedPage {
            request: Default::default(),
            width: 0,
            height: 0,
            rgba: Default::default(),
            error: Some("renderer crashed on page 1: Zanzibar".into()),
            text: None,
            warnings: Vec::new(),
            millis: 0,
        };
        assert_eq!(immagine(r), Err(ErroreDisegno::Motore));
        let r = RenderedPage {
            request: Default::default(),
            width: 10,
            height: 10,
            rgba: Default::default(),
            error: None,
            text: None,
            warnings: Vec::new(),
            millis: 0,
        };
        assert_eq!(immagine(r), Err(ErroreDisegno::Pixel));
        let generi: std::collections::HashSet<_> =
            [ErroreDisegno::Scala, ErroreDisegno::Rettangolo, ErroreDisegno::Pagina, ErroreDisegno::Motore, ErroreDisegno::Pixel]
                .map(ErroreDisegno::diagnosi)
                .into();
        assert_eq!(generi.len(), 5);
    }

    #[test]
    fn il_disegnatore_va_su_un_thread_di_lavoro() {
        fn manda<T: Send>() {}
        manda::<Disegnatore>();
        let mut d = con_quadrato().disegnatore();
        let img = std::thread::spawn(move || d.disegna(0, 1.0).unwrap()).join().unwrap();
        assert_eq!(img.size, [100, 200]);
    }
}
