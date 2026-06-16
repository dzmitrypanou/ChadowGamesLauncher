use crate::install::VersionDetails;
use crate::profile::{launcher_root, DisplayMode};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

const LAUNCHER_NAME: &str = "chadow-games-launcher";
const LAUNCHER_VERSION: &str = "3.2.2";
const PENDING_CONNECT_FILE: &str = "chadow-connect.txt";

static GAME_RUNNING: AtomicBool = AtomicBool::new(false);

pub fn is_game_running() -> bool {
    GAME_RUNNING.load(Ordering::SeqCst)
}

pub fn pick_launch_server(
    bootstrap: &Value,
    game_id: &str,
    server_id: Option<&str>,
) -> Option<(String, u16)> {
    #[derive(Deserialize)]
    struct ServerEntry {
        #[serde(default)]
        id: Option<String>,
        host: String,
        #[serde(default)]
        port: Option<u16>,
        #[serde(default, rename = "connectHost")]
        connect_host: Option<String>,
        #[serde(default, rename = "connectPort")]
        connect_port: Option<u16>,
    }

    fn resolve_connect(server: &ServerEntry) -> Option<(String, u16)> {
        let display_host = server.host.trim();
        let connect_host = server
            .connect_host
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(display_host);
        if connect_host.is_empty() {
            return None;
        }
        let port = server
            .connect_port
            .or(server.port)
            .unwrap_or(25565);
        Some((connect_host.to_string(), port))
    }

    #[derive(Deserialize)]
    struct GameEntry {
        #[serde(default)]
        id: Option<String>,
        #[serde(default)]
        servers: Vec<ServerEntry>,
    }

    let mut servers: Vec<ServerEntry> = Vec::new();

    if let Some(games) = bootstrap.get("games").and_then(|value| value.as_array()) {
        for game in games {
            if let Ok(parsed) = serde_json::from_value::<GameEntry>(game.clone()) {
                if parsed.id.as_deref().unwrap_or("minecraft") != game_id {
                    continue;
                }
                servers = parsed.servers;
                break;
            }
        }
    }

    if servers.is_empty() {
        if let Some(list) = bootstrap.get("servers").and_then(|value| value.as_array()) {
            for server in list {
                if let Ok(parsed) = serde_json::from_value::<ServerEntry>(server.clone()) {
                    servers.push(parsed);
                }
            }
        }
    }

    if let Some(id) = server_id.map(str::trim).filter(|value| !value.is_empty()) {
        for server in &servers {
            if server.id.as_deref() == Some(id) {
                if let Some(resolved) = resolve_connect(server) {
                    return Some(resolved);
                }
            }
        }
    }

    for server in servers {
        if let Some(resolved) = resolve_connect(&server) {
            return Some(resolved);
        }
    }

    None
}

pub fn format_quick_play_server(host: &str, port: u16) -> String {
    format!("{host}:{port}")
}

#[derive(Debug, Deserialize)]
struct ArgRule {
    action: String,
    os: Option<ArgRuleOs>,
    features: Option<HashMap<String, Value>>,
}

#[derive(Debug, Deserialize)]
struct ArgRuleOs {
    name: Option<String>,
    arch: Option<String>,
}

pub fn offline_uuid(username: &str) -> String {
    Uuid::new_v3(
        &Uuid::NAMESPACE_DNS,
        format!("OfflinePlayer:{username}").as_bytes(),
    )
    .to_string()
}

fn log_line(message: &str) {
    let _ = (|| -> Result<(), Box<dyn std::error::Error>> {
        let path = launcher_root().join("launcher.log");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        writeln!(file, "{message}")?;
        Ok(())
    })();
}

fn java_gui_exe(java_exe: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let name = java_exe
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if name.eq_ignore_ascii_case("java.exe") {
            return java_exe.with_file_name("javaw.exe");
        }
    }
    java_exe.to_path_buf()
}

fn read_log_tail(path: &Path, max_bytes: usize) -> String {
    let Ok(mut file) = std::fs::File::open(path) else {
        return String::new();
    };
    let len = file.metadata().map(|meta| meta.len() as usize).unwrap_or(0);
    if len > max_bytes {
        let _ = file.seek(std::io::SeekFrom::Start((len - max_bytes) as u64));
    }
    let mut buffer = String::new();
    let _ = file.read_to_string(&mut buffer);
    buffer.trim().chars().take(1200).collect()
}

fn rule_matches(rules: &[ArgRule], features: &HashMap<&str, bool>) -> bool {
    if rules.is_empty() {
        return true;
    }

    let mut allowed = false;
    for rule in rules {
        let os_ok = rule.os.as_ref().is_none_or(|os| {
            let name_ok = os.name.as_deref().is_none_or(|name| name == "windows");
            let arch_ok = os.arch.as_deref().is_none_or(|arch| arch == "x86_64" || arch == "x86");
            name_ok && arch_ok
        });
        let features_ok = rule.features.as_ref().is_none_or(|rule_features| {
            rule_features.iter().all(|(key, expected)| {
                let expected_bool = expected.as_bool().unwrap_or(false);
                features.get(key.as_str()).copied().unwrap_or(false) == expected_bool
            })
        });
        if os_ok && features_ok {
            allowed = rule.action == "allow";
        }
    }
    allowed
}

fn substitute(template: &str, vars: &HashMap<&str, String>) -> String {
    let mut result = template.to_string();
    for (key, value) in vars {
        result = result.replace(&format!("${{{key}}}"), value);
    }
    result
}

fn is_unresolved(value: &str) -> bool {
    value.contains("${")
}

fn push_resolved(out: &mut Vec<String>, raw: &str, vars: &HashMap<&str, String>) {
    let value = substitute(raw, vars);
    if !value.is_empty() && !is_unresolved(&value) {
        out.push(value);
    }
}

fn collect_arg_values(
    entry: &Value,
    vars: &HashMap<&str, String>,
    features: &HashMap<&str, bool>,
    out: &mut Vec<String>,
) {
    match entry {
        Value::String(text) => push_resolved(out, text, vars),
        Value::Object(obj) => {
            let rules: Vec<ArgRule> = obj
                .get("rules")
                .and_then(|value| serde_json::from_value(value.clone()).ok())
                .unwrap_or_default();
            if !rule_matches(&rules, features) {
                return;
            }

            match obj.get("value") {
                Some(Value::String(text)) => push_resolved(out, text, vars),
                Some(Value::Array(items)) => {
                    for item in items {
                        if let Some(text) = item.as_str() {
                            push_resolved(out, text, vars);
                        }
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }
}

fn resolve_args(
    entries: Option<&Vec<Value>>,
    vars: &HashMap<&str, String>,
    features: &HashMap<&str, bool>,
) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(items) = entries {
        for entry in items {
            collect_arg_values(entry, vars, features, &mut out);
        }
    }
    out
}

fn primary_screen_size(app: &AppHandle) -> (u32, u32) {
    app.primary_monitor()
        .ok()
        .flatten()
        .map(|monitor| {
            let area = monitor.work_area();
            (area.size.width, area.size.height)
        })
        .unwrap_or((1920, 1080))
}

fn set_game_option(lines: &mut Vec<String>, key: &str, value: &str) {
    let prefix = format!("{key}:");
    if let Some(line) = lines.iter_mut().find(|line| line.starts_with(&prefix)) {
        *line = format!("{key}:{value}");
    } else {
        lines.push(format!("{key}:{value}"));
    }
}

fn apply_fullscreen_option(game_dir: &Path, fullscreen: bool) -> Result<(), String> {
    let path = game_dir.join("options.txt");
    let mut lines: Vec<String> = if path.exists() {
        std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())?
            .lines()
            .map(ToString::to_string)
            .collect()
    } else {
        Vec::new()
    };

    set_game_option(
        &mut lines,
        "fullscreen",
        if fullscreen { "true" } else { "false" },
    );

    let contents = if lines.is_empty() {
        format!("fullscreen:{}", fullscreen)
    } else {
        lines.join("\n")
    };
    std::fs::write(path, contents).map_err(|e| e.to_string())
}

fn launch_features(custom_resolution: bool) -> HashMap<&'static str, bool> {
    HashMap::from([
        ("is_demo_user", false),
        ("has_custom_resolution", custom_resolution),
        ("has_quick_plays_support", false),
        ("is_quick_play_singleplayer", false),
        ("is_quick_play_multiplayer", false),
        ("is_quick_play_realms", false),
    ])
}

fn write_pending_connect(game_dir: &Path, server: Option<&(String, u16)>) -> Result<(), String> {
    let path = game_dir.join(PENDING_CONNECT_FILE);
    match server {
        Some((host, port)) => std::fs::write(&path, format_quick_play_server(host, *port))
            .map_err(|e| e.to_string()),
        None => {
            if path.exists() {
                std::fs::remove_file(path).map_err(|e| e.to_string())?;
            }
            Ok(())
        }
    }
}

fn filter_optional_game_args(args: Vec<String>) -> Vec<String> {
    let mut filtered = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg.starts_with("--") && flag_requires_value(arg) {
            if index + 1 < args.len() && !args[index + 1].starts_with("--") {
                let value = &args[index + 1];
                if !value.is_empty() && !is_unresolved(value) {
                    filtered.push(arg.clone());
                    filtered.push(value.clone());
                }
                index += 2;
                continue;
            }
            index += 1;
            continue;
        }
        filtered.push(arg.clone());
        index += 1;
    }
    filtered
}

fn flag_requires_value(flag: &str) -> bool {
    !matches!(flag, "--demo")
}

pub fn launch_game(
    app: &AppHandle,
    java_exe: &PathBuf,
    classpath: &str,
    details: &VersionDetails,
    username: &str,
    version: &str,
    ram_gb: u32,
    install_root: &Path,
    server: Option<&(String, u16)>,
    display_mode: DisplayMode,
) -> Result<(), String> {
    if is_game_running() {
        return Err("Игра уже запущена".to_string());
    }

    let root = install_root.to_path_buf();
    let assets_dir = root.join("assets");
    let game_dir = root.join("instances").join("default");
    let natives = root.join("natives");
    std::fs::create_dir_all(&game_dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&assets_dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&natives).map_err(|e| e.to_string())?;

    let fullscreen = display_mode == DisplayMode::Fullscreen;
    let window_size = if fullscreen {
        None
    } else {
        Some(primary_screen_size(app))
    };
    apply_fullscreen_option(&game_dir, fullscreen)?;
    write_pending_connect(&game_dir, server)?;

    let uuid = offline_uuid(username);
    let access_token = uuid.clone();
    let natives_str = natives.to_string_lossy().to_string();
    let assets_str = assets_dir.to_string_lossy().to_string();
    let game_str = game_dir.to_string_lossy().to_string();

    let mut vars: HashMap<&str, String> = HashMap::new();
    vars.insert("natives_directory", natives_str.clone());
    vars.insert("classpath", classpath.to_string());
    vars.insert("launcher_name", LAUNCHER_NAME.to_string());
    vars.insert("launcher_version", LAUNCHER_VERSION.to_string());
    vars.insert("auth_player_name", username.to_string());
    vars.insert("version_name", version.to_string());
    vars.insert("game_directory", game_str);
    vars.insert("assets_root", assets_str);
    vars.insert("assets_index_name", details.asset_index.id.clone());
    vars.insert("auth_uuid", uuid.clone());
    vars.insert("auth_access_token", access_token);
    vars.insert("auth_xuid", String::new());
    vars.insert("clientid", String::new());
    vars.insert("version_type", "release".to_string());
    let (resolution_width, resolution_height) = window_size
        .map(|(width, height)| (width.to_string(), height.to_string()))
        .unwrap_or_default();
    vars.insert("resolution_width", resolution_width);
    vars.insert("resolution_height", resolution_height);
    vars.insert("quickPlayPath", String::new());
    vars.insert("quickPlaySingleplayer", String::new());
    vars.insert("quickPlayMultiplayer", String::new());
    vars.insert("quickPlayRealms", String::new());

    let features = launch_features(window_size.is_some());

    let java_path = java_gui_exe(java_exe);
    let mut cmd = Command::new(&java_path);
    if let Some(java_home) = java_exe.parent().and_then(|bin| bin.parent()) {
        cmd.env("JAVA_HOME", java_home);
    }
    cmd.arg(format!("-Xmx{}G", ram_gb.max(2)));

    if let Some(arguments) = &details.arguments {
        let mut jvm_args = resolve_args(arguments.jvm.as_ref(), &vars, &features);
        if jvm_args.is_empty() {
            jvm_args = legacy_jvm_args(&natives_str, classpath);
        } else {
            jvm_args = merge_jvm_args(&natives_str, classpath, jvm_args);
        }
        for arg in &jvm_args {
            cmd.arg(arg);
        }

        let game_args =
            filter_optional_game_args(resolve_args(arguments.game.as_ref(), &vars, &features));
        cmd.arg(&details.main_class);
        for arg in &game_args {
            cmd.arg(arg);
        }

        log_line(&format!(
            "Game args: {}",
            game_args.join(" ")
        ));
    } else {
        for arg in legacy_jvm_args(&natives_str, classpath) {
            cmd.arg(arg);
        }
        cmd.arg(&details.main_class);
        for arg in legacy_game_args(
            username,
            version,
            &game_dir,
            &assets_dir,
            &details.asset_index.id,
            &vars["auth_uuid"],
            window_size,
        ) {
            cmd.arg(arg);
        }
    }

    let game_log_path = root.join("minecraft-game.log");
    let game_log_file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&game_log_path)
        .map_err(|e| format!("Не удалось создать лог: {e}"))?;
    let game_log_stdout = game_log_file
        .try_clone()
        .map_err(|e| format!("Не удалось создать лог: {e}"))?;

    log_line(&format!(
        "Launching Minecraft {version} for {username} via {}{}",
        java_path.display(),
        server
            .map(|(host, port)| format!(" -> {host}:{port}"))
            .unwrap_or_default()
    ));
    log_line(&format!("Main class: {}", details.main_class));

    cmd.stdin(Stdio::null())
        .stdout(Stdio::from(game_log_file))
        .stderr(Stdio::from(game_log_stdout));

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Не удалось запустить Java: {e}"))?;

    for _ in 0..30 {
        match child.try_wait() {
            Ok(Some(status)) => {
                let code = status.code().unwrap_or(-1);
                let tail = read_log_tail(&game_log_path, 16_384);
                let detail = if tail.is_empty() {
                    format!("код выхода {code}")
                } else {
                    format!("код выхода {code}: {tail}")
                };
                return Err(format!(
                    "Игра завершилась сразу после запуска ({detail})"
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(format!("Не удалось проверить процесс Java: {e}")),
        }
    }

    GAME_RUNNING.store(true, Ordering::SeqCst);
    let app_handle = app.clone();
    std::thread::spawn(move || {
        let _ = child.wait();
        GAME_RUNNING.store(false, Ordering::SeqCst);
        let _ = app_handle.emit("game-exited", ());
    });

    Ok(())
}

fn legacy_jvm_args(natives: &str, classpath: &str) -> Vec<String> {
    vec![
        format!("-Djava.library.path={natives}"),
        format!("-Djna.tmpdir={natives}"),
        format!("-Dorg.lwjgl.system.SharedLibraryExtractPath={natives}"),
        format!("-Dio.netty.native.workdir={natives}"),
        format!("-Dminecraft.launcher.brand={LAUNCHER_NAME}"),
        format!("-Dminecraft.launcher.version={LAUNCHER_VERSION}"),
        "-cp".to_string(),
        classpath.to_string(),
    ]
}

fn merge_jvm_args(natives: &str, classpath: &str, extra: Vec<String>) -> Vec<String> {
    let has_cp = extra
        .iter()
        .any(|arg| arg == "-cp" || arg == "--classpath");
    if has_cp {
        let mut merged = extra;
        prepend_native_jvm_props(&mut merged, natives);
        return merged;
    }

    let mut merged = legacy_jvm_args(natives, classpath);
    merged.extend(extra);
    merged
}

fn prepend_native_jvm_props(args: &mut Vec<String>, natives: &str) {
    let props = [
        format!("-Djava.library.path={natives}"),
        format!("-Djna.tmpdir={natives}"),
        format!("-Dorg.lwjgl.system.SharedLibraryExtractPath={natives}"),
        format!("-Dio.netty.native.workdir={natives}"),
    ];
    for prop in props.into_iter().rev() {
        if !args.iter().any(|arg| arg == &prop) {
            args.insert(0, prop);
        }
    }
}

fn legacy_game_args(
    username: &str,
    version: &str,
    game_dir: &Path,
    assets_dir: &Path,
    asset_index: &str,
    uuid: &str,
    window_size: Option<(u32, u32)>,
) -> Vec<String> {
    let mut args = vec![
        "--username".into(),
        username.into(),
        "--version".into(),
        version.into(),
        "--gameDir".into(),
        game_dir.to_string_lossy().to_string(),
        "--assetsDir".into(),
        assets_dir.to_string_lossy().to_string(),
        "--assetIndex".into(),
        asset_index.into(),
        "--uuid".into(),
        uuid.into(),
        "--accessToken".into(),
        uuid.into(),
        "--userType".into(),
        "legacy".into(),
        "--versionType".into(),
        "release".into(),
    ];

    if let Some((width, height)) = window_size {
        args.push("--width".into());
        args.push(width.to_string());
        args.push("--height".into());
        args.push(height.to_string());
    }

    args
}
