//! `heiwa capabilities` — local capability inventory refresh.
//!
//! Re-derives bounded, redacted capability metadata from local provider apps
//! and the curated source-pack catalog, then writes a local evidence receipt.
//! Raw auth, transcripts, and token-bearing material stay provider-owned and
//! are never surfaced. Reference sources stay read-only Intake/Evidence material
//! until promoted through manifest / adapter / tool-lease gates.

use anyhow::{anyhow, Result};
use chrono::Utc;
use heiwa_evidence::find_sensitive;
use serde::Serialize;
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::fs;
use std::path::{Path, PathBuf};

const CATALOG_SCHEMA_VERSION: &str = "heiwa_local_capability_inventory_v1";
const REFRESH_KIND: &str = "capability_refresh";

/// AI provider desktop apps Heiwa treats as ecosystem participants. Scans are
/// scoped to these so `installed_apps_observed` stays the provider-app set
/// rather than every app under `/Applications`.
const KNOWN_AI_APP_STEMS: &[&str] = &[
    "Codex",
    "Claude",
    "Gemini",
    "Antigravity",
    "ChatGPT",
    "Ollama",
];

/// A redacted local app observation: name and bundle identifier only — never a
/// filesystem path.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct AppObservation {
    pub name: String,
    pub bundle_id: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Observed {
    pub installed_apps: Vec<AppObservation>,
    pub providers: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct RefreshOutcome {
    pub report: Value,
    pub catalog_path: PathBuf,
    pub evidence_path: PathBuf,
}

pub fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("refresh") => refresh(&args[1..]),
        Some("status") | None => status(args),
        Some("--help") | Some("-h") => {
            print_help();
            Ok(())
        }
        Some(other) => Err(anyhow!("unknown capabilities command: {other}")),
    }
}

/// Produce the next catalog from the prior one: curated fields are preserved,
/// `generated_at` is bumped, the live AI-app scan replaces
/// `installed_apps_observed`, and a `refresh` provenance block is appended.
/// Curated `providers` are intentionally left untouched; detected provider CLIs
/// are recorded in provenance instead so curated version metadata is not lost.
pub(crate) fn refreshed_catalog(prior: &Value, observed: &Observed, generated_at: &str) -> Value {
    let mut next = match prior {
        Value::Object(_) => prior.clone(),
        _ => json!({}),
    };
    let obj = next
        .as_object_mut()
        .expect("refreshed catalog is always an object");

    obj.entry("schema_version".to_string())
        .or_insert_with(|| json!(CATALOG_SCHEMA_VERSION));
    obj.insert("generated_at".to_string(), json!(generated_at));

    // Rebuild installed_apps_observed from the live scan: refresh name/bundle_id,
    // preserve the curated `heiwa_use` annotation by bundle_id, and drop `path`
    // so absolute filesystem paths never enter the bounded surface.
    let apps: Vec<Value> = observed
        .installed_apps
        .iter()
        .map(|app| {
            let mut entry = serde_json::Map::new();
            entry.insert("name".to_string(), json!(app.name));
            entry.insert("bundle_id".to_string(), json!(app.bundle_id));
            if let Some(use_label) = curated_app_use(prior, &app.bundle_id) {
                entry.insert("heiwa_use".to_string(), json!(use_label));
            }
            Value::Object(entry)
        })
        .collect();
    obj.insert("installed_apps_observed".to_string(), json!(apps));

    obj.insert(
        "refresh".to_string(),
        json!({
            "at": generated_at,
            "method": "local_redacted_rescan",
            "observed_installed_apps": observed.installed_apps.len(),
            "detected_provider_clis": observed.providers,
            "redaction_applied": true,
            "posture": "bounded_counts_and_refs_only",
        }),
    );
    next
}

/// Extract `CFBundleIdentifier` from an XML `Info.plist`. Binary plists or
/// missing keys return `None` so the caller can fall back to the app name.
pub(crate) fn bundle_id_from_info_plist(xml: &str) -> Option<String> {
    let key_pos = xml.find("CFBundleIdentifier")?;
    let after = &xml[key_pos..];
    let open = after.find("<string>")? + "<string>".len();
    let rest = &after[open..];
    let close = rest.find("</string>")?;
    let id = rest[..close].trim().to_string();
    (!id.is_empty()).then_some(id)
}

/// Scan the given application directories for `.app` bundles whose name matches
/// one of `allow_stems` (empty = allow all) and return redacted observations
/// (app name + bundle identifier). Filesystem paths are never returned.
pub(crate) fn scan_installed_apps(
    app_dirs: &[PathBuf],
    allow_stems: &[&str],
) -> Vec<AppObservation> {
    let mut found: Vec<AppObservation> = Vec::new();
    for dir in app_dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("app") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if !allow_stems.is_empty()
                && !allow_stems
                    .iter()
                    .any(|allow| stem.eq_ignore_ascii_case(allow) || stem.contains(allow))
            {
                continue;
            }
            let info = path.join("Contents").join("Info.plist");
            let bundle_id = fs::read_to_string(&info)
                .ok()
                .and_then(|xml| bundle_id_from_info_plist(&xml))
                .unwrap_or_else(|| stem.to_string());
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(stem)
                .to_string();
            if found.iter().any(|app| app.bundle_id == bundle_id) {
                continue;
            }
            found.push(AppObservation { name, bundle_id });
        }
    }
    found.sort_by(|a, b| a.bundle_id.cmp(&b.bundle_id));
    found
}

/// Look up the curated `heiwa_use` annotation for a bundle id in the prior
/// catalog, so curated app semantics survive a refresh.
fn curated_app_use(prior: &Value, bundle_id: &str) -> Option<String> {
    prior
        .get("installed_apps_observed")
        .and_then(Value::as_array)?
        .iter()
        .find(|app| app.get("bundle_id").and_then(Value::as_str) == Some(bundle_id))
        .and_then(|app| app.get("heiwa_use").and_then(Value::as_str))
        .map(str::to_string)
}

/// Build a local evidence receipt matching the `~/.heiwa/state/evidence`
/// convention (`kind` / `created_at` / `redaction_applied` / `payload`).
pub(crate) fn evidence_receipt(kind: &str, payload: Value, created_at: &str) -> Value {
    json!({
        "kind": kind,
        "created_at": created_at,
        "redaction_applied": true,
        "payload": payload,
    })
}

/// Re-derive the bounded capability catalog, write an evidence receipt, and
/// return the surfaced report. A `find_sensitive` gate aborts the refresh if any
/// credential-shaped material would be persisted or surfaced.
pub(crate) fn perform_refresh(
    state_dir: &Path,
    observed: &Observed,
    now: &str,
    dry_run: bool,
) -> Result<RefreshOutcome> {
    let cap_dir = state_dir.join("capabilities");
    let prior = latest_catalog(&cap_dir).unwrap_or_else(|| json!({}));
    let next = refreshed_catalog(&prior, observed, now);

    // Security gate: refuse to persist or surface any sensitive material.
    if find_sensitive(&next).is_some() {
        return Err(anyhow!(
            "refresh aborted: refused to surface sensitive material in catalog"
        ));
    }

    let iso_date = now.get(0..10).unwrap_or("0000-00-00").to_string();
    let catalog_id = format!("local-capability-inventory-{iso_date}");
    let catalog_path = cap_dir.join(format!("{catalog_id}.json"));
    let counts = bounded_counts(&next);

    let payload = json!({
        "catalog_id": catalog_id,
        "catalog_basename": format!("{catalog_id}.json"),
        "counts": counts,
        "observed_installed_apps": observed.installed_apps.len(),
        "detected_provider_clis": observed.providers.len(),
        "source_posture": "local_read_only_redacted",
    });
    let receipt = evidence_receipt(REFRESH_KIND, payload, now);
    if find_sensitive(&receipt).is_some() {
        return Err(anyhow!(
            "refresh aborted: refused to surface sensitive material in receipt"
        ));
    }

    let compact: String = now.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let short = short_hash(&next);
    let evidence_dir = state_dir.join("evidence").join(&iso_date);
    let evidence_path = evidence_dir.join(format!("{compact}-{REFRESH_KIND}-{short}.json"));

    let report = json!({
        "command": "capabilities refresh",
        "dry_run": dry_run,
        "catalog_id": catalog_id,
        "catalog_path": catalog_path.display().to_string(),
        "evidence_path": evidence_path.display().to_string(),
        "counts": counts,
        "providers": observed.providers.len(),
        "installed_apps_observed": observed.installed_apps.len(),
        "redaction_applied": true,
    });

    if !dry_run {
        fs::create_dir_all(&cap_dir)?;
        fs::write(&catalog_path, serde_json::to_string_pretty(&next)?)?;
        fs::create_dir_all(&evidence_dir)?;
        fs::write(&evidence_path, serde_json::to_string_pretty(&receipt)?)?;
    }

    Ok(RefreshOutcome {
        report,
        catalog_path,
        evidence_path,
    })
}

/// Load the newest valid `local-capability-inventory-*` catalog. Files that do
/// not match the inventory naming convention or fail to parse are ignored, so a
/// stray or malformed file in the capabilities dir can never become the prior.
fn latest_catalog(cap_dir: &Path) -> Option<Value> {
    let entries = fs::read_dir(cap_dir).ok()?;
    let mut candidates: Vec<(String, Value)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        if !stem.starts_with("local-capability-inventory-") {
            continue;
        }
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&raw) else {
            continue;
        };
        candidates.push((stem, value));
    }
    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    candidates.into_iter().next().map(|(_, value)| value)
}

/// Bounded counts mirroring the `/api/v1/capabilities` surface: counts only,
/// never raw catalog bodies.
fn bounded_counts(catalog: &Value) -> Value {
    let count = |key: &str| {
        catalog
            .get(key)
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0)
    };
    json!({
        "providers": count("providers"),
        "installed_apps_observed": count("installed_apps_observed"),
        "codex_plugins_observed": count("codex_plugins_observed"),
        "codex_mcp_servers": count("codex_mcp_servers"),
        "claude_plugins_observed": count("claude_plugins_observed"),
        "gemini_extensions": count("gemini_extensions"),
        "gemini_skills_observed": count("gemini_skills_observed"),
        "reference_sources": count("reference_sources"),
        "integration_families": count("integration_families"),
        "runtime_targets": count("runtime_targets"),
        "performance_targets": count("performance_targets"),
    })
}

fn short_hash(value: &Value) -> String {
    let mut hasher = Sha1::new();
    hasher.update(value.to_string().as_bytes());
    hasher
        .finalize()
        .iter()
        .take(4)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

// ----- CLI handlers -----

fn refresh(args: &[String]) -> Result<()> {
    let dry_run = has_flag(args, "--dry-run");
    let json_out = has_flag(args, "--json");
    let now = now_rfc3339();
    let observed = Observed {
        installed_apps: scan_installed_apps(&default_app_dirs(), KNOWN_AI_APP_STEMS),
        providers: detect_provider_clis(),
    };
    let outcome = perform_refresh(&state_dir(), &observed, &now, dry_run)?;

    if json_out {
        println!("{}", outcome.report);
    } else {
        println!(
            "capabilities refresh{}",
            if dry_run { " (dry-run)" } else { "" }
        );
        println!("  catalog:  {}", outcome.catalog_path.display());
        println!("  evidence: {}", outcome.evidence_path.display());
        println!(
            "  installed apps: {}",
            outcome
                .report
                .get("installed_apps_observed")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        );
        println!(
            "  provider CLIs:  {}",
            outcome
                .report
                .get("providers")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        );
    }
    Ok(())
}

fn status(args: &[String]) -> Result<()> {
    let json_out = has_flag(args, "--json");
    let cap_dir = state_dir().join("capabilities");
    let catalog = latest_catalog(&cap_dir);
    let counts = catalog
        .as_ref()
        .map(bounded_counts)
        .unwrap_or_else(|| json!({}));
    let generated_at = catalog
        .as_ref()
        .and_then(|c| c.get("generated_at").and_then(Value::as_str))
        .map(str::to_string);
    let last_refresh = catalog.as_ref().and_then(|c| c.get("refresh").cloned());

    if json_out {
        println!(
            "{}",
            json!({
                "command": "capabilities status",
                "path": cap_dir.display().to_string(),
                "generated_at": generated_at,
                "counts": counts,
                "last_refresh": last_refresh,
            })
        );
    } else {
        println!("capabilities status");
        println!("  path: {}", cap_dir.display());
        if let Some(generated) = &generated_at {
            println!("  generated_at: {generated}");
        }
        println!("  counts: {counts}");
    }
    Ok(())
}

fn print_help() {
    println!("heiwa capabilities");
    println!();
    println!("Usage:");
    println!("  heiwa capabilities refresh [--dry-run] [--json]");
    println!("  heiwa capabilities status [--json]");
    println!();
    println!("Refresh re-derives bounded, redacted capability metadata from local");
    println!("provider apps and the curated source-pack catalog, writing an evidence");
    println!("receipt under ~/.heiwa/state/evidence. Raw auth, transcripts, and tokens");
    println!("stay provider-owned and are never surfaced.");
}

fn default_app_dirs() -> Vec<PathBuf> {
    let mut dirs_v = vec![PathBuf::from("/Applications")];
    if let Some(home) = crate::home::heiwa_home() {
        dirs_v.push(home.join("Applications"));
    }
    dirs_v
}

/// Report which provider CLIs resolve on `PATH` (names only — redacted signal).
fn detect_provider_clis() -> Vec<String> {
    const KNOWN: &[&str] = &["ollama", "gemini", "claude", "codex", "antigravity"];
    let Some(paths) = std::env::var_os("PATH") else {
        return Vec::new();
    };
    let path_dirs: Vec<PathBuf> = std::env::split_paths(&paths).collect();
    KNOWN
        .iter()
        .filter(|name| path_dirs.iter().any(|dir| dir.join(name).is_file()))
        .map(|name| name.to_string())
        .collect()
}

fn state_dir() -> PathBuf {
    crate::home::heiwa_state_dir()
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let mut p = std::env::temp_dir();
        p.push(format!(
            "heiwa-capabilities-{name}-{}-{nanos}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).expect("temp dir");
        p
    }

    #[test]
    fn find_sensitive_flags_auth_path() {
        let v = json!({ "observed": "/Users/x/.codex/auth.json" });
        assert!(
            find_sensitive(&v).is_some(),
            "auth.json path must be flagged"
        );
    }

    #[test]
    fn find_sensitive_flags_token_prefix() {
        let v = json!({ "leak": "ghp_ABCDEF0123456789abcdef" });
        assert!(
            find_sensitive(&v).is_some(),
            "github token prefix must be flagged"
        );
    }

    #[test]
    fn find_sensitive_ignores_policy_vocabulary() {
        let v = json!({
            "source_policy": {
                "excluded": ["oauth_tokens", "api_keys", "token_bearing_mcp_headers", "credential_files"]
            },
            "reference_sources": ["official.openai.agents-sdk", "official.ollama.api"],
            "secret_policy": "redacted_config_metadata_only"
        });
        assert_eq!(
            find_sensitive(&v),
            None,
            "policy vocabulary must not be a false positive"
        );
    }

    #[test]
    fn refreshed_catalog_preserves_curated_and_updates_generated_at() {
        let prior = json!({
            "schema_version": CATALOG_SCHEMA_VERSION,
            "generated_at": "2026-06-03T00:00:00-07:00",
            "reference_sources": ["a", "b", "c"],
            "integration_families": ["x", "y"],
            "installed_apps_observed": ["old1"],
        });
        let observed = Observed {
            installed_apps: vec![
                AppObservation {
                    name: "Codex.app".into(),
                    bundle_id: "com.openai.codex".into(),
                },
                AppObservation {
                    name: "Claude.app".into(),
                    bundle_id: "com.anthropic.claudefordesktop".into(),
                },
            ],
            providers: vec!["claude".into(), "ollama".into()],
        };

        let next = refreshed_catalog(&prior, &observed, "2026-06-03T09:00:00Z");

        assert_eq!(
            next.get("reference_sources")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(3),
            "curated reference_sources preserved"
        );
        assert_eq!(
            next.get("integration_families")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(2),
            "curated integration_families preserved"
        );
        assert_eq!(
            next.get("generated_at").and_then(Value::as_str),
            Some("2026-06-03T09:00:00Z"),
            "generated_at updated"
        );
        assert_eq!(
            next.get("installed_apps_observed")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(2),
            "observed apps merged in"
        );
        assert!(
            next.get("refresh").and_then(|r| r.get("method")).is_some(),
            "refresh provenance recorded"
        );
        assert_eq!(
            next.get("schema_version").and_then(Value::as_str),
            Some(CATALOG_SCHEMA_VERSION),
            "schema_version retained"
        );
    }

    #[test]
    fn refreshed_catalog_has_no_sensitive_material() {
        let prior = json!({ "reference_sources": ["official.ollama.api"] });
        let observed = Observed {
            installed_apps: vec![AppObservation {
                name: "Codex.app".into(),
                bundle_id: "com.openai.codex".into(),
            }],
            providers: vec!["codex".into()],
        };
        let next = refreshed_catalog(&prior, &observed, "2026-06-03T09:00:00Z");
        assert_eq!(find_sensitive(&next), None);
    }

    #[test]
    fn refreshed_catalog_preserves_app_annotations_and_drops_paths() {
        // Prior catalog carries a curated annotation and an absolute path.
        let prior = json!({
            "installed_apps_observed": [
                {
                    "name": "Codex.app",
                    "path": "/Users/example/Applications/Codex.app",
                    "bundle_id": "com.openai.codex",
                    "heiwa_use": "codex_app_capability_and_session_surface"
                }
            ]
        });
        let observed = Observed {
            installed_apps: vec![AppObservation {
                name: "Codex.app".into(),
                bundle_id: "com.openai.codex".into(),
            }],
            providers: vec![],
        };

        let next = refreshed_catalog(&prior, &observed, "2026-06-03T09:00:00Z");
        let apps = next
            .get("installed_apps_observed")
            .and_then(Value::as_array)
            .expect("apps array");
        assert_eq!(apps.len(), 1);
        let app = &apps[0];
        assert_eq!(
            app.get("heiwa_use").and_then(Value::as_str),
            Some("codex_app_capability_and_session_surface"),
            "curated heiwa_use annotation preserved by bundle_id"
        );
        assert!(
            app.get("path").is_none(),
            "absolute path dropped from refreshed catalog: {app}"
        );
        assert_eq!(find_sensitive(&next), None);
    }

    #[test]
    fn bundle_id_from_info_plist_extracts_identifier() {
        let xml = r#"<?xml version="1.0"?>
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>Codex</string>
  <key>CFBundleIdentifier</key><string>com.openai.codex</string>
</dict></plist>"#;
        assert_eq!(
            bundle_id_from_info_plist(xml).as_deref(),
            Some("com.openai.codex")
        );
    }

    #[test]
    fn scan_installed_apps_reads_temp_bundles_redacted() {
        let dir = temp_dir("scan");
        let app = dir.join("Foo.app").join("Contents");
        fs::create_dir_all(&app).unwrap();
        fs::write(
            app.join("Info.plist"),
            r#"<plist><dict><key>CFBundleIdentifier</key><string>com.example.foo</string></dict></plist>"#,
        )
        .unwrap();
        // Non-app entries must be ignored.
        fs::write(dir.join("note.txt"), "ignore me").unwrap();

        let apps = scan_installed_apps(std::slice::from_ref(&dir), &["Foo"]);

        assert!(
            apps.iter().any(|a| a.bundle_id == "com.example.foo"),
            "bundle id extracted: {apps:?}"
        );
        assert!(
            apps.iter().any(|a| a.name == "Foo.app"),
            "app name captured: {apps:?}"
        );
        let serialized = serde_json::to_value(&apps).expect("serialize observations");
        assert!(
            find_sensitive(&serialized).is_none(),
            "no sensitive material in app list"
        );
        assert!(
            !apps
                .iter()
                .any(|a| a.bundle_id.contains('/') || a.name.contains('/')),
            "redacted: names/bundle ids only, no filesystem paths: {apps:?}"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn evidence_receipt_marks_redaction_kind_and_payload() {
        let r = evidence_receipt(
            REFRESH_KIND,
            json!({ "catalog_id": "x" }),
            "2026-06-03T09:00:00Z",
        );
        assert_eq!(r.get("kind").and_then(Value::as_str), Some(REFRESH_KIND));
        assert_eq!(
            r.get("redaction_applied").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            r.get("created_at").and_then(Value::as_str),
            Some("2026-06-03T09:00:00Z")
        );
        assert!(r.get("payload").is_some(), "payload present");
    }

    #[test]
    fn perform_refresh_writes_catalog_and_evidence_and_ignores_invalid_prior() {
        let state = temp_dir("perform");
        let cap = state.join("capabilities");
        fs::create_dir_all(&cap).unwrap();
        // Invalid catalog must be ignored (not panic, not surfaced).
        fs::write(cap.join("broken.json"), "{ this is not valid json").unwrap();
        // A stray sensitive file in the dir must never be surfaced.
        fs::write(cap.join("auth.json"), r#"{"token":"ghp_secretvalue123"}"#).unwrap();
        // A valid prior catalog with curated fields.
        fs::write(
            cap.join("local-capability-inventory-2026-06-02.json"),
            json!({
                "schema_version": CATALOG_SCHEMA_VERSION,
                "reference_sources": ["a", "b"],
                "integration_families": ["x"]
            })
            .to_string(),
        )
        .unwrap();

        let observed = Observed {
            installed_apps: vec![AppObservation {
                name: "Codex.app".into(),
                bundle_id: "com.openai.codex".into(),
            }],
            providers: vec!["codex".into(), "ollama".into()],
        };
        let now = "2026-06-03T09:00:00Z";
        let outcome = perform_refresh(&state, &observed, now, false).expect("refresh ok");

        let written = fs::read_to_string(&outcome.catalog_path).expect("catalog written");
        let written_v: Value = serde_json::from_str(&written).unwrap();
        assert_eq!(
            written_v
                .get("reference_sources")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(2),
            "curated fields carried forward"
        );
        assert_eq!(
            written_v
                .get("installed_apps_observed")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(1)
        );
        assert!(
            outcome.evidence_path.exists(),
            "evidence receipt written to {}",
            outcome.evidence_path.display()
        );
        // SECURITY: nothing sensitive surfaced in report or written catalog.
        assert_eq!(find_sensitive(&outcome.report), None);
        assert_eq!(find_sensitive(&written_v), None);
        assert!(
            !written.contains("ghp_secretvalue123"),
            "stray secret must not leak into catalog"
        );
        assert!(
            !outcome.report.to_string().contains("ghp_secretvalue123"),
            "stray secret must not leak into report"
        );
        let _ = fs::remove_dir_all(&state);
    }

    #[test]
    fn perform_refresh_dry_run_writes_nothing() {
        let state = temp_dir("dry");
        let observed = Observed {
            installed_apps: vec![],
            providers: vec!["ollama".into()],
        };
        let outcome =
            perform_refresh(&state, &observed, "2026-06-03T09:00:00Z", true).expect("dry run ok");
        assert_eq!(
            outcome.report.get("dry_run").and_then(Value::as_bool),
            Some(true)
        );
        assert!(
            !outcome.catalog_path.exists(),
            "dry-run must not write catalog"
        );
        assert!(
            !outcome.evidence_path.exists(),
            "dry-run must not write evidence"
        );
        let _ = fs::remove_dir_all(&state);
    }
}
