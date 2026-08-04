//! Account model and authentication flows: account persistence
//! ([`account_list`]), Microsoft device-code OAuth ([`msa`]), Xbox/Minecraft
//! token exchange ([`xbox`], [`minecraft`]) and refresh handling ([`refresh`]).
pub mod account_list;
pub mod minecraft;
pub mod msa;
pub mod refresh;
pub mod xbox;

pub use account_list::AccountList;
pub use msa::{AuthError, MsAuthFlow};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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

impl Token {
    #[must_use]
    pub(crate) fn new(token: String, refresh_token: Option<String>, expires_in: u64) -> Self {
        let now = msa::now_unix();
        Self {
            issue_instant: now,
            not_after: Some(now + expires_in),
            token,
            refresh_token,
        }
    }

    #[must_use]
    pub(crate) fn new_no_expiry(token: String) -> Self {
        Self {
            issue_instant: msa::now_unix(),
            not_after: None,
            token,
            refresh_token: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthState {
    Offline,
    Online,
    Refreshing,
    Expired,
    Disabled,
    Gone,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinecraftProfile {
    pub id: String,
    pub name: String,
    pub skin_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skin_data: Option<String>,
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
        let id = uuid.simple().to_string();
        Self {
            account_type: AccountType::Offline,
            internal_id: id.clone(),
            active: None,
            msa_client_id: None,
            msa_token: None,
            user_token: None,
            xsts_token: None,
            mc_token: None,
            profile: Some(MinecraftProfile {
                id,
                name: username.to_string(),
                skin_url: None,
                skin_data: None,
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
        self.profile.as_ref().map_or("Unknown", |p| p.name.as_str())
    }

    #[must_use]
    pub fn auth_state(&self) -> AuthState {
        match self.account_type {
            AccountType::Offline => AuthState::Offline,
            AccountType::Microsoft => {
                if self.mc_token.is_some() && self.profile.is_some() {
                    AuthState::Online
                } else if self
                    .msa_token
                    .as_ref()
                    .and_then(|t| t.refresh_token.as_ref())
                    .is_some()
                {
                    AuthState::Expired
                } else {
                    AuthState::Gone
                }
            }
        }
    }

    #[must_use]
    pub fn skin_texture_url(&self) -> Option<String> {
        self.profile.as_ref().and_then(|p| p.skin_url.clone())
    }
}

/// Computes the offline-mode player UUID using MD5, matching vanilla Minecraft's
/// `nameUUIDFromBytes` algorithm (UUID v3 with the `OfflinePlayer:` prefix).
#[must_use]
pub(crate) fn offline_uuid(username: &str) -> Uuid {
    use md5::Digest as _;

    let input = format!("OfflinePlayer:{username}");
    let mut hasher = md5::Md5::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();

    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&result[..16]);

    // Set UUID version to 3 (MD5)
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    // Set UUID variant to RFC 4122
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    Uuid::from_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Known-correct offline UUID for "Notch" computed via vanilla Minecraft's
    /// `nameUUIDFromBytes("OfflinePlayer:Notch".getBytes())` with MD5.
    #[test]
    fn notch_offline_uuid_matches_vanilla() {
        let uuid = offline_uuid("Notch");
        assert_eq!(uuid.to_string(), "b50ad385-829d-3141-a216-7e7d7539ba7f");
    }

    #[test]
    fn jeb_offline_uuid_matches_vanilla() {
        let uuid = offline_uuid("jeb_");
        assert_eq!(uuid.to_string(), "a762f560-4fce-3236-812a-b80efff0b62b");
    }

    #[test]
    fn steve_offline_uuid_matches_vanilla() {
        let uuid = offline_uuid("Steve");
        assert_eq!(uuid.to_string(), "5627dd98-e6be-3c21-b8a8-e92344183641");
    }

    #[test]
    fn offline_uuid_is_v3() {
        let uuid = offline_uuid("TestUser");
        // Version nibble (byte 6, upper 4 bits) should be 3
        assert_eq!(uuid.as_bytes()[6] >> 4, 3);
        // Variant bits (byte 8, upper 2 bits) should be 10
        assert_eq!(uuid.as_bytes()[8] >> 6, 2);
    }
}
