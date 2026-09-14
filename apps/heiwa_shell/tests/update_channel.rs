//! The update channel end to end: choosing `dev` records the source checkout,
//! `heiwa app update` builds origin/dev in its own worktree once per commit and
//! installs what the checkout's build script produced, and returning to `main`
//! reinstalls a release even when the version number matches.
#![cfg(unix)]

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Stands in for `scripts/build_local_bundle.sh`: records each build, checks it
/// was handed a clean checkout of the commit it names, and emits a runtime and
/// cockpit that say which commit they came from.
const RECORDING_BUILD: &str = r#"#!/usr/bin/env bash
set -euo pipefail
out="$1"
echo "$HEIWA_BUILD_COMMIT" >> "$HEIWA_TEST_BUILDS"
test "$HEIWA_BUILD_CHANNEL" = dev
test "$(git rev-parse HEAD)" = "$HEIWA_BUILD_COMMIT"
test -z "$(git status --porcelain --untracked-files=no)"
test ! -e left-by-previous-build
mkdir -p "$out/cockpit"
printf '<!doctype html><title>%s</title>\n' "$HEIWA_BUILD_COMMIT" > "$out/cockpit/index.html"
printf '#!/bin/sh\necho "heiwa 9.9.9 (dev %s)"\n' "$HEIWA_BUILD_COMMIT" > "$out/heiwa"
chmod +x "$out/heiwa"
echo stray > left-by-previous-build
"#;

const FAILING_BUILD: &str = r#"#!/usr/bin/env bash
mkdir -p "$1/cockpit"
echo "error: this dev commit does not build" >&2
exit 3
"#;

/// A heiwa-universe origin with a dev branch, the operator's clone of it, and an
/// isolated home for the runtime under test.
struct Fixture {
    root: tempfile::TempDir,
}

impl Fixture {
    fn new(build_script: &str) -> Self {
        let fixture = Self {
            root: tempfile::tempdir().expect("create fixture root"),
        };
        for dir in [fixture.home(), fixture.outside()] {
            fs::create_dir_all(dir).expect("create fixture dir");
        }
        let origin = fixture.origin();
        let checkout = fixture.checkout();
        git(
            fixture.path(),
            &["init", "--quiet", "--bare", path_str(&origin)],
        );
        git(
            fixture.path(),
            &["clone", "--quiet", path_str(&origin), path_str(&checkout)],
        );
        fs::write(checkout.join("HEIWA.md"), "# Heiwa\n").unwrap();
        fs::create_dir_all(checkout.join("apps/heiwa_shell")).unwrap();
        fs::write(
            checkout.join("apps/heiwa_shell/Cargo.toml"),
            "[package]\nname = \"heiwa-shell\"\n",
        )
        .unwrap();
        fs::create_dir_all(checkout.join("scripts")).unwrap();
        fs::write(checkout.join("scripts/build_local_bundle.sh"), build_script).unwrap();
        git(&checkout, &["add", "-A"]);
        git(&checkout, &["commit", "--quiet", "-m", "first dev commit"]);
        git(
            &checkout,
            &["push", "--quiet", "origin", "HEAD:refs/heads/dev"],
        );
        fixture
    }

    fn path(&self) -> &Path {
        self.root.path()
    }

    fn origin(&self) -> PathBuf {
        self.path().join("origin.git")
    }

    fn checkout(&self) -> PathBuf {
        self.path().join("heiwa-universe")
    }

    fn home(&self) -> PathBuf {
        self.path().join("home")
    }

    fn outside(&self) -> PathBuf {
        self.path().join("outside")
    }

    fn heiwa_root(&self) -> PathBuf {
        self.home().join(".heiwa")
    }

    fn builds(&self) -> usize {
        fs::read_to_string(self.path().join("builds"))
            .map(|log| log.lines().count())
            .unwrap_or(0)
    }

    /// Land a commit on origin's dev from another clone, so only a fetch can
    /// show it to the operator's checkout.
    fn push_upstream_commit(&self, message: &str) -> String {
        let upstream = self.path().join("upstream");
        if !upstream.exists() {
            git(
                self.path(),
                &[
                    "clone",
                    "--quiet",
                    "--branch",
                    "dev",
                    path_str(&self.origin()),
                    path_str(&upstream),
                ],
            );
        }
        fs::write(upstream.join("CHANGE.md"), message).unwrap();
        git(&upstream, &["add", "-A"]);
        git(&upstream, &["commit", "--quiet", "-m", message]);
        git(
            &upstream,
            &["push", "--quiet", "origin", "HEAD:refs/heads/dev"],
        );
        git(&upstream, &["rev-parse", "HEAD"])
    }

    fn heiwa(&self, cwd: &Path, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_heiwa"))
            .args(args)
            .current_dir(cwd)
            .env_clear()
            .env("HOME", self.home())
            .env("USER", "heiwa-test")
            .env("TMPDIR", self.path())
            .env("PATH", "/usr/bin:/bin")
            .env("LC_ALL", "C")
            .env("HEIWA_DISABLE_KEYCHAIN", "1")
            .env("HEIWA_OLLAMA_BASE", "disabled-for-hermetic-tests")
            .env("HEIWA_TEST_BUILDS", self.path().join("builds"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            // Never discover a repository above the fixture, such as the
            // checkout running these tests.
            .env("GIT_CEILING_DIRECTORIES", self.path())
            .output()
            .expect("run heiwa")
    }

    fn heiwa_json(&self, cwd: &Path, args: &[&str]) -> Value {
        let output = self.heiwa(cwd, args);
        assert!(
            output.status.success(),
            "heiwa {args:?} failed\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
            panic!(
                "heiwa {args:?} printed non-JSON ({error}): {}",
                String::from_utf8_lossy(&output.stdout)
            )
        })
    }

    fn channel_state(&self) -> Value {
        serde_json::from_slice(&fs::read(self.heiwa_root().join("channel.json")).unwrap())
            .expect("channel.json is JSON")
    }
}

fn path_str(path: &Path) -> &str {
    path.to_str().expect("fixture paths are UTF-8")
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=Heiwa Test",
            "-c",
            "user.email=test@heiwa.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[test]
fn choosing_dev_records_the_primary_checkout_and_installs_nothing() {
    let fx = Fixture::new(RECORDING_BUILD);
    let status = fx.heiwa_json(&fx.outside(), &["app", "channel", "--json"]);
    assert_eq!(status["channel"], "main");
    assert_eq!(status["follows"], "GitHub Releases (tagged on main)");
    assert!(status["installed"].is_null());

    // Chosen from a short-lived agent worktree, the channel records the
    // primary checkout, which outlives that worktree.
    let agent = fx.path().join("agent-worktree");
    git(
        &fx.checkout(),
        &["worktree", "add", "--quiet", "--detach", path_str(&agent)],
    );
    let status = fx.heiwa_json(&agent, &["app", "channel", "dev", "--json"]);
    assert_eq!(status["channel"], "dev");
    let recorded = PathBuf::from(status["source_repo"].as_str().expect("source repo"));
    assert_eq!(
        fs::canonicalize(recorded).unwrap(),
        fs::canonicalize(fx.checkout()).unwrap()
    );
    assert!(
        !fx.heiwa_root().join("bin").join("heiwa").exists(),
        "choosing a channel installs nothing"
    );
    assert_eq!(fx.builds(), 0);

    // Leaving dev keeps the source for the next time it is chosen, even from
    // outside any checkout.
    let status = fx.heiwa_json(&fx.outside(), &["app", "channel", "main", "--json"]);
    assert_eq!(status["channel"], "main");
    let status = fx.heiwa_json(&fx.outside(), &["app", "channel", "dev", "--json"]);
    assert_eq!(status["channel"], "dev");
    assert_eq!(fx.channel_state()["channel"], "dev");

    let refused = fx.heiwa(
        &fx.outside(),
        &["app", "channel", "dev", "--source", path_str(&fx.outside())],
    );
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("not inside a heiwa-universe checkout"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert!(!fx
        .heiwa(&fx.outside(), &["app", "channel", "stable"])
        .status
        .success());
    assert_eq!(fx.channel_state()["channel"], "dev");
}

#[test]
fn dev_updates_build_origin_dev_once_per_commit_and_main_reinstalls_after() {
    let fx = Fixture::new(RECORDING_BUILD);
    let checkout = fx.checkout();
    fx.heiwa_json(
        &fx.outside(),
        &[
            "app",
            "channel",
            "dev",
            "--source",
            path_str(&checkout),
            "--json",
        ],
    );
    let first = git(&checkout, &["rev-parse", "refs/remotes/origin/dev"]);

    // A newer commit lands upstream. A dry run stays offline and plans
    // against the origin/dev already fetched.
    let second = fx.push_upstream_commit("second dev commit");
    let plan = fx.heiwa_json(&fx.outside(), &["app", "update", "--dry-run", "--json"]);
    assert_eq!(plan["source_mode"], "channel-dev");
    assert_eq!(plan["result"], "planned");
    assert_eq!(plan["fetched"], false);
    assert_eq!(plan["target_commit"], first.as_str());
    assert_eq!(fx.builds(), 0);

    let updated = fx.heiwa_json(&fx.outside(), &["app", "update", "--json"]);
    assert_eq!(updated["result"], "updated", "{updated}");
    assert_eq!(updated["target_commit"], second.as_str());
    assert_eq!(fx.builds(), 1);

    let root = fx.heiwa_root();
    let version = Command::new(root.join("bin").join("heiwa"))
        .arg("--version")
        .output()
        .expect("run the installed runtime");
    assert_eq!(
        String::from_utf8_lossy(&version.stdout).trim(),
        format!("heiwa 9.9.9 (dev {second})")
    );
    assert_eq!(
        fs::read_link(root.join("app").join("cockpit-current")).unwrap(),
        PathBuf::from(format!("cockpit-dev-{}", &second[..12]))
    );
    assert!(
        fs::read_to_string(root.join("app").join("cockpit-current").join("index.html"))
            .unwrap()
            .contains(&second)
    );

    let state = fx.channel_state();
    assert_eq!(state["installed"]["source"], "dev");
    assert_eq!(state["installed"]["commit"], second.as_str());
    let receipt: Value = serde_json::from_slice(
        &fs::read(
            state["installed"]["receipt"]
                .as_str()
                .expect("receipt path"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(receipt["event"], "heiwa.app.update.channel");
    assert_eq!(receipt["source"]["commit"], second.as_str());
    assert_eq!(
        receipt["installed"]["binary_sha256"].as_str().map(str::len),
        Some(64)
    );

    // The operator's checkout was fetched into, never checked out or built in.
    assert_eq!(git(&checkout, &["rev-parse", "HEAD"]), first);
    assert_eq!(git(&checkout, &["status", "--porcelain"]), "");
    assert_eq!(
        git(&checkout, &["rev-parse", "refs/remotes/origin/dev"]),
        second
    );

    let again = fx.heiwa_json(&fx.outside(), &["app", "update", "--json"]);
    assert_eq!(again["result"], "up_to_date");
    assert_eq!(fx.builds(), 1);

    // --force rebuilds the same commit, from a tree cleaned of the last build.
    let forced = fx.heiwa_json(&fx.outside(), &["app", "update", "--force", "--json"]);
    assert_eq!(forced["result"], "updated", "{forced}");
    assert_eq!(fx.builds(), 2);

    // Back on main, the release is reinstalled even at an equal version,
    // because the install root now holds a dev build.
    fx.heiwa_json(&fx.outside(), &["app", "channel", "main", "--json"]);
    let release = fx.heiwa_json(&fx.outside(), &["app", "update", "--dry-run", "--json"]);
    assert_eq!(release["source_mode"], "github-release");
    assert_eq!(release["channel"], "main");
    assert_eq!(release["reinstall"], true);
}

#[test]
fn a_failed_dev_build_leaves_the_install_untouched() {
    let fx = Fixture::new(FAILING_BUILD);
    fx.heiwa_json(
        &fx.outside(),
        &[
            "app",
            "channel",
            "dev",
            "--source",
            path_str(&fx.checkout()),
            "--json",
        ],
    );

    let output = fx.heiwa(&fx.outside(), &["app", "update", "--json"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("scripts/build_local_bundle.sh failed"),
        "{stderr}"
    );

    let root = fx.heiwa_root();
    let logs: Vec<PathBuf> = fs::read_dir(root.join("logs"))
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("app-update-dev-"))
        })
        .collect();
    assert_eq!(logs.len(), 1);
    assert!(fs::read_to_string(&logs[0])
        .unwrap()
        .contains("this dev commit does not build"));

    assert!(!root.join("bin").join("heiwa").exists());
    assert!(fs::symlink_metadata(root.join("app").join("cockpit-current")).is_err());
    assert!(fx.channel_state()["installed"].is_null());
    let leftovers = fs::read_dir(root.join("cache"))
        .map(|entries| entries.count())
        .unwrap_or(0);
    assert_eq!(leftovers, 0, "the partial build output is removed");
}
