//! Le tabelle (piano «Word completo», W2): inserirle, scriverci, righe e colonne, eliminare,
//! unire e dividere le celle, bordi e sfondo, intestazione ripetuta, le celle scelte col dito.
//! Tutto passa dai comandi `table.*` del motore, ognuno un passo di Annulla.

use serde_json::{Value, json};
use tascabile_platform::diagnosi;
use wordcraft_engine::doc::{Path, Pos, StoryRef};
use wordcraft_engine::{Selection, Session};

use crate::editor::Editor;
use crate::ponte::StatoIme;
use crate::vista::Tocco;

/// Le righe e le colonne più grandi della griglia di «Tabella» nel foglio Inserisci.
pub const RIGHE_MASSIME: usize = 8;
pub const COLONNE_MASSIME: usize = 6;

/// La tabella dove sta il cursore, per la barra della tabella.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InfoTabella {
    pub righe: usize,
    /// Le colonne della griglia della tabella.
    pub colonne: usize,
    /// La riga e la cella del cursore (da 0).
    pub riga: usize,
    pub cella: usize,
    /// Le celle scelte (almeno quella del cursore).
    pub celle_scelte: usize,
    /// La prima riga si ripete in cima a ogni pagina.
    pub intestazione: bool,
    /// Lo sfondo della cella del cursore, RRGGBB.
    pub sfondo: Option<[u8; 3]>,
}

/// I bordi semplici della tabella intera (`table.borders`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bordi {
    Tutti,
    /// Solo il contorno.
    Contorno,
    /// Solo le linee dentro.
    Dentro,
    Nessuno,
}

impl Bordi {
    fn valore(self) -> &'static str {
        match self {
            Bordi::Tutti => "all",
            Bordi::Contorno => "outside",
            Bordi::Dentro => "inside",
            Bordi::Nessuno => "none",
        }
    }
}

/// Un comando della barra della tabella.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComandoTabella {
    RigaSopra,
    RigaSotto,
    ColonnaSinistra,
    ColonnaDestra,
    EliminaRiga,
    EliminaColonna,
    EliminaTabella,
    /// Le celle scelte diventano una.
    Unisci,
    /// La cella del cursore torna alle celle che aveva prima di un'unione, o si divide in due
    /// colonne; con più celle scelte ognuna si divide in due.
    Dividi,
    Bordi(Bordi),
    /// Lo sfondo delle celle scelte (o di quella del cursore): RRGGBB, None per toglierlo.
    Sfondo(Option<[u8; 3]>),
    /// Accende o spegne l'intestazione ripetuta.
    Intestazione,
}

/// La cella del cursore: (percorso della tabella, riga, cella), solo nel corpo.
fn cella(s: &Session) -> Option<(Path, usize, usize)> {
    (s.sel.focus.story == StoryRef::Body).then(|| s.sel.focus.path.cell()).flatten()
}

/// La tabella dove sta il cursore; None fuori da una tabella.
pub fn info_tabella(s: &Session) -> Option<InfoTabella> {
    let (tp, riga, cella) = cella(s)?;
    let t = s.doc.table(StoryRef::Body, &tp)?;
    Some(InfoTabella {
        righe: t.rows.len(),
        colonne: t.cols(),
        riga,
        cella,
        celle_scelte: wordcraft_engine::cmd::table::selected_cells(s).len().max(1),
        intestazione: t.rows.first().is_some_and(|r| r.props.header),
        sfondo: t.rows.get(riga).and_then(|r| r.cells.get(cella)).and_then(|c| c.props.shading).map(|c| [c.0, c.1, c.2]),
    })
}

/// L'inizio della cella (`riga`, `cella`) della tabella `tp`.
fn inizio_cella(tp: &Path, riga: usize, cella: usize) -> Pos {
    let mut p = tp.0.clone();
    p.extend([riga as u32, cella as u32, 0]);
    Pos { story: StoryRef::Body, path: Path(p), off: 0 }
}

/// I parametri di Dividi: con più celle scelte ognuna in due colonne; una cella unita torna alle
/// celle che copre (colonne e righe); una cella sola in due colonne.
fn parametri_dividi(s: &Session) -> Value {
    if wordcraft_engine::cmd::table::split_targets(s).len() > 1 {
        return json!({ "columns": 2, "rows": 1, "mergeFirst": false });
    }
    let unita = cella(s).and_then(|(tp, r, c)| {
        let t = s.doc.table(StoryRef::Body, &tp)?;
        let colonne = t.rows.get(r)?.cells.get(c)?.span();
        let righe = t.vertical_extent(r, c).map_or(1, |(_, k)| k);
        (colonne > 1 || righe > 1).then_some((colonne, righe))
    });
    let (colonne, righe) = unita.unwrap_or((2, 1));
    json!({ "columns": colonne, "rows": righe })
}

impl Editor {
    /// Inserisce una tabella di `righe` per `colonne` al cursore, larga quanto il testo (Adatta
    /// alla finestra di Word, `table.autofit` col modo `window`: nella vista Scrittura sta nello
    /// schermo), in un passo di Annulla solo; il cursore va nella prima cella e la tastiera si
    /// collega lì.
    pub fn inserisci_tabella(&mut self, righe: usize, colonne: usize) -> Option<StatoIme> {
        match self.s.run("insert.table", &json!({ "rows": righe, "cols": colonne })) {
            Ok(_) => {
                self.s.join_next_undo();
                if let Err(e) = self.s.run("table.autofit", &json!({ "mode": "window" })) {
                    log::warn!("table.autofit: {}", crate::ponte::genere(&e));
                }
                diagnosi(format!("inserisci: tabella di {righe} righe e {colonne} colonne"));
            }
            Err(e) => {
                log::warn!("insert.table: {}", crate::ponte::genere(&e));
                diagnosi(format!("comando: insert.table, non riuscito ({})", crate::ponte::genere(&e)));
            }
        }
        self.dopo_comando_tabella();
        self.ponte.apri(&self.s)
    }

    /// Un comando della barra della tabella.
    pub fn tabella(&mut self, c: ComandoTabella) -> Option<StatoIme> {
        let (id, params) = match c {
            ComandoTabella::RigaSopra => ("table.insertRowAbove", json!({})),
            ComandoTabella::RigaSotto => ("table.insertRowBelow", json!({})),
            ComandoTabella::ColonnaSinistra => ("table.insertColumnLeft", json!({})),
            ComandoTabella::ColonnaDestra => ("table.insertColumnRight", json!({})),
            ComandoTabella::EliminaRiga => ("table.deleteRow", json!({})),
            ComandoTabella::EliminaColonna => ("table.deleteColumn", json!({})),
            ComandoTabella::EliminaTabella => ("table.deleteTable", json!({})),
            ComandoTabella::Unisci => ("table.merge", json!({})),
            ComandoTabella::Dividi => ("table.split", parametri_dividi(&self.s)),
            ComandoTabella::Bordi(b) => ("table.borders", json!({ "kind": b.valore() })),
            ComandoTabella::Sfondo(colore) => ("table.shading", json!({ "color": colore.map(|[r, g, b]| format!("{r:02X}{g:02X}{b:02X}")) })),
            ComandoTabella::Intestazione => return self.intestazione_ripetuta(),
        };
        self.comando_con(id, params, &format!("comando: {id}"))
    }

    /// Intestazione ripetuta: accesa, la prima riga si ripete in cima a ogni pagina; spenta, non
    /// si ripete più nessuna riga. `table.repeatHeader` lavora sulla riga del cursore: il cursore
    /// va lì un momento, in un passo di Annulla solo, poi torna dov'era.
    fn intestazione_ripetuta(&mut self) -> Option<StatoIme> {
        let (tp, _, _) = cella(&self.s)?;
        let t = self.s.doc.table(StoryRef::Body, &tp)?;
        let accendi = !t.rows.first().is_some_and(|r| r.props.header);
        let righe: Vec<usize> = if accendi { vec![0] } else { (0..t.rows.len()).filter(|r| t.rows[*r].props.header).collect() };
        let prima = self.s.sel.clone();
        let mut fatto = false;
        for (i, r) in righe.into_iter().enumerate() {
            if i > 0 {
                self.s.join_next_undo();
            }
            self.s.sel = Selection::caret(inizio_cella(&tp, r, 0));
            fatto |= self.esegui_tabella("table.repeatHeader", json!({ "value": accendi }));
        }
        self.s.sel = Selection { anchor: self.s.doc.clamp(&prima.anchor), focus: self.s.doc.clamp(&prima.focus) };
        if fatto {
            diagnosi(format!("tabella: intestazione ripetuta {}", if accendi { "accesa" } else { "spenta" }));
        }
        self.dopo_comando_tabella();
        self.ponte.dopo_comando(&self.s)
    }

    fn esegui_tabella(&mut self, id: &str, params: Value) -> bool {
        match self.s.run(id, &params) {
            Ok(_) => true,
            Err(e) => {
                log::warn!("{id}: {}", crate::ponte::genere(&e));
                diagnosi(format!("comando: {id}, non riuscito ({})", crate::ponte::genere(&e)));
                false
            }
        }
    }

    fn dopo_comando_tabella(&mut self) {
        self.vista.scorri_al_cursore();
        crate::editor::svuota_richieste(&mut self.s);
        self.formato = crate::editor::Formato::di(&self.s);
    }

    /// Il dito trascinato dalla cella del cursore: la selezione va da dove era il cursore fino al
    /// punto sotto il dito (`caret.set` con `extend`), e fra due celle della tabella prende le
    /// celle intere del rettangolo fra loro, come in Word. La tastiera aspetta che il dito si alzi.
    pub fn sposta_celle(&mut self, t: Tocco) {
        let l = self.s.layout();
        let Some(pos) = l.hit(0, t.x_pt, t.y_pt, StoryRef::Body) else { return };
        if self.s.sel_ends().focus != pos {
            self.esegui_tabella("caret.set", json!({ "pos": pos, "extend": true }));
        }
        crate::editor::svuota_richieste(&mut self.s);
    }

    /// Il dito si è alzato dalle celle: la tastiera riceve la selezione nuova. Nella diagnosi solo
    /// righe e colonne scelte.
    pub fn lascia_celle(&mut self) -> Option<StatoIme> {
        match self.s.cell_range() {
            Some(c) => diagnosi(format!("celle scelte: righe {}-{}, colonne {}-{}", c.rows.0 + 1, c.rows.1 + 1, c.cols.0 + 1, c.cols.1 + 1)),
            None => diagnosi("celle scelte: nessuna cella intera"),
        }
        self.dopo_comando_tabella();
        self.ponte.dopo_comando(&self.s)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::editor::Esito;
    use crate::prova::{documento, testi};
    use crate::vista::Gesto;
    use tascabile_platform::ImeSnapshot;

    /// Il testo dei paragrafi della cella (`riga`, `cella`) della prima tabella, uniti da «|».
    fn testo_cella(s: &Session, riga: usize, cella: usize) -> String {
        let t = s.doc.body.iter().find_map(|b| b.as_table()).expect("una tabella");
        let c = &t.rows[riga].cells[cella];
        c.blocks.iter().filter_map(|b| b.as_para()).map(|p| p.text.clone()).collect::<Vec<_>>().join("|")
    }

    fn tabella(s: &Session) -> &wordcraft_engine::doc::Table {
        s.doc.body.iter().find_map(|b| b.as_table()).expect("una tabella")
    }

    /// «Prima» e «Dopo», col cursore alla fine di «Prima» e la tastiera collegata, poi una
    /// tabella di 3 righe e 4 colonne inserita lì.
    fn con_tabella() -> Editor {
        let mut e = Editor::new(documento(&["Prima", "Dopo"]), "prova");
        e.s.run("caret.set", &json!({ "pos": Pos::body(0, 5) })).unwrap();
        e.s.reset_history();
        let st = e.inserisci_tabella(3, 4).expect("la tastiera si collega alla prima cella");
        assert_eq!((st.testo.as_str(), st.sel), ("", (0, 0)));
        e
    }

    /// Il punto in mezzo alla cella (`riga`, `cella`) nella vista Scrittura larga 412 dp.
    fn in_cella(e: &mut Editor, riga: usize, cella: usize) -> Tocco {
        let l = crate::vista::layout_scrittura(&mut e.s, 392.0, 1.0);
        let r = l.pages[0]
            .items
            .iter()
            .find_map(|it| match it {
                wordcraft_engine::layout::Placed::Cell { rect, row, cell, covered: false, .. } if *row == riga && *cell == cella => Some(*rect),
                _ => None,
            })
            .expect("la cella nel layout");
        Tocco { x_pt: r.x + r.w / 2.0, y_pt: r.y + r.h / 2.0 }
    }

    /// Scrive `testo` con la tastiera finta, lettera per lettera, a partire dallo stato `st`.
    fn scrivi(e: &mut Editor, mut st: StatoIme, testo: &str) -> StatoIme {
        for c in testo.chars() {
            let mut nuovo = st.testo.clone();
            let at = tascabile_platform::ime::utf16_to_byte(&nuovo, st.sel.1 as usize);
            nuovo.insert(at, c);
            let fine = st.sel.1 + c.len_utf16() as i32;
            let snap = ImeSnapshot { text: nuovo.clone(), sel: (fine, fine), comp: (-1, -1), version: st.versione };
            st = e.ricevi(&snap).unwrap_or(StatoIme { testo: nuovo, sel: (fine, fine), versione: st.versione });
        }
        st
    }

    #[test]
    fn inserire_una_tabella_larga_quanto_il_testo_in_un_passo() {
        let mut e = con_tabella();
        assert_eq!(testi(&e.s), ["Prima", "", "Dopo"], "la tabella dopo «Prima», il paragrafo vuoto dopo lei");
        let t = tabella(&e.s);
        assert_eq!((t.rows.len(), t.cols()), (3, 4));
        assert_eq!(t.props.width_pct, Some(100.0), "Adatta alla finestra");
        let info = info_tabella(&e.s).expect("il cursore nella tabella");
        assert_eq!(info, InfoTabella { righe: 3, colonne: 4, riga: 0, cella: 0, celle_scelte: 1, intestazione: false, sfondo: None });
        // Nella vista Scrittura stretta la tabella sta nello schermo.
        let l = crate::vista::layout_scrittura(&mut e.s, 392.0, 1.0);
        let destra = l.pages[0]
            .items
            .iter()
            .filter_map(|it| match it {
                wordcraft_engine::layout::Placed::Cell { rect, .. } => Some(rect.x + rect.w),
                _ => None,
            })
            .fold(0.0f32, f32::max);
        assert!(destra <= l.pages[0].w, "la tabella esce dalla pagina: {destra} su {}", l.pages[0].w);
        // Un Annulla solo la toglie.
        e.comando("edit.undo");
        assert!(e.s.doc.body.iter().all(|b| b.as_table().is_none()), "la tabella è rimasta");
        assert_eq!(testi(&e.s), ["Prima", "Dopo"]);
        let d = tascabile_platform::diagnosi_testo();
        assert!(d.contains("] inserisci: tabella di 3 righe e 4 colonne\n"), "{d}");
    }

    #[test]
    fn il_tocco_su_una_cella_e_la_tastiera_scrivono_li() {
        let mut e = con_tabella();
        let t = in_cella(&mut e, 1, 2);
        let st = e.tocca(t).expect("la tastiera si collega alla cella");
        assert_eq!(e.s.sel.focus.path.cell().map(|(_, r, c)| (r, c)), Some((1, 2)));
        let st = scrivi(&mut e, st, "Pressione");
        assert_eq!(testo_cella(&e.s, 1, 2), "Pressione");
        // Invio nella cella: un paragrafo nuovo nella cella stessa.
        let st = scrivi(&mut e, st, "\n120");
        assert_eq!(testo_cella(&e.s, 1, 2), "Pressione|120");
        assert_eq!(st.testo, "Pressione\n120", "la finestra della tastiera è la cella");
        // Un'altra cella.
        let t = in_cella(&mut e, 0, 0);
        let st = e.tocca(t).unwrap();
        scrivi(&mut e, st, "Ora");
        assert_eq!(testo_cella(&e.s, 0, 0), "Ora");
        assert_eq!(testi(&e.s), ["Prima", "", "Dopo"], "fuori dalla tabella niente è cambiato");
        let d = tascabile_platform::diagnosi_testo();
        assert!(!d.contains("Pressione") && !d.contains("Ora"), "{d}");
    }

    #[test]
    fn righe_e_colonne_sopra_sotto_a_sinistra_a_destra_ed_eliminate() {
        let mut e = con_tabella();
        e.tabella(ComandoTabella::RigaSotto);
        assert_eq!(info_tabella(&e.s).map(|i| (i.righe, i.riga)), Some((4, 1)), "il cursore nella riga nuova");
        e.tabella(ComandoTabella::RigaSopra);
        assert_eq!(info_tabella(&e.s).map(|i| (i.righe, i.riga)), Some((5, 1)));
        e.tabella(ComandoTabella::ColonnaDestra);
        assert_eq!(info_tabella(&e.s).map(|i| (i.colonne, i.cella)), Some((5, 1)));
        e.tabella(ComandoTabella::ColonnaSinistra);
        assert_eq!(info_tabella(&e.s).map(|i| (i.colonne, i.cella)), Some((6, 1)));
        assert_eq!(tabella(&e.s).props.width_pct, Some(100.0), "sempre larga quanto il testo");
        e.tabella(ComandoTabella::EliminaRiga);
        e.tabella(ComandoTabella::EliminaColonna);
        assert_eq!(info_tabella(&e.s).map(|i| (i.righe, i.colonne)), Some((4, 5)));
        // Ognuno è un passo di Annulla.
        e.comando("edit.undo");
        assert_eq!(info_tabella(&e.s).map(|i| (i.righe, i.colonne)), Some((4, 6)));
        e.tabella(ComandoTabella::EliminaTabella);
        assert!(info_tabella(&e.s).is_none(), "il cursore è uscito dalla tabella");
        assert!(e.s.doc.body.iter().all(|b| b.as_table().is_none()));
        e.comando("edit.undo");
        assert!(e.s.doc.body.iter().any(|b| b.as_table().is_some()), "Annulla rimette la tabella");
        let d = tascabile_platform::diagnosi_testo();
        for riga in ["] comando: table.insertRowBelow\n", "] comando: table.insertColumnLeft\n", "] comando: table.deleteTable\n"] {
            assert!(d.contains(riga), "manca «{riga}»:\n{d}");
        }
    }

    #[test]
    fn trascinare_su_piu_celle_le_sceglie_poi_unisci_e_dividi() {
        let mut e = con_tabella();
        let t = in_cella(&mut e, 0, 1);
        let st = e.tocca(t).unwrap();
        scrivi(&mut e, st, "A");
        let t = in_cella(&mut e, 1, 1);
        let st = e.tocca(t).unwrap();
        scrivi(&mut e, st, "B");
        // Dal tocco nella cella (0, 1) il dito va fino alla (1, 2): quattro celle.
        let t = in_cella(&mut e, 0, 1);
        e.tocca(t);
        let v = e.ponte.versione();
        let t = in_cella(&mut e, 1, 1);
        assert_eq!(e.gesto(Gesto::Celle(t)), Esito::default());
        let t = in_cella(&mut e, 1, 2);
        e.gesto(Gesto::Celle(t));
        assert_eq!(e.ponte.versione(), v, "mentre si trascina la tastiera aspetta");
        assert_eq!(e.s.cell_range().map(|c| (c.rows, c.cols)), Some(((0, 1), (1, 2))));
        assert_eq!(info_tabella(&e.s).map(|i| i.celle_scelte), Some(4));
        assert!(e.gesto(Gesto::CelleLasciate).stato.is_some(), "lasciate le celle, la tastiera ha la selezione");
        e.tabella(ComandoTabella::Unisci);
        let t = tabella(&e.s);
        assert_eq!(t.rows[0].cells.len(), 3, "due celle della prima riga ne fanno una");
        assert_eq!(t.rows[0].cells[1].span(), 2);
        assert_eq!(testo_cella(&e.s, 0, 1), "A|B", "il testo delle celle unite resta, un paragrafo per cella");
        // Dividi rimette le quattro celle.
        e.tabella(ComandoTabella::Dividi);
        let t = tabella(&e.s);
        assert_eq!((t.rows[0].cells.len(), t.rows[1].cells.len()), (4, 4), "{:?}", t.rows.iter().map(|r| r.cells.len()).collect::<Vec<_>>());
        assert_eq!(t.cols(), 4, "la griglia è quella di prima");
        // Una cella sola si divide in due colonne.
        let t = in_cella(&mut e, 2, 0);
        e.tocca(t);
        e.tabella(ComandoTabella::Dividi);
        assert_eq!(tabella(&e.s).rows[2].cells.len(), 5);
        // Annulla una cosa alla volta, fino alle quattro celle unite.
        e.comando("edit.undo");
        e.comando("edit.undo");
        assert_eq!(tabella(&e.s).rows[0].cells.len(), 3);
        let d = tascabile_platform::diagnosi_testo();
        assert!(d.contains("] celle scelte: righe 1-2, colonne 2-3\n") && d.contains("] comando: table.merge\n"), "{d}");
    }

    #[test]
    fn bordi_sfondo_e_intestazione_ripetuta() {
        let mut e = con_tabella();
        let t = in_cella(&mut e, 2, 1);
        e.tocca(t);
        e.tabella(ComandoTabella::Bordi(Bordi::Nessuno));
        let b = tabella(&e.s).props.borders.expect("i bordi della tabella");
        assert!(b.top.is_some_and(|x| !x.is_visible()) && b.inside_v.is_some_and(|x| !x.is_visible()), "{b:?}");
        e.tabella(ComandoTabella::Bordi(Bordi::Contorno));
        let b = tabella(&e.s).props.borders.unwrap();
        assert!(b.top.is_some_and(|x| x.is_visible()) && b.between.is_some_and(|x| !x.is_visible()), "{b:?}");
        e.tabella(ComandoTabella::Sfondo(Some([0xFF, 0xF2, 0xCC])));
        let sfondo = |e: &Editor, r: usize, c: usize| tabella(&e.s).rows[r].cells[c].props.shading;
        assert_eq!(sfondo(&e, 2, 1).map(|c| (c.0, c.1, c.2)), Some((0xFF, 0xF2, 0xCC)));
        assert_eq!(sfondo(&e, 2, 0), None, "solo la cella del cursore");
        assert_eq!(info_tabella(&e.s).and_then(|i| i.sfondo), Some([0xFF, 0xF2, 0xCC]));
        e.tabella(ComandoTabella::Sfondo(None));
        assert_eq!(sfondo(&e, 2, 1), None);
        // L'intestazione ripetuta è della prima riga, anche col cursore nella terza, che resta lì.
        e.tabella(ComandoTabella::Intestazione);
        let righe = |e: &Editor| tabella(&e.s).rows.iter().map(|r| r.props.header).collect::<Vec<_>>();
        assert_eq!(righe(&e), [true, false, false]);
        assert!(info_tabella(&e.s).is_some_and(|i| i.intestazione && i.riga == 2), "{:?}", info_tabella(&e.s));
        e.tabella(ComandoTabella::Intestazione);
        assert_eq!(righe(&e), [false, false, false]);
        e.comando("edit.undo");
        assert_eq!(righe(&e), [true, false, false], "un passo di Annulla");
        e.comando("edit.undo");
        e.comando("edit.undo");
        assert_eq!(sfondo(&e, 2, 1).map(|c| (c.0, c.1, c.2)), Some((0xFF, 0xF2, 0xCC)));
        let d = tascabile_platform::diagnosi_testo();
        assert!(d.contains("] tabella: intestazione ripetuta accesa\n") && d.contains("] comando: table.shading\n"), "{d}");
        assert!(!d.contains("FFF2CC"), "{d}");
    }

    #[test]
    fn una_lettera_sulle_celle_scelte_non_toglie_le_righe() {
        let mut e = con_tabella();
        let t = in_cella(&mut e, 0, 0);
        e.tocca(t);
        let t = in_cella(&mut e, 0, 3);
        e.sposta_celle(t);
        assert_eq!(e.s.cell_range().map(|c| (c.rows, c.cols)), Some(((0, 0), (0, 3))), "la prima riga intera");
        let st = e.lascia_celle().unwrap();
        // La lettera prende il posto delle celle scelte: si svuotano, la riga resta.
        let mut nuovo = st.testo.clone();
        let at = tascabile_platform::ime::utf16_to_byte(&nuovo, st.sel.1 as usize);
        nuovo.insert(at, 'x');
        e.ricevi(&ImeSnapshot { text: nuovo, sel: (st.sel.1 + 1, st.sel.1 + 1), comp: (-1, -1), version: st.versione });
        assert_eq!(tabella(&e.s).rows.len(), 3, "la riga è rimasta");
        assert_eq!(testo_cella(&e.s, 0, 0), "x");
    }

    #[test]
    fn salvata_in_docx_e_riaperta() {
        let mut e = con_tabella();
        let t = in_cella(&mut e, 0, 0);
        let st = e.tocca(t).unwrap();
        scrivi(&mut e, st, "Letto");
        let t = in_cella(&mut e, 1, 3);
        let st = e.tocca(t).unwrap();
        scrivi(&mut e, st, "Dolore 6");
        e.tabella(ComandoTabella::RigaSotto);
        let t = in_cella(&mut e, 0, 1);
        e.tocca(t);
        let t = in_cella(&mut e, 0, 2);
        e.sposta_celle(t);
        e.lascia_celle();
        e.tabella(ComandoTabella::Unisci);
        e.tabella(ComandoTabella::Intestazione);
        let byte = wordcraft_engine::io::save_bytes("tabella.docx", &e.s.doc).expect("salvata");
        let s = crate::documento::apri_byte("tabella.docx", &byte).expect("riaperta");
        let t = tabella(&s);
        assert_eq!(t.rows.len(), 4);
        assert_eq!(t.rows[0].cells.len(), 3, "le celle unite restano unite");
        assert!(t.rows[0].props.header, "l'intestazione ripetuta resta");
        assert_eq!(t.props.width_pct, Some(100.0));
        assert_eq!(testo_cella(&s, 0, 0), "Letto");
        assert_eq!(testo_cella(&s, 1, 3), "Dolore 6");
        assert_eq!(testi(&s), ["Prima", "", "Dopo"]);
    }
}
