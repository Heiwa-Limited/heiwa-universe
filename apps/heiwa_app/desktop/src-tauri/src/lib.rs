pub mod herd;
pub mod onboarding;
pub mod operator_stream;
pub mod proxy;

pub fn run() {
    tauri::Builder::default()
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
            proxy::runtime_health
        ])
        .run(tauri::generate_context!())
        .expect("error while running Heiwa desktop application");
}
