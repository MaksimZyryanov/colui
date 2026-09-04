use colui_app::{
    Clock, InventoryFuture, InventoryReader, InventoryRefresher, RuntimeInventorySource,
};
use colui_domain::{
    AppError, AppErrorCode, ContainerObservation, InventoryFreshness, ProjectRuntimeSnapshot,
    RuntimeInventory, RuntimeSessionState, SessionContext, Timestamp,
};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, watch, Mutex};

type RefreshResult = Result<RuntimeInventory, AppError>;

struct InFlight {
    completed: watch::Receiver<Option<RefreshResult>>,
}

struct State {
    current: RuntimeInventory,
    generation: u64,
    in_flight: Option<InFlight>,
    failures: u32,
    retry_at: Option<Duration>,
}

pub struct InventoryCoordinator {
    api: Arc<dyn RuntimeInventorySource>,
    clock: Arc<dyn Clock>,
    state: Mutex<State>,
    subscribers: broadcast::Sender<RuntimeInventory>,
}

impl InventoryCoordinator {
    pub fn new(api: Arc<dyn RuntimeInventorySource>, clock: Arc<dyn Clock>) -> Self {
        let (subscribers, _) = broadcast::channel(16);
        Self {
            api,
            clock,
            state: Mutex::new(State {
                current: RuntimeInventory::unavailable(),
                generation: 0,
                in_flight: None,
                failures: 0,
                retry_at: None,
            }),
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

    async fn refresh_inner(&self, automatic: bool) -> RefreshResult {
        let (sender, mut completed) = {
            let mut state = self.state.lock().await;
            if automatic
                && state
                    .retry_at
                    .is_some_and(|deadline| self.clock.monotonic() < deadline)
            {
                return Ok(state.current.clone());
            }
            if let Some(in_flight) = &state.in_flight {
                (None, in_flight.completed.clone())
            } else {
                let (sender, receiver) = watch::channel(None);
                state.in_flight = Some(InFlight {
                    completed: receiver.clone(),
                });
                (Some(sender), receiver)
            }
        };

        let Some(sender) = sender else {
            return wait_for_result(&mut completed).await;
        };

        let observed = self.observe().await;
        let mut state = self.state.lock().await;
        let result = self.complete_refresh(&mut state, observed);
        state
            .in_flight
            .take()
            .expect("refresh creator owns installed in-flight state");
        sender.send_replace(Some(result.clone()));
        result
    }

    fn complete_refresh(
        &self,
        state: &mut State,
        observed: Result<(SessionContext, Vec<ContainerObservation>), AppError>,
    ) -> RefreshResult {
        match observed {
            Ok((captured, observations)) => {
                let generation = state.generation.checked_add(1).ok_or_else(|| {
                    AppError::new(
                        AppErrorCode::RuntimeUnavailable,
                        "inventory",
                        None,
                        "inventory generation exhausted",
                    )
                })?;
                let snapshot = normalize(generation, self.clock.now(), captured, observations);
                state.generation = generation;
                state.current = snapshot.clone();
                state.failures = 0;
                state.retry_at = None;
                let _ = self.subscribers.send(snapshot.clone());
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
                state.retry_at = Some(self.clock.monotonic() + Duration::from_secs(delay));
                let mut retained = state.current.clone();
                retained.freshness = if retained.has_snapshot {
                    InventoryFreshness::Stale
                } else {
                    InventoryFreshness::Unavailable
                };
                retained.error = Some(error.clone());
                state.current = retained;
                Err(error)
            }
        }
    }

    async fn observe(&self) -> Result<(SessionContext, Vec<ContainerObservation>), AppError> {
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

async fn wait_for_result(completed: &mut watch::Receiver<Option<RefreshResult>>) -> RefreshResult {
    loop {
        if let Some(result) = completed.borrow().clone() {
            return result;
        }
        completed.changed().await.map_err(|_| {
            AppError::new(
                AppErrorCode::RuntimeUnavailable,
                "inventory",
                None,
                "inventory refresh cancelled",
            )
        })?;
    }
}

fn normalize(
    generation: u64,
    now: Timestamp,
    captured: SessionContext,
    observations: Vec<ContainerObservation>,
) -> RuntimeInventory {
    let mut projects = BTreeMap::<String, ProjectRuntimeSnapshot>::new();
    let mut containers = Vec::with_capacity(observations.len());
    let mut standalone = Vec::new();
    for observation in observations {
        add_observation(observation, &mut containers, &mut standalone, &mut projects);
    }
    RuntimeInventory {
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
    }
}

fn ready_context(state: RuntimeSessionState, message: &str) -> Result<SessionContext, AppError> {
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
