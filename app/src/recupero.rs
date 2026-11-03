//! Il recupero dopo un'interruzione (l'app chiusa a forza, il telefono spento, Android che toglie
//! il processo in secondo piano).
//!
//! Due copie, tutte e due nella cartella privata dell'app. Quella del motore
//! (`Session::enable_recovery` in `cartella_app()/recupero`), che il motore scrive da solo ogni 10
//! secondi di modifiche e toglie quando il documento è pulito. E la nostra
//! (`cartella_app()/ripristino`), scritta tre secondi dopo l'ultima modifica non salvata e quando
//! l'app perde il fuoco, perché il motore non ha un modo pubblico per forzare subito la sua (`tick`
//! salva solo a scadenza, `Recovery` è `pub(crate)`) e su Android dopo onPause non gira più nessun
//! frame. All'avvio la Home propone la più recente delle due, riconosciute dall'id della copia del
//! motore, mai dal solo nome.
//!
//! I PDF hanno la loro copia (`cartella_app()/ripristino-pdf`, piano «PDF completo», P2): i byte
//! del salvataggio incrementale con le modifiche, scritti dal thread del motore poco dopo l'ultima
//! modifica e quando l'app perde il fuoco, tolti dopo Salva o «Non salvare». All'avvio si
//! propongono con quelle di Word, dalla più recente.
//!
//! Nella diagnosi solo quante sono e com'è andata: i nomi dei documenti mai.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;
use tascabile_platform::diagnosi;
use tascabile_word::documento;
use tascabile_word::salva::{Origine, scrivi_atomico};
use wordcraft_engine::Session;
use wordcraft_engine::recovery::RecoveryInfo;

/// Dove il motore tiene le sue copie, nella cartella dell'app.
pub const CARTELLA_MOTORE: &str = "recupero";
/// Dove l'app tiene la sua copia, nella cartella dell'app.
pub const CARTELLA_COPIE: &str = "ripristino";
/// Dove l'app tiene le copie dei PDF, nella cartella dell'app.
pub const CARTELLA_COPIE_PDF: &str = "ripristino-pdf";
/// Il nome che il motore dà alla copia di un documento senza file.
const SENZA_NOME_DEL_MOTORE: &str = "Untitled draft";

/// La funzione che sveglia l'interfaccia quando il lavoro del recupero finisce (dal thread del
/// motore).
pub type Risveglio = Arc<dyn Fn() + Send + Sync>;

/// Accende il recupero del motore per un documento.
pub fn accendi(s: &mut Session, cartella_app: &Path, risveglio: &Risveglio) {
    match s.enable_recovery(cartella_app.join(CARTELLA_MOTORE), risveglio.clone()) {
        Ok(()) => {}
        Err(_) => diagnosi("recupero: non acceso"),
    }
}

/// Aspetta che il lavoro del recupero di `s` finisca, al massimo `tetto`.
fn aspetta(s: &mut Session, tetto: Duration) -> bool {
    let fine = Instant::now() + tetto;
    loop {
        s.recovery_tick();
        if s.recovery_ready() && !s.recovery_busy() {
            return true;
        }
        if Instant::now() > fine {
            return false;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Un documento che si chiude (salvato, o «Non salvare»): il motore toglie la sua copia, che
/// altrimenti all'avvio dopo sarebbe proposta come lavoro perso. Il documento esce comunque,
/// quindi qui lo si dà per pulito; si aspetta al massimo due secondi.
pub fn spegni(s: &mut Session) {
    s.dirty = false;
    // Due giri: il primo può trovare il motore a metà di una copia, che finisce prima di togliere.
    let ok = aspetta(s, Duration::from_secs(1)) && {
        s.recovery_tick();
        aspetta(s, Duration::from_secs(1))
    };
    if !ok {
        diagnosi("recupero: la copia del motore non si è tolta in tempo");
    }
}

/// Secondi UTC di un'ora ISO del motore («2026-10-24T03:45:12Z»); 0 se non si legge.
pub fn secondi_iso(iso: &str) -> u64 {
    let n = |a: usize, b: usize| iso.get(a..b).and_then(|t| t.parse::<i64>().ok());
    let (Some(a), Some(m), Some(g), Some(h), Some(mi), Some(se)) = (n(0, 4), n(5, 7), n(8, 10), n(11, 13), n(14, 16), n(17, 19)) else {
        return 0;
    };
    // days_from_civil di Howard Hinnant.
    let a = if m <= 2 { a - 1 } else { a };
    let era = a.div_euclid(400);
    let anno_era = a - era * 400;
    let giorno_anno = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + g - 1;
    let giorno_era = anno_era * 365 + anno_era / 4 - anno_era / 100 + giorno_anno;
    let giorni = era * 146_097 + giorno_era - 719_468;
    u64::try_from(giorni * 86_400 + h * 3600 + mi * 60 + se).unwrap_or(0)
}

fn adesso() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// La nostra copia privata di un documento con modifiche non salvate: un .docx e accanto un .json
/// con il nome da mostrare, quello che usa il motore, gli id delle copie che il motore ha scritto
/// per lo stesso documento (per riconoscerle: il nome non basta, il motore chiama allo stesso modo
/// tutti i documenti senza file e due file uguali in cartelle diverse) e l'ora.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Copia {
    pub file: PathBuf,
    pub nome: String,
    pub nome_motore: String,
    pub id_motore: Vec<String>,
    pub salvata_alle: u64,
}

/// Le copie a metà: il documento senza il suo .json, o il .json (che ha il nome del documento)
/// senza il documento. Non si propongono mai, e restavano in chiaro fino a «Cancella dati»
/// (revisione del 31/10, punto 12). All'avvio non se ne scrive nessuna.
pub fn togli_le_meta(cartella_app: &Path) {
    for (cartella, est) in [(CARTELLA_COPIE, "docx"), (CARTELLA_COPIE_PDF, "pdf")] {
        let Ok(voci) = std::fs::read_dir(cartella_app.join(cartella)) else { continue };
        for v in voci.flatten() {
            let p = v.path();
            let compagno = match p.extension().and_then(|e| e.to_str()) {
                Some("json") => p.with_extension(est),
                Some(e) if e == est => p.with_extension("json"),
                _ => continue,
            };
            if !compagno.is_file() {
                let _ = std::fs::remove_file(&p);
            }
        }
    }
}

impl Copia {
    fn dati(file: &Path) -> PathBuf {
        file.with_extension("json")
    }

    /// Scrive (o riscrive) la copia `id` di `s` in `cartella`, tutto o niente: prima il .docx, poi
    /// il .json, che la fa esistere.
    pub fn scrivi(cartella: &Path, id: &str, nome: &str, id_motore: &[String], s: &Session) -> Result<Copia, String> {
        std::fs::create_dir_all(cartella).map_err(|e| format!("{:?}", e.kind()))?;
        let byte = wordcraft_engine::io::save_bytes("copia.docx", &s.doc).map_err(|_| "errore del motore".to_string())?;
        let file = cartella.join(format!("{id}.docx"));
        scrivi_atomico(&file, &byte).map_err(|e| format!("{:?}", e.kind()))?;
        let nome_motore =
            s.path.as_ref().and_then(|p| p.file_name()).map_or_else(|| SENZA_NOME_DEL_MOTORE.to_string(), |n| n.to_string_lossy().into_owned());
        let c = Copia { file, nome: nome.to_string(), nome_motore, id_motore: id_motore.to_vec(), salvata_alle: adesso() };
        let dati = json!({ "nome": c.nome, "nomeMotore": c.nome_motore, "idMotore": c.id_motore, "salvataAlle": c.salvata_alle });
        scrivi_atomico(&Self::dati(&c.file), dati.to_string().as_bytes()).map_err(|e| format!("{:?}", e.kind()))?;
        Ok(c)
    }

    /// Le copie che ci sono in `cartella`: un .json leggibile con il suo .docx accanto.
    pub fn elenca(cartella: &Path) -> Vec<Copia> {
        let Ok(voci) = std::fs::read_dir(cartella) else { return Vec::new() };
        let mut copie = Vec::new();
        for v in voci.flatten() {
            let p = v.path();
            if p.extension().is_none_or(|e| e != "json") {
                continue;
            }
            let file = p.with_extension("docx");
            let Ok(testo) = std::fs::read_to_string(&p) else { continue };
            let Ok(d) = serde_json::from_str::<serde_json::Value>(&testo) else { continue };
            if !file.is_file() {
                continue;
            }
            let testo_di = |k: &str| d.get(k).and_then(|v| v.as_str()).unwrap_or_default().to_string();
            copie.push(Copia {
                file,
                nome: testo_di("nome"),
                nome_motore: testo_di("nomeMotore"),
                id_motore: d
                    .get("idMotore")
                    .and_then(|v| v.as_array())
                    .map(|v| v.iter().filter_map(|i| i.as_str().map(str::to_string)).collect())
                    .unwrap_or_default(),
                salvata_alle: d.get("salvataAlle").and_then(|v| v.as_u64()).unwrap_or(0),
            });
        }
        copie.sort_by(|a, b| b.salvata_alle.cmp(&a.salvata_alle).then(a.file.cmp(&b.file)));
        copie
    }

    /// Toglie la copia (prima il .json, così una a metà non si propone).
    pub fn togli(&self) {
        let _ = std::fs::remove_file(Self::dati(&self.file));
        let _ = std::fs::remove_file(&self.file);
    }
}

/// La copia privata di un PDF con modifiche non salvate: i byte in un .pdf e accanto un .json col
/// nome da mostrare e l'ora. Il nome mai nella diagnosi (e non in `Debug`).
#[derive(Clone, PartialEq, Eq)]
pub struct CopiaPdf {
    pub file: PathBuf,
    pub nome: String,
    pub salvata_alle: u64,
}

impl std::fmt::Debug for CopiaPdf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CopiaPdf").field("salvata_alle", &self.salvata_alle).finish()
    }
}

impl CopiaPdf {
    /// Il file della copia `id` in `cartella`.
    pub fn file_di(cartella: &Path, id: &str) -> PathBuf {
        cartella.join(format!("{id}.pdf"))
    }

    /// Scrive (o riscrive) la copia `id` in `cartella`, tutto o niente: prima il .pdf, poi il
    /// .json, che la fa esistere. Errore: il genere, per la diagnosi.
    pub fn scrivi(cartella: &Path, id: &str, nome: &str, byte: &[u8]) -> Result<CopiaPdf, String> {
        std::fs::create_dir_all(cartella).map_err(|e| format!("{:?}", e.kind()))?;
        let file = Self::file_di(cartella, id);
        scrivi_atomico(&file, byte).map_err(|e| format!("{:?}", e.kind()))?;
        let c = CopiaPdf { file, nome: nome.to_string(), salvata_alle: adesso() };
        let dati = json!({ "nome": c.nome, "salvataAlle": c.salvata_alle });
        scrivi_atomico(&c.file.with_extension("json"), dati.to_string().as_bytes()).map_err(|e| format!("{:?}", e.kind()))?;
        Ok(c)
    }

    /// Le copie che ci sono in `cartella`: un .json leggibile col suo .pdf accanto.
    pub fn elenca(cartella: &Path) -> Vec<CopiaPdf> {
        let Ok(voci) = std::fs::read_dir(cartella) else { return Vec::new() };
        let mut copie: Vec<CopiaPdf> = voci
            .flatten()
            .map(|v| v.path())
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .filter_map(|p| {
                let file = p.with_extension("pdf");
                let d: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&p).ok()?).ok()?;
                file.is_file().then(|| CopiaPdf {
                    file,
                    nome: d.get("nome").and_then(|v| v.as_str()).unwrap_or("documento.pdf").to_string(),
                    salvata_alle: d.get("salvataAlle").and_then(|v| v.as_u64()).unwrap_or(0),
                })
            })
            .collect();
        copie.sort_by(|a, b| b.salvata_alle.cmp(&a.salvata_alle).then(a.file.cmp(&b.file)));
        copie
    }

    /// Toglie la copia `id` di `cartella` (prima il .json, così una a metà non si propone).
    pub fn togli_id(cartella: &Path, id: &str) {
        let file = Self::file_di(cartella, id);
        let _ = std::fs::remove_file(file.with_extension("json"));
        let _ = std::fs::remove_file(file);
    }

    pub fn togli(&self) {
        let _ = std::fs::remove_file(self.file.with_extension("json"));
        let _ = std::fs::remove_file(&self.file);
    }

    /// L'id della copia: il nome del file senza estensione.
    pub fn id(&self) -> String {
        self.file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
    }
}

/// Una copia da proporre: del motore o nostra, di un documento o di un PDF.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fonte {
    Motore(String, String, u64),
    Copia(Copia),
    Pdf(CopiaPdf),
}

impl Fonte {
    fn da_motore(r: &RecoveryInfo) -> Fonte {
        Fonte::Motore(r.id.clone(), r.name.clone(), secondi_iso(&r.saved_at))
    }

    pub fn quando(&self) -> u64 {
        match self {
            Fonte::Motore(_, _, q) => *q,
            Fonte::Copia(c) => c.salvata_alle,
            Fonte::Pdf(c) => c.salvata_alle,
        }
    }

    /// Il nome da mostrare: per un documento senza file «Documento nuovo».
    pub fn nome(&self) -> String {
        let nome = match self {
            Fonte::Motore(_, n, _) if n != SENZA_NOME_DEL_MOTORE => n.clone(),
            Fonte::Motore(..) => String::new(),
            Fonte::Copia(c) => c.nome.clone(),
            Fonte::Pdf(c) => c.nome.clone(),
        };
        if nome.is_empty() { "Documento nuovo".to_string() } else { nome }
    }
}

/// Una proposta della Home: la copia da riaprire e, se c'è, la sua gemella più vecchia (l'altra
/// copia dello stesso documento), che se ne va con lei.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposta {
    pub fonte: Fonte,
    pub gemella: Option<Fonte>,
}

/// Le proposte, dalla più recente: ogni nostra copia va con la copia del motore più recente dello
/// stesso documento (uno dei suoi `id_motore`), e di due resta da proporre la più recente. Mai per
/// il solo nome: il motore chiama allo stesso modo tutti i documenti senza file (nuovi, arrivati
/// con «Apri con», riaperti dal recupero), e la gemella se ne andrebbe senza essere mai mostrata.
pub fn da_proporre(copie: Vec<Copia>, mut motore: Vec<RecoveryInfo>) -> Vec<Proposta> {
    motore.sort_by_key(|r| std::cmp::Reverse(secondi_iso(&r.saved_at)));
    let mut proposte = Vec::new();
    for c in copie {
        let gemella = motore.iter().position(|r| c.id_motore.contains(&r.id)).map(|i| Fonte::da_motore(&motore.remove(i)));
        let nostra = Fonte::Copia(c);
        proposte.push(match gemella {
            Some(m) if m.quando() > nostra.quando() => Proposta { fonte: m, gemella: Some(nostra) },
            gemella => Proposta { fonte: nostra, gemella },
        });
    }
    proposte.extend(motore.iter().map(|r| Proposta { fonte: Fonte::da_motore(r), gemella: None }));
    proposte.sort_by_key(|p| std::cmp::Reverse(p.fonte.quando()));
    proposte
}

/// Le proposte coi PDF da recuperare, dalla più recente.
pub fn con_i_pdf(mut proposte: Vec<Proposta>, pdf: Vec<CopiaPdf>) -> Vec<Proposta> {
    proposte.extend(pdf.into_iter().map(|c| Proposta { fonte: Fonte::Pdf(c), gemella: None }));
    proposte.sort_by_key(|p| std::cmp::Reverse(p.fonte.quando()));
    proposte
}

/// Un documento riaperto dal recupero.
pub struct Riaperto {
    pub s: Session,
    pub origine: Origine,
    pub nome: String,
    /// La nostra copia da cui viene, che resta finché il documento non è salvato o chiuso.
    pub copia: Option<Copia>,
    /// Il recupero del motore è già acceso (la sessione che ha riaperto la sua copia).
    pub recupero_acceso: bool,
    /// L'id della copia del motore riaperta, che il motore continua a scrivere.
    pub id_motore: Option<String>,
}

/// Il recupero all'avvio: legge le copie del motore con una sessione sua e le nostre, e tiene le
/// proposte per la Home.
pub struct Recupero {
    cartella_app: PathBuf,
    risveglio: Risveglio,
    /// La sessione che legge, riapre e scarta le copie del motore.
    lettore: Option<Session>,
    pronto: bool,
    pub proposte: Vec<Proposta>,
}

impl Recupero {
    pub fn avvia(cartella_app: &Path, risveglio: Risveglio) -> Self {
        let mut r = Self { cartella_app: cartella_app.to_path_buf(), risveglio, lettore: None, pronto: false, proposte: Vec::new() };
        r.nuovo_lettore();
        r
    }

    fn nuovo_lettore(&mut self) {
        let mut s = documento::nuovo();
        accendi(&mut s, &self.cartella_app, &self.risveglio);
        self.lettore = Some(s);
    }

    /// Le copie sono state lette.
    #[cfg(test)]
    pub fn pronto(&self) -> bool {
        self.pronto
    }

    /// A ogni frame finché non è pronto: quando il motore ha letto le sue copie, le proposte.
    pub fn passo(&mut self) {
        if self.pronto {
            return;
        }
        let Some(s) = &mut self.lettore else { return };
        s.recovery_tick();
        if !s.recovery_ready() || s.recovery_busy() {
            return;
        }
        self.pronto = true;
        let motore = s.recovery_documents();
        let copie = Copia::elenca(&self.cartella_app.join(CARTELLA_COPIE));
        diagnosi(format!("recupero: {} copie del motore, {} dell'app", motore.len(), copie.len()));
        let pdf = CopiaPdf::elenca(&self.cartella_app.join(CARTELLA_COPIE_PDF));
        if !pdf.is_empty() {
            diagnosi(format!("recupero: {} copie di pdf", pdf.len()));
        }
        self.proposte = con_i_pdf(da_proporre(copie, motore), pdf);
        self.chiudi_se_finito();
    }

    /// Senza più niente da proporre il lettore non serve: se ne va col suo thread.
    fn chiudi_se_finito(&mut self) {
        if self.proposte.is_empty() {
            self.lettore = None;
        }
    }

    /// Il lettore pronto, rifatto se è stato preso da una copia riaperta.
    fn lettore(&mut self) -> Option<&mut Session> {
        if self.lettore.is_none() {
            self.nuovo_lettore();
        }
        let s = self.lettore.as_mut()?;
        aspetta(s, Duration::from_secs(5)).then_some(s)
    }

    /// Scarta una copia del motore o nostra.
    fn scarta_fonte(&mut self, f: &Fonte) {
        match f {
            Fonte::Copia(c) => c.togli(),
            Fonte::Pdf(c) => c.togli(),
            Fonte::Motore(id, ..) => {
                let Some(s) = self.lettore() else {
                    diagnosi("recupero: copia del motore non scartata (non pronto)");
                    return;
                };
                if s.run("file.recover", &json!({ "discard": id })).is_err() || !aspetta(s, Duration::from_secs(5)) {
                    diagnosi("recupero: copia del motore non scartata");
                }
            }
        }
    }

    /// «Scarta» della proposta `i`, con la sua gemella.
    pub fn scarta(&mut self, i: usize) {
        if i >= self.proposte.len() {
            return;
        }
        let p = self.proposte.remove(i);
        self.scarta_fonte(&p.fonte);
        if let Some(g) = &p.gemella {
            self.scarta_fonte(g);
        }
        diagnosi("recupero: scartato");
        self.chiudi_se_finito();
    }

    /// «Riapri» della proposta `i`, se è un PDF: esce dalle proposte e la copia resta (la riapre
    /// il lettore, e se ne va dopo Salva o «Non salvare»).
    pub fn prendi_pdf(&mut self, i: usize) -> Option<CopiaPdf> {
        let Some(Proposta { fonte: Fonte::Pdf(c), .. }) = self.proposte.get(i) else { return None };
        let c = c.clone();
        self.proposte.remove(i);
        self.chiudi_se_finito();
        Some(c)
    }

    /// «Riapri» della proposta `i`: la gemella se ne va; la copia riaperta resta finché il
    /// documento non è salvato o chiuso. Errore: il genere, per la diagnosi.
    pub fn riapri(&mut self, i: usize) -> Result<Riaperto, &'static str> {
        if i >= self.proposte.len() {
            return Err("proposta che non c'è");
        }
        let p = self.proposte.remove(i);
        if let Some(g) = &p.gemella {
            self.scarta_fonte(g);
        }
        let riaperto = self.riapri_fonte(p);
        self.chiudi_se_finito();
        riaperto
    }

    fn riapri_fonte(&mut self, p: Proposta) -> Result<Riaperto, &'static str> {
        let nome = p.fonte.nome();
        let origine_nome = match &p.fonte {
            Fonte::Motore(_, n, _) if n == SENZA_NOME_DEL_MOTORE => String::new(),
            Fonte::Motore(_, n, _) => n.clone(),
            Fonte::Copia(c) => c.nome.clone(),
            Fonte::Pdf(c) => c.nome.clone(),
        };
        match p.fonte {
            Fonte::Pdf(_) => Err("copia di un pdf"),
            Fonte::Copia(c) => {
                let byte = std::fs::read(&c.file).map_err(|_| "copia dell'app non leggibile")?;
                let mut s = documento::apri_byte("copia.docx", &byte).map_err(|e| e.genere)?;
                s.dirty = true;
                Ok(Riaperto { s, origine: Origine::Recuperato { nome: origine_nome }, nome, copia: Some(c), recupero_acceso: false, id_motore: None })
            }
            Fonte::Motore(id, ..) => {
                let s = self.lettore().ok_or("recupero non pronto")?;
                let prima = s.document_id();
                s.run("file.recover", &json!({ "restore": id })).map_err(|_| "copia del motore non riaperta")?;
                if !aspetta(s, Duration::from_secs(10)) || s.document_id() == prima {
                    return Err("copia del motore non riaperta");
                }
                let s = self.lettore.take().ok_or("recupero non pronto")?;
                Ok(Riaperto { s, origine: Origine::Recuperato { nome: origine_nome }, nome, copia: None, recupero_acceso: true, id_motore: Some(id) })
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::prova::cartella_di_prova;

    fn info(id: &str, nome: &str, alle: &str) -> RecoveryInfo {
        RecoveryInfo { id: id.into(), name: nome.into(), saved_at: alle.into() }
    }

    fn copia(nome_motore: &str, id_motore: &[&str], alle: u64) -> Copia {
        Copia {
            file: PathBuf::from(format!("/c/{alle}.docx")),
            nome: "Verbale.docx".into(),
            nome_motore: nome_motore.into(),
            id_motore: id_motore.iter().map(|i| i.to_string()).collect(),
            salvata_alle: alle,
        }
    }

    #[test]
    fn ora_iso_in_secondi() {
        assert_eq!(secondi_iso("1970-01-01T00:00:00Z"), 0);
        assert_eq!(secondi_iso("2026-10-24T03:45:12Z"), 1_792_813_512);
        assert_eq!(secondi_iso("rotta"), 0);
    }

    #[test]
    fn di_due_copie_dello_stesso_documento_si_propone_la_piu_recente() {
        let alle = secondi_iso("2026-10-24T03:45:12Z");
        // La nostra è più recente: la gemella del motore se ne va con lei.
        let p = da_proporre(vec![copia("Verbale.docx", &["a"], alle + 5)], vec![info("a", "Verbale.docx", "2026-10-24T03:45:12Z")]);
        assert_eq!(p.len(), 1);
        assert!(matches!(&p[0].fonte, Fonte::Copia(_)));
        assert!(matches!(&p[0].gemella, Some(Fonte::Motore(id, ..)) if id == "a"));
        // Quella del motore è più recente.
        let p = da_proporre(vec![copia("Verbale.docx", &["a"], alle - 5)], vec![info("a", "Verbale.docx", "2026-10-24T03:45:12Z")]);
        assert!(matches!(&p[0].fonte, Fonte::Motore(id, ..) if id == "a"));
        // Un documento diverso resta a parte, in ordine di tempo.
        let p = da_proporre(vec![copia("Verbale.docx", &["a"], alle)], vec![info("b", "Altro.docx", "2026-10-24T04:00:00Z")]);
        assert_eq!(p.len(), 2);
        // Un file con lo stesso nome in un'altra cartella (o un altro documento senza file) non è
        // una gemella: il motore ha scritto la sua copia con un altro id.
        let p = da_proporre(vec![copia("Verbale.docx", &["a"], alle)], vec![info("b", "Verbale.docx", "2026-10-24T04:00:00Z")]);
        assert_eq!(p.len(), 2);
        assert!(matches!(&p[0].fonte, Fonte::Motore(id, ..) if id == "b"));
        assert_eq!(p[1].gemella, None);
    }

    #[test]
    fn due_documenti_senza_file_non_sono_gemelli() {
        // La nostra copia di un documento nuovo (o arrivato con «Apri con») e quella del motore di un
        // altro documento senza file hanno lo stesso nome del motore: restano due proposte, e
        // riaprire o scartare l'una non tocca l'altra.
        let alle = secondi_iso("2026-10-24T03:45:12Z");
        let p = da_proporre(vec![copia(SENZA_NOME_DEL_MOTORE, &[], alle)], vec![info("b", SENZA_NOME_DEL_MOTORE, "2026-10-24T04:00:00Z")]);
        assert_eq!(p.len(), 2);
        assert!(p.iter().all(|p| p.gemella.is_none()));
    }

    #[test]
    fn la_copia_di_un_pdf_si_scrive_si_propone_e_si_toglie() {
        let d = cartella_di_prova("recupero-pdf");
        let c = CopiaPdf::scrivi(&d, "uno", "Referto Zanzibar.pdf", b"%PDF-1.7 di prova").unwrap();
        assert_eq!(CopiaPdf::elenca(&d), std::slice::from_ref(&c));
        assert_eq!((c.id(), std::fs::read(&c.file).unwrap()), ("uno".to_string(), b"%PDF-1.7 di prova".to_vec()));
        assert!(!format!("{c:?}").contains("Zanzibar"));
        // Con le copie di Word, dalla più recente; il nome è quello del PDF.
        let word = copia("Verbale.docx", &[], c.salvata_alle - 5);
        let proposte = con_i_pdf(da_proporre(vec![word], Vec::new()), CopiaPdf::elenca(&d));
        assert!(matches!(&proposte[0].fonte, Fonte::Pdf(p) if p.nome == "Referto Zanzibar.pdf"));
        assert_eq!(proposte[0].fonte.nome(), "Referto Zanzibar.pdf");
        assert_eq!(proposte.len(), 2);
        // Un .pdf senza il suo .json (una copia a metà) non si propone.
        std::fs::write(d.join("due.pdf"), b"%PDF").unwrap();
        assert_eq!(CopiaPdf::elenca(&d).len(), 1);
        CopiaPdf::togli_id(&d, "uno");
        assert!(CopiaPdf::elenca(&d).is_empty());
    }

    #[test]
    fn la_copia_dell_app_si_scrive_si_legge_e_si_toglie() {
        let d = cartella_di_prova("recupero-copia");
        let mut s = tascabile_word::documento::nuovo();
        s.run("text.insert", &json!({ "text": "Testo da non perdere", "raw": true })).unwrap();
        let c = Copia::scrivi(&d, "uno", "Verbale.docx", &["a".to_string()], &s).unwrap();
        assert_eq!(Copia::elenca(&d), std::slice::from_ref(&c));
        let riaperto = documento::apri_byte("x.docx", &std::fs::read(&c.file).unwrap()).unwrap();
        assert_eq!(riaperto.doc.body[0].as_para().unwrap().text, "Testo da non perdere");
        c.togli();
        assert!(Copia::elenca(&d).is_empty());
        assert_eq!(std::fs::read_dir(&d).unwrap().count(), 0);
    }
}
