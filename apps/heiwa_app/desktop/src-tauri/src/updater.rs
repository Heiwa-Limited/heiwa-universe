//! Applying a published desktop update (L2).
//!
//! Registering `tauri_plugin_updater` only makes an update *fetchable*. These
//! two commands are the reachable path: the shell asks once on open, and if
//! there is something newer it tells the user and installs on their word.
//! Without them a signed release could sit on GitHub while every installed
//! shell stayed on the version it was first downloaded at.
//!
//! Deliberately user-accepted rather than silent. The shell owns a runtime it
//! supervises, and replacing the binary means relaunching the window — that
//! is not something to do underneath someone mid-sentence.

use serde::Serialize;

/// A newer release than the one running, as the shell needs to describe it.
#[derive(Clone, Debug, Serialize)]
pub struct UpdateOffer {
    /// Version of the available release.
    pub version: String,
    /// Version this shell is running, so the offer can say what it replaces.
    pub current_version: String,
    /// Release notes, when the manifest carries them.
    pub notes: Option<String>,
}

/// Whether a newer signed release is available.
///
/// `Ok(None)` is the ordinary answer: up to date, or no reachable manifest.
#[tauri::command]
pub async fn update_check(app: tauri::AppHandle) -> Result<Option<UpdateOffer>, String> {
    available_update(&app).await
}

/// Install the available update and relaunch into it.
///
/// Called only after the user accepts. It re-checks rather than trusting a
/// handle from the earlier call, because the offer the shell is showing may
/// have been made minutes ago.
#[tauri::command]
pub async fn update_install(app: tauri::AppHandle) -> Result<(), String> {
    install_update(&app).await
}

#[cfg(desktop)]
async fn available_update(app: &tauri::AppHandle) -> Result<Option<UpdateOffer>, String> {
    use tauri_plugin_updater::UpdaterExt;

    let updater = app.updater().map_err(|error| error.to_string())?;
    let update = updater.check().await.map_err(|error| error.to_string())?;
    Ok(update.map(|update| UpdateOffer {
        version: update.version.clone(),
        current_version: update.current_version.clone(),
        notes: update.body.clone(),
    }))
}

#[cfg(desktop)]
async fn install_update(app: &tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    use tauri_plugin_updater::UpdaterExt;

    // A separately started runtime must be updated through its owning CLI.
    let owns_runtime = app
        .state::<crate::SupervisedRuntime>()
        .0
        .lock()
        .map_err(|_| "Runtime ownership could not be checked.".to_string())?
        .is_some();
    if !owns_runtime {
        return Err("The runtime was started separately. Finish its work and use `heiwa app update` to update it and the app together.".into());
    }
    require_idle_runtime().await?;

    let update = app
        .updater()
        .map_err(|error| error.to_string())?
        .check()
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "No update is available.".to_string())?;

    let bytes = update
        .download(|_chunk, _total| {}, || {})
        .await
        .map_err(|error| error.to_string())?;

    // Downloading can take time. Do not reuse the pre-download activity check.
    require_idle_runtime().await?;
    update.install(bytes).map_err(|error| error.to_string())?;
    if let Some(runtime) = app
        .state::<crate::SupervisedRuntime>()
        .0
        .lock()
        .map_err(|_| "Runtime could not be stopped for the update.".to_string())?
        .take()
    {
        runtime.shutdown();
    }

    // Diverges: the process is replaced, so nothing after this runs. The
    // relaunch is the point — an installed-but-not-running update is the
    // same stale shell with extra bytes on disk.
    app.restart()
}

#[cfg(desktop)]
async fn require_idle_runtime() -> Result<(), String> {
    let health = crate::proxy::runtime_health().await;
    let snapshot = health
        .snapshot
        .ok_or("Heiwa could not verify runtime activity. Retry when the runtime is reachable.")?;
    check_update_activity(&snapshot)
}

#[cfg(any(desktop, test))]
fn check_update_activity(snapshot: &serde_json::Value) -> Result<(), String> {
    let count = |path| {
        snapshot
            .pointer(path)
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| {
                "Runtime activity is incomplete; the update has been deferred.".to_string()
            })
    };
    let workers = count("/data/workers/task_live")?;
    let approvals = count("/data/approvals/pending")?;
    if workers > 0 || approvals > 0 {
        return Err(
            "Finish active work and resolve pending approvals before updating Heiwa.".into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn updates_require_explicit_idle_counts() {
        for snapshot in [
            json!({}),
            json!({"data":{"workers":{"task_live":0}}}),
            json!({"data":{"workers":{"task_live":1},"approvals":{"pending":0}}}),
            json!({"data":{"workers":{"task_live":0},"approvals":{"pending":1}}}),
        ] {
            assert!(check_update_activity(&snapshot).is_err());
        }
        assert!(check_update_activity(
            &json!({"data":{"workers":{"task_live":0},"approvals":{"pending":0}}})
        )
        .is_ok());
    }
}

#[cfg(not(desktop))]
async fn available_update(_app: &tauri::AppHandle) -> Result<Option<UpdateOffer>, String> {
    // Mobile has no updater plugin (the dependency is desktop-target only),
    // and no bundle this app could replace.
    Ok(None)
}

#[cfg(not(desktop))]
async fn install_update(_app: &tauri::AppHandle) -> Result<(), String> {
    Err("This platform does not install its own updates.".to_string())
}
