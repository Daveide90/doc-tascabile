//! Pagine e file (piano «PDF completo», P6).
//!
//! «Organizza pagine» dal foglio Strumenti: la griglia delle miniature di D3 a tutto schermo, un
//! tocco sceglie o toglie una pagina, la pressione lunga la prende e il dito la porta dove deve
//! andare; in fondo la barra con Ruota a sinistra, Ruota a destra, Elimina, Duplica, Estrai in un
//! PDF nuovo, Pagina bianca (prima o dopo, grande come la pagina vicina) e Unisci. «Unisci»: un
//! altro PDF (dai recenti, da Documenti/Tascabile o dalla scelta dei file di Android, l'Intent
//! della foto della firma) in fondo o dopo una pagina. «Dividi»: per intervalli o ogni N pagine,
//! i file nuovi in Documenti/Tascabile. «Comprimi»: tre livelli su una copia, con la misura prima
//! e dopo. «Proteggi» con la password (AES-256) e «Togli la protezione»; «Salva pulito» nel menu
//! riscrive tutto il file senza le revisioni di prima.
//!
//! Le modifiche passano da `modifica_pdf` (l'avviso delle firme di P1, annulla e ripeti); i file
//! nuovi si fanno su un thread di lavoro dai byte di adesso, e il documento aperto non cambia.
//! Nella diagnosi conti, misure e tempi: mai nomi di file, percorsi, testi o password.

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui::{self, Id, Order, Rect, RichText, Sense, UiBuilder, pos2, vec2};
use tascabile_files::{Tipo, cartella_tascabile, tipo_da_nome};
use tascabile_pdf::pagine::{self, Livello};
use tascabile_pdf::{Modifica, Organizza, Pdf};
use tascabile_platform::{EsitoFoto, FonteFoto, diagnosi};
use tascabile_touch::tema::{self, Carattere};
use tascabile_touch::{BarraAlta, BarraFlottante, CampoTesto, Chip, Dialogo, Peso, Pulsante, PulsanteIcona, RispostaDialogo, icone};
use tascabile_word::salva;

use crate::pdf_lavoro::{AzioneSalva, Strumento, si_modifica};
use crate::pdf_ocr::{Lavoro, mostra_barra};
use crate::{Errore, TascabileApp, home};

pub(crate) const CHIUDI: &str = "Chiudi";
pub(crate) const RUOTA_A_SINISTRA: &str = "Ruota a sinistra";
pub(crate) const RUOTA_A_DESTRA: &str = "Ruota a destra";
pub(crate) const ELIMINA: &str = "Elimina";
/// Il velo di «Organizza pagine» mentre il motore finisce una modifica.
pub(crate) const UN_ATTIMO: &str = "Un attimo…";
pub(crate) const DUPLICA: &str = "Duplica";
pub(crate) const ESTRAI: &str = "Estrai in un PDF nuovo";
pub(crate) const PAGINA_BIANCA: &str = "Pagina bianca";
pub(crate) const UNISCI: &str = "Unisci";
pub(crate) const SCEGLI_TUTTE: &str = "Scegli tutte";
pub(crate) const TOGLI_LA_SCELTA: &str = "Togli la scelta";
pub(crate) const UNISCI_UN_PDF: &str = "Unisci un altro PDF";
pub(crate) const IN_FONDO: &str = "In fondo";
pub(crate) const DAL_TELEFONO: &str = "Scegli dal telefono";
pub(crate) const DIVIDI: &str = "Dividi";
pub(crate) const PER_INTERVALLI: &str = "Per intervalli";
pub(crate) const OGNI_N: &str = "Ogni N pagine";
pub(crate) const COMPRIMI: &str = "Comprimi";
pub(crate) const PROTEGGI: &str = "Proteggi";
pub(crate) const PROTEGGI_CON_PASSWORD: &str = "Proteggi con password";
pub(crate) const TOGLI_PASSWORD: &str = "Togli la password";
pub(crate) const SALVA_PULITO: &str = "Salva pulito";
pub(crate) const PDF_COMPRESSO: &str = "PDF compresso";
pub(crate) const APRI_LA_COPIA: &str = "Apri la copia";
const ANNULLA: &str = "Annulla";
const HO_CAPITO: &str = "Ho capito";
const DIVIDO: &str = "Divido il PDF";
const COMPRIMO: &str = "Comprimo il PDF";
const NON_FATTO: &str = "Non fatto";
/// Il nome di un PDF arrivato dalla scelta dei file di Android: il nome vero non arriva.
const PDF_SCELTO: &str = "PDF unito.pdf";
/// Il tetto di un PDF da unire, come quello dei PDF da aprire.
const MASSIMO_PDF: u64 = (tascabile_pdf::LIMITE_MB as u64) << 20;
/// Quanti PDF proporre nel foglio di Unisci.
const PROPOSTE: usize = 30;

/// «Pagina bianca»: dove, rispetto alla pagina vicina.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dove {
    Prima,
    Dopo,
}

/// Lo stato di «Pagine e file» per il PDF aperto.
#[derive(Default)]
pub(crate) struct StatoPagine {
    /// La griglia di «Organizza pagine», se è aperta.
    pub(crate) organizza: Option<Organizza>,
    /// La pagina vicina della pagina bianca (da 0).
    vicina: usize,
    /// Dove va il PDF da unire: in fondo (None) o dopo questa pagina (da 0).
    unisci_dopo: Option<usize>,
    /// La pagina proposta per «Dopo la pagina» nel foglio di Unisci.
    unisci_pagina: usize,
    /// I PDF da proporre nel foglio di Unisci: nome e chiave (percorso o URI), letti all'apertura.
    da_unire: Vec<(String, String)>,
    /// Il PDF da unire chiesto alla scelta dei file: si aspetta la sua richiesta.
    pub(crate) attende_pdf: Option<i32>,
    /// Il campo di Dividi (intervalli o quante pagine) o di Proteggi (la password).
    pub(crate) campo: CampoTesto,
    /// La password si vede (l'occhio).
    visibile: bool,
    /// Dividi ogni N pagine invece che per intervalli.
    pub(crate) ogni: bool,
    /// Quello scritto non va: l'avviso rosso sotto il campo.
    pub(crate) sbagliato: bool,
    /// Il dialogo «Togliere la password?».
    togli: bool,
    /// Il dialogo di «Salva pulito».
    pulito: bool,
    /// La copia compressa da dire: dove, i byte di prima e di dopo.
    compresso: Option<(PathBuf, usize, usize)>,
}

/// Un lavoro di «Pagine e file» sul suo thread.
pub(crate) struct LavoroFile {
    quale: Quale,
    lavoro: Lavoro<Result<FileFatti, &'static str>>,
}

#[derive(Debug, Clone)]
enum Quale {
    Estrai,
    Dividi,
    Comprimi {
        prima: usize,
    },
    /// Il PDF da unire si legge: il nome per il motore e il posto.
    Unisci {
        nome: String,
        posto: usize,
    },
}

impl Quale {
    fn nome(&self) -> &'static str {
        match self {
            Quale::Estrai => "estrai",
            Quale::Dividi => "dividi",
            Quale::Comprimi { .. } => "comprimi",
            Quale::Unisci { .. } => "unisci",
        }
    }
}

/// Quello che un lavoro ha fatto.
enum FileFatti {
    /// I file nuovi scritti in Documenti/Tascabile.
    Scritti(Vec<PathBuf>),
    /// La copia compressa e quanti byte.
    Compresso(PathBuf, usize),
    /// La copia non sarebbe più piccola: non si è scritta.
    NonPiuPiccolo,
    /// I byte del PDF da unire, letti e controllati, con le sue pagine.
    DaUnire(Arc<Vec<u8>>, usize),
}

/// «1 pagina», «3 pagine».
fn pagine(n: usize) -> String {
    if n == 1 { "1 pagina".into() } else { format!("{n} pagine") }
}

/// Il nome di un file da una parte: «Lettera pagine 1-3», «Lettera pagina 5».
fn nome_della_parte(base: &str, parte: &[usize]) -> String {
    let parola = if parte.len() == 1 { "pagina" } else { "pagine" };
    format!("{base} {parola} {}", pagine::pagine_dette(parte))
}

/// Legge un file fino al tetto dei PDF.
fn leggi_pdf(percorso: &std::path::Path) -> Result<Vec<u8>, &'static str> {
    let f = std::fs::File::open(percorso).map_err(|_| "lettura")?;
    let mut v = Vec::new();
    std::io::Read::read_to_end(&mut std::io::Read::take(f, MASSIMO_PDF + 1), &mut v).map_err(|_| "lettura")?;
    if v.len() as u64 > MASSIMO_PDF { Err("oltre 200 MB") } else { Ok(v) }
}

/// Controlla i byte di un PDF da unire, sul thread di lavoro.
fn da_unire(byte: Vec<u8>) -> Result<FileFatti, &'static str> {
    if !tascabile_pdf::sembra_pdf(&byte) {
        return Err("non è un pdf");
    }
    let byte = Arc::new(byte);
    let n = pagine::si_unisce(&byte)?;
    Ok(FileFatti::DaUnire(byte, n))
}

/// Il messaggio per un genere d'errore dei lavori.
fn messaggio(quale: &Quale, genere: &str) -> String {
    match genere {
        "firma digitale" => "Un PDF firmato non si comprime: la copia andrebbe riscritta tutta e le firme non varrebbero più.".into(),
        "permessi del PDF" => "Chi ha creato il PDF non permette di copiarne le pagine.".into(),
        "password" => "Il PDF scelto è protetto da una password: toglila prima di unirlo.".into(),
        "illeggibile" | "non è un pdf" => "Il file scelto non è un PDF che si legge.".into(),
        "oltre 200 MB" => "Il PDF scelto supera i 200 MB.".into(),
        _ => match quale {
            Quale::Estrai => "Non riesco a estrarre le pagine.".into(),
            Quale::Dividi => "Non riesco a dividere il PDF.".into(),
            Quale::Comprimi { .. } => "Non riesco a comprimere il PDF.".into(),
            Quale::Unisci { .. } => "Non riesco a leggere il PDF scelto.".into(),
        },
    }
}

impl TascabileApp {
    /// La base dei nomi dei file nuovi: il nome del PDF senza estensione.
    fn base_dei_file(&self) -> String {
        let nome = self.pdf.as_ref().map(|p| p.nome.as_str()).unwrap_or("Documento");
        home::nome_da_mostrare(nome, Tipo::Pdf).to_string()
    }

    /// I byte da cui fare i file nuovi e la loro password: quelli dell'istantanea, con le modifiche.
    fn byte_di_adesso(&self) -> Option<(Arc<Vec<u8>>, Option<String>)> {
        let s = self.pdf.as_ref()?.motore.stato();
        Some((s.display.clone(), s.configurazione.password.as_deref().map(str::to_string)))
    }

    /// Avvia un lavoro di «Pagine e file» sul thread `pdf-file`, se non ce n'è già uno.
    fn avvia_lavoro_file(
        &mut self,
        quale: Quale,
        ctx: &egui::Context,
        f: impl FnOnce(&(dyn Fn(usize, usize) -> bool + Sync)) -> Result<FileFatti, &'static str> + Send + 'static,
    ) {
        if self.file_pdf.is_some() {
            return;
        }
        diagnosi(format!("pdf: {} avviato", quale.nome()));
        self.file_pdf = Lavoro::avvia("pdf-file", ctx, f).map(|lavoro| LavoroFile { quale, lavoro });
    }

    // ---- Organizza pagine ----

    /// «Organizza pagine»: la griglia a tutto schermo, dalla pagina che si legge.
    pub(crate) fn apri_organizza_pdf(&mut self) {
        let Some(p) = self.pdf.as_mut() else { return };
        p.lettore.apri_miniature();
        p.pagine.organizza = Some(Organizza::default());
        diagnosi(format!("pdf: organizza aperto, {}", pagine(p.lettore.pagine())));
    }

    fn chiudi_organizza_pdf(&mut self) {
        if let Some(p) = self.pdf.as_mut()
            && p.pagine.organizza.take().is_some()
        {
            diagnosi("pdf: organizza chiuso");
        }
    }

    /// Le pagine scelte nella griglia, in ordine (solo quelle che ci sono ancora).
    fn scelte(&self) -> Vec<usize> {
        let Some(p) = self.pdf.as_ref() else { return Vec::new() };
        let n = p.lettore.pagine();
        p.pagine.organizza.as_ref().map(|o| o.scelte.iter().copied().filter(|q| *q < n).collect()).unwrap_or_default()
    }

    fn scegli_pagine(&mut self, scelte: impl IntoIterator<Item = usize>) {
        if let Some(o) = self.pdf.as_mut().and_then(|p| p.pagine.organizza.as_mut()) {
            o.scelte = scelte.into_iter().collect();
        }
    }

    /// La schermata di «Organizza pagine»: la barra alta (Chiudi, Annulla e Ripeti, Scegli tutte),
    /// la griglia che scorre e in fondo la barra delle azioni sulle pagine scelte.
    pub(crate) fn mostra_organizza_pdf(&mut self, ctx: &egui::Context) {
        #[derive(Clone, Copy)]
        enum Tocco {
            Chiudi,
            Annulla,
            Ripeti,
            Tutte,
            Barra(usize),
        }
        let Some(p) = self.pdf.as_mut() else { return };
        // Una modifica che il motore non ha finito: la griglia mostra ancora le pagine di prima, e
        // un altro comando andrebbe sulle pagine sbagliate (analisi del 27/10, pagine I1). Si
        // aspetta, col velo «Un attimo…»; Chiudi resta.
        let occupato = p.motore.modifica_in_corso();
        let Some(org) = p.pagine.organizza.as_mut() else { return };
        let n = p.lettore.pagine();
        let (annulla, ripeti) = (p.motore.stato().annulla, p.motore.stato().ripeti);
        let scelte = org.scelte.iter().filter(|q| **q < n).count();
        let tutte = scelte == n && n > 0;
        let sotto = match scelte {
            0 => "Tocca per scegliere, tieni premuto per spostare".to_string(),
            1 => format!("1 pagina scelta su {n}"),
            k => format!("{k} pagine scelte su {n}"),
        };
        let schermo = ctx.content_rect();
        let mut tocco = None;
        let mut esito = tascabile_pdf::EsitoGriglia::default();
        egui::Area::new(Id::new("organizza_pagine")).order(Order::Foreground).fixed_pos(schermo.min).constrain(false).show(ctx, |ui| {
            ui.set_min_size(schermo.size());
            let _ = ui.interact(schermo, Id::new("organizza_fondo"), Sense::click_and_drag());
            ui.painter().rect_filled(schermo, 0.0, tema::TAVOLO);
            let barra = Rect::from_min_size(schermo.min, vec2(schermo.width(), tema::BARRA_ALTA));
            ui.painter().rect_filled(barra, 0.0, tema::CARTA);
            let storia = annulla || ripeti;
            let r = ui
                .scope_builder(UiBuilder::new().max_rect(barra), |ui| {
                    let mut b = BarraAlta::new(Strumento::OrganizzaPagine.nome()).indietro(CHIUDI).sottotitolo(&sotto);
                    if storia {
                        b = b
                            .azione(PulsanteIcona::new(icone::UNDO, crate::pdf_lavoro::ANNULLA_MODIFICA).spento(!annulla))
                            .azione(PulsanteIcona::new(icone::REDO, crate::pdf_lavoro::RIPETI_MODIFICA).spento(!ripeti));
                    }
                    let (icona, testo) = if tutte { (icone::CLOSE, TOGLI_LA_SCELTA) } else { (icone::SELECT_ALL, SCEGLI_TUTTE) };
                    b.azione(PulsanteIcona::new(icona, testo)).mostra(ui)
                })
                .inner;
            if r.indietro {
                tocco = Some(Tocco::Chiudi);
            }
            tocco = match r.azione {
                Some(0) if storia => Some(Tocco::Annulla),
                Some(1) if storia => Some(Tocco::Ripeti),
                Some(_) => Some(Tocco::Tutte),
                None => tocco,
            };
            let fondo = BarraFlottante::posto(schermo);
            let griglia = Rect::from_min_max(pos2(schermo.left() + 10.0, barra.bottom()), pos2(schermo.right() - 10.0, fondo.top() - 6.0));
            ui.scope_builder(UiBuilder::new().max_rect(griglia), |ui| {
                use egui::scroll_area::ScrollSource;
                let sorgente = if org.tiene() { ScrollSource::SCROLL_BAR | ScrollSource::MOUSE_WHEEL } else { ScrollSource::ALL };
                egui::ScrollArea::vertical().id_salt("organizza_griglia").auto_shrink([false, false]).scroll_source(sorgente).show(ui, |ui| {
                    ui.add_space(10.0);
                    esito = p.lettore.mostra_organizza(ui, org);
                    ui.add_space(10.0);
                });
            });
            let nessuna = scelte == 0;
            let b = BarraFlottante::new()
                .pulsante(PulsanteIcona::new(icone::ROTATE_LEFT, RUOTA_A_SINISTRA).spento(nessuna))
                .pulsante(PulsanteIcona::new(icone::ROTATE_RIGHT, RUOTA_A_DESTRA).spento(nessuna))
                .pulsante(PulsanteIcona::new(icone::DELETE, ELIMINA).spento(nessuna))
                .pulsante(PulsanteIcona::new(icone::CONTENT_COPY, DUPLICA).spento(nessuna))
                .pulsante(PulsanteIcona::new(icone::UPLOAD_FILE, ESTRAI).spento(nessuna))
                .separatore()
                .pulsante(PulsanteIcona::new(icone::NOTE_ADD, PAGINA_BIANCA))
                .pulsante(PulsanteIcona::new(icone::MERGE, UNISCI));
            if let Some(i) = b.mostra(ui, fondo) {
                tocco = Some(Tocco::Barra(i));
            }
            if occupato {
                let font = Carattere::Grassetto.font(ui.ctx(), 15.0);
                let g = ui.painter().layout_no_wrap(UN_ATTIMO.to_string(), font, egui::Color32::WHITE);
                let pillola = Rect::from_center_size(griglia.center(), vec2(g.size().x + 32.0, 44.0));
                ui.painter().rect_filled(pillola, 22.0, tema::INDICATORE);
                ui.painter().galley(pillola.center() - g.size() / 2.0, g, egui::Color32::PLACEHOLDER);
                ui.interact(pillola, Id::new("organizza_un_attimo"), Sense::hover())
                    .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, UN_ATTIMO));
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(30));
            }
        });
        if occupato {
            if !matches!(tocco, None | Some(Tocco::Chiudi)) || esito.toccata.is_some() || esito.spostate.is_some() {
                diagnosi("pdf: organizza, comando mentre il motore lavora: aspetta");
            }
            esito.toccata = None;
            esito.spostate = None;
            tocco = tocco.filter(|t| matches!(t, Tocco::Chiudi));
        }
        if let Some(t) = esito.toccata
            && let Some(o) = self.pdf.as_mut().and_then(|p| p.pagine.organizza.as_mut())
            && !o.scelte.remove(&t)
        {
            o.scelte.insert(t);
        }
        if let Some(k) = esito.prese {
            diagnosi(format!("pdf: organizza, prese col dito: {}", pagine(k)));
        }
        if let Some((spostate, posto)) = esito.spostate {
            diagnosi(format!("pdf: organizza, spostate: {}", pagine(spostate.len())));
            let a = pagine::posto_dopo_lo_spostamento(&spostate, posto);
            let k = spostate.len();
            self.modifica_pdf(Modifica::sposta(spostate, posto));
            self.scegli_pagine(a..a + k);
        }
        match tocco {
            Some(Tocco::Chiudi) => self.chiudi_organizza_pdf(),
            Some(Tocco::Annulla) => {
                self.scegli_pagine([]);
                self.annulla_pdf();
            }
            Some(Tocco::Ripeti) => {
                self.scegli_pagine([]);
                self.ripeti_pdf();
            }
            Some(Tocco::Tutte) => self.scegli_pagine(if tutte { 0..0 } else { 0..n }),
            Some(Tocco::Barra(i)) => self.azione_organizza_pdf(i, ctx),
            None => {}
        }
    }

    /// Un'azione della barra in fondo di «Organizza pagine» sulle pagine scelte.
    fn azione_organizza_pdf(&mut self, i: usize, ctx: &egui::Context) {
        let scelte = self.scelte();
        let n = self.pdf.as_ref().map_or(0, |p| p.lettore.pagine());
        match i {
            0 | 1 if !scelte.is_empty() => {
                diagnosi(format!("pdf: organizza, ruota {}", pagine(scelte.len())));
                self.modifica_pdf(Modifica::ruota(scelte, if i == 0 { 270 } else { 90 }));
            }
            2 if !scelte.is_empty() => {
                if scelte.len() >= n {
                    self.errore =
                        Some(Errore { titolo: NON_FATTO, testo: "Non si possono eliminare tutte le pagine: ne deve restare almeno una.".into() });
                    return;
                }
                diagnosi(format!("pdf: organizza, elimina {}", pagine(scelte.len())));
                let k = scelte.len();
                self.modifica_pdf(Modifica::elimina(scelte));
                self.scegli_pagine([]);
                let firmato = self.pdf.as_ref().is_some_and(|p| p.motore.stato().protezione.firmato());
                let manda = self.pdf.as_ref().is_some_and(|p| p.in_attesa_firme.is_none());
                // Salva riscrive tutto il file non firmato: la pagina tolta non resta nelle versioni di prima.
                if manda && !firmato {
                    self.avviso_breve.avvisa(if k == 1 { "1 pagina eliminata".to_string() } else { format!("{k} pagine eliminate") });
                }
            }
            3 if !scelte.is_empty() => {
                diagnosi(format!("pdf: organizza, duplica {}", pagine(scelte.len())));
                self.modifica_pdf(Modifica::duplica(scelte));
                self.scegli_pagine([]);
            }
            4 if !scelte.is_empty() => self.estrai_pdf(scelte, ctx),
            5 => {
                let vicina = scelte.first().copied().unwrap_or(n.saturating_sub(1));
                if let Some(p) = self.pdf.as_mut() {
                    p.pagine.vicina = vicina;
                }
                self.fogli_pdf.bianca.apri();
            }
            6 => self.apri_unisci_pdf(scelte.last().copied()),
            _ => {}
        }
    }

    // ---- Estrai e Dividi ----

    /// «Estrai in un PDF nuovo»: le pagine scelte, nel loro ordine, in Documenti/Tascabile.
    fn estrai_pdf(&mut self, scelte: Vec<usize>, ctx: &egui::Context) {
        let Some((byte, password)) = self.byte_di_adesso() else { return };
        let cartella = cartella_tascabile(&self.piattaforma.radice_archivio());
        let nome = nome_della_parte(&self.base_dei_file(), &scelte);
        diagnosi(format!("pdf: estrai {}", pagine(scelte.len())));
        self.avvia_lavoro_file(Quale::Estrai, ctx, move |_| {
            let pdf = Pdf::apri_condiviso(byte, password.as_deref()).map_err(|_| "apertura")?;
            let nuovo = pdf.estrai(&scelte)?;
            std::fs::create_dir_all(&cartella).map_err(|_| "cartella")?;
            let percorso = salva::scrivi_nuovo(&cartella, &nome, "pdf", &nuovo).map_err(|_| "scrittura")?;
            Ok(FileFatti::Scritti(vec![percorso]))
        });
    }

    /// «Dividi» nel foglio Strumenti: il foglio col campo, che prende la tastiera.
    pub(crate) fn apri_dividi_pdf(&mut self) {
        self.fuoco_al_documento();
        let Some(p) = self.pdf.as_mut() else { return };
        p.pagine.campo = if p.pagine.ogni { CampoTesto::solo_cifre() } else { CampoTesto::new() };
        p.pagine.sbagliato = false;
        diagnosi("pdf: dividi aperto");
        self.fogli_pdf.dividi.apri();
        self.fuoco_al_campo_pdf();
    }

    /// Il foglio di «Dividi»: per intervalli o ogni N pagine, il campo, Dividi e sotto quante pagine
    /// ha il PDF (o, in rosso, che quello scritto non va).
    fn mostra_foglio_dividi(&mut self, ctx: &egui::Context) {
        let Some(p) = self.pdf.as_ref() else {
            self.fogli_pdf.dividi.mostra(ctx, DIVIDI, |_| ());
            return;
        };
        let n = p.lettore.pagine();
        let (ogni, sbagliato) = (p.pagine.ogni, p.pagine.sbagliato);
        let (mut campo, mut dividi, mut modo) = (false, false, None);
        self.fogli_pdf.dividi.mostra(ctx, DIVIDI, |ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
            ui.horizontal(|ui| {
                if ui.add(Chip::new(PER_INTERVALLI).acceso(!ogni)).clicked() {
                    modo = Some(false);
                }
                if ui.add(Chip::new(OGNI_N).acceso(ogni)).clicked() {
                    modo = Some(true);
                }
            });
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                let largo = (ui.available_width() - 104.0).max(48.0);
                let vuoto = if ogni { "Pagine in ogni file" } else { "1-3, 4-8" };
                campo = ui.allocate_ui(vec2(largo, tema::PULSANTE), |ui| p.pagine.campo.mostra(ui, vuoto).clicked()).inner;
                dividi = ui.add(Pulsante::new(DIVIDI, Peso::Principale).larghezza(96.0)).clicked();
            });
            ui.add_space(10.0);
            let (testo, colore) = match (sbagliato, ogni) {
                (true, false) => (format!("Scrivi gli intervalli come 1-3, 4-8 (9- vuol dire fino in fondo), da 1 a {n}."), tema::ROSSO),
                (true, true) => (format!("Scrivi quante pagine in ogni file, da 1 a {n}."), tema::ROSSO),
                (false, _) => (format!("Il PDF ha {}. I file nuovi vanno in Documenti/Tascabile: questo resta com'è.", pagine(n)), tema::MATITA),
            };
            ui.label(RichText::new(testo).font(Carattere::Testo.font(ctx, 15.0)).color(colore));
            ui.add_space(6.0);
        });
        if let Some(o) = modo
            && let Some(p) = self.pdf.as_mut()
            && p.pagine.ogni != o
        {
            p.pagine.ogni = o;
            p.pagine.sbagliato = false;
            self.fuoco_al_documento();
            if let Some(p) = self.pdf.as_mut() {
                p.pagine.campo = if o { CampoTesto::solo_cifre() } else { CampoTesto::new() };
            }
            self.fuoco_al_campo_pdf();
        }
        if campo {
            self.fuoco_al_campo_pdf();
        }
        if dividi {
            self.dividi_pdf(ctx);
        }
    }

    /// «Dividi» (o Invio nel campo): le parti scritte, ciascuna un file in Documenti/Tascabile.
    pub(crate) fn dividi_pdf(&mut self, ctx: &egui::Context) {
        let Some(p) = self.pdf.as_mut() else { return };
        let n = p.lettore.pagine();
        let testo = p.pagine.campo.testo().to_string();
        let parti = if p.pagine.ogni {
            testo.trim().parse::<usize>().ok().filter(|k| (1..=n).contains(k)).map(|k| pagine::ogni(k, n))
        } else {
            pagine::intervalli(&testo, n)
        };
        let Some(parti) = parti else {
            p.pagine.sbagliato = true;
            diagnosi("pdf: dividi, intervalli che non vanno");
            return;
        };
        self.chiudi_campo_pagine();
        self.fogli_pdf.dividi.chiudi();
        let Some((byte, password)) = self.byte_di_adesso() else { return };
        let cartella = cartella_tascabile(&self.piattaforma.radice_archivio());
        let base = self.base_dei_file();
        diagnosi(format!("pdf: dividi in {} parti", parti.len()));
        self.avvia_lavoro_file(Quale::Dividi, ctx, move |avanti| {
            let pdf = Pdf::apri_condiviso(byte, password.as_deref()).map_err(|_| "apertura")?;
            std::fs::create_dir_all(&cartella).map_err(|_| "cartella")?;
            let mut scritti = Vec::new();
            for (i, parte) in parti.iter().enumerate() {
                if !avanti(i, parti.len()) {
                    break;
                }
                let nuovo = pdf.estrai(parte)?;
                scritti.push(salva::scrivi_nuovo(&cartella, &nome_della_parte(&base, parte), "pdf", &nuovo).map_err(|_| "scrittura")?);
            }
            avanti(parti.len(), parti.len());
            Ok(FileFatti::Scritti(scritti))
        });
    }

    // ---- Pagina bianca e Unisci ----

    fn mostra_foglio_bianca(&mut self, ctx: &egui::Context) {
        let vicina = self.pdf.as_ref().map_or(0, |p| p.pagine.vicina);
        let prima = format!("Prima della pagina {}", vicina + 1);
        let dopo = format!("Dopo la pagina {}", vicina + 1);
        let scelta = self
            .fogli_pdf
            .bianca
            .mostra(ctx, PAGINA_BIANCA, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let mut scelta = None;
                if crate::pdf_firma::riga(ui, icone::NOTE_ADD, &prima, None, false).clicked() {
                    scelta = Some(Dove::Prima);
                }
                if crate::pdf_firma::riga(ui, icone::NOTE_ADD, &dopo, None, true).clicked() {
                    scelta = Some(Dove::Dopo);
                }
                ui.add_space(6.0);
                ui.label(RichText::new("Grande come la pagina vicina.").font(Carattere::Testo.font(ui.ctx(), 14.0)).color(tema::MATITA));
                ui.add_space(6.0);
                scelta
            })
            .flatten();
        let Some(dove) = scelta else { return };
        self.fogli_pdf.bianca.chiudi();
        let Some(misura) = self.pdf.as_ref().and_then(|p| p.motore.stato().misure.get(vicina).copied()) else { return };
        let posto = if dove == Dove::Prima { vicina } else { vicina + 1 };
        diagnosi(format!("pdf: pagina bianca {} della pagina {}", if dove == Dove::Prima { "prima" } else { "dopo" }, vicina + 1));
        self.modifica_pdf(Modifica::pagina_bianca(posto, misura));
        self.scegli_pagine([posto]);
    }

    /// «Unisci»: il foglio coi PDF da proporre (i recenti e quelli in Documenti/Tascabile, letti
    /// adesso) e la scelta dei file di Android; il PDF va in fondo o dopo la pagina `dopo`.
    pub(crate) fn apri_unisci_pdf(&mut self, dopo: Option<usize>) {
        let cartella = cartella_tascabile(&self.piattaforma.radice_archivio());
        let Some(p) = self.pdf.as_ref() else { return };
        let mia = p.chiave.clone();
        let corrente = p.lettore.pagina_corrente();
        let mut proposte: Vec<(String, String)> = self
            .home
            .recenti()
            .iter()
            .filter(|r| r.tipo == Tipo::Pdf && Some(&r.chiave) != mia.as_ref())
            .map(|r| (r.nome.clone(), r.chiave.clone()))
            .collect();
        if let Ok(voci) = std::fs::read_dir(&cartella) {
            let mut nella_cartella: Vec<(String, String)> = voci
                .flatten()
                .filter_map(|v| {
                    let nome = v.file_name().to_str()?.to_string();
                    let chiave = v.path().to_str()?.to_string();
                    (tipo_da_nome(&nome) == Tipo::Pdf && v.path().is_file()).then_some((nome, chiave))
                })
                .collect();
            nella_cartella.sort();
            for v in nella_cartella {
                if !proposte.iter().any(|(_, c)| *c == v.1) && Some(&v.1) != mia.as_ref() {
                    proposte.push(v);
                }
            }
        }
        proposte.truncate(PROPOSTE);
        diagnosi(format!("pdf: unisci aperto, {} pdf da proporre", proposte.len()));
        let Some(p) = self.pdf.as_mut() else { return };
        p.pagine.da_unire = proposte;
        p.pagine.unisci_dopo = dopo;
        p.pagine.unisci_pagina = dopo.unwrap_or(corrente);
        self.fogli_pdf.unisci.apri();
    }

    fn mostra_foglio_unisci(&mut self, ctx: &egui::Context) {
        enum Scelta {
            InFondo,
            Dopo,
            Telefono,
            File(usize),
        }
        let (proposte, dopo, pagina) = match self.pdf.as_ref() {
            Some(p) => (p.pagine.da_unire.clone(), p.pagine.unisci_dopo, p.pagine.unisci_pagina),
            None => (Vec::new(), None, 0),
        };
        let dopo_la = format!("Dopo la pagina {}", pagina + 1);
        let scelta = self
            .fogli_pdf
            .unisci
            .mostra(ctx, UNISCI_UN_PDF, |ui| {
                ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
                let mut scelta = None;
                ui.horizontal(|ui| {
                    if ui.add(Chip::new(IN_FONDO).acceso(dopo.is_none())).clicked() {
                        scelta = Some(Scelta::InFondo);
                    }
                    if ui.add(Chip::new(&dopo_la).acceso(dopo.is_some())).clicked() {
                        scelta = Some(Scelta::Dopo);
                    }
                });
                ui.add_space(8.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                if crate::pdf_firma::riga(ui, icone::FOLDER_OPEN, DAL_TELEFONO, None, proposte.is_empty()).clicked() {
                    scelta = Some(Scelta::Telefono);
                }
                for (i, (nome, _)) in proposte.iter().enumerate() {
                    let nome = home::nome_da_mostrare(nome, Tipo::Pdf);
                    if crate::pdf_firma::riga(ui, icone::PICTURE_AS_PDF, nome, None, i + 1 == proposte.len()).clicked() {
                        scelta = Some(Scelta::File(i));
                    }
                }
                ui.add_space(6.0);
                scelta
            })
            .flatten();
        let Some(scelta) = scelta else { return };
        let Some(p) = self.pdf.as_mut() else { return };
        match scelta {
            Scelta::InFondo => p.pagine.unisci_dopo = None,
            Scelta::Dopo => p.pagine.unisci_dopo = Some(p.pagine.unisci_pagina),
            Scelta::Telefono => {
                self.fogli_pdf.unisci.chiudi();
                p.pagine.attende_pdf = Some(self.piattaforma.chiedi_foto_firma(FonteFoto::Pdf));
            }
            Scelta::File(i) => {
                let Some((nome, chiave)) = p.pagine.da_unire.get(i).cloned() else { return };
                self.fogli_pdf.unisci.chiudi();
                self.leggi_da_unire(nome, chiave, ctx);
            }
        }
    }

    /// Il posto dove va il PDF da unire: in fondo o dopo la pagina scelta.
    fn posto_da_unire(&self) -> usize {
        let Some(p) = self.pdf.as_ref() else { return 0 };
        let n = p.lettore.pagine();
        p.pagine.unisci_dopo.map_or(n, |q| (q + 1).min(n))
    }

    /// Un PDF proposto da unire: un file si legge sul thread di lavoro, un URI qui (la piattaforma
    /// non va sugli altri thread), poi si controlla là.
    fn leggi_da_unire(&mut self, nome: String, chiave: String, ctx: &egui::Context) {
        let posto = self.posto_da_unire();
        let quale = Quale::Unisci { nome, posto };
        if home::e_uri(&chiave) {
            match self.piattaforma.read_uri(&chiave) {
                Ok(byte) => self.avvia_lavoro_file(quale, ctx, move |_| da_unire(byte)),
                Err(e) => {
                    diagnosi(format!("pdf: unisci, uri non letto ({})", e.genere));
                    self.errore = Some(Errore { titolo: NON_FATTO, testo: "Non riesco a leggere il PDF scelto: riaprilo dalla sua app.".into() });
                }
            }
        } else {
            let percorso = PathBuf::from(chiave);
            self.avvia_lavoro_file(quale, ctx, move |_| da_unire(leggi_pdf(&percorso)?));
        }
    }

    /// A ogni frame: il PDF chiesto alla scelta dei file, se è arrivato, si legge (e si cancella
    /// dalla cache) sul thread di lavoro.
    pub(crate) fn passo_unisci_pdf(&mut self, ctx: &egui::Context) {
        let Some(richiesta) = self.pdf.as_ref().and_then(|p| p.pagine.attende_pdf) else { return };
        let Some(esito) = self.piattaforma.prendi_foto_firma(richiesta) else { return };
        if let Some(p) = self.pdf.as_mut() {
            p.pagine.attende_pdf = None;
        }
        match esito {
            EsitoFoto::Pronta(file) => {
                let posto = self.posto_da_unire();
                self.avvia_lavoro_file(Quale::Unisci { nome: PDF_SCELTO.into(), posto }, ctx, move |_| {
                    let letto = leggi_pdf(&file);
                    crate::butta_la_foto(&file);
                    da_unire(letto?)
                });
            }
            EsitoFoto::Pronte { file, .. } => crate::butta_le_foto(&file, "pdf: unisci"),
            EsitoFoto::Annullata => diagnosi("pdf: unisci, nessun pdf scelto"),
            EsitoFoto::Superata => diagnosi("pdf: unisci, richiesta superata da un'altra"),
            EsitoFoto::Errore => {
                diagnosi("pdf: unisci, pdf non arrivato");
                self.avviso_breve.avvisa("Il PDF scelto non è arrivato.");
            }
        }
    }

    // ---- Comprimi ----

    pub(crate) fn apri_comprimi_pdf(&mut self) {
        diagnosi("pdf: comprimi aperto");
        self.fogli_pdf.comprimi.apri();
    }

    fn mostra_foglio_comprimi(&mut self, ctx: &egui::Context) {
        let prima = self.pdf.as_ref().map_or(0, |p| p.motore.stato().informazioni.byte);
        let scelta = self
            .fogli_pdf
            .comprimi
            .mostra(ctx, COMPRIMI, |ui| {
                ui.label(
                    RichText::new(format!(
                        "Adesso il PDF pesa {}. La copia compressa va in Documenti/Tascabile: questo resta com'è.",
                        home::misura(prima as u64)
                    ))
                    .font(Carattere::Testo.font(ui.ctx(), 15.0))
                    .color(tema::MATITA),
                );
                ui.add_space(8.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                let mut scelta = None;
                for (i, l) in Livello::TUTTI.into_iter().enumerate() {
                    let testo = format!("{} · {}", l.nome(), l.spiegazione());
                    if crate::pdf_firma::riga(ui, icone::COMPRESS, &testo, None, i == 2).clicked() {
                        scelta = Some(l);
                    }
                }
                ui.add_space(6.0);
                scelta
            })
            .flatten();
        if let Some(livello) = scelta {
            self.fogli_pdf.comprimi.chiudi();
            self.comprimi_pdf(livello, ctx);
        }
    }

    /// Comprime una copia a `livello` sul thread di lavoro, col dialogo dell'avanzamento.
    pub(crate) fn comprimi_pdf(&mut self, livello: Livello, ctx: &egui::Context) {
        let Some((byte, password)) = self.byte_di_adesso() else { return };
        let prima = self.pdf.as_ref().map_or(0, |p| p.motore.stato().informazioni.byte);
        let cartella = cartella_tascabile(&self.piattaforma.radice_archivio());
        let nome = format!("{} compresso", self.base_dei_file());
        diagnosi(format!("pdf: comprimi, livello {}, {prima} byte", livello.nome().to_lowercase()));
        self.avvia_lavoro_file(Quale::Comprimi { prima }, ctx, move |avanti| {
            let pdf = Pdf::apri_condiviso(byte, password.as_deref()).map_err(|_| "apertura")?;
            let compresso = pdf.compressione(livello)?.esegui(avanti)?;
            if compresso.len() >= prima {
                return Ok(FileFatti::NonPiuPiccolo);
            }
            std::fs::create_dir_all(&cartella).map_err(|_| "cartella")?;
            let percorso = salva::scrivi_nuovo(&cartella, &nome, "pdf", &compresso).map_err(|_| "scrittura")?;
            Ok(FileFatti::Compresso(percorso, compresso.len()))
        });
    }

    // ---- Proteggi, togli la protezione, Salva pulito ----

    /// «Proteggi» nel foglio Strumenti: il foglio con la password, che prende la tastiera.
    pub(crate) fn apri_proteggi_pdf(&mut self) {
        self.fuoco_al_documento();
        let Some(p) = self.pdf.as_mut() else { return };
        p.pagine.campo = CampoTesto::segreto();
        p.pagine.visibile = false;
        p.pagine.sbagliato = false;
        diagnosi("pdf: proteggi aperto");
        self.fogli_pdf.proteggi.apri();
        self.fuoco_al_campo_pdf();
    }

    fn mostra_foglio_proteggi(&mut self, ctx: &egui::Context) {
        let Some(p) = self.pdf.as_ref() else {
            self.fogli_pdf.proteggi.mostra(ctx, PROTEGGI_CON_PASSWORD, |_| ());
            return;
        };
        let (visibile, sbagliato) = (p.pagine.visibile, p.pagine.sbagliato);
        let avviso = crate::pdf_lavoro::con_la_copia(
            "Se la dimentichi non si recupera: nessuno, nemmeno Tascabile, potrà più aprire il PDF. La protezione si scrive quando salvi.",
            p,
        );
        let (mut campo, mut occhio, mut proteggi) = (false, false, false);
        self.fogli_pdf.proteggi.mostra(ctx, PROTEGGI_CON_PASSWORD, |ui| {
            ui.label(
                RichText::new("Chi apre il PDF dovrà scrivere questa password. Cifratura AES a 256 bit.")
                    .font(Carattere::Testo.font(ui.ctx(), 15.0))
                    .color(tema::MATITA),
            );
            ui.add_space(12.0);
            let r = p.pagine.campo.mostra_segreto(ui, "Password", visibile);
            (campo, occhio) = (r.campo.clicked(), r.occhio);
            if sbagliato {
                ui.add_space(6.0);
                ui.label(RichText::new("Scrivi la password.").font(Carattere::Grassetto.font(ui.ctx(), 15.0)).color(tema::ROSSO));
            }
            ui.add_space(12.0);
            ui.label(RichText::new(avviso.as_str()).font(Carattere::Grassetto.font(ui.ctx(), 15.0)).color(tema::INCHIOSTRO));
            ui.add_space(12.0);
            proteggi = ui.add(Pulsante::new(PROTEGGI, Peso::Principale).icona(icone::LOCK).larghezza(ui.available_width())).clicked();
            ui.add_space(6.0);
        });
        if occhio && let Some(p) = self.pdf.as_mut() {
            p.pagine.visibile = !p.pagine.visibile;
        }
        if campo {
            self.fuoco_al_campo_pdf();
        }
        if proteggi {
            self.proteggi_pdf();
        }
    }

    /// «Proteggi» (o Invio nel campo): la password va al motore e il campo si svuota; da lì sta
    /// solo nella modifica del motore, mai in un file, nella diagnosi o nei log.
    pub(crate) fn proteggi_pdf(&mut self) {
        let Some(p) = self.pdf.as_mut() else { return };
        if p.pagine.campo.testo().is_empty() {
            p.pagine.sbagliato = true;
            return;
        }
        let password = p.pagine.campo.testo().to_string();
        self.chiudi_campo_pagine();
        self.fogli_pdf.proteggi.chiudi();
        diagnosi("pdf: proteggi con password");
        self.modifica_pdf(Modifica::proteggi(&password));
        if self.pdf.as_ref().is_some_and(|p| p.in_attesa_firme.is_none()) && self.errore.is_none() {
            self.avviso_breve.avvisa("Password messa: il PDF si protegge quando salvi.");
        }
    }

    /// «Togli la protezione» nel foglio Strumenti: prima il dialogo.
    pub(crate) fn chiedi_togli_protezione_pdf(&mut self) {
        if let Some(p) = self.pdf.as_mut() {
            p.pagine.togli = true;
        }
    }

    /// «Salva pulito» nel menu: prima il dialogo che dice che cosa fa.
    pub(crate) fn chiedi_salva_pulito_pdf(&mut self) {
        if let Some(p) = self.pdf.as_mut() {
            p.pagine.pulito = true;
        }
    }

    /// I dialoghi di «Pagine e file»: togliere la password, Salva pulito, la copia compressa.
    fn mostra_dialoghi_pagine(&mut self, ctx: &egui::Context) {
        let Some(p) = self.pdf.as_ref() else { return };
        if p.pagine.togli {
            let r = Dialogo::new(Id::new("togli_password"), "Togliere la password?")
                .testo("Chi apre il PDF non dovrà più scriverla. La protezione se ne va quando salvi.")
                .pulsante(ANNULLA, Peso::Testo)
                .pulsante(TOGLI_PASSWORD, Peso::Principale)
                .mostra(ctx);
            if let Some(r) = r {
                if let Some(p) = self.pdf.as_mut() {
                    p.pagine.togli = false;
                }
                if r == RispostaDialogo::Pulsante(1) {
                    diagnosi("pdf: protezione tolta");
                    self.modifica_pdf(Modifica::togli_protezione());
                }
            }
            return;
        }
        if p.pagine.pulito {
            let testo = crate::pdf_lavoro::con_la_copia(
                "Riscrivo tutto il file com'è adesso, senza le versioni di prima: le pagine e i dati tolti non restano nascosti nel PDF.",
                p,
            );
            let r = Dialogo::new(Id::new("salva_pulito"), SALVA_PULITO)
                .testo(&testo)
                .pulsante(ANNULLA, Peso::Testo)
                .pulsante(SALVA_PULITO, Peso::Principale)
                .mostra(ctx);
            if let Some(r) = r {
                if let Some(p) = self.pdf.as_mut() {
                    p.pagine.pulito = false;
                }
                if r == RispostaDialogo::Pulsante(1) {
                    self.salva_pdf(AzioneSalva::SalvaPulito, None);
                }
            }
            return;
        }
        if let Some((percorso, prima, dopo)) = p.pagine.compresso.clone() {
            // La copia si apre solo se questo PDF non ha modifiche da salvare: aprirla lo chiude.
            let apri = !p.motore.stato().modificato && !p.copia.recuperato;
            let nome = percorso.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let meno = 100 - (dopo * 100).div_ceil(prima.max(1)).min(100);
            let testo = format!(
                "Da {} a {}: il {meno}% in meno. La copia è in Documenti/Tascabile: «{nome}». Questo PDF resta com'è.",
                home::misura(prima as u64),
                home::misura(dopo as u64)
            );
            let mut d = Dialogo::new(Id::new("pdf_compresso"), PDF_COMPRESSO).testo(&testo).pulsante(HO_CAPITO, Peso::Testo);
            if apri {
                d = d.pulsante(APRI_LA_COPIA, Peso::Principale);
            }
            if let Some(r) = d.mostra(ctx) {
                if let Some(p) = self.pdf.as_mut() {
                    p.pagine.compresso = None;
                }
                if apri && r == RispostaDialogo::Pulsante(1) {
                    diagnosi("pdf: si apre la copia compressa");
                    self.chiudi_organizza_pdf();
                    self.apri_file(&percorso, &nome);
                }
            }
        }
    }

    // ---- Il lavoro, i fogli, Indietro ----

    /// A ogni frame: l'esito del lavoro di «Pagine e file», se è arrivato.
    pub(crate) fn passo_file_pdf(&mut self) {
        let Some(l) = self.file_pdf.as_ref() else { return };
        let Some(esito) = l.lavoro.esito() else { return };
        let secondi = l.lavoro.inizio.elapsed().as_secs_f32();
        let fermato = l.lavoro.fermato();
        let quale = l.quale.clone();
        self.file_pdf = None;
        let nome = |p: &PathBuf| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match esito {
            Some(Ok(FileFatti::Scritti(file))) => {
                diagnosi(format!("pdf: {} fatto, {} file in {secondi:.1} s", quale.nome(), file.len()));
                match file.as_slice() {
                    [] => self.avviso_breve.avvisa("Nessun file scritto."),
                    [f] => self.avviso_breve.avvisa(format!("Salvato in Documenti/Tascabile: {}", nome(f))),
                    [f, ..] => {
                        let fermo = if fermato { " (fermato prima della fine)" } else { "" };
                        self.avviso_breve.avvisa(format!("{} file in Documenti/Tascabile{fermo}: «{}» e gli altri.", file.len(), nome(f)))
                    }
                }
            }
            Some(Ok(FileFatti::Compresso(percorso, dopo))) => {
                let prima = match quale {
                    Quale::Comprimi { prima } => prima,
                    _ => 0,
                };
                diagnosi(format!("pdf: compresso da {prima} a {dopo} byte in {secondi:.1} s"));
                if let Some(p) = self.pdf.as_mut() {
                    p.pagine.compresso = Some((percorso, prima, dopo));
                }
            }
            Some(Ok(FileFatti::NonPiuPiccolo)) => {
                diagnosi(format!("pdf: compresso, non più piccolo, in {secondi:.1} s"));
                self.avviso_breve.avvisa("Il PDF è già compresso: la copia non sarebbe più piccola, non l'ho scritta.");
            }
            Some(Ok(FileFatti::DaUnire(byte, n))) => {
                let Quale::Unisci { nome, posto } = quale else { return };
                let fine = self.pdf.as_ref().is_some_and(|p| posto >= p.lettore.pagine());
                diagnosi(format!("pdf: unisci {} {}", pagine(n), if fine { "in fondo" } else { "dopo una pagina" }));
                self.modifica_pdf(Modifica::unisci(&nome, byte, posto));
                if self.pdf.as_ref().is_some_and(|p| p.in_attesa_firme.is_none()) {
                    let dove = if fine { "in fondo".to_string() } else { format!("dopo la pagina {posto}") };
                    self.avviso_breve.avvisa(format!("Unisco {} {dove}.", pagine(n)));
                }
                self.scegli_pagine(posto..posto + n);
            }
            Some(Err("fermata")) => {
                diagnosi(format!("pdf: {} fermato", quale.nome()));
                self.avviso_breve.avvisa("Fermato: niente di nuovo.");
            }
            Some(Err(genere)) => {
                diagnosi(format!("pdf: {} non fatto ({genere})", quale.nome()));
                self.errore = Some(Errore { titolo: NON_FATTO, testo: messaggio(&quale, genere) });
            }
            None => self.errore = Some(Errore { titolo: NON_FATTO, testo: "Il lavoro si è fermato con un errore interno.".into() }),
        }
    }

    /// I fogli e i dialoghi di «Pagine e file», e l'avanzamento del lavoro: a ogni frame.
    pub(crate) fn mostra_fogli_pagine(&mut self, ctx: &egui::Context) {
        let dividi = self.fogli_pdf.dividi.aperto();
        let proteggi = self.fogli_pdf.proteggi.aperto();
        self.mostra_foglio_bianca(ctx);
        self.mostra_foglio_unisci(ctx);
        self.mostra_foglio_dividi(ctx);
        self.mostra_foglio_comprimi(ctx);
        self.mostra_foglio_proteggi(ctx);
        // Chiusi toccando fuori o con la X: il campo lascia la tastiera e si svuota.
        if (dividi && !self.fogli_pdf.dividi.aperto()) || (proteggi && !self.fogli_pdf.proteggi.aperto()) {
            self.chiudi_campo_pagine();
        }
        self.mostra_dialoghi_pagine(ctx);
        let annulla = match self.file_pdf.as_ref() {
            Some(l) => match &l.quale {
                Quale::Dividi => mostra_barra(ctx, "dividi_pdf", DIVIDO, "Scrivo i file nuovi", l.lavoro.frazione(), l.lavoro.fermato()),
                Quale::Comprimi { .. } => {
                    mostra_barra(ctx, "comprimi_pdf", COMPRIMO, "Comprimo le immagini", l.lavoro.frazione(), l.lavoro.fermato())
                }
                _ => false,
            },
            None => false,
        };
        if annulla {
            self.ferma_file_pdf();
        }
    }

    /// Il campo di Dividi o di Proteggi lascia la tastiera e si svuota (una password non resta).
    fn chiudi_campo_pagine(&mut self) {
        // Prima si svuota col fuoco, così anche la tastiera riceve il campo vuoto.
        if let Some(p) = self.pdf.as_mut()
            && let Some(st) = p.pagine.campo.svuota()
        {
            self.versioni.continua_dalla_versione(st.versione);
            self.piattaforma.set_ime_state(st.testo, st.sel.0, st.sel.1, st.versione);
        }
        let campo = self.fuoco == crate::Fuoco::Campo;
        self.fuoco_al_documento();
        if campo && self.piattaforma.keyboard_visible() {
            self.piattaforma.hide_keyboard();
        }
    }

    /// Il foglio di Dividi o di Proteggi è aperto: il suo campo riceve la tastiera.
    pub(crate) fn campo_pagine_aperto(&self) -> bool {
        self.fogli_pdf.dividi.aperto() || self.fogli_pdf.proteggi.aperto()
    }

    /// Invio nel campo di Dividi o di Proteggi.
    pub(crate) fn invio_pagine_pdf(&mut self) {
        let Some(ctx) = self.contesto.get().cloned() else { return };
        if self.fogli_pdf.dividi.aperto() {
            self.dividi_pdf(&ctx);
        } else if self.fogli_pdf.proteggi.aperto() {
            self.proteggi_pdf();
        }
    }

    /// Indietro o Annulla mentre si divide o si comprime: il lavoro si ferma.
    fn ferma_file_pdf(&mut self) -> bool {
        let Some(l) = self.file_pdf.as_ref() else { return false };
        if !l.lavoro.fermato() {
            l.lavoro.ferma();
            diagnosi(format!("pdf: {} da fermare", l.quale.nome()));
        }
        true
    }

    /// Indietro in «Pagine e file»: il lavoro, i dialoghi, i fogli, la pagina presa col dito, la
    /// griglia. Vero se ha chiuso qualcosa.
    pub(crate) fn indietro_pagine_pdf(&mut self) -> bool {
        if self.ferma_file_pdf() {
            return true;
        }
        if let Some(p) = self.pdf.as_mut()
            && (std::mem::take(&mut p.pagine.togli) | std::mem::take(&mut p.pagine.pulito) | p.pagine.compresso.take().is_some())
        {
            diagnosi("indietro: dialogo delle pagine chiuso");
            return true;
        }
        if self.campo_pagine_aperto() {
            self.chiudi_campo_pagine();
            self.fogli_pdf.dividi.chiudi();
            self.fogli_pdf.proteggi.chiudi();
            diagnosi("indietro: foglio delle pagine chiuso");
            return true;
        }
        if self.fogli_pdf.bianca.indietro() || self.fogli_pdf.unisci.indietro() || self.fogli_pdf.comprimi.indietro() {
            diagnosi("indietro: foglio delle pagine chiuso");
            return true;
        }
        let Some(o) = self.pdf.as_mut().and_then(|p| p.pagine.organizza.as_mut()) else { return false };
        if o.lascia() {
            diagnosi("indietro: pagina presa lasciata");
        } else {
            self.chiudi_organizza_pdf();
        }
        true
    }

    /// Per le prove: «Organizza pagine» è aperto, con queste pagine scelte.
    #[doc(hidden)]
    pub fn pagine_scelte(&self) -> Option<Vec<usize>> {
        self.pdf.as_ref()?.pagine.organizza.as_ref().map(|o| o.scelte.iter().copied().collect())
    }

    /// Per le prove: un lavoro di «Pagine e file» è in corso.
    #[doc(hidden)]
    pub fn lavoro_file_in_corso(&self) -> bool {
        self.file_pdf.is_some()
    }

    /// Per le prove: le pagine di adesso del PDF del lettore (il motore, non le miniature).
    #[doc(hidden)]
    pub fn misure_del_pdf(&self) -> Vec<tascabile_pdf::Misura> {
        self.pdf.as_ref().map(|p| p.motore.stato().misure.clone()).unwrap_or_default()
    }

    /// Per le prove: il PDF del lettore ha una password.
    #[doc(hidden)]
    pub fn pdf_cifrato(&self) -> Option<bool> {
        self.pdf.as_ref().map(|p| p.motore.stato().cifrato)
    }

    /// Per le prove: un PDF che si può modificare.
    #[doc(hidden)]
    pub fn pdf_si_modifica(&self) -> bool {
        self.pdf.as_ref().is_some_and(|p| si_modifica(p.motore.stato()))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn i_nomi_delle_parti_e_i_messaggi() {
        assert_eq!(nome_della_parte("Lettera", &[0, 1, 2]), "Lettera pagine 1-3");
        assert_eq!(nome_della_parte("Lettera", &[4]), "Lettera pagina 5");
        assert_eq!(pagine(1), "1 pagina");
        assert!(messaggio(&Quale::Comprimi { prima: 1 }, "firma digitale").contains("firmato"));
        assert!(messaggio(&Quale::Unisci { nome: "x".into(), posto: 0 }, "password").contains("password"));
        assert_eq!(messaggio(&Quale::Dividi, "scrittura"), "Non riesco a dividere il PDF.");
        for q in [Quale::Estrai, Quale::Dividi, Quale::Comprimi { prima: 0 }, Quale::Unisci { nome: String::new(), posto: 0 }] {
            for g in ["firma digitale", "permessi del PDF", "password", "illeggibile", "oltre 200 MB", "altro"] {
                let m = messaggio(&q, g);
                assert!(!m.contains('\u{2013}') && !m.contains('\u{2014}') && m.ends_with('.'), "{m}");
            }
        }
    }
}
