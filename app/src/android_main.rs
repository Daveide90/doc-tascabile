//! Ingresso Android: `android_main` (chiamato da android-activity) e i metodi nativi di MainActivity.
//! I nativi stanno qui, nel cdylib, perché devono essere esportati da libtascabile.so; il lavoro
//! lo fanno le funzioni `ricevi_*` di tascabile-platform. I nomi `Java_it_tascabile_app_MainActivity_*`
//! dipendono da package e classe Java: se cambiano là, vanno cambiati anche qui.

use jni::EnvUnowned;
use jni::errors::LogErrorAndDefault;
use jni::objects::{JClass, JString};
use jni::sys::{jint, jlong};
use tascabile_platform::ImeSnapshot;
use winit::platform::android::activity::AndroidApp;

#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    let avvio = std::time::Instant::now();
    // Prima riga del registro: i suoi tempi partono da qui. Resta in testa al registro.
    tascabile_platform::diagnosi_fissa(tascabile_platform::Fissa::Avvio, format!("avvio: Tascabile v{} su Android", crate::VERSIONE));
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("tascabile")
            // winit scrive ogni TextEvent (col testo battuto) a livello warn: si tace. Così lopdf,
            // che scrive i caratteri del testo che non sa codificare, i crate del disegno dei
            // PDF, che possono scrivere i nomi dei font del documento, e il lettore dei .pptx,
            // che scrive i nomi delle parti del file e pezzi del suo XML.
            .with_filter(
                env_filter::Builder::new()
                    .parse("info,winit=error,lopdf=off,hayro=off,hayro_interpret=off,hayro_syntax=off,vello_common=off,deckcraft_pptx=off")
                    .build(),
            ),
    );
    // Un panico nel log e nella diagnosi con file e riga soltanto: il messaggio può avere del testo.
    tascabile_platform::installa_gancio_dei_panici();
    let n = tascabile_word::font::aggiungi_font_emoji(std::path::Path::new("/system/fonts"));
    log::info!("font emoji aggiunti: {n}");
    let piattaforma = tascabile_platform::Piattaforma::nuova(app.clone());
    let options = eframe::NativeOptions { android_app: Some(app), renderer: eframe::Renderer::Wgpu, ..Default::default() };
    let esito = eframe::run_native(
        "Tascabile",
        options,
        Box::new(move |cc| {
            piattaforma.imposta_contesto(&cc.egui_ctx);
            // Il tema prima del primo frame: caratteri, colori chiari, misure.
            crate::prepara(&cc.egui_ctx);
            Ok(Box::new(crate::TascabileApp::new(piattaforma, avvio)))
        }),
    );
    if let Err(err) = esito {
        // Di solito RecreationAttempt: android_main rientrato nello stesso processo.
        // Al ritorno android-activity chiude l'Activity e MainActivity.onDestroy il processo.
        log::error!("eframe: {err}");
    }
}

// Metodi nativi di MainActivity (static native: il secondo argomento è la classe). Chiamati sul
// thread principale Java: un errore o un panico si scrive nel log, mai un'eccezione.

#[unsafe(no_mangle)]
pub extern "system" fn Java_it_tascabile_app_MainActivity_nativeImeState<'caller>(
    mut env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    text: JString<'caller>,
    sel_start: jint,
    sel_end: jint,
    comp_start: jint,
    comp_end: jint,
    version: jlong,
) {
    env.with_env(|env| -> jni::errors::Result<()> {
        tascabile_platform::ricevi_stato_ime(ImeSnapshot {
            text: text.try_to_string(env)?,
            sel: (sel_start, sel_end),
            comp: (comp_start, comp_end),
            version,
        });
        Ok(())
    })
    .resolve::<LogErrorAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_it_tascabile_app_MainActivity_nativeInsets<'caller>(
    mut env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    left: jint,
    top: jint,
    right: jint,
    bottom: jint,
    ime_bottom: jint,
    ime_visible: jint,
    finale: jint,
) {
    env.with_env(|_env| -> jni::errors::Result<()> {
        tascabile_platform::ricevi_insets(left, top, right, bottom, ime_bottom, ime_visible != 0, finale != 0);
        Ok(())
    })
    .resolve::<LogErrorAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_it_tascabile_app_MainActivity_nativeNewIntent<'caller>(mut env: EnvUnowned<'caller>, _class: JClass<'caller>) {
    env.with_env(|_env| -> jni::errors::Result<()> {
        tascabile_platform::ricevi_nuovo_intent();
        Ok(())
    })
    .resolve::<LogErrorAndDefault>()
}

/// Una riga di diagnosi da Java (errori della condivisione e degli appunti, senza contenuti).
#[unsafe(no_mangle)]
pub extern "system" fn Java_it_tascabile_app_MainActivity_nativeDiagnosi<'caller>(
    mut env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    evento: JString<'caller>,
) {
    env.with_env(|env| -> jni::errors::Result<()> {
        tascabile_platform::diagnosi(evento.try_to_string(env)?);
        Ok(())
    })
    .resolve::<LogErrorAndDefault>()
}

/// Lo schermo esterno della presentazione: larghezza e altezza in pixel, 0 × 0 se non c'è più.
#[unsafe(no_mangle)]
pub extern "system" fn Java_it_tascabile_app_MainActivity_nativeSchermoEsterno<'caller>(
    mut env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    larghezza: jint,
    altezza: jint,
) {
    env.with_env(|_env| -> jni::errors::Result<()> {
        tascabile_platform::ricevi_schermo_esterno(larghezza, altezza);
        Ok(())
    })
    .resolve::<LogErrorAndDefault>()
}

/// L'esito della finestra dell'impronta per la richiesta `richiesta` (1 riuscita, 0 annullata, -2
/// telefono senza blocco, -1 errore).
#[unsafe(no_mangle)]
pub extern "system" fn Java_it_tascabile_app_MainActivity_nativeImpronta<'caller>(
    mut env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    richiesta: jint,
    codice: jint,
) {
    env.with_env(|_env| -> jni::errors::Result<()> {
        tascabile_platform::ricevi_impronta(richiesta, codice);
        Ok(())
    })
    .resolve::<LogErrorAndDefault>()
}

/// L'esito della foto della firma per la richiesta `richiesta` (1 pronta nella cache, per più foto
/// dalla galleria quante, 0 annullata, -1 errore), con quante foto erano state scelte.
#[unsafe(no_mangle)]
pub extern "system" fn Java_it_tascabile_app_MainActivity_nativeFotoFirma<'caller>(
    mut env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    richiesta: jint,
    esito: jint,
    scelte: jint,
) {
    env.with_env(|_env| -> jni::errors::Result<()> {
        tascabile_platform::ricevi_foto_firma(richiesta, esito, scelte);
        Ok(())
    })
    .resolve::<LogErrorAndDefault>()
}
