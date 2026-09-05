use colui_app::{
    AutoRegisterCandidates, AutoRegistrationOutcome, CandidateLease, ConfigureAutoRegistration,
    DiscoveryFuture, DiscoveryReader, DiscoverySession, IdGenerator, IgnoreCandidate,
    InventoryFuture, InventoryReader, InventoryRefresher, JournalEventKind,
    ListDiscoveryCandidates, ProfileMutation, ProfileReader, ProfileStore, RegisterCandidate,
    RegisterCandidateRequest, RegistrySnapshot, ScheduleAutoRegistration, StoreFuture,
};
use colui_domain::{
    AppError, AppErrorCode, AppErrorSubjectKind, ComposeObservationGroup, ContainerId,
    DaemonFingerprint, DiscoveryClassification, InventoryFreshness, ProfileDraft, ProfileId,
    ProjectProfile, RegistrationOrigin, RuntimeInventory, RuntimeSessionId, Timestamp,
};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

struct CountingInventory {
    reads: AtomicUsize,
    inventory: RuntimeInventory,
}

impl InventoryReader for CountingInventory {
    fn current_inventory(&self) -> InventoryFuture<'_, RuntimeInventory> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(self.inventory.clone()) })
    }
}

impl InventoryRefresher for CountingInventory {
    fn refresh(&self) -> InventoryFuture<'_, RuntimeInventory> {
        panic!("pure discovery query must not refresh inventory")
    }
}

struct CountingRegistry {
    reads: AtomicUsize,
    writes: AtomicUsize,
    profiles: Vec<ProjectProfile>,
}

impl ProfileReader for CountingRegistry {
    fn load(&self) -> StoreFuture<'_, RegistrySnapshot> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {
            Ok(RegistrySnapshot {
                registry_revision: 0,
                profiles: self.profiles.clone(),
            })
        })
    }
}

impl ProfileStore for CountingRegistry {
    fn mutate(&self, _mutation: ProfileMutation) -> StoreFuture<'_, RegistrySnapshot> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        panic!("pure discovery query must not mutate registry")
    }
}

fn session(value: u128) -> RuntimeSessionId {
    RuntimeSessionId::new(Uuid::from_u128(value))
}

fn registered_profile() -> ProjectProfile {
    ProjectProfile::from_draft(
        ProfileId::parse("00000000-0000-0000-0000-000000000001").unwrap(),
        ProfileDraft {
            display_name: "Demo".try_into().unwrap(),
            compose_project_name: "demo".try_into().unwrap(),
            working_directory: "/work/demo".into(),
            compose_files: vec!["compose.yml".into()],
            environment_files: vec![],
            registration_origin: RegistrationOrigin::Manual,
        },
    )
    .unwrap()
}

fn inventory(
    runtime_session_id: Option<RuntimeSessionId>,
    generation: u64,
    files: &[&str],
) -> RuntimeInventory {
    RuntimeInventory {
        generation,
        has_snapshot: runtime_session_id.is_some(),
        observed_at: Some(Timestamp("2026-09-05T00:00:00Z".into())),
        runtime_session_id,
        daemon_fingerprint: Some(DaemonFingerprint::new("daemon", "1.0", "linux", "amd64")),
        freshness: InventoryFreshness::Fresh,
        last_successful_observed_at: None,
        containers: vec![],
        project_snapshots: vec![],
        compose_observation_groups: vec![ComposeObservationGroup {
            compose_project_name: "demo".into(),
            working_directory: Some("/work/demo".into()),
            config_files: files.iter().map(|value| (*value).into()).collect(),
            container_ids: vec![ContainerId("container".into())],
        }],
        standalone_containers: vec![],
        error: None,
    }
}

#[tokio::test]
async fn list_is_snapshot_only_and_never_mutates_registry() {
    let inventory = CountingInventory {
        reads: AtomicUsize::new(0),
        inventory: inventory(Some(session(1)), 7, &["compose.yml"]),
    };
    let registry = CountingRegistry {
        reads: AtomicUsize::new(0),
        writes: AtomicUsize::new(0),
        profiles: vec![],
    };
    let state = DiscoverySession::new();

    let candidates = ListDiscoveryCandidates::new(&inventory, &registry, &state)
        .execute()
        .await
        .unwrap();

    assert_eq!(candidates.len(), 1);
    assert_eq!(
        candidates[0].classification,
        DiscoveryClassification::NewUnambiguous
    );
    assert_eq!(inventory.reads.load(Ordering::SeqCst), 1);
    assert_eq!(registry.reads.load(Ordering::SeqCst), 1);
    assert_eq!(registry.writes.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn ignore_is_idempotent_and_resets_on_reconnect_while_journal_survives() {
    let state = DiscoverySession::new();
    let first = state
        .observe_inventory(inventory(Some(session(1)), 1, &["compose.yml"]), vec![])
        .await;
    let id = first[0].candidate_id.clone();

    assert!(IgnoreCandidate::new(&state).execute(id.clone()).await);
    assert!(!IgnoreCandidate::new(&state).execute(id).await);
    assert_eq!(state.journal().await.entries.len(), 1);
    assert!(state.candidates().await[0].ignored);

    state
        .observe_inventory(inventory(Some(session(2)), 2, &["compose.yml"]), vec![])
        .await;
    assert!(!state.candidates().await[0].ignored);
    assert_eq!(state.journal().await.entries.len(), 1);
}

#[tokio::test]
async fn journal_is_exact_fifo_and_uses_closed_redacted_templates() {
    let state = DiscoverySession::new();
    for count in 0..257 {
        state
            .record_counted_event(
                JournalEventKind::ConnectSucceeded,
                Some(session(1)),
                count,
                "SECRET=/work/private",
            )
            .await;
    }

    let journal = state.journal().await;
    assert_eq!(journal.entries.len(), 256);
    assert_eq!(journal.entries[0].sequence, 2);
    assert_eq!(journal.entries[255].sequence, 257);
    assert!(journal
        .entries
        .iter()
        .all(|entry| !entry.message.contains("SECRET")));
    assert!(journal
        .entries
        .iter()
        .all(|entry| entry.message.chars().count() <= 500));
}

#[tokio::test]
async fn auto_policy_defaults_disabled_deduplicates_and_cancels_pending_work() {
    let state = DiscoverySession::new();
    let registry = CountingRegistry {
        reads: AtomicUsize::new(0),
        writes: AtomicUsize::new(0),
        profiles: vec![],
    };
    assert!(!state.auto_registration_enabled().await);
    assert!(ScheduleAutoRegistration::new(&registry, &state)
        .execute(inventory(Some(session(1)), 1, &["compose.yml"]))
        .await
        .unwrap()
        .is_none());

    assert!(ConfigureAutoRegistration::new(&state).execute(true).await);
    let schedule = ScheduleAutoRegistration::new(&registry, &state)
        .execute(inventory(Some(session(1)), 1, &["compose.yml"]))
        .await
        .unwrap()
        .unwrap();
    assert!(ScheduleAutoRegistration::new(&registry, &state)
        .execute(inventory(Some(session(1)), 1, &["compose.yml"]))
        .await
        .unwrap()
        .is_none());
    assert!(!ConfigureAutoRegistration::new(&state).execute(false).await);
    assert!(state.claim_schedule(&schedule).await.is_empty());
}

#[tokio::test]
async fn publication_claim_completion_lifecycle_controls_metadata_dedup() {
    let state = DiscoverySession::new();
    let registry = CountingRegistry {
        reads: AtomicUsize::new(0),
        writes: AtomicUsize::new(0),
        profiles: vec![],
    };
    ConfigureAutoRegistration::new(&state).execute(true).await;
    let first = ScheduleAutoRegistration::new(&registry, &state)
        .execute(inventory(Some(session(1)), 1, &["compose.yml"]))
        .await
        .unwrap()
        .unwrap();
    let unclaimed = state.candidates().await.remove(0);
    assert!(
        !state
            .complete_auto_candidate(
                &first,
                &unclaimed,
                AutoRegistrationOutcome::RetryableFailure,
            )
            .await
    );
    let candidate = state.claim_schedule(&first).await.remove(0);
    assert!(
        state
            .complete_auto_candidate(
                &first,
                &candidate,
                AutoRegistrationOutcome::RetryableFailure
            )
            .await
    );

    let retry = ScheduleAutoRegistration::new(&registry, &state)
        .execute(inventory(Some(session(1)), 2, &["compose.yml"]))
        .await
        .unwrap()
        .unwrap();
    let retry_candidate = state.claim_schedule(&retry).await.remove(0);
    assert!(
        state
            .complete_auto_candidate(&retry, &retry_candidate, AutoRegistrationOutcome::Succeeded)
            .await
    );
    assert!(ScheduleAutoRegistration::new(&registry, &state)
        .execute(inventory(Some(session(1)), 3, &["compose.yml"]))
        .await
        .unwrap()
        .is_none());
    assert!(ScheduleAutoRegistration::new(&registry, &state)
        .execute(inventory(Some(session(1)), 4, &["other.yml"]))
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn registered_profile_suppresses_auto_registration() {
    let state = DiscoverySession::new();
    let registry = CountingRegistry {
        reads: AtomicUsize::new(0),
        writes: AtomicUsize::new(0),
        profiles: vec![registered_profile()],
    };
    ConfigureAutoRegistration::new(&state).execute(true).await;

    let schedule = ScheduleAutoRegistration::new(&registry, &state)
        .execute(inventory(Some(session(1)), 1, &["compose.yml"]))
        .await
        .unwrap();

    assert!(schedule.is_none());
    assert_eq!(registry.reads.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn late_completion_cannot_change_dedup_state() {
    let state = DiscoverySession::new();
    let registry = CountingRegistry {
        reads: AtomicUsize::new(0),
        writes: AtomicUsize::new(0),
        profiles: vec![],
    };
    ConfigureAutoRegistration::new(&state).execute(true).await;
    let first = ScheduleAutoRegistration::new(&registry, &state)
        .execute(inventory(Some(session(1)), 1, &["compose.yml"]))
        .await
        .unwrap()
        .unwrap();
    let candidate = state.claim_schedule(&first).await.remove(0);
    ScheduleAutoRegistration::new(&registry, &state)
        .execute(inventory(Some(session(1)), 2, &["other.yml"]))
        .await
        .unwrap();

    assert!(
        !state
            .complete_auto_candidate(&first, &candidate, AutoRegistrationOutcome::Succeeded)
            .await
    );
    assert!(ScheduleAutoRegistration::new(&registry, &state)
        .execute(inventory(Some(session(1)), 3, &["compose.yml"]))
        .await
        .unwrap()
        .is_some());
}

struct RegistrationInventory {
    inventory: Mutex<RuntimeInventory>,
    lease_valid: Arc<AtomicBool>,
}

impl InventoryReader for RegistrationInventory {
    fn current_inventory(&self) -> InventoryFuture<'_, RuntimeInventory> {
        let inventory = self.inventory.lock().unwrap().clone();
        Box::pin(async move { Ok(inventory) })
    }
}

impl DiscoveryReader for RegistrationInventory {
    fn lease_candidate(
        &self,
        runtime_session_id: RuntimeSessionId,
        inventory_generation: u64,
    ) -> DiscoveryFuture<'_, CandidateLease> {
        let current = self.inventory.lock().unwrap().clone();
        let valid = self.lease_valid.clone();
        Box::pin(async move {
            if current.runtime_session_id != Some(runtime_session_id)
                || current.generation != inventory_generation
            {
                return Err(AppError::new(
                    AppErrorCode::RuntimeUnavailable,
                    "lease",
                    None,
                    "changed",
                ));
            }
            Ok(CandidateLease::hold_validated((), move || {
                valid.load(Ordering::SeqCst)
            }))
        })
    }
}

#[derive(Clone)]
struct RegistrationStore {
    state: Arc<Mutex<RegistrationStoreState>>,
}

struct RegistrationStoreState {
    snapshot: RegistrySnapshot,
    writes: usize,
    before_mutation: Option<Box<dyn FnOnce() + Send>>,
    error: Option<AppError>,
}

impl RegistrationStore {
    fn new(profiles: Vec<ProjectProfile>) -> Self {
        Self {
            state: Arc::new(Mutex::new(RegistrationStoreState {
                snapshot: RegistrySnapshot {
                    registry_revision: 0,
                    profiles,
                },
                writes: 0,
                before_mutation: None,
                error: None,
            })),
        }
    }

    fn writes(&self) -> usize {
        self.state.lock().unwrap().writes
    }
}

impl ProfileReader for RegistrationStore {
    fn load(&self) -> StoreFuture<'_, RegistrySnapshot> {
        let snapshot = self.state.lock().unwrap().snapshot.clone();
        Box::pin(async move { Ok(snapshot) })
    }
}

impl ProfileStore for RegistrationStore {
    fn mutate(&self, mutation: ProfileMutation) -> StoreFuture<'_, RegistrySnapshot> {
        let state = self.state.clone();
        Box::pin(async move {
            let mut state = state.lock().unwrap();
            if let Some(error) = state.error.clone() {
                return Err(error);
            }
            if let Some(before) = state.before_mutation.take() {
                before();
            }
            let next = mutation(state.snapshot.clone())?;
            state.snapshot = next.clone();
            state.writes += 1;
            Ok(next)
        })
    }
}

struct SequenceIds(Mutex<VecDeque<ProfileId>>);

impl SequenceIds {
    fn starting_at(first: u128) -> Self {
        Self(Mutex::new(
            (first..first + 64)
                .map(|value| ProfileId::new(Uuid::from_u128(value)))
                .collect(),
        ))
    }
}

impl IdGenerator for SequenceIds {
    fn generate(&self) -> ProfileId {
        self.0.lock().unwrap().pop_front().unwrap()
    }
}

async fn registration_fixture() -> (
    RegistrationInventory,
    RegistrationStore,
    DiscoverySession,
    RegisterCandidateRequest,
) {
    let current = inventory(Some(session(9)), 3, &["compose.yml"]);
    let state = DiscoverySession::new();
    let candidates = state.observe_inventory(current.clone(), vec![]).await;
    (
        RegistrationInventory {
            inventory: Mutex::new(current),
            lease_valid: Arc::new(AtomicBool::new(true)),
        },
        RegistrationStore::new(vec![]),
        state,
        RegisterCandidateRequest::from(&candidates[0]),
    )
}

#[tokio::test]
async fn registration_rejects_disappeared_or_changed_candidate_without_write() {
    let (inventory, store, _state, request) = registration_fixture().await;
    inventory
        .inventory
        .lock()
        .unwrap()
        .compose_observation_groups
        .clear();
    let error = RegisterCandidate::new(&inventory, &store, &SequenceIds::starting_at(100))
        .execute(request)
        .await
        .unwrap_err();

    assert_eq!(error.code, AppErrorCode::CandidateStale);
    assert_eq!(error.subject.unwrap().kind, AppErrorSubjectKind::Candidate);
    assert_eq!(store.writes(), 0);
}

#[tokio::test]
async fn locked_recheck_detects_reconnect_and_matching_profile_lost_races() {
    let (inventory, store, _state, request) = registration_fixture().await;
    let valid = inventory.lease_valid.clone();
    store.state.lock().unwrap().before_mutation = Some(Box::new(move || {
        valid.store(false, Ordering::SeqCst);
    }));
    let error = RegisterCandidate::new(&inventory, &store, &SequenceIds::starting_at(100))
        .execute(request.clone())
        .await
        .unwrap_err();
    assert_eq!(error.code, AppErrorCode::CandidateStale);
    assert_eq!(store.writes(), 0);

    inventory.lease_valid.store(true, Ordering::SeqCst);
    store.state.lock().unwrap().before_mutation = Some(Box::new(|| {}));
    store.state.lock().unwrap().snapshot.profiles = vec![registered_profile()];
    let error = RegisterCandidate::new(&inventory, &store, &SequenceIds::starting_at(200))
        .execute(request)
        .await
        .unwrap_err();
    assert_eq!(error.code, AppErrorCode::ProfileAlreadyRegistered);
    assert!(error
        .details
        .unwrap()
        .contains("00000000-0000-0000-0000-000000000001"));
    assert_eq!(store.writes(), 0);
}

#[tokio::test]
async fn two_registrars_and_name_conflicts_never_update_existing_profiles() {
    let (inventory, store, _state, request) = registration_fixture().await;
    let first = RegisterCandidate::new(&inventory, &store, &SequenceIds::starting_at(100))
        .execute(request.clone())
        .await
        .unwrap();
    let error = RegisterCandidate::new(&inventory, &store, &SequenceIds::starting_at(200))
        .execute(request.clone())
        .await
        .unwrap_err();
    assert_eq!(error.code, AppErrorCode::ProfileAlreadyRegistered);
    assert_eq!(store.writes(), 1);
    assert_eq!(store.state.lock().unwrap().snapshot.profiles[0], first);

    store.state.lock().unwrap().snapshot.profiles[0].working_directory = "/other".into();
    let error = RegisterCandidate::new(&inventory, &store, &SequenceIds::starting_at(300))
        .execute(request)
        .await
        .unwrap_err();
    assert_eq!(error.code, AppErrorCode::DiscoveryConflict);
    assert_eq!(store.writes(), 1);
}

#[tokio::test]
async fn registration_uses_discovered_draft_and_bounds_id_collisions_at_sixteen() {
    let (inventory, store, _state, request) = registration_fixture().await;
    let colliding: Vec<_> = (1..=16)
        .map(|value| {
            ProjectProfile::from_draft(
                ProfileId::new(Uuid::from_u128(value)),
                ProfileDraft {
                    display_name: format!("Other {value}").try_into().unwrap(),
                    compose_project_name: format!("other-{value}").try_into().unwrap(),
                    working_directory: format!("/other/{value}").into(),
                    compose_files: vec!["compose.yml".into()],
                    environment_files: vec![],
                    registration_origin: RegistrationOrigin::Manual,
                },
            )
            .unwrap()
        })
        .collect();
    store.state.lock().unwrap().snapshot.profiles = colliding;
    let ids = SequenceIds(Mutex::new(
        (1..=16)
            .map(|value| ProfileId::new(Uuid::from_u128(value)))
            .collect(),
    ));
    let error = RegisterCandidate::new(&inventory, &store, &ids)
        .execute(request)
        .await
        .unwrap_err();
    assert_eq!(error.code, AppErrorCode::RegistryWriteFailed);
    assert_eq!(store.writes(), 0);

    let (inventory, store, _state, request) = registration_fixture().await;
    let profile = RegisterCandidate::new(&inventory, &store, &SequenceIds::starting_at(100))
        .execute(request)
        .await
        .unwrap();
    assert_eq!(profile.display_name.as_ref(), "demo");
    assert_eq!(profile.compose_project_name.as_ref(), "demo");
    assert_eq!(profile.working_directory.to_str(), Some("/work/demo"));
    assert_eq!(
        profile.compose_files[0].to_str(),
        Some("/work/demo/compose.yml")
    );
    assert!(profile.environment_files.is_empty());
    assert_eq!(profile.registration_origin, RegistrationOrigin::Discovered);
}

#[tokio::test]
async fn registration_preserves_corrupt_locked_and_write_failed_registry_errors() {
    for (code, retryable) in [
        (AppErrorCode::RegistryCorrupt, false),
        (AppErrorCode::RegistryLocked, true),
        (AppErrorCode::RegistryWriteFailed, true),
    ] {
        let (inventory, store, _state, request) = registration_fixture().await;
        store.state.lock().unwrap().error = Some(AppError::new(
            code,
            "registry_mutation",
            None,
            "excluded upstream detail",
        ));

        let error = RegisterCandidate::new(&inventory, &store, &SequenceIds::starting_at(100))
            .execute(request)
            .await
            .unwrap_err();

        assert_eq!(error.code, code);
        assert_eq!(error.retryable, retryable);
        assert_eq!(
            error.subject.as_ref().map(|subject| subject.kind),
            Some(AppErrorSubjectKind::Registry)
        );
        assert_eq!(store.writes(), 0);
    }
}

#[tokio::test]
async fn auto_registration_retries_only_on_next_generation_and_manual_bypasses_dedup() {
    let (inventory, store, state, request) = registration_fixture().await;
    ConfigureAutoRegistration::new(&state).execute(true).await;
    let schedule = ScheduleAutoRegistration::new(&store, &state)
        .execute(inventory.inventory.lock().unwrap().clone())
        .await
        .unwrap()
        .unwrap();
    store.state.lock().unwrap().error = Some(AppError::new(
        AppErrorCode::RegistryLocked,
        "mutate",
        None,
        "locked",
    ));
    let results =
        AutoRegisterCandidates::new(&inventory, &store, &SequenceIds::starting_at(100), &state)
            .execute(schedule)
            .await;
    assert_eq!(
        results[0].as_ref().unwrap_err().code,
        AppErrorCode::RegistryLocked
    );

    store.state.lock().unwrap().error = None;
    assert!(ScheduleAutoRegistration::new(&store, &state)
        .execute(inventory.inventory.lock().unwrap().clone())
        .await
        .unwrap()
        .is_none());
    inventory.inventory.lock().unwrap().generation = 4;
    let retry = ScheduleAutoRegistration::new(&store, &state)
        .execute(inventory.inventory.lock().unwrap().clone())
        .await
        .unwrap()
        .unwrap();
    assert!(AutoRegisterCandidates::new(
        &inventory,
        &store,
        &SequenceIds::starting_at(200),
        &state,
    )
    .execute(retry)
    .await[0]
        .is_ok());

    let error = RegisterCandidate::new(&inventory, &store, &SequenceIds::starting_at(300))
        .execute(RegisterCandidateRequest {
            inventory_generation: 4,
            ..request
        })
        .await
        .unwrap_err();
    assert_eq!(error.code, AppErrorCode::ProfileAlreadyRegistered);
}
