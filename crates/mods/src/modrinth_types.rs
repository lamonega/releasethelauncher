use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct SearchResponse {
    pub hits: Vec<SearchHit>,
    pub total_hits: usize,
    #[serde(default)]
    pub _offset: usize,
    pub _limit: usize,
}

#[derive(Debug, Deserialize)]
pub struct SearchHit {
    #[serde(rename = "project_id")]
    pub project_id: String,
    pub title: String,
    pub slug: String,
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(rename = "icon_url")]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub downloads: u64,
}

#[derive(Debug, Deserialize)]
pub struct ModrinthProject {
    pub id: String,
    pub title: String,
    pub slug: String,
    pub description: String,
    #[serde(default)]
    pub team: serde_json::Value,
    #[serde(rename = "icon_url")]
    pub icon_url: Option<String>,
    #[serde(rename = "source_url")]
    pub source_url: Option<String>,
    #[serde(default)]
    pub downloads: u64,
}

#[derive(Debug, Deserialize)]
pub struct ModrinthVersion {
    pub id: String,
    #[serde(rename = "project_id")]
    pub project_id: String,
    pub name: String,
    #[serde(rename = "version_number")]
    pub version_number: String,
    #[serde(rename = "version_type")]
    pub version_type: String,
    #[serde(rename = "game_versions", default)]
    pub game_versions: Vec<String>,
    #[serde(default)]
    pub loaders: Vec<String>,
    #[serde(default)]
    pub files: Vec<ModrinthFile>,
}

#[derive(Debug, Deserialize)]
pub struct ModrinthFile {
    #[serde(default)]
    pub hashes: HashMap<String, String>,
    pub url: Option<String>,
    #[serde(rename = "filename", default)]
    pub filename: String,
    #[serde(default)]
    pub primary: bool,
    #[serde(default)]
    pub size: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MrpackIndex {
    #[serde(default)]
    pub _format_version: u32,
    #[serde(default)]
    pub _game: String,
    #[serde(default)]
    pub _version_id: String,
    #[serde(default)]
    pub _name: String,
    #[serde(default)]
    pub files: Vec<MrpackFile>,
    #[serde(default)]
    pub _dependencies: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MrpackFile {
    pub path: String,
    #[serde(default)]
    pub hashes: HashMap<String, String>,
    #[serde(default)]
    pub downloads: Vec<String>,
    pub file_size: u64,
}
