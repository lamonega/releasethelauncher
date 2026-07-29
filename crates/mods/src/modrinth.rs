use futures::TryStreamExt;
use reqwest::Client;
use sha1::Digest as _;
use std::fmt::Write;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use super::modrinth_types::{ModrinthProject, ModrinthVersion, SearchResponse};
use crate::{
    InstalledMod, ModProvider, ModUpdate, ModVersion, ModsError, ProjectInfo, ProjectSummary,
    ReleaseType, SearchArgs, SearchResults, Side, SortOrder,
};

const BASE_URL: &str = "https://api.modrinth.com/v2";

enum HasherChoice {
    Sha1(sha1::Sha1),
    Sha2(sha2::Sha256),
    Sha512(sha2::Sha512),
}

impl HasherChoice {
    fn update(&mut self, data: &[u8]) {
        match self {
            Self::Sha1(h) => h.update(data),
            Self::Sha2(h) => h.update(data),
            Self::Sha512(h) => h.update(data),
        }
    }

    fn finalize_hex(&mut self) -> String {
        match self {
            Self::Sha1(h) => hex::encode(h.clone().finalize()),
            Self::Sha2(h) => hex::encode(h.clone().finalize()),
            Self::Sha512(h) => hex::encode(h.clone().finalize()),
        }
    }
}

pub struct ModrinthProvider {
    http: Client,
    api_token: Option<String>,
}

impl ModrinthProvider {
    #[must_use]
    pub fn new(api_token: Option<String>) -> Self {
        Self {
            http: Client::new(),
            api_token,
        }
    }

    fn build_headers(&self) -> Vec<(&str, &str)> {
        let mut headers = vec![("User-Agent", "release-the-launcher/0.1.0")];
        if let Some(ref token) = self.api_token {
            headers.push(("Authorization", token));
        }
        headers
    }

    fn build_search_url(args: &SearchArgs) -> String {
        let facets = Self::build_facets(args);
        let mut url = format!(
            "{BASE_URL}/search?limit={}&offset={}",
            args.limit, args.offset
        );
        if !args.query.is_empty() {
            let _ = write!(url, "&query={}", urlencoding::encode(&args.query));
        }
        if !facets.is_empty() {
            let _ = write!(url, "&facets={}", urlencoding::encode(&facets));
        }
        if args.sort != SortOrder::Relevance {
            let _ = write!(url, "&index={}", args.sort.as_str());
        }
        url
    }

    fn build_facets(args: &SearchArgs) -> String {
        Self::build_facets_with_type(args, "mod")
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

    fn build_search_url_with_type(args: &SearchArgs, project_type: &str) -> String {
        let facets = Self::build_facets_with_type(args, project_type);
        let mut url = format!(
            "{BASE_URL}/search?limit={}&offset={}",
            args.limit, args.offset
        );
        if !args.query.is_empty() {
            let _ = write!(url, "&query={}", urlencoding::encode(&args.query));
        }
        if !facets.is_empty() {
            let _ = write!(url, "&facets={}", urlencoding::encode(&facets));
        }
        if args.sort != SortOrder::Relevance {
            let _ = write!(url, "&index={}", args.sort.as_str());
        }
        url
    }

    /// Search for modpacks on Modrinth.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request or JSON parsing fails.
    pub async fn search_modpacks(&self, args: &SearchArgs) -> Result<SearchResults, ModsError> {
        let url = Self::build_search_url_with_type(args, "modpack");
        let mut req = self.http.get(&url);
        for (k, v) in self.build_headers() {
            req = req.header(k, v);
        }
        let resp: SearchResponse = req.send().await?.json().await?;

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

        Ok(SearchResults {
            hits,
            total_hits: resp.total_hits,
        })
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

        let mut req = self.http.get(url.as_str());
        for (k, v) in self.build_headers() {
            req = req.header(k, v);
        }

        let response = req.send().await?;
        let stream = response.bytes_stream();

        let zip_path = target_dir.join(&version.filename);
        fs::create_dir_all(target_dir)?;

        // Stream to a temp file, then rename
        let tmp_path = zip_path.with_extension("tmp");
        {
            let mut file = fs::File::create(&tmp_path)?;
            let mut stream = stream;
            while let Some(chunk) = stream.try_next().await? {
                file.write_all(&chunk)?;
            }
        }
        fs::rename(&tmp_path, &zip_path)?;

        let file = fs::File::open(&zip_path)?;
        let mut archive = zip::ZipArchive::new(file)?;

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let entry_path = entry.mangled_name();
            let name_str = entry_path.to_string_lossy();

            if name_str == "modrinth.index.json" {
                let out_path = target_dir.join("modrinth.index.json");
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
                let out_path = target_dir.join(".minecraft").join(rel);
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
    pub async fn download_modpack_files(
        &self,
        target_dir: &Path,
        progress: impl Fn(u64, u64, &str) + Send + Sync + 'static,
    ) -> Result<(), ModsError> {
        let index_path = target_dir.join("modrinth.index.json");
        if !index_path.exists() {
            return Ok(());
        }

        let content = fs::read_to_string(&index_path)?;
        let index: serde_json::Value = serde_json::from_str(&content)?;

        let Some(files) = index.get("files").and_then(|f| f.as_array()) else {
            return Ok(());
        };

        if files.is_empty() {
            return Ok(());
        }

        // Pre-scan: sum sizes of existing and missing files
        let mut initial_downloaded: u64 = 0;
        let mut total_bytes: u64 = 0;

        for file_obj in files {
            let size = file_obj.get("file_size").and_then(|v| v.as_u64()).unwrap_or(0);
            total_bytes += size;

            if let Some(path_str) = file_obj.get("path").and_then(|p| p.as_str()) {
                let dest = target_dir.join(".minecraft").join(path_str);
                if dest.exists() && dest.metadata().is_ok_and(|m| m.len() > 0) {
                    initial_downloaded += size;
                }
            }
        }

        let total_b = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(total_bytes));
        let downloaded_b = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(initial_downloaded));
        let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(16));
        let progress_cb = std::sync::Arc::new(progress);
        let mut tasks = Vec::new();
        let client = self.http.clone();

        for file_obj in files {
            let downloads = file_obj
                .get("downloads")
                .and_then(|d| d.get(0))
                .and_then(|u| u.as_str())
                .map(ToString::to_string);
            let rel_path = file_obj
                .get("path")
                .and_then(|p| p.as_str())
                .map(ToString::to_string);
            let size = file_obj.get("file_size").and_then(|v| v.as_u64()).unwrap_or(0);

            if let (Some(url), Some(path_str)) = (downloads, rel_path) {
                let dest = target_dir.join(".minecraft").join(&path_str);
                let display_name = Path::new(&path_str)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or(path_str);

                let sem = sem.clone();
                let client = client.clone();
                let downloaded_cnt = downloaded_b.clone();
                let total_cnt = total_b.clone();
                let progress_ref = progress_cb.clone();

                tasks.push(tokio::spawn(async move {
                    if !dest.exists() || dest.metadata().map_or(true, |m| m.len() == 0) {
                        let _permit = sem.acquire().await.unwrap();
                        if let Ok(resp) = client.get(&url).send().await {
                            if resp.status().is_success() {
                                if let Ok(bytes) = resp.bytes().await {
                                    if let Some(parent) = dest.parent() {
                                        let _ = fs::create_dir_all(parent);
                                    }
                                    let tmp = dest.with_extension("tmp");
                                    let _ = fs::write(&tmp, &bytes);
                                    let _ = fs::rename(&tmp, &dest);
                                }
                            }
                        }
                    }

                    downloaded_cnt.fetch_add(size, std::sync::atomic::Ordering::SeqCst);
                    let cur = downloaded_cnt.load(std::sync::atomic::Ordering::SeqCst);
                    let tot = total_cnt.load(std::sync::atomic::Ordering::SeqCst);
                    progress_ref(cur, tot.max(cur), &display_name);
                }));
            }
        }

        for task in tasks {
            let _ = task.await;
        }

        Ok(())
    }

    /// Download a modpack and extract it to create a new instance.
    /// Returns (`instance_name`, `mc_version`, `loader_from_manifest`).
    ///
    /// # Errors
    ///
    /// Returns an error if the version lookup, download, or manifest parsing fails.
    pub async fn install_modpack_as_instance(
        &self,
        project_id: &str,
        version_id: Option<&str>,
        target_base_dir: &Path,
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
        let instance_dir = target_base_dir.join(&instance_name);
        fs::create_dir_all(&instance_dir)?;

        self.download_modpack(version, &instance_dir).await?;

        let index_path = instance_dir.join("modrinth.index.json");
        let (mc_version, loader) = if index_path.exists() {
            let content = fs::read_to_string(&index_path)?;
            let index: serde_json::Value = serde_json::from_str(&content)?;

            let fallback_mc = version
                .mc_versions
                .first()
                .cloned()
                .unwrap_or_else(|| "1.21.1".to_string());

            let mc_ver = index
                .get("dependencies")
                .and_then(|d| d.get("minecraft"))
                .and_then(|v| v.as_str())
                .map(ToString::to_string)
                .unwrap_or(fallback_mc);

            let loader = if index
                .get("dependencies")
                .and_then(|d| d.get("fabric-loader"))
                .is_some()
            {
                let loader_ver = index["dependencies"]["fabric-loader"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                format!("Fabric:{loader_ver}")
            } else if index
                .get("dependencies")
                .and_then(|d| d.get("forge"))
                .is_some()
            {
                let loader_ver = index["dependencies"]["forge"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                format!("Forge:{loader_ver}")
            } else if index
                .get("dependencies")
                .and_then(|d| d.get("neoforge"))
                .is_some()
            {
                let loader_ver = index["dependencies"]["neoforge"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                format!("NeoForge:{loader_ver}")
            } else if index
                .get("dependencies")
                .and_then(|d| d.get("quilt-loader"))
                .is_some()
            {
                let loader_ver = index["dependencies"]["quilt-loader"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                format!("Quilt:{loader_ver}")
            } else {
                "Vanilla".to_string()
            };

            let mc_dir = instance_dir.join(".minecraft");
            fs::create_dir_all(&mc_dir)?;

            (mc_ver, loader)
        } else {
            let fallback_mc = version
                .mc_versions
                .first()
                .cloned()
                .unwrap_or_else(|| "1.21.1".to_string());
            (fallback_mc, "Vanilla".to_string())
        };

        Ok((instance_name, mc_version, loader))
    }

    /// Search with pagination support, filtering by project type.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request or JSON parsing fails.
    pub async fn search_page(
        &self,
        args: &SearchArgs,
        project_type: &str,
        offset: usize,
        limit: usize,
    ) -> Result<SearchResults, ModsError> {
        let mut paged_args = args.clone();
        paged_args.offset = offset;
        paged_args.limit = limit;
        let url = Self::build_search_url_with_type(&paged_args, project_type);
        let mut req = self.http.get(&url);
        for (k, v) in self.build_headers() {
            req = req.header(k, v);
        }
        let resp: SearchResponse = req.send().await?.json().await?;
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
        Ok(SearchResults {
            hits,
            total_hits: resp.total_hits,
        })
    }
}

#[async_trait::async_trait]
impl ModProvider for ModrinthProvider {
    fn name(&self) -> &'static str {
        "Modrinth"
    }

    async fn search(&self, args: SearchArgs) -> Result<SearchResults, ModsError> {
        let url = Self::build_search_url(&args);
        let mut req = self.http.get(&url);
        for (k, v) in self.build_headers() {
            req = req.header(k, v);
        }
        let resp: SearchResponse = req.send().await?.json().await?;

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

        Ok(SearchResults {
            hits,
            total_hits: resp.total_hits,
        })
    }

    async fn get_versions(
        &self,
        project_id: &str,
        mc_versions: &[String],
        loaders: &[String],
    ) -> Result<Vec<ModVersion>, ModsError> {
        let mut url = format!("{BASE_URL}/project/{project_id}/version?");
        if !mc_versions.is_empty() {
            let versions_json = serde_json::to_string(mc_versions).unwrap_or_default();
            let _ = write!(url, "game_versions={}", urlencoding::encode(&versions_json));
        }
        if !loaders.is_empty() {
            let loaders_json = serde_json::to_string(loaders).unwrap_or_default();
            if url.contains('?') {
                url.push('&');
            }
            let _ = write!(url, "loaders={}", urlencoding::encode(&loaders_json));
        }

        let mut req = self.http.get(&url);
        for (k, v) in self.build_headers() {
            req = req.header(k, v);
        }
        let resp: Vec<ModrinthVersion> = req.send().await?.json().await?;

        let versions = resp
            .into_iter()
            .map(|v| {
                let (hash, hash_type): (Option<String>, Option<String>) = v
                    .files
                    .first()
                    .and_then(|f| {
                        if let Some((algo, h)) = f.hashes.iter().next() {
                            Some((Some(h.clone()), Some(algo.clone())))
                        } else {
                            None
                        }
                    })
                    .unwrap_or((None, None));

                let primary_file = v
                    .files
                    .iter()
                    .find(|f| f.primary)
                    .or_else(|| v.files.first());

                let filename = primary_file.map(|f| f.filename.clone()).unwrap_or_default();

                let download_url = primary_file.and_then(|f| f.url.clone());

                let file_size = primary_file.map_or(0, |f| f.size);

                ModVersion {
                    id: v.id,
                    project_id: v.project_id,
                    name: v.name,
                    version_number: v.version_number,
                    release_type: match v.version_type.as_str() {
                        "beta" => ReleaseType::Beta,
                        "alpha" => ReleaseType::Alpha,
                        _ => ReleaseType::Release,
                    },
                    mc_versions: v.game_versions,
                    loaders: v.loaders,
                    download_url,
                    filename,
                    hash,
                    hash_type,
                    file_size,
                }
            })
            .collect();

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
        let mut updates = Vec::new();

        for installed_mod in installed {
            let algorithm = if installed_mod.hash_type == "sha512" {
                "sha512"
            } else {
                "sha1"
            };
            let url = format!(
                "{}/version_file/{}/update?algorithm={}",
                BASE_URL, installed_mod.hash, algorithm
            );

            let body = serde_json::json!({
                "loaders": loaders,
                "game_versions": mc_versions,
            });

            let mut req = self.http.post(&url).json(&body);
            for (k, v) in self.build_headers() {
                req = req.header(k, v);
            }

            if let Ok(resp) = req.send().await {
                if resp.status().is_success() {
                    if let Ok(version) = resp.json::<ModrinthVersion>().await {
                        let (hash, hash_type): (Option<String>, Option<String>) = version
                            .files
                            .first()
                            .and_then(|f| f.hashes.iter().next())
                            .map_or((None, None), |(a, h)| (Some(h.clone()), Some(a.clone())));

                        let primary = version
                            .files
                            .iter()
                            .find(|f| f.primary)
                            .or_else(|| version.files.first());

                        let latest = ModVersion {
                            id: version.id,
                            project_id: version.project_id,
                            name: version.name,
                            version_number: version.version_number,
                            release_type: match version.version_type.as_str() {
                                "beta" => ReleaseType::Beta,
                                "alpha" => ReleaseType::Alpha,
                                _ => ReleaseType::Release,
                            },
                            mc_versions: version.game_versions,
                            loaders: version.loaders,
                            download_url: primary.and_then(|f| f.url.clone()),
                            filename: primary.map(|f| f.filename.clone()).unwrap_or_default(),
                            hash,
                            hash_type,
                            file_size: primary.map_or(0, |f| f.size),
                        };

                        if latest.hash.as_deref() != Some(installed_mod.hash.as_str()) {
                            updates.push(ModUpdate {
                                installed: installed_mod.clone(),
                                latest,
                            });
                        }
                    }
                }
            }
        }

        Ok(updates)
    }

    async fn download_mod(
        &self,
        version: &ModVersion,
        target_dir: &Path,
    ) -> Result<std::path::PathBuf, ModsError> {
        let url = version
            .download_url
            .as_ref()
            .ok_or_else(|| ModsError::Provider("No download URL".into()))?;

        let mut req = self.http.get(url);
        for (k, v) in self.build_headers() {
            req = req.header(k, v);
        }

        let response = req.send().await?;
        let stream = response.bytes_stream();

        fs::create_dir_all(target_dir)?;
        let path = target_dir.join(&version.filename);
        let tmp_path = path.with_extension("tmp");

        let mut file = fs::File::create(&tmp_path)?;

        let mut hasher =
            version
                .hash_type
                .as_ref()
                .and_then(|hash_type| match hash_type.as_str() {
                    "sha1" => Some(HasherChoice::Sha1(sha1::Sha1::new())),
                    "sha256" => Some(HasherChoice::Sha2(sha2::Sha256::new())),
                    "sha512" => Some(HasherChoice::Sha512(sha2::Sha512::new())),
                    _ => None,
                });

        let mut stream = stream;
        while let Some(chunk) = stream.try_next().await? {
            if let Some(ref mut h) = hasher {
                h.update(&chunk);
            }
            file.write_all(&chunk)?;
        }
        drop(file);

        if let Some(ref expected) = version.hash {
            if let Some(ref mut h) = hasher {
                let computed = h.finalize_hex();
                if computed != *expected {
                    let _ = fs::remove_file(&tmp_path);
                    return Err(ModsError::Provider(format!(
                        "Checksum mismatch for {}: expected {expected}, got {computed}",
                        version.filename
                    )));
                }
            }
        }

        fs::rename(&tmp_path, &path)?;
        Ok(path)
    }
}
