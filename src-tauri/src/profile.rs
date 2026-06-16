use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DisplayMode {
    Windowed,
    Fullscreen,
}

impl Default for DisplayMode {
    fn default() -> Self {
        Self::Windowed
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub nickname: String,
    pub api_url: String,
    #[serde(default)]
    pub game_install_paths: HashMap<String, String>,
    #[serde(default)]
    pub selected_servers: HashMap<String, String>,
    #[serde(default)]
    pub display_mode: DisplayMode,
    #[serde(default)]
    pub dev_mode: bool,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            nickname: String::new(),
            api_url: "https://chadow.ru/api/minecraft/bootstrap".to_string(),
            game_install_paths: HashMap::new(),
            selected_servers: HashMap::new(),
            display_mode: DisplayMode::default(),
            dev_mode: false,
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

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let ty = entry.file_type().map_err(|e| e.to_string())?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else if ty.is_file() {
            if let Some(parent) = dst_path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::copy(&src_path, &dst_path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn move_path(src: &Path, dst: &Path) -> Result<(), String> {
    if !src.exists() {
        return Ok(());
    }
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    match fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(_) => {
            if src.is_dir() {
                copy_dir_recursive(src, dst)?;
                fs::remove_dir_all(src).map_err(|e| e.to_string())?;
            } else {
                fs::copy(src, dst).map_err(|e| e.to_string())?;
                fs::remove_file(src).map_err(|e| e.to_string())?;
            }
            Ok(())
        }
    }
}

fn move_minecraft_client_data(old_root: &Path, new_root: &Path) -> Result<(), String> {

    let top_level_dirs = ["versions", "libraries", "assets", "mods", "instances", "natives"];
    for dir_name in top_level_dirs {
        let src = old_root.join(dir_name);
        let dst = new_root.join(dir_name);
        move_path(&src, &dst)?;
    }

    let old_cache = old_root.join(".cache");
    let new_cache = new_root.join(".cache");
    if old_cache.exists() {
        fs::create_dir_all(&new_cache).map_err(|e| e.to_string())?;
        for entry in fs::read_dir(&old_cache).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("applied-pack-") && name_str.ends_with(".sha256") {
                move_path(&entry.path(), &new_cache.join(&name))?;
            }
        }
    }

    Ok(())
}

pub fn relocate_game_install_path(game_id: &str, path: Option<String>) -> Result<PathBuf, String> {
    let old_root = game_install_root(game_id);
    let new_root = match path.as_ref().map(|p| p.trim()).filter(|p| !p.is_empty()) {
        Some(value) => PathBuf::from(value),
        None => default_game_install_root(game_id),
    };

    if old_root == new_root {
        set_game_install_path(game_id, path)?;
        fs::create_dir_all(&new_root).map_err(|e| e.to_string())?;
        return Ok(new_root);
    }

    fs::create_dir_all(&new_root).map_err(|e| e.to_string())?;

    if old_root.exists() {
        if game_id == "minecraft" {
            move_minecraft_client_data(&old_root, &new_root)?;
        } else {
            move_path(&old_root, &new_root)?;
        }
    }

    set_game_install_path(game_id, path)?;
    Ok(new_root)
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
    let mut cleanup_roots: Vec<PathBuf> = vec![default_game_install_root("minecraft")];
    cleanup_roots.extend(
        profile
            .game_install_paths
            .values()
            .map(|path| PathBuf::from(path.trim()))
            .filter(|path| !path.as_os_str().is_empty()),
    );

    cleanup_roots.sort();
    cleanup_roots.dedup();

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

    for root in cleanup_roots {
        if root.exists() {
            let _ = fs::remove_dir_all(&root);
        }
    }

    for path in custom_paths {
        if path.exists() {
            let _ = fs::remove_dir_all(&path);
        }
    }

    Ok(())
}
