use colui_app::{
    ApplyProject, LifecycleFuture, LifecycleOperation, LifecycleResult, LifecycleRuntime,
    ProfileReader, RegistrySnapshot, StopProject, TearDownProject,
};
use colui_domain::{
    AppError, AppErrorCode, ComposeProjectName, DisplayName, ProfileDraft, ProfileId,
    ProjectProfile, RegistrationOrigin,
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

struct FakeReader {
    snapshot: RegistrySnapshot,
}

impl FakeReader {
    fn with_profile(profile: ProjectProfile) -> Self {
        Self {
            snapshot: RegistrySnapshot {
                registry_revision: 1,
                profiles: vec![profile],
            },
        }
    }
}

impl ProfileReader for FakeReader {
    fn load(&self) -> colui_app::StoreFuture<'_, RegistrySnapshot> {
        let snapshot = self.snapshot.clone();
        Box::pin(async move { Ok(snapshot) })
    }
}

struct FakeRuntime {
    operations: Arc<Mutex<Vec<LifecycleOperation>>>,
    error: Option<AppError>,
}

impl FakeRuntime {
    fn ready() -> Self {
        Self {
            operations: Arc::new(Mutex::new(Vec::new())),
            error: None,
        }
    }

    fn context_mismatch() -> Self {
        Self {
            operations: Arc::new(Mutex::new(Vec::new())),
            error: Some(AppError::new(
                AppErrorCode::RuntimeContextMismatch,
                "lifecycle",
                None,
                "runtime context is not verified",
            )),
        }
    }

    fn operations(&self) -> Vec<LifecycleOperation> {
        self.operations.lock().unwrap().clone()
    }

    fn invocations(&self) -> Vec<LifecycleInvocation> {
        self.operations()
            .into_iter()
            .map(|operation| LifecycleInvocation { operation })
            .collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LifecycleInvocation {
    operation: LifecycleOperation,
}

impl LifecycleRuntime for FakeRuntime {
    fn run_profile(
        &self,
        profile: ProjectProfile,
        operation: LifecycleOperation,
    ) -> LifecycleFuture<'_, LifecycleResult> {
        let operations = self.operations.clone();
        let error = self.error.clone();
        Box::pin(async move {
            if let Some(error) = error {
                return Err(error);
            }
            operations.lock().unwrap().push(operation);
            Ok(LifecycleResult {
                profile_id: profile.id,
                success: true,
            })
        })
    }
}

fn profile_id(value: &str) -> ProfileId {
    let uuid = match value {
        "id-1" => Uuid::from_u128(1),
        "missing" => Uuid::from_u128(2),
        _ => Uuid::from_u128(3),
    };
    ProfileId::new(uuid)
}

fn profile(value: &str) -> ProjectProfile {
    ProjectProfile::from_draft(
        profile_id(value),
        ProfileDraft {
            display_name: DisplayName::try_from("Demo").unwrap(),
            compose_project_name: ComposeProjectName::try_from("demo").unwrap(),
            working_directory: PathBuf::from("/tmp/demo"),
            compose_files: vec![PathBuf::from("compose.yml")],
            environment_files: vec![],
            registration_origin: RegistrationOrigin::Manual,
        },
    )
    .unwrap()
}

#[tokio::test]
async fn apply_looks_up_profile_and_uses_up_without_caller_paths() {
    let reader = FakeReader::with_profile(profile("id-1"));
    let runtime = FakeRuntime::ready();
    let result = ApplyProject::new(&reader, &runtime)
        .execute(profile_id("id-1"))
        .await
        .unwrap();
    assert_eq!(result.profile_id, profile_id("id-1"));
    assert_eq!(
        runtime.invocations()[0].operation,
        LifecycleOperation::Apply
    );
}

#[tokio::test]
async fn stop_and_tear_down_are_distinct() {
    let reader = FakeReader::with_profile(profile("id-1"));
    let runtime = FakeRuntime::ready();
    StopProject::new(&reader, &runtime)
        .execute(profile_id("id-1"))
        .await
        .unwrap();
    TearDownProject::new(&reader, &runtime)
        .execute(profile_id("id-1"))
        .await
        .unwrap();
    assert_eq!(
        runtime.operations(),
        vec![LifecycleOperation::Stop, LifecycleOperation::TearDown]
    );
}

#[tokio::test]
async fn mismatch_blocks_lifecycle_before_runner_call() {
    let reader = FakeReader::with_profile(profile("id-1"));
    let runtime = FakeRuntime::context_mismatch();
    let error = ApplyProject::new(&reader, &runtime)
        .execute(profile_id("id-1"))
        .await
        .unwrap_err();
    assert_eq!(error.code, AppErrorCode::RuntimeContextMismatch);
}

#[tokio::test]
async fn missing_profile_returns_not_found_before_runtime_call() {
    let reader = FakeReader::with_profile(profile("id-1"));
    let runtime = FakeRuntime::ready();
    let error = ApplyProject::new(&reader, &runtime)
        .execute(profile_id("missing"))
        .await
        .unwrap_err();
    assert_eq!(error.code, AppErrorCode::ProfileNotFound);
    assert!(runtime.operations().is_empty());
}
