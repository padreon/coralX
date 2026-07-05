use coralx::core;
use coralx::ui::app::CoralXApp;

fn main() -> eframe::Result {
    core::logger::setup_logging();
    core::logger::install_panic_hook();
    log::info!("coralX starting");

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 600.0])
            .with_min_inner_size([600.0, 380.0])
            .with_title("coralX"),
        ..Default::default()
    };

    eframe::run_native(
        "coralX",
        native_options,
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(CoralXApp::default()))
        }),
    )
}
