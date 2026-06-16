use crate::install::{download_file_resumable, http_client, read_version_details, VersionDetails};
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
    if json_path.exists() {
        if let Ok(existing) = read_version_details(&json_path) {
            if existing.main_class.contains("KnotClient") {
                return Ok(existing);
            }
        }
    }

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

    let inherits = fabric.inherits_from.as_deref().unwrap_or(version);
    let vanilla_json = fetch_vanilla_version_json(inherits).await?;
    let merged = merge_profiles(version, &fabric, &vanilla_json)?;

    let version_dir = root.join("versions").join(version);
    fs::create_dir_all(&version_dir).map_err(|e| e.to_string())?;
    let json_path = version_dir.join(format!("{version}.json"));
    fs::write(
        &json_path,
        serde_json::to_string_pretty(&merged).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    let _details: VersionDetails = serde_json::from_value(merged).map_err(|e| e.to_string())?;

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
            obj.insert("arguments".to_string(), args.clone());
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

async fn install_fabric_libraries(
    app: Option<&AppHandle>,
    root: &Path,
    fabric: &FabricProfile,
) -> Result<(), String> {
    for lib in &fabric.libraries {
        let parts: Vec<&str> = lib.name.split(':').collect();
        if parts.len() < 3 {
            continue;
        }
        let (group, artifact, version) = (parts[0], parts[1], parts[2]);
        let file_name = if parts.len() > 3 {
            format!("{artifact}-{}-{}.jar", parts[2], parts[3..].join("-"))
        } else {
            format!("{artifact}-{version}.jar")
        };
        let rel = format!("{}/{}/{}/{}", group.replace('.', "/"), artifact, version, file_name);
        let dest = root.join("libraries").join(&rel);
        if dest.exists() {
            continue;
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let base = lib
            .url
            .as_deref()
            .unwrap_or("https://libraries.minecraft.net/");
        let url = if base.ends_with('/') {
            format!("{base}{rel}")
        } else {
            format!("{base}/{rel}")
        };
        let maven_url = if lib.url.as_deref() == Some("https://maven.fabricmc.net/") {
            format!("https://maven.fabricmc.net/{group}/{artifact}/{version}/{file_name}")
        } else {
            url
        };
        download_file_resumable(app, &maven_url, &dest, 0, 0, "", None, None).await?;
    }
    Ok(())
}
