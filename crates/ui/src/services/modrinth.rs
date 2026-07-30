use std::sync::{Arc, Mutex};
use release_the_launcher_mods::{ModProvider, ModrinthProvider, SearchArgs};
use crate::UiMessage;

pub fn send_msg(queue: &Arc<Mutex<Vec<UiMessage>>>, ctx: &egui::Context, msg: UiMessage) {
    if let Ok(mut q) = queue.lock() {
        q.push(msg);
    }
    ctx.request_repaint();
}

pub fn search_modpacks(
    queue: Arc<Mutex<Vec<UiMessage>>>,
    ctx: egui::Context,
    handle: tokio::runtime::Handle,
    query: String,
    mc_version: String,
    loader: String,
) {
    handle.spawn(async move {
        let provider = ModrinthProvider::new(None);
        let args = SearchArgs {
            query,
            offset: 0,
            limit: 20,
            loaders: if loader.is_empty() {
                vec![]
            } else {
                vec![loader]
            },
            mc_versions: if mc_version.is_empty() {
                vec![]
            } else {
                vec![mc_version]
            },
            categories: vec![],
            sort: release_the_launcher_mods::SortOrder::Downloads,
        };
        let result = match provider.search_modpacks(&args).await {
            Ok(results) => UiMessage::ModrinthSearchResult(Ok(results)),
            Err(e) => UiMessage::ModrinthSearchResult(Err(e.to_string())),
        };
        send_msg(&queue, &ctx, result);
    });
}

pub fn install_mod(
    queue: Arc<Mutex<Vec<UiMessage>>>,
    ctx: egui::Context,
    handle: tokio::runtime::Handle,
    project_id: String,
    mods_dir: std::path::PathBuf,
    mc_version: Option<String>,
    loader_name: Option<String>,
) {
    handle.spawn(async move {
        let provider = ModrinthProvider::new(None);
        let mc_versions = mc_version.map(|v| vec![v]).unwrap_or_default();
        let loaders = loader_name
            .filter(|l| l != "vanilla")
            .map(|l| vec![l.to_lowercase()])
            .unwrap_or_default();

        let result = match provider.get_versions(&project_id, &mc_versions, &loaders).await {
            Ok(versions) => {
                if let Some(version) = versions.first() {
                    match provider.download_mod(version, &mods_dir).await {
                        Ok(path) => UiMessage::ModrinthInstallResult(Ok(path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default())),
                        Err(e) => UiMessage::ModrinthInstallResult(Err(e.to_string())),
                    }
                } else {
                    UiMessage::ModrinthInstallResult(Err(
                        "No compatible version found for this instance".into(),
                    ))
                }
            }
            Err(e) => UiMessage::ModrinthInstallResult(Err(e.to_string())),
        };
        send_msg(&queue, &ctx, result);
    });
}

pub fn fetch_modpack_versions(
    queue: Arc<Mutex<Vec<UiMessage>>>,
    ctx: egui::Context,
    handle: tokio::runtime::Handle,
    project_id: String,
) {
    let pid = project_id.clone();
    handle.spawn(async move {
        let provider = ModrinthProvider::new(None);
        let result = match provider.get_versions(&pid, &[], &[]).await {
            Ok(versions) => UiMessage::ModrinthVersionsResult {
                project_id: pid,
                result: Ok(versions),
            },
            Err(e) => UiMessage::ModrinthVersionsResult {
                project_id: pid,
                result: Err(e.to_string()),
            },
        };
        send_msg(&queue, &ctx, result);
    });
}

pub fn install_modpack_as_instance(
    queue: Arc<Mutex<Vec<UiMessage>>>,
    ctx: egui::Context,
    handle: tokio::runtime::Handle,
    project_id: String,
    version_id: Option<String>,
    instances_dir: std::path::PathBuf,
) {
    handle.spawn(async move {
        let provider = ModrinthProvider::new(None);
        let result = match provider
            .install_modpack_as_instance(&project_id, version_id.as_deref(), &instances_dir)
            .await
        {
            Ok((name, mc_ver, loader_str)) => {
                UiMessage::ModrinthInstallResult(Ok(format!("{name}|{mc_ver}|{loader_str}")))
            }
            Err(e) => UiMessage::ModrinthInstallResult(Err(e.to_string())),
        };
        send_msg(&queue, &ctx, result);
    });
}
