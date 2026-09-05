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
                    imported_profiles: 3,
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
