use colui_app::{InventoryFuture, InventoryReader, InventoryRefresher, RuntimeInventorySource};
use colui_domain::{
    AppError, AppErrorCode, ContainerObservation, InventoryFreshness, ProjectRuntimeSnapshot,
    RuntimeInventory, RuntimeSessionState, Timestamp,
};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, Mutex, OnceCell};

struct State {
    current: RuntimeInventory,
    generation: u64,
    in_flight: Option<Arc<OnceCell<Result<RuntimeInventory, AppError>>>>,
    failures: u32,
    retry_at: Option<Instant>,
}

pub struct InventoryCoordinator {
    api: Arc<dyn RuntimeInventorySource>,
    state: Arc<Mutex<State>>,
    subscribers: broadcast::Sender<RuntimeInventory>,
}

impl InventoryCoordinator {
    pub fn new(api: Arc<dyn RuntimeInventorySource>) -> Self {
        let (subscribers, _) = broadcast::channel(16);
        Self {
            api,
            state: Arc::new(Mutex::new(State {
                current: RuntimeInventory::unavailable(),
                generation: 0,
                in_flight: None,
                failures: 0,
                retry_at: None,
            })),
            subscribers,
        }
    }

    pub fn current_inventory(&self) -> InventoryFuture<'_, RuntimeInventory> {
        Box::pin(async { Ok(self.state.lock().await.current.clone()) })
    }

    pub fn refresh(&self) -> InventoryFuture<'_, RuntimeInventory> {
        Box::pin(self.refresh_inner(false))
    }

    pub fn refresh_automatic(&self) -> InventoryFuture<'_, RuntimeInventory> {
        Box::pin(self.refresh_inner(true))
    }

    pub fn subscribe(&self) -> broadcast::Receiver<RuntimeInventory> {
        self.subscribers.subscribe()
    }

    async fn refresh_inner(&self, automatic: bool) -> Result<RuntimeInventory, AppError> {
        let cell = {
            let mut state = self.state.lock().await;
            if automatic && state.retry_at.is_some_and(|at| Instant::now() < at) {
                return Ok(state.current.clone());
            }
            if let Some(cell) = &state.in_flight {
                cell.clone()
            } else {
                let cell = Arc::new(OnceCell::new());
                state.in_flight = Some(cell.clone());
                cell
            }
        };
        let result = cell
            .get_or_init(|| async {
                let observed = self.observe().await;
                let mut state = self.state.lock().await;
                match observed {
                    Ok((captured, observations)) => {
                        let mut projects = BTreeMap::<String, ProjectRuntimeSnapshot>::new();
                        let mut containers = Vec::with_capacity(observations.len());
                        let mut standalone = Vec::new();
                        for observation in observations {
                            add_observation(
                                observation,
                                &mut containers,
                                &mut standalone,
                                &mut projects,
                            );
                        }
                        let now = Timestamp(chrono::Utc::now().to_rfc3339());
                        let generation = state.generation + 1;
                        let snapshot = RuntimeInventory {
                            generation,
                            has_snapshot: true,
                            observed_at: Some(now.clone()),
                            runtime_session_id: Some(captured.session_id),
                            daemon_fingerprint: Some(captured.daemon_fingerprint),
                            freshness: InventoryFreshness::Fresh,
                            last_successful_observed_at: Some(now),
                            containers,
                            project_snapshots: projects.into_values().collect(),
                            standalone_containers: standalone,
                            error: None,
                        };
                        if generation > state.generation {
                            state.generation = generation;
                            state.current = snapshot.clone();
                            state.failures = 0;
                            state.retry_at = None;
                            let _ = self.subscribers.send(snapshot.clone());
                        }
                        Ok(snapshot)
                    }
                    Err(error) => {
                        state.failures = state.failures.saturating_add(1);
                        let delay = match state.failures {
                            1 => 1,
                            2 => 2,
                            3 => 4,
                            4 => 8,
                            _ => 10,
                        };
                        state.retry_at = Some(Instant::now() + Duration::from_secs(delay));
                        let mut retained = state.current.clone();
                        retained.freshness = if retained.has_snapshot {
                            InventoryFreshness::Stale
                        } else {
                            InventoryFreshness::Unavailable
                        };
                        retained.error = Some(error);
                        state.current = retained.clone();
                        Ok(retained)
                    }
                }
            })
            .await
            .clone();
        let mut state = self.state.lock().await;
        if state
            .in_flight
            .as_ref()
            .is_some_and(|active| Arc::ptr_eq(active, &cell))
        {
            state.in_flight = None;
        }
        result
    }

    async fn observe(
        &self,
    ) -> Result<(colui_domain::SessionContext, Vec<ContainerObservation>), AppError> {
        let captured = ready_context(self.api.session_state().await?, "runtime unavailable")?;
        let observations = self.api.list_containers().await?;
        let after = ready_context(self.api.session_state().await?, "runtime session changed")?;
        if captured.session_id != after.session_id
            || captured.daemon_fingerprint != after.daemon_fingerprint
        {
            return Err(AppError::new(
                AppErrorCode::RuntimeUnavailable,
                "inventory",
                None,
                "runtime session changed",
            ));
        }
        Ok((captured, observations))
    }
}

fn ready_context(
    state: RuntimeSessionState,
    message: &str,
) -> Result<colui_domain::SessionContext, AppError> {
    match state {
        RuntimeSessionState::Ready(context) => Ok(context),
        _ => Err(AppError::new(
            AppErrorCode::RuntimeUnavailable,
            "inventory",
            None,
            message,
        )),
    }
}

fn add_observation(
    observation: ContainerObservation,
    all: &mut Vec<colui_domain::ContainerInstance>,
    standalone: &mut Vec<colui_domain::ContainerInstance>,
    projects: &mut BTreeMap<String, ProjectRuntimeSnapshot>,
) {
    let instance = observation.instance;
    all.push(instance.clone());
    if let Some(metadata) = observation.compose {
        let project =
            projects
                .entry(metadata.project.clone())
                .or_insert_with(|| ProjectRuntimeSnapshot {
                    compose_project_name: metadata.project,
                    working_directory: metadata.working_directory,
                    config_files: metadata.config_files,
                    containers: Vec::new(),
                });
        project.containers.push(instance);
    } else {
        standalone.push(instance);
    }
}

impl InventoryReader for InventoryCoordinator {
    fn current_inventory(&self) -> InventoryFuture<'_, RuntimeInventory> {
        self.current_inventory()
    }
}
impl InventoryRefresher for InventoryCoordinator {
    fn refresh(&self) -> InventoryFuture<'_, RuntimeInventory> {
        self.refresh()
    }
}
