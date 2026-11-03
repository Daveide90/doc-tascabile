//! La «finestra»: i paragrafi attorno al cursore che diamo alla tastiera di Android, uniti da "\n".
//! Un a capo di riga dentro un paragrafo diventa U+000B (stessa lunghezza in byte): così nella
//! finestra "\n" vuol dire sempre «paragrafo nuovo». Segnalibri, commenti e gli altri segnaposto
//! invisibili diventano U+2060 (`SEGNAPOSTO`), della stessa lunghezza in byte di U+FFFC.

use wordcraft_engine::Session;
use wordcraft_engine::doc::{Path, Pos, StoryRef};

/// Paragrafi presi prima e dopo quello del cursore.
pub const RAGGIO: u32 = 2;
/// Tetto prudente: il buffer di GameTextInput è di 64 KiB (ricerca Android, 5.3).
pub const MAX_UTF16: usize = 8_000;
/// Oltre questo la finestra non va alla tastiera (buffer di GameTextInput: 64 KiB in MUTF-8,
/// fino a 3 byte per unità UTF-16).
pub const LIMITE_UTF16: usize = 10_000;
/// Al posto di "\n" dentro un paragrafo (a capo di riga di Word).
pub const A_CAPO_DI_RIGA: char = '\u{000B}';
/// Al posto dei segnaposto invisibili di Word (segnalibri, commenti, permessi, inizio e fine di
/// campi e controlli contenuto): U+2060, invisibile, non spezza le parole per i suggerimenti,
/// ed è lungo 3 byte come U+FFFC.
pub const SEGNAPOSTO: char = '\u{2060}';

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finestra {
    pub story: StoryRef,
    /// Percorso del contenitore: vuoto al primo livello di una storia (corpo, intestazione, nota),
    /// [blocco, riga, cella] dentro una tabella.
    pub prefisso: Vec<u32>,
    /// Indice del primo paragrafo nel contenitore.
    pub primo: u32,
    /// Quanti paragrafi.
    pub n: u32,
}

impl Finestra {
    /// Percorso del paragrafo `k` della finestra.
    pub fn percorso(&self, k: u32) -> Path {
        let mut p = self.prefisso.clone();
        p.push(self.primo + k);
        Path(p)
    }

    fn esiste(&self, s: &Session, i: u32) -> bool {
        let mut p = self.prefisso.clone();
        p.push(i);
        s.doc.para(self.story, &Path(p)).is_some()
    }

    /// C'è un paragrafo subito prima della finestra?
    pub fn ha_prima(&self, s: &Session) -> bool {
        self.primo > 0 && self.esiste(s, self.primo - 1)
    }

    /// C'è un paragrafo subito dopo la finestra?
    pub fn ha_dopo(&self, s: &Session) -> bool {
        self.esiste(s, self.primo + self.n)
    }

    /// Fino a `raggio` paragrafi prima e dopo quello del cursore, nello stesso contenitore.
    pub fn attorno(s: &Session, raggio: u32) -> Option<Finestra> {
        let f = &s.sel.focus;
        s.doc.para_at(f)?;
        let (&centro, prefisso) = f.path.0.split_last()?;
        let mut w = Finestra { story: f.story, prefisso: prefisso.to_vec(), primo: centro, n: 1 };
        while centro - w.primo < raggio && w.ha_prima(s) {
            w.primo -= 1;
            w.n += 1;
        }
        while w.primo + w.n - 1 - centro < raggio && w.ha_dopo(s) {
            w.n += 1;
        }
        Some(w)
    }

    /// Testo della finestra preso dal documento (None se un paragrafo non c'è più).
    pub fn testo(&self, s: &Session) -> Option<String> {
        let mut out = String::new();
        for k in 0..self.n {
            if k > 0 {
                out.push('\n');
            }
            let p = s.doc.para(self.story, &self.percorso(k))?;
            let mut oggetto = 0usize;
            for c in p.text.chars() {
                out.push(match c {
                    '\n' => A_CAPO_DI_RIGA,
                    '\u{FFFC}' => {
                        let segnaposto = p.objects.get(oggetto).is_some_and(|o| o.is_marker());
                        oggetto += 1;
                        if segnaposto { SEGNAPOSTO } else { c }
                    }
                    c => c,
                });
            }
        }
        Some(out)
    }

    /// Byte della finestra (su un confine di carattere) → posizione nel documento.
    pub fn pos(&self, testo: &str, byte: usize) -> Pos {
        let byte = testo.floor_char_boundary(byte);
        let k = testo[..byte].matches('\n').count() as u32;
        let inizio = testo[..byte].rfind('\n').map_or(0, |i| i + 1);
        Pos { story: self.story, path: self.percorso(k), off: byte - inizio }
    }

    /// Posizione nel documento → byte della finestra (None se è fuori dalla finestra).
    pub fn byte(&self, testo: &str, pos: &Pos) -> Option<usize> {
        if pos.story != self.story {
            return None;
        }
        let (&i, prefisso) = pos.path.0.split_last()?;
        if prefisso != self.prefisso.as_slice() || i < self.primo || i >= self.primo + self.n {
            return None;
        }
        let k = (i - self.primo) as usize;
        let mut segmenti = testo.split('\n');
        let inizio: usize = segmenti.by_ref().take(k).map(|seg| seg.len() + 1).sum();
        let segmento = segmenti.next()?;
        if pos.off > segmento.len() {
            return None;
        }
        Some(inizio + pos.off)
    }

    /// Indice, dentro la finestra, del paragrafo che contiene `byte`.
    pub fn paragrafo_di(&self, testo: &str, byte: usize) -> u32 {
        testo[..testo.floor_char_boundary(byte)].matches('\n').count() as u32
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::prova::documento;
    use serde_json::json;

    fn cursore(s: &mut Session, blocco: usize, off: usize) {
        s.run("caret.set", &json!({ "pos": Pos::body(blocco, off) })).unwrap();
    }

    #[test]
    fn finestra_al_centro() {
        let mut s = documento(&["p0", "p1", "p2", "p3", "p4", "p5", "p6"]);
        cursore(&mut s, 3, 1);
        let f = Finestra::attorno(&s, RAGGIO).unwrap();
        assert_eq!((f.primo, f.n), (1, 5));
        assert_eq!(f.testo(&s).unwrap(), "p1\np2\np3\np4\np5");
    }

    #[test]
    fn finestra_all_inizio() {
        let mut s = documento(&["p0", "p1", "p2", "p3"]);
        cursore(&mut s, 0, 0);
        let f = Finestra::attorno(&s, RAGGIO).unwrap();
        assert_eq!((f.primo, f.n), (0, 3));
        assert!(!f.ha_prima(&s));
        assert!(f.ha_dopo(&s));
    }

    #[test]
    fn da_byte_a_posizione_e_ritorno() {
        let mut s = documento(&["p0", "p1", "p2", "p3", "p4", "p5", "p6"]);
        cursore(&mut s, 3, 1);
        let f = Finestra::attorno(&s, RAGGIO).unwrap();
        let t = f.testo(&s).unwrap();
        let pos = Pos::body(3, 1);
        assert_eq!(f.byte(&t, &pos), Some(7));
        assert_eq!(f.pos(&t, 7), pos);
        // fine di p2, proprio prima del "\n"
        assert_eq!(f.pos(&t, 5), Pos::body(2, 2));
        // fuori dalla finestra
        assert_eq!(f.byte(&t, &Pos::body(6, 0)), None);
        assert_eq!(f.paragrafo_di(&t, 7), 2);
    }

    #[test]
    fn a_capo_di_riga_resta_un_byte() {
        let mut s = documento(&["uno"]);
        s.run("text.lineBreak", &json!({})).unwrap();
        s.run("text.insert", &json!({ "text": "due", "raw": true })).unwrap();
        let f = Finestra::attorno(&s, RAGGIO).unwrap();
        assert_eq!(f.testo(&s).unwrap(), "uno\u{000B}due");
    }

    fn cursore_in(s: &mut Session, pos: Pos) {
        s.run("caret.set", &json!({ "pos": pos })).unwrap();
    }

    /// Ogni confine di carattere della finestra: byte → pos → byte torna uguale, su un carattere vero.
    fn andata_e_ritorno(s: &Session, f: &Finestra, t: &str) {
        for (b, _) in t.char_indices().chain(std::iter::once((t.len(), ' '))) {
            let p = f.pos(t, b);
            assert_eq!(f.byte(t, &p), Some(b), "byte {b} in {t:?}");
            let par = s.doc.para_at(&p).expect("il paragrafo esiste");
            assert!(p.off <= par.text.len() && par.text.is_char_boundary(p.off), "off {} in {:?}", p.off, par.text);
        }
    }

    #[test]
    fn segnalibro_invisibile_alla_tastiera() {
        let mut s = documento(&["Ciao mondo"]);
        s.run("select.range", &json!({ "anchor": Pos::body(0, 5), "focus": Pos::body(0, 10) })).unwrap();
        s.run("insert.bookmark", &json!({ "name": "Segno" })).unwrap();
        cursore(&mut s, 0, 0);
        let f = Finestra::attorno(&s, RAGGIO).unwrap();
        let t = f.testo(&s).unwrap();
        assert!(!t.contains('\u{FFFC}'), "{t:?}");
        assert_eq!(t.replace(SEGNAPOSTO, ""), "Ciao mondo");
        andata_e_ritorno(&s, &f, &t);
    }

    #[test]
    fn tabella_prima_e_dopo_la_finestra() {
        let mut s = documento(&["p0", "p1", "p2", "p3", "p4"]);
        cursore(&mut s, 1, 2);
        s.run("insert.table", &json!({ "rows": 2, "cols": 2 })).unwrap();
        let tabella = s.doc.body.iter().position(|b| b.as_table().is_some()).unwrap();
        cursore(&mut s, tabella - 1, 0);
        let f = Finestra::attorno(&s, RAGGIO).unwrap();
        assert!(!f.ha_dopo(&s), "la tabella chiude la finestra");
        andata_e_ritorno(&s, &f, &f.testo(&s).unwrap());
        cursore(&mut s, tabella + 1, 0);
        let f = Finestra::attorno(&s, RAGGIO).unwrap();
        assert!(!f.ha_prima(&s), "la tabella apre la finestra");
        let t = f.testo(&s).unwrap();
        andata_e_ritorno(&s, &f, &t);
        let in_cella = Pos { story: StoryRef::Body, path: Path(vec![tabella as u32, 0, 0, 0]), off: 0 };
        assert_eq!(f.byte(&t, &in_cella), None);
    }

    #[test]
    fn finestra_dentro_una_cella() {
        let mut s = documento(&["p0", "p1"]);
        cursore(&mut s, 0, 2);
        s.run("insert.table", &json!({ "rows": 2, "cols": 2 })).unwrap();
        for (i, p) in ["uno", "dué", "tre😀", "quattro", "cinque", "sei"].iter().enumerate() {
            if i > 0 {
                s.run("text.newParagraph", &json!({})).unwrap();
            }
            s.run("text.insert", &json!({ "text": p, "raw": true })).unwrap();
        }
        let mut c = s.sel.focus.clone();
        c.path.0[3] = 2;
        c.off = 0;
        cursore_in(&mut s, c);
        let f = Finestra::attorno(&s, RAGGIO).unwrap();
        let t = f.testo(&s).unwrap();
        assert_eq!(f.prefisso.len(), 3);
        assert_eq!(t, "uno\ndué\ntre😀\nquattro\ncinque");
        andata_e_ritorno(&s, &f, &t);
        assert_eq!(f.byte(&t, &Pos::body(0, 0)), None);
    }

    #[test]
    fn finestra_nell_intestazione() {
        let mut s = documento(&["corpo"]);
        s.run("insert.editHeader", &json!({})).unwrap();
        s.run("text.insert", &json!({ "text": "Intestazione è", "raw": true })).unwrap();
        s.run("text.newParagraph", &json!({})).unwrap();
        s.run("text.insert", &json!({ "text": "seconda", "raw": true })).unwrap();
        let f = Finestra::attorno(&s, RAGGIO).unwrap();
        assert!(matches!(f.story, StoryRef::Part(_)));
        let t = f.testo(&s).unwrap();
        andata_e_ritorno(&s, &f, &t);
        assert_eq!(f.byte(&t, &Pos::body(0, 0)), None);
    }

    #[test]
    fn posizione_oltre_il_paragrafo_rifiutata() {
        let mut s = documento(&["p0", "p1", "p2", "p3", "p4"]);
        cursore(&mut s, 2, 0);
        let f = Finestra::attorno(&s, RAGGIO).unwrap();
        let t = f.testo(&s).unwrap();
        assert_eq!(f.byte(&t, &Pos::body(1, 5)), None);
        assert_eq!(f.byte(&t, &Pos::body(4, 99)), None);
    }

    #[test]
    fn byte_a_meta_carattere_senza_panico() {
        let mut s = documento(&["città"]);
        cursore(&mut s, 0, 0);
        let f = Finestra::attorno(&s, RAGGIO).unwrap();
        let t = f.testo(&s).unwrap();
        assert_eq!(f.pos(&t, 5), Pos::body(0, 4)); // 'à' occupa i byte 4..6
        assert_eq!(f.paragrafo_di(&t, 5), 0);
    }
}
