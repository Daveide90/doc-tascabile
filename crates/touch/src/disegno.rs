//! Pezzi di disegno comuni ai componenti: un'icona con l'etichetta in fila, un testo di una riga
//! tagliato coi puntini.

use std::sync::Arc;

use egui::text::{LayoutJob, TextWrapping};
use egui::{Color32, Context, FontId, Galley, Painter, Rect, Ui, pos2};

use crate::tema::Carattere;

/// Un'icona e un'etichetta in fila, già misurate, nello stesso colore. Serve a tutti i componenti
/// che mettono un'icona davanti a un testo.
pub(crate) struct Riga {
    icona: Option<Arc<Galley>>,
    testo: Option<Arc<Galley>>,
    spazio: f32,
}

impl Riga {
    /// `icona` col suo carattere e la sua misura, `testo` col suo font, `spazio` fra i due.
    pub(crate) fn new(ui: &Ui, icona: Option<(&str, Carattere, f32)>, testo: Option<(&str, FontId)>, spazio: f32, colore: Color32) -> Self {
        Self::dal_contesto(ui.ctx(), icona, testo, spazio, colore)
    }

    /// Come `new`, senza una `Ui`: per misurare prima di sapere dove disegnare.
    pub(crate) fn dal_contesto(
        ctx: &Context,
        icona: Option<(&str, Carattere, f32)>,
        testo: Option<(&str, FontId)>,
        spazio: f32,
        colore: Color32,
    ) -> Self {
        // Il font prima: `Carattere::font` guarda i caratteri, e dentro `fonts_mut` si bloccherebbe.
        let icona = icona.map(|(i, c, misura)| {
            let font = c.font(ctx, misura);
            ctx.fonts_mut(|f| f.layout_no_wrap(i.to_owned(), font, colore))
        });
        let testo = testo.map(|(t, font)| ctx.fonts_mut(|f| f.layout_no_wrap(t.to_owned(), font, colore)));
        Self { icona, testo, spazio }
    }

    pub(crate) fn larghezza(&self) -> f32 {
        let i = self.icona.as_ref().map_or(0.0, |g| g.size().x);
        let t = self.testo.as_ref().map_or(0.0, |g| g.size().x);
        let spazio = if self.icona.is_some() && self.testo.is_some() { self.spazio } else { 0.0 };
        i + spazio + t
    }

    /// Disegna la fila da `sinistra`, centrata in altezza su `centro_y`.
    pub(crate) fn dipingi(&self, painter: &Painter, sinistra: f32, centro_y: f32) {
        let mut x = sinistra;
        for g in [&self.icona, &self.testo].into_iter().flatten() {
            painter.galley(pos2(x, centro_y - g.size().y / 2.0), g.clone(), Color32::PLACEHOLDER);
            x += g.size().x + self.spazio;
        }
    }

    /// Disegna la fila al centro di `rect`.
    pub(crate) fn dipingi_al_centro(&self, painter: &Painter, rect: Rect) {
        self.dipingi(painter, rect.center().x - self.larghezza() / 2.0, rect.center().y);
    }
}

/// Un testo che va a capo, largo al più `larghezza` e su `righe` righe al più: oltre finisce coi
/// puntini.
pub(crate) fn testo_a_capo(ui: &Ui, testo: &str, font: FontId, colore: Color32, larghezza: f32, righe: usize) -> Arc<Galley> {
    let mut job = LayoutJob::simple(testo.to_owned(), font, colore, larghezza.max(0.0));
    job.wrap = TextWrapping { max_width: larghezza.max(0.0), max_rows: righe, break_anywhere: false, overflow_character: Some('…') };
    ui.painter().layout_job(job)
}

/// Un testo di una riga largo al più `larghezza`: se non ci sta finisce coi puntini.
pub(crate) fn testo_tagliato(ui: &Ui, testo: &str, font: FontId, colore: Color32, larghezza: f32) -> Arc<Galley> {
    let mut job = LayoutJob::simple_singleline(testo.to_owned(), font, colore);
    job.wrap = TextWrapping { max_width: larghezza.max(0.0), max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    ui.painter().layout_job(job)
}
