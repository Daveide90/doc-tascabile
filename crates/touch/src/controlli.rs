//! I controlli dei fogli: il gruppo segmentato, il passo con meno e più, il campione di uno stile e
//! la fila che scorre, la tavolozza dei colori, l'etichetta di sezione maiuscola.

use egui::scroll_area::{ScrollBarVisibility, ScrollSource};
use egui::text::LayoutJob;
use egui::{
    Align2, Color32, CornerRadius, Mesh, Rect, Response, ScrollArea, Sense, Shape, Stroke, StrokeKind, TextFormat, Ui, UiBuilder, Vec2, Widget,
    WidgetInfo, WidgetType, pos2, vec2,
};

use crate::icone;
use crate::pulsanti::{PulsanteIcona, info};
use crate::tema::{self, Carattere};

/// Il bordo interno del velo attorno ai segmenti (`.gruppo { padding: 4px }`).
const BORDO: f32 = 4.0;
/// Fra due segmenti (`.gruppo { gap: 2px }`).
const FRA: f32 = 2.0;
/// L'altezza di un gruppo o di un passo.
const ALTO: f32 = tema::SEGMENTO.y + 2.0 * BORDO;

/// Il posto del segmento che parte da `x`, nel velo `rect`.
fn segmento(rect: Rect, x: f32) -> Rect {
    Rect::from_min_size(pos2(x, rect.top() + BORDO), tema::SEGMENTO)
}

/// Un gruppo segmentato (`.gruppo`): segmenti con un'icona su un velo arrotondato. Con un
/// segmento acceso (allineamento) è una scelta fra le voci: l'acceso è bianco con l'icona blu;
/// senza (colore, evidenziatore, togli formato) è una fila di azioni.
#[derive(Debug, Clone)]
pub struct GruppoSegmentato<'a> {
    voci: &'a [(&'a str, &'a str)],
    acceso: Option<Option<usize>>,
}

impl<'a> GruppoSegmentato<'a> {
    /// Le voci sono (icona, etichetta per l'accessibilità).
    pub fn new(voci: &'a [(&'a str, &'a str)]) -> Self {
        Self { voci, acceso: None }
    }

    /// Una scelta fra le voci, con questa accesa (o nessuna).
    pub fn acceso(mut self, acceso: Option<usize>) -> Self {
        self.acceso = Some(acceso);
        self
    }

    /// Disegna il gruppo e dice quale voce è stata toccata.
    pub fn mostra(self, ui: &mut Ui) -> Option<usize> {
        let n = self.voci.len() as f32;
        let largo = 2.0 * BORDO + n * tema::SEGMENTO.x + (n - 1.0).max(0.0) * FRA;
        let (rect, _) = ui.allocate_exact_size(vec2(largo, ALTO), Sense::hover());
        // L'id dall'etichetta della prima voce, non dal posto: col foglio che sale o cambia
        // altezza il gruppo si sposta, e un tocco a metà andrebbe perso.
        let prima = self.voci.first().map_or("", |v| v.1);
        let mut ui = ui.new_child(UiBuilder::new().max_rect(rect).id_salt(("gruppo", prima)));
        ui.painter().rect_filled(rect, CornerRadius::same(tema::RAGGIO_GRUPPO), tema::VELO);
        let mut scelto = None;
        for (i, &(icona, etichetta)) in self.voci.iter().enumerate() {
            let posto = segmento(rect, rect.left() + BORDO + i as f32 * (tema::SEGMENTO.x + FRA));
            let mut p = PulsanteIcona::new(icona, etichetta).su_velo();
            if let Some(acceso) = self.acceso {
                let questo = acceso == Some(i);
                if questo {
                    tema::dipingi_ombra(ui.painter(), posto, tema::RAGGIO_SEGMENTO, &tema::OMBRA_ACCESO);
                }
                p = p.acceso(questo);
            }
            if p.mostra_in(&mut ui, posto, tema::RAGGIO_SEGMENTO).clicked() {
                scelto = Some(i);
            }
        }
        scelto
    }
}

/// Da che parte va un [`Passo`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verso {
    Meno,
    Piu,
}

/// Un passo (`.passo`): meno, il valore in grassetto, più, sul velo arrotondato.
#[derive(Debug, Clone)]
pub struct Passo<'a> {
    valore: &'a str,
    meno: &'a str,
    piu: &'a str,
    meno_spento: bool,
    piu_spento: bool,
}

impl<'a> Passo<'a> {
    /// `meno` e `piu` sono le etichette dei due pulsanti per l'accessibilità («Più piccolo»).
    pub fn new(valore: &'a str, meno: &'a str, piu: &'a str) -> Self {
        Self { valore, meno, piu, meno_spento: false, piu_spento: false }
    }

    /// Al minimo: meno si spegne.
    pub fn meno_spento(mut self, spento: bool) -> Self {
        self.meno_spento = spento;
        self
    }

    /// Al massimo: più si spegne.
    pub fn piu_spento(mut self, spento: bool) -> Self {
        self.piu_spento = spento;
        self
    }

    /// Disegna il passo e dice da che parte è stato toccato.
    pub fn mostra(self, ui: &mut Ui) -> Option<Verso> {
        // Il valore sta fra i due pulsanti, largo almeno quanto la sua scritta: «Pagina 2 di 2» del
        // link andava sotto il meno e il più (segnalazione dell'autore del 3/11).
        const VALORE: f32 = 56.0;
        let font = Carattere::Grassetto.font(ui.ctx(), 18.0);
        let scritta = ui.painter().layout_no_wrap(self.valore.to_string(), font.clone(), tema::INCHIOSTRO).size().x;
        let largo_valore = VALORE.max(scritta + 16.0);
        let largo = 2.0 * BORDO + 2.0 * tema::SEGMENTO.x + largo_valore;
        let (rect, _) = ui.allocate_exact_size(vec2(largo, ALTO), Sense::hover());
        let mut ui = ui.new_child(UiBuilder::new().max_rect(rect).id_salt(("passo", self.meno)));
        ui.painter().rect_filled(rect, CornerRadius::same(tema::RAGGIO_GRUPPO), tema::VELO);
        let meno = segmento(rect, rect.left() + BORDO);
        let valore = Rect::from_min_size(pos2(meno.right(), meno.top()), vec2(largo_valore, tema::SEGMENTO.y));
        let piu = segmento(rect, valore.right());
        ui.painter().text(valore.center(), Align2::CENTER_CENTER, self.valore, font, tema::INCHIOSTRO);
        ui.allocate_rect(valore, Sense::hover()).widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, self.valore));
        let mut scelto = None;
        if PulsanteIcona::new(icone::REMOVE, self.meno).su_velo().spento(self.meno_spento).mostra_in(&mut ui, meno, tema::RAGGIO_SEGMENTO).clicked() {
            scelto = Some(Verso::Meno);
        }
        if PulsanteIcona::new(icone::ADD, self.piu).su_velo().spento(self.piu_spento).mostra_in(&mut ui, piu, tema::RAGGIO_SEGMENTO).clicked() {
            scelto = Some(Verso::Piu);
        }
        scelto
    }
}

/// Il raggio dei campioni di stile (`.stile { border-radius: 14px }`).
const RAGGIO_CAMPIONE: u8 = 14;
/// La misura di un campione di stile: alto 64, largo almeno 92 (`.stile`).
const CAMPIONE: Vec2 = vec2(92.0, 64.0);

/// Un campione di stile (`.stile`): il nome dello stile scritto nel suo aspetto (carattere, misura
/// e colore), in un riquadro col bordo sottile; acceso ha il bordo blu sul blu tenue. È un
/// interruttore: lo stile del paragrafo del cursore è acceso.
#[derive(Debug, Clone)]
pub struct Campione<'a> {
    testo: &'a str,
    carattere: Carattere,
    misura: f32,
    colore: Color32,
    acceso: bool,
}

impl<'a> Campione<'a> {
    pub fn new(testo: &'a str, carattere: Carattere, misura: f32, colore: Color32) -> Self {
        Self { testo, carattere, misura, colore, acceso: false }
    }

    pub fn acceso(mut self, acceso: bool) -> Self {
        self.acceso = acceso;
        self
    }
}

impl Widget for Campione<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let g = ui.painter().layout_no_wrap(self.testo.to_owned(), self.carattere.font(ui.ctx(), self.misura), self.colore);
        let largo = (g.size().x + 28.0).max(CAMPIONE.x);
        let (rect, r) = ui.allocate_exact_size(vec2(largo, CAMPIONE.y), Sense::click());
        info(&r, true, Some(self.acceso), self.testo);
        if ui.is_rect_visible(rect) {
            let fondo = if r.is_pointer_button_down_on() {
                tema::PREMUTO
            } else if self.acceso {
                tema::BLU_TENUE
            } else {
                Color32::TRANSPARENT
            };
            let bordo = if self.acceso { tema::BLU } else { tema::FILO };
            ui.painter().rect(rect, CornerRadius::same(RAGGIO_CAMPIONE), fondo, Stroke::new(1.5, bordo), StrokeKind::Inside);
            ui.painter().galley(rect.center() - g.size() / 2.0, g, self.colore);
        }
        r
    }
}

/// Una fila che scorre in orizzontale col dito, senza barra di scorrimento, con 8 dp fra gli
/// elementi. Se continua oltre il bordo destro, l'ultimo 15% sfuma nel colore `fondo` (`.stili {
/// mask-image }`): si capisce che c'è dell'altro.
pub fn fila_scorrevole<R>(ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug, fondo: Color32, contenuto: impl FnOnce(&mut Ui) -> R) -> R {
    ui.scope(|ui| {
        // La sfumatura di egui (mezza, 20 dp, sul fondo della Ui) no: c'è quella del bozzetto.
        ui.spacing_mut().scroll.fade.strength = 0.0;
        let out = ScrollArea::horizontal()
            .id_salt(id)
            .scroll_bar_visibility(ScrollBarVisibility::AlwaysHidden)
            .scroll_source(ScrollSource::ALL)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = tema::SPAZIO;
                let r = ui.horizontal(contenuto);
                (r.inner, r.response.rect)
            });
        let (fuori, fila) = out.inner;
        let vista = out.inner_rect;
        let resto = out.content_size.x - out.state.offset.x - vista.width();
        if resto > 0.5 {
            let sfuma = Rect::from_x_y_ranges(vista.right() - vista.width() * 0.15..=vista.right(), fila.y_range());
            let mut mesh = Mesh::default();
            for (punto, colore) in [
                (sfuma.left_top(), Color32::TRANSPARENT),
                (sfuma.right_top(), fondo),
                (sfuma.right_bottom(), fondo),
                (sfuma.left_bottom(), Color32::TRANSPARENT),
            ] {
                mesh.colored_vertex(punto, colore);
            }
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(0, 2, 3);
            ui.painter().add(Shape::mesh(mesh));
        }
        fuori
    })
    .inner
}

/// Una tinta della [`Tavolozza`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tinta {
    /// Un colore pieno.
    Colore(Color32),
    /// Il colore automatico del testo (nero sulla pagina bianca): un cerchio bianco con la A.
    Automatico,
    /// Niente (l'evidenziatore tolto): un cerchio bianco barrato di rosso.
    Nessuno,
}

/// Il raggio del cerchio di una tinta; l'anello di quella scelta sta 4 dp fuori.
const RAGGIO_TINTA: f32 = 15.0;
/// Il bersaglio più stretto di una tinta in una fila sola: l'anello di quella scelta (43 dp) ci
/// sta senza toccare il cerchio vicino.
pub const TINTA_MINIMA: f32 = 46.0;

/// Una piccola tavolozza (colore del testo, evidenziatore): le tinte in fila, ognuna in un
/// bersaglio alto 48 dp, larghe uguali nello spazio che c'è; quella scelta ha l'anello blu.
#[derive(Debug, Clone)]
pub struct Tavolozza<'a> {
    tinte: &'a [(Tinta, &'a str)],
    acceso: Option<usize>,
}

impl<'a> Tavolozza<'a> {
    /// Le tinte sono (tinta, nome per l'accessibilità e le prove).
    pub fn new(tinte: &'a [(Tinta, &'a str)]) -> Self {
        Self { tinte, acceso: None }
    }

    /// La tinta di adesso, cerchiata (nessuna con una selezione di più colori).
    pub fn acceso(mut self, acceso: Option<usize>) -> Self {
        self.acceso = acceso;
        self
    }

    /// Disegna la tavolozza e dice quale tinta è stata toccata. Se in una fila ogni tinta avrebbe
    /// meno di [`TINTA_MINIMA`] dp (nove colori su un telefono da 372 dp: 0.8.3), le tinte vanno su
    /// due file.
    pub fn mostra(self, ui: &mut Ui) -> Option<usize> {
        let n = self.tinte.len().max(1);
        let largo = ui.available_width();
        let per_fila = if largo / n as f32 >= TINTA_MINIMA { n } else { n.div_ceil(2) };
        let file = n.div_ceil(per_fila);
        let (rect, _) = ui.allocate_exact_size(vec2(largo, tema::PULSANTE * file as f32), Sense::hover());
        let passo = largo / per_fila as f32;
        let mut scelto = None;
        for (i, &(tinta, etichetta)) in self.tinte.iter().enumerate() {
            let (fila, colonna) = (i / per_fila, i % per_fila);
            let posto = Rect::from_min_size(
                pos2(rect.left() + colonna as f32 * passo, rect.top() + fila as f32 * tema::PULSANTE),
                vec2(passo, tema::PULSANTE),
            );
            // Due tavolozze diverse (colore ed evidenziatore) non hanno gli stessi id.
            let r = ui.interact(posto, ui.id().with(("tinta", self.tinte[0].1, i)), Sense::click());
            let acceso = self.acceso == Some(i);
            info(&r, true, Some(acceso), etichetta);
            if r.clicked() {
                scelto = Some(i);
            }
            if !ui.is_rect_visible(posto) {
                continue;
            }
            let p = ui.painter();
            let c = posto.center();
            if r.is_pointer_button_down_on() {
                p.circle_filled(c, RAGGIO_TINTA + 6.0, tema::PREMUTO);
            }
            if acceso {
                p.circle_stroke(c, RAGGIO_TINTA + 4.0, Stroke::new(2.5, tema::BLU));
            }
            let filo = Stroke::new(1.0, tema::FILO);
            match tinta {
                Tinta::Colore(colore) => {
                    p.circle(c, RAGGIO_TINTA, colore, filo);
                }
                Tinta::Automatico => {
                    p.circle(c, RAGGIO_TINTA, tema::FOGLIO, filo);
                    p.text(c, Align2::CENTER_CENTER, "A", Carattere::Grassetto.font(ui.ctx(), 16.0), tema::INCHIOSTRO);
                }
                Tinta::Nessuno => {
                    p.circle(c, RAGGIO_TINTA, tema::FOGLIO, filo);
                    let d = RAGGIO_TINTA * std::f32::consts::FRAC_1_SQRT_2;
                    p.line_segment([c + vec2(-d, d), c + vec2(d, -d)], Stroke::new(2.0, tema::ROSSO));
                }
            }
        }
        scelto
    }
}

/// «3 righe e 4 colonne», «1 riga e 1 colonna»: la misura di una tabella a parole.
pub fn misura_tabella(righe: usize, colonne: usize) -> String {
    let r = if righe == 1 { "riga" } else { "righe" };
    let c = if colonne == 1 { "colonna" } else { "colonne" };
    format!("{righe} {r} e {colonne} {c}")
}

/// Il lato più grande di un quadretto della [`GrigliaTabella`], e lo spazio fra due, in dp.
const QUADRETTO: f32 = 40.0;
const FRA_QUADRETTI: f32 = 6.0;

/// La griglia di «Tabella» nel foglio Inserisci: `righe` per `colonne` quadretti, al centro dello
/// spazio. Quelli fino alla misura scelta sono blu tenue col bordo blu, gli altri bianchi col
/// filo. Un tocco su un quadretto, o il dito che ci passa sopra, sceglie la tabella che arriva
/// fin lì (i quadretti hanno la misura a parole, per l'accessibilità e le prove).
#[derive(Debug, Clone, Copy)]
pub struct GrigliaTabella {
    righe: usize,
    colonne: usize,
    scelta: (usize, usize),
}

impl GrigliaTabella {
    /// La griglia, con la misura scelta adesso (righe, colonne).
    pub fn new(righe: usize, colonne: usize, scelta: (usize, usize)) -> Self {
        Self { righe: righe.max(1), colonne: colonne.max(1), scelta }
    }

    /// Disegna la griglia e dice la misura scelta col dito in questo frame.
    pub fn mostra(self, ui: &mut Ui) -> Option<(usize, usize)> {
        let (righe, colonne) = (self.righe as f32, self.colonne as f32);
        let largo = ui.available_width();
        let lato = ((largo - (colonne - 1.0) * FRA_QUADRETTI) / colonne).clamp(8.0, QUADRETTO);
        let misura = vec2(colonne * lato + (colonne - 1.0) * FRA_QUADRETTI, righe * lato + (righe - 1.0) * FRA_QUADRETTI);
        let (posto, _) = ui.allocate_exact_size(vec2(largo, misura.y), Sense::hover());
        let griglia = Rect::from_min_size(pos2(posto.center().x - misura.x / 2.0, posto.top()), misura);
        // Sotto i quadretti, il dito che passa sopra la griglia.
        let dito = ui.interact(griglia, ui.id().with("griglia della tabella"), Sense::drag());
        let quadretto = |r: usize, c: usize| {
            Rect::from_min_size(
                pos2(griglia.left() + c as f32 * (lato + FRA_QUADRETTI), griglia.top() + r as f32 * (lato + FRA_QUADRETTI)),
                Vec2::splat(lato),
            )
        };
        let mut scelta = None;
        for r in 0..self.righe {
            for c in 0..self.colonne {
                let q = quadretto(r, c);
                let risposta = ui.interact(q, ui.id().with(("quadretto", r, c)), Sense::click());
                let dentro = r < self.scelta.0 && c < self.scelta.1;
                info(&risposta, true, Some(dentro), &misura_tabella(r + 1, c + 1));
                if risposta.clicked() {
                    scelta = Some((r + 1, c + 1));
                }
                let (fondo, filo) =
                    if dentro { (tema::BLU_TENUE, Stroke::new(1.5, tema::BLU)) } else { (tema::FOGLIO, Stroke::new(1.0, tema::FILO)) };
                ui.painter().rect(q, 4.0, fondo, filo, StrokeKind::Inside);
            }
        }
        if dito.dragged()
            && let Some(p) = dito.interact_pointer_pos()
        {
            let passo = lato + FRA_QUADRETTI;
            let a = |v: f32, n: usize| ((v / passo).floor() as isize).clamp(0, n as isize - 1) as usize + 1;
            scelta = Some((a(p.y - griglia.top(), self.righe), a(p.x - griglia.left(), self.colonne)));
        }
        scelta.filter(|s| *s != self.scelta)
    }
}

/// L'etichetta di una sezione di un foglio (`.etichetta`): maiuscola, grigia, in grassetto da 13
/// dp, un po' spaziata, con 16 dp sopra e lo spazio fra le righe di egui sotto.
pub fn etichetta_sezione(ui: &mut Ui, testo: &str) -> Response {
    let maiuscolo = testo.to_uppercase();
    let mut job = LayoutJob::default();
    let formato =
        TextFormat { font_id: Carattere::Grassetto.font(ui.ctx(), 13.0), color: tema::MATITA, extra_letter_spacing: 0.4, ..Default::default() };
    job.append(&maiuscolo, 0.0, formato);
    let galley = ui.painter().layout_job(job);
    let sopra = (16.0 - ui.spacing().item_spacing.y).max(0.0);
    let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), sopra + galley.size().y), Sense::hover());
    ui.painter().galley(pos2(rect.left() + 2.0, rect.top() + sopra), galley, Color32::PLACEHOLDER);
    r.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, &maiuscolo));
    r
}

/// Il titolo di una sezione della Home (`.titoletto`): Fraunces da 20 dp a 22 dal bordo, 16 sopra e
/// 8 sotto; con `collegamento`, a destra un testo blu in grassetto da toccare (sulla stessa linea
/// di base), la cui risposta torna a chi chiama.
pub fn titoletto(ui: &mut Ui, testo: &str, collegamento: Option<&str>) -> Option<Response> {
    const SOPRA: f32 = 16.0;
    const RIGA: f32 = 27.0;
    const SOTTO: f32 = 8.0;
    let ctx = ui.ctx().clone();
    let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), SOPRA + RIGA + SOTTO), Sense::hover());
    let titolo = ui.painter().layout_no_wrap(testo.to_owned(), Carattere::Titolo.font(&ctx, 20.0), tema::INCHIOSTRO);
    let riga = Rect::from_min_size(pos2(rect.left(), rect.top() + SOPRA), vec2(rect.width(), RIGA));
    let y = riga.center().y - titolo.size().y / 2.0;
    let base = y + linea_di_base(&titolo);
    let posto = Rect::from_min_size(pos2(rect.left() + tema::MARGINE_TITOLO, y), titolo.size());
    ui.painter().galley(posto.min, titolo, Color32::PLACEHOLDER);
    r.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, testo));
    collegamento.map(|c| {
        let g = ui.painter().layout_no_wrap(c.to_owned(), Carattere::Grassetto.font(&ctx, 14.0), tema::BLU);
        let origine = pos2(rect.right() - tema::MARGINE_TITOLO - g.size().x, base - linea_di_base(&g));
        let testo = Rect::from_min_size(origine, g.size());
        // Il bersaglio del dito: 48 dp di altezza attorno al testo, 8 di più ai lati.
        let bersaglio = Rect::from_center_size(testo.center(), vec2(testo.width() + 16.0, tema::PULSANTE.max(testo.height())));
        let risposta = ui.interact(bersaglio, ui.id().with(("titoletto", c)), Sense::click());
        risposta.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, c));
        if risposta.is_pointer_button_down_on() {
            ui.painter().rect_filled(bersaglio, CornerRadius::same(tema::RAGGIO_SEGMENTO), tema::PREMUTO);
        }
        ui.painter().galley(origine, g, Color32::PLACEHOLDER);
        risposta
    })
}

/// La linea di base della prima riga di un testo, dalla sua cima.
fn linea_di_base(g: &egui::Galley) -> f32 {
    g.rows.first().map_or(g.size().y, |r| r.pos.y + r.glyphs.first().map_or(r.height(), |c| c.pos.y))
}

#[cfg(test)]
mod test {
    use egui::accesskit::Toggled;
    use egui_kittest::kittest::{NodeT, Queryable};

    use super::*;
    use crate::icone;
    use crate::prova::{SCHERMO, banco, colore_del_testo, famiglia_del_testo, forme, rettangolo_del_testo, riempiti};

    #[test]
    fn il_titoletto_e_in_fraunces_col_collegamento_blu_a_destra() {
        let mut h = banco(
            |ui, tocchi: &mut u32| {
                if titoletto(ui, "Recenti", Some("Vedi tutti")).is_some_and(|r| r.clicked()) {
                    *tocchi += 1;
                }
                titoletto(ui, "Cartelle", None);
            },
            0,
        );
        assert_eq!(famiglia_del_testo(&h, "Recenti"), Some(Carattere::Titolo.famiglia()));
        assert_eq!(colore_del_testo(&h, "Recenti"), Some(tema::INCHIOSTRO));
        assert_eq!(colore_del_testo(&h, "Vedi tutti"), Some(tema::BLU));
        assert_eq!(famiglia_del_testo(&h, "Vedi tutti"), Some(Carattere::Grassetto.famiglia()));
        let titolo = rettangolo_del_testo(&h, "Recenti").unwrap();
        let link = rettangolo_del_testo(&h, "Vedi tutti").unwrap();
        // 22 dp dai bordi, 16 sopra il titolo.
        assert!((titolo.left() - 22.0).abs() < 2.0 && (link.right() - (SCHERMO.x - 22.0)).abs() < 2.0, "{titolo:?} {link:?}");
        assert!(titolo.top() >= 16.0 && titolo.top() < 26.0, "{titolo:?}");
        // Sulla stessa linea di base: il fondo delle lettere senza gambe quasi alla stessa altezza.
        assert!((titolo.bottom() - link.bottom()).abs() < 4.0, "{titolo:?} {link:?}");
        // Il collegamento si tocca anche oltre il testo, 48 dp di altezza.
        let nodo = h.get_by_label("Vedi tutti");
        assert!(nodo.rect().height() >= tema::PULSANTE && nodo.rect().contains_rect(link));
        nodo.click();
        h.run_steps(2);
        assert_eq!(*h.state(), 1);
        // Il secondo titoletto sta sotto il primo: 16 sopra, 27 la riga, 8 sotto.
        let cartelle = rettangolo_del_testo(&h, "Cartelle").unwrap();
        assert!(cartelle.top() > titolo.bottom() + 16.0, "{titolo:?} {cartelle:?}");
    }

    const ALLINEAMENTI: [(&str, &str); 4] = [
        (icone::FORMAT_ALIGN_LEFT, "A sinistra"),
        (icone::FORMAT_ALIGN_CENTER, "Al centro"),
        (icone::FORMAT_ALIGN_RIGHT, "A destra"),
        (icone::FORMAT_ALIGN_JUSTIFY, "Giustificato"),
    ];

    #[test]
    fn il_gruppo_segmentato_accende_un_segmento_bianco() {
        let mut h = banco(
            |ui, acceso: &mut Option<usize>| {
                if let Some(i) = GruppoSegmentato::new(&ALLINEAMENTI).acceso(*acceso).mostra(ui) {
                    *acceso = Some(i);
                }
            },
            Some(0),
        );
        let fondi = riempiti(&h, tema::VELO);
        assert_eq!(fondi.len(), 1, "{fondi:?}");
        assert_eq!(fondi[0].size(), egui::vec2(4.0 + 4.0 * 46.0 + 3.0 * 2.0 + 4.0, 52.0));
        let sinistra = h.get_by_label("A sinistra");
        assert_eq!(sinistra.rect().size(), tema::SEGMENTO);
        assert_eq!(sinistra.rect().min, fondi[0].min + egui::vec2(4.0, 4.0));
        assert_eq!(sinistra.accesskit_node().toggled(), Some(Toggled::True));
        assert_eq!(h.get_by_label("Al centro").accesskit_node().toggled(), Some(Toggled::False));
        // Acceso: bianco con l'ombra, icona blu; gli altri inchiostro, senza fondo.
        assert_eq!(riempiti(&h, tema::FOGLIO), vec![sinistra.rect()]);
        assert!(forme(&h).iter().any(|s| matches!(s, egui::Shape::Rect(r) if r.blur_width > 0.0)));
        assert!(riempiti(&h, tema::BLU_TENUE).is_empty());
        assert_eq!(colore_del_testo(&h, icone::FORMAT_ALIGN_LEFT), Some(tema::BLU));
        assert_eq!(colore_del_testo(&h, icone::FORMAT_ALIGN_CENTER), Some(tema::INCHIOSTRO));
        h.get_by_label("Al centro").click();
        h.run_steps(2);
        assert_eq!(*h.state(), Some(1));
        assert_eq!(colore_del_testo(&h, icone::FORMAT_ALIGN_CENTER), Some(tema::BLU));
    }

    #[test]
    fn un_gruppo_di_azioni_non_ha_interruttori() {
        const AZIONI: [(&str, &str); 2] = [(icone::FORMAT_COLOR_TEXT, "Colore del testo"), (icone::FORMAT_CLEAR, "Togli il formato")];
        let h = banco(|ui, _: &mut ()| _ = GruppoSegmentato::new(&AZIONI).mostra(ui), ());
        assert_eq!(h.get_by_label("Colore del testo").accesskit_node().toggled(), None);
        assert!(riempiti(&h, tema::FOGLIO).is_empty());
    }

    #[test]
    fn il_passo_ha_meno_il_valore_e_piu() {
        let mut h = banco(
            |ui, visti: &mut Vec<Verso>| {
                visti.extend(Passo::new("11", "Più piccolo", "Più grande").mostra(ui));
            },
            Vec::new(),
        );
        let fondi = riempiti(&h, tema::VELO);
        assert_eq!(fondi.len(), 1, "{fondi:?}");
        assert_eq!(fondi[0].size(), egui::vec2(4.0 + 46.0 + 56.0 + 46.0 + 4.0, 52.0));
        let meno = h.get_by_label("Più piccolo").rect();
        let piu = h.get_by_label("Più grande").rect();
        let valore = rettangolo_del_testo(&h, "11").unwrap();
        assert!(meno.right() < valore.left() && valore.right() < piu.left(), "{meno:?} {valore:?} {piu:?}");
        assert!((valore.center().x - fondi[0].center().x).abs() < 1.0);
        assert_eq!(famiglia_del_testo(&h, "11"), Some(Carattere::Grassetto.famiglia()));
        assert_eq!(colore_del_testo(&h, icone::REMOVE), Some(tema::INCHIOSTRO));
        h.get_by_label("Più grande").click();
        h.run_steps(2);
        h.get_by_label("Più piccolo").click();
        h.run_steps(2);
        assert_eq!(h.state(), &vec![Verso::Piu, Verso::Meno]);
    }

    #[test]
    fn un_valore_lungo_allarga_il_passo_e_non_va_sotto_i_pulsanti() {
        let h = banco(
            |ui, visti: &mut Vec<Verso>| {
                visti.extend(Passo::new("Pagina 12 di 20", "Pagina prima", "Pagina dopo").mostra(ui));
            },
            Vec::new(),
        );
        let meno = h.get_by_label("Pagina prima").rect();
        let piu = h.get_by_label("Pagina dopo").rect();
        let valore = rettangolo_del_testo(&h, "Pagina 12 di 20").unwrap();
        assert!(meno.right() <= valore.left() && valore.right() <= piu.left(), "{meno:?} {valore:?} {piu:?}");
    }

    #[test]
    fn il_passo_al_limite_spegne_il_suo_lato() {
        let mut h = banco(
            |ui, visti: &mut Vec<Verso>| {
                visti.extend(Passo::new("1", "Più piccolo", "Più grande").meno_spento(true).mostra(ui));
            },
            Vec::new(),
        );
        assert_eq!(colore_del_testo(&h, icone::REMOVE), Some(tema::SPENTO));
        h.get_by_label("Più piccolo").click();
        h.run_steps(2);
        assert!(h.state().is_empty());
    }

    /// I quattro stili del bozzetto 4, nell'aspetto del documento.
    fn stili(ui: &mut egui::Ui, scelto: &mut Option<usize>) {
        let titoli = Color32::from_rgb(0x1F, 0x3B, 0x63);
        let voci = [
            ("Normale", Carattere::Testo, 16.0, Color32::from_rgb(0x22, 0x22, 0x22)),
            ("Titolo 1", Carattere::Grassetto, 22.0, titoli),
            ("Titolo 2", Carattere::Grassetto, 19.0, titoli),
            ("Titolo 3", Carattere::Grassetto, 17.0, titoli),
        ];
        fila_scorrevole(ui, "stili", tema::FOGLIO, |ui| {
            for (i, (testo, carattere, misura, colore)) in voci.into_iter().enumerate() {
                if ui.add(Campione::new(testo, carattere, misura, colore).acceso(*scelto == Some(i))).clicked() {
                    *scelto = Some(i);
                }
            }
        });
    }

    #[test]
    fn i_campioni_degli_stili_scorrono_e_quello_acceso_e_blu() {
        let mut h = banco(
            |ui, s: &mut Option<usize>| {
                ui.add_space(20.0);
                ui.horizontal(|ui| {
                    ui.add_space(18.0);
                    ui.set_max_width(SCHERMO.x - 36.0);
                    stili(ui, s);
                });
            },
            Some(0),
        );
        let normale = h.get_by_label("Normale");
        // Alto 64, largo almeno 92 (`.stile`), acceso col bordo blu sul blu tenue.
        assert_eq!((normale.rect().height(), normale.rect().width()), (64.0, 92.0));
        assert_eq!(normale.accesskit_node().toggled(), Some(Toggled::True));
        assert_eq!(h.get_by_label("Titolo 1").accesskit_node().toggled(), Some(Toggled::False));
        let tutte = forme(&h);
        let bordo = |r: &egui::Rect| {
            tutte.iter().find_map(|s| match s {
                egui::Shape::Rect(x) if x.rect.shrink(1.0).contains_rect(r.shrink(1.0)) && x.stroke.width > 0.0 => Some(x.stroke.color),
                _ => None,
            })
        };
        assert_eq!(riempiti(&h, tema::BLU_TENUE), vec![normale.rect()]);
        assert_eq!(bordo(&normale.rect()), Some(tema::BLU));
        let titolo = h.get_by_label("Titolo 1").rect();
        assert_eq!(bordo(&titolo), Some(tema::FILO));
        assert!(titolo.width() > 92.0 && (titolo.left() - normale.rect().right() - 8.0).abs() < 0.5, "{titolo:?}");
        // Ogni campione nel suo aspetto.
        assert_eq!(famiglia_del_testo(&h, "Titolo 1"), Some(Carattere::Grassetto.famiglia()));
        assert_eq!(colore_del_testo(&h, "Titolo 2"), Some(Color32::from_rgb(0x1F, 0x3B, 0x63)));
        assert!(rettangolo_del_testo(&h, "Titolo 1").unwrap().height() > rettangolo_del_testo(&h, "Normale").unwrap().height());
        // Non stanno tutti: la fila finisce sfumata nel bianco del foglio.
        let sfumature: Vec<_> = tutte.iter().filter(|s| matches!(s, egui::Shape::Mesh(_))).collect();
        assert_eq!(sfumature.len(), 1, "{sfumature:?}");
        h.get_by_label("Titolo 2").click();
        h.run_steps(2);
        assert_eq!(*h.state(), Some(2));
        assert_eq!(riempiti(&h, tema::BLU_TENUE), vec![h.get_by_label("Titolo 2").rect()]);
    }

    #[test]
    fn una_fila_che_ci_sta_non_e_sfumata() {
        let h = banco(
            |ui, _: &mut ()| {
                fila_scorrevole(ui, "corta", tema::FOGLIO, |ui| ui.add(Campione::new("Normale", Carattere::Testo, 16.0, tema::INCHIOSTRO)));
            },
            (),
        );
        assert!(!forme(&h).iter().any(|s| matches!(s, egui::Shape::Mesh(_))));
    }

    const COLORI: [(Tinta, &str); 3] =
        [(Tinta::Automatico, "Automatico"), (Tinta::Colore(Color32::from_rgb(0xC0, 0, 0)), "Rosso scuro"), (Tinta::Nessuno, "Nessuno")];

    #[test]
    fn la_griglia_della_tabella_si_sceglie_col_tocco_e_col_dito() {
        assert_eq!(misura_tabella(3, 4), "3 righe e 4 colonne");
        assert_eq!(misura_tabella(1, 1), "1 riga e 1 colonna");
        let mut h = banco(
            |ui, scelta: &mut (usize, usize)| {
                ui.add_space(100.0);
                if let Some(s) = GrigliaTabella::new(8, 6, *scelta).mostra(ui) {
                    *scelta = s;
                }
            },
            (2, 2),
        );
        // Otto righe di sei quadretti da 40 dp, al centro.
        let primo = h.get_by_label("1 riga e 1 colonna").rect();
        let ultimo = h.get_by_label("8 righe e 6 colonne").rect();
        assert_eq!((primo.width(), primo.height()), (40.0, 40.0));
        assert!(((primo.left() + ultimo.right()) / 2.0 - SCHERMO.x / 2.0).abs() < 0.5, "{primo:?} {ultimo:?}");
        assert!((ultimo.bottom() - primo.top() - (8.0 * 40.0 + 7.0 * 6.0)).abs() < 0.5);
        // Quelli fino alla scelta sono accesi, in blu tenue.
        assert_eq!(h.get_by_label("2 righe e 2 colonne").accesskit_node().toggled(), Some(Toggled::True));
        assert_eq!(h.get_by_label("2 righe e 3 colonne").accesskit_node().toggled(), Some(Toggled::False));
        assert_eq!(riempiti(&h, tema::BLU_TENUE).len(), 4);
        h.get_by_label("3 righe e 4 colonne").click();
        h.run_steps(2);
        assert_eq!(*h.state(), (3, 4));
        assert_eq!(riempiti(&h, tema::BLU_TENUE).len(), 12);
        // Il dito che passa sopra la griglia sceglie dove arriva.
        let da = h.get_by_label("1 riga e 1 colonna").rect().center();
        let a = h.get_by_label("5 righe e 2 colonne").rect().center();
        h.event(egui::Event::PointerMoved(da));
        h.event(egui::Event::PointerButton { pos: da, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
        h.step();
        for k in 1..=8 {
            h.event(egui::Event::PointerMoved(da + (a - da) * (k as f32 / 8.0)));
            h.step();
        }
        h.event(egui::Event::PointerButton { pos: a, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
        h.run_steps(2);
        assert_eq!(*h.state(), (5, 2));
    }

    #[test]
    fn la_tavolozza_ha_le_tinte_in_fila_e_la_scelta_cerchiata() {
        let mut h = banco(
            |ui, scelto: &mut Option<usize>| {
                ui.add_space(30.0);
                if let Some(i) = Tavolozza::new(&COLORI).acceso(*scelto).mostra(ui) {
                    *scelto = Some(i);
                }
            },
            Some(1),
        );
        // Una fila larga quanto lo spazio, i bersagli alti 48 e larghi uguali.
        let posti: Vec<egui::Rect> = COLORI.iter().map(|(_, e)| h.get_by_label(e).rect()).collect();
        for p in &posti {
            assert_eq!(p.height(), tema::PULSANTE, "{p:?}");
            assert!((p.width() - SCHERMO.x / 3.0).abs() < 0.01, "{p:?}");
        }
        assert_eq!(h.get_by_label("Rosso scuro").accesskit_node().toggled(), Some(Toggled::True));
        assert_eq!(h.get_by_label("Automatico").accesskit_node().toggled(), Some(Toggled::False));
        let cerchi: Vec<egui::epaint::CircleShape> = forme(&h)
            .into_iter()
            .filter_map(|s| match s {
                egui::Shape::Circle(c) => Some(c),
                _ => None,
            })
            .collect();
        // Il colore pieno al centro del suo posto, l'anello blu attorno a quello scelto.
        let rosso = cerchi.iter().find(|c| c.fill == Color32::from_rgb(0xC0, 0, 0)).expect("il cerchio rosso");
        assert_eq!(rosso.center, posti[1].center());
        assert!(cerchi.iter().any(|c| c.stroke.color == tema::BLU && c.center == posti[1].center() && c.radius > rosso.radius));
        // Automatico: la A in inchiostro; Nessuno: la riga rossa che lo barra.
        assert!(rettangolo_del_testo(&h, "A").is_some_and(|r| posti[0].contains_rect(r)));
        assert!(forme(&h).iter().any(|s| matches!(s, egui::Shape::LineSegment { stroke, .. } if stroke.color == tema::ROSSO)));
        h.get_by_label("Nessuno").click();
        h.run_steps(2);
        assert_eq!(*h.state(), Some(2));
        assert!(cerchi_blu(&h, posti[2].center()));
    }

    /// Nove tinte su un telefono da 372 dp (lo spazio di un foglio, 336) non stanno in una fila
    /// da 46 dp l'una: vanno su due file, 5 e 4, coi bersagli larghi e i cerchi che non si toccano.
    #[test]
    fn la_tavolozza_stretta_va_su_due_file() {
        const NOVE: [(Tinta, &str); 9] = [
            (Tinta::Automatico, "Automatico"),
            (Tinta::Colore(Color32::from_rgb(0xC0, 0, 0)), "Rosso"),
            (Tinta::Colore(Color32::from_rgb(0xE0, 0x80, 0)), "Arancio"),
            (Tinta::Colore(Color32::from_rgb(0xC0, 0xA0, 0)), "Giallo"),
            (Tinta::Colore(Color32::from_rgb(0, 0x80, 0)), "Verde"),
            (Tinta::Colore(Color32::from_rgb(0, 0x60, 0xC0)), "Azzurro"),
            (Tinta::Colore(Color32::from_rgb(0, 0, 0x90)), "Blu"),
            (Tinta::Colore(Color32::from_rgb(0x70, 0, 0x90)), "Viola"),
            (Tinta::Colore(Color32::from_rgb(0x60, 0x60, 0x60)), "Grigio"),
        ];
        let h = banco(
            |ui, scelto: &mut Option<usize>| {
                ui.set_max_width(336.0);
                if let Some(i) = Tavolozza::new(&NOVE).acceso(*scelto).mostra(ui) {
                    *scelto = Some(i);
                }
            },
            Some(1),
        );
        let posti: Vec<egui::Rect> = NOVE.iter().map(|(_, e)| h.get_by_label(e).rect()).collect();
        let cime: std::collections::BTreeSet<i32> = posti.iter().map(|p| p.top() as i32).collect();
        assert_eq!(cime.len(), 2, "{posti:?}");
        assert!(posti.iter().all(|p| p.width() >= TINTA_MINIMA && p.height() == tema::PULSANTE), "{posti:?}");
        assert_eq!(posti.iter().filter(|p| p.top() == posti[0].top()).count(), 5);
        // Un banco largo ne tiene nove in una fila sola.
        let largo = banco(|ui, _: &mut ()| _ = Tavolozza::new(&NOVE).mostra(ui), ());
        assert!(
            SCHERMO.x / 9.0 < TINTA_MINIMA
                || NOVE.iter().all(|(_, e)| largo.get_by_label(e).rect().top() == largo.get_by_label("Automatico").rect().top())
        );
    }

    /// C'è l'anello blu attorno a questo centro.
    fn cerchi_blu(h: &egui_kittest::Harness<'_, Option<usize>>, centro: egui::Pos2) -> bool {
        forme(h).iter().any(|s| matches!(s, egui::Shape::Circle(c) if c.stroke.color == tema::BLU && c.center == centro))
    }

    #[test]
    fn l_etichetta_di_sezione_e_maiuscola_grigia_e_in_grassetto() {
        let h = banco(
            |ui, sotto: &mut Option<f32>| {
                ui.label("Sopra");
                etichetta_sezione(ui, "Paragrafo");
                *sotto = Some(ui.label("Sotto").rect.top());
            },
            None,
        );
        assert_eq!(colore_del_testo(&h, "PARAGRAFO"), Some(tema::MATITA));
        assert_eq!(famiglia_del_testo(&h, "PARAGRAFO"), Some(Carattere::Grassetto.famiglia()));
        h.get_by_label("PARAGRAFO");
        let sopra = rettangolo_del_testo(&h, "Sopra").unwrap();
        let etichetta = rettangolo_del_testo(&h, "PARAGRAFO").unwrap();
        // 16 dp sopra (come il margine del CSS), il testo spostato di 2 dp dal bordo.
        assert!(etichetta.top() - sopra.bottom() > 14.0, "{sopra:?} {etichetta:?}");
        assert!((etichetta.left() - sopra.left() - 2.0).abs() < 1.5, "{sopra:?} {etichetta:?}");
        assert!(h.state().unwrap() > etichetta.bottom());
    }
}
