use crate::{AssetIndex, ClientDownload, Extract, Library, Rule, RuleOs, VersionFile};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct VersionJson {
    pub main_class: Option<String>,
    pub minecraft_arguments: Option<String>,
    pub arguments: Option<ArgumentsJson>,
    pub libraries: Option<Vec<LibraryJson>>,
    pub jar_mods: Option<Vec<LibraryJson>>,
    pub java_version: Option<JavaVersionJson>,
    pub asset_index: Option<AssetIndexJson>,
    pub downloads: Option<DownloadsJson>,
    pub main_jar: Option<MainJarJson>,
    #[serde(rename = "type")]
    pub version_type: Option<String>,
    pub traits: Option<Vec<String>>,
    #[serde(rename = "+traits")]
    pub plus_traits: Option<Vec<String>>,
    #[serde(rename = "+tweakers")]
    pub plus_tweakers: Option<Vec<String>>,
    pub requires: Option<Vec<RequirementJson>>,
    #[serde(rename = "_requires")]
    pub underscore_requires: Option<Vec<RequirementJson>>,
}

#[derive(Deserialize, Debug, Default)]
pub struct ArgumentsJson {
    pub game: Option<Vec<ArgumentItem>>,
    pub jvm: Option<Vec<ArgumentItem>>,
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub enum ArgumentValue {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub enum ArgumentItem {
    Plain(String),
    WithRules {
        rules: Option<Vec<RuleJson>>,
        value: ArgumentValue,
    },
}

#[derive(Deserialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersionJson {
    pub major_version: Option<u64>,
}

#[derive(Deserialize, Debug, Default)]
pub struct AssetIndexJson {
    pub id: Option<String>,
    pub url: Option<String>,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

#[derive(Deserialize, Debug, Default)]
pub struct DownloadsJson {
    pub client: Option<ArtifactJson>,
    pub artifact: Option<ArtifactJson>,
}

#[derive(Deserialize, Debug, Default)]
pub struct MainJarJson {
    pub downloads: Option<DownloadsJson>,
}

#[derive(Deserialize, Debug, Default, Clone)]
pub struct RequirementJson {
    pub uid: String,
    pub suggests: Option<String>,
    pub equals: Option<String>,
}

#[derive(Deserialize, Debug, Default, Clone)]
pub struct LibraryJson {
    pub name: Option<String>,
    pub url: Option<String>,
    pub sha1: Option<String>,
    pub size: Option<u64>,
    pub downloads: Option<LibraryDownloadsJson>,
    pub rules: Option<Vec<RuleJson>>,
    pub extract: Option<ExtractJson>,
    pub natives: Option<HashMap<String, String>>,
    pub tweaker_class: Option<String>,
}

#[derive(Deserialize, Debug, Default, Clone)]
pub struct LibraryDownloadsJson {
    pub artifact: Option<ArtifactJson>,
    pub classifiers: Option<HashMap<String, ArtifactJson>>,
}

#[derive(Deserialize, Debug, Default, Clone)]
pub struct ArtifactJson {
    pub url: Option<String>,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

#[derive(Deserialize, Debug, Default, Clone)]
pub struct RuleJson {
    pub action: Option<String>,
    pub os: Option<RuleOsJson>,
    pub features: Option<HashMap<String, bool>>,
}

#[derive(Deserialize, Debug, Default, Clone)]
pub struct RuleOsJson {
    pub name: Option<String>,
    pub arch: Option<String>,
}

#[derive(Deserialize, Debug, Default, Clone)]
pub struct ExtractJson {
    pub exclude: Option<Vec<String>>,
}

impl From<&RuleJson> for Rule {
    fn from(rj: &RuleJson) -> Self {
        Self {
            action: rj.action.clone().unwrap_or_else(|| "allow".to_string()),
            os: rj.os.as_ref().map(|o| RuleOs {
                name: o.name.clone(),
                arch: o.arch.clone(),
            }),
            features: rj.features.clone().unwrap_or_default(),
        }
    }
}

#[must_use]
pub fn parse_version_json(vj: &VersionJson) -> VersionFile {
    let mut libraries = Vec::new();
    let mut tweakers = Vec::new();
    let mut jvm_args = Vec::new();
    let mut game_args = Vec::new();

    if let Some(libs) = &vj.libraries {
        for lib_json in libs {
            if let Some(t) = &lib_json.tweaker_class {
                tweakers.push(t.clone());
            }
            libraries.extend(parse_library(lib_json));
        }
    }

    if let Some(jar_mods) = &vj.jar_mods {
        for jm in jar_mods {
            libraries.extend(parse_library(jm));
        }
    }

    if let Some(args) = &vj.arguments {
        if let Some(game) = &args.game {
            for arg in game {
                parse_argument_item_enum(arg, &mut game_args);
            }
        }
        if let Some(jvm) = &args.jvm {
            for arg in jvm {
                parse_argument_item_enum(arg, &mut jvm_args);
            }
        }
    }

    if game_args.is_empty() {
        if let Some(ma) = &vj.minecraft_arguments {
            game_args.push(ma.clone());
        }
    }

    let minecraft_args = if game_args.is_empty() {
        None
    } else {
        Some(game_args.join(" "))
    };

    let asset_index = vj.asset_index.as_ref().map(|ai| AssetIndex {
        id: ai.id.clone().unwrap_or_default(),
        url: ai.url.clone().unwrap_or_default(),
        sha1: ai.sha1.clone(),
        size: ai.size.unwrap_or(0),
    });

    let client_download = vj
        .downloads
        .as_ref()
        .and_then(|d| d.client.clone().or_else(|| d.artifact.clone()))
        .or_else(|| {
            vj.main_jar
                .as_ref()
                .and_then(|mj| mj.downloads.as_ref())
                .and_then(|d| d.client.clone().or_else(|| d.artifact.clone()))
        })
        .map(|c| ClientDownload {
            url: c.url.unwrap_or_default(),
            sha1: c.sha1,
            size: c.size.unwrap_or(0),
        });

    let compatible_java_majors = vj
        .java_version
        .as_ref()
        .and_then(|jv| jv.major_version)
        .map(|v| vec![u32::try_from(v).unwrap_or(8)])
        .unwrap_or_default();

    let mut traits = Vec::new();
    if let Some(tr) = &vj.traits {
        traits.extend(tr.clone());
    }
    if let Some(tr) = &vj.plus_traits {
        traits.extend(tr.clone());
    }
    if let Some(tw) = &vj.plus_tweakers {
        tweakers.extend(tw.clone());
    }

    VersionFile {
        main_class: vj.main_class.clone(),
        minecraft_args,
        jvm_args,
        libraries,
        traits,
        compatible_java_majors,
        tweakers,
        asset_index,
        client_download,
        version_type: vj.version_type.clone(),
        ..VersionFile::default()
    }
}

fn parse_argument_item_enum(item: &ArgumentItem, target: &mut Vec<String>) {
    match item {
        ArgumentItem::Plain(s) => target.push(s.clone()),
        ArgumentItem::WithRules { rules, value } => {
            let rules_converted: Vec<Rule> = rules
                .as_deref()
                .unwrap_or(&[])
                .iter()
                .map(Into::into)
                .collect();
            if crate::platform::should_include(&rules_converted) {
                match value {
                    ArgumentValue::Single(s) => target.push(s.clone()),
                    ArgumentValue::Multiple(arr) => target.extend(arr.clone()),
                }
            }
        }
    }
}

#[must_use]
pub fn parse_requires(vj: &VersionJson) -> Vec<crate::Requirement> {
    let reqs = vj.requires.as_ref().or(vj.underscore_requires.as_ref());
    reqs.cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|r| crate::Requirement {
            uid: r.uid,
            suggests: r.suggests,
            equals: r.equals,
        })
        .collect()
}

#[must_use]
pub fn parse_library(lib: &LibraryJson) -> Vec<Library> {
    let name = lib.name.clone().unwrap_or_default();
    let artifact = lib.downloads.as_ref().and_then(|d| d.artifact.as_ref());

    let url = lib
        .url
        .clone()
        .or_else(|| artifact.and_then(|a| a.url.clone()))
        .or_else(|| (lib.downloads.is_some() && artifact.is_none()).then(String::new));
    let sha1 = lib
        .sha1
        .clone()
        .or_else(|| artifact.and_then(|a| a.sha1.clone()));
    let size = lib.size.or_else(|| artifact.and_then(|a| a.size));

    let rules: Vec<Rule> = lib
        .rules
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .map(Into::into)
        .collect();
    let extract = lib.extract.as_ref().map(|e| Extract {
        exclude: e.exclude.clone().unwrap_or_default(),
    });

    let parts: Vec<&str> = name.split(':').collect();

    if let Some(natives) = &lib.natives {
        let os = crate::platform::current_os();
        if let Some(classifier) = natives.get(os) {
            let classifier = classifier.replace("${arch}", crate::platform::current_arch());
            let native_name = format!("{name}:{classifier}");
            let (native_url, native_sha1, native_size) = lib
                .downloads
                .as_ref()
                .and_then(|d| d.classifiers.as_ref())
                .and_then(|classifiers| classifiers.get(&classifier))
                .map_or_else(
                    || (url.clone(), sha1.clone(), size),
                    |class_info| {
                        (
                            class_info.url.clone(),
                            class_info.sha1.clone(),
                            class_info.size,
                        )
                    },
                );
            return vec![
                Library {
                    name,
                    url,
                    sha1,
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

#[must_use]
pub fn default_java_major_for_version(mc_version: &str) -> u32 {
    if !mc_version.starts_with("1.") {
        return 21;
    }

    let minor: u32 = mc_version
        .strip_prefix("1.")
        .and_then(|s| s.split('.').next())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    match minor {
        0..=16 => 8,
        17..=20 => 17,
        _ => 21,
    }
}
