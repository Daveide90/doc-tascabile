//! Il dizionario italiano dell'ortografia (piano «Word completo», W4): i due file degli asset
//! letti al primo documento Word e dati al motore sul thread `word-dizionario` (meno di un decimo
//! di secondo sul PC); pronto, il layout del documento aperto si rifà con le sottolineature. Il
//! dizionario personale (le parole aggiunte) sta nella cartella privata dell'app e si legge
//! all'avvio. Nella diagnosi solo tempi e conti, mai una parola.

use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Instant;

use eframe::egui;
use wordcraft_engine::doc::Pos;

use tascabile_platform::diagnosi;
use tascabile_word::ortografia::{self, ASSET_AFF, ASSET_DIC};

use crate::TascabileApp;

/// Il dizionario italiano: se ne sta occupando il thread, è pronto o non c'è.
#[derive(Debug, Default)]
pub(crate) enum Dizionario {
    /// Nessun documento Word aperto finora.
    #[default]
    NonChiesto,
    /// Il thread lo sta preparando, da quando.
    InCarico(Receiver<Result<usize, &'static str>>, Instant),
    Pronto,
    /// Gli asset mancano o il motore non li ha capiti: niente sottolineature.
    Mancante,
}

impl TascabileApp {
    /// Al primo documento Word: i due file del dizionario dagli asset e il thread che li dà al
    /// motore. Le volte dopo non fa niente.
    pub(crate) fn carica_dizionario(&mut self) {
        if !matches!(self.dizionario, Dizionario::NonChiesto) {
            return;
        }
        let inizio = Instant::now();
        let (Some(aff), Some(dic)) = (self.piattaforma.leggi_asset(ASSET_AFF), self.piattaforma.leggi_asset(ASSET_DIC)) else {
            diagnosi("ortografia: dizionario mancante negli asset");
            self.dizionario = Dizionario::Mancante;
            return;
        };
        let (manda, ricevi) = channel();
        let risveglio = self.risveglio.clone();
        let avviato = std::thread::Builder::new().name("word-dizionario".into()).spawn(move || {
            let esito = ortografia::installa_italiano(&aff, &dic).map_err(|e| e.genere());
            let _ = manda.send(esito);
            risveglio();
        });
        self.dizionario = match avviato {
            Ok(_) => Dizionario::InCarico(ricevi, inizio),
            Err(_) => {
                diagnosi("ortografia: thread del dizionario non partito");
                Dizionario::Mancante
            }
        };
    }

    /// A ogni passo: il dizionario pronto rifà il layout del documento aperto, che così mostra le
    /// parole sbagliate.
    pub(crate) fn passo_dizionario(&mut self) {
        let Dizionario::InCarico(ricevi, inizio) = &self.dizionario else { return };
        let esito = match ricevi.try_recv() {
            Ok(esito) => esito,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("thread finito senza esito"),
        };
        let ms = inizio.elapsed().as_secs_f32() * 1000.0;
        match esito {
            Ok(radici) => {
                diagnosi(format!("ortografia: dizionario pronto, {radici} radici in {ms:.0} ms"));
                self.dizionario = Dizionario::Pronto;
                if let Some(e) = self.editor_mut() {
                    e.s.relayout();
                }
            }
            Err(genere) => {
                diagnosi(format!("ortografia: dizionario non caricato ({genere})"));
                self.dizionario = Dizionario::Mancante;
            }
        }
    }

    /// Per le prove: il dizionario è pronto.
    #[doc(hidden)]
    pub fn dizionario_pronto(&self) -> bool {
        matches!(self.dizionario, Dizionario::Pronto)
    }

    /// Per le prove: il rettangolo sullo schermo della prima volta di `parola` nei paragrafi del
    /// corpo, nella vista Scrittura com'era all'ultimo frame.
    #[doc(hidden)]
    pub fn parola_sullo_schermo_di_prova(&mut self, parola: &str) -> Option<egui::Rect> {
        let e = self.editor_mut()?;
        let (i, da) = e.s.doc.body.iter().enumerate().find_map(|(i, b)| Some((i, b.as_para()?.text.find(parola)?)))?;
        let (a, b) = (Pos::body(i, da), Pos::body(i, da + parola.len()));
        let l = e.s.layout();
        let r = l.selection_rects(&e.s.doc, &a, &b, e.s.page_hint).into_iter().map(|(_, r)| r).next()?;
        Some(egui::Rect::from_min_max(e.vista.sullo_schermo(r.x, r.y), e.vista.sullo_schermo(r.x + r.w, r.y + r.h)))
    }

    /// Per le prove: i testi dei paragrafi del corpo del documento Word aperto.
    #[doc(hidden)]
    pub fn testi_word_di_prova(&self) -> Vec<String> {
        self.editor().map(|e| e.s.doc.body.iter().filter_map(|b| b.as_para()).map(|p| p.text.clone()).collect()).unwrap_or_default()
    }
}
