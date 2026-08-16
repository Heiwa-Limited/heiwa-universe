pub mod herd;
pub mod onboarding;
pub mod operator_stream;
pub mod proxy;
pub mod runtime_supervisor;

use std::sync::Mutex;
use tauri::Manager;

/// The runtime this app started, if it started one.
///
/// Held in Tauri state so window teardown can stop it. An adopted runtime is
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
        .setup(|app| {
            // Own the runtime. An installable app cannot ask the user to
            // start a server first, and it must not kill one they already
            // started — `ensure_runtime` distinguishes the two.
            let executable_dir = std::env::current_exe()
                .ok()
                .and_then(|path| path.parent().map(std::path::Path::to_path_buf));
            let binary = runtime_supervisor::find_runtime_binary(
                executable_dir.as_deref(),
                which_on_path,
                |path| path.is_file(),
            );

            let (decision, owned) = runtime_supervisor::ensure_runtime(
                proxy::runtime_identity_confirmed,
                proxy::runtime_is_reachable,
                runtime_supervisor::spawn_runtime,
                binary,
            );

            app.manage(RuntimeStartup(decision));
            app.manage(SupervisedRuntime(Mutex::new(owned)));
            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                if let Some(state) = window.try_state::<SupervisedRuntime>() {
                    if let Ok(mut guard) = state.0.lock() {
                        if let Some(runtime) = guard.take() {
                            runtime.shutdown();
                        }
                    }
                }
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
            onboarding::establish_identity,
            onboarding::onboarding_state,
            operator_stream::operator_subscribe,
            proxy::api_get,
            proxy::api_post,
            proxy::runtime_health,
            runtime_startup
        ])
        .run(tauri::generate_context!())
        .expect("error while running Heiwa desktop application");
}

/// First match for `name` on `PATH`.
///
/// Deliberately not `heiwa_provider::resolve_command`: that also probes
/// Heiwa's own install locations, and here the question is narrower — the
/// bundle was already checked, so this is only the developer fallback.
fn which_on_path(name: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}
