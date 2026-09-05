use colui_app::*;
use colui_domain::{
    AppError, AppErrorCode, InventoryFreshness, RuntimeInventory, RuntimeSessionState,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Sections {
    reads: AtomicUsize,
}

impl DiagnosticsSectionsReader for Sections {
    fn read_sections(&self) -> DiagnosticsFuture<'_, DiagnosticsSections> {
        Box::pin(async move {
            self.reads.fetch_add(1, Ordering::SeqCst);
            Ok(DiagnosticsSections {
                runtime: RuntimeDiagnostics {
                    state: RuntimeSessionState::Disconnected,
                    resolved_endpoint: None,
                    api_fingerprint: None,
                    cli_fingerprint: None,
                    session_id: None,
                    connected_at: None,
                },
                registry: RegistryDiagnostics {
                    registry_path: PathBuf::from("/state/registry.json"),
                    backup_path: PathBuf::from("/state/registry.json.bak"),
                    revision: Some(7),
                    health: RegistryHealth {
                        state: RegistryHealthState::Healthy,
                        identity: None,
                        error: None,
                        last_operation_at: None,
                        last_failure_at: None,
                    },
                    lock_timeout: std::time::Duration::from_secs(2),
                    last_recovery_result: None,
                },
                import: ImportDiagnostics {
                    source_path: PathBuf::from("/state/projects.json"),
                    imported_count: 3,
                    source_preserved: true,
                    error: None,
                },
                operations: OperationsDiagnostics {
                    generation: 11,
                    active: vec![],
                },
                inventory: RuntimeInventory::unavailable(),
                definitions: DefinitionsDiagnostics {
                    generation: 13,
                    profiles: vec![],
                },
                journal: SessionJournal::default(),
            })
        })
    }
}

#[tokio::test]
async fn diagnostics_is_one_read_only_projection_with_independent_versions() {
    let sections = Sections {
        reads: AtomicUsize::new(0),
    };
    let snapshot = DiagnosticsQuery::new(&sections).read().await.unwrap();

    assert_eq!(snapshot.registry.revision, Some(7));
    assert_eq!(snapshot.operations.generation, 11);
    assert_eq!(snapshot.definitions.generation, 13);
    assert_eq!(snapshot.inventory.generation, 0);
    assert_eq!(snapshot.import.imported_count, 3);
    assert_eq!(sections.reads.load(Ordering::SeqCst), 1);
}

struct ReadOnlyOwners {
    reads: AtomicUsize,
    docker_calls: AtomicUsize,
    registry_writes: AtomicUsize,
    definition_refreshes: AtomicUsize,
    operation_leases: AtomicUsize,
}

impl RuntimeDiagnosticsReader for ReadOnlyOwners {
    fn runtime_diagnostics(&self) -> DiagnosticsFuture<'_, RuntimeDiagnostics> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {
            Ok(RuntimeDiagnostics {
                state: RuntimeSessionState::Disconnected,
                resolved_endpoint: None,
                api_fingerprint: None,
                cli_fingerprint: None,
                session_id: None,
                connected_at: None,
            })
        })
    }
}

impl RegistryDiagnosticsReader for ReadOnlyOwners {
    fn registry_diagnostics(&self) -> DiagnosticsFuture<'_, RegistryDiagnostics> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(registry_diagnostics()) })
    }
}

impl ImportDiagnosticsReader for ReadOnlyOwners {
    fn import_diagnostics(&self) -> DiagnosticsFuture<'_, ImportDiagnostics> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {
            Ok(ImportDiagnostics {
                source_path: PathBuf::from("/state/projects.json"),
                imported_count: 3,
                source_preserved: true,
                error: None,
            })
        })
    }
}

impl OperationProjectionReader for ReadOnlyOwners {
    fn operation_projection(&self) -> DiagnosticsFuture<'_, OperationsDiagnostics> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {
            Ok(OperationsDiagnostics {
                generation: 4,
                active: vec![],
            })
        })
    }
}

impl InventoryReader for ReadOnlyOwners {
    fn current_inventory(&self) -> InventoryFuture<'_, RuntimeInventory> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(RuntimeInventory::unavailable()) })
    }
}

impl DefinitionDiagnosticsReader for ReadOnlyOwners {
    fn definition_diagnostics(&self) -> DiagnosticsFuture<'_, DefinitionsDiagnostics> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {
            Ok(DefinitionsDiagnostics {
                generation: 5,
                profiles: vec![],
            })
        })
    }
}

impl JournalReader for ReadOnlyOwners {
    fn journal(&self) -> DiagnosticsFuture<'_, SessionJournal> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(SessionJournal::default()) })
    }
}

impl DockerApi for ReadOnlyOwners {
    fn list_containers(&self) -> RuntimeFuture<'_, Vec<colui_domain::ContainerObservation>> {
        self.docker_calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { unreachable!() })
    }

    fn inspect_container(
        &self,
        _: &colui_domain::ContainerId,
    ) -> RuntimeFuture<'_, colui_domain::ContainerDetails> {
        self.docker_calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { unreachable!() })
    }
}

impl ProfileReader for ReadOnlyOwners {
    fn load(&self) -> StoreFuture<'_, RegistrySnapshot> {
        Box::pin(async { unreachable!() })
    }
}

impl ProfileStore for ReadOnlyOwners {
    fn mutate(&self, _: ProfileMutation) -> StoreFuture<'_, RegistrySnapshot> {
        self.registry_writes.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { unreachable!() })
    }
}

impl DefinitionReader for ReadOnlyOwners {
    fn definition(
        &self,
        _: colui_domain::ProjectProfile,
    ) -> DefinitionFuture<'_, DefinitionProjection> {
        Box::pin(async { unreachable!() })
    }
}

impl DefinitionRefresher for ReadOnlyOwners {
    fn refresh_definition(
        &self,
        _: colui_domain::ProjectProfile,
    ) -> DefinitionFuture<'_, DefinitionProjection> {
        self.definition_refreshes.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { unreachable!() })
    }

    fn invalidate(&self, _: colui_domain::ProfileId) {
        self.definition_refreshes.fetch_add(1, Ordering::SeqCst);
    }
}

impl OperationLockReader for ReadOnlyOwners {
    fn is_busy(&self, _: &colui_domain::ProfileId) -> bool {
        false
    }
}

impl OperationLockManager for ReadOnlyOwners {
    fn acquire_lifecycle(
        &self,
        _: colui_domain::ProfileId,
        _: OperationKind,
    ) -> OperationFuture<'_, LifecycleOperationGuard> {
        self.operation_leases.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { unreachable!() })
    }

    fn acquire_definition(
        &self,
        _: colui_domain::ProfileId,
    ) -> Result<DefinitionLoadGuard, DefinitionBusy> {
        self.operation_leases.fetch_add(1, Ordering::SeqCst);
        Err(DefinitionBusy::DefinitionActive)
    }

    fn acquire_container(&self, _: &str) -> Result<ContainerOperationGuard, AppError> {
        self.operation_leases.fetch_add(1, Ordering::SeqCst);
        unreachable!()
    }

    fn acquire_mutation(&self) -> Result<RegistryMutationGuard, AppError> {
        self.operation_leases.fetch_add(1, Ordering::SeqCst);
        unreachable!()
    }

    fn acquire_recovery(&self) -> Result<RegistryRecoveryGuard, AppError> {
        self.operation_leases.fetch_add(1, Ordering::SeqCst);
        unreachable!()
    }
}

fn registry_diagnostics() -> RegistryDiagnostics {
    RegistryDiagnostics {
        registry_path: PathBuf::from("/state/registry.json"),
        backup_path: PathBuf::from("/state/registry.json.bak"),
        revision: Some(7),
        health: RegistryHealth {
            state: RegistryHealthState::Healthy,
            identity: None,
            error: None,
            last_operation_at: None,
            last_failure_at: None,
        },
        lock_timeout: std::time::Duration::from_secs(5),
        last_recovery_result: None,
    }
}

#[tokio::test]
async fn concrete_diagnostics_assembler_only_reads_existing_owners() {
    let owners = ReadOnlyOwners {
        reads: AtomicUsize::new(0),
        docker_calls: AtomicUsize::new(0),
        registry_writes: AtomicUsize::new(0),
        definition_refreshes: AtomicUsize::new(0),
        operation_leases: AtomicUsize::new(0),
    };
    let diagnostics = DiagnosticsAssembler::new(
        &owners, &owners, &owners, &owners, &owners, &owners, &owners,
    );

    let snapshot = diagnostics.read().await.unwrap();

    assert_eq!(snapshot.import.imported_count, 3);
    assert_eq!(owners.reads.load(Ordering::SeqCst), 7);
    assert_eq!(owners.docker_calls.load(Ordering::SeqCst), 0);
    assert_eq!(owners.registry_writes.load(Ordering::SeqCst), 0);
    assert_eq!(owners.definition_refreshes.load(Ordering::SeqCst), 0);
    assert_eq!(owners.operation_leases.load(Ordering::SeqCst), 0);
}

struct Connector {
    state: RuntimeSessionState,
}

impl RuntimeConnector for Connector {
    fn connect_runtime(
        &self,
        _: Option<colui_domain::DockerEndpoint>,
    ) -> RuntimeFuture<'_, RuntimeSessionState> {
        Box::pin(async { unreachable!() })
    }
    fn disconnect_runtime(&self) -> RuntimeFuture<'_, ()> {
        Box::pin(async { unreachable!() })
    }
    fn reconnect_runtime(
        &self,
        _: Option<colui_domain::DockerEndpoint>,
    ) -> RuntimeFuture<'_, RuntimeSessionState> {
        let state = self.state.clone();
        Box::pin(async move { Ok(state) })
    }
}

struct Inventory {
    refreshes: AtomicUsize,
    fail: bool,
}

impl InventoryReader for Inventory {
    fn current_inventory(&self) -> InventoryFuture<'_, RuntimeInventory> {
        Box::pin(async {
            let mut value = RuntimeInventory::unavailable();
            value.generation = 9;
            value.freshness = InventoryFreshness::Stale;
            Ok(value)
        })
    }
}

impl InventoryRefresher for Inventory {
    fn refresh(&self) -> InventoryFuture<'_, RuntimeInventory> {
        Box::pin(async move {
            self.refreshes.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                Err(AppError::new(
                    AppErrorCode::RuntimeUnavailable,
                    "inventory",
                    None,
                    "failed",
                ))
            } else {
                let mut value = RuntimeInventory::unavailable();
                value.generation = 10;
                Ok(value)
            }
        })
    }
}

#[tokio::test]
async fn reconnect_refreshes_exactly_once_and_returns_retained_inventory_on_failure() {
    let runtime = Connector {
        state: RuntimeSessionState::ContextMismatch(colui_domain::MismatchDetails::new(
            "unix:///docker.sock".try_into().unwrap(),
            colui_domain::DaemonFingerprint::new("api", "1", "linux", "amd64"),
            colui_domain::DaemonFingerprint::new("cli", "1", "linux", "amd64"),
        )),
    };
    let inventory = Inventory {
        refreshes: AtomicUsize::new(0),
        fail: true,
    };

    let result = ReconnectRuntime::new(&runtime, &inventory)
        .execute(None)
        .await
        .unwrap();

    assert!(matches!(
        result.runtime_state,
        RuntimeSessionState::ContextMismatch(_)
    ));
    assert_eq!(result.inventory.unwrap().generation, 9);
    assert_eq!(inventory.refreshes.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn failed_reconnect_does_not_refresh_or_invent_inventory() {
    let runtime = Connector {
        state: RuntimeSessionState::Failed(AppError::new(
            AppErrorCode::RuntimeConnectionFailed,
            "connect_runtime",
            None,
            "failed",
        )),
    };
    let inventory = Inventory {
        refreshes: AtomicUsize::new(0),
        fail: false,
    };

    let error = ReconnectRuntime::new(&runtime, &inventory)
        .execute(None)
        .await
        .unwrap_err();

    assert_eq!(error.code, AppErrorCode::RuntimeConnectionFailed);
    assert_eq!(inventory.refreshes.load(Ordering::SeqCst), 0);
}
