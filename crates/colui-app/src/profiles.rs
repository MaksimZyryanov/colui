use colui_domain::{
    AppError, AppErrorCode, ComposeProjectName, DisplayName, ProfileDraft, ProfileId,
    ProjectProfile, Revision,
};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

pub type StoreFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, AppError>> + Send + 'a>>;
pub type ProfileMutation =
    Box<dyn FnOnce(RegistrySnapshot) -> Result<RegistrySnapshot, AppError> + Send>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistrySnapshot {
    pub registry_revision: u64,
    pub profiles: Vec<ProjectProfile>,
}

pub trait ProfileReader {
    fn load(&self) -> StoreFuture<'_, RegistrySnapshot>;
}

pub trait ProfileStore: ProfileReader {
    fn mutate(&self, mutation: ProfileMutation) -> StoreFuture<'_, RegistrySnapshot>;
}

pub trait IdGenerator {
    fn generate(&self) -> ProfileId;
}

pub struct CreateProfile<'a, S, G> {
    store: &'a S,
    ids: &'a G,
}

impl<'a, S: ProfileStore, G: IdGenerator> CreateProfile<'a, S, G> {
    pub fn new(store: &'a S, ids: &'a G) -> Self {
        Self { store, ids }
    }

    pub async fn execute(&self, draft: ProfileDraft) -> Result<ProjectProfile, AppError> {
        let id = self.ids.generate();
        let profile = ProjectProfile::from_draft(id.clone(), draft)?;
        let snapshot = self
            .store
            .mutate(Box::new(move |mut snapshot| {
                if snapshot
                    .profiles
                    .iter()
                    .any(|current| current.id == profile.id)
                {
                    return Err(AppError::new(
                        AppErrorCode::ProfileAlreadyRegistered,
                        "create_profile",
                        Some(profile.id),
                        "profile is already registered",
                    ));
                }
                snapshot.profiles.push(profile);
                snapshot.registry_revision += 1;
                Ok(snapshot)
            }))
            .await?;

        snapshot
            .profiles
            .into_iter()
            .find(|current| current.id == id)
            .ok_or_else(|| not_found("create_profile", id))
    }
}

pub struct ListProfiles<'a, R> {
    reader: &'a R,
}

impl<'a, R: ProfileReader> ListProfiles<'a, R> {
    pub fn new(reader: &'a R) -> Self {
        Self { reader }
    }

    pub async fn execute(&self) -> Result<Vec<ProjectProfile>, AppError> {
        Ok(self.reader.load().await?.profiles)
    }
}

pub struct GetProfile<'a, R> {
    reader: &'a R,
}

impl<'a, R: ProfileReader> GetProfile<'a, R> {
    pub fn new(reader: &'a R) -> Self {
        Self { reader }
    }

    pub async fn execute(&self, id: ProfileId) -> Result<ProjectProfile, AppError> {
        self.reader
            .load()
            .await?
            .profiles
            .into_iter()
            .find(|profile| profile.id == id)
            .ok_or_else(|| not_found("get_profile", id))
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProfilePatch {
    pub display_name: Option<DisplayName>,
    pub compose_project_name: Option<ComposeProjectName>,
    pub working_directory: Option<PathBuf>,
    pub compose_files: Option<Vec<PathBuf>>,
    pub environment_files: Option<Vec<PathBuf>>,
}

pub struct UpdateProfile<'a, S> {
    store: &'a S,
}

impl<'a, S: ProfileStore> UpdateProfile<'a, S> {
    pub fn new(store: &'a S) -> Self {
        Self { store }
    }

    pub async fn execute(
        &self,
        id: ProfileId,
        expected_revision: u64,
        patch: ProfilePatch,
    ) -> Result<ProjectProfile, AppError> {
        let result_id = id.clone();
        let snapshot = self
            .store
            .mutate(Box::new(move |mut snapshot| {
                let position = snapshot
                    .profiles
                    .iter()
                    .position(|profile| profile.id == id)
                    .ok_or_else(|| not_found("update_profile", id.clone()))?;
                let current = &snapshot.profiles[position];
                require_revision("update_profile", current, expected_revision)?;

                let draft = ProfileDraft {
                    display_name: patch
                        .display_name
                        .unwrap_or_else(|| current.display_name.clone()),
                    compose_project_name: patch
                        .compose_project_name
                        .unwrap_or_else(|| current.compose_project_name.clone()),
                    working_directory: patch
                        .working_directory
                        .unwrap_or_else(|| current.working_directory.clone()),
                    compose_files: patch
                        .compose_files
                        .unwrap_or_else(|| current.compose_files.clone()),
                    environment_files: patch
                        .environment_files
                        .unwrap_or_else(|| current.environment_files.clone()),
                    registration_origin: current.registration_origin.clone(),
                };
                let mut updated = ProjectProfile::from_draft(id, draft)?;
                updated.revision = current.revision.next();
                snapshot.profiles[position] = updated;
                snapshot.registry_revision += 1;
                Ok(snapshot)
            }))
            .await?;

        snapshot
            .profiles
            .into_iter()
            .find(|profile| profile.id == result_id)
            .ok_or_else(|| not_found("update_profile", result_id))
    }
}

pub struct RemoveProfile<'a, S> {
    store: &'a S,
}

impl<'a, S: ProfileStore> RemoveProfile<'a, S> {
    pub fn new(store: &'a S) -> Self {
        Self { store }
    }

    pub async fn execute(&self, id: ProfileId, expected_revision: u64) -> Result<(), AppError> {
        self.store
            .mutate(Box::new(move |mut snapshot| {
                let position = snapshot
                    .profiles
                    .iter()
                    .position(|profile| profile.id == id)
                    .ok_or_else(|| not_found("remove_profile", id.clone()))?;
                require_revision(
                    "remove_profile",
                    &snapshot.profiles[position],
                    expected_revision,
                )?;
                snapshot.profiles.remove(position);
                snapshot.registry_revision += 1;
                Ok(snapshot)
            }))
            .await?;
        Ok(())
    }
}

fn require_revision(
    operation: &str,
    profile: &ProjectProfile,
    expected_revision: u64,
) -> Result<(), AppError> {
    if profile.revision == Revision::new(expected_revision) {
        Ok(())
    } else {
        Err(AppError::new(
            AppErrorCode::ProfileRevisionConflict,
            operation,
            Some(profile.id.clone()),
            "profile revision conflict",
        ))
    }
}

fn not_found(operation: &str, id: ProfileId) -> AppError {
    AppError::new(
        AppErrorCode::ProfileNotFound,
        operation,
        Some(id),
        "profile not found",
    )
}
