pub mod defaults {
    pub const DEFAULT_MEMORY_MIN: &str = "1024M";
    pub const DEFAULT_MEMORY_MAX: &str = "4096M";
    pub const TIMESTAMP_FORMAT: &str = "%H:%M:%S";
    pub const SETTINGS_FORMAT_VERSION: u32 = 1;
    pub const TOKEN_TTL_24H: u64 = 86_400;
    pub const TOKEN_TTL_1H: u64 = 3_600;
    pub const TOKEN_EXPIRY_BUFFER: u64 = 43_200;
    pub const SLOW_DOWN_DELAY_SECS: u64 = 5;
    pub const MIN_VALID_CACHE_SIZE: u64 = 1_000;
}

pub mod net {
    pub const DOWNLOAD_BUFFER_SIZE: usize = 64 * 1024;
    pub const DEFAULT_MAX_CONCURRENT_DOWNLOADS: usize = 10;
    pub const DEFAULT_MAX_RETRIES: u32 = 3;
    pub const POLL_INTERVAL_SECS: u64 = 5;
    pub const USER_AGENT: &str = "release-the-launcher/0.6.0";
    pub const NET_TIMEOUT_SECS: u64 = 30;
    pub const TEMP_FILE_EXT: &str = "tmp";
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
    pub const MS_DEVICE_CODE_URL: &str =
        "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
    pub const MS_TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
    pub const MS_SCOPES: &str = "XboxLive.SignIn XboxLive.offline_access";
    pub const DEFAULT_MSA_CLIENT_ID: &str = "16e109ad-0414-46dc-8d0f-8d3d4201563c";
    pub const XBL_AUTH_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
    pub const XSTS_AUTH_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
    pub const LAUNCHER_LOGIN_URL: &str = "https://api.minecraftservices.com/launcher/login";
    pub const MC_PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";
    pub const MC_ENTITLEMENT_URL: &str = "https://api.minecraftservices.com/launcher/license";
    pub const MC_TEXTURES_HTTP: &str = "http://textures.minecraft.net";
    pub const MC_ENTITLEMENT_NAME: &str = "game_minecraft";

    pub const MOJANG_LIBRARIES: &str = "https://libraries.minecraft.net";
    pub const MOJANG_RESOURCES: &str = "https://resources.download.minecraft.net";
    pub const FORGE_MAVEN: &str = "https://files.minecraftforge.net/maven";
    pub const FORGE_MAVEN_ALT: &str = "https://maven.minecraftforge.net";
    pub const FABRIC_MAVEN: &str = "https://maven.fabricmc.net";
    pub const NEOFORGE_MAVEN: &str = "https://maven.neoforged.net/releases";
    pub const PRISM_META_BASE: &str = "https://meta.prismlauncher.org/v1";
    pub const MODRINTH_API_URL: &str = "https://api.modrinth.com/v2";
    pub const PRISM_FML_BASE: &str = "https://files.prismlauncher.org/fmllibs";
    pub const WAYBACK_FML_BASE: &str =
        "https://web.archive.org/web/20210118183729id_/http://files.minecraftforge.net/fmllibs";
    pub const S3_MINECRAFT_INDEXES: &str = "https://s3.amazonaws.com/Minecraft.Download/indexes";
    pub const XBOX_SITE_NAME: &str = "user.auth.xboxlive.com";
    pub const XBOX_RELYING_PARTY: &str = "http://auth.xboxlive.com";
}

pub mod auth {
    pub const GRANT_DEVICE_CODE: &str = "urn:ietf:params:oauth:grant-type:device_code";
    pub const GRANT_REFRESH_TOKEN: &str = "refresh_token";
    pub const XSTS_SANDBOX_ID: &str = "RETAIL";
    pub const XSTS_RELYING_PARTY: &str = "rp://api.minecraftservices.com/";

    pub const XERR_NO_PROFILE: u64 = 2_148_916_233;
    pub const XERR_BLOCKED_REGION: u64 = 2_148_916_235;
    pub const XERR_UNDER_AGE: u64 = 2_148_916_238;
    pub const XERR_AGE_PROOF: u64 = 2_148_916_236;
    pub const XERR_BANNED: u64 = 2_148_916_227;
    pub const XERR_RESTRICTED: u64 = 2_148_916_229;
    pub const XERR_TOS: u64 = 2_148_916_234;
}
