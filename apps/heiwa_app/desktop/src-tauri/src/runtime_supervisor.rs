//! The app owns its runtime.
//!
//! Before this, the desktop shell proxied to `127.0.0.1:7474` and nothing
//! started anything: double-clicking Heiwa opened a window talking to a
//! server that did not exist. An installable application cannot ask the user
//! to run a server first — that is the difference between a product and a
//! development setup.
//!
//! So the app supervises a `heiwa app start` child for its own lifetime, and
//! adopts an already-running runtime instead of fighting it. The policy lives
//! in [`SupervisorDecision`] as a pure function of what was observed, so the
//! rules are testable without spawning processes or binding ports.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// What the app should do about the runtime, given what it observed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "action")]
pub enum SupervisorDecision {
    /// A runtime is already serving. Use it and start nothing.
    ///
    /// The developer case (a `heiwa app start` in a terminal) and the
    /// second-window case both land here. Spawning over a live runtime would
    /// fail on the port or, worse, contend for the evidence lease.
    Adopt,
    /// Nothing is serving. Start one and own it.
    Spawn { binary: PathBuf },
    /// Nothing is serving and no runtime binary can be found.
    ///
    /// Reported rather than retried: no amount of waiting produces a binary,
    /// and a spinner that never resolves is the worst version of this.
    Unavailable { detail: String },
}

/// What the app knows at startup.
#[derive(Debug, Clone)]
pub struct SupervisorFacts {
    /// Whether a *Heiwa* runtime already serves on the runtime port.
    ///
    /// Identity, not liveness: an unrelated listener holding the port is not
    /// something to adopt, it is something the bundled runtime has to be
    /// started in spite of.
    pub heiwa_runtime_serving: bool,
    /// The runtime binary, if one was found.
    pub binary: Option<PathBuf>,
}

impl SupervisorDecision {
    pub fn decide(facts: &SupervisorFacts) -> Self {
        if facts.heiwa_runtime_serving {
            return SupervisorDecision::Adopt;
        }
        match &facts.binary {
            Some(binary) => SupervisorDecision::Spawn {
                binary: binary.clone(),
            },
            None => SupervisorDecision::Unavailable {
                detail: "no `heiwa` runtime binary was found next to the app or on PATH"
                    .to_string(),
            },
        }
    }

    /// Whether the app started this runtime and must therefore stop it.
    ///
    /// An adopted runtime outlives the window: killing a server the user
    /// started in their terminal, because they closed a window, is a
    /// surprise the app has no right to spring.
    pub fn owns_runtime(&self) -> bool {
        matches!(self, SupervisorDecision::Spawn { .. })
    }
}

/// Locate the runtime binary.
///
/// Bundle-relative first: a packaged app must use the runtime it shipped
/// with, not whatever version happens to be on the user's PATH. Only when
/// there is no sibling — a developer running `tauri dev` — does PATH apply.
pub fn find_runtime_binary(
    resource_dir: Option<&std::path::Path>,
    executable_dir: Option<&std::path::Path>,
    path_lookup: impl Fn(&str) -> Option<PathBuf>,
    exists: impl Fn(&std::path::Path) -> bool,
) -> Option<PathBuf> {
    const BINARY: &str = if cfg!(windows) { "heiwa.exe" } else { "heiwa" };

    // The resource directory Tauri itself resolves, which is the only
    // portable answer. Each platform puts bundled resources somewhere
    // different — macOS `Contents/Resources`, Linux `usr/lib/<product>`,
    // Windows beside the executable — and hardcoding those layouts gets
    // Linux wrong, which is exactly how a deb or AppImage ends up installed
    // with a runtime it cannot find.
    //
    // Inside that directory the bundler keeps each resource's relative path,
    // so the staged `resources/heiwa` arrives under a `resources/`
    // subdirectory rather than at the root. Both are accepted: the app should
    // not break on a staging detail it does not control.
    if let Some(dir) = resource_dir {
        let staged = dir.join("resources").join(BINARY);
        if exists(&staged) {
            return Some(staged);
        }
        let bundled = dir.join(BINARY);
        if exists(&bundled) {
            return Some(bundled);
        }
    }

    // Fallbacks for a build where the resource directory is not available:
    // beside the executable, then the macOS bundle layout.
    if let Some(dir) = executable_dir {
        let sibling = dir.join(BINARY);
        if exists(&sibling) {
            return Some(sibling);
        }
        let macos_resources = dir.join("../Resources");
        let macos_staged = macos_resources.join("resources").join(BINARY);
        if exists(&macos_staged) {
            return Some(macos_staged);
        }
        let macos_bundled = macos_resources.join(BINARY);
        if exists(&macos_bundled) {
            return Some(macos_bundled);
        }
    }

    // Development only: no bundle, so whatever the developer has installed.
    path_lookup(BINARY)
}

// ---------------------------------------------------------------------------
// Live supervision
// ---------------------------------------------------------------------------

use std::process::{Child, Command};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long to wait for a spawned runtime to answer before giving up.
///
/// First launch does more than later ones — it creates the state tree and
/// recovers interrupted turns — so this is generous. It is a liveness
/// backstop, not a latency budget.
const READY_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(150);

/// The runtime this app started, held for the app's lifetime.
pub struct OwnedRuntime {
    child: Mutex<Option<Child>>,
}

impl OwnedRuntime {
    /// Stop the runtime this app started.
    ///
    /// Called on window close. An adopted runtime never reaches here, so a
    /// server the user started in a terminal survives closing the window.
    pub fn shutdown(&self) {
        let Ok(mut guard) = self.child.lock() else {
            return;
        };
        if let Some(mut child) = guard.take() {
            // The runtime holds an evidence lease and a heartbeat file. Ask
            // first; a kill leaves a stale lease that blocks the next start.
            #[cfg(unix)]
            unsafe {
                libc::kill(child.id() as i32, libc::SIGTERM);
            }
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => return,
                    Ok(None) if Instant::now() < deadline => {
                        std::thread::sleep(POLL_INTERVAL);
                    }
                    _ => break,
                }
            }
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for OwnedRuntime {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Bring a runtime up, or adopt one already running.
///
/// Two probes, because they answer different questions. `identifies_as_heiwa`
/// decides adoption and must prove the process on the port is a Heiwa runtime
/// this app can drive. `listening` is the cheap liveness poll used only while
/// a child *this* app started takes the port; it is never sufficient on its
/// own, so readiness still confirms identity before the window is told the
/// runtime is up.
///
/// Returns the decision taken and, when this app started the runtime, the
/// handle that stops it again.
pub fn ensure_runtime(
    identifies_as_heiwa: impl Fn() -> bool,
    listening: impl Fn() -> bool,
    spawn: impl FnOnce(&std::path::Path) -> std::io::Result<Child>,
    binary: Option<PathBuf>,
) -> (SupervisorDecision, Option<OwnedRuntime>) {
    let decision = SupervisorDecision::decide(&SupervisorFacts {
        heiwa_runtime_serving: identifies_as_heiwa(),
        binary,
    });

    let SupervisorDecision::Spawn { binary } = &decision else {
        return (decision, None);
    };

    let child = match spawn(binary) {
        Ok(child) => child,
        Err(error) => {
            return (
                SupervisorDecision::Unavailable {
                    detail: format!("could not start `{}`: {error}", binary.display()),
                },
                None,
            );
        }
    };

    // Wait for it to actually serve. Returning before the port answers would
    // make the first surface load fail on a runtime that was merely slow. The
    // cheap TCP poll gates the expensive identity check, and identity is what
    // decides readiness: if something else holds the port, our child never
    // takes it, and reporting ready on the foreign listener's TCP answer would
    // hand the window a server that cannot serve it.
    let deadline = Instant::now() + READY_TIMEOUT;
    while Instant::now() < deadline {
        if listening() && identifies_as_heiwa() {
            return (
                decision,
                Some(OwnedRuntime {
                    child: Mutex::new(Some(child)),
                }),
            );
        }
        std::thread::sleep(POLL_INTERVAL);
    }

    // Started but never answered as a Heiwa runtime. Keep the handle so the
    // stuck child is still cleaned up rather than left behind holding the
    // evidence lease.
    let detail = if listening() {
        format!(
            "the runtime did not take the port within {}s; something else is listening on it and \
             is not a Heiwa runtime",
            READY_TIMEOUT.as_secs()
        )
    } else {
        format!(
            "the runtime started but did not answer within {}s",
            READY_TIMEOUT.as_secs()
        )
    };
    (
        SupervisorDecision::Unavailable { detail },
        Some(OwnedRuntime {
            child: Mutex::new(Some(child)),
        }),
    )
}

/// Start the runtime the way the app needs it: serving, and not opening a
/// browser window of its own.
pub fn spawn_runtime(binary: &std::path::Path) -> std::io::Result<Child> {
    Command::new(binary)
        .args(["app", "start", "--no-open"])
        .stdin(std::process::Stdio::null())
        .spawn()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(heiwa_runtime_serving: bool, binary: Option<&str>) -> SupervisorFacts {
        SupervisorFacts {
            heiwa_runtime_serving,
            binary: binary.map(PathBuf::from),
        }
    }

    #[test]
    fn a_running_runtime_is_adopted_not_replaced() {
        // A developer with `heiwa app start` in a terminal, or a second
        // window: spawning would collide on the port and contend for the
        // evidence lease.
        let decision = SupervisorDecision::decide(&facts(true, Some("/usr/local/bin/heiwa")));
        assert_eq!(decision, SupervisorDecision::Adopt);
        assert!(!decision.owns_runtime());
    }

    #[test]
    fn nothing_running_and_a_binary_present_means_start_it() {
        let decision = SupervisorDecision::decide(&facts(false, Some("/Applications/heiwa")));
        assert_eq!(
            decision,
            SupervisorDecision::Spawn {
                binary: PathBuf::from("/Applications/heiwa")
            }
        );
        assert!(decision.owns_runtime(), "a spawned runtime must be stopped");
    }

    #[test]
    fn nothing_running_and_no_binary_is_reported_rather_than_retried() {
        // No amount of waiting produces a binary. A spinner here would be a
        // window that never becomes usable and never says why.
        let decision = SupervisorDecision::decide(&facts(false, None));
        let SupervisorDecision::Unavailable { detail } = &decision else {
            panic!("expected Unavailable, got {decision:?}");
        };
        assert!(!detail.trim().is_empty());
        assert!(!decision.owns_runtime());
    }

    #[test]
    fn the_bundled_runtime_wins_over_whatever_is_on_path() {
        // A packaged app must run the runtime it shipped with. Picking up a
        // different version from PATH is how an app and its runtime drift
        // into speaking different protocols on a user's machine.
        let found = find_runtime_binary(
            None,
            Some(std::path::Path::new(
                "/Applications/Heiwa.app/Contents/MacOS",
            )),
            |_| Some(PathBuf::from("/opt/homebrew/bin/heiwa")),
            |path| path.starts_with("/Applications/Heiwa.app"),
        );

        assert_eq!(
            found,
            Some(PathBuf::from(if cfg!(windows) {
                "/Applications/Heiwa.app/Contents/MacOS/heiwa.exe"
            } else {
                "/Applications/Heiwa.app/Contents/MacOS/heiwa"
            }))
        );
    }

    #[test]
    fn path_is_the_fallback_when_no_runtime_shipped_alongside() {
        // `tauri dev`: the app binary is in target/debug with no sibling.
        let found = find_runtime_binary(
            None,
            Some(std::path::Path::new("/repo/target/debug")),
            |_| Some(PathBuf::from("/opt/homebrew/bin/heiwa")),
            |_| false,
        );

        assert_eq!(found, Some(PathBuf::from("/opt/homebrew/bin/heiwa")));
    }

    #[test]
    fn no_sibling_and_nothing_on_path_finds_nothing() {
        assert_eq!(
            find_runtime_binary(
                None,
                Some(std::path::Path::new("/repo/target/debug")),
                |_| None,
                |_| false
            ),
            None
        );
    }

    #[test]
    fn an_adopted_runtime_is_not_owned_and_so_is_never_stopped() {
        let (decision, owned) = ensure_runtime(
            || true,
            || true,
            |_| panic!("must not spawn over a live runtime"),
            Some(PathBuf::from("/usr/local/bin/heiwa")),
        );

        assert_eq!(decision, SupervisorDecision::Adopt);
        assert!(
            owned.is_none(),
            "closing the window must not kill a runtime the user started"
        );
    }

    #[test]
    fn a_listening_port_is_not_adoption_evidence_and_does_not_signal_readiness() {
        // A port answering is not a runtime existing. Adoption keyed on a bare
        // TCP connect meant an unrelated service on 7474 left the bundled
        // runtime unstarted and the window bound to a server that could not
        // answer a single Heiwa call. Readiness has the same flaw in reverse:
        // the socket binds before the runtime can serve.
        //
        // So: something is listening throughout, and nothing identifies as
        // Heiwa until the third probe. The bundled runtime must still start,
        // and readiness must wait for identity.
        let identity_probes = std::cell::Cell::new(0_u32);
        let spawned = std::cell::Cell::new(false);
        let (decision, owned) = ensure_runtime(
            || {
                identity_probes.set(identity_probes.get() + 1);
                identity_probes.get() > 2
            },
            || true,
            |_| {
                spawned.set(true);
                Command::new(if cfg!(windows) { "cmd" } else { "sleep" })
                    .args(if cfg!(windows) {
                        vec!["/c", "timeout", "60"]
                    } else {
                        vec!["60"]
                    })
                    .spawn()
            },
            Some(PathBuf::from("/Applications/heiwa")),
        );

        assert!(spawned.get(), "the bundled runtime must still be started");
        assert_eq!(
            decision,
            SupervisorDecision::Spawn {
                binary: PathBuf::from("/Applications/heiwa")
            }
        );
        assert!(
            identity_probes.get() > 2,
            "identity, not the open port, must decide adoption and readiness"
        );
        owned.expect("a spawned runtime is owned").shutdown();
    }

    #[test]
    fn a_runtime_that_never_answers_is_reported_and_still_cleaned_up() {
        // Started but wedged. Reporting without the handle would leak a
        // child that holds the evidence lease and blocks the next start.
        let (decision, owned) = ensure_runtime(
            || false,
            || false,
            |_| {
                Command::new(if cfg!(windows) { "cmd" } else { "sleep" })
                    .args(if cfg!(windows) {
                        vec!["/c", "timeout", "60"]
                    } else {
                        vec!["60"]
                    })
                    .spawn()
            },
            Some(PathBuf::from("unused")),
        );

        assert!(matches!(decision, SupervisorDecision::Unavailable { .. }));
        assert!(owned.is_some(), "a stuck child must still be reaped");
        owned.unwrap().shutdown();
    }

    #[test]
    fn a_binary_that_cannot_be_started_is_reported_not_panicked() {
        let (decision, owned) = ensure_runtime(
            || false,
            || false,
            |_| Err(std::io::Error::new(std::io::ErrorKind::NotFound, "nope")),
            Some(PathBuf::from("/does/not/exist/heiwa")),
        );

        let SupervisorDecision::Unavailable { detail } = &decision else {
            panic!("expected Unavailable, got {decision:?}");
        };
        assert!(detail.contains("/does/not/exist/heiwa"), "{detail}");
        assert!(owned.is_none());
    }

    #[test]
    fn the_macos_bundle_finds_its_runtime_in_contents_resources() {
        // Contents/MacOS holds the app binary; bundled resources land in
        // Contents/Resources. Looking only beside the executable would miss
        // the runtime the bundle shipped and silently fall through to PATH.
        let found = find_runtime_binary(
            None,
            Some(std::path::Path::new(
                "/Applications/Heiwa.app/Contents/MacOS",
            )),
            |_| Some(PathBuf::from("/opt/homebrew/bin/heiwa")),
            |path| path.to_string_lossy().contains("../Resources"),
        );

        let found = found.expect("the bundled runtime");
        assert!(
            found.to_string_lossy().contains("Resources"),
            "resolved to {} instead of the bundled runtime",
            found.display()
        );
    }

    #[test]
    fn a_linux_package_finds_the_runtime_in_its_resource_directory() {
        // A deb or AppImage installs the executable to usr/bin and bundled
        // resources to usr/lib/<product>. Searching only beside the
        // executable misses it, so the installed app could not start the
        // runtime it shipped with — it would fall through to PATH, or to
        // nothing at all.
        //
        // The packaged layout is exact: the bundler keeps a resource's
        // relative path, so the staged `resources/heiwa` installs to
        // usr/lib/<product>/resources/heiwa and nothing sits at the root of
        // the resource directory.
        let installed = if cfg!(windows) {
            "/usr/lib/Heiwa/resources/heiwa.exe"
        } else {
            "/usr/lib/Heiwa/resources/heiwa"
        };
        let found = find_runtime_binary(
            Some(std::path::Path::new("/usr/lib/Heiwa")),
            Some(std::path::Path::new("/usr/bin")),
            |_| None,
            |path| path == std::path::Path::new(installed),
        );

        assert_eq!(found, Some(PathBuf::from(installed)));
    }

    #[test]
    fn a_runtime_staged_at_the_resource_root_is_still_found() {
        // Staging is the workflow's business, not the app's: a runtime placed
        // directly in the resource directory must work too, or a change to
        // how the bundle is assembled silently ships an app that cannot start
        // its runtime.
        let installed = if cfg!(windows) {
            "/usr/lib/Heiwa/heiwa.exe"
        } else {
            "/usr/lib/Heiwa/heiwa"
        };
        let found = find_runtime_binary(
            Some(std::path::Path::new("/usr/lib/Heiwa")),
            Some(std::path::Path::new("/usr/bin")),
            |_| None,
            |path| path == std::path::Path::new(installed),
        );

        assert_eq!(found, Some(PathBuf::from(installed)));
    }

    #[test]
    fn the_resource_directory_wins_over_a_sibling_and_over_path() {
        // When Tauri answers, that answer is authoritative: it is the only
        // source that is correct on every platform.
        let found = find_runtime_binary(
            Some(std::path::Path::new("/bundle/resources")),
            Some(std::path::Path::new("/bundle/bin")),
            |_| Some(PathBuf::from("/opt/homebrew/bin/heiwa")),
            |_| true,
        );

        assert!(
            found.expect("a runtime").starts_with("/bundle/resources"),
            "the resource directory must take precedence"
        );
    }
}
