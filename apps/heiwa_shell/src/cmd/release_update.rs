//! Update the installed runtime from a published GitHub release.
//!
//! This deliberately mirrors the invariants of the public installer
//! (`apps/heiwa_app/clients/web/install`), because they are the same trust
//! boundary reached two different ways. Both must:
//!
//!   * download over HTTPS from the release assets and nowhere else
//!   * verify the archive against the published SHA-256 checksums file
//!   * refuse an archive containing links, device nodes, traversal segments,
//!     or any path outside the expected archive root
//!   * stage into the destination directory and swap with an atomic rename, so
//!     an interrupted update never leaves a half-written binary in place
//!   * never restart the runtime on its own
//!
//! Extraction shells out to `tar` rather than linking an archive crate: the
//! installer already depends on system `tar` being present, and matching it
//! keeps one audited extraction path instead of two implementations that can
//! drift apart.

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const REPO: &str = "Heiwa-Limited/heiwa-universe";
const LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/Heiwa-Limited/heiwa-universe/releases/latest";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);

/// Platforms with a `.tar.gz` release asset and a supported install layout.
/// Windows ships a `.zip` and has no installer path yet, so it is refused with
/// a pointer to the release page rather than a half-working update.
fn supported_platform(platform: &str) -> bool {
    matches!(platform, "macos-aarch64" | "linux-x86_64")
}

fn archive_name(version: &str, platform: &str) -> String {
    format!("heiwa-{version}-{platform}.tar.gz")
}

fn archive_root(version: &str, platform: &str) -> String {
    format!("heiwa-{version}-{platform}")
}

fn checksums_name(version: &str) -> String {
    format!("heiwa-{version}-checksums.txt")
}

fn asset_url(version: &str, file: &str) -> String {
    format!("https://github.com/{REPO}/releases/download/v{version}/{file}")
}

fn cockpit_dir_name(version: &str, digest: &str) -> String {
    format!("cockpit-{version}-{}", &digest[..12])
}

fn validate_version(version: &str) -> Result<()> {
    let parts: Vec<&str> = version.split('.').collect();
    let shaped = parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()));
    if !shaped {
        bail!("release version must be a stable semantic version such as 0.1.0: {version}");
    }
    Ok(())
}

/// Pull the digest for `archive` out of a `sha256sum`-style checksums file.
fn expected_checksum(checksums: &str, archive: &str) -> Result<String> {
    for line in checksums.lines() {
        let mut fields = line.split_whitespace();
        let (Some(digest), Some(name)) = (fields.next(), fields.next()) else {
            continue;
        };
        // `sha256sum` marks binary entries with a leading `*`.
        if name.strip_prefix('*').unwrap_or(name) != archive {
            continue;
        }
        if digest.len() != 64 || !digest.chars().all(|c| c.is_ascii_hexdigit()) {
            bail!("release checksum entry for {archive} is malformed");
        }
        return Ok(digest.to_ascii_lowercase());
    }
    bail!("release checksums file has no entry for {archive}")
}

/// Reject anything that would write outside the archive root once extracted.
fn validate_listing(listing: &str, root: &str) -> Result<()> {
    let mut entries = 0usize;
    for path in listing.lines() {
        let path = path.trim();
        if path.is_empty() {
            continue;
        }
        entries += 1;
        let inside =
            path == root || path == format!("{root}/") || path.starts_with(&format!("{root}/"));
        if !inside {
            bail!("release archive contains a path outside {root}: {path}");
        }
        let padded = format!("/{path}/");
        if padded.contains("/../") || padded.contains("/./") {
            bail!("release archive contains a traversal path: {path}");
        }
    }
    if entries == 0 {
        bail!("release archive is empty");
    }
    Ok(())
}

/// `tar -tvzf` prints a mode string per entry; only regular files and
/// directories are acceptable. Symlinks and hard links are how an archive
/// escapes its extraction root even when every listed path looks contained.
fn validate_entry_types(verbose_listing: &str) -> Result<()> {
    for line in verbose_listing.lines() {
        let Some(kind) = line.trim_start().chars().next() else {
            continue;
        };
        if kind != '-' && kind != 'd' {
            bail!("release archive contains links or unsupported entry types");
        }
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path)
        .with_context(|| format!("could not read staged download {}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn http_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .user_agent(concat!("heiwa/", env!("CARGO_PKG_VERSION")))
        .https_only(true)
        .build()
        .context("could not build the release update HTTP client")
}

fn resolve_latest_version(client: &reqwest::blocking::Client) -> Result<String> {
    let payload: Value = client
        .get(LATEST_RELEASE_API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .context("could not reach the GitHub release API")?
        .error_for_status()
        .context("the GitHub release API rejected the request")?
        .json()
        .context("the GitHub release API returned an unreadable response")?;
    let tag = payload
        .get("tag_name")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("the GitHub release API response carried no tag_name"))?;
    let version = tag.strip_prefix('v').unwrap_or(tag).to_string();
    validate_version(&version)?;
    Ok(version)
}

fn download(client: &reqwest::blocking::Client, url: &str, dest: &Path) -> Result<()> {
    let bytes = client
        .get(url)
        .send()
        .with_context(|| format!("could not download {url}"))?
        .error_for_status()
        .with_context(|| format!("release asset is not available: {url}"))?
        .bytes()
        .with_context(|| format!("could not read the response body for {url}"))?;
    fs::write(dest, &bytes).with_context(|| format!("could not stage {}", dest.display()))?;
    Ok(())
}

fn tar_output(args: &[&str]) -> Result<String> {
    let output = Command::new("tar")
        .args(args)
        .output()
        .context("could not run tar; install it to update from a release")?;
    if !output.status.success() {
        bail!(
            "tar failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst).with_context(|| format!("could not create {}", dst.display()))?;
    for entry in fs::read_dir(src).with_context(|| format!("could not read {}", src.display()))? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if kind.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), &target)
                .with_context(|| format!("could not copy {}", entry.path().display()))?;
        } else {
            bail!(
                "unexpected entry type in the release cockpit: {}",
                entry.path().display()
            );
        }
    }
    Ok(())
}

#[cfg(unix)]
fn install_executable(staged: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(staged, fs::Permissions::from_mode(0o755))
        .with_context(|| format!("could not mark {} executable", staged.display()))
}

#[cfg(unix)]
fn swap_symlink(link: &Path, target_name: &str) -> Result<()> {
    if link.exists() && !link.is_symlink() {
        bail!(
            "{} exists and is not a managed symlink; move it aside and retry",
            link.display()
        );
    }
    let staged = link.with_file_name(format!(".cockpit-current.{}", std::process::id()));
    let _ = fs::remove_file(&staged);
    std::os::unix::fs::symlink(target_name, &staged).with_context(|| {
        format!(
            "could not stage the cockpit symlink at {}",
            staged.display()
        )
    })?;
    fs::rename(&staged, link)
        .with_context(|| format!("could not swap the cockpit symlink at {}", link.display()))?;
    Ok(())
}

// Windows release assets are `.zip` and the install layout has no symlinked
// cockpit, so there is no supported update path there yet. These stubs keep the
// crate compiling for the Windows test targets that certification builds; the
// runtime refuses the platform long before reaching them.
#[cfg(not(unix))]
fn install_executable(_staged: &Path) -> Result<()> {
    bail!("release update currently supports macOS and Linux only")
}

#[cfg(not(unix))]
fn swap_symlink(_link: &Path, _target_name: &str) -> Result<()> {
    bail!("release update currently supports macOS and Linux only")
}

fn plan(
    install_root: &Path,
    platform: &str,
    current_version: &str,
    latest_version: Option<&str>,
    dry_run: bool,
) -> Value {
    json!({
        "command": "app update",
        "source_mode": "github-release",
        "source": format!("https://github.com/{REPO}/releases"),
        "release_api": LATEST_RELEASE_API,
        "platform": platform,
        "installed_bin": install_root.join("bin").join("heiwa").display().to_string(),
        "current_version": current_version,
        "latest_version": latest_version,
        "update_available": latest_version.map(|latest| latest != current_version),
        "restart_policy": "prompt-before-restart",
        "dry_run": dry_run,
    })
}

fn print_plan(summary: &Value, json_output: bool) {
    if json_output {
        println!("{summary}");
        return;
    }
    let field = |key: &str| {
        summary
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string()
    };
    println!("heiwa app update");
    println!("  source_mode: {}", field("source_mode"));
    println!("  source: {}", field("source"));
    println!("  release_api: {}", field("release_api"));
    println!("  platform: {}", field("platform"));
    println!("  current_version: {}", field("current_version"));
    println!("  target: {}", field("installed_bin"));
    println!("  restart_policy: {}", field("restart_policy"));
    if summary.get("dry_run").and_then(Value::as_bool) == Some(true) {
        println!("  dry_run: true");
    }
}

/// Update the installed runtime in place. Callers on an async runtime must wrap
/// this in `tokio::task::block_in_place` — it performs blocking HTTP.
pub(crate) fn run(
    install_root: PathBuf,
    platform: &str,
    current_version: &str,
    dry_run: bool,
    json_output: bool,
) -> Result<()> {
    if !supported_platform(platform) {
        bail!(
            "release update does not support {platform} yet; download the asset from https://github.com/{REPO}/releases"
        );
    }
    if !cfg!(unix) {
        bail!("release update currently supports macOS and Linux only");
    }

    // A dry run reports the plan without touching the network: it has to stay
    // deterministic and offline so it is usable from a sandboxed CI job.
    if dry_run {
        print_plan(
            &plan(&install_root, platform, current_version, None, true),
            json_output,
        );
        return Ok(());
    }

    let client = http_client()?;
    let latest = resolve_latest_version(&client)?;
    print_plan(
        &plan(
            &install_root,
            platform,
            current_version,
            Some(&latest),
            false,
        ),
        json_output,
    );

    if latest == current_version {
        if !json_output {
            println!("  already on the latest release");
        }
        return Ok(());
    }

    let bin_dir = install_root.join("bin");
    let app_dir = install_root.join("app");
    fs::create_dir_all(&bin_dir)
        .with_context(|| format!("could not create {}", bin_dir.display()))?;
    fs::create_dir_all(&app_dir)
        .with_context(|| format!("could not create {}", app_dir.display()))?;

    // Stage under the install root so the final renames stay on one filesystem.
    let work_dir = install_root
        .join("cache")
        .join(format!("update-{}", std::process::id()));
    let _ = fs::remove_dir_all(&work_dir);
    fs::create_dir_all(&work_dir)
        .with_context(|| format!("could not create {}", work_dir.display()))?;
    let outcome = apply_update(
        &client,
        &work_dir,
        &bin_dir,
        &app_dir,
        platform,
        &latest,
        json_output,
    );
    let _ = fs::remove_dir_all(&work_dir);
    outcome
}

/// Move a verified, extracted payload into the install root and return the
/// installed binary path.
///
/// Split out from the download so the part that can damage an existing install
/// is testable without a network: every write here stages beside its
/// destination and lands with an atomic rename.
fn install_payload(
    extracted: &Path,
    bin_dir: &Path,
    app_dir: &Path,
    version: &str,
    digest: &str,
) -> Result<PathBuf> {
    let binary = extracted.join("heiwa");
    if !binary.is_file() {
        bail!("release archive does not contain the heiwa binary");
    }
    let cockpit_source = extracted.join("cockpit");
    if !cockpit_source.join("index.html").is_file() {
        bail!("release archive does not contain cockpit/index.html");
    }

    // Cockpit first: a new binary serving an old cockpit is exactly the
    // version-skew failure the runtime docs warn about, so the assets land
    // before the binary that serves them.
    let cockpit_name = cockpit_dir_name(version, digest);
    let cockpit_target = app_dir.join(&cockpit_name);
    if !cockpit_target.is_dir() {
        let staged_cockpit = app_dir.join(format!(".cockpit.new.{}", std::process::id()));
        let _ = fs::remove_dir_all(&staged_cockpit);
        copy_dir_all(&cockpit_source, &staged_cockpit)?;
        fs::rename(&staged_cockpit, &cockpit_target).with_context(|| {
            format!(
                "could not install the cockpit at {}",
                cockpit_target.display()
            )
        })?;
    }
    swap_symlink(&app_dir.join("cockpit-current"), &cockpit_name)?;

    let staged_bin = bin_dir.join(format!(".heiwa.new.{}", std::process::id()));
    let _ = fs::remove_file(&staged_bin);
    fs::copy(&binary, &staged_bin)
        .with_context(|| format!("could not stage the new binary at {}", staged_bin.display()))?;
    install_executable(&staged_bin)?;
    let installed_bin = bin_dir.join("heiwa");
    fs::rename(&staged_bin, &installed_bin).with_context(|| {
        format!(
            "could not install the binary at {}",
            installed_bin.display()
        )
    })?;
    Ok(installed_bin)
}

fn apply_update(
    client: &reqwest::blocking::Client,
    work_dir: &Path,
    bin_dir: &Path,
    app_dir: &Path,
    platform: &str,
    version: &str,
    json_output: bool,
) -> Result<()> {
    let archive = archive_name(version, platform);
    let checksums = checksums_name(version);
    let archive_path = work_dir.join(&archive);
    let checksums_path = work_dir.join(&checksums);

    download(client, &asset_url(version, &archive), &archive_path)?;
    download(client, &asset_url(version, &checksums), &checksums_path)?;

    let expected = expected_checksum(
        &fs::read_to_string(&checksums_path).context("could not read the release checksums")?,
        &archive,
    )?;
    let actual = sha256_file(&archive_path)?;
    if actual != expected {
        bail!("checksum mismatch for {archive}: expected {expected}, got {actual}");
    }

    let archive_display = archive_path.display().to_string();
    let root = archive_root(version, platform);
    validate_listing(&tar_output(&["-tzf", &archive_display])?, &root)?;
    validate_entry_types(&tar_output(&["-tvzf", &archive_display])?)?;

    let work_display = work_dir.display().to_string();
    tar_output(&["-xzf", &archive_display, "-C", &work_display])?;

    let extracted = work_dir.join(&root);
    let installed_bin = install_payload(&extracted, bin_dir, app_dir, version, &actual)?;

    if json_output {
        println!(
            "{}",
            json!({
                "command": "app update",
                "result": "updated",
                "version": version,
                "binary": installed_bin.display().to_string(),
                "cockpit": app_dir.join("cockpit-current").display().to_string(),
                "sha256": actual,
                "restart_required": true,
            })
        );
    } else {
        println!("  updated to v{version}");
        println!("  binary: {}", installed_bin.display());
        println!("  sha256: {actual}");
        println!("  restart the runtime to pick it up: heiwa app start --no-open");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expected_checksum_reads_plain_and_binary_entries() {
        let digest = "a".repeat(64);
        let checksums = format!(
            "{digest}  heiwa-0.1.0-linux-x86_64.tar.gz\n{digest} *heiwa-0.1.0-macos-aarch64.tar.gz\n"
        );
        assert_eq!(
            expected_checksum(&checksums, "heiwa-0.1.0-linux-x86_64.tar.gz").unwrap(),
            digest
        );
        assert_eq!(
            expected_checksum(&checksums, "heiwa-0.1.0-macos-aarch64.tar.gz").unwrap(),
            digest
        );
    }

    #[test]
    fn expected_checksum_rejects_missing_and_malformed_entries() {
        let digest = "a".repeat(64);
        let checksums = format!("{digest}  heiwa-0.1.0-linux-x86_64.tar.gz\n");
        assert!(expected_checksum(&checksums, "heiwa-0.1.0-macos-aarch64.tar.gz").is_err());
        assert!(expected_checksum("zz  heiwa.tar.gz", "heiwa.tar.gz").is_err());
        assert!(expected_checksum("", "heiwa.tar.gz").is_err());
    }

    #[test]
    fn validate_listing_accepts_the_expected_root() {
        let listing = "heiwa-0.1.0-linux-x86_64/\nheiwa-0.1.0-linux-x86_64/heiwa\nheiwa-0.1.0-linux-x86_64/cockpit/index.html\n";
        assert!(validate_listing(listing, "heiwa-0.1.0-linux-x86_64").is_ok());
    }

    #[test]
    fn validate_listing_rejects_escapes() {
        let root = "heiwa-0.1.0-linux-x86_64";
        assert!(validate_listing("etc/passwd\n", root).is_err());
        assert!(validate_listing("heiwa-0.1.0-linux-x86_64/../etc/passwd\n", root).is_err());
        assert!(validate_listing("heiwa-0.1.0-linux-x86_64-evil/heiwa\n", root).is_err());
        assert!(validate_listing("", root).is_err());
    }

    #[test]
    fn validate_entry_types_rejects_links() {
        assert!(
            validate_entry_types("-rw-r--r-- 0 0 12 heiwa\ndrwxr-xr-x 0 0 0 cockpit\n").is_ok()
        );
        assert!(validate_entry_types("lrwxrwxrwx 0 0 0 evil -> /etc/passwd\n").is_err());
        assert!(validate_entry_types("hrw-r--r-- 0 0 0 hardlink\n").is_err());
    }

    #[test]
    fn version_shape_is_enforced() {
        assert!(validate_version("0.1.0").is_ok());
        assert!(validate_version("v0.1.0").is_err());
        assert!(validate_version("0.1").is_err());
        assert!(validate_version("0.1.0-rc1").is_err());
        assert!(validate_version("").is_err());
    }

    #[test]
    fn asset_names_match_the_public_installer_layout() {
        assert_eq!(
            archive_name("0.1.0", "macos-aarch64"),
            "heiwa-0.1.0-macos-aarch64.tar.gz"
        );
        assert_eq!(checksums_name("0.1.0"), "heiwa-0.1.0-checksums.txt");
        assert_eq!(
            asset_url("0.1.0", "heiwa-0.1.0-checksums.txt"),
            "https://github.com/Heiwa-Limited/heiwa-universe/releases/download/v0.1.0/heiwa-0.1.0-checksums.txt"
        );
        assert_eq!(
            cockpit_dir_name("0.1.0", &"b".repeat(64)),
            "cockpit-0.1.0-bbbbbbbbbbbb"
        );
    }

    #[test]
    fn dry_run_plan_stays_offline_and_keeps_the_documented_fields() {
        let summary = plan(
            Path::new("/tmp/heiwa"),
            "macos-aarch64",
            "0.1.0",
            None,
            true,
        );
        assert_eq!(summary["source_mode"], "github-release");
        assert_eq!(
            summary["source"],
            "https://github.com/Heiwa-Limited/heiwa-universe/releases"
        );
        assert_eq!(summary["release_api"], LATEST_RELEASE_API);
        assert_eq!(summary["restart_policy"], "prompt-before-restart");
        assert_eq!(summary["dry_run"], true);
        // Unresolved without network, rather than guessed.
        assert!(summary["latest_version"].is_null());
        assert!(summary["update_available"].is_null());
    }

    #[test]
    fn resolved_plan_reports_whether_an_update_is_available() {
        let same = plan(
            Path::new("/tmp/heiwa"),
            "linux-x86_64",
            "0.1.0",
            Some("0.1.0"),
            false,
        );
        assert_eq!(same["update_available"], false);
        let newer = plan(
            Path::new("/tmp/heiwa"),
            "linux-x86_64",
            "0.1.0",
            Some("0.1.1"),
            false,
        );
        assert_eq!(newer["update_available"], true);
        assert_eq!(newer["latest_version"], "0.1.1");
    }

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "heiwa-release-update-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp root");
        dir
    }

    /// Build the shape `tar` would leave behind for a verified release archive.
    fn fake_payload(root: &Path, marker: &str) -> PathBuf {
        let extracted = root.join("extracted");
        fs::create_dir_all(extracted.join("cockpit").join("assets")).expect("create payload");
        fs::write(extracted.join("heiwa"), marker.as_bytes()).expect("write binary");
        fs::write(
            extracted.join("cockpit").join("index.html"),
            b"<!doctype html>",
        )
        .expect("write index");
        extracted
    }

    // Depends on the symlinked cockpit layout, which is Unix-only.
    #[cfg(unix)]
    #[test]
    fn install_payload_lands_binary_cockpit_and_symlink() {
        let root = temp_root("install");
        let extracted = fake_payload(&root, "new-binary");
        let bin_dir = root.join("bin");
        let app_dir = root.join("app");
        fs::create_dir_all(&bin_dir).unwrap();
        fs::create_dir_all(&app_dir).unwrap();
        // An older install already in place: the update must replace it.
        fs::write(bin_dir.join("heiwa"), b"old-binary").unwrap();

        let digest = "c".repeat(64);
        let installed =
            install_payload(&extracted, &bin_dir, &app_dir, "0.1.1", &digest).expect("install");

        assert_eq!(installed, bin_dir.join("heiwa"));
        assert_eq!(fs::read(&installed).unwrap(), b"new-binary");
        let cockpit = app_dir.join("cockpit-0.1.1-cccccccccccc");
        assert!(cockpit.join("index.html").is_file());
        let link = app_dir.join("cockpit-current");
        assert!(link.is_symlink());
        assert_eq!(
            fs::read_link(&link).unwrap(),
            Path::new("cockpit-0.1.1-cccccccccccc")
        );
        // No staging leftovers.
        let strays: Vec<_> = fs::read_dir(&bin_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with('.'))
            .collect();
        assert!(strays.is_empty(), "staging file left behind");

        let _ = fs::remove_dir_all(&root);
    }

    // Depends on the symlinked cockpit layout, which is Unix-only.
    #[cfg(unix)]
    #[test]
    fn install_payload_is_idempotent_for_the_same_digest() {
        let root = temp_root("idempotent");
        let extracted = fake_payload(&root, "same");
        let bin_dir = root.join("bin");
        let app_dir = root.join("app");
        fs::create_dir_all(&bin_dir).unwrap();
        fs::create_dir_all(&app_dir).unwrap();
        let digest = "d".repeat(64);

        install_payload(&extracted, &bin_dir, &app_dir, "0.1.1", &digest).expect("first install");
        install_payload(&extracted, &bin_dir, &app_dir, "0.1.1", &digest).expect("second install");

        assert!(app_dir.join("cockpit-0.1.1-dddddddddddd").is_dir());
        assert!(app_dir.join("cockpit-current").is_symlink());

        let _ = fs::remove_dir_all(&root);
    }

    // Depends on the symlinked cockpit layout, which is Unix-only.
    #[cfg(unix)]
    #[test]
    fn install_payload_refuses_an_unmanaged_cockpit_link() {
        let root = temp_root("unmanaged");
        let extracted = fake_payload(&root, "bin");
        let bin_dir = root.join("bin");
        let app_dir = root.join("app");
        fs::create_dir_all(&bin_dir).unwrap();
        // A real directory where the managed symlink belongs must not be clobbered.
        fs::create_dir_all(app_dir.join("cockpit-current")).unwrap();

        let err = install_payload(&extracted, &bin_dir, &app_dir, "0.1.1", &"e".repeat(64))
            .expect_err("must refuse an unmanaged cockpit-current");
        assert!(
            err.to_string().contains("not a managed symlink"),
            "unexpected error: {err}"
        );
        assert!(app_dir.join("cockpit-current").is_dir());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn install_payload_rejects_incomplete_archives() {
        let root = temp_root("incomplete");
        let bin_dir = root.join("bin");
        let app_dir = root.join("app");
        fs::create_dir_all(&bin_dir).unwrap();
        fs::create_dir_all(&app_dir).unwrap();
        let digest = "f".repeat(64);

        let no_binary = root.join("no-binary");
        fs::create_dir_all(no_binary.join("cockpit")).unwrap();
        fs::write(no_binary.join("cockpit").join("index.html"), b"x").unwrap();
        assert!(install_payload(&no_binary, &bin_dir, &app_dir, "0.1.1", &digest).is_err());

        let no_cockpit = root.join("no-cockpit");
        fs::create_dir_all(&no_cockpit).unwrap();
        fs::write(no_cockpit.join("heiwa"), b"x").unwrap();
        assert!(install_payload(&no_cockpit, &bin_dir, &app_dir, "0.1.1", &digest).is_err());

        // A rejected payload must not have touched the install root.
        assert!(!bin_dir.join("heiwa").exists());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn windows_is_refused_rather_than_half_supported() {
        assert!(supported_platform("macos-aarch64"));
        assert!(supported_platform("linux-x86_64"));
        assert!(!supported_platform("windows-x86_64"));
        assert!(!supported_platform("unsupported"));
    }
}
