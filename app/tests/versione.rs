//! La versione dell'APK (Gradle) e quella del workspace (Cargo) devono essere la stessa, e la build
//! di rilascio deve avere R8 con le regole che tengono i nomi chiamati da Rust e dal C++ di
//! GameActivity e che tolgono `Log.d` e `Log.v` (GameTextInput scrive in logcat il testo battuto).
//! Il controllo vero dei nomi sta nel dex, in `android/build.sh rilascio`; qui si prende prima,
//! sul PC, una regola tolta per sbaglio. La libreria Android si compila con le versioni del lock.

use std::fs;
use std::path::PathBuf;

fn android(percorso: &str) -> String {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../android").join(percorso);
    fs::read_to_string(&file).unwrap_or_else(|e| panic!("{} non si legge: {e}", file.display()))
}

/// Il valore dopo `chiave =` nella prima riga che comincia così, senza virgolette.
fn valore(testo: &str, chiave: &str) -> Option<String> {
    testo.lines().map(str::trim).find_map(|riga| {
        let resto = riga.strip_prefix(chiave)?.trim_start().strip_prefix('=')?;
        Some(resto.trim().trim_matches('"').to_string())
    })
}

/// Le righe che contano di un file di regole: senza commenti e senza spazi doppi.
fn regole(testo: &str) -> String {
    testo
        .lines()
        .map(|r| r.split('#').next().unwrap_or("").split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|r| !r.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn version_name_di_gradle_e_quella_di_cargo() {
    let gradle = android("app/build.gradle.kts");
    assert_eq!(valore(&gradle, "versionName").as_deref(), Some(tascabile::VERSIONE));
    assert_eq!(tascabile::VERSIONE, "0.9.5");
}

#[test]
fn version_code_della_0_7_0_e_8() {
    // La 0.0.1 aveva 1, la 0.1.0 2, la 0.2.0 3, la 0.3.0 4, la 0.4.0 5, la 0.5.0 6, la 0.6.0 7:
    // Android installa sopra solo con un numero più alto.
    let gradle = android("app/build.gradle.kts");
    assert_eq!(valore(&gradle, "versionCode").as_deref(), Some("31"));
}

#[test]
fn la_build_di_rilascio_usa_r8_con_le_regole_e_la_firma_vera() {
    let gradle = android("app/build.gradle.kts");
    assert_eq!(valore(&gradle, "isMinifyEnabled").as_deref(), Some("true"));
    assert!(gradle.contains("proguard-android-optimize.txt"), "senza ottimizzazione -assumenosideeffects non toglie niente");
    assert!(gradle.contains("\"proguard-rules.pro\""));
    assert!(gradle.contains("firma.properties"));
    assert!(gradle.contains("signingConfigs.getByName(\"release\")"));
}

#[test]
fn le_regole_tengono_main_activity_e_game_activity_e_tolgono_log_d_e_log_v() {
    let r = regole(&android("app/proguard-rules.pro"));
    for attesa in [
        "-keep class it.tascabile.app.MainActivity { *; }",
        "-keep class com.google.androidgamesdk.** { *; }",
        "-assumenosideeffects class android.util.Log {",
        "public static int d(...);",
        "public static int v(...);",
    ] {
        assert!(r.contains(attesa), "manca nelle regole: {attesa}\n{r}");
    }
}

#[test]
fn build_sh_usa_il_lock() {
    // Con --locked la libreria si compila con le versioni del lock (zip 3.0.0 compresa): se il
    // lock non è aggiornato la build si ferma, invece di sceglierne altre da sola.
    let script = android("build.sh");
    let righe: Vec<&str> = script.lines().map(str::trim).filter(|r| !r.starts_with('#') && r.contains("cargo ndk")).collect();
    assert_eq!(righe.len(), 1, "{righe:?}");
    assert!(righe[0].split_whitespace().any(|p| p == "--locked"), "cargo ndk senza --locked: {}", righe[0]);
}

#[test]
fn il_nome_sotto_l_icona_e_doc_tascabile() {
    // Scelta dell'autore, 24/10: l'app si chiama «Doc Tascabile»; il pacchetto resta it.tascabile.app.
    let stringhe = android("app/src/main/res/values/strings.xml");
    assert!(stringhe.contains(r#"<string name="app_name">Doc Tascabile</string>"#), "{stringhe}");
    let manifest = android("app/src/main/AndroidManifest.xml");
    assert!(manifest.contains(r#"android:label="@string/app_name""#), "il manifest non usa app_name");
    let gradle = android("app/build.gradle.kts");
    assert_eq!(valore(&gradle, "applicationId").as_deref(), Some("it.tascabile.app"));
}

#[test]
fn il_tema_android_e_chiaro_con_le_icone_scure_nelle_barre() {
    // L'app disegna la carta anche sotto la barra di stato e quella di navigazione: le loro
    // icone devono essere scure, anche col telefono in modalità scura.
    let temi = android("app/src/main/res/values/themes.xml");
    assert!(temi.contains(r#"parent="Theme.AppCompat.Light.NoActionBar""#), "{temi}");
    for voce in [
        r#"<item name="android:windowLightStatusBar">true</item>"#,
        r#"<item name="android:windowLightNavigationBar">true</item>"#,
        r#"<item name="android:windowBackground">@color/carta</item>"#,
        r#"<item name="android:statusBarColor">@color/carta</item>"#,
        r#"<item name="android:navigationBarColor">@color/carta</item>"#,
    ] {
        assert!(temi.contains(voce), "manca {voce}:\n{temi}");
    }
    let colori = android("app/src/main/res/values/colori.xml");
    assert!(colori.contains(r##"<color name="carta">#F6F3EE</color>"##), "{colori}");
}
