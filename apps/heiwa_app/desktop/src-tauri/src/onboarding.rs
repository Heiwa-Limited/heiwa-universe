//! First-run state for the desktop shell (L2).
//!
//! The shell renders whatever this returns; it does not decide readiness
//! itself. Both surfaces read `heiwa_identity::onboarding`, so the desktop
//! and the CLI cannot disagree about whether a user is set up — and a remedy
//! shown here is the same string `heiwa setup` prints.

use heiwa_identity::onboarding::{OnboardingFacts, OnboardingState};

/// What first run still needs, or that nothing does.
#[tauri::command]
pub async fn onboarding_state() -> OnboardingState {
    let paths = heiwa_config::HeiwaPaths::try_resolve();
    let identity = paths
        .as_ref()
        .and_then(|_| heiwa_identity::load().ok().flatten());

    let mut registry = heiwa_provider::AccountRegistry::load();
    heiwa_provider::detect::auto_discover(&mut registry).await;
    let fleet = heiwa_provider::health::FleetHealth::project(&registry.accounts);

    OnboardingState::project(&OnboardingFacts {
        has_state_root: paths.is_some(),
        identity: identity.as_ref().map(|id| id.display_name.as_str()),
        has_routable_account: fleet.has_routable_account(),
        provider_guidance: fleet.guidance(),
    })
}

/// Establish (or rename) this installation's local identity.
///
/// The only gap first run can close from inside the window: a provider
/// credential has to come from the user, and a state root has to exist
/// before the application starts.
#[tauri::command]
pub async fn establish_identity(display_name: String) -> Result<OnboardingState, String> {
    let created_at = chrono::Utc::now().to_rfc3339();
    let existing = heiwa_identity::load().map_err(|error| error.to_string())?;
    match existing {
        Some(identity) if identity.display_name != display_name.trim() => {
            heiwa_identity::rename(&display_name).map_err(|error| error.to_string())?;
        }
        Some(_) => {}
        None => {
            heiwa_identity::establish(&display_name, &created_at)
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(onboarding_state().await)
}
