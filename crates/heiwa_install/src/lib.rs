use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::env;
use std::error::Error;
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
pub struct DirectoryProbe {
    pub name: String,
    pub path: PathBuf,
    pub exists: bool,
    pub writable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutReport {
    pub root: PathBuf,
    pub directories: Vec<DirectoryProbe>,
}

impl LayoutReport {
    pub fn is_complete(&self) -> bool {
        self.directories
            .iter()
            .all(|dir| dir.exists && dir.writable)
    }

    pub fn missing(&self) -> Vec<&str> {
        self.directories
            .iter()
            .filter(|dir| !dir.exists)
            .map(|dir| dir.name.as_str())
            .collect()
    }
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
    #[serde(default = "machine_schema_version")]
    pub schema_version: String,
    pub device_id: String,
    #[serde(default)]
    pub display_name: String,
    pub hostname: String,
    pub os: String,
    pub arch: String,
    #[serde(default = "full_node_class")]
    pub device_class: String,
    pub installed_at: String,
    #[serde(default)]
    pub refreshed_at: String,
    #[serde(default)]
    pub hardware: MachineHardware,
    #[serde(default)]
    pub capabilities: MachineCapabilities,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<MachineRuntime>,
    pub runtimes: DoctorReport,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MachineHardware {
    pub logical_cpu_count: u32,
    pub memory_total_bytes: u64,
    pub cpu_model: Option<String>,
    pub hardware_model: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MachineCapabilities {
    pub provider_clis: Vec<String>,
    pub local_model_runtimes: Vec<String>,
    pub host_surfaces: Vec<String>,
    pub display_surfaces: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineRuntime {
    pub version: String,
    pub channel: String,
    pub install_path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachineManifestLoadIssue {
    ReadFailed,
    InvalidJson,
    UnsupportedSchema,
    InvalidShape,
}

#[derive(Debug)]
pub struct MachineManifestLoadError {
    issue: MachineManifestLoadIssue,
    detail: String,
}

impl MachineManifestLoadError {
    fn new(issue: MachineManifestLoadIssue, detail: impl Into<String>) -> Self {
        Self {
            issue,
            detail: detail.into(),
        }
    }

    pub fn issue(&self) -> MachineManifestLoadIssue {
        self.issue
    }

    pub fn user_message(&self) -> &'static str {
        match self.issue {
            MachineManifestLoadIssue::ReadFailed => "Machine identity file could not be read.",
            MachineManifestLoadIssue::InvalidJson => "Machine identity file is corrupt.",
            MachineManifestLoadIssue::UnsupportedSchema => {
                "Machine identity was written by a newer or incompatible Heiwa build."
            }
            MachineManifestLoadIssue::InvalidShape => "Machine identity file is incomplete.",
        }
    }
}

impl std::fmt::Display for MachineManifestLoadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl Error for MachineManifestLoadError {}

fn machine_schema_version() -> String {
    "heiwa_machine_v1".to_string()
}

fn full_node_class() -> String {
    "full_node".to_string()
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

/// The runtime root.
///
/// # Panics
///
/// Panics when no per-user root can be resolved — none of `HEIWA_HOME`,
/// `HEIWA_STATE_DIR`, `HOME`, or `USERPROFILE` is set and the platform
/// reports no home directory. Install and doctor flows treat that as
/// unrecoverable, because the alternative is provisioning a runtime tree
/// wherever the process happened to start. Callers that must not panic —
/// libraries, embedders, anything running under an empty environment — use
/// [`try_get_heiwa_dir`] instead.
pub fn get_heiwa_dir() -> PathBuf {
    try_get_heiwa_dir().expect("HOME, USERPROFILE, or HEIWA_HOME must be set")
}

/// The runtime root, or `None` when no real home exists.
///
/// Install and doctor flows create and inspect this directory tree, so a
/// cwd-relative fallback would provision a phantom runtime wherever the
/// process happened to start.
pub fn try_get_heiwa_dir() -> Option<PathBuf> {
    heiwa_config::HeiwaPaths::try_resolve().map(|paths| paths.runtime_root)
}

pub fn get_plugins_dir() -> PathBuf {
    get_heiwa_dir().join("plugins")
}

const RUNTIME_LAYOUT_DIRS: &[&str] = &[
    "app", "bin", "logs", "sessions", "cache", "state", "secrets", "plugins",
];

pub fn check_runtime_layout() -> LayoutReport {
    check_runtime_layout_at(&get_heiwa_dir())
}

pub fn check_runtime_layout_at(root: &Path) -> LayoutReport {
    let directories = RUNTIME_LAYOUT_DIRS
        .iter()
        .map(|name| {
            let path = root.join(name);
            let exists = path.is_dir();
            let writable = if exists {
                fs::metadata(&path)
                    .map(|meta| !meta.permissions().readonly())
                    .unwrap_or(false)
            } else {
                false
            };
            DirectoryProbe {
                name: (*name).to_string(),
                path,
                exists,
                writable,
            }
        })
        .collect();
    LayoutReport {
        root: root.to_path_buf(),
        directories,
    }
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
    write_home_app_launcher(&heiwa_dir)?;

    refresh_machine_manifest_at(&heiwa_dir, report.clone(), None)?;
    let manifest_path = heiwa_dir.join("machine.json");
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
    println!(
        "Installed Heiwa.app launcher at {:?}",
        heiwa_dir.join("app").join("Heiwa.app")
    );
    println!("Installation check complete.");

    Ok(())
}

/// Refresh local machine identity and capability truth while preserving the
/// installation's stable device id and original install timestamp.
pub fn refresh_machine_manifest_for_runtime(runtime: MachineRuntime) -> Result<MachineManifest> {
    let report = check_installation()?;
    refresh_machine_manifest_at(&get_heiwa_dir(), report, Some(runtime))
}

pub fn load_machine_manifest(
) -> std::result::Result<Option<MachineManifest>, MachineManifestLoadError> {
    load_existing_manifest(&get_heiwa_dir().join("machine.json"))
}

pub fn probe_machine_hardware() -> MachineHardware {
    MachineHardware {
        logical_cpu_count: std::thread::available_parallelism()
            .map(|count| count.get() as u32)
            .unwrap_or(1),
        memory_total_bytes: total_memory_bytes().unwrap_or(0),
        cpu_model: cpu_model(),
        hardware_model: hardware_model(),
    }
}

fn refresh_machine_manifest_at(
    heiwa_dir: &Path,
    report: DoctorReport,
    runtime: Option<MachineRuntime>,
) -> Result<MachineManifest> {
    fs::create_dir_all(heiwa_dir)?;
    let path = heiwa_dir.join("machine.json");
    let existing = load_existing_manifest(&path).map_err(anyhow::Error::new)?;
    let now = chrono::Utc::now().to_rfc3339();
    let hostname = get_hostname().unwrap_or_else(|| "unknown".to_string());
    let display_name = existing
        .as_ref()
        .map(|manifest| manifest.display_name.trim())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| hostname.clone());
    let manifest = MachineManifest {
        schema_version: machine_schema_version(),
        device_id: existing
            .as_ref()
            .map(|manifest| manifest.device_id.clone())
            .filter(|id| !id.trim().is_empty())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        display_name,
        hostname,
        os: env::consts::OS.to_string(),
        arch: env::consts::ARCH.to_string(),
        device_class: full_node_class(),
        installed_at: existing
            .as_ref()
            .map(|manifest| manifest.installed_at.clone())
            .filter(|timestamp| !timestamp.trim().is_empty())
            .unwrap_or_else(|| now.clone()),
        refreshed_at: now,
        hardware: probe_machine_hardware(),
        capabilities: capabilities_from_report(&report),
        runtime: runtime.or_else(|| existing.and_then(|manifest| manifest.runtime)),
        runtimes: report,
    };
    write_machine_manifest(&path, &manifest)?;
    Ok(manifest)
}

fn capabilities_from_report(report: &DoctorReport) -> MachineCapabilities {
    let mut provider_clis = Vec::new();
    if report.claude_installed {
        provider_clis.push("claude".to_string());
    }
    if report.codex_installed {
        provider_clis.push("codex".to_string());
    }
    if report.gemini_installed {
        provider_clis.push("gemini".to_string());
    }
    if report.antigravity_installed {
        provider_clis.push("antigravity".to_string());
    }
    let local_model_runtimes = if report.ollama_installed {
        vec!["ollama".to_string()]
    } else {
        Vec::new()
    };
    MachineCapabilities {
        provider_clis,
        local_model_runtimes,
        host_surfaces: vec!["terminal".to_string(), "desktop".to_string()],
        display_surfaces: vec!["desktop".to_string()],
    }
}

fn write_machine_manifest(path: &Path, manifest: &MachineManifest) -> Result<()> {
    let temporary = path.with_extension(format!("json.tmp-{}", uuid::Uuid::new_v4()));
    fs::write(&temporary, serde_json::to_vec_pretty(manifest)?)?;
    #[cfg(unix)]
    fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))?;
    replace_machine_manifest(&temporary, path)?;
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn replace_machine_manifest(temporary: &Path, path: &Path) -> Result<()> {
    fs::rename(temporary, path)?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn replace_machine_manifest(temporary: &Path, path: &Path) -> Result<()> {
    // Windows rename does not replace an existing file. Keep the previous
    // manifest recoverable until the new one occupies the canonical path.
    let backup = path.with_extension(format!("json.previous-{}", uuid::Uuid::new_v4()));
    if path.exists() {
        fs::rename(path, &backup)?;
    }
    if let Err(error) = fs::rename(temporary, path) {
        if backup.exists() {
            let _ = fs::rename(&backup, path);
        }
        return Err(error.into());
    }
    if backup.exists() {
        fs::remove_file(backup)?;
    }
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
    // ConfigRoot owns first-run creation of the directories it resolves
    // (runtime root, state, sessions, evidence) so the resolver and the
    // installer cannot disagree about where they are. The install-only
    // directories below are layered on top.
    if let Some(paths) = heiwa_config::HeiwaPaths::try_resolve() {
        if paths.runtime_root == heiwa_dir {
            paths.ensure()?;
        }
    }
    fs::create_dir_all(heiwa_dir)?;
    for dirname in [
        "app", "bin", "logs", "sessions", "cache", "state", "secrets", "plugins",
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

    // An installed binary may still carry a build-time repository path that exists
    // on the build machine. Never replace the executable that is currently running.
    if current_exe == launcher_path && current_exe.exists() {
        return Ok(());
    }

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

fn write_home_app_launcher(heiwa_dir: &Path) -> Result<()> {
    write_home_app_launcher_internal(heiwa_dir)
}

fn write_home_app_launcher_internal(heiwa_dir: &Path) -> Result<()> {
    if let Some(desktop_bundle) = find_built_desktop_app_bundle() {
        return install_desktop_app_bundle(heiwa_dir, &desktop_bundle);
    }

    let bundle_root = heiwa_dir.join("app").join("Heiwa.app");
    let contents_dir = bundle_root.join("Contents");
    let macos_dir = contents_dir.join("MacOS");
    let resources_dir = contents_dir.join("Resources");
    fs::create_dir_all(&macos_dir)?;
    fs::create_dir_all(&resources_dir)?;

    let executable_path = macos_dir.join("Heiwa");
    let launcher = format!(
        r#"#!/bin/zsh
set -euo pipefail

HEIWA_HOME="${{HEIWA_HOME:-{heiwa_dir}}}"
HEIWA_BIN="${{HEIWA_BIN:-$HEIWA_HOME/bin/heiwa}}"

if [[ ! -x "$HEIWA_BIN" ]]; then
  echo "[FATAL] Heiwa runtime launcher is missing: $HEIWA_BIN" >&2
  exit 1
fi

exec "$HEIWA_BIN" app start "$@"
"#,
        heiwa_dir = heiwa_dir.display()
    );
    fs::write(&executable_path, launcher)?;
    #[cfg(unix)]
    fs::set_permissions(&executable_path, fs::Permissions::from_mode(0o755))?;

    let plist = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleDisplayName</key>
  <string>Heiwa</string>
  <key>CFBundleExecutable</key>
  <string>Heiwa</string>
  <key>CFBundleIdentifier</key>
  <string>ltd.heiwa.app.local</string>
  <key>CFBundleName</key>
  <string>Heiwa</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.0-local</string>
  <key>CFBundleVersion</key>
  <string>1</string>
  <key>LSMinimumSystemVersion</key>
  <string>14.0</string>
  <key>NSPrincipalClass</key>
  <string>NSApplication</string>
</dict>
</plist>
"#;
    fs::write(contents_dir.join("Info.plist"), plist)?;

    let bin_launcher_path = heiwa_dir.join("bin").join("heiwa-app");
    let bin_launcher = format!(
        r#"#!/bin/zsh
set -euo pipefail

exec "{app_executable}" "$@"
"#,
        app_executable = executable_path.display()
    );
    fs::write(&bin_launcher_path, bin_launcher)?;
    #[cfg(unix)]
    fs::set_permissions(&bin_launcher_path, fs::Permissions::from_mode(0o755))?;

    Ok(())
}

fn find_built_desktop_app_bundle() -> Option<PathBuf> {
    let repo_root = get_repo_root();
    let candidate = repo_root
        .join("target")
        .join("release")
        .join("bundle")
        .join("macos")
        .join("Heiwa.app");
    if candidate
        .join("Contents")
        .join("MacOS")
        .join("Heiwa")
        .is_file()
    {
        Some(candidate)
    } else {
        None
    }
}

pub fn install_desktop_app_bundle(heiwa_dir: &Path, desktop_bundle: &Path) -> Result<()> {
    let app_root = heiwa_dir.join("app");
    let target_bundle = app_root.join("Heiwa.app");
    let staging_bundle = app_root.join(".Heiwa.app.installing");
    let backup_bundle = app_root.join(".Heiwa.app.previous");
    fs::create_dir_all(&app_root)?;
    if !target_bundle.exists() && backup_bundle.exists() {
        fs::rename(&backup_bundle, &target_bundle).with_context(|| {
            format!(
                "restore interrupted Heiwa.app backup to {}",
                target_bundle.display()
            )
        })?;
    }

    let source_executable = desktop_bundle.join("Contents").join("MacOS").join("Heiwa");
    if !source_executable.is_file() {
        return Err(anyhow!(
            "Tauri Heiwa.app bundle missing executable: {}",
            source_executable.display()
        ));
    }

    if staging_bundle.exists() {
        fs::remove_dir_all(&staging_bundle).with_context(|| {
            format!(
                "remove stale Heiwa.app staging bundle at {}",
                staging_bundle.display()
            )
        })?;
    }
    copy_dir_all(desktop_bundle, &staging_bundle).with_context(|| {
        format!(
            "stage Tauri Heiwa.app from {} to {}",
            desktop_bundle.display(),
            staging_bundle.display()
        )
    })?;

    let staging_executable = staging_bundle.join("Contents").join("MacOS").join("Heiwa");
    if !staging_executable.is_file() {
        return Err(anyhow!(
            "staged Tauri Heiwa.app bundle missing executable: {}",
            staging_executable.display()
        ));
    }
    #[cfg(unix)]
    fs::set_permissions(&staging_executable, fs::Permissions::from_mode(0o755))?;

    if backup_bundle.exists() {
        fs::remove_dir_all(&backup_bundle).with_context(|| {
            format!(
                "remove stale Heiwa.app backup at {}",
                backup_bundle.display()
            )
        })?;
    }
    let had_installed_bundle = target_bundle.exists();
    if had_installed_bundle {
        fs::rename(&target_bundle, &backup_bundle).with_context(|| {
            format!(
                "stage installed Heiwa.app for rollback at {}",
                backup_bundle.display()
            )
        })?;
    }
    if let Err(error) = fs::rename(&staging_bundle, &target_bundle) {
        if had_installed_bundle {
            fs::rename(&backup_bundle, &target_bundle).with_context(|| {
                format!("restore previous Heiwa.app after install failed: {error}")
            })?;
        }
        return Err(error)
            .with_context(|| format!("promote staged Heiwa.app to {}", target_bundle.display()));
    }
    if backup_bundle.exists() {
        fs::remove_dir_all(&backup_bundle).with_context(|| {
            format!(
                "remove replaced Heiwa.app backup at {}",
                backup_bundle.display()
            )
        })?;
    }

    let executable_path = target_bundle.join("Contents").join("MacOS").join("Heiwa");

    let bin_launcher_path = heiwa_dir.join("bin").join("heiwa-app");
    fs::create_dir_all(bin_launcher_path.parent().expect("launcher parent"))?;
    let bin_launcher = format!(
        r#"#!/bin/zsh
set -euo pipefail

exec "{app_executable}" "$@"
"#,
        app_executable = executable_path.display()
    );
    fs::write(&bin_launcher_path, bin_launcher)?;
    #[cfg(unix)]
    fs::set_permissions(&bin_launcher_path, fs::Permissions::from_mode(0o755))?;
    Ok(())
}

fn copy_dir_all(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&source_path, &target_path)?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &target_path)?;
            #[cfg(unix)]
            fs::set_permissions(&target_path, entry.metadata()?.permissions())?;
        }
    }
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

fn load_existing_manifest(
    path: &Path,
) -> std::result::Result<Option<MachineManifest>, MachineManifestLoadError> {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(MachineManifestLoadError::new(
                MachineManifestLoadIssue::ReadFailed,
                format!("read machine manifest at {}: {error}", path.display()),
            ))
        }
    };
    let value: serde_json::Value = serde_json::from_str(&raw).map_err(|error| {
        MachineManifestLoadError::new(
            MachineManifestLoadIssue::InvalidJson,
            format!("parse machine manifest at {}: {error}", path.display()),
        )
    })?;
    if let Some(schema) = value
        .get("schema_version")
        .and_then(serde_json::Value::as_str)
    {
        if schema != machine_schema_version() {
            return Err(MachineManifestLoadError::new(
                MachineManifestLoadIssue::UnsupportedSchema,
                format!("unsupported machine manifest schema at {}", path.display()),
            ));
        }
    }
    serde_json::from_value(value).map(Some).map_err(|error| {
        MachineManifestLoadError::new(
            MachineManifestLoadIssue::InvalidShape,
            format!("decode machine manifest at {}: {error}", path.display()),
        )
    })
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

#[cfg(target_os = "macos")]
fn total_memory_bytes() -> Option<u64> {
    command_stdout("/usr/sbin/sysctl", &["-n", "hw.memsize"])?
        .parse()
        .ok()
}

#[cfg(target_os = "linux")]
fn total_memory_bytes() -> Option<u64> {
    fs::read_to_string("/proc/meminfo")
        .ok()?
        .lines()
        .find_map(|line| {
            let kb = line.strip_prefix("MemTotal:")?.split_whitespace().next()?;
            kb.parse::<u64>().ok().map(|value| value * 1024)
        })
}

#[cfg(target_os = "windows")]
fn total_memory_bytes() -> Option<u64> {
    command_stdout(
        "powershell.exe",
        &[
            "-NoProfile",
            "-Command",
            "(Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory",
        ],
    )?
    .parse()
    .ok()
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn total_memory_bytes() -> Option<u64> {
    None
}

#[cfg(target_os = "macos")]
fn cpu_model() -> Option<String> {
    command_stdout("/usr/sbin/sysctl", &["-n", "machdep.cpu.brand_string"])
}

#[cfg(target_os = "linux")]
fn cpu_model() -> Option<String> {
    fs::read_to_string("/proc/cpuinfo")
        .ok()?
        .lines()
        .find_map(|line| line.split_once("model name").map(|(_, value)| value))
        .map(|value| value.trim_start_matches(':').trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(target_os = "windows")]
fn cpu_model() -> Option<String> {
    env::var("PROCESSOR_IDENTIFIER")
        .ok()
        .filter(|value| !value.trim().is_empty())
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn cpu_model() -> Option<String> {
    None
}

#[cfg(target_os = "macos")]
fn hardware_model() -> Option<String> {
    command_stdout("/usr/sbin/sysctl", &["-n", "hw.model"])
}

#[cfg(target_os = "linux")]
fn hardware_model() -> Option<String> {
    fs::read_to_string("/sys/devices/virtual/dmi/id/product_name")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(target_os = "windows")]
fn hardware_model() -> Option<String> {
    command_stdout(
        "powershell.exe",
        &[
            "-NoProfile",
            "-Command",
            "(Get-CimInstance Win32_ComputerSystem).Model",
        ],
    )
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn hardware_model() -> Option<String> {
    None
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn command_stdout(command: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(command).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!value.is_empty()).then_some(value)
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
    fn test_write_canonical_launcher_preserves_running_installed_binary() -> Result<()> {
        let tmp = tempdir()?;
        let heiwa_dir = tmp.path().join(".heiwa");
        let launcher_path = heiwa_dir.join("bin").join("heiwa");
        fs::create_dir_all(launcher_path.parent().expect("launcher parent"))?;
        fs::write(&launcher_path, "installed binary")?;

        let mock_repo = tmp.path().join("heiwa-universe");
        fs::create_dir_all(&mock_repo)?;
        fs::write(mock_repo.join("Cargo.toml"), "")?;

        write_canonical_launcher_internal(&heiwa_dir, &launcher_path, &mock_repo)?;

        assert_eq!(fs::read_to_string(launcher_path)?, "installed binary");
        Ok(())
    }

    #[test]
    fn test_check_runtime_layout_reports_missing_dirs() -> Result<()> {
        let tmp = tempdir()?;
        let root = tmp.path().join(".heiwa");
        fs::create_dir_all(&root)?;
        // Only create some of the expected dirs
        fs::create_dir_all(root.join("bin"))?;
        fs::create_dir_all(root.join("logs"))?;

        let report = check_runtime_layout_at(&root);
        assert_eq!(report.root, root);
        assert_eq!(report.directories.len(), RUNTIME_LAYOUT_DIRS.len());

        let bin = report
            .directories
            .iter()
            .find(|d| d.name == "bin")
            .expect("bin probe present");
        assert!(bin.exists, "bin should be present");
        assert!(bin.writable, "bin should be writable on a tempdir");

        let sessions = report
            .directories
            .iter()
            .find(|d| d.name == "sessions")
            .expect("sessions probe present");
        assert!(!sessions.exists, "sessions was never created");
        assert!(!sessions.writable);

        assert!(!report.is_complete());
        let missing = report.missing();
        assert!(missing.contains(&"sessions"));
        assert!(missing.contains(&"plugins"));
        assert!(!missing.contains(&"bin"));
        Ok(())
    }

    #[test]
    fn test_check_runtime_layout_complete_after_ensure() -> Result<()> {
        let tmp = tempdir()?;
        let root = tmp.path().join(".heiwa");
        ensure_runtime_layout(&root)?;
        let report = check_runtime_layout_at(&root);
        assert!(
            report.is_complete(),
            "ensure_runtime_layout should yield a complete layout: missing={:?}",
            report.missing()
        );
        assert!(report.missing().is_empty());
        Ok(())
    }

    // The desktop bundle install writes a zsh shim with POSIX paths; on
    // Windows the joined paths render with backslashes and the flow itself
    // is macOS-only, so keep the test off Windows.
    #[test]
    #[cfg(not(windows))]
    fn test_install_built_desktop_app_bundle_copies_tauri_bundle_and_shim() -> Result<()> {
        let tmp = tempdir()?;
        let heiwa_dir = tmp.path().join(".heiwa");
        fs::create_dir_all(heiwa_dir.join("bin"))?;

        let source_bundle = tmp.path().join("target/release/bundle/macos/Heiwa.app");
        let source_macos = source_bundle.join("Contents").join("MacOS");
        fs::create_dir_all(&source_macos)?;
        fs::write(
            source_bundle.join("Contents").join("Info.plist"),
            "tauri plist",
        )?;
        fs::write(source_macos.join("Heiwa"), "tauri binary")?;

        install_desktop_app_bundle(&heiwa_dir, &source_bundle)?;

        let installed_bundle = heiwa_dir.join("app").join("Heiwa.app");
        assert_eq!(
            fs::read_to_string(installed_bundle.join("Contents").join("Info.plist"))?,
            "tauri plist"
        );
        assert_eq!(
            fs::read_to_string(
                installed_bundle
                    .join("Contents")
                    .join("MacOS")
                    .join("Heiwa")
            )?,
            "tauri binary"
        );

        let shim = fs::read_to_string(heiwa_dir.join("bin").join("heiwa-app"))?;
        assert!(
            shim.contains("Contents/MacOS/Heiwa"),
            "shim launches Tauri app: {shim}"
        );
        assert!(shim.starts_with("#!/bin/zsh"));
        Ok(())
    }

    #[test]
    #[cfg(not(windows))]
    fn test_install_built_desktop_app_bundle_preserves_working_bundle_on_invalid_source(
    ) -> Result<()> {
        let tmp = tempdir()?;
        let heiwa_dir = tmp.path().join(".heiwa");
        let installed_macos = heiwa_dir
            .join("app")
            .join("Heiwa.app")
            .join("Contents")
            .join("MacOS");
        fs::create_dir_all(&installed_macos)?;
        fs::write(installed_macos.join("Heiwa"), "working binary")?;

        let invalid_source = tmp.path().join("invalid/Heiwa.app");
        fs::create_dir_all(invalid_source.join("Contents"))?;
        fs::write(invalid_source.join("Contents/Info.plist"), "invalid bundle")?;

        let error = install_desktop_app_bundle(&heiwa_dir, &invalid_source)
            .expect_err("a bundle without Contents/MacOS/Heiwa must be rejected");

        assert!(error.to_string().contains("missing executable"));
        assert_eq!(
            fs::read_to_string(installed_macos.join("Heiwa"))?,
            "working binary"
        );
        Ok(())
    }

    #[test]
    #[cfg(not(windows))]
    fn test_install_built_desktop_app_bundle_recovers_interrupted_backup_before_validation(
    ) -> Result<()> {
        let tmp = tempdir()?;
        let heiwa_dir = tmp.path().join(".heiwa");
        let backup_macos = heiwa_dir
            .join("app")
            .join(".Heiwa.app.previous")
            .join("Contents")
            .join("MacOS");
        fs::create_dir_all(&backup_macos)?;
        fs::write(backup_macos.join("Heiwa"), "recoverable binary")?;

        let invalid_source = tmp.path().join("invalid/Heiwa.app");
        fs::create_dir_all(invalid_source.join("Contents"))?;

        install_desktop_app_bundle(&heiwa_dir, &invalid_source)
            .expect_err("invalid source must still trigger rollback recovery");

        let installed_executable = heiwa_dir
            .join("app")
            .join("Heiwa.app")
            .join("Contents")
            .join("MacOS")
            .join("Heiwa");
        assert_eq!(
            fs::read_to_string(installed_executable)?,
            "recoverable binary"
        );
        assert!(!heiwa_dir.join("app/.Heiwa.app.previous").exists());
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
