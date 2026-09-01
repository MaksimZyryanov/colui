use colui_app::{
    CreateProfile, GetProfile, IdGenerator, ListProfiles, ProfileMutation, ProfileReader,
    ProfileStore, RegistrySnapshot, RemoveProfile, UpdateProfile,
};
use colui_domain::{
    AppError, AppErrorCode, ProfileDraft, ProfileId, ProjectProfile, RegistrationOrigin, Revision,
};
use std::future::Future;
use std::pin::Pin;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

type StoreFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, AppError>> + Send + 'a>>;

struct FakeProfileStore {
    state: Arc<Mutex<FakeStoreState>>,
}

struct FakeStoreState {
    snapshot: RegistrySnapshot,
    writes: u32,
}

impl FakeProfileStore {
    fn with_profiles(profiles: Vec<ProjectProfile>) -> Self {
        Self {
            state: Arc::new(Mutex::new(FakeStoreState {
                snapshot: RegistrySnapshot {
                    registry_revision: 0,
                    profiles,
                },
                writes: 0,
            })),
        }
    }

    fn write_count(&self) -> u32 {
        self.state.lock().unwrap().writes
    }

    fn snapshot(&self) -> RegistrySnapshot {
        self.state.lock().unwrap().snapshot.clone()
    }

    fn mutation_lock_is_held(&self) -> bool {
        self.state.try_lock().is_err()
    }
}

impl ProfileReader for FakeProfileStore {
    fn load(&self) -> StoreFuture<'_, RegistrySnapshot> {
        let snapshot = self.state.lock().unwrap().snapshot.clone();
        Box::pin(async move { Ok(snapshot) })
    }
}

impl ProfileStore for FakeProfileStore {
    fn mutate(&self, mutation: ProfileMutation) -> StoreFuture<'_, RegistrySnapshot> {
        let state = self.state.clone();
        Box::pin(async move {
            let mut state = state.lock().unwrap();
            let next = mutation(state.snapshot.clone())?;
            state.snapshot = next.clone();
            state.writes += 1;
            Ok(next)
        })
    }
}

fn profile_revision(revision: u64) -> ProjectProfile {
    let mut profile = ProjectProfile::from_draft(profile_id(), draft()).unwrap();
    profile.revision = Revision::new(revision);
    profile
}

fn draft() -> ProfileDraft {
    ProfileDraft {
        display_name: "Demo".try_into().unwrap(),
        compose_project_name: "demo".try_into().unwrap(),
        working_directory: "/tmp".into(),
        compose_files: vec!["compose.yml".into()],
        environment_files: vec![],
        registration_origin: RegistrationOrigin::Manual,
    }
}

fn profile_id() -> ProfileId {
    ProfileId::new(uuid::Uuid::from_u128(1))
}

fn patch() -> colui_app::ProfilePatch {
    colui_app::ProfilePatch {
        display_name: Some("Updated".try_into().unwrap()),
        ..Default::default()
    }
}

struct FixedIdGenerator(ProfileId);

impl IdGenerator for FixedIdGenerator {
    fn generate(&self) -> ProfileId {
        self.0.clone()
    }
}

#[tokio::test]
async fn list_profile_reads_without_mutation() {
    let store = FakeProfileStore::with_profiles(vec![]);
    let before = store.write_count();
    let result = ListProfiles::new(&store).execute().await.unwrap();
    assert!(result.is_empty());
    assert_eq!(before, store.write_count());
}

#[tokio::test]
async fn update_requires_expected_revision() {
    let store = FakeProfileStore::with_profiles(vec![profile_revision(3)]);
    let error = UpdateProfile::new(&store)
        .execute(profile_id(), 2, patch())
        .await
        .unwrap_err();
    assert_eq!(error.code, AppErrorCode::ProfileRevisionConflict);
    assert_eq!(store.write_count(), 0);
}

#[tokio::test]
async fn failed_update_leaves_snapshot_revision_and_write_count_unchanged() {
    let store = FakeProfileStore::with_profiles(vec![profile_revision(3)]);
    let before_snapshot = store.snapshot();
    let before_revision = before_snapshot.registry_revision;
    let before_writes = store.write_count();

    let error = UpdateProfile::new(&store)
        .execute(
            profile_id(),
            3,
            colui_app::ProfilePatch {
                compose_files: Some(vec![]),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();

    assert_eq!(error.code, AppErrorCode::ProfileInvalid);
    assert_eq!(store.snapshot(), before_snapshot);
    assert_eq!(store.snapshot().registry_revision, before_revision);
    assert_eq!(store.write_count(), before_writes);
}

#[tokio::test]
async fn fake_mutation_holds_snapshot_lock_through_closure() {
    let store = Arc::new(FakeProfileStore::with_profiles(vec![]));
    let closure_store = store.clone();

    store
        .mutate(Box::new(move |snapshot| {
            assert!(closure_store.mutation_lock_is_held());
            Ok(snapshot)
        }))
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_mutation_cannot_enter_before_prior_write() {
    let store = Arc::new(FakeProfileStore::with_profiles(vec![]));
    let (first_entered, first_entered_receiver) = mpsc::channel();
    let (release_first, release_first_receiver) = mpsc::channel();
    let (second_entered, second_entered_receiver) = mpsc::channel();

    let first_store = store.clone();
    let first = tokio::spawn(async move {
        first_store
            .mutate(Box::new(move |snapshot| {
                first_entered.send(()).unwrap();
                release_first_receiver.recv().unwrap();
                Ok(snapshot)
            }))
            .await
            .unwrap();
    });
    first_entered_receiver.recv().unwrap();

    let second_store = store;
    let second = tokio::spawn(async move {
        second_store
            .mutate(Box::new(move |snapshot| {
                second_entered.send(()).unwrap();
                Ok(snapshot)
            }))
            .await
            .unwrap();
    });

    let entered_before_release = second_entered_receiver
        .recv_timeout(std::time::Duration::from_millis(50))
        .is_ok();
    release_first.send(()).unwrap();
    first.await.unwrap();
    second.await.unwrap();

    assert!(!entered_before_release);
}

#[tokio::test]
async fn create_uses_generated_id_and_initial_revision() {
    let store = FakeProfileStore::with_profiles(vec![]);
    let ids = FixedIdGenerator(profile_id());

    let created = CreateProfile::new(&store, &ids)
        .execute(draft())
        .await
        .unwrap();

    assert_eq!(created.id, profile_id());
    assert_eq!(created.revision, Revision::initial());
    assert_eq!(store.snapshot().registry_revision, 1);
    assert_eq!(store.write_count(), 1);
}

#[tokio::test]
async fn get_profile_reads_without_mutation() {
    let store = FakeProfileStore::with_profiles(vec![profile_revision(1)]);

    let profile = GetProfile::new(&store).execute(profile_id()).await.unwrap();

    assert_eq!(profile.id, profile_id());
    assert_eq!(store.write_count(), 0);
}

#[tokio::test]
async fn get_missing_profile_returns_typed_error() {
    let store = FakeProfileStore::with_profiles(vec![]);

    let error = GetProfile::new(&store)
        .execute(profile_id())
        .await
        .unwrap_err();

    assert_eq!(error.code, AppErrorCode::ProfileNotFound);
    assert_eq!(error.subject_id, Some(profile_id()));
}

#[tokio::test]
async fn update_changes_fields_but_preserves_id_and_advances_revisions() {
    let store = FakeProfileStore::with_profiles(vec![profile_revision(3)]);

    let updated = UpdateProfile::new(&store)
        .execute(profile_id(), 3, patch())
        .await
        .unwrap();

    assert_eq!(updated.id, profile_id());
    assert_eq!(updated.revision, Revision::new(4));
    assert_eq!(store.snapshot().registry_revision, 1);
    assert_eq!(store.write_count(), 1);
}

#[tokio::test]
async fn update_missing_profile_returns_typed_error() {
    let store = FakeProfileStore::with_profiles(vec![]);

    let error = UpdateProfile::new(&store)
        .execute(profile_id(), 1, patch())
        .await
        .unwrap_err();

    assert_eq!(error.code, AppErrorCode::ProfileNotFound);
    assert_eq!(store.write_count(), 0);
}

#[tokio::test]
async fn remove_requires_expected_revision() {
    let store = FakeProfileStore::with_profiles(vec![profile_revision(3)]);

    let error = RemoveProfile::new(&store)
        .execute(profile_id(), 2)
        .await
        .unwrap_err();

    assert_eq!(error.code, AppErrorCode::ProfileRevisionConflict);
    assert_eq!(store.write_count(), 0);
}

#[tokio::test]
async fn remove_deletes_only_matching_revision() {
    let store = FakeProfileStore::with_profiles(vec![profile_revision(3)]);

    RemoveProfile::new(&store)
        .execute(profile_id(), 3)
        .await
        .unwrap();

    assert!(store.snapshot().profiles.is_empty());
    assert_eq!(store.snapshot().registry_revision, 1);
    assert_eq!(store.write_count(), 1);
}

#[tokio::test]
async fn remove_missing_profile_returns_typed_error() {
    let store = FakeProfileStore::with_profiles(vec![]);

    let error = RemoveProfile::new(&store)
        .execute(profile_id(), 1)
        .await
        .unwrap_err();

    assert_eq!(error.code, AppErrorCode::ProfileNotFound);
    assert_eq!(store.write_count(), 0);
}
