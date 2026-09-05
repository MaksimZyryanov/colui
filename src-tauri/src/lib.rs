pub mod commands;
pub mod dto;
pub mod schema_generation;

use colui_adapters::{
    registry::{import_v1_if_needed, RetainedImportResult},
    runtime::{ComposeExecutionGate, ComposeProcessRunner, RuntimeGateway},
    DefinitionCache, InventoryCoordinator, JsonProfileRegistry, OperationLockManager,
    RegistryConfig, UuidGenerator,
};
use colui_app::{
    Clock, DefinitionReader, IdGenerator, LifecycleFuture, LifecycleOperation, LifecycleResult,
    LifecycleRuntime, ProfileStore, ProjectStatusFuture, ProjectStatusReader, RuntimeConnector,
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
    pub locks: Arc<OperationLockManager>,
    pub inventory: Arc<InventoryCoordinator>,
    pub definitions: Arc<DefinitionCache>,
    pub retained_import: Arc<RetainedImportResult>,
    registry_diagnostics: Arc<JsonProfileRegistry>,
    gateway: Arc<RuntimeGateway>,
    pub discovery: Arc<colui_app::DiscoverySession>,
}

impl AppState {
    async fn initialize(directory: std::path::PathBuf) -> Result<Self, colui_domain::AppError> {
        let recovery_lock = directory.join("registry.recovery.lock");
        let legacy_path = directory.join("projects.json");
        let legacy_backup_path = directory.join("projects.json.v1.bak");
        let profiles = Arc::new(JsonProfileRegistry::new(RegistryConfig::in_directory(
            directory,
        ))?);
        let runner = Arc::new(ComposeProcessRunner::default());
        let compose_gate = Arc::new(ComposeExecutionGate::new());
        let gateway = Arc::new(RuntimeGateway::with_runner_and_gate(
            runner.clone(),
            compose_gate.clone(),
        ));
        let inventory = Arc::new(InventoryCoordinator::new(
            gateway.clone(),
            Arc::new(SystemClock(std::time::Instant::now())),
        ));
        let locks = Arc::new(OperationLockManager::new(recovery_lock));
        let retained_import = Arc::new(RetainedImportResult::default());
        let import = import_v1_if_needed(
            profiles.as_ref(),
            &legacy_path,
            &legacy_backup_path,
            &UuidGenerator,
        )
        .await
        .unwrap_or_else(|error| colui_app::ImportDiagnostics {
            source_preserved: legacy_path.exists(),
            source_path: legacy_path,
            imported_count: 0,
            error: Some(error),
        });
        retained_import.retain(import);
        let definitions = Arc::new(DefinitionCache::new(
            runner,
            gateway.clone(),
            Arc::new(SystemClock(std::time::Instant::now())),
            locks.clone(),
            compose_gate,
        ));
        let runtime = Arc::new(RuntimeFacade::new(
            gateway.clone(),
            inventory.clone(),
            definitions.clone(),
        ));
        Ok(Self {
            registry_diagnostics: profiles.clone(),
            profiles,
            ids: Arc::new(UuidGenerator),
            runtime,
            locks,
            inventory,
            definitions,
            retained_import,
            gateway,
            discovery: Arc::new(colui_app::DiscoverySession::new()),
        })
    }

    pub async fn diagnostics(
        &self,
    ) -> Result<colui_app::DiagnosticsSnapshot, colui_domain::AppError> {
        use colui_app::DiagnosticsReader;
        colui_app::DiagnosticsAssembler::new(
            self.gateway.as_ref(),
            self.registry_diagnostics.as_ref(),
            self.retained_import.as_ref(),
            self.locks.as_ref(),
            self.inventory.as_ref(),
            self.definitions.as_ref(),
            self.discovery.as_ref(),
        )
        .read()
        .await
    }
}

pub struct RuntimeFacade {
    gateway: Arc<RuntimeGateway>,
    inventory: Arc<InventoryCoordinator>,
    definitions: Arc<DefinitionCache>,
}

struct SystemClock(std::time::Instant);

impl Clock for SystemClock {
    fn now(&self) -> colui_domain::Timestamp {
        colui_domain::Timestamp(chrono::Utc::now().to_rfc3339())
    }

    fn monotonic(&self) -> std::time::Duration {
        self.0.elapsed()
    }
}

impl RuntimeFacade {
    fn new(
        gateway: Arc<RuntimeGateway>,
        inventory: Arc<InventoryCoordinator>,
        definitions: Arc<DefinitionCache>,
    ) -> Self {
        Self {
            gateway,
            inventory,
            definitions,
        }
    }
}

impl RuntimeConnector for RuntimeFacade {
    fn connect_runtime(
        &self,
        preference: Option<colui_domain::DockerEndpoint>,
    ) -> colui_app::RuntimeFuture<'_, colui_domain::RuntimeSessionState> {
        self.gateway.connect_runtime(preference)
    }
    fn disconnect_runtime(&self) -> colui_app::RuntimeFuture<'_, ()> {
        self.gateway.disconnect_runtime()
    }
    fn reconnect_runtime(
        &self,
        preference: Option<colui_domain::DockerEndpoint>,
    ) -> colui_app::RuntimeFuture<'_, colui_domain::RuntimeSessionState> {
        self.gateway.reconnect_runtime(preference)
    }
}
impl RuntimeStateReader for RuntimeFacade {
    fn session_state(&self) -> colui_app::RuntimeFuture<'_, colui_domain::RuntimeSessionState> {
        self.gateway.session_state()
    }
}
impl ProjectStatusReader for RuntimeFacade {
    fn project_status(
        &self,
        profile: colui_domain::ProjectProfile,
        registry: colui_app::RegistrySnapshot,
    ) -> ProjectStatusFuture<'_> {
        let inventory = self.inventory.clone();
        let definitions = self.definitions.clone();
        Box::pin(async move {
            let inventory_snapshot = inventory.current_inventory().await?;
            let definition = if inventory_snapshot.has_snapshot {
                Some(definitions.definition(profile.clone()).await?.definition)
            } else {
                None
            };
            Ok(
                colui_app::project_status_from_registry_inventory_and_definition(
                    &profile,
                    &registry,
                    inventory_snapshot,
                    definition,
                ),
            )
        })
    }
}
impl LifecycleRuntime for RuntimeFacade {
    fn run_profile(
        &self,
        profile: colui_domain::ProjectProfile,
        operation: LifecycleOperation,
    ) -> LifecycleFuture<'_, LifecycleResult> {
        self.gateway.run_profile(profile, operation)
    }
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let directory = tauri::Manager::path(app).app_data_dir()?;
            let state = tauri::async_runtime::block_on(AppState::initialize(directory))
                .map_err(|error| std::io::Error::other(error.message))?;
            tauri::Manager::manage(app, state);
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
            commands::inventory::get_inventory,
            commands::inventory::refresh_inventory,
            commands::definitions::get_project_details,
            commands::definitions::refresh_project_definition,
            commands::lifecycle::apply_project,
            commands::lifecycle::stop_project,
            commands::lifecycle::tear_down_project,
            commands::lifecycle::restart_project
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod diagnostics_tests {
    use super::*;

    #[test]
    fn startup_import_is_retained_in_real_application_diagnostics() {
        let directory = std::env::temp_dir().join(format!(
            "colui-diagnostics-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let legacy = directory.join("projects.json");
        std::fs::write(
            &legacy,
            br#"[{"name":"demo","working_dir":"/workspace","config_files":["compose.yml"]}]"#,
        )
        .unwrap();
        tauri::async_runtime::block_on(async {
            let state = AppState::initialize(directory.clone()).await.unwrap();
            let canonical = std::fs::read(directory.join("registry.json")).unwrap();
            let first = state.diagnostics().await.unwrap();
            assert_eq!(first.import.imported_count, 1);
            assert_eq!(first.import.source_path, legacy);
            assert!(first.import.source_preserved);
            assert!(first.definitions.profiles.is_empty());
            assert_eq!(first.inventory.generation, 0);
            assert!(first.operations.active.is_empty());
            assert_eq!(
                first.runtime.state,
                colui_domain::RuntimeSessionState::Disconnected
            );
            std::fs::write(&legacy, b"changed after startup").unwrap();
            let second = state.diagnostics().await.unwrap();
            assert_eq!(second.import, first.import);
            assert_eq!(
                std::fs::read(directory.join("registry.json")).unwrap(),
                canonical
            );
            assert!(!directory.join("registry.recovery.lock").exists());
        });
        std::fs::remove_dir_all(directory).unwrap();
    }
}
