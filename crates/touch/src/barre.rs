//! Le barre: la barra alta con titolo e sottotitolo, la barra di formato flottante a pillola e
//! la barra del contesto scura sopra la selezione.

use egui::{Area, Color32, Context, CornerRadius, Id, Order, Rect, Response, Sense, Ui, UiBuilder, Vec2, WidgetInfo, WidgetType, pos2, vec2};

use crate::disegno::{Riga, testo_tagliato};
use crate::icone;
use crate::pulsanti::{Pulsante, PulsanteIcona, info};
use crate::tema::{self, Carattere};

/// Il raggio dei pulsanti icona tondi.
const TONDO: u8 = (tema::PULSANTE / 2.0) as u8;

/// Il posto di un pulsante icona con la metà sinistra a `x`, a metà altezza di `rect`.
fn quadro(rect: Rect, x: f32) -> Rect {
    quadro_di(rect, x, tema::PULSANTE)
}

/// Come `quadro`, per un pulsante di lato `lato`.
fn quadro_di(rect: Rect, x: f32, lato: f32) -> Rect {
    Rect::from_min_size(pos2(x, rect.center().y - lato / 2.0), Vec2::splat(lato))
}

/// Nella barra flottante: lo spazio minimo fra due elementi (pulsanti e separatori), e il lato
/// più piccolo di un pulsante quando la barra è stretta.
const STACCO: f32 = 4.0;
const LATO_MINIMO: f32 = 40.0;

/// La barra alta (`.barra-alta`): indietro, titolo in grassetto con sotto un sottotitolo grigio
/// (con un'icona di stato, come la spunta verde di «Salvato»), le azioni a destra e, in fondo a
/// destra, un pulsante principale con testo (come «Crea PDF» della scansione).
#[derive(Debug, Clone)]
pub struct BarraAlta<'a> {
    titolo: &'a str,
    sottotitolo: Option<&'a str>,
    stato: Option<(&'a str, Color32)>,
    indietro: Option<&'a str>,
    azioni: Vec<PulsanteIcona<'a>>,
    principale: Option<Pulsante<'a>>,
}

/// Che cosa è stato toccato nella barra alta.
#[derive(Debug)]
pub struct RispostaBarraAlta {
    pub indietro: bool,
    /// L'azione toccata, nell'ordine in cui sono state aggiunte.
    pub azione: Option<usize>,
    /// Il pulsante principale.
    pub principale: bool,
    pub response: Response,
}

/// Dal bordo destro al pulsante principale.
const PRINCIPALE_DAL_BORDO: f32 = 8.0;

impl<'a> BarraAlta<'a> {
    pub fn new(titolo: &'a str) -> Self {
        Self { titolo, sottotitolo: None, stato: None, indietro: None, azioni: Vec::new(), principale: None }
    }

    /// Il pulsante principale con testo, in fondo a destra: le azioni stanno alla sua sinistra.
    pub fn principale(mut self, pulsante: Pulsante<'a>) -> Self {
        self.principale = Some(pulsante);
        self
    }

    /// La freccia indietro a sinistra, con questa etichetta per l'accessibilità.
    pub fn indietro(mut self, etichetta: &'a str) -> Self {
        self.indietro = Some(etichetta);
        self
    }

    pub fn sottotitolo(mut self, sottotitolo: &'a str) -> Self {
        self.sottotitolo = Some(sottotitolo);
        self
    }

    /// Un'icona piena prima del sottotitolo, in questo colore.
    pub fn stato(mut self, icona: &'a str, colore: Color32) -> Self {
        self.stato = Some((icona, colore));
        self
    }

    /// Un'azione a destra; le azioni stanno nell'ordine in cui si aggiungono.
    pub fn azione(mut self, pulsante: PulsanteIcona<'a>) -> Self {
        self.azioni.push(pulsante);
        self
    }

    /// Disegna la barra in cima allo spazio libero di `ui`, larga quanto lo spazio.
    pub fn mostra(self, ui: &mut Ui) -> RispostaBarraAlta {
        let rect = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), tema::BARRA_ALTA));
        ui.scope_builder(UiBuilder::new().max_rect(rect), |ui| self.dentro(ui, rect)).inner
    }

    fn dentro(self, ui: &mut Ui, rect: Rect) -> RispostaBarraAlta {
        let response = ui.allocate_rect(rect, Sense::hover());
        ui.painter().rect_filled(rect, 0.0, tema::CARTA);
        let mut sinistra = rect.left() + tema::MARGINE_LARGO;
        let mut indietro = false;
        if let Some(etichetta) = self.indietro {
            indietro = PulsanteIcona::new(icone::ARROW_BACK, etichetta).mostra_in(ui, quadro(rect, rect.left() + 4.0), TONDO).clicked();
            sinistra = rect.left() + 4.0 + tema::PULSANTE + 2.0;
        }
        // Il pulsante principale in fondo a destra, poi le azioni alla sua sinistra.
        let mut principale = false;
        let mut fine = rect.right() - 4.0;
        if let Some(p) = self.principale {
            let largo = p.larghezza_per(ui);
            let posto = Rect::from_min_size(
                pos2(rect.right() - PRINCIPALE_DAL_BORDO - largo, rect.center().y - tema::PULSANTE / 2.0),
                vec2(largo, tema::PULSANTE),
            );
            principale = ui.put(posto, p).clicked();
            fine = posto.left() - 4.0;
        }
        let n = self.azioni.len() as f32;
        let prima = fine - n * tema::PULSANTE;
        let mut azione = None;
        for (i, p) in self.azioni.into_iter().enumerate() {
            if p.mostra_in(ui, quadro(rect, prima + i as f32 * tema::PULSANTE), TONDO).clicked() {
                azione = Some(i);
            }
        }
        let destra = if n > 0.0 {
            prima - 4.0
        } else if principale_c_e(fine, rect) {
            fine - 4.0
        } else {
            rect.right() - tema::MARGINE_LARGO
        };
        let largo = destra - sinistra;
        let ctx = ui.ctx().clone();
        let titolo = testo_tagliato(ui, self.titolo, Carattere::Grassetto.font(&ctx, 17.0), tema::INCHIOSTRO, largo);
        let icona = self.stato.map(|(i, c)| ui.painter().layout_no_wrap(i.to_owned(), Carattere::IconaPiena.font(&ctx, 14.0), c));
        let davanti = icona.as_ref().map_or(0.0, |g| g.size().x + 4.0);
        let sotto = self.sottotitolo.map(|s| testo_tagliato(ui, s, Carattere::Testo.font(&ctx, 12.5), tema::MATITA, largo - davanti));
        let alto_sotto = match (&sotto, &icona) {
            (Some(s), Some(i)) => s.size().y.max(i.size().y),
            (Some(s), None) => s.size().y,
            _ => 0.0,
        };
        let y = rect.center().y - (titolo.size().y + alto_sotto) / 2.0;
        let posto = Rect::from_min_size(pos2(sinistra, y), titolo.size());
        ui.painter().galley(posto.min, titolo, Color32::PLACEHOLDER);
        ui.allocate_rect(posto, Sense::hover()).widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, self.titolo));
        if let (Some(s), Some(testo)) = (sotto, self.sottotitolo) {
            let centro = posto.bottom() + alto_sotto / 2.0;
            if let Some(i) = icona {
                ui.painter().galley(pos2(sinistra, centro - i.size().y / 2.0), i, Color32::PLACEHOLDER);
            }
            let posto = Rect::from_min_size(pos2(sinistra + davanti, centro - s.size().y / 2.0), s.size());
            ui.painter().galley(posto.min, s, Color32::PLACEHOLDER);
            ui.allocate_rect(posto, Sense::hover()).widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, testo));
        }
        RispostaBarraAlta { indietro, azione, principale, response }
    }
}

/// C'è il pulsante principale: le azioni non partono dal bordo.
fn principale_c_e(fine: f32, rect: Rect) -> bool {
    fine < rect.right() - 4.0
}

#[derive(Debug, Clone)]
enum Elemento<'a> {
    Pulsante(PulsanteIcona<'a>),
    Separatore,
}

/// La barra di formato flottante (`.formato`): una pillola bianca con l'ombra, i pulsanti
/// distribuiti su tutta la larghezza e separatori sottili fra i gruppi.
#[derive(Debug, Clone, Default)]
pub struct BarraFlottante<'a> {
    elementi: Vec<Elemento<'a>>,
    raggio: Option<u8>,
}

impl<'a> BarraFlottante<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn pulsante(mut self, pulsante: PulsanteIcona<'a>) -> Self {
        self.elementi.push(Elemento::Pulsante(pulsante));
        self
    }

    pub fn separatore(mut self) -> Self {
        self.elementi.push(Elemento::Separatore);
        self
    }

    /// Il raggio degli angoli; senza, la barra è una pillola (metà dell'altezza).
    pub fn raggio(mut self, raggio: u8) -> Self {
        self.raggio = Some(raggio);
        self
    }

    /// Il posto della barra in fondo a `zona`: 14 dp dai lati, 12 dal fondo.
    pub fn posto(zona: Rect) -> Rect {
        Rect::from_min_max(
            pos2(zona.left() + tema::MARGINE, zona.bottom() - tema::SOPRA_IL_FONDO - tema::FORMATO),
            pos2(zona.right() - tema::MARGINE, zona.bottom() - tema::SOPRA_IL_FONDO),
        )
    }

    /// Il posto della barra con la tastiera aperta (bozzetto 3), in fondo a `zona`, che finisce
    /// dove comincia la tastiera: più bassa, 8 dp dai lati e dalla tastiera. Va disegnata col
    /// raggio [`tema::RAGGIO_FORMATO_SULLA_TASTIERA`].
    pub fn posto_sulla_tastiera(zona: Rect) -> Rect {
        let m = tema::SOPRA_LA_TASTIERA;
        Rect::from_min_max(pos2(zona.left() + m, zona.bottom() - m - tema::FORMATO_SULLA_TASTIERA), pos2(zona.right() - m, zona.bottom() - m))
    }

    /// Disegna la barra in `rect`, sopra quello che c'è già, e dice quale pulsante è stato
    /// toccato (contando solo i pulsanti). Un tocco sulla barra fuori dai pulsanti resta alla barra.
    pub fn mostra(self, ui: &mut Ui, rect: Rect) -> Option<usize> {
        let mut ui = ui.new_child(UiBuilder::new().max_rect(rect).id_salt("barra flottante"));
        let raggio = CornerRadius::same(self.raggio.unwrap_or((rect.height() / 2.0) as u8));
        tema::dipingi_ombra(ui.painter(), rect, raggio, &tema::OMBRA_FLOTTANTE);
        ui.painter().rect_filled(rect, raggio, tema::FOGLIO);
        ui.allocate_rect(rect, Sense::click());
        let interno = rect.shrink2(vec2(6.0, 0.0));
        // I pulsanti sono di 48 dp; se così non resta lo stacco fra un elemento e l'altro (a 372
        // dp si toccavano, e il cerchio del pulsante acceso copriva il separatore), più piccoli,
        // non sotto i 40.
        let pulsanti = self.elementi.iter().filter(|e| matches!(e, Elemento::Pulsante(_))).count();
        let fili = self.elementi.len() - pulsanti;
        let lato = match pulsanti {
            0 => tema::PULSANTE,
            n => ((interno.width() - fili as f32 - STACCO * self.elementi.len().saturating_sub(1) as f32) / n as f32)
                .clamp(LATO_MINIMO, tema::PULSANTE),
        };
        let largo = |e: &Elemento<'_>| match e {
            Elemento::Pulsante(_) => lato,
            Elemento::Separatore => 1.0,
        };
        let occupato: f32 = self.elementi.iter().map(largo).sum();
        let passo = if self.elementi.len() > 1 { (interno.width() - occupato) / (self.elementi.len() - 1) as f32 } else { 0.0 };
        let (mut x, mut n, mut scelto) = (interno.left(), 0, None);
        for e in self.elementi {
            let w = largo(&e);
            match e {
                Elemento::Pulsante(p) => {
                    if p.mostra_in(&mut ui, quadro_di(rect, x, lato), (lato / 2.0) as u8).clicked() {
                        scelto = Some(n);
                    }
                    n += 1;
                }
                Elemento::Separatore => {
                    ui.painter().rect_filled(Rect::from_min_size(pos2(x, rect.center().y - 14.0), vec2(1.0, 28.0)), 0.0, tema::FILO);
                }
            }
            x += w + passo;
        }
        scelto
    }
}

/// La barra del lettore PDF (`.modi` del bozzetto 7): la pillola bianca con l'ombra della barra
/// di formato, alta 64 dp, con le voci fatte di icona e nome sotto, in grigio, che si dividono la
/// larghezza.
#[derive(Debug, Clone, Default)]
pub struct BarraVoci<'a> {
    voci: Vec<(&'a str, &'a str)>,
    /// La voce accesa (`.modo.acceso`), se ce n'è una: le voci sono modi.
    accesa: Option<usize>,
}

/// Dai lati della zona (`.modi { left: 12px; right: 12px }`).
const VOCI_DAI_LATI: f32 = 12.0;
/// La larghezza del fondo della voce accesa (`.modo { width: 72px }`).
const VOCE_ACCESA: f32 = 72.0;

impl<'a> BarraVoci<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Una voce con la sua icona e il suo nome (anche per l'accessibilità).
    pub fn voce(mut self, icona: &'a str, nome: &'a str) -> Self {
        self.voci.push((icona, nome));
        self
    }

    /// La voce accesa, quando le voci sono modi (Leggi, Compila…): fondo rosso tenue largo
    /// 72 dp con gli angoli da 16, icona e nome rossi (`.modo.acceso`).
    pub fn accesa(mut self, voce: Option<usize>) -> Self {
        self.accesa = voce;
        self
    }

    /// Il posto della barra in fondo a `zona`: 12 dp dai lati e dal fondo.
    pub fn posto(zona: Rect) -> Rect {
        Rect::from_min_max(
            pos2(zona.left() + VOCI_DAI_LATI, zona.bottom() - tema::SOPRA_IL_FONDO - tema::VOCI),
            pos2(zona.right() - VOCI_DAI_LATI, zona.bottom() - tema::SOPRA_IL_FONDO),
        )
    }

    /// Disegna la barra in `rect`, sopra quello che c'è già, e dice quale voce è stata toccata.
    pub fn mostra(self, ui: &mut Ui, rect: Rect) -> Option<usize> {
        let mut ui = ui.new_child(UiBuilder::new().max_rect(rect).id_salt("barra delle voci"));
        let raggio = CornerRadius::same(tema::RAGGIO_VOCI);
        tema::dipingi_ombra(ui.painter(), rect, raggio, &tema::OMBRA_FLOTTANTE);
        ui.painter().rect_filled(rect, raggio, tema::FOGLIO);
        ui.allocate_rect(rect, Sense::click());
        let ctx = ui.ctx().clone();
        let interno = rect.shrink2(vec2(4.0, 6.0));
        let largo = interno.width() / self.voci.len().max(1) as f32;
        let mut scelto = None;
        for (i, (icona, nome)) in self.voci.into_iter().enumerate() {
            let posto = Rect::from_min_size(pos2(interno.left() + i as f32 * largo, interno.top()), vec2(largo, interno.height()));
            let r = ui.allocate_rect(posto, Sense::click());
            let accesa = self.accesa == Some(i);
            info(&r, true, self.accesa.map(|_| accesa), nome);
            let pillola = Rect::from_center_size(posto.center(), vec2(VOCE_ACCESA.min(largo), posto.height()));
            if accesa {
                ui.painter().rect_filled(pillola, tema::RAGGIO_VOCE, tema::ROSSO_TENUE);
            } else if r.is_pointer_button_down_on() {
                ui.painter().rect_filled(posto, tema::RAGGIO_VOCE, tema::PREMUTO);
            }
            // `.modo`: l'icona da 22, 2 dp, il nome in grassetto da 12, in grigio (rosso se acceso).
            let colore = if accesa { tema::ROSSO } else { tema::MATITA };
            let g_icona = ui.painter().layout_no_wrap(icona.to_owned(), Carattere::Icona.font(&ctx, 22.0), colore);
            let g_nome = testo_tagliato(&ui, nome, Carattere::Grassetto.font(&ctx, 12.0), colore, largo - 4.0);
            let alto = g_icona.size().y + 2.0 + g_nome.size().y;
            let cima = posto.center().y - alto / 2.0;
            let sotto = cima + g_icona.size().y + 2.0;
            ui.painter().galley(pos2(posto.center().x - g_icona.size().x / 2.0, cima), g_icona, Color32::PLACEHOLDER);
            ui.painter().galley(pos2(posto.center().x - g_nome.size().x / 2.0, sotto), g_nome, Color32::PLACEHOLDER);
            if r.clicked() {
                scelto = Some(i);
            }
        }
        scelto
    }
}

/// Una voce della [`BarraScorrevole`].
#[derive(Debug, Clone)]
struct VoceScorrevole<'a> {
    icona: &'a str,
    nome: &'a str,
    /// Il nome per l'accessibilità e le prove, quando quello sulla barra è più corto.
    etichetta: &'a str,
    acceso: Option<bool>,
    spento: bool,
}

/// La voce più stretta della barra che scorre, in dp: un bersaglio da dito anche col nome corto.
const VOCE_STRETTA: f32 = 64.0;
/// Lo spazio ai lati del nome di una voce, in tutto.
const ATTORNO_AL_NOME: f32 = 18.0;

/// La barra della tabella: la pillola bianca con l'ombra della barra delle voci (alta 64 dp), con
/// voci fatte di icona e nome sotto, larghe quanto il loro nome, in una fila che scorre col dito
/// quando non stanno tutte (l'ultima parte sfuma: si capisce che c'è dell'altro). Un interruttore
/// acceso ha il fondo blu tenue e l'icona blu, come i pulsanti accesi della barra di formato; una
/// voce spenta è grigia e non si tocca.
#[derive(Debug, Clone, Default)]
pub struct BarraScorrevole<'a> {
    voci: Vec<VoceScorrevole<'a>>,
}

impl<'a> BarraScorrevole<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Una voce con la sua icona e il suo nome; `spenta` non si tocca.
    pub fn voce(mut self, icona: &'a str, nome: &'a str, spenta: bool) -> Self {
        self.voci.push(VoceScorrevole { icona, nome, etichetta: nome, acceso: None, spento: spenta });
        self
    }

    /// Un interruttore: `nome` sulla barra, `etichetta` per l'accessibilità.
    pub fn interruttore(mut self, icona: &'a str, nome: &'a str, etichetta: &'a str, acceso: bool) -> Self {
        self.voci.push(VoceScorrevole { icona, nome, etichetta, acceso: Some(acceso), spento: false });
        self
    }

    /// Disegna la barra in `rect`, sopra quello che c'è già, e dice quale voce è stata toccata,
    /// col suo posto sullo schermo (per aprirci sopra un menu).
    pub fn mostra(self, ui: &mut Ui, rect: Rect) -> Option<(usize, Rect)> {
        let mut ui = ui.new_child(UiBuilder::new().max_rect(rect).id_salt("barra scorrevole"));
        let raggio = CornerRadius::same(tema::RAGGIO_VOCI);
        tema::dipingi_ombra(ui.painter(), rect, raggio, &tema::OMBRA_FLOTTANTE);
        ui.painter().rect_filled(rect, raggio, tema::FOGLIO);
        ui.allocate_rect(rect, Sense::click());
        let interno = rect.shrink2(vec2(10.0, 6.0));
        let ctx = ui.ctx().clone();
        let mut scelto = None;
        ui.scope_builder(UiBuilder::new().max_rect(interno), |ui| {
            ui.set_clip_rect(interno);
            crate::controlli::fila_scorrevole(ui, "voci della barra scorrevole", tema::FOGLIO, |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for (i, v) in self.voci.into_iter().enumerate() {
                    let acceso = v.acceso == Some(true);
                    let colore = if v.spento {
                        tema::SPENTO
                    } else if acceso {
                        tema::BLU
                    } else {
                        tema::MATITA
                    };
                    let g_icona = ui.painter().layout_no_wrap(v.icona.to_owned(), Carattere::Icona.font(&ctx, 22.0), colore);
                    let g_nome = ui.painter().layout_no_wrap(v.nome.to_owned(), Carattere::Grassetto.font(&ctx, 12.0), colore);
                    let largo = (g_nome.size().x + ATTORNO_AL_NOME).max(VOCE_STRETTA);
                    let senso = if v.spento { Sense::hover() } else { Sense::click() };
                    let (posto, r) = ui.allocate_exact_size(vec2(largo, interno.height()), senso);
                    info(&r, !v.spento, v.acceso, v.etichetta);
                    if r.is_pointer_button_down_on() {
                        ui.painter().rect_filled(posto, tema::RAGGIO_VOCE, if acceso { tema::PREMUTO_VELO } else { tema::PREMUTO });
                    } else if acceso {
                        ui.painter().rect_filled(posto, tema::RAGGIO_VOCE, tema::BLU_TENUE);
                    }
                    let alto = g_icona.size().y + 2.0 + g_nome.size().y;
                    let cima = posto.center().y - alto / 2.0;
                    let sotto = cima + g_icona.size().y + 2.0;
                    ui.painter().galley(pos2(posto.center().x - g_icona.size().x / 2.0, cima), g_icona, Color32::PLACEHOLDER);
                    ui.painter().galley(pos2(posto.center().x - g_nome.size().x / 2.0, sotto), g_nome, Color32::PLACEHOLDER);
                    if r.clicked() {
                        scelto = Some((i, posto));
                    }
                }
            });
        });
        scelto
    }
}

/// Quanto sta la barra del contesto sopra la selezione.
const SOPRA: f32 = 20.0;
/// Quanto sta sotto, quando sopra non c'è posto: oltre le gocce delle maniglie.
const SOTTO: f32 = 40.0;
/// Dai bordi della zona.
const BORDO: f32 = 8.0;

/// La barra del contesto (`.contesto`): una pillola scura sopra la selezione, con azioni fatte di
/// icona ed etichetta (o della sola etichetta) separate da linee sottili; le azioni che non
/// stanno nella larghezza della zona vanno nella riga sotto, come quelle dopo un `a_capo`; sotto,
/// se ci sono, i colori da scegliere.
#[derive(Debug, Clone, Default)]
pub struct BarraContesto<'a> {
    azioni: Vec<(&'a str, &'a str)>,
    /// Le azioni da cui comincia una riga nuova (`a_capo`).
    a_capo: Vec<usize>,
    colori: &'a [(Color32, &'a str)],
    colore: Option<usize>,
}

/// La riga dei colori della barra del contesto: alta tanto, un colore ogni tanto, il raggio.
const RIGA_COLORI: f32 = 40.0;
const PASSO_COLORI: f32 = 40.0;
const RAGGIO_COLORE: f32 = 10.0;

impl<'a> BarraContesto<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Un'azione con la sua icona; con l'icona vuota solo l'etichetta (una parola da scegliere).
    pub fn azione(mut self, icona: &'a str, etichetta: &'a str) -> Self {
        self.azioni.push((icona, etichetta));
        self
    }

    /// Le azioni dopo questa vanno in una riga nuova.
    pub fn a_capo(mut self) -> Self {
        if !self.azioni.is_empty() && self.a_capo.last() != Some(&self.azioni.len()) {
            self.a_capo.push(self.azioni.len());
        }
        self
    }

    /// I colori in una riga sotto le azioni (colore, nome per l'accessibilità), quello `acceso`
    /// cerchiato: toccarne uno dà il suo numero dopo quelli delle azioni.
    pub fn colori(mut self, colori: &'a [(Color32, &'a str)], acceso: Option<usize>) -> Self {
        self.colori = colori;
        self.colore = acceso;
        self
    }

    fn righe(&self, ctx: &Context) -> Vec<Riga> {
        let font = Carattere::Grassetto.font(ctx, 14.5);
        self.azioni
            .iter()
            .map(|&(i, e)| {
                let icona = (!i.is_empty()).then_some((i, Carattere::Icona, 20.0));
                Riga::dal_contesto(ctx, icona, Some((e, font.clone())), 6.0, tema::FOGLIO)
            })
            .collect()
    }

    /// Le azioni divise in file larghe al più `largo` (una azione sola resta nella sua fila anche
    /// se è più larga), coi ritorni a capo chiesti.
    fn file(&self, righe: &[Riga], largo: f32) -> Vec<std::ops::Range<usize>> {
        let mut file = Vec::new();
        let (mut da, mut x) = (0, 12.0);
        for (i, r) in righe.iter().enumerate() {
            // Con la linea sottile prima, se non è la prima della fila.
            let w = r.larghezza() + 24.0 + 1.0;
            if i > da && (self.a_capo.contains(&i) || x + w > largo) {
                file.push(da..i);
                (da, x) = (i, 12.0);
            }
            x += if i > da { w } else { w - 1.0 };
        }
        if da < righe.len() {
            file.push(da..righe.len());
        }
        file
    }

    fn larghezza_fila(righe: &[Riga], fila: &std::ops::Range<usize>) -> f32 {
        12.0 + righe[fila.clone()].iter().map(|r| r.larghezza() + 24.0).sum::<f32>() + fila.len().saturating_sub(1) as f32
    }

    fn misura_di(righe: &[Riga], file: &[std::ops::Range<usize>], colori: usize) -> Vec2 {
        let azioni = file.iter().map(|f| Self::larghezza_fila(righe, f)).fold(12.0, f32::max);
        let largo = azioni.max(12.0 + colori as f32 * PASSO_COLORI);
        vec2(largo, tema::CONTESTO * file.len().max(1) as f32 + if colori > 0 { RIGA_COLORI } else { 0.0 })
    }

    /// La larghezza più grande di una barra nella `zona`.
    fn larghezza_massima(zona: Rect) -> f32 {
        zona.width() - 2.0 * BORDO
    }

    /// La misura della barra con queste azioni, in una zona larga quanto si vuole (le azioni
    /// vanno a capo solo dove lo si è chiesto).
    pub fn misura(&self, ui: &Ui) -> Vec2 {
        let righe = self.righe(ui.ctx());
        Self::misura_di(&righe, &self.file(&righe, f32::INFINITY), self.colori.len())
    }

    /// La misura della barra con queste azioni nella `zona`, con le file che ci stanno.
    pub fn misura_nella(&self, ctx: &Context, zona: Rect) -> Vec2 {
        let righe = self.righe(ctx);
        Self::misura_di(&righe, &self.file(&righe, Self::larghezza_massima(zona)), self.colori.len())
    }

    /// Il posto di una barra di questa misura: sopra la `selezione`, centrata, dentro la `zona`;
    /// sotto la selezione se sopra non c'è posto.
    pub fn posto(misura: Vec2, selezione: Rect, zona: Rect) -> Rect {
        let destra = (zona.right() - BORDO - misura.x).max(zona.left() + BORDO);
        let x = (selezione.center().x - misura.x / 2.0).clamp(zona.left() + BORDO, destra);
        let sopra = selezione.top() - SOPRA - misura.y;
        let y = if sopra >= zona.top() + BORDO { sopra } else { (selezione.bottom() + SOTTO).min(zona.bottom() - BORDO - misura.y) };
        Rect::from_min_size(pos2(x, y), misura)
    }

    /// Come [`mostra`](Self::mostra), in un'area tutta sua sopra il resto della schermata (anche
    /// sopra una pagina che scorre, che così non prende i tocchi sulla barra).
    pub fn mostra_in_primo_piano(self, ctx: &Context, id: Id, selezione: Rect, zona: Rect) -> Option<usize> {
        let rect = Self::posto(self.misura_nella(ctx, zona), selezione, zona);
        let mut scelto = None;
        Area::new(id).order(Order::Foreground).fixed_pos(rect.min).constrain(false).interactable(true).show(ctx, |ui| {
            // L'area è grande quanto la barra: i tocchi fuori vanno a quello che c'è sotto.
            ui.allocate_rect(rect, Sense::hover());
            scelto = self.mostra(ui, selezione, zona);
        });
        scelto
    }

    /// Disegna la barra vicino alla `selezione`, dentro la `zona`, sopra quello che c'è già, e
    /// dice quale azione (o quale colore, dopo le azioni) è stata toccata.
    pub fn mostra(self, ui: &mut Ui, selezione: Rect, zona: Rect) -> Option<usize> {
        let righe = self.righe(ui.ctx());
        let file = self.file(&righe, Self::larghezza_massima(zona));
        let rect = Self::posto(Self::misura_di(&righe, &file, self.colori.len()), selezione, zona);
        let mut ui = ui.new_child(UiBuilder::new().max_rect(rect).id_salt("barra del contesto"));
        let raggio = CornerRadius::same((tema::CONTESTO / 2.0) as u8);
        tema::dipingi_ombra(ui.painter(), rect, raggio, &tema::OMBRA_CONTESTO);
        ui.painter().rect_filled(rect, raggio, tema::SCURO);
        ui.allocate_rect(rect, Sense::click());
        let mut scelto = None;
        let mut righe: Vec<Option<Riga>> = righe.into_iter().map(Some).collect();
        for (f, fila) in file.iter().enumerate() {
            let meta = rect.top() + tema::CONTESTO * (f as f32 + 0.5);
            if f > 0 {
                // La linea sottile fra una fila e l'altra.
                let y = rect.top() + tema::CONTESTO * f as f32;
                ui.painter().rect_filled(Rect::from_min_max(pos2(rect.left() + 16.0, y), pos2(rect.right() - 16.0, y + 1.0)), 0.0, tema::FILO_SCURO);
            }
            let mut x = rect.left() + 6.0;
            for i in fila.clone() {
                let Some(riga) = righe[i].take() else { continue };
                let etichetta = self.azioni[i].1;
                let az = Rect::from_min_size(pos2(x, meta - 22.0), vec2(riga.larghezza() + 24.0, 44.0));
                let r = ui.allocate_rect(az, Sense::click());
                info(&r, true, None, etichetta);
                if r.is_pointer_button_down_on() {
                    ui.painter().rect_filled(az, 22.0, tema::PREMUTO_SCURO);
                }
                riga.dipingi(ui.painter(), az.left() + 12.0, az.center().y);
                if r.clicked() {
                    scelto = Some(i);
                }
                x = az.right();
                if i + 1 < fila.end {
                    ui.painter().rect_filled(Rect::from_min_size(pos2(x, meta - 12.0), vec2(1.0, 24.0)), 0.0, tema::FILO_SCURO);
                    x += 1.0;
                }
            }
        }
        let n = self.azioni.len();
        if !self.colori.is_empty() {
            let y = rect.top() + tema::CONTESTO * file.len().max(1) as f32 + RIGA_COLORI / 2.0 - 6.0;
            let primo = rect.center().x - (self.colori.len() as f32 - 1.0) * PASSO_COLORI / 2.0;
            for (i, (colore, nome)) in self.colori.iter().enumerate() {
                let c = pos2(primo + i as f32 * PASSO_COLORI, y);
                let r = ui.allocate_rect(Rect::from_center_size(c, Vec2::splat(PASSO_COLORI)), Sense::click());
                let acceso = self.colore == Some(i);
                info(&r, true, Some(acceso), nome);
                if r.is_pointer_button_down_on() {
                    ui.painter().circle_filled(c, RAGGIO_COLORE + 7.0, tema::PREMUTO_SCURO);
                }
                if acceso {
                    ui.painter().circle_stroke(c, RAGGIO_COLORE + 4.0, egui::Stroke::new(2.0, tema::FOGLIO));
                }
                ui.painter().circle_filled(c, RAGGIO_COLORE, *colore);
                if r.clicked() {
                    scelto = Some(n + i);
                }
            }
        }
        scelto
    }
}

#[cfg(test)]
mod test {
    use egui::accesskit::Toggled;
    use egui_kittest::kittest::{NodeT, Queryable};

    use super::*;
    use crate::icone;
    use crate::prova::{SCHERMO, banco, colore_del_testo, famiglia_del_testo, forme, rettangolo_del_testo, riempiti, tocco};
    use crate::pulsanti::{Peso, Pulsante};

    fn barra_del_documento(titolo: &'static str) -> BarraAlta<'static> {
        BarraAlta::new(titolo)
            .indietro("Indietro")
            .sottotitolo("Salvato · Documenti")
            .stato(icone::CHECK_CIRCLE, tema::VERDE)
            .azione(PulsanteIcona::new(icone::UNDO, "Annulla"))
            .azione(PulsanteIcona::new(icone::REDO, "Ripeti").spento(true))
            .azione(PulsanteIcona::new(icone::MORE_VERT, "Menu"))
    }

    #[derive(Default)]
    struct Visto {
        indietro: u32,
        azioni: Vec<usize>,
        rect: Option<egui::Rect>,
    }

    #[test]
    fn la_barra_alta_ha_indietro_titolo_sottotitolo_e_azioni() {
        let mut h = banco(
            |ui, v: &mut Visto| {
                let r = barra_del_documento("Relazione turno notte").mostra(ui);
                v.indietro += u32::from(r.indietro);
                v.azioni.extend(r.azione);
                v.rect = Some(r.response.rect);
            },
            Visto::default(),
        );
        let barra = h.state().rect.unwrap();
        assert_eq!(barra, egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(SCHERMO.x, tema::BARRA_ALTA)));
        assert!(riempiti(&h, tema::CARTA).contains(&barra));
        let indietro = h.get_by_label("Indietro").rect();
        let menu = h.get_by_label("Menu").rect();
        assert_eq!(indietro.left(), 4.0);
        assert_eq!(menu.right(), SCHERMO.x - 4.0);
        assert_eq!(h.get_by_label("Ripeti").rect().right(), menu.left());
        assert_eq!(indietro.center().y, tema::BARRA_ALTA / 2.0);
        // Il titolo in grassetto da 17 sopra il sottotitolo grigio con la spunta verde.
        let titolo = rettangolo_del_testo(&h, "Relazione turno notte").unwrap();
        let sotto = rettangolo_del_testo(&h, "Salvato · Documenti").unwrap();
        assert!(titolo.left() >= indietro.right() && titolo.bottom() <= sotto.top(), "{titolo:?} {sotto:?}");
        assert_eq!(famiglia_del_testo(&h, "Relazione turno notte"), Some(Carattere::Grassetto.famiglia()));
        assert_eq!(colore_del_testo(&h, "Relazione turno notte"), Some(tema::INCHIOSTRO));
        assert_eq!(colore_del_testo(&h, "Salvato · Documenti"), Some(tema::MATITA));
        assert_eq!(colore_del_testo(&h, icone::CHECK_CIRCLE), Some(tema::VERDE));
        assert_eq!(famiglia_del_testo(&h, icone::CHECK_CIRCLE), Some(Carattere::IconaPiena.famiglia()));
        h.get_by_label("Relazione turno notte");
        h.get_by_label("Annulla").click();
        h.run_steps(2);
        h.get_by_label("Ripeti").click();
        h.run_steps(2);
        h.get_by_label("Indietro").click();
        h.run_steps(2);
        assert_eq!(h.state().azioni, vec![0], "Ripeti è spento");
        assert_eq!(h.state().indietro, 1);
    }

    #[test]
    fn un_titolo_lungo_finisce_coi_puntini_prima_delle_azioni() {
        let titolo = "Relazione del turno di notte del reparto di medicina con le consegne per il mattino";
        let h = banco(|ui, _: &mut ()| _ = barra_del_documento(titolo).mostra(ui), ());
        let disegnato = rettangolo_del_testo(&h, titolo).unwrap();
        assert!(disegnato.right() <= h.get_by_label("Annulla").rect().left(), "{disegnato:?}");
    }

    /// Il pulsante principale con testo (come «Crea PDF» della scansione): in fondo a destra, si
    /// tocca, e il titolo lungo finisce coi puntini prima di lui.
    #[test]
    fn il_pulsante_principale_sta_a_destra_e_ferma_il_titolo() {
        #[derive(Default)]
        struct Toccato(u32);
        let titolo = "Scansione del contratto di affitto con tutte le pagine fotografate";
        let mut h = banco(
            |ui, t: &mut Toccato| {
                let r = BarraAlta::new(titolo)
                    .indietro("Indietro")
                    .principale(Pulsante::new("Crea PDF", Peso::Principale).icona(icone::CHECK))
                    .mostra(ui);
                t.0 += u32::from(r.principale);
            },
            Toccato::default(),
        );
        let crea = h.get_by_label("Crea PDF").rect();
        assert!((crea.right() - (SCHERMO.x - 8.0)).abs() < 0.5 && (crea.center().y - tema::BARRA_ALTA / 2.0).abs() < 0.5, "{crea:?}");
        let disegnato = rettangolo_del_testo(&h, titolo).unwrap();
        assert!(disegnato.right() <= crea.left() - 4.0, "{disegnato:?} {crea:?}");
        h.get_by_label("Crea PDF").click();
        h.run_steps(2);
        assert_eq!(h.state().0, 1);
        // Spento non si tocca.
        let mut h = banco(
            |ui, t: &mut Toccato| {
                let r = BarraAlta::new("Scansione").principale(Pulsante::new("Crea PDF", Peso::Principale).spento(true)).mostra(ui);
                t.0 += u32::from(r.principale);
            },
            Toccato::default(),
        );
        h.get_by_label("Crea PDF").click();
        h.run_steps(2);
        assert_eq!(h.state().0, 0);
    }

    #[test]
    fn senza_indietro_il_titolo_parte_dal_margine() {
        let h = banco(|ui, _: &mut ()| _ = BarraAlta::new("Impostazioni").mostra(ui), ());
        let titolo = rettangolo_del_testo(&h, "Impostazioni").unwrap();
        assert!((titolo.left() - tema::MARGINE_LARGO).abs() < 2.0, "{titolo:?}");
        // Senza sottotitolo, a metà altezza.
        assert!((titolo.center().y - tema::BARRA_ALTA / 2.0).abs() < 3.0, "{titolo:?}");
    }

    fn barra_di_formato() -> BarraFlottante<'static> {
        BarraFlottante::new()
            .pulsante(PulsanteIcona::new("Aa", "Formato").testo(Carattere::Titolo, 21.0))
            .separatore()
            .pulsante(PulsanteIcona::new(icone::FORMAT_BOLD, "Grassetto").acceso(true))
            .pulsante(PulsanteIcona::new(icone::FORMAT_ITALIC, "Corsivo").acceso(false))
            .pulsante(PulsanteIcona::new(icone::FORMAT_UNDERLINED, "Sottolineato").acceso(false))
            .separatore()
            .pulsante(PulsanteIcona::new(icone::FORMAT_LIST_BULLETED, "Elenco puntato"))
            .pulsante(PulsanteIcona::new(icone::FORMAT_LIST_NUMBERED, "Elenco numerato"))
            .pulsante(PulsanteIcona::new(icone::ADD_CIRCLE, "Inserisci"))
    }

    #[test]
    fn la_barra_flottante_sta_in_fondo_coi_pulsanti_distribuiti() {
        let zona = egui::Rect::from_min_size(egui::Pos2::ZERO, SCHERMO);
        let posto = BarraFlottante::posto(zona);
        assert_eq!(posto.height(), tema::FORMATO);
        assert_eq!((posto.left(), posto.right()), (tema::MARGINE, SCHERMO.x - tema::MARGINE));
        assert_eq!(posto.bottom(), SCHERMO.y - tema::SOPRA_IL_FONDO);
        let mut h = banco(
            move |ui, scelti: &mut Vec<usize>| {
                scelti.extend(barra_di_formato().mostra(ui, posto));
            },
            Vec::new(),
        );
        assert_eq!(riempiti(&h, tema::FOGLIO), vec![posto]);
        let aa = h.get_by_label("Formato").rect();
        let inserisci = h.get_by_label("Inserisci").rect();
        assert_eq!(aa.left(), posto.left() + 6.0);
        assert_eq!(inserisci.right(), posto.right() - 6.0);
        assert_eq!(aa.center().y, posto.center().y);
        // Due separatori sottili, color filo.
        let fili = riempiti(&h, tema::FILO);
        assert_eq!(fili.len(), 2, "{fili:?}");
        assert!(fili.iter().all(|f| f.width() == 1.0 && f.height() == 28.0), "{fili:?}");
        assert_eq!(riempiti(&h, tema::BLU_TENUE), vec![h.get_by_label("Grassetto").rect()]);
        // L'indice conta solo i pulsanti.
        h.get_by_label("Corsivo").click();
        h.run_steps(2);
        assert_eq!(h.state(), &vec![2]);
    }

    /// Su un telefono stretto (372 e 360 dp, anche con la tastiera) i pulsanti della barra
    /// flottante si fanno più piccoli, mai sotto 40 dp, perché fra due elementi (pulsanti e
    /// separatori) restino almeno 4 dp: a 48 dp si toccavano, e il cerchio del pulsante acceso
    /// copriva il separatore (0.8.3). A 412 dp restano di 48.
    #[test]
    fn sulla_barra_stretta_i_pulsanti_non_si_toccano() {
        // La barra di formato (sette pulsanti, due separatori) e quella di Organizza pagine
        // (sette pulsanti, un separatore prima degli ultimi due).
        fn organizza() -> BarraFlottante<'static> {
            let mut b = BarraFlottante::new();
            for (i, nome) in ["Uno", "Due", "Tre", "Quattro", "Cinque", "Sei", "Sette"].into_iter().enumerate() {
                if i == 5 {
                    b = b.separatore();
                }
                b = b.pulsante(PulsanteIcona::new(icone::DELETE, nome).acceso(i == 4));
            }
            b
        }
        let nomi_formato = ["Formato", "Grassetto", "Corsivo", "Sottolineato", "Elenco puntato", "Elenco numerato", "Inserisci"];
        let nomi_organizza = ["Uno", "Due", "Tre", "Quattro", "Cinque", "Sei", "Sette"];
        for (larga, tastiera) in [(372.0, false), (360.0, false), (372.0, true), (412.0, false)] {
            let zona = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(larga, 800.0));
            let posto = if tastiera { BarraFlottante::posto_sulla_tastiera(zona) } else { BarraFlottante::posto(zona) };
            for quale in 0..2 {
                let h = banco(move |ui, _: &mut ()| _ = if quale == 0 { barra_di_formato() } else { organizza() }.mostra(ui, posto), ());
                let nomi: &[&str] = if quale == 0 { &nomi_formato } else { &nomi_organizza };
                let mut pezzi: Vec<egui::Rect> = nomi.iter().map(|n| h.get_by_label(n).rect()).collect();
                let fili = riempiti(&h, tema::FILO);
                for r in &pezzi {
                    assert!(r.width() >= 40.0 && r.width() <= tema::PULSANTE && (r.width() - r.height()).abs() < 1.0, "{larga}: {r:?}");
                    assert!(r.left() >= posto.left() && r.right() <= posto.right(), "{larga}: {r:?} fuori da {posto:?}");
                }
                if larga >= 412.0 {
                    assert!(pezzi.iter().all(|r| r.width() == tema::PULSANTE), "a 412 dp i pulsanti restano di 48");
                }
                pezzi.extend(fili);
                pezzi.sort_by(|a, b| a.left().total_cmp(&b.left()));
                for coppia in pezzi.windows(2) {
                    let stacco = coppia[1].left() - coppia[0].right();
                    assert!(stacco >= 3.9, "{larga} dp, tastiera {tastiera}, barra {quale}: {stacco} dp fra {:?} e {:?}", coppia[0], coppia[1]);
                }
                // Il cerchio del pulsante acceso è grande quanto il pulsante.
                let acceso = riempiti(&h, tema::BLU_TENUE);
                assert_eq!(acceso.len(), 1);
                assert!(acceso[0].width() <= tema::PULSANTE && acceso[0].width() >= 40.0, "{acceso:?}");
            }
        }
    }

    #[test]
    fn sulla_tastiera_la_barra_e_piu_bassa_e_piu_larga() {
        // Bozzetto 3: 8 dp dai lati e dalla tastiera, alta 52, raggio 24.
        let zona = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(SCHERMO.x, 600.0));
        let posto = BarraFlottante::posto_sulla_tastiera(zona);
        assert_eq!(posto, egui::Rect::from_min_max(egui::pos2(8.0, 600.0 - 8.0 - 52.0), egui::pos2(SCHERMO.x - 8.0, 600.0 - 8.0)));
        let h = banco(move |ui, _: &mut ()| _ = barra_di_formato().raggio(tema::RAGGIO_FORMATO_SULLA_TASTIERA).mostra(ui, posto), ());
        let fondo = forme(&h)
            .into_iter()
            .find_map(|s| match s {
                egui::Shape::Rect(r) if r.fill == tema::FOGLIO && r.blur_width == 0.0 => Some(r),
                _ => None,
            })
            .expect("il fondo della barra");
        assert_eq!(fondo.rect, posto);
        assert_eq!(fondo.corner_radius, egui::CornerRadius::same(24));
        // I pulsanti restano da 48, a metà altezza.
        let aa = h.get_by_label("Formato").rect();
        assert_eq!((aa.height(), aa.center().y), (tema::PULSANTE, posto.center().y));
    }

    #[test]
    fn la_barra_delle_voci_sta_in_fondo_con_icona_e_nome() {
        // Bozzetto 7, `.modi`: 12 dp dai lati e sopra il fondo, alta 64, raggio 22.
        let zona = egui::Rect::from_min_size(egui::Pos2::ZERO, SCHERMO);
        let posto = BarraVoci::posto(zona);
        assert_eq!(posto, egui::Rect::from_min_max(egui::pos2(12.0, SCHERMO.y - 12.0 - 64.0), egui::pos2(SCHERMO.x - 12.0, SCHERMO.y - 12.0)));
        let barra = || BarraVoci::new().voce(icone::GRID_VIEW, "Miniature").voce(icone::TOC, "Indice").voce(icone::NUMBERS, "Vai a pagina");
        let mut h = banco(move |ui, scelti: &mut Vec<usize>| scelti.extend(barra().mostra(ui, posto)), Vec::new());
        assert_eq!(riempiti(&h, tema::FOGLIO), vec![posto]);
        // Le tre voci si dividono la barra, alla stessa altezza; il nome sta sotto l'icona, grigio.
        let voci = ["Miniature", "Indice", "Vai a pagina"].map(|n| h.get_by_label(n).rect());
        assert!((voci[1].center().x - posto.center().x).abs() < 0.5, "{voci:?}");
        assert!(voci.iter().all(|v| v.center().y == posto.center().y && v.height() >= tema::PULSANTE), "{voci:?}");
        for nome in ["Miniature", "Indice", "Vai a pagina"] {
            let testo = rettangolo_del_testo(&h, nome).unwrap();
            assert!(testo.top() > posto.center().y - 4.0, "{nome}: {testo:?}");
            assert_eq!(colore_del_testo(&h, nome), Some(tema::MATITA));
        }
        let icona = rettangolo_del_testo(&h, icone::TOC).unwrap();
        assert!(icona.bottom() <= rettangolo_del_testo(&h, "Indice").unwrap().top() + 1.0);
        h.get_by_label("Vai a pagina").click();
        h.run_steps(2);
        assert_eq!(h.state(), &vec![2]);
    }

    #[test]
    fn nella_barra_dei_modi_quello_acceso_e_rosso_tenue() {
        // Bozzetto 7, `.modo.acceso`: largo 72 dp, raggio 16, fondo rosso tenue, icona e nome rossi.
        let zona = egui::Rect::from_min_size(egui::Pos2::ZERO, SCHERMO);
        let posto = BarraVoci::posto(zona);
        let barra = || BarraVoci::new().voce(icone::CHROME_READER_MODE, "Leggi").voce(icone::APPS, "Strumenti").accesa(Some(0));
        let mut h = banco(move |ui, scelti: &mut Vec<usize>| scelti.extend(barra().mostra(ui, posto)), Vec::new());
        let accesi = riempiti(&h, tema::ROSSO_TENUE);
        assert_eq!(accesi.len(), 1, "{accesi:?}");
        let leggi = h.get_by_label("Leggi").rect();
        assert!((accesi[0].width() - 72.0).abs() < 0.5 && (accesi[0].center().x - leggi.center().x).abs() < 0.5, "{:?} {leggi:?}", accesi[0]);
        assert!(accesi[0].height() >= 48.0 && posto.contains_rect(accesi[0]), "{:?}", accesi[0]);
        assert_eq!(colore_del_testo(&h, "Leggi"), Some(tema::ROSSO));
        assert_eq!(colore_del_testo(&h, icone::CHROME_READER_MODE), Some(tema::ROSSO));
        assert_eq!(colore_del_testo(&h, "Strumenti"), Some(tema::MATITA));
        assert_eq!(h.get_by_label("Leggi").accesskit_node().toggled(), Some(Toggled::True));
        assert_eq!(h.get_by_label("Strumenti").accesskit_node().toggled(), Some(Toggled::False));
        h.get_by_label("Strumenti").click();
        h.run_steps(2);
        assert_eq!(h.state(), &vec![1]);
    }

    #[test]
    fn la_barra_scorrevole_ha_le_voci_larghe_quanto_il_nome_e_scorre() {
        let posto = egui::Rect::from_min_max(egui::pos2(8.0, 700.0), egui::pos2(404.0, 764.0));
        let barra = || {
            BarraScorrevole::new()
                .voce(icone::ADD_ROW_ABOVE, "Riga sopra", false)
                .voce(icone::ADD_ROW_BELOW, "Riga sotto", false)
                .voce(icone::ADD_COLUMN_LEFT, "Colonna a sinistra", false)
                .voce(icone::ADD_COLUMN_RIGHT, "Colonna a destra", false)
                .voce(icone::CELL_MERGE, "Unisci", true)
                .voce(icone::VERTICAL_SPLIT, "Dividi", false)
                .interruttore(icone::TABLE_ROWS, "Intestazione", "Intestazione ripetuta", true)
        };
        let mut h = banco(move |ui, scelti: &mut Vec<(usize, egui::Rect)>| scelti.extend(barra().mostra(ui, posto)), Vec::new());
        assert!(riempiti(&h, tema::FOGLIO).contains(&posto), "la pillola bianca");
        // Le voci stanno in fila, alte quanto la barra meno il bordo, larghe almeno 64 dp e di più
        // col nome lungo; il nome sotto l'icona.
        let riga = h.get_by_label("Riga sopra").rect();
        let colonna = h.get_by_label("Colonna a sinistra").rect();
        assert!(riga.height() >= tema::PULSANTE && (riga.center().y - posto.center().y).abs() < 0.5, "{riga:?}");
        let dividi = h.get_by_label("Dividi").rect();
        assert!((dividi.width() - 64.0).abs() < 0.5 && colonna.width() > riga.width() + 30.0, "{dividi:?} {riga:?} {colonna:?}");
        assert!(rettangolo_del_testo(&h, "Riga sopra").unwrap().top() > riga.center().y - 4.0);
        // Spenta: grigia e non si tocca. Interruttore acceso: blu tenue, icona e nome blu.
        assert_eq!(colore_del_testo(&h, "Unisci"), Some(tema::SPENTO));
        assert!(h.get_by_label("Unisci").accesskit_node().is_disabled());
        assert_eq!(colore_del_testo(&h, "Intestazione"), Some(tema::BLU));
        assert_eq!(h.get_by_label("Intestazione ripetuta").accesskit_node().toggled(), Some(Toggled::True));
        // Le ultime voci sono oltre il bordo: si arriva col dito che scorre.
        let ultima = h.get_by_label("Intestazione ripetuta").rect();
        assert!(ultima.right() > posto.right(), "{ultima:?}");
        let da = egui::pos2(300.0, posto.center().y);
        h.event(egui::Event::PointerMoved(da));
        h.event(egui::Event::PointerButton { pos: da, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
        h.step();
        for k in 1..=10 {
            h.event(egui::Event::PointerMoved(da - egui::vec2(30.0 * k as f32, 0.0)));
            h.step();
        }
        let a = da - egui::vec2(300.0, 0.0);
        h.event(egui::Event::PointerButton { pos: a, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
        h.run_steps(30);
        let ultima = h.get_by_label("Intestazione ripetuta").rect();
        assert!(ultima.right() <= posto.right(), "la barra non è scorsa: {ultima:?}");
        assert!(h.state().is_empty(), "lo scorrimento non è un tocco: {:?}", h.state());
        h.get_by_label("Intestazione ripetuta").click();
        h.run_steps(2);
        assert_eq!(h.state().iter().map(|(i, _)| *i).collect::<Vec<_>>(), [6]);
        assert_eq!(h.state()[0].1, h.get_by_label("Intestazione ripetuta").rect(), "il posto della voce");
        h.get_by_label("Unisci").click();
        h.run_steps(2);
        assert_eq!(h.state().len(), 1, "la voce spenta non si tocca");
    }

    fn barra_della_selezione() -> BarraContesto<'static> {
        BarraContesto::new()
            .azione(icone::CONTENT_CUT, "Taglia")
            .azione(icone::CONTENT_COPY, "Copia")
            .azione(icone::CONTENT_PASTE, "Incolla")
            .azione(icone::SELECT_ALL, "Tutto")
    }

    #[test]
    fn la_barra_del_contesto_sta_sopra_la_selezione_e_dentro_lo_schermo() {
        let zona = egui::Rect::from_min_size(egui::pos2(0.0, 64.0), egui::vec2(412.0, 800.0));
        let misura = egui::vec2(300.0, tema::CONTESTO);
        let sel = egui::Rect::from_min_max(egui::pos2(160.0, 320.0), egui::pos2(380.0, 360.0));
        let r = BarraContesto::posto(misura, sel, zona);
        assert_eq!(r.size(), misura);
        assert!(r.bottom() < sel.top() && r.bottom() > sel.top() - 30.0, "{r:?}");
        assert!(r.right() <= zona.right() - 8.0, "{r:?}");
        // Una selezione in cima: la barra va sotto, oltre le gocce.
        let in_cima = egui::Rect::from_min_max(egui::pos2(20.0, 80.0), egui::pos2(100.0, 100.0));
        let r = BarraContesto::posto(misura, in_cima, zona);
        assert!(r.top() > in_cima.bottom() + 22.0, "{r:?}");
        assert!(r.left() >= zona.left() + 8.0, "{r:?}");
    }

    #[test]
    fn la_barra_del_contesto_e_scura_con_icone_ed_etichette() {
        let zona = egui::Rect::from_min_size(egui::Pos2::ZERO, SCHERMO);
        let sel = egui::Rect::from_min_max(egui::pos2(160.0, 320.0), egui::pos2(380.0, 360.0));
        let mut h = banco(
            move |ui, scelti: &mut Vec<usize>| {
                scelti.extend(barra_della_selezione().mostra(ui, sel, zona));
            },
            Vec::new(),
        );
        let scuri = riempiti(&h, tema::SCURO);
        assert_eq!(scuri.len(), 1, "{scuri:?}");
        assert_eq!(scuri[0].height(), tema::CONTESTO);
        assert!(scuri[0].bottom() < sel.top());
        for etichetta in ["Taglia", "Copia", "Incolla", "Tutto"] {
            assert_eq!(colore_del_testo(&h, etichetta), Some(tema::FOGLIO), "{etichetta}");
            assert!(scuri[0].contains_rect(h.get_by_label(etichetta).rect()), "{etichetta}");
        }
        assert_eq!(famiglia_del_testo(&h, "Copia"), Some(Carattere::Grassetto.famiglia()));
        assert_eq!(colore_del_testo(&h, icone::CONTENT_COPY), Some(tema::FOGLIO));
        // Tre separatori fra quattro azioni.
        assert_eq!(riempiti(&h, tema::FILO_SCURO).len(), 3);
        h.get_by_label("Copia").click();
        h.run_steps(2);
        assert_eq!(h.state(), &vec![1]);
    }

    #[test]
    fn la_barra_del_contesto_coi_colori_sotto_le_azioni() {
        const COLORI: [(Color32, &str); 3] = [(Color32::YELLOW, "Giallo"), (Color32::GREEN, "Verde"), (Color32::RED, "Rosso")];
        let zona = egui::Rect::from_min_size(egui::Pos2::ZERO, SCHERMO);
        let sel = egui::Rect::from_min_max(egui::pos2(160.0, 420.0), egui::pos2(380.0, 460.0));
        let mut h = banco(
            move |ui, scelti: &mut Vec<usize>| {
                let barra = BarraContesto::new()
                    .azione(icone::INK_HIGHLIGHTER, "Evidenzia")
                    .azione(icone::FORMAT_UNDERLINED, "Sottolinea")
                    .colori(&COLORI, Some(1));
                scelti.extend(barra.mostra(ui, sel, zona));
            },
            Vec::new(),
        );
        let scuri = riempiti(&h, tema::SCURO);
        assert_eq!(scuri.len(), 1, "{scuri:?}");
        assert_eq!(scuri[0].height(), tema::CONTESTO + RIGA_COLORI);
        assert!(scuri[0].bottom() < sel.top());
        let rosso = h.get_by_label("Rosso").rect();
        assert!(scuri[0].contains_rect(rosso) && rosso.top() > h.get_by_label("Evidenzia").rect().bottom() - 4.0, "{rosso:?}");
        assert_eq!(h.get_by_label("Verde").accesskit_node().toggled(), Some(Toggled::True));
        assert_eq!(h.get_by_label("Giallo").accesskit_node().toggled(), Some(Toggled::False));
        h.get_by_label("Rosso").click();
        h.run_steps(2);
        h.get_by_label("Sottolinea").click();
        h.run_steps(2);
        assert_eq!(h.state(), &vec![4, 1]);
    }

    #[test]
    fn la_barra_del_contesto_va_a_capo_e_ha_azioni_di_sole_parole() {
        let zona = egui::Rect::from_min_size(egui::Pos2::ZERO, SCHERMO);
        let sel = egui::Rect::from_min_max(egui::pos2(160.0, 420.0), egui::pos2(260.0, 440.0));
        // Le quattro della selezione più Commenta non stanno in 412 dp: Commenta va sotto.
        let mut h = banco(
            move |ui, scelti: &mut Vec<usize>| {
                scelti.extend(barra_della_selezione().azione(icone::CHAT, "Commenta").mostra(ui, sel, zona));
            },
            Vec::new(),
        );
        let scuri = riempiti(&h, tema::SCURO);
        assert_eq!(scuri.len(), 1, "{scuri:?}");
        assert_eq!(scuri[0].height(), 2.0 * tema::CONTESTO);
        assert!(scuri[0].width() <= 412.0 - 16.0 && scuri[0].bottom() < sel.top(), "{:?}", scuri[0]);
        let (tutto, commenta) = (h.get_by_label("Tutto").rect(), h.get_by_label("Commenta").rect());
        assert!(commenta.top() >= tutto.bottom() && scuri[0].contains_rect(commenta), "{tutto:?} {commenta:?}");
        h.get_by_label("Commenta").click();
        h.run_steps(2);
        assert_eq!(h.state(), &vec![4]);

        // Le parole senza icona in una fila, poi a capo le due azioni con l'icona.
        let mut h = banco(
            move |ui, scelti: &mut Vec<usize>| {
                let barra = BarraContesto::new()
                    .azione("", "perché")
                    .azione("", "perciò")
                    .a_capo()
                    .azione(icone::SPELLCHECK, "Aggiungi al dizionario")
                    .azione(icone::VISIBILITY_OFF, "Ignora");
                scelti.extend(barra.mostra(ui, sel, zona));
            },
            Vec::new(),
        );
        let scuri = riempiti(&h, tema::SCURO);
        assert_eq!(scuri[0].height(), 2.0 * tema::CONTESTO);
        let (parola, aggiungi) = (h.get_by_label("perché").rect(), h.get_by_label("Aggiungi al dizionario").rect());
        assert!(aggiungi.top() >= parola.bottom(), "{parola:?} {aggiungi:?}");
        // La parola senza icona: larga quanto il suo testo più i margini, niente spazio per l'icona.
        assert!(parola.width() < 24.0 + 60.0, "{parola:?}");
        assert_eq!(famiglia_del_testo(&h, "perché"), Some(Carattere::Grassetto.famiglia()));
        h.get_by_label("perciò").click();
        h.run_steps(2);
        h.get_by_label("Ignora").click();
        h.run_steps(2);
        assert_eq!(h.state(), &vec![1, 3]);
    }

    /// Le scelte della barra in primo piano e i tocchi arrivati alla pagina sotto.
    #[derive(Default)]
    struct SopraLaPagina {
        scelte: Vec<usize>,
        pagina: u32,
    }

    #[test]
    fn in_primo_piano_la_barra_del_contesto_prende_il_tocco_e_la_pagina_no() {
        let zona = egui::Rect::from_min_max(egui::pos2(10.0, 64.0), egui::pos2(402.0, 843.0));
        let sel = egui::Rect::from_min_max(egui::pos2(160.0, 320.0), egui::pos2(380.0, 360.0));
        let mut h = banco(
            move |ui, s: &mut SopraLaPagina| {
                // La pagina sotto, che prende ogni tocco, disegnata prima.
                if ui.allocate_rect(zona, egui::Sense::click()).clicked() {
                    s.pagina += 1;
                }
                s.scelte.extend(barra_della_selezione().mostra_in_primo_piano(ui.ctx(), egui::Id::new("contesto"), sel, zona));
            },
            SopraLaPagina::default(),
        );
        h.run_steps(2);
        let copia = h.get_by_label("Copia").rect();
        assert!(copia.bottom() < sel.top() && zona.contains_rect(copia), "{copia:?}");
        tocco(&mut h, copia.center());
        assert_eq!((h.state().scelte.clone(), h.state().pagina), (vec![1], 0));
        // Fuori dalla barra il tocco va alla pagina.
        tocco(&mut h, egui::pos2(200.0, 700.0));
        assert_eq!((h.state().scelte.clone(), h.state().pagina), (vec![1], 1));
    }
}
