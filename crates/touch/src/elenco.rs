//! Schede ed elenchi: la scheda bianca con l'ombra, la voce di elenco col riquadro del tipo e il
//! menu con icona ed etichetta.

use std::sync::Arc;

use egui::{
    Align2, Area, Color32, Context, CornerRadius, Frame, Galley, Id, InnerResponse, Margin, Modal, Order, Pos2, Rect, Response, Sense, Shape, Ui,
    UiBuilder, UiKind, Vec2, WidgetInfo, WidgetType, pos2, vec2,
};

use crate::disegno::{testo_a_capo, testo_tagliato};
use crate::icone;
use crate::pulsanti::{PulsanteIcona, info};
use crate::tema::{self, Carattere, Ombra};

/// Una scheda (`.elenco`, `.cartella`): un foglio bianco arrotondato con l'ombra sotto.
#[derive(Debug, Clone, Copy)]
pub struct Scheda {
    raggio: u8,
    margine: Margin,
    ombra: Ombra,
    fondo: Color32,
}

impl Default for Scheda {
    fn default() -> Self {
        Self { raggio: tema::RAGGIO_SCHEDA, margine: Margin::ZERO, ombra: tema::OMBRA, fondo: tema::FOGLIO }
    }
}

impl Scheda {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn raggio(mut self, raggio: u8) -> Self {
        self.raggio = raggio;
        self
    }

    /// Lo spazio fra il bordo e il contenuto (di solito nessuno: le voci hanno il loro).
    pub fn margine(mut self, margine: impl Into<Margin>) -> Self {
        self.margine = margine.into();
        self
    }

    pub fn ombra(mut self, ombra: Ombra) -> Self {
        self.ombra = ombra;
        self
    }

    pub fn fondo(mut self, fondo: Color32) -> Self {
        self.fondo = fondo;
        self
    }

    pub fn show<R>(self, ui: &mut Ui, contenuto: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        // Il posto dell'ombra si tiene prima del foglio, che così le sta sopra.
        let posto = ui.painter().add(Shape::Noop);
        let raggio = CornerRadius::same(self.raggio);
        let r = Frame::new().fill(self.fondo).corner_radius(raggio).inner_margin(self.margine).show(ui, contenuto);
        ui.painter().set(posto, Shape::Vec(tema::forme_ombra(r.response.rect, raggio, &self.ombra)));
        r
    }
}

/// Il tipo di un file in una voce di elenco, col suo colore e la sua icona.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tipo {
    Word,
    Pdf,
    Slide,
    /// Un foglio di calcolo.
    Foglio,
    Cartella,
    /// Un file che l'app non apre (una foto, un archivio): grigio come le cartelle.
    Altro,
}

impl Tipo {
    /// Il colore del segno e quello del fondo del riquadro.
    pub fn colori(self) -> (Color32, Color32) {
        match self {
            Tipo::Word => (tema::BLU, tema::BLU_TENUE),
            Tipo::Pdf => (tema::ROSSO, tema::ROSSO_TENUE),
            Tipo::Slide => (tema::OCRA, tema::OCRA_TENUE),
            Tipo::Foglio => (tema::VERDE, tema::VERDE_TENUE),
            Tipo::Cartella | Tipo::Altro => (tema::MATITA, tema::VELO),
        }
    }

    pub fn icona(self) -> &'static str {
        match self {
            Tipo::Word => icone::DESCRIPTION,
            Tipo::Pdf => icone::PICTURE_AS_PDF,
            Tipo::Slide => icone::SLIDESHOW,
            Tipo::Foglio => icone::TABLE,
            Tipo::Cartella => icone::FOLDER,
            Tipo::Altro => icone::TEXT_SNIPPET,
        }
    }
}

/// Quanto il dito deve restare fermo su una voce di un elenco che scorre prima che si colori
/// (secondi), di quanto può muoversi (dp), e quanto resta colorata dopo un tocco (secondi).
const ATTESA_PREMUTO: f64 = 0.1;
const DITO_FERMO: f32 = 8.0;
const DOPO_IL_TOCCO: f64 = 0.15;

/// Una voce di un elenco che scorre si colora come premuta solo se il dito ci resta fermo un
/// attimo, o dopo un tocco: chi appoggia il dito per scorrere non la colora (segnalazione
/// dell'autore del 3/11: «preseleziona illuminando il primo file che tocco per scorrere»), come fa
/// Android.
pub fn premuto(ui: &Ui, r: &Response) -> bool {
    let id = r.id.with("premuto");
    let ora = ui.input(|i| i.time);
    if r.clicked() {
        ui.ctx().data_mut(|d| d.insert_temp(id, ora));
    }
    if let Some(t) = ui.ctx().data(|d| d.get_temp::<f64>(id))
        && ora - t < DOPO_IL_TOCCO
    {
        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(DOPO_IL_TOCCO - (ora - t)));
        return true;
    }
    if !r.is_pointer_button_down_on() {
        return false;
    }
    let (inizio, origine, qui) = ui.input(|i| (i.pointer.press_start_time(), i.pointer.press_origin(), i.pointer.interact_pos()));
    if matches!((origine, qui), (Some(a), Some(b)) if (b - a).length() > DITO_FERMO) {
        return false;
    }
    match inizio {
        Some(t) if ora - t >= ATTESA_PREMUTO => true,
        Some(t) => {
            ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(ATTESA_PREMUTO - (ora - t)));
            false
        }
        None => false,
    }
}

/// Una voce di elenco (`.voce`), alta 72 dp: il riquadro del tipo, il nome in grassetto, sotto il
/// luogo in grigio con la sua icona, a destra il pulsante del menu; una linea sottile sotto.
/// Spenta, è tutta grigia e non si tocca (un file che l'app non apre ancora). Una riga sotto
/// senza icona che non ci sta va a capo, e la voce è alta 88 dp.
#[derive(Debug, Clone)]
pub struct Voce<'a> {
    tipo: Tipo,
    icona: Option<&'a str>,
    nome: &'a str,
    dove: Option<(Option<&'a str>, &'a str)>,
    menu: Option<&'a str>,
    /// L'interruttore a destra, acceso o spento: tutta la voce lo cambia.
    interruttore: Option<bool>,
    ultima: bool,
    spenta: bool,
}

/// L'interruttore di una voce: largo, alto e il tondo dentro.
const INTERRUTTORE: Vec2 = vec2(40.0, 24.0);
/// La voce con la riga sotto su due righe, come le voci a tre righe di Android.
const VOCE_A_TRE_RIGHE: f32 = 88.0;
const TONDO_INTERRUTTORE: f32 = 9.0;

/// Che cosa è stato toccato in una voce.
#[derive(Debug)]
pub struct RispostaVoce {
    /// La voce intera.
    pub response: Response,
    /// Il pulsante del menu, se c'è (il suo rettangolo serve per aprire il menu lì).
    pub menu: Option<Response>,
}

impl RispostaVoce {
    pub fn aperta(&self) -> bool {
        self.response.clicked()
    }

    pub fn menu_toccato(&self) -> bool {
        self.menu.as_ref().is_some_and(Response::clicked)
    }
}

impl<'a> Voce<'a> {
    pub fn new(tipo: Tipo, nome: &'a str) -> Self {
        Self { tipo, icona: None, nome, dove: None, menu: None, interruttore: None, ultima: false, spenta: false }
    }

    /// Un'icona nel riquadro al posto di quella del tipo (la freccia di «..»), coi colori del tipo.
    pub fn icona(mut self, icona: &'a str) -> Self {
        self.icona = Some(icona);
        self
    }

    /// Il luogo e il quando sotto il nome, con un'icona davanti.
    pub fn dove(mut self, icona: &'a str, testo: &'a str) -> Self {
        self.dove = Some((Some(icona), testo));
        self
    }

    /// Una riga grigia sotto il nome, senza icona.
    pub fn sotto(mut self, testo: &'a str) -> Self {
        self.dove = Some((None, testo));
        self
    }

    /// Spenta: grigia, non si apre (il menu, se c'è, resta).
    pub fn spenta(mut self, spenta: bool) -> Self {
        self.spenta = spenta;
        self
    }

    /// Il pulsante del menu a destra, con questa etichetta per l'accessibilità.
    pub fn menu(mut self, etichetta: &'a str) -> Self {
        self.menu = Some(etichetta);
        self
    }

    /// L'interruttore a destra (`.interruttore`): acceso, blu col tondo bianco a destra; spento,
    /// grigio col tondo a sinistra. Un tocco su tutta la voce lo cambia (lo dice `aperta`).
    pub fn interruttore(mut self, acceso: bool) -> Self {
        self.interruttore = Some(acceso);
        self
    }

    /// L'ultima voce dell'elenco non ha la linea sotto.
    pub fn ultima(mut self, ultima: bool) -> Self {
        self.ultima = ultima;
        self
    }

    /// Disegna la voce in cima allo spazio libero di `ui`, larga quanto lo spazio.
    pub fn mostra(self, ui: &mut Ui) -> RispostaVoce {
        let largo = ui.available_width();
        let ctx = ui.ctx().clone();
        let matita = if self.spenta { tema::SPENTO } else { tema::MATITA };
        let spazio = self.spazio_del_testo(largo);
        // Il luogo, con l'icona, su una riga; la riga sotto senza icona (una spiegazione) va a
        // capo, su due al più: tagliata, perdeva proprio la parte che dice che cosa fa la voce
        // (0.8.3, a 372 dp).
        let dove = self.dove.map(|(icona, testo)| match icona {
            Some(i) => {
                let icona = ui.painter().layout_no_wrap(i.to_owned(), Carattere::Icona.font(&ctx, 15.0), matita);
                let testo = testo_tagliato(ui, testo, Carattere::Testo.font(&ctx, 13.0), matita, spazio - icona.size().x - 4.0);
                (Some(icona), testo)
            }
            None => (None, testo_a_capo(ui, testo, Carattere::Testo.font(&ctx, 13.0), matita, spazio, 2)),
        });
        let alto = if dove.as_ref().is_some_and(|(_, t)| t.rows.len() > 1) { VOCE_A_TRE_RIGHE } else { tema::VOCE };
        let rect = Rect::from_min_size(ui.cursor().min, vec2(largo, alto));
        ui.scope_builder(UiBuilder::new().max_rect(rect), |ui| self.dentro(ui, rect, dove)).inner
    }

    /// Lo spazio per il nome e la riga sotto in una voce larga `largo`: dopo il riquadro del
    /// tipo, prima del menu o dell'interruttore. Un nome più largo finisce coi puntini.
    pub fn spazio_del_testo(&self, largo: f32) -> f32 {
        let destra = if self.menu.is_some() {
            8.0 + tema::PULSANTE + 14.0
        } else if self.interruttore.is_some() {
            16.0 + INTERRUTTORE.x + 14.0
        } else {
            14.0
        };
        largo - (14.0 + tema::TIPO + 14.0) - destra
    }

    fn dentro(self, ui: &mut Ui, rect: Rect, dove: Option<(Option<Arc<Galley>>, Arc<Galley>)>) -> RispostaVoce {
        let response = ui.allocate_rect(rect, if self.spenta { Sense::hover() } else { Sense::click() });
        info(&response, !self.spenta, self.interruttore, self.nome);
        let ctx = ui.ctx().clone();
        if !self.spenta && premuto(ui, &response) {
            ui.painter().rect_filled(rect, 0.0, tema::PREMUTO);
        }
        if !self.ultima {
            ui.painter().rect_filled(Rect::from_min_max(pos2(rect.left(), rect.bottom() - 1.0), rect.max), 0.0, tema::FILO);
        }
        let (segno, fondo) = if self.spenta { (tema::SPENTO, tema::VELO) } else { self.tipo.colori() };
        let inchiostro = if self.spenta { tema::SPENTO } else { tema::INCHIOSTRO };
        let riquadro = Rect::from_min_size(pos2(rect.left() + 14.0, rect.center().y - tema::TIPO / 2.0), Vec2::splat(tema::TIPO));
        ui.painter().rect_filled(riquadro, tema::RAGGIO_SEGMENTO, fondo);
        let icona = self.icona.unwrap_or(self.tipo.icona());
        ui.painter().text(riquadro.center(), Align2::CENTER_CENTER, icona, Carattere::IconaPiena.font(&ctx, tema::ICONA), segno);
        let menu = self.menu.map(|etichetta| {
            let posto =
                Rect::from_min_size(pos2(rect.right() - 8.0 - tema::PULSANTE, rect.center().y - tema::PULSANTE / 2.0), Vec2::splat(tema::PULSANTE));
            PulsanteIcona::new(icone::MORE_VERT, etichetta).mostra_in(ui, posto, (tema::PULSANTE / 2.0) as u8)
        });
        if let Some(acceso) = self.interruttore {
            let pista = Rect::from_min_size(pos2(rect.right() - 16.0 - INTERRUTTORE.x, rect.center().y - INTERRUTTORE.y / 2.0), INTERRUTTORE);
            let (fondo, x) =
                if acceso { (tema::BLU, pista.right() - INTERRUTTORE.y / 2.0) } else { (tema::SPENTO, pista.left() + INTERRUTTORE.y / 2.0) };
            ui.painter().rect_filled(pista, (INTERRUTTORE.y / 2.0) as u8, fondo);
            ui.painter().circle_filled(pos2(x, pista.center().y), TONDO_INTERRUTTORE, tema::FOGLIO);
        }
        let sinistra = riquadro.right() + 14.0;
        let nome = testo_tagliato(ui, self.nome, Carattere::Grassetto.font(&ctx, 16.0), inchiostro, self.spazio_del_testo(rect.width()));
        let alto_dove = dove.as_ref().map_or(0.0, |(i, t)| 2.0 + i.as_ref().map_or(0.0, |i| i.size().y).max(t.size().y));
        let y = rect.center().y - (nome.size().y + alto_dove) / 2.0;
        let fondo_nome = y + nome.size().y;
        ui.painter().galley(pos2(sinistra, y), nome, Color32::PLACEHOLDER);
        if let Some((icona, testo)) = dove {
            let centro = fondo_nome + alto_dove / 2.0 + 1.0;
            let mut x = sinistra;
            if let Some(icona) = icona {
                x += icona.size().x + 4.0;
                ui.painter().galley(pos2(sinistra, centro - icona.size().y / 2.0), icona, Color32::PLACEHOLDER);
            }
            ui.painter().galley(pos2(x, centro - testo.size().y / 2.0), testo, Color32::PLACEHOLDER);
        }
        RispostaVoce { response, menu }
    }
}

/// Una cartella nella griglia della Home (`.cartella`): una scheda bianca alta 64 dp con l'icona
/// grigia, il nome in grassetto e sotto, in grigio, quanti file ha.
#[derive(Debug, Clone)]
pub struct SchedaCartella<'a> {
    icona: &'a str,
    nome: &'a str,
    sotto: &'a str,
}

impl<'a> SchedaCartella<'a> {
    pub fn new(icona: &'a str, nome: &'a str, sotto: &'a str) -> Self {
        Self { icona, nome, sotto }
    }

    /// Disegna la scheda in `rect` (di solito 64 dp di altezza) e la rende toccabile.
    pub fn mostra_in(self, ui: &mut Ui, rect: Rect) -> Response {
        let r = ui.allocate_rect(rect, Sense::click());
        info(&r, true, None, self.nome);
        if !ui.is_rect_visible(rect) {
            return r;
        }
        let ctx = ui.ctx().clone();
        let raggio = CornerRadius::same(tema::RAGGIO_CARTELLA);
        tema::dipingi_ombra(ui.painter(), rect, raggio, &tema::OMBRA);
        let fondo = if premuto(ui, &r) { tema::PREMUTO } else { tema::FOGLIO };
        ui.painter().rect_filled(rect, raggio, fondo);
        let x_icona = rect.left() + 14.0;
        ui.painter().text(
            pos2(x_icona + tema::ICONA / 2.0, rect.center().y),
            Align2::CENTER_CENTER,
            self.icona,
            Carattere::Icona.font(&ctx, tema::ICONA),
            tema::MATITA,
        );
        let x = x_icona + tema::ICONA + 12.0;
        let largo = rect.right() - 14.0 - x;
        let nome = testo_tagliato(ui, self.nome, Carattere::Grassetto.font(&ctx, 15.0), tema::INCHIOSTRO, largo);
        let sotto = testo_tagliato(ui, self.sotto, Carattere::Testo.font(&ctx, 12.0), tema::MATITA, largo);
        let y = rect.center().y - (nome.size().y + sotto.size().y) / 2.0;
        let a_capo = y + nome.size().y;
        ui.painter().galley(pos2(x, y), nome, Color32::PLACEHOLDER);
        ui.painter().galley(pos2(x, a_capo), sotto, Color32::PLACEHOLDER);
        r
    }
}

/// Che cosa si è scelto nel menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceltaMenu {
    /// Una voce, contando solo le voci (non i separatori).
    Voce(usize),
    /// Un tocco fuori dal menu: si chiude.
    Fuori,
}

#[derive(Debug, Clone)]
enum VoceMenu<'a> {
    Voce { icona: &'a str, etichetta: &'a str, sotto: Option<&'a str>, primo: bool, rosso: bool },
    Separatore,
}

/// Il menu (`.menu`): un foglio bianco largo 252 dp sopra tutto, con voci fatte di icona grigia
/// ed etichetta; la prima voce può essere in grassetto con l'icona blu, una voce può avere
/// sotto una riga grigia che la spiega, e una può essere tutta rossa (quella che non si disfa).
/// Chi lo usa tiene se è aperto e lo chiude a ogni scelta.
#[derive(Debug, Clone)]
pub struct Menu<'a> {
    id: Id,
    voci: Vec<VoceMenu<'a>>,
    /// Il punto del menu che sta nell'angolo dato a `mostra`.
    perno: Align2,
}

impl<'a> Menu<'a> {
    pub fn new(id: impl Into<Id>) -> Self {
        Self { id: id.into(), voci: Vec::new(), perno: Align2::RIGHT_TOP }
    }

    /// Il menu sopra il punto dato a `mostra`, col centro del suo lato basso lì (per un pulsante
    /// in fondo allo schermo, come quelli della barra della tabella), dentro lo schermo.
    pub fn sopra(mut self) -> Self {
        self.perno = Align2::CENTER_BOTTOM;
        self
    }

    /// La voce principale, in grassetto con l'icona blu.
    pub fn primo(mut self, icona: &'a str, etichetta: &'a str) -> Self {
        self.voci.push(VoceMenu::Voce { icona, etichetta, sotto: None, primo: true, rosso: false });
        self
    }

    pub fn voce(mut self, icona: &'a str, etichetta: &'a str) -> Self {
        self.voci.push(VoceMenu::Voce { icona, etichetta, sotto: None, primo: false, rosso: false });
        self
    }

    /// Una voce con sotto una riga grigia più piccola che dice che cosa fa: per le voci che si
    /// somigliano («Salva una copia», «Copia definitiva»).
    pub fn voce_spiegata(mut self, icona: &'a str, etichetta: &'a str, sotto: &'a str) -> Self {
        self.voci.push(VoceMenu::Voce { icona, etichetta, sotto: Some(sotto), primo: false, rosso: false });
        self
    }

    /// Una voce spiegata tutta rossa (icona, etichetta e riga sotto): quella da cui non si torna
    /// indietro, come la copia definitiva, dove quello che si fissa non si modifica più.
    pub fn voce_rossa(mut self, icona: &'a str, etichetta: &'a str, sotto: &'a str) -> Self {
        self.voci.push(VoceMenu::Voce { icona, etichetta, sotto: Some(sotto), primo: false, rosso: true });
        self
    }

    pub fn separatore(mut self) -> Self {
        self.voci.push(VoceMenu::Separatore);
        self
    }

    /// Disegna il menu con l'angolo in alto a destra in `angolo` (con `sopra`, il centro del lato
    /// basso), sopra tutto il resto. Un tocco fuori non arriva a quello che c'è sotto: chiude il
    /// menu.
    pub fn mostra(self, ctx: &Context, angolo: Pos2) -> Option<SceltaMenu> {
        let area = Area::new(self.id)
            .kind(UiKind::Modal)
            .sense(Sense::hover())
            .order(Order::Foreground)
            .interactable(true)
            .pivot(self.perno)
            .fixed_pos(angolo);
        let mut scelta = None;
        let risposta = Modal::new(self.id).area(area).frame(Frame::NONE).backdrop_color(Color32::TRANSPARENT).show(ctx, |ui| {
            Scheda::new().raggio(tema::RAGGIO_MENU).ombra(tema::OMBRA_MENU).margine(Margin::symmetric(0, 8)).show(ui, |ui| {
                ui.set_width(tema::MENU);
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                let mut n = 0;
                for v in self.voci {
                    match v {
                        VoceMenu::Voce { icona, etichetta, sotto, primo, rosso } => {
                            if voce_del_menu(ui, icona, etichetta, sotto, primo, rosso).clicked() {
                                scelta = Some(SceltaMenu::Voce(n));
                            }
                            n += 1;
                        }
                        VoceMenu::Separatore => {
                            let (r, _) = ui.allocate_exact_size(vec2(tema::MENU, 13.0), Sense::hover());
                            ui.painter().rect_filled(Rect::from_center_size(r.center(), vec2(tema::MENU, 1.0)), 0.0, tema::FILO);
                        }
                    }
                }
            })
        });
        if scelta.is_none() && risposta.backdrop_response.clicked() {
            scelta = Some(SceltaMenu::Fuori);
        }
        scelta
    }
}

/// Una voce del menu (`.menu .m`): l'icona a 18 dp dal bordo, l'etichetta a 58; va a capo se è
/// lunga, e due righe stanno ancora nei 48 dp (come nel bozzetto); tre o più la allungano. Con la
/// spiegazione (`sotto`), una riga grigia di 13 dp sotto l'etichetta, nella stessa colonna, con 8
/// dp d'aria sopra e sotto: la voce si allunga quanto serve. Una voce rossa ha rossi l'icona,
/// l'etichetta e la spiegazione.
fn voce_del_menu(ui: &mut Ui, icona: &str, etichetta: &str, sotto: Option<&str>, primo: bool, rosso: bool) -> Response {
    let ctx = ui.ctx().clone();
    let colonna = tema::MENU - 58.0 - 18.0;
    let font = if primo { Carattere::Grassetto } else { Carattere::Testo }.font(&ctx, 15.5);
    let (inchiostro, matita) = if rosso { (tema::ROSSO, tema::ROSSO) } else { (tema::INCHIOSTRO, tema::MATITA) };
    let testo = ui.painter().layout(etichetta.to_owned(), font, inchiostro, colonna);
    let spiega = sotto.map(|s| ui.painter().layout(s.to_owned(), Carattere::Testo.font(&ctx, 13.0), matita, colonna));
    let testi = testo.size().y + spiega.as_ref().map_or(0.0, |g| FRA_ETICHETTA_E_SPIEGA + g.size().y);
    let aria = if spiega.is_some() { 20.0 } else { 8.0 };
    let (rect, r) = ui.allocate_exact_size(vec2(tema::MENU, (testi + aria).max(tema::VOCE_MENU)), Sense::click());
    info(&r, true, None, etichetta);
    let p = ui.painter();
    if r.is_pointer_button_down_on() {
        p.rect_filled(rect, 0.0, tema::PREMUTO);
    }
    let colore = if primo { tema::BLU } else { matita };
    p.text(
        pos2(rect.left() + 18.0 + tema::ICONA / 2.0, rect.center().y),
        Align2::CENTER_CENTER,
        icona,
        Carattere::Icona.font(&ctx, tema::ICONA),
        colore,
    );
    let cima = rect.center().y - testi / 2.0;
    let sotto_l_etichetta = cima + testo.size().y + FRA_ETICHETTA_E_SPIEGA;
    p.galley(pos2(rect.left() + 58.0, cima), testo, Color32::PLACEHOLDER);
    if let (Some(g), Some(s)) = (spiega, sotto) {
        let posto = Rect::from_min_size(pos2(rect.left() + 58.0, sotto_l_etichetta), g.size());
        p.galley(posto.min, g, Color32::PLACEHOLDER);
        // Si legge anche da sola (lettore dello schermo); i tocchi restano della voce.
        ui.interact(posto, r.id.with("spiega"), Sense::hover()).widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, s));
    }
    r
}

/// Fra l'etichetta di una voce del menu e la riga che la spiega.
const FRA_ETICHETTA_E_SPIEGA: f32 = 1.0;

#[cfg(test)]
mod test {
    use egui_kittest::kittest::{NodeT, Queryable};

    use super::*;
    use crate::icone;
    use crate::prova::{SCHERMO, banco, colore_del_testo, famiglia_del_testo, forme, rettangolo_del_testo, riempiti, righe_del_testo, tocco};

    #[test]
    fn la_scheda_e_bianca_arrotondata_con_l_ombra_sotto() {
        let h = banco(
            |ui, rect: &mut Option<egui::Rect>| {
                ui.add_space(20.0);
                let r = Scheda::new().margine(egui::Margin::same(10)).show(ui, |ui| ui.label("Dentro"));
                *rect = Some(r.response.rect);
            },
            None,
        );
        let rect = h.state().unwrap();
        let tutte = forme(&h);
        let fondo = tutte
            .iter()
            .position(|s| matches!(s, egui::Shape::Rect(r) if r.fill == tema::FOGLIO && r.rect == rect && r.corner_radius == egui::CornerRadius::same(tema::RAGGIO_SCHEDA)))
            .expect("il fondo della scheda");
        let ombre: Vec<usize> =
            tutte.iter().enumerate().filter(|(_, s)| matches!(s, egui::Shape::Rect(r) if r.blur_width > 0.0)).map(|(i, _)| i).collect();
        assert_eq!(ombre.len(), 2, "due strati d'ombra");
        assert!(ombre.iter().all(|&i| i < fondo), "l'ombra va sotto la scheda");
        assert!(rect.contains_rect(rettangolo_del_testo(&h, "Dentro").unwrap()));
    }

    #[derive(Default)]
    struct Tocchi {
        aperte: Vec<&'static str>,
        menu: Vec<&'static str>,
        rect: Option<egui::Rect>,
    }

    fn elenco(ui: &mut egui::Ui, t: &mut Tocchi) {
        let voci = [
            (Tipo::Word, "Relazione turno notte", icone::FOLDER, "Documenti · oggi 08:12"),
            (Tipo::Pdf, "Consenso informato da firmare", icone::CHAT, "da WhatsApp · ieri 19:40"),
            (Tipo::Slide, "Lezione lesioni da pressione", icone::DOWNLOAD, "Download · 21 ottobre"),
        ];
        for (i, (tipo, nome, icona, dove)) in voci.into_iter().enumerate() {
            let r = Voce::new(tipo, nome).dove(icona, dove).menu("Altre azioni").ultima(i == 2).mostra(ui);
            if i == 1 {
                t.rect = Some(r.response.rect);
            }
            if r.aperta() {
                t.aperte.push(nome);
            }
            if r.menu_toccato() {
                t.menu.push(nome);
            }
        }
    }

    #[test]
    fn la_voce_e_alta_72_col_riquadro_del_tipo_nome_e_luogo() {
        let h = banco(elenco, Tocchi::default());
        let voce = h.state().rect.unwrap();
        assert_eq!(voce.size(), egui::vec2(SCHERMO.x, tema::VOCE));
        // Il riquadro rosso tenue del PDF, 44 dp, a 14 dal bordo, con l'icona piena rossa.
        let riquadri = riempiti(&h, tema::ROSSO_TENUE);
        assert_eq!(riquadri.len(), 1, "{riquadri:?}");
        assert_eq!(riquadri[0].size(), egui::Vec2::splat(tema::TIPO));
        assert_eq!(riquadri[0].left(), voce.left() + 14.0);
        assert_eq!(riquadri[0].center().y, voce.center().y);
        assert_eq!(colore_del_testo(&h, icone::PICTURE_AS_PDF), Some(tema::ROSSO));
        assert_eq!(famiglia_del_testo(&h, icone::PICTURE_AS_PDF), Some(Carattere::IconaPiena.famiglia()));
        assert_eq!(riempiti(&h, tema::BLU_TENUE).len(), 1);
        assert_eq!(riempiti(&h, tema::OCRA_TENUE).len(), 1);
        // Il nome in grassetto, il luogo grigio sotto, a destra del riquadro.
        let nome = rettangolo_del_testo(&h, "Consenso informato da firmare").unwrap();
        let dove = rettangolo_del_testo(&h, "da WhatsApp · ieri 19:40").unwrap();
        assert!(nome.left() >= riquadri[0].right() + 13.0 && nome.bottom() <= dove.top(), "{nome:?} {dove:?}");
        assert_eq!(famiglia_del_testo(&h, "Consenso informato da firmare"), Some(Carattere::Grassetto.famiglia()));
        assert_eq!(colore_del_testo(&h, "da WhatsApp · ieri 19:40"), Some(tema::MATITA));
        assert_eq!(colore_del_testo(&h, icone::CHAT), Some(tema::MATITA));
        // Il menu a destra; due linee sottili fra tre voci.
        let menu = h.get_all_by_label("Altre azioni").map(|n| n.rect()).collect::<Vec<_>>();
        assert_eq!(menu.len(), 3);
        assert_eq!(menu[1].right(), voce.right() - 8.0);
        let fili = riempiti(&h, tema::FILO);
        assert_eq!(fili.len(), 2, "{fili:?}");
        assert!(fili.iter().all(|f| f.height() == 1.0 && f.width() == SCHERMO.x));
    }

    #[test]
    fn toccare_la_voce_la_apre_e_il_menu_e_a_parte() {
        let mut h = banco(elenco, Tocchi::default());
        h.get_by_label("Relazione turno notte").click();
        h.run_steps(2);
        let menu = h.get_all_by_label("Altre azioni").nth(2).unwrap().rect().center();
        tocco(&mut h, menu);
        assert_eq!(h.state().aperte, vec!["Relazione turno notte"]);
        assert_eq!(h.state().menu, vec!["Lezione lesioni da pressione"]);
    }

    #[test]
    fn i_tipi_hanno_il_loro_colore_e_la_loro_icona() {
        assert_eq!(Tipo::Word.colori(), (tema::BLU, tema::BLU_TENUE));
        assert_eq!(Tipo::Pdf.colori(), (tema::ROSSO, tema::ROSSO_TENUE));
        assert_eq!(Tipo::Slide.colori(), (tema::OCRA, tema::OCRA_TENUE));
        assert_eq!(Tipo::Cartella.colori(), (tema::MATITA, tema::VELO));
        assert_eq!(Tipo::Altro.colori(), (tema::MATITA, tema::VELO));
        assert_eq!(Tipo::Word.icona(), icone::DESCRIPTION);
        assert_eq!(Tipo::Pdf.icona(), icone::PICTURE_AS_PDF);
        assert_eq!(Tipo::Slide.icona(), icone::SLIDESHOW);
        assert_eq!(Tipo::Cartella.icona(), icone::FOLDER);
        assert_eq!(Tipo::Altro.icona(), icone::TEXT_SNIPPET);
    }

    #[test]
    fn la_voce_con_l_interruttore() {
        let mut h = banco(
            |ui, acceso: &mut bool| {
                let voce = Voce::new(Tipo::Cartella, "Controllo ortografico").icona(icone::SPELLCHECK).sotto("Le parole sbagliate in rosso");
                if voce.interruttore(*acceso).mostra(ui).aperta() {
                    *acceso = !*acceso;
                }
            },
            true,
        );
        let voce = h.get_by_label("Controllo ortografico");
        assert_eq!(voce.accesskit_node().toggled(), Some(egui::accesskit::Toggled::True));
        // La pista blu a destra, a 16 dp dal bordo, col tondo bianco dalla parte destra.
        let piste = riempiti(&h, tema::BLU);
        assert_eq!(piste.len(), 1, "{piste:?}");
        assert_eq!((piste[0].size(), piste[0].right()), (INTERRUTTORE, SCHERMO.x - 16.0));
        voce.click();
        h.run_steps(2);
        assert!(!*h.state());
        assert_eq!(h.get_by_label("Controllo ortografico").accesskit_node().toggled(), Some(egui::accesskit::Toggled::False));
        assert!(riempiti(&h, tema::BLU).is_empty());
        assert_eq!(riempiti(&h, tema::SPENTO).len(), 1);
    }

    #[test]
    fn la_voce_spenta_e_grigia_e_non_si_apre() {
        let mut h = banco(
            |ui, aperte: &mut u32| {
                if Voce::new(Tipo::Pdf, "Bolletta.pdf").dove(icone::SCHEDULE, "in arrivo").spenta(true).mostra(ui).aperta() {
                    *aperte += 1;
                }
            },
            0,
        );
        let nodo = h.get_by_label("Bolletta.pdf");
        assert!(nodo.accesskit_node().is_disabled());
        nodo.click();
        h.run_steps(2);
        assert_eq!(*h.state(), 0);
        // Il nome, il riquadro e il luogo sbiaditi: il colore del tipo non c'è più.
        assert_eq!(colore_del_testo(&h, "Bolletta.pdf"), Some(tema::SPENTO));
        assert_eq!(colore_del_testo(&h, icone::PICTURE_AS_PDF), Some(tema::SPENTO));
        assert_eq!(colore_del_testo(&h, "in arrivo"), Some(tema::SPENTO));
        assert!(riempiti(&h, tema::ROSSO_TENUE).is_empty());
        assert_eq!(riempiti(&h, tema::VELO).len(), 1);
    }

    #[test]
    fn la_voce_puo_avere_la_sua_icona_e_una_riga_sotto_senza_icona() {
        let h = banco(
            |ui, _: &mut ()| {
                Voce::new(Tipo::Cartella, "..").icona(icone::ARROW_UPWARD).sotto("La cartella che la contiene").mostra(ui);
            },
            (),
        );
        assert_eq!(colore_del_testo(&h, icone::ARROW_UPWARD), Some(tema::MATITA));
        assert!(colore_del_testo(&h, icone::FOLDER).is_none());
        let nome = rettangolo_del_testo(&h, "..").unwrap();
        let sotto = rettangolo_del_testo(&h, "La cartella che la contiene").unwrap();
        // Senza icona davanti, la riga sotto parte dove parte il nome.
        assert!((sotto.left() - nome.left()).abs() < 2.0 && nome.bottom() <= sotto.top(), "{nome:?} {sotto:?}");
        assert_eq!(colore_del_testo(&h, "La cartella che la contiene"), Some(tema::MATITA));
    }

    /// Una riga sotto che non ci sta in una riga va a capo, su due al più, e la voce si allunga
    /// a 88 dp, come le voci a tre righe di Android: la spiegazione si legge intera (0.8.3, a
    /// 372 dp «mai testi né nomi d…»). Più lunga di due righe finisce coi puntini; la voce con la
    /// riga corta resta di 72, e il luogo con l'icona resta su una riga.
    #[test]
    fn una_riga_sotto_lunga_va_a_capo_e_la_voce_si_allunga() {
        const LUNGA: &str = "Tempi, tocchi e tastiera: mai testi né nomi di file, e tutto resta nel telefono";
        const LUNGHISSIMA: &str = "Tempi, tocchi e tastiera: mai testi né nomi di file, e tutto resta nel telefono finché non decidi tu di mandarlo a qualcuno con Condividi";
        const LUOGO: &str = "da WhatsApp · ieri 19:40 · una cartella con un nome lunghissimo che non ci sta";
        let h = banco(
            |ui, misure: &mut Vec<egui::Rect>| {
                misure.clear();
                ui.spacing_mut().item_spacing.y = 0.0;
                for testo in [LUNGA, LUNGHISSIMA, "Corta"] {
                    misure.push(
                        Voce::new(Tipo::Cartella, "Registro").icona(icone::SPELLCHECK).sotto(testo).interruttore(true).mostra(ui).response.rect,
                    );
                }
                misure.push(Voce::new(Tipo::Pdf, "Bolletta.pdf").dove(icone::CHAT, LUOGO).menu("Altre azioni").mostra(ui).response.rect);
            },
            Vec::new(),
        );
        let misure = h.state();
        assert_eq!(misure.iter().map(|r| r.height()).collect::<Vec<_>>(), vec![88.0, 88.0, tema::VOCE, tema::VOCE]);
        // Una sotto l'altra, senza spazi.
        for coppia in misure.windows(2) {
            assert_eq!(coppia[0].bottom(), coppia[1].top());
        }
        assert_eq!(righe_del_testo(&h, LUNGA), Some((2, false)));
        assert_eq!(righe_del_testo(&h, LUNGHISSIMA), Some((2, true)));
        assert_eq!(righe_del_testo(&h, "Corta"), Some((1, false)));
        assert_eq!(righe_del_testo(&h, LUOGO), Some((1, true)));
        // Le due righe stanno nella voce, prima dell'interruttore, sotto il nome.
        let lunga = rettangolo_del_testo(&h, LUNGA).unwrap();
        assert!(misure[0].contains_rect(lunga) && lunga.right() <= SCHERMO.x - 16.0 - INTERRUTTORE.x, "{lunga:?}");
        let nome = h.get_all_by_label("Registro").next().unwrap().rect();
        assert_eq!(nome, misure[0]);
    }

    #[test]
    fn la_scheda_della_cartella_ha_icona_nome_e_numero() {
        let mut h = banco(
            |ui, tocchi: &mut u32| {
                let posto = egui::Rect::from_min_size(egui::pos2(14.0, 100.0), egui::vec2(187.0, 64.0));
                if SchedaCartella::new(icone::DOWNLOAD, "Download", "24 file").mostra_in(ui, posto).clicked() {
                    *tocchi += 1;
                }
            },
            0,
        );
        let nodo = h.get_by_label("Download");
        assert_eq!(nodo.rect(), egui::Rect::from_min_size(egui::pos2(14.0, 100.0), egui::vec2(187.0, 64.0)));
        // Bianca, arrotondata 16 dp, con i due strati d'ombra sotto.
        let tutte = forme(&h);
        assert!(tutte.iter().any(|s| matches!(s, egui::Shape::Rect(r) if r.fill == tema::FOGLIO && r.rect == nodo.rect() && r.corner_radius == egui::CornerRadius::same(tema::RAGGIO_CARTELLA))));
        assert_eq!(tutte.iter().filter(|s| matches!(s, egui::Shape::Rect(r) if r.blur_width > 0.0)).count(), 2);
        // L'icona grigia a 14 dp dal bordo, poi il nome in grassetto e il numero grigio sotto.
        assert_eq!(colore_del_testo(&h, icone::DOWNLOAD), Some(tema::MATITA));
        let icona = rettangolo_del_testo(&h, icone::DOWNLOAD).unwrap();
        let nome = rettangolo_del_testo(&h, "Download").unwrap();
        let numero = rettangolo_del_testo(&h, "24 file").unwrap();
        assert!(icona.left() >= 14.0 + 13.0 && icona.right() < nome.left(), "{icona:?} {nome:?}");
        assert!(nome.bottom() <= numero.top() + 1.0 && (nome.left() - numero.left()).abs() < 2.0, "{nome:?} {numero:?}");
        assert_eq!(famiglia_del_testo(&h, "Download"), Some(Carattere::Grassetto.famiglia()));
        assert_eq!(colore_del_testo(&h, "24 file"), Some(tema::MATITA));
        nodo.click();
        h.run_steps(2);
        assert_eq!(*h.state(), 1);
    }

    const ANGOLO: egui::Pos2 = egui::pos2(404.0, 72.0);

    fn menu_del_documento(ui: &mut egui::Ui, scelte: &mut Vec<SceltaMenu>) {
        let scelta = Menu::new(egui::Id::new("menu"))
            .primo(icone::SAVE, "Salva")
            .voce(icone::FILE_COPY, "Salva una copia")
            .voce(icone::PICTURE_AS_PDF, "Esporta in PDF")
            .separatore()
            .voce(icone::INFO, "Informazioni sul documento")
            .mostra(ui.ctx(), ANGOLO);
        scelte.extend(scelta);
    }

    #[test]
    fn il_menu_sta_sotto_l_angolo_con_le_icone_e_le_voci() {
        let h = banco(menu_del_documento, Vec::new());
        let salva = h.get_by_label("Salva").rect();
        let info = h.get_by_label("Informazioni sul documento").rect();
        assert_eq!(salva.width(), tema::MENU);
        assert_eq!(salva.right(), ANGOLO.x);
        assert_eq!(salva.top(), ANGOLO.y + 8.0);
        assert_eq!(salva.height(), tema::VOCE_MENU);
        // La voce lunga va a capo, su due righe, e sta ancora nei 48 dp come nel bozzetto.
        let testo = rettangolo_del_testo(&h, "Informazioni sul documento").unwrap();
        assert!(testo.height() > 30.0 && testo.width() <= tema::MENU - 58.0 - 18.0, "{testo:?}");
        assert!(info.contains_rect(testo), "{info:?} {testo:?}");
        assert_eq!(famiglia_del_testo(&h, "Salva"), Some(Carattere::Grassetto.famiglia()));
        assert_eq!(colore_del_testo(&h, icone::SAVE), Some(tema::BLU));
        assert_eq!(famiglia_del_testo(&h, "Salva una copia"), Some(egui::FontFamily::Proportional));
        assert_eq!(colore_del_testo(&h, icone::FILE_COPY), Some(tema::MATITA));
        let fili = riempiti(&h, tema::FILO);
        assert_eq!(fili.len(), 1);
        assert_eq!(fili[0].width(), tema::MENU);
        assert!(riempiti(&h, tema::FOGLIO).iter().any(|r| r.width() == tema::MENU && r.contains_rect(info)));
    }

    #[test]
    fn il_menu_sopra_sta_sopra_il_punto_e_dentro_lo_schermo() {
        for x in [200.0, 30.0] {
            let punto = egui::pos2(x, 700.0);
            let mut h = banco(
                move |ui, scelte: &mut Vec<SceltaMenu>| {
                    let scelta = Menu::new("sopra")
                        .voce(icone::DELETE, "Elimina la riga")
                        .voce(icone::DELETE, "Elimina la tabella")
                        .sopra()
                        .mostra(ui.ctx(), punto);
                    scelte.extend(scelta);
                },
                Vec::new(),
            );
            h.run_steps(2);
            let riga = h.get_by_label("Elimina la riga").rect();
            let tabella = h.get_by_label("Elimina la tabella").rect();
            assert!((tabella.bottom() + 8.0 - punto.y).abs() < 0.5, "{tabella:?}");
            assert!(riga.left() >= 0.0 && riga.right() <= SCHERMO.x, "{riga:?}");
            if x == 200.0 {
                assert!((riga.center().x - x).abs() < 0.5, "{riga:?}");
            }
            h.get_by_label("Elimina la tabella").click();
            h.run_steps(2);
            assert_eq!(h.state(), &vec![SceltaMenu::Voce(1)]);
        }
    }

    /// Una voce spiegata (segnalazione dell'autore del 10/11: le copie del menu del PDF non si
    /// capiva in che cosa fossero diverse): sotto l'etichetta una riga grigia più piccola,
    /// nella stessa colonna; la voce si allunga e si tocca anche lì; le altre restano di 48 dp.
    #[test]
    fn una_voce_spiegata_ha_sotto_una_riga_grigia_e_si_tocca_anche_li() {
        const SPIEGA: &str = "Questo file, senza le versioni di prima";
        let mut h = banco(
            |ui, scelte: &mut Vec<SceltaMenu>| {
                let scelta = Menu::new("spiegato")
                    .primo(icone::SAVE, "Salva")
                    .voce_spiegata(icone::FILE_COPY, "Salva una copia", SPIEGA)
                    .voce(icone::SHARE, "Condividi")
                    .mostra(ui.ctx(), ANGOLO);
                scelte.extend(scelta);
            },
            Vec::new(),
        );
        h.run_steps(2);
        let salva = h.get_by_label("Salva").rect();
        let copia = h.get_by_label("Salva una copia").rect();
        let condividi = h.get_by_label("Condividi").rect();
        assert_eq!(salva.height(), tema::VOCE_MENU);
        assert_eq!(condividi.height(), tema::VOCE_MENU);
        let etichetta = rettangolo_del_testo(&h, "Salva una copia").unwrap();
        let spiega = rettangolo_del_testo(&h, SPIEGA).unwrap();
        assert!(copia.height() > tema::VOCE_MENU, "{copia:?}");
        assert!(copia.contains_rect(etichetta) && copia.contains_rect(spiega), "{copia:?} {etichetta:?} {spiega:?}");
        assert!(spiega.top() >= etichetta.bottom() - 4.0, "la spiegazione sta sotto l'etichetta: {etichetta:?} {spiega:?}");
        assert!((spiega.left() - etichetta.left()).abs() < 1.5, "stessa colonna: {etichetta:?} {spiega:?}");
        assert!(spiega.width() <= tema::MENU - 58.0 - 18.0, "{spiega:?}");
        assert!(spiega.top() - copia.top() >= 8.0 && copia.bottom() - spiega.bottom() >= 8.0, "aria sopra e sotto: {copia:?} {spiega:?}");
        assert_eq!(colore_del_testo(&h, SPIEGA), Some(tema::MATITA));
        assert_eq!(colore_del_testo(&h, "Salva una copia"), Some(tema::INCHIOSTRO));
        // La spiegazione si legge anche da sola (lettore dello schermo, prove), dentro la voce.
        assert!(copia.contains_rect(h.get_by_label(SPIEGA).rect()));
        // Un tocco sulla spiegazione sceglie la voce.
        tocco(&mut h, spiega.center());
        h.run_steps(2);
        assert_eq!(h.state(), &vec![SceltaMenu::Voce(1)]);
    }

    /// Una voce rossa (segnalazione dell'autore del 10/11: «seleziona in evidenza in rosso la
    /// copia definitiva»): l'icona, l'etichetta e la riga sotto sono rosse, le voci vicine restano
    /// com'erano, e si tocca come le altre.
    #[test]
    fn una_voce_rossa_ha_icona_etichetta_e_spiegazione_rosse() {
        const SPIEGA: &str = "Non si può più modificare";
        let mut h = banco(
            |ui, scelte: &mut Vec<SceltaMenu>| {
                let scelta = Menu::new("rosso")
                    .primo(icone::SAVE, "Salva")
                    .voce_spiegata(icone::FILE_COPY, "Salva una copia", "Si può ancora modificare")
                    .voce_rossa(icone::PICTURE_AS_PDF, "Copia definitiva", SPIEGA)
                    .mostra(ui.ctx(), ANGOLO);
                scelte.extend(scelta);
            },
            Vec::new(),
        );
        h.run_steps(2);
        for testo in ["Copia definitiva", SPIEGA, icone::PICTURE_AS_PDF] {
            assert_eq!(colore_del_testo(&h, testo), Some(tema::ROSSO), "«{testo}» non è rosso");
        }
        assert_eq!(colore_del_testo(&h, "Salva una copia"), Some(tema::INCHIOSTRO));
        assert_eq!(colore_del_testo(&h, "Si può ancora modificare"), Some(tema::MATITA));
        assert_eq!(colore_del_testo(&h, icone::FILE_COPY), Some(tema::MATITA));
        let definitiva = h.get_by_label("Copia definitiva").rect();
        assert!(definitiva.contains_rect(rettangolo_del_testo(&h, SPIEGA).unwrap()));
        h.get_by_label("Copia definitiva").click();
        h.run_steps(2);
        assert_eq!(h.state(), &vec![SceltaMenu::Voce(2)]);
    }

    #[test]
    fn toccare_una_voce_la_sceglie_toccare_fuori_lo_chiude() {
        let mut h = banco(menu_del_documento, Vec::new());
        h.get_by_label("Esporta in PDF").click();
        h.run_steps(2);
        h.get_by_label("Informazioni sul documento").click();
        h.run_steps(2);
        tocco(&mut h, egui::pos2(40.0, 800.0));
        assert_eq!(h.state(), &vec![SceltaMenu::Voce(2), SceltaMenu::Voce(3), SceltaMenu::Fuori]);
    }
}
