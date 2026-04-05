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
