use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Mutex, TryLockError};
use std::time::Duration;
#[cfg(test)]
use std::time::Instant;
use tauri::Manager;

const SCAN_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_RESULT_BYTES: usize = 64 * 1024;
static SCAN_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppleMailScanResult {
    fetched: usize,
    appended: usize,
    deduplicated: usize,
}

#[derive(Debug, Deserialize)]
struct ScanPayload {
    sources: Vec<SourceReport>,
    fetched: usize,
    appended: usize,
    deduplicated: usize,
}

#[derive(Debug, Deserialize)]
struct SourceReport {
    source: String,
    status: String,
    error: Option<String>,
    reason: Option<String>,
}

#[tauri::command]
pub async fn apple_mail_scan(app: tauri::AppHandle) -> Result<AppleMailScanResult, String> {
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

        run_scan_process(&binary, SCAN_TIMEOUT)
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

fn run_scan_process(binary: &Path, timeout: Duration) -> Result<AppleMailScanResult, String> {
    let mut command = Command::new(binary);
    command
        .args([
            "mail", "scan", "--source", "apple", "--limit", "50", "--json",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    let output = heiwa_core::subprocess::bounded_output(&mut command, timeout, MAX_RESULT_BYTES)?;
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
    if source.status != "scanned" {
        let detail = source
            .error
            .as_deref()
            .or(source.reason.as_deref())
            .unwrap_or("Apple Mail is not ready on this Mac.");
        let detail: String = detail
            .chars()
            .map(|character| {
                if character.is_control() {
                    ' '
                } else {
                    character
                }
            })
            .take(512)
            .collect();
        return Err(format!("Apple Mail could not be read: {detail}"));
    }
    Ok(AppleMailScanResult {
        fetched: payload.fetched,
        appended: payload.appended,
        deduplicated: payload.deduplicated,
    })
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
                fetched: 3,
                appended: 2,
                deduplicated: 1
            }
        );
    }

    #[test]
    fn treats_structured_error_and_skipped_sources_as_failures() {
        for (status, field, detail) in [
            ("error", "error", "Automation access denied"),
            ("skipped", "reason", "No Apple Mail account is configured"),
        ] {
            let payload = format!(
                r#"{{"sources":[{{"source":"apple","status":"{status}","{field}":"{detail}"}}],"fetched":0,"appended":0,"deduplicated":0}}"#,
            );
            let error = parse_scan_output(payload.as_bytes()).unwrap_err();
            assert!(error.contains(detail));
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
            let result = run_scan_process(path, timeout);
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
        assert!(result.unwrap_err().contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[cfg(unix)]
    #[test]
    fn times_out_when_a_descendant_keeps_stdout_open_after_the_parent_exits() {
        let path = runtime_script("#!/bin/sh\nsleep 10 &\nprintf '%s' '{\"sources\":[{\"source\":\"apple\",\"status\":\"scanned\"}],\"fetched\":0,\"appended\":0,\"deduplicated\":0}'\n");
        let started = Instant::now();
        let result = run_fresh_script(&path, Duration::from_millis(100));
        let _ = std::fs::remove_file(&path);
        assert!(result.unwrap_err().contains("timed out"));
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
