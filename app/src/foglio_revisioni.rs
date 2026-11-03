//! Commenti e revisioni nell'Editor (piano «Word completo», W4). Nel menu del documento la voce
//! «Revisioni» apre il foglio con l'interruttore «Tieni traccia delle modifiche», le modifiche da
//! accettare o rifiutare (una alla volta o tutte) e i commenti con le risposte (Rispondi, Risolvi,
//! Elimina); un tocco sul testo di una modifica o di un commento porta il cursore lì. «Commenta»
//! nella barra della selezione, e Rispondi, aprono il foglio «Commento» col suo campo. Nel testo
//! commenti e modifiche si vedono coi colori del motore. L'autore viene dalle Impostazioni.
//! Nella diagnosi solo conti e comandi.

use eframe::egui::{self, Label, RichText, Sense, vec2};
use tascabile_platform::diagnosi;
use tascabile_touch::tema::{self, Carattere};
use tascabile_touch::{CampoTesto, Peso, Pulsante, PulsanteIcona, Tipo, Voce, etichetta_sezione, icone};
use tascabile_word::revisioni::{Commento, Modifica, StatoRevisioni, data_breve, stato};
use wordcraft_engine::doc::Pos;

use crate::{QualeCampo, Schermata, TascabileApp};

/// Il titolo del foglio e la voce del menu.
pub(crate) const REVISIONI: &str = "Revisioni";
pub(crate) const TRACCIA: &str = "Tieni traccia delle modifiche";
const SPIEGA_TRACCIA: &str = "Le modifiche restano da accettare";
pub(crate) const ACCETTA: &str = "Accetta";
pub(crate) const RIFIUTA: &str = "Rifiuta";
pub(crate) const ACCETTA_TUTTO: &str = "Accetta tutto";
pub(crate) const RIFIUTA_TUTTO: &str = "Rifiuta tutto";
pub(crate) const RISPONDI: &str = "Rispondi";
pub(crate) const RISOLVI: &str = "Risolvi";
pub(crate) const RIAPRI: &str = "Riapri";
pub(crate) const ELIMINA: &str = "Elimina";
/// Il foglio del commento: i due titoli, il testo grigio del campo vuoto, il pulsante.
pub(crate) const COMMENTO: &str = "Commento";
pub(crate) const RISPOSTA: &str = "Risposta";
pub(crate) const VUOTO_COMMENTO: &str = "Scrivi il commento";
pub(crate) const VUOTO_RISPOSTA: &str = "Scrivi la risposta";
pub(crate) const FATTO: &str = "Fatto";
/// Il nome di chi non ha scritto il suo nome.
const SENZA_NOME: &str = "Senza nome";

/// Il campo del foglio del commento: un commento nuovo sulla selezione, o la risposta a un commento.
pub(crate) struct CampoCommento {
    pub(crate) campo: CampoTesto,
    pub(crate) risposta_a: Option<u32>,
}

/// Che cosa si è toccato nel foglio «Revisioni».
enum NelleRevisioni {
    Traccia,
    Modifica(usize, bool),
    Tutte(bool),
    Rispondi(u32),
    Risolvi(u32, bool),
    Elimina(u32),
    Vai(Pos),
}

/// Una modifica: il genere, il testo (si tocca per andarci) e chi l'ha fatta; a destra Accetta e
/// Rifiuta.
fn riga_modifica(ui: &mut egui::Ui, i: usize, m: &Modifica, ultima: bool) -> Option<NelleRevisioni> {
    let ctx = ui.ctx().clone();
    let largo = ui.available_width();
    let mut scelta = None;
    ui.horizontal(|ui| {
        ui.set_min_height(tema::VOCE);
        let colonna = largo - 2.0 * tema::PULSANTE - 2.0 * ui.spacing().item_spacing.x;
        ui.allocate_ui_with_layout(vec2(colonna, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_width(colonna);
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.add_space(6.0);
            ui.label(RichText::new(m.genere.nome()).font(Carattere::Grassetto.font(&ctx, 15.0)).color(tema::INCHIOSTRO));
            if !m.testo.is_empty() {
                let r = ui.add(
                    Label::new(RichText::new(format!("«{}»", m.testo)).font(Carattere::Testo.font(&ctx, 15.0)).color(tema::INCHIOSTRO))
                        .selectable(false),
                );
                if ui.interact(r.rect, ui.id().with(("modifica", i)), Sense::click()).clicked() {
                    scelta = Some(NelleRevisioni::Vai(m.inizio.clone()));
                }
            }
            let autore = if m.autore.trim().is_empty() { SENZA_NOME } else { m.autore.as_str() };
            ui.label(RichText::new(autore).font(Carattere::Testo.font(&ctx, 13.0)).color(tema::MATITA));
            ui.add_space(6.0);
        });
        if ui.add(PulsanteIcona::new(icone::CHECK, ACCETTA)).clicked() {
            scelta = Some(NelleRevisioni::Modifica(i, true));
        }
        if ui.add(PulsanteIcona::new(icone::CLOSE, RIFIUTA)).clicked() {
            scelta = Some(NelleRevisioni::Modifica(i, false));
        }
    });
    if !ultima {
        filo(ui);
    }
    scelta
}

/// Un commento: chi e quando, il testo (si tocca per andarci), le risposte rientrate, «Risolto» se
/// lo è; sotto Rispondi, Risolvi (o Riapri) ed Elimina.
fn scheda_commento(ui: &mut egui::Ui, c: &Commento, ultimo: bool) -> Option<NelleRevisioni> {
    let ctx = ui.ctx().clone();
    let mut scelta = None;
    let chi = |autore: &str, data: &str| {
        let nome = if autore.trim().is_empty() { SENZA_NOME.to_string() } else { autore.trim().to_string() };
        let data = data_breve(data);
        if data.is_empty() { nome } else { format!("{nome} · {data}") }
    };
    ui.add_space(10.0);
    ui.spacing_mut().item_spacing.y = 2.0;
    ui.label(RichText::new(chi(&c.autore, &c.data)).font(Carattere::Grassetto.font(&ctx, 14.0)).color(tema::MATITA));
    let testo = ui.add(Label::new(RichText::new(&c.testo).font(Carattere::Testo.font(&ctx, 16.0)).color(tema::INCHIOSTRO)).selectable(false));
    if ui.interact(testo.rect, ui.id().with(("commento", c.id)), Sense::click()).clicked()
        && let Some(a) = &c.ancora
    {
        scelta = Some(NelleRevisioni::Vai(a.clone()));
    }
    for r in &c.risposte {
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.add_space(4.0);
                ui.label(RichText::new(chi(&r.autore, &r.data)).font(Carattere::Grassetto.font(&ctx, 13.0)).color(tema::MATITA));
                ui.label(RichText::new(&r.testo).font(Carattere::Testo.font(&ctx, 15.0)).color(tema::INCHIOSTRO));
            });
        });
    }
    if c.risolto {
        ui.label(RichText::new(format!("{} Risolto", icone::TASK_ALT)).font(Carattere::Grassetto.font(&ctx, 13.0)).color(tema::VERDE));
    }
    ui.add_space(4.0);
    // I tre pulsanti si dividono la larghezza: larghi quanto il testo non ci stavano su un
    // telefono da 372 dp e allargavano il foglio fuori dallo schermo (0.8.3).
    let terzo = ((ui.available_width() - 8.0) / 3.0).floor();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        if ui.add(Pulsante::new(RISPONDI, Peso::Testo).icona(icone::REPLY).larghezza(terzo)).clicked() {
            scelta = Some(NelleRevisioni::Rispondi(c.id));
        }
        let (etichetta, icona) = if c.risolto { (RIAPRI, icone::UNDO) } else { (RISOLVI, icone::TASK_ALT) };
        if ui.add(Pulsante::new(etichetta, Peso::Testo).icona(icona).larghezza(terzo)).clicked() {
            scelta = Some(NelleRevisioni::Risolvi(c.id, !c.risolto));
        }
        if ui.add(Pulsante::new(ELIMINA, Peso::Testo).icona(icone::DELETE).larghezza(terzo)).clicked() {
            scelta = Some(NelleRevisioni::Elimina(c.id));
        }
    });
    ui.add_space(6.0);
    if !ultimo {
        filo(ui);
    }
    scelta
}

/// La linea sottile fra due righe.
fn filo(ui: &mut egui::Ui) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(r, 0.0, tema::FILO);
}

/// Il titolo di una sezione del foglio, grigio e maiuscolo, col numero.
fn sezione(ui: &mut egui::Ui, titolo: &str, n: usize) {
    ui.add_space(14.0);
    etichetta_sezione(ui, &format!("{titolo} · {n}"));
    ui.add_space(4.0);
}

/// Il foglio: l'interruttore, le modifiche coi due pulsanti di tutte, i commenti.
fn pagina_delle_revisioni(ui: &mut egui::Ui, st: &StatoRevisioni) -> Option<NelleRevisioni> {
    let ctx = ui.ctx().clone();
    ui.spacing_mut().item_spacing.y = 0.0;
    let mut scelta = None;
    if Voce::new(Tipo::Word, TRACCIA).icona(icone::TRACK_CHANGES).sotto(SPIEGA_TRACCIA).interruttore(st.traccia).ultima(true).mostra(ui).aperta() {
        scelta = Some(NelleRevisioni::Traccia);
    }
    if st.modifiche.is_empty() && st.commenti.is_empty() {
        ui.add_space(14.0);
        ui.label(
            RichText::new("Nessuna modifica né commento. Per un commento scegli il testo e tocca «Commenta».")
                .font(Carattere::Testo.font(&ctx, 15.0))
                .color(tema::MATITA),
        );
        ui.add_space(8.0);
        return scelta;
    }
    if !st.modifiche.is_empty() {
        sezione(ui, "Modifiche", st.modifiche.len());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if ui.add(Pulsante::new(ACCETTA_TUTTO, Peso::Tenue).icona(icone::DONE_ALL)).clicked() {
                scelta = Some(NelleRevisioni::Tutte(true));
            }
            if ui.add(Pulsante::new(RIFIUTA_TUTTO, Peso::Tenue).icona(icone::DELETE_SWEEP)).clicked() {
                scelta = Some(NelleRevisioni::Tutte(false));
            }
        });
        ui.add_space(4.0);
        for (i, m) in st.modifiche.iter().enumerate() {
            if let Some(s) = riga_modifica(ui, i, m, i + 1 == st.modifiche.len()) {
                scelta = Some(s);
            }
        }
    }
    if !st.commenti.is_empty() {
        sezione(ui, "Commenti", st.commenti.len());
        for (i, c) in st.commenti.iter().enumerate() {
            if let Some(s) = scheda_commento(ui, c, i + 1 == st.commenti.len()) {
                scelta = Some(s);
            }
        }
    }
    ui.add_space(8.0);
    scelta
}

impl TascabileApp {
    /// «Revisioni» dal menu del documento: il foglio con quello che c'è adesso.
    pub(crate) fn apri_revisioni(&mut self) {
        let Some(e) = self.editor_mut() else { return };
        let st = stato(&mut e.s);
        diagnosi(format!("revisioni: foglio aperto, {} modifiche, {} commenti", st.modifiche.len(), st.commenti.len()));
        self.revisioni = Some(st);
        self.foglio_revisioni.apri();
    }

    /// Il foglio «Revisioni», se è aperto.
    pub(crate) fn mostra_foglio_revisioni(&mut self, ctx: &egui::Context) {
        if self.schermata != Schermata::Editor || self.aperto.is_none() {
            self.foglio_revisioni.chiudi();
        }
        if !self.foglio_revisioni.aperto() {
            self.revisioni = None;
            return;
        }
        let Some(st) = self.revisioni.as_ref() else {
            self.foglio_revisioni.chiudi();
            return;
        };
        let scelta = self.foglio_revisioni.mostra(ctx, REVISIONI, |ui| pagina_delle_revisioni(ui, st)).flatten();
        if let Some(s) = scelta {
            self.nelle_revisioni(s);
        }
    }

    fn nelle_revisioni(&mut self, scelta: NelleRevisioni) {
        let modifiche = self.revisioni.as_ref().map(|r| r.modifiche.clone()).unwrap_or_default();
        let traccia = self.revisioni.as_ref().is_some_and(|r| r.traccia);
        let Some(e) = self.editor_mut() else { return };
        let st = match scelta {
            NelleRevisioni::Traccia => e.traccia_modifiche(!traccia),
            NelleRevisioni::Modifica(i, accetta) => match modifiche.get(i) {
                Some(m) => e.risolvi_modifica(m, accetta),
                None => None,
            },
            NelleRevisioni::Tutte(accetta) => e.risolvi_tutte(accetta),
            NelleRevisioni::Risolvi(id, risolto) => e.risolvi_commento(id, risolto),
            NelleRevisioni::Elimina(id) => e.elimina_commento(id),
            NelleRevisioni::Rispondi(id) => {
                self.foglio_revisioni.chiudi();
                self.apri_commento(Some(id));
                return;
            }
            NelleRevisioni::Vai(pos) => {
                let st = e.vai_a(&pos);
                self.foglio_revisioni.chiudi();
                self.dopo_ponte(st);
                return;
            }
        };
        self.dopo_ponte(st);
        if let Some(e) = self.editor_mut() {
            let nuovo = stato(&mut e.s);
            self.revisioni = Some(nuovo);
        }
    }

    /// Il foglio del commento col campo che prende la tastiera: un commento nuovo sulla selezione
    /// («Commenta» della barra) o la risposta al commento `risposta_a`.
    pub(crate) fn apri_commento(&mut self, risposta_a: Option<u32>) {
        diagnosi(if risposta_a.is_some() { "revisioni: foglio della risposta" } else { "revisioni: foglio del commento" });
        self.commento = Some(crate::foglio_revisioni::CampoCommento { campo: CampoTesto::new(), risposta_a });
        self.foglio_commento.apri();
        self.fuoco_al_campo_word(QualeCampo::Commento);
    }

    /// «Fatto» (o Invio nel campo): il commento sulla selezione, o la risposta; un campo vuoto non
    /// scrive niente. Dopo una risposta torna il foglio «Revisioni».
    pub(crate) fn conferma_commento(&mut self) {
        let Some((testo, risposta_a)) = self.commento.as_ref().map(|c| (c.campo.testo().trim().to_string(), c.risposta_a)) else { return };
        if testo.is_empty() {
            return;
        }
        self.chiudi_campi_word();
        self.foglio_commento.chiudi();
        let Some(e) = self.editor_mut() else { return };
        let st = match risposta_a {
            Some(id) => e.rispondi(id, &testo),
            None => e.commenta(&testo),
        };
        self.dopo_ponte(st);
        if risposta_a.is_some() {
            self.apri_revisioni();
        }
    }

    /// Il foglio «Commento» (o «Risposta»): a che cosa va, il nome con cui va, il campo e «Fatto».
    pub(crate) fn mostra_foglio_commento(&mut self, ctx: &egui::Context) {
        if self.schermata != Schermata::Editor || self.aperto.is_none() {
            self.foglio_commento.chiudi();
        }
        if !self.foglio_commento.aperto() {
            if self.commento.is_some() {
                self.chiudi_campi_word();
            }
            return;
        }
        let Some(c) = self.commento.as_ref() else {
            self.foglio_commento.chiudi();
            return;
        };
        let risposta = c.risposta_a.is_some();
        let autore = self.impostazioni.autore.clone();
        let vuoto = c.campo.testo().trim().is_empty();
        let (mut tocco, mut fatto) = (false, false);
        let titolo = if risposta { RISPOSTA } else { COMMENTO };
        self.foglio_commento.mostra(ctx, titolo, |ui| {
            ui.spacing_mut().item_spacing.y = 12.0;
            let dove = if risposta { "La risposta va sotto il commento" } else { "Il commento va sul testo scelto" };
            let chi = if autore.is_empty() {
                format!("{dove}, senza nome: il nome si scrive nelle Impostazioni, in «{}».", crate::impostazioni::AUTORE)
            } else {
                format!("{dove}, col nome {autore}.")
            };
            ui.label(RichText::new(chi).font(Carattere::Testo.font(ui.ctx(), 15.0)).color(tema::MATITA));
            tocco = c.campo.mostra(ui, if risposta { VUOTO_RISPOSTA } else { VUOTO_COMMENTO }).clicked();
            let largo = ui.available_width();
            fatto = ui.add(Pulsante::new(FATTO, Peso::Principale).icona(icone::CHECK).larghezza(largo).spento(vuoto)).clicked();
        });
        if tocco {
            self.fuoco_al_campo_word(QualeCampo::Commento);
        }
        if fatto {
            self.conferma_commento();
        }
        if !self.foglio_commento.aperto() && self.commento.is_some() {
            self.chiudi_campi_word();
        }
    }
}
