# Changelog

**[English](#english)** · **[Italiano](#italiano)**

## English

### 0.9.3 · 3 November 2026

- **App lock and private mode.** Turning on the app lock also turns on private mode, so screenshots, recordings and the preview in the recent apps are black. Private mode can still be turned off on its own.
- **Recovery copies of protected PDFs.** The recovery copy of a PDF opened with a password stays encrypted like the file; a new automated test checks it.
- **Security policy.** New `SECURITY.md`: private reporting, response times, how the app is checked and its known limits.
- **Build.** `android/build.sh` writes the APK to `android/uscita/` and no longer copies it to personal folders.
- **Code.** Comments and tests no longer contain personal names; this changelog is new.

### 0.9.2 · 3 November 2026

- **One zip library, version 9.2.3.** The archive check and the readers of the three engines now read with the same zip 9.2.3, which fixes CVE-2026-115905, CVE-2026-115906, CVE-2026-115909 and four RustSec advisories. The engine crates that used zip 3 are patched copies in `vendor/`, each with its notes.
- **"Cloaked" archives are refused.** Before counting bytes, the check makes sure that the two copies of each entry in an archive say the same thing; an archive that reads in two different ways does not reach the engines.
- **Nothing changes for real documents.** 885 real Office files open as before.

### 0.9.1 · 3 November 2026

- **Security.** Ten of the eleven points of a general security review fixed.
- **Correctness.** Twenty defects fixed: scan correction no longer erases nearby text, form lines or table rules; spreadsheet fixes; "All files" and search on a background thread; "Done" always on screen; and more.
- **Scan page format and Home.** In "Fix the edges" the page format of a scan can be chosen (Automatic at first). In the Home folders open, search works inside a folder, files the app cannot open are hidden, and a finger laid down to scroll no longer highlights an item.
- **Wider text box.** While editing a PDF paragraph, a handle on the right edge widens or narrows it up to the page edge.

### 0.9.0 · 3 November 2026

- **Spreadsheets, read only.** Excel (.xlsx, .xlsm, .xltx, .xltm, .xls, .xlsb), LibreOffice (.ods), .csv and .tsv, from folders, recents and "Open with". Scroll, pinch to zoom, frozen rows and columns, sheet tabs.
- **Cells as in Excel.** Saved values without recalculation, Carlito font, colours, borders, alignment, merged cells, text overflowing into empty cells, "#" for numbers that do not fit.
- **Italian dates and numbers.** Day/month/year, decimal comma, Italian TRUE/FALSE and error names.
- **Tap a cell** to see its address, full text and formula.

### 0.8.9 · 3 November 2026

- **Scanning.** A light sheet on a dark background is now found from its brightness; each side has a middle point to straighten bent pages and lens distortion; new Eraser tool for stains.
- **Scan correction.** Whole words are taken even when the OCR box is off; new letters are blurred like the scan.
- **Order.** PDFs come first in the Home chips, in "New" and in the guide.

### 0.8.8 · 3 November 2026

- **Scan correction in the scan's own typeface.** Only the changed words are redrawn into the image, in the closest of 16 fonts, with the real ink colour; the old text leaves the image and the file.
- **Edit on the page.** PDF paragraphs and texts are edited where they are, with a cursor that follows the finger.
- **WhatsApp files.** Shortcuts to received and sent documents; a file from "Open with" that is already in a phone folder opens from there, so Save writes to the real file.

### 0.8.7 · 3 November 2026

- **Scanned text can be corrected.** In a scanned and recognised PDF, a line can be rewritten; the old text is removed from the image too, not just covered.
- **Engine fix.** Replacing or removing an image no longer leaves the old image inside the saved file.

### 0.8.6 · 3 November 2026

- **"Security and privacy" in Settings.** App lock with fingerprint or PIN (off by default); fingerprint or PIN before placing the saved signature (on by default); these protections turn off only with the fingerprint; private mode against screenshots and recordings.
- **Network only with consent.** The network stays closed and opens only for a timestamp you have confirmed; it can be turned off completely.
- **Clearer messages.** A warning when "Open with" refuses a file; recents are no longer removed by mistake after an update.

### 0.8.5 · 31 October 2026

- **Hostile documents.** The archive check counts with the same zip reader as the engines; deeply nested Word files no longer crash the app; a crafted XFA form no longer keeps the CPU busy (CVE-2026-92987).
- **Saved signature.** Asks before replacing an existing saved signature.
- **Clipboard and network.** Copied text leaves the clipboard after two minutes; paste takes only text; the network bridge talks only to the timestamp servers, with time and request limits.
- **Build and texts.** Release builds stop if `cargo audit` does not answer; Gradle 8.14.6; temporary files and half-written copies are removed at start.

### 0.8.4 · 30 October 2026

- **Hostile presentations** go through the same size checks as Word files; a fake embedded font can no longer change other presentations.
- **App data stay on the phone.** No cloud backup and no phone-to-phone transfer.
- **"Open with"** accepts only files passed by other apps with permission; shared copies are removed after an hour.
- **Build.** Signature schemes v2 and v3, Gradle download checked by hash, release stops with a different Rust version or a new security advisory.
- **How it is made.** A line in the guide and in Settings on the use of AI in development.

### 0.8.3 · 29 October 2026

- **Scan from the gallery,** up to 30 photos at once, each through "Fix the edges".
- **Narrow phones.** The scan screen is redone so every button fits at 360 dp and in landscape; screenshots of the whole app are now also taken at 372 dp.
- **Photo requests.** Each request to Android has its own number and folder, so a photo can no longer land in the wrong place.

### 0.8.2 · 28 October 2026

- **Names.** "Timestamp con blockchain", "Controlla il timestamp" and "Copia definitiva", with a sheet that explains it.
- **Finger signature.** Fountain pen, pen or marker, black or blue; the stroke follows the speed of the finger.
- **Scanning.** "Fix the edges" with a magnifier; Document, Greys, Black and white or Original look; pages under 1 MB, always rebuilt from the photo.

### 0.8.1 · 28 October 2026

- **Timestamp.** A free public OpenTimestamps proof that a file existed at a given moment, saved next to the file (`.ots`) and checked later from the same menu. Only a salted hash of the file goes online, never the document.

### 0.8.0 · 28 October 2026

- **Beta.** Word, PDF and slides in one app. Fixes for Italian Word styles, scans, replaced photos, links on text, text recognition and export to Word of scans; slides reviewed; a quick guide in Settings.

## Italiano

### 0.9.3 · 3 novembre 2026

- **Blocco dell'app e modalità riservata.** Accendendo il blocco dell'app si accende anche la modalità riservata: screenshot, registrazioni e anteprima fra le app recenti sono neri. La modalità riservata si può ancora spegnere a parte.
- **Copie di recupero dei PDF protetti.** La copia di recupero di un PDF aperto con la password resta cifrata come il file; lo controlla una prova automatica nuova.
- **Regole di sicurezza.** Nuovo `SECURITY.md`: segnalazione privata, tempi di risposta, come si controlla l'app e i suoi limiti noti.
- **Build.** `android/build.sh` scrive l'APK in `android/uscita/` e non lo copia più in cartelle personali.
- **Codice.** Nei commenti e nelle prove non ci sono più nomi di persone; questo registro delle modifiche è nuovo.

### 0.9.2 · 3 novembre 2026

- **Una sola libreria zip, la 9.2.3.** Il controllo degli archivi e i lettori dei tre motori leggono con la stessa zip 9.2.3, che chiude CVE-2026-115905, CVE-2026-115906, CVE-2026-115909 e quattro avvisi RustSec. I crate dei motori che usavano zip 3 sono copie corrette in `vendor/`, ognuna con le sue note.
- **Gli archivi «mascherati» si rifiutano.** Prima di contare i byte, il controllo verifica che le due copie di ogni voce di un archivio dicano la stessa cosa; un archivio che si legge in due modi diversi non arriva ai motori.
- **Per i documenti veri non cambia niente.** 885 file di Office veri si aprono come prima.

### 0.9.1 · 3 novembre 2026

- **Sicurezza.** Corretti dieci degli undici punti di una revisione generale di sicurezza.
- **Correttezza.** Corretti venti difetti: la correzione delle scansioni non cancella più il testo vicino, le linee dei moduli e i fili delle tabelle; correzioni ai fogli di calcolo; «Tutti i file» e la ricerca su un thread a parte; «Fatto» sempre nello schermo; e altro.
- **Formato delle scansioni e Home.** In «Sistema i bordi» si sceglie il formato della pagina (all'inizio Automatico). Nella Home le cartelle si aprono, si cerca dentro una cartella, i file che l'app non apre non si vedono, e il dito appoggiato per scorrere non accende più la voce.
- **Riquadro più largo.** Mentre si scrive in un paragrafo del PDF, una presa sul bordo destro lo allarga o lo stringe fino al bordo della pagina.

### 0.9.0 · 3 novembre 2026

- **Fogli di calcolo in sola lettura.** Excel (.xlsx, .xlsm, .xltx, .xltm, .xls, .xlsb), LibreOffice (.ods), .csv e .tsv, dalle cartelle, dai recenti e con «Apri con». Si scorre, si ingrandisce col pizzico, righe e colonne bloccate, schede dei fogli.
- **Celle come in Excel.** Valori salvati senza ricalcolo, carattere Carlito, colori, bordi, allineamento, celle unite, testo che sborda nelle celle vuote, «#» per i numeri che non stanno.
- **Date e numeri all'italiana.** Giorno/mese/anno, virgola decimale, VERO e FALSO e gli errori in italiano.
- **Un tocco su una cella** mostra dove sta, il testo intero e la formula.

### 0.8.9 · 3 novembre 2026

- **Scansione.** Un foglio chiaro su un fondo scuro ora si trova dalla sua luce; ogni lato ha un punto in mezzo per stendere i fogli piegati e le distorsioni della lente; nuovo strumento Gomma per le macchie.
- **Correzione delle scansioni.** Le parole si prendono intere anche quando il riquadro dell'OCR è spostato; le lettere nuove sono sfocate come la scansione.
- **Ordine.** I PDF vengono prima nei chip della Home, in «Nuovo» e nella Guida.

### 0.8.8 · 3 novembre 2026

- **Le scansioni si correggono nel loro carattere.** Solo le parole cambiate si ridisegnano nella foto, nel più simile di 16 caratteri, col colore dell'inchiostro vero; la scritta di prima esce dalla foto e dal file.
- **Si scrive sulla pagina.** Paragrafi e testi del PDF si modificano dove sono, con un cursore che segue il dito.
- **I file di WhatsApp.** Scorciatoie ai documenti ricevuti e mandati; un file arrivato con «Apri con» che sta già in una cartella del telefono si apre da lì, così Salva scrive nel file vero.

### 0.8.7 · 3 novembre 2026

- **Il testo delle scansioni si corregge.** In un PDF scansionato e riconosciuto una riga si riscrive; la scritta di prima si toglie anche dalla foto, non solo si copre.
- **Correzione nel motore.** Sostituire o togliere un'immagine non lascia più l'immagine di prima nel file salvato.

### 0.8.6 · 3 novembre 2026

- **«Sicurezza e privacy» nelle Impostazioni.** Blocco dell'app con impronta o PIN (spento di base); impronta o PIN prima di mettere la firma salvata (acceso di base); queste protezioni si spengono solo con l'impronta; modalità riservata contro screenshot e registrazioni.
- **Rete solo col consenso.** La rete resta chiusa e si apre solo per un timestamp confermato; si può spegnere del tutto.
- **Messaggi più chiari.** Un avviso quando «Apri con» rifiuta un file; i recenti non si tolgono più per sbaglio dopo un aggiornamento.

### 0.8.5 · 31 ottobre 2026

- **Documenti ostili.** Il controllo degli archivi conta con lo stesso lettore zip dei motori; i Word annidati non fanno più chiudere l'app; un modulo XFA costruito apposta non tiene più occupata la CPU (CVE-2026-92987).
- **Firma salvata.** Prima di sostituire una firma salvata si chiede.
- **Appunti e rete.** Il testo copiato lascia gli appunti dopo due minuti; Incolla prende solo testo; il ponte di rete parla solo coi server del timestamp, con limiti di tempo e di richieste.
- **Build e testi.** Il rilascio si ferma se `cargo audit` non risponde; Gradle 8.14.6; all'avvio se ne vanno i temporanei e le copie a metà.

### 0.8.4 · 30 ottobre 2026

- **Presentazioni ostili** passano dagli stessi controlli di misura dei Word; un font incorporato falso non cambia più le altre presentazioni.
- **I dati dell'app restano nel telefono.** Niente backup nel cloud e niente trasferimento da telefono a telefono.
- **«Apri con»** accetta solo i file passati da altre app col permesso; le copie condivise se ne vanno dopo un'ora.
- **Build.** Firma con gli schemi v2 e v3, download di Gradle controllato con l'impronta, rilascio che si ferma con un'altra versione di Rust o un avviso di sicurezza nuovo.
- **Come è fatta.** Una riga nella Guida e nelle Impostazioni sull'uso dell'IA nello sviluppo.

### 0.8.3 · 29 ottobre 2026

- **Scansione dalla galleria,** fino a 30 foto insieme, ognuna da «Sistema i bordi».
- **Telefoni stretti.** La scansione è rifatta perché ogni pulsante stia a 360 dp e col telefono girato; le schermate di tutta l'app si fanno anche a 372 dp.
- **Foto chieste ad Android.** Ogni richiesta ha il suo numero e la sua cartella, e una foto non finisce più nel posto sbagliato.

### 0.8.2 · 28 ottobre 2026

- **I nomi.** «Timestamp con blockchain», «Controlla il timestamp» e «Copia definitiva», con un foglio che la spiega.
- **Firma col dito.** Stilografica, penna o pennarello, nero o blu; il tratto segue la velocità del dito.
- **Scansione.** «Sistema i bordi» con la lente; aspetto Documento, Grigi, Bianco e nero o Originale; pagine sotto 1 MB, rifatte sempre dalla foto.

### 0.8.1 · 28 ottobre 2026

- **Timestamp.** Una prova pubblica e gratuita OpenTimestamps che un file esisteva in un certo momento, salvata accanto al file (`.ots`) e controllata dopo dallo stesso menu. In rete va solo un'impronta del file mescolata con un numero a caso, mai il documento.

### 0.8.0 · 28 ottobre 2026

- **Beta.** Word, PDF e slide nella stessa app. Correzioni agli stili del Word italiano, alle scansioni, alle foto sostituite, ai link sul testo, al riconoscimento del testo e all'esportazione in Word delle scansioni; slide ripassate; una Guida rapida nelle Impostazioni.
