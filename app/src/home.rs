//! La Home del bozzetto 1: titolo e data, la ricerca, i filtri per tipo, i recenti col loro menu
//! (Apri, Condividi, Togli dai recenti), le cartelle del telefono in griglia col numero di file, il
//! pulsante «Nuovo» col suo foglio e l'icona delle Impostazioni. Toccando una cartella si apre la
//! sua schermata: la barra alta col percorso e le voci, «..» compresa.
//!
//! Il disco si legge solo quando serve (all'avvio, entrando in una cartella, tornando nella Home,
//! quando l'app ritrova il fuoco), mai a ogni frame. Nella diagnosi solo misure: mai nomi di file
//! o di cartelle, né quello che si cerca, che possono essere personali.

use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use eframe::egui::{self, Color32, Frame, Margin, Rect, RichText, ScrollArea, Sense, Ui, UiBuilder, Vec2, pos2, vec2};
use tascabile_files::{Recente, Recenti, Scorciatoia, Tipo, Voce, elenca, elenca_tutto, nome_contiene, scorciatoie};
use tascabile_platform::diagnosi;
use tascabile_touch::tema::{self, Carattere};
use tascabile_touch::{
    BarraAlta, CampoTesto, Chip, FoglioDalFondo, Menu, Peso, Pulsante, PulsanteIcona, PulsanteNuovo, SceltaMenu, Scheda, SchedaCartella, icone,
    titoletto,
};

use tascabile_word::modelli::Modello;

use crate::quando::{self, Locale};

/// Il file dei recenti, nella cartella dell'app.
pub const FILE_RECENTI: &str = "recenti.json";

/// Il nome della radice dei documenti nella Home.
const NOME_RADICE: &str = "Memoria del telefono";

/// Quanti recenti mostra la Home prima di «Vedi tutti», come il bozzetto.
pub const RECENTI_IN_HOME: usize = 4;

/// Quanti file trovati nelle cartelle si mostrano al massimo: oltre, conviene scrivere di più.
pub const TROVATI_AL_MASSIMO: usize = 50;

/// Il testo grigio della ricerca vuota.
pub const CERCA: &str = "Cerca nei documenti";
/// La ricerca dentro una cartella aperta (l'autore, 3/11: «metti un pulsante cerca»).
pub const CERCA_IN_CARTELLA: &str = "Cerca in questa cartella";
pub const CERCA_PULSANTE: &str = "Cerca";
/// Le due viste di una cartella (l'autore, 3/11: «fai decidere se voler visualizzare le cartelle
/// con sotto cartelle oppure tutte insieme»).
pub const VISTA_CARTELLE: &str = "Con le cartelle";
pub const VISTA_TUTTI: &str = "Tutti i file";
/// Il file della vista scelta, nella cartella dell'app.
pub const FILE_VISTA: &str = "vista-cartelle";
/// I file al più nella vista «Tutti i file» e nella ricerca di una cartella.
const TUTTI_AL_MASSIMO: usize = 2000;
/// Gli avvisi brevi della Home stanno sopra il «Nuovo»: tanto dal fondo della zona libera (il
/// pulsante è a 18 dp dal fondo, alto `tema::NUOVO`).
pub const SOPRA_IL_NUOVO: f32 = tema::MARGINE_LARGO + tema::NUOVO;

/// L'etichetta del pulsante del menu di un recente (per l'accessibilità e le prove), col nome
/// come lo mostra la Home.
pub fn etichetta_menu(nome: &str) -> String {
    format!("Altre azioni: {nome}")
}

/// Il nome di un documento nella Home, come nel bozzetto: senza l'estensione, che il riquadro del
/// tipo dice già («Relazione turno notte»). Un file d'altro tipo resta com'è, e anche un nome che
/// è solo un'estensione. Nelle cartelle i nomi restano interi.
pub fn nome_da_mostrare(nome: &str, tipo: Tipo) -> &str {
    match nome.rsplit_once('.') {
        Some((base, _)) if tipo != Tipo::Altro && !base.trim().is_empty() => base,
        _ => nome,
    }
}

/// Una chiave dei recenti che è un URI (content://, file://) e non un percorso.
pub fn e_uri(chiave: &str) -> bool {
    chiave.contains("://")
}

/// L'app da cui è arrivato un URI content://, se la si riconosce dall'autorità: serve a dire da
/// dove riaprirlo quando il permesso di leggerlo è scaduto.
pub fn provenienza(uri: &str) -> Option<&'static str> {
    let autorita = uri.strip_prefix("content://")?.split('/').next()?;
    let di = |pacchetto: &str| autorita == pacchetto || autorita.starts_with(&format!("{pacchetto}."));
    if di("com.whatsapp") {
        Some("WhatsApp")
    } else if di("org.telegram") {
        Some("Telegram")
    } else {
        None
    }
}

/// L'icona di una scorciatoia, dal suo nome (`tascabile_files::scorciatoie`).
fn icona_della_scorciatoia(nome: &str) -> &'static str {
    match nome {
        "Download" => icone::DOWNLOAD,
        "Documenti/Tascabile" => icone::FOLDER_OPEN,
        "WhatsApp" => icone::CHAT,
        "Telegram" => icone::SEND,
        _ => icone::FOLDER,
    }
}

/// Il nome di una scorciatoia nella griglia: l'ultimo pezzo («Tascabile» per «Documenti/Tascabile»),
/// che in una scheda larga metà schermo ci sta.
fn nome_breve(nome: &str) -> &str {
    nome.rsplit('/').next().unwrap_or(nome)
}

/// Un file di questo tipo si apre: i documenti di testo e, dalla tappa D, i PDF.
pub fn si_apre(tipo: Tipo) -> bool {
    matches!(tipo, Tipo::Documento | Tipo::Pdf | Tipo::Presentazione | Tipo::Foglio)
}

/// Il tipo di file della voce di elenco.
fn tipo_della_voce(tipo: Tipo) -> tascabile_touch::Tipo {
    match tipo {
        Tipo::Documento => tascabile_touch::Tipo::Word,
        Tipo::Pdf => tascabile_touch::Tipo::Pdf,
        Tipo::Presentazione => tascabile_touch::Tipo::Slide,
        Tipo::Foglio => tascabile_touch::Tipo::Foglio,
        Tipo::Altro => tascabile_touch::Tipo::Altro,
    }
}

/// «24 file», «1 file», «Nessun file»; «Non si legge» se la cartella non si è letta.
fn numero_di_file(n: Option<usize>) -> String {
    match n {
        None => "Non si legge".to_string(),
        Some(0) => "Nessun file".to_string(),
        Some(n) => format!("{n} file"),
    }
}

/// La misura di un file: «512 byte», «24 KB», «1,2 MB».
pub fn misura(byte: u64) -> String {
    const KB: u64 = 1024;
    match byte {
        b if b < KB => format!("{b} byte"),
        b if b < KB * KB => format!("{} KB", b.div_ceil(KB)),
        b => format!("{:.1} MB", b as f64 / (KB * KB) as f64).replace('.', ","),
    }
}

/// Secondi UTC di un istante del disco.
fn secondi(t: SystemTime) -> u64 {
    t.duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Il filtro per tipo dei chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filtro {
    Tutti,
    Documenti,
    Pdf,
    Slide,
    Fogli,
}

impl Filtro {
    /// I chip: prima i PDF, poi i documenti e le slide (l'autore, 3/11).
    pub const CHIP: [(Filtro, &'static str); 5] =
        [(Filtro::Tutti, "Tutti"), (Filtro::Pdf, "PDF"), (Filtro::Documenti, "Documenti"), (Filtro::Slide, "Slide"), (Filtro::Fogli, "Fogli")];

    fn lascia(self, tipo: Tipo) -> bool {
        match self {
            Filtro::Tutti => true,
            Filtro::Documenti => tipo == Tipo::Documento,
            Filtro::Pdf => tipo == Tipo::Pdf,
            Filtro::Slide => tipo == Tipo::Presentazione,
            Filtro::Fogli => tipo == Tipo::Foglio,
        }
    }
}

/// Un file passa la ricerca: il tipo va col filtro e il nome ha le parole cercate.
pub fn passa(nome: &str, tipo: Tipo, cercato: &str, filtro: Filtro) -> bool {
    filtro.lascia(tipo) && nome_contiene(nome, cercato)
}

/// La prima voce del foglio di «Nuovo».
pub(crate) const DOCUMENTO_VUOTO: &str = "Documento vuoto";

/// Quello che l'utente ha chiesto alla Home e che fa l'app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Azione {
    /// «Documento vuoto» nel foglio di «Nuovo».
    Nuovo,
    /// Un modello nel foglio di «Nuovo»: Verbale, Relazione, Lettera, Lezione.
    NuovoDaModello(Modello),
    /// «Scansiona» nel foglio di «Nuovo»: foto dei fogli in un PDF col testo (P8).
    Scansiona,
    /// Un documento toccato in una cartella o fra quelli trovati.
    ApriFile {
        percorso: PathBuf,
        nome: String,
    },
    ApriRecente(Recente),
    CondividiRecente(Recente),
    ConcediAccesso,
    Impostazioni,
    /// Il campo della ricerca toccato: prende la tastiera.
    Cerca,
    /// La X della ricerca: il campo va svuotato (e la tastiera, se è sua, lo deve sapere).
    SvuotaRicerca,
}

/// Quello che la Home deve sapere dall'app per disegnarsi.
pub struct Contesto {
    /// La tastiera è aperta: il «Nuovo» non si mostra, coprirebbe quello che si è trovato.
    pub tastiera: bool,
    /// Adesso, in secondi UTC: la data in testa e il «quando» dei recenti.
    pub adesso: u64,
}

/// La cartella aperta, letta quando ci si è entrati.
struct Cartella {
    percorso: PathBuf,
    /// Il contenuto, oppure l'errore della lettura: mai un elenco a metà.
    voci: io::Result<Vec<Voce>>,
    /// Tutti i file anche delle sottocartelle, letti la prima volta che servono.
    tutti: Option<Vec<Voce>>,
}

/// Che cosa si è toccato in una voce dei recenti.
enum ToccoRecente {
    Apri,
    /// Il pulsante del menu, col suo posto.
    Menu(Rect),
}

pub struct Home {
    /// Da dove partono le cartelle (sul telefono la memoria condivisa): sopra non si sale.
    radice: PathBuf,
    file_recenti: PathBuf,
    recenti: Recenti,
    accesso: bool,
    /// Le scorciatoie che esistono; senza l'accesso ai file non si mostrano, ma servono lo stesso
    /// a dire dove sta un recente.
    scorciatoie: Vec<Scorciatoia>,
    /// I file del primo livello di ogni scorciatoia (None se non si è letta), letti con
    /// `aggiorna`: il numero nelle schede e i file della ricerca.
    file: Vec<Option<Vec<Voce>>>,
    cartella: Option<Cartella>,
    /// Le cartelle aperte prima di quella sullo schermo: la freccia indietro ci torna.
    storia: Vec<PathBuf>,
    ricerca: CampoTesto,
    filtro: Filtro,
    /// Nella cartella aperta: tutti i file insieme invece delle sottocartelle, e la ricerca.
    tutti_insieme: bool,
    cerca_in_cartella: bool,
    file_vista: PathBuf,
    /// «Vedi tutti» toccato: tutti i recenti invece dei primi quattro.
    tutti_i_recenti: bool,
    /// Il menu aperto di un recente: la sua chiave e dove sta l'angolo in alto a destra.
    menu: Option<(String, egui::Pos2)>,
    foglio_nuovo: FoglioDalFondo,
}

impl Home {
    /// La Home con i recenti scritti in `cartella_app` e le scorciatoie che ci sono sotto `radice`.
    pub fn nuova(radice: PathBuf, cartella_app: &Path, accesso: bool) -> Self {
        let file_recenti = cartella_app.join(FILE_RECENTI);
        let recenti = Recenti::carica(&file_recenti);
        let mut home = Self {
            radice,
            file_recenti,
            recenti,
            accesso,
            scorciatoie: Vec::new(),
            file: Vec::new(),
            cartella: None,
            storia: Vec::new(),
            ricerca: CampoTesto::new(),
            filtro: Filtro::Tutti,
            tutti_insieme: std::fs::read_to_string(cartella_app.join(FILE_VISTA)).is_ok_and(|v| v.trim() == "tutti"),
            cerca_in_cartella: false,
            file_vista: cartella_app.join(FILE_VISTA),
            tutti_i_recenti: false,
            menu: None,
            foglio_nuovo: FoglioDalFondo::new(egui::Id::new("foglio_nuovo")),
        };
        home.aggiorna();
        diagnosi(format!("home: {} recenti, {} scorciatoie", home.recenti.voci().len(), home.scorciatoie.len()));
        home
    }

    pub fn accesso(&self) -> bool {
        self.accesso
    }

    /// I recenti, il più nuovo prima (anche per i PDF da unire).
    pub fn recenti(&self) -> &[Recente] {
        self.recenti.voci()
    }

    /// L'accesso ai file appena ricontrollato: decide il riquadro e se le cartelle si mostrano.
    pub fn imposta_accesso(&mut self, accesso: bool) {
        self.accesso = accesso;
    }

    /// Il campo della ricerca, che riceve la tastiera quando ha il fuoco.
    pub fn ricerca_mut(&mut self) -> &mut CampoTesto {
        &mut self.ricerca
    }

    #[cfg(test)]
    pub fn ricerca(&self) -> &CampoTesto {
        &self.ricerca
    }

    /// Rilegge dal disco le scorciatoie e, con l'accesso ai file, i loro file e la cartella aperta:
    /// tornando nella Home o nell'app può essere cambiato qualcosa. Le cartelle grandi sulla
    /// memoria condivisa costano: solo con la Home sullo schermo.
    pub fn aggiorna(&mut self) {
        let inizio = Instant::now();
        self.scorciatoie = scorciatoie(&self.radice);
        self.file = if self.accesso {
            // Solo i file che l'app apre: gli altri non si vedono (l'autore, 3/11: «se un file non lo
            // leggiamo deve sparire»).
            self.scorciatoie.iter().map(|s| elenca(&s.percorso).ok().map(|v| v.into_iter().filter(|v| !v.cartella && si_apre(v.tipo)).collect())).collect()
        } else {
            Vec::new()
        };
        if self.accesso {
            let n: usize = self.file.iter().flatten().map(Vec::len).sum();
            diagnosi(format!("home: {} scorciatoie lette, {n} file, in {:.0} ms", self.file.len(), inizio.elapsed().as_secs_f64() * 1000.0));
        }
        if self.accesso
            && let Some(c) = self.cartella.take()
        {
            self.leggi(c.percorso);
        }
    }

    /// Mette `r` in cima ai recenti e li scrive su disco.
    pub fn ricorda(&mut self, r: Recente) {
        self.recenti.aggiungi(r);
        self.salva_recenti();
    }

    /// Toglie dai recenti la voce con questa chiave e li scrive su disco.
    pub fn dimentica(&mut self, chiave: &str) {
        self.recenti.togli(chiave);
        self.salva_recenti();
    }

    fn salva_recenti(&self) {
        match self.recenti.salva(&self.file_recenti) {
            Ok(()) => diagnosi(format!("recenti: salvati, {} voci", self.recenti.voci().len())),
            Err(e) => diagnosi(format!("recenti: non salvati ({:?})", e.kind())),
        }
    }

    /// Dove sta un recente, per la riga sotto il nome: la cartella, oppure l'app da cui è arrivato.
    /// Anche per la barra alta dell'Editor («Salvato · Documenti»).
    pub fn dove(&self, chiave: &str) -> String {
        self.luogo(chiave).1
    }

    /// L'icona e il nome del posto di un recente: la cartella (con l'icona della sua scorciatoia),
    /// oppure l'app da cui è arrivato.
    fn luogo(&self, chiave: &str) -> (&'static str, String) {
        if e_uri(chiave) {
            return match provenienza(chiave) {
                Some(app) => (icona_della_scorciatoia(app), format!("da {app}")),
                None => (icone::APPS, "da un'altra app".to_string()),
            };
        }
        let cartella = Path::new(chiave).parent().unwrap_or(Path::new(""));
        let icona = match self.scorciatoia_vicina(cartella) {
            Some(s) => icona_della_scorciatoia(&s.nome),
            None if cartella == self.radice => icone::PHONE_ANDROID,
            None => icone::FOLDER,
        };
        (icona, self.nome_cartella(cartella))
    }

    /// La scorciatoia più vicina che contiene `cartella`.
    fn scorciatoia_vicina(&self, cartella: &Path) -> Option<&Scorciatoia> {
        self.scorciatoie.iter().filter(|s| cartella.starts_with(&s.percorso)).max_by_key(|s| s.percorso.components().count())
    }

    /// Il nome di una cartella come lo mostra la Home: dalla scorciatoia più vicina
    /// («Documenti/Lavoro»), oppure dalla radice («Lavoro»; la radice stessa è «Memoria del
    /// telefono»); fuori dalla radice, il percorso com'è.
    fn nome_cartella(&self, cartella: &Path) -> String {
        let unisci = |testa: &str, resto: &Path| {
            let mut pezzi: Vec<String> = Vec::new();
            if !testa.is_empty() {
                pezzi.push(testa.to_string());
            }
            pezzi.extend(resto.components().map(|c| c.as_os_str().to_string_lossy().into_owned()));
            pezzi.join("/")
        };
        if let Some(s) = self.scorciatoia_vicina(cartella)
            && let Ok(resto) = cartella.strip_prefix(&s.percorso)
        {
            return unisci(&s.nome, resto);
        }
        match cartella.strip_prefix(&self.radice) {
            Ok(resto) if resto.as_os_str().is_empty() => NOME_RADICE.to_string(),
            Ok(resto) => unisci("", resto),
            Err(_) => cartella.display().to_string(),
        }
    }

    /// Apre `percorso` nella schermata della cartella, leggendolo dal disco; quella di prima va
    /// nella storia, per la freccia indietro.
    pub fn entra(&mut self, percorso: PathBuf) {
        if let Some(c) = self.cartella.take() {
            self.storia.push(c.percorso);
        }
        self.leggi(percorso);
    }

    /// Chiude la ricerca nella cartella e la svuota (il campo è lo stesso della Home).
    fn chiudi_ricerca_in_cartella(&mut self) {
        if std::mem::take(&mut self.cerca_in_cartella) {
            let _ = self.ricerca.svuota();
        }
    }

    /// La vista delle cartelle: tutti i file insieme o le sottocartelle; resta per le volte dopo.
    fn scegli_vista(&mut self, tutti: bool) {
        if self.tutti_insieme == tutti {
            return;
        }
        self.tutti_insieme = tutti;
        diagnosi(format!("cartella: vista {}", if tutti { "tutti i file" } else { "con le cartelle" }));
        if let Err(e) = std::fs::write(&self.file_vista, if tutti { "tutti" } else { "cartelle" }) {
            diagnosi(format!("cartella: vista non salvata ({:?})", e.kind()));
        }
    }

    /// Legge `percorso` e lo mette sullo schermo, senza toccare la storia.
    fn leggi(&mut self, percorso: PathBuf) {
        let voci = elenca(&percorso);
        match &voci {
            Ok(v) => {
                let cartelle = v.iter().filter(|v| v.cartella).count();
                diagnosi(format!("cartella: {cartelle} cartelle e {} file", v.len() - cartelle));
            }
            Err(e) => diagnosi(format!("cartella: errore di lettura ({:?})", e.kind())),
        }
        self.cartella = Some(Cartella { percorso, voci, tutti: None });
    }

    /// Si può salire di una cartella: solo dentro la radice.
    pub fn si_risale(&self) -> bool {
        self.cartella.as_ref().is_some_and(|c| c.percorso != self.radice && c.percorso.starts_with(&self.radice))
    }

    /// «..»: la cartella che contiene quella aperta, mai sopra la radice.
    pub fn su(&mut self) {
        if !self.si_risale() {
            return;
        }
        if let Some(su) = self.cartella.as_ref().and_then(|c| c.percorso.parent()).map(Path::to_path_buf) {
            self.entra(su);
        }
    }

    /// La freccia indietro della cartella: la cartella di prima, oppure la Home.
    pub fn indietro_cartella(&mut self) {
        self.cartella = None;
        self.chiudi_ricerca_in_cartella();
        if let Some(prima) = self.storia.pop() {
            self.leggi(prima);
        }
    }

    #[cfg(test)]
    pub fn cartella_aperta(&self) -> Option<&Path> {
        self.cartella.as_ref().map(|c| c.percorso.as_path())
    }

    /// Il contenuto della cartella aperta (vuoto se non ce n'è una), o l'errore della lettura.
    #[cfg(test)]
    pub fn voci(&self) -> Result<&[Voce], &io::Error> {
        match &self.cartella {
            Some(c) => c.voci.as_deref(),
            None => Ok(&[]),
        }
    }

    /// C'è un filtro: del testo cercato o un chip diverso da «Tutti».
    fn filtro_attivo(&self) -> bool {
        !self.ricerca.testo().trim().is_empty() || self.filtro != Filtro::Tutti
    }

    /// Indietro di Android sopra tutto: chiude il menu di un recente o il foglio di «Nuovo».
    /// Vero se ha chiuso qualcosa.
    pub fn chiudi_sopra(&mut self) -> bool {
        self.menu.take().is_some() || self.foglio_nuovo.indietro()
    }

    /// Indietro di Android, dopo la tastiera: la cartella di prima (o la Home), poi via il filtro.
    /// Falso se nella Home non c'era niente da chiudere: l'app va in secondo piano.
    pub fn indietro(&mut self) -> bool {
        if self.cerca_in_cartella {
            diagnosi("indietro: ricerca nella cartella chiusa");
            self.chiudi_ricerca_in_cartella();
            return true;
        }
        if self.cartella.is_some() {
            diagnosi("indietro: cartella di prima");
            self.indietro_cartella();
            return true;
        }
        if self.filtro_attivo() {
            diagnosi("indietro: ricerca svuotata");
            // Qui la tastiera è già chiusa e il campo non ha il fuoco: niente da dirle.
            let _ = self.ricerca.svuota();
            self.filtro = Filtro::Tutti;
            return true;
        }
        false
    }

    /// Disegna la Home, o la cartella aperta.
    pub fn mostra(&mut self, ui: &mut Ui, c: &Contesto) -> Option<Azione> {
        let mut azione = None;
        if self.cartella.is_some() {
            self.mostra_cartella(ui, c, &mut azione);
        } else {
            self.mostra_principale(ui, c, &mut azione);
        }
        if let Some(a) = self.mostra_foglio_nuovo(ui.ctx()) {
            azione = Some(a);
        }
        if let Some(a) = self.mostra_menu(ui.ctx()) {
            azione = Some(a);
        }
        azione
    }

    fn mostra_principale(&mut self, ui: &mut Ui, c: &Contesto, azione: &mut Option<Azione>) {
        let zona = ui.max_rect();
        let oggi = Locale::da_utc(c.adesso);
        let mut vai = None;
        ScrollArea::vertical().id_salt("home").scroll_source(egui::scroll_area::ScrollSource::ALL).auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            if testa(ui, &quando::data_lunga(&oggi)) {
                *azione = Some(Azione::Impostazioni);
            }
            if !self.accesso {
                riquadro_accesso(ui, azione);
            }
            ui.add_space(12.0);
            con_margini(ui, tema::MARGINE_LARGO, |ui| {
                let r = self.ricerca.mostra_ricerca(ui, CERCA);
                if r.svuota {
                    *azione = Some(Azione::SvuotaRicerca);
                } else if r.campo.clicked() {
                    *azione = Some(Azione::Cerca);
                }
            });
            ui.add_space(6.0 + 14.0);
            con_margini(ui, tema::MARGINE_LARGO, |ui| {
                // La fila alta quanto i chip, non quanto un bersaglio da 48 dp.
                ui.spacing_mut().interact_size.y = tema::CHIP;
                // Cinque chip non stanno in 372 dp: la fila scorre col dito, invece di allargare
                // tutta la Home oltre lo schermo (0.9.0, segnalazione dell'autore del 3/11).
                ScrollArea::horizontal().id_salt("chip_della_home").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = tema::SPAZIO;
                        for (f, nome) in Filtro::CHIP {
                            if ui.add(Chip::new(nome).acceso(self.filtro == f)).clicked() {
                                self.filtro = f;
                            }
                        }
                    });
                });
            });
            ui.add_space(4.0);
            if self.filtro_attivo() {
                self.trovati(ui, &oggi, azione);
            } else {
                self.recenti_della_home(ui, &oggi, azione);
                if self.accesso && !self.scorciatoie.is_empty() {
                    vai = self.griglia(ui);
                }
            }
            // Lo spazio del «Nuovo», che sta sopra la fine dell'elenco.
            ui.add_space(tema::NUOVO + tema::MARGINE_LARGO + 24.0);
        });
        if let Some(cartella) = vai {
            self.storia.clear();
            self.entra(cartella);
        }
        if !c.tastiera && PulsanteNuovo::new("Nuovo").mostra(ui, zona).clicked() {
            diagnosi("nuovo: foglio aperto");
            self.foglio_nuovo.apri();
        }
    }

    /// «Recenti» con «Vedi tutti» e i primi quattro (o tutti) in una scheda.
    fn recenti_della_home(&mut self, ui: &mut Ui, oggi: &Locale, azione: &mut Option<Azione>) {
        let n = self.recenti.voci().len();
        let collegamento = (n > RECENTI_IN_HOME).then_some(if self.tutti_i_recenti { "Vedi meno" } else { "Vedi tutti" });
        if titoletto(ui, "Recenti", collegamento).is_some_and(|r| r.clicked()) {
            self.tutti_i_recenti = !self.tutti_i_recenti;
        }
        let quanti = if self.tutti_i_recenti { n } else { n.min(RECENTI_IN_HOME) };
        let voci: Vec<Recente> = self.recenti.voci()[..quanti].to_vec();
        if voci.is_empty() {
            con_margini(ui, tema::MARGINE, |ui| {
                Scheda::new().margine(Margin::same(18)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Qui compaiono i documenti aperti di recente.").color(tema::MATITA));
                });
            });
            return;
        }
        self.scheda_dei_recenti(ui, &voci, oggi, azione);
    }

    /// I recenti in una scheda; un tocco li apre, il menu si apre sul suo pulsante.
    fn scheda_dei_recenti(&mut self, ui: &mut Ui, voci: &[Recente], oggi: &Locale, azione: &mut Option<Azione>) {
        let mut menu = None;
        con_margini(ui, tema::MARGINE, |ui| {
            Scheda::new().show(ui, |ui| {
                for (i, r) in voci.iter().enumerate() {
                    let (icona, posto) = self.luogo(&r.chiave);
                    let dove = riga_del_recente(&posto, &quando::quando(&Locale::da_utc(r.aperto_alle), oggi), r);
                    match voce_recente(ui, r, icona, &dove, i + 1 == voci.len()) {
                        Some(ToccoRecente::Apri) => *azione = Some(Azione::ApriRecente(r.clone())),
                        Some(ToccoRecente::Menu(rect)) => menu = Some((r.chiave.clone(), rect.right_top())),
                        None => {}
                    }
                }
            });
        });
        if menu.is_some() {
            diagnosi("recenti: menu aperto");
            self.menu = menu;
        }
    }

    /// Le cartelle in griglia, due per riga, col numero di file. Restituisce quella toccata.
    fn griglia(&self, ui: &mut Ui) -> Option<PathBuf> {
        titoletto(ui, "Cartelle", None);
        let mut toccata = None;
        con_margini(ui, tema::MARGINE, |ui| {
            const SPAZIO: f32 = 10.0;
            const ALTA: f32 = 64.0;
            let larga = (ui.available_width() - SPAZIO) / 2.0;
            for (riga, coppia) in self.scorciatoie.iter().zip(self.numeri()).collect::<Vec<_>>().chunks(2).enumerate() {
                if riga > 0 {
                    ui.add_space(SPAZIO);
                }
                let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ALTA), Sense::hover());
                let mut fila = ui.new_child(UiBuilder::new().max_rect(rect));
                for (j, (s, n)) in coppia.iter().enumerate() {
                    let posto = Rect::from_min_size(pos2(rect.left() + j as f32 * (larga + SPAZIO), rect.top()), vec2(larga, ALTA));
                    let sotto = numero_di_file(*n);
                    if SchedaCartella::new(icona_della_scorciatoia(&s.nome), nome_breve(&s.nome), &sotto).mostra_in(&mut fila, posto).clicked() {
                        toccata = Some(s.percorso.clone());
                    }
                }
            }
        });
        if toccata.is_some() {
            diagnosi("cartella: aperta dalla Home");
        }
        toccata
    }

    /// Quanti file ha ogni scorciatoia (None se non si è letta).
    fn numeri(&self) -> Vec<Option<usize>> {
        (0..self.scorciatoie.len()).map(|i| self.file.get(i).and_then(|f| f.as_ref().map(Vec::len))).collect()
    }

    /// I recenti e i file delle cartelle che passano la ricerca e il filtro.
    fn trovati(&mut self, ui: &mut Ui, oggi: &Locale, azione: &mut Option<Azione>) {
        let cercato = self.ricerca.testo().trim().to_string();
        let recenti: Vec<Recente> = self.recenti.voci().iter().filter(|r| passa(&r.nome, r.tipo, &cercato, self.filtro)).cloned().collect();
        let chiavi: HashSet<&str> = self.recenti.voci().iter().map(|r| r.chiave.as_str()).collect();
        let mut file: Vec<(&Voce, usize)> = Vec::new();
        for (i, voci) in self.file.iter().enumerate() {
            for v in voci.iter().flatten() {
                let gia = v.percorso.to_str().is_some_and(|p| chiavi.contains(p));
                if !gia && si_apre(v.tipo) && passa(&v.nome, v.tipo, &cercato, self.filtro) {
                    file.push((v, i));
                }
            }
        }
        let altri = file.len().saturating_sub(TROVATI_AL_MASSIMO);
        file.truncate(TROVATI_AL_MASSIMO);
        let file: Vec<(Voce, usize)> = file.into_iter().map(|(v, i)| (v.clone(), i)).collect();
        if recenti.is_empty() && file.is_empty() {
            ui.add_space(16.0);
            con_margini(ui, tema::MARGINE_TITOLO, |ui| ui.label(RichText::new("Nessun documento trovato.").color(tema::MATITA)));
            return;
        }
        if !recenti.is_empty() {
            titoletto(ui, "Recenti", None);
            self.scheda_dei_recenti(ui, &recenti, oggi, azione);
        }
        if !file.is_empty() {
            titoletto(ui, "Nelle cartelle", None);
            con_margini(ui, tema::MARGINE, |ui| {
                Scheda::new().show(ui, |ui| {
                    for (k, (v, i)) in file.iter().enumerate() {
                        let s = &self.scorciatoie[*i];
                        let data = v.modificato.map(|t| quando::quando(&Locale::da_utc(secondi(t)), oggi)).unwrap_or_default();
                        let dove = if data.is_empty() { s.nome.clone() } else { format!("{} · {data}", s.nome) };
                        let spenta = !si_apre(v.tipo);
                        let voce = tascabile_touch::Voce::new(tipo_della_voce(v.tipo), nome_da_mostrare(&v.nome, v.tipo))
                            .dove(icona_della_scorciatoia(&s.nome), &dove)
                            .spenta(spenta)
                            .ultima(k + 1 == file.len());
                        if mostra_se_visibile(ui, voce) {
                            *azione = Some(Azione::ApriFile { percorso: v.percorso.clone(), nome: v.nome.clone() });
                        }
                    }
                });
            });
            if altri > 0 {
                ui.add_space(8.0);
                let testo = format!("E altri {altri}: scrivi qualche lettera in più per trovarli.");
                con_margini(ui, tema::MARGINE_TITOLO, |ui| ui.label(RichText::new(testo).color(tema::MATITA)));
            }
        }
    }

    /// La schermata della cartella aperta: la barra alta col percorso e il Cerca; la ricerca, se è
    /// aperta; le due viste («Con le cartelle»: «..», le sottocartelle e i file; «Tutti i file»:
    /// anche quelli delle sottocartelle, i più recenti prima). Solo i file che l'app apre.
    fn mostra_cartella(&mut self, ui: &mut Ui, c: &Contesto, azione: &mut Option<Azione>) {
        let tutti_insieme = self.tutti_insieme;
        let cercato = if self.cerca_in_cartella { self.ricerca.testo().trim().to_string() } else { String::new() };
        // Tutti i file della cartella, letti la prima volta che servono (vista o ricerca).
        if (tutti_insieme || !cercato.is_empty())
            && let Some(c) = self.cartella.as_mut()
            && c.tutti.is_none()
        {
            let tutti: Vec<Voce> = elenca_tutto(&c.percorso, TUTTI_AL_MASSIMO).into_iter().filter(|v| si_apre(v.tipo)).collect();
            diagnosi(format!("cartella: {} file con le sottocartelle", tutti.len()));
            c.tutti = Some(tutti);
        }
        let Some(cartella) = &self.cartella else { return };
        let oggi = Locale::da_utc(c.adesso);
        let titolo = self.nome_cartella(&cartella.percorso);
        let sotto = conteggio(&cartella.voci);
        let sopra = cartella.percorso.parent().map(|p| self.nome_cartella(p)).unwrap_or_default();
        let si_risale = self.si_risale();
        let (mut indietro, mut su, mut vai, mut vista, mut cerca) = (false, false, None, None, false);
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        let barra = BarraAlta::new(&titolo).indietro("Indietro").sottotitolo(&sotto).azione(PulsanteIcona::new(icone::SEARCH, CERCA_PULSANTE)).mostra(ui);
        indietro |= barra.indietro;
        cerca |= barra.azione.is_some();
        if self.cerca_in_cartella {
            ui.add_space(8.0);
            con_margini(ui, tema::MARGINE_LARGO, |ui| {
                let r = self.ricerca.mostra_ricerca(ui, CERCA_IN_CARTELLA);
                if r.svuota {
                    *azione = Some(Azione::SvuotaRicerca);
                } else if r.campo.clicked() {
                    *azione = Some(Azione::Cerca);
                }
            });
        }
        ui.add_space(10.0);
        con_margini(ui, tema::MARGINE_LARGO, |ui| {
            ui.spacing_mut().interact_size.y = tema::CHIP;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = tema::SPAZIO;
                if ui.add(Chip::new(VISTA_CARTELLE).acceso(!tutti_insieme)).clicked() {
                    vista = Some(false);
                }
                if ui.add(Chip::new(VISTA_TUTTI).acceso(tutti_insieme)).clicked() {
                    vista = Some(true);
                }
            });
        });
        ui.add_space(6.0);
        let id = egui::Id::new("cartella").with(&cartella.percorso).with(tutti_insieme);
        ScrollArea::vertical().id_salt(id).scroll_source(egui::scroll_area::ScrollSource::ALL).auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            ui.add_space(4.0);
            con_margini(ui, tema::MARGINE, |ui| {
                Scheda::new().show(ui, |ui| {
                    // La ricerca, o tutti i file insieme: con la sottocartella dove stanno.
                    if !cercato.is_empty() || tutti_insieme {
                        let tutti = cartella.tutti.as_deref().unwrap_or(&[]);
                        let trovati: Vec<&Voce> = tutti.iter().filter(|v| cercato.is_empty() || nome_contiene(&v.nome, &cercato)).collect();
                        if trovati.is_empty() {
                            let testo = if cercato.is_empty() { "Nessun documento qui." } else { "Nessun documento trovato." };
                            Frame::NONE.inner_margin(Margin::same(18)).show(ui, |ui| ui.label(RichText::new(testo).color(tema::MATITA)));
                        }
                        for (i, v) in trovati.iter().enumerate() {
                            let dove = dove_nella_cartella(&cartella.percorso, v, &oggi);
                            let voce = tascabile_touch::Voce::new(tipo_della_voce(v.tipo), &v.nome).dove(icone::SCHEDULE, &dove).ultima(i + 1 == trovati.len());
                            if mostra_se_visibile(ui, voce) {
                                *azione = Some(Azione::ApriFile { percorso: v.percorso.clone(), nome: v.nome.clone() });
                            }
                        }
                        return;
                    }
                    let voci: Vec<&Voce> = cartella.voci.as_deref().unwrap_or(&[]).iter().filter(|v| v.cartella || si_apre(v.tipo)).collect();
                    if si_risale {
                        let voce = tascabile_touch::Voce::new(tascabile_touch::Tipo::Cartella, "..").icona(icone::ARROW_UPWARD).sotto(&sopra);
                        su = voce.ultima(voci.is_empty() && cartella.voci.is_ok()).mostra(ui).aperta();
                    }
                    match &cartella.voci {
                        Err(e) => {
                            let testo = format!("Non riesco a leggere questa cartella ({:?}).", e.kind());
                            Frame::NONE.inner_margin(Margin::same(18)).show(ui, |ui| ui.colored_label(tema::ROSSO, testo));
                        }
                        Ok(_) if voci.is_empty() => {
                            Frame::NONE.inner_margin(Margin::same(18)).show(ui, |ui| ui.label(RichText::new("Cartella vuota.").color(tema::MATITA)));
                        }
                        Ok(_) => {
                            for (i, v) in voci.iter().enumerate() {
                                let ultima = i + 1 == voci.len();
                                if v.cartella {
                                    let voce = tascabile_touch::Voce::new(tascabile_touch::Tipo::Cartella, &v.nome).ultima(ultima);
                                    if mostra_se_visibile(ui, voce) {
                                        vai = Some(v.percorso.clone());
                                    }
                                    continue;
                                }
                                let dove = dove_del_file(v, &oggi);
                                let voce = tascabile_touch::Voce::new(tipo_della_voce(v.tipo), &v.nome).dove(icone::SCHEDULE, &dove).ultima(ultima);
                                if mostra_se_visibile(ui, voce) {
                                    *azione = Some(Azione::ApriFile { percorso: v.percorso.clone(), nome: v.nome.clone() });
                                }
                            }
                        }
                    }
                });
            });
            ui.add_space(24.0);
        });
        if let Some(t) = vista {
            self.scegli_vista(t);
        }
        if cerca {
            if self.cerca_in_cartella {
                self.chiudi_ricerca_in_cartella();
            } else {
                diagnosi("cartella: ricerca aperta");
                let _ = self.ricerca.svuota();
                self.cerca_in_cartella = true;
                *azione = Some(Azione::Cerca);
            }
        }
        if indietro {
            self.indietro_cartella();
        } else if su {
            self.chiudi_ricerca_in_cartella();
            self.su();
        } else if let Some(p) = vai {
            self.chiudi_ricerca_in_cartella();
            self.entra(p);
        }
    }

    /// Il foglio di «Nuovo»: prima «Scansiona» (i PDF vengono prima, l'autore il 3/11), poi
    /// «Documento vuoto» e i modelli (Verbale, Relazione, Lettera, Lezione).
    fn mostra_foglio_nuovo(&mut self, ctx: &egui::Context) -> Option<Azione> {
        let scelto = self.foglio_nuovo.mostra(ctx, "Nuovo", |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            if tascabile_touch::Voce::new(tascabile_touch::Tipo::Pdf, "Scansiona").sotto("Foto dei fogli in un PDF, col testo che si cerca").mostra(ui).aperta() {
                return Some(Azione::Scansiona);
            }
            if tascabile_touch::Voce::new(tascabile_touch::Tipo::Word, DOCUMENTO_VUOTO).sotto("Un documento Word vuoto").mostra(ui).aperta() {
                return Some(Azione::Nuovo);
            }
            let ultimo = Modello::TUTTI.len() - 1;
            for (i, m) in Modello::TUTTI.into_iter().enumerate() {
                let icona = match m {
                    Modello::Verbale => icone::GROUPS,
                    Modello::Relazione => icone::ASSIGNMENT,
                    Modello::Lettera => icone::MAIL,
                    Modello::Lezione => icone::SCHOOL,
                };
                if tascabile_touch::Voce::new(tascabile_touch::Tipo::Word, m.nome()).icona(icona).sotto(m.sotto()).ultima(i == ultimo).mostra(ui).aperta() {
                    return Some(Azione::NuovoDaModello(m));
                }
            }
            None
        });
        let azione = scelto.flatten()?;
        self.foglio_nuovo.chiudi();
        Some(azione)
    }

    /// Il menu di un recente, se è aperto: Apri, Condividi, Togli dai recenti.
    fn mostra_menu(&mut self, ctx: &egui::Context) -> Option<Azione> {
        let (chiave, angolo) = self.menu.clone()?;
        let scelta = Menu::new(egui::Id::new("menu_recente"))
            .primo(icone::FILE_OPEN, "Apri")
            .voce(icone::SHARE, "Condividi")
            .voce(icone::CLOSE, "Togli dai recenti")
            .mostra(ctx, angolo)?;
        self.menu = None;
        let r = self.recenti.voci().iter().find(|r| r.chiave == chiave).cloned()?;
        match scelta {
            SceltaMenu::Voce(0) => Some(Azione::ApriRecente(r)),
            SceltaMenu::Voce(1) => Some(Azione::CondividiRecente(r)),
            SceltaMenu::Voce(2) => {
                diagnosi("recenti: tolto dal menu");
                self.dimentica(&r.chiave);
                None
            }
            _ => None,
        }
    }
}

/// Il contenuto con `margine` dp a destra e a sinistra.
fn con_margini<R>(ui: &mut Ui, margine: f32, contenuto: impl FnOnce(&mut Ui) -> R) -> R {
    let m = margine as i8;
    Frame::NONE.inner_margin(Margin { left: m, right: m, top: 0, bottom: 0 }).show(ui, contenuto).inner
}

/// Una voce di elenco, se si vede; altrimenti solo il suo posto (una cartella con mille file non
/// si misura tutta a ogni frame). Vero se è stata toccata.
fn mostra_se_visibile(ui: &mut Ui, voce: tascabile_touch::Voce<'_>) -> bool {
    let posto = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), tema::VOCE));
    if !ui.is_rect_visible(posto) {
        ui.allocate_space(posto.size());
        return false;
    }
    voce.mostra(ui).aperta()
}

/// La riga sotto il nome di un recente: da dove, quando e, per un PDF, quante pagine («da
/// WhatsApp · ieri 19:40 · 3 pagine»), per una presentazione quante slide.
pub fn riga_del_recente(posto: &str, quando: &str, r: &Recente) -> String {
    match (r.tipo, r.pagine) {
        (Tipo::Pdf, Some(1)) => format!("{posto} · {quando} · 1 pagina"),
        (Tipo::Pdf, Some(n)) => format!("{posto} · {quando} · {n} pagine"),
        (Tipo::Presentazione, Some(n)) => format!("{posto} · {quando} · {n} slide"),
        (Tipo::Foglio, Some(1)) => format!("{posto} · {quando} · 1 foglio"),
        (Tipo::Foglio, Some(n)) => format!("{posto} · {quando} · {n} fogli"),
        _ => format!("{posto} · {quando}"),
    }
}

/// Una voce dei recenti, col pulsante del menu.
fn voce_recente(ui: &mut Ui, r: &Recente, icona: &str, dove: &str, ultima: bool) -> Option<ToccoRecente> {
    let nome = nome_da_mostrare(&r.nome, r.tipo);
    let etichetta = etichetta_menu(nome);
    let risposta = tascabile_touch::Voce::new(tipo_della_voce(r.tipo), nome).dove(icona, dove).menu(&etichetta).ultima(ultima).mostra(ui);
    if risposta.menu_toccato() {
        return risposta.menu.map(|m| ToccoRecente::Menu(m.rect));
    }
    risposta.aperta().then_some(ToccoRecente::Apri)
}

/// «21 ottobre · 24 KB».
/// La riga sotto un file nella vista «Tutti i file» o nella ricerca di una cartella: la
/// sottocartella dove sta (se non è quella aperta), la data, la misura.
fn dove_nella_cartella(cartella: &Path, v: &Voce, oggi: &Locale) -> String {
    let sotto = v.percorso.parent().and_then(|p| p.strip_prefix(cartella).ok()).map(|p| p.to_string_lossy().into_owned()).filter(|p| !p.is_empty());
    match sotto {
        Some(s) => format!("{s} · {}", dove_del_file(v, oggi)),
        None => dove_del_file(v, oggi),
    }
}

fn dove_del_file(v: &Voce, oggi: &Locale) -> String {
    let data = v.modificato.map(|t| quando::quando(&Locale::da_utc(secondi(t)), oggi));
    let mut pezzi: Vec<String> = Vec::new();
    pezzi.extend(data);
    pezzi.push(misura(v.byte));
    pezzi.join(" · ")
}

/// Il sottotitolo della barra di una cartella: «2 cartelle · 12 file», «Vuota», «Non si legge».
fn conteggio(voci: &io::Result<Vec<Voce>>) -> String {
    let Ok(voci) = voci else { return "Non si legge".to_string() };
    let cartelle = voci.iter().filter(|v| v.cartella).count();
    // Solo i file che l'app apre: gli altri non si vedono.
    let file = voci.iter().filter(|v| !v.cartella && si_apre(v.tipo)).count();
    let mut pezzi = Vec::new();
    match cartelle {
        0 => {}
        1 => pezzi.push("1 cartella".to_string()),
        n => pezzi.push(format!("{n} cartelle")),
    }
    if file > 0 {
        pezzi.push(format!("{file} file"));
    }
    if pezzi.is_empty() { "Vuota".to_string() } else { pezzi.join(" · ") }
}

/// La testa della Home: «Doc Tascabile.» in Fraunces con «Doc» e il punto in blu, sotto la data;
/// a destra le Impostazioni. Vero se si sono toccate le Impostazioni.
fn testa(ui: &mut Ui, data: &str) -> bool {
    let ctx = ui.ctx().clone();
    let font = Carattere::Titolo.font(&ctx, 36.0);
    let mut job = egui::text::LayoutJob::default();
    for (pezzo, colore) in [("Doc", tema::BLU), (" Tascabile", tema::INCHIOSTRO), (".", tema::BLU)] {
        job.append(pezzo, 0.0, egui::TextFormat { font_id: font.clone(), color: colore, extra_letter_spacing: -1.35, ..Default::default() });
    }
    let titolo = ui.painter().layout_job(job);
    let data = ui.painter().layout_no_wrap(data.to_owned(), Carattere::Testo.font(&ctx, 14.0), tema::MATITA);
    // `.home-testa`: 14 dp sopra, 6 sotto; il titolo alto 36 × 1,05, la data 4 sotto.
    let alto_testo = 37.8 + 4.0 + data.size().y;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 14.0 + alto_testo.max(tema::PULSANTE) + 6.0), Sense::hover());
    let x = rect.left() + tema::MARGINE_TITOLO;
    // Il titolo nella sua riga da 37,8 dp, come la linea del CSS.
    let y_titolo = rect.top() + 14.0 + (37.8 - titolo.size().y) / 2.0;
    let posto_titolo = Rect::from_min_size(pos2(x, y_titolo), titolo.size());
    ui.painter().galley(posto_titolo.min, titolo, Color32::PLACEHOLDER);
    ui.interact(posto_titolo, ui.id().with("titolo"), Sense::hover())
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, "Doc Tascabile"));
    let posto_data = Rect::from_min_size(pos2(x, rect.top() + 14.0 + 37.8 + 4.0), data.size());
    ui.painter().galley(posto_data.min, data.clone(), Color32::PLACEHOLDER);
    ui.interact(posto_data, ui.id().with("data"), Sense::hover())
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, data.text()));
    let posto = Rect::from_min_size(pos2(rect.right() - 12.0 - tema::PULSANTE, rect.top() + 14.0), Vec2::splat(tema::PULSANTE));
    let mut angolo = ui.new_child(UiBuilder::new().max_rect(posto));
    angolo.add(PulsanteIcona::new(icone::SETTINGS, "Impostazioni")).clicked()
}

/// Il riquadro che chiede l'accesso a tutti i file, sotto la testa.
fn riquadro_accesso(ui: &mut Ui, azione: &mut Option<Azione>) {
    ui.add_space(12.0);
    con_margini(ui, tema::MARGINE, |ui| {
        Scheda::new().margine(Margin::same(18)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 8.0;
            let ctx = ui.ctx().clone();
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                ui.label(RichText::new(icone::FOLDER_OPEN).font(Carattere::Icona.font(&ctx, tema::ICONA)).color(tema::BLU));
                ui.label(RichText::new("Serve l'accesso ai file").font(Carattere::Grassetto.font(&ctx, 17.0)).color(tema::INCHIOSTRO));
            });
            ui.label(
                // Corto: a 372 dp il testo di prima andava su sei righe, e il «Nuovo» copriva il menu
                // del quarto recente (0.8.3).
                RichText::new(
                    "Per aprire i documenti del telefono e gli allegati di WhatsApp. In rete va solo l'impronta di un file, se ne \
                     crei il timestamp.",
                )
                .font(Carattere::Testo.font(&ctx, 15.0))
                .color(tema::MATITA),
            );
            if ui.add(Pulsante::new("Concedi", Peso::Principale)).clicked() {
                *azione = Some(Azione::ConcediAccesso);
            }
        });
    });
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::prova::cartella_di_prova;
    use tascabile_files::Tipo;

    fn recente(chiave: &str, nome: &str) -> Recente {
        Recente { chiave: chiave.into(), nome: nome.into(), tipo: Tipo::Documento, aperto_alle: 1_792_738_800, pagine: None }
    }

    #[test]
    fn i_pdf_recenti_dicono_le_pagine() {
        let pdf = |pagine| Recente { tipo: Tipo::Pdf, pagine, ..recente("content://x/1", "Turni.pdf") };
        assert_eq!(riga_del_recente("da WhatsApp", "ieri 19:40", &pdf(Some(3))), "da WhatsApp · ieri 19:40 · 3 pagine");
        assert_eq!(riga_del_recente("Download", "oggi 08:12", &pdf(Some(1))), "Download · oggi 08:12 · 1 pagina");
        // Un PDF ricordato da una versione di prima, senza pagine, e un documento.
        assert_eq!(riga_del_recente("Download", "21 ottobre", &pdf(None)), "Download · 21 ottobre");
        assert_eq!(riga_del_recente("Documenti", "20 ottobre", &recente("/x/V.docx", "V.docx")), "Documenti · 20 ottobre");
        // Le presentazioni dicono le slide, che in italiano non cambiano al plurale.
        let slide = |pagine| Recente { tipo: Tipo::Presentazione, pagine, ..recente("/x/L.pptx", "L.pptx") };
        assert_eq!(riga_del_recente("Download", "oggi 08:12", &slide(Some(12))), "Download · oggi 08:12 · 12 slide");
        assert_eq!(riga_del_recente("Download", "oggi 08:12", &slide(Some(1))), "Download · oggi 08:12 · 1 slide");
    }

    /// Radice finta con Download, Documents e Documents/Tascabile; cartella dell'app accanto.
    fn radice_di_prova(nome: &str) -> (PathBuf, PathBuf) {
        let d = cartella_di_prova(nome);
        let radice = d.join("radice");
        for c in ["Download", "Documents/Tascabile"] {
            std::fs::create_dir_all(radice.join(c)).unwrap();
        }
        (radice, d.join("app"))
    }

    #[test]
    fn i_recenti_vengono_dal_file_della_cartella_dell_app() {
        let (radice, app) = radice_di_prova("home-recenti");
        let mut scritti = Recenti::default();
        scritti.aggiungi(recente("/x/Vecchio.docx", "Vecchio.docx"));
        scritti.aggiungi(recente("content://com.whatsapp.provider.media/item/7", "documento.docx"));
        scritti.salva(&app.join(FILE_RECENTI)).unwrap();
        let home = Home::nuova(radice, &app, true);
        let chiavi: Vec<&str> = home.recenti().iter().map(|r| r.chiave.as_str()).collect();
        assert_eq!(chiavi, ["content://com.whatsapp.provider.media/item/7", "/x/Vecchio.docx"]);
    }

    #[test]
    fn ricorda_mette_in_cima_e_scrive_il_file() {
        let (radice, app) = radice_di_prova("home-ricorda");
        let mut home = Home::nuova(radice.clone(), &app, true);
        home.ricorda(recente("/x/Primo.docx", "Primo.docx"));
        home.ricorda(recente("/x/Secondo.docx", "Secondo.docx"));
        home.ricorda(recente("/x/Primo.docx", "Primo.docx"));
        let su_disco = Recenti::carica(&app.join(FILE_RECENTI));
        let chiavi: Vec<&str> = su_disco.voci().iter().map(|r| r.chiave.as_str()).collect();
        assert_eq!(chiavi, ["/x/Primo.docx", "/x/Secondo.docx"]);
        home.dimentica("/x/Primo.docx");
        let su_disco = Recenti::carica(&app.join(FILE_RECENTI));
        assert_eq!(su_disco.voci().len(), 1);
        assert_eq!(home.recenti().len(), 1);
        // Un'altra Home, come dopo un riavvio dell'app, li ritrova.
        assert_eq!(Home::nuova(radice, &app, true).recenti()[0].chiave, "/x/Secondo.docx");
    }

    #[test]
    fn da_dove_viene_un_recente() {
        let (radice, app) = radice_di_prova("home-dove");
        let home = Home::nuova(radice.clone(), &app, true);
        let dove = |chiave: &Path| home.dove(&chiave.to_string_lossy());
        assert_eq!(home.dove("content://com.whatsapp.provider.media/item/123"), "da WhatsApp");
        assert_eq!(home.dove("content://com.whatsapp.w4b.provider.media/item/9"), "da WhatsApp");
        assert_eq!(home.dove("content://org.telegram.messenger.provider/media/x.docx"), "da Telegram");
        assert_eq!(home.dove("content://com.google.android.gm.sapi/x/message_attachment/1"), "da un'altra app");
        assert_eq!(dove(&radice.join("Download/Verbale.docx")), "Download");
        assert_eq!(dove(&radice.join("Documents/Tascabile/Verbale.docx")), "Documenti/Tascabile");
        assert_eq!(dove(&radice.join("Documents/Lavoro/Ottobre/Verbale.docx")), "Documenti/Lavoro/Ottobre");
        assert_eq!(dove(&radice.join("Lavoro/Verbale.docx")), "Lavoro");
        assert_eq!(dove(&radice.join("Verbale.docx")), "Memoria del telefono");
        // Fuori dalla radice (sul PC, un file passato a mano): la cartella com'è.
        assert_eq!(home.dove("/altrove/Verbale.docx"), "/altrove");
    }

    #[test]
    fn l_icona_del_posto_di_un_recente() {
        let (radice, app) = radice_di_prova("home-icone-dei-posti");
        let home = Home::nuova(radice.clone(), &app, true);
        let icona = |chiave: &str| home.luogo(chiave).0;
        assert_eq!(icona("content://com.whatsapp.provider.media/item/1"), icone::CHAT);
        assert_eq!(icona("content://org.telegram.messenger.provider/x"), icone::SEND);
        assert_eq!(icona("content://com.example.posta/allegato/9"), icone::APPS);
        let percorso = |p: &str| radice.join(p).to_string_lossy().into_owned();
        assert_eq!(icona(&percorso("Download/Verbale.docx")), icone::DOWNLOAD);
        assert_eq!(icona(&percorso("Documents/Lavoro/Verbale.docx")), icone::FOLDER);
        assert_eq!(icona(&percorso("Documents/Tascabile/Verbale.docx")), icone::FOLDER_OPEN);
        assert_eq!(icona(&percorso("Verbale.docx")), icone::PHONE_ANDROID);
        // Ogni scorciatoia ha la sua icona, e il nome breve per la griglia.
        for (nome, attesa) in [("Download", icone::DOWNLOAD), ("Documenti", icone::FOLDER), ("WhatsApp", icone::CHAT), ("Telegram", icone::SEND)] {
            assert_eq!(icona_della_scorciatoia(nome), attesa, "{nome}");
        }
        assert_eq!(nome_breve("Documenti/Tascabile"), "Tascabile");
        assert_eq!(nome_breve("Download"), "Download");
    }

    #[test]
    fn un_uri_non_si_riapre_da_un_altra_app() {
        assert_eq!(provenienza("content://com.whatsapp.provider.media/item/1"), Some("WhatsApp"));
        assert_eq!(provenienza("content://org.telegram.messenger.web.provider/x"), Some("Telegram"));
        assert_eq!(provenienza("content://com.android.providers.downloads.documents/document/5"), None);
        assert_eq!(provenienza("/storage/emulated/0/Download/com.whatsapp.docx"), None);
        assert!(e_uri("content://x/1") && e_uri("file:///storage/emulated/0/x.docx"));
        assert!(!e_uri("/storage/emulated/0/Download/x.docx"));
    }

    #[test]
    fn aggiorna_dice_se_c_e_l_accesso() {
        let (radice, app) = radice_di_prova("home-accesso");
        let mut home = Home::nuova(radice, &app, false);
        assert!(!home.accesso());
        assert!(home.file.is_empty(), "senza accesso le cartelle non si leggono");
        home.imposta_accesso(true);
        home.aggiorna();
        assert!(home.accesso());
        let nomi: Vec<&str> = home.scorciatoie.iter().map(|s| s.nome.as_str()).collect();
        assert_eq!(nomi, ["Download", "Documenti", "Documenti/Tascabile"]);
    }

    #[test]
    fn le_schede_contano_i_file_del_primo_livello() {
        let (radice, app) = radice_di_prova("home-numeri");
        for f in ["Download/a.docx", "Download/b.pdf", "Download/.nascosto", "Documents/Tascabile/c.docx"] {
            std::fs::write(radice.join(f), b"x").unwrap();
        }
        std::fs::create_dir_all(radice.join("Download/Lavoro")).unwrap();
        std::fs::write(radice.join("Download/Lavoro/d.docx"), b"x").unwrap();
        let home = Home::nuova(radice, &app, true);
        // Download: due file (la sottocartella e il nascosto no); Documenti: niente al primo livello.
        assert_eq!(home.numeri(), [Some(2), Some(0), Some(1)]);
        assert_eq!(numero_di_file(Some(2)), "2 file");
        assert_eq!(numero_di_file(Some(1)), "1 file");
        assert_eq!(numero_di_file(Some(0)), "Nessun file");
        assert_eq!(numero_di_file(None), "Non si legge");
        let d = tascabile_platform::diagnosi_testo();
        assert!(d.contains("] home: 3 scorciatoie lette, 3 file, in "), "{d}");
    }

    #[test]
    fn nella_home_i_nomi_senza_estensione() {
        assert_eq!(nome_da_mostrare("Relazione turno notte.docx", Tipo::Documento), "Relazione turno notte");
        assert_eq!(nome_da_mostrare("Bolletta.v2.pdf", Tipo::Pdf), "Bolletta.v2");
        assert_eq!(nome_da_mostrare("foto.jpg", Tipo::Altro), "foto.jpg");
        assert_eq!(nome_da_mostrare(".docx", Tipo::Documento), ".docx");
        assert_eq!(nome_da_mostrare("Verbale", Tipo::Documento), "Verbale");
    }

    #[test]
    fn la_ricerca_guarda_il_tipo_e_le_parole() {
        assert!(passa("Relazione turno notte.docx", Tipo::Documento, "turno", Filtro::Tutti));
        assert!(passa("Relazione turno notte.docx", Tipo::Documento, "", Filtro::Documenti));
        assert!(!passa("Relazione turno notte.docx", Tipo::Documento, "", Filtro::Pdf));
        assert!(passa("Bolletta.pdf", Tipo::Pdf, "bolL", Filtro::Pdf));
        assert!(passa("Lezione.pptx", Tipo::Presentazione, "", Filtro::Slide));
        assert!(!passa("Lezione.pptx", Tipo::Presentazione, "verbale", Filtro::Tutti));
    }

    #[test]
    fn le_misure_e_le_righe_della_cartella() {
        assert_eq!(misura(512), "512 byte");
        assert_eq!(misura(24 * 1024), "24 KB");
        assert_eq!(misura(1_300_000), "1,2 MB");
        let voce = |nome: &str, cartella: bool| Voce {
            nome: nome.into(),
            percorso: PathBuf::from(nome),
            cartella,
            tipo: if cartella { Tipo::Altro } else { tascabile_files::tipo_da_nome(nome) },
            modificato: None,
            byte: 2048,
        };
        assert_eq!(conteggio(&Ok(vec![])), "Vuota");
        assert_eq!(conteggio(&Ok(vec![voce("Lavoro", true)])), "1 cartella");
        assert_eq!(conteggio(&Ok(vec![voce("A", true), voce("B", true), voce("c.docx", false)])), "2 cartelle · 1 file");
        assert_eq!(conteggio(&Err(io::Error::from(io::ErrorKind::NotFound))), "Non si legge");
        let oggi = Locale { anno: 2026, mese: 10, giorno: 24, ora: 10, minuti: 0 };
        assert_eq!(dove_del_file(&voce("c.docx", false), &oggi), "2 KB");
        assert_eq!(dove_del_file(&voce("b.pdf", false), &oggi), "2 KB");
        assert_eq!(dove_del_file(&voce("c.pptx", false), &oggi), "2 KB");
        assert!(si_apre(Tipo::Documento) && si_apre(Tipo::Pdf) && si_apre(Tipo::Presentazione));
        assert!(!si_apre(Tipo::Altro));
    }

    #[test]
    fn un_file_di_una_sottocartella_dice_dove_sta() {
        let oggi = Locale::da_utc(0);
        let v = |p: &str| Voce { nome: "R.docx".into(), percorso: PathBuf::from(p), cartella: false, tipo: Tipo::Documento, byte: 2048, modificato: None };
        assert_eq!(dove_nella_cartella(Path::new("/r/Download"), &v("/r/Download/Lavoro/Turni/R.docx"), &oggi), "Lavoro/Turni · 2 KB");
        assert_eq!(dove_nella_cartella(Path::new("/r/Download"), &v("/r/Download/R.docx"), &oggi), "2 KB");
    }

    #[test]
    fn il_navigatore_entra_e_risale_fino_alla_radice() {
        let (radice, app) = radice_di_prova("home-navigatore");
        std::fs::write(radice.join("Download/Verbale.docx"), b"x").unwrap();
        let mut home = Home::nuova(radice.clone(), &app, true);
        assert_eq!(home.cartella_aperta(), None);
        home.entra(radice.join("Documents/Tascabile"));
        assert_eq!(home.cartella_aperta(), Some(radice.join("Documents/Tascabile").as_path()));
        assert!(home.si_risale());
        home.su();
        assert_eq!(home.cartella_aperta(), Some(radice.join("Documents").as_path()));
        home.su();
        assert_eq!(home.cartella_aperta(), Some(radice.as_path()));
        // Sopra la radice non si va: sul telefono /storage/emulated non si legge.
        assert!(!home.si_risale());
        home.su();
        assert_eq!(home.cartella_aperta(), Some(radice.as_path()));
        let nomi: Vec<String> = home.voci().unwrap().iter().map(|v| v.nome.clone()).collect();
        assert_eq!(nomi, ["Documents", "Download"]);
        // La freccia indietro ripercorre la strada fatta, poi torna alla Home.
        home.indietro_cartella();
        assert_eq!(home.cartella_aperta(), Some(radice.join("Documents").as_path()));
        home.indietro_cartella();
        assert_eq!(home.cartella_aperta(), Some(radice.join("Documents/Tascabile").as_path()));
        home.indietro_cartella();
        assert_eq!(home.cartella_aperta(), None);
    }

    #[test]
    fn la_cartella_nella_diagnosi_senza_nomi() {
        let (radice, app) = radice_di_prova("home-diagnosi");
        let zanzibar = radice.join("Download/Zanzibar");
        std::fs::create_dir_all(&zanzibar).unwrap();
        for nome in ["Kilimangiaro.docx", "Timbuctù.pdf"] {
            std::fs::write(zanzibar.join(nome), b"x").unwrap();
        }
        std::fs::create_dir(zanzibar.join("Sotto Zanzibar")).unwrap();
        let mut home = Home::nuova(radice, &app, true);
        home.entra(zanzibar.clone());
        // Sparita fra l'elenco e il tocco: l'errore, mai un elenco a metà.
        home.entra(zanzibar.join("Sparita Zanzibar"));
        assert_eq!(home.voci().unwrap_err().kind(), std::io::ErrorKind::NotFound);
        let d = tascabile_platform::diagnosi_testo();
        assert!(d.contains("] cartella: 1 cartelle e 2 file\n"), "{d}");
        assert!(d.contains("] cartella: errore di lettura (NotFound)\n"), "{d}");
        for parola in ["Zanzibar", "Kilimangiaro", "Timbuctù", "prove-app"] {
            assert!(!d.contains(parola), "«{parola}» nella diagnosi:\n{d}");
        }
    }

    #[test]
    fn aggiorna_rilegge_la_cartella_aperta_e_i_numeri() {
        let (radice, app) = radice_di_prova("home-aggiorna");
        let mut home = Home::nuova(radice.clone(), &app, true);
        home.entra(radice.join("Download"));
        assert!(home.voci().unwrap().is_empty());
        assert_eq!(home.numeri()[0], Some(0));
        std::fs::write(radice.join("Download/Arrivato.docx"), b"x").unwrap();
        home.aggiorna();
        assert_eq!(home.voci().unwrap()[0].nome, "Arrivato.docx");
        assert_eq!(home.numeri()[0], Some(1));
        // Rileggere non è entrare: la storia resta com'era.
        home.indietro_cartella();
        assert_eq!(home.cartella_aperta(), None);
    }
}
