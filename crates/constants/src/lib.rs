//! Centralized constants for the whole workspace: filesystem paths
//! ([`paths`]), remote endpoints and user-agent ([`urls`], [`net`]), default
//! values ([`defaults`]).
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::module_name_repetitions,
    clippy::struct_excessive_bools,
    clippy::too_many_lines,
    clippy::doc_markdown,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::similar_names,
    clippy::unused_async
)]

pub mod defaults {
    pub const DEFAULT_MEMORY_MIN: &str = "1024M";
    pub const DEFAULT_MEMORY_MAX: &str = "4096M";
    pub const TIMESTAMP_FORMAT: &str = "%H:%M:%S";
    pub const SETTINGS_FORMAT_VERSION: u32 = 1;
    pub const MIN_VALID_CACHE_SIZE: u64 = 1_000;
}

pub mod net {
    pub const DOWNLOAD_BUFFER_SIZE: usize = 64 * 1024;
    pub const DEFAULT_MAX_CONCURRENT_DOWNLOADS: usize = 10;
    pub const DEFAULT_MAX_RETRIES: u32 = 3;
    pub const POLL_INTERVAL_SECS: u64 = 5;
    pub const USER_AGENT: &str = concat!("release-the-launcher/", env!("CARGO_PKG_VERSION"));
    pub const NET_TIMEOUT_SECS: u64 = 30;
}

pub mod paths {
    pub const APP_DIR_NAME: &str = "release-the-launcher";
    pub const INSTANCES_DIR_NAME: &str = "instances";
    pub const ACCOUNTS_FILE_NAME: &str = "accounts.json";
    pub const SETTINGS_FILE_NAME: &str = "settings.toml";
    pub const LOG_FILE_NAME: &str = "launcher.log";
    /// The instance configuration file. Previously incorrectly named `instance.json`.
    pub const INSTANCE_CONFIG_FILE_NAME: &str = "instance.toml";
    pub const MMC_PACK_FILE_NAME: &str = "mmc-pack.json";
    pub const PACK_TOML_FILE_NAME: &str = "pack.toml";
    pub const MODRINTH_INDEX_FILE: &str = "modrinth.index.json";
    pub const MINECRAFT_DIR: &str = ".minecraft";
    pub const MODS_DIR: &str = "mods";
    pub const CONFIG_DIR: &str = "config";
    pub const SAVES_DIR: &str = "saves";
    pub const RESOURCE_PACKS_DIR: &str = "resourcepacks";
    pub const INDEX_DIR: &str = ".index";
    pub const SERVER_RESOURCE_PACKS_DIR: &str = "server-resource-packs";
}

pub mod urls {
    pub const MOJANG_LIBRARIES: &str = "https://libraries.minecraft.net";
    pub const MOJANG_RESOURCES: &str = "https://resources.download.minecraft.net";
    pub const FORGE_MAVEN: &str = "https://files.minecraftforge.net/maven";
    pub const FORGE_MAVEN_ALT: &str = "https://maven.minecraftforge.net";
    pub const FABRIC_MAVEN: &str = "https://maven.fabricmc.net";
    pub const NEOFORGE_MAVEN: &str = "https://maven.neoforged.net/releases";
    /// Mirror de BMCLAPI para librerías de Mojang when `libraries.minecraft.net` es inaccesible.
    pub const MOJANG_LIBRARIES_MIRROR: &str = "https://bmclapi2.bangbang93.com/maven";
    pub const PRISM_META_BASE: &str = "https://meta.prismlauncher.org/v1";
    pub const MODRINTH_API_URL: &str = "https://api.modrinth.com/v2";
    pub const PRISM_FML_BASE: &str = "https://files.prismlauncher.org/fmllibs";
    pub const WAYBACK_FML_BASE: &str =
        "https://web.archive.org/web/20210118183729id_/http://files.minecraftforge.net/fmllibs";
    pub const S3_MINECRAFT_INDEXES: &str = "https://s3.amazonaws.com/Minecraft.Download/indexes";
}

pub use defaults::*;
pub use net::*;
pub use paths::*;
pub use urls::*;
