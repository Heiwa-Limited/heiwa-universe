use serde::Deserialize;
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct HeiwaPaths {
    pub home_dir: PathBuf,
    pub state_dir: PathBuf,
    pub sessions_dir: PathBuf,
    pub config_path: PathBuf,
}

impl HeiwaPaths {
    pub fn resolve() -> Self {
        let home_dir = env::var("HOME")
            .or_else(|_| env::var("USERPROFILE"))
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."));
        let state_dir = home_dir.join(".heiwa");
        Self {
            sessions_dir: state_dir.join("sessions"),
            config_path: state_dir.join("config.toml"),
            home_dir,
            state_dir,
        }
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
            .unwrap_or_else(|| paths.state_dir.join("state").join("memory.sqlite3")),
        lance_path: env::var("HEIWA_EMBED_LANCE_PATH")
            .ok()
            .map(PathBuf::from)
            .or_else(|| file.embedding.lance_path.clone())
            .unwrap_or_else(|| paths.state_dir.join("state").join("lance")),
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
