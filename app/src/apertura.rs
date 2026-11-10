//! L'apertura di un documento su un thread suo (7/11/2026, segnalazione dell'autore: «se apro
//! docx alcuni lagga, e anche se apro direttamente dei file dalle altre app»). Prima la lettura
//! del file, la ricerca dello stesso file nelle cartelle per «Apri con» e l'apertura di un
//! documento di testo giravano dentro il fotogramma: con un documento lungo, o con un'app lenta a
//! mandare i byte, lo schermo restava fermo (revisione del 3/11, B4). Ora sul thread
//! «documento-apertura» si legge, si riconosce il tipo e, per un documento di testo, lo si apre;
//! lo schermo intanto mostra «Apro il documento» e Indietro lascia andare l'apertura. PDF, fogli
//! di calcolo e slide proseguono sui loro thread di sempre.
//!
//! Nella diagnosi, come prima, tipo e misura del file e com'è andata: mai il nome o l'indirizzo.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, Id, Rect, Sense, WidgetInfo, WidgetType, vec2};
use tascabile_files::{ImprontaFile, Recente, Tipo, tipo_da_nome};
use tascabile_platform::{ErroreLettura, diagnosi};
use tascabile_touch::tema::{self, Carattere};
use tascabile_word::documento;
use tascabile_word::editor::Editor;
use tascabile_word::salva::Origine;

use crate::{Da, Errore, NON_TROVATO, TascabileApp, adesso, home, leggi_file, non_si_apre_piu};

/// La pillola mentre un documento si apre.
pub(crate) const APRO_DOCUMENTO: &str = "Apro il documento";
/// Un recente che non si trova, con l'accesso ai file spento: Android nasconde i file delle altre
/// app e li fa sembrare spariti.
pub(crate) const NON_TROVATO_SENZA_ACCESSO: &str = "Non vedo i file del telefono: l'accesso ai file è spento. Riaccendilo dalla Home e riprova.";
/// Un recente che non si trova né al suo posto né altrove con gli stessi byte (`cerca_per_impronta`).
pub(crate) const NON_TROVATO_NEMMENO_ALTROVE: &str = "Non trovo il file, né al suo posto né altrove nel telefono: forse è stato cancellato o cambiato da un'altra app. Se non c'è più, toglilo dai recenti dal suo menu.";
/// Lo stack del thread: i lettori dei documenti vanno in profondità, come quelli dei fogli.
const STACK: usize = 16 << 20;
/// Un file della memoria che non si trova si riprova dopo un attimo, tre volte, per un secondo e
/// mezzo in tutto: «non trovato» arriva anche per un file che c'è, subito dopo un aggiornamento
/// dell'app o all'avvio, quando la memoria risponde in ritardo (segnalazioni del 1/11 e del 7/11).
const ATTESE_NON_TROVATO: [Duration; 3] = [Duration::from_millis(250), Duration::from_millis(500), Duration::from_millis(750)];

/// Un recente che non si trova al suo posto: si cerca dalla sua impronta nella memoria del
/// telefono (`tascabile_files::cerca_per_impronta`).
pub(crate) struct Ritrova {
    pub(crate) radice: PathBuf,
    pub(crate) impronta: ImprontaFile,
}

/// Per un documento di testo, l'impaginazione da fare già sul thread, come la farà l'Editor al
/// primo fotogramma: vista Scrittura, larga quanto lo schermo, zoom 1, con o senza le
/// sottolineature dell'ortografia. Il motore la tiene, e l'Editor la trova pronta: su un documento
/// di 200 pagine costa un secondo sul PC (7/11).
pub(crate) struct Impagina {
    pub(crate) larghezza: f32,
    pub(crate) ortografia: bool,
    /// Il dizionario si sta caricando: si aspetta, al più `ATTESA_DIZIONARIO`.
    pub(crate) aspetta_dizionario: bool,
}

/// Quanto il thread aspetta al più il dizionario prima di impaginare senza.
const ATTESA_DIZIONARIO: Duration = Duration::from_secs(8);

/// Com'è andata la lettura, oltre ai byte: lo stesso file trovato nelle cartelle per «Apri con»,
/// un recente ritrovato altrove dalla sua impronta, e l'impronta dei byte letti.
#[derive(Default)]
struct Esito {
    stesso_file: bool,
    spostato: bool,
    impronta: Option<ImprontaFile>,
    /// L'impaginazione fatta qui aveva già il dizionario.
    col_dizionario: bool,
}

/// Chi ha chiesto di aprire: decide che cosa si dice se il file non si legge.
pub(crate) enum Richiesta {
    Cartella,
    Recente(Recente),
    ApriCon,
}

/// Da dove vengono i byte.
pub(crate) enum Sorgente {
    /// Un file della memoria del telefono.
    Percorso(PathBuf),
    /// Un URI già aperto da Android (`apri_uri`): sul thread solo la lettura.
    File(std::fs::File),
    /// Niente da leggere (il permesso di un «Apri con» è scaduto, con questo errore): solo la
    /// ricerca dall'impronta.
    Nessuna(ErroreLettura),
}

/// Il documento: come si chiama e dove sta. «Apri con» può ritrovarlo nelle cartelle.
pub(crate) struct Dove {
    pub(crate) nome: String,
    pub(crate) chiave: Option<String>,
    pub(crate) origine: Origine,
}

/// Per «Apri con»: dove cercare lo stesso file, e quale app l'ha mandato.
pub(crate) struct CercaStesso {
    pub(crate) radice: PathBuf,
    pub(crate) da_app: Option<&'static str>,
}

/// Che cosa torna dal thread.
enum Pronto {
    /// Un documento di testo, aperto.
    Testo {
        sessione: Box<wordcraft_engine::Session>,
        misura: usize,
    },
    /// PDF, foglio di calcolo, presentazione: i byte, per il loro thread.
    Altro {
        byte: Vec<u8>,
    },
    NonLetto(ErroreLettura),
    NonAperto {
        misura: usize,
        errore: documento::ErroreApertura,
    },
}

/// Un documento che si apre sul suo thread.
pub(crate) struct AperturaDocumento {
    esito: Receiver<(Pronto, Dove, Esito)>,
    da: Da,
    richiesta: Richiesta,
    inizio: Instant,
}

/// Legge un file della memoria; se non si trova, riprova dopo un attimo (`ATTESE_NON_TROVATO`).
fn leggi_con_attese(p: &std::path::Path) -> Result<Vec<u8>, ErroreLettura> {
    let mut letti = leggi_file(p);
    for attesa in ATTESE_NON_TROVATO {
        match &letti {
            Err(e) if e.genere == "NotFound" => {
                std::thread::sleep(attesa);
                letti = leggi_file(p);
            }
            _ => break,
        }
    }
    letti
}

/// Il lavoro del thread: legge (o ritrova dall'impronta un recente che non è più al suo posto),
/// cerca lo stesso file nelle cartelle per «Apri con», riconosce il tipo, apre il testo.
fn lavora(
    sorgente: Sorgente,
    mut dove: Dove,
    cerca: Option<CercaStesso>,
    ritrova: Option<Ritrova>,
    impagina: Option<Impagina>,
) -> (Pronto, Dove, Esito) {
    let mut esito = Esito::default();
    let letti = match sorgente {
        Sorgente::Percorso(p) => leggi_con_attese(&p),
        Sorgente::File(file) => tascabile_platform::leggi_con_tetto(file, "URI"),
        Sorgente::Nessuna(errore) => Err(errore),
    };
    let byte = match letti {
        Ok(byte) => byte,
        // Non c'è più al suo posto: lo stesso file, con gli stessi byte, altrove nel telefono.
        Err(e) if e.genere == "NotFound" || non_si_apre_piu(&e.genere) => {
            let trovato =
                ritrova.and_then(|r| tascabile_files::cerca_per_impronta(&r.radice, &r.impronta)).and_then(|p| leggi_file(&p).ok().map(|b| (p, b)));
            let Some((percorso, byte)) = trovato else { return (Pronto::NonLetto(e), dove, esito) };
            esito.spostato = true;
            if let Some(n) = percorso.file_name().and_then(|n| n.to_str()) {
                dove.nome = n.to_string();
            }
            dove.chiave = percorso.to_str().map(str::to_string);
            dove.origine = Origine::File(percorso);
            byte
        }
        Err(e) => return (Pronto::NonLetto(e), dove, esito),
    };
    esito.impronta = Some(ImprontaFile::di(&byte));
    // Lo stesso file nelle cartelle del telefono (WhatsApp, Telegram, Download): si apre da lì,
    // così resta nei recenti anche quando il permesso di «Apri con» se ne va, e Salva scrive nel
    // file (segnalazione dell'autore del 3/11). Solo un file con gli stessi byte.
    if let Some(c) = cerca
        && let Some(percorso) = tascabile_files::lo_stesso_file(&c.radice, &dove.nome, &byte, c.da_app)
    {
        esito.stesso_file = true;
        if let Some(n) = percorso.file_name().and_then(|n| n.to_str()) {
            dove.nome = n.to_string();
        }
        dove.chiave = percorso.to_str().map(str::to_string);
        dove.origine = Origine::File(percorso);
    }
    let nome = dove.nome.as_str();
    // Anche un file che si chiama PDF (o arriva col tipo del PDF) ma non comincia con %PDF va
    // al lettore PDF, che dice in italiano che è vuoto o rovinato (analisi del 27/10, lettura I4).
    // I fogli di calcolo per nome, o per contenuto se il nome non dice niente (revisione del 3/11,
    // B3); le presentazioni per nome o per contenuto.
    let altro = tascabile_pdf::sembra_pdf(&byte)
        || tipo_da_nome(nome) == Tipo::Pdf
        || tipo_da_nome(nome) == Tipo::Foglio
        || (tipo_da_nome(nome) == Tipo::Altro && tascabile_foglio::sembra_foglio(&byte))
        || tipo_da_nome(nome) == Tipo::Presentazione
        || tascabile_slide::sembra_presentazione(&byte);
    if altro {
        return (Pronto::Altro { byte }, dove, esito);
    }
    let misura = byte.len();
    let pronto = match documento::apri_byte(nome, &byte) {
        Ok(mut sessione) => {
            if let Some(i) = impagina {
                if i.ortografia && i.aspetta_dizionario {
                    let fine = Instant::now() + ATTESA_DIZIONARIO;
                    while !crate::dizionario::INSTALLATO.load(std::sync::atomic::Ordering::SeqCst) && Instant::now() < fine {
                        std::thread::sleep(Duration::from_millis(20));
                    }
                }
                esito.col_dizionario = crate::dizionario::INSTALLATO.load(std::sync::atomic::Ordering::SeqCst);
                sessione.view.proofing = i.ortografia;
                let _ = tascabile_word::vista::layout_scrittura(&mut sessione, i.larghezza, 1.0);
            }
            Pronto::Testo { sessione: Box::new(sessione), misura }
        }
        Err(errore) => Pronto::NonAperto { misura, errore },
    };
    (pronto, dove, esito)
}

impl TascabileApp {
    /// Apre un documento sul thread «documento-apertura»: arriva in
    /// [`passo_apertura_documento`](Self::passo_apertura_documento). Un'apertura di prima, non
    /// ancora arrivata, si lascia andare.
    pub(crate) fn avvia_apertura(
        &mut self,
        da: Da,
        richiesta: Richiesta,
        sorgente: Sorgente,
        dove: Dove,
        cerca: Option<CercaStesso>,
        ritrova: Option<Ritrova>,
    ) {
        // Un altro documento al posto del PDF che chiedeva la password, e del foglio di calcolo
        // che si stava aprendo: finito dopo, chiuderebbe questo (revisione del 3/11).
        self.annulla_password();
        self.apertura_foglio = None;
        let inizio = Instant::now();
        // Un documento di testo: il dizionario parte subito, in parallelo alla lettura, e il
        // thread lo aspetta prima di impaginare.
        if tipo_da_nome(&dove.nome) == Tipo::Documento {
            self.carica_dizionario();
        }
        // La larghezza del foglio dell'Editor: lo schermo meno i margini della pagina.
        let impagina = self.contesto.get().map(|c| Impagina {
            larghezza: crate::schermo_editor::larghezza_del_foglio(c.content_rect().width()),
            ortografia: self.impostazioni.ortografia,
            aspetta_dizionario: self.dizionario_in_carico(),
        });
        let (manda, esito) = std::sync::mpsc::channel();
        let risveglio = self.risveglio.clone();
        let lavoro = move || {
            let _ = manda.send(lavora(sorgente, dove, cerca, ritrova, impagina));
            risveglio();
        };
        if let Err(e) = std::thread::Builder::new().name("documento-apertura".into()).stack_size(STACK).spawn(lavoro) {
            da.scrivi(format!("{}: thread di apertura non partito ({:?})", da.etichetta(), e.kind()));
            self.errore = Some(Errore::apertura("Non riesco ad aprire il file: il telefono non ha lasciato partire la lettura."));
            return;
        }
        // Mentre si apre, la tastiera si chiude e il documento di prima non la riceve più.
        self.piattaforma.hide_keyboard();
        self.fuoco_al_documento();
        if let Some(e) = self.editor_mut() {
            e.ponte.chiudi();
        }
        self.apertura_documento = Some(AperturaDocumento { esito, da, richiesta, inizio });
    }

    /// Per le prove: un documento si sta aprendo.
    #[doc(hidden)]
    pub fn documento_in_apertura(&self) -> bool {
        self.apertura_documento.is_some()
    }

    /// A ogni passo: il documento che si apre, se è pronto, va avanti. Un testo nell'Editor; un
    /// PDF, un foglio di calcolo o una presentazione al loro thread; un file che non si legge,
    /// l'avviso giusto per chi l'aveva chiesto.
    pub(crate) fn passo_apertura_documento(&mut self) {
        let Some(a) = &self.apertura_documento else { return };
        let (pronto, dove, esito) = match a.esito.try_recv() {
            Ok(arrivato) => arrivato,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                let a = self.apertura_documento.take();
                if let Some(a) = a {
                    a.da.scrivi(format!("{}: il thread di apertura si è fermato", a.da.etichetta()));
                }
                self.errore = Some(Errore::apertura("Non riesco ad aprire il file: la lettura si è interrotta."));
                return;
            }
        };
        let Some(a) = self.apertura_documento.take() else { return };
        if esito.stesso_file {
            diagnosi("apri con: lo stesso file è nelle cartelle del telefono, aperto da lì");
        }
        // Un recente ritrovato altrove: la voce vecchia se ne va, quella nuova arriva all'apertura.
        if esito.spostato {
            diagnosi("recenti: non era più al suo posto, ritrovato dall'impronta");
            if let Richiesta::Recente(r) = &a.richiesta {
                self.home.dimentica(&r.chiave);
            }
        }
        if let (Some(impronta), Some(chiave)) = (esito.impronta, &dove.chiave) {
            self.home.segna_impronta(chiave, impronta);
        }
        let etichetta = a.da.etichetta();
        let tipo = documento::estensione_nota(&dove.nome).unwrap_or_else(|| "sconosciuto".to_string());
        match pronto {
            Pronto::Testo { sessione, misura } => {
                a.da.scrivi(format!(
                    "{etichetta}: {misura} byte, tipo {tipo}, aperto in {:.0} ms, {} paragrafi",
                    a.inizio.elapsed().as_secs_f64() * 1000.0,
                    sessione.doc.paragraph_count()
                ));
                let mut editor = Editor::con_origine(*sessione, &dove.nome, dove.origine);
                editor.aperto_alle = a.inizio;
                self.cambia_documento(editor);
                // Se il dizionario arriva dopo, questo documento non si reimpagina: lo aveva già.
                self.impaginato_col_dizionario = esito.col_dizionario;
                // Il motore salta commenti e note di .odt e .rtf, e Salva li scrive in un .docx
                // accanto: va detto subito, prima che si cerchino.
                if matches!(tipo.as_str(), "odt" | "rtf") {
                    self.avviso_breve.avvisa("Di questo file qui non si vedono tutte le parti, come commenti e note: Salva fa un .docx accanto.");
                }
                match dove.chiave {
                    Some(chiave) => self.home.ricorda(Recente {
                        chiave,
                        nome: dove.nome.clone(),
                        tipo: tipo_da_nome(&dove.nome),
                        aperto_alle: adesso(),
                        pagine: None,
                        impronta: None,
                    }),
                    None => diagnosi("recenti: percorso che non è UTF-8, non ricordato"),
                }
            }
            Pronto::Altro { byte } => {
                let nome = dove.nome.as_str();
                if tascabile_pdf::sembra_pdf(&byte) || tipo_da_nome(nome) == Tipo::Pdf {
                    self.apri_pdf(a.da, byte, nome, dove.chiave, a.inizio);
                } else if tipo_da_nome(nome) == Tipo::Foglio || (tipo_da_nome(nome) == Tipo::Altro && tascabile_foglio::sembra_foglio(&byte)) {
                    self.apri_foglio(a.da, byte, nome, dove.chiave, a.inizio);
                } else {
                    self.apri_slide(a.da, byte, nome, dove.chiave, a.inizio);
                }
            }
            Pronto::NonAperto { misura, errore } => {
                // Il genere sì (zip troppo grande, protetto da password), il messaggio no: quello
                // del motore può riportare il nome del file.
                a.da.scrivi(format!("{etichetta}: {misura} byte, tipo {tipo}, errore di apertura ({})", errore.genere));
                self.errore = Some(Errore::apertura(format!("Non riesco ad aprire il file: {errore}")));
            }
            Pronto::NonLetto(e) => {
                a.da.scrivi(format!("{etichetta}: errore di lettura ({}), tipo {tipo}", e.genere));
                match a.richiesta {
                    Richiesta::Recente(r) => self.recente_non_letto(r, e),
                    Richiesta::Cartella | Richiesta::ApriCon => {
                        self.errore = Some(Errore::apertura(format!("Non riesco ad aprire il file: {e}")));
                    }
                }
            }
        }
    }

    /// Un recente che non si è letto. Un URI che non si legge più (il permesso di un «Apri con»
    /// non dura) esce dai recenti, e l'avviso dice da dove riaprirlo. Un file che non si trova
    /// invece resta: si toglie dal suo menu.
    pub(crate) fn recente_non_letto(&mut self, r: Recente, e: ErroreLettura) {
        let uri = home::e_uri(&r.chiave);
        if uri && non_si_apre_piu(&e.genere) {
            self.home.dimentica(&r.chiave);
            diagnosi("recenti: tolto un documento arrivato da un'altra app, che non si apre più");
            let mut messaggio = match home::provenienza(&r.chiave) {
                Some(app) => format!("Non si apre più: riaprilo da {app}."),
                None => "Non si apre più: riaprilo dall'app da cui era arrivato.".to_string(),
            };
            // Un file con lo stesso nome nelle cartelle del telefono si dice, non si apre: può
            // essere un altro documento, e Salva ci scriverebbe sopra (revisione del 3/11, M3).
            if self.home.accesso()
                && let Some(cartella) = tascabile_files::cartella_dell_omonimo(&self.piattaforma.radice_archivio(), &r.nome)
            {
                diagnosi("recenti: un file con lo stesso nome nelle cartelle del telefono, detto e non aperto");
                messaggio.push_str(&format!(" In «{cartella}» c'è un file con lo stesso nome: se è lui, aprilo da lì."));
            }
            self.errore = Some(Errore::apertura(messaggio));
        } else if !uri && e.genere == "NotFound" {
            // Il perché nel registro, senza nomi: con l'accesso ai file spento Android fa sembrare
            // sparito ogni file delle altre app; con l'impronta, il file non è nemmeno altrove.
            let accesso = self.home.accesso();
            let impronta = r.impronta.is_some();
            diagnosi(format!(
                "recenti: file non trovato, accesso ai file {}, impronta {}, lasciato nei recenti",
                if accesso { "sì" } else { "no" },
                if impronta { "nota" } else { "assente" }
            ));
            let mut messaggio = if !accesso {
                NON_TROVATO_SENZA_ACCESSO.to_string()
            } else if impronta {
                NON_TROVATO_NEMMENO_ALTROVE.to_string()
            } else {
                NON_TROVATO.to_string()
            };
            if accesso && let Some(cartella) = tascabile_files::cartella_dell_omonimo(&self.piattaforma.radice_archivio(), &r.nome) {
                diagnosi("recenti: un file con lo stesso nome nelle cartelle del telefono, detto e non aperto");
                messaggio.push_str(&format!(" In «{cartella}» c'è un file con lo stesso nome: se è lui, aprilo da lì."));
            }
            self.errore = Some(Errore::apertura(messaggio));
        } else {
            self.errore = Some(Errore::apertura(format!("Non riesco ad aprire il file: {e}")));
        }
    }

    /// Indietro mentre un documento si apre: l'apertura si lascia andare. Vero se c'era.
    pub(crate) fn lascia_l_apertura(&mut self) -> bool {
        if self.apertura_documento.take().is_some() {
            diagnosi("indietro: apertura del documento annullata");
            return true;
        }
        false
    }

    /// Mentre un documento si apre, sopra tutto la pillola «Apro il documento»: i tocchi sotto
    /// non arrivano.
    pub(crate) fn mostra_apertura_documento(&self, ctx: &egui::Context) {
        if self.apertura_documento.is_none() {
            return;
        }
        let zona = ctx.content_rect();
        egui::Area::new(Id::new("apertura_documento")).order(egui::Order::Foreground).fixed_pos(zona.min).show(ctx, |ui| {
            ui.allocate_rect(Rect::from_min_size(zona.min, zona.size()), Sense::click_and_drag());
            let g = ui.painter().layout_no_wrap(APRO_DOCUMENTO.to_string(), Carattere::Grassetto.font(ui.ctx(), 15.0), Color32::WHITE);
            let pillola = Rect::from_center_size(zona.center(), vec2(g.size().x + 32.0, 44.0));
            ui.painter().rect_filled(pillola, 22.0, tema::INDICATORE);
            ui.painter().galley(pillola.center() - g.size() / 2.0, g, Color32::PLACEHOLDER);
            ui.interact(pillola, Id::new("apertura_documento_testo"), Sense::hover())
                .widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, APRO_DOCUMENTO));
            ctx.request_repaint_after(Duration::from_millis(100));
        });
    }
}
