//! I fogli di calcolo in sola lettura (0.9.0, l'autore il 3/11: «inserisci fogli di calcolo per ora
//! solo in visualizzazione»). Si apre su un thread suo con lo stack grande (i lettori di GridCraft
//! vanno in profondità); la griglia la disegna egui dai dati di `tascabile_foglio::vista`: le
//! intestazioni delle colonne (A, B, C) e delle righe (1, 2, 3), le righe e le colonne bloccate
//! del file ferme, il resto che scorre col dito e si ingrandisce col pizzico; sotto le schede dei
//! fogli. Un tocco su una cella mostra il suo contenuto intero e la formula.
//!
//! Nella diagnosi solo misure e conti: mai un testo, un nome di foglio, una cella.

use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Instant;

use eframe::egui::{self, Align2, Color32, FontFamily, FontId, Id, Pos2, Rect, Sense, Stroke, UiBuilder, Vec2, WidgetInfo, WidgetType, pos2, vec2};
use tascabile_files::{Recente, Tipo};
use tascabile_foglio::{Allinea, Assi, CARLITO, Cartella, Casella, CellRef, Dettaglio, Genere, Sheet};
use tascabile_platform::diagnosi;
use tascabile_touch::tema::{self, Carattere};
use tascabile_touch::{BarraAlta, Chip, PulsanteIcona, icone};

use crate::{Da, Errore, Schermata, TascabileApp, adesso, home};

pub(crate) const INDIETRO: &str = "Indietro";
pub(crate) const CHIUDI_CELLA: &str = "Chiudi la cella";
pub(crate) const APRO: &str = "Apro il foglio di calcolo";
/// La griglia, per l'accessibilità e le prove.
pub(crate) const GRIGLIA: &str = "Griglia del foglio";

/// Lo zoom: dp dello schermo per pixel del foglio (a 96 dpi), all'apertura e ai limiti.
const ZOOM: f32 = 1.0;
const ZOOM_MINIMO: f32 = 0.35;
const ZOOM_MASSIMO: f32 = 4.0;
/// Le intestazioni delle righe (larghezza) e delle colonne (altezza), dp.
const LARGO_RIGHE: f32 = 40.0;
const ALTO_COLONNE: f32 = 24.0;
/// La fila delle schede dei fogli in fondo, dp.
const ALTO_SCHEDE: f32 = 56.0;
/// Lo stack del thread di apertura: i lettori e le formule vanno in profondità.
const STACK: usize = 16 << 20;
const RIGA_GRIGLIA: Color32 = Color32::from_rgb(0xE1, 0xDF, 0xDA);
const FONDO_INTESTAZIONI: Color32 = Color32::from_rgb(0xF2, 0xF0, 0xEB);

/// Un foglio di calcolo che si apre sul suo thread.
pub(crate) struct AperturaFoglio {
    esito: Receiver<Result<Cartella, Genere>>,
    da: Da,
    misura: usize,
    inizio: Instant,
    nome: String,
    chiave: Option<String>,
}

/// Il foglio di calcolo aperto: la cartella, la scheda che si vede, le sue misure, dove si guarda
/// (la parte che scorre, in pixel del foglio dall'inizio delle righe e colonne non bloccate) e lo
/// zoom, la cella toccata.
pub(crate) struct FoglioAperto {
    pub(crate) cartella: Cartella,
    pub(crate) nome: String,
    pub(crate) chiave: Option<String>,
    scheda: usize,
    assi: Assi,
    scorre: Vec2,
    zoom: f32,
    dettaglio: Option<Dettaglio>,
}

impl FoglioAperto {
    fn nuovo(cartella: Cartella, nome: String, chiave: Option<String>) -> FoglioAperto {
        let scheda = cartella.iniziale;
        let assi = Assi::di(Self::foglio_di(&cartella, scheda));
        FoglioAperto { cartella, nome, chiave, scheda, assi, scorre: Vec2::ZERO, zoom: ZOOM, dettaglio: None }
    }

    fn foglio_di(c: &Cartella, scheda: usize) -> &Sheet {
        c.schede.get(scheda).and_then(|i| c.cartella.sheet(*i)).or_else(|| c.cartella.sheet(0)).expect("la cartella ha almeno una scheda")
    }

    fn foglio(&self) -> &Sheet {
        Self::foglio_di(&self.cartella, self.scheda)
    }

    fn cambia_scheda(&mut self, i: usize) {
        if i < self.cartella.schede.len() && i != self.scheda {
            self.scheda = i;
            self.assi = Assi::di(self.foglio());
            self.scorre = Vec2::ZERO;
            self.dettaglio = None;
        }
    }
}

/// Le righe e le colonne bloccate del foglio, in pixel del foglio: fin dove arrivano.
fn bloccate(sh: &Sheet, assi: &Assi) -> (u32, u32, f32, f32) {
    let (r, c) = sh.freeze.unwrap_or((0, 0));
    (r, c, assi.righe.start(r) as f32, assi.colonne.start(c) as f32)
}

impl TascabileApp {
    /// Apre `byte` come foglio di calcolo sul thread «foglio-apertura»; arriva in
    /// [`passo_apertura_foglio`](Self::passo_apertura_foglio).
    pub(crate) fn apri_foglio(&mut self, da: Da, byte: Vec<u8>, nome: &str, chiave: Option<String>, inizio: Instant) {
        let misura = byte.len();
        let (manda, esito) = std::sync::mpsc::channel();
        let risveglio = self.risveglio.clone();
        let lavoro = move || {
            let esito = Cartella::apri(&byte);
            drop(byte);
            let _ = manda.send(esito);
            risveglio();
        };
        if let Err(e) = std::thread::Builder::new().name("foglio-apertura".into()).stack_size(STACK).spawn(lavoro) {
            da.scrivi(format!("{}: {misura} byte, thread di apertura del foglio non partito ({:?})", da.etichetta(), e.kind()));
            self.errore = Some(Errore::apertura("Non riesco ad aprire il file: il telefono non ha lasciato partire la lettura."));
            return;
        }
        self.piattaforma.hide_keyboard();
        self.fuoco_al_documento();
        if let Some(e) = self.editor_mut() {
            e.ponte.chiudi();
        }
        self.apertura_foglio = Some(AperturaFoglio { esito, da, misura, inizio, nome: nome.to_string(), chiave });
    }

    /// A ogni passo: il foglio che si apre, se è pronto, va sullo schermo (gli altri documenti si
    /// chiudono); se non si apre, lo dice.
    pub(crate) fn passo_apertura_foglio(&mut self) {
        let Some(a) = &self.apertura_foglio else { return };
        let esito = match a.esito.try_recv() {
            Ok(esito) => esito,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err(Genere::Illeggibile),
        };
        let Some(a) = self.apertura_foglio.take() else { return };
        let etichetta = a.da.etichetta();
        match esito {
            Ok(cartella) => {
                a.da.scrivi(format!(
                    "{etichetta}: {} byte, foglio di calcolo, {} fogli da mostrare su {}, {} avvisi del lettore, aperto in {:.0} ms",
                    a.misura,
                    cartella.schede.len(),
                    cartella.cartella.sheets.len(),
                    cartella.avvisi,
                    a.inizio.elapsed().as_secs_f64() * 1000.0
                ));
                if cartella.matrici {
                    self.avviso_breve.avvisa("Alcune formule a matrice mostrano solo la prima cella: qui i fogli non si ricalcolano.");
                }
                let uscito = self.sostituisci(None);
                self.metti_da_parte(uscito);
                self.chiudi_pdf();
                self.chiudi_slide();
                let schede = cartella.schede.len();
                self.foglio = Some(FoglioAperto::nuovo(cartella, a.nome.clone(), a.chiave.clone()));
                self.vai(Schermata::Foglio);
                match a.chiave {
                    Some(chiave) => self.home.ricorda(Recente { chiave, nome: a.nome, tipo: Tipo::Foglio, aperto_alle: adesso(), pagine: Some(schede) }),
                    None => diagnosi("recenti: percorso che non è UTF-8, non ricordato"),
                }
            }
            Err(g) => {
                a.da.scrivi(format!("{etichetta}: {} byte, foglio di calcolo, errore di apertura ({})", a.misura, g.diagnosi()));
                self.errore = Some(Errore::apertura(format!("Non riesco ad aprire il file: {g}")));
            }
        }
    }

    /// Chiude il foglio di calcolo, se c'è.
    pub(crate) fn chiudi_foglio(&mut self) {
        if self.foglio.take().is_some() {
            diagnosi("foglio di calcolo chiuso");
        }
    }

    /// Indietro dal foglio: prima la cella toccata, poi la Home.
    pub(crate) fn esci_dal_foglio(&mut self) {
        if let Some(f) = self.foglio.as_mut()
            && f.dettaglio.take().is_some()
        {
            return;
        }
        self.chiudi_poi(crate::Dopo::Home);
    }

    /// Mentre il foglio si apre, sopra tutto la pillola «Apro il foglio di calcolo».
    pub(crate) fn mostra_apertura_foglio(&self, ctx: &egui::Context) {
        if self.apertura_foglio.is_none() {
            return;
        }
        let zona = ctx.content_rect();
        egui::Area::new(Id::new("apertura_foglio")).order(egui::Order::Foreground).fixed_pos(zona.min).show(ctx, |ui| {
            ui.allocate_rect(Rect::from_min_size(zona.min, zona.size()), Sense::click_and_drag());
            let g = ui.painter().layout_no_wrap(APRO.to_string(), Carattere::Grassetto.font(ui.ctx(), 15.0), Color32::WHITE);
            let pillola = Rect::from_center_size(zona.center(), vec2(g.size().x + 32.0, 44.0));
            ui.painter().rect_filled(pillola, 22.0, tema::INDICATORE);
            ui.painter().galley(pillola.center() - g.size() / 2.0, g, Color32::PLACEHOLDER);
            ui.interact(pillola, Id::new("apertura_foglio_testo"), Sense::hover()).widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, APRO));
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        });
    }

    /// La schermata del foglio di calcolo: la barra alta, la griglia con le intestazioni, le schede
    /// in fondo; sopra, la cella toccata.
    pub(crate) fn mostra_foglio(&mut self, ui: &mut egui::Ui) {
        let Some(f) = self.foglio.as_mut() else {
            self.vai(Schermata::Home);
            return;
        };
        let zona = ui.max_rect();
        let barra = Rect::from_min_size(zona.min, vec2(zona.width(), tema::BARRA_ALTA));
        let nome = home::nome_da_mostrare(&f.nome, Tipo::Foglio).to_string();
        let sotto = match f.cartella.schede.len() {
            1 => "Foglio di calcolo · sola lettura".to_string(),
            n => format!("Foglio di calcolo · {n} fogli · sola lettura"),
        };
        let r = ui.scope_builder(UiBuilder::new().max_rect(barra), |ui| BarraAlta::new(&nome).indietro(INDIETRO).sottotitolo(&sotto).mostra(ui)).inner;
        let schede = f.cartella.schede.len() > 1;
        let fondo = if schede { zona.bottom() - ALTO_SCHEDE } else { zona.bottom() };
        let griglia = Rect::from_min_max(pos2(zona.left(), barra.bottom()), pos2(zona.right(), fondo));
        disegna_griglia(ui, f, griglia);
        let mut scheda = None;
        if schede {
            let fila = Rect::from_min_max(pos2(zona.left(), fondo), zona.max);
            ui.painter().rect_filled(fila, 0.0, tema::CARTA);
            ui.painter().line_segment([fila.left_top(), fila.right_top()], Stroke::new(1.0, tema::FILO));
            ui.scope_builder(UiBuilder::new().max_rect(fila.shrink2(vec2(8.0, 8.0))), |ui| {
                egui::ScrollArea::horizontal().id_salt("schede_del_foglio").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(ui, |ui| {
                    ui.horizontal_centered(|ui| {
                        for i in 0..f.cartella.schede.len() {
                            if ui.add(Chip::new(f.cartella.nome_scheda(i)).acceso(i == f.scheda)).clicked() {
                                scheda = Some(i);
                            }
                        }
                    });
                });
            });
        }
        if let Some(i) = scheda {
            diagnosi(format!("foglio di calcolo: scheda {} di {}", i + 1, f.cartella.schede.len()));
            f.cambia_scheda(i);
        }
        // La cella toccata, in un pannello in fondo: dove, il testo intero, la formula.
        let mut chiudi = false;
        if let Some(d) = &f.dettaglio {
            let largo = zona.width().min(480.0) - 24.0;
            let testo = ui.painter().layout(d.testo.clone(), Carattere::Testo.font(ui.ctx(), 16.0), tema::INCHIOSTRO, largo - 32.0);
            let formula = d.formula.as_ref().map(|t| ui.painter().layout(t.clone(), FontId::monospace(14.0), tema::MATITA, largo - 32.0));
            let alto = 16.0 + 24.0 + 8.0 + testo.size().y + formula.as_ref().map_or(0.0, |g| g.size().y + 8.0) + 16.0;
            let pannello = Rect::from_min_size(pos2(zona.center().x - largo / 2.0, fondo - alto - 12.0), vec2(largo, alto));
            tema::dipingi_ombra(ui.painter(), pannello, egui::CornerRadius::same(16), &tema::OMBRA_FLOTTANTE);
            ui.painter().rect_filled(pannello, 16.0, tema::FOGLIO);
            let _ = ui.interact(pannello, Id::new("cella_del_foglio"), Sense::click());
            ui.painter().text(pannello.left_top() + vec2(16.0, 16.0), Align2::LEFT_TOP, &d.dove, Carattere::Grassetto.font(ui.ctx(), 15.0), tema::BLU);
            let mut y = pannello.top() + 16.0 + 24.0 + 8.0;
            let alto_testo = testo.size().y;
            ui.painter().galley(pos2(pannello.left() + 16.0, y), testo, tema::INCHIOSTRO);
            ui.interact(Rect::from_min_size(pos2(pannello.left() + 16.0, y), vec2(largo - 32.0, alto_testo.max(16.0))), Id::new("cella_testo"), Sense::hover())
                .widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, &d.testo));
            y += alto_testo + 8.0;
            if let Some(g) = formula {
                ui.painter().galley(pos2(pannello.left() + 16.0, y), g, tema::MATITA);
            }
            let x = Rect::from_min_size(pos2(pannello.right() - 8.0 - tema::PULSANTE, pannello.top() + 4.0), Vec2::splat(tema::PULSANTE));
            chiudi = ui.put(x, PulsanteIcona::new(icone::CLOSE, CHIUDI_CELLA)).clicked();
        }
        if chiudi {
            f.dettaglio = None;
        }
        if r.indietro {
            self.esci_dal_foglio();
        }
    }

    /// Per le prove: il foglio di calcolo aperto, la scheda che si vede e lo zoom.
    #[doc(hidden)]
    pub fn foglio_aperto(&self) -> Option<(usize, usize, f32)> {
        self.foglio.as_ref().map(|f| (f.cartella.schede.len(), f.scheda, f.zoom))
    }

    /// Per le prove: la cella toccata, se il pannello è aperto.
    #[doc(hidden)]
    pub fn cella_del_foglio(&self) -> Option<Dettaglio> {
        self.foglio.as_ref()?.dettaglio.clone()
    }

    /// Per le prove: un foglio di calcolo si sta aprendo.
    #[doc(hidden)]
    pub fn foglio_in_apertura(&self) -> bool {
        self.apertura_foglio.is_some()
    }
}

/// La griglia in `dove`: le intestazioni, i quattro pezzi (righe e colonne bloccate ferme, il
/// resto che scorre), il dito che scorre, il pizzico, il tocco su una cella.
fn disegna_griglia(ui: &mut egui::Ui, f: &mut FoglioAperto, dove: Rect) {
    let celle = Rect::from_min_max(dove.min + vec2(LARGO_RIGHE, ALTO_COLONNE), dove.max);
    let risposta = ui.interact(celle, Id::new("griglia_del_foglio"), Sense::click_and_drag());
    risposta.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, GRIGLIA));
    // Il pizzico ingrandisce attorno alle dita; il dito scorre.
    let (zoom_delta, centro) = ui.input(|i| (i.zoom_delta(), i.multi_touch().map(|m| m.center_pos).or(i.pointer.hover_pos())));
    let (_, _, fy, fx) = bloccate(f.foglio(), &f.assi);
    if (zoom_delta - 1.0).abs() > 1e-3 {
        let nuovo = (f.zoom * zoom_delta).clamp(ZOOM_MINIMO, ZOOM_MASSIMO);
        if let Some(c) = centro.filter(|c| celle.contains(*c)) {
            // Il punto del foglio sotto le dita resta lì.
            let dentro = c - celle.min - vec2(fx, fy) * f.zoom;
            let punto = f.scorre + dentro / f.zoom;
            f.scorre = punto - dentro / nuovo;
        }
        f.zoom = nuovo;
    }
    if risposta.dragged() && ui.input(|i| i.multi_touch().is_none()) {
        f.scorre -= risposta.drag_delta() / f.zoom;
    }
    // Fin dove si scorre: un poco oltre l'ultima cella usata.
    let sh = f.foglio();
    let usato = sh.used_range().map_or((20, 10), |r| (r.end.row + 20, r.end.col + 5));
    let massimo = vec2(f.assi.colonne.start(usato.1) as f32 - fx, f.assi.righe.start(usato.0) as f32 - fy);
    let vista = celle.size() / f.zoom - vec2(fx, fy);
    f.scorre = vec2(f.scorre.x.clamp(0.0, (massimo.x - vista.x).max(0.0)), f.scorre.y.clamp(0.0, (massimo.y - vista.y).max(0.0)));
    if risposta.clicked()
        && let Some(p) = risposta.interact_pointer_pos()
    {
        let x = if p.x - celle.left() < fx * f.zoom { (p.x - celle.left()) / f.zoom } else { (p.x - celle.left()) / f.zoom + f.scorre.x };
        let y = if p.y - celle.top() < fy * f.zoom { (p.y - celle.top()) / f.zoom } else { (p.y - celle.top()) / f.zoom + f.scorre.y };
        let c = f.assi.cella_a(f64::from(x), f64::from(y));
        let d = tascabile_foglio::dettaglio(&f.cartella.cartella, f.foglio(), c);
        diagnosi(format!("foglio di calcolo: cella toccata, formula {}", if d.formula.is_some() { "sì" } else { "no" }));
        f.dettaglio = Some(d);
    }
    let painter = ui.painter_at(dove);
    painter.rect_filled(dove, 0.0, tema::FOGLIO);
    let f: &FoglioAperto = f;
    let sh = f.foglio();
    let (fr, fc, fy, fx) = bloccate(sh, &f.assi);
    let z = f.zoom;
    // I quattro pezzi: (righe, colonne) bloccate o che scorrono.
    let bloccate_y = (fy * z).min(celle.height() * 0.6);
    let bloccate_x = (fx * z).min(celle.width() * 0.6);
    let pezzi = [(false, false), (false, true), (true, false), (true, true)];
    for (scorre_y, scorre_x) in pezzi {
        let clip = Rect::from_min_max(
            pos2(if scorre_x { celle.left() + bloccate_x } else { celle.left() }, if scorre_y { celle.top() + bloccate_y } else { celle.top() }),
            pos2(if scorre_x { celle.right() } else { celle.left() + bloccate_x }, if scorre_y { celle.bottom() } else { celle.top() + bloccate_y }),
        );
        if clip.width() <= 0.0 || clip.height() <= 0.0 {
            continue;
        }
        // Dal foglio allo schermo in questo pezzo.
        let ox = if scorre_x { celle.left() + bloccate_x - (fx + f.scorre.x) * z } else { celle.left() };
        let oy = if scorre_y { celle.top() + bloccate_y - (fy + f.scorre.y) * z } else { celle.top() };
        let a_schermo = |x: f64, y: f64| pos2(ox + x as f32 * z, oy + y as f32 * z);
        let (x0, x1) = if scorre_x { (fx + f.scorre.x, fx + f.scorre.x + clip.width() / z) } else { (0.0, fx) };
        let (y0, y1) = if scorre_y { (fy + f.scorre.y, fy + f.scorre.y + clip.height() / z) } else { (0.0, fy) };
        let righe = (f.assi.righe.at(f64::from(y0)), f.assi.righe.at(f64::from(y1)));
        let colonne = (f.assi.colonne.at(f64::from(x0)), f.assi.colonne.at(f64::from(x1)));
        if (!scorre_y && fr == 0) || (!scorre_x && fc == 0) {
            continue;
        }
        let p = ui.painter_at(clip);
        // La griglia leggera.
        if sh.show_gridlines {
            for r in righe.0..=righe.1 + 1 {
                let y = a_schermo(0.0, f.assi.righe.start(r)).y;
                p.line_segment([pos2(clip.left(), y), pos2(clip.right(), y)], Stroke::new(1.0, RIGA_GRIGLIA));
            }
            for c in colonne.0..=colonne.1 + 1 {
                let x = a_schermo(f.assi.colonne.start(c), 0.0).x;
                p.line_segment([pos2(x, clip.top()), pos2(x, clip.bottom())], Stroke::new(1.0, RIGA_GRIGLIA));
            }
        }
        let caselle = tascabile_foglio::vista::caselle(&f.cartella.cartella, sh, &f.assi, righe, colonne);
        for k in &caselle {
            disegna_casella(ui, &p, clip, k, &a_schermo, z);
        }
    }
    // Le linee delle righe e colonne bloccate, un poco più scure.
    if bloccate_y > 0.0 {
        painter.line_segment([pos2(celle.left(), celle.top() + bloccate_y), pos2(celle.right(), celle.top() + bloccate_y)], Stroke::new(1.5, tema::MATITA));
    }
    if bloccate_x > 0.0 {
        painter.line_segment([pos2(celle.left() + bloccate_x, celle.top()), pos2(celle.left() + bloccate_x, celle.bottom())], Stroke::new(1.5, tema::MATITA));
    }
    // Le intestazioni: le lettere delle colonne in alto, i numeri delle righe a sinistra.
    let alto = Rect::from_min_max(pos2(celle.left(), dove.top()), pos2(celle.right(), celle.top()));
    let sinistra = Rect::from_min_max(pos2(dove.left(), celle.top()), pos2(celle.left(), celle.bottom()));
    painter.rect_filled(alto, 0.0, FONDO_INTESTAZIONI);
    painter.rect_filled(sinistra, 0.0, FONDO_INTESTAZIONI);
    painter.rect_filled(Rect::from_min_max(dove.min, celle.min), 0.0, FONDO_INTESTAZIONI);
    let font = FontId::proportional(12.0);
    let pa = ui.painter_at(alto);
    let ps = ui.painter_at(sinistra);
    let colonna_x = |c: u32| {
        let x = f.assi.colonne.start(c) as f32;
        if x < fx { celle.left() + x * z } else { celle.left() + bloccate_x + (x - fx - f.scorre.x) * z }
    };
    let riga_y = |r: u32| {
        let y = f.assi.righe.start(r) as f32;
        if y < fy { celle.top() + y * z } else { celle.top() + bloccate_y + (y - fy - f.scorre.y) * z }
    };
    let prima_colonna = f.assi.colonne.at(f64::from(fx + f.scorre.x));
    let prima_riga = f.assi.righe.at(f64::from(fy + f.scorre.y));
    for c in (0..fc).chain(prima_colonna..) {
        let (a, b) = (colonna_x(c), colonna_x(c + 1));
        if a > alto.right() {
            break;
        }
        if b - a < 1.0 {
            continue;
        }
        pa.line_segment([pos2(b, alto.top()), pos2(b, alto.bottom())], Stroke::new(1.0, RIGA_GRIGLIA));
        if b - a > 14.0 {
            pa.text(pos2((a + b) / 2.0, alto.center().y), Align2::CENTER_CENTER, tascabile_foglio::nome_colonna(c), font.clone(), tema::MATITA);
        }
    }
    for r in (0..fr).chain(prima_riga..) {
        let (a, b) = (riga_y(r), riga_y(r + 1));
        if a > sinistra.bottom() {
            break;
        }
        if b - a < 1.0 {
            continue;
        }
        ps.line_segment([pos2(sinistra.left(), b), pos2(sinistra.right(), b)], Stroke::new(1.0, RIGA_GRIGLIA));
        if b - a > 10.0 {
            ps.text(pos2(sinistra.center().x, (a + b) / 2.0), Align2::CENTER_CENTER, (r + 1).to_string(), font.clone(), tema::MATITA);
        }
    }
    // La cella toccata, col bordo blu.
    if let Some(d) = &f.dettaglio
        && let Some(c) = cella_da_nome(&d.dove)
    {
        let area = sh.merge_at(c).unwrap_or(tascabile_foglio::RangeRef::cell(c));
        let rett = Rect::from_min_max(pos2(colonna_x(area.start.col), riga_y(area.start.row)), pos2(colonna_x(area.end.col + 1), riga_y(area.end.row + 1)));
        ui.painter_at(celle).rect_stroke(rett, 0.0, Stroke::new(2.0, tema::BLU), egui::StrokeKind::Inside);
    }
}

/// La cella «B7» come riga e colonna.
fn cella_da_nome(a1: &str) -> Option<CellRef> {
    let lettere: String = a1.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    let numero: u32 = a1[lettere.len()..].parse().ok()?;
    let colonna = lettere.bytes().try_fold(0u32, |a, b| Some(a * 26 + u32::from(b.to_ascii_uppercase().checked_sub(b'A')?) + 1))?;
    Some(CellRef::new(numero.checked_sub(1)?, colonna.checked_sub(1)?))
}

/// Il carattere di una cella: Carlito (le misure di Calibri) se c'è, se no il proporzionale.
fn carattere(ctx: &egui::Context, k: &Casella, misura: f32) -> FontId {
    let nome = CARLITO[usize::from(k.grassetto) + 2 * usize::from(k.corsivo)];
    let famiglia = FontFamily::Name(nome.into());
    if ctx.fonts(|f| f.definitions().families.contains_key(&famiglia)) {
        FontId::new(misura, famiglia)
    } else if k.grassetto {
        Carattere::Grassetto.font(ctx, misura)
    } else {
        FontId::proportional(misura)
    }
}

/// Una cella sullo schermo: lo sfondo, il testo (che sborda nelle vuote accanto, va a capo se la
/// cella lo chiede, `#` per un numero che non ci sta), i bordi.
fn disegna_casella(ui: &egui::Ui, p: &egui::Painter, clip: Rect, k: &Casella, a_schermo: &impl Fn(f64, f64) -> Pos2, z: f32) {
    let rett = Rect::from_min_max(a_schermo(k.rett[0], k.rett[1]), a_schermo(k.rett[2], k.rett[3]));
    if !rett.intersects(clip) && !(k.spazio[0] < k.rett[0] || k.spazio[1] > k.rett[2]) {
        return;
    }
    if let Some(s) = k.sfondo {
        p.rect_filled(rett, 0.0, Color32::from_rgb(s[0], s[1], s[2]));
    }
    // Punti del file in pixel a 96 dpi, poi in dp.
    let misura = k.misura * 96.0 / 72.0 * z;
    if !k.testo.is_empty() && misura >= 3.0 {
        let colore = Color32::from_rgb(k.colore[0], k.colore[1], k.colore[2]);
        let font = carattere(ui.ctx(), k, misura);
        let margine = 2.0 * z;
        let rientro = f32::from(k.rientro) * 9.0 * z;
        let largo = (rett.width() - 2.0 * margine - rientro).max(1.0);
        let mut g = if k.a_capo { p.layout(k.testo.clone(), font.clone(), colore, largo) } else { p.layout_no_wrap(k.testo.clone(), font.clone(), colore) };
        // Un numero che non sta nella colonna: i cancelletti, come Excel.
        let spazio = Rect::from_min_max(pos2(a_schermo(k.spazio[0], 0.0).x, rett.top()), pos2(a_schermo(k.spazio[1], 0.0).x, rett.bottom()));
        if k.orizzontale == Allinea::Fine && k.spazio == [k.rett[0], k.rett[2]] && g.size().x > largo && k.testo.chars().any(|c| c.is_ascii_digit()) {
            let cancelletto = p.layout_no_wrap("#".into(), font.clone(), colore).size().x.max(1.0);
            g = p.layout_no_wrap("#".repeat((largo / cancelletto).floor().max(1.0) as usize), font, colore);
        }
        let x = match k.orizzontale {
            Allinea::Inizio => rett.left() + margine + rientro,
            Allinea::Mezzo => rett.center().x - g.size().x / 2.0,
            Allinea::Fine => rett.right() - margine - g.size().x,
        };
        let y = match k.verticale {
            Allinea::Inizio => rett.top() + 1.0,
            Allinea::Mezzo => rett.center().y - g.size().y / 2.0,
            Allinea::Fine => rett.bottom() - g.size().y - 1.0,
        };
        let dentro = if k.a_capo { rett } else { spazio };
        let pt = ui.painter_at(clip.intersect(dentro));
        let larghezza = g.size().x;
        let base = pos2(x, y);
        let alto = g.size().y;
        pt.galley(base, g, colore);
        if k.sottolineato {
            pt.line_segment([pos2(x, y + alto - 1.0), pos2(x + larghezza, y + alto - 1.0)], Stroke::new(1.0, colore));
        }
        if k.barrato {
            pt.line_segment([pos2(x, y + alto / 2.0), pos2(x + larghezza, y + alto / 2.0)], Stroke::new(1.0, colore));
        }
    }
    let linee = [(rett.left_top(), rett.left_bottom()), (rett.left_top(), rett.right_top()), (rett.right_top(), rett.right_bottom()), (rett.left_bottom(), rett.right_bottom())];
    for (b, (a, c)) in k.bordi.iter().zip(linee) {
        if let Some(b) = b {
            p.line_segment([a, c], Stroke::new((b.spessore * z).max(1.0), Color32::from_rgb(b.colore[0], b.colore[1], b.colore[2])));
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn il_nome_di_una_cella() {
        assert_eq!(cella_da_nome("B7"), Some(CellRef::new(6, 1)));
        assert_eq!(cella_da_nome("AB12"), Some(CellRef::new(11, 27)));
        assert_eq!(cella_da_nome("7"), None);
    }
}
