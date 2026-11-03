//! L'ortografia italiana: il dizionario Hunspell `it_IT` degli asset (di LibreItalia, GPL 3, letto
//! come dati da `asset/dizionario`) dato al motore, il dizionario personale nella memoria privata
//! dell'app, la parola sottolineata sotto il cursore coi suggerimenti, e quello che se ne fa:
//! correggerla, aggiungerla al dizionario, ignorarla. Nella diagnosi mai una parola: solo conti e
//! tempi.

use std::path::PathBuf;
use std::sync::RwLock;

use serde_json::{Value, json};
use wordcraft_engine::Session;
use wordcraft_engine::doc::Pos;
use wordcraft_engine::proof;

/// I due file del dizionario negli asset dell'APK (sul PC in `crates/mod-word/asset`).
pub const ASSET_AFF: &str = "dizionario/it_IT.aff";
pub const ASSET_DIC: &str = "dizionario/it_IT.dic";
/// Quanti suggerimenti al più nella barra.
pub const MASSIMO_SUGGERIMENTI: usize = 3;
/// Il nome del file del dizionario personale, nella cartella privata dell'app.
pub const FILE_PERSONALE: &str = "dizionario-personale.txt";

/// Il dizionario personale: il file dove il motore scrive le parole aggiunte.
static PERSONALE: RwLock<Option<PathBuf>> = RwLock::new(None);

/// Perché il dizionario non si è installato: solo il genere, mai il contenuto dei file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroreDizionario {
    /// La codifica dei file (`SET`) non è una di quelle che il motore legge.
    Codifica,
    /// Il motore non ha capito i file.
    File,
}

impl ErroreDizionario {
    pub fn genere(self) -> &'static str {
        match self {
            ErroreDizionario::Codifica => "codifica",
            ErroreDizionario::File => "file non validi",
        }
    }
}

/// Dà al motore il dizionario italiano dai byte dei due file (`.aff` e `.dic`): da qui in poi le
/// parole dei testi in italiano si controllano. Dice quante radici ha. Dopo, i layout vanno rifatti
/// (`Session::relayout`): quelli già fatti non hanno le sottolineature.
pub fn installa_italiano(aff: &[u8], dic: &[u8]) -> Result<usize, ErroreDizionario> {
    let (aff, dic) = proof::hunspell::decode(aff, dic).map_err(|_| ErroreDizionario::Codifica)?;
    proof::it::install_dictionary(&aff, &dic).map_err(|_| ErroreDizionario::File)?;
    Ok(proof::it::dictionary().len())
}

/// Le parole del dizionario personale nel `file` (nella memoria privata dell'app) entrano nel
/// motore, e le sessioni preparate da qui in poi ci scrivono quelle aggiunte. Dice quante sono.
pub fn usa_dizionario_personale(file: PathBuf) -> usize {
    let n = proof::load_user_dictionary(&file).len();
    *PERSONALE.write().unwrap_or_else(|e| e.into_inner()) = Some(file);
    n
}

/// La sessione scrive le parole aggiunte nel dizionario personale, se c'è.
pub(crate) fn prepara(s: &mut Session) {
    s.user_dict_path = PERSONALE.read().unwrap_or_else(|e| e.into_inner()).clone();
}

/// La parola sottolineata col cursore dentro (o a un suo bordo).
#[derive(Debug, Clone, PartialEq)]
pub struct Sbagliata {
    pub inizio: Pos,
    pub fine: Pos,
    /// Al più `MASSIMO_SUGGERIMENTI`, il migliore prima.
    pub suggerimenti: Vec<String>,
    /// Ortografia (la linea rossa ondulata: la parola si aggiunge al dizionario o si ignora) o
    /// grammatica (la doppia linea blu: solo i suggerimenti della regola).
    pub ortografia: bool,
}

/// La parola sottolineata al cursore, se il controllo è acceso e il cursore è senza selezione.
pub fn al_cursore(s: &mut Session) -> Option<Sbagliata> {
    if !s.view.proofing || !s.sel.is_collapsed() {
        return None;
    }
    let v = s.run("review.suggestions", &json!({})).ok()?;
    let pos = |chiave: &str| serde_json::from_value::<Pos>(v.get(chiave)?.clone()).ok();
    let (inizio, fine) = (pos("start")?, pos("end")?);
    let suggerimenti = v
        .get("suggestions")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).filter(|t| !t.trim().is_empty()).take(MASSIMO_SUGGERIMENTI).map(str::to_string).collect())
        .unwrap_or_default();
    let ortografia = v.get("kind").and_then(Value::as_str) == Some("spelling");
    Some(Sbagliata { inizio, fine, suggerimenti, ortografia })
}

#[cfg(test)]
pub(crate) mod test {
    use super::*;
    use crate::prova::documento;

    /// Il dizionario degli asset nel motore (una volta per processo; installarlo di nuovo non
    /// cambia niente).
    pub(crate) fn con_il_dizionario() {
        static FATTO: std::sync::Once = std::sync::Once::new();
        FATTO.call_once(|| {
            let cartella = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("asset");
            let aff = std::fs::read(cartella.join(ASSET_AFF)).unwrap();
            let dic = std::fs::read(cartella.join(ASSET_DIC)).unwrap();
            let t = std::time::Instant::now();
            let n = installa_italiano(&aff, &dic).expect("il dizionario degli asset");
            eprintln!("dizionario italiano: {n} radici in {:.0} ms", t.elapsed().as_secs_f32() * 1000.0);
            assert!(n > 90_000, "{n}");
        });
    }

    #[test]
    fn il_dizionario_degli_asset_controlla_l_italiano() {
        con_il_dizionario();
        for giusta in ["perché", "infermiere", "somministrata", "l'amico", "prescrizione", "Roma"] {
            assert!(proof::it::is_correct(giusta), "{giusta}");
        }
        for sbagliata in ["perche", "infermiero", "sommnistrata", "precrizione"] {
            assert!(!proof::it::is_correct(sbagliata), "{sbagliata}");
        }
        assert_eq!(installa_italiano(b"SET SHIFT_JIS\n", b"1\nx\n"), Err(ErroreDizionario::Codifica));
    }

    /// Il cursore dentro la parola `parola` del paragrafo 0.
    fn cursore_in(s: &mut Session, parola: &str) {
        let da = s.doc.body[0].as_para().unwrap().text.find(parola).unwrap();
        s.run("caret.set", &json!({ "pos": Pos::body(0, da + 2) })).unwrap();
    }

    #[test]
    fn la_parola_sbagliata_al_cursore_coi_suggerimenti() {
        con_il_dizionario();
        let mut s = documento(&["Il letto perche la terapia"]);
        cursore_in(&mut s, "perche");
        let sb = al_cursore(&mut s).expect("parola sottolineata");
        assert_eq!((sb.inizio.off, sb.fine.off, sb.ortografia), (9, 15, true));
        assert_eq!(sb.suggerimenti.first().map(String::as_str), Some("perché"), "{:?}", sb.suggerimenti);
        assert!(sb.suggerimenti.len() <= MASSIMO_SUGGERIMENTI);
        // Su una parola giusta niente; con una selezione niente; col controllo spento niente.
        cursore_in(&mut s, "terapia");
        assert_eq!(al_cursore(&mut s), None);
        s.run("select.all", &json!({})).unwrap();
        assert_eq!(al_cursore(&mut s), None);
        cursore_in(&mut s, "perche");
        s.view.proofing = false;
        assert_eq!(al_cursore(&mut s), None);
    }

    /// Le linee ondulate (dell'ortografia) nella lista di disegno della pagina di Scrittura.
    fn onde(s: &mut Session) -> usize {
        use wordcraft_engine::layout::display::{Draw, Stroke, page_display};
        let l = crate::vista::layout_scrittura(s, 412.0, 1.0);
        page_display(&s.doc, &l.pages[0], &Default::default()).iter().filter(|d| matches!(d, Draw::Line { stroke: Stroke::Wave, .. })).count()
    }

    #[test]
    fn il_layout_sottolinea_le_parole_sbagliate() {
        con_il_dizionario();
        let mut s = documento(&["Il letto perche la terapia"]);
        assert_eq!(onde(&mut s), 1, "una linea ondulata sotto «perche»");
        s.view.proofing = false;
        assert_eq!(onde(&mut s), 0);
    }
}
