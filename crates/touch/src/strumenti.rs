//! La griglia del foglio «Strumenti PDF» (bozzetto 8, `.griglia` e `.strumento`): quattro colonne,
//! ogni strumento un quadrato dagli angoli tondi col colore del suo gruppo e l'icona, il nome
//! sotto su una o due righe.

use egui::{Align, Color32, CornerRadius, Rect, Sense, Ui, pos2, vec2};

use crate::pulsanti::info;
use crate::tema::{self, Carattere};

/// Le colonne della griglia.
const COLONNE: usize = 4;
/// Lo spazio fra due colonne e fra due file (`gap: 8px 6px`).
const FRA_COLONNE: f32 = 6.0;
const FRA_FILE: f32 = 8.0;
/// Il quadrato dell'icona (`.q`: 56 dp, angoli da 18), sopra e sotto 6 dp, 7 fra quadrato e nome.
const QUADRATO: f32 = 56.0;
const RAGGIO_QUADRATO: u8 = 18;
const SOPRA: f32 = 6.0;
const FRA_QUADRATO_E_NOME: f32 = 7.0;
/// Il nome: grassetto da 12,5 dp, al massimo due righe.
const NOME: f32 = 12.5;

/// Il colore di uno strumento, quello del suo gruppo: rosso tenue per Modifica, blu tenue per
/// Compila e annota, il velo per Pagine e file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tono {
    Rosso,
    Blu,
    Neutro,
}

impl Tono {
    /// Il fondo del quadrato e il colore dell'icona.
    pub fn colori(self) -> (Color32, Color32) {
        match self {
            Tono::Rosso => (tema::ROSSO_TENUE, tema::ROSSO),
            Tono::Blu => (tema::BLU_TENUE, tema::BLU),
            Tono::Neutro => (tema::VELO, tema::INCHIOSTRO),
        }
    }
}

/// Una griglia di strumenti, larga quanto `ui`.
#[derive(Debug, Clone, Default)]
pub struct GrigliaStrumenti<'a> {
    voci: Vec<(&'a str, &'a str, Tono)>,
}

impl<'a> GrigliaStrumenti<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Uno strumento: l'icona, il nome (anche per l'accessibilità) e il colore.
    pub fn voce(mut self, icona: &'a str, nome: &'a str, tono: Tono) -> Self {
        self.voci.push((icona, nome, tono));
        self
    }

    /// Disegna la griglia e dice quale strumento è stato toccato.
    pub fn mostra(self, ui: &mut Ui) -> Option<usize> {
        let ctx = ui.ctx().clone();
        let largo = ui.available_width();
        let colonna = ((largo - FRA_COLONNE * (COLONNE - 1) as f32) / COLONNE as f32).max(QUADRATO);
        let mut scelto = None;
        for (fila, voci) in self.voci.chunks(COLONNE).enumerate() {
            if fila > 0 {
                ui.add_space(FRA_FILE);
            }
            // I nomi della fila, centrati e a capo nella colonna: la fila è alta quanto il più alto.
            let nomi: Vec<_> = voci
                .iter()
                .map(|(_, nome, _)| {
                    let mut job = egui::text::LayoutJob::simple((*nome).to_owned(), Carattere::Grassetto.font(&ctx, NOME), tema::INCHIOSTRO, colonna);
                    job.halign = Align::Center;
                    job.wrap.max_rows = 2;
                    ui.painter().layout_job(job)
                })
                .collect();
            let alto_nome = nomi.iter().map(|g| g.size().y).fold(0.0, f32::max);
            let alta = SOPRA + QUADRATO + FRA_QUADRATO_E_NOME + alto_nome + SOPRA;
            let (fila_rect, _) = ui.allocate_exact_size(vec2(largo, alta), Sense::hover());
            for (c, ((icona, nome, tono), g)) in voci.iter().zip(nomi).enumerate() {
                let i = fila * COLONNE + c;
                let cella = Rect::from_min_size(pos2(fila_rect.left() + c as f32 * (colonna + FRA_COLONNE), fila_rect.top()), vec2(colonna, alta));
                // L'id dal nome: due griglie nello stesso foglio non si pestano i piedi.
                let r = ui.interact(cella, ui.id().with(("strumento", *nome)), Sense::click());
                info(&r, true, None, nome);
                let quadrato = Rect::from_min_size(pos2(cella.center().x - QUADRATO / 2.0, cella.top() + SOPRA), vec2(QUADRATO, QUADRATO));
                let (fondo, colore) = tono.colori();
                let raggio = CornerRadius::same(RAGGIO_QUADRATO);
                ui.painter().rect_filled(quadrato, raggio, fondo);
                if r.is_pointer_button_down_on() {
                    ui.painter().rect_filled(quadrato, raggio, Color32::from_black_alpha(20));
                }
                let g_icona = ui.painter().layout_no_wrap((*icona).to_owned(), Carattere::Icona.font(&ctx, tema::ICONA), colore);
                ui.painter().galley(quadrato.center() - g_icona.size() / 2.0, g_icona, Color32::PLACEHOLDER);
                // Con l'allineamento al centro il nome si disegna dal suo centro.
                ui.painter().galley(pos2(cella.center().x, quadrato.bottom() + FRA_QUADRATO_E_NOME), g, Color32::PLACEHOLDER);
                if r.clicked() {
                    scelto = Some(i);
                }
            }
        }
        scelto
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::icone;
    use crate::prova::{banco, rettangolo_del_testo, riempiti};
    use egui_kittest::kittest::Queryable;

    fn griglia() -> GrigliaStrumenti<'static> {
        GrigliaStrumenti::new()
            .voce(icone::EDIT_NOTE, "Modifica testo", Tono::Rosso)
            .voce(icone::SIGNATURE, "Firma", Tono::Blu)
            .voce(icone::VIEW_MODULE, "Organizza pagine", Tono::Neutro)
            .voce(icone::ROTATE_RIGHT, "Ruota pagina", Tono::Neutro)
            .voce(icone::TOC, "Indice", Tono::Neutro)
    }

    #[test]
    fn quattro_colonne_coi_quadrati_colorati_e_il_nome_sotto() {
        let mut h = banco(
            |ui, scelti: &mut Vec<usize>| {
                ui.set_width(376.0);
                scelti.extend(griglia().mostra(ui));
            },
            Vec::new(),
        );
        // Quadrati da 56 dp coi colori dei gruppi: uno rosso, uno blu, tre col velo.
        let rossi = riempiti(&h, tema::ROSSO_TENUE);
        let blu = riempiti(&h, tema::BLU_TENUE);
        let neutri = riempiti(&h, tema::VELO);
        assert_eq!((rossi.len(), blu.len(), neutri.len()), (1, 1, 3), "{rossi:?} {blu:?} {neutri:?}");
        assert!(rossi.iter().chain(&blu).chain(&neutri).all(|q| q.size() == vec2(QUADRATO, QUADRATO)));
        // La quinta va a capo, nella prima colonna; le colonne sono larghe uguali.
        let celle = ["Modifica testo", "Firma", "Organizza pagine", "Ruota pagina", "Indice"].map(|n| h.get_by_label(n).rect());
        assert!((celle[4].left() - celle[0].left()).abs() < 0.5 && celle[4].top() > celle[0].bottom());
        assert!((celle[1].left() - celle[0].right() - FRA_COLONNE).abs() < 0.5);
        assert!(celle.iter().all(|c| (c.width() - celle[0].width()).abs() < 0.5));
        // Il nome sotto il quadrato, centrato, dentro la sua colonna anche su due righe.
        for (n, cella) in ["Organizza pagine", "Firma"].iter().zip([celle[2], celle[1]]) {
            let nome = rettangolo_del_testo(&h, n).unwrap();
            assert!(cella.contains_rect(nome), "{n}: {nome:?} fuori da {cella:?}");
            assert!((nome.center().x - cella.center().x).abs() < 2.0, "{n}: {nome:?} {cella:?}");
            assert!(nome.top() >= cella.top() + SOPRA + QUADRATO, "{n}: {nome:?}");
        }
        h.get_by_label("Ruota pagina").click();
        h.run_steps(2);
        assert_eq!(h.state(), &vec![3]);
    }

    #[test]
    fn due_griglie_nello_stesso_foglio_hanno_tutte_le_voci() {
        let h = banco(
            |ui, _: &mut ()| {
                ui.set_width(376.0);
                GrigliaStrumenti::new().voce(icone::ROTATE_RIGHT, "Ruota pagina", Tono::Neutro).mostra(ui);
                GrigliaStrumenti::new().voce(icone::GRID_VIEW, "Miniature", Tono::Neutro).mostra(ui);
            },
            (),
        );
        h.get_by_label("Ruota pagina");
        h.get_by_label("Miniature");
    }
}
