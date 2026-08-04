use crate::{widgets, LauncherApp, View};
use release_the_launcher_core::ModLoader;
use release_the_launcher_mods::ProjectInfo;

mod manual;
mod modrinth;

use manual::show_manual;
use modrinth::show_modrinth;

pub fn show(app: &mut LauncherApp, ui: &mut egui::Ui) {
    if app.new_instance_state.version_list_state == VersionListState::Idle {
        app.new_instance_state.version_list_state = VersionListState::Loading;
        app.coordinator.fetch_versions_list();
    }

    if widgets::page_header(ui, app, "New Instance", Some(View::InstanceList)) {
        return;
    }

    let tabs = [
        (InstanceTab::Manual, "Manual"),
        (InstanceTab::Modrinth, "From Modrinth"),
    ];

    if let Some(target_tab) = widgets::tab_row(ui, &app.theme, &app.new_instance_state.tab, &tabs) {
        app.coordinator.log(
            crate::log::LogLevel::Info,
            &format!("UI: Switched to {target_tab:?} tab"),
        );
        app.new_instance_state.tab = target_tab;
    }

    ui.add_space(app.theme.spacing.sm);
    ui.separator();
    ui.add_space(app.theme.spacing.sm);

    let mut state = std::mem::take(&mut app.new_instance_state);
    match state.tab {
        InstanceTab::Manual => show_manual(app, ui, &mut state),
        InstanceTab::Modrinth => show_modrinth(app, ui, &mut state),
    }
    app.new_instance_state = state;
}

pub fn process_message(app: &mut crate::LauncherApp, msg: crate::UiMessage) {
    match msg {
        crate::UiMessage::ModrinthSearchResult(result) => match result {
            Ok(results) => {
                app.new_instance_state.modrinth_status =
                    format!("Found {} modpacks", results.total_hits);
                app.new_instance_state.modrinth_results = results.hits;
            }
            Err(e) => {
                app.new_instance_state.modrinth_status = format!("Search failed: {e}");
            }
        },
        crate::UiMessage::ModrinthVersionsResult { project_id, result } => match result {
            Ok(versions) => {
                app.new_instance_state
                    .modpack_versions
                    .insert(project_id, versions);
                app.new_instance_state.loading_versions_for_project = None;
            }
            Err(e) => {
                app.new_instance_state.modrinth_status = format!("Failed to load versions: {e}");
                app.new_instance_state.loading_versions_for_project = None;
            }
        },
        crate::UiMessage::ModrinthInstallResult {
            instance_id,
            name,
            mc_version,
            loader,
            modpack_project_id,
            modpack_version_id,
        } => {
            let modpack_ids = modpack_project_id
                .as_deref()
                .zip(modpack_version_id.as_deref());
            handle_install_result(app, &instance_id, &name, &mc_version, &loader, modpack_ids);
        }
        crate::UiMessage::VersionListResult(result) => match result {
            Ok(versions) => {
                if let Some((latest, _)) = versions
                    .iter()
                    .find(|(_, t)| t == "release")
                    .or_else(|| versions.first())
                {
                    app.new_instance_state.mc_version = latest.clone();
                }
                app.new_instance_state.available_versions = versions;
                app.new_instance_state.version_list_state = VersionListState::Loaded;
            }
            Err(e) => {
                app.new_instance_state.modrinth_status = format!("Failed to load versions: {e}");
            }
        },
        crate::UiMessage::LoaderVersionsResult {
            loader_type,
            mc_version,
            result,
        } if app.new_instance_state.loader_type.as_str() == loader_type
            && app.new_instance_state.mc_version == mc_version =>
        {
            match result {
                Ok(versions) => {
                    app.new_instance_state.loader_versions = versions;
                    app.new_instance_state.loader_versions_loading = false;
                    app.new_instance_state.loader_versions_error = None;
                    if !app.new_instance_state.loader_versions.is_empty()
                        && (app.new_instance_state.loader_version.is_empty()
                            || !app
                                .new_instance_state
                                .loader_versions
                                .contains(&app.new_instance_state.loader_version))
                    {
                        app.new_instance_state.loader_version =
                            app.new_instance_state.loader_versions[0].clone();
                    }
                }
                Err(e) => {
                    app.new_instance_state.loader_versions_loading = false;
                    app.new_instance_state.loader_versions_error = Some(e);
                }
            }
        }
        _ => {}
    }
}

pub fn handle_install_result(
    app: &mut LauncherApp,
    instance_id: &str,
    name: &str,
    mc_version: &str,
    loader_raw: &str,
    modpack_ids: Option<(&str, &str)>,
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
    let (modpack_project_id, modpack_version_id) = modpack_ids.map_or((None, None), |(p, v)| {
        (Some(p.to_string()), Some(v.to_string()))
    });
    match app.coordinator.create_instance(
        name,
        mc_version.to_string(),
        loader,
        modpack_project_id,
        modpack_version_id,
    ) {
        Ok(instance_id) => {
            app.coordinator.log(
                crate::log::LogLevel::Info,
                &format!("UI: Installed modpack instance '{instance_id}'"),
            );
            app.status_message = format!("Installed modpack instance: {name}");
            app.current_view = View::InstanceList;
            app.new_instance_state.installing_modpack_id = None;
        }
        Err(e) => {
            app.coordinator.log(
                crate::log::LogLevel::Error,
                &format!("Failed to install modpack instance: {e}"),
            );
            app.status_message = format!("Error: {e}");
            app.new_instance_state.installing_modpack_id = None;
        }
    }
}

pub struct NewInstanceState {
    pub name: String,
    pub mc_version: String,
    pub loader_type: LoaderType,
    pub loader_version: String,
    pub tab: InstanceTab,
    pub modrinth_query: String,
    pub mc_version_filter: String,
    pub loader_filter: LoaderType,
    pub modrinth_status: String,
    pub modrinth_results: Vec<ProjectInfo>,
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
            loader_filter: LoaderType::default(),
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum LoaderType {
    #[default]
    Any,
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
            Self::Any => "Any",
            Self::Vanilla => "Vanilla",
            Self::Fabric => "Fabric",
            Self::Forge => "Forge",
            Self::NeoForge => "NeoForge",
            Self::Quilt => "Quilt",
        }
    }

    #[must_use]
    pub fn into_mod_loader(self, version: String) -> ModLoader {
        match self {
            Self::Vanilla | Self::Any => ModLoader::Vanilla,
            Self::Fabric => ModLoader::Fabric {
                loader_version: version,
            },
            Self::Forge => ModLoader::Forge {
                loader_version: version,
            },
            Self::NeoForge => ModLoader::NeoForge {
                loader_version: version,
            },
            Self::Quilt => ModLoader::Quilt {
                loader_version: version,
            },
        }
    }
}
