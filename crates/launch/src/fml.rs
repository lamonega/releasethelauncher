use crate::{LaunchError, LaunchProfile};
use reqwest::Client;
use sha1::{Digest, Sha1};

struct FmlLibSeed {
    file_name: &'static str,
    sha1: &'static str,
    url: &'static str,
}

// ponytail: only 1.5.2 has a checksum-verified copy (web.archive.org snapshot
// from 2021; the sha1 matches what FML 5.2.23.738 expects — the file served
// by files.minecraftforge.net today differs and fails FML's check). Other
// pre-1.6 versions need verified copies before they can be seeded here.
const FML_LIB_SEEDS: &[(&str, FmlLibSeed)] = &[(
    "1.5.2",
    FmlLibSeed {
        file_name: "deobfuscation_data_1.5.2.zip",
        sha1: "446e55cd986582c70fcf12cb27bc00114c5adfd9",
        url: "https://web.archive.org/web/20210118183729id_/http://files.minecraftforge.net/fmllibs/deobfuscation_data_1.5.2.zip",
    },
)];

/// FML (pre-1.6) downloads `deobfuscation_data_{mc}.zip` into its home
/// (`~/.minecraft/lib` — FML of that era ignores `--gameDir`) on first launch
/// and aborts on checksum mismatch. The upstream file changed since 2013, so
/// legacy Forge cannot boot without seeding the correct copy. No-op for
/// profiles that don't need it.
pub async fn ensure_fml_deobfuscation_data(profile: &LaunchProfile) -> Result<(), LaunchError> {
    if !profile.traits.iter().any(|t| t == "legacyFML") {
        return Ok(());
    }
    let Some(seed) = FML_LIB_SEEDS
        .iter()
        .find(|(mc, _)| *mc == profile.mc_version)
        .map(|(_, s)| s)
    else {
        return Ok(());
    };
    let lib_dir = dirs::home_dir()
        .ok_or_else(|| LaunchError::Launch("could not resolve home directory".into()))?
        .join(".minecraft")
        .join("lib");
    let target = lib_dir.join(seed.file_name);
    if sha1_file(&target)?.as_deref() == Some(seed.sha1) {
        return Ok(());
    }
    let bytes = Client::new().get(seed.url).send().await?.bytes().await?;
    let actual = hex::encode(Sha1::digest(&bytes));
    if actual != seed.sha1 {
        return Err(LaunchError::Launch(format!(
            "FML library {} checksum mismatch: got {actual}, expected {}",
            seed.file_name, seed.sha1
        )));
    }
    std::fs::create_dir_all(&lib_dir)?;
    std::fs::write(&target, &bytes)?;
    Ok(())
}

fn sha1_file(path: &std::path::Path) -> Result<Option<String>, LaunchError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(hex::encode(Sha1::digest(&bytes)))),
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
        assert_eq!(seed.file_name, "deobfuscation_data_1.5.2.zip");
        assert_eq!(seed.sha1.len(), 40);
    }
}
