use std::collections::HashMap;

use crate::{ClientDownload, Component, LaunchError, Library};
use crate::resolve::default_java_major_for_version;


fn maven_key(name: &str) -> String {
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() >= 4 {
        format!("{}:{}:{}", parts[0], parts[1], parts[3])
    } else if parts.len() >= 2 {
        format!("{}:{}", parts[0], parts[1])
    } else {
        name.to_string()
    }
}

fn maven_version(name: &str) -> &str {
    let mut count = 0;
    let mut start = 0;
    for (i, b) in name.bytes().enumerate() {
        if b == b':' {
            count += 1;
            if count == 2 {
                start = i + 1;
            } else if count == 3 {
                return &name[start..i];
            }
        }
    }
    if count >= 2 {
        &name[start..]
    } else {
        ""
    }
}

fn parse_version_segments(v: &str) -> Vec<u32> {
    v.split('.')
        .filter_map(|s| {
            s.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse()
                .ok()
        })
        .collect()
}

fn is_higher_version(a: &str, b: &str) -> bool {
    let va = parse_version_segments(a);
    let vb = parse_version_segments(b);
    va > vb
}

#[derive(Debug, Clone)]
pub struct LaunchProfile {
    pub mc_version: String,
    pub mc_version_type: String,
    pub main_class: String,
    pub libraries: Vec<Library>,
    pub native_libraries: Vec<Library>,
    pub asset_index: AssetIndex,
    pub client_download: Option<ClientDownload>,
    pub jvm_args: Vec<String>,
    pub game_args_template: String,
    pub traits: Vec<String>,
    pub compatible_java_majors: Vec<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct AssetIndex {
    pub id: String,
    pub url: String,
    pub sha1: Option<String>,
    pub size: u64,
}

fn upsert_library(map: &mut HashMap<String, Library>, lib: &Library) {
    let key = maven_key(&lib.name);
    let version = maven_version(&lib.name).to_string();
    match map.get(&key) {
        Some(existing) => {
            let existing_ver = maven_version(&existing.name);
            if is_higher_version(&version, existing_ver) {
                map.insert(key, lib.clone());
            }
        }
        None => {
            map.insert(key, lib.clone());
        }
    }
}

/// # Errors
/// Returns an error if the components are inconsistent.
pub fn assemble_launch_profile(components: &[Component]) -> Result<LaunchProfile, LaunchError> {
    let mut libraries_map: HashMap<String, Library> = HashMap::new();
    let mut native_map: HashMap<String, Library> = HashMap::new();
    let mut main_class = None;
    let mut mc_version = String::new();
    let mut mc_version_type = String::new();
    let mut jvm_args = Vec::new();
    let mut game_args_template = String::new();
    let mut all_traits = Vec::new();
    let mut compatible_java_majors = Vec::new();
    let mut asset_index = AssetIndex::default();
    let mut client_download = None;

    for component in components {
        if component.uid == "net.minecraft" {
            mc_version.clone_from(&component.version);
        }
        // Propagate version_type from component (release/snapshot/old_beta/old_alpha)
        if mc_version_type.is_empty() {
            if let Some(ref vt) = component.version_file.version_type {
                mc_version_type = vt.clone();
            }
        }
        if let Some(ref mc) = component.version_file.main_class {
            if component.uid != "net.minecraft" || main_class.is_none() {
                main_class = Some(mc.clone());
            }
        }
        if component.version_file.minecraft_args.is_some() && game_args_template.is_empty() {
            game_args_template = component
                .version_file
                .minecraft_args
                .clone()
                .unwrap_or_default();
        }
        if let Some(ai) = &component.version_file.asset_index {
            if asset_index.id.is_empty() {
                asset_index = ai.clone();
            }
        }
        if client_download.is_none() {
            client_download = component.version_file.client_download.clone();
        }
        for lib in &component.version_file.libraries {
            if lib.is_native {
                upsert_library(&mut native_map, lib);
            } else {
                upsert_library(&mut libraries_map, lib);
            }
        }
        for arg in &component.version_file.jvm_args {
            if !jvm_args.contains(arg) {
                jvm_args.push(arg.clone());
            }
        }
        for t in &component.version_file.traits {
            if !all_traits.contains(t) {
                all_traits.push(t.clone());
            }
        }
        for j in &component.version_file.compatible_java_majors {
            if !compatible_java_majors.contains(j) {
                compatible_java_majors.push(*j);
            }
        }
    }

    if compatible_java_majors.is_empty() {
        let major = default_java_major_for_version(&mc_version);
        compatible_java_majors = vec![major];
    }

    // If no asset index was found in any component (e.g. Beta/Alpha versions),
    // fall back to the "legacy" index used by Mojang for pre-1.6 assets.
    if asset_index.id.is_empty() {
        asset_index = AssetIndex {
            id: "legacy".to_string(),
            url: "https://s3.amazonaws.com/Minecraft.Download/indexes/legacy.json".to_string(),
            sha1: None,
            size: 0,
        };
    }

    // If version_type was never set, infer it from the version string
    if mc_version_type.is_empty() {
        mc_version_type = "release".to_string();
    }

    Ok(LaunchProfile {
        mc_version,
        mc_version_type,
        main_class: main_class.unwrap_or_else(|| "net.minecraft.client.main.Main".to_string()),
        libraries: libraries_map.into_values().collect(),
        native_libraries: native_map.into_values().collect(),
        asset_index,
        client_download,
        jvm_args,
        game_args_template,
        traits: all_traits,
        compatible_java_majors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VersionFile;

    #[test]
    fn test_modloader_main_class_precedence() {
        let vanilla = Component {
            uid: "net.minecraft".to_string(),
            version: "26.1.2".to_string(),
            is_locked: true,
            dependencies: vec![],
            conflicts: vec![],
            version_file: VersionFile {
                main_class: Some("net.minecraft.client.main.Main".to_string()),
                compatible_java_majors: vec![25],
                ..VersionFile::default()
            },
        };

        let fabric = Component {
            uid: "net.fabricmc.fabric-loader".to_string(),
            version: "0.19.3".to_string(),
            is_locked: true,
            dependencies: vec![],
            conflicts: vec![],
            version_file: VersionFile {
                main_class: Some("net.fabricmc.loader.impl.launch.knot.KnotClient".to_string()),
                ..VersionFile::default()
            },
        };

        // Test vanilla first, fabric second
        let profile1 = assemble_launch_profile(&[vanilla.clone(), fabric.clone()]).unwrap();
        assert_eq!(profile1.main_class, "net.fabricmc.loader.impl.launch.knot.KnotClient");
        assert_eq!(profile1.compatible_java_majors, vec![25]);

        // Test fabric first, vanilla second
        let profile2 = assemble_launch_profile(&[fabric, vanilla]).unwrap();
        assert_eq!(profile2.main_class, "net.fabricmc.loader.impl.launch.knot.KnotClient");
        assert_eq!(profile2.compatible_java_majors, vec![25]);
    }
}
