//! Desktop setup distinguishes local workspace access from inference readiness.
mod resources;

use heiwa_identity::onboarding::{OnboardingFacts, OnboardingState};
use serde::Serialize;

#[derive(Serialize)]
pub struct DesktopOnboardingState {
    #[serde(flatten)]
    inference: OnboardingState,
    workspace: WorkspaceState,
}

#[derive(Serialize)]
struct WorkspaceState {
    can_enter: bool,
    setup_complete: bool,
    resources: Vec<resources::Resource>,
    connections: Vec<heiwa_provider::connections::ConnectionSummary>,
    cli_path: Option<String>,
}

/// Passive by default. Explicit verification may contact configured provider
/// endpoints and update their registry, but does not run model inference.
#[tauri::command]
pub async fn onboarding_state(
    verify_providers: Option<bool>,
) -> Result<DesktopOnboardingState, String> {
    let paths = heiwa_config::HeiwaPaths::try_resolve();
    let identity = match &paths {
        Some(paths) => heiwa_identity::load_from(&paths.runtime_root).map_err(|e| e.to_string())?,
        None => None,
    };
    let setup = match &paths {
        Some(paths) => heiwa_identity::workspace_setup::load_from(&paths.runtime_root)?,
        None => None,
    };
    let mut registry = if paths.is_some() {
        heiwa_provider::AccountRegistry::load_strict().map_err(|error| error.to_string())?
    } else {
        heiwa_provider::AccountRegistry::default()
    };
    if verify_providers == Some(true) {
        if paths.is_none() {
            return Err("Resolve the local state directory before verifying providers.".into());
        }
        heiwa_provider::detect::auto_discover(&mut registry).await;
        registry.save().map_err(|error| error.to_string())?;
    }
    let fleet = heiwa_provider::health::FleetHealth::project(&registry.accounts);
    Ok(DesktopOnboardingState {
        inference: OnboardingState::project(&OnboardingFacts {
            has_state_root: paths.is_some(),
            identity: identity.as_ref().map(|id| id.display_name.as_str()),
            has_routable_account: fleet.has_routable_account(),
            provider_guidance: fleet.guidance(),
        }),
        workspace: WorkspaceState {
            can_enter: paths.is_some() && identity.is_some(),
            setup_complete: setup.is_some(),
            resources: resources::discover(paths.as_ref(), &registry),
            connections: heiwa_provider::connections::summaries(&registry),
            cli_path: paths.as_ref().and_then(|paths| {
                let binary = paths.runtime_root.join("bin").join(if cfg!(windows) {
                    "heiwa.exe"
                } else {
                    "heiwa"
                });
                binary.is_file().then(|| binary.display().to_string())
            }),
        },
    })
}

#[tauri::command]
pub async fn connect_api_provider(
    provider: String,
    api_key: String,
) -> Result<DesktopOnboardingState, String> {
    heiwa_provider::connections::connect_api_key(&provider, &api_key)
        .await
        .map_err(|error| error.to_string())?;
    onboarding_state(None).await
}

#[tauri::command]
pub async fn verify_api_provider(account_id: String) -> Result<DesktopOnboardingState, String> {
    heiwa_provider::connections::verify_api_connection(&account_id)
        .await
        .map_err(|error| error.to_string())?;
    onboarding_state(None).await
}

#[tauri::command]
pub async fn disconnect_api_provider(account_id: String) -> Result<DesktopOnboardingState, String> {
    heiwa_provider::connections::disconnect_api_connection(&account_id)
        .map_err(|error| error.to_string())?;
    onboarding_state(None).await
}

#[tauri::command]
pub async fn establish_identity(display_name: String) -> Result<DesktopOnboardingState, String> {
    let name = display_name.trim();
    if name.is_empty() || name.chars().count() > 120 {
        return Err("Enter a name between 1 and 120 characters.".into());
    }
    let created_at = chrono::Utc::now().to_rfc3339();
    match heiwa_identity::load().map_err(|error| error.to_string())? {
        Some(identity) if identity.display_name != name => {
            heiwa_identity::rename(name).map_err(|error| error.to_string())?;
        }
        Some(_) => {}
        None => {
            heiwa_identity::establish(name, &created_at).map_err(|error| error.to_string())?;
        }
    }
    onboarding_state(None).await
}

#[tauri::command]
pub async fn complete_workspace_setup() -> Result<DesktopOnboardingState, String> {
    let paths = heiwa_config::HeiwaPaths::try_resolve().ok_or("No local state directory.")?;
    heiwa_identity::workspace_setup::complete_in(&paths.runtime_root)?;
    onboarding_state(None).await
}

/// UI input selects a catalog entry, never an arbitrary URL or shell command.
#[tauri::command]
pub async fn open_resource_guide(resource_id: String) -> Result<(), String> {
    let url = resources::guide_for(&resource_id).ok_or("No setup guide for this resource.")?;
    #[cfg(target_os = "macos")]
    {
        tauri::async_runtime::spawn_blocking(move || {
            let status = std::process::Command::new("/usr/bin/open")
                .arg(url)
                .status()
                .map_err(|error| error.to_string())?;
            if status.success() {
                Ok(())
            } else {
                Err("macOS could not open the setup guide.".into())
            }
        })
        .await
        .map_err(|error| error.to_string())?
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = url;
        Err("Resource setup guides currently require macOS.".into())
    }
}
