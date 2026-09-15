//! L-009: the app-manifest ACL is only as good as its coverage of the real
//! command set. This test reads `src/lib.rs` and `build.rs` as plain text
//! and cross-checks two lists that must never drift apart:
//!
//! - the commands registered in `tauri::generate_handler![...]` (what the
//!   webview can *reach*), and
//! - the commands declared in `build.rs`'s `COMMANDS` (what `tauri-build`
//!   autogenerates an `allow-<command>` / `deny-<command>` permission for,
//!   and therefore what `capabilities/default.json` can grant).
//!
//! If a command is added to one list and not the other, either it is
//! invokable with zero capability gating (added to the handler but not the
//! manifest) or a real command silently stops working once someone tries to
//! grant it (added to the manifest but never registered). Both are bugs this
//! test exists to catch before review.
//!
//! This is deliberately a text scan rather than a proc-macro/AST parse: the
//! two source files are small, hand-written, and use one simple call-site
//! shape each. A change to that shape (e.g. reformatting the macro call
//! across multiple `generate_handler!` invocations) should fail loudly here
//! rather than silently stop checking anything, which is why every parsing
//! step asserts the shape it expects rather than falling back quietly.

use std::fs;
use std::path::PathBuf;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Pulls the comma-separated contents out of the first `open`..`close`
/// bracketed region found after `marker`, assuming no nested brackets of the
/// same kind appear inside (true for both call sites this test reads).
fn extract_bracketed(source: &str, marker: &str, open: char, close: char) -> Vec<String> {
    let start = source
        .find(marker)
        .unwrap_or_else(|| panic!("expected to find `{marker}` in source"));
    let after_marker = &source[start + marker.len()..];
    let open_at = after_marker
        .find(open)
        .unwrap_or_else(|| panic!("expected `{open}` after `{marker}`"));
    let close_at = after_marker[open_at + 1..]
        .find(close)
        .unwrap_or_else(|| panic!("expected closing `{close}` after `{marker}`"));
    let inner = &after_marker[open_at + 1..open_at + 1 + close_at];
    inner
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect()
}

/// `generate_handler!` entries may be module-qualified (`herd::herd_panes`);
/// the string Tauri actually registers and the frontend invokes by is the
/// bare function name, so that's what this test compares against `build.rs`.
fn bare_command_name(entry: &str) -> String {
    entry
        .rsplit("::")
        .next()
        .expect("split always yields at least one segment")
        .trim()
        .to_string()
}

fn unquote(entry: &str) -> String {
    let trimmed = entry.trim();
    let trimmed = trimmed.strip_prefix('"').unwrap_or(trimmed);
    let trimmed = trimmed.strip_suffix('"').unwrap_or(trimmed);
    trimmed.to_string()
}

fn generate_handler_commands() -> Vec<String> {
    let lib_rs =
        fs::read_to_string(manifest_dir().join("src/lib.rs")).expect("src/lib.rs must be readable");
    extract_bracketed(&lib_rs, "tauri::generate_handler!", '[', ']')
        .into_iter()
        .map(|entry| bare_command_name(&entry))
        .collect()
}

fn app_manifest_commands() -> Vec<String> {
    let build_rs =
        fs::read_to_string(manifest_dir().join("build.rs")).expect("build.rs must be readable");
    extract_bracketed(&build_rs, "const COMMANDS: &[&str] = &", '[', ']')
        .into_iter()
        .map(|entry| unquote(&entry))
        .collect()
}

#[test]
fn generate_handler_and_app_manifest_commands_match() {
    let mut handler_commands = generate_handler_commands();
    let mut manifest_commands = app_manifest_commands();

    assert!(
        !handler_commands.is_empty(),
        "parsed zero commands out of tauri::generate_handler![..] in src/lib.rs; \
         the parser in this test likely needs updating for a reformatted macro call"
    );
    assert!(
        !manifest_commands.is_empty(),
        "parsed zero commands out of build.rs's COMMANDS; \
         the parser in this test likely needs updating for a reformatted const"
    );

    handler_commands.sort();
    handler_commands.dedup();
    manifest_commands.sort();
    manifest_commands.dedup();

    if handler_commands != manifest_commands {
        let only_in_handler: Vec<_> = handler_commands
            .iter()
            .filter(|command| !manifest_commands.contains(command))
            .collect();
        let only_in_manifest: Vec<_> = manifest_commands
            .iter()
            .filter(|command| !handler_commands.contains(command))
            .collect();
        panic!(
            "src/lib.rs's generate_handler! and build.rs's COMMANDS have drifted.\n\
             Registered but not in the app-manifest ACL (invokable with no capability \
             gating until build.rs is updated): {only_in_handler:?}\n\
             In the app-manifest ACL but never registered (dead permission, or a typo \
             for a real command): {only_in_manifest:?}"
        );
    }
}

#[test]
fn app_manifest_commands_are_plain_identifiers() {
    // A quoting or parsing mistake in build.rs (e.g. an accidental module
    // path or stray punctuation) would otherwise silently produce a
    // permission for a command that can never match a real invoke call.
    for command in app_manifest_commands() {
        assert!(
            !command.is_empty()
                && command
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
                && command.chars().next().is_some_and(|c| !c.is_ascii_digit()),
            "build.rs COMMANDS entry {command:?} is not a plain snake_case identifier"
        );
    }
}
