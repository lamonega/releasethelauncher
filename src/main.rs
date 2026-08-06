#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::module_name_repetitions,
    clippy::struct_excessive_bools,
    clippy::too_many_lines,
    clippy::doc_markdown,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::similar_names,
    clippy::unused_async,
    clippy::redundant_closure_for_method_calls,
    clippy::map_unwrap_or,
    clippy::new_without_default,
    clippy::double_must_use,
    clippy::manual_let_else,
    clippy::single_match_else
)]

use release_the_launcher_coordinator::Coordinator;
use release_the_launcher_ui::theme::Theme;
use release_the_launcher_ui::LauncherApp;

fn main() -> Result<(), eframe::Error> {
    tracing_subscriber::fmt()
        .with_ansi(false)
        .without_time()
        .with_target(true)
        .init();

    let coordinator = Coordinator::new();

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

            Box::new(LauncherApp::new(coordinator, theme))
        }),
    )
}
