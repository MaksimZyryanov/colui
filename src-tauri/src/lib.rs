pub mod commands;
pub mod dto;
pub mod schema_generation;

use colui_adapters::{
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
            let profiles = Arc::new(
                JsonProfileRegistry::new(RegistryConfig::in_directory(directory))
                    .map_err(|error| std::io::Error::other(error.message))?,
            );
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
            let locks = Arc::new(OperationLockManager::new());
            let definitions = Arc::new(DefinitionCache::new(
                runner,
                gateway.clone(),
                Arc::new(SystemClock(std::time::Instant::now())),
                locks.clone(),
                compose_gate,
            ));
            let runtime: Arc<dyn RuntimePort> = Arc::new(RuntimeFacade::new(
                gateway,
                inventory.clone(),
                definitions.clone(),
            ));
            tauri::Manager::manage(
                app,
                AppState {
                    profiles,
                    ids: Arc::new(UuidGenerator),
                    runtime,
                    locks,
                    inventory,
                    definitions,
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
