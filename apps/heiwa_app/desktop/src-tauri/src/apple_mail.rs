use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Mutex, TryLockError};
use std::time::{Duration, Instant};
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

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // The CLI launches osascript. A separate process group lets timeout
        // cleanup stop both rather than leaving an Automation read behind.
        unsafe {
            command.pre_exec(|| {
                if libc::setpgid(0, 0) == 0 {
                    Ok(())
                } else {
                    Err(std::io::Error::last_os_error())
                }
            });
        }
    }

    let mut child = command
        .spawn()
        .map_err(|_| "Apple Mail reading could not start.".to_string())?;
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            terminate_process_tree(&mut child);
            return Err("Apple Mail reading could not capture its result.".to_string());
        }
    };
    let (output_tx, output_rx) = std::sync::mpsc::sync_channel(1);
    let _reader = std::thread::spawn(move || {
        let mut stdout = stdout;
        let mut bytes = Vec::new();
        let mut chunk = [0_u8; 4096];
        loop {
            match stdout.read(&mut chunk) {
                Ok(0) => {
                    let _ = output_tx.send(Ok(bytes));
                    return;
                }
                Ok(count) if bytes.len() + count <= MAX_RESULT_BYTES => {
                    bytes.extend_from_slice(&chunk[..count]);
                }
                Ok(_) => {
                    let _ = output_tx.send(Err(
                        "The Heiwa runtime returned an oversized Apple Mail result.".to_string(),
                    ));
                    return;
                }
                Err(_) => {
                    let _ = output_tx.send(Err(
                        "Apple Mail reading could not capture its result.".to_string()
                    ));
                    return;
                }
            }
        }
    });
    let started = Instant::now();
    let mut status = None;
    let mut output = None;
    loop {
        match output_rx.try_recv() {
            Ok(Ok(bytes)) => output = Some(bytes),
            Ok(Err(error)) => {
                terminate_process_tree(&mut child);
                return Err(error);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) if output.is_none() => {
                terminate_process_tree(&mut child);
                return Err("Apple Mail reading could not capture its result.".to_string());
            }
            Err(_) => {}
        }
        match child.try_wait() {
            Ok(Some(next)) => status = Some(next),
            Ok(None) => {}
            Err(_) => {
                terminate_process_tree(&mut child);
                return Err("Apple Mail reading stopped unexpectedly.".to_string());
            }
        }
        if status.is_some() && output.is_some() {
            break;
        }
        if started.elapsed() >= timeout {
            terminate_process_tree(&mut child);
            return Err("Apple Mail did not respond before the read timed out.".to_string());
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let status = status.expect("checked above");
    let output = output.expect("checked above");
    if !status.success() {
        return Err(
            "Apple Mail could not be read. Check Automation access in System Settings.".to_string(),
        );
    }
    parse_scan_output(&output)
}

fn terminate_process_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    unsafe {
        libc::killpg(child.id() as libc::pid_t, libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
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

    #[cfg(unix)]
    #[test]
    fn times_out_a_hung_runtime_process() {
        use std::os::unix::fs::PermissionsExt;
        let path = std::env::temp_dir().join(format!("heiwa-mail-scan-{}", uuid::Uuid::new_v4()));
        std::fs::write(&path, "#!/bin/sh\nsleep 10\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        let started = Instant::now();
        let result = run_scan_process(&path, Duration::from_millis(50));
        let _ = std::fs::remove_file(&path);
        assert!(result.unwrap_err().contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[cfg(unix)]
    #[test]
    fn times_out_when_a_descendant_keeps_stdout_open_after_the_parent_exits() {
        use std::os::unix::fs::PermissionsExt;
        let path = std::env::temp_dir().join(format!("heiwa-mail-scan-{}", uuid::Uuid::new_v4()));
        std::fs::write(&path, "#!/bin/sh\nsleep 10 &\nprintf '%s' '{\"sources\":[{\"source\":\"apple\",\"status\":\"scanned\"}],\"fetched\":0,\"appended\":0,\"deduplicated\":0}'\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        let started = Instant::now();
        let result = run_scan_process(&path, Duration::from_millis(100));
        let _ = std::fs::remove_file(&path);
        assert!(result.unwrap_err().contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_oversized_runtime_output() {
        use std::os::unix::fs::PermissionsExt;
        let path = std::env::temp_dir().join(format!("heiwa-mail-scan-{}", uuid::Uuid::new_v4()));
        std::fs::write(&path, "#!/bin/sh\nyes x | head -c 70000\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        let result = run_scan_process(&path, Duration::from_secs(2));
        let _ = std::fs::remove_file(&path);
        assert!(result.unwrap_err().contains("oversized"));
    }
}
