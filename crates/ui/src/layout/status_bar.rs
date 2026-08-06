use crate::{widgets, DownloadPhase};

#[must_use]
pub fn progress_ratio(completed: u64, total: u64) -> f32 {
    if total == 0 {
        return 0.0;
    }
    let scaled = completed.saturating_mul(10_000) / total;
    let capped = u16::try_from(scaled.min(10_000)).unwrap_or(10_000);
    f32::from(capped) / 10_000.0
}

pub fn show(app: &crate::LauncherApp, ctx: &egui::Context) {
    egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
        ui.style_mut().visuals.panel_fill = app.theme.surface_alt;
        ui.add_space(app.theme.spacing.xs);
        ui.horizontal(|ui| {
            match &app.download_state.phase {
                DownloadPhase::Idle => {}
                DownloadPhase::Resolving => {
                    widgets::loading_row(ui, "Resolving dependencies...");
                }
                DownloadPhase::Downloading { message } => {
                    widgets::loading_row(ui, message.as_str());
                    widgets::right_aligned(ui, |ui| {
                        let raw_pct = app
                            .download_state
                            .completed
                            .checked_mul(100)
                            .and_then(|v| v.checked_div(app.download_state.total))
                            .unwrap_or(0);
                        let pct = u32::try_from(raw_pct).unwrap_or(100);
                        ui.add(
                            egui::ProgressBar::new(progress_ratio(
                                app.download_state.completed,
                                app.download_state.total,
                            ))
                            .text(format!("{pct}%"))
                            .desired_width(140.0),
                        );
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
        let close = |a: f32, b: f32| (a - b).abs() < 1e-6;
        assert!(close(progress_ratio(0, 100), 0.0));
        assert!(close(progress_ratio(50, 100), 0.5));
        assert!(close(progress_ratio(100, 100), 1.0));
        assert!(close(progress_ratio(0, 0), 0.0));
    }
}
