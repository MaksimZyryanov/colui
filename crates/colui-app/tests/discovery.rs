use colui_app::{
    AutoRegistrationOutcome, ConfigureAutoRegistration, DiscoverySession, IgnoreCandidate,
    InventoryFuture, InventoryReader, InventoryRefresher, JournalEventKind,
    ListDiscoveryCandidates, ProfileMutation, ProfileReader, ProfileStore, RegistrySnapshot,
    StoreFuture,
};
use colui_domain::{
    ComposeObservationGroup, ContainerId, DaemonFingerprint, DiscoveryClassification,
    InventoryFreshness, RuntimeInventory, RuntimeSessionId, Timestamp,
};
use std::sync::atomic::{AtomicUsize, Ordering};
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
}

impl ProfileReader for CountingRegistry {
    fn load(&self) -> StoreFuture<'_, RegistrySnapshot> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {
            Ok(RegistrySnapshot {
                registry_revision: 0,
                profiles: vec![],
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
    assert!(!state.auto_registration_enabled().await);
    assert!(state
        .observe_successful_publication(&inventory(Some(session(1)), 1, &["compose.yml"]))
        .await
        .is_none());

    assert!(ConfigureAutoRegistration::new(&state).execute(true).await);
    let schedule = state
        .observe_successful_publication(&inventory(Some(session(1)), 1, &["compose.yml"]))
        .await
        .unwrap();
    assert!(state
        .observe_successful_publication(&inventory(Some(session(1)), 1, &["compose.yml"]))
        .await
        .is_none());
    assert!(!ConfigureAutoRegistration::new(&state).execute(false).await);
    assert!(!state.claim_schedule(&schedule).await);
}

#[tokio::test]
async fn metadata_hash_and_outcomes_control_rescheduling() {
    let state = DiscoverySession::new();
    ConfigureAutoRegistration::new(&state).execute(true).await;
    let first_inventory = inventory(Some(session(1)), 1, &["compose.yml"]);
    state
        .observe_inventory(first_inventory.clone(), vec![])
        .await;
    let first = state
        .observe_successful_publication(&first_inventory)
        .await
        .unwrap();
    let candidate = state.candidates().await.remove(0);
    state
        .complete_auto_candidate(
            &first,
            &candidate,
            AutoRegistrationOutcome::RetryableFailure,
        )
        .await;

    let next = inventory(Some(session(1)), 2, &["compose.yml"]);
    assert!(state.observe_successful_publication(&next).await.is_some());
    state.observe_inventory(next.clone(), vec![]).await;
    let retry = state
        .observe_successful_publication(&inventory(Some(session(1)), 3, &["compose.yml"]))
        .await
        .unwrap();
    state
        .complete_auto_candidate(
            &retry,
            &state.candidates().await[0],
            AutoRegistrationOutcome::Succeeded,
        )
        .await;
    assert!(state
        .observe_successful_publication(&inventory(Some(session(1)), 4, &["compose.yml"]))
        .await
        .is_none());
    assert!(state
        .observe_successful_publication(&inventory(Some(session(1)), 5, &["other.yml"]))
        .await
        .is_some());
}
