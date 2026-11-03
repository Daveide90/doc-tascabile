//! La galleria dei componenti del tema (tappa A1), a misura di telefono come le altre prove a
//! schermo (412 × 915 dp, densità 2,625, wgpu senza finestra). Le immagini vanno in
//! target/tmp/schermate/: galleria.png (i componenti in linea, le barre, il pulsante «Nuovo» e
//! l'avviso breve), galleria-foglio.png (il foglio dal fondo coi controlli, come il bozzetto 4),
//! galleria-menu.png (bozzetto 6) e galleria-dialogo.png, da guardare accanto ai bozzetti di
//! design/bozzetti/png.

use eframe::egui;
use egui::text::CCursor;
use egui::{Color32, CornerRadius, FontId, Id, Rect, Ui, UiBuilder, pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use tascabile_touch::tema::{self, Carattere};
use tascabile_touch::{
    AvvisoBreve, BarraAlta, BarraContesto, BarraFlottante, Bordo, Chip, Dialogo, FoglioDalFondo, GruppoSegmentato, Maniglia, Menu, Passo, Peso,
    Pulsante, PulsanteIcona, PulsanteNuovo, Scheda, Tipo, Voce, etichetta_sezione, goccia, icone,
};

const SCHERMO: egui::Vec2 = egui::vec2(412.0, 915.0);

/// Un banco a misura di telefono col tema installato; la `ui` data a `app` copre lo schermo, sul
/// fondo carta.
fn telefono<S: 'static>(mut app: impl FnMut(&mut Ui, &mut S) + 'static, stato: S) -> Harness<'static, S> {
    let tutto = move |ui: &mut Ui, s: &mut S| {
        let zona = ui.ctx().content_rect();
        ui.painter().rect_filled(zona, 0.0, tema::CARTA);
        ui.scope_builder(UiBuilder::new().max_rect(zona), |ui| app(ui, s));
    };
    let mut h = Harness::builder().with_size(SCHERMO).with_pixels_per_point(2.625).with_step_dt(1.0 / 60.0).wgpu().build_ui_state(tutto, stato);
    tema::installa(&h.ctx);
    // Abbastanza passi perché il foglio finisca di salire.
    h.run_steps(30);
    h
}

/// Salva l'immagine in target/tmp/schermate e la restituisce.
fn salva<S>(h: &mut Harness<'_, S>, nome: &str) -> image::RgbaImage {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("schermate");
    std::fs::create_dir_all(&dir).unwrap();
    let img = h.render().expect("disegno wgpu senza finestra");
    img.save(dir.join(format!("{nome}.png"))).unwrap();
    img
}

/// Quanti pixel dell'immagine hanno proprio questo colore.
fn pixel_di(img: &image::RgbaImage, colore: Color32) -> usize {
    img.pixels().filter(|p| p.0 == colore.to_array()).count()
}

/// La barra alta del documento, come nei bozzetti 2, 4, 5 e 6.
fn barra_del_documento(ui: &mut Ui) {
    BarraAlta::new("Relazione turno notte")
        .indietro("Indietro")
        .sottotitolo("Salvato · Documenti")
        .stato(icone::CHECK_CIRCLE, tema::VERDE)
        .azione(PulsanteIcona::new(icone::UNDO, "Annulla"))
        .azione(PulsanteIcona::new(icone::REDO, "Ripeti").spento(true))
        .azione(PulsanteIcona::new(icone::MORE_VERT, "Menu"))
        .mostra(ui);
}

/// La barra di formato del bozzetto 2.
fn barra_di_formato(ui: &mut Ui, zona: Rect) {
    BarraFlottante::new()
        .pulsante(PulsanteIcona::new("Aa", "Formato").testo(Carattere::Titolo, 21.0))
        .separatore()
        .pulsante(PulsanteIcona::new(icone::FORMAT_BOLD, "Grassetto").acceso(true))
        .pulsante(PulsanteIcona::new(icone::FORMAT_ITALIC, "Corsivo").acceso(false))
        .pulsante(PulsanteIcona::new(icone::FORMAT_UNDERLINED, "Sottolineato").acceso(false))
        .separatore()
        .pulsante(PulsanteIcona::new(icone::FORMAT_LIST_BULLETED, "Elenco puntato"))
        .pulsante(PulsanteIcona::new(icone::FORMAT_LIST_NUMBERED, "Elenco numerato"))
        .pulsante(PulsanteIcona::new(icone::ADD_CIRCLE, "Inserisci"))
        .mostra(ui, BarraFlottante::posto(zona));
}

/// La pagina del documento sulla carta, come nei bozzetti (qui col carattere del tema: nell'app
/// la disegna il motore). Restituisce il paragrafo da selezionare e dove sta.
fn pagina(ui: &mut Ui, zona: Rect) -> (std::sync::Arc<egui::Galley>, egui::Pos2) {
    let foglio = Rect::from_min_max(pos2(zona.left() + 10.0, zona.top() + tema::BARRA_ALTA), zona.max);
    let raggio = CornerRadius { nw: tema::RAGGIO_PAGINA, ne: tema::RAGGIO_PAGINA, sw: 0, se: 0 };
    let foglio = Rect::from_min_max(foglio.min, pos2(zona.right() - 10.0, zona.bottom()));
    tema::dipingi_ombra(ui.painter(), foglio, raggio, &tema::OMBRA);
    ui.painter().rect_filled(foglio, raggio, tema::FOGLIO);
    let titoli = Color32::from_rgb(0x1F, 0x3B, 0x63);
    let testo = Color32::from_rgb(0x22, 0x22, 0x22);
    let largo = foglio.width() - 48.0;
    let mut y = foglio.top() + 26.0;
    let mut paragrafo = |testo: &str, font: FontId, colore: Color32, dopo: f32| {
        let g = ui.painter().layout(testo.to_owned(), font, colore, largo);
        let dove = pos2(foglio.left() + 24.0, y);
        y += g.size().y + dopo;
        ui.painter().galley(dove, g.clone(), colore);
        (g, dove)
    };
    let ctx = ui.ctx().clone();
    paragrafo("Relazione turno di notte", Carattere::Grassetto.font(&ctx, 26.0), titoli, 14.0);
    paragrafo(
        "Turno dalle 21 alle 7, reparto di medicina. Presenti due infermieri e un operatore socio sanitario.",
        FontId::proportional(16.5),
        testo,
        18.0,
    );
    paragrafo("Andamento del turno", Carattere::Grassetto.font(&ctx, 19.0), titoli, 8.0);
    let scelto = paragrafo(
        "Notte regolare. Alle 2:30 rivalutazione del dolore al letto 12 con NRS 6, somministrata la terapia al bisogno come da prescrizione.",
        FontId::proportional(16.5),
        testo,
        12.0,
    );
    paragrafo("Da segnalare al mattino", Carattere::Grassetto.font(&ctx, 19.0), titoli, 8.0);
    paragrafo("Controllo della glicemia alle 7 al letto 7.", FontId::proportional(16.5), testo, 0.0);
    scelto
}

/// Una fila con il margine dei contenuti a sinistra.
fn fila(ui: &mut Ui, margine: f32, contenuto: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.add_space(margine);
        contenuto(ui);
    });
}

/// I componenti in linea, la selezione con le gocce e la barra del contesto, l'avviso breve, il
/// pulsante «Nuovo» e la barra di formato.
fn galleria(ui: &mut Ui, avviso: &mut AvvisoBreve) {
    let zona = ui.max_rect();
    barra_del_documento(ui);
    ui.add_space(4.0);
    fila(ui, tema::MARGINE_LARGO, |ui| {
        ui.add(Chip::new("Tutti").acceso(true));
        ui.add(Chip::new("Documenti"));
        ui.add(Chip::new("PDF"));
        ui.add(Chip::new("Slide"));
    });
    fila(ui, tema::MARGINE_LARGO, |ui| {
        ui.add(PulsanteIcona::new(icone::FORMAT_ITALIC, "Normale"));
        ui.add(PulsanteIcona::new(icone::FORMAT_BOLD, "Acceso").acceso(true));
        ui.add(PulsanteIcona::new(icone::REDO, "Spento").spento(true));
        ui.add(Pulsante::new("Annulla", Peso::Testo));
        ui.add(Pulsante::new("Salva", Peso::Principale));
    });
    ui.add_space(4.0);
    let scheda = Rect::from_min_max(pos2(zona.left() + tema::MARGINE, ui.cursor().top()), pos2(zona.right() - tema::MARGINE, zona.bottom()));
    ui.scope_builder(UiBuilder::new().max_rect(scheda), |ui| {
        Scheda::new().show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let voci = [
                (Tipo::Word, "Relazione turno notte", icone::FOLDER, "Documenti · oggi 08:12"),
                (Tipo::Pdf, "Consenso informato da firmare", icone::CHAT, "da WhatsApp · ieri 19:40"),
                (Tipo::Slide, "Lezione lesioni da pressione", icone::DOWNLOAD, "Download · 21 ottobre"),
                (Tipo::Cartella, "Turni di novembre", icone::FOLDER, "Documenti · 4 file"),
            ];
            for (i, (tipo, nome, icona, dove)) in voci.into_iter().enumerate() {
                Voce::new(tipo, nome).dove(icona, dove).menu("Altre azioni").ultima(i == 3).mostra(ui);
            }
        });
    });
    // Un paragrafo con una parte selezionata su due righe, le gocce e la barra del contesto.
    ui.add_space(86.0);
    let testo = "Notte regolare. Alle 2:30 rivalutazione del dolore al letto 12 con NRS 6, somministrata la terapia al bisogno come da prescrizione.";
    let g = ui.painter().layout(testo.to_owned(), FontId::proportional(16.5), tema::INCHIOSTRO, zona.width() - 2.0 * 34.0);
    let origine = pos2(zona.left() + 34.0, ui.cursor().top());
    let punto = |parola: &str, dopo: bool| {
        let i = testo.find(parola).unwrap() + if dopo { parola.len() } else { 0 };
        g.pos_from_cursor(CCursor::new(i)).translate(origine.to_vec2())
    };
    let (a, b) = (punto("somministrata", false), punto("bisogno", true));
    let riga_a =
        Rect::from_min_max(a.min, pos2(origine.x + g.rows.iter().find(|r| r.rect().top() + origine.y == a.top()).unwrap().rect().right(), a.max.y));
    let riga_b = Rect::from_min_max(pos2(origine.x, b.top()), b.max);
    ui.painter().rect_filled(riga_a, 2.0, tema::SELEZIONE);
    ui.painter().rect_filled(riga_b, 2.0, tema::SELEZIONE);
    ui.painter().galley(origine, g.clone(), tema::INCHIOSTRO);
    goccia(ui.painter(), Maniglia::Inizio, Bordo { x: a.left(), cima: a.top(), fondo: a.bottom() }, tema::BLU);
    goccia(ui.painter(), Maniglia::Fine, Bordo { x: b.right(), cima: b.top(), fondo: b.bottom() }, tema::BLU);
    BarraContesto::new()
        .azione(icone::CONTENT_CUT, "Taglia")
        .azione(icone::CONTENT_COPY, "Copia")
        .azione(icone::CONTENT_PASTE, "Incolla")
        .azione(icone::SELECT_ALL, "Tutto")
        .mostra(ui, riga_a.union(riga_b), zona);
    let sopra_la_barra = BarraFlottante::posto(zona).top();
    let nuovo = PulsanteNuovo::new("Nuovo").mostra(ui, Rect::from_min_max(zona.min, pos2(zona.right(), sopra_la_barra)));
    avviso.mostra(ui.ctx(), nuovo.rect.top());
    barra_di_formato(ui, zona);
}

/// Le quattro immagini una dopo l'altra: un disegno wgpu alla volta, come in schermata.rs.
#[test]
fn galleria_a_schermo() {
    galleria_dei_componenti();
    galleria_del_foglio();
    galleria_del_menu();
    galleria_del_dialogo();
}

fn galleria_dei_componenti() {
    let mut avviso = AvvisoBreve::new(Id::new("avviso"));
    avviso.avvisa("Copia salvata in Documenti");
    let mut h = telefono(galleria, avviso);
    h.get_by_label("Grassetto");
    h.get_by_label("Copia salvata in Documenti");
    let img = salva(&mut h, "galleria");
    // I colori pieni del tema arrivano a schermo così come sono: la carta, il foglio delle
    // schede, il blu di «Nuovo», lo scuro della barra del contesto, i fondi tenui dei tipi.
    for (nome, colore, almeno) in [
        ("carta", tema::CARTA, 100_000),
        ("foglio", tema::FOGLIO, 100_000),
        ("blu", tema::BLU, 20_000),
        ("scuro", tema::SCURO, 20_000),
        ("rosso tenue", tema::ROSSO_TENUE, 5_000),
        ("ocra tenue", tema::OCRA_TENUE, 5_000),
    ] {
        let n = pixel_di(&img, colore);
        assert!(n >= almeno, "{nome}: {n} pixel");
    }
}

const ALLINEAMENTI: [(&str, &str); 4] = [
    (icone::FORMAT_ALIGN_LEFT, "A sinistra"),
    (icone::FORMAT_ALIGN_CENTER, "Al centro"),
    (icone::FORMAT_ALIGN_RIGHT, "A destra"),
    (icone::FORMAT_ALIGN_JUSTIFY, "Giustificato"),
];
const RIENTRI: [(&str, &str); 2] = [(icone::FORMAT_INDENT_DECREASE, "Meno rientro"), (icone::FORMAT_INDENT_INCREASE, "Più rientro")];
const TESTO: [(&str, &str); 3] =
    [(icone::FORMAT_COLOR_TEXT, "Colore del testo"), (icone::FORMAT_INK_HIGHLIGHTER, "Evidenziatore"), (icone::FORMAT_CLEAR, "Togli il formato")];

/// Il foglio «Aa» del bozzetto 4, coi controlli che ci sono già (gli stili sono della tappa A4).
fn foglio(ui: &mut Ui, f: &mut FoglioDalFondo) {
    let zona = ui.max_rect();
    barra_del_documento(ui);
    pagina(ui, zona);
    f.mostra(ui.ctx(), "Formato", |ui| {
        etichetta_sezione(ui, "Testo");
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            Passo::new("11", "Più piccolo", "Più grande").mostra(ui);
            GruppoSegmentato::new(&TESTO).mostra(ui);
        });
        etichetta_sezione(ui, "Paragrafo");
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            GruppoSegmentato::new(&ALLINEAMENTI).acceso(Some(0)).mostra(ui);
            GruppoSegmentato::new(&RIENTRI).mostra(ui);
        });
        etichetta_sezione(ui, "Elenchi");
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            let largo = (ui.available_width() - 10.0) / 2.0;
            ui.add(Pulsante::new("Puntato", Peso::Tenue).icona(icone::FORMAT_LIST_BULLETED).acceso(true).larghezza(largo));
            ui.add(Pulsante::new("Numerato", Peso::Tenue).icona(icone::FORMAT_LIST_NUMBERED).acceso(false).larghezza(largo));
        });
    });
}

fn galleria_del_foglio() {
    let mut f = FoglioDalFondo::new(Id::new("formato"));
    f.apri();
    let mut h = telefono(foglio, f);
    h.get_by_label("Chiudi");
    h.get_by_label("Al centro");
    salva(&mut h, "galleria-foglio");
}

fn galleria_del_menu() {
    let mut h = telefono(
        |ui, _: &mut ()| {
            let zona = ui.max_rect();
            barra_del_documento(ui);
            pagina(ui, zona);
            barra_di_formato(ui, zona);
            Menu::new(Id::new("menu"))
                .primo(icone::SAVE, "Salva")
                .voce(icone::FILE_COPY, "Salva una copia")
                .voce(icone::PICTURE_AS_PDF, "Esporta in PDF")
                .voce(icone::SHARE, "Condividi")
                .separatore()
                .voce(icone::FIND_REPLACE, "Trova e sostituisci")
                .voce(icone::TOC, "Indice")
                .voce(icone::ARTICLE, "Vista pagina")
                .separatore()
                .voce(icone::INFO, "Informazioni sul documento")
                .mostra(ui.ctx(), pos2(zona.right() - 8.0, zona.top() + 44.0));
        },
        (),
    );
    h.get_by_label("Vista pagina");
    salva(&mut h, "galleria-menu");
}

fn galleria_del_dialogo() {
    let mut h = telefono(
        |ui, _: &mut ()| {
            let zona = ui.max_rect();
            barra_del_documento(ui);
            pagina(ui, zona);
            barra_di_formato(ui, zona);
            Dialogo::new(Id::new("salvare"), "Salvare le modifiche?")
                .testo("Il documento ha modifiche che non sono state salvate.")
                .pulsante("Annulla", Peso::Testo)
                .pulsante("Non salvare", Peso::Testo)
                .pulsante("Salva", Peso::Principale)
                .mostra(ui.ctx());
        },
        (),
    );
    h.get_by_label("Non salvare");
    salva(&mut h, "galleria-dialogo");
}
