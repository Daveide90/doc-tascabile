//! «Riconosci testo» (piano «PDF completo», P8): le pagine disegnate a 300 dpi vanno al
//! riconoscitore di `tascabile-ocr` (PP-OCRv6 di PaddleOCR, italiano con gli accenti) e le parole
//! lette diventano lo strato di testo invisibile del motore (`Edit::AddOcrText`, modo di resa 3,
//! Helvetica WinAnsi, ogni parola stirata sul suo riquadro): l'immagine non cambia, il testo si
//! cerca, si seleziona e si copia. Tutto in un passo che si annulla, «Riconosci testo».
//!
//! Il lavoro è lento (secondi a pagina sul telefono): [`LavoroOcr`] prende dall'istantanea quello
//! che serve e gira su un thread suo, con l'avanzamento e la richiesta di fermarsi; la modifica
//! va poi al thread del motore come le altre. Le pagine che hanno già testo si saltano, come fa
//! Acrobat («la pagina contiene testo»), ma non le scansioni con pochi segni sopra (vedi
//! [`com_e_la_pagina`]).

use std::sync::Arc;

use pdfcraft_engine::Edit;
pub use pdfcraft_engine::ocr::PlacedWord;
use pdfcraft_render::{PageInfo, PageRenderer, RenderConfig, RenderRequest, RequestKind};
pub use tascabile_ocr::{ErroreOcr, Modelli, Ocr, Parola, Riga};

use crate::modifica::Modifica;
use crate::motore::Istantanea;

/// La risoluzione delle pagine da leggere: quella di una scansione buona.
pub const DPI: f32 = 300.0;
/// Il lato più lungo della pagina disegnata, in pixel: un foglio più grande di un A4 si legge a
/// meno di 300 dpi (35 MB di pixel al massimo, come un A4 a 300 dpi).
const LATO_MASSIMO: f32 = 3600.0;

/// Una scansione: un'immagine che copre almeno questa parte della pagina.
const SCANSIONE: f64 = 0.5;
/// Sopra una scansione il timbro di uno scanner («Scanned with CamScanner») ha poche lettere:
/// con più di così la pagina ha un testo suo.
const LETTERE_DI_UN_TIMBRO: usize = 40;

/// Com'è una pagina per «Riconosci testo».
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pagina {
    DaLeggere,
    /// Ha già il suo testo: scritto (un PDF fatto al computer) o riconosciuto prima (lo strato
    /// invisibile di un OCR, nostro o dello scanner).
    ConTesto,
}

/// La pagina `pagina` ha il testo suo, o va letta: una scansione si legge anche con un timbro
/// sopra. `d` è senza i segni del modo Modifica (vedi [`senza_i_segni`]). Prima bastava un glifo
/// qualunque (il numero di pagina messo dall'app, una filigrana) per saltarla (analisi del 27/10,
/// pagine I4).
fn com_e_la_pagina(d: &pdfcraft_engine::Document, pagina: usize, info: &PageInfo) -> Pagina {
    let blocchi = d.text_blocks(pagina);
    if blocchi.iter().any(|b| b.invisible) {
        return Pagina::ConTesto;
    }
    let lettere: usize = blocchi.iter().map(|b| b.text.chars().filter(|c| !c.is_whitespace()).count()).sum();
    if lettere == 0 {
        return Pagina::DaLeggere;
    }
    let pagina_intera = f64::from(info.width) * f64::from(info.height);
    let scansione = d.page_images(pagina).iter().any(|i| {
        let r = i.visible_rect.unwrap_or(i.rect);
        (r[2] - r[0]).abs() * (r[3] - r[1]).abs() >= SCANSIONE * pagina_intera
    });
    if scansione && lettere <= LETTERE_DI_UN_TIMBRO { Pagina::DaLeggere } else { Pagina::ConTesto }
}

/// Il documento di `byte` per guardare le pagine, senza i numeri, le intestazioni, la filigrana e
/// lo sfondo messi col modo Modifica: non sono il testo della pagina. È una copia di questo
/// thread (quello del motore sta sul suo) e non si salva mai.
fn senza_i_segni(byte: Arc<Vec<u8>>, password: Option<&str>) -> Option<crate::Pdf> {
    let mut pdf = crate::Pdf::apri_condiviso(byte, password).ok()?;
    for kind in pdf.documento().marks().to_vec() {
        let _ = pdf.sessione.apply(pdf.id, Edit::RemoveMarks { kind });
    }
    Some(pdf)
}

/// Le parole lette nei pixel di una pagina portate nello spazio del PDF: `scala` è quanti pixel
/// per punto ha l'immagine della pagina come si vede (`info`, con la sua rotazione).
pub fn parole_nella_pagina(righe: &[Riga], info: &PageInfo, scala: f32) -> Vec<PlacedWord> {
    let scala = scala.max(1e-3);
    let nello_spazio = |x: f32, y: f32| {
        let [u, v] = info.view_to_user(x / scala, y / scala);
        [u as f64, v as f64]
    };
    righe
        .iter()
        .flat_map(|r| &r.parole)
        .map(|p| {
            let [l, t, d, b] = p.riquadro;
            let o = nello_spazio(l, b);
            let br = nello_spazio(d, b);
            let tl = nello_spazio(l, t);
            PlacedWord { text: p.testo.clone(), origin: o, across: [br[0] - o[0], br[1] - o[1]], up: [tl[0] - o[0], tl[1] - o[1]] }
        })
        .collect()
}

/// La modifica che scrive le parole lette, pagina per pagina, in un passo solo; niente se non
/// c'è nessuna parola.
pub fn modifica_ocr(lette: &[(usize, Vec<PlacedWord>)]) -> Option<Modifica> {
    let edits: Vec<Edit> =
        lette.iter().filter(|(_, p)| !p.is_empty()).map(|(pagina, parole)| Edit::AddOcrText { page: *pagina, words: parole.clone() }).collect();
    (!edits.is_empty()).then(|| Modifica::del_motore(Edit::Batch { label: "Riconosci testo".into(), edits }))
}

/// Com'è andato il riconoscimento di un documento.
#[derive(Debug, Default)]
pub struct EsitoOcr {
    /// Per pagina (da 0) le parole lette, già nello spazio del PDF.
    pub lette: Vec<(usize, Vec<PlacedWord>)>,
    /// Le pagine lasciate com'erano perché hanno già testo.
    pub con_testo: usize,
    /// Le pagine che non si sono potute disegnare o leggere.
    pub non_lette: usize,
}

impl EsitoOcr {
    pub fn parole(&self) -> usize {
        self.lette.iter().map(|(_, p)| p.len()).sum()
    }

    pub fn modifica(&self) -> Option<Modifica> {
        modifica_ocr(&self.lette)
    }
}

impl crate::Pdf {
    /// «Esporta in Word»: il documento come `.docx` del motore (`export_office`), paragrafi in
    /// ordine di lettura e immagini, una sezione per pagina; le pagine scansionate danno testo
    /// solo dopo l'OCR. Lavoro lento: fuori dal thread dell'interfaccia.
    pub fn in_word(&self) -> Vec<u8> {
        self.documento().export_office(pdfcraft_engine::compare::OfficeFormat::Docx)
    }

    /// Il testo riconosciuto, per un `.docx` da modificare: per ogni pagina i suoi paragrafi,
    /// quelli dello strato invisibile dell'OCR (sulle scansioni il testo che si vede sono i segni
    /// e i timbri) o, nelle pagine non riconosciute, quelli che si vedono. Niente se nessuna
    /// pagina è riconosciuta: allora va il `.docx` del motore, che tiene immagini e formato ma
    /// mette le parole dell'OCR in caselle sopra la scansione, a pezzi (analisi del 27/10,
    /// pagine I6).
    pub fn testo_riconosciuto(&self) -> Option<Vec<Vec<String>>> {
        let d = self.documento();
        let mut riconosciuta = false;
        let pagine = (0..self.pagine())
            .map(|pagina| {
                let blocchi = d.text_blocks(pagina);
                let invisibili = blocchi.iter().any(|b| b.invisible);
                riconosciuta |= invisibili;
                blocchi.iter().filter(|b| b.invisible == invisibili && !b.text.trim().is_empty()).map(|b| b.text.trim().to_string()).collect()
            })
            .collect();
        riconosciuta.then_some(pagine)
    }
}

/// Quello che serve per riconoscere le pagine di un documento, staccato dal thread del motore.
pub struct LavoroOcr {
    byte: Arc<Vec<u8>>,
    configurazione: RenderConfig,
    geometrie: Arc<Vec<PageInfo>>,
    pub pagine: Vec<usize>,
}

impl LavoroOcr {
    /// Le pagine `pagine` (da 0; vuoto vuol dire tutte) del documento come lo dice l'istantanea.
    pub fn nuovo(istantanea: &Istantanea, pagine: &[usize]) -> LavoroOcr {
        let tutte = istantanea.geometrie.len();
        let pagine = if pagine.is_empty() { (0..tutte).collect() } else { pagine.iter().copied().filter(|p| *p < tutte).collect() };
        LavoroOcr {
            byte: istantanea.display.clone(),
            configurazione: istantanea.configurazione.clone(),
            geometrie: istantanea.geometrie.clone(),
            pagine,
        }
    }

    /// Legge le pagine. `avanti(fatte, totale)` si chiama prima di ogni pagina e fra una riga e
    /// l'altra (con le stesse cifre): se dice no ci si ferma, con [`ErroreOcr::Fermato`].
    pub fn esegui(self, ocr: &Ocr, avanti: &(dyn Fn(usize, usize) -> bool + Sync)) -> Result<EsitoOcr, ErroreOcr> {
        let mut disegnatore = PageRenderer::new(self.byte.clone(), self.configurazione.clone());
        // Se il documento non si apre qui, conta come testo ogni glifo, come prima.
        let documento = senza_i_segni(self.byte.clone(), self.configurazione.password.as_deref());
        let totale = self.pagine.len();
        let mut esito = EsitoOcr::default();
        for (fatte, &pagina) in self.pagine.iter().enumerate() {
            if !avanti(fatte, totale) {
                return Err(ErroreOcr::Fermato);
            }
            let Some(info) = self.geometrie.get(pagina) else { continue };
            let com_e = match &documento {
                Some(pdf) => com_e_la_pagina(pdf.documento(), pagina, info),
                None => {
                    let testo = disegnatore.render(RenderRequest { page: pagina, kind: RequestKind::Text, scale: 1.0, ..Default::default() });
                    if testo.text.is_some_and(|t| t.glyphs.iter().any(|g| !g.text.trim().is_empty())) { Pagina::ConTesto } else { Pagina::DaLeggere }
                }
            };
            if com_e == Pagina::ConTesto {
                esito.con_testo += 1;
                continue;
            }
            let lato = info.width.max(info.height).max(1.0);
            let scala = (DPI / 72.0).min(LATO_MASSIMO / lato);
            let foto = disegnatore.render(RenderRequest { page: pagina, scale: scala, ..Default::default() });
            if foto.error.is_some() || foto.width == 0 {
                esito.non_lette += 1;
                continue;
            }
            let righe = match ocr.riconosci(&foto.rgba, foto.width, foto.height, &|| !avanti(fatte, totale)) {
                Ok(r) => r,
                Err(ErroreOcr::Fermato) => return Err(ErroreOcr::Fermato),
                Err(_) => {
                    esito.non_lette += 1;
                    continue;
                }
            };
            let scala_vera = foto.width as f32 / info.width.max(1e-3);
            esito.lette.push((pagina, parole_nella_pagina(&righe, info, scala_vera)));
        }
        avanti(totale, totale);
        Ok(esito)
    }
}

#[cfg(test)]
pub(crate) mod test {
    use super::*;
    use crate::prova::{FRASI, PAROLE, Pagina, pagina_italiana, pdf, scansione_italiana};
    use std::sync::OnceLock;

    /// Il riconoscitore coi modelli veri di `crates/ocr/modelli`, caricato una volta per tutte
    /// le prove.
    pub(crate) fn ocr() -> &'static Ocr {
        static OCR: OnceLock<Ocr> = OnceLock::new();
        OCR.get_or_init(|| {
            let modelli = Modelli::da_cartella(&tascabile_ocr::cartella_dei_modelli()).expect("modelli in crates/ocr/modelli");
            Ocr::carica(modelli).expect("modelli caricati")
        })
    }

    fn testo(pdf: &crate::Pdf, pagina: usize) -> Option<Arc<pdfcraft_render::PageText>> {
        let ist = pdf.istantanea();
        let mut r = PageRenderer::new(ist.display.clone(), ist.configurazione.clone());
        r.render(RenderRequest { page: pagina, kind: RequestKind::Text, scale: 1.0, ..Default::default() }).text
    }

    #[test]
    fn le_accentate_si_leggono_giuste() {
        let pdf = crate::Pdf::apri(pagina_italiana(), None).expect("aperto");
        let foto = pdf.disegnatore().disegna(0, DPI / 72.0).expect("disegnata");
        let rgba: Vec<u8> = foto.pixels.iter().flat_map(|p| p.to_array()).collect();
        let inizio = std::time::Instant::now();
        let righe = ocr().riconosci(&rgba, foto.size[0] as u32, foto.size[1] as u32, &|| false).expect("riconosciuta");
        eprintln!("OCR di un A4 a 300 dpi sul PC: {:?}, {} righe", inizio.elapsed(), righe.len());
        let lette: Vec<String> = righe.iter().map(Riga::testo).collect();
        let tutte = lette.join(" ");
        for p in PAROLE {
            assert!(tutte.split(|c: char| c.is_whitespace() || ",.:".contains(c)).any(|w| w == p), "manca «{p}» in {lette:?}");
        }
        assert_eq!(lette[0], "perché però più città");
    }

    #[test]
    fn lo_strato_di_testo_si_cerca_e_l_annullare_lo_toglie() {
        let mut pdf = crate::Pdf::apri(scansione_italiana(), None).expect("aperto");
        assert!(testo(&pdf, 0).is_none_or(|t| t.glyphs.is_empty()), "la scansione non ha testo");
        let lavoro = LavoroOcr::nuovo(&pdf.istantanea(), &[]);
        let chiamate = std::sync::atomic::AtomicUsize::new(0);
        let esito = lavoro
            .esegui(ocr(), &|_, _| {
                chiamate.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                true
            })
            .expect("letto");
        assert!(chiamate.load(std::sync::atomic::Ordering::Relaxed) >= 2);
        assert_eq!((esito.con_testo, esito.non_lette), (0, 0));
        assert!(esito.parole() >= 15, "{}", esito.parole());
        let modifica = esito.modifica().expect("parole");
        pdf.sessione.apply(pdf.id, modifica.edit.clone()).expect("applicata");
        let t = testo(&pdf, 0).expect("testo");
        for p in ["più", "città", "perché"] {
            assert!(!crate::ricerca::trova_nella_pagina(&t, &crate::ricerca::ago(p).expect("ago")).is_empty(), "«{p}» non si trova");
        }
        // Anche senza accenti, come cerca chi scrive in fretta.
        assert!(!crate::ricerca::trova_nella_pagina(&t, &crate::ricerca::ago("citta").expect("ago")).is_empty());
        // Per Word le righe lette vengono intere, nell'ordine.
        let paragrafi = pdf.testo_riconosciuto().expect("riconosciuto").concat().join(" ");
        assert!(paragrafi.contains("perché però più città") && paragrafi.contains("Nove giorni fa"), "{paragrafi:?}");
        // La seconda volta la pagina ha già testo e si salta.
        let di_nuovo = LavoroOcr::nuovo(&pdf.istantanea(), &[0]).esegui(ocr(), &|_, _| true).expect("letto");
        assert_eq!((di_nuovo.con_testo, di_nuovo.parole()), (1, 0));
        assert!(di_nuovo.modifica().is_none());
        pdf.sessione.undo(pdf.id).expect("annullata");
        assert!(testo(&pdf, 0).is_none_or(|t| t.glyphs.is_empty()));
    }

    #[test]
    fn una_scansione_coi_numeri_o_la_filigrana_si_riconosce_lo_stesso() {
        // Analisi del 27/10, pagine I4: bastava un glifo qualunque (il numero di pagina messo
        // dall'app, una filigrana, il timbro di uno scanner) e la scansione si saltava come una
        // pagina che ha già il suo testo.
        let intestazione = "Azienda sanitaria locale · Ufficio protocollo generale";
        for segno in [Modifica::numeri_di_pagina(1, intestazione, "Pagina <<1>> di <<n>>"), Modifica::filigrana(1, "COPIA CONFORME", 0.3)] {
            let mut pdf = crate::Pdf::apri(scansione_italiana(), None).expect("aperto");
            pdf.sessione.apply(pdf.id, segno.edit.clone()).expect("il segno");
            assert!(testo(&pdf, 0).is_some_and(|t| !t.glyphs.is_empty()), "il segno ha il suo testo");
            let esito = LavoroOcr::nuovo(&pdf.istantanea(), &[]).esegui(ocr(), &|_, _| true).expect("letto");
            assert_eq!(esito.con_testo, 0);
            assert!(esito.parole() >= 15, "{}", esito.parole());
        }
        // Una pagina col testo vero resta com'è, anche con un'immagine grande quanto la pagina e i
        // numeri messi dall'app.
        let mut pdf = crate::Pdf::apri(pagina_italiana(), None).expect("aperto");
        let esito = LavoroOcr::nuovo(&pdf.istantanea(), &[]).esegui(ocr(), &|_, _| true).expect("letto");
        assert_eq!((esito.con_testo, esito.parole()), (1, 0));
        let foto = Arc::new(crate::prova::foto_di_una_firma(600, 850, 60));
        let sfondo = Edit::AddImage { page: 0, rect: Some([0.0, 0.0, 595.0, 842.0]), name: "foto.jpg".into(), bytes: foto };
        pdf.sessione.apply(pdf.id, sfondo).expect("l'immagine");
        pdf.sessione.apply(pdf.id, Modifica::numeri_di_pagina(1, "", "Pagina <<1>> di <<n>>").edit).expect("i numeri");
        assert!(pdf.documento().page_images(0).iter().any(|i| i.rect[2] - i.rect[0] > 590.0), "l'immagine grande");
        let esito = LavoroOcr::nuovo(&pdf.istantanea(), &[]).esegui(ocr(), &|_, _| true).expect("letto");
        assert_eq!((esito.con_testo, esito.parole()), (1, 0));
    }

    /// Il tempo per pagina sul PC, su un A4 pieno (40 righe in Helvetica 11 pt): a mano, con
    /// `cargo test -p tascabile-pdf --release tempo_per_pagina -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn tempo_per_pagina() {
        let mut c = String::from("BT /F1 11 Tf 56 790 Td 17.5 TL ");
        for i in 0..40 {
            let f = FRASI[i % FRASI.len()];
            c.push_str(&format!("({i}. {f} {f}) Tj T* "));
        }
        c.push_str("ET");
        let pdf = crate::Pdf::apri(pdf(&[Pagina { larghezza: 595.0, altezza: 842.0, rotazione: 0, contenuto: &c }]), None).expect("aperto");
        let foto = pdf.disegnatore().disegna(0, DPI / 72.0).expect("disegnata");
        let rgba: Vec<u8> = foto.pixels.iter().flat_map(|p| p.to_array()).collect();
        let _ = ocr();
        for giro in 0..3 {
            let inizio = std::time::Instant::now();
            let righe = ocr().riconosci(&rgba, foto.size[0] as u32, foto.size[1] as u32, &|| false).expect("riconosciuta");
            let giuste = righe.iter().filter(|r| r.testo().contains("perché però più città") || r.testo().contains("libertà")).count();
            eprintln!("giro {giro}: {:?} per un A4 pieno a 300 dpi, {} righe, {giuste} con le accentate giuste", inizio.elapsed(), righe.len());
        }
    }

    #[test]
    fn in_word_viene_un_docx() {
        let docx = crate::Pdf::apri(pagina_italiana(), None).expect("aperto").in_word();
        assert!(docx.starts_with(b"PK") && docx.len() > 1000, "{} byte", docx.len());
        assert!(docx.windows(17).any(|w| w == b"word/document.xml"));
    }

    #[test]
    fn il_testo_riconosciuto_viene_a_paragrafi() {
        // Analisi del 27/10, pagine I6: il .docx del motore mette le parole dell'OCR in caselle
        // sopra l'immagine, che si leggono a pezzi. Il testo per Word viene dallo strato invisibile.
        let pdf = crate::Pdf::apri(crate::prova::scansione_riconosciuta(), None).expect("aperto");
        let pagine = pdf.testo_riconosciuto().expect("riconosciuto");
        assert_eq!(pagine.len(), 1);
        let tutto = pagine[0].join("\n");
        for riga in crate::prova::RIGHE {
            assert!(tutto.contains(riga), "manca «{riga}» in {:?}", pagine[0]);
        }
        // Un PDF fatto al computer, o una scansione non riconosciuta: va il .docx del motore.
        assert!(crate::Pdf::apri(pagina_italiana(), None).expect("aperto").testo_riconosciuto().is_none());
        assert!(crate::Pdf::apri(scansione_italiana(), None).expect("aperto").testo_riconosciuto().is_none());
    }

    #[test]
    fn fermarsi_non_scrive_niente() {
        let pdf = crate::Pdf::apri(scansione_italiana(), None).expect("aperto");
        let lavoro = LavoroOcr::nuovo(&pdf.istantanea(), &[0, 7]);
        assert_eq!(lavoro.pagine, vec![0]);
        assert!(matches!(lavoro.esegui(ocr(), &|_, _| false), Err(ErroreOcr::Fermato)));
    }

    #[test]
    fn le_parole_vanno_nello_spazio_della_pagina() {
        // 2 pixel per punto, A4 diritto: y verso l'alto.
        let info = crate::Pdf::apri(pdf(&[Pagina::bianca(595.0, 842.0)]), None).expect("aperto").istantanea().geometrie[0].clone();
        let righe = [Riga { parole: vec![Parola { testo: "più".into(), riquadro: [100.0, 200.0, 300.0, 240.0] }] }];
        let p = &parole_nella_pagina(&righe, &info, 2.0)[0];
        let vicini = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3;
        assert!(vicini(p.origin, [50.0, 842.0 - 120.0]), "{:?}", p.origin);
        assert!(vicini(p.across, [100.0, 0.0]), "{:?}", p.across);
        assert!(vicini(p.up, [0.0, 20.0]), "{:?}", p.up);
        assert!(modifica_ocr(&[(0, Vec::new())]).is_none());
        let m = modifica_ocr(&[(0, vec![p.clone()])]).expect("modifica");
        assert_eq!(m.toccate, crate::Toccate::Alcune(vec![0]));
    }
}
