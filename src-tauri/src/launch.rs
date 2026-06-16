use crate::install::VersionDetails;
use crate::profile::game_root;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

const LAUNCHER_NAME: &str = "chadow-games-launcher";
const LAUNCHER_VERSION: &str = "3.2.2";

static GAME_RUNNING: AtomicBool = AtomicBool::new(false);

pub fn is_game_running() -> bool {
    GAME_RUNNING.load(Ordering::SeqCst)
}

pub fn pick_launch_server(bootstrap: &Value) -> Option<(String, u16)> {
    #[derive(Deserialize)]
    struct ServerEntry {
        host: String,
        #[serde(default)]
        port: Option<u16>,
    }

    #[derive(Deserialize)]
    struct GameEntry {
        #[serde(default)]
        servers: Vec<ServerEntry>,
    }

    if let Some(games) = bootstrap.get("games").and_then(|value| value.as_array()) {
        for game in games {
            if let Ok(parsed) = serde_json::from_value::<GameEntry>(game.clone()) {
                if let Some(server) = parsed.servers.into_iter().find(|s| !s.host.trim().is_empty())
                {
                    return Some((server.host, server.port.unwrap_or(25565)));
                }
            }
        }
    }

    if let Some(servers) = bootstrap.get("servers").and_then(|value| value.as_array()) {
        for server in servers {
            if let Ok(parsed) = serde_json::from_value::<ServerEntry>(server.clone()) {
                if !parsed.host.trim().is_empty() {
                    return Some((parsed.host, parsed.port.unwrap_or(25565)));
                }
            }
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
        let path = game_root().join("launcher.log");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        writeln!(file, "{message}")?;
        Ok(())
    })();
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

fn launch_features(server: Option<&(String, u16)>) -> HashMap<&'static str, bool> {
    let quick_multiplayer = server.is_some();
    HashMap::from([
        ("is_demo_user", false),
        ("has_custom_resolution", false),
        ("has_quick_plays_support", false),
        ("is_quick_play_singleplayer", false),
        ("is_quick_play_multiplayer", quick_multiplayer),
        ("is_quick_play_realms", false),
    ])
}

fn ensure_quick_play_args(mut args: Vec<String>, server: Option<&(String, u16)>) -> Vec<String> {
    let Some((host, port)) = server else {
        return args;
    };

    let target = format_quick_play_server(host, *port);
    let has_flag = args
        .windows(2)
        .any(|pair| pair[0] == "--quickPlayMultiplayer" && pair[1] == target);

    if !has_flag {
        args.push("--quickPlayMultiplayer".into());
        args.push(target);
    }

    args
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
    server: Option<&(String, u16)>,
) -> Result<(), String> {
    if is_game_running() {
        return Err("Игра уже запущена".to_string());
    }

    let root = game_root();
    let assets_dir = root.join("assets");
    let game_dir = root.join("instances").join("default");
    let natives = root.join("natives");
    std::fs::create_dir_all(&game_dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&assets_dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&natives).map_err(|e| e.to_string())?;

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
    vars.insert("resolution_width", String::new());
    vars.insert("resolution_height", String::new());
    vars.insert("quickPlayPath", String::new());
    vars.insert("quickPlaySingleplayer", String::new());
    vars.insert(
        "quickPlayMultiplayer",
        server
            .map(|(host, port)| format_quick_play_server(host, *port))
            .unwrap_or_default(),
    );
    vars.insert("quickPlayRealms", String::new());

    let features = launch_features(server);

    let mut cmd = Command::new(java_exe);
    if let Some(java_home) = java_exe.parent().and_then(|bin| bin.parent()) {
        cmd.env("JAVA_HOME", java_home);
    }
    cmd.arg(format!("-Xmx{}G", ram_gb.max(2)));

    if let Some(arguments) = &details.arguments {
        let mut jvm_args = resolve_args(arguments.jvm.as_ref(), &vars, &features);
        if jvm_args.is_empty() {
            jvm_args = legacy_jvm_args(&natives_str, classpath);
        }
        for arg in &jvm_args {
            cmd.arg(arg);
        }

        let game_args = ensure_quick_play_args(
            filter_optional_game_args(resolve_args(arguments.game.as_ref(), &vars, &features)),
            server,
        );
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
            server,
        ) {
            cmd.arg(arg);
        }
    }

    let stderr_path = root.join("minecraft-stderr.log");
    let stderr_file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&stderr_path)
        .map_err(|e| format!("Не удалось создать лог: {e}"))?;

    log_line(&format!(
        "Launching Minecraft {version} for {username} via {}{}",
        java_exe.display(),
        server
            .map(|(host, port)| format!(" -> {host}:{port}"))
            .unwrap_or_default()
    ));

    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr_file));

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Не удалось запустить Java: {e}"))?;

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

fn legacy_game_args(
    username: &str,
    version: &str,
    game_dir: &Path,
    assets_dir: &Path,
    asset_index: &str,
    uuid: &str,
    server: Option<&(String, u16)>,
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

    if let Some((host, port)) = server {
        args.push("--quickPlayMultiplayer".into());
        args.push(format_quick_play_server(host, *port));
    }

    args
}
