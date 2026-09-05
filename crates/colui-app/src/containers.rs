use crate::{InventoryRefresher, OperationLockManager, RuntimeFuture, RuntimeStateReader};
use colui_domain::{
    AppError, AppErrorCode, AppErrorSubject, AppErrorSubjectKind, ContainerId, InventoryFreshness,
    RuntimeInventory, RuntimeSessionId,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContainerAction {
    Start,
    Stop,
    Restart,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContainerActionObservation {
    ConfirmedInSession,
    IndeterminateAfterSessionChange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContainerActionResult {
    pub container_id: ContainerId,
    pub action: ContainerAction,
    pub observation: ContainerActionObservation,
    pub inventory: RuntimeInventory,
}

/// The gateway checks the expected session when capturing its API client.
/// Daemon success must not be rejected by a subsequent session change.
pub trait ContainerRuntime: RuntimeStateReader {
    fn run_container(
        &self,
        container_id: ContainerId,
        action: ContainerAction,
        expected_session: RuntimeSessionId,
    ) -> RuntimeFuture<'_, ()>;
}

pub fn container_operation_error(container_id: &ContainerId, message: &str) -> AppError {
    AppError::for_subject(
        AppErrorCode::ContainerOperationFailed,
        "container_action",
        AppErrorSubject {
            kind: AppErrorSubjectKind::Container,
            id: container_id.0.clone(),
        },
        message,
    )
}

pub struct RunContainerAction<'a, R: ?Sized, I: ?Sized, L: ?Sized> {
    runtime: &'a R,
    inventory: &'a I,
    locks: &'a L,
}

impl<'a, R, I, L> RunContainerAction<'a, R, I, L>
where
    R: ContainerRuntime + ?Sized,
    I: InventoryRefresher + ?Sized,
    L: OperationLockManager + ?Sized,
{
    pub fn new(runtime: &'a R, inventory: &'a I, locks: &'a L) -> Self {
        Self {
            runtime,
            inventory,
            locks,
        }
    }

    pub async fn execute(
        &self,
        container_id: ContainerId,
        action: ContainerAction,
        expected_session: RuntimeSessionId,
    ) -> Result<ContainerActionResult, AppError> {
        self.check_session(&container_id, expected_session).await?;
        let _lease = self.locks.acquire_container(&container_id.0)?;
        let current = self.inventory.current_inventory().await.map_err(|_| {
            container_operation_error(&container_id, "container inventory unavailable")
        })?;
        if !current.has_snapshot
            || current.freshness != InventoryFreshness::Fresh
            || current.runtime_session_id != Some(expected_session)
            || !current
                .standalone_containers
                .iter()
                .any(|value| value.id == container_id)
        {
            return Err(container_operation_error(
                &container_id,
                "fresh standalone container required",
            ));
        }
        self.check_session(&container_id, expected_session).await?;
        self.runtime
            .run_container(container_id.clone(), action, expected_session)
            .await
            .map_err(|_| container_operation_error(&container_id, "container operation failed"))?;
        let marker = self.inventory.observation_marker();
        let inventory = match self.inventory.refresh_after(marker).await {
            Ok(inventory) => inventory,
            Err(_) => self.inventory.current_inventory().await?,
        };
        let observation = if self
            .check_session(&container_id, expected_session)
            .await
            .is_ok()
        {
            ContainerActionObservation::ConfirmedInSession
        } else {
            ContainerActionObservation::IndeterminateAfterSessionChange
        };
        Ok(ContainerActionResult {
            container_id,
            action,
            observation,
            inventory,
        })
    }

    async fn check_session(
        &self,
        id: &ContainerId,
        expected: RuntimeSessionId,
    ) -> Result<(), AppError> {
        match self.runtime.api_read_context().await {
            Ok(context) if context.session_id == expected => Ok(()),
            _ => Err(container_operation_error(
                id,
                "runtime session changed or unavailable",
            )),
        }
    }
}
