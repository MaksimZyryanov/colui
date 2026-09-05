use crate::InventoryReader;
use crate::{InventoryRefresher, OperationLockManager, RuntimeFuture, RuntimeStateReader};
use colui_domain::{
    AppError, AppErrorCode, AppErrorSubject, AppErrorSubjectKind, ContainerId, InventoryFreshness,
    RuntimeInventory, RuntimeSessionId,
};
use colui_domain::{ContainerInstance, PortBinding, Timestamp};
use std::{collections::VecDeque, net::IpAddr};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContainerLogsRequest {
    pub container_id: ContainerId,
    pub runtime_session_id: RuntimeSessionId,
}

#[derive(Clone, Eq, PartialEq)]
pub struct ContainerLogs {
    pub container_id: ContainerId,
    pub text: String,
    pub retained_bytes: u32,
    pub truncated: bool,
    pub observed_at: Timestamp,
}

impl std::fmt::Debug for ContainerLogs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContainerLogs")
            .field("container_id", &self.container_id)
            .field("retained_bytes", &self.retained_bytes)
            .field("truncated", &self.truncated)
            .finish_non_exhaustive()
    }
}

/// Request-local payload retention. Never stores a complete Docker response.
#[derive(Default)]
pub struct LogByteRing {
    bytes: VecDeque<u8>,
    truncated: bool,
}

impl LogByteRing {
    pub const CAPACITY: usize = 262_144;

    pub fn push(&mut self, bytes: &[u8]) {
        let discarded = self
            .bytes
            .len()
            .saturating_add(bytes.len())
            .saturating_sub(Self::CAPACITY);
        self.truncated |= discarded != 0;
        self.bytes.drain(..discarded.min(self.bytes.len()));
        self.bytes
            .extend(&bytes[bytes.len().saturating_sub(Self::CAPACITY)..]);
    }

    pub fn finish(mut self, container_id: ContainerId, observed_at: Timestamp) -> ContainerLogs {
        ContainerLogs {
            container_id,
            retained_bytes: self.bytes.len() as u32,
            truncated: self.truncated,
            text: String::from_utf8_lossy(self.bytes.make_contiguous()).into_owned(),
            observed_at,
        }
    }
}

pub trait ContainerLogsRuntime: RuntimeStateReader {
    fn read_logs(&self, request: ContainerLogsRequest) -> RuntimeFuture<'_, ContainerLogs>;
}

pub fn container_logs_error(id: &ContainerId, retryable: bool) -> AppError {
    let mut error = container_operation_error(id, "container logs unavailable");
    error.operation = "container_logs".into();
    error.retryable = retryable;
    error
}

async fn check_read_session<R: RuntimeStateReader + ?Sized>(
    runtime: &R,
    id: &ContainerId,
    expected: RuntimeSessionId,
) -> Result<(), AppError> {
    match runtime.api_read_context().await {
        Ok(context) if context.session_id == expected => Ok(()),
        _ => Err(container_operation_error(
            id,
            "runtime session changed or unavailable",
        )),
    }
}

async fn read_standalone<R: RuntimeStateReader + ?Sized, I: InventoryReader + ?Sized>(
    runtime: &R,
    inventory: &I,
    id: &ContainerId,
    expected: RuntimeSessionId,
) -> Result<ContainerInstance, AppError> {
    check_read_session(runtime, id, expected).await?;
    let current = inventory
        .current_inventory()
        .await
        .map_err(|_| container_operation_error(id, "container inventory unavailable"))?;
    if !current.has_snapshot
        || current.freshness != InventoryFreshness::Fresh
        || current.runtime_session_id != Some(expected)
    {
        return Err(container_operation_error(
            id,
            "fresh standalone container required",
        ));
    }
    let container = current
        .standalone_containers
        .into_iter()
        .find(|c| &c.id == id)
        .ok_or_else(|| container_operation_error(id, "standalone container unavailable"))?;
    check_read_session(runtime, id, expected).await?;
    Ok(container)
}

pub struct ReadContainerLogs<'a, R: ?Sized, I: ?Sized> {
    runtime: &'a R,
    inventory: &'a I,
}
impl<'a, R: ContainerLogsRuntime + ?Sized, I: InventoryReader + ?Sized>
    ReadContainerLogs<'a, R, I>
{
    pub fn new(runtime: &'a R, inventory: &'a I) -> Self {
        Self { runtime, inventory }
    }
    pub async fn execute(&self, request: ContainerLogsRequest) -> Result<ContainerLogs, AppError> {
        read_standalone(
            self.runtime,
            self.inventory,
            &request.container_id,
            request.runtime_session_id,
        )
        .await?;
        let result = self.runtime.read_logs(request.clone()).await;
        check_read_session(
            self.runtime,
            &request.container_id,
            request.runtime_session_id,
        )
        .await?;
        result.map_err(|e| container_logs_error(&request.container_id, e.retryable))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortBindingAction {
    pub copy: String,
    pub url: Option<String>,
}
impl From<&PortBinding> for PortBindingAction {
    fn from(binding: &PortBinding) -> Self {
        Self {
            copy: binding_display(binding),
            url: binding_url(binding),
        }
    }
}

pub fn binding_display(binding: &PortBinding) -> String {
    let host = binding.host_ip.as_deref().map(|ip| {
        if ip.contains(':') {
            format!("[{ip}]")
        } else {
            ip.to_owned()
        }
    });
    let address = match (host, binding.host_port) {
        (None, None) => "<unpublished>".into(),
        (host, port) => format!(
            "{}:{}",
            host.unwrap_or_else(|| "?".into()),
            port.map(|p| p.to_string()).unwrap_or_else(|| "?".into())
        ),
    };
    format!(
        "{address} -> {}/{}",
        binding.container_port,
        binding.protocol.to_ascii_lowercase()
    )
}

pub fn binding_url(binding: &PortBinding) -> Option<String> {
    if !binding.protocol.eq_ignore_ascii_case("tcp") {
        return None;
    }
    let port = binding.host_port.filter(|port| *port != 0)?;
    let recognized = |p| matches!(p, 80 | 443 | 3000 | 5173 | 8000 | 8080 | 8443);
    let service = if recognized(binding.container_port) {
        binding.container_port
    } else if recognized(port) {
        port
    } else {
        return None;
    };
    let host = match binding.host_ip.as_deref()?.parse::<IpAddr>().ok()? {
        IpAddr::V4(ip) if ip.is_unspecified() => "127.0.0.1".into(),
        IpAddr::V6(ip) if ip.is_unspecified() => "[::1]".into(),
        IpAddr::V4(ip) => ip.to_string(),
        IpAddr::V6(ip) => format!("[{ip}]"),
    };
    let scheme = if matches!(service, 443 | 8443) {
        "https"
    } else {
        "http"
    };
    Some(format!("{scheme}://{host}:{port}"))
}

/// Narrow shell capability. Commands never accept its URL parameter from IPC.
pub trait BrowserOpener {
    fn open(&self, url: &str) -> Result<(), AppError>;
}
pub struct OpenContainerPort<'a, R: ?Sized, I: ?Sized, O: ?Sized> {
    runtime: &'a R,
    inventory: &'a I,
    opener: &'a O,
}
impl<
        'a,
        R: RuntimeStateReader + ?Sized,
        I: InventoryReader + ?Sized,
        O: BrowserOpener + ?Sized,
    > OpenContainerPort<'a, R, I, O>
{
    pub fn new(runtime: &'a R, inventory: &'a I, opener: &'a O) -> Self {
        Self {
            runtime,
            inventory,
            opener,
        }
    }
    pub async fn execute(
        &self,
        id: ContainerId,
        expected: RuntimeSessionId,
        index: usize,
    ) -> Result<(), AppError> {
        let container = read_standalone(self.runtime, self.inventory, &id, expected).await?;
        let url = container
            .published_ports
            .get(index)
            .and_then(binding_url)
            .ok_or_else(|| container_operation_error(&id, "binding cannot be opened"))?;
        self.opener
            .open(&url)
            .map_err(|_| container_operation_error(&id, "browser open failed"))
    }
}

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
