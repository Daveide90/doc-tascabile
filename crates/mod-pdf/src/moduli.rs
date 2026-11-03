//! I moduli del PDF (piano «PDF completo», P2): i campi AcroForm come li vede chi compila (che
//! campo è, dove sta sulle pagine come si vedono, il valore, le scelte), i segni di Fill & Sign
//! (testo libero, spunta, croce, data) e le modifiche che li cambiano.
//!
//! Il valore di un campo va al motore solo alla conferma, mai a ogni tasto (ricerca PDFCraft,
//! sezione 5): `SetFieldValue` riscrive l'aspetto e rifà i calcoli del modulo. Gli script restano
//! spenti; si propone di accenderli solo se il modulo ha calcoli veri, cioè script che non sono le
//! funzioni AF di Acrobat (che girano native anche a script spenti), script del documento o un
//! modulo XFA (verifica della ricerca).
//!
//! I segni di Fill & Sign sono commenti (`AddAnnotation`): `Typewriter` per il testo e la data,
//! `Mark` per spunta e croce, posati dove si tocca con le funzioni del motore (`text_box_at`,
//! `FillMark::rect_at`), che tengono conto della pagina girata.
//!
//! Il nome di un campo, i valori e i testi servono al motore e allo schermo: mai nella diagnosi
//! (`Debug` di `Campo` e `Segno` non li dice).

use std::collections::HashMap;

use pdfcraft_engine::{
    Document, Edit, FieldValue, FillMark, FormField, FormFieldKind, NewAnnotation, Shape, Style, annot_text, field_flags, form_scripts, text_box_at,
};
use pdfcraft_render::PageInfo;

use crate::apertura::Pdf;
use crate::modifica::{GenereModifica, Modifica, Toccate};

/// Che campo è, per chi compila.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenereCampo {
    /// Un campo di testo (anche una data, se ha il formato di una data).
    Testo,
    Casella,
    Radio,
    /// Un menu a tendina, anche con un valore da scrivere.
    Menu,
    /// Un elenco, anche a scelta multipla.
    Elenco,
    /// Un campo della firma non ancora firmato digitalmente: ci si posa la firma di Fill & Sign
    /// (non si compila). Uno firmato è di sola lettura.
    Firma,
    /// I pulsanti: qui non si compilano.
    Altro,
}

/// Un posto del campo su una pagina (il suo widget).
#[derive(Debug, Clone, PartialEq)]
pub struct PostoCampo {
    pub pagina: usize,
    /// In pt nella pagina come si vede, dall'alto a sinistra: [x0, y0, x1, y1].
    pub rettangolo: [f32; 4],
    /// Caselle e radio: il nome dello stato acceso di questo posto.
    pub acceso_come: Option<String>,
    /// Caselle e radio: questo posto è acceso.
    pub acceso: bool,
}

/// Un campo del modulo da compilare.
#[derive(Clone, PartialEq)]
pub struct Campo {
    /// Il nome completo del campo, che vuole il motore: mai nella diagnosi.
    pub nome: String,
    pub genere: GenereCampo,
    /// Testo: il testo. Casella e radio: lo stato acceso (niente se spento). Menu ed elenchi: i
    /// valori esportati scelti.
    pub valore: Vec<String>,
    /// Menu ed elenchi: (valore esportato, testo mostrato).
    pub opzioni: Vec<(String, String)>,
    pub sola_lettura: bool,
    pub obbligatorio: bool,
    pub multilinea: bool,
    pub password: bool,
    /// Un menu in cui si può scrivere un valore che non è fra le scelte.
    pub scrivibile: bool,
    /// Un elenco a scelta multipla.
    pub multipla: bool,
    /// Un gruppo di radio che non si spegne toccando quello acceso.
    pub sempre_uno: bool,
    /// Quanti caratteri al massimo, se il campo lo dice.
    pub massimo: Option<usize>,
    /// Il formato della data (lettere di Acrobat, «dd/mm/yyyy»), se è un campo data.
    pub data: Option<String>,
    /// Il suggerimento del campo (`/TU`): l'etichetta per l'accessibilità.
    pub suggerimento: Option<String>,
    pub posti: Vec<PostoCampo>,
}

/// Senza nome, valori, scelte e suggerimento, che sono del documento.
impl std::fmt::Debug for Campo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Campo")
            .field("genere", &self.genere)
            .field("vuoto", &self.vuoto())
            .field("opzioni", &self.opzioni.len())
            .field("sola_lettura", &self.sola_lettura)
            .field("posti", &self.posti.len())
            .finish()
    }
}

impl Campo {
    /// Si compila qui: un campo di testo, una casella, un radio, un menu o un elenco, non di sola
    /// lettura e con almeno un posto su una pagina.
    pub fn compilabile(&self) -> bool {
        !matches!(self.genere, GenereCampo::Altro | GenereCampo::Firma) && !self.sola_lettura && !self.posti.is_empty()
    }

    /// Non ha un valore: testo vuoto, nessuna scelta, nessun radio acceso. Una casella spenta non
    /// è vuota: spenta è una risposta.
    pub fn vuoto(&self) -> bool {
        match self.genere {
            GenereCampo::Casella | GenereCampo::Altro | GenereCampo::Firma => false,
            _ => self.valore.iter().all(|v| v.trim().is_empty()),
        }
    }

    /// Resta da compilare.
    pub fn da_compilare(&self) -> bool {
        self.compilabile() && self.vuoto()
    }

    /// Il testo di un campo di testo (o il valore scritto di un menu).
    pub fn testo(&self) -> String {
        self.valore.first().cloned().unwrap_or_default()
    }

    /// Il valore come si mostra: le scelte coi loro testi, separate da virgole.
    pub fn mostrato(&self) -> String {
        self.valore.iter().map(|v| self.opzioni.iter().find(|(e, _)| e == v).map_or(v.as_str(), |(_, d)| d.as_str())).collect::<Vec<_>>().join(", ")
    }

    /// Un campo data: si compila col selettore della data o con la tastiera.
    pub fn e_data(&self) -> bool {
        self.data.is_some()
    }

    /// Si scrive con la tastiera: un campo di testo (anche una data) o un menu in cui si scrive.
    pub fn si_scrive(&self) -> bool {
        self.compilabile() && (self.genere == GenereCampo::Testo || (self.genere == GenereCampo::Menu && self.scrivibile))
    }

    /// Il posto `posto` è acceso (caselle e radio).
    pub fn acceso(&self, posto: usize) -> bool {
        self.posti.get(posto).is_some_and(|p| p.acceso)
    }

    /// Il valore dopo un tocco sul posto `posto` di una casella o di un radio: la casella si
    /// accende o si spegne; il radio si accende, e se era già acceso si spegne solo se il gruppo
    /// lo permette (come ui-egui, `forms_ui.rs`). None se il tocco non cambia niente.
    /// Il posto `posto`, spento, è di una casella che il motore non sa accendere lì: ha uno stato
    /// acceso diverso da quello del primo posto, e il motore accende sempre quello del primo
    /// (`pdfcraft-forms`, `set_value`). Toccarlo accenderebbe un altro posto (analisi del 27/10,
    /// compila 6): va detto, e la risposta si segna con Fill & Sign.
    pub fn posto_che_il_motore_non_accende(&self, posto: usize) -> bool {
        let primo = self.posti.iter().find_map(|p| p.acceso_come.as_deref());
        self.genere == GenereCampo::Casella
            && self.posti.get(posto).is_some_and(|p| !p.acceso && p.acceso_come.is_some() && p.acceso_come.as_deref() != primo)
    }

    pub fn valore_dopo_il_tocco(&self, posto: usize) -> Option<FieldValue> {
        let p = self.posti.get(posto)?;
        match self.genere {
            GenereCampo::Casella if self.posto_che_il_motore_non_accende(posto) => None,
            GenereCampo::Casella => Some(FieldValue::Check(!self.posti.iter().any(|p| p.acceso))),
            GenereCampo::Radio if p.acceso && self.sempre_uno => None,
            GenereCampo::Radio if p.acceso => Some(FieldValue::Radio(None)),
            GenereCampo::Radio => Some(FieldValue::Radio(p.acceso_come.clone())),
            _ => None,
        }
    }

    /// Il testo da dare al motore: senza i caratteri oltre il massimo del campo.
    pub fn testo_nei_limiti(&self, testo: &str) -> String {
        match self.massimo {
            Some(n) if n > 0 => testo.chars().take(n).collect(),
            _ => testo.to_string(),
        }
    }

    /// Le pagine dei suoi posti, in ordine e senza doppi.
    pub fn pagine(&self) -> Vec<usize> {
        let mut v: Vec<usize> = self.posti.iter().map(|p| p.pagina).collect();
        v.sort_unstable();
        v.dedup();
        v
    }
}

/// Quanti campi restano da compilare.
pub fn da_compilare(campi: &[Campo]) -> usize {
    campi.iter().filter(|c| c.da_compilare()).count()
}

/// Un rettangolo dello spazio utente del PDF nella pagina come si vede (pt dall'alto a sinistra),
/// coi lati in ordine; None se non è finito.
pub(crate) fn in_vista(info: &PageInfo, r: [f64; 4]) -> Option<[f32; 4]> {
    let (a, b) = (info.user_to_view(r[0] as f32, r[1] as f32), info.user_to_view(r[2] as f32, r[3] as f32));
    let v = [a[0].min(b[0]), a[1].min(b[1]), a[0].max(b[0]), a[1].max(b[1])];
    v.iter().all(|x| x.is_finite()).then_some(v)
}

fn genere(f: &FormField) -> GenereCampo {
    match f.kind {
        FormFieldKind::Text => GenereCampo::Testo,
        FormFieldKind::CheckBox => GenereCampo::Casella,
        FormFieldKind::Radio => GenereCampo::Radio,
        FormFieldKind::Combo => GenereCampo::Menu,
        FormFieldKind::List => GenereCampo::Elenco,
        FormFieldKind::Signature => GenereCampo::Firma,
        FormFieldKind::PushButton => GenereCampo::Altro,
    }
}

/// I campi del motore come li vede chi compila, nell'ordine di tabulazione del documento (le
/// pagine in ordine; nella pagina come dice il PDF). Solo i posti che si vedono su una pagina.
pub fn campi_da(form: &[FormField], pagine: &[PageInfo]) -> Vec<Campo> {
    let mut campi: Vec<(usize, Campo)> = form
        .iter()
        .map(|f| {
            let mut ordine = usize::MAX;
            let posti: Vec<PostoCampo> = f
                .widgets
                .iter()
                .filter(|w| !w.hidden)
                .filter_map(|w| {
                    let pagina = w.page?;
                    let rettangolo = in_vista(pagine.get(pagina)?, w.rect)?;
                    ordine = ordine.min(w.tab);
                    let acceso = w.on_state.is_some() && w.state.is_some() && w.state == w.on_state;
                    Some(PostoCampo { pagina, rettangolo, acceso_come: w.on_state.clone(), acceso })
                })
                .collect();
            let genere = genere(f);
            let data = match &f.actions.format {
                form_scripts::Format::Date(formato) if genere == GenereCampo::Testo => Some(formato.clone()),
                _ => None,
            };
            let campo = Campo {
                nome: f.name.clone(),
                genere,
                valore: f.value.clone(),
                opzioni: f.options.clone(),
                sola_lettura: f.read_only() || f.widgets.iter().all(|w| w.read_only),
                obbligatorio: f.has(field_flags::REQUIRED),
                multilinea: f.has(field_flags::MULTILINE),
                password: f.has(field_flags::PASSWORD),
                scrivibile: f.has(field_flags::EDIT),
                multipla: f.has(field_flags::MULTI_SELECT),
                sempre_uno: f.has(field_flags::NO_TOGGLE_TO_OFF),
                massimo: f.max_len,
                data,
                suggerimento: f.tooltip.clone().filter(|t| !t.trim().is_empty()),
                posti,
            };
            (ordine, campo)
        })
        .collect();
    campi.sort_by_key(|(ordine, _)| *ordine);
    campi.into_iter().map(|(_, c)| c).collect()
}

/// Il modulo ha calcoli veri, che girano solo con gli script accesi: script dei campi che non
/// sono le funzioni AF, script del documento, un modulo XFA. `has_javascript` del motore non basta:
/// è vero anche per i moduli con le sole funzioni AF (verifica della ricerca).
pub fn ha_calcoli(form: &[FormField], script_del_documento: usize, xfa: bool) -> bool {
    xfa || script_del_documento > 0 || form.iter().any(|f| !f.actions.unsupported.is_empty())
}

/// Le pagine dei campi che una modifica ha cambiato (il campo compilato e quelli rifatti dai
/// calcoli): confronta i campi di prima e di dopo. Tutte se il modulo è cambiato di forma.
pub fn pagine_cambiate(prima: &[FormField], dopo: &[FormField]) -> Toccate {
    if prima.len() != dopo.len() {
        return Toccate::Tutte;
    }
    let vecchi: HashMap<(&str, u32, u16), &FormField> = prima.iter().map(|f| ((f.name.as_str(), f.obj.num, f.obj.generation), f)).collect();
    let mut pagine = Vec::new();
    for f in dopo {
        let Some(v) = vecchi.get(&(f.name.as_str(), f.obj.num, f.obj.generation)) else { return Toccate::Tutte };
        let stati = |c: &FormField| c.widgets.iter().map(|w| w.state.clone()).collect::<Vec<_>>();
        if v.value != f.value || stati(v) != stati(f) {
            pagine.extend(f.widgets.iter().filter_map(|w| w.page));
        }
    }
    if pagine.is_empty() { Toccate::Nessuna } else { Toccate::alcune(pagine) }
}

/// Che segno di Fill & Sign è.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GenereSegno {
    /// Testo battuto (anche una data), con la misura dei caratteri in pt.
    Testo { misura: f32 },
    /// Un timbro (spunta, croce, pallino, linea o scritta): si ingrandisce.
    Segno,
    /// Una firma o le iniziali: col dito (fino alla 0.8.1 inchiostro, poi contorni pieni) o da
    /// un'immagine. Si ingrandisce con le sue proporzioni.
    Firma,
    /// Gli altri: si spostano soltanto.
    Altro,
}

/// Un segno di Fill & Sign su una pagina, da spostare, ingrandire o togliere.
#[derive(Clone, PartialEq)]
pub struct Segno {
    pub pagina: usize,
    /// La sua posizione in `/Annots` della pagina (come lo indicano le modifiche): si rilegge
    /// dopo ogni modifica, mai tenuta.
    pub indice: usize,
    /// In pt nella pagina come si vede.
    pub rettangolo: [f32; 4],
    pub genere: GenereSegno,
    /// Il testo di un testo battuto: mai nella diagnosi.
    pub testo: String,
}

/// Senza il testo.
impl std::fmt::Debug for Segno {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Segno").field("pagina", &self.pagina).field("indice", &self.indice).field("genere", &self.genere).finish()
    }
}

/// I segni di Fill & Sign delle pagine, quelli non bloccati.
pub fn segni_da(doc: &Document) -> Vec<Segno> {
    let info = &doc.info;
    info.annotations
        .iter()
        .filter(|a| a.fill_sign && !a.locked)
        .filter_map(|a| {
            let pagina = info.pages.get(a.page)?;
            let rettangolo = in_vista(pagina, a.shown_rect(pagina.rotation).map(f64::from))?;
            let genere = match a.subtype.as_str() {
                "FreeText" => {
                    let misura = doc.comment_props(a.page, a.index).and_then(|p| p.font_size).unwrap_or(MISURA_TESTO);
                    GenereSegno::Testo { misura: misura as f32 }
                }
                // La firma col dito (dalla 0.8.2) e quella da una foto sono timbri col nome nel testo.
                "Stamp" if matches!(a.contents.as_deref(), Some("Signature" | "Initials")) => GenereSegno::Firma,
                "Stamp" => GenereSegno::Segno,
                "Ink" => GenereSegno::Firma,
                _ => GenereSegno::Altro,
            };
            Some(Segno { pagina: a.page, indice: a.index, rettangolo, genere, testo: a.contents.clone().unwrap_or_default() })
        })
        .collect()
}

/// La misura del testo battuto di Fill & Sign, in pt (12, come il testo dei moduli A4), e quella
/// più piccola e più grande che il motore accetta.
pub const MISURA_TESTO: f64 = 12.0;
pub const MISURA_MINIMA: f64 = 4.0;
pub const MISURA_MASSIMA: f64 = 300.0;

/// Il segno da posare con un tocco.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NuovoSegno {
    Spunta,
    Croce,
}

/// Il riquadro di un testo battuto alto e largo quanto il testo, col lato sinistro sul punto `a`
/// (pt della pagina come si vede) e il testo centrato in altezza lì, nello spazio utente.
/// `text_box_at` del motore vuole l'angolo in alto a sinistra, come si vede.
fn riquadro_del_testo(info: &PageInfo, a: [f32; 2], testo: &str, misura: f64) -> Option<[f64; 4]> {
    let largo = (annot_text::text_width(testo, misura) + 8.0).clamp(20.0, 600.0);
    let alto = misura * 1.2 + 6.0;
    let u = info.view_to_user(a[0], a[1] - alto as f32 / 2.0);
    text_box_at(i64::from(info.rotation), [f64::from(u[0]), f64::from(u[1])], largo, alto)
}

impl Modifica {
    /// Il valore di un campo, alla conferma.
    pub fn valore(campo: &Campo, valore: FieldValue) -> Modifica {
        Modifica {
            edit: Edit::SetFieldValue { name: campo.nome.clone(), value: valore },
            genere: GenereModifica::Moduli,
            toccate: Toccate::alcune(campo.pagine()),
            carattere: None,
            pulizia: None,
        }
    }

    /// Un testo battuto di Fill & Sign sulla pagina `pagina` (da 0), col lato sinistro sul punto
    /// `a` (pt della pagina come si vede); `autore` va in `/T` (un'impostazione, mai scritto nel
    /// codice). None se il punto non va.
    pub fn testo_libero(pagina: usize, info: &PageInfo, a: [f32; 2], testo: &str, misura: f64, autore: &str) -> Option<Modifica> {
        let rect = riquadro_del_testo(info, a, testo, misura)?;
        let shape = Shape::Typewriter { rect, font_size: misura };
        let style = Style::default_for(&shape);
        let nuova = NewAnnotation { page: pagina, shape, style, contents: testo.to_string(), author: autore.to_string() };
        Some(Modifica::del_motore(Edit::AddAnnotation(nuova)))
    }

    /// Una spunta o una croce di Fill & Sign centrata sul punto `a` (pt della pagina come si vede).
    pub fn segno(pagina: usize, info: &PageInfo, a: [f32; 2], segno: NuovoSegno, autore: &str) -> Option<Modifica> {
        if !(a[0].is_finite() && a[1].is_finite()) {
            return None;
        }
        let u = info.view_to_user(a[0], a[1]);
        let mark = match segno {
            NuovoSegno::Spunta => FillMark::Check,
            NuovoSegno::Croce => FillMark::Cross,
        };
        let shape = Shape::Mark { rect: mark.rect_at([f64::from(u[0]), f64::from(u[1])], i64::from(info.rotation)), mark };
        let style = Style::default_for(&shape);
        let nuova = NewAnnotation { page: pagina, shape, style, contents: String::new(), author: autore.to_string() };
        Some(Modifica::del_motore(Edit::AddAnnotation(nuova)))
    }

    /// Sposta il segno di `dx`, `dy` pt della pagina come si vede.
    pub fn sposta_segno(s: &Segno, info: &PageInfo, dx: f32, dy: f32) -> Modifica {
        let (a, b) = (info.view_to_user(0.0, 0.0), info.view_to_user(dx, dy));
        let edit = Edit::MoveAnnotation { page: s.pagina, index: s.indice, dx: f64::from(b[0] - a[0]), dy: f64::from(b[1] - a[1]) };
        Modifica::del_motore(edit)
    }

    /// Il segno nel riquadro `r` (pt della pagina come si vede): una spunta o una croce si
    /// ingrandisce; un testo cambia la misura dei caratteri (in proporzione all'altezza) e il suo
    /// riquadro torna alto e largo quanto il testo, dall'angolo in alto a sinistra di `r`.
    pub fn misura_segno(s: &Segno, info: &PageInfo, r: [f32; 4]) -> Option<Modifica> {
        if !r.iter().all(|v| v.is_finite()) || r[2] <= r[0] || r[3] <= r[1] {
            return None;
        }
        match s.genere {
            GenereSegno::Testo { misura } => {
                let alto_prima = (s.rettangolo[3] - s.rettangolo[1]).max(1.0);
                let nuova = (f64::from(misura) * f64::from((r[3] - r[1]) / alto_prima)).clamp(MISURA_MINIMA, MISURA_MASSIMA);
                let nuova = (nuova * 2.0).round() / 2.0;
                let rect = riquadro_del_testo(info, [r[0], r[1] + (nuova * 1.2 + 6.0) as f32 / 2.0], &s.testo, nuova)?;
                let edits = vec![
                    Edit::FontSizeAnnotation { page: s.pagina, index: s.indice, size: nuova },
                    Edit::ResizeAnnotation { page: s.pagina, index: s.indice, rect },
                ];
                Some(Modifica::del_motore(Edit::Batch { label: "Fill & Sign".into(), edits }))
            }
            _ => {
                let u = info.view_rect_to_user(r);
                Some(Modifica::del_motore(Edit::ResizeAnnotation { page: s.pagina, index: s.indice, rect: u.map(f64::from) }))
            }
        }
    }

    /// Un testo battuto nuovo per il segno: il riquadro torna alto e largo quanto il testo, col lato
    /// sinistro e la cima di prima (come in `misura_segno`). Col solo testo il motore teneva la
    /// larghezza di prima e andava a capo in una colonna stretta (analisi del 27/10, compila 7).
    pub fn testo_del_segno(s: &Segno, info: &PageInfo, testo: &str) -> Modifica {
        let edit = Edit::SetAnnotationContents { page: s.pagina, index: s.indice, text: testo.to_string() };
        let GenereSegno::Testo { misura } = s.genere else { return Modifica::del_motore(edit) };
        let misura = f64::from(misura);
        let a = [s.rettangolo[0], s.rettangolo[1] + (misura * 1.2 + 6.0) as f32 / 2.0];
        match riquadro_del_testo(info, a, testo, misura) {
            Some(rect) => {
                let edits = vec![edit, Edit::ResizeAnnotation { page: s.pagina, index: s.indice, rect }];
                Modifica::del_motore(Edit::Batch { label: "Fill & Sign".into(), edits })
            }
            None => Modifica::del_motore(edit),
        }
    }

    /// Toglie il segno.
    pub fn togli_segno(s: &Segno) -> Modifica {
        Modifica::del_motore(Edit::DeleteAnnotation { page: s.pagina, index: s.indice })
    }
}

/// Una data del calendario: anno, mese (da 1) e giorno (da 1), con l'ora se il campo ce l'ha: un
/// campo «data e ora» la tiene quando si cambia il giorno (analisi del 27/10, critico 11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Data {
    pub anno: i32,
    pub mese: u32,
    pub giorno: u32,
    pub ore: u32,
    pub minuti: u32,
    pub secondi: u32,
}

impl Data {
    /// I giorni del mese.
    pub fn giorni_del_mese(anno: i32, mese: u32) -> u32 {
        match mese {
            2 if (anno % 4 == 0 && anno % 100 != 0) || anno % 400 == 0 => 29,
            2 => 28,
            4 | 6 | 9 | 11 => 30,
            _ => 31,
        }
    }

    /// La stessa data col giorno tenuto nel mese (il 31 in un mese di 30 diventa il 30).
    pub fn tenuta(self) -> Data {
        let mese = self.mese.clamp(1, 12);
        let anno = self.anno.clamp(1, 9999);
        Data { anno, mese, giorno: self.giorno.clamp(1, Self::giorni_del_mese(anno, mese)), ..self }
    }

    /// Un giorno, un mese o un anno in più (o in meno), tenendo il giorno nel mese; i giorni e i
    /// mesi girano (dopo il 31 il primo), gli anni no.
    pub fn sposta(self, quale: ParteData, avanti: bool) -> Data {
        let passo: i64 = if avanti { 1 } else { -1 };
        let d = self.tenuta();
        match quale {
            ParteData::Giorno => {
                let n = i64::from(Self::giorni_del_mese(d.anno, d.mese));
                Data { giorno: ((i64::from(d.giorno) - 1 + passo).rem_euclid(n) + 1) as u32, ..d }
            }
            ParteData::Mese => Data { mese: ((i64::from(d.mese) - 1 + passo).rem_euclid(12) + 1) as u32, ..d }.tenuta(),
            ParteData::Anno => Data { anno: (i64::from(d.anno) + passo).clamp(1, 9999) as i32, ..d }.tenuta(),
        }
    }

    /// La data scritta col formato di Acrobat del campo («dd/mm/yyyy», «d mmmm yyyy»…).
    pub fn scritta(self, formato: &str) -> String {
        let d = self.tenuta();
        form_scripts::format_date(form_scripts::DateTime { y: d.anno, m: d.mese, d: d.giorno, hh: d.ore, mm: d.minuti, ss: d.secondi }, formato)
    }

    /// La data di un testo scritto col formato del campo, se è una data.
    pub fn leggi(testo: &str, formato: &str) -> Option<Data> {
        let t = form_scripts::parse_date(testo, formato)?;
        Some(Data { anno: t.y, mese: t.m, giorno: t.d, ore: t.hh, minuti: t.mm, secondi: t.ss })
    }

    /// «24/10/2026»: la data di Fill & Sign.
    pub fn all_italiana(self) -> String {
        self.scritta("dd/mm/yyyy")
    }
}

/// La prima lettera di `testo` che la vecchia tabella dei caratteri occidentali (WinAnsi) non ha:
/// senza un carattere del modulo che ce l'abbia, il motore la disegna «?» nei campi e nei testi di
/// Fill & Sign («Ionuț» diventa «Ionu?»), anche se il valore del campo resta giusto.
pub fn lettera_fuori_da_winansi(testo: &str) -> Option<char> {
    testo.lines().find_map(pdfcraft_fonts::first_non_win_ansi)
}

/// Le parti di una data da spostare col selettore.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParteData {
    Giorno,
    Mese,
    Anno,
}

impl Pdf {
    /// I campi del modulo, come li vede chi compila; i campi della firma già firmati
    /// digitalmente sono di sola lettura.
    pub fn campi(&self) -> Vec<Campo> {
        let d = self.documento();
        let mut campi = campi_da(&d.form, &d.info.pages);
        for c in campi.iter_mut().filter(|c| c.genere == GenereCampo::Firma) {
            c.sola_lettura |= d.signatures.iter().any(|f| f.signed && f.field == c.nome);
        }
        campi
    }

    /// I segni di Fill & Sign delle pagine.
    pub fn segni(&self) -> Vec<Segno> {
        segni_da(self.documento())
    }

    /// Il modulo ha calcoli che vogliono gli script accesi.
    pub fn calcoli(&self) -> bool {
        let d = self.documento();
        ha_calcoli(&d.form, d.document_scripts().len(), d.xfa.is_some() || d.info.xfa.is_some())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn la_casella_a_due_stati_non_accende_il_posto_sbagliato() {
        // Analisi del 27/10, compila 6: toccando «Due» il motore accendeva «Uno» (accende sempre
        // lo stato del primo posto di una casella), e un secondo tocco spegneva tutto.
        let campi = Pdf::apri(crate::prova::casella_a_due_stati(), None).unwrap().campi();
        let c = campi.iter().find(|c| c.suggerimento.as_deref() == Some("Scelta")).unwrap();
        assert_eq!(c.posti.iter().map(|p| p.acceso_come.as_deref()).collect::<Vec<_>>(), [Some("Uno"), Some("Due")]);
        assert_eq!(c.valore_dopo_il_tocco(0), Some(FieldValue::Check(true)));
        assert_eq!(c.valore_dopo_il_tocco(1), None, "toccando «Due» si accenderebbe «Uno»");
    }

    #[test]
    fn le_lettere_fuori_da_winansi() {
        assert_eq!(lettera_fuori_da_winansi("Ionuț Ștefănescu, Łódź"), Some('ț'));
        assert_eq!(lettera_fuori_da_winansi("Perché costa 5 €, così\nà capo"), None);
        assert_eq!(lettera_fuori_da_winansi("SpO₂ ≥ 94%"), Some('₂'));
    }

    #[test]
    fn una_data_con_l_ora_tiene_l_ora() {
        // Analisi del 27/10, critico 11: «Metti la data» in un campo data e ora scriveva 00:00.
        let d = Data::leggi("24/10/2026 14:30", "dd/mm/yyyy HH:MM").unwrap();
        assert_eq!(d.scritta("dd/mm/yyyy HH:MM"), "24/10/2026 14:30");
        assert_eq!(d.sposta(ParteData::Giorno, true).scritta("dd/mm/yyyy HH:MM"), "25/10/2026 14:30");
        assert_eq!(Data::leggi("24/10/2026", "dd/mm/yyyy").unwrap().scritta("dd/mm/yyyy"), "24/10/2026");
    }
    use crate::prova::{CampoDiProva, modulo_da_compilare};

    fn apri() -> Pdf {
        Pdf::apri(modulo_da_compilare(), None).unwrap()
    }

    fn campo<'a>(campi: &'a [Campo], suggerimento: &str) -> &'a Campo {
        campi.iter().find(|c| c.suggerimento.as_deref() == Some(suggerimento)).unwrap_or_else(|| panic!("manca il campo {suggerimento}"))
    }

    #[test]
    fn i_campi_del_modulo_di_prova_coi_generi_i_valori_e_i_posti() {
        let p = apri();
        let campi = p.campi();
        let sottoscritto = campo(&campi, CampoDiProva::Sottoscritto.suggerimento());
        assert_eq!((sottoscritto.genere, sottoscritto.testo().as_str()), (GenereCampo::Testo, "Paziente A"));
        assert!(!sottoscritto.da_compilare());
        let residente = campo(&campi, CampoDiProva::Residente.suggerimento());
        assert!(residente.da_compilare() && residente.si_scrive());
        // Il posto nella pagina come si vede, dall'alto: il campo «residente in» sta sotto quello
        // del sottoscritto, tutti e due nella prima pagina.
        assert_eq!(residente.posti[0].pagina, 0);
        assert!(residente.posti[0].rettangolo[1] > sottoscritto.posti[0].rettangolo[3]);
        let nascita = campo(&campi, CampoDiProva::DataDiNascita.suggerimento());
        assert_eq!(nascita.data.as_deref(), Some("dd/mm/yyyy"));
        let acconsente = campo(&campi, CampoDiProva::Acconsente.suggerimento());
        assert_eq!(acconsente.genere, GenereCampo::Casella);
        assert!(acconsente.acceso(0) && !acconsente.vuoto());
        let reparto = campo(&campi, CampoDiProva::Reparto.suggerimento());
        assert_eq!(reparto.genere, GenereCampo::Menu);
        assert_eq!(reparto.opzioni.len(), 4);
        let allergie = campo(&campi, CampoDiProva::Allergie.suggerimento());
        assert!(allergie.genere == GenereCampo::Elenco && allergie.multipla);
        let ricovero = campo(&campi, CampoDiProva::Ricovero.suggerimento());
        assert_eq!((ricovero.genere, ricovero.posti.len()), (GenereCampo::Radio, 2));
        // I due vuoti del bozzetto 7: «residente in» e «Luogo».
        assert_eq!(da_compilare(&campi), 2);
        // Nella diagnosi (Debug) niente nomi né valori.
        let detto = format!("{campi:?}");
        assert!(!detto.contains("Paziente") && !detto.contains("Residente") && !detto.contains("sottoscritto"), "{detto}");
        // Niente calcoli veri: le date sono funzioni AF.
        assert!(!p.calcoli());
    }

    #[test]
    fn il_tocco_su_caselle_e_radio() {
        let p = apri();
        let campi = p.campi();
        let acconsente = campo(&campi, CampoDiProva::Acconsente.suggerimento());
        assert_eq!(acconsente.valore_dopo_il_tocco(0), Some(FieldValue::Check(false)));
        let non = campo(&campi, CampoDiProva::NonAcconsente.suggerimento());
        assert_eq!(non.valore_dopo_il_tocco(0), Some(FieldValue::Check(true)));
        let ricovero = campo(&campi, CampoDiProva::Ricovero.suggerimento());
        // Il primo radio è acceso; il gruppo del motore (come Acrobat) non si spegne toccandolo, il
        // secondo si accende. Un gruppo che lo permette si spegne.
        assert!(ricovero.acceso(0) && !ricovero.acceso(1) && ricovero.sempre_uno);
        assert_eq!(ricovero.valore_dopo_il_tocco(0), None);
        assert_eq!(ricovero.valore_dopo_il_tocco(1), Some(FieldValue::Radio(ricovero.posti[1].acceso_come.clone())));
        let spegnibile = Campo { sempre_uno: false, ..ricovero.clone() };
        assert_eq!(spegnibile.valore_dopo_il_tocco(0), Some(FieldValue::Radio(None)));
        assert_eq!(acconsente.valore_dopo_il_tocco(9), None);
        let breve = Campo { massimo: Some(3), ..acconsente.clone() };
        assert_eq!(breve.testo_nei_limiti("àbcdef"), "àbc");
    }

    #[test]
    fn le_date_del_selettore() {
        let d = Data { anno: 2026, mese: 1, giorno: 31, ..Default::default() };
        assert_eq!(d.sposta(ParteData::Mese, true), Data { anno: 2026, mese: 2, giorno: 28, ..Default::default() });
        assert_eq!(d.sposta(ParteData::Giorno, true), Data { anno: 2026, mese: 1, giorno: 1, ..Default::default() });
        assert_eq!(d.sposta(ParteData::Mese, false), Data { anno: 2026, mese: 12, giorno: 31, ..Default::default() });
        assert_eq!(
            Data { anno: 2024, mese: 2, giorno: 29, ..Default::default() }.sposta(ParteData::Anno, true),
            Data { anno: 2025, mese: 2, giorno: 28, ..Default::default() }
        );
        assert_eq!(Data { anno: 2026, mese: 10, giorno: 24, ..Default::default() }.all_italiana(), "24/10/2026");
        assert_eq!(Data { anno: 2026, mese: 3, giorno: 5, ..Default::default() }.scritta("mm/dd/yyyy"), "03/05/2026");
        assert_eq!(Data::leggi("12/03/1958", "dd/mm/yyyy"), Some(Data { anno: 1958, mese: 3, giorno: 12, ..Default::default() }));
        assert_eq!(Data::leggi("non è una data", "dd/mm/yyyy"), None);
    }

    type Esito = Result<usize, String>;
    const ATTESA: std::time::Duration = std::time::Duration::from_secs(20);

    fn motore(byte: Vec<u8>) -> crate::Motore<Esito> {
        crate::Motore::avvia(Pdf::apri(byte, None).unwrap(), std::sync::Arc::new(|| {})).unwrap()
    }

    fn del_motore(m: &crate::Motore<Esito>, c: CampoDiProva) -> &Campo {
        campo(&m.stato().campi, c.suggerimento())
    }

    /// Il giro completo col motore vero: testo, data, casella, radio, menu ed elenco compilati,
    /// salvati in incrementale, riaperti coi valori; i campi da compilare scendono a zero.
    #[test]
    fn compila_salva_riapri_e_controlla_i_valori() {
        use crate::{Evento, Toccate};
        let originale = modulo_da_compilare();
        let mut m = motore(originale.clone());
        assert_eq!(da_compilare(&m.stato().campi), 2);
        let residente = del_motore(&m, CampoDiProva::Residente).clone();
        m.applica(Modifica::valore(&residente, FieldValue::Text("Via dei Mille 12, Roma".into())), true);
        let fatti = m.aspetta(ATTESA);
        // Solo la prima pagina, dove sta il campo.
        assert!(
            matches!(&fatti[0].evento, Evento::Applicata { genere: GenereModifica::Moduli, toccate: Toccate::Alcune(p), .. } if p == &[0]),
            "il campo non è stato compilato"
        );
        assert_eq!(del_motore(&m, CampoDiProva::Residente).testo(), "Via dei Mille 12, Roma");
        assert_eq!(da_compilare(&m.stato().campi), 1);
        let luogo = del_motore(&m, CampoDiProva::Luogo).clone();
        m.applica(Modifica::valore(&luogo, FieldValue::Text("Roma".into())), true);
        let non = del_motore(&m, CampoDiProva::NonAcconsente).clone();
        m.applica(Modifica::valore(&non, non.valore_dopo_il_tocco(0).unwrap()), true);
        let ricovero = del_motore(&m, CampoDiProva::Ricovero).clone();
        m.applica(Modifica::valore(&ricovero, ricovero.valore_dopo_il_tocco(1).unwrap()), true);
        let reparto = del_motore(&m, CampoDiProva::Reparto).clone();
        m.applica(Modifica::valore(&reparto, FieldValue::Choice(vec!["Cardiologia".into()])), true);
        let allergie = del_motore(&m, CampoDiProva::Allergie).clone();
        m.applica(Modifica::valore(&allergie, FieldValue::Choice(vec!["Farmaci".into(), "Lattice".into()])), true);
        let data = del_motore(&m, CampoDiProva::Data).clone();
        m.applica(
            Modifica::valore(&data, FieldValue::Text(Data { anno: 2026, mese: 11, giorno: 2, ..Default::default() }.scritta("dd/mm/yyyy"))),
            true,
        );
        let fatti = m.aspetta(ATTESA);
        assert!(fatti.iter().all(|f| matches!(f.evento, Evento::Applicata { .. })), "una modifica non è riuscita");
        assert_eq!(da_compilare(&m.stato().campi), 0);
        assert!(m.stato().modificato);
        let salvati = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let dove = salvati.clone();
        m.scrivi(
            Box::new(move |b: &std::sync::Arc<Vec<u8>>| {
                *dove.lock().unwrap() = b.to_vec();
                Ok(b.len())
            }),
            true,
        );
        m.aspetta(ATTESA);
        assert!(!m.stato().modificato);
        let salvati = salvati.lock().unwrap().clone();
        assert_eq!(salvati.windows(5).filter(|w| w == b"%%EOF").count(), 1, "il salvataggio ha lasciato le revisioni di prima");
        let riaperto = Pdf::apri(salvati, None).unwrap();
        let campi = riaperto.campi();
        assert_eq!(campo(&campi, "Residente in").testo(), "Via dei Mille 12, Roma");
        assert_eq!(campo(&campi, "Luogo").testo(), "Roma");
        assert_eq!(campo(&campi, "Data").testo(), "02/11/2026");
        assert!(campo(&campi, "Non acconsente").acceso(0));
        assert!(campo(&campi, "Ricovero").acceso(1) && !campo(&campi, "Ricovero").acceso(0));
        assert_eq!(campo(&campi, "Reparto").mostrato(), "Cardiologia");
        assert_eq!(campo(&campi, "Allergie").valore, ["Farmaci", "Lattice"]);
        assert_eq!(da_compilare(&campi), 0);
    }

    /// Un campo di sola lettura il motore non lo scrive: il documento resta com'era.
    #[test]
    fn un_valore_che_il_motore_rifiuta_lascia_tutto_com_era() {
        use crate::Evento;
        let mut m = motore(modulo_da_compilare());
        let mut finto = del_motore(&m, CampoDiProva::Residente).clone();
        finto.nome = "non.c.e".into();
        m.applica(Modifica::valore(&finto, FieldValue::Text("x".into())), true);
        assert!(matches!(m.aspetta(ATTESA)[0].evento, Evento::NonApplicata { genere: GenereModifica::Moduli, errore: "valore del campo" }));
        assert!(!m.stato().modificato);
        // Una data che non è una data, per il formato del campo: rifiutata.
        let data = del_motore(&m, CampoDiProva::Data).clone();
        m.applica(Modifica::valore(&data, FieldValue::Text("trentuno febbraio".into())), true);
        assert!(matches!(m.aspetta(ATTESA)[0].evento, Evento::NonApplicata { errore: "valore del campo", .. }));
        assert_eq!(del_motore(&m, CampoDiProva::Data).testo(), "24/10/2026");
    }

    /// Analisi del 27/10, compila 7: Riscrivi con un testo più lungo teneva il riquadro di prima, e
    /// il testo andava a capo in una colonna stretta, anche a metà parola.
    #[test]
    fn un_testo_di_fill_and_sign_riscritto_piu_lungo_resta_su_una_riga() {
        let mut m = motore(crate::prova::modulo());
        let info = m.stato().geometrie[0].clone();
        m.applica(Modifica::testo_libero(0, &info, [100.0, 200.0], "Mario", MISURA_TESTO, "").unwrap(), true);
        m.aspetta(ATTESA);
        let segno = m.stato().segni.iter().find(|s| matches!(s.genere, GenereSegno::Testo { .. })).unwrap().clone();
        let prima = segno.rettangolo;
        m.applica(Modifica::testo_del_segno(&segno, &info, "Mario Rossi, via Roma 12"), true);
        m.aspetta(ATTESA);
        let dopo = m.stato().segni.iter().find(|s| s.indice == segno.indice).unwrap().rettangolo;
        assert!((dopo[3] - dopo[1] - (prima[3] - prima[1])).abs() < 1.0, "è andato a capo: prima {prima:?}, dopo {dopo:?}");
        assert!((dopo[0] - prima[0]).abs() < 1.0 && dopo[2] - dopo[0] > (prima[2] - prima[0]) * 2.5, "prima {prima:?}, dopo {dopo:?}");
    }

    /// Fill & Sign su un PDF senza campi: testo, spunta e croce dove si tocca, poi spostati,
    /// ingranditi, riscritti e tolti.
    #[test]
    fn i_segni_di_fill_and_sign_si_posano_si_spostano_e_si_tolgono() {
        use crate::Evento;
        let mut m = motore(crate::prova::modulo());
        let info = m.stato().geometrie[0].clone();
        m.applica(Modifica::testo_libero(0, &info, [100.0, 200.0], "Mario", MISURA_TESTO, "").unwrap(), true);
        m.applica(Modifica::segno(0, &info, [300.0, 400.0], NuovoSegno::Spunta, "").unwrap(), true);
        m.applica(Modifica::segno(0, &info, [300.0, 450.0], NuovoSegno::Croce, "").unwrap(), true);
        assert!(m.aspetta(ATTESA).iter().all(|f| matches!(f.evento, Evento::Applicata { .. })));
        let segni = m.stato().segni.clone();
        assert_eq!(segni.len(), 3);
        let testo = segni.iter().find(|s| matches!(s.genere, GenereSegno::Testo { .. })).unwrap().clone();
        assert_eq!(testo.testo, "Mario");
        assert_eq!(testo.genere, GenereSegno::Testo { misura: MISURA_TESTO as f32 });
        // Il testo comincia dove si è toccato ed è centrato in altezza lì.
        let [x0, y0, _, y1] = testo.rettangolo;
        assert!((x0 - 100.0).abs() < 1.0 && y0 < 200.0 && y1 > 200.0, "testo fuori posto: {:?}", testo.rettangolo);
        let spunta = segni.iter().find(|s| s.genere == GenereSegno::Segno && s.rettangolo[1] < 420.0).unwrap().clone();
        let c = [(spunta.rettangolo[0] + spunta.rettangolo[2]) / 2.0, (spunta.rettangolo[1] + spunta.rettangolo[3]) / 2.0];
        assert!((c[0] - 300.0).abs() < 1.0 && (c[1] - 400.0).abs() < 1.0, "spunta fuori posto: {c:?}");
        // Spostata di 50 pt a destra e 20 in giù, nella pagina come si vede.
        m.applica(Modifica::sposta_segno(&spunta, &info, 50.0, 20.0), true);
        m.aspetta(ATTESA);
        let dopo = m.stato().segni.iter().find(|s| s.indice == spunta.indice).unwrap().rettangolo;
        assert!((dopo[0] - spunta.rettangolo[0] - 50.0).abs() < 0.5 && (dopo[1] - spunta.rettangolo[1] - 20.0).abs() < 0.5, "{dopo:?}");
        // Ingrandita al doppio.
        let r = [dopo[0], dopo[1], dopo[0] + 2.0 * (dopo[2] - dopo[0]), dopo[1] + 2.0 * (dopo[3] - dopo[1])];
        m.applica(Modifica::misura_segno(&m.stato().segni.iter().find(|s| s.indice == spunta.indice).unwrap().clone(), &info, r).unwrap(), true);
        m.aspetta(ATTESA);
        let grande = m.stato().segni.iter().find(|s| s.indice == spunta.indice).unwrap().rettangolo;
        assert!(((grande[2] - grande[0]) - (r[2] - r[0])).abs() < 0.5, "{grande:?}");
        // Il testo al doppio dell'altezza: la misura raddoppia.
        let t = &testo.rettangolo;
        m.applica(Modifica::misura_segno(&testo, &info, [t[0], t[1], t[2] * 2.0, t[1] + 2.0 * (t[3] - t[1])]).unwrap(), true);
        m.aspetta(ATTESA);
        let testo = m.stato().segni.iter().find(|s| matches!(s.genere, GenereSegno::Testo { .. })).unwrap().clone();
        let GenereSegno::Testo { misura } = testo.genere else { unreachable!() };
        assert!((misura - 24.0).abs() < 0.6, "misura {misura}");
        m.applica(Modifica::testo_del_segno(&testo, &info, "Mario Rossi"), true);
        m.aspetta(ATTESA);
        assert!(m.stato().segni.iter().any(|s| s.testo == "Mario Rossi"));
        // Tolta la croce: ne restano due.
        let croce = m.stato().segni.iter().find(|s| s.genere == GenereSegno::Segno && s.indice != spunta.indice).unwrap().clone();
        m.applica(Modifica::togli_segno(&croce), true);
        m.aspetta(ATTESA);
        assert_eq!(m.stato().segni.len(), 2);
        // Nella diagnosi (Debug) niente testo.
        assert!(!format!("{:?}", m.stato().segni).contains("Mario"));
    }

    /// Su una pagina girata il testo si posa dove si tocca, nella pagina come si vede.
    #[test]
    fn su_una_pagina_girata_il_segno_va_dove_si_tocca() {
        let girata = crate::prova::pdf(&[crate::prova::Pagina { larghezza: 595.0, altezza: 842.0, rotazione: 90, contenuto: "" }]);
        let mut m = motore(girata);
        let info = m.stato().geometrie[0].clone();
        assert_eq!(m.stato().misure[0].larghezza, 842.0);
        m.applica(Modifica::testo_libero(0, &info, [500.0, 100.0], "Su", MISURA_TESTO, "").unwrap(), true);
        m.applica(Modifica::segno(0, &info, [700.0, 300.0], NuovoSegno::Croce, "").unwrap(), true);
        m.aspetta(ATTESA);
        let segni = m.stato().segni.clone();
        let t = segni.iter().find(|s| matches!(s.genere, GenereSegno::Testo { .. })).unwrap().rettangolo;
        assert!((t[0] - 500.0).abs() < 1.0 && t[1] < 100.0 && t[3] > 100.0 && t[2] - t[0] > t[3] - t[1], "testo fuori posto: {t:?}");
        let c = segni.iter().find(|s| s.genere == GenereSegno::Segno).unwrap().rettangolo;
        assert!(((c[0] + c[2]) / 2.0 - 700.0).abs() < 1.0 && ((c[1] + c[3]) / 2.0 - 300.0).abs() < 1.0, "croce fuori posto: {c:?}");
    }

    /// La copia di recupero ha le modifiche, non le segna come salvate e non aspetta Salva; gli
    /// script si accendono.
    #[test]
    fn la_copia_di_recupero_e_gli_script() {
        use crate::Evento;
        let mut m = motore(modulo_da_compilare());
        let luogo = del_motore(&m, CampoDiProva::Luogo).clone();
        m.applica(Modifica::valore(&luogo, FieldValue::Text("Napoli".into())), true);
        let copiati = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let dove = copiati.clone();
        assert!(m.copia(Box::new(move |b: &std::sync::Arc<Vec<u8>>| {
            *dove.lock().unwrap() = b.to_vec();
            Ok(b.len())
        })));
        assert!(!m.scrive(), "la copia conta come un salvataggio");
        let fatti = m.aspetta(ATTESA);
        assert!(matches!(&fatti[1].evento, Evento::Copiata { esito: Ok(n) } if *n > 0));
        assert!(!m.modifica_in_corso() && m.stato().modificato);
        let copia = Pdf::apri(copiati.lock().unwrap().clone(), None).unwrap();
        assert_eq!(campo(&copia.campi(), "Luogo").testo(), "Napoli");
        assert!(!m.stato().javascript);
        m.javascript(true);
        assert!(matches!(m.aspetta(ATTESA)[0].evento, Evento::Javascript { acceso: true }));
        assert!(m.stato().javascript && !m.modifica_in_corso());
    }

    #[test]
    fn calcoli_veri_solo_con_script_che_non_sono_af() {
        // Un campo con uno script di calcolo che non è una funzione AF: i calcoli ci sono.
        assert!(Pdf::apri(crate::prova::modulo_coi_calcoli(), None).unwrap().calcoli());
        assert!(ha_calcoli(&[], 1, false) && ha_calcoli(&[], 0, true) && !ha_calcoli(&[], 0, false));
    }
}
