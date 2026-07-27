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
                libraries.push(parse_library(lib));
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
        let url = format!(
            "{FORGE_MAVEN}/net/minecraftforge/forge/{mc_version}/{forge_version}/forge-{mc_version}-install-profile.json"
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
                libraries.push(parse_library(lib));
            }
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
                traits: vec!["legacyFML".to_string()],
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
                libraries.push(parse_library(lib));
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
                libraries.push(parse_library(lib));
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
            libraries.push(parse_library(lib));
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
        .map_or_else(|| vec![17, 21], |v| vec![v as u32]);

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

fn parse_library(lib: &serde_json::Value) -> Library {
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
    let is_native = lib.get("natives").is_some();

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

    Library {
        name,
        url,
        sha1,
        size,
        is_native,
        rules,
        extract,
    }
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
