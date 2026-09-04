pub mod commands;
pub mod dto;
pub mod schema_generation;

use colui_adapters::{
    runtime::{ComposeProcessRunner, RuntimeGateway},
    DefinitionCache, InventoryCoordinator, JsonProfileRegistry, OperationLockManager,
    RegistryConfig, UuidGenerator,
};
use colui_app::{
    Clock, DefinitionReader, IdGenerator, LifecycleExecutor, LifecycleFuture, LifecycleOperation,
    LifecycleResult, LifecycleRuntime, OperationLockManager as OperationLockManagerPort,
    ProfileStore, ProjectStatusFuture, ProjectStatusReader, RuntimeConnector, RuntimeStateReader,
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

pub struct RuntimeFacade {
    gateway: Arc<RuntimeGateway>,
    inventory: Arc<InventoryCoordinator>,
    locks: Arc<OperationLockManager>,
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
        locks: Arc<OperationLockManager>,
        definitions: Arc<DefinitionCache>,
    ) -> Self {
        Self {
            gateway,
            inventory,
            locks,
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
    fn project_status(&self, profile: colui_domain::ProjectProfile) -> ProjectStatusFuture<'_> {
        let inventory = self.inventory.clone();
        let definitions = self.definitions.clone();
        Box::pin(async move {
            let inventory_snapshot = inventory.current_inventory().await?;
            let definition = if inventory_snapshot.has_snapshot {
                Some(definitions.definition(profile.clone()).await?)
            } else {
                None
            };
            Ok(colui_app::project_status_from_inventory_and_definition(
                &profile,
                inventory_snapshot,
                definition,
            ))
        })
    }
}
impl LifecycleRuntime for RuntimeFacade {
    fn run_profile(
        &self,
        profile: colui_domain::ProjectProfile,
        operation: LifecycleOperation,
    ) -> LifecycleFuture<'_, LifecycleResult> {
        let gateway = self.gateway.clone();
        let inventory = self.inventory.clone();
        let locks = self.locks.clone();
        Box::pin(async move {
            let _guard = locks
                .acquire_lifecycle(profile.id.clone(), operation)
                .await?;
            let execution = gateway.execute_profile(profile, operation).await?;
            let inventory = if execution.success {
                match inventory.refresh().await {
                    Ok(inventory) => inventory,
                    Err(error) => {
                        let mut inventory = inventory.current_inventory().await?;
                        inventory.error = Some(error);
                        inventory.freshness = if inventory.has_snapshot {
                            colui_domain::InventoryFreshness::Stale
                        } else {
                            colui_domain::InventoryFreshness::Unavailable
                        };
                        inventory
                    }
                }
            } else {
                inventory.current_inventory().await?
            };
            Ok(LifecycleResult {
                profile_id: execution.profile_id,
                success: execution.success,
                inventory,
            })
        })
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
            let gateway = Arc::new(RuntimeGateway::with_runner(runner.clone()));
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
            ));
            let runtime: Arc<dyn RuntimePort> =
                Arc::new(RuntimeFacade::new(gateway, inventory, locks, definitions));
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
