use crate::{Component, LaunchError, Requirement, VersionFile};
use super::parsers::parse_library;
use reqwest::Client;

pub const FORGE_MAVEN: &str = "https://files.minecraftforge.net/maven";

fn parse_version_key(v: &str) -> Vec<u64> {
    v.split(|c: char| !c.is_numeric())
        .filter_map(|s| s.parse::<u64>().ok())
        .collect()
}

/// Fetches the `Forge` component for a given Minecraft and `Forge` version.
///
/// # Errors
///
/// Returns [`LaunchError`] if the HTTP request or JSON parsing fails.
pub async fn fetch_forge_component(
    client: &Client,
    mc_version: &str,
    forge_version: &str,
) -> Result<Component, LaunchError> {
    let forge_version = forge_version
        .strip_suffix(&format!("-{mc_version}"))
        .unwrap_or(forge_version);
    let full_ver = if forge_version.contains(mc_version) {
        forge_version.to_string()
    } else {
        format!("{mc_version}-{forge_version}")
    };

    let mut libraries = Vec::new();
    let mut main_class = None;
    let mut tweakers = Vec::new();
    let mut traits = vec!["legacyFML".to_string()];

    fetch_forge_metadata(
        client,
        mc_version,
        forge_version,
        &mut main_class,
        &mut libraries,
        &mut tweakers,
        &mut traits,
    )
    .await;

    ensure_forge_compatibility(mc_version, &full_ver, &mut main_class, &mut libraries, &traits);

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
            traits,
            tweakers,
            ..VersionFile::default()
        },
    })
}

async fn fetch_forge_metadata(
    client: &Client,
    mc_version: &str,
    forge_version: &str,
    main_class: &mut Option<String>,
    libraries: &mut Vec<crate::Library>,
    tweakers: &mut Vec<String>,
    traits: &mut Vec<String>,
) {
    let full_ver = format!("{mc_version}-{forge_version}");
    let meta_urls = vec![
        format!("https://meta.prismlauncher.org/v1/net.minecraftforge/{forge_version}.json"),
        format!("https://meta.prismlauncher.org/v1/net.minecraftforge/{full_ver}.json"),
        format!("{FORGE_MAVEN}/net/minecraftforge/forge/{full_ver}/forge-{full_ver}-install-profile.json"),
        format!("{FORGE_MAVEN}/net/minecraftforge/forge/{mc_version}/{forge_version}/forge-{mc_version}-install-profile.json"),
    ];

    for url in meta_urls {
        if let Ok(resp_res) = client.get(&url).send().await {
            if resp_res.status().is_success() {
                if let Ok(resp) = resp_res.json::<serde_json::Value>().await {
                    if let Some(main) = resp.get("mainClass").and_then(|v| v.as_str()) {
                        *main_class = Some(main.to_string());
                    } else if let Some(data) = resp.get("data") {
                        if let Some(mc_main) = data
                            .get("MINECRAFT_MAIN_CLASS")
                            .and_then(|v| v.get("client"))
                        {
                            if let Some(s) = mc_main.as_str() {
                                *main_class = Some(s.to_string());
                            }
                        }
                    }

                    if let Some(libs) = resp
                        .get("libraries")
                        .or_else(|| resp.get("versionInfo").and_then(|v| v.get("libraries")))
                        .and_then(|v| v.as_array())
                    {
                        for lib in libs {
                            libraries.extend(parse_library(lib));
                        }
                    }

                    if let Some(jar_mods) = resp.get("jarMods").and_then(|v| v.as_array()) {
                        for jm in jar_mods {
                            libraries.extend(parse_library(jm));
                        }
                    }

                    if let Some(tweaks) = resp.get("+tweakers").and_then(|v| v.as_array()) {
                        for tw in tweaks {
                            if let Some(s) = tw.as_str() {
                                tweakers.push(s.to_string());
                            }
                        }
                    }

                    if let Some(tr_arr) = resp.get("+traits").and_then(|v| v.as_array()) {
                        for tr in tr_arr {
                            if let Some(s) = tr.as_str() {
                                if !traits.contains(&s.to_string()) {
                                    traits.push(s.to_string());
                                }
                            }
                        }
                    }

                    if !libraries.is_empty() {
                        break;
                    }
                }
            }
        }
    }
}

fn ensure_forge_compatibility(
    mc_version: &str,
    full_ver: &str,
    main_class: &mut Option<String>,
    libraries: &mut Vec<crate::Library>,
    traits: &[String],
) {
    let is_mc_1_6_or_newer = !mc_version.starts_with("1.0")
        && !mc_version.starts_with("1.1")
        && !mc_version.starts_with("1.2")
        && !mc_version.starts_with("1.3")
        && !mc_version.starts_with("1.4")
        && !mc_version.starts_with("1.5");

    let is_launchwrapper = is_mc_1_6_or_newer
        && (main_class
            .as_deref()
            .unwrap_or("net.minecraft.launchwrapper.Launch")
            == "net.minecraft.launchwrapper.Launch"
            || traits.iter().any(|t| t == "legacyFML"));

    if is_launchwrapper {
        if main_class.is_none() {
            *main_class = Some("net.minecraft.launchwrapper.Launch".to_string());
        }

        if !libraries.iter().any(|l| l.name.contains("launchwrapper")) {
            libraries.push(crate::Library {
                name: "net.minecraft:launchwrapper:1.12".to_string(),
                url: Some(
                    "https://libraries.minecraft.net/net/minecraft/launchwrapper/1.12/launchwrapper-1.12.jar"
                        .to_string(),
                ),
                sha1: None,
                size: None,
                is_native: false,
                rules: vec![],
                extract: None,
            });
        }

        if !libraries.iter().any(|l| l.name.contains("asm-all")) {
            libraries.push(crate::Library {
                name: "org.ow2.asm:asm-all:5.0.3".to_string(),
                url: Some(
                    "https://libraries.minecraft.net/org/ow2/asm/asm-all/5.0.3/asm-all-5.0.3.jar"
                        .to_string(),
                ),
                sha1: None,
                size: None,
                is_native: false,
                rules: vec![],
                extract: None,
            });
        }
    }

    if !libraries
        .iter()
        .any(|l| l.name.contains("net.minecraftforge:forge"))
    {
        let forge_jar_url = format!(
            "{FORGE_MAVEN}/net/minecraftforge/forge/{full_ver}/forge-{full_ver}-universal.jar"
        );
        libraries.push(crate::Library {
            name: format!("net.minecraftforge:forge:{full_ver}"),
            url: Some(forge_jar_url),
            sha1: None,
            size: None,
            is_native: false,
            rules: vec![],
            extract: None,
        });
    }
}

/// Fetches available `Forge` loader versions for a given Minecraft version.
///
/// # Errors
///
/// Returns [`LaunchError`] if the HTTP request fails.
pub async fn fetch_forge_loader_versions(
    client: &Client,
    mc_version: &str,
) -> Result<Vec<String>, LaunchError> {
    let url = "https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml";
    let resp = client.get(url).send().await?.text().await?;
    let prefix = format!("<version>{mc_version}-");
    let mc_suffix = format!("-{mc_version}");
    let mut versions = Vec::new();
    for line in resp.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(&prefix) && trimmed.ends_with("</version>") {
            let mut ver = trimmed
                .strip_prefix(&prefix)
                .and_then(|s| s.strip_suffix("</version>"))
                .unwrap_or("");
            if let Some(clean) = ver.strip_suffix(&mc_suffix) {
                ver = clean;
            }
            if !ver.is_empty() && !versions.contains(&ver.to_string()) {
                versions.push(ver.to_string());
            }
        }
    }

    versions.sort_by_key(|b| std::cmp::Reverse(parse_version_key(b)));

    if versions.is_empty() {
        if let Ok(promo_resp) = client
            .get("https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json")
            .send()
            .await
        {
            if let Ok(json) = promo_resp.json::<serde_json::Value>().await {
                if let Some(promos) = json.get("promos").and_then(|p| p.as_object()) {
                    let key_latest = format!("{mc_version}-latest");
                    let key_rec = format!("{mc_version}-recommended");
                    if let Some(v) = promos.get(&key_rec).and_then(|v| v.as_str()) {
                        if !versions.contains(&v.to_string()) {
                            versions.push(v.to_string());
                        }
                    }
                    if let Some(v) = promos.get(&key_latest).and_then(|v| v.as_str()) {
                        if !versions.contains(&v.to_string()) {
                            versions.push(v.to_string());
                        }
                    }
                }
            }
        }
    }
    Ok(versions)
}
