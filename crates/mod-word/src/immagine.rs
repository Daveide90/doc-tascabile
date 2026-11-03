//! Le immagini nel documento (piano «Word completo», W3): la foto della galleria o della
//! fotocamera preparata per il documento (girata come dice l'EXIF, ridotta se enorme), inserita
//! al cursore larga al massimo come il testo e scelta; poi la misura col dito (proporzioni
//! tenute), l'allineamento, il testo intorno (in linea o quadrato), la descrizione ed Elimina.
//! Tutto passa dai comandi del motore (`insert.picture`, `picture.size`, `arrange.wrap`,
//! `arrange.align`, `object.altText`, `text.delete`), ognuno un passo di Annulla.

use std::io::Cursor;

use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits};
use serde_json::{Value, json};
use tascabile_platform::diagnosi;
use wordcraft_engine::Session;
use wordcraft_engine::doc::para::{FloatAlign, InlineObject, OBJ, Wrap};
use wordcraft_engine::doc::props::Align;
use wordcraft_engine::doc::{Pos, StoryRef};

use crate::editor::Editor;
use crate::ponte::StatoIme;
use crate::vista::Tocco;

/// Il lato più lungo di una foto nel documento, in pixel: basta per il testo di una pagina A4
/// stampato a 300 punti per pollice (1950 pixel su 16,5 cm).
pub const LATO_MASSIMO: u32 = 2048;
/// Una foto già dritta, non più grande di `LATO_MASSIMO` e più leggera di così entra coi suoi
/// byte, senza i dati nascosti.
const BYTE_DA_TENERE: usize = 4 << 20;
/// Oltre, non si prova nemmeno a ridurla (come la foto della firma): 200 megapixel, 20 000 pixel
/// per lato, 1 GiB per decodificarla.
const PIXEL_DA_RIDURRE: u64 = 200_000_000;
const LATO_DA_RIDURRE: u32 = 20_000;
/// La qualità del JPEG di una foto ridotta.
const QUALITA_JPEG: u8 = 85;
/// Il tetto del file della foto letto dalla cache, come per la foto della firma.
pub const MASSIMO_FILE: u64 = 64 << 20;
/// La misura più piccola col dito, in punti (meno di un centimetro).
pub const LATO_MINIMO_PT: f32 = 24.0;
/// Quanto vale un pixel della foto sulla pagina: 96 pixel per pollice, come Word.
const PT_PER_PIXEL: f32 = 0.75;

/// Perché una foto non entra nel documento.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroreImmagine {
    /// Né JPEG né PNG.
    Formato,
    /// Troppo grande anche da ridurre.
    Grande,
    /// Non si legge.
    Lettura,
}

impl ErroreImmagine {
    /// Per chi usa l'app.
    pub fn messaggio(self) -> &'static str {
        match self {
            ErroreImmagine::Formato => "Scegli una foto JPEG o PNG.",
            ErroreImmagine::Grande => "La foto è troppo grande: scattane una a una risoluzione più bassa.",
            ErroreImmagine::Lettura => "La foto non si legge.",
        }
    }

    /// Per la diagnosi.
    pub fn genere(self) -> &'static str {
        match self {
            ErroreImmagine::Formato => "formato",
            ErroreImmagine::Grande => "troppo grande",
            ErroreImmagine::Lettura => "lettura",
        }
    }
}

/// La foto pronta per il documento, con i suoi pixel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImmaginePronta {
    pub byte: Vec<u8>,
    pub pixel: (u32, u32),
}

/// La foto `byte` (JPEG o PNG) pronta per il documento, sempre senza EXIF, XMP e testi: coi suoi
/// byte se è già dritta, non più grande di `LATO_MASSIMO` e leggera; altrimenti decodificata,
/// girata come dice l'EXIF (il motore non lo legge e la mostrerebbe di traverso), ridotta a
/// `LATO_MASSIMO` sul lato lungo e riscritta, in PNG se ha la trasparenza, in JPEG se no.
pub fn prepara(byte: &[u8]) -> Result<ImmaginePronta, ErroreImmagine> {
    let formato = image::guess_format(byte).map_err(|_| ErroreImmagine::Formato)?;
    if !matches!(formato, ImageFormat::Png | ImageFormat::Jpeg) {
        return Err(ErroreImmagine::Formato);
    }
    let lettore = || ImageReader::with_format(Cursor::new(byte), formato);
    let (w, h) = lettore().into_dimensions().map_err(|_| ErroreImmagine::Lettura)?;
    if w == 0 || h == 0 {
        return Err(ErroreImmagine::Lettura);
    }
    if u64::from(w) * u64::from(h) > PIXEL_DA_RIDURRE || w.max(h) > LATO_DA_RIDURRE {
        return Err(ErroreImmagine::Grande);
    }
    let mut r = lettore();
    let mut limiti = Limits::default();
    limiti.max_image_width = Some(LATO_DA_RIDURRE);
    limiti.max_image_height = Some(LATO_DA_RIDURRE);
    limiti.max_alloc = Some(1 << 30);
    r.limits(limiti);
    let errore = |e: image::ImageError| if matches!(e, image::ImageError::Limits(_)) { ErroreImmagine::Grande } else { ErroreImmagine::Lettura };
    let mut decoder = r.into_decoder().map_err(errore)?;
    let orientamento = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    // Già dritta e piccola: i suoi byte, ma senza EXIF, XMP e testi (la posizione GPS non deve
    // finire nel documento); se non si leggono fino ai dati, si riscrive.
    if orientamento == Orientation::NoTransforms && w.max(h) <= LATO_MASSIMO && byte.len() <= BYTE_DA_TENERE {
        if let Some(pulita) = tascabile_files::senza_metadati(byte) {
            return Ok(ImmaginePronta { byte: pulita, pixel: (w, h) });
        }
    }
    let mut foto = DynamicImage::from_decoder(decoder).map_err(errore)?;
    foto.apply_orientation(orientamento);
    if foto.width().max(foto.height()) > LATO_MASSIMO {
        foto = foto.thumbnail(LATO_MASSIMO, LATO_MASSIMO);
    }
    let pixel = (foto.width(), foto.height());
    let mut out = Vec::new();
    if foto.color().has_alpha() {
        foto.into_rgba8().write_to(&mut Cursor::new(&mut out), ImageFormat::Png).map_err(errore)?;
    } else {
        foto.into_rgb8().write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, QUALITA_JPEG)).map_err(errore)?;
    }
    Ok(ImmaginePronta { byte: out, pixel })
}

/// Come sta il testo attorno all'immagine (`arrange.wrap`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Intorno {
    /// Nella riga, come una lettera grande.
    #[default]
    InLinea,
    /// Il testo le gira attorno, a destra e a sinistra.
    Quadrato,
}

/// Da che parte sta l'immagine.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Lato {
    #[default]
    Sinistra,
    Centro,
    Destra,
}

impl Lato {
    fn valore(self) -> &'static str {
        match self {
            Lato::Sinistra => "left",
            Lato::Centro => "center",
            Lato::Destra => "right",
        }
    }

    /// Il comando dell'allineamento del paragrafo, per un'immagine in linea.
    fn comando_paragrafo(self) -> &'static str {
        match self {
            Lato::Sinistra => "para.alignLeft",
            Lato::Centro => "para.alignCenter",
            Lato::Destra => "para.alignRight",
        }
    }
}

/// L'immagine scelta, per la barra dell'immagine e la maniglia.
#[derive(Debug, Clone, PartialEq)]
pub struct InfoImmagine {
    /// La misura sulla pagina, in punti.
    pub larghezza: f32,
    pub altezza: f32,
    pub intorno: Intorno,
    pub lato: Lato,
    /// Il testo alternativo.
    pub descrizione: String,
    /// La larghezza del testo della pagina, in punti: più larga non si fa.
    pub larghezza_massima: f32,
}

/// L'immagine scelta: la selezione è proprio il carattere di un'immagine del corpo. Restituisce il
/// suo posto e com'è.
pub fn immagine_scelta(s: &Session) -> Option<(Pos, InfoImmagine)> {
    let (a, b) = s.sel.ordered();
    if a.story != StoryRef::Body || a.path != b.path || b.off != a.off + OBJ.len_utf8() {
        return None;
    }
    let p = s.doc.para(a.story, &a.path)?;
    let InlineObject::Image { w, h, alt, float, .. } = p.object_at(a.off)? else { return None };
    let sezione = wordcraft_engine::cmd::page::sect(s);
    let larghezza_massima = sezione.text_width();
    let (intorno, lato) = if float.wrap == Wrap::Inline {
        let rp = s.doc.styles.resolve_para(&p.props);
        let lato = match rp.align.visual(rp.bidi) {
            Align::Center => Lato::Centro,
            Align::Right => Lato::Destra,
            _ => Lato::Sinistra,
        };
        (Intorno::InLinea, lato)
    } else {
        let lato = match float.h_align {
            Some(FloatAlign::Center) => Lato::Centro,
            Some(FloatAlign::End | FloatAlign::Outside) => Lato::Destra,
            Some(FloatAlign::Start | FloatAlign::Inside) => Lato::Sinistra,
            None if float.x <= 1.0 => Lato::Sinistra,
            None if (float.x - (larghezza_massima - w)).abs() <= 1.0 => Lato::Destra,
            None => Lato::Centro,
        };
        (Intorno::Quadrato, lato)
    };
    Some((a, InfoImmagine { larghezza: *w, altezza: *h, intorno, lato, descrizione: alt.clone(), larghezza_massima }))
}

/// I pixel di una foto, dalla sua intestazione.
fn pixel(byte: &[u8]) -> Option<(u32, u32)> {
    ImageReader::new(Cursor::new(byte)).with_guessed_format().ok()?.into_dimensions().ok()
}

impl Editor {
    /// L'immagine sotto il dito, nella vista Scrittura: il posto del suo carattere.
    pub(crate) fn immagine_sotto(&mut self, t: Tocco) -> Option<Pos> {
        let l = self.s.layout();
        let h = l.object_at(0, t.x_pt, t.y_pt, 0.0)?;
        let p = self.s.doc.para(h.story, &h.path)?;
        matches!(p.object_at(h.off), Some(InlineObject::Image { .. })).then(|| h.pos())
    }

    /// Inserisce la foto pronta al cursore, in un paragrafo suo (il testo prima e dopo il cursore
    /// va nei paragrafi accanto, così allinearla non sposta il testo), larga al massimo
    /// `larghezza_massima` punti (lo spazio del testo sullo schermo) e il testo della pagina (lo fa
    /// il motore), mai più della sua misura vera; poi la sceglie, col riquadro e la maniglia. Un
    /// passo di Annulla solo. Vero se è entrata. Nella diagnosi solo pixel e misure.
    pub fn inserisci_immagine(&mut self, foto: &ImmaginePronta, larghezza_massima: f32) -> bool {
        let Some((pw, ph)) = pixel(&foto.byte) else {
            diagnosi("inserisci: immagine che non si legge");
            return false;
        };
        let larga = (pw as f32 * PT_PER_PIXEL).min(larghezza_massima.max(LATO_MINIMO_PT));
        let dati = wordcraft_engine::cmd::insert::base64_encode(&foto.byte);
        // Il testo prima del cursore resta nel suo paragrafo.
        let diviso = self.s.sel.is_collapsed() && self.s.sel.focus.off > 0 && self.esegui("text.newParagraph", json!({}));
        if diviso {
            self.s.join_next_undo();
        }
        let fatto = match self.s.run("insert.picture", &json!({ "data": dati, "width": larga })) {
            Ok(v) => {
                let misura = |k: &str| v.get(k).and_then(Value::as_f64).unwrap_or(0.0);
                diagnosi(format!("inserisci: immagine di {pw}×{ph} pixel, {:.0}×{:.0} pt", misura("width"), misura("height")));
                true
            }
            Err(e) => {
                log::warn!("insert.picture: {}", crate::ponte::genere(&e));
                diagnosi(format!("comando: insert.picture, non riuscito ({})", crate::ponte::genere(&e)));
                false
            }
        };
        if !fatto {
            if diviso {
                self.esegui("edit.undo", json!({}));
            }
            return false;
        }
        // Il motore lascia il cursore subito dopo l'immagine; il testo dopo va nel paragrafo dopo.
        let fine = self.s.sel.focus.clone();
        let pos = Pos { off: fine.off.saturating_sub(OBJ.len_utf8()), ..fine.clone() };
        if self.s.doc.para_at(&fine).is_some_and(|p| p.text.len() > fine.off) {
            self.s.join_next_undo();
            self.esegui("text.newParagraph", json!({}));
        }
        self.scegli_immagine(pos);
        self.vista.scorri_al_cursore();
        true
    }

    /// Sceglie l'immagine in `pos`: la selezione prende il suo carattere, la tastiera si chiude
    /// (come in Word sul telefono) e la vista mostra riquadro e maniglia.
    pub fn scegli_immagine(&mut self, pos: Pos) -> Option<StatoIme> {
        let fine = Pos { off: pos.off + OBJ.len_utf8(), ..pos.clone() };
        self.esegui("select.range", json!({ "anchor": pos, "focus": fine }));
        self.vista.chiudi_barra();
        self.ponte.chiudi();
        diagnosi(format!("immagine scelta: paragrafo {:?}, byte {}", pos.path.0, pos.off));
        crate::editor::svuota_richieste(&mut self.s);
        self.formato = crate::editor::Formato::di(&self.s);
        None
    }

    /// La maniglia lasciata: l'immagine scelta larga `larghezza` punti, con le proporzioni di
    /// prima (`picture.size` con la larghezza sola), fra `LATO_MINIMO_PT` e la larghezza del testo.
    pub fn misura_immagine(&mut self, larghezza: f32) -> Option<StatoIme> {
        let (_, info) = immagine_scelta(&self.s)?;
        let w = larghezza.clamp(LATO_MINIMO_PT, info.larghezza_massima.max(LATO_MINIMO_PT));
        if (w - info.larghezza).abs() < 0.5 {
            return None;
        }
        self.comando_con("picture.size", json!({ "width": w }), &format!("immagine: larga {w:.0} pt"))
    }

    /// Allinea l'immagine scelta: in linea col paragrafo (`para.align…`), col testo intorno nella
    /// pagina (`arrange.align`, dal margine sinistro).
    pub fn allinea_immagine(&mut self, lato: Lato) -> Option<StatoIme> {
        let (_, info) = immagine_scelta(&self.s)?;
        match info.intorno {
            Intorno::InLinea => self.comando(lato.comando_paragrafo()),
            Intorno::Quadrato => self.comando_con("arrange.align", json!({ "value": lato.valore() }), "comando: arrange.align"),
        }
    }

    /// Il testo intorno all'immagine scelta: in linea o quadrato (`arrange.wrap`). Passando al
    /// quadrato resta dalla parte dove stava (`arrange.align` nello stesso passo di Annulla).
    pub fn intorno_immagine(&mut self, intorno: Intorno) -> Option<StatoIme> {
        let (_, info) = immagine_scelta(&self.s)?;
        if info.intorno == intorno {
            return None;
        }
        match intorno {
            Intorno::InLinea => self.comando_con("arrange.wrap", json!({ "wrap": "inline" }), "immagine: in linea"),
            Intorno::Quadrato => {
                if self.esegui("arrange.wrap", json!({ "wrap": "square" })) {
                    self.s.join_next_undo();
                    self.esegui("arrange.align", json!({ "value": info.lato.valore() }));
                    diagnosi("immagine: testo intorno, quadrato");
                }
                self.dopo_selezione()
            }
        }
    }

    /// La descrizione dell'immagine scelta (il testo alternativo, `object.altText`). Nella
    /// diagnosi solo quanto è lunga.
    pub fn descrivi_immagine(&mut self, testo: &str) -> Option<StatoIme> {
        let (_, info) = immagine_scelta(&self.s)?;
        let testo = testo.trim();
        if info.descrizione == testo {
            return None;
        }
        self.comando_con("object.altText", json!({ "text": testo }), &format!("immagine: descrizione di {} caratteri", testo.chars().count()))
    }

    /// Elimina l'immagine scelta.
    pub fn elimina_immagine(&mut self) -> Option<StatoIme> {
        immagine_scelta(&self.s)?;
        self.comando_con("text.delete", json!({}), "immagine: eliminata")
    }
}

#[cfg(test)]
pub(crate) mod test {
    use super::*;
    use crate::editor::Esito;
    use crate::prova::documento;
    use crate::vista::Gesto;

    /// Una foto di prova: `w` per `h` pixel a righe colorate, in PNG (con `alfa` trasparente a
    /// metà) o in JPEG.
    pub(crate) fn foto(w: u32, h: u32, formato: ImageFormat, alfa: bool) -> Vec<u8> {
        let img = image::RgbaImage::from_fn(w, h, |x, y| {
            let a = if alfa && x < w / 2 { 0 } else { 255 };
            image::Rgba([(x * 255 / w.max(1)) as u8, (y * 255 / h.max(1)) as u8, 0x80, a])
        });
        let mut out = Vec::new();
        let dinamica = if alfa { DynamicImage::ImageRgba8(img) } else { DynamicImage::ImageRgb8(DynamicImage::ImageRgba8(img).into_rgb8()) };
        dinamica.write_to(&mut Cursor::new(&mut out), formato).unwrap();
        out
    }

    /// Un JPEG di `w` per `h` pixel che l'EXIF dice girato di 90 gradi a destra (orientamento 6),
    /// come le foto del telefono tenuto in verticale.
    fn foto_girata(w: u32, h: u32) -> Vec<u8> {
        use image::ImageEncoder;
        let img = image::RgbImage::from_fn(w, h, |x, _| image::Rgb([if x < w / 2 { 255 } else { 0 }, 0, 0]));
        // TIFF in little endian: un IFD con la sola voce Orientation (0x0112, SHORT, 6).
        let exif = vec![0x49, 0x49, 0x2A, 0x00, 0x08, 0, 0, 0, 0x01, 0x00, 0x12, 0x01, 0x03, 0x00, 0x01, 0, 0, 0, 0x06, 0x00, 0x00, 0x00, 0, 0, 0, 0];
        let mut out = Vec::new();
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90);
        enc.set_exif_metadata(exif).unwrap();
        enc.write_image(img.as_raw(), w, h, image::ExtendedColorType::Rgb8).unwrap();
        out
    }

    /// Un JPEG dritto di `w` per `h` pixel con l'EXIF di una foto del telefono: l'orientamento 1
    /// e, dopo, una posizione GPS da ritrovare.
    fn foto_col_gps(w: u32, h: u32) -> Vec<u8> {
        use image::ImageEncoder;
        let img = image::RgbImage::from_fn(w, h, |x, _| image::Rgb([x as u8, 0, 0]));
        let mut exif =
            vec![0x49, 0x49, 0x2A, 0x00, 0x08, 0, 0, 0, 0x01, 0x00, 0x12, 0x01, 0x03, 0x00, 0x01, 0, 0, 0, 0x01, 0x00, 0x00, 0x00, 0, 0, 0, 0];
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
    fn la_foto_entra_nel_documento_senza_gps_ne_testi_nascosti() {
        // Analisi del 27/10, critico 2: una foto piccola e dritta entrava coi suoi byte, e nel
        // .docx finiva l'EXIF con la posizione GPS.
        let foto_gps = foto_col_gps(80, 60);
        assert!(contiene(&foto_gps, b"GPS 41.90"), "la foto di prova ha la posizione");
        let pronta = prepara(&foto_gps).unwrap();
        assert_eq!(pronta.pixel, (80, 60));
        assert!(!contiene(&pronta.byte, b"Exif") && !contiene(&pronta.byte, b"GPS 41.90"), "la posizione è rimasta nella foto");
        // Un PNG con un testo dopo l'IHDR (8 byte di firma e 25 dell'IHDR), col suo CRC.
        let png = foto(80, 60, ImageFormat::Png, false);
        let dati = b"Comment\0Casa del paziente";
        let mut crc = flate2::Crc::new();
        crc.update(b"tEXt");
        crc.update(dati);
        let testo = [&(dati.len() as u32).to_be_bytes()[..], &b"tEXt"[..], &dati[..], &crc.sum().to_be_bytes()[..]].concat();
        let pronta = prepara(&[&png[..33], &testo[..], &png[33..]].concat()).unwrap();
        assert_eq!(pronta.pixel, (80, 60));
        assert!(!contiene(&pronta.byte, b"Casa del paziente"), "il testo è rimasto nella foto");
    }

    /// «Prima» e «Dopo» col cursore alla fine di «Prima», poi una foto di 800 per 400 pixel
    /// inserita lì, larga al massimo 240 punti (il testo della vista Scrittura larga 392 dp ne ha
    /// 258).
    pub(crate) fn con_immagine() -> Editor {
        let mut e = Editor::new(documento(&["Prima", "Dopo"]), "prova");
        e.s.run("caret.set", &json!({ "pos": Pos::body(0, 5) })).unwrap();
        e.s.reset_history();
        let pronta = prepara(&foto(800, 400, ImageFormat::Png, false)).unwrap();
        assert!(e.inserisci_immagine(&pronta, 240.0));
        e
    }

    /// Il centro dell'immagine nella vista Scrittura larga 392 dp.
    fn centro_immagine(e: &mut Editor) -> Tocco {
        let l = crate::vista::layout_scrittura(&mut e.s, 392.0, 1.0);
        let o = l.find_object(0, |o| o.text_box.is_none()).expect("l'immagine nel layout");
        Tocco { x_pt: o.rect.x + o.rect.w / 2.0, y_pt: o.rect.y + o.rect.h / 2.0 }
    }

    #[test]
    fn una_foto_piccola_entra_com_e() {
        let png = foto(120, 80, ImageFormat::Png, false);
        let p = prepara(&png).unwrap();
        assert_eq!((p.byte == png, p.pixel), (true, (120, 80)));
        assert_eq!(prepara(b"non un'immagine"), Err(ErroreImmagine::Formato));
        assert_eq!(prepara(&foto(10, 10, ImageFormat::Bmp, false)), Err(ErroreImmagine::Formato));
    }

    #[test]
    fn una_foto_enorme_si_riduce_e_resta_trasparente_se_lo_era() {
        let p = prepara(&foto(3000, 1000, ImageFormat::Jpeg, false)).unwrap();
        assert_eq!(p.pixel, (2048, 683));
        assert!(p.byte.starts_with(&[0xFF, 0xD8]), "una foto senza trasparenza in JPEG");
        let p = prepara(&foto(2600, 2600, ImageFormat::Png, true)).unwrap();
        assert_eq!(p.pixel, (2048, 2048));
        assert!(p.byte.starts_with(&[0x89, b'P', b'N', b'G']), "la trasparenza resta: PNG");
        let ridotta = image::load_from_memory(&p.byte).unwrap();
        assert_eq!(ridotta.to_rgba8().get_pixel(10, 10).0[3], 0);
    }

    #[test]
    fn una_foto_girata_dall_exif_si_raddrizza() {
        let p = prepara(&foto_girata(40, 20)).unwrap();
        assert_eq!(p.pixel, (20, 40), "girata di 90 gradi");
        let dritta = image::load_from_memory(&p.byte).unwrap().to_rgb8();
        assert_eq!(dritta.dimensions(), (20, 40));
        // La metà sinistra rossa, girata a destra, va in alto.
        assert!(dritta.get_pixel(10, 5).0[0] > 200 && dritta.get_pixel(10, 35).0[0] < 60, "{:?}", dritta.get_pixel(10, 5));
    }

    #[test]
    fn inserita_al_cursore_larga_al_massimo_come_il_testo_e_scelta() {
        let mut e = con_immagine();
        assert_eq!(crate::prova::testi(&e.s), ["Prima".to_string(), OBJ.to_string(), "Dopo".to_string()], "in un paragrafo suo");
        let (pos, info) = immagine_scelta(&e.s).expect("l'immagine nuova è scelta");
        assert_eq!(pos, Pos::body(1, 0));
        assert_eq!((info.larghezza, info.altezza, info.intorno), (240.0, 120.0, Intorno::InLinea));
        assert!(!e.ponte.attivo(), "con l'immagine scelta la tastiera si chiude");
        // Più larga del testo della pagina non entra: la misura vera è 600 punti.
        let mut grande = Editor::new(documento(&["Prima"]), "prova");
        grande.inserisci_immagine(&prepara(&foto(800, 400, ImageFormat::Png, false)).unwrap(), 2000.0);
        let (_, info) = immagine_scelta(&grande.s).unwrap();
        assert!((info.larghezza - info.larghezza_massima).abs() < 0.5, "{info:?}");
        // E mai più della sua misura vera.
        let mut piccola = Editor::new(documento(&["Prima"]), "prova");
        piccola.inserisci_immagine(&prepara(&foto(40, 20, ImageFormat::Png, false)).unwrap(), 300.0);
        assert_eq!(immagine_scelta(&piccola.s).map(|(_, i)| (i.larghezza, i.altezza)), Some((30.0, 15.0)));
        // In mezzo a una parola: il testo prima e quello dopo nei paragrafi accanto.
        let mut mezzo = Editor::new(documento(&["Primadopo"]), "prova");
        mezzo.s.run("caret.set", &json!({ "pos": Pos::body(0, 5) })).unwrap();
        mezzo.s.reset_history();
        mezzo.inserisci_immagine(&prepara(&foto(40, 20, ImageFormat::Png, false)).unwrap(), 300.0);
        assert_eq!(crate::prova::testi(&mezzo.s), ["Prima".to_string(), OBJ.to_string(), "dopo".to_string()]);
        assert_eq!(immagine_scelta(&mezzo.s).map(|(p, _)| p), Some(Pos::body(1, 0)));
        mezzo.comando("edit.undo");
        assert_eq!(crate::prova::testi(&mezzo.s), ["Primadopo"], "un passo di Annulla");
        // Un Annulla solo la toglie.
        e.comando("edit.undo");
        assert_eq!(crate::prova::testi(&e.s), ["Prima", "Dopo"]);
        let d = tascabile_platform::diagnosi_testo();
        assert!(d.contains("] inserisci: immagine di 800×400 pixel, 240×120 pt\n"), "{d}");
    }

    #[test]
    fn il_tocco_sull_immagine_la_sceglie_e_quello_sul_testo_la_lascia() {
        let mut e = con_immagine();
        e.s.run("caret.set", &json!({ "pos": Pos::body(2, 2) })).unwrap();
        assert!(immagine_scelta(&e.s).is_none());
        let t = centro_immagine(&mut e);
        assert_eq!(e.gesto(Gesto::Tocco(t)), Esito::default(), "niente tastiera sull'immagine");
        assert!(immagine_scelta(&e.s).is_some());
        // Anche la pressione lunga e il doppio tocco la scelgono, non una parola.
        e.s.run("caret.set", &json!({ "pos": Pos::body(2, 2) })).unwrap();
        e.gesto(Gesto::PressioneLunga(t));
        assert!(immagine_scelta(&e.s).is_some());
        e.s.run("caret.set", &json!({ "pos": Pos::body(2, 2) })).unwrap();
        e.gesto(Gesto::DoppioTocco(t));
        assert!(immagine_scelta(&e.s).is_some());
        // Un tocco su «Dopo»: il cursore lì e la tastiera.
        let l = crate::vista::layout_scrittura(&mut e.s, 392.0, 1.0);
        let c = l.caret_on(&Pos::body(2, 2), 0).unwrap();
        let esito = e.gesto(Gesto::Tocco(Tocco { x_pt: c.x + 0.3, y_pt: c.top + c.height / 2.0 }));
        assert!(esito.apri_tastiera && immagine_scelta(&e.s).is_none());
        let d = tascabile_platform::diagnosi_testo();
        assert!(d.contains("] immagine scelta: paragrafo [1], byte 0\n"), "{d}");
    }

    #[test]
    fn la_misura_col_dito_tiene_le_proporzioni_fra_i_limiti() {
        let mut e = con_immagine();
        e.misura_immagine(200.0);
        let info = |e: &Editor| immagine_scelta(&e.s).map(|(_, i)| (i.larghezza, i.altezza));
        assert_eq!(info(&e), Some((200.0, 100.0)));
        e.misura_immagine(5.0);
        assert_eq!(info(&e), Some((LATO_MINIMO_PT, LATO_MINIMO_PT / 2.0)));
        e.misura_immagine(5000.0);
        let (_, i) = immagine_scelta(&e.s).unwrap();
        assert!((i.larghezza - i.larghezza_massima).abs() < 0.5 && (i.altezza * 2.0 - i.larghezza).abs() < 0.5, "{i:?}");
        // Ognuna un passo di Annulla, e l'immagine resta scelta.
        e.comando("edit.undo");
        e.comando("edit.undo");
        assert_eq!(info(&e).map(|(w, _)| w), Some(200.0));
        let d = tascabile_platform::diagnosi_testo();
        assert!(d.contains("] immagine: larga 200 pt\n"), "{d}");
    }

    #[test]
    fn allinea_testo_intorno_descrizione_ed_elimina() {
        let mut e = con_immagine();
        e.allinea_immagine(Lato::Centro);
        assert_eq!(immagine_scelta(&e.s).map(|(_, i)| (i.intorno, i.lato)), Some((Intorno::InLinea, Lato::Centro)));
        // Il testo intorno: l'immagine galleggia, resta al centro.
        e.intorno_immagine(Intorno::Quadrato);
        let (_, i) = immagine_scelta(&e.s).unwrap();
        assert_eq!((i.intorno, i.lato), (Intorno::Quadrato, Lato::Centro));
        e.allinea_immagine(Lato::Destra);
        assert_eq!(immagine_scelta(&e.s).map(|(_, i)| i.lato), Some(Lato::Destra));
        e.allinea_immagine(Lato::Sinistra);
        assert_eq!(immagine_scelta(&e.s).map(|(_, i)| i.lato), Some(Lato::Sinistra));
        // Annulla: a destra; poi quadrato al centro; poi in linea con un Annulla solo.
        e.comando("edit.undo");
        e.comando("edit.undo");
        assert_eq!(immagine_scelta(&e.s).map(|(_, i)| (i.intorno, i.lato)), Some((Intorno::Quadrato, Lato::Centro)));
        e.comando("edit.undo");
        assert_eq!(immagine_scelta(&e.s).map(|(_, i)| i.intorno), Some(Intorno::InLinea));
        // La descrizione, mai nella diagnosi.
        e.descrivi_immagine("  Medicazione del giorno  ");
        assert_eq!(immagine_scelta(&e.s).map(|(_, i)| i.descrizione), Some("Medicazione del giorno".to_string()));
        e.elimina_immagine();
        assert!(immagine_scelta(&e.s).is_none());
        assert_eq!(crate::prova::testi(&e.s), ["Prima", "", "Dopo"], "resta il suo paragrafo, vuoto, come in Word");
        e.comando("edit.undo");
        assert_eq!(crate::prova::testi(&e.s)[1], OBJ.to_string(), "Annulla rimette l'immagine");
        let d = tascabile_platform::diagnosi_testo();
        assert!(d.contains("] immagine: descrizione di 22 caratteri\n") && d.contains("] immagine: eliminata\n"), "{d}");
        assert!(!d.contains("Medicazione"), "{d}");
    }

    #[test]
    fn nella_vista_scrittura_l_immagine_si_vede() {
        let mut e = con_immagine();
        let l = crate::vista::layout_scrittura(&mut e.s, 392.0, 1.0);
        let o = l.find_object(0, |o| o.text_box.is_none()).expect("l'immagine nel layout");
        assert!(o.rect.right() <= l.pages[0].w, "esce dalla pagina: {:?}", o.rect);
        let fascia = crate::vista::rendi_fascia(&e.s, &l, 0, 600.0, 1.0).unwrap();
        // Al centro dell'immagine i suoi colori, non la carta trasparente.
        let (x, y) = ((o.rect.x + o.rect.w / 2.0) as usize, (o.rect.y + o.rect.h / 2.0) as usize);
        let i = (y * fascia.width as usize + x) * 4;
        assert_eq!(fascia.pixels[i + 3], 255, "l'immagine non è disegnata");
        assert_eq!(fascia.pixels[i + 2], 0x80);
    }

    #[test]
    fn salvata_in_docx_e_riaperta() {
        let mut e = con_immagine();
        e.misura_immagine(240.0);
        e.descrivi_immagine("Lesione al tallone");
        e.intorno_immagine(Intorno::Quadrato);
        let byte = wordcraft_engine::io::save_bytes("immagine.docx", &e.s.doc).expect("salvata");
        let s = crate::documento::apri_byte("immagine.docx", &byte).expect("riaperta");
        let p = s.doc.body[1].as_para().unwrap();
        let o = p.objects.iter().find(|o| matches!(o, InlineObject::Image { .. })).expect("l'immagine nel file");
        let InlineObject::Image { w, h, alt, float, media, .. } = o else { unreachable!() };
        assert_eq!((*w, *h, alt.as_str(), float.wrap), (240.0, 120.0, "Lesione al tallone", Wrap::Square));
        assert!(s.doc.media.get(media).is_some_and(|b| pixel(b) == Some((800, 400))), "i pixel della foto");
    }
}
