use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct SearchResponse {
    pub hits: Vec<SearchHit>,
    pub total_hits: usize,
    #[serde(default)]
    pub offset: usize,
    pub limit: usize,
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
    pub team: Vec<String>,
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
    #[serde(rename = "game_versions")]
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub files: Vec<ModrinthFile>,
}

#[derive(Debug, Deserialize)]
pub struct ModrinthFile {
    pub hashes: HashMap<String, String>,
    pub url: Option<String>,
    #[serde(rename = "filename")]
    pub filename: String,
    pub primary: bool,
    pub size: u64,
}
