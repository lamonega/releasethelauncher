# Public Visibility Map (T3.0)

Task T3.0 — definitive per-crate map of `pub` items used cross-crate vs
internal-only, for `crates/{core,auth,net,mods,launch}`.

Reference: `plan.md` §1.3 (error hygiene & public visibility), §2 (dependency
graph), §3 Phase 3 T3.0–T3.5. The T3.x agents (T3.1 core … T3.5 launch) use this
document to downgrade `pub` → `pub(crate)` and to remove dead public surface.

Method: every `pub` item in each crate was inventoried, then its fully-qualified
name and bare identifier were searched workspace-wide (excluding the defining
crate and `target/`). "Cross-crate" means referenced by another crate (ui,
coordinator, a sibling crate, or an integration test in `crates/*/tests`).
"Transitively required" means the item is a public field/return type of a
cross-crate-used type but is never *named* cross-crate.

Note: `pub mod` at the crate root (e.g. `core::hash`) is separate from the
top-level re-exports in `lib.rs` (`pub use hash::{...}`). Some consumers use the
module path, some use the re-export — each line below states which.

---

## 1. Dependency graph (verified from Cargo.toml)

| crate       | depends on                        | anyhow?                    |
|-------------|-----------------------------------|----------------------------|
| `core`      | constants                         | no                         |
| `net`       | constants                         | no                         |
| `auth`      | constants, net                    | no                         |
| `mods`      | constants, core, net              | no                         |
| `launch`    | constants, core, net              | **listed but UNUSED**      |
| `coordinator` | constants, core, auth, mods, launch, net | yes (only consumer) |
| `ui`        | coordinator (+ re-exports)        | no                         |

- `src/main.rs` imports only `coordinator` and `ui`. The five target crates are
  consumed exclusively by `coordinator`/`ui` and by sibling-crate edges
  (auth→net, mods→{core,net}, launch→{core,net}).
- **anyhow dead dependency (confirmed):** `crates/launch/Cargo.toml:17` lists
  `anyhow`, and `crates/launch/src/` never uses it. T3.5 should remove it.
  `crates/coordinator/src/flow/launch.rs` is the only `anyhow` user in the
  workspace (12 call sites) and that usage is deliberate and contained.
- **Packwiz (recommendation: remove outright).** `crates/mods/src/packwiz.rs`
  exposes 9 pub items (`PackwizMod`, `PackwizLauncherData`, `PackwizDownload`,
  `PackwizUpdate`, `PackwizModrinthUpdate`, `PackwizDependency`,
  `save_packwiz_metadata`, `load_packwiz_metadata`,
  `remove_packwiz_metadata`). Workspace-wide search shows **zero callers** — the
  `pub mod packwiz` at `mods/src/lib.rs:3` is the only reference. Delete the
  module in T3.4 (nothing else touches it).

---

## 2. Per-crate maps

Legend for "Action":
- **KEEP** — used cross-crate; must stay `pub`.
- **TRANSITIVE** — never named cross-crate, but a required field/return/error
  type of a cross-crate-used API; keep `pub` (usually a small serde/error enum).
- **pub(crate)** — used only inside the crate (or not at all); safe to narrow.
- **REMOVE** — no callers anywhere in the workspace.

### 2.1 `core` (T3.1)

Modules `archive`, `hash`, `instance`, `log`, `settings` are all reached
cross-crate via module paths → keep `pub mod`.

| item | location | used cross-crate? | action |
|---|---|---|---|
| `archive::extract_zip_with_filter` | archive.rs:24 | yes — `launch/natives.rs:116` | KEEP |
| `archive::read_zip_entry_bytes` | archive.rs:87 | yes — `mods/parser.rs` (6 sites), `launch/fml.rs:100` | KEEP |
| `archive::ArchiveError` | archive.rs:8 | no (error type of used API) | TRANSITIVE |
| `hash::compute_sha1_bytes` | hash.rs:21 | yes — `launch/fml.rs:71,75`, `launch/assets.rs:63`, `coordinator/lib.rs:684` | KEEP |
| `hash::compute_sha1_file` | hash.rs:28 | yes — `launch/fml.rs:121` | KEEP |
| `hash::compute_sha256_bytes` | hash.rs:33 | **no callers anywhere** | pub(crate) / remove |
| `hash::compute_sha256_file` | hash.rs:40 | **no callers anywhere** | pub(crate) / remove |
| `instance::InstanceId` (`type`) | instance.rs:9 | no (in signatures of used methods) | TRANSITIVE |
| `instance::CoreError` | instance.rs:12 | no (error type of used methods) | TRANSITIVE |
| `instance::Instance` | instance.rs:23 | yes via `InstanceManager::get` — coordinator reads `inst.id/root/settings` and calls `mods_dir()`, `minecraft_dir()` | KEEP |
| `Instance::minecraft_dir/mods_dir` | instance.rs:31,36 | yes (coordinator) | KEEP |
| `Instance::index_dir/config_path` | instance.rs:41,46 | no cross-crate callers | pub(crate) |
| `instance::InstanceManager` | instance.rs:51 | yes — coordinator (new, discover, get, list, create, delete, get_mods_dir, update_instance_java_settings) | KEEP |
| `InstanceManager::instances_dir` | instance.rs:58 | no cross-crate callers | pub(crate) |
| `InstanceManager::get_index_dir` | instance.rs:177 | no cross-crate callers | pub(crate) |
| `log::LogEntry` | log.rs:10 | yes — `coordinator` `Event::Log` (via `pub use …log`) | KEEP |
| `log::LogLevel` | log.rs:18 | yes — coordinator, ui (`coordinator::log::LogLevel`) | KEEP |
| `log::LogBuffer` | log.rs:40 | yes — coordinator (`core::log::LogBuffer`); `new/set_log_file_path/push/entries` used | KEEP |
| `settings::SettingsError` | settings.rs:10 | no (error type of `GlobalSettings/InstanceSettings::load`) | TRANSITIVE |
| `settings::ModLoader` | settings.rs:27 | yes — coordinator + ui (variants matched at `flow/launch.rs:637-653`) | KEEP |
| `settings::InstanceSettings` | settings.rs:57 | yes — coordinator (fields `name/minecraft_version/loader/modpack_*`/commands; `new`, `loader_name`) | KEEP |
| `settings::JavaSettings` | settings.rs:95 | yes — `coordinator/dto.rs:4`, ui | KEEP |
| `settings::GlobalSettings` | settings.rs:155 | yes — coordinator + ui (`java_path_for/memory_min_for/memory_max_for/load/save` used) | KEEP |

Top-level re-exports (`core/src/lib.rs:7-11`): consumers use module paths for
`hash::*`, `archive::*`, `log::*`, `settings::{GlobalSettings, ModLoader}`, and
the top-level re-export for `JavaSettings`, `ModLoader`, `GlobalSettings`,
`InstanceManager`, `InstanceSettings`, `compute_sha1_bytes`. The
`compute_sha256_*` re-exports (lib.rs:8) are unused anywhere.

### 2.2 `auth` (T3.2)

Modules `account_list`, `minecraft`, `msa`, `refresh`, `xbox` are reached
cross-crate → keep `pub mod` (all five are used: `refresh::*` and
`xbox::get_xbox_tokens`, `minecraft::complete_microsoft_auth` cross-crate;
`account_list::AccountList` and `msa::MsAuthFlow` via re-export).

| item | location | used cross-crate? | action |
|---|---|---|---|
| `AccountType` | lib.rs:14 | yes — coordinator dto.rs:3, ui (matched) | KEEP |
| `Token` | lib.rs:22 | yes — pub field of `AccountData.mc_token`, read at `coordinator/flow/launch.rs:47,130` | KEEP |
| `Token::new`, `Token::new_no_expiry` | lib.rs:31,42 | no cross-crate callers | pub(crate) |
| `AuthState` | lib.rs:53 | yes — coordinator dto.rs:3, ui (matched) | KEEP |
| `MinecraftProfile` | lib.rs:63 | no named use (pub field of `AccountData.profile`) | TRANSITIVE |
| `Entitlement` | lib.rs:73 | no named use (pub field of `AccountData.entitlement`) | TRANSITIVE |
| `AccountData` | lib.rs:79 | yes — coordinator, ui (`flow/launch.rs:25`, `flow/msa.rs:3`, `lib.rs:62,408,420`, `ui/widgets.rs:96`) | KEEP |
| `AccountData::offline` | lib.rs:102 | yes — coordinator `add_offline_account` | KEEP |
| `AccountData::display_name` | lib.rs:129 | yes — coordinator, ui | KEEP |
| `AccountData::auth_state` | lib.rs:134 | yes — coordinator `accounts()` | KEEP |
| `AccountData::skin_texture_url` | lib.rs:155 | yes — coordinator | KEEP |
| `offline_uuid` | lib.rs:163 | no cross-crate callers | pub(crate) |
| re-export `AccountList` | lib.rs:7 | yes — coordinator (`load/active/add/save/set_active/remove/accounts/active_index`) | KEEP |
| `account_list::AccountListFile` | account_list.rs:11 | no named use (on-disk format) | TRANSITIVE |
| re-export `AuthError` | lib.rs:8 | no (error type of `MsAuthFlow` methods; consumers `map_err(|e| e.to_string())`) | TRANSITIVE (re-export not needed cross-crate) |
| re-export `MsAuthFlow` | lib.rs:8 | yes — `flow/msa.rs:4` (`new_default`, `request_device_code`, `poll_for_token`, `http()`, `client_id()`) | KEEP |
| `MsAuthFlow::new`, `with_http` | msa.rs:53,58 | no cross-crate callers | pub(crate) |
| `MsDeviceCode` | msa.rs:24 | no — coordinator has its own `Event::MsDeviceCode` with same fields; never names `auth::MsDeviceCode` | pub(crate) |
| `MsaTokens` | msa.rs:35 | yes — `.access_token` read at `flow/msa.rs:30` | KEEP |
| `token_from_msa_tokens` | msa.rs:154 | no cross-crate callers | pub(crate) |
| `now_unix` | msa.rs:146 | no cross-crate callers | pub(crate) |
| `refresh::needs_refresh` | refresh.rs:13 | yes — coordinator (`flow/launch.rs:111`, `lib.rs:609`) | KEEP |
| `refresh::try_refresh_if_needed` | refresh.rs:20 | yes — coordinator (`flow/launch.rs:118`, `lib.rs:611`) | KEEP |
| `refresh::refresh_account` | refresh.rs | no cross-crate callers (internal helper) | pub(crate) |
| `xbox::get_xbox_tokens` | xbox.rs | yes — `flow/msa.rs:30` | KEEP |
| `xbox::XboxTokens` | xbox.rs:46 | no named use (passed to `complete_microsoft_auth`; `.user_token` read inside auth) | TRANSITIVE |
| `minecraft::complete_microsoft_auth` | minecraft.rs | yes — `flow/msa.rs:34` | KEEP |
| `minecraft::launcher_login`, `fetch_profile`, `fetch_entitlement` | minecraft.rs | no cross-crate callers (internal steps of `complete_microsoft_auth`) | pub(crate) |

### 2.3 `net` (T3.3)

| item | location | used cross-crate? | action |
|---|---|---|---|
| `pub mod cache` | lib.rs:1 | yes — via `net::cache::{CacheEntry, HttpMetaCache}` from mods | KEEP |
| re-export `CacheEntry`, `HttpMetaCache` | lib.rs:3 | **no** — every consumer uses the module path; re-export unused | remove re-export |
| `cache::CacheEntry` | cache.rs:8 | yes — constructed in `mods/modrinth.rs:510,572` (all fields written) | KEEP |
| `cache::HttpMetaCache` | cache.rs:25 | yes — `mods/modrinth.rs:15`; `load/resolve/update/save` used, `remove/entry` internal | KEEP |
| `NetError` | lib.rs:11 | yes — `launch/lib.rs:124` (`#[from]`) | KEEP |
| `default_client` | lib.rs:25 | yes — `auth/msa.rs:54`, `launch/download.rs:45`, `launch/fml.rs:64`, `launch/resolve/mod.rs:29`, `coordinator/lib.rs:151`, `mods/modrinth.rs:103` | KEEP |
| `HashKind` | lib.rs:34 | yes — `Sha1` in `launch/download.rs:267,437,496`; `Sha512` in `mods/modrinth.rs:220,354,688` | KEEP |
| `HashKind::Sha256` variant | lib.rs:36 | no cross-crate caller (used only inside net's `Hasher`) | keep variant (harmless; part of pub enum) |
| `download_to_file` | lib.rs:79 | yes — `launch/download.rs:269,438,497`, `mods/modrinth.rs` | KEEP |

### 2.4 `mods` (T3.4)

| item | location | used cross-crate? | action |
|---|---|---|---|
| `pub mod modrinth` | lib.rs:1 | yes — coordinator/ui reference `ModrinthProvider` | KEEP |
| `pub mod modrinth_types` | lib.rs:2 | **no** — only used by `modrinth.rs`; module never referenced outside | pub(crate) |
| `pub mod packwiz` | lib.rs:3 | **no callers anywhere** | REMOVE (see §1) |
| `pub mod parser` | lib.rs:4 | yes — `parser::parse_mod_metadata` at `coordinator/lib.rs:256,275` | KEEP |
| `ModsError` | lib.rs:12 | no named use (error type of every pub mods API) | TRANSITIVE |
| `Side` | lib.rs:28 | no named use (field type of `ProjectSummary.side`) | TRANSITIVE |
| `ReleaseType` | lib.rs:46 | yes — variants matched in `ui/new_instance/modrinth.rs:348-350` | KEEP |
| `SortOrder` | lib.rs:64 | yes — `coordinator/flow/modrinth.rs:30,70` | KEEP |
| `SearchArgs` | lib.rs:86 | yes — constructed in `flow/modrinth.rs` | KEEP |
| `SearchResults` | lib.rs:97 | yes — `coordinator` `Event::ModrinthSearchResult` | KEEP |
| `ProjectSummary` | lib.rs:103 | yes — `ui/mod_browser.rs` | KEEP |
| `ProjectInfo` | lib.rs:115 | no cross-crate callers (returned by `ModProvider::get_project`, which has no cross-crate callers) | TRANSITIVE (keep; trait method) |
| `ModVersion` | lib.rs:128 | yes — `Event::ModrinthVersionsResult`, ui | KEEP |
| `InstalledMod` | lib.rs:144 | yes — constructed at `coordinator/lib.rs:685` | KEEP |
| `ModUpdate` | lib.rs:153 | yes — `Event::ModUpdatesResult`, ui | KEEP |
| `ModDetails` | lib.rs:159 | yes — `coordinator/dto.rs:5`, `Event::ModsMetadataResult`, ui | KEEP |
| `ModProvider` trait | lib.rs:171 | yes — imported at `coordinator/lib.rs:678` to call `check_updates`; `search/get_versions/download_mod` via `flow/modrinth.rs`; `get_project` has no cross-crate callers | KEEP |
| `ModEntry` | lib.rs:201 | yes — returned by `list_mods`, fields read at `coordinator/lib.rs:246-258` | KEEP |
| `list_mods` | lib.rs:209 | yes — coordinator (4 sites) | KEEP |
| `enable_mod`, `disable_mod` | lib.rs:246,269 | yes — coordinator `toggle_mod` | KEEP |
| `safe_join_under` | lib.rs:320 | already `pub(crate)` — confirmed internal (T1.1) | — |
| `parser::parse_mod_metadata` | parser.rs:14 | yes — coordinator | KEEP |
| `ModrinthProvider` | modrinth.rs:94 | yes — `new`, `with_client`, and search/versions/download/modpack methods used by `flow/modrinth.rs`, `flow/launch.rs:527,553`, `coordinator/lib.rs:695` | KEEP |
| `ModrinthProvider::with_cache` | modrinth.rs:120 | **no callers anywhere** (only definition) | REMOVE (dead builder step) |
| `modrinth_types::SearchResponse/SearchHit/ModrinthProject/ModrinthVersion/ModrinthFile/MrpackIndex/MrpackFile` | modrinth_types.rs | no (internal serde DTOs) | pub(crate) |
| `packwiz::*` (9 items) | packwiz.rs | **no callers anywhere** | REMOVE |

### 2.5 `launch` (T3.5)

Modules reached cross-crate: `assets` (`AssetManager`), `command`
(`build_command`, `PlayerAuth`), `download` (`DownloadManager`), `fml`
(`ensure_fml_deobfuscation_data`), `java` (`resolve_java`), `memory`
(`has_enough_memory`), `natives` (`extract_natives`, `verify_natives_dir`),
`profile` (`assemble_launch_profile`, `AssetIndex`, `LaunchProfile`), `resolve`
(`DependencyResolver`, `resolve::resolve_dependencies`). **`platform` has no
cross-crate use** → consider `pub(crate)` for the whole module.

| item | location | used cross-crate? | action |
|---|---|---|---|
| `MavenCoord` + methods | lib.rs:25 | no (used by `download.rs`, `command.rs` internally) | pub(crate) |
| `Component` | lib.rs:89 | yes — `Vec<Component>` returned by `flow/launch.rs:616`; fields never read cross-crate | KEEP (type); fields stay pub (read in launch) |
| `Requirement` | lib.rs:99 | no named use (field type of `Component.dependencies`) | TRANSITIVE |
| `LaunchError` | lib.rs:106 | no named use (error type of used APIs) | TRANSITIVE |
| `Library` | lib.rs:128 | no named use (field type of `LaunchProfile.libraries/native_libraries`, read cross-crate at `flow/launch.rs:682-719`) | TRANSITIVE |
| `Rule`, `RuleOs`, `Extract` | lib.rs:139,146,152 | no named use (field types of `Library`) | TRANSITIVE |
| `ClientDownload` | lib.rs:157 | yes — fields `.url/.sha1` read at `flow/launch.rs:379` | KEEP |
| `VersionFile` | lib.rs:164 | no named use (field type of `Component.version_file`) | TRANSITIVE |
| re-export `build_command` | lib.rs:16 | yes — `flow/launch.rs:255` | KEEP |
| re-export `launch_game` | lib.rs:16 | **no callers anywhere** (coordinator spawns the game itself in `spawn_and_stream_output`) | REMOVE |
| re-export `run_pre_launch_command`, `run_post_launch_command` | lib.rs:16 | yes — `flow/launch.rs:582,598` | KEEP |
| re-export `PlayerAuth` | lib.rs:16 | yes — `flow/launch.rs:259` | KEEP |
| re-export `DownloadManager` | lib.rs:18 | yes — methods `new` (377,680,782), `download_client_jar` (379), `download_libraries` (694), `download_asset_index` (772), `download_asset_objects` (785) | KEEP |
| `DownloadManager::with_client`, `libraries_dir`, `local_path_for_library` | download.rs:52,62,115 | no cross-crate callers | pub(crate) |
| re-export `library_filename` | lib.rs:18 | no cross-crate use (internal via `crate::download::library_filename`; `download.rs:83,99,124`, `command.rs:282`) | drop re-export / pub(crate) |
| re-export `ensure_fml_deobfuscation_data` | lib.rs:19 | yes — `flow/launch.rs:320` | KEEP |
| re-export `extract_natives` | lib.rs:20 | yes — `flow/launch.rs:730` | KEEP |
| re-export `verify_natives_dir` | lib.rs:20 | yes — `flow/launch.rs:737` | KEEP |
| re-export `is_native_binary` | lib.rs:20 | no cross-crate use (internal at `natives.rs:25,126`) | drop re-export / pub(crate) |
| re-export `assemble_launch_profile` | lib.rs:21 | yes — `flow/launch.rs:294` + integration test | KEEP |
| re-export `AssetIndex` | lib.rs:21 | yes — `flow/launch.rs:754` | KEEP |
| re-export `LaunchProfile` | lib.rs:21 | yes — fields `.mc_version/.libraries/.native_libraries/.client_download` read cross-crate | KEEP |
| re-export `DependencyResolver` | lib.rs:22 | yes — `new/fetch_manifest/fetch_vanilla_component/fetch_fabric_component/fetch_forge_component/fetch_neoforge_component/fetch_quilt_component/fetch_loader_versions/available_versions_with_types` all called in `flow/launch.rs:617-857` + integration test | KEEP |
| `DependencyResolver::with_client`, `get_version_url`, `available_versions` | resolve/mod.rs:33,49,59 | no cross-crate callers | pub(crate) |
| `resolve::resolve_dependencies` | resolve/mod.rs | yes — `flow/launch.rs:667` + `launch/tests/resolve_152.rs:1` | KEEP |
| `resolve::fabric/forge/loader/neoforge/parsers/prism/quilt` modules + all their pub items (`fetch_manifest`, `fetch_version_metadata`, `fetch_meta_component`, `fetch_meta_loader_versions`, `fetch_forge_component`, `fabric_prism_meta_url`, `quilt_prism_meta_url`, `neoforge_prism_meta_url`, `MavenMetadata`, `Versioning`, `Versions`, `LoaderParams`, `VersionManifest`, `VersionManifestEntry`, `VersionJson`, `ArgumentsJson`, `ArgumentValue`, `ArgumentItem`, `JavaVersionJson`, `AssetIndexJson`, `DownloadsJson`, `MainJarJson`, `RequirementJson`, `LibraryJson`, `LibraryDownloadsJson`, `ArtifactJson`, `RuleJson`, `RuleOsJson`, `ExtractJson`, `parse_*` fns, `default_java_major_for_version`) | resolve/* | no cross-crate use (all internal to launch resolve) | pub(crate) (T3.5); drop the `pub use parsers::{…}` re-export at resolve/mod.rs:12 |
| `memory::has_enough_memory` | memory.rs:5 | yes — `flow/launch.rs:184` | KEEP |
| `java::resolve_java` | java.rs:28 | yes — `flow/launch.rs:827` | KEEP |
| `java::validate_java`, `detect_java_major_version`, `parse_java_version_output` | java.rs:338,379,386 | no cross-crate callers (used inside launch) | pub(crate) |
| `natives::is_native_binary` | natives.rs:9 | no cross-crate callers | pub(crate) |
| `assets::AssetManager` | assets.rs:21 | yes — `new` (765), `parse_asset_index` (800), `reconstruct_virtual_assets` (803) | KEEP |
| `AssetManager::asset_index_path`, `asset_object_path` | assets.rs:36,41 | no cross-crate callers | pub(crate) |
| `assets::AssetIndexJson` | assets.rs:9 | no named use (returned by `parse_asset_index`) | TRANSITIVE |
| `platform::{current_os,current_arch,should_include,should_include_library,classpath_separator}` | platform.rs:5-97 | no cross-crate callers | pub(crate) (module-level) |
| `command::{clean_environment,set_game_env,jvm_args,game_args,DEFAULT_WINDOW_WIDTH,DEFAULT_WINDOW_HEIGHT,run_shell_command}` | command.rs:15-225 | no cross-crate callers (all used internally by `build_command`/pre/post hooks) | pub(crate) |

---

## 3. `let _ =` discard inventory (`crates/{core,auth,net,mods,launch}/src`)

26 hits total. **MUST-FIX: 12** (production code, meaningful errors swallowed).
**BENIGN: 14** (intentional best-effort + test cleanup). auth has **0** hits.

### MUST-FIX (12)

| file:line | code | assessment |
|---|---|---|
| `mods/src/modrinth.rs:521` | `let _ = cache_guard.save();` (search) | cache persistence failure silently dropped → log `tracing::warn!` |
| `mods/src/modrinth.rs:583` | `let _ = cache_guard.save();` (get_versions) | same as above |
| `mods/src/modrinth.rs:809` | `let _ = zip_writer.add_directory(...)` (repack_jar_entries) | repack of untrusted modpack jar; dir entry loss silent |
| `mods/src/modrinth.rs:813` | `let _ = std::io::Write::write_all(&mut zip_writer, &buffer)` | entry content write failure = corrupt repacked jar, silent |
| `mods/src/modrinth.rs:820` | `let _ = zip_writer.finish()` | failing to finalize the repacked jar, silent |
| `mods/src/modrinth.rs:847` | `let _ = fs::create_dir_all(&out_path)` (extract_resource_entries) | untrusted zip; create failure silently skipped (path already guarded by `safe_join_under`) |
| `mods/src/modrinth.rs:850` | `let _ = fs::create_dir_all(parent)` | same as above |
| `mods/src/modrinth.rs:853` | `let _ = std::io::copy(&mut zip_entry, &mut out_file)` | untrusted zip; extract write failure silently skipped |
| `mods/src/modrinth.rs:877` | `let _ = fs::create_dir_all(parent)` (extract_mod_entries) | same |
| `mods/src/modrinth.rs:880` | `let _ = std::io::copy(&mut zip_entry, &mut out_file)` | same |
| `net/src/lib.rs:124` | `let _ = tokio::fs::remove_file(&tmp_dest).await` | temp-file cleanup after checksum mismatch; stale `.tmp` on failure → log `tracing::warn!` |
| `launch/src/download.rs:462` | `let _ = res;` (JoinSet in `download_asset_objects`) | join results discarded; currently always `Ok` because tasks log+swallow their own errors — latent; propagate a count of failures |

### BENIGN (14)

| file:line | code | assessment |
|---|---|---|
| `core/src/log.rs:86` | `let _ = writeln!(...)` (log file write) | intentional — log-write failure must not break the app |
| `core/src/instance.rs:220` | `let _ = fs::remove_dir_all(&temp_dir)` | test cleanup |
| `core/src/instance.rs:238` | `let _ = fs::remove_dir_all(&temp_dir)` | test cleanup |
| `mods/src/modrinth.rs:745` | `let _ = fs::remove_file(path)` | best-effort delete of source modpack zip after unpack (optional to log) |
| `mods/src/modrinth.rs:903` | `let _ = fs::remove_dir_all(&dir)` | test cleanup |
| `mods/src/modrinth.rs:947` | `let _ = fs::remove_dir_all(&root)` | test cleanup |
| `mods/src/lib.rs:377` | `let _ = fs::remove_dir_all(&temp_dir)` | test cleanup |
| `mods/src/lib.rs:385` | `let _ = fs::remove_dir_all(&temp_dir)` | test cleanup |
| `launch/src/download.rs:526` | `let _ = fs::create_dir_all(&path)` (TestDir::new) | test helper |
| `launch/src/download.rs:535` | `let _ = fs::remove_dir_all(&self.0)` (Drop for TestDir) | test helper |
| `launch/src/download.rs:647` | `let _ = stream.read(&mut buf)` | test TCP server |
| `launch/src/download.rs:655` | `let _ = stream.write_all(...)` | test TCP server |
| `launch/src/download.rs:659` | `let _ = stream.write_all(body)` | test TCP server |
| `launch/src/natives.rs:229` | `let _ = fs::remove_dir_all(temp_dir)` | test cleanup |

(For reference only — coordinator, not in the five target crates but in plan
§1.3: `flow/launch.rs:456` `let _ = reader.join();` benign; `lib.rs:75`
`let _ = queue.send(event);` intentional fire-and-forget; `lib.rs:115`
`let _ = std::fs::create_dir_all(&fallback);` benign fallback; `lib.rs:390`
`let _ = id;` unused-parameter discard in `toggle_mod` — smell, remove the param
or use it.)

---

## 4. Dead public surface (no callers anywhere)

1. `launch/src/command.rs:306` `pub async fn launch_game` — zero callers
   workspace-wide (coordinator spawns the game itself). Remove.
2. `mods/src/modrinth.rs:120` `ModrinthProvider::with_cache` — zero callers.
   Remove.
3. `mods/src/packwiz.rs` whole module — zero callers. Remove.
4. `core::hash::{compute_sha256_bytes, compute_sha256_file}` — zero callers.
   `pub(crate)` or remove.
5. `net/src/lib.rs:3` re-export of `CacheEntry`/`HttpMetaCache` — all consumers
   use the `net::cache::` module path. Drop the re-export.
6. `mods/src/lib.rs:2` `pub mod modrinth_types` — internal only. `pub(crate)`.
7. `launch/src/lib.rs:16-22` re-exports of `launch_game` (remove),
   `library_filename`, `is_native_binary` (drop re-exports; keep items
   `pub(crate)` inside launch).
8. `crates/launch/Cargo.toml:17` `anyhow` — dead dependency. Remove in T3.5.

## 5. Summary counts

| crate | pub items surveyed | used cross-crate | internal-only / transitive | `let _ =` MUST-FIX / BENIGN |
|-------|--------------------|------------------|----------------------------|------------------------------|
| core  | 5 modules + 19 re-exports, ~35 items | 22 | ~13 (2 sha256 unused) | 0 / 3 |
| auth  | 5 modules + ~28 items | 15 | 13 | 0 / 0 |
| net   | 1 module + 6 items | 6 | 1 re-export unused, Sha256 variant internal | 1 / 0 |
| mods  | 4 modules + ~40 items | 24 | 16 (packwiz dead) | 10 / 5 |
| launch | 10 modules + ~75 items | 27 | ~48 (resolve/parsers/platform/pub(crate) candidates) | 1 / 6 |
| **total** | | | | **12 / 14** |
