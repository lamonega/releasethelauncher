use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use reqwest::Client;

use super::modrinth_types::{ModrinthProject, ModrinthVersion, MrpackFile, MrpackIndex, SearchResponse};
use crate::{
    safe_join_under, InstalledMod, ModProvider, ModUpdate, ModVersion, ModsError, ProjectInfo,
    ProjectSummary, ReleaseType, SearchArgs, SearchResults, Side, SortOrder,
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

impl From<ModrinthVersion> for ModVersion {
    fn from(v: ModrinthVersion) -> Self {
        Self::from(&v)
    }
}

fn hits_to_summaries(resp: SearchResponse) -> SearchResults {
    let hits = resp
        .hits
        .into_iter()
        .map(|h| ProjectSummary {
            id: h.project_id,
            name: h.title,
            slug: h.slug,
            description: h.description,
            author: h.author,
            icon_url: h.icon_url,
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

    /// Search for modpacks on Modrinth.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request or JSON parsing fails.
    pub async fn search_modpacks(&self, args: &SearchArgs) -> Result<SearchResults, ModsError> {
        let query_params = Self::build_search_query(args, "modpack");
        let url = format!("{BASE_URL}/search");
        let mut req = self.http.get(&url).query(&query_params);
        for (k, v) in self.build_headers() {
            req = req.header(k, v);
        }
        let resp: SearchResponse = req.send().await?.json().await?;
        Ok(hits_to_summaries(resp))
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

        let checksum = match (&version.hash_type, &version.hash) {
            (Some(ht), Some(h)) => {
                let kind = match ht.as_str() {
                    "sha512" => HashKind::Sha512,
                    _ => HashKind::Sha1,
                };
                Some((kind, h.as_str()))
            }
            _ => None,
        };

        download_to_file(&self.http, url, &zip_path, checksum, None::<fn(u64, u64)>)
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
        let index: MrpackIndex = serde_json::from_str(&content)?;

        if index.files.is_empty() {
            return Ok(());
        }

        let mc_dir = target_dir.join(".minecraft");
        let (total_bytes, initial_downloaded) =
            count_modpack_file_sizes(&index.files, &mc_dir);

        let total_b = Arc::new(std::sync::atomic::AtomicU64::new(total_bytes));
        let downloaded_b = Arc::new(std::sync::atomic::AtomicU64::new(initial_downloaded));
        let sem = Arc::new(tokio::sync::Semaphore::new(
            release_the_launcher_constants::net::DEFAULT_MAX_CONCURRENT_DOWNLOADS,
        ));
        let progress_cb = Arc::new(progress);
        let mut tasks = Vec::new();
        let client = self.http.clone();

        for file_obj in index.files {
            if let Some(url) = file_obj.downloads.first().cloned() {
                let path_str = file_obj.path.clone();
                let Ok(dest) =
                    safe_join_under(&target_dir.join(".minecraft"), Path::new(&path_str))
                else {
                    log::warn!("Skipping modpack file outside target dir: {path_str}");
                    continue;
                };
                let display_name = Path::new(&path_str)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or(path_str);

                let sem = sem.clone();
                let client = client.clone();
                let downloaded_cnt = downloaded_b.clone();
                let total_cnt = total_b.clone();
                let progress_ref = progress_cb.clone();
                let size = file_obj.file_size;

                let checksum = file_obj
                    .hashes
                    .get("sha512")
                    .map(|h| (HashKind::Sha512, h.clone()))
                    .or_else(|| {
                        file_obj
                            .hashes
                            .get("sha1")
                            .map(|h| (HashKind::Sha1, h.clone()))
                    });

                tasks.push(tokio::spawn(async move {
                    if !dest.exists() || dest.metadata().map_or(true, |m| m.len() == 0) {
                        let Ok(_permit) = sem.acquire().await else {
                            return;
                        };
                        let checksum_ref = checksum.as_ref().map(|(k, h)| (*k, h.as_str()));
                        if download_to_file(
                            &client,
                            &url,
                            &dest,
                            checksum_ref,
                            None::<fn(u64, u64)>,
                        )
                        .await
                        .is_ok()
                        {
                            downloaded_cnt.fetch_add(size, std::sync::atomic::Ordering::SeqCst);
                        } else {
                            log::warn!("Failed to download mod file {display_name}");
                        }
                    } else {
                        downloaded_cnt.fetch_add(size, std::sync::atomic::Ordering::SeqCst);
                    }
                    let cur = downloaded_cnt.load(std::sync::atomic::Ordering::SeqCst);
                    let tot = total_cnt.load(std::sync::atomic::Ordering::SeqCst);
                    progress_ref(cur, tot.max(cur), &display_name);
                }));
            }
        }

        for task in tasks {
            if let Err(e) = task.await {
                log::warn!("A download task panicked: {e}");
            }
        }

        Ok(())
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
        let versions = self.get_versions(project_id, &[], &[]).await?;
        let version = if let Some(vid) = version_id {
            versions
                .iter()
                .find(|v| v.id == vid)
                .or_else(|| versions.first())
                .ok_or_else(|| ModsError::Provider("Version not found".into()))?
        } else {
            versions
                .first()
                .ok_or_else(|| ModsError::Provider("No versions found".into()))?
        };

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
        let versions = self.get_versions(project_id, &[], &[]).await?;
        let version = if let Some(vid) = version_id {
            versions
                .iter()
                .find(|v| v.id == vid)
                .or_else(|| versions.first())
                .ok_or_else(|| ModsError::Provider("Version not found".into()))?
        } else {
            versions
                .first()
                .ok_or_else(|| ModsError::Provider("No versions found".into()))?
        };

        self.download_modpack(version, target_dir).await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl ModProvider for ModrinthProvider {
    fn name(&self) -> &'static str {
        "Modrinth"
    }

    async fn search(&self, args: SearchArgs) -> Result<SearchResults, ModsError> {
        let query_params = Self::build_search_query(&args, "mod");
        let path = format!("{BASE_URL}/search");

        let cache_key = format!(
            "search?{}",
            serde_json::to_string(&query_params).unwrap_or_default()
        );
        if let Ok(mut cache_guard) = self.cache.lock() {
            if let Some(entry) = cache_guard.resolve(BASE_URL, &cache_key) {
                if let Some(ref json_str) = entry.md5 {
                    if let Ok(resp) = serde_json::from_str::<SearchResponse>(json_str) {
                        return Ok(hits_to_summaries(resp));
                    }
                }
            }
        }

        let mut req = self.http.get(&path).query(&query_params);
        for (k, v) in self.build_headers() {
            req = req.header(k, v);
        }
        let json_text = req.send().await?.text().await?;
        let resp: SearchResponse = serde_json::from_str(&json_text)?;

        if let Ok(mut cache_guard) = self.cache.lock() {
            let entry = release_the_launcher_net::cache::CacheEntry {
                base_path: BASE_URL.to_string(),
                relative_path: cache_key,
                etag: None,
                last_modified: None,
                md5: Some(json_text),
                max_age: 900,
                last_accessed: 0,
                is_eternal: false,
            };
            cache_guard.update(entry);
            if let Err(e) = cache_guard.save() {
                log::warn!("Failed to persist modrinth search cache: {e}");
            }
        }

        Ok(hits_to_summaries(resp))
    }

    async fn get_versions(
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

        if let Ok(mut cache_guard) = self.cache.lock() {
            if let Some(entry) = cache_guard.resolve(BASE_URL, &cache_key) {
                if let Some(ref json_str) = entry.md5 {
                    if let Ok(resp) = serde_json::from_str::<Vec<ModrinthVersion>>(json_str) {
                        let versions = resp.into_iter().map(Into::into).collect();
                        return Ok(versions);
                    }
                }
            }
        }

        let mut req = self.http.get(&path).query(&query_params);
        for (k, v) in self.build_headers() {
            req = req.header(k, v);
        }
        let json_text = req.send().await?.text().await?;
        let resp: Vec<ModrinthVersion> = serde_json::from_str(&json_text)?;

        if let Ok(mut cache_guard) = self.cache.lock() {
            let entry = release_the_launcher_net::cache::CacheEntry {
                base_path: BASE_URL.to_string(),
                relative_path: cache_key,
                etag: None,
                last_modified: None,
                md5: Some(json_text),
                max_age: 900,
                last_accessed: 0,
                is_eternal: false,
            };
            cache_guard.update(entry);
            if let Err(e) = cache_guard.save() {
                log::warn!("Failed to persist modrinth versions cache: {e}");
            }
        }

        let versions = resp.into_iter().map(Into::into).collect();
        Ok(versions)
    }

    async fn get_project(&self, project_id: &str) -> Result<ProjectInfo, ModsError> {
        let url = format!("{BASE_URL}/project/{project_id}");
        let mut req = self.http.get(&url);
        for (k, v) in self.build_headers() {
            req = req.header(k, v);
        }
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

    async fn check_updates(
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

        let mut req = self.http.post(&url).json(&body);
        for (k, v) in self.build_headers() {
            req = req.header(k, v);
        }

        let resp = req.send().await?;
        if !resp.status().is_success() {
            return Ok(Vec::new());
        }

        let update_map: HashMap<String, ModrinthVersion> = resp.json().await?;
        let mut updates = Vec::new();

        for installed_mod in installed {
            let lower_hash = installed_mod.hash.to_lowercase();
            if let Some(version) = update_map.get(&lower_hash) {
                let latest: ModVersion = version.into();
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

    async fn download_mod(
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

        let checksum = match (&version.hash_type, &version.hash) {
            (Some(ht), Some(h)) => {
                let kind = match ht.as_str() {
                    "sha512" => HashKind::Sha512,
                    _ => HashKind::Sha1,
                };
                Some((kind, h.as_str()))
            }
            _ => None,
        };

        download_to_file(&self.http, url, &path, checksum, None::<fn(u64, u64)>)
            .await
            .map_err(|e| ModsError::Provider(e.to_string()))?;

        let final_path = unpack_structured_mod_archive_if_needed(&path, target_dir);
        Ok(final_path)
    }
}

fn count_modpack_file_sizes(files: &[MrpackFile], mc_dir: &Path) -> (u64, u64) {
    let mut total_bytes: u64 = 0;
    let mut initial_downloaded: u64 = 0;
    for file_obj in files {
        let size = file_obj.file_size;
        if let Ok(dest) = safe_join_under(mc_dir, Path::new(&file_obj.path)) {
            total_bytes += size;
            if dest.exists() && dest.metadata().is_ok_and(|m| m.len() > 0) {
                initial_downloaded += size;
            }
        } else {
            log::warn!(
                "Skipping modpack file outside target dir: {}",
                file_obj.path
            );
        }
    }
    (total_bytes, initial_downloaded)
}

fn unpack_structured_mod_archive_if_needed(path: &Path, target_dir: &Path) -> PathBuf {
    if path
        .extension()
        .and_then(|s| s.to_str())
        .map(str::to_lowercase)
        .as_deref()
        != Some("zip")
    {
        return path.to_path_buf();
    }

    let Ok(file) = fs::File::open(path) else {
        return path.to_path_buf();
    };

    let Ok(mut archive) = zip::ZipArchive::new(file) else {
        return path.to_path_buf();
    };

    let (has_jar_dir, has_resources_dir, has_mods_dir) = detect_archive_structure(&mut archive);

    if !has_jar_dir && !has_resources_dir && !has_mods_dir {
        return path.to_path_buf();
    }

    let parent_mc_dir = target_dir.parent().unwrap_or(target_dir);
    let resources_dest = parent_mc_dir.join("resources");
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("mod");
    let new_jar_path = target_dir.join(format!("{stem}.jar"));

    if has_jar_dir {
        repack_jar_entries(&mut archive, &new_jar_path);
    }
    if has_resources_dir {
        extract_resource_entries(&mut archive, &resources_dest);
    }
    if has_mods_dir {
        extract_mod_entries(&mut archive, target_dir);
    }

    fs::remove_file(path).ok();
    if has_jar_dir {
        new_jar_path
    } else {
        target_dir.to_path_buf()
    }
}

fn detect_archive_structure<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
) -> (bool, bool, bool) {
    let mut has_jar_dir = false;
    let mut has_resources_dir = false;
    let mut has_mods_dir = false;

    for i in 0..archive.len() {
        if let Ok(file) = archive.by_index(i) {
            let name = file.name();
            if name.starts_with("Jar/")
                || name.starts_with("jar/")
                || name.starts_with("minecraft/")
            {
                has_jar_dir = true;
            }
            if name.starts_with("Resources/") || name.starts_with("resources/") {
                has_resources_dir = true;
            }
            if name.starts_with("mods/") {
                has_mods_dir = true;
            }
        }
    }

    (has_jar_dir, has_resources_dir, has_mods_dir)
}

fn repack_jar_entries<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    new_jar_path: &Path,
) {
    let Ok(jar_file) = fs::File::create(new_jar_path) else {
        return;
    };
    let mut zip_writer = zip::ZipWriter::new(jar_file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    for i in 0..archive.len() {
        if let Ok(mut zip_entry) = archive.by_index(i) {
            let name = zip_entry.name().to_string();
            let prefix = if name.starts_with("Jar/") {
                Some("Jar/")
            } else if name.starts_with("jar/") {
                Some("jar/")
            } else if name.starts_with("minecraft/") {
                Some("minecraft/")
            } else {
                None
            };

            if let Some(pfx) = prefix {
                let inner_name = &name[pfx.len()..];
                if !inner_name.is_empty() {
                    if zip_entry.is_dir() {
                        if let Err(e) = zip_writer.add_directory(inner_name, options) {
                            log::warn!("Failed to add directory {inner_name} to repacked jar: {e}");
                        }
                    } else if zip_writer.start_file(inner_name, options).is_ok() {
                        let mut buffer = Vec::new();
                        if std::io::Read::read_to_end(&mut zip_entry, &mut buffer).is_ok() {
                            if let Err(e) = std::io::Write::write_all(&mut zip_writer, &buffer) {
                                log::warn!("Failed to write {inner_name} to repacked jar: {e}");
                            }
                        }
                    }
                }
            }
        }
    }
    if let Err(e) = zip_writer.finish() {
        log::warn!(
            "Failed to finalize repacked jar {jar}: {e}",
            jar = new_jar_path.display()
        );
    }
}

fn extract_resource_entries<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    resources_dest: &Path,
) {
    for i in 0..archive.len() {
        if let Ok(mut zip_entry) = archive.by_index(i) {
            let name = zip_entry.name().to_string();
            let prefix = if name.starts_with("Resources/") {
                Some("Resources/")
            } else if name.starts_with("resources/") {
                Some("resources/")
            } else {
                None
            };

            if let Some(pfx) = prefix {
                let inner_name = &name[pfx.len()..];
                if !inner_name.is_empty() {
                    let Ok(out_path) = safe_join_under(resources_dest, Path::new(inner_name))
                    else {
                        log::warn!("Skipping resource entry outside target dir: {name}");
                        continue;
                    };
                    if zip_entry.is_dir() {
                        if let Err(e) = fs::create_dir_all(&out_path) {
                            log::warn!(
                                "Failed to create resource dir {dir}: {e}",
                                dir = out_path.display()
                            );
                        }
                    } else {
                        if let Some(parent) = out_path.parent() {
                            if let Err(e) = fs::create_dir_all(parent) {
                                log::warn!(
                                    "Failed to create resource dir {dir}: {e}",
                                    dir = parent.display()
                                );
                            }
                        }
                        if let Ok(mut out_file) = fs::File::create(&out_path) {
                            if let Err(e) = std::io::copy(&mut zip_entry, &mut out_file) {
                                log::warn!("Failed to extract resource {name}: {e}");
                            }
                        }
                    }
                }
            }
        }
    }
}

fn extract_mod_entries<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    target_dir: &Path,
) {
    for i in 0..archive.len() {
        if let Ok(mut zip_entry) = archive.by_index(i) {
            let name = zip_entry.name().to_string();
            if name.starts_with("mods/") && !zip_entry.is_dir() {
                let inner_name = &name[5..];
                if !inner_name.is_empty() {
                    let Ok(out_path) = safe_join_under(target_dir, Path::new(inner_name)) else {
                        log::warn!("Skipping mod entry outside target dir: {name}");
                        continue;
                    };
                    if let Some(parent) = out_path.parent() {
                        if let Err(e) = fs::create_dir_all(parent) {
                            log::warn!(
                                "Failed to create mods dir {dir}: {e}",
                                dir = parent.display()
                            );
                        }
                    }
                    if let Ok(mut out_file) = fs::File::create(&out_path) {
                        if let Err(e) = std::io::copy(&mut zip_entry, &mut out_file) {
                            log::warn!("Failed to extract mod {name}: {e}");
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn temp_root() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rtl_mods_extract_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        fs::remove_dir_all(&dir).ok();
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn build_malicious_zip() -> Vec<u8> {
        let cursor = std::io::Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(cursor);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        writer.start_file("mods/legit.jar", options).unwrap();
        writer.write_all(b"jar").unwrap();
        writer.start_file("mods/../../evil", options).unwrap();
        writer.write_all(b"evil").unwrap();
        writer.start_file("resources/legit.txt", options).unwrap();
        writer.write_all(b"text").unwrap();
        writer
            .start_file("resources/../../evil.sh", options)
            .unwrap();
        writer.write_all(b"evil").unwrap();
        writer
            .start_file("overrides/../../evil.sh", options)
            .unwrap();
        writer.write_all(b"evil").unwrap();

        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn extract_entries_reject_traversal() {
        let root = temp_root();
        let target_dir = root.join("instance");
        let resources_dest = root.join("resources");
        fs::create_dir_all(&target_dir).unwrap();
        fs::create_dir_all(&resources_dest).unwrap();

        let bytes = build_malicious_zip();
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();

        extract_mod_entries(&mut archive, &target_dir);
        extract_resource_entries(&mut archive, &resources_dest);

        assert!(target_dir.join("legit.jar").exists());
        assert!(resources_dest.join("legit.txt").exists());
        assert!(!root.join("evil").exists());
        assert!(!root.join("evil.sh").exists());

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn overrides_traversal_is_rejected_by_enclosed_name() {
        let bytes = build_malicious_zip();
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        for i in 0..archive.len() {
            let entry = archive.by_index(i).unwrap();
            if entry.name() == "overrides/../../evil.sh" {
                // download_modpack only writes overrides/ entries accepted by enclosed_name.
                assert!(entry.enclosed_name().is_none());
            }
        }
    }
}
