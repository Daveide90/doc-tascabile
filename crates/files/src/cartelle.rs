//! Le cartelle del telefono: che cosa c'è dentro, e le scorciatoie della Home.

use std::cmp::Ordering;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::iter::Peekable;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::{Tipo, tipo_da_nome};

/// Una riga del navigatore: una cartella o un file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Voce {
    /// Il nome da mostrare (se non è UTF-8 valido, con i caratteri sostitutivi).
    pub nome: String,
    /// Il percorso vero, da aprire o da visitare.
    pub percorso: PathBuf,
    pub cartella: bool,
    /// Sempre `Tipo::Altro` per le cartelle, anche se il nome finisce con «.docx».
    pub tipo: Tipo,
    pub modificato: Option<SystemTime>,
    /// La misura del file; 0 per le cartelle.
    pub byte: u64,
}

/// Che cosa c'è in `cartella`: prima le cartelle, poi i file, in ordine di nome senza maiuscole
/// né accenti e coi numeri per valore (`confronta_nomi`). Niente nascosti (il nome comincia col
/// punto) e niente collegamenti rotti. Un errore nel leggere la cartella, anche a metà, torna a
/// chi chiama: mai un elenco a metà. Legge il disco: va chiamata quando si entra in una
/// cartella, non a ogni frame.
pub fn elenca(cartella: &Path) -> io::Result<Vec<Voce>> {
    raccogli(fs::read_dir(cartella)?.map(|voce| voce.map(|v| (v.file_name(), v.path()))))
}

/// Tutti i file di `cartella` e delle sue sottocartelle (fino a sei livelli, senza le nascoste),
/// i più recenti prima, al più `massimo`: la vista «Tutti i file» di una cartella e la sua
/// ricerca. Una sottocartella che non si legge si salta. I collegamenti a cartelle non si seguono:
/// un giro tornerebbe su se stesso.
pub fn elenca_tutto(cartella: &Path, massimo: usize) -> Vec<Voce> {
    let mut file = Vec::new();
    let mut da_vedere = vec![(cartella.to_path_buf(), 0usize)];
    while let Some((c, livello)) = da_vedere.pop() {
        let Ok(voci) = elenca(&c) else { continue };
        for v in voci {
            if v.cartella {
                let collegamento = fs::symlink_metadata(&v.percorso).is_ok_and(|m| m.file_type().is_symlink());
                if livello < 6 && !collegamento {
                    da_vedere.push((v.percorso, livello + 1));
                }
            } else if file.len() < massimo {
                file.push(v);
            }
        }
        if file.len() >= massimo {
            break;
        }
    }
    file.sort_by(|a, b| b.modificato.cmp(&a.modificato).then_with(|| confronta_nomi(&a.nome, &b.nome, true)));
    file
}

/// Il lavoro di `elenca` su quello che dà `read_dir`: nome e percorso di ogni voce, oppure un
/// errore della lettura. Sta a parte perché in una prova quell'errore non si ottiene da un disco
/// vero.
fn raccogli(lette: impl IntoIterator<Item = io::Result<(OsString, PathBuf)>>) -> io::Result<Vec<Voce>> {
    let mut voci = Vec::new();
    for voce in lette {
        // L'errore della lettura è della cartella, non di una voce: dopo, read_dir non dà più
        // niente, e un elenco a metà sembrerebbe intero.
        let (nome, percorso) = voce?;
        let nome = nome.to_string_lossy().into_owned();
        if nome.starts_with('.') {
            continue;
        }
        // Un collegamento rotto, o una voce di cui non si leggono le informazioni, non si apre e
        // non si visita.
        let Ok(info) = fs::metadata(&percorso) else { continue };
        let cartella = info.is_dir();
        voci.push(Voce {
            tipo: if cartella { Tipo::Altro } else { tipo_da_nome(&nome) },
            modificato: info.modified().ok(),
            byte: if cartella { 0 } else { info.len() },
            nome,
            percorso,
            cartella,
        });
    }
    voci.sort_by(|a, b| b.cartella.cmp(&a.cartella).then_with(|| confronta_nomi(&a.nome, &b.nome, !a.cartella)));
    Ok(voci)
}

/// L'ordine dei nomi nel navigatore: senza maiuscole né accenti, con i numeri per valore
/// («Verbale (2)» prima di «Verbale (10)») e, per i file, prima il nome e poi l'estensione
/// («Verbale.docx» prima di «Verbale (2).docx»). A parità, i caratteri così come sono, così due
/// nomi diversi non sono mai pari e l'ordine è sempre lo stesso.
fn confronta_nomi(a: &str, b: &str, file: bool) -> Ordering {
    let (base_a, estensione_a) = if file { dividi(a) } else { (a, "") };
    let (base_b, estensione_b) = if file { dividi(b) } else { (b, "") };
    naturale(base_a, base_b).then_with(|| naturale(estensione_a, estensione_b)).then_with(|| a.cmp(b))
}

/// Nome ed estensione: «.docx» e «Verbale» non hanno estensione.
fn dividi(nome: &str) -> (&str, &str) {
    match nome.rsplit_once('.') {
        Some((base, estensione)) if !base.is_empty() => (base, estensione),
        _ => (nome, ""),
    }
}

/// Confronto senza maiuscole né accenti, con le cifre di seguito lette come un solo numero.
fn naturale(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (piegati(a), piegati(b));
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let (n, m) = (numero(&mut x), numero(&mut y));
                // Senza zeri davanti, più cifre vuol dire numero più grande; a pari cifre conta
                // l'ordine delle cifre. Va bene anche per numeri più lunghi di un u64.
                let o = n.len().cmp(&m.len()).then_with(|| n.cmp(&m));
                if o.is_ne() {
                    return o;
                }
            }
            (Some(c), Some(d)) if c != d => return c.cmp(&d),
            _ => {
                x.next();
                y.next();
            }
        }
    }
}

/// I caratteri di `testo` in minuscolo e senza accenti.
fn piegati(testo: &str) -> Peekable<impl Iterator<Item = char> + '_> {
    testo.chars().flat_map(char::to_lowercase).map(senza_accento).peekable()
}

/// Le cifre che seguono, senza gli zeri davanti.
fn numero(cifre: &mut Peekable<impl Iterator<Item = char>>) -> String {
    let mut n = String::new();
    while let Some(c) = cifre.next_if(char::is_ascii_digit) {
        if !(n.is_empty() && c == '0') {
            n.push(c);
        }
    }
    n
}

/// La lettera senza accento, per le lettere minuscole dell'italiano e delle lingue vicine.
fn senza_accento(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'ç' => 'c',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ý' | 'ÿ' => 'y',
        _ => c,
    }
}

/// Il nome contiene tutte le parole di `cercato`, in qualunque ordine, senza guardare maiuscole e
/// accenti (come l'ordine dei nomi). Senza parole, ogni nome va bene.
pub fn nome_contiene(nome: &str, cercato: &str) -> bool {
    let nome: String = piegati(nome).collect();
    cercato.split_whitespace().all(|parola| nome.contains(&piegati(parola).collect::<String>()))
}

/// Una cartella da raggiungere con un tocco dalla Home.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scorciatoia {
    /// Il nome da mostrare, in italiano.
    pub nome: String,
    pub percorso: PathBuf,
}

/// Le scorciatoie che esistono sotto `radice` (la memoria condivisa del telefono), in
/// quest'ordine: Download, Documenti, Documenti/Tascabile, allegati di WhatsApp, Telegram.
/// Legge il disco: va chiamata quando si apre la Home, non a ogni frame.
pub fn scorciatoie(radice: &Path) -> Vec<Scorciatoia> {
    // La cartella di WhatsApp senza le sue due dentro, solo se il WhatsApp del telefono non le ha.
    let divisa = radice.join(WHATSAPP).join("Private").is_dir();
    SCORCIATOIE
        .iter()
        .filter(|(nome, _)| !(divisa && *nome == "WhatsApp"))
        .map(|(nome, dove)| Scorciatoia { nome: (*nome).to_owned(), percorso: radice.join(dove) })
        .filter(|s| s.percorso.is_dir())
        .collect()
}

/// I documenti di WhatsApp, in Android/media da quando c'è la memoria a compartimenti.
const WHATSAPP: &str = "Android/media/com.whatsapp/WhatsApp/Media/WhatsApp Documents";

/// Nome da mostrare e percorso dalla radice. Sul telefono le cartelle comuni hanno il nome
/// inglese (`Environment.DIRECTORY_DOCUMENTS` è «Documents», `DIRECTORY_DOWNLOADS` «Download»).
/// WhatsApp oggi tiene i documenti ricevuti in «Private» e quelli mandati in «Sent», e nella
/// cartella sopra niente: la Home diceva «WhatsApp · Nessun file» (segnalazione dell'autore del 3/11).
const SCORCIATOIE: &[(&str, &str)] = &[
    ("Download", "Download"),
    ("Documenti", "Documents"),
    ("Documenti/Tascabile", "Documents/Tascabile"),
    ("WhatsApp ricevuti", "Android/media/com.whatsapp/WhatsApp/Media/WhatsApp Documents/Private"),
    ("WhatsApp mandati", "Android/media/com.whatsapp/WhatsApp/Media/WhatsApp Documents/Sent"),
    ("WhatsApp", WHATSAPP),
    ("Telegram", "Download/Telegram"),
];

/// Il file che si chiama `nome` nelle cartelle delle scorciatoie (il più recente, se sono più
/// d'uno): un documento arrivato con «Apri con» da un'altra app, quando il permesso di leggerlo
/// non c'è più e i suoi byte non si possono confrontare.
pub fn per_nome(radice: &Path, nome: &str) -> Option<PathBuf> {
    if nome.is_empty() || Path::new(nome).file_name().is_none_or(|f| f != nome) {
        return None;
    }
    SCORCIATOIE
        .iter()
        .map(|(_, dove)| radice.join(dove).join(nome))
        .filter_map(|p| fs::metadata(&p).ok().filter(|m| m.is_file()).map(|m| (m.modified().ok(), p)))
        .max_by_key(|(quando, _)| *quando)
        .map(|(_, p)| p)
}

/// Il file del telefono che ha questi `byte`, in una delle cartelle delle scorciatoie (prima uno
/// che si chiama `nome`, poi gli altri con la stessa misura). Un documento passato con «Apri
/// con» da WhatsApp, Telegram o da Download è quasi sempre lì: aperto da lì resta nei recenti
/// anche quando il permesso di «Apri con» se ne va (dopo un aggiornamento dell'app: «Non si apre
/// più: riaprilo da WhatsApp», segnalazione del 3/11) e Salva scrive nel file vero. Solo un file
/// con gli stessi byte, mai uno che gli somiglia.
pub fn lo_stesso_file(radice: &Path, nome: &str, byte: &[u8]) -> Option<PathBuf> {
    let cartelle: Vec<PathBuf> = SCORCIATOIE.iter().map(|(_, dove)| radice.join(dove)).filter(|c| c.is_dir()).collect();
    let uguale = |p: &Path| fs::metadata(p).is_ok_and(|m| m.is_file() && m.len() == byte.len() as u64) && fs::read(p).is_ok_and(|b| b == byte);
    // Un nome con dentro una cartella non è un nome di file.
    let semplice = !nome.is_empty() && Path::new(nome).file_name().is_some_and(|f| f == nome);
    if semplice
        && let Some(p) = cartelle.iter().map(|c| c.join(nome)).find(|p| uguale(p))
    {
        return Some(p);
    }
    for c in &cartelle {
        let Ok(voci) = fs::read_dir(c) else { continue };
        for v in voci.flatten() {
            let p = v.path();
            if v.metadata().is_ok_and(|m| m.is_file() && m.len() == byte.len() as u64) && uguale(&p) {
                return Some(p);
            }
        }
    }
    None
}

/// Dove vanno i documenti nuovi e le copie: Documenti/Tascabile, cioè «Documents/Tascabile» sul
/// disco. Non la crea: la crea chi ci salva per primo.
pub fn cartella_tascabile(radice: &Path) -> PathBuf {
    radice.join("Documents").join("Tascabile")
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::prova::cartella_di_prova;

    fn nomi(voci: &[Voce]) -> Vec<&str> {
        voci.iter().map(|v| v.nome.as_str()).collect()
    }

    /// Crea dei file da un byte nella cartella.
    fn file(cartella: &Path, nomi: &[&str]) {
        for nome in nomi {
            fs::write(cartella.join(nome), b"x").unwrap();
        }
    }

    #[test]
    fn cartelle_prima_poi_file_senza_maiuscole() {
        let d = cartella_di_prova("elenca-ordine");
        fs::create_dir(d.join("zeta")).unwrap();
        fs::create_dir(d.join("Alfa")).unwrap();
        file(&d, &["beta.docx", "Bravo.pdf", "alfa.txt"]);
        assert_eq!(nomi(&elenca(&d).unwrap()), ["Alfa", "zeta", "alfa.txt", "beta.docx", "Bravo.pdf"]);
    }

    #[test]
    fn niente_nascosti() {
        let d = cartella_di_prova("elenca-nascosti");
        fs::create_dir(d.join(".thumbnails")).unwrap();
        file(&d, &[".nomedia", ".trashed-1729700000-Verbale.docx", "Verbale.docx"]);
        assert_eq!(nomi(&elenca(&d).unwrap()), ["Verbale.docx"]);
    }

    #[test]
    fn voce_con_tipo_misura_e_data() {
        let d = cartella_di_prova("elenca-voce");
        // Una cartella col nome di un documento resta una cartella.
        fs::create_dir(d.join("Lettere.docx")).unwrap();
        fs::write(d.join("Verbale.docx"), vec![0u8; 1234]).unwrap();
        let voci = elenca(&d).unwrap();
        let cartella = &voci[0];
        assert_eq!((cartella.nome.as_str(), cartella.cartella, cartella.tipo, cartella.byte), ("Lettere.docx", true, Tipo::Altro, 0));
        assert_eq!(cartella.percorso, d.join("Lettere.docx"));
        assert!(cartella.modificato.is_some());
        let documento = &voci[1];
        assert_eq!((documento.nome.as_str(), documento.cartella, documento.tipo, documento.byte), ("Verbale.docx", false, Tipo::Documento, 1234));
        assert_eq!(documento.percorso, d.join("Verbale.docx"));
        assert_eq!(documento.modificato, fs::metadata(d.join("Verbale.docx")).unwrap().modified().ok());
        assert_eq!(voci.len(), 2);
    }

    #[test]
    fn numeri_in_ordine_di_valore() {
        let d = cartella_di_prova("elenca-numeri");
        file(&d, &["Verbale (10).docx", "Verbale (2).docx", "Verbale.docx", "Verbale (3).pdf", "Capitolo 10.docx", "capitolo 9.docx"]);
        assert_eq!(
            nomi(&elenca(&d).unwrap()),
            ["capitolo 9.docx", "Capitolo 10.docx", "Verbale.docx", "Verbale (2).docx", "Verbale (3).pdf", "Verbale (10).docx"]
        );
    }

    #[test]
    fn lettere_accentate_insieme_a_quelle_senza() {
        let d = cartella_di_prova("elenca-accenti");
        file(&d, &["Zucca.docx", "Èlite.docx", "Esame.docx", "Cittadino.docx", "Città.docx", "Ciao.docx"]);
        assert_eq!(nomi(&elenca(&d).unwrap()), ["Ciao.docx", "Città.docx", "Cittadino.docx", "Èlite.docx", "Esame.docx", "Zucca.docx"]);
    }

    #[test]
    fn stesso_nome_stessa_estensione_prima() {
        let d = cartella_di_prova("elenca-estensioni");
        file(&d, &["Verbale.pdf", "Verbale.docx", "Verbale 2026.docx"]);
        assert_eq!(nomi(&elenca(&d).unwrap()), ["Verbale.docx", "Verbale.pdf", "Verbale 2026.docx"]);
    }

    /// I nomi in ordine, come li mette `confronta_nomi` (tutti file, o tutte cartelle).
    fn in_ordine<'a>(nomi: &[&'a str], file: bool) -> Vec<&'a str> {
        let mut v = nomi.to_vec();
        v.sort_by(|a, b| confronta_nomi(a, b, file));
        v
    }

    #[test]
    fn zeri_davanti_e_numeri_lunghissimi() {
        assert_eq!(
            in_ordine(&["Capitolo 8.docx", "Capitolo 7.docx", "Capitolo 007.docx"], true),
            ["Capitolo 007.docx", "Capitolo 7.docx", "Capitolo 8.docx"]
        );
        assert_eq!(
            in_ordine(&["Foto 123456789012345678901234567890.jpg", "Foto 99.jpg"], true),
            ["Foto 99.jpg", "Foto 123456789012345678901234567890.jpg"]
        );
        assert_eq!(in_ordine(&["0.txt", "00.txt", "1.txt"], true), ["0.txt", "00.txt", "1.txt"]);
    }

    #[test]
    fn nomi_diversi_mai_pari() {
        assert_eq!(confronta_nomi("verbale.docx", "Verbale.docx", true), Ordering::Greater);
        assert_eq!(confronta_nomi("Città", "Citta", false), Ordering::Greater);
        assert_eq!(confronta_nomi("Verbale.docx", "Verbale.docx", true), Ordering::Equal);
    }

    #[test]
    fn le_cartelle_non_hanno_estensione() {
        // Per una cartella il punto è un carattere come gli altri.
        assert_eq!(in_ordine(&["Rel.10", "Rel 2", "Rel.9"], false), ["Rel 2", "Rel.9", "Rel.10"]);
    }

    #[cfg(unix)]
    #[test]
    fn collegamenti_seguiti_e_rotti_saltati() {
        let d = cartella_di_prova("elenca-collegamenti");
        fs::create_dir(d.join("vera")).unwrap();
        std::os::unix::fs::symlink(d.join("vera"), d.join("verso la vera")).unwrap();
        std::os::unix::fs::symlink(d.join("manca.docx"), d.join("rotto.docx")).unwrap();
        let voci = elenca(&d).unwrap();
        assert_eq!(nomi(&voci), ["vera", "verso la vera"]);
        assert!(voci[1].cartella);
    }

    #[test]
    fn cartella_che_non_c_e_o_che_e_un_file() {
        let d = cartella_di_prova("elenca-errori");
        assert_eq!(elenca(&d.join("manca")).unwrap_err().kind(), io::ErrorKind::NotFound);
        file(&d, &["Verbale.docx"]);
        assert!(elenca(&d.join("Verbale.docx")).is_err());
    }

    #[test]
    fn errore_di_lettura_a_meta_elenco() {
        // Su Linux e su Android, dopo un errore read_dir non dà più niente: saltarlo darebbe un
        // elenco a metà che sembra intero. Qui la memoria condivisa, che sul telefono passa da
        // FUSE, smette di rispondere dopo la prima voce.
        let d = cartella_di_prova("elenca-errore-a-meta");
        file(&d, &["Verbale 01.docx", "Verbale 02.docx"]);
        let lette = [Ok((OsString::from("Verbale 01.docx"), d.join("Verbale 01.docx"))), Err(io::Error::from(io::ErrorKind::NotConnected))];
        assert_eq!(raccogli(lette).unwrap_err().kind(), io::ErrorKind::NotConnected);
    }

    #[test]
    fn scorciatoie_solo_quelle_che_ci_sono() {
        let r = cartella_di_prova("scorciatoie");
        let whatsapp = "Android/media/com.whatsapp/WhatsApp/Media/WhatsApp Documents";
        for c in ["Download", "Documents/Tascabile", whatsapp] {
            fs::create_dir_all(r.join(c)).unwrap();
        }
        let scorciatoia = |nome: &str, dove: &str| Scorciatoia { nome: nome.into(), percorso: r.join(dove) };
        assert_eq!(
            scorciatoie(&r),
            [
                scorciatoia("Download", "Download"),
                scorciatoia("Documenti", "Documents"),
                scorciatoia("Documenti/Tascabile", "Documents/Tascabile"),
                scorciatoia("WhatsApp", whatsapp),
            ]
        );
        assert_eq!(scorciatoie(&r)[2].percorso, cartella_tascabile(&r));
        fs::create_dir(r.join("Download/Telegram")).unwrap();
        assert_eq!(scorciatoie(&r).last(), Some(&scorciatoia("Telegram", "Download/Telegram")));
        assert_eq!(scorciatoie(&r).len(), 5);
        // Il WhatsApp di oggi: i ricevuti in Private e i mandati in Sent, sopra niente. Le sue
        // due cartelle al posto di quella sopra, che diceva «Nessun file».
        fs::create_dir(r.join(whatsapp).join("Private")).unwrap();
        fs::create_dir(r.join(whatsapp).join("Sent")).unwrap();
        let nomi: Vec<String> = scorciatoie(&r).into_iter().map(|s| s.nome).collect();
        assert_eq!(nomi, ["Download", "Documenti", "Documenti/Tascabile", "WhatsApp ricevuti", "WhatsApp mandati", "Telegram"]);
        assert_eq!(scorciatoie(&r)[3].percorso, r.join(whatsapp).join("Private"));
    }

    #[test]
    fn tutti_i_file_anche_delle_sottocartelle() {
        let d = cartella_di_prova("elenca-tutto");
        fs::create_dir_all(d.join("Lavoro/Turni")).unwrap();
        fs::create_dir_all(d.join(".nascosta")).unwrap();
        file(&d, &["Verbale.docx"]);
        file(&d.join("Lavoro"), &["Relazione.pdf"]);
        file(&d.join("Lavoro/Turni"), &["Novembre.xlsx"]);
        file(&d.join(".nascosta"), &["Segreto.pdf"]);
        let mut nomi: Vec<String> = elenca_tutto(&d, 100).into_iter().map(|v| v.nome).collect();
        nomi.sort();
        assert_eq!(nomi, ["Novembre.xlsx", "Relazione.pdf", "Verbale.docx"]);
        assert_eq!(elenca_tutto(&d, 2).len(), 2);
        assert!(elenca_tutto(&d.join("non c'è"), 10).is_empty());
    }

    #[test]
    fn un_file_arrivato_con_apri_con_si_ritrova_sul_telefono() {
        let r = cartella_di_prova("lo-stesso-file");
        let ricevuti = r.join("Android/media/com.whatsapp/WhatsApp/Media/WhatsApp Documents/Private");
        fs::create_dir_all(&ricevuti).unwrap();
        fs::create_dir_all(r.join("Download")).unwrap();
        fs::write(ricevuti.join("Referto.pdf"), b"%PDF-1.7 il referto").unwrap();
        // Lo stesso nome con un altro contenuto (un'altra versione): non è lui.
        fs::write(r.join("Download/Referto.pdf"), b"%PDF-1.7 un altro").unwrap();
        assert_eq!(lo_stesso_file(&r, "Referto.pdf", b"%PDF-1.7 il referto"), Some(ricevuti.join("Referto.pdf")));
        // WhatsApp a volte lo chiama in un altro modo: si ritrova dai byte.
        assert_eq!(lo_stesso_file(&r, "Referto (1).pdf", b"%PDF-1.7 il referto"), Some(ricevuti.join("Referto.pdf")));
        // Niente con quei byte; un nome che è un percorso non esce dalle cartelle.
        assert_eq!(lo_stesso_file(&r, "Referto.pdf", b"%PDF-1.7 mai visto"), None);
        fs::write(r.join("segreto.pdf"), b"%PDF-1.7 fuori").unwrap();
        assert_eq!(lo_stesso_file(&r, "../../../segreto.pdf", b"%PDF-1.7 fuori"), None);
        // Senza i byte, per nome: il più recente.
        assert!(per_nome(&r, "Referto.pdf").is_some());
        assert_eq!(per_nome(&r, "Mai visto.pdf"), None);
        assert_eq!(per_nome(&r, "../segreto.pdf"), None);
    }

    #[test]
    fn la_ricerca_trova_le_parole_senza_maiuscole_ne_accenti() {
        let nome = "Relazione del Turno di notte, perché città.docx";
        for cercato in ["turno", "RELAZIONE", "  notte ", "perche", "CITTA", "turno relazione", "", "   ", ".docx"] {
            assert!(nome_contiene(nome, cercato), "{cercato:?}");
        }
        for cercato in ["mattino", "turno mattino", "relazioni"] {
            assert!(!nome_contiene(nome, cercato), "{cercato:?}");
        }
        // Le cifre sono cifre: «Verbale 12» non è «Verbale 1».
        assert!(nome_contiene("Verbale 12.docx", "12") && !nome_contiene("Verbale 1.docx", "12"));
    }

    #[test]
    fn scorciatoie_senza_cartelle() {
        let r = cartella_di_prova("scorciatoie-vuote");
        assert!(scorciatoie(&r).is_empty());
        assert!(scorciatoie(&r.join("manca")).is_empty());
        // Un file al posto della cartella non è una scorciatoia.
        fs::write(r.join("Download"), b"x").unwrap();
        assert!(scorciatoie(&r).is_empty());
    }

    #[test]
    fn tascabile_sta_in_documents() {
        // Sul telefono le cartelle comuni hanno il nome inglese: «Documenti» è Documents.
        assert_eq!(cartella_tascabile(Path::new("/storage/emulated/0")), Path::new("/storage/emulated/0/Documents/Tascabile"));
    }
}
