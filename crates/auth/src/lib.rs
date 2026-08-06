//! Account model and authentication flows: account persistence
//! ([`account_list`]), Microsoft device-code OAuth ([`msa`]), Xbox/Minecraft
//! token exchange ([`xbox`], [`minecraft`]) and refresh handling ([`refresh`]).
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
    clippy::unused_async,
    clippy::redundant_closure_for_method_calls,
    clippy::map_unwrap_or,
    clippy::new_without_default,
    clippy::double_must_use,
    clippy::manual_let_else,
    clippy::single_match_else
)]
pub mod account_list;
pub mod minecraft;
pub mod msa;
pub mod refresh;
pub mod xbox;

pub use account_list::AccountList;
pub use msa::AuthError;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum AccountType {
    #[serde(rename = "microsoft")]
    Microsoft,
    #[default]
    #[serde(rename = "offline")]
    Offline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthState {
    Offline,
    Online,
    Expired,
    Gone,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountData {
    #[serde(rename = "type", default)]
    pub account_type: AccountType,
    #[serde(alias = "internal_id", default)]
    pub id: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub uuid: String,
    #[serde(
        rename = "yggdrasil_token",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub mc_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub refresh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub skin_url: Option<String>,
}

impl AccountData {
    #[must_use]
    pub fn offline(username: &str) -> Self {
        let uuid = offline_uuid(username).to_string();
        Self {
            account_type: AccountType::Offline,
            id: uuid.clone(),
            username: username.to_string(),
            uuid,
            mc_token: None,
            refresh_token: None,
            skin_url: None,
        }
    }

    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.username
    }

    #[must_use]
    pub fn auth_state(&self) -> AuthState {
        match self.account_type {
            AccountType::Offline => AuthState::Offline,
            AccountType::Microsoft => {
                if self.mc_token.is_some() {
                    AuthState::Online
                } else if self.refresh_token.is_some() {
                    AuthState::Expired
                } else {
                    AuthState::Gone
                }
            }
        }
    }

    #[must_use]
    pub fn skin_texture_url(&self) -> Option<String> {
        self.skin_url.clone()
    }
}

/// Computes the offline-mode player UUID using MD5, matching vanilla Minecraft's
/// `nameUUIDFromBytes` algorithm (UUID v3 with the `OfflinePlayer:` prefix).
#[must_use]
pub(crate) fn offline_uuid(username: &str) -> Uuid {
    use md5::Digest;

    let mut bytes: [u8; 16] = md5::Md5::digest(format!("OfflinePlayer:{username}")).into();
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
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
