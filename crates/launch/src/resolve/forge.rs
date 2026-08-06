use super::parsers::{parse_library, VersionJson};
use crate::{Component, LaunchError, Requirement, VersionFile};
use reqwest::Client;

use release_the_launcher_constants::urls;

use super::loader::MavenMetadata;

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

    ensure_forge_compatibility(
        mc_version,
        &full_ver,
        &mut main_class,
        &mut libraries,
        &traits,
    );

    Ok(Component {
        uid: "net.minecraftforge".to_string(),
        version: forge_version.to_string(),
        dependencies: vec![Requirement {
            uid: "net.minecraft".to_string(),
            suggests: Some(mc_version.to_string()),
            equals: Some(mc_version.to_string()),
        }],
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
        format!(
            "{}/net.minecraftforge/{forge_version}.json",
            urls::PRISM_META_BASE
        ),
        format!(
            "{}/net.minecraftforge/{full_ver}.json",
            urls::PRISM_META_BASE
        ),
    ];

    for url in meta_urls {
        if let Ok(resp_res) = client.get(&url).send().await {
            if resp_res.status().is_success() {
                if let Ok(vj) = resp_res.json::<VersionJson>().await {
                    if let Some(main) = &vj.main_class {
                        *main_class = Some(main.clone());
                    }

                    if let Some(libs) = &vj.libraries {
                        for lib in libs {
                            libraries.extend(parse_library(lib));
                        }
                    }

                    if let Some(jar_mods) = &vj.jar_mods {
                        for jm in jar_mods {
                            libraries.extend(parse_library(jm));
                        }
                    }

                    if let Some(tweaks) = &vj.plus_tweakers {
                        for tw in tweaks {
                            tweakers.push(tw.clone());
                        }
                    }

                    if let Some(tr_arr) = &vj.plus_traits {
                        for tr in tr_arr {
                            if !traits.contains(tr) {
                                traits.push(tr.clone());
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

    if tweakers.is_empty() && main_class.is_none() {
        fetch_legacy_installer_metadata(
            client,
            mc_version,
            forge_version,
            main_class,
            libraries,
            tweakers,
        )
        .await;
    }
}

/// Legacy Forge (pre-1.6) publishes its launch metadata only inside the installer jar
/// (`install_profile.json`). Falls back to it when the metadata endpoints come up empty.
async fn fetch_legacy_installer_metadata(
    client: &Client,
    mc_version: &str,
    forge_version: &str,
    main_class: &mut Option<String>,
    libraries: &mut Vec<crate::Library>,
    tweakers: &mut Vec<String>,
) {
    let full_ver = format!("{mc_version}-{forge_version}");
    let urls = [
        format!(
            "{}/net/minecraftforge/forge/{full_ver}/forge-{full_ver}-installer.jar",
            urls::FORGE_MAVEN
        ),
        format!(
            "{}/net/minecraftforge/forge/{full_ver}/forge-{full_ver}-installer.jar",
            urls::FORGE_MAVEN_ALT
        ),
    ];
    for url in urls {
        let Ok(resp) = client.get(&url).send().await else {
            continue;
        };
        if !resp.status().is_success() {
            continue;
        }
        let Ok(bytes) = resp.bytes().await else {
            continue;
        };
        let Ok(mut archive) = zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec())) else {
            continue;
        };
        let Ok(mut entry) = archive.by_name("install_profile.json") else {
            continue;
        };
        let mut content = String::new();
        if std::io::Read::read_to_string(&mut entry, &mut content).is_err() {
            continue;
        }
        if let Ok(vj) = serde_json::from_str::<VersionJson>(&content) {
            parse_installer_version_info(&vj, main_class, libraries, tweakers);
        }
        return;
    }
}

fn parse_installer_version_info(
    vj: &VersionJson,
    main_class: &mut Option<String>,
    libraries: &mut Vec<crate::Library>,
    tweakers: &mut Vec<String>,
) {
    if main_class.is_none() {
        if let Some(mc) = &vj.main_class {
            *main_class = Some(mc.clone());
        }
    }

    if let Some(libs) = &vj.libraries {
        for lib in libs {
            for parsed in parse_library(lib) {
                if parsed.name.starts_with("net.minecraftforge:minecraftforge") {
                    continue;
                }
                if parsed.name.starts_with("org.lwjgl.lwjgl:")
                    || parsed.name.starts_with("net.java.jinput:")
                    || parsed.name.starts_with("net.java.jutils:")
                {
                    continue;
                }
                if !libraries.iter().any(|l| l.name == parsed.name) {
                    libraries.push(parsed);
                }
            }
        }
    }

    if let Some(args) = &vj.minecraft_arguments {
        let mut parts = args.split_whitespace();
        while let Some(arg) = parts.next() {
            if arg == "--tweakClass" {
                if let Some(tweak) = parts.next() {
                    if !tweakers.contains(&tweak.to_string()) {
                        tweakers.push(tweak.to_string());
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

    let is_launchwrapper = (is_mc_1_6_or_newer || traits.iter().any(|t| t == "legacyFML"))
        && (main_class
            .as_deref()
            .unwrap_or("net.minecraft.launchwrapper.Launch")
            == "net.minecraft.launchwrapper.Launch");

    if is_launchwrapper {
        if main_class.is_none() {
            *main_class = Some("net.minecraft.launchwrapper.Launch".to_string());
        }

        if !libraries.iter().any(|l| l.name.contains("launchwrapper")) {
            libraries.push(crate::Library {
                name: "net.minecraft:launchwrapper:1.12".to_string(),
                url: Some(format!(
                    "{}/net/minecraft/launchwrapper/1.12/launchwrapper-1.12.jar",
                    urls::MOJANG_LIBRARIES
                )),
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
                url: Some(format!(
                    "{}/org/ow2/asm/asm-all/5.0.3/asm-all-5.0.3.jar",
                    urls::MOJANG_LIBRARIES
                )),
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
            "{}/net/minecraftforge/forge/{full_ver}/forge-{full_ver}-universal.jar",
            urls::FORGE_MAVEN
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
    let url = format!(
        "{}/net/minecraftforge/forge/maven-metadata.xml",
        urls::FORGE_MAVEN_ALT
    );
    let mut versions = Vec::new();

    if let Ok(resp) = client.get(url).send().await {
        if let Ok(resp_text) = resp.text().await {
            if let Ok(meta) = quick_xml::de::from_str::<MavenMetadata>(&resp_text) {
                let prefix = format!("{mc_version}-");
                let mc_suffix = format!("-{mc_version}");
                for v in meta.versioning.versions.version {
                    let rest = v.strip_prefix(&prefix).unwrap_or(&v);
                    let ver = rest.strip_suffix(&mc_suffix).unwrap_or(rest);
                    if !ver.is_empty() && !versions.contains(&ver.to_string()) {
                        versions.push(ver.to_string());
                    }
                }
            }
        }
    }

    versions.sort_by(|a, b| {
        match (
            version_compare::Version::from(b),
            version_compare::Version::from(a),
        ) {
            (Some(vb), Some(va)) => vb.compare(&va).ord().unwrap_or_else(|| b.cmp(a)),
            _ => b.cmp(a),
        }
    });

    if versions.is_empty() {
        if let Ok(promo_resp) = client
            .get(format!(
                "{}/net/minecraftforge/forge/promotions_slim.json",
                urls::FORGE_MAVEN
            ))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_install_profile() -> serde_json::Value {
        // Trimmed from the real forge-1.5.2-7.8.1.738-installer.jar install_profile.json
        serde_json::json!({
            "install": {},
            "versionInfo": {
                "id": "1.5.2-7.8.1.738",
                "mainClass": "net.minecraft.launchwrapper.Launch",
                "minecraftArguments": "${auth_player_name} ${auth_session} --gameDir ${game_directory} --assetsDir ${game_assets} --tweakClass net.minecraftforge.legacy._1_5_2.LibraryFixerTweaker",
                "libraries": [
                    { "name": "net.minecraftforge:minecraftforge:7.8.1.738", "url": urls::FORGE_MAVEN_ALT },
                    { "name": "org.scala-lang:scala-library:2.10.0-custom", "url": urls::FORGE_MAVEN_ALT },
                    { "name": "net.sourceforge.argo:argo:3.2-small", "url": urls::FORGE_MAVEN_ALT },
                    { "name": "org.bouncycastle:bcprov-jdk15on:148", "url": urls::FORGE_MAVEN_ALT },
                    { "name": "com.google.guava:guava:14.0-rc3", "url": urls::FORGE_MAVEN_ALT },
                    { "name": "net.minecraftforge:legacyfixer:1.0", "url": urls::FORGE_MAVEN_ALT },
                    { "name": "org.ow2.asm:asm-all:4.1" },
                    { "name": "net.minecraft:launchwrapper:1.5" }
                ]
            }
        })
    }

    #[test]
    fn parses_installer_version_info() {
        let mut main_class = None;
        let mut libraries = vec![crate::Library {
            name: "org.ow2.asm:asm-all:4.1".to_string(),
            url: None,
            sha1: None,
            size: None,
            is_native: false,
            rules: vec![],
            extract: None,
        }];
        let mut tweakers = Vec::new();

        let vj: VersionJson =
            serde_json::from_value(sample_install_profile()["versionInfo"].clone()).unwrap();
        parse_installer_version_info(&vj, &mut main_class, &mut libraries, &mut tweakers);

        assert_eq!(
            main_class.as_deref(),
            Some("net.minecraft.launchwrapper.Launch")
        );
        assert_eq!(
            tweakers,
            vec!["net.minecraftforge.legacy._1_5_2.LibraryFixerTweaker"]
        );
        let names: Vec<&str> = libraries.iter().map(|l| l.name.as_str()).collect();
        assert!(names.contains(&"net.minecraftforge:legacyfixer:1.0"));
        assert!(names.contains(&"org.scala-lang:scala-library:2.10.0-custom"));
        assert!(names.contains(&"net.sourceforge.argo:argo:3.2-small"));
        assert!(names.contains(&"org.bouncycastle:bcprov-jdk15on:148"));
        assert!(names.contains(&"com.google.guava:guava:14.0-rc3"));
        assert!(!names.contains(&"net.minecraftforge:minecraftforge:7.8.1.738"));
        assert_eq!(
            names
                .iter()
                .filter(|n| **n == "org.ow2.asm:asm-all:4.1")
                .count(),
            1
        );
    }
}
