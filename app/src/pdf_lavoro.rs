//! Il lavoro sul PDF (piano «PDF completo», P1): le modifiche mandate al motore sul suo thread,
//! annulla e ripeti, lo stato «Modifiche non salvate», Salva (sul file quando si può
//! scrivere, altrimenti una copia in Documents/Tascabile come in Word), Salva una copia e
//! Condividi il PDF modificato; l'avviso delle firme digitali prima della prima modifica che le
//! renderebbe non valide; la barra dei modi del bozzetto 7 e il foglio Strumenti del bozzetto 8,
//! con solo le voci che funzionano (gli altri modi e strumenti arrivano coi passaggi dopo: qui
//! c'è la loro struttura, spenta).
//!
//! Nella diagnosi solo generi, numeri e tempi: mai il nome del file, il percorso, le etichette
//! del motore (possono avere nomi di file) o il testo del documento.

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui;
use tascabile_files::{Recente, Tipo, cartella_tascabile, impronta};
use tascabile_pdf::{Evento, Fatto, GenereModifica, Istantanea, Modifica, Scritto, Scrittura, Toccate};
use tascabile_platform::diagnosi;
use tascabile_touch::tema::{self, Carattere};
use tascabile_touch::{Dialogo, GrigliaStrumenti, Peso, RispostaDialogo, Tono, etichetta_sezione, icone};
use tascabile_word::salva::{self, Come, ErroreSalva, Salvato};

use crate::schermo_pdf::{ANNULLA, PdfAperto};
use crate::{Da, Errore, NON_CONDIVISO, NON_SALVATO, TascabileApp, adesso, home, scrivi_esito};

/// Le voci del menu e i pulsanti della barra alta (per l'accessibilità e per le prove).
pub(crate) const SALVA: &str = "Salva";
pub(crate) const SALVA_COPIA: &str = "Salva una copia";
pub(crate) const ANNULLA_MODIFICA: &str = "Annulla modifica";
pub(crate) const RIPETI_MODIFICA: &str = "Ripeti modifica";
/// Lo stato sotto il nome, come nell'Editor.
pub(crate) const MODIFICHE_NON_SALVATE: &str = "Modifiche non salvate";
/// La voce in fondo alla barra dei modi e il titolo del suo foglio.
pub(crate) const STRUMENTI: &str = "Strumenti";
pub(crate) const STRUMENTI_PDF: &str = "Strumenti PDF";
/// L'avviso delle firme digitali.
pub(crate) const PDF_FIRMATO: &str = "PDF firmato";
pub(crate) const MODIFICA_ORIGINALE: &str = "Modifica l'originale";
pub(crate) const LAVORA_SU_COPIA: &str = "Lavora su una copia";
const FIRMA_SUL_FILE: &str = "Questo PDF ha una firma digitale: se lo modifichi, la firma risulterà non valida. Puoi lavorare su una copia e lasciare intatto l'originale firmato.";
const FIRMA_IN_COPIA: &str = "Questo PDF ha una firma digitale: se lo modifichi, la firma risulterà non valida. Le modifiche andranno in una copia in Documents/Tascabile: l'originale firmato resta com'è.";
/// Il dialogo di un PDF che si può solo leggere.
pub(crate) const SOLO_LETTURA: &str = "Solo lettura";
const CERTIFICATO: &str = "Chi ha firmato questo PDF l'ha certificato senza modifiche permesse: si può solo leggere.";
const PROTETTO: &str = "Questo PDF è protetto contro le modifiche: si può solo leggere.";
const FIRMA_ROVINATA: &str = "Questa modifica renderebbe non valida la firma digitale del PDF: non l'ho fatta.";

/// I modi della barra in fondo (bozzetto 7). Leggi e Compila (P2) funzionano; Annota e Modifica
/// arrivano coi passaggi dopo e fino ad allora non compaiono.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModoPdf {
    Leggi,
    Compila,
    Annota,
    Modifica,
}

impl ModoPdf {
    const TUTTI: [ModoPdf; 4] = [ModoPdf::Leggi, ModoPdf::Compila, ModoPdf::Annota, ModoPdf::Modifica];

    pub(crate) fn nome(self) -> &'static str {
        match self {
            ModoPdf::Leggi => "Leggi",
            ModoPdf::Compila => "Compila",
            ModoPdf::Annota => "Annota",
            ModoPdf::Modifica => "Modifica",
        }
    }

    pub(crate) fn icona(self) -> &'static str {
        match self {
            ModoPdf::Leggi => icone::CHROME_READER_MODE,
            ModoPdf::Compila => icone::EDIT_DOCUMENT,
            ModoPdf::Annota => icone::INK_HIGHLIGHTER,
            ModoPdf::Modifica => icone::EDIT_NOTE,
        }
    }

    /// Funziona in questa versione.
    fn fatto(self) -> bool {
        true
    }

    /// Cambia il documento: non c'è in un PDF che si può solo leggere.
    fn cambia(self) -> bool {
        self != ModoPdf::Leggi
    }
}

/// I modi da mostrare nella barra in fondo per questo documento.
pub(crate) fn modi_disponibili(stato: &Istantanea) -> Vec<ModoPdf> {
    let modificabile = si_modifica(stato);
    ModoPdf::TUTTI.into_iter().filter(|m| m.fatto() && (!m.cambia() || modificabile)).collect()
}

/// Il motore lo modifica e le firme non lo vietano.
pub(crate) fn si_modifica(stato: &Istantanea) -> bool {
    stato.modificabile && !stato.protezione.sola_lettura()
}

/// I gruppi del foglio Strumenti, nell'ordine del bozzetto 8, più quello per orientarsi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Gruppo {
    Modifica,
    CompilaEAnnota,
    PagineEFile,
    Leggi,
}

impl Gruppo {
    const TUTTI: [Gruppo; 4] = [Gruppo::Modifica, Gruppo::CompilaEAnnota, Gruppo::PagineEFile, Gruppo::Leggi];

    fn nome(self) -> &'static str {
        match self {
            Gruppo::Modifica => "Modifica",
            Gruppo::CompilaEAnnota => "Compila e annota",
            Gruppo::PagineEFile => "Pagine e file",
            Gruppo::Leggi => "Leggi",
        }
    }

    fn tono(self) -> Tono {
        match self {
            Gruppo::Modifica => Tono::Rosso,
            Gruppo::CompilaEAnnota => Tono::Blu,
            Gruppo::PagineEFile | Gruppo::Leggi => Tono::Neutro,
        }
    }
}

/// Gli strumenti del foglio (bozzetto 8). Compaiono solo quelli che funzionano: gli altri si
/// accendono coi passaggi del piano.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Strumento {
    ModificaTesto,
    AggiungiTesto,
    Immagine,
    RiconosciTesto,
    Forme,
    Link,
    Firma,
    Evidenzia,
    Penna,
    Nota,
    OrganizzaPagine,
    Unisci,
    Dividi,
    Comprimi,
    EsportaWord,
    Proteggi,
    TogliProtezione,
    Oscura,
    Filigrana,
    NumeriDiPagina,
    Scansiona,
    Miniature,
    Indice,
    VaiAPagina,
}

impl Strumento {
    const TUTTI: [Strumento; 24] = [
        Strumento::ModificaTesto,
        Strumento::AggiungiTesto,
        Strumento::Immagine,
        Strumento::RiconosciTesto,
        Strumento::Forme,
        Strumento::Link,
        Strumento::Firma,
        Strumento::Evidenzia,
        Strumento::Penna,
        Strumento::Nota,
        Strumento::OrganizzaPagine,
        Strumento::Unisci,
        Strumento::Dividi,
        Strumento::Comprimi,
        Strumento::EsportaWord,
        Strumento::Proteggi,
        Strumento::TogliProtezione,
        Strumento::Oscura,
        Strumento::Filigrana,
        Strumento::NumeriDiPagina,
        Strumento::Scansiona,
        Strumento::Miniature,
        Strumento::Indice,
        Strumento::VaiAPagina,
    ];

    fn gruppo(self) -> Gruppo {
        use Strumento::*;
        match self {
            ModificaTesto | AggiungiTesto | Immagine | RiconosciTesto | Forme | Link => Gruppo::Modifica,
            Firma | Evidenzia | Penna | Nota => Gruppo::CompilaEAnnota,
            OrganizzaPagine | Unisci | Dividi | Comprimi | EsportaWord | Proteggi | TogliProtezione | Oscura | Filigrana | NumeriDiPagina
            | Scansiona => Gruppo::PagineEFile,
            Miniature | Indice | VaiAPagina => Gruppo::Leggi,
        }
    }

    pub(crate) fn nome(self) -> &'static str {
        use Strumento::*;
        match self {
            ModificaTesto => "Modifica testo",
            AggiungiTesto => "Aggiungi testo",
            Immagine => "Immagine",
            RiconosciTesto => "Riconosci testo",
            Forme => "Forme",
            Link => "Link",
            Firma => "Firma",
            Evidenzia => "Evidenzia",
            Penna => "Penna",
            Nota => "Nota",
            OrganizzaPagine => "Organizza pagine",
            Unisci => crate::pdf_pagine::UNISCI,
            Dividi => crate::pdf_pagine::DIVIDI,
            Comprimi => crate::pdf_pagine::COMPRIMI,
            EsportaWord => "Esporta in Word",
            Proteggi => crate::pdf_pagine::PROTEGGI,
            TogliProtezione => "Togli la protezione",
            Oscura => "Oscura",
            Filigrana => "Filigrana",
            NumeriDiPagina => "Numeri di pagina",
            Scansiona => "Scansiona",
            Miniature => crate::schermo_pdf::MINIATURE,
            Indice => crate::schermo_pdf::INDICE,
            VaiAPagina => crate::schermo_pdf::VAI_A_PAGINA,
        }
    }

    fn icona(self) -> &'static str {
        use Strumento::*;
        match self {
            ModificaTesto => icone::EDIT_NOTE,
            AggiungiTesto => icone::TEXT_FIELDS,
            Immagine => icone::ADD_PHOTO_ALTERNATE,
            RiconosciTesto => icone::DOCUMENT_SCANNER,
            Forme => icone::SHAPES,
            Link => icone::ADD_LINK,
            Firma => icone::SIGNATURE,
            Evidenzia => icone::INK_HIGHLIGHTER,
            Penna => icone::DRAW,
            Nota => icone::STICKY_NOTE_2,
            OrganizzaPagine => icone::VIEW_MODULE,
            Unisci => icone::MERGE,
            Dividi => icone::CALL_SPLIT,
            Comprimi => icone::COMPRESS,
            EsportaWord => icone::DESCRIPTION,
            Proteggi => icone::LOCK,
            TogliProtezione => icone::LOCK_OPEN,
            Oscura => icone::VISIBILITY_OFF,
            Filigrana => icone::BRANDING_WATERMARK,
            NumeriDiPagina => icone::FORMAT_LIST_NUMBERED,
            Scansiona => icone::PHOTO_CAMERA,
            Miniature => icone::GRID_VIEW,
            Indice => icone::TOC,
            VaiAPagina => icone::NUMBERS,
        }
    }

    /// Funziona in questa versione.
    fn fatto(self) -> bool {
        use Strumento::*;
        matches!(
            self,
            ModificaTesto
                | AggiungiTesto
                | Immagine
                | RiconosciTesto
                | Forme
                | Link
                | Firma
                | Evidenzia
                | Penna
                | Nota
                | OrganizzaPagine
                | Unisci
                | Dividi
                | Comprimi
                | Proteggi
                | TogliProtezione
                | EsportaWord
                | Oscura
                | Filigrana
                | NumeriDiPagina
                | Scansiona
                | Miniature
                | Indice
                | VaiAPagina
        )
    }

    /// Cambia il documento: non c'è in un PDF che si può solo leggere.
    fn cambia(self) -> bool {
        // Esporta in Word e Scansiona fanno un file nuovo: ci sono anche nei PDF da solo leggere.
        self.gruppo() != Gruppo::Leggi && !matches!(self, Strumento::EsportaWord | Strumento::Scansiona)
    }
}

impl Strumento {
    /// C'è per questo documento: Proteggi senza password e Togli la protezione con; sui firmati
    /// niente che riscriva tutto il file (comprimere, la password), che renderebbe non valide le
    /// firme e il motore rifiuta.
    fn per(self, stato: &Istantanea) -> bool {
        let firmato = stato.protezione.firmato();
        match self {
            Strumento::Proteggi => !stato.cifrato && !firmato,
            Strumento::TogliProtezione => stato.cifrato && !firmato,
            Strumento::Comprimi => !firmato,
            _ => true,
        }
    }
}

/// Gli strumenti da mostrare per questo documento, gruppo per gruppo (i gruppi vuoti no).
pub(crate) fn strumenti_disponibili(stato: &Istantanea) -> Vec<(&'static str, Tono, Vec<Strumento>)> {
    let modificabile = si_modifica(stato);
    Gruppo::TUTTI
        .into_iter()
        .filter_map(|g| {
            let voci: Vec<Strumento> =
                Strumento::TUTTI.into_iter().filter(|s| s.gruppo() == g && s.fatto() && (!s.cambia() || modificabile) && s.per(stato)).collect();
            (!voci.is_empty()).then(|| (g.nome(), g.tono(), voci))
        })
        .collect()
}

/// Che cosa scrive un salvataggio del PDF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AzioneSalva {
    /// Sul file aperto se si può scrivere, altrimenti una copia in Documents/Tascabile.
    Salva,
    /// Una copia in Documents/Tascabile, che da lì è il documento.
    SalvaCopia,
    /// I byte di adesso, con le modifiche, al foglio di condivisione: il documento resta da
    /// salvare com'era.
    Condividi,
    /// La copia definitiva (moduli, note, disegni e firme fissati nelle pagine) in
    /// Documents/Tascabile: il documento resta com'era.
    Definitiva,
}

impl AzioneSalva {
    /// Per la diagnosi.
    fn etichetta(self) -> &'static str {
        match self {
            AzioneSalva::Salva => "salva pdf",
            AzioneSalva::SalvaCopia => "salva una copia pdf",
            AzioneSalva::Condividi => "condividi pdf aperto",
            AzioneSalva::Definitiva => "copia definitiva pdf",
        }
    }
}

/// Com'è andata una scrittura del PDF, sul thread del motore: l'esito e l'impronta dei byte
/// scritti (la chiave nuova della ripresa).
pub(crate) struct ScrittoPdf {
    azione: AzioneSalva,
    esito: Result<Salvato, ErroreSalva>,
    impronta: Option<String>,
    /// L'impronta dei byte scritti per i recenti (SHA-256): con lei il file si ritrova se
    /// un'altra app lo sposta dopo il salvataggio (revisione del 10/11).
    impronta_file: Option<tascabile_files::ImprontaFile>,
}

impl Scritto for ScrittoPdf {
    fn scritto(&self) -> bool {
        self.esito.is_ok()
    }
}

/// Dove scrive un salvataggio del PDF.
enum Destino {
    /// Sul file aperto, sostituito tutto o niente; se il telefono non lo lascia scrivere, una
    /// copia in Documents/Tascabile.
    File { percorso: PathBuf, radice: PathBuf, base: String },
    /// Un file nuovo in Documents/Tascabile, che non sostituisce niente.
    Copia { radice: PathBuf, base: String },
    /// In `condivisi/` della cache, per il foglio di condivisione.
    Condivisi { cache: PathBuf, nome: String },
}

fn da_io(e: std::io::Error) -> ErroreSalva {
    ErroreSalva { genere: format!("{:?}", e.kind()), messaggio: e.to_string() }
}

/// Una copia nuova in Documents/Tascabile sotto `radice` (creata se manca).
fn copia_in_documenti(radice: &std::path::Path, base: &str, byte: &[u8]) -> Result<Salvato, ErroreSalva> {
    let cartella = cartella_tascabile(radice);
    std::fs::create_dir_all(&cartella).map_err(da_io)?;
    let percorso = salva::scrivi_nuovo(&cartella, base, "pdf", byte).map_err(da_io)?;
    Ok(Salvato { percorso, byte: byte.len() as u64, tipo: "pdf".into(), come: Come::Copia })
}

fn scrivi_pdf(destino: Destino, byte: &[u8]) -> Result<Salvato, ErroreSalva> {
    match destino {
        Destino::File { percorso, radice, base } => match salva::scrivi_atomico(&percorso, byte) {
            Ok(()) => Ok(Salvato { percorso, byte: byte.len() as u64, tipo: "pdf".into(), come: Come::SulFile }),
            Err(e) if matches!(e.kind(), std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::ReadOnlyFilesystem) => {
                copia_in_documenti(&radice, &base, byte)
            }
            Err(e) => Err(da_io(e)),
        },
        Destino::Copia { radice, base } => copia_in_documenti(&radice, &base, byte),
        Destino::Condivisi { cache, nome } => salva::byte_in_condivisi(&nome, byte, &cache),
    }
}

/// La base del nome di una copia: il nome del PDF senza estensione; per un PDF arrivato senza
/// nome («documento.pdf») «Documento» con data e ora.
fn base_del_nome(nome: &str) -> String {
    if nome == "documento.pdf" {
        return salva::nome_di_ripiego_adesso();
    }
    home::nome_da_mostrare(nome, Tipo::Pdf).to_string()
}

/// Il nome con cui si condivide: in .pdf anche se il file aveva un altro nome.
fn nome_da_condividere(nome: &str) -> String {
    if tascabile_files::tipo_da_nome(nome) == Tipo::Pdf { nome.to_string() } else { format!("{}.pdf", home::nome_da_mostrare(nome, Tipo::Pdf)) }
}

/// Dove va il salvataggio quando non scrive sul file aperto (un PDF arrivato da un'altra app, una
/// copia scelta): Oscura e Proteggi lo dicono prima, perché l'originale resta com'era
/// e chi lo inoltra manda quello (revisione del 31/10, punto 11).
pub(crate) const IN_UNA_COPIA: &str = "Il salvataggio va in una copia in Documents/Tascabile: l'originale resta com'era.";

/// Il testo di un dialogo, con la frase sulla copia se Salva non scrive sul file aperto.
pub(crate) fn con_la_copia(testo: &str, p: &PdfAperto) -> String {
    if salva_sul_file(p).is_some() { testo.to_string() } else { format!("{testo} {IN_UNA_COPIA}") }
}

/// Salva scrive sul file aperto: è un file della memoria del telefono (non arrivato da un'altra
/// app) e non si è scelto di lavorare su una copia.
fn salva_sul_file(p: &PdfAperto) -> Option<PathBuf> {
    match (&p.chiave, p.da) {
        (Some(c), Da::Cartella | Da::Recenti) if !home::e_uri(c) && !p.su_copia => Some(PathBuf::from(c)),
        _ => None,
    }
}

impl TascabileApp {
    /// Annulla nella barra alta.
    pub(crate) fn annulla_pdf(&mut self) {
        if let Some(p) = self.pdf.as_mut()
            && p.motore.annulla()
        {
            diagnosi("pdf: annulla chiesto");
        }
    }

    /// Ripeti nella barra alta.
    pub(crate) fn ripeti_pdf(&mut self) {
        if let Some(p) = self.pdf.as_mut()
            && p.motore.ripeti()
        {
            diagnosi("pdf: ripeti chiesto");
        }
    }

    /// Un modo della barra in fondo. Lasciando Compila la scrittura si conferma, e la proposta e
    /// il segno scelto si chiudono; entrando, se il modulo ha calcoli veri, lo si dice.
    pub(crate) fn scegli_modo_pdf(&mut self, modo: ModoPdf) {
        if modo != ModoPdf::Compila {
            self.conferma_scrittura_pdf(true);
        }
        let Some(p) = self.pdf.as_mut() else { return };
        if p.modo != modo {
            p.modo = modo;
            p.compila.proposta = None;
            p.compila.segno = None;
            p.compila.trascina = None;
            p.modifica.chiudi();
            p.annota.chiudi();
            diagnosi(format!("pdf: modo {}", modo.nome().to_lowercase()));
        }
        self.controlla_i_calcoli_pdf();
    }

    /// «Strumenti» nella barra in fondo: il foglio del bozzetto 8.
    pub(crate) fn apri_strumenti_pdf(&mut self) {
        let Some(p) = self.pdf.as_ref() else { return };
        let voci: usize = strumenti_disponibili(p.motore.stato()).iter().map(|(_, _, v)| v.len()).sum();
        diagnosi(format!("pdf: strumenti aperti, {voci} voci"));
        self.fogli_pdf.strumenti.apri();
    }

    /// Il foglio «Strumenti PDF»: i gruppi con le voci che funzionano e, per un PDF che si può
    /// solo leggere, perché. Un tocco chiude il foglio e fa lo strumento.
    pub(crate) fn mostra_foglio_strumenti_pdf(&mut self, ctx: &egui::Context) {
        let (gruppi, perche) = match self.pdf.as_ref() {
            Some(p) => {
                let stato = p.motore.stato();
                let perche = if stato.protezione.sola_lettura() {
                    Some(CERTIFICATO)
                } else if !stato.modificabile {
                    Some(PROTETTO)
                } else {
                    None
                };
                (strumenti_disponibili(stato), perche)
            }
            None => (Vec::new(), None),
        };
        let scelto = self
            .fogli_pdf
            .strumenti
            .mostra(ctx, STRUMENTI_PDF, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let mut scelto = None;
                for (nome, tono, voci) in &gruppi {
                    etichetta_sezione(ui, nome);
                    ui.add_space(10.0);
                    let mut griglia = GrigliaStrumenti::new();
                    for s in voci {
                        griglia = griglia.voce(s.icona(), s.nome(), *tono);
                    }
                    if let Some(i) = griglia.mostra(ui) {
                        scelto = Some(voci[i]);
                    }
                    ui.add_space(8.0);
                }
                if let Some(perche) = perche {
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new(perche).font(Carattere::Testo.font(ui.ctx(), 15.0)).color(tema::MATITA));
                    ui.add_space(8.0);
                }
                scelto
            })
            .flatten();
        let Some(s) = scelto else { return };
        self.fogli_pdf.strumenti.chiudi();
        diagnosi(format!("pdf: strumento {}", s.nome().to_lowercase()));
        // Gli strumenti che fanno un altro file dal documento com'è: prima l'oscuramento in sospeso.
        if matches!(s, Strumento::Unisci | Strumento::Dividi | Strumento::Comprimi | Strumento::EsportaWord) && self.oscura_da_applicare() {
            return;
        }
        match s {
            Strumento::Firma => self.apri_firma_pdf(crate::pdf_firma::Dove::DaToccare),
            Strumento::OrganizzaPagine => self.apri_organizza_pdf(),
            Strumento::Unisci => self.apri_unisci_pdf(None),
            Strumento::Dividi => self.apri_dividi_pdf(),
            Strumento::Comprimi => self.apri_comprimi_pdf(),
            Strumento::Proteggi => self.apri_proteggi_pdf(),
            Strumento::TogliProtezione => self.chiedi_togli_protezione_pdf(),
            Strumento::Miniature => self.apri_miniature(),
            Strumento::Indice => self.apri_indice_pdf(),
            Strumento::VaiAPagina => self.apri_vai_a_pagina(),
            Strumento::RiconosciTesto => self.apri_riconosci_pdf(),
            Strumento::EsportaWord => self.esporta_word_pdf(ctx),
            Strumento::Scansiona => self.apri_scansione(),
            Strumento::ModificaTesto
            | Strumento::AggiungiTesto
            | Strumento::Immagine
            | Strumento::Forme
            | Strumento::Link
            | Strumento::Oscura
            | Strumento::Filigrana
            | Strumento::NumeriDiPagina => self.strumento_modifica_pdf(s),
            Strumento::Evidenzia | Strumento::Penna | Strumento::Nota => self.strumento_annota_pdf(s),
        }
    }

    /// Manda una modifica al motore. Un PDF che si può solo leggere lo dice; su un PDF firmato
    /// la prima modifica che renderebbe non valida una firma aspetta la risposta all'avviso;
    /// senza quel sì, una modifica che la rovinasse lo stesso (il motore lo dice dopo) si annulla.
    pub(crate) fn modifica_pdf(&mut self, m: Modifica) {
        let Some(p) = self.pdf.as_mut() else { return };
        let stato = p.motore.stato();
        if !si_modifica(stato) {
            let testo = if stato.protezione.sola_lettura() { CERTIFICATO } else { PROTETTO };
            diagnosi(format!("pdf: modifica ({}) in un pdf da solo leggere", m.genere.nome()));
            self.errore = Some(Errore { titolo: SOLO_LETTURA, testo: testo.to_string() });
            return;
        }
        if stato.protezione.invalida(m.genere) && !p.firme_accettate {
            diagnosi(format!("pdf: modifica ({}) in un pdf firmato, si avvisa", m.genere.nome()));
            p.in_attesa_firme = Some(m);
            return;
        }
        let genere = m.genere;
        if !p.motore.applica(m, !p.firme_accettate) {
            self.motore_fermo();
            return;
        }
        diagnosi(format!("pdf: modifica ({}) chiesta", genere.nome()));
    }

    /// L'avviso delle firme digitali, se una modifica lo aspetta: la firma risulterà non valida;
    /// si propone di lavorare su una copia (e, se Salva scriverebbe sull'originale, di
    /// modificare quello). Un tocco fuori o Indietro valgono Annulla.
    pub(crate) fn mostra_firme_pdf(&mut self, ctx: &egui::Context) {
        let Some(p) = self.pdf.as_ref() else { return };
        if p.in_attesa_firme.is_none() {
            return;
        }
        let sul_file = salva_sul_file(p).is_some();
        let mut d = Dialogo::new(egui::Id::new("pdf_firmato"), PDF_FIRMATO)
            .testo(if sul_file { FIRMA_SUL_FILE } else { FIRMA_IN_COPIA })
            .pulsante(ANNULLA, Peso::Testo);
        if sul_file {
            d = d.pulsante(MODIFICA_ORIGINALE, Peso::Testo);
        }
        let risposta = d.pulsante(LAVORA_SU_COPIA, Peso::Principale).mostra(ctx);
        match risposta {
            None => {}
            Some(RispostaDialogo::Pulsante(0) | RispostaDialogo::Fuori) => {
                self.annulla_firme_pdf();
            }
            Some(RispostaDialogo::Pulsante(1)) if sul_file => self.accetta_firme_pdf(false),
            Some(RispostaDialogo::Pulsante(_)) => self.accetta_firme_pdf(true),
        }
    }

    /// Annulla nell'avviso delle firme (o Indietro): la modifica non si fa.
    pub(crate) fn annulla_firme_pdf(&mut self) -> bool {
        let Some(p) = self.pdf.as_mut() else { return false };
        if p.in_attesa_firme.take().is_none() {
            return false;
        }
        diagnosi("pdf: firme, modifica annullata");
        true
    }

    /// Sì all'avviso delle firme: la modifica si fa, e le prossime non chiedono più; con
    /// `copia` Salva scriverà una copia in Documents/Tascabile e mai sull'originale.
    fn accetta_firme_pdf(&mut self, copia: bool) {
        let Some(p) = self.pdf.as_mut() else { return };
        let Some(m) = p.in_attesa_firme.take() else { return };
        p.firme_accettate = true;
        p.su_copia |= copia;
        diagnosi(if copia { "pdf: firme, si lavora su una copia" } else { "pdf: firme, si modifica l'originale" });
        let genere = m.genere;
        if !p.motore.applica(m, false) {
            self.motore_fermo();
            return;
        }
        diagnosi(format!("pdf: modifica ({}) chiesta", genere.nome()));
        if copia {
            self.avviso_breve.avvisa("Le modifiche andranno in una copia: l'originale firmato resta com'è.");
        }
    }

    /// Salva, Salva una copia o Condividi: i byte li prepara il motore sul suo thread e li scrive
    /// lì; l'esito arriva con `passo_motore_pdf`. Con `dopo` (dal dialogo «Salvare le
    /// modifiche?») si fa quello se il salvataggio riesce. Un salvataggio già in corso non se ne
    /// prende un altro: si aspetta lui.
    pub(crate) fn salva_pdf(&mut self, azione: AzioneSalva, dopo: Option<crate::Dopo>) {
        // Coi rettangoli di Oscura non ancora applicati niente esce: nel file il testo c'è ancora.
        if self.oscura_da_applicare() {
            return;
        }
        let radice = self.piattaforma.radice_archivio();
        let cache = self.piattaforma.cartella_cache();
        let Some(p) = self.pdf.as_mut() else { return };
        if p.motore.scrive() {
            if dopo.is_some() {
                p.dopo_il_salvataggio = dopo;
            }
            diagnosi(format!("{}: un salvataggio è già in corso, si aspetta", azione.etichetta()));
            return;
        }
        let base = base_del_nome(&p.nome);
        let destino = match azione {
            AzioneSalva::Salva => match salva_sul_file(p) {
                Some(percorso) => Destino::File { percorso, radice, base },
                None => Destino::Copia { radice, base },
            },
            AzioneSalva::SalvaCopia => Destino::Copia { radice, base },
            AzioneSalva::Condividi => Destino::Condivisi { cache, nome: nome_da_condividere(&p.nome) },
            AzioneSalva::Definitiva => Destino::Copia { radice, base: format!("{base} definitivo") },
        };
        let scrittura: Scrittura<ScrittoPdf> = Box::new(move |byte: &Arc<Vec<u8>>| {
            let esito = scrivi_pdf(destino, byte);
            let salvato = esito.is_ok() && !matches!(azione, AzioneSalva::Condividi | AzioneSalva::Definitiva);
            let impronta = salvato.then(|| impronta(byte));
            let impronta_file = salvato.then(|| tascabile_files::ImprontaFile::di(byte));
            ScrittoPdf { azione, esito, impronta, impronta_file }
        });
        let mandato = match azione {
            AzioneSalva::Definitiva => p.motore.appiattita(scrittura),
            _ => p.motore.scrivi(scrittura, azione != AzioneSalva::Condividi),
        };
        if !mandato {
            self.motore_fermo();
            return;
        }
        p.dopo_il_salvataggio = dopo;
        diagnosi(format!("{}: chiesto", azione.etichetta()));
    }

    /// Il thread del motore si è fermato: niente più modifiche né salvataggi.
    pub(crate) fn motore_fermo(&mut self) {
        diagnosi("pdf: il motore si è fermato");
        self.errore = Some(Errore {
            titolo: NON_SALVATO,
            testo: "Il motore dei PDF si è fermato con un errore interno: le modifiche non salvate non si possono più salvare. Riapri il PDF."
                .to_string(),
        });
    }

    /// A ogni passo: le risposte del motore (modifiche, annulla e ripeti, salvataggi), coi loro
    /// effetti sul lettore, sullo schermo e nella diagnosi.
    pub(crate) fn passo_motore_pdf(&mut self) {
        let Some(p) = self.pdf.as_mut() else { return };
        for f in p.motore.ricevi() {
            self.dopo_il_motore(f);
        }
    }

    /// Per le prove: aspetta che il motore del PDF abbia finito i comandi mandati, al massimo
    /// 20 s, e ne prende le risposte.
    #[doc(hidden)]
    pub fn aspetta_il_motore_pdf(&mut self) {
        let Some(p) = self.pdf.as_mut() else { return };
        for f in p.motore.aspetta(std::time::Duration::from_secs(20)) {
            self.dopo_il_motore(f);
        }
    }

    /// Per le prove: il PDF del lettore ha modifiche non salvate (None senza PDF).
    #[doc(hidden)]
    pub fn pdf_modificato(&self) -> Option<bool> {
        self.pdf.as_ref().map(|p| p.motore.stato().modificato)
    }

    /// Per le prove a schermo: apre il PDF di `percorso` come dalle cartelle della Home.
    #[doc(hidden)]
    pub fn apri_pdf_dalla_cartella(&mut self, percorso: &std::path::Path) {
        let nome = percorso.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        self.apri_file(percorso, &nome);
    }

    /// Per le prove a schermo: una modifica qualunque del motore (un'annotazione di prova), come
    /// la manderebbe uno strumento.
    #[doc(hidden)]
    pub fn modifica_pdf_di_prova(&mut self, m: Modifica) {
        self.modifica_pdf(m);
    }

    fn dopo_il_motore(&mut self, f: Fatto<ScrittoPdf>) {
        let Some(p) = self.pdf.as_mut() else { return };
        let ms = f.ms;
        let toccate = match &f.evento {
            Evento::Applicata { toccate, .. } | Evento::Annullata { toccate } | Evento::Ripetuta { toccate } => toccate.clone(),
            _ => Toccate::Nessuna,
        };
        // I byte da disegnare possono essere nuovi anche senza pagine toccate (un salvataggio, una
        // modifica rifiutata e tolta).
        p.lettore.aggiorna(p.motore.stato(), &toccate);
        let compila = match &f.evento {
            Evento::Applicata { .. } => Some(Some(true)),
            Evento::Rifiutata { .. } | Evento::NonApplicata { .. } => Some(Some(false)),
            Evento::Annullata { .. } | Evento::Ripetuta { .. } => Some(None),
            _ => None,
        };
        if let Some(c) = compila {
            self.compila_dopo_il_motore(c);
        }
        self.modifica_dopo_il_motore(&f.evento);
        self.annota_dopo_il_motore(&f.evento);
        if matches!(f.evento, Evento::Applicata { .. } | Evento::Annullata { .. } | Evento::Ripetuta { .. }) {
            self.modifica_da_copiare();
        }
        let pagine = |t: &Toccate| t.quante().map_or_else(|| "tutte le pagine".to_string(), |n| format!("{n} pagine"));
        match f.evento {
            Evento::Applicata { genere, toccate, rovinate } => {
                if let Some(p) = self.pdf.as_mut() {
                    p.modifica.correzione = None;
                }
                let firme = if rovinate > 0 { format!(", {rovinate} firme non più valide") } else { String::new() };
                diagnosi(format!("pdf: modifica ({}) applicata in {ms} ms, {} toccate{firme}", genere.nome(), pagine(&toccate)));
            }
            Evento::Rifiutata { genere, rovinate } => {
                diagnosi(format!("pdf: modifica ({}) tolta, renderebbe non valide {rovinate} firme", genere.nome()));
                self.errore = Some(Errore { titolo: PDF_FIRMATO, testo: FIRMA_ROVINATA.to_string() });
            }
            Evento::NonApplicata { genere, errore } => {
                diagnosi(format!("pdf: modifica ({}) non applicata ({errore})", genere.nome()));
                self.errore = Some(Errore { titolo: "Modifica non riuscita", testo: messaggio_della_modifica(genere, errore).to_string() });
                // Una correzione della scansione: la scrittura si riapre col testo scritto, da
                // accorciare, invece di riscriverlo tutto.
                if let Some((chiave, testo)) = self.pdf.as_mut().and_then(|p| p.modifica.correzione.take())
                    && let Some(p) = self.pdf.as_mut()
                    && p.modifica.oggetto(chiave).is_some()
                {
                    diagnosi("pdf: correzione della scansione riaperta col testo scritto");
                    p.modifica.scelto = Some(chiave);
                    self.apri_scrittura_pdf(crate::pdf_compila::Bersaglio::Oggetto(chiave), &testo, false);
                }
            }
            Evento::Annullata { toccate } => diagnosi(format!("pdf: annullato in {ms} ms, {} toccate", pagine(&toccate))),
            Evento::Ripetuta { toccate } => diagnosi(format!("pdf: ripetuto in {ms} ms, {} toccate", pagine(&toccate))),
            Evento::Niente => diagnosi("pdf: niente da annullare o da ripetere"),
            Evento::NonAnnullata { errore } => {
                diagnosi(format!("pdf: annulla o ripeti non riuscito ({errore})"));
                self.errore = Some(Errore {
                    titolo: "Modifica non riuscita",
                    testo: "Non riesco ad annullare o ripetere: il PDF è rimasto com'era.".to_string(),
                });
            }
            Evento::Scritta { esito, segnata, byte } => self.scritto_pdf(esito, segnata, byte, ms),
            Evento::NonScritta { errore } => {
                diagnosi(format!("pdf: il motore non ha preparato i byte da salvare ({errore})"));
                if let Some(p) = self.pdf.as_mut() {
                    p.dopo_il_salvataggio = None;
                }
                self.errore =
                    Some(Errore { titolo: NON_SALVATO, testo: "Non riesco a salvare: il lettore dei PDF non ha preparato il file.".to_string() });
            }
            Evento::Copiata { esito } => self.copia_pdf_finita(esito, ms),
            Evento::Javascript { acceso } => diagnosi(if acceso { "pdf: calcoli del modulo accesi" } else { "pdf: calcoli del modulo spenti" }),
            Evento::CambiaCarattere { .. } | Evento::Oggetti { .. } => {}
            Evento::Fermo => self.motore_fermo(),
        }
    }

    /// Una scrittura finita: condividere, oppure dire dove è stato salvato (una copia diventa il
    /// documento, nella barra alta e nei recenti), e poi fare quello che aspettava il salvataggio.
    fn scritto_pdf(&mut self, s: ScrittoPdf, segnata: bool, byte: usize, ms: u32) {
        scrivi_esito(s.azione.etichetta(), "pdf", &s.esito);
        let r = match s.esito {
            Ok(r) => r,
            Err(e) => {
                if let Some(p) = self.pdf.as_mut() {
                    p.dopo_il_salvataggio = None;
                }
                self.errore = Some(match s.azione {
                    AzioneSalva::Condividi => Errore { titolo: NON_CONDIVISO, testo: format!("Non riesco a condividere il PDF: {e}") },
                    AzioneSalva::Definitiva => Errore { titolo: NON_SALVATO, testo: format!("Non riesco a fare la copia definitiva: {e}") },
                    _ => Errore { titolo: NON_SALVATO, testo: format!("Non riesco a salvare: {e}") },
                });
                return;
            }
        };
        if s.azione == AzioneSalva::Condividi {
            self.piattaforma.condividi_file(&r.percorso, "application/pdf", "Condividi il PDF");
            return;
        }
        if s.azione == AzioneSalva::Definitiva {
            // Una copia per chi la riceve: il documento aperto resta quello di prima. Dov'è lo dice
            // un dialogo che resta, con «Condividi» (10/11): l'avviso breve spariva in tre secondi.
            diagnosi(format!("pdf: copia definitiva in {ms} ms, {byte} byte"));
            if let Some(p) = self.pdf.as_mut() {
                p.pagine.definitiva = Some(r.percorso);
            }
            return;
        }
        diagnosi(format!("pdf: salvato in {ms} ms, {byte} byte{}", if segnata { "" } else { ", ma il motore non l'ha preso come base" }));
        let Some(p) = self.pdf.as_mut() else { return };
        if segnata {
            p.salvato = true;
            // Un PDF recuperato, salvato, non ha più niente da salvare: la sua copia se ne va.
            p.copia.recuperato = false;
            if let Some(i) = s.impronta {
                // Il contenuto è cambiato: la ripresa ha una chiave nuova.
                p.impronta = i;
                p.ricordata = None;
            }
        }
        let file = r.percorso.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if r.come == Come::Copia {
            // La copia diventa il documento: Salva dopo scrive su di lei.
            let prima = p.chiave.take();
            p.nome = file.clone();
            p.da = Da::Cartella;
            p.su_copia = false;
            p.chiave = r.percorso.to_str().map(str::to_string);
            let pagine = p.lettore.pagine();
            if let Some(c) = prima.filter(|c| home::e_uri(c)) {
                self.home.dimentica(&c);
            }
            match r.percorso.to_str() {
                Some(chiave) => self.home.ricorda(Recente {
                    chiave: chiave.to_string(),
                    nome: file.clone(),
                    tipo: Tipo::Pdf,
                    aperto_alle: adesso(),
                    pagine: Some(pagine),
                    impronta: s.impronta_file,
                }),
                None => diagnosi("recenti: percorso che non è UTF-8, non ricordato"),
            }
        } else if let (Some(chiave), Some(impronta)) = (r.percorso.to_str(), s.impronta_file) {
            // Scritto sul file aperto: il suo recente prende l'impronta dei byte nuovi.
            self.home.impronta_salvata(chiave, impronta);
        }
        // Un file nuovo: l'avviso ha «Condividi» (10/11).
        match (s.azione, r.come) {
            (_, Come::SulFile) => self.avviso_breve.avvisa("Salvato"),
            (AzioneSalva::SalvaCopia, _) => self.avvisa_col_condividi(format!("Copia salvata in Documents/Tascabile: {file}"), &r.percorso),
            _ => self.avvisa_col_condividi(format!("Salvato in Documents/Tascabile: {file}"), &r.percorso),
        }
        if let Some(dopo) = self.pdf.as_mut().and_then(|p| p.dopo_il_salvataggio.take()) {
            self.fai(dopo);
        }
    }
}

/// Il messaggio di una modifica che il motore non ha fatto, dal genere dell'errore.
fn messaggio_della_modifica(genere: GenereModifica, errore: &str) -> &'static str {
    match errore {
        "sola lettura" => PROTETTO,
        "permessi del PDF" => "Chi ha creato il PDF non permette questa modifica: il PDF è rimasto com'era.",
        "valore del campo" => "Questo valore non va bene per il campo (una data o un numero scritti in un altro modo?): il campo è rimasto com'era.",
        "foto della scansione" => {
            "La foto di questa scansione è in un formato che non so rifare (fax, JBIG2 o JPEG 2000) o è girata nella pagina: la correzione non si fa, per non lasciare la scritta di prima nascosta sotto. Il PDF è rimasto com'era."
        }
        "senza la foto della scansione" => "Sotto questo testo non c'è la foto di una scansione da correggere: la correzione non si fa. Il PDF è rimasto com'era.",
        "lettere della scansione" => "Nella foto non riesco a trovare le lettere di questa riga (troppo chiare o sbiadite): la correzione non si fa. Il PDF è rimasto com'era.",
        "riga della scansione piena" => "Il testo nuovo non entra nella riga della scansione: accorcialo un poco e riprova. Il PDF è rimasto com'era.",
        "lettera fuori dal carattere della scansione" => {
            "Nel testo nuovo c'è un simbolo che il carattere della scansione non ha (le lettere italiane sì): la correzione non si fa. Il PDF è rimasto com'era."
        }
        "firma digitale" => {
            "Questo PDF firmato non si può modificare così: il motore dovrebbe riscriverlo tutto e le firme non sarebbero più valide."
        }
        _ if genere == GenereModifica::Pagine => "Non riesco a cambiare la pagina: il PDF è rimasto com'era.",
        _ => "Non riesco a fare la modifica: il PDF è rimasto com'era.",
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn i_nomi_delle_copie_e_della_condivisione() {
        assert_eq!(base_del_nome("Consenso informato.pdf"), "Consenso informato");
        assert!(base_del_nome("documento.pdf").starts_with("Documento "));
        assert_eq!(nome_da_condividere("Bolletta.PDF"), "Bolletta.PDF");
        assert_eq!(nome_da_condividere("Bolletta Zanzibar.docx"), "Bolletta Zanzibar.pdf");
    }

    #[test]
    fn strumenti_e_modi_solo_quelli_che_funzionano() {
        // Le voci accese in questo passaggio; gli altri modi e strumenti hanno la loro struttura.
        assert_eq!(ModoPdf::TUTTI.iter().filter(|m| m.fatto()).count(), 4);
        let fatti: Vec<&str> = Strumento::TUTTI.iter().filter(|s| s.fatto()).map(|s| s.nome()).collect();
        assert_eq!(
            fatti,
            [
                "Modifica testo",
                "Aggiungi testo",
                "Immagine",
                "Riconosci testo",
                "Forme",
                "Link",
                "Firma",
                "Evidenzia",
                "Penna",
                "Nota",
                "Organizza pagine",
                "Unisci",
                "Dividi",
                "Comprimi",
                "Esporta in Word",
                "Proteggi",
                "Togli la protezione",
                "Oscura",
                "Filigrana",
                "Numeri di pagina",
                "Scansiona",
                "Miniature",
                "Indice",
                "Vai a pagina"
            ]
        );
        // Ogni strumento ha un nome e un'icona sua.
        let nomi: std::collections::HashSet<&str> = Strumento::TUTTI.iter().map(|s| s.nome()).collect();
        assert_eq!(nomi.len(), Strumento::TUTTI.len());
    }
}
