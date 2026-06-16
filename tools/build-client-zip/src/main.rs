use chadow_games_launcher_lib::fabric;
use chadow_games_launcher_lib::install::{ensure_minecraft, is_version_installed, read_version_details};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{BufWriter, Read};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

const VERSION: &str = "1.21.11";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .map(Path::to_path_buf)
        .expect("repo root")
}

fn find_mod_jar() -> Result<PathBuf, String> {
    let libs = repo_root().join("client-mod").join("build").join("libs");
    let mut jars: Vec<PathBuf> = fs::read_dir(&libs)
        .map_err(|e| format!("Не найдена папка мода: {e}"))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "jar")
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        name.starts_with("chadow-games-client-") && !name.contains("sources")
                    })
        })
        .collect();

    jars.sort();
    jars.pop().ok_or_else(|| {
        format!(
            "Сначала соберите мод: client-mod\\gradlew.bat build (ожидался jar в {})",
            libs.display()
        )
    })
}

fn copy_mod(root: &Path, mod_jar: &Path) -> Result<(), String> {
    let mods_dir = root.join("mods");
    fs::create_dir_all(&mods_dir).map_err(|e| e.to_string())?;
    let dest = mods_dir.join(
        mod_jar
            .file_name()
            .ok_or_else(|| "Некорректное имя jar мода".to_string())?,
    );
    fs::copy(mod_jar, &dest).map_err(|e| e.to_string())?;
    println!("    mod: {}", dest.file_name().unwrap().to_string_lossy());
    Ok(())
}

fn add_path_to_zip(
    zip: &mut ZipWriter<BufWriter<File>>,
    base: &Path,
    path: &Path,
    options: SimpleFileOptions,
) -> Result<(), String> {
    if path.is_dir() {
        for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            add_path_to_zip(zip, base, &entry.path(), options)?;
        }
        return Ok(());
    }

    let relative = path
        .strip_prefix(base)
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .replace('\\', "/");

    zip.start_file(relative, options)
        .map_err(|e| e.to_string())?;
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    std::io::copy(&mut file, zip).map_err(|e| e.to_string())?;
    Ok(())
}

fn create_client_zip(staging: &Path, zip_path: &Path) -> Result<(), String> {
    if zip_path.exists() {
        fs::remove_file(zip_path).map_err(|e| e.to_string())?;
    }
    if let Some(parent) = zip_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let file = File::create(zip_path).map_err(|e| e.to_string())?;
    let mut zip = ZipWriter::new(BufWriter::new(file));
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    for name in ["versions", "libraries", "assets", "mods"] {
        let dir = staging.join(name);
        if !dir.exists() {
            return Err(format!("Нет папки для архива: {}", dir.display()));
        }
        add_path_to_zip(&mut zip, staging, &dir, options)?;
    }

    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let out_dir = repo_root().join("dist");
    let zip_path = out_dir.join(format!("minecraft-{VERSION}-client.zip"));
    let staging = std::env::temp_dir().join("chadow-full-client-staging");

    println!("==> Mod JAR...");
    let mod_jar = find_mod_jar()?;
    println!("    {}", mod_jar.display());

    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&staging).map_err(|e| e.to_string())?;

    println!("==> Copy mod into staging...");
    copy_mod(&staging, &mod_jar)?;

    println!("==> Download Minecraft {VERSION} from Mojang (jar, libraries, assets)...");
    println!("    This may take several minutes.");
    let (_jar, details) = ensure_minecraft(
        None,
        &staging,
        VERSION,
        None,
        true,
        |percent, message| println!("    [{percent:>3}%] {message}"),
    )
    .await?;

    fabric::normalize_client_pack_layout(&staging)?;

    if !is_version_installed(&staging, VERSION, &details) {
        return Err("Клиент скачан не полностью — не хватает библиотек или ресурсов".to_string());
    }

    let json_path = staging
        .join("versions")
        .join(VERSION)
        .join(format!("{VERSION}.json"));
    let details = read_version_details(&json_path)?;
    if details.main_class.is_empty() {
        return Err("version.json не содержит mainClass".to_string());
    }

    println!("==> Creating ZIP...");
    create_client_zip(&staging, &zip_path)?;

    let size = fs::metadata(&zip_path)
        .map_err(|e| e.to_string())?
        .len();
    let hash = sha256_file(&zip_path)?;
    let size_mb = (size as f64) / (1024.0 * 1024.0);

    println!();
    println!("Done!");
    println!("  File:   {}", zip_path.display());
    println!("  Size:   {size_mb:.1} MB ({size} bytes)");
    println!("  SHA256: {hash}");
    println!();
    println!("Upload at https://chadow.ru/admin/minecraft");
    println!("  Version: {VERSION}");
    println!("  ZIP:     minecraft-{VERSION}-client.zip");

    let _ = fs::remove_dir_all(&staging);
    Ok(())
}
