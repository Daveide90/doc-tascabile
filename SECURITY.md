# Security / Sicurezza

**[English](#english)** · **[Italiano](#italiano)**

## English

Doc Tascabile opens documents on the phone and goes online only for the optional timestamp, after asking. The main risk is a document built to attack the readers (Word, PDF, slides, spreadsheets) or the timestamp connection.

**How to report.** Use GitHub's private reporting on this repository (Security tab › Report a vulnerability), not a public issue. Include the app version (shown in Impostazioni › Informazioni), the phone model, the steps and, if you can, the file that triggers the problem. Never send documents with real personal data.

**What happens next.** An answer within 7 days. A confirmed problem is fixed in a new version; the fix is listed in the release notes and in the [CHANGELOG](CHANGELOG.md). Whoever reported it is credited, if they wish.

**Supported versions.** Only the latest release.

**How it is checked.** Automated tests that read the manifest, the network rules, the data rules and the signature (`app/tests/sicurezza.rs`); release checks in `android/build.sh` (pinned Rust version, `cargo audit` that stops on any new advisory, accepted advisories listed with their reason in `android/avvisi-accettati.txt`, no PC paths in the binaries, signature certificate); five security reviews by AI agents between 24 October and 3 November 2026. None of this is an independent professional audit.

**Recovery copies.** The copies of unsaved work stay in the app's private folder, excluded from backups and from phone-to-phone transfer. The recovery copy of a PDF opened with a password stays encrypted like the file: an automated test checks that its bytes carry neither the page text nor the text just added, and that it does not open without the password.

**Known limits.** No fuzzing yet. The document readers run in the same process that holds the Internet permission and the access to all files. The saved signature is stored in the app's private folder without encryption; the fingerprint asked before placing it is a check of the app.

## Italiano

Doc Tascabile apre i documenti sul telefono e va in rete solo per il timestamp facoltativo, dopo averlo chiesto. Il rischio principale è un documento costruito per attaccare i lettori (Word, PDF, slide, fogli di calcolo) o il collegamento del timestamp.

**Come segnalare.** Con la segnalazione privata di GitHub su questo repository (scheda Security › Report a vulnerability), non con una issue pubblica. Indicare la versione dell'app (la mostra Impostazioni › Informazioni), il modello del telefono, i passi e, se possibile, il file che fa scattare il problema. Mai documenti con dati personali veri.

**Cosa succede dopo.** Una risposta entro 7 giorni. Un problema confermato si corregge in una nuova versione, elencato nelle note della release e nel [CHANGELOG](CHANGELOG.md). Chi segnala viene citato, se lo desidera.

**Versioni seguite.** Solo l'ultima release.

**Come si controlla.** Prove automatiche che leggono manifest, regole di rete, regole dei dati e firma (`app/tests/sicurezza.rs`); controlli di rilascio in `android/build.sh` (versione di Rust fissata, `cargo audit` che si ferma su ogni avviso nuovo, avvisi accettati elencati col motivo in `android/avvisi-accettati.txt`, nessun percorso del PC nei binari, certificato della firma); cinque revisioni di sicurezza di agenti di intelligenza artificiale fra il 24 ottobre e il 3 novembre 2026. Niente di questo è una revisione professionale indipendente.

**Copie di recupero.** Le copie del lavoro non salvato stanno nella cartella privata dell'app, escluse dal backup e dal trasferimento fra telefoni. La copia di recupero di un PDF aperto con la password resta cifrata come il file: una prova automatica controlla che nei suoi byte non ci siano né il testo della pagina né quello appena aggiunto, e che senza la password non si apra.

**Limiti noti.** Il fuzzing non c'è ancora. I lettori dei documenti girano nello stesso processo che ha il permesso Internet e l'accesso a tutti i file. La firma salvata sta nella cartella privata dell'app senza cifratura; l'impronta chiesta prima di metterla è un controllo dell'app.
