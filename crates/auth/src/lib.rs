pub mod account_list;
pub mod msa;
pub mod xbox;
pub mod minecraft;
pub mod refresh;

pub use account_list::AccountList;
pub use msa::{MsAuthFlow, AuthError};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AccountType {
    #[serde(rename = "microsoft")]
    Microsoft,
    #[serde(rename = "offline")]
    Offline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    pub issue_instant: u64,
    pub not_after: Option<u64>,
    pub token: String,
    pub refresh_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinecraftProfile {
    pub id: String,
    pub name: String,
    pub skin_url: Option<String>,
    pub cape_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entitlement {
    pub owns_minecraft: bool,
    pub can_play_minecraft: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountData {
    #[serde(rename = "type")]
    pub account_type: AccountType,
    pub internal_id: String,
    pub active: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msa_client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msa_token: Option<Token>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_token: Option<Token>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xsts_token: Option<Token>,
    #[serde(rename = "yggdrasil_token", skip_serializing_if = "Option::is_none")]
    pub mc_token: Option<Token>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<MinecraftProfile>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entitlement: Option<Entitlement>,
}

impl AccountData {
    #[must_use]
    pub fn offline(username: &str) -> Self {
        let uuid = offline_uuid(username);
        Self {
            account_type: AccountType::Offline,
            internal_id: uuid.to_string().replace('-', ""),
            active: None,
            msa_client_id: None,
            msa_token: None,
            user_token: None,
            xsts_token: None,
            mc_token: None,
            profile: Some(MinecraftProfile {
                id: uuid.to_string().replace('-', ""),
                name: username.to_string(),
                skin_url: None,
                cape_url: None,
            }),
            entitlement: Some(Entitlement {
                owns_minecraft: true,
                can_play_minecraft: true,
            }),
        }
    }

    #[must_use]
    pub fn display_name(&self) -> &str {
        self.profile
            .as_ref()
            .map_or("Unknown", |p| p.name.as_str())
    }
}

#[must_use]
pub fn offline_uuid(username: &str) -> Uuid {
    let input = format!("OfflinePlayer:{username}");
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();

    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&result[..16]);

    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    Uuid::from_bytes(bytes)
}
