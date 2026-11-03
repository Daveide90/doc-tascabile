//! Il foglio «Pagina» dell'Editor (piano «Word completo», W3), dal menu del documento: formato
//! (A4, A5, Lettera), orientamento e margini (normali, stretti, larghi) con un tocco, e
//! «Intestazione e piè di pagina», la sua pagina coi due campi, i numeri di pagina (in basso al
//! centro o a destra) e «Pagina N di M». Dietro il foglio la vista Pagina, che mostra subito quello
//! che cambia.

use eframe::egui::{self, RichText};
use tascabile_platform::diagnosi;
use tascabile_touch::tema::{self, Carattere};
use tascabile_touch::{CampoTesto, Chip, Peso, Pulsante, Tipo, Voce, etichetta_sezione, icone};
use tascabile_word::pagina::{Formato, IntestazioneEPie, Margini, NumeriPagina, info_pagina, intestazione_e_pie};
use tascabile_word::vista::Modo;

use crate::{QualeCampo, Schermata, TascabileApp};

/// Il titolo del foglio (e la voce del menu) e quello della sua seconda pagina.
pub(crate) const PAGINA: &str = "Pagina";
pub(crate) const INTESTAZIONE_E_PIE: &str = "Intestazione e piè di pagina";
/// I chip dell'orientamento.
pub(crate) const VERTICALE: &str = "Verticale";
pub(crate) const ORIZZONTALE: &str = "Orizzontale";
/// La seconda pagina: il testo grigio dei campi vuoti, i numeri, il totale, il pulsante.
pub(crate) const VUOTO_INTESTAZIONE: &str = "Testo in cima a ogni pagina";
pub(crate) const VUOTO_PIE: &str = "Testo in fondo a ogni pagina";
pub(crate) const NUMERI: [(NumeriPagina, &str); 3] =
    [(NumeriPagina::Nessuno, "Nessuno"), (NumeriPagina::AlCentro, "Al centro"), (NumeriPagina::ADestra, "A destra")];
pub(crate) const DI_TOTALE: &str = "Pagina N di M";
pub(crate) const APPLICA: &str = "Applica";
/// Al posto del campo, per un'intestazione o un piè di pagina che il foglio non riscrive: il
/// motore lo farebbe un paragrafo di testo semplice (analisi del 27/10, critico 5).
pub(crate) const RICCA: &str = "Ha tabelle, immagini o testo formattato: si cambia al PC.";
/// Al posto dei numeri, quando li ha già un piè di pagina che il foglio non riscrive.
pub(crate) const NUMERI_AL_PC: &str = "I numeri di questo piè di pagina si cambiano al PC.";

/// La seconda pagina del foglio: i due campi e le scelte dei numeri, presi quando si è aperta.
pub(crate) struct CampiPagina {
    pub(crate) intestazione: CampoTesto,
    pub(crate) pie: CampoTesto,
    pub(crate) numeri: NumeriPagina,
    pub(crate) di_totale: bool,
    /// Com'erano nel documento: col testo ricco il campo non c'è, e coi numeri del file nemmeno
    /// le loro scelte.
    pub(crate) intestazione_ricca: bool,
    pub(crate) pie_ricco: bool,
    pub(crate) numeri_del_file: NumeriPagina,
}

/// Che cosa si è toccato nella prima pagina del foglio.
enum NellaPagina {
    Formato(Formato),
    Orientamento(bool),
    Margini(Margini),
    Intestazioni,
}

/// Che cosa si è toccato nella seconda.
enum NelleIntestazioni {
    Campo(QualeCampo),
    Numeri(NumeriPagina),
    DiTotale,
    Applica,
}

/// Quello che c'è in intestazione e piè di pagina, in poche parole, sotto la voce.
fn riassunto(i: &IntestazioneEPie) -> String {
    let mut parti = Vec::new();
    if !i.intestazione.is_empty() {
        parti.push("intestazione".to_string());
    }
    if !i.pie.is_empty() {
        parti.push("piè di pagina".to_string());
    }
    let dove = match i.numeri {
        NumeriPagina::Nessuno => None,
        NumeriPagina::AlCentro => Some("al centro"),
        NumeriPagina::ADestra => Some("a destra"),
        NumeriPagina::ASinistra => Some("a sinistra"),
    };
    if let Some(dove) = dove {
        parti.push(if i.di_totale { format!("N di M {dove}") } else { format!("numeri {dove}") });
    }
    if parti.is_empty() {
        return "Nessuna".to_string();
    }
    let testo = parti.join(", ");
    let mut c = testo.chars();
    c.next().map(|p| p.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// Una misura in centimetri all'italiana, con una cifra dopo la virgola se serve: «21», «29,7».
fn cm(v: f32) -> String {
    let d = (v * 10.0).round() as i64;
    if d % 10 == 0 { format!("{}", d / 10) } else { format!("{},{}", d / 10, d % 10) }
}

/// Una fila di chip, uno acceso: restituisce quello toccato.
fn chip<T: Copy>(ui: &mut egui::Ui, voci: &[(T, &str)], acceso: impl Fn(T) -> bool) -> Option<T> {
    let mut scelto = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
        for (v, nome) in voci {
            if ui.add(Chip::new(nome).acceso(acceso(*v))).clicked() {
                scelto = Some(*v);
            }
        }
    });
    scelto
}

impl TascabileApp {
    /// «Pagina» dal menu: Trova si chiude, la tastiera pure, la vista passa a Pagina e il foglio
    /// sale.
    pub(crate) fn apri_foglio_pagina(&mut self) {
        self.chiudi_trova();
        if self.piattaforma.keyboard_visible() {
            self.piattaforma.hide_keyboard();
        }
        let Some(e) = self.editor_mut() else { return };
        e.ponte.chiudi();
        e.vista.cambia_modo(Modo::Pagina);
        diagnosi("pagina: foglio aperto");
        self.campi_pagina = None;
        self.foglio_pagina.apri();
    }

    /// Il foglio «Pagina», o la sua seconda pagina coi campi.
    pub(crate) fn mostra_foglio_pagina(&mut self, ctx: &egui::Context) {
        if self.schermata != Schermata::Editor || self.aperto.is_none() {
            self.foglio_pagina.chiudi();
        }
        if !self.foglio_pagina.aperto() {
            if self.campi_pagina.is_some() {
                self.chiudi_campi_word();
            }
            return;
        }
        if self.campi_pagina.is_some() {
            self.pagina_delle_intestazioni(ctx);
            return;
        }
        let Some(e) = self.editor() else { return };
        let info = info_pagina(&e.s);
        let sotto = riassunto(&intestazione_e_pie(&e.s));
        let formati: Vec<(Formato, &str)> = Formato::TUTTI.iter().map(|f| (*f, f.nome())).collect();
        let margini: Vec<(Margini, &str)> = Margini::TUTTI.iter().map(|m| (*m, m.nome())).collect();
        let scelto = self
            .foglio_pagina
            .mostra(ctx, PAGINA, |ui| {
                let mut scelto = None;
                ui.spacing_mut().item_spacing.y = 6.0;
                etichetta_sezione(ui, "Formato");
                if let Some(f) = chip(ui, &formati, |f| info.formato == Some(f)) {
                    scelto = Some(NellaPagina::Formato(f));
                }
                ui.label(
                    RichText::new(format!("{} × {} cm", cm(info.larghezza_cm), cm(info.altezza_cm)))
                        .font(Carattere::Testo.font(ui.ctx(), 13.0))
                        .color(tema::MATITA),
                );
                etichetta_sezione(ui, "Orientamento");
                if let Some(o) = chip(ui, &[(false, VERTICALE), (true, ORIZZONTALE)], |o| info.orizzontale == o) {
                    scelto = Some(NellaPagina::Orientamento(o));
                }
                etichetta_sezione(ui, "Margini");
                if let Some(m) = chip(ui, &margini, |m| info.margini == Some(m)) {
                    scelto = Some(NellaPagina::Margini(m));
                }
                ui.add_space(6.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                if Voce::new(Tipo::Word, INTESTAZIONE_E_PIE).icona(icone::PAGE_HEADER).sotto(&sotto).ultima(true).mostra(ui).aperta() {
                    scelto = Some(NellaPagina::Intestazioni);
                }
                scelto
            })
            .flatten();
        let Some(scelto) = scelto else { return };
        if let NellaPagina::Intestazioni = scelto {
            self.apri_intestazioni();
            return;
        }
        let Some(e) = self.editor_mut() else { return };
        let st = match scelto {
            NellaPagina::Formato(f) => e.formato_pagina(f),
            NellaPagina::Orientamento(o) => e.orientamento_pagina(o),
            NellaPagina::Margini(m) => e.margini_pagina(m),
            NellaPagina::Intestazioni => None,
        };
        self.dopo_ponte(st);
    }

    /// La seconda pagina: i campi con quello che c'è adesso.
    fn apri_intestazioni(&mut self) {
        let Some(e) = self.editor() else { return };
        let adesso = intestazione_e_pie(&e.s);
        diagnosi("pagina: intestazione e piè di pagina");
        self.campi_pagina = Some(CampiPagina {
            intestazione: CampoTesto::new().con_testo(&adesso.intestazione),
            pie: CampoTesto::new().con_testo(&adesso.pie),
            numeri: adesso.numeri,
            di_totale: adesso.di_totale,
            intestazione_ricca: adesso.intestazione_ricca,
            pie_ricco: adesso.pie_ricco,
            numeri_del_file: adesso.numeri,
        });
    }

    /// La seconda pagina del foglio: l'intestazione, il piè di pagina, i numeri e «Applica».
    fn pagina_delle_intestazioni(&mut self, ctx: &egui::Context) {
        let Some(c) = self.campi_pagina.as_ref() else { return };
        let scelto = self
            .foglio_pagina
            .mostra(ctx, INTESTAZIONE_E_PIE, |ui| {
                let mut scelto = None;
                ui.spacing_mut().item_spacing.y = 6.0;
                let nota = |ui: &mut egui::Ui, testo: &str| {
                    ui.label(RichText::new(testo).font(Carattere::Testo.font(ui.ctx(), 13.0)).color(tema::MATITA));
                };
                etichetta_sezione(ui, "Intestazione");
                if c.intestazione_ricca {
                    nota(ui, RICCA);
                } else if c.intestazione.mostra(ui, VUOTO_INTESTAZIONE).clicked() {
                    scelto = Some(NelleIntestazioni::Campo(QualeCampo::Intestazione));
                }
                etichetta_sezione(ui, "Piè di pagina");
                if c.pie_ricco {
                    nota(ui, RICCA);
                } else if c.pie.mostra(ui, VUOTO_PIE).clicked() {
                    scelto = Some(NelleIntestazioni::Campo(QualeCampo::Pie));
                }
                etichetta_sezione(ui, "Numeri di pagina, in basso");
                if c.pie_ricco && c.numeri_del_file != NumeriPagina::Nessuno {
                    nota(ui, NUMERI_AL_PC);
                } else {
                    if let Some(n) = chip(ui, &NUMERI, |n| c.numeri == n) {
                        scelto = Some(NelleIntestazioni::Numeri(n));
                    }
                    if c.numeri != NumeriPagina::Nessuno && ui.add(Chip::new(DI_TOTALE).acceso(c.di_totale)).clicked() {
                        scelto = Some(NelleIntestazioni::DiTotale);
                    }
                }
                ui.add_space(4.0);
                let largo = ui.available_width();
                if ui.add(Pulsante::new(APPLICA, Peso::Principale).icona(icone::CHECK).larghezza(largo)).clicked() {
                    scelto = Some(NelleIntestazioni::Applica);
                }
                scelto
            })
            .flatten();
        match scelto {
            Some(NelleIntestazioni::Campo(q)) => self.fuoco_al_campo_word(q),
            Some(NelleIntestazioni::Numeri(n)) => {
                if let Some(c) = self.campi_pagina.as_mut() {
                    c.numeri = n;
                }
            }
            Some(NelleIntestazioni::DiTotale) => {
                if let Some(c) = self.campi_pagina.as_mut() {
                    c.di_totale = !c.di_totale;
                }
            }
            Some(NelleIntestazioni::Applica) => self.applica_intestazioni(),
            None => {}
        }
        if !self.foglio_pagina.aperto() && self.campi_pagina.is_some() {
            self.chiudi_campi_word();
        }
    }

    /// «Applica» (o Invio nel piè di pagina): intestazione, piè di pagina e numeri nel documento,
    /// in un passo di Annulla; il foglio si chiude e resta la vista Pagina, che li mostra.
    pub(crate) fn applica_intestazioni(&mut self) {
        let Some(c) = self.campi_pagina.as_ref() else { return };
        let nuovo = IntestazioneEPie {
            intestazione: c.intestazione.testo().to_string(),
            pie: c.pie.testo().to_string(),
            numeri: c.numeri,
            di_totale: c.di_totale,
            ..Default::default()
        };
        self.chiudi_campi_word();
        self.foglio_pagina.chiudi();
        if let Some(e) = self.editor_mut() {
            let st = e.cambia_intestazione_e_pie(&nuovo);
            e.vista.cambia_modo(Modo::Pagina);
            self.dopo_ponte(st);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn il_riassunto_e_le_misure() {
        assert_eq!(riassunto(&IntestazioneEPie::default()), "Nessuna");
        let tutto =
            IntestazioneEPie { intestazione: "A".into(), pie: "B".into(), numeri: NumeriPagina::AlCentro, di_totale: true, ..Default::default() };
        assert_eq!(riassunto(&tutto), "Intestazione, piè di pagina, N di M al centro");
        let numeri = IntestazioneEPie { numeri: NumeriPagina::ADestra, ..Default::default() };
        assert_eq!(riassunto(&numeri), "Numeri a destra");
        assert_eq!((cm(21.0), cm(29.7), cm(14.8)), ("21".to_string(), "29,7".to_string(), "14,8".to_string()));
    }
}
