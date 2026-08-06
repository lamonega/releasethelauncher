mod mrpack;
mod types;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use reqwest::Client;

use types::{ModrinthProject, ModrinthVersion, SearchResponse};

use crate::error::ModsError;
use crate::types::{
    InstalledMod, ModUpdate, ModVersion, ProjectInfo, ReleaseType, SearchArgs, SearchResults, Side,
    SortOrder,
};

use release_the_launcher_constants::urls;
use release_the_launcher_net::cache::HttpMetaCache;
use release_the_launcher_net::{download_to_file, HashKind};

const BASE_URL: &str = urls::MODRINTH_API_URL;

impl From<&ModrinthVersion> for ModVersion {
    fn from(v: &ModrinthVersion) -> Self {
        let primary_file = v
            .files
            .iter()
            .find(|f| f.primary)
            .or_else(|| v.files.first());

        let (hash, hash_type) = primary_file
            .and_then(|f| {
                f.hashes
                    .get("sha512")
                    .map(|h| (Some(h.clone()), Some("sha512".to_string())))
                    .or_else(|| {
                        f.hashes
                            .get("sha1")
                            .map(|h| (Some(h.clone()), Some("sha1".to_string())))
                    })
            })
            .unwrap_or((None, None));

        let filename = primary_file.map(|f| f.filename.clone()).unwrap_or_default();
        let download_url = primary_file.and_then(|f| f.url.clone());
        let file_size = primary_file.map_or(0, |f| f.size);

        Self {
            id: v.id.clone(),
            project_id: v.project_id.clone(),
            name: v.name.clone(),
            version_number: v.version_number.clone(),
            release_type: match v.version_type.as_str() {
                "beta" => ReleaseType::Beta,
                "alpha" => ReleaseType::Alpha,
                _ => ReleaseType::Release,
            },
            mc_versions: v.game_versions.clone(),
            loaders: v.loaders.clone(),
            download_url,
            filename,
            hash,
            hash_type,
            file_size,
        }
    }
}

impl ModVersion {
    fn checksum(&self) -> Option<(HashKind, &str)> {
        match (&self.hash_type, &self.hash) {
            (Some(ht), Some(h)) => {
                let kind = match ht.as_str() {
                    "sha512" => HashKind::Sha512,
                    _ => HashKind::Sha1,
                };
                Some((kind, h.as_str()))
            }
            _ => None,
        }
    }
}

fn hits_to_summaries(resp: SearchResponse) -> SearchResults {
    let hits = resp
        .hits
        .into_iter()
        .map(|h| ProjectInfo {
            id: h.project_id,
            name: h.title,
            slug: h.slug,
            description: h.description,
            authors: vec![h.author],
            icon_url: h.icon_url,
            website_url: None,
            downloads: h.downloads,
            side: Side::Universal,
        })
        .collect();

    SearchResults {
        hits,
        total_hits: resp.total_hits,
    }
}

pub struct ModrinthProvider {
    http: Client,
    api_token: Option<String>,
    cache: Arc<Mutex<HttpMetaCache>>,
}

impl ModrinthProvider {
    #[must_use]
    pub fn new(api_token: Option<String>) -> Self {
        Self::with_client(release_the_launcher_net::default_client(), api_token)
    }

    #[must_use]
    pub fn with_client(http: Client, api_token: Option<String>) -> Self {
        let cache_path = std::env::temp_dir()
            .join(release_the_launcher_constants::paths::APP_DIR_NAME)
            .join("modrinth_meta_cache.json");
        let cache = HttpMetaCache::load(&cache_path);
        Self {
            http,
            api_token,
            cache: Arc::new(Mutex::new(cache)),
        }
    }

    fn build_headers(&self) -> Vec<(&str, &str)> {
        let mut headers = vec![(
            "User-Agent",
            release_the_launcher_constants::net::USER_AGENT,
        )];
        if let Some(ref token) = self.api_token {
            headers.push(("Authorization", token));
        }
        headers
    }

    fn apply_headers(&self, mut req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        for (k, v) in self.build_headers() {
            req = req.header(k, v);
        }
        req
    }

    fn cache_get(&self, cache_key: &str) -> Option<String> {
        if let Ok(cache_guard) = self.cache.lock() {
            if let Some(entry) = cache_guard.resolve(BASE_URL, cache_key) {
                return entry.data;
            }
        }
        None
    }

    fn cache_put(&self, cache_key: &str, json: &str) {
        if let Ok(mut cache_guard) = self.cache.lock() {
            let entry = release_the_launcher_net::cache::CacheEntry {
                base_path: BASE_URL.to_string(),
                relative_path: cache_key.to_string(),
                data: Some(json.to_string()),
                max_age: 900,
                last_accessed: 0,
                is_eternal: false,
            };
            cache_guard.update(entry);
            if let Err(e) = cache_guard.save() {
                log::warn!("Failed to persist modrinth cache: {e}");
            }
        }
    }

    fn build_search_query(args: &SearchArgs, project_type: &str) -> Vec<(String, String)> {
        let mut params = vec![
            ("limit".to_string(), args.limit.to_string()),
            ("offset".to_string(), args.offset.to_string()),
        ];
        if !args.query.is_empty() {
            params.push(("query".to_string(), args.query.clone()));
        }
        let facets = Self::build_facets_with_type(args, project_type);
        if !facets.is_empty() && facets != "[]" {
            params.push(("facets".to_string(), facets));
        }
        if args.sort != SortOrder::Relevance {
            params.push(("index".to_string(), args.sort.as_str().to_string()));
        }
        params
    }

    fn build_facets_with_type(args: &SearchArgs, project_type: &str) -> String {
        let mut facets: Vec<Vec<String>> = Vec::new();

        if !args.loaders.is_empty() {
            let loader_facets: Vec<String> = args
                .loaders
                .iter()
                .map(|l| format!("categories:{l}"))
                .collect();
            facets.push(loader_facets);
        }

        if !args.mc_versions.is_empty() {
            let version_facets: Vec<String> = args
                .mc_versions
                .iter()
                .map(|v| format!("versions:{v}"))
                .collect();
            facets.push(version_facets);
        }

        facets.push(vec![format!("project_type:{project_type}")]);

        serde_json::to_string(&facets).unwrap_or_else(|_| "[]".to_string())
    }

    /// Download and extract a .mrpack modpack into the target instance directory.
    ///
    /// # Errors
    ///
    /// Returns an error if the download, ZIP extraction, or manifest parsing fails.
    pub async fn download_modpack(
        &self,
        version: &ModVersion,
        target_dir: &Path,
    ) -> Result<PathBuf, ModsError> {
        let url = version
            .download_url
            .as_ref()
            .ok_or_else(|| ModsError::Provider("No download URL".into()))?;

        let zip_path = target_dir.join(&version.filename);
        if let Some(parent) = target_dir.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::create_dir_all(target_dir)?;

        let checksum = version.checksum();

        download_to_file(&self.http, url, &zip_path, checksum, None)
            .await
            .map_err(|e| ModsError::Provider(e.to_string()))?;

        let file = fs::File::open(&zip_path)?;
        let mut archive = zip::ZipArchive::new(file)?;

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let Some(entry_path) = entry.enclosed_name() else {
                continue;
            };
            let name_str = entry_path.to_string_lossy();

            if name_str == release_the_launcher_constants::paths::MODRINTH_INDEX_FILE {
                let out_path =
                    target_dir.join(release_the_launcher_constants::paths::MODRINTH_INDEX_FILE);
                let mut out_file = fs::File::create(&out_path)?;
                std::io::copy(&mut entry, &mut out_file)?;
                continue;
            }

            let components: Vec<_> = entry_path.components().collect();
            if components.is_empty() {
                continue;
            }
            let first = components[0].as_os_str().to_string_lossy();
            if first == "overrides" || first == "client-overrides" {
                let rel: PathBuf = components[1..].iter().collect();
                let out_path = target_dir
                    .join(release_the_launcher_constants::paths::MINECRAFT_DIR)
                    .join(rel);
                if entry.is_dir() {
                    fs::create_dir_all(&out_path)?;
                } else {
                    if let Some(parent) = out_path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    let mut out_file = fs::File::create(&out_path)?;
                    std::io::copy(&mut entry, &mut out_file)?;
                }
            }
        }

        Ok(zip_path)
    }

    /// Download all mod files specified in `modrinth.index.json` in parallel.
    ///
    /// # Errors
    ///
    /// Returns an error if reading `modrinth.index.json` or downloading fails.
    ///
    /// # Panics
    ///
    /// Panics if a task join fails.
    pub async fn download_modpack_files(
        &self,
        target_dir: &Path,
        progress: impl Fn(u64, u64, &str) + Send + Sync + 'static,
    ) -> Result<(), ModsError> {
        let index_path =
            target_dir.join(release_the_launcher_constants::paths::MODRINTH_INDEX_FILE);
        if !index_path.exists() {
            return Ok(());
        }

        let content = fs::read_to_string(&index_path)?;
        let index: types::MrpackIndex = serde_json::from_str(&content)?;

        if index.files.is_empty() {
            return Ok(());
        }

        mrpack::download_mrpack_files(&self.http, target_dir, &index, progress).await
    }

    async fn resolve_version(
        &self,
        project_id: &str,
        version_id: Option<&str>,
    ) -> Result<ModVersion, ModsError> {
        let versions = self.get_versions(project_id, &[], &[]).await?;
        if let Some(vid) = version_id {
            versions
                .iter()
                .find(|v| v.id == vid)
                .or_else(|| versions.first())
                .cloned()
                .ok_or_else(|| ModsError::Provider("Version not found".into()))
        } else {
            versions
                .first()
                .cloned()
                .ok_or_else(|| ModsError::Provider("No versions found".into()))
        }
    }

    /// Resolves a modpack without downloading anything. Returns
    /// (`instance_name`, `mc_version`, `loader_name`).
    ///
    /// # Errors
    ///
    /// Returns an error if the project or version lookup fails.
    pub async fn resolve_modpack_as_instance(
        &self,
        project_id: &str,
        version_id: Option<&str>,
    ) -> Result<(String, String, String), ModsError> {
        let version = self.resolve_version(project_id, version_id).await?;

        let project = self.get_project(project_id).await?;
        let instance_name = if version_id.is_some() {
            format!("{} ({})", project.name, version.version_number)
        } else {
            project.name.clone()
        };
        let mc_version = version.mc_versions.first().cloned().unwrap_or_default();
        let loader = version
            .loaders
            .first()
            .map_or("Vanilla", |l| match l.to_lowercase().as_str() {
                "fabric" => "Fabric",
                "forge" => "Forge",
                "neoforge" => "NeoForge",
                "quilt" => "Quilt",
                _ => "Vanilla",
            })
            .to_string();

        Ok((instance_name, mc_version, loader))
    }

    /// Downloads and extracts a modpack's `.mrpack` (index + overrides) into the
    /// target instance directory without downloading any mod files.
    ///
    /// # Errors
    ///
    /// Returns an error if the version lookup, download, or extraction fails.
    pub async fn download_modpack_manifest(
        &self,
        project_id: &str,
        version_id: Option<&str>,
        target_dir: &Path,
    ) -> Result<(), ModsError> {
        let version = self.resolve_version(project_id, version_id).await?;

        self.download_modpack(&version, target_dir).await?;
        Ok(())
    }

    #[must_use]
    pub fn name(&self) -> &'static str {
        "Modrinth"
    }

    /// # Errors
    ///
    /// Returns an error if the HTTP request or JSON parsing fails.
    pub async fn search(
        &self,
        args: SearchArgs,
        project_type: Option<&str>,
    ) -> Result<SearchResults, ModsError> {
        let query_params = Self::build_search_query(&args, project_type.unwrap_or("mod"));
        let path = format!("{BASE_URL}/search");

        let cache_key = format!(
            "search?{}",
            serde_json::to_string(&query_params).unwrap_or_default()
        );
        if let Some(json) = self.cache_get(&cache_key) {
            if let Ok(resp) = serde_json::from_str::<SearchResponse>(&json) {
                return Ok(hits_to_summaries(resp));
            }
        }

        let req = self.apply_headers(self.http.get(&path).query(&query_params));
        let json_text = req.send().await?.text().await?;
        let resp: SearchResponse = serde_json::from_str(&json_text)?;

        self.cache_put(&cache_key, &json_text);

        Ok(hits_to_summaries(resp))
    }

    /// # Errors
    ///
    /// Returns an error if the HTTP request or JSON parsing fails.
    pub async fn get_versions(
        &self,
        project_id: &str,
        mc_versions: &[String],
        loaders: &[String],
    ) -> Result<Vec<ModVersion>, ModsError> {
        let mut query_params = Vec::new();
        if !mc_versions.is_empty() {
            query_params.push((
                "game_versions".to_string(),
                serde_json::to_string(mc_versions).unwrap_or_default(),
            ));
        }
        if !loaders.is_empty() {
            query_params.push((
                "loaders".to_string(),
                serde_json::to_string(loaders).unwrap_or_default(),
            ));
        }

        let path = format!("{BASE_URL}/project/{project_id}/version");
        let cache_key = format!(
            "version/{project_id}?{}",
            serde_json::to_string(&query_params).unwrap_or_default()
        );

        if let Some(json) = self.cache_get(&cache_key) {
            if let Ok(resp) = serde_json::from_str::<Vec<ModrinthVersion>>(&json) {
                let versions = resp.into_iter().map(|v| ModVersion::from(&v)).collect();
                return Ok(versions);
            }
        }

        let req = self.apply_headers(self.http.get(&path).query(&query_params));
        let json_text = req.send().await?.text().await?;
        let resp: Vec<ModrinthVersion> = serde_json::from_str(&json_text)?;

        self.cache_put(&cache_key, &json_text);

        let versions = resp.into_iter().map(|v| ModVersion::from(&v)).collect();
        Ok(versions)
    }

    /// # Errors
    ///
    /// Returns an error if the HTTP request or JSON parsing fails.
    pub async fn get_project(&self, project_id: &str) -> Result<ProjectInfo, ModsError> {
        let url = format!("{BASE_URL}/project/{project_id}");
        let req = self.apply_headers(self.http.get(&url));
        let resp: ModrinthProject = req.send().await?.json().await?;

        let authors = match resp.team {
            serde_json::Value::String(s) => vec![s],
            serde_json::Value::Array(arr) => arr
                .into_iter()
                .filter_map(|v| v.as_str().map(ToString::to_string))
                .collect(),
            _ => Vec::new(),
        };

        Ok(ProjectInfo {
            id: resp.id,
            name: resp.title,
            slug: resp.slug,
            description: resp.description,
            authors,
            icon_url: resp.icon_url,
            website_url: resp.source_url,
            downloads: resp.downloads,
            side: Side::Universal,
        })
    }

    /// # Errors
    ///
    /// Returns an error if the HTTP request or JSON parsing fails.
    pub async fn check_updates(
        &self,
        installed: &[InstalledMod],
        mc_versions: &[String],
        loaders: &[String],
    ) -> Result<Vec<ModUpdate>, ModsError> {
        if installed.is_empty() {
            return Ok(Vec::new());
        }

        let hashes: Vec<String> = installed
            .iter()
            .map(|mod_item| mod_item.hash.to_lowercase())
            .collect();

        let url = format!("{BASE_URL}/version_files/update");
        let body = serde_json::json!({
            "hashes": hashes,
            "algorithm": "sha1",
            "loaders": loaders,
            "game_versions": mc_versions,
        });

        let req = self.apply_headers(self.http.post(&url).json(&body));

        let resp = req.send().await?;
        if !resp.status().is_success() {
            return Ok(Vec::new());
        }

        let update_map: HashMap<String, ModrinthVersion> = resp.json().await?;
        let mut updates = Vec::new();

        for installed_mod in installed {
            let lower_hash = installed_mod.hash.to_lowercase();
            if let Some(version) = update_map.get(&lower_hash) {
                let latest: ModVersion = ModVersion::from(version);
                if latest.hash.as_deref() != Some(&lower_hash) {
                    updates.push(ModUpdate {
                        installed: installed_mod.clone(),
                        latest,
                    });
                }
            }
        }

        Ok(updates)
    }

    /// # Errors
    ///
    /// Returns an error if the download, ZIP extraction, or manifest parsing fails.
    pub async fn download_mod(
        &self,
        version: &ModVersion,
        target_dir: &Path,
    ) -> Result<PathBuf, ModsError> {
        let url = version
            .download_url
            .as_ref()
            .ok_or_else(|| ModsError::Provider("No download URL".into()))?;

        fs::create_dir_all(target_dir)?;
        let path = target_dir.join(&version.filename);

        let checksum = version.checksum();

        download_to_file(&self.http, url, &path, checksum, None)
            .await
            .map_err(|e| ModsError::Provider(e.to_string()))?;

        let final_path = mrpack::unpack_structured_mod_archive_if_needed(&path, target_dir);
        Ok(final_path)
    }
}
