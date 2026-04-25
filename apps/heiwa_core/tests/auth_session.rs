use heiwa_core::{
    auth::{
        auth_me_from_headers, auth_me_payload, extract_auth_subject, sign_jwt, verify_jwt,
        AuthClaims, AuthSubject,
    },
    config::RuntimeConfig,
};

fn test_config() -> RuntimeConfig {
    RuntimeConfig {
        port: 8080,
        state_backend: "spacetimedb".to_string(),
        stdb_server: "maincloud".to_string(),
        stdb_identity: "heiwaproductiondb".to_string(),
        stdb_url: "https://maincloud.spacetimedb.com".to_string(),
        stdb_token: "stdb-token".to_string(),
        log_level: "INFO".to_string(),
        machine_auth_token: "operator-token".to_string(),
        jwt_signing_secret: "auth-secret".to_string(),
        node_id: "heiwa-core-0".to_string(),
        model_tiers_seed_path: "config/seeds/model_tiers.json".to_string(),
        ai_router_seed_path: "config/swarm/ai_router.json".to_string(),
    }
}

fn test_claims() -> AuthClaims {
    AuthClaims {
        sub: "user-alpha".to_string(),
        owner_id: "user-alpha".to_string(),
        principal_id: "discord:alpha".to_string(),
        username: Some("alpha".to_string()),
        discord_user_id: Some("discord-alpha".to_string()),
        iat: 1_700_000_000,
        exp: 4_102_444_800,
        iss: "heiwa-core".to_string(),
        aud: "heiwa".to_string(),
    }
}

#[test]
fn jwt_round_trip_preserves_identity_claims() {
    let token = sign_jwt(&test_claims(), "auth-secret").expect("token");
    let decoded = verify_jwt(&token, "auth-secret").expect("claims");

    assert_eq!(decoded.sub, "user-alpha");
    assert_eq!(decoded.owner_id, "user-alpha");
    assert_eq!(decoded.principal_id, "discord:alpha");
    assert_eq!(decoded.discord_user_id.as_deref(), Some("discord-alpha"));
}

#[test]
fn jwt_signing_matches_standard_hs256_reference() {
    let token = sign_jwt(&test_claims(), "auth-secret").expect("token");

    assert_eq!(
        token,
        "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyLWFscGhhIiwib3duZXJfaWQiOiJ1c2VyLWFscGhhIiwicHJpbmNpcGFsX2lkIjoiZGlzY29yZDphbHBoYSIsInVzZXJuYW1lIjoiYWxwaGEiLCJkaXNjb3JkX3VzZXJfaWQiOiJkaXNjb3JkLWFscGhhIiwiaWF0IjoxNzAwMDAwMDAwLCJleHAiOjQxMDI0NDQ4MDAsImlzcyI6ImhlaXdhLWNvcmUiLCJhdWQiOiJoZWl3YSJ9.uW4DUlEcXQREsPWSdJ3j1ttpM2hwa6ZWyJbMvKZqQZk"
    );
}

#[test]
fn extract_auth_subject_accepts_cookie_and_machine_token() {
    let user_token = sign_jwt(&test_claims(), "auth-secret").expect("token");
    let cfg = test_config();

    let user = extract_auth_subject(Some(&format!("heiwa_session={user_token}")), None, &cfg)
        .expect("user auth");

    match user {
        AuthSubject::User(claims) => assert_eq!(claims.owner_id, "user-alpha"),
        AuthSubject::Operator => panic!("expected user auth subject"),
    }

    let operator =
        extract_auth_subject(None, Some("Bearer operator-token"), &cfg).expect("operator");
    assert!(matches!(operator, AuthSubject::Operator));
}

#[test]
fn auth_me_payload_reports_user_and_operator_identity() {
    let user_payload = auth_me_payload(&AuthSubject::User(test_claims()));
    assert_eq!(user_payload["authenticated"], true);
    assert_eq!(user_payload["subject"]["kind"], "user");
    assert_eq!(user_payload["subject"]["owner_id"], "user-alpha");

    let operator_payload = auth_me_payload(&AuthSubject::Operator);
    assert_eq!(operator_payload["authenticated"], true);
    assert_eq!(operator_payload["subject"]["kind"], "operator");
}

#[test]
fn auth_me_from_headers_accepts_signed_session_cookie() {
    let user_token = sign_jwt(&test_claims(), "auth-secret").expect("token");
    let cfg = test_config();

    let (status, payload) =
        auth_me_from_headers(Some(&format!("heiwa_session={user_token}")), None, &cfg);

    assert_eq!(status, 200);
    assert_eq!(payload["authenticated"], true);
    assert_eq!(payload["subject"]["kind"], "user");
    assert_eq!(payload["subject"]["owner_id"], "user-alpha");
}

#[test]
fn auth_me_from_headers_rejects_missing_credentials() {
    let cfg = test_config();

    let (status, payload) = auth_me_from_headers(None, None, &cfg);

    assert_eq!(status, 401);
    assert_eq!(payload["authenticated"], false);
    assert_eq!(payload["error"], "unauthorized");
}
