//! The app owns its native folder picker; the bundled runtime owns grants.
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

static PICKING: AtomicBool = AtomicBool::new(false);
struct PickerGuard;
impl Drop for PickerGuard {
    fn drop(&mut self) {
        PICKING.store(false, Ordering::Release);
    }
}

fn valid_thread_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 256
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b))
}

#[tauri::command]
pub async fn select_workspace_folder(
    app: tauri::AppHandle,
    thread_id: String,
) -> Result<Value, String> {
    if !valid_thread_id(&thread_id) {
        return Err("Invalid conversation ID".into());
    }
    if PICKING.swap(true, Ordering::AcqRel) {
        return Err("A folder picker is already open".into());
    }
    let _guard = PickerGuard;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title("Choose a workspace for Heiwa")
        .pick_folder(move |selection| {
            let _ = tx.send(selection);
        });
    let Some(selection) = rx
        .await
        .map_err(|_| "Folder selection ended unexpectedly")?
    else {
        return Ok(json!({"cancelled":true}));
    };
    let path = selection.into_path().map_err(|_| "Choose a local folder")?;
    let resources = app
        .path()
        .resource_dir()
        .map_err(|_| "Heiwa resources are unavailable")?;
    // Never discover a different checkout's CLI through PATH for a native grant.
    let binary = crate::runtime_supervisor::find_runtime_binary(
        Some(&resources),
        None,
        |_| None,
        |p| p.is_file(),
    )
    .filter(|binary| binary.starts_with(&resources))
    .ok_or("A matching bundled runtime is required")?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut command = std::process::Command::new(binary);
        command
            .args(["app", "grant-workspace", &thread_id])
            .arg(path)
            .arg("--json");
        let output = heiwa_core::subprocess::bounded_output(
            &mut command,
            std::time::Duration::from_secs(15),
            256 * 1024,
        )?;
        let value: Value =
            serde_json::from_slice(&output).map_err(|_| "Invalid workspace grant response")?;
        if value["ok"] != true || !value["data"]["workspace"].is_string() {
            return Err("The workspace grant could not be verified".into());
        }
        Ok(value["data"].clone())
    })
    .await
    .map_err(|_| "Workspace selection task failed")?
}

#[cfg(test)]
mod tests {
    #[test]
    fn picker_thread_id_cannot_change_cli_or_path_shape() {
        assert!(super::valid_thread_id("thread-1234"));
        for id in ["", "../escape", "a/b", "a\n"] {
            assert!(!super::valid_thread_id(id));
        }
    }
}
