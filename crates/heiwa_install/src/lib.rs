mod provider_surface_sync;
mod runtime_projection;
mod runtime_seed;

use anyhow::Result;
use heiwa_paths::RuntimePaths;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorReport {
    pub rust_version: Option<String>,
    pub node_version: Option<String>,
    pub python_version: Option<String>,
    pub claude_installed: bool,
    pub codex_installed: bool,
    pub gemini_installed: bool,
    pub antigravity_installed: bool,
    pub ollama_installed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineManifest {
    pub device_id: String,
    pub hostname: String,
    pub os: String,
    pub arch: String,
    pub installed_at: String,
    pub runtimes: DoctorReport,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum LauncherWriteStatus {
    WroteLauncher,
    PreservedExistingBinary,
}

pub fn get_heiwa_dir() -> PathBuf {
    RuntimePaths::discover().root().to_path_buf()
}

pub fn check_installation() -> Result<DoctorReport> {
    Ok(DoctorReport {
        rust_version: get_version("rustc", &["--version"]),
        node_version: get_version("node", &["--version"]),
        python_version: get_version("python3", &["--version"]),
        claude_installed: has_command("claude"),
        codex_installed: has_command("codex"),
        gemini_installed: has_command("gemini"),
        antigravity_installed: has_command("antigravity"),
        ollama_installed: has_command("ollama"),
    })
}

pub fn run_install() -> Result<()> {
    println!("Checking prerequisites...");
    let report = check_installation()?;

    let paths = RuntimePaths::discover();
    let heiwa_dir = paths.root().to_path_buf();
    ensure_runtime_layout(&paths)?;
    write_default_config(&paths)?;
    migrate_legacy_files(&paths)?;
    let repo_root = get_repo_root();
    runtime_seed::seed_runtime(&paths, &repo_root)?;
    runtime_projection::write_projections(&paths)?;
    provider_surface_sync::sync_surfaces(&paths, &repo_root)?;
    let launcher_status = write_canonical_launcher(paths.root())?;

    let manifest_path = paths.machine();
    let manifest = load_existing_manifest(&manifest_path).unwrap_or_else(|| MachineManifest {
        device_id: uuid::Uuid::new_v4().to_string(),
        hostname: get_hostname().unwrap_or_else(|| "unknown".to_string()),
        os: env::consts::OS.to_string(),
        arch: env::consts::ARCH.to_string(),
        installed_at: chrono::Utc::now().to_rfc3339(),
        runtimes: report.clone(),
    });
    let manifest = MachineManifest {
        runtimes: report.clone(),
        ..manifest
    };

    fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)?;
    println!("Machine manifest written to {:?}", manifest_path);

    if report.rust_version.is_none() {
        println!("Rust not found. Please install Rust: https://rustup.rs/");
    }

    if report.node_version.is_none() {
        println!("Node.js not found. Please install Node.js: https://nodejs.org/");
    }

    if report.python_version.is_none() {
        println!("Python 3 not found. Please install Python 3.");
    }

    match launcher_status {
        LauncherWriteStatus::WroteLauncher => {
            println!(
                "Installed canonical launcher at {:?}",
                heiwa_dir.join("bin").join("heiwa")
            );
        }
        LauncherWriteStatus::PreservedExistingBinary => {
            println!(
                "Preserved installed Heiwa binary at {:?}",
                heiwa_dir.join("bin").join("heiwa")
            );
        }
    }
    println!("Installation check complete.");

    Ok(())
}

fn ensure_runtime_layout(paths: &RuntimePaths) -> Result<()> {
    fs::create_dir_all(paths.root())?;
    for dirname in [
        "bin",
        "logs",
        "sessions",
        "cache",
        "state",
        "secrets",
        "providers",
        "models",
        "capabilities",
        "modes",
        "policies",
        "generated",
        "artifacts",
    ] {
        fs::create_dir_all(paths.root().join(dirname))?;
    }
    Ok(())
}

fn write_default_config(paths: &RuntimePaths) -> Result<()> {
    let config_path = paths.config();
    if config_path.exists() {
        return Ok(());
    }

    fs::write(
        config_path,
        concat!(
            "[runtime]\n",
            "root = \"~/.heiwa\"\n",
            "default_mode = \"concise\"\n",
            "sessions_owner = \"heiwa\"\n",
            "sandboxes_owner = \"heiwa\"\n\n",
            "[providers]\n",
            "auth_owner = \"provider\"\n",
            "inference_owner = \"provider\"\n",
        ),
    )?;
    Ok(())
}

fn migrate_legacy_files(paths: &RuntimePaths) -> Result<()> {
    migrate_if_missing(
        &paths.root().join("accounts.json"),
        &paths.provider_registry(),
    )?;
    migrate_if_missing(
        &paths.root().join("provider_connections.json"),
        &paths.legacy_connections(),
    )?;
    migrate_if_missing(
        &paths.root().join("identity.json"),
        &paths.identity(),
    )?;
    migrate_if_missing(
        &paths.root().join("connection.json"),
        &paths.connection(),
    )?;
    Ok(())
}

fn migrate_if_missing(old: &Path, new: &Path) -> Result<()> {
    if new.exists() || !old.exists() {
        return Ok(());
    }

    if let Some(parent) = new.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(old, new)?;
    Ok(())
}

fn write_canonical_launcher(heiwa_dir: &Path) -> Result<LauncherWriteStatus> {
    let launcher_path = heiwa_dir.join("bin").join("heiwa");
    if should_preserve_existing_binary(&launcher_path)? {
        return Ok(LauncherWriteStatus::PreservedExistingBinary);
    }
    let repo_root = get_repo_root();
    let launcher = format!(
        r#"#!/bin/zsh
set -euo pipefail

REPO_ROOT="${{HEIWA_ROOT:-{repo_root}}}"
RUST_BIN_OVERRIDE="${{HEIWA_SHELL_BIN:-}}"

typeset -a candidates
if [[ -n "$RUST_BIN_OVERRIDE" ]]; then
  candidates+=("$RUST_BIN_OVERRIDE")
fi
candidates+=(
  "$REPO_ROOT/target/debug/heiwa"
  "$REPO_ROOT/target/release/heiwa"
  "$REPO_ROOT/target/debug/heiwa-shell"
  "$REPO_ROOT/target/release/heiwa-shell"
)

for candidate in "${{candidates[@]}}"; do
  if [[ -x "$candidate" ]]; then
    exec "$candidate" "$@"
  fi
done

LAUNCHER="$REPO_ROOT/apps/heiwa_cli/bin/heiwa"
if [[ -f "$LAUNCHER" ]]; then
  exec node "$LAUNCHER" "$@"
fi

if [[ -f "$REPO_ROOT/Cargo.toml" ]]; then
  exec cargo run -q -p heiwa-shell --bin heiwa -- "$@"
fi

echo "[FATAL] Could not locate a Heiwa launcher from $REPO_ROOT" >&2
exit 1
"#,
        repo_root = repo_root.display()
    );

    fs::write(&launcher_path, launcher)?;
    #[cfg(unix)]
    fs::set_permissions(&launcher_path, fs::Permissions::from_mode(0o755))?;
    Ok(LauncherWriteStatus::WroteLauncher)
}

fn should_preserve_existing_binary(path: &Path) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }

    let bytes = fs::read(path)?;
    if bytes.starts_with(b"#!/bin/zsh") {
        return Ok(false);
    }

    Ok(true)
}

fn get_repo_root() -> PathBuf {
    env::var("HEIWA_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .expect("crate parent")
                .parent()
                .expect("repo root")
                .to_path_buf()
        })
}

fn load_existing_manifest(path: &Path) -> Option<MachineManifest> {
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
}

fn get_version(cmd: &str, args: &[&str]) -> Option<String> {
    Command::new(cmd)
        .args(args)
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                None
            }
        })
}

fn has_command(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn get_hostname() -> Option<String> {
    Command::new("hostname")
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                None
            }
        })
}

#[cfg(test)]
mod tests {
    use super::{write_canonical_launcher, LauncherWriteStatus};
    use std::fs;

    #[test]
    fn existing_binary_is_reported_as_preserved() {
        let tmp = std::env::temp_dir().join(format!(
            "heiwa-launcher-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        let bin_dir = tmp.join("bin");
        fs::create_dir_all(&bin_dir).expect("create bin dir");
        fs::write(bin_dir.join("heiwa"), b"\x7fELFfake-heiwa-binary").expect("write fake binary");

        let status = write_canonical_launcher(&tmp).expect("write launcher");
        assert!(matches!(status, LauncherWriteStatus::PreservedExistingBinary));

        let _ = fs::remove_dir_all(&tmp);
    }
}
