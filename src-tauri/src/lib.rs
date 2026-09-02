pub mod commands;
pub mod dto;
pub mod schema_generation;

use colui_adapters::{
    runtime::{ComposeProcessRunner, RuntimeGateway},
    JsonProfileRegistry, RegistryConfig, UuidGenerator,
};
use colui_app::{
    IdGenerator, LifecycleRuntime, ProfileStore, ProjectStatusReader, RuntimeConnector,
    RuntimeStateReader,
};
use std::sync::Arc;

pub trait RuntimePort:
    RuntimeConnector + RuntimeStateReader + LifecycleRuntime + ProjectStatusReader
{
}
impl<T> RuntimePort for T where
    T: RuntimeConnector + RuntimeStateReader + LifecycleRuntime + ProjectStatusReader
{
}

pub struct AppState {
    pub profiles: Arc<dyn ProfileStore>,
    pub ids: Arc<dyn IdGenerator>,
    pub runtime: Arc<dyn RuntimePort>,
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let directory = tauri::Manager::path(app).app_data_dir()?;
            let profiles = Arc::new(
                JsonProfileRegistry::new(RegistryConfig::in_directory(directory))
                    .map_err(|error| std::io::Error::other(error.message))?,
            );
            let runtime = Arc::new(RuntimeGateway::new(Box::new(
                ComposeProcessRunner::default(),
            )));
            tauri::Manager::manage(
                app,
                AppState {
                    profiles,
                    ids: Arc::new(UuidGenerator),
                    runtime,
                },
            );
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::profiles::list_profiles,
            commands::profiles::get_profile,
            commands::profiles::inspect_profile_draft,
            commands::profiles::create_profile,
            commands::profiles::update_profile,
            commands::profiles::remove_profile,
            commands::runtime::get_runtime_state,
            commands::runtime::connect_runtime,
            commands::runtime::get_project_status,
            commands::lifecycle::apply_project,
            commands::lifecycle::stop_project,
            commands::lifecycle::tear_down_project,
            commands::lifecycle::restart_project
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
