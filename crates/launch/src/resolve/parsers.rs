use crate::{AssetIndex, ClientDownload, Extract, Library, Rule, RuleOs, VersionFile};

#[must_use]
pub fn parse_version_json(value: &serde_json::Value) -> VersionFile {
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

    if let Some(jar_mods) = value.get("jarMods").and_then(|v| v.as_array()) {
        for jm in jar_mods {
            libraries.extend(parse_library(jm));
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

    let asset_index = parse_asset_index(value);
    let client_download = parse_client_download(value);

    let compatible_java_majors = value
        .get("javaVersion")
        .and_then(|jv| jv.get("majorVersion"))
        .and_then(serde_json::Value::as_u64)
        .map(|v| vec![u32::try_from(v).unwrap_or(8)])
        .unwrap_or_default();

    let version_type = value
        .get("type")
        .and_then(|v| v.as_str())
        .map(ToString::to_string);

    let mut traits = Vec::new();
    if let Some(tr) = value.get("traits").and_then(|v| v.as_array()) {
        for t in tr {
            if let Some(s) = t.as_str() {
                traits.push(s.to_string());
            }
        }
    }
    if let Some(tr) = value.get("+traits").and_then(|v| v.as_array()) {
        for t in tr {
            if let Some(s) = t.as_str() {
                traits.push(s.to_string());
            }
        }
    }

    VersionFile {
        main_class,
        minecraft_args,
        jvm_args,
        libraries,
        compatible_java_majors,
        tweakers,
        traits,
        asset_index,
        client_download,
        version_type,
        ..VersionFile::default()
    }
}

#[must_use]
pub fn parse_requires(value: &serde_json::Value) -> Vec<crate::Requirement> {
    let mut reqs = Vec::new();
    if let Some(arr) = value.get("requires").and_then(|v| v.as_array()) {
        for r in arr {
            if let Some(uid) = r.get("uid").and_then(|v| v.as_str()) {
                let suggests = r.get("suggests").and_then(|v| v.as_str()).map(ToString::to_string);
                let equals = r.get("equals").and_then(|v| v.as_str()).map(ToString::to_string);
                reqs.push(crate::Requirement {
                    uid: uid.to_string(),
                    suggests,
                    equals,
                });
            }
        }
    }
    reqs
}

fn parse_asset_index(value: &serde_json::Value) -> Option<AssetIndex> {
    value.get("assetIndex").map(|ai| AssetIndex {
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
    })
}

fn parse_client_download(value: &serde_json::Value) -> Option<ClientDownload> {
    if let Some(c) = value.get("downloads").and_then(|d| d.get("client")) {
        return Some(ClientDownload {
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
    }

    if let Some(artifact) = value
        .get("mainJar")
        .and_then(|m| m.get("downloads"))
        .and_then(|d| d.get("artifact"))
    {
        return Some(ClientDownload {
            url: artifact
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            sha1: artifact
                .get("sha1")
                .and_then(|v| v.as_str())
                .map(ToString::to_string),
            size: artifact
                .get("size")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0),
        });
    }

    None
}

#[must_use]
pub fn parse_rules(rules_val: &serde_json::Value) -> Vec<Rule> {
    rules_val
        .as_array()
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
        .unwrap_or_default()
}

#[must_use]
pub fn parse_extract(extract_val: &serde_json::Value) -> Option<Extract> {
    extract_val.as_object().map(|e| Extract {
        exclude: e
            .get("exclude")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(ToString::to_string))
                    .collect()
            })
            .unwrap_or_default(),
    })
}

#[must_use]
pub fn parse_library(lib: &serde_json::Value) -> Vec<Library> {
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

    let rules = lib.get("rules").map_or_else(Vec::new, parse_rules);
    let extract = lib.get("extract").and_then(parse_extract);

    let parts: Vec<&str> = name.split(':').collect();

    // Old format: "natives" field + "downloads.classifiers"
    if let Some(natives) = lib.get("natives").and_then(|v| v.as_object()) {
        let os = crate::platform::current_os();
        if let Some(classifier) = natives.get(os).and_then(|v| v.as_str()) {
            let classifier = classifier.replace("${arch}", crate::platform::current_arch());
            let native_name = format!("{name}:{classifier}");
            let (native_url, native_sha1, native_size) = lib
                .get("downloads")
                .and_then(|d| d.get("classifiers"))
                .and_then(|classifiers| classifiers.get(&classifier))
                .map_or_else(
                    || (url.clone(), sha1.clone(), size),
                    |class_info| {
                        (
                            class_info
                                .get("url")
                                .and_then(|v| v.as_str())
                                .map(ToString::to_string),
                            class_info
                                .get("sha1")
                                .and_then(|v| v.as_str())
                                .map(ToString::to_string),
                            class_info.get("size").and_then(serde_json::Value::as_u64),
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

pub fn parse_argument_item(item: &serde_json::Value, target: &mut Vec<String>) {
    if let Some(s) = item.as_str() {
        target.push(s.to_string());
    } else if let Some(obj) = item.as_object() {
        let rules = obj.get("rules").map_or_else(Vec::new, parse_rules);

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
