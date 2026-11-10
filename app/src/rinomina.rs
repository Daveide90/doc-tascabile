//! Rinominare un file (7/11/2026, segnalazione dell'autore: «non c'è la possibilità di cambiare
//! nome ai file»). Dal menu ⋮ di un recente o di un file in una cartella, e dal menu dell'Editor
//! per il documento aperto: un foglio dal fondo col nome di adesso, senza l'estensione, e
//! «Fatto». Il nome cambia nella stessa cartella e l'estensione resta (`tascabile_files::rinomina`);
//! poi il recente, la cartella aperta e il documento aperto prendono il nome nuovo. Un documento
//! arrivato con «Apri con» appartiene all'app che l'ha mandato: non si rinomina, se ne salva una
//! copia.
//!
//! Nella diagnosi com'è andata, mai il nome.

use std::path::{Path, PathBuf};

use eframe::egui::{self, RichText, vec2};
use tascabile_files::ErroreRinomina;
use tascabile_platform::{TipoTastiera, diagnosi};
use tascabile_touch::tema::{self, Carattere};
use tascabile_touch::{CampoTesto, FoglioDalFondo, Peso, Pulsante};
use tascabile_word::salva::Origine;

use crate::fissa_data::{TimestampRinominato, rinomina_il_timestamp};
use crate::{Errore, Fuoco, TascabileApp, home};

/// Il titolo del foglio, la voce dei menu.
pub(crate) const RINOMINA: &str = home::RINOMINA;
/// Il testo grigio del campo vuoto.
pub(crate) const NOME_NUOVO: &str = "Nome del file";
/// «Fatto» del foglio.
pub(crate) const FATTO: &str = "Fatto";
const SPIEGA: &str = "Il nome nuovo, nella stessa cartella. L'estensione resta quella di prima.";
/// Un documento arrivato con «Apri con».
pub(crate) const DA_ALTRA_APP: &str =
    "Questo documento è arrivato da un'altra app ed è suo: qui non si rinomina. Per dargli un nome tuo, salvane una copia.";
pub(crate) const NOME_PRESO: &str = "C'è già un file con questo nome in questa cartella.";
pub(crate) const NOME_VUOTO: &str = "Scrivi il nome nuovo.";
const NON_RINOMINATO: &str = "Non rinominato";

/// Il foglio del rinomina: il campo col nome e il file da rinominare.
pub(crate) struct StatoRinomina {
    pub(crate) foglio: FoglioDalFondo,
    pub(crate) campo: CampoTesto,
    percorso: Option<PathBuf>,
    /// Il foglio è stato aperto: chiuso toccando fuori o con Indietro, il campo lascia la tastiera.
    aperto: bool,
}

impl Default for StatoRinomina {
    fn default() -> Self {
        StatoRinomina { foglio: FoglioDalFondo::new(egui::Id::new("foglio_rinomina")), campo: CampoTesto::new(), percorso: None, aperto: false }
    }
}

impl StatoRinomina {
    /// Il campo, se il foglio è aperto: riceve la tastiera prima degli altri.
    pub(crate) fn campo_mut(&mut self) -> Option<&mut CampoTesto> {
        self.foglio.aperto().then_some(&mut self.campo)
    }

    pub(crate) fn aperto(&self) -> bool {
        self.aperto
    }
}

impl TascabileApp {
    /// «Rinomina» toccato: per un file il foglio col nome di adesso; per un documento arrivato
    /// da un'altra app, l'avviso.
    pub(crate) fn apri_rinomina(&mut self, chiave: &str, nome: &str) {
        if home::e_uri(chiave) {
            diagnosi("rinomina: documento di un'altra app, non si rinomina");
            self.errore = Some(Errore { titolo: NON_RINOMINATO, testo: DA_ALTRA_APP.to_string() });
            return;
        }
        let percorso = PathBuf::from(chiave);
        let base = Path::new(nome).file_stem().or_else(|| percorso.file_stem()).map(|b| b.to_string_lossy().into_owned()).unwrap_or_default();
        self.rinomina.campo = CampoTesto::new().con_testo(&base);
        self.rinomina.percorso = Some(percorso);
        self.rinomina.foglio.apri();
        self.rinomina.aperto = true;
        diagnosi("rinomina: foglio aperto");
        self.fuoco_al_campo_rinomina();
    }

    /// «Rinomina» dal menu dell'Editor: il file del documento aperto. Un documento nuovo, mai
    /// salvato, non ha ancora un file: lo dice.
    pub(crate) fn rinomina_documento_aperto(&mut self) {
        let Some(e) = self.editor() else { return };
        let nome = e.nome.clone();
        match &e.origine {
            Origine::File(p) => {
                let chiave = p.to_string_lossy().into_owned();
                self.apri_rinomina(&chiave, &nome);
            }
            Origine::Uri { uri, .. } => {
                let uri = uri.clone();
                self.apri_rinomina(&uri, &nome);
            }
            _ => {
                diagnosi("rinomina: documento mai salvato");
                self.avviso_breve.avvisa("Questo documento non è ancora un file: salvalo, e il nome sarà il tuo.");
            }
        }
    }

    /// Il campo del rinomina prende la tastiera, come quello dell'autore.
    pub(crate) fn fuoco_al_campo_rinomina(&mut self) {
        let dopo = self.versioni.versione();
        let st = self.rinomina.campo.prendi_fuoco(dopo);
        self.versioni.continua_dalla_versione(st.versione);
        self.fuoco = Fuoco::Campo;
        self.piattaforma.tipo_tastiera(TipoTastiera::Testo);
        self.piattaforma.set_ime_state(st.testo, st.sel.0, st.sel.1, st.versione);
        self.piattaforma.show_keyboard();
    }

    /// «Fatto» o Invio: il file prende il nome nuovo. Un nome vuoto o già preso lo dice e il
    /// foglio resta aperto, per correggerlo.
    pub(crate) fn conferma_rinomina(&mut self) {
        let Some(vecchio) = self.rinomina.percorso.clone() else { return };
        let nome = self.rinomina.campo.testo().to_string();
        match tascabile_files::rinomina(&vecchio, &nome) {
            Ok(nuovo) => {
                diagnosi("rinomina: fatto");
                let timestamp = self.dopo_rinomina(&vecchio, &nuovo);
                self.chiudi_rinomina();
                let nome = nuovo.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                self.avviso_breve.avvisa(match timestamp {
                    TimestampRinominato::Nessuno => format!("Rinominato: {nome}"),
                    TimestampRinominato::Seguito => format!("Rinominato: {nome}, col suo timestamp"),
                    TimestampRinominato::Rimasto => format!("Rinominato: {nome}. Il suo timestamp è rimasto col nome di prima."),
                });
            }
            Err(ErroreRinomina::Uguale) => {
                diagnosi("rinomina: nome uguale, niente da fare");
                self.chiudi_rinomina();
            }
            Err(ErroreRinomina::Vuoto) => {
                diagnosi("rinomina: nome vuoto");
                self.avviso_breve.avvisa(NOME_VUOTO);
            }
            Err(ErroreRinomina::Esiste) => {
                diagnosi("rinomina: nome già preso");
                self.avviso_breve.avvisa(NOME_PRESO);
            }
            Err(ErroreRinomina::Disco(k)) => {
                diagnosi(format!("rinomina: non fatto ({k:?})"));
                self.chiudi_rinomina();
                self.errore = Some(Errore {
                    titolo: NON_RINOMINATO,
                    testo: "Non riesco a cambiare il nome: forse il file è stato spostato, o in questa cartella non si può scrivere.".to_string(),
                });
            }
        }
    }

    /// Dopo il rinomina: il timestamp del file, che lo segue col nome nuovo, poi il recente e la
    /// cartella aperta (che rilegge i file, col `.ots` già al suo posto), e il documento aperto
    /// se è lui. Com'è andata al timestamp.
    fn dopo_rinomina(&mut self, vecchio: &Path, nuovo: &Path) -> TimestampRinominato {
        let timestamp = rinomina_il_timestamp(vecchio, nuovo, &self.piattaforma.radice_archivio());
        match timestamp {
            TimestampRinominato::Nessuno => {}
            TimestampRinominato::Seguito => diagnosi("rinomina: anche il timestamp"),
            TimestampRinominato::Rimasto => diagnosi("rinomina: il timestamp è rimasto col nome di prima"),
        }
        let vecchia = vecchio.to_string_lossy().into_owned();
        self.home.rinominato(&vecchia, nuovo);
        let nome = nuovo.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if let Some(e) = self.editor_mut()
            && matches!(&e.origine, Origine::File(p) if p == vecchio)
        {
            e.origine = Origine::File(nuovo.to_path_buf());
            e.nome = nome.clone();
            diagnosi("rinomina: anche il documento aperto");
        }
        let nuova = nuovo.to_str().map(str::to_string);
        if let Some(p) = self.pdf.as_mut()
            && p.chiave.as_deref() == Some(vecchia.as_str())
        {
            p.chiave = nuova.clone();
            p.nome = nome.clone();
            diagnosi("rinomina: anche il PDF aperto");
        }
        timestamp
    }

    /// Chiude il foglio del rinomina; il campo lascia la tastiera.
    pub(crate) fn chiudi_rinomina(&mut self) {
        let campo = self.fuoco == Fuoco::Campo;
        self.fuoco_al_documento();
        self.rinomina.foglio.chiudi();
        self.rinomina.aperto = false;
        self.rinomina.percorso = None;
        if campo && self.piattaforma.keyboard_visible() {
            self.piattaforma.hide_keyboard();
        }
    }

    /// Il foglio del rinomina, sopra la schermata di adesso.
    pub(crate) fn mostra_rinomina(&mut self, ctx: &egui::Context) {
        if !self.rinomina.aperto {
            return;
        }
        let campo = &self.rinomina.campo;
        let scelta = self
            .rinomina
            .foglio
            .mostra(ctx, RINOMINA, |ui| {
                let ctx = ui.ctx().clone();
                let mut s = None;
                ui.label(RichText::new(SPIEGA).font(Carattere::Testo.font(&ctx, 15.0)).color(tema::MATITA));
                ui.add_space(12.0);
                if ui.allocate_ui(vec2(ui.available_width(), tema::PULSANTE), |ui| campo.mostra(ui, NOME_NUOVO).clicked()).inner {
                    s = Some(false);
                }
                ui.add_space(14.0);
                if ui.add(Pulsante::new(FATTO, Peso::Principale)).clicked() {
                    s = Some(true);
                }
                ui.add_space(8.0);
                s
            })
            .flatten();
        match scelta {
            Some(true) => self.conferma_rinomina(),
            Some(false) => self.fuoco_al_campo_rinomina(),
            None => {}
        }
        // Chiuso toccando fuori o con Indietro: il campo lascia la tastiera.
        if self.rinomina.aperto && !self.rinomina.foglio.aperto() {
            self.chiudi_rinomina();
        }
    }
}
