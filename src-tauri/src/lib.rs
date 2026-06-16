pub mod bootstrap;
pub mod fabric;
pub mod install;
mod launch;
mod ping;
mod profile;
mod server_wait;
mod server_wake;

use install::{
    client_pack_needs_update, collect_classpath, ensure_java, ensure_minecraft, is_version_installed,
    read_version_details, request_install_cancel, reset_install_cancel, ClientPack,
};
use launch::{is_game_running, launch_game, pick_launch_server};
use ping::PingResult;
use profile::{
    cache_bootstrap as persist_bootstrap_cache, clear_all_data as wipe_launcher_data,
    ensure_dirs, ensure_game_install_dirs, game_install_path_info,
    load_cached_bootstrap as read_bootstrap_cache, load_profile as read_profile,
    relocate_game_install_path, save_profile as write_profile, GameInstallPathInfo, Profile,
};
use serde_json::Value;
use tauri::AppHandle;
use tauri::Emitter;

const DEFAULT_RAM_GB: u32 = 4;
const MINECRAFT_GAME_ID: &str = "minecraft";

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct LaunchResult {
    launched: bool,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct BootstrapInput {
    enabled: bool,
    minecraft_version: String,
    java_major: u32,
    #[serde(default)]
    client_pack: Option<ClientPack>,
}

#[tauri::command]
fn load_profile() -> Profile {
    read_profile()
}

#[tauri::command]
fn save_profile(profile: Profile) -> Result<(), String> {
    write_profile(&profile)
}

#[tauri::command]
fn get_game_install_path(game_id: String) -> GameInstallPathInfo {
    game_install_path_info(&game_id)
}

#[tauri::command]
fn client_pack_update_needed(
    game_id: String,
    minecraft_version: String,
    client_pack: Option<ClientPack>,
) -> Result<bool, String> {
    if game_id != MINECRAFT_GAME_ID {
        return Ok(false);
    }

    let install_root = ensure_game_install_dirs(&game_id)?;
    let version_dir = install_root.join("versions").join(&minecraft_version);
    let jar_path = version_dir.join(format!("{minecraft_version}.jar"));
    let json_path = version_dir.join(format!("{minecraft_version}.json"));

    if !jar_path.exists() || !json_path.exists() {
        return Ok(false);
    }

    Ok(match client_pack.as_ref() {
        Some(pack) => client_pack_needs_update(&install_root, &minecraft_version, pack),
        None => false,
    })
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ClientInstallStatus {
    installed: bool,
    needsUpdate: bool,
}

#[tauri::command]
fn client_install_status(
    game_id: String,
    minecraft_version: String,
    client_pack: Option<ClientPack>,
) -> Result<ClientInstallStatus, String> {
    if game_id != MINECRAFT_GAME_ID {
        return Ok(ClientInstallStatus { installed: false, needsUpdate: false });
    }

    let install_root = ensure_game_install_dirs(&game_id)?;
    let version_dir = install_root.join("versions").join(&minecraft_version);
    let jar_path = version_dir.join(format!("{minecraft_version}.jar"));
    let json_path = version_dir.join(format!("{minecraft_version}.json"));

    if !jar_path.exists() || !json_path.exists() {
        return Ok(ClientInstallStatus { installed: false, needsUpdate: false });
    }

    let details = read_version_details(&json_path)?;
    let base_installed = is_version_installed(&install_root, &minecraft_version, &details);

    let needs_update = base_installed
        && client_pack
            .as_ref()
            .map(|pack| client_pack_needs_update(&install_root, &minecraft_version, pack))
            .unwrap_or(false);

    Ok(ClientInstallStatus {
        installed: base_installed,
        needsUpdate: needs_update,
    })
}

#[tauri::command]
async fn set_game_install_path_cmd(
    app: AppHandle,
    game_id: String,
    path: Option<String>,
    bootstrap: Option<Value>,
) -> Result<(), String> {
    if is_game_running() {
        return Err("Закройте игру перед сменой папки установки".to_string());
    }
    let install_root = relocate_game_install_path(&game_id, path)?;

    if game_id == MINECRAFT_GAME_ID {
        if let Some(raw_bootstrap) = bootstrap {
            if let Ok(config) = serde_json::from_value::<BootstrapInput>(raw_bootstrap) {
                if config.enabled {
                    let version = config.minecraft_version;
                    let client_pack = config.client_pack.as_ref();
                    let profile = read_profile();
                    if let Err(err) = ensure_minecraft(
                        Some(&app),
                        &install_root,
                        &version,
                        client_pack,
                        profile.dev_mode,
                        |percent, message| {
                            let _ = app.emit(
                                "install-progress",
                                serde_json::json!({ "percent": percent, "message": message }),
                            );
                        },
                    )
                    .await
                    {
                        let _ = app.emit(
                            "install-progress",
                            serde_json::json!({
                                "percent": 0,
                                "message": format!("Папка сохранена, но докачка не удалась: {err}")
                            }),
                        );
                    }
                }
            }
        }
    }

    Ok(())
}

#[tauri::command]
fn cache_bootstrap(payload: Value) -> Result<(), String> {
    persist_bootstrap_cache(&payload)
}

#[tauri::command]
fn load_cached_bootstrap() -> Option<Value> {
    read_bootstrap_cache()
}

#[tauri::command]
async fn fetch_bootstrap(api_url: String) -> Result<Value, String> {
    bootstrap::fetch_bootstrap(&api_url).await
}

#[tauri::command]
async fn ping_server(host: String, port: u16) -> Result<PingResult, String> {
    Ok(ping::ping_server(&host, port).await)
}

#[tauri::command]
async fn wake_game_servers(
    api_url: String,
    game_id: String,
    server_id: Option<String>,
) -> Result<(), String> {
    server_wake::wake_game_servers(
        &api_url,
        &game_id,
        server_id.as_deref(),
    )
    .await
}

#[tauri::command]
fn game_is_running() -> bool {
    is_game_running()
}

#[tauri::command]
async fn prepare_and_launch(
    app: AppHandle,
    nickname: String,
    game_id: String,
    server_id: Option<String>,
    api_url: String,
    bootstrap: Value,
) -> Result<LaunchResult, String> {
    reset_install_cancel();
    let config: BootstrapInput = serde_json::from_value(bootstrap.clone()).map_err(|e| e.to_string())?;
    if !config.enabled {
        return Err("Лаунчер отключён администратором".to_string());
    }

    if game_id != MINECRAFT_GAME_ID {
        return Err("Эта игра пока не поддерживается".to_string());
    }

    ensure_dirs()?;
    let profile = read_profile();
    let install_root = ensure_game_install_dirs(&game_id)?;

    let emit = |percent: u8, message: &str| {
        let _ = app.emit(
            "install-progress",
            serde_json::json!({ "percent": percent, "message": message }),
        );
    };

    let server = pick_launch_server(&bootstrap, &game_id, server_id.as_deref());

    emit(1, "Запуск сервера…");
    if let Err(err) = server_wake::wake_game_servers(
        &api_url,
        &game_id,
        server_id.as_deref(),
    )
    .await
    {
        emit(1, &format!("Сервер: {err}"));
    }

    emit(5, "Подготовка Java…");
    let java_exe = ensure_java(&app, config.java_major, |p, m| emit(p, m)).await?;

    emit(42, "Подготовка Minecraft…");
    let version = config.minecraft_version.clone();
    let client_pack = config.client_pack.as_ref();
    let (_jar, mut details) = ensure_minecraft(
        Some(&app),
        &install_root,
        &version,
        client_pack,
        profile.dev_mode,
        |p, m| emit(p, m),
    )
    .await?;

    if !is_version_installed(&install_root, &version, &details) {
        emit(90, "Докачка библиотек…");
        install::install_libraries(Some(&app), &install_root, &details.libraries, 90, 98).await?;
        let json_path = install_root
            .join("versions")
            .join(&version)
            .join(format!("{version}.json"));
        details = read_version_details(&json_path)?;
        if !is_version_installed(&install_root, &version, &details) {
            return Err("Клиент установлен не полностью — не хватает библиотек".to_string());
        }
    }

    let classpath = collect_classpath(&install_root, &version, &details.libraries)?;
    if classpath.is_empty() {
        return Err("Не удалось собрать classpath для запуска".to_string());
    }

    if let Some((host, port)) = server.as_ref() {
        server_wait::wait_for_server_online(host, *port, |message| {
            emit(95, message);
        })
        .await?;
    }

    emit(100, "Запуск…");

    launch_game(
        &app,
        &java_exe,
        &classpath,
        &details,
        &nickname,
        &version,
        DEFAULT_RAM_GB,
        &install_root,
        server.as_ref(),
        profile.display_mode,
    )?;

    Ok(LaunchResult { launched: true })
}

#[tauri::command]
fn cancel_install() -> Result<(), String> {
    request_install_cancel();
    Ok(())
}

#[tauri::command]
fn clear_all_data() -> Result<(), String> {
    if is_game_running() {
        return Err("Закройте игру перед очисткой данных".to_string());
    }
    wipe_launcher_data()
}

#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("Разрешены только http(s) ссылки".to_string());
    }
    open::that(&url).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            load_profile,
            save_profile,
            get_game_install_path,
            client_pack_update_needed,
            client_install_status,
            set_game_install_path_cmd,
            cache_bootstrap,
            load_cached_bootstrap,
            fetch_bootstrap,
            ping_server,
            wake_game_servers,
            game_is_running,
            prepare_and_launch,
            cancel_install,
            clear_all_data,
            open_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
