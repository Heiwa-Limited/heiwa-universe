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

#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    pub enabled: bool,
    pub model: String,
    pub ollama_url: Option<String>,
    pub sqlite_path: PathBuf,
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
    sqlite_path: Option<PathBuf>,
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
        sqlite_path: env::var("HEIWA_EMBED_SQLITE_PATH")
            .ok()
            .map(PathBuf::from)
            .or_else(|| file.embedding.sqlite_path.clone())
            .unwrap_or_else(|| paths.state_dir.join("state").join("memory.sqlite3")),
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
    }
}
