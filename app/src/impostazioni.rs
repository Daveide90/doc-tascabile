//! Le Impostazioni: la Guida rapida, la diagnosi (il registro e la sua condivisione), le prove per chi sviluppa (la
//! checklist della tastiera e il documento lungo) e le informazioni sull'app, con le licenze di
//! caratteri, icone, motori e librerie. Toccando una licenza, il suo testo sale in un foglio.

use std::path::Path;

use eframe::egui::{self, Margin, RichText, ScrollArea, Ui, Vec2, vec2};
use tascabile_touch::tema::{self, Carattere};
use tascabile_touch::{BarraAlta, CampoTesto, FoglioDalFondo, Peso, Pulsante, Scheda, Tipo, Voce, etichetta_sezione, icone};

use crate::VERSIONE;

/// La riga delle Impostazioni che toglie la firma salvata.
pub(crate) const CANCELLA_FIRMA: &str = "Cancella firma salvata";
/// La riga e il foglio dell'autore dei commenti, nei PDF e nei documenti Word.
pub(crate) const AUTORE: &str = "Autore dei commenti";
pub(crate) const VUOTO_AUTORE: &str = "Nome da mettere nei commenti";
pub(crate) const FATTO: &str = "Fatto";
pub(crate) const TOGLI_NOME: &str = "Togli il nome";
const SENZA_AUTORE: &str = "Nessuno: i commenti non dicono chi li ha scritti";
const SPIEGA_AUTORE: &str = "Il nome che va nelle note, nelle evidenziature, nei disegni, nelle firme e negli altri segni dei PDF, e nei \
     commenti e nelle revisioni dei documenti Word. Lo scrivi tu, e Tascabile non lo prende dal telefono: però resta dentro i \
     file che segni, e chi li riceve lo legge.";
/// La riga del controllo ortografico, col suo interruttore.
/// La sezione della sicurezza (roadmap della sicurezza, Fase 1) e la modalità riservata.
pub(crate) const SICUREZZA: &str = "Sicurezza e privacy";
pub(crate) const RISERVATA: &str = "Modalità riservata";
const SPIEGA_RISERVATA: &str = "Schermo nero in screenshot, registrazioni e videochiamate";
/// Accesa: c'è il file. Spenta di base, perché gli screenshot servono anche per mostrare un problema.
const FILE_RISERVATA: &str = "riservata";
/// La rete per il timestamp (livello 1): accesa, l'app chiede prima di ogni uso; spenta, mai.
pub(crate) const RETE_TIMESTAMP: &str = "Rete per il timestamp";
const SPIEGA_RETE: &str = "Chiede prima di ogni uso; spenta, non va mai su internet";
/// Spenta: c'è il file.
const FILE_RETE_SPENTA: &str = "rete-timestamp-spenta";
/// Il blocco dell'app con l'impronta o il PIN: spento di base (acceso: c'è il file).
pub(crate) const BLOCCO_APP: &str = "Blocco dell'app";
const SPIEGA_BLOCCO: &str = "Impronta o PIN all'apertura e dopo 2 minuti fuori";
const FILE_BLOCCO: &str = "blocco-app";
/// L'impronta o il PIN prima di posare la firma salvata: accesa di base (spenta: c'è il file).
pub(crate) const IMPRONTA_FIRMA: &str = "Impronta per la firma";
const SPIEGA_IMPRONTA_FIRMA: &str = "La firma salvata si mette solo con impronta o PIN";
const FILE_IMPRONTA_SPENTA: &str = "impronta-firma-spenta";
pub(crate) const ORTOGRAFIA: &str = "Controllo ortografico";
const SPIEGA_ORTOGRAFIA: &str = "Le parole sbagliate sottolineate in rosso nei documenti Word";
/// Il file del controllo ortografico spento, nella cartella dell'app (acceso se non c'è).
const FILE_ORTOGRAFIA: &str = "ortografia-spenta";
/// Il file dell'autore nella cartella dell'app, e quanto può essere lungo.
const FILE_AUTORE: &str = "autore.txt";
const MASSIMO_AUTORE: usize = 80;

/// L'autore scritto nelle Impostazioni, dalla cartella dell'app: vuoto se non c'è (mai preso dal
/// telefono). Una riga, al massimo `MASSIMO_AUTORE` caratteri.
pub(crate) fn leggi_autore(cartella: &Path) -> String {
    std::fs::read_to_string(cartella.join(FILE_AUTORE)).map(|t| pulito(&t)).unwrap_or_default()
}

/// Scrive l'autore nella cartella dell'app (vuoto: il file se ne va).
pub(crate) fn scrivi_autore(cartella: &Path, autore: &str) -> std::io::Result<()> {
    let file = cartella.join(FILE_AUTORE);
    if autore.is_empty() {
        return match std::fs::remove_file(file) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            altro => altro,
        };
    }
    std::fs::create_dir_all(cartella)?;
    tascabile_word::salva::scrivi_atomico(&file, autore.as_bytes())
}

/// La modalità riservata è accesa, se la si è accesa nelle Impostazioni.
pub(crate) fn leggi_riservata(cartella: &Path) -> bool {
    cartella.join(FILE_RISERVATA).exists()
}

/// Accende o spegne la modalità riservata, nella cartella dell'app (accesa: c'è il file).
pub(crate) fn scrivi_riservata(cartella: &Path, accesa: bool) -> std::io::Result<()> {
    let file = cartella.join(FILE_RISERVATA);
    if !accesa {
        return match std::fs::remove_file(file) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            altro => altro,
        };
    }
    std::fs::create_dir_all(cartella)?;
    tascabile_word::salva::scrivi_atomico(&file, b"")
}

/// La rete per il timestamp è accesa (si chiede a ogni uso), se non la si è spenta.
pub(crate) fn leggi_rete_timestamp(cartella: &Path) -> bool {
    !cartella.join(FILE_RETE_SPENTA).exists()
}

/// Accende o spegne la rete per il timestamp (spenta: c'è il file).
pub(crate) fn scrivi_rete_timestamp(cartella: &Path, accesa: bool) -> std::io::Result<()> {
    scrivi_file_della_scelta(&cartella.join(FILE_RETE_SPENTA), !accesa)
}

/// Il file della scelta c'è (vero) o no.
fn scrivi_file_della_scelta(file: &Path, c_e: bool) -> std::io::Result<()> {
    if !c_e {
        return match std::fs::remove_file(file) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            altro => altro,
        };
    }
    if let Some(cartella) = file.parent() {
        std::fs::create_dir_all(cartella)?;
    }
    tascabile_word::salva::scrivi_atomico(file, b"")
}

/// Il blocco dell'app è acceso, se lo si è acceso nelle Impostazioni.
pub(crate) fn leggi_blocco(cartella: &Path) -> bool {
    cartella.join(FILE_BLOCCO).exists()
}

/// Accende o spegne il blocco dell'app (acceso: c'è il file).
pub(crate) fn scrivi_blocco(cartella: &Path, acceso: bool) -> std::io::Result<()> {
    scrivi_file_della_scelta(&cartella.join(FILE_BLOCCO), acceso)
}

/// L'impronta per la firma è accesa, se non la si è spenta.
pub(crate) fn leggi_impronta_firma(cartella: &Path) -> bool {
    !cartella.join(FILE_IMPRONTA_SPENTA).exists()
}

/// Accende o spegne l'impronta per la firma (spenta: c'è il file).
pub(crate) fn scrivi_impronta_firma(cartella: &Path, accesa: bool) -> std::io::Result<()> {
    scrivi_file_della_scelta(&cartella.join(FILE_IMPRONTA_SPENTA), !accesa)
}

/// Il controllo ortografico è acceso, se non lo si è spento nelle Impostazioni.
pub(crate) fn leggi_ortografia(cartella: &Path) -> bool {
    !cartella.join(FILE_ORTOGRAFIA).exists()
}

/// Accende o spegne il controllo ortografico, nella cartella dell'app (spento: c'è il file).
pub(crate) fn scrivi_ortografia(cartella: &Path, acceso: bool) -> std::io::Result<()> {
    let file = cartella.join(FILE_ORTOGRAFIA);
    if acceso {
        return match std::fs::remove_file(file) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            altro => altro,
        };
    }
    std::fs::create_dir_all(cartella)?;
    tascabile_word::salva::scrivi_atomico(&file, b"")
}

/// Il nome com'è da tenere: la prima riga, senza spazi ai lati né caratteri di controllo.
pub(crate) fn pulito(testo: &str) -> String {
    let riga = testo.lines().next().unwrap_or("");
    riga.chars().filter(|c| !c.is_control()).take(MASSIMO_AUTORE).collect::<String>().trim().to_string()
}

/// Il nome dell'app sullo schermo.
pub const NOME_APP: &str = "Doc Tascabile";

/// Quello che si è scelto nelle Impostazioni e che fa l'app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scelta {
    Indietro,
    /// Il registro della diagnosi, sullo schermo.
    Registro,
    CondividiDiagnosi,
    Checklist,
    Lungo,
    /// Toglie la firma e le iniziali salvate nella memoria dell'app.
    CancellaFirma,
    /// L'autore dei commenti: il foglio si apre, il campo si tocca, «Fatto», «Togli il nome».
    Autore,
    CampoAutore,
    AutoreFatto,
    TogliAutore,
    /// L'interruttore del controllo ortografico.
    Ortografia,
    /// L'interruttore della modalità riservata.
    Riservata,
    /// L'interruttore della rete per il timestamp.
    ReteTimestamp,
    /// L'interruttore dell'impronta per la firma.
    ImprontaFirma,
    /// L'interruttore del blocco dell'app.
    Blocco,
}

/// Le righe dell'elenco: (icona, nome, riga sotto, scelta).
const DIAGNOSI: [(&str, &str, &str, Scelta); 2] = [
    (icone::BUG_REPORT, "Registro degli eventi", "Tempi, tocchi e tastiera: mai testi né nomi di file", Scelta::Registro),
    (icone::SHARE, "Condividi la diagnosi", "Il registro, da mandare tu a chi sviluppa", Scelta::CondividiDiagnosi),
];
const PROVE: [(&str, &str, &str, Scelta); 2] = [
    (icone::CHECKLIST, "Checklist della tastiera", "Le prove da fare sul telefono, con le risposte", Scelta::Checklist),
    (icone::ARTICLE, "Documento lungo", "Trecento paragrafi, per le misure", Scelta::Lungo),
];

/// Un pezzo di altri dentro l'app, con la sua licenza.
struct Componente {
    nome: &'static str,
    icona: &'static str,
    /// La riga sotto il nome: a che cosa serve, chi lo fa, la licenza.
    sotto: &'static str,
    /// Il testo del foglio: chi lo fa e la licenza intera.
    testo: fn() -> String,
}

/// La licenza MIT, senza la riga del copyright (che sta sopra, per ogni componente).
const MIT: &str = "Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the \"Software\"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:\n\nThe above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.\n\nTHE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.";

/// La licenza Apache 2.0 intera: è quella delle icone.
const APACHE: &str = include_str!("../../crates/touch/fonts/LICENSE-MaterialSymbols.txt");

const COMPONENTI: [Componente; 14] = [
    Componente {
        nome: "Licenza di Doc Tascabile",
        icona: icone::INFO,
        sotto: "La licenza di questa app · PolyForm Noncommercial 1.0.0",
        testo: || include_str!("../../LICENSE").to_string(),
    },
    Componente {
        nome: "Fraunces",
        icona: icone::TITLE,
        sotto: "Il carattere dei titoli · SIL Open Font License 1.1",
        testo: || include_str!("../../crates/touch/fonts/OFL-Fraunces.txt").to_string(),
    },
    Componente {
        nome: "Atkinson Hyperlegible Next",
        icona: icone::TEXT_FIELDS,
        sotto: "Il carattere del testo · SIL Open Font License 1.1",
        testo: || include_str!("../../crates/touch/fonts/OFL-AtkinsonHyperlegibleNext.txt").to_string(),
    },
    Componente {
        nome: "Material Symbols",
        icona: icone::APPS,
        sotto: "Le icone, di Google · Apache License 2.0",
        testo: || format!("Material Symbols, Google.\n\n{APACHE}"),
    },
    Componente {
        nome: "WordCraft",
        icona: icone::DESCRIPTION,
        sotto: "Il motore dei documenti · MIT o Apache 2.0",
        testo: || {
            format!(
                "WordCraft\nCopyright (c) 2026 ArtCraft Team and the WordCraft contributors.\nLicenza MIT o Apache 2.0, a scelta; qui la MIT.\n\n\
                 WordCraft porta con sé i caratteri Source Sans 3, Source Serif 4, Inter e JetBrains Mono (SIL Open Font License 1.1) e i \
                 dati della sillabazione con le loro licenze, elencati nel file NOTICE di WordCraft.\n\n{MIT}"
            )
        },
    },
    Componente {
        nome: "GridCraft",
        icona: icone::TABLE,
        sotto: "I fogli di calcolo · MIT o Apache 2.0",
        testo: || {
            format!(
                "GridCraft\nCopyright (c) 2026 ArtCraft Team and the GridCraft contributors.\nLicenza MIT o Apache 2.0, a scelta; qui la MIT.\n\n\
                 Con RaTeX (MIT) per le equazioni dentro le celle.\n\n{MIT}"
            )
        },
    },
    Componente {
        nome: "egui ed eframe",
        icona: icone::VIEW_MODULE,
        sotto: "L'interfaccia, di Emil Ernerfeldt · MIT o Apache 2.0",
        testo: || {
            format!(
                "egui ed eframe, di Emil Ernerfeldt e di chi contribuisce.\nLicenza MIT o Apache 2.0, a scelta; qui la MIT.\n\n\
                 Con i caratteri di riserva di egui: Ubuntu Light (Ubuntu Font Licence 1.0), Noto Emoji (SIL Open Font License 1.1), \
                 Hack (MIT, Copyright 2018 Source Foundry Authors; Bitstream Vera License) ed emoji-icon-font (MIT, Copyright (c) 2014 \
                 John Slegers).\n\n{MIT}"
            )
        },
    },
    Componente {
        nome: "wgpu e winit",
        icona: icone::PALETTE,
        sotto: "Il disegno e la finestra · MIT o Apache 2.0; Apache 2.0",
        testo: || {
            format!(
                "wgpu: Copyright (c) 2025 The gfx-rs developers. Licenza MIT o Apache 2.0, a scelta.\n\
                 winit, di chi contribuisce a winit: Apache License 2.0.\n\n{APACHE}"
            )
        },
    },
    Componente {
        nome: "Riconoscimento del testo",
        icona: icone::DOCUMENT_SCANNER,
        sotto: "PaddleOCR PP-OCRv6 · Apache 2.0; rilevamento di ocrs · CC BY-SA 4.0",
        testo: || format!("{}\n\n{APACHE}", include_str!("../../crates/ocr/modelli/NOTICE")),
    },
    Componente {
        nome: "Dizionario italiano",
        icona: icone::SPELLCHECK,
        sotto: "L'ortografia, di LibreItalia · GNU GPL 3",
        testo: || {
            format!(
                "{}\n{}",
                include_str!("../../crates/mod-word/asset/dizionario/LEGGIMI.txt"),
                include_str!("../../crates/mod-word/asset/dizionario/COPYING-GPL-3.0.txt")
            )
        },
    },
    Componente {
        nome: "PDFCraft e DeckCraft",
        icona: icone::PICTURE_AS_PDF,
        sotto: "I motori dei PDF e delle slide · MIT o Apache 2.0",
        testo: || {
            format!(
                "PDFCraft\nCopyright (c) 2026 ArtCraft Team and the PdfCraft contributors.\n\
                 DeckCraft\nCopyright (c) 2026 ArtCraft Team and the DeckCraft contributors.\n\
                 Licenza MIT o Apache 2.0, a scelta; qui la MIT.\n\n\
                 Il materiale di terzi che portano con sé (caratteri, icone Lucide con licenza ISC, dati) è nei loro file NOTICE, \
                 riportati per intero in «Tutte le librerie Rust».\n\n{MIT}"
            )
        },
    },
    Componente {
        nome: "Caratteri delle slide",
        icona: icone::SLIDESHOW,
        sotto: "Carlito, Caladea, Inter, Liberation · SIL Open Font License 1.1",
        testo: || {
            format!(
                "Caratteri usati per mostrare slide e fogli di calcolo al posto di quelli di Office.\n\n\
                 Carlito\n\n{}\n\nCaladea\n\n{}\n\nInter\n\n{}\n\nLiberation Sans\n\n{}\n\nLiberation Serif\n\n{}",
                include_str!("../../crates/mod-slide/fonts/carlito/OFL.txt"),
                include_str!("../../crates/mod-slide/fonts/caladea/OFL.txt"),
                include_str!("../../crates/mod-slide/fonts/inter/OFL.txt"),
                include_str!("../../crates/mod-slide/fonts/liberation-sans/OFL.txt"),
                include_str!("../../crates/mod-slide/fonts/liberation-serif/OFL.txt")
            )
        },
    },
    Componente {
        nome: "Tutte le librerie Rust",
        icona: icone::LIST,
        sotto: "L'elenco completo, con autori, licenze e NOTICE",
        testo: || include_str!("../licenze/librerie-rust.txt").to_string(),
    },
    Componente {
        nome: "Librerie Android",
        icona: icone::PHONE_ANDROID,
        sotto: "AndroidX, GameActivity, Kotlin · Apache 2.0",
        testo: || format!("{}\n\n{APACHE}", include_str!("../licenze/librerie-android.txt")),
    },
];

/// La schermata delle Impostazioni, col foglio della licenza aperta.
pub struct Impostazioni {
    foglio: FoglioDalFondo,
    /// Il componente della licenza nel foglio.
    licenza: usize,
    /// L'autore dei segni e dei commenti nei PDF (va in `/T`, ricerca PDFCraft, sezione 7) e dei
    /// commenti e delle revisioni nei documenti Word: mai scritto nel codice; vuoto finché chi usa
    /// l'app non lo sceglie.
    pub(crate) autore: String,
    /// Il controllo ortografico dei documenti Word.
    pub(crate) ortografia: bool,
    /// La modalità riservata, accesa.
    pub(crate) riservata: bool,
    /// La rete per il timestamp, accesa (si chiede a ogni uso).
    pub(crate) rete_timestamp: bool,
    /// L'impronta o il PIN prima di posare la firma salvata.
    pub(crate) impronta_firma: bool,
    /// Il blocco dell'app con l'impronta o il PIN.
    pub(crate) blocco: bool,
    /// C'è una firma (o le iniziali) salvata: si mostra «Cancella firma salvata».
    pub(crate) firma_salvata: bool,
    /// Il foglio della Guida rapida.
    foglio_guida: FoglioDalFondo,
    /// Il foglio dell'autore col suo campo, e se era aperto al frame prima.
    pub(crate) foglio_autore: FoglioDalFondo,
    pub(crate) campo_autore: CampoTesto,
    pub(crate) autore_aperto: bool,
}

impl Default for Impostazioni {
    fn default() -> Self {
        Self {
            foglio: FoglioDalFondo::new(egui::Id::new("foglio_licenza")),
            licenza: 0,
            autore: String::new(),
            ortografia: true,
            riservata: false,
            rete_timestamp: true,
            impronta_firma: true,
            blocco: false,
            firma_salvata: false,
            foglio_guida: FoglioDalFondo::new(egui::Id::new("foglio_guida")),
            foglio_autore: FoglioDalFondo::new(egui::Id::new("foglio_autore")),
            campo_autore: CampoTesto::new(),
            autore_aperto: false,
        }
    }
}

impl Impostazioni {
    /// Indietro di Android: chiude il foglio della licenza, quello dell'autore o la guida, se è
    /// aperto (vero).
    pub fn indietro(&mut self) -> bool {
        self.foglio.indietro() || self.foglio_autore.indietro() || self.foglio_guida.indietro()
    }

    /// Chiude i fogli, uscendo dalla schermata.
    pub fn chiudi(&mut self) {
        self.foglio.chiudi();
        self.foglio_autore.chiudi();
        self.foglio_guida.chiudi();
    }

    /// Le Impostazioni con l'autore dei commenti e il controllo ortografico della cartella dell'app.
    pub(crate) fn con_sicurezza(mut self, blocco: bool, riservata: bool, rete_timestamp: bool, impronta_firma: bool) -> Self {
        self.blocco = blocco;
        self.riservata = riservata;
        self.rete_timestamp = rete_timestamp;
        self.impronta_firma = impronta_firma;
        self
    }

    pub(crate) fn con_autore(autore: String, ortografia: bool) -> Self {
        Self { autore, ortografia, ..Self::default() }
    }

    /// Il campo dell'autore, se il suo foglio è aperto: prende la tastiera.
    pub(crate) fn campo_autore_mut(&mut self) -> Option<&mut CampoTesto> {
        self.foglio_autore.aperto().then_some(&mut self.campo_autore)
    }

    /// Disegna la schermata; il foglio della licenza va mostrato a ogni frame perché salga.
    pub fn mostra(&mut self, ui: &mut Ui) -> Option<Scelta> {
        let mut scelta = None;
        let mut licenza = None;
        let mut guida = false;
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        if BarraAlta::new("Impostazioni").indietro("Indietro").mostra(ui).indietro {
            scelta = Some(Scelta::Indietro);
        }
        ScrollArea::vertical().id_salt("impostazioni").scroll_source(egui::scroll_area::ScrollSource::ALL).auto_shrink([false, false]).show(
            ui,
            |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                sezione(ui, "Aiuto");
                scheda(ui, |ui| guida = riga(ui, icone::HELP, crate::guida::GUIDA, crate::guida::SOTTO, true));
                sezione(ui, SICUREZZA);
                scheda(ui, |ui| {
                    let voce = Voce::new(Tipo::Cartella, BLOCCO_APP).icona(icone::LOCK).sotto(SPIEGA_BLOCCO).ultima(false);
                    if voce.interruttore(self.blocco).mostra(ui).aperta() {
                        scelta = Some(Scelta::Blocco);
                    }
                    let voce = Voce::new(Tipo::Cartella, RISERVATA).icona(icone::VISIBILITY_OFF).sotto(SPIEGA_RISERVATA).ultima(false);
                    if voce.interruttore(self.riservata).mostra(ui).aperta() {
                        scelta = Some(Scelta::Riservata);
                    }
                    let voce = Voce::new(Tipo::Cartella, RETE_TIMESTAMP).icona(icone::SCHEDULE).sotto(SPIEGA_RETE).ultima(false);
                    if voce.interruttore(self.rete_timestamp).mostra(ui).aperta() {
                        scelta = Some(Scelta::ReteTimestamp);
                    }
                    let voce = Voce::new(Tipo::Cartella, IMPRONTA_FIRMA).icona(icone::SIGNATURE).sotto(SPIEGA_IMPRONTA_FIRMA).ultima(!self.firma_salvata);
                    if voce.interruttore(self.impronta_firma).mostra(ui).aperta() {
                        scelta = Some(Scelta::ImprontaFirma);
                    }
                    if self.firma_salvata && riga(ui, icone::SIGNATURE, CANCELLA_FIRMA, "La firma e le iniziali salvate in Tascabile per i PDF", true) {
                        scelta = Some(Scelta::CancellaFirma);
                    }
                });
                for (titolo, righe) in [("Diagnosi", &DIAGNOSI), ("Prove per chi sviluppa", &PROVE)] {
                    sezione(ui, titolo);
                    scheda(ui, |ui| {
                        for (i, (icona, nome, sotto, s)) in righe.iter().enumerate() {
                            if riga(ui, icona, nome, sotto, i + 1 == righe.len()) {
                                scelta = Some(*s);
                            }
                        }
                    });
                }
                sezione(ui, "Ortografia");
                scheda(ui, |ui| {
                    let voce = Voce::new(Tipo::Cartella, ORTOGRAFIA).icona(icone::SPELLCHECK).sotto(SPIEGA_ORTOGRAFIA).ultima(true);
                    if voce.interruttore(self.ortografia).mostra(ui).aperta() {
                        scelta = Some(Scelta::Ortografia);
                    }
                });
                sezione(ui, "Commenti");
                scheda(ui, |ui| {
                    let sotto = if self.autore.is_empty() { SENZA_AUTORE } else { self.autore.as_str() };
                    if riga(ui, icone::PERSON, AUTORE, sotto, true) {
                        scelta = Some(Scelta::Autore);
                    }
                });
                sezione(ui, "Informazioni");
                scheda(ui, |ui| {
                    egui::Frame::NONE.inner_margin(Margin::same(18)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = 4.0;
                        let ctx = ui.ctx().clone();
                        ui.label(RichText::new(NOME_APP).font(Carattere::Titolo.font(&ctx, 22.0)).color(tema::INCHIOSTRO));
                        ui.label(RichText::new(format!("Versione {VERSIONE}")).font(Carattere::Grassetto.font(&ctx, 15.0)).color(tema::INCHIOSTRO));
                        ui.label(
                            RichText::new("I documenti restano sul telefono: in rete va solo l'impronta di un file, col «Timestamp con blockchain».")
                                .font(Carattere::Testo.font(&ctx, 14.0))
                                .color(tema::MATITA),
                        );
                        ui.add_space(4.0);
                        ui.label(RichText::new(crate::guida::SVILUPPO_CON_IA).font(Carattere::Testo.font(&ctx, 14.0)).color(tema::MATITA));
                    });
                });
                sezione(ui, "Caratteri, icone e librerie");
                scheda(ui, |ui| {
                    for (i, c) in COMPONENTI.iter().enumerate() {
                        if riga(ui, c.icona, c.nome, c.sotto, i + 1 == COMPONENTI.len()) {
                            licenza = Some(i);
                        }
                    }
                });
                ui.add_space(24.0);
            },
        );
        if let Some(i) = licenza {
            self.licenza = i;
            self.foglio.apri();
        }
        if guida {
            tascabile_platform::diagnosi("impostazioni: guida rapida aperta");
            self.foglio_guida.apri();
        }
        self.foglio_guida.mostra(ui.ctx(), crate::guida::GUIDA, crate::guida::mostra);
        let c = &COMPONENTI[self.licenza];
        self.foglio.mostra(ui.ctx(), c.nome, |ui| {
            let ctx = ui.ctx().clone();
            // Un'etichetta per paragrafo: l'elenco delle librerie è lungo, e così egui disegna
            // solo i paragrafi che si vedono.
            for paragrafo in (c.testo)().split("\n\n") {
                ui.label(RichText::new(paragrafo).font(Carattere::Testo.font(&ctx, 13.0)).color(tema::INCHIOSTRO));
            }
        });
        let c_e = !self.autore.is_empty();
        let campo = &self.campo_autore;
        let dal_foglio = self
            .foglio_autore
            .mostra(ui.ctx(), AUTORE, |ui| {
                let ctx = ui.ctx().clone();
                let mut s = None;
                ui.label(RichText::new(SPIEGA_AUTORE).font(Carattere::Testo.font(&ctx, 15.0)).color(tema::MATITA));
                ui.add_space(12.0);
                if ui.allocate_ui(vec2(ui.available_width(), tema::PULSANTE), |ui| campo.mostra(ui, VUOTO_AUTORE).clicked()).inner {
                    s = Some(Scelta::CampoAutore);
                }
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    if ui.add(Pulsante::new(FATTO, Peso::Principale)).clicked() {
                        s = Some(Scelta::AutoreFatto);
                    }
                    if c_e && ui.add(Pulsante::new(TOGLI_NOME, Peso::Testo)).clicked() {
                        s = Some(Scelta::TogliAutore);
                    }
                });
                ui.add_space(8.0);
                s
            })
            .flatten();
        scelta.or(dal_foglio)
    }
}

/// Il titolo di una sezione, maiuscolo e grigio, al margine dei titoli.
fn sezione(ui: &mut Ui, titolo: &str) {
    egui::Frame::NONE.inner_margin(Margin { left: tema::MARGINE_TITOLO as i8 - 2, right: 0, top: 8, bottom: 8 }).show(ui, |ui| {
        etichetta_sezione(ui, titolo);
    });
}

/// Una scheda bianca ai margini delle schede.
fn scheda(ui: &mut Ui, contenuto: impl FnOnce(&mut Ui)) {
    egui::Frame::NONE.inner_margin(Margin::symmetric(tema::MARGINE as i8, 0)).show(ui, |ui| {
        Scheda::new().show(ui, contenuto);
    });
}

/// Una riga dell'elenco: la voce col riquadro grigio e l'icona; vero se toccata.
fn riga(ui: &mut Ui, icona: &str, nome: &str, sotto: &str, ultima: bool) -> bool {
    Voce::new(Tipo::Cartella, nome).icona(icona).sotto(sotto).ultima(ultima).mostra(ui).aperta()
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn l_autore_si_scrive_e_si_toglie() {
        let cartella = crate::prova::cartella_di_prova("autore").join("app");
        assert_eq!(leggi_autore(&cartella), "");
        scrivi_autore(&cartella, "Mario Rossi").unwrap();
        assert_eq!(leggi_autore(&cartella), "Mario Rossi");
        std::fs::write(cartella.join(FILE_AUTORE), "  Anna\u{7}\nseconda riga").unwrap();
        assert_eq!(leggi_autore(&cartella), "Anna");
        assert_eq!(pulito(&"x".repeat(200)).len(), MASSIMO_AUTORE);
        scrivi_autore(&cartella, "").unwrap();
        assert!(!cartella.join(FILE_AUTORE).exists());
        scrivi_autore(&cartella, "").unwrap();
    }

    #[test]
    fn il_controllo_ortografico_si_spegne_e_si_riaccende() {
        let cartella = crate::prova::cartella_di_prova("ortografia").join("app");
        assert!(leggi_ortografia(&cartella), "acceso finché non lo si spegne");
        scrivi_ortografia(&cartella, false).unwrap();
        assert!(!leggi_ortografia(&cartella));
        scrivi_ortografia(&cartella, true).unwrap();
        assert!(leggi_ortografia(&cartella));
        scrivi_ortografia(&cartella, true).unwrap();
    }

    #[test]
    fn ogni_licenza_ha_il_suo_testo() {
        let licenze = ["SIL OPEN FONT LICENSE Version 1.1", "Apache License", "Permission is hereby granted", "GNU GENERAL PUBLIC LICENSE", "PolyForm Noncommercial License 1.0.0"];
        for c in &COMPONENTI {
            let testo = (c.testo)();
            assert!(testo.len() > 500, "{}: {} caratteri", c.nome, testo.len());
            assert!(licenze.iter().any(|l| testo.contains(l)), "{}: manca il testo della licenza", c.nome);
        }
        let testo = |nome: &str| (COMPONENTI.iter().find(|c| c.nome == nome).expect(nome).testo)();
        // I due caratteri con la OFL e il loro copyright, le icone con la Apache 2.0.
        assert!(testo("Fraunces").contains("SIL OPEN FONT LICENSE") && testo("Fraunces").contains("The Fraunces Project Authors"));
        let atkinson = testo("Atkinson Hyperlegible Next");
        assert!(atkinson.contains("SIL OPEN FONT LICENSE") && atkinson.contains("Atkinson Hyperlegible Next Project Authors"));
        assert!(testo("Material Symbols").contains("Apache License") && testo("Material Symbols").contains("Version 2.0"));
        assert!(testo("WordCraft").contains("ArtCraft Team") && testo("WordCraft").contains("Permission is hereby granted"));
        assert!(testo("egui ed eframe").contains("Emil Ernerfeldt"));
        // I modelli dell'OCR (P8): chi li fa, da dove vengono e le due licenze.
        let ocr = testo("Riconoscimento del testo");
        for atteso in ["PaddleOCR", "Apache License", "CC BY-SA 4.0", "Robert Knight", "huggingface.co/PaddlePaddle"] {
            assert!(ocr.contains(atteso), "manca {atteso}");
        }
        // Il dizionario dell'ortografia (W4): chi lo fa, da dove viene e la GPL 3 intera.
        let dizionario = testo("Dizionario italiano");
        for atteso in ["LibreItalia", "Version 3, 29 June 2007", "it_IT.dic", "non modificati"] {
            assert!(dizionario.contains(atteso), "manca {atteso}");
        }
        // La licenza dell'app stessa, con la riga «Required Notice» che la PolyForm chiede di
        // passare a chi riceve il software (revisione del 6/11/2026, punto 8).
        let propria = testo("Licenza di Doc Tascabile");
        assert!(propria.contains("Required Notice: Copyright (c) 2026 Daveide90") && propria.contains("PolyForm Noncommercial"));
        // Gli avvisi che BSD, ISC, Unicode e Apache chiedono anche nell'APK (6/11/2026).
        let craft = testo("PDFCraft e DeckCraft");
        assert!(craft.contains("PdfCraft contributors") && craft.contains("DeckCraft contributors"));
        let slide = testo("Caratteri delle slide");
        for atteso in ["Carlito", "Caladea", "Inter", "Liberation Sans", "Liberation Serif"] {
            assert!(slide.contains(atteso), "manca {atteso}");
        }
        assert_eq!(slide.matches("SIL OPEN FONT LICENSE").count(), 5, "una OFL per carattere");
        let rust = testo("Tutte le librerie Rust");
        for atteso in [
            "aws-lc-sys",
            "Redistribution and use in source and binary forms",
            "UNICODE LICENSE V3",
            "ISC",
            "ArtCraft Team and the WordCraft contributors",
            "UBUNTU FONT LICENCE",
            "TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION",
        ] {
            assert!(rust.contains(atteso), "manca {atteso}");
        }
        let android = testo("Librerie Android");
        for atteso in ["Android Open Source Project", "GameActivity", "Stephen Colebourne", "kotlinx.coroutines", "Apache License"] {
            assert!(android.contains(atteso), "manca {atteso}");
        }
    }
}
