//! Le tabelle nell'Editor (piano «Word completo», W2): la scelta di righe e colonne nel foglio
//! «Inserisci», la barra della tabella sopra la barra di formato quando il cursore è in una
//! tabella, il menu di Elimina che sale dalla barra e il foglio «Bordi e sfondo».

use eframe::egui::{self, Color32, Rect, RichText, pos2};
use tascabile_platform::diagnosi;
use tascabile_touch::tema::{self, Carattere};
use tascabile_touch::{
    BarraScorrevole, GrigliaTabella, Menu, Peso, Pulsante, SceltaMenu, Tavolozza, Tinta, etichetta_sezione, icone, misura_tabella,
};
use tascabile_word::tabella::{Bordi, COLONNE_MASSIME, ComandoTabella, InfoTabella, RIGHE_MASSIME};

use crate::{Schermata, TascabileApp};

/// Le voci della barra della tabella (nome sulla barra, che è anche l'etichetta).
pub(crate) const RIGA_SOPRA: &str = "Riga sopra";
pub(crate) const RIGA_SOTTO: &str = "Riga sotto";
pub(crate) const COLONNA_SINISTRA: &str = "Colonna a sinistra";
pub(crate) const COLONNA_DESTRA: &str = "Colonna a destra";
pub(crate) const ELIMINA: &str = "Elimina";
pub(crate) const UNISCI: &str = "Unisci";
pub(crate) const DIVIDI: &str = "Dividi";
pub(crate) const BORDI_E_SFONDO: &str = "Bordi e sfondo";
/// L'interruttore dell'intestazione: «Intestazione» sulla barra, per esteso nell'etichetta.
pub(crate) const INTESTAZIONE: &str = "Intestazione ripetuta";
/// Le voci del menu di Elimina.
pub(crate) const ELIMINA_RIGA: &str = "Elimina la riga";
pub(crate) const ELIMINA_COLONNA: &str = "Elimina la colonna";
pub(crate) const ELIMINA_TABELLA: &str = "Elimina la tabella";
/// La voce del foglio «Inserisci» e il pulsante della sua griglia.
pub(crate) const TABELLA: &str = "Tabella";
pub(crate) const INSERISCI_TABELLA: &str = "Inserisci la tabella";
/// La misura della griglia quando si apre.
const SCELTA_INIZIALE: (usize, usize) = (3, 3);
/// Fra la barra della tabella e quella di formato, in dp.
const SOPRA_LA_FORMATO: f32 = 8.0;

/// I bordi del foglio: (bordi, icona, nome).
const BORDI: [(Bordi, &str, &str); 4] = [
    (Bordi::Tutti, icone::BORDER_ALL, "Tutti"),
    (Bordi::Contorno, icone::BORDER_OUTER, "Contorno"),
    (Bordi::Dentro, icone::BORDER_INNER, "Dentro"),
    (Bordi::Nessuno, icone::BORDER_CLEAR, "Nessuno"),
];

/// Gli sfondi delle celle: nessuno, poi sei tinte chiare dei temi di Word, su cui il testo nero si
/// legge. (tinta, nome, colore).
const SFONDI: [(Tinta, &str, Option<[u8; 3]>); 7] = [
    (Tinta::Nessuno, "Nessuno sfondo", None),
    (Tinta::Colore(Color32::from_rgb(0xD9, 0xD9, 0xD9)), "Grigio chiaro", Some([0xD9, 0xD9, 0xD9])),
    (Tinta::Colore(Color32::from_rgb(0xFF, 0xF2, 0xCC)), "Giallo chiaro", Some([0xFF, 0xF2, 0xCC])),
    (Tinta::Colore(Color32::from_rgb(0xE2, 0xEF, 0xDA)), "Verde chiaro", Some([0xE2, 0xEF, 0xDA])),
    (Tinta::Colore(Color32::from_rgb(0xDD, 0xEB, 0xF7)), "Azzurro chiaro", Some([0xDD, 0xEB, 0xF7])),
    (Tinta::Colore(Color32::from_rgb(0xFC, 0xE4, 0xD6)), "Arancione chiaro", Some([0xFC, 0xE4, 0xD6])),
    (Tinta::Colore(Color32::from_rgb(0xE4, 0xDF, 0xEC)), "Lilla", Some([0xE4, 0xDF, 0xEC])),
];

/// Il posto della barra della tabella: sopra quella di formato `formato`, larga uguale.
pub(crate) fn posto_della_barra(formato: Rect) -> Rect {
    Rect::from_min_max(pos2(formato.left(), formato.top() - SOPRA_LA_FORMATO - tema::VOCI), pos2(formato.right(), formato.top() - SOPRA_LA_FORMATO))
}

/// Che cosa si è toccato nel foglio «Inserisci» con la griglia aperta.
pub(crate) enum NellaGriglia {
    Misura((usize, usize)),
    Inserisci,
}

/// La pagina della griglia nel foglio «Inserisci»: la misura a parole, la griglia, il pulsante.
pub(crate) fn pagina_della_griglia(ui: &mut egui::Ui, scelta: (usize, usize)) -> Option<NellaGriglia> {
    let ctx = ui.ctx().clone();
    let mut fatto = None;
    ui.spacing_mut().item_spacing.y = 12.0;
    ui.label(RichText::new(misura_tabella(scelta.0, scelta.1)).font(Carattere::Grassetto.font(&ctx, 16.0)).color(tema::INCHIOSTRO));
    if let Some(m) = GrigliaTabella::new(RIGHE_MASSIME, COLONNE_MASSIME, scelta).mostra(ui) {
        fatto = Some(NellaGriglia::Misura(m));
    }
    let largo = ui.available_width();
    if ui.add(Pulsante::new(INSERISCI_TABELLA, Peso::Principale).icona(icone::TABLE).larghezza(largo)).clicked() {
        fatto = Some(NellaGriglia::Inserisci);
    }
    fatto
}

impl TascabileApp {
    /// La voce «Tabella» del foglio «Inserisci»: il foglio mostra la griglia.
    pub(crate) fn apri_griglia_tabella(&mut self) {
        diagnosi("inserisci: griglia della tabella");
        self.tabella_scelta = Some(SCELTA_INIZIALE);
    }

    /// Una misura scelta nella griglia.
    pub(crate) fn scegli_misura_tabella(&mut self, m: (usize, usize)) {
        self.tabella_scelta = Some(m);
    }

    /// «Inserisci la tabella»: il foglio si chiude, la tabella va al cursore e la tastiera si apre
    /// nella prima cella.
    pub(crate) fn inserisci_tabella_scelta(&mut self) {
        let Some((righe, colonne)) = self.tabella_scelta.take() else { return };
        self.foglio_inserisci.chiudi();
        self.fuoco_al_documento();
        let Some(e) = self.editor_mut() else { return };
        let st = e.inserisci_tabella(righe, colonne);
        let collegata = st.is_some();
        self.dopo_ponte(st);
        if collegata {
            self.piattaforma.show_keyboard();
        }
    }

    /// La barra della tabella in `posto`: righe e colonne, Elimina (col suo menu), Unisci (con
    /// due celle scelte o più), Dividi, Bordi e sfondo (col suo foglio), Intestazione ripetuta.
    pub(crate) fn barra_tabella(&mut self, ui: &mut egui::Ui, posto: Rect, info: InfoTabella) {
        let scelta = BarraScorrevole::new()
            .voce(icone::ADD_ROW_ABOVE, RIGA_SOPRA, false)
            .voce(icone::ADD_ROW_BELOW, RIGA_SOTTO, false)
            .voce(icone::ADD_COLUMN_LEFT, COLONNA_SINISTRA, false)
            .voce(icone::ADD_COLUMN_RIGHT, COLONNA_DESTRA, false)
            .voce(icone::DELETE, ELIMINA, false)
            .voce(icone::CELL_MERGE, UNISCI, info.celle_scelte < 2)
            .voce(icone::VERTICAL_SPLIT, DIVIDI, false)
            .voce(icone::BORDER_ALL, BORDI_E_SFONDO, false)
            .interruttore(icone::TABLE_ROWS, "Intestazione", INTESTAZIONE, info.intestazione)
            .mostra(ui, posto);
        let Some((i, dove)) = scelta else { return };
        let comando = match i {
            0 => ComandoTabella::RigaSopra,
            1 => ComandoTabella::RigaSotto,
            2 => ComandoTabella::ColonnaSinistra,
            3 => ComandoTabella::ColonnaDestra,
            4 => {
                diagnosi("barra della tabella: menu di elimina");
                self.menu_tabella = Some(pos2(dove.center().x, dove.top() - 4.0));
                return;
            }
            5 => ComandoTabella::Unisci,
            6 => ComandoTabella::Dividi,
            7 => {
                diagnosi("barra della tabella: bordi e sfondo");
                self.foglio_bordi.apri();
                return;
            }
            _ => ComandoTabella::Intestazione,
        };
        self.comando_tabella(comando);
    }

    /// Per le prove: il centro sullo schermo della cella (`riga`, `cella`) della prima tabella del
    /// documento, nella vista Scrittura com'era all'ultimo frame.
    #[doc(hidden)]
    pub fn centro_cella_di_prova(&mut self, riga: usize, cella: usize) -> Option<egui::Pos2> {
        let e = self.editor_mut()?;
        let l = e.s.layout();
        let r = l.pages.first()?.items.iter().find_map(|it| match it {
            wordcraft_engine::layout::Placed::Cell { rect, row, cell, covered: false, .. } if *row == riga && *cell == cella => Some(*rect),
            _ => None,
        })?;
        Some(e.vista.sullo_schermo(r.x + r.w / 2.0, r.y + r.h / 2.0))
    }

    fn comando_tabella(&mut self, c: ComandoTabella) {
        if let Some(e) = self.editor_mut() {
            let st = e.tabella(c);
            self.dopo_ponte(st);
        }
    }

    /// Il menu di Elimina sopra la sua voce, se è aperto.
    pub(crate) fn mostra_menu_tabella(&mut self, ctx: &egui::Context) {
        let Some(punto) = self.menu_tabella else { return };
        let scelta = Menu::new(egui::Id::new("menu_tabella"))
            .voce(icone::TABLE_ROWS, ELIMINA_RIGA)
            .voce(icone::VIEW_COLUMN, ELIMINA_COLONNA)
            .voce(icone::DELETE, ELIMINA_TABELLA)
            .sopra()
            .mostra(ctx, punto);
        let Some(scelta) = scelta else { return };
        self.menu_tabella = None;
        let SceltaMenu::Voce(i) = scelta else { return };
        let c = match i {
            0 => ComandoTabella::EliminaRiga,
            1 => ComandoTabella::EliminaColonna,
            _ => ComandoTabella::EliminaTabella,
        };
        self.comando_tabella(c);
    }

    /// Il foglio «Bordi e sfondo»: i bordi della tabella intera e lo sfondo delle celle scelte (o
    /// di quella del cursore). Resta aperto per il comando dopo; si chiude se il cursore esce dalla
    /// tabella.
    pub(crate) fn mostra_foglio_bordi(&mut self, ctx: &egui::Context) {
        let info = self.editor().and_then(|e| tascabile_word::tabella::info_tabella(&e.s));
        if self.schermata != Schermata::Editor || info.is_none() {
            self.foglio_bordi.chiudi();
        }
        let sfondo = info.and_then(|i| i.sfondo);
        let scelto = self
            .foglio_bordi
            .mostra(ctx, BORDI_E_SFONDO, |ui| {
                let mut scelto = None;
                etichetta_sezione(ui, "Bordi della tabella");
                let largo = (ui.available_width() - 10.0) / 2.0;
                for coppia in BORDI.chunks(2) {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        for (b, icona, nome) in coppia {
                            if ui.add(Pulsante::new(nome, Peso::Tenue).icona(icona).larghezza(largo)).clicked() {
                                scelto = Some(ComandoTabella::Bordi(*b));
                            }
                        }
                    });
                }
                etichetta_sezione(ui, "Sfondo delle celle");
                let tinte: Vec<(Tinta, &str)> = SFONDI.iter().map(|(t, n, _)| (*t, *n)).collect();
                let adesso = SFONDI.iter().position(|(_, _, c)| *c == sfondo);
                if let Some(i) = Tavolozza::new(&tinte).acceso(adesso).mostra(ui) {
                    scelto = Some(ComandoTabella::Sfondo(SFONDI[i].2));
                }
                ui.label(
                    RichText::new("Lo sfondo va sulle celle scelte, o su quella del cursore.")
                        .font(Carattere::Testo.font(ui.ctx(), 13.0))
                        .color(tema::MATITA),
                );
                scelto
            })
            .flatten();
        if let Some(c) = scelto {
            self.comando_tabella(c);
        }
    }
}
