use serde::Deserialize;
use serde_json::Value;
use std::io::Read;
use std::path::Path;

use crate::{ModDetails, ModsError};

fn fallback_details(stem: &str, version: &str) -> ModDetails {
    ModDetails {
        mod_id: stem.to_string(),
        name: stem.to_string(),
        version: version.to_string(),
        mc_version: None,
        description: String::new(),
        authors: Vec::new(),
        dependencies: Vec::new(),
        side: None,
    }
}

pub fn parse_mod_metadata(jar_path: &Path) -> Result<ModDetails, ModsError> {
    let file = std::fs::File::open(jar_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    let mut fabric_json = None;
    let mut mods_toml = None;
    let mut quilt_json = None;
    let mut mcmod_info = None;
    let mut manifest_mf = None;

    for i in 0..archive.len() {
        if let Ok(mut file) = archive.by_index(i) {
            let name = file.name();
            if name == "fabric.mod.json" {
                let mut buf = String::new();
                let _ = file.read_to_string(&mut buf);
                fabric_json = Some(buf);
            } else if name == "META-INF/mods.toml" || name == "META-INF/neoforge.mods.toml" {
                let mut buf = String::new();
                let _ = file.read_to_string(&mut buf);
                mods_toml = Some(buf);
            } else if name == "quilt.mod.json" {
                let mut buf = String::new();
                let _ = file.read_to_string(&mut buf);
                quilt_json = Some(buf);
            } else if name == "mcmod.info" {
                let mut buf = String::new();
                let _ = file.read_to_string(&mut buf);
                mcmod_info = Some(buf);
            } else if name == "META-INF/MANIFEST.MF" {
                let mut buf = String::new();
                let _ = file.read_to_string(&mut buf);
                manifest_mf = Some(buf);
            }
        }
    }

    if let Some(content) = fabric_json {
        return parse_fabric_mod_json(&content);
    }
    if let Some(content) = mods_toml {
        return parse_mods_toml(&content);
    }
    if let Some(content) = quilt_json {
        return parse_quilt_mod_json(&content);
    }
    if let Some(content) = mcmod_info {
        return parse_mcmod_info(&content);
    }
    if let Some(content) = manifest_mf {
        if let Some(version) = extract_manifest_version(&content) {
            let stem = jar_path.file_stem().map_or_else(
                || "unknown".to_string(),
                |s| s.to_string_lossy().to_string(),
            );
            return Ok(fallback_details(&stem, &version));
        }
    }

    let stem = jar_path.file_stem().map_or_else(
        || "unknown".to_string(),
        |s| s.to_string_lossy().to_string(),
    );
    Ok(fallback_details(&stem, "unknown"))
}

#[derive(Deserialize)]
struct FabricMod {
    #[serde(default)]
    id: String,
    name: Option<String>,
    #[serde(default)]
    version: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    authors: Value,
    #[serde(default)]
    depends: std::collections::HashMap<String, Value>,
    environment: Option<String>,
}

fn parse_fabric_mod_json(content: &str) -> Result<ModDetails, ModsError> {
    let fm: FabricMod = serde_json::from_str(content)?;

    let mod_id = if fm.id.is_empty() {
        "unknown".to_string()
    } else {
        fm.id.clone()
    };
    let name = fm.name.unwrap_or_else(|| mod_id.clone());
    let version = if fm.version.is_empty() {
        "unknown".to_string()
    } else {
        fm.version
    };

    let authors = fm
        .authors
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|a| {
                    a.as_str().map_or_else(
                        || {
                            a.as_object().and_then(|obj| {
                                obj.get("name")
                                    .and_then(|n| n.as_str())
                                    .map(ToString::to_string)
                            })
                        },
                        |s| Some(s.to_string()),
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    let mc_version = fm
        .depends
        .get("minecraft")
        .and_then(|v| v.as_str())
        .map(ToString::to_string);
    let dependencies = fm
        .depends
        .keys()
        .filter(|k| k.as_str() != "minecraft" && k.as_str() != "java")
        .cloned()
        .collect();

    Ok(ModDetails {
        mod_id,
        name,
        version,
        mc_version,
        description: fm.description,
        authors,
        dependencies,
        side: fm.environment,
    })
}

#[derive(Deserialize)]
struct ModsToml {
    mods: Option<Vec<ForgeMod>>,
    #[serde(rename = "modLoader")]
    mod_loader: Option<Vec<ForgeMod>>,
    dependencies: Option<std::collections::HashMap<String, Vec<ForgeDependency>>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ForgeMod {
    mod_id: Option<String>,
    display_name: Option<String>,
    version: Option<String>,
    #[serde(default)]
    description: String,
    authors: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ForgeDependency {
    mod_id: Option<String>,
}

fn parse_mods_toml(content: &str) -> Result<ModDetails, ModsError> {
    let toml: ModsToml = toml::from_str(content)?;

    let fm = toml
        .mods
        .and_then(|m| m.into_iter().next())
        .or_else(|| toml.mod_loader.and_then(|m| m.into_iter().next()));

    let (mod_id, name, version, description, authors) = if let Some(fm) = fm {
        let mid = fm.mod_id.unwrap_or_else(|| "unknown".to_string());
        let nm = fm.display_name.unwrap_or_else(|| mid.clone());
        let ver = fm.version.unwrap_or_else(|| "unknown".to_string());
        let auths = fm.authors.map(|s| vec![s]).unwrap_or_default();
        (mid, nm, ver, fm.description, auths)
    } else {
        (
            "unknown".to_string(),
            "unknown".to_string(),
            "unknown".to_string(),
            String::new(),
            Vec::new(),
        )
    };

    let dependencies = toml
        .dependencies
        .and_then(|deps| deps.into_values().next())
        .map(|arr| {
            arr.into_iter()
                .filter_map(|d| d.mod_id)
                .filter(|id| id != "minecraft" && id != "java")
                .collect()
        })
        .unwrap_or_default();

    Ok(ModDetails {
        mod_id,
        name,
        version,
        mc_version: None,
        description,
        authors,
        dependencies,
        side: None,
    })
}

#[derive(Deserialize)]
struct QuiltMod {
    quilt_loader: Option<QuiltLoader>,
}

#[derive(Deserialize)]
struct QuiltLoader {
    #[serde(default)]
    id: String,
    metadata: Option<QuiltMetadata>,
    depends: Option<std::collections::HashMap<String, Value>>,
}

#[derive(Deserialize)]
struct QuiltMetadata {
    name: Option<String>,
    #[serde(default)]
    version: String,
    #[serde(default)]
    description: String,
    contributors: Option<std::collections::HashMap<String, Value>>,
}

fn parse_quilt_mod_json(content: &str) -> Result<ModDetails, ModsError> {
    let qm: QuiltMod = serde_json::from_str(content)?;

    if let Some(loader) = qm.quilt_loader {
        let mod_id = if loader.id.is_empty() {
            "unknown".to_string()
        } else {
            loader.id.clone()
        };
        let (name, version, description, authors) = if let Some(meta) = loader.metadata {
            let nm = meta.name.unwrap_or_else(|| mod_id.clone());
            let ver = if meta.version.is_empty() {
                "unknown".to_string()
            } else {
                meta.version
            };
            let auths = meta
                .contributors
                .map(|c| c.keys().cloned().collect())
                .unwrap_or_default();
            (nm, ver, meta.description, auths)
        } else {
            (
                mod_id.clone(),
                "unknown".to_string(),
                String::new(),
                Vec::new(),
            )
        };

        let dependencies = loader
            .depends
            .map(|d| {
                d.keys()
                    .filter(|k| k.as_str() != "minecraft" && k.as_str() != "java")
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();

        Ok(ModDetails {
            mod_id,
            name,
            version,
            mc_version: None,
            description,
            authors,
            dependencies,
            side: None,
        })
    } else {
        Ok(fallback_details("unknown", "unknown"))
    }
}

#[derive(Deserialize)]
struct McmodInfo {
    #[serde(default)]
    modid: String,
    name: Option<String>,
    #[serde(default)]
    version: String,
    #[serde(default)]
    description: String,
    mcversion: Option<String>,
}

fn parse_mcmod_info(content: &str) -> Result<ModDetails, ModsError> {
    let arr: Vec<McmodInfo> = serde_json::from_str(content)?;
    let first = arr.into_iter().next();

    let (mod_id, name, version, description, mc_version) = if let Some(first) = first {
        let mid = if first.modid.is_empty() {
            "unknown".to_string()
        } else {
            first.modid
        };
        let nm = first.name.unwrap_or_else(|| mid.clone());
        let ver = if first.version.is_empty() {
            "unknown".to_string()
        } else {
            first.version
        };
        (mid, nm, ver, first.description, first.mcversion)
    } else {
        (
            "unknown".to_string(),
            "unknown".to_string(),
            "unknown".to_string(),
            String::new(),
            None,
        )
    };

    Ok(ModDetails {
        mod_id,
        name,
        version,
        mc_version,
        description,
        authors: Vec::new(),
        dependencies: Vec::new(),
        side: None,
    })
}

fn extract_manifest_version(content: &str) -> Option<String> {
    for line in content.lines() {
        if let Some(val) = line.strip_prefix("Implementation-Version: ") {
            return Some(val.trim().to_string());
        }
    }
    None
}
