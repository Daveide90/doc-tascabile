//! Il riconoscimento del testo di Tascabile, in italiano con gli accenti e senza internet (piano
//! «PDF completo», P8; ricerca 2026-10-24-pdf-modifica-ocr, sezioni 4, 5 e 7, via B).
//!
//! La stessa strada della PR 1012 di PDFCraft, senza toccare il motore: il modello di ocrs trova
//! le parole e le mette in righe, poi PP-OCRv6 small di PaddleOCR (un modello per 50 lingue,
//! italiano compreso, Apache 2.0) legge ogni riga con RTen. La decodifica tiene solo i caratteri
//! che lo strato di testo del PDF sa scrivere (WinAnsi: le lettere accentate, «€», «°», gli
//! apostrofi tipografici), così una lettera mai vista diventa la più probabile fra quelle giuste
//! invece di un «?». Il risultato sono righe di parole coi riquadri in pixel dell'immagine; chi
//! scrive il PDF le porta nello spazio della pagina.
//!
//! I modelli arrivano come byte (sul telefono dagli asset dell'APK, sul PC dalla cartella
//! `modelli` di questo crate): niente viene scaricato mentre l'app gira.

use image::{RgbImage, imageops};
use rten_imageproc::BoundingRect;
use rten_tensor::prelude::*;
use rten_tensor::{NdTensor, Tensor};

/// I file dei modelli, con i nomi degli asset dell'APK e della cartella `modelli`.
pub const RILEVAMENTO: &str = "text-detection.rten";
pub const RICONOSCIMENTO: &str = "PP-OCRv6_small_rec.onnx";
pub const DIZIONARIO: &str = "ppocrv6_dict.txt";

/// Il dizionario vero pesa 75 KB: uno molto più grande non è il nostro.
const DIZIONARIO_MASSIMO: usize = 1_000_000;

/// L'altezza delle righe per il riconoscitore e i limiti della larghezza (inference.yml di
/// PP-OCRv6, come la PR 1012).
const ALTEZZA_RIGA: u32 = 48;
const LARGHEZZA_MINIMA: u32 = 320;
const LARGHEZZA_MASSIMA: u32 = 3200;

/// Il lato più lungo dell'immagine su cui si cercano le righe (circa 140 dpi su un A4).
const LATO_RILEVAMENTO: f32 = 1600.0;

/// Una parola letta con fiducia media più bassa di così si lascia fuori.
const FIDUCIA_MINIMA: f32 = 0.5;

/// La cartella dei modelli sul PC, per le prove e il binario desktop (sul telefono stanno negli
/// asset dell'APK: il percorso del PC non deve finire nella libreria Android).
#[cfg(not(target_os = "android"))]
pub fn cartella_dei_modelli() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("modelli")
}

/// Che cosa non è andato. Mai il testo letto né i pixel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroreOcr {
    /// Un modello manca o non si carica: quale file.
    Modello(&'static str),
    /// L'immagine è vuota o non è della misura detta.
    Immagine,
    /// Il motore dei modelli ha dato un errore (il suo genere, senza testo del documento).
    Motore(String),
    /// Chi l'ha chiesto ha detto di fermarsi.
    Fermato,
}

impl std::fmt::Display for ErroreOcr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErroreOcr::Modello(quale) => write!(f, "modello {quale} non caricato"),
            ErroreOcr::Immagine => f.write_str("immagine vuota"),
            ErroreOcr::Motore(genere) => write!(f, "motore dei modelli: {genere}"),
            ErroreOcr::Fermato => f.write_str("fermato"),
        }
    }
}

impl std::error::Error for ErroreOcr {}

/// I byte dei tre file.
pub struct Modelli {
    pub rilevamento: Vec<u8>,
    pub riconoscimento: Vec<u8>,
    pub dizionario: Vec<u8>,
}

impl Modelli {
    /// I modelli letti da `leggi` (un nome di file, i suoi byte): gli asset dell'APK o una
    /// cartella. Il primo che manca è l'errore.
    pub fn da(mut leggi: impl FnMut(&'static str) -> Option<Vec<u8>>) -> Result<Modelli, ErroreOcr> {
        let mut uno = |nome| leggi(nome).filter(|b| !b.is_empty()).ok_or(ErroreOcr::Modello(nome));
        Ok(Modelli { rilevamento: uno(RILEVAMENTO)?, riconoscimento: uno(RICONOSCIMENTO)?, dizionario: uno(DIZIONARIO)? })
    }

    /// I modelli nella cartella `cartella`.
    pub fn da_cartella(cartella: &std::path::Path) -> Result<Modelli, ErroreOcr> {
        Modelli::da(|nome| std::fs::read(cartella.join(nome)).ok())
    }
}

/// Una parola letta: il testo e il riquadro in pixel dell'immagine `[sinistra, alto, destra,
/// basso]`. Debug senza il testo, che può essere un dato personale.
#[derive(Clone, PartialEq)]
pub struct Parola {
    pub testo: String,
    pub riquadro: [f32; 4],
}

impl std::fmt::Debug for Parola {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Parola").field("caratteri", &self.testo.chars().count()).field("riquadro", &self.riquadro).finish()
    }
}

/// Una riga di parole, nell'ordine di lettura.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Riga {
    pub parole: Vec<Parola>,
}

impl Riga {
    pub fn testo(&self) -> String {
        self.parole.iter().map(|p| p.testo.as_str()).collect::<Vec<_>>().join(" ")
    }
}

/// Il riconoscitore caricato. Caricarlo costa (decine di MB di pesi): se ne tiene uno e lo si
/// riusa; si può dare ad altri thread.
pub struct Ocr {
    rilevatore: ocrs::OcrEngine,
    lettore: rten::Model,
    /// Le classi del riconoscitore: 0 è il vuoto della decodifica CTC, poi le righe del
    /// dizionario e in fondo lo spazio.
    alfabeto: Vec<String>,
    /// Per ogni classe: la si può scrivere nello strato di testo (WinAnsi).
    ammesse: Vec<bool>,
}

impl Ocr {
    pub fn carica(modelli: Modelli) -> Result<Ocr, ErroreOcr> {
        if modelli.dizionario.len() > DIZIONARIO_MASSIMO {
            return Err(ErroreOcr::Modello(DIZIONARIO));
        }
        let dizionario = String::from_utf8(modelli.dizionario).map_err(|_| ErroreOcr::Modello(DIZIONARIO))?;
        let alfabeto = alfabeto(&dizionario);
        let ammesse = alfabeto.iter().map(|c| ammessa(c)).collect();
        let rilevamento = rten::Model::load(modelli.rilevamento).map_err(|_| ErroreOcr::Modello(RILEVAMENTO))?;
        let lettore = rten::Model::load(modelli.riconoscimento).map_err(|_| ErroreOcr::Modello(RICONOSCIMENTO))?;
        let parametri = ocrs::OcrEngineParams { detection_model: Some(rilevamento), ..Default::default() };
        let rilevatore = ocrs::OcrEngine::new(parametri).map_err(|_| ErroreOcr::Modello(RILEVAMENTO))?;
        Ok(Ocr { rilevatore, lettore, alfabeto, ammesse })
    }

    /// Le righe di testo di un'immagine RGBA (`larghezza` × `altezza` pixel, 4 byte a pixel, su
    /// fondo opaco), nell'ordine di lettura. `ferma` si guarda prima di ogni riga, anche da più thread: se dice
    /// sì, [`ErroreOcr::Fermato`].
    pub fn riconosci(&self, rgba: &[u8], larghezza: u32, altezza: u32, ferma: &(dyn Fn() -> bool + Sync)) -> Result<Vec<Riga>, ErroreOcr> {
        let pixel = (larghezza as usize).checked_mul(altezza as usize).and_then(|n| n.checked_mul(4));
        if larghezza == 0 || altezza == 0 || pixel != Some(rgba.len()) {
            return Err(ErroreOcr::Immagine);
        }
        let rgb: Vec<u8> = rgba.as_chunks::<4>().0.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
        let immagine = RgbImage::from_raw(larghezza, altezza, rgb).ok_or(ErroreOcr::Immagine)?;
        let motore = |e: &dyn std::fmt::Display| ErroreOcr::Motore(genere(e));
        // Il rilevatore ha un ingresso di misura fissa: l'immagine grande gli arriva ridotta
        // comunque, ma la sua maschera torna alla misura piena e lì costa secondi. Si rileva su
        // una copia più piccola e si leggono le righe su quella vera.
        let k = (LATO_RILEVAMENTO / larghezza.max(altezza) as f32).min(1.0);
        let ridotta;
        let da_rilevare = if k < 1.0 {
            let (w, h) = (((larghezza as f32 * k).round() as u32).max(1), ((altezza as f32 * k).round() as u32).max(1));
            ridotta = imageops::thumbnail(&immagine, w, h);
            &ridotta
        } else {
            &immagine
        };
        let sorgente = ocrs::ImageSource::from_bytes(da_rilevare.as_raw(), da_rilevare.dimensions()).map_err(|_| ErroreOcr::Immagine)?;
        let ingresso = self.rilevatore.prepare_input(sorgente).map_err(|e| motore(&e))?;
        let parole = self.rilevatore.detect_words(&ingresso).map_err(|e| motore(&e))?;
        let righe = self.rilevatore.find_text_lines(&ingresso, &parole);
        let riquadri: Vec<[f32; 4]> = righe
            .iter()
            .map(|riga| {
                let r = riga.iter().map(|r| r.bounding_rect()).fold([f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY], |a, r| {
                    [a[0].min(r.left()), a[1].min(r.top()), a[2].max(r.right()), a[3].max(r.bottom())]
                });
                r.map(|v| v / k)
            })
            .collect();
        // Le righe si leggono insieme sui core del telefono: una riga da sola è un ingresso
        // piccolo, che RTen non riesce a dividere bene fra i thread.
        use rayon::prelude::*;
        let lette: Vec<Result<Vec<Parola>, ErroreOcr>> =
            riquadri.par_iter().map(|r| if ferma() { Err(ErroreOcr::Fermato) } else { self.leggi_riga(&immagine, *r) }).collect();
        let mut fuori = Vec::new();
        for parole in lette {
            let parole = parole?;
            if !parole.is_empty() {
                fuori.push(Riga { parole });
            }
        }
        Ok(fuori)
    }

    /// Le parole della riga nel riquadro `riquadro` (pixel dell'immagine).
    fn leggi_riga(&self, immagine: &RgbImage, riquadro: [f32; 4]) -> Result<Vec<Parola>, ErroreOcr> {
        let Some([x, y, w, h]) = ritaglio(immagine.width(), immagine.height(), riquadro) else {
            return Ok(Vec::new());
        };
        // Il riconoscitore vede il margine, ma lo strato di testo tiene l'altezza della riga
        // trovata, non quella del ritaglio.
        let alto = riquadro[1].clamp(0.0, immagine.height() as f32);
        let basso = riquadro[3].clamp(0.0, immagine.height() as f32);
        if basso <= alto {
            return Ok(Vec::new());
        }
        let pezzo = imageops::crop_imm(immagine, x, y, w, h).to_image();
        let rw = ((ALTEZZA_RIGA as f64 * w as f64 / h as f64).ceil() as u32).clamp(1, LARGHEZZA_MASSIMA);
        let larghezza = rw.max(LARGHEZZA_MINIMA) as usize;
        let ridotto = imageops::resize(&pezzo, rw, ALTEZZA_RIGA, imageops::FilterType::Triangle);
        // PaddleOCR vuole BGR in [-1, 1], con zeri a destra fino alla larghezza minima.
        let mut ingresso = NdTensor::<f32, 4>::zeros([1, 3, ALTEZZA_RIGA as usize, larghezza]);
        for (px, py, p) in ridotto.enumerate_pixels() {
            for c in 0..3 {
                ingresso[[0, c, py as usize, px as usize]] = p[2 - c] as f32 / 127.5 - 1.0;
            }
        }
        let uscita = self.lettore.run_one(ingresso.view().into(), None).map_err(|e| ErroreOcr::Motore(genere(&e)))?;
        let uscita: Tensor<f32> = uscita.try_into().map_err(|_| ErroreOcr::Motore("uscita non f32".into()))?;
        let forma = uscita.shape();
        if forma.len() != 3 || forma[0] != 1 || forma[2] != self.alfabeto.len() {
            return Err(ErroreOcr::Motore(format!("uscita {forma:?} con {} classi", self.alfabeto.len())));
        }
        let colonne = forma[1];
        let valori: Vec<f32> = uscita.iter().copied().collect();
        // Colonne dell'uscita in pixel del ritaglio; quelle sugli zeri a destra non contano.
        let passo = larghezza as f32 / colonne.max(1) as f32 * w as f32 / rw as f32;
        let utili = ((w as f32 / passo).ceil() as usize).min(colonne);
        let lette = decodifica(&valori[..utili * forma[2]], forma[2], &self.alfabeto, &self.ammesse);
        Ok(lette
            .into_iter()
            .map(|(testo, inizio, fine)| Parola {
                testo,
                riquadro: [x as f32 + inizio as f32 * passo, alto, (x as f32 + (fine + 1) as f32 * passo).min(immagine.width() as f32), basso],
            })
            .collect())
    }
}

/// Il genere di un errore del motore dei modelli: la sua prima riga, che parla di operatori e
/// forme, mai del documento.
fn genere(e: &dyn std::fmt::Display) -> String {
    e.to_string().lines().next().unwrap_or("").chars().take(120).collect()
}

/// Le classi del riconoscitore: il vuoto, le righe del dizionario, lo spazio.
fn alfabeto(dizionario: &str) -> Vec<String> {
    let mut a = vec![String::new()];
    a.extend(dizionario.lines().map(|r| r.trim_end_matches('\r').to_owned()));
    a.push(" ".into());
    a
}

/// I caratteri che lo strato di testo invisibile del motore sa scrivere: WinAnsi (Windows-1252),
/// senza i caratteri di controllo, lo spazio che non va a capo e il trattino morbido.
fn win_ansi(c: char) -> bool {
    // I 27 caratteri di 0x80..0x9F di Windows-1252, con i numeri: «€», virgolette e apostrofi
    // tipografici, puntini, trattini, Š, Œ, Ž, ™ e gli altri.
    const ALTRI: &[u32] = &[
        0x20ac, 0x201a, 0x0192, 0x201e, 0x2026, 0x2020, 0x2021, 0x02c6, 0x2030, 0x0160, 0x2039, 0x0152, 0x017d, 0x2018, 0x2019, 0x201c, 0x201d,
        0x2022, 0x2013, 0x2014, 0x02dc, 0x2122, 0x0161, 0x203a, 0x0153, 0x017e, 0x0178,
    ];
    let n = c as u32;
    matches!(n, 0x21..=0x7e | 0xa1..=0xac | 0xae..=0xff) || ALTRI.contains(&n)
}

/// Una classe che la decodifica può scegliere: il vuoto, lo spazio, o un pezzo di testo fatto
/// tutto di caratteri WinAnsi.
fn ammessa(classe: &str) -> bool {
    classe.is_empty() || classe == " " || classe.chars().all(win_ansi)
}

/// La decodifica CTC di una riga: `valori` sono le probabilità, `classi` per colonna. In ogni
/// colonna vince la classe ammessa più probabile; le ripetute di seguito valgono una, il vuoto
/// (classe 0) le separa, lo spazio chiude la parola. Dà le parole con la prima e l'ultima colonna,
/// senza quelle lette con fiducia media sotto [`FIDUCIA_MINIMA`].
fn decodifica(valori: &[f32], classi: usize, alfabeto: &[String], ammesse: &[bool]) -> Vec<(String, usize, usize)> {
    let mut parole = Vec::new();
    let (mut testo, mut inizio, mut fine, mut fiducia, mut quanti) = (String::new(), 0, 0, 0.0f32, 0usize);
    let mut chiudi = |testo: &mut String, inizio: usize, fine: usize, fiducia: f32, quanti: usize| {
        if !testo.is_empty() && fiducia / quanti.max(1) as f32 >= FIDUCIA_MINIMA {
            parole.push((std::mem::take(testo), inizio, fine));
        } else {
            testo.clear();
        }
    };
    let mut prima = 0usize;
    if classi == 0 {
        return parole;
    }
    for (colonna, riga) in valori.chunks_exact(classi).enumerate() {
        let migliore =
            riga.iter().enumerate().filter(|(i, v)| v.is_finite() && ammesse.get(*i).copied().unwrap_or(false)).max_by(|a, b| a.1.total_cmp(b.1));
        let Some((indice, &valore)) = migliore else { continue };
        if indice != 0 && indice != prima {
            let classe = alfabeto.get(indice).map_or("", String::as_str);
            if classe.trim().is_empty() {
                chiudi(&mut testo, inizio, fine, fiducia, quanti);
                (fiducia, quanti) = (0.0, 0);
            } else {
                if testo.is_empty() {
                    inizio = colonna;
                }
                testo.push_str(classe);
                fine = colonna;
                fiducia += valore;
                quanti += 1;
            }
        } else if indice != 0 && !testo.is_empty() {
            fine = colonna;
        }
        prima = indice;
    }
    chiudi(&mut testo, inizio, fine, fiducia, quanti);
    parole
}

/// Il ritaglio di una riga `[sinistra, alto, destra, basso]` con un margine (un po' d'altezza
/// sopra e sotto per gli accenti delle maiuscole, di più ai lati per la punteggiatura in fondo),
/// dentro l'immagine: `[x, y, larghezza, altezza]`.
fn ritaglio(larghezza: u32, altezza: u32, r: [f32; 4]) -> Option<[u32; 4]> {
    let [l, t, d, b] = r;
    if r.iter().any(|v| !v.is_finite()) || d <= l || b <= t {
        return None;
    }
    let margine = (b - t).max(1.0);
    let x = (l - 0.3 * margine).floor().clamp(0.0, larghezza as f32) as u32;
    let y = (t - 0.15 * margine).floor().clamp(0.0, altezza as f32) as u32;
    let destra = (d + 0.3 * margine).ceil().clamp(0.0, larghezza as f32) as u32;
    let sotto = (b + 0.15 * margine).ceil().clamp(0.0, altezza as f32) as u32;
    let (w, h) = (destra.saturating_sub(x), sotto.saturating_sub(y));
    (w > 0 && h > 0).then_some([x, y, w, h])
}

#[cfg(test)]
mod test {
    use super::*;

    fn alfa(classi: &[&str]) -> Vec<String> {
        classi.iter().map(|s| s.to_string()).collect()
    }

    /// Una colonna con la classe `i` al 90% e il resto al 2%.
    fn colonna(classi: usize, i: usize) -> Vec<f32> {
        let mut c = vec![0.02; classi];
        c[i] = 0.9;
        c
    }

    #[test]
    fn le_lettere_italiane_passano_quelle_fuori_da_win_ansi_no() {
        for c in ["à", "è", "é", "ì", "ò", "ù", "È", "À", "€", "°", "\u{2019}", "\u{201c}", "«", "»", "a", "Z", "7", ",", "Œ", "ç", "ñ", "", " "]
        {
            assert!(ammessa(c), "{c} dovrebbe passare");
        }
        for c in ["新", "，", "Ω", "Ж", "ə", "ł", "\u{a0}", "\u{ad}", "\u{7}", "é新"] {
            assert!(!ammessa(c), "{c:?} non dovrebbe passare");
        }
    }

    #[test]
    fn l_alfabeto_ha_il_vuoto_in_testa_e_lo_spazio_in_fondo() {
        assert_eq!(alfabeto("a\nb\r\nè\n"), alfa(&["", "a", "b", "è", " "]));
    }

    #[test]
    fn la_decodifica_fonde_le_ripetute_e_divide_le_parole_sullo_spazio() {
        let a = alfa(&["", "p", "i", "ù", "c", "t", "à", " "]);
        let ammesse = vec![true; a.len()];
        // «pp i ù _ ù» poi spazio poi «c i t t _ t à»: «più» e «città».
        let colonne = [1, 1, 2, 3, 0, 7, 4, 2, 5, 5, 0, 5, 6];
        let valori: Vec<f32> = colonne.iter().flat_map(|&i| colonna(a.len(), i)).collect();
        let parole = decodifica(&valori, a.len(), &a, &ammesse);
        assert_eq!(parole, vec![("più".to_string(), 0, 3), ("città".to_string(), 6, 12)]);
    }

    #[test]
    fn la_decodifica_sceglie_fra_le_classi_ammesse() {
        // La colonna dice «，» (non WinAnsi) al 90% e «,» al 50%: vince la virgola.
        let a = alfa(&["", "a", ",", "，", " "]);
        let ammesse: Vec<bool> = a.iter().map(|c| ammessa(c)).collect();
        let mut c = vec![0.01; a.len()];
        c[3] = 0.9;
        c[2] = 0.5;
        let valori: Vec<f32> = colonna(a.len(), 1).into_iter().chain(c).collect();
        let parole = decodifica(&valori, a.len(), &a, &ammesse);
        assert_eq!(parole.len(), 1);
        assert_eq!(parole[0].0, "a,");
    }

    #[test]
    fn una_parola_incerta_resta_fuori() {
        let a = alfa(&["", "x", " "]);
        let ammesse = vec![true; a.len()];
        let mut c = vec![0.0; a.len()];
        c[1] = 0.3;
        c[0] = 0.2;
        assert!(decodifica(&c, a.len(), &a, &ammesse).is_empty());
        assert!(decodifica(&[], 0, &a, &ammesse).is_empty());
    }

    #[test]
    fn il_ritaglio_tiene_il_margine_e_resta_nell_immagine() {
        assert_eq!(ritaglio(100, 100, [20.0, 30.0, 70.0, 50.0]), Some([14, 27, 62, 26]));
        assert_eq!(ritaglio(100, 100, [0.0, 0.0, 100.0, 100.0]), Some([0, 0, 100, 100]));
        for r in [[f32::NAN, 0.0, 10.0, 10.0], [0.0, 0.0, f32::INFINITY, 10.0], [20.0, 20.0, 10.0, 30.0], [200.0, 20.0, 300.0, 40.0]] {
            assert_eq!(ritaglio(100, 100, r), None, "{r:?}");
        }
    }

    #[test]
    fn i_modelli_mancanti_dicono_quale() {
        let solo_uno = |nome: &str| (nome == RILEVAMENTO).then(|| vec![1u8]);
        assert_eq!(Modelli::da(solo_uno).err(), Some(ErroreOcr::Modello(RICONOSCIMENTO)));
        let vuoti = |_: &str| Some(Vec::new());
        assert_eq!(Modelli::da(vuoti).err(), Some(ErroreOcr::Modello(RILEVAMENTO)));
    }

    #[test]
    fn una_parola_nel_debug_non_mostra_il_testo() {
        let p = Parola { testo: "segreto".into(), riquadro: [0.0; 4] };
        assert!(!format!("{p:?}").contains("segreto"));
    }

    #[test]
    fn i_modelli_veri_si_caricano_e_un_immagine_bianca_non_ha_righe() {
        let ocr = Ocr::carica(Modelli::da_cartella(&cartella_dei_modelli()).expect("modelli in crates/ocr/modelli")).expect("caricati");
        assert!(ocr.alfabeto.len() > 18000);
        let bianca = vec![255u8; 200 * 100 * 4];
        assert!(ocr.riconosci(&bianca, 200, 100, &|| false).expect("riconosciuta").is_empty());
        assert_eq!(ocr.riconosci(&bianca, 201, 100, &|| false), Err(ErroreOcr::Immagine));
    }
}
