use crate::install::{download_file_resumable, http_client, maven_library_path, read_version_details, VersionDetails};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

#[derive(Debug, Deserialize)]
struct FabricProfile {
    id: String,
    #[serde(rename = "inheritsFrom")]
    inherits_from: Option<String>,
    #[serde(rename = "mainClass")]
    main_class: String,
    libraries: Vec<FabricLibrary>,
    arguments: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct FabricLibrary {
    name: String,
    url: Option<String>,
}

pub fn mods_dir(root: &Path) -> PathBuf {
    root.join("instances").join("default").join("mods")
}

pub fn has_mods(root: &Path) -> bool {
    let dir = mods_dir(root);
    if !dir.is_dir() {
        return false;
    }
    fs::read_dir(dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .any(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("jar"))
        })
}

pub fn normalize_client_pack_layout(root: &Path) -> Result<(), String> {
    fs::create_dir_all(mods_dir(root)).map_err(|e| e.to_string())?;

    let nested = root.join("mods");
    if nested.is_dir() {
        for entry in fs::read_dir(&nested).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry
                .path()
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("jar"))
            {
                let dest = mods_dir(root).join(entry.file_name());
                if !dest.exists() {
                    fs::copy(entry.path(), &dest).map_err(|e| e.to_string())?;
                }
            }
        }
    }

    Ok(())
}

pub async fn ensure_fabric_loader(
    app: Option<&AppHandle>,
    root: &Path,
    version: &str,
    loader_version: &str,
) -> Result<VersionDetails, String> {
    let version_dir = root.join("versions").join(version);
    let json_path = version_dir.join(format!("{version}.json"));

    let profile_url = format!(
        "https://meta.fabricmc.net/v2/versions/loader/{version}/{loader_version}/profile/json"
    );
    let fabric: FabricProfile = http_client()
        .get(&profile_url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let fabric_ready = fabric_loader_jar_path(root, loader_version).exists();
    let needs_profile_merge = if json_path.exists() {
        read_version_details(&json_path)
            .map(|details| fabric_profile_needs_refresh(&details))
            .unwrap_or(true)
    } else {
        true
    };

    if needs_profile_merge {
        let inherits = fabric.inherits_from.as_deref().unwrap_or(version);
        let vanilla_json = fetch_vanilla_version_json(inherits).await?;
        let merged = merge_profiles(version, &fabric, &vanilla_json)?;

        fs::create_dir_all(&version_dir).map_err(|e| e.to_string())?;
        fs::write(
            &json_path,
            serde_json::to_string_pretty(&merged).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    } else if !fabric_ready {
        log_fabric_line("Fabric profile present but libraries missing, resuming download");
    }

    install_fabric_libraries(app, root, &fabric).await?;

    let jar_path = version_dir.join(format!("{version}.jar"));
    if !jar_path.exists() {
        return Err("Клиент Minecraft не установлен для Fabric".to_string());
    }

    read_version_details(&json_path)
}

async fn fetch_vanilla_version_json(version: &str) -> Result<Value, String> {
    let manifest: Value = http_client()
        .get("https://launchermeta.mojang.com/mc/game/version_manifest_v2.json")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let entry = manifest
        .get("versions")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .find(|v| v.get("id").and_then(|id| id.as_str()) == Some(version))
        .ok_or_else(|| format!("Версия {version} не найдена в манифесте Mojang"))?;

    let url = entry
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Нет URL версии Mojang".to_string())?;

    http_client()
        .get(url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

fn merge_profiles(version: &str, fabric: &FabricProfile, vanilla: &Value) -> Result<Value, String> {
    let mut merged = vanilla.clone();
    if let Some(obj) = merged.as_object_mut() {
        obj.insert("id".to_string(), json!(version));
        obj.insert("mainClass".to_string(), json!(fabric.main_class));
        if let Some(args) = &fabric.arguments {
            obj.insert("arguments".to_string(), merge_arguments(vanilla, args));
        }

        let mut libs = vanilla
            .get("libraries")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let mut seen = HashMap::new();
        for lib in &libs {
            if let Some(name) = lib.get("name").and_then(|v| v.as_str()) {
                seen.insert(name.to_string(), true);
            }
        }

        for lib in &fabric.libraries {
            if seen.contains_key(&lib.name) {
                continue;
            }
            let mut entry = json!({ "name": lib.name });
            if let Some(url) = &lib.url {
                entry["url"] = json!(url);
            }
            libs.push(entry);
            seen.insert(lib.name.clone(), true);
        }

        obj.insert("libraries".to_string(), json!(libs));
    }

    Ok(merged)
}

fn merge_arguments(vanilla: &Value, fabric_args: &Value) -> Value {
    let mut merged = vanilla
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({ "jvm": [], "game": [] }));

    let Some(merged_obj) = merged.as_object_mut() else {
        return merged;
    };
    let Some(fabric_obj) = fabric_args.as_object() else {
        return merged;
    };

    if let Some(fabric_jvm) = fabric_obj.get("jvm").and_then(|v| v.as_array()) {
        let mut jvm = merged_obj
            .get("jvm")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for item in fabric_jvm {
            jvm.push(item.clone());
        }
        merged_obj.insert("jvm".to_string(), json!(jvm));
    }

    if let Some(fabric_game) = fabric_obj.get("game").and_then(|v| v.as_array()) {
        if !fabric_game.is_empty() {
            merged_obj.insert("game".to_string(), json!(fabric_game));
        }
    }

    merged
}

fn fabric_profile_needs_refresh(details: &VersionDetails) -> bool {
    if !details.main_class.contains("KnotClient") {
        return true;
    }
    details
        .arguments
        .as_ref()
        .and_then(|args| args.game.as_ref())
        .map(|game| game.is_empty())
        .unwrap_or(true)
}

fn fabric_loader_jar_path(root: &Path, loader_version: &str) -> PathBuf {
    maven_library_path(root, &format!("net.fabricmc:fabric-loader:{loader_version}"))
        .unwrap_or_else(|| {
            root.join("libraries")
                .join("net/fabricmc/fabric-loader")
                .join(loader_version)
                .join(format!("fabric-loader-{loader_version}.jar"))
        })
}

fn log_fabric_line(message: &str) {
    let _ = (|| -> Result<(), Box<dyn std::error::Error>> {
        let path = crate::profile::launcher_root().join("launcher.log");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        use std::fs::OpenOptions;
        use std::io::Write;
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        writeln!(file, "{message}")?;
        Ok(())
    })();
}

async fn install_fabric_libraries(
    app: Option<&AppHandle>,
    root: &Path,
    fabric: &FabricProfile,
) -> Result<(), String> {
    for lib in &fabric.libraries {
        let Some(dest) = maven_library_path(root, &lib.name) else {
            continue;
        };
        if dest.exists() {
            continue;
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let rel = dest
            .strip_prefix(root.join("libraries"))
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        let base = lib
            .url
            .as_deref()
            .unwrap_or("https://libraries.minecraft.net/");
        let download_url = if base.ends_with('/') {
            format!("{base}{rel}")
        } else {
            format!("{base}/{rel}")
        };
        let label = format!("Fabric: {}", lib.name.rsplit(':').next().unwrap_or("lib"));
        download_file_resumable(app, &download_url, &dest, 95, 98, &label, None, None).await?;
    }
    Ok(())
}
