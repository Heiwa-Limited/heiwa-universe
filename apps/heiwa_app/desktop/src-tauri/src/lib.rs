pub mod apple_mail;
pub mod herd;
pub mod onboarding;
pub mod operator_stream;
pub mod operator_subscriptions;
pub mod proxy;
pub mod runtime_supervisor;
pub mod updater;

use std::sync::Mutex;
use tauri::Manager;

/// The runtime this app started, if it started one.
///
/// Held in Tauri state so application exit can stop it. An adopted runtime is
/// never stored here, which is what keeps closing the window from killing a
/// server the user started themselves.
struct SupervisedRuntime(Mutex<Option<runtime_supervisor::OwnedRuntime>>);

/// What the app decided about the runtime at startup.
///
/// The shell reads this to tell the user why nothing works, rather than
/// leaving them with an empty window and a spinner.
struct RuntimeStartup(runtime_supervisor::SupervisorDecision);

#[tauri::command]
fn runtime_startup(
    state: tauri::State<'_, RuntimeStartup>,
) -> runtime_supervisor::SupervisorDecision {
    state.0.clone()
}

pub fn run() {
    tauri::Builder::default()
        .manage(operator_subscriptions::OperatorSubscriptions::default())
        .setup(|app| {
            // The shipped bundle updates itself. Without this the runtime
            // could advance through `heiwa app update` while the shell it
            // lives in stayed on whatever version was first installed.
            // Registering only makes an update fetchable — `updater::
            // update_check` and `updater::update_install` are what actually
            // apply one, asked for by the shell on open.
            #[cfg(desktop)]
            app.handle()
                .plugin(tauri_plugin_updater::Builder::new().build())?;

            // Own the runtime. An installable app cannot ask the user to
            // start a server first, and it must not kill one they already
            // started — `ensure_runtime` distinguishes the two.
            let executable_dir = std::env::current_exe()
                .ok()
                .and_then(|path| path.parent().map(std::path::Path::to_path_buf));
            // Tauri resolves the resource directory per platform; hardcoding
            // the layouts gets Linux wrong, where a deb installs resources to
            // usr/lib/<product> rather than beside the executable.
            let resource_dir = app.path().resource_dir().ok();
            let binary = runtime_supervisor::find_runtime_binary(
                resource_dir.as_deref(),
                executable_dir.as_deref(),
                which_on_path,
                |path| path.is_file(),
            );

            // A downloaded app is also a CLI installation. Only packaged
            // resources may provision it; a PATH fallback in tauri dev may
            // belong to another checkout and must not silently replace it.
            let prepare = || -> Result<(), String> {
                if let (Some(resource_dir), Some(binary)) = (&resource_dir, &binary) {
                    if binary.starts_with(resource_dir) {
                        let root = heiwa_install::try_get_heiwa_dir()
                            .ok_or("The local Heiwa directory could not be resolved.")?;
                        heiwa_install::install_runtime_binary(&root, binary).map_err(|error| {
                            format!("The bundled CLI could not be installed: {error}")
                        })?;
                    }
                }
                heiwa_core::config::ensure_desktop_machine_auth().map_err(|error| error.to_string())
            };
            let (decision, owned) = match prepare() {
                Ok(()) => runtime_supervisor::ensure_runtime(
                    proxy::runtime_identity_confirmed,
                    proxy::runtime_is_reachable,
                    runtime_supervisor::spawn_runtime,
                    binary,
                ),
                Err(error) => (
                    runtime_supervisor::SupervisorDecision::Unavailable {
                        detail: format!(
                            "Local runtime authentication could not be prepared: {error}"
                        ),
                    },
                    None,
                ),
            };

            app.manage(RuntimeStartup(decision));
            app.manage(SupervisedRuntime(Mutex::new(owned)));
            Ok(())
        })
        .on_window_event(|window, event| {
            #[cfg(target_os = "macos")]
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Standard Mac close behavior: work belongs to the app, and
                // reopening from the Dock should restore the same window.
                if window.hide().is_ok() {
                    api.prevent_close();
                }
            }
            if matches!(event, tauri::WindowEvent::Destroyed) {
                window
                    .state::<operator_subscriptions::OperatorSubscriptions>()
                    .cancel_window(window.label());
            }
        })
        .invoke_handler(tauri::generate_handler![
            herd::herd_command_catalog,
            herd::herd_pane_focus,
            herd::herd_pane_read,
            herd::herd_pane_run,
            herd::herd_pane_send,
            herd::herd_pane_split,
            herd::herd_panes,
            apple_mail::apple_mail_scan,
            onboarding::establish_identity,
            onboarding::onboarding_state,
            onboarding::complete_workspace_setup,
            onboarding::open_resource_guide,
            onboarding::connect_api_provider,
            onboarding::verify_api_provider,
            onboarding::disconnect_api_provider,
            operator_stream::operator_subscribe,
            operator_stream::operator_unsubscribe,
            proxy::api_get,
            proxy::api_post,
            proxy::runtime_health,
            runtime_startup,
            updater::update_check,
            updater::update_install
        ])
        .build(tauri::generate_context!())
        .expect("error while building Heiwa desktop application")
        .run(|app, event| {
            #[cfg(target_os = "macos")]
            if matches!(event, tauri::RunEvent::Reopen { .. }) {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
            }
            // Windows own observations; the application owns the child.
            // Closing one window must not stop work viewed by another.
            if matches!(event, tauri::RunEvent::Exit) {
                if let Some(state) = app.try_state::<SupervisedRuntime>() {
                    if let Ok(mut guard) = state.0.lock() {
                        if let Some(runtime) = guard.take() {
                            runtime.shutdown();
                        }
                    }
                }
            }
        });
}

/// First match for `name` on `PATH`.
///
/// Deliberately not `heiwa_provider::resolve_command`: that also probes
/// Heiwa's own install locations, and here the question is narrower — the
/// bundle was already checked, so this is only the developer fallback.
pub(crate) fn which_on_path(name: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}
