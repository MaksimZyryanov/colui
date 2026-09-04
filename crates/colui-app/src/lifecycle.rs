use crate::{InventoryRefresher, OperationLockManager, ProfileReader};
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

impl LifecycleResult {
    pub fn inventory_generation(&self) -> u64 {
        self.inventory.generation
    }
}

pub trait LifecycleRuntime: Send + Sync {
    fn run_profile(
        &self,
        profile: ProjectProfile,
        operation: LifecycleOperation,
    ) -> LifecycleFuture<'_, LifecycleResult>;
}

pub async fn run_lifecycle<R, L, I>(
    runtime: &R,
    locks: &L,
    inventory: &I,
    profile: ProjectProfile,
    operation: LifecycleOperation,
) -> Result<LifecycleResult, AppError>
where
    R: LifecycleRuntime + ?Sized,
    L: OperationLockManager + ?Sized,
    I: InventoryRefresher + ?Sized,
{
    let _guard = locks
        .acquire_lifecycle(profile.id.clone(), operation)
        .await?;
    let result = runtime.run_profile(profile, operation).await?;
    let inventory = if result.success {
        match inventory.refresh().await {
            Ok(inventory) => inventory,
            Err(error) => {
                let mut inventory = inventory.current_inventory().await?;
                inventory.freshness = if inventory.has_snapshot {
                    colui_domain::InventoryFreshness::Stale
                } else {
                    colui_domain::InventoryFreshness::Unavailable
                };
                inventory.error = Some(error);
                inventory
            }
        }
    } else {
        inventory.current_inventory().await?
    };
    Ok(LifecycleResult {
        inventory,
        ..result
    })
}

struct LifecycleUseCase<'a, R: ?Sized, T: ?Sized, L: ?Sized, I: ?Sized> {
    reader: &'a R,
    runtime: &'a T,
    locks: Option<&'a L>,
    inventory: Option<&'a I>,
    operation: LifecycleOperation,
}

impl<'a, R, T, L, I> LifecycleUseCase<'a, R, T, L, I>
where
    R: ProfileReader + ?Sized,
    T: LifecycleRuntime + ?Sized,
    L: ?Sized,
    I: ?Sized,
{
    fn new(
        reader: &'a R,
        runtime: &'a T,
        locks: &'a L,
        inventory: &'a I,
        operation: LifecycleOperation,
    ) -> Self {
        Self {
            reader,
            runtime,
            locks: Some(locks),
            inventory: Some(inventory),
            operation,
        }
    }

    async fn execute(&self, id: ProfileId) -> Result<LifecycleResult, AppError>
    where
        L: OperationLockManager,
        I: InventoryRefresher,
    {
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
        match (self.locks, self.inventory) {
            (Some(locks), Some(inventory)) => {
                run_lifecycle(self.runtime, locks, inventory, profile, self.operation).await
            }
            _ => self.runtime.run_profile(profile, self.operation).await,
        }
    }

    async fn execute_plain(&self, id: ProfileId) -> Result<LifecycleResult, AppError> {
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
        pub struct $name<'a, R: ?Sized, T: ?Sized, L: ?Sized, I: ?Sized> {
            inner: LifecycleUseCase<'a, R, T, L, I>,
        }

        impl<'a, R, T> $name<'a, R, T, (), ()>
        where
            R: ProfileReader + Sync + ?Sized,
            T: LifecycleRuntime + ?Sized,
        {
            pub fn new(reader: &'a R, runtime: &'a T) -> Self {
                Self {
                    inner: LifecycleUseCase {
                        reader,
                        runtime,
                        locks: None,
                        inventory: None,
                        operation: LifecycleOperation::$operation,
                    },
                }
            }

            pub fn execute(&self, id: ProfileId) -> LifecycleFuture<'_, LifecycleResult> {
                Box::pin(self.inner.execute_plain(id))
            }
        }

        impl<'a, R, T, L, I> $name<'a, R, T, L, I>
        where
            R: ProfileReader + Sync + ?Sized,
            T: LifecycleRuntime + ?Sized,
            L: OperationLockManager + ?Sized,
            I: InventoryRefresher + ?Sized,
        {
            pub fn new_with_dependencies(
                reader: &'a R,
                runtime: &'a T,
                locks: &'a L,
                inventory: &'a I,
            ) -> Self {
                Self {
                    inner: LifecycleUseCase::new(
                        reader,
                        runtime,
                        locks,
                        inventory,
                        LifecycleOperation::$operation,
                    ),
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
