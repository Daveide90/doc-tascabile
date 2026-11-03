//! Dove si era arrivati a leggere ogni PDF: la pagina, il punto nella pagina e lo zoom, in un file
//! JSON nella cartella dell'app, per riprendere da lì. La chiave è l'impronta del contenuto del
//! file, non il suo nome né il percorso: lo stesso PDF arrivato da WhatsApp o aperto dalla
//! cartella riprende dallo stesso punto, e nel file e nella diagnosi non c'è nessun nome.

use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::recenti::scrivi_tutto_o_niente;

/// Quante posizioni si tengono al massimo: le più vecchie escono.
pub const MASSIMO_POSIZIONI: usize = 300;

/// L'impronta di un file: FNV-1a a 64 bit di tutti i byte, con la misura, in 16 cifre
/// esadecimali più la misura. Non è una firma: serve a riconoscere lo stesso contenuto, e da lei
/// non si risale né al nome né al testo. Costa una passata sui byte (circa 1 GB al secondo).
pub fn impronta(byte: &[u8]) -> String {
    const BASE: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIMO: u64 = 0x0000_0100_0000_01b3;
    let mut h = BASE;
    for b in byte {
        h ^= u64::from(*b);
        h = h.wrapping_mul(PRIMO);
    }
    format!("{h:016x}-{}", byte.len())
}

/// Dove si era arrivati in un PDF.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Posizione {
    /// L'impronta del file ([`impronta`]).
    pub chiave: String,
    /// La pagina, da 0.
    pub pagina: usize,
    /// Il punto della pagina in pt dall'alto che stava in cima alla vista.
    pub y: f32,
    /// Il punto in orizzontale che stava al centro della vista, in frazione della larghezza della
    /// pagina (da 0 a 1); None nei file di prima, che lo lasciavano fuori.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<f32>,
    pub zoom: f32,
    /// Quando, in secondi dal 1° gennaio 1970 (UTC).
    pub alle: u64,
}

/// Le posizioni ricordate: la più recente in cima, una per chiave, al massimo
/// [`MASSIMO_POSIZIONI`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Posizioni {
    voci: Vec<Posizione>,
}

#[derive(Deserialize)]
struct Lette {
    voci: Vec<Posizione>,
}

#[derive(Serialize)]
struct Scritte<'a> {
    voci: &'a [Posizione],
}

impl Posizioni {
    /// Le posizioni scritte in `file`; un file che manca o non si capisce dà una lista vuota.
    /// Le voci con numeri impossibili (zoom o punto non finiti) si lasciano fuori.
    pub fn carica(file: &Path) -> Posizioni {
        let lette = fs::read(file).ok().and_then(|byte| serde_json::from_slice::<Lette>(&byte).ok());
        let mut p = Posizioni::default();
        for mut voce in lette.map(|l| l.voci).unwrap_or_default().into_iter().rev() {
            if voce.y.is_finite() && voce.zoom.is_finite() && voce.zoom > 0.0 {
                voce.x = voce.x.filter(|x| x.is_finite() && (0.0..=1.0).contains(x));
                p.ricorda(voce);
            }
        }
        p
    }

    /// Scrive le posizioni in `file`, senza lasciarlo mai a metà.
    pub fn salva(&self, file: &Path) -> io::Result<()> {
        let byte = serde_json::to_vec(&Scritte { voci: &self.voci }).map_err(io::Error::other)?;
        scrivi_tutto_o_niente(file, &byte)
    }

    /// Mette `p` in cima, al posto di quella con la stessa chiave.
    pub fn ricorda(&mut self, p: Posizione) {
        self.voci.retain(|v| v.chiave != p.chiave);
        self.voci.insert(0, p);
        self.voci.truncate(MASSIMO_POSIZIONI);
    }

    pub fn trova(&self, chiave: &str) -> Option<&Posizione> {
        self.voci.iter().find(|v| v.chiave == chiave)
    }

    pub fn voci(&self) -> &[Posizione] {
        &self.voci
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::prova::cartella_di_prova;

    fn posizione(chiave: &str, pagina: usize) -> Posizione {
        Posizione { chiave: chiave.into(), pagina, y: 120.5, x: None, zoom: 1.5, alle: 1_792_738_800 }
    }

    #[test]
    fn l_impronta_riconosce_il_contenuto_senza_il_nome() {
        let a = impronta(b"%PDF-1.7 contenuto");
        assert_eq!(a, impronta(b"%PDF-1.7 contenuto"));
        assert_ne!(a, impronta(b"%PDF-1.7 contenutO"));
        assert_ne!(impronta(b""), impronta(b"\0"));
        // Il valore noto di FNV-1a a 64 bit per «a».
        assert_eq!(impronta(b"a"), "af63dc4c8601ec8c-1");
        assert_eq!(a.len(), 16 + 1 + 2);
    }

    #[test]
    fn la_piu_recente_in_cima_una_per_chiave_e_il_tetto() {
        let mut p = Posizioni::default();
        for i in 0..MASSIMO_POSIZIONI + 5 {
            p.ricorda(posizione(&format!("k{i}"), i));
        }
        assert_eq!(p.voci().len(), MASSIMO_POSIZIONI);
        assert_eq!(p.voci()[0].chiave, format!("k{}", MASSIMO_POSIZIONI + 4));
        assert!(p.trova("k0").is_none() && p.trova("k5").is_some());
        p.ricorda(posizione("k5", 99));
        assert_eq!(p.voci()[0], posizione("k5", 99));
        assert_eq!(p.voci().iter().filter(|v| v.chiave == "k5").count(), 1);
    }

    #[test]
    fn salva_carica_e_scarta_quello_che_non_va() {
        let d = cartella_di_prova("posizioni");
        let file = d.join("app/posizioni-pdf.json");
        assert_eq!(Posizioni::carica(&file), Posizioni::default());
        let mut p = Posizioni::default();
        p.ricorda(posizione("a", 1));
        p.ricorda(posizione("b", 7));
        p.salva(&file).unwrap();
        assert_eq!(Posizioni::carica(&file), p);
        let scritto: serde_json::Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
        assert_eq!(scritto["voci"][0], serde_json::json!({"chiave": "b", "pagina": 7, "y": 120.5, "zoom": 1.5, "alle": 1_792_738_800u64}));
        // Uno zoom a zero o negativo non si riprende; un file rotto è una lista vuota.
        std::fs::write(
            &file,
            br#"{"voci":[{"chiave":"c","pagina":2,"y":1.0,"zoom":0.0,"alle":1},{"chiave":"d","pagina":3,"y":1.0,"zoom":2.0,"alle":1}]}"#,
        )
        .unwrap();
        let lette = Posizioni::carica(&file);
        assert_eq!(lette.voci().iter().map(|v| v.chiave.as_str()).collect::<Vec<_>>(), ["d"]);
        std::fs::write(&file, b"{\"voci\": 3}").unwrap();
        assert_eq!(Posizioni::carica(&file), Posizioni::default());
    }

    #[test]
    fn la_x_si_scrive_e_si_legge_e_i_file_di_prima_vanno_ancora() {
        let d = cartella_di_prova("posizioni-x");
        let file = d.join("app/posizioni-pdf.json");
        let mut p = Posizioni::default();
        p.ricorda(Posizione { x: Some(0.75), ..posizione("a", 1) });
        p.salva(&file).unwrap();
        assert_eq!(Posizioni::carica(&file).voci()[0].x, Some(0.75));
        // Un file di prima, senza la x, e una x impossibile: None.
        std::fs::write(
            &file,
            br#"{"voci":[{"chiave":"c","pagina":2,"y":1.0,"zoom":1.0,"alle":1},{"chiave":"d","pagina":3,"y":1.0,"zoom":2.0,"x":7.5,"alle":1}]}"#,
        )
        .unwrap();
        let lette = Posizioni::carica(&file);
        assert_eq!(lette.voci().iter().map(|v| (v.chiave.as_str(), v.x)).collect::<Vec<_>>(), [("c", None), ("d", None)]);
    }
}
