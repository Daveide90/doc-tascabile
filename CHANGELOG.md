# Changelog

**[English](#english)** · **[Italiano](#italiano)**

## English

### 0.9.9 · 10 November 2026

Copies you can tell apart, share at once and find again.

- **The three "copies" say what they are.** In the PDF menu, «Salva una copia», «Salva pulito» and «Copia definitiva» each have a line underneath: another file, the same as this one; this file, without the removed data; a file to send, with everything fixed in the page. The «Salva pulito» dialog says that the file stays this one, and points to «Copia definitiva» for a copy to send.
- **Share right after making a copy.** When a definitive copy is ready, a dialog stays on the screen, says where the file is and has «Condividi». The messages after «Salva pulito», «Salva una copia», an extracted PDF and a document saved as a copy (Word and PDF) have «Condividi» too, and so does the dialog of the compressed copy.
- **Files are where the app says, under the name every file manager shows.** The folder is `Documents/Tascabile` in the phone's storage. The app called it «Documenti/Tascabile», a name that no file manager shows; messages, the Home and the guide now use the real one. When you leave the app it tells Android about the new files in that folder, so that apps which look for documents through the phone's index find them too (on the author's phone 7 files out of 22 were missing from it).

### 0.9.8 · 10 November 2026

Fixes from a review of the whole app.

- **A saved file is still found after another app moves it.** After "Salva" and "Salva una copia" (Word and PDF) the recent takes the fingerprint of the saved bytes. It kept the one from opening, so a file saved and then moved was not found, or an older copy opened in its place.
- **The timestamp follows a renamed file.** Renaming a file also renames its `.ots` proof, never over another file, and the message says so. A new proof is written without ever replacing a file that is already there.
- **"Open with" for more files.** The app is now offered for `.rtf` files, for Word and Excel templates and macro-enabled documents, and for documents another app sends without saying their type (only by file extension, never for any file). If the document cannot be handed to the app, a message says so.
- **Small fixes.** On narrow phones the five filter chips of the Home fit the screen and the track-changes switch shows its whole name («Traccia le modifiche»). Renaming never replaces a file created at the same moment. A slide the engine cannot draw no longer stops the others. A new test opens about 13,000 damaged documents, spreadsheets, slides and PDFs: none stops the app.

### 0.9.7 · 7 November 2026

- **"Open with" from Telegram and WhatsApp no longer freezes on a white screen.** When the other app went through Android's chooser asking for a result, Android ignored the app's single window and opened a second one inside the other app, in the same process: it never drew and the app stayed stuck until removed from recents (found in the phone's system log). "Open with" now reaches a small activity without a screen that hands the document to the app's window and closes.
- **A signature can go on top of another one.** With a signature waiting for "Tap where to put it", a tap on an existing signature, mark or field selected it and the new one was not placed; now the tap always places it. PDF Information also shows "Firme a mano" (hand signatures), counted apart from digital signatures.
- **My signatures.** Up to 6 signatures and 6 sets of initials saved, all listed in the signature sheet. "Le mie firme", from the signature sheet and from Settings, shows them with "Delete" for each (with confirmation) and "Delete all". A new signature is added next to the saved ones after asking; the signature saved before this version stays the first.

### 0.9.6 · 7 November 2026

- **Word documents open without freezing the screen.** Reading, opening and the first layout run on a background thread, also for files sent with "Open with": the screen shows «Apro il documento» and Back cancels. The Italian dictionary starts loading at the same moment, so a document is laid out once instead of twice.
- **Recents that moved are found again.** Each recent remembers a fingerprint of its bytes (SHA-256), kept only in the app's list of recents on the phone. If another app moved or renamed the file, the app looks for the same bytes in the phone's storage, reopens it and updates the recent. When a file cannot be found, the message says whether file access is off or the file is gone.
- **Rename.** From the ⋮ menu of recents and of files in folders, and from the editor menu: same folder, same extension. A document received with "Open with" belongs to the other app and is not renamed; a copy can be saved instead.
- **Smoother scrolling in long documents.** Each strip of the page now draws only what it shows: in a 6000-paragraph test document a strip went from about 1000 ms to 9 ms on a PC.

### 0.9.5 · 6 November 2026

- **The app's own licence in the app.** Impostazioni › Caratteri, icone e librerie now opens with «Licenza di Doc Tascabile»: the PolyForm Noncommercial License 1.0.0 with its «Required Notice», which the licence asks to pass on with the software. Fix from an independent review of the publication.

### 0.9.4 · 6 November 2026

- **Third-party licences complete in the app.** Impostazioni › Caratteri, icone e librerie now also lists the PDFCraft and DeckCraft engines, the slide fonts (Carlito, Caladea, Inter, Liberation, with their OFL texts), every Rust library compiled into the app (479 crates, with copyright lines, licence texts and NOTICE files, BSD, ISC and Unicode included) and the Android libraries (AndroidX, GameActivity, Kotlin with its NOTICE and the ThreeTen BSD licence). The Rust list is generated from `Cargo.lock` by `app/licenze/genera.py`, and a release build stops if it is out of date.

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

### 0.9.9 · 10 novembre 2026

Copie che si distinguono, si condividono subito e si ritrovano.

- **Le tre «copie» dicono che cosa sono.** Nel menu del PDF «Salva una copia», «Salva pulito» e «Copia definitiva» hanno sotto una riga: un altro file, uguale a questo; questo file, senza i dati tolti; un file da mandare, con tutto fissato nella pagina. Il dialogo di «Salva pulito» dice che il file resta questo, e rimanda a «Copia definitiva» per una copia da mandare.
- **Condividi subito dopo una copia.** Quando la copia definitiva è pronta un dialogo resta sullo schermo, dice dov'è il file e ha «Condividi». Anche gli avvisi dopo «Salva pulito», «Salva una copia», un PDF estratto e un documento salvato in una copia (Word e PDF) hanno «Condividi», e così il dialogo della copia compressa.
- **I file sono dove dice l'app, col nome che mostra ogni gestore di file.** La cartella è `Documents/Tascabile`, nella memoria del telefono. L'app la chiamava «Documenti/Tascabile», un nome che nessun gestore di file mostra; gli avvisi, la Home e la guida ora usano quello vero. Uscendo dall'app i file nuovi di quella cartella vengono detti ad Android, così li trovano anche le app che cercano i documenti passando dall'indice del telefono (sul telefono dell'autore ne mancavano 7 su 22).

### 0.9.8 · 10 novembre 2026

Correzioni da una revisione di tutta l'app.

- **Un file salvato si ritrova anche se un'altra app lo sposta.** Dopo «Salva» e «Salva una copia» (Word e PDF) il recente prende l'impronta dei byte salvati. Teneva quella dell'apertura: un file salvato e poi spostato non si ritrovava, o al suo posto si apriva una copia più vecchia.
- **Il timestamp segue il file rinominato.** Rinominando un file prende il nome nuovo anche la sua prova `.ots`, mai sopra un altro file, e il messaggio lo dice. Una prova nuova si scrive senza mai sostituire un file che c'è già.
- **«Apri con» per più file.** L'app ora si offre per i file `.rtf`, per i modelli e i documenti con macro di Word e di Excel, e per i documenti che un'altra app manda senza dirne il tipo (solo in base all'estensione, mai per un file qualunque). Se il documento non riesce ad arrivare all'app, un messaggio lo dice.
- **Piccole correzioni.** Sui telefoni stretti i cinque chip della Home stanno nello schermo e l'interruttore delle revisioni mostra il nome intero («Traccia le modifiche»). Il rinomina non sostituisce mai un file creato nello stesso momento. Una slide che il motore non riesce a disegnare non ferma più le altre. Una prova nuova apre circa 13.000 documenti, fogli, slide e PDF rovinati: nessuno ferma l'app.

### 0.9.7 · 7 novembre 2026

- **L'«Apri con» da Telegram e WhatsApp non si blocca più su schermo bianco.** Quando l'altra app passava dal selettore di Android chiedendo un risultato, Android ignorava la finestra unica dell'app e ne apriva una seconda dentro l'altra app, nello stesso processo: non disegnava mai e l'app restava ferma finché non la si toglieva dalle recenti (trovato nel registro di sistema del telefono). Ora l'«Apri con» arriva a una piccola attività senza schermo che passa il documento alla finestra dell'app e si chiude.
- **Una firma si mette anche sopra un'altra.** Con una firma in attesa di «Tocca dove mettere la firma», il tocco su una firma, un segno o un campo già presenti li sceglieva e la nuova non si posava; ora il tocco la posa sempre. Nelle Informazioni del PDF c'è anche «Firme a mano», contate a parte dalle firme digitali.
- **Le mie firme.** Fino a 6 firme e 6 iniziali salvate, tutte nel foglio della firma. «Le mie firme», dal foglio della firma e dalle Impostazioni, le mostra con «Elimina» per ognuna (con conferma) ed «Elimina tutte». Una firma nuova si aggiunge accanto a quelle salvate dopo una domanda; quella salvata prima di questa versione resta la prima.

### 0.9.6 · 7 novembre 2026

- **I documenti Word si aprono senza fermare lo schermo.** Lettura, apertura e prima impaginazione vanno su un thread a parte, anche per i file mandati con «Apri con»: lo schermo mostra «Apro il documento» e Indietro annulla. Il dizionario italiano parte nello stesso momento, così il documento si impagina una volta sola invece di due.
- **I recenti spostati si ritrovano.** Ogni recente ricorda un'impronta dei suoi byte (SHA-256), tenuta solo nell'elenco dei recenti dell'app, sul telefono. Se un'altra app ha spostato o rinominato il file, l'app cerca gli stessi byte nella memoria del telefono, lo riapre e aggiorna il recente. Quando un file non si trova, il messaggio dice se è spento l'accesso ai file o se il file non c'è più.
- **Rinomina.** Dal menu ⋮ dei recenti e dei file nelle cartelle, e dal menu dell'Editor: stessa cartella, stessa estensione. Un documento arrivato con «Apri con» è dell'altra app e non si rinomina; se ne può salvare una copia.
- **Scorrimento più fluido nei documenti lunghi.** Ogni fascia della pagina ora disegna solo quello che mostra: in un documento di prova da 6000 paragrafi una fascia è passata da circa 1000 ms a 9 ms su PC.

### 0.9.5 · 6 novembre 2026

- **La licenza dell'app dentro l'app.** Impostazioni › Caratteri, icone e librerie ora comincia con «Licenza di Doc Tascabile»: la PolyForm Noncommercial License 1.0.0 con la sua «Required Notice», che la licenza chiede di passare insieme al software. Correzione da una revisione indipendente della pubblicazione.

### 0.9.4 · 6 novembre 2026

- **Licenze di terzi complete nell'app.** In Impostazioni › Caratteri, icone e librerie ci sono anche i motori PDFCraft e DeckCraft, i caratteri delle slide (Carlito, Caladea, Inter, Liberation, con i testi OFL), tutte le librerie Rust compilate nell'app (479 crate, con le righe di copyright, i testi delle licenze e i file NOTICE, comprese BSD, ISC e Unicode) e le librerie Android (AndroidX, GameActivity, Kotlin con il suo NOTICE e la licenza BSD di ThreeTen). L'elenco Rust lo genera `app/licenze/genera.py` da `Cargo.lock`, e la build di rilascio si ferma se non è aggiornato.

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
