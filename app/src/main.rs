//! Binario desktop: la stessa app in una finestra grande come un telefono.
//! Con un percorso come argomento lo apre come farebbe «Apri con».

#[cfg(not(target_os = "android"))]
fn main() -> eframe::Result {
    let avvio = std::time::Instant::now();
    // Prima riga del registro: i suoi tempi partono da qui. Resta in testa al registro.
    tascabile_platform::diagnosi_fissa(tascabile_platform::Fissa::Avvio, format!("avvio: Tascabile v{} su PC", tascabile::VERSIONE));
    env_logger::init();
    // Come sul telefono: un panico con file e riga soltanto, mai il messaggio.
    tascabile_platform::installa_gancio_dei_panici();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([412.0, 915.0]).with_title("Doc Tascabile · telefono simulato"),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Tascabile",
        options,
        Box::new(move |cc| {
            // Il tema prima del primo frame: caratteri, colori chiari, misure.
            tascabile::prepara(&cc.egui_ctx);
            let piattaforma = tascabile::Piattaforma::default();
            if let Some(percorso) = std::env::args().nth(1) {
                piattaforma.simula_apri_con(&percorso, None);
            }
            Ok(Box::new(tascabile::TascabileApp::new(piattaforma, avvio)))
        }),
    )
}

#[cfg(target_os = "android")]
fn main() {}
