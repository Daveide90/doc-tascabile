//! Nomi per i file nuovi: liberi nella cartella, e ricavati da una riga di testo.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use unicode_segmentation::UnicodeSegmentation;

/// Quanti caratteri al massimo ha un nome ricavato dal testo, estensione esclusa.
pub const MASSIMO_CARATTERI_NOME: usize = 40;

/// Il nome quando dal testo non resta niente.
pub const NOME_DI_RIPIEGO: &str = "Documento";

/// Il nome di file più lungo che ext4 e f2fs accettano, in byte.
const MASSIMO_BYTE_NOME: usize = 255;

/// La barra separa le cartelle; gli altri non li accettano Windows e le schede di memoria FAT.
const VIETATI: [char; 9] = ['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

/// Dove finisce una riga: a capo (anche quello del motore, `LINE_BREAK`), ritorno, tabulazione
/// verticale, salto pagina e interruzione di colonna (`PAGE_BREAK` e `COLUMN_BREAK` del motore) e
/// i separatori di Unicode.
const FINE_RIGA: [char; 8] = ['\n', '\r', '\u{000B}', '\u{000C}', '\u{000E}', '\u{0085}', '\u{2028}', '\u{2029}'];

/// Un file che in `cartella` non c'è ancora: «Verbale.docx», poi «Verbale (2).docx», «Verbale
/// (3).docx»... `estensione` va bene col punto o senza. `base` passa dalla pulizia di
/// `nome_pulito`, senza il taglio a 40 caratteri: niente cartelle dentro il nome, niente
/// caratteri vietati, mai un file nascosto, mai oltre i 255 byte. Occupato vuol dire che c'è
/// qualcosa, anche una cartella o un collegamento rotto, oppure che il controllo del nome non è
/// riuscito (`occupato`). Se nemmeno «Verbale (10000).docx» risulta libero si ferma lì e dà
/// quello, che libero non è: succede in una cartella che non si attraversa, dove il salvataggio
/// fallirà, o con diecimila file dallo stesso nome. Dice solo che il nome è libero adesso: fra il
/// controllo e il salvataggio un altro può prenderlo, e questi casi li chiude solo un salvataggio
/// che non sostituisce.
pub fn nome_libero(cartella: &Path, base: &str, estensione: &str) -> PathBuf {
    cerca_nome_libero(cartella, base, estensione, |percorso| fs::symlink_metadata(percorso).map(drop))
}

/// Il lavoro di `nome_libero` con `guarda` al posto di lstat: dice che cosa risponde il sistema
/// per un nome. Sta a parte perché in una prova un errore passeggero del disco non si ottiene da
/// un disco vero.
fn cerca_nome_libero(cartella: &Path, base: &str, estensione: &str, mut guarda: impl FnMut(&Path) -> io::Result<()>) -> PathBuf {
    let base = ripulita(base);
    let base = if base.is_empty() { NOME_DI_RIPIEGO } else { base.as_str() };
    let estensione = estensione.trim_start_matches('.');
    let candidato = |numero: Option<u32>| {
        let mut coda = numero.map(|n| format!(" ({n})")).unwrap_or_default();
        if !estensione.is_empty() {
            coda.push('.');
            coda.push_str(estensione);
        }
        let base = accorcia_a_byte(base, MASSIMO_BYTE_NOME.saturating_sub(coda.len()));
        cartella.join(format!("{base}{coda}"))
    };
    let mut percorso = candidato(None);
    let mut numero = 2;
    while occupato(guarda(&percorso)) && numero <= ULTIMO_NUMERO {
        percorso = candidato(Some(numero));
        numero += 1;
    }
    percorso
}

/// Il numero più alto che `nome_libero` prova: «Verbale (10000).docx». Senza un tetto, in una
/// cartella dove nessun nome si riesce a controllare la ricerca non finirebbe mai.
const ULTIMO_NUMERO: u32 = 10_000;

/// Il nome non si può usare: c'è già qualcosa (un file, una cartella o un collegamento, anche
/// rotto) oppure il controllo non è riuscito. Libero è solo un nome che non c'è: «non c'è»
/// (NotFound) o una cartella che manca o è un file (NotADirectory), dove l'errore lo dirà il
/// salvataggio. Ogni altro errore, anche passeggero (EIO dal disco o da FUSE, una cartella che
/// non si attraversa), non dice che il nome è libero: il salvataggio del motore guarda di nuovo,
/// e un file che a quel punto trova lo sostituisce.
fn occupato(esito: io::Result<()>) -> bool {
    match esito {
        Ok(()) => true,
        Err(e) => !matches!(e.kind(), io::ErrorKind::NotFound | io::ErrorKind::NotADirectory),
    }
}

/// Da una riga di testo (il primo titolo, il primo paragrafo) a un nome di file: la prima riga
/// che dice qualcosa, cioè con almeno una lettera o una cifra (una riga di trattini o di sole
/// emoji no), senza / \ : * ? " < > | né caratteri di controllo o invisibili, senza punti e
/// spazi ai bordi, al massimo 40 caratteri (tagliati dove finisce una parola, se si può, e mai a
/// metà di un carattere composto); mai vuoto: «Documento».
///
/// Il testo va passato come si legge, altrimenti nel nome del file finisce testo che il
/// documento ha tolto o nasconde, come un nome cancellato con le revisioni da un documento
/// anonimizzato. Dal motore, quindi, mai `Paragraph::text`, `plain_text()` o `shown_text()`
/// (nemmeno quelli di `Document`): tengono il testo cancellato con le revisioni (il lettore
/// .docx legge `w:delText` come `w:t` e segna solo la run, con `del`) e quello nascosto; e
/// `final_text()` toglie le cancellazioni ma tiene il testo nascosto. Per un paragrafo il testo
/// giusto è `Paragraph::text_without` con gli intervalli delle run che `doc.styles.resolve_char`
/// (stile del paragrafo, run) dà con `del` o con `hidden`, anche quando viene da uno stile:
/// quelle che la vista senza revisioni lascia fuori. Lo mostrano le prove dei nomi in
/// tascabile-word, in `documento.rs`.
pub fn nome_pulito(testo: &str) -> String {
    let riga = testo.split(FINE_RIGA).map(ripulita).find(|r| r.chars().any(char::is_alphanumeric)).unwrap_or_default();
    let nome = accorcia(&riga, MASSIMO_CARATTERI_NOME).trim_end_matches(['.', ' ']);
    if nome.is_empty() { NOME_DI_RIPIEGO.to_owned() } else { nome.to_owned() }
}

/// `testo` senza caratteri vietati, di controllo o invisibili, con gli spazi compressi e senza
/// punti né spazi ai bordi (un punto davanti farebbe un file nascosto). Può restare vuoto.
fn ripulita(testo: &str) -> String {
    let mut nome = String::with_capacity(testo.len());
    let mut spazio = false;
    for c in testo.chars() {
        let c = match c {
            // Il trattino che non va a capo (`NB_HYPHEN` del motore) diventa quello normale.
            '\u{2011}' => '-',
            c if VIETATI.contains(&c) || c.is_control() || c.is_whitespace() => ' ',
            c => c,
        };
        if invisibile(c) {
            continue;
        }
        if c == ' ' {
            spazio = true;
            continue;
        }
        if spazio && !nome.is_empty() {
            nome.push(' ');
        }
        spazio = false;
        nome.push(c);
    }
    nome.trim_matches(['.', ' ']).to_owned()
}

/// Caratteri che non si vedono: il segnaposto di un oggetto dentro al testo (`OBJ` del motore),
/// il trattino facoltativo, gli spazi larghi zero, i segni di direzione del testo e il BOM.
fn invisibile(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}' | '\u{200B}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}' | '\u{FFFC}'
    )
}

/// I primi `massimo` caratteri di `nome` senza spezzare un carattere composto
/// (`fine_di_grafema`), tagliati dove finisce una parola se così ne restano almeno la metà.
fn accorcia(nome: &str, massimo: usize) -> &str {
    let Some((taglio, _)) = nome.char_indices().nth(massimo) else {
        return nome;
    };
    let taglio = fine_di_grafema(nome, taglio);
    let dentro = &nome[..taglio];
    if nome[taglio..].starts_with(' ') {
        return dentro;
    }
    match dentro.rfind(' ') {
        Some(spazio) if dentro[..spazio].chars().count() >= massimo / 2 => &dentro[..spazio],
        _ => dentro,
    }
}

/// `base` in non più di `byte` byte, senza spezzare un carattere, nemmeno composto
/// (`fine_di_grafema`), e senza punti né spazi in fondo.
fn accorcia_a_byte(base: &str, byte: usize) -> &str {
    let mut fine = byte.min(base.len());
    while !base.is_char_boundary(fine) {
        fine -= 1;
    }
    base[..fine_di_grafema(base, fine)].trim_end_matches(['.', ' '])
}

/// `fine`, un confine fra due caratteri di `testo`, portata indietro dove finisce un carattere
/// come lo si vede (un grafema): mai a metà di un'emoji fatta di più caratteri, di una bandiera o
/// di una lettera con l'accento scritto a parte. Resta dov'è solo se già il primo grafema va
/// oltre: meglio un pezzo che un nome vuoto.
fn fine_di_grafema(testo: &str, fine: usize) -> usize {
    if fine >= testo.len() {
        return testo.len();
    }
    match testo.grapheme_indices(true).map(|(inizio, _)| inizio).take_while(|&inizio| inizio <= fine).last() {
        Some(inizio) if inizio > 0 => inizio,
        _ => fine,
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::prova::cartella_di_prova;

    #[test]
    fn il_primo_nome_libero() {
        let d = cartella_di_prova("nome-libero");
        assert_eq!(nome_libero(&d, "Verbale", "docx"), d.join("Verbale.docx"));
        fs::write(d.join("Verbale.docx"), b"x").unwrap();
        assert_eq!(nome_libero(&d, "Verbale", "docx"), d.join("Verbale (2).docx"));
        fs::write(d.join("Verbale (2).docx"), b"x").unwrap();
        assert_eq!(nome_libero(&d, "Verbale", "docx"), d.join("Verbale (3).docx"));
        // Con un'altra estensione è un altro nome.
        assert_eq!(nome_libero(&d, "Verbale", "pdf"), d.join("Verbale.pdf"));
    }

    #[test]
    fn cartella_che_non_c_e_o_che_e_un_file() {
        // Per ogni nome il sistema risponde «non c'è» (NotFound, NotADirectory): la ricerca si
        // ferma al primo, e l'errore lo dirà il salvataggio.
        let d = cartella_di_prova("nome-libero-senza-cartella");
        assert_eq!(nome_libero(&d.join("manca"), "Verbale", "docx"), d.join("manca/Verbale.docx"));
        fs::write(d.join("file"), b"x").unwrap();
        assert_eq!(nome_libero(&d.join("file"), "Verbale", "docx"), d.join("file/Verbale.docx"));
    }

    #[test]
    fn un_errore_passeggero_non_fa_libero_il_nome() {
        // In Download c'è già «Verbale.docx» e il suo controllo risponde una volta EIO (5 su Linux
        // e su Android), come un disco o FUSE che per un momento non risponde. Quel nome non è
        // libero: il salvataggio del motore guarda di nuovo, trova il file e lo sostituisce.
        let d = cartella_di_prova("nome-libero-errore-passeggero");
        fs::write(d.join("Verbale.docx"), b"x").unwrap();
        let mut errori = 1;
        let guarda = |percorso: &Path| {
            if percorso.ends_with("Verbale.docx") && errori > 0 {
                errori -= 1;
                return Err(io::Error::from_raw_os_error(5));
            }
            fs::symlink_metadata(percorso).map(drop)
        };
        assert_eq!(cerca_nome_libero(&d, "Verbale", "docx", guarda), d.join("Verbale (2).docx"));
    }

    #[test]
    fn senza_nomi_controllati_si_ferma_al_tetto() {
        // Nessun nome si riesce a controllare, come in una cartella che non si attraversa: la
        // ricerca finisce a «(10000)», un nome che il salvataggio non riuscirà a scrivere.
        let d = Path::new("cartella-chiusa");
        let mut controlli = 0;
        let nome = cerca_nome_libero(d, "Verbale", "docx", |_| {
            controlli += 1;
            Err(io::Error::from(io::ErrorKind::PermissionDenied))
        });
        assert_eq!(nome, d.join("Verbale (10000).docx"));
        assert_eq!(controlli, 10_000);
    }

    #[cfg(unix)]
    #[test]
    fn cartella_che_non_si_attraversa() {
        use std::os::unix::fs::PermissionsExt;
        /// Rimette i permessi anche se la prova fallisce, così la cartella si può cancellare.
        struct Riapri<'a>(&'a Path);
        impl Drop for Riapri<'_> {
            fn drop(&mut self) {
                let _ = fs::set_permissions(self.0, fs::Permissions::from_mode(0o755));
            }
        }
        let d = cartella_di_prova("nome-libero-chiusa");
        let chiusa = d.join("chiusa");
        fs::create_dir(&chiusa).unwrap();
        fs::set_permissions(&chiusa, fs::Permissions::from_mode(0o000)).unwrap();
        let _riapri = Riapri(&chiusa);
        // Da root la cartella si attraversa lo stesso, e la prova non dice niente.
        if fs::symlink_metadata(chiusa.join("Verbale.docx")).err().map(|e| e.kind()) != Some(io::ErrorKind::PermissionDenied) {
            return;
        }
        assert_eq!(nome_libero(&chiusa, "Verbale", "docx"), chiusa.join("Verbale (10000).docx"));
    }

    #[test]
    fn riempie_i_buchi() {
        let d = cartella_di_prova("nome-libero-buchi");
        for nome in ["Verbale.docx", "Verbale (3).docx"] {
            fs::write(d.join(nome), b"x").unwrap();
        }
        assert_eq!(nome_libero(&d, "Verbale", "docx"), d.join("Verbale (2).docx"));
    }

    #[cfg(unix)]
    #[test]
    fn occupato_anche_da_una_cartella_o_da_un_collegamento_rotto() {
        let d = cartella_di_prova("nome-libero-occupato");
        fs::create_dir(d.join("Verbale.docx")).unwrap();
        std::os::unix::fs::symlink(d.join("manca"), d.join("Verbale (2).docx")).unwrap();
        assert_eq!(nome_libero(&d, "Verbale", "docx"), d.join("Verbale (3).docx"));
    }

    #[test]
    fn estensione_col_punto_o_senza() {
        let d = cartella_di_prova("nome-libero-estensione");
        assert_eq!(nome_libero(&d, "Verbale", ".pdf"), d.join("Verbale.pdf"));
        assert_eq!(nome_libero(&d, "Verbale", ""), d.join("Verbale"));
        fs::write(d.join("Verbale"), b"x").unwrap();
        assert_eq!(nome_libero(&d, "Verbale", ""), d.join("Verbale (2)"));
    }

    #[test]
    fn la_base_resta_nella_cartella_e_si_vede() {
        let d = cartella_di_prova("nome-libero-base");
        assert_eq!(nome_libero(&d, "../Segreto: bozza?", "docx"), d.join("Segreto bozza.docx"));
        assert_eq!(nome_libero(&d, ".nascosto", "docx"), d.join("nascosto.docx"));
        assert_eq!(nome_libero(&d, "", "docx"), d.join("Documento.docx"));
        assert_eq!(nome_libero(&d, " / ", "docx"), d.join("Documento.docx"));
        // Il nome di un file che c'è già non si accorcia a 40 caratteri.
        let lungo = "Relazione annuale sulle attività del servizio infermieristico";
        assert_eq!(nome_libero(&d, lungo, "docx"), d.join(format!("{lungo}.docx")));
    }

    #[test]
    fn mai_oltre_255_byte() {
        // 255 byte è il nome più lungo che ext4 e f2fs accettano.
        let d = cartella_di_prova("nome-libero-lungo");
        let base = "è".repeat(126);
        let primo = nome_libero(&d, &base, "docx");
        let nome = primo.file_name().unwrap().to_str().unwrap().to_owned();
        assert!(nome.len() <= 255 && nome.ends_with(".docx"), "{} byte", nome.len());
        fs::write(&primo, b"x").unwrap();
        let secondo = nome_libero(&d, &base, "docx");
        let nome = secondo.file_name().unwrap().to_str().unwrap().to_owned();
        assert!(nome.len() <= 255 && nome.ends_with("è (2).docx"), "{} byte", nome.len());
        fs::write(&secondo, b"x").unwrap();
    }

    #[test]
    fn caratteri_vietati() {
        assert_eq!(nome_pulito(r#"a/b\c:d*e?f"g<h>i|j"#), "a b c d e f g h i j");
        assert_eq!(nome_pulito("Verbale del 12/10/2026"), "Verbale del 12 10 2026");
        assert_eq!(nome_pulito("Domanda: perché?"), "Domanda perché");
    }

    #[test]
    fn lettere_accentate_ed_emoji_restano() {
        assert_eq!(nome_pulito("Perché è così: città e virtù"), "Perché è così città e virtù");
        assert_eq!(nome_pulito("Ærøskøbing, Łódź, São Paulo"), "Ærøskøbing, Łódź, São Paulo");
        assert_eq!(nome_pulito("Festa 🎉 di fine anno"), "Festa 🎉 di fine anno");
    }

    #[test]
    fn al_massimo_quaranta_caratteri() {
        assert_eq!(MASSIMO_CARATTERI_NOME, 40);
        // Taglio dove finisce una parola...
        assert_eq!(nome_pulito("Relazione sulla riunione del consiglio di amministrazione"), "Relazione sulla riunione del consiglio");
        // ...anche quando finisce proprio al quarantesimo carattere.
        assert_eq!(nome_pulito("Verbale della riunione straordinaria del consiglio"), "Verbale della riunione straordinaria del");
        // Senza uno spazio abbastanza avanti si taglia a 40 caratteri, contati come lettere.
        assert_eq!(nome_pulito("Via Lunghissimaparolasenzaspazialcunopercontinuare"), "Via Lunghissimaparolasenzaspazialcunoper");
        assert_eq!(nome_pulito(&"è".repeat(50)), "è".repeat(40));
        // Il taglio non lascia un punto in fondo: qui i primi 40 caratteri finiscono con «finale.».
        assert_eq!(nome_pulito("Primo capitolo dell'introduzione finale. Poi il resto"), "Primo capitolo dell'introduzione finale");
    }

    #[test]
    fn il_taglio_non_spezza_i_caratteri_composti() {
        // Un'emoji di più caratteri uniti da U+200D, una bandiera (due indicatori regionali) e una
        // lettera con l'accento scritto a parte restano intere oppure fuori: mai un U+200D
        // invisibile in fondo, mezza bandiera o una lettera senza l'accento.
        let tecnica = "👩\u{200D}💻";
        assert_eq!(nome_pulito(&format!("Ab{}", tecnica.repeat(13))), format!("Ab{}", tecnica.repeat(12)));
        assert_eq!(nome_pulito(&format!("Viaggio{}", "🇮🇹".repeat(17))), format!("Viaggio{}", "🇮🇹".repeat(16)));
        assert_eq!(nome_pulito(&format!("{}a\u{0300}bc", "x".repeat(39))), "x".repeat(39));
    }

    #[test]
    fn il_taglio_a_255_byte_non_spezza_i_caratteri_composti() {
        // Undici byte per emoji: nei 250 che restano accanto a «.docx», e nei 246 accanto a
        // « (2).docx», ne stanno 22 intere.
        let d = cartella_di_prova("nome-libero-composti");
        let tecnica = "👩\u{200D}💻";
        let primo = nome_libero(&d, &tecnica.repeat(30), "docx");
        assert_eq!(primo, d.join(format!("{}.docx", tecnica.repeat(22))));
        fs::write(&primo, b"x").unwrap();
        assert_eq!(nome_libero(&d, &tecnica.repeat(30), "docx"), d.join(format!("{} (2).docx", tecnica.repeat(22))));
        // Un grafema che da solo supera il limite (una lettera con duecento accenti) si taglia fra
        // i suoi caratteri: un nome vuoto farebbe il file nascosto «.docx».
        let nome = nome_libero(&d, &format!("a{}", "\u{0301}".repeat(200)), "docx");
        let nome = nome.file_name().unwrap().to_str().unwrap();
        assert!(nome.starts_with('a') && nome.ends_with(".docx") && nome.len() <= 255, "{} byte", nome.len());
    }

    #[test]
    fn mai_vuoto() {
        for testo in ["", "   ", "///", "...", ". . .", "\n\t", "\u{FEFF}", "\u{FFFC}", "-----", "= = =\n___", "🎄"] {
            assert_eq!(nome_pulito(testo), "Documento", "{testo:?}");
        }
    }

    #[test]
    fn la_prima_riga_che_dice_qualcosa() {
        assert_eq!(nome_pulito("Verbale\nseconda riga"), "Verbale");
        assert_eq!(nome_pulito("\n\n  \nTitolo vero"), "Titolo vero");
        // Asterischi soli, poi un salto pagina del motore.
        assert_eq!(nome_pulito("***\u{000C}Dopo il salto pagina"), "Dopo il salto pagina");
        // L'interruzione di colonna del motore, come in un .docx a due colonne.
        assert_eq!(nome_pulito("Verbale\u{000E}Ordine del giorno e presenti"), "Verbale");
        assert_eq!(nome_pulito("Prima\u{2028}seconda"), "Prima");
        assert_eq!(nome_pulito("Prima\r\nseconda"), "Prima");
        // Una riga di soli separatori non dice niente, e nemmeno una di sole emoji: conta la prima
        // con una lettera o una cifra.
        assert_eq!(nome_pulito("-----\nVerbale"), "Verbale");
        assert_eq!(nome_pulito("====\nVerbale"), "Verbale");
        assert_eq!(nome_pulito(&format!("{}\nNome e cognome", "_".repeat(60))), "Nome e cognome");
        assert_eq!(nome_pulito("🎄🎄🎄\nAuguri"), "Auguri");
        assert_eq!(nome_pulito("2026\nBilancio"), "2026");
    }

    #[test]
    fn niente_punti_e_spazi_ai_bordi() {
        assert_eq!(nome_pulito("...e poi."), "e poi");
        assert_eq!(nome_pulito("  .Nascosto  "), "Nascosto");
        assert_eq!(nome_pulito("v. 2.0"), "v. 2.0");
    }

    #[test]
    fn spazi_compressi_e_invisibili_tolti() {
        assert_eq!(nome_pulito("Verbale    del\t12"), "Verbale del 12");
        assert_eq!(nome_pulito("Verbale\u{00A0}del\u{00A0}12"), "Verbale del 12");
        assert_eq!(nome_pulito("\u{FEFF}Verbale\u{200B} con\u{00AD}clusivo"), "Verbale conclusivo");
        // Il segnaposto di un'immagine dentro al titolo.
        assert_eq!(nome_pulito("Foto \u{FFFC} della sede"), "Foto della sede");
        // Il trattino che non va a capo diventa quello normale.
        assert_eq!(nome_pulito("Anno 2025\u{2011}2026"), "Anno 2025-2026");
        // Il segno che gira il testo al contrario, che può far sembrare .docx un altro tipo.
        assert_eq!(nome_pulito("Fattura\u{202E}xcod"), "Fatturaxcod");
    }

    #[test]
    fn un_nome_pulito_si_salva_cosi_com_e() {
        let d = cartella_di_prova("nome-pulito-e-libero");
        let nome = nome_pulito("Verbale: riunione del 12/10, «bozza»");
        assert_eq!(nome_libero(&d, &nome, "docx"), d.join(format!("{nome}.docx")));
    }
}
