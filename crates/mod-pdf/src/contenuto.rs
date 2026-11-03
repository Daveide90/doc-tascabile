//! Il modo Modifica (piano «PDF completo», P7): i paragrafi, le immagini e quello che Tascabile ha
//! aggiunto su una pagina, come li vede chi tocca, e le modifiche che li cambiano: riscrivere un
//! paragrafo, un testo nuovo, spostare, ingrandire, sostituire e togliere, la filigrana, i numeri
//! di pagina con l'intestazione, l'oscuramento vero.
//!
//! L'inventario è quello del motore (`editable_objects`): paragrafi (`text_blocks`), immagini
//! (`page_images`) e aggiunte (`added`), coi numeri che vogliono le modifiche. Si rilegge dopo ogni
//! modifica della pagina: i numeri dei paragrafi cambiano quando la geometria si raggruppa in un
//! altro modo (ricerca sulla modifica, sezione 1.6).
//!
//! Riscrivere un paragrafo è fragile (`docs/gaps.md` del motore): il motore riusa il carattere
//! incorporato se ha le lettere nuove, altrimenti la riga passa a un carattere standard. Nessuna
//! funzione lo dice prima: il motore del thread prova la modifica, guarda i caratteri delle righe
//! e, se sono cambiati, la annulla e lo dice (`Evento::CambiaCarattere`); chi la chiede la
//! rimanda col sì ([`Modifica::anche_se_cambia_carattere`]).
//!
//! Il testo di un paragrafo e quello aggiunto servono al motore e allo schermo: mai nella diagnosi
//! (`Debug` di [`Oggetto`] non li dice).

use std::sync::Arc;

use pdfcraft_engine::{
    AddedContent, AddedText, BlockStyle, Document, Edit, HeaderFooter, ImageEdit, NewAnnotation, ObjectKind, ObjectTarget, OverlayLook, Shape, Style,
    Watermark, added_text_rect, added_text_width, first_undrawable,
};
use pdfcraft_render::PageInfo;

use crate::apertura::Pdf;
use crate::modifica::{GenereModifica, Modifica, Toccate};
use crate::moduli::in_vista;

/// Che oggetto della pagina è, per chi modifica.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GenereOggetto {
    /// Un paragrafo del PDF: si riscrive, si sposta, si allarga (il testo va a capo nella
    /// larghezza nuova).
    Testo,
    /// Un'immagine del PDF; `gruppo` per un disegno vettoriale raggruppato, che si sposta e si
    /// toglie ma non si sostituisce con una foto.
    Immagine { gruppo: bool },
    /// Un testo aggiunto: la misura dei caratteri in pt e il colore.
    TestoAggiunto { misura: f32, colore: [f32; 3] },
    /// Un'immagine aggiunta.
    ImmagineAggiunta,
    /// Il testo riconosciuto di una scansione (lo strato invisibile dell'OCR): si corregge
    /// soltanto, e la correzione lo riscrive visibile togliendo la scritta di prima anche dalla
    /// foto (`correzione`). Non si sposta e non si allarga.
    Scansione,
}

impl GenereOggetto {
    /// È un testo (si riscrive con la tastiera).
    pub fn testo(self) -> bool {
        matches!(self, GenereOggetto::Testo | GenereOggetto::TestoAggiunto { .. } | GenereOggetto::Scansione)
    }

    /// Si sposta col dito e si ingrandisce con la maniglia (il testo di una scansione no).
    pub fn si_muove(self) -> bool {
        self != GenereOggetto::Scansione
    }

    /// È un'immagine che si può sostituire con un'altra.
    pub fn sostituibile(self) -> bool {
        matches!(self, GenereOggetto::Immagine { gruppo: false } | GenereOggetto::ImmagineAggiunta)
    }

    /// Si può togliere (un paragrafo del PDF no: si riscrive).
    pub fn si_toglie(self) -> bool {
        !matches!(self, GenereOggetto::Testo | GenereOggetto::Scansione)
    }

    /// Per la diagnosi.
    pub fn nome(self) -> &'static str {
        match self {
            GenereOggetto::Testo => "paragrafo",
            GenereOggetto::Immagine { gruppo: false } => "immagine",
            GenereOggetto::Immagine { gruppo: true } => "disegno",
            GenereOggetto::TestoAggiunto { .. } => "testo aggiunto",
            GenereOggetto::ImmagineAggiunta => "immagine aggiunta",
            GenereOggetto::Scansione => "testo della scansione",
        }
    }
}

/// Un oggetto di una pagina che si modifica.
#[derive(Clone, PartialEq)]
pub struct Oggetto {
    pub pagina: usize,
    pub(crate) bersaglio: ObjectTarget,
    pub genere: GenereOggetto,
    /// La parte che si vede, in pt nella pagina come si vede.
    pub rettangolo: [f32; 4],
    /// Il testo da riscrivere: per un paragrafo le righe unite da spazi, per un testo aggiunto il
    /// suo testo. Mai nella diagnosi.
    pub testo: String,
    /// Un paragrafo: il testo con gli a capo fra le righe, come lo vuole il motore.
    pub righe: String,
    /// Un'immagine del PDF: il suo riquadro intero nello spazio utente (può uscire dalla parte
    /// che si vede).
    pub(crate) intero: [f64; 4],
    /// Un'aggiunta: com'è per il motore.
    pub(crate) aggiunto: Option<AddedContent>,
    /// Un'immagine del PDF disegnata di traverso (girata di un quarto di giro): la sua larghezza
    /// va lungo l'altezza dello spazio utente.
    pub(crate) di_traverso: bool,
}

/// Senza il testo.
impl std::fmt::Debug for Oggetto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Oggetto").field("pagina", &self.pagina).field("genere", &self.genere).field("indice", &self.bersaglio.index).finish()
    }
}

impl Oggetto {
    /// Chi è nella pagina, finché la pagina non cambia: il genere dell'inventario e il numero.
    pub fn chiave(&self) -> (usize, u8, usize) {
        let k = match self.bersaglio.kind {
            ObjectKind::Added => 0,
            ObjectKind::Text => 1,
            ObjectKind::Image => 2,
        };
        (self.pagina, k, self.bersaglio.index)
    }

    /// Si ingrandisce con le sue proporzioni (le immagini e i testi aggiunti) o si allarga
    /// soltanto (un paragrafo, che va a capo nella larghezza nuova).
    pub fn con_proporzioni(&self) -> bool {
        self.genere != GenereOggetto::Testo
    }
}

/// Gli oggetti della pagina `pagina` (da 0) che si modificano, nell'ordine dell'inventario del
/// motore: prima le aggiunte, poi i paragrafi, poi le immagini. Vuoto se la pagina non si legge o
/// ha troppi oggetti. I paragrafi invisibili (lo strato dell'OCR) sono il testo di una scansione,
/// da correggere; quelli girati no.
pub(crate) fn oggetti_da(d: &Document, pagina: usize) -> Vec<Oggetto> {
    let Some(info) = d.info.pages.get(pagina) else { return Vec::new() };
    let Ok(inventario) = d.editable_objects(pagina) else { return Vec::new() };
    let blocchi = d.text_blocks(pagina);
    let immagini = d.page_images(pagina);
    let aggiunti: Vec<&AddedContent> = d.added().iter().filter(|a| a.page == pagina).map(|a| &a.content).collect();
    inventario
        .into_iter()
        .filter_map(|o| {
            let rettangolo = in_vista(info, o.rect)?;
            let i = o.target.index;
            let (genere, righe, intero, aggiunto) = match o.target.kind {
                ObjectKind::Text => {
                    let b = blocchi.get(i).filter(|b| b.rotation.abs() < 0.5)?;
                    let genere = if b.invisible { GenereOggetto::Scansione } else { GenereOggetto::Testo };
                    (genere, b.editor_text.clone(), o.rect, None)
                }
                ObjectKind::Image => (GenereOggetto::Immagine { gruppo: immagini.get(i)?.is_form }, String::new(), immagini.get(i)?.rect, None),
                ObjectKind::Added => {
                    let c = (*aggiunti.get(i)?).clone();
                    let genere = match &c {
                        AddedContent::Text(t) => GenereOggetto::TestoAggiunto { misura: t.size as f32, colore: t.color.map(|v| v as f32) },
                        AddedContent::Image(_) => GenereOggetto::ImmagineAggiunta,
                    };
                    let righe = match &c {
                        AddedContent::Text(t) => t.text.clone(),
                        AddedContent::Image(_) => String::new(),
                    };
                    (genere, righe, o.rect, Some(c))
                }
            };
            let testo = righe.replace('\n', " ");
            // Il lato della larghezza dell'immagine (la prima colonna della matrice) è più in su che
            // di lato.
            let di_traverso = o.target.kind == ObjectKind::Image && immagini.get(i).is_some_and(|m| m.matrix[0].abs() < m.matrix[1].abs());
            Some(Oggetto { pagina, bersaglio: o.target, genere, rettangolo, testo, righe, intero, aggiunto, di_traverso })
        })
        .collect()
}

impl Pdf {
    /// Gli oggetti della pagina `pagina` che si modificano.
    pub fn oggetti(&self, pagina: usize) -> Vec<Oggetto> {
        oggetti_da(self.documento(), pagina)
    }
}

/// I caratteri (il `/BaseFont`) delle righe del paragrafo `blocco` di `pagina`, e quante righe
/// della pagina ne usano altri: dopo una riscrittura che cambia carattere, le altre crescono.
pub(crate) fn caratteri_del_paragrafo(d: &Document, pagina: usize, blocco: usize) -> (Vec<String>, usize) {
    let righe = d.text_lines(pagina);
    let mut suoi: Vec<String> = d
        .text_blocks(pagina)
        .get(blocco)
        .map(|b| b.lines.iter().filter_map(|i| righe.get(*i)).map(|l| l.base_font.clone()).collect())
        .unwrap_or_default();
    suoi.sort();
    suoi.dedup();
    let altre = righe.iter().filter(|l| !suoi.contains(&l.base_font)).count();
    (suoi, altre)
}

/// Le righe della pagina con un carattere che non è fra `suoi`.
pub(crate) fn righe_con_altri_caratteri(d: &Document, pagina: usize, suoi: &[String]) -> usize {
    d.text_lines(pagina).iter().filter(|l| !suoi.contains(&l.base_font)).count()
}

/// Il testo nuovo di un paragrafo con gli a capo di prima fuori dal pezzo cambiato: chi scrive lo
/// vede su una riga sola (`righe` con gli spazi al posto degli a capo, gli stessi byte), e così le
/// righe che non tocca restano com'erano.
pub fn rimetti_gli_a_capo(righe: &str, nuovo: &str) -> String {
    let unito = righe.replace('\n', " ");
    let mut prima = unito.bytes().zip(nuovo.bytes()).take_while(|(a, b)| a == b).count();
    while !(unito.is_char_boundary(prima) && nuovo.is_char_boundary(prima)) {
        prima -= 1;
    }
    let massimo = unito.len().min(nuovo.len()) - prima;
    let mut dopo = unito.bytes().rev().zip(nuovo.bytes().rev()).take(massimo).take_while(|(a, b)| a == b).count();
    while !(unito.is_char_boundary(unito.len() - dopo) && nuovo.is_char_boundary(nuovo.len() - dopo)) {
        dopo -= 1;
    }
    format!("{}{}{}", &righe[..prima], &nuovo[prima..nuovo.len() - dopo], &righe[righe.len() - dopo..])
}

/// Il primo carattere di `testo` che un testo nuovo (in un carattere standard) non sa scrivere:
/// l'italiano e «€» sì, «≥» o «₂» no.
pub fn carattere_che_manca(testo: &str) -> Option<char> {
    first_undrawable(&AddedText { text: testo.to_string(), ..AddedText::default() })
}

/// Il nome che il motore vuole per i byte di un'immagine (ne guarda l'estensione): mai quello del
/// file di chi la sceglie.
fn nome_immagine(byte: &[u8]) -> &'static str {
    if byte.starts_with(b"\x89PNG") { "immagine.png" } else { "immagine.jpg" }
}

/// Un rettangolo in pt della pagina come si vede (dall'alto) nello spazio delle aggiunte (dal
/// basso a sinistra della pagina come si vede).
fn in_aggiunta(info: &PageInfo, r: [f32; 4]) -> [f64; 4] {
    let h = f64::from(info.height);
    [f64::from(r[0]), h - f64::from(r[3]), f64::from(r[2]), h - f64::from(r[1])]
}

/// Fra un testo aggiunto e il bordo destro della pagina, in pt.
const MARGINE_DESTRO: f32 = 12.0;
/// Un testo aggiunto vicino al bordo è largo almeno così, in pt.
const LARGO_MINIMO: f32 = 36.0;

/// Un testo aggiunto largo quanto il suo testo ma non oltre il bordo destro della pagina (meno un
/// margine), dove il motore lo manda a capo, con l'angolo in alto a sinistra in `a` (pt della
/// pagina come si vede) e l'altezza delle sue righe dopo l'a capo. Senza limite una frase lunga
/// finiva fuori dalla pagina (analisi del 27/10, modifica I4).
fn testo_largo_quanto_serve(info: &PageInfo, a: [f32; 2], mut t: AddedText) -> AddedText {
    t.rect = [0.0, 0.0, 1.0, 1.0];
    let serve = (added_text_width(&t) + 2.0).max(4.0) as f32;
    let largo = serve.min((info.width - a[0] - MARGINE_DESTRO).max(LARGO_MINIMO));
    t.rect = in_aggiunta(info, [a[0], a[1], a[0] + largo, a[1] + 1.0]);
    // Il motore misura l'altezza dalle righe andate a capo, tenendo fermo il lato in alto.
    t.rect = added_text_rect(&t);
    t
}

/// Il riquadro più grande largo `proporzione` volte l'altezza dentro `r` ([x0, y0, x1, y1]), nel
/// suo mezzo.
fn con_le_proporzioni(r: [f64; 4], proporzione: f64) -> [f64; 4] {
    let (w, h) = ((r[2] - r[0]).abs(), (r[3] - r[1]).abs());
    let (nw, nh) = if w > h * proporzione { (h * proporzione, h) } else { (w, w / proporzione) };
    let (cx, cy) = ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0);
    [cx - nw / 2.0, cy - nh / 2.0, cx + nw / 2.0, cy + nh / 2.0]
}

/// Una modifica del contenuto di una pagina.
fn della_pagina(pagina: usize, edit: Edit) -> Modifica {
    Modifica { edit, genere: GenereModifica::Pagine, toccate: Toccate::alcune([pagina]), carattere: None, pulizia: None }
}

/// La misura più piccola e più grande dei caratteri di un testo aggiunto.
const MISURA_MINIMA: f64 = 4.0;
const MISURA_MASSIMA: f64 = 300.0;

impl Modifica {
    /// La stessa modifica, da fare anche se il paragrafo cambia carattere: chi l'ha chiesta ha
    /// detto di continuare.
    pub fn anche_se_cambia_carattere(mut self) -> Modifica {
        self.carattere = None;
        self
    }

    /// Il testo nuovo di un paragrafo o di un testo aggiunto (scritto su una riga sola). Un
    /// paragrafo riscritto si controlla: se cambia carattere, il motore del thread lo dice.
    pub fn testo_dell_oggetto(o: &Oggetto, info: &PageInfo, testo: &str) -> Option<Modifica> {
        match (&o.genere, &o.aggiunto) {
            (GenereOggetto::Testo, _) => {
                let edit = Edit::EditTextBlock {
                    page: o.pagina,
                    block: o.bersaglio.index,
                    text: rimetti_gli_a_capo(&o.righe, testo),
                    style: BlockStyle::default(),
                };
                Some(Modifica { carattere: Some((o.pagina, o.bersaglio.index)), ..della_pagina(o.pagina, edit) })
            }
            (GenereOggetto::TestoAggiunto { .. }, Some(AddedContent::Text(t))) => {
                let nuovo = AddedText { text: testo.to_string(), ..t.clone() };
                let nuovo = testo_largo_quanto_serve(info, [o.rettangolo[0], o.rettangolo[1]], nuovo);
                Some(della_pagina(o.pagina, Edit::UpdateContent { page: o.pagina, index: o.bersaglio.index, content: AddedContent::Text(nuovo) }))
            }
            _ => None,
        }
    }

    /// Il testo nuovo di un paragrafo, largo `quanto` volte quello di prima: la presa della
    /// scrittura (richiesta dell'autore, 3/11: «poter aumentare allargando il box delle parole che correggo per
    /// evitare che vada a capo»). Il testo senza gli a capo di prima, che si rifà nella larghezza
    /// nuova; testo e larghezza in una modifica sola, che Annulla toglie insieme.
    pub fn testo_largo(o: &Oggetto, info: &PageInfo, testo: &str, quanto: f32) -> Option<Modifica> {
        if o.genere != GenereOggetto::Testo || !quanto.is_finite() || quanto <= 0.0 {
            return None;
        }
        let [x0, y0, x1, y1] = o.rettangolo;
        // La larghezza lungo la riga di base, come in `misura_oggetto`.
        let riga = if info.rotation % 180 == 90 { y1 - y0 } else { x1 - x0 };
        let style = BlockStyle { width: Some(f64::from(riga * quanto)), ..BlockStyle::default() };
        let edit = Edit::EditTextBlock { page: o.pagina, block: o.bersaglio.index, text: testo.to_string(), style };
        Some(Modifica { carattere: Some((o.pagina, o.bersaglio.index)), ..della_pagina(o.pagina, edit) })
    }

    /// Un testo nuovo sulla pagina `pagina`, con l'angolo in alto a sinistra nel punto `a` (pt
    /// della pagina come si vede), `misura` in pt e `colore` RGB da 0 a 1, in Helvetica.
    pub fn testo_nuovo(pagina: usize, info: &PageInfo, a: [f32; 2], testo: &str, misura: f64, colore: [f32; 3]) -> Option<Modifica> {
        if !(a[0].is_finite() && a[1].is_finite()) || testo.trim().is_empty() {
            return None;
        }
        let t = AddedText {
            text: testo.to_string(),
            size: misura.clamp(MISURA_MINIMA, MISURA_MASSIMA),
            color: colore.map(f64::from),
            ..AddedText::default()
        };
        let text = testo_largo_quanto_serve(info, a, t);
        Some(della_pagina(pagina, Edit::AddText { page: pagina, text }))
    }

    /// Un testo aggiunto con la misura dei caratteri `misura` (pt) e il colore `colore`, largo
    /// quanto il testo, dallo stesso angolo in alto a sinistra.
    pub fn stile_del_testo(o: &Oggetto, info: &PageInfo, misura: f64, colore: [f32; 3]) -> Option<Modifica> {
        let Some(AddedContent::Text(t)) = &o.aggiunto else { return None };
        let nuovo = AddedText { size: misura.clamp(MISURA_MINIMA, MISURA_MASSIMA), color: colore.map(f64::from), ..t.clone() };
        let nuovo = testo_largo_quanto_serve(info, [o.rettangolo[0], o.rettangolo[1]], nuovo);
        Some(della_pagina(o.pagina, Edit::UpdateContent { page: o.pagina, index: o.bersaglio.index, content: AddedContent::Text(nuovo) }))
    }

    /// Toglie la filigrana (`numeri` falso) o l'intestazione e il piè di pagina da tutte le
    /// pagine.
    pub fn togli_marche(numeri: bool) -> Modifica {
        let kind = if numeri { pdfcraft_engine::MarkKind::HeaderFooter } else { pdfcraft_engine::MarkKind::Watermark };
        Modifica { edit: Edit::RemoveMarks { kind }, genere: GenereModifica::Pagine, toccate: Toccate::Tutte, carattere: None, pulizia: None }
    }

    /// Sposta l'oggetto di `dx`, `dy` pt della pagina come si vede.
    pub fn sposta_oggetto(o: &Oggetto, info: &PageInfo, dx: f32, dy: f32) -> Modifica {
        let (a, b) = (info.view_to_user(0.0, 0.0), info.view_to_user(dx, dy));
        let offset = [f64::from(b[0] - a[0]), f64::from(b[1] - a[1])];
        della_pagina(o.pagina, Edit::MoveObjects { page: o.pagina, objects: vec![o.bersaglio], offset })
    }

    /// L'oggetto nel riquadro `r` (pt della pagina come si vede, dall'angolo in alto a sinistra
    /// dell'oggetto): un'immagine si ingrandisce, un testo aggiunto cambia la misura dei caratteri
    /// in proporzione all'altezza, un paragrafo va a capo nella larghezza nuova.
    pub fn misura_oggetto(o: &Oggetto, info: &PageInfo, r: [f32; 4]) -> Option<Modifica> {
        if !r.iter().all(|v| v.is_finite()) || r[2] - r[0] < 2.0 || r[3] - r[1] < 2.0 {
            return None;
        }
        let [x0, y0, x1, y1] = o.rettangolo;
        let edit = match (&o.genere, &o.aggiunto) {
            (GenereOggetto::Testo, _) => {
                // La maniglia dice di quanto allargare; il motore vuole la larghezza lungo la riga di
                // base, che su una pagina girata di 90 o 270 gradi è l'altezza che si vede. Con la
                // larghezza che si vede il paragrafo diventava una parola per riga (analisi del
                // 27/10, modifica I1).
                let quanto = (r[2] - r[0]) / (x1 - x0).max(1e-3);
                let riga = if info.rotation % 180 == 90 { y1 - y0 } else { x1 - x0 };
                let style = BlockStyle { width: Some(f64::from(riga * quanto)), ..BlockStyle::default() };
                // Il testo con le righe unite da spazi: il motore tiene ogni a capo come un a capo
                // vero, e con le righe il paragrafo non si rifaceva (analisi del 27/10, modifica I2).
                Edit::EditTextBlock { page: o.pagina, block: o.bersaglio.index, text: o.testo.clone(), style }
            }
            (GenereOggetto::Immagine { .. }, _) => {
                // Il riquadro intero cresce come la parte che si vede, dallo stesso angolo.
                let k = (r[2] - r[0]) / (x1 - x0).max(1e-3);
                let v = in_vista(info, o.intero)?;
                let nuovo = [r[0] + (v[0] - x0) * k, r[1] + (v[1] - y0) * k, r[0] + (v[2] - x0) * k, r[1] + (v[3] - y0) * k];
                Edit::EditPageImage {
                    page: o.pagina,
                    index: o.bersaglio.index,
                    change: ImageEdit::Move(info.view_rect_to_user(nuovo).map(f64::from)),
                }
            }
            (GenereOggetto::TestoAggiunto { misura, .. }, Some(AddedContent::Text(t))) => {
                let k = f64::from((r[3] - r[1]) / (y1 - y0).max(1e-3));
                let size = ((f64::from(*misura) * k).clamp(MISURA_MINIMA, MISURA_MASSIMA) * 2.0).round() / 2.0;
                let nuovo = testo_largo_quanto_serve(info, [r[0], r[1]], AddedText { size, ..t.clone() });
                Edit::UpdateContent { page: o.pagina, index: o.bersaglio.index, content: AddedContent::Text(nuovo) }
            }
            (GenereOggetto::ImmagineAggiunta, Some(AddedContent::Image(i))) => {
                let nuova = pdfcraft_engine::AddedImage { rect: in_aggiunta(info, r), ..i.clone() };
                Edit::UpdateContent { page: o.pagina, index: o.bersaglio.index, content: AddedContent::Image(nuova) }
            }
            _ => return None,
        };
        Some(della_pagina(o.pagina, edit))
    }

    /// Toglie l'oggetto (un paragrafo del PDF no). Un'immagine del PDF tolta esce dal file al
    /// salvataggio, che lo riscrive tutto (vendor/pdfcraft-edit; un PDF firmato si salva in coda e
    /// la tiene nella revisione di prima).
    pub fn togli_oggetto(o: &Oggetto) -> Option<Modifica> {
        let edit = match o.genere {
            GenereOggetto::Testo | GenereOggetto::Scansione => return None,
            GenereOggetto::Immagine { .. } => Edit::EditPageImage { page: o.pagina, index: o.bersaglio.index, change: ImageEdit::Delete },
            GenereOggetto::TestoAggiunto { .. } | GenereOggetto::ImmagineAggiunta => Edit::DeleteContent { page: o.pagina, index: o.bersaglio.index },
        };
        Some(della_pagina(o.pagina, edit))
    }

    /// Un'altra immagine (PNG o JPEG, larga `proporzione` volte l'altezza) al posto di questa: la
    /// più grande che entra nel riquadro di prima, nel mezzo, con le sue proporzioni. Un passo
    /// solo da annullare. Prima prendeva il riquadro vecchio e usciva stirata (analisi del 27/10,
    /// modifica I3).
    pub fn sostituisci_immagine(o: &Oggetto, byte: Arc<Vec<u8>>, proporzione: f32) -> Option<Modifica> {
        if !(proporzione.is_finite() && proporzione > 0.0) {
            return None;
        }
        let (page, index, name) = (o.pagina, o.bersaglio.index, nome_immagine(&byte).to_string());
        let k = f64::from(proporzione);
        // Le proporzioni si contano lungo i lati dell'immagine: girata di un quarto di giro, la sua
        // larghezza è l'altezza del riquadro.
        let lungo_i_lati = |girata: bool| if girata { 1.0 / k } else { k };
        let edits = match (&o.genere, &o.aggiunto) {
            (GenereOggetto::Immagine { gruppo: false }, _) => vec![
                Edit::EditPageImage { page, index, change: ImageEdit::Move(con_le_proporzioni(o.intero, lungo_i_lati(o.di_traverso))) },
                Edit::EditPageImage { page, index, change: ImageEdit::Replace { name, bytes: byte } },
            ],
            (GenereOggetto::ImmagineAggiunta, Some(AddedContent::Image(i))) => {
                let rect = con_le_proporzioni(i.rect, lungo_i_lati(i.rotation % 2 == 1));
                // Prima il riquadro: la sostituzione tiene quello che trova.
                vec![
                    Edit::UpdateContent { page, index, content: AddedContent::Image(pdfcraft_engine::AddedImage { rect, ..i.clone() }) },
                    Edit::ReplaceImage { page, index, name, bytes: byte },
                ]
            }
            _ => return None,
        };
        Some(della_pagina(page, Edit::Batch { label: "Sostituisci".into(), edits }))
    }

    /// Un'immagine nuova (PNG o JPEG, larga `proporzione` volte l'altezza) centrata sul punto `a`
    /// (pt della pagina come si vede): larga metà della pagina, alta al massimo metà, dentro la
    /// pagina.
    pub fn immagine_nuova(pagina: usize, info: &PageInfo, a: [f32; 2], byte: Arc<Vec<u8>>, proporzione: f32) -> Option<Modifica> {
        if !(a[0].is_finite() && a[1].is_finite() && proporzione.is_finite() && proporzione > 0.0) {
            return None;
        }
        let (pw, ph) = (info.width, info.height);
        let mut w = pw * 0.5;
        let mut h = w / proporzione;
        if h > ph * 0.5 {
            h = ph * 0.5;
            w = h * proporzione;
        }
        let x = (a[0] - w / 2.0).clamp(0.0, (pw - w).max(0.0));
        let y = (a[1] - h / 2.0).clamp(0.0, (ph - h).max(0.0));
        let rect = Some(in_aggiunta(info, [x, y, x + w, y + h]));
        Some(della_pagina(pagina, Edit::AddImage { page: pagina, rect, name: nome_immagine(&byte).to_string(), bytes: byte }))
    }

    /// La filigrana `testo` su tutte le `pagine`, di traverso, con l'opacità da 0 a 1; prende il
    /// posto di quella di prima.
    pub fn filigrana(pagine: usize, testo: &str, opacita: f64) -> Modifica {
        let settings = Watermark { text: testo.to_string(), opacity: opacita.clamp(0.05, 1.0), ..Watermark::default() };
        let edit = Edit::AddWatermark { pages: (0..pagine).collect(), settings, replace: true, file: None };
        Modifica { edit, genere: GenereModifica::Pagine, toccate: Toccate::Tutte, carattere: None, pulizia: None }
    }

    /// L'intestazione (al centro in alto) e il piè di pagina (al centro in basso) su tutte le
    /// `pagine`, a 9 pt; dentro `<<1>>` è il numero della pagina e `<<n>>` quante sono, così
    /// «Pagina <<1>> di <<n>>» scrive «Pagina 1 di 5». Prendono il posto di quelli di prima.
    pub fn numeri_di_pagina(pagine: usize, intestazione: &str, pie: &str) -> Modifica {
        let text = [String::new(), intestazione.to_string(), String::new(), String::new(), pie.to_string(), String::new()];
        let settings = HeaderFooter { text, font_size: 9.0, ..HeaderFooter::default() };
        let edit = Edit::AddHeaderFooter { pages: (0..pagine).collect(), settings, replace: true };
        Modifica { edit, genere: GenereModifica::Pagine, toccate: Toccate::Tutte, carattere: None, pulizia: None }
    }

    /// Oscura per sempre quello che sta sotto i `rettangoli` (pt della pagina come si vede) della
    /// pagina `pagina`: i segni dell'oscuramento e la loro applicazione in un passo solo. Il
    /// motore toglie i glifi, le immagini e i disegni coperti, controlla che non resti niente e
    /// fa riscrivere tutto il file al salvataggio dopo (le revisioni vecchie spariscono).
    pub fn oscura(pagina: usize, info: &PageInfo, rettangoli: &[[f32; 4]]) -> Option<Modifica> {
        let quads: Vec<[f64; 8]> = rettangoli
            .iter()
            .filter(|r| r.iter().all(|v| v.is_finite()) && r[2] - r[0] >= 1.0 && r[3] - r[1] >= 1.0)
            .map(|r| info.view_rect_to_quad(*r))
            .collect();
        if quads.is_empty() {
            return None;
        }
        let shape = Shape::Redact { quads, overlay: String::new(), look: OverlayLook::default() };
        let style = Style::default_for(&shape);
        let segni = Edit::AddAnnotation(NewAnnotation { page: pagina, shape, style, contents: String::new(), author: String::new() });
        let edit = Edit::Batch { label: "Oscura".into(), edits: vec![segni, Edit::ApplyRedactions { pages: Some(vec![pagina]) }] };
        Some(della_pagina(pagina, edit))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::motore::{Evento, Fatto, Motore, Scrittura};
    use crate::prova::{Pagina, foto_di_una_firma, pdf};
    use std::sync::Mutex;
    use std::time::Duration;

    type Esito = Result<usize, String>;
    const ATTESA: Duration = Duration::from_secs(30);

    fn motore(byte: Vec<u8>) -> Motore<Esito> {
        Motore::avvia(Pdf::apri(byte, None).unwrap(), Arc::new(|| {})).unwrap()
    }

    fn due_pagine() -> Vec<u8> {
        let prima = "BT /F1 24 Tf 60 700 Td (Pagina di prova) Tj ET\nBT /F1 12 Tf 60 500 Td (Codice segreto 98765) Tj ET";
        pdf(&[
            Pagina { larghezza: 595.0, altezza: 842.0, rotazione: 0, contenuto: prima },
            Pagina { larghezza: 595.0, altezza: 842.0, rotazione: 0, contenuto: "BT /F1 12 Tf 60 700 Td (Seconda) Tj ET" },
        ])
    }

    fn oggetti(m: &mut Motore<Esito>, pagina: usize) -> Vec<Oggetto> {
        assert!(m.oggetti(pagina));
        let fatti = m.aspetta(ATTESA);
        match fatti.into_iter().last() {
            Some(Fatto { evento: Evento::Oggetti { pagina: p, oggetti }, .. }) if p == pagina => oggetti.as_ref().clone(),
            _ => panic!("gli oggetti della pagina non sono arrivati"),
        }
    }

    fn applica(m: &mut Motore<Esito>, modifica: Modifica) -> Evento<Esito> {
        assert!(m.applica(modifica, true));
        m.aspetta(ATTESA).pop().expect("la risposta del motore").evento
    }

    fn salva(m: &mut Motore<Esito>) -> Vec<u8> {
        let dove = Arc::new(Mutex::new(Vec::new()));
        let d = dove.clone();
        let scrittura: Scrittura<Esito> = Box::new(move |b: &Arc<Vec<u8>>| {
            *d.lock().unwrap() = b.to_vec();
            Ok(b.len())
        });
        m.scrivi(scrittura, true);
        assert!(matches!(m.aspetta(ATTESA).pop().map(|f| f.evento), Some(Evento::Scritta { segnata: true, .. })));
        dove.lock().unwrap().clone()
    }

    fn testi(byte: &[u8], pagina: usize) -> String {
        let pdf = Pdf::apri(byte.to_vec(), None).unwrap();
        pdf.documento().text_lines(pagina).iter().map(|l| l.text.clone()).collect::<Vec<_>>().join("|")
    }

    fn info(m: &Motore<Esito>, pagina: usize) -> PageInfo {
        m.stato().geometrie[pagina].clone()
    }

    #[test]
    fn un_paragrafo_si_riscrive_e_avvisa_quando_cambia_carattere() {
        let mut m = motore(due_pagine());
        let tutti = oggetti(&mut m, 0);
        let o = tutti.iter().find(|o| o.testo == "Pagina di prova").cloned().expect("il paragrafo");
        assert_eq!(o.genere, GenereOggetto::Testo);
        assert!(!format!("{o:?}").contains("prova"));
        // In vista: dall'alto, la riga a 700 pt dal basso sta intorno a 142 pt dall'alto.
        assert!(o.rettangolo[1] < 142.0 && o.rettangolo[3] > 130.0, "{:?}", o.rettangolo);
        let i = info(&m, 0);
        // Lettere che il carattere ha: si riscrive nel suo carattere.
        let e = applica(&mut m, Modifica::testo_dell_oggetto(&o, &i, "Pagina di verifica").unwrap());
        assert!(matches!(e, Evento::Applicata { .. }));
        assert!(oggetti(&mut m, 0).iter().any(|o| o.testo == "Pagina di verifica"));
        // Un simbolo che il carattere non ha: la riga cambierebbe carattere, il motore lo dice e
        // la toglie; Ripeti non la rifà.
        let o = oggetti(&mut m, 0).into_iter().find(|o| o.testo == "Pagina di verifica").unwrap();
        let e = applica(&mut m, Modifica::testo_dell_oggetto(&o, &i, "Pagina ≥ verifica").unwrap());
        let Evento::CambiaCarattere { pagina: 0, modifica } = e else { panic!("doveva avvisare del carattere") };
        assert!(m.stato().annulla && !m.stato().ripeti);
        assert!(oggetti(&mut m, 0).iter().any(|o| o.testo == "Pagina di verifica"));
        // Col sì si fa.
        assert!(matches!(applica(&mut m, modifica.anche_se_cambia_carattere()), Evento::Applicata { .. }));
        assert!(oggetti(&mut m, 0).iter().any(|o| o.testo.contains('≥')));
        let salvato = salva(&mut m);
        assert!(testi(&salvato, 0).contains("Pagina ≥ verifica"));
        // Annullando torna com'era.
        m.annulla();
        m.aspetta(ATTESA);
        assert!(oggetti(&mut m, 0).iter().any(|o| o.testo == "Pagina di verifica"));
    }

    #[test]
    fn un_testo_nuovo_si_sposta_si_ingrandisce_si_riscrive_e_si_toglie() {
        let mut m = motore(due_pagine());
        let i = info(&m, 0);
        let rosso = [0.8, 0.1, 0.1];
        assert!(Modifica::testo_nuovo(0, &i, [100.0, 300.0], "  ", 14.0, rosso).is_none());
        let e = applica(&mut m, Modifica::testo_nuovo(0, &i, [100.0, 300.0], "Visto, si approva", 14.0, rosso).unwrap());
        assert!(matches!(e, Evento::Applicata { toccate: Toccate::Alcune(ref p), .. } if p == &[0]));
        let o = oggetti(&mut m, 0).into_iter().find(|o| matches!(o.genere, GenereOggetto::TestoAggiunto { .. })).expect("il testo aggiunto");
        assert_eq!(o.testo, "Visto, si approva");
        let GenereOggetto::TestoAggiunto { misura, colore } = o.genere else { unreachable!() };
        assert!((misura - 14.0).abs() < 0.01 && (colore[0] - 0.8).abs() < 0.01);
        assert!((o.rettangolo[0] - 100.0).abs() < 2.0 && (o.rettangolo[1] - 300.0).abs() < 4.0, "{:?}", o.rettangolo);
        // Spostato col dito di 50 e 20 pt.
        applica(&mut m, Modifica::sposta_oggetto(&o, &i, 50.0, 20.0));
        let o = oggetti(&mut m, 0).into_iter().find(|o| matches!(o.genere, GenereOggetto::TestoAggiunto { .. })).unwrap();
        assert!((o.rettangolo[0] - 150.0).abs() < 2.0 && (o.rettangolo[1] - 320.0).abs() < 4.0, "{:?}", o.rettangolo);
        // Il doppio più alto: caratteri da 28 pt.
        let [x0, y0, x1, y1] = o.rettangolo;
        applica(&mut m, Modifica::misura_oggetto(&o, &i, [x0, y0, x0 + (x1 - x0) * 2.0, y0 + (y1 - y0) * 2.0]).unwrap());
        let o = oggetti(&mut m, 0).into_iter().find(|o| matches!(o.genere, GenereOggetto::TestoAggiunto { .. })).unwrap();
        assert!(matches!(o.genere, GenereOggetto::TestoAggiunto { misura, .. } if (misura - 28.0).abs() < 0.01));
        assert!((o.rettangolo[0] - 150.0).abs() < 2.0);
        applica(&mut m, Modifica::stile_del_testo(&o, &i, 20.0, [0.1, 0.2, 0.6]).unwrap());
        let o = oggetti(&mut m, 0).into_iter().find(|o| matches!(o.genere, GenereOggetto::TestoAggiunto { .. })).unwrap();
        assert!(
            matches!(o.genere, GenereOggetto::TestoAggiunto { misura, colore } if (misura - 20.0).abs() < 0.01 && (colore[2] - 0.6).abs() < 0.01)
        );
        applica(&mut m, Modifica::testo_dell_oggetto(&o, &i, "Visto").unwrap());
        let salvato = salva(&mut m);
        assert!(testi(&salvato, 0).contains("Visto"));
        let o = oggetti(&mut m, 0).into_iter().find(|o| o.testo == "Visto").unwrap();
        applica(&mut m, Modifica::togli_oggetto(&o).unwrap());
        assert!(!oggetti(&mut m, 0).iter().any(|o| matches!(o.genere, GenereOggetto::TestoAggiunto { .. })));
        // Un paragrafo del PDF si sposta anche lui, e si allarga.
        let p = oggetti(&mut m, 0).into_iter().find(|o| o.testo == "Pagina di prova").unwrap();
        assert!(Modifica::togli_oggetto(&p).is_none());
        applica(&mut m, Modifica::sposta_oggetto(&p, &i, 0.0, 100.0));
        let q = oggetti(&mut m, 0).into_iter().find(|o| o.testo == "Pagina di prova").unwrap();
        assert!((q.rettangolo[1] - p.rettangolo[1] - 100.0).abs() < 2.0, "{:?} {:?}", p.rettangolo, q.rettangolo);
        let [x0, y0, _, y1] = q.rettangolo;
        assert!(matches!(applica(&mut m, Modifica::misura_oggetto(&q, &i, [x0, y0, x0 + 80.0, y1]).unwrap()), Evento::Applicata { .. }));
        let r = oggetti(&mut m, 0).into_iter().filter(|o| o.genere == GenereOggetto::Testo).map(|o| o.rettangolo).collect::<Vec<_>>();
        assert!(r.iter().any(|r| r[3] - r[1] > (y1 - y0) * 1.5), "il paragrafo va a capo nella larghezza nuova: {r:?}");
    }

    #[test]
    fn un_testo_aggiunto_lungo_va_a_capo_prima_del_bordo() {
        // Analisi del 27/10, modifica I4: largo quanto il testo e senza limite, una frase lunga
        // finiva oltre il bordo destro e la fine non si vedeva (restava nel file).
        let mut m = motore(due_pagine());
        let i = info(&m, 0);
        let frase = "Il paziente va rivalutato domani mattina dal medico di guardia prima della dimissione";
        applica(&mut m, Modifica::testo_nuovo(0, &i, [300.0, 400.0], frase, 14.0, [0.0, 0.0, 0.0]).unwrap());
        let o = oggetti(&mut m, 0).into_iter().find(|o| matches!(o.genere, GenereOggetto::TestoAggiunto { .. })).expect("il testo aggiunto");
        assert!(o.rettangolo[2] <= i.width, "il testo esce dalla pagina: {:?}, pagina larga {}", o.rettangolo, i.width);
        assert!(o.rettangolo[3] - o.rettangolo[1] > 14.0 * 1.2 * 1.5, "non è andato a capo: {:?}", o.rettangolo);
        // Uno corto resta su una riga, largo quanto serve.
        applica(&mut m, Modifica::testo_nuovo(0, &i, [60.0, 600.0], "Visto", 14.0, [0.0, 0.0, 0.0]).unwrap());
        let corto = oggetti(&mut m, 0).into_iter().find(|o| o.testo == "Visto").unwrap();
        assert!(corto.rettangolo[3] - corto.rettangolo[1] < 14.0 * 1.2 * 1.5 && corto.rettangolo[2] < 120.0, "{:?}", corto.rettangolo);
    }

    #[test]
    fn allargare_un_paragrafo_su_una_pagina_girata_non_lo_spezza() {
        // Analisi del 27/10, modifica I1: su una pagina girata di 90 gradi la larghezza nuova era
        // quella che si vede, cioè lo spessore delle righe, e il paragrafo diventava una parola
        // per riga.
        let riga = "BT /F1 12 Tf 60 700 Td (Terapia da proseguire a domicilio) Tj ET";
        let mut m = motore(pdf(&[Pagina { larghezza: 595.0, altezza: 842.0, rotazione: 90, contenuto: riga }]));
        let i = info(&m, 0);
        let trova = |m: &mut Motore<Esito>| {
            oggetti(m, 0).into_iter().find(|o| o.genere == GenereOggetto::Testo && o.testo.contains("domicilio")).expect("il paragrafo")
        };
        let p = trova(&mut m);
        assert_eq!(p.righe.lines().count(), 1);
        let [x0, y0, x1, y1] = p.rettangolo;
        applica(&mut m, Modifica::misura_oggetto(&p, &i, [x0, y0, x0 + (x1 - x0) * 1.5, y1]).unwrap());
        let q = trova(&mut m);
        assert_eq!(q.righe.lines().count(), 1, "il paragrafo si è spezzato: {:?}", q.righe);
    }

    #[test]
    fn allargare_un_paragrafo_di_piu_righe_lo_rifa_nella_larghezza_nuova() {
        // Analisi del 27/10, modifica I2: si mandavano le righe con un a capo fra l'una e l'altra,
        // il motore teneva ogni a capo, e allargando il paragrafo restava com'era.
        let tre = "BT /F1 12 Tf 14 TL 60 700 Td (Terapia da proseguire) Tj T* (a domicilio con il) Tj T* (controllo fra due giorni) Tj ET";
        let mut m = motore(pdf(&[Pagina { larghezza: 595.0, altezza: 842.0, rotazione: 0, contenuto: tre }]));
        let i = info(&m, 0);
        let trova = |m: &mut Motore<Esito>| {
            oggetti(m, 0).into_iter().find(|o| o.genere == GenereOggetto::Testo && o.testo.contains("domicilio")).expect("il paragrafo")
        };
        let p = trova(&mut m);
        let [x0, y0, x1, y1] = p.rettangolo;
        assert!(y1 - y0 > 30.0, "tre righe: {:?}", p.rettangolo);
        applica(&mut m, Modifica::misura_oggetto(&p, &i, [x0, y0, x0 + (x1 - x0) * 3.0, y1]).unwrap());
        let q = trova(&mut m);
        assert!(q.rettangolo[3] - q.rettangolo[1] < (y1 - y0) * 0.6, "il paragrafo non si è rifatto: {:?} → {:?}", p.rettangolo, q.rettangolo);
    }

    #[test]
    fn un_immagine_si_inserisce_si_sposta_si_sostituisce_e_si_toglie() {
        let mut m = motore(due_pagine());
        let i = info(&m, 0);
        let foto = Arc::new(foto_di_una_firma(400, 200, 80));
        let e = applica(&mut m, Modifica::immagine_nuova(0, &i, [300.0, 400.0], foto.clone(), 2.0).unwrap());
        assert!(matches!(e, Evento::Applicata { .. }));
        let o = oggetti(&mut m, 0).into_iter().find(|o| o.genere == GenereOggetto::ImmagineAggiunta).expect("l'immagine");
        let [x0, y0, x1, y1] = o.rettangolo;
        // Larga metà pagina, alta la metà della larghezza, centrata sul punto.
        assert!(((x1 - x0) - 297.5).abs() < 1.0 && ((y1 - y0) - 148.75).abs() < 1.0, "{:?}", o.rettangolo);
        assert!(((x0 + x1) / 2.0 - 300.0).abs() < 1.0 && ((y0 + y1) / 2.0 - 400.0).abs() < 1.0);
        applica(&mut m, Modifica::sposta_oggetto(&o, &i, -40.0, 60.0));
        let o = oggetti(&mut m, 0).into_iter().find(|o| o.genere == GenereOggetto::ImmagineAggiunta).unwrap();
        assert!((o.rettangolo[0] - (x0 - 40.0)).abs() < 1.0 && (o.rettangolo[1] - (y0 + 60.0)).abs() < 1.0);
        let [a0, b0, a1, b1] = o.rettangolo;
        applica(&mut m, Modifica::misura_oggetto(&o, &i, [a0, b0, a0 + (a1 - a0) / 2.0, b0 + (b1 - b0) / 2.0]).unwrap());
        let o = oggetti(&mut m, 0).into_iter().find(|o| o.genere == GenereOggetto::ImmagineAggiunta).unwrap();
        assert!(((o.rettangolo[2] - o.rettangolo[0]) - (a1 - a0) / 2.0).abs() < 1.0);
        let altra = Arc::new(foto_di_una_firma(100, 100, 60));
        assert!(matches!(applica(&mut m, Modifica::sostituisci_immagine(&o, altra, 1.0).unwrap()), Evento::Applicata { .. }));
        let salvato = salva(&mut m);
        let riaperto = Pdf::apri(salvato, None).unwrap();
        assert!(riaperto.oggetti(0).iter().any(|o| o.genere == GenereOggetto::ImmagineAggiunta));
        let o = oggetti(&mut m, 0).into_iter().find(|o| o.genere == GenereOggetto::ImmagineAggiunta).unwrap();
        applica(&mut m, Modifica::togli_oggetto(&o).unwrap());
        assert!(!oggetti(&mut m, 0).iter().any(|o| o.genere == GenereOggetto::ImmagineAggiunta));
    }

    /// Un A4 (girato di `rotazione` gradi) con un'immagine grigia di 3 × 1 pixel disegnata con la
    /// matrice `cm`.
    fn con_un_immagine(rotazione: u32, cm: &str) -> Vec<u8> {
        use crate::prova::{flusso, pdf_grezzo};
        pdf_grezzo(&[
            "<< /Type /Catalog /Pages 2 0 R >>".into(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Rotate {rotazione} /Resources << /XObject << /Im1 5 0 R >> >> /Contents 4 0 R >>"
            ),
            flusso("", &format!("q {cm} cm /Im1 Do Q")),
            flusso(
                "/Type /XObject /Subtype /Image /Width 3 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /ASCIIHexDecode",
                "2080C0>",
            ),
        ])
    }

    /// Larghezza su altezza.
    fn proporzione(r: [f32; 4]) -> f32 {
        (r[2] - r[0]) / (r[3] - r[1])
    }

    fn la_prima(m: &mut Motore<Esito>, genere: GenereOggetto) -> Oggetto {
        oggetti(m, 0).into_iter().find(|o| o.genere == genere).expect("l'immagine")
    }

    /// Lo stesso centro, e uno dei due lati uguale: il riquadro più grande che ci sta.
    fn dentro_e_nel_mezzo(dopo: [f32; 4], prima: [f32; 4]) -> bool {
        let uguali = |a: f32, b: f32| (a - b).abs() < 0.5;
        uguali(dopo[0] + dopo[2], prima[0] + prima[2])
            && uguali(dopo[1] + dopo[3], prima[1] + prima[3])
            && (uguali(dopo[2] - dopo[0], prima[2] - prima[0]) || uguali(dopo[3] - dopo[1], prima[3] - prima[1]))
            && dopo[2] - dopo[0] <= prima[2] - prima[0] + 0.5
            && dopo[3] - dopo[1] <= prima[3] - prima[1] + 0.5
    }

    #[test]
    fn la_foto_nuova_tiene_le_sue_proporzioni_nel_riquadro_di_prima() {
        // Un'immagine del PDF larga tre volte l'altezza; al suo posto una foto alta il doppio della
        // larghezza. Prima usciva stirata nel riquadro vecchio (analisi del 27/10, modifica I3).
        let mut m = motore(con_un_immagine(0, "300 0 0 100 100 500"));
        let i = info(&m, 0);
        let immagine = GenereOggetto::Immagine { gruppo: false };
        let o = la_prima(&mut m, immagine);
        let prima = o.rettangolo;
        assert!((proporzione(prima) - 3.0).abs() < 0.01, "{prima:?}");
        let verticale = Arc::new(foto_di_una_firma(100, 200, 60));
        assert!(matches!(applica(&mut m, Modifica::sostituisci_immagine(&o, verticale.clone(), 0.5).unwrap()), Evento::Applicata { .. }));
        let dopo = la_prima(&mut m, immagine).rettangolo;
        assert!((proporzione(dopo) - 0.5).abs() < 0.01 && dentro_e_nel_mezzo(dopo, prima), "{prima:?} → {dopo:?}");

        // Lo stesso con un'immagine aggiunta, larga il doppio dell'altezza.
        let orizzontale = Arc::new(foto_di_una_firma(400, 200, 60));
        applica(&mut m, Modifica::immagine_nuova(0, &i, [300.0, 650.0], orizzontale, 2.0).unwrap());
        let o = la_prima(&mut m, GenereOggetto::ImmagineAggiunta);
        let prima = o.rettangolo;
        assert!((proporzione(prima) - 2.0).abs() < 0.01, "{prima:?}");
        assert!(matches!(applica(&mut m, Modifica::sostituisci_immagine(&o, verticale, 0.5).unwrap()), Evento::Applicata { .. }));
        let dopo = la_prima(&mut m, GenereOggetto::ImmagineAggiunta).rettangolo;
        assert!((proporzione(dopo) - 0.5).abs() < 0.01 && dentro_e_nel_mezzo(dopo, prima), "{prima:?} → {dopo:?}");
        // Sempre una modifica sola: Annulla torna all'immagine di prima, col suo riquadro.
        assert!(m.annulla());
        m.aspetta(ATTESA);
        let tornata = la_prima(&mut m, GenereOggetto::ImmagineAggiunta).rettangolo;
        assert!((proporzione(tornata) - 2.0).abs() < 0.01, "{tornata:?}");
    }

    #[test]
    fn la_foto_nuova_non_si_deforma_in_un_immagine_girata() {
        // Disegnata di traverso: si vede larga 300 e alta 100, ma lungo i suoi lati la larghezza
        // va in su. Una foto alta il doppio della larghezza ci entra coricata: larga 200, alta 100.
        let immagine = GenereOggetto::Immagine { gruppo: false };
        let mut m = motore(con_un_immagine(0, "0 100 -300 0 400 500"));
        let o = la_prima(&mut m, immagine);
        let prima = o.rettangolo;
        assert!((proporzione(prima) - 3.0).abs() < 0.01, "{prima:?}");
        applica(&mut m, Modifica::sostituisci_immagine(&o, Arc::new(foto_di_una_firma(100, 200, 60)), 0.5).unwrap());
        let dopo = la_prima(&mut m, immagine).rettangolo;
        assert!((proporzione(dopo) - 2.0).abs() < 0.01 && dentro_e_nel_mezzo(dopo, prima), "{prima:?} → {dopo:?}");

        // Su una pagina girata la foto gira con la pagina, come tutto quello che c'è sopra.
        let mut m = motore(con_un_immagine(90, "300 0 0 100 100 500"));
        let o = la_prima(&mut m, immagine);
        let prima = o.rettangolo;
        assert!((proporzione(prima) - 1.0 / 3.0).abs() < 0.01, "{prima:?}");
        applica(&mut m, Modifica::sostituisci_immagine(&o, Arc::new(foto_di_una_firma(100, 200, 60)), 0.5).unwrap());
        let dopo = la_prima(&mut m, immagine).rettangolo;
        assert!((proporzione(dopo) - 2.0).abs() < 0.01 && dentro_e_nel_mezzo(dopo, prima), "{prima:?} → {dopo:?}");
    }

    #[test]
    fn l_oscuramento_toglie_il_testo_dai_byte_salvati() {
        let originale = due_pagine();
        assert!(originale.windows(7).any(|w| w == b"segreto"));
        let mut m = motore(originale);
        let segreto = oggetti(&mut m, 0).into_iter().find(|o| o.testo.contains("segreto")).unwrap();
        let i = info(&m, 0);
        assert!(Modifica::oscura(0, &i, &[[0.0, 0.0, 0.5, 0.5]]).is_none());
        let e = applica(&mut m, Modifica::oscura(0, &i, &[segreto.rettangolo]).unwrap());
        assert!(matches!(e, Evento::Applicata { .. }), "l'oscuramento non è riuscito");
        assert!(!oggetti(&mut m, 0).iter().any(|o| o.testo.contains("segreto")));
        let salvato = salva(&mut m);
        // Riscritto tutto: niente revisione vecchia col testo, né nei byte né nel testo.
        assert!(!salvato.windows(7).any(|w| w == b"segreto"));
        assert_eq!(salvato.windows(5).filter(|w| w == b"%%EOF").count(), 1);
        let t = testi(&salvato, 0);
        assert!(!t.contains("segreto") && !t.contains("98765") && t.contains("Pagina di prova"), "{t}");
    }

    /// Bug del 1/11 e del 3/11: le lettere di un documento fotografato e riconosciuto si
    /// correggono, nel carattere della scansione. Il testo della scansione è un oggetto da
    /// correggere (non si sposta, non si toglie). Corretto, nella foto la parola cambiata è
    /// disegnata come le altre e la riga è quasi uguale alla scansione del foglio scritto già
    /// giusto; il testo resta invisibile sopra la foto e si cerca col testo nuovo; la scritta di
    /// prima se ne va dal testo e dai byte del file salvato (la foto di prima non c'è più). Le
    /// altre righe della foto sono com'erano.
    #[test]
    fn il_testo_di_una_scansione_si_corregge_nel_suo_carattere() {
        use crate::correzione::ai_pixel;
        let originale = crate::prova::scansione_riconosciuta_su_carta();
        let prima = Pdf::apri(originale.clone(), None).unwrap();
        let d = prima.documento();
        let (estensione, foto_prima) = d.page_image_file(0, 0).unwrap();
        assert_eq!(estensione, "jpg", "una scansione in JPEG, come quelle dei telefoni");
        let immagine = d.page_images(0)[0].clone();
        let misura = (immagine.width, immagine.height);
        let riga = |t: &str| d.text_lines(0).into_iter().find(|l| l.text.contains(t)).expect("la riga riconosciuta").rect;
        // Le righe nella foto, con l'inchiostro che sporge dal riquadro dell'OCR.
        let fascia = |r: [f64; 4]| {
            let [_, y0, _, y1] = ai_pixel(immagine.matrix, misura, r).unwrap();
            (y0.saturating_sub(12), y1 + 12)
        };
        let (nove, perche) = (fascia(riga("Nove")), fascia(riga("perché")));
        let foto_iniziale = image::load_from_memory(&foto_prima).unwrap().to_rgb8();
        let mut m = motore(originale);
        let tutti = oggetti(&mut m, 0);
        let o = tutti.iter().find(|o| o.genere == GenereOggetto::Scansione && o.testo.contains("Nove")).expect("il testo della scansione").clone();
        assert!(!o.genere.si_muove() && !o.genere.si_toglie() && o.genere.testo());
        assert!(Modifica::togli_oggetto(&o).is_none());
        let foto_del_pdf = tutti.iter().find(|o| o.genere == GenereOggetto::Immagine { gruppo: false }).expect("la foto della pagina");
        assert!(Modifica::correggi_scansione(foto_del_pdf, "Altro").is_none());
        let e = applica(&mut m, Modifica::correggi_scansione(&o, "Dieci giorni fa").unwrap());
        if let Evento::NonApplicata { errore, .. } = &e {
            panic!("la correzione non è riuscita: {errore}");
        }
        assert!(matches!(e, Evento::Applicata { .. }), "la correzione non è riuscita");
        let dopo = oggetti(&mut m, 0);
        assert!(dopo.iter().any(|x| x.genere == GenereOggetto::Scansione && x.testo == "Dieci giorni fa"), "il testo nuovo resta invisibile e da correggere");
        assert!(!dopo.iter().any(|x| x.genere == GenereOggetto::Testo), "niente testo che si vede sopra la foto");
        assert!(!dopo.iter().any(|x| x.testo.contains("Nove")));
        assert!(dopo.iter().any(|x| x.genere == GenereOggetto::Scansione && x.testo.contains("perché")), "le altre righe restano da correggere");
        let salvato = salva(&mut m);
        // La foto di prima non è più nel file: il JPEG sta nel PDF così com'è, e non c'è. (Un
        // pezzo non basta: la carta uniforme fa sequenze uguali anche nel JPEG nuovo.)
        assert!(!salvato.windows(foto_prima.len()).any(|w| w == foto_prima.as_slice()), "la foto di prima è ancora nel file");
        let t = testi(&salvato, 0);
        assert!(t.contains("Dieci") && !t.contains("Nove"), "{t}");
        let riaperto = Pdf::apri(salvato, None).unwrap();
        let blocchi = riaperto.documento().text_blocks(0);
        assert!(blocchi.iter().any(|b| b.text == "Dieci giorni fa") && blocchi.iter().all(|b| b.invisible), "{blocchi:?}");
        assert_eq!(riaperto.documento().page_images(0).len(), 1);
        let (estensione, byte) = riaperto.documento().page_image_file(0, 0).unwrap();
        assert_eq!(estensione, "jpg");
        let foto = image::load_from_memory(&byte).unwrap().to_rgb8();
        // L'inchiostro della riga corretta e quello della scansione del foglio scritto giusto si
        // sovrappongono quasi tutto (a un pixel di distanza): stesso carattere, misura e posto.
        let giusta = crate::prova::foto_su_carta_con_l_ultima_riga("Dieci giorni fa");
        let scuro = |f: &image::RgbImage, x: u32, y: u32| f.get_pixel(x, y).0.iter().map(|v| u32::from(*v)).sum::<u32>() < 3 * 120;
        let vicino = |f: &image::RgbImage, x: u32, y: u32| (x.saturating_sub(1)..=x + 1).any(|x| (y.saturating_sub(1)..=y + 1).any(|y| scuro(f, x, y)));
        let (mut nostri, mut giusti, mut presi, mut trovati) = (0, 0, 0, 0);
        for y in nove.0..nove.1 {
            for x in 0..misura.0 {
                if scuro(&foto, x, y) {
                    nostri += 1;
                    presi += usize::from(vicino(&giusta, x, y));
                }
                if scuro(&giusta, x, y) {
                    giusti += 1;
                    trovati += usize::from(vicino(&foto, x, y));
                }
            }
        }
        assert!(giusti > 200, "la prova non ha l'inchiostro dove serve");
        assert!(presi * 100 >= nostri * 90 && trovati * 100 >= giusti * 90, "inchiostro nostro {presi}/{nostri}, giusto {trovati}/{giusti}");
        // La riga di sopra è quella di prima (a parte il JPEG rifatto).
        let mut diversi = 0;
        for y in perche.0..perche.1 {
            for x in 0..misura.0 {
                diversi += usize::from(scuro(&foto, x, y) != scuro(&foto_iniziale, x, y));
            }
        }
        assert!(diversi < 30, "la riga di sopra è cambiata: {diversi} pixel");
    }

    /// Un'immagine del PDF sostituita o tolta esce davvero dal file salvato. Fino alla 0.8.6 il
    /// motore lasciava il suo nome nelle risorse della pagina, e il salvataggio completo si
    /// portava dietro la foto di prima (correzione in vendor/pdfcraft-edit).
    #[test]
    fn un_immagine_sostituita_o_tolta_non_resta_nel_file() {
        let jpeg = crate::prova::foto_di_un_foglio();
        let s = pdfcraft_engine::Session::new();
        let con_la_foto = s.create_from_images_with_resolution(&[("foglio.jpg".into(), jpeg.clone())], pdfcraft_engine::ImageResolution::Dpi(150.0)).unwrap();
        let dentro = |byte: &[u8]| byte.windows(jpeg.len()).any(|w| w == jpeg.as_slice());
        assert!(dentro(&con_la_foto), "la foto deve stare nel PDF così com'è");
        let mut png = Vec::new();
        image::RgbImage::from_pixel(40, 30, image::Rgb([200, 30, 30])).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let foto = |m: &mut Motore<Esito>| oggetti(m, 0).into_iter().find(|o| o.genere == GenereOggetto::Immagine { gruppo: false }).unwrap();
        let mut m = motore(con_la_foto.to_vec());
        let o = foto(&mut m);
        assert!(matches!(applica(&mut m, Modifica::sostituisci_immagine(&o, Arc::new(png), 4.0 / 3.0).unwrap()), Evento::Applicata { .. }));
        assert!(!dentro(&salva(&mut m)), "la foto sostituita è ancora nel file");
        let mut m = motore(con_la_foto.to_vec());
        let o = foto(&mut m);
        assert!(matches!(applica(&mut m, Modifica::togli_oggetto(&o).unwrap()), Evento::Applicata { .. }));
        assert!(!dentro(&salva(&mut m)), "la foto tolta è ancora nel file");
    }

    /// La correzione si ferma, e la pagina resta com'era, quando la scritta di prima non si può
    /// togliere davvero: sotto il testo riconosciuto non c'è una foto, o la foto è in un formato
    /// che non si sa ripulire (qui JPEG 2000; così anche i fax e JBIG2). Mai la scritta vecchia
    /// nascosta sotto un riquadro.
    #[test]
    fn una_scansione_che_non_si_ripulisce_resta_com_era() {
        use crate::prova::{flusso, pdf_grezzo};
        let con_parola = |byte: Vec<u8>| {
            let mut s = pdfcraft_engine::Session::new();
            let id = s.open("scansione.pdf", None, Arc::new(byte), None).unwrap();
            let words = vec![pdfcraft_engine::ocr::PlacedWord { text: "Mario".into(), origin: [72.0, 700.0], across: [44.0, 0.0], up: [0.0, 16.0] }];
            s.apply(id, Edit::AddOcrText { page: 0, words }).unwrap();
            s.save_full_bytes(id).unwrap().to_vec()
        };
        let senza_foto = con_parola(pdf(&[Pagina::bianca(595.0, 842.0)]));
        let jpeg_2000 = con_parola(pdf_grezzo(&[
            "<< /Type /Catalog /Pages 2 0 R >>".into(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>".into(),
            flusso("/Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /JPXDecode", "xxxx"),
            flusso("", "q 595 0 0 842 0 0 cm /Im0 Do Q"),
        ]));
        for (byte, perche) in [(senza_foto, "senza la foto della scansione"), (jpeg_2000, "foto della scansione")] {
            let mut m = motore(byte);
            let o = oggetti(&mut m, 0).into_iter().find(|o| o.genere == GenereOggetto::Scansione).expect("il testo della scansione");
            match applica(&mut m, Modifica::correggi_scansione(&o, "Luca").unwrap()) {
                Evento::NonApplicata { errore, .. } => assert_eq!(errore, perche),
                _ => panic!("la correzione doveva fermarsi ({perche})"),
            }
            assert!(oggetti(&mut m, 0).iter().any(|x| x.genere == GenereOggetto::Scansione && x.testo == "Mario"));
        }
    }

    #[test]
    fn filigrana_e_numeri_di_pagina() {
        let mut m = motore(due_pagine());
        assert!(matches!(applica(&mut m, Modifica::filigrana(2, "BOZZA", 0.3)), Evento::Applicata { toccate: Toccate::Tutte, .. }));
        let e = applica(&mut m, Modifica::numeri_di_pagina(2, "Relazione", "Pagina <<1>> di <<n>>"));
        assert!(matches!(e, Evento::Applicata { .. }));
        assert!(m.stato().filigrana && m.stato().numeri);
        // Una seconda filigrana prende il posto della prima.
        applica(&mut m, Modifica::filigrana(2, "COPIA", 0.5));
        let salvato = salva(&mut m);
        let riaperto = Pdf::apri(salvato.clone(), None).unwrap();
        let marche = riaperto.documento().marks().to_vec();
        assert!(marche.contains(&pdfcraft_engine::MarkKind::Watermark) && marche.contains(&pdfcraft_engine::MarkKind::HeaderFooter));
        let (uno, due) = (testi(&salvato, 0), testi(&salvato, 1));
        assert!(uno.contains("Pagina 1 di 2") && due.contains("Pagina 2 di 2") && uno.contains("Relazione"), "{uno} / {due}");
        assert!(uno.contains("COPIA") && !uno.contains("BOZZA"), "{uno}");
        applica(&mut m, Modifica::togli_marche(false));
        assert!(!m.stato().filigrana && m.stato().numeri);
        applica(&mut m, Modifica::togli_marche(true));
        assert!(!m.stato().numeri);
    }

    #[test]
    fn gli_a_capo_restano_fuori_dal_pezzo_cambiato() {
        let righe = "Il paziente è\nricoverato in\nMedicina.";
        // Una parola cambiata nella seconda riga: le altre tengono l'a capo.
        assert_eq!(rimetti_gli_a_capo(righe, "Il paziente è dimesso in Medicina."), "Il paziente è\ndimesso in\nMedicina.");
        // Niente di cambiato: tutto com'era.
        assert_eq!(rimetti_gli_a_capo(righe, "Il paziente è ricoverato in Medicina."), righe);
        // In coda e in testa, con lettere accentate vicine al taglio.
        assert_eq!(rimetti_gli_a_capo("è\nà", "è\u{a0}à"), "è\u{a0}à");
        assert_eq!(rimetti_gli_a_capo("uno\ndue", "uno due tre"), "uno\ndue tre");
        assert_eq!(rimetti_gli_a_capo("uno\ndue", ""), "");
        assert_eq!(rimetti_gli_a_capo("", "nuovo"), "nuovo");
        assert_eq!(rimetti_gli_a_capo("aaa\naaa", "aaa aaaa"), "aaa\naaaa");
    }

    #[test]
    fn le_lettere_dei_testi_nuovi() {
        assert_eq!(carattere_che_manca("Perché costa 5 €, così"), None);
        assert_eq!(carattere_che_manca("SpO₂ ≥ 94%"), Some('₂'));
    }

    #[test]
    fn i_generi_degli_oggetti() {
        assert!(GenereOggetto::Testo.testo() && !GenereOggetto::Testo.si_toglie() && !GenereOggetto::Testo.sostituibile());
        assert!(GenereOggetto::Immagine { gruppo: false }.sostituibile() && !GenereOggetto::Immagine { gruppo: true }.sostituibile());
        assert!(GenereOggetto::ImmagineAggiunta.si_toglie() && GenereOggetto::TestoAggiunto { misura: 12.0, colore: [0.0; 3] }.testo());
        assert_eq!(GenereOggetto::Immagine { gruppo: true }.nome(), "disegno");
    }
}
