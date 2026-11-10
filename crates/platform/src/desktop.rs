//! Piattaforma finta per Linux, con la stessa interfaccia di quella Android.
//! Simula anche la tastiera (un testo con selezione, come l'Editable di GameTextInput),
//! così il ponte tastiera si prova sul PC senza telefono; e poi cartelle, appunti in memoria e
//! una condivisione che lascia solo la sua riga nella diagnosi.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};

use crate::ime::{byte_to_utf16, utf16_to_byte};
use crate::{
    ErroreLettura, EsitoFoto, FonteFoto, ImeSnapshot, OpenRequest, RichiesteTastiera, RispostaRete, TASTIERA_FINTA_PT, TastoCrudo, TipoTastiera,
    diagnosi, riga_rete, righe,
};

#[derive(Default)]
struct Finta {
    aperta: bool,
    testo: String,
    /// Selezione in byte (inizio, fine); il cursore è la fine.
    sel: (usize, usize),
    versione: i64,
    pendente: Option<ImeSnapshot>,
    crudi: Vec<TastoCrudo>,
    /// Ultimo stato ricevuto dall'app, per i test.
    ultimo_stato: Option<(String, i32, i32, i64)>,
    richiesta: Option<OpenRequest>,
    /// Istante (`InputState::time`) dell'ultimo input visto da `tasti_crudi`: ogni passo di
    /// egui ha il suo, e così gli eventi di un passo si applicano una volta sola.
    ultimo_input: Option<f64>,
    /// Appunti finti.
    appunti: Option<String>,
    /// I link esterni aperti, per le prove (mai nella diagnosi).
    indirizzi: Vec<String>,
    /// Le foto chieste, e il canale da cui arriva l'esito che fanno arrivare le prove, come sul
    /// telefono.
    foto_chieste: Vec<FonteFoto>,
    canale: crate::CanaleFoto,
    /// La presentazione delle Slide accesa: sul telefono schermo acceso, barre di sistema
    /// nascoste e tasti del volume.
    presentazione: bool,
    /// La modalità riservata: sul telefono FLAG_SECURE e finestre di altre app nascoste.
    riservata: bool,
    /// L'impronta: quello che risponde la finestra finta (riuscita, se le prove non dicono
    /// altro), quante se ne sono chieste, e la richiesta che aspetta il suo esito.
    impronta_simulata: Option<crate::EsitoImpronta>,
    impronte_chieste: i32,
    impronta_in_attesa: Option<(i32, crate::EsitoImpronta)>,
    /// Lo schermo esterno finto, e la misura delle immagini mandate al proiettore.
    schermo_esterno: Option<(u32, u32)>,
    proiettate: Vec<(u32, u32)>,
}

/// La rete finta delle prove: dall'indirizzo e dal corpo, la risposta.
type ReteSimulata = std::sync::Arc<dyn Fn(&str, Option<&[u8]>) -> RispostaRete + Send + Sync>;

/// La rete sul PC: non c'è (l'app la usa solo per il timestamp). Nelle prove si simula con
/// [`Piattaforma::simula_rete`].
#[derive(Clone, Default)]
pub struct Rete {
    simulata: Option<ReteSimulata>,
    /// Il cancello della rete, chiuso finché chi usa l'app non dà il consenso a un'operazione
    /// (`Piattaforma::consenti_rete`).
    aperta: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Rete {
    /// Una richiesta https: POST col corpo, GET senza. Dal thread che chiama.
    pub fn chiedi(&self, indirizzo: &str, corpo: Option<&[u8]>) -> RispostaRete {
        if !self.aperta.load(std::sync::atomic::Ordering::SeqCst) {
            diagnosi("rete: chiusa, nessun consenso");
            return RispostaRete::Collegamento;
        }
        let inizio = std::time::Instant::now();
        let r = match &self.simulata {
            Some(f) => f(indirizzo, corpo),
            None => RispostaRete::Collegamento,
        };
        diagnosi(riga_rete(&r, inizio.elapsed()));
        r
    }
}

pub struct Piattaforma {
    /// La rete, simulata nelle prove.
    rete: RefCell<Rete>,
    finta: RefCell<Finta>,
    /// Richieste di aprire e chiudere la tastiera, per la diagnosi.
    richieste: RichiesteTastiera,
    /// Dati dell'app (sul telefono `getFilesDir()`).
    app: PathBuf,
    /// Cache dell'app (sul telefono `getCacheDir()`).
    cache: PathBuf,
    /// Radice dei documenti (sul telefono la memoria condivisa).
    radice: PathBuf,
    /// «Accesso a tutti i file»: sul PC c'è sempre, salvo nelle prove che lo tolgono.
    accesso: Cell<bool>,
    /// Quante volte l'app è andata in secondo piano: sul PC non succede niente, le prove lo contano.
    secondo_piano: Cell<u32>,
    /// Il tipo della tastiera chiesto per ultimo.
    tipo: Cell<TipoTastiera>,
}

/// Sul PC: dati in ~/.local/share/tascabile-prova, cache in ~/.cache/tascabile-prova e, come
/// radice dei documenti, la casa.
impl Default for Piattaforma {
    fn default() -> Self {
        let casa = std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from);
        let (app, cache) = cartelle_prova_del_pc(&casa);
        Self::con_cartelle(app, cache, casa)
    }
}

fn emetti(f: &mut Finta) {
    let a = byte_to_utf16(&f.testo, f.sel.0) as i32;
    let b = byte_to_utf16(&f.testo, f.sel.1) as i32;
    f.pendente = Some(ImeSnapshot { text: f.testo.clone(), sel: (a, b), comp: (-1, -1), version: f.versione });
}

/// Le cartelle della prova sul PC nella casa `casa`: (dati dell'app, cache).
fn cartelle_prova_del_pc(casa: &Path) -> (PathBuf, PathBuf) {
    (casa.join(".local/share/tascabile-prova"), casa.join(".cache/tascabile-prova"))
}

/// La cartella, creata se manca: sul telefono le crea Android, qui si fa al primo uso. Un errore
/// va nella diagnosi senza il percorso (dentro c'è il nome dell'utente del PC).
fn creata(cartella: &Path, quale: &str) -> PathBuf {
    if let Err(e) = std::fs::create_dir_all(cartella) {
        diagnosi(format!("{quale}: non creata ({:?})", e.kind()));
    }
    cartella.to_path_buf()
}

impl Piattaforma {
    /// Per i test: cartelle dell'app e della cache (create al primo uso) e radice dei documenti
    /// scelte da chi prova.
    pub fn con_cartelle(app: PathBuf, cache: PathBuf, radice: PathBuf) -> Self {
        Self {
            rete: RefCell::default(),
            finta: RefCell::default(),
            richieste: RichiesteTastiera::nuove(),
            app,
            cache,
            radice,
            accesso: Cell::new(true),
            secondo_piano: Cell::new(0),
            tipo: Cell::new(TipoTastiera::Testo),
        }
    }

    /// Dati dell'app (recenti, recupero).
    pub fn cartella_app(&self) -> PathBuf {
        creata(&self.app, "cartella dell'app")
    }

    /// Cache dell'app (file da condividere in `condivisi/`).
    pub fn cartella_cache(&self) -> PathBuf {
        creata(&self.cache, "cartella della cache")
    }

    /// Da dove partono le cartelle dei documenti.
    pub fn radice_archivio(&self) -> PathBuf {
        self.radice.clone()
    }

    /// Sul PC l'accesso ai file c'è sempre, salvo che una prova lo tolga (`simula_accesso_file`).
    pub fn accesso_file(&self) -> bool {
        self.accesso.get()
    }

    /// Sul PC non c'è una schermata da aprire: l'accesso non cambia (lo decide la prova).
    pub fn chiedi_accesso_file(&self) {
        diagnosi("accesso ai file: chiesto; sul PC non c'è la schermata delle impostazioni");
    }

    /// Per le prove: l'accesso ai file concesso o no, come se l'utente avesse toccato
    /// l'interruttore nelle impostazioni.
    pub fn simula_accesso_file(&self, concesso: bool) {
        self.accesso.set(concesso);
    }

    /// Sul PC non c'è il foglio di condivisione: resta solo la riga della diagnosi, con tipo e
    /// misura del file. Come sul telefono, solo i file di `cartella_cache()/condivisi/`.
    pub fn condividi_file(&self, percorso: &Path, mime: &str, _titolo: &str) {
        if !righe::dentro_condivisi(&self.cartella_cache(), percorso) {
            diagnosi(righe::CONDIVIDI_FUORI);
            return;
        }
        diagnosi(righe::condividi_file(percorso, mime));
    }

    /// Un link esterno di un PDF, già confermato: sul telefono va al browser con un Intent di
    /// sistema; sul PC resta la riga della diagnosi, col solo genere dell'indirizzo.
    pub fn apri_indirizzo(&self, indirizzo: &str) {
        match crate::genere_indirizzo(indirizzo) {
            Some(genere) => {
                diagnosi(format!("link esterno: {genere} aperta fuori dall'app"));
                self.finta.borrow_mut().indirizzi.push(indirizzo.to_owned());
            }
            None => diagnosi("link esterno: indirizzo che non si apre"),
        }
    }

    /// La presentazione delle Slide comincia o finisce: qui la si ricorda per le prove.
    pub fn riservata(&self, accesa: bool) {
        self.finta.borrow_mut().riservata = accesa;
    }

    /// La finestra dell'impronta finta: risponde quello che dice `simula_impronta`, di base
    /// «riuscita».
    pub fn chiedi_impronta(&self, _titolo: &str, _spiegazione: &str) -> i32 {
        let mut f = self.finta.borrow_mut();
        f.impronte_chieste += 1;
        let esito = f.impronta_simulata.unwrap_or(crate::EsitoImpronta::Riuscita);
        f.impronta_in_attesa = Some((f.impronte_chieste, esito));
        f.impronte_chieste
    }

    pub fn esito_impronta(&self, richiesta: i32) -> Option<crate::EsitoImpronta> {
        let mut f = self.finta.borrow_mut();
        match f.impronta_in_attesa {
            Some((r, esito)) if r == richiesta => {
                f.impronta_in_attesa = None;
                Some(esito)
            }
            _ => None,
        }
    }

    /// Per le prove: che cosa risponde la finestra dell'impronta.
    pub fn simula_impronta(&self, esito: crate::EsitoImpronta) {
        self.finta.borrow_mut().impronta_simulata = Some(esito);
    }

    /// Per le prove: quante impronte si sono chieste.
    pub fn impronte_chieste(&self) -> i32 {
        self.finta.borrow().impronte_chieste
    }

    /// Per le prove: la modalità riservata è accesa.
    pub fn riservata_accesa(&self) -> bool {
        self.finta.borrow().riservata
    }

    pub fn presentazione(&self, attiva: bool) {
        self.finta.borrow_mut().presentazione = attiva;
    }

    /// Per le prove: la presentazione è accesa.
    pub fn presentazione_attiva(&self) -> bool {
        self.finta.borrow().presentazione
    }

    /// Lo schermo esterno della presentazione (cavo HDMI, Miracast) in pixel, se c'è.
    pub fn schermo_esterno(&self) -> Option<(u32, u32)> {
        self.finta.borrow().schermo_esterno
    }

    /// Per le prove: uno schermo esterno attaccato o staccato.
    /// La rete per il timestamp, da usare su un thread di lavoro.
    pub fn rete(&self) -> Rete {
        self.rete.borrow().clone()
    }

    /// Per le prove: la rete risponde con `f` (indirizzo, corpo se è un POST).
    pub fn simula_rete(&self, f: impl Fn(&str, Option<&[u8]>) -> RispostaRete + Send + Sync + 'static) {
        let aperta = self.rete.borrow().aperta.clone();
        *self.rete.borrow_mut() = Rete { simulata: Some(std::sync::Arc::new(f)), aperta };
    }

    /// Apre o chiude il cancello della rete (livello 1 della roadmap della sicurezza).
    pub fn consenti_rete(&self, aperta: bool) {
        self.rete.borrow().aperta.store(aperta, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn simula_schermo_esterno(&self, misura: Option<(u32, u32)>) {
        self.finta.borrow_mut().schermo_esterno = misura;
    }

    /// Un'immagine per lo schermo esterno (RGBA premoltiplicato): qui se ne tiene la misura.
    pub fn proietta(&self, _pixel: Vec<u8>, larghezza: u32, altezza: u32) {
        self.finta.borrow_mut().proiettate.push((larghezza, altezza));
    }

    /// Per le prove: le misure delle immagini mandate allo schermo esterno, in ordine.
    pub fn proiettate(&self) -> Vec<(u32, u32)> {
        self.finta.borrow().proiettate.clone()
    }

    /// Per le prove: i link esterni aperti.
    pub fn indirizzi_aperti(&self) -> Vec<String> {
        self.finta.borrow().indirizzi.clone()
    }

    pub fn condividi_testo(&self, testo: &str, _titolo: &str) {
        diagnosi(righe::condividi_testo(testo));
    }

    /// Appunti finti, in memoria.
    pub fn copia_testo(&self, testo: &str) {
        diagnosi(righe::copia(testo));
        self.finta.borrow_mut().appunti = Some(testo.to_owned());
    }

    pub fn incolla_testo(&self) -> Option<String> {
        let testo = crate::entro_il_tetto_dell_incolla(self.finta.borrow().appunti.clone());
        diagnosi(righe::incolla(testo.as_deref()));
        testo
    }

    pub fn show_keyboard(&self) {
        self.richieste.apri();
        self.finta.borrow_mut().aperta = true;
    }

    /// Si può chiamare a ogni frame: nella diagnosi va solo la prima richiesta di una serie.
    pub fn hide_keyboard(&self) {
        self.richieste.chiudi();
        self.finta.borrow_mut().aperta = false;
    }

    pub fn keyboard_visible(&self) -> bool {
        self.finta.borrow().aperta
    }

    /// Altezza della tastiera in punti (0 se chiusa).
    pub fn keyboard_height(&self) -> f32 {
        if self.keyboard_visible() { TASTIERA_FINTA_PT } else { 0.0 }
    }

    /// La tastiera per il campo che ha il fuoco: sul PC resta solo la riga della diagnosi, quando
    /// cambia.
    pub fn tipo_tastiera(&self, tipo: TipoTastiera) {
        if self.tipo.replace(tipo) != tipo {
            diagnosi(righe::tipo_tastiera(tipo));
        }
    }

    /// Solo sul PC, per le prove: il tipo della tastiera di adesso.
    pub fn tipo_della_tastiera(&self) -> TipoTastiera {
        self.tipo.get()
    }

    /// L'app dà alla tastiera un testo nuovo (selezione in unità UTF-16).
    pub fn set_ime_state(&self, text: String, sel_start: i32, sel_end: i32, version: i64) {
        diagnosi(righe::finestra(&text, sel_start, sel_end, version, self.tipo.get()));
        let mut f = self.finta.borrow_mut();
        let a = utf16_to_byte(&text, sel_start.max(0) as usize);
        let b = utf16_to_byte(&text, sel_end.max(0) as usize);
        f.ultimo_stato = Some((text.clone(), sel_start, sel_end, version));
        f.testo = text;
        f.sel = (a.min(b), a.max(b));
        f.versione = version;
    }

    pub fn take_ime_snapshot(&self) -> Option<ImeSnapshot> {
        self.finta.borrow_mut().pendente.take()
    }

    /// La richiesta «Apri con» finta; rifiutata, il perché. Un indirizzo con lo schema passa dallo
    /// stesso controllo del telefono (`uri_da_aprire`); i percorsi del PC no.
    pub fn take_open_request(&self) -> Option<Result<OpenRequest, &'static str>> {
        let r = self.finta.borrow_mut().richiesta.take()?;
        if r.uri.contains("://")
            && let Err(perche) = crate::uri_da_aprire(&r.uri)
        {
            crate::diagnosi(format!("apri con: indirizzo non aperto ({perche})"));
            return Some(Err(perche));
        }
        Some(Ok(r))
    }

    /// Sul PC l'«URI» è un percorso. Nel genere dell'errore solo il tipo dell'errore di I/O o il
    /// tetto, come sul telefono.
    pub fn read_uri(&self, uri: &str) -> Result<Vec<u8>, ErroreLettura> {
        crate::leggi_con_tetto(self.apri_uri(uri)?, uri)
    }

    /// Come sul telefono: il file aperto, da leggere anche su un altro thread.
    pub fn apri_uri(&self, uri: &str) -> Result<std::fs::File, ErroreLettura> {
        std::fs::File::open(uri).map_err(|e| ErroreLettura { genere: format!("{:?}", e.kind()), messaggio: format!("{uri}: {e}") })
    }

    /// La foto della firma: sul telefono la fotocamera o la scelta di un'immagine, con un Intent
    /// di sistema; sul PC la richiesta resta per le prove, che fanno arrivare la foto con
    /// `simula_foto_firma`. Il numero della richiesta, con cui chiederne l'esito.
    pub fn chiedi_foto_firma(&self, fonte: FonteFoto) -> i32 {
        diagnosi(fonte.chiesta());
        let mut f = self.finta.borrow_mut();
        f.foto_chieste.push(fonte);
        f.canale.chiedi(fonte, || 1)
    }

    /// La richiesta di foto in corso (0 prima della prima).
    pub fn richiesta_foto_in_corso(&self) -> i32 {
        self.finta.borrow().canale.in_corso()
    }

    /// All'avvio: via le foto rimaste nella cache da prima.
    pub fn pulisci_le_foto(&self) {
        crate::pulisci_le_foto(&self.cache);
    }

    /// All'avvio: via le copie condivise più vecchie di un'ora.
    pub fn pulisci_i_condivisi(&self) {
        crate::pulisci_i_condivisi(&self.cache, crate::ETA_DEI_CONDIVISI);
    }

    /// Un file degli asset dell'APK: sul PC dalle cartelle che Gradle mette negli asset, i
    /// modelli del riconoscimento del testo (`crates/ocr/modelli`) e il dizionario italiano
    /// (`crates/mod-word/asset`).
    pub fn leggi_asset(&self, nome: &str) -> Option<Vec<u8>> {
        let qui = Path::new(env!("CARGO_MANIFEST_DIR"));
        ["../ocr/modelli", "../mod-word/asset"].iter().find_map(|c| std::fs::read(qui.join(c).join(nome)).ok())
    }

    /// L'esito della richiesta `richiesta`, una volta sola: None se non è ancora arrivato,
    /// [`EsitoFoto::Superata`] se nel frattempo ne è partita un'altra.
    pub fn prendi_foto_firma(&self, richiesta: i32) -> Option<EsitoFoto> {
        self.finta.borrow_mut().canale.prendi(richiesta, &self.cache)
    }

    /// Per le prove: l'esito dal Java per la richiesta `richiesta`, come sul telefono (quello di
    /// una richiesta superata non si tiene, e la sua cartella se ne va).
    fn ricevi_foto(&self, richiesta: i32, esito: i32, scelte: i32) {
        if !self.finta.borrow_mut().canale.ricevi(richiesta, esito, scelte) {
            let _ = std::fs::remove_dir_all(crate::cartella_delle_scelte(&self.cache, richiesta));
            diagnosi("foto: esito di una richiesta superata, lasciato");
        }
    }

    /// Per le prove: le foto chieste.
    pub fn foto_chieste(&self) -> Vec<FonteFoto> {
        self.finta.borrow().foto_chieste.clone()
    }

    /// Per le prove: arriva la foto `byte` per la richiesta in corso (nel primo file della sua
    /// cartella, come la scrive Android), o nessuna (indietro dalla fotocamera).
    pub fn simula_foto_firma(&self, byte: Option<&[u8]>) {
        self.simula_foto_di(self.richiesta_foto_in_corso(), byte);
    }

    /// Per le prove: arriva la foto `byte` per la richiesta `richiesta`, anche superata (come una
    /// copia lenta che finisce dopo che n'è partita un'altra).
    pub fn simula_foto_di(&self, richiesta: i32, byte: Option<&[u8]>) {
        let esito = match byte {
            Some(b) => {
                let file = crate::file_della_pagina(&creata(&self.cache, "cartella della cache"), richiesta, 1);
                let scritta = file.parent().is_some_and(|c| std::fs::create_dir_all(c).is_ok()) && std::fs::write(&file, b).is_ok();
                if scritta { 1 } else { -1 }
            }
            None => 0,
        };
        self.ricevi_foto(richiesta, esito, esito.max(0));
    }

    /// Per le prove: arrivano le foto `foto` scelte insieme dalla galleria per la richiesta in
    /// corso (le prime 30, nella cartella della richiesta, come le copia Android), o nessuna
    /// (indietro dalla galleria).
    pub fn simula_foto_galleria(&self, foto: &[&[u8]]) {
        let richiesta = self.richiesta_foto_in_corso();
        if foto.is_empty() {
            self.ricevi_foto(richiesta, 0, 0);
            return;
        }
        let cache = creata(&self.cache, "cartella della cache");
        let _ = std::fs::remove_dir_all(crate::cartella_delle_scelte(&cache, richiesta));
        let tenute = &foto[..foto.len().min(crate::MASSIMO_PAGINE_SCELTE)];
        let file: Vec<PathBuf> = (1..=tenute.len()).map(|i| crate::file_della_pagina(&cache, richiesta, i)).collect();
        let scritte =
            file.iter().zip(tenute).all(|(f, b)| f.parent().is_some_and(|c| std::fs::create_dir_all(c).is_ok()) && std::fs::write(f, b).is_ok());
        if scritte {
            self.ricevi_foto(richiesta, tenute.len() as i32, foto.len() as i32);
        } else {
            self.ricevi_foto(richiesta, -1, 0);
        }
    }

    pub fn move_to_background(&self) {
        self.secondo_piano.set(self.secondo_piano.get() + 1);
    }

    /// Solo sul PC, per le prove: quante volte si è chiesto di andare in secondo piano.
    pub fn volte_in_secondo_piano(&self) -> u32 {
        self.secondo_piano.get()
    }

    /// Sul PC: una barra di stato finta in alto e la tastiera simulata in basso.
    pub fn apply_raw_input(&self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        raw.safe_area_insets =
            Some(egui::SafeAreaInsets(egui::epaint::MarginF32 { left: 0.0, right: 0.0, top: 24.0, bottom: self.keyboard_height() }));
    }

    /// La tastiera vera del PC arriva come eventi egui: si applicano al testo finto come
    /// farebbe GameTextInput. Restituisce i tasti che la tastiera lascia all'app.
    pub fn tasti_crudi(&self, ctx: &egui::Context) -> Vec<TastoCrudo> {
        // eframe chiama `logic` anche a finestra nascosta, senza un passo nuovo, e `ctx.input()`
        // ha ancora gli eventi dell'ultimo passo: non vanno riscritti. Il passo si riconosce
        // dall'istante del suo input, che fuori dai passi non cambia; `cumulative_pass_nr` non
        // basta, perché fuori dai passi vale già il numero del passo che verrà, e allora gli
        // eventi vecchi si ripeterebbero una volta e quelli del passo dopo andrebbero persi.
        // Il secondo passo di un frame rifatto con `request_discard` può avere lo stesso
        // istante, ma egui gli dà gli eventi già tolti: non si perde niente.
        let istante = ctx.input(|i| i.time);
        let nuovo = self.finta.borrow_mut().ultimo_input.replace(istante) != Some(istante);
        if self.keyboard_visible() && nuovo {
            for ev in ctx.input(|i| i.events.clone()) {
                match ev {
                    egui::Event::Text(t) => self.simula_testo(&t),
                    egui::Event::Key { key, pressed: true, .. } => match key {
                        egui::Key::Backspace => self.simula_backspace(),
                        egui::Key::Delete => self.simula_canc(),
                        egui::Key::Enter => self.simula_testo("\n"),
                        egui::Key::ArrowLeft => self.simula_freccia(false),
                        egui::Key::ArrowRight => self.simula_freccia(true),
                        _ => {}
                    },
                    _ => {}
                }
            }
        }
        std::mem::take(&mut self.finta.borrow_mut().crudi)
    }

    /// Scrive `t` al posto della selezione, come `commitText`.
    pub fn simula_testo(&self, t: &str) {
        let mut f = self.finta.borrow_mut();
        let (a, b) = f.sel;
        f.testo.replace_range(a..b, t);
        let c = a + t.len();
        f.sel = (c, c);
        emetti(&mut f);
    }

    /// Come una tastiera che corregge una parola: sostituisce l'intervallo UTF-16 `da..a`.
    pub fn simula_sostituzione(&self, da: usize, a: usize, t: &str) {
        let (da, a) = (da.min(a), da.max(a));
        let mut f = self.finta.borrow_mut();
        let da = utf16_to_byte(&f.testo, da);
        let a = utf16_to_byte(&f.testo, a);
        f.testo.replace_range(da..a, t);
        let c = da + t.len();
        f.sel = (c, c);
        emetti(&mut f);
    }

    pub fn simula_backspace(&self) {
        let mut f = self.finta.borrow_mut();
        let (a, b) = f.sel;
        let prima = f.testo[..a].chars().next_back();
        if a != b {
            f.testo.replace_range(a..b, "");
            f.sel = (a, a);
        } else if let Some(c) = prima {
            let da = a - c.len_utf8();
            f.testo.replace_range(da..a, "");
            f.sel = (da, da);
        } else {
            // Cursore a 0: GameTextInput lascia passare il tasto (processKeyEvent restituisce false).
            f.crudi.push(TastoCrudo::Backspace);
            return;
        }
        emetti(&mut f);
    }

    pub fn simula_canc(&self) {
        let mut f = self.finta.borrow_mut();
        let (a, b) = f.sel;
        let dopo = f.testo[b..].chars().next();
        if a != b {
            f.testo.replace_range(a..b, "");
        } else if let Some(c) = dopo {
            f.testo.replace_range(a..a + c.len_utf8(), "");
        } else {
            f.crudi.push(TastoCrudo::Canc);
            return;
        }
        f.sel = (a, a);
        emetti(&mut f);
    }

    pub fn simula_freccia(&self, avanti: bool) {
        let mut f = self.finta.borrow_mut();
        let c = f.sel.1;
        let nuovo = if avanti {
            f.testo[c..].chars().next().map_or(c, |ch| c + ch.len_utf8())
        } else {
            f.testo[..c].chars().next_back().map_or(c, |ch| c - ch.len_utf8())
        };
        f.sel = (nuovo, nuovo);
        emetti(&mut f);
    }

    /// Per i test: l'ultimo stato che l'app ha dato alla tastiera (testo, inizio, fine, versione).
    pub fn ultimo_stato(&self) -> Option<(String, i32, i32, i64)> {
        self.finta.borrow().ultimo_stato.clone()
    }

    /// Per il desktop e i test: un «Apri con» di un file locale.
    pub fn simula_apri_con(&self, percorso: &str, mime: Option<&str>) {
        self.finta.borrow_mut().richiesta = Some(OpenRequest { uri: percorso.into(), mime: mime.map(Into::into), nome: None });
    }

    /// Come `simula_apri_con`, con il nome vero che l'app che lo manda dà al file.
    pub fn simula_apri_con_col_nome(&self, percorso: &str, mime: Option<&str>, nome: &str) {
        self.finta.borrow_mut().richiesta = Some(OpenRequest { uri: percorso.into(), mime: mime.map(Into::into), nome: Some(nome.into()) });
    }
}

#[cfg(test)]
mod test {
    use super::*;

    /// Più foto scelte insieme dalla galleria per la scansione (0.8.3): nella cartella della
    /// richiesta in corso, nell'ordine della scelta, al più 30 (con quante erano state scelte);
    /// nessuna è come tornare indietro. Una richiesta nuova lascia l'esito di quella di prima.
    #[test]
    fn dalla_galleria_piu_foto_arrivano_in_ordine_nella_cache() {
        let d = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/prove-platform/galleria");
        let _ = std::fs::remove_dir_all(&d);
        let p = Piattaforma::con_cartelle(d.join("app"), d.join("cache"), d.join("radice"));
        let r = p.chiedi_foto_firma(FonteFoto::GalleriaPiu);
        assert_eq!(p.foto_chieste(), [FonteFoto::GalleriaPiu]);
        p.simula_foto_galleria(&[b"uno", b"due", b"tre"]);
        let Some(EsitoFoto::Pronte { file, scelte }) = p.prendi_foto_firma(r) else { panic!("le foto non sono arrivate") };
        let cartella = file[0].parent().unwrap().to_path_buf();
        assert!(cartella.starts_with(d.join("cache/foto")) && cartella.file_name().unwrap().to_string_lossy().starts_with("scelte-"));
        assert_eq!(
            file.iter().map(|f| f.file_name().unwrap().to_string_lossy().into_owned()).collect::<Vec<_>>(),
            ["pagina-1", "pagina-2", "pagina-3"]
        );
        assert_eq!((std::fs::read(&file[1]).unwrap(), scelte), (b"due".to_vec(), 3));
        assert!(p.prendi_foto_firma(r).is_none());
        // Più di 30: arrivano le prime 30, e si sa quante erano.
        let r = p.chiedi_foto_firma(FonteFoto::GalleriaPiu);
        let tante: Vec<&[u8]> = (0..33).map(|_| &b"foglio"[..]).collect();
        p.simula_foto_galleria(&tante);
        let Some(EsitoFoto::Pronte { file: altre, scelte }) = p.prendi_foto_firma(r) else { panic!() };
        assert_eq!((altre.len(), scelte), (crate::MASSIMO_PAGINE_SCELTE, 33));
        assert_ne!(altre[0].parent(), file[0].parent(), "due richieste nella stessa cartella");
        // Un esito che nessuno ha preso non arriva alla richiesta dopo, e chi aspettava quella di
        // prima sa che è superata.
        p.simula_foto_galleria(&[b"vecchia"]);
        let foto = p.chiedi_foto_firma(FonteFoto::Fotocamera);
        assert_eq!((p.prendi_foto_firma(r), p.prendi_foto_firma(foto)), (Some(EsitoFoto::Superata), None));
        let r = p.chiedi_foto_firma(FonteFoto::GalleriaPiu);
        p.simula_foto_galleria(&[]);
        assert_eq!(p.prendi_foto_firma(r), Some(EsitoFoto::Annullata));
        // Una foto sola nella cartella della sua richiesta; quella di una richiesta superata, che
        // arriva tardi, non si tiene e la sua cartella se ne va.
        let vecchia = p.chiedi_foto_firma(FonteFoto::Galleria);
        let nuova = p.chiedi_foto_firma(FonteFoto::Fotocamera);
        p.simula_foto_di(vecchia, Some(b"tardi"));
        assert!(!crate::cartella_delle_scelte(&d.join("cache"), vecchia).exists(), "la foto superata è rimasta");
        assert_eq!(p.prendi_foto_firma(nuova), None);
        p.simula_foto_firma(Some(b"foglio"));
        assert_eq!(p.prendi_foto_firma(nuova), Some(EsitoFoto::Pronta(crate::file_della_pagina(&d.join("cache"), nuova, 1))));
    }

    #[test]
    fn apri_con_ha_un_tetto_di_50_mb() {
        let d = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/prove-platform/tetto");
        std::fs::create_dir_all(&d).unwrap();
        let grande = d.join("grande.docx");
        // Un file rado: 50 MB e un byte senza occuparli sul disco.
        std::fs::File::create(&grande).unwrap().set_len(50 * 1024 * 1024 + 1).unwrap();
        let p = Piattaforma::default();
        let e = p.read_uri(grande.to_str().unwrap()).unwrap_err();
        assert_eq!(e.genere, "oltre 50 MB");
        std::fs::File::create(&grande).unwrap().set_len(50 * 1024 * 1024).unwrap();
        assert_eq!(p.read_uri(grande.to_str().unwrap()).unwrap().len(), 50 * 1024 * 1024);
        std::fs::remove_file(&grande).unwrap();
    }

    #[test]
    fn scrivere_manda_uno_stato() {
        let p = Piattaforma::default();
        p.set_ime_state("Ciao".into(), 4, 4, 7);
        p.simula_testo(" a tutti");
        let s = p.take_ime_snapshot().unwrap();
        assert_eq!(s, ImeSnapshot { text: "Ciao a tutti".into(), sel: (12, 12), comp: (-1, -1), version: 7 });
        assert!(p.take_ime_snapshot().is_none());
    }

    #[test]
    fn backspace_all_inizio_resta_all_app() {
        let p = Piattaforma::default();
        p.set_ime_state("abc".into(), 0, 0, 1);
        p.simula_backspace();
        assert!(p.take_ime_snapshot().is_none());
        assert_eq!(std::mem::take(&mut p.finta.borrow_mut().crudi), vec![TastoCrudo::Backspace]);
    }

    #[test]
    fn emoji_conta_due_unita() {
        let p = Piattaforma::default();
        p.set_ime_state("a".into(), 1, 1, 1);
        p.simula_testo("😀");
        assert_eq!(p.take_ime_snapshot().unwrap().sel, (3, 3));
    }

    #[test]
    fn freccia_sposta_solo_il_cursore() {
        let p = Piattaforma::default();
        p.set_ime_state("àb".into(), 2, 2, 1);
        p.simula_freccia(false);
        let s = p.take_ime_snapshot().unwrap();
        assert_eq!((s.text.as_str(), s.sel), ("àb", (1, 1)));
    }

    #[test]
    fn sostituzione_come_autocorrezione() {
        let p = Piattaforma::default();
        p.set_ime_state("teh".into(), 3, 3, 1);
        p.simula_sostituzione(0, 3, "the ");
        assert_eq!(p.take_ime_snapshot().unwrap().text, "the ");
    }

    #[test]
    fn canc_cancella_il_carattere_dopo() {
        let p = Piattaforma::default();
        p.set_ime_state("àb".into(), 0, 0, 1);
        p.simula_canc();
        let s = p.take_ime_snapshot().unwrap();
        assert_eq!((s.text.as_str(), s.sel), ("b", (0, 0)));
    }

    #[test]
    fn canc_alla_fine_resta_all_app() {
        let p = Piattaforma::default();
        p.set_ime_state("ab".into(), 2, 2, 1);
        p.simula_canc();
        assert!(p.take_ime_snapshot().is_none());
        assert_eq!(std::mem::take(&mut p.finta.borrow_mut().crudi), vec![TastoCrudo::Canc]);
    }

    #[test]
    fn sostituzione_con_estremi_invertiti() {
        let p = Piattaforma::default();
        p.set_ime_state("teh".into(), 3, 3, 1);
        p.simula_sostituzione(3, 0, "the");
        assert_eq!(p.take_ime_snapshot().unwrap().text, "the");
    }

    /// Cartella vuota per una prova, dentro target/ (mai in /tmp); non ancora creata.
    fn cartella_di_prova(nome: &str) -> PathBuf {
        let d = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/prove-piattaforma").join(nome);
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    /// Piattaforma con le cartelle di una prova.
    fn con_cartelle_di_prova(nome: &str) -> (Piattaforma, PathBuf) {
        let d = cartella_di_prova(nome);
        (Piattaforma::con_cartelle(d.join("app"), d.join("cache"), d.join("radice")), d)
    }

    #[test]
    fn cartelle_del_pc() {
        let (app, cache) = cartelle_prova_del_pc(Path::new("/home/prova"));
        assert_eq!(app, Path::new("/home/prova/.local/share/tascabile-prova"));
        assert_eq!(cache, Path::new("/home/prova/.cache/tascabile-prova"));
    }

    #[test]
    fn cartelle_create_al_primo_uso() {
        let (p, d) = con_cartelle_di_prova("cartelle");
        assert!(!d.join("app").exists() && !d.join("cache").exists());
        assert_eq!(p.cartella_app(), d.join("app"));
        assert!(d.join("app").is_dir());
        assert_eq!(p.cartella_cache(), d.join("cache"));
        assert!(d.join("cache").is_dir());
        assert_eq!(p.radice_archivio(), d.join("radice"));
        // Una seconda volta va bene uguale.
        assert_eq!(p.cartella_app(), d.join("app"));
    }

    #[test]
    fn il_tipo_della_tastiera_e_la_password_senza_misura() {
        let p = Piattaforma::default();
        assert_eq!(p.tipo_della_tastiera(), TipoTastiera::Testo);
        p.tipo_tastiera(TipoTastiera::Password);
        p.tipo_tastiera(TipoTastiera::Password);
        assert_eq!(p.tipo_della_tastiera(), TipoTastiera::Password);
        p.set_ime_state("Kilimangiaro9".into(), 13, 13, 4242);
        p.tipo_tastiera(TipoTastiera::Numeri);
        p.set_ime_state("12".into(), 2, 2, 4243);
        p.tipo_tastiera(TipoTastiera::Testo);
        let d = crate::diagnosi_testo();
        // Una riga per cambio, e della password nemmeno quanto è lunga.
        assert_eq!(d.matches("] tastiera: tipo password\n").count(), 1, "{d}");
        assert!(d.contains("] tastiera: finestra v4242 inviata, campo della password\n"), "{d}");
        assert!(!d.contains("13 unità") && !d.contains("sel 13..13") && !d.contains("Kilimangiaro"), "{d}");
        assert!(d.contains("] tastiera: tipo numeri\n") && d.contains("] tastiera: finestra v4243 inviata, 2 unità, sel 2..2\n"), "{d}");
        assert!(d.contains("] tastiera: tipo testo\n"), "{d}");
        assert_eq!(TipoTastiera::da_codice(TipoTastiera::Password.codice() as u8), TipoTastiera::Password);
        assert_eq!(TipoTastiera::da_codice(9), TipoTastiera::Testo);
    }

    /// Roadmap della sicurezza, livello 1: la rete parte chiusa, anche con un server che risponde, e
    /// si apre e si chiude col consenso.
    #[test]
    fn la_rete_parte_chiusa() {
        let p = Piattaforma::default();
        p.simula_rete(|_, _| RispostaRete::Ok(vec![1]));
        assert!(matches!(p.rete().chiedi("https://a.pool.opentimestamps.org/digest", None), RispostaRete::Collegamento));
        p.consenti_rete(true);
        assert!(matches!(p.rete().chiedi("https://a.pool.opentimestamps.org/digest", None), RispostaRete::Ok(_)));
        p.consenti_rete(false);
        assert!(matches!(p.rete().chiedi("https://a.pool.opentimestamps.org/digest", None), RispostaRete::Collegamento));
        assert!(crate::diagnosi_testo().contains("] rete: chiusa, nessun consenso\n"));
    }

    #[test]
    fn appunti_finti_andata_e_ritorno() {
        let p = Piattaforma::default();
        assert_eq!(p.incolla_testo(), None);
        p.copia_testo("Zanzibar, isola di diciannove");
        assert_eq!(p.incolla_testo().as_deref(), Some("Zanzibar, isola di diciannove"));
        let d = crate::diagnosi_testo();
        assert!(d.contains("] appunti: copiati 29 caratteri\n"), "{d}");
        assert!(d.contains("] appunti: incollati 29 caratteri\n"), "{d}");
        assert!(d.contains("] appunti: niente da incollare\n"), "{d}");
        // Oltre il milione di caratteri non si incolla (revisione del 31/10, punto 6).
        p.copia_testo(&"x".repeat(crate::MASSIMO_INCOLLA + 1));
        assert_eq!(p.incolla_testo(), None);
        assert!(crate::diagnosi_testo().contains("] appunti: testo oltre il milione di caratteri, non incollato\n"));
        assert!(!d.contains("Zanzibar") && !d.contains("diciannove"), "{d}");
    }

    #[test]
    fn condividi_file_lascia_una_riga_con_tipo_e_misura() {
        let (p, _) = con_cartelle_di_prova("condividi");
        let condivisi = p.cartella_cache().join("condivisi");
        std::fs::create_dir_all(&condivisi).unwrap();
        let file = condivisi.join("Verbale Zanzibar.docx");
        std::fs::write(&file, vec![7u8; 12_345]).unwrap();
        p.condividi_file(&file, "application/vnd.openxmlformats-officedocument.wordprocessingml.document", "Condividi Kilimangiaro");
        let d = crate::diagnosi_testo();
        let righe: Vec<&str> = d.lines().filter(|r| r.contains("12345 byte")).collect();
        assert_eq!(
            righe.iter().map(|r| r.split_once("] ").unwrap().1).collect::<Vec<_>>(),
            ["condividi file: application/vnd.openxmlformats-officedocument.wordprocessingml.document, 12345 byte"],
            "{d}"
        );
        for parola in ["Verbale", "Zanzibar", "Kilimangiaro", "prove-piattaforma"] {
            assert!(!d.contains(parola), "«{parola}» nella diagnosi:\n{d}");
        }
        // Un file che non c'è: niente misura, ma nemmeno il nome.
        p.condividi_file(&condivisi.join("Assente Zanzibar.pdf"), "application/pdf", "titolo");
        let d = crate::diagnosi_testo();
        assert!(d.contains("] condividi file: application/pdf, file non leggibile (NotFound)\n"), "{d}");
        assert!(!d.contains("Assente"), "{d}");
    }

    #[test]
    fn condividi_file_solo_da_condivisi() {
        // Come FileProvider sul telefono (res/xml/percorsi_condivisi.xml): si condivide solo da
        // cache/condivisi/. Un file fuori, anche raggiunto con «..» o con un collegamento, non
        // lascia la riga di una condivisione (tipo e misura) ma quella del rifiuto.
        let (p, d) = con_cartelle_di_prova("condividi-fuori");
        let documenti = d.join("radice/Documenti");
        std::fs::create_dir_all(&documenti).unwrap();
        let originale = documenti.join("Originale Zanzibar.docx");
        std::fs::write(&originale, vec![1u8; 4321]).unwrap();
        let condivisi = p.cartella_cache().join("condivisi");
        std::fs::create_dir_all(&condivisi).unwrap();
        std::fs::write(p.cartella_cache().join("Fuori Zanzibar.docx"), vec![2u8; 2468]).unwrap();
        let collegamento = condivisi.join("Collegamento Zanzibar.docx");
        std::os::unix::fs::symlink(&originale, &collegamento).unwrap();
        let docx = "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
        p.condividi_file(&originale, docx, "Condividi");
        p.condividi_file(&condivisi.join("../Fuori Zanzibar.docx"), docx, "Condividi");
        p.condividi_file(&collegamento, docx, "Condividi");
        let d = crate::diagnosi_testo();
        for misura in ["4321 byte", "2468 byte"] {
            assert!(!d.contains(misura), "condivisione da fuori data per fatta ({misura}):\n{d}");
        }
        assert!(d.contains("] condividi file: fuori dalla cartella condivisi\n"), "{d}");
        assert!(!d.contains("Zanzibar"), "{d}");
    }

    #[test]
    fn errore_di_lettura_col_genere_senza_il_nome() {
        let (p, d) = con_cartelle_di_prova("leggi");
        std::fs::create_dir_all(&d).unwrap();
        let e = p.read_uri(d.join("Mancante Zanzibar.docx").to_str().unwrap()).unwrap_err();
        assert_eq!(e.genere, "NotFound");
        let e = p.read_uri(d.to_str().unwrap()).unwrap_err();
        assert_eq!(e.genere, "IsADirectory");
        // Sullo schermo il messaggio intero, come prima.
        let e = p.read_uri(d.join("Mancante Zanzibar.docx").to_str().unwrap()).unwrap_err();
        assert!(e.to_string().contains("Mancante Zanzibar.docx"), "{e}");
    }

    #[test]
    fn condividi_testo_senza_il_testo() {
        let p = Piattaforma::default();
        p.condividi_testo("Zanzibar e Kilimangiaro, ventisette", "Diagnosi di Timbuctù");
        let d = crate::diagnosi_testo();
        assert!(d.contains("] condividi testo: 35 caratteri\n"), "{d}");
        for parola in ["Zanzibar", "Kilimangiaro", "ventisette", "Timbuctù"] {
            assert!(!d.contains(parola), "«{parola}» nella diagnosi:\n{d}");
        }
    }

    #[test]
    fn sul_pc_l_accesso_ai_file_c_e_sempre() {
        let p = Piattaforma::default();
        assert!(p.accesso_file());
        p.chiedi_accesso_file();
        assert!(p.accesso_file());
        assert!(crate::diagnosi_testo().contains("] accesso ai file: chiesto; sul PC non c'è la schermata delle impostazioni\n"));
    }

    #[test]
    fn accesso_ai_file_negato_per_prova() {
        // Per provare la Home senza permesso: lo decide la prova, e la richiesta da sola non lo
        // concede, come sul telefono finché l'utente non tocca l'interruttore.
        let p = Piattaforma::default();
        p.simula_accesso_file(false);
        assert!(!p.accesso_file());
        p.chiedi_accesso_file();
        assert!(!p.accesso_file());
        p.simula_accesso_file(true);
        assert!(p.accesso_file());
    }

    #[test]
    fn chiusura_chiesta_a_ogni_frame_lascia_una_riga() {
        // Su Android la tastiera resta visibile per tutta l'animazione di chiusura, e intanto
        // l'app la chiede chiusa a ogni frame (`dopo_ponte`, finché il ponte è chiuso): 35
        // richieste di fila sono una chiusura sola. Dopo un'apertura, chiesta dall'app o venuta
        // da sé (su Android lo dicono gli insets), una riga nuova.
        let p = Piattaforma::default();
        crate::diagnosi("prova-chiusure: inizio");
        p.show_keyboard();
        for _ in 0..35 {
            p.hide_keyboard();
        }
        p.richieste.riaperta();
        for _ in 0..3 {
            p.hide_keyboard();
        }
        p.show_keyboard();
        p.hide_keyboard();
        let d = crate::diagnosi_testo();
        let dopo = d.split_once("] prova-chiusure: inizio\n").expect("manca la riga d'inizio").1;
        // In questo crate è l'unica prova che chiede di chiudere la tastiera: le righe di altri
        // thread scritte dopo l'inizio non ne contano.
        let chiusure = dopo.lines().filter(|r| r.ends_with("] tastiera: richiesta di chiuderla")).count();
        assert_eq!(chiusure, 3, "{d}");
    }

    #[test]
    fn la_finestra_inviata_va_in_diagnosi_senza_testo() {
        let p = Piattaforma::default();
        p.set_ime_state("Zanzibar è un'isola".into(), 3, 5, 4242);
        let d = crate::diagnosi_testo();
        assert!(d.contains("] tastiera: finestra v4242 inviata, 19 unità, sel 3..5\n"), "{d}");
        assert!(!d.contains("Zanzibar"), "{d}");
    }

    /// Un passo di egui con questi eventi: restituisce i tasti crudi.
    fn passo(ctx: &egui::Context, p: &Piattaforma, eventi: Vec<egui::Event>, volte: usize) -> Vec<TastoCrudo> {
        let mut crudi = Vec::new();
        let mut out = ctx.run_ui(egui::RawInput { events: eventi, ..Default::default() }, |ui| {
            for _ in 0..volte {
                crudi.extend(p.tasti_crudi(ui.ctx()));
            }
        });
        out.textures_delta.clear();
        crudi
    }

    fn tasto(key: egui::Key) -> egui::Event {
        egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }
    }

    #[test]
    fn tasti_crudi_applica_il_testo() {
        let p = Piattaforma::default();
        p.show_keyboard();
        p.set_ime_state("ab".into(), 2, 2, 1);
        let ctx = egui::Context::default();
        assert!(passo(&ctx, &p, vec![egui::Event::Text("c".into())], 1).is_empty());
        assert_eq!(p.take_ime_snapshot().unwrap().text, "abc");
    }

    #[test]
    fn tasti_crudi_lascia_il_backspace_all_inizio() {
        let p = Piattaforma::default();
        p.show_keyboard();
        p.set_ime_state("ab".into(), 0, 0, 1);
        let ctx = egui::Context::default();
        assert_eq!(passo(&ctx, &p, vec![tasto(egui::Key::Backspace)], 1), vec![TastoCrudo::Backspace]);
        assert!(p.take_ime_snapshot().is_none());
    }

    #[test]
    fn tasti_crudi_non_ripete_lo_stesso_passo() {
        let p = Piattaforma::default();
        p.show_keyboard();
        p.set_ime_state("ab".into(), 2, 2, 1);
        let ctx = egui::Context::default();
        passo(&ctx, &p, vec![egui::Event::Text("c".into())], 2);
        assert_eq!(p.take_ime_snapshot().unwrap().text, "abc");
    }

    #[test]
    fn tasti_crudi_a_finestra_nascosta() {
        let p = Piattaforma::default();
        p.show_keyboard();
        p.set_ime_state("ab".into(), 2, 2, 1);
        let ctx = egui::Context::default();
        passo(&ctx, &p, vec![egui::Event::Text("c".into())], 1);
        // Finestra nascosta: eframe chiama solo `logic`, fuori dai passi, e `ctx.input()` ha
        // ancora gli eventi dell'ultimo passo, che non vanno riscritti.
        for _ in 0..3 {
            let _ = ctx.run_logic(&egui::RawInput::default(), |ctx| assert!(p.tasti_crudi(ctx).is_empty()));
        }
        let nascosta = p.take_ime_snapshot().map(|s| s.text);
        // Finestra di nuovo visibile: gli eventi del primo passo sono nuovi e non vanno persi.
        passo(&ctx, &p, vec![egui::Event::Text("d".into())], 1);
        let visibile = p.take_ime_snapshot().map(|s| s.text);
        assert_eq!((nascosta.as_deref(), visibile.as_deref()), (Some("abc"), Some("abcd")));
    }
}
