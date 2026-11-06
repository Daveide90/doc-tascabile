#!/usr/bin/env bash
# Tascabile: libtascabile.so con cargo-ndk, poi APK con Gradle. Niente emulatore: l'APK va sul telefono.
#   ./build.sh            APK di prova, firmato con la chiave di debug: DocTascabile_v<versione>_debug_<data>.apk
#   ./build.sh rilascio   APK di rilascio, con R8 e la chiave vera:     DocTascabile_v<versione>_<data>.apk (in android/uscita/)
set -euo pipefail
cd "$(dirname "$0")"                       # cartella android/

MODO="${1:-prova}"
case "$MODO" in
    prova | rilascio) ;;
    *) echo "Uso: ./build.sh [prova|rilascio]"; exit 2 ;;
esac

export ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/28.2.13676358"
export JAVA_HOME="${JAVA_HOME:-/usr/lib/jvm/java-21-openjdk-amd64}"   # non il JDK 25 di sistema
export PATH="$HOME/.cargo/bin:$JAVA_HOME/bin:$PATH"
echo "sdk.dir=$ANDROID_HOME" > local.properties
BUILD_TOOLS="$ANDROID_HOME/build-tools/36.0.0"

# 0. Il compilatore e le librerie (audit di sicurezza del 30/10, problema 12). Rust deve essere la
# versione scritta in rust-toolchain.toml («# versione:»): un `rustup update` non cambia in silenzio
# il compilatore delle build di rilascio.
RUST_ATTESA=$(sed -n 's/^# versione: *//p' ../rust-toolchain.toml)
RUST_VERA=$(cd .. && rustc --version | cut -d' ' -f2)
[ -n "$RUST_ATTESA" ] || { echo "ERRORE: manca la riga «# versione:» in rust-toolchain.toml"; exit 1; }
[ "$RUST_VERA" = "$RUST_ATTESA" ] || { echo "ERRORE: Rust è $RUST_VERA, rust-toolchain.toml vuole $RUST_ATTESA"; exit 1; }
echo "Rust: $RUST_VERA, come in rust-toolchain.toml."

# 0b. Per il rilascio, cargo audit sulle librerie del lock: nessun avviso di sicurezza nuovo, né
# vulnerabilità né «unsound» (codice che può rompere la memoria). Quelli già guardati e non
# raggiungibili stanno in avvisi-accettati.txt, ognuno col suo perché. Passa solo una risposta
# esplicita: un errore qualsiasi, o niente, ferma il rilascio (revisione del 31/10, punto 10).
# Senza rete si usa il registro scaricato l'ultima volta, se ha al più 7 giorni.
if [ "$MODO" = rilascio ]; then
    command -v cargo-audit > /dev/null || { echo "ERRORE: manca cargo-audit (cargo install cargo-audit --locked)"; exit 1; }
    nuovi_avvisi() {
        python3 -I -c '
import json, sys
try:
    accettati = {r.split()[0] for r in open(sys.argv[1], encoding="utf-8") if r.strip() and not r.startswith("#")}
    d = json.load(sys.stdin)
    trovati = {v["advisory"]["id"] for v in d["vulnerabilities"]["list"]}
    trovati |= {w["advisory"]["id"] for w in d.get("warnings", {}).get("unsound", []) if w.get("advisory")}
    nuovi = sorted(trovati - accettati)
    print(" ".join(nuovi) if nuovi else "NESSUNO")
except Exception:
    print("ILLEGGIBILE")' avvisi-accettati.txt
    }
    NUOVI=$( (cd .. && cargo audit --json 2> /dev/null) | nuovi_avvisi || true)
    if [ "$NUOVI" = ILLEGGIBILE ] || [ -z "$NUOVI" ]; then
        REGISTRO="${CARGO_HOME:-$HOME/.cargo}/advisory-db"
        GIORNI=$(( ($(date +%s) - $(git -C "$REGISTRO" log -1 --format=%ct 2> /dev/null || echo 0)) / 86400 ))
        [ "$GIORNI" -le 7 ] || { echo "ERRORE: registro degli avvisi non aggiornato e vecchio di $GIORNI giorni: serve la rete"; exit 1; }
        echo "Attenzione: registro degli avvisi non aggiornato (senza rete?): uso quello di $GIORNI giorni fa."
        NUOVI=$( (cd .. && cargo audit --json --no-fetch 2> /dev/null) | nuovi_avvisi || true)
    fi
    case "$NUOVI" in
        NESSUNO) echo "Librerie: nessun avviso di sicurezza nuovo (cargo audit, vulnerabilità e «unsound»)." ;;
        ILLEGGIBILE | "") echo "ERRORE: cargo-audit non ha risposto"; exit 1 ;;
        *) echo "ERRORE: avvisi di sicurezza nuovi nelle librerie: $NUOVI (cargo audit, poi avvisi-accettati.txt)"; exit 1 ;;
    esac
fi

# 0c. Per il rilascio, gli avvisi delle librerie nell'app (Impostazioni › Tutte le librerie Rust)
# devono corrispondere a Cargo.lock: MIT, BSD, ISC e Apache li chiedono anche nell'APK (6/11/2026).
if [ "$MODO" = rilascio ]; then
    python3 -I ../app/licenze/genera.py --controlla || { echo "ERRORE: elenco delle librerie vecchio: python3 -I app/licenze/genera.py"; exit 1; }
    echo "Licenze: l'elenco delle librerie corrisponde a Cargo.lock."
fi

# La chiave di rilascio e le sue password stanno fuori da git (vedi app/build.gradle.kts).
if [ "$MODO" = rilascio ]; then
    [ -f firma.properties ] || { echo "ERRORE: manca android/firma.properties (chiave di rilascio)"; exit 1; }
    CHIAVE=$(grep -m1 '^storeFile=' firma.properties | cut -d= -f2-)
    [ -f "$CHIAVE" ] || { echo "ERRORE: manca la chiave di rilascio android/$CHIAVE"; exit 1; }
fi

# 1. Rust in release: app/src/main/jniLibs/arm64-v8a/libtascabile.so
# rustc scrive nella libreria i percorsi dei sorgenti (per i panic), e quelli delle dipendenze
# stanno nella cartella di casa: li si riscrive in /~, così l'APK non porta i percorsi del PC.
# cargo-ndk non tocca i rustflags; la prima volta il target Android si ricompila da capo.
# Il codice C di aws-lc-sys (firme di PDFCraft) va col clang dell'NDK, senza CMake: le variabili
# AWS_LC_SYS_* per Android stanno in .cargo/config.toml, così valgono anche per cargo ndk clippy.
export CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS="--remap-path-prefix=$HOME=/~"
( cd .. && cargo ndk -t arm64-v8a -P 30 -o android/app/src/main/jniLibs build --locked --lib --release -p tascabile )

# 1b. cargo-ndk copia in jniLibs ogni .so che esce dalla build, anche le copie cdylib che rten,
# ocrs e boa dichiarano per sé: libtascabile.so le ha già dentro e nessuno le
# carica. Fuori dall'APK, che altrimenti porta 2 o 3 MB inutili.
find app/src/main/jniLibs/arm64-v8a -maxdepth 1 -name '*.so' ! -name libtascabile.so -delete

# 2. Controllo: i metodi nativi nostri e quelli di GameActivity devono essere esportati
NM="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-nm"
SO=app/src/main/jniLibs/arm64-v8a/libtascabile.so
NOSTRI=$("$NM" -D --defined-only "$SO" | grep -c "Java_it_tascabile_app_MainActivity_native" || true)
GAME=$("$NM" -D --defined-only "$SO" | grep -c "Java_com_google_androidgamesdk" || true)
# Quanti sono lo dice nomi-jni.txt: le righe native di MainActivity.
ATTESI=$(grep -c "^Lit/tascabile/app/MainActivity; .* native$" nomi-jni.txt)
[ "$NOSTRI" = "$ATTESI" ] || { echo "ERRORE: metodi nativi esportati $NOSTRI su $ATTESI"; exit 1; }
[ "$GAME" -gt 0 ] || { echo "ERRORE: mancano i metodi nativi di GameActivity"; exit 1; }

# 2b. Controllo: nessun percorso del PC nella libreria (l'APK va anche ad altri).
PERCORSI=$(strings "$SO" | grep -cF -e "$HOME" -e /home/ || true)
[ "$PERCORSI" = 0 ] || { echo "ERRORE: nella libreria restano $PERCORSI percorsi del PC"; exit 1; }
echo "Libreria: nessun percorso del PC."

# 2c. Controllo: il C e il C++ dentro la libreria (aws-lc, GameActivity) hanno lo stack canary
# (__stack_chk_fail, che llvm-nm scrive __stack_chk_fail@LIBC). MobSF lo cercava e non c'era
# (controllo del 27/10): i flag stanno in .cargo/config.toml, insieme a FORTIFY. FORTIFY si conta
# soltanto: è attivo (negli oggetti di aws-lc ci sono __memcpy_chk e simili), ma i pezzi di aws-lc
# con quelle chiamate il linker li toglie perché nessuno li usa, e nella libreria possono restarne zero.
CANARY=$("$NM" -D --undefined-only "$SO" | grep -cE ' __stack_chk_fail(@|$)' || true)
FORTIFY=$("$NM" -D --undefined-only "$SO" | grep -E ' __[a-z0-9_]+_chk(@|$)' | grep -vc '__stack_chk_fail' || true)
[ "$CANARY" -gt 0 ] || { echo "ERRORE: il C della libreria è senza stack canary"; exit 1; }
echo "Libreria: stack canary; funzioni di FORTIFY nella libreria finale: $FORTIFY."

# 3. APK. L'impacchettamento incrementale di Gradle riscrive l'APK sul posto e lascia i buchi della
# libreria vecchia (87 MB invece di 47): si parte sempre da un APK nuovo.
if [ "$MODO" = rilascio ]; then
    USCITA=app/build/outputs/apk/release/app-release.apk
    rm -f "$USCITA"
    ./gradlew --no-daemon --console=plain assembleRelease
else
    USCITA=app/build/outputs/apk/debug/app-debug.apk
    rm -f "$USCITA"
    ./gradlew --no-daemon --console=plain assembleDebug
fi

# 4. Controllo del dex: ogni nome di nomi-jni.txt (classe, nome, tipo, nativo o no) deve esserci
# ancora, perché il codice nativo lo cerca per nome; nel rilascio, nessuna chiamata a Log.d o Log.v
# (GameTextInput scriverebbe in logcat il testo battuto). Le cartelle di lavoro stanno in app/build.
LAVORO=app/build/tmp/controllo-dex
rm -rf "$LAVORO"
mkdir -p "$LAVORO"
unzip -q -o "$USCITA" 'classes*.dex' -d "$LAVORO"
: > "$LAVORO/nomi.txt"
: > "$LAVORO/codice.txt"
for DEX in "$LAVORO"/classes*.dex; do
    # Una riga per metodo o campo: classe nome tipo, più «native» per i metodi nativi.
    "$BUILD_TOOLS/dexdump" "$DEX" | awk '
        /^  Class descriptor/ { gsub(/\x27/, "", $4); classe = $4 }
        /^      name  / { gsub(/\x27/, "", $3); nome = $3 }
        /^      type  / { gsub(/\x27/, "", $3); tipo = $3 }
        /^      access/ { print classe, nome, tipo ($0 ~ /NATIVE/ ? " native" : "") }
    ' >> "$LAVORO/nomi.txt"
    "$BUILD_TOOLS/dexdump" -d "$DEX" >> "$LAVORO/codice.txt"
done
MANCANTI=$(grep -v '^#' nomi-jni.txt | grep -v '^$' | grep -Fxv -f "$LAVORO/nomi.txt" || true)
[ -z "$MANCANTI" ] || { echo "ERRORE: nel dex mancano questi nomi chiamati dal codice nativo:"; echo "$MANCANTI"; exit 1; }
ATTESI=$(grep -v '^#' nomi-jni.txt | grep -vc '^$')
echo "Dex: tutti i $ATTESI nomi chiamati dal codice nativo ci sono."
if [ "$MODO" = rilascio ]; then
    LOG=$(grep -cE 'Landroid/util/Log;\.(d|v):' "$LAVORO/codice.txt" || true)
    [ "$LOG" = 0 ] || { echo "ERRORE: nel dex restano $LOG chiamate a Log.d o Log.v"; exit 1; }
    echo "Dex: nessuna chiamata a Log.d o Log.v."
fi
# Anche nell'APK finito (dex, risorse, librerie, META-INF) nessun percorso del PC.
unzip -q -o "$USCITA" -d "$LAVORO/apk"
FILE_PC=$(grep -rlaF -e "$HOME" -e /home/ "$LAVORO/apk" || true)
[ -z "$FILE_PC" ] || { echo "ERRORE: percorsi del PC in questi file dell'APK:"; echo "$FILE_PC"; exit 1; }
echo "APK: nessun percorso del PC."

# 5. Firma: nel rilascio, il certificato dell'APK deve essere quello della chiave vera.
if [ "$MODO" = rilascio ]; then
    # La password passa a keytool per variabile d'ambiente, non sulla riga di comando.
    PASSWORD_FIRMA=$(grep -m1 '^storePassword=' firma.properties | cut -d= -f2-)
    ALIAS=$(grep -m1 '^keyAlias=' firma.properties | cut -d= -f2-)
    export PASSWORD_FIRMA
    ATTESA=$(keytool -list -v -keystore "$CHIAVE" -alias "$ALIAS" -storepass:env PASSWORD_FIRMA \
        | grep -m1 'SHA256:' | awk '{print $2}' | tr -d ':' | tr 'A-F' 'a-f')
    unset PASSWORD_FIRMA
    TROVATA=$("$BUILD_TOOLS/apksigner" verify --print-certs "$USCITA" | grep -m1 'certificate SHA-256 digest' | awk '{print $NF}')
    [ -n "$ATTESA" ] && [ "$TROVATA" = "$ATTESA" ] || { echo "ERRORE: l'APK non è firmato con la chiave di rilascio"; exit 1; }
    echo "Firma: chiave di rilascio (SHA-256 $TROVATA)."
fi

# 6. L'APK in android/uscita/ (fuori da git). Le copie personali, se servono, le fa
#    privato/dopo-build.sh, anch'esso fuori da git, con il percorso dell'APK e il modo.
VERSIONE=$(grep -m1 '^version' ../Cargo.toml | cut -d'"' -f2)
DEST="$(pwd)/uscita"
mkdir -p "$DEST"
if [ "$MODO" = rilascio ]; then
    APK="$DEST/DocTascabile_v${VERSIONE}_$(date +%Y%m%d).apk"
else
    APK="$DEST/DocTascabile_v${VERSIONE}_debug_$(date +%Y%m%d).apk"
fi
cp "$USCITA" "$APK"
if [ -x ../privato/dopo-build.sh ]; then ../privato/dopo-build.sh "$APK" "$MODO"; fi
ls -l --block-size=K "$SO" "$APK"
echo "APK pronto: $APK"
