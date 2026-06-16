use crate::profile::launcher_root;
use futures_util::StreamExt;
use reqwest::header::RANGE;
use reqwest::StatusCode;
use serde::Deserialize;
use sha1::{Digest as Sha1Digest, Sha1};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientPack {
    pub version: String,
    pub url: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub size: u64,
}

#[derive(Debug, Deserialize)]
struct VersionManifest {
    versions: Vec<VersionEntry>,
}

#[derive(Debug, Deserialize)]
struct VersionEntry {
    id: String,
    url: String,
}

#[derive(Debug, Deserialize)]
pub struct VersionArguments {
    pub jvm: Option<Vec<serde_json::Value>>,
    pub game: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
pub struct VersionDetails {
    #[serde(rename = "mainClass")]
    pub main_class: String,
    #[serde(rename = "assetIndex")]
    pub asset_index: AssetIndex,
    downloads: VersionDownloads,
    pub libraries: Vec<Library>,
    pub arguments: Option<VersionArguments>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AssetIndex {
    pub id: String,
    url: String,
}

#[derive(Debug, Deserialize)]
struct VersionDownloads {
    client: DownloadEntry,
}

#[derive(Debug, Deserialize)]
struct DownloadEntry {
    url: String,
    #[serde(default)]
    sha1: String,
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Library {
    name: String,
    downloads: Option<LibraryDownloads>,
    rules: Option<Vec<LibraryRule>>,
    #[allow(dead_code)]
    natives: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize)]
struct LibraryDownloads {
    artifact: Option<DownloadEntry>,
    classifiers: Option<HashMap<String, DownloadEntry>>,
}

#[derive(Debug, Deserialize)]
struct LibraryRule {
    action: String,
    os: Option<LibraryRuleOs>,
}

#[derive(Debug, Deserialize)]
struct LibraryRuleOs {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AssetIndexFile {
    objects: HashMap<String, AssetObject>,
}

#[derive(Debug, Deserialize)]
struct AssetObject {
    hash: String,
}

fn log_line(message: &str) {
    let _ = (|| -> Result<(), Box<dyn std::error::Error>> {
        let path = launcher_root().join("launcher.log");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        writeln!(file, "{message}")?;
        Ok(())
    })();
}

pub async fn ensure_java(
    app: &AppHandle,
    major: u32,
    emit: impl Fn(u8, &str),
) -> Result<PathBuf, String> {
    let java_dir = launcher_root().join("java").join(format!("{major}"));
    if let Some(home) = find_java_home(&java_dir) {
        return Ok(java_home_bin(&home));
    }

    // Broken partial install (e.g. only bin/ copied without lib/)
    if java_dir.exists() {
        log_line(&format!("Removing broken Java {major} install"));
        let _ = fs::remove_dir_all(&java_dir);
    }

    emit(5, "Скачивание Java…");
    log_line(&format!("Downloading Java {major}"));
    fs::create_dir_all(&java_dir).map_err(|e| e.to_string())?;

    let url = format!(
        "https://api.adoptium.net/v3/binary/latest/{major}/ga/windows/x64/jdk/hotspot/normal/eclipse?project=jdk"
    );
    let zip_path = java_dir.join("jdk.zip");
    download_file_resumable(
        Some(app),
        &url,
        &zip_path,
        5,
        35,
        "Java",
        None,
        None,
    )
    .await?;

    emit(40, "Распаковка Java…");
    extract_zip(&zip_path, &java_dir)?;
    let _ = fs::remove_file(&zip_path);

    find_java_home(&java_dir)
        .map(|home| java_home_bin(&home))
        .ok_or_else(|| "Java не найдена после установки".to_string())
}

fn is_java_home_valid(home: &Path) -> bool {
    home.join("bin").join("java.exe").exists() && home.join("lib").join("jvm.cfg").exists()
}

fn java_home_bin(home: &Path) -> PathBuf {
    home.join("bin").join("java.exe")
}

fn find_java_home(java_dir: &Path) -> Option<PathBuf> {
    if is_java_home_valid(java_dir) {
        return Some(java_dir.to_path_buf());
    }

    let entries = fs::read_dir(java_dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.file_type().ok()?.is_dir() && is_java_home_valid(&path) {
            return Some(path);
        }
    }
    None
}

fn client_pack_stamp_path(root: &Path, version: &str) -> PathBuf {
    root.join(".cache").join(format!("applied-pack-{version}.sha256"))
}

fn read_applied_client_pack_sha256(root: &Path, version: &str) -> Option<String> {
    let path = client_pack_stamp_path(root, version);
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty())
}

fn write_applied_client_pack_sha256(root: &Path, version: &str, sha256: &str) -> Result<(), String> {
    let sha256 = sha256.trim().to_lowercase();
    if sha256.is_empty() {
        return Ok(());
    }
    let path = client_pack_stamp_path(root, version);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(path, sha256).map_err(|e| e.to_string())
}

pub(crate) fn client_pack_needs_update(root: &Path, version: &str, pack: &ClientPack) -> bool {
    if pack.version != version || pack.url.trim().is_empty() {
        return false;
    }
    let expected = pack.sha256.trim().to_lowercase();
    if expected.is_empty() {
        return true;
    }
    read_applied_client_pack_sha256(root, version).as_deref() != Some(expected.as_str())
}

pub async fn ensure_minecraft(
    app: &AppHandle,
    install_root: &Path,
    version: &str,
    client_pack: Option<&ClientPack>,
    emit: impl Fn(u8, &str),
) -> Result<(PathBuf, VersionDetails), String> {
    let root = install_root.to_path_buf();
    let version_dir = root.join("versions").join(version);
    let jar_path = version_dir.join(format!("{version}.jar"));
    let json_path = version_dir.join(format!("{version}.json"));

    if jar_path.exists() && json_path.exists() {
        if let Ok(mut details) = read_version_details(&json_path) {
            let pack_update = client_pack
                .map(|pack| client_pack_needs_update(&root, version, pack))
                .unwrap_or(false);
            if is_version_installed(&root, version, &details) && !pack_update {
                if crate::fabric::has_mods(&root) {
                    emit(95, "Подготовка Fabric…");
                    details = crate::fabric::ensure_fabric_loader(Some(app), &root, version, "0.18.2")
                        .await?;
                }
                emit(99, "Клиент уже установлен");
                return Ok((jar_path, details));
            }
            if pack_update {
                log_line(&format!("Client pack update available for {version}"));
            } else {
                log_line(&format!("Incomplete install for {version}, resuming download"));
            }
        }
    }

    let mut client_pack_overlay = false;
    if let Some(pack) = client_pack {
        if pack.version == version && !pack.url.trim().is_empty() {
            match download_and_extract_client_pack(app, &root, version, pack, &emit).await {
                Ok(ClientPackResult::Full(details)) => {
                    let details = apply_fabric_if_mods(app, &root, version, details, &emit).await?;
                    return Ok((jar_path, details));
                }
                Ok(ClientPackResult::Overlay) => {
                    client_pack_overlay = true;
                    log_line("Client pack overlay: mods only");
                }
                Err(err) => {
                    log_line(&format!("Client pack install failed: {err}, falling back to Mojang CDN"));
                    emit(45, "Архив недоступен, загрузка с Mojang…");
                }
            }
        }
    }

    if client_pack_overlay
        && jar_path.exists()
        && json_path.exists()
        && is_version_installed(
            &root,
            version,
            &read_version_details(&json_path).map_err(|e| e.to_string())?,
        )
    {
        emit(95, "Подготовка Fabric…");
        let details = crate::fabric::ensure_fabric_loader(Some(app), &root, version, "0.18.2").await?;
        return Ok((jar_path, details));
    }

    emit(45, "Загрузка манифеста Minecraft…");
    fs::create_dir_all(&version_dir).map_err(|e| e.to_string())?;

    let manifest: VersionManifest = http_client()
        .get("https://launchermeta.mojang.com/mc/game/version_manifest_v2.json")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let entry = manifest
        .versions
        .into_iter()
        .find(|v| v.id == version)
        .ok_or_else(|| format!("Версия {version} не найдена в манифесте Mojang"))?;

    let version_json = http_client()
        .get(&entry.url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;

    fs::write(&json_path, &version_json).map_err(|e| e.to_string())?;

    let details: VersionDetails =
        serde_json::from_str(&version_json).map_err(|e| format!("Ошибка разбора version.json: {e}"))?;
    let mut details = details;

    emit(50, "Скачивание клиента…");
    download_file_resumable(
        Some(app),
        &details.downloads.client.url,
        &jar_path,
        50,
        58,
        "Клиент",
        Some(&details.downloads.client.sha1),
        None,
    )
    .await?;

    emit(59, "Скачивание библиотек…");
    install_libraries(app, &root, &details.libraries, 59, 82).await?;

    emit(83, "Скачивание ресурсов…");
    install_assets(app, &root, &details.asset_index, 83, 99).await?;

    if crate::fabric::has_mods(&root) {
        emit(95, "Подготовка Fabric…");
        details = crate::fabric::ensure_fabric_loader(Some(app), &root, version, "0.18.2").await?;
    }

    Ok((jar_path, details))
}

async fn apply_fabric_if_mods(
    app: &AppHandle,
    root: &Path,
    version: &str,
    details: VersionDetails,
    emit: &impl Fn(u8, &str),
) -> Result<VersionDetails, String> {
    if crate::fabric::has_mods(root) {
        emit(95, "Подготовка Fabric…");
        crate::fabric::ensure_fabric_loader(Some(app), root, version, "0.18.2").await
    } else {
        Ok(details)
    }
}

enum ClientPackResult {
    Full(VersionDetails),
    Overlay,
}

async fn download_and_extract_client_pack(
    app: &AppHandle,
    root: &Path,
    version: &str,
    pack: &ClientPack,
    emit: &impl Fn(u8, &str),
) -> Result<ClientPackResult, String> {
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let cache_dir = root.join(".cache");
    fs::create_dir_all(&cache_dir).map_err(|e| e.to_string())?;
    let zip_path = cache_dir.join(format!("client-{version}.zip"));

    emit(46, "Скачивание архива клиента…");
    log_line(&format!("Downloading client pack from {}", pack.url));

    let skip_download = if zip_path.exists() && !pack.sha256.is_empty() {
        verify_file_sha256(&zip_path, &pack.sha256).is_ok()
    } else {
        false
    };

    if !skip_download {
        download_file_resumable(
            Some(app),
            &pack.url,
            &zip_path,
            46,
            88,
            "Архив клиента",
            None,
            Some(&pack.sha256),
        )
        .await?;
    }

    if !pack.sha256.is_empty() {
        verify_file_sha256(&zip_path, &pack.sha256)?;
    }

    emit(90, "Распаковка клиента…");
    extract_game_pack(&zip_path, root)?;
    let _ = fs::remove_file(&zip_path);
    crate::fabric::normalize_client_pack_layout(root)?;

    let json_path = root.join("versions").join(version).join(format!("{version}.json"));
    if !json_path.exists() {
        if crate::fabric::has_mods(root) {
            return Ok(ClientPackResult::Overlay);
        }
        return Err("В архиве нет version.json".to_string());
    }

    let details = read_version_details(&json_path)?;
    emit(92, "Подготовка natives…");
    extract_all_natives(root, &details.libraries)?;

    if !is_version_installed(root, version, &details) {
        return Err("Архив распакован не полностью".to_string());
    }

    write_applied_client_pack_sha256(root, version, &pack.sha256)?;

    emit(99, "Клиент установлен из архива");
    Ok(ClientPackResult::Full(details))
}

async fn install_from_client_pack(
    app: &AppHandle,
    root: &Path,
    version: &str,
    pack: &ClientPack,
    emit: &impl Fn(u8, &str),
) -> Result<VersionDetails, String> {
    match download_and_extract_client_pack(app, root, version, pack, emit).await? {
        ClientPackResult::Full(details) => apply_fabric_if_mods(app, root, version, details, emit).await,
        ClientPackResult::Overlay => Err("В архиве нет version.json".to_string()),
    }
}

fn verify_file_sha1(path: &Path, expected: &str) -> Result<(), String> {
    let expected = expected.trim().to_lowercase();
    if expected.is_empty() {
        return Ok(());
    }

    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha1::new();
    let mut buffer = [0u8; 65536];
    loop {
        let read = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    let hash = format!("{:x}", hasher.finalize());
    if hash != expected {
        return Err(format!(
            "SHA1 не совпадает для {}",
            path.file_name().unwrap_or_default().to_string_lossy()
        ));
    }
    Ok(())
}

fn is_file_valid(path: &Path, expected_sha1: Option<&str>, expected_sha256: Option<&str>) -> bool {
    if !path.exists() {
        return false;
    }
    if path.metadata().map(|m| m.len()).unwrap_or(0) == 0 {
        return false;
    }
    if let Some(expected) = expected_sha256.filter(|value| !value.trim().is_empty()) {
        return verify_file_sha256(path, expected).is_ok();
    }
    if let Some(expected) = expected_sha1.filter(|value| !value.trim().is_empty()) {
        return verify_file_sha1(path, expected).is_ok();
    }
    true
}

async fn remote_content_length(url: &str) -> Option<u64> {
    http_client()
        .head(url)
        .send()
        .await
        .ok()
        .filter(|response| response.status().is_success())
        .and_then(|response| response.content_length())
}

pub(crate) async fn download_file_resumable(
    app: Option<&AppHandle>,
    url: &str,
    dest: &Path,
    from: u8,
    to: u8,
    label: &str,
    expected_sha1: Option<&str>,
    expected_sha256: Option<&str>,
) -> Result<(), String> {
    if is_file_valid(dest, expected_sha1, expected_sha256) {
        if let Some(app) = app {
            emit_progress(app, to, format!("{label} уже загружен"));
        }
        return Ok(());
    }

    let remote_size = remote_content_length(url).await;
    prepare_partial_download(dest, remote_size, expected_sha1, expected_sha256)?;

    if let Some(app) = app {
        emit_progress(app, from, label.to_string());
    }

    let client = http_client_long();
    let mut offset = dest.metadata().map(|m| m.len()).unwrap_or(0);

    loop {
        let mut request = client.get(url);
        if offset > 0 {
            request = request.header(RANGE, format!("bytes={offset}-"));
        }

        let response = request.send().await.map_err(|e| e.to_string())?;
        let status = response.status();

        if offset > 0 && status == StatusCode::OK {
            fs::remove_file(dest).map_err(|e| e.to_string())?;
            offset = 0;
            continue;
        }

        if !status.is_success() && status != StatusCode::PARTIAL_CONTENT {
            return Err(format!("HTTP {status} для {url}"));
        }

        let remainder = response.content_length().unwrap_or(0);
        let total_expected: Option<u64> = if offset > 0 {
            Some(remote_size.unwrap_or(offset + remainder))
        } else {
            remote_size.or(if remainder > 0 { Some(remainder) } else { None })
        };

        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let mut file = if offset > 0 {
            OpenOptions::new()
                .append(true)
                .open(dest)
                .map_err(|e| e.to_string())?
        } else {
            fs::File::create(dest).map_err(|e| e.to_string())?
        };

        let mut downloaded = offset;
        let started_at = Instant::now();
        let mut last_emit_at = Instant::now();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| e.to_string())?;
            file.write_all(&chunk).map_err(|e| e.to_string())?;
            downloaded += chunk.len() as u64;

            if let Some(app) = app {
                if let Some(total) = total_expected {
                    if total > 0 {
                        let ratio = downloaded as f64 / total as f64;
                        let pct = from + ((to.saturating_sub(from)) as f64 * ratio) as u8;
                        let elapsed = started_at.elapsed().as_secs_f64().max(0.001);
                        let speed = downloaded as f64 / elapsed;
                        let now = Instant::now();
                        if now.duration_since(last_emit_at) >= Duration::from_millis(200) || downloaded >= total {
                            last_emit_at = now;
                            emit_progress(
                                app,
                                pct,
                                format!(
                                    "{label} {ratio_pct:.1}% · {done_mb:.1}/{total_mb:.1} MB · {speed_mb:.2} MB/s",
                                    ratio_pct = ratio * 100.0,
                                    done_mb = downloaded as f64 / (1024.0 * 1024.0),
                                    total_mb = total as f64 / (1024.0 * 1024.0),
                                    speed_mb = speed / (1024.0 * 1024.0),
                                ),
                            );
                        }
                    }
                }
            }
        }

        break;
    }

    if !is_file_valid(dest, expected_sha1, expected_sha256) {
        let _ = fs::remove_file(dest);
        return Err(format!(
            "Файл повреждён после загрузки: {}",
            dest.file_name().unwrap_or_default().to_string_lossy()
        ));
    }

    if let Some(app) = app {
        emit_progress(app, to, format!("{label} готово"));
    }
    Ok(())
}

fn prepare_partial_download(
    dest: &Path,
    remote_size: Option<u64>,
    expected_sha1: Option<&str>,
    expected_sha256: Option<&str>,
) -> Result<(), String> {
    if !dest.exists() {
        return Ok(());
    }

    let local_len = dest.metadata().map(|m| m.len()).unwrap_or(0);
    if local_len == 0 {
        return Ok(());
    }

    if let Some(remote) = remote_size {
        if local_len > remote {
            fs::remove_file(dest).map_err(|e| e.to_string())?;
            return Ok(());
        }
        if local_len == remote && !is_file_valid(dest, expected_sha1, expected_sha256) {
            fs::remove_file(dest).map_err(|e| e.to_string())?;
        }
        return Ok(());
    }

    if is_file_valid(dest, expected_sha1, expected_sha256) {
        return Ok(());
    }

    if expected_sha1.is_some() || expected_sha256.is_some() {
        fs::remove_file(dest).map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn verify_file_sha256(path: &Path, expected: &str) -> Result<(), String> {
    let expected = expected.trim().to_lowercase();
    if expected.is_empty() {
        return Ok(());
    }

    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let read = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    let hash = format!("{:x}", hasher.finalize());
    if hash != expected {
        return Err("Контрольная сумма архива не совпадает".to_string());
    }
    Ok(())
}

fn extract_game_pack(zip_path: &Path, dest: &Path) -> Result<(), String> {
    let file = fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let outpath = match file.enclosed_name() {
            Some(path) => dest.join(path),
            None => continue,
        };
        if outpath.exists() {
            if let Ok(existing) = fs::metadata(&outpath) {
                if existing.len() == file.size() {
                    continue;
                }
            }
            fs::remove_file(&outpath).ok();
        }
        if file.name().ends_with('/') {
            fs::create_dir_all(&outpath).map_err(|e| e.to_string())?;
        } else {
            if let Some(p) = outpath.parent() {
                fs::create_dir_all(p).map_err(|e| e.to_string())?;
            }
            let mut outfile = fs::File::create(&outpath).map_err(|e| e.to_string())?;
            std::io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn extract_all_natives(root: &Path, libraries: &[Library]) -> Result<(), String> {
    let natives_dir = root.join("natives");
    fs::create_dir_all(&natives_dir).map_err(|e| e.to_string())?;

    for lib in libraries {
        if !library_allowed(lib) {
            continue;
        }
        if is_native_library(&lib.name) {
            if let Some(artifact) = library_artifact(lib) {
                let path = library_destination(root, lib, artifact);
                if path.exists() {
                    let _ = extract_native_jar(&path, &natives_dir);
                }
            }
            continue;
        }

        if let Some(downloads) = &lib.downloads {
            if let Some(classifiers) = &downloads.classifiers {
                for key in ["natives-windows", "natives-windows-x64"] {
                    if let Some(native) = classifiers.get(key) {
                        let native_path = artifact_destination(root, native);
                        if native_path.exists() {
                            let _ = extract_native_jar(&native_path, &natives_dir);
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

pub(crate) fn read_version_details(json_path: &Path) -> Result<VersionDetails, String> {
    let raw = fs::read_to_string(json_path).map_err(|e| e.to_string())?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

fn is_version_installed(root: &Path, version: &str, details: &VersionDetails) -> bool {
    let jar = root.join("versions").join(version).join(format!("{version}.jar"));
    if !jar.exists() || jar.metadata().map(|m| m.len()).unwrap_or(0) < 1024 {
        return false;
    }

    for lib in &details.libraries {
        if !library_allowed(lib) {
            continue;
        }
        let Some(path) = library_file_path(root, lib) else {
            continue;
        };
        if !path.exists() {
            log_line(&format!("Missing library: {}", lib.name));
            return false;
        }
    }

    let index_path = root
        .join("assets")
        .join("indexes")
        .join(format!("{}.json", details.asset_index.id));
    index_path.exists()
}

async fn install_libraries(
    app: &AppHandle,
    root: &Path,
    libraries: &[Library],
    from: u8,
    to: u8,
) -> Result<(), String> {
    let applicable: Vec<&Library> = libraries.iter().filter(|lib| library_allowed(lib)).collect();
    let total = applicable.len().max(1);
    let natives_dir = root.join("natives");

    for (i, lib) in applicable.iter().enumerate() {
        let pct = from + ((i as u8).saturating_mul(to.saturating_sub(from)) / total as u8);
        emit_progress(app, pct, format!("Библиотеки ({}/{})", i + 1, total));

        let Some(artifact) = library_artifact(lib) else {
            continue;
        };
        let path = library_destination(root, lib, artifact);
        if path.exists() {
            let valid = if !artifact.sha1.is_empty() {
                verify_file_sha1(&path, &artifact.sha1).is_ok()
            } else {
                path.metadata().map(|m| m.len()).unwrap_or(0) > 0
            };
            if valid {
                if is_native_library(&lib.name) {
                    let _ = extract_native_jar(&path, &natives_dir);
                }
                continue;
            }
            let _ = fs::remove_file(&path);
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let sha1 = if artifact.sha1.is_empty() {
            None
        } else {
            Some(artifact.sha1.as_str())
        };
        download_file_resumable(
            None,
            &artifact.url,
            &path,
            0,
            0,
            "",
            sha1,
            None,
        )
        .await?;
        log_line(&format!("Downloaded {}", lib.name));

        if is_native_library(&lib.name) {
            extract_native_jar(&path, &natives_dir)?;
        }

        if let Some(downloads) = &lib.downloads {
            if let Some(classifiers) = &downloads.classifiers {
                for key in ["natives-windows", "natives-windows-x64"] {
                    if let Some(native) = classifiers.get(key) {
                        let native_path = artifact_destination(root, native);
                        if native_path.exists() {
                            let valid = if !native.sha1.is_empty() {
                                verify_file_sha1(&native_path, &native.sha1).is_ok()
                            } else {
                                native_path.metadata().map(|m| m.len()).unwrap_or(0) > 0
                            };
                            if !valid {
                                let _ = fs::remove_file(&native_path);
                            }
                        }
                        if !native_path.exists() {
                            if let Some(parent) = native_path.parent() {
                                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                            }
                            let sha1 = if native.sha1.is_empty() {
                                None
                            } else {
                                Some(native.sha1.as_str())
                            };
                            download_file_resumable(
                                None,
                                &native.url,
                                &native_path,
                                0,
                                0,
                                "",
                                sha1,
                                None,
                            )
                            .await?;
                        }
                        extract_native_jar(&native_path, &natives_dir)?;
                    }
                }
            }
        }
    }
    Ok(())
}

fn library_artifact(lib: &Library) -> Option<&DownloadEntry> {
    lib.downloads.as_ref()?.artifact.as_ref()
}

fn library_destination(root: &Path, lib: &Library, artifact: &DownloadEntry) -> PathBuf {
    artifact_destination(root, artifact)
}

fn artifact_destination(root: &Path, artifact: &DownloadEntry) -> PathBuf {
    if let Some(path) = &artifact.path {
        root.join("libraries").join(path)
    } else {
        root.join("libraries").join("unknown.jar")
    }
}

pub(crate) fn maven_library_path(root: &Path, name: &str) -> Option<PathBuf> {
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    let (group, artifact, version) = (parts[0], parts[1], parts[2]);
    let file_name = if parts.len() > 3 {
        format!("{artifact}-{}-{}.jar", parts[2], parts[3..].join("-"))
    } else {
        format!("{artifact}-{version}.jar")
    };
    let rel = format!(
        "{}/{}/{}/{}",
        group.replace('.', "/"),
        artifact,
        version,
        file_name
    );
    Some(root.join("libraries").join(rel))
}

fn library_file_path(root: &Path, lib: &Library) -> Option<PathBuf> {
    if let Some(artifact) = library_artifact(lib) {
        let path = library_destination(root, lib, artifact);
        if artifact.path.is_some() || path.exists() {
            return Some(path);
        }
    }
    maven_library_path(root, &lib.name)
}

fn is_native_library(name: &str) -> bool {
    name.contains(":natives-")
}

fn library_allowed(lib: &Library) -> bool {
    if is_native_library(&lib.name) {
        return lib.name.ends_with(":natives-windows");
    }

    let Some(rules) = &lib.rules else {
        return true;
    };
    let mut allow = false;
    for rule in rules {
        let os_name = rule.os.as_ref().and_then(|o| o.name.as_deref());
        let os_match = os_name == Some("windows") || os_name.is_none();
        if !os_match {
            continue;
        }
        allow = rule.action == "allow";
    }
    allow
}

async fn install_assets(
    app: &AppHandle,
    root: &Path,
    index: &AssetIndex,
    from: u8,
    to: u8,
) -> Result<(), String> {
    let assets_dir = root.join("assets");
    fs::create_dir_all(&assets_dir).map_err(|e| e.to_string())?;
    let index_path = assets_dir.join("indexes").join(format!("{}.json", index.id));
    if let Some(parent) = index_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    if !index_path.exists() {
        download_file_resumable(None, &index.url, &index_path, 0, 0, "", None, None).await?;
    }

    let raw = fs::read_to_string(&index_path).map_err(|e| e.to_string())?;
    let index_data: AssetIndexFile = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    let objects: Vec<(String, AssetObject)> = index_data.objects.into_iter().collect();
    let total = objects.len().max(1);

    for (i, (_name, meta)) in objects.iter().enumerate() {
        if meta.hash.len() < 2 {
            continue;
        }
        let path = assets_dir.join("objects").join(&meta.hash[..2]).join(&meta.hash);
        if path.exists() && verify_file_sha1(&path, &meta.hash).is_ok() {
            continue;
        }
        if path.exists() {
            let _ = fs::remove_file(&path);
        }
        if i % 25 == 0 {
            let pct = from + ((i as u8).saturating_mul(to.saturating_sub(from)) / total as u8);
            emit_progress(app, pct, format!("Ресурсы ({}/{})", i + 1, total));
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let url = format!(
            "https://resources.download.minecraft.net/{}/{}",
            &meta.hash[..2],
            meta.hash
        );
        download_file_resumable(
            None,
            &url,
            &path,
            0,
            0,
            "",
            Some(&meta.hash),
            None,
        )
        .await?;
    }

    emit_progress(app, to, "Ресурсы готовы".to_string());
    Ok(())
}

fn emit_progress(app: &AppHandle, percent: u8, message: String) {
    let _ = app.emit(
        "install-progress",
        ProgressPayload { percent, message },
    );
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressPayload {
    percent: u8,
    message: String,
}

pub(crate) fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .expect("http client")
}

fn http_client_long() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(3600))
        .build()
        .expect("http client long")
}

fn extract_zip(zip_path: &Path, dest: &Path) -> Result<(), String> {
    let file = fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let outpath = match file.enclosed_name() {
            Some(path) => dest.join(path),
            None => continue,
        };
        if file.name().ends_with('/') {
            fs::create_dir_all(&outpath).map_err(|e| e.to_string())?;
        } else {
            if let Some(p) = outpath.parent() {
                fs::create_dir_all(p).map_err(|e| e.to_string())?;
            }
            let mut outfile = fs::File::create(&outpath).map_err(|e| e.to_string())?;
            std::io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let ty = entry.file_type().map_err(|e| e.to_string())?;
        let dest_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dest_path)?;
        } else {
            fs::copy(entry.path(), dest_path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn extract_native_jar(jar_path: &Path, dest: &Path) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let file = fs::File::open(jar_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        if file.name().ends_with(".dll") {
            let outpath = dest.join(Path::new(file.name()).file_name().unwrap_or_default());
            let mut outfile = fs::File::create(&outpath).map_err(|e| e.to_string())?;
            std::io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub fn collect_classpath(root: &Path, version: &str, libraries: &[Library]) -> Result<String, String> {
    let mut paths = Vec::new();
    let mut fabric_loader = None;

    for lib in libraries {
        if !library_allowed(lib) || is_native_library(&lib.name) {
            continue;
        }
        if let Some(path) = library_file_path(root, lib) {
            if !path.exists() {
                continue;
            }
            if lib.name.starts_with("net.fabricmc:fabric-loader:") {
                fabric_loader = Some(path);
            } else {
                paths.push(path);
            }
        }
    }

    if let Some(loader) = fabric_loader {
        paths.insert(0, loader);
    }

    paths.push(
        root.join("versions")
            .join(version)
            .join(format!("{version}.jar")),
    );
    Ok(paths
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(";"))
}
