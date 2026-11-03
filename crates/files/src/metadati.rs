//! Le foto senza i dati nascosti: l'EXIF (la posizione GPS, il telefono, l'ora), l'XMP, i
//! commenti e i testi dei PNG, tolti senza ricomprimere. Servono per le foto che finiscono in un
//! documento (analisi del 27/10, critico 2): per un infermiere di famiglia la posizione di una
//! foto è l'indirizzo del paziente.

/// I byte di `byte` (JPEG o PNG) senza i metadati, coi pixel e i colori di prima; `None` se non è
/// un JPEG o un PNG che si legge fino ai dati (allora la foto va riscritta).
pub fn senza_metadati(byte: &[u8]) -> Option<Vec<u8>> {
    if byte.starts_with(&[0xFF, 0xD8]) {
        jpeg(byte)
    } else if byte.starts_with(PNG) {
        png(byte)
    } else {
        None
    }
}

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";

/// Il JPEG segmento per segmento fino all'inizio dei dati (SOS): restano JFIF (APP0), il profilo
/// dei colori (APP2 con ICC_PROFILE), Adobe (APP14, serve ai colori CMYK) e i segmenti che non
/// sono APP né commenti (tabelle, misure); dal primo SOS in poi tutto com'è.
fn jpeg(byte: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(byte.len());
    out.extend_from_slice(&byte[..2]);
    let mut i = 2;
    loop {
        // Un marcatore: 0xFF (anche più d'uno, di riempimento) e il suo codice.
        if *byte.get(i)? != 0xFF {
            return None;
        }
        let mut j = i + 1;
        while *byte.get(j)? == 0xFF {
            j += 1;
        }
        let codice = byte[j];
        let dopo = j + 1;
        match codice {
            0xDA => {
                out.extend_from_slice(&byte[i..]);
                return Some(out);
            }
            0xD9 => return None,
            0x01 | 0xD0..=0xD8 => {
                out.extend_from_slice(&byte[i..dopo]);
                i = dopo;
                continue;
            }
            _ => {}
        }
        let lunghezza = usize::from(u16::from_be_bytes([*byte.get(dopo)?, *byte.get(dopo + 1)?]));
        if lunghezza < 2 {
            return None;
        }
        let fine = dopo.checked_add(lunghezza)?;
        let dati = byte.get(dopo + 2..fine)?;
        let tieni = match codice {
            0xE0 | 0xEE => true,
            0xE2 => dati.starts_with(b"ICC_PROFILE\0"),
            0xE1..=0xEF | 0xFE => false,
            _ => true,
        };
        if tieni {
            out.extend_from_slice(&byte[i..fine]);
        }
        i = fine;
    }
}

/// Il PNG blocco per blocco fino a IEND: via i testi (tEXt, zTXt, iTXt), l'EXIF (eXIf) e l'ora
/// (tIME); il resto com'è, coi suoi CRC.
fn png(byte: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(byte.len());
    out.extend_from_slice(PNG);
    let mut i = PNG.len();
    loop {
        let lunghezza = usize::try_from(u32::from_be_bytes(byte.get(i..i + 4)?.try_into().ok()?)).ok()?;
        let fine = i.checked_add(12)?.checked_add(lunghezza)?;
        let blocco = byte.get(i..fine)?;
        let tipo = &blocco[4..8];
        if !matches!(tipo, b"tEXt" | b"zTXt" | b"iTXt" | b"eXIf" | b"tIME") {
            out.extend_from_slice(blocco);
        }
        if tipo == b"IEND" {
            return Some(out);
        }
        i = fine;
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn segmento(codice: u8, dati: &[u8]) -> Vec<u8> {
        let mut s = vec![0xFF, codice];
        s.extend_from_slice(&u16::try_from(dati.len() + 2).unwrap().to_be_bytes());
        s.extend_from_slice(dati);
        s
    }

    fn blocco(tipo: &[u8; 4], dati: &[u8]) -> Vec<u8> {
        [&u32::try_from(dati.len()).unwrap().to_be_bytes()[..], &tipo[..], dati, &[1, 2, 3, 4][..]].concat()
    }

    #[test]
    fn un_jpeg_perde_exif_xmp_e_commenti_e_tiene_colori_e_dati() {
        let jfif = segmento(0xE0, b"JFIF\0\x01\x01\0\0\x01\0\x01\0\0");
        let exif = segmento(0xE1, b"Exif\0\0MM\0*\0\0\0\x08 GPS 41.90 N 12.49 E");
        let xmp = segmento(0xE1, b"http://ns.adobe.com/xap/1.0/\0<x:xmpmeta/>");
        let icc = segmento(0xE2, b"ICC_PROFILE\0\x01\x01profilo");
        let flashpix = segmento(0xE2, b"FPXR\0dati della fotocamera");
        let iptc = segmento(0xED, b"Photoshop 3.0\0IPTC");
        let commento = segmento(0xFE, b"Casa del paziente");
        let tabelle = segmento(0xDB, &[0; 65]);
        let adobe = segmento(0xEE, b"Adobe\0\x64\0\0\0\0\x01");
        // Dall'inizio dei dati (SOS) in poi tutto com'è: 0xFF 0x00, un RST, la fine.
        let dati = [&segmento(0xDA, &[1, 1, 0, 0, 0x3F, 0])[..], &[0x12, 0xFF, 0x00, 0x34, 0xFF, 0xD0, 0x56, 0xFF, 0xD9][..]].concat();
        let soi = &[0xFF, 0xD8][..];
        let foto = [soi, &jfif[..], &exif[..], &xmp[..], &icc[..], &flashpix[..], &iptc[..], &commento[..], &tabelle[..], &adobe[..], &dati[..]].concat();
        let pulita = [soi, &jfif[..], &icc[..], &tabelle[..], &adobe[..], &dati[..]].concat();
        assert_eq!(senza_metadati(&foto), Some(pulita));
    }

    #[test]
    fn un_png_perde_testi_exif_e_ora_e_tiene_il_resto() {
        let ihdr = blocco(b"IHDR", &[0; 13]);
        let testo = blocco(b"tEXt", b"Comment\0Casa del paziente");
        let srgb = blocco(b"sRGB", &[0]);
        let exif = blocco(b"eXIf", b"MM\0*GPS");
        let ora = blocco(b"tIME", &[7, 234, 10, 27, 9, 0, 0]);
        let itxt = blocco(b"iTXt", b"XML:com.adobe.xmp\0\0\0\0\0<x/>");
        let ztxt = blocco(b"zTXt", b"Author\0\0x");
        let idat = blocco(b"IDAT", b"pixel");
        let iend = blocco(b"IEND", b"");
        let foto = [PNG, &ihdr[..], &testo[..], &srgb[..], &exif[..], &ora[..], &itxt[..], &ztxt[..], &idat[..], &iend[..]].concat();
        let pulita = [PNG, &ihdr[..], &srgb[..], &idat[..], &iend[..]].concat();
        assert_eq!(senza_metadati(&foto), Some(pulita));
    }

    #[test]
    fn un_file_che_non_si_legge_fino_ai_dati_non_si_pulisce() {
        assert_eq!(senza_metadati(b"GIF89a"), None);
        assert_eq!(senza_metadati(&[0xFF, 0xD8, 0xFF, 0xE1, 0x00, 0x40, b'E']), None, "segmento troncato");
        assert_eq!(senza_metadati(&[0xFF, 0xD8, 0xFF, 0xD9]), None, "la fine prima dei dati");
        assert_eq!(senza_metadati(&[PNG, &[0, 0, 0, 13, b'I', b'H'][..]].concat()), None, "blocco troncato");
        assert_eq!(senza_metadati(&[PNG, &blocco(b"IHDR", &[0; 13])[..]].concat()), None, "senza IEND");
    }
}
