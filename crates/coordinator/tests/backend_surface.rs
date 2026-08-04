// Architecture guard (plan T3.6): none of the backend crates (`core`, `auth`,
// `net`, `mods`, `launch`) may depend on `anyhow`. `anyhow` is reserved for the
// coordinator, where its ergonomic error handling is contained. If a future
// change re-adds `anyhow` to a backend crate, this test fails loudly.
//
// A dependency-level check is intentionally stricter than "no `anyhow` in the
// public API": if a backend crate does not depend on `anyhow` at all, it cannot
// leak it into its public surface.

use std::fs;
use std::path::PathBuf;

const BACKEND_CRATES: &[&str] = &["core", "auth", "net", "mods", "launch"];

fn workspace_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .expect("coordinator manifest must live in crates/")
        .parent()
        .expect("crates/ must live in the workspace root")
        .to_path_buf()
}

fn declares_anyhow(cargo_toml: &str) -> Option<usize> {
    cargo_toml.lines().position(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("anyhow")
            && trimmed["anyhow".len()..]
                .chars()
                .next()
                .is_some_and(|c| c == '=' || c.is_whitespace())
    })
}

#[test]
fn backend_crates_do_not_depend_on_anyhow() {
    let root = workspace_root();
    for crate_name in BACKEND_CRATES {
        let path = root.join("crates").join(crate_name).join("Cargo.toml");
        let contents = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        if let Some(line) = declares_anyhow(&contents) {
            panic!(
                "crate `{crate_name}` declares a dependency on `anyhow` at line {} of {} \
                 (plan T3.6: `anyhow` is coordinator-only)",
                line + 1,
                path.display()
            );
        }
    }
}
