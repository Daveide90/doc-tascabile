//! Il lettore delle Slide: le slide una sotto l'altra sul tavolo, larghe quanto lo schermo, come
//! le pagine del lettore PDF (stessa impaginazione, `tascabile_pdf::geometria`), col pizzico, il
//! doppio tocco e l'indicatore «2 / 12». Le slide le disegna il thread di disegno alla larghezza
//! che hanno sullo schermo; le nascoste si vedono velate, con l'occhio barrato.

use std::collections::HashMap;
use std::io;
use std::time::Duration;

use egui::scroll_area::ScrollSource;
use egui::{Color32, Id, Pos2, Rect, Sense, Ui, Vec2, WidgetInfo, WidgetType, pos2, vec2};
use tascabile_pdf::Misura;
use tascabile_pdf::geometria::{CIMA, Impaginazione};
use tascabile_touch::icone;
use tascabile_touch::tema::{self, Carattere};

use crate::apertura::Presentazione;
use crate::disegno::{Disegnatore, Richiesta, Uso, larghezza_entro};

/// Le slide sopra e sotto quelle sullo schermo che si disegnano prima, per lo scorrimento.
const VICINE: usize = 1;
/// Il lato più lungo di una slide disegnata, in pixel, e al più quello delle texture della scheda
/// grafica (`max_texture_side`): oltre, l'immagine si stira. Le slide si leggono anche così, e una
/// texture più grande non sta in tutti i telefoni.
const LATO_MASSIMO: f32 = 4096.0;
/// Il tetto della memoria delle slide disegnate: oltre, si liberano le più lontane da quelle
/// sullo schermo.
const TETTO: usize = 96 << 20;
/// Le slide disegnate prese dal thread in un frame al massimo: caricare una texture costa.
const PER_FRAME: usize = 2;
/// Dopo l'ultimo cambio di zoom senza dita (la rotella sul PC), tanto prima delle slide nitide;
/// col dito vale il momento in cui si alzano le dita.
const CALMA_ZOOM: f64 = 0.25;
const ATTESA: Duration = Duration::from_millis(16);
/// L'indicatore della slide resta tanto dopo l'ultimo movimento, poi sfuma.
const DURATA_INDICATORE: f64 = 1.2;
const SFUMATURA_INDICATORE: f32 = 0.3;
/// Il tondo scuro dell'occhio barrato sopra una slide nascosta, e l'icona.
const RAGGIO_NASCOSTA: f32 = 26.0;
const ICONA_NASCOSTA: f32 = 26.0;
/// Le miniature: una colonna ogni tanti dp di larghezza, da due (le slide sono larghe: in
/// verticale) a sei; lo spazio fra due celle, l'altezza del numero sotto, il margine attorno
/// perché l'ombra e il bordo blu non finiscano tagliati, il tetto della loro memoria.
const LARGHEZZA_COLONNA: f32 = 200.0;
const COLONNE_MINIATURE: std::ops::RangeInclusive<usize> = 2..=6;
const SPAZIO_MINIATURE: f32 = 12.0;
const NUMERO_MINIATURA: f32 = 30.0;
const ATTORNO_MINIATURE: Vec2 = vec2(4.0, 6.0);
const TETTO_MINIATURE: usize = 24 << 20;
/// Una riga di miniature non è più alta di questa parte dello schermo (con più colonne).
const RIGA_MASSIMA: f32 = 0.4;

/// Quello che è successo nel lettore in un frame.
#[derive(Debug, Default)]
pub struct EsitoLettura {
    /// Un tocco su una slide: quale.
    pub tocco: Option<usize>,
}

/// Una slide disegnata e caricata.
struct Disegno {
    larghezza: u32,
    texture: egui::TextureHandle,
    byte: usize,
}

/// Il lettore di una presentazione, col suo thread di disegno.
pub struct LettoreSlide {
    id: Id,
    disegnatore: Disegnatore,
    misure: Vec<Misura>,
    nascoste: Vec<bool>,
    /// L'impaginazione dell'ultimo frame, rifatta quando cambiano zoom o larghezza.
    imp: Option<Impaginazione>,
    /// Lo zoom che si vede, e quello a cui sono disegnate le slide.
    zoom: f32,
    zoom_nitido: f32,
    /// Quando è cambiato lo zoom l'ultima volta (`InputState::time`).
    ultimo_zoom: f64,
    /// Due dita sullo schermo: il pizzico è in corso, e lo scorrimento di egui è fermo.
    dita: bool,
    centro_dita: Option<Pos2>,
    scorrimento: Vec2,
    /// Lo scorrimento da dare all'area al frame dopo (pizzico, doppio tocco, salto).
    forza: Option<Vec2>,
    /// La slide da portare in cima al frame dopo.
    salto: Option<usize>,
    /// La slide scelta (con un salto o un tocco) e lo scorrimento di quando è stata scelta (None
    /// finché il salto non è fatto): finché la vista non si muove è lei quella che si legge, anche
    /// se la geometria ne direbbe un'altra. Con poche slide la vista si ferma in fondo, dove si
    /// leggerebbe l'ultima, e le note sarebbero quelle sbagliate.
    scelta: Option<(usize, Option<Vec2>)>,
    vista: Vec2,
    /// Lo spazio in più dopo l'ultima slide, per la barra flottante.
    sotto: f32,
    disegni: HashMap<usize, Disegno>,
    /// Le miniature disegnate, e quelle che la griglia voleva nell'ultimo frame.
    miniature: HashMap<usize, Disegno>,
    miniature_volute: Vec<Richiesta>,
    /// Le slide chieste al thread e non ancora arrivate.
    chieste: Vec<Richiesta>,
    /// Le slide che il lettore vuole disegnate adesso: quelle sullo schermo e le vicine che
    /// stanno nel tetto.
    volute: Vec<usize>,
    /// Quando si è mossa la vista l'ultima volta, per l'indicatore; all'apertura si mostra.
    ultimo_movimento: Option<f64>,
    disegnate: usize,
    ms_totali: u64,
    ms_massimo: u32,
}

impl LettoreSlide {
    /// Il lettore della presentazione `p`, col suo thread di disegno, che chiama `sveglia` a ogni
    /// slide pronta (chi chiama chiede un frame a egui).
    pub fn new(p: &Presentazione, id: Id, sveglia: impl Fn() + Send + 'static) -> io::Result<Self> {
        let doc = p.documento().clone();
        let misura = Misura { larghezza: doc.slide_size.width as f32, altezza: doc.slide_size.height as f32 };
        let misure = vec![misura; doc.slides.len()];
        let nascoste = doc.slides.iter().map(|s| s.hidden).collect();
        let disegnatore = Disegnatore::avvia(doc, sveglia)?;
        Ok(Self {
            id,
            disegnatore,
            misure,
            nascoste,
            imp: None,
            zoom: 1.0,
            zoom_nitido: 1.0,
            ultimo_zoom: f64::NEG_INFINITY,
            dita: false,
            centro_dita: None,
            scorrimento: Vec2::ZERO,
            forza: None,
            salto: None,
            scelta: None,
            vista: Vec2::ZERO,
            sotto: 0.0,
            disegni: HashMap::new(),
            miniature: HashMap::new(),
            miniature_volute: Vec::new(),
            chieste: Vec::new(),
            volute: Vec::new(),
            ultimo_movimento: None,
            disegnate: 0,
            ms_totali: 0,
            ms_massimo: 0,
        })
    }

    /// La slide che si sta leggendo: quella scelta, finché la vista non si muove; se no quella che
    /// occupa più spazio nella vista.
    pub fn corrente(&self) -> usize {
        if let Some((i, _)) = self.scelta {
            return i;
        }
        self.imp.as_ref().map_or(0, |imp| imp.corrente(Rect::from_min_size(self.scorrimento.to_pos2(), self.vista)))
    }

    /// Porta la slide `i` in cima alla vista (o più su che si può), al frame dopo, e la sceglie.
    pub fn vai_alla_slide(&mut self, i: usize) {
        if i < self.misure.len() {
            self.salto = Some(i);
            self.scelta = Some((i, None));
        }
    }

    /// Lo spazio in fondo, in dp, sotto l'ultima slide: per la barra flottante sopra il lettore.
    pub fn spazio_sotto(&mut self, dp: f32) {
        self.sotto = dp;
    }

    /// Mancano slide nitide: chieste al thread e non arrivate, o uno zoom nuovo da disegnare.
    pub fn in_attesa(&self) -> bool {
        self.imp.is_none() || !self.chieste.is_empty() || self.zoom != self.zoom_nitido
    }

    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    /// Le larghezze in pixel delle slide disegnate, in ordine di slide.
    pub fn larghezze(&self) -> Vec<u32> {
        let mut v: Vec<(usize, u32)> = self.disegni.iter().map(|(i, d)| (*i, d.larghezza)).collect();
        v.sort_unstable();
        v.into_iter().map(|(_, l)| l).collect()
    }

    /// Quante miniature sono disegnate.
    pub fn miniature_pronte(&self) -> usize {
        self.miniature.len()
    }

    /// Le larghezze in pixel delle miniature disegnate, in ordine di slide.
    pub fn larghezze_miniature(&self) -> Vec<u32> {
        let mut v: Vec<(usize, u32)> = self.miniature.iter().map(|(i, d)| (*i, d.larghezza)).collect();
        v.sort_unstable();
        v.into_iter().map(|(_, l)| l).collect()
    }

    /// Quante slide ha disegnato il thread finora.
    pub fn disegnate(&self) -> usize {
        self.disegnate
    }

    /// Una riga di misure per la diagnosi: quante slide disegnate, quanto tempo.
    pub fn riga_misure(&self) -> String {
        let media = if self.disegnate > 0 { self.ms_totali as f64 / self.disegnate as f64 } else { 0.0 };
        format!("{} slide disegnate, in media {media:.0} ms, al massimo {} ms", self.disegnate, self.ms_massimo)
    }

    fn impaginazione(&mut self, larghezza: f32) -> Impaginazione {
        match &self.imp {
            Some(imp) if imp.zoom() == self.zoom && imp.larghezza_vista() == larghezza && imp.spazio_sotto() == self.sotto => imp.clone(),
            _ => {
                let imp = Impaginazione::new(self.misure.clone(), larghezza, self.zoom).con_spazio_sotto(self.sotto);
                self.imp = Some(imp.clone());
                imp
            }
        }
    }

    /// Disegna il lettore in tutto lo spazio di `ui`.
    pub fn mostra(&mut self, ui: &mut Ui) -> EsitoLettura {
        let mut esito = EsitoLettura::default();
        let rect = ui.max_rect();
        let ppp = ui.ctx().pixels_per_point();
        let adesso = ui.input(|i| i.time);
        if self.ultimo_movimento.is_none() {
            self.ultimo_movimento = Some(adesso);
        }
        ui.painter().rect_filled(rect, 0.0, tema::TAVOLO);
        let vista = rect.size();
        // Il telefono girato: la slide che si leggeva resta in cima.
        if let Some(imp) = &self.imp
            && imp.larghezza_vista() != vista.x
            && self.salto.is_none()
        {
            self.salto = Some(self.corrente());
        }
        self.vista = vista;
        let imp = self.impaginazione(vista.x);
        self.pizzico(ui, rect, &imp, adesso);
        let imp = self.impaginazione(vista.x);
        if let Some(i) = self.salto.take() {
            self.forza = Some(imp.tieni(vec2(self.scorrimento.x, imp.rettangolo(i).top() - CIMA), vista));
            self.ultimo_movimento = Some(adesso);
            // Il telefono girato porta qui la slide scelta: resta scelta col suo posto nuovo.
            if self.scelta.is_some_and(|(j, _)| j == i) {
                self.scelta = Some((i, None));
            }
        }
        // Le slide nitide dello zoom nuovo, a dita alzate e dopo un attimo di calma.
        let calmo = !self.dita && adesso - self.ultimo_zoom >= CALMA_ZOOM;
        if !calmo {
            let resta = (CALMA_ZOOM - (adesso - self.ultimo_zoom)).max(0.0);
            ui.ctx().request_repaint_after(Duration::from_secs_f64(resta).max(ATTESA));
        }
        if calmo {
            self.zoom_nitido = self.zoom;
        }
        // Le slide nitide fino al bordo, come le pagine del PDF: niente velo sui lati che scorrono.
        ui.spacing_mut().scroll.fade.strength = 0.0;
        let sorgente = if self.dita { ScrollSource::NONE } else { ScrollSource::ALL };
        let mut area = egui::ScrollArea::both().id_salt(self.id).scroll_source(sorgente).auto_shrink([false, false]);
        if let Some(f) = self.forza.take() {
            area = area.scroll_offset(imp.tieni(f, vista));
        }
        let mut doppio = None;
        let mut tocco = None;
        area.show_viewport(ui, |ui, viewport| {
            let (contenuto, risposta) = ui.allocate_exact_size(imp.contenuto().max(viewport.size()), Sense::click());
            let scorrimento = viewport.min.to_vec2();
            if scorrimento != self.scorrimento {
                self.ultimo_movimento = Some(adesso);
            }
            self.scorrimento = scorrimento;
            // La slide scelta prende il posto del salto; quando la vista si muove non vale più.
            match self.scelta {
                Some((i, None)) => self.scelta = Some((i, Some(scorrimento))),
                Some((_, Some(dove))) if (dove - scorrimento).length() > 0.5 => self.scelta = None,
                _ => {}
            }
            self.ricevi(ui.ctx());
            if calmo {
                let lato_massimo = (ui.input(|i| i.max_texture_side) as f32).min(LATO_MASSIMO);
                self.chiedi(&imp, viewport, ppp, lato_massimo);
            }
            self.dipingi(ui, &imp, contenuto.min, viewport);
            self.libera(&imp, viewport);
            // In un pizzico niente tocchi: il primo dito resta spesso fermo.
            if let Some(p) = (!self.dita).then(|| risposta.interact_pointer_pos()).flatten() {
                if risposta.double_clicked() {
                    doppio = Some(p);
                } else if risposta.clicked() {
                    tocco = Some((p - contenuto.min).to_pos2());
                }
            }
        });
        if let Some(t) = tocco {
            esito.tocco = imp.punto(t).map(|(i, _)| i).filter(|i| imp.rettangolo(*i).contains(t));
            // La slide toccata è quella che si legge.
            if let Some(i) = esito.tocco {
                self.scelta = Some((i, Some(self.scorrimento)));
            }
        }
        if let Some(p) = doppio {
            let a = p - rect.min;
            let (nuova, s) = imp.zoom_attorno(imp.zoom_doppio_tocco(), a, a, self.scorrimento, vista);
            self.zoom = nuova.zoom();
            self.forza = Some(s);
            // Niente pizzico da aspettare: le slide nitide subito.
            self.ultimo_zoom = adesso - CALMA_ZOOM;
            self.ultimo_movimento = Some(adesso);
            ui.ctx().request_repaint();
        }
        self.fine_pizzico(ui);
        self.indicatore(ui, rect, adesso);
        if self.in_attesa() {
            ui.ctx().request_repaint_after(ATTESA);
        }
        esito
    }

    /// Il pizzico: lo zoom attorno al centro delle dita, che possono anche spostarsi.
    fn pizzico(&mut self, ui: &Ui, rect: Rect, imp: &Impaginazione, adesso: f64) {
        let (fattore, dita, sopra) = ui.input(|i| (i.zoom_delta(), i.multi_touch(), i.pointer.hover_pos()));
        if dita.is_some() {
            self.dita = true;
        }
        let centro_prima = self.centro_dita;
        self.centro_dita = dita.map(|d| d.center_pos);
        let centro = self.centro_dita.or(centro_prima).or(sopra).unwrap_or(rect.center());
        let da = centro_prima.unwrap_or(centro);
        let spostate = dita.is_some() && da != centro;
        if fattore == 1.0 && !spostate {
            return;
        }
        let (nuova, s) = imp.zoom_attorno(self.zoom * fattore, da - rect.min, centro - rect.min, self.scorrimento, rect.size());
        if nuova.zoom() != self.zoom {
            self.ultimo_zoom = adesso;
        }
        self.zoom = nuova.zoom();
        self.forza = Some(s);
        self.ultimo_movimento = Some(adesso);
    }

    /// A fine frame: con tutte le dita alzate il pizzico è finito.
    fn fine_pizzico(&mut self, ui: &Ui) {
        if self.dita && !ui.input(|i| i.any_touches() || i.pointer.any_down()) {
            self.dita = false;
            self.centro_dita = None;
            ui.ctx().request_repaint();
        }
    }

    /// La larghezza in pixel a cui va disegnata la slide `i` per lo zoom di adesso, col lato più
    /// lungo al più `lato_massimo` pixel.
    fn larghezza_voluta(imp: &Impaginazione, i: usize, ppp: f32, lato_massimo: f32) -> u32 {
        let m = imp.misura(i);
        let voluta = (m.larghezza * imp.dp_per_pt(i) * ppp).round().max(1.0) as u32;
        larghezza_entro(f64::from(m.larghezza), f64::from(m.altezza), voluta, lato_massimo as u32)
    }

    /// Chiede al thread le slide sullo schermo (e le vicine) che non sono già disegnate alla
    /// larghezza giusta, prima quelle sullo schermo; solo quando l'elenco cambia. Le vicine solo
    /// finché stanno nel tetto insieme a quelle sullo schermo: allo zoom più grande una slide è
    /// 38 MB, e la vicina, liberata appena arrivata, si chiedeva di nuovo senza fine.
    fn chiedi(&mut self, imp: &Impaginazione, viewport: Rect, ppp: f32, lato_massimo: f32) {
        let visibili = imp.visibili(viewport);
        let attorno = visibili.start.saturating_sub(VICINE)..(visibili.end + VICINE).min(self.misure.len());
        let richiesta =
            |i: usize| Richiesta { indice: i, larghezza: Self::larghezza_voluta(imp, i, ppp, lato_massimo), uso: Uso::Vista, passo: None };
        let byte = |r: &Richiesta| {
            let m = imp.misura(r.indice);
            let altezza = (f64::from(m.altezza.max(1.0)) * f64::from(r.larghezza) / f64::from(m.larghezza.max(1.0))).round() as usize;
            r.larghezza as usize * altezza.max(1) * 4
        };
        let mut ordine: Vec<Richiesta> = visibili.clone().map(richiesta).collect();
        let mut usati: usize = ordine.iter().map(byte).sum();
        for r in attorno.filter(|i| !visibili.contains(i)).map(richiesta) {
            usati += byte(&r);
            if usati > TETTO {
                break;
            }
            ordine.push(r);
        }
        self.volute = ordine.iter().map(|r| r.indice).collect();
        let miniature = std::mem::take(&mut self.miniature_volute);
        let voluti: Vec<Richiesta> = ordine
            .into_iter()
            .chain(miniature)
            .filter(|r| {
                let disegni = if r.uso == Uso::Miniatura { &self.miniature } else { &self.disegni };
                disegni.get(&r.indice).is_none_or(|d| d.larghezza != r.larghezza)
            })
            .collect();
        if voluti != self.chieste {
            self.disegnatore.chiedi(voluti.iter().copied());
            self.chieste = voluti;
        }
    }

    /// Le slide disegnate dal thread, al massimo `PER_FRAME`, in texture.
    fn ricevi(&mut self, ctx: &egui::Context) {
        for _ in 0..PER_FRAME {
            let Some(d) = self.disegnatore.prendi() else { return };
            self.disegnate += 1;
            self.ms_totali += u64::from(d.ms);
            self.ms_massimo = self.ms_massimo.max(d.ms);
            self.chieste.retain(|c| *c != d.richiesta);
            let img = d.immagine;
            let colori = egui::ColorImage::from_rgba_premultiplied([img.larghezza as usize, img.altezza as usize], &img.pixel);
            let texture = ctx.load_texture(format!("slide {}", d.richiesta.indice), colori, egui::TextureOptions::LINEAR);
            let byte = img.pixel.len();
            let disegni = if d.richiesta.uso == Uso::Miniatura { &mut self.miniature } else { &mut self.disegni };
            disegni.insert(d.richiesta.indice, Disegno { larghezza: d.richiesta.larghezza, texture, byte });
        }
        ctx.request_repaint();
    }

    /// Le slide sullo schermo: l'ombra, il foglio bianco, l'immagine (anche di un altro zoom,
    /// stirata, finché arriva quella nitida); sopra le nascoste il velo e l'occhio barrato.
    fn dipingi(&self, ui: &Ui, imp: &Impaginazione, origine: Pos2, viewport: Rect) {
        let painter = ui.painter();
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        for i in imp.visibili(viewport) {
            let r = imp.rettangolo(i).translate(origine.to_vec2());
            tema::dipingi_ombra(painter, r, 0, &tema::OMBRA_PAGINA);
            painter.rect_filled(r, 0.0, tema::FOGLIO);
            if let Some(d) = self.disegni.get(&i) {
                painter.image(d.texture.id(), r, uv, Color32::WHITE);
            }
            if self.nascoste[i] {
                painter.rect_filled(r, 0.0, tema::VELO.gamma_multiply(0.75));
                painter.circle_filled(r.center(), RAGGIO_NASCOSTA, tema::INDICATORE);
                let g = painter.layout_no_wrap(icone::VISIBILITY_OFF.to_string(), Carattere::Icona.font(ui.ctx(), ICONA_NASCOSTA), Color32::WHITE);
                painter.galley(r.center() - g.size() / 2.0, g, Color32::PLACEHOLDER);
            }
        }
    }

    /// Oltre il tetto della memoria, via le slide disegnate che il lettore non vuole, prima le più
    /// lontane da quelle sullo schermo; le vicine volute per ultime.
    fn libera(&mut self, imp: &Impaginazione, viewport: Rect) {
        let mut byte: usize = self.disegni.values().map(|d| d.byte).sum();
        if byte <= TETTO {
            return;
        }
        let visibili = imp.visibili(viewport);
        let distanza = |i: usize| if i < visibili.start { visibili.start - i } else { i.saturating_sub(visibili.end.saturating_sub(1)) };
        let mut lontane: Vec<usize> = self.disegni.keys().copied().filter(|i| !visibili.contains(i)).collect();
        lontane.sort_by_key(|i| (self.volute.contains(i), std::cmp::Reverse(distanza(*i))));
        for i in lontane {
            if byte <= TETTO {
                break;
            }
            if let Some(d) = self.disegni.remove(&i) {
                byte -= d.byte;
            }
        }
    }

    /// La griglia delle miniature, larga quanto `ui`, da mettere in un'area che scorre: due
    /// colonne (di più se è larga o se lo schermo è basso), il numero sotto ogni slide, la slide
    /// che si legge col bordo blu, le nascoste velate. Le miniature si chiedono al thread dopo le
    /// slide del lettore (al frame dopo). Restituisce la slide toccata.
    pub fn mostra_miniature(&mut self, ui: &mut Ui) -> Option<usize> {
        let n = self.misure.len();
        if n == 0 {
            return None;
        }
        let ppp = ui.ctx().pixels_per_point();
        let largo = ui.available_width();
        let interno = largo - 2.0 * ATTORNO_MINIATURE.x;
        let m = self.misure[0];
        let proporzione = m.altezza.max(1.0) / m.larghezza.max(1.0);
        let cella_di = |colonne: usize| ((interno - SPAZIO_MINIATURE * (colonne - 1) as f32) / colonne as f32).max(1.0);
        let riga_di = |colonne: usize| cella_di(colonne) * proporzione + NUMERO_MINIATURA + SPAZIO_MINIATURE;
        let mut colonne = ((interno / LARGHEZZA_COLONNA) as usize).clamp(*COLONNE_MINIATURE.start(), *COLONNE_MINIATURE.end());
        // Una riga al più due quinti dello schermo: nel foglio, che ne prende tre quarti, della
        // riga dopo si vede sempre un pezzo e si capisce che ci sono altre slide (0.8.3: col
        // telefono girato la quarta restava tutta sotto il bordo).
        let massima = ui.ctx().content_rect().height() * RIGA_MASSIMA;
        while colonne < *COLONNE_MINIATURE.end() && riga_di(colonne) > massima {
            colonne += 1;
        }
        let cella_w = cella_di(colonne);
        let cella_h = cella_w * proporzione;
        let riga = riga_di(colonne);
        let righe = n.div_ceil(colonne);
        let (fuori, _) = ui.allocate_exact_size(vec2(largo, righe as f32 * riga + 2.0 * ATTORNO_MINIATURE.y), Sense::hover());
        let tutto = fuori.shrink2(ATTORNO_MINIATURE);
        let clip = ui.clip_rect();
        let prima = (((clip.top() - tutto.top()) / riga).floor().max(0.0) as usize).min(righe);
        let ultima = ((((clip.bottom() - tutto.top()) / riga).ceil().max(0.0)) as usize).min(righe);
        let larghezza = (cella_w * ppp).round().max(1.0) as u32;
        let corrente = self.corrente();
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        let mut toccata = None;
        let mut volute = Vec::new();
        // Le righe a vista, più una sopra e una sotto da chiedere prima che arrivino.
        for r in prima.saturating_sub(1)..(ultima + 1).min(righe) {
            for c in 0..colonne {
                let i = r * colonne + c;
                if i >= n {
                    break;
                }
                if self.miniature.get(&i).is_none_or(|d| d.larghezza != larghezza) {
                    volute.push(Richiesta { indice: i, larghezza, uso: Uso::Miniatura, passo: None });
                }
                if !(prima..ultima).contains(&r) {
                    continue;
                }
                let cella = Rect::from_min_size(
                    pos2(tutto.left() + c as f32 * (cella_w + SPAZIO_MINIATURE), tutto.top() + r as f32 * riga),
                    vec2(cella_w, cella_h),
                );
                let numero = Rect::from_min_size(cella.left_bottom(), vec2(cella_w, NUMERO_MINIATURA));
                let risposta = ui.interact(cella.union(numero), self.id.with(("miniatura", i)), Sense::click());
                let etichetta = format!("Slide {}", i + 1);
                risposta.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, &etichetta));
                let painter = ui.painter();
                tema::dipingi_ombra(painter, cella, 0, &tema::OMBRA_PAGINA);
                painter.rect_filled(cella, 0.0, tema::FOGLIO);
                if let Some(d) = self.miniature.get(&i).or_else(|| self.disegni.get(&i)) {
                    painter.image(d.texture.id(), cella, uv, Color32::WHITE);
                }
                if self.nascoste[i] {
                    painter.rect_filled(cella, 0.0, tema::VELO.gamma_multiply(0.75));
                    let g = painter.layout_no_wrap(icone::VISIBILITY_OFF.to_string(), Carattere::Icona.font(ui.ctx(), 20.0), tema::MATITA);
                    painter.galley(cella.center() - g.size() / 2.0, g, Color32::PLACEHOLDER);
                }
                if risposta.is_pointer_button_down_on() {
                    painter.rect_filled(cella, 0.0, tema::SELEZIONE);
                }
                if i == corrente {
                    painter.rect_stroke(cella.expand(1.0), 2.0, egui::Stroke::new(2.5, tema::BLU), egui::StrokeKind::Outside);
                }
                let (font, colore) = if i == corrente { (Carattere::Grassetto, tema::BLU) } else { (Carattere::Testo, tema::MATITA) };
                let g = painter.layout_no_wrap((i + 1).to_string(), font.font(ui.ctx(), 13.0), colore);
                painter.galley(numero.center() - g.size() / 2.0, g, Color32::PLACEHOLDER);
                if risposta.clicked() {
                    toccata = Some(i);
                }
            }
        }
        // Oltre il tetto, via le miniature che la griglia non vuole adesso.
        let mut byte: usize = self.miniature.values().map(|d| d.byte).sum();
        if byte > TETTO_MINIATURE {
            let attorno = prima.saturating_sub(1) * colonne..((ultima + 1) * colonne).min(n);
            let lontane: Vec<usize> = self.miniature.keys().copied().filter(|i| !attorno.contains(i)).collect();
            for i in lontane {
                if byte <= TETTO_MINIATURE {
                    break;
                }
                if let Some(d) = self.miniature.remove(&i) {
                    byte -= d.byte;
                }
            }
        }
        if !volute.is_empty() {
            ui.ctx().request_repaint_after(ATTESA);
        }
        self.miniature_volute = volute;
        toccata
    }

    /// Per la griglia mostrata senza il lettore (durante la presentazione): prende le miniature
    /// disegnate e chiede quelle che la griglia vuole. Col lettore sullo schermo lo fa lui.
    pub fn aggiorna_miniature(&mut self, ctx: &egui::Context) {
        self.ricevi(ctx);
        let volute: Vec<Richiesta> = std::mem::take(&mut self.miniature_volute)
            .into_iter()
            .filter(|r| self.miniature.get(&r.indice).is_none_or(|d| d.larghezza != r.larghezza))
            .collect();
        if volute != self.chieste {
            self.disegnatore.chiedi(volute.iter().copied());
            self.chieste = volute;
        }
        if !self.chieste.is_empty() {
            ctx.request_repaint_after(ATTESA);
        }
    }

    /// «2 / 12» in una pillola scura, per un attimo dopo ogni movimento.
    fn indicatore(&self, ui: &Ui, rect: Rect, adesso: f64) {
        let n = self.misure.len();
        if n == 0 {
            return;
        }
        let passato = adesso - self.ultimo_movimento.unwrap_or(adesso);
        let visibile = passato < DURATA_INDICATORE;
        if visibile {
            ui.ctx().request_repaint_after(Duration::from_secs_f64(DURATA_INDICATORE - passato));
        }
        let alfa = ui.ctx().animate_bool_with_time(self.id.with("indicatore"), visibile, SFUMATURA_INDICATORE);
        if alfa <= 0.0 {
            return;
        }
        let testo = format!("{} / {n}", self.corrente() + 1);
        let font = Carattere::Grassetto.font(ui.ctx(), 13.0);
        let g = ui.painter().layout_no_wrap(testo.clone(), font, Color32::WHITE.gamma_multiply(alfa));
        let misura = vec2(g.size().x + 24.0, (g.size().y + 10.0).max(28.0));
        let centro = pos2(rect.center().x, rect.top() + rect.height() * 2.0 / 3.0);
        let pillola = Rect::from_center_size(centro, misura);
        ui.painter().rect_filled(pillola, misura.y / 2.0, tema::INDICATORE.gamma_multiply(alfa));
        ui.painter().galley(pillola.center() - g.size() / 2.0, g, Color32::PLACEHOLDER);
        ui.interact(pillola, self.id.with("slide"), Sense::hover()).widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, &testo));
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::apertura::Presentazione;
    use egui::{Pos2, Rect, pos2, vec2};
    use std::time::{Duration, Instant};

    /// Un telefono finto: 412 × 851 dp (lo schermo sotto la barra alta) a 2,625 pixel per dp.
    struct Telefono {
        ctx: egui::Context,
        t: f64,
        /// Il lato più lungo di una texture che la scheda grafica accetta.
        lato_texture: usize,
        /// Lo schermo, in dp.
        schermo: egui::Vec2,
    }

    struct Frame {
        esito: EsitoLettura,
        forme: Vec<egui::epaint::ClippedShape>,
    }

    impl Frame {
        fn testi(&self) -> Vec<String> {
            self.forme
                .iter()
                .filter_map(|f| match &f.shape {
                    egui::Shape::Text(t) => Some(t.galley.text().to_string()),
                    _ => None,
                })
                .collect()
        }

        /// Dove sono disegnate le immagini delle slide.
        fn immagini(&self) -> Vec<Rect> {
            self.forme
                .iter()
                .filter_map(|f| match &f.shape {
                    egui::Shape::Mesh(m) if m.texture_id != egui::TextureId::default() => Some(m.calc_bounds()),
                    _ => None,
                })
                .collect()
        }
    }

    impl Telefono {
        fn new() -> Self {
            Self { ctx: egui::Context::default(), t: 0.0, lato_texture: 4096, schermo: vec2(412.0, 851.0) }
        }

        fn lettore(&self) -> LettoreSlide {
            self.lettore_di(crate::prova::LEZIONE)
        }

        fn lettore_di(&self, byte: &[u8]) -> LettoreSlide {
            let p = Presentazione::apri(byte).unwrap();
            let ctx = self.ctx.clone();
            LettoreSlide::new(&p, egui::Id::new("lettore di prova"), move || ctx.request_repaint()).unwrap()
        }

        fn passo(&mut self, l: &mut LettoreSlide, eventi: Vec<egui::Event>) -> Frame {
            let mut ri = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.schermo)),
                max_texture_side: Some(self.lato_texture),
                time: Some(self.t),
                predicted_dt: 1.0 / 60.0,
                events: eventi,
                ..Default::default()
            };
            ri.viewports.entry(egui::ViewportId::ROOT).or_default().native_pixels_per_point = Some(2.625);
            self.t += 1.0 / 60.0;
            let mut esito = EsitoLettura::default();
            let mut out = self.ctx.run_ui(ri, |ui| esito = l.mostra(ui));
            // Un delta buttato senza svuotarlo manda epaint in panico (debug_assert nel Drop).
            out.textures_delta.clear();
            Frame { esito, forme: std::mem::take(&mut out.shapes) }
        }

        /// Un frame col lettore e, sopra, la griglia delle miniature in un'area che scorre larga
        /// 376 dp dal punto (18, 300), alta 500: restituisce la slide toccata nella griglia.
        fn passo_con_griglia(&mut self, l: &mut LettoreSlide, eventi: Vec<egui::Event>) -> (Option<usize>, Frame) {
            let mut ri = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.schermo)),
                max_texture_side: Some(self.lato_texture),
                time: Some(self.t),
                predicted_dt: 1.0 / 60.0,
                events: eventi,
                ..Default::default()
            };
            ri.viewports.entry(egui::ViewportId::ROOT).or_default().native_pixels_per_point = Some(2.625);
            self.t += 1.0 / 60.0;
            let mut scelta = None;
            let mut esito = EsitoLettura::default();
            let mut out = self.ctx.run_ui(ri, |ui| {
                esito = l.mostra(ui);
                egui::Area::new(egui::Id::new("griglia")).fixed_pos(pos2(18.0, 300.0)).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
                    egui::ScrollArea::vertical().max_height(500.0).show(ui, |ui| {
                        ui.set_width(376.0);
                        scelta = l.mostra_miniature(ui);
                    });
                });
            });
            out.textures_delta.clear();
            (scelta, Frame { esito, forme: std::mem::take(&mut out.shapes) })
        }

        /// Un frame con la sola griglia delle miniature, larga 376 dp, senza il lettore.
        fn solo_griglia(&mut self, l: &mut LettoreSlide) {
            self.solo_griglia_larga(l, 376.0);
        }

        /// Un frame con la sola griglia delle miniature, larga `largo` dp, senza il lettore.
        fn solo_griglia_larga(&mut self, l: &mut LettoreSlide, largo: f32) {
            let mut ri = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.schermo)),
                max_texture_side: Some(self.lato_texture),
                time: Some(self.t),
                predicted_dt: 1.0 / 60.0,
                ..Default::default()
            };
            ri.viewports.entry(egui::ViewportId::ROOT).or_default().native_pixels_per_point = Some(2.625);
            self.t += 1.0 / 60.0;
            let mut out = self.ctx.run_ui(ri, |ui| {
                egui::ScrollArea::vertical().max_height(500.0).show(ui, |ui| {
                    ui.set_width(largo);
                    l.mostra_miniature(ui);
                });
                l.aggiorna_miniature(ui.ctx());
            });
            out.textures_delta.clear();
        }

        /// Frame finché il lettore non aspetta più slide dal thread, al massimo 20 s; l'ultimo.
        fn aspetta(&mut self, l: &mut LettoreSlide) -> Frame {
            let fine = Instant::now() + Duration::from_secs(20);
            while l.in_attesa() {
                assert!(Instant::now() < fine, "il lettore non ha finito in 20 s");
                self.passo(l, vec![]);
                std::thread::sleep(Duration::from_millis(5));
            }
            self.passo(l, vec![])
        }
    }

    fn tasto(pressed: bool, pos: Pos2) -> egui::Event {
        egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE }
    }

    /// Un tocco breve in `pos`.
    fn tocca(tel: &mut Telefono, l: &mut LettoreSlide, pos: Pos2) -> Frame {
        tel.passo(l, vec![egui::Event::PointerMoved(pos), tasto(true, pos)]);
        tel.passo(l, vec![tasto(false, pos)])
    }

    #[test]
    fn le_slide_si_vedono_larghe_quanto_lo_schermo() {
        let mut tel = Telefono::new();
        let mut l = tel.lettore();
        let f = tel.aspetta(&mut l);
        let immagini = f.immagini();
        // Larghe 412 - 2 × 16 dp, alte in proporzione (16:9); la prima sotto la cima.
        let prima = immagini.iter().find(|r| (r.top() - 14.0).abs() < 0.5).expect("la prima slide non c'è");
        assert!((prima.width() - 380.0).abs() < 0.5 && (prima.height() - 213.75).abs() < 0.5, "{prima:?}");
        // Nell'altezza del telefono se ne vedono quattro, l'ultima in parte.
        assert_eq!(immagini.len(), 4, "{immagini:?}");
        assert_eq!(l.corrente(), 0);
    }

    #[test]
    fn le_slide_sono_disegnate_ai_pixel_dello_schermo_e_non_si_ridisegnano() {
        let mut tel = Telefono::new();
        let mut l = tel.lettore();
        tel.aspetta(&mut l);
        // 380 dp a 2,625 pixel per dp.
        assert_eq!(l.larghezze(), vec![998; 4]);
        let disegnate = l.disegnate();
        assert_eq!(disegnate, 4);
        for _ in 0..10 {
            tel.passo(&mut l, vec![]);
        }
        assert_eq!(l.disegnate(), disegnate, "una slide uguale si è ridisegnata");
    }

    #[test]
    fn l_indicatore_dice_la_slide_che_si_legge() {
        let mut tel = Telefono::new();
        let mut l = tel.lettore();
        let f = tel.aspetta(&mut l);
        assert!(f.testi().contains(&"1 / 4".to_string()), "{:?}", f.testi());
        l.vai_alla_slide(3);
        tel.passo(&mut l, vec![]);
        let f = tel.aspetta(&mut l);
        assert_eq!(l.corrente(), 3);
        assert!(f.testi().contains(&"4 / 4".to_string()), "{:?}", f.testi());
    }

    #[test]
    fn la_slide_nascosta_e_velata_con_l_occhio_barrato() {
        let mut tel = Telefono::new();
        let mut l = tel.lettore();
        let f = tel.aspetta(&mut l);
        let occhi = f.testi().iter().filter(|t| t.as_str() == tascabile_touch::icone::VISIBILITY_OFF).count();
        assert_eq!(occhi, 1, "{:?}", f.testi());
    }

    #[test]
    fn un_tocco_dice_quale_slide() {
        let mut tel = Telefono::new();
        let mut l = tel.lettore();
        tel.aspetta(&mut l);
        // La seconda slide: da 14 + 213,75 + 20 a 461,5 dp.
        let f = tocca(&mut tel, &mut l, pos2(200.0, 350.0));
        assert_eq!(f.esito.tocco, Some(1));
        // La slide toccata diventa quella che si legge, anche se in cima c'è la prima.
        assert_eq!(l.corrente(), 1);
    }

    #[test]
    fn dopo_un_salto_si_legge_la_slide_scelta_finche_non_si_scorre() {
        let mut tel = Telefono::new();
        let mut l = tel.lettore();
        tel.aspetta(&mut l);
        // Con quattro slide la seconda non arriva in cima: la vista si ferma in fondo, dove
        // per la geometria si leggerebbe l'ultima.
        l.vai_alla_slide(1);
        tel.passo(&mut l, vec![]);
        let f = tel.aspetta(&mut l);
        assert_eq!(l.corrente(), 1);
        assert!(f.testi().contains(&"2 / 4".to_string()), "{:?}", f.testi());
        // Si scorre fino in cima: si legge di nuovo quello che dice la geometria.
        let su = egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, 2000.0),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        };
        tel.passo(&mut l, vec![egui::Event::PointerMoved(pos2(200.0, 400.0)), su]);
        tel.aspetta(&mut l);
        assert_eq!(l.corrente(), 0);
    }

    #[test]
    fn le_miniature_si_disegnano_piccole_e_un_tocco_dice_quale_slide() {
        let mut tel = Telefono::new();
        let mut l = tel.lettore();
        tel.aspetta(&mut l);
        let fine = Instant::now() + Duration::from_secs(20);
        while l.miniature_pronte() < 4 {
            assert!(Instant::now() < fine, "le miniature non sono arrivate in 20 s");
            tel.passo_con_griglia(&mut l, vec![]);
            std::thread::sleep(Duration::from_millis(5));
        }
        // Due colonne in 376 dp, meno 4 dp per lato e 12 in mezzo: 178 dp, a 2,625 pixel per dp.
        assert_eq!(l.larghezze_miniature(), vec![467; 4]);
        let (_, f) = tel.passo_con_griglia(&mut l, vec![]);
        for numero in 1..=4 {
            assert!(f.testi().contains(&numero.to_string()), "manca il numero {numero}: {:?}", f.testi());
        }
        // La terza slide: prima colonna, seconda riga (righe da 100,125 dp di miniatura, 30 di
        // numero e 12 di spazio, sotto i 6 dp attorno alla griglia).
        let dove = pos2(18.0 + 4.0 + 89.0, 300.0 + 6.0 + 142.125 + 50.0);
        tel.passo_con_griglia(&mut l, vec![egui::Event::PointerMoved(dove), tasto(true, dove)]);
        let (scelta, _) = tel.passo_con_griglia(&mut l, vec![tasto(false, dove)]);
        assert_eq!(scelta, Some(2));
    }

    #[test]
    fn le_slide_non_superano_il_lato_massimo_delle_texture() {
        // Una scheda grafica che accetta texture fino a 2048 pixel per lato: col doppio tocco la
        // slide sarebbe larga 2494 pixel.
        let mut tel = Telefono { lato_texture: 2048, ..Telefono::new() };
        let mut l = tel.lettore();
        tel.aspetta(&mut l);
        let p = pos2(200.0, 120.0);
        tocca(&mut tel, &mut l, p);
        tocca(&mut tel, &mut l, p);
        tel.passo(&mut l, vec![]);
        tel.aspetta(&mut l);
        assert!(l.larghezze().iter().all(|w| *w <= 2048), "{:?}", l.larghezze());
        assert!(l.larghezze().contains(&2048), "{:?}", l.larghezze());
    }

    #[test]
    fn le_miniature_si_disegnano_anche_senza_il_lettore_sullo_schermo() {
        // Durante la presentazione il lettore non si mostra, ma la sua griglia sì.
        let mut tel = Telefono::new();
        let mut l = tel.lettore();
        let fine = Instant::now() + Duration::from_secs(20);
        while l.miniature_pronte() < 4 {
            assert!(Instant::now() < fine, "le miniature non sono arrivate in 20 s");
            tel.solo_griglia(&mut l);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(l.disegnate(), 4, "ha disegnato anche le slide del lettore");
    }

    #[test]
    fn la_griglia_larga_ha_piu_colonne() {
        // Col telefono girato la griglia è larga 880 dp: quattro colonne, miniature più piccole.
        let mut tel = Telefono::new();
        let mut l = tel.lettore();
        let fine = Instant::now() + Duration::from_secs(20);
        while l.miniature_pronte() < 4 {
            assert!(Instant::now() < fine, "le miniature non sono arrivate in 20 s");
            tel.solo_griglia_larga(&mut l, 880.0);
            std::thread::sleep(Duration::from_millis(5));
        }
        // (880 - 8 - 3 × 12) / 4 = 209 dp, a 2,625 pixel per dp.
        assert_eq!(l.larghezze_miniature(), vec![549; 4]);
    }

    /// Col telefono da 372 dp girato (827 × 372 dp) la griglia nel foglio è larga 791 dp: con
    /// tre colonne una riga sarebbe alta 184 dp e la seconda finirebbe tutta sotto il bordo del
    /// foglio. Una riga non va oltre due quinti dello schermo (149 dp): quattro colonne.
    #[test]
    fn col_telefono_girato_della_riga_dopo_si_vede_un_pezzo() {
        let mut tel = Telefono { schermo: vec2(827.0, 372.0), ..Telefono::new() };
        let mut l = tel.lettore();
        let fine = Instant::now() + Duration::from_secs(20);
        while l.miniature_pronte() < 4 {
            assert!(Instant::now() < fine, "le miniature non sono arrivate in 20 s");
            tel.solo_griglia_larga(&mut l, 791.0);
            std::thread::sleep(Duration::from_millis(5));
        }
        // (791 - 8 - 3 × 12) / 4 = 186,75 dp, a 2,625 pixel per dp; la riga è alta 105 + 42 dp.
        assert_eq!(l.larghezze_miniature(), vec![490; 4]);
    }

    #[test]
    fn le_slide_in_quattro_terzi_e_in_verticale_hanno_le_loro_proporzioni() {
        let mut tel = Telefono::new();
        let mut l = tel.lettore_di(crate::prova::QUATTRO_TERZI);
        let f = tel.aspetta(&mut l);
        let prima = f.immagini().into_iter().min_by(|a, b| a.top().total_cmp(&b.top())).expect("la prima slide");
        // Larghe 380 dp, alte tre quarti.
        assert!((prima.width() - 380.0).abs() < 0.5 && (prima.height() - 285.0).abs() < 0.5, "{prima:?}");
        let mut l = tel.lettore_di(crate::prova::VERTICALE);
        let f = tel.aspetta(&mut l);
        let r = f.immagini().into_iter().next().expect("la slide");
        assert!((r.width() - 380.0).abs() < 0.5 && (r.height() - 380.0 * 960.0 / 540.0).abs() < 0.5, "{r:?}");
        // Disegnata ai pixel dello schermo, sotto il lato delle texture.
        assert_eq!(l.larghezze(), vec![998]);
    }

    #[test]
    fn senza_slide_il_lettore_non_aspetta_niente() {
        let mut tel = Telefono::new();
        let mut l = tel.lettore_di(crate::prova::SENZA_SLIDE);
        let f = tel.aspetta(&mut l);
        assert!(f.immagini().is_empty());
        assert_eq!(l.corrente(), 0);
        let (scelta, _) = tel.passo_con_griglia(&mut l, vec![]);
        assert_eq!(scelta, None);
        assert!(!l.in_attesa());
    }

    #[test]
    fn allo_zoom_massimo_le_slide_non_si_ridisegnano_senza_fine() {
        // A zoom 6 una slide è larga 4096 pixel, 38 MB: la vicina chiesta per lo scorrimento
        // superava il tetto, si liberava appena arrivata e si chiedeva di nuovo, per sempre.
        let mut tel = Telefono::new();
        let mut l = tel.lettore();
        tel.aspetta(&mut l);
        let p = pos2(200.0, 330.0);
        tel.passo(&mut l, vec![egui::Event::PointerMoved(p), egui::Event::Zoom(6.0)]);
        assert_eq!(l.zoom(), tascabile_pdf::geometria::ZOOM_MASSIMO);
        for _ in 0..20 {
            tel.passo(&mut l, vec![]);
        }
        tel.aspetta(&mut l);
        let disegnate = l.disegnate();
        for _ in 0..60 {
            tel.passo(&mut l, vec![]);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(l.disegnate(), disegnate, "le slide si ridisegnano senza fine");
        assert!(l.larghezze().contains(&4096), "{:?}", l.larghezze());
    }

    #[test]
    fn il_doppio_tocco_ingrandisce_e_le_slide_si_ridisegnano_nitide() {
        let mut tel = Telefono::new();
        let mut l = tel.lettore();
        tel.aspetta(&mut l);
        let p = pos2(200.0, 120.0);
        tocca(&mut tel, &mut l, p);
        tocca(&mut tel, &mut l, p);
        assert_eq!(l.zoom(), tascabile_pdf::geometria::ZOOM_DOPPIO_TOCCO);
        tel.passo(&mut l, vec![]);
        tel.aspetta(&mut l);
        // 380 × 2,5 dp a 2,625 pixel per dp: 2494 pixel, sotto il tetto di 4096.
        assert!(l.larghezze().contains(&2494), "{:?}", l.larghezze());
    }
}
