//! Attrezzi per le prove dei componenti: un banco col tema installato e le forme disegnate.

use egui::{Color32, Rect, Shape};
use egui_kittest::Harness;

use crate::tema;

/// Lo schermo del telefono, in dp.
pub const SCHERMO: egui::Vec2 = egui::vec2(412.0, 915.0);

/// Un banco di prova grande come il telefono, col tema installato e già attivo. La `ui` data a
/// `app` copre tutto lo schermo, senza i margini del pannello di egui.
pub fn banco<S>(mut app: impl FnMut(&mut egui::Ui, &mut S) + 'static, stato: S) -> Harness<'static, S> {
    let tutto = move |ui: &mut egui::Ui, s: &mut S| {
        let zona = ui.ctx().content_rect();
        ui.scope_builder(egui::UiBuilder::new().max_rect(zona), |ui| app(ui, s));
    };
    let mut h = Harness::builder().with_size(SCHERMO).build_ui_state(tutto, stato);
    tema::installa(&h.ctx);
    h.run_steps(2);
    h
}

/// Un tocco del dito in questo punto, poi qualche passo.
pub fn tocco<S>(h: &mut Harness<'static, S>, pos: egui::Pos2) {
    h.event(egui::Event::PointerMoved(pos));
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
    }
    h.run_steps(3);
}

/// Le forme disegnate nell'ultimo passo, con quelle dentro i gruppi.
pub fn forme<S>(h: &Harness<'_, S>) -> Vec<Shape> {
    fn appiattisci(s: &Shape, fuori: &mut Vec<Shape>) {
        match s {
            Shape::Vec(v) => v.iter().for_each(|s| appiattisci(s, fuori)),
            s => fuori.push(s.clone()),
        }
    }
    let mut fuori = Vec::new();
    for c in &h.output().shapes {
        appiattisci(&c.shape, &mut fuori);
    }
    fuori
}

/// I rettangoli pieni di questo colore, senza le ombre (che sono sfumate).
pub fn riempiti<S>(h: &Harness<'_, S>, colore: Color32) -> Vec<Rect> {
    forme(h)
        .into_iter()
        .filter_map(|s| match s {
            Shape::Rect(r) if r.fill == colore && r.blur_width == 0.0 => Some(r.rect),
            Shape::Circle(c) if c.fill == colore => Some(Rect::from_center_size(c.center, egui::Vec2::splat(2.0 * c.radius))),
            _ => None,
        })
        .collect()
}

/// Il colore con cui è disegnato il testo `testo`, se c'è.
pub fn colore_del_testo<S>(h: &Harness<'_, S>, testo: &str) -> Option<Color32> {
    forme(h).into_iter().find_map(|s| match s {
        Shape::Text(t) if t.galley.job.text == testo => {
            let c = t.galley.job.sections.first().map(|s| s.format.color).unwrap_or(t.fallback_color);
            Some(if c == Color32::PLACEHOLDER { t.fallback_color } else { c })
        }
        _ => None,
    })
}

/// La famiglia del primo pezzo del testo `testo` disegnato, se c'è.
pub fn famiglia_del_testo<S>(h: &Harness<'_, S>, testo: &str) -> Option<egui::FontFamily> {
    forme(h).into_iter().find_map(|s| match s {
        Shape::Text(t) if t.galley.job.text == testo => t.galley.job.sections.first().map(|s| s.format.font_id.family.clone()),
        _ => None,
    })
}

/// Quante righe ha il testo `testo` disegnato e se finisce coi puntini, se c'è.
pub fn righe_del_testo<S>(h: &Harness<'_, S>, testo: &str) -> Option<(usize, bool)> {
    forme(h).into_iter().find_map(|s| match s {
        Shape::Text(t) if t.galley.job.text == testo => Some((t.galley.rows.len(), t.galley.elided)),
        _ => None,
    })
}

/// Il rettangolo del testo `testo` disegnato, se c'è.
pub fn rettangolo_del_testo<S>(h: &Harness<'_, S>, testo: &str) -> Option<Rect> {
    forme(h).into_iter().find_map(|s| match s {
        Shape::Text(t) if t.galley.job.text == testo => Some(t.visual_bounding_rect()),
        _ => None,
    })
}
