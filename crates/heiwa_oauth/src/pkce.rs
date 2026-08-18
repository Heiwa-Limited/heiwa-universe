//! PKCE (RFC 7636) verifier and challenge.
//!
//! Installed applications are public clients: the binary ships to users, so any
//! "client secret" in it is readable by anyone who has the app. PKCE is what
//! actually protects the exchange — the authorization code is useless without
//! the verifier, which never leaves this process.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use rand::Rng;
use sha2::{Digest, Sha256};

/// Unreserved characters RFC 7636 permits in a verifier.
const VERIFIER_ALPHABET: &[u8] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";

/// RFC 7636 allows 43..=128. Take the maximum: the verifier is never shown to a
/// user or typed, so there is no cost to the extra entropy.
const VERIFIER_LEN: usize = 128;

#[derive(Debug, Clone)]
pub struct Pkce {
    verifier: String,
}

impl Pkce {
    /// Generate a verifier from the OS CSPRNG.
    pub fn generate() -> Self {
        let mut rng = rand::thread_rng();
        let verifier = (0..VERIFIER_LEN)
            .map(|_| VERIFIER_ALPHABET[rng.gen_range(0..VERIFIER_ALPHABET.len())] as char)
            .collect();
        Self { verifier }
    }

    /// Reconstruct from a stored verifier. The caller owns whether that storage
    /// was safe; nothing here re-validates entropy it did not create.
    pub fn from_verifier(verifier: impl Into<String>) -> Self {
        Self {
            verifier: verifier.into(),
        }
    }

    pub fn verifier(&self) -> &str {
        &self.verifier
    }

    /// BASE64URL(SHA256(verifier)), unpadded — the `S256` method.
    pub fn challenge(&self) -> String {
        let digest = Sha256::digest(self.verifier.as_bytes());
        URL_SAFE_NO_PAD.encode(digest)
    }

    /// Always `S256`. `plain` is permitted by the RFC and is pointless: it
    /// sends the verifier itself, which is the thing PKCE exists to withhold.
    pub fn method(&self) -> &'static str {
        "S256"
    }
}

impl Default for Pkce {
    fn default() -> Self {
        Self::generate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifier_satisfies_rfc7636_length_and_alphabet() {
        let pkce = Pkce::generate();
        let len = pkce.verifier().len();
        assert!(
            (43..=128).contains(&len),
            "verifier length {len} out of range"
        );
        for byte in pkce.verifier().bytes() {
            assert!(
                VERIFIER_ALPHABET.contains(&byte),
                "character {byte:?} is outside the permitted alphabet"
            );
        }
    }

    #[test]
    fn challenge_matches_the_rfc7636_worked_example() {
        // RFC 7636 Appendix B fixes this pair, so it catches an encoder that
        // pads, uses standard base64, or hashes the wrong bytes.
        let pkce = Pkce::from_verifier("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk");
        assert_eq!(
            pkce.challenge(),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn challenge_is_unpadded_urlsafe() {
        let challenge = Pkce::generate().challenge();
        assert!(!challenge.contains('='), "challenge must not be padded");
        assert!(
            !challenge.contains('+') && !challenge.contains('/'),
            "must be url-safe"
        );
        assert_eq!(
            challenge.len(),
            43,
            "SHA-256 base64url unpadded is 43 chars"
        );
    }

    #[test]
    fn generated_verifiers_differ() {
        assert_ne!(Pkce::generate().verifier(), Pkce::generate().verifier());
    }
}
