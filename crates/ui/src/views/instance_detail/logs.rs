use super::DetailTabState;
use crate::App;

pub fn show_logs(
    app: &mut App,
    ui: &mut egui::Ui,
    instance_id: &str,
    root_path: &std::path::Path,
    tab_state: &mut DetailTabState,
) {
    let cache = &mut tab_state.log_cache;

    // Determine target log file path if not already resolved or changed
    if cache.file_path.is_none() {
        let mc_log_path = root_path.join(".minecraft").join("logs").join("latest.log");
        let alt_log_path = root_path.join("logs").join("latest.log");
        if mc_log_path.exists() {
            cache.file_path = Some(mc_log_path);
        } else if alt_log_path.exists() {
            cache.file_path = Some(alt_log_path);
        }
    }

    // Check disk file mtime / len to reload only when file on disk changes
    if let Some(ref path) = cache.file_path {
        let metadata = std::fs::metadata(path).ok();
        let current_mtime = metadata.as_ref().and_then(|m| m.modified().ok());
        let current_len = metadata.as_ref().map_or(0, |m| m.len());

        if cache.last_mtime != current_mtime || cache.file_len != current_len {
            if let Ok(content) = std::fs::read_to_string(path) {
                cache.disk_lines = content.lines().map(String::from).collect();
                cache.last_mtime = current_mtime;
                cache.file_len = current_len;
            }
        }
    }

    let target_key = format!("instance:{instance_id}");
    let buffer_entries: Vec<_> = app
        .coordinator
        .log_buffer()
        .entries()
        .into_iter()
        .filter(|e| e.target == target_key || e.target == instance_id)
        .collect();

    let has_disk_logs = !cache.disk_lines.is_empty();
    let has_buffer_logs = !buffer_entries.is_empty();

    show_logs_header(
        app,
        ui,
        instance_id,
        &buffer_entries,
        &cache.disk_lines,
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

    show_logs_content(app, ui, &buffer_entries, &cache.disk_lines);
}

fn show_logs_header(
    app: &mut App,
    ui: &mut egui::Ui,
    instance_id: &str,
    buffer_entries: &[crate::log::LogEntry],
    disk_lines: &[String],
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
                for line in disk_lines {
                    full_logs.push_str(line);
                    full_logs.push('\n');
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
    disk_lines: &[String],
) {
    let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
    let total_rows = buffer_entries.len() + disk_lines.len();

    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show_rows(ui, row_height, total_rows, |ui, row_range| {
            ui.style_mut().override_text_style = Some(egui::TextStyle::Monospace);

            let buffer_len = buffer_entries.len();
            for row in row_range {
                if row < buffer_len {
                    let entry = &buffer_entries[row];
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
                } else {
                    let line = &disk_lines[row - buffer_len];
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
