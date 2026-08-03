use std::path::PathBuf;

use release_the_launcher_mods::{ModProvider, ModrinthProvider, SearchArgs, SortOrder};

use crate::{push_event, Event, Queue};

pub async fn search_modpacks(
    queue: Queue,
    query: String,
    mc_version: String,
    loader: String,
    http: reqwest::Client,
) {
    let provider = ModrinthProvider::with_client(http, None);
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
}

pub async fn search_mods(
    queue: Queue,
    query: String,
    mc_version: String,
    loader_name: String,
    http: reqwest::Client,
) {
    let provider = ModrinthProvider::with_client(http, None);
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
}

pub async fn install_mod(
    queue: Queue,
    project_id: String,
    mods_dir: PathBuf,
    mc_version: Option<String>,
    loader_name: Option<String>,
    http: reqwest::Client,
) {
    let provider = ModrinthProvider::with_client(http, None);
    let mc_versions = mc_version.map(|v| vec![v]).unwrap_or_default();
    let loaders = loader_name
        .filter(|l| l != "vanilla")
        .map(|l| vec![l.to_lowercase()])
        .unwrap_or_default();

    let result = match provider
        .get_versions(&project_id, &mc_versions, &loaders)
        .await
    {
        Ok(versions) => {
            if let Some(version) = versions.first() {
                match provider.download_mod(version, &mods_dir).await {
                    Ok(path) => {
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();
                        Event::Status(format!("Installed mod: {name}"))
                    }
                    Err(e) => Event::DownloadError(format!("Mod install failed: {e}")),
                }
            } else {
                Event::DownloadError("No compatible version found for this instance".into())
            }
        }
        Err(e) => Event::DownloadError(format!("Failed to get mod versions: {e}")),
    };
    push_event(&queue, result);
}

pub async fn fetch_modpack_versions(queue: Queue, project_id: String) {
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
}

pub async fn install_modpack_as_instance(
    queue: Queue,
    project_id: String,
    version_id: Option<String>,
    instances_dir: PathBuf,
    http: reqwest::Client,
) {
    let provider = ModrinthProvider::with_client(http, None);
    let result = match provider
        .install_modpack_as_instance(&project_id, version_id.as_deref(), &instances_dir)
        .await
    {
        Ok((name, mc_ver, loader_str)) => Event::ModrinthInstallResult {
            instance_id: name.clone(),
            name,
            mc_version: mc_ver,
            loader: loader_str,
        },
        Err(e) => Event::DownloadError(format!("Modpack install failed: {e}")),
    };
    push_event(&queue, result);
}
