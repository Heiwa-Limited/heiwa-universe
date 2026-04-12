use std::env;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct RuntimePaths {
    root: PathBuf,
}

impl RuntimePaths {
    pub fn discover() -> Self {
        let home = env::var("HOME")
            .or_else(|_| env::var("USERPROFILE"))
            .expect("HOME or USERPROFILE must be set");
        Self::from_home(PathBuf::from(home))
    }

    pub fn from_home(home: PathBuf) -> Self {
        Self {
            root: home.join(".heiwa"),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn config(&self) -> PathBuf {
        self.root.join("config.toml")
    }

    pub fn machine(&self) -> PathBuf {
        self.root.join("machine.json")
    }

    pub fn provider_registry(&self) -> PathBuf {
        self.root.join("providers").join("registry.json")
    }

    pub fn legacy_connections(&self) -> PathBuf {
        self.root.join("providers").join("legacy_connections.json")
    }

    pub fn identity(&self) -> PathBuf {
        self.root.join("state").join("identity.json")
    }

    pub fn connection(&self) -> PathBuf {
        self.root.join("state").join("connection.json")
    }

    pub fn inventory(&self) -> PathBuf {
        self.root.join("models").join("inventory.json")
    }

    pub fn runtime_policy(&self) -> PathBuf {
        self.root.join("policies").join("runtime.toml")
    }

    pub fn concise_mode(&self) -> PathBuf {
        self.root.join("modes").join("concise").join("MODE.md")
    }
}
