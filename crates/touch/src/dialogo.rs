//! Il dialogo sopra il velo scuro e l'avviso breve in basso.

use std::time::Duration;

use egui::{Align, Align2, Area, Color32, Context, Frame, Id, Layout, Margin, Modal, Order, Rect, Sense, WidgetInfo, WidgetType, pos2, vec2};

use crate::elenco::Scheda;
use crate::pulsanti::{Peso, Pulsante};
use crate::tema::{self, Carattere};

/// La larghezza massima del dialogo, in dp.
const LARGO: f32 = 360.0;
/// Il margine interno del dialogo e la distanza minima dai bordi dello schermo.
const BORDO: i8 = 24;

/// Che cosa si è scelto nel dialogo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RispostaDialogo {
    /// Un pulsante, nell'ordine in cui sono stati aggiunti.
    Pulsante(usize),
    /// Un tocco fuori dal dialogo, sul velo.
    Fuori,
}

/// Un dialogo (modifiche non salvate, lavoro da recuperare, errori): un foglio bianco al centro,
/// sopra il velo scuro, col titolo in Fraunces, un testo grigio e i pulsanti in fondo a destra
/// (uno sotto l'altro se in fila non stanno). Chi lo usa tiene se è aperto e lo chiude a ogni
/// risposta; Indietro lo gestisce chi lo usa.
#[derive(Debug, Clone)]
pub struct Dialogo<'a> {
    id: Id,
    titolo: &'a str,
    testo: Option<&'a str>,
    pulsanti: Vec<(&'a str, Peso)>,
}

impl<'a> Dialogo<'a> {
    pub fn new(id: impl Into<Id>, titolo: &'a str) -> Self {
        Self { id: id.into(), titolo, testo: None, pulsanti: Vec::new() }
    }

    pub fn testo(mut self, testo: &'a str) -> Self {
        self.testo = Some(testo);
        self
    }

    /// Un pulsante; stanno nell'ordine in cui si aggiungono, il principale di solito per ultimo.
    pub fn pulsante(mut self, etichetta: &'a str, peso: Peso) -> Self {
        self.pulsanti.push((etichetta, peso));
        self
    }

    pub fn mostra(self, ctx: &Context) -> Option<RispostaDialogo> {
        self.mostra_con(ctx, |_| ()).0
    }

    /// Come [`mostra`](Self::mostra), con `contenuto` fra il testo e i pulsanti (un campo, come
    /// la password di un PDF), largo quanto il dialogo; restituisce anche quello che dà lui.
    pub fn mostra_con<R>(self, ctx: &Context, contenuto: impl FnOnce(&mut egui::Ui) -> R) -> (Option<RispostaDialogo>, Option<R>) {
        let zona = ctx.content_rect();
        let mut dato = None;
        let interno = (zona.width() - 2.0 * f32::from(BORDO)).min(LARGO) - 2.0 * f32::from(BORDO);
        let mut scelta = None;
        let risposta = Modal::new(self.id).backdrop_color(tema::VELO_SCURO).frame(Frame::NONE).show(ctx, |ui| {
            Scheda::new().raggio(tema::RAGGIO_FOGLIO).ombra(tema::OMBRA_MENU).margine(Margin::same(BORDO)).show(ui, |ui| {
                ui.set_width(interno);
                ui.spacing_mut().item_spacing = vec2(tema::SPAZIO, 0.0);
                let ctx = ui.ctx().clone();
                let titolo = ui.painter().layout(self.titolo.to_owned(), Carattere::Titolo.font(&ctx, 22.0), tema::INCHIOSTRO, interno);
                let (r, risposta) = ui.allocate_exact_size(titolo.size(), Sense::hover());
                ui.painter().galley(r.min, titolo, Color32::PLACEHOLDER);
                risposta.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, self.titolo));
                if let Some(testo) = self.testo {
                    ui.add_space(12.0);
                    let g = ui.painter().layout(testo.to_owned(), Carattere::Testo.font(&ctx, 16.0), tema::MATITA, interno);
                    let (r, risposta) = ui.allocate_exact_size(g.size(), Sense::hover());
                    ui.painter().galley(r.min, g, Color32::PLACEHOLDER);
                    risposta.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, testo));
                }
                ui.add_space(16.0);
                dato = Some(contenuto(ui));
                ui.add_space(8.0);
                let font = Carattere::Grassetto.font(&ctx, 15.0);
                let larghi: f32 =
                    self.pulsanti.iter().map(|(t, _)| ui.painter().layout_no_wrap((*t).to_owned(), font.clone(), tema::BLU).size().x + 40.0).sum();
                let in_fila = larghi + tema::SPAZIO * self.pulsanti.len().saturating_sub(1) as f32 <= interno;
                if in_fila {
                    // Alta quanto un pulsante: una fila da destra centrata prenderebbe tutta
                    // l'altezza del dialogo del frame prima, che così non si accorcerebbe più
                    // (la password: «Password sbagliata» che se ne va).
                    ui.allocate_ui_with_layout(vec2(interno, tema::PULSANTE), Layout::right_to_left(Align::Center), |ui| {
                        for (i, &(t, peso)) in self.pulsanti.iter().enumerate().rev() {
                            if ui.add(Pulsante::new(t, peso)).clicked() {
                                scelta = Some(RispostaDialogo::Pulsante(i));
                            }
                        }
                    });
                } else {
                    for (i, &(t, peso)) in self.pulsanti.iter().enumerate() {
                        if i > 0 {
                            ui.add_space(tema::SPAZIO);
                        }
                        if ui.add(Pulsante::new(t, peso).larghezza(interno)).clicked() {
                            scelta = Some(RispostaDialogo::Pulsante(i));
                        }
                    }
                }
            });
        });
        if scelta.is_none() && risposta.backdrop_response.clicked() {
            scelta = Some(RispostaDialogo::Fuori);
        }
        (scelta, dato)
    }
}

/// Quanto resta sullo schermo un avviso breve, in secondi.
pub const DURATA_AVVISO: f64 = 3.0;
/// Quanto resta un avviso con un'azione da toccare (come «Condividi» del PDF pronto).
pub const DURATA_CON_AZIONE: f64 = 6.0;

/// Un avviso breve (il «toast» di Android): una pillola scura in basso con un testo bianco, che
/// sparisce da sola dopo [`DURATA_AVVISO`] secondi e non si prende i tocchi. Con un'azione (lo
/// «snackbar»), a destra c'è il pulsante col testo blu tenue: l'avviso resta
/// [`DURATA_CON_AZIONE`] secondi e si prende i tocchi solo lì sopra.
#[derive(Debug, Clone)]
pub struct AvvisoBreve {
    id: Id,
    testo: Option<String>,
    azione: Option<String>,
    /// Quando si è visto la prima volta, nel tempo di egui.
    dal: Option<f64>,
}

impl AvvisoBreve {
    pub fn new(id: impl Into<Id>) -> Self {
        Self { id: id.into(), testo: None, azione: None, dal: None }
    }

    /// Mostra questo testo da adesso, al posto di quello che c'era.
    pub fn avvisa(&mut self, testo: impl Into<String>) {
        self.testo = Some(testo.into());
        self.azione = None;
        self.dal = None;
    }

    /// Mostra questo testo con un'azione da toccare, al posto di quello che c'era.
    pub fn avvisa_con_azione(&mut self, testo: impl Into<String>, azione: impl Into<String>) {
        self.avvisa(testo);
        self.azione = Some(azione.into());
    }

    pub fn visibile(&self) -> bool {
        self.testo.is_some()
    }

    /// Toglie l'avviso subito.
    pub fn togli(&mut self) {
        self.testo = None;
        self.azione = None;
        self.dal = None;
    }

    /// Disegna l'avviso, se c'è, centrato e col fondo 16 dp sopra la y `fondo` (per esempio sopra
    /// la barra flottante). Va chiamata a ogni frame. Vero se è stata toccata l'azione: l'avviso
    /// allora se ne va.
    pub fn mostra(&mut self, ctx: &Context, fondo: f32) -> bool {
        let Some(testo) = self.testo.clone() else { return false };
        let durata = if self.azione.is_some() { DURATA_CON_AZIONE } else { DURATA_AVVISO };
        let ora = ctx.input(|i| i.time);
        let passato = ora - *self.dal.get_or_insert(ora);
        if passato >= durata {
            self.togli();
            return false;
        }
        ctx.request_repaint_after(Duration::from_secs_f64(durata - passato));
        let zona = ctx.content_rect();
        let massimo = (zona.width() - 2.0 * tema::MARGINE_LARGO).min(420.0);
        let azione = self.azione.clone();
        // Sopra tutto: un foglio o una schermata a tutto schermo aperti dopo l'avviso (come «Sistema
        // i bordi» della scansione) non lo devono coprire.
        let toccata = Area::new(self.id)
            .order(Order::Tooltip)
            .interactable(azione.is_some())
            .pivot(Align2::CENTER_BOTTOM)
            .fixed_pos(pos2(zona.center().x, fondo - 16.0))
            .show(ctx, |ui| {
                let grassetto = Carattere::Grassetto.font(ui.ctx(), 15.0);
                let pulsante = azione.as_ref().map(|a| ui.painter().layout_no_wrap(a.clone(), grassetto, tema::BLU_TENUE));
                let per_l_azione = pulsante.as_ref().map_or(0.0, |g| g.size().x + 24.0 + 4.0);
                let g = ui.painter().layout(testo.clone(), Carattere::Testo.font(ui.ctx(), 15.0), tema::FOGLIO, massimo - 40.0 - per_l_azione);
                let alto = (g.size().y + 28.0).max(tema::PULSANTE);
                let largo = if pulsante.is_some() { 20.0 + g.size().x + 8.0 + per_l_azione } else { g.size().x + 40.0 };
                let (rect, r) = ui.allocate_exact_size(vec2(largo, alto), Sense::hover());
                let raggio = (tema::PULSANTE / 2.0) as u8;
                tema::dipingi_ombra(ui.painter(), rect, raggio, &tema::OMBRA_CONTESTO);
                ui.painter().rect_filled(rect, raggio, tema::SCURO);
                let x = if pulsante.is_some() { rect.left() + 20.0 } else { rect.center().x - g.size().x / 2.0 };
                let posto_testo = Rect::from_min_size(pos2(x, rect.center().y - g.size().y / 2.0), g.size());
                ui.painter().galley(posto_testo.min, g, Color32::PLACEHOLDER);
                // L'etichetta per l'accessibilità e le prove: la pillola intera, o solo il testo
                // accanto all'azione.
                if azione.is_none() {
                    r.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, &testo));
                } else {
                    let etichetta = ui.interact(posto_testo, self.id.with("testo"), Sense::hover());
                    etichetta.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, &testo));
                }
                let (Some(p), Some(a)) = (pulsante, azione.as_ref()) else { return false };
                let posto = Rect::from_min_max(
                    pos2(rect.right() - 4.0 - p.size().x - 24.0, rect.center().y - tema::PULSANTE / 2.0),
                    pos2(rect.right() - 4.0, rect.center().y + tema::PULSANTE / 2.0),
                );
                let b = ui.interact(posto, self.id.with("azione"), Sense::click());
                b.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, a));
                if b.is_pointer_button_down_on() {
                    ui.painter().rect_filled(posto, raggio, tema::PREMUTO_SCURO);
                }
                ui.painter().galley(posto.center() - p.size() / 2.0, p, Color32::PLACEHOLDER);
                b.clicked()
            })
            .inner;
        if toccata {
            self.togli();
        }
        toccata
    }
}

#[cfg(test)]
mod test {
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    use super::*;
    use crate::prova::{SCHERMO, banco, colore_del_testo, famiglia_del_testo, rettangolo_del_testo, riempiti, tocco};

    fn salvare(ui: &mut egui::Ui, risposte: &mut Vec<RispostaDialogo>) {
        let r = Dialogo::new(egui::Id::new("salvare"), "Salvare le modifiche?")
            .testo("Il documento ha modifiche che non sono state salvate.")
            .pulsante("Annulla", Peso::Testo)
            .pulsante("Non salvare", Peso::Testo)
            .pulsante("Salva", Peso::Principale)
            .mostra(ui.ctx());
        risposte.extend(r);
    }

    #[test]
    fn il_dialogo_sta_al_centro_sopra_il_velo() {
        let h = banco(salvare, Vec::new());
        let schermo = egui::Rect::from_min_size(egui::Pos2::ZERO, SCHERMO);
        assert_eq!(riempiti(&h, tema::VELO_SCURO), vec![schermo]);
        let titolo = rettangolo_del_testo(&h, "Salvare le modifiche?").unwrap();
        let foglio: Vec<_> = riempiti(&h, tema::FOGLIO).into_iter().filter(|r| r.contains_rect(titolo)).collect();
        assert_eq!(foglio.len(), 1, "{foglio:?}");
        assert!((foglio[0].center().x - SCHERMO.x / 2.0).abs() < 1.0 && (foglio[0].center().y - SCHERMO.y / 2.0).abs() < 1.0, "{foglio:?}");
        assert!(foglio[0].width() <= SCHERMO.x - 2.0 * 24.0);
        assert_eq!(famiglia_del_testo(&h, "Salvare le modifiche?"), Some(Carattere::Titolo.famiglia()));
        assert_eq!(colore_del_testo(&h, "Il documento ha modifiche che non sono state salvate."), Some(tema::MATITA));
        // I pulsanti in fila in fondo a destra, nell'ordine dato.
        let annulla = h.get_by_label("Annulla").rect();
        let non_salvare = h.get_by_label("Non salvare").rect();
        let salva = h.get_by_label("Salva").rect();
        assert!(annulla.right() <= non_salvare.left() && non_salvare.right() <= salva.left());
        assert_eq!(annulla.center().y, salva.center().y);
        assert!((salva.right() - (foglio[0].right() - 24.0)).abs() < 1.0, "{salva:?} {foglio:?}");
        assert_eq!(colore_del_testo(&h, "Salva"), Some(tema::FOGLIO));
    }

    #[test]
    fn il_contenuto_sta_fra_il_testo_e_i_pulsanti() {
        let h = banco(
            |ui, visto: &mut Option<f32>| {
                let (_, dato) = Dialogo::new(egui::Id::new("con"), "PDF protetto")
                    .testo("Si apre con una password.")
                    .pulsante("Apri", Peso::Principale)
                    .mostra_con(ui.ctx(), |ui| {
                        ui.label("Il campo");
                        ui.available_width()
                    });
                *visto = dato;
            },
            None,
        );
        let testo = rettangolo_del_testo(&h, "Si apre con una password.").unwrap();
        let campo = rettangolo_del_testo(&h, "Il campo").unwrap();
        let apri = h.get_by_label("Apri").rect();
        assert!(testo.bottom() + 16.0 <= campo.top() + 0.5 && campo.bottom() + 8.0 <= apri.top() + 0.5, "{testo:?} {campo:?} {apri:?}");
        // Largo quanto il dialogo dentro i margini.
        assert_eq!(*h.state(), Some((SCHERMO.x - 2.0 * 24.0).min(360.0) - 2.0 * 24.0));
    }

    #[test]
    fn il_dialogo_si_accorcia_quando_il_contenuto_cala() {
        // Una riga in più per un frame, poi via: il foglio torna alto come prima.
        let mut h = banco(
            |ui, riga: &mut bool| {
                Dialogo::new(egui::Id::new("cala"), "PDF protetto").pulsante("Apri", Peso::Principale).mostra_con(ui.ctx(), |ui| {
                    if *riga {
                        ui.add_space(40.0);
                    }
                });
            },
            false,
        );
        let fondo = |h: &egui_kittest::Harness<'_, bool>| {
            let apri = h.get_by_label("Apri").rect();
            riempiti(h, tema::FOGLIO).into_iter().filter(|r| r.contains_rect(apri)).map(|r| r.bottom() - apri.bottom()).fold(f32::MAX, f32::min)
        };
        let prima = fondo(&h);
        *h.state_mut() = true;
        h.run_steps(3);
        *h.state_mut() = false;
        h.run_steps(3);
        assert!((fondo(&h) - prima).abs() < 0.5, "{prima} {}", fondo(&h));
        assert!((prima - 24.0).abs() < 0.5, "{prima}");
    }

    #[test]
    fn un_pulsante_risponde_e_un_tocco_fuori_anche() {
        let mut h = banco(salvare, Vec::new());
        h.get_by_label("Non salvare").click();
        h.run_steps(2);
        tocco(&mut h, egui::pos2(20.0, 40.0));
        assert_eq!(h.state(), &vec![RispostaDialogo::Pulsante(1), RispostaDialogo::Fuori]);
    }

    #[test]
    fn pulsanti_che_non_stanno_in_fila_vanno_uno_sotto_l_altro() {
        let h = banco(
            |ui, _: &mut ()| {
                Dialogo::new(egui::Id::new("lungo"), "Lavoro da recuperare")
                    .pulsante("Riapri la copia di recupero", Peso::Principale)
                    .pulsante("Scarta la copia di recupero", Peso::Testo)
                    .mostra(ui.ctx());
            },
            (),
        );
        let riapri = h.get_by_label("Riapri la copia di recupero").rect();
        let scarta = h.get_by_label("Scarta la copia di recupero").rect();
        assert!(riapri.bottom() <= scarta.top(), "{riapri:?} {scarta:?}");
        assert_eq!((riapri.left(), riapri.right()), (scarta.left(), scarta.right()));
    }

    fn banco_dell_avviso() -> Harness<'static, AvvisoBreve> {
        let mut h = Harness::builder().with_size(SCHERMO).with_step_dt(0.25).build_ui_state(
            |ui, a: &mut AvvisoBreve| {
                ui.label("Sotto l'avviso");
                a.mostra(ui.ctx(), 800.0);
            },
            AvvisoBreve::new(egui::Id::new("avviso")),
        );
        tema::installa(&h.ctx);
        h.run_steps(2);
        h
    }

    #[test]
    fn l_avviso_breve_si_vede_in_basso_e_poi_sparisce() {
        let mut h = banco_dell_avviso();
        assert!(h.query_by_label("Copia salvata in Documenti").is_none());
        h.state_mut().avvisa("Copia salvata in Documenti");
        h.run_steps(2);
        let avviso = h.get_by_label("Copia salvata in Documenti").rect();
        assert!(avviso.bottom() <= 800.0 && avviso.bottom() > 760.0, "{avviso:?}");
        assert!((avviso.center().x - SCHERMO.x / 2.0).abs() < 1.0);
        assert_eq!(riempiti(&h, tema::SCURO), vec![avviso]);
        assert_eq!(colore_del_testo(&h, "Copia salvata in Documenti"), Some(tema::FOGLIO));
        // L'avviso non si prende i tocchi: il resto dello schermo resta usabile.
        assert!(h.state().visibile());
        h.run_steps((DURATA_AVVISO / 0.25) as usize + 2);
        assert!(h.query_by_label("Copia salvata in Documenti").is_none());
        assert!(!h.state().visibile());
    }

    /// Un'area a tutto schermo in primo piano aperta dopo l'avviso (come «Sistema i bordi» della
    /// scansione, 0.8.3) non lo copre: l'avviso sta sopra.
    #[test]
    fn l_avviso_sta_sopra_un_area_aperta_dopo() {
        let mut h = Harness::builder().with_size(SCHERMO).with_step_dt(0.25).build_ui_state(
            |ui, (a, aperta): &mut (AvvisoBreve, bool)| {
                a.mostra(ui.ctx(), 800.0);
                if *aperta {
                    egui::Area::new(egui::Id::new("tutto schermo")).order(Order::Foreground).fixed_pos(egui::Pos2::ZERO).show(ui.ctx(), |ui| {
                        ui.painter().rect_filled(egui::Rect::from_min_size(egui::Pos2::ZERO, SCHERMO), 0.0, tema::CARTA);
                        ui.allocate_rect(egui::Rect::from_min_size(egui::Pos2::ZERO, SCHERMO), egui::Sense::click());
                    });
                }
            },
            (AvvisoBreve::new(egui::Id::new("avviso")), false),
        );
        tema::installa(&h.ctx);
        // Con un'azione l'avviso prende i tocchi: il livello che li prende è quello di sopra.
        h.state_mut().0.avvisa_con_azione("Ho preso le prime 30 foto", "Vedi");
        h.run_steps(2);
        h.state_mut().1 = true;
        h.run_steps(2);
        let centro = h.get_by_label("Ho preso le prime 30 foto").rect().center();
        let sopra = h.ctx.layer_id_at(centro).expect("un livello sotto il punto");
        assert_eq!(sopra.id, egui::Id::new("avviso"), "l'avviso è coperto da {sopra:?}");
    }

    #[test]
    fn l_avviso_con_l_azione_si_tocca_e_resta_di_piu() {
        use egui::accesskit::Role;
        let mut h = Harness::builder().with_size(SCHERMO).with_step_dt(0.25).build_ui_state(
            |ui, (a, toccate): &mut (AvvisoBreve, u32)| {
                ui.label("Sotto l'avviso");
                if a.mostra(ui.ctx(), 800.0) {
                    *toccate += 1;
                }
            },
            (AvvisoBreve::new(egui::Id::new("avviso")), 0),
        );
        tema::installa(&h.ctx);
        h.run_steps(2);
        h.state_mut().0.avvisa_con_azione("PDF pronto in Documenti/Tascabile", "Condividi");
        h.run_steps(2);
        let testo = h.get_by_label("PDF pronto in Documenti/Tascabile").rect();
        let azione = h.get_by_role_and_label(Role::Button, "Condividi").rect();
        let pillola = riempiti(&h, tema::SCURO);
        assert_eq!(pillola.len(), 1, "{pillola:?}");
        assert!(pillola[0].contains_rect(azione) && testo.right() <= azione.left(), "{pillola:?} {testo:?} {azione:?}");
        assert!(azione.height() >= tema::PULSANTE && pillola[0].bottom() <= 800.0);
        assert_eq!(colore_del_testo(&h, "Condividi"), Some(tema::BLU_TENUE));
        // Resta più di un avviso semplice, per dare il tempo di toccare.
        h.run_steps((DURATA_AVVISO / 0.25) as usize + 2);
        h.get_by_label("PDF pronto in Documenti/Tascabile");
        h.get_by_role_and_label(Role::Button, "Condividi").click();
        h.run_steps(2);
        assert_eq!(h.state().1, 1);
        assert!(h.query_by_label("PDF pronto in Documenti/Tascabile").is_none(), "toccata l'azione, l'avviso se ne va");
        // Senza tocchi se ne va da solo.
        h.state_mut().0.avvisa_con_azione("Di nuovo", "Condividi");
        h.run_steps((DURATA_CON_AZIONE / 0.25) as usize + 2);
        assert!(!h.state().0.visibile());
    }

    #[test]
    fn un_avviso_nuovo_riparte_da_capo() {
        let mut h = banco_dell_avviso();
        h.state_mut().avvisa("Primo");
        h.run_steps((DURATA_AVVISO / 0.25) as usize - 2);
        h.state_mut().avvisa("Secondo");
        h.run_steps(4);
        assert!(h.query_by_label("Primo").is_none());
        h.get_by_label("Secondo");
    }
}
