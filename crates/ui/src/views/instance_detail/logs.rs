use crate::App;

pub fn show_logs(app: &mut App, ui: &mut egui::Ui, instance_id: &str, root_path: &std::path::Path) {
    let mc_log_path = root_path.join(".minecraft").join("logs").join("latest.log");
    let alt_log_path = root_path.join("logs").join("latest.log");

    let log_file_path = if mc_log_path.exists() {
        Some(mc_log_path)
    } else if alt_log_path.exists() {
        Some(alt_log_path)
    } else {
        None
    };

    let target_key = format!("instance:{instance_id}");
    let buffer_entries: Vec<_> = app
        .coordinator
        .log_buffer()
        .entries()
        .into_iter()
        .filter(|e| e.target == target_key || e.target == instance_id)
        .collect();

    let disk_content = log_file_path.and_then(|p| std::fs::read_to_string(p).ok());
    let has_disk_logs = disk_content.as_ref().is_some_and(|c| !c.trim().is_empty());
    let has_buffer_logs = !buffer_entries.is_empty();

    show_logs_header(
        app,
        ui,
        instance_id,
        &buffer_entries,
        disk_content.as_ref(),
        has_disk_logs,
        has_buffer_logs,
    );

    ui.add_space(app.theme.spacing.xs);

    if !has_disk_logs && !has_buffer_logs {
        ui.colored_label(
            app.theme.text_secondary,
            "No log entries yet for this instance.",
        );
        return;
    }

    show_logs_content(app, ui, &buffer_entries, disk_content.as_ref());
}

fn show_logs_header(
    app: &mut App,
    ui: &mut egui::Ui,
    instance_id: &str,
    buffer_entries: &[crate::log::LogEntry],
    disk_content: Option<&String>,
    has_disk_logs: bool,
    has_buffer_logs: bool,
) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Instance logs:").strong());
        crate::widgets::right_aligned(ui, |ui| {
            if (has_disk_logs || has_buffer_logs)
                && ui
                    .add(crate::widgets::icon_button(crate::icons::COPY, "Copy Logs"))
                    .clicked()
            {
                let mut full_logs = String::new();
                for entry in buffer_entries {
                    use std::fmt::Write;
                    let _ = writeln!(
                        full_logs,
                        "[{}] [{}] {}",
                        entry.timestamp,
                        entry.level.as_str(),
                        entry.message
                    );
                }
                if let Some(content) = disk_content {
                    full_logs.push_str(content);
                }
                ui.output_mut(|o| o.copied_text = full_logs);
                app.status_message = "Logs copied to clipboard!".to_string();
                app.log(
                    crate::log::LogLevel::Info,
                    &format!("UI: Copied logs for instance '{instance_id}' to clipboard"),
                );
            }
        });
    });
}

fn show_logs_content(
    app: &App,
    ui: &mut egui::Ui,
    buffer_entries: &[crate::log::LogEntry],
    disk_content: Option<&String>,
) {
    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            ui.style_mut().override_text_style = Some(egui::TextStyle::Monospace);

            if !buffer_entries.is_empty() {
                for entry in buffer_entries {
                    let color = match entry.level {
                        crate::log::LogLevel::Error => app.theme.log_colors.error,
                        crate::log::LogLevel::Warn => app.theme.log_colors.warn,
                        crate::log::LogLevel::Info => app.theme.log_colors.info,
                        crate::log::LogLevel::Debug => app.theme.log_colors.debug,
                        crate::log::LogLevel::Trace => app.theme.log_colors.trace,
                    };
                    let text = format!(
                        "[{}] [{}] {}",
                        entry.timestamp,
                        entry.level.as_str(),
                        entry.message
                    );
                    ui.colored_label(color, text);
                }
            }

            if let Some(content) = disk_content {
                for line in content.lines() {
                    let color = if line.contains("/ERROR")
                        || line.contains("ERROR")
                        || line.contains("FATAL")
                    {
                        app.theme.log_colors.error
                    } else if line.contains("/WARN")
                        || line.contains("WARN")
                        || line.contains("WARNING")
                    {
                        app.theme.log_colors.warn
                    } else if line.contains("/INFO") || line.contains("INFO") {
                        app.theme.log_colors.info
                    } else if line.contains("/DEBUG") || line.contains("DEBUG") {
                        app.theme.log_colors.debug
                    } else {
                        app.theme.text_secondary
                    };
                    ui.colored_label(color, line);
                }
            }
        });
}
