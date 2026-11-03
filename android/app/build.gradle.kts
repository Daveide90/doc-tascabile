import java.util.Properties

plugins {
    id("com.android.application")
}

// La firma di rilascio: android/firma.properties (storeFile, storePassword, keyAlias, keyPassword)
// e la chiave che nomina, tutte e due fuori da git. Senza il file la build di prova va lo stesso;
// quella di rilascio si ferma (build.sh lo controlla prima, con un messaggio chiaro).
val firma = Properties().apply {
    val file = rootProject.file("firma.properties")
    if (file.isFile) file.inputStream().use { load(it) }
}

android {
    namespace = "it.tascabile.app"
    compileSdk = 36
    // Lo stesso NDK di cargo-ndk: AGP lo usa per togliere i simboli di debug dal .so.
    ndkVersion = "28.2.13676358"

    defaultConfig {
        applicationId = "it.tascabile.app"
        minSdk = 30
        targetSdk = 35
        // versionName uguale alla versione del workspace (Cargo.toml): lo controlla app/tests/versione.rs.
        versionCode = 29
        versionName = "0.9.3"
        ndk { abiFilters += listOf("arm64-v8a") }
    }

    signingConfigs {
        create("release") {
            if (!firma.isEmpty) {
                storeFile = rootProject.file(firma.getProperty("storeFile"))
                storePassword = firma.getProperty("storePassword")
                keyAlias = firma.getProperty("keyAlias")
                keyPassword = firma.getProperty("keyPassword")
            }
            // Anche lo schema v3: se un giorno la chiave va cambiata, l'app si aggiorna senza
            // disinstallarla (audit di sicurezza del 30/10, problema 9).
            enableV2Signing = true
            enableV3Signing = true
        }
    }

    buildTypes {
        release {
            // R8: toglie il codice Java che non serve e, con le regole di proguard-rules.pro, le
            // chiamate a Log.d e Log.v; serve il file «optimize», perché quello senza ha
            // -dontoptimize e togliere le chiamate senza effetti è un'ottimizzazione. I nomi
            // chiamati dal codice nativo li tengono le regole e li controlla build.sh nel dex.
            isMinifyEnabled = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = signingConfigs.getByName("release")
        }
    }

    // I modelli del riconoscimento del testo (piano «PDF completo», P8) negli asset dell'APK:
    // l'app li legge al primo uso. Licenze e provenienza in crates/ocr/modelli/NOTICE.
    // Il dizionario italiano dell'ortografia (piano «Word completo», W4) in dizionario/: l'app lo
    // legge al primo documento Word. Licenza e provenienza in crates/mod-word/asset/dizionario.
    sourceSets {
        getByName("main") {
            assets.srcDir("../../crates/ocr/modelli")
            assets.srcDir("../../crates/mod-word/asset")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

dependencies {
    // GameActivity estende AppCompatActivity e usa WindowInsetsCompat.
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("androidx.core:core:1.13.1")
    // Deve restare 4.4.0: è la versione con cui è provato android-activity 0.6.1.
    implementation("androidx.games:games-activity:4.4.0")
}

// Da appcompat arriva kotlin-stdlib 1.8.22, che contiene già le classi di kotlin-stdlib-jdk7 e
// kotlin-stdlib-jdk8; kotlinx-coroutines 1.6.4 (da lifecycle) chiede ancora quei due alla 1.6.21:
// classi doppie, e checkDebugDuplicateClasses ferma il build. Senza il plugin Kotlin nessuno
// allinea le versioni, quindi si tolgono: dalla 1.8 in poi sono jar vuoti.
configurations.configureEach {
    exclude(group = "org.jetbrains.kotlin", module = "kotlin-stdlib-jdk7")
    exclude(group = "org.jetbrains.kotlin", module = "kotlin-stdlib-jdk8")
}
