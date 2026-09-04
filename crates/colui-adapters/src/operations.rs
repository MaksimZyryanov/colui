use colui_app::{
    DefinitionBusy, DefinitionLoadGuard, LifecycleOperationGuard, OperationFuture, OperationKind,
    OperationLockManager as OperationLockManagerPort, OperationLockReader,
};
use colui_domain::{AppError, AppErrorCode, ProfileId};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use tokio::sync::{Mutex, MutexGuard, Notify};

pub struct OperationLockManager {
    state: Arc<LockState>,
}

struct LockState {
    profiles: Mutex<HashMap<ProfileId, ProfileState>>,
    next_token: AtomicU64,
}

struct ProfileState {
    lifecycle: Option<LifecycleLease>,
    definition: Option<u64>,
    notify: Arc<Notify>,
}

#[derive(Clone, Copy)]
enum LifecycleLease {
    Pending { token: u64, kind: OperationKind },
    Active { token: u64, kind: OperationKind },
}

struct LifecycleReservation {
    state: Arc<LockState>,
    profile_id: ProfileId,
    token: u64,
    promoted: bool,
}

impl OperationLockManager {
    pub fn new() -> Self {
        Self {
            state: Arc::new(LockState {
                profiles: Mutex::new(HashMap::new()),
                next_token: AtomicU64::new(0),
            }),
        }
    }

    fn reserve_lifecycle(
        &self,
        profile_id: ProfileId,
        kind: OperationKind,
    ) -> Result<LifecycleReservation, AppError> {
        let token = self.state.next_token.fetch_add(1, Ordering::Relaxed);
        let mut profiles = lock_profiles(&self.state);
        let profile = profiles
            .entry(profile_id.clone())
            .or_insert_with(ProfileState::new);
        if profile.lifecycle.is_some() {
            return Err(AppError::new(
                AppErrorCode::OperationConflict,
                "acquire_lifecycle",
                Some(profile_id),
                "operation already in progress",
            ));
        }
        profile.lifecycle = Some(LifecycleLease::Pending { token, kind });
        Ok(LifecycleReservation {
            state: Arc::clone(&self.state),
            profile_id,
            token,
            promoted: false,
        })
    }
}

impl Default for OperationLockManager {
    fn default() -> Self {
        Self::new()
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
        let token = self.state.next_token.fetch_add(1, Ordering::Relaxed);
        let mut profiles = lock_profiles(&self.state);
        let profile = profiles
            .entry(profile_id.clone())
            .or_insert_with(ProfileState::new);
        match profile.lifecycle {
            Some(LifecycleLease::Pending { .. }) => return Err(DefinitionBusy::LifecyclePending),
            Some(LifecycleLease::Active { kind, .. }) => {
                let _ = kind;
                return Err(DefinitionBusy::LifecyclePending);
            }
            None => {}
        }
        if profile.definition.is_some() {
            return Err(DefinitionBusy::DefinitionActive);
        }
        profile.definition = Some(token);
        let state = Arc::clone(&self.state);
        Ok(DefinitionLoadGuard::new(move || {
            release_definition(&state, &profile_id, token);
        }))
    }
}

impl Drop for LifecycleReservation {
    fn drop(&mut self) {
        if !self.promoted {
            release_pending(&self.state, &self.profile_id, self.token);
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
            match profile.lifecycle {
                Some(LifecycleLease::Pending { token, kind }) if token == reservation.token => {
                    if profile.definition.is_some() {
                        Some(Arc::clone(&profile.notify))
                    } else {
                        profile.lifecycle = Some(LifecycleLease::Active { token, kind });
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
        return Ok(LifecycleOperationGuard::new(move || {
            release_lifecycle(&state, &profile_id, token);
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
}

fn release_definition(state: &LockState, profile_id: &ProfileId, token: u64) {
    let mut profiles = lock_profiles(state);
    if let Some(profile) = profiles.get_mut(profile_id) {
        if profile.definition == Some(token) {
            profile.definition = None;
            profile.notify.notify_one();
            remove_if_idle(&mut profiles, profile_id);
        }
    }
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
    loop {
        if let Ok(profiles) = state.profiles.try_lock() {
            return profiles;
        }
        std::thread::yield_now();
    }
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
