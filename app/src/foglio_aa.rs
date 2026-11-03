//! Il foglio «Aa» del bozzetto 4: gli stili nel loro aspetto, in una fila che scorre; la misura del
//! testo con meno e più; colore del testo, evidenziatore (ognuno con la sua piccola tavolozza) e
//! togli formato; allineamento e rientri a gruppi segmentati; gli elenchi. Stile, misura,
//! allineamento, colori ed elenchi sono quelli del paragrafo (o della selezione) al cursore.

use eframe::egui::{self, Color32};
use serde_json::{Value, json};
use tascabile_platform::diagnosi;
use tascabile_touch::tema::{self, Carattere};
use tascabile_touch::{Campione, GruppoSegmentato, Passo, Peso, Pulsante, Tavolozza, Tinta, Verso, etichetta_sezione, fila_scorrevole, icone};
use tascabile_word::editor::{Allineamento, Formato, aspetto_stile};

use crate::{Schermata, TascabileApp};

/// Gli stili del foglio: (nome sul campione, comando, nome dello stile nel documento, misura del
/// campione in dp come nel bozzetto: `.stile.n`, `.t1`, `.t2`, `.t3`). Il nome serve per i
/// documenti che hanno id diversi, come il Word italiano, che chiama «Titolo1» lo stile «heading 1».
const STILI: [(&str, &str, &str, f32); 4] = [
    ("Normale", "para.normal", "Normal", 16.0),
    ("Titolo 1", "para.heading1", "heading 1", 22.0),
    ("Titolo 2", "para.heading2", "heading 2", 19.0),
    ("Titolo 3", "para.heading3", "heading 3", 17.0),
];

/// L'id dello stile di ogni comando degli stili, per accendere il campione.
fn id_dello_stile(comando: &str) -> &'static str {
    match comando {
        "para.heading1" => "Heading1",
        "para.heading2" => "Heading2",
        "para.heading3" => "Heading3",
        _ => "Normal",
    }
}

/// I colori dei campioni quando il documento non ha lo stile (come nel bozzetto).
const TESTO_DEL_BOZZETTO: Color32 = Color32::from_rgb(0x22, 0x22, 0x22);
const TITOLI_DEL_BOZZETTO: Color32 = Color32::from_rgb(0x1F, 0x3B, 0x63);

/// Il gruppo del testo: colore, evidenziatore, togli formato.
const TESTO: [(&str, &str); 3] =
    [(icone::FORMAT_COLOR_TEXT, "Colore del testo"), (icone::FORMAT_INK_HIGHLIGHTER, "Evidenziatore"), (icone::FORMAT_CLEAR, "Togli il formato")];
/// Togli formato: via il formato dei caratteri (misura, colori, grassetto…), lo stile del
/// paragrafo resta. `format.clear` rimetterebbe anche il paragrafo a Normale.
const TOGLI_FORMATO: &str = "format.resetChar";

/// L'allineamento, coi comandi che lo mettono senza alternarlo (`para.align` col valore: i
/// pulsanti di Word, toccati di nuovo, tornano a sinistra).
const ALLINEAMENTI: [(&str, &str); 4] = [
    (icone::FORMAT_ALIGN_LEFT, "A sinistra"),
    (icone::FORMAT_ALIGN_CENTER, "Al centro"),
    (icone::FORMAT_ALIGN_RIGHT, "A destra"),
    (icone::FORMAT_ALIGN_JUSTIFY, "Giustificato"),
];
const VALORI_ALLINEAMENTO: [(Allineamento, &str); 4] =
    [(Allineamento::Sinistra, "left"), (Allineamento::Centro, "center"), (Allineamento::Destra, "right"), (Allineamento::Giustificato, "justify")];
const RIENTRI: [(&str, &str); 2] = [(icone::FORMAT_INDENT_DECREASE, "Meno rientro"), (icone::FORMAT_INDENT_INCREASE, "Più rientro")];

/// La tavolozza del colore del testo: automatico, poi otto colori standard di Word leggibili sul
/// bianco (senza il giallo e il verde chiaro). (tinta, nome, valore per `format.color`).
const COLORI: [(Tinta, &str, &str); 9] = [
    (Tinta::Automatico, "Automatico", "auto"),
    (Tinta::Colore(Color32::from_rgb(0xC0, 0x00, 0x00)), "Rosso scuro", "C00000"),
    (Tinta::Colore(Color32::from_rgb(0xFF, 0x00, 0x00)), "Rosso", "FF0000"),
    (Tinta::Colore(Color32::from_rgb(0xFF, 0xC0, 0x00)), "Arancione", "FFC000"),
    (Tinta::Colore(Color32::from_rgb(0x00, 0xB0, 0x50)), "Verde", "00B050"),
    (Tinta::Colore(Color32::from_rgb(0x00, 0xB0, 0xF0)), "Azzurro", "00B0F0"),
    (Tinta::Colore(Color32::from_rgb(0x00, 0x70, 0xC0)), "Blu", "0070C0"),
    (Tinta::Colore(Color32::from_rgb(0x00, 0x20, 0x60)), "Blu scuro", "002060"),
    (Tinta::Colore(Color32::from_rgb(0x70, 0x30, 0xA0)), "Viola", "7030A0"),
];

/// La tavolozza dell'evidenziatore: nessuno, poi otto dei colori dell'evidenziatore di Word (i
/// soli che ha), quelli su cui il testo nero si legge. (tinta, nome, valore per
/// `format.highlight`, colore come lo dice `format.state`).
const EVIDENZIATORI: [(Tinta, &str, &str, &str); 9] = [
    (Tinta::Nessuno, "Nessuno", "none", ""),
    (Tinta::Colore(Color32::from_rgb(0xFF, 0xFF, 0x00)), "Giallo", "yellow", "FFFF00"),
    (Tinta::Colore(Color32::from_rgb(0x00, 0xFF, 0x00)), "Verde chiaro", "brightGreen", "00FF00"),
    (Tinta::Colore(Color32::from_rgb(0x00, 0xFF, 0xFF)), "Turchese", "turquoise", "00FFFF"),
    (Tinta::Colore(Color32::from_rgb(0xFF, 0x00, 0xFF)), "Rosa", "pink", "FF00FF"),
    (Tinta::Colore(Color32::from_rgb(0xFF, 0x00, 0x00)), "Rosso", "red", "FF0000"),
    (Tinta::Colore(Color32::from_rgb(0x00, 0x80, 0x00)), "Verde", "green", "008000"),
    (Tinta::Colore(Color32::from_rgb(0x80, 0x80, 0x00)), "Giallo scuro", "darkYellow", "808000"),
    (Tinta::Colore(Color32::from_rgb(0xC0, 0xC0, 0xC0)), "Grigio chiaro", "gray25", "C0C0C0"),
];

/// Quale tavolozza è aperta sotto la fila del testo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum QualeTavolozza {
    Colore,
    Evidenziatore,
}

/// L'aspetto di un campione: carattere, misura e colore dello stile nel documento.
struct AspettoCampione {
    carattere: Carattere,
    colore: Color32,
}

/// L'aspetto dei quattro campioni, dagli stili del documento (il colore e il grassetto; la misura
/// è quella del bozzetto, perché un titolo a 20 punti non starebbe nel campione).
fn aspetti(s: &wordcraft_engine::Session) -> Vec<AspettoCampione> {
    STILI
        .iter()
        .enumerate()
        .map(|(i, (_, _, nome, _))| {
            let ripiego = if i == 0 { TESTO_DEL_BOZZETTO } else { TITOLI_DEL_BOZZETTO };
            match aspetto_stile(s, nome) {
                Some(a) => AspettoCampione {
                    carattere: if a.grassetto { Carattere::Grassetto } else { Carattere::Testo },
                    colore: a.colore.map_or(TESTO_DEL_BOZZETTO, |[r, g, b]| Color32::from_rgb(r, g, b)),
                },
                None => AspettoCampione { carattere: if i == 0 { Carattere::Testo } else { Carattere::Grassetto }, colore: ripiego },
            }
        })
        .collect()
}

/// Il contenuto del foglio: restituisce il comando toccato, col suo valore. Gli stili tornano
/// come `para.style` col nome dello stile e il comando del motore per ripiego (`stile_scelto`).
fn contenuto(ui: &mut egui::Ui, f: &Formato, campioni: &[AspettoCampione], tavolozza: &mut Option<QualeTavolozza>) -> Option<(&'static str, Value)> {
    let mut scelto = None;
    etichetta_sezione(ui, "Stile");
    fila_scorrevole(ui, "stili_aa", tema::FOGLIO, |ui| {
        for ((nome, comando, nome_stile, misura), a) in STILI.iter().zip(campioni) {
            let acceso = f.stile == id_dello_stile(comando) || f.nome_stile.eq_ignore_ascii_case(nome_stile);
            if ui.add(Campione::new(nome, a.carattere, *misura, a.colore).acceso(acceso)).clicked() {
                scelto = Some(("para.style", json!({ "style": nome_stile, "ripiego": comando })));
            }
        }
    });
    etichetta_sezione(ui, "Testo");
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        let valore = f.dimensione();
        let passo = Passo::new(&valore, "Più piccolo", "Più grande").meno_spento(f.mezzi_punti.is_some_and(|m| m <= 2)).mostra(ui);
        match passo {
            Some(Verso::Meno) => scelto = Some(("format.shrinkFont", json!({}))),
            Some(Verso::Piu) => scelto = Some(("format.growFont", json!({}))),
            None => {}
        }
        let mut gruppo = GruppoSegmentato::new(&TESTO);
        if let Some(t) = *tavolozza {
            gruppo = gruppo.acceso(Some(if t == QualeTavolozza::Colore { 0 } else { 1 }));
        }
        match gruppo.mostra(ui) {
            Some(i @ (0 | 1)) => {
                let quale = if i == 0 { QualeTavolozza::Colore } else { QualeTavolozza::Evidenziatore };
                *tavolozza = if *tavolozza == Some(quale) { None } else { Some(quale) };
            }
            Some(_) => scelto = Some((TOGLI_FORMATO, json!({}))),
            None => {}
        }
    });
    match *tavolozza {
        Some(QualeTavolozza::Colore) => {
            etichetta_sezione(ui, "Colore del testo");
            let tinte: Vec<(Tinta, &str)> = COLORI.iter().map(|(t, n, _)| (*t, *n)).collect();
            let adesso = f.colore.as_deref().and_then(|c| COLORI.iter().position(|(_, _, v)| v.eq_ignore_ascii_case(c)));
            if let Some(i) = Tavolozza::new(&tinte).acceso(adesso).mostra(ui) {
                scelto = Some(("format.color", json!({ "color": COLORI[i].2 })));
                *tavolozza = None;
            }
        }
        Some(QualeTavolozza::Evidenziatore) => {
            etichetta_sezione(ui, "Evidenziatore");
            let tinte: Vec<(Tinta, &str)> = EVIDENZIATORI.iter().map(|(t, n, _, _)| (*t, *n)).collect();
            let adesso = EVIDENZIATORI.iter().position(|(_, _, _, hex)| f.evidenziatore.as_deref().unwrap_or("").eq_ignore_ascii_case(hex));
            if let Some(i) = Tavolozza::new(&tinte).acceso(adesso).mostra(ui) {
                scelto = Some(("format.highlight", json!({ "color": EVIDENZIATORI[i].2 })));
                *tavolozza = None;
            }
        }
        None => {}
    }
    etichetta_sezione(ui, "Paragrafo");
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        let acceso = VALORI_ALLINEAMENTO.iter().position(|(a, _)| *a == f.allineamento);
        if let Some(i) = GruppoSegmentato::new(&ALLINEAMENTI).acceso(acceso).mostra(ui) {
            scelto = Some(("para.align", json!({ "value": VALORI_ALLINEAMENTO[i].1 })));
        }
        match GruppoSegmentato::new(&RIENTRI).mostra(ui) {
            Some(0) => scelto = Some(("para.outdent", json!({}))),
            Some(_) => scelto = Some(("para.indent", json!({}))),
            None => {}
        }
    });
    etichetta_sezione(ui, "Elenchi");
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        let largo = (ui.available_width() - 10.0) / 2.0;
        for (nome, icona, comando, acceso) in [
            ("Puntato", icone::FORMAT_LIST_BULLETED, "para.bullets", f.puntato),
            ("Numerato", icone::FORMAT_LIST_NUMBERED, "para.numbering", f.numerato),
        ] {
            if ui.add(Pulsante::new(nome, Peso::Tenue).icona(icona).acceso(acceso).larghezza(largo)).clicked() {
                scelto = Some((comando, json!({})));
            }
        }
    });
    scelto
}

impl TascabileApp {
    /// Il foglio «Aa», sopra l'Editor; va mostrato a ogni frame perché salga. Ogni comando passa
    /// da `Editor::comando_con_valore`, che risincronizza la tastiera, e il foglio resta aperto
    /// per il comando dopo. La tavolozza aperta si chiude col foglio.
    pub(crate) fn mostra_foglio_aa(&mut self, ctx: &egui::Context) {
        if self.schermata != Schermata::Editor || self.aperto.is_none() {
            self.foglio_aa.chiudi();
        }
        if !self.foglio_aa.aperto() {
            self.tavolozza_aa = None;
        }
        let Some(e) = self.aperto.as_ref().map(|a| &a.editor) else {
            self.foglio_aa.mostra(ctx, "Formato", |_| ());
            return;
        };
        let formato = e.formato.clone();
        let campioni = if self.foglio_aa.aperto() { aspetti(&e.s) } else { Vec::new() };
        let mut tavolozza = self.tavolozza_aa;
        let scelto = self.foglio_aa.mostra(ctx, "Formato", |ui| contenuto(ui, &formato, &campioni, &mut tavolozza)).flatten();
        if tavolozza != self.tavolozza_aa {
            diagnosi(format!("foglio del formato: tavolozza {}", if tavolozza.is_some() { "aperta" } else { "chiusa" }));
            self.tavolozza_aa = tavolozza;
        }
        if let Some((id, valore)) = scelto
            && let Some(e) = self.editor_mut()
        {
            let st = match (id, valore["style"].as_str(), valore["ripiego"].as_str()) {
                ("para.style", Some(nome), Some(ripiego)) => e.stile(nome, ripiego),
                _ => e.comando_con_valore(id, valore),
            };
            self.dopo_ponte(st);
        }
    }
}
