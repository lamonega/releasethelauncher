use std::collections::HashMap;

use crate::resolve::parsers::default_java_major_for_version;
use crate::{ClientDownload, Component, LaunchError, Library};

fn maven_key(name: &str) -> String {
    if let Some(coord) = crate::MavenCoord::parse(name) {
        if let Some(classifier) = coord.classifier {
            format!("{}:{}:{}", coord.group, coord.artifact, classifier)
        } else {
            format!("{}:{}", coord.group, coord.artifact)
        }
    } else {
        name.to_string()
    }
}

fn maven_version(name: &str) -> String {
    if let Some(coord) = crate::MavenCoord::parse(name) {
        coord.version
    } else {
        String::new()
    }
}

fn is_higher_version(a: &str, b: &str) -> bool {
    match (
        version_compare::Version::from(a),
        version_compare::Version::from(b),
    ) {
        (Some(va), Some(vb)) => va > vb,
        _ => a > b,
    }
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
    pub tweakers: Vec<String>,
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
    let version = maven_version(&lib.name);
    match map.get(&key) {
        Some(existing) if existing.rules != lib.rules => {
            // Platform variants of the same artifact (e.g. lwjgl 2.9.0 vs the
            // 2.9.1-nightly build that is osx-only): keep both, the per-OS rule
            // filtering picks the right one later.
            map.insert(
                format!("{key}#{}", maven_version(&existing.name)),
                existing.clone(),
            );
            map.remove(&key);
            map.insert(format!("{key}#{version}"), lib.clone());
        }
        Some(existing) => {
            if is_higher_version(&version, &maven_version(&existing.name)) {
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
    let mut acc = ComponentAccumulator::default();

    for component in components {
        acc.process(component);
    }

    let mut compatible_java_majors = acc.compatible_java_majors;
    if compatible_java_majors.is_empty() {
        let major = default_java_major_for_version(&acc.mc_version);
        compatible_java_majors = vec![major];
    }

    let asset_index = if acc.asset_index.id.is_empty() {
        AssetIndex {
            id: "legacy".to_string(),
            url: format!(
                "{}/legacy.json",
                release_the_launcher_constants::urls::S3_MINECRAFT_INDEXES
            ),
            sha1: None,
            size: 0,
        }
    } else {
        acc.asset_index
    };

    let mc_version_type = if acc.mc_version_type.is_empty() {
        "release".to_string()
    } else {
        acc.mc_version_type
    };

    let default_main_class = if acc.all_traits.contains(&"legacyLaunch".to_string()) {
        "net.minecraft.client.Minecraft".to_string()
    } else {
        "net.minecraft.client.main.Main".to_string()
    };

    Ok(LaunchProfile {
        mc_version: acc.mc_version,
        mc_version_type,
        main_class: acc.main_class.unwrap_or(default_main_class),
        libraries: acc.libraries_map.into_values().collect(),
        native_libraries: acc.native_map.into_values().collect(),
        asset_index,
        client_download: acc.client_download,
        jvm_args: acc.jvm_args,
        game_args_template: acc.game_args_template,
        traits: acc.all_traits,
        tweakers: acc.all_tweakers,
        compatible_java_majors,
    })
}

#[derive(Default)]
struct ComponentAccumulator {
    libraries_map: HashMap<String, Library>,
    native_map: HashMap<String, Library>,
    main_class: Option<String>,
    mc_version: String,
    mc_version_type: String,
    jvm_args: Vec<String>,
    game_args_template: String,
    all_traits: Vec<String>,
    all_tweakers: Vec<String>,
    compatible_java_majors: Vec<u32>,
    asset_index: AssetIndex,
    client_download: Option<ClientDownload>,
}

impl ComponentAccumulator {
    fn process(&mut self, component: &Component) {
        if component.uid == "net.minecraft" {
            self.mc_version.clone_from(&component.version);
        }
        if self.mc_version_type.is_empty() {
            if let Some(ref vt) = component.version_file.version_type {
                self.mc_version_type.clone_from(vt);
            }
        }
        if let Some(ref mc) = component.version_file.main_class {
            if component.uid != "net.minecraft" || self.main_class.is_none() {
                self.main_class = Some(mc.clone());
            }
        }
        if component.version_file.minecraft_args.is_some() && self.game_args_template.is_empty() {
            self.game_args_template = component
                .version_file
                .minecraft_args
                .clone()
                .unwrap_or_default();
        }
        if let Some(ai) = &component.version_file.asset_index {
            if self.asset_index.id.is_empty() {
                self.asset_index = ai.clone();
            }
        }
        if self.client_download.is_none() {
            self.client_download
                .clone_from(&component.version_file.client_download);
        }
        for lib in &component.version_file.libraries {
            if lib.is_native {
                upsert_library(&mut self.native_map, lib);
            } else {
                upsert_library(&mut self.libraries_map, lib);
            }
        }
        for arg in &component.version_file.jvm_args {
            if !self.jvm_args.contains(arg) {
                self.jvm_args.push(arg.clone());
            }
        }
        for t in &component.version_file.traits {
            if !self.all_traits.contains(t) {
                self.all_traits.push(t.clone());
            }
        }
        for t in &component.version_file.tweakers {
            if !self.all_tweakers.contains(t) {
                self.all_tweakers.push(t.clone());
            }
        }
        for j in &component.version_file.compatible_java_majors {
            if !self.compatible_java_majors.contains(j) {
                self.compatible_java_majors.push(*j);
            }
        }
    }
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
            dependencies: vec![],
            version_file: VersionFile {
                main_class: Some("net.minecraft.client.main.Main".to_string()),
                compatible_java_majors: vec![25],
                ..VersionFile::default()
            },
        };

        let fabric = Component {
            uid: "net.fabricmc.fabric-loader".to_string(),
            version: "0.19.3".to_string(),
            dependencies: vec![],
            version_file: VersionFile {
                main_class: Some("net.fabricmc.loader.impl.launch.knot.KnotClient".to_string()),
                ..VersionFile::default()
            },
        };

        // Test vanilla first, fabric second
        let profile1 = assemble_launch_profile(&[vanilla.clone(), fabric.clone()]).unwrap();
        assert_eq!(
            profile1.main_class,
            "net.fabricmc.loader.impl.launch.knot.KnotClient"
        );
        assert_eq!(profile1.compatible_java_majors, vec![25]);

        // Test fabric first, vanilla second
        let profile2 = assemble_launch_profile(&[fabric, vanilla]).unwrap();
        assert_eq!(
            profile2.main_class,
            "net.fabricmc.loader.impl.launch.knot.KnotClient"
        );
        assert_eq!(profile2.compatible_java_majors, vec![25]);
    }
}
