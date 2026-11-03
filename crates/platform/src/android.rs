//! Piattaforma Android (GameActivity): l'unico file del crate che parla JNI.
//! I metodi nativi di MainActivity stanno nell'app (devono finire nel .so) e chiamano le
//! funzioni `ricevi_*` qui sotto. Ricerca: docs/ricerca/2026-10-15-android-egui-tastiera.md.
//!
//! Thread: le letture semplici (cartelle, permesso, appunti) si fanno subito dal thread di
//! android_main (`with_activity`); quello che apre schermate o scrive negli appunti va sul thread
//! principale Java (`on_activity`), senza aspettarlo.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use jni::objects::{JByteArray, JObject, JString};
use jni::strings::JNIStr;
use jni::{JValue, JavaVM, jni_sig, jni_str};
use winit::platform::android::activity::AndroidApp;
use winit::platform::android::activity::ndk::configuration::UiModeNight;

use crate::{
    ErroreLettura, EsitoFoto, Fissa, FonteFoto, ImeSnapshot, Insets, OpenRequest, RichiesteTastiera, RispostaRete, TastoCrudo, TipoTastiera,
    diagnosi, diagnosi_fissa, diagnosi_raffica, riga_insets, riga_rete, righe, risposta_dal_java,
};

/// Un errore JNI per il registro e per logcat: di un'eccezione Java solo la classe, perché il
/// messaggio può contenere un percorso col nome di un file (FileProvider, ContentResolver).
fn errore_jni(e: &jni::errors::Error) -> String {
    match e {
        jni::errors::Error::CaughtJavaException { name, .. } => format!("eccezione {name}"),
        altro => altro.to_string(),
    }
}

/// Contesto egui, per chiedere un frame dai callback Java (thread principale).
static CTX: OnceLock<egui::Context> = OnceLock::new();
// Insets in pixel fisici, scritti dal thread principale Java.
static INS_LEFT: AtomicI32 = AtomicI32::new(0);
static INS_TOP: AtomicI32 = AtomicI32::new(0);
static INS_RIGHT: AtomicI32 = AtomicI32::new(0);
static INS_BOTTOM: AtomicI32 = AtomicI32::new(0);
static IME_BOTTOM: AtomicI32 = AtomicI32::new(0);
static IME_VISIBLE: AtomicBool = AtomicBool::new(false);
/// Gli insets dell'ultima riga della diagnosi (`riga_insets`).
static INSETS_NEL_REGISTRO: Mutex<Option<Insets>> = Mutex::new(None);
/// Richieste di aprire e chiudere la tastiera, per la diagnosi: anche `ricevi_insets` deve
/// poterla dare per riaperta.
static RICHIESTE: RichiesteTastiera = RichiesteTastiera::nuove();
/// Pixel per punto dell'ultimo frame (bit di un f32; all'inizio 1,0).
static PPP: AtomicU32 = AtomicU32::new(1.0_f32.to_bits());
/// Vero all'avvio: va letto anche l'Intent con cui l'app è stata aperta.
static NEW_INTENT: AtomicBool = AtomicBool::new(true);
static PENDING_IME: Mutex<Option<ImeSnapshot>> = Mutex::new(None);
/// Istante (`InputState::time`, bit di un f64) dell'ultimo input visto da `tasti_crudi`.
/// All'inizio NaN, che non coincide con nessun istante vero.
static ULTIMO_INPUT: AtomicU64 = AtomicU64::new(f64::NAN.to_bits());

/// Richiesta di «Accesso a tutti i file», per scrivere nella diagnosi com'è finita quando l'utente
/// torna dalle impostazioni: NIENTE, poi CHIESTO (schermata in apertura), poi FUORI (l'app ha
/// perso il fuoco), poi di nuovo NIENTE quando lo ritrova.
static ACCESSO_CHIESTO: AtomicU8 = AtomicU8::new(NIENTE);
/// Il tipo della tastiera chiesto per ultimo (`TipoTastiera::codice`): MainActivity parte col
/// testo (onCreate).
static TIPO_TASTIERA: AtomicU8 = AtomicU8::new(0);
const NIENTE: u8 = 0;
const CHIESTO: u8 = 1;
const FUORI: u8 = 2;

/// Percorso assoluto di un `java.io.File`.
fn percorso_assoluto(env: &mut jni::Env<'_>, file: &JObject<'_>) -> jni::errors::Result<PathBuf> {
    if file.is_null() {
        return Err(jni::errors::Error::NullPtr("java.io.File"));
    }
    let p = env.call_method(file, jni_str!("getAbsolutePath"), jni_sig!("()Ljava/lang/String;"), &[])?.l()?;
    Ok(PathBuf::from(env.cast_local::<JString>(p)?.try_to_string(env)?))
}

/// Sul thread principale: la schermata «Accesso a tutti i file» di Tascabile
/// (`ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION` con `package:`); se nessuna Activity la
/// gestisce, l'elenco di tutte le app (`ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION`). Da
/// `Settings.java` (AOSP): «In some cases, a matching Activity may not exist, so ensure you
/// safeguard against this».
fn apri_impostazioni_accesso(env: &mut jni::Env<'_>, activity: &JObject<'_>) -> jni::errors::Result<()> {
    let pkg = env.call_method(activity, jni_str!("getPackageName"), jni_sig!("()Ljava/lang/String;"), &[])?.l()?;
    let pkg = env.cast_local::<JString>(pkg)?.try_to_string(env)?;
    let uri_s = env.new_string(format!("package:{pkg}"))?;
    let uri = env
        .call_static_method(
            jni_str!("android/net/Uri"),
            jni_str!("parse"),
            jni_sig!("(Ljava/lang/String;)Landroid/net/Uri;"),
            &[JValue::Object(&uri_s)],
        )?
        .l()?;
    let azione = env.new_string("android.settings.MANAGE_APP_ALL_FILES_ACCESS_PERMISSION")?;
    let intent = env.new_object(
        jni_str!("android/content/Intent"),
        jni_sig!("(Ljava/lang/String;Landroid/net/Uri;)V"),
        &[JValue::Object(&azione), JValue::Object(&uri)],
    )?;
    match env.call_method(activity, jni_str!("startActivity"), jni_sig!("(Landroid/content/Intent;)V"), &[JValue::Object(&intent)]) {
        Ok(_) => return Ok(()),
        Err(jni::errors::Error::JavaException) => {
            // L'eccezione resta in sospeso finché non la si prende: solo dopo si può chiamare altro.
            let perche = env.exception_catch().err().map_or_else(|| "?".to_string(), |e| errore_jni(&e));
            diagnosi(format!("accesso ai file: schermata di Tascabile assente ({perche}), si apre l'elenco delle app"));
        }
        Err(e) => return Err(e),
    }
    let azione = env.new_string("android.settings.MANAGE_ALL_FILES_ACCESS_PERMISSION")?;
    let intent = env.new_object(jni_str!("android/content/Intent"), jni_sig!("(Ljava/lang/String;)V"), &[JValue::Object(&azione)])?;
    env.call_method(activity, jni_str!("startActivity"), jni_sig!("(Landroid/content/Intent;)V"), &[JValue::Object(&intent)])?;
    Ok(())
}

fn repaint() {
    if let Some(ctx) = CTX.get() {
        ctx.request_repaint();
    }
}

/// Da MainActivity.stateChanged: la tastiera ha cambiato testo, selezione o composizione.
///
/// È l'unico punto che tratta il -1 di Java (selezione assente nell'Editable): il cursore va alla
/// fine del testo, in unità UTF-16, così il ponte non vede mai una selezione negativa.
pub fn ricevi_stato_ime(mut snap: ImeSnapshot) {
    if snap.sel.0 < 0 || snap.sel.1 < 0 {
        let fine = snap.text.encode_utf16().count() as i32;
        // Solo gli indici: il testo battuto non va in logcat; di una password nemmeno la misura.
        let riga = righe::senza_selezione(snap.version, snap.sel, fine, TipoTastiera::da_codice(TIPO_TASTIERA.load(Ordering::Relaxed)));
        log::warn!("{riga}");
        diagnosi(riga);
        snap.sel = (fine, fine);
    }
    let versione = snap.version;
    let sostituito = PENDING_IME.lock().unwrap_or_else(|e| e.into_inner()).replace(snap);
    // L'app prende un solo stato per frame, il più nuovo: quello sostituito non avrà la sua riga
    // dal ponte.
    if let Some(vecchio) = sostituito {
        diagnosi(format!("tastiera: stato v{} sostituito da uno più nuovo (v{versione}) prima del frame", vecchio.version));
    }
    repaint();
}

/// Da MainActivity: barre di sistema e tastiera, in pixel fisici. `finale`: da
/// onApplyWindowInsets, che prima di un'animazione della tastiera manda già lo stato di fine
/// animazione; se no è un frame dell'animazione (onProgress). Tutti e due servono alla
/// schermata, solo il primo alla diagnosi (`riga_insets`).
pub fn ricevi_insets(left: i32, top: i32, right: i32, bottom: i32, ime_bottom: i32, ime_visible: bool, finale: bool) {
    let r = Ordering::Relaxed;
    INS_LEFT.store(left, r);
    INS_TOP.store(top, r);
    INS_RIGHT.store(right, r);
    INS_BOTTOM.store(bottom, r);
    IME_BOTTOM.store(ime_bottom, r);
    IME_VISIBLE.store(ime_visible, r);
    if finale && ime_visible {
        // La tastiera si apre, anche da sé (per esempio al ritorno nell'app): la prossima
        // richiesta di chiuderla va nella diagnosi.
        RICHIESTE.riaperta();
    }
    let nuovi = Insets { sinistra: left, sopra: top, destra: right, sotto: bottom, tastiera: ime_bottom, visibile: ime_visible };
    let riga = riga_insets(&mut INSETS_NEL_REGISTRO.lock().unwrap_or_else(|e| e.into_inner()), nuovi, finale);
    if let Some((tipo, riga)) = riga {
        diagnosi_raffica(tipo, riga);
    }
    repaint();
}

/// Le foto chieste a MainActivity: la richiesta in corso, col suo numero, e il suo esito. Dietro
/// un Mutex, perché l'esito arriva dal thread di Java (anche quello che copia le immagini scelte)
/// e la domanda dal thread dell'interfaccia: il confronto col numero e la scrittura dell'esito
/// vanno insieme (revisione del codice della 0.8.3).
static FOTO: Mutex<crate::CanaleFoto> = Mutex::new(crate::CanaleFoto::NUOVO);
/// La cache dell'app, per cancellare le foto di una richiesta vecchia appena arrivano.
static CACHE: OnceLock<PathBuf> = OnceLock::new();

fn canale_foto() -> std::sync::MutexGuard<'static, crate::CanaleFoto> {
    FOTO.lock().unwrap_or_else(|e| e.into_inner())
}

/// Il primo numero delle richieste di foto: dall'ora, così non torna uguale a uno di prima che
/// Android togliesse l'app.
fn seme_delle_richieste() -> i32 {
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_millis());
    ((ms % 1_000_000_000) as i32).max(1)
}

/// Da MainActivity.onActivityResult (thread principale o quello che copia le immagini scelte):
/// per la richiesta `richiesta`, la foto è nella sua cartella della cache (1; per più foto dalla
/// galleria quante, su `scelte`), non c'è (0), non si è letta (-1). L'esito di una richiesta
/// superata si lascia, e la sua cartella si cancella.
pub fn ricevi_foto_firma(richiesta: i32, esito: i32, scelte: i32) {
    let tenuto = canale_foto().ricevi(richiesta, esito, scelte);
    if !tenuto {
        if let Some(cache) = CACHE.get() {
            let _ = std::fs::remove_dir_all(crate::cartella_delle_scelte(cache, richiesta));
        }
        diagnosi("foto: esito di una richiesta superata, lasciato");
        return;
    }
    repaint();
}

/// L'impronta chiesta: il numero dell'ultima richiesta e il suo esito, quando arriva da
/// MainActivity (`nativeImpronta`). L'esito di una richiesta vecchia si lascia.
static IMPRONTA: Mutex<(i32, Option<crate::EsitoImpronta>)> = Mutex::new((0, None));

/// L'esito della finestra dell'impronta per la richiesta `richiesta` (codice di MainActivity).
pub fn ricevi_impronta(richiesta: i32, codice: i32) {
    let mut g = IMPRONTA.lock().unwrap_or_else(|e| e.into_inner());
    if g.0 == richiesta {
        g.1 = Some(crate::EsitoImpronta::da_codice(codice));
    }
    drop(g);
    repaint();
}

/// Lo schermo esterno della presentazione (larghezza e altezza in pixel), da MainActivity: c'è
/// solo mentre si presenta e uno schermo è attaccato.
static SCHERMO_ESTERNO: Mutex<Option<(u32, u32)>> = Mutex::new(None);

/// Da MainActivity (thread principale): uno schermo esterno è pronto, larghezza × altezza in
/// pixel, o non c'è più (0 × 0).
pub fn ricevi_schermo_esterno(larghezza: i32, altezza: i32) {
    let misura = (larghezza > 0 && altezza > 0).then(|| (larghezza as u32, altezza as u32));
    *SCHERMO_ESTERNO.lock().unwrap_or_else(|e| e.into_inner()) = misura;
    repaint();
}

/// Da MainActivity.onNewIntent: un nuovo «Apri con» ad app aperta.
pub fn ricevi_nuovo_intent() {
    NEW_INTENT.store(true, Ordering::Release);
    repaint();
}

/// La rete del timestamp, l'unica cosa per cui l'app va in rete: chiede al Java
/// (`MainActivity.tascabileRete`, HttpsURLConnection) dal thread che chiama, che si attacca alla
/// JVM. Va usata da un thread di lavoro: una richiesta aspetta fino a 25 secondi.
#[derive(Clone)]
pub struct Rete {
    app: AndroidApp,
    /// Il cancello della rete, chiuso finché chi usa l'app non dà il consenso a un'operazione
    /// (`Piattaforma::consenti_rete`); lo stesso in Java (`tascabileConsentiRete`).
    aperta: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Rete {
    /// Una richiesta https: POST col corpo, GET senza.
    pub fn chiedi(&self, indirizzo: &str, corpo: Option<&[u8]>) -> RispostaRete {
        if !self.aperta.load(std::sync::atomic::Ordering::SeqCst) {
            diagnosi("rete: chiusa, nessun consenso");
            return RispostaRete::Collegamento;
        }
        let inizio = std::time::Instant::now();
        // SAFETY: il puntatore viene da android-activity ed è quello della JavaVM del processo.
        let vm = unsafe { JavaVM::from_raw(self.app.vm_as_ptr().cast()) };
        let esito = vm.attach_current_thread(|env| -> jni::errors::Result<Vec<u8>> {
            let raw: jni::sys::jobject = self.app.activity_as_ptr().cast();
            // SAFETY: come in `Piattaforma::with_activity`: un riferimento globale che non ci
            // appartiene, valido finché vive `self.app`.
            let activity = unsafe { env.as_cast_raw::<JObject>(&raw)? };
            let url = env.new_string(indirizzo)?;
            let corpo: JObject = match corpo {
                Some(c) => env.byte_array_from_slice(c)?.into(),
                None => JObject::null(),
            };
            let r = env
                .call_method(
                    &activity,
                    jni_str!("tascabileRete"),
                    jni_sig!("(Ljava/lang/String;[B)[B"),
                    &[JValue::Object(&url), JValue::Object(&corpo)],
                )?
                .l()?;
            if r.is_null() {
                return Ok(Vec::new());
            }
            let r = env.cast_local::<JByteArray>(r)?;
            env.convert_byte_array(&r)
        });
        let r = match esito {
            Ok(byte) => risposta_dal_java(&byte),
            Err(err) => {
                log::error!("rete: {}", errore_jni(&err));
                RispostaRete::Collegamento
            }
        };
        diagnosi(riga_rete(&r, inizio.elapsed()));
        r
    }
}

/// Tutto ciò che il resto dell'app può chiedere ad Android.
pub struct Piattaforma {
    app: AndroidApp,
    // Cartelle che non cambiano finché vive il processo: si chiedono a Java una volta sola.
    cartella_app: OnceLock<PathBuf>,
    cartella_cache: OnceLock<PathBuf>,
    radice: OnceLock<PathBuf>,
    /// Il cancello della rete del timestamp, chiuso all'avvio.
    rete_aperta: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Piattaforma {
    pub fn nuova(app: AndroidApp) -> Self {
        Self {
            app,
            cartella_app: OnceLock::new(),
            cartella_cache: OnceLock::new(),
            radice: OnceLock::new(),
            rete_aperta: std::sync::Arc::default(),
        }
    }

    /// Apre o chiude il cancello della rete (livello 1 della roadmap della sicurezza), in Rust e
    /// in Java (`MainActivity.tascabileConsentiRete`).
    pub fn consenti_rete(&self, aperta: bool) {
        self.rete_aperta.store(aperta, std::sync::atomic::Ordering::SeqCst);
        self.on_activity(move |env, activity| {
            env.call_method(activity, jni_str!("tascabileConsentiRete"), jni_sig!("(Z)V"), &[JValue::Bool(aperta)])?;
            Ok(())
        });
    }

    /// Dati dell'app (recenti, recupero): `getFilesDir()`, che Android crea da sé.
    pub fn cartella_app(&self) -> PathBuf {
        let leggi = || self.cartella_dell_activity(jni_str!("getFilesDir"), "cartella dell'app", "/data/user/0/it.tascabile.app/files");
        self.cartella_app.get_or_init(leggi).clone()
    }

    /// Cache dell'app (file da condividere in `condivisi/`): `getCacheDir()`.
    pub fn cartella_cache(&self) -> PathBuf {
        let leggi = || self.cartella_dell_activity(jni_str!("getCacheDir"), "cartella della cache", "/data/user/0/it.tascabile.app/cache");
        self.cartella_cache.get_or_init(leggi).clone()
    }

    /// Radice della memoria condivisa, di solito /storage/emulated/0:
    /// `Environment.getExternalStorageDirectory()`, che con l'accesso a tutti i file si usa con i
    /// percorsi normali.
    pub fn radice_archivio(&self) -> PathBuf {
        let leggi = || {
            self.with_activity(|env, _activity| {
                let file = env
                    .call_static_method(
                        jni_str!("android/os/Environment"),
                        jni_str!("getExternalStorageDirectory"),
                        jni_sig!("()Ljava/io/File;"),
                        &[],
                    )?
                    .l()?;
                percorso_assoluto(env, &file)
            })
            .unwrap_or_else(|e| {
                diagnosi(format!("radice dei documenti: non letta ({}), uso /storage/emulated/0", errore_jni(&e)));
                PathBuf::from("/storage/emulated/0")
            })
        };
        self.radice.get_or_init(leggi).clone()
    }

    /// Una cartella dell'Activity (`metodo` senza argomenti che restituisce un `java.io.File`); se
    /// JNI non risponde, il percorso solito dell'utente principale (`ripiego`).
    fn cartella_dell_activity(&self, metodo: &'static JNIStr, quale: &str, ripiego: &str) -> PathBuf {
        self.with_activity(|env, activity| {
            let file = env.call_method(activity, metodo, jni_sig!("()Ljava/io/File;"), &[])?.l()?;
            percorso_assoluto(env, &file)
        })
        .unwrap_or_else(|e| {
            diagnosi(format!("{quale}: non letta ({}), uso il percorso solito", errore_jni(&e)));
            PathBuf::from(ripiego)
        })
    }

    /// «Accesso a tutti i file» concesso? `Environment.isExternalStorageManager()` (Android 11+).
    pub fn accesso_file(&self) -> bool {
        self.with_activity(|env, _activity| {
            env.call_static_method(jni_str!("android/os/Environment"), jni_str!("isExternalStorageManager"), jni_sig!("()Z"), &[])?.z()
        })
        .unwrap_or_else(|e| {
            diagnosi(format!("accesso ai file: controllo non riuscito ({})", errore_jni(&e)));
            false
        })
    }

    /// Apre la schermata di sistema per concedere l'accesso a tutti i file. Quando l'utente torna
    /// nell'app, la diagnosi dice com'è finita (`apply_raw_input`).
    pub fn chiedi_accesso_file(&self) {
        diagnosi("accesso ai file: si apre la schermata delle impostazioni");
        ACCESSO_CHIESTO.store(CHIESTO, Ordering::Relaxed);
        self.on_activity(|env, activity| {
            let esito = apri_impostazioni_accesso(env, activity);
            if esito.is_err() {
                ACCESSO_CHIESTO.store(NIENTE, Ordering::Relaxed);
            }
            esito
        });
    }

    /// Condivide un file di `cartella_cache()/condivisi/` col foglio di sistema (FileProvider in
    /// `MainActivity.tascabileCondividiFile`). Nella diagnosi tipo e misura, mai il nome. Un file
    /// fuori da lì FileProvider lo rifiuta: si controlla già qui, come sul PC, così la riga di una
    /// condivisione partita non compare per un file che non partirà.
    pub fn condividi_file(&self, percorso: &Path, mime: &str, titolo: &str) {
        if !righe::dentro_condivisi(&self.cartella_cache(), percorso) {
            diagnosi(righe::CONDIVIDI_FUORI);
            return;
        }
        diagnosi(righe::condividi_file(percorso, mime));
        let (percorso, mime, titolo) = (percorso.to_string_lossy().into_owned(), mime.to_owned(), titolo.to_owned());
        self.on_activity(move |env, activity| {
            let (p, m, t) = (env.new_string(&percorso)?, env.new_string(&mime)?, env.new_string(&titolo)?);
            env.call_method(
                activity,
                jni_str!("tascabileCondividiFile"),
                jni_sig!("(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V"),
                &[JValue::Object(&p), JValue::Object(&m), JValue::Object(&t)],
            )?;
            Ok(())
        });
    }

    /// Un link esterno di un PDF, già confermato da chi legge, all'app di sistema che lo apre (di
    /// solito il browser) con un Intent `ACTION_VIEW` (`MainActivity.tascabileApriIndirizzo`):
    /// la pagina la apre un'altra app (la rete di Tascabile serve solo al timestamp). Solo pagine
    /// web e mail; nella diagnosi il genere, mai l'indirizzo.
    pub fn apri_indirizzo(&self, indirizzo: &str) {
        let Some(genere) = crate::genere_indirizzo(indirizzo) else {
            diagnosi("link esterno: indirizzo che non si apre");
            return;
        };
        diagnosi(format!("link esterno: {genere} aperta fuori dall'app"));
        let indirizzo = indirizzo.trim().to_owned();
        self.on_activity(move |env, activity| {
            let s = env.new_string(&indirizzo)?;
            env.call_method(activity, jni_str!("tascabileApriIndirizzo"), jni_sig!("(Ljava/lang/String;)V"), &[JValue::Object(&s)])?;
            Ok(())
        });
    }

    /// La foto della firma su carta: la fotocamera di sistema (`ACTION_IMAGE_CAPTURE`, che
    /// scrive nella cartella della richiesta tramite il FileProvider) o la scelta di un'immagine
    /// (`ACTION_GET_CONTENT`, copiata lì da MainActivity); nessun permesso. Il numero della
    /// richiesta, con cui `prendi_foto_firma` ne dà l'esito.
    pub fn chiedi_foto_firma(&self, fonte: FonteFoto) -> i32 {
        diagnosi(fonte.chiesta());
        let _ = CACHE.set(self.cartella_cache());
        let richiesta = canale_foto().chiedi(fonte, seme_delle_richieste);
        let codice = fonte.codice();
        self.on_activity(move |env, activity| {
            env.call_method(activity, jni_str!("tascabileFotoFirma"), jni_sig!("(II)V"), &[JValue::Int(codice), JValue::Int(richiesta)])?;
            Ok(())
        });
        richiesta
    }

    /// La richiesta di foto in corso (0 prima della prima).
    pub fn richiesta_foto_in_corso(&self) -> i32 {
        canale_foto().in_corso()
    }

    /// Chiede l'impronta (o il PIN, la sequenza, la password del telefono) con la finestra di
    /// sistema di Android (`MainActivity.tascabileImpronta`): il numero della richiesta, con cui
    /// `esito_impronta` ne dà l'esito.
    pub fn chiedi_impronta(&self, titolo: &str, spiegazione: &str) -> i32 {
        let richiesta = {
            let mut g = IMPRONTA.lock().unwrap_or_else(|e| e.into_inner());
            g.0 = g.0.checked_add(1).unwrap_or(1).max(1);
            g.1 = None;
            g.0
        };
        let (titolo, spiegazione) = (titolo.to_string(), spiegazione.to_string());
        self.on_activity(move |env, activity| {
            let mut chiama = || -> jni::errors::Result<()> {
                let (t, s) = (env.new_string(&titolo)?, env.new_string(&spiegazione)?);
                env.call_method(
                    activity,
                    jni_str!("tascabileImpronta"),
                    jni_sig!("(ILjava/lang/String;Ljava/lang/String;)V"),
                    &[JValue::Int(richiesta), JValue::Object(&t), JValue::Object(&s)],
                )?;
                Ok(())
            };
            // La finestra non si è aperta: un esito «errore», non una richiesta appesa per sempre.
            let esito = chiama();
            if esito.is_err() {
                ricevi_impronta(richiesta, -1);
            }
            esito
        });
        richiesta
    }

    /// L'esito della richiesta d'impronta `richiesta`, se è arrivato (una volta sola).
    pub fn esito_impronta(&self, richiesta: i32) -> Option<crate::EsitoImpronta> {
        let mut g = IMPRONTA.lock().unwrap_or_else(|e| e.into_inner());
        if g.0 == richiesta { g.1.take() } else { None }
    }

    /// Un file degli asset dell'APK (i modelli del riconoscimento del testo, il dizionario
    /// italiano), letto tutto; None se manca o non si legge. Nella diagnosi solo il nome
    /// dell'asset, che è nostro.
    pub fn leggi_asset(&self, nome: &str) -> Option<Vec<u8>> {
        let c = std::ffi::CString::new(nome).ok()?;
        let Some(mut asset) = self.app.asset_manager().open(&c) else {
            diagnosi(format!("asset {nome}: manca"));
            return None;
        };
        let mut byte = Vec::new();
        match std::io::Read::read_to_end(&mut asset, &mut byte) {
            Ok(_) => Some(byte),
            Err(e) => {
                diagnosi(format!("asset {nome}: non letto ({:?})", e.kind()));
                None
            }
        }
    }

    /// L'esito della richiesta `richiesta`, una volta sola: None se non è ancora arrivato,
    /// [`EsitoFoto::Superata`] se nel frattempo ne è partita un'altra.
    pub fn prendi_foto_firma(&self, richiesta: i32) -> Option<EsitoFoto> {
        canale_foto().prendi(richiesta, &self.cartella_cache())
    }

    /// All'avvio: via le foto rimaste nella cache da prima.
    pub fn pulisci_le_foto(&self) {
        crate::pulisci_le_foto(&self.cartella_cache());
    }

    /// All'avvio: via le copie condivise più vecchie di un'ora.
    pub fn pulisci_i_condivisi(&self) {
        crate::pulisci_i_condivisi(&self.cartella_cache(), crate::ETA_DEI_CONDIVISI);
    }

    /// Condivide un testo (il registro della diagnosi) col foglio di sistema.
    pub fn condividi_testo(&self, testo: &str, titolo: &str) {
        diagnosi(righe::condividi_testo(testo));
        let (testo, titolo) = (testo.to_owned(), titolo.to_owned());
        self.on_activity(move |env, activity| {
            let (s, t) = (env.new_string(&testo)?, env.new_string(&titolo)?);
            env.call_method(
                activity,
                jni_str!("tascabileCondividiTesto"),
                jni_sig!("(Ljava/lang/String;Ljava/lang/String;)V"),
                &[JValue::Object(&s), JValue::Object(&t)],
            )?;
            Ok(())
        });
    }

    /// La presentazione delle Slide comincia o finisce (`MainActivity.tascabilePresentazione`):
    /// mentre c'è lo schermo resta acceso, le barre di sistema si nascondono e i tasti del volume
    /// diventano Pagina giù e Pagina su, cioè avanti e indietro.
    pub fn presentazione(&self, attiva: bool) {
        self.on_activity(move |env, activity| {
            env.call_method(activity, jni_str!("tascabilePresentazione"), jni_sig!("(Z)V"), &[JValue::Bool(attiva)])?;
            Ok(())
        });
    }

    /// La modalità riservata delle Impostazioni (`MainActivity.tascabileRiservata`): accesa, la
    /// finestra è nera negli screenshot, nelle registrazioni e nella condivisione dello schermo
    /// (FLAG_SECURE), e da Android 12 le finestre di altre app non stanno sopra la nostra.
    pub fn riservata(&self, accesa: bool) {
        self.on_activity(move |env, activity| {
            env.call_method(activity, jni_str!("tascabileRiservata"), jni_sig!("(Z)V"), &[JValue::Bool(accesa)])?;
            Ok(())
        });
    }

    /// La rete per il timestamp, da usare su un thread di lavoro.
    pub fn rete(&self) -> Rete {
        Rete { app: self.app.clone(), aperta: self.rete_aperta.clone() }
    }

    /// Lo schermo esterno della presentazione (cavo HDMI, Miracast) in pixel, se c'è.
    pub fn schermo_esterno(&self) -> Option<(u32, u32)> {
        *SCHERMO_ESTERNO.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Un'immagine per lo schermo esterno (RGBA premoltiplicato, come la Bitmap ARGB_8888 di
    /// Android in memoria), a `MainActivity.tascabileProietta` sul thread principale.
    pub fn proietta(&self, pixel: Vec<u8>, larghezza: u32, altezza: u32) {
        self.on_activity(move |env, activity| {
            let byte = env.byte_array_from_slice(&pixel)?;
            env.call_method(
                activity,
                jni_str!("tascabileProietta"),
                jni_sig!("([BII)V"),
                &[JValue::Object(&byte), JValue::Int(larghezza as i32), JValue::Int(altezza as i32)],
            )?;
            Ok(())
        });
    }

    /// Testo negli appunti di sistema (ClipboardManager), sul thread principale.
    pub fn copia_testo(&self, testo: &str) {
        diagnosi(righe::copia(testo));
        let testo = testo.to_owned();
        self.on_activity(move |env, activity| {
            let s = env.new_string(&testo)?;
            env.call_method(activity, jni_str!("tascabileCopy"), jni_sig!("(Ljava/lang/String;)V"), &[JValue::Object(&s)])?;
            Ok(())
        });
    }

    /// Testo degli appunti di sistema, letto subito dal thread di android_main: serve la
    /// risposta, e `on_activity` non aspetta. Si può: `ClipboardManager.getPrimaryClip()` è una
    /// chiamata Binder al servizio, senza Looper; l'unico Handler del ClipboardManager è quello
    /// del thread principale, che `SystemServiceRegistry` gli passa, e serve solo agli
    /// ascoltatori dei cambi (AOSP, `ClipboardManager.java` e `SystemServiceRegistry.java`). Da
    /// Android 10 risponde null se l'app non ha il fuoco: chiamarla dopo un tocco nell'app.
    pub fn incolla_testo(&self) -> Option<String> {
        let testo = self
            .with_activity(|env, activity| {
                let v = env.call_method(activity, jni_str!("tascabilePaste"), jni_sig!("()Ljava/lang/String;"), &[])?.l()?;
                if v.is_null() {
                    return Ok(None);
                }
                Ok(Some(env.cast_local::<JString>(v)?.try_to_string(env)?))
            })
            .unwrap_or_else(|e| {
                let e = errore_jni(&e);
                log::error!("incolla: {e}");
                diagnosi(format!("appunti: lettura non riuscita ({e})"));
                None
            });
        let testo = crate::entro_il_tetto_dell_incolla(testo);
        diagnosi(righe::incolla(testo.as_deref()));
        testo
    }

    /// Il contesto egui serve per chiedere un frame quando Java manda tastiera, insets o Intent.
    pub fn imposta_contesto(&self, ctx: &egui::Context) {
        let _ = CTX.set(ctx.clone());
    }

    pub fn show_keyboard(&self) {
        RICHIESTE.apri();
        // false = richiesta esplicita: con SHOW_IMPLICIT Android può non mostrarla.
        self.app.show_soft_input(false);
    }

    /// Si può chiamare a ogni frame: nella diagnosi va solo la prima richiesta di una serie.
    pub fn hide_keyboard(&self) {
        RICHIESTE.chiudi();
        // false: chiude anche la tastiera aperta con una richiesta esplicita.
        self.app.hide_soft_input(false);
    }

    /// Secondo gli ultimi insets mandati da Java.
    pub fn keyboard_visible(&self) -> bool {
        IME_VISIBLE.load(Ordering::Relaxed)
    }

    /// Altezza della tastiera in punti egui (0 se chiusa).
    pub fn keyboard_height(&self) -> f32 {
        let ppp = f32::from_bits(PPP.load(Ordering::Relaxed)).max(0.1);
        IME_BOTTOM.load(Ordering::Relaxed).max(0) as f32 / ppp
    }

    pub fn take_ime_snapshot(&self) -> Option<ImeSnapshot> {
        PENDING_IME.lock().unwrap_or_else(|e| e.into_inner()).take()
    }

    /// La tastiera per il campo che ha il fuoco (cifre per «Vai a pagina», password senza
    /// suggerimenti): MainActivity cambia l'EditorInfo di GameTextInput e fa ripartire la
    /// tastiera. Solo quando cambia; prima di `set_ime_state` e di `show_keyboard`, che vanno
    /// dopo di lei sul thread principale.
    pub fn tipo_tastiera(&self, tipo: TipoTastiera) {
        let codice = tipo.codice();
        if TIPO_TASTIERA.swap(codice as u8, Ordering::Relaxed) == codice as u8 {
            return;
        }
        diagnosi(righe::tipo_tastiera(tipo));
        self.on_activity(move |env, activity| {
            env.call_method(activity, jni_str!("tascabileTipoTastiera"), jni_sig!("(I)V"), &[JValue::Int(codice)])?;
            Ok(())
        });
    }

    /// Dà alla tastiera il testo della finestra e la selezione (unità UTF-16).
    pub fn set_ime_state(&self, text: String, sel_start: i32, sel_end: i32, version: i64) {
        let tipo = TipoTastiera::da_codice(TIPO_TASTIERA.load(Ordering::Relaxed));
        diagnosi(righe::finestra(&text, sel_start, sel_end, version, tipo));
        self.on_activity(move |env, activity| {
            let jtext = env.new_string(&text)?;
            env.call_method(
                activity,
                jni_str!("tascabileSetIme"),
                jni_sig!("(Ljava/lang/String;IIJ)V"),
                &[JValue::Object(&jtext), JValue::Int(sel_start), JValue::Int(sel_end), JValue::Long(version)],
            )?;
            Ok(())
        });
    }

    /// Con la tastiera aperta GameTextInput consuma i tasti; arrivano a egui solo quelli che lascia
    /// passare (per esempio Backspace col cursore a 0) e quelli di una tastiera fisica.
    pub fn tasti_crudi(&self, ctx: &egui::Context) -> Vec<TastoCrudo> {
        // eframe chiama `logic` anche fuori dai passi (finestra nascosta), e allora `ctx.input()` ha
        // ancora gli eventi dell'ultimo passo: non vanno ripetuti. Il passo si riconosce
        // dall'istante del suo input, come nella piattaforma finta (desktop.rs): il contatore
        // dei passi cresce solo in `end_pass` e dentro `logic` non basta.
        let istante = ctx.input(|i| i.time).to_bits();
        if ULTIMO_INPUT.swap(istante, Ordering::Relaxed) == istante {
            return Vec::new();
        }
        ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Key { key: egui::Key::Backspace, pressed: true, .. } => Some(TastoCrudo::Backspace),
                    egui::Event::Key { key: egui::Key::Delete, pressed: true, .. } => Some(TastoCrudo::Canc),
                    egui::Event::Key { key: egui::Key::Enter, pressed: true, .. } => Some(TastoCrudo::Invio),
                    _ => None,
                })
                .collect()
        })
    }

    /// Barre di sistema e tastiera nella zona sicura di egui (punti); tema chiaro o scuro.
    pub fn apply_raw_input(&self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        // Dopo `chiedi_accesso_file`: l'app perde il fuoco quando si aprono le impostazioni e lo
        // ritrova quando l'utente torna; allora la diagnosi dice se adesso l'accesso c'è.
        match ACCESSO_CHIESTO.load(Ordering::Relaxed) {
            CHIESTO if !raw.focused => ACCESSO_CHIESTO.store(FUORI, Ordering::Relaxed),
            FUORI if raw.focused => {
                ACCESSO_CHIESTO.store(NIENTE, Ordering::Relaxed);
                let accesso = if self.accesso_file() { "concesso" } else { "negato" };
                diagnosi_fissa(Fissa::AccessoFile, format!("accesso ai file dopo la richiesta: {accesso}"));
            }
            _ => {}
        }
        let nativi = raw.viewport().native_pixels_per_point.unwrap_or(1.0);
        let ppp = (nativi * ctx.zoom_factor()).max(0.1);
        PPP.store(ppp.to_bits(), Ordering::Relaxed);
        let pt = |px: i32| px.max(0) as f32 / ppp;
        // In basso la più alta fra barra di navigazione e tastiera.
        let basso = INS_BOTTOM.load(Ordering::Relaxed).max(IME_BOTTOM.load(Ordering::Relaxed));
        raw.safe_area_insets = Some(egui::SafeAreaInsets(egui::epaint::MarginF32 {
            left: pt(INS_LEFT.load(Ordering::Relaxed)),
            right: pt(INS_RIGHT.load(Ordering::Relaxed)),
            top: pt(INS_TOP.load(Ordering::Relaxed)),
            bottom: pt(basso),
        }));
        // winit su Android non conosce il tema: lo si legge dalla Configuration.
        raw.system_theme = match self.app.config().ui_mode_night() {
            UiModeNight::Yes => Some(egui::Theme::Dark),
            UiModeNight::No => Some(egui::Theme::Light),
            _ => None,
        };
    }

    /// La richiesta «Apri con» arrivata all'avvio o con onNewIntent; rifiutata, il perché.
    pub fn take_open_request(&self) -> Option<Result<OpenRequest, &'static str>> {
        if !NEW_INTENT.swap(false, Ordering::AcqRel) {
            return None;
        }
        self.with_activity(|env, activity| {
            let intent = env.call_method(activity, jni_str!("getIntent"), jni_sig!("()Landroid/content/Intent;"), &[])?.l()?;
            if intent.is_null() {
                return Ok(None);
            }
            let action = env.call_method(&intent, jni_str!("getAction"), jni_sig!("()Ljava/lang/String;"), &[])?.l()?;
            if action.is_null() {
                return Ok(None);
            }
            let action = env.cast_local::<JString>(action)?.try_to_string(env)?;
            if action != "android.intent.action.VIEW" {
                return Ok(None);
            }
            let data = env.call_method(&intent, jni_str!("getData"), jni_sig!("()Landroid/net/Uri;"), &[])?.l()?;
            if data.is_null() {
                return Ok(None);
            }
            let uri = env.call_method(&data, jni_str!("toString"), jni_sig!("()Ljava/lang/String;"), &[])?.l()?;
            let uri = env.cast_local::<JString>(uri)?.try_to_string(env)?;
            // Solo content:// di altre app (audit del 30/10, problema 4); nella diagnosi il perché.
            if let Err(perche) = crate::uri_da_aprire(&uri) {
                diagnosi(format!("apri con: indirizzo non aperto ({perche})"));
                return Ok(Some(Err(perche)));
            }
            let mime = env.call_method(&intent, jni_str!("getType"), jni_sig!("()Ljava/lang/String;"), &[])?.l()?;
            let mime = if mime.is_null() { None } else { Some(env.cast_local::<JString>(mime)?.try_to_string(env)?) };
            // Il nome vero del file (DISPLAY_NAME), che la parte Java chiede al ContentResolver:
            // per i recenti e per «Salva una copia», mai per la diagnosi.
            let nome = env
                .call_method(activity, jni_str!("tascabileNomeUri"), jni_sig!("(Landroid/net/Uri;)Ljava/lang/String;"), &[JValue::Object(&data)])?
                .l()?;
            let nome = if nome.is_null() { None } else { Some(env.cast_local::<JString>(nome)?.try_to_string(env)?) };
            Ok(Some(Ok(OpenRequest { uri, mime, nome })))
        })
        .unwrap_or_else(|err| {
            let err = errore_jni(&err);
            log::error!("Intent: {err}");
            diagnosi(format!("apri con: Intent non leggibile ({err})"));
            None
        })
    }

    /// Legge tutto un URI content:// (il descrittore può essere una pipe: niente seek). Il genere
    /// dell'errore dice quale dei casi è stato, senza l'URI: permesso scaduto
    /// (SecurityException), file sparito (FileNotFoundException), errore di lettura, tetto.
    pub fn read_uri(&self, uri: &str) -> Result<Vec<u8>, ErroreLettura> {
        use std::os::fd::FromRawFd;
        let uri = uri.to_owned();
        let fd = self
            .with_activity(move |env, activity| {
                let resolver =
                    env.call_method(activity, jni_str!("getContentResolver"), jni_sig!("()Landroid/content/ContentResolver;"), &[])?.l()?;
                let juri_s = env.new_string(&uri)?;
                let juri = env
                    .call_static_method(
                        jni_str!("android/net/Uri"),
                        jni_str!("parse"),
                        jni_sig!("(Ljava/lang/String;)Landroid/net/Uri;"),
                        &[JValue::Object(&juri_s)],
                    )?
                    .l()?;
                let mode = env.new_string("r")?;
                let pfd = env
                    .call_method(
                        &resolver,
                        jni_str!("openFileDescriptor"),
                        jni_sig!("(Landroid/net/Uri;Ljava/lang/String;)Landroid/os/ParcelFileDescriptor;"),
                        &[JValue::Object(&juri), JValue::Object(&mode)],
                    )?
                    .l()?;
                if pfd.is_null() {
                    return Err(jni::errors::Error::NullPtr("openFileDescriptor"));
                }
                env.call_method(&pfd, jni_str!("detachFd"), jni_sig!("()I"), &[])?.i()
            })
            .map_err(|e| ErroreLettura { genere: errore_jni(&e), messaggio: e.to_string() })?;
        if fd < 0 {
            let messaggio = format!("descrittore non valido ({fd})");
            return Err(ErroreLettura { genere: messaggio.clone(), messaggio });
        }
        // SAFETY: detachFd ci ha ceduto il descrittore, aperto; File lo chiude quando esce di scope.
        let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
        // Tetto alla dimensione: un file enorme non deve finire tutto in memoria.
        crate::leggi_con_tetto(&mut file, "URI")
    }

    /// Indietro alla radice: l'app va in secondo piano, l'Activity resta viva (winit 0.30 non
    /// sopravvive alla distruzione dell'Activity).
    pub fn move_to_background(&self) {
        self.on_activity(|env, activity| {
            env.call_method(activity, jni_str!("moveTaskToBack"), jni_sig!("(Z)Z"), &[JValue::Bool(true)])?;
            Ok(())
        });
    }

    /// Esegue `f` subito, sul thread corrente (android_main), con l'Activity.
    fn with_activity<T>(&self, f: impl for<'a, 'b> FnOnce(&mut jni::Env<'a>, &JObject<'b>) -> jni::errors::Result<T>) -> jni::errors::Result<T> {
        // SAFETY: il puntatore viene da android-activity ed è quello della JavaVM del processo.
        let vm = unsafe { JavaVM::from_raw(self.app.vm_as_ptr().cast()) };
        vm.attach_current_thread(|env| -> jni::errors::Result<T> {
            let raw: jni::sys::jobject = self.app.activity_as_ptr().cast();
            // SAFETY: riferimento globale che non ci appartiene, valido finché vive `self.app`:
            // si avvolge senza prenderne possesso e non si cancella mai.
            let activity = unsafe { env.as_cast_raw::<JObject>(&raw)? };
            f(env, &activity)
        })
    }

    /// Esegue `f` più tardi sul thread principale Java, senza aspettare.
    fn on_activity<F>(&self, f: F)
    where
        F: for<'a, 'b> FnOnce(&mut jni::Env<'a>, &JObject<'b>) -> jni::errors::Result<()> + Send + 'static,
    {
        let app = self.app.clone();
        self.app.run_on_java_main_thread(Box::new(move || {
            // SAFETY: come in `with_activity`.
            let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) };
            let esito = vm.attach_current_thread(|env| -> jni::errors::Result<()> {
                let raw: jni::sys::jobject = app.activity_as_ptr().cast();
                // SAFETY: come in `with_activity`; `app` vive fino alla fine della chiusura.
                let activity = unsafe { env.as_cast_raw::<JObject>(&raw)? };
                f(env, &activity)
            });
            if let Err(err) = esito {
                let err = errore_jni(&err);
                log::error!("JNI sul thread principale: {err}");
                diagnosi(format!("JNI sul thread principale: {err}"));
            }
        }));
    }
}
