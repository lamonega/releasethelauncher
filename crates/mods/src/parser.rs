use std::fs;
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

use crate::ModDetails;

/// Parse mod metadata from a JAR file.
///
/// # Errors
///
/// Returns an error if the file cannot be read or the archive cannot be opened.
///
/// # Panics
///
/// Panics if the JAR file name has no stem (no file name before the extension).
pub fn parse_mod_metadata(jar_path: &Path) -> Result<ModDetails, crate::ModsError> {
    let file = fs::File::open(jar_path)?;
    let mut archive = ZipArchive::new(file)?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();

        if name == "fabric.mod.json" {
            let mut content = String::new();
            entry.read_to_string(&mut content)?;
            return parse_fabric_mod_json(&content);
        }

        if name == "META-INF/mods.toml" || name == "META-INF/neoforge.mods.toml" {
            let mut content = String::new();
            entry.read_to_string(&mut content)?;
            return parse_mods_toml(&content);
        }

        if name == "quilt.mod.json" {
            let mut content = String::new();
            entry.read_to_string(&mut content)?;
            return parse_quilt_mod_json(&content);
        }

        if name == "mcmod.info" {
            let mut content = String::new();
            entry.read_to_string(&mut content)?;
            return parse_mcmod_info(&content);
        }
    }

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();

        if name == "META-INF/MANIFEST.MF" {
            let mut content = String::new();
            entry.read_to_string(&mut content)?;
            if let Some(version) = extract_manifest_version(&content) {
                return Ok(ModDetails {
                    mod_id: jar_path.file_stem().unwrap().to_string_lossy().to_string(),
                    name: jar_path.file_stem().unwrap().to_string_lossy().to_string(),
                    version,
                    mc_version: None,
                    description: String::new(),
                    authors: Vec::new(),
                });
            }
        }
    }

    let stem = jar_path.file_stem().unwrap().to_string_lossy().to_string();
    Ok(ModDetails {
        mod_id: stem.clone(),
        name: stem,
        version: "unknown".to_string(),
        mc_version: None,
        description: String::new(),
        authors: Vec::new(),
    })
}

fn parse_fabric_mod_json(content: &str) -> Result<ModDetails, crate::ModsError> {
    let value: serde_json::Value = serde_json::from_str(content)?;

    let mod_id = value
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let name = value
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(&mod_id)
        .to_string();

    let version = value
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let description = value
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let authors = value
        .get("authors")
        .and_then(|v| v.as_array())
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

    let mc_version = value
        .get("depends")
        .and_then(|v| v.get("minecraft"))
        .and_then(|v| v.as_str())
        .map(ToString::to_string);

    Ok(ModDetails {
        mod_id,
        name,
        version,
        mc_version,
        description,
        authors,
    })
}

fn parse_mods_toml(content: &str) -> Result<ModDetails, crate::ModsError> {
    let value: toml::Value = content.parse()?;

    let mod_id = value
        .get("mods")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.get("modId"))
        .and_then(|v| v.as_str())
        .or_else(|| {
            value
                .get("modLoader")
                .and_then(|v| v.as_array())
                .and_then(|a| a.first())
                .and_then(|v| v.get("modId"))
                .and_then(|v| v.as_str())
        })
        .unwrap_or("unknown")
        .to_string();

    let name = value
        .get("mods")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.get("displayName"))
        .and_then(|v| v.as_str())
        .unwrap_or(&mod_id)
        .to_string();

    let version = value
        .get("mods")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.get("version"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let description = value
        .get("mods")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.get("description"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let authors = value
        .get("mods")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.get("authors"))
        .and_then(|v| v.as_str())
        .map(|s| vec![s.to_string()])
        .unwrap_or_default();

    Ok(ModDetails {
        mod_id,
        name,
        version,
        mc_version: None,
        description,
        authors,
    })
}

fn parse_quilt_mod_json(content: &str) -> Result<ModDetails, crate::ModsError> {
    let value: serde_json::Value = serde_json::from_str(content)?;

    let quilt_loader = value.get("quilt_loader").and_then(|v| v.as_object());

    let mod_id = quilt_loader
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let name = quilt_loader
        .and_then(|v| v.get("metadata"))
        .and_then(|v| v.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or(&mod_id)
        .to_string();

    let version = quilt_loader
        .and_then(|v| v.get("metadata"))
        .and_then(|v| v.get("version"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let description = quilt_loader
        .and_then(|v| v.get("metadata"))
        .and_then(|v| v.get("description"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let authors = quilt_loader
        .and_then(|v| v.get("metadata"))
        .and_then(|v| v.get("contributors"))
        .and_then(|v| v.as_object())
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();

    Ok(ModDetails {
        mod_id,
        name,
        version,
        mc_version: None,
        description,
        authors,
    })
}

fn parse_mcmod_info(content: &str) -> Result<ModDetails, crate::ModsError> {
    let value: serde_json::Value = serde_json::from_str(content)?;

    let first = value
        .as_array()
        .and_then(|a| a.first())
        .cloned()
        .unwrap_or_default();

    let mod_id = first
        .get("modid")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let name = first
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(&mod_id)
        .to_string();

    let version = first
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let description = first
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let mc_version = first
        .get("mcversion")
        .and_then(|v| v.as_str())
        .map(ToString::to_string);

    Ok(ModDetails {
        mod_id,
        name,
        version,
        mc_version,
        description,
        authors: Vec::new(),
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
