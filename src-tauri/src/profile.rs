use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub nickname: String,
    pub api_url: String,
    #[serde(default)]
    pub game_install_paths: HashMap<String, String>,
    #[serde(default)]
    pub selected_servers: HashMap<String, String>,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            nickname: String::new(),
            api_url: "https://chadow.ru/api/minecraft/bootstrap".to_string(),
            game_install_paths: HashMap::new(),
            selected_servers: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameInstallPathInfo {
    pub game_id: String,
    pub path: String,
    pub default_path: String,
    pub is_custom: bool,
}

pub fn launcher_root() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ChadowGamesLauncher")
}

pub fn game_root() -> PathBuf {
    launcher_root()
}

pub fn default_game_install_root(game_id: &str) -> PathBuf {
    if game_id == "minecraft" {
        return launcher_root();
    }
    launcher_root().join("games").join(game_id)
}

pub fn game_install_root(game_id: &str) -> PathBuf {
    let profile = load_profile();
    if let Some(path) = profile.game_install_paths.get(game_id) {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    default_game_install_root(game_id)
}

pub fn game_install_path_info(game_id: &str) -> GameInstallPathInfo {
    let profile = load_profile();
    let default_path = default_game_install_root(game_id);
    let is_custom = profile
        .game_install_paths
        .get(game_id)
        .map(|path| !path.trim().is_empty())
        .unwrap_or(false);
    let path = if is_custom {
        PathBuf::from(profile.game_install_paths.get(game_id).unwrap())
    } else {
        default_path.clone()
    };

    GameInstallPathInfo {
        game_id: game_id.to_string(),
        path: path.to_string_lossy().to_string(),
        default_path: default_path.to_string_lossy().to_string(),
        is_custom,
    }
}

pub fn set_game_install_path(game_id: &str, path: Option<String>) -> Result<(), String> {
    let mut profile = load_profile();
    match path {
        Some(value) if !value.trim().is_empty() => {
            profile
                .game_install_paths
                .insert(game_id.to_string(), value.trim().to_string());
        }
        _ => {
            profile.game_install_paths.remove(game_id);
        }
    }
    save_profile(&profile)
}

pub fn profile_path() -> PathBuf {
    launcher_root().join("profile.json")
}

pub fn bootstrap_cache_path() -> PathBuf {
    launcher_root().join("bootstrap-cache.json")
}

pub fn ensure_dirs() -> Result<(), String> {
    fs::create_dir_all(launcher_root()).map_err(|e| e.to_string())
}

pub fn ensure_game_install_dirs(game_id: &str) -> Result<PathBuf, String> {
    let root = game_install_root(game_id);
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    Ok(root)
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

fn is_subpath(parent: &Path, child: &Path) -> bool {
    child.canonicalize().ok().and_then(|child_abs| {
        parent.canonicalize().ok().map(|parent_abs| child_abs.starts_with(parent_abs))
    }).unwrap_or(false)
}

pub fn clear_all_data() -> Result<(), String> {
    let profile = load_profile();
    let launcher = launcher_root();
    let custom_paths: Vec<PathBuf> = profile
        .game_install_paths
        .values()
        .map(|path| PathBuf::from(path.trim()))
        .filter(|path| !path.as_os_str().is_empty())
        .filter(|path| !is_subpath(&launcher, path))
        .collect();

    if launcher.exists() {
        fs::remove_dir_all(&launcher).map_err(|e| e.to_string())?;
    }

    for path in custom_paths {
        if path.exists() {
            let _ = fs::remove_dir_all(&path);
        }
    }

    Ok(())
}
