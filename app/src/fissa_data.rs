//! «Timestamp con blockchain» e «Controlla il timestamp» nei menu di Word, PDF e Slide (piano
//! un documento interno di lavoro; il nome è dell'autore, che non voleva «Fissa
//! la data»): la prova pubblica e gratuita che il file esisteva così com'è, col protocollo aperto
//! OpenTimestamps (crate `tascabile-prova-data`). È
//! l'unica cosa per cui l'app va in rete, e ci va solo l'impronta del file. La prova si salva
//! accanto al file (`Nome.pdf.ots`), o in Documents/Tascabile se il file è arrivato da un'altra
//! app o la sua cartella non si scrive. Il lavoro gira su un thread suo, col dialogo
//! dell'avanzamento; l'esito sale in un foglio.
//!
//! Nella diagnosi esiti e tempi, mai nomi di file né impronte.

use std::path::{Path, PathBuf};

use eframe::egui::{self, RichText};
use tascabile_platform::{RispostaRete, diagnosi};
use tascabile_prova_data::{ErroreData, ErroreRete, Stato};
use tascabile_touch::tema::{self, Carattere};
use tascabile_touch::{Dialogo, FoglioDalFondo, Peso, Pulsante, RispostaDialogo, icone};
use tascabile_word::salva::Origine;

use crate::pdf_ocr::{Lavoro, mostra_barra};
use crate::quando::Locale;
use crate::{Schermata, TascabileApp, home};

/// Le voci dei menu e il titolo del foglio.
pub(crate) const TIMESTAMP_CON_BLOCKCHAIN: &str = "Timestamp con blockchain";
pub(crate) const CONTROLLA_IL_TIMESTAMP: &str = "Controlla il timestamp";
/// Il pulsante del foglio che spiega.
pub(crate) const CREA_IL_TIMESTAMP: &str = "Crea il timestamp";
/// I titoli degli esiti.
pub(crate) const TIMESTAMP_CREATO: &str = "Timestamp creato";
pub(crate) const TIMESTAMP_CONFERMATO: &str = "Timestamp confermato";
pub(crate) const IN_ATTESA: &str = "Conferma in arrivo";
pub(crate) const FILE_CAMBIATO: &str = "File cambiato";
pub(crate) const SENZA_RETE: &str = "Niente internet";
pub(crate) const TIMESTAMP_CHE_NON_VA: &str = "Timestamp che non va";
/// La spiegazione prima di fare il timestamp.
pub(crate) const SPIEGAZIONE: [&str; 4] = [
    "Crea un timestamp con blockchain: una prova pubblica e gratuita che questo file esisteva oggi, così com'è.",
    "Per crearlo l'app si collega a internet, ai server pubblici di OpenTimestamps: va solo l'impronta del file, un codice che cambia \
     se cambia anche una virgola e da cui il file non si ricostruisce. Il documento resta sul telefono.",
    "Il timestamp si salva accanto al file, col suo nome e «.ots»: tienilo insieme al file e non modificare il file, o non varrà per \
     la versione nuova. La conferma arriva entro qualche ora, quando l'impronta entra in un blocco della blockchain: poi «Controlla il \
     timestamp», dallo stesso menu.",
    "Non è una firma e non è la marca temporale con valore legale: è una prova che chiunque può controllare, anche senza l'app, \
     col programma libero OpenTimestamps.",
];

/// Il file del documento sullo schermo: il percorso o l'URI da cui è aperto, e il suo nome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileDelDocumento {
    chiave: String,
    nome: String,
}

impl FileDelDocumento {
    fn uri(&self) -> bool {
        home::e_uri(&self.chiave)
    }

    /// Dove può stare la sua prova, in ordine: accanto al file (se è un file del telefono) e in
    /// Documents/Tascabile.
    fn posti(&self, radice: &Path) -> Vec<PathBuf> {
        let mut v = Vec::new();
        if !self.uri() {
            v.push(PathBuf::from(format!("{}.ots", self.chiave)));
        }
        // Solo il nome del file: per un documento arrivato con «Apri con» lo manda l'altra app, e
        // «../» o un percorso intero portavano il .ots fuori da Documents/Tascabile (revisione del
        // 31/10, punto 7).
        let nome = Path::new(&self.nome).file_name().map_or_else(|| "documento".to_string(), |n| n.to_string_lossy().into_owned());
        v.push(tascabile_files::cartella_tascabile(radice).join(format!("{nome}.ots")));
        v
    }

    /// La prova che c'è già, se c'è.
    fn prova(&self, radice: &Path) -> Option<PathBuf> {
        self.posti(radice).into_iter().find(|p| p.is_file())
    }
}

/// Com'è andato il lavoro sul thread.
#[derive(Debug)]
pub(crate) enum Lavorato {
    Fissata { posto: PathBuf },
    Controllata { stato: Stato, posto: PathBuf },
}

/// Perché il lavoro non è riuscito.
#[derive(Debug)]
pub(crate) enum ErroreLavoro {
    /// Il file del documento o la sua prova non si leggono.
    Lettura,
    /// La prova non si scrive in nessuno dei posti.
    Scrittura,
    Data(ErroreData),
}

type Esito = Result<Lavorato, ErroreLavoro>;

/// La domanda prima di controllare un timestamp, che va su internet (roadmap della sicurezza,
/// livello 1: la rete solo col consenso, a ogni uso).
pub(crate) const CONTROLLARE: &str = "Controllare il timestamp?";
pub(crate) const CONTROLLA: &str = "Controlla";
pub(crate) const NO: &str = "No";
const CONSENSO_CONTROLLO: &str = "Per controllarlo l'app si collega a internet: chiede ai server di OpenTimestamps e agli esploratori \
     della blockchain se l'impronta del file è entrata in un blocco. Va solo l'impronta, mai il documento; come ogni sito, i server \
     vedono l'indirizzo internet del telefono.";
/// Con la rete per il timestamp spenta nelle Impostazioni.
pub(crate) const RETE_SPENTA: &str = "La rete per il timestamp è spenta: si accende nelle Impostazioni, in «Sicurezza e privacy».";

/// Quello che il foglio mostra.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Contenuto {
    /// La spiegazione, col pulsante per fare il timestamp di questo file.
    Spiega(FileDelDocumento),
    /// Un esito: titolo e testo.
    Esito { titolo: String, testo: String },
}

/// «Timestamp con blockchain» nell'app: il foglio, quello che mostra, il lavoro in corso.
pub(crate) struct DataDelDocumento {
    pub(crate) foglio: FoglioDalFondo,
    contenuto: Option<Contenuto>,
    lavoro: Option<(Lavoro<Esito>, &'static str)>,
    /// Il controllo che aspetta il consenso: il file e la sua prova.
    da_controllare: Option<(FileDelDocumento, PathBuf)>,
}

impl DataDelDocumento {
    pub(crate) fn nuova() -> Self {
        Self { foglio: FoglioDalFondo::new(egui::Id::new("foglio_data")), contenuto: None, lavoro: None, da_controllare: None }
    }
}

/// La rete della piattaforma per il crate della prova: 404 vuol dire «non ancora».
struct ReteApp(tascabile_platform::Rete);

fn dalla_piattaforma(r: RispostaRete) -> Result<Vec<u8>, ErroreRete> {
    match r {
        RispostaRete::Ok(b) => Ok(b),
        RispostaRete::Stato(404) => Err(ErroreRete::NonTrovato),
        RispostaRete::Stato(s) => Err(ErroreRete::Risposta(s)),
        RispostaRete::Collegamento => Err(ErroreRete::Collegamento),
        RispostaRete::TroppoLunga => Err(ErroreRete::TroppoLunga),
    }
}

impl tascabile_prova_data::Rete for ReteApp {
    fn post(&self, indirizzo: &str, corpo: &[u8]) -> Result<Vec<u8>, ErroreRete> {
        dalla_piattaforma(self.0.chiedi(indirizzo, Some(corpo)))
    }

    fn get(&self, indirizzo: &str) -> Result<Vec<u8>, ErroreRete> {
        dalla_piattaforma(self.0.chiedi(indirizzo, None))
    }
}

/// «28/10/2026 alle 21:12», nell'ora italiana.
fn quando(secondi_utc: u32) -> String {
    let l = Locale::da_utc(u64::from(secondi_utc));
    format!("{:02}/{:02}/{} alle {}:{:02}", l.giorno, l.mese, l.anno, l.ora, l.minuti)
}

/// Il nome del file della prova e dove sta, per i messaggi: «Contratto.pdf.ots, accanto al file».
fn dove_sta(posto: &Path, radice: &Path) -> String {
    let nome = posto.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if posto.starts_with(tascabile_files::cartella_tascabile(radice)) {
        format!("{nome}, in Documents/Tascabile")
    } else {
        format!("{nome}, accanto al file")
    }
}

/// Scrive la prova nel primo posto dove si riesce: accanto al file, se no in Documents/Tascabile.
/// Mai sopra un file che c'è già: il file si crea in esclusiva, e se il nome è preso (anche da un
/// collegamento, che non si segue) si passa al posto dopo. Prima c'era un controllo e poi la
/// scrittura: un file comparso in mezzo si sovrascriveva (revisione del 10/11). Una scrittura
/// rimasta a metà non lascia il pezzo.
fn scrivi_nuova(posti: &[PathBuf], byte: &[u8]) -> Option<PathBuf> {
    use std::io::Write;
    posti.iter().find_map(|p| {
        if let Some(cartella) = p.parent() {
            let _ = std::fs::create_dir_all(cartella);
        }
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(p).ok()?;
        match file.write_all(byte).and_then(|()| file.sync_all()) {
            Ok(()) => Some(p.clone()),
            Err(_) => {
                drop(file);
                let _ = std::fs::remove_file(p);
                None
            }
        }
    })
}

/// Com'è andata al timestamp di un file rinominato.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TimestampRinominato {
    /// Il file non ha un timestamp, o quello in Documents/Tascabile con quel nome non è il suo.
    Nessuno,
    /// Il timestamp ha preso il nome nuovo, come il file.
    Seguito,
    /// Il timestamp c'è ma è rimasto col nome di prima: il nome nuovo è preso, o il disco ha
    /// detto di no.
    Rimasto,
}

/// Il nome di un file con «.ots» in coda, senza passare da una stringa: vale anche per i nomi
/// che non sono UTF-8.
fn con_ots(file: &Path) -> PathBuf {
    let mut nome = file.as_os_str().to_os_string();
    nome.push(".ots");
    PathBuf::from(nome)
}

/// Dopo il rinomina di un file, il suo timestamp prende il nome nuovo (revisione del 10/11:
/// restava col nome vecchio, e «Controlla il timestamp» sul file rinominato non lo trovava più).
/// Accanto al file il `.ots` è suo per posto. In Documents/Tascabile, dove va quando accanto al
/// file non si scrive, il nome non basta, perché due file con lo stesso nome in cartelle diverse
/// lì hanno lo stesso posto: segue il file solo la prova con l'impronta dei suoi byte. Mai sopra
/// un altro file.
pub(crate) fn rinomina_il_timestamp(vecchio: &Path, nuovo: &Path, radice: &Path) -> TimestampRinominato {
    let sposta = |da: &Path, a: &Path| match tascabile_files::sposta_senza_sostituire(da, a) {
        Ok(()) => TimestampRinominato::Seguito,
        Err(_) => TimestampRinominato::Rimasto,
    };
    let accanto = con_ots(vecchio);
    if accanto.is_file() {
        return sposta(&accanto, &con_ots(nuovo));
    }
    let (Some(nome_vecchio), Some(nome_nuovo)) = (vecchio.file_name(), nuovo.file_name()) else {
        return TimestampRinominato::Nessuno;
    };
    let cartella = tascabile_files::cartella_tascabile(radice);
    let prova = cartella.join(con_ots(Path::new(nome_vecchio)));
    if !prova.is_file() {
        return TimestampRinominato::Nessuno;
    }
    let sua = leggi_la_prova(&prova).ok().and_then(|byte| tascabile_prova_data::Prova::leggi(&byte).ok()).is_some_and(|p| {
        let impronta = tascabile_prova_data::formato::esadecimale(p.impronta());
        tascabile_files::ImprontaFile::del_file(nuovo).is_ok_and(|i| i.sha256 == impronta)
    });
    if !sua {
        return TimestampRinominato::Nessuno;
    }
    sposta(&prova, &cartella.join(con_ots(Path::new(nome_nuovo))))
}

/// Il titolo e il testo del foglio per un esito.
fn esito_da_mostrare(esito: &Esito, radice: &Path) -> (String, String) {
    match esito {
        Ok(Lavorato::Fissata { posto }) => (
            TIMESTAMP_CREATO.into(),
            format!(
                "Il timestamp è salvato: {}. La conferma arriva entro qualche ora: poi, dallo stesso menu, «Controlla il timestamp».",
                dove_sta(posto, radice)
            ),
        ),
        Ok(Lavorato::Controllata { stato, posto }) => match stato {
            Stato::Confermata { ora, .. } => (
                TIMESTAMP_CONFERMATO.into(),
                format!(
                    "Il file esisteva già così com'è il {}: lo conferma la blockchain. Chiunque può controllarlo col timestamp, {}.",
                    quando(*ora),
                    dove_sta(posto, radice)
                ),
            ),
            Stato::InAttesa => {
                (IN_ATTESA.into(), "La conferma non è ancora arrivata: di solito ci vuole qualche ora. Riprova più tardi, dallo stesso menu.".into())
            }
            Stato::Diverso => (
                FILE_CAMBIATO.into(),
                "Il file è cambiato dopo il timestamp: non vale per questa versione. Per fare il timestamp di questa, salvane una copia e \
                 fai il timestamp della copia."
                    .into(),
            ),
            Stato::SenzaRete => (SENZA_RETE.into(), "Non riesco a raggiungere i server: sei collegato a internet? Riprova fra poco.".into()),
        },
        Err(ErroreLavoro::Data(ErroreData::PocheRisposte(n))) => (
            SENZA_RETE.into(),
            format!(
                "Hanno risposto {n} server su {}, e ne servono almeno {}: sei collegato a internet? Riprova fra poco.",
                tascabile_prova_data::server::AGGREGATORI.len(),
                tascabile_prova_data::server::RISPOSTE_MINIME
            ),
        ),
        Err(ErroreLavoro::Data(ErroreData::Formato(_))) => {
            (TIMESTAMP_CHE_NON_VA.into(), "Il timestamp accanto al file non si legge: forse è rovinato.".into())
        }
        Err(ErroreLavoro::Data(ErroreData::Blocco(_))) => (
            TIMESTAMP_CHE_NON_VA.into(),
            "Il timestamp non regge: la blockchain non contiene quello che dice. Non fidarti di questo timestamp.".into(),
        ),
        Err(ErroreLavoro::Lettura) => (TIMESTAMP_CHE_NON_VA.into(), "Non riesco a leggere il file o il suo timestamp.".into()),
        Err(ErroreLavoro::Scrittura) => {
            (TIMESTAMP_CHE_NON_VA.into(), "Non riesco a salvare il timestamp, né accanto al file né in Documents/Tascabile.".into())
        }
    }
}

/// Per la diagnosi: com'è andata, senza nomi.
fn riga_diagnosi(esito: &Esito) -> String {
    match esito {
        Ok(Lavorato::Fissata { .. }) => "data: fissata".into(),
        Ok(Lavorato::Controllata { stato: Stato::Confermata { altezza, .. }, .. }) => format!("data: confermata nel blocco {altezza}"),
        Ok(Lavorato::Controllata { stato: Stato::InAttesa, .. }) => "data: in attesa".into(),
        Ok(Lavorato::Controllata { stato: Stato::Diverso, .. }) => "data: file cambiato".into(),
        Ok(Lavorato::Controllata { stato: Stato::SenzaRete, .. }) => "data: senza rete".into(),
        Err(ErroreLavoro::Data(e)) => format!("data: non riuscita ({e})"),
        Err(ErroreLavoro::Lettura) => "data: lettura non riuscita".into(),
        Err(ErroreLavoro::Scrittura) => "data: prova non scritta".into(),
    }
}

impl TascabileApp {
    /// Il file del documento sullo schermo, o perché non si può fissarne la data.
    fn file_per_la_data(&self) -> Result<FileDelDocumento, &'static str> {
        const SALVA_PRIMA: &str = "Salva prima: il timestamp si fa sul file salvato.";
        const NESSUN_FILE: &str = "Salva prima il documento: il timestamp si fa su un file.";
        let nome_di = |chiave: &str, nome: &str| -> String {
            if home::e_uri(chiave) {
                nome.to_string()
            } else {
                Path::new(chiave).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| nome.to_string())
            }
        };
        match self.schermata {
            Schermata::Editor => {
                let a = self.aperto.as_ref().ok_or(NESSUN_FILE)?;
                if a.prova.is_some() {
                    return Err(NESSUN_FILE);
                }
                if a.editor.s.dirty {
                    return Err(SALVA_PRIMA);
                }
                match &a.editor.origine {
                    Origine::File(p) => {
                        let chiave = p.to_string_lossy().into_owned();
                        Ok(FileDelDocumento { nome: nome_di(&chiave, &a.editor.nome), chiave })
                    }
                    Origine::Uri { uri, nome } => Ok(FileDelDocumento { chiave: uri.clone(), nome: nome.clone() }),
                    Origine::Nuovo | Origine::Recuperato { .. } => Err(NESSUN_FILE),
                }
            }
            Schermata::Pdf => {
                let p = self.pdf.as_ref().ok_or(NESSUN_FILE)?;
                if p.motore.stato().modificato {
                    return Err(SALVA_PRIMA);
                }
                let chiave = p.chiave.clone().ok_or(NESSUN_FILE)?;
                Ok(FileDelDocumento { nome: nome_di(&chiave, &p.nome), chiave })
            }
            Schermata::Slide => {
                let s = self.slide.as_ref().ok_or(NESSUN_FILE)?;
                let chiave = s.chiave_del_file().ok_or(NESSUN_FILE)?;
                Ok(FileDelDocumento { nome: nome_di(&chiave, s.nome_del_file()), chiave })
            }
            Schermata::Foglio => {
                let f = self.foglio.as_ref().ok_or(NESSUN_FILE)?;
                let chiave = f.chiave.clone().ok_or(NESSUN_FILE)?;
                Ok(FileDelDocumento { nome: nome_di(&chiave, &f.nome), chiave })
            }
            _ => Err(NESSUN_FILE),
        }
    }

    /// La voce del menu: «Controlla il timestamp» se accanto al file c'è la sua prova.
    pub(crate) fn voce_della_data(&self) -> &'static str {
        let radice = self.piattaforma.radice_archivio();
        match self.file_per_la_data() {
            Ok(f) if f.prova(&radice).is_some() => CONTROLLA_IL_TIMESTAMP,
            _ => TIMESTAMP_CON_BLOCKCHAIN,
        }
    }

    /// La voce del menu toccata: controlla la prova che c'è, o spiega prima di fare il timestamp.
    pub(crate) fn data_dal_menu(&mut self, ctx: &egui::Context) {
        if self.data.lavoro.is_some() {
            return;
        }
        let f = match self.file_per_la_data() {
            Ok(f) => f,
            Err(perche) => {
                diagnosi("data: non si può, documento senza file o non salvato");
                self.avviso_breve.avvisa(perche);
                return;
            }
        };
        if !self.impostazioni.rete_timestamp {
            diagnosi("data: rete spenta nelle Impostazioni");
            self.avviso_breve.avvisa(RETE_SPENTA);
            return;
        }
        let radice = self.piattaforma.radice_archivio();
        match f.prova(&radice) {
            Some(posto) => {
                diagnosi("data: si chiede il consenso alla rete per il controllo");
                self.data.da_controllare = Some((f, posto));
                ctx.request_repaint();
            }
            None => {
                diagnosi("data: spiegazione aperta");
                self.data.contenuto = Some(Contenuto::Spiega(f));
                self.data.foglio.apri();
            }
        }
    }

    /// I byte del file: quelli di un URI si leggono adesso (serve l'Activity), quelli di un
    /// percorso sul thread di lavoro.
    fn byte_subito(&self, f: &FileDelDocumento) -> Result<Option<Vec<u8>>, ErroreLavoro> {
        if f.uri() { self.piattaforma.read_uri(&f.chiave).map(Some).map_err(|_| ErroreLavoro::Lettura) } else { Ok(None) }
    }

    fn fissa_la_data(&mut self, ctx: &egui::Context, f: FileDelDocumento) {
        let subito = match self.byte_subito(&f) {
            Ok(b) => b,
            Err(e) => return self.mostra_esito(Err(e)),
        };
        let mut numero = [0u8; 16];
        if getrandom::fill(&mut numero).is_err() {
            diagnosi("data: numero a caso non disponibile");
            return self.mostra_esito(Err(ErroreLavoro::Lettura));
        }
        let rete = ReteApp(self.piattaforma.rete());
        // Il consenso è il tocco su «Crea il timestamp», dopo la spiegazione: la rete si apre per
        // questo lavoro e si richiude alla fine (`passo_data`).
        self.piattaforma.consenti_rete(true);
        let posti = f.posti(&self.piattaforma.radice_archivio());
        let chiave = f.chiave.clone();
        diagnosi("data: fisso la data");
        let lavoro = Lavoro::avvia("data-fissa", ctx, move |avanti| -> Esito {
            avanti(0, 2);
            let byte = match subito {
                Some(b) => b,
                None => std::fs::read(&chiave).map_err(|_| ErroreLavoro::Lettura)?,
            };
            let prova = tascabile_prova_data::fissa(&byte, &rete, numero).map_err(ErroreLavoro::Data)?;
            avanti(1, 2);
            let posto = scrivi_nuova(&posti, &prova).ok_or(ErroreLavoro::Scrittura)?;
            Ok(Lavorato::Fissata { posto })
        });
        self.data.lavoro = lavoro.map(|l| (l, TIMESTAMP_CON_BLOCKCHAIN));
        if self.data.lavoro.is_none() {
            self.piattaforma.consenti_rete(false);
        }
    }

    fn controlla_la_data(&mut self, ctx: &egui::Context, f: FileDelDocumento, posto: PathBuf) {
        let subito = match self.byte_subito(&f) {
            Ok(b) => b,
            Err(e) => return self.mostra_esito(Err(e)),
        };
        let rete = ReteApp(self.piattaforma.rete());
        self.piattaforma.consenti_rete(true);
        let chiave = f.chiave.clone();
        diagnosi("data: controllo la data");
        let lavoro = Lavoro::avvia("data-controlla", ctx, move |avanti| -> Esito {
            avanti(0, 2);
            let byte = match subito {
                Some(b) => b,
                None => std::fs::read(&chiave).map_err(|_| ErroreLavoro::Lettura)?,
            };
            let prova = leggi_la_prova(&posto)?;
            let c = tascabile_prova_data::controlla(&byte, &prova, &rete).map_err(ErroreLavoro::Data)?;
            avanti(1, 2);
            // Il seguito dei server va nella prova: la volta dopo non serve chiederlo.
            if let Some(aggiornata) = c.aggiornata {
                tascabile_word::salva::scrivi_atomico(&posto, &aggiornata).map_err(|_| ErroreLavoro::Scrittura)?;
            }
            Ok(Lavorato::Controllata { stato: c.stato, posto })
        });
        self.data.lavoro = lavoro.map(|l| (l, CONTROLLA_IL_TIMESTAMP));
        if self.data.lavoro.is_none() {
            self.piattaforma.consenti_rete(false);
        }
    }

    fn mostra_esito(&mut self, esito: Esito) {
        diagnosi(riga_diagnosi(&esito));
        let (titolo, testo) = esito_da_mostrare(&esito, &self.piattaforma.radice_archivio());
        self.data.contenuto = Some(Contenuto::Esito { titolo, testo });
        self.data.foglio.apri();
    }

    /// A ogni frame: l'esito del lavoro, se è arrivato, nel foglio.
    pub(crate) fn passo_data(&mut self) {
        let Some((l, _)) = self.data.lavoro.as_ref() else { return };
        let Some(esito) = l.esito() else { return };
        let fermato = l.fermato();
        let secondi = l.inizio.elapsed().as_secs_f32();
        self.data.lavoro = None;
        // Finito il lavoro a cui si era dato il consenso, la rete si richiude.
        self.piattaforma.consenti_rete(false);
        diagnosi(format!("data: lavoro finito in {secondi:.1} s"));
        match esito {
            // Annullato: l'esito arriva lo stesso, ma chi ha annullato non lo vuole.
            _ if fermato => diagnosi("data: annullata"),
            Some(e) => self.mostra_esito(e),
            None => self.mostra_esito(Err(ErroreLavoro::Lettura)),
        }
    }

    /// Indietro: chiude il foglio, o lascia andare il lavoro in corso. Vero se è stato usato qui.
    pub(crate) fn indietro_data(&mut self) -> bool {
        if let Some((l, _)) = self.data.lavoro.as_ref() {
            l.ferma();
            return true;
        }
        if self.data.foglio.indietro() {
            return true;
        }
        false
    }

    /// Il dialogo dell'avanzamento mentre si lavora, e il foglio della spiegazione o dell'esito.
    pub(crate) fn mostra_data(&mut self, ctx: &egui::Context) {
        if self.data.da_controllare.is_some() {
            let risposta = Dialogo::new(egui::Id::new("consenso_controllo"), CONTROLLARE)
                .testo(CONSENSO_CONTROLLO)
                .pulsante(NO, Peso::Testo)
                .pulsante(CONTROLLA, Peso::Principale)
                .mostra(ctx);
            if let Some(r) = risposta
                && let Some((f, posto)) = self.data.da_controllare.take()
            {
                if r == RispostaDialogo::Pulsante(1) {
                    diagnosi("data: consenso alla rete per il controllo");
                    self.controlla_la_data(ctx, f, posto);
                } else {
                    diagnosi("data: niente rete, controllo lasciato");
                }
            }
            return;
        }
        if let Some((l, titolo)) = self.data.lavoro.as_ref() {
            let testo = if *titolo == TIMESTAMP_CON_BLOCKCHAIN { "Mando l'impronta del file ai server" } else { "Chiedo la conferma ai server" };
            if mostra_barra(ctx, "lavoro_data", titolo, testo, l.frazione(), l.fermato()) {
                l.ferma();
            }
        }
        let contenuto = self.data.contenuto.clone();
        let titolo = match &contenuto {
            Some(Contenuto::Spiega(_)) | None => TIMESTAMP_CON_BLOCKCHAIN.to_string(),
            Some(Contenuto::Esito { titolo, .. }) => titolo.clone(),
        };
        let toccato = self
            .data
            .foglio
            .mostra(ctx, &titolo, |ui| {
                let font = Carattere::Testo.font(ui.ctx(), 16.0);
                match &contenuto {
                    Some(Contenuto::Spiega(_)) => {
                        for (i, p) in SPIEGAZIONE.iter().enumerate() {
                            if i > 0 {
                                ui.add_space(10.0);
                            }
                            ui.label(RichText::new(*p).font(font.clone()).color(tema::INCHIOSTRO));
                        }
                        ui.add_space(16.0);
                        let fissa = ui.add(Pulsante::new(CREA_IL_TIMESTAMP, Peso::Principale).icona(icone::SCHEDULE)).clicked();
                        ui.add_space(8.0);
                        fissa
                    }
                    Some(Contenuto::Esito { testo, .. }) => {
                        ui.label(RichText::new(testo.as_str()).font(font).color(tema::INCHIOSTRO));
                        ui.add_space(12.0);
                        false
                    }
                    None => false,
                }
            })
            .unwrap_or(false);
        if toccato && let Some(Contenuto::Spiega(f)) = contenuto {
            self.data.foglio.chiudi();
            self.data.contenuto = None;
            self.fissa_la_data(ctx, f);
        }
    }

    /// Per le prove: si sta fissando o controllando la data.
    #[doc(hidden)]
    pub fn data_in_corso(&self) -> bool {
        self.data.lavoro.is_some()
    }
}

/// Al più tanti byte letti da una prova .ots: quelle vere sono di qualche KB, una costruita apposta
/// può avere migliaia di rami (revisione del 31/10, punto 7).
const PROVA_MASSIMA: u64 = 64 * 1024;

fn leggi_la_prova(posto: &Path) -> Result<Vec<u8>, ErroreLavoro> {
    use std::io::Read;
    let mut v = Vec::new();
    std::fs::File::open(posto).and_then(|f| f.take(PROVA_MASSIMA + 1).read_to_end(&mut v)).map_err(|_| ErroreLavoro::Lettura)?;
    if v.len() as u64 > PROVA_MASSIMA {
        return Err(ErroreLavoro::Lettura);
    }
    Ok(v)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn i_posti_della_prova() {
        let radice = Path::new("/storage/emulated/0");
        let f = FileDelDocumento { chiave: "/storage/emulated/0/Download/Contratto.pdf".into(), nome: "Contratto.pdf".into() };
        assert_eq!(
            f.posti(radice),
            [
                PathBuf::from("/storage/emulated/0/Download/Contratto.pdf.ots"),
                PathBuf::from("/storage/emulated/0/Documents/Tascabile/Contratto.pdf.ots")
            ]
        );
        let u = FileDelDocumento { chiave: "content://com.whatsapp/x".into(), nome: "Referto.pdf".into() };
        assert_eq!(u.posti(radice), [PathBuf::from("/storage/emulated/0/Documents/Tascabile/Referto.pdf.ots")]);
        // Il nome mandato da un'altra app: solo il nome del file (revisione del 31/10, punto 7).
        for (nome, ots) in [("../../Download/Finto.pdf", "Finto.pdf.ots"), ("/storage/emulated/0/Download/Altro.pdf", "Altro.pdf.ots"), ("..", "documento.ots")] {
            let f = FileDelDocumento { chiave: "content://x/1".into(), nome: nome.into() };
            assert_eq!(f.posti(radice), [PathBuf::from("/storage/emulated/0/Documents/Tascabile").join(ots)], "{nome}");
        }
        assert_eq!(dove_sta(Path::new("/storage/emulated/0/Download/Contratto.pdf.ots"), radice), "Contratto.pdf.ots, accanto al file");
        assert_eq!(dove_sta(Path::new("/storage/emulated/0/Documents/Tascabile/Referto.pdf.ots"), radice), "Referto.pdf.ots, in Documents/Tascabile");
    }

    #[test]
    fn il_timestamp_di_un_file_rinominato() {
        use tascabile_prova_data::prova::{ESEMPIO_OTS, ESEMPIO_TXT};
        let radice = std::env::temp_dir().join(format!("tascabile-timestamp-rinominato-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&radice);
        let scaricati = radice.join("Download");
        let tascabile = tascabile_files::cartella_tascabile(&radice);
        std::fs::create_dir_all(&scaricati).unwrap();
        std::fs::create_dir_all(&tascabile).unwrap();
        let (vecchio, nuovo) = (scaricati.join("Saluto.txt"), scaricati.join("Ciao.txt"));
        std::fs::write(&nuovo, ESEMPIO_TXT).unwrap();

        // Nessun timestamp: niente da fare, e niente di nuovo sul disco.
        assert_eq!(rinomina_il_timestamp(&vecchio, &nuovo, &radice), TimestampRinominato::Nessuno);
        assert!(!scaricati.join("Ciao.txt.ots").exists());

        // Accanto al file: prende il nome nuovo, coi suoi byte.
        std::fs::write(scaricati.join("Saluto.txt.ots"), ESEMPIO_OTS).unwrap();
        assert_eq!(rinomina_il_timestamp(&vecchio, &nuovo, &radice), TimestampRinominato::Seguito);
        assert!(!scaricati.join("Saluto.txt.ots").exists());
        assert_eq!(std::fs::read(scaricati.join("Ciao.txt.ots")).unwrap(), ESEMPIO_OTS);

        // Il nome nuovo del timestamp è preso da un altro file: resta col nome di prima, e
        // l'altro non si tocca.
        std::fs::write(scaricati.join("Saluto.txt.ots"), ESEMPIO_OTS).unwrap();
        std::fs::write(scaricati.join("Ciao.txt.ots"), b"di un altro").unwrap();
        assert_eq!(rinomina_il_timestamp(&vecchio, &nuovo, &radice), TimestampRinominato::Rimasto);
        assert_eq!(std::fs::read(scaricati.join("Saluto.txt.ots")).unwrap(), ESEMPIO_OTS);
        assert_eq!(std::fs::read(scaricati.join("Ciao.txt.ots")).unwrap(), b"di un altro");
        std::fs::remove_file(scaricati.join("Saluto.txt.ots")).unwrap();
        std::fs::remove_file(scaricati.join("Ciao.txt.ots")).unwrap();

        // In Documents/Tascabile (dove va quando accanto al file non si scrive) il nome non basta:
        // segue il file solo se l'impronta scritta nella prova è quella dei suoi byte.
        std::fs::write(tascabile.join("Saluto.txt.ots"), ESEMPIO_OTS).unwrap();
        assert_eq!(rinomina_il_timestamp(&vecchio, &nuovo, &radice), TimestampRinominato::Seguito);
        assert!(!tascabile.join("Saluto.txt.ots").exists());
        assert_eq!(std::fs::read(tascabile.join("Ciao.txt.ots")).unwrap(), ESEMPIO_OTS);
        std::fs::remove_file(tascabile.join("Ciao.txt.ots")).unwrap();

        // La prova di un altro file con lo stesso nome, in un'altra cartella: resta dov'è.
        std::fs::write(tascabile.join("Saluto.txt.ots"), ESEMPIO_OTS).unwrap();
        std::fs::write(&nuovo, b"un altro file che si chiamava uguale").unwrap();
        assert_eq!(rinomina_il_timestamp(&vecchio, &nuovo, &radice), TimestampRinominato::Nessuno);
        assert_eq!(std::fs::read(tascabile.join("Saluto.txt.ots")).unwrap(), ESEMPIO_OTS);
        assert!(!tascabile.join("Ciao.txt.ots").exists());
        // Un file che non è una prova, con quel nome: resta dov'è.
        std::fs::write(tascabile.join("Saluto.txt.ots"), b"non una prova").unwrap();
        assert_eq!(rinomina_il_timestamp(&vecchio, &nuovo, &radice), TimestampRinominato::Nessuno);
        assert!(tascabile.join("Saluto.txt.ots").exists());
        let _ = std::fs::remove_dir_all(&radice);
    }

    #[test]
    fn la_prova_nuova_non_va_mai_sopra_un_file() {
        let d = std::env::temp_dir().join(format!("tascabile-prova-nuova-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let (accanto, ripiego) = (d.join("Contratto.pdf.ots"), d.join("Tascabile").join("Contratto.pdf.ots"));
        // Libero: va nel primo posto.
        assert_eq!(scrivi_nuova(&[accanto.clone(), ripiego.clone()], b"prima"), Some(accanto.clone()));
        assert_eq!(std::fs::read(&accanto).unwrap(), b"prima");
        // Preso: il primo non si tocca, va nel secondo (la cartella si crea).
        assert_eq!(scrivi_nuova(&[accanto.clone(), ripiego.clone()], b"seconda"), Some(ripiego.clone()));
        assert_eq!(std::fs::read(&accanto).unwrap(), b"prima");
        assert_eq!(std::fs::read(&ripiego).unwrap(), b"seconda");
        // Presi tutti e due: niente, e nessuno dei due cambia.
        assert_eq!(scrivi_nuova(&[accanto.clone(), ripiego.clone()], b"terza"), None);
        assert_eq!(std::fs::read(&accanto).unwrap(), b"prima");
        assert_eq!(std::fs::read(&ripiego).unwrap(), b"seconda");
        // Un collegamento rotto con quel nome: non si segue, il nome è preso.
        let rotto = d.join("Rotto.ots");
        std::os::unix::fs::symlink(d.join("altrove-che-non-c-e"), &rotto).unwrap();
        assert_eq!(scrivi_nuova(std::slice::from_ref(&rotto), b"quarta"), None);
        assert!(!d.join("altrove-che-non-c-e").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn la_risposta_della_piattaforma_per_la_prova() {
        assert_eq!(dalla_piattaforma(RispostaRete::Ok(vec![1])), Ok(vec![1]));
        assert_eq!(dalla_piattaforma(RispostaRete::Stato(404)), Err(ErroreRete::NonTrovato));
        assert_eq!(dalla_piattaforma(RispostaRete::Stato(500)), Err(ErroreRete::Risposta(500)));
        assert_eq!(dalla_piattaforma(RispostaRete::Collegamento), Err(ErroreRete::Collegamento));
    }
}
