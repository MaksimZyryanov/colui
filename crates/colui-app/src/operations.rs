use colui_domain::{AppError, ProfileId};
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;

pub use crate::lifecycle::LifecycleOperation as OperationKind;

pub type OperationFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, AppError>> + Send + 'a>>;

type ReleaseAction = Box<dyn FnOnce() + Send + 'static>;

struct ReleaseOnce(Mutex<Option<ReleaseAction>>);

impl ReleaseOnce {
    fn new(release: impl FnOnce() + Send + 'static) -> Self {
        Self(Mutex::new(Some(Box::new(release))))
    }
}

impl Drop for ReleaseOnce {
    fn drop(&mut self) {
        let release = match self.0.lock() {
            Ok(mut action) => action.take(),
            Err(poisoned) => poisoned.into_inner().take(),
        };
        if let Some(release) = release {
            release();
        }
    }
}

/// RAII guard for lifecycle operations.
/// Releases the operation lock exactly once when dropped.
pub struct LifecycleOperationGuard {
    _release: ReleaseOnce,
}

impl LifecycleOperationGuard {
    pub fn new(release: impl FnOnce() + Send + 'static) -> Self {
        Self {
            _release: ReleaseOnce::new(release),
        }
    }
}

impl fmt::Debug for LifecycleOperationGuard {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("LifecycleOperationGuard(..)")
    }
}

/// RAII guard for definition loading operations.
/// Releases the definition lock exactly once when dropped.
pub struct DefinitionLoadGuard {
    _release: ReleaseOnce,
}

macro_rules! operation_guard {
    ($name:ident) => {
        pub struct $name {
            _release: ReleaseOnce,
        }

        impl $name {
            pub fn new(release: impl FnOnce() + Send + 'static) -> Self {
                Self {
                    _release: ReleaseOnce::new(release),
                }
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(concat!(stringify!($name), "(..)"))
            }
        }
    };
}

operation_guard!(ContainerOperationGuard);
operation_guard!(RegistryMutationGuard);
operation_guard!(RegistryRecoveryGuard);

impl DefinitionLoadGuard {
    pub fn new(release: impl FnOnce() + Send + 'static) -> Self {
        Self {
            _release: ReleaseOnce::new(release),
        }
    }
}

impl fmt::Debug for DefinitionLoadGuard {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DefinitionLoadGuard(..)")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DefinitionBusy {
    LifecyclePending,
    DefinitionActive,
}

pub trait OperationLockReader: Send + Sync {
    fn is_busy(&self, profile_id: &ProfileId) -> bool;
}

/// Manages exclusive locks for profile lifecycle operations and definition loads.
pub trait OperationLockManager: OperationLockReader {
    fn acquire_lifecycle(
        &self,
        profile_id: ProfileId,
        operation: OperationKind,
    ) -> OperationFuture<'_, LifecycleOperationGuard>;

    fn acquire_definition(
        &self,
        profile_id: ProfileId,
    ) -> Result<DefinitionLoadGuard, DefinitionBusy>;

    fn acquire_container(&self, _container_id: &str) -> Result<ContainerOperationGuard, AppError> {
        Err(unsupported_lease("acquire_container"))
    }
    fn acquire_mutation(&self) -> Result<RegistryMutationGuard, AppError> {
        Err(unsupported_lease("acquire_mutation"))
    }
    fn acquire_recovery(&self) -> Result<RegistryRecoveryGuard, AppError> {
        Err(unsupported_lease("acquire_recovery"))
    }
}

fn unsupported_lease(operation: &str) -> AppError {
    AppError::new(
        colui_domain::AppErrorCode::OperationConflict,
        operation,
        None,
        "operation lock manager does not support this lease",
    )
}
