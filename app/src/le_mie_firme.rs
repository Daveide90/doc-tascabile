//! «Le mie firme» (7/11/2026, segnalazione dell'autore: «manca un gestionale firme, non si può
//! eliminare una firma se salvata»). Le firme e le iniziali salvate in Tascabile, ognuna con la
//! sua immagine e «Elimina», e in fondo «Elimina tutte». Si apre dal foglio della firma dei PDF e
//! dalle Impostazioni. Eliminare chiede conferma; nei documenti dove la firma è già messa resta.
//!
//! Nella diagnosi solo quante ne restano, mai tratti o pixel.

use eframe::egui::{self, Rect, RichText, Sense, Stroke, StrokeKind, WidgetInfo, WidgetType, pos2, vec2};
use tascabile_platform::diagnosi;
use tascabile_touch::tema::{self, Carattere};
use tascabile_touch::{Dialogo, FoglioDalFondo, Peso, Pulsante, PulsanteIcona, RispostaDialogo, icone};

use crate::TascabileApp;
use crate::pdf_firma::{LE_MIE_FIRME, disegna_firma};

/// «Elimina» davanti al nome della firma, per l'accessibilità e le prove: «Elimina Firma 2».
pub(crate) const ELIMINA: &str = "Elimina";
pub(crate) const ELIMINA_TUTTE: &str = "Elimina tutte";
pub(crate) const ELIMINARE_QUESTA: &str = "Eliminare questa firma?";
pub(crate) const ELIMINARE_TUTTE: &str = "Eliminare tutte le firme salvate?";
const SPIEGA: &str =
    "Le firme e le iniziali salvate in Tascabile, solo su questo telefono. Eliminarle non le toglie dai documenti dove le hai già messe.";
const SPIEGA_ELIMINA: &str = "La togli da Tascabile. Nei documenti dove l'hai già messa resta.";
const SPIEGA_ELIMINA_TUTTE: &str = "Le togli tutte da Tascabile, firme e iniziali. Nei documenti dove le hai già messe restano.";
const NESSUNA: &str = "Nessuna firma salvata. Ne salvi una la prima volta che firmi un PDF.";

/// Che cosa aspetta la conferma.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DaEliminare {
    Una(usize),
    Tutte,
}

/// Il foglio di «Le mie firme» e la conferma aperta.
pub(crate) struct StatoLeMieFirme {
    pub(crate) foglio: FoglioDalFondo,
    conferma: Option<DaEliminare>,
}

impl Default for StatoLeMieFirme {
    fn default() -> Self {
        StatoLeMieFirme { foglio: FoglioDalFondo::new(egui::Id::new("foglio_le_mie_firme")), conferma: None }
    }
}

impl StatoLeMieFirme {
    /// Indietro: chiude la conferma, poi il foglio (vero se ha chiuso qualcosa).
    pub(crate) fn indietro(&mut self) -> bool {
        if self.conferma.take().is_some() {
            diagnosi("indietro: conferma di eliminare la firma chiusa");
            return true;
        }
        self.foglio.indietro()
    }
}

/// Che cosa si è toccato nel foglio.
enum Tocco {
    Elimina(usize),
    EliminaTutte,
}

impl TascabileApp {
    /// Apre «Le mie firme».
    pub(crate) fn apri_le_mie_firme(&mut self) {
        let n = self.firme().voci().len();
        diagnosi(format!("firma: le mie firme aperte, {n} salvate"));
        self.le_mie_firme.conferma = None;
        self.le_mie_firme.foglio.apri();
    }

    /// Il foglio di «Le mie firme» e la sua conferma, sopra la schermata di adesso.
    pub(crate) fn mostra_le_mie_firme(&mut self, ctx: &egui::Context) {
        if self.le_mie_firme.foglio.aperto() {
            self.foglio_le_mie_firme(ctx);
        }
        self.conferma_le_mie_firme(ctx);
    }

    fn foglio_le_mie_firme(&mut self, ctx: &egui::Context) {
        let n = self.firme().voci().len();
        let voci: Vec<_> = (0..n)
            .map(|i| {
                let t = self.firme.texture(ctx, i);
                let posto = self.firme.posto(i);
                let v = &self.firme.voci()[i];
                (v.firma.clone(), t, v.nome(posto))
            })
            .collect();
        let tocco = self
            .le_mie_firme
            .foglio
            .mostra(ctx, LE_MIE_FIRME, |ui| {
                let ctx = ui.ctx().clone();
                let mut tocco = None;
                ui.label(RichText::new(if voci.is_empty() { NESSUNA } else { SPIEGA }).font(Carattere::Testo.font(&ctx, 15.0)).color(tema::MATITA));
                ui.add_space(10.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                for (i, (firma, texture, nome)) in voci.iter().enumerate() {
                    let largo = ui.available_width();
                    let (rect, r) = ui.allocate_exact_size(vec2(largo, 60.0), Sense::hover());
                    r.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, nome));
                    let box_ = Rect::from_min_size(pos2(rect.left() + 4.0, rect.top() + 8.0), vec2(110.0, 44.0));
                    ui.painter().rect(box_, 8.0, tema::FOGLIO, Stroke::new(1.0, tema::FILO), StrokeKind::Inside);
                    disegna_firma(ui.painter(), box_.shrink(5.0), firma, texture.as_ref());
                    let g = ui.painter().layout_no_wrap(nome.clone(), Carattere::Grassetto.font(&ctx, 16.0), tema::INCHIOSTRO);
                    ui.painter().galley(pos2(box_.right() + 14.0, rect.center().y - g.size().y / 2.0), g, tema::INCHIOSTRO);
                    let cestino = Rect::from_center_size(pos2(rect.right() - 26.0, rect.center().y), vec2(48.0, 48.0));
                    let etichetta = format!("{ELIMINA} {nome}");
                    if ui.put(cestino, PulsanteIcona::new(icone::DELETE, &etichetta)).clicked() {
                        tocco = Some(Tocco::Elimina(i));
                    }
                    if i + 1 < voci.len() {
                        ui.painter().rect_filled(Rect::from_min_max(pos2(rect.left(), rect.bottom() - 1.0), rect.max), 0.0, tema::FILO);
                    }
                }
                if voci.len() > 1 {
                    ui.add_space(16.0);
                    if ui.add(Pulsante::new(ELIMINA_TUTTE, Peso::Testo)).clicked() {
                        tocco = Some(Tocco::EliminaTutte);
                    }
                }
                ui.add_space(8.0);
                tocco
            })
            .flatten();
        match tocco {
            Some(Tocco::Elimina(i)) => {
                diagnosi("firma: si chiede se eliminare una salvata");
                self.le_mie_firme.conferma = Some(DaEliminare::Una(i));
            }
            Some(Tocco::EliminaTutte) => {
                diagnosi("firma: si chiede se eliminare tutte le salvate");
                self.le_mie_firme.conferma = Some(DaEliminare::Tutte);
            }
            None => {}
        }
    }

    /// «Eliminare questa firma?»: «Annulla» (o un tocco fuori, o Indietro) la lascia; «Elimina» la
    /// toglie. Il foglio resta aperto con quelle che restano; senza più firme si chiude.
    fn conferma_le_mie_firme(&mut self, ctx: &egui::Context) {
        let Some(da) = self.le_mie_firme.conferma else { return };
        let (titolo, testo) = match da {
            DaEliminare::Una(_) => (ELIMINARE_QUESTA, SPIEGA_ELIMINA),
            DaEliminare::Tutte => (ELIMINARE_TUTTE, SPIEGA_ELIMINA_TUTTE),
        };
        let risposta = Dialogo::new(egui::Id::new("elimina_firma"), titolo)
            .testo(testo)
            .pulsante("Annulla", Peso::Testo)
            .pulsante(ELIMINA, Peso::Principale)
            .mostra(ctx);
        let Some(risposta) = risposta else { return };
        self.le_mie_firme.conferma = None;
        if risposta != RispostaDialogo::Pulsante(1) {
            diagnosi("firma: eliminazione annullata");
            return;
        }
        match da {
            DaEliminare::Una(i) => self.elimina_firma_salvata(i),
            DaEliminare::Tutte => self.cancella_firma_salvata(),
        }
        if !self.firme().qualcuna() {
            self.le_mie_firme.foglio.chiudi();
        }
    }
}
