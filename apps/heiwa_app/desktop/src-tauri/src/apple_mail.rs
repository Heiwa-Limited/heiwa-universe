use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Mutex, TryLockError};
use std::time::Duration;
#[cfg(test)]
use std::time::Instant;
use tauri::Manager;

const BACKGROUND_SCAN_TIMEOUT: Duration = Duration::from_secs(25);
const EXPLICIT_SCAN_TIMEOUT: Duration = Duration::from_secs(45);
const TAURI_TIMEOUT_GRACE: Duration = Duration::from_secs(5);
const MAX_RESULT_BYTES: usize = 64 * 1024;
static SCAN_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppleMailScanResult {
    status: String,
    freshness: Option<String>,
    fetched: usize,
    appended: usize,
    deduplicated: usize,
    updated: usize,
    removed: usize,
    error: Option<String>,
    error_class: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct AppleMailScanOptions {
    #[serde(default)]
    background: bool,
    #[serde(default)]
    stale_seconds: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ScanPayload {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    freshness: Option<String>,
    sources: Vec<SourceReport>,
    fetched: usize,
    #[serde(default)]
    appended: usize,
    #[serde(default)]
    deduplicated: usize,
    #[serde(default)]
    updated: usize,
    #[serde(default)]
    removed: usize,
    #[serde(default)]
    error_class: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SourceReport {
    source: String,
    status: String,
    error: Option<String>,
    reason: Option<String>,
    error_class: Option<String>,
}

#[tauri::command]
pub async fn apple_mail_scan(
    app: tauri::AppHandle,
    options: Option<AppleMailScanOptions>,
) -> Result<AppleMailScanResult, String> {
    if !cfg!(target_os = "macos") {
        return Err("Apple Mail reading is available only on macOS.".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = acquire_scan_lock()?;
        let executable_dir = std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf));
        let resource_dir = app.path().resource_dir().ok();
        let binary = crate::runtime_supervisor::find_runtime_binary(
            resource_dir.as_deref(),
            executable_dir.as_deref(),
            crate::which_on_path,
            |path| path.is_file(),
        )
        .ok_or_else(|| "The bundled Heiwa runtime could not be found.".to_string())?;

        let options = options.unwrap_or_default();
        let cli_timeout = if options.background {
            BACKGROUND_SCAN_TIMEOUT
        } else {
            EXPLICIT_SCAN_TIMEOUT
        };
        run_scan_process(
            &binary,
            cli_timeout + TAURI_TIMEOUT_GRACE,
            options.background,
            options.stale_seconds,
        )
    })
    .await
    .map_err(|_| "Apple Mail reading stopped unexpectedly.".to_string())?
}

fn acquire_scan_lock() -> Result<std::sync::MutexGuard<'static, ()>, String> {
    match SCAN_LOCK.try_lock() {
        Ok(guard) => Ok(guard),
        Err(TryLockError::WouldBlock) => {
            Err("Apple Mail is already being read in another window.".to_string())
        }
        Err(TryLockError::Poisoned(_)) => {
            Err("Apple Mail reading is temporarily unavailable.".to_string())
        }
    }
}

fn run_scan_process(
    binary: &Path,
    timeout: Duration,
    background: bool,
    stale_seconds: Option<u64>,
) -> Result<AppleMailScanResult, String> {
    let mut command = Command::new(binary);
    command.args(["mail", "scan", "--source", "apple"]);
    if background {
        command.args(["--if-running", "--if-stale"]);
        command.arg(stale_seconds.unwrap_or(180).min(86_400).to_string());
    }
    command
        .arg("--json")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    let output =
        match heiwa_core::subprocess::bounded_output(&mut command, timeout, MAX_RESULT_BYTES) {
            Ok(output) => output,
            Err(error) if error.contains("timed out") => {
                return Ok(AppleMailScanResult {
                    status: "error".into(),
                    freshness: Some("timeout".into()),
                    fetched: 0,
                    appended: 0,
                    deduplicated: 0,
                    updated: 0,
                    removed: 0,
                    error: Some(error),
                    error_class: Some("timeout".into()),
                });
            }
            Err(error) => return Err(error),
        };
    parse_scan_output(&output)
}

fn parse_scan_output(output: &[u8]) -> Result<AppleMailScanResult, String> {
    let payload: ScanPayload = serde_json::from_slice(output)
        .map_err(|_| "The Heiwa runtime returned an invalid Apple Mail result.".to_string())?;
    let source = payload
        .sources
        .iter()
        .find(|report| report.source == "apple")
        .ok_or_else(|| "The Heiwa runtime did not report an Apple Mail result.".to_string())?;
    let status = payload
        .status
        .clone()
        .unwrap_or_else(|| source.status.clone());
    let detail = source
        .error
        .as_deref()
        .or(source.reason.as_deref())
        .map(sanitize_detail);
    let error_class = payload
        .error_class
        .clone()
        .or(source.error_class.clone())
        .or_else(|| detail.as_deref().map(classify_error).map(str::to_string));
    let structured_error = matches!(status.as_str(), "error" | "backoff" | "skipped");
    Ok(AppleMailScanResult {
        status,
        freshness: payload.freshness,
        fetched: payload.fetched,
        appended: payload.appended,
        deduplicated: payload.deduplicated,
        updated: payload.updated,
        removed: payload.removed,
        error: if structured_error {
            detail.or_else(|| Some("Apple Mail is not ready on this Mac.".into()))
        } else {
            detail
        },
        error_class,
    })
}

fn sanitize_detail(detail: &str) -> String {
    detail
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .take(512)
        .collect()
}

fn classify_error(detail: &str) -> &'static str {
    let lower = detail.to_ascii_lowercase();
    if lower.contains("timed out") || lower.contains("timeout") {
        "timeout"
    } else if lower.contains("not authorized")
        || lower.contains("-1743")
        || lower.contains("permission")
        || lower.contains("automation")
        || lower.contains("access denied")
    {
        "automation_denied"
    } else {
        "failed"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_scanned_result_without_mail_rows() {
        let result = parse_scan_output(
            br#"{
            "sources":[{"source":"apple","status":"scanned","fetched":3}],
            "fetched":3,"appended":2,"deduplicated":1
        }"#,
        )
        .unwrap();
        assert_eq!(
            result,
            AppleMailScanResult {
                status: "scanned".into(),
                freshness: None,
                fetched: 3,
                appended: 2,
                deduplicated: 1,
                updated: 0,
                removed: 0,
                error: None,
                error_class: None,
            }
        );
    }

    #[test]
    fn preserves_structured_error_and_skipped_sources_for_the_ui() {
        for (status, field, detail) in [
            ("error", "error", "Automation access denied"),
            ("skipped", "reason", "No Apple Mail account is configured"),
        ] {
            let payload = format!(
                r#"{{"sources":[{{"source":"apple","status":"{status}","{field}":"{detail}"}}],"fetched":0,"appended":0,"deduplicated":0}}"#,
            );
            let result = parse_scan_output(payload.as_bytes()).unwrap();
            assert_eq!(result.error.as_deref(), Some(detail));
        }
    }

    #[test]
    fn prevents_overlapping_reads_across_windows() {
        let guard = acquire_scan_lock().unwrap();
        assert!(acquire_scan_lock()
            .unwrap_err()
            .contains("already being read"));
        drop(guard);
        assert!(acquire_scan_lock().is_ok());
    }

    /// Write an executable stand-in for the runtime.
    #[cfg(unix)]
    fn runtime_script(body: &str) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = std::env::temp_dir().join(format!("heiwa-mail-scan-{}", uuid::Uuid::new_v4()));
        std::fs::write(&path, body).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        path
    }

    /// Run a freshly written script as the runtime.
    ///
    /// Tests run in parallel, and another test's fork can inherit this
    /// script's write descriptor for the instant before that child execs.
    /// Linux refuses to execute a file that is open for writing (ETXTBSY), so
    /// the spawn fails with "could not start". That is a harness race, not the
    /// behaviour under test: a failed start is retried briefly and every other
    /// outcome is returned exactly as observed.
    #[cfg(unix)]
    fn run_fresh_script(path: &Path, timeout: Duration) -> Result<AppleMailScanResult, String> {
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            let result = run_scan_process(path, timeout, false, None);
            let could_not_start =
                matches!(&result, Err(error) if error.contains("could not start"));
            if !could_not_start || Instant::now() >= deadline {
                return result;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[cfg(unix)]
    #[test]
    fn times_out_a_hung_runtime_process() {
        let path = runtime_script("#!/bin/sh\nsleep 10\n");
        let started = Instant::now();
        let result = run_fresh_script(&path, Duration::from_millis(50));
        let _ = std::fs::remove_file(&path);
        let result = result.unwrap();
        assert_eq!(result.status, "error");
        assert_eq!(result.error_class.as_deref(), Some("timeout"));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[cfg(unix)]
    #[test]
    fn times_out_when_a_descendant_keeps_stdout_open_after_the_parent_exits() {
        let path = runtime_script("#!/bin/sh\nsleep 10 &\nprintf '%s' '{\"sources\":[{\"source\":\"apple\",\"status\":\"scanned\"}],\"fetched\":0,\"appended\":0,\"deduplicated\":0}'\n");
        let started = Instant::now();
        let result = run_fresh_script(&path, Duration::from_millis(100));
        let _ = std::fs::remove_file(&path);
        let result = result.unwrap();
        assert_eq!(result.status, "error");
        assert_eq!(result.error_class.as_deref(), Some("timeout"));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_oversized_runtime_output() {
        let path = runtime_script("#!/bin/sh\nyes x | head -c 70000\n");
        let result = run_fresh_script(&path, Duration::from_secs(2));
        let _ = std::fs::remove_file(&path);
        assert!(result.unwrap_err().contains("oversized"));
    }
}
