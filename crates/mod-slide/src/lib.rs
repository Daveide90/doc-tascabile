//! Le Slide di Tascabile: presentazioni .pptx, .ppsx e .potx da leggere, presentare ed esportare,
//! coi crate di DeckCraft sotto il motore (niente `deckcraft-engine`, quindi niente codec video:
//! ricerca DeckCraft, sezione 13, e piano della tappa G).

mod apertura;
mod disegno;
mod lettore;
mod spettacolo;

pub use apertura::{ErroreApertura, ErrorePdf, Genere, Immagine, LIMITE_MB, Presentazione, sembra_presentazione};
pub use disegno::{Disegnata, Disegnatore, Richiesta, Uso};
pub use lettore::{EsitoLettura, LettoreSlide};
pub use spettacolo::{EsitoSpettacolo, FINE, Spettacolo};

/// Per le prove dell'app: la lezione di prova e i file che non si aprono.
#[doc(hidden)]
pub mod prova {
    /// Quattro slide in Calibri: titolo; tre punti che entrano uno alla volta, con le note;
    /// un'immagine; una slide nascosta (piano della tappa G, G1).
    pub const LEZIONE: &[u8] = include_bytes!("../tests/fixtures/lezione.pptx");
    /// Due slide in 4:3 (720 × 540 punti), fatte col server MCP di DeckCraft come le altre qui
    /// sotto (tappa I, ripasso delle Slide).
    pub const QUATTRO_TERZI: &[u8] = include_bytes!("../tests/fixtures/quattro-terzi.pptx");
    /// Le stesse due slide, tutte e due nascoste.
    pub const TUTTE_NASCOSTE: &[u8] = include_bytes!("../tests/fixtures/tutte-nascoste.pptx");
    /// Una presentazione senza slide.
    pub const SENZA_SLIDE: &[u8] = include_bytes!("../tests/fixtures/senza-slide.pptx");
    /// Una slide in verticale (540 × 960 punti).
    pub const VERTICALE: &[u8] = include_bytes!("../tests/fixtures/verticale.pptx");

    /// Un .ppt vecchio, per quel che ne vede l'apertura: un contenitore OLE senza cifratura.
    pub fn ppt_vecchio() -> Vec<u8> {
        let mut byte = vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
        byte.resize(1024, 0);
        byte
    }
}

#[cfg(test)]
mod test {
    const LEZIONE: &[u8] = include_bytes!("../tests/fixtures/lezione.pptx");

    #[test]
    fn la_lezione_si_apre_e_la_prima_slide_si_disegna() {
        assert!(deckcraft_pptx::sniff(LEZIONE));
        let (mut p, danni) = deckcraft_pptx::import_checked(LEZIONE).expect("si apre");
        deckcraft_format::repair(&mut p);
        assert!(danni.is_empty(), "{danni:?}");
        assert_eq!(p.slides.len(), 4);
        let img = deckcraft_render::render_slide(&p, 0, &deckcraft_render::RenderOpts { scale: 1.0, ..Default::default() });
        assert_eq!((img.width, img.height), (960, 540));
    }

    #[test]
    fn i_font_delle_slide_sono_incorporati() {
        // Il PC ha Carlito fra i font di sistema e il telefono no: si guarda la lista incorporata,
        // non has_family, che cerca anche nel sistema.
        let famiglie: Vec<&str> = deckcraft_fonts::CRAFT_FONTS.iter().map(|f| f.family).collect();
        for f in ["Carlito", "Caladea", "Liberation Sans", "Liberation Serif", "Inter"] {
            assert!(famiglie.contains(&f), "{f} manca: {famiglie:?}");
        }
    }
}
