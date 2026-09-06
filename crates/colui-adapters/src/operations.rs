use colui_app::{
    ActiveOperation, ContainerOperationGuard, DefinitionBusy, DefinitionLoadGuard,
    LifecycleOperationGuard, OperationFuture, OperationKind,
    OperationLockManager as OperationLockManagerPort, OperationLockReader, OperationPhase,
    OperationProjectionReader, OperationsDiagnostics, RegistryMutationGuard, RegistryRecoveryGuard,
};
use colui_domain::{AppError, AppErrorCode, ProfileId, Timestamp};
use fs2::FileExt;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex, MutexGuard,
};
use tokio::sync::Notify;

pub struct OperationLockManager {
    state: Arc<LockState>,
}

struct LockState {
    profiles: Mutex<HashMap<ProfileId, ProfileState>>,
    barrier: Mutex<BarrierState>,
    recovery_path: PathBuf,
    next_token: AtomicU64,
    projection_generation: AtomicU64,
}

#[derive(Default)]
struct BarrierState {
    shared: usize,
    recovering: bool,
    containers: HashMap<String, Timestamp>,
}

struct ProfileState {
    lifecycle: Option<LifecycleLease>,
    definition: Option<DefinitionLease>,
    notify: Arc<Notify>,
}

#[derive(Clone)]
enum LifecycleLease {
    Pending {
        token: u64,
        kind: OperationKind,
        started_at: Timestamp,
    },
    Active {
        token: u64,
        kind: OperationKind,
        started_at: Timestamp,
    },
}

#[derive(Clone)]
struct DefinitionLease {
    token: u64,
    started_at: Timestamp,
}

struct LifecycleReservation {
    state: Arc<LockState>,
    profile_id: ProfileId,
    token: u64,
    promoted: bool,
    recovery_file: Option<File>,
}

impl OperationLockManager {
    pub fn new(path: PathBuf) -> Self {
        Self {
            state: Arc::new(LockState {
                profiles: Mutex::new(HashMap::new()),
                barrier: Mutex::new(BarrierState::default()),
                recovery_path: path,
                next_token: AtomicU64::new(0),
                projection_generation: AtomicU64::new(0),
            }),
        }
    }

    fn reserve_lifecycle(
        &self,
        profile_id: ProfileId,
        kind: OperationKind,
    ) -> Result<LifecycleReservation, AppError> {
        let recovery_file = acquire_shared(&self.state)?;
        let token = self.state.next_token.fetch_add(1, Ordering::Relaxed);
        let mut profiles = lock_profiles(&self.state);
        let profile = profiles
            .entry(profile_id.clone())
            .or_insert_with(ProfileState::new);
        if profile.lifecycle.is_some() {
            release_shared(&self.state);
            return Err(AppError::new(
                AppErrorCode::OperationConflict,
                "acquire_lifecycle",
                Some(profile_id),
                "operation already in progress",
            ));
        }
        profile.lifecycle = Some(LifecycleLease::Pending {
            token,
            kind,
            started_at: now(),
        });
        self.state
            .projection_generation
            .fetch_add(1, Ordering::Relaxed);
        Ok(LifecycleReservation {
            state: Arc::clone(&self.state),
            profile_id,
            token,
            promoted: false,
            recovery_file,
        })
    }

    pub fn acquire_container(
        &self,
        container_id: &str,
    ) -> Result<ContainerOperationGuard, AppError> {
        let recovery_file = acquire_shared(&self.state)?;
        let mut barrier = lock_barrier(&self.state);
        if barrier.containers.contains_key(container_id) {
            barrier.shared -= 1;
            return Err(conflict(
                "acquire_container",
                "container operation already in progress",
            ));
        }
        barrier.containers.insert(container_id.to_owned(), now());
        self.state
            .projection_generation
            .fetch_add(1, Ordering::Relaxed);
        drop(barrier);
        let state = Arc::clone(&self.state);
        let container_id = container_id.to_owned();
        Ok(ContainerOperationGuard::new(move || {
            let _file = recovery_file;
            let mut barrier = lock_barrier(&state);
            barrier.containers.remove(&container_id);
            barrier.shared -= 1;
            state.projection_generation.fetch_add(1, Ordering::Relaxed);
        }))
    }

    pub fn acquire_mutation(&self) -> Result<RegistryMutationGuard, AppError> {
        let recovery_file = acquire_shared(&self.state)?;
        let state = Arc::clone(&self.state);
        Ok(RegistryMutationGuard::new(move || {
            let _file = recovery_file;
            release_shared(&state);
        }))
    }

    pub fn acquire_recovery(&self) -> Result<RegistryRecoveryGuard, AppError> {
        {
            let mut barrier = lock_barrier(&self.state);
            if barrier.recovering || barrier.shared != 0 {
                return Err(conflict(
                    "acquire_recovery",
                    "operation already in progress",
                ));
            }
            barrier.recovering = true;
        }
        let recovery_file = match acquire_file(&self.state.recovery_path, true) {
            Ok(file) => file,
            Err(error) => {
                lock_barrier(&self.state).recovering = false;
                return Err(error);
            }
        };
        let state = Arc::clone(&self.state);
        Ok(RegistryRecoveryGuard::new(move || {
            let _file = recovery_file;
            lock_barrier(&state).recovering = false;
        }))
    }
}

impl OperationLockReader for OperationLockManager {
    fn is_busy(&self, profile_id: &ProfileId) -> bool {
        lock_profiles(&self.state)
            .get(profile_id)
            .map(|profile| profile.lifecycle.is_some() || profile.definition.is_some())
            .unwrap_or(false)
    }
}

impl OperationLockManagerPort for OperationLockManager {
    fn acquire_lifecycle(
        &self,
        profile_id: ProfileId,
        kind: OperationKind,
    ) -> OperationFuture<'_, LifecycleOperationGuard> {
        let reservation = self.reserve_lifecycle(profile_id, kind);
        Box::pin(async move {
            let reservation = reservation?;
            wait_for_definition(reservation).await
        })
    }

    fn acquire_definition(
        &self,
        profile_id: ProfileId,
    ) -> Result<DefinitionLoadGuard, DefinitionBusy> {
        let recovery_file =
            acquire_shared(&self.state).map_err(|_| DefinitionBusy::LifecyclePending)?;
        let token = self.state.next_token.fetch_add(1, Ordering::Relaxed);
        let mut profiles = lock_profiles(&self.state);
        let profile = profiles
            .entry(profile_id.clone())
            .or_insert_with(ProfileState::new);
        match profile.lifecycle.as_ref() {
            Some(LifecycleLease::Pending { .. }) => {
                release_shared(&self.state);
                return Err(DefinitionBusy::LifecyclePending);
            }
            Some(LifecycleLease::Active {
                kind, started_at, ..
            }) => {
                let _ = (kind, started_at);
                release_shared(&self.state);
                return Err(DefinitionBusy::LifecyclePending);
            }
            None => {}
        }
        if let Some(lease) = profile.definition.as_ref() {
            let _ = &lease.started_at;
            release_shared(&self.state);
            return Err(DefinitionBusy::DefinitionActive);
        }
        profile.definition = Some(DefinitionLease {
            token,
            started_at: now(),
        });
        self.state
            .projection_generation
            .fetch_add(1, Ordering::Relaxed);
        let state = Arc::clone(&self.state);
        Ok(DefinitionLoadGuard::new(move || {
            let _file = recovery_file;
            release_definition(&state, &profile_id, token);
            release_shared(&state);
        }))
    }

    fn acquire_container(&self, container_id: &str) -> Result<ContainerOperationGuard, AppError> {
        OperationLockManager::acquire_container(self, container_id)
    }

    fn acquire_mutation(&self) -> Result<RegistryMutationGuard, AppError> {
        OperationLockManager::acquire_mutation(self)
    }

    fn acquire_recovery(&self) -> Result<RegistryRecoveryGuard, AppError> {
        OperationLockManager::acquire_recovery(self)
    }
}

impl OperationProjectionReader for OperationLockManager {
    fn operation_projection(&self) -> colui_app::DiagnosticsFuture<'_, OperationsDiagnostics> {
        Box::pin(async move {
            let mut active = Vec::new();
            let profiles = lock_profiles(&self.state);
            let barrier = lock_barrier(&self.state);
            for (profile_id, profile) in profiles.iter() {
                if let Some(lease) = &profile.lifecycle {
                    let (kind, started_at, phase) = match lease {
                        LifecycleLease::Pending {
                            kind, started_at, ..
                        } => (*kind, started_at.clone(), OperationPhase::Pending),
                        LifecycleLease::Active {
                            kind, started_at, ..
                        } => (*kind, started_at.clone(), OperationPhase::Active),
                    };
                    active.push(ActiveOperation {
                        kind: operation_name(kind).into(),
                        subject_id: profile_id.to_string(),
                        started_at,
                        phase,
                    });
                }
                if let Some(lease) = &profile.definition {
                    active.push(ActiveOperation {
                        kind: "definition".into(),
                        subject_id: profile_id.to_string(),
                        started_at: lease.started_at.clone(),
                        phase: OperationPhase::Active,
                    });
                }
            }
            for (container_id, started_at) in &barrier.containers {
                active.push(ActiveOperation {
                    kind: "container".into(),
                    subject_id: container_id.clone(),
                    started_at: started_at.clone(),
                    phase: OperationPhase::Active,
                });
            }
            active.sort_by(|left, right| {
                left.subject_id
                    .cmp(&right.subject_id)
                    .then(left.kind.cmp(&right.kind))
            });
            let projection = OperationsDiagnostics {
                generation: self.state.projection_generation.load(Ordering::Relaxed),
                active,
            };
            drop(profiles);
            drop(barrier);
            Ok(projection)
        })
    }
}

fn operation_name(kind: OperationKind) -> &'static str {
    match kind {
        OperationKind::Apply => "apply",
        OperationKind::Stop => "stop",
        OperationKind::TearDown => "tear_down",
        OperationKind::Restart => "restart",
    }
}

impl Drop for LifecycleReservation {
    fn drop(&mut self) {
        if !self.promoted {
            release_pending(&self.state, &self.profile_id, self.token);
            release_shared(&self.state);
        }
    }
}

async fn wait_for_definition(
    mut reservation: LifecycleReservation,
) -> Result<LifecycleOperationGuard, AppError> {
    loop {
        let notify = {
            let mut profiles = lock_profiles(&reservation.state);
            let profile = profiles
                .get_mut(&reservation.profile_id)
                .expect("lifecycle reservation must have a profile state");
            match profile.lifecycle.as_ref() {
                Some(LifecycleLease::Pending { token, kind, .. })
                    if *token == reservation.token =>
                {
                    if profile.definition.is_some() {
                        Some(Arc::clone(&profile.notify))
                    } else {
                        profile.lifecycle = Some(LifecycleLease::Active {
                            token: *token,
                            kind: *kind,
                            started_at: now(),
                        });
                        reservation
                            .state
                            .projection_generation
                            .fetch_add(1, Ordering::Relaxed);
                        reservation.promoted = true;
                        None
                    }
                }
                _ => panic!("lifecycle reservation lost its profile state"),
            }
        };

        if let Some(notify) = notify {
            notify.notified().await;
            continue;
        }

        let state = Arc::clone(&reservation.state);
        let profile_id = reservation.profile_id.clone();
        let token = reservation.token;
        let recovery_file = reservation.recovery_file.take();
        return Ok(LifecycleOperationGuard::new(move || {
            let _file = recovery_file;
            release_lifecycle(&state, &profile_id, token);
            release_shared(&state);
        }));
    }
}

fn release_pending(state: &LockState, profile_id: &ProfileId, token: u64) {
    let mut profiles = lock_profiles(state);
    let should_remove = profiles
        .get(profile_id)
        .map(|profile| {
            matches!(
                profile.lifecycle,
                Some(LifecycleLease::Pending {
                    token: current,
                    ..
                }) if current == token
            ) && profile.definition.is_none()
        })
        .unwrap_or(false);
    if should_remove {
        profiles.remove(profile_id);
    } else if let Some(profile) = profiles.get_mut(profile_id) {
        if matches!(
            profile.lifecycle,
            Some(LifecycleLease::Pending {
                token: current,
                ..
            }) if current == token
        ) {
            profile.lifecycle = None;
            profile.notify.notify_one();
            remove_if_idle(&mut profiles, profile_id);
        }
    }
    state.projection_generation.fetch_add(1, Ordering::Relaxed);
}

fn release_lifecycle(state: &LockState, profile_id: &ProfileId, token: u64) {
    let mut profiles = lock_profiles(state);
    if let Some(profile) = profiles.get_mut(profile_id) {
        if matches!(
            profile.lifecycle,
            Some(LifecycleLease::Active {
                token: current,
                ..
            }) if current == token
        ) {
            profile.lifecycle = None;
            profile.notify.notify_one();
            remove_if_idle(&mut profiles, profile_id);
        }
    }
    state.projection_generation.fetch_add(1, Ordering::Relaxed);
}

fn release_definition(state: &LockState, profile_id: &ProfileId, token: u64) {
    let mut profiles = lock_profiles(state);
    if let Some(profile) = profiles.get_mut(profile_id) {
        if profile
            .definition
            .as_ref()
            .map(|lease| lease.token == token)
            .unwrap_or(false)
        {
            profile.definition = None;
            profile.notify.notify_one();
            remove_if_idle(&mut profiles, profile_id);
        }
    }
    state.projection_generation.fetch_add(1, Ordering::Relaxed);
}

fn remove_if_idle(profiles: &mut HashMap<ProfileId, ProfileState>, profile_id: &ProfileId) {
    if profiles
        .get(profile_id)
        .map(|profile| profile.lifecycle.is_none() && profile.definition.is_none())
        .unwrap_or(false)
    {
        profiles.remove(profile_id);
    }
}

fn lock_profiles<'a>(state: &'a LockState) -> MutexGuard<'a, HashMap<ProfileId, ProfileState>> {
    match state.profiles.lock() {
        Ok(profiles) => profiles,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn now() -> Timestamp {
    Timestamp(chrono::Utc::now().to_rfc3339())
}

fn lock_barrier(state: &LockState) -> MutexGuard<'_, BarrierState> {
    state
        .barrier
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn acquire_shared(state: &Arc<LockState>) -> Result<Option<File>, AppError> {
    {
        let mut barrier = lock_barrier(state);
        if barrier.recovering {
            return Err(conflict(
                "acquire_operation",
                "registry recovery in progress",
            ));
        }
        barrier.shared += 1;
    }
    match acquire_file(&state.recovery_path, false) {
        Ok(file) => Ok(file),
        Err(error) => {
            release_shared(state);
            Err(error)
        }
    }
}

fn release_shared(state: &LockState) {
    let mut barrier = lock_barrier(state);
    barrier.shared = barrier.shared.saturating_sub(1);
}

fn acquire_file(path: &PathBuf, exclusive: bool) -> Result<Option<File>, AppError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(recovery_io)?;
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(recovery_io)?;
    let result = if exclusive {
        FileExt::try_lock_exclusive(&file)
    } else {
        FileExt::try_lock_shared(&file)
    };
    result.map_err(|error| {
        if error.kind() == std::io::ErrorKind::WouldBlock {
            AppError::new(
                AppErrorCode::RecoveryConflict,
                "lock_registry_recovery",
                None,
                "registry recovery lock is busy",
            )
        } else {
            recovery_io(error)
        }
    })?;
    Ok(Some(file))
}

fn conflict(operation: &str, message: &str) -> AppError {
    AppError::new(AppErrorCode::OperationConflict, operation, None, message)
}

fn recovery_io(error: std::io::Error) -> AppError {
    AppError::new(
        AppErrorCode::RegistryWriteFailed,
        "lock_registry_recovery",
        None,
        "registry recovery lock failed",
    )
    .with_details(error.to_string())
}

impl ProfileState {
    fn new() -> Self {
        Self {
            lifecycle: None,
            definition: None,
            notify: Arc::new(Notify::new()),
        }
    }
}
