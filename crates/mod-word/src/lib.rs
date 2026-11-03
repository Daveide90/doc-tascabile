//! Modulo Word di Tascabile: il motore di WordCraft con un'interfaccia per il pollice.

pub mod documento;
pub mod editor;
pub mod esempi;
pub mod finestra;
pub mod font;
pub mod immagine;
pub mod modelli;
pub mod ortografia;
pub mod pacchetto;
pub mod pagina;
pub mod ponte;
pub mod revisioni;
pub mod salva;
pub mod tabella;
pub mod vista;

#[cfg(test)]
pub(crate) mod prova {
    use serde_json::json;
    use wordcraft_engine::Session;

    /// Documento con questi paragrafi; cursore alla fine dell'ultimo.
    pub fn documento(paragrafi: &[&str]) -> Session {
        let mut s = crate::documento::nuovo();
        for (i, p) in paragrafi.iter().enumerate() {
            if i > 0 {
                s.run("text.newParagraph", &json!({})).unwrap();
            }
            if !p.is_empty() {
                s.run("text.insert", &json!({ "text": p, "raw": true })).unwrap();
            }
        }
        s
    }

    /// Testi dei paragrafi del corpo.
    pub fn testi(s: &Session) -> Vec<String> {
        s.doc.body.iter().filter_map(|b| b.as_para()).map(|p| p.text.clone()).collect()
    }

    pub const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

    /// Un .docx scritto a mano: `corpo` dentro w:body, `sect` dentro w:sectPr (i riferimenti a
    /// intestazioni e piè di pagina) e le parti in più (nome, tipo della relazione, tipo del
    /// contenuto, xml), con le relazioni rIdP0, rIdP1… dal documento.
    pub fn docx(corpo: &str, sect: &str, parti: &[(&str, &str, &str, &str)]) -> Vec<u8> {
        use std::io::Write;
        let mut tipi = String::new();
        let mut rels = String::new();
        for (i, (nome, tipo, ct, _)) in parti.iter().enumerate() {
            tipi.push_str(&format!(r#"<Override PartName="/word/{nome}" ContentType="{ct}"/>"#));
            rels.push_str(&format!(r#"<Relationship Id="rIdP{i}" Type="{R}/{tipo}" Target="{nome}"/>"#));
        }
        let mut voci: Vec<(String, String)> = vec![
            ("[Content_Types].xml".into(), format!(r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>{tipi}</Types>"#)),
            ("_rels/.rels".into(), r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.into()),
            ("word/_rels/document.xml.rels".into(), format!(r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{rels}</Relationships>"#)),
            ("word/document.xml".into(), format!(r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="{W}" xmlns:r="{R}"><w:body>{corpo}<w:sectPr>{sect}<w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1417" w:right="1134" w:bottom="1134" w:left="1134" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr></w:body></w:document>"#)),
        ];
        for (nome, _, _, xml) in parti {
            voci.push((format!("word/{nome}"), xml.to_string()));
        }
        let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (nome, testo) in voci {
            z.start_file(nome, zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(testo.as_bytes()).unwrap();
        }
        z.finish().unwrap().into_inner()
    }

    /// L'XML delle parti del .docx `byte` il cui nome contiene `nome`, uniti.
    pub fn xml_delle_parti(byte: &[u8], nome: &str) -> String {
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(byte)).unwrap();
        let nomi: Vec<String> =
            z.file_names().map(Result::unwrap).filter(|n| n.contains(nome) && n.ends_with(".xml")).map(|n| n.into_owned()).collect();
        let mut xml = String::new();
        for n in nomi {
            std::io::Read::read_to_string(&mut z.by_name(&n).unwrap(), &mut xml).unwrap();
        }
        xml
    }
}
