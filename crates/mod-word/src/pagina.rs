//! La pagina (piano «Word completo», W3): formato (A4, A5, Lettera), orientamento e margini
//! (normali, stretti, larghi) della sezione del cursore, coi comandi `layout.size`,
//! `layout.orientation` e `layout.margins` del motore; l'intestazione e il piè di pagina, col
//! testo e i numeri di pagina in basso (al centro o a destra, anche «Pagina N di M»), con
//! `insert.header`, `insert.footer` e i campi PAGE e NUMPAGES. Si vedono nella vista Pagina.
//! Ogni cambio è un passo di Annulla.

use serde_json::{Value, json};
use tascabile_platform::diagnosi;
use wordcraft_engine::Session;
use wordcraft_engine::doc::para::{InlineObject, OBJ};
use wordcraft_engine::doc::props::{Align, ParaProps};
use wordcraft_engine::doc::{Paragraph, StoryRef};
use wordcraft_engine::{Selection, cmd::page};

use crate::editor::Editor;
use crate::ponte::StatoIme;
use crate::salva::come_si_legge;

/// I formati del foglio «Pagina».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Formato {
    A4,
    A5,
    Lettera,
}

impl Formato {
    pub const TUTTI: [Formato; 3] = [Formato::A4, Formato::A5, Formato::Lettera];

    /// Il nome per `layout.size`.
    fn carta(self) -> &'static str {
        match self {
            Formato::A4 => "A4",
            Formato::A5 => "A5",
            Formato::Lettera => "Letter",
        }
    }

    /// Il nome da mostrare.
    pub fn nome(self) -> &'static str {
        match self {
            Formato::A4 => "A4",
            Formato::A5 => "A5",
            Formato::Lettera => "Lettera",
        }
    }

    /// I lati in punti, il corto prima (come `PAPER_SIZES` del motore).
    fn lati(self) -> (f32, f32) {
        match self {
            Formato::A4 => (595.28, 841.89),
            Formato::A5 => (419.53, 595.28),
            Formato::Lettera => (612.0, 792.0),
        }
    }

    /// Il formato di una pagina larga `w` e alta `h` punti, in tutti e due i versi, a mezzo punto.
    fn di(w: f32, h: f32) -> Option<Formato> {
        let (corto, lungo) = (w.min(h), w.max(h));
        Formato::TUTTI.into_iter().find(|f| {
            let (a, b) = f.lati();
            (a - corto).abs() < 0.5 && (b - lungo).abs() < 0.5
        })
    }
}

/// I margini del foglio «Pagina»: i preset di Word (Normali 2,54 cm, Stretti 1,27 cm, Larghi
/// 5,08 cm ai lati).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Margini {
    Normali,
    Stretti,
    Larghi,
}

impl Margini {
    pub const TUTTI: [Margini; 3] = [Margini::Normali, Margini::Stretti, Margini::Larghi];

    fn preset(self) -> &'static str {
        match self {
            Margini::Normali => "normal",
            Margini::Stretti => "narrow",
            Margini::Larghi => "wide",
        }
    }

    pub fn nome(self) -> &'static str {
        match self {
            Margini::Normali => "Normali",
            Margini::Stretti => "Stretti",
            Margini::Larghi => "Larghi",
        }
    }
}

/// La pagina della sezione del cursore.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InfoPagina {
    /// None per un formato che non è nel foglio.
    pub formato: Option<Formato>,
    pub orizzontale: bool,
    /// None per margini che non sono un preset del foglio.
    pub margini: Option<Margini>,
    /// La misura della pagina, in centimetri.
    pub larghezza_cm: f32,
    pub altezza_cm: f32,
}

/// Com'è la pagina dove sta il cursore.
pub fn info_pagina(s: &Session) -> InfoPagina {
    let x = page::sect(s);
    let margini = page::margins_preset_at(s).and_then(|p| Margini::TUTTI.into_iter().find(|m| m.preset() == p));
    let cm = |pt: f32| pt / 72.0 * 2.54;
    InfoPagina { formato: Formato::di(x.page_w, x.page_h), orizzontale: x.landscape, margini, larghezza_cm: cm(x.page_w), altezza_cm: cm(x.page_h) }
}

/// I numeri di pagina in fondo alla pagina.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NumeriPagina {
    #[default]
    Nessuno,
    AlCentro,
    ADestra,
    /// A sinistra, da un file: il foglio non li offre e non li tocca.
    ASinistra,
}

/// L'intestazione e il piè di pagina della sezione del cursore, come li mostra il foglio.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IntestazioneEPie {
    /// Il testo dell'intestazione, i paragrafi uniti da uno spazio.
    pub intestazione: String,
    /// Il testo del piè di pagina, senza il paragrafo dei numeri.
    pub pie: String,
    pub numeri: NumeriPagina,
    /// I numeri dicono anche quante sono le pagine: «Pagina 2 di 5».
    pub di_totale: bool,
    /// L'intestazione ha più di testo semplice (tabelle, immagini, campi, testo formattato): il
    /// foglio non la riscrive, perché il motore la farebbe un paragrafo di testo semplice.
    pub intestazione_ricca: bool,
    /// Lo stesso per il piè di pagina, senza contare il paragrafo dei numeri: ai numeri si
    /// aggiunge solo un paragrafo in fondo.
    pub pie_ricco: bool,
}

/// Il campo di un oggetto, maiuscolo e senza spazi intorno («PAGE», «NUMPAGES»…).
fn campo(o: &InlineObject) -> Option<String> {
    match o {
        InlineObject::Field { instr, .. } => instr.split_whitespace().next().map(str::to_ascii_uppercase),
        _ => None,
    }
}

/// Il testo dei paragrafi `p` come si legge, senza oggetti e campi, uniti da uno spazio.
fn testo_di<'a>(s: &Session, p: impl Iterator<Item = &'a Paragraph>) -> String {
    let righe: Vec<String> = p.map(|p| come_si_legge(&s.doc, p).replace(OBJ, "").trim().to_string()).filter(|t| !t.is_empty()).collect();
    righe.join(" ")
}

/// La storia `id` ha più di paragrafi di testo semplice: una tabella, un oggetto (immagine, campo
/// che non è il numero di pagina), testo o paragrafi formattati (lo stile del paragrafo non conta).
/// Il paragrafo dei numeri di pagina non conta.
fn ricca(s: &Session, id: Option<u32>) -> bool {
    let Some(parte) = id.and_then(|id| s.doc.parts.get(&id)) else { return false };
    parte.blocks.iter().any(|b| match b.as_para() {
        None => true,
        Some(p) if p.objects.iter().any(|o| campo(o).as_deref() == Some("PAGE")) => false,
        Some(p) => !p.objects.is_empty() || p.runs.iter().any(|r| !r.props.is_empty()) || !ParaProps { style: None, ..p.props.clone() }.is_empty(),
    })
}

/// L'intestazione e il piè di pagina della sezione del cursore (quelli di tutte le pagine, o della
/// prima se la sezione ha solo quelli).
pub fn intestazione_e_pie(s: &Session) -> IntestazioneEPie {
    let x = page::sect(s);
    let paragrafi = |id: Option<u32>| -> Vec<&Paragraph> {
        id.and_then(|id| s.doc.parts.get(&id)).map(|p| p.blocks.iter().filter_map(|b| b.as_para()).collect()).unwrap_or_default()
    };
    let (id_intestazione, id_pie) = (x.headers.default.or(x.headers.first), x.footers.default.or(x.footers.first));
    let intestazione = paragrafi(id_intestazione);
    let pie = paragrafi(id_pie);
    let con_numero = |p: &&Paragraph| p.objects.iter().any(|o| campo(o).as_deref() == Some("PAGE"));
    let numeri = pie.iter().find(|p| con_numero(p));
    let (numeri, di_totale) = match numeri {
        None => (NumeriPagina::Nessuno, false),
        Some(p) => {
            let rp = s.doc.styles.resolve_para(&p.props);
            let dove = match rp.align.visual(rp.bidi) {
                Align::Center => NumeriPagina::AlCentro,
                Align::Right => NumeriPagina::ADestra,
                _ => NumeriPagina::ASinistra,
            };
            (dove, p.objects.iter().any(|o| campo(o).as_deref() == Some("NUMPAGES")))
        }
    };
    IntestazioneEPie {
        intestazione: testo_di(s, intestazione.into_iter()),
        pie: testo_di(s, pie.into_iter().filter(|p| !con_numero(p))),
        numeri,
        di_totale,
        intestazione_ricca: ricca(s, id_intestazione),
        pie_ricco: ricca(s, id_pie),
    }
}

impl Editor {
    /// Il formato della pagina (`layout.size`, l'orientamento resta).
    pub fn formato_pagina(&mut self, f: Formato) -> Option<StatoIme> {
        self.comando_con("layout.size", json!({ "name": f.carta() }), &format!("pagina: formato {}", f.nome()))
    }

    /// La pagina in orizzontale o in verticale (`layout.orientation`).
    pub fn orientamento_pagina(&mut self, orizzontale: bool) -> Option<StatoIme> {
        let valore = if orizzontale { "landscape" } else { "portrait" };
        self.comando_con(
            "layout.orientation",
            json!({ "value": valore }),
            &format!("pagina: {}", if orizzontale { "orizzontale" } else { "verticale" }),
        )
    }

    /// I margini (`layout.margins` col preset).
    pub fn margini_pagina(&mut self, m: Margini) -> Option<StatoIme> {
        self.comando_con("layout.margins", json!({ "preset": m.preset() }), &format!("pagina: margini {}", m.nome().to_lowercase()))
    }

    /// Un comando del motore nel passo di Annulla di quelli prima (`primo` falso), col risultato.
    fn passo_pagina(&mut self, primo: &mut bool, id: &str, params: Value) -> Option<Value> {
        if !*primo {
            self.s.join_next_undo();
        }
        match self.s.run(id, &params) {
            Ok(v) => {
                *primo = false;
                Some(v)
            }
            Err(e) => {
                log::warn!("{id}: {}", crate::ponte::genere(&e));
                diagnosi(format!("comando: {id}, non riuscito ({})", crate::ponte::genere(&e)));
                None
            }
        }
    }

    /// L'intestazione e il piè di pagina come `nuovo`, in un passo di Annulla solo: si riscrive
    /// solo quello che è cambiato (un piè di pagina di un file resta com'è se non lo si tocca), e
    /// mai una storia ricca (`intestazione_ricca`, `pie_ricco`): il suo testo resta com'è, e ai
    /// numeri si aggiunge solo un paragrafo in fondo (analisi del 27/10, critico 5).
    /// L'intestazione è il suo testo (`insert.header`); il piè di pagina il suo testo
    /// (`insert.footer`) e sotto, se ci sono, i numeri di pagina in un paragrafo al centro o a
    /// destra, col campo PAGE e, con `di_totale`, «Pagina N di M» col campo NUMPAGES. Il cursore
    /// torna dov'era. Nella diagnosi solo quanto sono lunghi i testi e dove vanno i numeri.
    pub fn cambia_intestazione_e_pie(&mut self, nuovo: &IntestazioneEPie) -> Option<StatoIme> {
        let prima = intestazione_e_pie(&self.s);
        let intestazione = nuovo.intestazione.trim();
        let pie = nuovo.pie.trim();
        let cambia_intestazione = intestazione != prima.intestazione && !prima.intestazione_ricca;
        let cambia_numeri = nuovo.numeri != prima.numeri || (nuovo.numeri != NumeriPagina::Nessuno && nuovo.di_totale != prima.di_totale);
        let riscrivi_pie = !prima.pie_ricco && (pie != prima.pie || cambia_numeri);
        let aggiungi_numeri = prima.pie_ricco && prima.numeri == NumeriPagina::Nessuno && nuovo.numeri != NumeriPagina::Nessuno;
        if !cambia_intestazione && !riscrivi_pie && !aggiungi_numeri {
            return None;
        }
        let sel = self.s.sel.clone();
        let mut primo = true;
        if cambia_intestazione {
            self.passo_pagina(&mut primo, "insert.header", json!({ "text": intestazione }));
        }
        // La storia dei numeri e se vogliono un paragrafo nuovo: quella appena riscritta, o il piè
        // di pagina ricco com'è (nel suo ultimo paragrafo se è vuoto, come dopo una tabella).
        let storia = if riscrivi_pie {
            self.passo_pagina(&mut primo, "insert.footer", json!({ "text": pie }))
                .and_then(|v| v.get("story").and_then(Value::as_u64))
                .map(|id| (id as u32, !pie.is_empty()))
        } else if aggiungi_numeri {
            let x = page::sect(&self.s);
            x.footers.default.or(x.footers.first).map(|id| {
                let ultimo = self.s.doc.parts.get(&id).and_then(|p| p.blocks.last()).and_then(|b| b.as_para());
                (id, !ultimo.is_some_and(|p| p.text.is_empty() && p.objects.is_empty()))
            })
        } else {
            None
        };
        let allinea = match nuovo.numeri {
            NumeriPagina::Nessuno => None,
            NumeriPagina::AlCentro => Some("para.alignCenter"),
            NumeriPagina::ADestra => Some("para.alignRight"),
            NumeriPagina::ASinistra => Some("para.alignLeft"),
        };
        if let (Some(allinea), Some((id, a_capo))) = (allinea, storia) {
            let fine = self.s.doc.end_of(StoryRef::Part(id));
            self.passo_pagina(&mut primo, "caret.set", json!({ "pos": fine }));
            if a_capo {
                self.passo_pagina(&mut primo, "text.newParagraph", json!({}));
            }
            self.passo_pagina(&mut primo, allinea, json!({}));
            let pagina = json!({ "instr": "PAGE", "result": "1" });
            if nuovo.di_totale {
                self.passo_pagina(&mut primo, "text.insert", json!({ "text": "Pagina ", "raw": true }));
                self.passo_pagina(&mut primo, "insert.field", pagina);
                self.passo_pagina(&mut primo, "text.insert", json!({ "text": " di ", "raw": true }));
                self.passo_pagina(&mut primo, "insert.field", json!({ "instr": "NUMPAGES", "result": "1" }));
            } else {
                self.passo_pagina(&mut primo, "insert.field", pagina);
            }
        }
        self.s.sel = Selection { anchor: self.s.doc.clamp(&sel.anchor), focus: self.s.doc.clamp(&sel.focus) };
        let numeri = match nuovo.numeri {
            NumeriPagina::Nessuno => "senza numeri",
            NumeriPagina::AlCentro => "numeri in basso al centro",
            NumeriPagina::ADestra => "numeri in basso a destra",
            NumeriPagina::ASinistra => "numeri in basso a sinistra",
        };
        let totale = if nuovo.numeri != NumeriPagina::Nessuno && nuovo.di_totale { ", con il totale" } else { "" };
        diagnosi(format!(
            "pagina: intestazione di {} caratteri, piè di pagina di {} caratteri, {numeri}{totale}",
            intestazione.chars().count(),
            pie.chars().count()
        ));
        self.dopo_selezione()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    /// Un .docx con un piè di pagina da carta intestata: «ASL Roma 1» in grassetto, al centro, a 8
    /// punti, e sotto una tabella con indirizzo e telefono.
    fn carta_intestata() -> Session {
        let pie = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><w:ftr xmlns:w="{}"><w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:rPr><w:b/><w:sz w:val="16"/></w:rPr><w:t>ASL Roma 1</w:t></w:r></w:p><w:tbl><w:tblPr><w:tblW w:w="5000" w:type="pct"/></w:tblPr><w:tblGrid><w:gridCol w:w="4800"/><w:gridCol w:w="4800"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>Via Roma 1</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Tel 06 123</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/></w:ftr>"#,
            crate::prova::W
        );
        let tipo = "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml";
        let corpo = r#"<w:p><w:r><w:t>Gentile dottore</w:t></w:r></w:p>"#;
        let byte = crate::prova::docx(corpo, r#"<w:footerReference w:type="default" r:id="rIdP0"/>"#, &[("footer1.xml", "footer", tipo, &pie)]);
        crate::documento::apri_byte("lettera.docx", &byte).unwrap()
    }

    fn pie_salvato(s: &Session) -> String {
        crate::prova::xml_delle_parti(&wordcraft_engine::io::save_bytes("lettera.docx", &s.doc).unwrap(), "footer")
    }

    #[test]
    fn i_numeri_di_pagina_si_aggiungono_senza_toccare_il_resto_del_pie_di_pagina() {
        // Analisi del 27/10, critico 5: Applica riscriveva tutto il piè di pagina come testo
        // semplice, e una carta intestata perdeva la tabella e il formato.
        let mut e = Editor::new(carta_intestata(), "lettera.docx");
        let prima = intestazione_e_pie(&e.s);
        assert!(prima.pie_ricco && !prima.intestazione_ricca, "{prima:?}");
        e.cambia_intestazione_e_pie(&IntestazioneEPie { numeri: NumeriPagina::AlCentro, ..prima.clone() });
        let xml = pie_salvato(&e.s);
        assert!(xml.contains("<w:tbl>") && xml.contains("Tel 06 123"), "è sparita la tabella: {xml}");
        assert!(xml.contains("<w:b/>"), "è sparito il grassetto: {xml}");
        assert_eq!(intestazione_e_pie(&e.s).numeri, NumeriPagina::AlCentro);
        // Il testo di un piè di pagina ricco non si riscrive dal foglio: la tabella resta.
        e.cambia_intestazione_e_pie(&IntestazioneEPie { pie: "Altro".into(), ..intestazione_e_pie(&e.s) });
        assert!(pie_salvato(&e.s).contains("Tel 06 123"));
        // Un piè di pagina di solo testo invece sì, coi numeri sotto.
        let mut e = Editor::new(documento(&["Lettera"]), "lettera.docx");
        e.cambia_intestazione_e_pie(&IntestazioneEPie { pie: "ASL Roma 1".into(), numeri: NumeriPagina::ADestra, ..Default::default() });
        let ip = intestazione_e_pie(&e.s);
        assert_eq!((ip.pie.as_str(), ip.numeri, ip.pie_ricco), ("ASL Roma 1", NumeriPagina::ADestra, false));
    }
    use crate::editor::Inserimento;
    use crate::prova::documento;
    use wordcraft_engine::doc::Pos;
    use wordcraft_engine::layout::{Page, Placed};

    /// I campi come li mostra la pagina (`shown` dei paragrafi), in quest'ordine.
    fn campi_mostrati(items: &[Placed]) -> Vec<String> {
        items
            .iter()
            .flat_map(|it| if let Placed::Lines { para, .. } = it { para.shown.iter().map(|s| s.1.clone()).collect() } else { vec![] })
            .collect()
    }

    /// Le righe di testo di intestazione o piè di pagina della pagina.
    fn righe(items: &[Placed]) -> usize {
        items.iter().filter(|it| matches!(it, Placed::Lines { .. })).count()
    }

    /// Un documento lungo due pagine (l'interruzione di pagina dopo il primo paragrafo), col
    /// cursore nel secondo.
    fn due_pagine() -> Editor {
        let mut e = Editor::new(documento(&["Relazione", "Seconda pagina"]), "prova");
        e.s.run("caret.set", &json!({ "pos": Pos::body(0, 9) })).unwrap();
        e.inserisci(Inserimento::InterruzionePagina);
        e.s.run("caret.set", &json!({ "pos": Pos::body(1, 3) })).unwrap();
        e.s.reset_history();
        e
    }

    fn pagine(e: &mut Editor) -> Vec<Page> {
        crate::vista::layout_pagine(&mut e.s).pages.clone()
    }

    #[test]
    fn l_interruzione_di_pagina_fa_due_pagine_nella_vista_pagina() {
        let mut e = due_pagine();
        let p = pagine(&mut e);
        assert_eq!(p.len(), 2);
        let l = crate::vista::layout_pagine(&mut e.s);
        assert_eq!(l.caret(&Pos::body(1, 0)).map(|c| c.page), Some(1), "il testo dopo l'interruzione è sulla seconda pagina");
    }

    #[test]
    fn in_scrittura_il_testo_dopo_l_interruzione_va_a_capo() {
        let mut e = Editor::new(documento(&["Prima Dopo"]), "prova");
        e.s.run("caret.set", &json!({ "pos": Pos::body(0, 6) })).unwrap();
        e.inserisci(Inserimento::InterruzionePagina);
        let l = crate::vista::layout_scrittura(&mut e.s, 392.0, 1.0);
        let prima = l.caret_on(&Pos::body(0, 0), 0).unwrap();
        let dopo = l.caret_on(&e.s.sel.focus, 0).unwrap();
        assert!(dopo.top > prima.top + prima.height * 0.5, "«Dopo» sulla stessa riga di «Prima»");
    }

    #[test]
    fn formato_orientamento_e_margini() {
        let mut e = due_pagine();
        let i = info_pagina(&e.s);
        assert_eq!((i.formato, i.orizzontale, i.margini), (Some(Formato::A4), false, Some(Margini::Normali)), "{i:?}");
        e.formato_pagina(Formato::A5);
        assert_eq!(info_pagina(&e.s).formato, Some(Formato::A5));
        e.formato_pagina(Formato::Lettera);
        e.formato_pagina(Formato::A4);
        e.orientamento_pagina(true);
        let i = info_pagina(&e.s);
        assert_eq!((i.formato, i.orizzontale), (Some(Formato::A4), true));
        assert!((i.larghezza_cm - 29.7).abs() < 0.05 && (i.altezza_cm - 21.0).abs() < 0.05, "{i:?}");
        let p = pagine(&mut e);
        assert!(p[0].w > p[0].h, "la pagina è orizzontale: {} per {}", p[0].w, p[0].h);
        e.margini_pagina(Margini::Stretti);
        assert_eq!(info_pagina(&e.s).margini, Some(Margini::Stretti));
        assert!((page::sect(&e.s).margin_left - 36.0).abs() < 0.1);
        e.margini_pagina(Margini::Larghi);
        assert_eq!(info_pagina(&e.s).margini, Some(Margini::Larghi));
        // Un passo di Annulla per volta.
        e.comando("edit.undo");
        assert_eq!(info_pagina(&e.s).margini, Some(Margini::Stretti));
        e.comando("edit.undo");
        e.comando("edit.undo");
        assert!(!info_pagina(&e.s).orizzontale);
        let d = tascabile_platform::diagnosi_testo();
        for riga in ["] pagina: formato A5\n", "] pagina: orizzontale\n", "] pagina: margini stretti\n"] {
            assert!(d.contains(riga), "manca «{riga}»:\n{d}");
        }
    }

    #[test]
    fn intestazione_pie_di_pagina_e_numeri_in_un_passo() {
        let mut e = due_pagine();
        assert_eq!(intestazione_e_pie(&e.s), IntestazioneEPie::default());
        let cursore = e.s.sel.clone();
        let nuovo = IntestazioneEPie {
            intestazione: "Reparto di medicina".into(),
            pie: "Riservato".into(),
            numeri: NumeriPagina::AlCentro,
            di_totale: true,
            ..Default::default()
        };
        e.cambia_intestazione_e_pie(&nuovo);
        assert_eq!(intestazione_e_pie(&e.s), nuovo, "si rilegge com'è stato scritto");
        assert_eq!(e.s.sel, cursore, "il cursore torna nel testo, dov'era");
        // Nella vista Pagina: intestazione e piè di pagina su tutte e due le pagine, coi numeri.
        let p = pagine(&mut e);
        for (i, pagina) in p.iter().enumerate() {
            assert!(righe(&pagina.header) >= 1, "pagina {}: niente intestazione", i + 1);
            assert_eq!(righe(&pagina.footer), 2, "pagina {}: il testo e i numeri", i + 1);
            assert_eq!(campi_mostrati(&pagina.footer), [format!("{}", i + 1), "2".to_string()], "pagina {}", i + 1);
        }
        // Un Annulla solo toglie tutto.
        e.comando("edit.undo");
        assert_eq!(intestazione_e_pie(&e.s), IntestazioneEPie::default());
        e.comando("edit.redo");
        // I numeri a destra e senza il totale; il testo del piè di pagina tolto.
        let destra = IntestazioneEPie { pie: String::new(), numeri: NumeriPagina::ADestra, di_totale: false, ..nuovo.clone() };
        e.cambia_intestazione_e_pie(&destra);
        assert_eq!(intestazione_e_pie(&e.s), destra);
        let p = pagine(&mut e);
        assert_eq!((righe(&p[1].footer), campi_mostrati(&p[1].footer)), (1, vec!["2".to_string()]));
        // Niente da cambiare: nessun passo di Annulla.
        let passi = e.s.undo_depth();
        assert!(e.cambia_intestazione_e_pie(&destra).is_none());
        assert_eq!(e.s.undo_depth(), passi);
        let d = tascabile_platform::diagnosi_testo();
        assert!(
            d.contains("] pagina: intestazione di 19 caratteri, piè di pagina di 9 caratteri, numeri in basso al centro, con il totale\n"),
            "{d}"
        );
        assert!(!d.contains("Reparto") && !d.contains("Riservato"), "{d}");
    }

    #[test]
    fn salvata_in_docx_e_riaperta() {
        let mut e = due_pagine();
        e.orientamento_pagina(true);
        e.margini_pagina(Margini::Stretti);
        let nuovo = IntestazioneEPie {
            intestazione: "Ambulatorio".into(),
            pie: "Foglio interno".into(),
            numeri: NumeriPagina::ADestra,
            di_totale: true,
            ..Default::default()
        };
        e.cambia_intestazione_e_pie(&nuovo);
        let byte = wordcraft_engine::io::save_bytes("pagina.docx", &e.s.doc).expect("salvata");
        let mut s = crate::documento::apri_byte("pagina.docx", &byte).expect("riaperta");
        assert_eq!(intestazione_e_pie(&s), nuovo);
        let i = info_pagina(&s);
        assert_eq!((i.formato, i.orizzontale, i.margini), (Some(Formato::A4), true, Some(Margini::Stretti)));
        let l = crate::vista::layout_pagine(&mut s);
        assert_eq!(campi_mostrati(&l.pages[0].footer), ["1", "2"]);
    }
}
