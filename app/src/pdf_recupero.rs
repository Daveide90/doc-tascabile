//! La copia di recupero di un PDF con modifiche non salvate (piano «PDF completo», P2; dubbio di
//! P1: se Android chiudeva l'app, le modifiche si perdevano).
//!
//! I byte del salvataggio incrementale con le modifiche vanno in una copia privata nella cartella
//! dell'app (`ripristino-pdf`), scritta dal thread del motore (mai su quello dell'interfaccia)
//! poco dopo l'ultima modifica e quando l'app perde il fuoco: su Android dopo onPause non gira più
//! nessun frame, ma il thread del motore sì. Il file dell'utente non si tocca. All'avvio la Home la
//! propone come le copie di Word; riaperta, il PDF ha le modifiche e va salvato (in una copia in
//! Documents/Tascabile, come un documento recuperato di Word). La copia se ne va dopo Salva, dopo
//! «Non salvare», o quando annullando il PDF torna com'era salvato.
//!
//! Una copia che il thread del motore sta scrivendo mentre il PDF si chiude non deve restare: la
//! scrittura e la chiusura passano dallo stesso lucchetto, e dopo la chiusura non si scrive più.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use eframe::egui;
use tascabile_pdf::LavoroCopia;
use tascabile_platform::diagnosi;

use crate::recupero::{CARTELLA_COPIE_PDF, CopiaPdf};
use crate::{Errore, NON_RECUPERATO, TascabileApp, adesso};

/// La copia di recupero di un PDF aperto.
#[derive(Default)]
pub(crate) struct CopiaDelPdf {
    /// L'id dei file della copia, deciso alla prima (o quello della copia da cui si è riaperto).
    id: Option<String>,
    /// La copia sui file, se c'è.
    scritta: Option<CopiaPdf>,
    /// Da quando ci sono modifiche che la copia non ha.
    da_copiare: Option<Instant>,
    /// Copie mandate al motore e non ancora finite.
    in_volo: usize,
    /// Il PDF si è chiuso: le copie in volo non scrivono più. La si chiude togliendo i file,
    /// sotto lo stesso lucchetto della scrittura.
    chiusa: Arc<Mutex<bool>>,
    /// Il PDF viene da una copia di recupero: ha modifiche da salvare anche senza modificarlo.
    pub(crate) recuperato: bool,
}

impl CopiaDelPdf {
    /// Il PDF riaperto dalla copia `c`, che resta e si riscrive con le modifiche dopo.
    pub(crate) fn da_recupero(c: CopiaPdf) -> Self {
        Self { id: Some(c.id()), scritta: Some(c), recuperato: true, ..Self::default() }
    }

    /// C'è una copia sui file (o se ne sta scrivendo una).
    pub(crate) fn c_e(&self) -> bool {
        self.scritta.is_some() || self.in_volo > 0
    }
}

impl TascabileApp {
    fn cartella_copie_pdf(&self) -> PathBuf {
        self.piattaforma.cartella_app().join(CARTELLA_COPIE_PDF)
    }

    /// Una modifica, un annulla o un ripeti arrivati dal motore: la copia non è più al passo.
    pub(crate) fn modifica_da_copiare(&mut self) {
        if let Some(p) = self.pdf.as_mut() {
            p.copia.da_copiare = Some(Instant::now());
        }
    }

    /// A ogni passo: la copia `ritardo_copia` dopo l'ultima modifica (con un frame chiesto per
    /// allora); la copia tolta quando il PDF è di nuovo pulito.
    pub(crate) fn passo_copia_pdf(&mut self, ctx: &egui::Context) {
        let ritardo = self.ritardo_copia;
        let cartella = self.cartella_copie_pdf();
        let Some(p) = self.pdf.as_mut() else { return };
        let da_salvare = p.motore.stato().modificato || p.copia.recuperato;
        match p.copia.da_copiare {
            Some(_) if !da_salvare => p.copia.da_copiare = None,
            // Una modifica ancora in volo: si aspetta lei.
            Some(_) if p.motore.modifica_in_corso() => {}
            Some(dal) => {
                let passato = dal.elapsed();
                if passato >= ritardo {
                    self.manda_copia_pdf();
                } else {
                    ctx.request_repaint_after(ritardo - passato);
                }
                return;
            }
            None => {}
        }
        let Some(p) = self.pdf.as_mut() else { return };
        if !da_salvare && p.copia.in_volo == 0 && !p.motore.modifica_in_corso() && p.copia.c_e() {
            togli_copia(&cartella, &mut p.copia, false);
            diagnosi("recupero: copia del pdf tolta, il pdf è pulito");
        }
    }

    /// L'app perde il fuoco: la copia subito, se ci sono modifiche che non ha (anche una appena
    /// mandata al motore: la copia gli va in coda e la prende).
    pub(crate) fn copia_pdf_subito(&mut self) {
        let Some(p) = self.pdf.as_ref() else { return };
        if p.copia.da_copiare.is_some() || p.motore.modifica_in_corso() {
            self.manda_copia_pdf();
        }
    }

    /// Manda la copia al thread del motore, che la scrive coi byte di adesso.
    fn manda_copia_pdf(&mut self) {
        let cartella = self.cartella_copie_pdf();
        self.copie_scritte += 1;
        let nuovo = format!("pdf-{}-{}-{}", adesso(), std::process::id(), self.copie_scritte);
        let Some(p) = self.pdf.as_mut() else { return };
        let id = p.copia.id.get_or_insert(nuovo).clone();
        let nome = p.nome.clone();
        let chiusa = p.copia.chiusa.clone();
        let lavoro: LavoroCopia = Box::new(move |byte: &Arc<Vec<u8>>| {
            let chiusa = chiusa.lock().unwrap_or_else(|e| e.into_inner());
            if *chiusa {
                return Err("pdf chiuso".to_string());
            }
            CopiaPdf::scrivi(&cartella, &id, &nome, byte).map(|_| byte.len())
        });
        p.copia.da_copiare = None;
        if p.motore.copia(lavoro) {
            p.copia.in_volo += 1;
        }
    }

    /// Una copia finita sul thread del motore.
    pub(crate) fn copia_pdf_finita(&mut self, esito: Result<usize, String>, ms: u32) {
        let cartella = self.cartella_copie_pdf();
        let Some(p) = self.pdf.as_mut() else { return };
        p.copia.in_volo = p.copia.in_volo.saturating_sub(1);
        match esito {
            Ok(byte) => {
                diagnosi(format!("recupero: copia del pdf scritta, {byte} byte in {ms} ms"));
                if let Some(id) = &p.copia.id {
                    p.copia.scritta = Some(CopiaPdf { file: CopiaPdf::file_di(&cartella, id), nome: p.nome.clone(), salvata_alle: adesso() });
                }
            }
            Err(genere) => diagnosi(format!("recupero: copia del pdf non scritta ({genere})")),
        }
    }

    /// Il PDF si chiude (salvato o «Non salvare»): la copia se ne va, e quelle ancora in volo non
    /// scrivono più.
    pub(crate) fn chiudi_copia_pdf(&mut self) {
        let cartella = self.cartella_copie_pdf();
        let Some(p) = self.pdf.as_mut() else { return };
        if p.copia.c_e() || p.copia.id.is_some() {
            togli_copia(&cartella, &mut p.copia, true);
        }
    }

    /// Per le prove: la copia di recupero del PDF aperto è sui file.
    #[doc(hidden)]
    pub fn pdf_ha_la_copia(&self) -> bool {
        self.pdf.as_ref().is_some_and(|p| p.copia.scritta.is_some())
    }

    /// «Riapri» di un PDF proposto nella Home: i byte della copia, nel lettore, con le modifiche
    /// da salvare.
    pub(crate) fn riapri_pdf_recuperato(&mut self, c: CopiaPdf) {
        match std::fs::read(&c.file) {
            Ok(byte) => {
                diagnosi(format!("recupero: pdf da riaprire, {} byte", byte.len()));
                let nome = c.nome.clone();
                self.avvia_pdf_recuperato(byte, nome, c);
            }
            Err(e) => {
                diagnosi(format!("recupero: pdf non riaperto ({:?})", e.kind()));
                self.errore = Some(Errore { titolo: NON_RECUPERATO, testo: "Non riesco a riaprire la copia di recupero.".to_string() });
            }
        }
    }
}

/// Toglie i file della copia (sotto il lucchetto delle scritture); con `per_sempre` le copie in
/// volo non scrivono più.
fn togli_copia(cartella: &std::path::Path, copia: &mut CopiaDelPdf, per_sempre: bool) {
    let mut chiusa = copia.chiusa.lock().unwrap_or_else(|e| e.into_inner());
    if per_sempre {
        *chiusa = true;
    }
    if let Some(id) = &copia.id {
        CopiaPdf::togli_id(cartella, id);
    }
    if let Some(c) = copia.scritta.take() {
        c.togli();
    }
    drop(chiusa);
    copia.recuperato = false;
    copia.da_copiare = None;
}
