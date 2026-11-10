//! Compilare un PDF (piano «PDF completo», P2): il modo Compila del bozzetto 7.
//!
//! I campi del modulo evidenziati sulla pagina (fondo blu tenue; quelli pieni col bordo blu, i
//! vuoti tratteggiati, come `.campo.pieno` e `.campo.vuoto`). Un tocco su un campo di testo apre
//! la scrittura: il campo si porta sopra la tastiera, la barra sulla tastiera ha il campo di testo
//! vero (CampoTesto, con la tastiera di Android), «Campo prima», «Campo dopo» e «Fatto»; il valore
//! va al motore solo alla conferma (Invio, «Fatto», un tocco fuori, il campo dopo, Indietro), mai
//! a ogni tasto (ricerca PDFCraft, sezione 5). Caselle e radio cambiano con un tocco; menu ed
//! elenchi si scelgono in un foglio dal fondo; i campi data hanno il loro foglio, con «Oggi» e un
//! selettore di giorno, mese e anno.
//!
//! I moduli senza campi (Fill & Sign): un tocco su un punto vuoto della pagina propone Testo,
//! Spunta, Croce e Data; il segno posato resta scelto, col riquadro blu: si sposta col dito, si
//! ingrandisce con la maniglia nell'angolo, si riscrive o si toglie dalla sua barra.
//!
//! Gli script del modulo restano spenti; se il modulo ha calcoli veri lo si dice una volta, con
//! la proposta di accenderli.
//!
//! Nella diagnosi solo generi e conti: mai il nome di un campo, un valore, un testo scritto.

use eframe::egui::{self, Color32, Id, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, UiBuilder, Vec2, WidgetInfo, WidgetType, pos2, vec2};
use tascabile_pdf::moduli::MISURA_TESTO;
use tascabile_pdf::{Campo, Data, FieldValue, GenereCampo, GenereSegno, Modifica, NuovoSegno, ParteData, Segno};
use tascabile_platform::diagnosi;
use tascabile_touch::tema::{self, Carattere};
use tascabile_touch::{BarraContesto, BarraFlottante, CampoTesto, Dialogo, Passo, Peso, Pulsante, PulsanteIcona, RispostaDialogo, Verso, icone};

use crate::pdf_lavoro::ModoPdf;
use crate::{Errore, Fuoco, TascabileApp, adesso};

/// I pulsanti della barra sulla tastiera (accessibilità e prove).
pub(crate) const CAMPO_PRIMA: &str = "Campo prima";
pub(crate) const CAMPO_DOPO: &str = "Campo dopo";
pub(crate) const FATTO: &str = "Fatto";
pub(crate) const SCEGLI_LA_DATA: &str = "Scegli la data";
/// La proposta di Fill & Sign e la barra di un segno scelto.
pub(crate) const TESTO: &str = "Testo";
pub(crate) const SPUNTA: &str = "Spunta";
pub(crate) const CROCE: &str = "Croce";
pub(crate) const DATA: &str = "Data";
pub(crate) const RISCRIVI: &str = "Riscrivi";
pub(crate) const TOGLI: &str = "Togli";
/// Il foglio della data.
pub(crate) const OGGI: &str = "Oggi";
pub(crate) const METTI_LA_DATA: &str = "Metti la data";
pub(crate) const SCRIVI_A_MANO: &str = "Scrivi";
pub(crate) const SVUOTA: &str = "Svuota";
/// Nel foglio di un menu in cui si può scrivere.
pub(crate) const SCRIVI_UN_ALTRO: &str = "Scrivi un altro valore";
/// Il dialogo dei calcoli.
pub(crate) const CALCOLI: &str = "Calcoli del modulo";
pub(crate) const ACCENDI_I_CALCOLI: &str = "Accendi i calcoli";
pub(crate) const NON_ORA: &str = "Non ora";
const TESTO_CALCOLI: &str = "Questo modulo fa dei calcoli da solo (somme, controlli, campi che si riempiono), con degli script. Gli script sono spenti: se li accendi girano dentro Tascabile, senza rete e senza accesso ai file. Accendili solo per i moduli di chi conosci: uno script fatto apposta può bloccare l'app. Senza, i campi dei calcoli restano da compilare a mano.";
/// Il testo grigio del campo nella barra, per un testo nuovo di Fill & Sign.
pub(crate) const VUOTO_TESTO: &str = "Scrivi il testo";

/// Il tratteggio dei campi vuoti (`.campo.vuoto { border: 1px dashed #93A6D6 }`) e i fondi:
/// rgba(35,70,160,.07) per i pieni, .05 per i vuoti.
const TRATTO: Color32 = Color32::from_rgb(0x93, 0xA6, 0xD6);
const FONDO_PIENO: Color32 = Color32::from_rgba_unmultiplied_const(35, 70, 160, 18);
const FONDO_VUOTO: Color32 = Color32::from_rgba_unmultiplied_const(35, 70, 160, 13);
/// La barra sulla tastiera: alta come la barra di formato (con dentro un campo da 48 dp), a 8 dp
/// dai lati e dalla tastiera.
const BARRA: f32 = 60.0;
/// Un campo piccolo (una casella da 13 pt) si prende anche un poco fuori: almeno tanto per lato.
const PRESA_MINIMA: f32 = 32.0;
/// Il tocco su una pagina prende il campo più vicino entro tanti dp.
const BORDO_CAMPO: f32 = 6.0;
/// La maniglia di un segno scelto: il cerchio, quanto sta fuori dall'angolo (un segno è piccolo:
/// la maniglia sopra lo coprirebbe) e la parte da prendere col dito.
const MANIGLIA: f32 = 7.0;
const FUORI_DALL_ANGOLO: f32 = 12.0;
const PRESA_MANIGLIA: f32 = 28.0;

/// Che cosa si scrive con la tastiera nel modo Compila.
#[derive(Clone, PartialEq)]
pub(crate) enum Bersaglio {
    /// Un campo del modulo, col suo nome.
    Campo(String),
    /// Un testo nuovo di Fill & Sign: la pagina e il punto (pt) del lato sinistro.
    Nuovo {
        pagina: usize,
        punto: [f32; 2],
    },
    /// Il testo di un segno di Fill & Sign già posato.
    Segno {
        pagina: usize,
        indice: usize,
    },
    /// Modo Modifica: un paragrafo o un testo aggiunto da riscrivere.
    Oggetto(crate::pdf_modifica::Chiave),
    /// Modo Modifica: un testo nuovo, con l'angolo in alto a sinistra nel punto (pt).
    TestoNuovo {
        pagina: usize,
        punto: [f32; 2],
    },
    /// Il testo della filigrana e quello dell'intestazione, per i loro fogli.
    Filigrana,
    Intestazione,
    /// Modo Annota: una nota nuova con l'icona nel punto (pt), un testo libero nuovo con
    /// l'angolo in alto a sinistra nel punto, il testo di un'annotazione (nota o testo libero).
    NotaNuova {
        pagina: usize,
        punto: [f32; 2],
    },
    TestoLibero {
        pagina: usize,
        punto: [f32; 2],
    },
    Annotazione {
        pagina: usize,
        indice: usize,
    },
    /// Modo Modifica: l'indirizzo di un link, per il suo foglio.
    Indirizzo,
}

/// Senza il nome del campo.
impl std::fmt::Debug for Bersaglio {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Bersaglio::Campo(_) => f.write_str("Campo"),
            Bersaglio::Nuovo { pagina, .. } => write!(f, "Nuovo {{ pagina: {pagina} }}"),
            Bersaglio::Segno { pagina, indice } => write!(f, "Segno {{ pagina: {pagina}, indice: {indice} }}"),
            Bersaglio::Oggetto(c) => write!(f, "Oggetto {c:?}"),
            Bersaglio::TestoNuovo { pagina, .. } => write!(f, "TestoNuovo {{ pagina: {pagina} }}"),
            Bersaglio::Filigrana => f.write_str("Filigrana"),
            Bersaglio::Intestazione => f.write_str("Intestazione"),
            Bersaglio::NotaNuova { pagina, .. } => write!(f, "NotaNuova {{ pagina: {pagina} }}"),
            Bersaglio::TestoLibero { pagina, .. } => write!(f, "TestoLibero {{ pagina: {pagina} }}"),
            Bersaglio::Annotazione { pagina, indice } => write!(f, "Annotazione {{ pagina: {pagina}, indice: {indice} }}"),
            Bersaglio::Indirizzo => f.write_str("Indirizzo"),
        }
    }
}

/// Un testo che si scrive con la tastiera.
pub(crate) struct Scrittura {
    pub(crate) bersaglio: Bersaglio,
    pub(crate) campo: CampoTesto,
    /// Il testo di quando si è cominciato o dell'ultima conferma: se non cambia, al motore non va
    /// niente.
    confermato: String,
    /// L'altezza della tastiera quando il campo si è portato sopra la barra: se cambia, lo si
    /// riporta.
    visto_con: Option<f32>,
}

/// Un segno di Fill & Sign che si trascina col dito.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Trascina {
    /// Si sposta: di quanto, in dp.
    Sposta { pagina: usize, indice: usize, delta: Vec2 },
    /// La maniglia nell'angolo: di quanto si è mossa, in dp.
    Misura { pagina: usize, indice: usize, delta: Vec2 },
}

/// Lo stato del modo Compila di un PDF.
#[derive(Default)]
pub(crate) struct Compila {
    pub(crate) scrittura: Option<Scrittura>,
    /// Il menu o l'elenco aperto nel foglio: il nome del campo e le scelte segnate.
    pub(crate) scelta: Option<(String, Vec<String>)>,
    /// Il campo data aperto nel suo foglio, con la data del selettore.
    pub(crate) data: Option<(String, Data)>,
    /// La proposta di Fill & Sign: la pagina e il punto toccato (pt).
    pub(crate) proposta: Option<(usize, Pos2)>,
    /// Il segno scelto: pagina e indice (si rilegge dall'istantanea).
    pub(crate) segno: Option<(usize, usize)>,
    /// Un segno appena chiesto su questa pagina: quando arriva, è lui lo scelto.
    pub(crate) da_scegliere: Option<usize>,
    pub(crate) trascina: Option<Trascina>,
    /// La proposta di accendere i calcoli è già stata fatta, e il suo dialogo è aperto.
    pub(crate) calcoli_chiesti: bool,
    pub(crate) chiedi_calcoli: bool,
    /// La firma: dove va, il foglio col dito, la foto.
    pub(crate) firma: crate::pdf_firma::FirmaPdf,
    /// I testi dei campi confermati e mandati al motore, che l'istantanea non ha ancora: tornando
    /// subito a uno di quei campi si riapre con loro, non col testo di prima (analisi del 27/10,
    /// compila 8).
    pub(crate) in_volo: std::collections::HashMap<String, String>,
}

/// Quello che si è toccato nel modo Compila in un frame, da fare dopo averlo disegnato.
enum Azione {
    Campo(String, usize),
    /// Il riquadro di un campo firma: la pagina e il riquadro (pt).
    Firma(usize, [f32; 4]),
    Vuoto(usize, Pos2),
    Segno(usize, usize),
    Sposta(usize, usize, Vec2),
    Misura(usize, usize, Vec2),
}

/// «2 campi da compilare», «1 campo da compilare», «tutto compilato».
pub(crate) fn da_compilare_detto(n: usize) -> String {
    match n {
        0 => "tutto compilato".to_string(),
        1 => "1 campo da compilare".to_string(),
        n => format!("{n} campi da compilare"),
    }
}

/// Il fondo e il bordo di un campo: pieno col bordo blu, vuoto tratteggiato.
fn evidenzia(painter: &egui::Painter, r: Rect, pieno: bool) {
    let raggio = (r.height().min(r.width()) / 5.0).min(4.0);
    if pieno {
        painter.rect(r, raggio, FONDO_PIENO, Stroke::new(1.5, tema::BLU), StrokeKind::Inside);
    } else {
        painter.rect_filled(r, raggio, FONDO_VUOTO);
        tratteggia(painter, r.shrink(0.5), Stroke::new(1.0, TRATTO));
    }
}

fn tratteggia(painter: &egui::Painter, r: Rect, tratto: Stroke) {
    let punti = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
    painter.extend(egui::Shape::dashed_line(&punti, tratto, 3.0, 2.0));
}

/// Il campo che si scrive, sulla pagina: bianco col bordo blu e il testo di adesso (punti per una
/// password), in alto se ha più righe.
fn dipingi_in_scrittura(painter: &egui::Painter, ctx: &egui::Context, r: Rect, testo: &str, segreto: bool, righe: bool) {
    painter.rect(r, 3.0, tema::FOGLIO, Stroke::new(2.0, tema::BLU), StrokeKind::Outside);
    let mostrato: String = if segreto { testo.chars().map(|_| tascabile_touch::PUNTO_SEGRETO).collect() } else { testo.to_string() };
    let misura = if righe { 11.0 } else { (r.height() * 0.62).clamp(8.0, 22.0) };
    let g = painter.layout_no_wrap(mostrato, Carattere::Testo.font(ctx, misura), tema::INCHIOSTRO);
    let y = if righe { r.top() + 2.0 } else { r.center().y - g.size().y / 2.0 };
    let dentro = painter.with_clip_rect(r.intersect(painter.clip_rect()));
    // Il testo lungo scorre a sinistra, perché si veda la fine, dove si scrive.
    let x = if g.size().x > r.width() - 8.0 { r.right() - 4.0 - g.size().x } else { r.left() + 4.0 };
    dentro.galley(pos2(x, y), g, tema::INCHIOSTRO);
}

/// Dove si prende un campo col dito: il suo posto, allargato fino a `PRESA_MINIMA` per lato.
fn presa(r: Rect) -> Rect {
    Rect::from_center_size(r.center(), vec2(r.width().max(PRESA_MINIMA), r.height().max(PRESA_MINIMA)))
}

/// La presa del posto `r` fra i posti `vicini` (lui compreso, che si salta): allargata come
/// [`presa`], ma mai oltre metà strada verso un vicino. Fra due prese che si coprono egui dà il
/// tocco all'ultima creata, cioè al campo dopo: in una fila di caselle piccole, come una scala del
/// dolore senza zoom, si toccava il 5 e si accendeva il 6 (analisi del 27/10, critico 8).
fn presa_fra(r: Rect, vicini: &[Rect]) -> Rect {
    let mut p = presa(r);
    for o in vicini.iter().filter(|o| **o != r && presa(**o).intersects(presa(r))) {
        if o.min.x >= r.max.x {
            p.max.x = p.max.x.min((r.max.x + o.min.x) / 2.0);
        } else if o.max.x <= r.min.x {
            p.min.x = p.min.x.max((o.max.x + r.min.x) / 2.0);
        } else if o.min.y >= r.max.y {
            p.max.y = p.max.y.min((r.max.y + o.min.y) / 2.0);
        } else if o.max.y <= r.min.y {
            p.min.y = p.min.y.max((o.max.y + r.min.y) / 2.0);
        }
    }
    p
}

/// L'etichetta di un posto di un campo per l'accessibilità: il suggerimento del campo (o il suo
/// genere), con lo stato acceso di un radio.
fn etichetta(c: &Campo, posto: usize, i: usize) -> String {
    let base = c.suggerimento.clone().unwrap_or_else(|| {
        let genere = match c.genere {
            GenereCampo::Testo if c.e_data() => "Data",
            GenereCampo::Testo => "Campo di testo",
            GenereCampo::Casella => "Casella",
            GenereCampo::Radio => "Scelta",
            GenereCampo::Menu => "Menu",
            GenereCampo::Elenco => "Elenco",
            GenereCampo::Firma => "Firma",
            GenereCampo::Altro => "Campo",
        };
        format!("{genere} {}", i + 1)
    });
    match (c.genere, c.posti.get(posto).and_then(|p| p.acceso_come.as_deref())) {
        (GenereCampo::Radio, Some(come)) => format!("{base}: {come}"),
        _ => base,
    }
}

/// Il campo e il posto più vicini al punto `pt` (pt) della pagina `pagina`, entro `bordo` pt.
fn campo_sotto(campi: &[Campo], pagina: usize, pt: Pos2, bordo: f32) -> Option<(String, usize)> {
    let distanza = |r: &[f32; 4]| (r[0] - pt.x).max(pt.x - r[2]).max(0.0).max((r[1] - pt.y).max(pt.y - r[3]).max(0.0));
    campi
        .iter()
        .filter(|c| c.compilabile())
        .flat_map(|c| c.posti.iter().enumerate().map(move |(j, p)| (c, j, p)))
        .filter(|(_, _, p)| p.pagina == pagina && distanza(&p.rettangolo) <= bordo)
        .min_by(|a, b| distanza(&a.2.rettangolo).total_cmp(&distanza(&b.2.rettangolo)))
        .map(|(c, j, _)| (c.nome.clone(), j))
}

/// Il riquadro di un segno che si ingrandisce con la maniglia mossa di `delta`: dall'angolo in
/// alto a sinistra, con le proporzioni di prima.
fn ingrandito(r: Rect, delta: Vec2) -> (Rect, f32) {
    let k = ((r.width() + delta.x) / r.width().max(1.0)).max((r.height() + delta.y) / r.height().max(1.0)).clamp(0.3, 8.0);
    (Rect::from_min_size(r.min, r.size() * k), k)
}

/// La data di oggi nell'ora del telefono.
pub(crate) fn oggi() -> Data {
    let (anno, mese, giorno, _, _) = tascabile_word::salva::ora_locale(adesso() as i64);
    Data { anno, mese, giorno, ..Default::default() }
}

/// Una riga di un foglio di scelta, alta 52 dp: l'icona (radio o casella, blu se scelta) e il
/// testo; la linea sottile sotto, se non è l'ultima.
fn riga_scelta(ui: &mut egui::Ui, icona: &str, testo: &str, accesa: bool, ultima: bool) -> egui::Response {
    let ctx = ui.ctx().clone();
    let largo = ui.available_width();
    let (rect, r) = ui.allocate_exact_size(vec2(largo, 52.0), Sense::click());
    r.widget_info(|| WidgetInfo::selected(WidgetType::RadioButton, true, accesa, testo));
    if r.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, egui::CornerRadius::same(tema::RAGGIO_SEGMENTO), tema::PREMUTO);
    }
    let colore = if accesa { tema::BLU } else { tema::MATITA };
    let carattere = if accesa { Carattere::IconaPiena } else { Carattere::Icona };
    ui.painter().text(
        pos2(rect.left() + 8.0 + tema::ICONA / 2.0, rect.center().y),
        egui::Align2::CENTER_CENTER,
        icona,
        carattere.font(&ctx, tema::ICONA),
        colore,
    );
    let mut job =
        egui::text::LayoutJob::single_section(testo.to_string(), egui::TextFormat::simple(Carattere::Testo.font(&ctx, 16.0), tema::INCHIOSTRO));
    job.wrap = egui::text::TextWrapping::truncate_at_width((largo - 56.0).max(24.0));
    let g = ui.painter().layout_job(job);
    ui.painter().galley(pos2(rect.left() + 48.0, rect.center().y - g.size().y / 2.0), g, tema::INCHIOSTRO);
    if !ultima {
        ui.painter().rect_filled(Rect::from_min_max(pos2(rect.left(), rect.bottom() - 1.0), rect.max), 0.0, tema::FILO);
    }
    r
}

impl TascabileApp {
    /// Per le prove: il testo della scrittura aperta nel modo Compila, se c'è.
    #[doc(hidden)]
    pub fn pdf_in_scrittura(&self) -> Option<String> {
        self.pdf.as_ref()?.compila.scrittura.as_ref().map(|s| s.campo.testo().to_string())
    }

    /// Per le prove a schermo: il campo del modulo col suggerimento `suggerimento` si tocca (il
    /// suo primo posto, o quello `posto`).
    #[doc(hidden)]
    pub fn tocca_campo_pdf_di_prova(&mut self, suggerimento: &str, posto: usize) {
        let nome = self
            .pdf
            .as_ref()
            .and_then(|p| p.motore.stato().campi.iter().find(|c| c.suggerimento.as_deref() == Some(suggerimento)).map(|c| c.nome.clone()));
        if let Some(n) = nome {
            self.tocca_campo_pdf(&n, posto);
        }
    }

    /// Il campo del modulo col nome `nome`, nell'istantanea di adesso.
    fn campo_pdf(&self, nome: &str) -> Option<Campo> {
        self.pdf.as_ref()?.motore.stato().campi.iter().find(|c| c.nome == nome).cloned()
    }

    /// Il segno di Fill & Sign `(pagina, indice)`, nell'istantanea di adesso.
    fn segno_pdf(&self, pagina: usize, indice: usize) -> Option<Segno> {
        self.pdf.as_ref()?.motore.stato().segni.iter().find(|s| s.pagina == pagina && s.indice == indice).cloned()
    }

    /// Le pagine e i campi evidenziati nel modo Compila, sopra il lettore (`pagine`, la sua zona),
    /// coi segni di Fill & Sign e la proposta; `tocco` è il tocco sulla pagina del lettore in
    /// questo frame, fuori dai campi.
    pub(crate) fn mostra_compila(&mut self, ui: &mut egui::Ui, pagine: Rect, sotto: f32, tocco: Option<(usize, Pos2)>) {
        let Some(p) = self.pdf.as_mut() else { return };
        let stato = p.motore.stato();
        let (campi, segni) = (stato.campi.clone(), stato.segni.clone());
        // Ogni pagina ha la sua scala: a zoom 1 ognuna è larga quanto lo schermo.
        let scale: Vec<f32> = (0..p.lettore.pagine()).map(|i| p.lettore.dp_per_pt(i).max(1e-3)).collect();
        let k_di = |pagina: usize| scale.get(pagina).copied().unwrap_or(1.0);
        let ctx = ui.ctx().clone();
        let painter = ui.painter_at(pagine);
        let attivo = match p.compila.scrittura.as_ref().map(|s| &s.bersaglio) {
            Some(Bersaglio::Campo(n)) => Some(n.clone()),
            _ => None,
        };
        let mut azione = None;
        // Una firma da posare dove si tocca: campi e segni non prendono il tocco, che va alla
        // pagina e posa la firma anche sopra un'altra firma o un campo (segnalazione dell'autore
        // del 7/11: «se sovrapposta la firma risulta 1 e non 2»).
        let posa = p.compila.firma.da_toccare.is_some();
        let tocco_o_niente = |senso: Sense| if posa { Sense::hover() } else { senso };
        let posti: Vec<Rect> = campi
            .iter()
            .filter(|c| c.compilabile())
            .flat_map(|c| c.posti.iter())
            .filter_map(|posto| p.lettore.sullo_schermo(posto.pagina, posto.rettangolo))
            .filter(|r| r.intersects(pagine))
            .collect();
        for (i, c) in campi.iter().enumerate().filter(|(_, c)| c.compilabile()) {
            for (j, posto) in c.posti.iter().enumerate() {
                let Some(r) = p.lettore.sullo_schermo(posto.pagina, posto.rettangolo) else { continue };
                if !r.intersects(pagine) {
                    continue;
                }
                if attivo.as_deref() == Some(c.nome.as_str()) {
                    let testo = p.compila.scrittura.as_ref().map(|s| s.campo.testo().to_string()).unwrap_or_default();
                    dipingi_in_scrittura(&painter, &ctx, r, &testo, c.password, c.multilinea);
                } else {
                    let pieno = match c.genere {
                        GenereCampo::Casella | GenereCampo::Radio => posto.acceso,
                        _ => !c.vuoto(),
                    };
                    evidenzia(&painter, r, pieno);
                }
                let risposta = ui.interact(presa_fra(r, &posti).intersect(pagine), Id::new(("campo_pdf", i, j)), tocco_o_niente(Sense::click()));
                let nome = etichetta(c, j, i);
                risposta.widget_info(|| match c.genere {
                    GenereCampo::Casella => WidgetInfo::selected(WidgetType::Checkbox, true, posto.acceso, &nome),
                    GenereCampo::Radio => WidgetInfo::selected(WidgetType::RadioButton, true, posto.acceso, &nome),
                    GenereCampo::Testo => WidgetInfo::labeled(WidgetType::TextEdit, true, &nome),
                    _ => WidgetInfo::labeled(WidgetType::ComboBox, true, &nome),
                });
                if risposta.clicked() {
                    azione = Some(Azione::Campo(c.nome.clone(), j));
                }
            }
        }
        // I campi firma ancora da firmare: «Tocca per firmare» (bozzetto 7), finché sopra non c'è
        // una firma di Fill & Sign.
        for c in campi.iter().filter(|c| c.genere == GenereCampo::Firma && !c.sola_lettura) {
            for (j, posto) in c.posti.iter().enumerate() {
                let [x0, y0, x1, y1] = posto.rettangolo;
                let firmato = segni.iter().any(|s| {
                    let centro = [(s.rettangolo[0] + s.rettangolo[2]) / 2.0, (s.rettangolo[1] + s.rettangolo[3]) / 2.0];
                    s.pagina == posto.pagina
                        && !matches!(s.genere, GenereSegno::Testo { .. })
                        && (x0..=x1).contains(&centro[0])
                        && (y0..=y1).contains(&centro[1])
                });
                let Some(r) = p.lettore.sullo_schermo(posto.pagina, posto.rettangolo) else { continue };
                if firmato || !r.intersects(pagine) {
                    continue;
                }
                crate::pdf_firma::tocca_per_firmare(&painter, &ctx, r);
                let risposta = ui.interact(r.intersect(pagine), Id::new(("firma_pdf", j, &c.nome)), tocco_o_niente(Sense::click()));
                let nome = c.suggerimento.clone().unwrap_or_else(|| crate::pdf_firma::TOCCA_PER_FIRMARE.to_string());
                risposta.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, &nome));
                if risposta.clicked() {
                    azione = Some(Azione::Firma(posto.pagina, posto.rettangolo));
                }
            }
        }
        // I segni di Fill & Sign: tratteggiati; quello scelto col riquadro blu e la maniglia.
        let posti_dei_segni: Vec<Rect> =
            segni.iter().filter_map(|s| p.lettore.sullo_schermo(s.pagina, s.rettangolo)).filter(|r| r.intersects(pagine)).collect();
        for s in segni.iter() {
            let Some(r) = p.lettore.sullo_schermo(s.pagina, s.rettangolo) else { continue };
            if !r.intersects(pagine) {
                continue;
            }
            let chiave = (s.pagina, s.indice);
            // Si trascina solo il segno già scelto: con tutti trascinabili, scorrere la pagina col
            // dito vicino a una spunta la spostava (analisi del 27/10, compila 9).
            let senso = tocco_o_niente(if p.compila.segno == Some(chiave) { Sense::click_and_drag() } else { Sense::click() });
            let risposta = ui.interact(presa_fra(r, &posti_dei_segni).intersect(pagine), Id::new(("segno_pdf", s.pagina, s.indice)), senso);
            risposta.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, nome_del_segno(s)));
            // Il movimento si conta da dove il dito si è appoggiato: egui comincia il trascinamento
            // solo dopo qualche dp, e quelli non vanno persi.
            let origine = ui.input(|i| i.pointer.press_origin());
            let fin_qui = |r: &egui::Response| match (r.interact_pointer_pos(), origine) {
                (Some(a), Some(o)) => a - o,
                _ => Vec2::ZERO,
            };
            if risposta.drag_started() {
                p.compila.segno = Some(chiave);
                p.compila.proposta = None;
                p.compila.trascina = Some(Trascina::Sposta { pagina: s.pagina, indice: s.indice, delta: Vec2::ZERO });
            }
            if risposta.dragged()
                && let Some(Trascina::Sposta { pagina, indice, delta }) = p.compila.trascina.as_mut()
                && (*pagina, *indice) == chiave
            {
                *delta = fin_qui(&risposta);
            }
            if risposta.drag_stopped()
                && let Some(Trascina::Sposta { delta, .. }) = p.compila.trascina.take()
            {
                azione = Some(Azione::Sposta(s.pagina, s.indice, delta));
            }
            if risposta.clicked() {
                azione = Some(Azione::Segno(s.pagina, s.indice));
            }
            let (dove, mosso) = match p.compila.trascina {
                Some(Trascina::Sposta { pagina, indice, delta }) if (pagina, indice) == chiave => (r.translate(delta), delta != Vec2::ZERO),
                Some(Trascina::Misura { pagina, indice, delta }) if (pagina, indice) == chiave => (ingrandito(r, delta).0, delta != Vec2::ZERO),
                _ => (r, false),
            };
            if mosso {
                anteprima_del_segno(&painter, &ctx, s, dove, dove.height() / r.height().max(1.0) * k_di(s.pagina));
            }
            if p.compila.segno == Some(chiave) {
                painter.rect_stroke(dove, 2.0, Stroke::new(1.5, tema::BLU), StrokeKind::Outside);
                let centro = dove.right_bottom() + Vec2::splat(FUORI_DALL_ANGOLO);
                painter.line_segment([dove.right_bottom(), centro], Stroke::new(1.5, tema::BLU));
                painter.circle(centro, MANIGLIA, tema::BLU, Stroke::new(2.0, tema::FOGLIO));
                if s.genere != GenereSegno::Altro {
                    let m = ui.interact(
                        Rect::from_center_size(centro, Vec2::splat(PRESA_MANIGLIA)),
                        Id::new(("maniglia_segno", s.pagina, s.indice)),
                        tocco_o_niente(Sense::drag()),
                    );
                    m.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, "Ingrandisci"));
                    if m.drag_started() {
                        p.compila.trascina = Some(Trascina::Misura { pagina: s.pagina, indice: s.indice, delta: Vec2::ZERO });
                    }
                    if m.dragged()
                        && let Some(Trascina::Misura { delta, .. }) = p.compila.trascina.as_mut()
                    {
                        *delta = fin_qui(&m);
                    }
                    if m.drag_stopped()
                        && let Some(Trascina::Misura { delta, .. }) = p.compila.trascina.take()
                    {
                        azione = Some(Azione::Misura(s.pagina, s.indice, delta));
                    }
                }
            } else {
                tratteggia(&painter, r.expand(1.0), Stroke::new(1.0, TRATTO));
            }
        }
        // Il testo nuovo di Fill & Sign che si scrive, dove si è toccato.
        if let Some(Scrittura { bersaglio: Bersaglio::Nuovo { pagina, punto }, campo, .. }) = &p.compila.scrittura
            && let Some(a) = p.lettore.sullo_schermo(*pagina, [punto[0], punto[1], punto[0], punto[1]])
        {
            let misura = MISURA_TESTO as f32 * k_di(*pagina);
            let (testo, colore) = if campo.testo().is_empty() { (VUOTO_TESTO, tema::MATITA) } else { (campo.testo(), tema::INCHIOSTRO) };
            let g = painter.layout_no_wrap(testo.to_string(), Carattere::Testo.font(&ctx, misura.max(6.0)), colore);
            let r = Rect::from_min_size(pos2(a.min.x, a.min.y - g.size().y / 2.0), g.size()).expand2(vec2(3.0, 2.0));
            painter.rect(r, 2.0, tema::FOGLIO, Stroke::new(1.5, tema::BLU), StrokeKind::Outside);
            painter.galley(r.min + vec2(3.0, 2.0), g, colore);
        }
        if let Some(Scrittura { bersaglio: Bersaglio::Segno { pagina, indice }, campo, .. }) = &p.compila.scrittura
            && let Some(s) = segni.iter().find(|s| s.pagina == *pagina && s.indice == *indice)
            && let Some(r) = p.lettore.sullo_schermo(s.pagina, s.rettangolo)
        {
            let segno = Segno { testo: campo.testo().to_string(), ..s.clone() };
            anteprima_del_segno(&painter, &ctx, &segno, r, k_di(s.pagina));
            painter.rect_stroke(r, 2.0, Stroke::new(1.5, tema::BLU), StrokeKind::Outside);
        }
        // Il punto della proposta.
        if let Some((pagina, pt)) = p.compila.proposta
            && let Some(r) = p.lettore.sullo_schermo(pagina, [pt.x, pt.y, pt.x, pt.y])
        {
            painter.circle(r.center(), 9.0, FONDO_PIENO, Stroke::new(1.5, tema::BLU));
            painter.circle_filled(r.center(), 3.5, tema::BLU);
        }
        if azione.is_none()
            && let Some((pagina, pt)) = tocco
        {
            azione = Some(match campo_sotto(&campi, pagina, pt, BORDO_CAMPO / k_di(pagina)) {
                Some((nome, j)) if !posa => Azione::Campo(nome, j),
                _ => Azione::Vuoto(pagina, pt),
            });
        }
        match azione {
            Some(Azione::Campo(nome, j)) => self.tocca_campo_pdf(&nome, j),
            Some(Azione::Firma(pagina, r)) => {
                diagnosi(format!("pdf: campo firma toccato nella pagina {}", pagina + 1));
                self.apri_firma_pdf(crate::pdf_firma::Dove::Riquadro(pagina, r));
            }
            Some(Azione::Vuoto(pagina, pt)) => self.tocco_vuoto_pdf(pagina, pt),
            Some(Azione::Segno(pagina, indice)) => self.scegli_segno_pdf(pagina, indice),
            Some(Azione::Sposta(pagina, indice, delta)) => self.sposta_segno_pdf(pagina, indice, delta),
            Some(Azione::Misura(pagina, indice, delta)) => self.misura_segno_pdf(pagina, indice, delta),
            None => {}
        }
        let zona = Rect::from_min_max(pagine.min, pos2(pagine.right(), (pagine.bottom() - sotto).max(pagine.top())));
        self.mostra_proposta_pdf(&ctx, zona);
    }

    /// Un tocco su un campo: un testo si scrive, una data apre il suo foglio, caselle e radio
    /// cambiano, menu ed elenchi aprono il foglio delle scelte.
    pub(crate) fn tocca_campo_pdf(&mut self, nome: &str, posto: usize) {
        let Some(c) = self.campo_pdf(nome) else { return };
        if let Some(p) = self.pdf.as_mut() {
            p.compila.proposta = None;
            p.compila.segno = None;
            if p.compila.scrittura.as_ref().is_some_and(|s| s.bersaglio == Bersaglio::Campo(nome.to_string())) {
                return;
            }
        }
        match c.genere {
            GenereCampo::Testo if c.e_data() => {
                self.conferma_scrittura_pdf(true);
                self.apri_data_pdf(&c, &c.testo());
            }
            GenereCampo::Testo => {
                self.conferma_scrittura_pdf(false);
                self.apri_scrittura_pdf(Bersaglio::Campo(c.nome.clone()), &c.testo(), c.password);
            }
            GenereCampo::Casella | GenereCampo::Radio => {
                self.conferma_scrittura_pdf(true);
                match c.valore_dopo_il_tocco(posto) {
                    Some(v) => {
                        diagnosi(if c.genere == GenereCampo::Casella { "pdf: casella toccata" } else { "pdf: radio toccato" });
                        self.modifica_pdf(Modifica::valore(&c, v));
                    }
                    // Il motore accenderebbe un altro posto della stessa casella (analisi del 27/10,
                    // compila 6): niente, e si dice come segnare la risposta.
                    None if c.posto_che_il_motore_non_accende(posto) => {
                        diagnosi("pdf: casella a più stati, posto che il motore non accende");
                        self.avviso_breve.avvisa("Questa casella non si può ancora spuntare da qui: mettici una spunta con Fill & Sign.");
                    }
                    None => diagnosi("pdf: radio già acceso, resta"),
                }
            }
            GenereCampo::Menu | GenereCampo::Elenco => {
                self.conferma_scrittura_pdf(true);
                diagnosi(format!("pdf: scelte aperte, {} voci", c.opzioni.len()));
                if let Some(p) = self.pdf.as_mut() {
                    p.compila.scelta = Some((c.nome.clone(), c.valore.clone()));
                }
                self.fogli_pdf.scelta.apri();
            }
            GenereCampo::Firma | GenereCampo::Altro => {}
        }
    }

    /// Un tocco su un punto della pagina fuori dai campi: chiude la scrittura (e conferma), la
    /// proposta o il segno scelto; se non c'era niente, propone Fill & Sign lì.
    fn tocco_vuoto_pdf(&mut self, pagina: usize, pt: Pos2) {
        if self.tocco_per_la_firma_pdf(pagina, pt) {
            return;
        }
        let Some(p) = self.pdf.as_mut() else { return };
        if p.compila.scrittura.is_some() {
            self.conferma_scrittura_pdf(true);
            return;
        }
        if p.compila.proposta.take().is_some() | p.compila.segno.take().is_some() {
            return;
        }
        p.compila.proposta = Some((pagina, pt));
        diagnosi(format!("pdf: fill & sign proposto nella pagina {}", pagina + 1));
    }

    fn scegli_segno_pdf(&mut self, pagina: usize, indice: usize) {
        self.conferma_scrittura_pdf(true);
        if let Some(p) = self.pdf.as_mut() {
            p.compila.proposta = None;
            p.compila.segno = Some((pagina, indice));
        }
    }

    fn sposta_segno_pdf(&mut self, pagina: usize, indice: usize, delta: Vec2) {
        let Some(p) = self.pdf.as_ref() else { return };
        let k = p.lettore.dp_per_pt(pagina).max(1e-3);
        let Some(info) = p.motore.stato().geometrie.get(pagina).cloned() else { return };
        if delta.length() < 2.0 {
            return;
        }
        let Some(s) = self.segno_pdf(pagina, indice) else { return };
        diagnosi("pdf: segno spostato");
        self.modifica_pdf(Modifica::sposta_segno(&s, &info, delta.x / k, delta.y / k));
    }

    fn misura_segno_pdf(&mut self, pagina: usize, indice: usize, delta: Vec2) {
        let Some(p) = self.pdf.as_ref() else { return };
        let Some(info) = p.motore.stato().geometrie.get(pagina).cloned() else { return };
        let Some(s) = self.segno_pdf(pagina, indice) else { return };
        let Some(r) = p.lettore.sullo_schermo(pagina, s.rettangolo) else { return };
        let (_, scala) = ingrandito(r, delta);
        if (scala - 1.0).abs() < 0.02 {
            return;
        }
        let [x0, y0, x1, y1] = s.rettangolo;
        let nuovo = [x0, y0, x0 + (x1 - x0) * scala, y0 + (y1 - y0) * scala];
        if let Some(m) = Modifica::misura_segno(&s, &info, nuovo) {
            diagnosi(format!("pdf: segno ingrandito di {:.0} %", (scala - 1.0) * 100.0));
            self.modifica_pdf(m);
        }
    }

    /// La barra scura sopra il punto toccato (Testo, Spunta, Croce, Data) o sopra il segno scelto
    /// (Riscrivi per un testo, Togli).
    fn mostra_proposta_pdf(&mut self, ctx: &egui::Context, zona: Rect) {
        let Some(p) = self.pdf.as_ref() else { return };
        if let Some((pagina, pt)) = p.compila.proposta {
            let Some(r) = p.lettore.sullo_schermo(pagina, [pt.x, pt.y, pt.x, pt.y]) else { return };
            let scelta = BarraContesto::new()
                .azione(icone::TEXT_FIELDS, TESTO)
                .azione(icone::CHECK, SPUNTA)
                .azione(icone::CLOSE, CROCE)
                .azione(icone::CALENDAR_TODAY, DATA)
                .azione(icone::SIGNATURE, crate::pdf_firma::FIRMA)
                .mostra_in_primo_piano(ctx, Id::new("proposta_fill_sign"), r.expand(10.0), zona);
            if let Some(i) = scelta {
                self.posa_segno_pdf(pagina, pt, i);
            }
            return;
        }
        let Some((pagina, indice)) = p.compila.segno else { return };
        if p.compila.trascina.is_some() || p.compila.scrittura.is_some() {
            return;
        }
        let Some(s) = self.segno_pdf(pagina, indice) else { return };
        let Some(r) = p.lettore.sullo_schermo(pagina, s.rettangolo) else { return };
        let testo = matches!(s.genere, GenereSegno::Testo { .. });
        let mut barra = BarraContesto::new();
        if testo {
            barra = barra.azione(icone::EDIT, RISCRIVI);
        }
        let scelta =
            barra.azione(icone::DELETE, TOGLI).mostra_in_primo_piano(ctx, Id::new("segno_scelto"), r.expand(MANIGLIA + FUORI_DALL_ANGOLO), zona);
        match scelta {
            Some(0) if testo => {
                diagnosi("pdf: segno da riscrivere");
                self.apri_scrittura_pdf(Bersaglio::Segno { pagina, indice }, &s.testo, false);
            }
            Some(_) => {
                diagnosi("pdf: segno tolto");
                if let Some(p) = self.pdf.as_mut() {
                    p.compila.segno = None;
                }
                self.modifica_pdf(Modifica::togli_segno(&s));
            }
            None => {}
        }
    }

    /// Una scelta della proposta: un testo da scrivere, una spunta, una croce, la data di oggi.
    fn posa_segno_pdf(&mut self, pagina: usize, pt: Pos2, scelta: usize) {
        if scelta == 4 {
            diagnosi("pdf: fill & sign, firma");
            if let Some(p) = self.pdf.as_mut() {
                p.compila.proposta = None;
            }
            self.apri_firma_pdf(crate::pdf_firma::Dove::Punto(pagina, pt));
            return;
        }
        let autore = self.impostazioni.autore.clone();
        let Some(p) = self.pdf.as_mut() else { return };
        p.compila.proposta = None;
        let Some(info) = p.motore.stato().geometrie.get(pagina).cloned() else { return };
        let punto = [pt.x, pt.y];
        let modifica = match scelta {
            0 => {
                diagnosi("pdf: fill & sign, testo");
                self.apri_scrittura_pdf(Bersaglio::Nuovo { pagina, punto }, "", false);
                return;
            }
            1 => {
                diagnosi("pdf: fill & sign, spunta");
                Modifica::segno(pagina, &info, punto, NuovoSegno::Spunta, &autore)
            }
            2 => {
                diagnosi("pdf: fill & sign, croce");
                Modifica::segno(pagina, &info, punto, NuovoSegno::Croce, &autore)
            }
            3 => {
                diagnosi("pdf: fill & sign, data di oggi");
                Modifica::testo_libero(pagina, &info, punto, &oggi().all_italiana(), MISURA_TESTO, &autore)
            }
            _ => None,
        };
        if let Some(m) = modifica {
            p.compila.da_scegliere = Some(pagina);
            self.modifica_pdf(m);
        }
    }

    /// Comincia a scrivere `bersaglio` con la tastiera vera, partendo da `testo`: la ricerca si
    /// chiude, il campo prende la tastiera (quella delle password per un campo password) e si
    /// porta sopra la barra.
    pub(crate) fn apri_scrittura_pdf(&mut self, bersaglio: Bersaglio, testo: &str, segreto: bool) {
        self.chiudi_ricerca_pdf();
        let Some(p) = self.pdf.as_mut() else { return };
        // Un campo appena confermato, col motore che non ha ancora risposto: il testo mandato.
        if !p.motore.modifica_in_corso() {
            p.compila.in_volo.clear();
        }
        let testo = match &bersaglio {
            Bersaglio::Campo(nome) => p.compila.in_volo.get(nome).cloned().unwrap_or_else(|| testo.to_string()),
            _ => testo.to_string(),
        };
        let testo = testo.as_str();
        p.compila.proposta = None;
        if matches!(bersaglio, Bersaglio::Campo(_)) {
            p.compila.segno = None;
        }
        // Un campo a più righe del modulo e il testo di una nota tengono gli a capo, e Invio non
        // li conferma: si conferma con «Fatto».
        let a_piu_righe = match &bersaglio {
            Bersaglio::Campo(nome) => p.motore.stato().campi.iter().any(|c| &c.nome == nome && c.multilinea),
            Bersaglio::NotaNuova { .. } | Bersaglio::Annotazione { .. } => true,
            _ => false,
        };
        let campo = if segreto {
            CampoTesto::segreto()
        } else if a_piu_righe {
            CampoTesto::a_piu_righe()
        } else {
            CampoTesto::new()
        }
        .con_testo(testo);
        diagnosi(match &bersaglio {
            Bersaglio::Campo(_) if segreto => "pdf: scrittura di un campo password",
            Bersaglio::Campo(_) => "pdf: scrittura di un campo di testo",
            Bersaglio::Nuovo { .. } => "pdf: scrittura di un testo nuovo",
            Bersaglio::Segno { .. } => "pdf: scrittura di un testo posato",
            Bersaglio::Oggetto(_) => "pdf: scrittura di un testo da modificare",
            Bersaglio::TestoNuovo { .. } => "pdf: scrittura di un testo da aggiungere",
            Bersaglio::Filigrana => "pdf: scrittura della filigrana",
            Bersaglio::Intestazione => "pdf: scrittura dell'intestazione",
            Bersaglio::NotaNuova { .. } => "pdf: scrittura di una nota",
            Bersaglio::TestoLibero { .. } => "pdf: scrittura di un testo libero",
            Bersaglio::Annotazione { .. } => "pdf: scrittura del testo di un'annotazione",
            Bersaglio::Indirizzo => "pdf: scrittura dell'indirizzo di un link",
        });
        p.compila.scrittura = Some(Scrittura { bersaglio, campo, confermato: testo.to_string(), visto_con: None });
        self.fuoco_al_campo_pdf();
    }

    /// Il testo scritto va al motore, se è cambiato: il valore del campo (nei limiti del campo),
    /// un testo nuovo di Fill & Sign, il testo di un segno (vuoto, lo toglie). Con `chiudi` la
    /// scrittura finisce e la tastiera si chiude; senza resta aperta (il campo dopo, l'app che
    /// perde il fuoco).
    pub(crate) fn conferma_scrittura_pdf(&mut self, chiudi: bool) {
        let Some(s) = self.pdf.as_ref().and_then(|p| p.compila.scrittura.as_ref()) else { return };
        let testo = s.campo.testo().to_string();
        let bersaglio = s.bersaglio.clone();
        // Un testo di Fill & Sign confermato è un segno posato: la sua scrittura finisce sempre
        // (scriverlo ancora ne poserebbe un altro).
        let chiudi = chiudi || !matches!(bersaglio, Bersaglio::Campo(_));
        // Un paragrafo allargato con la presa è cambiato anche col testo di prima.
        let allargato = matches!(bersaglio, Bersaglio::Oggetto(_)) && self.pdf.as_ref().and_then(|p| p.modifica.allargata).is_some_and(|k| (k - 1.0).abs() > 0.01);
        let cambiato = testo != s.confermato || allargato;
        if cambiato {
            let autore = self.impostazioni.autore.clone();
            match &bersaglio {
                Bersaglio::Campo(nome) => {
                    if let Some(c) = self.campo_pdf(nome) {
                        let valore = match c.genere {
                            GenereCampo::Menu if testo.is_empty() => FieldValue::Choice(Vec::new()),
                            GenereCampo::Menu => FieldValue::Choice(vec![testo.clone()]),
                            _ => FieldValue::Text(c.testo_nei_limiti(&testo)),
                        };
                        diagnosi("pdf: campo confermato");
                        self.modifica_pdf(Modifica::valore(&c, valore));
                        if let Some(p) = self.pdf.as_mut() {
                            p.compila.in_volo.insert(nome.clone(), testo.clone());
                        }
                        // Il valore resta giusto, ma senza un carattere del modulo che abbia la
                        // lettera il motore la disegna «?» (analisi del 27/10, critico 12).
                        if let Some(ch) = tascabile_pdf::lettera_fuori_da_winansi(&testo) {
                            self.avviso_breve.avvisa(format!("«{ch}» non c'è nel carattere del modulo: chi apre il PDF può vedere «?»."));
                        }
                    }
                }
                // Un testo di Fill & Sign avrebbe i «?» disegnati per sempre: non si posa, e la
                // scrittura resta aperta per correggerlo.
                Bersaglio::Nuovo { .. } | Bersaglio::Segno { .. }
                    if !testo.trim().is_empty()
                        && let Some(ch) = tascabile_pdf::lettera_fuori_da_winansi(&testo) =>
                {
                    diagnosi("pdf: fill & sign, testo con un carattere che non si scrive");
                    self.errore = Some(Errore {
                        titolo: "Testo non posato",
                        testo: format!("«{ch}» non c'è nel carattere di Fill & Sign: nel PDF diventerebbe «?». Le lettere italiane e «€» sì."),
                    });
                    return;
                }
                Bersaglio::Nuovo { pagina, punto } if !testo.trim().is_empty() => {
                    let info = self.pdf.as_ref().and_then(|p| p.motore.stato().geometrie.get(*pagina).cloned());
                    if let Some(m) = info.and_then(|i| Modifica::testo_libero(*pagina, &i, *punto, &testo, MISURA_TESTO, &autore)) {
                        diagnosi("pdf: fill & sign, testo posato");
                        if let Some(p) = self.pdf.as_mut() {
                            p.compila.da_scegliere = Some(*pagina);
                        }
                        self.modifica_pdf(m);
                    }
                }
                Bersaglio::Nuovo { .. } => {}
                Bersaglio::Segno { pagina, indice } => {
                    if let Some(seg) = self.segno_pdf(*pagina, *indice)
                        && let Some(info) = self.pdf.as_ref().and_then(|p| p.motore.stato().geometrie.get(seg.pagina).cloned())
                    {
                        let m = if testo.trim().is_empty() { Modifica::togli_segno(&seg) } else { Modifica::testo_del_segno(&seg, &info, &testo) };
                        diagnosi("pdf: testo del segno confermato");
                        self.modifica_pdf(m);
                    }
                }
                Bersaglio::TestoNuovo { .. } | Bersaglio::NotaNuova { .. } | Bersaglio::TestoLibero { .. } if testo.trim().is_empty() => {}
                altro @ (Bersaglio::NotaNuova { .. } | Bersaglio::TestoLibero { .. } | Bersaglio::Annotazione { .. }) => {
                    self.conferma_testo_annota(altro, &testo)
                }
                altro => self.conferma_testo_modifica(altro, &testo),
            }
        }
        let Some(p) = self.pdf.as_mut() else { return };
        if let Some(s) = p.compila.scrittura.as_mut() {
            s.confermato = testo;
        }
        if chiudi {
            // Prima il fuoco (il campo lo lascia, e le versioni della tastiera continuano dalla
            // sua), poi via la scrittura.
            let campo = self.fuoco == Fuoco::Campo;
            self.fuoco_al_documento();
            if let Some(p) = self.pdf.as_mut() {
                p.compila.scrittura = None;
            }
            if campo && self.piattaforma.keyboard_visible() {
                self.piattaforma.hide_keyboard();
            }
        }
    }

    /// «Campo dopo» o «Campo prima» (Invio vale il dopo): il valore si conferma e si scrive il
    /// campo di testo dopo (o prima) nell'ordine del modulo; dopo l'ultimo la scrittura finisce.
    pub(crate) fn campo_vicino_pdf(&mut self, avanti: bool) {
        let Some(p) = self.pdf.as_ref() else { return };
        let Some(Scrittura { bersaglio: Bersaglio::Campo(nome), .. }) = p.compila.scrittura.as_ref() else {
            self.conferma_scrittura_pdf(true);
            return;
        };
        let scrivibili: Vec<Campo> = p.motore.stato().campi.iter().filter(|c| c.si_scrive() && c.genere == GenereCampo::Testo).cloned().collect();
        let qui = scrivibili.iter().position(|c| &c.nome == nome);
        let vicino = match (qui, avanti) {
            (Some(i), true) => scrivibili.get(i + 1),
            (Some(i), false) => i.checked_sub(1).and_then(|i| scrivibili.get(i)),
            (None, _) => None,
        }
        .cloned();
        match vicino {
            Some(c) => {
                // Anche una data si scrive con la tastiera, per non perdere il filo; il suo foglio
                // è a un tocco, col calendario nella barra.
                self.conferma_scrittura_pdf(false);
                diagnosi(if avanti { "pdf: campo dopo" } else { "pdf: campo prima" });
                self.apri_scrittura_pdf(Bersaglio::Campo(c.nome.clone()), &c.testo(), c.password);
            }
            None => self.conferma_scrittura_pdf(true),
        }
    }

    /// Ci sono un campo prima e un campo dopo da scrivere.
    fn vicini_pdf(&self) -> (bool, bool) {
        let Some(p) = self.pdf.as_ref() else { return (false, false) };
        let Some(Scrittura { bersaglio: Bersaglio::Campo(nome), .. }) = p.compila.scrittura.as_ref() else { return (false, false) };
        let scrivibili: Vec<&Campo> = p.motore.stato().campi.iter().filter(|c| c.si_scrive() && c.genere == GenereCampo::Testo).collect();
        match scrivibili.iter().position(|c| &c.nome == nome) {
            Some(i) => (i > 0, i + 1 < scrivibili.len()),
            None => (false, false),
        }
    }

    /// Il campo della scrittura sopra la barra, se non si vede: all'inizio e quando la tastiera
    /// cambia altezza.
    pub(crate) fn tieni_in_vista_pdf(&mut self) {
        let tastiera = self.piattaforma.keyboard_height();
        let Some(p) = self.pdf.as_mut() else { return };
        let Some(s) = p.compila.scrittura.as_mut() else { return };
        if s.visto_con == Some(tastiera) {
            return;
        }
        s.visto_con = Some(tastiera);
        let dove = match &s.bersaglio {
            Bersaglio::Campo(nome) => {
                p.motore.stato().campi.iter().find(|c| &c.nome == nome).and_then(|c| c.posti.first()).map(|x| (x.pagina, x.rettangolo))
            }
            Bersaglio::Nuovo { pagina, punto } => Some((*pagina, [punto[0], punto[1] - 10.0, punto[0] + 120.0, punto[1] + 10.0])),
            Bersaglio::Segno { pagina, indice } => {
                p.motore.stato().segni.iter().find(|x| x.pagina == *pagina && x.indice == *indice).map(|x| (x.pagina, x.rettangolo))
            }
            Bersaglio::Oggetto(c) => p.modifica.oggetto(*c).map(|o| (o.pagina, o.rettangolo)),
            Bersaglio::TestoNuovo { pagina, punto } => Some((*pagina, [punto[0], punto[1], punto[0] + 120.0, punto[1] + 20.0])),
            Bersaglio::NotaNuova { pagina, punto } => Some((*pagina, [punto[0] - 10.0, punto[1] - 10.0, punto[0] + 10.0, punto[1] + 10.0])),
            Bersaglio::TestoLibero { pagina, punto } => Some((*pagina, [punto[0], punto[1], punto[0] + 120.0, punto[1] + 20.0])),
            Bersaglio::Annotazione { pagina, indice } => {
                p.motore.stato().annotazioni.iter().find(|a| a.chiave() == (*pagina, *indice)).map(|a| (a.pagina, a.rettangolo))
            }
            Bersaglio::Filigrana | Bersaglio::Intestazione | Bersaglio::Indirizzo => None,
        };
        if let Some((pagina, r)) = dove {
            p.lettore.mostra_rettangolo(pagina, r);
        }
    }

    /// Il posto della barra della scrittura: sulla tastiera se è aperta, altrimenti in fondo.
    pub(crate) fn posto_barra_scrittura(zona: Rect, tastiera: bool) -> Rect {
        if tastiera {
            let m = tema::SOPRA_LA_TASTIERA;
            Rect::from_min_max(pos2(zona.left() + m, zona.bottom() - m - BARRA), pos2(zona.right() - m, zona.bottom() - m))
        } else {
            BarraFlottante::posto(zona)
        }
    }

    /// La barra della scrittura (sulla tastiera, come la barra di formato del bozzetto 3): campo
    /// prima e campo dopo (per i campi del modulo), il campo di testo vero, il calendario per una
    /// data, «Fatto».
    pub(crate) fn mostra_barra_scrittura(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let (prima, dopo) = self.vicini_pdf();
        let data = match self.pdf.as_ref().and_then(|p| p.compila.scrittura.as_ref()).map(|s| s.bersaglio.clone()) {
            Some(Bersaglio::Campo(nome)) => self.campo_pdf(&nome).filter(Campo::e_data),
            _ => None,
        };
        let del_modulo = matches!(self.pdf.as_ref().and_then(|p| p.compila.scrittura.as_ref()).map(|s| &s.bersaglio), Some(Bersaglio::Campo(_)));
        let Some(s) = self.pdf.as_ref().and_then(|p| p.compila.scrittura.as_ref()) else { return };
        let mut ui = ui.new_child(UiBuilder::new().max_rect(rect).id_salt("barra della scrittura"));
        let raggio = egui::CornerRadius::same(tema::RAGGIO_FORMATO_SULLA_TASTIERA);
        tema::dipingi_ombra(ui.painter(), rect, raggio, &tema::OMBRA_FLOTTANTE);
        ui.painter().rect_filled(rect, raggio, tema::FOGLIO);
        ui.allocate_rect(rect, Sense::click());
        let quadro = |x: f32| Rect::from_min_size(pos2(x, rect.center().y - tema::PULSANTE / 2.0), Vec2::splat(tema::PULSANTE));
        let mut sinistra = rect.left() + 6.0;
        let (mut su, mut giu) = (false, false);
        if del_modulo {
            su = ui.put(quadro(sinistra), PulsanteIcona::new(icone::KEYBOARD_ARROW_UP, CAMPO_PRIMA).spento(!prima)).clicked();
            sinistra += tema::PULSANTE;
            giu = ui.put(quadro(sinistra), PulsanteIcona::new(icone::KEYBOARD_ARROW_DOWN, CAMPO_DOPO).spento(!dopo)).clicked();
            sinistra += tema::PULSANTE + 4.0;
        }
        let mut destra = rect.right() - 6.0 - tema::PULSANTE;
        let fatto = ui.put(quadro(destra), PulsanteIcona::new(icone::CHECK, FATTO).colore(tema::BLU)).clicked();
        let mut calendario = false;
        if data.is_some() {
            destra -= tema::PULSANTE;
            calendario = ui.put(quadro(destra), PulsanteIcona::new(icone::CALENDAR_TODAY, SCEGLI_LA_DATA)).clicked();
        }
        let vuoto = match &s.bersaglio {
            Bersaglio::Campo(_) => "",
            _ => VUOTO_TESTO,
        };
        let dentro = Rect::from_min_max(pos2(sinistra, rect.center().y - 24.0), pos2(destra - 4.0, rect.center().y + 24.0));
        let tocco_campo = ui
            .scope_builder(UiBuilder::new().max_rect(dentro), |ui| {
                if s.campo.e_segreto() { s.campo.mostra_segreto(ui, vuoto, false).campo.clicked() } else { s.campo.mostra(ui, vuoto).clicked() }
            })
            .inner;
        // Il calendario parte dalla data appena scritta, non da quella del motore: la conferma
        // gliela manda adesso, e il motore non l'ha ancora (analisi del 27/10, critico 10).
        let scritto = calendario.then(|| s.campo.testo().to_string());
        if tocco_campo {
            self.fuoco_al_campo_pdf();
        }
        if su {
            self.campo_vicino_pdf(false);
        } else if giu {
            self.campo_vicino_pdf(true);
        } else if fatto {
            self.conferma_scrittura_pdf(true);
        } else if let (Some(c), Some(scritto)) = (data, scritto) {
            self.conferma_scrittura_pdf(true);
            self.apri_data_pdf(&c, &scritto);
        }
    }

    /// Il foglio della data del campo `c`, dalla data `testo` (o da oggi).
    fn apri_data_pdf(&mut self, c: &Campo, testo: &str) {
        let formato = c.data.clone().unwrap_or_else(|| "dd/mm/yyyy".to_string());
        let data = Data::leggi(testo, &formato).unwrap_or_else(oggi);
        diagnosi("pdf: foglio della data aperto");
        if let Some(p) = self.pdf.as_mut() {
            p.compila.data = Some((c.nome.clone(), data));
        }
        self.fogli_pdf.data.apri();
    }

    /// I fogli del modo Compila: le scelte di un menu o di un elenco e la data. Vanno chiamati a
    /// ogni frame.
    pub(crate) fn mostra_fogli_compila(&mut self, ctx: &egui::Context) {
        self.mostra_foglio_scelta(ctx);
        self.mostra_foglio_data(ctx);
    }

    fn mostra_foglio_scelta(&mut self, ctx: &egui::Context) {
        let aperto = self.pdf.as_ref().and_then(|p| p.compila.scelta.clone());
        let campo = aperto.as_ref().and_then(|(nome, _)| self.campo_pdf(nome));
        let titolo = campo.as_ref().and_then(|c| c.suggerimento.clone()).unwrap_or_else(|| "Scegli".to_string());
        enum Tocco {
            Voce(usize),
            Scrivi,
            Fatto,
        }
        let tocco = self
            .fogli_pdf
            .scelta
            .mostra(ctx, &titolo, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let (Some(c), Some((_, scelti))) = (campo.as_ref(), aperto.as_ref()) else { return None };
                let mut tocco = None;
                let n = c.opzioni.len();
                for (i, (esportato, mostrato)) in c.opzioni.iter().enumerate() {
                    let acceso = scelti.contains(esportato);
                    let icona = match (c.multipla, acceso) {
                        (true, true) => icone::CHECK_BOX,
                        (true, false) => icone::CHECK_BOX_OUTLINE_BLANK,
                        (false, true) => icone::RADIO_BUTTON_CHECKED,
                        (false, false) => icone::RADIO_BUTTON_UNCHECKED,
                    };
                    let testo = if mostrato.trim().is_empty() { esportato } else { mostrato };
                    if riga_scelta(ui, icona, testo, acceso, i + 1 == n && !c.scrivibile).clicked() {
                        tocco = Some(Tocco::Voce(i));
                    }
                }
                if c.scrivibile && riga_scelta(ui, icone::EDIT, SCRIVI_UN_ALTRO, false, true).clicked() {
                    tocco = Some(Tocco::Scrivi);
                }
                if c.multipla {
                    ui.add_space(12.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(Pulsante::new(FATTO, Peso::Principale).larghezza(120.0)).clicked() {
                            tocco = Some(Tocco::Fatto);
                        }
                    });
                    ui.add_space(6.0);
                }
                tocco
            })
            .flatten();
        if !self.fogli_pdf.scelta.aperto()
            && let Some(p) = self.pdf.as_mut()
        {
            p.compila.scelta = None;
        }
        let (Some(c), Some(tocco)) = (campo, tocco) else { return };
        match tocco {
            Tocco::Voce(i) if c.multipla => {
                if let Some((_, scelti)) = self.pdf.as_mut().and_then(|p| p.compila.scelta.as_mut()) {
                    let e = c.opzioni[i].0.clone();
                    match scelti.iter().position(|s| *s == e) {
                        Some(j) => {
                            scelti.remove(j);
                        }
                        None => scelti.push(e),
                    }
                }
            }
            Tocco::Voce(i) => {
                self.fogli_pdf.scelta.chiudi();
                if let Some(p) = self.pdf.as_mut() {
                    p.compila.scelta = None;
                }
                let scelto = vec![c.opzioni[i].0.clone()];
                if scelto != c.valore {
                    diagnosi(format!("pdf: scelta {} di {}", i + 1, c.opzioni.len()));
                    self.modifica_pdf(Modifica::valore(&c, FieldValue::Choice(scelto)));
                }
            }
            Tocco::Fatto => {
                self.fogli_pdf.scelta.chiudi();
                let scelti = self.pdf.as_mut().and_then(|p| p.compila.scelta.take()).map(|(_, s)| s).unwrap_or_default();
                // Nell'ordine delle scelte del campo, come le scrive Acrobat.
                let in_ordine: Vec<String> = c.opzioni.iter().map(|(e, _)| e.clone()).filter(|e| scelti.contains(e)).collect();
                if in_ordine != c.valore {
                    diagnosi(format!("pdf: {} scelte di {}", in_ordine.len(), c.opzioni.len()));
                    self.modifica_pdf(Modifica::valore(&c, FieldValue::Choice(in_ordine)));
                }
            }
            Tocco::Scrivi => {
                self.fogli_pdf.scelta.chiudi();
                if let Some(p) = self.pdf.as_mut() {
                    p.compila.scelta = None;
                }
                self.apri_scrittura_pdf(Bersaglio::Campo(c.nome.clone()), &c.mostrato(), false);
            }
        }
    }

    fn mostra_foglio_data(&mut self, ctx: &egui::Context) {
        let aperto = self.pdf.as_ref().and_then(|p| p.compila.data.clone());
        let campo = aperto.as_ref().and_then(|(nome, _)| self.campo_pdf(nome));
        let titolo = campo.as_ref().and_then(|c| c.suggerimento.clone()).unwrap_or_else(|| DATA.to_string());
        let formato = campo.as_ref().and_then(|c| c.data.clone()).unwrap_or_else(|| "dd/mm/yyyy".to_string());
        enum Tocco {
            Sposta(ParteData, bool),
            Oggi,
            Scrivi,
            Svuota,
            Metti,
        }
        let tocco = self
            .fogli_pdf
            .data
            .mostra(ctx, &titolo, |ui| {
                let (Some(c), Some((_, d))) = (campo.as_ref(), aperto.as_ref()) else { return None };
                let mut tocco = None;
                ui.add_space(4.0);
                ui.label(RichText::new(d.scritta(&formato)).font(Carattere::Grassetto.font(ui.ctx(), 26.0)).color(tema::INCHIOSTRO));
                ui.add_space(12.0);
                for (parte, nome, valore) in [
                    (ParteData::Giorno, "Giorno", d.giorno.to_string()),
                    (ParteData::Mese, "Mese", d.mese.to_string()),
                    (ParteData::Anno, "Anno", d.anno.to_string()),
                ] {
                    ui.horizontal(|ui| {
                        ui.set_min_height(tema::PULSANTE);
                        ui.label(RichText::new(nome).font(Carattere::Testo.font(ui.ctx(), 16.0)).color(tema::INCHIOSTRO));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let meno = format!("{nome} prima");
                            let piu = format!("{nome} dopo");
                            match Passo::new(&valore, &meno, &piu).mostra(ui) {
                                Some(Verso::Meno) => tocco = Some(Tocco::Sposta(parte, false)),
                                Some(Verso::Piu) => tocco = Some(Tocco::Sposta(parte, true)),
                                None => {}
                            }
                        });
                    });
                    ui.add_space(6.0);
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if ui.add(Pulsante::new(OGGI, Peso::Testo).icona(icone::TODAY)).clicked() {
                        tocco = Some(Tocco::Oggi);
                    }
                    if ui.add(Pulsante::new(SCRIVI_A_MANO, Peso::Testo).icona(icone::KEYBOARD)).clicked() {
                        tocco = Some(Tocco::Scrivi);
                    }
                    if !c.vuoto() && ui.add(Pulsante::new(SVUOTA, Peso::Testo)).clicked() {
                        tocco = Some(Tocco::Svuota);
                    }
                });
                ui.add_space(10.0);
                if ui.add(Pulsante::new(METTI_LA_DATA, Peso::Principale).larghezza(ui.available_width())).clicked() {
                    tocco = Some(Tocco::Metti);
                }
                ui.add_space(6.0);
                tocco
            })
            .flatten();
        if !self.fogli_pdf.data.aperto()
            && let Some(p) = self.pdf.as_mut()
        {
            p.compila.data = None;
        }
        let (Some(c), Some(tocco)) = (campo, tocco) else { return };
        let data = self.pdf.as_ref().and_then(|p| p.compila.data.as_ref()).map(|(_, d)| *d).unwrap_or_else(oggi);
        let metti = |app: &mut TascabileApp, testo: String| {
            app.fogli_pdf.data.chiudi();
            if let Some(p) = app.pdf.as_mut() {
                p.compila.data = None;
            }
            if testo != c.testo() {
                diagnosi("pdf: data messa");
                app.modifica_pdf(Modifica::valore(&c, FieldValue::Text(testo)));
            }
        };
        match tocco {
            Tocco::Sposta(parte, avanti) => {
                if let Some((_, d)) = self.pdf.as_mut().and_then(|p| p.compila.data.as_mut()) {
                    *d = d.sposta(parte, avanti);
                }
            }
            // Il giorno di oggi; l'ora resta quella del campo.
            Tocco::Oggi => metti(self, Data { ore: data.ore, minuti: data.minuti, secondi: data.secondi, ..oggi() }.scritta(&formato)),
            Tocco::Metti => metti(self, data.scritta(&formato)),
            Tocco::Svuota => metti(self, String::new()),
            Tocco::Scrivi => {
                self.fogli_pdf.data.chiudi();
                if let Some(p) = self.pdf.as_mut() {
                    p.compila.data = None;
                }
                self.apri_scrittura_pdf(Bersaglio::Campo(c.nome.clone()), &c.testo(), false);
            }
        }
    }

    /// Il modo Compila acceso: se il modulo ha calcoli veri e gli script sono spenti, lo si dice
    /// una volta per documento.
    pub(crate) fn controlla_i_calcoli_pdf(&mut self) {
        let Some(p) = self.pdf.as_mut() else { return };
        let stato = p.motore.stato();
        if p.modo == ModoPdf::Compila && stato.calcoli && !stato.javascript && !p.compila.calcoli_chiesti {
            p.compila.calcoli_chiesti = true;
            p.compila.chiedi_calcoli = true;
            diagnosi("pdf: il modulo ha calcoli, si propone di accenderli");
        }
    }

    /// Il dialogo dei calcoli del modulo: «Non ora» o «Accendi i calcoli».
    pub(crate) fn mostra_calcoli_pdf(&mut self, ctx: &egui::Context) {
        if !self.pdf.as_ref().is_some_and(|p| p.compila.chiedi_calcoli) {
            return;
        }
        let risposta = Dialogo::new(Id::new("calcoli_pdf"), CALCOLI)
            .testo(TESTO_CALCOLI)
            // «Non ora» è la scelta principale: con i calcoli accesi uno script del modulo può
            // tenere occupato il motore a lungo o riempire la memoria (revisione del 31/10, punto 8).
            .pulsante(NON_ORA, Peso::Principale)
            .pulsante(ACCENDI_I_CALCOLI, Peso::Testo)
            .mostra(ctx);
        let Some(risposta) = risposta else { return };
        let Some(p) = self.pdf.as_mut() else { return };
        p.compila.chiedi_calcoli = false;
        match risposta {
            RispostaDialogo::Pulsante(1) => {
                diagnosi("pdf: calcoli, si accendono");
                if !p.motore.javascript(true) {
                    self.motore_fermo();
                }
            }
            _ => diagnosi("pdf: calcoli, non ora"),
        }
    }

    /// Dopo una risposta del motore: il segno appena posato diventa lo scelto; annullando o
    /// ripetendo gli indici possono cambiare e la scelta si lascia.
    pub(crate) fn compila_dopo_il_motore(&mut self, applicata: Option<bool>) {
        let Some(p) = self.pdf.as_mut() else { return };
        match applicata {
            Some(true) => {
                if let Some(pagina) = p.compila.da_scegliere.take() {
                    p.compila.segno =
                        p.motore.stato().segni.iter().filter(|s| s.pagina == pagina).max_by_key(|s| s.indice).map(|s| (s.pagina, s.indice));
                }
            }
            Some(false) => p.compila.da_scegliere = None,
            None => {
                p.compila.segno = None;
                p.compila.trascina = None;
            }
        }
    }
}

/// Il nome di un segno per l'accessibilità: il suo genere, mai il testo.
fn nome_del_segno(s: &Segno) -> &'static str {
    match s.genere {
        GenereSegno::Testo { .. } => "Testo posato",
        GenereSegno::Segno => "Segno posato",
        GenereSegno::Firma => "Firma posata",
        GenereSegno::Altro => "Segno posato",
    }
}

/// Un segno mentre si sposta o si ingrandisce: il fondo bianco velato e, per un testo, il testo
/// alla misura nuova (`dp_per_pt` dp per pt).
fn anteprima_del_segno(painter: &egui::Painter, ctx: &egui::Context, s: &Segno, dove: Rect, dp_per_pt: f32) {
    painter.rect_filled(dove, 2.0, Color32::from_rgba_unmultiplied(255, 255, 255, 200));
    match s.genere {
        GenereSegno::Testo { misura } if !s.testo.is_empty() => {
            let g = painter.layout_no_wrap(s.testo.clone(), Carattere::Testo.font(ctx, (misura * dp_per_pt).max(4.0)), tema::INCHIOSTRO);
            painter.with_clip_rect(dove).galley(pos2(dove.left() + 4.0 * dp_per_pt, dove.center().y - g.size().y / 2.0), g, tema::INCHIOSTRO);
        }
        _ => {}
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn i_campi_da_compilare_detti_e_la_presa_dei_campi_piccoli() {
        assert_eq!(da_compilare_detto(0), "tutto compilato");
        assert_eq!(da_compilare_detto(1), "1 campo da compilare");
        assert_eq!(da_compilare_detto(2), "2 campi da compilare");
        // Una casella da 8 dp si prende in un quadrato da 32 dp attorno; un campo largo resta largo.
        let r = presa(Rect::from_min_size(pos2(100.0, 100.0), vec2(8.0, 8.0)));
        assert_eq!(r.size(), vec2(32.0, 32.0));
        assert_eq!(r.center(), pos2(104.0, 104.0));
        assert_eq!(presa(Rect::from_min_size(pos2(0.0, 0.0), vec2(200.0, 40.0))).size(), vec2(200.0, 40.0));
        let (g, k) = ingrandito(Rect::from_min_size(pos2(10.0, 10.0), vec2(20.0, 10.0)), vec2(20.0, 0.0));
        assert_eq!((g.size(), k), (vec2(40.0, 20.0), 2.0));
        assert_eq!(format!("{:?}", Bersaglio::Campo("residenza".into())), "Campo");
    }
}
