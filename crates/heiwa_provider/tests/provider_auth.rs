use heiwa_provider::{get_auth_status, AuthKind};
use std::path::PathBuf;
use std::process::Command;

#[test]
fn test_provider_status_discovery() {
    // We expect built-in providers to be at least discoverable
    let claude = get_auth_status("claude");
    assert!(claude.is_some(), "Claude provider should be known");

    let acc = claude.unwrap();
    assert_eq!(acc.provider_id, "claude");
    assert_eq!(acc.auth_kind, AuthKind::OauthCli);
}

#[tokio::test(flavor = "multi_thread")]
async fn ollama_status_probe_is_safe_inside_async_runtime() {
    let ollama = get_auth_status("ollama").expect("ollama provider should be known");
    assert_eq!(ollama.provider_id, "ollama");
    assert_eq!(ollama.auth_kind, AuthKind::LocalRuntime);
}

#[test]
fn test_antigravity_provider_is_known_and_codex_is_oauth_cli() {
    let antigravity = get_auth_status("antigravity").expect("antigravity provider should be known");
    assert_eq!(antigravity.provider_id, "antigravity");
    assert_eq!(antigravity.auth_kind, AuthKind::OauthCli);

    let codex = get_auth_status("codex").expect("codex provider should be known");
    assert_eq!(codex.provider_id, "codex");
    assert_eq!(codex.auth_kind, AuthKind::OauthCli);
}

#[test]
fn test_gemini_discovery() {
    let gemini = get_auth_status("gemini").expect("gemini provider should be known");
    assert_eq!(gemini.provider_id, "gemini");
    assert_eq!(gemini.auth_kind, AuthKind::OauthCli);
    assert!(
        matches!(
            gemini.status.as_str(),
            "connected" | "installed_unverified" | "not_installed"
        ),
        "unexpected Gemini auth status: {}",
        gemini.status
    );

    let gemini_oauth = PathBuf::from(std::env::var("HOME").unwrap_or_default())
        .join(".gemini")
        .join("oauth_creds.json");
    if has_command("gemini") && gemini_oauth.exists() {
        assert_eq!(gemini.status, "connected");
    }
}

#[test]
fn test_codex_native_auth_discovery() {
    let codex = get_auth_status("codex").expect("codex provider should be known");
    assert_eq!(codex.provider_id, "codex");
    assert_eq!(codex.auth_kind, AuthKind::OauthCli);
    let home = std::env::var("HOME").unwrap_or_default();
    let auth_json = std::path::PathBuf::from(home)
        .join(".codex")
        .join("auth.json");
    if auth_json.exists() {
        assert_eq!(codex.status, "connected", "auth.json present => connected");
    }
}

#[test]
fn test_antigravity_native_auth_discovery() {
    let ag = get_auth_status("antigravity").expect("antigravity provider should be known");
    assert_eq!(ag.auth_kind, AuthKind::OauthCli);
    let home = std::env::var("HOME").unwrap_or_default();
    let gemini_oauth = std::path::PathBuf::from(&home)
        .join(".gemini")
        .join("oauth_creds.json");
    let ag_init = std::path::PathBuf::from(&home)
        .join(".antigravity")
        .join("argv.json");
    if has_command("antigravity") && gemini_oauth.exists() && ag_init.exists() {
        assert_eq!(ag.status, "connected");
    }
}

#[test]
fn test_claude_native_auth_discovery() {
    let claude = get_auth_status("claude").expect("claude provider should be known");
    assert_eq!(claude.provider_id, "claude");
    assert_eq!(claude.auth_kind, AuthKind::OauthCli);
    let home = std::env::var("HOME").unwrap_or_default();
    let claude_dir = PathBuf::from(&home).join(".claude");
    let settings = claude_dir.join("settings.json");
    if settings.exists() {
        assert_eq!(
            claude.status, "connected",
            "settings.json present => connected"
        );
    }
}

fn has_command(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}
