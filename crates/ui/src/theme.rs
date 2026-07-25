use egui::{Color32, FontFamily, FontId, Stroke, TextStyle, Vec2};

#[derive(Clone, Debug)]
pub struct Theme {
    pub background: Color32,
    pub surface: Color32,
    pub surface_alt: Color32,
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub accent: Color32,
    pub accent_text: Color32,
    pub border: Color32,
    pub log_colors: LogColors,
    pub spacing: Spacing,
}

#[derive(Clone, Debug)]
pub struct LogColors {
    pub error: Color32,
    pub warn: Color32,
    pub info: Color32,
    pub debug: Color32,
    pub trace: Color32,
}

#[derive(Clone, Debug)]
pub struct Spacing {
    pub xs: f32,
    pub sm: f32,
    pub md: f32,
    pub lg: f32,
}

impl Default for Spacing {
    fn default() -> Self {
        Self {
            xs: 4.0,
            sm: 8.0,
            md: 16.0,
            lg: 24.0,
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            background: Color32::from_rgb(20, 20, 24),
            surface: Color32::from_rgb(30, 30, 36),
            surface_alt: Color32::from_rgb(40, 40, 48),
            text_primary: Color32::from_rgb(200, 200, 204),
            text_secondary: Color32::from_rgb(120, 120, 128),
            accent: Color32::from_rgb(70, 110, 140),
            accent_text: Color32::from_rgb(220, 220, 224),
            border: Color32::from_rgb(50, 50, 56),
            log_colors: LogColors {
                error: Color32::from_rgb(180, 100, 100),
                warn: Color32::from_rgb(180, 160, 100),
                info: Color32::from_rgb(190, 190, 194),
                debug: Color32::from_rgb(130, 130, 136),
                trace: Color32::from_rgb(80, 80, 88),
            },
            spacing: Spacing::default(),
        }
    }
}

impl Theme {
    #[must_use]
    pub fn apply(ctx: &egui::Context) -> Self {
        let theme = Self::default();

        let visuals = egui::Visuals {
            dark_mode: true,
            window_fill: theme.background,
            panel_fill: theme.surface,
            faint_bg_color: theme.surface_alt,
            extreme_bg_color: theme.surface_alt,
            code_bg_color: theme.surface_alt,
            warn_fg_color: theme.log_colors.warn,
            error_fg_color: theme.log_colors.error,
            hyperlink_color: theme.accent,
            selection: egui::style::Selection {
                bg_fill: theme.accent,
                stroke: Stroke::new(1.0_f32, theme.border),
            },
            widgets: egui::style::Widgets {
                noninteractive: egui::style::WidgetVisuals {
                    bg_fill: theme.surface,
                    weak_bg_fill: theme.border,
                    bg_stroke: Stroke::new(1.0_f32, theme.border),
                    rounding: egui::Rounding::same(4.0_f32),
                    fg_stroke: Stroke::new(1.0_f32, theme.text_secondary),
                    expansion: 0.0,
                },
                inactive: egui::style::WidgetVisuals {
                    bg_fill: theme.surface,
                    weak_bg_fill: Color32::TRANSPARENT,
                    bg_stroke: Stroke::new(1.0_f32, theme.border),
                    rounding: egui::Rounding::same(4.0_f32),
                    fg_stroke: Stroke::new(1.0_f32, theme.text_primary),
                    expansion: 0.0,
                },
                hovered: egui::style::WidgetVisuals {
                    bg_fill: theme.surface_alt,
                    weak_bg_fill: Color32::TRANSPARENT,
                    bg_stroke: Stroke::new(1.0_f32, theme.accent),
                    rounding: egui::Rounding::same(4.0_f32),
                    fg_stroke: Stroke::new(1.0_f32, theme.text_primary),
                    expansion: 0.0,
                },
                active: egui::style::WidgetVisuals {
                    bg_fill: theme.accent,
                    weak_bg_fill: Color32::TRANSPARENT,
                    bg_stroke: Stroke::new(1.0_f32, theme.accent),
                    rounding: egui::Rounding::same(4.0_f32),
                    fg_stroke: Stroke::new(1.0_f32, theme.accent_text),
                    expansion: 0.0,
                },
                open: egui::style::WidgetVisuals {
                    bg_fill: theme.surface_alt,
                    weak_bg_fill: Color32::TRANSPARENT,
                    bg_stroke: Stroke::new(1.0_f32, theme.border),
                    rounding: egui::Rounding::same(4.0_f32),
                    fg_stroke: Stroke::new(1.0_f32, theme.text_primary),
                    expansion: 0.0,
                },
            },
            ..Default::default()
        };

        let style = egui::Style {
            spacing: egui::style::Spacing {
                item_spacing: Vec2::new(theme.spacing.sm, theme.spacing.sm),
                button_padding: Vec2::new(theme.spacing.sm, theme.spacing.xs),
                indent: theme.spacing.md,
                menu_margin: egui::Margin::symmetric(theme.spacing.sm, theme.spacing.xs),
                combo_height: 20.0,
                ..Default::default()
            },
            text_styles: [
                (
                    TextStyle::Heading,
                    FontId::new(18.0, FontFamily::Proportional),
                ),
                (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
                (
                    TextStyle::Monospace,
                    FontId::new(13.0, FontFamily::Monospace),
                ),
                (
                    TextStyle::Button,
                    FontId::new(14.0, FontFamily::Proportional),
                ),
                (
                    TextStyle::Small,
                    FontId::new(11.0, FontFamily::Proportional),
                ),
            ]
            .into(),
            ..Default::default()
        };

        ctx.set_visuals(visuals);
        ctx.set_style(style);

        let mut fonts = egui::FontDefinitions::default();
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::variants::Variant::Regular);
        ctx.set_fonts(fonts);

        theme
    }
}

pub mod icons {
    pub const LAUNCH: &str = egui_phosphor::regular::PLAY;
    pub const DELETE: &str = egui_phosphor::regular::TRASH;
    pub const BACK: &str = egui_phosphor::regular::ARROW_LEFT;
    pub const FOLDER: &str = egui_phosphor::regular::FOLDER;
    pub const ADD: &str = egui_phosphor::regular::PLUS;
    pub const SEARCH: &str = egui_phosphor::regular::MAGNIFYING_GLASS;
}
