//! Le due maniglie della selezione, da trascinare col dito, e la lente che ingrandisce il testo
//! sotto il dito mentre si trascina.

use egui::{Color32, CornerRadius, Id, Painter, Pos2, Rect, Sense, Stroke, StrokeKind, TextureId, Ui, Vec2, pos2, vec2};

/// Metà del lato della goccia di una maniglia, in dp: quanto scende sotto la riga fino al centro.
pub const RAGGIO: f32 = GOCCIA / 2.0;
/// Lato della zona che si prende col dito, in dp: 48, il minimo di Android per un bersaglio.
pub const PRESA: f32 = 48.0;
/// Misura della lente, in dp.
pub const LENTE: Vec2 = vec2(140.0, 56.0);
/// Quanto ingrandisce la lente.
pub const INGRANDIMENTO: f32 = 1.5;
/// Quanto sta sopra il punto la metà della lente, in dp: sopra il dito, che la coprirebbe.
const SOPRA: f32 = 72.0;
/// Il lato della goccia di una maniglia (`.goccia` nei bozzetti), in dp.
pub const GOCCIA: f32 = 22.0;

/// Quale delle due maniglie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Maniglia {
    Inizio,
    Fine,
}

/// Un bordo della selezione sullo schermo: la x e la riga, dalla cima al fondo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bordo {
    pub x: f32,
    pub cima: f32,
    pub fondo: f32,
}

impl Bordo {
    /// Il punto da dare al motore per questo bordo: a metà della riga.
    pub fn punto(&self) -> Pos2 {
        pos2(self.x, (self.cima + self.fondo) / 2.0)
    }
}

/// Che cosa ha fatto il dito con una maniglia in questo frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Trascinamento {
    /// La maniglia si sposta: `punto` è dove deve andare quel bordo della selezione, cioè il dito
    /// più la distanza che c'era al momento della presa (così la riga sotto il dito non salta).
    /// `presa` è vero nel primo frame del trascinamento.
    Sposta { maniglia: Maniglia, punto: Pos2, presa: bool },
    /// Il dito si è alzato.
    Lasciata(Maniglia),
}

/// Le due maniglie. Tengono solo quale è presa e con che distanza dal dito.
#[derive(Debug, Default)]
pub struct Maniglie {
    presa: Option<(Maniglia, Vec2)>,
}

impl Maniglie {
    /// La maniglia che il dito sta trascinando.
    pub fn trascinata(&self) -> Option<Maniglia> {
        self.presa.map(|(m, _)| m)
    }

    /// Il centro della goccia di una maniglia, dove la prende il dito: appesa sotto la riga, fuori
    /// dalla selezione (a sinistra l'inizio, a destra la fine), come in Android.
    pub fn centro(maniglia: Maniglia, b: Bordo) -> Pos2 {
        rettangolo_goccia(maniglia, b).center()
    }

    /// Disegna le gocce ai due bordi della selezione e le fa prendere col dito; va chiamata a
    /// ogni frame finché c'è una selezione o una maniglia presa, perché egui tenga il
    /// trascinamento. Le gocce stanno sopra quello che è stato disegnato prima nella stessa `ui`.
    pub fn mostra(&mut self, ui: &mut Ui, id: Id, inizio: Bordo, fine: Bordo, colore: Color32) -> Option<Trascinamento> {
        let mut esito = None;
        for (maniglia, b) in [(Maniglia::Inizio, inizio), (Maniglia::Fine, fine)] {
            let centro = Self::centro(maniglia, b);
            let r = ui.interact(Rect::from_center_size(centro, Vec2::splat(PRESA)), id.with(maniglia_id(maniglia)), Sense::drag());
            goccia(ui.painter(), maniglia, b, colore);
            if r.drag_started()
                && let Some(dito) = r.interact_pointer_pos()
            {
                self.presa = Some((maniglia, b.punto() - dito));
                esito = Some(Trascinamento::Sposta { maniglia, punto: b.punto(), presa: true });
            } else if r.dragged()
                && let Some((m, distanza)) = self.presa
                && m == maniglia
                && let Some(dito) = r.interact_pointer_pos()
            {
                esito = Some(Trascinamento::Sposta { maniglia, punto: dito + distanza, presa: false });
            }
            if r.drag_stopped() && self.trascinata() == Some(maniglia) {
                self.presa = None;
                esito = Some(Trascinamento::Lasciata(maniglia));
            }
        }
        esito
    }

    /// La selezione è sparita mentre una maniglia era presa (Annulla, un tocco altrove): la presa
    /// si lascia.
    pub fn lascia(&mut self) {
        self.presa = None;
    }
}

fn maniglia_id(m: Maniglia) -> &'static str {
    match m {
        Maniglia::Inizio => "maniglia inizio",
        Maniglia::Fine => "maniglia fine",
    }
}

/// Il posto della goccia di una maniglia (`.goccia`): un quadrato appeso sotto la riga, con
/// l'angolo vivo sul bordo della selezione; l'inizio sta a sinistra del bordo, la fine a destra.
pub fn rettangolo_goccia(maniglia: Maniglia, b: Bordo) -> Rect {
    let x = match maniglia {
        Maniglia::Inizio => b.x - GOCCIA,
        Maniglia::Fine => b.x,
    };
    Rect::from_min_size(pos2(x, b.fondo), Vec2::splat(GOCCIA))
}

/// Disegna la goccia di una maniglia: tre angoli tondi e quello vivo sul bordo della selezione
/// (in alto a destra per l'inizio, in alto a sinistra per la fine), come in Android. Restituisce
/// il suo rettangolo.
pub fn goccia(painter: &Painter, maniglia: Maniglia, b: Bordo, colore: Color32) -> Rect {
    let r = rettangolo_goccia(maniglia, b);
    let t = (GOCCIA / 2.0) as u8;
    let angoli = match maniglia {
        Maniglia::Inizio => CornerRadius { nw: t, ne: 0, sw: t, se: t },
        Maniglia::Fine => CornerRadius { nw: 0, ne: t, sw: t, se: t },
    };
    painter.rect_filled(r, angoli, colore);
    r
}

/// Quanto sta sotto il fondo della riga il centro del tondo della goccia del cursore, in dp: la
/// goccia è il quadrato di `GOCCIA` girato di 45 gradi, con la punta in alto sul cursore.
pub const CENTRO_GOCCIA_CURSORE: f32 = GOCCIA * std::f32::consts::FRAC_1_SQRT_2;

/// Che cosa ha fatto il dito con la goccia del cursore in questo frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrascinamentoCursore {
    /// Il cursore va al punto: il dito più la distanza che c'era quando si è posato, a metà della
    /// riga. `presa` è vero nel primo frame.
    Sposta { punto: Pos2, presa: bool },
    /// Il dito si è alzato dopo averla trascinata.
    Lasciata,
    /// Un tocco sulla goccia, senza trascinarla.
    Toccata,
}

/// La goccia del cursore, come quella di Android sotto il cursore di un campo di testo: una sola,
/// centrata sotto il cursore, da trascinare per spostarlo. Tiene solo la distanza dal dito.
#[derive(Debug, Default)]
pub struct ManigliaCursore {
    presa: Option<Vec2>,
}

impl ManigliaCursore {
    /// Il dito la sta trascinando.
    pub fn trascinata(&self) -> bool {
        self.presa.is_some()
    }

    /// Il centro del tondo della goccia, dove la prende il dito.
    pub fn centro(b: Bordo) -> Pos2 {
        pos2(b.x, b.fondo + CENTRO_GOCCIA_CURSORE)
    }

    /// Disegna la goccia sotto il cursore `b` e la fa prendere col dito; va chiamata a ogni frame
    /// finché si vede o è presa. Sta sopra quello che è stato disegnato prima nella stessa `ui`.
    pub fn mostra(&mut self, ui: &mut Ui, id: Id, b: Bordo, colore: Color32) -> Option<TrascinamentoCursore> {
        let r = ui.interact(Rect::from_center_size(Self::centro(b), Vec2::splat(PRESA)), id, Sense::click_and_drag());
        goccia_cursore(ui.painter(), b, colore);
        let mut esito = None;
        if r.drag_started() {
            // La distanza da dove il dito si è posato, non da dove il trascinamento è cominciato
            // (qualche dp più in là): la riga sotto il dito non salta.
            let dito = ui.input(|i| i.pointer.press_origin()).or_else(|| r.interact_pointer_pos());
            if let (Some(posato), Some(adesso)) = (dito, r.interact_pointer_pos()) {
                let distanza = b.punto() - posato;
                self.presa = Some(distanza);
                esito = Some(TrascinamentoCursore::Sposta { punto: adesso + distanza, presa: true });
            }
        } else if r.dragged()
            && let (Some(distanza), Some(dito)) = (self.presa, r.interact_pointer_pos())
        {
            esito = Some(TrascinamentoCursore::Sposta { punto: dito + distanza, presa: false });
        }
        if r.drag_stopped() && self.presa.take().is_some() {
            esito = Some(TrascinamentoCursore::Lasciata);
        } else if r.clicked() {
            esito = Some(TrascinamentoCursore::Toccata);
        }
        esito
    }

    /// Il cursore non si vede più (una selezione, un tocco altrove): la presa si lascia.
    pub fn lascia(&mut self) {
        self.presa = None;
    }
}

/// Disegna la goccia del cursore: il tondo sotto la riga e la punta in alto sul cursore, come il
/// quadrato della goccia girato di 45 gradi con l'angolo vivo in alto. Restituisce il rettangolo
/// che la contiene.
pub fn goccia_cursore(painter: &Painter, b: Bordo, colore: Color32) -> Rect {
    let punta = pos2(b.x, b.fondo);
    let centro = ManigliaCursore::centro(b);
    let raggio = GOCCIA / 2.0;
    // Dalla tangente a destra, giù attorno al fondo, fino a quella a sinistra: 270 gradi.
    let mut punti = vec![punta];
    let passi = 24;
    for k in 0..=passi {
        let angolo = (-45.0 + 270.0 * k as f32 / passi as f32).to_radians();
        punti.push(centro + vec2(angolo.cos(), angolo.sin()) * raggio);
    }
    painter.add(egui::Shape::convex_polygon(punti, colore, Stroke::NONE));
    Rect::from_min_max(pos2(b.x - raggio, b.fondo), pos2(b.x + raggio, centro.y + raggio))
}

/// Il raggio del tondo della maniglia della misura, in dp.
pub const RAGGIO_MISURA: f32 = 10.0;
/// Lo spessore del riquadro attorno a un oggetto scelto, in dp.
pub const RIQUADRO: f32 = 2.0;

/// Che cosa ha fatto il dito con la maniglia della misura in questo frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrascinamentoMisura {
    /// La misura che il dito sta dando, sullo schermo; `presa` è vero nel primo frame.
    Sposta { misura: Vec2, presa: bool },
    /// Il dito si è alzato: la misura da dare all'oggetto.
    Lasciata { misura: Vec2 },
}

/// Il riquadro di un oggetto scelto (un'immagine) con la maniglia tonda nell'angolo in basso a
/// destra: il dito la trascina e l'oggetto cambia misura con le sue proporzioni, l'angolo in alto a
/// sinistra fermo. Mentre il dito la tiene, il riquadro mostra la misura nuova; l'oggetto la prende
/// quando il dito si alza. Tiene solo la distanza dal dito e la misura data.
#[derive(Debug, Default)]
pub struct ManigliaMisura {
    presa: Option<Vec2>,
    misura: Option<Vec2>,
}

impl ManigliaMisura {
    /// Il dito la sta trascinando.
    pub fn trascinata(&self) -> bool {
        self.presa.is_some()
    }

    /// La misura che il dito sta dando, finché la trascina.
    pub fn misura(&self) -> Option<Vec2> {
        self.misura
    }

    /// Disegna il riquadro attorno a `oggetto` (o alla misura che il dito sta dando) e la maniglia
    /// nel suo angolo, e la fa prendere col dito; va chiamata a ogni frame finché l'oggetto è
    /// scelto. `larghezze` sono la più piccola e la più grande che il dito può dare, in dp.
    pub fn mostra(&mut self, ui: &mut Ui, id: Id, oggetto: Rect, larghezze: (f32, f32), colore: Color32) -> Option<TrascinamentoMisura> {
        let adesso = Rect::from_min_size(oggetto.min, self.misura.unwrap_or(oggetto.size()));
        let r = ui.interact(Rect::from_center_size(adesso.max, Vec2::splat(PRESA)), id, Sense::drag());
        let mut esito = None;
        if r.drag_started() {
            let dito = ui.input(|i| i.pointer.press_origin()).or_else(|| r.interact_pointer_pos());
            if let (Some(posato), Some(ora)) = (dito, r.interact_pointer_pos()) {
                let distanza = adesso.max - posato;
                self.presa = Some(distanza);
                let misura = misura_per(oggetto, ora + distanza, larghezze);
                self.misura = Some(misura);
                esito = Some(TrascinamentoMisura::Sposta { misura, presa: true });
            }
        } else if r.dragged()
            && let (Some(distanza), Some(dito)) = (self.presa, r.interact_pointer_pos())
        {
            let misura = misura_per(oggetto, dito + distanza, larghezze);
            self.misura = Some(misura);
            esito = Some(TrascinamentoMisura::Sposta { misura, presa: false });
        }
        if r.drag_stopped() && self.presa.take().is_some() {
            esito = self.misura.take().map(|misura| TrascinamentoMisura::Lasciata { misura });
        }
        let riquadro = Rect::from_min_size(oggetto.min, self.misura.unwrap_or(oggetto.size()));
        riquadro_misura(ui.painter(), riquadro, colore);
        esito
    }

    /// L'oggetto non è più scelto: la presa si lascia.
    pub fn lascia(&mut self) {
        self.presa = None;
        self.misura = None;
    }
}

/// La misura con le proporzioni di `oggetto` il cui angolo in basso a destra è più vicino a
/// `angolo`, con l'angolo in alto a sinistra fermo, larga fra le due `larghezze`.
fn misura_per(oggetto: Rect, angolo: Pos2, (minima, massima): (f32, f32)) -> Vec2 {
    let s = oggetto.size();
    if s.x <= 0.0 || s.y <= 0.0 {
        return s;
    }
    let d = angolo - oggetto.min;
    let k = (d.x * s.x + d.y * s.y) / (s.x * s.x + s.y * s.y);
    let w = (s.x * k).clamp(minima, massima.max(minima));
    vec2(w, w * s.y / s.x)
}

/// Disegna il riquadro di un oggetto scelto e la maniglia tonda, bianca col bordo, nel suo angolo
/// in basso a destra.
pub fn riquadro_misura(painter: &Painter, riquadro: Rect, colore: Color32) {
    painter.rect_stroke(riquadro, 0.0, Stroke::new(RIQUADRO, colore), StrokeKind::Outside);
    painter.circle(riquadro.max, RAGGIO_MISURA, Color32::WHITE, Stroke::new(2.5, colore));
}

/// La lente: un riquadro sopra `punto` con il testo attorno a `punto` ingrandito. `immagini` sono
/// le texture già a schermo con il loro rettangolo, `evidenza` i rettangoli della selezione (sotto
/// il testo), `zona` lo spazio in cui la lente deve restare. Restituisce dove l'ha disegnata.
pub fn lente(painter: &Painter, zona: Rect, punto: Pos2, carta: Color32, evidenza: &[(Rect, Color32)], immagini: &[(TextureId, Rect)]) -> Rect {
    lente_con(painter, zona, punto, carta, evidenza, immagini, &[])
}

/// Come [`lente`], con altri rettangoli colorati sopra le immagini (`sopra`): la selezione sulle
/// pagine di un PDF, che sono immagini opache.
pub fn lente_con(
    painter: &Painter,
    zona: Rect,
    punto: Pos2,
    carta: Color32,
    evidenza: &[(Rect, Color32)],
    immagini: &[(TextureId, Rect)],
    sopra: &[(Rect, Color32)],
) -> Rect {
    let mut centro = pos2(punto.x, punto.y - SOPRA);
    // Dentro la zona: se sopra non c'è posto, sotto il dito.
    if centro.y - LENTE.y / 2.0 < zona.min.y {
        centro.y = punto.y + SOPRA;
    }
    centro.x = centro.x.clamp(zona.min.x + LENTE.x / 2.0, (zona.max.x - LENTE.x / 2.0).max(zona.min.x + LENTE.x / 2.0));
    let riquadro = Rect::from_center_size(centro, LENTE);
    // Un rettangolo della pagina portato nella lente: ingrandito attorno a `punto`.
    let dentro = |r: Rect| Rect::from_min_max(centro + (r.min - punto) * INGRANDIMENTO, centro + (r.max - punto) * INGRANDIMENTO);
    let tondo = CornerRadius::same(12);
    painter.rect_filled(riquadro.translate(vec2(0.0, 2.0)), tondo, Color32::from_black_alpha(40));
    painter.rect_filled(riquadro, tondo, carta);
    let p = painter.with_clip_rect(riquadro.shrink(1.0).intersect(painter.clip_rect()));
    for (r, colore) in evidenza {
        p.rect_filled(dentro(*r), 0.0, *colore);
    }
    let uv = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
    for (tex, r) in immagini {
        let d = dentro(*r);
        if d.intersects(riquadro) {
            p.image(*tex, d, uv, Color32::WHITE);
        }
    }
    for (r, colore) in sopra {
        p.rect_filled(dentro(*r), 0.0, *colore);
    }
    painter.rect_stroke(riquadro, tondo, Stroke::new(1.0, Color32::from_gray(160)), StrokeKind::Inside);
    riquadro
}

#[cfg(test)]
mod test {
    use super::*;

    /// egui senza schermo, con il tempo che avanza di un sessantesimo a frame.
    struct Banco {
        ctx: egui::Context,
        t: f64,
        maniglie: Maniglie,
    }

    const INIZIO: Bordo = Bordo { x: 100.0, cima: 200.0, fondo: 220.0 };
    const FINE: Bordo = Bordo { x: 250.0, cima: 200.0, fondo: 220.0 };

    impl Banco {
        fn new() -> Self {
            Self { ctx: egui::Context::default(), t: 0.0, maniglie: Maniglie::default() }
        }

        fn passo(&mut self, eventi: Vec<egui::Event>) -> Option<Trascinamento> {
            let ri = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(412.0, 800.0))),
                time: Some(self.t),
                events: eventi,
                ..Default::default()
            };
            self.t += 1.0 / 60.0;
            let mut esito = None;
            let m = &mut self.maniglie;
            let mut out = self.ctx.run_ui(ri, |ui| esito = m.mostra(ui, Id::new("prova"), INIZIO, FINE, Color32::BLUE));
            // Un delta buttato senza svuotarlo manda epaint in panico.
            out.textures_delta.clear();
            esito
        }
    }

    fn giu(p: Pos2) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(p),
            egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE },
        ]
    }

    fn su(p: Pos2) -> Vec<egui::Event> {
        vec![egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE }]
    }

    #[test]
    fn trascinare_la_fine_la_sposta_col_dito() {
        let mut b = Banco::new();
        b.passo(vec![]);
        let presa = Maniglie::centro(Maniglia::Fine, FINE);
        let mut visti: Vec<_> = b.passo(giu(presa)).into_iter().collect();
        for k in 1..=5 {
            let p = presa + vec2(10.0 * k as f32, 0.0);
            if let Some(t) = b.passo(vec![egui::Event::PointerMoved(p)]) {
                visti.push(t);
            }
        }
        assert!(matches!(visti.first(), Some(Trascinamento::Sposta { maniglia: Maniglia::Fine, presa: true, .. })), "{visti:?}");
        assert_eq!(b.maniglie.trascinata(), Some(Maniglia::Fine));
        // L'ultimo punto: il dito è andato 50 dp a destra, il bordo con lui, a metà della riga.
        let Some(Trascinamento::Sposta { punto, presa: false, .. }) = visti.last() else { panic!("{visti:?}") };
        assert!((punto.x - (FINE.x + 50.0)).abs() < 0.01 && (punto.y - FINE.punto().y).abs() < 0.01, "{punto:?}");
        let fine = b.passo(su(presa + vec2(50.0, 0.0)));
        assert_eq!(fine, Some(Trascinamento::Lasciata(Maniglia::Fine)));
        assert_eq!(b.maniglie.trascinata(), None);
    }

    #[test]
    fn le_maniglie_sono_due_gocce_senza_pallini_ne_linee() {
        let ctx = egui::Context::default();
        let mut m = Maniglie::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            m.mostra(ui, Id::new("gocce"), INIZIO, FINE, Color32::BLUE);
        });
        out.textures_delta.clear();
        let mut rettangoli = Vec::new();
        for c in &out.shapes {
            match &c.shape {
                egui::Shape::Rect(r) => rettangoli.push((r.rect, r.fill)),
                egui::Shape::Circle(_) | egui::Shape::LineSegment { .. } => panic!("pallino o linea: {:?}", c.shape),
                _ => {}
            }
        }
        let attese = vec![(rettangolo_goccia(Maniglia::Inizio, INIZIO), Color32::BLUE), (rettangolo_goccia(Maniglia::Fine, FINE), Color32::BLUE)];
        assert_eq!(rettangoli, attese);
        // Il dito prende la goccia nel suo centro.
        assert_eq!(Maniglie::centro(Maniglia::Inizio, INIZIO), rettangolo_goccia(Maniglia::Inizio, INIZIO).center());
        assert_eq!(Maniglie::centro(Maniglia::Fine, FINE), rettangolo_goccia(Maniglia::Fine, FINE).center());
    }

    #[test]
    fn un_tocco_lontano_non_prende_le_maniglie() {
        let mut b = Banco::new();
        b.passo(vec![]);
        let p = pos2(180.0, 500.0);
        b.passo(giu(p));
        let mosso = b.passo(vec![egui::Event::PointerMoved(p + vec2(40.0, 0.0))]);
        assert_eq!(mosso, None);
        assert_eq!(b.maniglie.trascinata(), None);
    }

    #[test]
    fn la_goccia_ha_l_angolo_vivo_sul_bordo_della_selezione() {
        let b = Bordo { x: 246.0, cima: 320.0, fondo: 342.0 };
        let inizio = rettangolo_goccia(Maniglia::Inizio, b);
        let fine = rettangolo_goccia(Maniglia::Fine, b);
        assert_eq!(inizio, Rect::from_min_max(pos2(246.0 - GOCCIA, 342.0), pos2(246.0, 342.0 + GOCCIA)));
        assert_eq!(fine, Rect::from_min_max(pos2(246.0, 342.0), pos2(246.0 + GOCCIA, 342.0 + GOCCIA)));
        let ctx = egui::Context::default();
        let mut forme = Vec::new();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            goccia(ui.painter(), Maniglia::Inizio, b, Color32::BLUE);
            goccia(ui.painter(), Maniglia::Fine, b, Color32::BLUE);
        });
        out.textures_delta.clear();
        for c in out.shapes {
            if let egui::Shape::Rect(r) = c.shape {
                forme.push((r.rect, r.corner_radius));
            }
        }
        let tondo = (GOCCIA / 2.0) as u8;
        assert_eq!(
            forme,
            vec![(inizio, CornerRadius { nw: tondo, ne: 0, sw: tondo, se: tondo }), (fine, CornerRadius { nw: 0, ne: tondo, sw: tondo, se: tondo }),]
        );
    }

    /// Un frame con la goccia del cursore su `CURSORE`.
    fn passo_cursore(ctx: &egui::Context, t: &mut f64, m: &mut ManigliaCursore, eventi: Vec<egui::Event>) -> Option<TrascinamentoCursore> {
        let ri = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(412.0, 800.0))),
            time: Some(*t),
            events: eventi,
            ..Default::default()
        };
        *t += 1.0 / 60.0;
        let mut esito = None;
        let mut out = ctx.run_ui(ri, |ui| esito = m.mostra(ui, Id::new("cursore"), CURSORE, Color32::BLUE));
        out.textures_delta.clear();
        esito
    }

    const CURSORE: Bordo = Bordo { x: 150.0, cima: 200.0, fondo: 220.0 };

    #[test]
    fn la_goccia_del_cursore_si_trascina_col_dito() {
        let (ctx, mut t, mut m) = (egui::Context::default(), 0.0, ManigliaCursore::default());
        passo_cursore(&ctx, &mut t, &mut m, vec![]);
        let presa = ManigliaCursore::centro(CURSORE);
        let mut visti: Vec<_> = passo_cursore(&ctx, &mut t, &mut m, giu(presa)).into_iter().collect();
        for k in 1..=5 {
            visti.extend(passo_cursore(&ctx, &mut t, &mut m, vec![egui::Event::PointerMoved(presa + vec2(10.0 * k as f32, 0.0))]));
        }
        assert!(matches!(visti.first(), Some(TrascinamentoCursore::Sposta { presa: true, .. })), "{visti:?}");
        assert!(m.trascinata());
        // Il dito è andato 50 dp a destra, il cursore con lui, a metà della riga.
        let Some(TrascinamentoCursore::Sposta { punto, presa: false }) = visti.last() else { panic!("{visti:?}") };
        assert!((punto.x - (CURSORE.x + 50.0)).abs() < 0.01 && (punto.y - CURSORE.punto().y).abs() < 0.01, "{punto:?}");
        assert_eq!(passo_cursore(&ctx, &mut t, &mut m, su(presa + vec2(50.0, 0.0))), Some(TrascinamentoCursore::Lasciata));
        assert!(!m.trascinata());
        // Un tocco fermo sulla goccia non la trascina.
        let mut visti: Vec<_> = passo_cursore(&ctx, &mut t, &mut m, giu(presa)).into_iter().collect();
        visti.extend(passo_cursore(&ctx, &mut t, &mut m, su(presa)));
        assert_eq!(visti, [TrascinamentoCursore::Toccata]);
    }

    fn passo_misura(ctx: &egui::Context, t: &mut f64, m: &mut ManigliaMisura, eventi: Vec<egui::Event>) -> Option<TrascinamentoMisura> {
        let ri = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(412.0, 800.0))),
            time: Some(*t),
            events: eventi,
            ..Default::default()
        };
        *t += 1.0 / 60.0;
        let mut esito = None;
        let mut out = ctx.run_ui(ri, |ui| esito = m.mostra(ui, Id::new("misura"), OGGETTO, (24.0, 300.0), Color32::BLUE));
        out.textures_delta.clear();
        esito
    }

    const OGGETTO: Rect = Rect { min: pos2(40.0, 100.0), max: pos2(240.0, 200.0) };

    #[test]
    fn la_maniglia_della_misura_tiene_le_proporzioni_fra_i_limiti() {
        let (ctx, mut t, mut m) = (egui::Context::default(), 0.0, ManigliaMisura::default());
        passo_misura(&ctx, &mut t, &mut m, vec![]);
        // Il dito un po' dentro il tondo, poi in diagonale 40 dp a sinistra e 20 in su.
        let presa = OGGETTO.max + vec2(-3.0, -2.0);
        let mut visti: Vec<_> = passo_misura(&ctx, &mut t, &mut m, giu(presa)).into_iter().collect();
        for k in 1..=4 {
            visti.extend(passo_misura(&ctx, &mut t, &mut m, vec![egui::Event::PointerMoved(presa + vec2(-10.0, -5.0) * k as f32)]));
        }
        assert!(matches!(visti.first(), Some(TrascinamentoMisura::Sposta { presa: true, .. })), "{visti:?}");
        assert!(m.trascinata());
        let Some(TrascinamentoMisura::Sposta { misura, presa: false }) = visti.last() else { panic!("{visti:?}") };
        assert!((misura.x - 160.0).abs() < 0.01 && (misura.y - 80.0).abs() < 0.01, "{misura:?}");
        // Il dito si alza: la misura da dare, una volta sola.
        let fine = passo_misura(&ctx, &mut t, &mut m, su(presa + vec2(-40.0, -20.0)));
        let Some(TrascinamentoMisura::Lasciata { misura }) = fine else { panic!("{fine:?}") };
        assert!((misura.x - 160.0).abs() < 0.01, "{misura:?}");
        assert!(!m.trascinata() && m.misura().is_none());
        // Fra i limiti: tirata lontano si ferma a 300 dp, schiacciata a 24.
        assert_eq!(misura_per(OGGETTO, pos2(900.0, 900.0), (24.0, 300.0)), vec2(300.0, 150.0));
        assert_eq!(misura_per(OGGETTO, pos2(0.0, 0.0), (24.0, 300.0)), vec2(24.0, 12.0));
    }

    #[test]
    fn la_goccia_del_cursore_ha_la_punta_sul_cursore() {
        let ctx = egui::Context::default();
        let mut contorno = None;
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| contorno = Some(goccia_cursore(ui.painter(), CURSORE, Color32::BLUE)));
        out.textures_delta.clear();
        let poligoni: Vec<&egui::epaint::PathShape> = out
            .shapes
            .iter()
            .filter_map(|c| match &c.shape {
                egui::Shape::Path(p) => Some(p),
                _ => None,
            })
            .collect();
        let [p] = poligoni.as_slice() else { panic!("una goccia sola: {:?}", out.shapes) };
        assert!(p.closed && p.fill == Color32::BLUE);
        // La punta in alto, sul cursore; il tondo largo quanto la goccia, centrato sotto.
        assert_eq!(p.points[0], pos2(CURSORE.x, CURSORE.fondo));
        let r = Rect::from_points(&p.points);
        assert!((r.width() - GOCCIA).abs() < 0.05 && (r.center().x - CURSORE.x).abs() < 0.01, "{r:?}");
        assert!(
            (r.top() - CURSORE.fondo).abs() < 0.01 && (r.bottom() - (CURSORE.fondo + CENTRO_GOCCIA_CURSORE + GOCCIA / 2.0)).abs() < 0.05,
            "{r:?}"
        );
        assert_eq!(contorno, Some(Rect::from_min_max(pos2(CURSORE.x - GOCCIA / 2.0, CURSORE.fondo), pos2(CURSORE.x + GOCCIA / 2.0, r.bottom()))));
    }

    #[test]
    fn la_lente_sta_sopra_il_punto_e_dentro_la_zona() {
        let ctx = egui::Context::default();
        let zona = Rect::from_min_size(Pos2::ZERO, vec2(412.0, 800.0));
        let mut riquadri = Vec::new();
        let mut out = ctx.run_ui(egui::RawInput { screen_rect: Some(zona), ..Default::default() }, |ui| {
            let painter = ui.painter().clone();
            let img = [(TextureId::Managed(0), Rect::from_min_size(Pos2::ZERO, vec2(412.0, 400.0)))];
            riquadri.push(lente(&painter, zona, pos2(200.0, 300.0), Color32::WHITE, &[], &img));
            // Vicino al bordo in alto a sinistra: la lente va sotto il dito e resta dentro.
            riquadri.push(lente(&painter, zona, pos2(5.0, 20.0), Color32::WHITE, &[], &img));
        });
        out.textures_delta.clear();
        assert!(riquadri[0].max.y < 300.0 && (riquadri[0].center().x - 200.0).abs() < 0.01, "{:?}", riquadri[0]);
        assert!(zona.contains_rect(riquadri[1]) && riquadri[1].min.y > 20.0, "{:?}", riquadri[1]);
    }

    #[test]
    fn la_lente_puo_colorare_sopra_le_immagini() {
        let ctx = egui::Context::default();
        let zona = Rect::from_min_size(Pos2::ZERO, vec2(412.0, 800.0));
        let sel = [(Rect::from_min_size(pos2(190.0, 295.0), vec2(30.0, 10.0)), Color32::BLUE)];
        let img = [(TextureId::Managed(0), Rect::from_min_size(Pos2::ZERO, vec2(412.0, 400.0)))];
        let mut out = ctx.run_ui(egui::RawInput { screen_rect: Some(zona), ..Default::default() }, |ui| {
            lente_con(ui.painter(), zona, pos2(200.0, 300.0), Color32::WHITE, &[], &img, &sel);
        });
        out.textures_delta.clear();
        // L'immagine prima, poi la selezione blu ingrandita sopra di lei.
        let ordine: Vec<&str> = out
            .shapes
            .iter()
            .filter_map(|c| match &c.shape {
                egui::Shape::Mesh(_) => Some("immagine"),
                egui::Shape::Rect(r) if r.fill == Color32::BLUE => Some("selezione"),
                _ => None,
            })
            .collect();
        assert_eq!(ordine, ["immagine", "selezione"]);
    }
}
