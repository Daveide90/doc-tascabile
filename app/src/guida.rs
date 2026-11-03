//! La Guida rapida (tappa I, I8): che cosa fa l'app, le cose da sapere per Word, PDF e Slide, dove
//! vanno i file, i limiti di questa versione e che cosa fare se qualcosa non va. Testi fissi, in
//! un foglio che sale dalle Impostazioni.

use eframe::egui::{self, RichText};
use tascabile_touch::tema::{self, Carattere};

/// Il nome della voce nelle Impostazioni e il titolo del foglio.
pub(crate) const GUIDA: &str = "Guida rapida";
/// La riga sotto la voce.
pub(crate) const SOTTO: &str = "Che cosa fa l'app e come si usa, in due minuti";

/// Come è fatta l'app, nella Guida e nelle Impostazioni (0.8.4): una scelta di trasparenza. L'AI
/// Act (art. 50) chiede di dirlo ai sistemi di intelligenza artificiale che parlano con le persone
/// o che generano contenuti, non ai programmi scritti col loro aiuto. Dentro l'app l'intelligenza
/// artificiale c'è solo nel riconoscimento del testo, due reti neurali che girano sul telefono
/// (revisione del 31/10, punto 11).
pub(crate) const SVILUPPO_CON_IA: &str = "Doc Tascabile è stato sviluppato con l'ausilio di intelligenza artificiale (Claude, di \
     Anthropic) e controllato con prove automatiche e sul telefono. Dentro l'app l'intelligenza artificiale c'è in un punto \
     solo: il riconoscimento del testo, fatto da due modelli che girano sul telefono, senza rete. Il testo riconosciuto può \
     avere errori: va riletto.";

/// Le parti della guida: il titolo e il testo.
pub(crate) const PARTI: [(&str, &str); 11] = [
    (
        "Che cosa fa",
        "Doc Tascabile apre, scrive e firma i documenti sul telefono: PDF, Word (.docx, e anche .doc, .odt, .rtf, .txt) e \
         presentazioni (.pptx, .ppsx, .potx); legge i fogli di calcolo (.xlsx, .xls, .ods, .csv). I file si aprono dalla Home, dalle cartelle o con «Apri con» da un'altra app, \
         come WhatsApp o la posta. I documenti restano sul telefono: l'app va in rete solo quando crei o controlli un \
         «Timestamp con blockchain», dopo avertelo chiesto, e manda un'impronta del file mescolata con un numero a caso, mai il \
         documento. Come ogni sito, i server vedono l'indirizzo internet del telefono.",
    ),
    (
        "PDF",
        "Si legge scorrendo, col pizzico e col doppio tocco; la lente cerca. In fondo i modi: Compila riempie i moduli e firma \
         col dito, Annota evidenzia, sottolinea, scrive note e disegna, Modifica cambia i testi, aggiunge testo e immagini e \
         oscura per sempre quello che non si deve leggere. In una scansione riconosciuta, Modifica corregge anche il testo \
         (un nome, una data): le parole cambiate si ridisegnano nella foto, nel carattere più simile a quello stampato, \
         e la scritta di prima esce dal file. Strumenti ha il resto: le pagine (ordine, rotazione, unione, \
         divisione), la password, i numeri di pagina, la filigrana, Riconosci testo (il testo delle scansioni), Esporta in \
         Word e Scansiona, che fa un PDF dalle foto dei fogli, fatte lì o scelte dalla galleria (anche più d'una): per ogni \
         foto si sistemano i bordi del foglio, trascinando i quattro angoli (e i punti in mezzo ai lati, per un foglio \
         piegato), con Aspetto si sceglie come viene la pagina (Documento, Grigi, Bianco e nero, Originale) e la Gomma \
         sbianca col dito macchie e ombre. Nel menu «Copia definitiva» fa una copia da mandare: moduli, note e \
         firme diventano parte della pagina, uguale per tutti. La firma col dito ha tre tratti, stilografica, \
         penna e pennarello, nero o blu.",
    ),
    (
        "Word",
        "Tocca il testo e scrivi con la tastiera del telefono. Nella barra sopra la tastiera «Aa» cambia lo stile (Titolo 1, \
         2, 3) e il formato, poi grassetto, corsivo, sottolineato ed elenchi; «+» inserisce una tabella, un'immagine, la data, \
         un salto pagina. Nel menu (i tre puntini) Salva, Salva una copia, Esporta in PDF, Condividi, Trova, la pagina coi \
         numeri e le revisioni. «Nuovo» nella Home crea un documento vuoto o da un modello: verbale, relazione, lettera, \
         lezione.",
    ),
    (
        "Slide",
        "Le slide si leggono una sotto l'altra, con le miniature e le note. Presenta va a tutto schermo: un tocco a destra o \
         il volume giù va avanti, a sinistra o il volume su torna indietro; un tocco in fondo apre la barra con la penna, lo \
         schermo nero, le note e la griglia delle slide. Con un cavo HDMI o col Miracast del telefono le slide vanno sul \
         televisore o sul proiettore, e le note restano sul telefono. Nel menu Presenta dall'inizio, Condividi e il PDF delle \
         slide, anche con le note.",
    ),
    (
        "Fogli di calcolo",
        "I fogli di Excel (.xlsx, .xls), di LibreOffice (.ods) e i .csv si leggono, per ora senza modificarli: col dito si \
         scorre, col pizzico si ingrandisce, in fondo le schede dei fogli. Le righe e le colonne bloccate nel file restano \
         ferme. Un tocco su una cella mostra il suo contenuto intero e la formula. Date e numeri si leggono all'italiana; i \
         valori sono quelli salvati da Excel, senza ricalcolo. I file protetti da password non si aprono ancora.",
    ),
    (
        "Il timestamp con blockchain",
        "Nel menu di un documento salvato, «Timestamp con blockchain» crea una prova pubblica e gratuita che il file esisteva in \
         quel momento, così com'è: il timestamp si salva accanto al file, col suo nome e «.ots». Dopo qualche ora «Controlla il \
         timestamp», dallo stesso menu, lo conferma e dice da quando il file esiste. Per crearlo e controllarlo l'app parla con \
         server pubblici (OpenTimestamps, blockstream.info, mempool.space): ricevono un'impronta, mai il documento. Prima \
         l'app te lo chiede ogni volta; con «Rete per il timestamp» spenta nelle Impostazioni non va mai in rete. Non è una \
         firma e non è la marca temporale con valore legale.",
    ),
    (
        "Sicurezza e privacy",
        "Nelle Impostazioni. Il blocco dell'app chiede l'impronta o il PIN del telefono all'apertura e quando torni dopo 2 \
         minuti fuori dall'app. L'impronta per la firma fa mettere la firma salvata solo dopo l'impronta o il PIN, e poi per 5 \
         minuti non la richiede. La modalità riservata rende nero lo schermo negli screenshot, nelle registrazioni e nelle \
         videochiamate. Accendendo il blocco si accende anche la modalità riservata; la si può spegnere a parte. Blocco e impronta per la firma si spengono solo con l'impronta, e funzionano se nel telefono c'è un \
         blocco dello schermo: PIN, sequenza o impronta.",
    ),
    (
        "Dove vanno i file",
        "Le copie, i PDF esportati, i file Word fatti dai PDF e le scansioni vanno in Documenti/Tascabile. Se l'app si chiude \
         all'improvviso, alla riapertura il lavoro non salvato si può recuperare.",
    ),
    (
        "I limiti di questa versione",
        "Apri documenti tuoi o di persone che conosci: l'app non è fatta per file sconosciuti presi da internet. Le slide si \
         leggono e si presentano, ma non si modificano; video e audio delle slide non partono e le animazioni vanno a scatti. \
         Il testo delle scansioni si riconosce in italiano, e una pagina già riconosciuta non si riconosce di nuovo: se il \
         foglio è storto, giralo con Ruota prima di Crea PDF. Una correzione usa il carattere più simile fra quelli \
         dell'app (come Calibri, Arial, Times, Cambria): uno raro o la scrittura a mano non vengono uguali; le scansioni \
         salvate come fax, JBIG2 o JPEG 2000 non si correggono. Il tema scuro non c'è ancora.",
    ),
    (
        "Se qualcosa non va",
        "Nelle Impostazioni, «Condividi la diagnosi» apre la condivisione del telefono: scegli tu a chi mandare il registro \
         degli eventi (tempi, tocchi e conti, mai i testi o i nomi dei file). Da solo non parte niente, e il registro sta solo in \
         memoria: sparisce quando l'app si chiude del tutto.",
    ),
    ("Come è fatta", SVILUPPO_CON_IA),
];

/// Le parti della guida una sotto l'altra, nel foglio che scorre.
pub(crate) fn mostra(ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    for (i, (titolo, testo)) in PARTI.iter().enumerate() {
        if i > 0 {
            ui.add_space(16.0);
        }
        ui.label(RichText::new(*titolo).font(Carattere::Grassetto.font(&ctx, 17.0)).color(tema::INCHIOSTRO));
        ui.add_space(4.0);
        ui.label(RichText::new(*testo).font(Carattere::Testo.font(&ctx, 15.0)).color(tema::INCHIOSTRO));
    }
    ui.add_space(8.0);
}
