//! L2 first-run contract.
//!
//! The roadmap scopes L2 — "first run establishes a local user identity, a
//! configuration root, and at least one provider account, entirely inside the
//! application" — but its Verification section defines criteria for L0, L1,
//! L3, and L4 and none for L2. This file is the criterion, chosen to match
//! the wording: *the application itself takes a user from an empty state root
//! to a completed turn, without reading documentation.*
//!
//! Concretely: `heiwa setup` on an empty root reports every gap with a remedy
//! and exits non-zero; running the remedy it names closes that gap; and once
//! no gaps remain, a turn completes. Every step drives the shipped binary, so
//! a remedy that does not actually work fails the test.
//!
//! Isolation is the same as the L1 harness: emptied `PATH`, no system bin
//! directories, a dead local-runtime endpoint, and a loopback mock.

mod support;

use support::{FreshInstall, MockProvider, HARNESS_API_KEY};

/// THE L2 CONTRACT: an empty state root reaches a completed turn using only
/// what the application itself tells the user to do.
#[test]
fn first_run_walks_an_empty_install_to_a_working_turn() {
    let mock = MockProvider::start();
    let install = FreshInstall::empty();

    // 1. The application reports what is missing, and says so non-zero.
    let setup = install.run(&mock, &["setup", "--name", "Ada"]);
    let report = setup.text();
    assert!(
        !setup.status.success(),
        "an incomplete install must not report success: {report}"
    );
    assert!(
        report.contains("provider"),
        "the report must name the provider gap: {report}"
    );
    assert!(
        report.contains("auth add-key"),
        "the report must name the remedy, not just the problem: {report}"
    );
    assert!(
        report.contains("ANTHROPIC_API_KEY"),
        "the remedy must name the path that works without a keychain: {report}"
    );

    // 2. Identity — the gap setup can close itself — is closed.
    let whoami = install.run(&mock, &["whoami"]);
    assert!(whoami.status.success(), "whoami failed: {}", whoami.text());
    assert!(
        whoami.stdout.contains("Ada"),
        "identity was not established: {}",
        whoami.stdout
    );

    // 3. The user supplies a provider key. The keychain path (`auth
    //    add-key`) is not exercised here on purpose: it writes to the real
    //    developer keychain and does not exist on a headless machine at all.
    //    The environment path is the one a container user takes, and it must
    //    work with no keychain present.
    install.provide_api_key();

    // 4. Setup now reports a complete install, and says so with exit 0.
    let setup = install.run(&mock, &["setup"]);
    assert!(
        setup.status.success(),
        "install still incomplete after following its own instructions: {}",
        setup.text()
    );
    assert!(
        setup.stdout.contains("Ada"),
        "a completed setup should greet the user it set up: {}",
        setup.stdout
    );

    // 5. And the thing the user came for works.
    let ask = install.run(&mock, &["ask", "Say hello."]);
    assert!(ask.status.success(), "turn failed: {}", ask.text());
    assert!(
        ask.stdout.contains("Heiwa is running."),
        "the model's reply did not reach the user: {}",
        ask.stdout
    );

    let turn = mock
        .request_matching("POST /v1/messages")
        .expect("the turn reached the provider");
    assert_eq!(
        turn.headers.get("x-api-key").map(String::as_str),
        Some(HARNESS_API_KEY),
        "the turn did not use the key added during setup"
    );
}

/// The identity is minted once and never moves under the user.
#[test]
fn re_running_setup_keeps_the_installation_id() {
    let mock = MockProvider::start();
    let install = FreshInstall::empty();

    install.run(&mock, &["setup", "--name", "Ada"]);
    let first = install.run(&mock, &["whoami"]).stdout;

    install.run(&mock, &["setup", "--name", "Ada"]);
    let second = install.run(&mock, &["whoami"]).stdout;

    assert_eq!(
        first, second,
        "re-running setup changed the identity connector credentials attach to"
    );
}

/// Renaming is a display change, not a new installation.
#[test]
fn renaming_changes_the_name_and_nothing_else() {
    let mock = MockProvider::start();
    let install = FreshInstall::empty();

    install.run(&mock, &["setup", "--name", "Ada"]);
    let before = installation_id(&install.run(&mock, &["whoami"]).stdout);

    install.run(&mock, &["setup", "--name", "Ada Lovelace"]);
    let after = install.run(&mock, &["whoami"]).stdout;

    assert!(
        after.contains("Ada Lovelace"),
        "rename did not apply: {after}"
    );
    assert_eq!(
        before,
        installation_id(&after),
        "renaming minted a new installation id"
    );
}

// A process-level "no state root" test is not written here: on macOS
// `dirs::home_dir()` reads the passwd entry regardless of `env_clear()`, so
// the state a test would need cannot be produced, and a test that cannot fail
// is worse than none. The projection's behavior without a root is covered by
// `heiwa_identity::onboarding` unit tests, which inject the fact directly.

fn installation_id(whoami: &str) -> String {
    whoami
        .lines()
        .find_map(|line| line.trim().strip_prefix("installation:"))
        .unwrap_or_default()
        .trim()
        .to_string()
}
