use heiwa_core::{
    auth::{
        auth_me_from_headers, auth_me_payload, extract_auth_subject, sign_jwt, sign_local_request,
        verify_jwt, verify_local_request, AuthClaims, AuthSubject, LocalRequestParts,
        LOCAL_REQUEST_MAX_BODY_BYTES, LOCAL_REQUEST_MAX_TARGET_BYTES,
    },
    config::RuntimeConfig,
};

fn test_config() -> RuntimeConfig {
    RuntimeConfig {
        port: 8080,
        state_backend: "local-jsonl".to_string(),
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
    let expected = [
        "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9",
        "eyJzdWIiOiJ1c2VyLWFscGhhIiwib3duZXJfaWQiOiJ1c2VyLWFscGhhIiwicHJpbmNpcGFsX2lkIjoiZGlzY29yZDphbHBoYSIsInVzZXJuYW1lIjoiYWxwaGEiLCJkaXNjb3JkX3VzZXJfaWQiOiJkaXNjb3JkLWFscGhhIiwiaWF0IjoxNzAwMDAwMDAwLCJleHAiOjQxMDI0NDQ4MDAsImlzcyI6ImhlaXdhLWNvcmUiLCJhdWQiOiJoZWl3YSJ9",
        "uW4DUlEcXQREsPWSdJ3j1ttpM2hwa6ZWyJbMvKZqQZk",
    ]
    .join(".");

    assert_eq!(token, expected);
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

fn local_request_parts<'a>(body: &'a [u8]) -> LocalRequestParts<'a> {
    LocalRequestParts {
        method: "POST",
        port: 7475,
        target: "/api/v1/operator/threads/default/turns?mode=auto",
        body,
    }
}

#[test]
fn local_request_signature_matches_v1_contract_and_round_trips() {
    let body = br#"{"prompt":"peace"}"#;
    let signed = sign_local_request(
        local_request_parts(body),
        1_750_000_000,
        "0123456789abcdef0123456789abcdef",
        "machine-test-token",
    )
    .expect("signed request");

    assert_eq!(signed.version, "1");
    assert_eq!(signed.timestamp, "1750000000");
    assert_eq!(
        signed.signature,
        "IseIJXzbszlyNPl7_oOD_UxMDj9B6gdKeeDdFWOJgC8"
    );
    let verified = verify_local_request(
        local_request_parts(body),
        &signed,
        "machine-test-token",
        1_750_000_020,
    )
    .expect("signature verifies within skew");
    assert_eq!(verified.timestamp, 1_750_000_000);
    assert_eq!(verified.nonce, "0123456789abcdef0123456789abcdef");
}

#[test]
fn local_request_signature_binds_method_port_target_and_body() {
    let body = br#"{"prompt":"peace"}"#;
    let signed = sign_local_request(
        local_request_parts(body),
        1_750_000_000,
        "0123456789abcdef0123456789abcdef",
        "machine-test-token",
    )
    .unwrap();

    for changed in [
        LocalRequestParts {
            method: "GET",
            ..local_request_parts(body)
        },
        LocalRequestParts {
            port: 7474,
            ..local_request_parts(body)
        },
        LocalRequestParts {
            target: "/api/v1/operator/threads/default/turns?mode=manual",
            ..local_request_parts(body)
        },
        local_request_parts(br#"{"prompt":"war"}"#),
    ] {
        assert!(
            verify_local_request(changed, &signed, "machine-test-token", 1_750_000_000,).is_err()
        );
    }
}

#[test]
fn local_request_signature_rejects_skew_and_malformed_fields() {
    let body = b"";
    let signed = sign_local_request(
        LocalRequestParts {
            method: "GET",
            port: 7474,
            target: "/api/v1/operator/threads",
            body,
        },
        1_750_000_000,
        "0123456789abcdef0123456789abcdef",
        "machine-test-token",
    )
    .unwrap();

    assert!(verify_local_request(
        LocalRequestParts {
            method: "GET",
            port: 7474,
            target: "/api/v1/operator/threads",
            body,
        },
        &signed,
        "machine-test-token",
        1_750_000_031,
    )
    .is_err());

    for mutate in [
        (
            "2",
            signed.timestamp.as_str(),
            signed.nonce.as_str(),
            signed.signature.as_str(),
        ),
        (
            "1",
            "+1750000000",
            signed.nonce.as_str(),
            signed.signature.as_str(),
        ),
        (
            "1",
            signed.timestamp.as_str(),
            "short",
            signed.signature.as_str(),
        ),
        (
            "1",
            signed.timestamp.as_str(),
            signed.nonce.as_str(),
            "short",
        ),
    ] {
        let malformed = heiwa_core::auth::LocalRequestSignature {
            version: mutate.0.to_string(),
            timestamp: mutate.1.to_string(),
            nonce: mutate.2.to_string(),
            signature: mutate.3.to_string(),
        };
        assert!(verify_local_request(
            LocalRequestParts {
                method: "GET",
                port: 7474,
                target: "/api/v1/operator/threads",
                body,
            },
            &malformed,
            "machine-test-token",
            1_750_000_000,
        )
        .is_err());
    }
}

#[test]
fn local_request_signature_enforces_field_length_bounds() {
    let nonce = "0123456789abcdef0123456789abcdef";
    let oversized_target = format!("/{}", "a".repeat(LOCAL_REQUEST_MAX_TARGET_BYTES));
    assert!(sign_local_request(
        LocalRequestParts {
            method: "GET",
            port: 7474,
            target: &oversized_target,
            body: b"",
        },
        1_750_000_000,
        nonce,
        "machine-test-token",
    )
    .is_err());

    let oversized_body = vec![0_u8; LOCAL_REQUEST_MAX_BODY_BYTES + 1];
    assert!(sign_local_request(
        LocalRequestParts {
            method: "POST",
            port: 7474,
            target: "/api/v1/operator/threads",
            body: &oversized_body,
        },
        1_750_000_000,
        nonce,
        "machine-test-token",
    )
    .is_err());

    assert!(sign_local_request(
        LocalRequestParts {
            method: "METHODTHATISTOOLONG",
            port: 7474,
            target: "/api/v1/operator/threads",
            body: b"",
        },
        1_750_000_000,
        nonce,
        "machine-test-token",
    )
    .is_err());
    assert!(sign_local_request(
        LocalRequestParts {
            method: "GET",
            port: 7474,
            target: "/api/v1/operator/threads",
            body: b"",
        },
        1_750_000_000,
        nonce,
        &"s".repeat(4097),
    )
    .is_err());
}
