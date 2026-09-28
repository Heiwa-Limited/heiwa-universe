//! Local text truth for the finance plane.
//!
//! Layout under the store root (the runtime passes `<state>/finance`):
//!
//! ```text
//! snapshot.json        accounts + holdings as last reported (replaced per sync)
//! activities.jsonl     transactions, upserted by (account, id)
//! bars/<SYMBOL>.jsonl  daily bars, upserted by date
//! fx/<SERIES>.jsonl    daily FX, upserted by date
//! settings.json        user choices (base currency, benchmark, TFSA room)
//! sync.json            last sync outcome
//! ```
//!
//! Every file is owner-only (0600 in a 0700 directory) and replaced
//! atomically, so a reader never sees a half-written file.

use crate::market::{Bar, FxRate};
use crate::model::{Account, Activity, Holding};
use crate::FinanceError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const SNAPSHOT_SCHEMA: u32 = 1;

/// Accounts and holdings exactly as the last successful brokerage read
/// reported them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub schema_version: u32,
    pub synced_at: String,
    pub accounts: Vec<Account>,
    pub holdings: Vec<Holding>,
    /// Per-account `as_of` time the brokerage attached to its positions.
    #[serde(default)]
    pub positions_as_of: BTreeMap<String, String>,
}

/// The user's own TFSA room on January 1 of `year`, as CRA My Account
/// reports it. Heiwa cannot know this; the user supplies it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TfsaRoom {
    pub year: i32,
    pub room_at_start: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_base_currency")]
    pub base_currency: String,
    /// CAD-listed fund the portfolio is measured against.
    #[serde(default = "default_benchmark")]
    pub benchmark: String,
    #[serde(default)]
    pub tfsa_room: Option<TfsaRoom>,
}

fn default_base_currency() -> String {
    "CAD".into()
}

/// S&P/TSX Capped Composite, the benchmark SPIVA Canada uses for Canadian
/// equity. A measuring stick, not a recommendation; users change it.
fn default_benchmark() -> String {
    "XIC.TO".into()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            base_currency: default_base_currency(),
            benchmark: default_benchmark(),
            tfsa_room: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SourceIssue {
    pub source: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SyncCounts {
    pub accounts: usize,
    pub holdings: usize,
    pub activities_added: usize,
    pub activities_updated: usize,
    pub fx_days: usize,
    pub bar_series: usize,
    pub bars_added: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SyncStatus {
    pub last_attempt_at: Option<String>,
    pub last_success_at: Option<String>,
    /// `ok`, `partial` (some sources failed), or `error` (nothing refreshed).
    pub outcome: Option<String>,
    #[serde(default)]
    pub issues: Vec<SourceIssue>,
    #[serde(default)]
    pub counts: SyncCounts,
    /// When each bar series was last fetched, by symbol.
    #[serde(default)]
    pub bars_fetched_at: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MergeCounts {
    pub added: usize,
    pub updated: usize,
}

pub struct FinanceStore {
    root: PathBuf,
}

/// Exclusive hold on the store for one sync; released on drop.
pub struct StoreLock {
    _file: std::fs::File,
}

impl FinanceStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Wait for, then hold, the store's sync lock.
    pub fn lock(&self) -> Result<StoreLock, FinanceError> {
        let file = self.lock_file()?;
        file.lock()
            .map_err(|error| store_error("lock the finance store", error))?;
        Ok(StoreLock { _file: file })
    }

    /// `None` when another sync holds the store.
    pub fn try_lock(&self) -> Result<Option<StoreLock>, FinanceError> {
        let file = self.lock_file()?;
        match file.try_lock() {
            Ok(()) => Ok(Some(StoreLock { _file: file })),
            Err(std::fs::TryLockError::WouldBlock) => Ok(None),
            Err(std::fs::TryLockError::Error(error)) => {
                Err(store_error("lock the finance store", error))
            }
        }
    }

    fn lock_file(&self) -> Result<std::fs::File, FinanceError> {
        ensure_private_dir(&self.root)?;
        std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.root.join("sync.lock"))
            .map_err(|error| store_error("open the finance lock", error))
    }

    pub fn save_snapshot(&self, snapshot: &Snapshot) -> Result<(), FinanceError> {
        self.write_json(&self.root.join("snapshot.json"), snapshot)
    }

    pub fn load_snapshot(&self) -> Result<Option<Snapshot>, FinanceError> {
        let snapshot: Option<Snapshot> = read_json(&self.root.join("snapshot.json"))?;
        match snapshot {
            Some(snapshot) if snapshot.schema_version != SNAPSHOT_SCHEMA => Err(FinanceError::Store(
                format!(
                    "finance snapshot schema {} is newer than this Heiwa understands; update Heiwa rather than overwriting it",
                    snapshot.schema_version
                ),
            )),
            other => Ok(other),
        }
    }

    pub fn merge_activities(&self, incoming: Vec<Activity>) -> Result<MergeCounts, FinanceError> {
        let path = self.root.join("activities.jsonl");
        let mut rows: BTreeMap<(String, String), Activity> = read_jsonl::<Activity>(&path)?
            .into_iter()
            .map(|row| ((row.account_id.clone(), row.id.clone()), row))
            .collect();
        let mut counts = MergeCounts::default();
        for row in incoming {
            match rows.insert((row.account_id.clone(), row.id.clone()), row.clone()) {
                None => counts.added += 1,
                Some(previous) if previous != row => counts.updated += 1,
                Some(_) => {}
            }
        }
        let mut ordered: Vec<Activity> = rows.into_values().collect();
        ordered.sort_by(|a, b| {
            (a.trade_date.as_deref(), &a.account_id, &a.id).cmp(&(
                b.trade_date.as_deref(),
                &b.account_id,
                &b.id,
            ))
        });
        self.write_jsonl(&path, &ordered)?;
        Ok(counts)
    }

    pub fn load_activities(&self) -> Result<Vec<Activity>, FinanceError> {
        read_jsonl(&self.root.join("activities.jsonl"))
    }

    /// Upsert bars by date; returns how many dates were new.
    pub fn merge_bars(&self, symbol: &str, incoming: Vec<Bar>) -> Result<usize, FinanceError> {
        let path = self.series_path("bars", symbol)?;
        let mut rows: BTreeMap<String, Bar> = read_jsonl::<Bar>(&path)?
            .into_iter()
            .map(|bar| (bar.date.clone(), bar))
            .collect();
        let before = rows.len();
        rows.extend(incoming.into_iter().map(|bar| (bar.date.clone(), bar)));
        let added = rows.len() - before;
        self.write_jsonl(&path, &rows.into_values().collect::<Vec<_>>())?;
        Ok(added)
    }

    pub fn load_bars(&self, symbol: &str) -> Result<Vec<Bar>, FinanceError> {
        read_jsonl(&self.series_path("bars", symbol)?)
    }

    /// Upsert FX rates by date; returns how many dates were new.
    pub fn merge_fx(&self, series: &str, incoming: Vec<FxRate>) -> Result<usize, FinanceError> {
        let path = self.series_path("fx", series)?;
        let mut rows: BTreeMap<String, FxRate> = read_jsonl::<FxRate>(&path)?
            .into_iter()
            .map(|rate| (rate.date.clone(), rate))
            .collect();
        let before = rows.len();
        rows.extend(incoming.into_iter().map(|rate| (rate.date.clone(), rate)));
        let added = rows.len() - before;
        self.write_jsonl(&path, &rows.into_values().collect::<Vec<_>>())?;
        Ok(added)
    }

    pub fn load_fx(&self, series: &str) -> Result<Vec<FxRate>, FinanceError> {
        read_jsonl(&self.series_path("fx", series)?)
    }

    pub fn load_settings(&self) -> Result<Settings, FinanceError> {
        Ok(read_json(&self.root.join("settings.json"))?.unwrap_or_default())
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<(), FinanceError> {
        self.write_json(&self.root.join("settings.json"), settings)
    }

    pub fn load_sync_status(&self) -> Result<SyncStatus, FinanceError> {
        Ok(read_json(&self.root.join("sync.json"))?.unwrap_or_default())
    }

    pub fn save_sync_status(&self, status: &SyncStatus) -> Result<(), FinanceError> {
        self.write_json(&self.root.join("sync.json"), status)
    }

    /// `<root>/<dir>/<name>.jsonl` for a ticker or series name that cannot
    /// leave its directory.
    fn series_path(&self, dir: &str, name: &str) -> Result<PathBuf, FinanceError> {
        let safe = !name.is_empty()
            && name.len() <= 32
            && !name.starts_with('.')
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'));
        if !safe {
            return Err(FinanceError::Store(format!(
                "{name:?} is not a market series name"
            )));
        }
        Ok(self.root.join(dir).join(format!("{name}.jsonl")))
    }

    fn write_json<T: Serialize>(&self, path: &Path, value: &T) -> Result<(), FinanceError> {
        let body = serde_json::to_vec_pretty(value).map_err(|error| {
            FinanceError::Store(format!("encode {}: {error}", display_name(path)))
        })?;
        self.write_private(path, &body)
    }

    fn write_jsonl<T: Serialize>(&self, path: &Path, rows: &[T]) -> Result<(), FinanceError> {
        let mut body = Vec::new();
        for row in rows {
            serde_json::to_writer(&mut body, row).map_err(|error| {
                FinanceError::Store(format!("encode {}: {error}", display_name(path)))
            })?;
            body.push(b'\n');
        }
        self.write_private(path, &body)
    }

    /// Owner-only, atomic replace: write a sibling temp file, fsync, rename.
    fn write_private(&self, path: &Path, body: &[u8]) -> Result<(), FinanceError> {
        use std::io::Write as _;
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        ensure_private_dir(&self.root)?;
        let parent = path
            .parent()
            .ok_or_else(|| FinanceError::Store("path has no parent".into()))?;
        ensure_private_dir(parent)?;
        let temporary = parent.join(format!(
            ".{}.{}.{}.tmp",
            display_name(path),
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let result = (|| -> std::io::Result<()> {
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&temporary)?;
            file.write_all(body)?;
            file.sync_all()?;
            std::fs::rename(&temporary, path)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result.map_err(|error| store_error(&format!("write {}", display_name(path)), error))
    }
}

fn ensure_private_dir(dir: &Path) -> Result<(), FinanceError> {
    std::fs::create_dir_all(dir).map_err(|error| store_error("create the finance store", error))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| store_error("protect the finance store", error))?;
    }
    Ok(())
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>, FinanceError> {
    match std::fs::read(path) {
        Ok(raw) => serde_json::from_slice(&raw).map(Some).map_err(|error| {
            FinanceError::Store(format!("{} is unreadable: {error}", display_name(path)))
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(store_error(&format!("read {}", display_name(path)), error)),
    }
}

fn read_jsonl<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Vec<T>, FinanceError> {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(store_error(&format!("read {}", display_name(path)), error)),
    };
    raw.lines()
        .filter(|line| !line.trim().is_empty())
        .enumerate()
        .map(|(index, line)| {
            serde_json::from_str(line).map_err(|error| {
                FinanceError::Store(format!(
                    "{} line {} is unreadable: {error}",
                    display_name(path),
                    index + 1
                ))
            })
        })
        .collect()
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn store_error(action: &str, error: std::io::Error) -> FinanceError {
    FinanceError::Store(format!("could not {action}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AccountKind, ActivityKind};

    fn store() -> (tempfile::TempDir, FinanceStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = FinanceStore::new(dir.path().join("finance"));
        (dir, store)
    }

    fn account(id: &str) -> Account {
        Account {
            id: id.into(),
            source: "snaptrade".into(),
            institution: "Example Brokerage".into(),
            name: "TFSA".into(),
            number_hint: Some("4567".into()),
            kind: AccountKind::Tfsa,
            raw_type: Some("TFSA".into()),
            total_value: Some(1000.0),
            currency: Some("CAD".into()),
            cash: Vec::new(),
            holdings_synced_at: None,
            transactions_synced_at: None,
            is_paper: false,
        }
    }

    fn activity(id: &str, date: &str, amount: f64) -> Activity {
        Activity {
            id: id.into(),
            account_id: "acct-1".into(),
            kind: ActivityKind::Contribution,
            raw_type: "CONTRIBUTION".into(),
            symbol: None,
            units: None,
            price: None,
            amount: Some(amount),
            currency: Some("CAD".into()),
            fee: None,
            trade_date: Some(date.into()),
            settlement_date: Some(date.into()),
            description: None,
        }
    }

    fn bar(date: &str, close: f64) -> Bar {
        Bar {
            date: date.into(),
            open: None,
            high: None,
            low: None,
            close,
            volume: None,
        }
    }

    #[test]
    fn a_fresh_store_reads_as_empty() {
        let (_dir, store) = store();
        assert_eq!(store.load_snapshot().unwrap(), None);
        assert!(store.load_activities().unwrap().is_empty());
        assert!(store.load_bars("XIC.TO").unwrap().is_empty());
        assert!(store.load_fx("FXUSDCAD").unwrap().is_empty());
        assert_eq!(store.load_settings().unwrap(), Settings::default());
        assert_eq!(store.load_sync_status().unwrap(), SyncStatus::default());
    }

    #[test]
    fn snapshot_round_trips_through_owner_private_files() {
        let (_dir, store) = store();
        let snapshot = Snapshot {
            schema_version: SNAPSHOT_SCHEMA,
            synced_at: "2026-09-25T12:00:00Z".into(),
            accounts: vec![account("acct-1")],
            holdings: Vec::new(),
            positions_as_of: BTreeMap::from([("acct-1".into(), "2026-09-25T11:59:00Z".into())]),
        };
        store.save_snapshot(&snapshot).unwrap();
        assert_eq!(store.load_snapshot().unwrap(), Some(snapshot));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(store.root()), 0o700);
            assert_eq!(mode(&store.root().join("snapshot.json")), 0o600);
        }
        let leftovers: Vec<_> = std::fs::read_dir(store.root())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "atomic writes leave no temp files");
    }

    #[test]
    fn activities_upsert_by_account_and_id_and_stay_date_ordered() {
        let (_dir, store) = store();
        let first = store
            .merge_activities(vec![
                activity("a2", "2026-08-02", 250.0),
                activity("a1", "2026-08-01", 250.0),
            ])
            .unwrap();
        assert_eq!(
            first,
            MergeCounts {
                added: 2,
                updated: 0
            }
        );
        let mut other_account = activity("a1", "2026-07-01", 100.0);
        other_account.account_id = "acct-2".into();
        let second = store
            .merge_activities(vec![
                activity("a2", "2026-08-02", 300.0),
                activity("a3", "2026-08-03", 250.0),
                activity("a1", "2026-08-01", 250.0),
                other_account,
            ])
            .unwrap();
        assert_eq!(
            second,
            MergeCounts {
                added: 2,
                updated: 1
            }
        );
        let stored = store.load_activities().unwrap();
        let keys: Vec<_> = stored
            .iter()
            .map(|row| format!("{}:{}", row.account_id, row.id))
            .collect();
        assert_eq!(keys, ["acct-2:a1", "acct-1:a1", "acct-1:a2", "acct-1:a3"]);
        assert_eq!(stored[2].amount, Some(300.0));
    }

    #[test]
    fn bars_and_fx_upsert_by_date() {
        let (_dir, store) = store();
        assert_eq!(
            store
                .merge_bars(
                    "XIC.TO",
                    vec![bar("2026-09-24", 40.0), bar("2026-09-23", 39.5)]
                )
                .unwrap(),
            2
        );
        assert_eq!(
            store
                .merge_bars(
                    "XIC.TO",
                    vec![bar("2026-09-24", 40.2), bar("2026-09-25", 40.4)]
                )
                .unwrap(),
            1
        );
        let bars = store.load_bars("XIC.TO").unwrap();
        assert_eq!(
            bars.iter().map(|bar| bar.close).collect::<Vec<_>>(),
            [39.5, 40.2, 40.4]
        );

        let rate = |date: &str, rate: f64| FxRate {
            date: date.into(),
            rate,
        };
        assert_eq!(
            store
                .merge_fx("FXUSDCAD", vec![rate("2026-09-25", 1.4145)])
                .unwrap(),
            1
        );
        assert_eq!(
            store
                .merge_fx(
                    "FXUSDCAD",
                    vec![rate("2026-09-24", 1.4136), rate("2026-09-25", 1.4145)]
                )
                .unwrap(),
            1
        );
        assert_eq!(store.load_fx("FXUSDCAD").unwrap().len(), 2);
    }

    #[test]
    fn series_names_cannot_escape_the_store() {
        let (_dir, store) = store();
        for name in ["../evil", "a/b", "", ".hidden", "x y"] {
            assert!(
                store
                    .merge_bars(name, vec![bar("2026-09-24", 1.0)])
                    .is_err(),
                "{name:?}"
            );
            assert!(store.load_bars(name).is_err(), "{name:?}");
            assert!(store.merge_fx(name, Vec::new()).is_err(), "{name:?}");
        }
    }

    #[test]
    fn settings_and_sync_status_round_trip() {
        let (_dir, store) = store();
        let settings = Settings {
            base_currency: "CAD".into(),
            benchmark: "XEQT.TO".into(),
            tfsa_room: Some(TfsaRoom {
                year: 2026,
                room_at_start: 7000.0,
            }),
        };
        store.save_settings(&settings).unwrap();
        assert_eq!(store.load_settings().unwrap(), settings);

        let status = SyncStatus {
            last_attempt_at: Some("2026-09-25T12:00:00Z".into()),
            outcome: Some("partial".into()),
            issues: vec![SourceIssue {
                source: "Alpha Vantage".into(),
                message: "rate limited".into(),
            }],
            ..SyncStatus::default()
        };
        store.save_sync_status(&status).unwrap();
        assert_eq!(store.load_sync_status().unwrap(), status);
    }

    #[test]
    fn the_sync_lock_is_exclusive_across_handles() {
        let (_dir, store) = store();
        let held = store.lock().unwrap();
        let other = FinanceStore::new(store.root());
        assert!(
            other.try_lock().unwrap().is_none(),
            "second holder must not acquire"
        );
        drop(held);
        assert!(other.try_lock().unwrap().is_some());
    }
}
