use crate::{
    AssetIndex, Component, Extract, LaunchError, Library, Requirement, Rule, RuleOs, VersionFile,
};
use reqwest::Client;

const VERSION_MANIFEST_URL: &str =
    "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json";
const FABRIC_META_URL: &str = "https://meta.fabricmc.net/v2";
const QUILT_META_URL: &str = "https://meta.quiltmc.org/v3";
const FORGE_MAVEN: &str = "https://files.minecraftforge.net/maven";
const NEOFORGE_MAVEN: &str = "https://maven.neoforged.net/releases";

#[derive(Debug, Clone)]
struct VersionManifestEntry {
    id: String,
    url: String,
    version_type: String,
}

#[derive(Debug)]
struct VersionManifest {
    versions: Vec<VersionManifestEntry>,
}

pub struct DependencyResolver {
    pub(crate) http: Client,
    manifest: Option<VersionManifest>,
}

impl Default for DependencyResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl DependencyResolver {
    #[must_use]
    pub fn new() -> Self {
        Self {
            http: Client::new(),
            manifest: None,
        }
    }

    /// # Errors
    /// Returns an error if the HTTP request or JSON parsing fails.
    pub async fn fetch_manifest(&mut self) -> Result<(), LaunchError> {
        let resp: serde_json::Value = self
            .http
            .get(VERSION_MANIFEST_URL)
            .send()
            .await?
            .json()
            .await?;

        let versions: Vec<VersionManifestEntry> =
            resp["versions"].as_array().map_or(vec![], |arr| {
                arr.iter()
                    .filter_map(|v| {
                        Some(VersionManifestEntry {
                            id: v["id"].as_str()?.to_string(),
                            url: v["url"].as_str()?.to_string(),
                            version_type: v["type"].as_str().unwrap_or("release").to_string(),
                        })
                    })
                    .collect()
            });

        self.manifest = Some(VersionManifest { versions });
        Ok(())
    }

    #[must_use]
    pub fn get_version_url(&self, version_id: &str) -> Option<String> {
        self.manifest.as_ref().and_then(|m| {
            m.versions
                .iter()
                .find(|v| v.id == version_id)
                .map(|v| v.url.clone())
        })
    }

    /// Returns all known version IDs from the manifest, if fetched.
    #[must_use]
    pub fn available_versions(&self) -> Vec<String> {
        self.manifest.as_ref().map_or_else(Vec::new, |m| {
            m.versions.iter().map(|v| v.id.clone()).collect()
        })
    }

    /// Returns all known version IDs with their Mojang type ("release", "snapshot", "old_beta", "old_alpha").
    #[must_use]
    pub fn available_versions_with_types(&self) -> Vec<(String, String)> {
        self.manifest.as_ref().map_or_else(Vec::new, |m| {
            m.versions
                .iter()
                .map(|v| (v.id.clone(), v.version_type.clone()))
                .collect()
        })
    }

    /// # Errors
    /// Returns an error if the HTTP request or JSON parsing fails.
    pub async fn fetch_version_metadata(&self, url: &str) -> Result<VersionFile, LaunchError> {
        let resp: serde_json::Value = self.http.get(url).send().await?.json().await?;
        Ok(parse_version_json(&resp))
    }

    /// # Errors
    /// Returns an error if the version is not found or the request fails.
    pub async fn fetch_vanilla_component(
        &self,
        version_id: &str,
    ) -> Result<Component, LaunchError> {
        let url = self
            .get_version_url(version_id)
            .ok_or_else(|| LaunchError::VersionNotFound(version_id.to_string()))?;
        let version_file = self.fetch_version_metadata(&url).await?;
        Ok(Component {
            uid: "net.minecraft".to_string(),
            version: version_id.to_string(),
            is_locked: true,
            dependencies: Vec::new(),
            conflicts: Vec::new(),
            version_file,
        })
    }

    /// # Errors
    /// Returns an error if the Fabric metadata request fails.
    pub async fn fetch_fabric_component(
        &self,
        mc_version: &str,
        loader_version: Option<&str>,
    ) -> Result<Component, LaunchError> {
        let loader_url = if let Some(lv) = loader_version {
            format!("{FABRIC_META_URL}/versions/loader/{mc_version}/{lv}/profile/json")
        } else {
            let versions: Vec<serde_json::Value> = self
                .http
                .get(format!("{FABRIC_META_URL}/versions/loader/{mc_version}"))
                .send()
                .await?
                .json()
                .await?;
            let latest = versions.last().ok_or_else(|| {
                LaunchError::VersionNotFound("No Fabric loader version found".into())
            })?;
            let loader_ver = latest
                .get("loader")
                .and_then(|v| v.get("version"))
                .and_then(|v| v.as_str())
                .unwrap_or("0.16.9");
            format!("{FABRIC_META_URL}/versions/loader/{mc_version}/{loader_ver}/profile/json")
        };

        let resp: serde_json::Value = self.http.get(&loader_url).send().await?.json().await?;
        let mut libraries = Vec::new();
        let mut main_class = None;

        if let Some(libs) = resp.get("libraries").and_then(|v| v.as_array()) {
            for lib in libs {
                libraries.extend(parse_library(lib));
            }
        }
        if let Some(mc) = resp.get("mainClass").and_then(|v| v.as_str()) {
            main_class = Some(mc.to_string());
        }

        let loader_ver = loader_version.unwrap_or("unknown");

        Ok(Component {
            uid: "net.fabricmc.fabric-loader".to_string(),
            version: loader_ver.to_string(),
            is_locked: true,
            dependencies: vec![
                Requirement {
                    uid: "net.minecraft".to_string(),
                    suggests: Some(mc_version.to_string()),
                    equals: Some(mc_version.to_string()),
                },
                Requirement {
                    uid: "net.fabricmc.intermediary".to_string(),
                    suggests: Some(mc_version.to_string()),
                    equals: None,
                },
            ],
            conflicts: vec![
                "net.neoforged".into(),
                "net.minecraftforge".into(),
                "org.quiltmc".into(),
            ],
            version_file: VersionFile {
                main_class,
                libraries,
                ..VersionFile::default()
            },
        })
    }

    /// # Errors
    /// Returns an error if the Forge metadata request fails.
    pub async fn fetch_forge_component(
        &self,
        mc_version: &str,
        forge_version: &str,
    ) -> Result<Component, LaunchError> {
        let full_ver = if forge_version.contains(mc_version) {
            forge_version.to_string()
        } else {
            format!("{mc_version}-{forge_version}")
        };

        let mut libraries = Vec::new();
        let mut main_class = None;
        let mut tweakers = Vec::new();
        let mut traits = vec!["legacyFML".to_string()];

        let meta_urls = vec![
            format!("https://meta.prismlauncher.org/v1/net.minecraftforge/{forge_version}.json"),
            format!("https://meta.prismlauncher.org/v1/net.minecraftforge/{full_ver}.json"),
            format!("{FORGE_MAVEN}/net/minecraftforge/forge/{full_ver}/forge-{full_ver}-install-profile.json"),
            format!("{FORGE_MAVEN}/net/minecraftforge/forge/{mc_version}/{forge_version}/forge-{mc_version}-install-profile.json"),
        ];

        for url in meta_urls {
            if let Ok(resp_res) = self.http.get(&url).send().await {
                if resp_res.status().is_success() {
                    if let Ok(resp) = resp_res.json::<serde_json::Value>().await {
                        if let Some(main) = resp.get("mainClass").and_then(|v| v.as_str()) {
                            main_class = Some(main.to_string());
                        } else if let Some(data) = resp.get("data") {
                            if let Some(mc_main) = data
                                .get("MINECRAFT_MAIN_CLASS")
                                .and_then(|v| v.get("client"))
                            {
                                if let Some(s) = mc_main.as_str() {
                                    main_class = Some(s.to_string());
                                }
                            }
                        }

                        if let Some(libs) = resp
                            .get("libraries")
                            .or_else(|| resp.get("versionInfo").and_then(|v| v.get("libraries")))
                            .and_then(|v| v.as_array())
                        {
                            for lib in libs {
                                libraries.extend(parse_library(lib));
                            }
                        }

                        if let Some(tweaks) = resp.get("+tweakers").and_then(|v| v.as_array()) {
                            for tw in tweaks {
                                if let Some(s) = tw.as_str() {
                                    tweakers.push(s.to_string());
                                }
                            }
                        }

                        if let Some(tr_arr) = resp.get("+traits").and_then(|v| v.as_array()) {
                            for tr in tr_arr {
                                if let Some(s) = tr.as_str() {
                                    if !traits.contains(&s.to_string()) {
                                        traits.push(s.to_string());
                                    }
                                }
                            }
                        }

                        if !libraries.is_empty() {
                            break;
                        }
                    }
                }
            }
        }

        let is_launchwrapper = main_class
            .as_deref()
            .unwrap_or("net.minecraft.launchwrapper.Launch")
            == "net.minecraft.launchwrapper.Launch"
            || traits.iter().any(|t| t == "legacyFML");

        if is_launchwrapper {
            if main_class.is_none() {
                main_class = Some("net.minecraft.launchwrapper.Launch".to_string());
            }

            if !libraries.iter().any(|l| l.name.contains("launchwrapper")) {
                libraries.push(crate::Library {
                    name: "net.minecraft:launchwrapper:1.12".to_string(),
                    url: Some(
                        "https://libraries.minecraft.net/net/minecraft/launchwrapper/1.12/launchwrapper-1.12.jar"
                            .to_string(),
                    ),
                    sha1: None,
                    size: None,
                    is_native: false,
                    rules: vec![],
                    extract: None,
                });
            }

            if !libraries.iter().any(|l| l.name.contains("asm-all")) {
                libraries.push(crate::Library {
                    name: "org.ow2.asm:asm-all:5.0.3".to_string(),
                    url: Some(
                        "https://libraries.minecraft.net/org/ow2/asm/asm-all/5.0.3/asm-all-5.0.3.jar"
                            .to_string(),
                    ),
                    sha1: None,
                    size: None,
                    is_native: false,
                    rules: vec![],
                    extract: None,
                });
            }
        }

        if !libraries
            .iter()
            .any(|l| l.name.contains("net.minecraftforge:forge"))
        {
            let forge_jar_url = format!(
                "{FORGE_MAVEN}/net/minecraftforge/forge/{full_ver}/forge-{full_ver}-universal.jar"
            );
            libraries.push(crate::Library {
                name: format!("net.minecraftforge:forge:{full_ver}"),
                url: Some(forge_jar_url),
                sha1: None,
                size: None,
                is_native: false,
                rules: vec![],
                extract: None,
            });
        }

        Ok(Component {
            uid: "net.minecraftforge".to_string(),
            version: forge_version.to_string(),
            is_locked: true,
            dependencies: vec![Requirement {
                uid: "net.minecraft".to_string(),
                suggests: Some(mc_version.to_string()),
                equals: Some(mc_version.to_string()),
            }],
            conflicts: vec![
                "net.neoforged".into(),
                "net.fabricmc.fabric-loader".into(),
                "org.quiltmc".into(),
            ],
            version_file: VersionFile {
                main_class,
                libraries,
                tweakers,
                traits,
                ..VersionFile::default()
            },
        })
    }

    /// # Errors
    /// Returns an error if the `NeoForge` metadata request fails.
    pub async fn fetch_neoforge_component(
        &self,
        mc_version: &str,
        neoforge_version: &str,
    ) -> Result<Component, LaunchError> {
        let url = format!(
            "{NEOFORGE_MAVEN}/net/neoforged/neoforge/{neoforge_version}/neoforge-{neoforge_version}-install-profile.json"
        );
        let resp: serde_json::Value = self.http.get(&url).send().await?.json().await?;
        let mut libraries = Vec::new();
        let mut main_class = None;

        if let Some(data) = resp.get("data") {
            if let Some(mc_main) = data
                .get("MINECRAFT_MAIN_CLASS")
                .and_then(|v| v.get("client"))
            {
                if let Some(s) = mc_main.as_str() {
                    main_class = Some(s.to_string());
                }
            }
        }
        if let Some(libs) = resp
            .get("versionInfo")
            .and_then(|v| v.get("libraries"))
            .and_then(|v| v.as_array())
        {
            for lib in libs {
                libraries.extend(parse_library(lib));
            }
        }

        Ok(Component {
            uid: "net.neoforged".to_string(),
            version: neoforge_version.to_string(),
            is_locked: true,
            dependencies: vec![Requirement {
                uid: "net.minecraft".to_string(),
                suggests: Some(mc_version.to_string()),
                equals: Some(mc_version.to_string()),
            }],
            conflicts: vec![
                "net.minecraftforge".into(),
                "net.fabricmc.fabric-loader".into(),
                "org.quiltmc".into(),
            ],
            version_file: VersionFile {
                main_class,
                libraries,
                ..VersionFile::default()
            },
        })
    }

    /// # Errors
    /// Returns an error if the Quilt metadata request fails.
    pub async fn fetch_quilt_component(
        &self,
        mc_version: &str,
        loader_version: Option<&str>,
    ) -> Result<Component, LaunchError> {
        let loader_url = if let Some(lv) = loader_version {
            format!("{QUILT_META_URL}/versions/loader/{mc_version}/{lv}/profile/json")
        } else {
            let versions: Vec<serde_json::Value> = self
                .http
                .get(format!("{QUILT_META_URL}/versions/loader/{mc_version}"))
                .send()
                .await?
                .json()
                .await?;
            let latest = versions.first().ok_or_else(|| {
                LaunchError::VersionNotFound("No Quilt loader version found".into())
            })?;
            let loader_ver = latest
                .get("loader")
                .and_then(|v| v.get("version"))
                .and_then(|v| v.as_str())
                .unwrap_or("0.26.13");
            format!("{QUILT_META_URL}/versions/loader/{mc_version}/{loader_ver}/profile/json")
        };

        let resp: serde_json::Value = self.http.get(&loader_url).send().await?.json().await?;
        let mut libraries = Vec::new();
        let mut main_class = None;

        if let Some(libs) = resp.get("libraries").and_then(|v| v.as_array()) {
            for lib in libs {
                libraries.extend(parse_library(lib));
            }
        }
        if let Some(mc) = resp.get("mainClass").and_then(|v| v.as_str()) {
            main_class = Some(mc.to_string());
        }

        let loader_ver = loader_version.unwrap_or("unknown");

        Ok(Component {
            uid: "org.quiltmc.quilt-loader".to_string(),
            version: loader_ver.to_string(),
            is_locked: true,
            dependencies: vec![
                Requirement {
                    uid: "net.minecraft".to_string(),
                    suggests: Some(mc_version.to_string()),
                    equals: Some(mc_version.to_string()),
                },
                Requirement {
                    uid: "org.quiltmc.quilt-intermediary".to_string(),
                    suggests: Some(mc_version.to_string()),
                    equals: None,
                },
            ],
            conflicts: vec![
                "net.neoforged".into(),
                "net.minecraftforge".into(),
                "net.fabricmc.fabric-loader".into(),
            ],
            version_file: VersionFile {
                main_class,
                libraries,
                ..VersionFile::default()
            },
        })
    }

    /// # Errors
    /// Returns an error if fetching metadata for the specified loader fails.
    pub async fn fetch_loader_versions(
        &self,
        loader_type: &str,
        mc_version: &str,
    ) -> Result<Vec<String>, LaunchError> {
        match loader_type.to_lowercase().as_str() {
            "fabric" => {
                let url = format!("{FABRIC_META_URL}/versions/loader/{mc_version}");
                let resp: Vec<serde_json::Value> = self.http.get(&url).send().await?.json().await?;
                let mut versions = Vec::new();
                for v in resp {
                    let is_stable = v
                        .get("loader")
                        .and_then(|l| l.get("stable"))
                        .and_then(|s| s.as_bool())
                        .unwrap_or(true);
                    if !is_stable {
                        continue;
                    }
                    if let Some(ver) = v
                        .get("loader")
                        .and_then(|l| l.get("version"))
                        .and_then(|s| s.as_str())
                    {
                        if !versions.contains(&ver.to_string()) {
                            versions.push(ver.to_string());
                        }
                    }
                }
                Ok(versions)
            }
            "quilt" => {
                let url = format!("{QUILT_META_URL}/versions/loader/{mc_version}");
                let resp: Vec<serde_json::Value> = self.http.get(&url).send().await?.json().await?;
                let mut versions = Vec::new();
                for v in resp {
                    let is_stable = v
                        .get("loader")
                        .and_then(|l| l.get("stable"))
                        .and_then(|s| s.as_bool())
                        .unwrap_or(true);
                    if !is_stable {
                        continue;
                    }
                    if let Some(ver) = v
                        .get("loader")
                        .and_then(|l| l.get("version"))
                        .and_then(|s| s.as_str())
                    {
                        if !versions.contains(&ver.to_string()) {
                            versions.push(ver.to_string());
                        }
                    }
                }
                Ok(versions)
            }
            "forge" => {
                let url = "https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml";
                let resp = self.http.get(url).send().await?.text().await?;
                let prefix = format!("<version>{mc_version}-");
                let mut versions = Vec::new();
                for line in resp.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with(&prefix) && trimmed.ends_with("</version>") {
                        let ver = trimmed
                            .strip_prefix(&prefix)
                            .and_then(|s| s.strip_suffix("</version>"))
                            .unwrap_or("");
                        if !ver.is_empty() && !versions.contains(&ver.to_string()) {
                            versions.push(ver.to_string());
                        }
                    }
                }
                versions.reverse();

                if versions.is_empty() {
                    if let Ok(promo_resp) = self
                        .http
                        .get("https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json")
                        .send()
                        .await
                    {
                        if let Ok(json) = promo_resp.json::<serde_json::Value>().await {
                            if let Some(promos) = json.get("promos").and_then(|p| p.as_object()) {
                                let key_latest = format!("{mc_version}-latest");
                                let key_rec = format!("{mc_version}-recommended");
                                if let Some(v) = promos.get(&key_rec).and_then(|v| v.as_str()) {
                                    if !versions.contains(&v.to_string()) {
                                        versions.push(v.to_string());
                                    }
                                }
                                if let Some(v) = promos.get(&key_latest).and_then(|v| v.as_str()) {
                                    if !versions.contains(&v.to_string()) {
                                        versions.push(v.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(versions)
            }
            "neoforge" => {
                let url = "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml";
                let resp = self.http.get(url).send().await?.text().await?;
                let neoforge_prefix = if let Some(stripped) = mc_version.strip_prefix("1.") {
                    let parts: Vec<&str> = stripped.split('.').collect();
                    if parts.len() >= 2 {
                        format!("{}.{}.", parts[0], parts[1])
                    } else if parts.len() == 1 {
                        format!("{}.0.", parts[0])
                    } else {
                        mc_version.to_string()
                    }
                } else {
                    mc_version.to_string()
                };

                let tag_prefix = format!("<version>{neoforge_prefix}");
                let mut versions = Vec::new();
                for line in resp.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with(&tag_prefix) && trimmed.ends_with("</version>") {
                        let ver = trimmed
                            .strip_prefix("<version>")
                            .and_then(|s| s.strip_suffix("</version>"))
                            .unwrap_or("");
                        if !ver.is_empty() && !versions.contains(&ver.to_string()) {
                            versions.push(ver.to_string());
                        }
                    }
                }
                versions.reverse();
                Ok(versions)
            }
            _ => Ok(Vec::new()),
        }
    }
}

/// # Errors
/// Returns an error if a dependency fetch fails.
pub async fn resolve_dependencies(
    resolver: &mut DependencyResolver,
    components: Vec<Component>,
) -> Result<Vec<Component>, LaunchError> {
    let mut resolved_deps: std::collections::HashMap<String, Component> =
        std::collections::HashMap::new();

    for component in components {
        resolved_deps.insert(component.uid.clone(), component);
    }

    for _ in 0..50 {
        let new_reqs: Vec<Requirement> = resolved_deps
            .values()
            .flat_map(|c| c.dependencies.clone())
            .filter(|req| !resolved_deps.contains_key(&req.uid))
            .collect();

        if new_reqs.is_empty() {
            break;
        }

        for req in new_reqs {
            if resolved_deps.contains_key(&req.uid) {
                continue;
            }

            let version = req
                .equals
                .clone()
                .or_else(|| req.suggests.clone())
                .unwrap_or_default();

            if req.uid == "net.fabricmc.intermediary" {
                let url = format!("{FABRIC_META_URL}/versions/intermediary/{version}");
                if let Ok(resp) = resolver.http.get(&url).send().await {
                    if let Ok(versions) = resp.json::<Vec<serde_json::Value>>().await {
                        if let Some(latest) = versions.first() {
                            let loader_version = latest
                                .get("version")
                                .and_then(|v| v.as_str())
                                .unwrap_or(&version)
                                .to_string();
                            resolved_deps.insert(
                                req.uid.clone(),
                                Component {
                                    uid: req.uid.clone(),
                                    version: loader_version,
                                    is_locked: false,
                                    dependencies: Vec::new(),
                                    conflicts: Vec::new(),
                                    version_file: VersionFile::default(),
                                },
                            );
                        }
                    }
                }
            }
        }
    }

    Ok(resolved_deps.into_values().collect())
}

fn parse_version_json(value: &serde_json::Value) -> VersionFile {
    let mut libraries = Vec::new();
    let mut tweakers = Vec::new();
    let mut jvm_args = Vec::new();

    if let Some(libs) = value.get("libraries").and_then(|v| v.as_array()) {
        for lib in libs {
            if let Some(tweaker) = lib.get("tweaker_class").and_then(|v| v.as_str()) {
                tweakers.push(tweaker.to_string());
            }
            libraries.extend(parse_library(lib));
        }
    }

    let main_class = value
        .get("mainClass")
        .and_then(|v| v.as_str())
        .map(ToString::to_string);
    let mut game_args = Vec::new();

    if let Some(minecraft_args_str) = value.get("minecraftArguments").and_then(|v| v.as_str()) {
        game_args.push(minecraft_args_str.to_string());
    } else if let Some(game) = value
        .get("arguments")
        .and_then(|v| v.get("game"))
        .and_then(|v| v.as_array())
    {
        for arg in game {
            parse_argument_item(arg, &mut game_args);
        }
    }

    if let Some(jvm) = value
        .get("arguments")
        .and_then(|v| v.get("jvm"))
        .and_then(|v| v.as_array())
    {
        for arg in jvm {
            parse_argument_item(arg, &mut jvm_args);
        }
    }

    let minecraft_args = if game_args.is_empty() {
        None
    } else {
        Some(game_args.join(" "))
    };

    let asset_index = value.get("assetIndex").map(|ai| AssetIndex {
        id: ai
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        url: ai
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        sha1: ai
            .get("sha1")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        size: ai
            .get("size")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
    });

    let client_download = value
        .get("downloads")
        .and_then(|d| d.get("client"))
        .map(|c| crate::ClientDownload {
            url: c
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            sha1: c
                .get("sha1")
                .and_then(|v| v.as_str())
                .map(ToString::to_string),
            size: c
                .get("size")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0),
        });

    let compatible_java_majors = value
        .get("javaVersion")
        .and_then(|jv| jv.get("majorVersion"))
        .and_then(serde_json::Value::as_u64)
        .map_or_else(|| vec![17, 21, 25], |v| vec![v as u32]);

    VersionFile {
        main_class,
        minecraft_args,
        jvm_args,
        libraries,
        tweakers,
        asset_index,
        client_download,
        compatible_java_majors,
        ..VersionFile::default()
    }
}

fn parse_library(lib: &serde_json::Value) -> Vec<Library> {
    let name = lib
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let artifact = lib.get("downloads").and_then(|d| d.get("artifact"));

    let url = lib
        .get("url")
        .and_then(|v| v.as_str())
        .or_else(|| artifact.and_then(|a| a.get("url")).and_then(|v| v.as_str()))
        .map(ToString::to_string);
    let sha1 = lib
        .get("sha1")
        .and_then(|v| v.as_str())
        .or_else(|| {
            artifact
                .and_then(|a| a.get("sha1"))
                .and_then(|v| v.as_str())
        })
        .map(ToString::to_string);
    let size = lib
        .get("size")
        .and_then(serde_json::Value::as_u64)
        .or_else(|| {
            artifact
                .and_then(|a| a.get("size"))
                .and_then(serde_json::Value::as_u64)
        });

    let rules: Vec<Rule> = lib
        .get("rules")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|r| Rule {
                    action: r
                        .get("action")
                        .and_then(|v| v.as_str())
                        .unwrap_or("allow")
                        .to_string(),
                    os: r.get("os").and_then(|v| v.as_object()).map(|o| RuleOs {
                        name: o
                            .get("name")
                            .and_then(|v| v.as_str())
                            .map(ToString::to_string),
                        arch: o
                            .get("arch")
                            .and_then(|v| v.as_str())
                            .map(ToString::to_string),
                    }),
                    features: r
                        .get("features")
                        .and_then(|v| v.as_object())
                        .map(|obj| {
                            obj.iter()
                                .map(|(k, v)| (k.clone(), v.as_bool().unwrap_or(false)))
                                .collect()
                        })
                        .unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default();

    let extract = lib
        .get("extract")
        .and_then(|v| v.as_object())
        .map(|e| Extract {
            exclude: e
                .get("exclude")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(ToString::to_string))
                        .collect()
                })
                .unwrap_or_default(),
        });

    let parts: Vec<&str> = name.split(':').collect();

    // Old format: "natives" field + "downloads.classifiers"
    if let Some(natives) = lib.get("natives").and_then(|v| v.as_object()) {
        let os = crate::platform::current_os();
        if let Some(classifier) = natives.get(os).and_then(|v| v.as_str()) {
            let classifier = classifier.replace("${arch}", crate::platform::current_arch());
            let native_name = format!("{name}:{classifier}");
            let (native_url, native_sha1, native_size) =
                if let Some(classifiers) = lib.get("downloads").and_then(|d| d.get("classifiers"))
                {
                    if let Some(class_info) = classifiers.get(&classifier) {
                        (
                            class_info
                                .get("url")
                                .and_then(|v| v.as_str())
                                .map(ToString::to_string),
                            class_info
                                .get("sha1")
                                .and_then(|v| v.as_str())
                                .map(ToString::to_string),
                            class_info
                                .get("size")
                                .and_then(serde_json::Value::as_u64),
                        )
                    } else {
                        (url.clone(), sha1.clone(), size)
                    }
                } else {
                    (url.clone(), sha1.clone(), size)
                };
            return vec![
                Library {
                    name: name.clone(),
                    url: url.clone(),
                    sha1: sha1.clone(),
                    size,
                    is_native: false,
                    rules: rules.clone(),
                    extract: extract.clone(),
                },
                Library {
                    name: native_name,
                    url: native_url,
                    sha1: native_sha1,
                    size: native_size,
                    is_native: true,
                    rules,
                    extract,
                },
            ];
        }
    }

    // New format: separate entry with "natives-" classifier in the name
    if parts.len() >= 4 && parts[3].starts_with("natives-") {
        return vec![Library {
            name,
            url,
            sha1,
            size,
            is_native: true,
            rules,
            extract,
        }];
    }

    vec![Library {
        name,
        url,
        sha1,
        size,
        is_native: false,
        rules,
        extract,
    }]
}

fn parse_argument_item(item: &serde_json::Value, target: &mut Vec<String>) {
    if let Some(s) = item.as_str() {
        target.push(s.to_string());
    } else if let Some(obj) = item.as_object() {
        let rules: Vec<Rule> = obj
            .get("rules")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|r| Rule {
                        action: r
                            .get("action")
                            .and_then(|v| v.as_str())
                            .unwrap_or("allow")
                            .to_string(),
                        os: r.get("os").and_then(|v| v.as_object()).map(|o| RuleOs {
                            name: o
                                .get("name")
                                .and_then(|v| v.as_str())
                                .map(ToString::to_string),
                            arch: o
                                .get("arch")
                                .and_then(|v| v.as_str())
                                .map(ToString::to_string),
                        }),
                        features: r
                            .get("features")
                            .and_then(|v| v.as_object())
                            .map(|obj| {
                                obj.iter()
                                    .map(|(k, v)| (k.clone(), v.as_bool().unwrap_or(false)))
                                    .collect()
                            })
                            .unwrap_or_default(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        if crate::platform::should_include(&rules) {
            if let Some(val) = obj.get("value") {
                if let Some(s) = val.as_str() {
                    target.push(s.to_string());
                } else if let Some(arr) = val.as_array() {
                    for elem in arr {
                        if let Some(s) = elem.as_str() {
                            target.push(s.to_string());
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::profile::assemble_launch_profile;
    use crate::{Component, Library, Rule, RuleOs, VersionFile};
    use super::*;

    #[test]
    fn parse_library_new_format_native() {
        let json = serde_json::json!({
            "downloads": {
                "artifact": {
                    "path": "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar",
                    "sha1": "a5ed18a2b82fc91b81f40d717cb1f64c9dcb0540",
                    "size": 165442,
                    "url": "https://libraries.minecraft.net/org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar"
                }
            },
            "name": "org.lwjgl:lwjgl:3.3.3:natives-windows",
            "rules": [{"action": "allow", "os": {"name": "windows"}}]
        });

        let result = parse_library(&json);
        assert_eq!(result.len(), 1);
        assert!(result[0].is_native);
        assert_eq!(result[0].name, "org.lwjgl:lwjgl:3.3.3:natives-windows");
        assert!(result[0].url.is_some());
        eprintln!("Test OK: native library parsed: name={} url={:?}", result[0].name, result[0].url);
    }

    #[test]
    fn parse_library_new_format_regular() {
        let json = serde_json::json!({
            "downloads": {
                "artifact": {
                    "path": "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar",
                    "sha1": "29589b5f87ed335a6c7e7ee6a5775f81f97ecb84",
                    "size": 785029,
                    "url": "https://libraries.minecraft.net/org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar"
                }
            },
            "name": "org.lwjgl:lwjgl:3.3.3"
        });

        let result = parse_library(&json);
        assert_eq!(result.len(), 1);
        assert!(!result[0].is_native);
        eprintln!("Test OK: regular library parsed: name={}", result[0].name);
    }

    #[test]
    fn parse_version_json_1206() {
        let json = serde_json::json!({
            "libraries": [
                {
                    "downloads": {
                        "artifact": {
                            "path": "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar",
                            "sha1": "29589b5f87ed335a6c7e7ee6a5775f81f97ecb84",
                            "size": 785029,
                            "url": "https://libraries.minecraft.net/org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar"
                        }
                    },
                    "name": "org.lwjgl:lwjgl:3.3.3"
                },
                {
                    "downloads": {
                        "artifact": {
                            "path": "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar",
                            "sha1": "a5ed18a2b82fc91b81f40d717cb1f64c9dcb0540",
                            "size": 165442,
                            "url": "https://libraries.minecraft.net/org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar"
                        }
                    },
                    "name": "org.lwjgl:lwjgl:3.3.3:natives-windows",
                    "rules": [{"action": "allow", "os": {"name": "windows"}}]
                }
            ]
        });

        let vf = parse_version_json(&json);
        let natives: Vec<&Library> = vf.libraries.iter().filter(|l| l.is_native).collect();
        assert_eq!(natives.len(), 1, "Expected 1 native library, got {}", natives.len());
        for l in &natives {
            eprintln!("Found native: name={} url={:?}", l.name, l.url);
        }
        assert_eq!(natives[0].name, "org.lwjgl:lwjgl:3.3.3:natives-windows");
        assert!(natives[0].url.is_some());
        assert!(natives[0].url.as_ref().unwrap().contains("natives-windows.jar"));
    }

    #[test]
    fn assemble_profile_with_natives() {
        // Simulate what parse_version_json produces for a 1.20.6-style JSON
        let native_lib = Library {
            name: "org.lwjgl:lwjgl:3.3.3:natives-windows".to_string(),
            url: Some("https://libraries.minecraft.net/org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar".to_string()),
            sha1: Some("a5ed18a2b82fc91b81f40d717cb1f64c9dcb0540".to_string()),
            size: Some(165442),
            is_native: true,
            rules: vec![Rule {
                action: "allow".to_string(),
                os: Some(RuleOs {
                    name: Some("windows".to_string()),
                    arch: None,
                }),
                features: std::collections::HashMap::new(),
            }],
            extract: None,
        };
        let regular_lib = Library {
            name: "org.lwjgl:lwjgl:3.3.3".to_string(),
            url: Some("https://libraries.minecraft.net/org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar".to_string()),
            sha1: Some("29589b5f87ed335a6c7e7ee6a5775f81f97ecb84".to_string()),
            size: Some(785029),
            is_native: false,
            rules: vec![],
            extract: None,
        };
        let vf = VersionFile {
            libraries: vec![regular_lib, native_lib],
            ..VersionFile::default()
        };
        let component = Component {
            uid: "net.minecraft".to_string(),
            version: "1.20.6".to_string(),
            is_locked: true,
            dependencies: vec![],
            conflicts: vec![],
            version_file: vf,
        };
        let profile = assemble_launch_profile(&[component]).unwrap();
        eprintln!("libraries: {}, native_libraries: {}", profile.libraries.len(), profile.native_libraries.len());
        assert!(!profile.native_libraries.is_empty(), "native_libraries should NOT be empty!");
        assert_eq!(profile.native_libraries.len(), 1);
        assert!(profile.native_libraries[0].name.contains("natives-windows"));
    }

    #[test]
    fn parse_library_old_format() {
        let json = serde_json::json!({
            "name": "org.lwjgl:lwjgl:3.3.3",
            "natives": {
                "windows": "natives-windows",
                "linux": "natives-linux",
                "osx": "natives-osx"
            },
            "downloads": {
                "artifact": {
                    "url": "https://libraries.minecraft.net/org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar"
                },
                "classifiers": {
                    "natives-windows": {
                        "url": "https://libraries.minecraft.net/org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar"
                    }
                }
            }
        });

        let result = parse_library(&json);
        assert_eq!(result.len(), 2);
        assert!(!result[0].is_native);
        assert!(result[1].is_native);
        eprintln!("Test OK: old format parsed: {:?} + native {:?}", result[0].name, result[1].name);
    }

    #[tokio::test]
    async fn test_fetch_loader_versions() {
        let resolver = DependencyResolver::new();
        let fabric_versions = resolver.fetch_loader_versions("fabric", "1.20.1").await.unwrap();
        assert!(!fabric_versions.is_empty(), "Fabric versions should not be empty for 1.20.1");

        let forge_versions_1_20_1 = resolver.fetch_loader_versions("forge", "1.20.1").await.unwrap();
        assert!(!forge_versions_1_20_1.is_empty(), "Forge versions should not be empty for 1.20.1");
        assert!(forge_versions_1_20_1.contains(&"47.4.22".to_string()) || forge_versions_1_20_1.iter().any(|v| v.starts_with("47.")));

        let neoforge_versions_1_20_4 = resolver.fetch_loader_versions("neoforge", "1.20.4").await.unwrap();
        assert!(!neoforge_versions_1_20_4.is_empty(), "NeoForge versions should not be empty for 1.20.4");
        assert!(neoforge_versions_1_20_4.iter().all(|v| v.starts_with("20.4.")));
    }
}
