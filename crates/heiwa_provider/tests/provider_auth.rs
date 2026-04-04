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
