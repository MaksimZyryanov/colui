use crate::ProfileReader;
use colui_domain::{AppError, AppErrorCode, ProfileId, ProjectProfile, RuntimeInventory};
use std::future::Future;
use std::pin::Pin;

pub type LifecycleFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, AppError>> + Send + 'a>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecycleOperation {
    Apply,
    Stop,
    TearDown,
    Restart,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LifecycleResult {
    pub profile_id: ProfileId,
    pub success: bool,
    pub inventory: RuntimeInventory,
}

pub trait LifecycleRuntime: Send + Sync {
    fn run_profile(
        &self,
        profile: ProjectProfile,
        operation: LifecycleOperation,
    ) -> LifecycleFuture<'_, LifecycleResult>;
}

struct LifecycleUseCase<'a, R: ?Sized, T: ?Sized> {
    reader: &'a R,
    runtime: &'a T,
    operation: LifecycleOperation,
}

impl<'a, R: ProfileReader + ?Sized, T: LifecycleRuntime + ?Sized> LifecycleUseCase<'a, R, T> {
    fn new(reader: &'a R, runtime: &'a T, operation: LifecycleOperation) -> Self {
        Self {
            reader,
            runtime,
            operation,
        }
    }

    async fn execute(&self, id: ProfileId) -> Result<LifecycleResult, AppError> {
        let profile = self
            .reader
            .load()
            .await?
            .profiles
            .into_iter()
            .find(|profile| profile.id == id)
            .ok_or_else(|| {
                AppError::new(
                    AppErrorCode::ProfileNotFound,
                    lifecycle_name(self.operation),
                    Some(id),
                    "profile not found",
                )
            })?;
        self.runtime.run_profile(profile, self.operation).await
    }
}

macro_rules! lifecycle_wrapper {
    ($name:ident, $operation:ident) => {
        pub struct $name<'a, R: ?Sized, T: ?Sized> {
            inner: LifecycleUseCase<'a, R, T>,
        }

        impl<'a, R: ProfileReader + Sync + ?Sized, T: LifecycleRuntime + ?Sized> $name<'a, R, T> {
            pub fn new(reader: &'a R, runtime: &'a T) -> Self {
                Self {
                    inner: LifecycleUseCase::new(reader, runtime, LifecycleOperation::$operation),
                }
            }

            pub fn execute(&self, id: ProfileId) -> LifecycleFuture<'_, LifecycleResult> {
                Box::pin(self.inner.execute(id))
            }
        }
    };
}

lifecycle_wrapper!(ApplyProject, Apply);
lifecycle_wrapper!(StopProject, Stop);
lifecycle_wrapper!(TearDownProject, TearDown);
lifecycle_wrapper!(RestartProject, Restart);

fn lifecycle_name(operation: LifecycleOperation) -> &'static str {
    match operation {
        LifecycleOperation::Apply => "apply_project",
        LifecycleOperation::Stop => "stop_project",
        LifecycleOperation::TearDown => "tear_down_project",
        LifecycleOperation::Restart => "restart_project",
    }
}
