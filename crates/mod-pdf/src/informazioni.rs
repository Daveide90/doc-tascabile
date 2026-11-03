//! «Informazioni sul documento»: pagine, misura del file, titolo e autore se il PDF li dice, e le
//! firme digitali che il motore ha trovato e controllato all'apertura (`Document::signatures`,
//! ricerca PDFCraft, sezione 9). Titolo, autore e firmatari sono testo del documento: sullo
//! schermo sì, nella diagnosi mai.

use pdfcraft_engine::{SignatureInfo, SignatureStatus};

use crate::apertura::Pdf;

/// Le firme digitali di un PDF: quante ci sono, e come le ha trovate il motore. Senza certificati
/// fidati sul telefono una firma integra resta «non verificata» (`Status::Unknown`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Firme {
    /// I campi firma che hanno una firma (o una marca temporale del documento).
    pub firmate: usize,
    /// Integre e di qualcuno di fidato.
    pub valide: usize,
    /// Rovinate: i byte firmati sono cambiati, la firma non torna, o le modifiche dopo non erano
    /// permesse.
    pub non_valide: usize,
}

impl Firme {
    pub fn da(firme: &[SignatureInfo]) -> Firme {
        let firmate: Vec<&SignatureInfo> = firme.iter().filter(|s| s.signed).collect();
        Firme {
            firmate: firmate.len(),
            valide: firmate.iter().filter(|s| s.status == SignatureStatus::Valid).count(),
            non_valide: firmate.iter().filter(|s| s.status == SignatureStatus::Invalid).count(),
        }
    }

    /// Per lo schermo: «Nessuna», «1 firma, non verificata», «2 firme, valide», «2 firme, 1 non
    /// valida».
    pub fn descrizione(&self) -> String {
        let n = self.firmate;
        if n == 0 {
            return "Nessuna".to_string();
        }
        let quante = if n == 1 { "1 firma".to_string() } else { format!("{n} firme") };
        let come = if self.non_valide > 0 {
            match (n, self.non_valide) {
                (1, _) => "non valida".to_string(),
                (_, k) if k == n => "non valide".to_string(),
                (_, 1) => "1 non valida".to_string(),
                (_, k) => format!("{k} non valide"),
            }
        } else if self.valide == n {
            if n == 1 { "valida" } else { "valide" }.to_string()
        } else if n == 1 {
            "non verificata".to_string()
        } else {
            "non verificate".to_string()
        };
        format!("{quante}, {come}")
    }
}

/// Quello che il foglio delle informazioni mostra di un PDF.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Informazioni {
    pub pagine: usize,
    /// I byte del file com'è stato aperto.
    pub byte: usize,
    pub titolo: Option<String>,
    pub autore: Option<String>,
    pub firme: Firme,
}

/// Un testo del documento da mostrare su una riga: senza a capo né caratteri di controllo e senza
/// spazi ai lati; None se non resta niente.
fn su_una_riga(testo: Option<&str>) -> Option<String> {
    let una_riga: String = testo?.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let pulito = una_riga.split_whitespace().collect::<Vec<_>>().join(" ");
    (!pulito.is_empty()).then_some(pulito)
}

impl Pdf {
    /// Pagine, misura, titolo, autore e firme, da quello che il motore ha letto all'apertura.
    pub fn informazioni(&self) -> Informazioni {
        let d = self.documento();
        Informazioni {
            pagine: d.info.pages.len(),
            byte: d.bytes.len(),
            titolo: su_una_riga(d.info.title.as_deref()),
            autore: su_una_riga(d.info.author.as_deref()),
            firme: Firme::da(&d.signatures),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::prova::{Pagina, pdf, pdf_con_proprieta};
    use pdfcraft_engine::sign::{Certificate, DigitalId, Name, PrivateKey, Time};
    use pdfcraft_engine::{Session, SignOptions};
    use std::sync::Arc;

    #[test]
    fn titolo_autore_pagine_e_misura() {
        let a4 = Pagina::bianca(595.0, 842.0);
        let byte = pdf_con_proprieta(&[a4, a4], "Consenso\ninformato ", "Ambulatorio di prova");
        let misura = byte.len();
        let info = Pdf::apri(byte, None).unwrap().informazioni();
        assert_eq!(
            info,
            Informazioni {
                pagine: 2,
                byte: misura,
                titolo: Some("Consenso informato".into()),
                autore: Some("Ambulatorio di prova".into()),
                firme: Firme::default(),
            }
        );
        // Senza /Info: niente titolo né autore; un titolo di soli spazi non conta.
        let senza = Pdf::apri(pdf(&[a4]), None).unwrap().informazioni();
        assert_eq!((senza.titolo, senza.autore, senza.firme.firmate), (None, None, 0));
        assert_eq!(su_una_riga(Some("  \t ")), None);
    }

    #[test]
    fn le_firme_dette_per_lo_schermo() {
        let f = |firmate, valide, non_valide| Firme { firmate, valide, non_valide }.descrizione();
        assert_eq!(f(0, 0, 0), "Nessuna");
        assert_eq!(f(1, 0, 0), "1 firma, non verificata");
        assert_eq!(f(1, 1, 0), "1 firma, valida");
        assert_eq!(f(1, 0, 1), "1 firma, non valida");
        assert_eq!(f(2, 0, 0), "2 firme, non verificate");
        assert_eq!(f(2, 2, 0), "2 firme, valide");
        assert_eq!(f(2, 1, 1), "2 firme, 1 non valida");
        assert_eq!(f(3, 0, 2), "3 firme, 2 non valide");
        assert_eq!(f(2, 0, 2), "2 firme, non valide");
    }

    /// Un PDF firmato con un'identità fatta qui (chiave P-256, certificato firmato da sé): il
    /// motore trova la firma, integra ma di qualcuno che il telefono non conosce.
    #[test]
    fn un_pdf_firmato_ha_una_firma_non_verificata() {
        let mut s = Session::new();
        let id = s.open("prova.pdf", None, Arc::new(pdf(&[Pagina::bianca(595.0, 842.0)])), None).unwrap();
        let chiave = PrivateKey::generate_p256().unwrap();
        let nome = Name::build("Firmatario di prova", "", "", "", "IT");
        let certificato = Certificate::self_signed(&nome, &chiave, Time::from_unix(1_792_738_800), 5, &[1, 2, 3]).unwrap();
        let identita = DigitalId { key: chiave, certificate: certificato, chain: Vec::new(), friendly_name: None };
        let firmato = s.sign(id, &identita, SignOptions::default()).unwrap();
        let info = Pdf::apri(firmato.to_vec(), None).unwrap().informazioni();
        assert_eq!(info.firme, Firme { firmate: 1, valide: 0, non_valide: 0 });
        assert_eq!(info.firme.descrizione(), "1 firma, non verificata");
    }
}
