mod bootstrap;
mod fabric;
mod install;
mod launch;
mod ping;
mod profile;

use install::{collect_classpath, ensure_java, ensure_minecraft, ClientPack};
use launch::{is_game_running, launch_game, pick_launch_server};
use ping::PingResult;
use profile::{
    cache_bootstrap as persist_bootstrap_cache, ensure_dirs, game_root,
    load_cached_bootstrap as read_bootstrap_cache, load_profile as read_profile,
    save_profile as write_profile, Profile,
};
use serde_json::Value;
use tauri::AppHandle;
use tauri::Emitter;

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
fn game_is_running() -> bool {
    is_game_running()
}

#[tauri::command]
async fn prepare_and_launch(
    app: AppHandle,
    nickname: String,
    ram_gb: u32,
    bootstrap: Value,
) -> Result<LaunchResult, String> {
    let config: BootstrapInput = serde_json::from_value(bootstrap.clone()).map_err(|e| e.to_string())?;
    if !config.enabled {
        return Err("Лаунчер отключён администратором".to_string());
    }

    ensure_dirs()?;

    let emit = |percent: u8, message: &str| {
        let _ = app.emit(
            "install-progress",
            serde_json::json!({ "percent": percent, "message": message }),
        );
    };

    emit(1, "Подготовка Java…");
    let java_exe = ensure_java(&app, config.java_major, |p, m| emit(p, m)).await?;

    emit(42, "Подготовка Minecraft…");
    let version = config.minecraft_version.clone();
    let client_pack = config.client_pack.as_ref();
    let (_jar, details) =
        ensure_minecraft(&app, &version, client_pack, |p, m| emit(p, m)).await?;

    let classpath = collect_classpath(&game_root(), &version, &details.libraries)?;
    emit(100, "Запуск…");

    let server = pick_launch_server(&bootstrap);

    launch_game(
        &app,
        &java_exe,
        &classpath,
        &details,
        &nickname,
        &version,
        ram_gb,
        server.as_ref(),
    )?;

    Ok(LaunchResult { launched: true })
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
        .invoke_handler(tauri::generate_handler![
            load_profile,
            save_profile,
            cache_bootstrap,
            load_cached_bootstrap,
            fetch_bootstrap,
            ping_server,
            game_is_running,
            prepare_and_launch,
            open_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
