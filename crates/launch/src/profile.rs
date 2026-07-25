use crate::{Component, Library, LaunchError};

#[derive(Debug, Clone)]
pub struct LaunchProfile {
    pub mc_version: String,
    pub mc_version_type: String,
    pub main_class: String,
    pub libraries: Vec<Library>,
    pub native_libraries: Vec<Library>,
    pub asset_index: AssetIndex,
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

/// # Errors
/// Returns an error if the components are inconsistent.
pub fn assemble_launch_profile(components: &[Component]) -> Result<LaunchProfile, LaunchError> {
    let mut libraries = Vec::new();
    let mut native_libraries = Vec::new();
    let mut main_class = None;
    let mut mc_version = String::new();
    let mc_version_type = "release".to_string();
    let mut jvm_args = Vec::new();
    let mut game_args_template = String::new();
    let mut all_traits = Vec::new();
    let mut compatible_java_majors = Vec::new();

    for component in components {
        if component.uid == "net.minecraft" {
            mc_version.clone_from(&component.version);
        }
        if component.version_file.main_class.is_some() && main_class.is_none() {
            main_class.clone_from(&component.version_file.main_class);
        }
        if component.version_file.minecraft_args.is_some() && game_args_template.is_empty() {
            game_args_template = component.version_file.minecraft_args.clone().unwrap_or_default();
        }
        for lib in &component.version_file.libraries {
            if lib.is_native {
                if !native_libraries.iter().any(|e: &Library| e.name == lib.name) {
                    native_libraries.push(lib.clone());
                }
            } else if !libraries.iter().any(|e: &Library| e.name == lib.name) {
                libraries.push(lib.clone());
            }
        }
        for arg in &component.version_file.jvm_args {
            if !jvm_args.contains(arg) { jvm_args.push(arg.clone()); }
        }
        for t in &component.version_file.traits {
            if !all_traits.contains(t) { all_traits.push(t.clone()); }
        }
        for j in &component.version_file.compatible_java_majors {
            if !compatible_java_majors.contains(j) { compatible_java_majors.push(*j); }
        }
    }

    if compatible_java_majors.is_empty() { compatible_java_majors = vec![17, 21]; }

    Ok(LaunchProfile {
        mc_version,
        mc_version_type,
        main_class: main_class.unwrap_or_else(|| "net.minecraft.client.main.Main".to_string()),
        libraries,
        native_libraries,
        asset_index: AssetIndex::default(),
        jvm_args,
        game_args_template,
        traits: all_traits,
        compatible_java_majors,
    })
}
