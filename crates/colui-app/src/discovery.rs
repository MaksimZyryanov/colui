use crate::{InventoryReader, ProfileReader};
use colui_domain::{
    classify_candidates, AppError, CandidateId, DiscoveryCandidate, DiscoveryClassification,
    RuntimeInventory, RuntimeSessionId, Timestamp,
};
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

const JOURNAL_CAPACITY: usize = 256;
const MESSAGE_LIMIT: usize = 500;

pub type DiscoveryFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, AppError>> + Send + 'a>>;

pub struct CandidateLease {
    _guard: Box<dyn Send + Sync>,
}

impl CandidateLease {
    pub fn hold<T: Send + Sync + 'static>(guard: T) -> Self {
        Self {
            _guard: Box::new(guard),
        }
    }
}

pub trait DiscoveryReader: InventoryReader {
    fn lease_candidate(
        &self,
        runtime_session_id: RuntimeSessionId,
        inventory_generation: u64,
    ) -> DiscoveryFuture<'_, CandidateLease>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JournalEventKind {
    ConnectStarted,
    ConnectSucceeded,
    ConnectFailed,
    DisconnectStarted,
    DisconnectSucceeded,
    DisconnectFailed,
    ReconnectStarted,
    ReconnectSucceeded,
    ReconnectFailed,
    ManualRegistrationStarted,
    ManualRegistrationSucceeded,
    ManualRegistrationFailed,
    AutoRegistrationStarted,
    AutoRegistrationSucceeded,
    AutoRegistrationFailed,
    BackupStarted,
    BackupSucceeded,
    BackupFailed,
    RestoreStarted,
    RestoreSucceeded,
    RestoreFailed,
    ProfileLifecycleStarted,
    ProfileLifecycleSucceeded,
    ProfileLifecycleFailed,
    ContainerActionStarted,
    ContainerActionSucceeded,
    ContainerActionFailed,
    CandidateIgnored,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JournalSeverity {
    Info,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JournalEntry {
    pub sequence: u64,
    pub timestamp: Timestamp,
    pub runtime_session_id: Option<RuntimeSessionId>,
    pub kind: JournalEventKind,
    pub severity: JournalSeverity,
    pub candidate_id: Option<CandidateId>,
    pub stable_error_code: Option<String>,
    pub message: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SessionJournal {
    pub entries: Vec<JournalEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AutoRegistrationSchedule {
    pub runtime_session_id: RuntimeSessionId,
    pub inventory_generation: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AutoRegistrationOutcome {
    Succeeded,
    TerminalFailure,
    RetryableFailure,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct AutoKey {
    session: RuntimeSessionId,
    candidate: CandidateId,
    metadata_hash: String,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct ScheduleKey {
    session: RuntimeSessionId,
    generation: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum AutoState {
    Pending,
    Succeeded,
    TerminalFailure,
    RetryableFailure { next_generation: u64 },
}

#[derive(Default)]
struct State {
    active_session: Option<RuntimeSessionId>,
    generation: Option<u64>,
    candidates: Vec<DiscoveryCandidate>,
    ignored: BTreeSet<CandidateId>,
    auto_enabled: bool,
    last_scheduled_generation: Option<u64>,
    scheduled: HashMap<ScheduleKey, Vec<DiscoveryCandidate>>,
    auto: HashMap<AutoKey, AutoState>,
    journal: VecDeque<JournalEntry>,
    next_sequence: u64,
}

#[derive(Clone, Default)]
pub struct DiscoverySession {
    state: Arc<Mutex<State>>,
}

impl DiscoverySession {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn observe_inventory(
        &self,
        inventory: RuntimeInventory,
        profiles: Vec<colui_domain::ProjectProfile>,
    ) -> Vec<DiscoveryCandidate> {
        let mut state = self.state.lock().expect("discovery state poisoned");
        let Some(runtime_session_id) = inventory.runtime_session_id else {
            state.active_session = None;
            state.generation = None;
            state.candidates.clear();
            state.ignored.clear();
            state.auto.clear();
            state.last_scheduled_generation = None;
            return vec![];
        };
        synchronize_session(&mut state, runtime_session_id);
        let mut candidates = classify_candidates(
            runtime_session_id,
            inventory.generation,
            &inventory.compose_observation_groups,
            &profiles,
        );
        for candidate in &mut candidates {
            candidate.ignored = state.ignored.contains(&candidate.candidate_id);
        }
        state.generation = Some(inventory.generation);
        state.candidates = candidates.clone();
        candidates
    }

    pub async fn candidates(&self) -> Vec<DiscoveryCandidate> {
        self.state
            .lock()
            .expect("discovery state poisoned")
            .candidates
            .clone()
    }

    pub async fn journal(&self) -> SessionJournal {
        SessionJournal {
            entries: self
                .state
                .lock()
                .expect("discovery state poisoned")
                .journal
                .iter()
                .cloned()
                .collect(),
        }
    }

    pub async fn auto_registration_enabled(&self) -> bool {
        self.state
            .lock()
            .expect("discovery state poisoned")
            .auto_enabled
    }

    async fn observe_successful_publication(
        &self,
        inventory: RuntimeInventory,
        profiles: Vec<colui_domain::ProjectProfile>,
    ) -> Option<AutoRegistrationSchedule> {
        let session = inventory.runtime_session_id?;
        let mut state = self.state.lock().expect("discovery state poisoned");
        synchronize_session(&mut state, session);
        if !state.auto_enabled
            || state
                .last_scheduled_generation
                .is_some_and(|generation| generation >= inventory.generation)
        {
            return None;
        }
        if state
            .generation
            .is_some_and(|generation| generation < inventory.generation)
        {
            state.auto.retain(|_, status| *status != AutoState::Pending);
            state.scheduled.clear();
        }
        let mut candidates = classify_candidates(
            session,
            inventory.generation,
            &inventory.compose_observation_groups,
            &profiles,
        );
        for candidate in &mut candidates {
            candidate.ignored = state.ignored.contains(&candidate.candidate_id);
        }
        state.generation = Some(inventory.generation);
        state.candidates = candidates.clone();
        let scheduled: Vec<_> = candidates
            .into_iter()
            .filter(|candidate| {
                candidate.classification == DiscoveryClassification::NewUnambiguous
                    && !candidate.ignored
                    && auto_key_schedulable(&state, candidate, inventory.generation)
            })
            .collect();
        state.last_scheduled_generation = Some(inventory.generation);
        if scheduled.is_empty() {
            return None;
        }
        let schedule = AutoRegistrationSchedule {
            runtime_session_id: session,
            inventory_generation: inventory.generation,
        };
        state.scheduled.insert(schedule_key(&schedule), scheduled);
        Some(schedule)
    }

    pub async fn claim_schedule(
        &self,
        schedule: &AutoRegistrationSchedule,
    ) -> Vec<DiscoveryCandidate> {
        let mut state = self.state.lock().expect("discovery state poisoned");
        if !state.auto_enabled
            || state.active_session != Some(schedule.runtime_session_id)
            || state.generation != Some(schedule.inventory_generation)
        {
            return vec![];
        }
        let Some(candidates) = state.scheduled.remove(&schedule_key(schedule)) else {
            return vec![];
        };
        for candidate in &candidates {
            state.auto.insert(auto_key(candidate), AutoState::Pending);
        }
        candidates
    }

    pub async fn complete_auto_candidate(
        &self,
        schedule: &AutoRegistrationSchedule,
        candidate: &DiscoveryCandidate,
        outcome: AutoRegistrationOutcome,
    ) -> bool {
        let mut state = self.state.lock().expect("discovery state poisoned");
        let key = auto_key(candidate);
        if state.active_session != Some(schedule.runtime_session_id)
            || state.generation != Some(schedule.inventory_generation)
            || candidate.runtime_session_id != schedule.runtime_session_id
            || candidate.inventory_generation != schedule.inventory_generation
            || state.auto.get(&key) != Some(&AutoState::Pending)
        {
            return false;
        }
        let status = match outcome {
            AutoRegistrationOutcome::Succeeded => AutoState::Succeeded,
            AutoRegistrationOutcome::TerminalFailure => AutoState::TerminalFailure,
            AutoRegistrationOutcome::RetryableFailure => AutoState::RetryableFailure {
                next_generation: schedule.inventory_generation.saturating_add(1),
            },
        };
        state.auto.insert(key, status);
        true
    }

    pub async fn record_counted_event(
        &self,
        kind: JournalEventKind,
        runtime_session_id: Option<RuntimeSessionId>,
        count: usize,
        _excluded_source: &str,
    ) {
        let message = static_message(kind, count, None);
        append_journal(
            &mut self.state.lock().expect("discovery state poisoned"),
            kind,
            runtime_session_id,
            None,
            message,
        );
    }
}

pub struct ScheduleAutoRegistration<'a, P: ?Sized> {
    profiles: &'a P,
    session: &'a DiscoverySession,
}

impl<'a, P: ProfileReader + ?Sized> ScheduleAutoRegistration<'a, P> {
    pub fn new(profiles: &'a P, session: &'a DiscoverySession) -> Self {
        Self { profiles, session }
    }

    pub async fn execute(
        &self,
        inventory: RuntimeInventory,
    ) -> Result<Option<AutoRegistrationSchedule>, AppError> {
        let profiles = self.profiles.load().await?.profiles;
        Ok(self
            .session
            .observe_successful_publication(inventory, profiles)
            .await)
    }
}

pub struct ListDiscoveryCandidates<'a, I: ?Sized, P: ?Sized> {
    inventory: &'a I,
    profiles: &'a P,
    session: &'a DiscoverySession,
}

impl<'a, I: InventoryReader + ?Sized, P: ProfileReader + ?Sized> ListDiscoveryCandidates<'a, I, P> {
    pub fn new(inventory: &'a I, profiles: &'a P, session: &'a DiscoverySession) -> Self {
        Self {
            inventory,
            profiles,
            session,
        }
    }

    pub async fn execute(&self) -> Result<Vec<DiscoveryCandidate>, AppError> {
        let inventory = self.inventory.current_inventory().await?;
        let profiles = self.profiles.load().await?.profiles;
        Ok(self.session.observe_inventory(inventory, profiles).await)
    }
}

pub struct IgnoreCandidate<'a> {
    session: &'a DiscoverySession,
}

impl<'a> IgnoreCandidate<'a> {
    pub fn new(session: &'a DiscoverySession) -> Self {
        Self { session }
    }

    pub async fn execute(&self, candidate_id: CandidateId) -> bool {
        let mut state = self.session.state.lock().expect("discovery state poisoned");
        if !state
            .candidates
            .iter()
            .any(|candidate| candidate.candidate_id == candidate_id)
            || !state.ignored.insert(candidate_id.clone())
        {
            return false;
        }
        for candidate in &mut state.candidates {
            if candidate.candidate_id == candidate_id {
                candidate.ignored = true;
            }
        }
        let session = state.active_session;
        let message = static_message(JournalEventKind::CandidateIgnored, 0, Some(&candidate_id));
        append_journal(
            &mut state,
            JournalEventKind::CandidateIgnored,
            session,
            Some(candidate_id),
            message,
        );
        true
    }
}

pub struct ConfigureAutoRegistration<'a> {
    session: &'a DiscoverySession,
}

impl<'a> ConfigureAutoRegistration<'a> {
    pub fn new(session: &'a DiscoverySession) -> Self {
        Self { session }
    }

    pub async fn execute(&self, enabled: bool) -> bool {
        let mut state = self.session.state.lock().expect("discovery state poisoned");
        state.auto_enabled = enabled;
        if !enabled {
            state.auto.retain(|_, value| *value != AutoState::Pending);
            state.scheduled.clear();
        }
        enabled
    }
}

fn synchronize_session(state: &mut State, session: RuntimeSessionId) {
    if state.active_session == Some(session) {
        return;
    }
    state.active_session = Some(session);
    state.generation = None;
    state.candidates.clear();
    state.ignored.clear();
    state.auto.clear();
    state.scheduled.clear();
    state.last_scheduled_generation = None;
}

fn schedule_key(schedule: &AutoRegistrationSchedule) -> ScheduleKey {
    ScheduleKey {
        session: schedule.runtime_session_id,
        generation: schedule.inventory_generation,
    }
}

fn auto_key(candidate: &DiscoveryCandidate) -> AutoKey {
    AutoKey {
        session: candidate.runtime_session_id,
        candidate: candidate.candidate_id.clone(),
        metadata_hash: metadata_hash(&candidate.config_files),
    }
}

fn auto_key_schedulable(state: &State, candidate: &DiscoveryCandidate, generation: u64) -> bool {
    match state.auto.get(&auto_key(candidate)) {
        None => true,
        Some(AutoState::RetryableFailure { next_generation }) => generation >= *next_generation,
        Some(AutoState::Pending | AutoState::Succeeded | AutoState::TerminalFailure) => false,
    }
}

fn metadata_hash(files: &[String]) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for file in files {
        for byte in (file.len() as u64)
            .to_be_bytes()
            .iter()
            .chain(file.as_bytes())
        {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    format!("{hash:016x}")
}

fn append_journal(
    state: &mut State,
    kind: JournalEventKind,
    runtime_session_id: Option<RuntimeSessionId>,
    candidate_id: Option<CandidateId>,
    message: String,
) {
    state.next_sequence = state.next_sequence.saturating_add(1);
    if state.journal.len() == JOURNAL_CAPACITY {
        state.journal.pop_front();
    }
    let failed = matches!(
        kind,
        JournalEventKind::ConnectFailed
            | JournalEventKind::DisconnectFailed
            | JournalEventKind::ReconnectFailed
            | JournalEventKind::ManualRegistrationFailed
            | JournalEventKind::AutoRegistrationFailed
            | JournalEventKind::BackupFailed
            | JournalEventKind::RestoreFailed
            | JournalEventKind::ProfileLifecycleFailed
            | JournalEventKind::ContainerActionFailed
    );
    state.journal.push_back(JournalEntry {
        sequence: state.next_sequence,
        timestamp: current_timestamp(),
        runtime_session_id,
        kind,
        severity: if failed {
            JournalSeverity::Error
        } else {
            JournalSeverity::Info
        },
        candidate_id,
        stable_error_code: None,
        message: message.chars().take(MESSAGE_LIMIT).collect(),
    });
}

fn current_timestamp() -> Timestamp {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = (seconds / 86_400) as i64;
    let second_of_day = seconds % 86_400;
    let (year, month, day) = civil_date(days);
    Timestamp(format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        second_of_day / 3_600,
        second_of_day % 3_600 / 60,
        second_of_day % 60,
    ))
}

fn civil_date(days_since_epoch: i64) -> (i64, i64, i64) {
    let days = days_since_epoch + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month, day)
}

fn static_message(kind: JournalEventKind, count: usize, candidate: Option<&CandidateId>) -> String {
    match kind {
        JournalEventKind::CandidateIgnored => format!(
            "Candidate {} ignored",
            candidate.expect("candidate event has ID")
        ),
        JournalEventKind::ConnectStarted => format!("Connection started for {count} target(s)"),
        JournalEventKind::ConnectSucceeded => format!("Connection succeeded for {count} target(s)"),
        JournalEventKind::ConnectFailed => format!("Connection failed for {count} target(s)"),
        _ => format!("Application operation processed {count} item(s)"),
    }
}
