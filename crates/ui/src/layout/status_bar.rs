use crate::{App, DownloadPhase};

#[must_use]
pub fn progress_ratio(completed: u64, total: u64) -> f32 {
    if total == 0 {
        return 0.0;
    }
    let permil = completed.saturating_mul(1000) / total;
    let permil_u16 = u16::try_from(permil).unwrap_or(u16::MAX);
    f32::from(permil_u16) / 1000.0
}

pub fn show(ctx: &egui::Context, app: &App) {
    egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
        ui.style_mut().visuals.panel_fill = app.theme.surface_alt;
        ui.add_space(app.theme.spacing.xs);
        ui.horizontal(|ui| {
            match &app.download_state.phase {
                DownloadPhase::Idle => {}
                DownloadPhase::Resolving => {
                    ui.spinner();
                    ui.label("Resolving dependencies...");
                }
                DownloadPhase::Downloading { message } => {
                    ui.spinner();
                    ui.label(message.as_str());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(pct) = app.download_state.completed.saturating_mul(100).checked_div(app.download_state.total) {
                            let progress = f32::from(u16::try_from(pct).unwrap_or(u16::MAX)) / 100.0;
                            ui.add(
                                egui::ProgressBar::new(progress)
                                    .text(format!("{pct}%"))
                                    .desired_width(140.0),
                            );
                        }
                    });
                }
            }
            if !app.status_message.is_empty() && app.download_state.phase == DownloadPhase::Idle {
                ui.label(app.status_message.clone());
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_ratio() {
        assert_eq!(progress_ratio(0, 100), 0.0);
        assert_eq!(progress_ratio(50, 100), 0.5);
        assert_eq!(progress_ratio(100, 100), 1.0);
        assert_eq!(progress_ratio(0, 0), 0.0);
    }
}
