//! Il foglio che sale dal fondo dello schermo (il «bottom sheet» di Android), nell'aspetto dei
//! bozzetti (`.sotto`): bianco, gli angoli in alto tondi, la lineetta, il titolo in Fraunces e la
//! X per chiudere, sopra il velo scuro.

use egui::{Align, Align2, Context, CornerRadius, Frame, Id, Layout, Margin, Modal, Order, RichText, ScrollArea, Sense, Ui, UiKind, vec2};

use crate::icone;
use crate::pulsanti::PulsanteIcona;
use crate::tema::{self, Carattere};

/// Quanto dura la salita, in secondi.
const SALITA: f32 = 0.2;
/// Il margine interno del foglio, in dp (`.sotto { padding: 10px 18px 36px }`; in basso 14, perché
/// i 22 della barra di navigazione nell'app sono fuori dalla zona libera).
const MARGINE: Margin = Margin { left: 18, right: 18, top: 10, bottom: 14 };
/// Il foglio non va oltre questa parte della zona libera: sopra resta un po' del documento, da
/// toccare per chiuderlo; il contenuto più lungo scorre.
const ALTEZZA_MASSIMA: f32 = 0.75;

/// Un foglio che sale dal fondo, largo quanto la zona libera da barre e tastiera, sopra tutto il
/// resto. Si chiude toccando fuori, con la X (quando i caratteri del tema ci sono) o con Indietro
/// (`indietro`), che chi lo usa deve chiamare prima di ogni altra cosa che faccia Indietro.
pub struct FoglioDalFondo {
    id: Id,
    aperto: bool,
}

impl FoglioDalFondo {
    pub fn new(id: impl Into<Id>) -> Self {
        Self { id: id.into(), aperto: false }
    }

    pub fn aperto(&self) -> bool {
        self.aperto
    }

    pub fn apri(&mut self) {
        self.aperto = true;
    }

    pub fn chiudi(&mut self) {
        self.aperto = false;
    }

    /// Indietro: se il foglio è aperto lo chiude e dice vero, cioè che Indietro è stato usato qui.
    pub fn indietro(&mut self) -> bool {
        std::mem::replace(&mut self.aperto, false)
    }

    /// Disegna il foglio, se è aperto, con `titolo` e il contenuto; restituisce quello che
    /// restituisce il contenuto, oppure None se il foglio è chiuso. Va chiamata a ogni frame,
    /// anche da chiuso: così all'apertura il foglio sale invece di comparire. Il contenuto ha i
    /// colori chiari del tema anche se l'app è scura.
    pub fn mostra<R>(&mut self, ctx: &Context, titolo: &str, contenuto: impl FnOnce(&mut Ui) -> R) -> Option<R> {
        // Da 0 (giù, fuori dallo schermo) a 1 (su); riparte da 0 a ogni apertura.
        let su = ctx.animate_bool_with_time(self.id.with("salita"), self.aperto, SALITA);
        if !self.aperto {
            return None;
        }
        let zona = ctx.content_rect();
        let id_altezza = self.id.with("altezza");
        let altezza = ctx.data(|d| d.get_temp::<f32>(id_altezza)).unwrap_or(zona.height() / 2.0);
        // Come l'area di `Modal`, ma in fondo e senza il vincolo di restare nella zona libera,
        // che durante la salita lo terrebbe fermo dov'è.
        let area = egui::Area::new(self.id)
            .kind(UiKind::Modal)
            .sense(Sense::hover())
            .anchor(Align2::CENTER_BOTTOM, vec2(0.0, (1.0 - su) * altezza))
            .order(Order::Foreground)
            .interactable(true)
            .constrain(false);
        let raggio = tema::RAGGIO_FOGLIO;
        let frame = Frame::new()
            .fill(tema::FOGLIO)
            .corner_radius(CornerRadius { nw: raggio, ne: raggio, sw: 0, se: 0 })
            .shadow(tema::OMBRA_FOGLIO[0])
            .inner_margin(MARGINE);
        let largo = (zona.width() - MARGINE.sum().x).max(0.0);
        // Senza i caratteri del tema la X sarebbe un quadratino: si chiude toccando fuori.
        let con_la_x = Carattere::Icona.pronto(ctx);
        let mut chiudi = false;
        let risposta = Modal::new(self.id).area(area).frame(frame).backdrop_color(tema::VELO_SCURO).show(ctx, |ui| {
            let lampeggio = ui.visuals().text_cursor.blink;
            ui.style_mut().visuals = tema::visuals();
            ui.style_mut().visuals.text_cursor.blink = lampeggio;
            ui.set_width(largo);
            // La lineetta in cima (`.maniglia`).
            let (r, _) = ui.allocate_exact_size(vec2(largo, 4.0), Sense::hover());
            ui.painter().rect_filled(egui::Rect::from_center_size(r.center(), vec2(36.0, 4.0)), 2.0, tema::LINEETTA);
            ui.add_space(10.0 - ui.spacing().item_spacing.y);
            ui.horizontal(|ui| {
                ui.set_min_height(tema::PULSANTE);
                ui.label(RichText::new(titolo).font(Carattere::Titolo.font(ui.ctx(), 23.0)).color(tema::INCHIOSTRO));
                if con_la_x {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        chiudi = ui.add(PulsanteIcona::new(icone::CLOSE, "Chiudi")).clicked();
                    });
                }
            });
            // Lo spazio per lo scorrimento si chiede esplicitamente: dentro un'area di egui lo
            // spazio libero è la misura del frame prima, e un contenuto che all'inizio non c'era
            // tutto resterebbe tagliato (da scorrere) anche con lo schermo libero. Il foglio
            // intero, testa compresa, sta nella parte massima della zona.
            let testa = ui.cursor().top() - ui.min_rect().top() + MARGINE.sum().y;
            let alto = (zona.height() * ALTEZZA_MASSIMA - testa).max(tema::PULSANTE);
            // Mai più largo dello schermo: un contenuto troppo largo (una fila di pulsanti che non
            // va a capo) allargherebbe il foglio, che centrato uscirebbe dai due lati (0.8.3, il
            // foglio Revisioni a 372 dp). Il contenuto va in un Ui figlio, tagliato al bordo del
            // foglio, e del figlio il foglio prende solo l'altezza.
            let posto = egui::Rect::from_min_size(ui.cursor().min, vec2(largo, alto));
            let bordo = ui.clip_rect();
            let mut dentro = ui.new_child(egui::UiBuilder::new().max_rect(posto).layout(*ui.layout()));
            dentro.set_clip_rect(
                egui::Rect::from_x_y_ranges(posto.left() - f32::from(MARGINE.left)..=posto.right() + f32::from(MARGINE.right), bordo.y_range())
                    .intersect(bordo),
            );
            let fatto = ScrollArea::vertical()
                .id_salt(self.id.with("scorre"))
                .max_height(alto)
                .scroll_source(egui::scroll_area::ScrollSource::ALL)
                .show(&mut dentro, contenuto)
                .inner;
            ui.advance_cursor_after_rect(egui::Rect::from_min_size(posto.min, vec2(largo, dentro.min_rect().height())));
            fatto
        });
        ctx.data_mut(|d| d.insert_temp(id_altezza, risposta.response.rect.height()));
        if chiudi || risposta.backdrop_response.clicked() {
            self.aperto = false;
        }
        Some(risposta.inner)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use egui::accesskit::Role;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    const SCHERMO: egui::Vec2 = egui::vec2(412.0, 915.0);

    /// Lo stato della prova: il foglio e quante volte si è toccato il suo bottone.
    struct Prova {
        foglio: FoglioDalFondo,
        tocchi: u32,
    }

    fn harness(aperto: bool) -> Harness<'static, Prova> {
        let mut foglio = FoglioDalFondo::new(Id::new("prova"));
        if aperto {
            foglio.apri();
        }
        let mut h = Harness::builder().with_size(SCHERMO).build_ui_state(
            |ui, p: &mut Prova| {
                ui.label("Sotto il foglio");
                let toccato = p.foglio.mostra(ui.ctx(), "Formato", |ui| ui.button("Dentro").clicked());
                if toccato == Some(true) {
                    p.tocchi += 1;
                }
            },
            Prova { foglio, tocchi: 0 },
        );
        h.run_steps(4);
        h
    }

    /// Un tocco del dito in questo punto, poi qualche frame.
    fn tocco(h: &mut Harness<'static, Prova>, pos: egui::Pos2) {
        h.event(egui::Event::PointerMoved(pos));
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
        }
        h.run_steps(3);
    }

    #[test]
    fn chiuso_non_si_vede() {
        let h = harness(false);
        assert!(h.query_by_label("Formato").is_none());
        assert!(h.query_by_label("Dentro").is_none());
    }

    #[test]
    fn aperto_sta_in_fondo_largo_quanto_lo_schermo() {
        let h = harness(true);
        let titolo = h.get_by_label("Formato").rect();
        let dentro = h.get_by_role_and_label(Role::Button, "Dentro").rect();
        // Nella metà bassa, e il contenuto sotto il titolo.
        assert!(titolo.min.y > SCHERMO.y / 2.0, "titolo troppo in alto: {titolo:?}");
        assert!(dentro.min.y > titolo.max.y && dentro.max.y <= SCHERMO.y, "contenuto fuori posto: {dentro:?}");
        assert!(titolo.min.x < 40.0, "il foglio non parte dal bordo: {titolo:?}");
    }

    /// Un contenuto più largo dello schermo (una fila che non va a capo) non allarga il foglio:
    /// il titolo resta al suo posto e il foglio resta dentro lo schermo (0.8.3, il foglio
    /// Revisioni a 372 dp usciva dai due lati).
    #[test]
    fn un_contenuto_troppo_largo_non_porta_il_foglio_fuori_dallo_schermo() {
        let mut foglio = FoglioDalFondo::new(Id::new("largo"));
        foglio.apri();
        let mut h = Harness::builder().with_size(SCHERMO).build_ui_state(
            |ui, f: &mut FoglioDalFondo| {
                f.mostra(ui.ctx(), "Revisioni", |ui| {
                    ui.horizontal(|ui| {
                        for i in 0..3 {
                            ui.add_sized(vec2(160.0, 40.0), egui::Button::new(format!("Pulsante {i}")));
                        }
                    });
                });
            },
            foglio,
        );
        h.run_steps(4);
        let titolo = h.get_by_label("Revisioni").rect();
        assert!(titolo.left() >= 0.0 && (titolo.left() - MARGINE.left as f32).abs() < 2.0, "il titolo è fuori posto: {titolo:?}");
        let primo = h.get_by_label("Pulsante 0").rect();
        assert!(primo.left() >= 0.0, "il foglio esce a sinistra: {primo:?}");
    }

    #[test]
    fn un_tocco_dentro_arriva_al_contenuto_e_il_foglio_resta() {
        let mut h = harness(true);
        h.get_by_role_and_label(Role::Button, "Dentro").click();
        h.run_steps(3);
        assert_eq!(h.state().tocchi, 1);
        assert!(h.state().foglio.aperto());
        assert!(h.query_by_label("Dentro").is_some());
    }

    #[test]
    fn un_tocco_fuori_lo_chiude() {
        let mut h = harness(true);
        tocco(&mut h, egui::pos2(200.0, 60.0));
        assert!(!h.state().foglio.aperto());
        assert!(h.query_by_label("Dentro").is_none());
        assert_eq!(h.state().tocchi, 0);
    }

    #[test]
    fn un_tocco_sul_bordo_del_foglio_non_lo_chiude() {
        let mut h = harness(true);
        let titolo = h.get_by_label("Formato").rect();
        tocco(&mut h, egui::pos2(SCHERMO.x - 8.0, titolo.center().y));
        assert!(h.state().foglio.aperto(), "un tocco sul foglio, fuori dai bottoni, l'ha chiuso");
    }

    #[test]
    fn indietro_lo_chiude_e_lo_dice() {
        let mut h = harness(true);
        assert!(h.state_mut().foglio.indietro(), "Indietro col foglio aperto non è stato usato");
        h.run_steps(3);
        assert!(!h.state().foglio.aperto());
        assert!(h.query_by_label("Dentro").is_none());
        // Chiuso, Indietro non è suo: va a chi viene dopo.
        assert!(!h.state_mut().foglio.indietro());
    }

    #[test]
    fn un_contenuto_alto_resta_tutto_sullo_schermo() {
        let mut foglio = FoglioDalFondo::new(Id::new("alto"));
        foglio.apri();
        let mut h = Harness::builder().with_size(SCHERMO).build_ui_state(
            |ui, f: &mut FoglioDalFondo| {
                f.mostra(ui.ctx(), "Formato", |ui| {
                    for i in 0..6 {
                        ui.label(format!("Riga {i}"));
                        ui.add(egui::Button::new(format!("Bottone {i}")).min_size(egui::vec2(100.0, 48.0)));
                        ui.add_space(6.0);
                    }
                });
            },
            foglio,
        );
        h.run_steps(6);
        let ultimo = h.get_by_label("Bottone 5").rect();
        assert!(ultimo.max.y <= SCHERMO.y, "l'ultimo bottone esce dallo schermo: {ultimo:?}");
    }

    #[test]
    fn un_contenuto_troppo_alto_scorre_e_lascia_vedere_il_documento() {
        let mut foglio = FoglioDalFondo::new(Id::new("troppo"));
        foglio.apri();
        let mut h = Harness::builder().with_size(SCHERMO).build_ui_state(
            |ui, f: &mut FoglioDalFondo| {
                f.mostra(ui.ctx(), "Formato", |ui| {
                    for i in 0..30 {
                        ui.add(egui::Button::new(format!("Bottone {i}")).min_size(egui::vec2(100.0, 48.0)));
                    }
                });
            },
            foglio,
        );
        h.run_steps(6);
        let titolo = h.get_by_label("Formato").rect();
        assert!(titolo.min.y > SCHERMO.y * (1.0 - ALTEZZA_MASSIMA) - 40.0, "il foglio copre tutto: {titolo:?}");
        assert!(h.get_by_label("Bottone 0").rect().min.y > titolo.max.y);
    }

    #[test]
    fn col_tema_ha_la_lineetta_il_titolo_in_fraunces_e_la_x() {
        use crate::prova::{banco, famiglia_del_testo, riempiti};
        use crate::tema;
        let mut foglio = FoglioDalFondo::new(Id::new("tema"));
        foglio.apri();
        let mut h = banco(
            |ui, f: &mut FoglioDalFondo| {
                f.mostra(ui.ctx(), "Formato", |ui| ui.label("Dentro"));
            },
            foglio,
        );
        h.run_steps(4);
        assert_eq!(famiglia_del_testo(&h, "Formato"), Some(tema::Carattere::Titolo.famiglia()));
        let lineette = riempiti(&h, tema::LINEETTA);
        assert_eq!(lineette.len(), 1, "{lineette:?}");
        assert_eq!(lineette[0].size(), egui::vec2(36.0, 4.0));
        assert!((lineette[0].center().x - SCHERMO.x / 2.0).abs() < 0.5);
        assert_eq!(riempiti(&h, tema::VELO_SCURO).len(), 1, "il velo sullo schermo");
        let titolo = h.get_by_label("Formato").rect();
        let chiudi = h.get_by_label("Chiudi").rect();
        assert_eq!(chiudi.right(), SCHERMO.x - tema::MARGINE_LARGO);
        assert!((chiudi.center().y - titolo.center().y).abs() < 2.0, "{chiudi:?} {titolo:?}");
        assert!(titolo.top() > lineette[0].bottom() + 8.0);
        // Il foglio bianco, largo quanto lo schermo, con gli angoli in alto tondi.
        let fondo = crate::prova::forme(&h)
            .into_iter()
            .find_map(|s| match s {
                egui::Shape::Rect(r) if r.fill == tema::FOGLIO && r.rect.contains_rect(titolo) => Some(r),
                _ => None,
            })
            .expect("il foglio");
        assert_eq!(fondo.rect.width(), SCHERMO.x);
        assert_eq!(fondo.corner_radius, CornerRadius { nw: tema::RAGGIO_FOGLIO, ne: tema::RAGGIO_FOGLIO, sw: 0, se: 0 });
        h.get_by_label("Chiudi").click();
        h.run_steps(3);
        assert!(!h.state().aperto());
    }

    #[test]
    fn senza_tema_niente_x_e_niente_panico() {
        let h = harness(true);
        assert!(h.query_by_label("Chiudi").is_none());
        h.get_by_label("Formato");
    }

    #[test]
    fn dentro_il_foglio_i_colori_sono_chiari_anche_col_tema_scuro() {
        let mut foglio = FoglioDalFondo::new(Id::new("scuro"));
        foglio.apri();
        let mut h = Harness::builder().with_size(SCHERMO).with_theme(egui::Theme::Dark).build_ui_state(
            |ui, (f, colore): &mut (FoglioDalFondo, Option<egui::Color32>)| {
                if let Some(c) = f.mostra(ui.ctx(), "Formato", |ui| ui.visuals().text_color()) {
                    *colore = Some(c);
                }
            },
            (foglio, None),
        );
        h.run_steps(3);
        assert_eq!(h.state().1, Some(crate::tema::INCHIOSTRO));
    }

    #[test]
    fn sale_dal_fondo() {
        // Un frame ogni 1/60 di secondo; il foglio, chiuso, si mostra a ogni frame come fa l'app.
        let mut h = Harness::builder().with_size(SCHERMO).with_step_dt(1.0 / 60.0).build_ui_state(
            |ui, f: &mut FoglioDalFondo| {
                f.mostra(ui.ctx(), "Formato", |ui| ui.label("Dentro"));
            },
            FoglioDalFondo::new(Id::new("sale")),
        );
        h.run_steps(2);
        h.state_mut().apri();
        // Il primo frame è quello in cui egui misura l'area senza disegnarla.
        h.run_steps(2);
        let primo = h.get_by_label("Formato").rect().min.y;
        h.run_steps(30);
        let ultimo = h.get_by_label("Formato").rect().min.y;
        assert!(primo > ultimo + 20.0, "il foglio non sale: {primo} poi {ultimo}");
    }
}
