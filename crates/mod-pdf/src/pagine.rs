//! Pagine e file (piano «PDF completo», P6): le modifiche delle pagine (eliminare, duplicare,
//! spostare, la pagina bianca grande come la vicina, un altro PDF unito dentro), la password
//! (AES-256, il predefinito del motore) e i file nuovi fatti dal documento: pagine estratte, parti
//! di una divisione, la copia compressa.
//!
//! Le modifiche vanno al motore del thread come tutte le altre e si annullano (ricerca PDFCraft,
//! sezione 8: `InsertBlankPage` vuole la misura, `InsertPagesFrom` i byte e un nome). I file nuovi
//! si fanno da un `Pdf` aperto sui byte di adesso (quelli dell'istantanea), su un thread di
//! lavoro: il documento aperto non cambia. Gli errori escono col loro genere, mai col messaggio
//! del motore, che può riportare pezzi del documento.

use std::sync::Arc;

use pdfcraft_engine::optimize::{Compression, ImageSettings, Settings};
use pdfcraft_engine::optimizer::{OptimizeJob, OptimizeStage};
use pdfcraft_engine::{Edit, EditError, FieldConflicts, Protection};

use crate::apertura::{Misura, Pdf};
use crate::modifica::Modifica;
use crate::motore::genere_dell_errore;

/// La misura più piccola e più grande di una pagina per il motore, in punti
/// (`crates/automation/src/lib.rs:1627-1639` di PDFCraft).
const LATO_MINIMO: f32 = 1.0;
const LATO_MASSIMO: f32 = 14_400.0;

impl Modifica {
    /// Toglie le pagine `pagine` (da 0). Il motore non toglie tutte le pagine.
    pub fn elimina(pagine: Vec<usize>) -> Modifica {
        Modifica::del_motore(Edit::DeletePages { pages: pagine })
    }

    /// Una copia delle pagine `pagine`, nel loro ordine, subito dopo l'ultima di loro.
    pub fn duplica(pagine: Vec<usize>) -> Modifica {
        Modifica::del_motore(Edit::DuplicatePages { pages: pagine, at: None })
    }

    /// Porta le pagine `pagine` (nel loro ordine) al posto `posto` dell'ordine di adesso: davanti
    /// alla pagina che oggi sta lì, o in fondo con `posto` uguale al numero delle pagine.
    pub fn sposta(pagine: Vec<usize>, posto: usize) -> Modifica {
        let a = posto_dopo_lo_spostamento(&pagine, posto);
        Modifica::del_motore(Edit::MovePages { pages: pagine, to: a })
    }

    /// Una pagina bianca al posto `posto` (0 è prima della prima), grande `misura` (quella della
    /// pagina vicina come si vede).
    pub fn pagina_bianca(posto: usize, misura: Misura) -> Modifica {
        let lato = |v: f32| f64::from(if v.is_finite() { v.clamp(LATO_MINIMO, LATO_MASSIMO) } else { 595.0 });
        Modifica::del_motore(Edit::InsertBlankPage { at: posto, width: lato(misura.larghezza), height: lato(misura.altezza) })
    }

    /// Tutte le pagine del PDF `byte` al posto `posto`. `nome` (il nome del file) resta nel
    /// documento come titolo del segnalibro che raccoglie i segnalibri dell'altro PDF, come fa
    /// Acrobat; i campi con un nome già usato prendono un nome nuovo.
    pub fn unisci(nome: &str, byte: Arc<Vec<u8>>, posto: usize) -> Modifica {
        Modifica::del_motore(Edit::InsertPagesFrom {
            name: nome.to_string(),
            bytes: byte,
            pages: None,
            at: posto,
            field_conflicts: FieldConflicts::Rename,
        })
    }

    /// La password per aprire il PDF, in AES-256, senza altri limiti: la scrive il salvataggio
    /// dopo, che riscrive tutto il file. La password resta solo nella memoria del motore.
    pub fn proteggi(password: &str) -> Modifica {
        Modifica::del_motore(Edit::Protect(Protection { open_password: Some(password.to_string()), ..Default::default() }))
    }

    /// Toglie la password (e gli altri limiti): la scrive il salvataggio dopo.
    pub fn togli_protezione() -> Modifica {
        Modifica::del_motore(Edit::RemoveProtection)
    }
}

/// Dove va a finire chi si sposta, per il motore (`MovePages` vuole il posto nell'ordine senza le
/// pagine che si spostano): `posto` è nell'ordine di adesso, davanti alla pagina che sta lì.
pub fn posto_dopo_lo_spostamento(pagine: &[usize], posto: usize) -> usize {
    let mut v = pagine.to_vec();
    v.sort_unstable();
    v.dedup();
    posto - v.iter().filter(|p| **p < posto).count()
}

/// Lo spostamento cambia l'ordine: le pagine non stanno già lì, una dopo l'altra.
pub fn sposta_qualcosa(pagine: &[usize], posto: usize) -> bool {
    let mut v = pagine.to_vec();
    v.sort_unstable();
    v.dedup();
    let a = posto_dopo_lo_spostamento(&v, posto);
    v.iter().enumerate().any(|(i, p)| *p != a + i)
}

/// Le pagine da 0 dette come le scrive una persona, da 1: «1-3, 5, 7-8».
pub fn pagine_dette(pagine: &[usize]) -> String {
    let mut v = pagine.to_vec();
    v.sort_unstable();
    v.dedup();
    let mut pezzi: Vec<String> = Vec::new();
    let mut i = 0;
    while i < v.len() {
        let mut j = i;
        while j + 1 < v.len() && v[j + 1] == v[j] + 1 {
            j += 1;
        }
        pezzi.push(if i == j { format!("{}", v[i] + 1) } else { format!("{}-{}", v[i] + 1, v[j] + 1) });
        i = j + 1;
    }
    pezzi.join(", ")
}

/// Gli intervalli scritti per dividere un PDF di `pagine` pagine («1-3, 4-8», «5», «9-» fino in
/// fondo), separati da virgole, punti e virgola o spazi: le pagine (da 0) di ogni parte. None se
/// non c'è niente o se un pezzo non va (fuori dal PDF, al contrario, non un numero).
pub fn intervalli(testo: &str, pagine: usize) -> Option<Vec<Vec<usize>>> {
    let numero = |s: &str| s.trim().parse::<usize>().ok().filter(|n| (1..=pagine).contains(n));
    let mut parti = Vec::new();
    // Gli spazi attorno al trattino non dividono: «1 - 3» è un intervallo.
    let pulito = testo.replace(" -", "-").replace("- ", "-");
    for pezzo in pulito.split([',', ';', ' ', '\n']).map(str::trim).filter(|p| !p.is_empty()) {
        let (a, b) = match pezzo.split_once('-') {
            Some((a, "")) => (numero(a)?, pagine),
            Some((a, b)) => (numero(a)?, numero(b)?),
            None => {
                let n = numero(pezzo)?;
                (n, n)
            }
        };
        if a > b {
            return None;
        }
        parti.push((a - 1..b).collect::<Vec<usize>>());
    }
    (!parti.is_empty()).then_some(parti)
}

/// Le parti di un PDF di `pagine` pagine diviso ogni `n` pagine (l'ultima può essere più corta).
pub fn ogni(n: usize, pagine: usize) -> Vec<Vec<usize>> {
    let n = n.max(1);
    (0..pagine).step_by(n).map(|a| (a..(a + n).min(pagine)).collect()).collect()
}

/// Quanto si comprime: le immagini più fitte di una soglia si riducono a una risoluzione e si
/// riscrivono in JPEG con una qualità; il resto (oggetti doppi, flussi senza compressione) si
/// pulisce in tutti e tre i livelli.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Livello {
    /// Oltre 300 ppi a 200, JPEG alto (80): le immagini restano nitide.
    Leggera,
    /// Le scelte di «Riduci dimensioni file» di Acrobat: oltre 225 ppi a 150, JPEG medio (60).
    Media,
    /// Oltre 144 ppi a 96, JPEG basso (45): per leggere sullo schermo, il file più piccolo.
    Forte,
}

impl Livello {
    pub const TUTTI: [Livello; 3] = [Livello::Leggera, Livello::Media, Livello::Forte];

    pub fn nome(self) -> &'static str {
        match self {
            Livello::Leggera => "Leggera",
            Livello::Media => "Media",
            Livello::Forte => "Forte",
        }
    }

    /// Che cosa vuol dire, per chi sceglie.
    pub fn spiegazione(self) -> &'static str {
        match self {
            Livello::Leggera => "Le immagini restano nitide",
            Livello::Media => "Come «Riduci dimensioni» di Acrobat",
            Livello::Forte => "Il file più piccolo, da leggere sullo schermo",
        }
    }

    fn immagini(self) -> ImageSettings {
        match self {
            Livello::Leggera => ImageSettings { downsample: true, target_ppi: 200.0, above_ppi: 300.0, compression: Compression::Jpeg(80) },
            Livello::Media => ImageSettings::REDUCE,
            Livello::Forte => ImageSettings { downsample: true, target_ppi: 96.0, above_ppi: 144.0, compression: Compression::Jpeg(45) },
        }
    }

    /// Le impostazioni del motore per questo livello.
    pub fn impostazioni(self) -> Settings {
        Settings { color: self.immagini(), gray: self.immagini(), ..Settings::default() }
    }
}

/// Una compressione pronta da fare su un thread di lavoro: quello che serve al motore, staccato
/// dalla sessione, e la misura di adesso.
pub struct Compressione {
    lavoro: OptimizeJob,
    /// I byte del documento prima di comprimerlo.
    pub prima: usize,
}

impl Compressione {
    /// Comprime: `avanti` riceve quanto è fatto (in millesimi) e risponde se continuare. I byte
    /// della copia compressa, o il genere dell'errore («fermata» se si è fermata).
    pub fn esegui(self, avanti: &(dyn Fn(usize, usize) -> bool + Sync)) -> Result<Arc<Vec<u8>>, &'static str> {
        let esito = self.lavoro.run(|s: OptimizeStage| avanti((s.fraction() * 1000.0) as usize, 1000));
        match esito {
            Ok((byte, _)) => Ok(byte),
            Err(EditError::Cancelled) => Err("fermata"),
            Err(e) => Err(genere_dell_errore(&e)),
        }
    }
}

impl Pdf {
    /// Un PDF nuovo con le pagine `pagine` (da 0) del documento, nel loro ordine. Se il documento
    /// si è aperto con una password, il PDF nuovo chiede la stessa (AES-256, come Proteggi): il
    /// motore lo scrive in chiaro, e chiunque avesse il file leggerebbe le pagine.
    pub fn estrai(&self, pagine: &[usize]) -> Result<Arc<Vec<u8>>, &'static str> {
        if pagine.is_empty() {
            return Err("nessuna pagina");
        }
        let nuovo = self.sessione.extract(self.id, pagine).map_err(|e| genere_dell_errore(&e))?;
        let Some(password) = self.documento().password.as_deref().filter(|p| !p.is_empty()) else { return Ok(nuovo) };
        let mut copia = Pdf::apri_condiviso(nuovo, None).map_err(|_| "apertura")?;
        let protezione = Edit::Protect(Protection { open_password: Some(password.to_string()), ..Default::default() });
        copia.sessione.apply(copia.id, protezione).map_err(|e| genere_dell_errore(&e))?;
        match copia.sessione.save_full_bytes(copia.id) {
            Err(EditError::Signed) => copia.sessione.save_bytes(copia.id),
            altro => altro,
        }
        .map_err(|e| genere_dell_errore(&e))
    }

    /// La compressione del documento com'è a `livello`, da fare con [`Compressione::esegui`]. Un
    /// PDF firmato no: la copia è una riscrittura completa.
    pub fn compressione(&self, livello: Livello) -> Result<Compressione, &'static str> {
        let lavoro = self.sessione.optimize_job(self.id, &livello.impostazioni(), &[]).map_err(|e| genere_dell_errore(&e))?;
        Ok(Compressione { lavoro, prima: self.documento().bytes.len() })
    }
}

/// Un altro PDF da unire si legge: non è protetto da password e i suoi permessi lasciano copiare
/// le pagine. Il genere del problema, se c'è.
pub fn si_unisce(byte: &Arc<Vec<u8>>) -> Result<usize, &'static str> {
    use pdfcraft_engine::SourceProblem;
    match pdfcraft_engine::combine_source_check(byte, None) {
        Ok(()) => pdfcraft_engine::source_page_count(byte, None).filter(|n| *n > 0).ok_or("illeggibile"),
        Err(SourceProblem::Password | SourceProblem::WrongPassword) => Err("password"),
        Err(SourceProblem::NotPermitted) => Err("permessi del PDF"),
        Err(SourceProblem::Unreadable(_)) => Err("illeggibile"),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::motore::{Evento, Motore};
    use crate::prova::{Pagina, firmato, pdf};
    use std::sync::Mutex;
    use std::time::Duration;

    type Esito = Result<usize, String>;
    const ATTESA: Duration = Duration::from_secs(20);

    /// Quattro pagine col loro numero scritto: «Pagina 1» e così via, la terza orizzontale.
    fn quattro() -> Vec<u8> {
        let testi = [
            "BT /F1 24 Tf 60 700 Td (Pagina 1) Tj ET",
            "BT /F1 24 Tf 60 700 Td (Pagina 2) Tj ET",
            "BT /F1 24 Tf 60 500 Td (Pagina 3) Tj ET",
            "BT /F1 24 Tf 60 700 Td (Pagina 4) Tj ET",
        ];
        let pagine: Vec<Pagina<'_>> = testi
            .iter()
            .enumerate()
            .map(|(i, c)| {
                if i == 2 {
                    Pagina { larghezza: 842.0, altezza: 595.0, rotazione: 0, contenuto: c }
                } else {
                    Pagina { larghezza: 595.0, altezza: 842.0, rotazione: 0, contenuto: c }
                }
            })
            .collect();
        pdf(&pagine)
    }

    /// Il testo di ogni pagina, in ordine: per sapere dove sono finite.
    fn ordine(byte: &[u8], password: Option<&str>) -> Vec<String> {
        let p = Pdf::apri(byte.to_vec(), password).unwrap();
        (0..p.pagine()).map(|i| p.testo(i).map(|t| t.glyphs.iter().map(|g| g.text.as_str()).collect::<String>()).unwrap_or_default()).collect()
    }

    fn motore(byte: Vec<u8>) -> Motore<Esito> {
        Motore::avvia(Pdf::apri(byte, None).unwrap(), Arc::new(|| {})).unwrap()
    }

    fn salvato(m: &mut Motore<Esito>, pulito: bool) -> Vec<u8> {
        let dove = Arc::new(Mutex::new(Vec::new()));
        let d = dove.clone();
        let scrittura = Box::new(move |b: &Arc<Vec<u8>>| {
            *d.lock().unwrap() = b.to_vec();
            Ok(b.len())
        });
        if pulito {
            m.scrivi_pulito(scrittura)
        } else {
            m.scrivi(scrittura, true)
        };
        let fatti = m.aspetta(ATTESA);
        assert!(matches!(fatti.last().map(|f| &f.evento), Some(Evento::Scritta { segnata: true, .. })), "non salvato");
        dove.lock().unwrap().clone()
    }

    fn applica(m: &mut Motore<Esito>, modifica: Modifica) {
        m.applica(modifica, true);
        let fatti = m.aspetta(ATTESA);
        assert!(matches!(fatti[0].evento, Evento::Applicata { .. }), "la modifica non è stata applicata");
    }

    #[test]
    fn il_posto_dello_spostamento() {
        // Davanti alla pagina che oggi sta al posto 3, senza le pagine che si spostano.
        assert_eq!(posto_dopo_lo_spostamento(&[0], 3), 2);
        assert_eq!(posto_dopo_lo_spostamento(&[3], 0), 0);
        assert_eq!(posto_dopo_lo_spostamento(&[1, 2], 4), 2);
        assert_eq!(posto_dopo_lo_spostamento(&[2, 1, 2], 4), 2);
        // Lasciate dove sono: niente da fare.
        assert!(!sposta_qualcosa(&[1], 1) && !sposta_qualcosa(&[1], 2) && !sposta_qualcosa(&[1, 2], 3));
        assert!(sposta_qualcosa(&[1], 0) && sposta_qualcosa(&[1], 3) && sposta_qualcosa(&[0, 2], 1));
    }

    #[test]
    fn le_pagine_si_dicono_e_gli_intervalli_si_leggono() {
        assert_eq!(pagine_dette(&[4, 0, 1, 2, 6, 7]), "1-3, 5, 7-8");
        assert_eq!(pagine_dette(&[3]), "4");
        assert_eq!(intervalli("1-3, 4-8", 10), Some(vec![vec![0, 1, 2], vec![3, 4, 5, 6, 7]]));
        assert_eq!(intervalli(" 5 ;9- ", 10), Some(vec![vec![4], vec![8, 9]]));
        assert_eq!(intervalli("1 - 2 3", 3), Some(vec![vec![0, 1], vec![2]]));
        for sbagliato in ["", " , ", "0-2", "3-1", "1-11", "a", "2-x", "1--2"] {
            assert_eq!(intervalli(sbagliato, 10), None, "«{sbagliato}»");
        }
        assert_eq!(ogni(3, 7), vec![vec![0, 1, 2], vec![3, 4, 5], vec![6]]);
        assert_eq!(ogni(0, 2), vec![vec![0], vec![1]]);
        assert_eq!(ogni(5, 0), Vec::<Vec<usize>>::new());
    }

    #[test]
    fn eliminare_duplicare_spostare_e_la_pagina_bianca_si_annullano() {
        let mut m = motore(quattro());
        applica(&mut m, Modifica::elimina(vec![1]));
        assert_eq!(m.stato().misure.len(), 3);
        applica(&mut m, Modifica::duplica(vec![0]));
        assert_eq!(m.stato().misure.len(), 4);
        // La quarta in cima: davanti alla pagina che sta al posto 0.
        applica(&mut m, Modifica::sposta(vec![3], 0));
        // Una bianca dopo la terza (quella orizzontale, che adesso è la quarta), grande come lei.
        let vicina = m.stato().misure[3];
        assert_eq!((vicina.larghezza, vicina.altezza), (842.0, 595.0));
        applica(&mut m, Modifica::pagina_bianca(4, vicina));
        assert_eq!(m.stato().misure[4], vicina);
        let byte = salvato(&mut m, false);
        assert_eq!(ordine(&byte, None), ["Pagina 4", "Pagina 1", "Pagina 1", "Pagina 3", ""]);
        // Tutto si annulla, una modifica alla volta.
        for _ in 0..4 {
            m.annulla();
        }
        m.aspetta(ATTESA);
        assert_eq!(m.stato().misure.len(), 4);
        assert!(!m.stato().annulla);
        // Togliere tutte le pagine il motore non lo fa.
        m.applica(Modifica::elimina(vec![0, 1, 2, 3]), true);
        assert!(matches!(m.aspetta(ATTESA)[0].evento, Evento::NonApplicata { .. }));
    }

    #[test]
    fn un_altro_pdf_si_unisce_in_fondo_o_dopo_una_pagina() {
        let altro = Arc::new(pdf(&[Pagina { larghezza: 300.0, altezza: 400.0, rotazione: 0, contenuto: "BT /F1 24 Tf 20 300 Td (Altro) Tj ET" }]));
        assert_eq!(si_unisce(&altro), Ok(1));
        assert_eq!(si_unisce(&Arc::new(crate::prova::cifrato("Segreta1"))), Err("password"));
        assert_eq!(si_unisce(&Arc::new(b"%PDF-1.7 niente".to_vec())), Err("illeggibile"));
        let mut m = motore(quattro());
        applica(&mut m, Modifica::unisci("Allegato Zanzibar.pdf", altro.clone(), 4));
        applica(&mut m, Modifica::unisci("Allegato Zanzibar.pdf", altro, 1));
        let byte = salvato(&mut m, false);
        assert_eq!(ordine(&byte, None), ["Pagina 1", "Altro", "Pagina 2", "Pagina 3", "Pagina 4", "Altro"]);
        assert_eq!(m.stato().misure[1], Misura { larghezza: 300.0, altezza: 400.0 });
    }

    #[test]
    fn la_password_si_mette_e_si_toglie_col_salvataggio() {
        let mut m = motore(quattro());
        assert!(!m.stato().cifrato);
        applica(&mut m, Modifica::proteggi("Zanzibar 1"));
        assert!(m.stato().cifrato && m.stato().modificato);
        let byte = salvato(&mut m, false);
        // Riaperto, chiede la password; con lei, AES-256 e le pagine di prima.
        assert!(Pdf::apri(byte.clone(), None).err().unwrap().chiede_password());
        assert_eq!(ordine(&byte, Some("Zanzibar 1"))[3], "Pagina 4");
        let riaperto = Pdf::apri(byte.clone(), Some("Zanzibar 1")).unwrap();
        assert_eq!(riaperto.documento().security_summary().map(|s| s.method), Some("AES, 256-bit".to_string()));
        // Il lettore disegna ancora il documento protetto, coi byte e la configurazione nuovi.
        let ist = m.stato().clone();
        let mut d = pdfcraft_render::PageRenderer::new(ist.display.clone(), ist.configurazione.clone());
        let r = d.render(pdfcraft_render::RenderRequest {
            page: 0,
            kind: pdfcraft_render::RequestKind::Pixels,
            tile: None,
            scale: 0.2,
            tag: 0,
            preview: true,
        });
        assert!(crate::disegno::immagine(r).is_ok(), "la pagina protetta non si disegna");
        // Togliere la protezione: dopo il salvataggio si apre senza password.
        applica(&mut m, Modifica::togli_protezione());
        assert!(!m.stato().cifrato);
        let byte = salvato(&mut m, false);
        assert_eq!(ordine(&byte, None).len(), 4);
        // Anche un PDF arrivato con la password, aperto con lei.
        let mut m: Motore<Esito> = Motore::avvia(Pdf::apri(crate::prova::cifrato("Segreta1"), Some("Segreta1")).unwrap(), Arc::new(|| {})).unwrap();
        assert!(m.stato().cifrato);
        applica(&mut m, Modifica::togli_protezione());
        assert!(Pdf::apri(salvato(&mut m, false), None).is_ok());
        // Una password vuota il motore non la prende.
        let mut m = motore(quattro());
        m.applica(Modifica::proteggi(""), true);
        assert!(matches!(m.aspetta(ATTESA)[0].evento, Evento::NonApplicata { .. }));
    }

    #[test]
    fn le_pagine_estratte_da_un_pdf_con_la_password_la_chiedono_anche_loro() {
        // Analisi del 27/10, critico 3: Estrai e Dividi scrivevano i file nuovi in chiaro, nella
        // memoria che leggono tutte le app.
        let pdf = Pdf::apri(crate::prova::cifrato("Segreta1"), Some("Segreta1")).unwrap();
        let estratte = pdf.estrai(&[0]).unwrap().to_vec();
        assert!(Pdf::apri(estratte.clone(), None).err().is_some_and(|e| e.chiede_password()), "si apre senza password");
        assert_eq!(Pdf::apri(estratte, Some("Segreta1")).unwrap().pagine(), 1);
        // Un PDF senza password dà pagine senza password.
        let libere = Pdf::apri(quattro(), None).unwrap().estrai(&[0, 2]).unwrap().to_vec();
        assert_eq!(Pdf::apri(libere, None).unwrap().pagine(), 2);
    }

    #[test]
    fn salva_e_salva_pulito_tolgono_le_pagine_eliminate_anche_dalle_revisioni() {
        let segreta = pdf(&[
            Pagina { larghezza: 595.0, altezza: 842.0, rotazione: 0, contenuto: "BT /F1 24 Tf 60 700 Td (Copertina) Tj ET" },
            Pagina { larghezza: 595.0, altezza: 842.0, rotazione: 0, contenuto: "BT /F1 24 Tf 60 700 Td (Diagnosi Zanzibar) Tj ET" },
        ]);
        let mut m = motore(segreta);
        applica(&mut m, Modifica::elimina(vec![1]));
        // Salva e Salva pulito: tutto riscritto, una revisione sola, la pagina tolta non c'è più.
        for pulito in [false, true] {
            let byte = salvato(&mut m, pulito);
            let testo = String::from_utf8_lossy(&byte);
            assert!(!testo.contains("Zanzibar"), "la pagina tolta è rimasta nel file (pulito {pulito})");
            assert_eq!(testo.matches("%%EOF").count(), 1);
            assert_eq!(ordine(&byte, None), ["Copertina"]);
        }
        assert!(!m.stato().modificato && m.stato().annulla, "dopo Salva pulito si annulla ancora");
        // Su un firmato il motore non riscrive tutto.
        let mut m = motore(firmato(quattro(), None));
        m.scrivi_pulito(Box::new(|b: &Arc<Vec<u8>>| Ok(b.len())));
        assert!(matches!(m.aspetta(ATTESA)[0].evento, Evento::NonScritta { errore: "firma digitale" }));
    }

    #[test]
    fn estrarre_e_dividere_fanno_pdf_nuovi_dai_byte_di_adesso() {
        let mut m = motore(quattro());
        applica(&mut m, Modifica::sposta(vec![3], 0));
        // Il lavoro dai byte dell'istantanea, su un altro thread: il documento resta com'è.
        let ist = m.stato().clone();
        let byte = std::thread::spawn(move || {
            let p = Pdf::apri_condiviso(ist.display.clone(), ist.configurazione.password.as_deref()).unwrap();
            let estratte = p.estrai(&[0, 2]).unwrap();
            let parti: Vec<Arc<Vec<u8>>> = intervalli("1-2, 3-", 4).unwrap().iter().map(|v| p.estrai(v).unwrap()).collect();
            (estratte, parti, p.estrai(&[]))
        })
        .join()
        .unwrap();
        assert_eq!(ordine(&byte.0, None), ["Pagina 4", "Pagina 2"]);
        assert_eq!(byte.1.len(), 2);
        assert_eq!(ordine(&byte.1[0], None), ["Pagina 4", "Pagina 1"]);
        assert_eq!(ordine(&byte.1[1], None), ["Pagina 2", "Pagina 3"]);
        assert_eq!(byte.2, Err("nessuna pagina"));
        assert_eq!(m.stato().misure.len(), 4);
    }

    #[test]
    fn i_tre_livelli_comprimono_sempre_di_piu() {
        let byte = crate::prova::pdf_con_foto();
        let p = Pdf::apri(byte.clone(), None).unwrap();
        let mut misure = Vec::new();
        for livello in Livello::TUTTI {
            let c = p.compressione(livello).unwrap();
            assert_eq!(c.prima, byte.len());
            let fatti = Mutex::new(Vec::new());
            let compresso = c
                .esegui(&|a, b| {
                    fatti.lock().unwrap().push((a, b));
                    true
                })
                .unwrap();
            assert!(fatti.lock().unwrap().iter().all(|(a, b)| *b == 1000 && *a <= 1000));
            assert_eq!(Pdf::apri(compresso.to_vec(), None).unwrap().pagine(), 1);
            misure.push(compresso.len());
        }
        assert!(misure[0] < byte.len() && misure[1] < misure[0] && misure[2] < misure[1], "{misure:?} da {}", byte.len());
        // Fermata a metà: niente copia.
        assert_eq!(p.compressione(Livello::Media).unwrap().esegui(&|_, _| false).err(), Some("fermata"));
        // Un firmato no.
        let f = Pdf::apri(firmato(quattro(), None), None).unwrap();
        assert_eq!(f.compressione(Livello::Media).err(), Some("firma digitale"));
        assert_eq!(Livello::Forte.nome(), "Forte");
    }
}
