use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub nickname: String,
    pub ram_gb: u32,
    pub api_url: String,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            nickname: String::new(),
            ram_gb: 4,
            api_url: "https://chadow.ru/api/minecraft/bootstrap".to_string(),
        }
    }
}

pub fn game_root() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ChadowGamesLauncher")
}

pub fn profile_path() -> PathBuf {
    game_root().join("profile.json")
}

pub fn bootstrap_cache_path() -> PathBuf {
    game_root().join("bootstrap-cache.json")
}

pub fn ensure_dirs() -> Result<(), String> {
    fs::create_dir_all(game_root()).map_err(|e| e.to_string())
}

pub fn load_profile() -> Profile {
    ensure_dirs().ok();
    let path = profile_path();
    if !path.exists() {
        return Profile::default();
    }
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_profile(profile: &Profile) -> Result<(), String> {
    ensure_dirs()?;
    fs::write(profile_path(), serde_json::to_string_pretty(profile).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

pub fn cache_bootstrap(payload: &serde_json::Value) -> Result<(), String> {
    ensure_dirs()?;
    fs::write(bootstrap_cache_path(), payload.to_string()).map_err(|e| e.to_string())
}

pub fn load_cached_bootstrap() -> Option<serde_json::Value> {
    let path = bootstrap_cache_path();
    if !path.exists() {
        return None;
    }
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}
