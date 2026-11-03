//! Correggere il testo di una scansione (bug segnalato dall'autore il 1/11: le lettere dei documenti
//! fotografati e riconosciuti non si cambiavano, per esempio un nome). Il testo riconosciuto è lo
//! strato invisibile dell'OCR sopra la foto della pagina; quello che si legge sono i pixel.
//!
//! Dalla 0.8.8 (seconda segnalazione del 3/11: «se modifico un file scansionato cambia il font ed
//! è diverso dal resto») la correzione si fa nella foto, e solo dove serve: le parole cambiate si
//! ridisegnano nei pixel col carattere più simile a quello stampato, alla sua misura e col colore
//! del suo inchiostro ([`crate::ritocco`]); le altre parole restano i pixel di prima, spostate
//! nella riga se serve posto. La scritta di prima se ne va dalla foto, e la foto nuova prende il
//! posto di quella vecchia: il salvataggio riscrive tutto il file, e la foto vecchia non resta.
//! Lo strato invisibile si rifà parola per parola sui riquadri nuovi, così la ricerca trova il
//! testo nuovo nel punto giusto. Prima (0.8.7) il paragrafo si riscriveva tutto in Helvetica sopra
//! un riquadro del colore della carta, e si vedeva.
//!
//! Una foto che non si sa aprire (fax CCITT, JBIG2, JPEG 2000) o girata nella pagina ferma la
//! correzione: mai la scritta vecchia nascosta sotto qualcosa.

use std::sync::Arc;

use image::{DynamicImage, GenericImageView, ImageFormat};
use pdfcraft_engine::{Document, Edit, ImageEdit};

use crate::modifica::{GenereModifica, Modifica};
use crate::ritocco::{Intoppo, ParolaFoto, Voce};

/// La qualità del JPEG della foto corretta: come quella delle pagine di Scansiona, un poco sopra.
const QUALITA: u8 = 90;
/// Quanti pixel intorno alle righe si guardano per il colore della carta.
const ANELLO: u32 = 6;
/// Foto più grandi di così (pixel) non si decodificano: 80 milioni, un A3 a 600 dpi.
const PIXEL_MASSIMI: u64 = 80_000_000;
/// Oltre tante parole per parte il confronto fra il testo di prima e quello nuovo non si fa
/// parola per parola: tutto quello che sta in mezzo cambia.
const PAROLE_DA_CONFRONTARE: usize = 2000;

/// Il paragrafo invisibile da correggere: la foto da rifare sul thread del motore.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pulizia {
    pub(crate) pagina: usize,
    pub(crate) blocco: usize,
}

/// Perché la correzione non si fa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErroreCorrezione {
    /// Il paragrafo non è (più) il testo invisibile di una scansione.
    NonScansione,
    /// Sotto il paragrafo non c'è la foto di una scansione.
    SenzaFoto,
    /// La foto c'è ma non si sa rifare (formato, misura, girata).
    Foto,
    /// Nella foto non si trovano le lettere delle righe da cambiare.
    SenzaLettere,
    /// Il testo nuovo non entra nella riga.
    NonSta,
}

impl ErroreCorrezione {
    /// Per la diagnosi e per il messaggio dell'app.
    pub(crate) fn genere(self) -> &'static str {
        match self {
            ErroreCorrezione::NonScansione => "non è una scansione",
            ErroreCorrezione::SenzaFoto => "senza la foto della scansione",
            ErroreCorrezione::Foto => "foto della scansione",
            ErroreCorrezione::SenzaLettere => "lettere della scansione",
            ErroreCorrezione::NonSta => "riga della scansione piena",
        }
    }
}

impl From<Intoppo> for ErroreCorrezione {
    fn from(i: Intoppo) -> Self {
        match i {
            Intoppo::NonSta => ErroreCorrezione::NonSta,
            Intoppo::SenzaLettere => ErroreCorrezione::SenzaLettere,
        }
    }
}

/// La modifica pronta per il motore, in un passo solo: la foto rifatta al posto di quella di
/// prima, il paragrafo invisibile tolto e rimesso parola per parola col testo nuovo. `edit` è la
/// riscrittura chiesta (`EditTextBlock`): qui se ne prende solo il testo.
pub(crate) fn risolvi(d: &Document, p: Pulizia, edit: Edit) -> Result<Edit, ErroreCorrezione> {
    let Edit::EditTextBlock { page, block, text, .. } = edit else { return Err(ErroreCorrezione::NonScansione) };
    let blocchi = d.text_blocks(p.pagina);
    let blocco = blocchi.get(p.blocco).filter(|b| b.invisible).ok_or(ErroreCorrezione::NonScansione)?;
    let righe = d.text_lines(p.pagina);
    let parole = parole_del_paragrafo(blocco, &righe);
    let unione = parole.iter().flatten().map(|(_, r)| *r).reduce(unisci).ok_or(ErroreCorrezione::NonScansione)?;
    // La foto sotto il paragrafo: quella che lo contiene, la più grande se sono più d'una.
    let immagini = d.page_images(p.pagina);
    let Some((indice, immagine)) = immagini.iter().enumerate().filter(|(_, i)| !i.is_form && contiene(i.rect, unione)).max_by(|a, b| area(a.1.rect).total_cmp(&area(b.1.rect)))
    else {
        // Un disegno raggruppato sotto le righe non si rifà a pixel: meglio fermarsi.
        return Err(if immagini.iter().any(|i| si_toccano(i.rect, unione)) { ErroreCorrezione::Foto } else { ErroreCorrezione::SenzaFoto });
    };
    let matrice = immagine.matrix;
    let misura = (immagine.width, immagine.height);
    // Le righe della foto devono essere quelle della pagina: dritta, non girata né rovesciata.
    let [a, b, c, dd, _, _] = matrice;
    if !(a > 0.0 && dd > 0.0 && b.abs() <= a * 1e-6 && c.abs() <= dd * 1e-6) {
        return Err(ErroreCorrezione::Foto);
    }
    if u64::from(misura.0) * u64::from(misura.1) > PIXEL_MASSIMI {
        return Err(ErroreCorrezione::Foto);
    }
    let (estensione, byte) = d.page_image_file(p.pagina, indice).map_err(|_| ErroreCorrezione::Foto)?;
    let formato = match estensione {
        "jpg" => ImageFormat::Jpeg,
        "png" => ImageFormat::Png,
        _ => return Err(ErroreCorrezione::Foto),
    };
    let foto = image::load_from_memory_with_format(&byte, formato).map_err(|_| ErroreCorrezione::Foto)?;
    if foto.dimensions() != misura {
        return Err(ErroreCorrezione::Foto);
    }
    let mut rgb = foto.to_rgb8();
    let in_pixel = |r: [f64; 4]| ai_pixel(matrice, misura, r);
    let righe_foto: Vec<Vec<ParolaFoto>> =
        parole.iter().map(|riga| riga.iter().filter_map(|(t, r)| in_pixel(*r).map(|r| ParolaFoto { testo: t.clone(), r })).collect()).collect();
    let vecchie: Vec<Vec<&str>> = righe_foto.iter().map(|r| r.iter().map(|p| p.testo.as_str()).collect()).collect();
    let nuove: Vec<&str> = text.split_whitespace().collect();
    let per_riga = allinea(&vecchie, &nuove);
    // Fin dove può arrivare una riga: il margine destro del testo della pagina.
    let destra = righe
        .iter()
        .filter_map(|l| in_pixel(l.rect))
        .map(|r| r[2])
        .chain(righe_foto.iter().flatten().map(|p| p.r[2]))
        .max()
        .unwrap_or(misura.0)
        .min(misura.0.saturating_sub(1));
    // Prima i riquadri veri di tutte le righe, poi le righe cambiate si rifanno.
    let affinate: Vec<Option<(Vec<ParolaFoto>, crate::ritocco::Inchiostro)>> = righe_foto.iter().map(|r| crate::ritocco::affina_riga(&rgb, r)).collect();
    let mut finali: Vec<ParolaFoto> = Vec::new();
    for (i, riga) in righe_foto.iter().enumerate() {
        let voci = &per_riga[i];
        let uguale = voci.len() == riga.len() && voci.iter().enumerate().all(|(k, v)| *v == Voce::Tenuta(k));
        match (&affinate[i], uguale) {
            (Some((parole, _)), true) => finali.extend(parole.iter().cloned()),
            (None, true) => finali.extend(riga.iter().cloned()),
            (None, false) => return Err(ErroreCorrezione::SenzaLettere),
            (Some((parole, inchiostro)), false) => {
                let fondo = |r: Option<&Vec<ParolaFoto>>, alto: bool| r.and_then(|r| if alto { r.iter().map(|p| p.r[1]).min() } else { r.iter().map(|p| p.r[3]).max() });
                let sopra = i.checked_sub(1).and_then(|j| fondo(affinate[j].as_ref().map(|a| &a.0), false)).unwrap_or(0);
                let sotto = affinate.get(i + 1).and_then(|a| fondo(a.as_ref().map(|a| &a.0), true)).unwrap_or(misura.1);
                finali.extend(crate::ritocco::riscrivi_riga(&mut rgb, parole, inchiostro, voci, destra, (sopra, sotto))?);
            }
        }
    }
    let rifatta = scrivi(foto, rgb, formato)?;
    let words = finali.iter().map(|p| parola_nella_pagina(matrice, misura, p)).collect();
    let name = if formato == ImageFormat::Jpeg { "scansione.jpg" } else { "scansione.png" }.to_string();
    Ok(Edit::Batch {
        label: "Correggi la scansione".into(),
        edits: vec![
            Edit::EditPageImage { page, index: indice, change: ImageEdit::Replace { name, bytes: Arc::new(rifatta) } },
            Edit::EditTextBlock { page, block, text: String::new(), style: pdfcraft_engine::BlockStyle::default() },
            Edit::AddOcrText { page, words },
        ],
    })
}

/// Le parole del paragrafo, riga per riga da sinistra, coi loro riquadri nello spazio utente. Una
/// riga dell'OCR che non dice dove sta ogni parola si divide in proporzione alle lettere.
fn parole_del_paragrafo(blocco: &pdfcraft_engine::TextBlock, righe: &[pdfcraft_engine::TextLine]) -> Vec<Vec<(String, [f64; 4])>> {
    blocco
        .rows
        .iter()
        .map(|riga| {
            let mut pezzi: Vec<&pdfcraft_engine::TextLine> = riga.iter().filter_map(|i| righe.get(*i)).collect();
            pezzi.sort_by(|a, b| a.rect[0].min(a.rect[2]).total_cmp(&b.rect[0].min(b.rect[2])));
            let mut parole = Vec::new();
            for l in pezzi {
                let testi: Vec<&str> = l.text.split_whitespace().collect();
                if testi.len() == l.ink.len() {
                    parole.extend(testi.iter().zip(&l.ink).map(|(t, r)| (t.to_string(), ordinato(*r))));
                    continue;
                }
                let r = ordinato(l.rect);
                let lettere: usize = testi.iter().map(|t| t.chars().count() + 1).sum();
                let passo = (r[2] - r[0]) / lettere.max(1) as f64;
                let mut x = r[0];
                for t in testi {
                    let largo = passo * t.chars().count() as f64;
                    parole.push((t.to_string(), [x, r[1], x + largo, r[3]]));
                    x += largo + passo;
                }
            }
            parole
        })
        .collect()
}

/// Il testo nuovo diviso fra le righe di prima: per ogni riga le sue parole rimaste (il loro
/// numero) e quelle nuove, nell'ordine. Le parole si confrontano col metodo della sottosequenza
/// comune più lunga; una parola nuova va nella riga della parola di prima che la precede.
pub(crate) fn allinea(vecchie: &[Vec<&str>], nuove: &[&str]) -> Vec<Vec<Voce>> {
    let tutte: Vec<(usize, usize, &str)> = vecchie.iter().enumerate().flat_map(|(r, riga)| riga.iter().enumerate().map(move |(k, t)| (r, k, *t))).collect();
    let (n, m) = (tutte.len(), nuove.len());
    let prima = tutte.iter().zip(nuove).take_while(|(a, b)| a.2 == **b).count();
    let dopo = tutte[prima..].iter().rev().zip(nuove[prima..].iter().rev()).take_while(|(a, b)| a.2 == **b).count();
    // I passi: (Some(vecchia), Some(nuova)) tenuta, (Some, None) tolta, (None, Some) aggiunta.
    let mut passi: Vec<(Option<usize>, Option<usize>)> = (0..prima).map(|i| (Some(i), Some(i))).collect();
    let (a, b) = (&tutte[prima..n - dopo], &nuove[prima..m - dopo]);
    if a.len() <= PAROLE_DA_CONFRONTARE && b.len() <= PAROLE_DA_CONFRONTARE {
        let mut lcs = vec![0u32; (a.len() + 1) * (b.len() + 1)];
        let w = b.len() + 1;
        for i in (0..a.len()).rev() {
            for j in (0..b.len()).rev() {
                lcs[i * w + j] = if a[i].2 == b[j] { lcs[(i + 1) * w + j + 1] + 1 } else { lcs[(i + 1) * w + j].max(lcs[i * w + j + 1]) };
            }
        }
        let (mut i, mut j) = (0, 0);
        let mut tolte = Vec::new();
        let mut aggiunte = Vec::new();
        let scarica = |passi: &mut Vec<(Option<usize>, Option<usize>)>, tolte: &mut Vec<usize>, aggiunte: &mut Vec<usize>| {
            passi.extend(tolte.drain(..).map(|t| (Some(prima + t), None)));
            passi.extend(aggiunte.drain(..).map(|g| (None, Some(prima + g))));
        };
        while i < a.len() || j < b.len() {
            if i < a.len() && j < b.len() && a[i].2 == b[j] {
                scarica(&mut passi, &mut tolte, &mut aggiunte);
                passi.push((Some(prima + i), Some(prima + j)));
                i += 1;
                j += 1;
            } else if j < b.len() && (i == a.len() || lcs[i * w + j + 1] >= lcs[(i + 1) * w + j]) {
                aggiunte.push(j);
                j += 1;
            } else {
                tolte.push(i);
                i += 1;
            }
        }
        scarica(&mut passi, &mut tolte, &mut aggiunte);
    } else {
        passi.extend((0..a.len()).map(|t| (Some(prima + t), None)));
        passi.extend((0..b.len()).map(|g| (None, Some(prima + g))));
    }
    passi.extend((0..dopo).map(|k| (Some(n - dopo + k), Some(m - dopo + k))));
    let mut per_riga: Vec<Vec<Voce>> = vec![Vec::new(); vecchie.len()];
    let mut riga = 0;
    for passo in passi {
        match passo {
            (Some(v), Some(_)) => {
                riga = tutte[v].0;
                per_riga[riga].push(Voce::Tenuta(tutte[v].1));
            }
            (Some(v), None) => riga = tutte[v].0,
            (None, Some(g)) => {
                if let Some(r) = per_riga.get_mut(riga) {
                    r.push(Voce::Nuova(nuove[g].to_string()));
                }
            }
            (None, None) => {}
        }
    }
    per_riga
}

/// La foto rifatta `rgb` nel genere di pixel di `prima` (grigi, colori, colori con la
/// trasparenza) e nel suo formato.
fn scrivi(prima: DynamicImage, rgb: image::RgbImage, formato: ImageFormat) -> Result<Vec<u8>, ErroreCorrezione> {
    let rifatta = match prima {
        DynamicImage::ImageLuma8(_) => DynamicImage::ImageLuma8(DynamicImage::ImageRgb8(rgb).to_luma8()),
        f if f.color().has_alpha() => {
            let mut c = f.to_rgba8();
            for (q, p) in c.pixels_mut().zip(rgb.pixels()) {
                q.0[..3].copy_from_slice(&p.0);
            }
            DynamicImage::ImageRgba8(c)
        }
        _ => DynamicImage::ImageRgb8(rgb),
    };
    let mut fuori = Vec::new();
    let scritta = match formato {
        ImageFormat::Jpeg => rifatta.write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(&mut fuori, QUALITA)),
        _ => rifatta.write_to(&mut std::io::Cursor::new(&mut fuori), ImageFormat::Png),
    };
    scritta.map_err(|_| ErroreCorrezione::Foto)?;
    Ok(fuori)
}

/// Una parola della foto nello strato invisibile: il suo riquadro dai pixel alla pagina.
fn parola_nella_pagina(matrice: [f64; 6], misura: (u32, u32), p: &ParolaFoto) -> crate::ocr::PlacedWord {
    let nella_pagina = |x: u32, y: u32| dalla_foto(matrice, misura, f64::from(x), f64::from(y));
    let o = nella_pagina(p.r[0], p.r[3]);
    let destra = nella_pagina(p.r[2], p.r[3]);
    let su = nella_pagina(p.r[0], p.r[1]);
    crate::ocr::PlacedWord { text: p.testo.clone(), origin: o, across: [destra[0] - o[0], destra[1] - o[1]], up: [su[0] - o[0], su[1] - o[1]] }
}

/// Il rettangolo [x0, y0, x1, y1] con x0 ≤ x1 e y0 ≤ y1.
fn ordinato(r: [f64; 4]) -> [f64; 4] {
    [r[0].min(r[2]), r[1].min(r[3]), r[0].max(r[2]), r[1].max(r[3])]
}

fn unisci(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]
}

fn area(r: [f64; 4]) -> f64 {
    ((r[2] - r[0]) * (r[3] - r[1])).abs()
}

/// `fuori` contiene `dentro` (con mezzo punto di gioco).
fn contiene(fuori: [f64; 4], dentro: [f64; 4]) -> bool {
    let (f, d) = (ordinato(fuori), ordinato(dentro));
    d[0] >= f[0] - 0.5 && d[1] >= f[1] - 0.5 && d[2] <= f[2] + 0.5 && d[3] <= f[3] + 0.5
}

/// Due rettangoli [x0, y0, x1, y1] (in qualunque ordine dei lati) hanno una parte in comune.
fn si_toccano(a: [f64; 4], b: [f64; 4]) -> bool {
    let (a, b) = (ordinato(a), ordinato(b));
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}

/// Un punto della pagina nei pixel della foto disegnata con `matrice` (dal quadrato unitario
/// dell'immagine allo spazio utente; la prima riga di pixel sta in alto) e grande `misura`.
fn nella_foto(matrice: [f64; 6], misura: (u32, u32), x: f64, y: f64) -> Option<[f64; 2]> {
    let [a, b, c, d, e, f] = matrice;
    let det = a * d - b * c;
    if !det.is_finite() || det.abs() < 1e-12 {
        return None;
    }
    let u = (d * (x - e) - c * (y - f)) / det;
    let v = (-b * (x - e) + a * (y - f)) / det;
    Some([u * f64::from(misura.0), (1.0 - v) * f64::from(misura.1)])
}

/// Un pixel della foto nella pagina: il contrario di [`nella_foto`].
fn dalla_foto(matrice: [f64; 6], misura: (u32, u32), px: f64, py: f64) -> [f64; 2] {
    let [a, b, c, d, e, f] = matrice;
    let (u, v) = (px / f64::from(misura.0.max(1)), 1.0 - py / f64::from(misura.1.max(1)));
    [a * u + c * v + e, b * u + d * v + f]
}

/// Il rettangolo `r` della pagina nei pixel della foto: [x0, y0, x1, y1] interi (x1 e y1 esclusi),
/// arrotondati verso fuori, dentro la foto. None se cade fuori.
pub(crate) fn ai_pixel(matrice: [f64; 6], misura: (u32, u32), r: [f64; 4]) -> Option<[u32; 4]> {
    let mut x0 = f64::INFINITY;
    let mut y0 = f64::INFINITY;
    let mut x1 = f64::NEG_INFINITY;
    let mut y1 = f64::NEG_INFINITY;
    for (x, y) in [(r[0], r[1]), (r[2], r[1]), (r[2], r[3]), (r[0], r[3])] {
        let [px, py] = nella_foto(matrice, misura, x, y)?;
        x0 = x0.min(px);
        y0 = y0.min(py);
        x1 = x1.max(px);
        y1 = y1.max(py);
    }
    if ![x0, y0, x1, y1].iter().all(|v| v.is_finite()) {
        return None;
    }
    let (w, h) = (f64::from(misura.0), f64::from(misura.1));
    let r = [x0.floor().clamp(0.0, w) as u32, y0.floor().clamp(0.0, h) as u32, x1.ceil().clamp(0.0, w) as u32, y1.ceil().clamp(0.0, h) as u32];
    (r[2] > r[0] && r[3] > r[1]).then_some(r)
}

/// Il colore della carta intorno ai rettangoli `coperti` (pixel): fra i pixel di un anello largo
/// `ANELLO` intorno a ognuno, fuori da tutti, la media di quelli fra il 60° e il 90° percentile
/// della luminosità. L'inchiostro è scuro e sta sotto; un riflesso è sopra. Bianco se intorno non
/// c'è niente da guardare.
pub(crate) fn colore_della_carta(foto: &image::RgbImage, coperti: &[[u32; 4]]) -> [u8; 3] {
    let (w, h) = foto.dimensions();
    let dentro_uno = |x: u32, y: u32| coperti.iter().any(|r| x >= r[0] && x < r[2] && y >= r[1] && y < r[3]);
    let mut campioni: Vec<[u8; 3]> = Vec::new();
    for r in coperti {
        let (x0, y0) = (r[0].saturating_sub(ANELLO), r[1].saturating_sub(ANELLO));
        let (x1, y1) = ((r[2] + ANELLO).min(w), (r[3] + ANELLO).min(h));
        for y in y0..y1 {
            for x in x0..x1 {
                if !dentro_uno(x, y) {
                    campioni.push(foto.get_pixel(x, y).0);
                }
            }
        }
    }
    if campioni.is_empty() {
        return [255, 255, 255];
    }
    let luce = |p: &[u8; 3]| 299 * u32::from(p[0]) + 587 * u32::from(p[1]) + 114 * u32::from(p[2]);
    campioni.sort_by_key(luce);
    let n = campioni.len();
    let (da, a) = (n * 6 / 10, (n * 9 / 10).max(n * 6 / 10 + 1).min(n));
    let scelti = &campioni[da..a];
    let mut somma = [0u64; 3];
    for p in scelti {
        for (s, v) in somma.iter_mut().zip(p) {
            *s += u64::from(*v);
        }
    }
    somma.map(|s| (s / scelti.len() as u64) as u8)
}

impl Modifica {
    /// Corregge il testo riconosciuto di una scansione: il paragrafo invisibile `o` diventa
    /// `testo`, e nella foto le parole cambiate si ridisegnano nel carattere della scansione (vedi
    /// il modulo). La foto rifatta la prepara il motore sul suo thread; se non si sa rifare, la
    /// modifica non si fa.
    pub fn correggi_scansione(o: &crate::Oggetto, testo: &str) -> Option<Modifica> {
        if o.genere != crate::GenereOggetto::Scansione {
            return None;
        }
        let edit = Edit::EditTextBlock {
            page: o.pagina,
            block: o.bersaglio.index,
            text: crate::contenuto::rimetti_gli_a_capo(&o.righe, testo),
            style: pdfcraft_engine::BlockStyle::default(),
        };
        Some(Modifica {
            edit,
            genere: GenereModifica::Pagine,
            toccate: crate::Toccate::alcune([o.pagina]),
            // Niente avviso del carattere: le lettere nuove sono pixel della foto, nel carattere più
            // simile a quello della scansione.
            carattere: None,
            pulizia: Some(Pulizia { pagina: o.pagina, blocco: o.bersaglio.index }),
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn un_rettangolo_della_pagina_nei_pixel_della_foto() {
        // Una foto 200 × 100 pixel disegnata su [0, 0, 400, 200] della pagina: 2 pt per pixel.
        let m = [400.0, 0.0, 0.0, 200.0, 0.0, 0.0];
        // In basso a sinistra della pagina: in basso a sinistra della foto (le righe dall'alto).
        assert_eq!(ai_pixel(m, (200, 100), [0.0, 0.0, 40.0, 20.0]), Some([0, 90, 20, 100]));
        // In alto a destra, coi lati in un altro ordine.
        assert_eq!(ai_pixel(m, (200, 100), [400.0, 200.0, 360.0, 180.0]), Some([180, 0, 200, 10]));
        // A metà pixel si arrotonda verso fuori.
        assert_eq!(ai_pixel(m, (200, 100), [3.0, 3.0, 5.0, 5.0]), Some([1, 97, 3, 99]));
        // E indietro: un pixel della foto nella pagina.
        let [x, y] = dalla_foto(m, (200, 100), 20.0, 90.0);
        assert!((x - 40.0).abs() < 1e-9 && (y - 20.0).abs() < 1e-9, "{x} {y}");
        // Fuori dalla foto: niente.
        assert_eq!(ai_pixel(m, (200, 100), [500.0, 0.0, 600.0, 20.0]), None);
        // Una matrice che non si inverte.
        assert_eq!(ai_pixel([0.0; 6], (200, 100), [0.0, 0.0, 40.0, 20.0]), None);
    }

    #[test]
    fn il_testo_nuovo_si_divide_fra_le_righe_di_prima() {
        use Voce::{Nuova, Tenuta};
        let righe = vec![vec!["Area", "lavoro/istruzione"], vec!["Tempo", "libero/hobby"]];
        // Una parola tolta.
        assert_eq!(allinea(&righe, &["Area", "Tempo", "libero/hobby"]), vec![vec![Tenuta(0)], vec![Tenuta(0), Tenuta(1)]]);
        // Una cambiata in fondo alla prima riga resta nella prima riga.
        assert_eq!(
            allinea(&righe, &["Area", "studio", "Tempo", "libero/hobby"]),
            vec![vec![Tenuta(0), Nuova("studio".into())], vec![Tenuta(0), Tenuta(1)]]
        );
        // Una cambiata in testa alla seconda va nella seconda.
        assert_eq!(
            allinea(&righe, &["Area", "lavoro/istruzione", "Ore", "libero/hobby"]),
            vec![vec![Tenuta(0), Tenuta(1)], vec![Nuova("Ore".into()), Tenuta(1)]]
        );
        // Una aggiunta in mezzo e una in fondo.
        assert_eq!(
            allinea(&righe, &["Area", "di", "lavoro/istruzione", "Tempo", "libero/hobby", "ecc."]),
            vec![vec![Tenuta(0), Nuova("di".into()), Tenuta(1)], vec![Tenuta(0), Tenuta(1), Nuova("ecc.".into())]]
        );
        // Tutto uguale; tutto tolto.
        assert_eq!(allinea(&righe, &["Area", "lavoro/istruzione", "Tempo", "libero/hobby"]), vec![vec![Tenuta(0), Tenuta(1)], vec![Tenuta(0), Tenuta(1)]]);
        assert_eq!(allinea(&righe, &[]), vec![Vec::<Voce>::new(), Vec::new()]);
    }

    #[test]
    fn il_colore_della_carta_non_e_l_inchiostro() {
        // Carta color avorio con una riga d'inchiostro dentro il rettangolo e un'altra sopra,
        // nell'anello: il colore resta quello della carta.
        let carta = [245, 237, 219];
        let mut foto = image::RgbImage::from_pixel(100, 60, image::Rgb(carta));
        for x in 10..90 {
            for y in 20..30 {
                foto.put_pixel(x, y, image::Rgb([20, 20, 20]));
            }
            foto.put_pixel(x, 16, image::Rgb([20, 20, 20]));
        }
        let c = colore_della_carta(&foto, &[[8, 18, 92, 32]]);
        assert!(c.iter().zip(carta).all(|(a, b)| a.abs_diff(b) <= 2), "{c:?}");
        // Niente intorno: bianco.
        assert_eq!(colore_della_carta(&foto, &[[0, 0, 100, 60]]), [255, 255, 255]);
    }
}
