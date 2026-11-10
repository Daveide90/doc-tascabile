//! Il lettore dei PDF (bozzetto 7): la barra alta col nome, «PDF · N pagine», indietro, la lente
//! e il menu (Miniature, Vai a pagina, Indice, Condividi); sotto, il lettore di `tascabile_pdf`;
//! in fondo la barra flottante con Miniature, Indice e Vai a pagina. Il PDF si apre su un thread
//! suo e arriva al thread dell'interfaccia quando è pronto; intanto, dopo un attimo, «Apro il
//! PDF…».
//!
//! Per orientarsi: la ricerca nella barra alta come Trova di Word (campo a pillola con la
//! tastiera vera, conteggio «2 di 15», frecce, X); le miniature, l'indice dei segnalibri e «Vai a
//! pagina» in fogli dal fondo; i link, che portano alla pagina o, quelli esterni, chiedono prima
//! di aprire il browser.
//!
//! Per prendere il testo e tornare: la pressione lunga seleziona una parola (maniglie, lente,
//! Copia e Tutto) e il testo copiato va negli appunti di Android segnato come sensibile; un PDF
//! con password chiede la password in un dialogo (punti al posto delle lettere, l'occhio per
//! vederla, «Password sbagliata»), che non va mai in un log, nella diagnosi né in un file; ogni
//! PDF riprende dal punto e dallo zoom dell'ultima volta (`posizioni-pdf.json` nella cartella
//! dell'app, con l'impronta del contenuto come chiave); «Informazioni sul documento» nel menu.
//!
//! Nella diagnosi solo misure: byte, pagine, tempi, zoom, conti e generi d'errore, mai il nome
//! del file, l'URI, il testo cercato o selezionato, la password, i titoli o gli indirizzi dei
//! link.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use eframe::egui::{self, CornerRadius, Rect, RichText, UiBuilder, Vec2, pos2, vec2};
use tascabile_files::{Posizione, Posizioni, Recente, Tipo, impronta, tipo_da_nome};
use tascabile_pdf::lettore::{Conteggio, Esito, Lettore};
use tascabile_pdf::{Destinazione, ErroreApertura, Genere, Informazioni, Modifica, Motore, Pdf, Segnalibro};
use tascabile_platform::{TipoTastiera, diagnosi, diagnosi_raffica, genere_indirizzo};
use tascabile_touch::tema::{self, Carattere};
use tascabile_touch::{
    BarraAlta, BarraVoci, CampoTesto, Dialogo, FoglioDalFondo, Menu, Peso, Pulsante, PulsanteIcona, RispostaDialogo, SceltaMenu, icone,
};

use crate::pdf_compila::{Compila, da_compilare_detto};
use crate::pdf_lavoro::{
    ANNULLA_MODIFICA, AzioneSalva, MODIFICHE_NON_SALVATE, ModoPdf, RIPETI_MODIFICA, SALVA, SALVA_COPIA, STRUMENTI, ScrittoPdf, modi_disponibili,
    si_modifica,
};
use crate::pdf_recupero::CopiaDelPdf;
use crate::recupero::CopiaPdf;
use crate::schermo_editor::{DOPO, PRIMA, riga_informazione};
use crate::{Da, Dopo, Errore, Fuoco, Schermata, TascabileApp, adesso, home};

/// Le etichette dei pulsanti della barra alta (per l'accessibilità e per le prove).
pub(crate) const INDIETRO: &str = "Indietro";
pub(crate) const MENU: &str = "Menu";
pub(crate) const CONDIVIDI: &str = "Condividi";
/// La voce del menu che fa la copia per chi la riceve: moduli, note, disegni e firme diventano
/// parte della pagina. Fino alla 0.8.1 era «Copia appiattita» (l'autore il 28/10: «che vuol dire pdf
/// piatto?»): ora il foglio spiega prima che cosa fa, e il pulsante la crea.
pub(crate) const COPIA_DEFINITIVA: &str = "Copia definitiva";
pub(crate) const CREA_DEFINITIVA: &str = "Crea la copia definitiva";
/// Il foglio della copia definitiva, in tre capoversi: che cosa si perde nella copia (in rosso,
/// per primo: segnalazione dell'autore del 10/11, «in sostanza i dati li perdi»), che l'originale
/// resta, e che non è un sigillo.
pub(crate) const SPIEGA_DEFINITIVA: [&str; 3] = [
    "Nella copia moduli compilati, note, evidenziazioni, disegni e firme diventano parte della pagina: lì non si modificano e \
     non si tolgono più.",
    "È un file in più: il tuo PDF resta com'è e si può ancora modificare. La copia va nella memoria del telefono, in \
     Documents/Tascabile, e appena è pronta la puoi condividere.",
    "Chi la riceve la vede uguale su ogni telefono e computer. Non è un sigillo: chi ha un programma che modifica i PDF può \
     ancora cambiarla.",
];
/// Sotto la spiegazione, se il PDF ha firme digitali.
const FIRME_NELLA_DEFINITIVA: &str = "Questo PDF ha firme digitali: nella copia non valgono più.";
/// Le righe sotto le due copie del menu, sulla stessa domanda: dopo, si può ancora modificare?
/// (Seconda segnalazione dell'autore del 10/11: le righe della 0.9.9, «un altro file, uguale a
/// questo» e «da mandare, con tutto fissato», «creano ancora più confusione».) Ognuna sta su una
/// riga sola; quella della copia definitiva è rossa come la sua voce.
pub(crate) const SPIEGA_SALVA_COPIA: &str = "Si può ancora modificare";
pub(crate) const SPIEGA_COPIA_DEFINITIVA: &str = "Non si può più modificare";
/// Il dialogo dopo la copia definitiva: dov'è, e «Condividi» subito.
pub(crate) const COPIA_PRONTA: &str = "Copia definitiva pronta";
pub(crate) const CHIUDI: &str = "Chiudi";
pub(crate) const CERCA: &str = "Cerca";
/// Il testo grigio del campo della ricerca vuoto, e la X che la chiude.
pub(crate) const VUOTO_CERCA: &str = "Cerca nel PDF";
pub(crate) const CHIUDI_CERCA: &str = "Chiudi la ricerca";
/// Le voci del menu e della barra in fondo, e i titoli dei loro fogli.
pub(crate) const MINIATURE: &str = "Miniature";
pub(crate) const INDICE: &str = "Indice";
pub(crate) const VAI_A_PAGINA: &str = "Vai a pagina";
pub(crate) const VAI: &str = "Vai";
/// Il dialogo di un link esterno.
pub(crate) const APRIRE_IL_LINK: &str = "Aprire il link?";
pub(crate) const APRI: &str = "Apri";
pub(crate) const ANNULLA: &str = "Annulla";
const LINK_CHE_NON_SI_APRE: &str = "Link che non si apre";
/// Il campo della ricerca dal bordo sinistro, come Trova.
const CERCA_DA_SINISTRA: f32 = 12.0;
/// La barra in fondo lascia tanto spazio sopra di sé dopo l'ultima pagina.
const SOPRA_LA_BARRA: f32 = 8.0;
/// Quanto rientra un segnalibro per ogni livello sotto il primo, come l'Indice di Word; oltre il
/// quarto non rientra più.
const RIENTRO: f32 = 20.0;
/// Un indirizzo lungo nel dialogo si accorcia a tanti caratteri.
const INDIRIZZO_MASSIMO: usize = 120;
/// Il testo mentre il PDF si apre.
pub(crate) const APRO: &str = "Apro il PDF…";
/// Dopo quanto si dice che il PDF si sta aprendo: prima sarebbe solo un lampo.
const DOPO_QUANTO_SI_DICE: Duration = Duration::from_millis(300);
/// L'angolo del menu, come quello dell'Editor (`.menu`).
const MENU_DESTRA: f32 = 8.0;
const MENU_SOTTO: f32 = 44.0;
/// Sotto questa larghezza (dp) Ripeti va nel menu: con quattro icone nella barra alta al nome e
/// allo stato restavano 116 dp, e «2 da compilare» si perdeva (0.8.3, il telefono dell'autore è
/// largo 372 dp).
const RIPETI_NEL_MENU_SOTTO: f32 = 400.0;
/// La voce del menu e il titolo del suo foglio.
pub(crate) const INFORMAZIONI: &str = "Informazioni sul documento";
const TITOLO_INFORMAZIONI: &str = "Informazioni";
/// Il dialogo della password.
pub(crate) const PDF_PROTETTO: &str = "PDF protetto";
pub(crate) const PASSWORD_SBAGLIATA: &str = "Password sbagliata";
pub(crate) const VUOTO_PASSWORD: &str = "Password";
/// L'avviso di una pressione lunga su una pagina senza testo.
pub(crate) const SENZA_TESTO: &str = "In questa pagina non c'è testo da selezionare: forse è una scansione.";
/// Il file delle posizioni nella cartella dell'app.
pub(crate) const FILE_POSIZIONI: &str = "posizioni-pdf.json";
/// La posizione si ricorda quando la vista resta ferma tanto (e alla chiusura).
const RICORDA_DOPO: Duration = Duration::from_millis(1500);

/// Il nome di un PDF arrivato con «Apri con»: quello vero, con «.pdf» in fondo se gli manca;
/// senza, dall'URI se finisce in .pdf (decodificato dai «%», e del percorso che ne esce l'ultimo
/// pezzo), altrimenti «documento.pdf».
pub(crate) fn nome_del_pdf(vero: &str, uri: &str) -> String {
    let vero = vero.trim();
    if !vero.is_empty() {
        return if tipo_da_nome(vero) == Tipo::Pdf { vero.to_string() } else { format!("{vero}.pdf") };
    }
    const RIPIEGO: &str = "documento.pdf";
    let ultimo = uri.rsplit('/').next().unwrap_or("");
    let Some(decodificato) = decodifica_percento(ultimo) else { return RIPIEGO.to_string() };
    let nome = decodificato.rsplit(['/', '\\', ':']).next().unwrap_or("").trim();
    let pulito = !nome.starts_with('.') && !nome.chars().any(char::is_control);
    if pulito && tipo_da_nome(nome) == Tipo::Pdf { nome.to_string() } else { RIPIEGO.to_string() }
}

/// Un pezzo di URI coi «%» decodificati (UTF-8); None se la codifica è rotta.
fn decodifica_percento(pezzo: &str) -> Option<String> {
    let mut byte = Vec::with_capacity(pezzo.len());
    let mut i = 0;
    while i < pezzo.len() {
        if pezzo.as_bytes()[i] == b'%' {
            byte.push(u8::from_str_radix(pezzo.get(i + 1..i + 3)?, 16).ok()?);
            i += 3;
        } else {
            byte.push(pezzo.as_bytes()[i]);
            i += 1;
        }
    }
    String::from_utf8(byte).ok()
}

/// «PDF · 3 pagine», «PDF · 1 pagina».
pub(crate) fn sottotitolo(pagine: usize) -> String {
    if pagine == 1 { "PDF · 1 pagina".to_string() } else { format!("PDF · {pagine} pagine") }
}

/// «2 di 15», «Cerco…» mentre si leggono le pagine senza ancora un risultato, «Nessun
/// risultato» a ricerca finita; niente col campo vuoto.
pub(crate) fn conteggio_pdf(c: Option<Conteggio>, vuoto: bool) -> String {
    match c {
        _ if vuoto => String::new(),
        None => String::new(),
        Some(Conteggio { totale: 0, finita: false, .. }) => "Cerco…".to_string(),
        Some(Conteggio { totale: 0, .. }) => "Nessun risultato".to_string(),
        Some(Conteggio { attuale: Some(i), totale, .. }) => format!("{} di {totale}", i + 1),
        Some(Conteggio { totale, .. }) => totale.to_string(),
    }
}

/// Un indirizzo lungo accorciato coi puntini, per il dialogo.
fn accorciato(indirizzo: &str) -> String {
    if indirizzo.chars().count() <= INDIRIZZO_MASSIMO {
        indirizzo.to_string()
    } else {
        format!("{}…", indirizzo.chars().take(INDIRIZZO_MASSIMO).collect::<String>())
    }
}

/// Il PDF del lettore.
pub(crate) struct PdfAperto {
    /// Il motore del PDF, sul suo thread: modifiche, annulla e ripeti, salvataggio.
    pub(crate) motore: Motore<ScrittoPdf>,
    pub(crate) lettore: Lettore,
    /// Il nome del file, per la barra alta e per condividerlo.
    pub(crate) nome: String,
    /// Da quando si è cominciato ad aprirlo, per la prima pagina nella diagnosi.
    inizio: Instant,
    /// La ricerca al posto della barra alta, col suo campo.
    pub(crate) trova: Option<CampoTesto>,
    /// Il campo di «Vai a pagina» e se il numero scritto non va.
    pub(crate) vai: CampoTesto,
    vai_sbagliato: bool,
    /// Un link esterno toccato, che aspetta la conferma.
    pub(crate) link_esterno: Option<String>,
    /// Il percorso o l'URI da cui è arrivato (per la cartella nelle informazioni, e Salva), se c'è.
    pub(crate) chiave: Option<String>,
    /// Da dove si è aperto: Salva scrive sul file solo se è un file della memoria del telefono.
    pub(crate) da: Da,
    /// L'impronta del contenuto: la chiave della sua posizione.
    pub(crate) impronta: String,
    /// L'ultima posizione scritta (pagina, punto, zoom, x), e l'ultima vista con da quando.
    pub(crate) ricordata: Option<PuntoLetto>,
    vista: Option<(PuntoLetto, Instant)>,
    /// «Informazioni sul documento», prese quando si apre il foglio, con la cartella.
    informazioni: Option<(Informazioni, String)>,
    /// Il modo della barra in fondo.
    pub(crate) modo: ModoPdf,
    /// L'avviso delle firme digitali ha avuto un sì: le modifiche dopo non lo chiedono più.
    pub(crate) firme_accettate: bool,
    /// Le modifiche vanno in una copia (scelto all'avviso delle firme): Salva non tocca
    /// l'originale.
    pub(crate) su_copia: bool,
    /// Una modifica che aspetta la risposta all'avviso delle firme.
    pub(crate) in_attesa_firme: Option<Modifica>,
    /// Quello da fare quando il salvataggio chiesto dal dialogo «Salvare le modifiche?» riesce.
    pub(crate) dopo_il_salvataggio: Option<Dopo>,
    /// È stato salvato in questa sessione: la barra alta dice «Salvato» e dove.
    pub(crate) salvato: bool,
    /// Il modo Compila: la scrittura di un campo, i fogli delle scelte e della data, Fill & Sign.
    pub(crate) compila: Compila,
    /// La copia di recupero delle modifiche non salvate.
    pub(crate) copia: CopiaDelPdf,
    /// Il modo Modifica: gli oggetti delle pagine, quello scelto, Oscura, le immagini.
    pub(crate) modifica: crate::pdf_modifica::StatoModifica,
    /// Il modo Annota: l'attrezzo, i colori, i tratti in volo, l'annotazione scelta.
    pub(crate) annota: crate::pdf_annota::StatoAnnota,
    /// «Pagine e file» (P6): la griglia di Organizza pagine, Unisci, Dividi, Proteggi.
    pub(crate) pagine: crate::pdf_pagine::StatoPagine,
}

/// Un PDF che chiede la password: i byte già letti (si riprova senza rileggere il file), il
/// campo coi punti e se l'ultima password non andava. La password sta solo nel campo finché non
/// si tocca «Apri».
pub(crate) struct Password {
    byte: Arc<Vec<u8>>,
    impronta: String,
    da: Da,
    nome: String,
    chiave: Option<String>,
    /// La copia di recupero da cui si riapre, se si riapre da lì.
    recupero: Option<CopiaPdf>,
    pub(crate) campo: CampoTesto,
    pub(crate) visibile: bool,
    pub(crate) sbagliata: bool,
}

/// I fogli dal fondo del lettore: vanno disegnati a ogni frame, anche chiusi, per salire.
pub(crate) struct FogliPdf {
    pub(crate) miniature: FoglioDalFondo,
    pub(crate) indice: FoglioDalFondo,
    pub(crate) vai: FoglioDalFondo,
    /// «Informazioni sul documento».
    pub(crate) info: FoglioDalFondo,
    /// «Copia definitiva»: che cosa fa, prima di farla.
    pub(crate) definitiva: FoglioDalFondo,
    /// «Strumenti PDF» (bozzetto 8).
    pub(crate) strumenti: FoglioDalFondo,
    /// Le scelte di un menu o di un elenco del modulo, e la data di un campo data.
    pub(crate) scelta: FoglioDalFondo,
    pub(crate) data: FoglioDalFondo,
    /// Il foglio della firma e l'anteprima della firma dalla foto.
    pub(crate) firma: FoglioDalFondo,
    pub(crate) foto: FoglioDalFondo,
    /// Il modo Modifica: da dove viene l'immagine, il formato di un testo aggiunto, la filigrana,
    /// i numeri di pagina.
    pub(crate) immagine: FoglioDalFondo,
    pub(crate) testo: FoglioDalFondo,
    pub(crate) filigrana: FoglioDalFondo,
    pub(crate) numeri: FoglioDalFondo,
    /// «Riconosci testo»: tutte le pagine o questa (P8).
    pub(crate) riconosci: FoglioDalFondo,
    /// Il modo Annota: la nota col suo testo, il formato dell'annotazione scelta (P5); il modo
    /// Modifica: dove porta un link.
    pub(crate) nota: FoglioDalFondo,
    pub(crate) annotazione: FoglioDalFondo,
    pub(crate) link: FoglioDalFondo,
    /// «Pagine e file» (P6): la pagina bianca, Unisci, Dividi, Comprimi, Proteggi.
    pub(crate) bianca: FoglioDalFondo,
    pub(crate) unisci: FoglioDalFondo,
    pub(crate) dividi: FoglioDalFondo,
    pub(crate) comprimi: FoglioDalFondo,
    pub(crate) proteggi: FoglioDalFondo,
    /// «Vai a pagina» era aperto al frame prima: se si chiude toccando fuori, il campo lascia la
    /// tastiera.
    vai_prima: bool,
}

impl FogliPdf {
    pub(crate) fn new() -> Self {
        Self {
            miniature: FoglioDalFondo::new(egui::Id::new("foglio_miniature")),
            indice: FoglioDalFondo::new(egui::Id::new("foglio_indice_pdf")),
            vai: FoglioDalFondo::new(egui::Id::new("foglio_vai_a_pagina")),
            info: FoglioDalFondo::new(egui::Id::new("foglio_informazioni_pdf")),
            definitiva: FoglioDalFondo::new(egui::Id::new("foglio_copia_definitiva")),
            strumenti: FoglioDalFondo::new(egui::Id::new("foglio_strumenti_pdf")),
            scelta: FoglioDalFondo::new(egui::Id::new("foglio_scelta_pdf")),
            data: FoglioDalFondo::new(egui::Id::new("foglio_data_pdf")),
            firma: FoglioDalFondo::new(egui::Id::new("foglio_firma_pdf")),
            foto: FoglioDalFondo::new(egui::Id::new("foglio_foto_firma_pdf")),
            immagine: FoglioDalFondo::new(egui::Id::new("foglio_immagine_pdf")),
            testo: FoglioDalFondo::new(egui::Id::new("foglio_testo_pdf")),
            filigrana: FoglioDalFondo::new(egui::Id::new("foglio_filigrana_pdf")),
            numeri: FoglioDalFondo::new(egui::Id::new("foglio_numeri_pdf")),
            riconosci: FoglioDalFondo::new(egui::Id::new("foglio_riconosci_pdf")),
            nota: FoglioDalFondo::new(egui::Id::new("foglio_nota_pdf")),
            annotazione: FoglioDalFondo::new(egui::Id::new("foglio_annotazione_pdf")),
            link: FoglioDalFondo::new(egui::Id::new("foglio_link_pdf")),
            bianca: FoglioDalFondo::new(egui::Id::new("foglio_pagina_bianca")),
            unisci: FoglioDalFondo::new(egui::Id::new("foglio_unisci_pdf")),
            dividi: FoglioDalFondo::new(egui::Id::new("foglio_dividi_pdf")),
            comprimi: FoglioDalFondo::new(egui::Id::new("foglio_comprimi_pdf")),
            proteggi: FoglioDalFondo::new(egui::Id::new("foglio_proteggi_pdf")),
            vai_prima: false,
        }
    }

    pub(crate) fn chiudi(&mut self) {
        self.miniature.chiudi();
        self.indice.chiudi();
        self.vai.chiudi();
        self.info.chiudi();
        self.strumenti.chiudi();
        self.scelta.chiudi();
        self.data.chiudi();
        self.firma.chiudi();
        self.foto.chiudi();
        self.immagine.chiudi();
        self.testo.chiudi();
        self.filigrana.chiudi();
        self.numeri.chiudi();
        self.riconosci.chiudi();
        self.nota.chiudi();
        self.annotazione.chiudi();
        self.link.chiudi();
        self.bianca.chiudi();
        self.unisci.chiudi();
        self.dividi.chiudi();
        self.comprimi.chiudi();
        self.proteggi.chiudi();
    }
}

/// Quello che si è toccato nel lettore in un frame, da fare dopo averlo disegnato.
enum Azione {
    Annulla,
    Ripeti,
    Cerca,
    Menu,
    Modo(ModoPdf),
    Strumenti,
}

/// Il punto che si legge in un PDF, per la ripresa: pagina, y in pt dall'alto, zoom e x in
/// frazione della larghezza della pagina.
pub(crate) type PuntoLetto = (usize, f32, f32, f32);

/// Una posizione che si scrive sul suo thread, con pagina e pagine per la riga della diagnosi.
pub(crate) type ScritturaPosizione = (std::thread::JoinHandle<Result<(), std::io::ErrorKind>>, usize, usize);

/// Un PDF che si apre sul suo thread: l'esito con l'impronta del contenuto e la posizione
/// ricordata (lette lì, non sul thread dell'interfaccia).
pub(crate) struct AperturaPdf {
    esito: Receiver<(Result<Pdf, ErroreApertura>, String, Option<Posizione>)>,
    da: Da,
    /// I byte, tenuti anche qui: se il PDF chiede la password si riprova con loro.
    byte: Arc<Vec<u8>>,
    inizio: Instant,
    /// Quando lo si è chiesto: dopo `DOPO_QUANTO_SI_DICE` lo si dice sullo schermo.
    chiesto: Instant,
    nome: String,
    chiave: Option<String>,
    /// Si apre con una password data nel dialogo.
    con_password: bool,
    /// La copia di recupero da cui si riapre, se si riapre da lì.
    recupero: Option<CopiaPdf>,
}

/// Lo zoom all'italiana per la diagnosi: «1,50».
fn zoom_detto(z: f32) -> String {
    format!("{z:.2}").replace('.', ",")
}

impl TascabileApp {
    /// Comincia ad aprire un PDF (riconosciuto dal contenuto, qualunque sia il nome) sul suo
    /// thread: la sua lettura e i suoi controlli non fermano l'interfaccia. Un'apertura chiesta
    /// prima e non finita si lascia andare.
    pub(crate) fn apri_pdf(&mut self, da: Da, byte: Vec<u8>, nome: &str, chiave: Option<String>, inizio: Instant) {
        // Un nome senza estensione (un «Apri con» senza nome) diventa un PDF.
        let nome = if nome.contains('.') { nome.to_string() } else { nome_del_pdf(nome, "") };
        self.avvia_pdf(da, Arc::new(byte), nome, chiave, inizio, None, None, None);
    }

    /// Riapre i byte di una copia di recupero: senza file (Salva scrive una copia in
    /// Documents/Tascabile) e con le modifiche da salvare.
    pub(crate) fn avvia_pdf_recuperato(&mut self, byte: Vec<u8>, nome: String, copia: CopiaPdf) {
        self.avvia_pdf(Da::Recupero, Arc::new(byte), nome, None, Instant::now(), None, None, Some(copia));
    }

    /// L'apertura sul thread `pdf-apertura`, con la password del dialogo se c'è (si sposta nel
    /// thread e lì finisce); l'impronta del contenuto si calcola lì la prima volta.
    #[allow(clippy::too_many_arguments)]
    fn avvia_pdf(
        &mut self,
        da: Da,
        byte: Arc<Vec<u8>>,
        nome: String,
        chiave: Option<String>,
        inizio: Instant,
        password: Option<String>,
        impronta_nota: Option<String>,
        recupero: Option<CopiaPdf>,
    ) {
        let misura = byte.len();
        let (manda, esito) = std::sync::mpsc::channel();
        let risveglio = self.risveglio.clone();
        let con_password = password.is_some();
        let per_il_thread = byte.clone();
        // Una posizione che si sta scrivendo (il PDF di prima chiuso adesso) va letta finita.
        self.aspetta_le_posizioni();
        let file_posizioni = self.piattaforma.cartella_app().join(FILE_POSIZIONI);
        let lavoro = move || {
            let chiave = impronta_nota.unwrap_or_else(|| impronta(&per_il_thread));
            let pdf = Pdf::apri_condiviso(per_il_thread, password.as_deref());
            drop(password);
            let ricordata = if pdf.is_ok() { Posizioni::carica(&file_posizioni).trova(&chiave).cloned() } else { None };
            let _ = manda.send((pdf, chiave, ricordata));
            risveglio();
        };
        if let Err(e) = std::thread::Builder::new().name("pdf-apertura".into()).spawn(lavoro) {
            da.scrivi(format!("{}: {misura} byte, tipo pdf, thread di apertura non partito ({:?})", da.etichetta(), e.kind()));
            self.errore = Some(Errore::apertura("Non riesco ad aprire il file: il telefono non ha lasciato partire la lettura."));
            return;
        }
        // Mentre si apre, il documento dell'Editor non riceve più niente (la tastiera si chiude e
        // i tocchi li ferma il velo di `mostra_apertura_pdf`): quando il PDF arriva lo si chiude,
        // e modifiche fatte nel frattempo andrebbero perse senza chiedere.
        self.piattaforma.hide_keyboard();
        self.fuoco_al_documento();
        if let Some(e) = self.editor_mut() {
            e.ponte.chiudi();
        }
        self.apertura_pdf = Some(AperturaPdf { esito, da, byte, inizio, chiesto: Instant::now(), nome, chiave, con_password, recupero });
    }

    /// A ogni passo: se il PDF che si apre è pronto, va nel lettore (il documento dell'Editor si
    /// chiude, come aprendo un documento); se non si apre, lo dice il dialogo.
    pub(crate) fn passo_apertura_pdf(&mut self) {
        let Some(a) = &self.apertura_pdf else { return };
        let (esito, impronta, ricordata) = match a.esito.try_recv() {
            Ok(esito) => esito,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => (Err(Genere::PanicoDelLettore.into()), String::new(), None),
        };
        let Some(a) = self.apertura_pdf.take() else { return };
        let etichetta = a.da.etichetta();
        let misura = a.byte.len();
        match esito {
            Ok(pdf) => {
                let ms = a.inizio.elapsed().as_secs_f64() * 1000.0;
                let pagine = pdf.pagine();
                let con = if a.con_password { ", con la password" } else { "" };
                let riparato = pdf.riparato();
                let rip = if riparato { ", riparato" } else { "" };
                a.da.scrivi(format!("{etichetta}: {misura} byte, tipo pdf, aperto in {ms:.0} ms, {pagine} pagine{con}{rip}"));
                if riparato {
                    self.avviso_breve.avvisa("Il PDF era rovinato e l'ho riparato: potrebbe mancare qualcosa, anche delle pagine.");
                }
                // Il documento dell'Editor esce, come aprendo un altro documento: le modifiche le
                // ha già chieste chi ha cominciato l'apertura.
                let motore: Motore<ScrittoPdf> = match Motore::avvia(pdf, self.risveglio.clone()) {
                    Ok(m) => m,
                    Err(e) => {
                        diagnosi(format!("pdf: thread del motore non partito ({:?})", e.kind()));
                        self.errore = Some(Errore::apertura("Non riesco ad aprire il file: il telefono non ha lasciato partire il lettore dei PDF."));
                        return;
                    }
                };
                let uscito = self.sostituisci(None);
                self.metti_da_parte(uscito);
                self.chiudi_pdf();
                self.chiudi_slide();
                self.chiudi_foglio();
                let mut lettore = Lettore::new(motore.stato(), egui::Id::new("lettore_pdf"));
                // La ripresa: il punto e lo zoom dell'ultima volta, se il PDF li ha ancora.
                let ricordata = ricordata.filter(|r| r.pagina < pagine).map(|r| {
                    lettore.imposta_zoom(r.zoom);
                    match r.x {
                        Some(x) => lettore.vai_al_punto_xy(r.pagina, x, r.y),
                        None => lettore.vai_al_punto(r.pagina, Some(r.y)),
                    }
                    diagnosi(format!(
                        "pdf: ripresa dalla pagina {} di {pagine}, zoom {}, chiave {}",
                        r.pagina + 1,
                        zoom_detto(lettore.zoom()),
                        impronta.get(..8).unwrap_or("")
                    ));
                    (r.pagina, r.y, lettore.zoom(), r.x.unwrap_or(0.5))
                });
                // Un modulo da compilare si apre nel modo Compila, coi campi evidenziati.
                let stato = motore.stato();
                let modulo = si_modifica(stato) && stato.campi.iter().any(|c| c.compilabile());
                let modo = if modulo { ModoPdf::Compila } else { ModoPdf::Leggi };
                if modulo {
                    diagnosi(format!("pdf: modulo con {} campi, {} da compilare", stato.campi.len(), tascabile_pdf::da_compilare(&stato.campi)));
                }
                let protezione = motore.stato().protezione;
                if protezione.firmato() {
                    let certificato = if protezione.sola_lettura() { ", certificato senza modifiche" } else { "" };
                    diagnosi(format!("pdf: firmato{certificato}"));
                }
                self.pdf = Some(PdfAperto {
                    motore,
                    lettore,
                    nome: a.nome.clone(),
                    inizio: a.inizio,
                    trova: None,
                    vai: CampoTesto::solo_cifre(),
                    vai_sbagliato: false,
                    link_esterno: None,
                    chiave: a.chiave.clone(),
                    da: a.da,
                    impronta,
                    ricordata,
                    vista: None,
                    informazioni: None,
                    modo,
                    firme_accettate: false,
                    su_copia: false,
                    in_attesa_firme: None,
                    dopo_il_salvataggio: None,
                    salvato: false,
                    compila: Compila::default(),
                    copia: a.recupero.map(CopiaDelPdf::da_recupero).unwrap_or_default(),
                    modifica: Default::default(),
                    annota: Default::default(),
                    pagine: Default::default(),
                });
                self.vai(Schermata::Pdf);
                self.controlla_i_calcoli_pdf();
                match a.chiave {
                    Some(chiave) => self.home.ricorda(Recente { chiave, nome: a.nome, tipo: Tipo::Pdf, aperto_alle: adesso(), pagine: Some(pagine), impronta: None }),
                    // Una copia di recupero non è un file dell'utente: niente recenti.
                    None if a.da == Da::Recupero => {}
                    None => diagnosi("recenti: percorso che non è UTF-8, non ricordato"),
                }
            }
            Err(e) if e.chiede_password() && !impronta.is_empty() => {
                let sbagliata = e.genere == Genere::PasswordSbagliata;
                let come = if sbagliata { "password sbagliata, si chiede di nuovo" } else { "protetto da password, si chiede" };
                a.da.scrivi(format!("{etichetta}: {misura} byte, tipo pdf, {come}"));
                self.password_pdf = Some(Password {
                    byte: a.byte,
                    impronta,
                    da: a.da,
                    nome: a.nome,
                    chiave: a.chiave,
                    recupero: a.recupero,
                    campo: CampoTesto::segreto(),
                    visibile: false,
                    sbagliata,
                });
                self.fuoco_alla_password();
            }
            Err(e) => {
                a.da.scrivi(format!("{etichetta}: {misura} byte, tipo pdf, errore di apertura ({})", e.genere.diagnosi()));
                self.errore = Some(Errore::apertura(format!("Non riesco ad aprire il file: {e}")));
            }
        }
    }

    /// Per le prove a schermo: c'è un PDF nel lettore e aspetta ancora immagini dal pool (None
    /// senza PDF, o mentre si apre).
    #[doc(hidden)]
    pub fn pdf_in_attesa(&self) -> Option<bool> {
        self.pdf.as_ref().map(|p| p.lettore.in_attesa())
    }

    /// Per le prove a schermo: il testo selezionato nel lettore, se c'è.
    #[doc(hidden)]
    pub fn pdf_testo_selezionato(&self) -> Option<String> {
        self.pdf.as_ref().and_then(|p| p.lettore.testo_selezionato())
    }

    /// Per le prove a schermo: c'è il dialogo della password.
    #[doc(hidden)]
    pub fn chiede_la_password(&self) -> bool {
        self.password_pdf.is_some()
    }

    /// Chiude il PDF del lettore, se c'è, con le misure dei disegni nella diagnosi; con lui la
    /// ricerca, i fogli e il campo che aveva la tastiera.
    pub(crate) fn chiudi_pdf(&mut self) {
        self.menu_pdf = false;
        self.fogli_pdf.chiudi();
        if self.pdf.is_some() && self.fuoco == Fuoco::Campo && self.schermata == Schermata::Pdf {
            self.fuoco_al_documento();
            self.piattaforma.hide_keyboard();
        }
        self.ricorda_posizione_pdf(true);
        // Chiuso dopo Salva o «Non salvare»: la copia di recupero non serve più.
        self.chiudi_copia_pdf();
        if let Some(p) = self.pdf.take() {
            diagnosi(format!("pdf chiuso: {}", p.lettore.riga_misure()));
        }
    }

    /// Scrive dove si è arrivati nel PDF (punto e zoom) nel file delle posizioni: alla chiusura e
    /// quando l'app perde il fuoco (`subito`), o quando la vista è ferma da `RICORDA_DOPO` in un
    /// punto diverso da quello scritto l'ultima volta. Su Android dopo onPause non gira più
    /// nessun frame: per questo non si aspetta la chiusura. Restituisce quanto manca, se la
    /// vista si è mossa e la posizione aspetta: senza input egui non ridisegna, e chi chiama
    /// chiede un frame per allora.
    pub(crate) fn ricorda_posizione_pdf(&mut self, subito: bool) -> Option<Duration> {
        let p = self.pdf.as_mut()?;
        let (pagina, y) = p.lettore.posizione()?;
        let x = p.lettore.posizione_x().unwrap_or(0.5);
        let adesso_qui = (pagina, y, p.lettore.zoom(), x);
        match p.vista {
            Some((v, _)) if v == adesso_qui => {}
            _ => p.vista = Some((adesso_qui, Instant::now())),
        }
        if p.ricordata == Some(adesso_qui) {
            return None;
        }
        let ferma_da = p.vista.map_or(Duration::ZERO, |(_, dal)| dal.elapsed());
        if !subito && ferma_da < RICORDA_DOPO {
            return Some(RICORDA_DOPO - ferma_da);
        }
        let posizione = Posizione { chiave: p.impronta.clone(), pagina, y, x: Some(x), zoom: adesso_qui.2, alle: adesso() };
        let pagine = p.lettore.pagine();
        p.ricordata = Some(adesso_qui);
        self.scrivi_posizione(posizione, pagine);
        None
    }

    /// Scrive una posizione nel file delle posizioni su un thread suo (il file con `sync_all`
    /// sul thread dell'interfaccia poteva fermare un frame). Una alla volta: quella di prima, se
    /// non ha finito, si aspetta. La riga della diagnosi la scrive il thread dell'interfaccia
    /// quando la scrittura è finita (`passo_posizioni`).
    fn scrivi_posizione(&mut self, posizione: Posizione, pagine: usize) {
        self.aspetta_le_posizioni();
        let file = self.piattaforma.cartella_app().join(FILE_POSIZIONI);
        let pagina = posizione.pagina;
        let lavoro = move || {
            let mut posizioni = Posizioni::carica(&file);
            posizioni.ricorda(posizione);
            posizioni.salva(&file).map_err(|e| e.kind())
        };
        match std::thread::Builder::new().name("pdf-posizioni".into()).spawn(lavoro) {
            Ok(h) => self.scrittura_posizioni = Some((h, pagina, pagine)),
            Err(e) => diagnosi(format!("pdf: posizione non ricordata, thread non partito ({:?})", e.kind())),
        }
    }

    /// Per le prove: una posizione si sta scrivendo (o è scritta e la sua riga aspetta).
    #[doc(hidden)]
    pub fn posizioni_in_scrittura(&self) -> bool {
        self.scrittura_posizioni.is_some()
    }

    /// Aspetta la posizione che si sta scrivendo, con la sua riga nella diagnosi.
    pub(crate) fn aspetta_le_posizioni(&mut self) {
        let Some((h, pagina, pagine)) = self.scrittura_posizioni.take() else { return };
        match h.join() {
            Ok(Ok(())) => diagnosi_raffica("pdf-posizione", format!("pdf: posizione ricordata, pagina {} di {pagine}", pagina + 1)),
            Ok(Err(genere)) => diagnosi(format!("pdf: posizione non ricordata ({genere:?})")),
            Err(_) => diagnosi("pdf: posizione non ricordata (panico nel thread)"),
        }
    }

    /// A ogni passo: la posizione scritta, se il suo thread ha finito, va nella diagnosi.
    pub(crate) fn passo_posizioni(&mut self) {
        if self.scrittura_posizioni.as_ref().is_some_and(|(h, _, _)| h.is_finished()) {
            self.aspetta_le_posizioni();
        }
    }

    /// Indietro dal lettore: alla Home, chiedendo prima se ci sono modifiche da salvare (anche il
    /// campo che si sta scrivendo, che si conferma).
    pub(crate) fn esci_dal_pdf(&mut self) {
        self.conferma_scrittura_pdf(true);
        self.chiudi_poi(Dopo::Home);
    }

    /// La schermata del lettore, dentro la zona libera da barre di sistema: la barra alta (o la
    /// ricerca al suo posto), le pagine, la barra in fondo (non con la tastiera aperta) e il menu.
    pub(crate) fn mostra_pdf(&mut self, ui: &mut egui::Ui) {
        let tastiera = self.piattaforma.keyboard_height() > 0.0;
        let Some(p) = self.pdf.as_mut() else {
            self.vai(Schermata::Home);
            return;
        };
        let zona = ui.max_rect();
        let barra = Rect::from_min_size(zona.min, vec2(zona.width(), tema::BARRA_ALTA));
        let mut azione = None;
        let mut indietro = false;
        // «Organizza pagine» copre tutto: le barre del lettore non servono (e non si toccano).
        let organizza = p.pagine.organizza.is_some();
        if p.trova.is_none() && !organizza {
            let nome = home::nome_da_mostrare(&p.nome, Tipo::Pdf).to_string();
            let stato = p.motore.stato();
            // Annulla e Ripeti quando il documento ha modifiche da annullare o da ripetere,
            // accesi o spenti; lo stato al posto di «PDF · N pagine».
            let storia = stato.annulla || stato.ripeti;
            let (annulla, ripeti) = (stato.annulla, stato.ripeti);
            let ripeti_nella_barra = storia && zona.width() >= RIPETI_NEL_MENU_SOTTO;
            // Un modulo dice quanti campi restano da compilare (bozzetto 7).
            let campi = stato.campi.iter().any(|c| c.compilabile()).then(|| da_compilare_detto(tascabile_pdf::da_compilare(&stato.campi)));
            let (sotto, salvato) = if stato.modificato || p.copia.recuperato {
                // Prima quanti campi restano: se non ci sta tutto, si taglia la fine.
                match campi.filter(|_| p.modo == ModoPdf::Compila) {
                    Some(c) => (
                        format!(
                            "{} · non salvato",
                            c.replace(" campi da compilare", " da compilare").replace(" campo da compilare", " da compilare")
                        ),
                        false,
                    ),
                    None => (MODIFICHE_NON_SALVATE.to_string(), false),
                }
            } else if p.salvato {
                let dove = p.chiave.as_deref().map(|c| self.home.dove(c)).unwrap_or_default();
                (format!("Salvato · {dove}"), true)
            } else {
                // Le firme ancora valide si dicono: modificarlo le renderebbe non valide.
                let firme = if stato.protezione.sola_lettura() {
                    " · solo lettura"
                } else if stato.protezione.firmato() {
                    " · firmato"
                } else {
                    ""
                };
                let campi = campi.map(|c| format!(" · {c}")).unwrap_or_default();
                (format!("{}{firme}{campi}", sottotitolo(p.lettore.pagine())), false)
            };
            let r = ui
                .scope_builder(UiBuilder::new().max_rect(barra), |ui| {
                    let mut b = BarraAlta::new(&nome).indietro(INDIETRO).sottotitolo(&sotto);
                    if salvato {
                        b = b.stato(icone::CHECK_CIRCLE, tema::VERDE);
                    }
                    if storia {
                        b = b.azione(PulsanteIcona::new(icone::UNDO, ANNULLA_MODIFICA).spento(!annulla));
                    }
                    if ripeti_nella_barra {
                        b = b.azione(PulsanteIcona::new(icone::REDO, RIPETI_MODIFICA).spento(!ripeti));
                    }
                    b.azione(PulsanteIcona::new(icone::SEARCH, CERCA)).azione(PulsanteIcona::new(icone::MORE_VERT, MENU)).mostra(ui)
                })
                .inner;
            indietro = r.indietro;
            let primo = usize::from(storia) + usize::from(ripeti_nella_barra);
            azione = match r.azione {
                Some(0) if storia => Some(Azione::Annulla),
                Some(1) if ripeti_nella_barra => Some(Azione::Ripeti),
                Some(i) if i == primo => Some(Azione::Cerca),
                Some(_) => Some(Azione::Menu),
                None => None,
            };
        }
        // In fondo la barra dei modi; mentre si scrive un campo, la barra della scrittura (sulla
        // tastiera se è aperta).
        let scrive = p.compila.scrittura.is_some();
        let voci = (!tastiera && !scrive && !organizza).then(|| BarraVoci::posto(zona));
        // Un testo della pagina nel modo Modifica si scrive lì dov'è, senza la barra.
        let sulla_pagina = crate::pdf_modifica::posto_sulla_pagina(p, ui.ctx(), zona).is_some();
        let barra_scrittura = (scrive && !sulla_pagina).then(|| Self::posto_barra_scrittura(zona, tastiera));
        let sopra = voci.or(barra_scrittura).map_or(0.0, |b| zona.bottom() - b.top() + SOPRA_LA_BARRA);
        // Sopra la barra dei modi la striscia degli attrezzi del modo Annota (o delle forme del modo
        // Modifica): le pagine salgono anche sopra di lei.
        let striscia = match p.modo {
            _ if voci.is_none() => 0.0,
            ModoPdf::Annota => p.annota.altezza_striscia(),
            ModoPdf::Modifica => p.modifica.altezza_striscia(),
            _ => 0.0,
        };
        p.lettore.spazio_sotto(sopra + striscia);
        // Nel modo Annota la barra della selezione segna il testo; la penna disegna invece di
        // scorrere; nel modo Modifica un link si sceglie invece di seguirlo.
        p.lettore.azioni_della_selezione((p.modo == ModoPdf::Annota).then(|| crate::pdf_annota::azioni_marca(p.annota.colore_marca)));
        p.lettore.dito_scorre(!p.annota.disegna(p.modo) && !p.modifica.disegna(p.modo));
        p.lettore.segue_link(p.modo != ModoPdf::Modifica);
        let pagine = Rect::from_min_max(pos2(zona.left(), barra.bottom()), zona.max);
        let compila = p.modo == ModoPdf::Compila;
        let esito = ui.scope_builder(UiBuilder::new().max_rect(pagine), |ui| p.lettore.mostra(ui)).inner;
        scrivi_esito_del_lettore(&esito, p.inizio);
        // Il testo copiato negli appunti, segnato come sensibile (nella diagnosi quanti caratteri).
        if let Some(testo) = &esito.copia {
            self.piattaforma.copia_testo(testo);
        }
        if esito.senza_testo {
            self.avviso_breve.avvisa(SENZA_TESTO);
        }
        if let Some(resta) = self.ricorda_posizione_pdf(false) {
            ui.ctx().request_repaint_after(resta);
        }
        let modo = self.pdf.as_ref().map(|p| p.modo);
        if compila {
            self.mostra_compila(ui, pagine, sopra, esito.tocco);
        } else if modo == Some(ModoPdf::Modifica) {
            self.mostra_modifica(ui, pagine, sopra, esito.tocco);
        } else if modo == Some(ModoPdf::Annota) {
            self.mostra_annota(ui, pagine, sopra, esito.tocco, esito.scelta_selezione.clone());
        }
        self.tieni_in_vista_pdf();
        if let Some(b) = barra_scrittura {
            self.mostra_barra_scrittura(ui, b);
        }
        if let Some(b) = voci
            && let Some(p) = self.pdf.as_ref()
        {
            // La barra dei modi del bozzetto 7: i modi che ci sono (gli altri arrivano coi passaggi
            // dopo), quello acceso in rosso tenue, e Strumenti.
            let modi = modi_disponibili(p.motore.stato());
            let acceso = modi.iter().position(|m| *m == p.modo);
            let mut barra = BarraVoci::new().accesa(acceso);
            for m in &modi {
                barra = barra.voce(m.icona(), m.nome());
            }
            let scelta = barra.voce(icone::APPS, STRUMENTI).mostra(ui, b);
            azione = match scelta {
                Some(i) if i < modi.len() => Some(Azione::Modo(modi[i])),
                Some(_) => Some(Azione::Strumenti),
                None => azione,
            };
        }
        if self.pdf.as_ref().is_some_and(|p| p.trova.is_some()) {
            ui.scope_builder(UiBuilder::new().max_rect(barra), |ui| self.barra_cerca_pdf(ui));
        }
        if let Some(d) = esito.link {
            self.link_toccato(d);
        }
        // Un'azione della barra alta o un altro modo: prima il campo che si scrive va al motore.
        if azione.is_some() {
            self.conferma_scrittura_pdf(true);
        }
        match azione {
            Some(Azione::Annulla) => self.annulla_pdf(),
            Some(Azione::Ripeti) => self.ripeti_pdf(),
            Some(Azione::Cerca) => self.apri_ricerca_pdf(),
            Some(Azione::Menu) => {
                diagnosi("menu del pdf: aperto");
                self.menu_pdf = true;
                self.ripeti_nel_menu_pdf = zona.width() < RIPETI_NEL_MENU_SOTTO && self.pdf.as_ref().is_some_and(|p| p.motore.stato().ripeti);
            }
            Some(Azione::Modo(m)) => self.scegli_modo_pdf(m),
            Some(Azione::Strumenti) => self.apri_strumenti_pdf(),
            None => {}
        }
        self.mostra_menu_pdf(ui.ctx(), zona);
        if indietro {
            self.esci_dal_pdf();
        }
    }

    /// La ricerca al posto della barra alta, alta uguale, come Trova di Word: il campo a pillola
    /// con la lente e il conteggio, che col tocco prende la tastiera; il risultato prima e quello
    /// dopo (spenti senza risultati); la X che chiude.
    fn barra_cerca_pdf(&mut self, ui: &mut egui::Ui) {
        let Some(p) = self.pdf.as_ref() else { return };
        let Some(campo) = p.trova.as_ref() else { return };
        let conto = p.lettore.conteggio();
        let conteggio = conteggio_pdf(conto, campo.testo().is_empty());
        let ci_sono = conto.is_some_and(|c| c.totale > 0);
        let barra = ui.max_rect();
        ui.painter().rect_filled(barra, 0.0, tema::CARTA);
        let posto =
            |destra: f32| Rect::from_min_size(pos2(destra - tema::PULSANTE, barra.center().y - tema::PULSANTE / 2.0), Vec2::splat(tema::PULSANTE));
        let chiudi = posto(barra.right() - 4.0);
        let dopo = posto(chiudi.left());
        let prima = posto(dopo.left());
        let chiudi = ui.put(chiudi, PulsanteIcona::new(icone::CLOSE, CHIUDI_CERCA)).clicked();
        let dopo = ui.put(dopo, PulsanteIcona::new(icone::KEYBOARD_ARROW_DOWN, DOPO).spento(!ci_sono)).clicked();
        let prima_r = ui.put(prima, PulsanteIcona::new(icone::KEYBOARD_ARROW_UP, PRIMA).spento(!ci_sono));
        let dentro = Rect::from_min_max(
            pos2(barra.left() + CERCA_DA_SINISTRA, barra.center().y - tema::PULSANTE / 2.0),
            pos2(prima_r.rect.left() - 4.0, barra.center().y + tema::PULSANTE / 2.0),
        );
        let tocco_campo = ui.scope_builder(UiBuilder::new().max_rect(dentro), |ui| campo.mostra_trova(ui, VUOTO_CERCA, &conteggio).clicked()).inner;
        if tocco_campo {
            self.fuoco_al_campo_pdf();
        }
        if prima_r.clicked() {
            self.risultato_pdf(false);
        }
        if dopo {
            self.risultato_pdf(true);
        }
        if chiudi {
            self.chiudi_ricerca_pdf();
        }
    }

    /// La lente: la ricerca al posto della barra alta, col campo che prende la tastiera.
    pub(crate) fn apri_ricerca_pdf(&mut self) {
        let Some(p) = self.pdf.as_mut() else { return };
        if p.trova.is_none() {
            diagnosi("pdf: ricerca aperta");
            p.trova = Some(CampoTesto::new());
        }
        self.fuoco_al_campo_pdf();
    }

    /// La X della ricerca (o Indietro): via la barra, i risultati e la tastiera.
    pub(crate) fn chiudi_ricerca_pdf(&mut self) {
        if self.pdf.as_ref().is_none_or(|p| p.trova.is_none()) {
            return;
        }
        let campo = self.fuoco == Fuoco::Campo;
        self.fuoco_al_documento();
        if let Some(p) = self.pdf.as_mut() {
            p.trova = None;
            p.lettore.chiudi_ricerca();
        }
        if campo && self.piattaforma.keyboard_visible() {
            self.piattaforma.hide_keyboard();
        }
        diagnosi("pdf: ricerca chiusa");
    }

    /// Il campo che riceve la tastiera nel lettore: quello di «Vai a pagina» col suo foglio
    /// aperto, se no il campo del modulo che si scrive, se no quello della ricerca.
    pub(crate) fn campo_pdf_mut(&mut self) -> Option<&mut CampoTesto> {
        let vai = self.fogli_pdf.vai.aperto();
        let pagine = self.campo_pagine_aperto();
        let p = self.pdf.as_mut()?;
        if pagine {
            return Some(&mut p.pagine.campo);
        }
        if vai {
            return Some(&mut p.vai);
        }
        match p.compila.scrittura.as_mut() {
            Some(s) => Some(&mut s.campo),
            None => p.trova.as_mut(),
        }
    }

    /// Il campo del lettore prende la tastiera, con una versione più nuova di tutte quelle date
    /// prima; i documenti che si apriranno continuano da lì. «Vai a pagina» vuole la tastiera dei
    /// numeri, la ricerca quella del testo.
    pub(crate) fn fuoco_al_campo_pdf(&mut self) {
        let dopo = self.versioni.versione();
        let segreto = self.pdf.as_ref().and_then(|p| p.compila.scrittura.as_ref()).is_some_and(|s| s.campo.e_segreto());
        let pagine = self.campo_pagine_aperto().then(|| self.pdf.as_ref().map(|p| (p.pagine.campo.e_segreto(), p.pagine.ogni))).flatten();
        let tipo = match () {
            _ if pagine.is_some_and(|(segreto, _)| segreto) => TipoTastiera::Password,
            _ if pagine.is_some_and(|(_, ogni)| ogni) => TipoTastiera::Numeri,
            _ if pagine.is_some() => TipoTastiera::Testo,
            _ if self.fogli_pdf.vai.aperto() => TipoTastiera::Numeri,
            _ if segreto => TipoTastiera::Password,
            _ => TipoTastiera::Testo,
        };
        let Some(c) = self.campo_pdf_mut() else { return };
        let st = c.prendi_fuoco(dopo);
        self.versioni.continua_dalla_versione(st.versione);
        if self.fuoco != Fuoco::Campo {
            diagnosi(format!("fuoco: campo del pdf, finestra v{}", st.versione));
        }
        self.fuoco = Fuoco::Campo;
        self.piattaforma.tipo_tastiera(tipo);
        self.piattaforma.set_ime_state(st.testo, st.sel.0, st.sel.1, st.versione);
        self.piattaforma.show_keyboard();
    }

    /// Il testo del campo è cambiato: la ricerca riparte (o, in «Vai a pagina», sparisce l'avviso
    /// del numero sbagliato).
    pub(crate) fn campo_pdf_cambiato(&mut self) {
        let vai = self.fogli_pdf.vai.aperto();
        let pagine = self.campo_pagine_aperto();
        let Some(p) = self.pdf.as_mut() else { return };
        if pagine {
            p.pagine.sbagliato = false;
            return;
        }
        if vai {
            p.vai_sbagliato = false;
            return;
        }
        // Il campo di un modulo: il valore va al motore solo alla conferma.
        if p.compila.scrittura.is_some() {
            return;
        }
        let Some(testo) = p.trova.as_ref().map(|c| c.testo().to_string()) else { return };
        p.lettore.cerca(&testo);
        diagnosi_raffica("pdf-ricerca", format!("pdf: ricerca di {} caratteri", testo.chars().count()));
    }

    /// Invio nel campo: in «Vai a pagina» si va, in un campo del modulo il campo dopo, nella
    /// ricerca il risultato dopo.
    pub(crate) fn invio_nel_pdf(&mut self) {
        if self.campo_pagine_aperto() {
            self.invio_pagine_pdf();
        } else if self.fogli_pdf.vai.aperto() {
            self.vai_al_numero();
        } else if self.pdf.as_ref().is_some_and(|p| p.compila.scrittura.is_some()) {
            self.campo_vicino_pdf(true);
        } else {
            self.risultato_pdf(true);
        }
    }

    fn risultato_pdf(&mut self, avanti: bool) {
        if let Some(p) = self.pdf.as_mut() {
            p.lettore.risultato_altro(avanti);
        }
    }

    /// Un link toccato: quelli a una pagina il lettore li ha già seguiti; uno esterno si apre solo
    /// dopo la conferma, e solo se è una pagina web o una mail; gli altri non si aprono.
    fn link_toccato(&mut self, d: Destinazione) {
        match d {
            Destinazione::Pagina { pagina, .. } => diagnosi(format!("pdf: link alla pagina {}", pagina + 1)),
            Destinazione::Indirizzo(indirizzo) => match genere_indirizzo(&indirizzo) {
                Some(genere) => {
                    diagnosi(format!("pdf: link esterno toccato ({genere}), si chiede"));
                    if let Some(p) = self.pdf.as_mut() {
                        p.link_esterno = Some(indirizzo);
                    }
                }
                None => {
                    diagnosi("pdf: link esterno che non si apre");
                    self.errore = Some(Errore {
                        titolo: LINK_CHE_NON_SI_APRE,
                        testo: "Questo link porta a un indirizzo che Tascabile non apre: solo pagine web e indirizzi di posta.".to_string(),
                    });
                }
            },
            Destinazione::Altro => {
                diagnosi("pdf: link a un altro file o a un'azione, non seguito");
                self.avviso_breve.avvisa("Questo link porta fuori dal PDF: in Tascabile non si apre.");
            }
        }
    }

    /// Il dialogo di un link esterno: l'indirizzo e dove si apre; «Apri» lo manda al browser.
    pub(crate) fn mostra_link_esterno(&mut self, ctx: &egui::Context) {
        let Some(indirizzo) = self.pdf.as_ref().and_then(|p| p.link_esterno.clone()) else { return };
        let testo = format!("{}\n\nSi apre fuori da Tascabile, nel browser o nell'app della posta.", accorciato(&indirizzo));
        let risposta = Dialogo::new(egui::Id::new("link_esterno"), APRIRE_IL_LINK)
            .testo(&testo)
            .pulsante(ANNULLA, Peso::Testo)
            .pulsante(APRI, Peso::Principale)
            .mostra(ctx);
        let Some(risposta) = risposta else { return };
        if let Some(p) = self.pdf.as_mut() {
            p.link_esterno = None;
        }
        match risposta {
            RispostaDialogo::Pulsante(1) => self.piattaforma.apri_indirizzo(&indirizzo),
            _ => diagnosi("pdf: link esterno non aperto"),
        }
    }

    /// Il menu del lettore, come quello dell'Editor: Salva, Salva una copia, Condividi, Esporta in
    /// Word, Copia definitiva, il timestamp; Miniature, Vai a pagina, Indice; Informazioni sul
    /// documento. Su un telefono stretto, in cima, Ripeti quando all'apertura c'era da ripetere.
    fn mostra_menu_pdf(&mut self, ctx: &egui::Context, zona: Rect) {
        if !self.menu_pdf {
            return;
        }
        let ripeti = self.ripeti_nel_menu_pdf;
        let mut menu = Menu::new(egui::Id::new("menu_pdf"));
        if ripeti {
            menu = menu.voce(icone::REDO, RIPETI_MODIFICA).separatore();
        }
        // I tre salvataggi insieme, uno sotto l'altro: Salva, la copia che si può ancora
        // modificare e, in rosso, quella che non si modifica più. Sotto un filo, mandare ed
        // esportare. («Salva pulito» non c'è più dalla 0.9.10: Salva riscrive già tutto il file.)
        let data = self.voce_della_data();
        let scelta = menu
            .primo(icone::SAVE, SALVA)
            .voce_spiegata(icone::FILE_COPY, SALVA_COPIA, SPIEGA_SALVA_COPIA)
            .voce_rossa(icone::PICTURE_AS_PDF, COPIA_DEFINITIVA, SPIEGA_COPIA_DEFINITIVA)
            .separatore()
            .voce(icone::SHARE, CONDIVIDI)
            .voce(icone::DESCRIPTION, crate::pdf_ocr::ESPORTA_WORD)
            .voce(icone::SCHEDULE, data)
            .separatore()
            .voce(icone::GRID_VIEW, MINIATURE)
            .voce(icone::NUMBERS, VAI_A_PAGINA)
            .voce(icone::TOC, INDICE)
            .separatore()
            .voce(icone::INFO, INFORMAZIONI)
            .mostra(ctx, pos2(zona.right() - MENU_DESTRA, zona.top() + MENU_SOTTO));
        let Some(scelta) = scelta else { return };
        self.menu_pdf = false;
        // Le voci dopo Ripeti vengono una dopo, se c'è.
        let scelta = match scelta {
            SceltaMenu::Voce(0) if ripeti => {
                self.ripeti_pdf();
                return;
            }
            SceltaMenu::Voce(i) if ripeti => SceltaMenu::Voce(i - 1),
            s => s,
        };
        match scelta {
            SceltaMenu::Voce(0) => self.salva_pdf(AzioneSalva::Salva, None),
            SceltaMenu::Voce(1) => self.salva_pdf(AzioneSalva::SalvaCopia, None),
            SceltaMenu::Voce(2) => self.apri_copia_definitiva(),
            SceltaMenu::Voce(3) => self.condividi_pdf_aperto(),
            SceltaMenu::Voce(4) => self.esporta_word_pdf(ctx),
            SceltaMenu::Voce(5) => self.data_dal_menu(ctx),
            SceltaMenu::Voce(6) => self.apri_miniature(),
            SceltaMenu::Voce(7) => self.apri_vai_a_pagina(),
            SceltaMenu::Voce(8) => self.apri_indice_pdf(),
            SceltaMenu::Voce(9) => self.apri_informazioni_pdf(),
            _ => {}
        }
    }

    /// «Copia definitiva» dal menu: prima il foglio che spiega.
    fn apri_copia_definitiva(&mut self) {
        if self.oscura_da_applicare() {
            return;
        }
        diagnosi("pdf: copia definitiva spiegata");
        self.fogli_pdf.definitiva.apri();
    }

    /// Il foglio della copia definitiva: che cosa fa (e che le firme digitali nella copia non
    /// valgono più), e il pulsante che la crea.
    fn mostra_foglio_definitiva(&mut self, ctx: &egui::Context) {
        let firmato = self.pdf.as_ref().is_some_and(|p| p.motore.stato().protezione.firmato());
        let crea = self
            .fogli_pdf
            .definitiva
            .mostra(ctx, COPIA_DEFINITIVA, |ui| {
                let font = Carattere::Testo.font(ui.ctx(), 16.0);
                for (i, p) in SPIEGA_DEFINITIVA.iter().enumerate() {
                    if i > 0 {
                        ui.add_space(10.0);
                    }
                    // Per primo, in rosso, quello che nella copia non si modifica più.
                    ui.label(RichText::new(*p).font(font.clone()).color(if i == 0 { tema::ROSSO } else { tema::INCHIOSTRO }));
                }
                if firmato {
                    ui.add_space(10.0);
                    ui.label(RichText::new(FIRME_NELLA_DEFINITIVA).font(font.clone()).color(tema::ROSSO));
                }
                ui.add_space(16.0);
                let crea = ui.add(Pulsante::new(CREA_DEFINITIVA, Peso::Principale).icona(icone::TASK_ALT)).clicked();
                ui.add_space(8.0);
                crea
            })
            .unwrap_or(false);
        if crea {
            self.fogli_pdf.definitiva.chiudi();
            self.salva_pdf(AzioneSalva::Definitiva, None);
        }
    }

    /// «Informazioni sul documento»: pagine, misura, titolo e autore, firme e cartella, prese
    /// adesso. Nella diagnosi solo i numeri.
    fn apri_informazioni_pdf(&mut self) {
        let Some(p) = self.pdf.as_ref() else { return };
        let info = p.motore.stato().informazioni.clone();
        let cartella = match p.chiave.as_deref() {
            Some(c) if home::e_uri(c) => format!("Arrivato {}, non è salvato nel telefono", self.home.dove(c)),
            Some(c) => self.home.dove(c),
            None => "Non si sa".to_string(),
        };
        diagnosi(format!("pdf: informazioni, {} pagine, {} byte, {} firme", info.pagine, info.byte, info.firme.firmate));
        if let Some(p) = self.pdf.as_mut() {
            p.informazioni = Some((info, cartella));
        }
        self.fogli_pdf.info.apri();
    }

    /// Il foglio «Informazioni»: una riga per cosa, titolo e autore solo se il PDF li dice.
    fn mostra_foglio_info_pdf(&mut self, ctx: &egui::Context) {
        let a_mano = self.pdf.as_ref().map_or(0, |p| p.motore.stato().segni.iter().filter(|s| s.genere == tascabile_pdf::GenereSegno::Firma).count());
        let righe = righe_informazioni(self.pdf.as_ref().and_then(|p| p.informazioni.as_ref()), a_mano);
        self.fogli_pdf.info.mostra(ctx, TITOLO_INFORMAZIONI, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for (i, (etichetta, valore)) in righe.iter().enumerate() {
                riga_informazione(ui, etichetta, valore, i + 1 == righe.len());
            }
        });
    }

    /// Il campo della password prende la tastiera (quella delle password: niente suggerimenti
    /// né maiuscole), con una versione più nuova di tutte quelle date prima.
    pub(crate) fn fuoco_alla_password(&mut self) {
        let dopo = self.versioni.versione();
        let Some(p) = self.password_pdf.as_mut() else { return };
        let st = p.campo.prendi_fuoco(dopo);
        self.versioni.continua_dalla_versione(st.versione);
        if self.fuoco != Fuoco::Campo {
            diagnosi(format!("fuoco: campo della password, finestra v{}", st.versione));
        }
        self.fuoco = Fuoco::Campo;
        self.piattaforma.tipo_tastiera(TipoTastiera::Password);
        self.piattaforma.set_ime_state(st.testo, st.sel.0, st.sel.1, st.versione);
        self.piattaforma.show_keyboard();
    }

    /// Il campo della password si svuota, e con lui il testo che la tastiera tiene
    /// (GameTextInput lo terrebbe finché un altro campo non le dà il suo).
    fn svuota_la_password(&mut self) {
        let Some(st) = self.password_pdf.as_mut().and_then(|p| p.campo.svuota()) else { return };
        self.versioni.continua_dalla_versione(st.versione);
        self.piattaforma.set_ime_state(st.testo, st.sel.0, st.sel.1, st.versione);
    }

    /// «Apri» (o Invio) nel dialogo della password: il PDF si riapre con lei, dai byte già letti.
    /// Il campo si svuota; la password va al thread dell'apertura e finisce lì.
    pub(crate) fn apri_con_la_password(&mut self) {
        let Some(p) = self.password_pdf.as_ref() else { return };
        if p.campo.testo().is_empty() {
            return;
        }
        let password = p.campo.testo().to_string();
        self.svuota_la_password();
        let campo = self.fuoco == Fuoco::Campo;
        self.fuoco_al_documento();
        let Some(p) = self.password_pdf.take() else { return };
        if campo {
            self.piattaforma.hide_keyboard();
        }
        diagnosi("pdf: password data, si riapre");
        self.avvia_pdf(p.da, p.byte, p.nome, p.chiave, Instant::now(), Some(password), Some(p.impronta), p.recupero);
    }

    /// Annulla (o Indietro) nel dialogo della password: il PDF non si apre.
    pub(crate) fn annulla_password(&mut self) {
        if self.password_pdf.is_none() {
            return;
        }
        self.svuota_la_password();
        let campo = self.fuoco == Fuoco::Campo;
        self.fuoco_al_documento();
        self.password_pdf = None;
        if campo && self.piattaforma.keyboard_visible() {
            self.piattaforma.hide_keyboard();
        }
        diagnosi("pdf: password non data, il PDF non si apre");
    }

    /// Il dialogo della password, sopra la schermata da cui si apriva il PDF: il nome, il campo
    /// coi punti e l'occhio, «Password sbagliata» in rosso se l'ultima non andava, Annulla e Apri.
    /// Un tocco fuori non lo chiude: si perderebbe quello che si è scritto.
    pub(crate) fn mostra_password_pdf(&mut self, ctx: &egui::Context) {
        let Some(p) = self.password_pdf.as_ref() else { return };
        let testo = format!("«{}» si apre con una password.", home::nome_da_mostrare(&p.nome, Tipo::Pdf));
        let (risposta, tocchi) = Dialogo::new(egui::Id::new("password_pdf"), PDF_PROTETTO)
            .testo(&testo)
            .pulsante(ANNULLA, Peso::Testo)
            .pulsante(APRI, Peso::Principale)
            .mostra_con(ctx, |ui| {
                let r = p.campo.mostra_segreto(ui, VUOTO_PASSWORD, p.visibile);
                if p.sbagliata {
                    ui.add_space(8.0);
                    ui.label(RichText::new(PASSWORD_SBAGLIATA).font(Carattere::Grassetto.font(ui.ctx(), 15.0)).color(tema::ROSSO));
                }
                // Con gli 8 del dialogo, 20 dp prima dei pulsanti.
                ui.add_space(12.0);
                (r.campo.clicked(), r.occhio)
            });
        let (campo, occhio) = tocchi.unwrap_or_default();
        if occhio && let Some(p) = self.password_pdf.as_mut() {
            p.visibile = !p.visibile;
        }
        if campo {
            self.fuoco_alla_password();
        }
        match risposta {
            Some(RispostaDialogo::Pulsante(1)) => self.apri_con_la_password(),
            Some(RispostaDialogo::Pulsante(_)) => self.annulla_password(),
            Some(RispostaDialogo::Fuori) | None => {}
        }
    }

    pub(crate) fn apri_miniature(&mut self) {
        let Some(p) = self.pdf.as_mut() else { return };
        p.lettore.apri_miniature();
        diagnosi(format!("pdf: miniature aperte, {} pagine", p.lettore.pagine()));
        self.fogli_pdf.miniature.apri();
    }

    pub(crate) fn apri_indice_pdf(&mut self) {
        let Some(p) = self.pdf.as_ref() else { return };
        let stato = p.motore.stato();
        let altri = if stato.altri_segnalibri { ", altri non letti" } else { "" };
        diagnosi(format!("pdf: indice aperto, {} segnalibri{altri}", stato.segnalibri.len()));
        self.fogli_pdf.indice.apri();
    }

    /// «Vai a pagina»: il foglio col campo vuoto, che prende la tastiera.
    pub(crate) fn apri_vai_a_pagina(&mut self) {
        // Prima lascia la tastiera la ricerca, se l'aveva.
        self.fuoco_al_documento();
        let Some(p) = self.pdf.as_mut() else { return };
        p.vai = CampoTesto::solo_cifre();
        p.vai_sbagliato = false;
        diagnosi("pdf: vai a pagina aperto");
        self.fogli_pdf.vai.apri();
        self.fogli_pdf.vai_prima = true;
        self.fuoco_al_campo_pdf();
    }

    /// «Vai» col numero del campo: la pagina, se c'è; se no l'avviso sotto il campo.
    fn vai_al_numero(&mut self) {
        let Some(p) = self.pdf.as_mut() else { return };
        let n = p.lettore.pagine();
        match p.vai.testo().parse::<usize>() {
            Ok(numero) if (1..=n).contains(&numero) => {
                p.lettore.vai_alla_pagina(numero - 1);
                diagnosi(format!("pdf: vai alla pagina {numero} di {n}"));
                self.chiudi_vai_a_pagina();
            }
            _ => {
                p.vai_sbagliato = true;
                diagnosi("pdf: vai a pagina, numero che non va");
            }
        }
    }

    /// Il foglio di «Vai a pagina» si chiude: il campo lascia la tastiera.
    fn chiudi_vai_a_pagina(&mut self) {
        let campo = self.fuoco == Fuoco::Campo;
        self.fuoco_al_documento();
        self.fogli_pdf.vai.chiudi();
        self.fogli_pdf.vai_prima = false;
        if campo && self.piattaforma.keyboard_visible() {
            self.piattaforma.hide_keyboard();
        }
    }

    /// I fogli del lettore: miniature, indice e «Vai a pagina». Vanno chiamati a ogni frame.
    pub(crate) fn mostra_fogli_pdf(&mut self, ctx: &egui::Context) {
        if self.schermata != Schermata::Pdf || self.pdf.is_none() {
            self.fogli_pdf.chiudi();
        }
        // Le miniature: un tocco porta alla pagina e chiude il foglio.
        let scelta = match self.pdf.as_mut() {
            Some(p) => self.fogli_pdf.miniature.mostra(ctx, MINIATURE, |ui| p.lettore.mostra_miniature(ui)).flatten(),
            None => self.fogli_pdf.miniature.mostra(ctx, MINIATURE, |_| None).flatten(),
        };
        if let Some(pagina) = scelta
            && let Some(p) = self.pdf.as_mut()
        {
            p.lettore.vai_alla_pagina(pagina);
            diagnosi(format!("pdf: miniatura, salto alla pagina {}", pagina + 1));
            self.fogli_pdf.miniature.chiudi();
        }
        // L'indice: i segnalibri rientrati per livello, col numero della pagina a destra.
        let vuoto = Vec::new();
        let (segnalibri, altri) = self.pdf.as_ref().map_or((&vuoto, false), |p| (&p.motore.stato().segnalibri, p.motore.stato().altri_segnalibri));
        let scelto = self
            .fogli_pdf
            .indice
            .mostra(ctx, INDICE, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                if segnalibri.is_empty() {
                    indice_vuoto(ui);
                }
                let mut scelto = None;
                for (i, s) in segnalibri.iter().enumerate() {
                    if riga_segnalibro(ui, s, i + 1 == segnalibri.len()).clicked() {
                        scelto = Some(i);
                    }
                }
                if altri {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("Il PDF ha altri segnalibri, che qui non compaiono.")
                            .font(Carattere::Testo.font(ctx, 14.0))
                            .color(tema::MATITA),
                    );
                }
                scelto
            })
            .flatten();
        if let Some(i) = scelto
            && let Some(p) = self.pdf.as_mut()
            && let Some(s) = p.motore.stato().segnalibri.get(i).cloned()
        {
            if let Some(pagina) = s.pagina {
                p.lettore.vai_al_punto(pagina, s.y);
                diagnosi(format!("pdf: indice, salto alla pagina {}", pagina + 1));
            } else {
                diagnosi("pdf: indice, segnalibro senza pagina");
            }
            self.fogli_pdf.indice.chiudi();
        }
        self.mostra_foglio_vai(ctx);
        self.mostra_foglio_info_pdf(ctx);
        self.mostra_foglio_definitiva(ctx);
        self.mostra_foglio_strumenti_pdf(ctx);
        self.mostra_fogli_compila(ctx);
        self.mostra_fogli_firma(ctx);
        self.mostra_fogli_modifica(ctx);
        self.mostra_fogli_annota(ctx);
        self.mostra_fogli_pagine(ctx);
    }

    /// Il foglio di «Vai a pagina»: il campo del numero e «Vai», sotto quante pagine ci sono (o, in
    /// rosso, che il numero non va).
    fn mostra_foglio_vai(&mut self, ctx: &egui::Context) {
        let pagine = self.pdf.as_ref().map_or(0, |p| p.lettore.pagine());
        let (mut tocco_campo, mut vai) = (false, false);
        match self.pdf.as_ref() {
            Some(p) => {
                let sotto = if p.vai_sbagliato {
                    format!("Scrivi un numero da 1 a {pagine}.")
                } else {
                    format!("Il PDF ha {}.", sottotitolo(pagine).replace("PDF · ", ""))
                };
                let colore = if p.vai_sbagliato { tema::ROSSO } else { tema::MATITA };
                self.fogli_pdf.vai.mostra(ctx, VAI_A_PAGINA, |ui| {
                    ui.spacing_mut().item_spacing = vec2(tema::SPAZIO, 0.0);
                    ui.horizontal(|ui| {
                        let largo = (ui.available_width() - 96.0).max(48.0);
                        tocco_campo = ui.allocate_ui(vec2(largo, tema::PULSANTE), |ui| p.vai.mostra(ui, "Numero della pagina").clicked()).inner;
                        vai = ui.add(Pulsante::new(VAI, Peso::Principale).larghezza(88.0)).clicked();
                    });
                    ui.add_space(10.0);
                    ui.label(RichText::new(sotto).font(Carattere::Testo.font(ctx, 15.0)).color(colore));
                    ui.add_space(6.0);
                });
            }
            None => {
                self.fogli_pdf.vai.mostra(ctx, VAI_A_PAGINA, |_| ());
            }
        }
        if tocco_campo {
            self.fuoco_al_campo_pdf();
        }
        if vai {
            self.vai_al_numero();
        }
        // Chiuso toccando fuori o con la X: il campo lascia la tastiera.
        if self.fogli_pdf.vai_prima && !self.fogli_pdf.vai.aperto() {
            self.chiudi_vai_a_pagina();
        }
    }

    /// «Condividi»: il PDF com'è adesso, con le modifiche anche non salvate, in `condivisi/` e al
    /// foglio di sistema, col suo nome (in .pdf anche se il file aveva un altro nome). I byte li
    /// prepara il motore sul suo thread; nella diagnosi esito e misura.
    pub(crate) fn condividi_pdf_aperto(&mut self) {
        self.salva_pdf(AzioneSalva::Condividi, None);
    }

    /// Mentre il PDF si apre, un velo trasparente ferma i tocchi sullo schermo di prima; se
    /// l'apertura dura più di un attimo, «Apro il PDF…» in una pillola scura in mezzo.
    pub(crate) fn mostra_apertura_pdf(&self, ctx: &egui::Context) {
        let Some(a) = &self.apertura_pdf else { return };
        let zona = ctx.content_rect();
        egui::Area::new(egui::Id::new("apertura_pdf")).order(egui::Order::Foreground).fixed_pos(zona.min).show(ctx, |ui| {
            ui.allocate_rect(Rect::from_min_size(zona.min, zona.size()), egui::Sense::click_and_drag());
            let passato = a.chiesto.elapsed();
            if passato < DOPO_QUANTO_SI_DICE {
                ctx.request_repaint_after(DOPO_QUANTO_SI_DICE - passato);
                return;
            }
            let font = Carattere::Grassetto.font(ui.ctx(), 15.0);
            let g = ui.painter().layout_no_wrap(APRO.to_string(), font, egui::Color32::WHITE);
            let pillola = Rect::from_center_size(zona.center(), vec2(g.size().x + 32.0, 44.0));
            ui.painter().rect_filled(pillola, 22.0, tema::INDICATORE);
            ui.painter().galley(pillola.center() - g.size() / 2.0, g, egui::Color32::PLACEHOLDER);
            ui.interact(pillola, egui::Id::new("apertura_pdf_testo"), egui::Sense::hover())
                .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, APRO));
        });
    }
}

/// Una riga dell'indice, alta almeno 48 dp e larga quanto il foglio, come l'Indice di Word: il
/// titolo su una riga (coi puntini se è lungo), rientrato per livello, il primo livello in
/// grassetto, dal terzo più piccolo e grigio; a destra il numero della pagina, in grigio. La linea
/// sottile sotto, se non è l'ultima.
fn riga_segnalibro(ui: &mut egui::Ui, s: &Segnalibro, ultima: bool) -> egui::Response {
    let ctx = ui.ctx().clone();
    let (carattere, misura, colore) = match s.livello {
        0 => (Carattere::Grassetto, 16.0, tema::INCHIOSTRO),
        1 => (Carattere::Testo, 16.0, tema::INCHIOSTRO),
        _ => (Carattere::Testo, 15.0, tema::MATITA),
    };
    let largo = ui.available_width();
    let rientro = 4.0 + RIENTRO * s.livello.min(4) as f32;
    let numero = s.pagina.map(|p| ui.painter().layout_no_wrap((p + 1).to_string(), Carattere::Testo.font(&ctx, 14.0), tema::MATITA));
    let spazio_numero = numero.as_ref().map_or(0.0, |g| g.size().x + 16.0);
    let mut job = egui::text::LayoutJob::single_section(s.titolo.clone(), egui::TextFormat::simple(carattere.font(&ctx, misura), colore));
    job.wrap = egui::text::TextWrapping::truncate_at_width((largo - rientro - 8.0 - spazio_numero).max(24.0));
    let g = ui.painter().layout_job(job);
    let (rect, r) = ui.allocate_exact_size(vec2(largo, tema::PULSANTE.max(g.size().y + 16.0)), egui::Sense::click());
    let etichetta = match s.pagina {
        Some(p) => format!("{}, pagina {}", s.titolo, p + 1),
        None => s.titolo.clone(),
    };
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, s.pagina.is_some(), &etichetta));
    if r.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, CornerRadius::same(tema::RAGGIO_SEGMENTO), tema::PREMUTO);
    }
    ui.painter().galley(pos2(rect.left() + rientro, rect.center().y - g.size().y / 2.0), g, colore);
    if let Some(n) = numero {
        ui.painter().galley(pos2(rect.right() - 8.0 - n.size().x, rect.center().y - n.size().y / 2.0), n, tema::MATITA);
    }
    if !ultima {
        ui.painter().rect_filled(Rect::from_min_max(pos2(rect.left(), rect.bottom() - 1.0), rect.max), 0.0, tema::FILO);
    }
    r
}

/// L'indice di un PDF senza segnalibri: l'icona grigia, «Nessun segnalibro» e perché.
fn indice_vuoto(ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        ui.label(RichText::new(icone::TOC).font(Carattere::Icona.font(&ctx, tema::ICONA)).color(tema::MATITA));
        ui.label(RichText::new("Nessun segnalibro").font(Carattere::Grassetto.font(&ctx, 16.0)).color(tema::INCHIOSTRO));
    });
    ui.add_space(6.0);
    ui.label(
        RichText::new("Questo PDF non ha un indice: chi l'ha creato non ci ha messo i segnalibri. Per spostarti usa le miniature o «Vai a pagina».")
            .font(Carattere::Testo.font(&ctx, 15.0))
            .color(tema::MATITA),
    );
    ui.add_space(8.0);
}

/// Le righe della diagnosi di un frame del lettore: la prima pagina, lo zoom nuovo, una pagina
/// che non si disegna, una ricerca finita.
fn scrivi_esito_del_lettore(esito: &Esito, inizio: Instant) {
    if let Some(p) = esito.prima_pagina {
        diagnosi(format!(
            "pdf: prima pagina sullo schermo dopo {:.0} ms dall'apertura, disegnata in {} ms, {} × {} pixel",
            inizio.elapsed().as_secs_f64() * 1000.0,
            p.disegno_ms,
            p.larghezza,
            p.altezza
        ));
    }
    if let Some(z) = esito.zoom {
        diagnosi(format!("pdf: zoom {}", format!("{z:.2}").replace('.', ",")));
    }
    if let Some(e) = esito.errore {
        diagnosi(format!("pdf: pagina non disegnata ({e})"));
    }
    if let Some(r) = esito.ricerca_finita {
        diagnosi(format!("pdf: ricerca finita, {} risultati in {} pagine lette, {} ms", r.risultati, r.pagine, r.ms));
    }
    match (esito.selezionati, esito.tutta) {
        (Some(n), true) => diagnosi(format!("pdf: tutta la pagina selezionata, {n} caratteri")),
        (Some(n), false) => diagnosi(format!("pdf: selezione di {n} caratteri")),
        (None, _) => {}
    }
    if esito.copia.is_some() {
        diagnosi("pdf: selezione copiata");
    }
    if esito.senza_testo {
        diagnosi("pdf: pressione lunga su una pagina senza testo");
    }
}

/// Le righe di «Informazioni»: pagine, misura del file, titolo e autore se ci sono, firme
/// digitali, firme e iniziali messe col dito o da una foto (`a_mano`, se ce ne sono) e cartella.
/// Le due righe delle firme stanno separate: «Firme digitali» contava solo quelle col certificato,
/// e due firme a mano sembravano non contare (segnalazione dell'autore del 7/11).
/// La riga delle firme messe con Fill & Sign, nelle Informazioni.
pub(crate) const FIRME_A_MANO: &str = "Firme a mano";

fn righe_informazioni(info: Option<&(Informazioni, String)>, a_mano: usize) -> Vec<(&'static str, String)> {
    let Some((i, cartella)) = info else { return Vec::new() };
    let mut righe = vec![("Pagine", crate::schermo_editor::numero(i.pagine)), ("Misura", home::misura(i.byte as u64))];
    righe.extend(i.titolo.clone().map(|t| ("Titolo", t)));
    righe.extend(i.autore.clone().map(|a| ("Autore", a)));
    righe.push(("Firme digitali", i.firme.descrizione()));
    if a_mano > 0 {
        righe.push((FIRME_A_MANO, if a_mano == 1 { "1 firma".to_string() } else { format!("{a_mano} firme") }));
    }
    righe.push(("Cartella", cartella.clone()));
    righe
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn i_nomi_dei_pdf() {
        assert_eq!(nome_del_pdf("Bolletta luce.pdf", "content://x/1"), "Bolletta luce.pdf");
        assert_eq!(nome_del_pdf(" Bolletta luce ", "content://x/1"), "Bolletta luce.pdf");
        assert_eq!(nome_del_pdf("", "/storage/emulated/0/Download/Turni.PDF"), "Turni.PDF");
        assert_eq!(nome_del_pdf("", "content://com.whatsapp.provider.media/item/42"), "documento.pdf");
        // Senza il nome vero, l'ultimo pezzo dell'URI può essere codificato coi «%»: si decodifica,
        // e del percorso che ne esce si tiene l'ultimo pezzo (revisione della 0.3.0).
        assert_eq!(nome_del_pdf("", "content://x/Bolletta%20luce.pdf"), "Bolletta luce.pdf");
        assert_eq!(nome_del_pdf("", "content://com.android.externalstorage.documents/document/primary%3ADownload%2FTurni.pdf"), "Turni.pdf");
        assert_eq!(nome_del_pdf("", "content://x/caff%C3%A8.pdf"), "caffè.pdf");
        assert_eq!(nome_del_pdf("", "content://com.android.providers.downloads.documents/document/msf%3A1234"), "documento.pdf");
        // Una codifica rotta o che porta fuori dal nome: «documento.pdf».
        assert_eq!(nome_del_pdf("", "content://x/a%2"), "documento.pdf");
        assert_eq!(nome_del_pdf("", "content://x/%2E%2E%2F.pdf"), "documento.pdf");
        assert_eq!(nome_del_pdf("", "content://x/%FF%FE.pdf"), "documento.pdf");
        assert_eq!(sottotitolo(1), "PDF · 1 pagina");
        assert_eq!(sottotitolo(12), "PDF · 12 pagine");
    }

    #[test]
    fn il_conteggio_della_ricerca() {
        let c = |attuale, totale, finita| Some(Conteggio { attuale, totale, finita });
        assert_eq!(conteggio_pdf(c(Some(1), 15, true), false), "2 di 15");
        assert_eq!(conteggio_pdf(c(Some(0), 3, false), false), "1 di 3");
        assert_eq!(conteggio_pdf(c(None, 0, false), false), "Cerco…");
        assert_eq!(conteggio_pdf(c(None, 0, true), false), "Nessun risultato");
        assert_eq!(conteggio_pdf(c(None, 4, false), false), "4");
        assert_eq!(conteggio_pdf(c(Some(1), 15, true), true), "");
        assert_eq!(conteggio_pdf(None, false), "");
        let lungo = format!("https://esempio.it/{}", "a".repeat(200));
        assert_eq!(accorciato(&lungo).chars().count(), INDIRIZZO_MASSIMO + 1);
        assert_eq!(accorciato("https://esempio.it"), "https://esempio.it");
    }
}
