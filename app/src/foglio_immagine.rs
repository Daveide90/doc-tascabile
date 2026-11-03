//! Le immagini nell'Editor (piano «Word completo», W3): «Immagine» nel foglio «Inserisci», dalla
//! galleria o dalla fotocamera (lo stesso Intent della foto della firma del PDF), la foto preparata
//! sul suo thread (cancellata dalla cache appena letta) e inserita al cursore; con un'immagine
//! scelta, sopra la barra di formato, la barra dell'immagine: allinea (col suo menu: a sinistra,
//! al centro, a destra), testo intorno, descrizione (col suo foglio) ed elimina.

use std::sync::mpsc::{Receiver, TryRecvError};

use eframe::egui::{self, Rect, RichText, pos2};
use tascabile_platform::{EsitoFoto, FonteFoto, diagnosi};
use tascabile_touch::tema::{self, Carattere};
use tascabile_touch::{BarraScorrevole, CampoTesto, Menu, Peso, Pulsante, SceltaMenu, icone};
use tascabile_word::immagine::{ErroreImmagine, ImmaginePronta, InfoImmagine, Intorno, Lato, MASSIMO_FILE, immagine_scelta, prepara};

use crate::{QualeCampo, Schermata, TascabileApp};

/// La voce del foglio «Inserisci» e le due righe della sua pagina.
pub(crate) const IMMAGINE: &str = "Immagine";
pub(crate) const DALLA_GALLERIA: &str = "Dalla galleria";
pub(crate) const DALLA_FOTOCAMERA: &str = "Dalla fotocamera";
/// Le voci della barra dell'immagine; «Allinea» apre il menu dei tre lati.
pub(crate) const ALLINEA: &str = "Allinea";
pub(crate) const A_SINISTRA: &str = "Allinea a sinistra";
pub(crate) const AL_CENTRO: &str = "Allinea al centro";
pub(crate) const A_DESTRA: &str = "Allinea a destra";
pub(crate) const TESTO_INTORNO: &str = "Testo intorno";
pub(crate) const DESCRIZIONE: &str = "Descrizione";
/// Il foglio della descrizione: il testo grigio del campo vuoto e il pulsante.
pub(crate) const VUOTO_DESCRIZIONE: &str = "Che cosa mostra l'immagine";
pub(crate) const FATTO: &str = "Fatto";

/// L'icona dell'allineamento di un lato.
fn icona_del_lato(lato: Lato) -> &'static str {
    match lato {
        Lato::Sinistra => icone::FORMAT_ALIGN_LEFT,
        Lato::Centro => icone::FORMAT_ALIGN_CENTER,
        Lato::Destra => icone::FORMAT_ALIGN_RIGHT,
    }
}

/// La foto per il documento: chiesta ad Android, o che si prepara sul suo thread.
pub(crate) enum AttesaImmagine {
    /// La foto chiesta, col numero della richiesta.
    Foto(i32),
    Lavoro(Receiver<Result<ImmaginePronta, ErroreImmagine>>),
}

/// La pagina delle fonti nel foglio «Inserisci»: la galleria e la fotocamera.
pub(crate) fn pagina_delle_fonti(ui: &mut egui::Ui) -> Option<FonteFoto> {
    ui.spacing_mut().item_spacing.y = 0.0;
    let mut fonte = None;
    if crate::pdf_firma::riga(ui, icone::IMAGE, DALLA_GALLERIA, None, false).clicked() {
        fonte = Some(FonteFoto::Galleria);
    }
    if crate::pdf_firma::riga(ui, icone::PHOTO_CAMERA, DALLA_FOTOCAMERA, None, true).clicked() {
        fonte = Some(FonteFoto::Fotocamera);
    }
    fonte
}

impl TascabileApp {
    /// «Immagine» nel foglio «Inserisci»: il foglio mostra da dove prenderla.
    pub(crate) fn apri_fonti_immagine(&mut self) {
        diagnosi("inserisci: da dove prendere l'immagine");
        self.fonti_immagine = true;
    }

    /// La foto chiesta ad Android: il foglio si chiude e la tastiera pure (l'app della foto copre
    /// tutto).
    pub(crate) fn chiedi_immagine(&mut self, fonte: FonteFoto) {
        self.foglio_inserisci.chiudi();
        self.fonti_immagine = false;
        diagnosi(format!("word: immagine dalla {}", fonte.nome()));
        if self.piattaforma.keyboard_visible() {
            self.piattaforma.hide_keyboard();
        }
        self.immagine_word = Some(AttesaImmagine::Foto(self.piattaforma.chiedi_foto_firma(fonte)));
    }

    /// A ogni frame: la foto chiesta per il documento, se è arrivata, si legge (e si cancella dalla
    /// cache) e si prepara sul thread `word-immagine`; pronta, va al cursore e resta scelta.
    pub(crate) fn passo_immagine_word(&mut self, ctx: &egui::Context) {
        if let Some(AttesaImmagine::Foto(richiesta)) = self.immagine_word
            && let Some(esito) = self.piattaforma.prendi_foto_firma(richiesta)
        {
            self.immagine_word = None;
            match esito {
                EsitoFoto::Pronta(file) => {
                    diagnosi("word: immagine arrivata");
                    let (tx, rx) = std::sync::mpsc::channel();
                    let ctx2 = ctx.clone();
                    let avviato = std::thread::Builder::new().name("word-immagine".into()).spawn(move || {
                        let letta = std::fs::File::open(&file).and_then(|f| {
                            let mut v = Vec::new();
                            std::io::Read::read_to_end(&mut std::io::Read::take(f, MASSIMO_FILE + 1), &mut v).map(|_| v)
                        });
                        crate::butta_la_foto(&file);
                        let esito = match letta {
                            Ok(b) if b.len() as u64 <= MASSIMO_FILE => prepara(&b),
                            Ok(_) => Err(ErroreImmagine::Grande),
                            Err(_) => Err(ErroreImmagine::Lettura),
                        };
                        match &esito {
                            Ok(p) => diagnosi(format!("word: immagine pronta, {}×{} pixel, {} byte", p.pixel.0, p.pixel.1, p.byte.len())),
                            Err(e) => diagnosi(format!("word: immagine non pronta ({})", e.genere())),
                        }
                        let _ = tx.send(esito);
                        ctx2.request_repaint();
                    });
                    match avviato {
                        Ok(_) => self.immagine_word = Some(AttesaImmagine::Lavoro(rx)),
                        Err(_) => diagnosi("word: thread dell'immagine non avviato"),
                    }
                }
                EsitoFoto::Pronte { file, .. } => crate::butta_le_foto(&file, "word: immagine"),
                EsitoFoto::Annullata => diagnosi("word: immagine annullata"),
                EsitoFoto::Superata => diagnosi("word: immagine, richiesta superata da un'altra"),
                EsitoFoto::Errore => {
                    diagnosi("word: immagine non arrivata");
                    self.avviso_breve.avvisa("La foto non è arrivata.");
                }
            }
        }
        let Some(AttesaImmagine::Lavoro(rx)) = &self.immagine_word else { return };
        match rx.try_recv() {
            Ok(Ok(pronta)) => {
                self.immagine_word = None;
                self.fuoco_al_documento();
                if let Some(e) = self.editor_mut() {
                    let larga = e.vista.larghezza_del_testo();
                    e.vista.cambia_modo(tascabile_word::vista::Modo::Scrittura);
                    e.inserisci_immagine(&pronta, larga);
                }
                self.dopo_ponte(None);
            }
            Ok(Err(e)) => {
                self.immagine_word = None;
                self.avviso_breve.avvisa(e.messaggio());
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => self.immagine_word = None,
        }
    }

    /// La barra dell'immagine scelta in `posto`: Allinea (con l'icona del lato dove sta, apre il
    /// suo menu), il testo intorno (acceso col quadrato), la descrizione (col suo foglio),
    /// elimina. Coi tre lati sulla barra, a 372 dp Elimina restava fuori, da scorrere (0.8.3).
    pub(crate) fn barra_immagine(&mut self, ui: &mut egui::Ui, posto: Rect, info: &InfoImmagine) {
        let scelta = BarraScorrevole::new()
            .voce(icona_del_lato(info.lato), ALLINEA, false)
            .interruttore(icone::WRAP_TEXT, TESTO_INTORNO, TESTO_INTORNO, info.intorno == Intorno::Quadrato)
            .voce(icone::SHORT_TEXT, DESCRIZIONE, false)
            .voce(icone::DELETE, crate::foglio_tabella::ELIMINA, false)
            .mostra(ui, posto);
        let Some((i, dove)) = scelta else { return };
        if i == 0 {
            diagnosi("immagine: menu dell'allineamento");
            self.menu_allinea = Some((pos2(dove.center().x, dove.top() - 4.0), info.lato));
            return;
        }
        let Some(e) = self.editor_mut() else { return };
        let st = match i {
            1 => e.intorno_immagine(if info.intorno == Intorno::Quadrato { Intorno::InLinea } else { Intorno::Quadrato }),
            2 => {
                self.apri_descrizione(&info.descrizione);
                return;
            }
            _ => e.elimina_immagine(),
        };
        self.dopo_ponte(st);
    }

    /// Il menu di Allinea sopra la sua voce, se è aperto: i tre lati, in grassetto quello dove
    /// sta l'immagine.
    pub(crate) fn mostra_menu_allinea(&mut self, ctx: &egui::Context) {
        let Some((punto, adesso)) = self.menu_allinea else { return };
        const LATI: [(Lato, &str); 3] = [(Lato::Sinistra, A_SINISTRA), (Lato::Centro, AL_CENTRO), (Lato::Destra, A_DESTRA)];
        let mut menu = Menu::new(egui::Id::new("menu_allinea")).sopra();
        for (lato, nome) in LATI {
            menu = if lato == adesso { menu.primo(icona_del_lato(lato), nome) } else { menu.voce(icona_del_lato(lato), nome) };
        }
        let Some(scelta) = menu.mostra(ctx, punto) else { return };
        self.menu_allinea = None;
        let SceltaMenu::Voce(i) = scelta else { return };
        let Some(e) = self.editor_mut() else { return };
        let st = e.allinea_immagine(LATI[i.min(2)].0);
        self.dopo_ponte(st);
    }

    /// Il foglio della descrizione, col campo che prende la tastiera.
    fn apri_descrizione(&mut self, adesso: &str) {
        diagnosi("immagine: foglio della descrizione");
        self.descrizione = Some(CampoTesto::new().con_testo(adesso));
        self.foglio_descrizione.apri();
        self.fuoco_al_campo_word(QualeCampo::Descrizione);
    }

    /// «Fatto» (o Invio nel campo): la descrizione va all'immagine scelta e il foglio si chiude.
    pub(crate) fn conferma_descrizione(&mut self) {
        let Some(testo) = self.descrizione.as_ref().map(|c| c.testo().to_string()) else { return };
        self.chiudi_campi_word();
        self.foglio_descrizione.chiudi();
        if let Some(e) = self.editor_mut() {
            let st = e.descrivi_immagine(&testo);
            self.dopo_ponte(st);
        }
    }

    /// Il foglio «Descrizione»: due righe che dicono a che cosa serve, il campo e «Fatto». Si
    /// chiude se l'immagine non è più scelta.
    pub(crate) fn mostra_foglio_descrizione(&mut self, ctx: &egui::Context) {
        let scelta = self.editor().is_some_and(|e| immagine_scelta(&e.s).is_some());
        if self.schermata != Schermata::Editor || !scelta {
            self.foglio_descrizione.chiudi();
        }
        if !self.foglio_descrizione.aperto() {
            if self.descrizione.is_some() {
                self.chiudi_campi_word();
            }
            return;
        }
        let Some(campo) = self.descrizione.as_ref() else {
            self.foglio_descrizione.chiudi();
            return;
        };
        let (mut tocco, mut fatto) = (false, false);
        self.foglio_descrizione.mostra(ctx, DESCRIZIONE, |ui| {
            ui.spacing_mut().item_spacing.y = 12.0;
            ui.label(
                RichText::new("Che cosa mostra l'immagine, in una frase: la leggono i lettori dello schermo e resta nel file.")
                    .font(Carattere::Testo.font(ui.ctx(), 15.0))
                    .color(tema::MATITA),
            );
            tocco = campo.mostra(ui, VUOTO_DESCRIZIONE).clicked();
            let largo = ui.available_width();
            fatto = ui.add(Pulsante::new(FATTO, Peso::Principale).icona(icone::CHECK).larghezza(largo)).clicked();
        });
        if tocco {
            self.fuoco_al_campo_word(QualeCampo::Descrizione);
        }
        if fatto {
            self.conferma_descrizione();
        }
        if !self.foglio_descrizione.aperto() && self.descrizione.is_some() {
            self.chiudi_campi_word();
        }
    }

    /// Per le prove: il rettangolo sullo schermo dell'immagine scelta, nella vista Scrittura com'era
    /// all'ultimo frame.
    #[doc(hidden)]
    pub fn immagine_sullo_schermo_di_prova(&mut self) -> Option<Rect> {
        let e = self.editor_mut()?;
        let (pos, _) = immagine_scelta(&e.s)?;
        let l = e.s.layout();
        let r = l.object(&pos, e.s.page_hint)?.rect;
        Some(Rect::from_min_max(e.vista.sullo_schermo(r.x, r.y), e.vista.sullo_schermo(r.x + r.w, r.y + r.h)))
    }

    /// Per le prove: la prima immagine del documento sullo schermo, scelta o no.
    #[doc(hidden)]
    pub fn prima_immagine_di_prova(&mut self) -> Option<Rect> {
        let e = self.editor_mut()?;
        let l = e.s.layout();
        let r = l.find_object(0, |o| o.text_box.is_none())?.rect;
        Some(Rect::from_min_max(e.vista.sullo_schermo(r.x, r.y), e.vista.sullo_schermo(r.x + r.w, r.y + r.h)))
    }

    /// Per le prove a schermo e sul PC: arriva la foto `byte` chiesta ad Android.
    #[doc(hidden)]
    #[cfg(not(target_os = "android"))]
    pub fn simula_foto_di_prova(&self, byte: &[u8]) {
        self.piattaforma.simula_foto_firma(Some(byte));
    }

    /// Per le prove: la foto per il documento si prepara ancora.
    #[doc(hidden)]
    pub fn immagine_in_arrivo(&self) -> bool {
        self.immagine_word.is_some()
    }
}
