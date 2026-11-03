//! Commenti e revisioni (piano «Word completo», W4), coi comandi `review.*` del motore: le
//! modifiche tenute a traccia (aggiunte, tolte, di formato, spostate) da accettare o rifiutare, una
//! alla volta o tutte; i commenti con le risposte, da scrivere su una selezione, rispondere,
//! risolvere, eliminare. L'autore è quello della sessione (`Session::author`), che l'app prende
//! dalle Impostazioni. Nella diagnosi solo conti e comandi, mai un testo o un nome.

use serde_json::{Value, json};
use tascabile_platform::diagnosi;
use wordcraft_engine::Session;
use wordcraft_engine::doc::Pos;
use wordcraft_engine::doc::para::{InlineObject, OBJ};

use crate::editor::Editor;
use crate::ponte::{StatoIme, genere};

/// Quanti caratteri di una modifica o di un commento si mostrano nel foglio, al più.
pub const ESTRATTO: usize = 120;

/// Di che genere è una modifica tenuta a traccia.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenereModifica {
    Aggiunto,
    Tolto,
    Formato,
    /// Il testo spostato, dove stava.
    SpostatoDa,
    /// Il testo spostato, dove sta adesso.
    SpostatoQui,
}

impl GenereModifica {
    /// Il nome nel foglio.
    pub fn nome(self) -> &'static str {
        match self {
            GenereModifica::Aggiunto => "Aggiunto",
            GenereModifica::Tolto => "Tolto",
            GenereModifica::Formato => "Formato cambiato",
            GenereModifica::SpostatoDa => "Spostato da qui",
            GenereModifica::SpostatoQui => "Spostato qui",
        }
    }
}

/// Una modifica tenuta a traccia.
#[derive(Debug, Clone, PartialEq)]
pub struct Modifica {
    pub genere: GenereModifica,
    pub inizio: Pos,
    pub fine: Pos,
    /// Il testo della modifica, al più `ESTRATTO` caratteri.
    pub testo: String,
    pub autore: String,
}

/// Una risposta a un commento.
#[derive(Debug, Clone, PartialEq)]
pub struct Risposta {
    pub id: u32,
    pub autore: String,
    pub data: String,
    pub testo: String,
}

/// Un commento con le sue risposte (anche le risposte alle risposte, in ordine).
#[derive(Debug, Clone, PartialEq)]
pub struct Commento {
    pub id: u32,
    pub autore: String,
    pub data: String,
    pub testo: String,
    pub risolto: bool,
    /// Dove comincia il testo commentato.
    pub ancora: Option<Pos>,
    pub risposte: Vec<Risposta>,
}

/// Tutto quello che il foglio «Revisioni» mostra.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StatoRevisioni {
    /// «Tieni traccia delle modifiche».
    pub traccia: bool,
    pub modifiche: Vec<Modifica>,
    pub commenti: Vec<Commento>,
}

/// Il testo al più lungo `ESTRATTO` caratteri, sulla stessa riga, coi puntini se è tagliato.
fn estratto(testo: &str) -> String {
    let riga: String = testo.split_whitespace().collect::<Vec<_>>().join(" ");
    if riga.chars().count() <= ESTRATTO {
        return riga;
    }
    let mut corto: String = riga.chars().take(ESTRATTO).collect();
    corto.push('…');
    corto
}

fn stringa(v: &Value, chiave: &str) -> String {
    v.get(chiave).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn posizione(v: &Value, chiave: &str) -> Option<Pos> {
    serde_json::from_value(v.get(chiave)?.clone()).ok()
}

/// Le modifiche e i commenti del documento, in ordine di lettura.
pub fn stato(s: &mut Session) -> StatoRevisioni {
    let lista = |s: &mut Session, id: &str| match s.run(id, &json!({})) {
        Ok(Value::Array(v)) => v,
        _ => Vec::new(),
    };
    let modifiche = lista(s, "review.changes")
        .iter()
        .filter_map(|v| {
            let genere = match v.get("kind").and_then(Value::as_str)? {
                "insert" => GenereModifica::Aggiunto,
                "delete" => GenereModifica::Tolto,
                "format" => GenereModifica::Formato,
                "moveFrom" => GenereModifica::SpostatoDa,
                "moveTo" => GenereModifica::SpostatoQui,
                _ => return None,
            };
            let testo = if genere == GenereModifica::Formato { stringa(v, "description") } else { stringa(v, "text") };
            Some(Modifica {
                genere,
                inizio: posizione(v, "start")?,
                fine: posizione(v, "end")?,
                testo: estratto(&testo),
                autore: stringa(v, "author"),
            })
        })
        .collect();
    let tutti = lista(s, "review.comments");
    // Le risposte vanno sotto il commento da cui parte la loro catena (al più 64 passi: un file
    // ostile con un giro di risposte non blocca niente).
    let padre = |id: u64| tutti.iter().find(|c| c.get("id").and_then(Value::as_u64) == Some(id)).and_then(|c| c.get("parent")?.as_u64());
    let radice = |mut id: u64| {
        for _ in 0..64 {
            match padre(id) {
                Some(p) => id = p,
                None => break,
            }
        }
        id
    };
    let mut commenti: Vec<Commento> = Vec::new();
    for c in tutti.iter().filter(|c| c.get("parent").is_none_or(Value::is_null)) {
        let Some(id) = c.get("id").and_then(Value::as_u64) else { continue };
        commenti.push(Commento {
            id: id as u32,
            autore: stringa(c, "author"),
            data: stringa(c, "date"),
            testo: estratto(&stringa(c, "text")),
            risolto: c.get("resolved").and_then(Value::as_bool).unwrap_or(false),
            ancora: posizione(c, "anchor"),
            risposte: Vec::new(),
        });
    }
    for r in tutti.iter().filter(|c| c.get("parent").is_some_and(|p| !p.is_null())) {
        let Some(id) = r.get("id").and_then(Value::as_u64) else { continue };
        let capo = radice(id);
        if let Some(c) = commenti.iter_mut().find(|c| u64::from(c.id) == capo) {
            c.risposte.push(Risposta { id: id as u32, autore: stringa(r, "author"), data: stringa(r, "date"), testo: estratto(&stringa(r, "text")) });
        }
    }
    StatoRevisioni { traccia: s.doc.settings.track_changes, modifiche, commenti }
}

/// Il giorno di un commento all'italiana, «26/10/2026», dall'ISO 8601 del file; vuoto se non si
/// legge. Solo il giorno: l'ora del file è ora UTC (il motore, Word con `dateUtc`) ora locale
/// segnata come UTC (Word col solo `w:date`), e non si sa quale.
pub fn data_breve(iso: &str) -> String {
    let giorno = iso.split_once('T').map_or(iso, |(g, _)| g);
    let mut parti = giorno.split('-');
    let (Some(a), Some(m), Some(g), None) = (parti.next(), parti.next(), parti.next(), parti.next()) else { return String::new() };
    let cifre = |t: &str, n: usize| t.len() == n && t.chars().all(|c| c.is_ascii_digit());
    if cifre(a, 4) && cifre(m, 2) && cifre(g, 2) { format!("{g}/{m}/{a}") } else { String::new() }
}

impl Editor {
    /// «Tieni traccia delle modifiche» acceso o spento (resta nel documento, come in Word).
    pub fn traccia_modifiche(&mut self, acceso: bool) -> Option<StatoIme> {
        let riga = if acceso { "revisioni: traccia accesa" } else { "revisioni: traccia spenta" };
        self.comando_con("review.trackChanges", json!({ "value": acceso }), riga)
    }

    /// Accetta (o rifiuta) la modifica `m`, in un passo di Annulla.
    pub fn risolvi_modifica(&mut self, m: &Modifica, accetta: bool) -> Option<StatoIme> {
        self.esegui("select.range", json!({ "anchor": m.inizio, "focus": m.fine }));
        let (id, riga) =
            if accetta { ("review.accept", "revisioni: modifica accettata") } else { ("review.reject", "revisioni: modifica rifiutata") };
        self.comando_con(id, json!({}), riga)
    }

    /// Accetta (o rifiuta) tutte le modifiche, in un passo di Annulla.
    pub fn risolvi_tutte(&mut self, accetta: bool) -> Option<StatoIme> {
        let n = stato(&mut self.s).modifiche.len();
        let (id, verbo) = if accetta { ("review.acceptAll", "accettate") } else { ("review.rejectAll", "rifiutate") };
        self.comando_con(id, json!({}), &format!("revisioni: {n} modifiche {verbo}"))
    }

    /// Un commento sulla selezione (o sulla parola del cursore) col testo `testo`, in un passo di
    /// Annulla; il cursore va subito dopo il testo commentato.
    pub fn commenta(&mut self, testo: &str) -> Option<StatoIme> {
        let fine = self.s.sel.ordered().1;
        let id = match self.s.run("review.newComment", &json!({ "text": testo })) {
            Ok(v) => {
                diagnosi("revisioni: commento scritto");
                v.get("id").and_then(Value::as_u64)
            }
            Err(e) => {
                diagnosi(format!("comando: review.newComment, non riuscito ({})", genere(&e)));
                None
            }
        };
        // La fine del commento sta nel paragrafo della fine della selezione: il cursore dopo di lei.
        let dopo = id.and_then(|id| {
            let p = self.s.doc.para_at(&fine)?;
            let o = p
                .object_offsets()
                .into_iter()
                .find(|o| matches!(p.object_at(*o), Some(InlineObject::CommentEnd { id: x }) if u64::from(*x) == id))?;
            Some(Pos { off: o + OBJ.len_utf8(), ..fine.clone() })
        });
        match dopo {
            Some(pos) => {
                self.esegui("caret.set", json!({ "pos": pos }));
            }
            None => {
                self.esegui("select.collapse", json!({ "end": true }));
            }
        }
        self.vista.scorri_al_cursore();
        self.dopo_selezione()
    }

    /// Una risposta al commento `id`.
    pub fn rispondi(&mut self, id: u32, testo: &str) -> Option<StatoIme> {
        self.comando_con("review.reply", json!({ "id": id, "text": testo }), "revisioni: risposta scritta")
    }

    /// Il commento `id` risolto (o riaperto), con le risposte.
    pub fn risolvi_commento(&mut self, id: u32, risolto: bool) -> Option<StatoIme> {
        let riga = if risolto { "revisioni: commento risolto" } else { "revisioni: commento riaperto" };
        self.comando_con("review.resolveComment", json!({ "id": id, "value": risolto }), riga)
    }

    /// Il commento `id` eliminato, con le sue risposte.
    pub fn elimina_commento(&mut self, id: u32) -> Option<StatoIme> {
        self.comando_con("review.deleteComment", json!({ "id": id }), "revisioni: commento eliminato")
    }

    /// Il cursore dove comincia il commento o la modifica (dal foglio «Revisioni»).
    pub fn vai_a(&mut self, pos: &Pos) -> Option<StatoIme> {
        diagnosi("revisioni: salto nel testo");
        self.vai_al_titolo(pos)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::prova::{documento, testi};

    /// Il testo che si legge (senza quello tolto con le revisioni).
    fn come_si_legge(s: &Session) -> String {
        s.doc.body.iter().filter_map(|b| b.as_para()).map(|p| crate::salva::come_si_legge(&s.doc, p)).collect::<Vec<_>>().join("\n")
    }

    #[test]
    fn le_modifiche_tenute_a_traccia_si_accettano_e_si_rifiutano() {
        let mut e = Editor::new(documento(&["Ciao mondo"]), "prova");
        e.s.author = "Anna".into();
        assert!(!stato(&mut e.s).traccia);
        e.traccia_modifiche(true);
        assert!(stato(&mut e.s).traccia);
        e.s.run("caret.set", &json!({ "pos": Pos::body(0, 10) })).unwrap();
        e.s.run("text.insert", &json!({ "text": " bello", "raw": true })).unwrap();
        e.s.run("select.range", &json!({ "anchor": Pos::body(0, 0), "focus": Pos::body(0, 5) })).unwrap();
        e.s.run("text.delete", &json!({})).unwrap();
        let st = stato(&mut e.s);
        let generi: Vec<(GenereModifica, &str, &str)> = st.modifiche.iter().map(|m| (m.genere, m.testo.as_str(), m.autore.as_str())).collect();
        assert_eq!(generi, [(GenereModifica::Tolto, "Ciao", "Anna"), (GenereModifica::Aggiunto, "bello", "Anna")], "{:?}", st.modifiche);
        // Rifiutata la cancellazione, «Ciao» torna; accettata l'aggiunta, «bello» resta senza segni.
        let tolta = st.modifiche[0].clone();
        e.risolvi_modifica(&tolta, false);
        assert_eq!(come_si_legge(&e.s), "Ciao mondo bello");
        let aggiunta = stato(&mut e.s).modifiche[0].clone();
        e.risolvi_modifica(&aggiunta, true);
        assert!(stato(&mut e.s).modifiche.is_empty());
        assert_eq!(testi(&e.s), ["Ciao mondo bello"]);
        // Annulla rimette la modifica.
        e.comando("edit.undo");
        assert_eq!(stato(&mut e.s).modifiche.len(), 1);
        // Tutte insieme, in un passo.
        e.s.run("select.range", &json!({ "anchor": Pos::body(0, 0), "focus": Pos::body(0, 5) })).unwrap();
        e.s.run("text.delete", &json!({})).unwrap();
        assert_eq!(stato(&mut e.s).modifiche.len(), 2);
        e.risolvi_tutte(false);
        assert_eq!((come_si_legge(&e.s).as_str(), stato(&mut e.s).modifiche.len()), ("Ciao mondo", 0));
        e.comando("edit.undo");
        assert_eq!(stato(&mut e.s).modifiche.len(), 2);
        e.risolvi_tutte(true);
        assert_eq!(testi(&e.s), ["mondo bello"]);
        let d = tascabile_platform::diagnosi_testo();
        assert!(d.contains("] revisioni: modifica rifiutata\n") && d.contains("] revisioni: 2 modifiche accettate\n"), "{d}");
        assert!(!d.contains("bello") && !d.contains("Anna"), "{d}");
    }

    #[test]
    fn i_commenti_si_scrivono_rispondono_risolvono_ed_eliminano() {
        let mut e = Editor::new(documento(&["Il letto 12 va rivalutato domani"]), "prova");
        e.s.author = "Anna".into();
        e.s.run("select.range", &json!({ "anchor": Pos::body(0, 3), "focus": Pos::body(0, 11) })).unwrap();
        e.commenta("Quale letto?");
        assert!(e.s.sel.is_collapsed(), "la selezione si chiude dopo il commento");
        // Il cursore subito dopo «letto 12» e la fine del commento: due segnaposto di 3 byte prima.
        assert_eq!(e.s.sel.focus, Pos::body(0, 11 + 6), "{:?}", e.s.doc.body[0].as_para().unwrap().text);
        e.comando("edit.undo");
        assert!(e.s.doc.comments.is_empty(), "un passo di Annulla");
        e.comando("edit.redo");
        let st = stato(&mut e.s);
        assert_eq!(st.commenti.len(), 1);
        let c = &st.commenti[0];
        assert_eq!((c.autore.as_str(), c.testo.as_str(), c.risolto), ("Anna", "Quale letto?", false));
        assert_eq!(c.ancora.as_ref().map(|p| (p.path.0.clone(), p.off)), Some((vec![0], 3)));
        assert!(!data_breve(&c.data).is_empty(), "{}", c.data);
        let id = c.id;
        e.s.author = "Bruno".into();
        e.rispondi(id, "Il 12, vicino alla finestra");
        e.rispondi(id, "Grazie");
        let st = stato(&mut e.s);
        assert_eq!(st.commenti.len(), 1, "le risposte stanno sotto il commento");
        let r: Vec<(&str, &str)> = st.commenti[0].risposte.iter().map(|r| (r.autore.as_str(), r.testo.as_str())).collect();
        assert_eq!(r, [("Bruno", "Il 12, vicino alla finestra"), ("Bruno", "Grazie")]);
        e.risolvi_commento(id, true);
        assert!(stato(&mut e.s).commenti[0].risolto);
        e.comando("edit.undo");
        assert!(!stato(&mut e.s).commenti[0].risolto, "Annulla riapre il commento");
        e.elimina_commento(id);
        assert!(stato(&mut e.s).commenti.is_empty() && e.s.doc.comments.is_empty(), "via anche le risposte");
        assert_eq!(come_si_legge(&e.s), "Il letto 12 va rivalutato domani");
        let d = tascabile_platform::diagnosi_testo();
        assert!(d.contains("] revisioni: commento scritto\n") && d.contains("] revisioni: risposta scritta\n"), "{d}");
        assert!(!d.contains("Quale letto") && !d.contains("Bruno"), "{d}");
    }

    #[test]
    fn i_commenti_e_le_revisioni_restano_nel_docx() {
        let mut e = Editor::new(documento(&["Ciao mondo"]), "prova");
        e.s.author = "Anna".into();
        e.s.run("select.range", &json!({ "anchor": Pos::body(0, 0), "focus": Pos::body(0, 4) })).unwrap();
        e.commenta("Saluto");
        e.traccia_modifiche(true);
        e.s.run("caret.set", &json!({ "pos": Pos::body(0, e.s.doc.body[0].as_para().unwrap().text.len()) })).unwrap();
        e.s.run("text.insert", &json!({ "text": "!", "raw": true })).unwrap();
        let byte = wordcraft_engine::io::save_bytes("prova.docx", &e.s.doc).unwrap();
        let mut s = crate::documento::apri_byte("prova.docx", &byte).unwrap();
        let st = stato(&mut s);
        assert!(st.traccia);
        assert_eq!(st.commenti.iter().map(|c| (c.autore.as_str(), c.testo.as_str())).collect::<Vec<_>>(), [("Anna", "Saluto")]);
        assert_eq!(st.modifiche.iter().map(|m| (m.genere, m.testo.as_str())).collect::<Vec<_>>(), [(GenereModifica::Aggiunto, "!")]);
    }

    #[test]
    fn date_ed_estratti() {
        assert_eq!(data_breve("2026-10-26T23:10:00Z"), "26/10/2026");
        assert_eq!(data_breve("2026-10-26"), "26/10/2026");
        assert_eq!(data_breve("ieri"), "");
        assert_eq!(estratto("uno\n  due"), "uno due");
        let lungo = "a".repeat(ESTRATTO + 10);
        assert_eq!(estratto(&lungo).chars().count(), ESTRATTO + 1);
    }
}
