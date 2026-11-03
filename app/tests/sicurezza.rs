//! Le difese del manifest, della firma e della build che non si vedono nelle prove dell'app
//! (audit di sicurezza del 30/10, `docs/sicurezza/audit-2026-10-30.md`): si leggono i file, così
//! una riga tolta per sbaglio si prende sul PC prima del telefono.

use std::fs;
use std::path::PathBuf;

fn android(percorso: &str) -> String {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../android").join(percorso);
    fs::read_to_string(&file).unwrap_or_else(|e| panic!("{} non si legge: {e}", file.display()))
}

fn del_progetto(percorso: &str) -> String {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join(percorso);
    fs::read_to_string(&file).unwrap_or_else(|e| panic!("{} non si legge: {e}", file.display()))
}

/// Il testo di un elemento XML che comincia con `<nome` fino alla sua `>`, spazi uniti.
fn elemento(xml: &str, nome: &str) -> String {
    let i = xml.find(&format!("<{nome}")).unwrap_or_else(|| panic!("manca <{nome}"));
    let fine = xml[i..].find('>').unwrap() + i;
    xml[i..=fine].split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Problema 2: niente backup su Google Drive e niente trasferimento da telefono a telefono
/// (D2D), che su alcuni telefoni `allowBackup="false"` non ferma: le regole escludono tutte le
/// cartelle dell'app in tutti e due.
#[test]
fn i_dati_dell_app_non_escono_con_backup_o_trasferimento() {
    let manifest = android("app/src/main/AndroidManifest.xml");
    let app = elemento(&manifest, "application");
    assert!(app.contains(r#"android:allowBackup="false""#), "{app}");
    assert!(app.contains(r#"android:dataExtractionRules="@xml/regole_dati""#), "{app}");
    let regole = android("app/src/main/res/xml/regole_dati.xml");
    for parte in ["cloud-backup", "device-transfer"] {
        let inizio = regole.find(&format!("<{parte}>")).unwrap_or_else(|| panic!("manca <{parte}>"));
        let fine = regole.find(&format!("</{parte}>")).unwrap_or_else(|| panic!("manca </{parte}>"));
        let dentro = &regole[inizio..fine];
        assert!(!dentro.contains("<include"), "{parte} include qualcosa");
        for dominio in ["root", "file", "database", "sharedpref", "external", "device_root", "device_file", "device_database", "device_sharedpref"] {
            assert!(dentro.contains(&format!(r#"<exclude domain="{dominio}" path="."/>"#)), "{parte} non esclude {dominio}");
        }
    }
}

/// Revisione del 31/10, punto 7: il ponte di rete in Java ammette solo i server del timestamp, gli
/// stessi del Rust; la configurazione della rete vieta il traffico in chiaro e, da Android 16 (le
/// risorse -v36), vuole la Certificate Transparency.
#[test]
fn la_rete_va_solo_ai_server_del_timestamp() {
    let java = android("app/src/main/java/it/tascabile/app/MainActivity.java");
    let tra = |testo: &str, da: &str| -> String {
        let i = testo.find(da).unwrap_or_else(|| panic!("manca {da}"));
        testo[i..i + testo[i..].find("];").unwrap()].to_string()
    };
    let server = del_progetto("crates/prova-data/src/server.rs");
    for elenco in [tra(&server, "pub const AGGREGATORI"), tra(&server, "pub const ESPLORATORI")] {
        for pezzo in elenco.split("\"https://").skip(1) {
            let nome = pezzo.split(['/', '"']).next().unwrap();
            assert!(java.contains(&format!("\"{nome}\"")), "{nome} manca fra i server ammessi in MainActivity");
        }
    }
    let fidati = tra(&del_progetto("crates/prova-data/src/formato.rs"), "const SERVER_FIDATI");
    for pezzo in fidati.split('"').skip(1).step_by(2) {
        assert!(java.contains(&format!("\"{pezzo}\"")), "{pezzo} manca fra i calendari ammessi in MainActivity");
    }
    let app = elemento(&android("app/src/main/AndroidManifest.xml"), "application");
    assert!(app.contains(r#"android:networkSecurityConfig="@xml/rete""#), "{app}");
    for (file, ct) in [("app/src/main/res/xml/rete.xml", false), ("app/src/main/res/xml-v36/rete.xml", true)] {
        let x = android(file);
        assert!(x.contains(r#"cleartextTrafficPermitted="false""#) && x.contains(r#"<certificates src="system" />"#), "{file}");
        assert_eq!(x.contains(r#"<certificateTransparency enabled="true" />"#), ct, "{file}");
    }
}

/// Problema 11: contro il dirottamento del task (StrandHogg) Google indica come difesa Android 11
/// come minimo (minSdk 30, «version 2 of the attack can only be prevented by this SDK version
/// patch»): l'app non scende sotto, e così non serve cambiare l'affinità dell'Activity.
#[test]
fn il_minimo_e_android_11_che_chiude_il_dirottamento_del_task() {
    let gradle = android("app/build.gradle.kts");
    let minimo = gradle.lines().map(str::trim).find_map(|r| r.strip_prefix("minSdk =")).expect("manca minSdk").trim();
    assert!(minimo.parse::<u32>().is_ok_and(|v| v >= 30), "minSdk {minimo}");
}

/// Problema 9: la firma di rilascio anche con lo schema v3, che permette di passare a una chiave
/// nuova senza disinstallare.
#[test]
fn la_firma_di_rilascio_ha_anche_lo_schema_v3() {
    let gradle = android("app/build.gradle.kts");
    let righe: Vec<&str> = gradle.lines().map(str::trim).collect();
    assert!(righe.contains(&"enableV2Signing = true"), "manca v2");
    assert!(righe.contains(&"enableV3Signing = true"), "manca v3");
}

/// Problema 12: il wrapper di Gradle controlla l'impronta dello zip che scarica.
#[test]
fn il_wrapper_di_gradle_controlla_lo_zip() {
    let wrapper = android("gradle/wrapper/gradle-wrapper.properties");
    let impronta = wrapper.lines().find_map(|r| r.strip_prefix("distributionSha256Sum=")).expect("manca distributionSha256Sum");
    assert!(impronta.len() == 64 && impronta.bytes().all(|c| c.is_ascii_hexdigit()), "{impronta}");
    // La 8.14.6 corregge tre avvisi alti della 8.14.5 (revisione del 31/10).
    assert!(wrapper.contains("gradle-8.14.6-bin.zip"), "l'impronta è di gradle-8.14.6-bin.zip");
}

/// Problema 12: la build di rilascio si ferma se Rust non è la versione scritta in
/// rust-toolchain.toml, o se cargo-audit trova un avviso di sicurezza che non è fra quelli già
/// guardati e non raggiungibili (`android/avvisi-accettati.txt`, ognuno col suo perché).
#[test]
fn la_build_controlla_rust_e_gli_avvisi_di_sicurezza() {
    let toolchain = del_progetto("rust-toolchain.toml");
    let versione =
        toolchain.lines().find_map(|r| r.trim().strip_prefix("# versione:")).expect("manca la riga «# versione:» in rust-toolchain.toml").trim();
    assert_eq!(versione.split('.').count(), 3, "{versione}");
    let build = android("build.sh");
    assert!(build.contains("rust-toolchain.toml") && build.contains("rustc --version"), "build.sh non controlla la versione di Rust");
    assert!(build.contains("cargo audit") && build.contains("avvisi-accettati.txt"), "build.sh non lancia cargo-audit");
    let accettati = android("avvisi-accettati.txt");
    let righe: Vec<&str> = accettati.lines().filter(|r| !r.trim().is_empty() && !r.starts_with('#')).collect();
    assert!(!righe.is_empty());
    for r in &righe {
        let (id, perche) = r.split_once(' ').unwrap_or((r, ""));
        assert!(id.starts_with("RUSTSEC-") && perche.trim().len() > 20, "ogni avviso accettato vuole il suo perché: {r}");
    }
}
