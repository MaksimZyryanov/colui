use colui_app::{
    AutoRegistrationSchedule, CandidateLease, Clock, DiscoveryFuture, DiscoveryReader,
    DiscoverySession, InventoryFuture, InventoryReader, InventoryRefresher, ProfileReader,
    RuntimeInventorySource, ScheduleAutoRegistration,
};
use colui_domain::{
    AppError, AppErrorCode, ComposeObservationGroup, ContainerObservation, InventoryFreshness,
    ProjectRuntimeSnapshot, RuntimeInventory, SessionContext, Timestamp,
};
use std::collections::{BTreeMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, watch, Mutex, RwLock};

type RefreshResult = Result<RuntimeInventory, AppError>;

struct InFlight {
    completed: watch::Sender<Option<RefreshResult>>,
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
    state: Arc<Mutex<State>>,
    publication_gate: Arc<RwLock<()>>,
    subscribers: broadcast::Sender<RuntimeInventory>,
    joins: broadcast::Sender<()>,
}

impl InventoryCoordinator {
    pub fn new(api: Arc<dyn RuntimeInventorySource>, clock: Arc<dyn Clock>) -> Self {
        let (subscribers, _) = broadcast::channel(16);
        let (joins, _) = broadcast::channel(16);
        Self {
            api,
            clock,
            state: Arc::new(Mutex::new(State {
                current: RuntimeInventory::unavailable(),
                generation: 0,
                in_flight: None,
                failures: 0,
                retry_at: None,
            })),
            publication_gate: Arc::new(RwLock::new(())),
            subscribers,
            joins,
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

    pub fn subscribe_refresh_joins(&self) -> broadcast::Receiver<()> {
        self.joins.subscribe()
    }

    pub fn start_auto_registration_scheduler(
        &self,
        profiles: Arc<dyn ProfileReader>,
        discovery: Arc<DiscoverySession>,
    ) -> mpsc::Receiver<AutoRegistrationSchedule> {
        let mut publications = self.subscribe();
        let (scheduled, receiver) = mpsc::channel(16);
        tokio::spawn(async move {
            loop {
                let inventory = match publications.recv().await {
                    Ok(inventory) => inventory,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                };
                let Ok(Some(schedule)) =
                    ScheduleAutoRegistration::new(profiles.as_ref(), &discovery)
                        .execute(inventory)
                        .await
                else {
                    continue;
                };
                if scheduled.send(schedule).await.is_err() {
                    break;
                }
            }
        });
        receiver
    }

    async fn refresh_inner(&self, automatic: bool) -> RefreshResult {
        let (creator, mut completed) = {
            let mut state = self.state.lock().await;
            if automatic
                && state
                    .retry_at
                    .is_some_and(|deadline| self.clock.monotonic() < deadline)
            {
                return Err(state
                    .current
                    .error
                    .clone()
                    .expect("automatic backoff follows a retained refresh error"));
            }
            if let Some(in_flight) = &state.in_flight {
                let _ = self.joins.send(());
                (false, in_flight.completed.subscribe())
            } else {
                let (sender, receiver) = watch::channel(None);
                state.in_flight = Some(InFlight { completed: sender });
                (true, receiver)
            }
        };

        if creator {
            let api = self.api.clone();
            let clock = self.clock.clone();
            let state = self.state.clone();
            let publication_gate = self.publication_gate.clone();
            let subscribers = self.subscribers.clone();
            tokio::spawn(async move {
                let observed = observe(&api).await;
                let _publication = publication_gate.write().await;
                let mut state = state.lock().await;
                let result = complete_refresh(&clock, &subscribers, &mut state, observed);
                let in_flight = state
                    .in_flight
                    .take()
                    .expect("refresh worker owns installed in-flight state");
                in_flight.completed.send_replace(Some(result));
            });
        }
        wait_for_result(&mut completed).await
    }
}

fn complete_refresh(
    clock: &Arc<dyn Clock>,
    subscribers: &broadcast::Sender<RuntimeInventory>,
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
            let snapshot = normalize(generation, clock.now(), captured, observations);
            state.generation = generation;
            state.current = snapshot.clone();
            state.failures = 0;
            state.retry_at = None;
            let _ = subscribers.send(snapshot.clone());
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
            state.retry_at = Some(clock.monotonic() + Duration::from_secs(delay));
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

async fn observe(
    api: &Arc<dyn RuntimeInventorySource>,
) -> Result<(SessionContext, Vec<ContainerObservation>), AppError> {
    let captured = api.api_read_context().await?;
    let observations = api.list_containers().await?;
    let after = api.api_read_context().await?;
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
    let mut groups =
        BTreeMap::<(String, Option<String>, Vec<String>), Vec<colui_domain::ContainerId>>::new();
    let mut containers = Vec::with_capacity(observations.len());
    let mut standalone = Vec::new();
    for observation in observations {
        add_observation(
            observation,
            &mut containers,
            &mut standalone,
            &mut projects,
            &mut groups,
        );
    }
    let compose_observation_groups = groups
        .into_iter()
        .map(
            |((compose_project_name, working_directory, config_files), mut container_ids)| {
                container_ids.sort_by(|left, right| left.0.cmp(&right.0));
                ComposeObservationGroup {
                    compose_project_name,
                    working_directory,
                    config_files,
                    container_ids,
                }
            },
        )
        .collect();
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
        compose_observation_groups,
        standalone_containers: standalone,
        error: None,
    }
}

fn add_observation(
    observation: ContainerObservation,
    all: &mut Vec<colui_domain::ContainerInstance>,
    standalone: &mut Vec<colui_domain::ContainerInstance>,
    projects: &mut BTreeMap<String, ProjectRuntimeSnapshot>,
    groups: &mut BTreeMap<(String, Option<String>, Vec<String>), Vec<colui_domain::ContainerId>>,
) {
    let instance = observation.instance;
    all.push(instance.clone());
    if let Some(metadata) = observation.compose {
        let group_tuple = normalize_compose_tuple(
            &metadata.project,
            metadata.working_directory.as_deref(),
            &metadata.config_files,
        );
        groups
            .entry(group_tuple)
            .or_default()
            .push(instance.id.clone());
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

fn normalize_compose_tuple(
    project: &str,
    working_directory: Option<&str>,
    config_files: &[String],
) -> (String, Option<String>, Vec<String>) {
    let working_directory = working_directory
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .and_then(|path| normalize_absolute_path(Path::new(path)));
    let mut seen = HashSet::new();
    let config_files = config_files
        .iter()
        .map(|path| path.trim())
        .filter(|path| !path.is_empty())
        .filter_map(|path| {
            let path = Path::new(path);
            if path.is_absolute() {
                normalize_absolute_path(path)
            } else {
                working_directory.as_ref().and_then(|directory| {
                    normalize_absolute_path(&PathBuf::from(directory).join(path))
                })
            }
        })
        .filter(|path| seen.insert(path.clone()))
        .collect();
    (project.trim().to_owned(), working_directory, config_files)
}

fn normalize_absolute_path(path: &Path) -> Option<String> {
    if !path.is_absolute() {
        return None;
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::RootDir | Component::Prefix(_) => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(value) => normalized.push(value),
        }
    }
    normalized.to_str().map(str::to_owned)
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

impl DiscoveryReader for InventoryCoordinator {
    fn lease_candidate(
        &self,
        runtime_session_id: colui_domain::RuntimeSessionId,
        inventory_generation: u64,
    ) -> DiscoveryFuture<'_, CandidateLease> {
        let gate = self.publication_gate.clone();
        Box::pin(async move {
            let guard = gate.read_owned().await;
            let current = self.state.lock().await.current.clone();
            if current.runtime_session_id != Some(runtime_session_id)
                || current.generation != inventory_generation
            {
                return Err(AppError::new(
                    AppErrorCode::RuntimeUnavailable,
                    "lease_candidate",
                    None,
                    "discovery evidence changed",
                ));
            }
            Ok(CandidateLease::hold(guard))
        })
    }
}
