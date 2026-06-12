pub mod proxy;

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            proxy::api_get,
            proxy::api_post,
            proxy::runtime_health
        ])
        .run(tauri::generate_context!())
        .expect("error while running Heiwa desktop application");
}
