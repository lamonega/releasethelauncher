use crate::{widgets, App, View};
use release_the_launcher_core::ModLoader;
use release_the_launcher_mods::ProjectSummary;

mod manual;
mod modrinth;

use manual::show_manual;
use modrinth::show_modrinth;

pub fn show(app: &mut App, ui: &mut egui::Ui, state: &mut NewInstanceState) {
    process_messages(app, state);

    // Trigger version list fetch on first open if not yet loaded
    if state.version_list_state == VersionListState::Idle {
        state.version_list_state = VersionListState::Loading;
        app.fetch_versions_list();
    }

    if widgets::page_header(ui, app, "New Instance", Some(View::InstanceList)) {
        return;
    }

    let tabs = [
        (InstanceTab::Manual, "Manual"),
        (InstanceTab::Modrinth, "From Modrinth"),
    ];

    if let Some(target_tab) = widgets::tab_row(ui, &app.theme, &state.tab, &tabs) {
        app.log(
            crate::log::LogLevel::Info,
            &format!("UI: Switched to {target_tab:?} tab"),
        );
        state.tab = target_tab;
    }

    ui.add_space(app.theme.spacing.sm);
    ui.separator();
    ui.add_space(app.theme.spacing.sm);

    match state.tab {
        InstanceTab::Manual => show_manual(app, ui, state),
        InstanceTab::Modrinth => show_modrinth(app, ui, state),
    }
}

pub fn process_messages(app: &mut App, state: &mut NewInstanceState) {
    let messages = app.drain_ui_queue();
    for msg in messages {
        match msg {
            crate::UiMessage::ModrinthSearchResult(result) => match result {
                Ok(results) => {
                    state.modrinth_status = format!("Found {} modpacks", results.total_hits);
                    state.modrinth_results = results.hits;
                }
                Err(e) => {
                    state.modrinth_status = format!("Search failed: {e}");
                }
            },
            crate::UiMessage::ModrinthVersionsResult { project_id, result } => match result {
                Ok(versions) => {
                    state.modpack_versions.insert(project_id, versions);
                    state.loading_versions_for_project = None;
                }
                Err(e) => {
                    state.modrinth_status = format!("Failed to load versions: {e}");
                    state.loading_versions_for_project = None;
                }
            },
            crate::UiMessage::ModrinthInstallResult {
                instance_id,
                name,
                mc_version,
                loader,
            } => {
                handle_install_result(app, state, &instance_id, &name, &mc_version, &loader);
            }
            crate::UiMessage::VersionListResult(result) => match result {
                Ok(versions) => {
                    if let Some((latest, _)) = versions
                        .iter()
                        .find(|(_, t)| t == "release")
                        .or_else(|| versions.first())
                    {
                        state.mc_version = latest.clone();
                    }
                    state.available_versions = versions;
                    state.version_list_state = VersionListState::Loaded;
                }
                Err(e) => {
                    state.modrinth_status = format!("Failed to load versions: {e}");
                }
            },
            crate::UiMessage::LoaderVersionsResult {
                loader_type,
                mc_version,
                result,
            } if state.loader_type.as_str() == loader_type && state.mc_version == mc_version => {
                match result {
                    Ok(versions) => {
                        state.loader_versions = versions;
                        state.loader_versions_loading = false;
                        state.loader_versions_error = None;
                        if !state.loader_versions.is_empty()
                            && (state.loader_version.is_empty()
                                || !state.loader_versions.contains(&state.loader_version))
                        {
                            state.loader_version = state.loader_versions[0].clone();
                        }
                    }
                    Err(e) => {
                        state.loader_versions_loading = false;
                        state.loader_versions_error = Some(e);
                    }
                }
            }
            _ => {}
        }
    }
}

pub fn handle_install_result(
    app: &mut App,
    state: &mut NewInstanceState,
    instance_id: &str,
    name: &str,
    mc_version: &str,
    loader_raw: &str,
) {
    if instance_id.is_empty() {
        return;
    }
    let loader = if loader_raw.starts_with("Fabric") {
        ModLoader::Fabric {
            loader_version: loader_raw.strip_prefix("Fabric:").unwrap_or("").to_string(),
        }
    } else if loader_raw.starts_with("Forge") {
        ModLoader::Forge {
            loader_version: loader_raw.strip_prefix("Forge:").unwrap_or("").to_string(),
        }
    } else if loader_raw.starts_with("NeoForge") {
        ModLoader::NeoForge {
            loader_version: loader_raw
                .strip_prefix("NeoForge:")
                .unwrap_or("")
                .to_string(),
        }
    } else if loader_raw.starts_with("Quilt") {
        ModLoader::Quilt {
            loader_version: loader_raw.strip_prefix("Quilt:").unwrap_or("").to_string(),
        }
    } else {
        ModLoader::Vanilla
    };
    let settings = release_the_launcher_core::InstanceSettings::new(
        name.to_string(),
        mc_version.to_string(),
        loader,
    );
    let _ = app
        .coordinator
        .instance_manager
        .create(instance_id, settings);
    app.status_message = format!("Installed modpack instance: {name}");
    app.current_view = View::InstanceList;
    state.installing_modpack_id = None;
}

pub struct NewInstanceState {
    pub name: String,
    pub mc_version: String,
    pub loader_type: LoaderType,
    pub loader_version: String,
    pub tab: InstanceTab,
    pub modrinth_query: String,
    pub mc_version_filter: String,
    pub loader_filter: LoaderFilter,
    pub modrinth_status: String,
    pub modrinth_results: Vec<ProjectSummary>,
    pub installing_modpack_id: Option<String>,
    pub available_versions: Vec<(String, String)>,
    pub version_list_state: VersionListState,
    pub modpack_versions:
        std::collections::HashMap<String, Vec<release_the_launcher_mods::ModVersion>>,
    pub loading_versions_for_project: Option<String>,
    pub expanded_project_id: Option<String>,
    pub filter_types: [bool; 3],
    pub version_search_query: String,
    pub manual_show_types: [bool; 4],
    pub loader_versions: Vec<String>,
    pub loader_versions_loading: bool,
    pub loader_versions_error: Option<String>,
    pub last_fetched_loader_key: Option<(LoaderType, String)>,
}

impl Default for NewInstanceState {
    fn default() -> Self {
        Self {
            name: String::new(),
            mc_version: String::new(),
            loader_type: LoaderType::default(),
            loader_version: String::new(),
            tab: InstanceTab::default(),
            modrinth_query: String::new(),
            mc_version_filter: String::new(),
            loader_filter: LoaderFilter::default(),
            modrinth_status: String::new(),
            modrinth_results: Vec::new(),
            installing_modpack_id: None,
            available_versions: Vec::new(),
            version_list_state: VersionListState::Idle,
            modpack_versions: std::collections::HashMap::new(),
            loading_versions_for_project: None,
            expanded_project_id: None,
            filter_types: [true, false, false],
            version_search_query: String::new(),
            manual_show_types: [true, false, false, false],
            loader_versions: Vec::new(),
            loader_versions_loading: false,
            loader_versions_error: None,
            last_fetched_loader_key: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionListState {
    Idle,
    Loading,
    Loaded,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum InstanceTab {
    #[default]
    Manual,
    Modrinth,
}

pub type NewInstanceTab = InstanceTab;

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub enum LoaderType {
    #[default]
    Vanilla,
    Fabric,
    Forge,
    NeoForge,
    Quilt,
}

impl LoaderType {
    #[must_use]
    pub const fn as_str(&self) -> &str {
        match self {
            Self::Vanilla => "Vanilla",
            Self::Fabric => "Fabric",
            Self::Forge => "Forge",
            Self::NeoForge => "NeoForge",
            Self::Quilt => "Quilt",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum LoaderFilter {
    #[default]
    Any,
    Fabric,
    Forge,
    NeoForge,
    Quilt,
}

impl LoaderFilter {
    #[must_use]
    pub const fn as_str(&self) -> &str {
        match self {
            Self::Any => "Any",
            Self::Fabric => "Fabric",
            Self::Forge => "Forge",
            Self::NeoForge => "NeoForge",
            Self::Quilt => "Quilt",
        }
    }
}
