//! L'impronta e il blocco dell'app (roadmap della sicurezza, Fase 1): una strada sola per tutto
//! quello che chiede l'impronta o il PIN del telefono, con la finestra di sistema di Android
//! (BiometricPrompt): posare la firma salvata, sbloccare l'app, accendere il blocco e spegnere le
//! protezioni. Spegnerle chiede l'impronta, altrimenti chi ha in mano il telefono sbloccato le
//! spegnerebbe e poi userebbe la firma.

use std::time::{Duration, SystemTime};

use eframe::egui::{self, RichText};
use tascabile_platform::{EsitoImpronta, diagnosi};
use tascabile_touch::tema::{self, Carattere};
use tascabile_touch::{Peso, Pulsante, icone};

use crate::{Errore, TascabileApp, impostazioni};

pub(crate) const BLOCCATA: &str = "Tascabile è bloccata";
const SPIEGA_BLOCCATA: &str = "Si apre con l'impronta o il PIN del telefono.";
pub(crate) const SBLOCCA: &str = "Sblocca";
const NON_RIUSCITA: &str = "L'impronta non è riuscita: riprova.";
pub(crate) const SENZA_BLOCCO_FIRMA: &str = "Per usare la firma salvata serve un blocco dello schermo del telefono (PIN, sequenza o impronta), \
     oppure spegni «Impronta per la firma» nelle Impostazioni.";
const FIRMA_NON_RIUSCITA: &str = "L'impronta non è riuscita: la firma salvata non si è messa.";
pub(crate) const BLOCCO_SENZA_BLOCCO: &str = "Per il blocco dell'app serve un blocco dello schermo del telefono: PIN, sequenza o impronta.";
pub(crate) const BLOCCO_SPENTO: &str = "Il telefono non ha più un blocco dello schermo: il blocco dell'app si è spento.";
const NON_CAMBIATA: &str = "L'impronta non è riuscita: la scelta non è cambiata.";

/// Un'impronta riuscita vale per qualche minuto, per la firma e per le Impostazioni.
pub(crate) const VALE_UN_IMPRONTA: Duration = Duration::from_secs(5 * 60);
/// Fuori dall'app almeno tanto, al ritorno l'app è bloccata (se il blocco è acceso).
pub(crate) const BLOCCO_DOPO: Duration = Duration::from_secs(2 * 60);

/// Per che cosa si chiede l'impronta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PerCosa {
    /// Posare la firma salvata (le iniziali, se vero).
    Firma(bool),
    Sblocca,
    AccendiBlocco,
    SpegniBlocco,
    SpegniImprontaFirma,
}

impl PerCosa {
    /// Per la diagnosi.
    fn nome(self) -> &'static str {
        match self {
            PerCosa::Firma(false) => "la firma salvata",
            PerCosa::Firma(true) => "le iniziali salvate",
            PerCosa::Sblocca => "sbloccare l'app",
            PerCosa::AccendiBlocco => "accendere il blocco dell'app",
            PerCosa::SpegniBlocco => "spegnere il blocco dell'app",
            PerCosa::SpegniImprontaFirma => "spegnere l'impronta per la firma",
        }
    }

    /// Il titolo e la spiegazione della finestra di Android.
    fn finestra(self) -> (&'static str, &'static str) {
        match self {
            PerCosa::Firma(_) => ("Metti la firma salvata", "La tua impronta o il blocco del telefono, per usare la firma salvata in Tascabile."),
            PerCosa::Sblocca => ("Sblocca Tascabile", "La tua impronta o il blocco del telefono, per aprire Tascabile."),
            PerCosa::AccendiBlocco | PerCosa::SpegniBlocco | PerCosa::SpegniImprontaFirma => {
                ("Sicurezza di Tascabile", "La tua impronta o il blocco del telefono, per cambiare questa protezione.")
            }
        }
    }
}

/// Lo stato dell'impronta e del blocco.
#[derive(Default)]
pub(crate) struct Blocco {
    /// L'app è bloccata: si vede solo la schermata del blocco.
    pub(crate) bloccata: bool,
    /// L'impronta per sbloccare va chiesta appena l'app ha il fuoco (all'avvio e al ritorno).
    da_chiedere: bool,
    /// La richiesta in corso: il suo numero, e per che cosa.
    pub(crate) in_corso: Option<(i32, PerCosa)>,
    /// L'ultima impronta riuscita, all'ora del sistema: per il blocco al ritorno (un'impronta
    /// fatta mentre si era fuori sblocca).
    pub(crate) riuscita_alle: Option<SystemTime>,
    /// La stessa, sull'orologio dall'accensione (sonno compreso, e non si sposta): per la grazia
    /// di 5 minuti della firma.
    pub(crate) riuscita_dall_accensione: Option<Duration>,
    /// Da quando l'app è senza fuoco.
    pub(crate) fuori_dal: Option<SystemTime>,
    /// Perché l'app non si è sbloccata, sulla schermata del blocco.
    messaggio: Option<&'static str>,
}

impl Blocco {
    /// All'avvio: col blocco acceso l'app parte bloccata.
    pub(crate) fn all_avvio(acceso: bool) -> Self {
        Self { bloccata: acceso, da_chiedere: acceso, ..Self::default() }
    }

    /// Un'impronta è riuscita da meno di `VALE_UN_IMPRONTA`, misurati sull'orologio
    /// dall'accensione (`adesso`), che spostare l'ora del telefono non cambia (revisione del 3/11,
    /// M4: prima l'ora di sistema, e portandola indietro la grazia si allungava).
    fn recente(&self, adesso: Duration) -> bool {
        self.riuscita_dall_accensione.and_then(|t| adesso.checked_sub(t)).is_some_and(|d| d < VALE_UN_IMPRONTA)
    }

    /// Al ritorno nell'app: si è stati fuori almeno `BLOCCO_DOPO` (o l'orologio è andato indietro),
    /// e non per la finestra dell'impronta riuscita nel frattempo.
    fn da_bloccare(&self, adesso: SystemTime) -> bool {
        let Some(uscita) = self.fuori_dal else { return false };
        let lunga = adesso.duration_since(uscita).map_or(true, |d| d >= BLOCCO_DOPO);
        let sbloccata_dopo = self.riuscita_alle.is_some_and(|r| r >= uscita);
        lunga && !sbloccata_dopo
    }
}

impl TascabileApp {
    /// Fa `per` dopo l'impronta: subito se ne è riuscita una da poco (non per sbloccare l'app).
    pub(crate) fn con_l_impronta(&mut self, per: PerCosa) {
        // La grazia vale solo per la firma: per spegnere una protezione l'impronta si chiede sempre.
        if matches!(per, PerCosa::Firma(_)) && self.blocco.recente(tascabile_platform::dall_accensione()) {
            self.dopo_l_impronta(per);
            return;
        }
        let (titolo, spiegazione) = per.finestra();
        let richiesta = self.piattaforma.chiedi_impronta(titolo, spiegazione);
        diagnosi(format!("impronta: chiesta per {}", per.nome()));
        self.blocco.in_corso = Some((richiesta, per));
    }

    /// A ogni frame: l'esito dell'impronta, se è arrivato; la richiesta per sbloccare, appena
    /// l'app ha il fuoco.
    pub(crate) fn passo_blocco(&mut self, ctx: &egui::Context) {
        if let Some((richiesta, per)) = self.blocco.in_corso
            && let Some(esito) = self.piattaforma.esito_impronta(richiesta)
        {
            self.blocco.in_corso = None;
            diagnosi(format!("impronta: {} per {}", esito.genere(), per.nome()));
            if esito == EsitoImpronta::Riuscita {
                self.blocco.riuscita_alle = Some(SystemTime::now());
                self.blocco.riuscita_dall_accensione = Some(tascabile_platform::dall_accensione());
                self.dopo_l_impronta(per);
            } else {
                self.senza_l_impronta(per, esito);
            }
        }
        if self.blocco.bloccata && self.blocco.da_chiedere && ctx.input(|i| i.focused) {
            self.blocco.da_chiedere = false;
            self.con_l_impronta(PerCosa::Sblocca);
        }
    }

    /// Il fuoco perso (`perso`) o ritrovato: l'ora dell'uscita, e al ritorno dopo qualche minuto
    /// l'app si blocca.
    pub(crate) fn fuoco_e_blocco(&mut self, perso: bool, ritrovato: bool) {
        let adesso = SystemTime::now();
        if perso {
            self.blocco.fuori_dal = Some(adesso);
        }
        if ritrovato {
            if self.impostazioni.blocco && !self.blocco.bloccata && self.blocco.da_bloccare(adesso) {
                self.blocca();
            }
            self.blocco.fuori_dal = None;
        }
    }

    /// Blocca l'app: si vede solo la schermata del blocco, la tastiera si chiude e i suoi tasti non
    /// arrivano a nessuno; per la firma e le Impostazioni l'impronta si richiede.
    pub(crate) fn blocca(&mut self) {
        self.blocco.bloccata = true;
        self.blocco.da_chiedere = true;
        self.blocco.riuscita_alle = None;
        self.blocco.riuscita_dall_accensione = None;
        self.blocco.in_corso = None;
        self.blocco.messaggio = None;
        self.conferma_scrittura_pdf(false);
        self.piattaforma.hide_keyboard();
        self.fuoco_al_documento();
        if let Some(e) = self.editor_mut() {
            e.ponte.chiudi();
        }
        diagnosi("blocco: app bloccata");
    }

    /// L'impronta è riuscita (o ne era riuscita una da poco): si fa quello che aspettava.
    fn dopo_l_impronta(&mut self, per: PerCosa) {
        match per {
            PerCosa::Firma(iniziali) => self.posa_la_firma_salvata(iniziali),
            PerCosa::Sblocca => {
                self.blocco.bloccata = false;
                self.blocco.messaggio = None;
                diagnosi("blocco: app sbloccata");
            }
            PerCosa::AccendiBlocco => self.scegli_blocco(true),
            PerCosa::SpegniBlocco => self.scegli_blocco(false),
            PerCosa::SpegniImprontaFirma => self.scegli_impronta_firma(false),
        }
    }

    /// L'impronta annullata, non riuscita o impossibile (telefono senza blocco dello schermo).
    fn senza_l_impronta(&mut self, per: PerCosa, esito: EsitoImpronta) {
        let senza_blocco = esito == EsitoImpronta::SenzaBlocco;
        let errore = esito == EsitoImpronta::Errore;
        match per {
            PerCosa::Firma(_) => {
                if let Some(p) = self.pdf.as_mut() {
                    p.compila.firma.dove = None;
                }
                if senza_blocco {
                    self.avviso_breve.avvisa(SENZA_BLOCCO_FIRMA);
                } else if errore {
                    self.avviso_breve.avvisa(FIRMA_NON_RIUSCITA);
                }
            }
            // Un telefono senza blocco dello schermo l'app non la può bloccare: chi ha il telefono
            // ha già tutto. Per togliere il blocco dello schermo Android chiede il PIN.
            PerCosa::Sblocca if senza_blocco => {
                self.blocco.bloccata = false;
                self.scegli_blocco(false);
                self.avviso_breve.avvisa(BLOCCO_SPENTO);
            }
            PerCosa::Sblocca => self.blocco.messaggio = errore.then_some(NON_RIUSCITA),
            PerCosa::AccendiBlocco if senza_blocco => self.avviso_breve.avvisa(BLOCCO_SENZA_BLOCCO),
            PerCosa::SpegniBlocco if senza_blocco => self.scegli_blocco(false),
            PerCosa::SpegniImprontaFirma if senza_blocco => self.scegli_impronta_firma(false),
            PerCosa::AccendiBlocco | PerCosa::SpegniBlocco | PerCosa::SpegniImprontaFirma => {
                if errore {
                    self.avviso_breve.avvisa(NON_CAMBIATA);
                }
            }
        }
    }

    /// L'interruttore del blocco toccato nelle Impostazioni: acceso o spento dopo l'impronta.
    pub(crate) fn tocca_blocco(&mut self) {
        self.con_l_impronta(if self.impostazioni.blocco { PerCosa::SpegniBlocco } else { PerCosa::AccendiBlocco });
    }

    /// L'interruttore dell'impronta per la firma toccato: si accende subito, si spegne dopo
    /// l'impronta.
    pub(crate) fn tocca_impronta_firma(&mut self) {
        if self.impostazioni.impronta_firma {
            self.con_l_impronta(PerCosa::SpegniImprontaFirma);
        } else {
            self.scegli_impronta_firma(true);
        }
    }

    fn scegli_blocco(&mut self, acceso: bool) {
        match impostazioni::scrivi_blocco(&self.piattaforma.cartella_app(), acceso) {
            Ok(()) => {
                diagnosi(if acceso { "impostazioni: blocco dell'app acceso" } else { "impostazioni: blocco dell'app spento" });
                self.impostazioni.blocco = acceso;
            }
            Err(e) => {
                diagnosi(format!("impostazioni: blocco dell'app non scritto ({:?})", e.kind()));
                self.errore = Some(Errore { titolo: crate::NON_SALVATO, testo: "Non riesco a salvare la scelta nelle Impostazioni.".to_string() });
            }
        }
    }

    fn scegli_impronta_firma(&mut self, accesa: bool) {
        match impostazioni::scrivi_impronta_firma(&self.piattaforma.cartella_app(), accesa) {
            Ok(()) => {
                diagnosi(if accesa { "impostazioni: impronta per la firma accesa" } else { "impostazioni: impronta per la firma spenta" });
                self.impostazioni.impronta_firma = accesa;
            }
            Err(e) => {
                diagnosi(format!("impostazioni: impronta per la firma non scritta ({:?})", e.kind()));
                self.errore = Some(Errore { titolo: crate::NON_SALVATO, testo: "Non riesco a salvare la scelta nelle Impostazioni.".to_string() });
            }
        }
    }

    /// Per le schermate: l'app bloccata, con l'impronta finta che risponde «annullata» (sul PC).
    #[doc(hidden)]
    #[cfg(not(target_os = "android"))]
    pub fn blocca_di_prova(&mut self) {
        self.piattaforma.simula_impronta(EsitoImpronta::Annullata);
        self.blocca();
    }

    /// Per le schermate: di nuovo sbloccata.
    #[doc(hidden)]
    pub fn sblocca_di_prova(&mut self) {
        self.blocco.bloccata = false;
    }

    /// La schermata del blocco, al posto di tutto il resto.
    pub(crate) fn mostra_bloccata(&mut self, ui: &mut egui::Ui) {
        let zona = ui.ctx().content_rect();
        let ctx = ui.ctx().clone();
        let testo = self.blocco.messaggio.unwrap_or(SPIEGA_BLOCCATA);
        let mut sblocca = false;
        ui.scope_builder(egui::UiBuilder::new().max_rect(zona.shrink2(egui::vec2(24.0, 0.0))).layout(egui::Layout::top_down(egui::Align::Center)), |ui| {
            ui.add_space((zona.height() * 0.28).max(24.0));
            ui.label(RichText::new(icone::LOCK).font(Carattere::Icona.font(&ctx, 48.0)).color(tema::INCHIOSTRO));
            ui.add_space(16.0);
            ui.label(RichText::new(BLOCCATA).font(Carattere::Titolo.font(&ctx, 24.0)).color(tema::INCHIOSTRO));
            ui.add_space(8.0);
            ui.add(egui::Label::new(RichText::new(testo).font(Carattere::Testo.font(&ctx, 16.0)).color(tema::MATITA)).wrap());
            ui.add_space(24.0);
            sblocca = ui.add(Pulsante::new(SBLOCCA, Peso::Principale).icona(icone::LOCK_OPEN)).clicked();
        });
        if sblocca {
            self.con_l_impronta(PerCosa::Sblocca);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn si_blocca_solo_dopo_qualche_minuto_fuori() {
        let adesso = SystemTime::now();
        let mut b = Blocco::default();
        assert!(!b.da_bloccare(adesso), "mai uscita");
        b.fuori_dal = Some(adesso - Duration::from_secs(30));
        assert!(!b.da_bloccare(adesso));
        b.fuori_dal = Some(adesso - BLOCCO_DOPO);
        assert!(b.da_bloccare(adesso));
        // L'orologio spostato indietro mentre l'app era fuori: si blocca.
        b.fuori_dal = Some(adesso + Duration::from_secs(3600));
        assert!(b.da_bloccare(adesso));
        // Fuori per la finestra dell'impronta, riuscita: non si blocca.
        b.fuori_dal = Some(adesso - BLOCCO_DOPO);
        b.riuscita_alle = Some(adesso - Duration::from_secs(1));
        assert!(!b.da_bloccare(adesso));
        // Un'impronta di prima dell'uscita non conta.
        b.riuscita_alle = Some(adesso - BLOCCO_DOPO - Duration::from_secs(1));
        assert!(b.da_bloccare(adesso));
    }

    #[test]
    fn un_impronta_vale_cinque_minuti_e_non_con_l_orologio_indietro() {
        // Sull'orologio dall'accensione, che l'ora del telefono non sposta (revisione del 3/11, M4).
        let adesso = Duration::from_secs(10_000);
        let mut b = Blocco::default();
        assert!(!b.recente(adesso));
        b.riuscita_dall_accensione = Some(adesso - Duration::from_secs(60));
        assert!(b.recente(adesso));
        b.riuscita_dall_accensione = Some(adesso - VALE_UN_IMPRONTA);
        assert!(!b.recente(adesso));
        b.riuscita_dall_accensione = Some(adesso + Duration::from_secs(60));
        assert!(!b.recente(adesso));
        // L'ora di sistema spostata non conta più: la grazia si misura solo dall'accensione.
        b.riuscita_dall_accensione = Some(adesso - Duration::from_secs(60));
        b.riuscita_alle = Some(SystemTime::now() + Duration::from_secs(3600));
        assert!(b.recente(adesso));
        // L'orologio dall'accensione va avanti da solo.
        let a = tascabile_platform::dall_accensione();
        assert!(tascabile_platform::dall_accensione() >= a);
    }
}
