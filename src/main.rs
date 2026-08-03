#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use release_the_launcher_coordinator::Coordinator;
use release_the_launcher_ui::theme::Theme;
use release_the_launcher_ui::{App, LauncherApp};

fn main() -> Result<(), eframe::Error> {
    tracing_subscriber::fmt()
        .with_ansi(false)
        .without_time()
        .with_target(true)
        .init();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to create tokio runtime");
    let _guard = rt.enter();

    // Initialize coordinator (does I/O) before entering the egui render loop.
    let mut coordinator = Coordinator::new();
    coordinator.attach_runtime(tokio::runtime::Handle::current());

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_inner_size([1000.0, 700.0])
            .with_min_inner_size([800.0, 500.0])
            .with_title("Release The Launcher"),
        ..Default::default()
    };

    eframe::run_native(
        "Release The Launcher",
        options,
        Box::new(move |cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            let theme = Theme::apply(&cc.egui_ctx);
            let app = App::new(coordinator, theme, Some(cc.egui_ctx.clone()));
            Box::new(LauncherApp::new(app))
        }),
    )
}
