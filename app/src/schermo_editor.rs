//! L'Editor dei bozzetti 2, 3, 5 e 6: la barra alta con indietro, il nome, lo stato del documento,
//! Annulla, Ripeti e il menu; la pagina come un foglio bianco sulla carta, con gli angoli in alto
//! tondi; la barra di formato flottante, che con la tastiera aperta si appoggia sulla tastiera e
//! lascia sempre visibile la riga del cursore; il menu con le icone, i fogli «Inserisci» e
//! «Informazioni sul documento»; Trova al posto della barra alta e il foglio dell'Indice. Il
//! foglio «Aa» è in `foglio_aa.rs`.

use eframe::egui::{self, Align, CornerRadius, Layout, Margin, Rect, RichText, UiBuilder, Vec2, pos2};
use tascabile_files::tipo_da_nome;
use tascabile_platform::diagnosi;
use tascabile_touch::tema::{self, Carattere};
use tascabile_touch::{BarraAlta, BarraFlottante, Chip, Menu, Peso, Pulsante, PulsanteIcona, SceltaMenu, Tipo, Voce, icone};
use tascabile_word::editor::{Appunti, Conteggi, Inserimento, Titolo, conteggi, data_e_ora_adesso};
use tascabile_word::immagine::immagine_scelta;
use tascabile_word::salva::Origine;
use tascabile_word::tabella::info_tabella;
use tascabile_word::vista::{Modo, colore_carta};

use crate::foglio_immagine::{self, IMMAGINE};
use crate::foglio_tabella::{self, NellaGriglia, TABELLA};
use crate::{Comando, Dopo, Fuoco, Schermata, TascabileApp, home};

/// Le etichette dei pulsanti della barra alta (per l'accessibilità e per le prove).
pub(crate) const INDIETRO: &str = "Indietro";
pub(crate) const ANNULLA: &str = "Annulla modifica";
pub(crate) const RIPETI: &str = "Ripeti modifica";
pub(crate) const MENU: &str = "Menu";
/// Le etichette di Trova: il testo grigio del campo vuoto e i pulsanti.
pub(crate) const VUOTO_TROVA: &str = "Trova nel documento";
pub(crate) const PRIMA: &str = "Risultato prima";
pub(crate) const DOPO: &str = "Risultato dopo";
pub(crate) const CHIUDI_TROVA: &str = "Chiudi Trova";
/// Il pulsante che apre la seconda riga di Trova, e quello che c'è nella riga.
pub(crate) const TROVA_E_SOSTITUISCI: &str = "Trova e sostituisci";
pub(crate) const VUOTO_SOSTITUISCI: &str = "Sostituisci con";
pub(crate) const SOSTITUISCI: &str = "Sostituisci";
pub(crate) const TUTTI: &str = "Tutti";
pub(crate) const MAIUSCOLE: &str = "Maiuscole";
pub(crate) const PAROLA_INTERA: &str = "Parola intera";
/// Il campo di Trova dal bordo sinistro.
const TROVA_DA_SINISTRA: f32 = 12.0;
/// «Aa», che apre il foglio del formato, e «+», che apre il foglio «Inserisci».
pub(crate) const FORMATO: &str = "Formato";
pub(crate) const INSERISCI: &str = "Inserisci";

/// I pulsanti della barra di formato fra «Aa» e «+»: (icona, comando di WordCraft, etichetta).
/// I primi tre sono il formato del testo, gli ultimi due gli elenchi, separati da una linea.
pub(crate) const BARRA: [(&str, &str, &str); 5] = [
    (icone::FORMAT_BOLD, "format.bold", "Grassetto"),
    (icone::FORMAT_ITALIC, "format.italic", "Corsivo"),
    (icone::FORMAT_UNDERLINED, "format.underline", "Sottolineato"),
    (icone::FORMAT_LIST_BULLETED, "para.bullets", "Elenco puntato"),
    (icone::FORMAT_LIST_NUMBERED, "para.numbering", "Elenco numerato"),
];

/// La pagina dai bordi dello schermo (`.foglio { left: 10px; right: 10px }`).
const MARGINE_PAGINA: f32 = 10.0;
/// Con la tastiera aperta la pagina finisce sotto la barra di formato, per tanti dp (bozzetto 3).
const SOTTO_LA_BARRA: f32 = 4.0;
/// L'angolo del menu: 8 dp dal bordo destro, 44 sotto la cima della barra alta (`.menu`).
const MENU_DESTRA: f32 = 8.0;
const MENU_SOTTO: f32 = 44.0;

/// Le voci del menu, nell'ordine del bozzetto 6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VoceMenu {
    File(Comando),
    /// «Timestamp con blockchain» o «Controlla il timestamp».
    Data,
    Trova,
    Indice,
    Vista,
    Pagina,
    Revisioni,
    Informazioni,
}

const VOCI_MENU: [VoceMenu; 11] = [
    VoceMenu::File(Comando::Salva),
    VoceMenu::File(Comando::SalvaCopia),
    VoceMenu::File(Comando::EsportaPdf),
    VoceMenu::File(Comando::Condividi),
    VoceMenu::Data,
    VoceMenu::Trova,
    VoceMenu::Indice,
    VoceMenu::Vista,
    VoceMenu::Pagina,
    VoceMenu::Revisioni,
    VoceMenu::Informazioni,
];

/// Le voci del foglio «Inserisci»: (cosa, icona, nome).
const VOCI_INSERISCI: [(Inserimento, &str, &str); 3] = [
    (Inserimento::DataOra, icone::CALENDAR_TODAY, "Data e ora"),
    (Inserimento::InterruzionePagina, icone::INSERT_PAGE_BREAK, "Interruzione di pagina"),
    (Inserimento::LineaOrizzontale, icone::HORIZONTAL_RULE, "Linea orizzontale"),
];

/// «Informazioni sul documento», presa quando si apre il foglio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Informazioni {
    conteggi: Conteggi,
    cartella: String,
}

/// Un numero all'italiana, col punto fra le migliaia: «12.345».
pub(crate) fn numero(n: usize) -> String {
    let cifre = n.to_string();
    let mut fuori = String::new();
    for (i, c) in cifre.chars().enumerate() {
        if i > 0 && (cifre.len() - i).is_multiple_of(3) {
            fuori.push('.');
        }
        fuori.push(c);
    }
    fuori
}

impl TascabileApp {
    /// La schermata dell'Editor, dentro la zona libera da barre di sistema e tastiera.
    pub(crate) fn mostra_editor(&mut self, ui: &mut egui::Ui) {
        if self.aperto.is_none() {
            self.vai(Schermata::Home);
            return;
        }
        let zona = ui.max_rect();
        let in_pagina = self.editor().is_some_and(|e| e.vista.modo == Modo::Pagina);
        let tastiera = self.piattaforma.keyboard_height() > 0.0;
        // In cima, uno sotto l'altro: la barra alta (o Trova al suo posto), i messaggi.
        let mut esci = false;
        let sopra = ui
            .scope_builder(UiBuilder::new().max_rect(zona).layout(Layout::top_down(Align::Min)), |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                if self.trova.is_some() {
                    self.barra_trova(ui);
                    if self.sostituisci_con.is_some() {
                        self.riga_sostituisci(ui);
                    }
                } else {
                    esci = self.barra_alta(ui);
                }
                self.messaggi(ui);
            })
            .response
            .rect;
        let cima = sopra.bottom().max(zona.top() + tema::BARRA_ALTA);
        // La barra di formato (non in Pagina, che è in sola lettura): in fondo, o sulla tastiera.
        let barra = (!in_pagina).then(|| if tastiera { BarraFlottante::posto_sulla_tastiera(zona) } else { BarraFlottante::posto(zona) });
        let pagina = match barra {
            None => Rect::from_min_max(pos2(zona.left(), cima), zona.max),
            Some(b) => {
                let fondo = if tastiera { b.top() + SOTTO_LA_BARRA } else { zona.bottom() };
                Rect::from_min_max(pos2(zona.left() + MARGINE_PAGINA, cima), pos2(zona.right() - MARGINE_PAGINA, fondo.max(cima)))
            }
        };
        if barra.is_some() {
            self.dipingi_foglio(ui, pagina, tastiera);
        }
        // Con un'immagine scelta, sopra la barra di formato quella dell'immagine; se no, col cursore
        // in una tabella, quella della tabella.
        let immagine = if in_pagina { None } else { self.editor().and_then(|e| immagine_scelta(&e.s)).map(|(_, i)| i) };
        let immagine = barra.zip(immagine).map(|(b, i)| (foglio_tabella::posto_della_barra(b), i));
        let info = if in_pagina || immagine.is_some() { None } else { self.editor().and_then(|e| info_tabella(&e.s)) };
        let tabella = barra.zip(info).map(|(b, i)| (foglio_tabella::posto_della_barra(b), i));
        let sopra_la_formato = tabella.map(|(t, _)| t.top()).or(immagine.as_ref().map(|(t, _)| t.top()));
        // Quanto della pagina coprono le barre, che stanno sopra.
        let coperto = barra.map_or(0.0, |b| (pagina.bottom() - sopra_la_formato.unwrap_or(b.top())).max(0.0));
        ui.scope_builder(UiBuilder::new().max_rect(pagina), |ui| self.mostra_documento(ui, coperto, barra.is_some()));
        if let Some(b) = barra {
            self.barra_formato(ui, b, tastiera);
        }
        // `info` è di prima dei gesti di questo frame: se un tocco porta il cursore fuori dalla
        // tabella, la barra se ne va al frame dopo.
        if let Some((posto, i)) = tabella {
            self.barra_tabella(ui, posto, i);
        }
        if let Some((posto, i)) = &immagine {
            self.barra_immagine(ui, *posto, i);
        }
        if tabella.is_none() {
            self.menu_tabella = None;
        }
        if immagine.is_none() {
            self.menu_allinea = None;
        }
        self.mostra_menu_tabella(ui.ctx());
        self.mostra_menu_allinea(ui.ctx());
        self.mostra_menu_editor(ui.ctx(), zona);
        if esci {
            self.chiudi_poi(Dopo::Home);
        }
    }

    /// La pagina come un foglio sulla carta (`.foglio`): l'ombra, il colore della pagina, gli
    /// angoli in alto tondi. Senza tastiera continua fino in fondo allo schermo, anche sotto la
    /// barra di navigazione; con la tastiera finisce sotto la barra di formato, tonda anche in basso.
    fn dipingi_foglio(&self, ui: &egui::Ui, pagina: Rect, tastiera: bool) {
        let Some(e) = self.editor() else { return };
        let r = tema::RAGGIO_PAGINA;
        let (foglio, raggio) = if tastiera {
            (pagina, CornerRadius::same(r))
        } else {
            let fondo = ui.ctx().viewport_rect().bottom().max(pagina.bottom());
            (Rect::from_min_max(pagina.min, pos2(pagina.right(), fondo)), CornerRadius { nw: r, ne: r, sw: 0, se: 0 })
        };
        tema::dipingi_ombra(ui.painter(), foglio, raggio, &tema::OMBRA);
        ui.painter().rect_filled(foglio, raggio, colore_carta(&e.s));
    }

    /// Il documento nella sua pagina: la vista, i suoi gesti, la tastiera e gli appunti.
    fn mostra_documento(&mut self, ui: &mut egui::Ui, coperto: f32, foglio: bool) {
        // Col fuoco al campo, il ponte del documento resta avanti al campo con le versioni: un
        // tocco sul testo gli dà una finestra più nuova di ogni stato del campo.
        let campi = [self.trova.as_ref(), self.sostituisci_con.as_ref()].into_iter().flatten().map(|c| c.versione()).max();
        if let (Some(v), Some(a)) = (campi, self.aperto.as_mut()) {
            a.editor.ponte.continua_dalla_versione(v);
        }
        let campo = self.fuoco == Fuoco::Campo;
        let Some(e) = self.editor_mut() else { return };
        e.vista.senza_barra = campo;
        e.vista.coperto_sotto = coperto;
        // Il foglio lo dipinge l'app, con gli angoli tondi.
        e.vista.dipingi_carta = !foglio;
        let esito = e.mostra(ui);
        if esito.commenta {
            // «Commenta» sulla selezione: il foglio del commento, che prende la tastiera.
            self.apri_commento(None);
            return;
        }
        if esito.apri_tastiera {
            // Un tocco sul testo: la tastiera torna al documento.
            self.fuoco_al_documento();
        }
        self.dopo_ponte(esito.stato);
        if esito.apri_tastiera {
            self.piattaforma.show_keyboard();
        }
        match esito.appunti {
            Some(Appunti::Copia(testo)) => self.piattaforma.copia_testo(&testo),
            Some(Appunti::Incolla) => {
                // Gli appunti si leggono qui, subito dopo il tocco su Incolla: da Android 10
                // rispondono solo all'app che ha il fuoco.
                if let Some(testo) = self.piattaforma.incolla_testo()
                    && let Some(e) = self.editor_mut()
                {
                    let st = e.incolla(&testo);
                    self.dopo_ponte(st);
                }
            }
            None => {}
        }
    }

    /// Il nome del documento nella barra alta: senza l'estensione, come nella Home.
    fn nome_da_mostrare(&self) -> String {
        let nome = self.editor().map_or("", |e| e.nome.as_str());
        home::nome_da_mostrare(nome, tipo_da_nome(nome)).to_string()
    }

    /// Lo stato del documento sotto il nome, e se ha la spunta verde di «Salvato».
    fn stato_del_documento(&self) -> (String, bool) {
        let Some(a) = self.aperto.as_ref() else { return (String::new(), false) };
        if a.prova.is_some() {
            return ("Documento di prova".to_string(), false);
        }
        if a.editor.s.dirty {
            return ("Modifiche non salvate".to_string(), false);
        }
        match &a.editor.origine {
            Origine::File(p) => (format!("Salvato · {}", self.home.dove(&p.to_string_lossy())), true),
            // Arrivato da un'altra app: «Salva» ne fa una copia nel telefono.
            Origine::Uri { uri, .. } => (format!("Aperto {}", self.home.dove(uri)), false),
            Origine::Nuovo | Origine::Recuperato { .. } => ("Non ancora salvato".to_string(), false),
        }
    }

    /// La barra alta (`.barra-alta`); vero se si è toccato indietro.
    fn barra_alta(&mut self, ui: &mut egui::Ui) -> bool {
        let in_pagina = self.editor().is_some_and(|e| e.vista.modo == Modo::Pagina);
        // In Pagina, sola lettura, Annulla e Ripeti sono spenti.
        let annulla = self.editor().is_some_and(|e| e.s.can_undo()) && !in_pagina;
        let ripeti = self.editor().is_some_and(|e| e.s.can_redo()) && !in_pagina;
        let nome = self.nome_da_mostrare();
        let (stato, salvato) = self.stato_del_documento();
        let mut barra = BarraAlta::new(&nome).indietro(INDIETRO).sottotitolo(&stato);
        if salvato {
            barra = barra.stato(icone::CHECK_CIRCLE, tema::VERDE);
        }
        let r = barra
            .azione(PulsanteIcona::new(icone::UNDO, ANNULLA).spento(!annulla))
            .azione(PulsanteIcona::new(icone::REDO, RIPETI).spento(!ripeti))
            .azione(PulsanteIcona::new(icone::MORE_VERT, MENU))
            .mostra(ui);
        match r.azione {
            Some(i @ (0 | 1)) => {
                let id = if i == 0 { "edit.undo" } else { "edit.redo" };
                if let Some(e) = self.editor_mut() {
                    let st = e.comando(id);
                    self.dopo_ponte(st);
                }
            }
            Some(_) => {
                diagnosi("menu dell'editor: aperto");
                self.menu_editor = true;
            }
            None => {}
        }
        r.indietro
    }

    /// Trova al posto della barra alta, alta uguale: il campo a pillola con la lente e il
    /// conteggio («2 di 5»), che col tocco prende la tastiera; il risultato prima e quello dopo
    /// (spenti senza risultati); il pulsante di Trova e sostituisci, acceso con la seconda riga
    /// aperta; la X che chiude.
    fn barra_trova(&mut self, ui: &mut egui::Ui) {
        let ricerca = self.editor().map(|e| e.ricerca()).unwrap_or_default();
        let vuoto = self.trova.as_ref().is_none_or(|c| c.testo().is_empty());
        let conteggio = crate::conteggio_ricerca(ricerca, vuoto);
        let ci_sono = ricerca.conteggio > 0;
        let (barra, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), tema::BARRA_ALTA), egui::Sense::hover());
        let posto =
            |destra: f32| Rect::from_min_size(pos2(destra - tema::PULSANTE, barra.center().y - tema::PULSANTE / 2.0), Vec2::splat(tema::PULSANTE));
        let chiudi = posto(barra.right() - 4.0);
        let sostituisci = posto(chiudi.left());
        let dopo = posto(sostituisci.left());
        let prima = posto(dopo.left());
        let chiudi = ui.put(chiudi, PulsanteIcona::new(icone::CLOSE, CHIUDI_TROVA)).clicked();
        let aperta = self.sostituisci_con.is_some();
        let sostituisci = ui.put(sostituisci, PulsanteIcona::new(icone::FIND_REPLACE, TROVA_E_SOSTITUISCI).acceso(aperta)).clicked();
        let dopo = ui.put(dopo, PulsanteIcona::new(icone::KEYBOARD_ARROW_DOWN, DOPO).spento(!ci_sono)).clicked();
        let prima_r = ui.put(prima, PulsanteIcona::new(icone::KEYBOARD_ARROW_UP, PRIMA).spento(!ci_sono));
        let campo = Rect::from_min_max(
            pos2(barra.left() + TROVA_DA_SINISTRA, barra.center().y - tema::PULSANTE / 2.0),
            pos2(prima_r.rect.left() - 4.0, barra.center().y + tema::PULSANTE / 2.0),
        );
        let tocco_campo = match &self.trova {
            Some(c) => ui.scope_builder(UiBuilder::new().max_rect(campo), |ui| c.mostra_trova(ui, VUOTO_TROVA, &conteggio).clicked()).inner,
            None => false,
        };
        if tocco_campo {
            self.fuoco_al_campo();
        }
        if prima_r.clicked() {
            self.trova_altro(false);
        }
        if dopo {
            self.trova_altro(true);
        }
        // I pulsanti messi con `put` lasciano il cursore al loro fondo: quello che viene dopo
        // (la seconda riga) comincia sotto la barra.
        ui.advance_cursor_after_rect(barra);
        if sostituisci {
            self.apri_o_chiudi_sostituisci();
        }
        if chiudi {
            self.chiudi_trova();
        }
    }

    /// La seconda riga di Trova, sotto la barra: il campo «Sostituisci con» a pillola con la
    /// matita, che col tocco prende la tastiera, e i pulsanti Sostituisci e Tutti (spenti senza
    /// risultati); sotto, gli interruttori Maiuscole e Parola intera.
    fn riga_sostituisci(&mut self, ui: &mut egui::Ui) {
        let ci_sono = self.editor().is_some_and(|e| e.ricerca().conteggio > 0);
        let opzioni = self.editor().map(|e| e.opzioni_ricerca()).unwrap_or_default();
        let (riga, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), tema::PULSANTE + 8.0), egui::Sense::hover());
        // Alta quanto i pulsanti: in una riga più alta egui non li centra.
        let dentro = Rect::from_min_size(
            pos2(riga.left() + TROVA_DA_SINISTRA, riga.top() + 4.0),
            egui::vec2(riga.width() - TROVA_DA_SINISTRA - 4.0, tema::PULSANTE),
        );
        let (mut tocco_campo, mut uno, mut tutti) = (false, false, false);
        ui.scope_builder(UiBuilder::new().max_rect(dentro).layout(Layout::right_to_left(Align::Center)), |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            tutti = ui.add(Pulsante::new(TUTTI, Peso::Testo).spento(!ci_sono)).clicked();
            uno = ui.add(Pulsante::new(SOSTITUISCI, Peso::Testo).spento(!ci_sono)).clicked();
            ui.add_space(4.0);
            if let Some(c) = &self.sostituisci_con {
                tocco_campo = c.mostra_sostituisci(ui, VUOTO_SOSTITUISCI).clicked();
            }
        });
        let (mut maiuscole, mut parola) = (false, false);
        let (fila, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), tema::CHIP + 14.0), egui::Sense::hover());
        let fila = Rect::from_min_size(
            pos2(fila.left() + TROVA_DA_SINISTRA, fila.top() + 4.0),
            egui::vec2(fila.width() - TROVA_DA_SINISTRA - 4.0, tema::CHIP),
        );
        ui.scope_builder(UiBuilder::new().max_rect(fila).layout(Layout::left_to_right(Align::Center)), |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            maiuscole = ui.add(Chip::new(MAIUSCOLE).acceso(opzioni.maiuscole)).clicked();
            parola = ui.add(Chip::new(PAROLA_INTERA).acceso(opzioni.parola_intera)).clicked();
        });
        if tocco_campo {
            self.fuoco_al_sostituisci();
        }
        if uno {
            self.sostituisci_uno();
        }
        if tutti {
            self.sostituisci_tutti();
        }
        if maiuscole || parola {
            let mut o = opzioni;
            o.maiuscole ^= maiuscole;
            o.parola_intera ^= parola;
            self.cambia_opzioni_di_trova(o);
        }
    }

    /// Sotto la barra alta gli stati del documento, che restano finché durano (non sono esiti): in
    /// ocra la protezione di «Limita modifiche» di Word, in rosso l'avviso del ponte della tastiera
    /// (un paragrafo troppo lungo per scriverci). Gli errori vanno nel dialogo, gli esiti negli
    /// avvisi brevi.
    fn messaggi(&mut self, ui: &mut egui::Ui) {
        let Some(e) = self.editor() else { return };
        let righe: Vec<(String, egui::Color32)> =
            [e.protezione().map(|p| (p.to_string(), tema::OCRA)), e.ponte.avviso.clone().map(|a| (a, tema::ROSSO))].into_iter().flatten().collect();
        if righe.is_empty() {
            return;
        }
        let font = Carattere::Testo.font(ui.ctx(), 14.0);
        egui::Frame::NONE.inner_margin(Margin { left: tema::MARGINE_TITOLO as i8, right: tema::MARGINE_TITOLO as i8, top: 0, bottom: 6 }).show(
            ui,
            |ui| {
                for (testo, colore) in righe {
                    ui.label(RichText::new(testo).font(font.clone()).color(colore));
                }
            },
        );
    }

    /// La barra di formato flottante (`.formato`): «Aa», grassetto, corsivo, sottolineato, gli
    /// elenchi e «+», accesi col formato al cursore.
    fn barra_formato(&mut self, ui: &mut egui::Ui, posto: Rect, tastiera: bool) {
        let f = self.editor().map(|e| e.formato.clone()).unwrap_or_default();
        let accesi = [f.grassetto, f.corsivo, f.sottolineato, f.puntato, f.numerato];
        let mut barra = BarraFlottante::new().pulsante(PulsanteIcona::new("Aa", FORMATO).testo(Carattere::Titolo, 21.0)).separatore();
        for (i, ((icona, _, etichetta), acceso)) in BARRA.into_iter().zip(accesi).enumerate() {
            if i == 3 {
                barra = barra.separatore();
            }
            barra = barra.pulsante(PulsanteIcona::new(icona, etichetta).acceso(acceso));
        }
        barra = barra.pulsante(PulsanteIcona::new(icone::ADD_CIRCLE, INSERISCI));
        if tastiera {
            barra = barra.raggio(tema::RAGGIO_FORMATO_SULLA_TASTIERA);
        }
        match barra.mostra(ui, posto) {
            Some(0) => {
                diagnosi("foglio del formato: aperto");
                self.foglio_aa.apri();
            }
            Some(i) if i <= BARRA.len() => {
                if let Some(e) = self.editor_mut() {
                    let st = e.comando(BARRA[i - 1].1);
                    self.dopo_ponte(st);
                }
            }
            Some(_) => {
                diagnosi("inserisci: foglio aperto");
                self.foglio_inserisci.apri();
            }
            None => {}
        }
    }

    /// Il menu del bozzetto 6, sotto il suo pulsante, se è aperto.
    fn mostra_menu_editor(&mut self, ctx: &egui::Context, zona: Rect) {
        if !self.menu_editor {
            return;
        }
        let in_pagina = self.editor().is_some_and(|e| e.vista.modo == Modo::Pagina);
        let (icona_vista, vista) = if in_pagina { (icone::EDIT_NOTE, "Vista scrittura") } else { (icone::ARTICLE, "Vista pagina") };
        let data = self.voce_della_data();
        let scelta = Menu::new(egui::Id::new("menu_editor"))
            .primo(icone::SAVE, "Salva")
            .voce(icone::FILE_COPY, "Salva una copia")
            .voce(icone::PICTURE_AS_PDF, "Esporta in PDF")
            .voce(icone::SHARE, "Condividi")
            .voce(icone::SCHEDULE, data)
            .separatore()
            .voce(icone::FIND_IN_PAGE, "Trova")
            .voce(icone::TOC, "Indice")
            .voce(icona_vista, vista)
            .separatore()
            .voce(icone::MARGIN, crate::foglio_pagina::PAGINA)
            .voce(icone::TRACK_CHANGES, crate::foglio_revisioni::REVISIONI)
            .voce(icone::INFO, "Informazioni sul documento")
            .mostra(ctx, pos2(zona.right() - MENU_DESTRA, zona.top() + MENU_SOTTO));
        let Some(scelta) = scelta else { return };
        self.menu_editor = false;
        let SceltaMenu::Voce(i) = scelta else { return };
        match VOCI_MENU.get(i) {
            Some(VoceMenu::File(c)) => {
                self.esegui(*c);
            }
            Some(VoceMenu::Data) => self.data_dal_menu(ctx),
            Some(VoceMenu::Trova) => self.apri_trova(),
            Some(VoceMenu::Indice) => self.apri_indice(),
            Some(VoceMenu::Vista) => self.cambia_vista(),
            Some(VoceMenu::Pagina) => self.apri_foglio_pagina(),
            Some(VoceMenu::Revisioni) => self.apri_revisioni(),
            Some(VoceMenu::Informazioni) => self.apri_informazioni(),
            None => {}
        }
    }

    /// Il foglio «Inserisci»: la tabella (che apre la sua griglia), l'immagine (che chiede da
    /// dove), data e ora (con quella di adesso sotto), interruzione di pagina, linea orizzontale.
    /// Un tocco inserisce e chiude il foglio.
    pub(crate) fn mostra_foglio_inserisci(&mut self, ctx: &egui::Context) {
        if self.schermata != Schermata::Editor || self.aperto.is_none() {
            self.foglio_inserisci.chiudi();
        }
        if !self.foglio_inserisci.aperto() {
            self.tabella_scelta = None;
            self.fonti_immagine = false;
        }
        // Con le fonti dell'immagine aperte il foglio mostra loro.
        if self.fonti_immagine {
            if let Some(fonte) = self.foglio_inserisci.mostra(ctx, IMMAGINE, foglio_immagine::pagina_delle_fonti).flatten() {
                self.chiedi_immagine(fonte);
            }
            return;
        }
        // Con la griglia della tabella aperta il foglio mostra lei.
        if let Some(scelta) = self.tabella_scelta {
            match self.foglio_inserisci.mostra(ctx, TABELLA, |ui| foglio_tabella::pagina_della_griglia(ui, scelta)).flatten() {
                Some(NellaGriglia::Misura(m)) => self.scegli_misura_tabella(m),
                Some(NellaGriglia::Inserisci) => self.inserisci_tabella_scelta(),
                None => {}
            }
            return;
        }
        let adesso = data_e_ora_adesso();
        let (mut griglia, mut fonti) = (false, false);
        let scelto = self
            .foglio_inserisci
            .mostra(ctx, "Inserisci", |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let mut scelto = None;
                griglia = Voce::new(Tipo::Word, TABELLA).icona(icone::TABLE).sotto("Fino a 8 righe e 6 colonne, da scegliere").mostra(ui).aperta();
                fonti = Voce::new(Tipo::Word, IMMAGINE).icona(icone::IMAGE).sotto("Dalla galleria o dalla fotocamera").mostra(ui).aperta();
                for (i, (cosa, icona, nome)) in VOCI_INSERISCI.into_iter().enumerate() {
                    let sotto = match cosa {
                        Inserimento::DataOra => adesso.as_str(),
                        Inserimento::InterruzionePagina => "Il testo dopo il cursore va su una pagina nuova",
                        Inserimento::LineaOrizzontale => "Una linea sotto il paragrafo",
                    };
                    if Voce::new(Tipo::Word, nome).icona(icona).sotto(sotto).ultima(i + 1 == VOCI_INSERISCI.len()).mostra(ui).aperta() {
                        scelto = Some(cosa);
                    }
                }
                scelto
            })
            .flatten();
        if griglia {
            self.apri_griglia_tabella();
            return;
        }
        if fonti {
            self.apri_fonti_immagine();
            return;
        }
        let Some(cosa) = scelto else { return };
        self.foglio_inserisci.chiudi();
        if let Some(e) = self.editor_mut() {
            let st = e.inserisci(cosa);
            self.dopo_ponte(st);
        }
    }

    /// Il foglio dell'Indice: i titoli del documento, rientrati per livello, uno per riga; un
    /// tocco porta il cursore all'inizio del titolo (e la pagina fin lì) e chiude il foglio.
    pub(crate) fn mostra_foglio_indice(&mut self, ctx: &egui::Context) {
        if self.schermata != Schermata::Editor || self.aperto.is_none() {
            self.foglio_indice.chiudi();
        }
        let titoli = &self.indice;
        let scelto = self
            .foglio_indice
            .mostra(ctx, "Indice", |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                if titoli.is_empty() {
                    indice_vuoto(ui);
                }
                let mut scelto = None;
                for (i, t) in titoli.iter().enumerate() {
                    if riga_titolo(ui, t, i + 1 == titoli.len()).clicked() {
                        scelto = Some(i);
                    }
                }
                scelto
            })
            .flatten();
        if let Some(i) = scelto
            && let Some(pos) = self.indice.get(i).map(|t| t.pos.clone())
        {
            self.foglio_indice.chiudi();
            if let Some(e) = self.editor_mut() {
                let st = e.vai_al_titolo(&pos);
                self.dopo_ponte(st);
            }
        }
    }

    /// «Informazioni sul documento» dal menu: i conteggi di adesso e dove sta il file.
    fn apri_informazioni(&mut self) {
        let cartella = match self.aperto.as_ref().map(|a| &a.editor.origine) {
            Some(Origine::File(p)) => self.home.dove(&p.to_string_lossy()),
            Some(Origine::Uri { uri, .. }) => format!("Arrivato {}, non è salvato nel telefono", self.home.dove(uri)),
            Some(Origine::Nuovo | Origine::Recuperato { .. }) | None => "Non ancora salvato".to_string(),
        };
        let Some(e) = self.editor_mut() else { return };
        let conteggi = conteggi(&mut e.s);
        diagnosi(format!("informazioni: {} parole, {} pagine", conteggi.parole, conteggi.pagine));
        self.informazioni = Some(Informazioni { conteggi, cartella });
        self.foglio_info.apri();
    }

    /// Il foglio «Informazioni»: parole, caratteri, paragrafi, pagine e cartella, una riga l'uno.
    pub(crate) fn mostra_foglio_info(&mut self, ctx: &egui::Context) {
        if self.schermata != Schermata::Editor || self.aperto.is_none() {
            self.foglio_info.chiudi();
        }
        let Some(info) = self.informazioni.clone() else {
            self.foglio_info.chiudi();
            return;
        };
        let c = info.conteggi;
        let righe = [
            ("Parole", numero(c.parole)),
            ("Caratteri", format!("{}, {} senza spazi", numero(c.caratteri), numero(c.caratteri_senza_spazi))),
            ("Paragrafi", numero(c.paragrafi)),
            ("Pagine", numero(c.pagine)),
            ("Cartella", info.cartella),
        ];
        self.foglio_info.mostra(ctx, "Informazioni", |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for (i, (etichetta, valore)) in righe.iter().enumerate() {
                riga_informazione(ui, etichetta, valore, i + 1 == righe.len());
            }
        });
    }
}

/// Quanto rientra un titolo per ogni livello sotto il primo, in dp; oltre il quarto non rientra più.
const RIENTRO_TITOLO: f32 = 20.0;

/// Una riga dell'Indice, alta 48 dp e larga quanto il foglio: il titolo su una riga sola (coi
/// puntini se è lungo), rientrato per livello; il primo livello in grassetto, il terzo e oltre più
/// piccoli e grigi. La linea sottile sotto, se non è l'ultima.
fn riga_titolo(ui: &mut egui::Ui, t: &Titolo, ultima: bool) -> egui::Response {
    let ctx = ui.ctx().clone();
    let (carattere, misura, colore) = match t.livello {
        0 => (Carattere::Grassetto, 16.0, tema::INCHIOSTRO),
        1 => (Carattere::Testo, 16.0, tema::INCHIOSTRO),
        _ => (Carattere::Testo, 15.0, tema::MATITA),
    };
    let largo = ui.available_width();
    let rientro = 4.0 + RIENTRO_TITOLO * f32::from(t.livello.min(4));
    let mut job = egui::text::LayoutJob::single_section(t.testo.clone(), egui::TextFormat::simple(carattere.font(&ctx, misura), colore));
    job.wrap = egui::text::TextWrapping::truncate_at_width(largo - rientro - 8.0);
    let g = ui.painter().layout_job(job);
    let (rect, r) = ui.allocate_exact_size(egui::vec2(largo, tema::PULSANTE.max(g.size().y + 16.0)), egui::Sense::click());
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &t.testo));
    if r.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, CornerRadius::same(tema::RAGGIO_SEGMENTO), tema::PREMUTO);
    }
    ui.painter().galley(pos2(rect.left() + rientro, rect.center().y - g.size().y / 2.0), g, colore);
    if !ultima {
        ui.painter().rect_filled(Rect::from_min_max(pos2(rect.left(), rect.bottom() - 1.0), rect.max), 0.0, tema::FILO);
    }
    r
}

/// L'Indice di un documento senza titoli: l'icona grigia, «Nessun titolo» e come farne uno.
fn indice_vuoto(ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        ui.label(RichText::new(icone::TOC).font(Carattere::Icona.font(&ctx, tema::ICONA)).color(tema::MATITA));
        ui.label(RichText::new("Nessun titolo").font(Carattere::Grassetto.font(&ctx, 16.0)).color(tema::INCHIOSTRO));
    });
    ui.add_space(6.0);
    ui.label(
        RichText::new("I paragrafi con lo stile Titolo 1, Titolo 2 o Titolo 3 (dal foglio «Aa») compaiono qui.")
            .font(Carattere::Testo.font(&ctx, 15.0))
            .color(tema::MATITA),
    );
    ui.add_space(8.0);
}

/// Una riga di «Informazioni»: l'etichetta grigia a sinistra, il valore in grassetto a destra,
/// alta 48 dp, con la linea sottile sotto se non è l'ultima.
pub(crate) fn riga_informazione(ui: &mut egui::Ui, etichetta: &str, valore: &str, ultima: bool) {
    let ctx = ui.ctx().clone();
    let r = ui
        .horizontal(|ui| {
            ui.set_min_height(tema::PULSANTE);
            ui.label(RichText::new(etichetta).font(Carattere::Testo.font(&ctx, 15.0)).color(tema::MATITA));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add(egui::Label::new(RichText::new(valore).font(Carattere::Grassetto.font(&ctx, 15.0)).color(tema::INCHIOSTRO)).truncate());
            });
        })
        .response
        .rect;
    if !ultima {
        ui.painter().rect_filled(Rect::from_min_max(pos2(r.left(), r.bottom() - 1.0), r.max), 0.0, tema::FILO);
    }
}
