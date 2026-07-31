use std::path::Path;

use crate::download::DownloadManager;
use crate::{LaunchError, LaunchProfile};
use reqwest::Client;

struct FmlLibSeed {
    sha1: &'static str,
    url: &'static str,
}

// ponytail: fallback for jars that have no usable hash in fmlversion.properties
// (missing/placeholder). Only 1.5.2 has a checksum-verified copy
// (Prism Launcher mirror / web.archive.org snapshot; the sha1 matches what FML 5.2.23.738
// expects — the file served by files.minecraftforge.net today differs and
// fails FML's check). The 1.5.x family is covered by auto-extraction from the
// universal jar instead.
const FML_LIB_SEEDS: &[(&str, FmlLibSeed)] = &[(
    "1.5.2",
    FmlLibSeed {
        sha1: "446e55cd986582c70fcf12cb27bc00114c5adfd9",
        url: "https://files.prismlauncher.org/fmllibs/deobfuscation_data_1.5.2.zip",
    },
)];

/// FML (pre-1.6) downloads `deobfuscation_data_{mc}.zip` into its home
/// (`~/.minecraft/lib` — FML of that era ignores `--gameDir`) on first launch
/// and aborts on checksum mismatch.
///
/// The upstream file changed since 2013; the expected checksum is read from
/// `fmlversion.properties` inside the downloaded universal jar (baked in for
/// the 1.5.x era), with the static table as fallback. No-op for profiles that
/// don't need it.
///
/// # Errors
///
/// Returns an error if the checksum-verified seed cannot be downloaded or
/// written to FML's home directory.
pub async fn ensure_fml_deobfuscation_data(
    profile: &LaunchProfile,
    instance_root: &Path,
) -> Result<(), LaunchError> {
    if !profile.traits.iter().any(|t| t == "legacyFML") {
        return Ok(());
    }
    let mc = &profile.mc_version;
    let seed = FML_LIB_SEEDS.iter().find(|(m, _)| m == mc).map(|(_, s)| s);
    let expected = deobfuscation_hash_from_jar(profile, instance_root)
        .or_else(|| seed.map(|s| s.sha1.to_string()));
    let Some(expected) = expected else {
        return Ok(());
    };
    let file_name = format!("deobfuscation_data_{mc}.zip");
    let primary_url = seed.map_or_else(
        || format!("https://files.prismlauncher.org/fmllibs/{file_name}"),
        |s| s.url.to_string(),
    );
    let fallback_url = format!(
        "https://web.archive.org/web/20210118183729id_/http://files.minecraftforge.net/fmllibs/{file_name}"
    );

    let lib_dir = dirs::home_dir()
        .ok_or_else(|| LaunchError::Launch("could not resolve home directory".into()))?
        .join(".minecraft")
        .join("lib");
    let target = lib_dir.join(&file_name);
    if sha1_file(&target)?.as_deref() == Some(expected.as_str()) {
        return Ok(());
    }

    let client = Client::new();
    let bytes_opt = match client.get(&primary_url).send().await {
        Ok(resp) if resp.status().is_success() => resp.bytes().await.ok(),
        _ => None,
    };

    let bytes = match bytes_opt {
        Some(b) if release_the_launcher_core::hash::compute_sha1_bytes(&b) == expected => b,
        _ => {
            let resp = client.get(&fallback_url).send().await?;
            let b = resp.bytes().await?;
            let actual = release_the_launcher_core::hash::compute_sha1_bytes(&b);
            if actual != expected {
                return Err(LaunchError::Launch(format!(
                    "FML library {file_name} checksum mismatch: got {actual}, expected {expected}"
                )));
            }
            b
        }
    };

    std::fs::create_dir_all(&lib_dir)?;
    std::fs::write(&target, &bytes)?;
    Ok(())
}

/// Expected deobfuscation checksum from the universal jar's
/// `fmlversion.properties`. Pre-1.6 maven jars carry a `${deobf.checksum}`
/// placeholder (the installer substitutes it), which reads as `None`.
fn deobfuscation_hash_from_jar(profile: &LaunchProfile, instance_root: &Path) -> Option<String> {
    let lib = profile
        .libraries
        .iter()
        .find(|l| l.name.contains("net.minecraftforge:forge"))?;
    let rel = DownloadManager::local_path_for_library(lib)?;
    let jar_path = instance_root.join("libraries").join(rel);
    let bytes = release_the_launcher_core::archive::read_zip_entry_bytes(
        &jar_path,
        "fmlversion.properties",
    )
    .ok()?;
    let content = String::from_utf8_lossy(&bytes);
    parse_deobfuscation_hash(&content)
}

fn parse_deobfuscation_hash(content: &str) -> Option<String> {
    content
        .lines()
        .find_map(|l| l.strip_prefix("fmlbuild.deobfuscation.hash="))
        .map(str::trim)
        .filter(|h| {
            !h.starts_with("${") && h.len() == 40 && h.chars().all(|c| c.is_ascii_hexdigit())
        })
        .map(str::to_string)
}

fn sha1_file(path: &Path) -> Result<Option<String>, LaunchError> {
    match release_the_launcher_core::hash::compute_sha1_file(path) {
        Ok(hash) => Ok(Some(hash)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_file_missing_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(sha1_file(&dir.path().join("nope.zip")).unwrap(), None);
    }

    #[test]
    fn sha1_file_returns_hash() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("a.zip");
        std::fs::write(&f, b"hello").unwrap();
        assert_eq!(
            sha1_file(&f).unwrap().as_deref(),
            Some("aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d")
        );
    }

    #[test]
    fn seed_covers_152() {
        let seed = FML_LIB_SEEDS
            .iter()
            .find(|(mc, _)| *mc == "1.5.2")
            .map(|(_, s)| s)
            .expect("1.5.2 seed");
        assert_eq!(seed.sha1.len(), 40);
    }

    #[test]
    fn parses_real_152_properties() {
        let content = "\
#Mon, 17 Jun 2013 09:33:39 -0600
fmlbuild.major.number=5
fmlbuild.build.number=738
fmlbuild.deobfuscation.hash=446e55cd986582c70fcf12cb27bc00114c5adfd9
";
        assert_eq!(
            parse_deobfuscation_hash(content).as_deref(),
            Some("446e55cd986582c70fcf12cb27bc00114c5adfd9")
        );
    }

    #[test]
    fn rejects_installer_placeholder() {
        let content = "fmlbuild.deobfuscation.hash=${deobf.checksum}\n";
        assert_eq!(parse_deobfuscation_hash(content), None);
    }

    #[test]
    fn rejects_missing_and_garbage() {
        assert_eq!(parse_deobfuscation_hash("fmlbuild.mcversion=1.5.2\n"), None);
        assert_eq!(
            parse_deobfuscation_hash("fmlbuild.deobfuscation.hash=notahex\n"),
            None
        );
    }
}
