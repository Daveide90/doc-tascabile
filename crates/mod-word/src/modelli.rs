//! I modelli del «Nuovo» della Home: Verbale, Relazione, Lettera, Lezione. Ognuno è un .docx
//! nostro, semplice, scritto dal motore nelle prove (`scrivi`, qui sotto) e incluso nell'app
//! (`crates/mod-word/modelli`): un titolo, i campi da riempire («Data:», «Presenti:») col nome in
//! grassetto, le sezioni coi loro titoli e gli elenchi. Nessun dato di nessuno. Si apre come un
//! documento nuovo: «Salva» chiede dove, col nome dal titolo.

use wordcraft_engine::Session;

use crate::documento::{self, ErroreApertura};

/// Un modello del «Nuovo».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modello {
    Verbale,
    Relazione,
    Lettera,
    Lezione,
}

impl Modello {
    /// Nell'ordine del foglio «Nuovo».
    pub const TUTTI: [Modello; 4] = [Modello::Verbale, Modello::Relazione, Modello::Lettera, Modello::Lezione];

    /// Il nome nel foglio e nella barra alta.
    pub fn nome(self) -> &'static str {
        match self {
            Modello::Verbale => "Verbale",
            Modello::Relazione => "Relazione",
            Modello::Lettera => "Lettera",
            Modello::Lezione => "Lezione",
        }
    }

    /// La riga sotto il nome, nel foglio.
    pub fn sotto(self) -> &'static str {
        match self {
            Modello::Verbale => "Data, presenti, ordine del giorno, decisioni",
            Modello::Relazione => "Oggetto, premessa, situazione, proposte, conclusioni",
            Modello::Lettera => "Mittente, destinatario, oggetto, saluti e firma",
            Modello::Lezione => "Corso, obiettivi, contenuti, attività, verifica",
        }
    }

    /// Il .docx incluso nell'app.
    pub fn byte(self) -> &'static [u8] {
        match self {
            Modello::Verbale => include_bytes!("../modelli/verbale.docx"),
            Modello::Relazione => include_bytes!("../modelli/relazione.docx"),
            Modello::Lettera => include_bytes!("../modelli/lettera.docx"),
            Modello::Lezione => include_bytes!("../modelli/lezione.docx"),
        }
    }

    /// Il nome del file nella cartella dei modelli.
    pub fn file(self) -> &'static str {
        match self {
            Modello::Verbale => "verbale.docx",
            Modello::Relazione => "relazione.docx",
            Modello::Lettera => "lettera.docx",
            Modello::Lezione => "lezione.docx",
        }
    }

    /// Il documento nuovo dal modello, col cursore in fondo al primo campo da riempire.
    pub fn apri(self) -> Result<Session, ErroreApertura> {
        let mut s = documento::apri_byte(self.file(), self.byte())?;
        if let Some(pos) = primo_campo(&s) {
            let _ = s.run("caret.set", &serde_json::json!({ "pos": pos }));
        }
        Ok(s)
    }
}

/// La fine del primo paragrafo che finisce coi due punti e uno spazio (il primo campo).
fn primo_campo(s: &Session) -> Option<wordcraft_engine::doc::Pos> {
    s.doc.body.iter().enumerate().find_map(|(i, b)| {
        let p = b.as_para()?;
        p.text.ends_with(": ").then(|| wordcraft_engine::doc::Pos::body(i, p.text.len()))
    })
}

/// Un paragrafo di un modello.
#[cfg(test)]
#[derive(Debug, Clone, Copy)]
enum Riga {
    /// Il titolo del documento (Titolo 1).
    Titolo(&'static str),
    /// Il titolo di una sezione (Titolo 2).
    Sezione(&'static str),
    /// Un campo: il nome in grassetto, poi uno spazio per scriverci.
    Campo(&'static str),
    /// Testo normale (vuoto: il paragrafo dove scrivere).
    Testo(&'static str),
    /// Testo normale allineato a destra (il destinatario della lettera).
    ADestra(&'static str),
    Puntato(&'static str),
    Numerato(&'static str),
}

#[cfg(test)]
impl Modello {
    fn righe(self) -> &'static [Riga] {
        use Riga::*;
        match self {
            Modello::Verbale => &[
                Titolo("Verbale di riunione"),
                Campo("Data:"),
                Campo("Ora di inizio:"),
                Campo("Luogo:"),
                Campo("Presenti:"),
                Campo("Assenti:"),
                Campo("Presiede:"),
                Campo("Verbalizza:"),
                Sezione("Ordine del giorno"),
                Numerato("Primo punto"),
                Numerato("Secondo punto"),
                Numerato("Varie ed eventuali"),
                Sezione("Discussione"),
                Testo(""),
                Sezione("Decisioni"),
                Puntato(""),
                Sezione("Prossima riunione"),
                Campo("Data e luogo:"),
                Campo("Ora di chiusura:"),
                Testo(""),
                Campo("Firma di chi presiede:"),
                Campo("Firma di chi verbalizza:"),
            ],
            Modello::Relazione => &[
                Titolo("Relazione"),
                Campo("Oggetto:"),
                Campo("Data:"),
                Campo("A cura di:"),
                Campo("Destinatari:"),
                Sezione("Premessa"),
                Testo(""),
                Sezione("Situazione"),
                Testo(""),
                Sezione("Analisi"),
                Testo(""),
                Sezione("Proposte"),
                Puntato(""),
                Sezione("Conclusioni"),
                Testo(""),
            ],
            Modello::Lettera => &[
                Campo("Mittente:"),
                Campo("Indirizzo:"),
                Testo(""),
                ADestra("Spettabile"),
                ADestra(""),
                ADestra(""),
                Testo(""),
                Campo("Luogo e data:"),
                Testo(""),
                Campo("Oggetto:"),
                Testo(""),
                Testo("Gentile"),
                Testo(""),
                Testo("Cordiali saluti."),
                Testo(""),
                Campo("Firma:"),
            ],
            Modello::Lezione => &[
                Titolo("Lezione"),
                Campo("Corso:"),
                Campo("Argomento:"),
                Campo("Docente:"),
                Campo("Data:"),
                Campo("Durata:"),
                Sezione("Obiettivi"),
                Puntato(""),
                Sezione("Contenuti"),
                Numerato(""),
                Sezione("Attività"),
                Testo(""),
                Sezione("Materiali"),
                Puntato(""),
                Sezione("Verifica dell'apprendimento"),
                Testo(""),
            ],
        }
    }

    /// Il modello scritto dal motore, da un documento nuovo in italiano: prima i paragrafi con lo
    /// stile, l'allineamento e il testo, poi gli elenchi, un gruppo di voci vicine alla volta (un
    /// Invio su una voce vuota chiuderebbe l'elenco, come in Word).
    fn scrivi(self) -> Session {
        use serde_json::json;
        use wordcraft_engine::doc::Pos;
        let mut s = documento::nuovo();
        let esegui = |s: &mut Session, id: &str, p: serde_json::Value| {
            s.run(id, &p).unwrap_or_else(|e| panic!("{id}: {e}"));
        };
        let righe = self.righe();
        for (i, riga) in righe.iter().enumerate() {
            if i > 0 {
                esegui(&mut s, "text.newParagraph", json!({}));
            }
            let (stile, testo) = match *riga {
                Riga::Titolo(t) => ("para.heading1", t),
                Riga::Sezione(t) => ("para.heading2", t),
                Riga::Campo(t) | Riga::Testo(t) | Riga::ADestra(t) | Riga::Puntato(t) | Riga::Numerato(t) => ("para.normal", t),
            };
            esegui(&mut s, stile, json!({}));
            let allinea = if matches!(riga, Riga::ADestra(_)) { "right" } else { "left" };
            esegui(&mut s, "para.align", json!({ "value": allinea }));
            if let Riga::Campo(nome) = *riga {
                let inizio = s.sel.focus.clone();
                esegui(&mut s, "text.insert", json!({ "text": nome, "raw": true }));
                let fine = s.sel.focus.clone();
                esegui(&mut s, "select.range", json!({ "anchor": inizio, "focus": fine }));
                esegui(&mut s, "format.bold", json!({}));
                esegui(&mut s, "select.collapse", json!({ "end": true }));
                esegui(&mut s, "format.bold", json!({}));
                esegui(&mut s, "text.insert", json!({ "text": " ", "raw": true }));
            } else if !testo.is_empty() {
                esegui(&mut s, "text.insert", json!({ "text": testo, "raw": true }));
            }
        }
        let elenco = |r: &Riga| match r {
            Riga::Puntato(_) => Some("para.bullets"),
            Riga::Numerato(_) => Some("para.numbering"),
            _ => None,
        };
        let mut i = 0;
        while i < righe.len() {
            let Some(comando) = elenco(&righe[i]) else {
                i += 1;
                continue;
            };
            let mut fine = i;
            while fine + 1 < righe.len() && elenco(&righe[fine + 1]) == Some(comando) {
                fine += 1;
            }
            let ultimo = s.doc.body[fine].as_para().map_or(0, |p| p.text.len());
            esegui(&mut s, "select.range", json!({ "anchor": Pos::body(i, 0), "focus": Pos::body(fine, ultimo) }));
            esegui(&mut s, comando, json!({}));
            i = fine + 1;
        }
        esegui(&mut s, "caret.set", json!({ "pos": Pos::body(0, 0) }));
        s
    }
}

#[cfg(test)]
mod test {
    use super::*;

    /// Il contenuto di un documento da confrontare: per paragrafo lo stile, l'elenco, l'allineamento,
    /// il testo e il testo in grassetto.
    fn contenuto(s: &Session) -> Vec<String> {
        s.doc
            .body
            .iter()
            .filter_map(|b| b.as_para())
            .map(|p| {
                let r = s.doc.styles.resolve_para(&p.props);
                let grassetto: String = p.run_ranges().filter(|(_, c)| c.bold == Some(true)).map(|(r, _)| &p.text[r]).collect();
                format!("{:?} {:?} {:?} «{}» «{grassetto}»", p.props.style, r.numbering.is_some(), r.align, p.text)
            })
            .collect()
    }

    /// I .docx inclusi sono quelli che il motore scrive adesso. Con TASCABILE_RIFAI_MODELLI=1 la
    /// prova li riscrive in crates/mod-word/modelli.
    #[test]
    fn i_modelli_inclusi_sono_quelli_scritti_dal_motore() {
        let cartella = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("modelli");
        for m in Modello::TUTTI {
            let scritto = m.scrivi();
            if std::env::var_os("TASCABILE_RIFAI_MODELLI").is_some() {
                let byte = wordcraft_engine::io::save_bytes(m.file(), &scritto.doc).unwrap();
                std::fs::write(cartella.join(m.file()), byte).unwrap();
                continue;
            }
            assert_eq!(scritto.doc.body.len(), m.righe().len(), "{}: un paragrafo per riga", m.nome());
            let incluso = m.apri().unwrap_or_else(|e| panic!("{}: {e}", m.nome()));
            assert_eq!(contenuto(&incluso), contenuto(&scritto), "{}: da rifare con TASCABILE_RIFAI_MODELLI=1", m.nome());
        }
    }

    #[test]
    fn un_modello_si_apre_come_un_documento_nuovo_in_italiano() {
        for m in Modello::TUTTI {
            let s = m.apri().unwrap();
            assert!(s.path.is_none() && !s.dirty && !s.can_undo(), "{}", m.nome());
            assert_eq!(s.doc.styles.default_chr.lang.as_deref(), Some("it-IT"), "{}", m.nome());
            // Nessun dato di nessuno: niente autore, date o commenti nel file.
            assert!(s.doc.core.creator.is_empty() && s.doc.core.last_modified_by.is_empty(), "{}", m.nome());
            assert!(s.doc.core.created.is_empty() && s.doc.comments.is_empty() && s.doc.revisions.is_empty(), "{}", m.nome());
            // Il cursore in fondo al primo campo, pronto per scriverci.
            let p = s.doc.para_at(&s.sel.focus).unwrap();
            assert!(p.text.ends_with(": ") && s.sel.focus.off == p.text.len(), "{}: {:?}", m.nome(), p.text);
        }
    }

    #[test]
    fn il_verbale_ha_titolo_campi_in_grassetto_e_ordine_del_giorno() {
        let s = Modello::Verbale.apri().unwrap();
        let c = contenuto(&s);
        assert!(c[0].contains("«Verbale di riunione»") && c[0].contains("Heading1"), "{}", c[0]);
        assert!(c.iter().any(|r| r.ends_with("«Presenti: » «Presenti:»")), "{c:#?}");
        assert!(c.iter().any(|r| r.contains("true") && r.contains("«Varie ed eventuali»")), "{c:#?}");
        // Le sezioni dopo un elenco non sono nell'elenco.
        let decisioni = c.iter().find(|r| r.contains("«Decisioni»")).unwrap();
        assert!(decisioni.contains("Heading2") && decisioni.contains(" false "), "{decisioni}");
        let titoli = crate::editor::titoli(&s);
        assert!(titoli.len() >= 5, "l'Indice ha le sezioni: {}", titoli.len());
    }

    #[test]
    fn nella_lettera_il_destinatario_sta_a_destra() {
        let s = Modello::Lettera.apri().unwrap();
        let c = contenuto(&s);
        let spettabile = c.iter().find(|r| r.contains("«Spettabile»")).unwrap();
        assert!(spettabile.contains("Right"), "{spettabile}");
        assert!(c.iter().filter(|r| r.contains("Right")).count() == 3, "{c:#?}");
        assert!(c.iter().any(|r| r.ends_with("«Oggetto: » «Oggetto:»")), "{c:#?}");
    }
}
