//! Which line of development an installation follows.
//!
//! `main` is the public channel: GitHub Releases, which are tagged on `main`.
//! `dev` builds `origin/dev` from a local heiwa-universe checkout, for operators
//! who develop Heiwa and want the integration branch on their own machine.
//!
//! The choice and the last installed build live in `<runtime root>/channel.json`.
//! A missing file means `main`, so every existing and public install keeps
//! following releases without writing anything.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: &str = "heiwa_update_channel_v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    #[default]
    Main,
    Dev,
}

impl Channel {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "main" => Some(Self::Main),
            "dev" => Some(Self::Dev),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Dev => "dev",
        }
    }
}

/// What `heiwa app update` last put in the install root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledBuild {
    /// `main`, `dev`, or `checkout` for an explicit `--source checkout` install.
    pub source: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    pub installed_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelState {
    pub schema_version: String,
    pub channel: Channel,
    /// The checkout whose `origin/dev` the dev channel builds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_repo: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed: Option<InstalledBuild>,
}

impl Default for ChannelState {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION.to_string(),
            channel: Channel::Main,
            source_repo: None,
            installed: None,
        }
    }
}

impl ChannelState {
    /// Whether the installed build came from somewhere other than `source`, so
    /// an equal version number does not prove the right bytes are installed.
    pub fn installed_from_elsewhere(&self, source: &str) -> bool {
        self.installed
            .as_ref()
            .is_some_and(|installed| installed.source != source)
    }
}

pub fn state_path(root: &Path) -> PathBuf {
    root.join("channel.json")
}

/// Read the channel state. A missing file is the default `main` channel; an
/// unreadable or foreign file is an error, because callers of this function
/// are about to change what is installed.
pub fn load(root: &Path) -> Result<ChannelState> {
    let path = state_path(root);
    let raw = match fs::read(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ChannelState::default())
        }
        Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
    };
    let state: ChannelState =
        serde_json::from_slice(&raw).with_context(|| format!("parse {}", path.display()))?;
    if state.schema_version != SCHEMA_VERSION {
        bail!(
            "{} has unsupported schema {}",
            path.display(),
            state.schema_version
        );
    }
    Ok(state)
}

/// The configured channel for read-only callers such as the desktop update
/// offer. An unreadable file falls back to `main`, the public default.
pub fn configured(root: &Path) -> Channel {
    load(root).map(|state| state.channel).unwrap_or_default()
}

pub fn save(root: &Path, state: &ChannelState) -> Result<()> {
    fs::create_dir_all(root).with_context(|| format!("create {}", root.display()))?;
    let path = state_path(root);
    let mut staged = tempfile::NamedTempFile::new_in(root)?;
    serde_json::to_writer_pretty(&mut staged, state)?;
    staged.write_all(b"\n")?;
    staged.as_file().sync_all()?;
    staged
        .persist(&path)
        .map_err(|error| error.error)
        .with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(source: &str, commit: Option<&str>) -> InstalledBuild {
        InstalledBuild {
            source: source.to_string(),
            version: "heiwa 0.3.0".to_string(),
            commit: commit.map(str::to_string),
            installed_at: "2026-09-13T08:00:00Z".to_string(),
            receipt: None,
        }
    }

    #[test]
    fn a_missing_file_follows_main_without_writing() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(load(root.path()).unwrap(), ChannelState::default());
        assert_eq!(configured(root.path()), Channel::Main);
        assert!(!state_path(root.path()).exists());
    }

    #[test]
    fn the_choice_and_installed_build_survive_a_round_trip() {
        let root = tempfile::tempdir().unwrap();
        let state = ChannelState {
            channel: Channel::Dev,
            source_repo: Some(PathBuf::from("/src/heiwa-universe")),
            installed: Some(build("dev", Some("0123456789abcdef"))),
            ..ChannelState::default()
        };
        save(root.path(), &state).unwrap();
        assert_eq!(load(root.path()).unwrap(), state);
        assert_eq!(configured(root.path()), Channel::Dev);
        let raw = fs::read_to_string(state_path(root.path())).unwrap();
        assert!(raw.contains("\"channel\": \"dev\""), "{raw}");
    }

    #[test]
    fn a_corrupt_file_blocks_installs_but_offers_stay_on_main() {
        let root = tempfile::tempdir().unwrap();
        fs::write(state_path(root.path()), b"{not json").unwrap();
        assert!(load(root.path()).is_err());
        assert_eq!(configured(root.path()), Channel::Main);

        fs::write(
            state_path(root.path()),
            br#"{"schema_version":"heiwa_update_channel_v9","channel":"dev"}"#,
        )
        .unwrap();
        assert!(load(root.path())
            .unwrap_err()
            .to_string()
            .contains("unsupported schema"));
    }

    #[test]
    fn only_the_two_branch_names_are_channels() {
        assert_eq!(Channel::parse("main"), Some(Channel::Main));
        assert_eq!(Channel::parse("dev"), Some(Channel::Dev));
        for other in ["stable", "Dev", " dev", "nightly", ""] {
            assert_eq!(Channel::parse(other), None, "{other:?}");
        }
    }

    #[test]
    fn an_equal_version_from_another_source_is_not_already_installed() {
        let mut state = ChannelState::default();
        assert!(!state.installed_from_elsewhere("main"));
        state.installed = Some(build("dev", Some("abc")));
        assert!(state.installed_from_elsewhere("main"));
        assert!(!state.installed_from_elsewhere("dev"));
    }
}
