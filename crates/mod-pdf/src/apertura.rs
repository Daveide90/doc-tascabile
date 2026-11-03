//! Aprire un PDF dai suoi byte con il motore di PDFCraft.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use pdfcraft_engine::{DocId, Document, Session};
use pdfcraft_render::OpenError;

/// Il tetto di un PDF da aprire: il motore tiene in memoria tutto il file, a volte due volte
/// (ricerca PDFCraft, sezione 2).
pub const LIMITE_MB: usize = 200;

/// Il nome del documento per il motore: sempre questo, così il nome vero del file non entra mai
/// nei suoi messaggi. I byte arrivano dal ContentResolver, quindi niente percorso.
const NOME: &str = "documento.pdf";

/// I modi in cui un PDF non si apre. Nella diagnosi va solo `diagnosi()`, sullo schermo il
/// messaggio: tutti e due fissi, mai il messaggio del motore, che può riportare pezzi del file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Genere {
    Vuoto,
    TroppoGrande,
    ChiedePassword,
    PasswordSbagliata,
    Illeggibile,
    NonSupportato,
    PanicoDelLettore,
}

impl Genere {
    /// Il genere per la diagnosi.
    pub fn diagnosi(self) -> &'static str {
        match self {
            Genere::Vuoto => "file vuoto",
            Genere::TroppoGrande => "oltre 200 MB",
            Genere::ChiedePassword => "protetto da password",
            Genere::PasswordSbagliata => "password sbagliata",
            Genere::Illeggibile => "PDF illeggibile",
            Genere::NonSupportato => "PDF non supportato",
            Genere::PanicoDelLettore => "panico del lettore",
        }
    }

    /// Il messaggio per lo schermo, che segue «Non riesco ad aprire il file: ».
    pub fn messaggio(self) -> &'static str {
        match self {
            Genere::Vuoto => "il file è vuoto.",
            Genere::TroppoGrande => "il PDF supera i 200 MB: in questa versione non si apre.",
            Genere::ChiedePassword => "il PDF è protetto da password.",
            Genere::PasswordSbagliata => "la password non è giusta.",
            Genere::Illeggibile => "non è un PDF che si legge, forse è rovinato.",
            Genere::NonSupportato => "il PDF usa qualcosa che il lettore non sa ancora leggere.",
            Genere::PanicoDelLettore => "il lettore dei PDF si è fermato con un errore interno.",
        }
    }
}

/// Un PDF che non si apre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErroreApertura {
    pub genere: Genere,
}

impl ErroreApertura {
    /// Il PDF chiede una password, o quella data non è giusta: si può chiedere di nuovo.
    pub fn chiede_password(&self) -> bool {
        matches!(self.genere, Genere::ChiedePassword | Genere::PasswordSbagliata)
    }
}

impl std::fmt::Display for ErroreApertura {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.genere.messaggio())
    }
}

impl std::error::Error for ErroreApertura {}

impl From<Genere> for ErroreApertura {
    fn from(genere: Genere) -> Self {
        Self { genere }
    }
}

/// Il file comincia come un PDF: `%PDF-` nei primi 1024 byte, dove lo cercano anche gli altri
/// lettori (la spazzatura prima dell'intestazione è ammessa). Non dice se poi si apre.
pub fn sembra_pdf(byte: &[u8]) -> bool {
    const INTESTAZIONE: &[u8] = b"%PDF-";
    let inizio = &byte[..byte.len().min(1024 + INTESTAZIONE.len() - 1)];
    inizio.windows(INTESTAZIONE.len()).any(|w| w == INTESTAZIONE)
}

/// Un PDF aperto: una sessione del motore tutta sua con un documento solo.
pub struct Pdf {
    pub(crate) sessione: Session,
    pub(crate) id: DocId,
}

/// La misura di una pagina in punti tipografici, come si vede: dopo `/Rotate` e `/UserUnit`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Misura {
    pub larghezza: f32,
    pub altezza: f32,
}

impl Pdf {
    /// Apre i byte di un PDF, con la password se c'è. Un errore o un panico del motore diventano
    /// `Err`, mai un'app che si chiude. Lavoro pesante (legge tutte le pagine, le firme, i campi):
    /// fuori dal thread dell'interfaccia; `Pdf` si può mandare a un altro thread. Il motore crea
    /// anche il suo pool di disegno, con i thread fermi finché nessuno lo usa (qui non si usa:
    /// si disegna con `disegnatore`).
    pub fn apri(byte: Vec<u8>, password: Option<&str>) -> Result<Pdf, ErroreApertura> {
        Self::apri_condiviso(Arc::new(byte), password)
    }

    /// Come [`apri`](Self::apri), coi byte condivisi: chi chiama li tiene, per riprovare con
    /// un'altra password senza rileggere il file. La password resta solo nella memoria del
    /// motore finché il PDF è aperto: mai in un log, nella diagnosi o in un file.
    pub fn apri_condiviso(byte: Arc<Vec<u8>>, password: Option<&str>) -> Result<Pdf, ErroreApertura> {
        if byte.is_empty() {
            return Err(Genere::Vuoto.into());
        }
        if byte.len() > LIMITE_MB * 1024 * 1024 {
            return Err(Genere::TroppoGrande.into());
        }
        let mut sessione = Session::new();
        // In questa tappa si legge soltanto: gli script dei moduli restano spenti (ricerca
        // PDFCraft, sezioni 5 e 15).
        sessione.set_javascript(false);
        // Il motore ferma già i suoi panici (`guard`); questo è per quelli fuori.
        let esito = catch_unwind(AssertUnwindSafe(|| sessione.prepare_open(NOME, None, byte, password)));
        let pronto = dal_motore(esito)?;
        let id = sessione.commit_open(pronto);
        Ok(Pdf { sessione, id })
    }

    /// Il motore ha dovuto riparare il file per aprirlo (un PDF scaricato a metà, rovinato): può
    /// mancare qualcosa, per esempio delle pagine, e va detto (analisi del 27/10, lettura I6). Il
    /// registro delle riparazioni no: può avere pezzi del file.
    pub fn riparato(&self) -> bool {
        !self.documento().repair_log().is_empty()
    }

    /// Il documento del motore, che resta aperto finché c'è il `Pdf`.
    pub(crate) fn documento(&self) -> &Document {
        self.sessione.get(self.id).expect("il documento resta aperto finché c'è il Pdf")
    }

    /// Quante pagine ha.
    pub fn pagine(&self) -> usize {
        self.documento().info.pages.len()
    }

    /// La misura della pagina `pagina` (da 0), se c'è.
    pub fn misura(&self, pagina: usize) -> Option<Misura> {
        self.documento().info.pages.get(pagina).map(|p| Misura { larghezza: p.width, altezza: p.height })
    }

    /// Le misure di tutte le pagine, in ordine.
    pub fn misure(&self) -> Vec<Misura> {
        self.documento().info.pages.iter().map(|p| Misura { larghezza: p.width, altezza: p.height }).collect()
    }

    /// I byte del file com'è stato aperto, senza copiarli: per condividerlo.
    pub fn byte(&self) -> Arc<Vec<u8>> {
        self.documento().bytes.clone()
    }
}

/// L'esito del motore, panico compreso, in un `ErroreApertura`.
fn dal_motore<T>(esito: std::thread::Result<Result<T, OpenError>>) -> Result<T, ErroreApertura> {
    let genere = match esito {
        Ok(Ok(v)) => return Ok(v),
        Ok(Err(OpenError::NeedsPassword)) => Genere::ChiedePassword,
        Ok(Err(OpenError::WrongPassword)) => Genere::PasswordSbagliata,
        Ok(Err(OpenError::Unsupported(_))) => Genere::NonSupportato,
        Ok(Err(OpenError::Invalid(_))) => Genere::Illeggibile,
        Err(_) => Genere::PanicoDelLettore,
    };
    Err(genere.into())
}

#[cfg(test)]
pub(crate) mod test {
    use super::*;
    use crate::prova::cifrato;
    use crate::prova::{Pagina, pdf};
    use std::sync::Arc;

    #[test]
    fn un_pdf_troncato_si_dice_riparato() {
        let intero = crate::prova::pdf(&[crate::prova::Pagina::bianca(595.0, 842.0); 3]);
        assert!(!Pdf::apri(intero.clone(), None).unwrap().riparato());
        assert!(Pdf::apri(intero[..intero.len() * 6 / 10].to_vec(), None).unwrap().riparato());
    }

    #[test]
    fn apre_e_conta_le_pagine() {
        let byte = pdf(&[Pagina::bianca(595.0, 842.0), Pagina::bianca(595.0, 842.0), Pagina::bianca(595.0, 842.0)]);
        let p = Pdf::apri(byte, None).unwrap();
        assert_eq!(p.pagine(), 3);
        // Il PDF di prova è scritto bene: il motore non ha dovuto ripararlo.
        assert!(p.documento().repair_log().is_empty());
    }

    #[test]
    fn misura_di_ogni_pagina_come_si_vede() {
        let girata = Pagina { larghezza: 200.0, altezza: 100.0, rotazione: 90, contenuto: "" };
        let byte = pdf(&[Pagina::bianca(595.0, 842.0), Pagina::bianca(612.0, 792.0), girata]);
        let p = Pdf::apri(byte, None).unwrap();
        assert_eq!(p.misura(0), Some(Misura { larghezza: 595.0, altezza: 842.0 }));
        assert_eq!(p.misura(1), Some(Misura { larghezza: 612.0, altezza: 792.0 }));
        // Girata di 90 gradi: si vede alta e stretta.
        assert_eq!(p.misura(2), Some(Misura { larghezza: 100.0, altezza: 200.0 }));
        assert_eq!(p.misura(3), None);
    }

    #[test]
    fn riconosce_l_intestazione() {
        assert!(sembra_pdf(&pdf(&[Pagina::bianca(10.0, 10.0)])));
        // Spazzatura prima dell'intestazione, entro i primi 1024 byte.
        let mut dopo = vec![b' '; 1000];
        dopo.extend_from_slice(b"%PDF-1.4\n");
        assert!(sembra_pdf(&dopo));
        let mut troppo_dopo = vec![b' '; 1024];
        troppo_dopo.extend_from_slice(b"%PDF-1.4\n");
        assert!(!sembra_pdf(&troppo_dopo));
        assert!(!sembra_pdf(b"PK\x03\x04 un docx"));
        assert!(!sembra_pdf(b"%PDF"));
        assert!(!sembra_pdf(&[]));
    }

    #[test]
    fn vuoto_e_spazzatura_danno_errore_senza_panico() {
        assert_eq!(Pdf::apri(Vec::new(), None).err().unwrap().genere, Genere::Vuoto);
        let e = Pdf::apri([0u8, 1, 2, 3, 255, 254].repeat(111), None).err().unwrap();
        assert_eq!(e.genere, Genere::Illeggibile);
        let e = Pdf::apri(b"%PDF-1.7\nniente altro".to_vec(), None).err().unwrap();
        assert_eq!(e.genere, Genere::Illeggibile);
    }

    #[test]
    fn oltre_il_tetto_non_arriva_al_motore() {
        // Zeri: la memoria arriva dal sistema già azzerata e non si tocca davvero.
        let e = Pdf::apri(vec![0u8; LIMITE_MB * 1024 * 1024 + 1], None).err().unwrap();
        assert_eq!(e.genere, Genere::TroppoGrande);
        assert!(e.to_string().contains("200 MB"), "{e}");
        assert_eq!(e.genere.diagnosi(), format!("oltre {LIMITE_MB} MB"));
    }

    #[test]
    fn password_chiesta_sbagliata_e_giusta() {
        let byte = cifrato("Segreta1");
        let e = Pdf::apri(byte.clone(), None).err().unwrap();
        assert_eq!(e.genere, Genere::ChiedePassword);
        assert!(e.chiede_password());
        assert_eq!(e.genere.diagnosi(), "protetto da password");
        let e = Pdf::apri(byte.clone(), Some("sbagliata")).err().unwrap();
        assert_eq!(e.genere, Genere::PasswordSbagliata);
        assert!(e.chiede_password());
        let p = Pdf::apri(byte, Some("Segreta1")).unwrap();
        assert_eq!(p.pagine(), 1);
        assert_eq!(p.misura(0), Some(Misura { larghezza: 595.0, altezza: 842.0 }));
        assert!(!ErroreApertura { genere: Genere::Illeggibile }.chiede_password());
    }

    #[test]
    fn errori_del_motore_e_panico() {
        use pdfcraft_render::OpenError;
        let genere = |esito: std::thread::Result<Result<(), OpenError>>| dal_motore(esito).err().unwrap().genere;
        assert_eq!(genere(Ok(Err(OpenError::NeedsPassword))), Genere::ChiedePassword);
        assert_eq!(genere(Ok(Err(OpenError::WrongPassword))), Genere::PasswordSbagliata);
        assert_eq!(genere(Ok(Err(OpenError::Invalid("Verbale Zanzibar.pdf".into())))), Genere::Illeggibile);
        assert_eq!(genere(Ok(Err(OpenError::Unsupported("Zanzibar".into())))), Genere::NonSupportato);
        let panico = std::panic::catch_unwind(|| -> Result<(), OpenError> { panic!("testo del documento: Zanzibar") });
        assert_eq!(genere(panico), Genere::PanicoDelLettore);
    }

    #[test]
    fn messaggi_in_italiano_senza_il_motore() {
        let tutti = [
            Genere::Vuoto,
            Genere::TroppoGrande,
            Genere::ChiedePassword,
            Genere::PasswordSbagliata,
            Genere::Illeggibile,
            Genere::NonSupportato,
            Genere::PanicoDelLettore,
        ];
        let mut visti = std::collections::HashSet::new();
        for g in tutti {
            let m = ErroreApertura { genere: g }.to_string();
            // Comincia in minuscolo (segue «Non riesco ad aprire il file: ») e finisce col punto.
            assert!(m.starts_with(|c: char| c.is_lowercase()) && m.ends_with('.'), "{m}");
            assert!(!m.contains('\u{2013}') && !m.contains('\u{2014}'), "{m}");
            assert!(visti.insert(g.diagnosi()), "genere doppio: {}", g.diagnosi());
        }
    }

    #[test]
    fn misure_e_byte_del_file() {
        let byte = pdf(&[Pagina::bianca(595.0, 842.0), Pagina::bianca(612.0, 792.0)]);
        let p = Pdf::apri(byte.clone(), None).unwrap();
        assert_eq!(p.misure(), vec![Misura { larghezza: 595.0, altezza: 842.0 }, Misura { larghezza: 612.0, altezza: 792.0 }]);
        assert_eq!(*p.byte(), byte);
    }

    #[test]
    fn coi_byte_condivisi_si_riprova_la_password_senza_rileggere() {
        let byte = Arc::new(cifrato("Segreta1"));
        let e = Pdf::apri_condiviso(byte.clone(), Some("segreta1")).err().unwrap();
        assert_eq!(e.genere, Genere::PasswordSbagliata);
        // I byte restano a chi chiama, interi: il secondo tentativo li riusa.
        let p = Pdf::apri_condiviso(byte.clone(), Some("Segreta1")).unwrap();
        assert_eq!(p.pagine(), 1);
        assert!(Arc::ptr_eq(&p.byte(), &byte));
        assert_eq!(Pdf::apri_condiviso(Arc::new(Vec::new()), None).err().unwrap().genere, Genere::Vuoto);
    }

    #[test]
    fn si_manda_a_un_altro_thread() {
        fn manda<T: Send>() {}
        manda::<Pdf>();
        let p = std::thread::spawn(|| Pdf::apri(pdf(&[Pagina::bianca(100.0, 100.0)]), None).unwrap()).join().unwrap();
        assert_eq!(p.pagine(), 1);
    }
}
