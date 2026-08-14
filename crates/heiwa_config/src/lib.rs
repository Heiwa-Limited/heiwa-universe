use serde::Deserialize;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;

/// ConfigRoot: the single resolver for Heiwa's per-user state layout.
///
/// This is the only code permitted to know where user state lives. Every
/// crate and app resolves the runtime root, state dir, evidence dir, and
/// session dir through this struct instead of reading `HOME`/`HEIWA_*`
/// env vars or joining `.heiwa` by hand.
///
/// Layout and overrides:
/// - `runtime_root`  = `HEIWA_HOME`          | `<home>/.heiwa`
/// - `state_dir`     = `HEIWA_STATE_DIR`     | `<runtime_root>/state`
/// - `evidence_dir`  = `HEIWA_EVIDENCE_DIR`  | `<runtime_root>/evidence`
/// - `sessions_dir`  =                         `<runtime_root>/sessions`
/// - `config_path`   =                         `<runtime_root>/config.toml`
///
/// `HOME` wins over the platform home lookup so hermetic tests and sandboxed
/// runs can redirect all state with one env var; on Windows the platform
/// lookup ignores `HOME`, which previously let sandboxed state leak into the
/// real user profile.
#[derive(Debug, Clone)]
pub struct HeiwaPaths {
    pub home_dir: PathBuf,
    /// The per-user runtime root (`~/.heiwa` unless `HEIWA_HOME` overrides).
    pub runtime_root: PathBuf,
    /// Hot operational state (`<runtime_root>/state` unless `HEIWA_STATE_DIR`).
    pub state_dir: PathBuf,
    /// True when `HEIWA_STATE_DIR` explicitly relocated `state_dir`. Callers
    /// that historically anchored files at the runtime root (quota's
    /// `state.db`) use this to preserve their on-disk contract.
    pub state_dir_is_override: bool,
    /// Evidence journal root (`<runtime_root>/evidence` unless
    /// `HEIWA_EVIDENCE_DIR`).
    pub evidence_dir: PathBuf,
    pub sessions_dir: PathBuf,
    pub config_path: PathBuf,
}

impl HeiwaPaths {
    /// Resolve from the process environment, falling back to a
    /// cwd-relative root when no home can be found.
    ///
    /// Callers that would rather fail than write to an attacker-writable
    /// working directory must use [`try_resolve`](Self::try_resolve).
    pub fn resolve() -> Self {
        Self::resolve_from(|key| env::var_os(key), dirs::home_dir())
    }

    /// Resolve only when a real root exists.
    ///
    /// Returns `None` when neither `HEIWA_HOME` nor any home directory is
    /// resolvable — a containerized run with no `HOME` and no passwd entry,
    /// for instance. Anything that reads secrets or writes the evidence
    /// journal must use this: silently falling back to `./.heiwa` would read
    /// credentials from, and append receipts to, whatever directory the
    /// process happens to be started in.
    pub fn try_resolve() -> Option<Self> {
        Self::try_resolve_from(|key| env::var_os(key), dirs::home_dir())
    }

    /// Pure form of [`try_resolve`](Self::try_resolve).
    pub fn try_resolve_from(
        env: impl Fn(&str) -> Option<OsString>,
        platform_home: Option<PathBuf>,
    ) -> Option<Self> {
        let has_root = ["HEIWA_HOME", "HOME", "USERPROFILE"]
            .iter()
            .any(|key| env(key).is_some_and(|value| !value.is_empty()))
            || platform_home.is_some();
        has_root.then(|| Self::resolve_from(env, platform_home))
    }

    /// Pure resolution from an injected environment, so precedence is
    /// testable without touching process-global env (parallel test threads
    /// race on `set_var`).
    pub fn resolve_from(
        env: impl Fn(&str) -> Option<OsString>,
        platform_home: Option<PathBuf>,
    ) -> Self {
        let non_empty = |key: &str| {
            env(key)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        };

        let home_dir = non_empty("HOME")
            .or_else(|| non_empty("USERPROFILE"))
            .or(platform_home)
            .unwrap_or_else(|| PathBuf::from("."));

        let runtime_root = non_empty("HEIWA_HOME").unwrap_or_else(|| home_dir.join(".heiwa"));

        let state_override = non_empty("HEIWA_STATE_DIR");
        let state_dir_is_override = state_override.is_some();
        let state_dir = state_override.unwrap_or_else(|| runtime_root.join("state"));

        let evidence_dir =
            non_empty("HEIWA_EVIDENCE_DIR").unwrap_or_else(|| runtime_root.join("evidence"));

        Self {
            sessions_dir: runtime_root.join("sessions"),
            config_path: runtime_root.join("config.toml"),
            home_dir,
            runtime_root,
            state_dir,
            state_dir_is_override,
            evidence_dir,
        }
    }

    /// Redacted receipts plane: always under the state dir.
    pub fn receipts_dir(&self) -> PathBuf {
        self.state_dir.join("evidence")
    }

    /// First-run creation of the per-user layout. Idempotent.
    pub fn ensure(&self) -> std::io::Result<()> {
        for dir in [
            &self.runtime_root,
            &self.state_dir,
            &self.sessions_dir,
            &self.evidence_dir,
        ] {
            fs::create_dir_all(dir)?;
        }
        Ok(())
    }
}

/// Vector store backing the embedding index. The index is derived and
/// rebuildable from transcript truth; switching backends is a re-embed, not
/// a migration of authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EmbedBackend {
    #[default]
    Sqlite,
    Lance,
}

impl EmbedBackend {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "sqlite" => Some(Self::Sqlite),
            "lance" => Some(Self::Lance),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    pub enabled: bool,
    pub model: String,
    pub ollama_url: Option<String>,
    pub backend: EmbedBackend,
    pub sqlite_path: PathBuf,
    pub lance_path: PathBuf,
    /// Embedding dimensionality; fixed per store (1024 for
    /// qwen3-embedding:0.6b).
    pub dim: usize,
    pub request_timeout_ms: u64,
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub paths: HeiwaPaths,
    pub embedding: EmbeddingConfig,
}

#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    #[serde(default)]
    embedding: EmbeddingFileConfig,
}

#[derive(Debug, Default, Deserialize)]
struct EmbeddingFileConfig {
    enabled: Option<bool>,
    model: Option<String>,
    ollama_url: Option<String>,
    backend: Option<String>,
    sqlite_path: Option<PathBuf>,
    lance_path: Option<PathBuf>,
    dim: Option<usize>,
    request_timeout_ms: Option<u64>,
}

pub fn load() -> AppConfig {
    let paths = HeiwaPaths::resolve();
    let file = load_file_config(&paths.config_path);

    let default_ollama = match env::var("HEIWA_RUNTIME")
        .unwrap_or_else(|_| "local".to_string())
        .to_ascii_lowercase()
        .as_str()
    {
        "remote" | "cloud" => None,
        _ => Some("http://127.0.0.1:11434".to_string()),
    };

    let embedding = EmbeddingConfig {
        enabled: env_bool("HEIWA_EMBED_SYNC_ON_APPEND")
            .unwrap_or_else(|| file.embedding.enabled.unwrap_or(false)),
        model: env::var("HEIWA_EMBED_MODEL")
            .ok()
            .or_else(|| file.embedding.model.clone())
            .unwrap_or_else(|| "qwen3-embedding:0.6b".to_string()),
        ollama_url: env::var("HEIWA_OLLAMA_URL")
            .ok()
            .or_else(|| file.embedding.ollama_url.clone())
            .or(default_ollama),
        backend: env::var("HEIWA_EMBED_BACKEND")
            .ok()
            .as_deref()
            .or(file.embedding.backend.as_deref())
            .and_then(EmbedBackend::parse)
            .unwrap_or_default(),
        sqlite_path: env::var("HEIWA_EMBED_SQLITE_PATH")
            .ok()
            .map(PathBuf::from)
            .or_else(|| file.embedding.sqlite_path.clone())
            .unwrap_or_else(|| paths.state_dir.join("memory.sqlite3")),
        lance_path: env::var("HEIWA_EMBED_LANCE_PATH")
            .ok()
            .map(PathBuf::from)
            .or_else(|| file.embedding.lance_path.clone())
            .unwrap_or_else(|| paths.state_dir.join("lance")),
        dim: env::var("HEIWA_EMBED_DIM")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .or(file.embedding.dim)
            .unwrap_or(1024),
        request_timeout_ms: env::var("HEIWA_EMBED_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .or(file.embedding.request_timeout_ms)
            .unwrap_or(1_500),
    };

    AppConfig { paths, embedding }
}

fn load_file_config(path: &PathBuf) -> FileConfig {
    let Ok(raw) = fs::read_to_string(path) else {
        return FileConfig::default();
    };
    toml::from_str(&raw).unwrap_or_default()
}

fn env_bool(key: &str) -> Option<bool> {
    let value = env::var(key).ok()?;
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod config_root_tests {
    use super::*;
    use std::ffi::OsString;

    fn env_of<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |key: &str| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| OsString::from(v))
        }
    }

    #[test]
    fn defaults_derive_every_path_from_platform_home() {
        let paths = HeiwaPaths::resolve_from(env_of(&[]), Some(PathBuf::from("/platform/home")));
        assert_eq!(paths.home_dir, PathBuf::from("/platform/home"));
        assert_eq!(paths.runtime_root, PathBuf::from("/platform/home/.heiwa"));
        assert_eq!(
            paths.state_dir,
            PathBuf::from("/platform/home/.heiwa/state")
        );
        assert!(!paths.state_dir_is_override);
        assert_eq!(
            paths.evidence_dir,
            PathBuf::from("/platform/home/.heiwa/evidence")
        );
        assert_eq!(
            paths.sessions_dir,
            PathBuf::from("/platform/home/.heiwa/sessions")
        );
        assert_eq!(
            paths.config_path,
            PathBuf::from("/platform/home/.heiwa/config.toml")
        );
    }

    #[test]
    fn home_env_wins_over_platform_home_for_hermetic_tests() {
        let paths = HeiwaPaths::resolve_from(
            env_of(&[("HOME", "/sandbox/home")]),
            Some(PathBuf::from("/platform/home")),
        );
        assert_eq!(paths.runtime_root, PathBuf::from("/sandbox/home/.heiwa"));
    }

    #[test]
    fn userprofile_used_when_home_absent() {
        let paths = HeiwaPaths::resolve_from(env_of(&[("USERPROFILE", "/win/profile")]), None);
        assert_eq!(paths.runtime_root, PathBuf::from("/win/profile/.heiwa"));
    }

    #[test]
    fn heiwa_home_overrides_runtime_root_and_children_follow() {
        let paths = HeiwaPaths::resolve_from(
            env_of(&[("HOME", "/h"), ("HEIWA_HOME", "/custom/root")]),
            None,
        );
        assert_eq!(paths.runtime_root, PathBuf::from("/custom/root"));
        assert_eq!(paths.state_dir, PathBuf::from("/custom/root/state"));
        assert_eq!(paths.evidence_dir, PathBuf::from("/custom/root/evidence"));
        assert_eq!(paths.sessions_dir, PathBuf::from("/custom/root/sessions"));
        assert_eq!(paths.config_path, PathBuf::from("/custom/root/config.toml"));
    }

    #[test]
    fn state_dir_env_replaces_state_root_and_flags_override() {
        let paths = HeiwaPaths::resolve_from(
            env_of(&[("HOME", "/h"), ("HEIWA_STATE_DIR", "/isolated/state")]),
            None,
        );
        assert_eq!(paths.state_dir, PathBuf::from("/isolated/state"));
        assert!(paths.state_dir_is_override);
        assert_eq!(paths.runtime_root, PathBuf::from("/h/.heiwa"));
    }

    #[test]
    fn evidence_dir_env_replaces_evidence_root() {
        let paths = HeiwaPaths::resolve_from(
            env_of(&[("HOME", "/h"), ("HEIWA_EVIDENCE_DIR", "/isolated/evidence")]),
            None,
        );
        assert_eq!(paths.evidence_dir, PathBuf::from("/isolated/evidence"));
    }

    #[test]
    fn empty_env_values_are_ignored() {
        let paths = HeiwaPaths::resolve_from(
            env_of(&[
                ("HOME", "/h"),
                ("HEIWA_HOME", ""),
                ("HEIWA_STATE_DIR", ""),
                ("HEIWA_EVIDENCE_DIR", ""),
            ]),
            None,
        );
        assert_eq!(paths.runtime_root, PathBuf::from("/h/.heiwa"));
        assert_eq!(paths.state_dir, PathBuf::from("/h/.heiwa/state"));
        assert!(!paths.state_dir_is_override);
        assert_eq!(paths.evidence_dir, PathBuf::from("/h/.heiwa/evidence"));
    }

    #[test]
    fn no_home_anywhere_falls_back_to_cwd_relative() {
        let paths = HeiwaPaths::resolve_from(env_of(&[]), None);
        assert_eq!(paths.runtime_root, PathBuf::from("./.heiwa"));
    }

    #[test]
    fn try_resolve_refuses_to_guess_a_root_when_there_is_none() {
        // With no home at all there is no state root to guess. Callers that
        // read secrets must get None here rather than a cwd-relative path an
        // attacker who controls the working directory could populate.
        assert!(HeiwaPaths::try_resolve_from(env_of(&[]), None).is_none());
    }

    #[test]
    fn try_resolve_yields_the_same_layout_as_resolve_when_a_root_exists() {
        for env in [
            vec![("HOME", "/h")],
            vec![("USERPROFILE", "/win")],
            vec![("HEIWA_HOME", "/custom")],
        ] {
            let strict = HeiwaPaths::try_resolve_from(env_of(&env), None).expect("root");
            let lenient = HeiwaPaths::resolve_from(env_of(&env), None);
            assert_eq!(strict.runtime_root, lenient.runtime_root);
        }
        // A platform home with no env vars set still counts as a real root.
        assert!(HeiwaPaths::try_resolve_from(env_of(&[]), Some(PathBuf::from("/p"))).is_some());
    }

    #[test]
    fn try_resolve_ignores_empty_env_values() {
        assert!(
            HeiwaPaths::try_resolve_from(env_of(&[("HOME", ""), ("HEIWA_HOME", "")]), None)
                .is_none()
        );
    }

    #[test]
    fn receipts_dir_lives_under_state_dir() {
        let default = HeiwaPaths::resolve_from(env_of(&[("HOME", "/h")]), None);
        assert_eq!(
            default.receipts_dir(),
            PathBuf::from("/h/.heiwa/state/evidence")
        );
        let overridden = HeiwaPaths::resolve_from(
            env_of(&[("HOME", "/h"), ("HEIWA_STATE_DIR", "/iso/state")]),
            None,
        );
        assert_eq!(
            overridden.receipts_dir(),
            PathBuf::from("/iso/state/evidence")
        );
    }

    #[test]
    fn ensure_creates_the_per_user_layout_on_first_run() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("fresh-user");
        let paths = HeiwaPaths::resolve_from(
            env_of(&[("HEIWA_HOME", root.to_str().expect("utf8 temp path"))]),
            None,
        );
        assert!(!root.exists());
        paths.ensure().expect("ensure creates layout");
        assert!(paths.runtime_root.is_dir());
        assert!(paths.state_dir.is_dir());
        assert!(paths.sessions_dir.is_dir());
        assert!(paths.evidence_dir.is_dir());
        paths.ensure().expect("ensure is idempotent");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_loads_defaults_without_file() {
        let config = load();
        assert_eq!(config.embedding.model, "qwen3-embedding:0.6b");
        assert!(config.embedding.sqlite_path.ends_with("memory.sqlite3"));
        assert_eq!(config.embedding.backend, EmbedBackend::Sqlite);
        assert!(config.embedding.lance_path.ends_with("lance"));
        assert_eq!(config.embedding.dim, 1024);
    }

    #[test]
    fn embed_backend_parses_known_values_and_rejects_unknown() {
        assert_eq!(EmbedBackend::parse("sqlite"), Some(EmbedBackend::Sqlite));
        assert_eq!(EmbedBackend::parse("Lance"), Some(EmbedBackend::Lance));
        assert_eq!(EmbedBackend::parse("stdb"), None);
    }
}
