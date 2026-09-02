fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            colui_tauri_lib::commands::profiles::list_profiles,
            colui_tauri_lib::commands::profiles::get_profile,
            colui_tauri_lib::commands::profiles::inspect_profile_draft,
            colui_tauri_lib::commands::profiles::create_profile,
            colui_tauri_lib::commands::profiles::update_profile,
            colui_tauri_lib::commands::profiles::remove_profile,
            colui_tauri_lib::commands::runtime::get_runtime_state,
            colui_tauri_lib::commands::runtime::connect_runtime,
            colui_tauri_lib::commands::runtime::get_project_status,
            colui_tauri_lib::commands::lifecycle::apply_project,
            colui_tauri_lib::commands::lifecycle::stop_project,
            colui_tauri_lib::commands::lifecycle::tear_down_project,
            colui_tauri_lib::commands::lifecycle::restart_project
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
