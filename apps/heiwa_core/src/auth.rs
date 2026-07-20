use anyhow::{anyhow, Result};
use axum::{extract::State, http::HeaderMap, response::IntoResponse, Json};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{config::RuntimeConfig, runtime::state::SharedState};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthClaims {
    pub sub: String,
    pub owner_id: String,
    pub principal_id: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub discord_user_id: Option<String>,
    pub iat: i64,
    pub exp: i64,
    pub iss: String,
    pub aud: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthSubject {
    Operator,
    User(AuthClaims),
}

pub const LOCAL_REQUEST_AUTH_VERSION_HEADER: &str = "x-heiwa-local-auth-version";
pub const LOCAL_REQUEST_AUTH_TIMESTAMP_HEADER: &str = "x-heiwa-local-auth-timestamp";
pub const LOCAL_REQUEST_AUTH_NONCE_HEADER: &str = "x-heiwa-local-auth-nonce";
pub const LOCAL_REQUEST_AUTH_SIGNATURE_HEADER: &str = "x-heiwa-local-auth-signature";
pub const LOCAL_REQUEST_AUTH_VERSION: &str = "1";
pub const LOCAL_REQUEST_MAX_TARGET_BYTES: usize = 8 * 1024;
pub const LOCAL_REQUEST_MAX_BODY_BYTES: usize = 10 * 1024 * 1024;
pub const LOCAL_REQUEST_MAX_SKEW_SECONDS: u64 = 30;

#[derive(Debug, Clone, Copy)]
pub struct LocalRequestParts<'a> {
    pub method: &'a str,
    pub port: u16,
    pub target: &'a str,
    pub body: &'a [u8],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalRequestSignature {
    pub version: String,
    pub timestamp: String,
    pub nonce: String,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedLocalRequest {
    pub timestamp: i64,
    pub nonce: String,
}

/// Sign the v1 local-runtime request contract.
///
/// The canonical input is newline-framed as domain, method, numeric local
/// port, exact request target, lowercase body SHA-256, timestamp, and nonce.
pub fn sign_local_request(
    parts: LocalRequestParts<'_>,
    timestamp: i64,
    nonce: &str,
    secret: &str,
) -> Result<LocalRequestSignature> {
    validate_local_request_parts(parts)?;
    validate_local_request_secret(secret)?;
    validate_local_request_timestamp(timestamp)?;
    validate_local_request_nonce(nonce)?;
    let canonical = canonical_local_request(parts, timestamp, nonce);
    let signature = URL_SAFE_NO_PAD.encode(sign_bytes(canonical.as_bytes(), secret)?);
    Ok(LocalRequestSignature {
        version: LOCAL_REQUEST_AUTH_VERSION.to_string(),
        timestamp: timestamp.to_string(),
        nonce: nonce.to_string(),
        signature,
    })
}

pub fn verify_local_request(
    parts: LocalRequestParts<'_>,
    signed: &LocalRequestSignature,
    secret: &str,
    now: i64,
) -> Result<VerifiedLocalRequest> {
    validate_local_request_parts(parts)?;
    validate_local_request_secret(secret)?;
    if signed.version != LOCAL_REQUEST_AUTH_VERSION {
        return Err(anyhow!("invalid local request auth version"));
    }
    let timestamp = parse_local_request_timestamp(&signed.timestamp)?;
    if timestamp.abs_diff(now) > LOCAL_REQUEST_MAX_SKEW_SECONDS {
        return Err(anyhow!("local request timestamp outside allowed skew"));
    }
    validate_local_request_nonce(&signed.nonce)?;
    if signed.signature.len() != 43
        || !signed
            .signature
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(anyhow!("invalid local request signature encoding"));
    }
    let actual = URL_SAFE_NO_PAD
        .decode(&signed.signature)
        .map_err(|_| anyhow!("invalid local request signature encoding"))?;
    let canonical = canonical_local_request(parts, timestamp, &signed.nonce);
    let expected = sign_bytes(canonical.as_bytes(), secret)?;
    if !constant_time_eq(&expected, &actual) {
        return Err(anyhow!("invalid local request signature"));
    }
    Ok(VerifiedLocalRequest {
        timestamp,
        nonce: signed.nonce.clone(),
    })
}

fn canonical_local_request(parts: LocalRequestParts<'_>, timestamp: i64, nonce: &str) -> String {
    format!(
        "heiwa-local-request-v1\n{}\n{}\n{}\n{}\n{}\n{}",
        parts.method,
        parts.port,
        parts.target,
        sha256_hex(parts.body),
        timestamp,
        nonce,
    )
}

fn validate_local_request_parts(parts: LocalRequestParts<'_>) -> Result<()> {
    if parts.method.is_empty()
        || parts.method.len() > 16
        || !parts.method.bytes().all(|byte| byte.is_ascii_uppercase())
    {
        return Err(anyhow!("invalid local request method"));
    }
    if parts.port == 0 {
        return Err(anyhow!("invalid local request port"));
    }
    if parts.target.is_empty()
        || parts.target.len() > LOCAL_REQUEST_MAX_TARGET_BYTES
        || !parts.target.starts_with('/')
        || !parts
            .target
            .bytes()
            .all(|byte| byte.is_ascii() && !byte.is_ascii_control() && byte != b' ')
    {
        return Err(anyhow!("invalid local request target"));
    }
    if parts.body.len() > LOCAL_REQUEST_MAX_BODY_BYTES {
        return Err(anyhow!("local request body too large"));
    }
    Ok(())
}

fn validate_local_request_secret(secret: &str) -> Result<()> {
    if secret.trim().is_empty() || secret.len() > 4 * 1024 {
        return Err(anyhow!("invalid local request secret"));
    }
    Ok(())
}

fn validate_local_request_timestamp(timestamp: i64) -> Result<()> {
    if timestamp < 0 || timestamp.to_string().len() > 20 {
        return Err(anyhow!("invalid local request timestamp"));
    }
    Ok(())
}

fn parse_local_request_timestamp(raw: &str) -> Result<i64> {
    if raw.is_empty() || raw.len() > 20 || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(anyhow!("invalid local request timestamp"));
    }
    let timestamp = raw
        .parse::<i64>()
        .map_err(|_| anyhow!("invalid local request timestamp"))?;
    validate_local_request_timestamp(timestamp)?;
    if raw != timestamp.to_string() {
        return Err(anyhow!("invalid local request timestamp"));
    }
    Ok(timestamp)
}

fn validate_local_request_nonce(nonce: &str) -> Result<()> {
    if nonce.len() != 32
        || !nonce
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(anyhow!("invalid local request nonce"));
    }
    Ok(())
}

fn sha256_hex(input: &[u8]) -> String {
    let mut output = String::with_capacity(64);
    for byte in sha256(input) {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

fn constant_time_eq(expected: &[u8], actual: &[u8]) -> bool {
    let mut difference = expected.len() ^ actual.len();
    let compared = expected.len().max(actual.len());
    for index in 0..compared {
        let left = expected.get(index).copied().unwrap_or_default();
        let right = actual.get(index).copied().unwrap_or_default();
        difference |= usize::from(left ^ right);
    }
    difference == 0
}

pub fn sign_jwt(claims: &AuthClaims, secret: &str) -> Result<String> {
    let header = json!({
        "alg": "HS256",
        "typ": "JWT",
    });
    let header = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header)?);
    let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(claims)?);
    let signing_input = format!("{header}.{payload}");
    let signature = sign_bytes(signing_input.as_bytes(), secret)?;
    Ok(format!(
        "{signing_input}.{}",
        URL_SAFE_NO_PAD.encode(signature)
    ))
}

pub fn verify_jwt(token: &str, secret: &str) -> Result<AuthClaims> {
    let mut parts = token.split('.');
    let header = parts.next().ok_or_else(|| anyhow!("missing header"))?;
    let payload = parts.next().ok_or_else(|| anyhow!("missing payload"))?;
    let signature = parts.next().ok_or_else(|| anyhow!("missing signature"))?;
    if parts.next().is_some() {
        return Err(anyhow!("too many jwt segments"));
    }

    let signing_input = format!("{header}.{payload}");
    let expected_signature = sign_bytes(signing_input.as_bytes(), secret)?;
    let actual_signature = URL_SAFE_NO_PAD
        .decode(signature)
        .map_err(|_| anyhow!("invalid signature encoding"))?;
    if expected_signature != actual_signature {
        return Err(anyhow!("invalid signature"));
    }

    let claims: AuthClaims = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(payload)
            .map_err(|_| anyhow!("invalid payload encoding"))?,
    )?;

    let now = now_epoch_seconds();
    if claims.exp < now {
        return Err(anyhow!("token expired"));
    }
    if claims.iss != "heiwa-core" {
        return Err(anyhow!("invalid issuer"));
    }
    if claims.aud != "heiwa" {
        return Err(anyhow!("invalid audience"));
    }

    Ok(claims)
}

pub fn extract_auth_subject(
    cookie_header: Option<&str>,
    authorization_header: Option<&str>,
    cfg: &RuntimeConfig,
) -> Result<AuthSubject> {
    if let Some(token) = authorization_header.and_then(parse_bearer_token) {
        if token == cfg.machine_auth_token {
            return Ok(AuthSubject::Operator);
        }
        if !cfg.jwt_signing_secret.is_empty() {
            if let Ok(claims) = verify_jwt(token, &cfg.jwt_signing_secret) {
                return Ok(AuthSubject::User(claims));
            }
        }
    }

    if let Some(token) = cookie_header.and_then(parse_session_cookie) {
        if !cfg.jwt_signing_secret.is_empty() {
            if let Ok(claims) = verify_jwt(token, &cfg.jwt_signing_secret) {
                return Ok(AuthSubject::User(claims));
            }
        }
    }

    Err(anyhow!("unauthorized"))
}

pub fn auth_me_payload(subject: &AuthSubject) -> Value {
    match subject {
        AuthSubject::Operator => json!({
            "authenticated": true,
            "subject": {
                "kind": "operator",
                "owner_id": "local-operator",
                "principal_id": "local-operator",
            }
        }),
        AuthSubject::User(claims) => json!({
            "authenticated": true,
            "subject": {
                "kind": "user",
                "sub": claims.sub,
                "owner_id": claims.owner_id,
                "principal_id": claims.principal_id,
                "username": claims.username,
                "discord_user_id": claims.discord_user_id,
            }
        }),
    }
}

pub fn auth_me_from_headers(
    cookie_header: Option<&str>,
    authorization_header: Option<&str>,
    cfg: &RuntimeConfig,
) -> (u16, Value) {
    match extract_auth_subject(cookie_header, authorization_header, cfg) {
        Ok(subject) => (200, auth_me_payload(&subject)),
        Err(_) => (
            401,
            json!({"authenticated": false, "error": "unauthorized"}),
        ),
    }
}

pub async fn auth_me_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let cookie = headers.get("cookie").and_then(|value| value.to_str().ok());
    let authorization = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok());
    let (status, payload) = auth_me_from_headers(cookie, authorization, &state.config);
    let status = if status == 200 {
        axum::http::StatusCode::OK
    } else {
        axum::http::StatusCode::UNAUTHORIZED
    };
    (status, Json(payload)).into_response()
}

fn parse_bearer_token(header: &str) -> Option<&str> {
    let trimmed = header.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(token) = trimmed.strip_prefix("Bearer ") {
        let token = token.trim();
        if token.is_empty() {
            None
        } else {
            Some(token)
        }
    } else {
        Some(trimmed)
    }
}

fn parse_session_cookie(cookie_header: &str) -> Option<&str> {
    cookie_header
        .split(';')
        .map(str::trim)
        .find_map(|segment| segment.strip_prefix("heiwa_session="))
        .filter(|value| !value.is_empty())
}

fn sign_bytes(input: &[u8], secret: &str) -> Result<Vec<u8>> {
    if secret.is_empty() {
        return Err(anyhow!("invalid secret"));
    }
    Ok(hmac_sha256(secret.as_bytes(), input).to_vec())
}

fn now_epoch_seconds() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    const BLOCK_SIZE: usize = 64;

    let mut working_key = [0_u8; BLOCK_SIZE];
    if key.len() > BLOCK_SIZE {
        working_key[..32].copy_from_slice(&sha256(key));
    } else {
        working_key[..key.len()].copy_from_slice(key);
    }

    let mut inner_pad = [0_u8; BLOCK_SIZE];
    let mut outer_pad = [0_u8; BLOCK_SIZE];
    for index in 0..BLOCK_SIZE {
        inner_pad[index] = working_key[index] ^ 0x36;
        outer_pad[index] = working_key[index] ^ 0x5c;
    }

    let mut inner_input = Vec::with_capacity(BLOCK_SIZE + message.len());
    inner_input.extend_from_slice(&inner_pad);
    inner_input.extend_from_slice(message);
    let inner_hash = sha256(&inner_input);

    let mut outer_input = Vec::with_capacity(BLOCK_SIZE + inner_hash.len());
    outer_input.extend_from_slice(&outer_pad);
    outer_input.extend_from_slice(&inner_hash);
    sha256(&outer_input)
}

fn sha256(input: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let mut h = [
        0x6a09e667_u32,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19,
    ];

    let mut padded = input.to_vec();
    let bit_len = (padded.len() as u64) * 8;
    padded.push(0x80);
    while (padded.len() % 64) != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    let mut w = [0_u32; 64];
    for chunk in padded.chunks_exact(64) {
        for (index, word) in chunk.chunks_exact(4).enumerate().take(16) {
            w[index] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for index in 16..64 {
            let s0 = w[index - 15].rotate_right(7)
                ^ w[index - 15].rotate_right(18)
                ^ (w[index - 15] >> 3);
            let s1 = w[index - 2].rotate_right(17)
                ^ w[index - 2].rotate_right(19)
                ^ (w[index - 2] >> 10);
            w[index] = w[index - 16]
                .wrapping_add(s0)
                .wrapping_add(w[index - 7])
                .wrapping_add(s1);
        }

        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut hh = h[7];

        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[index])
                .wrapping_add(w[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut digest = [0_u8; 32];
    for (index, word) in h.iter().enumerate() {
        digest[index * 4..(index + 1) * 4].copy_from_slice(&word.to_be_bytes());
    }
    digest
}
