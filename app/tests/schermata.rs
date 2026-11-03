//! Schermate a misura di telefono (412 × 915 dp, densità 2,625, o quella di `TASCABILE_SCHERMO`,
//! come «372x827@3» per il telefono da 372 dp), disegnate con wgpu senza finestra.
//! Senza le barre di sistema: kittest misura l'immagine dalla zona libera e, con le barre, ne
//! taglierebbe il fondo; accanto ai bozzetti, che le hanno, tutto sta 28 dp più in alto.
//! Ci sono la Home con dei recenti finti, il suo foglio «Nuovo» e il menu di un recente, una
//! cartella, le Impostazioni; poi la checklist aperta dalle Impostazioni; poi
//! la relazione dei bozzetti 2, 3, 5 e 6 aperta dai recenti: l'editor, l'editor con la tastiera
//! (simulata dagli insets: l'immagine finisce dove comincia la tastiera), la selezione allargata
//! trascinando la goccia, il menu, il foglio «Inserisci», le informazioni, il foglio «Aa» (anche
//! con la tavolozza), l'Indice, la vista Pagina, Trova, il dialogo «Salvare le modifiche?» e
//! l'avviso del PDF pronto; poi, al giro dopo, il lavoro da recuperare; poi la Home senza
//! l'accesso ai file; infine il lettore dei PDF (aperto, la selezione con la lente, la ricerca,
//! l'indice, le miniature, il menu, le informazioni, lo zoom) e il dialogo della password. Le
//! immagini vanno in target/tmp/schermate/, da guardare accanto ai bozzetti di
//! design/bozzetti/png.

use std::path::Path;
use std::time::{Instant, SystemTime};

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::{Value, json};
use tascabile::{Piattaforma, TascabileApp};
use tascabile_word::documento::{self, DP_PER_PT};
use tascabile_word::vista::layout_scrittura;
use wordcraft_engine::Session;
use wordcraft_engine::doc::Pos;

/// La relazione dei bozzetti 2, 3, 5 e 6: (comando dello stile, testo) per paragrafo.
const RELAZIONE: [(&str, &str); 9] = [
    ("para.heading1", "Relazione turno di notte"),
    ("para.normal", "Turno dalle 21 alle 7, reparto di medicina. Presenti due infermieri e un operatore socio sanitario."),
    ("para.heading2", "Andamento del turno"),
    (
        "para.normal",
        "Notte regolare. Alle 2:30 rivalutazione del dolore al letto 12 con NRS 6, somministrata la terapia al bisogno come da prescrizione",
    ),
    ("para.bullets", "Due accessi venosi sostituiti"),
    ("", "Un prelievo urgente inviato al laboratorio"),
    ("", "Mobilizzazione ogni 2 ore ai letti 4 e 9"),
    ("para.heading2", "Da segnalare al mattino"),
    ("para.normal", "Controllo della glicemia alle 7 al letto 7."),
];

/// Il paragrafo della relazione col cursore (bozzetto 2) e la selezione (bozzetto 5).
const PARAGRAFO_DEL_CURSORE: usize = 3;

/// La relazione scritta dal motore, come .docx.
fn relazione() -> Vec<u8> {
    let mut s = documento::nuovo();
    let esegui = |s: &mut Session, id: &str, p: Value| {
        s.run(id, &p).unwrap_or_else(|e| panic!("{id}: {e}"));
    };
    for (i, (stile, testo)) in RELAZIONE.into_iter().enumerate() {
        if i > 0 {
            esegui(&mut s, "text.newParagraph", json!({}));
        }
        // Dopo l'elenco si torna al testo normale: il titolo spegne la voce.
        if stile == "para.heading2" && i > PARAGRAFO_DEL_CURSORE {
            esegui(&mut s, "para.bullets", json!({}));
        }
        if !stile.is_empty() {
            esegui(&mut s, stile, json!({}));
        }
        esegui(&mut s, "text.insert", json!({ "text": testo, "raw": true }));
    }
    wordcraft_engine::io::save_bytes("relazione.docx", &s.doc).expect("relazione in docx")
}

/// Dove sta sullo schermo il bordo in `pos` della relazione, appena aperta e non scorsa: la
/// pagina comincia sotto la barra alta (64 dp), a 10 dp dal bordo, larga lo schermo meno 20 dp
/// (392 a 412). Restituisce (x, cima, fondo) della riga, in dp.
fn bordo_nella_relazione(pos: Pos) -> (f32, f32, f32) {
    let mut s = documento::apri_byte("relazione.docx", &relazione()).unwrap();
    let l = layout_scrittura(&mut s, misura().x - 20.0, 1.0);
    let c = l.caret_on(&pos, 0).expect("posizione nel layout");
    (10.0 + c.x * DP_PER_PT, 64.0 + c.top * DP_PER_PT, 64.0 + (c.top + c.height) * DP_PER_PT)
}

/// La posizione nel paragrafo del cursore dove finisce (o comincia, con `inizio`) `parola`.
fn nel_paragrafo(parola: &str, inizio: bool) -> Pos {
    let testo = RELAZIONE[PARAGRAFO_DEL_CURSORE].1;
    let da = testo.find(parola).expect("parola nel paragrafo");
    Pos::body(PARAGRAFO_DEL_CURSORE, if inizio { da } else { da + parola.len() })
}

/// Le cartelle finte del telefono, con file vuoti dai nomi inventati: (cartella, quanti file,
/// nomi dei primi).
const CARTELLE: [(&str, usize, &[&str]); 4] = [
    (
        "Download",
        24,
        &[
            "Lezione lesioni da pressione.pptx",
            "Circolare ferie estive.pdf",
            "Modulo rimborso spese.pdf",
            "Turni di novembre.docx",
            "Foto della lavagna.jpg",
            "Procedura lavaggio mani.pdf",
        ],
    ),
    ("Documents", 12, &["Relazione turno notte.docx", "Verbale riunione di reparto.docx"]),
    ("Android/media/com.whatsapp/WhatsApp/Media/WhatsApp Documents", 38, &["Consenso informato da firmare.pdf"]),
    ("Download/Telegram", 5, &[]),
];

/// Prepara la radice finta e i recenti come nel bozzetto 1 (oggi, ieri, tre e quattro giorni fa).
fn telefono_finto(prova: &Path) {
    let radice = prova.join("radice");
    for (cartella, quanti, nomi) in CARTELLE {
        let c = radice.join(cartella);
        std::fs::create_dir_all(&c).unwrap();
        for i in 0..quanti {
            let nome = nomi.get(i).map_or_else(|| format!("Scansione {:02}.pdf", i + 1), |n| n.to_string());
            std::fs::write(c.join(nome), b"").unwrap();
        }
    }
    let adesso = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
    let giorno = 86_400;
    let recente = |chiave: String, nome: &str, tipo: &str, prima: u64| {
        format!(r#"{{"chiave":{chiave:?},"nome":{nome:?},"tipo":"{tipo}","aperto_alle":{}}}"#, adesso - prima)
    };
    let in_radice = |p: &str| radice.join(p).to_string_lossy().into_owned();
    // La relazione dei bozzetti dell'editor è un documento vero, da aprire.
    std::fs::write(radice.join("Documents/Relazione turno notte.docx"), relazione()).unwrap();
    let voci = [
        recente(in_radice("Documents/Relazione turno notte.docx"), "Relazione turno notte.docx", "Documento", 60),
        recente("content://com.whatsapp.provider.media/item/42".into(), "Consenso informato da firmare.pdf", "Pdf", giorno),
        recente(in_radice("Download/Lezione lesioni da pressione.pptx"), "Lezione lesioni da pressione.pptx", "Presentazione", 3 * giorno),
        recente(in_radice("Documents/Verbale riunione di reparto.docx"), "Verbale riunione di reparto.docx", "Documento", 4 * giorno),
        recente(in_radice("Download/Turni di novembre.docx"), "Turni di novembre.docx", "Documento", 5 * giorno),
    ];
    std::fs::create_dir_all(prova.join("app")).unwrap();
    std::fs::write(prova.join("app/recenti.json"), format!(r#"{{"voci":[{}]}}"#, voci.join(","))).unwrap();
}

/// Le prove a schermo disegnano con wgpu una alla volta: in parallelo, con nove insieme, il
/// processo si è fermato qualche volta con un SIGSEGV dentro il disegno senza finestra.
fn una_alla_volta() -> std::sync::MutexGuard<'static, ()> {
    static UNA: std::sync::Mutex<()> = std::sync::Mutex::new(());
    UNA.lock().unwrap_or_else(|e| e.into_inner())
}

/// Salva l'immagine in target/tmp/schermate e la restituisce.
/// La misura del telefono delle schermate, in dp, e la sua densità: 412 × 915 a 2,625, o quella
/// di `TASCABILE_SCHERMO`, scritta come «372x827@3» (il telefono da 372 dp), per rifarle tutte a
/// quella misura (0.8.3: Rifai finiva sotto Altra pagina solo sul suo telefono).
fn misura_e_densita() -> (egui::Vec2, f32) {
    std::env::var("TASCABILE_SCHERMO")
        .ok()
        .and_then(|v| {
            let (misura, densita) = v.split_once('@')?;
            let (w, h) = misura.split_once('x')?;
            Some((egui::vec2(w.parse().ok()?, h.parse().ok()?), densita.parse().ok()?))
        })
        .unwrap_or((egui::vec2(412.0, 915.0), 2.625))
}

fn misura() -> egui::Vec2 {
    misura_e_densita().0
}

fn densita() -> f32 {
    misura_e_densita().1
}

/// La larghezza in dp di una pagina A4 del lettore: lo schermo meno 16 dp per parte.
fn pagina_dp() -> f32 {
    misura().x - 32.0
}

/// Il punto dello schermo che sulla prima pagina del lettore sta a (`x`, `y`) dp di una pagina
/// larga 380 dp (quella dello schermo da 412), alla misura di oggi.
fn sulla_pagina(x: f32, y: f32) -> egui::Pos2 {
    let k = pagina_dp() / 380.0;
    egui::pos2(16.0 + x * k, 78.0 + y * k)
}

/// Dove vanno le immagini: target/tmp/schermate, o schermate-372x827-3 con `TASCABILE_SCHERMO`.
fn cartella_delle_schermate() -> std::path::PathBuf {
    let nome = match std::env::var("TASCABILE_SCHERMO") {
        Ok(v) => format!("schermate-{}", v.replace('@', "-")),
        Err(_) => "schermate".to_string(),
    };
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(nome);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn salva(h: &mut Harness<'_, TascabileApp>, nome: &str) -> image::RgbaImage {
    // Senza la freccia del mouse, che kittest disegna dov'è l'ultimo tocco.
    h.event(egui::Event::PointerGone);
    h.step();
    let dir = cartella_delle_schermate();
    std::fs::create_dir_all(&dir).unwrap();
    let img = h.render().expect("disegno wgpu senza finestra");
    img.save(dir.join(format!("{nome}.png"))).unwrap();
    img
}

fn passi(h: &mut Harness<'_, TascabileApp>, n: usize) {
    for _ in 0..n {
        h.step();
    }
}

/// Un evento del dito (tocco) insieme a quelli del puntatore che Android manda col primo dito.
fn dito(h: &mut Harness<'_, TascabileApp>, phase: egui::TouchPhase, pos: egui::Pos2) {
    h.event(egui::Event::Touch { device_id: egui::TouchDeviceId(1), id: egui::TouchId(0), phase, pos, force: None });
    h.event(egui::Event::PointerMoved(pos));
    let premuto = match phase {
        egui::TouchPhase::Start => Some(true),
        egui::TouchPhase::End | egui::TouchPhase::Cancel => Some(false),
        egui::TouchPhase::Move => None,
    };
    if let Some(pressed) = premuto {
        h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
    }
}

/// Un tocco breve in `pos`, solo con gli eventi del puntatore: kittest mette ogni evento del dito
/// in un passo suo (un quarto di secondo), e con quelli del tocco il dito resterebbe giù troppo.
fn tocca_in(h: &mut Harness<'_, TascabileApp>, pos: egui::Pos2) {
    h.event(egui::Event::PointerMoved(pos));
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
    }
    passi(h, 4);
}

/// La parte di schermo della tastiera finta, alta come quella del PC, negli insets.
fn insets_della_tastiera(h: &mut Harness<'_, TascabileApp>, aperta: bool) {
    let basso = if aperta { tascabile_platform::TASTIERA_FINTA_PT } else { 0.0 };
    h.input_mut().safe_area_insets = Some(egui::SafeAreaInsets(egui::epaint::MarginF32 { left: 0.0, right: 0.0, top: 0.0, bottom: basso }));
}

/// L'app sullo schermo del telefono finto. Con la tastiera kittest disegna solo la zona libera
/// (gli insets la tolgono): l'immagine finisce dove comincia la tastiera.
fn disegna(ui: &mut egui::Ui, app: &mut TascabileApp) {
    let ctx = ui.ctx().clone();
    app.passo_logica(&ctx);
    app.disegna(ui);
}

/// Quanti pixel dell'immagine hanno proprio questo colore.
fn pixel_di(img: &image::RgbaImage, colore: [u8; 3]) -> usize {
    img.pixels().filter(|p| p.0[..3] == colore).count()
}

#[test]
fn schermata_telefono() {
    let _una = una_alla_volta();
    // Cartelle della prova in target/: né i recenti né le cartelle vere del PC.
    let prova = Path::new(env!("CARGO_TARGET_TMPDIR")).join("prove-schermata");
    let _ = std::fs::remove_dir_all(&prova);
    telefono_finto(&prova);
    let piattaforma = Piattaforma::con_cartelle(prova.join("app"), prova.join("cache"), prova.join("radice"));
    let app = TascabileApp::new(piattaforma, Instant::now());
    let mut harness = Harness::builder().with_size(misura()).with_pixels_per_point(densita()).wgpu().build_ui_state(disegna, app);
    harness.run_steps(4);

    // La Home: tutta color carta, coi recenti nella scheda bianca e il «Nuovo» blu.
    harness.get_by_label("Doc Tascabile");
    harness.get_by_label("Relazione turno notte");
    let img = salva(&mut harness, "home");
    let (larghezza, altezza) = img.dimensions();
    let tutti = (larghezza * altezza) as usize;
    assert!(pixel_di(&img, [0xF6, 0xF3, 0xEE]) * 4 > tutti, "la Home non è color carta");
    assert!(pixel_di(&img, [0xFF, 0xFF, 0xFF]) > tutti / 8, "mancano le schede bianche");
    assert!(pixel_di(&img, [0x23, 0x46, 0xA0]) > 20_000, "manca il blu del «Nuovo»");
    // Il foglio «Nuovo», salito.
    harness.get_by_label("Nuovo").click();
    passi(&mut harness, 20);
    harness.get_by_label("Documento vuoto");
    salva(&mut harness, "home-nuovo");
    harness.key_press(egui::Key::BrowserBack);
    passi(&mut harness, 20);
    // Il menu del primo recente.
    harness.get_by_label("Altre azioni: Relazione turno notte").click();
    passi(&mut harness, 3);
    harness.get_by_label("Togli dai recenti");
    salva(&mut harness, "home-menu");
    harness.key_press(egui::Key::BrowserBack);
    passi(&mut harness, 3);

    // Una cartella: la barra alta col percorso e le voci.
    harness.get_by_label("Download").click();
    passi(&mut harness, 4);
    harness.get_by_label("Lezione lesioni da pressione.pptx");
    salva(&mut harness, "cartelle");
    harness.key_press(egui::Key::BrowserBack);
    passi(&mut harness, 4);

    // Le Impostazioni.
    harness.get_by_label("Impostazioni").click();
    passi(&mut harness, 4);
    harness.get_by_label("Registro degli eventi");
    salva(&mut harness, "impostazioni");
    // La Guida rapida, in cima: il foglio con le parti della guida; Indietro lo chiude.
    harness.get_by_label("Guida rapida").click();
    passi(&mut harness, 14);
    harness.get_by_label("I limiti di questa versione");
    salva(&mut harness, "impostazioni-guida");
    harness.key_press(egui::Key::BrowserBack);
    passi(&mut harness, 14);
    // Il blocco dell'app (sicurezza, Fase 1): solo la schermata del blocco.
    harness.state_mut().blocca_di_prova();
    passi(&mut harness, 4);
    harness.get_by_label("Tascabile è bloccata");
    assert!(harness.query_by_label("Registro degli eventi").is_none());
    salva(&mut harness, "bloccata");
    harness.state_mut().sblocca_di_prova();
    passi(&mut harness, 4);

    // La checklist, dalle prove per chi sviluppa. Due fasce nuove per frame: qualche frame per
    // riempire lo schermo.
    harness.get_by_label("Checklist della tastiera").click();
    passi(&mut harness, 8);
    let img = salva(&mut harness, "checklist");
    // Solo la fascia della pagina, dal 25% al 75% dell'altezza: testata e barra da sole hanno
    // centinaia di migliaia di pixel scuri, e con loro anche una pagina vuota passerebbe.
    let (larghezza, altezza) = img.dimensions();
    let (mut scuri, mut bianchi, mut tutti) = (0u32, 0u32, 0u32);
    for y in altezza / 4..altezza * 3 / 4 {
        for x in 0..larghezza {
            let [r, g, b, _] = img.get_pixel(x, y).0;
            tutti += 1;
            if r < 90 && g < 90 && b < 90 {
                scuri += 1;
            }
            if r > 240 && g > 240 && b > 240 {
                bianchi += 1;
            }
        }
    }
    assert!(bianchi * 2 > tutti, "la pagina non è bianca: {bianchi} pixel bianchi su {tutti}");
    // Sul PC la checklist ne ha circa 55.000; 5.000 sono qualche riga di testo.
    assert!(scuri > 5_000, "pagina senza testo scuro: {scuri} pixel scuri");
    // Dalla checklist si torna alla Home.
    harness.key_press(egui::Key::BrowserBack);
    passi(&mut harness, 4);

    // La relazione dei bozzetti dell'editor, dai recenti. Il cursore dopo «prescrizione», con un
    // tocco lì (la tastiera si apre) e Indietro (si chiude): bozzetto 2.
    harness.get_by_label("Relazione turno notte").click();
    passi(&mut harness, 8);
    harness.get_by_label("Salvato · Documenti");
    let (x, cima, fondo) = bordo_nella_relazione(nel_paragrafo("prescrizione", false));
    tocca_in(&mut harness, egui::pos2(x + 1.0, (cima + fondo) / 2.0));
    harness.key_press(egui::Key::BrowserBack);
    passi(&mut harness, 8);
    let img = salva(&mut harness, "editor");
    let (larghezza, altezza) = img.dimensions();
    let (mut scuri, mut bianchi, mut tutti) = (0u32, 0u32, 0u32);
    // Solo la pagina, fra la barra alta e la barra di formato: una pagina vuota non passerebbe.
    for y in altezza / 5..altezza * 3 / 5 {
        for x in larghezza / 8..larghezza * 7 / 8 {
            let [r, g, b, _] = img.get_pixel(x, y).0;
            tutti += 1;
            if r < 90 && g < 90 && b < 110 {
                scuri += 1;
            }
            if r > 240 && g > 240 && b > 240 {
                bianchi += 1;
            }
        }
    }
    assert!(bianchi * 2 > tutti, "la pagina non è bianca: {bianchi} pixel bianchi su {tutti}");
    assert!(scuri > 5_000, "pagina senza testo scuro: {scuri} pixel scuri");
    // La relazione sta tutta nello schermo: niente barra di scorrimento sul bordo della pagina.
    let bordo = (larghezza * 7 / 8..larghezza).filter(|&x| img.get_pixel(x, altezza / 2).0[0] < 160).count();
    assert_eq!(bordo, 0, "una barra di scorrimento sulla pagina corta");
    // Con la tastiera (bozzetto 3): un altro tocco lì la riapre, gli insets le fanno posto.
    tocca_in(&mut harness, egui::pos2(x + 1.0, (cima + fondo) / 2.0));
    insets_della_tastiera(&mut harness, true);
    passi(&mut harness, 20);
    harness.get_by_label("Grassetto");
    salva(&mut harness, "editor-tastiera");
    harness.key_press(egui::Key::BrowserBack);
    insets_della_tastiera(&mut harness, false);
    passi(&mut harness, 20);

    // La selezione (bozzetto 5): pressione lunga su «somministrata», poi la goccia della fine
    // trascinata dopo «bisogno».
    let (x0, _, _) = bordo_nella_relazione(nel_paragrafo("somministrata", true));
    let (x1, c1, f1) = bordo_nella_relazione(nel_paragrafo("somministrata", false));
    let p = egui::pos2((x0 + x1) / 2.0, (c1 + f1) / 2.0);
    dito(&mut harness, egui::TouchPhase::Start, p);
    passi(&mut harness, 60);
    dito(&mut harness, egui::TouchPhase::End, p);
    passi(&mut harness, 4);
    harness.get_by_label("Copia");
    let (x2, c2, f2) = bordo_nella_relazione(nel_paragrafo("bisogno", false));
    let presa = egui::pos2(x1 + 11.0, f1 + 11.0);
    let spostamento = egui::pos2(x2, (c2 + f2) / 2.0) - egui::pos2(x1, (c1 + f1) / 2.0);
    dito(&mut harness, egui::TouchPhase::Start, presa);
    harness.step();
    for k in 1..=10 {
        dito(&mut harness, egui::TouchPhase::Move, presa + spostamento * (k as f32 / 10.0));
        harness.step();
    }
    dito(&mut harness, egui::TouchPhase::End, presa + spostamento);
    passi(&mut harness, 6);
    harness.get_by_label("Tutto");
    salva(&mut harness, "selezione");
    harness.key_press(egui::Key::BrowserBack);
    passi(&mut harness, 4);

    // Il menu (bozzetto 6).
    harness.get_by_label("Menu").click();
    passi(&mut harness, 4);
    harness.get_by_label("Informazioni sul documento");
    salva(&mut harness, "menu");
    harness.key_press(egui::Key::BrowserBack);
    passi(&mut harness, 4);
    // Il foglio «Inserisci» dal «+».
    harness.get_by_label("Inserisci").click();
    passi(&mut harness, 20);
    harness.get_by_label("Interruzione di pagina");
    salva(&mut harness, "inserisci");
    harness.key_press(egui::Key::BrowserBack);
    passi(&mut harness, 20);
    // Le informazioni sul documento, dal menu.
    let menu = |harness: &mut Harness<'_, TascabileApp>, voce: &str| {
        harness.get_by_label("Menu").click();
        passi(harness, 3);
        harness.get_by_label(voce).click();
        passi(harness, 20);
    };
    menu(&mut harness, "Informazioni sul documento");
    harness.get_by_label("Parole");
    salva(&mut harness, "informazioni");
    harness.key_press(egui::Key::BrowserBack);
    passi(&mut harness, 20);

    // Il foglio «Aa» (bozzetto 4) col cursore dopo «prescrizione», poi con la tavolozza del
    // colore del testo aperta.
    tocca_in(&mut harness, egui::pos2(x + 1.0, (cima + fondo) / 2.0));
    harness.key_press(egui::Key::BrowserBack);
    passi(&mut harness, 8);
    harness.get_by_label("Formato").click();
    passi(&mut harness, 20);
    harness.get_by_label("Titolo 1");
    salva(&mut harness, "foglio_aa");
    harness.get_by_label("Colore del testo").click();
    passi(&mut harness, 8);
    harness.get_by_label("Rosso scuro");
    salva(&mut harness, "foglio_aa-colori");
    harness.key_press(egui::Key::BrowserBack);
    passi(&mut harness, 20);

    // L'Indice dal menu: il titolo e i due sottotitoli della relazione.
    menu(&mut harness, "Indice");
    harness.get_by_label("Da segnalare al mattino");
    salva(&mut harness, "indice");
    harness.key_press(egui::Key::BrowserBack);
    passi(&mut harness, 20);

    // La vista Pagina dal menu: pagine bianche sulla carta, col testo.
    menu(&mut harness, "Vista pagina");
    let img = salva(&mut harness, "pagina");
    let (larghezza, altezza) = img.dimensions();
    let (mut carta, mut scuri) = (0u32, 0u32);
    for y in altezza / 4..altezza * 3 / 4 {
        for x in 0..larghezza {
            let [r, g, b, _] = img.get_pixel(x, y).0;
            if [r, g, b] == [0xF6, 0xF3, 0xEE] {
                carta += 1;
            }
            if r < 90 && g < 90 && b < 90 {
                scuri += 1;
            }
        }
    }
    assert!(carta > 1_000, "niente carta attorno alla pagina: {carta} pixel");
    assert!(scuri > 2_000, "pagine senza testo: {scuri} pixel scuri");
    // Trova al posto della barra alta, con «letto» battuto nel campo dalla tastiera finta.
    menu(&mut harness, "Trova");
    harness.event(egui::Event::Text("letto".into()));
    passi(&mut harness, 8);
    harness.get_by_label("Chiudi Trova");
    salva(&mut harness, "trova");
    harness.get_by_label("2 di 2");
    // Indietro chiude la tastiera, poi Trova.
    for _ in 0..2 {
        harness.key_press(egui::Key::BrowserBack);
        passi(&mut harness, 8);
    }
    assert!(harness.query_by_label("Chiudi Trova").is_none());

    // Una modifica dopo «prescrizione», poi Indietro (la tastiera si chiude) e di nuovo Indietro:
    // il dialogo «Salvare le modifiche?».
    tocca_in(&mut harness, egui::pos2(x + 1.0, (cima + fondo) / 2.0));
    harness.event(egui::Event::Text(" orale".into()));
    passi(&mut harness, 4);
    harness.get_by_label("Modifiche non salvate");
    for _ in 0..2 {
        harness.key_press(egui::Key::BrowserBack);
        passi(&mut harness, 8);
    }
    harness.get_by_label("Salvare le modifiche?");
    salva(&mut harness, "dialogo-salvare");
    harness.get_by_label("Annulla").click();
    passi(&mut harness, 8);
    // L'avviso breve di «Esporta in PDF», con l'azione per condividerlo.
    menu(&mut harness, "Esporta in PDF");
    harness.get_by_label("PDF pronto in Documenti/Tascabile: Relazione turno notte.pdf");
    salva(&mut harness, "avviso");
    // L'app perde il fuoco con le modifiche: la sua copia privata. Poi si chiude senza salvare.
    harness.input_mut().focused = false;
    passi(&mut harness, 2);
    drop(harness);

    // Al giro dopo, nella Home, il dialogo del lavoro da recuperare; «Scarta» lo toglie.
    let piattaforma = Piattaforma::con_cartelle(prova.join("app"), prova.join("cache"), prova.join("radice"));
    let app = TascabileApp::new(piattaforma, Instant::now());
    let mut harness = Harness::builder().with_size(misura()).with_pixels_per_point(densita()).wgpu().build_ui_state(disegna, app);
    let fine = Instant::now() + std::time::Duration::from_secs(10);
    while harness.query_by_label("Lavoro da recuperare").is_none() {
        assert!(Instant::now() < fine, "il lavoro da recuperare non è comparso");
        harness.step();
    }
    passi(&mut harness, 4);
    harness.get_by_label_contains("«Relazione turno notte»");
    salva(&mut harness, "recupero");
    harness.get_by_label("Scarta").click();
    passi(&mut harness, 4);
    assert!(harness.query_by_label("Lavoro da recuperare").is_none());
    drop(harness);

    // La Home senza l'accesso ai file: il riquadro del permesso, senza le cartelle.
    let piattaforma = Piattaforma::con_cartelle(prova.join("app"), prova.join("cache"), prova.join("radice"));
    piattaforma.simula_accesso_file(false);
    let app = TascabileApp::new(piattaforma, Instant::now());
    let mut harness = Harness::builder().with_size(misura()).with_pixels_per_point(densita()).wgpu().build_ui_state(disegna, app);
    harness.run_steps(4);
    harness.get_by_label("Serve l'accesso ai file");
    assert!(harness.query_by_label("Cartelle").is_none());
    salva(&mut harness, "home-permesso");
    drop(harness);

    schermate_del_pdf(&prova);
    schermata_della_password(&prova);
    schermate_del_lavoro(&prova);
    schermate_del_modulo(&prova);
    schermate_della_firma(&prova);
    schermata_del_recupero(&prova);
    schermate_della_modifica(&prova);
    schermate_della_scansione(&prova);
    schermate_dell_ocr(&prova);
    schermate_delle_slide(&prova);
    schermate_dei_fogli(&prova);
    schermate_della_data(&prova);
}

/// Il «Timestamp con blockchain» (piano 2026-10-28-fissa-la-data, F4) su un PDF, coi server
/// simulati: il foglio che spiega (data-spiegazione.png) e l'esito con la prova accanto al file
/// (data-fissata.png). Poi, dallo stesso menu, il foglio della copia definitiva
/// (copia-definitiva.png).
fn schermate_della_data(prova: &Path) {
    use tascabile_platform::RispostaRete;
    let file = prova.join("radice/Download/Contratto da datare.pdf");
    std::fs::write(&file, tascabile_pdf::prova::pdf(&[tascabile_pdf::prova::Pagina::bianca(595.0, 842.0)])).unwrap();
    let _ = std::fs::remove_file(prova.join("radice/Download/Contratto da datare.pdf.ots"));
    let piattaforma = Piattaforma::con_cartelle(prova.join("app"), prova.join("cache"), prova.join("radice"));
    let attesa = tascabile_prova_data::prova::risposta_in_attesa("https://alice.btc.calendar.opentimestamps.org");
    piattaforma.simula_rete(move |indirizzo, corpo| {
        if corpo.is_some() && indirizzo.ends_with("/digest") { RispostaRete::Ok(attesa.clone()) } else { RispostaRete::Collegamento }
    });
    let app = TascabileApp::new(piattaforma, Instant::now());
    let mut h = Harness::builder().with_size(misura()).with_pixels_per_point(densita()).with_step_dt(1.0 / 30.0).wgpu().build_ui_state(disegna, app);
    h.step();
    h.state_mut().apri_pdf_dalla_cartella(&file);
    aspetta_il_pdf(&mut h);
    passi(&mut h, 20);
    h.get_by_label("Menu").click();
    passi(&mut h, 4);
    h.get_by_label("Timestamp con blockchain").click();
    passi(&mut h, 16);
    salva(&mut h, "data-spiegazione");
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Crea il timestamp").click();
    // Finito davvero: prima del primo passo il lavoro non è ancora partito, e «non in corso» da
    // solo passava subito (sotto carico la schermata non trovava l'esito).
    let ots = prova.join("radice/Download/Contratto da datare.pdf.ots");
    finche(&mut h, 30, "data fissata", |a| !a.data_in_corso() && ots.is_file());
    passi(&mut h, 16);
    h.get_by_label("Timestamp creato");
    salva(&mut h, "data-fissata");
    assert!(prova.join("radice/Download/Contratto da datare.pdf.ots").is_file());
    h.get_by_label("Chiudi").click();
    passi(&mut h, 16);
    h.get_by_label("Menu").click();
    passi(&mut h, 4);
    h.get_by_label("Copia definitiva").click();
    passi(&mut h, 16);
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Crea la copia definitiva");
    salva(&mut h, "copia-definitiva");
}

/// Le Slide (tappa G): la lezione di prova arrivata con «Apri con», col nome di uno dei recenti
/// della Home: la lettura, le miniature, le note della seconda slide.
/// I fogli di calcolo in sola lettura (0.9.0): foglio.png con la griglia, le intestazioni, la
/// riga unita in grassetto, la data all'italiana e le schede; foglio-cella.png col pannello della
/// cella toccata (la somma, con la formula); foglio-zoom.png ingrandito.
fn schermate_dei_fogli(prova: &Path) {
    let file = prova.join("radice/Download/Turni del reparto.xlsx");
    std::fs::write(&file, tascabile_foglio::prova::cartella_di_prova()).unwrap();
    let piattaforma = Piattaforma::con_cartelle(prova.join("app"), prova.join("cache"), prova.join("radice"));
    let app = TascabileApp::new(piattaforma, Instant::now());
    let mut h = Harness::builder().with_size(misura()).with_pixels_per_point(densita()).with_step_dt(1.0 / 30.0).wgpu().build_ui_state(disegna, app);
    h.step();
    h.state_mut().apri_pdf_dalla_cartella(&file);
    finche(&mut h, 20, "il foglio di calcolo", |a| a.foglio_aperto().is_some());
    passi(&mut h, 4);
    h.get_by_label("Turni del reparto");
    h.get_by_label("Pazienti");
    salva(&mut h, "foglio");
    let griglia = h.get_by_label("Griglia del foglio").rect();
    tocca_in(&mut h, griglia.min + egui::vec2(2.5 * 64.0, 4.5 * 20.0));
    passi(&mut h, 3);
    h.get_by_label("74,3");
    salva(&mut h, "foglio-cella");
    h.get_by_label("Chiudi la cella").click();
    passi(&mut h, 2);
    // Il dito scorre giù: le due righe bloccate restano in alto (foglio-scorre.png).
    let partenza = griglia.center();
    h.event(egui::Event::PointerMoved(partenza));
    h.event(egui::Event::PointerButton { pos: partenza, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
    h.step();
    for i in 1..=10 {
        h.event(egui::Event::PointerMoved(partenza + egui::vec2(-6.0 * i as f32, -12.0 * i as f32)));
        h.step();
    }
    h.event(egui::Event::PointerButton { pos: partenza + egui::vec2(-60.0, -120.0), button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
    passi(&mut h, 3);
    salva(&mut h, "foglio-scorre");
    // Il pizzico: due dita che si allontanano.
    let centro = griglia.center();
    h.event(egui::Event::Zoom(1.8));
    h.event(egui::Event::PointerMoved(centro));
    passi(&mut h, 3);
    salva(&mut h, "foglio-zoom");
}

fn schermate_delle_slide(prova: &Path) {
    let file = prova.join("radice/Download/Lezione lesioni da pressione.pptx");
    std::fs::write(&file, tascabile_slide::prova::LEZIONE).unwrap();
    let piattaforma = Piattaforma::con_cartelle(prova.join("app"), prova.join("cache"), prova.join("radice"));
    let mime = "application/vnd.openxmlformats-officedocument.presentationml.presentation";
    piattaforma.simula_apri_con_col_nome(file.to_str().unwrap(), Some(mime), "Lezione lesioni da pressione.pptx");
    let app = TascabileApp::new(piattaforma, Instant::now());
    let mut h = Harness::builder().with_size(misura()).with_pixels_per_point(densita()).with_step_dt(1.0 / 30.0).wgpu().build_ui_state(disegna, app);
    h.step();
    aspetta_le_slide(&mut h);
    h.get_by_label("Lezione lesioni da pressione");
    h.get_by_label("Slide · 4 slide");
    h.get_by_label("1 / 4");
    let img = salva(&mut h, "slide");
    // Il tavolo attorno, le slide bianche, il titolo scuro «Lezione di prova».
    let tutti = (img.width() * img.height()) as usize;
    assert!(pixel_di(&img, [0xE9, 0xE5, 0xDE]) > tutti / 50, "manca il tavolo");
    assert!(pixel_di(&img, [0xFF, 0xFF, 0xFF]) > tutti / 4, "mancano le slide bianche");
    let scuri = img.pixels().filter(|p| p.0[0] < 90 && p.0[1] < 90 && p.0[2] < 90).count();
    assert!(scuri > 2_000, "slide senza testo: {scuri} pixel scuri");
    // Le miniature, con la slide nascosta velata.
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Miniature").click();
    passi(&mut h, 3);
    aspetta_le_slide(&mut h);
    passi(&mut h, 8);
    salva(&mut h, "slide-miniature");
    // La seconda slide dalla griglia, poi le sue note.
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Slide 2").click();
    passi(&mut h, 3);
    aspetta_le_slide(&mut h);
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Note").click();
    passi(&mut h, 8);
    h.get_by_label("Nota per il relatore");
    salva(&mut h, "slide-note");
    // La presentazione dalla seconda slide: lo schermo nero attorno, la barra, la penna, il laser.
    h.key_press(egui::Key::BrowserBack);
    passi(&mut h, 8);
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Presenta").click();
    passi(&mut h, 3);
    aspetta_la_presentazione(&mut h);
    let img = salva(&mut h, "presentazione");
    let tutti = (img.width() * img.height()) as usize;
    assert!(pixel_di(&img, [0, 0, 0]) > tutti / 2, "manca il nero attorno alla slide");
    tocca_in(&mut h, egui::pos2(206.0, 890.0));
    passi(&mut h, 6);
    h.get_by_label("Penna");
    salva(&mut h, "presentazione-barra");
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Penna").click();
    passi(&mut h, 3);
    // Un tocco fuori dalla barra la chiude.
    tocca_in(&mut h, egui::pos2(206.0, 300.0));
    passi(&mut h, 3);
    assert!(h.query_by_label("Penna").is_none(), "la barra è rimasta aperta");
    tratto(&mut h, egui::pos2(60.0, 420.0), 12, 20.0);
    passi(&mut h, 3);
    salva(&mut h, "presentazione-penna");
    // Il laser: la penna spenta e il dito fermo un attimo.
    tocca_in(&mut h, egui::pos2(206.0, 890.0));
    passi(&mut h, 3);
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Penna").click();
    passi(&mut h, 3);
    tocca_in(&mut h, egui::pos2(206.0, 300.0));
    passi(&mut h, 3);
    let punto = egui::pos2(250.0, 470.0);
    h.event(egui::Event::PointerMoved(punto));
    h.event(egui::Event::PointerButton { pos: punto, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
    passi(&mut h, 20);
    let img = salva_senza_alzare(&mut h, "presentazione-laser");
    let rossi = img.pixels().filter(|p| p.0[0] > 230 && p.0[1] < 80 && p.0[2] < 80).count();
    assert!(rossi > 50, "manca il laser: {rossi} pixel rossi");
    drop(h);
    schermata_della_presentazione_orizzontale(prova, &file);
}

/// La presentazione col telefono girato (915 × 412 dp): la slide alta quanto lo schermo, il nero
/// ai lati, la slide 2 con due punti su tre.
fn schermata_della_presentazione_orizzontale(prova: &Path, file: &Path) {
    let piattaforma = Piattaforma::con_cartelle(prova.join("app"), prova.join("cache"), prova.join("radice"));
    let mime = "application/vnd.openxmlformats-officedocument.presentationml.presentation";
    piattaforma.simula_apri_con_col_nome(file.to_str().unwrap(), Some(mime), "Lezione lesioni da pressione.pptx");
    let app = TascabileApp::new(piattaforma, Instant::now());
    let mut h = Harness::builder()
        .with_size(egui::vec2(misura().y, misura().x))
        .with_pixels_per_point(densita())
        .with_step_dt(1.0 / 30.0)
        .wgpu()
        .build_ui_state(disegna, app);
    h.step();
    aspetta_le_slide(&mut h);
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Miniature").click();
    passi(&mut h, 6);
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Slide 2").click();
    passi(&mut h, 3);
    aspetta_le_slide(&mut h);
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Presenta").click();
    passi(&mut h, 3);
    aspetta_la_presentazione(&mut h);
    for _ in 0..2 {
        h.key_press(egui::Key::PageDown);
        passi(&mut h, 2);
    }
    aspetta_la_presentazione(&mut h);
    let img = salva(&mut h, "presentazione-orizzontale");
    // Alta 412 dp e larga 732,4: ai lati restano 91 dp di nero per parte.
    let nero_a_sinistra = (0..img.height()).all(|y| img.get_pixel(100, y).0[..3] == [0, 0, 0]);
    assert!(nero_a_sinistra, "la slide non sta in mezzo col nero ai lati");
    // Il relatore: le note della slide e il passo dopo (il terzo punto).
    tocca_in(&mut h, egui::pos2(457.0, 395.0));
    passi(&mut h, 4);
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Note").click();
    passi(&mut h, 4);
    tocca_in(&mut h, egui::pos2(457.0, 30.0));
    passi(&mut h, 4);
    h.get_by_label("Nota per il relatore");
    salva(&mut h, "presentazione-relatore");
    // La griglia per saltare, con la slide che si presenta segnata.
    tocca_in(&mut h, egui::pos2(457.0, 395.0));
    passi(&mut h, 4);
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Slide").click();
    passi(&mut h, 6);
    aspetta_le_slide(&mut h);
    passi(&mut h, 8);
    salva(&mut h, "presentazione-griglia");
}

/// Passi finché la presentazione non ha il suo fotogramma, al massimo 20 s.
fn aspetta_la_presentazione(h: &mut Harness<'_, TascabileApp>) {
    let fine = Instant::now() + std::time::Duration::from_secs(20);
    while h.state().presentazione_in_attesa() != Some(false) {
        assert!(Instant::now() < fine, "la presentazione non è partita in 20 s");
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.step();
    }
    passi(h, 2);
}

/// Come `salva`, ma col dito ancora giù: il laser c'è solo così.
fn salva_senza_alzare(h: &mut Harness<'_, TascabileApp>, nome: &str) -> image::RgbaImage {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("schermate");
    std::fs::create_dir_all(&dir).unwrap();
    let img = h.render().expect("disegno wgpu senza finestra");
    img.save(dir.join(format!("{nome}.png"))).unwrap();
    img
}

/// Passi finché la presentazione non è aperta e le slide sullo schermo (e le miniature, se il
/// foglio è aperto) non sono disegnate, al massimo 20 s.
fn aspetta_le_slide(h: &mut Harness<'_, TascabileApp>) {
    let fine = Instant::now() + std::time::Duration::from_secs(20);
    while h.state().slide_in_apertura() || h.state().slide_in_attesa() != Some(false) {
        assert!(Instant::now() < fine, "le slide non sono pronte in 20 s");
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.step();
    }
    passi(h, 2);
}

/// Un tratto del dito da `da`, verso destra, con gli eventi del puntatore (un'onda alta `alto`).
fn tratto(h: &mut Harness<'_, TascabileApp>, da: egui::Pos2, quanti: usize, alto: f32) {
    let premi = |h: &mut Harness<'_, TascabileApp>, pos, pressed| {
        h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE })
    };
    h.event(egui::Event::PointerMoved(da));
    premi(h, da, true);
    h.step();
    let mut p = da;
    for i in 1..=quanti {
        let t = i as f32;
        p = da + egui::vec2(9.0 * t, alto * (t * 0.45).sin() - 1.2 * t);
        h.event(egui::Event::PointerMoved(p));
        h.step();
    }
    premi(h, p, false);
    passi(h, 2);
}

/// La firma (piano «PDF completo», P3) sul modulo da compilare: pdf-firma-dito.png col foglio a
/// tutto schermo e una firma col dito; pdf-firma-posata.png con la firma nel campo firma, scelta;
/// pdf-firma-foto.png con la firma pulita dalla foto di prova di una firma su carta, da usare.
fn schermate_della_firma(prova: &Path) {
    let file = prova.join("radice/Download/Consenso informato da firmare.pdf");
    std::fs::write(&file, tascabile_pdf::prova::modulo_da_compilare()).unwrap();
    let _ = std::fs::remove_file(prova.join("app/posizioni-pdf.json"));
    let _ = std::fs::remove_dir_all(prova.join("app/firme"));
    let piattaforma = Piattaforma::con_cartelle(prova.join("app"), prova.join("cache"), prova.join("radice"));
    let app = TascabileApp::new(piattaforma, Instant::now());
    let mut h = Harness::builder().with_size(misura()).with_pixels_per_point(densita()).with_step_dt(1.0 / 30.0).wgpu().build_ui_state(disegna, app);
    h.step();
    h.state_mut().apri_pdf_dalla_cartella(&file);
    aspetta_il_pdf(&mut h);
    passi(&mut h, 40);
    h.get_by_label("Firma del paziente o del tutore").click();
    passi(&mut h, 15);
    h.get_by_label("Firma col dito").click();
    passi(&mut h, 4);
    // Nome e cognome inventati, in due tratti, e la sottolineatura: con la stilografica nera, poi
    // la penna e il pennarello blu (ritocchi della 0.8.2, R2), e di nuovo la stilografica nera.
    tratto(&mut h, egui::pos2(70.0, 420.0), 14, 34.0);
    tratto(&mut h, egui::pos2(200.0, 400.0), 16, 26.0);
    tratto(&mut h, egui::pos2(80.0, 500.0), 30, 3.0);
    let nero = tascabile_pdf::tratto::Colore::Nero.rgb();
    let img = salva(&mut h, "pdf-firma-dito");
    assert!(pixel_di(&img, nero) > 1_500, "manca l'inchiostro della firma");
    h.get_by_label("Penna").click();
    passi(&mut h, 2);
    let penna = pixel_di(&salva(&mut h, "pdf-firma-dito-penna"), nero);
    h.get_by_label("Pennarello").click();
    h.get_by_label("Blu").click();
    passi(&mut h, 2);
    let img = salva(&mut h, "pdf-firma-dito-pennarello-blu");
    let blu = pixel_di(&img, tascabile_pdf::tratto::Colore::Blu.rgb());
    assert!(blu * 10 > 16 * penna, "pennarello blu {blu} pixel, penna nera {penna}");
    // Nel riquadro della firma (sopra le file in fondo, dove c'è il campione del nero) niente nero.
    let riquadro = image::imageops::crop_imm(&img, 0, 0, img.width(), img.height() * 3 / 4).to_image();
    assert_eq!(pixel_di(&riquadro, nero), 0, "col blu niente nero");
    h.get_by_label("Stilografica").click();
    h.get_by_label("Nero").click();
    passi(&mut h, 2);
    h.get_by_label("Fatto").click();
    passi(&mut h, 4);
    h.state_mut().aspetta_il_motore_pdf();
    aspetta_le_immagini(&mut h);
    passi(&mut h, 10);
    assert!(h.query_by_label("Firma del paziente o del tutore").is_none());
    h.get_by_label("Firma posata");
    salva(&mut h, "pdf-firma-posata");
    // La foto della firma su carta, da un punto vuoto sotto il campo.
    let dove = sulla_pagina(200.0, 470.0);
    tocca_in(&mut h, dove);
    if h.query_by_label("Spunta").is_none() {
        passi(&mut h, 15);
        tocca_in(&mut h, dove);
    }
    h.get_by_label("Firma").click();
    passi(&mut h, 15);
    h.get_by_label("Foto della firma su carta").click();
    passi(&mut h, 4);
    h.state().simula_foto_firma_di_prova(Some(&tascabile_pdf::prova::foto_di_una_firma(2400, 1800, 85)));
    let fine = Instant::now() + std::time::Duration::from_secs(20);
    while h.query_by_label("Usa questa firma").is_none() {
        assert!(Instant::now() < fine, "la foto non è stata pulita in 20 s");
        std::thread::sleep(std::time::Duration::from_millis(10));
        h.step();
    }
    passi(&mut h, 20);
    salva(&mut h, "pdf-firma-foto");
    h.get_by_label("Usa questa firma").click();
    passi(&mut h, 4);
    h.state_mut().aspetta_il_motore_pdf();
    // Via senza salvare.
    for _ in 0..3 {
        h.key_press(egui::Key::BrowserBack);
        passi(&mut h, 6);
        if h.query_by_label("Non salvare").is_some() {
            break;
        }
    }
    h.get_by_label("Non salvare").click();
    passi(&mut h, 6);
}

/// Il modo Compila (piano «PDF completo», P2), col consenso del bozzetto 7 fatto modulo vero:
/// pdf-compila.png com'è aperto (Compila acceso, i campi evidenziati, «2 campi da compilare»);
/// pdf-campo-tastiera.png con «residente in» che si scrive e la tastiera aperta (l'immagine finisce
/// dove comincia); pdf-elenco.png col foglio del menu del reparto; pdf-compila-ovunque.png con
/// Fill & Sign su un PDF senza campi: una data e una spunta posate e la proposta in un punto vuoto.
fn schermate_del_modulo(prova: &Path) {
    let file = prova.join("radice/Download/Consenso informato da firmare.pdf");
    std::fs::write(&file, tascabile_pdf::prova::modulo_da_compilare()).unwrap();
    let _ = std::fs::remove_file(prova.join("app/posizioni-pdf.json"));
    let piattaforma = Piattaforma::con_cartelle(prova.join("app"), prova.join("cache"), prova.join("radice"));
    let app = TascabileApp::new(piattaforma, Instant::now());
    let mut h = Harness::builder().with_size(misura()).with_pixels_per_point(densita()).with_step_dt(1.0 / 30.0).wgpu().build_ui_state(disegna, app);
    h.step();
    h.state_mut().apri_pdf_dalla_cartella(&file);
    aspetta_il_pdf(&mut h);
    passi(&mut h, 40);
    h.get_by_label("PDF · 3 pagine · 2 campi da compilare");
    h.get_by_label("Compila");
    let img = salva(&mut h, "pdf-compila");
    // Il blu dei bordi dei campi pieni e il fondo rosso tenue di Compila acceso.
    assert!(pixel_di(&img, [0x23, 0x46, 0xA0]) > 2_000, "mancano i bordi blu dei campi");
    assert!(pixel_di(&img, [0xF7, 0xE4, 0xDF]) > 3_000, "manca il fondo del modo acceso");
    // «residente in» con la tastiera: la barra della scrittura sulla tastiera.
    h.get_by_label("Residente in").click();
    passi(&mut h, 2);
    h.event(egui::Event::Text("Via dei Mille 12".into()));
    passi(&mut h, 2);
    insets_della_tastiera(&mut h, true);
    passi(&mut h, 20);
    h.get_by_label("Campo dopo");
    h.get_by_label("Fatto");
    salva(&mut h, "pdf-campo-tastiera");
    h.get_by_label("Fatto").click();
    insets_della_tastiera(&mut h, false);
    passi(&mut h, 6);
    h.state_mut().aspetta_il_motore_pdf();
    aspetta_le_immagini(&mut h);
    // Il menu del reparto, nella seconda pagina: il foglio delle scelte.
    h.state_mut().tocca_campo_pdf_di_prova("Reparto", 0);
    passi(&mut h, 12);
    h.get_by_label("Cardiologia");
    salva(&mut h, "pdf-elenco");
    h.key_press(egui::Key::BrowserBack);
    passi(&mut h, 12);
    h.key_press(egui::Key::BrowserBack);
    passi(&mut h, 6);
    h.get_by_label("Non salvare").click();
    passi(&mut h, 6);
    // Fill & Sign sul consenso disegnato, senza campi.
    let senza = prova.join("radice/Download/Consenso da firmare a mano.pdf");
    std::fs::write(&senza, tascabile_pdf::prova::modulo()).unwrap();
    h.state_mut().apri_pdf_dalla_cartella(&senza);
    aspetta_il_pdf(&mut h);
    passi(&mut h, 40);
    h.get_by_label("Compila").click();
    passi(&mut h, 4);
    // La data accanto a «Luogo», poi una spunta sulla casella del «non acconsente».
    // Il primo tocco lascia il segno scelto; il secondo, dopo mezzo secondo (prima sarebbe un
    // doppio tocco, che ingrandisce), propone.
    let tocco_vuoto = |h: &mut Harness<'_, TascabileApp>, dove: egui::Pos2| {
        tocca_in(h, dove);
        if h.query_by_label("Spunta").is_none() {
            passi(h, 15);
            tocca_in(h, dove);
        }
    };
    tocco_vuoto(&mut h, sulla_pagina(254.0, 283.0));
    h.get_by_label("Data").click();
    passi(&mut h, 4);
    h.state_mut().aspetta_il_motore_pdf();
    tocco_vuoto(&mut h, sulla_pagina(34.5, 255.0));
    h.get_by_label("Spunta").click();
    passi(&mut h, 4);
    h.state_mut().aspetta_il_motore_pdf();
    aspetta_le_immagini(&mut h);
    passi(&mut h, 4);
    // Un punto vuoto sotto il riquadro della firma: la proposta.
    tocco_vuoto(&mut h, sulla_pagina(120.0, 420.0));
    passi(&mut h, 4);
    h.get_by_label("Testo");
    h.get_by_label("Croce");
    salva(&mut h, "pdf-compila-ovunque");
    // Via senza salvare: la proposta si chiude, poi «Non salvare».
    h.key_press(egui::Key::BrowserBack);
    passi(&mut h, 4);
    h.key_press(egui::Key::BrowserBack);
    passi(&mut h, 6);
    h.get_by_label("Non salvare").click();
    passi(&mut h, 6);
}

/// Una lettera di prova per il modo Modifica: righe in Helvetica e una riga coi dati da
/// oscurare (inventati).
fn lettera_da_modificare() -> Vec<u8> {
    let contenuto = "BT /F2 18 Tf 60 760 Td (Relazione di dimissione) Tj ET\n\
                     BT /F1 12 Tf 60 710 Td (Il paziente e' stato ricoverato dal 12 al 18 ottobre.) Tj ET\n\
                     BT /F1 12 Tf 60 670 Td (Terapia: proseguire la medicazione ogni due giorni.) Tj ET\n\
                     BT /F1 12 Tf 60 610 Td (Codice fiscale: RSSMRA80A01H501U) Tj ET\n\
                     BT /F1 12 Tf 60 570 Td (Telefono: 333 000 0000) Tj ET\n\
                     BT /F1 12 Tf 60 480 Td (Il coordinatore infermieristico) Tj ET";
    tascabile_pdf::prova::pdf(&[tascabile_pdf::prova::Pagina { larghezza: 595.0, altezza: 842.0, rotazione: 0, contenuto }])
}

/// Passi e attese finché gli oggetti della pagina non soddisfano `fatto`, al massimo 20 s.
fn aspetta_gli_oggetti(
    h: &mut Harness<'_, TascabileApp>,
    cosa: &str,
    fatto: impl Fn(&[(tascabile_pdf::GenereOggetto, String, egui::Rect)]) -> bool,
) -> Vec<(tascabile_pdf::GenereOggetto, String, egui::Rect)> {
    let fine = Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let o = h.state_mut().oggetti_pdf_di_prova(0);
        if fatto(&o) {
            passi(h, 6);
            return h.state_mut().oggetti_pdf_di_prova(0);
        }
        assert!(Instant::now() < fine, "{cosa}: non è successo in 20 s");
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.step();
    }
}

/// Il dito appoggiato in `da`, mosso di `delta` in dieci passi e alzato.
fn trascina(h: &mut Harness<'_, TascabileApp>, da: egui::Pos2, delta: egui::Vec2) {
    h.event(egui::Event::PointerMoved(da));
    h.event(egui::Event::PointerButton { pos: da, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
    h.step();
    for i in 1..=10 {
        h.event(egui::Event::PointerMoved(da + delta * (i as f32 / 10.0)));
        h.step();
    }
    let a = da + delta;
    h.event(egui::Event::PointerButton { pos: a, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
    passi(h, 2);
}

/// La correzione di una scansione (0.8.8, segnalazioni dell'autore del 3/11): pdf-scansione-scrive.png
/// con la riga riconosciuta che si scrive sulla pagina, alla sua misura, col cursore e la
/// tastiera aperta; pdf-scansione-corretta.png con la riga corretta nella foto, nel carattere della
/// scansione.
fn schermate_della_scansione(prova: &Path) {
    use tascabile_pdf::GenereOggetto;
    let file = prova.join("radice/Download/Scansione.pdf");
    std::fs::write(&file, tascabile_pdf::prova::scansione_riconosciuta_su_carta()).unwrap();
    let _ = std::fs::remove_file(prova.join("app/posizioni-pdf.json"));
    let piattaforma = Piattaforma::con_cartelle(prova.join("app"), prova.join("cache"), prova.join("radice"));
    let app = TascabileApp::new(piattaforma, Instant::now());
    let mut h = Harness::builder().with_size(misura()).with_pixels_per_point(densita()).with_step_dt(1.0 / 30.0).wgpu().build_ui_state(disegna, app);
    h.step();
    h.state_mut().apri_pdf_dalla_cartella(&file);
    aspetta_il_pdf(&mut h);
    passi(&mut h, 40);
    h.get_by_label("Modifica").click();
    passi(&mut h, 4);
    let oggetti = aspetta_gli_oggetti(&mut h, "il testo della scansione", |o| o.iter().any(|o| o.0 == GenereOggetto::Scansione));
    let riga = oggetti.iter().find(|o| o.0 == GenereOggetto::Scansione && o.1.starts_with("Nove")).expect("la riga riconosciuta").2;
    tocca_in(&mut h, riga.center());
    h.event(egui::Event::Text(" circa".into()));
    passi(&mut h, 2);
    insets_della_tastiera(&mut h, true);
    passi(&mut h, 20);
    h.get_by_label("Testo che scrivi");
    salva(&mut h, "pdf-scansione-scrive");
    h.get_by_label("Fatto").click();
    insets_della_tastiera(&mut h, false);
    passi(&mut h, 6);
    aspetta_gli_oggetti(&mut h, "la riga corretta", |o| o.iter().any(|o| o.0 == GenereOggetto::Scansione && o.1.ends_with("fa circa")));
    aspetta_le_immagini(&mut h);
    passi(&mut h, 10);
    salva(&mut h, "pdf-scansione-corretta");
}

/// Il modo Modifica (piano «PDF completo», P7) su una lettera: pdf-modifica-testo.png con una
/// riga che si riscrive e la tastiera aperta (l'immagine finisce dove comincia); pdf-sposta.png
/// con un testo aggiunto spostato col dito, scelto, con la maniglia e la sua barra;
/// pdf-immagine.png con un'immagine dalla galleria inserita dove si è toccato, scelta;
/// pdf-oscura.png con i rettangoli disegnati col dito sui dati da oscurare e Applica.
fn schermate_della_modifica(prova: &Path) {
    use tascabile_pdf::GenereOggetto;
    let file = prova.join("radice/Download/Relazione di dimissione.pdf");
    std::fs::write(&file, lettera_da_modificare()).unwrap();
    let _ = std::fs::remove_file(prova.join("app/posizioni-pdf.json"));
    let piattaforma = Piattaforma::con_cartelle(prova.join("app"), prova.join("cache"), prova.join("radice"));
    let app = TascabileApp::new(piattaforma, Instant::now());
    let mut h = Harness::builder().with_size(misura()).with_pixels_per_point(densita()).with_step_dt(1.0 / 30.0).wgpu().build_ui_state(disegna, app);
    h.step();
    h.state_mut().apri_pdf_dalla_cartella(&file);
    aspetta_il_pdf(&mut h);
    passi(&mut h, 40);
    h.get_by_label("Modifica").click();
    passi(&mut h, 4);
    let oggetti = aspetta_gli_oggetti(&mut h, "gli oggetti della lettera", |o| o.len() >= 5);
    // Una riga riscritta con la tastiera.
    let riga = oggetti.iter().find(|o| o.1.starts_with("Terapia")).expect("la riga della terapia").2;
    tocca_in(&mut h, riga.center());
    h.event(egui::Event::Text(", poi controllo".into()));
    passi(&mut h, 2);
    insets_della_tastiera(&mut h, true);
    passi(&mut h, 20);
    h.get_by_label("Fatto");
    salva(&mut h, "pdf-modifica-testo");
    h.get_by_label("Fatto").click();
    insets_della_tastiera(&mut h, false);
    passi(&mut h, 6);
    aspetta_gli_oggetti(&mut h, "la riga riscritta", |o| o.iter().any(|o| o.1.ends_with("poi controllo")));
    aspetta_le_immagini(&mut h);
    // Un testo nuovo dalla proposta su un punto vuoto, poi spostato col dito.
    let vuoto = egui::pos2(16.0 + 330.0 * pagina_dp() / 595.0, 78.0 + 300.0 * pagina_dp() / 595.0);
    tocca_in(&mut h, vuoto);
    if h.query_by_label("Aggiungi testo").is_none() {
        passi(&mut h, 15);
        tocca_in(&mut h, vuoto);
    }
    h.get_by_label("Aggiungi testo").click();
    passi(&mut h, 4);
    h.event(egui::Event::Text("Visto, si approva".into()));
    passi(&mut h, 2);
    h.get_by_label("Fatto").click();
    passi(&mut h, 4);
    let oggetti = aspetta_gli_oggetti(&mut h, "il testo aggiunto", |o| o.iter().any(|o| matches!(o.0, GenereOggetto::TestoAggiunto { .. })));
    let dove = oggetti.iter().find(|o| matches!(o.0, GenereOggetto::TestoAggiunto { .. })).unwrap().2;
    trascina(&mut h, dove.center(), egui::vec2(-90.0, 50.0));
    aspetta_gli_oggetti(&mut h, "il testo spostato", |o| {
        o.iter().any(|o| matches!(o.0, GenereOggetto::TestoAggiunto { .. }) && (o.2.center().x - (dove.center().x - 90.0)).abs() < 3.0)
    });
    aspetta_le_immagini(&mut h);
    passi(&mut h, 10);
    h.get_by_label("Formato");
    salva(&mut h, "pdf-sposta");
    // Un'immagine dalla galleria, dove si tocca.
    let vuoto = egui::pos2(16.0 + 400.0 * pagina_dp() / 595.0, 78.0 + 640.0 * pagina_dp() / 595.0);
    tocca_in(&mut h, vuoto);
    if h.query_by_label("Immagine").is_none() {
        passi(&mut h, 15);
        tocca_in(&mut h, vuoto);
    }
    h.get_by_label("Immagine").click();
    passi(&mut h, 15);
    h.get_by_label("Dalla galleria").click();
    passi(&mut h, 4);
    h.state().simula_immagine_di_prova(&tascabile_pdf::prova::foto_di_una_firma(900, 600, 85));
    aspetta_gli_oggetti(&mut h, "l'immagine inserita", |o| o.iter().any(|o| o.0 == GenereOggetto::ImmagineAggiunta));
    aspetta_le_immagini(&mut h);
    passi(&mut h, 20);
    h.get_by_label("Sostituisci");
    salva(&mut h, "pdf-immagine");
    // Oscura dal foglio Strumenti: due rettangoli col dito sui dati.
    h.get_by_label("Strumenti").click();
    passi(&mut h, 15);
    h.get_by_label("Oscura").scroll_to_me();
    passi(&mut h, 4);
    h.get_by_label("Oscura").click();
    passi(&mut h, 15);
    let oggetti = h.state_mut().oggetti_pdf_di_prova(0);
    for inizio in ["Codice fiscale", "Telefono"] {
        let r = oggetti.iter().find(|o| o.1.starts_with(inizio)).expect("la riga da oscurare").2.expand(3.0);
        trascina(&mut h, r.left_top(), r.size());
    }
    passi(&mut h, 4);
    h.get_by_label("Applica");
    let img = salva(&mut h, "pdf-oscura");
    assert!(pixel_di(&img, [0xE3, 0x21, 0x21]) > 300, "mancano i bordi rossi dei rettangoli da oscurare");
    h.get_by_label("Applica").click();
    passi(&mut h, 4);
    h.get_by_label("Oscurare per sempre?");
    h.get_by_label("Oscura").click();
    passi(&mut h, 4);
    aspetta_gli_oggetti(&mut h, "i dati oscurati", |o| !o.is_empty() && !o.iter().any(|o| o.1.contains("RSSMRA")));
    // Via senza salvare.
    for _ in 0..4 {
        h.key_press(egui::Key::BrowserBack);
        passi(&mut h, 6);
        if h.query_by_label("Non salvare").is_some() {
            break;
        }
    }
    h.get_by_label("Non salvare").click();
    passi(&mut h, 6);
}

/// La copia di recupero di un PDF (pdf-recupero.png): il modulo con «residente in» compilato,
/// l'app che perde il fuoco (la copia) e poi muore senza salvare; alla riapertura il dialogo
/// sopra la Home propone di riaprirlo. Riaperto, ha il valore.
fn schermata_del_recupero(prova: &Path) {
    let file = prova.join("radice/Download/Consenso informato da firmare.pdf");
    std::fs::write(&file, tascabile_pdf::prova::modulo_da_compilare()).unwrap();
    let nuova = || {
        let piattaforma = Piattaforma::con_cartelle(prova.join("app"), prova.join("cache"), prova.join("radice"));
        let app = TascabileApp::new(piattaforma, Instant::now());
        Harness::builder().with_size(misura()).with_pixels_per_point(densita()).with_step_dt(1.0 / 30.0).wgpu().build_ui_state(disegna, app)
    };
    let mut h = nuova();
    h.step();
    h.state_mut().apri_pdf_dalla_cartella(&file);
    aspetta_il_pdf(&mut h);
    h.get_by_label("Residente in").click();
    passi(&mut h, 2);
    h.event(egui::Event::Text("Via dei Mille 12".into()));
    passi(&mut h, 2);
    h.input_mut().focused = false;
    passi(&mut h, 1);
    h.input_mut().focused = true;
    h.state_mut().aspetta_il_motore_pdf();
    passi(&mut h, 2);
    assert!(h.state().pdf_ha_la_copia(), "la copia del pdf non c'è");
    drop(h);
    let mut h = nuova();
    let fine = Instant::now() + std::time::Duration::from_secs(10);
    while h.query_by_label("Lavoro da recuperare").is_none() {
        assert!(Instant::now() < fine, "il recupero non ha proposto il pdf");
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.step();
    }
    passi(&mut h, 12);
    h.get_by_label_contains("«Consenso informato da firmare»");
    salva(&mut h, "pdf-recupero");
    h.get_by_label("Riapri").click();
    aspetta_il_pdf(&mut h);
    passi(&mut h, 6);
    h.get_by_label_contains("· non salvato");
    h.key_press(egui::Key::BrowserBack);
    passi(&mut h, 6);
    h.get_by_label("Non salvare").click();
    passi(&mut h, 6);
}

/// Passi (e attese vere) finché il PDF che si apre non è nel lettore, al massimo 20 s.
fn aspetta_il_pdf(h: &mut Harness<'_, TascabileApp>) {
    let fine = Instant::now() + std::time::Duration::from_secs(20);
    while h.state().pdf_in_attesa().is_none() {
        assert!(Instant::now() < fine, "il PDF non si è aperto in 20 s");
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.step();
    }
    aspetta_le_immagini(h);
}

/// Il lavoro sul PDF (piano «PDF completo», P1), col modulo aperto dalla cartella Download:
/// pdf-modi.png con una nota di prova (le modifiche dell'utente arrivano coi passaggi dopo),
/// Annulla e Ripeti (sotto i 400 dp Ripeti è nel menu) e «Modifiche non salvate» nella barra
/// alta, la barra dei modi con Leggi acceso e Strumenti; pdf-strumenti.png col foglio degli
/// strumenti; pdf-salvare.png col dialogo dell'uscita; pdf-firmato.png con l'avviso delle firme
/// di un PDF firmato.
fn schermate_del_lavoro(prova: &Path) {
    let file = prova.join("radice/Download/Consenso informato da firmare.pdf");
    // Senza la ripresa delle schermate di prima (lo zoom del pizzico): la pagina larga quanto lo
    // schermo, come nel bozzetto.
    let _ = std::fs::remove_file(prova.join("app/posizioni-pdf.json"));
    let piattaforma = Piattaforma::con_cartelle(prova.join("app"), prova.join("cache"), prova.join("radice"));
    let app = TascabileApp::new(piattaforma, Instant::now());
    let mut h = Harness::builder().with_size(misura()).with_pixels_per_point(densita()).with_step_dt(1.0 / 30.0).wgpu().build_ui_state(disegna, app);
    h.step();
    h.state_mut().apri_pdf_dalla_cartella(&file);
    aspetta_il_pdf(&mut h);
    // Una nota in alto a destra della prima pagina: la modifica di prova.
    h.state_mut().modifica_pdf_di_prova(tascabile_pdf::prova::nota(0, [520.0, 800.0]));
    h.state_mut().aspetta_il_motore_pdf();
    aspetta_le_immagini(&mut h);
    passi(&mut h, 40);
    h.get_by_label("Modifiche non salvate");
    h.get_by_label("Annulla modifica");
    // Sotto i 400 dp Ripeti sta nel menu.
    if misura().x >= 400.0 {
        h.get_by_label("Ripeti modifica");
    } else {
        assert!(h.query_by_label("Ripeti modifica").is_none(), "Ripeti nella barra stretta");
    }
    h.get_by_label("Leggi");
    let img = salva(&mut h, "pdf-modi");
    // Il rosso tenue del modo acceso, il rosso del suo nome.
    assert!(pixel_di(&img, [0xF7, 0xE4, 0xDF]) > 3_000, "manca il fondo del modo acceso");
    // Il foglio Strumenti.
    h.get_by_label("Strumenti").click();
    passi(&mut h, 12);
    h.get_by_label("Strumenti PDF");
    h.get_by_label("Organizza pagine");
    salva(&mut h, "pdf-strumenti");
    h.key_press(egui::Key::BrowserBack);
    passi(&mut h, 12);
    // Indietro con le modifiche: «Salvare le modifiche?».
    h.key_press(egui::Key::BrowserBack);
    passi(&mut h, 6);
    h.get_by_label("Salvare le modifiche?");
    salva(&mut h, "pdf-salvare");
    h.get_by_label("Non salvare").click();
    passi(&mut h, 6);
    assert!(h.state().pdf_in_attesa().is_none(), "il PDF è rimasto aperto");
    // Un PDF firmato: la prima modifica che renderebbe non valida la firma chiede prima.
    let firmato = prova.join("radice/Download/Consenso firmato.pdf");
    std::fs::write(&firmato, tascabile_pdf::prova::firmato(tascabile_pdf::prova::modulo(), None)).unwrap();
    h.state_mut().apri_pdf_dalla_cartella(&firmato);
    aspetta_il_pdf(&mut h);
    passi(&mut h, 40);
    h.get_by_label("Strumenti").click();
    passi(&mut h, 12);
    h.get_by_label("Organizza pagine").scroll_to_me();
    passi(&mut h, 4);
    h.get_by_label("Organizza pagine").click();
    passi(&mut h, 12);
    h.get_by_label("Pagina 1").click();
    passi(&mut h, 4);
    h.get_by_label("Ruota a destra").click();
    passi(&mut h, 12);
    h.get_by_label("PDF firmato");
    h.get_by_label("Lavora su una copia");
    salva(&mut h, "pdf-firmato");
    h.get_by_label("Annulla").click();
    passi(&mut h, 6);
    assert!(h.query_by_label("PDF firmato").is_none());
}

/// Dove sta sullo schermo un carattere della prima pagina del PDF `byte`, appena aperto: la pagina
/// sotto la barra alta (64 dp), a 16 dp dal lato e 14 dalla cima, larga 380 dp per 595 pt.
/// Restituisce il rettangolo del carattere `scarto` di `parola` (con `scarto` negativo, dalla fine).
fn carattere_sullo_schermo(byte: &[u8], parola: &str, scarto: isize) -> egui::Rect {
    let pdf = tascabile_pdf::Pdf::apri(byte.to_vec(), None).unwrap();
    let t = pdf.testo(0).unwrap();
    let caratteri: String = t.glyphs.iter().map(|g| g.text.as_str()).collect();
    let inizio = caratteri[..caratteri.find(parola).expect("parola nella pagina")].chars().count();
    let n = parola.chars().count() as isize;
    let i = inizio + scarto.rem_euclid(n) as usize;
    let [x0, y0, x1, y1] = t.glyphs[i].rect;
    let k = pagina_dp() / 595.0;
    egui::Rect::from_min_max(egui::pos2(16.0 + x0 * k, 78.0 + y0 * k), egui::pos2(16.0 + x1 * k, 78.0 + y1 * k))
}

/// La selezione del testo (pdf-selezione.png): pressione lunga su «chiaro», la goccia della fine
/// trascinata fino a «comprensibile» (a metà strada pdf-selezione-lente.png, con la lente), poi la
/// barra del contesto scura con Copia e Tutto. Indietro la chiude.
fn schermata_della_selezione(h: &mut Harness<'_, TascabileApp>, modulo: &[u8]) {
    let parola = carattere_sullo_schermo(modulo, "chiaro", 0).union(carattere_sullo_schermo(modulo, "chiaro", -1));
    let dove = parola.center();
    dito(h, egui::TouchPhase::Start, dove);
    passi(h, 24);
    dito(h, egui::TouchPhase::End, dove);
    h.event(egui::Event::PointerGone);
    let fine = Instant::now() + std::time::Duration::from_secs(20);
    while h.state().pdf_testo_selezionato().is_none() {
        assert!(Instant::now() < fine, "la parola non si è selezionata");
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.step();
    }
    assert_eq!(h.state().pdf_testo_selezionato().as_deref(), Some("chiaro"));
    // La goccia della fine: appesa sotto la riga, a destra dell'ultima lettera.
    let presa = egui::pos2(parola.right() + 11.0, parola.bottom() + 11.0);
    let arrivo = carattere_sullo_schermo(modulo, "comprensibile", -1);
    let arrivo = egui::pos2(arrivo.right() + 11.0, arrivo.bottom() + 11.0);
    dito(h, egui::TouchPhase::Start, presa);
    h.step();
    for i in 1..=10 {
        dito(h, egui::TouchPhase::Move, presa + (arrivo - presa) * (i as f32 / 10.0));
        h.step();
        if i == 6 {
            let img = h.render().expect("disegno wgpu senza finestra");
            img.save(Path::new(env!("CARGO_TARGET_TMPDIR")).join("schermate/pdf-selezione-lente.png")).unwrap();
        }
    }
    dito(h, egui::TouchPhase::End, arrivo);
    // Il dito se ne va: la barra di scorrimento che egui mostra sotto il dito sfuma.
    h.event(egui::Event::PointerGone);
    passi(h, 12);
    assert_eq!(h.state().pdf_testo_selezionato().as_deref(), Some("chiaro e comprensibile"));
    h.get_by_label("Copia");
    h.get_by_label("Tutto");
    let img = salva(h, "pdf-selezione");
    // La barra scura e la selezione azzurra sopra il bianco della pagina.
    assert!(pixel_di(&img, [0x2A, 0x26, 0x22]) > 5_000, "manca la barra scura");
    let azzurri = img.pixels().filter(|p| p.0[0].abs_diff(215) <= 4 && p.0[1].abs_diff(222) <= 4 && p.0[2].abs_diff(238) <= 4).count();
    assert!(azzurri > 1_000, "manca la selezione azzurra: {azzurri} pixel");
    h.key_press(egui::Key::BrowserBack);
    passi(h, 4);
    assert!(h.state().pdf_testo_selezionato().is_none());
}

/// Il menu del lettore (pdf-menu.png) e le informazioni sul documento (pdf-informazioni.png).
fn schermate_del_menu(h: &mut Harness<'_, TascabileApp>) {
    h.get_by_label("Menu").click();
    passi(h, 6);
    h.get_by_label("Informazioni sul documento");
    salva(h, "pdf-menu");
    h.get_by_label("Informazioni sul documento").click();
    passi(h, 12);
    h.get_by_label("Firme digitali");
    h.get_by_label("Ambulatorio di prova");
    salva(h, "pdf-informazioni");
    h.key_press(egui::Key::BrowserBack);
    passi(h, 12);
    assert!(h.query_by_label("Firme digitali").is_none());
}

/// Un PDF con password arrivato con «Apri con» (pdf-password.png): il dialogo sopra la Home, con
/// la tastiera aperta (l'immagine finisce dove comincia), dopo una password sbagliata: il campo
/// vuoto col fuoco e «Password sbagliata» in rosso (riscrivendo se ne va). Poi qualche lettera
/// della nuova, a punti (pdf-password-punti.png).
fn schermata_della_password(prova: &Path) {
    let file = prova.join("radice/Download/Referto protetto.pdf");
    std::fs::write(&file, tascabile_pdf::prova::cifrato("Segreta1")).unwrap();
    let piattaforma = Piattaforma::con_cartelle(prova.join("app"), prova.join("cache"), prova.join("radice"));
    piattaforma.simula_apri_con(file.to_str().unwrap(), Some("application/pdf"));
    let app = TascabileApp::new(piattaforma, Instant::now());
    let mut h = Harness::builder().with_size(misura()).with_pixels_per_point(densita()).wgpu().build_ui_state(disegna, app);
    let aspetta = |h: &mut Harness<'_, TascabileApp>| {
        let fine = Instant::now() + std::time::Duration::from_secs(20);
        h.step();
        while !h.state().chiede_la_password() {
            assert!(Instant::now() < fine, "il dialogo della password non è arrivato");
            std::thread::sleep(std::time::Duration::from_millis(5));
            h.step();
        }
        passi(h, 4);
    };
    aspetta(&mut h);
    h.event(egui::Event::Text("segreta".into()));
    passi(&mut h, 2);
    h.key_press(egui::Key::Enter);
    passi(&mut h, 2);
    aspetta(&mut h);
    h.get_by_label("Password sbagliata");
    insets_della_tastiera(&mut h, true);
    passi(&mut h, 4);
    h.get_by_label("PDF protetto");
    let img = salva(&mut h, "pdf-password");
    // Il rosso di «Password sbagliata» e il blu del campo col fuoco.
    let rossi = img.pixels().filter(|p| p.0[0] > 160 && p.0[1] < 110 && p.0[2] < 90).count();
    assert!(rossi > 300, "manca «Password sbagliata» in rosso: {rossi} pixel");
    assert!(pixel_di(&img, [0x23, 0x46, 0xA0]) > 1_000, "manca il blu del campo e di Apri");
    h.event(egui::Event::Text("Segr".into()));
    passi(&mut h, 4);
    assert!(h.query_by_label("Password sbagliata").is_none());
    salva(&mut h, "pdf-password-punti");
}

/// Passi (e attese vere: il pool disegna sui suoi thread) finché il lettore non ha più immagini
/// da aspettare, al massimo 20 s.
fn aspetta_le_immagini(h: &mut Harness<'_, TascabileApp>) {
    let fine = Instant::now() + std::time::Duration::from_secs(20);
    loop {
        h.step();
        if h.state().pdf_in_attesa() == Some(false) {
            break;
        }
        assert!(Instant::now() < fine, "le pagine del PDF non sono arrivate in 20 s");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// Il lettore dei PDF (bozzetto 7) col modulo di prova di tre pagine, arrivato con «Apri con»:
/// pdf.png com'è aperto, con l'indicatore della pagina; pdf-cerca.png con la ricerca nella barra
/// alta e i risultati in giallo; pdf-indice.png coi segnalibri del modulo; pdf-miniature.png con
/// le miniature; pdf-zoom.png dopo un pizzico, con le tessere nitide. Passi da 1/30 di secondo:
/// l'indicatore resta 1,2 s.
fn schermate_del_pdf(prova: &Path) {
    let file = prova.join("radice/Download/Consenso informato da firmare.pdf");
    std::fs::write(&file, tascabile_pdf::prova::modulo()).unwrap();
    let piattaforma = Piattaforma::con_cartelle(prova.join("app"), prova.join("cache"), prova.join("radice"));
    piattaforma.simula_apri_con(file.to_str().unwrap(), Some("application/pdf"));
    let app = TascabileApp::new(piattaforma, Instant::now());
    let mut h = Harness::builder().with_size(misura()).with_pixels_per_point(densita()).with_step_dt(1.0 / 30.0).wgpu().build_ui_state(disegna, app);
    let fine = Instant::now() + std::time::Duration::from_secs(20);
    while h.state().pdf_in_attesa().is_none() {
        assert!(Instant::now() < fine, "il PDF non si è aperto in 20 s");
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.step();
    }
    aspetta_le_immagini(&mut h);
    h.get_by_label("Consenso informato da firmare");
    h.get_by_label("PDF · 3 pagine");
    h.get_by_label("1 / 3");
    let img = salva(&mut h, "pdf");
    // Il tavolo attorno alle pagine bianche, il testo scuro, i campi blu del modulo.
    let tutti = (img.width() * img.height()) as usize;
    // Il tavolo sta nei margini da 16 dp, in parte sotto l'ombra delle pagine.
    let tavolo = pixel_di(&img, [0xE9, 0xE5, 0xDE]);
    assert!(tavolo > tutti / 100, "manca il tavolo: {tavolo} pixel su {tutti}");
    assert!(pixel_di(&img, [0xFF, 0xFF, 0xFF]) > tutti / 3, "mancano le pagine bianche");
    let scuri = img.pixels().filter(|p| p.0[0] < 90 && p.0[1] < 90 && p.0[2] < 90).count();
    assert!(scuri > 10_000, "pagina senza testo: {scuri} pixel scuri");
    let blu = img.pixels().filter(|p| p.0[2] > 140 && p.0[0] < 80 && p.0[1] < 110).count();
    assert!(blu > 2_000, "pagina senza i campi blu: {blu} pixel");
    for riga in tascabile_platform::diagnosi_testo().lines().filter(|r| r.contains("tipo pdf") || r.contains("pdf: ")) {
        eprintln!("misura del PDF: {riga}");
    }
    let modulo = tascabile_pdf::prova::modulo();
    schermata_della_selezione(&mut h, &modulo);
    schermate_del_menu(&mut h);

    // La ricerca al posto della barra alta (pdf-cerca.png): «trattamento» battuto con la tastiera
    // finta, poi la tastiera chiusa con Indietro, per vedere i risultati e la barra in fondo.
    h.get_by_label("Cerca").click();
    passi(&mut h, 2);
    h.event(egui::Event::Text("trattamento".into()));
    passi(&mut h, 4);
    aspetta_le_immagini(&mut h);
    h.key_press(egui::Key::BrowserBack);
    passi(&mut h, 6);
    h.get_by_label("1 di 28");
    let img = salva(&mut h, "pdf-cerca");
    // Il giallo dei risultati sulle pagine.
    let gialli = img.pixels().filter(|p| p.0[0] > 235 && p.0[1] > 180 && p.0[2] < 170 && p.0[0] > p.0[2] + 80).count();
    assert!(gialli > 1_000, "risultati senza giallo: {gialli} pixel");
    h.get_by_label("Chiudi la ricerca").click();
    passi(&mut h, 4);
    assert!(h.query_by_label("1 di 28").is_none());

    // L'indice dei segnalibri del modulo (pdf-indice.png), dal foglio Strumenti.
    h.get_by_label("Strumenti").click();
    passi(&mut h, 12);
    h.get_by_label("Indice").scroll_to_me();
    passi(&mut h, 4);
    h.get_by_label("Indice").click();
    passi(&mut h, 12);
    h.get_by_label("Firma, pagina 1");
    salva(&mut h, "pdf-indice");
    h.key_press(egui::Key::BrowserBack);
    passi(&mut h, 12);

    // Le miniature (pdf-miniature.png), dal foglio Strumenti, con la prima pagina col bordo blu.
    h.get_by_label("Strumenti").click();
    passi(&mut h, 12);
    h.get_by_label("Miniature").scroll_to_me();
    passi(&mut h, 4);
    h.get_by_label("Miniature").click();
    passi(&mut h, 4);
    aspetta_le_immagini(&mut h);
    passi(&mut h, 12);
    aspetta_le_immagini(&mut h);
    h.get_by_label("Pagina 3");
    salva(&mut h, "pdf-miniature");
    h.key_press(egui::Key::BrowserBack);
    passi(&mut h, 12);
    for riga in tascabile_platform::diagnosi_testo().lines().filter(|r| r.contains("pdf: ricerca finita") || r.contains("pdf: miniature")) {
        eprintln!("misura del PDF: {riga}");
    }

    // Un pizzico di tre volte attorno al primo campo: subito l'immagine di prima ingrandita, poi
    // le tessere nitide.
    let dita = egui::pos2(120.0, 200.0);
    h.event(egui::Event::PointerMoved(dita));
    h.event(egui::Event::Zoom(3.0));
    h.step();
    h.step();
    aspetta_le_immagini(&mut h);
    passi(&mut h, 12);
    aspetta_le_immagini(&mut h);
    let img = salva(&mut h, "pdf-zoom");
    let scuri = img.pixels().filter(|p| p.0[0] < 90 && p.0[1] < 90 && p.0[2] < 90).count();
    assert!(scuri > 10_000, "zoom senza testo: {scuri} pixel scuri");
    h.key_press(egui::Key::BrowserBack);
    passi(&mut h, 2);
    for riga in tascabile_platform::diagnosi_testo().lines().filter(|r| r.contains("pdf: zoom") || r.contains("pdf chiuso")) {
        eprintln!("misura del PDF: {riga}");
    }
}

/// Passi finché `fatto` non dice sì, al massimo `secondi`.
fn finche(h: &mut Harness<'_, TascabileApp>, secondi: u64, cosa: &str, fatto: impl Fn(&TascabileApp) -> bool) {
    let fine = Instant::now() + std::time::Duration::from_secs(secondi);
    while !fatto(h.state()) {
        assert!(Instant::now() < fine, "{cosa}: non in {secondi} s");
        std::thread::sleep(std::time::Duration::from_millis(10));
        h.step();
    }
}

/// «Riconosci testo» su una lettera scansionata, poi la ricerca di «città» nel testo riconosciuto
/// (pdf-ocr.png); «Scansiona» dal foglio Strumenti con due foto finte di un foglio sul tavolo,
/// raddrizzate e pulite (pdf-scansione.png), e il PDF nuovo con l'OCR aperto nel lettore.
fn schermate_dell_ocr(prova: &Path) {
    let file = prova.join("radice/Download/Lettera scansionata.pdf");
    std::fs::write(&file, tascabile_pdf::prova::scansione_italiana()).unwrap();
    let _ = std::fs::remove_file(prova.join("app/posizioni-pdf.json"));
    let piattaforma = Piattaforma::con_cartelle(prova.join("app"), prova.join("cache"), prova.join("radice"));
    let app = TascabileApp::new(piattaforma, Instant::now());
    let mut h = Harness::builder().with_size(misura()).with_pixels_per_point(densita()).with_step_dt(1.0 / 30.0).wgpu().build_ui_state(disegna, app);
    h.step();
    h.state_mut().apri_pdf_dalla_cartella(&file);
    aspetta_il_pdf(&mut h);
    passi(&mut h, 40);
    h.get_by_label("Strumenti").click();
    passi(&mut h, 12);
    h.get_by_label("Riconosci testo").click();
    passi(&mut h, 12);
    h.get_by_label("Questa pagina");
    h.get_by_label("Tutte le pagine").click();
    passi(&mut h, 3);
    h.get_by_label("Riconosco il testo");
    finche(&mut h, 120, "riconoscimento", |a| !a.lavoro_in_corso());
    h.state_mut().aspetta_il_motore_pdf();
    passi(&mut h, 6);
    assert_eq!(h.state().pdf_modificato(), Some(true), "lo strato di testo non è arrivato al motore");
    aspetta_le_immagini(&mut h);
    // La ricerca trova le parole riconosciute, accentate comprese.
    h.get_by_label("Cerca").click();
    passi(&mut h, 2);
    h.event(egui::Event::Text("città".into()));
    passi(&mut h, 4);
    aspetta_le_immagini(&mut h);
    h.key_press(egui::Key::BrowserBack);
    passi(&mut h, 6);
    h.get_by_label("1 di 2");
    let img = salva(&mut h, "pdf-ocr");
    let gialli = img.pixels().filter(|p| p.0[0] > 235 && p.0[1] > 180 && p.0[2] < 170 && p.0[0] > p.0[2] + 80).count();
    assert!(gialli > 300, "risultati senza giallo: {gialli} pixel");
    h.get_by_label("Chiudi la ricerca").click();
    passi(&mut h, 4);

    // Scansiona dal foglio Strumenti: la scansione vuota con Fotocamera e Galleria
    // (scansione-vuota.png, 0.8.3), due foto coi loro bordi, l'anteprima.
    h.get_by_label("Strumenti").click();
    passi(&mut h, 12);
    h.get_by_label("Scansiona").scroll_to_me();
    passi(&mut h, 4);
    h.get_by_label("Scansiona").click();
    passi(&mut h, 6);
    assert_eq!(h.state().pagine_scansionate(), Some(0));
    salva(&mut h, "scansione-vuota");
    for n in 1..=2 {
        h.get_by_label("Fotocamera").click();
        passi(&mut h, 3);
        h.state().simula_foto_firma_di_prova(Some(&tascabile_pdf::prova::foto_di_un_foglio()));
        finche(&mut h, 60, "bordi da sistemare", |a| a.bordi_aperti() && !a.lavoro_in_corso());
        passi(&mut h, 6);
        if n == 1 {
            // «Sistema i bordi» (ritocchi della 0.8.2, R3): scansione-bordi.png con gli angoli
            // trovati; scansione-lente.png con l'angolo in basso a destra preso col dito e la lente,
            // poi l'angolo torna dov'era.
            h.get_by_label("Sistema i bordi");
            salva(&mut h, "scansione-bordi");
            let dove = h.get_by_label("Angolo in basso a destra").rect().center();
            let premi = |h: &mut Harness<'_, TascabileApp>, pos, pressed| {
                h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE })
            };
            h.event(egui::Event::PointerMoved(dove));
            premi(&mut h, dove, true);
            h.step();
            for i in 1..=8 {
                h.event(egui::Event::PointerMoved(dove + egui::vec2(-2.0 * i as f32, -3.0 * i as f32)));
                h.step();
            }
            let img = salva(&mut h, "scansione-lente");
            assert!(pixel_di(&img, [0xFF, 0xFF, 0xFF]) > 1_000, "la lente senza il suo bordo bianco");
            for i in (0..8).rev() {
                h.event(egui::Event::PointerMoved(dove + egui::vec2(-2.0 * i as f32, -3.0 * i as f32)));
                h.step();
            }
            premi(&mut h, dove, false);
            passi(&mut h, 3);
            // Il lato in basso tirato giù, come per un foglio piegato (3/11): scansione-lati.png;
            // poi torna dov'era.
            let lato = h.get_by_label("Lato in basso").rect().center();
            trascina(&mut h, lato, egui::vec2(0.0, 30.0));
            passi(&mut h, 3);
            salva(&mut h, "scansione-lati");
            let lato = h.get_by_label("Lato in basso").rect().center();
            trascina(&mut h, lato, egui::vec2(0.0, -30.0));
            passi(&mut h, 3);
        }
        h.get_by_label("Fatto").click();
        finche(&mut h, 60, "foto pulita", |a| a.pagine_scansionate() == Some(n) && !a.lavoro_in_corso());
        passi(&mut h, 4);
    }
    h.get_by_label("2 pagine");
    h.get_by_label("Rifai");
    let img = salva(&mut h, "pdf-scansione");
    // La gomma (3/11): una passata sulla pagina, scansione-gomma.png, poi Fatto la rifà.
    h.get_by_label("Gomma").click();
    passi(&mut h, 3);
    let pagina = h.get_by_label("Passa il dito sulle macchie da togliere").rect();
    trascina(&mut h, egui::pos2(pagina.left() + 30.0, pagina.top() + 70.0), egui::vec2(pagina.width() - 60.0, 10.0));
    passi(&mut h, 3);
    salva(&mut h, "scansione-gomma");
    h.get_by_label("Fatto").click();
    passi(&mut h, 2);
    finche(&mut h, 60, "pagina con la gomma", |a| !a.lavoro_in_corso() && a.ritocchi_scansionati().get(1).is_some_and(|r| r.cancellature.len() == 1));
    passi(&mut h, 4);
    // La pagina pulita: carta bianca e testo scuro, niente tavolo marrone.
    let bianchi = pixel_di(&img, [0xFF, 0xFF, 0xFF]);
    assert!(bianchi > (img.width() * img.height()) as usize / 5, "anteprima senza carta bianca: {bianchi}");
    // L'aspetto dal suo foglio (scansione-aspetti.png), poi Bianco e nero:
    // scansione-bianco-e-nero.png.
    h.get_by_label("Aspetto").click();
    passi(&mut h, 4);
    salva(&mut h, "scansione-aspetti");
    h.get_by_label("Bianco e nero").click();
    passi(&mut h, 2);
    finche(&mut h, 60, "pagina in bianco e nero", |a| !a.lavoro_in_corso());
    passi(&mut h, 4);
    salva(&mut h, "scansione-bianco-e-nero");
    // Ruota, nella barra alta: la pagina che si vede gira di un quarto di giro (analisi del
    // 27/10, pagine I5), pdf-scansione-girata.png.
    h.get_by_label("Ruota").click();
    finche(&mut h, 60, "pagina girata", |a| !a.lavoro_in_corso() && a.misure_scansionate().get(1).is_some_and(|m| m.0 > m.1));
    passi(&mut h, 4);
    salva(&mut h, "pdf-scansione-girata");
    h.get_by_label("Crea PDF").click();
    passi(&mut h, 3);
    finche(&mut h, 120, "pdf della scansione", |a| a.pagine_scansionate().is_none());
    // La lettera ha lo strato di testo non salvato: prima «Salvare le modifiche?», che prima
    // della correzione del 27/10 (critico 6) mancava e buttava via il riconoscimento.
    passi(&mut h, 3);
    h.get_by_label("Salvare le modifiche?");
    h.get_by_label("Non salvare").click();
    passi(&mut h, 3);
    aspetta_il_pdf(&mut h);
    passi(&mut h, 20);
    h.get_by_label("PDF · 2 pagine");
    let fatti: Vec<_> = std::fs::read_dir(prova.join("radice/Documents/Tascabile"))
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("Scansione "))
        .collect();
    assert_eq!(fatti.len(), 1, "il PDF della scansione non è in Documenti/Tascabile");
    // Nella diagnosi conti e tempi, mai testo riconosciuto.
    let diagnosi = tascabile_platform::diagnosi_testo();
    for riga in diagnosi.lines().filter(|r| r.contains("ocr") || r.contains("scansione")) {
        eprintln!("diagnosi: {riga}");
    }
    for parola in ["città", "perché", "Lettera scansionata"] {
        assert!(!diagnosi.contains(parola), "«{parola}» nella diagnosi");
    }
}

/// Il modo Annota e le forme (piano «PDF completo», P5) sulla lettera di prova: pdf-annota.png
/// con una parola evidenziata in giallo, una sottolineata in azzurro e la barra della selezione
/// con Evidenzia, Sottolinea, Barra e i colori; pdf-penna.png con due tratti rossi della penna e
/// la striscia con i colori e gli spessori; pdf-nota.png con la nota posata e il suo foglio col
/// testo; pdf-forme.png con un link tratteggiato sul titolo, un rettangolo, un'ellisse e una
/// freccia col dito, e la striscia delle forme. Una prova a parte, per lanciarla da sola.
#[test]
fn schermate_delle_annotazioni() {
    let _una = una_alla_volta();
    use tascabile_pdf::{Forma, GenereAnnotazione, Marca};
    let prova = Path::new(env!("CARGO_TARGET_TMPDIR")).join("prove-annota");
    let _ = std::fs::remove_dir_all(&prova);
    std::fs::create_dir_all(prova.join("radice/Download")).unwrap();
    std::fs::create_dir_all(prova.join("app")).unwrap();
    std::fs::write(prova.join("app/autore.txt"), "Infermiere di turno").unwrap();
    let file = prova.join("radice/Download/Relazione di dimissione.pdf");
    std::fs::write(&file, lettera_da_modificare()).unwrap();
    let piattaforma = Piattaforma::con_cartelle(prova.join("app"), prova.join("cache"), prova.join("radice"));
    let app = TascabileApp::new(piattaforma, Instant::now());
    let mut h = Harness::builder().with_size(misura()).with_pixels_per_point(densita()).with_step_dt(1.0 / 30.0).wgpu().build_ui_state(disegna, app);
    h.step();
    h.state_mut().apri_pdf_dalla_cartella(&file);
    aspetta_il_pdf(&mut h);
    passi(&mut h, 40);
    h.get_by_label("Annota").click();
    passi(&mut h, 6);
    let k = pagina_dp() / 595.0;
    let sullo_schermo = |x: f32, y: f32| egui::pos2(16.0 + x * k, 78.0 + y * k);
    let quante =
        |h: &mut Harness<'_, TascabileApp>, g: GenereAnnotazione| h.state_mut().annotazioni_pdf_di_prova().iter().filter(|a| a.0 == g).count();
    // Una parola col tasto destro (la pressione lunga), poi la barra della selezione.
    let scegli = |h: &mut Harness<'_, TascabileApp>, dove: egui::Pos2| {
        h.event(egui::Event::PointerMoved(dove));
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton { pos: dove, button: egui::PointerButton::Secondary, pressed, modifiers: egui::Modifiers::NONE });
        }
        let fine = Instant::now() + std::time::Duration::from_secs(20);
        while h.query_by_label("Evidenzia").is_none() {
            assert!(Instant::now() < fine, "la barra della selezione non è comparsa");
            std::thread::sleep(std::time::Duration::from_millis(10));
            h.step();
        }
        passi(h, 4);
    };
    // «ricoverato» evidenziata in giallo, «medicazione» sottolineata in azzurro.
    scegli(&mut h, sullo_schermo(185.0, 128.0));
    h.get_by_label("Evidenzia").click();
    passi(&mut h, 4);
    finche(&mut h, 20, "l'evidenziatura", |a| a.pdf_modificato() == Some(true));
    scegli(&mut h, sullo_schermo(205.0, 168.0));
    h.get_by_label("Azzurro").click();
    passi(&mut h, 3);
    h.get_by_label("Sottolinea").click();
    passi(&mut h, 4);
    let fine = Instant::now() + std::time::Duration::from_secs(20);
    while quante(&mut h, GenereAnnotazione::Marca(Marca::Sottolinea)) == 0 {
        assert!(Instant::now() < fine, "la sottolineatura non c'è");
        h.step();
    }
    aspetta_le_immagini(&mut h);
    // La barra aperta su «coordinatore», per la schermata.
    scegli(&mut h, sullo_schermo(100.0, 358.0));
    passi(&mut h, 10);
    h.get_by_label("Sottolinea");
    salva(&mut h, "pdf-annota");
    assert_eq!(quante(&mut h, GenereAnnotazione::Marca(Marca::Evidenzia)), 1);
    // La penna rossa: due tratti col dito, la pagina ferma.
    h.key_press(egui::Key::BrowserBack);
    passi(&mut h, 4);
    h.get_by_label("Penna").click();
    passi(&mut h, 4);
    h.get_by_label("Rosso").click();
    passi(&mut h, 3);
    tratto(&mut h, sullo_schermo(60.0, 300.0), 26, 8.0);
    tratto(&mut h, sullo_schermo(330.0, 225.0), 12, 14.0);
    let fine = Instant::now() + std::time::Duration::from_secs(20);
    while quante(&mut h, GenereAnnotazione::Penna) < 2 {
        assert!(Instant::now() < fine, "i tratti della penna non ci sono");
        h.step();
    }
    aspetta_le_immagini(&mut h);
    passi(&mut h, 10);
    h.get_by_label("Spesso");
    salva(&mut h, "pdf-penna");
    // Una nota a destra del titolo, poi il suo foglio.
    h.get_by_label("Nota").click();
    passi(&mut h, 4);
    tocca_in(&mut h, sullo_schermo(500.0, 80.0));
    passi(&mut h, 4);
    h.event(egui::Event::Text("Telefonare al medico di famiglia per la terapia".into()));
    passi(&mut h, 2);
    h.get_by_label("Fatto").click();
    passi(&mut h, 4);
    let fine = Instant::now() + std::time::Duration::from_secs(20);
    while quante(&mut h, GenereAnnotazione::Nota) == 0 {
        assert!(Instant::now() < fine, "la nota non c'è");
        h.step();
    }
    aspetta_le_immagini(&mut h);
    passi(&mut h, 10);
    let nota = h.state_mut().annotazioni_pdf_di_prova().into_iter().find(|a| a.0 == GenereAnnotazione::Nota).unwrap().2;
    tocca_in(&mut h, nota.center());
    passi(&mut h, 20);
    h.get_by_label("Telefonare al medico di famiglia per la terapia");
    salva(&mut h, "pdf-nota");
    h.key_press(egui::Key::BrowserBack);
    passi(&mut h, 15);
    // Il modo Modifica: un link sul titolo che porta a un indirizzo, poi le forme.
    h.get_by_label("Strumenti").click();
    passi(&mut h, 15);
    h.get_by_label("Link").click();
    passi(&mut h, 15);
    trascina(&mut h, sullo_schermo(55.0, 62.0), egui::vec2(230.0 * k, 28.0 * k));
    passi(&mut h, 15);
    h.get_by_label("Un indirizzo").click();
    passi(&mut h, 15);
    h.get_by_label("Scrivi l'indirizzo").click();
    passi(&mut h, 4);
    h.event(egui::Event::Text("www.asl.it".into()));
    passi(&mut h, 2);
    h.get_by_label("Fatto").click();
    passi(&mut h, 15);
    h.get_by_label("Metti il link").click();
    passi(&mut h, 6);
    finche(&mut h, 20, "il link", |a| a.pdf_modificato() == Some(true));
    h.get_by_label("Strumenti").click();
    passi(&mut h, 15);
    h.get_by_label("Forme").click();
    passi(&mut h, 15);
    trascina(&mut h, sullo_schermo(52.0, 216.0), egui::vec2(250.0 * k, 24.0 * k));
    h.get_by_label("Ellisse").click();
    passi(&mut h, 3);
    trascina(&mut h, sullo_schermo(50.0, 350.0), egui::vec2(190.0 * k, 34.0 * k));
    h.get_by_label("Freccia").click();
    passi(&mut h, 3);
    trascina(&mut h, sullo_schermo(420.0, 330.0), egui::vec2(-150.0 * k, -80.0 * k));
    let fine = Instant::now() + std::time::Duration::from_secs(20);
    while quante(&mut h, GenereAnnotazione::Forma(Forma::Freccia)) == 0 {
        assert!(Instant::now() < fine, "la freccia non c'è");
        h.step();
    }
    aspetta_le_immagini(&mut h);
    passi(&mut h, 10);
    assert_eq!(quante(&mut h, GenereAnnotazione::Forma(Forma::Rettangolo)), 1);
    assert_eq!(quante(&mut h, GenereAnnotazione::Forma(Forma::Ellisse)), 1);
    h.get_by_label("Fine delle forme");
    salva(&mut h, "pdf-forme");
    // Via senza salvare.
    for _ in 0..6 {
        h.key_press(egui::Key::BrowserBack);
        passi(&mut h, 6);
        if h.query_by_label("Non salvare").is_some() {
            break;
        }
    }
    h.get_by_label("Non salvare").click();
    passi(&mut h, 6);
}

/// Sette pagine di un programma di corso, con un titolo, una fascia colorata diversa per pagina e
/// righe grigie al posto del testo; la quarta orizzontale. Per le miniature di «Organizza pagine».
fn programma_di_prova() -> Vec<u8> {
    let colori = ["0.80 0.30 0.22", "0.20 0.45 0.75", "0.25 0.60 0.35", "0.85 0.60 0.15", "0.55 0.30 0.65", "0.20 0.55 0.60", "0.60 0.45 0.30"];
    let contenuti: Vec<String> = colori
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let (w, h) = if i == 3 { (842.0, 595.0) } else { (595.0, 842.0) };
            let mut s = format!(
                "{c} rg 50 {} {} 30 re f BT /F2 34 Tf 60 {} Td (Lezione {}) Tj ET 0.75 0.75 0.75 rg ",
                h - 110.0,
                w - 100.0,
                h - 170.0,
                i + 1
            );
            let mut y = h - 230.0;
            while y > 90.0 {
                s.push_str(&format!("60 {y} {} 12 re f ", w - 160.0 - (y as i32 % 7) as f32 * 30.0));
                y -= 34.0;
            }
            s
        })
        .collect();
    let pagine: Vec<tascabile_pdf::prova::Pagina<'_>> = contenuti
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let (larghezza, altezza) = if i == 3 { (842.0, 595.0) } else { (595.0, 842.0) };
            tascabile_pdf::prova::Pagina { larghezza, altezza, rotazione: 0, contenuto: c }
        })
        .collect();
    tascabile_pdf::prova::pdf(&pagine)
}

/// «Pagine e file» (piano «PDF completo», P6): pdf-organizza.png con la griglia, due pagine
/// scelte e la sesta presa col dito, con la barra blu dove andrà; pdf-dividi.png col foglio di
/// Dividi e gli intervalli scritti; pdf-comprimi.png col dialogo della copia compressa, la misura
/// prima e dopo; pdf-proteggi.png col foglio della password e l'avviso che non si recupera.
#[test]
fn schermate_delle_pagine() {
    let _una = una_alla_volta();
    let prova = Path::new(env!("CARGO_TARGET_TMPDIR")).join("prove-pagine");
    let _ = std::fs::remove_dir_all(&prova);
    std::fs::create_dir_all(prova.join("radice/Download")).unwrap();
    let file = prova.join("radice/Download/Programma corso OSS.pdf");
    std::fs::write(&file, programma_di_prova()).unwrap();
    let foto = prova.join("radice/Download/Foto del cartellone.pdf");
    std::fs::write(&foto, tascabile_pdf::prova::pdf_con_foto()).unwrap();
    let piattaforma = Piattaforma::con_cartelle(prova.join("app"), prova.join("cache"), prova.join("radice"));
    let app = TascabileApp::new(piattaforma, Instant::now());
    let mut h = Harness::builder().with_size(misura()).with_pixels_per_point(densita()).with_step_dt(1.0 / 30.0).wgpu().build_ui_state(disegna, app);
    h.step();
    h.state_mut().apri_pdf_dalla_cartella(&file);
    aspetta_il_pdf(&mut h);
    passi(&mut h, 40);
    h.get_by_label("Strumenti").click();
    passi(&mut h, 12);
    h.get_by_label("Organizza pagine").scroll_to_me();
    passi(&mut h, 4);
    h.get_by_label("Organizza pagine").click();
    passi(&mut h, 12);
    aspetta_le_immagini(&mut h);
    passi(&mut h, 10);
    for n in [2, 3] {
        h.get_by_label(&format!("Pagina {n}")).click();
        passi(&mut h, 4);
    }
    assert_eq!(h.state().pagine_scelte(), Some(vec![1, 2]));
    // La sesta presa col dito e portata fra la prima e la seconda, ancora giù.
    let da = h.get_by_label("Pagina 6").rect().center();
    let a = h.get_by_label("Pagina 2").rect();
    let q = egui::pos2(a.left() + 6.0, a.center().y);
    dito(&mut h, egui::TouchPhase::Start, da);
    passi(&mut h, 40);
    for i in 1..=12 {
        dito(&mut h, egui::TouchPhase::Move, da + (q - da) * (i as f32 / 12.0));
        h.step();
    }
    passi(&mut h, 4);
    salva(&mut h, "pdf-organizza");
    dito(&mut h, egui::TouchPhase::End, q);
    passi(&mut h, 6);
    h.state_mut().aspetta_il_motore_pdf();
    passi(&mut h, 6);
    assert_eq!(h.state().pagine_scelte(), Some(vec![1]), "la sesta non è andata al secondo posto");
    h.get_by_label("Chiudi").click();
    passi(&mut h, 10);
    // Dividi per intervalli.
    h.get_by_label("Strumenti").click();
    passi(&mut h, 12);
    h.get_by_label("Dividi").scroll_to_me();
    passi(&mut h, 4);
    h.get_by_label("Dividi").click();
    passi(&mut h, 12);
    h.event(egui::Event::Text("1-3, 4-7".into()));
    passi(&mut h, 12);
    salva(&mut h, "pdf-dividi");
    h.key_press(egui::Key::BrowserBack);
    passi(&mut h, 12);
    // Comprimi la foto: il dialogo con la misura di prima e di dopo.
    // Il programma con la pagina spostata si chiude senza salvare.
    h.key_press(egui::Key::BrowserBack);
    passi(&mut h, 6);
    h.get_by_label("Non salvare").click();
    passi(&mut h, 6);
    h.state_mut().apri_pdf_dalla_cartella(&foto);
    finche(&mut h, 20, "il PDF della foto", |a| a.misure_del_pdf().len() == 1);
    aspetta_il_pdf(&mut h);
    passi(&mut h, 20);
    h.get_by_label("Strumenti").click();
    passi(&mut h, 12);
    h.get_by_label("Comprimi").scroll_to_me();
    passi(&mut h, 4);
    h.get_by_label("Comprimi").click();
    passi(&mut h, 12);
    h.get_by_label("Media · Come «Riduci dimensioni» di Acrobat").click();
    passi(&mut h, 4);
    finche(&mut h, 30, "la copia compressa", |a| !a.lavoro_file_in_corso());
    passi(&mut h, 12);
    h.get_by_label("PDF compresso");
    salva(&mut h, "pdf-comprimi");
    h.get_by_label("Ho capito").click();
    passi(&mut h, 12);
    // Proteggi: la password coi punti e l'avviso.
    h.get_by_label("Strumenti").click();
    passi(&mut h, 12);
    h.get_by_label("Proteggi").scroll_to_me();
    passi(&mut h, 4);
    h.get_by_label("Proteggi").click();
    passi(&mut h, 12);
    h.event(egui::Event::Text("cartellone".into()));
    passi(&mut h, 12);
    h.get_by_label_contains("non si recupera");
    salva(&mut h, "pdf-proteggi");
}

/// Scrivere comodo (piano «Word completo», W1), sulla relazione dei bozzetti: word-cursore.png
/// col cursore messo da un tocco dopo «prescrizione» e la sua goccia trascinata fin dopo
/// «terapia», con la lente; word-sostituisci.png con Trova («letto») e la seconda riga aperta,
/// con «posto letto» nel campo; poi Tutti e il suo avviso. Una prova a parte, per lanciarla da
/// sola.
#[test]
fn schermate_di_word() {
    let _una = una_alla_volta();
    let prova = Path::new(env!("CARGO_TARGET_TMPDIR")).join("prove-schermata-word");
    let _ = std::fs::remove_dir_all(&prova);
    telefono_finto(&prova);
    let piattaforma = Piattaforma::con_cartelle(prova.join("app"), prova.join("cache"), prova.join("radice"));
    let app = TascabileApp::new(piattaforma, Instant::now());
    let mut h = Harness::builder().with_size(misura()).with_pixels_per_point(densita()).with_step_dt(1.0 / 30.0).wgpu().build_ui_state(disegna, app);
    h.run_steps(4);
    h.get_by_label("Relazione turno notte").click();
    passi(&mut h, 30);
    h.get_by_label("Salvato · Documenti");
    // Il tocco dopo «prescrizione»: il cursore e, sotto, la goccia.
    let (x, cima, fondo) = bordo_nella_relazione(nel_paragrafo("prescrizione", false));
    tocca_in(&mut h, egui::pos2(x + 1.0, (cima + fondo) / 2.0));
    passi(&mut h, 4);
    // La goccia presa nel suo tondo e portata dopo «terapia»: a metà strada la schermata.
    let presa = egui::pos2(x, fondo + tascabile_touch::CENTRO_GOCCIA_CURSORE);
    let (x2, c2, f2) = bordo_nella_relazione(nel_paragrafo("terapia", false));
    let spostamento = egui::pos2(x2, (c2 + f2) / 2.0) - egui::pos2(x, (cima + fondo) / 2.0);
    dito(&mut h, egui::TouchPhase::Start, presa);
    h.step();
    for k in 1..=12 {
        dito(&mut h, egui::TouchPhase::Move, presa + spostamento * (k.min(10) as f32 / 10.0));
        h.step();
    }
    let img = h.render().expect("disegno wgpu senza finestra");
    img.save(cartella_delle_schermate().join("word-cursore.png")).unwrap();
    // Sotto il cursore nuovo, dopo «terapia», la goccia blu: il suo tondo pieno di blu.
    let ppp = densita();
    let tondo = egui::pos2(x2, f2 + tascabile_touch::CENTRO_GOCCIA_CURSORE);
    let blu = (-6..=6)
        .flat_map(|dy| (-6..=6).map(move |dx| (dx, dy)))
        .filter(|(dx, dy)| img.get_pixel(((tondo.x + *dx as f32) * ppp) as u32, ((tondo.y + *dy as f32) * ppp) as u32).0[..3] == [0x23, 0x46, 0xA0])
        .count();
    assert!(blu > 100, "manca la goccia del cursore sotto «terapia»: {blu} punti blu");
    dito(&mut h, egui::TouchPhase::End, presa + spostamento);
    h.event(egui::Event::PointerGone);
    passi(&mut h, 6);

    // Trova «letto» e la seconda riga: «posto letto» nel campo Sostituisci con.
    h.get_by_label("Menu").click();
    passi(&mut h, 6);
    h.get_by_label("Trova").click();
    passi(&mut h, 6);
    h.event(egui::Event::Text("letto".into()));
    passi(&mut h, 6);
    h.get_by_label("Trova e sostituisci").click();
    passi(&mut h, 6);
    h.event(egui::Event::Text("posto letto".into()));
    passi(&mut h, 6);
    // Il cursore è dopo «terapia»: il primo risultato dopo di lui è il secondo.
    h.get_by_label("2 di 2");
    h.get_by_value("posto letto");
    let img = salva(&mut h, "word-sostituisci");
    // La pillola bianca del campo nuovo sotto la barra di Trova, sulla carta.
    let bianchi = (0..misura().x as u32)
        .filter(|x| img.get_pixel((*x as f32 * densita()) as u32, (92.0 * densita()) as u32).0[..3] == [0xFF, 0xFF, 0xFF])
        .count();
    assert!(bianchi > 100, "manca il campo «Sostituisci con»: {bianchi} punti bianchi");
    // Tutti: l'avviso di quanti. «posto letto» contiene ancora «letto»: i risultati restano due.
    h.get_by_label("Tutti").click();
    passi(&mut h, 4);
    h.get_by_label("Sostituiti 2");
    h.get_by_label_contains("di 2");
}

/// Le tabelle (piano «Word completo», W2), sulla relazione dei bozzetti: dopo «prescrizione» una
/// tabella di 3 per 3 dalla griglia del foglio Inserisci (word-tabella-griglia.png), scritta cella
/// per cella toccandole, col cursore nell'ultima e la barra della tabella sopra quella di formato
/// (word-tabella.png); poi il dito dalla cella del cursore che sceglie quattro celle, con Unisci
/// acceso nella barra scorsa fin lì (word-tabella-barra.png); unite, salvate e rilette dal file.
#[test]
fn schermate_delle_tabelle() {
    let _una = una_alla_volta();
    let prova = Path::new(env!("CARGO_TARGET_TMPDIR")).join("prove-schermata-tabelle");
    let _ = std::fs::remove_dir_all(&prova);
    telefono_finto(&prova);
    let piattaforma = Piattaforma::con_cartelle(prova.join("app"), prova.join("cache"), prova.join("radice"));
    let app = TascabileApp::new(piattaforma, Instant::now());
    let mut h = Harness::builder().with_size(misura()).with_pixels_per_point(densita()).with_step_dt(1.0 / 30.0).wgpu().build_ui_state(disegna, app);
    h.run_steps(4);
    h.get_by_label("Relazione turno notte").click();
    passi(&mut h, 30);
    // Il cursore dopo «prescrizione», poi Inserisci e Tabella: la griglia, a 3 per 3.
    let (x, cima, fondo) = bordo_nella_relazione(nel_paragrafo("prescrizione", false));
    tocca_in(&mut h, egui::pos2(x + 1.0, (cima + fondo) / 2.0));
    h.get_by_label("Inserisci").click();
    passi(&mut h, 12);
    h.get_by_label("Tabella").click();
    passi(&mut h, 12);
    salva(&mut h, "word-tabella-griglia");
    h.get_by_label("Inserisci la tabella").click();
    passi(&mut h, 12);
    // Una cella alla volta: il tocco ci mette il cursore e la tastiera scrive lì.
    let celle = [["Ora", "Letto", "Nota"], ["02:30", "12", "NRS 6, terapia"], ["05:00", "4", "Mobilizzato"]];
    for (r, riga) in celle.iter().enumerate() {
        for (c, testo) in riga.iter().enumerate() {
            if (r, c) != (0, 0) {
                let p = h.state_mut().centro_cella_di_prova(r, c).expect("la cella sullo schermo");
                tocca_in(&mut h, p);
            }
            h.event(egui::Event::Text((*testo).into()));
            passi(&mut h, 4);
        }
    }
    // La tastiera giù: la barra della tabella sopra quella di formato, in fondo.
    h.key_press(egui::Key::BrowserBack);
    passi(&mut h, 30);
    h.get_by_label("Riga sopra");
    let img = salva(&mut h, "word-tabella");
    // Le due barre bianche: quella della tabella sopra quella di formato.
    let formato = h.get_by_label("Formato").rect();
    let riga = h.get_by_label("Riga sopra").rect();
    assert!(riga.bottom() < formato.top(), "{riga:?} {formato:?}");
    let bianco = |y: f32| {
        (40..(misura().x - 40.0) as u32)
            .filter(|x| img.get_pixel((*x as f32 * densita()) as u32, (y * densita()) as u32).0[..3] == [0xFF, 0xFF, 0xFF])
            .count()
    };
    assert!(bianco(riga.top() + 2.0) > 250, "manca la barra della tabella");

    // Il dito dalla cella del cursore, (2, 2), fino a (1, 1): quattro celle scelte.
    let da = h.state_mut().centro_cella_di_prova(2, 2).unwrap();
    let a = h.state_mut().centro_cella_di_prova(1, 1).unwrap();
    dito(&mut h, egui::TouchPhase::Start, da);
    h.step();
    for k in 1..=10 {
        dito(&mut h, egui::TouchPhase::Move, da + (a - da) * (k as f32 / 10.0));
        h.step();
    }
    dito(&mut h, egui::TouchPhase::End, a);
    h.event(egui::Event::PointerGone);
    passi(&mut h, 10);
    // La barra scorsa col dito fino a Unisci, acceso con quattro celle.
    let y = h.get_by_label("Riga sopra").rect().center().y;
    let da = egui::pos2(340.0, y);
    h.event(egui::Event::PointerMoved(da));
    h.event(egui::Event::PointerButton { pos: da, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
    h.step();
    for k in 1..=10 {
        h.event(egui::Event::PointerMoved(da - egui::vec2(24.0 * k as f32, 0.0)));
        h.step();
    }
    let a = da - egui::vec2(240.0, 0.0);
    h.event(egui::Event::PointerButton { pos: a, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
    h.event(egui::Event::PointerGone);
    passi(&mut h, 40);
    let unisci = h.get_by_label("Unisci");
    assert!(!unisci.accesskit_node().is_disabled(), "Unisci spento con quattro celle scelte");
    assert!(unisci.rect().right() < 404.0, "Unisci fuori dalla barra: {:?}", unisci.rect());
    salva(&mut h, "word-tabella-barra");
    h.get_by_label("Unisci").click();
    passi(&mut h, 6);
    // Salva e rileggi dal file: le celle unite restano, col testo di tutte e quattro.
    h.get_by_label("Menu").click();
    passi(&mut h, 6);
    h.get_by_label("Salva").click();
    passi(&mut h, 20);
    let byte = std::fs::read(prova.join("radice/Documents/Relazione turno notte.docx")).unwrap();
    let s = documento::apri_byte("relazione.docx", &byte).unwrap();
    let t = s.doc.body.iter().find_map(|b| b.as_table()).expect("la tabella nel file");
    assert_eq!(t.rows.len(), 3);
    assert_eq!(t.rows[1].cells.len(), 2, "{:?}", t.rows.iter().map(|r| r.cells.len()).collect::<Vec<_>>());
    let testo: Vec<String> = t.rows[1].cells[1].blocks.iter().filter_map(|b| b.as_para()).map(|p| p.text.clone()).collect();
    assert_eq!(testo, ["12", "NRS 6, terapia", "4", "Mobilizzato"]);
}

/// Una foto finta per il documento: un cielo che sfuma, un prato verde e un sole giallo, larga
/// `w` e alta `h` pixel, in JPEG come quelle della fotocamera.
fn foto_finta(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbImage::from_fn(w, h, |x, y| {
        let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
        let sole = ((fx - 0.75).powi(2) + (fy - 0.25).powi(2) * 0.6).sqrt() < 0.09;
        if sole {
            image::Rgb([0xF6, 0xC9, 0x3B])
        } else if fy > 0.62 + 0.05 * (fx * 9.0).sin() {
            image::Rgb([0x4E, (0x9A as f32 - 30.0 * fy) as u8, 0x3C])
        } else {
            image::Rgb([(0x8E as f32 + 60.0 * fy) as u8, (0xC2 as f32 + 30.0 * fy) as u8, 0xEC])
        }
    });
    let mut out = Vec::new();
    image::DynamicImage::ImageRgb8(img).write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Jpeg).unwrap();
    out
}

/// Le immagini e la pagina (piano «Word completo», W3), sulla relazione dei bozzetti: dopo
/// «prescrizione» una foto dalla galleria (finta, 1600 per 1200 pixel), inserita larga quanto il
/// testo dello schermo e scelta, col riquadro, la maniglia e la barra dell'immagine
/// (word-immagine.png); la maniglia tirata a due terzi, la misura nuova; dal menu il foglio
/// «Pagina» sulla vista Pagina, in A4 orizzontale coi margini stretti (word-pagina.png); poi
/// intestazione, piè di pagina e «Pagina N di M» in basso a destra, che si vedono sulle pagine
/// (word-intestazione.png); salvata, riletta dal file.
#[test]
fn schermate_delle_immagini_e_della_pagina() {
    let _una = una_alla_volta();
    let prova = Path::new(env!("CARGO_TARGET_TMPDIR")).join("prove-schermata-immagini");
    let _ = std::fs::remove_dir_all(&prova);
    telefono_finto(&prova);
    let piattaforma = Piattaforma::con_cartelle(prova.join("app"), prova.join("cache"), prova.join("radice"));
    let app = TascabileApp::new(piattaforma, Instant::now());
    let mut h = Harness::builder().with_size(misura()).with_pixels_per_point(densita()).with_step_dt(1.0 / 30.0).wgpu().build_ui_state(disegna, app);
    h.run_steps(4);
    h.get_by_label("Relazione turno notte").click();
    passi(&mut h, 30);
    // Il cursore dopo «prescrizione», poi Inserisci, Immagine, Dalla galleria.
    let (x, cima, fondo) = bordo_nella_relazione(nel_paragrafo("prescrizione", false));
    tocca_in(&mut h, egui::pos2(x + 1.0, (cima + fondo) / 2.0));
    h.get_by_label("Inserisci").click();
    passi(&mut h, 12);
    h.get_by_label("Immagine").click();
    passi(&mut h, 12);
    h.get_by_label("Dalla galleria").click();
    passi(&mut h, 4);
    h.state().simula_foto_di_prova(&foto_finta(1600, 1200));
    finche(&mut h, 20, "la foto pronta", |a| !a.immagine_in_arrivo());
    passi(&mut h, 30);
    let r = h.state_mut().immagine_sullo_schermo_di_prova().expect("l'immagine scelta sullo schermo");
    assert!(r.width() > 300.0 && r.right() <= 404.0, "larga quanto il testo dello schermo: {r:?}");
    // Al centro, poi la schermata.
    h.get_by_label("Allinea").click();
    passi(&mut h, 6);
    h.get_by_label("Allinea al centro").click();
    passi(&mut h, 10);
    let img = salva(&mut h, "word-immagine");
    let r = h.state_mut().immagine_sullo_schermo_di_prova().unwrap();
    // Il riquadro blu attorno all'immagine e la maniglia bianca nel suo angolo.
    let ppp = densita();
    let px = |p: egui::Pos2| img.get_pixel((p.x * ppp) as u32, (p.y * ppp) as u32).0;
    assert_eq!(px(egui::pos2(r.center().x, r.top() - 1.0))[..3], [0x23, 0x46, 0xA0], "manca il riquadro sopra l'immagine");
    assert_eq!(px(r.max)[..3], [0xFF, 0xFF, 0xFF], "manca la maniglia nell'angolo");
    // Il sole giallo della foto, dentro l'immagine.
    let sole = px(egui::pos2(r.left() + r.width() * 0.75, r.top() + r.height() * 0.25));
    assert!(sole[0] > 0xD0 && sole[1] > 0xA0 && sole[2] < 0x80, "{sole:?}");
    h.get_by_label("Testo intorno");

    // La maniglia tirata fino a due terzi della larghezza.
    let da = r.max;
    let a = r.min + r.size() * (2.0 / 3.0);
    dito(&mut h, egui::TouchPhase::Start, da);
    h.step();
    for k in 1..=10 {
        dito(&mut h, egui::TouchPhase::Move, da + (a - da) * (k as f32 / 10.0));
        h.step();
    }
    dito(&mut h, egui::TouchPhase::End, a);
    h.event(egui::Event::PointerGone);
    passi(&mut h, 10);
    let piccola = h.state_mut().immagine_sullo_schermo_di_prova().expect("resta scelta");
    assert!((piccola.width() - r.width() * 2.0 / 3.0).abs() < 2.0, "{piccola:?} invece di due terzi di {r:?}");

    // Il foglio «Pagina» dal menu: dietro, la vista Pagina; A4 orizzontale, margini stretti.
    h.get_by_label("Menu").click();
    passi(&mut h, 6);
    h.get_by_label("Pagina").click();
    passi(&mut h, 12);
    h.get_by_label("Orizzontale").click();
    passi(&mut h, 6);
    h.get_by_label("Stretti").click();
    passi(&mut h, 20);
    passi(&mut h, 30);
    h.get_by_label("29,7 × 21 cm");
    salva(&mut h, "word-pagina");

    // Intestazione e piè di pagina, coi numeri in basso a destra.
    h.get_by_label("Intestazione e piè di pagina").click();
    passi(&mut h, 12);
    h.get_by_value("Testo in cima a ogni pagina").click();
    passi(&mut h, 4);
    h.event(egui::Event::Text("Reparto di medicina".into()));
    passi(&mut h, 4);
    h.get_by_value("Testo in fondo a ogni pagina").click();
    passi(&mut h, 4);
    h.event(egui::Event::Text("Relazione del turno di notte".into()));
    passi(&mut h, 4);
    h.get_by_label("A destra").click();
    passi(&mut h, 6);
    h.get_by_label("Pagina N di M").click();
    passi(&mut h, 6);
    h.get_by_label("Applica").click();
    passi(&mut h, 30);
    passi(&mut h, 30);
    let img = salva(&mut h, "word-intestazione");
    // La pagina orizzontale: l'intestazione in cima e il piè di pagina in fondo (i pixel grigi del
    // loro testo nella fascia di ciascuno).
    let scuri = |y0: f32, y1: f32| {
        let mut n = 0;
        for y in (y0 * ppp) as u32..(y1 * ppp) as u32 {
            for x in (20.0 * ppp) as u32..((misura().x - 20.0) * ppp) as u32 {
                let p = img.get_pixel(x, y).0;
                if p[0] < 180 && p[1] < 180 && p[2] < 180 {
                    n += 1;
                }
            }
        }
        n
    };
    // La prima pagina comincia sotto la barra alta (64 dp) più 14; larga 380 dp, alta 380 · 21 / 29,7.
    let alta = pagina_dp() * 21.0 / 29.7;
    assert!(scuri(78.0 + 6.0, 78.0 + 24.0) > 40, "manca l'intestazione in cima alla pagina");
    assert!(scuri(78.0 + alta - 22.0, 78.0 + alta - 4.0) > 40, "manca il piè di pagina in fondo alla pagina");

    // Salva e rileggi dal file: l'immagine, la pagina, intestazione, piè di pagina e numeri.
    h.get_by_label("Menu").click();
    passi(&mut h, 6);
    h.get_by_label("Salva").click();
    passi(&mut h, 20);
    let byte = std::fs::read(prova.join("radice/Documents/Relazione turno notte.docx")).unwrap();
    let s = documento::apri_byte("relazione.docx", &byte).unwrap();
    let p = s.doc.body[PARAGRAFO_DEL_CURSORE + 1].as_para().unwrap();
    let o = p.objects.iter().find_map(|o| match o {
        wordcraft_engine::doc::para::InlineObject::Image { w, h, media, .. } => Some((*w, *h, media.clone())),
        _ => None,
    });
    let (w, alto, media) = o.expect("l'immagine nel file");
    assert!((w / alto - 4.0 / 3.0).abs() < 0.01, "le proporzioni della foto: {w} per {alto}");
    assert!(s.doc.media.get(&media).is_some_and(|b| b.len() > 1000), "la foto nel file");
    let i = tascabile_word::pagina::info_pagina(&s);
    assert_eq!(
        (i.formato, i.orizzontale, i.margini),
        (Some(tascabile_word::pagina::Formato::A4), true, Some(tascabile_word::pagina::Margini::Stretti))
    );
    let ip = tascabile_word::pagina::intestazione_e_pie(&s);
    assert_eq!((ip.intestazione.as_str(), ip.pie.as_str()), ("Reparto di medicina", "Relazione del turno di notte"));
    assert_eq!((ip.numeri, ip.di_totale), (tascabile_word::pagina::NumeriPagina::ADestra, true));
}

/// L'ortografia (piano «Word completo», W4), sulla relazione dei bozzetti: dopo «prescrizione» si
/// scrive « del medico, pazziente tranquillo»; «pazziente» ha la linea rossa ondulata e un tocco su
/// di lei mostra sopra la barra coi suggerimenti, Aggiungi al dizionario e Ignora
/// (word-ortografia.png); il primo suggerimento la corregge.
#[test]
fn schermate_dell_ortografia() {
    let _una = una_alla_volta();
    let prova = Path::new(env!("CARGO_TARGET_TMPDIR")).join("prove-schermata-ortografia");
    let _ = std::fs::remove_dir_all(&prova);
    telefono_finto(&prova);
    let piattaforma = Piattaforma::con_cartelle(prova.join("app"), prova.join("cache"), prova.join("radice"));
    let app = TascabileApp::new(piattaforma, Instant::now());
    let mut h = Harness::builder().with_size(misura()).with_pixels_per_point(densita()).with_step_dt(1.0 / 30.0).wgpu().build_ui_state(disegna, app);
    h.run_steps(4);
    h.get_by_label("Relazione turno notte").click();
    passi(&mut h, 30);
    finche(&mut h, 20, "il dizionario pronto", |a| a.dizionario_pronto());
    // Il cursore dopo «prescrizione», poi la parola sbagliata scritta con la tastiera.
    let (x, cima, fondo) = bordo_nella_relazione(nel_paragrafo("prescrizione", false));
    tocca_in(&mut h, egui::pos2(x + 1.0, (cima + fondo) / 2.0));
    for parola in [" del medico,", " pazziente", " tranquillo"] {
        h.event(egui::Event::Text(parola.into()));
        passi(&mut h, 4);
    }
    // La tastiera giù, poi il tocco a metà di «pazziente».
    h.key_press(egui::Key::BrowserBack);
    passi(&mut h, 30);
    let r = h.state_mut().parola_sullo_schermo_di_prova("pazziente").expect("la parola sullo schermo");
    // La linea rossa ondulata sotto la parola: fra la metà della riga e il suo fondo.
    let img = h.render().expect("disegno wgpu senza finestra");
    let ppp = densita();
    let rossi = |img: &image::RgbaImage| {
        let mut n = 0;
        for y in (r.center().y * ppp) as u32..(r.bottom() * ppp) as u32 {
            for x in (r.left() * ppp) as u32..(r.right() * ppp) as u32 {
                let p = img.get_pixel(x, y).0;
                if p[0] > 180 && p[1] < 120 && p[2] < 120 {
                    n += 1;
                }
            }
        }
        n
    };
    assert!(rossi(&img) > 40, "manca la linea rossa sotto «pazziente»: {} punti rossi", rossi(&img));
    tocca_in(&mut h, r.center());
    passi(&mut h, 10);
    h.get_by_label("Aggiungi al dizionario");
    h.get_by_label("Ignora");
    let img = salva(&mut h, "word-ortografia");
    // Il primo suggerimento è «paziente», sopra la parola.
    let paziente = h.get_by_label("paziente").rect();
    assert!(paziente.bottom() < r.top(), "{paziente:?} {r:?}");
    assert!(rossi(&img) > 40);
    h.get_by_label("paziente").click();
    passi(&mut h, 10);
    let testi = h.state().testi_word_di_prova();
    assert!(testi[PARAGRAFO_DEL_CURSORE].ends_with("prescrizione del medico, paziente tranquillo"), "{:?}", testi[PARAGRAFO_DEL_CURSORE]);
    assert!(h.query_by_label("Ignora").is_none(), "la barra se ne va");
}

/// I modelli (piano «Word completo», W4): il foglio «Nuovo» della Home col documento vuoto, i
/// quattro modelli e Scansiona (word-modelli.png); poi il Verbale aperto, col titolo, i campi in
/// grassetto e l'ordine del giorno numerato (word-modello-verbale.png).
#[test]
fn schermate_dei_modelli() {
    let _una = una_alla_volta();
    let prova = Path::new(env!("CARGO_TARGET_TMPDIR")).join("prove-schermata-modelli");
    let _ = std::fs::remove_dir_all(&prova);
    telefono_finto(&prova);
    let piattaforma = Piattaforma::con_cartelle(prova.join("app"), prova.join("cache"), prova.join("radice"));
    let app = TascabileApp::new(piattaforma, Instant::now());
    let mut h = Harness::builder().with_size(misura()).with_pixels_per_point(densita()).with_step_dt(1.0 / 30.0).wgpu().build_ui_state(disegna, app);
    h.run_steps(4);
    h.get_by_label("Nuovo").click();
    passi(&mut h, 20);
    for voce in ["Documento vuoto", "Verbale", "Relazione", "Lettera", "Lezione", "Scansiona"] {
        let r = h.get_by_label(voce).rect();
        assert!(r.bottom() <= misura().y, "«{voce}» fuori dallo schermo: {r:?}");
    }
    salva(&mut h, "word-modelli");
    h.get_by_label("Verbale").click();
    passi(&mut h, 30);
    finche(&mut h, 20, "il dizionario pronto", |a| a.dizionario_pronto());
    passi(&mut h, 10);
    h.get_by_label("Non ancora salvato");
    let img = salva(&mut h, "word-modello-verbale");
    // Il titolo blu del Titolo 1 in cima alla pagina.
    let blu = (0..misura().x as u32)
        .flat_map(|x| (110..140u32).map(move |y| (x, y)))
        .filter(|(x, y)| {
            let p = img.get_pixel((*x as f32 * densita()) as u32, (*y as f32 * densita()) as u32).0;
            p[2] as u32 > p[0] as u32 + 40
        })
        .count();
    assert!(blu > 100, "manca il titolo del verbale: {blu}");
    // Nessuna parola del modello è sottolineata.
    assert!(h.state_mut().parola_sullo_schermo_di_prova("Presenti").is_some());
}

/// La pressione lunga del dito in `pos`: la parola sotto si sceglie, con la barra della selezione.
fn premi_a_lungo(h: &mut Harness<'_, TascabileApp>, pos: egui::Pos2) {
    dito(h, egui::TouchPhase::Start, pos);
    passi(h, 25);
    dito(h, egui::TouchPhase::End, pos);
    h.event(egui::Event::PointerGone);
    passi(h, 10);
}

/// Commenti e revisioni (piano «Word completo», W4), sulla relazione dei bozzetti con l'autore
/// «Anna Bianchi» nelle Impostazioni: dal menu il foglio «Revisioni» e «Tieni traccia delle
/// modifiche»; dopo «prescrizione» si scrive « del medico» (un'aggiunta), «regolare» scelto col
/// la pressione lunga e cancellato (una cancellazione), «Mobilizzazione» commentata dalla barra della
/// selezione; il testo coi colori del motore (word-revisioni-testo.png) e il foglio con le due
/// modifiche e il commento (word-revisioni.png); poi Accetta tutto.
#[test]
fn schermate_delle_revisioni() {
    let _una = una_alla_volta();
    let prova = Path::new(env!("CARGO_TARGET_TMPDIR")).join("prove-schermata-revisioni");
    let _ = std::fs::remove_dir_all(&prova);
    telefono_finto(&prova);
    std::fs::write(prova.join("app/autore.txt"), "Anna Bianchi").unwrap();
    let piattaforma = Piattaforma::con_cartelle(prova.join("app"), prova.join("cache"), prova.join("radice"));
    let app = TascabileApp::new(piattaforma, Instant::now());
    let mut h = Harness::builder().with_size(misura()).with_pixels_per_point(densita()).with_step_dt(1.0 / 30.0).wgpu().build_ui_state(disegna, app);
    h.run_steps(4);
    h.get_by_label("Relazione turno notte").click();
    passi(&mut h, 30);
    finche(&mut h, 20, "il dizionario pronto", |a| a.dizionario_pronto());
    // Le revisioni accese dal foglio.
    h.get_by_label("Menu").click();
    passi(&mut h, 6);
    h.get_by_label("Revisioni").click();
    passi(&mut h, 12);
    h.get_by_label("Tieni traccia delle modifiche").click();
    passi(&mut h, 6);
    h.key_press(egui::Key::BrowserBack);
    passi(&mut h, 20);
    // Un'aggiunta dopo «prescrizione».
    let (x, cima, fondo) = bordo_nella_relazione(nel_paragrafo("prescrizione", false));
    tocca_in(&mut h, egui::pos2(x + 1.0, (cima + fondo) / 2.0));
    h.event(egui::Event::Text(" del medico".into()));
    passi(&mut h, 6);
    // «regolare» scelto con la pressione lunga, poi cancellato.
    let r = h.state_mut().parola_sullo_schermo_di_prova("regolare").unwrap();
    premi_a_lungo(&mut h, r.center());
    h.key_press(egui::Key::Backspace);
    passi(&mut h, 6);
    // «Mobilizzazione» con la pressione lunga, poi Commenta.
    let r = h.state_mut().parola_sullo_schermo_di_prova("Mobilizzazione").unwrap();
    premi_a_lungo(&mut h, r.center());
    h.get_by_label("Commenta").click();
    passi(&mut h, 12);
    h.get_by_label_contains("col nome Anna Bianchi");
    h.event(egui::Event::Text("Ogni quanto, di preciso?".into()));
    passi(&mut h, 6);
    h.get_by_label("Fatto").click();
    passi(&mut h, 30);
    let img = salva(&mut h, "word-revisioni-testo");
    // Il viola tenue del testo commentato sotto «Mobilizzazione».
    let r = h.state_mut().parola_sullo_schermo_di_prova("Mobilizzazione").unwrap();
    let viola = img.get_pixel((r.left() * densita()) as u32 + 2, ((r.top() + 2.0) * densita()) as u32).0;
    assert_eq!(viola[..3], [0xEF, 0xE3, 0xF7], "manca il colore del commento");
    // Il foglio con le due modifiche e il commento.
    h.get_by_label("Menu").click();
    passi(&mut h, 6);
    h.get_by_label("Revisioni").click();
    passi(&mut h, 20);
    h.get_by_label("Ogni quanto, di preciso?");
    assert_eq!(h.get_all_by_label("Accetta").count(), 2);
    // L'autore sotto le due modifiche e sopra il commento.
    assert_eq!(h.query_all_by_label_contains("Anna Bianchi").count(), 3);
    salva(&mut h, "word-revisioni");
    h.get_by_label("Accetta tutto").click();
    passi(&mut h, 10);
    let testi = h.state().testi_word_di_prova();
    let p = testi[PARAGRAFO_DEL_CURSORE].replace('\u{fffc}', "");
    assert!(p.starts_with("Notte . Alle") && p.ends_with("prescrizione del medico"), "{p:?}");
}
