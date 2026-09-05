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

pub struct ApplicationEvents {
    sequence: std::sync::Mutex<u64>,
    emit: Box<dyn Fn(dto::ApplicationStateChangedDto) + Send + Sync>,
}

impl ApplicationEvents {
    pub fn new(emit: impl Fn(dto::ApplicationStateChangedDto) + Send + Sync + 'static) -> Self {
        Self {
            sequence: std::sync::Mutex::new(0),
            emit: Box::new(emit),
        }
    }

    pub fn publish(&self, scopes: &[dto::ApplicationStateScopeDto]) {
        // Serialize allocation and delivery so concurrent commands cannot emit out of order.
        let mut sequence = self.sequence.lock().expect("event sequence poisoned");
        *sequence = sequence.checked_add(1).expect("event sequence exhausted");
        (self.emit)(dto::ApplicationStateChangedDto {
            sequence: *sequence,
            scopes: scopes.to_vec(),
        });
    }
}

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
    pub events: Arc<ApplicationEvents>,
}

// A session-filtered view, not an inventory cache or refresh owner. Diagnostics
// continues to receive the coordinator's unmodified retained snapshot.
struct DiscoveryInventory {
    inventory: Arc<InventoryCoordinator>,
    gateway: Arc<RuntimeGateway>,
}

impl colui_app::InventoryReader for DiscoveryInventory {
    fn current_inventory(&self) -> colui_app::InventoryFuture<'_, colui_domain::RuntimeInventory> {
        Box::pin(async move {
            let inventory = self.inventory.current_inventory().await?;
            let session = self
                .gateway
                .api_read_context()
                .await
                .ok()
                .map(|v| v.session_id);
            if session.is_some() && inventory.runtime_session_id == session {
                Ok(inventory)
            } else {
                Ok(colui_domain::RuntimeInventory {
                    generation: inventory.generation,
                    ..colui_domain::RuntimeInventory::unavailable()
                })
            }
        })
    }
}

impl colui_app::DiscoveryReader for DiscoveryInventory {
    fn lease_candidate(
        &self,
        session: colui_domain::RuntimeSessionId,
        generation: u64,
    ) -> colui_app::DiscoveryFuture<'_, colui_app::CandidateLease> {
        Box::pin(async move {
            let lease = colui_app::DiscoveryReader::lease_candidate(
                self.inventory.as_ref(),
                session,
                generation,
            )
            .await?;
            let valid = self.gateway.api_session_validator(session).await?;
            Ok(colui_app::CandidateLease::hold_validated(lease, valid))
        })
    }
}

impl AppState {
    fn discovery_inventory(&self) -> DiscoveryInventory {
        DiscoveryInventory {
            inventory: self.inventory.clone(),
            gateway: self.gateway.clone(),
        }
    }
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
            events: Arc::new(ApplicationEvents::new(|_| {})),
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

    pub(crate) async fn journaled<T>(
        &self,
        kinds: [colui_app::JournalEventKind; 3],
        subject: Option<colui_domain::AppErrorSubject>,
        scopes: &[dto::ApplicationStateScopeDto],
        work: impl std::future::Future<Output = Result<T, colui_domain::AppError>>,
    ) -> Result<T, dto::AppErrorDto> {
        let session = self
            .gateway
            .api_read_context()
            .await
            .ok()
            .map(|v| v.session_id);
        self.discovery
            .record_operation(kinds[0], session, subject.clone(), None)
            .await;
        self.events
            .publish(&[dto::ApplicationStateScopeDto::Diagnostics]);
        let result = work.await;
        if matches!(
            kinds[0],
            colui_app::JournalEventKind::ConnectStarted
                | colui_app::JournalEventKind::DisconnectStarted
                | colui_app::JournalEventKind::ReconnectStarted
        ) {
            let current = self
                .gateway
                .api_read_context()
                .await
                .ok()
                .map(|v| v.session_id);
            self.discovery.observe_runtime_session(current).await;
        }
        let originating_session = session.or(self
            .gateway
            .api_read_context()
            .await
            .ok()
            .map(|v| v.session_id));
        self.discovery
            .record_operation(
                if result.is_ok() { kinds[1] } else { kinds[2] },
                originating_session,
                subject,
                result.as_ref().err(),
            )
            .await;
        self.events.publish(scopes);
        result.map_err(Into::into)
    }

    fn start_auto_worker(&self) {
        let mut scheduled = self.inventory.start_auto_registration_scheduler(
            self.registry_diagnostics.clone(),
            self.discovery.clone(),
        );
        let inventory = self.discovery_inventory();
        let registry = self.profiles.clone();
        let ids = self.ids.clone();
        let locks = self.locks.clone();
        let discovery = self.discovery.clone();
        let events = self.events.clone();
        tauri::async_runtime::spawn(async move {
            while let Some(schedule) = scheduled.recv().await {
                let results = colui_app::AutoRegisterCandidates::new(
                    &inventory,
                    registry.as_ref(),
                    ids.as_ref(),
                    locks.as_ref(),
                    discovery.as_ref(),
                )
                .execute(schedule)
                .await;
                if !results.is_empty() {
                    use dto::ApplicationStateScopeDto as S;
                    events.publish(if results.iter().any(Result::is_ok) {
                        &[S::Profiles, S::Discovery, S::Diagnostics]
                    } else {
                        &[S::Diagnostics]
                    });
                }
            }
        });
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
            let mut state = tauri::async_runtime::block_on(AppState::initialize(directory))
                .map_err(|error| std::io::Error::other(error.message))?;
            let handle = app.handle().clone();
            state.events = Arc::new(ApplicationEvents::new(move |event| {
                use tauri::Emitter;
                let _ = handle.emit("application_state_changed", event);
            }));
            tauri::async_runtime::block_on(async {
                state.start_auto_worker();
            });
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
            commands::runtime::disconnect_runtime,
            commands::runtime::reconnect_runtime,
            commands::discovery::list_discovery_candidates,
            commands::discovery::ignore_candidate,
            commands::discovery::configure_auto_registration,
            commands::discovery::get_auto_registration_configuration,
            commands::discovery::register_candidate,
            commands::discovery::auto_register_candidates,
            commands::diagnostics::get_diagnostics,
            commands::diagnostics::create_registry_backup,
            commands::diagnostics::restore_registry_backup,
            commands::containers::run_container_action,
            commands::containers::get_container_logs,
            commands::containers::open_container_port,
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

    struct DiscoveryDocker;
    impl colui_adapters::runtime::DockerControl for DiscoveryDocker {
        fn info(&self) -> colui_app::RuntimeFuture<'_, colui_domain::DaemonFingerprint> {
            Box::pin(async {
                Ok(colui_domain::DaemonFingerprint::new(
                    "same", "1", "linux", "x86_64",
                ))
            })
        }
        fn list(&self) -> colui_app::RuntimeFuture<'_, Vec<colui_domain::ContainerObservation>> {
            Box::pin(async {
                Ok(vec![colui_domain::ContainerObservation::new(
                    colui_domain::ContainerInstance {
                        id: colui_domain::ContainerId("a".repeat(64)),
                        name: "SECRET_LABEL".into(),
                        image: "SECRET_IMAGE".into(),
                        state: colui_domain::ContainerState::Running,
                        status_text: "running".into(),
                        service_name: None,
                        published_ports: vec![],
                    },
                    Some(colui_domain::ComposeContainerMetadata {
                        project: "demo".into(),
                        working_directory: Some("/SECRET_PATH".into()),
                        config_files: vec!["compose.yml".into()],
                        service: None,
                    }),
                )])
            })
        }
        fn inspect(
            &self,
            _: &colui_domain::ContainerId,
        ) -> colui_app::RuntimeFuture<'_, colui_domain::ContainerDetails> {
            panic!("discovery must not inspect")
        }
        fn action(
            &self,
            _: colui_domain::ContainerId,
            _: colui_app::ContainerAction,
        ) -> colui_app::RuntimeFuture<'_, ()> {
            panic!("discovery must not mutate Docker")
        }
        fn logs(
            &self,
            _: colui_domain::ContainerId,
        ) -> colui_app::RuntimeFuture<'_, colui_app::ContainerLogs> {
            panic!("discovery must not read logs")
        }
    }
    struct DiscoveryRunner;
    impl colui_app::ComposeRunner for DiscoveryRunner {
        fn invoke(
            &self,
            invocation: colui_app::ComposeInvocation,
        ) -> colui_app::RuntimeFuture<'_, colui_app::ComposeProcessResult> {
            assert_eq!(invocation.args, vec!["info"]);
            Box::pin(async {
                Ok(colui_app::ComposeProcessResult::completed(
                    0,
                    "ID: same\nServer Version: 1\nOSType: linux\nArchitecture: x86_64\n",
                    "",
                    std::time::Duration::ZERO,
                ))
            })
        }
    }

    #[test]
    fn disconnected_discovery_does_not_revive_retained_candidates_or_register_them() {
        use tauri::Manager;
        let directory =
            std::env::temp_dir().join(format!("colui-session-command-test-{}", std::process::id()));
        let mut state =
            tauri::async_runtime::block_on(AppState::initialize(directory.clone())).unwrap();
        state.gateway = Arc::new(RuntimeGateway::new_for_tests(
            Box::new(DiscoveryDocker),
            Box::new(DiscoveryRunner),
        ));
        state.inventory = Arc::new(InventoryCoordinator::new(
            state.gateway.clone(),
            Arc::new(SystemClock(std::time::Instant::now())),
        ));
        state.runtime = Arc::new(RuntimeFacade::new(
            state.gateway.clone(),
            state.inventory.clone(),
            state.definitions.clone(),
        ));
        let app = tauri::test::mock_builder()
            .manage(state)
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        tauri::async_runtime::block_on(async {
            commands::runtime::connect_runtime(app.state())
                .await
                .unwrap();
            app.state::<AppState>().inventory.refresh().await.unwrap();
            let candidates = commands::discovery::list_discovery_candidates(app.state())
                .await
                .unwrap()
                .candidates;
            assert_eq!(candidates.len(), 1);
            let candidate = candidates.into_iter().next().unwrap();
            let reconnected = commands::runtime::reconnect_runtime(app.state())
                .await
                .unwrap();
            assert!(reconnected.inventory.is_some());
            assert!(!commands::discovery::ignore_candidate(
                dto::IgnoreCandidateRequestDto {
                    candidate_id: candidate.candidate_id.clone()
                },
                app.state()
            )
            .await
            .unwrap());
            let candidate = commands::discovery::list_discovery_candidates(app.state())
                .await
                .unwrap()
                .candidates
                .into_iter()
                .next()
                .unwrap();
            let lease = colui_app::DiscoveryReader::lease_candidate(
                &app.state::<AppState>().discovery_inventory(),
                dto::decode_session_id(candidate.runtime_session_id.clone()).unwrap(),
                candidate.inventory_generation,
            )
            .await
            .unwrap();
            assert!(lease.is_valid());
            commands::runtime::disconnect_runtime(app.state())
                .await
                .unwrap();
            assert!(!lease.is_valid());
            drop(lease);
            assert!(commands::discovery::list_discovery_candidates(app.state())
                .await
                .unwrap()
                .candidates
                .is_empty());
            let result = commands::discovery::register_candidate(
                dto::RegisterCandidateRequestDto {
                    candidate_id: candidate.candidate_id,
                    runtime_session_id: candidate.runtime_session_id,
                    inventory_generation: candidate.inventory_generation,
                    compose_project_name: candidate.compose_project_name,
                    working_directory: candidate.working_directory,
                    config_files: candidate.config_files,
                },
                app.state(),
            )
            .await;
            assert_eq!(
                result.unwrap_err().code,
                dto::AppErrorCodeDto::CandidateStale
            );
            assert!(commands::profiles::list_profiles(app.state())
                .await
                .unwrap()
                .is_empty());
        });
        drop(app);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn production_auto_worker_registers_after_publication_and_emits_invalidations() {
        use tauri::Manager;
        let directory =
            std::env::temp_dir().join(format!("colui-auto-worker-test-{}", std::process::id()));
        let mut state =
            tauri::async_runtime::block_on(AppState::initialize(directory.clone())).unwrap();
        state.gateway = Arc::new(RuntimeGateway::new_for_tests(
            Box::new(DiscoveryDocker),
            Box::new(DiscoveryRunner),
        ));
        state.inventory = Arc::new(InventoryCoordinator::new(
            state.gateway.clone(),
            Arc::new(SystemClock(std::time::Instant::now())),
        ));
        state.runtime = Arc::new(RuntimeFacade::new(
            state.gateway.clone(),
            state.inventory.clone(),
            state.definitions.clone(),
        ));
        let (sent, received) = std::sync::mpsc::channel();
        state.events = Arc::new(ApplicationEvents::new(move |event| {
            if event
                .scopes
                .contains(&dto::ApplicationStateScopeDto::Profiles)
            {
                sent.send(event).unwrap();
            }
        }));
        let app = tauri::test::mock_builder()
            .manage(state)
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        tauri::async_runtime::block_on(async {
            commands::runtime::connect_runtime(app.state())
                .await
                .unwrap();
            commands::discovery::configure_auto_registration(
                dto::ConfigureAutoRegistrationRequestDto { enabled: true },
                app.state(),
            )
            .await
            .unwrap();
            app.state::<AppState>().start_auto_worker();
            app.state::<AppState>().inventory.refresh().await.unwrap();
        });
        let event = received
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("worker must publish registration invalidation");
        assert_eq!(
            event.scopes,
            vec![
                dto::ApplicationStateScopeDto::Profiles,
                dto::ApplicationStateScopeDto::Discovery,
                dto::ApplicationStateScopeDto::Diagnostics
            ]
        );
        tauri::async_runtime::block_on(async {
            let profiles = commands::profiles::list_profiles(app.state())
                .await
                .unwrap();
            assert_eq!(profiles.len(), 1);
            assert_eq!(
                profiles[0].registration_origin,
                dto::RegistrationOriginDto::Discovered
            );
            let diagnostics = commands::diagnostics::get_diagnostics(app.state())
                .await
                .unwrap();
            assert_eq!(diagnostics.inventory.generation, 1);
            assert_eq!(
                diagnostics.journal.entries.last().unwrap().kind,
                dto::JournalEventKindDto::AutoRegistrationSucceeded
            );
            assert!(!serde_json::to_string(&diagnostics.journal)
                .unwrap()
                .contains("SECRET"));
            commands::discovery::configure_auto_registration(
                dto::ConfigureAutoRegistrationRequestDto { enabled: false },
                app.state(),
            )
            .await
            .unwrap();
        });
        drop(app);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn commands_use_single_owners_and_reads_do_not_journal_or_emit() {
        use dto::*;
        use tauri::Manager;
        let directory =
            std::env::temp_dir().join(format!("colui-command-test-{}", std::process::id()));
        let emitted = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = emitted.clone();
        let mut state =
            tauri::async_runtime::block_on(AppState::initialize(directory.clone())).unwrap();
        state.events = Arc::new(ApplicationEvents::new(move |event| {
            sink.lock().unwrap().push(event)
        }));
        let app = tauri::test::mock_builder()
            .manage(state)
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        tauri::async_runtime::block_on(async {
            let first = commands::diagnostics::get_diagnostics(app.state())
                .await
                .unwrap();
            assert!(first.registry.health.identity.is_none());
            assert_eq!(
                serde_json::to_value(&first).unwrap()["registry"]["backup"]["state"],
                "missing"
            );
            assert!(commands::discovery::list_discovery_candidates(app.state())
                .await
                .unwrap()
                .candidates
                .is_empty());
            assert_eq!(
                commands::runtime::get_runtime_state(app.state())
                    .await
                    .unwrap(),
                RuntimeStateDto::Disconnected
            );
            assert!(emitted.lock().unwrap().is_empty());
            assert!(first.journal.entries.is_empty());

            let draft = serde_json::from_value(serde_json::json!({
                "displayName":"Demo", "composeProjectName":"demo", "workingDirectory":"/SECRET_PATH",
                "composeFiles":["SECRET_COMPOSE.yml"], "environmentFiles":["SECRET_ENV"], "registrationOrigin":"manual"
            })).unwrap();
            let profile = commands::profiles::create_profile(draft, app.state())
                .await
                .unwrap();
            assert!(emitted
                .lock()
                .unwrap()
                .last()
                .unwrap()
                .scopes
                .contains(&ApplicationStateScopeDto::Profiles));
            let canonical = std::fs::read(directory.join("registry.json")).unwrap();
            commands::diagnostics::create_registry_backup(app.state())
                .await
                .unwrap();
            assert_eq!(
                std::fs::read(directory.join("registry.json.bak")).unwrap(),
                canonical
            );
            let before_reads = emitted.lock().unwrap().len();
            let diagnostics = commands::diagnostics::get_diagnostics(app.state())
                .await
                .unwrap();
            let backup = serde_json::to_value(&diagnostics).unwrap()["registry"]["backup"].clone();
            assert_eq!(backup["exists"], true);
            assert_eq!(backup["state"], "valid");
            assert!(backup["modifiedAt"].is_string());
            commands::profiles::list_profiles(app.state())
                .await
                .unwrap();
            commands::discovery::list_discovery_candidates(app.state())
                .await
                .unwrap();
            assert_eq!(emitted.lock().unwrap().len(), before_reads);
            assert_eq!(
                std::fs::read(directory.join("registry.json")).unwrap(),
                canonical
            );
            assert_eq!(diagnostics.journal.entries.len(), 2);
            assert_eq!(
                diagnostics.journal.entries[0].kind,
                JournalEventKindDto::BackupStarted
            );
            assert_eq!(
                diagnostics.journal.entries[1].kind,
                JournalEventKindDto::BackupSucceeded
            );
            assert!(!serde_json::to_string(&diagnostics.journal)
                .unwrap()
                .contains("SECRET"));

            let session = "00000000-0000-0000-0000-000000000001".to_owned();
            let logs = commands::containers::get_container_logs(
                ContainerLogsRequestDto {
                    container_id: "a".repeat(64),
                    runtime_session_id: session.clone(),
                },
                app.state(),
            )
            .await
            .unwrap_err();
            assert_eq!(logs.code, AppErrorCodeDto::ContainerOperationFailed);
            assert_eq!(emitted.lock().unwrap().len(), before_reads);
            let open = commands::containers::open_container_port(
                OpenContainerPortRequestDto {
                    container_id: "a".repeat(64),
                    runtime_session_id: session,
                    binding_index: 0,
                },
                app.state(),
            )
            .await
            .unwrap_err();
            assert_eq!(
                open.subject.unwrap().kind,
                AppErrorSubjectKindDto::Container
            );
            assert_eq!(emitted.lock().unwrap().len(), before_reads);

            commands::discovery::configure_auto_registration(
                ConfigureAutoRegistrationRequestDto { enabled: true },
                app.state(),
            )
            .await
            .unwrap();
            let after_toggle = emitted.lock().unwrap().len();
            commands::discovery::configure_auto_registration(
                ConfigureAutoRegistrationRequestDto { enabled: true },
                app.state(),
            )
            .await
            .unwrap();
            assert_eq!(emitted.lock().unwrap().len(), after_toggle);
            assert!(!commands::discovery::ignore_candidate(
                IgnoreCandidateRequestDto {
                    candidate_id: "a".repeat(64)
                },
                app.state()
            )
            .await
            .unwrap());
            assert_eq!(emitted.lock().unwrap().len(), after_toggle);
            commands::profiles::remove_profile(
                RemoveProfileRequestDto {
                    profile_id: profile.id,
                    expected_revision: profile.revision,
                },
                app.state(),
            )
            .await
            .unwrap();
            let restored = commands::diagnostics::restore_registry_backup(app.state())
                .await
                .unwrap();
            assert_eq!(restored.len(), 1);
            assert!(restored[0].revision > 1);
            let before_corruption_read = emitted.lock().unwrap().len();
            std::fs::write(directory.join("registry.json.bak"), b"SECRET_BAD_BACKUP").unwrap();
            std::fs::write(directory.join("registry.json"), b"SECRET_BAD_REGISTRY").unwrap();
            let diagnostics = commands::diagnostics::get_diagnostics(app.state())
                .await
                .unwrap();
            assert_eq!(
                diagnostics.registry.backup.state,
                dto::BackupValidationStateDto::Corrupt
            );
            assert_eq!(
                diagnostics.registry.health.state,
                dto::RegistryHealthStateDto::Corrupt
            );
            assert_eq!(
                diagnostics
                    .registry
                    .health
                    .error
                    .unwrap()
                    .subject
                    .unwrap()
                    .kind,
                dto::AppErrorSubjectKindDto::Registry
            );
            assert!(
                commands::discovery::get_auto_registration_configuration(app.state())
                    .await
                    .unwrap()
                    .enabled
            );
            assert_eq!(emitted.lock().unwrap().len(), before_corruption_read);
            assert!(!serde_json::to_string(&diagnostics.journal)
                .unwrap()
                .contains("SECRET"));
        });
        let events = emitted.lock().unwrap();
        for (index, event) in events.iter().enumerate() {
            assert_eq!(event.sequence, index as u64 + 1);
        }
        drop(app);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn event_sequences_are_ordered_and_diagnostics_reads_emit_nothing() {
        let received = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = received.clone();
        let events = Arc::new(ApplicationEvents::new(move |event| {
            sink.lock().unwrap().push(event);
        }));
        events.publish(&[dto::ApplicationStateScopeDto::Discovery]);
        events.publish(&[
            dto::ApplicationStateScopeDto::Profiles,
            dto::ApplicationStateScopeDto::Diagnostics,
        ]);
        let directory =
            std::env::temp_dir().join(format!("colui-event-test-{}", std::process::id()));
        tauri::async_runtime::block_on(async {
            let mut state = AppState::initialize(directory.clone()).await.unwrap();
            state.events = events;
            state.diagnostics().await.unwrap();
            state.diagnostics().await.unwrap();
        });
        let values = received.lock().unwrap();
        assert_eq!(values.len(), 2);
        assert_eq!(values[0].sequence, 1);
        assert_eq!(values[1].sequence, 2);
        assert_eq!(
            values[0].scopes,
            vec![dto::ApplicationStateScopeDto::Discovery]
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

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
