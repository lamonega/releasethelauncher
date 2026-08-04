pub(crate) mod fabric;
pub(crate) mod forge;
pub(crate) mod loader;
pub(crate) mod neoforge;
pub(crate) mod parsers;
pub(crate) mod prism;
pub(crate) mod quilt;

use crate::{Component, LaunchError, Requirement, VersionFile};
use reqwest::Client;

use release_the_launcher_constants::urls;

pub struct DependencyResolver {
    pub(crate) http: Client,
    manifest: Option<prism::VersionManifest>,
}

impl Default for DependencyResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl DependencyResolver {
    #[must_use]
    pub fn new() -> Self {
        Self::with_client(release_the_launcher_net::default_client())
    }

    #[must_use]
    pub(crate) const fn with_client(http: Client) -> Self {
        Self {
            http,
            manifest: None,
        }
    }

    /// # Errors
    /// Returns an error if the HTTP request or JSON parsing fails.
    pub async fn fetch_manifest(&mut self) -> Result<(), LaunchError> {
        let manifest = prism::fetch_manifest(&self.http).await?;
        self.manifest = Some(manifest);
        Ok(())
    }

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
        prism::fetch_version_metadata(&self.http, url).await
    }

    /// # Errors
    /// Returns an error if the version is not found or the request fails.
    pub async fn fetch_vanilla_component(
        &self,
        version_id: &str,
    ) -> Result<Component, LaunchError> {
        prism::fetch_vanilla_component(&self.http, self.manifest.as_ref(), version_id).await
    }

    /// # Errors
    /// Returns an error if the Fabric metadata request fails.
    pub async fn fetch_fabric_component(
        &self,
        mc_version: &str,
        loader_version: Option<&str>,
    ) -> Result<Component, LaunchError> {
        fabric::fetch_fabric_component(&self.http, mc_version, loader_version).await
    }

    /// # Errors
    /// Returns an error if the Forge metadata request fails.
    pub async fn fetch_forge_component(
        &self,
        mc_version: &str,
        forge_version: &str,
    ) -> Result<Component, LaunchError> {
        forge::fetch_forge_component(&self.http, mc_version, forge_version).await
    }

    /// # Errors
    /// Returns an error if the `NeoForge` metadata request fails.
    pub async fn fetch_neoforge_component(
        &self,
        mc_version: &str,
        neoforge_version: &str,
    ) -> Result<Component, LaunchError> {
        neoforge::fetch_neoforge_component(&self.http, mc_version, neoforge_version).await
    }

    /// # Errors
    /// Returns an error if the Quilt metadata request fails.
    pub async fn fetch_quilt_component(
        &self,
        mc_version: &str,
        loader_version: Option<&str>,
    ) -> Result<Component, LaunchError> {
        quilt::fetch_quilt_component(&self.http, mc_version, loader_version).await
    }

    /// # Errors
    /// Returns an error if fetching metadata for the specified loader fails.
    pub async fn fetch_loader_versions(
        &self,
        loader_type: &str,
        mc_version: &str,
    ) -> Result<Vec<String>, LaunchError> {
        match loader_type.to_lowercase().as_str() {
            "fabric" => fabric::fetch_fabric_loader_versions(&self.http, mc_version).await,
            "quilt" => quilt::fetch_quilt_loader_versions(&self.http, mc_version).await,
            "forge" => forge::fetch_forge_loader_versions(&self.http, mc_version).await,
            "neoforge" => neoforge::fetch_neoforge_loader_versions(&self.http, mc_version).await,
            _ => Ok(Vec::new()),
        }
    }
}

use std::collections::{HashMap, HashSet, VecDeque};

/// # Errors
/// Returns an error if a dependency fetch fails or a conflict is detected.
pub async fn resolve_dependencies(
    resolver: &mut DependencyResolver,
    components: Vec<Component>,
) -> Result<Vec<Component>, LaunchError> {
    let mut resolved_deps: HashMap<String, Component> = HashMap::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<Requirement> = VecDeque::new();

    for component in components {
        visited.insert(component.uid.clone());
        for req in &component.dependencies {
            queue.push_back(req.clone());
        }
        resolved_deps.insert(component.uid.clone(), component);
    }

    while let Some(req) = queue.pop_front() {
        if visited.contains(&req.uid) || resolved_deps.contains_key(&req.uid) {
            continue;
        }
        visited.insert(req.uid.clone());

        let version = req
            .equals
            .clone()
            .or_else(|| req.suggests.clone())
            .unwrap_or_default();

        let url = if version.is_empty() {
            format!("{}/{}/index.json", urls::PRISM_META_BASE, req.uid)
        } else {
            format!("{}/{}/{version}.json", urls::PRISM_META_BASE, req.uid)
        };

        let resp: serde_json::Value = resolver.http.get(&url).send().await?.json().await?;
        let version_file = parsers::parse_version_json(&resp);
        let dependencies = parsers::parse_requires(&resp);
        let actual_version = resp
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or(&version)
            .to_string();

        let comp = Component {
            uid: req.uid.clone(),
            version: actual_version,
            is_locked: false,
            dependencies: dependencies.clone(),
            conflicts: Vec::new(),
            version_file,
        };

        if !comp.is_locked {
            for dep in &dependencies {
                queue.push_back(dep.clone());
            }
        }

        resolved_deps.insert(req.uid.clone(), comp);
    }

    for comp in resolved_deps.values() {
        for conflict in &comp.conflicts {
            if resolved_deps
                .keys()
                .any(|other_uid| other_uid == conflict || other_uid.starts_with(conflict))
            {
                return Err(LaunchError::DependencyConflict {
                    component: comp.uid.clone(),
                    conflict: conflict.clone(),
                });
            }
        }
    }

    Ok(resolved_deps.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::parsers::{default_java_major_for_version, parse_library, parse_version_json};
    use super::*;
    use crate::profile::assemble_launch_profile;
    use crate::{Component, Library, Rule, RuleOs, VersionFile};

    #[test]
    fn parse_library_new_format_native() {
        let json = serde_json::json!({
            "downloads": {
                "artifact": {
                    "path": "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar",
                    "sha1": "a5ed18a2b82fc91b81f40d717cb1f64c9dcb0540",
                    "size": 165_442,
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
    }

    #[test]
    fn parse_library_new_format_regular() {
        let json = serde_json::json!({
            "downloads": {
                "artifact": {
                    "path": "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar",
                    "sha1": "29589b5f87ed335a6c7e7ee6a5775f81f97ecb84",
                    "size": 785_029,
                    "url": "https://libraries.minecraft.net/org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar"
                }
            },
            "name": "org.lwjgl:lwjgl:3.3.3"
        });

        let result = parse_library(&json);
        assert_eq!(result.len(), 1);
        assert!(!result[0].is_native);
    }

    #[test]
    fn parse_version_json_prism_meta_artifact() {
        let json = serde_json::json!({
            "mainJar": {
                "downloads": {
                    "artifact": {
                        "sha1": "30c73b1c5da787909b2f73340419fdf13b9def88",
                        "size": 26_836_906,
                        "url": "https://piston-data.mojang.com/v1/objects/30c73b1c5da787909b2f73340419fdf13b9def88/client.jar"
                    }
                },
                "name": "com.mojang:minecraft:1.21.1:client"
            }
        });

        let vf = parse_version_json(&json);
        let dl = vf
            .client_download
            .expect("client jar from mainJar.downloads.artifact");
        assert_eq!(
            dl.url,
            "https://piston-data.mojang.com/v1/objects/30c73b1c5da787909b2f73340419fdf13b9def88/client.jar"
        );
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
                            "size": 785_029,
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
                            "size": 165_442,
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
        assert_eq!(natives.len(), 1);
        assert_eq!(natives[0].name, "org.lwjgl:lwjgl:3.3.3:natives-windows");
        assert!(natives[0].url.is_some());
    }

    #[test]
    fn assemble_profile_with_natives() {
        let native_lib = Library {
            name: "org.lwjgl:lwjgl:3.3.3:natives-windows".to_string(),
            url: Some("https://libraries.minecraft.net/org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar".to_string()),
            sha1: Some("a5ed18a2b82fc91b81f40d717cb1f64c9dcb0540".to_string()),
            size: Some(165_442),
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
            url: Some(
                "https://libraries.minecraft.net/org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar".to_string(),
            ),
            sha1: Some("29589b5f87ed335a6c7e7ee6a5775f81f97ecb84".to_string()),
            size: Some(785_029),
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
        assert!(!profile.native_libraries.is_empty());
        assert_eq!(profile.native_libraries.len(), 1);
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
    }

    #[tokio::test]
    async fn test_fetch_loader_versions() {
        let resolver = DependencyResolver::new();
        let fabric_versions = resolver
            .fetch_loader_versions("fabric", "1.20.1")
            .await
            .unwrap();
        assert!(!fabric_versions.is_empty());

        let forge_versions_1_20_1 = resolver
            .fetch_loader_versions("forge", "1.20.1")
            .await
            .unwrap();
        assert!(!forge_versions_1_20_1.is_empty());

        let forge_versions_1_8_9 = resolver
            .fetch_loader_versions("forge", "1.8.9")
            .await
            .unwrap();
        assert!(!forge_versions_1_8_9.is_empty());

        let neoforge_versions_1_20_4 = resolver
            .fetch_loader_versions("neoforge", "1.20.4")
            .await
            .unwrap();
        assert!(!neoforge_versions_1_20_4.is_empty());
    }

    #[test]
    fn test_default_java_major_for_version() {
        assert_eq!(default_java_major_for_version("1.7.10"), 8);
        assert_eq!(default_java_major_for_version("1.8.9"), 8);
        assert_eq!(default_java_major_for_version("1.12.2"), 8);
        assert_eq!(default_java_major_for_version("1.16.5"), 8);
        assert_eq!(default_java_major_for_version("1.17.1"), 17);
        assert_eq!(default_java_major_for_version("1.18.2"), 17);
        assert_eq!(default_java_major_for_version("1.20.1"), 17);
        assert_eq!(default_java_major_for_version("1.21"), 21);
        assert_eq!(default_java_major_for_version("26.1"), 21);
    }

    #[test]
    fn parse_library_missing_artifact_is_skipped() {
        let json = serde_json::json!({
            "name": "tv.twitch:twitch-external-platform:4.5",
            "natives": {
                "linux": "natives-linux",
                "osx": "natives-osx",
                "windows": "natives-windows"
            },
            "downloads": {
                "classifiers": {
                    "natives-windows": {
                        "url": "https://libraries.minecraft.net/tv/twitch/twitch-external-platform/4.5/twitch-external-platform-4.5-natives-windows.jar"
                    }
                }
            }
        });

        let result = parse_library(&json);
        assert_eq!(result.len(), 2);
        assert!(!result[0].is_native);
        assert_eq!(result[0].url.as_deref(), Some(""));
        assert!(result[1].is_native);
        assert!(result[1]
            .url
            .as_deref()
            .is_some_and(|u| u.starts_with("https://")));
    }

    #[tokio::test]
    async fn test_resolve_1_5_2_forge() {
        let mut resolver = DependencyResolver::new();
        resolver.fetch_manifest().await.unwrap();
        let vanilla = resolver.fetch_vanilla_component("1.5.2").await.unwrap();
        let forge = resolver
            .fetch_forge_component("1.5.2", "7.8.1.738")
            .await
            .unwrap();
        let merged = resolve_dependencies(&mut resolver, vec![vanilla, forge])
            .await
            .unwrap();
        let profile = assemble_launch_profile(&merged).unwrap();
        assert_eq!(profile.main_class, "net.minecraft.launchwrapper.Launch");
        // Prism Meta serves 1.5.2 as a requires-only component; the org.lwjgl
        // component must be resolved and beat the dead 2.9.0 from the legacy
        // Forge installer.
        assert!(profile
            .libraries
            .iter()
            .any(|l| l.name.contains("2.9.4-nightly")));
        assert!(!profile.libraries.iter().any(|l| l.name.contains("2.9.0")));
    }
}
