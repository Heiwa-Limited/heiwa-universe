use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

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
pub struct AiOpsReport {
    pub mcp_notion_http: bool,
    pub biome_configured: bool,
    pub npm_lint_uses_biome: bool,
    pub ci_lint_uses_biome: bool,
    pub ci_clippy_dead_code_enforced: bool,
    pub ci_unused_deps_uses_cargo_machete: bool,
}

impl AiOpsReport {
    pub fn is_clean(&self) -> bool {
        self.mcp_notion_http
            && self.biome_configured
            && self.npm_lint_uses_biome
            && self.ci_lint_uses_biome
            && self.ci_clippy_dead_code_enforced
            && self.ci_unused_deps_uses_cargo_machete
    }
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginSource {
    pub scheme: String,
    pub host: String,
    pub owner: String,
    pub repo: String,
    pub reference: Option<String>,
}

impl PluginSource {
    pub fn canonical(&self) -> String {
        match &self.reference {
            Some(reference) => {
                format!("{}:{}/{}@{}", self.scheme, self.owner, self.repo, reference)
            }
            None => format!("{}:{}/{}", self.scheme, self.owner, self.repo),
        }
    }

    pub fn clone_url(&self) -> String {
        format!("https://{}/{}/{}.git", self.host, self.owner, self.repo)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPlugin {
    pub source: PluginSource,
    pub canonical: String,
    pub clone_url: String,
    pub install_dir: PathBuf,
    pub receipt_path: PathBuf,
    pub installed_at: String,
}

#[derive(Debug, Clone)]
pub enum InstallOutcome {
    RuntimeBootstrap,
    Plugin(Box<InstalledPlugin>),
}

pub fn get_heiwa_dir() -> PathBuf {
    let home = env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .expect("HOME or USERPROFILE must be set");
    PathBuf::from(home).join(".heiwa")
}

pub fn get_plugins_dir() -> PathBuf {
    get_heiwa_dir().join("plugins")
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

pub fn check_ai_ops() -> Result<AiOpsReport> {
    check_ai_ops_at(&get_repo_root())
}

pub fn check_ai_ops_at(repo_root: &Path) -> Result<AiOpsReport> {
    let mcp_notion_http = mcp_notion_has_http_type(&repo_root.join(".mcp.json"));
    let biome_configured = repo_root.join("biome.json").is_file();
    let npm_lint_uses_biome = package_lint_uses_biome(&repo_root.join("package.json"));
    let ci = fs::read_to_string(repo_root.join(".github/workflows/ci.yml")).unwrap_or_default();

    Ok(AiOpsReport {
        mcp_notion_http,
        biome_configured,
        npm_lint_uses_biome,
        ci_lint_uses_biome: ci.contains("npm run lint") && ci.contains("Run Biome"),
        ci_clippy_dead_code_enforced: ci.contains("cargo clippy")
            && ci.contains("-D warnings")
            && !ci.contains("-A dead_code"),
        ci_unused_deps_uses_cargo_machete: ci.contains("cargo machete"),
    })
}

pub fn run_install() -> Result<()> {
    println!("Checking prerequisites...");
    let report = check_installation()?;

    let heiwa_dir = get_heiwa_dir();
    ensure_runtime_layout(&heiwa_dir)?;
    write_canonical_launcher(&heiwa_dir)?;

    let manifest_path = heiwa_dir.join("machine.json");
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

    println!(
        "Installed canonical launcher at {:?}",
        heiwa_dir.join("bin").join("heiwa")
    );
    println!("Installation check complete.");

    Ok(())
}

pub fn run_install_target(target: Option<&str>) -> Result<InstallOutcome> {
    match target {
        Some(raw) => install_plugin(raw)
            .map(Box::new)
            .map(InstallOutcome::Plugin),
        None => {
            run_install()?;
            Ok(InstallOutcome::RuntimeBootstrap)
        }
    }
}

pub fn parse_plugin_source(raw: &str) -> Result<PluginSource> {
    let repo_spec = raw
        .strip_prefix("gh:")
        .ok_or_else(|| anyhow!("plugin source must start with gh:"))?;

    let (repo_path, reference) = match repo_spec.split_once('@') {
        Some((_, "")) => {
            return Err(anyhow!("plugin reference cannot be empty"));
        }
        Some((path, reference)) => (path, Some(reference.to_string())),
        None => (repo_spec, None),
    };

    let mut segments = repo_path.split('/');
    let owner = segments
        .next()
        .filter(|segment| !segment.is_empty())
        .ok_or_else(|| anyhow!("plugin source must be gh:owner/repo"))?;
    let repo = segments
        .next()
        .filter(|segment| !segment.is_empty())
        .ok_or_else(|| anyhow!("plugin source must be gh:owner/repo"))?;

    if segments.next().is_some() {
        return Err(anyhow!("plugin source must be gh:owner/repo"));
    }

    for segment in [owner, repo] {
        if !segment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        {
            return Err(anyhow!(
                "plugin source segments may only contain letters, digits, '.', '-', and '_'"
            ));
        }
    }

    Ok(PluginSource {
        scheme: "gh".to_string(),
        host: "github.com".to_string(),
        owner: owner.to_string(),
        repo: repo.to_string(),
        reference,
    })
}

pub fn plan_plugin_install(raw: &str) -> Result<InstalledPlugin> {
    let source = parse_plugin_source(raw)?;
    let install_dir = get_plugins_dir()
        .join(&source.host)
        .join(&source.owner)
        .join(&source.repo);
    let receipt_path = install_dir.join(".heiwa-install.json");

    Ok(InstalledPlugin {
        canonical: source.canonical(),
        clone_url: source.clone_url(),
        source,
        install_dir,
        receipt_path,
        installed_at: chrono::Utc::now().to_rfc3339(),
    })
}

pub fn install_plugin(raw: &str) -> Result<InstalledPlugin> {
    let plugin = plan_plugin_install(raw)?;
    let heiwa_dir = get_heiwa_dir();

    ensure_runtime_layout(&heiwa_dir)?;

    let parent = plugin
        .install_dir
        .parent()
        .ok_or_else(|| anyhow!("plugin install path is missing a parent directory"))?;
    fs::create_dir_all(parent)?;

    if plugin.install_dir.exists() {
        return Err(anyhow!(
            "plugin already installed at {}",
            plugin.install_dir.display()
        ));
    }

    clone_plugin_repo(&plugin.source, &plugin.install_dir)?;
    fs::write(&plugin.receipt_path, serde_json::to_string_pretty(&plugin)?)?;

    Ok(plugin)
}

fn mcp_notion_has_http_type(path: &Path) -> bool {
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|json| {
            json.pointer("/mcpServers/notion/type")
                .and_then(|value| value.as_str())
                .map(|value| value == "http")
        })
        .unwrap_or(false)
}

fn package_lint_uses_biome(path: &Path) -> bool {
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|json| {
            json.pointer("/scripts/lint")
                .and_then(|value| value.as_str())
                .map(|value| value.contains("biome ci"))
        })
        .unwrap_or(false)
}

fn ensure_runtime_layout(heiwa_dir: &Path) -> Result<()> {
    fs::create_dir_all(heiwa_dir)?;
    for dirname in [
        "bin", "logs", "sessions", "cache", "state", "secrets", "plugins",
    ] {
        fs::create_dir_all(heiwa_dir.join(dirname))?;
    }
    Ok(())
}

fn write_canonical_launcher(heiwa_dir: &Path) -> Result<()> {
    let current_exe = env::current_exe().unwrap_or_default();
    let repo_root = get_repo_root();
    write_canonical_launcher_internal(heiwa_dir, &current_exe, &repo_root)
}

fn write_canonical_launcher_internal(
    heiwa_dir: &Path,
    current_exe: &Path,
    repo_root: &Path,
) -> Result<()> {
    let launcher_path = heiwa_dir.join("bin").join("heiwa");

    // Robust dev-env check: Does Cargo.toml exist where we expect it in the monorepo?
    let is_dev_env = repo_root.join("Cargo.toml").exists();

    if !is_dev_env && current_exe.exists() {
        if current_exe != launcher_path {
            fs::copy(current_exe, &launcher_path)?;
            #[cfg(unix)]
            fs::set_permissions(&launcher_path, fs::Permissions::from_mode(0o755))?;
        }
        return Ok(());
    }

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
    Ok(())
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

fn clone_plugin_repo(source: &PluginSource, install_dir: &Path) -> Result<()> {
    if !has_command("git") {
        return Err(anyhow!("git is required to install plugins"));
    }

    let clone_output = Command::new("git")
        .arg("clone")
        .arg(source.clone_url())
        .arg(install_dir)
        .output()
        .with_context(|| format!("failed to clone {}", source.canonical()))?;

    if !clone_output.status.success() {
        let _ = fs::remove_dir_all(install_dir);
        let stderr = String::from_utf8_lossy(&clone_output.stderr)
            .trim()
            .to_string();
        let stdout = String::from_utf8_lossy(&clone_output.stdout)
            .trim()
            .to_string();
        let detail = if !stderr.is_empty() { stderr } else { stdout };
        return Err(anyhow!(
            "git clone failed for {}: {}",
            source.canonical(),
            detail
        ));
    }

    if let Some(reference) = &source.reference {
        let checkout_output = Command::new("git")
            .arg("-C")
            .arg(install_dir)
            .arg("checkout")
            .arg(reference)
            .output()
            .with_context(|| format!("failed to checkout {}", reference))?;

        if !checkout_output.status.success() {
            let _ = fs::remove_dir_all(install_dir);
            let stderr = String::from_utf8_lossy(&checkout_output.stderr)
                .trim()
                .to_string();
            let stdout = String::from_utf8_lossy(&checkout_output.stdout)
                .trim()
                .to_string();
            let detail = if !stderr.is_empty() { stderr } else { stdout };
            return Err(anyhow!(
                "git checkout failed for {}@{}: {}",
                source.canonical(),
                reference,
                detail
            ));
        }
    }

    Ok(())
}

fn get_hostname() -> Option<String> {
    Command::new("hostname").output().ok().and_then(|output| {
        if output.status.success() {
            Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_write_canonical_launcher_portable() -> Result<()> {
        let tmp = tempdir()?;
        let heiwa_dir = tmp.path().join(".heiwa");
        fs::create_dir_all(heiwa_dir.join("bin"))?;

        let mock_exe = tmp.path().join("heiwa-portable");
        fs::write(&mock_exe, "binary content")?;

        let mock_repo = tmp.path().join("not-a-repo");
        fs::create_dir_all(&mock_repo)?;
        // No Cargo.toml here

        write_canonical_launcher_internal(&heiwa_dir, &mock_exe, &mock_repo)?;

        let target = heiwa_dir.join("bin").join("heiwa");
        assert!(target.exists());
        let content = fs::read_to_string(target)?;
        assert_eq!(content, "binary content");

        Ok(())
    }

    #[test]
    fn test_write_canonical_launcher_dev() -> Result<()> {
        let tmp = tempdir()?;
        let heiwa_dir = tmp.path().join(".heiwa");
        fs::create_dir_all(heiwa_dir.join("bin"))?;

        let mock_exe = tmp.path().join("target/debug/heiwa");
        fs::create_dir_all(mock_exe.parent().unwrap())?;
        fs::write(&mock_exe, "binary content")?;

        let mock_repo = tmp.path().join("heiwa-universe");
        fs::create_dir_all(&mock_repo)?;
        fs::write(mock_repo.join("Cargo.toml"), "")?;

        write_canonical_launcher_internal(&heiwa_dir, &mock_exe, &mock_repo)?;

        let target = heiwa_dir.join("bin").join("heiwa");
        assert!(target.exists());
        let content = fs::read_to_string(target)?;
        assert!(content.starts_with("#!/bin/zsh"));
        assert!(content.contains("REPO_ROOT=\"${HEIWA_ROOT:-"));

        Ok(())
    }
}
