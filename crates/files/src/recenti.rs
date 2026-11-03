//! I documenti aperti di recente, in un file JSON nella cartella dell'app.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::Tipo;

/// Quanti recenti si tengono al massimo.
pub const MASSIMO_RECENTI: usize = 30;

/// Un documento aperto di recente.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recente {
    /// Il percorso del file oppure l'URI content:// da cui è arrivato: due recenti con la stessa
    /// chiave sono lo stesso documento. Un URI content:// si riapre solo finché vive il task di
    /// Android che l'ha ricevuto: il permesso di lettura di un «Apri con» non è persistente (lo
    /// sarebbe solo con FLAG_GRANT_PERSISTABLE_URI_PERMISSION da chi lo manda, che WhatsApp di
    /// solito non mette, e takePersistableUriPermission da noi) e sparisce quando il task finisce
    /// (per esempio se l'app viene tolta dalle recenti) o il telefono si riavvia. Chi riapre una
    /// voce così deve aspettarsi che non si apra più.
    pub chiave: String,
    /// Il nome da mostrare.
    pub nome: String,
    pub tipo: Tipo,
    /// Quando è stato aperto, in secondi dal 1° gennaio 1970 (UTC).
    pub aperto_alle: u64,
    /// Le pagine di un PDF o le slide di una presentazione, da dire sotto il nome; nei file
    /// scritti prima della 0.3 non c'è.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pagine: Option<usize>,
}

/// I recenti: al massimo `MASSIMO_RECENTI`, il più recente in cima, una voce sola per chiave.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Recenti {
    voci: Vec<Recente>,
}

impl Recenti {
    /// I recenti scritti in `file`. Un file che manca o che non si capisce dà una lista vuota,
    /// mai un errore: i recenti sono una comodità, non devono fermare l'app.
    pub fn carica(file: &Path) -> Recenti {
        let letti = fs::read(file).ok().and_then(|byte| serde_json::from_slice::<Letti>(&byte).ok());
        let mut r = Recenti::default();
        // Dal fondo alla cima con le regole di aggiungi: anche un file scritto male esce con una
        // voce sola per chiave (quella più in alto) e senza superare il tetto.
        for voce in letti.map(|l| l.voci).unwrap_or_default().into_iter().rev() {
            r.aggiungi(voce);
        }
        r
    }

    /// Scrive i recenti in `file` senza lasciarlo mai a metà: prima un file temporaneo nella
    /// stessa cartella, poi lo scambio col rename. Crea la cartella, se manca.
    pub fn salva(&self, file: &Path) -> io::Result<()> {
        let byte = serde_json::to_vec(&Scritti { voci: &self.voci }).map_err(io::Error::other)?;
        scrivi_tutto_o_niente(file, &byte)
    }

    /// Mette `r` in cima; un recente con la stessa chiave lascia il posto a questo.
    pub fn aggiungi(&mut self, r: Recente) {
        self.voci.retain(|v| v.chiave != r.chiave);
        self.voci.insert(0, r);
        self.voci.truncate(MASSIMO_RECENTI);
    }

    pub fn togli(&mut self, chiave: &str) {
        self.voci.retain(|v| v.chiave != chiave);
    }

    pub fn voci(&self) -> &[Recente] {
        &self.voci
    }
}

/// Com'è fatto il file: `{"voci": [...]}`, così una versione più nuova può aggiungere campi.
#[derive(Deserialize)]
struct Letti {
    voci: Vec<Recente>,
}

#[derive(Serialize)]
struct Scritti<'a> {
    voci: &'a [Recente],
}

/// Scrive `byte` in `file` passando da un temporaneo nella stessa cartella: chi legge trova il
/// file di prima o quello nuovo, mai uno a metà. Se qualcosa va storto il temporaneo sparisce.
/// La cartella non si sincronizza, come nel salvataggio del motore: su f2fs, la /data di molti
/// telefoni, il sync di una cartella fa un checkpoint di tutta la partizione. Se il telefono si
/// spegne di colpo subito dopo, lo scambio si può perdere: resta il file di prima, intero, e
/// accanto il temporaneo.
pub(crate) fn scrivi_tutto_o_niente(file: &Path, byte: &[u8]) -> io::Result<()> {
    static PROSSIMO: AtomicU64 = AtomicU64::new(0);
    if file.file_name().is_none() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "manca il nome del file"));
    }
    let cartella = file.parent().unwrap_or(Path::new(""));
    fs::create_dir_all(cartella)?;
    // Un nome corto, nascosto, che nessun altro salvataggio in corso usa (processo e contatore);
    // un temporaneo rimasto da un processo morto con gli stessi numeri si riscrive e basta.
    let numero = PROSSIMO.fetch_add(1, Ordering::Relaxed);
    let temporaneo = cartella.join(format!(".salvataggio-{}-{numero}.tmp", std::process::id()));
    let scritto = (|| {
        let mut f = File::create(&temporaneo)?;
        f.write_all(byte)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&temporaneo, file)
    })();
    if scritto.is_err() {
        let _ = fs::remove_file(&temporaneo);
    }
    scritto
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::prova::cartella_di_prova;
    use serde_json::json;

    fn recente(chiave: &str, aperto_alle: u64) -> Recente {
        Recente { chiave: chiave.into(), nome: format!("{chiave}.docx"), tipo: Tipo::Documento, aperto_alle, pagine: None }
    }

    fn chiavi(r: &Recenti) -> Vec<&str> {
        r.voci().iter().map(|v| v.chiave.as_str()).collect()
    }

    /// I nomi dei file nella cartella.
    fn contenuto(cartella: &Path) -> Vec<String> {
        let mut nomi: Vec<String> = fs::read_dir(cartella).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        nomi.sort();
        nomi
    }

    #[test]
    fn il_piu_recente_in_cima() {
        let mut r = Recenti::default();
        r.aggiungi(recente("a", 1));
        r.aggiungi(recente("b", 2));
        r.aggiungi(recente("c", 3));
        assert_eq!(chiavi(&r), ["c", "b", "a"]);
    }

    #[test]
    fn niente_doppioni_per_chiave() {
        let mut r = Recenti::default();
        r.aggiungi(recente("a", 1));
        r.aggiungi(recente("b", 2));
        let mut di_nuovo = recente("a", 3);
        di_nuovo.nome = "Rinominato.docx".into();
        r.aggiungi(di_nuovo.clone());
        assert_eq!(chiavi(&r), ["a", "b"]);
        assert_eq!(r.voci()[0], di_nuovo);
    }

    #[test]
    fn tetto_di_trenta() {
        assert_eq!(MASSIMO_RECENTI, 30);
        let mut r = Recenti::default();
        for i in 0..35 {
            r.aggiungi(recente(&format!("k{i}"), i));
        }
        assert_eq!(r.voci().len(), 30);
        assert_eq!(r.voci()[0].chiave, "k34");
        assert_eq!(r.voci()[29].chiave, "k5");
        // Uno che c'è già torna in cima senza far uscire nessuno.
        r.aggiungi(recente("k20", 99));
        assert_eq!(r.voci().len(), 30);
        assert_eq!(chiavi(&r)[..2], ["k20", "k34"]);
        assert_eq!(r.voci()[29].chiave, "k5");
    }

    #[test]
    fn togli_per_chiave() {
        let mut r = Recenti::default();
        r.aggiungi(recente("a", 1));
        r.aggiungi(recente("b", 2));
        r.togli("a");
        assert_eq!(chiavi(&r), ["b"]);
        r.togli("non c'è");
        assert_eq!(chiavi(&r), ["b"]);
    }

    #[test]
    fn salva_e_carica() {
        let d = cartella_di_prova("recenti-salva");
        let file = d.join("recenti.json");
        let mut r = Recenti::default();
        r.aggiungi(Recente {
            chiave: "/storage/emulated/0/Documents/Tascabile/Perché è così.docx".into(),
            nome: "Perché è così.docx".into(),
            tipo: Tipo::Documento,
            aperto_alle: 1_729_700_000,
            pagine: None,
        });
        r.aggiungi(Recente {
            chiave: "content://com.whatsapp.provider.media/item/123".into(),
            nome: "Bolletta «luce» \"marzo\".pdf".into(),
            tipo: Tipo::Pdf,
            aperto_alle: 1_729_700_100,
            pagine: Some(3),
        });
        r.salva(&file).unwrap();
        assert_eq!(Recenti::carica(&file), r);
        // Il temporaneo non resta accanto.
        assert_eq!(contenuto(&d), ["recenti.json"]);
        // La forma del file sul disco: le versioni successive dell'app devono saperlo leggere.
        let scritto: serde_json::Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
        assert_eq!(
            scritto,
            json!({"voci": [
                {"chiave": "content://com.whatsapp.provider.media/item/123", "nome": "Bolletta «luce» \"marzo\".pdf", "tipo": "Pdf", "aperto_alle": 1_729_700_100u64, "pagine": 3},
                {"chiave": "/storage/emulated/0/Documents/Tascabile/Perché è così.docx", "nome": "Perché è così.docx", "tipo": "Documento", "aperto_alle": 1_729_700_000u64},
            ]})
        );
    }

    #[test]
    fn salva_sostituisce_il_file_di_prima() {
        let d = cartella_di_prova("recenti-sostituisce");
        let file = d.join("recenti.json");
        let mut r = Recenti::default();
        r.aggiungi(recente("a", 1));
        r.salva(&file).unwrap();
        r.aggiungi(recente("b", 2));
        r.salva(&file).unwrap();
        assert_eq!(chiavi(&Recenti::carica(&file)), ["b", "a"]);
        assert_eq!(contenuto(&d), ["recenti.json"]);
    }

    #[test]
    fn salva_crea_la_cartella() {
        let d = cartella_di_prova("recenti-cartella");
        let file = d.join("dati/app/recenti.json");
        let mut r = Recenti::default();
        r.aggiungi(recente("a", 1));
        r.salva(&file).unwrap();
        assert_eq!(Recenti::carica(&file), r);
    }

    #[test]
    fn salva_che_non_riesce_lascia_tutto_come_prima() {
        // Al posto del file c'è una cartella: lo scambio non riesce, e il temporaneo sparisce.
        let d = cartella_di_prova("recenti-non-riesce");
        let file = d.join("recenti.json");
        fs::create_dir(&file).unwrap();
        let mut r = Recenti::default();
        r.aggiungi(recente("a", 1));
        assert!(r.salva(&file).is_err());
        assert!(file.is_dir());
        assert_eq!(contenuto(&d), ["recenti.json"]);
    }

    #[test]
    fn file_che_manca_lista_vuota() {
        let d = cartella_di_prova("recenti-manca");
        assert_eq!(Recenti::carica(&d.join("recenti.json")), Recenti::default());
    }

    #[test]
    fn i_recenti_di_prima_senza_pagine_si_leggono() {
        let d = cartella_di_prova("recenti-senza-pagine");
        let file = d.join("recenti.json");
        fs::write(&file, br#"{"voci":[{"chiave":"a","nome":"Turni.pdf","tipo":"Pdf","aperto_alle":5}]}"#).unwrap();
        let r = Recenti::carica(&file);
        assert_eq!(r.voci()[0].pagine, None);
        assert_eq!(r.voci()[0].tipo, Tipo::Pdf);
    }

    #[test]
    fn file_rotto_lista_vuota() {
        let d = cartella_di_prova("recenti-rotto");
        let file = d.join("recenti.json");
        let rotti: [&[u8]; 7] = [
            b"",
            b"\xff\xfe\x00spazzatura",
            b"{\"voci\": [{\"chiave\": \"a\"",
            b"{\"voci\": 3}",
            b"[1, 2, 3]",
            b"null",
            b"{\"voci\": [{\"chiave\": 1}]}",
        ];
        for rotto in rotti {
            fs::write(&file, rotto).unwrap();
            assert_eq!(Recenti::carica(&file), Recenti::default(), "{}", String::from_utf8_lossy(rotto));
        }
        // Una cartella al posto del file.
        fs::remove_file(&file).unwrap();
        fs::create_dir(&file).unwrap();
        assert_eq!(Recenti::carica(&file), Recenti::default());
    }

    #[test]
    fn caricati_con_le_stesse_regole() {
        // Un file con doppioni e troppe voci: vince la voce più in alto, oltre il tetto escono le
        // più in basso.
        let d = cartella_di_prova("recenti-regole");
        let file = d.join("recenti.json");
        let mut voci: Vec<_> =
            (0..40).map(|i| json!({"chiave": format!("k{i}"), "nome": "giusto", "tipo": "Documento", "aperto_alle": 100 - i})).collect();
        voci.insert(1, json!({"chiave": "k0", "nome": "doppione", "tipo": "Pdf", "aperto_alle": 1}));
        fs::write(&file, serde_json::to_vec(&json!({ "voci": voci })).unwrap()).unwrap();
        let r = Recenti::carica(&file);
        assert_eq!(r.voci().len(), 30);
        assert_eq!(chiavi(&r)[..3], ["k0", "k1", "k2"]);
        assert_eq!(r.voci()[0].nome, "giusto");
        assert_eq!(r.voci()[29].chiave, "k29");
    }

    #[test]
    fn tipo_sconosciuto_diventa_altro() {
        // Un recente scritto da una versione più nuova, con un tipo che questa non conosce.
        let d = cartella_di_prova("recenti-tipo-nuovo");
        let file = d.join("recenti.json");
        let voci = json!({"voci": [
            {"chiave": "/x/Mappa.svg", "nome": "Mappa.svg", "tipo": "Disegno", "aperto_alle": 5},
            {"chiave": "/x/a.docx", "nome": "a.docx", "tipo": "Documento", "aperto_alle": 4},
        ]});
        fs::write(&file, serde_json::to_vec(&voci).unwrap()).unwrap();
        let tipi: Vec<Tipo> = Recenti::carica(&file).voci().iter().map(|v| v.tipo).collect();
        assert_eq!(tipi, [Tipo::Altro, Tipo::Documento]);
        // Un foglio di calcolo, dalla 0.9.0, è un tipo che si conosce.
        fs::write(&file, br#"{"voci": [{"chiave": "/x/Conti.xlsx", "nome": "Conti.xlsx", "tipo": "Foglio", "aperto_alle": 5}]}"#).unwrap();
        assert_eq!(Recenti::carica(&file).voci()[0].tipo, Tipo::Foglio);
    }

    #[test]
    fn campi_nuovi_ignorati() {
        // Una versione più nuova può aggiungere campi senza far perdere la lista a questa.
        let d = cartella_di_prova("recenti-campi-nuovi");
        let file = d.join("recenti.json");
        let voci = json!({"filtro": "Pdf", "voci": [
            {"chiave": "/x/a.docx", "nome": "a.docx", "tipo": "Documento", "aperto_alle": 4, "pagina": 3},
        ]});
        fs::write(&file, serde_json::to_vec(&voci).unwrap()).unwrap();
        assert_eq!(chiavi(&Recenti::carica(&file)), ["/x/a.docx"]);
    }
}
