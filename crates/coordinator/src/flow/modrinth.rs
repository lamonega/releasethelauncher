use std::path::PathBuf;
use std::sync::Arc;

use release_the_launcher_mods::{ModProvider, ModrinthProvider, SearchArgs, SortOrder};

use crate::{push_event, Event, Queue};

pub fn search_modpacks(
    queue: &Queue,
    handle: &tokio::runtime::Handle,
    query: String,
    mc_version: String,
    loader: String,
) {
    let queue = Arc::clone(queue);
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
            sort: SortOrder::Downloads,
        };
        let result = match provider.search_modpacks(&args).await {
            Ok(results) => Event::ModrinthSearchResult(Ok(results)),
            Err(e) => Event::ModrinthSearchResult(Err(e.to_string())),
        };
        push_event(&queue, result);
    });
}

pub fn search_mods(
    queue: &Queue,
    handle: &tokio::runtime::Handle,
    query: String,
    mc_version: String,
    loader_name: String,
) {
    let queue = Arc::clone(queue);
    handle.spawn(async move {
        let provider = ModrinthProvider::new(None);
        let mc_versions = if mc_version.is_empty() {
            vec![]
        } else {
            vec![mc_version]
        };
        let loader_clean = loader_name.split_whitespace().next().unwrap_or("");
        let loaders = if loader_clean.is_empty() || loader_clean == "vanilla" {
            vec![]
        } else {
            vec![loader_clean.to_string()]
        };
        let args = SearchArgs {
            query,
            offset: 0,
            limit: 20,
            loaders,
            mc_versions,
            categories: vec![],
            sort: SortOrder::Downloads,
        };
        let result = match provider.search(args).await {
            Ok(results) => Event::ModrinthSearchResult(Ok(results)),
            Err(e) => Event::ModrinthSearchResult(Err(e.to_string())),
        };
        push_event(&queue, result);
    });
}

pub fn install_mod(
    queue: &Queue,
    handle: &tokio::runtime::Handle,
    project_id: String,
    mods_dir: PathBuf,
    mc_version: Option<String>,
    loader_name: Option<String>,
) {
    let queue = Arc::clone(queue);
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
                        Ok(path) => Event::ModrinthInstallResult(Ok(path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default())),
                        Err(e) => Event::ModrinthInstallResult(Err(e.to_string())),
                    }
                } else {
                    Event::ModrinthInstallResult(Err(
                        "No compatible version found for this instance".into(),
                    ))
                }
            }
            Err(e) => Event::ModrinthInstallResult(Err(e.to_string())),
        };
        push_event(&queue, result);
    });
}

pub fn fetch_modpack_versions(
    queue: &Queue,
    handle: &tokio::runtime::Handle,
    project_id: String,
) {
    let queue = Arc::clone(queue);
    handle.spawn(async move {
        let provider = ModrinthProvider::new(None);
        let result = match provider.get_versions(&project_id, &[], &[]).await {
            Ok(versions) => Event::ModrinthVersionsResult {
                project_id,
                result: Ok(versions),
            },
            Err(e) => Event::ModrinthVersionsResult {
                project_id,
                result: Err(e.to_string()),
            },
        };
        push_event(&queue, result);
    });
}

pub fn install_modpack_as_instance(
    queue: &Queue,
    handle: &tokio::runtime::Handle,
    project_id: String,
    version_id: Option<String>,
    instances_dir: PathBuf,
) {
    let queue = Arc::clone(queue);
    handle.spawn(async move {
        let provider = ModrinthProvider::new(None);
        let result = match provider
            .install_modpack_as_instance(&project_id, version_id.as_deref(), &instances_dir)
            .await
        {
            Ok((name, mc_ver, loader_str)) => {
                Event::ModrinthInstallResult(Ok(format!("{name}|{mc_ver}|{loader_str}")))
            }
            Err(e) => Event::ModrinthInstallResult(Err(e.to_string())),
        };
        push_event(&queue, result);
    });
}
