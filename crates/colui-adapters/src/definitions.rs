use crate::runtime::{build_cli_environment, ComposeExecutionGate};
use colui_app::{
    Clock, ComposeInvocation, ComposeRunner, DefinitionBusy, DefinitionFuture,
    DefinitionProjection, DefinitionReader, DefinitionRefresher, RuntimeStateReader,
};
use colui_domain::{
    AppError, AppErrorCode, DefinitionRevision, DefinitionState, Issue, ProjectDefinition,
    ProjectProfile, ServiceDefinition, Timestamp,
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const TTL: Duration = Duration::from_secs(60);

#[derive(Clone)]
struct CachedDefinition {
    profile_revision: colui_domain::Revision,
    definition_revision: DefinitionRevision,
    loaded_at: Timestamp,
    loaded_mono: Duration,
    state: DefinitionState,
    services: Vec<ServiceDefinition>,
    issues: Vec<Issue>,
    load_error: Option<AppError>,
}

struct State {
    entries: HashMap<colui_domain::ProfileId, CachedDefinition>,
    epoch: HashMap<colui_domain::ProfileId, u64>,
    revisions: HashMap<colui_domain::ProfileId, colui_domain::Revision>,
}

pub struct DefinitionCache {
    runner: Arc<dyn ComposeRunner>,
    runtime: Arc<dyn RuntimeStateReader>,
    clock: Arc<dyn Clock>,
    locks: Arc<dyn colui_app::OperationLockManager>,
    compose_gate: Arc<ComposeExecutionGate>,
    state: Arc<Mutex<State>>,
}

impl DefinitionCache {
    pub fn new(
        runner: Arc<dyn ComposeRunner>,
        runtime: Arc<dyn RuntimeStateReader>,
        clock: Arc<dyn Clock>,
        locks: Arc<dyn colui_app::OperationLockManager>,
        compose_gate: Arc<ComposeExecutionGate>,
    ) -> Self {
        Self {
            runner,
            runtime,
            clock,
            locks,
            compose_gate,
            state: Arc::new(Mutex::new(State {
                entries: HashMap::new(),
                epoch: HashMap::new(),
                revisions: HashMap::new(),
            })),
        }
    }

    async fn access(
        &self,
        profile: ProjectProfile,
        force: bool,
    ) -> Result<DefinitionProjection, AppError> {
        let id = profile.id.clone();
        let now = self.clock.monotonic();
        let cached = {
            let mut state = lock(&self.state);
            let previous_revision = state.revisions.insert(id.clone(), profile.revision);
            if previous_revision.is_some_and(|revision| revision != profile.revision) {
                state.entries.remove(&id);
            }
            state.entries.get(&id).cloned()
        };
        if !force
            && cached.as_ref().is_some_and(|e| {
                e.profile_revision == profile.revision && now.saturating_sub(e.loaded_mono) < TTL
            })
        {
            return Ok(to_projection(&id, cached.unwrap()));
        }
        let guard = match self.locks.acquire_definition(id.clone()) {
            Ok(guard) => guard,
            Err(busy @ (DefinitionBusy::LifecyclePending | DefinitionBusy::DefinitionActive)) => {
                return Ok(retained(
                    &id,
                    cached,
                    Some(definition_busy_error(id.clone(), busy)),
                ))
            }
        };
        let epoch = lock(&self.state).epoch.get(&id).copied().unwrap_or(0);
        let result = self.load(&profile).await;
        let completed_mono = self.clock.monotonic();
        let mut state = lock(&self.state);
        let current_epoch = state.epoch.get(&id).copied().unwrap_or(0);
        if current_epoch == epoch && state.revisions.get(&id).copied() == Some(profile.revision) {
            match result {
                Ok((revision, services, issues)) => {
                    let entry = CachedDefinition {
                        profile_revision: profile.revision,
                        definition_revision: revision,
                        loaded_at: self.clock.now(),
                        loaded_mono: completed_mono,
                        state: if issues.is_empty() {
                            DefinitionState::Valid
                        } else {
                            DefinitionState::Invalid
                        },
                        services,
                        issues,
                        load_error: None,
                    };
                    let output = to_projection(&id, entry.clone());
                    state.entries.insert(id, entry);
                    drop(guard);
                    Ok(output)
                }
                Err(error) => {
                    if let Some(entry) = state.entries.get_mut(&id) {
                        entry.state = DefinitionState::Stale;
                        entry.load_error = Some(error.clone());
                    }
                    let output = retained(&id, state.entries.get(&id).cloned(), Some(error));
                    drop(guard);
                    Ok(output)
                }
            }
        } else {
            drop(guard);
            Ok(state
                .entries
                .get(&id)
                .cloned()
                .map(|entry| to_projection(&id, entry))
                .unwrap_or_else(|| retained(&id, None, None)))
        }
    }

    async fn load(
        &self,
        profile: &ProjectProfile,
    ) -> Result<(DefinitionRevision, Vec<ServiceDefinition>, Vec<Issue>), AppError> {
        let endpoint = match self.runtime.session_state().await? {
            colui_domain::RuntimeSessionState::Ready(context) => context.endpoint,
            _ => {
                return Err(AppError::new(
                    AppErrorCode::RuntimeUnavailable,
                    "definition",
                    Some(profile.id.clone()),
                    "runtime unavailable",
                ))
            }
        };
        let mut args = vec!["compose".into()];
        for path in &profile.compose_files {
            args.extend([
                "-f".into(),
                if path.is_absolute() {
                    path.to_string_lossy().into_owned()
                } else {
                    profile
                        .working_directory
                        .join(path)
                        .to_string_lossy()
                        .into_owned()
                },
            ]);
        }
        for path in &profile.environment_files {
            args.extend([
                "--env-file".into(),
                profile
                    .working_directory
                    .join(path)
                    .to_string_lossy()
                    .into_owned(),
            ]);
        }
        args.extend([
            "--project-name".into(),
            profile.compose_project_name.as_ref().to_owned(),
            "config".into(),
            "--format".into(),
            "json".into(),
        ]);
        let _permit = self.compose_gate.acquire().await?;
        let output = self
            .runner
            .invoke(ComposeInvocation {
                executable: "docker".into(),
                args,
                working_directory: profile.working_directory.clone(),
                environment: build_cli_environment(endpoint.as_str(), std::env::vars().collect()),
                deadline: Instant::now() + Duration::from_secs(120),
            })
            .await?;
        if output.timed_out() || output.exit_code() != Some(0) {
            return Err(compose_config_error(profile.id.clone(), output.stderr()));
        }
        let value: serde_json::Value = match serde_json::from_str(output.stdout()) {
            Ok(value) => value,
            Err(error) => {
                return Ok((
                    DefinitionRevision(String::new()),
                    Vec::new(),
                    vec![Issue {
                        field: None,
                        message: error.to_string(),
                    }],
                ))
            }
        };
        let canonical = canonical(value.clone());
        let digest = Sha256::digest(serde_json::to_vec(&canonical).unwrap());
        let revision =
            DefinitionRevision(digest.iter().map(|byte| format!("{byte:02x}")).collect());
        let mut services = Vec::new();
        if let Some(map) = value.get("services").and_then(serde_json::Value::as_object) {
            for (name, service) in map {
                services.push(ServiceDefinition {
                    name: name.clone(),
                    image: service
                        .get("image")
                        .and_then(|v| v.as_str())
                        .map(str::to_owned),
                    build_context: service
                        .get("build")
                        .and_then(|v| v.as_str())
                        .map(str::to_owned)
                        .or_else(|| {
                            service
                                .get("build")
                                .and_then(|v| v.get("context"))
                                .and_then(|v| v.as_str())
                                .map(str::to_owned)
                        }),
                    declared_ports: service
                        .get("ports")
                        .and_then(|v| v.as_array())
                        .map(|ports| {
                            ports
                                .iter()
                                .map(|v| {
                                    v.as_str()
                                        .map(str::to_owned)
                                        .unwrap_or_else(|| v.to_string())
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                });
            }
        }
        Ok((revision, services, Vec::new()))
    }
}

fn compose_config_error(profile_id: colui_domain::ProfileId, _stderr: &str) -> AppError {
    AppError::new(
        AppErrorCode::DefinitionFailed,
        "definition",
        Some(profile_id),
        "compose config failed",
    )
}

fn definition_busy_error(profile_id: colui_domain::ProfileId, busy: DefinitionBusy) -> AppError {
    let message = match busy {
        DefinitionBusy::LifecyclePending => "lifecycle operation has priority",
        DefinitionBusy::DefinitionActive => "definition load already active",
    };
    AppError::new(
        AppErrorCode::OperationConflict,
        "definition",
        Some(profile_id),
        message,
    )
}

fn canonical(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.into_iter()
                .map(|(key, value)| (key, canonical(value)))
                .collect(),
        ),
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.into_iter().map(canonical).collect())
        }
        value => value,
    }
}
fn to_definition(id: &colui_domain::ProfileId, entry: CachedDefinition) -> ProjectDefinition {
    ProjectDefinition {
        profile_id: id.clone(),
        definition_revision: entry.definition_revision,
        loaded_at: entry.loaded_at,
        state: entry.state,
        services: entry.services,
        issues: entry.issues,
    }
}
fn to_projection(id: &colui_domain::ProfileId, entry: CachedDefinition) -> DefinitionProjection {
    let error = entry.load_error.clone();
    DefinitionProjection {
        definition: to_definition(id, entry),
        error,
    }
}
fn retained(
    id: &colui_domain::ProfileId,
    entry: Option<CachedDefinition>,
    error: Option<AppError>,
) -> DefinitionProjection {
    match entry {
        None => DefinitionProjection {
            definition: ProjectDefinition {
                profile_id: id.clone(),
                definition_revision: DefinitionRevision(String::new()),
                loaded_at: Timestamp(String::new()),
                state: DefinitionState::Unchecked,
                services: Vec::new(),
                issues: Vec::new(),
            },
            error,
        },
        Some(mut entry) => {
            entry.state = DefinitionState::Stale;
            let retained_error = error.or_else(|| entry.load_error.clone());
            DefinitionProjection {
                definition: to_definition(id, entry),
                error: retained_error,
            }
        }
    }
}

impl DefinitionReader for DefinitionCache {
    fn definition(&self, profile: ProjectProfile) -> DefinitionFuture<'_, DefinitionProjection> {
        Box::pin(self.access(profile, false))
    }
}
impl DefinitionRefresher for DefinitionCache {
    fn refresh_definition(
        &self,
        profile: ProjectProfile,
    ) -> DefinitionFuture<'_, DefinitionProjection> {
        Box::pin(self.access(profile, true))
    }
    fn invalidate(&self, profile_id: colui_domain::ProfileId) {
        let mut state = lock(&self.state);
        *state.epoch.entry(profile_id.clone()).or_default() += 1;
        state.entries.remove(&profile_id);
    }
}
impl colui_app::DefinitionInvalidator for DefinitionCache {
    fn invalidate_all(&self) {
        let mut state = lock(&self.state);
        for epoch in state.epoch.values_mut() {
            *epoch += 1;
        }
        state.entries.clear();
        state.revisions.clear();
    }
}

fn lock(state: &Mutex<State>) -> std::sync::MutexGuard<'_, State> {
    state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::compose_config_error;
    use colui_domain::{AppErrorCode, ProfileId};
    use uuid::Uuid;

    #[test]
    fn compose_failure_omits_raw_stderr_from_transport_error() {
        let error = compose_config_error(
            ProfileId::new(Uuid::nil()),
            "secret=/Users/max/private.env project=top-secret",
        );
        assert_eq!(error.code, AppErrorCode::DefinitionFailed);
        assert!(error.details.is_none());
        assert!(!error.message.contains("secret"));
        assert!(!error.message.contains("/Users/max"));
    }
}
