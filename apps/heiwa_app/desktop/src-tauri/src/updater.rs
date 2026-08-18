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
    use tauri_plugin_updater::UpdaterExt;

    let update = app
        .updater()
        .map_err(|error| error.to_string())?
        .check()
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "No update is available.".to_string())?;

    update
        .download_and_install(|_chunk, _total| {}, || {})
        .await
        .map_err(|error| error.to_string())?;

    // Diverges: the process is replaced, so nothing after this runs. The
    // relaunch is the point — an installed-but-not-running update is the
    // same stale shell with extra bytes on disk.
    app.restart()
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
