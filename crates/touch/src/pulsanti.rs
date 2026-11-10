//! I pulsanti del tema: il pulsante icona da 48 dp, il chip, il pulsante con l'etichetta e il
//! pulsante «Nuovo» flottante.

use egui::{Align2, Color32, CornerRadius, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2, Widget, WidgetInfo, WidgetType, vec2};

use crate::disegno::Riga;
use crate::tema::{self, Carattere};

/// Le informazioni per l'accessibilità (e per le prove) di un pulsante: con `acceso` è un
/// interruttore, acceso o spento.
pub(crate) fn info(r: &Response, abilitato: bool, acceso: Option<bool>, etichetta: &str) {
    r.widget_info(|| match acceso {
        Some(acceso) => WidgetInfo::selected(WidgetType::Button, abilitato, acceso, etichetta),
        None => WidgetInfo::labeled(WidgetType::Button, abilitato, etichetta),
    });
}

/// Un pulsante con un'icona, tondo, da 48 dp (`.ib`): normale, acceso (fondo blu tenue e icona
/// blu, come il grassetto attivo nella barra di formato) o spento (icona grigia, non si tocca).
/// Il segno può essere anche un testo, come «Aa».
#[derive(Debug, Clone)]
pub struct PulsanteIcona<'a> {
    segno: &'a str,
    etichetta: &'a str,
    carattere: Carattere,
    misura: f32,
    colore: Color32,
    acceso: Option<bool>,
    spento: bool,
    su_velo: bool,
}

impl<'a> PulsanteIcona<'a> {
    /// `etichetta` è il nome del pulsante per l'accessibilità e per le prove: l'icona non ha testo.
    pub fn new(icona: &'a str, etichetta: &'a str) -> Self {
        Self {
            segno: icona,
            etichetta,
            carattere: Carattere::Icona,
            misura: tema::ICONA,
            colore: tema::INCHIOSTRO,
            acceso: None,
            spento: false,
            su_velo: false,
        }
    }

    /// L'icona piena invece di quella vuota.
    pub fn piena(mut self) -> Self {
        self.carattere = Carattere::IconaPiena;
        self
    }

    /// Un testo al posto dell'icona, come «Aa» in Fraunces da 21 dp.
    pub fn testo(mut self, carattere: Carattere, misura: f32) -> Self {
        self.carattere = carattere;
        self.misura = misura;
        self
    }

    /// Il colore del segno a riposo (di solito inchiostro).
    pub fn colore(mut self, colore: Color32) -> Self {
        self.colore = colore;
        self
    }

    /// Un interruttore, acceso o no.
    pub fn acceso(mut self, acceso: bool) -> Self {
        self.acceso = Some(acceso);
        self
    }

    /// Spento: grigio e non si tocca (Ripeti senza niente da ripetere).
    pub fn spento(mut self, spento: bool) -> Self {
        self.spento = spento;
        self
    }

    /// Un segmento di un gruppo sul velo: acceso è bianco (l'ombra la mette il gruppo).
    pub(crate) fn su_velo(mut self) -> Self {
        self.su_velo = true;
        self
    }

    /// Disegna il pulsante in `rect`, già preso; `raggio` è quello del fondo.
    fn dipingi(&self, ui: &Ui, rect: Rect, raggio: u8, r: &Response) {
        let acceso = self.acceso == Some(true);
        let (fondo_acceso, premuto) = if self.su_velo { (tema::FOGLIO, tema::PREMUTO_VELO) } else { (tema::BLU_TENUE, tema::PREMUTO) };
        let fondo =
            if r.is_pointer_button_down_on() { Some(if acceso { tema::PREMUTO_VELO } else { premuto }) } else { acceso.then_some(fondo_acceso) };
        let p = ui.painter();
        if let Some(fondo) = fondo {
            p.rect_filled(rect, CornerRadius::same(raggio), fondo);
        }
        let colore = if self.spento {
            tema::SPENTO
        } else if acceso {
            tema::BLU
        } else {
            self.colore
        };
        p.text(rect.center(), Align2::CENTER_CENTER, self.segno, self.carattere.font(ui.ctx(), self.misura), colore);
    }

    fn senso(&self) -> Sense {
        if self.spento { Sense::hover() } else { Sense::click() }
    }

    /// Le informazioni per l'accessibilità e il disegno, nel posto già preso.
    fn finisci(self, ui: &Ui, rect: Rect, raggio: u8, r: Response) -> Response {
        info(&r, !self.spento, self.acceso, self.etichetta);
        if ui.is_rect_visible(rect) {
            self.dipingi(ui, rect, raggio, &r);
        }
        r
    }

    /// Prende il posto `rect`, fuori dal flusso di `ui` (barre, gruppi), e ci disegna il pulsante.
    pub(crate) fn mostra_in(self, ui: &mut Ui, rect: Rect, raggio: u8) -> Response {
        let r = ui.allocate_rect(rect, self.senso());
        self.finisci(ui, rect, raggio, r)
    }
}

impl Widget for PulsanteIcona<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, r) = ui.allocate_exact_size(Vec2::splat(tema::PULSANTE), self.senso());
        self.finisci(ui, rect, (tema::PULSANTE / 2.0) as u8, r)
    }
}

/// Un chip (`.chip`): un filtro da 34 dp col bordo sottile; acceso è pieno di inchiostro col testo
/// bianco.
#[derive(Debug, Clone)]
pub struct Chip<'a> {
    testo: &'a str,
    icona: Option<&'a str>,
    acceso: bool,
    margine: f32,
}

impl<'a> Chip<'a> {
    /// Il margine dentro un chip, i due lati insieme: 14 dp per lato.
    pub const MARGINE: f32 = 28.0;

    pub fn new(testo: &'a str) -> Self {
        Self { testo, icona: None, acceso: false, margine: Self::MARGINE }
    }

    /// Un margine più stretto di [`Chip::MARGINE`] (i due lati insieme), per una fila che se no
    /// non sta nello schermo.
    pub fn margine(mut self, margine: f32) -> Self {
        self.margine = margine;
        self
    }

    /// Quanto è largo ciò che c'è scritto, senza il margine: serve a chi mette i chip in fila.
    pub fn larghezza_del_testo(&self, ui: &Ui) -> f32 {
        self.riga(ui, tema::INCHIOSTRO).larghezza()
    }

    fn riga(&self, ui: &Ui, colore: Color32) -> Riga {
        let font = Carattere::Grassetto.font(ui.ctx(), 14.0);
        Riga::new(ui, self.icona.map(|i| (i, Carattere::Icona, 18.0)), Some((self.testo, font)), 6.0, colore)
    }

    /// Un'icona prima del testo.
    pub fn icona(mut self, icona: &'a str) -> Self {
        self.icona = Some(icona);
        self
    }

    pub fn acceso(mut self, acceso: bool) -> Self {
        self.acceso = acceso;
        self
    }
}

impl Widget for Chip<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let colore = if self.acceso { tema::FOGLIO } else { tema::INCHIOSTRO };
        let riga = self.riga(ui, colore);
        let (rect, r) = ui.allocate_exact_size(vec2(riga.larghezza() + self.margine, tema::CHIP), Sense::click());
        info(&r, true, Some(self.acceso), self.testo);
        if ui.is_rect_visible(rect) {
            let p = ui.painter();
            let raggio = CornerRadius::same((tema::CHIP / 2.0) as u8);
            if self.acceso {
                p.rect_filled(rect, raggio, tema::INCHIOSTRO);
            } else {
                if r.is_pointer_button_down_on() {
                    p.rect_filled(rect, raggio, tema::PREMUTO);
                }
                p.rect_stroke(rect, raggio, Stroke::new(1.0, tema::FILO), StrokeKind::Inside);
            }
            riga.dipingi_al_centro(p, rect);
        }
        r
    }
}

/// Quanto pesa un [`Pulsante`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Peso {
    /// Pieno di blu col testo bianco: l'azione principale di un dialogo.
    Principale,
    /// Sul velo col testo inchiostro; acceso, blu tenue col testo blu (`.bott`).
    Tenue,
    /// Solo il testo blu: le altre azioni di un dialogo.
    Testo,
}

/// Un pulsante con l'etichetta e, se serve, un'icona davanti.
#[derive(Debug, Clone)]
pub struct Pulsante<'a> {
    testo: &'a str,
    peso: Peso,
    icona: Option<&'a str>,
    acceso: Option<bool>,
    larghezza: Option<f32>,
    spento: bool,
}

impl<'a> Pulsante<'a> {
    pub fn new(testo: &'a str, peso: Peso) -> Self {
        Self { testo, peso, icona: None, acceso: None, larghezza: None, spento: false }
    }

    /// Spento: il testo grigio, senza fondo, e non si tocca (Sostituisci senza risultati).
    pub fn spento(mut self, spento: bool) -> Self {
        self.spento = spento;
        self
    }

    pub fn icona(mut self, icona: &'a str) -> Self {
        self.icona = Some(icona);
        self
    }

    /// Un interruttore, acceso o no (conta per il peso [`Peso::Tenue`]).
    pub fn acceso(mut self, acceso: bool) -> Self {
        self.acceso = Some(acceso);
        self
    }

    /// Una larghezza fissa, come i due pulsanti degli elenchi che dividono la riga.
    pub fn larghezza(mut self, larghezza: f32) -> Self {
        self.larghezza = Some(larghezza);
        self
    }
}

impl Pulsante<'_> {
    /// La larghezza che prende in `ui`: quella fissa, o l'icona e il testo con i margini.
    pub fn larghezza_per(&self, ui: &Ui) -> f32 {
        self.larghezza.unwrap_or_else(|| {
            let font = Carattere::Grassetto.font(ui.ctx(), 15.0);
            Riga::new(ui, self.icona.map(|i| (i, Carattere::Icona, tema::ICONA)), Some((self.testo, font)), 8.0, tema::INCHIOSTRO).larghezza() + 40.0
        })
    }
}

impl Widget for Pulsante<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let acceso = self.acceso == Some(true);
        let (fondo, premuto, colore, alto, raggio) = match self.peso {
            Peso::Principale => (Some(tema::BLU), tema::PREMUTO_BLU, tema::FOGLIO, tema::PULSANTE, 24),
            Peso::Testo => (None, tema::BLU_TENUE, tema::BLU, tema::PULSANTE, 24),
            Peso::Tenue if acceso => (Some(tema::BLU_TENUE), tema::PREMUTO_VELO, tema::BLU, tema::PULSANTE_LARGO, tema::RAGGIO_GRUPPO),
            Peso::Tenue => (Some(tema::VELO), tema::PREMUTO_VELO, tema::INCHIOSTRO, tema::PULSANTE_LARGO, tema::RAGGIO_GRUPPO),
        };
        let (fondo, colore) = if self.spento { (None, tema::SPENTO) } else { (fondo, colore) };
        let font = Carattere::Grassetto.font(ui.ctx(), 15.0);
        let riga = Riga::new(ui, self.icona.map(|i| (i, Carattere::Icona, tema::ICONA)), Some((self.testo, font)), 8.0, colore);
        let largo = self.larghezza.unwrap_or(riga.larghezza() + 40.0);
        let (rect, r) = ui.allocate_exact_size(vec2(largo, alto), if self.spento { Sense::hover() } else { Sense::click() });
        info(&r, !self.spento, self.acceso, self.testo);
        if ui.is_rect_visible(rect) {
            let p = ui.painter();
            let fondo = if r.is_pointer_button_down_on() && !self.spento { Some(premuto) } else { fondo };
            if let Some(fondo) = fondo {
                p.rect_filled(rect, CornerRadius::same(raggio), fondo);
            }
            riga.dipingi_al_centro(p, rect);
        }
        r
    }
}

/// Il pulsante «Nuovo» flottante (`.fab`): blu, con l'icona e il testo, in basso a destra.
#[derive(Debug, Clone)]
pub struct PulsanteNuovo<'a> {
    testo: &'a str,
    icona: &'a str,
}

impl<'a> PulsanteNuovo<'a> {
    pub fn new(testo: &'a str) -> Self {
        Self { testo, icona: crate::icone::ADD }
    }

    pub fn icona(mut self, icona: &'a str) -> Self {
        self.icona = icona;
        self
    }

    /// Disegna il pulsante in basso a destra di `zona`, a 18 dp dai bordi, sopra quello che c'è
    /// già: va chiamata dopo il resto della schermata.
    pub fn mostra(self, ui: &mut Ui, zona: Rect) -> Response {
        let font = Carattere::Grassetto.font(ui.ctx(), 16.0);
        let riga = Riga::new(ui, Some((self.icona, Carattere::Icona, tema::ICONA)), Some((self.testo, font)), 10.0, tema::FOGLIO);
        let misura = vec2(18.0 + riga.larghezza() + 22.0, tema::NUOVO);
        let rect = Rect::from_min_size(zona.right_bottom() - vec2(tema::MARGINE_LARGO, tema::MARGINE_LARGO) - misura, misura);
        let r = ui.interact(rect, ui.id().with(("nuovo", self.testo)), Sense::click());
        info(&r, true, None, self.testo);
        let p = ui.painter();
        tema::dipingi_ombra(p, rect, tema::RAGGIO_NUOVO, &tema::OMBRA_NUOVO);
        let fondo = if r.is_pointer_button_down_on() { tema::PREMUTO_BLU } else { tema::BLU };
        p.rect_filled(rect, CornerRadius::same(tema::RAGGIO_NUOVO), fondo);
        riga.dipingi(p, rect.left() + 18.0, rect.center().y);
        r
    }
}

#[cfg(test)]
mod test {
    use egui::accesskit::Toggled;
    use egui_kittest::kittest::{NodeT, Queryable};

    use super::*;
    use crate::icone;
    use crate::prova::{SCHERMO, banco, colore_del_testo, famiglia_del_testo, riempiti};

    #[test]
    fn il_pulsante_icona_e_da_48_e_risponde_al_tocco() {
        let mut h = banco(
            |ui, tocchi: &mut u32| {
                if ui.add(PulsanteIcona::new(icone::FORMAT_ITALIC, "Corsivo")).clicked() {
                    *tocchi += 1;
                }
            },
            0,
        );
        let nodo = h.get_by_label("Corsivo");
        assert_eq!(nodo.rect().size(), egui::vec2(48.0, 48.0));
        nodo.click();
        h.run_steps(2);
        assert_eq!(*h.state(), 1);
        assert_eq!(colore_del_testo(&h, icone::FORMAT_ITALIC), Some(tema::INCHIOSTRO));
        assert_eq!(famiglia_del_testo(&h, icone::FORMAT_ITALIC), Some(Carattere::Icona.famiglia()));
        // Senza fondo, a riposo.
        assert!(riempiti(&h, tema::BLU_TENUE).is_empty());
    }

    #[test]
    fn il_pulsante_spento_e_grigio_e_non_si_tocca() {
        let mut h = banco(
            |ui, tocchi: &mut u32| {
                if ui.add(Pulsante::new("Tutti", Peso::Testo).spento(true)).clicked() {
                    *tocchi += 1;
                }
            },
            0,
        );
        let nodo = h.get_by_label("Tutti");
        assert!(nodo.accesskit_node().is_disabled());
        nodo.click();
        h.run_steps(2);
        assert_eq!(*h.state(), 0);
        assert_eq!(colore_del_testo(&h, "Tutti"), Some(tema::SPENTO));
        // Acceso è blu e si tocca.
        let h = banco(|ui, _: &mut ()| _ = ui.add(Pulsante::new("Tutti", Peso::Testo)), ());
        assert!(!h.get_by_label("Tutti").accesskit_node().is_disabled());
        assert_eq!(colore_del_testo(&h, "Tutti"), Some(tema::BLU));
    }

    #[test]
    fn acceso_ha_il_fondo_blu_tenue_e_l_icona_blu() {
        let h = banco(|ui, _: &mut ()| _ = ui.add(PulsanteIcona::new(icone::FORMAT_BOLD, "Grassetto").acceso(true)), ());
        let nodo = h.get_by_label("Grassetto");
        assert_eq!(nodo.accesskit_node().toggled(), Some(Toggled::True));
        let fondi = riempiti(&h, tema::BLU_TENUE);
        assert_eq!(fondi.len(), 1, "{fondi:?}");
        assert_eq!(fondi[0], nodo.rect());
        assert_eq!(colore_del_testo(&h, icone::FORMAT_BOLD), Some(tema::BLU));
    }

    #[test]
    fn spento_e_grigio_e_non_risponde() {
        let mut h = banco(
            |ui, tocchi: &mut u32| {
                if ui.add(PulsanteIcona::new(icone::REDO, "Ripeti").spento(true)).clicked() {
                    *tocchi += 1;
                }
            },
            0,
        );
        let nodo = h.get_by_label("Ripeti");
        assert!(nodo.accesskit_node().is_disabled());
        nodo.click();
        h.run_steps(2);
        assert_eq!(*h.state(), 0);
        assert_eq!(colore_del_testo(&h, icone::REDO), Some(tema::SPENTO));
    }

    #[test]
    fn il_segno_puo_essere_un_testo_in_fraunces() {
        let h = banco(|ui, _: &mut ()| _ = ui.add(PulsanteIcona::new("Aa", "Formato").testo(Carattere::Titolo, 21.0)), ());
        assert_eq!(famiglia_del_testo(&h, "Aa"), Some(Carattere::Titolo.famiglia()));
    }

    #[test]
    fn il_chip_acceso_e_scuro_col_testo_bianco() {
        let mut h = banco(
            |ui, scelto: &mut usize| {
                ui.horizontal(|ui| {
                    for (i, nome) in ["Tutti", "PDF"].into_iter().enumerate() {
                        if ui.add(Chip::new(nome).acceso(*scelto == i)).clicked() {
                            *scelto = i;
                        }
                    }
                });
            },
            0,
        );
        let tutti = h.get_by_label("Tutti");
        assert_eq!(tutti.rect().height(), tema::CHIP);
        assert_eq!(tutti.accesskit_node().toggled(), Some(Toggled::True));
        assert_eq!(colore_del_testo(&h, "Tutti"), Some(tema::FOGLIO));
        assert_eq!(colore_del_testo(&h, "PDF"), Some(tema::INCHIOSTRO));
        assert_eq!(famiglia_del_testo(&h, "PDF"), Some(Carattere::Grassetto.famiglia()));
        assert_eq!(riempiti(&h, tema::INCHIOSTRO), vec![tutti.rect()]);
        h.get_by_label("PDF").click();
        h.run_steps(2);
        assert_eq!(*h.state(), 1);
        assert_eq!(colore_del_testo(&h, "PDF"), Some(tema::FOGLIO));
    }

    #[test]
    fn i_pesi_del_pulsante() {
        let mut h = banco(
            |ui, tocchi: &mut Vec<&'static str>| {
                for (testo, p) in [
                    ("Salva", Pulsante::new("Salva", Peso::Principale)),
                    ("Annulla", Pulsante::new("Annulla", Peso::Testo)),
                    ("Puntato", Pulsante::new("Puntato", Peso::Tenue).icona(icone::FORMAT_LIST_BULLETED).acceso(true)),
                    ("Numerato", Pulsante::new("Numerato", Peso::Tenue).icona(icone::FORMAT_LIST_NUMBERED).larghezza(180.0)),
                ] {
                    if ui.add(p).clicked() {
                        tocchi.push(testo);
                    }
                }
            },
            Vec::new(),
        );
        assert_eq!(colore_del_testo(&h, "Salva"), Some(tema::FOGLIO));
        assert_eq!(riempiti(&h, tema::BLU), vec![h.get_by_label("Salva").rect()]);
        assert_eq!(colore_del_testo(&h, "Annulla"), Some(tema::BLU));
        assert_eq!(colore_del_testo(&h, "Puntato"), Some(tema::BLU));
        assert_eq!(colore_del_testo(&h, icone::FORMAT_LIST_BULLETED), Some(tema::BLU));
        assert_eq!(riempiti(&h, tema::BLU_TENUE), vec![h.get_by_label("Puntato").rect()]);
        assert_eq!(colore_del_testo(&h, "Numerato"), Some(tema::INCHIOSTRO));
        let numerato = h.get_by_label("Numerato").rect();
        assert_eq!(numerato.size(), egui::vec2(180.0, tema::PULSANTE_LARGO));
        assert!(h.get_by_label("Salva").rect().height() >= tema::PULSANTE);
        h.get_by_label("Annulla").click();
        h.run_steps(2);
        assert_eq!(h.state(), &vec!["Annulla"]);
    }

    #[test]
    fn il_nuovo_sta_in_basso_a_destra_e_risponde() {
        let mut h = banco(
            |ui, tocchi: &mut u32| {
                let zona = ui.ctx().content_rect();
                if PulsanteNuovo::new("Nuovo").mostra(ui, zona).clicked() {
                    *tocchi += 1;
                }
            },
            0,
        );
        let r = h.get_by_label("Nuovo").rect();
        assert_eq!(r.height(), tema::NUOVO);
        assert_eq!(r.right(), SCHERMO.x - tema::MARGINE_LARGO);
        assert_eq!(r.bottom(), SCHERMO.y - tema::MARGINE_LARGO);
        assert_eq!(riempiti(&h, tema::BLU), vec![r]);
        assert_eq!(colore_del_testo(&h, "Nuovo"), Some(tema::FOGLIO));
        assert_eq!(colore_del_testo(&h, icone::ADD), Some(tema::FOGLIO));
        h.get_by_label("Nuovo").click();
        h.run_steps(2);
        assert_eq!(*h.state(), 1);
    }
}
