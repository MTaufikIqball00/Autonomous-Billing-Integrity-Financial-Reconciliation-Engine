use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;
use tokio::signal;

const WINDOWS_PG_ARCHIVE_URL: &str = "https://github.com/theseus-rs/postgresql-binaries/releases/download/19.0.0/postgresql-19.0.0-x86_64-pc-windows-msvc.tar.gz";
const WINDOWS_PG_ARCHIVE_SHA256: &str = "2cf7918634790e9443ef4edb5ea47006e2fcbaa659e88e74c0dae63cdcf9d19b";

fn exe_name(base: &str) -> String {
    if cfg!(windows) {
        format!("{}.exe", base)
    } else {
        base.to_string()
    }
}

fn validate_db_identifier(db_name: &str) -> Result<()> {
    let valid = !db_name.is_empty()
        && db_name.len() <= 63
        && db_name
            .chars()
            .next()
            .map(|c| c.is_ascii_alphabetic() || c == '_')
            .unwrap_or(false)
        && db_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !valid {
        bail!(
            "Invalid POSTGRES_DB identifier '{}': must be 1..=63 ASCII letters/digits/underscore and start with a letter or '_'.",
            db_name
        );
    }
    Ok(())
}

fn find_bin_dir(install_dir: &Path) -> Option<PathBuf> {
    let direct = install_dir.join("bin");
    if direct.join(exe_name("initdb")).exists() {
        return Some(direct);
    }
    if let Ok(entries) = std::fs::read_dir(install_dir) {
        for entry in entries.flatten() {
            let candidate = entry.path().join("bin");
            if candidate.join(exe_name("initdb")).exists() {
                return Some(candidate);
            }
        }
    }
    None
}

async fn ensure_postgres_binaries(install_dir: &Path) -> Result<PathBuf> {
    if let Some(bin_dir) = find_bin_dir(install_dir) {
        return Ok(bin_dir);
    }

    if !cfg!(windows) {
        bail!(
            "No local PostgreSQL binaries found in {:?}. Install PostgreSQL or configure DATABASE_URL.",
            install_dir
        );
    }

    std::fs::create_dir_all(install_dir)?;
    let archive_path = install_dir.join("postgresql-portable.tar.gz");
    println!("[pg-manager] Downloading isolated portable PostgreSQL archive...");

    let client = reqwest::Client::builder()
        .user_agent("abifr-pg-manager/0.1")
        .build()?;
    let bytes = client
        .get(WINDOWS_PG_ARCHIVE_URL)
        .send()
        .await
        .context("Failed downloading PostgreSQL archive")?
        .error_for_status()?
        .bytes()
        .await?;

    let expected_sha256 = std::env::var("PG_ARCHIVE_SHA256")
        .unwrap_or_else(|_| WINDOWS_PG_ARCHIVE_SHA256.to_string())
        .trim()
        .to_ascii_lowercase();
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let actual_sha256 = hex::encode(hasher.finalize());
    if actual_sha256 != expected_sha256 {
        bail!(
            "PostgreSQL archive SHA-256 verification failed! Expected {}, got {}",
            expected_sha256,
            actual_sha256
        );
    }
    println!(
        "[pg-manager] SHA-256 verified ({})...",
        &actual_sha256[..16]
    );

    std::fs::write(&archive_path, &bytes)?;

    println!(
        "[pg-manager] Extracting PostgreSQL binaries ({} MB)...",
        bytes.len() / (1024 * 1024)
    );
    let status = Command::new("tar")
        .args(["-xzf"])
        .arg(&archive_path)
        .arg("-C")
        .arg(install_dir)
        .status()
        .context("Failed executing tar to extract PostgreSQL archive")?;

    let _ = std::fs::remove_file(&archive_path);
    if !status.success() {
        bail!("tar extraction of PostgreSQL archive failed");
    }

    find_bin_dir(install_dir)
        .context("Extracted PostgreSQL archive did not contain bin/initdb.exe")
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();

    let host = std::env::var("POSTGRES_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let port: u16 = std::env::var("POSTGRES_PORT")
        .unwrap_or_else(|_| "14543".into())
        .parse()
        .context("POSTGRES_PORT must be valid u16")?;
    let username = std::env::var("POSTGRES_USER").unwrap_or_else(|_| "abifr_app".into());
    let password = std::env::var("POSTGRES_PASSWORD")
        .unwrap_or_else(|_| "abifr_local_dev_pw_2026".into());
    let db_name = std::env::var("POSTGRES_DB").unwrap_or_else(|_| "abifr".into());
    validate_db_identifier(&db_name)?;

    let runtime_root = std::env::current_dir()?
        .join(std::env::var("PG_RUNTIME_DIR").unwrap_or_else(|_| ".runtime/postgres".into()));
    let data_dir = runtime_root.join("data");
    let install_dir = runtime_root.join("install");
    let pg_log = runtime_root.join("server.log");
    std::fs::create_dir_all(&runtime_root)?;

    println!(
        "[pg-manager] Preparing isolated PostgreSQL on {}:{} ...",
        host, port
    );
    let bin_dir = ensure_postgres_binaries(&install_dir).await?;
    let initdb = bin_dir.join(exe_name("initdb"));
    let pg_ctl = bin_dir.join(exe_name("pg_ctl"));
    let createdb = bin_dir.join(exe_name("createdb"));
    let psql = bin_dir.join(exe_name("psql"));

    if !data_dir.join("PG_VERSION").exists() {
        std::fs::create_dir_all(&data_dir)?;
        let pw_file = runtime_root.join(".pwtmp");
        std::fs::write(&pw_file, format!("{}\n", password))?;
        println!(
            "[pg-manager] Initializing PostgreSQL cluster in {:?} ...",
            data_dir
        );
        let init_status = Command::new(&initdb)
            .arg("-D")
            .arg(&data_dir)
            .arg("-U")
            .arg(&username)
            .arg("--pwfile")
            .arg(&pw_file)
            .arg("-A")
            .arg("scram-sha-256")
            .arg("-E")
            .arg("UTF8")
            .arg("--no-locale")
            .status()
            .context("Failed to execute initdb")?;
        let _ = std::fs::remove_file(&pw_file);
        if !init_status.success() {
            bail!("initdb exited with non-zero status");
        }
    }

    let opts = format!("-h {} -p {}", host, port);
    let start_status = Command::new(&pg_ctl)
        .arg("-D")
        .arg(&data_dir)
        .arg("-l")
        .arg(&pg_log)
        .arg("-o")
        .arg(&opts)
        .arg("-w")
        .arg("start")
        .status()
        .context("Failed to start PostgreSQL via pg_ctl")?;

    if !start_status.success() {
        bail!("pg_ctl start failed; inspect {:?}", pg_log);
    }

    // Safe parameterized psql variable binding (no SQL string interpolation)
    let db_check = Command::new(&psql)
        .env("PGPASSWORD", &password)
        .args([
            "-h",
            &host,
            "-p",
            &port.to_string(),
            "-U",
            &username,
            "-d",
            "postgres",
            "-v",
        ])
        .arg(format!("dbname={}", db_name))
        .args([
            "-tAc",
            "SELECT 1 FROM pg_database WHERE datname = :'dbname'",
        ])
        .output()?;

    let exists = String::from_utf8_lossy(&db_check.stdout).trim() == "1";
    if !exists {
        println!("[pg-manager] Creating database '{}' ...", db_name);
        let c_status = Command::new(&createdb)
            .env("PGPASSWORD", &password)
            .args(["-h", &host, "-p", &port.to_string(), "-U", &username, &db_name])
            .status()?;
        if !c_status.success() {
            bail!("createdb failed for '{}'", db_name);
        }
    }

    println!(
        "[pg-manager] READY: Isolated PostgreSQL running at {}:{} (database='{}', user='{}')",
        host, port, db_name, username
    );

    signal::ctrl_c().await?;
    println!("[pg-manager] Stopping PostgreSQL cleanly...");
    let _ = Command::new(&pg_ctl)
        .arg("-D")
        .arg(&data_dir)
        .args(["-m", "fast", "stop"])
        .status();
    println!("[pg-manager] PostgreSQL stopped.");
    Ok(())
}
