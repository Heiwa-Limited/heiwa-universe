use heiwa_provider::{get_auth_status, AuthKind};

#[test]
fn test_provider_status_discovery() {
    // We expect built-in providers to be at least discoverable
    let claude = get_auth_status("claude");
    assert!(claude.is_some(), "Claude provider should be known");

    let acc = claude.unwrap();
    assert_eq!(acc.provider_id, "claude");
    assert_eq!(acc.auth_kind, AuthKind::OauthCli);
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
    // On this machine, it should be connected since we have oauth_creds.json
    assert_eq!(gemini.status, "connected");
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
    if gemini_oauth.exists() && ag_init.exists() {
        assert_eq!(ag.status, "connected");
    }
}
