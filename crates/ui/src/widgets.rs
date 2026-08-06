use crate::theme::Theme;
use crate::{LauncherApp, View};

/// Renders a standardized page header with a title and optional back navigation button.
/// Returns `true` if the back button was clicked.
pub fn page_header(
    ui: &mut egui::Ui,
    app: &mut LauncherApp,
    title: &str,
    back_to: Option<View>,
) -> bool {
    let mut back_clicked = false;
    ui.horizontal(|ui| {
        ui.heading(title);
        if let Some(view) = back_to {
            right_aligned(ui, |ui| {
                if ui.add(icon_button(crate::icons::BACK, "Back")).clicked() {
                    app.coordinator.log(
                        crate::log::LogLevel::Info,
                        &format!("UI: Navigated back from {title}"),
                    );
                    app.current_view = view;
                    back_clicked = true;
                }
            });
        }
    });
    ui.add_space(app.theme.spacing.sm);
    back_clicked
}

/// Helper to render content aligned to the right side of a horizontal container.
pub fn right_aligned<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add)
        .inner
}

/// Creates a standard icon button with consistent spacing between icon and label text.
pub fn icon_button(icon: &str, label: &str) -> egui::Button<'static> {
    let text = format!("{icon} {label}");
    egui::Button::new(text)
}

/// Renders a horizontal row of selectable tab buttons. Returns `Some(selected_value)` if a tab was clicked.
pub fn tab_row<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    theme: &Theme,
    current: &T,
    tabs: &[(T, &str)],
) -> Option<T> {
    let mut selected = None;
    ui.horizontal(|ui| {
        for (value, label) in tabs {
            let button = if current == value {
                egui::Button::new(*label).fill(theme.accent)
            } else {
                egui::Button::new(*label)
            };
            if ui.add(button).clicked() {
                selected = Some(*value);
            }
        }
    });
    selected
}

/// Renders a loading spinner with text on a single line.
pub fn loading_row(ui: &mut egui::Ui, text: &str) {
    ui.horizontal(|ui| {
        ui.spinner();
        ui.label(text);
    });
}

/// Renders a single-line search input with a search button. Returns `true` if the button was clicked.
pub fn search_bar(ui: &mut egui::Ui, query: &mut String) -> bool {
    let mut searched = false;
    ui.horizontal(|ui| {
        ui.label("Search:");
        ui.text_edit_singleline(query);
        if ui
            .add(icon_button(crate::icons::SEARCH, "Search"))
            .clicked()
        {
            searched = true;
        }
    });
    searched
}

/// Renders a settings label and single-line text input field. Returns `true` if the text changed.
pub fn settings_field(ui: &mut egui::Ui, label: &str, value: &mut String) -> bool {
    ui.label(label);
    ui.text_edit_singleline(value).changed()
}
