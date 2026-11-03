//! La firma di Fill & Sign (piano «PDF completo», P3): disegnata col dito o presa dalla foto di
//! una firma su carta, posata sulla pagina come un commento che si sposta, si ingrandisce e si
//! annulla.
//!
//! La firma col dito (dalla 0.8.2) tiene di ogni punto la velocità del dito, col tratto scelto
//! (stilografica, penna, pennarello) e il colore (nero, blu): lo spessore viene da lì (modulo
//! `tratto`). Nella pagina va come i contorni pieni dei tratti (`Shape::TypedSignature`, la forma
//! del motore per la firma battuta: un timbro di Fill & Sign riempito col non-zero, nel suo colore),
//! nitida a ogni zoom e uguale a com'era sullo schermo. Fino alla 0.8.1 era un `/Ink` nero da
//! 1,5 pt, tutto uguale («il tratto è troppo omogeneo»). I tratti si tengono normalizzati, con la y
//! verso l'alto, come `SavedSig::Drawn` di ui-egui; la posa gira il riquadro con la pagina come
//! `typed_signature_shape` del motore (`upright_box`, `displayed_offset`).
//!
//! La foto della firma su carta passa da `SignaturePhoto` del motore (via carta, ombre e tavolo,
//! resta l'inchiostro su fondo trasparente), che vuole al massimo 4 MiB e 32 megapixel: una foto
//! del telefono più grande si riduce prima qui, già girata come dice l'EXIF.
//!
//! Le firme sono di chi usa l'app: `Debug` dice solo che genere e quanto è grande, mai i punti.

use std::borrow::Cow;
use std::io::Cursor;

use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits};
use pdfcraft_engine::signature_image::upright_box;
use pdfcraft_engine::signature_scan::{DEFAULT_SENSITIVITY, MAX_PHOTO_BYTES, SCAN_MAX_SIDE};
use pdfcraft_engine::{Edit, MarkFile, NewAnnotation, ScanError, Shape, SignatureImage, SignaturePhoto, Style, displayed_offset};
use pdfcraft_render::PageInfo;

use crate::modifica::Modifica;
use crate::tratto::{Colore, Pezzo, Punto, Stile, meno_il_filo, pezzi, semplifica};

/// Una firma posata con un tocco: larga al massimo 150 pt (come ui-egui) e alta al massimo 50.
pub const LARGA: f32 = 150.0;
pub const ALTA: f32 = 50.0;
/// Le iniziali posate con un tocco.
pub const LARGHE_INIZIALI: f32 = 60.0;
pub const ALTE_INIZIALI: f32 = 36.0;

/// Una firma (o le iniziali) di chi usa l'app.
#[derive(Clone, PartialEq)]
pub enum Firma {
    /// Disegnata col dito: i tratti, il tratto scelto e il colore.
    Tratti(Disegno),
    /// Da una foto o da un'immagine: un PNG del motore, l'inchiostro su fondo trasparente.
    Immagine(SignatureImage),
}

/// Senza i punti e i pixel.
impl std::fmt::Debug for Firma {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Firma::Tratti(d) => write!(f, "Tratti({} tratti, {}, {})", d.tratti.len(), d.stile.chiave(), d.colore.chiave()),
            Firma::Immagine(i) => write!(f, "Immagine({}x{})", i.size()[0], i.size()[1]),
        }
    }
}

/// La prima riga del file di una firma col dito: «1» fino alla 0.8.1 (solo i punti), «2» dalla
/// 0.8.2 (il lato, il tratto, il colore e la velocità di ogni punto).
const INTESTAZIONE_1: &str = "tascabile-firma 1";
const INTESTAZIONE: &str = "tascabile-firma 2";
/// Il lato delle firme salvate prima, che non lo dicevano: con la penna da 2 dp, una firma larga
/// 150 pt ha il tratto di un punto, come il vecchio inchiostro da 1,5.
const LATO_DI_PRIMA: f32 = 300.0;
/// Una firma letta da un file non va oltre questi tratti e punti.
const MASSIMO_TRATTI: usize = 2_000;
const MASSIMO_PUNTI: usize = 200_000;
/// I lati di un capo nei contorni del PDF (mezzo cerchio): lisci anche a 300 pt.
const PASSI_DEL_PDF: usize = 10;
/// Nel PDF i punti dei contorni che si scostano meno di tanto (un quattromillesimo del lato della
/// firma) si tolgono.
const TOLLERANZA_DEL_PDF: f32 = 0.00025;

/// I limiti dei punti: [x0, y0, x1, y1].
fn limiti(punti: impl IntoIterator<Item = [f32; 2]>) -> Option<[f32; 4]> {
    let mut l = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for p in punti {
        l = [l[0].min(p[0]), l[1].min(p[1]), l[2].max(p[0]), l[3].max(p[1])];
    }
    l.iter().all(|v| v.is_finite() && v.abs() < f32::MAX).then_some(l)
}

/// Dove arriva l'inchiostro di `pezzi`, [x0, y0, x1, y1]: i bordi e i cerchi dei capi.
pub fn inchiostro_dei_pezzi(pezzi: &[Pezzo]) -> Option<[f32; 4]> {
    let capi = pezzi
        .iter()
        .flat_map(|p| p.capi)
        .flat_map(|c| [[c.centro[0] - c.raggio, c.centro[1] - c.raggio], [c.centro[0] + c.raggio, c.centro[1] + c.raggio]]);
    limiti(pezzi.iter().flat_map(|p| p.sinistra.iter().chain(&p.destra).copied()).chain(capi))
}

/// Un contorno chiuso: i suoi punti.
pub type Contorno = Vec<[f32; 2]>;

/// La firma col dito: i tratti con la velocità del dito in ogni punto, il tratto scelto e il
/// colore.
#[derive(Clone, PartialEq)]
pub struct Disegno {
    /// Normalizzati sul lato più lungo dei tratti (da 0 a 1), con la y verso l'alto; la velocità
    /// resta in dp al millisecondo.
    pub tratti: Vec<Vec<Punto>>,
    /// Quanti dp era quel lato quando si è firmato: gli spessori stanno in questa misura.
    pub lato: f32,
    pub stile: Stile,
    pub colore: Colore,
}

impl Disegno {
    /// I pezzi pieni di tutti i tratti, nella misura della firma (y verso l'alto). Gli spessori si
    /// contano con la y verso il basso, come quando si è firmato (il pennino della stilografica).
    pub fn pezzi(&self) -> Vec<Pezzo> {
        self.pezzi_meno(0.0)
    }

    /// Come [`Disegno::pezzi`], con gli spessori meno `togli` (nella misura della firma), per lo
    /// schermo (`tratto::meno_il_filo`).
    pub fn pezzi_meno(&self, togli: f32) -> Vec<Pezzo> {
        self.tratti
            .iter()
            .flat_map(|t| {
                let giu: Vec<Punto> = t.iter().map(|p| Punto { y: -p.y, ..*p }).collect();
                let mut spessori = self.stile.spessori(&giu, self.lato);
                meno_il_filo(&mut spessori, togli);
                pezzi(&t.iter().map(|p| [p.x, p.y]).collect::<Vec<_>>(), &spessori)
            })
            .collect()
    }

    /// Dove arriva l'inchiostro, nella misura della firma.
    pub fn inchiostro(&self) -> Option<[f32; 4]> {
        inchiostro_dei_pezzi(&self.pezzi())
    }

    /// I contorni per il PDF: chiusi, senza i punti che non si vedono, normalizzati su dove arriva
    /// l'inchiostro (da 0 a 1, y verso l'alto); e quanto è largo e alto l'inchiostro.
    pub fn contorni(&self) -> Option<(Vec<Contorno>, [f32; 2])> {
        let poligoni: Vec<Vec<[f32; 2]>> =
            self.pezzi().iter().map(|p| semplifica(&p.contorno(PASSI_DEL_PDF), TOLLERANZA_DEL_PDF)).filter(|c| c.len() >= 3).collect();
        let [x0, y0, x1, y1] = limiti(poligoni.iter().flatten().copied())?;
        let (w, h) = ((x1 - x0).max(1e-4), (y1 - y0).max(1e-4));
        let normalizzati = poligoni.iter().map(|c| c.iter().map(|p| [(p[0] - x0) / w, (p[1] - y0) / h]).collect()).collect();
        Some((normalizzati, [w, h]))
    }
}

impl Firma {
    /// La firma dai tratti del dito (dp dello schermo, y verso il basso, con la velocità) col
    /// tratto e il colore scelti: normalizzati sul lato più lungo dei tratti, con la y verso
    /// l'alto. None senza inchiostro.
    pub fn da_tratti(tratti: &[Vec<Punto>], stile: Stile, colore: Colore) -> Option<Firma> {
        let tratti: Vec<Vec<Punto>> = tratti.iter().filter(|t| !t.is_empty()).cloned().collect();
        let [x0, y0, x1, y1] = limiti(tratti.iter().flatten().map(|p| [p.x, p.y]))?;
        let lato = (x1 - x0).max(y1 - y0).max(1.0);
        let tratti = tratti.iter().map(|t| t.iter().map(|p| Punto { x: (p.x - x0) / lato, y: (y1 - p.y) / lato, v: p.v }).collect()).collect();
        Some(Firma::Tratti(Disegno { tratti, lato, stile, colore }))
    }

    /// Quanto è alta sulla sua larghezza (per i tratti, dove arriva l'inchiostro).
    pub fn proporzione(&self) -> f32 {
        match self {
            Firma::Tratti(d) => d.inchiostro().map_or(1.0, |[x0, y0, x1, y1]| (y1 - y0).max(1e-3) / (x1 - x0).max(1e-3)),
            Firma::Immagine(i) => i.size()[1].max(1) as f32 / i.size()[0].max(1) as f32,
        }
    }

    /// Il file della firma salvata: «txt» per i tratti, «png» per un'immagine.
    pub fn estensione(&self) -> &'static str {
        match self {
            Firma::Tratti(_) => "txt",
            Firma::Immagine(_) => "png",
        }
    }

    /// I byte del file della firma salvata: per i tratti l'intestazione, il lato, il tratto e il
    /// colore, poi un tratto per riga (x, y e velocità di ogni punto); o il PNG del motore.
    pub fn in_byte(&self) -> Vec<u8> {
        match self {
            Firma::Immagine(i) => i.bytes().to_vec(),
            Firma::Tratti(d) => {
                let mut s = format!("{INTESTAZIONE}\nlato {:.2} stile {} colore {}", d.lato, d.stile.chiave(), d.colore.chiave());
                for tratto in &d.tratti {
                    s.push('\n');
                    let numeri: Vec<String> =
                        tratto.iter().flat_map(|p| [format!("{:.4}", p.x), format!("{:.4}", p.y), format!("{:.3}", p.v)]).collect();
                    s.push_str(&numeri.join(" "));
                }
                s.push('\n');
                s.into_bytes()
            }
        }
    }

    /// La firma dal suo file. None se non è una firma (o è troppo grande). Una firma salvata fino
    /// alla 0.8.1 (solo i punti) torna come penna nera.
    pub fn da_byte(byte: &[u8]) -> Option<Firma> {
        if byte.starts_with(b"\x89PNG") {
            return SignatureImage::from_bytes(byte).ok().map(Firma::Immagine);
        }
        let testo = std::str::from_utf8(byte).ok()?;
        let mut righe = testo.lines();
        let (lato, stile, colore, passo) = match righe.next()? {
            INTESTAZIONE_1 => (LATO_DI_PRIMA, Stile::Penna, Colore::Nero, 2),
            INTESTAZIONE => {
                let (mut lato, mut stile, mut colore) = (None, Stile::default(), Colore::default());
                let parole: Vec<&str> = righe.next()?.split_ascii_whitespace().collect();
                for coppia in parole.chunks(2) {
                    match coppia {
                        ["lato", v] => lato = v.parse::<f32>().ok().filter(|l| l.is_finite() && (1.0..=1e5).contains(l)),
                        ["stile", v] => stile = Stile::da_chiave(v).unwrap_or_default(),
                        ["colore", v] => colore = Colore::da_chiave(v).unwrap_or_default(),
                        _ => {}
                    }
                }
                (lato?, stile, colore, 3)
            }
            _ => return None,
        };
        let mut tratti = Vec::new();
        let mut punti = 0;
        for riga in righe.filter(|r| !r.trim().is_empty()) {
            let numeri: Vec<f32> = riga.split_ascii_whitespace().map(|n| n.parse::<f32>().ok().filter(|v| v.is_finite())).collect::<Option<_>>()?;
            punti += numeri.len() / passo;
            if !numeri.len().is_multiple_of(passo) || tratti.len() >= MASSIMO_TRATTI || punti > MASSIMO_PUNTI {
                return None;
            }
            tratti.push(numeri.chunks(passo).map(|c| Punto { x: c[0], y: c[1], v: c.get(2).map_or(0.0, |v| v.max(0.0)) }).collect::<Vec<_>>());
        }
        limiti(tratti.iter().flatten().map(|p: &Punto| [p.x, p.y]))?;
        Some(Firma::Tratti(Disegno { tratti, lato, stile, colore }))
    }
}

impl Modifica {
    /// La firma sulla pagina `pagina` (da 0) col lato sinistro sul punto `a` (pt della pagina
    /// come si vede) e centrata in altezza lì, larga al massimo `largo` e alta al massimo `alto`
    /// pt, con le sue proporzioni; diritta come si vede la pagina, anche girata. `autore` va in
    /// `/T`. None se il punto o la firma non vanno.
    #[allow(clippy::too_many_arguments)]
    pub fn firma(
        pagina: usize,
        info: &PageInfo,
        a: [f32; 2],
        firma: &Firma,
        largo: f32,
        alto: f32,
        iniziali: bool,
        autore: &str,
    ) -> Option<Modifica> {
        if !(a.iter().chain(&[largo, alto]).all(|v| v.is_finite()) && largo > 0.0 && alto > 0.0) {
            return None;
        }
        let u = info.view_to_user(a[0], a[1]);
        let at = [f64::from(u[0]), f64::from(u[1])];
        let rotazione = i64::from(info.rotation);
        let (etichetta, nome) = if iniziali { ("Add initials", "Initials") } else { ("Add signature", "Signature") };
        match firma {
            Firma::Tratti(d) => {
                let (contorni, [w, h]) = d.contorni()?;
                let (w, h) = (f64::from(w), f64::from(h));
                let k = (f64::from(largo) / w).min(f64::from(alto) / h);
                let rect = upright_box(rotazione, at, w * k, h * k)?;
                // A destra e in su come si vede, nello spazio utente: un punto del riquadro come si
                // vede va nello stesso punto del riquadro dell'utente (`typed_signature_shape`).
                let [a, b] = displayed_offset(rotazione, [0.0, 0.0], 1.0, 0.0);
                let [c, e] = displayed_offset(rotazione, [0.0, 0.0], 0.0, 1.0);
                let in_utente = |p: &[f32; 2]| {
                    let (x, y) = (f64::from(p[0]), f64::from(p[1]));
                    [a * x + c * y + (-a).max(0.0) + (-c).max(0.0), b * x + e * y + (-b).max(0.0) + (-e).max(0.0)].map(|v| (v * 1e4).round() / 1e4)
                };
                let contours = contorni.iter().map(|contorno| contorno.iter().map(in_utente).collect()).collect();
                let shape = Shape::TypedSignature { rect, contours };
                let mut style = Style::default_for(&shape);
                style.color = d.colore.rgb().map(|v| f64::from(v) / 255.0);
                let nuova = NewAnnotation { page: pagina, shape, style, contents: nome.into(), author: autore.to_string() };
                Some(Modifica::del_motore(Edit::Batch { label: etichetta.into(), edits: vec![Edit::AddAnnotation(nuova)] }))
            }
            Firma::Immagine(i) => {
                let [w, h] = i.size().map(|v| v.max(1) as f64);
                let k = (f64::from(largo) / w).min(f64::from(alto) / h);
                let rect = upright_box(rotazione, at, w * k, h * k)?;
                let file = MarkFile { name: "signature.png".into(), bytes: i.bytes().clone(), page: 0 };
                let edits = vec![Edit::AddCustomStamp { page: pagina, rect, name: nome.into(), file, author: autore.into() }];
                Some(Modifica::del_motore(Edit::Batch { label: etichetta.into(), edits }))
            }
        }
    }

    /// La firma nel riquadro `r` di un campo firma (pt della pagina come si vede): centrata, larga
    /// al massimo nove decimi del riquadro e alta otto.
    pub fn firma_nel_riquadro(pagina: usize, info: &PageInfo, r: [f32; 4], firma: &Firma, iniziali: bool, autore: &str) -> Option<Modifica> {
        let (largo, alto) = ((r[2] - r[0]) * 0.9, (r[3] - r[1]) * 0.8);
        let w = largo.min(alto / firma.proporzione().max(1e-3));
        let a = [(r[0] + r[2]) / 2.0 - w / 2.0, (r[1] + r[3]) / 2.0];
        Modifica::firma(pagina, info, a, firma, w, alto, iniziali, autore)
    }
}

/// Perché una foto non è diventata una firma.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroreFoto {
    /// Non è un'immagine JPEG o PNG.
    Formato,
    /// Troppo grande anche da ridurre.
    Grande,
    /// Nella foto non si trova una firma.
    Inchiostro,
    /// Non si legge.
    Lettura,
    /// Gli angoli scelti per una pagina della scansione non fanno un foglio.
    Angoli,
}

impl ErroreFoto {
    /// Per chi usa l'app.
    pub fn messaggio(self) -> &'static str {
        match self {
            ErroreFoto::Formato => "Scegli una foto JPEG o PNG.",
            ErroreFoto::Grande => "La foto è troppo grande: scattane una a una risoluzione più bassa.",
            ErroreFoto::Inchiostro => "Nella foto non si vede una firma: fotografala da vicino, su un foglio bianco.",
            ErroreFoto::Lettura => "La foto non si legge.",
            ErroreFoto::Angoli => "Gli angoli non fanno un foglio: mettili sui quattro angoli, senza incrociarli.",
        }
    }

    /// Per la diagnosi.
    pub fn genere(self) -> &'static str {
        match self {
            ErroreFoto::Formato => "formato",
            ErroreFoto::Grande => "troppo grande",
            ErroreFoto::Inchiostro => "niente inchiostro",
            ErroreFoto::Lettura => "lettura",
            ErroreFoto::Angoli => "angoli",
        }
    }
}

/// I limiti di SignaturePhoto: 32 megapixel, 8192 pixel per lato.
const PIXEL_DEL_MOTORE: u64 = 32 << 20;
const LATO_DEL_MOTORE: u32 = 8192;
/// La foto che si riduce: al massimo 200 megapixel e 20 000 pixel per lato, e 1 GiB per
/// decodificarla.
const PIXEL_DA_RIDURRE: u64 = 200_000_000;
const LATO_DA_RIDURRE: u32 = 20_000;
/// Il lato più lungo della foto ridotta: più dei 2400 pixel che il motore pulisce.
const LATO_RIDOTTO: u32 = 3000;

/// La foto sotto i limiti del motore (4 MiB, 32 megapixel, 8192 pixel per lato), sempre senza
/// EXIF, XMP e testi: coi suoi byte se ci sta già ed è dritta, altrimenti decodificata, girata
/// come dice l'EXIF, ridotta a 3000 pixel sul lato lungo e riscritta in JPEG.
pub fn riduci_foto(byte: &[u8]) -> Result<Cow<'_, [u8]>, ErroreFoto> {
    let formato = image::guess_format(byte).map_err(|_| ErroreFoto::Formato)?;
    if !matches!(formato, ImageFormat::Png | ImageFormat::Jpeg) {
        return Err(ErroreFoto::Formato);
    }
    let lettore = || ImageReader::with_format(Cursor::new(byte), formato);
    let (w, h) = lettore().into_dimensions().map_err(|_| ErroreFoto::Lettura)?;
    // Il motore incorpora il JPEG com'è: nei limiti si tiene, ma senza i dati nascosti (la
    // posizione GPS non deve finire nel PDF), e solo se è già dritta, perché l'orientamento sta
    // nell'EXIF che si toglie. Se no, o se non si legge fino ai dati, si riscrive.
    let dritta = || lettore().into_decoder().ok().and_then(|mut d| d.orientation().ok()).is_none_or(|o| o == Orientation::NoTransforms);
    if byte.len() <= MAX_PHOTO_BYTES && u64::from(w) * u64::from(h) <= PIXEL_DEL_MOTORE && w.max(h) <= LATO_DEL_MOTORE && dritta() {
        match tascabile_files::senza_metadati(byte) {
            Some(pulita) if pulita.len() == byte.len() => return Ok(Cow::Borrowed(byte)),
            Some(pulita) => return Ok(Cow::Owned(pulita)),
            None => {}
        }
    }
    if u64::from(w) * u64::from(h) > PIXEL_DA_RIDURRE || w.max(h) > LATO_DA_RIDURRE {
        return Err(ErroreFoto::Grande);
    }
    let mut r = lettore();
    let mut limiti = Limits::default();
    limiti.max_image_width = Some(LATO_DA_RIDURRE);
    limiti.max_image_height = Some(LATO_DA_RIDURRE);
    limiti.max_alloc = Some(1 << 30);
    r.limits(limiti);
    let grande = |e: image::ImageError| if matches!(e, image::ImageError::Limits(_)) { ErroreFoto::Grande } else { ErroreFoto::Lettura };
    let mut decoder = r.into_decoder().map_err(grande)?;
    let orientamento = decoder.orientation().map_err(grande)?;
    let mut foto = DynamicImage::from_decoder(decoder).map_err(grande)?;
    foto.apply_orientation(orientamento);
    for (lato, qualita) in [(LATO_RIDOTTO, 90), (2000, 80), (1200, 70)] {
        let piccola = if foto.width().max(foto.height()) > lato { foto.thumbnail(lato, lato) } else { foto.clone() };
        let mut jpeg = Vec::new();
        piccola.into_rgb8().write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, qualita)).map_err(grande)?;
        if jpeg.len() <= MAX_PHOTO_BYTES {
            return Ok(Cow::Owned(jpeg));
        }
    }
    Err(ErroreFoto::Grande)
}

/// Un'immagine da mettere nella pagina (modo Modifica) da una foto o da un'immagine della
/// galleria: ridotta sotto i limiti del motore, dritta e senza i dati nascosti come la foto della
/// firma, con la sua proporzione (larghezza su altezza).
pub fn immagine_da_foto(byte: &[u8]) -> Result<(Vec<u8>, f32), ErroreFoto> {
    let ridotta = riduci_foto(byte)?.into_owned();
    let formato = image::guess_format(&ridotta).map_err(|_| ErroreFoto::Formato)?;
    let (w, h) = ImageReader::with_format(Cursor::new(&ridotta), formato).into_dimensions().map_err(|_| ErroreFoto::Lettura)?;
    if w == 0 || h == 0 {
        return Err(ErroreFoto::Lettura);
    }
    Ok((ridotta, w as f32 / h as f32))
}

/// La firma dalla foto di una firma su carta: ridotta sotto i limiti, poi il motore trova la
/// firma (e la raddrizza se è di traverso) e toglie carta, ombre e tavolo.
pub fn firma_da_foto(byte: &[u8]) -> Result<Firma, ErroreFoto> {
    let ridotta = riduci_foto(byte)?;
    let da_scan = |e: ScanError| match e {
        ScanError::Format => ErroreFoto::Formato,
        ScanError::Size => ErroreFoto::Grande,
        ScanError::NoInk => ErroreFoto::Inchiostro,
        _ => ErroreFoto::Lettura,
    };
    let mut foto = SignaturePhoto::from_bytes(&ridotta).map_err(da_scan)?;
    let cornice = foto.auto_frame();
    foto.scan(cornice, DEFAULT_SENSITIVITY, SCAN_MAX_SIDE).map(Firma::Immagine).map_err(da_scan)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn un_immagine_dalla_galleria_con_la_sua_proporzione() {
        let (byte, proporzione) = immagine_da_foto(&crate::prova::foto_di_una_firma(400, 200, 80)).unwrap();
        assert!(byte.starts_with(&[0xFF, 0xD8]) && (proporzione - 2.0).abs() < 1e-3);
        assert_eq!(immagine_da_foto(b"non un'immagine"), Err(ErroreFoto::Formato));
    }
    use crate::moduli::GenereSegno;

    type Esito = Result<usize, String>;
    const ATTESA: std::time::Duration = std::time::Duration::from_secs(20);

    fn motore(byte: Vec<u8>) -> crate::Motore<Esito> {
        crate::Motore::avvia(crate::Pdf::apri(byte, None).unwrap(), std::sync::Arc::new(|| {})).unwrap()
    }

    /// Una firma di prova col dito: un'onda e un tratto sotto, in dp dello schermo, un punto ogni
    /// 16 ms come i frame.
    fn tratti_del_dito() -> Vec<Vec<Punto>> {
        let onda: Vec<[f32; 3]> = (0..=60).map(|i| [20.0 + 5.0 * i as f32, 100.0 - 30.0 * (i as f32 / 6.0).sin(), 16.0 * i as f32]).collect();
        let sotto: Vec<[f32; 3]> = (0..=10).map(|i| [40.0 + 22.0 * i as f32, 150.0 - i as f32, 16.0 * i as f32]).collect();
        vec![crate::tratto::liscia(&crate::tratto::punti_del_dito(&onda)), crate::tratto::liscia(&crate::tratto::punti_del_dito(&sotto))]
    }

    #[test]
    fn i_tratti_si_normalizzano_e_tornano_uguali_dal_file() {
        let Some(Firma::Tratti(d)) = Firma::da_tratti(&tratti_del_dito(), Stile::Stilografica, Colore::Blu) else { panic!("niente firma") };
        let [x0, y0, x1, y1] = limiti(d.tratti.iter().flatten().map(|p| [p.x, p.y])).unwrap();
        // Sul lato più lungo, da 0 a 1; la y verso l'alto: il tratto sotto ha la y più piccola. Il
        // lato è quello dello schermo (300 dp), e la velocità resta.
        assert!(x0.abs() < 1e-6 && (x1 - 1.0).abs() < 1e-6 && y0.abs() < 1e-6 && y1 < 1.0, "{:?}", [x0, y0, x1, y1]);
        assert!(d.tratti[1][0].y < d.tratti[0][0].y);
        assert!((d.lato - 300.0).abs() < 1e-3 && d.tratti[0].iter().all(|p| p.v > 0.0));
        let firma = Firma::Tratti(d.clone());
        // L'inchiostro esce dai tratti di mezzo spessore: la proporzione è la sua.
        let [i0, j0, i1, j1] = d.inchiostro().unwrap();
        assert!(i0 < 0.0 && j0 < 0.0 && i1 > 1.0 && (firma.proporzione() - (j1 - j0) / (i1 - i0)).abs() < 1e-5);
        let byte = firma.in_byte();
        assert_eq!(firma.estensione(), "txt");
        assert!(byte.starts_with(b"tascabile-firma 2\nlato 300.00 stile stilografica colore blu\n"));
        let Some(Firma::Tratti(riletta)) = Firma::da_byte(&byte) else { panic!("non riletta") };
        assert_eq!((riletta.stile, riletta.colore, riletta.tratti.len()), (Stile::Stilografica, Colore::Blu, 2));
        assert!((riletta.lato - d.lato).abs() < 0.01);
        let vicini = |a: &Punto, b: &Punto| (a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4 && (a.v - b.v).abs() < 1e-3;
        assert!(riletta.tratti.iter().flatten().zip(d.tratti.iter().flatten()).all(|(a, b)| vicini(a, b)));
        // Una firma salvata fino alla 0.8.1 (solo i punti) torna come penna nera.
        let Some(Firma::Tratti(vecchia)) = Firma::da_byte(b"tascabile-firma 1\n0.0 0.5 1.0 0.25\n0.5 0.0 0.5 0.4\n") else { panic!("non letta") };
        assert_eq!((vecchia.stile, vecchia.colore, vecchia.lato), (Stile::Penna, Colore::Nero, LATO_DI_PRIMA));
        assert_eq!(vecchia.tratti[0], vec![Punto { x: 0.0, y: 0.5, v: 0.0 }, Punto { x: 1.0, y: 0.25, v: 0.0 }]);
        // Un file che non è una firma non si legge; nemmeno senza inchiostro, né senza il lato.
        assert!(Firma::da_byte(b"altro").is_none() && Firma::da_byte(b"tascabile-firma 1\n1 2 3\n").is_none());
        assert!(Firma::da_tratti(&[vec![]], Stile::Penna, Colore::Nero).is_none() && Firma::da_byte(b"tascabile-firma 1\n").is_none());
        assert!(Firma::da_byte(b"tascabile-firma 2\nstile penna colore nero\n0 0 1 1 1 1\n").is_none());
        assert!(Firma::da_byte(b"tascabile-firma 2\nlato 300 stile penna colore nero\n0 0 1 1\n").is_none());
        // Nella diagnosi niente punti.
        assert_eq!(format!("{firma:?}"), "Tratti(2 tratti, stilografica, blu)");
    }

    /// Un tratto dritto per gli spessori: lento nella prima metà (un quinto di dp al millisecondo)
    /// e veloce nella seconda (tre dp al millisecondo), in dp dello schermo.
    fn lento_e_veloce() -> Vec<Vec<Punto>> {
        let mut grezzi = Vec::new();
        let (mut x, mut t) = (0.0, 0.0);
        while x < 150.0 {
            grezzi.push([x, 100.0, t]);
            (x, t) = (x + 2.0, t + 10.0);
        }
        while x <= 300.0 {
            grezzi.push([x, 100.0, t]);
            (x, t) = (x + 12.0, t + 4.0);
        }
        vec![crate::tratto::liscia(&crate::tratto::punti_del_dito(&grezzi))]
    }

    /// I byte del PDF del motore, salvato.
    fn salvato(m: &mut crate::Motore<Esito>) -> Vec<u8> {
        let salvati = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let dove = salvati.clone();
        m.scrivi(
            Box::new(move |b: &std::sync::Arc<Vec<u8>>| {
                *dove.lock().unwrap() = b.to_vec();
                Ok(b.len())
            }),
            true,
        );
        m.aspetta(ATTESA);
        salvati.lock().unwrap().clone()
    }

    /// La firma `lento_e_veloce` col tratto e il colore dati, posata larga 300 pt, salvata e
    /// disegnata a 2 pixel per punto: quanti pixel scuri ha la colonna a un terzo (dove il dito
    /// andava piano) e a tre quarti (dove correva), il colore più scuro della prima, e i byte.
    fn disegnata(stile: Stile, colore: Colore) -> (usize, usize, egui::Color32, Vec<u8>) {
        let firma = Firma::da_tratti(&lento_e_veloce(), stile, colore).unwrap();
        let mut m = motore(crate::prova::pdf(&[crate::prova::Pagina::bianca(595.0, 842.0)]));
        let info = m.stato().geometrie[0].clone();
        m.applica(Modifica::firma(0, &info, [100.0, 400.0], &firma, 300.0, 100.0, false, "").unwrap(), true);
        m.aspetta(ATTESA);
        let [x0, _, x1, _] = m.stato().segni[0].rettangolo;
        let byte = salvato(&mut m);
        let img = crate::Pdf::apri(byte.clone(), None).unwrap().disegnatore().disegna(0, 2.0).unwrap();
        let colonna = |x: f32| {
            let x = (x * 2.0) as usize;
            let scuri: Vec<egui::Color32> = (0..img.size[1])
                .map(|y| img.pixels[y * img.size[0] + x])
                .filter(|c| u16::from(c.r()) + u16::from(c.g()) + u16::from(c.b()) < 480)
                .collect();
            let piu_scuro =
                scuri.iter().min_by_key(|c| u16::from(c.r()) + u16::from(c.g()) + u16::from(c.b())).copied().unwrap_or(egui::Color32::WHITE);
            (scuri.len(), piu_scuro)
        };
        let (lenta, scuro) = colonna(x0 + (x1 - x0) / 3.0);
        let (veloce, _) = colonna(x0 + (x1 - x0) * 0.75);
        (lenta, veloce, scuro, byte)
    }

    /// Nel PDF la firma col dito è un timbro di Fill & Sign coi contorni pieni (la forma del motore
    /// per la firma battuta): con la stilografica il tratto è pieno dove il dito andava piano e
    /// sottile dove correva, con la penna è uguale, e il blu è blu.
    #[test]
    fn nel_pdf_la_firma_col_dito_ha_gli_spessori_e_il_colore_del_tratto() {
        let (lenta, veloce, scuro, byte) = disegnata(Stile::Stilografica, Colore::Nero);
        assert!(lenta >= 2 * veloce && veloce >= 2, "stilografica: {lenta} pixel dove andava piano, {veloce} dove correva");
        assert!(scuro.r() < 60 && scuro.b() < 60, "{scuro:?}");
        // Contorni e non inchiostro, e pochi: il file cresce di poco.
        let vuoto = crate::prova::pdf(&[crate::prova::Pagina::bianca(595.0, 842.0)]).len();
        assert!(byte.len() < vuoto + 12_000, "{} byte, vuoto {vuoto}", byte.len());
        let riaperto = crate::Pdf::apri(byte, None).unwrap().segni();
        assert_eq!(riaperto.iter().map(|s| s.genere).collect::<Vec<_>>(), vec![GenereSegno::Firma]);
        let (lenta, veloce, _, _) = disegnata(Stile::Penna, Colore::Nero);
        assert!(lenta.abs_diff(veloce) <= 1 && veloce >= 3, "penna: {lenta} e {veloce} pixel");
        let (pennarello, _, _, _) = disegnata(Stile::Pennarello, Colore::Nero);
        assert!(pennarello >= 2 * lenta, "pennarello: {pennarello} pixel, penna {lenta}");
        let (_, _, blu, _) = disegnata(Stile::Penna, Colore::Blu);
        assert!(blu.b() > blu.r() + 60 && blu.b() > blu.g() + 40, "{blu:?}");
    }

    /// La firma col dito posata, ingrandita, salvata e riaperta col motore vero; anche sul
    /// riquadro di un campo firma.
    #[test]
    fn la_firma_col_dito_si_posa_si_ingrandisce_e_resta_nel_file() {
        use crate::Evento;
        let originale = crate::prova::modulo_da_compilare();
        let mut m = motore(originale.clone());
        let info = m.stato().geometrie[0].clone();
        let firma = Firma::da_tratti(&tratti_del_dito(), Stile::Stilografica, Colore::Nero).unwrap();
        m.applica(Modifica::firma(0, &info, [100.0, 300.0], &firma, LARGA, ALTA, false, "").unwrap(), true);
        assert!(matches!(m.aspetta(ATTESA)[0].evento, Evento::Applicata { .. }), "la firma non è posata");
        let segni = m.stato().segni.clone();
        assert_eq!(segni.len(), 1);
        let s = segni[0].clone();
        assert_eq!(s.genere, GenereSegno::Firma);
        // Il lato sinistro sul punto, larga 150 pt (con l'inchiostro), centrata lì.
        let [x0, y0, x1, y1] = s.rettangolo;
        assert!((x0 - 100.0).abs() < 0.5 && ((x1 - x0) - LARGA).abs() < 0.5, "{:?}", s.rettangolo);
        assert!(((y0 + y1) / 2.0 - 300.0).abs() < 0.5 && y1 - y0 <= ALTA + 0.5, "{:?}", s.rettangolo);
        // Al doppio con la maniglia.
        let r = [x0, y0, x0 + 2.0 * (x1 - x0), y0 + 2.0 * (y1 - y0)];
        m.applica(Modifica::misura_segno(&s, &info, r).unwrap(), true);
        m.aspetta(ATTESA);
        let grande = m.stato().segni[0].rettangolo;
        assert!(((grande[2] - grande[0]) - (r[2] - r[0])).abs() < 1.0, "{grande:?}");
        // Annullata l'ingrandita, torna com'era.
        m.annulla();
        m.aspetta(ATTESA);
        assert!((m.stato().segni[0].rettangolo[2] - x1).abs() < 0.5);
        // Sul riquadro del campo firma: centrata dentro.
        let campo = m.stato().campi.iter().find(|c| c.genere == crate::GenereCampo::Firma).cloned().expect("il campo firma");
        assert!(!campo.compilabile() && crate::da_compilare(&m.stato().campi) == 2);
        let riquadro = campo.posti[0].rettangolo;
        m.applica(Modifica::firma_nel_riquadro(0, &info, riquadro, &firma, false, "").unwrap(), true);
        m.aspetta(ATTESA);
        let nel = m.stato().segni.iter().max_by_key(|s| s.indice).map(|s| s.rettangolo).unwrap();
        assert!(nel[0] > riquadro[0] && nel[2] < riquadro[2], "{nel:?} fuori da {riquadro:?}");
        assert!(nel[1] >= riquadro[1] - 1.0 && nel[3] <= riquadro[3] + 1.0, "{nel:?} fuori da {riquadro:?}");
        // Salvata e riaperta: due firme di Fill & Sign.
        let salvati = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let dove = salvati.clone();
        m.scrivi(
            Box::new(move |b: &std::sync::Arc<Vec<u8>>| {
                *dove.lock().unwrap() = b.to_vec();
                Ok(b.len())
            }),
            true,
        );
        m.aspetta(ATTESA);
        let salvati = salvati.lock().unwrap().clone();
        assert_eq!(salvati.windows(5).filter(|w| w == b"%%EOF").count(), 1, "il salvataggio ha lasciato le revisioni di prima");
        let riaperto = crate::Pdf::apri(salvati, None).unwrap();
        assert_eq!(riaperto.segni().iter().filter(|s| s.genere == GenereSegno::Firma).count(), 2);
    }

    /// Su una pagina girata la firma si legge diritta, come si vede: più larga che alta.
    #[test]
    fn su_una_pagina_girata_la_firma_resta_diritta() {
        let girata = crate::prova::pdf(&[crate::prova::Pagina { larghezza: 595.0, altezza: 842.0, rotazione: 90, contenuto: "" }]);
        let mut m = motore(girata);
        let info = m.stato().geometrie[0].clone();
        let firma = Firma::da_tratti(&tratti_del_dito(), Stile::Penna, Colore::Nero).unwrap();
        m.applica(Modifica::firma(0, &info, [400.0, 200.0], &firma, LARGA, ALTA, false, "").unwrap(), true);
        m.aspetta(ATTESA);
        let [x0, y0, x1, y1] = m.stato().segni[0].rettangolo;
        assert!(x1 - x0 > y1 - y0 && (x0 - 400.0).abs() < 3.0 && ((y0 + y1) / 2.0 - 200.0).abs() < 2.0, "{:?}", [x0, y0, x1, y1]);
    }

    /// La foto di una firma su carta più grande dei limiti del motore (32 megapixel): ridotta,
    /// pulita, posata come immagine e salvata come firma; la firma salvata si rilegge.
    #[test]
    fn la_firma_da_una_foto_grande_si_riduce_si_pulisce_e_si_posa() {
        let foto = crate::prova::foto_di_una_firma(7000, 5000, 85);
        let (w, h) = (7000u64, 5000u64);
        assert!(w * h > PIXEL_DEL_MOTORE);
        assert!(SignaturePhoto::from_bytes(&foto).is_err(), "il motore la prenderebbe già");
        let ridotta = riduci_foto(&foto).unwrap();
        assert!(matches!(ridotta, Cow::Owned(_)) && ridotta.len() <= MAX_PHOTO_BYTES);
        let firma = firma_da_foto(&foto).unwrap();
        let Firma::Immagine(img) = &firma else { panic!("non un'immagine") };
        // L'inchiostro su fondo trasparente, più largo che alto.
        let [iw, ih] = img.size();
        assert!(iw > ih, "{iw}x{ih}");
        let trasparenti = img.rgba().as_chunks::<4>().0.iter().filter(|p| p[3] == 0).count();
        assert!(trasparenti > iw * ih / 3, "lo sfondo non è trasparente");
        assert_eq!(firma.estensione(), "png");
        assert!(matches!(Firma::da_byte(&firma.in_byte()), Some(Firma::Immagine(_))));
        // Posata: un timbro di Fill & Sign largo al massimo 150 pt.
        let mut m = motore(crate::prova::modulo());
        let info = m.stato().geometrie[0].clone();
        m.applica(Modifica::firma(0, &info, [100.0, 400.0], &firma, LARGA, ALTA, false, "").unwrap(), true);
        m.aspetta(ATTESA);
        let s = m.stato().segni[0].clone();
        assert_eq!(s.genere, GenereSegno::Firma);
        assert!(s.rettangolo[2] - s.rettangolo[0] <= LARGA + 1.0 && (s.rettangolo[0] - 100.0).abs() < 1.0, "{:?}", s.rettangolo);
        // Una foto piccola resta com'è; una che non è una foto no; un foglio bianco non ha firma.
        let piccola = crate::prova::foto_di_una_firma(1200, 900, 90);
        assert!(matches!(riduci_foto(&piccola), Ok(Cow::Borrowed(_))));
        assert_eq!(riduci_foto(b"GIF89a....").unwrap_err(), ErroreFoto::Formato);
        let mut bianca = Vec::new();
        image::RgbImage::from_pixel(800, 600, image::Rgb([235, 235, 230]))
            .write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bianca, 90))
            .unwrap();
        assert_eq!(firma_da_foto(&bianca).unwrap_err(), ErroreFoto::Inchiostro);
    }

    /// Un JPEG di `w` per `h` pixel con l'EXIF di una foto del telefono: l'orientamento
    /// `orientamento` e, dopo, una posizione GPS da ritrovare.
    fn foto_col_gps(w: u32, h: u32, orientamento: u8) -> Vec<u8> {
        use image::ImageEncoder;
        let img = image::RgbImage::from_fn(w, h, |x, _| image::Rgb([x as u8, 0, 0]));
        let mut exif =
            vec![0x49, 0x49, 0x2A, 0x00, 0x08, 0, 0, 0, 0x01, 0x00, 0x12, 0x01, 0x03, 0x00, 0x01, 0, 0, 0, orientamento, 0, 0, 0, 0, 0, 0, 0];
        exif.extend_from_slice(b"GPS 41.90 N 12.49 E");
        let mut out = Vec::new();
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90);
        enc.set_exif_metadata(exif).unwrap();
        enc.write_image(img.as_raw(), w, h, image::ExtendedColorType::Rgb8).unwrap();
        out
    }

    fn contiene(byte: &[u8], cosa: &[u8]) -> bool {
        byte.windows(cosa.len()).any(|w| w == cosa)
    }

    #[test]
    fn un_immagine_entra_nel_pdf_senza_la_posizione_gps() {
        // Analisi del 27/10: il motore incorpora il JPEG com'è, e una foto nei limiti portava nel
        // PDF l'EXIF con la posizione GPS.
        let (byte, proporzione) = immagine_da_foto(&foto_col_gps(80, 60, 1)).unwrap();
        assert!(!contiene(&byte, b"Exif") && !contiene(&byte, b"GPS 41.90"), "la posizione è rimasta nella foto");
        assert_eq!(proporzione, 80.0 / 60.0);
        // Girata dall'EXIF (orientamento 6): l'orientamento sta nell'EXIF, quindi si raddrizza qui.
        let (byte, proporzione) = immagine_da_foto(&foto_col_gps(80, 60, 6)).unwrap();
        assert!(!contiene(&byte, b"Exif") && !contiene(&byte, b"GPS 41.90"), "la posizione è rimasta nella foto girata");
        let dritta = image::load_from_memory(&byte).unwrap();
        assert_eq!((dritta.width(), dritta.height(), proporzione), (60, 80, 60.0 / 80.0));
        // Anche la foto di una firma arriva al motore senza la posizione.
        assert!(!contiene(&riduci_foto(&foto_col_gps(80, 60, 1)).unwrap(), b"GPS 41.90"));
    }
}
