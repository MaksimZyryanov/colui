mod definition;
mod discovery;
mod error;
mod identity;
mod profile;
mod runtime;

pub use definition::{
    ComposeContainerMetadata, ComposeObservationGroup, ContainerId, ContainerInstance,
    ContainerObservation, ContainerState, DefinitionRevision, DefinitionState, InventoryFreshness,
    Issue, IssueCode, PortBinding, ProjectDefinition, ProjectRuntimeSnapshot, RuntimeActivity,
    RuntimeInventory, RuntimePresence, ServiceDefinition, Timestamp,
};
pub use discovery::{
    classify_candidates, CandidateId, DiscoveryCandidate, DiscoveryClassification,
    DiscoveryConflictEvidence, DiscoveryConflictSource,
};
pub use error::{AppError, AppErrorCode, AppErrorSubject, AppErrorSubjectKind};
pub use identity::{ComposeProjectName, DisplayName, ProfileId, Revision};
pub use profile::{validate_draft, ProfileDraft, ProjectProfile, RegistrationOrigin};
pub use runtime::{
    ContainerDetails, DaemonFingerprint, DockerEndpoint, MismatchDetails, RuntimeSessionId,
    RuntimeSessionState, SessionContext,
};
