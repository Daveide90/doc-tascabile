//! «Riconosci testo», «Esporta in Word» e «Scansiona» (piano «PDF completo», P8).
//!
//! Riconosci testo: dal foglio Strumenti, su tutte le pagine o su quella che si legge. Il
//! riconoscitore (`tascabile-ocr`, coi modelli negli asset dell'APK letti la prima volta e tenuti
//! per le volte dopo) gira su un thread suo col dialogo dell'avanzamento e Annulla; le parole
//! diventano lo strato di testo invisibile con una modifica del motore, che si annulla.
//!
//! Esporta in Word: dal menu e dal foglio Strumenti, il `.docx` del motore in Documenti/Tascabile.
//!
//! Scansiona: dal foglio Strumenti e dal «Nuovo» della Home. La fotocamera di sistema (l'Intent
//! della foto della firma, P3); poi, dalla 0.8.2 (R3, «migliora la scansione, ora fa pena»), dopo
//! ogni foto la schermata «Sistema i bordi»: la foto intera coi quattro angoli trovati dal motore
//! (o, se non li trova, un riquadro poco dentro la foto) da spostare col dito, con la lente sotto il
//! dito, e Tutta la foto, Rifai, Fatto. Raddrizzamento e pulizia su un thread; l'anteprima a tutto
//! schermo con la pagina che si sceglie dalle miniature, l'aspetto (Documento, Grigi, Bianco e
//! nero, Originale), Bordi e Ruota in alto, Rifai, Altra pagina e Crea PDF in fondo; il PDF nuovo
//! va in Documenti/Tascabile con l'OCR già fatto e si apre. Foto e pagine si tengono in JPEG (o PNG
//! a un bit), non in pixel nudi. La foto si cancella dalla cache appena letta; nella diagnosi solo
//! conti, misure e tempi, mai testo, pixel o nomi di file.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use eframe::egui::{self, Color32, Id, Order, Pos2, Rect, Sense, Stroke, UiBuilder, WidgetInfo, WidgetType, pos2, vec2};
use tascabile_files::cartella_tascabile;
use tascabile_pdf::ocr::{EsitoOcr, LavoroOcr};
use tascabile_pdf::scansione::{
    Angoli, Aspetto, Cancellatura, ErroreScansione, Formato, FotoScansionata, Lati, PaginaScansionata, Ritocchi, angoli_buoni, crea_pdf,
    fai_pagina, lati_curvi, lati_dritti, prepara_foto,
};
use tascabile_pdf::{ErroreFoto, Pdf};
use tascabile_platform::{EsitoFoto, FonteFoto, MASSIMO_PAGINE_SCELTE, diagnosi};
use tascabile_touch::tema::{self, Carattere};
use tascabile_touch::{BarraAlta, BarraVoci, Chip, Dialogo, Peso, Pulsante, PulsanteIcona, RispostaDialogo, icone};
use tascabile_word::salva;

use crate::{Errore, TascabileApp};

pub(crate) const TUTTE_LE_PAGINE: &str = "Tutte le pagine";
pub(crate) const QUESTA_PAGINA: &str = "Questa pagina";
const RICONOSCO: &str = "Riconosco il testo";
const ANNULLA: &str = "Annulla";
pub(crate) const ESPORTA_WORD: &str = "Esporta in Word";
pub(crate) const SCANSIONE: &str = "Scansione";
pub(crate) const RIFAI: &str = "Rifai";
pub(crate) const RUOTA: &str = "Ruota";
pub(crate) const CREA_PDF: &str = "Crea PDF";
pub(crate) const CHIUDI: &str = "Chiudi";
pub(crate) const BORDI: &str = "Bordi";
/// Le due fonti delle pagine, in fondo (0.8.3: dalla galleria anche più foto insieme).
pub(crate) const FOTOCAMERA: &str = "Fotocamera";
pub(crate) const GALLERIA: &str = "Galleria";
/// Gli strumenti della pagina che si vede, nella barra sotto la pagina.
pub(crate) const ASPETTO: &str = "Aspetto";
pub(crate) const TOGLI: &str = "Togli";
/// Il cancellino della pagina (l'autore, 3/11): col dito si sbiancano le macchie.
pub(crate) const GOMMA: &str = "Gomma";
pub(crate) const ANNULLA_PASSATA: &str = "Annulla";
pub(crate) const MISURE_GOMMA: [&str; 3] = ["Sottile", "Media", "Larga"];
/// Il raggio della gomma sullo schermo, per misura (dp).
const RAGGI_GOMMA: [f32; 3] = [7.0, 14.0, 26.0];
const SPIEGA_GOMMA: &str = "Passa il dito sulle macchie da togliere";
pub(crate) const PASSATE_TOLTE: &str = "Bordi nuovi: le passate della gomma sono state tolte, ripassale.";
pub(crate) const PASSATA_TOLTA: &str = "Tolta l'ultima passata della gomma.";
/// La fila dei formati del foglio in «Sistema i bordi», dp.
const ALTO_FORMATI: f32 = 48.0;
/// I punti in mezzo ai lati, per l'accessibilità e le prove.
pub(crate) const LATI: [&str; 4] = ["Lato in alto", "Lato a destra", "Lato in basso", "Lato a sinistra"];
/// Il pulsante del dialogo che chiede prima di togliere una pagina.
pub(crate) const TOGLI_LA_PAGINA: &str = "Togli la pagina";
/// Il foglio dei quattro aspetti.
pub(crate) const ASPETTO_DELLA_PAGINA: &str = "Aspetto della pagina";
/// La X del foglio degli aspetti (Chiudi è già la freccia della scansione).
pub(crate) const CHIUDI_ASPETTO: &str = "Chiudi l'aspetto";
/// La scansione vuota.
const PER_COMINCIARE: &str = "Fotografa il foglio, o scegli dalla galleria le foto che hai già: anche più di una, fino a 30, una per pagina.";
/// La schermata dopo ogni foto (R3).
pub(crate) const SISTEMA_I_BORDI: &str = "Sistema i bordi";
pub(crate) const TUTTA_LA_FOTO: &str = "Tutta la foto";
pub(crate) const FATTO: &str = "Fatto";
/// Per una foto della galleria al posto di Rifai: la si lascia e si passa alla prossima.
pub(crate) const SALTA: &str = "Salta";
pub(crate) const LASCIA_LA_FOTO: &str = "Lascia questa foto";
pub(crate) const LASCIA_LE_FOTO: &str = "Lascia queste foto";
/// I quattro angoli, per l'accessibilità e per le prove (nell'ordine degli `Angoli`).
pub(crate) const ANGOLI: [&str; 4] =
    ["Angolo in alto a sinistra", "Angolo in alto a destra", "Angolo in basso a destra", "Angolo in basso a sinistra"];
const SPIEGA_BORDI: &str = "Trascina i quattro angoli sugli angoli del foglio; i punti in mezzo ai lati seguono un foglio piegato.";
const BORDI_NON_TROVATI: &str = "Non ho trovato i bordi del foglio: trascina i quattro angoli sugli angoli del foglio.";
const ANGOLI_INCROCIATI: &str = "Gli angoli si incrociano: rimettili ognuno al suo posto.";
const PAGINA_DI_PRIMA: &str = "Finisco la pagina di prima: un attimo.";
/// Il dialogo che chiede prima di buttare le pagine fotografate (analisi del 27/10, pagine I2).
pub(crate) const BUTTARE: &str = "Buttare le pagine fotografate?";
pub(crate) const BUTTA: &str = "Butta";
pub(crate) const ANNULLA_SCANSIONE: &str = "Annulla";
const CREO: &str = "Creo il PDF";
const NON_RICONOSCIUTO: &str = "Testo non riconosciuto";
const NON_ESPORTATO: &str = "Non esportato";
const SCANSIONE_NON_FATTA: &str = "Scansione non salvata";

/// Il riconoscitore, caricato la prima volta (pesa una ventina di MB) e tenuto per le volte dopo.
static OCR: Mutex<Option<Arc<tascabile_pdf::ocr::Ocr>>> = Mutex::new(None);

/// I byte dei modelli dagli asset, solo se il riconoscitore non c'è ancora (sul thread
/// dell'interfaccia: la piattaforma non va sugli altri).
fn modelli_se_servono(p: &crate::Piattaforma) -> Option<tascabile_pdf::ocr::Modelli> {
    if OCR.lock().is_ok_and(|o| o.is_some()) {
        return None;
    }
    tascabile_pdf::ocr::Modelli::da(|nome| p.leggi_asset(nome)).ok()
}

/// Il riconoscitore, sul thread di lavoro: quello già caricato o uno nuovo dai modelli.
fn riconoscitore(modelli: Option<tascabile_pdf::ocr::Modelli>) -> Result<Arc<tascabile_pdf::ocr::Ocr>, tascabile_pdf::ocr::ErroreOcr> {
    let mut posto = OCR.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(o) = posto.as_ref() {
        return Ok(o.clone());
    }
    let inizio = Instant::now();
    let modelli = modelli.ok_or(tascabile_pdf::ocr::ErroreOcr::Modello("asset"))?;
    let o = Arc::new(tascabile_pdf::ocr::Ocr::carica(modelli)?);
    diagnosi(format!("ocr: modelli caricati in {:.1} s", inizio.elapsed().as_secs_f32()));
    *posto = Some(o.clone());
    Ok(o)
}

/// Un lavoro lungo su un thread suo, con l'avanzamento e la richiesta di fermarsi.
pub(crate) struct Lavoro<T> {
    fatte: Arc<AtomicUsize>,
    totale: Arc<AtomicUsize>,
    ferma: Arc<AtomicBool>,
    rx: Receiver<T>,
    pub(crate) inizio: Instant,
}

impl<T: Send + 'static> Lavoro<T> {
    /// Avvia `f` sul thread `nome`; `f` riceve chi dice l'avanzamento (fatte, totale) e risponde
    /// se continuare.
    pub(crate) fn avvia(
        nome: &str,
        ctx: &egui::Context,
        f: impl FnOnce(&(dyn Fn(usize, usize) -> bool + Sync)) -> T + Send + 'static,
    ) -> Option<Self> {
        let (fatte, totale, ferma) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)), Arc::new(AtomicBool::new(false)));
        let (tx, rx) = std::sync::mpsc::channel();
        let (f2, t2, s2, ctx2) = (fatte.clone(), totale.clone(), ferma.clone(), ctx.clone());
        let avviato = std::thread::Builder::new().name(nome.into()).spawn(move || {
            let avanti = |a: usize, b: usize| {
                if f2.swap(a, Ordering::Relaxed) != a || t2.swap(b, Ordering::Relaxed) != b {
                    ctx2.request_repaint();
                }
                !s2.load(Ordering::Relaxed)
            };
            let esito = f(&avanti);
            let _ = tx.send(esito);
            ctx2.request_repaint();
        });
        if avviato.is_err() {
            diagnosi(format!("{nome}: thread non avviato"));
            return None;
        }
        Some(Lavoro { fatte, totale, ferma, rx, inizio: Instant::now() })
    }

    pub(crate) fn ferma(&self) {
        self.ferma.store(true, Ordering::Relaxed);
    }

    pub(crate) fn fermato(&self) -> bool {
        self.ferma.load(Ordering::Relaxed)
    }

    /// L'esito, se è arrivato (o se il thread è finito senza darlo).
    pub(crate) fn esito(&self) -> Option<Option<T>> {
        match self.rx.try_recv() {
            Ok(e) => Some(Some(e)),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(None),
        }
    }

    /// Quanto è fatto, da 0 a 1.
    pub(crate) fn frazione(&self) -> f32 {
        let (fatte, totale) = (self.fatte.load(Ordering::Relaxed), self.totale.load(Ordering::Relaxed));
        if totale == 0 { 0.0 } else { fatte as f32 / totale as f32 }
    }

    /// «Pagina 2 di 5» e la frazione fatta.
    fn a_che_punto(&self) -> (String, f32) {
        let (fatte, totale) = (self.fatte.load(Ordering::Relaxed), self.totale.load(Ordering::Relaxed));
        if totale == 0 {
            return ("Preparo il riconoscimento".into(), 0.0);
        }
        (format!("Pagina {} di {totale}", (fatte + 1).min(totale)), fatte as f32 / totale as f32)
    }
}

/// Il dialogo dell'avanzamento: il titolo, «Pagina 2 di 5», la barra e Annulla. Vero se si è
/// toccato Annulla.
fn mostra_avanzamento<T: Send + 'static>(ctx: &egui::Context, id: &str, titolo: &str, lavoro: &Lavoro<T>) -> bool {
    let (testo, frazione) = lavoro.a_che_punto();
    mostra_barra(ctx, id, titolo, &testo, frazione, lavoro.fermato())
}

/// Il dialogo dell'avanzamento con un testo e una frazione dati: vero se si è toccato Annulla.
pub(crate) fn mostra_barra(ctx: &egui::Context, id: &str, titolo: &str, testo: &str, frazione: f32, fermato: bool) -> bool {
    let (testo, frazione) = if fermato { ("Mi fermo", 1.0) } else { (testo, frazione) };
    let (risposta, _) = Dialogo::new(Id::new(id), titolo).testo(testo).pulsante(ANNULLA, Peso::Testo).mostra_con(ctx, |ui| {
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 8.0), Sense::hover());
        ui.painter().rect_filled(r, 4.0, tema::FILO);
        let pieno = Rect::from_min_max(r.min, pos2(r.left() + r.width() * frazione.clamp(0.0, 1.0), r.bottom()));
        ui.painter().rect_filled(pieno, 4.0, tema::BLU);
        ui.add_space(6.0);
    });
    ctx.request_repaint_after(std::time::Duration::from_millis(200));
    matches!(risposta, Some(RispostaDialogo::Pulsante(0)))
}

/// Quante pagine, per i messaggi: «1 pagina», «3 pagine».
fn pagine(n: usize) -> String {
    if n == 1 { "1 pagina".into() } else { format!("{n} pagine") }
}

/// Il lato lungo della foto nella schermata dei bordi (e nella lente): basta per vedere gli angoli,
/// e la texture resta leggera.
const LATO_FOTO_BORDI: u32 = 1600;
/// Il lato lungo dell'anteprima di una pagina.
const LATO_PAGINA: u32 = 1200;
/// Un dito a meno di tanto (dp) da un angolo lo prende.
const PRESA: f32 = 48.0;
/// La lente: il raggio (dp) e quanto ingrandisce la foto com'è sullo schermo.
const RAGGIO_LENTE: f32 = 58.0;
const INGRANDISCE: f32 = 3.0;
/// Il velo sulla foto fuori dal foglio.
const VELO_FUORI: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 120);

/// La foto pronta per «Sistema i bordi», dal thread che la prepara: la foto, la sua immagine per lo
/// schermo, gli angoli da cui partire, l'aspetto e i giri della pagina che ne verrà, da dove viene
/// (per Rifai o Salta) e quale delle foto scelte insieme è («2 di 5»).
struct Preparata {
    foto: Arc<FotoScansionata>,
    immagine: egui::ColorImage,
    angoli: Angoli,
    /// I lati curvi trovati (o di prima) e le passate della gomma da tenere.
    ritocchi: Ritocchi,
    aspetto: Aspetto,
    giri: u8,
    fonte: FonteFoto,
    posizione: Option<(usize, usize)>,
}

/// La schermata «Sistema i bordi» aperta.
pub(crate) struct Bordi {
    foto: Arc<FotoScansionata>,
    texture: egui::TextureHandle,
    /// Gli angoli di adesso e quelli da cui si è partiti (per dire se si sono spostati).
    angoli: Angoli,
    partenza: Angoli,
    /// I punti in mezzo ai lati: dritti, o curvi per un foglio piegato.
    lati: Lati,
    /// Le passate della gomma della pagina che si rifà: restano se i bordi restano com'erano.
    cancellature: Vec<Cancellatura>,
    /// Il formato del foglio: A4, A3, A5, biglietto da visita o automatico.
    formato: Formato,
    /// I lati e il formato da cui si è partiti: cambiati loro o gli angoli, le passate della gomma
    /// cadrebbero su un altro punto del foglio.
    lati_di_partenza: Lati,
    formato_di_partenza: Formato,
    /// Il punto preso col dito (gli angoli da 0 a 3, i punti in mezzo ai lati da 4 a 7) e lo
    /// scarto fra il dito e il punto, nei pixel della foto.
    preso: Option<(usize, [f32; 2])>,
    aspetto: Aspetto,
    giri: u8,
    fonte: FonteFoto,
    posizione: Option<(usize, usize)>,
    /// Dove sta la foto sullo schermo e quanti dp per pixel della foto (dell'ultimo frame).
    posto: Rect,
    scala: f32,
}

/// La pagina fatta dal thread, con la sua anteprima.
type Fatta = (PaginaScansionata, egui::ColorImage);

/// La scansione aperta: le pagine già fatte con le anteprime e quella che si vede, l'aspetto per le
/// pagine nuove, la foto che si aspetta o si prepara, le foto della galleria che aspettano i loro
/// bordi, i bordi da sistemare, la pagina che si fa, il PDF che si crea, il foglio degli aspetti e
/// il dialogo che chiede prima di togliere una pagina.
#[derive(Default)]
pub(crate) struct Scansione {
    pagine: Vec<(PaginaScansionata, Option<egui::TextureHandle>)>,
    corrente: usize,
    aspetto: Aspetto,
    /// La foto chiesta (il numero della richiesta), che non è ancora arrivata.
    pub(crate) attende_foto: Option<i32>,
    /// La pagina da rifare: resta finché non arriva quella nuova (analisi del 27/10, pagine I3).
    da_rifare: Option<usize>,
    /// Il dialogo «Buttare le pagine fotografate?» è aperto.
    chiedi_di_buttare: bool,
    /// Le foto scelte insieme dalla galleria che aspettano (file della cache), e quante erano.
    coda: VecDeque<PathBuf>,
    scelte: usize,
    preparazione: Option<Receiver<Result<Preparata, ErroreFoto>>>,
    bordi: Option<Bordi>,
    /// La pagina che si fa, e dove va: al posto di questa pagina, o in fondo. Il posto si decide
    /// quando parte, così lasciare i bordi di un'altra foto non lo cambia.
    lavorazione: Option<(Lavorazione, Option<usize>)>,
    creazione: Option<Lavoro<Result<(PathBuf, usize), &'static str>>>,
    /// Il foglio degli aspetti è aperto.
    scegli_aspetto: bool,
    /// Il dialogo «Togliere la pagina?» per questa pagina.
    togli: Option<usize>,
    /// La miniatura della pagina che si vede va portata in vista nella striscia.
    porta_in_vista: bool,
    /// La gomma aperta sulla pagina che si vede.
    gomma: Option<Gomma>,
    /// Il formato scelto per le pagine nuove: l'ultimo scelto in «Sistema i bordi».
    formato: Formato,
}

/// La gomma aperta: le passate fatte (sulla pagina come si vede, da 0 a 1, col raggio in
/// frazione della larghezza), quella che il dito sta facendo, la misura scelta.
#[derive(Default)]
pub(crate) struct Gomma {
    passate: Vec<Cancellatura>,
    tratto: Vec<[f32; 2]>,
    misura: usize,
}

/// Che cosa si è fatto con la gomma aperta.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ToccoGomma {
    /// Il dito passa da qui (da 0 a 1 sulla pagina come si vede).
    Punto([f32; 2]),
    /// Il dito si è alzato: la passata è finita, col raggio dato (frazione della larghezza).
    Fine(f32),
    /// Un tocco solo, qui, col raggio dato.
    Tocco([f32; 2], f32),
    Annulla,
    Misura(usize),
    Fatto,
}

/// Le passate della gomma fatte sulla pagina come si vede (girata `giri` quarti di giro, larga
/// `larghezza` e alta `altezza`) portate sulla pagina prima dei giri, dove le rifà `fai_pagina`.
fn passate_senza_giri(passate: &[Cancellatura], giri: u8, larghezza: u32, altezza: u32) -> Vec<Cancellatura> {
    let dispari = giri % 2 == 1;
    passate
        .iter()
        .map(|c| Cancellatura {
            punti: c
                .punti
                .iter()
                .map(|&[x, y]| match giri % 4 {
                    1 => [y, 1.0 - x],
                    2 => [1.0 - x, 1.0 - y],
                    3 => [1.0 - y, x],
                    _ => [x, y],
                })
                .collect(),
            raggio: if dispari { c.raggio * larghezza as f32 / altezza.max(1) as f32 } else { c.raggio },
        })
        .collect()
}

/// Una pagina che si fa sul suo thread.
type Lavorazione = Receiver<Result<Fatta, ErroreFoto>>;

/// Che cosa fa ogni aspetto, nel suo foglio, e la sua icona.
fn spiega_aspetto(a: Aspetto) -> (&'static str, &'static str) {
    match a {
        Aspetto::Documento => (icone::DESCRIPTION, "Pulita: la carta bianca anche dove c'era l'ombra, l'inchiostro a colori"),
        Aspetto::Grigi => (icone::CONTRAST, "Pulita, senza colori"),
        Aspetto::BiancoNero => (icone::TEXT_FIELDS, "Solo nero sul bianco: per i testi, il file più leggero"),
        Aspetto::Originale => (icone::IMAGE, "La foto raddrizzata, com'era"),
    }
}

/// Gli strumenti della pagina che si vede, nell'ordine della barra.
const STRUMENTI: [(&str, &str); 6] = [
    (icone::CROP, BORDI),
    (icone::ROTATE_RIGHT, RUOTA),
    (icone::CONTRAST, ASPETTO),
    (icone::INK_ERASER, GOMMA),
    (icone::PHOTO_CAMERA, RIFAI),
    (icone::DELETE, TOGLI),
];
/// In orizzontale i comandi stanno in una colonna a destra, larga tanto: sei strumenti da almeno
/// 44 dp l'uno (con la Gomma, 3/11).
const COLONNA: f32 = 280.0;

/// Il tetto della foto letta dalla cache, come per l'immagine del modo Modifica.
const MASSIMO_FOTO: u64 = 64 << 20;

/// `f` sul thread `nome`; l'esito arriva nel ricevitore, e poi si ridisegna.
fn sul_thread<T: Send + 'static>(nome: &str, ctx: &egui::Context, f: impl FnOnce() -> T + Send + 'static) -> Option<Receiver<T>> {
    let (tx, rx) = std::sync::mpsc::channel();
    let ctx2 = ctx.clone();
    match std::thread::Builder::new().name(nome.into()).spawn(move || {
        let _ = tx.send(f());
        ctx2.request_repaint();
    }) {
        Ok(_) => Some(rx),
        Err(_) => {
            diagnosi(format!("{nome}: thread non avviato"));
            None
        }
    }
}

/// La lente sotto il dito: la foto attorno all'angolo `angolo` (sullo schermo) ingrandita in un
/// cerchio sopra il dito `dito` (o di lato, se sopra non c'è posto), coi due lati del foglio che
/// partono da lì e il mirino. Fuori dalla foto è scuro.
fn lente(painter: &egui::Painter, b: &Bordi, angolo: usize, dito: Pos2, schermo: Rect, colore: Color32) {
    let q = |i: usize| pos2(b.posto.left() + b.angoli[i][0] * b.scala, b.posto.top() + b.angoli[i][1] * b.scala);
    // Un punto in mezzo a un lato: la lente sopra di lui, senza i lati.
    let punto = if angolo < 4 { b.angoli[angolo] } else { b.lati[(angolo - 4) % 4] };
    let distanza = RAGGIO_LENTE + 56.0;
    let alto = schermo.top() + tema::BARRA_ALTA + RAGGIO_LENTE + 4.0;
    let mut centro = dito - vec2(0.0, distanza);
    if centro.y < alto {
        let verso = if dito.x > schermo.center().x { -1.0 } else { 1.0 };
        centro = pos2(dito.x + verso * distanza, dito.y.max(alto));
    }
    centro.x = centro.x.clamp(schermo.left() + RAGGIO_LENTE + 4.0, schermo.right() - RAGGIO_LENTE - 4.0);
    // La foto: anelli e spicchi, ogni punto col suo uv; fuori dalla foto il punto è scuro.
    let (uv_x, uv_y) = (punto[0] / b.foto.larghezza.max(1) as f32, punto[1] / b.foto.altezza.max(1) as f32);
    let uv_per_dp = vec2(1.0 / b.posto.width().max(1.0), 1.0 / b.posto.height().max(1.0));
    const ANELLI: usize = 6;
    const SPICCHI: usize = 48;
    let mut mesh = egui::Mesh::with_texture(b.texture.id());
    for anello in 0..=ANELLI {
        let r = RAGGIO_LENTE * anello as f32 / ANELLI as f32;
        for spicchio in 0..(if anello == 0 { 1 } else { SPICCHI }) {
            let (s, c) = (std::f32::consts::TAU * spicchio as f32 / SPICCHI as f32).sin_cos();
            let uv = pos2(uv_x + c * r / INGRANDISCE * uv_per_dp.x, uv_y + s * r / INGRANDISCE * uv_per_dp.y);
            let dentro = (0.0..=1.0).contains(&uv.x) && (0.0..=1.0).contains(&uv.y);
            mesh.vertices.push(egui::epaint::Vertex {
                pos: centro + vec2(c, s) * r,
                uv,
                color: if dentro { Color32::WHITE } else { Color32::from_gray(30) },
            });
        }
    }
    let punto = |anello: usize, spicchio: usize| if anello == 0 { 0 } else { (1 + (anello - 1) * SPICCHI + spicchio % SPICCHI) as u32 };
    for anello in 0..ANELLI {
        for spicchio in 0..SPICCHI {
            if anello == 0 {
                mesh.add_triangle(0, punto(1, spicchio), punto(1, spicchio + 1));
            } else {
                let (a, b2, c, d) =
                    (punto(anello, spicchio), punto(anello, spicchio + 1), punto(anello + 1, spicchio), punto(anello + 1, spicchio + 1));
                mesh.add_triangle(a, c, d);
                mesh.add_triangle(a, d, b2);
            }
        }
    }
    painter.add(egui::Shape::mesh(mesh));
    // I due lati che partono dall'angolo, ingranditi, fino al bordo della lente; il mirino.
    for altro in [(angolo + 3) % 4, (angolo + 1) % 4].into_iter().filter(|_| angolo < 4) {
        let verso = (q(altro) - q(angolo)).normalized();
        if verso.is_finite() {
            painter.line_segment([centro, centro + verso * RAGGIO_LENTE], Stroke::new(2.0, colore));
        }
    }
    painter.circle_stroke(centro, 5.0, Stroke::new(2.0, colore));
    painter.circle_stroke(centro, RAGGIO_LENTE, Stroke::new(3.0, Color32::WHITE));
    painter.circle_stroke(centro, RAGGIO_LENTE + 1.5, Stroke::new(1.0, Color32::from_black_alpha(90)));
}

impl TascabileApp {
    /// «Riconosci testo» nel foglio Strumenti: il foglio con Tutte le pagine e Questa pagina.
    pub(crate) fn apri_riconosci_pdf(&mut self) {
        self.fogli_pdf.riconosci.apri();
    }

    /// Il foglio di «Riconosci testo» e, mentre si riconosce, il dialogo dell'avanzamento.
    pub(crate) fn mostra_riconosci_pdf(&mut self, ctx: &egui::Context) {
        let (pagine_tutte, corrente) = self.pdf.as_ref().map_or((0, 0), |p| (p.lettore.pagine(), p.lettore.pagina_corrente()));
        let scelta = self
            .fogli_pdf
            .riconosci
            .mostra(ctx, crate::pdf_lavoro::Strumento::RiconosciTesto.nome(), |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let mut scelta = None;
                if crate::pdf_firma::riga(ui, icone::FILE_COPY, TUTTE_LE_PAGINE, None, false).clicked() {
                    scelta = Some(Vec::new());
                }
                if crate::pdf_firma::riga(ui, icone::DESCRIPTION, QUESTA_PAGINA, None, true).clicked() {
                    scelta = Some(vec![corrente]);
                }
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(format!("In italiano, sul telefono: niente va in rete. {} in tutto.", pagine(pagine_tutte)))
                        .font(Carattere::Testo.font(ui.ctx(), 14.0))
                        .color(tema::MATITA),
                );
                ui.add_space(6.0);
                scelta
            })
            .flatten();
        if let Some(scelte) = scelta {
            self.fogli_pdf.riconosci.chiudi();
            self.riconosci_pdf(&scelte, ctx);
        }
        let annulla = self.riconosci.as_ref().is_some_and(|l| mostra_avanzamento(ctx, "riconosci_testo", RICONOSCO, l));
        if annulla {
            self.ferma_riconosci_pdf();
        }
    }

    /// Indietro o Annulla mentre si riconosce: il lavoro si ferma e non scrive niente.
    pub(crate) fn ferma_riconosci_pdf(&mut self) -> bool {
        let Some(l) = self.riconosci.as_ref() else { return false };
        if !l.fermato() {
            l.ferma();
            diagnosi("pdf: ocr, fermato");
        }
        true
    }

    /// Riconosce le pagine `pagine` (da 0; vuoto vuol dire tutte) sul thread `pdf-ocr`.
    pub(crate) fn riconosci_pdf(&mut self, pagine: &[usize], ctx: &egui::Context) {
        if self.riconosci.is_some() {
            return;
        }
        let Some(p) = self.pdf.as_ref() else { return };
        if !crate::pdf_lavoro::si_modifica(p.motore.stato()) {
            self.errore = Some(Errore { titolo: NON_RICONOSCIUTO, testo: "Questo PDF si può solo leggere.".into() });
            return;
        }
        let lavoro = LavoroOcr::nuovo(p.motore.stato(), pagine);
        diagnosi(format!("pdf: ocr di {} su {}", pagine_diagnosi(lavoro.pagine.len()), p.lettore.pagine()));
        let modelli = modelli_se_servono(&self.piattaforma);
        self.riconosci = Lavoro::avvia("pdf-ocr", ctx, move |avanti| {
            let ocr = riconoscitore(modelli)?;
            lavoro.esegui(&ocr, avanti)
        });
    }

    /// A ogni frame: l'esito del riconoscimento, se è arrivato, diventa la modifica del motore.
    pub(crate) fn passo_riconosci_pdf(&mut self) {
        let Some(l) = self.riconosci.as_ref() else { return };
        let Some(esito) = l.esito() else { return };
        let secondi = l.inizio.elapsed().as_secs_f32();
        self.riconosci = None;
        match esito {
            Some(Ok(e)) => self.dopo_il_riconoscimento(e, secondi),
            Some(Err(tascabile_pdf::ocr::ErroreOcr::Fermato)) => {
                diagnosi(format!("pdf: ocr annullato dopo {secondi:.1} s"));
                self.avviso_breve.avvisa("Riconoscimento annullato: il PDF è com'era.");
            }
            Some(Err(e)) => {
                diagnosi(format!("pdf: ocr non fatto ({e})"));
                self.errore = Some(Errore { titolo: NON_RICONOSCIUTO, testo: "Non riesco a caricare il riconoscimento del testo.".into() });
            }
            None => {
                diagnosi("pdf: ocr, il thread si è fermato");
                self.errore = Some(Errore { titolo: NON_RICONOSCIUTO, testo: "Il riconoscimento si è fermato.".into() });
            }
        }
    }

    fn dopo_il_riconoscimento(&mut self, e: EsitoOcr, secondi: f32) {
        let lette = e.lette.len();
        diagnosi(format!(
            "pdf: ocr fatto, {} parole in {}, {} con testo, {} non lette, {secondi:.1} s ({:.1} s a pagina)",
            e.parole(),
            pagine(lette),
            e.con_testo,
            e.non_lette,
            secondi / lette.max(1) as f32
        ));
        match e.modifica() {
            Some(m) => {
                self.modifica_pdf(m);
                let gia = if e.con_testo > 0 { format!("; {} avevano già il testo", pagine(e.con_testo)) } else { String::new() };
                self.avviso_breve.avvisa(format!("Testo riconosciuto: {} parole in {}{gia}. Ora si cerca e si copia.", e.parole(), pagine(lette)));
            }
            None if e.con_testo > 0 && lette == 0 => {
                self.avviso_breve.avvisa(if e.con_testo == 1 { "La pagina ha già il suo testo." } else { "Le pagine hanno già il loro testo." })
            }
            None => self.avviso_breve.avvisa("Non ho trovato testo da riconoscere."),
        }
    }

    /// «Esporta in Word»: il `.docx` del motore in Documenti/Tascabile, fatto sul thread
    /// `pdf-word` dai byte di adesso (con le modifiche non salvate).
    pub(crate) fn esporta_word_pdf(&mut self, ctx: &egui::Context) {
        if self.word.is_some() {
            return;
        }
        let Some(p) = self.pdf.as_ref() else { return };
        let stato = p.motore.stato();
        let (byte, password) = (stato.display.clone(), stato.configurazione.password.clone());
        let base = p.nome.strip_suffix(".pdf").or_else(|| p.nome.strip_suffix(".PDF")).unwrap_or(&p.nome).to_string();
        let cartella = cartella_tascabile(&self.piattaforma.radice_archivio());
        diagnosi("pdf: esporta in word");
        self.word = Lavoro::avvia("pdf-word", ctx, move |_| {
            let pdf = Pdf::apri_condiviso(byte, password.as_deref()).map_err(|_| "apertura")?;
            // Una scansione riconosciuta: i paragrafi del testo, da modificare (il .docx del motore
            // mette le parole in caselle sopra l'immagine, a pezzi; analisi del 27/10, pagine I6).
            let riconosciuto = pdf.testo_riconosciuto();
            let docx = match &riconosciuto {
                Some(pagine) => tascabile_word::salva::docx_dai_paragrafi(pagine).map_err(|_| "word")?,
                None => pdf.in_word(),
            };
            std::fs::create_dir_all(&cartella).map_err(|_| "cartella")?;
            let percorso = salva::scrivi_nuovo(&cartella, &base, "docx", &docx).map_err(|_| "scrittura")?;
            Ok((percorso, docx.len(), riconosciuto.is_some()))
        });
    }

    /// A ogni frame: il `.docx`, se è pronto.
    pub(crate) fn passo_word_pdf(&mut self) {
        let Some(l) = self.word.as_ref() else { return };
        let Some(esito) = l.esito() else { return };
        let secondi = l.inizio.elapsed().as_secs_f32();
        self.word = None;
        match esito {
            Some(Ok((percorso, byte, riconosciuto))) => {
                let da = if riconosciuto { " dal testo riconosciuto" } else { "" };
                diagnosi(format!("pdf: word esportato{da}, {byte} byte in {secondi:.1} s"));
                let nome = percorso.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                // Il .docx esce senza la password del PDF (qui non si sa cifrare un .docx): va
                // detto, perché finisce nella memoria che leggono le altre app.
                let protetto = self.pdf.as_ref().is_some_and(|p| p.motore.stato().cifrato);
                let senza = if protetto { ", senza la password del PDF" } else { "" };
                let solo = if riconosciuto { ", solo il testo riconosciuto" } else { "" };
                self.avviso_breve.avvisa(format!("Salvato in Documenti/Tascabile: {nome}{senza}{solo}"));
            }
            Some(Err(genere)) => {
                diagnosi(format!("pdf: word non esportato ({genere})"));
                self.errore = Some(Errore { titolo: NON_ESPORTATO, testo: "Non riesco a esportare il PDF in Word.".into() });
            }
            None => self.errore = Some(Errore { titolo: NON_ESPORTATO, testo: "L'esportazione si è fermata.".into() }),
        }
    }

    /// Il PDF della scansione è pronto in Documenti/Tascabile: si apre al posto del documento di
    /// adesso, ma prima «Salvare le modifiche?» se ne ha, come Indietro: il PDF di prima non esce
    /// senza le sue modifiche e la sua copia di recupero. Con Annulla la scansione resta dov'è.
    pub(crate) fn apri_la_scansione(&mut self, percorso: std::path::PathBuf, nome: String) {
        self.avviso_breve.avvisa(format!("Scansione salvata in Documenti/Tascabile: {nome}"));
        self.chiudi_poi(crate::Dopo::ApriFile(percorso, nome));
    }

    /// «Scansiona»: la scansione a tutto schermo, vuota, con Fotocamera e Galleria in fondo (0.8.3;
    /// prima partiva subito la fotocamera e la galleria non c'era).
    pub(crate) fn apri_scansione(&mut self) {
        if self.scansione.is_some() {
            return;
        }
        diagnosi("scansione: aperta");
        self.scansione = Some(Scansione::default());
    }

    /// La fotocamera per una pagina: nuova, o quella da rifare.
    fn scatta(&mut self) {
        let Some(s) = self.scansione.as_mut() else { return };
        s.attende_foto = Some(self.piattaforma.chiedi_foto_firma(FonteFoto::Fotocamera));
    }

    /// La galleria: una o più foto, una pagina nuova per ognuna.
    fn scegli_dalla_galleria(&mut self) {
        let Some(s) = self.scansione.as_mut() else { return };
        s.da_rifare = None;
        s.attende_foto = Some(self.piattaforma.chiedi_foto_firma(FonteFoto::GalleriaPiu));
    }

    /// Indietro nella scansione: ferma il PDF che si crea; coi dialoghi o col foglio degli aspetti
    /// aperti vale Annulla; da «Sistema i bordi» lascia le foto; con pagine fotografate chiede
    /// prima di buttarle (anche il gesto dal bordo può essere per sbaglio); senza pagine chiude.
    pub(crate) fn indietro_scansione(&mut self) -> bool {
        let Some(s) = self.scansione.as_mut() else { return false };
        if let Some(c) = s.creazione.as_ref() {
            c.ferma();
            diagnosi("scansione: pdf fermato");
            return true;
        }
        if std::mem::take(&mut s.chiedi_di_buttare) {
            diagnosi("scansione: pagine tenute");
            return true;
        }
        if s.togli.take().is_some() {
            diagnosi("scansione: pagina tenuta");
            return true;
        }
        if std::mem::take(&mut s.scegli_aspetto) {
            return true;
        }
        if s.gomma.take().is_some() {
            diagnosi("scansione: gomma chiusa senza passate");
            return true;
        }
        if s.bordi.is_some() || s.preparazione.is_some() {
            self.lascia_le_foto();
            return true;
        }
        if s.attende_foto.take().is_some() {
            // La fotocamera o la galleria non hanno risposto: non si aspetta più.
            s.da_rifare = None;
            diagnosi("scansione: non aspetto più la foto");
            return true;
        }
        self.chiedi_o_chiudi_scansione();
        true
    }

    /// La gomma: le passate si aggiungono, l'ultima si toglie con Annulla; «Fatto» rifà la pagina
    /// dalla sua foto con le passate nuove (portate sulla pagina prima dei giri), che restano.
    fn tocco_della_gomma(&mut self, ctx: &egui::Context, t: ToccoGomma) {
        let Some(s) = self.scansione.as_mut() else { return };
        let Some(g) = s.gomma.as_mut() else { return };
        match t {
            ToccoGomma::Punto(p) => g.tratto.push(p),
            ToccoGomma::Fine(raggio) => {
                if !g.tratto.is_empty() {
                    g.passate.push(Cancellatura { punti: std::mem::take(&mut g.tratto), raggio });
                }
            }
            ToccoGomma::Tocco(p, raggio) => g.passate.push(Cancellatura { punti: vec![p], raggio }),
            ToccoGomma::Annulla if !g.passate.is_empty() => {
                g.passate.pop();
            }
            // Senza passate nuove, Annulla toglie l'ultima delle volte prima: una volta fatte non
            // si toglievano più (revisione del 3/11). La pagina si rifà, la gomma si chiude.
            ToccoGomma::Annulla => {
                let Some((p, _)) = s.pagine.get(s.corrente) else { return };
                if p.ritocchi.cancellature.is_empty() {
                    return;
                }
                s.gomma = None;
                let mut ritocchi = p.ritocchi.clone();
                ritocchi.cancellature.pop();
                let (foto, angoli, aspetto, giri, posto) = (p.foto.clone(), p.angoli, p.aspetto, p.giri, Some(s.corrente));
                diagnosi("scansione: gomma, tolta una passata di prima");
                self.avviso_breve.avvisa(PASSATA_TOLTA);
                self.fai_la_pagina(ctx, foto, angoli, ritocchi, aspetto, giri, posto);
            }
            ToccoGomma::Misura(m) => g.misura = m.min(2),
            ToccoGomma::Fatto => {
                let Some(g) = s.gomma.take() else { return };
                if g.passate.is_empty() {
                    diagnosi("scansione: gomma chiusa senza passate");
                    return;
                }
                let Some((p, _)) = s.pagine.get(s.corrente) else { return };
                let mut ritocchi = p.ritocchi.clone();
                ritocchi.cancellature.extend(passate_senza_giri(&g.passate, p.giri, p.larghezza, p.altezza));
                let (foto, angoli, aspetto, giri, posto) = (p.foto.clone(), p.angoli, p.aspetto, p.giri, Some(s.corrente));
                diagnosi(format!("scansione: gomma, {} passate", g.passate.len()));
                self.fai_la_pagina(ctx, foto, angoli, ritocchi, aspetto, giri, posto);
            }
        }
    }

    /// Via dai bordi (o dalla foto che si prepara) senza farne pagine: la foto di adesso e quelle
    /// della galleria che aspettavano, che si cancellano dalla cache. Le pagine restano com'erano,
    /// e la scansione resta aperta.
    fn lascia_le_foto(&mut self) {
        let Some(s) = self.scansione.as_mut() else { return };
        s.bordi = None;
        s.preparazione = None;
        s.da_rifare = None;
        let altre = s.coda.len();
        for f in s.coda.drain(..) {
            crate::butta_la_foto(&f);
        }
        if altre == 0 {
            diagnosi("scansione: foto lasciata");
        } else {
            diagnosi(format!("scansione: foto lasciata, con altre {altre} della galleria"));
        }
    }

    /// La X o Indietro: con pagine fotografate il dialogo che chiede, senza si chiude.
    fn chiedi_o_chiudi_scansione(&mut self) {
        let Some(s) = self.scansione.as_mut() else { return };
        if s.pagine.is_empty() {
            self.chiudi_scansione();
        } else {
            diagnosi("scansione: buttare le pagine, chiesto");
            s.chiedi_di_buttare = true;
        }
    }

    /// Per le prove: la scansione aspetta una foto o che si prepari.
    #[doc(hidden)]
    pub fn scansione_attende(&self) -> bool {
        self.scansione.as_ref().is_some_and(|s| s.attende_foto.is_some() || s.preparazione.is_some())
    }

    /// Per le prove: «Sistema i bordi» è aperto.
    #[doc(hidden)]
    pub fn bordi_aperti(&self) -> bool {
        self.scansione.as_ref().is_some_and(|s| s.bordi.is_some())
    }

    /// Per le prove: gli angoli di «Sistema i bordi», nei pixel della foto.
    #[doc(hidden)]
    pub fn angoli_dei_bordi(&self) -> Option<Angoli> {
        self.scansione.as_ref()?.bordi.as_ref().map(|b| b.angoli)
    }

    /// Per le prove: i punti in mezzo ai lati di «Sistema i bordi».
    #[doc(hidden)]
    pub fn lati_dei_bordi(&self) -> Option<Lati> {
        self.scansione.as_ref()?.bordi.as_ref().map(|b| b.lati)
    }

    /// Per le prove: i ritocchi delle pagine fotografate (lati curvi, passate della gomma).
    #[doc(hidden)]
    pub fn ritocchi_scansionati(&self) -> Vec<Ritocchi> {
        self.scansione.as_ref().map_or(Vec::new(), |s| s.pagine.iter().map(|(p, _)| p.ritocchi.clone()).collect())
    }

    /// Per le prove: quante foto della galleria aspettano i loro bordi.
    #[doc(hidden)]
    pub fn foto_in_coda(&self) -> usize {
        self.scansione.as_ref().map_or(0, |s| s.coda.len())
    }

    fn chiudi_scansione(&mut self) {
        if let Some(mut s) = self.scansione.take() {
            for f in s.coda.drain(..) {
                crate::butta_la_foto(&f);
            }
            diagnosi(format!("scansione: chiusa con {}", pagine_diagnosi(s.pagine.len())));
        }
    }

    /// Prepara la foto nel file `file` della cache (si cancella appena letta) sul thread
    /// `scansione-foto`, per «Sistema i bordi»: `fonte` dice se lì c'è Rifai o Salta, `posizione`
    /// quale delle foto scelte insieme è.
    fn prepara_la_foto(&mut self, ctx: &egui::Context, file: PathBuf, fonte: FonteFoto, posizione: Option<(usize, usize)>) {
        let Some(s) = self.scansione.as_mut() else { return };
        // Rifai tiene l'aspetto della pagina che rifà; una pagina nuova quello scelto.
        let aspetto = s.da_rifare.and_then(|i| s.pagine.get(i)).map_or(s.aspetto, |(p, _)| p.aspetto);
        let formato = s.da_rifare.and_then(|i| s.pagine.get(i)).map_or(s.formato, |(p, _)| p.ritocchi.formato);
        let se_non_parte = file.clone();
        s.preparazione = sul_thread("scansione-foto", ctx, move || {
            let inizio = Instant::now();
            let letta = std::fs::File::open(&file).and_then(|f| {
                let mut v = Vec::new();
                std::io::Read::read_to_end(&mut std::io::Read::take(f, MASSIMO_FOTO + 1), &mut v).map(|_| v)
            });
            crate::butta_la_foto(&file);
            let preparata = (|| {
                let foto = match letta {
                    Ok(b) if b.len() as u64 <= MASSIMO_FOTO => prepara_foto(&b)?,
                    Ok(_) => return Err(ErroreFoto::Grande),
                    Err(_) => return Err(ErroreFoto::Lettura),
                };
                let immagine = foto.anteprima(LATO_FOTO_BORDI)?;
                let angoli = foto.angoli_di_partenza();
                let ritocchi = Ritocchi { lati: foto.lati_di_partenza(), cancellature: Vec::new(), formato };
                Ok(Preparata { foto: Arc::new(foto), immagine, angoli, ritocchi, aspetto, giri: 0, fonte, posizione })
            })();
            match &preparata {
                Ok(p) => diagnosi(format!(
                    "scansione: foto {} × {} dalla {}, {} KB, bordi trovati {}{}, in {:.1} s",
                    p.foto.larghezza,
                    p.foto.altezza,
                    fonte.nome(),
                    p.foto.jpeg.len() / 1024,
                    if p.foto.trovati.is_some() { "sì" } else { "no" },
                    if p.foto.lati_trovati.is_some() { ", lati curvi" } else { "" },
                    inizio.elapsed().as_secs_f32()
                )),
                Err(e) => diagnosi(format!("scansione: foto non preparata ({})", e.genere())),
            }
            preparata
        });
        if s.preparazione.is_none() {
            // Il thread non è partito: la foto non la legge nessuno.
            crate::butta_la_foto(&se_non_parte);
            self.avviso_breve.avvisa("La foto non si è potuta preparare.");
        }
    }

    /// Dopo una foto che non si è preparata: la prossima della galleria, se c'è; se no la pagina
    /// da rifare resta com'era.
    fn dopo_una_foto_che_non_va(&mut self, ctx: &egui::Context) {
        if !self.prepara_la_prossima(ctx)
            && let Some(s) = self.scansione.as_mut()
        {
            s.da_rifare = None;
        }
    }

    /// La prossima delle foto scelte insieme dalla galleria, se ce n'è una: vero.
    fn prepara_la_prossima(&mut self, ctx: &egui::Context) -> bool {
        let Some(s) = self.scansione.as_mut() else { return false };
        let Some(file) = s.coda.pop_front() else { return false };
        let posizione = (s.scelte.saturating_sub(s.coda.len()), s.scelte);
        self.prepara_la_foto(ctx, file, FonteFoto::Galleria, Some(posizione));
        true
    }

    /// La foto di una pagina fatta va in «Sistema i bordi» con gli angoli `angoli`: la sua immagine
    /// per lo schermo si fa sul thread `scansione-bordi`.
    fn apri_bordi(&mut self, ctx: &egui::Context, foto: Arc<FotoScansionata>, angoli: Angoli, ritocchi: Ritocchi, aspetto: Aspetto, giri: u8) {
        let Some(s) = self.scansione.as_mut() else { return };
        s.preparazione = sul_thread("scansione-bordi", ctx, move || {
            let immagine = foto.anteprima(LATO_FOTO_BORDI)?;
            Ok(Preparata { foto, immagine, angoli, ritocchi, aspetto, giri, fonte: FonteFoto::Fotocamera, posizione: None })
        });
    }

    /// La pagina dalla foto con gli angoli, l'aspetto e i giri dati, sul thread `scansione-pagina`;
    /// arriva al posto della pagina `posto`, o in fondo.
    #[allow(clippy::too_many_arguments)]
    fn fai_la_pagina(
        &mut self,
        ctx: &egui::Context,
        foto: Arc<FotoScansionata>,
        angoli: Angoli,
        ritocchi: Ritocchi,
        aspetto: Aspetto,
        giri: u8,
        posto: Option<usize>,
    ) {
        let Some(s) = self.scansione.as_mut() else { return };
        if s.lavorazione.is_some() {
            return;
        }
        let ricevitore = sul_thread("scansione-pagina", ctx, move || {
            let inizio = Instant::now();
            let pagina = fai_pagina(foto, angoli, ritocchi, aspetto, giri)?;
            let immagine = pagina.anteprima(LATO_PAGINA)?;
            diagnosi(format!(
                "scansione: pagina {} × {}, {}, {} giri, lati {}, {} passate di gomma, {} KB, in {:.1} s",
                pagina.larghezza,
                pagina.altezza,
                pagina.aspetto.chiave(),
                pagina.giri,
                if pagina.ritocchi.lati.is_some() { "curvi" } else { "dritti" },
                pagina.ritocchi.cancellature.len(),
                pagina.file.len() / 1024,
                inizio.elapsed().as_secs_f32()
            ));
            Ok((pagina, immagine))
        });
        s.lavorazione = ricevitore.map(|r| (r, posto));
    }

    /// A ogni frame: la foto della fotocamera, o le foto della galleria una alla volta, si
    /// preparano e aprono «Sistema i bordi»; la pagina fatta arriva al suo posto; il PDF creato si
    /// apre. Annullare la fotocamera o la galleria lascia la scansione com'era.
    pub(crate) fn passo_scansione(&mut self, ctx: &egui::Context) {
        if let Some(richiesta) = self.scansione.as_ref().and_then(|s| s.attende_foto)
            && let Some(esito) = self.piattaforma.prendi_foto_firma(richiesta)
        {
            let Some(s) = self.scansione.as_mut() else { return };
            s.attende_foto = None;
            match esito {
                EsitoFoto::Pronta(file) => self.prepara_la_foto(ctx, file, FonteFoto::Fotocamera, None),
                EsitoFoto::Pronte { file, scelte } => {
                    diagnosi(format!("scansione: {} foto dalla galleria su {scelte} scelte", file.len()));
                    // Una coda di prima (non dovrebbe esserci): le sue foto stanno nella cartella
                    // della sua richiesta, non in quella nuova.
                    for vecchia in s.coda.drain(..) {
                        crate::butta_la_foto(&vecchia);
                    }
                    let arrivate = file.len();
                    s.scelte = arrivate;
                    s.coda = file.into();
                    if scelte > arrivate {
                        let avviso = if scelte > MASSIMO_PAGINE_SCELTE && arrivate == MASSIMO_PAGINE_SCELTE {
                            format!(
                                "Ho preso le prime {MASSIMO_PAGINE_SCELTE} delle {scelte} foto scelte: le altre si aggiungono dopo, con Galleria."
                            )
                        } else if scelte > MASSIMO_PAGINE_SCELTE {
                            format!("Ho preso {arrivate} delle {scelte} foto scelte: al più {MASSIMO_PAGINE_SCELTE}, e qualcuna non si è letta.")
                        } else {
                            format!("{} delle foto scelte non si sono lette: ho preso le altre {arrivate}.", scelte - arrivate)
                        };
                        self.avviso_breve.avvisa(avviso);
                    }
                    self.prepara_la_prossima(ctx);
                }
                EsitoFoto::Annullata => {
                    diagnosi("scansione: foto annullata");
                    // La pagina da rifare resta com'era.
                    s.da_rifare = None;
                }
                EsitoFoto::Superata => {
                    diagnosi("scansione: foto superata da un'altra richiesta");
                    s.da_rifare = None;
                }
                EsitoFoto::Errore => {
                    diagnosi("scansione: foto non arrivata");
                    s.da_rifare = None;
                    self.avviso_breve.avvisa("La foto non è arrivata.");
                }
            }
        }
        let Some(s) = self.scansione.as_mut() else { return };
        if let Some(rx) = s.preparazione.as_ref() {
            match rx.try_recv() {
                Ok(Ok(p)) => {
                    s.preparazione = None;
                    // I bordi vanno sopra tutto: niente dialoghi o fogli rimasti aperti sotto.
                    s.chiedi_di_buttare = false;
                    s.togli = None;
                    s.scegli_aspetto = false;
                    let texture = ctx.load_texture("scansione_foto", p.immagine, egui::TextureOptions::LINEAR);
                    let lati = p.ritocchi.lati.unwrap_or_else(|| lati_dritti(p.angoli));
                    s.bordi = Some(Bordi {
                        foto: p.foto,
                        texture,
                        angoli: p.angoli,
                        partenza: p.angoli,
                        lati,
                        cancellature: p.ritocchi.cancellature,
                        formato: p.ritocchi.formato,
                        lati_di_partenza: lati,
                        formato_di_partenza: p.ritocchi.formato,
                        preso: None,
                        aspetto: p.aspetto,
                        giri: p.giri,
                        fonte: p.fonte,
                        posizione: p.posizione,
                        posto: Rect::NOTHING,
                        scala: 1.0,
                    });
                }
                Ok(Err(e)) => {
                    s.preparazione = None;
                    self.avviso_breve.avvisa(e.messaggio());
                    self.dopo_una_foto_che_non_va(ctx);
                }
                Err(TryRecvError::Disconnected) => {
                    // Il thread si è fermato senza dirlo: come una foto che non si legge.
                    s.preparazione = None;
                    diagnosi("scansione: preparazione della foto fermata");
                    self.avviso_breve.avvisa(ErroreFoto::Lettura.messaggio());
                    self.dopo_una_foto_che_non_va(ctx);
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        let Some(s) = self.scansione.as_mut() else { return };
        if let Some((rx, posto)) = s.lavorazione.as_ref() {
            let posto = *posto;
            match rx.try_recv() {
                Ok(Ok((pagina, immagine))) => {
                    s.lavorazione = None;
                    s.porta_in_vista = true;
                    // Rifai, Bordi, l'aspetto, Ruota: la pagina nuova va al posto di quella, solo adesso.
                    let posto = posto.filter(|i| *i < s.pagine.len());
                    let anteprima =
                        ctx.load_texture(format!("scansione_{}", posto.unwrap_or(s.pagine.len())), immagine, egui::TextureOptions::LINEAR);
                    match posto {
                        Some(i) => {
                            s.pagine[i] = (pagina, Some(anteprima));
                            s.corrente = i;
                        }
                        None => {
                            s.pagine.push((pagina, Some(anteprima)));
                            s.corrente = s.pagine.len() - 1;
                        }
                    }
                }
                Ok(Err(e)) => {
                    s.lavorazione = None;
                    self.avviso_breve.avvisa(e.messaggio());
                }
                Err(TryRecvError::Disconnected) => {
                    s.lavorazione = None;
                    diagnosi("scansione: pagina fermata");
                    self.avviso_breve.avvisa("La pagina non si è potuta fare.");
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        let Some(s) = self.scansione.as_mut() else { return };
        let Some(c) = s.creazione.as_ref() else { return };
        let Some(esito) = c.esito() else { return };
        let secondi = c.inizio.elapsed().as_secs_f32();
        s.creazione = None;
        match esito {
            Some(Ok((percorso, parole))) => {
                let n = s.pagine.len();
                diagnosi(format!("scansione: pdf di {} con {parole} parole in {secondi:.1} s", pagine_diagnosi(n)));
                self.chiudi_scansione();
                let nome = percorso.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                self.apri_la_scansione(percorso, nome);
            }
            Some(Err("fermata")) => diagnosi("scansione: pdf non creato, fermato"),
            Some(Err(genere)) => {
                diagnosi(format!("scansione: pdf non creato ({genere})"));
                self.errore = Some(Errore { titolo: SCANSIONE_NON_FATTA, testo: "Non riesco a creare il PDF della scansione.".into() });
            }
            None => self.errore = Some(Errore { titolo: SCANSIONE_NON_FATTA, testo: "La creazione del PDF si è fermata.".into() }),
        }
    }

    /// «Crea PDF»: il PDF con l'OCR sul thread `scansione-pdf`, poi in Documenti/Tascabile.
    fn crea_pdf_della_scansione(&mut self, ctx: &egui::Context) {
        let Some(s) = self.scansione.as_ref() else { return };
        if s.pagine.is_empty() || s.creazione.is_some() {
            return;
        }
        let pagine: Vec<PaginaScansionata> = s.pagine.iter().map(|(p, _)| p.clone()).collect();
        let cartella = cartella_tascabile(&self.piattaforma.radice_archivio());
        let (anno, mese, giorno, ora, minuti) = salva::ora_locale(crate::adesso() as i64);
        let base = format!("Scansione {giorno:02}-{mese:02}-{anno} {ora:02}.{minuti:02}");
        let modelli = modelli_se_servono(&self.piattaforma);
        diagnosi(format!("scansione: crea il pdf di {}", pagine_diagnosi(pagine.len())));
        let creazione = Lavoro::avvia("scansione-pdf", ctx, move |avanti| {
            // Senza modelli il PDF si fa lo stesso, senza testo.
            let ocr = riconoscitore(modelli).ok();
            let (byte, parole) = crea_pdf(&pagine, ocr.as_deref(), avanti).map_err(|e| match e {
                ErroreScansione::Fermata => "fermata",
                ErroreScansione::Vuota => "vuota",
                ErroreScansione::Motore(g) => g,
            })?;
            std::fs::create_dir_all(&cartella).map_err(|_| "cartella")?;
            let percorso = salva::scrivi_nuovo(&cartella, &base, "pdf", &byte).map_err(|_| "scrittura")?;
            Ok((percorso, parole))
        });
        if let Some(s) = self.scansione.as_mut() {
            s.creazione = creazione;
        }
    }

    /// La pagina che si vede, rifatta dalla sua foto con l'aspetto e i giri dati (Ruota, gli
    /// aspetti): arriva al suo posto. La foto non si sciupa: ogni volta si riparte da lei.
    fn rifai_la_corrente(&mut self, ctx: &egui::Context, aspetto: Option<Aspetto>, giro: bool) {
        let Some(s) = self.scansione.as_mut() else { return };
        if s.lavorazione.is_some() || s.preparazione.is_some() || s.attende_foto.is_some() || s.creazione.is_some() {
            return;
        }
        let Some((p, _)) = s.pagine.get(s.corrente) else { return };
        let (foto, angoli, ritocchi) = (p.foto.clone(), p.angoli, p.ritocchi.clone());
        let aspetto = aspetto.unwrap_or(p.aspetto);
        let giri = (p.giri + u8::from(giro)) % 4;
        let posto = Some(s.corrente);
        self.fai_la_pagina(ctx, foto, angoli, ritocchi, aspetto, giri, posto);
    }

    /// La scansione a tutto schermo: «Sistema i bordi» se è aperto. Se no, in alto Chiudi, il titolo
    /// e Crea PDF; la pagina che si vede (o la riga che dice che cosa si aspetta, o come si
    /// comincia), le miniature da toccare; sotto la barra degli strumenti della pagina (Bordi,
    /// Ruota, Aspetto, Rifai, Togli); in fondo Fotocamera e Galleria. In orizzontale gli strumenti
    /// e le due fonti stanno in una colonna a destra.
    pub(crate) fn mostra_scansione(&mut self, ctx: &egui::Context) {
        enum Tocco {
            Chiudi,
            Crea,
            Fotocamera,
            Galleria,
            Strumento(usize),
            Gomma(ToccoGomma),
            Pagina(usize),
            Aspetto(Aspetto),
            ChiudiAspetti,
        }
        if self.scansione.as_ref().is_some_and(|s| s.bordi.is_some()) {
            self.mostra_bordi(ctx);
            return;
        }
        let Some(s) = self.scansione.as_ref() else { return };
        let schermo = ctx.content_rect();
        let orizzontale = schermo.width() > schermo.height();
        let mut tocco = None;
        egui::Area::new(Id::new("scansione")).order(Order::Foreground).fixed_pos(schermo.min).constrain(false).show(ctx, |ui| {
            ui.set_min_size(schermo.size());
            let _ = ui.interact(schermo, Id::new("scansione_fondo"), Sense::click_and_drag());
            ui.painter().rect_filled(schermo, 0.0, tema::CARTA);
            let barra = Rect::from_min_size(schermo.min, vec2(schermo.width(), tema::BARRA_ALTA));
            // Quante pagine, sotto il titolo: accanto, con Crea PDF a 372 dp diventava «2 pagi…».
            let quante = pagine(s.pagine.len());
            let occupata = s.preparazione.is_some() || s.lavorazione.is_some() || s.attende_foto.is_some() || s.creazione.is_some() || s.gomma.is_some();
            let mut alta = BarraAlta::new(SCANSIONE).indietro(CHIUDI);
            if !s.pagine.is_empty() {
                alta = alta.sottotitolo(&quante).principale(Pulsante::new(CREA_PDF, Peso::Principale).icona(icone::CHECK).spento(occupata));
            }
            let r = ui.scope_builder(UiBuilder::new().max_rect(barra), |ui| alta.mostra(ui)).inner;
            if r.indietro {
                tocco = Some(Tocco::Chiudi);
            }
            if r.principale {
                tocco = Some(Tocco::Crea);
            }
            // I posti: la pagina e le miniature; gli strumenti; le due fonti.
            let con_pagine = !s.pagine.is_empty();
            let (zona_pagine, strumenti, fonti) = if orizzontale {
                let colonna = Rect::from_min_max(
                    pos2(schermo.right() - 12.0 - COLONNA, barra.bottom() + 8.0),
                    pos2(schermo.right() - 12.0, schermo.bottom() - 12.0),
                );
                let fonti = Rect::from_min_max(pos2(colonna.left(), colonna.bottom() - 2.0 * tema::PULSANTE_LARGO - 8.0), colonna.max);
                let strumenti = Rect::from_min_size(colonna.min, vec2(colonna.width(), tema::VOCI));
                (
                    Rect::from_min_max(pos2(schermo.left() + 16.0, barra.bottom() + 8.0), pos2(colonna.left() - 16.0, schermo.bottom() - 12.0)),
                    strumenti,
                    fonti,
                )
            } else {
                let fonti = Rect::from_min_max(
                    pos2(schermo.left() + 12.0, schermo.bottom() - 12.0 - tema::PULSANTE_LARGO),
                    pos2(schermo.right() - 12.0, schermo.bottom() - 12.0),
                );
                let strumenti = Rect::from_min_max(pos2(fonti.left(), fonti.top() - 10.0 - tema::VOCI), pos2(fonti.right(), fonti.top() - 10.0));
                let sotto = if con_pagine { strumenti.top() } else { fonti.top() };
                (Rect::from_min_max(pos2(schermo.left() + 16.0, barra.bottom() + 8.0), pos2(schermo.right() - 16.0, sotto - 10.0)), strumenti, fonti)
            };
            let striscia = match (s.pagine.len() > 1, orizzontale) {
                (false, _) => 0.0,
                (true, true) => 64.0,
                (true, false) => 84.0,
            };
            let zona = Rect::from_min_max(zona_pagine.min, pos2(zona_pagine.right(), zona_pagine.bottom() - striscia));
            let riga = |ui: &egui::Ui, testo: &str| {
                ui.painter().text(zona.center(), egui::Align2::CENTER_CENTER, testo, Carattere::Testo.font(ui.ctx(), 16.0), tema::MATITA);
            };
            // La pagina che si fa per una foto nuova si aspetta; una pagina che si rifà si vede
            // ancora com'era, velata.
            let nuova = s.lavorazione.as_ref().is_some_and(|(_, posto)| posto.is_none());
            match (s.pagine.get(s.corrente), s.preparazione.is_some(), nuova, s.attende_foto.is_some()) {
                (_, true, _, _) => {
                    riga(ui, "Preparo la foto");
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(150));
                }
                (_, false, true, _) => {
                    riga(ui, "Raddrizzo e pulisco la pagina");
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(150));
                }
                (None, false, false, true) => riga(ui, "Aspetto la foto"),
                (Some((p, Some(t))), false, false, _) => {
                    let proporzione = p.larghezza as f32 / p.altezza.max(1) as f32;
                    let mut misura = vec2(zona.height() * proporzione, zona.height());
                    if misura.x > zona.width() {
                        misura = vec2(zona.width(), zona.width() / proporzione);
                    }
                    let foglio = Rect::from_center_size(zona.center(), misura);
                    ui.painter().rect_filled(foglio.expand(1.0), 2.0, tema::FILO);
                    ui.painter().image(t.id(), foglio, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    // La gomma: il dito sulla pagina lascia il bianco; le passate si vedono subito,
                    // e la pagina si rifà con «Fatto».
                    if let Some(g) = &s.gomma {
                        let raggio = RAGGI_GOMMA[g.misura.min(2)];
                        let gesto = ui.interact(foglio, Id::new("scansione_gomma"), Sense::click_and_drag());
                        gesto.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, SPIEGA_GOMMA));
                        let nella_pagina = |d: Pos2| [((d.x - foglio.left()) / foglio.width()).clamp(0.0, 1.0), ((d.y - foglio.top()) / foglio.height()).clamp(0.0, 1.0)];
                        let frazione = raggio / foglio.width().max(1.0);
                        if let Some(d) = gesto.interact_pointer_pos() {
                            if gesto.clicked() {
                                tocco = Some(Tocco::Gomma(ToccoGomma::Tocco(nella_pagina(d), frazione)));
                            } else if gesto.drag_started() || gesto.dragged() {
                                tocco = Some(Tocco::Gomma(ToccoGomma::Punto(nella_pagina(d))));
                            }
                        }
                        if gesto.drag_stopped() {
                            tocco = Some(Tocco::Gomma(ToccoGomma::Fine(frazione)));
                        }
                        let a_schermo = |p: &[f32; 2]| pos2(foglio.left() + p[0] * foglio.width(), foglio.top() + p[1] * foglio.height());
                        let painter = ui.painter_at(foglio);
                        let passate = g.passate.iter().map(|c| (c.punti.as_slice(), c.raggio * foglio.width())).chain(std::iter::once((g.tratto.as_slice(), raggio)));
                        for (punti, r) in passate {
                            let punti: Vec<Pos2> = punti.iter().map(a_schermo).collect();
                            for p in &punti {
                                painter.circle_filled(*p, r, tema::FOGLIO);
                            }
                            if punti.len() > 1 {
                                painter.add(egui::Shape::line(punti, Stroke::new(2.0 * r, tema::FOGLIO)));
                            }
                        }
                        let spiega = ui.painter().layout_no_wrap(SPIEGA_GOMMA.into(), Carattere::Testo.font(ui.ctx(), 13.0), tema::INCHIOSTRO);
                        let pillola = Rect::from_center_size(pos2(foglio.center().x, foglio.top() + 18.0), spiega.size() + vec2(20.0, 10.0));
                        ui.painter().rect_filled(pillola, 12.0, Color32::from_white_alpha(230));
                        ui.painter().galley(pillola.center() - spiega.size() / 2.0, spiega, tema::INCHIOSTRO);
                    }
                    if s.lavorazione.is_some() {
                        ui.painter().rect_filled(foglio, 2.0, Color32::from_white_alpha(150));
                        ui.painter().text(
                            foglio.center(),
                            egui::Align2::CENTER_CENTER,
                            "Preparo la pagina",
                            Carattere::Grassetto.font(ui.ctx(), 16.0),
                            tema::INCHIOSTRO,
                        );
                        ui.ctx().request_repaint_after(std::time::Duration::from_millis(150));
                    }
                }
                (None, false, false, false) => {
                    // Come si comincia: l'icona e la spiegazione, sopra le due fonti.
                    let ctx = ui.ctx().clone();
                    ui.painter().text(
                        zona.center() - vec2(0.0, 48.0),
                        egui::Align2::CENTER_CENTER,
                        icone::DOCUMENT_SCANNER,
                        Carattere::Icona.font(&ctx, 56.0),
                        tema::MATITA,
                    );
                    let testo = Rect::from_center_size(zona.center() + vec2(0.0, 30.0), vec2(zona.width().min(320.0), 80.0));
                    ui.scope_builder(UiBuilder::new().max_rect(testo), |ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(egui::RichText::new(PER_COMINCIARE).font(Carattere::Testo.font(ui.ctx(), 15.0)).color(tema::MATITA));
                        });
                    });
                }
                _ => riga(ui, "Nessuna pagina"),
            }
            // Le pagine in piccolo, in una fila che scorre col dito (con la galleria possono essere
            // trenta): quella che si vede ha il bordo blu, e quando cambia viene in vista.
            if striscia > 0.0 {
                let fila = Rect::from_min_max(pos2(zona_pagine.left(), zona_pagine.bottom() - striscia + 6.0), zona_pagine.max);
                let alto = striscia - 14.0;
                ui.scope_builder(UiBuilder::new().max_rect(fila), |ui| {
                    egui::ScrollArea::horizontal()
                        .id_salt("scansione_miniature")
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 10.0;
                                // Un margine attorno, per il bordo blu.
                                ui.add_space(3.0);
                                for (i, (p, t)) in s.pagine.iter().enumerate() {
                                    let largo = alto * p.larghezza as f32 / p.altezza.max(1) as f32;
                                    let (r, risposta) = ui.allocate_exact_size(vec2(largo, alto), Sense::click());
                                    let nome = format!("Pagina {}", i + 1);
                                    risposta.widget_info(|| WidgetInfo::selected(WidgetType::Button, true, i == s.corrente, &nome));
                                    if risposta.clicked() && !occupata {
                                        tocco = Some(Tocco::Pagina(i));
                                    }
                                    if i == s.corrente && s.porta_in_vista {
                                        risposta.scroll_to_me(Some(egui::Align::Center));
                                    }
                                    let bordo = if i == s.corrente { Stroke::new(2.5, tema::BLU) } else { Stroke::new(1.0, tema::FILO) };
                                    if let Some(t) = t {
                                        ui.painter().image(t.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                                    }
                                    ui.painter().rect_stroke(r.expand(1.0), 2.0, bordo, egui::StrokeKind::Outside);
                                    ui.painter().text(
                                        r.center_bottom() + vec2(0.0, -8.0),
                                        egui::Align2::CENTER_CENTER,
                                        format!("{}", i + 1),
                                        Carattere::Testo.font(ui.ctx(), 12.0),
                                        tema::MATITA,
                                    );
                                }
                                ui.add_space(3.0);
                            });
                        });
                });
            }
            // Gli strumenti della pagina che si vede; con la gomma aperta i suoi.
            if let Some(g) = &s.gomma {
                let barra = BarraVoci::new()
                    .voce(icone::UNDO, ANNULLA_PASSATA)
                    .voce(icone::LINE_WEIGHT, MISURE_GOMMA[0])
                    .voce(icone::LINE_WEIGHT, MISURE_GOMMA[1])
                    .voce(icone::LINE_WEIGHT, MISURE_GOMMA[2])
                    .voce(icone::CHECK, FATTO)
                    .accesa(Some(1 + g.misura.min(2)));
                if let Some(i) = barra.mostra(ui, strumenti) {
                    tocco = Some(Tocco::Gomma(match i {
                        0 => ToccoGomma::Annulla,
                        1..=3 => ToccoGomma::Misura(i - 1),
                        _ => ToccoGomma::Fatto,
                    }));
                }
            } else if con_pagine {
                ui.add_enabled_ui(!occupata, |ui| {
                    let barra = STRUMENTI.iter().fold(BarraVoci::new(), |b, (icona, nome)| b.voce(icona, nome));
                    if let Some(i) = barra.mostra(ui, strumenti) {
                        tocco = Some(Tocco::Strumento(i));
                    }
                });
            }
            // Le due fonti: metà e metà in verticale, una sopra l'altra in orizzontale.
            let (camera, galleria) = if orizzontale {
                let alto = tema::PULSANTE_LARGO;
                (
                    Rect::from_min_size(fonti.min, vec2(fonti.width(), alto)),
                    Rect::from_min_size(pos2(fonti.left(), fonti.top() + alto + 8.0), vec2(fonti.width(), alto)),
                )
            } else {
                let mezzo = (fonti.width() - 8.0) / 2.0;
                (
                    Rect::from_min_size(fonti.min, vec2(mezzo, fonti.height())),
                    Rect::from_min_size(pos2(fonti.left() + mezzo + 8.0, fonti.top()), vec2(mezzo, fonti.height())),
                )
            };
            if ui.put(camera, Pulsante::new(FOTOCAMERA, Peso::Tenue).icona(icone::PHOTO_CAMERA).larghezza(camera.width()).spento(occupata)).clicked()
            {
                tocco = Some(Tocco::Fotocamera);
            }
            if ui.put(galleria, Pulsante::new(GALLERIA, Peso::Tenue).icona(icone::IMAGE).larghezza(galleria.width()).spento(occupata)).clicked() {
                tocco = Some(Tocco::Galleria);
            }
            // Il foglio dei quattro aspetti, sopra tutto, come gli altri fogli dal fondo (la
            // lineetta, il titolo, la X): un tocco fuori lo chiude.
            if s.scegli_aspetto
                && let Some((p, _)) = s.pagine.get(s.corrente)
            {
                let velo = ui.interact(schermo, Id::new("scansione_aspetti_velo"), Sense::click());
                ui.painter().rect_filled(schermo, 0.0, tema::VELO_SCURO);
                let largo = schermo.width().min(480.0);
                let alto_riga = 64.0;
                // La lineetta e il titolo, le righe, il margine in fondo.
                let testa = 10.0 + 4.0 + 10.0 + tema::PULSANTE;
                let alto = testa + 4.0 * alto_riga + 14.0;
                let pannello = Rect::from_min_size(pos2(schermo.center().x - largo / 2.0, schermo.bottom() - alto), vec2(largo, alto));
                let _ = ui.interact(pannello, Id::new("scansione_aspetti_foglio"), Sense::click());
                let raggio = tema::RAGGIO_FOGLIO;
                ui.painter().rect_filled(pannello, egui::CornerRadius { nw: raggio, ne: raggio, sw: 0, se: 0 }, tema::FOGLIO);
                let ctx = ui.ctx().clone();
                ui.painter().rect_filled(
                    Rect::from_center_size(pos2(pannello.center().x, pannello.top() + 12.0), vec2(36.0, 4.0)),
                    2.0,
                    tema::LINEETTA,
                );
                let riga_titolo = Rect::from_min_size(pannello.left_top() + vec2(18.0, 24.0), vec2(largo - 36.0, tema::PULSANTE));
                let titolo = ui.painter().layout_no_wrap(ASPETTO_DELLA_PAGINA.to_string(), Carattere::Titolo.font(&ctx, 23.0), tema::INCHIOSTRO);
                let posto_titolo = Rect::from_min_size(pos2(riga_titolo.left(), riga_titolo.center().y - titolo.size().y / 2.0), titolo.size());
                ui.painter().galley(posto_titolo.min, titolo, tema::INCHIOSTRO);
                ui.allocate_rect(posto_titolo, Sense::hover()).widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, ASPETTO_DELLA_PAGINA));
                if Carattere::Icona.pronto(&ctx) {
                    let x = Rect::from_min_size(pos2(riga_titolo.right() - tema::PULSANTE, riga_titolo.top()), vec2(tema::PULSANTE, tema::PULSANTE));
                    if ui.put(x, PulsanteIcona::new(icone::CLOSE, CHIUDI_ASPETTO)).clicked() {
                        tocco = Some(Tocco::ChiudiAspetti);
                    }
                }
                for (i, a) in Aspetto::TUTTI.into_iter().enumerate() {
                    let posto = Rect::from_min_size(pannello.left_top() + vec2(8.0, testa + i as f32 * alto_riga), vec2(largo - 16.0, alto_riga));
                    let r = ui.interact(posto, Id::new(("scansione_aspetto", i)), Sense::click());
                    let scelto = p.aspetto == a;
                    r.widget_info(|| WidgetInfo::selected(WidgetType::Button, true, scelto, a.nome()));
                    if r.is_pointer_button_down_on() {
                        ui.painter().rect_filled(posto, 12.0, tema::PREMUTO);
                    }
                    let (icona, spiega) = spiega_aspetto(a);
                    let colore = if scelto { tema::BLU } else { tema::INCHIOSTRO };
                    // L'icona dal margine del titolo, il testo dopo.
                    ui.painter().text(
                        posto.left_center() + vec2(10.0 + tema::ICONA / 2.0, 0.0),
                        egui::Align2::CENTER_CENTER,
                        icona,
                        Carattere::Icona.font(&ctx, tema::ICONA),
                        if scelto { tema::BLU } else { tema::MATITA },
                    );
                    let testo = 10.0 + tema::ICONA + 16.0;
                    let dentro = posto.width() - testo - 40.0;
                    let nome = ui.painter().layout_no_wrap(a.nome().to_string(), Carattere::Grassetto.font(&ctx, 16.0), colore);
                    let sotto = ui.painter().layout(spiega.to_string(), Carattere::Testo.font(&ctx, 13.0), tema::MATITA, dentro);
                    let cima = posto.center().y - (nome.size().y + 2.0 + sotto.size().y) / 2.0;
                    let dove = pos2(posto.left() + testo, cima + nome.size().y + 2.0);
                    ui.allocate_rect(Rect::from_min_size(dove, sotto.size()), Sense::hover())
                        .widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, spiega));
                    ui.painter().galley(pos2(posto.left() + testo, cima), nome.clone(), colore);
                    ui.painter().galley(dove, sotto, tema::MATITA);
                    if scelto {
                        ui.painter().text(
                            posto.right_center() - vec2(24.0, 0.0),
                            egui::Align2::CENTER_CENTER,
                            icone::CHECK,
                            Carattere::Icona.font(&ctx, tema::ICONA),
                            tema::BLU,
                        );
                    }
                    if r.clicked() {
                        tocco = Some(Tocco::Aspetto(a));
                    }
                }
                if velo.clicked() && tocco.is_none() {
                    tocco = Some(Tocco::ChiudiAspetti);
                }
            }
        });
        if let Some(s) = self.scansione.as_mut() {
            s.porta_in_vista = false;
        }
        let annulla = self.scansione.as_ref().and_then(|s| s.creazione.as_ref()).is_some_and(|c| mostra_avanzamento(ctx, "scansione_pdf", CREO, c));
        if annulla {
            self.indietro_scansione();
        }
        // «Buttare le pagine fotografate?»: Annulla resta, Butta chiude.
        if let Some(s) = self.scansione.as_ref()
            && s.chiedi_di_buttare
        {
            let testo = match s.pagine.len() {
                1 => "La pagina fotografata non è ancora in un PDF.".to_string(),
                n => format!("Le {n} pagine fotografate non sono ancora in un PDF."),
            };
            let r = Dialogo::new(Id::new("scansione_buttare"), BUTTARE)
                .testo(&testo)
                .pulsante(ANNULLA_SCANSIONE, Peso::Testo)
                .pulsante(BUTTA, Peso::Principale)
                .mostra(ctx);
            match r {
                Some(RispostaDialogo::Pulsante(1)) => {
                    diagnosi("scansione: pagine buttate");
                    self.chiudi_scansione();
                    return;
                }
                Some(_) => {
                    if let Some(s) = self.scansione.as_mut() {
                        s.chiedi_di_buttare = false;
                    }
                    diagnosi("scansione: pagine tenute");
                }
                None => {}
            }
            return;
        }
        // «Togliere la pagina?»: Annulla resta, Togli la pagina la toglie.
        if let Some(i) = self.scansione.as_ref().and_then(|s| s.togli) {
            let titolo = format!("Togliere la pagina {}?", i + 1);
            let r = Dialogo::new(Id::new("scansione_togli"), &titolo)
                .testo("Si toglie dalla scansione: per rimetterla, rifai la foto o sceglila dalla galleria.")
                .pulsante(ANNULLA_SCANSIONE, Peso::Testo)
                .pulsante(TOGLI_LA_PAGINA, Peso::Principale)
                .mostra(ctx);
            if let Some(scelta) = r
                && let Some(s) = self.scansione.as_mut()
            {
                s.togli = None;
                if scelta == RispostaDialogo::Pulsante(1) && i < s.pagine.len() {
                    s.pagine.remove(i);
                    s.corrente = s.corrente.min(s.pagine.len().saturating_sub(1));
                    s.porta_in_vista = true;
                    diagnosi(format!("scansione: pagina tolta, ne restano {}", s.pagine.len()));
                } else {
                    diagnosi("scansione: pagina tenuta");
                }
            }
            return;
        }
        match tocco {
            // La X fa come Indietro: ferma, lascia o chiede, secondo che cosa sta succedendo.
            Some(Tocco::Chiudi) => {
                self.indietro_scansione();
            }
            Some(Tocco::Crea) => self.crea_pdf_della_scansione(ctx),
            Some(Tocco::Fotocamera) => {
                if let Some(s) = self.scansione.as_mut() {
                    s.da_rifare = None;
                }
                diagnosi("scansione: pagina dalla fotocamera");
                self.scatta();
            }
            Some(Tocco::Galleria) => {
                diagnosi("scansione: pagine dalla galleria");
                self.scegli_dalla_galleria();
            }
            Some(Tocco::Strumento(0)) => {
                let Some(s) = self.scansione.as_mut() else { return };
                let Some((p, _)) = s.pagine.get(s.corrente) else { return };
                let (foto, angoli, ritocchi, aspetto, giri) = (p.foto.clone(), p.angoli, p.ritocchi.clone(), p.aspetto, p.giri);
                s.da_rifare = Some(s.corrente);
                diagnosi("scansione: bordi della pagina da rifare");
                self.apri_bordi(ctx, foto, angoli, ritocchi, aspetto, giri);
            }
            Some(Tocco::Strumento(1)) => {
                diagnosi("scansione: pagina girata");
                self.rifai_la_corrente(ctx, None, true);
            }
            Some(Tocco::Strumento(2)) => {
                if let Some(s) = self.scansione.as_mut() {
                    s.scegli_aspetto = true;
                }
            }
            Some(Tocco::Strumento(3)) => {
                if let Some(s) = self.scansione.as_mut()
                    && s.corrente < s.pagine.len()
                {
                    diagnosi("scansione: gomma aperta");
                    s.gomma = Some(Gomma { misura: 1, ..Gomma::default() });
                }
            }
            Some(Tocco::Strumento(4)) => {
                if let Some(s) = self.scansione.as_mut() {
                    s.da_rifare = Some(s.corrente).filter(|i| *i < s.pagine.len());
                    diagnosi("scansione: pagina da rifare");
                }
                self.scatta();
            }
            Some(Tocco::Gomma(g)) => self.tocco_della_gomma(ctx, g),
            Some(Tocco::Strumento(_)) => {
                if let Some(s) = self.scansione.as_mut() {
                    s.togli = Some(s.corrente).filter(|i| *i < s.pagine.len());
                }
            }
            Some(Tocco::Aspetto(a)) => {
                let Some(s) = self.scansione.as_mut() else { return };
                s.scegli_aspetto = false;
                if s.pagine.get(s.corrente).is_some_and(|(p, _)| p.aspetto != a) {
                    diagnosi(format!("scansione: aspetto {}", a.chiave()));
                    // Vale anche per le pagine che vengono dopo.
                    s.aspetto = a;
                    self.rifai_la_corrente(ctx, Some(a), false);
                }
            }
            Some(Tocco::ChiudiAspetti) => {
                if let Some(s) = self.scansione.as_mut() {
                    s.scegli_aspetto = false;
                }
            }
            Some(Tocco::Pagina(i)) => {
                if let Some(s) = self.scansione.as_mut() {
                    s.corrente = i;
                    s.porta_in_vista = true;
                }
            }
            None => {}
        }
    }

    /// «Sistema i bordi»: la foto intera col velo fuori dal foglio, i lati (rossi se gli angoli si
    /// incrociano) e i quattro angoli da trascinare col dito, con la lente sopra il dito; sotto la
    /// spiegazione, Tutta la foto e Rifai (per una foto della galleria Salta), e Fatto. In
    /// orizzontale la spiegazione e i pulsanti stanno in una colonna a destra.
    fn mostra_bordi(&mut self, ctx: &egui::Context) {
        enum Tocco {
            Lascia,
            Tutta,
            Rifai,
            Salta,
            Fatto,
            Formato(Formato),
        }
        let (pagina_di_prima, altre) = self.scansione.as_ref().map_or((false, 0), |s| (s.lavorazione.is_some(), s.coda.len()));
        let Some(b) = self.scansione.as_mut().and_then(|s| s.bordi.as_mut()) else { return };
        let schermo = ctx.content_rect();
        let orizzontale = schermo.width() > schermo.height();
        let mut tocco = None;
        let (fw, fh) = (b.foto.larghezza as f32, b.foto.altezza as f32);
        egui::Area::new(Id::new("scansione_bordi")).order(Order::Foreground).fixed_pos(schermo.min).constrain(false).show(ctx, |ui| {
            ui.set_min_size(schermo.size());
            let _ = ui.interact(schermo, Id::new("scansione_bordi_fondo"), Sense::click_and_drag());
            ui.painter().rect_filled(schermo, 0.0, tema::CARTA);
            let barra = Rect::from_min_size(schermo.min, vec2(schermo.width(), tema::BARRA_ALTA));
            let titolo = match b.posizione {
                Some((n, di)) if di > 1 => format!("{SISTEMA_I_BORDI} · {n} di {di}"),
                _ => SISTEMA_I_BORDI.to_string(),
            };
            let lascia = if altre > 0 { LASCIA_LE_FOTO } else { LASCIA_LA_FOTO };
            let r = ui.scope_builder(UiBuilder::new().max_rect(barra), |ui| BarraAlta::new(&titolo).indietro(lascia).mostra(ui)).inner;
            if r.indietro {
                tocco = Some(Tocco::Lascia);
            }
            // I posti: la foto, con un margine per prendere gli angoli sui bordi; i comandi.
            // La spiegazione, i formati del foglio, Tutta la foto e Rifai, Fatto.
            let comandi_alti = 44.0 + ALTO_FORMATI + tema::PULSANTE_LARGO + 8.0 + tema::PULSANTE_LARGO;
            let (zona, comandi) = if orizzontale {
                let colonna = Rect::from_min_max(
                    pos2(schermo.right() - 12.0 - COLONNA, barra.bottom() + 12.0),
                    pos2(schermo.right() - 12.0, schermo.bottom() - 12.0),
                );
                (
                    Rect::from_min_max(pos2(schermo.left() + 28.0, barra.bottom() + 20.0), pos2(colonna.left() - 24.0, schermo.bottom() - 20.0)),
                    colonna,
                )
            } else {
                let comandi = Rect::from_min_max(
                    pos2(schermo.left() + 12.0, schermo.bottom() - 12.0 - comandi_alti),
                    pos2(schermo.right() - 12.0, schermo.bottom() - 12.0),
                );
                (Rect::from_min_max(pos2(schermo.left() + 28.0, barra.bottom() + 20.0), pos2(schermo.right() - 28.0, comandi.top() - 16.0)), comandi)
            };
            let k = (zona.width() / fw.max(1.0)).min(zona.height() / fh.max(1.0));
            let posto = Rect::from_center_size(zona.center(), vec2(fw * k, fh * k));
            (b.posto, b.scala) = (posto, k);
            ui.painter().image(b.texture.id(), posto, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            // Il dito: prende l'angolo più vicino e lo porta, a scarto costante, dentro la foto.
            let area = ui.interact(posto.expand(PRESA / 2.0), Id::new("scansione_bordi_foto"), Sense::drag());
            let a_schermo = |p: [f32; 2]| pos2(posto.left() + p[0] * k, posto.top() + p[1] * k);
            let in_foto = |p: Pos2| [(p.x - posto.left()) / k, (p.y - posto.top()) / k];
            if area.drag_started()
                && let Some(origine) = ui.input(|i| i.pointer.press_origin())
            {
                // Gli angoli e i punti in mezzo ai lati: si prende il più vicino al dito.
                let punti: Vec<[f32; 2]> = b.angoli.iter().chain(b.lati.iter()).copied().collect();
                let vicino = (0..8).map(|i| (i, (a_schermo(punti[i]) - origine).length())).min_by(|x, y| x.1.total_cmp(&y.1));
                b.preso = vicino.filter(|(_, d)| *d <= PRESA).map(|(i, _)| {
                    let o = in_foto(origine);
                    (i, [punti[i][0] - o[0], punti[i][1] - o[1]])
                });
            }
            let dito = ui.input(|i| i.pointer.interact_pos());
            if area.dragged()
                && let (Some((i, scarto)), Some(d)) = (b.preso, dito)
            {
                let p = in_foto(d);
                let nuovo = [(p[0] + scarto[0]).clamp(0.0, fw), (p[1] + scarto[1]).clamp(0.0, fh)];
                if i < 4 {
                    // I punti in mezzo ai due lati dell'angolo vanno con lui, della metà.
                    let prima = b.angoli[i];
                    b.angoli[i] = nuovo;
                    let mezzo = [(nuovo[0] - prima[0]) / 2.0, (nuovo[1] - prima[1]) / 2.0];
                    for k in [i, (i + 3) % 4] {
                        b.lati[k] = [b.lati[k][0] + mezzo[0], b.lati[k][1] + mezzo[1]];
                    }
                } else {
                    b.lati[i - 4] = nuovo;
                }
            }
            if !area.dragged() {
                b.preso = None;
            }
            let buoni = angoli_buoni(&b.angoli, b.foto.larghezza, b.foto.altezza);
            let colore = if buoni { tema::BLU } else { tema::ROSSO };
            let q = b.angoli.map(a_schermo);
            let m = b.lati.map(a_schermo);
            // Il lato `k`, da un angolo al dopo, passa dal suo punto in mezzo (curva di secondo grado).
            const PASSI: usize = 16;
            let sul_lato = |k: usize, t: f32| {
                let (a, c) = (q[k], q[(k + 1) % 4]);
                let ctrl = pos2(2.0 * m[k].x - (a.x + c.x) / 2.0, 2.0 * m[k].y - (a.y + c.y) / 2.0);
                let u = 1.0 - t;
                pos2(u * u * a.x + 2.0 * u * t * ctrl.x + t * t * c.x, u * u * a.y + 2.0 * u * t * ctrl.y + t * t * c.y)
            };
            // Il velo fuori dal foglio: per ogni lato, strisce fra il lato della foto e quello del foglio.
            let foto = [posto.left_top(), posto.right_top(), posto.right_bottom(), posto.left_bottom()];
            let mut velo = egui::Mesh::default();
            for k in 0..4 {
                for j in 0..PASSI {
                    let (t0, t1) = (j as f32 / PASSI as f32, (j + 1) as f32 / PASSI as f32);
                    let fuori = |t: f32| foto[k] + (foto[(k + 1) % 4] - foto[k]) * t;
                    let n = velo.vertices.len() as u32;
                    for p in [fuori(t0), fuori(t1), sul_lato(k, t1), sul_lato(k, t0)] {
                        velo.colored_vertex(p, VELO_FUORI);
                    }
                    velo.add_triangle(n, n + 1, n + 2);
                    velo.add_triangle(n, n + 2, n + 3);
                }
            }
            ui.painter().add(egui::Shape::mesh(velo));
            let contorno: Vec<Pos2> = (0..4).flat_map(|k| (0..PASSI).map(move |j| (k, j as f32 / PASSI as f32))).map(|(k, t)| sul_lato(k, t)).collect();
            ui.painter().add(egui::Shape::closed_line(contorno, Stroke::new(2.5, colore)));
            for (k, c) in m.iter().enumerate() {
                let preso = b.preso.is_some_and(|(j, _)| j == k + 4);
                let raggio = if preso { 12.0 } else { 8.0 };
                ui.painter().circle(*c, raggio, Color32::from_white_alpha(if preso { 120 } else { 220 }), Stroke::new(2.5, colore));
                let r = ui.allocate_rect(Rect::from_center_size(*c, vec2(2.0 * raggio, 2.0 * raggio)), Sense::hover());
                r.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, LATI[k]));
            }
            for (i, c) in q.iter().enumerate() {
                let preso = b.preso.is_some_and(|(j, _)| j == i);
                let raggio = if preso { 15.0 } else { 11.0 };
                ui.painter().circle(*c, raggio, Color32::from_white_alpha(if preso { 120 } else { 220 }), Stroke::new(3.0, colore));
                // Per l'accessibilità e per le prove: dove sta ogni angolo.
                let r = ui.allocate_rect(Rect::from_center_size(*c, vec2(2.0 * raggio, 2.0 * raggio)), Sense::hover());
                r.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, ANGOLI[i]));
            }
            if let (Some((i, _)), Some(d)) = (b.preso, dito) {
                lente(ui.painter(), b, i, d, schermo, colore);
            }
            // La spiegazione e i pulsanti.
            let testo = if !buoni {
                ANGOLI_INCROCIATI
            } else if pagina_di_prima {
                PAGINA_DI_PRIMA
            } else if b.foto.trovati.is_none() && b.angoli == b.partenza {
                BORDI_NON_TROVATI
            } else {
                SPIEGA_BORDI
            };
            let fatto_si = buoni && !pagina_di_prima;
            let (spiegazione, formati, tutta, rifai, fatto) = if orizzontale {
                let alto = tema::PULSANTE_LARGO;
                let fatto = Rect::from_min_size(pos2(comandi.left(), comandi.bottom() - alto), vec2(comandi.width(), alto));
                let rifai = fatto.translate(vec2(0.0, -(alto + 8.0)));
                let tutta = rifai.translate(vec2(0.0, -(alto + 8.0)));
                let formati = Rect::from_min_size(pos2(comandi.left(), tutta.top() - 8.0 - ALTO_FORMATI), vec2(comandi.width(), ALTO_FORMATI));
                (Rect::from_min_max(comandi.min, pos2(comandi.right(), formati.top())), formati, tutta, rifai, fatto)
            } else {
                let spiegazione = Rect::from_min_size(comandi.min, vec2(comandi.width(), 44.0));
                let formati = Rect::from_min_size(pos2(comandi.left(), spiegazione.bottom()), vec2(comandi.width(), ALTO_FORMATI));
                let mezzo = (comandi.width() - 8.0) / 2.0;
                let tutta = Rect::from_min_size(pos2(comandi.left(), formati.bottom()), vec2(mezzo, tema::PULSANTE_LARGO));
                let rifai = Rect::from_min_size(pos2(tutta.right() + 8.0, tutta.top()), vec2(mezzo, tema::PULSANTE_LARGO));
                let fatto = Rect::from_min_size(pos2(comandi.left(), tutta.bottom() + 8.0), vec2(comandi.width(), tema::PULSANTE_LARGO));
                (spiegazione, formati, tutta, rifai, fatto)
            };
            // Il formato del foglio: la pagina ne prende le proporzioni e il PDF la misura.
            ui.scope_builder(UiBuilder::new().max_rect(formati.shrink2(vec2(0.0, 4.0))), |ui| {
                egui::ScrollArea::horizontal().id_salt("formati_del_foglio").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(ui, |ui| {
                    ui.horizontal_centered(|ui| {
                        for f in Formato::TUTTI {
                            if ui.add(Chip::new(f.nome()).acceso(f == b.formato)).clicked() {
                                tocco = Some(Tocco::Formato(f));
                            }
                        }
                    });
                });
            });
            ui.scope_builder(UiBuilder::new().max_rect(spiegazione), |ui| {
                let allinea = if orizzontale {
                    egui::Layout::top_down(egui::Align::Min)
                } else {
                    egui::Layout::centered_and_justified(egui::Direction::TopDown)
                };
                ui.with_layout(allinea, |ui| {
                    ui.label(egui::RichText::new(testo).font(Carattere::Testo.font(ui.ctx(), 14.0)).color(if buoni {
                        tema::MATITA
                    } else {
                        tema::ROSSO
                    }));
                });
            });
            if ui.put(tutta, Pulsante::new(TUTTA_LA_FOTO, Peso::Tenue).larghezza(tutta.width())).clicked() {
                tocco = Some(Tocco::Tutta);
            }
            let galleria = b.fonte == FonteFoto::Galleria;
            if ui.put(rifai, Pulsante::new(if galleria { SALTA } else { RIFAI }, Peso::Tenue).larghezza(rifai.width())).clicked() {
                tocco = Some(if galleria { Tocco::Salta } else { Tocco::Rifai });
            }
            if ui.put(fatto, Pulsante::new(FATTO, Peso::Principale).icona(icone::CHECK).larghezza(fatto.width()).spento(!fatto_si)).clicked() {
                tocco = Some(Tocco::Fatto);
            }
            if b.preso.is_some() || pagina_di_prima {
                ctx.request_repaint_after(std::time::Duration::from_millis(if b.preso.is_some() { 0 } else { 150 }));
            }
        });
        match tocco {
            Some(Tocco::Lascia) => self.lascia_le_foto(),
            Some(Tocco::Formato(f)) => {
                if let Some(b) = self.scansione.as_mut().and_then(|s| s.bordi.as_mut()) {
                    b.formato = f;
                }
            }
            Some(Tocco::Tutta) => {
                if let Some(b) = self.scansione.as_mut().and_then(|s| s.bordi.as_mut()) {
                    b.angoli = b.foto.tutta();
                    b.lati = lati_dritti(b.angoli);
                    diagnosi("scansione: tutta la foto");
                }
            }
            Some(Tocco::Rifai) => {
                if let Some(s) = self.scansione.as_mut() {
                    s.bordi = None;
                }
                diagnosi("scansione: foto da rifare");
                self.scatta();
            }
            Some(Tocco::Salta) => {
                if let Some(s) = self.scansione.as_mut() {
                    s.bordi = None;
                }
                diagnosi("scansione: foto saltata");
                self.prepara_la_prossima(ctx);
            }
            Some(Tocco::Fatto) => {
                let Some(b) = self.scansione.as_mut().and_then(|s| s.bordi.take()) else { return };
                let spostati = if b.angoli == b.foto.tutta() {
                    "tutta la foto"
                } else if b.angoli == b.partenza {
                    "come trovati"
                } else {
                    "spostati"
                };
                diagnosi(format!("scansione: bordi fatti, {spostati}"));
                // Le passate della gomma sono in coordinate della pagina raddrizzata: con altri
                // angoli, lati o formato cadrebbero su un altro punto, magari su una firma
                // (revisione del 3/11). Si tolgono, e si dice.
                let mut b = b;
                if !b.cancellature.is_empty() && (b.angoli != b.partenza || b.lati != b.lati_di_partenza || b.formato != b.formato_di_partenza) {
                    diagnosi(format!("scansione: bordi cambiati, tolte {} passate di gomma", b.cancellature.len()));
                    b.cancellature.clear();
                    self.avviso_breve.avvisa(PASSATE_TOLTE);
                }
                // La pagina va al posto di quella da rifare (Rifai, Bordi), o in fondo.
                let posto = self.scansione.as_mut().and_then(|s| s.da_rifare.take());
                let lati = lati_curvi(b.angoli, b.lati).then_some(b.lati);
                if lati.is_some() {
                    diagnosi("scansione: lati curvi");
                }
                if let Some(s) = self.scansione.as_mut() {
                    s.formato = b.formato;
                }
                if b.formato != Formato::Automatico {
                    diagnosi(format!("scansione: formato {}", b.formato.chiave()));
                }
                let ritocchi = Ritocchi { lati, cancellature: b.cancellature, formato: b.formato };
                self.fai_la_pagina(ctx, b.foto, b.angoli, ritocchi, b.aspetto, b.giri, posto);
                // La prossima foto della galleria si prepara mentre questa pagina si fa.
                self.prepara_la_prossima(ctx);
            }
            None => {}
        }
    }

    /// Per le prove a schermo: la scansione è aperta e ha quante pagine.
    #[doc(hidden)]
    pub fn pagine_scansionate(&self) -> Option<usize> {
        self.scansione.as_ref().map(|s| s.pagine.len())
    }

    /// Per le prove: larghezza e altezza in pixel delle pagine fotografate.
    #[doc(hidden)]
    pub fn misure_scansionate(&self) -> Vec<(u32, u32)> {
        self.scansione.as_ref().map_or(Vec::new(), |s| s.pagine.iter().map(|(p, _)| (p.larghezza, p.altezza)).collect())
    }

    /// Per le prove: l'aspetto delle pagine fotografate, e quale si vede.
    #[doc(hidden)]
    pub fn aspetti_scansionati(&self) -> (Vec<Aspetto>, usize) {
        self.scansione.as_ref().map_or((Vec::new(), 0), |s| (s.pagine.iter().map(|(p, _)| p.aspetto).collect(), s.corrente))
    }

    /// Per le prove: si riconosce (o si prepara una foto, si fa una pagina, si crea il PDF della
    /// scansione) adesso.
    #[doc(hidden)]
    pub fn lavoro_in_corso(&self) -> bool {
        self.riconosci.is_some()
            || self.word.is_some()
            || self.scansione.as_ref().is_some_and(|s| s.preparazione.is_some() || s.lavorazione.is_some() || s.creazione.is_some())
    }
}

/// «1 pagina», «3 pagine» per la diagnosi.
fn pagine_diagnosi(n: usize) -> String {
    pagine(n)
}
