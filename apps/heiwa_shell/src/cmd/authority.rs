//! Typed decision authority and inspection over the runtime's private key.
//! Credential possession is not a same-UID worker containment boundary.
use anyhow::{anyhow, Result};
use heiwa_core::{auth::AuthSubject, config::RuntimeConfig, integrity};
use serde_json::{json, Value};
use std::path::Path;

pub(crate) use integrity::DecisionIntegrity as Integrity;

// Never expose or print the key through Debug or decision evidence.
#[derive(Clone)]
pub(crate) struct DecisionKey {
    secret: String,
}

impl DecisionKey {
    pub(crate) fn load(root: &Path) -> Result<Self> {
        let secret = heiwa_core::config::read_runtime_secret_file(root, "machine_auth_token")
            .ok_or_else(|| {
                anyhow!("a private runtime credential file is required for approval decisions")
            })?;
        Ok(Self { secret })
    }

    pub(crate) fn load_default() -> Result<Self> {
        let paths = heiwa_config::HeiwaPaths::try_resolve()
            .ok_or_else(|| anyhow!("the Heiwa runtime root is unavailable"))?;
        Self::load(&paths.runtime_root)
    }
}

pub(crate) struct Principal {
    key: DecisionKey,
    authenticated_by: &'static str,
    client_label: String,
}

impl Principal {
    pub(crate) fn cli() -> Result<Self> {
        Ok(Self {
            key: DecisionKey::load_default()?,
            authenticated_by: "runtime_credential_file",
            client_label: "heiwa-cli".to_string(),
        })
    }

    pub(crate) fn runtime_request(
        subject: &AuthSubject,
        config: &RuntimeConfig,
        client_label: Option<&str>,
    ) -> Result<Self> {
        if !matches!(subject, AuthSubject::Operator) {
            return Err(anyhow!("approval decisions require operator authority"));
        }
        Self::runtime_with_key(subject, config, client_label, DecisionKey::load_default()?)
    }

    fn runtime_with_key(
        subject: &AuthSubject,
        config: &RuntimeConfig,
        client_label: Option<&str>,
        key: DecisionKey,
    ) -> Result<Self> {
        if !matches!(subject, AuthSubject::Operator) {
            return Err(anyhow!("approval decisions require operator authority"));
        }
        // Compare via the existing constant-time HMAC verifier, without
        // exporting the secret or duplicating authentication mechanics.
        let witness = json!({"domain": "runtime-decision-principal-v1"});
        let effective_tag = integrity::decision_tag(&config.machine_auth_token, &witness)
            .ok_or_else(|| anyhow!("runtime credential is unavailable"))?;
        if !integrity::verify_decision_tag(&key.secret, &witness, &effective_tag) {
            return Err(anyhow!(
                "effective runtime credential does not match the private credential file"
            ));
        }
        Ok(Self {
            key,
            authenticated_by: "runtime_request",
            client_label: client_label
                .map(bounded_label)
                .filter(|label| !label.is_empty())
                .unwrap_or_else(|| "unlabeled".to_string()),
        })
    }

    pub(crate) fn key(&self) -> &DecisionKey {
        &self.key
    }
    pub(crate) fn display_label(&self) -> &str {
        &self.client_label
    }
    pub(crate) fn to_json(&self) -> Value {
        json!({"authenticated_by": self.authenticated_by, "client_label": self.client_label, "client_label_source": "self_reported"})
    }
}

fn bounded_label(label: &str) -> String {
    label
        .chars()
        .filter(|character| character.is_ascii_graphic() || *character == ' ')
        .take(64)
        .collect()
}

pub(crate) fn seal(key: &DecisionKey, record: &mut Value) -> Result<()> {
    integrity::seal_decision(&key.secret, record)
}

pub(crate) fn classify(key: Option<&DecisionKey>, stem: &str, record: &Value) -> Integrity {
    integrity::classify_decision(key.map(|key| key.secret.as_str()), stem, record)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn keyed_root() -> (tempfile::TempDir, DecisionKey) {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("secrets");
        std::fs::create_dir(&directory).unwrap();
        let file = directory.join("machine_auth_token");
        std::fs::write(&file, "test-decision-credential").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        let key = DecisionKey::load(root.path()).unwrap();
        (root, key)
    }

    fn config(token: &str) -> RuntimeConfig {
        RuntimeConfig {
            port: 1,
            state_backend: String::new(),
            log_level: String::new(),
            machine_auth_token: token.to_string(),
            jwt_signing_secret: String::new(),
            node_id: String::new(),
            model_tiers_seed_path: String::new(),
            ai_router_seed_path: String::new(),
        }
    }

    #[test]
    fn runtime_principal_requires_operator_and_matching_file_key() {
        let (_root, key) = keyed_root();
        let claims = heiwa_core::auth::AuthClaims {
            sub: "user".into(),
            owner_id: "user".into(),
            principal_id: "user".into(),
            username: None,
            discord_user_id: None,
            iat: 1,
            exp: 2,
            iss: "heiwa".into(),
            aud: "heiwa".into(),
        };
        assert!(Principal::runtime_with_key(
            &AuthSubject::User(claims),
            &config("test-decision-credential"),
            None,
            key.clone()
        )
        .is_err());
        assert!(Principal::runtime_with_key(
            &AuthSubject::Operator,
            &config("env-override"),
            None,
            key.clone()
        )
        .is_err());
        assert!(Principal::runtime_with_key(
            &AuthSubject::Operator,
            &config(""),
            None,
            key.clone()
        )
        .is_err());
        let principal = Principal::runtime_with_key(
            &AuthSubject::Operator,
            &config("test-decision-credential"),
            Some("Heiwa.app\n"),
            key,
        )
        .unwrap();
        assert_eq!(principal.to_json()["authenticated_by"], "runtime_request");
        assert_eq!(principal.to_json()["client_label"], "Heiwa.app");
        assert_eq!(principal.to_json()["client_label_source"], "self_reported");
    }

    #[test]
    fn secret_is_not_loaded_from_an_empty_root_and_labels_are_bounded() {
        assert!(DecisionKey::load(tempfile::tempdir().unwrap().path()).is_err());
        assert_eq!(bounded_label(&"x".repeat(500)).len(), 64);
    }
}
