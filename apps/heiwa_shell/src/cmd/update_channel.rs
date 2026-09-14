//! `heiwa app channel` and the dev half of `heiwa app update`.
//!
//! A dev update is the release pipeline run locally against `origin/dev`:
//! fetch, check out that exact commit in a Heiwa-managed worktree, let the
//! checkout build itself with `scripts/build_local_bundle.sh`, then install
//! what it produced with the release updater's invariants — cockpit before the
//! binary that serves it, every write staged and renamed, no restart. The
//! operator's own working tree is never built, checked out, or cleaned.

use anyhow::{anyhow, bail, Context, Result};
use heiwa_install::update_channel::{self, Channel, ChannelState, InstalledBuild};
use serde_json::{json, Value};
use std::env;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

const DEV_REF: &str = "refs/remotes/origin/dev";
const DEV_REFSPEC: &str = "+refs/heads/dev:refs/remotes/origin/dev";
const BUILD_SCRIPT: &str = "scripts/build_local_bundle.sh";
const BUILD_WORKTREE: &str = "heiwa-universe-dev";

pub(crate) fn run(args: &[String]) -> Result<()> {
    let mut action: Option<&str> = None;
    let mut source: Option<PathBuf> = None;
    let mut json_output = false;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => json_output = true,
            "--source" => {
                let path = args
                    .next()
                    .ok_or_else(|| anyhow!("--source needs a checkout path"))?;
                source = Some(PathBuf::from(path));
            }
            "-h" | "--help" => {
                print_help();
                return Ok(());
            }
            value if action.is_none() && !value.starts_with('-') => action = Some(value),
            other => bail!("unexpected app channel argument: {other}"),
        }
    }

    let root = heiwa_install::get_heiwa_dir();
    match action {
        None | Some("show") if source.is_none() => {
            print_status(&update_channel::load(&root)?, &root, json_output);
            Ok(())
        }
        None | Some("show") => {
            bail!("choose the dev source with `heiwa app channel dev --source <checkout>`")
        }
        Some(name) => {
            let channel = Channel::parse(name)
                .ok_or_else(|| anyhow!("unknown channel {name}; choose main or dev"))?;
            select(&root, channel, source, json_output)
        }
    }
}

fn print_help() {
    println!("heiwa app channel");
    println!();
    println!("Usage:");
    println!("  heiwa app channel [show] [--json]");
    println!("  heiwa app channel main");
    println!("  heiwa app channel dev [--source <checkout>]");
    println!();
    println!("main follows GitHub Releases, which are tagged on main. It is the default.");
    println!("dev builds origin/dev from a heiwa-universe checkout in a Heiwa-managed worktree.");
    println!("Choosing a channel installs nothing; `heiwa app update` follows it.");
}

fn select(root: &Path, channel: Channel, source: Option<PathBuf>, json_output: bool) -> Result<()> {
    let mut state = update_channel::load(root)?;
    match channel {
        Channel::Main if source.is_some() => bail!("--source applies only to the dev channel"),
        Channel::Main => {}
        Channel::Dev => {
            // An explicit path wins, then the checkout this runs in, then the
            // checkout recorded last time.
            let candidate = source
                .or_else(|| {
                    env::current_dir()
                        .ok()
                        .filter(|dir| checkout_toplevel(dir).is_some())
                })
                .or_else(|| state.source_repo.clone())
                .ok_or_else(|| {
                    anyhow!(
                        "the dev channel builds origin/dev from a heiwa-universe checkout; run this inside one or pass --source <checkout>"
                    )
                })?;
            state.source_repo = Some(source_repository(&candidate)?);
        }
    }
    state.channel = channel;
    update_channel::save(root, &state)?;
    print_status(&state, root, json_output);
    Ok(())
}

/// The top of the heiwa-universe checkout containing `path`.
fn checkout_toplevel(path: &Path) -> Option<PathBuf> {
    let top = PathBuf::from(git(path, &["rev-parse", "--show-toplevel"]).ok()?);
    let is_heiwa = top.join("HEIWA.md").is_file()
        && top
            .join("apps")
            .join("heiwa_shell")
            .join("Cargo.toml")
            .is_file();
    is_heiwa.then_some(top)
}

/// The repository that owns the checkout at `path`, recorded as its primary
/// working tree, so a channel chosen from a short-lived agent worktree keeps
/// working after that worktree is removed.
fn source_repository(path: &Path) -> Result<PathBuf> {
    let top = checkout_toplevel(path)
        .ok_or_else(|| anyhow!("{} is not inside a heiwa-universe checkout", path.display()))?;
    let common = git_common_dir(&top)?;
    let repository = match (common.file_name(), common.parent()) {
        (Some(name), Some(parent)) if name == ".git" => parent.to_path_buf(),
        _ => top,
    };
    git(&repository, &["remote", "get-url", "origin"]).with_context(|| {
        format!(
            "{} has no origin remote to fetch dev from",
            repository.display()
        )
    })?;
    Ok(repository)
}

fn follows(state: &ChannelState) -> String {
    match (state.channel, &state.source_repo) {
        (Channel::Main, _) => "GitHub Releases (tagged on main)".to_string(),
        (Channel::Dev, Some(repository)) => {
            format!("origin/dev, built from {}", repository.display())
        }
        (Channel::Dev, None) => "origin/dev (no source checkout recorded)".to_string(),
    }
}

fn describe(build: &InstalledBuild) -> String {
    match &build.commit {
        Some(commit) => format!(
            "{} {} ({}, {})",
            build.source,
            short(commit),
            build.version,
            build.installed_at
        ),
        None => format!(
            "{} ({}, {})",
            build.source, build.version, build.installed_at
        ),
    }
}

fn print_status(state: &ChannelState, root: &Path, json_output: bool) {
    if json_output {
        println!(
            "{}",
            json!({
                "command": "app channel",
                "channel": state.channel.as_str(),
                "follows": follows(state),
                "source_repo": state.source_repo,
                "installed": state.installed,
                "state_path": update_channel::state_path(root),
                "update_command": "heiwa app update",
            })
        );
        return;
    }
    println!("heiwa app channel");
    println!("  channel: {}", state.channel.as_str());
    println!("  follows: {}", follows(state));
    match &state.installed {
        Some(build) => println!("  installed: {}", describe(build)),
        None => println!("  installed: not recorded by a channel update yet"),
    }
    println!("  update: heiwa app update");
}

pub(crate) struct DevUpdate {
    pub dry_run: bool,
    pub json_output: bool,
    pub force: bool,
}

pub(crate) fn update_dev(root: &Path, options: DevUpdate) -> Result<()> {
    if !cfg!(unix) {
        bail!("the dev channel builds with bash and supports macOS and Linux only");
    }
    let state = update_channel::load(root)?;
    let source = state.source_repo.clone().ok_or_else(|| {
        anyhow!("the dev channel has no source checkout; run `heiwa app channel dev --source <checkout>`")
    })?;
    // Like the release updater, a dry run stays offline: it plans against the
    // origin/dev that was last fetched and says so.
    if !options.dry_run {
        git_fetch_dev(&source)?;
    }
    let commit = git(
        &source,
        &["rev-parse", "--verify", &format!("{DEV_REF}^{{commit}}")],
    )
    .map_err(|_| {
        anyhow!(
            "{} has no origin/dev yet; run `heiwa app update` to fetch it",
            source.display()
        )
    })?;
    let up_to_date = state
        .installed
        .as_ref()
        .is_some_and(|build| build.source == "dev" && build.commit.as_deref() == Some(&commit));
    let worktree = root.join("build").join(BUILD_WORKTREE);
    let target_dir = cargo_target_dir(&source)?;
    let mut report = json!({
        "command": "app update",
        "channel": "dev",
        "source_mode": "channel-dev",
        "source": source,
        "remote_ref": "origin/dev",
        "fetched": !options.dry_run,
        "target_commit": commit,
        "installed_before": state.installed,
        "up_to_date": up_to_date,
        "build_worktree": worktree,
        "build_script": BUILD_SCRIPT,
        "cargo_target_dir": target_dir,
        "installed_bin": root.join("bin").join("heiwa"),
        "installed_app": root.join("app").join("Heiwa.app"),
        "restart_policy": "prompt-before-restart",
        "dry_run": options.dry_run,
    });

    if !options.json_output {
        println!("heiwa app update");
        println!("  channel: dev");
        println!("  source: origin/dev in {}", source.display());
        println!("  target_commit: {}", short(&commit));
        match &state.installed {
            Some(build) => println!("  installed: {}", describe(build)),
            None => println!("  installed: not recorded by a channel update yet"),
        }
        println!("  build_worktree: {}", worktree.display());
        println!("  restart_policy: prompt-before-restart");
    }
    if options.dry_run {
        report["result"] = json!("planned");
        if options.json_output {
            println!("{report}");
        } else {
            println!("  fetched: no (a dry run plans against the cached origin/dev)");
            println!("  dry_run: true");
        }
        return Ok(());
    }
    if up_to_date && !options.force {
        report["result"] = json!("up_to_date");
        if options.json_output {
            println!("{report}");
        } else {
            println!("  already at origin/dev {}", short(&commit));
        }
        return Ok(());
    }

    prepare_worktree(&source, &worktree, &commit)?;
    if !worktree.join(BUILD_SCRIPT).is_file() {
        bail!(
            "origin/dev {} has no {BUILD_SCRIPT}; it predates dev-channel builds",
            short(&commit)
        );
    }
    let output = root
        .join("cache")
        .join(format!("dev-build-{}", std::process::id()));
    let _ = fs::remove_dir_all(&output);
    fs::create_dir_all(&output).with_context(|| format!("create {}", output.display()))?;
    let build = Build {
        root,
        source: &source,
        commit: &commit,
        worktree: &worktree,
        target_dir: &target_dir,
        output: &output,
        installed_before: state.installed.as_ref(),
        json_output: options.json_output,
    };
    let outcome = build.run_and_install(&mut report);
    let _ = fs::remove_dir_all(&output);
    outcome?;
    if options.json_output {
        println!("{report}");
    }
    Ok(())
}

struct Build<'a> {
    root: &'a Path,
    source: &'a Path,
    commit: &'a str,
    worktree: &'a Path,
    target_dir: &'a Path,
    output: &'a Path,
    installed_before: Option<&'a InstalledBuild>,
    json_output: bool,
}

impl Build<'_> {
    fn run_and_install(&self, report: &mut Value) -> Result<()> {
        let started = Instant::now();
        let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        // Machine-readable output stays one JSON document, so build logs go
        // to a file instead of stdout.
        let log_path = self.json_output.then(|| {
            self.root
                .join("logs")
                .join(format!("app-update-dev-{stamp}.log"))
        });
        let mut command = Command::new("bash");
        command
            .arg(BUILD_SCRIPT)
            .arg(self.output)
            .current_dir(self.worktree)
            .env("HEIWA_BUILD_CHANNEL", "dev")
            .env("HEIWA_BUILD_COMMIT", self.commit)
            .env("CARGO_TARGET_DIR", self.target_dir)
            .stdin(Stdio::null());
        if let Some(log_path) = &log_path {
            if let Some(parent) = log_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let log =
                File::create(log_path).with_context(|| format!("create {}", log_path.display()))?;
            command.stdout(log.try_clone()?).stderr(log);
        } else {
            println!("  building: bash {BUILD_SCRIPT} (the first build takes a while)");
        }
        let status = command.status().context("run the dev build")?;
        if !status.success() {
            let hint = log_path
                .as_ref()
                .map(|path| format!("; see {}", path.display()))
                .unwrap_or_default();
            bail!("{BUILD_SCRIPT} failed with {status}{hint}");
        }

        let runtime = self.output.join("heiwa");
        let cockpit = self.output.join("cockpit");
        let bundle = self.output.join("Heiwa.app");
        if fs::metadata(&runtime).map_or(true, |meta| !meta.is_file() || meta.len() == 0) {
            bail!("the dev build did not produce a heiwa binary");
        }
        if !cockpit.join("index.html").is_file() {
            bail!("the dev build did not produce cockpit/index.html");
        }
        // Bytes that cannot identify themselves never replace a working install.
        let version = binary_version(&runtime)?;
        let desktop = bundle
            .join("Contents")
            .join("MacOS")
            .join("Heiwa")
            .is_file();
        let apple_helper = self.output.join("heiwa-apple-resources").is_file();

        // Cockpit first, as the release updater does: a new binary serving an
        // old cockpit is the version-skew failure.
        let cockpit_name = format!("cockpit-dev-{}", short(self.commit));
        install_cockpit(self.root, &cockpit, &cockpit_name)?;
        let installed_bin = heiwa_install::install_runtime_binary(self.root, &runtime)?;
        if desktop {
            heiwa_install::install_desktop_app_bundle(self.root, &bundle)?;
        }

        let installed_at = chrono::Utc::now().to_rfc3339();
        let receipt_id = format!("heiwa-app-update-dev-{stamp}");
        let receipt_path = crate::home::heiwa_state_dir()
            .join("evidence")
            .join("promotion")
            .join(format!("{receipt_id}.json"));
        let installed = json!({
            "version": version,
            "binary": installed_bin,
            "binary_sha256": super::release_update::sha256_file(&installed_bin)?,
            "apple_helper": apple_helper,
            "cockpit": self.root.join("app").join(&cockpit_name),
            "desktop_bundle": desktop,
        });
        let receipt = json!({
            "schema_version": "heiwa_promotion_receipt_v1",
            "receipt_id": receipt_id,
            "event": "heiwa.app.update.channel",
            "plane": "evidence",
            "created_at": installed_at,
            "channel": "dev",
            "source": {
                "kind": "channel-dev",
                "repository": self.source,
                "ref": "origin/dev",
                "commit": self.commit,
                "worktree": self.worktree,
            },
            "build": {
                "script": BUILD_SCRIPT,
                "cargo_target_dir": self.target_dir,
                "log": log_path,
                "seconds": started.elapsed().as_secs(),
            },
            "installed": installed,
            "installed_before": self.installed_before,
            "restart_policy": "prompt-before-restart",
        });
        if let Some(parent) = receipt_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&receipt_path, serde_json::to_vec_pretty(&receipt)?)
            .with_context(|| format!("write {}", receipt_path.display()))?;

        let mut state = update_channel::load(self.root)?;
        state.installed = Some(InstalledBuild {
            source: "dev".to_string(),
            version: version.clone(),
            commit: Some(self.commit.to_string()),
            installed_at,
            receipt: Some(receipt_path.clone()),
        });
        update_channel::save(self.root, &state)?;

        report["result"] = json!("updated");
        report["installed"] = receipt["installed"].clone();
        report["receipt"] = json!(receipt_path);
        if !self.json_output {
            println!("  status: updated to origin/dev {}", short(self.commit));
            println!("  binary: {} ({version})", installed_bin.display());
            println!("  cockpit: {cockpit_name}");
            println!(
                "  app_bundle: {}",
                if desktop {
                    "updated"
                } else {
                    "not built on this platform"
                }
            );
            println!("  receipt: {}", receipt_path.display());
            println!("  restart the runtime to pick it up: heiwa app start --no-open");
        }
        Ok(())
    }
}

/// Check out exactly `commit` in the Heiwa-managed build worktree. Tracked
/// changes and untracked files a previous build left behind are discarded;
/// ignored caches such as `node_modules` survive, which keeps later builds
/// incremental.
fn prepare_worktree(source: &Path, worktree: &Path, commit: &str) -> Result<()> {
    let attached = worktree.join(".git").is_file()
        && git_common_dir(worktree).ok() == Some(git_common_dir(source)?);
    if attached {
        git(
            worktree,
            &["checkout", "--quiet", "--detach", "--force", commit],
        )?;
        git(worktree, &["clean", "-ffdq"])?;
    } else {
        if worktree.exists() {
            // Heiwa owns this directory. A tree from another repository or an
            // interrupted add is rebuilt rather than trusted.
            fs::remove_dir_all(worktree)
                .with_context(|| format!("remove stale build tree {}", worktree.display()))?;
        }
        if let Some(parent) = worktree.parent() {
            fs::create_dir_all(parent)?;
        }
        let _ = git(source, &["worktree", "prune"]);
        let path = worktree.display().to_string();
        git(
            source,
            &[
                "worktree", "add", "--quiet", "--detach", "--force", &path, commit,
            ],
        )?;
    }
    let head = git(worktree, &["rev-parse", "HEAD"])?;
    if head != commit {
        bail!("the build worktree is at {head}, expected {commit}");
    }
    let changes = git(worktree, &["status", "--porcelain", "--untracked-files=no"])?;
    if !changes.is_empty() {
        bail!("the build worktree still has tracked changes:\n{changes}");
    }
    Ok(())
}

fn install_cockpit(root: &Path, built: &Path, name: &str) -> Result<()> {
    let app_dir = root.join("app");
    fs::create_dir_all(&app_dir).with_context(|| format!("create {}", app_dir.display()))?;
    let target = app_dir.join(name);
    // Named by commit, so an existing complete copy is the same assets.
    if !target.join("index.html").is_file() {
        let staged = app_dir.join(format!(".cockpit.new.{}", std::process::id()));
        let _ = fs::remove_dir_all(&staged);
        super::release_update::copy_dir_all(built, &staged)?;
        if target.exists() {
            fs::remove_dir_all(&target)?;
        }
        fs::rename(&staged, &target)
            .with_context(|| format!("install the cockpit at {}", target.display()))?;
    }
    super::release_update::swap_symlink(&app_dir.join("cockpit-current"), name)
}

/// Builds reuse the source checkout's Cargo cache unless the operator chose
/// another: registry dependencies compile once for both, while the checkout's
/// own crates build separately because their paths differ.
fn cargo_target_dir(source: &Path) -> Result<PathBuf> {
    Ok(cargo_target_dir_from(
        source,
        env::var_os("CARGO_TARGET_DIR"),
        &env::current_dir()?,
    ))
}

fn cargo_target_dir_from(
    source: &Path,
    operator: Option<std::ffi::OsString>,
    cwd: &Path,
) -> PathBuf {
    match operator
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        Some(dir) if dir.is_absolute() => dir,
        Some(dir) => cwd.join(dir),
        None => source.join("target"),
    }
}

fn binary_version(binary: &Path) -> Result<String> {
    let output = Command::new(binary)
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("run {} --version", binary.display()))?;
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !output.status.success() || version.is_empty() {
        bail!(
            "the built runtime failed `--version`: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(version)
}

fn git_fetch_dev(source: &Path) -> Result<()> {
    let output = Command::new("git")
        .arg("-C")
        .arg(source)
        .args(["fetch", "--quiet", "origin", DEV_REFSPEC])
        // A fetch that needs credentials fails instead of waiting on a prompt
        // nobody will answer.
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .output()
        .context("run git fetch; the dev channel needs git on PATH")?;
    if !output.status.success() {
        bail!(
            "could not fetch origin/dev into {}: {}",
            source.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

fn git_common_dir(dir: &Path) -> Result<PathBuf> {
    let common = git(
        dir,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    // Compare real paths: temp and home directories are often reached through
    // symlinks that git reports differently from different working trees.
    fs::canonicalize(&common).with_context(|| format!("resolve {common}"))
}

fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .context("run git; the dev channel needs git on PATH")?;
    if !output.status.success() {
        bail!(
            "git {} failed in {}: {}",
            args.join(" "),
            dir.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn short(commit: &str) -> &str {
    &commit[..commit.len().min(12)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_commits_are_twelve_characters_or_the_whole_value() {
        assert_eq!(short("0123456789abcdef0123"), "0123456789ab");
        assert_eq!(short("abc"), "abc");
    }

    #[test]
    fn builds_share_the_source_cache_unless_the_operator_chose_another() {
        let source = Path::new("/src/heiwa-universe");
        let cwd = Path::new("/work");
        assert_eq!(
            cargo_target_dir_from(source, None, cwd),
            source.join("target")
        );
        assert_eq!(
            cargo_target_dir_from(source, Some("".into()), cwd),
            source.join("target")
        );
        assert_eq!(
            cargo_target_dir_from(source, Some("/cache/heiwa".into()), cwd),
            PathBuf::from("/cache/heiwa")
        );
        assert_eq!(
            cargo_target_dir_from(source, Some("relative".into()), cwd),
            PathBuf::from("/work/relative")
        );
    }

    #[test]
    fn follows_names_the_release_authority_or_the_source_checkout() {
        let mut state = ChannelState::default();
        assert_eq!(follows(&state), "GitHub Releases (tagged on main)");
        state.channel = Channel::Dev;
        assert_eq!(follows(&state), "origin/dev (no source checkout recorded)");
        state.source_repo = Some(PathBuf::from("/src/heiwa-universe"));
        assert_eq!(
            follows(&state),
            "origin/dev, built from /src/heiwa-universe"
        );
    }
}
