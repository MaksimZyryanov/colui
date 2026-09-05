use crate::{RegistryHealth, SessionJournal};
use colui_domain::{
    AppError, DaemonFingerprint, DockerEndpoint, ProfileId, ProjectDefinition, RuntimeInventory,
    RuntimeSessionId, RuntimeSessionState, Timestamp,
};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::Duration;

pub type DiagnosticsFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, AppError>> + Send + 'a>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeDiagnostics {
    pub state: RuntimeSessionState,
    pub resolved_endpoint: Option<DockerEndpoint>,
    pub api_fingerprint: Option<DaemonFingerprint>,
    pub cli_fingerprint: Option<DaemonFingerprint>,
    pub session_id: Option<RuntimeSessionId>,
    pub connected_at: Option<Timestamp>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistryDiagnostics {
    pub registry_path: PathBuf,
    pub backup_path: PathBuf,
    pub backup: RegistryBackupDiagnostics,
    pub revision: Option<u64>,
    pub health: RegistryHealth,
    pub lock_timeout: Duration,
    pub last_recovery_result: Option<Result<(), AppError>>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BackupValidationState {
    #[default]
    Missing,
    Valid,
    Corrupt,
    Unreadable,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RegistryBackupDiagnostics {
    pub exists: bool,
    pub modified_at: Option<Timestamp>,
    pub state: BackupValidationState,
    pub error: Option<AppError>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportDiagnostics {
    pub source_path: PathBuf,
    pub imported_count: usize,
    pub source_preserved: bool,
    pub error: Option<AppError>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationPhase {
    Pending,
    Active,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActiveOperation {
    pub kind: String,
    pub subject_id: String,
    pub started_at: Timestamp,
    pub phase: OperationPhase,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationsDiagnostics {
    pub generation: u64,
    pub active: Vec<ActiveOperation>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileDefinitionDiagnostics {
    pub profile_id: ProfileId,
    pub definition: Option<ProjectDefinition>,
    pub error: Option<AppError>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DefinitionsDiagnostics {
    pub generation: u64,
    pub profiles: Vec<ProfileDefinitionDiagnostics>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticsSections {
    pub runtime: RuntimeDiagnostics,
    pub registry: RegistryDiagnostics,
    pub import: ImportDiagnostics,
    pub operations: OperationsDiagnostics,
    pub inventory: RuntimeInventory,
    pub definitions: DefinitionsDiagnostics,
    pub journal: SessionJournal,
}

pub type DiagnosticsSnapshot = DiagnosticsSections;

pub trait DiagnosticsSectionsReader: Send + Sync {
    fn read_sections(&self) -> DiagnosticsFuture<'_, DiagnosticsSections>;
}

pub trait RuntimeDiagnosticsReader: Send + Sync {
    fn runtime_diagnostics(&self) -> DiagnosticsFuture<'_, RuntimeDiagnostics>;
}

pub trait RegistryDiagnosticsReader: Send + Sync {
    fn registry_diagnostics(&self) -> DiagnosticsFuture<'_, RegistryDiagnostics>;
}

pub trait ImportDiagnosticsReader: Send + Sync {
    fn import_diagnostics(&self) -> DiagnosticsFuture<'_, ImportDiagnostics>;
}

pub trait DefinitionDiagnosticsReader: Send + Sync {
    fn definition_diagnostics(&self) -> DiagnosticsFuture<'_, DefinitionsDiagnostics>;
}

pub trait JournalReader: Send + Sync {
    fn journal(&self) -> DiagnosticsFuture<'_, SessionJournal>;
}

pub trait DiagnosticsReader: Send + Sync {
    fn read(&self) -> DiagnosticsFuture<'_, DiagnosticsSnapshot>;
}

pub struct DiagnosticsAssembler<'a> {
    runtime: &'a dyn RuntimeDiagnosticsReader,
    registry: &'a dyn RegistryDiagnosticsReader,
    import: &'a dyn ImportDiagnosticsReader,
    operations: &'a dyn crate::OperationProjectionReader,
    inventory: &'a dyn crate::InventoryReader,
    definitions: &'a dyn DefinitionDiagnosticsReader,
    journal: &'a dyn JournalReader,
}

impl<'a> DiagnosticsAssembler<'a> {
    pub fn new(
        runtime: &'a dyn RuntimeDiagnosticsReader,
        registry: &'a dyn RegistryDiagnosticsReader,
        import: &'a dyn ImportDiagnosticsReader,
        operations: &'a dyn crate::OperationProjectionReader,
        inventory: &'a dyn crate::InventoryReader,
        definitions: &'a dyn DefinitionDiagnosticsReader,
        journal: &'a dyn JournalReader,
    ) -> Self {
        Self {
            runtime,
            registry,
            import,
            operations,
            inventory,
            definitions,
            journal,
        }
    }
}

impl DiagnosticsReader for DiagnosticsAssembler<'_> {
    fn read(&self) -> DiagnosticsFuture<'_, DiagnosticsSnapshot> {
        Box::pin(async move {
            Ok(DiagnosticsSnapshot {
                runtime: self.runtime.runtime_diagnostics().await?,
                registry: self.registry.registry_diagnostics().await?,
                import: self.import.import_diagnostics().await?,
                operations: self.operations.operation_projection().await?,
                inventory: self.inventory.current_inventory().await?,
                definitions: self.definitions.definition_diagnostics().await?,
                journal: self.journal.journal().await?,
            })
        })
    }
}

pub struct DiagnosticsQuery<'a, R: ?Sized> {
    sections: &'a R,
}

impl<'a, R: DiagnosticsSectionsReader + ?Sized> DiagnosticsQuery<'a, R> {
    pub fn new(sections: &'a R) -> Self {
        Self { sections }
    }
}

impl<R: DiagnosticsSectionsReader + ?Sized> DiagnosticsReader for DiagnosticsQuery<'_, R> {
    fn read(&self) -> DiagnosticsFuture<'_, DiagnosticsSnapshot> {
        self.sections.read_sections()
    }
}
