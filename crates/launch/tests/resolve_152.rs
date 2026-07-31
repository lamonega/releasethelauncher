use release_the_launcher_launch::{assemble_launch_profile, DependencyResolver};

#[tokio::test]
async fn resolve_152_forge_738_profile() {
    let mut resolver = DependencyResolver::new();
    resolver.fetch_manifest().await.unwrap();
    let vanilla = resolver
        .fetch_vanilla_component("1.5.2")
        .await
        .expect("vanilla 1.5.2");
    let forge = resolver
        .fetch_forge_component("1.5.2", "7.8.1.738")
        .await
        .expect("forge 7.8.1.738");

    eprintln!("=== FORGE COMPONENT LIBRARIES ({}) ===", forge.version_file.libraries.len());
    for l in &forge.version_file.libraries {
        eprintln!("  {}", l.name);
    }
    eprintln!("forge main_class={:?}", forge.version_file.main_class);
    eprintln!("forge tweakers={:?}", forge.version_file.tweakers);

    let profile = assemble_launch_profile(&[vanilla, forge]).unwrap();
    let mut names: Vec<String> = profile
        .libraries
        .iter()
        .chain(profile.native_libraries.iter())
        .map(|l| l.name.clone())
        .collect();
    names.sort();
    eprintln!("=== PROFILE LIBRARIES ({}) ===", names.len());
    for n in &names {
        eprintln!("  {n}");
    }
    eprintln!("main_class={}", profile.main_class);
    eprintln!("tweakers={:?}", profile.tweakers);

    assert!(
        names.iter().any(|n| n.starts_with("org.lwjgl.lwjgl:lwjgl:2.9.0")),
        "lwjgl 2.9.0 missing from profile"
    );
    assert!(
        names.iter().any(|n| n.contains("legacyfixer")),
        "legacyfixer missing from profile"
    );
    assert_eq!(
        profile.tweakers,
        vec!["net.minecraftforge.legacy._1_5_2.LibraryFixerTweaker"]
    );
}
