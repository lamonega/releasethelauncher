use crate::LauncherApp;

pub fn show_info(
    app: &LauncherApp,
    ui: &mut egui::Ui,
    root_display: &str,
    loader_name: &str,
    mc_version: &str,
    java_settings: &release_the_launcher_core::JavaSettings,
) {
    ui.colored_label(app.theme.text_secondary, "Instance folder:");
    ui.monospace(root_display);

    ui.add_space(app.theme.spacing.xs);
    ui.colored_label(app.theme.text_secondary, format!("Loader: {loader_name}"));
    ui.colored_label(app.theme.text_secondary, format!("Minecraft: {mc_version}"));

    let gs = app.coordinator.settings();

    let java_display = match &java_settings.path {
        Some(p) if !p.trim().is_empty() => format!("{p} (Custom)"),
        _ => {
            let global_p = gs.java.path.as_deref().unwrap_or("System Default");
            format!("{global_p} (Global Default)")
        }
    };
    ui.horizontal(|ui| {
        ui.colored_label(app.theme.text_secondary, "Java Path:");
        ui.monospace(java_display);
    });

    let min_display = match &java_settings.memory_min {
        Some(m) if !m.trim().is_empty() => format!("{m} (Custom)"),
        _ => {
            let global_m = gs.java.memory_min.as_deref().unwrap_or("1G");
            format!("{global_m} (Global Default)")
        }
    };
    ui.horizontal(|ui| {
        ui.colored_label(app.theme.text_secondary, "Min Memory:");
        ui.label(min_display);
    });

    let max_display = match &java_settings.memory_max {
        Some(m) if !m.trim().is_empty() => format!("{m} (Custom)"),
        _ => {
            let global_m = gs.java.memory_max.as_deref().unwrap_or("2G");
            format!("{global_m} (Global Default)")
        }
    };
    ui.horizontal(|ui| {
        ui.colored_label(app.theme.text_secondary, "Max Memory:");
        ui.label(max_display);
    });
}
