//! Le annotazioni di un PDF (piano «PDF completo», P5): evidenziare, sottolineare e barrare il
//! testo dalla selezione, la penna col dito e la gomma, le note e il testo libero, le forme e i
//! link del modo Modifica, e la copia appiattita per chi la riceve.
//!
//! Come le vede chi tocca: le annotazioni del motore (`DocInfo::annotations`, senza i segni di
//! Fill & Sign, le risposte e quelle bloccate) con il posto in pt nella pagina come si vede,
//! dopo `/Rotate`, come il lettore. Le modifiche partono da lì e vanno nello spazio utente del
//! PDF: i quadrilateri dalla selezione con `view_rect_to_quad` (ricerca PDFCraft, sezione 7), la
//! nota con l'angolo in alto a sinistra dell'icona come si vede (`icon_anchor`: le note restano
//! diritte sulle pagine girate). L'autore va in `/T` da un'impostazione di chi usa l'app, mai
//! scritto nel codice. Un'annotazione si indica con la sua posizione in `/Annots` della pagina:
//! si rilegge dopo ogni modifica, mai tenuta.
//!
//! Testi, autori e indirizzi sono del documento: mai nella diagnosi (`Debug` non li dice).

use std::sync::Arc;

use pdfcraft_engine::{
    Document, Edit, EditError, LineEnding, LinkAction, LinkStyle, Markup, NOTE_SIZE, NewAnnotation, NoteIcon, Shape, Style, annot_text,
};
use pdfcraft_render::PageInfo;

use crate::apertura::Pdf;
use crate::modifica::{GenereModifica, Modifica, Toccate};
use crate::moduli::in_vista;
use crate::navigazione::Collegamento;

/// Come si segna il testo scelto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marca {
    Evidenzia,
    Sottolinea,
    Barra,
    /// La sottolineatura ondulata: c'è nei PDF di altri, qui non si fa.
    Ondulata,
}

impl Marca {
    fn del_motore(self) -> Markup {
        match self {
            Marca::Evidenzia => Markup::Highlight,
            Marca::Sottolinea => Markup::Underline,
            Marca::Barra => Markup::StrikeOut,
            Marca::Ondulata => Markup::Squiggly,
        }
    }
}

/// Le forme che si disegnano col dito.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Forma {
    Rettangolo,
    Ellisse,
    Linea,
    Freccia,
}

/// Che cos'è un'annotazione, per chi la tocca.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GenereAnnotazione {
    Marca(Marca),
    /// Un disegno a mano libera (inchiostro).
    Penna,
    /// Una nota: l'icona sulla pagina, il testo in un foglio.
    Nota,
    /// Un testo libero (FreeText) coi caratteri di questa misura, in pt.
    TestoLibero {
        misura: f32,
    },
    Forma(Forma),
    /// Le altre (timbri, poligoni, allegati…): si possono soltanto togliere.
    Altro,
}

impl GenereAnnotazione {
    /// Per la diagnosi.
    pub fn nome(self) -> &'static str {
        match self {
            GenereAnnotazione::Marca(Marca::Evidenzia) => "evidenziato",
            GenereAnnotazione::Marca(Marca::Sottolinea) => "sottolineato",
            GenereAnnotazione::Marca(Marca::Barra) => "barrato",
            GenereAnnotazione::Marca(Marca::Ondulata) => "ondulato",
            GenereAnnotazione::Penna => "penna",
            GenereAnnotazione::Nota => "nota",
            GenereAnnotazione::TestoLibero { .. } => "testo libero",
            GenereAnnotazione::Forma(Forma::Rettangolo) => "rettangolo",
            GenereAnnotazione::Forma(Forma::Ellisse) => "ellisse",
            GenereAnnotazione::Forma(Forma::Linea) => "linea",
            GenereAnnotazione::Forma(Forma::Freccia) => "freccia",
            GenereAnnotazione::Altro => "altro",
        }
    }

    /// Ha un testo da leggere e riscrivere.
    pub fn con_testo(self) -> bool {
        matches!(self, GenereAnnotazione::Nota | GenereAnnotazione::TestoLibero { .. })
    }

    /// Ha un colore da cambiare.
    pub fn con_colore(self) -> bool {
        self != GenereAnnotazione::Altro
    }

    /// Ha uno spessore da cambiare (la linea dei disegni e delle forme).
    pub fn con_spessore(self) -> bool {
        matches!(self, GenereAnnotazione::Penna | GenereAnnotazione::Forma(_))
    }
}

/// Un'annotazione su una pagina, da cambiare o togliere.
#[derive(Clone, PartialEq)]
pub struct Annotazione {
    pub pagina: usize,
    /// La sua posizione in `/Annots` della pagina: si rilegge dopo ogni modifica.
    pub indice: usize,
    /// Dove si vede, in pt nella pagina come si vede.
    pub rettangolo: [f32; 4],
    pub genere: GenereAnnotazione,
    /// Il colore (dei caratteri, per un testo libero), da 0 a 1.
    pub colore: Option<[f32; 3]>,
    /// Lo spessore della linea, in pt.
    pub spessore: Option<f32>,
    /// I pezzi di testo segnati (pt della pagina come si vede), per prenderli col dito.
    pub pezzi: Vec<[f32; 4]>,
    /// I due capi di una linea o di una freccia (pt della pagina come si vede).
    pub capi: Option<[[f32; 2]; 2]>,
    /// Il testo di una nota o di un testo libero: mai nella diagnosi.
    pub testo: String,
    /// Chi l'ha scritta, come dice il PDF: mai nella diagnosi.
    pub autore: String,
}

/// Senza testo né autore.
impl std::fmt::Debug for Annotazione {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Annotazione")
            .field("pagina", &self.pagina)
            .field("indice", &self.indice)
            .field("genere", &self.genere)
            .field("rettangolo", &self.rettangolo)
            .finish()
    }
}

impl Annotazione {
    /// Chi è nella pagina: pagina e posizione.
    pub fn chiave(&self) -> (usize, usize) {
        (self.pagina, self.indice)
    }

    /// Quanto dista il punto `pt` (pt nella pagina come si vede) da quello che si vede
    /// dell'annotazione: dai pezzi di testo segnati, dalla linea, o dal riquadro.
    pub fn distanza(&self, pt: [f32; 2]) -> f32 {
        let dal = |r: &[f32; 4]| (r[0] - pt[0]).max(pt[0] - r[2]).max(0.0).hypot((r[1] - pt[1]).max(pt[1] - r[3]).max(0.0));
        if let Some([a, b]) = self.capi {
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let l2 = dx * dx + dy * dy;
            let t = if l2 > 0.0 { (((pt[0] - a[0]) * dx + (pt[1] - a[1]) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
            let d = (pt[0] - (a[0] + t * dx)).hypot(pt[1] - (a[1] + t * dy));
            return (d - self.spessore.unwrap_or(1.0) / 2.0).max(0.0);
        }
        if !self.pezzi.is_empty() {
            return self.pezzi.iter().map(dal).fold(f32::MAX, f32::min);
        }
        dal(&self.rettangolo)
    }
}

/// L'annotazione della pagina `pagina` sotto il punto `pt` (pt nella pagina come si vede), entro
/// `bordo` pt: la più vicina, a pari distanza la più piccola (una nota sopra un rettangolo).
pub fn annotazione_sotto(annotazioni: &[Annotazione], pagina: usize, pt: [f32; 2], bordo: f32) -> Option<&Annotazione> {
    let area = |a: &Annotazione| (a.rettangolo[2] - a.rettangolo[0]) * (a.rettangolo[3] - a.rettangolo[1]);
    annotazioni
        .iter()
        .filter(|a| a.pagina == pagina && a.distanza(pt) <= bordo)
        .min_by(|a, b| (a.distanza(pt), area(a)).partial_cmp(&(b.distanza(pt), area(b))).unwrap_or(std::cmp::Ordering::Equal))
}

fn genere(subtype: &str, intent: Option<&str>, props: Option<&pdfcraft_engine::CommentProps>) -> Option<GenereAnnotazione> {
    Some(match subtype {
        "Highlight" => GenereAnnotazione::Marca(Marca::Evidenzia),
        "Underline" => GenereAnnotazione::Marca(Marca::Sottolinea),
        "StrikeOut" => GenereAnnotazione::Marca(Marca::Barra),
        "Squiggly" => GenereAnnotazione::Marca(Marca::Ondulata),
        "Ink" => GenereAnnotazione::Penna,
        "Text" => GenereAnnotazione::Nota,
        "FreeText" if intent == Some("FreeTextCallout") => GenereAnnotazione::Altro,
        // Il testo battuto di Fill & Sign è un segno del modo Compila.
        "FreeText" if intent == Some("FreeTextTypeWriter") => return None,
        "FreeText" => GenereAnnotazione::TestoLibero { misura: props.and_then(|p| p.font_size).unwrap_or(MISURA_TESTO) as f32 },
        "Square" => GenereAnnotazione::Forma(Forma::Rettangolo),
        "Circle" => GenereAnnotazione::Forma(Forma::Ellisse),
        "Line" => {
            let freccia = props.and_then(|p| p.endings.as_ref()).is_some_and(|e| e.iter().any(|f| *f != LineEnding::None));
            GenereAnnotazione::Forma(if freccia { Forma::Freccia } else { Forma::Linea })
        }
        _ => GenereAnnotazione::Altro,
    })
}

/// Le annotazioni delle pagine che si cambiano col dito: non i segni di Fill & Sign (sono del
/// modo Compila), non le risposte e gli stati, non quelle bloccate.
pub(crate) fn annotazioni_da(doc: &Document) -> Vec<Annotazione> {
    let info = &doc.info;
    info.annotations
        .iter()
        .filter(|a| !a.fill_sign && !a.locked && a.in_reply_to.is_none() && a.state.is_none())
        .filter_map(|a| {
            let pagina = info.pages.get(a.page)?;
            let props = doc.comment_props(a.page, a.index);
            let genere = genere(&a.subtype, a.intent.as_deref(), props.as_ref())?;
            let rettangolo = in_vista(pagina, a.shown_rect(pagina.rotation).map(f64::from))?;
            let pezzi = a
                .quads
                .iter()
                .filter_map(|q| {
                    let xs = [q[0], q[2], q[4], q[6]];
                    let ys = [q[1], q[3], q[5], q[7]];
                    let r = [
                        xs.iter().copied().fold(f32::MAX, f32::min),
                        ys.iter().copied().fold(f32::MAX, f32::min),
                        xs.iter().copied().fold(f32::MIN, f32::max),
                        ys.iter().copied().fold(f32::MIN, f32::max),
                    ];
                    in_vista(pagina, r.map(f64::from))
                })
                .collect();
            let capi = (a.subtype == "Line" && a.points.len() == 2).then(|| {
                let v = |p: [f32; 2]| pagina.user_to_view(p[0], p[1]);
                [v(a.points[0]), v(a.points[1])]
            });
            let colore = props.as_ref().and_then(|p| p.color).map(|c| c.map(|v| v as f32)).or(a.color);
            let spessore = props.as_ref().and_then(|p| p.width).map(|w| w as f32);
            Some(Annotazione {
                pagina: a.page,
                indice: a.index,
                rettangolo,
                genere,
                colore,
                spessore,
                pezzi,
                capi,
                testo: a.contents.clone().unwrap_or_default(),
                autore: a.author.clone().unwrap_or_default(),
            })
        })
        .collect()
}

impl Pdf {
    /// Le annotazioni delle pagine che si cambiano col dito.
    pub fn annotazioni(&self) -> Vec<Annotazione> {
        annotazioni_da(self.documento())
    }

    /// La copia appiattita del documento com'è adesso, modifiche comprese: commenti e campi
    /// diventano contenuto delle pagine, così chi la riceve la vede uguale in ogni lettore e non
    /// la cambia. Un PDF nuovo riscritto tutto (niente revisioni di prima coi commenti); uno
    /// firmato in incrementale (il motore non lo riscrive), con le firme che non saranno più
    /// valide. Il documento aperto resta com'è.
    pub fn appiattita(&self) -> Result<Arc<Vec<u8>>, EditError> {
        let byte = self.sessione.save_bytes(self.id)?;
        let password = self.documento().password.clone();
        let copia = Pdf::apri_condiviso(byte, password.as_deref()).map_err(|_| EditError::Reopen("la copia non si riapre".into()))?;
        let mut copia = copia;
        copia.sessione.apply(copia.id, Edit::Flatten { comments: true, fields: true })?;
        match copia.sessione.save_full_bytes(copia.id) {
            Err(EditError::Signed) => copia.sessione.save_bytes(copia.id),
            altro => altro,
        }
    }
}

/// La misura dei caratteri di un testo libero, in pt, se il PDF non la dice.
pub const MISURA_TESTO: f64 = 14.0;
/// Il testo libero va a capo oltre questa larghezza, in pt.
const LARGHEZZA_TESTO: f64 = 260.0;
/// Il margine dentro il riquadro di un testo libero, in pt.
const MARGINE_TESTO: f64 = 4.0;

fn finito(v: &[f32]) -> bool {
    v.iter().all(|x| x.is_finite())
}

fn rgb(c: [f32; 3]) -> [f64; 3] {
    c.map(|v| f64::from(v.clamp(0.0, 1.0)))
}

fn nuova(pagina: usize, shape: Shape, colore: [f32; 3], spessore: Option<f32>, testo: &str, autore: &str) -> Modifica {
    let mut style = Style::default_for(&shape);
    style.color = rgb(colore);
    if let Some(s) = spessore {
        style.width = f64::from(s.clamp(0.25, 48.0));
    }
    Modifica::del_motore(Edit::AddAnnotation(NewAnnotation { page: pagina, shape, style, contents: testo.to_string(), author: autore.to_string() }))
}

/// Il riquadro (pt nella pagina come si vede) di un testo libero con l'angolo in alto a sinistra
/// in `a`: largo quanto la riga più lunga (fino a `LARGHEZZA_TESTO`, poi a capo), alto quanto le
/// righe, dentro la pagina larga `larga`.
fn riquadro_libero(a: [f32; 2], testo: &str, misura: f64, larga: f32) -> [f32; 4] {
    let lunga = testo.lines().map(|r| annot_text::text_width(r, misura)).fold(0.0, f64::max);
    let posto = (f64::from(larga) - f64::from(a[0]) - 6.0).max(40.0);
    let largo = (lunga + 2.0 * MARGINE_TESTO + 2.0).clamp(24.0, LARGHEZZA_TESTO.min(posto).max(24.0));
    let righe: usize = testo.lines().map(|r| annot_text::wrap(r, misura, largo - 2.0 * MARGINE_TESTO).len().max(1)).sum::<usize>().max(1);
    let alto = righe as f64 * misura * 1.2 + 2.0 * MARGINE_TESTO;
    [a[0], a[1], a[0] + largo as f32, a[1] + alto as f32]
}

/// Il riquadro del testo libero `a` per `testo` coi caratteri di `misura`, come alla nascita, dal
/// suo angolo in alto a sinistra come si vede.
fn riquadro_del_libero(a: &Annotazione, info: &PageInfo, testo: &str, misura: f64) -> Edit {
    let r = riquadro_libero([a.rettangolo[0], a.rettangolo[1]], testo, misura, info.width);
    Edit::ResizeAnnotation { page: a.pagina, index: a.indice, rect: info.view_rect_to_user(r).map(f64::from) }
}

/// Dove porta un link nuovo.
#[derive(Clone, PartialEq)]
pub enum Porta {
    /// Una pagina di questo PDF (da 0).
    Pagina(usize),
    /// Un indirizzo (una pagina web, una mail): mai nella diagnosi.
    Indirizzo(String),
}

impl std::fmt::Debug for Porta {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Porta::Pagina(p) => write!(f, "Pagina({p})"),
            Porta::Indirizzo(_) => f.write_str("Indirizzo"),
        }
    }
}

impl Porta {
    fn azione(&self) -> Option<LinkAction> {
        match self {
            Porta::Pagina(p) => Some(LinkAction::Page(*p)),
            Porta::Indirizzo(i) if !i.trim().is_empty() => Some(LinkAction::Uri(indirizzo_completo(i.trim()))),
            Porta::Indirizzo(_) => None,
        }
    }
}

/// Un indirizzo scritto a mano, come lo vuole un lettore di PDF: «www.esempio.it» diventa
/// «https://www.esempio.it», una mail «mailto:».
pub fn indirizzo_completo(scritto: &str) -> String {
    let s = scritto.trim();
    let ha_schema =
        s.split_once(':').is_some_and(|(schema, _)| !schema.is_empty() && schema.chars().all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c)));
    if ha_schema {
        s.to_string()
    } else if s.contains('@') && !s.contains('/') {
        format!("mailto:{s}")
    } else {
        format!("https://{s}")
    }
}

impl Modifica {
    /// Evidenzia, sottolinea o barra il testo dei `rettangoli` (pt della pagina come si vede, le
    /// righe della selezione) col `colore`. None senza rettangoli.
    pub fn marca(pagina: usize, info: &PageInfo, rettangoli: &[[f32; 4]], marca: Marca, colore: [f32; 3], autore: &str) -> Option<Modifica> {
        let quads: Vec<[f64; 8]> =
            rettangoli.iter().filter(|r| finito(&r[..]) && r[2] > r[0] && r[3] > r[1]).map(|r| info.view_rect_to_quad(*r)).collect();
        if quads.is_empty() {
            return None;
        }
        Some(nuova(pagina, Shape::TextMarkup { kind: marca.del_motore(), quads }, colore, None, "", autore))
    }

    /// Un tratto della penna dai `punti` del dito (pt della pagina come si vede), lisciato, col
    /// `colore` e lo `spessore` (pt). None con meno di due punti.
    pub fn tratto(pagina: usize, info: &PageInfo, punti: &[[f32; 2]], colore: [f32; 3], spessore: f32, autore: &str) -> Option<Modifica> {
        let liscio = crate::tratto::liscia_xy(punti);
        if liscio.len() < 2 {
            return None;
        }
        let tratto: Vec<[f64; 2]> = liscio.iter().map(|p| info.view_to_user(p[0], p[1]).map(f64::from)).collect();
        Some(nuova(pagina, Shape::Ink { strokes: vec![tratto] }, colore, Some(spessore), "", autore))
    }

    /// La gomma lungo il `percorso` del dito (pt della pagina come si vede): toglie dai disegni
    /// della penna le parti entro `raggio` pt (un disegno che resta vuoto se ne va), in un passo
    /// solo. None se il dito non è passato su nessun disegno.
    pub fn gomma(pagina: usize, info: &PageInfo, annotazioni: &[Annotazione], percorso: &[[f32; 2]], raggio: f32) -> Option<Modifica> {
        let percorso: Vec<[f32; 2]> = percorso.iter().filter(|p| finito(&p[..])).copied().collect();
        if percorso.is_empty() || !(raggio.is_finite() && raggio > 0.0) {
            return None;
        }
        // Il percorso fitto (un punto ogni mezzo raggio) per vedere che cosa attraversa, anche fra
        // due punti lontani del dito.
        let mut fitto = vec![percorso[0]];
        for w in percorso.windows(2) {
            let n = ((w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]) / (raggio / 2.0)).ceil().clamp(1.0, 2000.0) as usize;
            fitto.extend((1..=n).map(|k| {
                let t = k as f32 / n as f32;
                [w[0][0] + (w[1][0] - w[0][0]) * t, w[0][1] + (w[1][1] - w[0][1]) * t]
            }));
        }
        let vicino = |a: &Annotazione| fitto.iter().any(|p| a.distanza(*p) <= raggio + a.spessore.unwrap_or(1.0));
        let mut toccati: Vec<usize> =
            annotazioni.iter().filter(|a| a.pagina == pagina && a.genere == GenereAnnotazione::Penna && vicino(a)).map(|a| a.indice).collect();
        if toccati.is_empty() {
            return None;
        }
        // Prima gli ultimi: un disegno tolto tutto sposta quelli dopo.
        toccati.sort_unstable_by(|a, b| b.cmp(a));
        toccati.dedup();
        let path: Vec<[f64; 2]> = percorso.iter().map(|p| info.view_to_user(p[0], p[1]).map(f64::from)).collect();
        let edits = toccati.into_iter().map(|index| Edit::EraseInk { page: pagina, index, path: path.clone(), radius: f64::from(raggio) }).collect();
        Some(Modifica::del_motore(Edit::Batch { label: "Erase".into(), edits }))
    }

    /// Una nota con l'icona centrata nel punto `a` (pt della pagina come si vede) e il `testo`.
    pub fn nota(pagina: usize, info: &PageInfo, a: [f32; 2], testo: &str, colore: [f32; 3], autore: &str) -> Option<Modifica> {
        if !finito(&a) || testo.trim().is_empty() {
            return None;
        }
        // L'angolo in alto a sinistra dell'icona come si vede: le note restano diritte anche
        // sulle pagine girate (icon_anchor dell'automazione).
        let mezza = (NOTE_SIZE / 2.0) as f32;
        let at = info.view_to_user(a[0] - mezza, a[1] - mezza).map(f64::from);
        Some(nuova(pagina, Shape::Note { at, icon: NoteIcon::Comment }, colore, None, testo, autore))
    }

    /// Un testo libero (FreeText) con l'angolo in alto a sinistra nel punto `a` (pt della pagina
    /// come si vede), coi caratteri di `misura` pt e `colore`, senza bordo.
    pub fn testo_libero_nuovo(
        pagina: usize,
        info: &PageInfo,
        a: [f32; 2],
        testo: &str,
        misura: f64,
        colore: [f32; 3],
        autore: &str,
    ) -> Option<Modifica> {
        if !finito(&a) || testo.trim().is_empty() || !misura.is_finite() {
            return None;
        }
        let misura = misura.clamp(4.0, 200.0);
        let r = riquadro_libero(a, testo, misura, info.width);
        let rect = info.view_rect_to_user(r).map(f64::from);
        Some(nuova(pagina, Shape::TextBox { rect, font_size: misura }, colore, Some(0.0), testo, autore))
    }

    /// Una forma disegnata col dito da `da` ad `a` (`capi`, pt della pagina come si vede): il
    /// rettangolo e l'ellisse nel riquadro dei due punti, la linea e la freccia (la punta in `a`)
    /// fra i due. None se è troppo piccola per vederla.
    pub fn forma(
        pagina: usize,
        info: &PageInfo,
        forma: Forma,
        capi: [[f32; 2]; 2],
        colore: [f32; 3],
        spessore: f32,
        autore: &str,
    ) -> Option<Modifica> {
        let [da, a] = capi;
        if !finito(&da) || !finito(&a) {
            return None;
        }
        let shape = match forma {
            Forma::Rettangolo | Forma::Ellisse => {
                let r = [da[0].min(a[0]), da[1].min(a[1]), da[0].max(a[0]), da[1].max(a[1])];
                if r[2] - r[0] < 4.0 || r[3] - r[1] < 4.0 {
                    return None;
                }
                let rect = info.view_rect_to_user(r).map(f64::from);
                if forma == Forma::Rettangolo { Shape::Rectangle { rect } } else { Shape::Oval { rect } }
            }
            Forma::Linea | Forma::Freccia => {
                if (a[0] - da[0]).hypot(a[1] - da[1]) < 6.0 {
                    return None;
                }
                let end = if forma == Forma::Freccia { LineEnding::OpenArrow } else { LineEnding::None };
                Shape::Line {
                    from: info.view_to_user(da[0], da[1]).map(f64::from),
                    to: info.view_to_user(a[0], a[1]).map(f64::from),
                    start: LineEnding::None,
                    end,
                }
            }
        };
        Some(nuova(pagina, shape, colore, Some(spessore), "", autore))
    }

    /// Il testo nuovo di una nota o di un testo libero (vuoto: l'annotazione se ne va). Un testo
    /// libero rifà il riquadro come alla nascita, dallo stesso angolo in alto a sinistra: con la
    /// larghezza di prima andava a capo in una colonna stretta (analisi del 27/10, modifica I5).
    pub fn testo_dell_annotazione(a: &Annotazione, info: &PageInfo, testo: &str) -> Modifica {
        if testo.trim().is_empty() {
            return Modifica::togli_annotazione(a);
        }
        let edit = Edit::SetAnnotationContents { page: a.pagina, index: a.indice, text: testo.to_string() };
        match a.genere {
            GenereAnnotazione::TestoLibero { misura } => {
                let edits = vec![edit, riquadro_del_libero(a, info, testo, f64::from(misura))];
                Modifica::del_motore(Edit::Batch { label: "Riscrivi".into(), edits })
            }
            _ => Modifica::del_motore(edit),
        }
    }

    /// Il colore di un'annotazione (dei caratteri, per un testo libero).
    pub fn colore_dell_annotazione(a: &Annotazione, colore: [f32; 3]) -> Modifica {
        let edit = Edit::StyleAnnotation { page: a.pagina, index: a.indice, color: Some(rgb(colore)), opacity: None, width: None, endings: None };
        Modifica::del_motore(edit)
    }

    /// Lo spessore della linea di un disegno o di una forma, in pt.
    pub fn spessore_dell_annotazione(a: &Annotazione, spessore: f32) -> Modifica {
        let w = f64::from(spessore.clamp(0.25, 48.0));
        let edit = Edit::StyleAnnotation { page: a.pagina, index: a.indice, color: None, opacity: None, width: Some(w), endings: None };
        Modifica::del_motore(edit)
    }

    /// La misura dei caratteri di un testo libero, in pt: il riquadro si rifà come alla nascita
    /// (il motore farebbe crescere solo l'altezza, e il testo andrebbe a capo).
    pub fn misura_dell_annotazione(a: &Annotazione, info: &PageInfo, misura: f64) -> Modifica {
        let misura = misura.clamp(4.0, 200.0);
        let edit = Edit::FontSizeAnnotation { page: a.pagina, index: a.indice, size: misura };
        match a.genere {
            GenereAnnotazione::TestoLibero { .. } => {
                let edits = vec![edit, riquadro_del_libero(a, info, &a.testo, misura)];
                Modifica::del_motore(Edit::Batch { label: "Formato".into(), edits })
            }
            _ => Modifica::del_motore(edit),
        }
    }

    /// Sposta l'annotazione di `dx`, `dy` pt della pagina come si vede.
    pub fn sposta_annotazione(a: &Annotazione, info: &PageInfo, dx: f32, dy: f32) -> Modifica {
        let (p, q) = (info.view_to_user(0.0, 0.0), info.view_to_user(dx, dy));
        let edit = Edit::MoveAnnotation { page: a.pagina, index: a.indice, dx: f64::from(q[0] - p[0]), dy: f64::from(q[1] - p[1]) };
        Modifica::del_motore(edit)
    }

    /// Toglie l'annotazione.
    pub fn togli_annotazione(a: &Annotazione) -> Modifica {
        Modifica::del_motore(Edit::DeleteAnnotation { page: a.pagina, index: a.indice })
    }

    /// Un link nuovo sul riquadro `r` (pt della pagina come si vede) che porta a `porta`. Il
    /// riquadro è invisibile, come i link nuovi di Acrobat. None se il riquadro è troppo piccolo
    /// o l'indirizzo è vuoto.
    pub fn link(pagina: usize, info: &PageInfo, r: [f32; 4], porta: &Porta) -> Option<Modifica> {
        if !finito(&r) || r[2] - r[0] < 4.0 || r[3] - r[1] < 4.0 {
            return None;
        }
        let action = porta.azione()?;
        let rect = info.view_rect_to_user(r).map(f64::from);
        let edit = Edit::AddLink { page: pagina, rect, action, style: LinkStyle::default() };
        Some(Modifica { edit, genere: GenereModifica::Pagine, toccate: Toccate::alcune([pagina]), carattere: None, pulizia: None })
    }

    /// Dove porta il link `c`.
    pub fn cambia_link(c: &Collegamento, porta: &Porta) -> Option<Modifica> {
        let action = porta.azione()?;
        let edit = Edit::SetLink { page: c.pagina, index: c.indice, rect: None, action: Some(action), style: None };
        Some(Modifica { edit, genere: GenereModifica::Pagine, toccate: Toccate::alcune([c.pagina]), carattere: None, pulizia: None })
    }

    /// Toglie il link `c`.
    pub fn togli_link(c: &Collegamento) -> Modifica {
        let edit = Edit::DeleteLink { page: c.pagina, index: c.indice };
        Modifica { edit, genere: GenereModifica::Pagine, toccate: Toccate::alcune([c.pagina]), carattere: None, pulizia: None }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::motore::{Evento, Motore, Scrittura};
    use crate::prova::{Pagina, pdf};
    use std::sync::Mutex;
    use std::time::Duration;

    type Esito = Result<usize, String>;
    const ATTESA: Duration = Duration::from_secs(30);
    const GIALLO: [f32; 3] = [1.0, 0.9, 0.1];
    const ROSSO: [f32; 3] = [0.8, 0.1, 0.1];
    const BLU: [f32; 3] = [0.1, 0.3, 0.8];

    fn lettera(rotazione: u16) -> Vec<u8> {
        let contenuto = "BT /F1 18 Tf 60 760 Td (Relazione di dimissione) Tj ET\nBT /F1 12 Tf 60 700 Td (Terapia da proseguire) Tj ET";
        pdf(&[Pagina { larghezza: 595.0, altezza: 842.0, rotazione, contenuto }, Pagina::bianca(595.0, 842.0)])
    }

    fn motore(byte: Vec<u8>) -> Motore<Esito> {
        Motore::avvia(Pdf::apri(byte, None).unwrap(), Arc::new(|| {})).unwrap()
    }

    fn applica(m: &mut Motore<Esito>, modifica: Modifica) -> Evento<Esito> {
        assert!(m.applica(modifica, true));
        m.aspetta(ATTESA).pop().expect("la risposta del motore").evento
    }

    fn applicata(m: &mut Motore<Esito>, modifica: Modifica) {
        let e = applica(m, modifica);
        assert!(matches!(e, Evento::Applicata { .. }), "la modifica non è passata");
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

    fn info(m: &Motore<Esito>, pagina: usize) -> PageInfo {
        m.stato().geometrie[pagina].clone()
    }

    fn di(m: &Motore<Esito>, genere: GenereAnnotazione) -> Vec<Annotazione> {
        m.stato().annotazioni.iter().filter(|a| a.genere == genere).cloned().collect()
    }

    fn vicino(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.02)
    }

    #[test]
    fn evidenziare_sottolineare_e_barrare_dalla_selezione() {
        let mut m = motore(lettera(0));
        let i = info(&m, 0);
        // La riga «Terapia da proseguire» sta intorno a 142 pt dall'alto.
        let riga = [[60.0, 130.0, 190.0, 145.0]];
        assert!(Modifica::marca(0, &i, &[], Marca::Evidenzia, GIALLO, "").is_none());
        let e = applica(&mut m, Modifica::marca(0, &i, &riga, Marca::Evidenzia, GIALLO, "Anna").unwrap());
        assert!(matches!(e, Evento::Applicata { genere: GenereModifica::Commenti, toccate: Toccate::Alcune(ref p), .. } if p == &[0]));
        applicata(&mut m, Modifica::marca(0, &i, &[[60.0, 80.0, 300.0, 100.0]], Marca::Sottolinea, BLU, "Anna").unwrap());
        applicata(&mut m, Modifica::marca(0, &i, &riga, Marca::Barra, ROSSO, "Anna").unwrap());
        let ev = di(&m, GenereAnnotazione::Marca(Marca::Evidenzia));
        assert_eq!(ev.len(), 1);
        let a = &ev[0];
        assert!(vicino(a.colore.unwrap(), GIALLO), "{:?}", a.colore);
        assert_eq!(a.pezzi.len(), 1);
        assert!((a.pezzi[0][0] - 60.0).abs() < 0.5 && (a.pezzi[0][3] - 145.0).abs() < 0.5, "{:?}", a.pezzi);
        assert_eq!(a.autore, "Anna");
        // Il dito sulla riga prende l'evidenziatura o la barra (stessa riga), non la sottolineatura.
        let sotto = annotazione_sotto(&m.stato().annotazioni, 0, [100.0, 138.0], 4.0).unwrap();
        assert!(matches!(sotto.genere, GenereAnnotazione::Marca(Marca::Evidenzia | Marca::Barra)));
        assert!(annotazione_sotto(&m.stato().annotazioni, 0, [100.0, 400.0], 4.0).is_none());
        assert!(annotazione_sotto(&m.stato().annotazioni, 1, [100.0, 138.0], 4.0).is_none());
        // Il colore cambia, l'annotazione si toglie, e si annulla.
        let a = di(&m, GenereAnnotazione::Marca(Marca::Evidenzia)).remove(0);
        applicata(&mut m, Modifica::colore_dell_annotazione(&a, BLU));
        assert!(vicino(di(&m, GenereAnnotazione::Marca(Marca::Evidenzia))[0].colore.unwrap(), BLU));
        let salvato = salva(&mut m);
        let riletto = Pdf::apri(salvato, None).unwrap().annotazioni();
        assert_eq!(riletto.len(), 3);
        assert!(riletto.iter().all(|a| a.autore == "Anna"));
        let b = di(&m, GenereAnnotazione::Marca(Marca::Barra)).remove(0);
        applicata(&mut m, Modifica::togli_annotazione(&b));
        assert!(di(&m, GenereAnnotazione::Marca(Marca::Barra)).is_empty());
        m.annulla();
        m.aspetta(ATTESA);
        assert_eq!(di(&m, GenereAnnotazione::Marca(Marca::Barra)).len(), 1);
        // Nella diagnosi niente testo né autore.
        assert!(!format!("{:?}", m.stato().annotazioni).contains("Anna"));
    }

    #[test]
    fn la_penna_disegna_e_la_gomma_toglie_dove_passa() {
        let mut m = motore(lettera(0));
        let i = info(&m, 0);
        let onda: Vec<[f32; 2]> = (0..=40).map(|k| [100.0 + k as f32 * 5.0, 400.0 + (k as f32 / 3.0).sin() * 10.0]).collect();
        assert!(Modifica::tratto(0, &i, &onda[..1], ROSSO, 2.0, "").is_none());
        applicata(&mut m, Modifica::tratto(0, &i, &onda, ROSSO, 3.0, "").unwrap());
        applicata(&mut m, Modifica::tratto(0, &i, &[[100.0, 600.0], [300.0, 600.0]], BLU, 1.0, "").unwrap());
        let tratti = di(&m, GenereAnnotazione::Penna);
        assert_eq!(tratti.len(), 2);
        let primo = tratti.iter().find(|a| a.rettangolo[1] < 500.0).unwrap();
        assert!(vicino(primo.colore.unwrap(), ROSSO) && (primo.spessore.unwrap() - 3.0).abs() < 0.01);
        assert!(primo.rettangolo[0] < 102.0 && primo.rettangolo[2] > 298.0, "{:?}", primo.rettangolo);
        // La gomma lontana dai disegni non fa niente; di traverso sul primo lo taglia in due
        // pezzi (resta un disegno solo, coi due tratti), sul secondo tutto lo toglie.
        assert!(Modifica::gomma(0, &i, &m.stato().annotazioni, &[[500.0, 800.0]], 6.0).is_none());
        let taglio = [[200.0, 370.0], [200.0, 430.0]];
        let g = Modifica::gomma(0, &i, &m.stato().annotazioni, &taglio, 6.0).unwrap();
        let e = applica(&mut m, g);
        assert!(matches!(e, Evento::Applicata { toccate: Toccate::Alcune(ref p), .. } if p == &[0]));
        let tratti = di(&m, GenereAnnotazione::Penna);
        assert_eq!(tratti.len(), 2);
        let via: Vec<[f32; 2]> = (0..=20).map(|k| [100.0 + k as f32 * 10.0, 600.0]).collect();
        let g = Modifica::gomma(0, &i, &m.stato().annotazioni, &via, 6.0).unwrap();
        applicata(&mut m, g);
        assert_eq!(di(&m, GenereAnnotazione::Penna).len(), 1);
        // Lo spessore cambia; annullando torna il disegno tolto.
        let a = di(&m, GenereAnnotazione::Penna).remove(0);
        applicata(&mut m, Modifica::spessore_dell_annotazione(&a, 6.0));
        assert!((di(&m, GenereAnnotazione::Penna)[0].spessore.unwrap() - 6.0).abs() < 0.01);
        m.annulla();
        m.annulla();
        m.aspetta(ATTESA);
        assert_eq!(di(&m, GenereAnnotazione::Penna).len(), 2);
    }

    #[test]
    fn il_testo_libero_riscritto_o_ingrandito_rifa_il_riquadro() {
        // Analisi del 27/10, modifica I5: Riscrivi e «Più grande» tenevano la larghezza di prima, e
        // il testo andava a capo in una colonna stretta spezzando le parole.
        let mut m = motore(lettera(0));
        let i = info(&m, 0);
        applicata(&mut m, Modifica::testo_libero_nuovo(0, &i, [80.0, 300.0], "Visto", 14.0, BLU, "").unwrap());
        let libero =
            |m: &Motore<Esito>| m.stato().annotazioni.iter().find(|a| matches!(a.genere, GenereAnnotazione::TestoLibero { .. })).cloned().unwrap();
        let una_riga = |r: [f32; 4], misura: f32| r[3] - r[1] < misura * 1.2 * 1.5 + 2.0 * MARGINE_TESTO as f32;
        let t = libero(&m);
        applicata(&mut m, Modifica::testo_dell_annotazione(&t, &i, "Visto dal medico"));
        let t = libero(&m);
        assert!(una_riga(t.rettangolo, 14.0), "riscritto, va a capo in colonna: {:?}", t.rettangolo);
        applicata(&mut m, Modifica::misura_dell_annotazione(&t, &i, 20.0));
        let t = libero(&m);
        assert!(una_riga(t.rettangolo, 20.0), "ingrandito, va a capo in colonna: {:?}", t.rettangolo);
        assert!((t.rettangolo[0] - 80.0).abs() < 1.0 && (t.rettangolo[1] - 300.0).abs() < 1.0, "l'angolo si è spostato: {:?}", t.rettangolo);
    }

    #[test]
    fn la_nota_sta_dove_si_tocca_anche_sulla_pagina_girata() {
        for rotazione in [0, 90] {
            let mut m = motore(lettera(rotazione));
            let i = info(&m, 0);
            assert!(Modifica::nota(0, &i, [200.0, 300.0], "  ", GIALLO, "").is_none());
            applicata(&mut m, Modifica::nota(0, &i, [200.0, 300.0], "Chiedere al medico", GIALLO, "").unwrap());
            let n = di(&m, GenereAnnotazione::Nota).remove(0);
            let centro = [(n.rettangolo[0] + n.rettangolo[2]) / 2.0, (n.rettangolo[1] + n.rettangolo[3]) / 2.0];
            assert!((centro[0] - 200.0).abs() < 1.5 && (centro[1] - 300.0).abs() < 1.5, "girata {rotazione}: {:?}", n.rettangolo);
            assert_eq!(n.testo, "Chiedere al medico");
            assert!(n.genere.con_testo() && !n.genere.con_spessore());
            assert!(!format!("{n:?}").contains("medico"));
            applicata(&mut m, Modifica::testo_dell_annotazione(&n, &i, "Chiedere al medico di turno"));
            let n = di(&m, GenereAnnotazione::Nota).remove(0);
            assert_eq!(n.testo, "Chiedere al medico di turno");
            // Spostata di 50 pt a destra come si vede.
            applicata(&mut m, Modifica::sposta_annotazione(&n, &i, 50.0, 0.0));
            let s = di(&m, GenereAnnotazione::Nota).remove(0);
            assert!((s.rettangolo[0] - n.rettangolo[0] - 50.0).abs() < 1.0, "girata {rotazione}: {:?} {:?}", n.rettangolo, s.rettangolo);
            // Il testo vuoto la toglie.
            applicata(&mut m, Modifica::testo_dell_annotazione(&s, &i, " "));
            assert!(di(&m, GenereAnnotazione::Nota).is_empty());
        }
    }

    #[test]
    fn il_testo_libero_ha_misura_colore_e_si_riscrive() {
        for rotazione in [0, 90] {
            let mut m = motore(lettera(rotazione));
            let i = info(&m, 0);
            applicata(&mut m, Modifica::testo_libero_nuovo(0, &i, [80.0, 300.0], "Controllo fra due giorni", 16.0, BLU, "").unwrap());
            let t = m.stato().annotazioni.iter().find(|a| matches!(a.genere, GenereAnnotazione::TestoLibero { .. })).cloned().unwrap();
            assert!(matches!(t.genere, GenereAnnotazione::TestoLibero { misura } if (misura - 16.0).abs() < 0.01));
            assert!(vicino(t.colore.unwrap(), BLU), "{:?}", t.colore);
            assert!((t.rettangolo[0] - 80.0).abs() < 1.0 && (t.rettangolo[1] - 300.0).abs() < 1.0, "girata {rotazione}: {:?}", t.rettangolo);
            assert!(t.rettangolo[2] - t.rettangolo[0] > 150.0, "{:?}", t.rettangolo);
            assert_eq!(t.testo, "Controllo fra due giorni");
            applicata(&mut m, Modifica::misura_dell_annotazione(&t, &i, 24.0));
            applicata(&mut m, Modifica::colore_dell_annotazione(&t, ROSSO));
            applicata(&mut m, Modifica::testo_dell_annotazione(&t, &i, "Controllo domani"));
            let t = m.stato().annotazioni.iter().find(|a| matches!(a.genere, GenereAnnotazione::TestoLibero { .. })).cloned().unwrap();
            assert!(matches!(t.genere, GenereAnnotazione::TestoLibero { misura } if (misura - 24.0).abs() < 0.01));
            assert!(vicino(t.colore.unwrap(), ROSSO));
            assert_eq!(t.testo, "Controllo domani");
        }
        // Un testo lungo va a capo: il riquadro resta largo al massimo `LARGHEZZA_TESTO`.
        let r = riquadro_libero([60.0, 100.0], &"parola ".repeat(40), 12.0, 595.0);
        assert!(r[2] - r[0] <= LARGHEZZA_TESTO as f32 + 0.1 && r[3] - r[1] > 50.0, "{r:?}");
    }

    #[test]
    fn forme_col_dito() {
        let mut m = motore(lettera(0));
        let i = info(&m, 0);
        assert!(Modifica::forma(0, &i, Forma::Rettangolo, [[100.0, 100.0], [102.0, 300.0]], ROSSO, 2.0, "").is_none());
        assert!(Modifica::forma(0, &i, Forma::Freccia, [[100.0, 100.0], [102.0, 101.0]], ROSSO, 2.0, "").is_none());
        applicata(&mut m, Modifica::forma(0, &i, Forma::Rettangolo, [[300.0, 300.0], [100.0, 200.0]], ROSSO, 2.0, "").unwrap());
        applicata(&mut m, Modifica::forma(0, &i, Forma::Ellisse, [[100.0, 400.0], [300.0, 500.0]], BLU, 4.0, "").unwrap());
        applicata(&mut m, Modifica::forma(0, &i, Forma::Linea, [[100.0, 600.0], [400.0, 600.0]], ROSSO, 2.0, "").unwrap());
        applicata(&mut m, Modifica::forma(0, &i, Forma::Freccia, [[100.0, 700.0], [400.0, 760.0]], ROSSO, 2.0, "").unwrap());
        let r = di(&m, GenereAnnotazione::Forma(Forma::Rettangolo)).remove(0);
        assert!((r.rettangolo[0] - 100.0).abs() < 3.0 && (r.rettangolo[3] - 300.0).abs() < 3.0, "{:?}", r.rettangolo);
        let e = di(&m, GenereAnnotazione::Forma(Forma::Ellisse)).remove(0);
        assert!(vicino(e.colore.unwrap(), BLU) && (e.spessore.unwrap() - 4.0).abs() < 0.01);
        let l = di(&m, GenereAnnotazione::Forma(Forma::Linea)).remove(0);
        let [a, b] = l.capi.unwrap();
        assert!((a[0] - 100.0).abs() < 0.5 && (a[1] - 600.0).abs() < 0.5 && (b[0] - 400.0).abs() < 0.5, "{:?}", l.capi);
        let f = di(&m, GenereAnnotazione::Forma(Forma::Freccia)).remove(0);
        assert!((f.capi.unwrap()[1][1] - 760.0).abs() < 0.5);
        // Il dito prende la linea vicino al tratto, non lontano anche se dentro il riquadro.
        let tutte = m.stato().annotazioni.clone();
        assert_eq!(annotazione_sotto(&tutte, 0, [250.0, 603.0], 4.0).map(|a| a.genere), Some(GenereAnnotazione::Forma(Forma::Linea)));
        assert_eq!(annotazione_sotto(&tutte, 0, [250.0, 730.0], 4.0).map(|a| a.genere), Some(GenereAnnotazione::Forma(Forma::Freccia)));
        assert!(annotazione_sotto(&tutte, 0, [120.0, 750.0], 4.0).is_none());
        let salvato = salva(&mut m);
        assert_eq!(Pdf::apri(salvato, None).unwrap().annotazioni().len(), 4);
    }

    #[test]
    fn link_a_una_pagina_e_a_un_indirizzo() {
        let mut m = motore(lettera(0));
        let i = info(&m, 0);
        assert!(Modifica::link(0, &i, [60.0, 100.0, 62.0, 200.0], &Porta::Pagina(1)).is_none());
        assert!(Modifica::link(0, &i, [60.0, 100.0, 300.0, 200.0], &Porta::Indirizzo(" ".into())).is_none());
        let e = applica(&mut m, Modifica::link(0, &i, [60.0, 120.0, 300.0, 150.0], &Porta::Pagina(1)).unwrap());
        assert!(matches!(e, Evento::Applicata { genere: GenereModifica::Pagine, toccate: Toccate::Alcune(ref p), .. } if p == &[0]));
        applicata(&mut m, Modifica::link(0, &i, [60.0, 70.0, 300.0, 100.0], &Porta::Indirizzo("www.esempio.it".into())).unwrap());
        let link = m.stato().collegamenti.clone();
        assert_eq!(link.len(), 2);
        let a_pagina = link.iter().find(|c| matches!(c.destinazione, crate::Destinazione::Pagina { pagina: 1, .. })).unwrap().clone();
        assert!((a_pagina.rettangolo[0] - 60.0).abs() < 0.5 && (a_pagina.rettangolo[3] - 150.0).abs() < 0.5);
        assert!(link.iter().any(|c| c.destinazione == crate::Destinazione::Indirizzo("https://www.esempio.it".into())));
        // Cambiato e tolto.
        applicata(&mut m, Modifica::cambia_link(&a_pagina, &Porta::Indirizzo("mario@esempio.it".into())).unwrap());
        let link = m.stato().collegamenti.clone();
        assert!(link.iter().any(|c| c.destinazione == crate::Destinazione::Indirizzo("mailto:mario@esempio.it".into())));
        let primo = link[0].clone();
        applicata(&mut m, Modifica::togli_link(&primo));
        assert_eq!(m.stato().collegamenti.len(), 1);
        assert!(!format!("{:?}", Porta::Indirizzo("segreto.it".into())).contains("segreto"));
    }

    #[test]
    fn gli_indirizzi_scritti_a_mano() {
        assert_eq!(indirizzo_completo(" www.asl.it "), "https://www.asl.it");
        assert_eq!(indirizzo_completo("https://asl.it/a"), "https://asl.it/a");
        assert_eq!(indirizzo_completo("mario.rossi@asl.it"), "mailto:mario.rossi@asl.it");
        assert_eq!(indirizzo_completo("tel:+39061234"), "tel:+39061234");
        assert_eq!(indirizzo_completo("asl.it/pagina"), "https://asl.it/pagina");
    }

    #[test]
    fn la_copia_appiattita_fissa_commenti_e_campi_e_lascia_il_documento() {
        let mut m = motore(crate::prova::modulo_da_compilare());
        let i = info(&m, 0);
        applicata(&mut m, Modifica::nota(0, &i, [300.0, 300.0], "Da firmare", GIALLO, "").unwrap());
        applicata(&mut m, Modifica::tratto(0, &i, &[[100.0, 500.0], [300.0, 520.0]], ROSSO, 2.0, "").unwrap());
        let campi = m.stato().campi.iter().filter(|c| c.compilabile()).count();
        assert!(campi > 0);
        let dove = Arc::new(Mutex::new(Vec::new()));
        let d = dove.clone();
        assert!(m.appiattita(Box::new(move |b: &Arc<Vec<u8>>| {
            *d.lock().unwrap() = b.to_vec();
            Ok(b.len())
        })));
        let fatto = m.aspetta(ATTESA).pop().unwrap().evento;
        assert!(matches!(fatto, Evento::Scritta { segnata: false, .. }));
        assert!(!m.scrive());
        let piatta = Pdf::apri(dove.lock().unwrap().clone(), None).unwrap();
        assert!(piatta.annotazioni().is_empty());
        assert!(piatta.campi().iter().all(|c| !c.compilabile()), "i campi devono essere fissati");
        // Riscritto tutto: una revisione sola.
        assert_eq!(piatta.byte().windows(5).filter(|w| w == b"%%EOF").count(), 1);
        // Il documento aperto è com'era, con le sue modifiche da salvare.
        assert_eq!(m.stato().annotazioni.len(), 2);
        assert!(m.stato().modificato && m.stato().annulla);
        assert_eq!(m.stato().campi.iter().filter(|c| c.compilabile()).count(), campi);
    }
}
