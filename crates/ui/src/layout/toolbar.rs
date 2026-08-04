use crate::theme;
use crate::{widgets, LogLevel, View};

pub fn show(app: &mut crate::LauncherApp, ctx: &egui::Context) {
    egui::TopBottomPanel::top("toolbar")
        .frame(
            egui::Frame::none()
                .fill(app.theme.surface_alt)
                .inner_margin(egui::Margin::symmetric(10.0, 6.0)),
        )
        .show(ctx, |ui| {
            let panel_rect = ui.max_rect();

            ui.horizontal(|ui| {
                ui.heading("Release The Launcher");

                widgets::right_aligned(ui, |ui| {
                    let btn_size = egui::vec2(28.0, 24.0);

                    // Close Button "X"
                    let close_btn = egui::Button::new(egui::RichText::new("X").strong().size(14.0))
                        .min_size(btn_size);
                    if ui.add(close_btn).clicked() {
                        app.coordinator
                            .log(LogLevel::Info, "UI: Close button clicked");
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }

                    // Maximize / Restore Button
                    let max_text = if app.maximized { "❐" } else { "□" };
                    let max_btn =
                        egui::Button::new(egui::RichText::new(max_text).strong().size(14.0))
                            .min_size(btn_size);
                    if ui.add(max_btn).clicked() {
                        app.maximized = !app.maximized;
                        let action = if app.maximized {
                            "maximized"
                        } else {
                            "restored"
                        };
                        app.coordinator
                            .log(LogLevel::Info, &format!("UI: Window {action}"));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(app.maximized));
                    }

                    // Minimize Button
                    let min_btn = egui::Button::new(egui::RichText::new("—").strong().size(14.0))
                        .min_size(btn_size);
                    if ui.add(min_btn).clicked() {
                        app.coordinator.log(LogLevel::Info, "UI: Window minimized");
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                    }

                    ui.separator();

                    if ui.button("Accounts").clicked() {
                        app.coordinator
                            .log(LogLevel::Info, "UI: Navigated to Accounts");
                        app.current_view = View::AccountList;
                    }
                    if ui
                        .add(widgets::icon_button(theme::icons::SETTINGS, "Settings"))
                        .clicked()
                    {
                        app.coordinator
                            .log(LogLevel::Info, "UI: Navigated to Settings");
                        app.current_view = View::Settings;
                    }

                    // Calculate left edge of the right-side button group
                    let right_buttons_left_x = ui.min_rect().min.x;

                    // Drag region covers everything from left edge to the start of the right buttons
                    let drag_rect = egui::Rect::from_min_max(
                        panel_rect.min,
                        egui::pos2(right_buttons_left_x - 8.0, panel_rect.max.y),
                    );

                    let title_drag = ui.interact(
                        drag_rect,
                        egui::Id::new("title_drag_area"),
                        egui::Sense::drag(),
                    );
                    if title_drag.drag_started_by(egui::PointerButton::Primary) {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                });
            });
        });
}
