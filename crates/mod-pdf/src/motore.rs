//! Il motore su un thread suo, per modificare e salvare. `Session::apply` è sincrono e con file
//! grandi o moduli coi calcoli può durare centinaia di millisecondi, e così `mark_saved`, che
//! rilegge tutto il file (ricerca PDFCraft, sezioni 2, 9 e 13): il `Pdf` va su un thread
//! `pdf-motore` e l'interfaccia gli manda i comandi in una coda (modifica, annulla, ripeti,
//! scrivi). Per ogni comando torna una risposta con l'esito e un'[`Istantanea`] nuova: quello che
//! l'interfaccia sa del documento (i byte da disegnare, le pagine, i link, l'indice, le firme,
//! annulla e ripeti). Il lettore disegna coi byte dell'istantanea su un pool suo, da rifare
//! quando cambiano (`Arc::ptr_eq`, non la generazione: verifica della ricerca).
//!
//! Il salvataggio riscrive tutto il documento (`save_full_bytes`), così nel file non restano le
//! revisioni di prima con quello che si è tolto; un firmato il motore non lo riscrive, e si salva
//! in coda (`save_bytes`): l'originale resta identico, firme comprese, e si aggiunge solo quello
//! che è cambiato. La copia di recupero è sempre in coda, per fare presto. I byte passano da
//! `check_save`, poi a chi li scrive (una funzione data col comando, che gira su questo thread:
//! il file non si scrive sul thread dell'interfaccia) e, se scritti e se lo si chiede, a
//! `mark_saved`, che sposta la base del salvataggio dopo. Tutto nello stesso comando: nessuna
//! modifica può entrare fra i byte scritti e la base nuova.
//!
//! Le firme: prima di ogni modifica si contano quelle integre e con le modifiche dopo permesse, e
//! dopo di nuovo (il motore le ricontrolla a ogni modifica). Se la modifica ne ha rovinata
//! qualcuna e chi l'ha chiesta non l'ha messo in conto, si annulla subito e non si può ripetere.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};

use pdfcraft_engine::EditError;
use pdfcraft_render::{PageInfo, RenderConfig};

use crate::annotazioni::Annotazione;
use crate::apertura::{Misura, Pdf};
use crate::contenuto::{Oggetto, caratteri_del_paragrafo, righe_con_altri_caratteri};
use crate::informazioni::Informazioni;
use crate::modifica::{GenereModifica, Modifica, Protezione, Toccate, firme_intatte};
use crate::moduli::{Campo, Segno, pagine_cambiate};
use crate::navigazione::{Collegamento, Segnalibro};

/// Chi chiede un frame all'interfaccia quando arriva una risposta.
pub type Risveglio = Arc<dyn Fn() + Send + Sync>;

/// Quello che restituisce chi scrive i byte salvati: dice se il file è stato scritto (e allora
/// il motore ne fa la base del salvataggio dopo, se lo si è chiesto).
pub trait Scritto: Send + 'static {
    fn scritto(&self) -> bool;
}

impl<T: Send + 'static, E: Send + 'static> Scritto for Result<T, E> {
    fn scritto(&self) -> bool {
        self.is_ok()
    }
}

/// Chi scrive i byte salvati: gira sul thread del motore.
pub type Scrittura<R> = Box<dyn FnOnce(&Arc<Vec<u8>>) -> R + Send>;

/// Chi scrive la copia di recupero coi byte di adesso: gira sul thread del motore e dice quanti
/// byte ha scritto, o il genere dell'errore (mai il messaggio, che ha il percorso).
pub type LavoroCopia = Box<dyn FnOnce(&Arc<Vec<u8>>) -> Result<usize, String> + Send>;

/// I passi che si possono annullare, come nel motore (`MAX_UNDO`, privato).
const PASSI: usize = 100;

/// Quello che l'interfaccia sa del documento, preso dal motore dopo l'apertura e dopo ogni
/// comando. Costa poco: i byte sono condivisi.
#[derive(Clone)]
pub struct Istantanea {
    /// I byte da disegnare (`Document::display`).
    pub display: Arc<Vec<u8>>,
    pub configurazione: RenderConfig,
    pub misure: Vec<Misura>,
    pub collegamenti: Vec<Collegamento>,
    pub segnalibri: Vec<Segnalibro>,
    /// Il PDF ha altri segnalibri che il motore non ha letto.
    pub altri_segnalibri: bool,
    pub informazioni: Informazioni,
    pub protezione: Protezione,
    /// Il motore lo può modificare (no per un PDF protetto che apre solo in lettura).
    pub modificabile: bool,
    pub annulla: bool,
    pub ripeti: bool,
    /// Ci sono modifiche non salvate.
    pub modificato: bool,
    /// I campi del modulo, come li vede chi compila.
    pub campi: Arc<Vec<Campo>>,
    /// I segni di Fill & Sign sulle pagine.
    pub segni: Arc<Vec<Segno>>,
    /// Le annotazioni che si cambiano col dito (modo Annota e forme del modo Modifica).
    pub annotazioni: Arc<Vec<Annotazione>>,
    /// Le pagine come le vede il motore (rotazione e riquadro), per portare un punto toccato
    /// nello spazio del PDF.
    pub geometrie: Arc<Vec<PageInfo>>,
    /// Il modulo ha calcoli che girano solo con gli script accesi.
    pub calcoli: bool,
    /// Gli script del modulo sono accesi.
    pub javascript: bool,
    /// Ci sono una filigrana, un'intestazione o un piè di pagina messi col modo Modifica.
    pub filigrana: bool,
    pub numeri: bool,
    /// Il documento ha una password (anche messa adesso e non ancora salvata).
    pub cifrato: bool,
}

impl Pdf {
    /// Quello che l'interfaccia sa del documento adesso.
    pub fn istantanea(&self) -> Istantanea {
        let d = self.documento();
        let (segnalibri, altri_segnalibri) = self.segnalibri();
        Istantanea {
            display: d.display.clone(),
            configurazione: self.sessione.render_config(self.id).unwrap_or_default(),
            misure: self.misure(),
            collegamenti: self.collegamenti(),
            segnalibri,
            altri_segnalibri,
            informazioni: self.informazioni(),
            protezione: Protezione::da(&d.signatures),
            modificabile: d.editable(),
            annulla: d.can_undo().is_some(),
            ripeti: d.can_redo().is_some(),
            modificato: d.dirty,
            campi: Arc::new(self.campi()),
            segni: Arc::new(self.segni()),
            annotazioni: Arc::new(self.annotazioni()),
            geometrie: Arc::new(d.info.pages.clone()),
            calcoli: self.calcoli(),
            javascript: self.sessione.javascript(),
            filigrana: d.marks().contains(&pdfcraft_engine::MarkKind::Watermark),
            numeri: d.marks().contains(&pdfcraft_engine::MarkKind::HeaderFooter),
            cifrato: d.security_summary().is_some(),
        }
    }
}

/// Un comando per il thread del motore.
enum Comando<R> {
    Applica {
        modifica: Box<Modifica>,
        rifiuta_se_rovina: bool,
    },
    Annulla,
    Ripeti,
    Scrivi {
        scrittura: Scrittura<R>,
        segna: bool,
        /// Una riscrittura completa (`save_full_bytes`): niente revisioni di prima.
        completo: bool,
    },
    /// La copia appiattita (commenti e campi nelle pagine) a chi la scrive, senza toccare il
    /// documento.
    Appiattita {
        scrittura: Scrittura<R>,
    },
    /// La copia di recupero: i byte del salvataggio incrementale, senza segnarli.
    Copia {
        lavoro: LavoroCopia,
    },
    /// Accende o spegne gli script del modulo.
    Javascript(bool),
    /// Gli oggetti di una pagina, per il modo Modifica.
    Oggetti {
        pagina: usize,
    },
}

/// Com'è andato un comando.
pub enum Evento<R> {
    /// La modifica è fatta; `rovinate` le firme che non sono più valide (messe in conto da chi
    /// l'ha chiesta).
    Applicata {
        genere: GenereModifica,
        toccate: Toccate,
        rovinate: usize,
    },
    /// La modifica avrebbe rovinato `rovinate` firme che chi l'ha chiesta non aveva messo in
    /// conto: annullata subito, e non si può ripetere.
    Rifiutata {
        genere: GenereModifica,
        rovinate: usize,
    },
    /// Il motore non l'ha fatta: il genere dell'errore, mai il suo messaggio.
    NonApplicata {
        genere: GenereModifica,
        errore: &'static str,
    },
    Annullata {
        toccate: Toccate,
    },
    Ripetuta {
        toccate: Toccate,
    },
    /// Niente da annullare o da ripetere.
    Niente,
    /// Annulla o ripeti non sono riusciti: il genere dell'errore.
    NonAnnullata {
        errore: &'static str,
    },
    /// I byte sono stati dati a chi li scrive: il suo esito, se il motore ne ha fatto la base
    /// del salvataggio dopo, e quanti erano.
    Scritta {
        esito: R,
        segnata: bool,
        byte: usize,
    },
    /// Il motore non ha preparato i byte (o non si riaprivano): il genere dell'errore.
    NonScritta {
        errore: &'static str,
    },
    /// La copia di recupero: quanti byte, o il genere dell'errore.
    Copiata {
        esito: Result<usize, String>,
    },
    /// Gli script del modulo adesso sono accesi o spenti.
    Javascript {
        acceso: bool,
    },
    /// Il paragrafo riscritto avrebbe cambiato carattere (il PDF non ha le lettere nuove): la
    /// modifica è stata annullata subito e non si può ripetere; `modifica`, col sì già dato, la
    /// rifà se chi l'ha chiesta vuole continuare.
    CambiaCarattere {
        pagina: usize,
        modifica: Box<Modifica>,
    },
    /// Gli oggetti della pagina `pagina` che si modificano, come sono adesso.
    Oggetti {
        pagina: usize,
        oggetti: Arc<Vec<Oggetto>>,
    },
    /// Il thread del motore si è fermato con un panico: niente più comandi.
    Fermo,
}

/// Un comando finito: com'è andato e quanto ci ha messo il motore.
pub struct Fatto<R> {
    pub evento: Evento<R>,
    pub ms: u32,
}

struct Risposta<R> {
    evento: Evento<R>,
    stato: Option<Istantanea>,
    ms: u32,
}

/// Il genere di un errore del motore, per la diagnosi e per scegliere il messaggio: mai il
/// messaggio del motore, che può riportare pezzi del documento.
pub fn genere_dell_errore(e: &EditError) -> &'static str {
    match e {
        EditError::ReadOnly(_) => "sola lettura",
        EditError::NotPermitted(_) => "permessi del PDF",
        EditError::SignedRewrite(_) | EditError::Signed => "firma digitale",
        EditError::Write(_) => "scrittura del motore",
        EditError::Reopen(_) => "rilettura del motore",
        EditError::NothingToUndo => "niente da annullare",
        EditError::NothingToRedo => "niente da ripetere",
        // Un valore che il campo non prende (una data scritta in un altro modo, un campo che non
        // c'è più o di sola lettura): il motore lo dice prima di scriverlo.
        EditError::Form(_) => "valore del campo",
        _ => "errore del motore",
    }
}

/// Un passo che si può annullare: le pagine che ha toccato e se è una modifica rifiutata per le
/// firme (da non ripetere).
struct Passo {
    toccate: Toccate,
    rifiutato: bool,
}

/// Le pagine toccate dai passi da annullare e da ripetere, in parallelo con le pile del motore.
#[derive(Default)]
struct Pile {
    indietro: Vec<Passo>,
    avanti: Vec<Passo>,
}

impl Pile {
    fn fatto(&mut self, toccate: Toccate) {
        self.indietro.push(Passo { toccate, rifiutato: false });
        if self.indietro.len() > PASSI {
            self.indietro.remove(0);
        }
        self.avanti.clear();
    }

    fn annullato(&mut self, rifiutato: bool) -> Toccate {
        let mut p = self.indietro.pop().unwrap_or(Passo { toccate: Toccate::Tutte, rifiutato: false });
        p.rifiutato |= rifiutato;
        let toccate = p.toccate.clone();
        self.avanti.push(p);
        toccate
    }

    fn ripetuto(&mut self) -> Toccate {
        let p = self.avanti.pop().unwrap_or(Passo { toccate: Toccate::Tutte, rifiutato: false });
        let toccate = p.toccate.clone();
        self.indietro.push(p);
        toccate
    }

    /// Il primo passo da ripetere è una modifica rifiutata per le firme.
    fn ripetere_vietato(&self) -> bool {
        self.avanti.last().is_some_and(|p| p.rifiutato)
    }
}

/// Il motore di un PDF aperto, sul suo thread. Chiuso (`drop`), il thread finisce il comando che
/// sta facendo e si ferma, e il documento si libera lì, non sul thread dell'interfaccia.
pub struct Motore<R: Scritto> {
    comandi: Sender<Comando<R>>,
    risposte: Receiver<Risposta<R>>,
    stato: Istantanea,
    /// I comandi mandati e non ancora finiti, e fra questi le scritture e quelli che possono
    /// cambiare il documento (modifiche, annulla, ripeti).
    in_volo: usize,
    scritture: usize,
    modifiche: usize,
    fermo: bool,
}

impl<R: Scritto> Motore<R> {
    /// Porta `pdf` sul thread `pdf-motore`. L'istantanea della partenza si prende qui.
    pub fn avvia(pdf: Pdf, risveglio: Risveglio) -> std::io::Result<Motore<R>> {
        let stato = pdf.istantanea();
        let (comandi, ricevuti) = mpsc::channel();
        let (manda, risposte) = mpsc::channel();
        std::thread::Builder::new().name("pdf-motore".into()).spawn(move || lavora(pdf, ricevuti, manda, risveglio))?;
        Ok(Motore { comandi, risposte, stato, in_volo: 0, scritture: 0, modifiche: 0, fermo: false })
    }

    /// L'ultima istantanea arrivata.
    pub fn stato(&self) -> &Istantanea {
        &self.stato
    }

    /// Ci sono comandi mandati e non ancora finiti.
    pub fn occupato(&self) -> bool {
        self.in_volo > 0
    }

    /// C'è una scrittura in corso: un'altra aspetta che finisca.
    pub fn scrive(&self) -> bool {
        self.scritture > 0
    }

    /// C'è una modifica (o annulla, o ripeti) mandata e non ancora finita: l'istantanea può non
    /// dire ancora che il documento ha modifiche da salvare.
    pub fn modifica_in_corso(&self) -> bool {
        self.modifiche > 0
    }

    /// Il thread del motore si è fermato.
    pub fn fermo(&self) -> bool {
        self.fermo
    }

    fn manda(&mut self, c: Comando<R>) -> bool {
        if self.fermo {
            return false;
        }
        if self.comandi.send(c).is_err() {
            self.fermo = true;
            return false;
        }
        self.in_volo += 1;
        true
    }

    /// Manda una modifica. Con `rifiuta_se_rovina`, una modifica che rende non valida una firma
    /// si annulla subito (chi la chiede non l'aveva messo in conto).
    pub fn applica(&mut self, modifica: Modifica, rifiuta_se_rovina: bool) -> bool {
        self.manda_modifica(Comando::Applica { modifica: Box::new(modifica), rifiuta_se_rovina })
    }

    pub fn annulla(&mut self) -> bool {
        self.manda_modifica(Comando::Annulla)
    }

    pub fn ripeti(&mut self) -> bool {
        self.manda_modifica(Comando::Ripeti)
    }

    fn manda_modifica(&mut self, c: Comando<R>) -> bool {
        let mandato = self.manda(c);
        if mandato {
            self.modifiche += 1;
        }
        mandato
    }

    /// La copia di recupero: i byte del salvataggio incrementale di adesso (senza `check_save`,
    /// per fare presto, e senza segnarli) a `lavoro`, sul thread del motore, dopo i comandi già
    /// mandati. Non conta come scrittura: un Salva non la aspetta.
    pub fn copia(&mut self, lavoro: LavoroCopia) -> bool {
        self.manda(Comando::Copia { lavoro })
    }

    /// Chiede gli oggetti della pagina `pagina` che si modificano (arrivano con
    /// `Evento::Oggetti`), dopo i comandi già mandati.
    pub fn oggetti(&mut self, pagina: usize) -> bool {
        self.manda(Comando::Oggetti { pagina })
    }

    /// Accende (o spegne) gli script del modulo, per i calcoli che non sono funzioni AF.
    pub fn javascript(&mut self, acceso: bool) -> bool {
        self.manda(Comando::Javascript(acceso))
    }

    /// Prepara i byte del salvataggio (il documento riscritto tutto; in coda se è firmato) e li dà
    /// a `scrittura`, sul thread del motore; con `segna`, se li ha scritti, diventano la base del
    /// salvataggio dopo (Salva, Salva una copia), senza resta tutto com'è (Condividi).
    pub fn scrivi(&mut self, scrittura: Scrittura<R>, segna: bool) -> bool {
        self.manda_scrittura(Comando::Scrivi { scrittura, segna, completo: false })
    }

    /// Come Salva, ma un firmato non ripiega sul salvataggio in coda: il motore lo rifiuta. I byte
    /// diventano la base del salvataggio dopo. (Era il «Salva pulito» del menu, tolto nella 0.9.10
    /// perché Salva riscrive già tutto il file; resta per chi vuole l'errore al posto del ripiego.)
    pub fn scrivi_pulito(&mut self, scrittura: Scrittura<R>) -> bool {
        self.manda_scrittura(Comando::Scrivi { scrittura, segna: true, completo: true })
    }

    fn manda_scrittura(&mut self, c: Comando<R>) -> bool {
        let mandato = self.manda(c);
        if mandato {
            self.scritture += 1;
        }
        mandato
    }

    /// Prepara la copia appiattita del documento com'è adesso (commenti e campi fissati nelle
    /// pagine, [`Pdf::appiattita`]) e la dà a `scrittura`, sul thread del motore. Il documento
    /// resta com'è; l'esito arriva come una scrittura non segnata.
    pub fn appiattita(&mut self, scrittura: Scrittura<R>) -> bool {
        self.manda_scrittura(Comando::Appiattita { scrittura })
    }

    /// I comandi finiti da prendere, in ordine; l'istantanea si aggiorna con loro.
    pub fn ricevi(&mut self) -> Vec<Fatto<R>> {
        let mut fatti = Vec::new();
        loop {
            match self.risposte.try_recv() {
                Ok(r) => fatti.push(self.prendi(r)),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if !self.fermo {
                        self.fermo = true;
                        self.in_volo = 0;
                        self.scritture = 0;
                        self.modifiche = 0;
                        fatti.push(Fatto { evento: Evento::Fermo, ms: 0 });
                    }
                    break;
                }
            }
        }
        fatti
    }

    /// Come [`ricevi`](Self::ricevi), aspettando che tutti i comandi mandati siano finiti, al
    /// massimo `quanto`: per le prove.
    pub fn aspetta(&mut self, quanto: Duration) -> Vec<Fatto<R>> {
        let fine = Instant::now() + quanto;
        let mut fatti = Vec::new();
        while self.in_volo > 0 && !self.fermo {
            let resta = fine.saturating_duration_since(Instant::now());
            match self.risposte.recv_timeout(resta) {
                Ok(r) => fatti.push(self.prendi(r)),
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    fatti.extend(self.ricevi());
                    break;
                }
            }
        }
        fatti
    }

    fn prendi(&mut self, r: Risposta<R>) -> Fatto<R> {
        self.in_volo = self.in_volo.saturating_sub(1);
        match r.evento {
            Evento::Scritta { .. } | Evento::NonScritta { .. } => self.scritture = self.scritture.saturating_sub(1),
            Evento::Fermo | Evento::Copiata { .. } | Evento::Javascript { .. } | Evento::Oggetti { .. } => {}
            Evento::Applicata { .. }
            | Evento::Rifiutata { .. }
            | Evento::NonApplicata { .. }
            | Evento::Annullata { .. }
            | Evento::Ripetuta { .. }
            | Evento::Niente
            | Evento::CambiaCarattere { .. }
            | Evento::NonAnnullata { .. } => self.modifiche = self.modifiche.saturating_sub(1),
        }
        if matches!(r.evento, Evento::Fermo) {
            self.fermo = true;
        }
        if let Some(s) = r.stato {
            self.stato = s;
        }
        Fatto { evento: r.evento, ms: r.ms }
    }
}

/// Il thread del motore: un comando alla volta, finché l'interfaccia tiene il `Motore`.
fn lavora<R: Scritto>(mut pdf: Pdf, comandi: Receiver<Comando<R>>, risposte: Sender<Risposta<R>>, risveglio: Risveglio) {
    let mut pile = Pile::default();
    while let Ok(comando) = comandi.recv() {
        let inizio = Instant::now();
        // Il motore ferma già i panici delle modifiche (`guard`); questo è per quelli fuori.
        let esito = catch_unwind(AssertUnwindSafe(|| esegui(&mut pdf, &mut pile, comando)));
        // L'uscita degli script dei moduli (la console) il motore la tiene finché qualcuno non la
        // prende, e l'app non la legge: si butta dopo ogni comando, se no uno script che scrive a
        // ogni calcolo riempie la memoria (revisione del 31/10, punto 8).
        let _ = pdf.sessione.take_js_output(pdf.id);
        let ms = inizio.elapsed().as_millis().min(u128::from(u32::MAX)) as u32;
        let (risposta, fermo) = match esito {
            Ok((evento, stato)) => (Risposta { evento, stato: Some(stato), ms }, false),
            Err(_) => (Risposta { evento: Evento::Fermo, stato: None, ms }, true),
        };
        let mandata = risposte.send(risposta).is_ok();
        risveglio();
        if fermo || !mandata {
            return;
        }
    }
}

fn esegui<R: Scritto>(pdf: &mut Pdf, pile: &mut Pile, comando: Comando<R>) -> (Evento<R>, Istantanea) {
    let id = pdf.id;
    let evento = match comando {
        Comando::Applica { modifica, rifiuta_se_rovina } => {
            let Modifica { edit, genere, toccate, carattere, pulizia } = *modifica;
            // La correzione di una scansione: la foto pulita e il colore della carta, qui sul thread
            // del motore; se la foto non si sa ripulire, la modifica non si fa.
            let edit = match pulizia {
                None => edit,
                Some(p) => match crate::correzione::risolvi(pdf.documento(), p, edit) {
                    Ok(pronta) => pronta,
                    Err(e) => {
                        let mut stato = pdf.istantanea();
                        stato.ripeti &= !pile.ripetere_vietato();
                        return (Evento::NonApplicata { genere, errore: e.genere() }, stato);
                    }
                },
            };
            let prima = firme_intatte(&pdf.documento().signatures);
            // Un campo compilato può cambiarne altri coi calcoli, anche in altre pagine.
            let modulo = (genere == GenereModifica::Moduli).then(|| pdf.documento().form.clone());
            // Un paragrafo riscritto: i suoi caratteri prima, e la modifica per rimandarla col sì.
            let controllo = carattere.map(|(pagina, blocco)| {
                let (suoi, altre) = caratteri_del_paragrafo(pdf.documento(), pagina, blocco);
                (pagina, suoi, altre, Modifica { edit: edit.clone(), genere, toccate: toccate.clone(), carattere: None, pulizia: None })
            });
            match pdf.sessione.apply(id, edit) {
                Err(e) => Evento::NonApplicata { genere, errore: genere_dell_errore(&e) },
                Ok(()) => {
                    let toccate = match modulo {
                        Some(m) => toccate.unisci(pagine_cambiate(&m, &pdf.documento().form)),
                        None => toccate,
                    };
                    pile.fatto(toccate.clone());
                    let rovinate = prima.saturating_sub(firme_intatte(&pdf.documento().signatures));
                    let cambia =
                        controllo.as_ref().is_some_and(|(pagina, suoi, altre, _)| righe_con_altri_caratteri(pdf.documento(), *pagina, suoi) > *altre);
                    if rovinate > 0 && rifiuta_se_rovina && pdf.sessione.undo(id).is_ok() {
                        pile.annullato(true);
                        Evento::Rifiutata { genere, rovinate }
                    } else if cambia
                        && let Some((pagina, _, _, modifica)) = controllo
                        && pdf.sessione.undo(id).is_ok()
                    {
                        pile.annullato(true);
                        Evento::CambiaCarattere { pagina, modifica: Box::new(modifica) }
                    } else {
                        Evento::Applicata { genere, toccate, rovinate }
                    }
                }
            }
        }
        Comando::Annulla => match pdf.sessione.undo(id) {
            Ok(_) => Evento::Annullata { toccate: pile.annullato(false) },
            Err(EditError::NothingToUndo) => Evento::Niente,
            Err(e) => Evento::NonAnnullata { errore: genere_dell_errore(&e) },
        },
        Comando::Ripeti if pile.ripetere_vietato() => Evento::Niente,
        Comando::Ripeti => match pdf.sessione.redo(id) {
            Ok(_) => Evento::Ripetuta { toccate: pile.ripetuto() },
            Err(EditError::NothingToRedo) => Evento::Niente,
            Err(e) => Evento::NonAnnullata { errore: genere_dell_errore(&e) },
        },
        Comando::Scrivi { scrittura, segna, completo } => {
            // Si riscrive tutto il documento: in coda, le revisioni di prima terrebbero quello che
            // si è tolto (testo riscritto, note, immagini, pagine) per chi taglia il file. Un
            // firmato il motore non lo riscrive: si salva in coda e la firma resta valida.
            let byte = match pdf.sessione.save_full_bytes(id) {
                Err(_) if !completo => pdf.sessione.save_bytes(id),
                byte => byte,
            };
            let pronti = byte.and_then(|b| pdf.sessione.check_save(id, &b).map(|()| b));
            match pronti {
                Err(e) => Evento::NonScritta { errore: genere_dell_errore(&e) },
                Ok(byte) => {
                    let quanti = byte.len();
                    let esito = scrittura(&byte);
                    // Il percorso no: il nome del documento per il motore resta quello fisso.
                    let segnata = segna && esito.scritto() && pdf.sessione.mark_saved(id, byte, None).is_ok();
                    Evento::Scritta { esito, segnata, byte: quanti }
                }
            }
        }
        Comando::Appiattita { scrittura } => match pdf.appiattita() {
            Err(e) => Evento::NonScritta { errore: genere_dell_errore(&e) },
            Ok(byte) => {
                let quanti = byte.len();
                Evento::Scritta { esito: scrittura(&byte), segnata: false, byte: quanti }
            }
        },
        Comando::Copia { lavoro } => match pdf.sessione.save_bytes(id) {
            Err(e) => Evento::Copiata { esito: Err(genere_dell_errore(&e).to_string()) },
            Ok(byte) => Evento::Copiata { esito: lavoro(&byte) },
        },
        Comando::Javascript(acceso) => {
            pdf.sessione.set_javascript(acceso);
            Evento::Javascript { acceso: pdf.sessione.javascript() }
        }
        Comando::Oggetti { pagina } => Evento::Oggetti { pagina, oggetti: Arc::new(pdf.oggetti(pagina)) },
    };
    let mut stato = pdf.istantanea();
    stato.ripeti &= !pile.ripetere_vietato();
    (evento, stato)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::prova::{Pagina, firmato, pdf};
    use pdfcraft_engine::{Edit, NewAnnotation, NoteIcon, Shape, Style};
    use std::sync::Mutex;

    type Esito = Result<usize, String>;

    const ATTESA: Duration = Duration::from_secs(20);

    fn tre_pagine() -> Vec<u8> {
        let contenuto = "BT /F1 24 Tf 60 700 Td (Pagina di prova) Tj ET";
        pdf(&[Pagina { larghezza: 595.0, altezza: 842.0, rotazione: 0, contenuto }; 3])
    }

    fn motore(byte: Vec<u8>) -> Motore<Esito> {
        Motore::avvia(Pdf::apri(byte, None).unwrap(), Arc::new(|| {})).unwrap()
    }

    fn nota(pagina: usize) -> Modifica {
        let shape = Shape::Note { at: [100.0, 700.0], icon: NoteIcon::Comment };
        let style = Style::default_for(&shape);
        Modifica::del_motore(Edit::AddAnnotation(NewAnnotation { page: pagina, shape, style, contents: String::new(), author: String::new() }))
    }

    /// Una scrittura che tiene i byte qui, per guardarli.
    fn in_memoria(dove: &Arc<Mutex<Vec<u8>>>) -> Scrittura<Esito> {
        let dove = dove.clone();
        Box::new(move |byte: &Arc<Vec<u8>>| {
            *dove.lock().unwrap() = byte.to_vec();
            Ok(byte.len())
        })
    }

    #[test]
    fn una_rotazione_si_annulla_si_ripete_e_ridisegna_la_pagina() {
        let mut m = motore(tre_pagine());
        let display = m.stato().display.clone();
        assert!(!m.stato().annulla && !m.stato().ripeti && !m.stato().modificato && m.stato().modificabile);
        assert!(m.applica(Modifica::ruota(vec![1], 90), true));
        assert!(m.occupato() && m.modifica_in_corso() && !m.scrive());
        let fatti = m.aspetta(ATTESA);
        assert!(!m.occupato() && !m.modifica_in_corso());
        assert!(
            matches!(&fatti[..], [Fatto { evento: Evento::Applicata { genere: GenereModifica::Pagine, toccate: Toccate::Alcune(p), rovinate: 0 }, .. }] if p == &[1])
        );
        // La pagina girata si vede larga; i byte da disegnare sono nuovi.
        assert_eq!(m.stato().misure[1], Misura { larghezza: 842.0, altezza: 595.0 });
        assert!(!Arc::ptr_eq(&display, &m.stato().display));
        assert!(m.stato().annulla && !m.stato().ripeti && m.stato().modificato);
        m.annulla();
        let fatti = m.aspetta(ATTESA);
        assert!(matches!(&fatti[..], [Fatto { evento: Evento::Annullata { toccate: Toccate::Alcune(p) }, .. }] if p == &[1]));
        assert_eq!(m.stato().misure[1], Misura { larghezza: 595.0, altezza: 842.0 });
        assert!(!m.stato().annulla && m.stato().ripeti && !m.stato().modificato);
        m.ripeti();
        let fatti = m.aspetta(ATTESA);
        assert!(matches!(&fatti[..], [Fatto { evento: Evento::Ripetuta { toccate: Toccate::Alcune(p) }, .. }] if p == &[1]));
        assert_eq!(m.stato().misure[1].larghezza, 842.0);
        // Niente da ripetere, poi niente da annullare dopo due annulla.
        m.ripeti();
        m.annulla();
        m.annulla();
        let fatti = m.aspetta(ATTESA);
        assert_eq!(fatti.len(), 3);
        assert!(matches!(fatti[0].evento, Evento::Niente) && matches!(fatti[2].evento, Evento::Niente));
    }

    #[test]
    fn il_salvataggio_riscrive_il_pdf_e_diventa_la_base() {
        let originale = tre_pagine();
        let mut m = motore(originale.clone());
        m.applica(Modifica::ruota(vec![0], 90), true);
        let scritti = Arc::new(Mutex::new(Vec::new()));
        // Condividi: i byte con la modifica, ma il documento resta da salvare.
        m.scrivi(in_memoria(&scritti), false);
        assert!(m.scrive());
        let fatti = m.aspetta(ATTESA);
        assert!(!m.scrive());
        assert!(matches!(&fatti[1].evento, Evento::Scritta { esito: Ok(n), segnata: false, byte } if n == byte));
        assert!(m.stato().modificato);
        let condivisi = scritti.lock().unwrap().clone();
        // Salva: il documento riscritto tutto, in una revisione sola.
        m.scrivi(in_memoria(&scritti), true);
        let fatti = m.aspetta(ATTESA);
        assert!(matches!(&fatti[0].evento, Evento::Scritta { esito: Ok(_), segnata: true, .. }));
        assert!(!m.stato().modificato && m.stato().annulla, "dopo il salvataggio si può ancora annullare");
        let salvati = scritti.lock().unwrap().clone();
        assert_eq!(salvati.windows(5).filter(|w| w == b"%%EOF").count(), 1, "l'originale è rimasto in testa al file");
        assert_eq!(salvati.len(), condivisi.len());
        // Riaperto, il PDF ha la pagina girata.
        let riaperto = Pdf::apri(salvati.clone(), None).unwrap();
        assert_eq!(riaperto.misura(0), Some(Misura { larghezza: 842.0, altezza: 595.0 }));
        // Una scrittura che non riesce: il documento resta da salvare com'era.
        m.applica(Modifica::ruota(vec![2], 90), true);
        m.scrivi(Box::new(|_: &Arc<Vec<u8>>| Err("disco pieno".to_string())), true);
        let fatti = m.aspetta(ATTESA);
        assert!(matches!(&fatti[1].evento, Evento::Scritta { esito: Err(_), segnata: false, .. }));
        assert!(m.stato().modificato);
        // Annullando fino al file salvato, il documento è di nuovo pulito.
        m.annulla();
        m.aspetta(ATTESA);
        assert!(!m.stato().modificato);
    }

    /// I byte fino al primo `%%EOF`: la prima revisione del file, quella che rilegge chi lo taglia.
    fn prima_revisione(byte: &[u8]) -> Vec<u8> {
        let fine = byte.windows(5).position(|w| w == b"%%EOF").expect("un %%EOF") + 5;
        byte[..fine].to_vec()
    }

    #[test]
    fn una_pagina_eliminata_non_resta_nei_byte_scritti_di_un_pdf_non_firmato() {
        // Analisi del 27/10, critico 1: Condividi e Salva scrivevano in coda, e chi tagliava il
        // file al primo %%EOF rileggeva la pagina tolta (e così il testo riscritto, le note e le
        // immagini tolte).
        let mut m = motore(tre_pagine());
        m.applica(Modifica::elimina(vec![2]), true);
        let scritti = Arc::new(Mutex::new(Vec::new()));
        for (azione, segna) in [("Condividi", false), ("Salva", true)] {
            m.scrivi(in_memoria(&scritti), segna);
            let fatti = m.aspetta(ATTESA);
            assert!(fatti.iter().any(|f| matches!(&f.evento, Evento::Scritta { esito: Ok(_), .. })), "{azione}: niente scritto");
            let byte = scritti.lock().unwrap().clone();
            let prima = Pdf::apri(prima_revisione(&byte), None).unwrap();
            assert_eq!(prima.pagine(), 2, "{azione}: la prima revisione del file ha ancora la pagina tolta");
        }
    }

    #[test]
    fn le_firme_dicono_che_cosa_si_puo_fare_come_il_motore() {
        // Firma di approvazione: le note sì, le pagine no (il motore la dà per rovinata).
        let mut m = motore(firmato(tre_pagine(), None));
        assert_eq!(m.stato().protezione, Protezione::Firmato { certificazione: None });
        assert_eq!(m.stato().informazioni.firme.firmate, 1);
        m.applica(nota(0), true);
        let fatti = m.aspetta(ATTESA);
        assert!(matches!(fatti[0].evento, Evento::Applicata { rovinate: 0, .. }), "una nota rovina una firma di approvazione");
        assert!(!m.stato().protezione.invalida(GenereModifica::Commenti));
        // La rotazione chiesta senza metterlo in conto: rifiutata, annullata e da non ripetere.
        m.applica(Modifica::ruota(vec![0], 90), true);
        let fatti = m.aspetta(ATTESA);
        assert!(matches!(fatti[0].evento, Evento::Rifiutata { rovinate: 1, .. }), "la rotazione non ha rovinato la firma");
        assert!(m.stato().protezione.invalida(GenereModifica::Pagine));
        assert_eq!(m.stato().misure[0].larghezza, 595.0);
        assert!(m.stato().annulla && !m.stato().ripeti);
        m.ripeti();
        assert!(matches!(m.aspetta(ATTESA)[0].evento, Evento::Niente));
        assert_eq!(m.stato().misure[0].larghezza, 595.0);
        // Messa in conto (chi la chiede ha avvisato): si fa, e la firma risulta rovinata.
        m.applica(Modifica::ruota(vec![0], 90), false);
        let fatti = m.aspetta(ATTESA);
        assert!(matches!(fatti[0].evento, Evento::Applicata { rovinate: 1, .. }));
        assert_eq!(m.stato().protezione, Protezione::Nessuna, "la firma rovinata conta ancora");
        // Certificazione «nessuna modifica»: si può solo leggere, e una nota la rovinerebbe.
        let mut m = motore(firmato(tre_pagine(), Some(1)));
        assert!(m.stato().protezione.sola_lettura());
        m.applica(nota(0), true);
        assert!(matches!(m.aspetta(ATTESA)[0].evento, Evento::Rifiutata { rovinate: 1, .. }));
        // Certificazione coi moduli: le note no.
        let mut m = motore(firmato(tre_pagine(), Some(2)));
        assert!(m.stato().protezione.invalida(GenereModifica::Commenti));
        m.applica(nota(0), true);
        assert!(matches!(m.aspetta(ATTESA)[0].evento, Evento::Rifiutata { .. }));
    }

    #[test]
    fn un_firmato_salvato_tiene_la_firma_dei_byte_di_prima() {
        let originale = firmato(tre_pagine(), None);
        let mut m = motore(originale.clone());
        m.applica(nota(1), true);
        let scritti = Arc::new(Mutex::new(Vec::new()));
        m.scrivi(in_memoria(&scritti), true);
        m.aspetta(ATTESA);
        let salvati = scritti.lock().unwrap().clone();
        assert_eq!(salvati[..originale.len()], originale[..]);
        let riaperto = Pdf::apri(salvati, None).unwrap();
        assert_eq!(riaperto.informazioni().firme.non_valide, 0, "la nota ha rovinato la firma salvata");
    }

    #[test]
    fn un_pdf_con_password_si_modifica() {
        let byte = crate::prova::cifrato("Segreta1");
        let mut m: Motore<Esito> = Motore::avvia(Pdf::apri(byte, Some("Segreta1")).unwrap(), Arc::new(|| {})).unwrap();
        // Aperto con la password d'apertura: il motore lo modifica (il proprietario non l'ha
        // ristretto).
        assert!(m.stato().modificabile);
        m.applica(Modifica::ruota(vec![0], 90), true);
        assert!(matches!(m.aspetta(ATTESA)[0].evento, Evento::Applicata { .. }));
    }

    #[test]
    fn il_risveglio_arriva_e_chiuso_il_motore_il_thread_finisce() {
        let svegli = Arc::new(Mutex::new(0));
        let conta = svegli.clone();
        let mut m: Motore<Esito> = Motore::avvia(Pdf::apri(tre_pagine(), None).unwrap(), Arc::new(move || *conta.lock().unwrap() += 1)).unwrap();
        m.applica(Modifica::ruota(vec![0], 90), true);
        m.annulla();
        assert_eq!(m.aspetta(ATTESA).len(), 2);
        // Il risveglio viene dopo la risposta: un attimo.
        let fine = Instant::now() + ATTESA;
        while *svegli.lock().unwrap() < 2 {
            assert!(Instant::now() < fine, "il risveglio non è arrivato");
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(m.ricevi().is_empty());
        drop(m);
        // Il thread lascia andare il risveglio quando finisce.
        let fine = Instant::now() + ATTESA;
        while Arc::strong_count(&svegli) > 1 {
            assert!(Instant::now() < fine, "il thread del motore non è finito");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn gli_errori_del_motore_senza_il_messaggio() {
        assert_eq!(genere_dell_errore(&EditError::ReadOnly("Verbale Zanzibar".into())), "sola lettura");
        assert_eq!(genere_dell_errore(&EditError::SignedRewrite("Zanzibar".into())), "firma digitale");
        assert_eq!(genere_dell_errore(&EditError::Invalid("Zanzibar".into())), "errore del motore");
        let mut m = motore(tre_pagine());
        // Una pagina che non c'è: il motore dice di no, e il documento resta com'era.
        m.applica(Modifica::ruota(vec![7], 90), true);
        assert!(matches!(m.aspetta(ATTESA)[0].evento, Evento::NonApplicata { errore: "errore del motore", .. }));
        assert!(!m.stato().modificato && !m.stato().annulla);
    }
}
