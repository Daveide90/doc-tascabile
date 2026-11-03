//! «Scansiona» (piano «PDF completo», P8; rifatta nei ritocchi della 0.8.2, R3, dopo «migliora la
//! scansione, ora fa pena»): le foto dei fogli diventano un PDF nuovo, già con lo strato di testo
//! dell'OCR.
//!
//! Ogni foto si gira come dice l'EXIF, si riduce a 3000 pixel sul lato lungo e si tiene in JPEG;
//! il motore cerca i bordi del foglio (`camera_edges`) e, se non è sicuro, rinuncia: allora gli
//! angoli partono da un riquadro poco dentro la foto. Gli angoli li sistema chi scansiona (nell'app,
//! «Sistema i bordi»); poi il motore raddrizza la prospettiva (`camera::warp`) e dà alla pagina
//! l'aspetto scelto fra i suoi quattro (`camera_clean`, a livello 60): Documento (pulita, a
//! colori), Grigi, Bianco e nero, Originale. La pagina si tiene pronta: in JPEG, o in PNG a un bit
//! per il bianco e nero, con la risoluzione che fa il suo lato lungo grande come quello di un A4.
//! Ogni pagina si rifà dalla sua foto, mai da sé stessa: girarla o cambiarle l'aspetto non la
//! sciupa. Fino alla 0.8.1 foto e pagine stavano in pixel nudi (19 MB a pagina), gli angoli non si
//! potevano sistemare e, se il motore non trovava i bordi, nel PDF finiva anche il tavolo.

use std::io::Cursor;
use std::sync::Arc;

use image::codecs::jpeg::{JpegEncoder, PixelDensity};
use image::{DynamicImage, GrayImage, ImageDecoder, ImageFormat, ImageReader, Limits, RgbImage};
use pdfcraft_engine::camera_clean::{Look, Pixels};
use pdfcraft_engine::{Edit, ImageResolution, Session};
use tascabile_ocr::{ErroreOcr, Ocr};

use crate::firma::ErroreFoto;
use crate::ocr::parole_nella_pagina;

/// Il lato più lungo della foto ridotta: circa 260 dpi su un A4, abbastanza per l'OCR.
const LATO: u32 = 3000;
/// Una foto più grande di così non si apre nemmeno (come la foto della firma).
const PIXEL_MASSIMI: u64 = 200_000_000;
const LATO_MASSIMO: u32 = 20_000;
/// Il livello della pulizia: luce uniforme e contrasto, via i puntini (la base della ricerca,
/// sezione 6).
const LIVELLO: u8 = 60;
/// La qualità del JPEG della foto (da cui si rifà la pagina) e di quello delle pagine.
const QUALITA_FOTO: u8 = 92;
const QUALITA: u8 = 88;
/// Senza bordi trovati gli angoli partono da un riquadro tanto dentro la foto (un ventesimo).
const DENTRO: f32 = 0.05;
/// I bordi trovati dal motore sbagliano di qualche pixel (li cerca su una foto di 400): gli angoli
/// partono un poco più dentro il foglio, tanto del lato lungo, così il tavolo non resta in un filo
/// scuro sul bordo della pagina.
const RIENTRO: f32 = 0.008;
/// Un foglio è grande almeno un centesimo della foto.
const AREA_MINIMA: f32 = 0.01;

/// I quattro angoli del foglio nei pixel della foto: in alto a sinistra, in alto a destra, in basso
/// a destra, in basso a sinistra.
pub type Angoli = [[f32; 2]; 4];

/// I punti in mezzo ai lati del foglio (alto, destro, basso, sinistro), nei pixel della foto: un
/// lato curvo (foglio piegato, curvatura dell'obiettivo) passa da lì.
pub type Lati = [[f32; 2]; 4];

/// Una passata del cancellino: i punti del dito nella pagina dritta prima dei giri (da 0 a 1
/// della larghezza e dell'altezza) e il raggio (frazione della larghezza).
#[derive(Debug, Clone, PartialEq)]
pub struct Cancellatura {
    pub punti: Vec<[f32; 2]>,
    pub raggio: f32,
}

/// Quello che si fa alla pagina oltre agli angoli: i lati curvi, le passate del cancellino e il
/// formato del foglio. Restano quando la pagina si rifà con Ruota o un altro aspetto.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Ritocchi {
    pub lati: Option<Lati>,
    pub cancellature: Vec<Cancellatura>,
    pub formato: Formato,
}

/// Il formato del foglio fotografato (l'autore, 3/11: «fai in modo che l'utente possa scegliere che
/// foglio è, un A4, un A3 o un A5 o biglietto da visita, in modo che il PDF sappia già cos'è quando
/// stampa»): la pagina ne prende le proporzioni e, nel PDF, la misura vera. Automatico: le
/// proporzioni vere del foglio, uguali a quelle di un A4 o di un foglio Lettera quando ci sono
/// vicine, e il lato lungo di un A4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Formato {
    #[default]
    Automatico,
    A4,
    A3,
    A5,
    Biglietto,
}

impl Formato {
    pub const TUTTI: [Formato; 5] = [Formato::Automatico, Formato::A4, Formato::A3, Formato::A5, Formato::Biglietto];

    pub fn nome(self) -> &'static str {
        match self {
            Formato::Automatico => "Automatico",
            Formato::A4 => "A4",
            Formato::A3 => "A3",
            Formato::A5 => "A5",
            Formato::Biglietto => "Biglietto da visita",
        }
    }

    /// Per la diagnosi.
    pub fn chiave(self) -> &'static str {
        match self {
            Formato::Automatico => "automatico",
            Formato::A4 => "a4",
            Formato::A3 => "a3",
            Formato::A5 => "a5",
            Formato::Biglietto => "biglietto",
        }
    }

    /// Il lato corto e il lato lungo in millimetri (None: automatico).
    pub fn millimetri(self) -> Option<(f64, f64)> {
        match self {
            Formato::Automatico => None,
            Formato::A4 => Some((210.0, 297.0)),
            Formato::A3 => Some((297.0, 420.0)),
            Formato::A5 => Some((148.0, 210.0)),
            Formato::Biglietto => Some((55.0, 85.0)),
        }
    }
}

/// I fogli di cui le proporzioni automatiche prendono la forma esatta quando ci sono vicine (lato
/// corto su lato lungo, lato lungo in millimetri, quanto vicine): la serie A e il Lettera.
const FOGLI_NOTI: [(f64, f64, f64); 2] = [(std::f64::consts::FRAC_1_SQRT_2, 297.0, 0.06), (215.9 / 279.4, 279.4, 0.04)];

/// Il rapporto fra lato corto e lato lungo della pagina e il suo lato lungo in millimetri, dal
/// rapporto `stimato` (larghezza su altezza) e dal formato scelto.
fn proporzioni(stimato: f64, formato: Formato) -> (f64, f64) {
    let corto_su_lungo = if stimato > 1.0 { 1.0 / stimato } else { stimato };
    match formato.millimetri() {
        Some((corto, lungo)) => (corto / lungo, lungo),
        None => FOGLI_NOTI
            .iter()
            .filter(|(r, _, quanto)| (corto_su_lungo - r).abs() / r < *quanto)
            .min_by(|a, b| (corto_su_lungo - a.0).abs().total_cmp(&(corto_su_lungo - b.0).abs()))
            .map_or((corto_su_lungo, A4_LUNGO_MM), |(r, lungo, _)| (*r, *lungo)),
    }
}

/// Il lato lungo di un A4 in millimetri: la misura delle pagine automatiche di forma qualunque.
const A4_LUNGO_MM: f64 = 297.0;

/// L'aspetto di una pagina: i quattro del motore.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Aspetto {
    /// Pulita: la carta bianca anche dove c'era l'ombra, l'inchiostro a colori.
    #[default]
    Documento,
    /// Pulita, in grigi.
    Grigi,
    /// Solo nero sul bianco: ogni punto o l'uno o l'altro.
    BiancoNero,
    /// La foto raddrizzata, com'era.
    Originale,
}

impl Aspetto {
    pub const TUTTI: [Aspetto; 4] = [Aspetto::Documento, Aspetto::Grigi, Aspetto::BiancoNero, Aspetto::Originale];

    /// Il nome per chi usa l'app.
    pub fn nome(self) -> &'static str {
        match self {
            Aspetto::Documento => "Documento",
            Aspetto::Grigi => "Grigi",
            Aspetto::BiancoNero => "Bianco e nero",
            Aspetto::Originale => "Originale",
        }
    }

    /// Per la diagnosi.
    pub fn chiave(self) -> &'static str {
        match self {
            Aspetto::Documento => "documento",
            Aspetto::Grigi => "grigi",
            Aspetto::BiancoNero => "bianco e nero",
            Aspetto::Originale => "originale",
        }
    }

    fn look(self) -> Look {
        match self {
            Aspetto::Documento => Look::Colour,
            Aspetto::Grigi => Look::Greyscale,
            Aspetto::BiancoNero => Look::BlackWhite,
            Aspetto::Originale => Look::Original,
        }
    }
}

/// Una foto per la scansione: dritta, al più 3000 pixel sul lato lungo, in JPEG; e gli angoli del
/// foglio, se il motore li ha trovati. Debug senza pixel.
#[derive(Clone, PartialEq)]
pub struct FotoScansionata {
    pub larghezza: u32,
    pub altezza: u32,
    pub jpeg: Vec<u8>,
    pub trovati: Option<Angoli>,
    /// I lati curvi del foglio trovato, se lo sono.
    pub lati_trovati: Option<Lati>,
    /// La distanza focale dell'obiettivo in pixel di questa foto, dai dati che il telefono ci
    /// scrive (la focale equivalente a 35 mm); None se non ci sono (una foto da WhatsApp).
    pub focale: Option<f32>,
}

impl std::fmt::Debug for FotoScansionata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "FotoScansionata({} × {}, {} byte, bordi trovati {})", self.larghezza, self.altezza, self.jpeg.len(), self.trovati.is_some())
    }
}

impl FotoScansionata {
    /// Gli angoli di tutta la foto.
    pub fn tutta(&self) -> Angoli {
        let (w, h) = (self.larghezza as f32, self.altezza as f32);
        [[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]]
    }

    /// Gli angoli da cui partire: quelli trovati dal motore, un poco più dentro il foglio, o, se
    /// non li ha trovati, un riquadro poco dentro la foto.
    pub fn angoli_di_partenza(&self) -> Angoli {
        let (w, h) = (self.larghezza as f32, self.altezza as f32);
        match self.trovati {
            Some(a) => {
                let centro = [a.iter().map(|p| p[0]).sum::<f32>() / 4.0, a.iter().map(|p| p[1]).sum::<f32>() / 4.0];
                let passo = RIENTRO * w.max(h);
                a.map(|p| {
                    let (dx, dy) = (centro[0] - p[0], centro[1] - p[1]);
                    let l = dx.hypot(dy).max(1e-3);
                    let k = (passo / l).min(0.25);
                    [p[0] + dx * k, p[1] + dy * k]
                })
            }
            None => {
                let (x0, y0, x1, y1) = (w * DENTRO, h * DENTRO, w * (1.0 - DENTRO), h * (1.0 - DENTRO));
                [[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
            }
        }
    }

    /// I punti in mezzo ai lati da cui partire: quelli curvi trovati, portati dentro come gli
    /// angoli; None se il foglio ha i lati dritti (o non si è trovato).
    pub fn lati_di_partenza(&self) -> Option<Lati> {
        let lati = self.lati_trovati?;
        let a = self.trovati?;
        let centro = [a.iter().map(|p| p[0]).sum::<f32>() / 4.0, a.iter().map(|p| p[1]).sum::<f32>() / 4.0];
        let passo = RIENTRO * (self.larghezza as f32).max(self.altezza as f32);
        Some(lati.map(|p| {
            let (dx, dy) = (centro[0] - p[0], centro[1] - p[1]);
            let l = dx.hypot(dy).max(1e-3);
            let k = (passo / l).min(0.25);
            [p[0] + dx * k, p[1] + dy * k]
        }))
    }

    fn rgb(&self) -> Result<RgbImage, ErroreFoto> {
        image::load_from_memory_with_format(&self.jpeg, ImageFormat::Jpeg).map(DynamicImage::into_rgb8).map_err(|_| ErroreFoto::Lettura)
    }

    /// La foto per lo schermo, al più `lato` pixel sul lato lungo.
    pub fn anteprima(&self, lato: u32) -> Result<egui::ColorImage, ErroreFoto> {
        Ok(anteprima_di(DynamicImage::ImageRgb8(self.rgb()?), lato))
    }
}

/// L'immagine per lo schermo, al più `lato` pixel sul lato lungo.
fn anteprima_di(img: DynamicImage, lato: u32) -> egui::ColorImage {
    let piccola = if img.width().max(img.height()) > lato { img.thumbnail(lato, lato) } else { img }.into_rgb8();
    egui::ColorImage::from_rgb([piccola.width() as usize, piccola.height() as usize], piccola.as_raw())
}

/// La foto `byte` (JPEG o PNG) per la scansione: girata come dice l'EXIF, ridotta, in JPEG, coi
/// bordi del foglio se il motore li trova (portati dentro la foto: il motore li lascia uscire un
/// poco).
pub fn prepara_foto(byte: &[u8]) -> Result<FotoScansionata, ErroreFoto> {
    let formato = image::guess_format(byte).map_err(|_| ErroreFoto::Formato)?;
    if !matches!(formato, ImageFormat::Png | ImageFormat::Jpeg) {
        return Err(ErroreFoto::Formato);
    }
    let mut r = ImageReader::with_format(Cursor::new(byte), formato);
    let (w, h) = ImageReader::with_format(Cursor::new(byte), formato).into_dimensions().map_err(|_| ErroreFoto::Lettura)?;
    if u64::from(w) * u64::from(h) > PIXEL_MASSIMI || w.max(h) > LATO_MASSIMO {
        return Err(ErroreFoto::Grande);
    }
    let mut limiti = Limits::default();
    limiti.max_image_width = Some(LATO_MASSIMO);
    limiti.max_image_height = Some(LATO_MASSIMO);
    limiti.max_alloc = Some(1 << 30);
    r.limits(limiti);
    let grande = |e: image::ImageError| if matches!(e, image::ImageError::Limits(_)) { ErroreFoto::Grande } else { ErroreFoto::Lettura };
    let mut decoder = r.into_decoder().map_err(grande)?;
    let orientamento = decoder.orientation().map_err(grande)?;
    let focale_35 = decoder.exif_metadata().ok().flatten().and_then(|e| focale_35mm(&e));
    let mut foto = DynamicImage::from_decoder(decoder).map_err(grande)?;
    foto.apply_orientation(orientamento);
    if foto.width().max(foto.height()) > LATO {
        foto = foto.thumbnail(LATO, LATO);
    }
    let rgba = foto.into_rgba8();
    let (w, h) = rgba.dimensions();
    let (fw, fh) = (w as f32, h as f32);
    // Prima il foglio chiaro su un fondo scuro, trovato dalla sua luce (il riconoscitore del
    // motore, che cerca i bordi dritti, a volte lo mancava: segnalazione del 3/11); se non c'è,
    // quello del motore.
    let dentro = |p: [f32; 2]| [p[0].clamp(0.0, fw), p[1].clamp(0.0, fh)];
    let dalla_luce = crate::foglio::trova_il_foglio(rgba.as_raw(), w, h).filter(|t| angoli_buoni(&t.angoli.map(dentro), w, h));
    let (trovati, lati_trovati) = match dalla_luce {
        Some(t) => (Some(t.angoli.map(dentro)), t.lati.map(|l| l.map(dentro))),
        None => (
            pdfcraft_engine::camera_edges::detect_document(rgba.as_raw(), w, h)
                .map(|q| q.map(|p| [(p[0] as f32).clamp(0.0, fw), (p[1] as f32).clamp(0.0, fh)]))
                .filter(|a| angoli_buoni(a, w, h)),
            None,
        ),
    };
    let jpeg = in_jpeg(&DynamicImage::ImageRgb8(DynamicImage::ImageRgba8(rgba).into_rgb8()), QUALITA_FOTO, None)?;
    // La focale equivalente a 35 mm è riferita alla diagonale del fotogramma (43,27 mm).
    let focale = focale_35.map(|f| f / 43.27 * fw.hypot(fh));
    Ok(FotoScansionata { larghezza: w, altezza: h, jpeg, trovati, lati_trovati, focale })
}

/// La focale equivalente a 35 mm che il telefono scrive nei dati della foto (EXIF, etichetta
/// 0xA405 nella sezione Exif), se c'è.
fn focale_35mm(exif: &[u8]) -> Option<f32> {
    let t = exif.strip_prefix(b"Exif\0\0").unwrap_or(exif);
    let piccolo = match t.get(0..2)? {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let u16_a = |o: usize| t.get(o..o + 2).map(|b| if piccolo { u16::from_le_bytes([b[0], b[1]]) } else { u16::from_be_bytes([b[0], b[1]]) });
    let u32_a = |o: usize| {
        t.get(o..o + 4).map(|b| if piccolo { u32::from_le_bytes([b[0], b[1], b[2], b[3]]) } else { u32::from_be_bytes([b[0], b[1], b[2], b[3]]) })
    };
    let cerca = |sezione: usize, etichetta: u16| {
        let n = usize::from(u16_a(sezione)?);
        (0..n.min(512)).map(|i| sezione + 2 + 12 * i).find(|voce| u16_a(*voce) == Some(etichetta))
    };
    let sezione_exif = u32_a(cerca(u32_a(4)? as usize, 0x8769)? + 8)? as usize;
    let f = u16_a(cerca(sezione_exif, 0xA405)? + 8)?;
    (f > 0).then_some(f32::from(f))
}

/// I punti in mezzo ai quattro lati dritti degli `angoli`: un foglio senza curve.
pub fn lati_dritti(angoli: Angoli) -> Lati {
    crate::stendi::lati_dritti(angoli)
}

/// Almeno un lato si scosta di un pixel e mezzo dal suo mezzo dritto: il foglio va steso fra le
/// curve.
pub fn lati_curvi(angoli: Angoli, lati: Lati) -> bool {
    lati_dritti(angoli).iter().zip(lati).any(|(d, l)| (d[0] - l[0]).hypot(d[1] - l[1]) > 1.5)
}

/// Gli angoli fanno un foglio: stanno nella foto, girano come in alto a sinistra, in alto a destra,
/// in basso a destra, in basso a sinistra (in senso orario, con la y verso il basso), il foglio è
/// convesso e grande almeno un centesimo della foto.
pub fn angoli_buoni(a: &Angoli, larghezza: u32, altezza: u32) -> bool {
    let (w, h) = (larghezza as f32, altezza as f32);
    let dentro = |p: &[f32; 2]| p[0].is_finite() && p[1].is_finite() && (-0.5..=w + 0.5).contains(&p[0]) && (-0.5..=h + 0.5).contains(&p[1]);
    if !a.iter().all(dentro) {
        return false;
    }
    let girano = (0..4).all(|i| {
        let (p, q, r) = (a[i], a[(i + 1) % 4], a[(i + 2) % 4]);
        (q[0] - p[0]) * (r[1] - q[1]) - (q[1] - p[1]) * (r[0] - q[0]) > 0.0
    });
    let area = (0..4).map(|i| a[i][0] * a[(i + 1) % 4][1] - a[(i + 1) % 4][0] * a[i][1]).sum::<f32>() / 2.0;
    girano && area >= AREA_MINIMA * w * h
}

/// I byte di un'immagine in JPEG, con la risoluzione `dpi` se c'è.
fn in_jpeg(img: &DynamicImage, qualita: u8, dpi: Option<u16>) -> Result<Vec<u8>, ErroreFoto> {
    let mut out = Vec::new();
    let mut enc = JpegEncoder::new_with_quality(&mut out, qualita);
    if let Some(d) = dpi {
        enc.set_pixel_density(PixelDensity::dpi(d));
    }
    img.write_with_encoder(enc).map_err(|_| ErroreFoto::Lettura)?;
    Ok(out)
}

/// I byte di una pagina in bianco e nero in PNG a un bit (bianco dal 128 in su), con la sua
/// risoluzione.
fn in_png_a_un_bit(img: &GrayImage, dpi: f64) -> Result<Vec<u8>, ErroreFoto> {
    let (w, h) = img.dimensions();
    let riga = (w as usize).div_ceil(8);
    let mut dati = vec![0u8; riga * h as usize];
    for (y, r) in img.rows().enumerate() {
        for (x, p) in r.enumerate() {
            if p.0[0] >= 128 {
                dati[y * riga + x / 8] |= 0x80 >> (x % 8);
            }
        }
    }
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, w, h);
    enc.set_color(png::ColorType::Grayscale);
    enc.set_depth(png::BitDepth::One);
    let al_metro = (dpi / 0.0254).round() as u32;
    enc.set_pixel_dims(Some(png::PixelDimensions { xppu: al_metro, yppu: al_metro, unit: png::Unit::Meter }));
    let mut scrittore = enc.write_header().map_err(|_| ErroreFoto::Lettura)?;
    scrittore.write_image_data(&dati).map_err(|_| ErroreFoto::Lettura)?;
    scrittore.finish().map_err(|_| ErroreFoto::Lettura)?;
    Ok(out)
}

/// `img` girata di `giri` quarti di giro in senso orario.
fn girata<P: image::Pixel + 'static>(img: image::ImageBuffer<P, Vec<P::Subpixel>>, giri: u8) -> image::ImageBuffer<P, Vec<P::Subpixel>> {
    match giri % 4 {
        1 => image::imageops::rotate90(&img),
        2 => image::imageops::rotate180(&img),
        3 => image::imageops::rotate270(&img),
        _ => img,
    }
}

/// Una pagina pronta: la foto da cui viene, gli angoli, l'aspetto e i quarti di giro con cui è
/// fatta, la sua misura in pixel e il file (JPEG, o PNG a un bit per il bianco e nero) con la
/// risoluzione di un A4 sul lato lungo. Debug senza pixel.
#[derive(Clone, PartialEq)]
pub struct PaginaScansionata {
    pub foto: Arc<FotoScansionata>,
    pub angoli: Angoli,
    pub ritocchi: Ritocchi,
    pub aspetto: Aspetto,
    pub giri: u8,
    pub larghezza: u32,
    pub altezza: u32,
    pub file: Vec<u8>,
}

impl std::fmt::Debug for PaginaScansionata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "PaginaScansionata({} × {}, {}, {} giri, {} byte, raddrizzata {})",
            self.larghezza,
            self.altezza,
            self.aspetto.chiave(),
            self.giri,
            self.file.len(),
            self.raddrizzata()
        )
    }
}

/// La pagina dalla foto `foto` con gli `angoli`, i `ritocchi` (lati curvi, cancellino),
/// l'`aspetto` e `giri` quarti di giro in senso orario. Con gli angoli di tutta la foto e i lati
/// dritti non si raddrizza niente.
pub fn fai_pagina(foto: Arc<FotoScansionata>, angoli: Angoli, ritocchi: Ritocchi, aspetto: Aspetto, giri: u8) -> Result<PaginaScansionata, ErroreFoto> {
    let rgb = foto.rgb()?;
    let (w, h) = rgb.dimensions();
    // Il lato lungo della pagina in millimetri, per la sua misura nel PDF.
    let (rgb, w, h, lungo_mm) = if ritocchi.lati.is_none() && angoli == foto.tutta() {
        let (_, lungo) = proporzioni(f64::from(w) / f64::from(h.max(1)), ritocchi.formato);
        (rgb.into_raw(), w, h, lungo)
    } else {
        if !angoli_buoni(&angoli, w, h) {
            return Err(ErroreFoto::Angoli);
        }
        // Le proporzioni vere del foglio (la prospettiva le accorcia: segnalazione del 3/11) o
        // quelle del formato scelto, voltate come il foglio; il lato lungo tiene i pixel della foto.
        let lati = ritocchi.lati.unwrap_or_else(|| lati_dritti(angoli));
        let (largo, alto) = crate::stendi::lunghezze(angoli, lati);
        let stimato = crate::stendi::proporzione_vera(angoli, f64::from(w), f64::from(h), foto.focale.map(f64::from)).unwrap_or(largo / alto.max(1.0));
        let (corto_su_lungo, lungo_mm) = proporzioni(stimato, ritocchi.formato);
        let lungo = largo.max(alto).round().max(1.0);
        let corto = (lungo * corto_su_lungo).round().max(1.0);
        let misura = if stimato > 1.0 { (lungo as u32, corto as u32) } else { (corto as u32, lungo as u32) };
        let (pixel, pw, ph) = crate::stendi::stendi(&rgb, angoli, lati, Some(misura)).ok_or(ErroreFoto::Angoli)?;
        (pixel, pw, ph, lungo_mm)
    };
    let mut pixel = pdfcraft_engine::camera_clean::apply(&rgb, w, h, aspetto.look(), LIVELLO).ok_or(ErroreFoto::Lettura)?;
    // Il cancellino, sulla pagina pulita e prima dei giri: dove è passato, il colore della carta.
    let passate: Vec<(Vec<[f32; 2]>, f32)> = ritocchi.cancellature.iter().map(|c| (c.punti.clone(), c.raggio)).collect();
    match &mut pixel {
        Pixels::Rgb(p) => crate::stendi::cancella(p, 3, w, h, &passate),
        Pixels::Grey(p) => crate::stendi::cancella(p, 1, w, h, &passate),
    }
    let giri = giri % 4;
    // Il lato lungo grande come quello del formato (un A4, se automatico).
    let dpi = f64::from(w.max(h)) / (lungo_mm / 25.4);
    let punti = dpi.round().clamp(1.0, f64::from(u16::MAX)) as u16;
    let (larghezza, altezza, file) = match pixel {
        Pixels::Rgb(rgb) => {
            let img = girata(RgbImage::from_raw(w, h, rgb).ok_or(ErroreFoto::Lettura)?, giri);
            (img.width(), img.height(), in_jpeg(&DynamicImage::ImageRgb8(img), QUALITA, Some(punti))?)
        }
        Pixels::Grey(grigi) => {
            let img = girata(GrayImage::from_raw(w, h, grigi).ok_or(ErroreFoto::Lettura)?, giri);
            let (larghezza, altezza) = img.dimensions();
            let file = if aspetto == Aspetto::BiancoNero {
                in_png_a_un_bit(&img, dpi)?
            } else {
                in_jpeg(&DynamicImage::ImageLuma8(img), QUALITA, Some(punti))?
            };
            (larghezza, altezza, file)
        }
    };
    Ok(PaginaScansionata { foto, angoli, ritocchi, aspetto, giri, larghezza, altezza, file })
}

impl PaginaScansionata {
    /// Gli angoli non sono quelli di tutta la foto: la prospettiva è stata raddrizzata.
    pub fn raddrizzata(&self) -> bool {
        self.angoli != self.foto.tutta()
    }

    /// Il nome del file nel PDF: «pagina-2.jpg».
    fn nome(&self, i: usize) -> String {
        let estensione = if self.file.starts_with(b"\x89PNG") { "png" } else { "jpg" };
        format!("pagina-{}.{estensione}", i + 1)
    }

    fn immagine(&self) -> Result<DynamicImage, ErroreFoto> {
        image::load_from_memory(&self.file).map_err(|_| ErroreFoto::Lettura)
    }

    /// L'anteprima per lo schermo, al più `lato` pixel sul lato lungo.
    pub fn anteprima(&self, lato: u32) -> Result<egui::ColorImage, ErroreFoto> {
        Ok(anteprima_di(self.immagine()?, lato))
    }
}

/// Che cosa non è andato nel fare il PDF della scansione.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroreScansione {
    /// Nessuna pagina.
    Vuota,
    /// Chi l'ha chiesto ha detto di fermarsi.
    Fermata,
    /// Il motore non ha fatto il PDF: il genere, mai il contenuto.
    Motore(&'static str),
}

/// Il PDF nuovo con le pagine `pagine` e, con `ocr`, lo strato di testo riconosciuto (in un
/// passo solo, poi un salvataggio completo). `avanti(fatte, totale)` si chiama prima di ogni
/// pagina e fra una riga e l'altra: se dice no ci si ferma. Dà i byte e quante parole sono state
/// lette. Le pagine entrano com'erano, senza rifarle: ognuna è grande come un A4 sul lato lungo per
/// la risoluzione scritta nel suo file.
pub fn crea_pdf(
    pagine: &[PaginaScansionata],
    ocr: Option<&Ocr>,
    avanti: &(dyn Fn(usize, usize) -> bool + Sync),
) -> Result<(Vec<u8>, usize), ErroreScansione> {
    if pagine.is_empty() {
        return Err(ErroreScansione::Vuota);
    }
    let immagini: Vec<(String, Vec<u8>)> = pagine.iter().enumerate().map(|(i, p)| (p.nome(i), p.file.clone())).collect();
    let mut sessione = Session::new();
    let byte = sessione.create_from_images_with_resolution(&immagini, ImageResolution::Embedded).map_err(|_| ErroreScansione::Motore("creazione"))?;
    let id = sessione.open("scansione.pdf", None, byte.clone(), None).map_err(|_| ErroreScansione::Motore("apertura"))?;
    let Some(ocr) = ocr else { return Ok((byte.to_vec(), 0)) };
    let geometrie = sessione.get(id).map(|d| d.info.pages.clone()).unwrap_or_default();
    let mut edits = Vec::new();
    let mut parole = 0;
    for (i, p) in pagine.iter().enumerate() {
        if !avanti(i, pagine.len()) {
            return Err(ErroreScansione::Fermata);
        }
        let (Some(info), Ok(img)) = (geometrie.get(i), p.immagine()) else { continue };
        let rgba = img.into_rgba8();
        let righe = match ocr.riconosci(rgba.as_raw(), p.larghezza, p.altezza, &|| !avanti(i, pagine.len())) {
            Ok(r) => r,
            Err(ErroreOcr::Fermato) => return Err(ErroreScansione::Fermata),
            Err(_) => continue,
        };
        let messe = parole_nella_pagina(&righe, info, p.larghezza as f32 / info.width.max(1e-3));
        parole += messe.len();
        if !messe.is_empty() {
            edits.push(Edit::AddOcrText { page: i, words: messe });
        }
    }
    avanti(pagine.len(), pagine.len());
    if edits.is_empty() {
        return Ok((byte.to_vec(), 0));
    }
    sessione.apply(id, Edit::Batch { label: "Riconosci testo".into(), edits }).map_err(|_| ErroreScansione::Motore("testo"))?;
    let salvati = sessione.save_full_bytes(id).map_err(|_| ErroreScansione::Motore("salvataggio"))?;
    Ok((salvati.to_vec(), parole))
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::ocr::test::ocr;
    use crate::prova::foto_di_un_foglio;

    /// Il foglio della foto di prova: 1240 × 1755 pixel (un A4 a 150 dpi) da (120, 140), su un
    /// tavolo scuro.
    const FOGLIO: Angoli = [[120.0, 140.0], [1360.0, 140.0], [1360.0, 1895.0], [120.0, 1895.0]];
    /// La risoluzione nel JPEG è in punti per pollice interi: il lato lungo della pagina è quello
    /// dell'A4 a meno di tanti punti (tre decimi di millimetro su cento).
    const QUASI_A4: f32 = 3.0;

    fn preparata() -> Arc<FotoScansionata> {
        Arc::new(prepara_foto(&foto_di_un_foglio()).expect("preparata"))
    }

    /// Il pixel (x, y) della pagina, in RGB.
    fn pixel(p: &PaginaScansionata, x: u32, y: u32) -> [u8; 3] {
        p.immagine().unwrap().into_rgb8().get_pixel(x, y).0
    }

    #[test]
    fn la_foto_si_prepara_in_jpeg_coi_bordi_trovati() {
        let foto = preparata();
        assert_eq!((foto.larghezza, foto.altezza), (1500, 2055));
        assert!(foto.jpeg.starts_with(&[0xFF, 0xD8]) && foto.jpeg.len() < 1500 * 2055 * 3 / 8, "{foto:?}");
        let trovati = foto.trovati.expect("i bordi del foglio");
        for (t, v) in trovati.iter().zip(FOGLIO) {
            assert!((t[0] - v[0]).abs() < 12.0 && (t[1] - v[1]).abs() < 12.0, "{trovati:?}");
        }
        // Gli angoli partono un poco più dentro, da 5 a 15 pixel per lato.
        let partenza = foto.angoli_di_partenza();
        for (p, (t, segno)) in partenza.iter().zip(trovati.iter().zip([[1.0, 1.0], [-1.0, 1.0], [-1.0, -1.0], [1.0, -1.0]])) {
            let (dx, dy) = ((p[0] - t[0]) * segno[0], (p[1] - t[1]) * segno[1]);
            assert!((5.0..15.0).contains(&dx) && (5.0..15.0).contains(&dy), "{partenza:?} da {trovati:?}");
        }
        assert_eq!(foto.anteprima(500).unwrap().size, [365, 500]);
        assert_eq!(prepara_foto(b"non una foto"), Err(ErroreFoto::Formato));
        // Un foglio bianco senza bordi: gli angoli partono un ventesimo dentro la foto.
        let mut liscia = Vec::new();
        image::RgbImage::from_pixel(800, 600, image::Rgb([235, 235, 230]))
            .write_with_encoder(JpegEncoder::new_with_quality(&mut liscia, 90))
            .unwrap();
        let foto = prepara_foto(&liscia).unwrap();
        assert_eq!(foto.trovati, None);
        assert_eq!(foto.angoli_di_partenza(), [[40.0, 30.0], [760.0, 30.0], [760.0, 570.0], [40.0, 570.0]]);
    }

    #[test]
    fn gli_angoli_devono_fare_un_foglio() {
        assert!(angoli_buoni(&FOGLIO, 1500, 2054));
        // Due angoli scambiati si incrociano; all'incontrario girano al contrario; troppo piccolo;
        // fuori dalla foto.
        let incrociati = [FOGLIO[1], FOGLIO[0], FOGLIO[2], FOGLIO[3]];
        let rovescio = [FOGLIO[3], FOGLIO[2], FOGLIO[1], FOGLIO[0]];
        let piccolo = [[10.0, 10.0], [30.0, 10.0], [30.0, 30.0], [10.0, 30.0]];
        let fuori = [[-50.0, 140.0], FOGLIO[1], FOGLIO[2], FOGLIO[3]];
        for a in [incrociati, rovescio, piccolo, fuori] {
            assert!(!angoli_buoni(&a, 1500, 2054), "{a:?}");
        }
        assert_eq!(fai_pagina(preparata(), incrociati, Ritocchi::default(), Aspetto::Documento, 0).unwrap_err(), ErroreFoto::Angoli);
    }

    #[test]
    fn la_pagina_viene_dagli_angoli_dati_ed_e_piccola() {
        let foto = preparata();
        let pagina = fai_pagina(foto.clone(), foto.angoli_di_partenza(), Ritocchi::default(), Aspetto::Documento, 0).expect("fatta");
        assert!(pagina.raddrizzata());
        // Il foglio senza il tavolo, poco dentro i bordi veri (gli angoli partono un poco dentro).
        assert!((1200..1240).contains(&pagina.larghezza) && (1715..1755).contains(&pagina.altezza), "{pagina:?}");
        // Sui bordi niente filo scuro del tavolo.
        for (x, y) in [
            (0, pagina.altezza / 2),
            (pagina.larghezza - 1, pagina.altezza / 2),
            (pagina.larghezza / 2, 0),
            (pagina.larghezza / 2, pagina.altezza - 1),
        ] {
            assert!(pixel(&pagina, x, y)[0] > 200, "({x}, {y}): {:?}", pixel(&pagina, x, y));
        }
        // La carta è bianca anche dove c'era l'ombra, in JPEG a colori, piccolo.
        assert!(pixel(&pagina, 20, pagina.altezza / 2)[0] > 230 && pixel(&pagina, pagina.larghezza - 20, pagina.altezza - 20)[0] > 230);
        assert!(pagina.file.starts_with(&[0xFF, 0xD8]) && pagina.file.len() < (pagina.larghezza * pagina.altezza) as usize / 2, "{pagina:?}");
        // Gli angoli dati sono quelli usati: la metà di sopra del foglio è alta la metà.
        let [a, b, c, d] = FOGLIO;
        let mezzo = [a, b, [c[0], (b[1] + c[1]) / 2.0], [d[0], (a[1] + d[1]) / 2.0]];
        let sopra = fai_pagina(foto.clone(), mezzo, Ritocchi::default(), Aspetto::Documento, 0).expect("fatta");
        assert!((sopra.altezza as i32 - 878).abs() < 3 && (sopra.larghezza as i32 - 1240).abs() < 3, "{sopra:?}");
        // Tutta la foto: niente si raddrizza, e il tavolo resta.
        let tutta = fai_pagina(foto.clone(), foto.tutta(), Ritocchi::default(), Aspetto::Originale, 0).expect("fatta");
        assert!(!tutta.raddrizzata() && (tutta.larghezza, tutta.altezza) == (1500, 2055));
        assert!(pixel(&tutta, 30, 30)[0] < 90, "il tavolo: {:?}", pixel(&tutta, 30, 30));
        // Nella diagnosi niente pixel.
        assert_eq!(format!("{:?}", tutta).split(',').next(), Some("PaginaScansionata(1500 × 2055"));
    }

    /// I ritocchi della pagina (3/11): i lati curvi la stendono fra le curve, il cancellino
    /// sbianca dove passa, e tutti e due restano quando la pagina si rifà girata.
    #[test]
    fn i_ritocchi_restano_nella_pagina() {
        let foto = preparata();
        let angoli = foto.angoli_di_partenza();
        let dritta = fai_pagina(foto.clone(), angoli, Ritocchi::default(), Aspetto::Grigi, 0).expect("fatta");
        // I lati dritti dati come punti in mezzo: la stessa misura della prospettiva del motore.
        let lati = crate::stendi::lati_dritti(angoli);
        let stesa = fai_pagina(foto.clone(), angoli, Ritocchi { lati: Some(lati), ..Ritocchi::default() }, Aspetto::Grigi, 0).expect("stesa");
        assert!(stesa.larghezza.abs_diff(dritta.larghezza) <= 2 && stesa.altezza.abs_diff(dritta.altezza) <= 2, "{stesa:?} {dritta:?}");
        // Il cancellino su una striscia a sinistra, il 30% della larghezza: lì la pagina è bianca,
        // il resto delle righe resta.
        let scuri = |p: &PaginaScansionata, dove: &dyn Fn(u32, u32, u32, u32) -> bool| {
            let img = image::load_from_memory(&p.file).unwrap().into_luma8();
            let (w, h) = img.dimensions();
            (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).filter(|&(x, y)| dove(x, y, w, h) && img.get_pixel(x, y).0[0] < 100).count()
        };
        let a_sinistra = |x: u32, _: u32, w: u32, _: u32| x * 10 < w * 3;
        let a_destra = |x: u32, _: u32, w: u32, _: u32| x * 10 > w * 3 + w / 50;
        assert!(scuri(&dritta, &a_sinistra) > 100 && scuri(&dritta, &a_destra) > 100, "la prova vuole testo a sinistra e a destra");
        let passate: Vec<Cancellatura> = (0..6).map(|i| Cancellatura { punti: vec![[i as f32 * 0.05 + 0.025, 0.0], [i as f32 * 0.05 + 0.025, 1.0]], raggio: 0.03 }).collect();
        let ritocchi = Ritocchi { lati: None, cancellature: passate, formato: Formato::Automatico };
        let cancellata = fai_pagina(foto.clone(), angoli, ritocchi.clone(), Aspetto::Grigi, 0).expect("cancellata");
        assert_eq!(scuri(&cancellata, &a_sinistra), 0);
        assert!(scuri(&cancellata, &a_destra) > 100);
        // Girata: i ritocchi restano, e il bianco va col foglio (la striscia di sinistra è ora in alto).
        let girata = fai_pagina(foto, angoli, ritocchi.clone(), Aspetto::Grigi, 1).expect("girata");
        assert_eq!(girata.ritocchi, ritocchi);
        assert_eq!(scuri(&girata, &|_, y, _, h| y * 10 < h * 3), 0);
        assert!(scuri(&girata, &|_, _, _, _| true) > 100);
    }

    /// Il formato scelto è la misura vera della pagina nel PDF (l'autore, 3/11: «in modo che il PDF
    /// sappia già cos'è quando stampa»), e la pagina ne prende le proporzioni.
    #[test]
    fn il_formato_scelto_e_la_misura_della_pagina_nel_pdf() {
        let foto = preparata();
        let fatta = |formato| fai_pagina(foto.clone(), FOGLIO, Ritocchi { formato, ..Ritocchi::default() }, Aspetto::Grigi, 0).unwrap();
        let formati = [Formato::A4, Formato::A3, Formato::A5, Formato::Biglietto, Formato::Automatico];
        let pagine: Vec<PaginaScansionata> = formati.iter().map(|f| fatta(*f)).collect();
        let pdf = crate::Pdf::apri(crea_pdf(&pagine, None, &|_, _| true).expect("creato").0, None).expect("riaperto");
        let punti = |mm: f64| (mm * 72.0 / 25.4) as f32;
        for (i, f) in formati.iter().enumerate() {
            let (corto, lungo) = f.millimetri().unwrap_or((210.0, 297.0));
            let m = pdf.misura(i).expect("misura");
            assert!((m.larghezza - punti(corto)).abs() < 4.0 && (m.altezza - punti(lungo)).abs() < 4.0, "{f:?}: {m:?}");
            // Le proporzioni della pagina sono quelle del formato.
            let p = &pagine[i];
            assert!((f64::from(p.larghezza) / f64::from(p.altezza) - corto / lungo).abs() < 0.01, "{f:?}: {p:?}");
        }
    }

    /// Un A4 fotografato col telefono inclinato di 50 gradi (la foto del registro dell'autore, 3/11):
    /// coi lati come si vedono la pagina usciva quasi quadrata, ora è un A4.
    #[test]
    fn un_a4_fotografato_storto_resta_un_a4() {
        let foto = preparata();
        let (w, h) = (f64::from(foto.larghezza), f64::from(foto.altezza));
        let f = 0.75 * w.max(h);
        let a = 50f64.to_radians();
        let distanza = 1.25 * 297.0;
        let angoli = [(-105.0, -148.5), (105.0, -148.5), (105.0, 148.5), (-105.0, 148.5)].map(|(x, y): (f64, f64)| {
            let (y1, z) = (y * a.cos(), y * a.sin() + distanza);
            [(w / 2.0 + f * x / z) as f32, (h / 2.0 + f * y1 / z) as f32]
        });
        let (largo, alto) = crate::stendi::lunghezze(angoli, lati_dritti(angoli));
        assert!(largo / alto > 0.85, "la prova vuole un foglio che si vede schiacciato: {largo} {alto}");
        let p = fai_pagina(foto, angoli, Ritocchi::default(), Aspetto::Grigi, 0).unwrap();
        let r = f64::from(p.larghezza) / f64::from(p.altezza);
        assert!((r - std::f64::consts::FRAC_1_SQRT_2).abs() < 0.002, "{p:?}");
    }

    #[test]
    fn la_focale_si_legge_dai_dati_della_foto() {
        // Un EXIF piccolo, in little endian: la sezione 0 con il puntatore alla sezione Exif, che
        // ha la focale equivalente a 35 mm (26 mm).
        let mut e = b"II*\0".to_vec();
        e.extend(8u32.to_le_bytes());
        e.extend(1u16.to_le_bytes());
        e.extend(0x8769u16.to_le_bytes());
        e.extend(4u16.to_le_bytes());
        e.extend(1u32.to_le_bytes());
        e.extend(26u32.to_le_bytes());
        e.extend(0u32.to_le_bytes());
        e.extend(1u16.to_le_bytes());
        e.extend(0xA405u16.to_le_bytes());
        e.extend(3u16.to_le_bytes());
        e.extend(1u32.to_le_bytes());
        e.extend(26u16.to_le_bytes());
        e.extend([0, 0]);
        assert_eq!(focale_35mm(&e), Some(26.0));
        let mut con_prefisso = b"Exif\0\0".to_vec();
        con_prefisso.extend(&e);
        assert_eq!(focale_35mm(&con_prefisso), Some(26.0));
        assert_eq!(focale_35mm(b"niente"), None);
        assert_eq!(focale_35mm(&e[..20]), None);
    }

    #[test]
    fn i_quattro_aspetti_della_pagina() {
        let foto = preparata();
        let angoli = foto.angoli_di_partenza();
        let fatta = |a| fai_pagina(foto.clone(), angoli, Ritocchi::default(), a, 0).expect("fatta");
        let componenti = |p: &PaginaScansionata| image::load_from_memory(&p.file).unwrap().color().channel_count();
        // Documento: a colori e pulita.
        let documento = fatta(Aspetto::Documento);
        assert_eq!(componenti(&documento), 3);
        // Grigi: JPEG a un canale.
        let grigi = fatta(Aspetto::Grigi);
        assert!(grigi.file.starts_with(&[0xFF, 0xD8]) && componenti(&grigi) == 1);
        // Bianco e nero: PNG a un bit, solo bianco o nero, il più piccolo.
        let bn = fatta(Aspetto::BiancoNero);
        assert!(bn.file.starts_with(b"\x89PNG"));
        let img = bn.immagine().unwrap().into_luma8();
        assert!(img.pixels().all(|p| p.0[0] == 0 || p.0[0] == 255));
        assert!(img.pixels().filter(|p| p.0[0] == 0).count() > 2000, "il testo è sparito");
        assert!(bn.file.len() < grigi.file.len() && bn.file.len() < documento.file.len(), "{bn:?} {grigi:?} {documento:?}");
        // Originale: l'ombra della lampada resta (a sinistra più scuro che a destra).
        let originale = fatta(Aspetto::Originale);
        let (sinistra, destra) = (pixel(&originale, 15, 900)[0], pixel(&originale, originale.larghezza - 15, 900)[0]);
        assert!(sinistra + 20 < destra, "{sinistra} {destra}");
        assert!(pixel(&documento, 15, 900)[0] > 230);
    }

    #[test]
    fn la_pagina_si_gira_in_senso_orario_rifacendola_dalla_foto() {
        // Analisi del 27/10, pagine I5: il foglio fotografato di traverso restava coricato.
        let foto = preparata();
        let angoli = foto.angoli_di_partenza();
        let dritta = fai_pagina(foto.clone(), angoli, Ritocchi::default(), Aspetto::Originale, 0).expect("fatta");
        let girata = fai_pagina(foto.clone(), angoli, Ritocchi::default(), Aspetto::Originale, 1).expect("girata");
        assert_eq!((girata.larghezza, girata.altezza, girata.giri), (dritta.altezza, dritta.larghezza, 1));
        // L'angolo in alto a sinistra va in alto a destra.
        let (a, b) = (pixel(&dritta, 5, 5), pixel(&girata, girata.larghezza - 6, 5));
        assert!(a.iter().zip(b).all(|(x, y)| x.abs_diff(y) < 12), "{a:?} {b:?}");
        assert_eq!(fai_pagina(foto, angoli, Ritocchi::default(), Aspetto::Originale, 4).expect("quattro giri").giri, 0);
    }

    #[test]
    fn due_a4_fotografati_da_distanze_diverse_sono_due_a4() {
        // Analisi del 27/10, pagine I7: con una risoluzione sola, quella della foto più grande, il
        // foglio fotografato da più lontano diventava un A6 (356 × 506 pt). Ora ogni pagina dice
        // la sua risoluzione; anche in bianco e nero.
        let foto = preparata();
        let [a, b, _, d] = FOGLIO;
        let piccolo = [a, [a[0] + 620.0, b[1]], [a[0] + 620.0, a[1] + 877.0], [d[0], a[1] + 877.0]];
        let pagine = vec![
            fai_pagina(foto.clone(), FOGLIO, Ritocchi::default(), Aspetto::Documento, 0).unwrap(),
            fai_pagina(foto.clone(), piccolo, Ritocchi::default(), Aspetto::Grigi, 0).unwrap(),
            fai_pagina(foto, FOGLIO, Ritocchi::default(), Aspetto::BiancoNero, 1).unwrap(),
        ];
        let pdf = crate::Pdf::apri(crea_pdf(&pagine, None, &|_, _| true).expect("creato").0, None).expect("riaperto");
        // Il lato lungo è quello dell'A4; il corto segue le proporzioni della pagina.
        for i in 0..2 {
            let m = pdf.misura(i).expect("misura");
            assert!((m.altezza - 842.0).abs() < QUASI_A4 && (m.larghezza - 595.0).abs() < 4.0, "pagina {i}: {m:?}");
        }
        let m = pdf.misura(2).expect("misura");
        assert!((m.larghezza - 842.0).abs() < QUASI_A4 && (m.altezza - 595.0).abs() < 4.0, "pagina girata: {m:?}");
    }

    #[test]
    fn il_pdf_della_scansione_ha_il_testo_e_la_misura_di_un_a4() {
        let foto = preparata();
        let pagina = fai_pagina(foto.clone(), foto.angoli_di_partenza(), Ritocchi::default(), Aspetto::Documento, 0).expect("fatta");
        let pagine = vec![pagina.clone(), pagina];
        let inizio = std::time::Instant::now();
        let (byte, parole) = crea_pdf(&pagine, Some(ocr()), &|_, _| true).expect("creato");
        eprintln!("PDF di 2 pagine con l'OCR: {:?}, {parole} parole, {} byte", inizio.elapsed(), byte.len());
        assert!(parole >= 30, "{parole}");
        let pdf = crate::Pdf::apri(byte, None).expect("riaperto");
        assert_eq!(pdf.pagine(), 2);
        let m = pdf.misura(0).expect("misura");
        assert!((m.altezza - 842.0).abs() < QUASI_A4 && (m.larghezza - 595.0).abs() < 15.0, "{m:?}");
        let ist = pdf.istantanea();
        let mut r = pdfcraft_render::PageRenderer::new(ist.display.clone(), ist.configurazione.clone());
        for pagina in 0..2 {
            let t = r
                .render(pdfcraft_render::RenderRequest { page: pagina, kind: pdfcraft_render::RequestKind::Text, scale: 1.0, ..Default::default() })
                .text
                .expect("testo");
            for p in ["più", "città", "perché", "libertà"] {
                assert!(!crate::ricerca::trova_nella_pagina(&t, &crate::ricerca::ago(p).expect("ago")).is_empty(), "«{p}» a pagina {pagina}");
            }
        }
        // Senza OCR il PDF c'è lo stesso; senza pagine no; fermato, niente.
        assert_eq!(crea_pdf(&pagine[..1], None, &|_, _| true).expect("creato").1, 0);
        assert_eq!(crea_pdf(&[], None, &|_, _| true), Err(ErroreScansione::Vuota));
        assert_eq!(crea_pdf(&pagine, Some(ocr()), &|_, _| false), Err(ErroreScansione::Fermata));
    }
}
