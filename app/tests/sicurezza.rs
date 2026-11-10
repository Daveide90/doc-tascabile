//! Le difese del manifest, della firma e della build che non si vedono nelle prove dell'app
//! (audit di sicurezza del 30/10, un documento interno di lavoro): si leggono i file, così
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

/// Il controllo degli archivi (`tascabile_files::archivio`) legge con la stessa libreria zip dei
/// motori, così vede gli stessi byte che vedranno loro (correzione della 0.8.5). Una seconda zip
/// nel lock, per esempio un motore aggiornato che ne porta un'altra, riaprirebbe la differenza.
/// La versione è quella che il controllo chiede, almeno la 9.0.5 (CVE-2026-115905, intestazioni
/// che non coincidono; CVE-2026-115906 e 115909 e gli avvisi RUSTSEC-2026-0367, 0368, 0369 e
/// 0429 sono corretti dalla 9.0.0).
#[test]
fn il_controllo_e_i_motori_leggono_con_la_stessa_zip() {
    let lock = del_progetto("Cargo.lock");
    let versioni: Vec<&str> = lock
        .split("[[package]]")
        .filter(|p| p.lines().any(|r| r.trim() == r#"name = "zip""#))
        .filter_map(|p| p.lines().find_map(|r| r.trim().strip_prefix("version = ")))
        .map(|v| v.trim_matches('"'))
        .collect();
    assert_eq!(versioni.len(), 1, "nel lock ci sono più zip: {versioni:?}");
    let numeri: Vec<u32> = versioni[0].split('.').map(|n| n.parse().unwrap()).collect();
    assert!(numeri >= vec![9, 0, 5], "zip {} ha le intestazioni che non coincidono (CVE-2026-115905)", versioni[0]);
    let manifest = del_progetto("Cargo.toml");
    let riga = manifest.lines().find(|r| r.starts_with("zip = ")).expect("manca zip fra le dipendenze del workspace");
    assert!(riga.contains(&format!(r#""={}""#, versioni[0])), "il workspace non fissa la zip del lock: {riga}");
}

/// Segnalazione dell'autore del 7/11: «se apro un pdf da WhatsApp o Telegram l'app si blocca su
/// schermo bianco». Nel registro del telefono, l'«Apri con» di Telegram passa dal selettore di
/// Android chiedendo un risultato (flg=0x3000001, FORWARD_RESULT): Android allora ignora
/// singleTask e crea una seconda MainActivity nel task di Telegram, nello stesso processo; la
/// seconda non disegna mai (eframe parte una volta sola per processo) e il processo si blocca.
/// L'«Apri con» arriva a un'attività di passaggio senza schermo, che gira il file a MainActivity
/// con NEW_TASK e senza risultato, e si chiude: così singleTask vale e la finestra resta una.
#[test]
fn l_apri_con_passa_da_un_attivita_di_passaggio() {
    let manifest = android("app/src/main/AndroidManifest.xml");
    let blocco = |nome: &str| -> String {
        let i = manifest.find(&format!("android:name=\".{nome}\"")).unwrap_or_else(|| panic!("manca {nome}"));
        let inizio = manifest[..i].rfind("<activity").unwrap();
        let fine = manifest[i..].find("</activity>").unwrap() + i;
        manifest[inizio..fine].to_string()
    };
    let principale = blocco("MainActivity");
    assert!(!principale.contains("android.intent.action.VIEW"), "MainActivity riceve ancora l'«Apri con» direttamente");
    assert!(principale.contains(r#"android:launchMode="singleTask""#));
    let passaggio = blocco("ApriConActivity");
    // Come MainActivity alle 16:52 del 7/11, che a freddo partiva: finestra unica in un task suo e
    // un tema visibile. Invisibile (NoDisplay) e «multipla» nel task di Telegram, il telefono
    // (nubia) la creava senza mai avviare il processo: ad app chiusa l'«Apri con» non partiva.
    assert!(!passaggio.contains("Theme.NoDisplay") && !passaggio.contains("Translucent"), "un'attività invisibile non avvia l'app chiusa");
    for attributo in [
        r#"android:exported="true""#,
        r#"android:launchMode="singleTask""#,
        r#"android:taskAffinity="it.tascabile.app.apricon""#,
        r#"android:noHistory="true""#,
        r#"android:excludeFromRecents="true""#,
        r#"android:autoRemoveFromRecents="true""#,
        "android.intent.action.VIEW",
        r#"android:mimeType="application/pdf""#,
    ] {
        assert!(passaggio.contains(attributo), "ApriConActivity senza {attributo}");
    }
    let java = android("app/src/main/java/it/tascabile/app/ApriConActivity.java");
    assert!(java.contains("Intent.FLAG_ACTIVITY_NEW_TASK"), "MainActivity va aperta in un task suo");
    assert!(java.contains("setFlags("), "i flag dell'altra app (FORWARD_RESULT) vanno tolti, non aggiunti");
    assert!(java.contains("Intent.FLAG_GRANT_READ_URI_PERMISSION"), "il permesso di lettura va passato a MainActivity");
    assert!(java.contains("MainActivity.class") && java.contains("finish()"));
    assert!(!java.contains("startActivityForResult"));
    // Revisione del 10/11: se il passaggio non riesce, chi ha toccato «Apri con» lo deve sapere
    // (prima restava solo una riga nel registro di sistema). Nel messaggio niente URI.
    assert!(java.contains("Toast.makeText("), "un passaggio non riuscito va detto a chi usa l'app");
    assert!(!java.contains("getMessage()"), "il messaggio dell'errore può avere l'URI");
}

/// Revisione del 10/11: l'app non si offriva per i file .rtf (Android li dichiara `text/rtf`), per
/// i modelli e i documenti con macro di Word e di Excel, e per i documenti che un'altra app manda
/// senza dirne il tipo (`application/octet-stream`). Per questi ultimi conta l'estensione nel
/// percorso dell'URI: l'app non si offre mai per un file qualunque.
#[test]
fn l_apri_con_si_offre_anche_per_rtf_modelli_e_file_senza_tipo() {
    let manifest = android("app/src/main/AndroidManifest.xml");
    for tipo in [
        "text/rtf",
        "application/x-rtf",
        "application/vnd.ms-word.document.macroenabled.12",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.template",
        "application/vnd.ms-word.template.macroenabled.12",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.template",
        "application/vnd.ms-excel.template.macroenabled.12",
        "application/vnd.ms-excel.sheet.binary.macroenabled.12",
    ] {
        assert!(manifest.contains(&format!(r#"android:mimeType="{tipo}""#)), "manca {tipo}");
    }
    // Il filtro dei file senza tipo è a parte: in un filtro solo i percorsi varrebbero per tutti i
    // tipi, e un PDF con un URI senza nome non si aprirebbe più.
    let i = manifest.find(r#"android:mimeType="application/octet-stream""#).expect("manca il filtro dei file senza tipo");
    let inizio = manifest[..i].rfind("<intent-filter").unwrap();
    let fine = manifest[i..].find("</intent-filter>").unwrap() + i;
    let filtro = &manifest[inizio..fine];
    assert!(filtro.contains(r#"android:scheme="content""#) && filtro.contains(r#"android:host="*""#), "{filtro}");
    assert!(filtro.contains("android.intent.action.VIEW") && filtro.contains("android.intent.category.DEFAULT"));
    assert_eq!(filtro.matches("android:mimeType=").count(), 1, "il filtro per estensione vale solo per i file senza tipo");
    // Le estensioni che l'app apre (non il testo semplice: comparirebbe per ogni .txt). Il motivo
    // di Android si ferma al primo punto e non torna indietro: serve una riga per ogni numero di
    // punti nel percorso, da uno a sei (le cartelle di Telegram ne hanno già due), e una per le
    // maiuscole.
    for ext in ["docx", "docm", "dotx", "dotm", "doc", "odt", "rtf", "pdf", "pptx", "ppsx", "potx", "xlsx", "xlsm", "xls", "xlsb", "ods", "csv"] {
        for e in [ext.to_string(), ext.to_uppercase()] {
            for punti in 0..6 {
                let motivo = format!(r#"android:pathPattern=".*{}\\.{e}""#, r"\\..*".repeat(punti));
                assert!(filtro.contains(&motivo), "manca {motivo}");
            }
        }
    }
    assert_eq!(filtro.matches("android:pathPattern=").count(), 17 * 2 * 6);
    // Mai tutti i file.
    assert!(!manifest.contains(r#"android:mimeType="*/*""#));
    let principale = &manifest[..manifest.find(".ApriConActivity").unwrap()];
    assert!(!principale.contains("octet-stream"), "il filtro va sull'attività di passaggio, non su MainActivity");
}
