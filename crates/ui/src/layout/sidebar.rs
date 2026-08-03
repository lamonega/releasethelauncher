use crate::theme;
use crate::views::instance_detail::DetailTabState;
use crate::views::new_instance::NewInstanceState;
use crate::{empty_state, widgets, App, DetailTab, LogLevel, View};

pub fn show(
    app: &mut App,
    selected_instance_id: &mut Option<String>,
    new_instance_state: &mut NewInstanceState,
    detail_tab_state: &mut DetailTabState,
    ctx: &egui::Context,
) {
    egui::SidePanel::left("sidebar")
        .default_width(200.0)
        .show(ctx, |ui| {
            ui.add_space(app.theme.spacing.sm);
            ui.horizontal(|ui| {
                ui.heading("Instances");
                widgets::right_aligned(ui, |ui| {
                    if ui
                        .add(widgets::icon_button(theme::icons::ADD, "New").fill(app.theme.accent))
                        .clicked()
                    {
                        app.log(LogLevel::Info, "UI: Navigated to New Instance");
                        *new_instance_state = NewInstanceState::default();
                        app.current_view = View::NewInstance;
                        *selected_instance_id = None;
                    }
                });
            });
            ui.separator();

            let instances = app.instance_ids();

            if instances.is_empty() {
                empty_state(
                    ui,
                    &app.theme,
                    &["No instances.", "Create one to get started."],
                );
            } else {
                for id in &instances {
                    if let Some(instance) = app.coordinator.instance_manager().get(id) {
                        let is_selected = selected_instance_id.as_deref() == Some(id.as_str());

                        let bg_color = if is_selected {
                            app.theme.surface_alt
                        } else {
                            egui::Color32::TRANSPARENT
                        };

                        let name = &instance.settings.name;
                        let mc_ver = &instance.settings.minecraft_version;
                        let loader = instance.settings.loader_name();
                        let text = format!("{name}\n{mc_ver} ({loader})");

                        let btn = egui::Button::new(text)
                            .fill(bg_color)
                            .min_size(egui::vec2(ui.available_width(), 40.0));

                        if ui.add(btn).clicked() {
                            app.log(LogLevel::Info, &format!("UI: Selected instance '{id}'"));
                            *selected_instance_id = Some(id.clone());
                            app.current_view = View::InstanceDetail {
                                id: id.clone(),
                                tab: DetailTab::Info,
                            };
                            *detail_tab_state = DetailTabState::default();
                        }
                    }
                }
            }
        });
}
