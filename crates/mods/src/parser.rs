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
                let stem = jar_path.file_stem().map_or_else(
                    || "unknown".to_string(),
                    |s| s.to_string_lossy().to_string(),
                );
                return Ok(ModDetails {
                    mod_id: stem.clone(),
                    name: stem,
                    version,
                    mc_version: None,
                    description: String::new(),
                    authors: Vec::new(),
                    dependencies: Vec::new(),
                    side: None,
                });
            }
        }
    }

    let stem = jar_path.file_stem().map_or_else(
        || "unknown".to_string(),
        |s| s.to_string_lossy().to_string(),
    );
    Ok(ModDetails {
        mod_id: stem.clone(),
        name: stem,
        version: "unknown".to_string(),
        mc_version: None,
        description: String::new(),
        authors: Vec::new(),
        dependencies: Vec::new(),
        side: None,
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

    let dependencies = value
        .get("depends")
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.keys()
                .filter(|k| *k != "minecraft" && *k != "java")
                .cloned()
                .collect()
        })
        .unwrap_or_default();

    let side = value
        .get("environment")
        .and_then(|v| v.as_str())
        .map(ToString::to_string);

    Ok(ModDetails {
        mod_id,
        name,
        version,
        mc_version,
        description,
        authors,
        dependencies,
        side,
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

    let dependencies = value
        .get("dependencies")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|dep| {
                    dep.get("modId")
                        .and_then(|v| v.as_str())
                        .filter(|id| *id != "minecraft" && *id != "java")
                        .map(ToString::to_string)
                })
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

    let dependencies = quilt_loader
        .and_then(|v| v.get("depends"))
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.keys()
                .filter(|k| *k != "minecraft" && *k != "java")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_fabric_mod_json() {
        let json = r#"{
            "id": "examplemod",
            "name": "Example Mod",
            "version": "1.0.0",
            "description": "An example mod"
        }"#;
        let details = parse_fabric_mod_json(json).unwrap();
        assert_eq!(details.mod_id, "examplemod");
        assert_eq!(details.name, "Example Mod");
        assert_eq!(details.version, "1.0.0");
    }

    #[test]
    fn test_parse_mcmod_info() {
        let json = r#"[
            {
                "modid": "legacy_mod",
                "name": "Legacy Mod",
                "version": "2.0.0",
                "mcversion": "1.12.2"
            }
        ]"#;
        let details = parse_mcmod_info(json).unwrap();
        assert_eq!(details.mod_id, "legacy_mod");
        assert_eq!(details.version, "2.0.0");
        assert_eq!(details.mc_version.as_deref(), Some("1.12.2"));
    }

    #[test]
    fn test_extract_manifest_version() {
        let manifest = "Manifest-Version: 1.0\nImplementation-Version: 3.2.1\n";
        assert_eq!(
            extract_manifest_version(manifest),
            Some("3.2.1".to_string())
        );
    }
}
