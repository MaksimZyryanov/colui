use crate::{
    ComposeObservationGroup, ComposeProjectName, ProfileId, ProjectProfile, RuntimeSessionId,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct CandidateId(String);

impl AsRef<str> for CandidateId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CandidateId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiscoveryCandidate {
    pub candidate_id: CandidateId,
    pub runtime_session_id: RuntimeSessionId,
    pub inventory_generation: u64,
    pub compose_project_name: String,
    pub working_directory: Option<String>,
    pub config_files: Vec<String>,
    pub container_count: u32,
    pub classification: DiscoveryClassification,
    pub conflicts: Vec<DiscoveryConflictEvidence>,
    pub ignored: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiscoveryConflictEvidence {
    pub source: DiscoveryConflictSource,
    pub profile_id: Option<ProfileId>,
    pub working_directory: Option<String>,
    pub config_files: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum DiscoveryConflictSource {
    RuntimeObservation,
    RegisteredProfile,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DiscoveryClassification {
    NewUnambiguous,
    NameConflict,
    IncompleteMetadata,
    AlreadyRegistered,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct NormalizedObservation {
    name: String,
    valid_name: bool,
    working_directory: Option<String>,
    config_files: Vec<String>,
    container_count: u32,
}

impl NormalizedObservation {
    fn is_complete(&self) -> bool {
        self.valid_name && self.working_directory.is_some() && !self.config_files.is_empty()
    }

    fn tuple(&self) -> (&Option<String>, &Vec<String>) {
        (&self.working_directory, &self.config_files)
    }

    fn evidence(&self) -> DiscoveryConflictEvidence {
        DiscoveryConflictEvidence {
            source: DiscoveryConflictSource::RuntimeObservation,
            profile_id: None,
            working_directory: self.working_directory.clone(),
            config_files: self.config_files.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NormalizedProfile {
    profile_id: ProfileId,
    working_directory: Option<String>,
    config_files: Vec<String>,
    comparable: bool,
}

impl NormalizedProfile {
    fn evidence(&self) -> DiscoveryConflictEvidence {
        DiscoveryConflictEvidence {
            source: DiscoveryConflictSource::RegisteredProfile,
            profile_id: Some(self.profile_id.clone()),
            working_directory: self.working_directory.clone(),
            config_files: self.config_files.clone(),
        }
    }
}

pub fn classify_candidates(
    runtime_session_id: RuntimeSessionId,
    inventory_generation: u64,
    observation_groups: &[ComposeObservationGroup],
    profiles: &[ProjectProfile],
) -> Vec<DiscoveryCandidate> {
    let mut observations: Vec<_> = observation_groups
        .iter()
        .map(normalize_observation)
        .collect();
    observations.sort();

    let mut profiles_by_name: BTreeMap<String, Vec<NormalizedProfile>> = BTreeMap::new();
    for profile in profiles {
        profiles_by_name
            .entry(profile.compose_project_name().as_ref().to_owned())
            .or_default()
            .push(normalize_profile(profile));
    }
    for values in profiles_by_name.values_mut() {
        values.sort_by(|left, right| {
            (
                &left.working_directory,
                &left.config_files,
                left.profile_id.to_string(),
            )
                .cmp(&(
                    &right.working_directory,
                    &right.config_files,
                    right.profile_id.to_string(),
                ))
        });
    }

    let mut by_identity: BTreeMap<CandidateId, Vec<NormalizedObservation>> = BTreeMap::new();
    for observation in &observations {
        by_identity
            .entry(candidate_id(
                &runtime_session_id,
                &observation.name,
                observation.working_directory.as_deref(),
            ))
            .or_default()
            .push(observation.clone());
    }

    let mut candidates = Vec::with_capacity(by_identity.len());
    for (candidate_id, identity_observations) in by_identity {
        let representative = &identity_observations[0];
        let same_name_observations: Vec<_> = observations
            .iter()
            .filter(|other| {
                representative.valid_name && other.valid_name && other.name == representative.name
            })
            .collect();
        let same_name_profiles = profiles_by_name
            .get(&representative.name)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let matching_profiles = same_name_profiles
            .iter()
            .filter(|profile| {
                profile.comparable
                    && (&profile.working_directory, &profile.config_files) == representative.tuple()
            })
            .count();
        let runtime_conflict = same_name_observations
            .iter()
            .any(|other| other.tuple() != representative.tuple());
        let profile_conflict = !same_name_profiles.is_empty()
            && (matching_profiles != same_name_profiles.len() || matching_profiles > 1);
        let identity_metadata_conflict = identity_observations
            .iter()
            .any(|other| other.config_files != representative.config_files);

        let classification = if !representative.valid_name {
            DiscoveryClassification::IncompleteMetadata
        } else if identity_metadata_conflict {
            DiscoveryClassification::NameConflict
        } else if !representative.is_complete() {
            DiscoveryClassification::IncompleteMetadata
        } else if runtime_conflict || profile_conflict {
            DiscoveryClassification::NameConflict
        } else if matching_profiles == 1 {
            DiscoveryClassification::AlreadyRegistered
        } else {
            DiscoveryClassification::NewUnambiguous
        };

        let mut conflicts = Vec::new();
        if classification == DiscoveryClassification::NameConflict {
            if identity_metadata_conflict {
                for observation in &identity_observations {
                    let evidence = observation.evidence();
                    if !conflicts.contains(&evidence) {
                        conflicts.push(evidence);
                    }
                }
            }
            if runtime_conflict {
                for observation in same_name_observations
                    .iter()
                    .filter(|other| other.tuple() != representative.tuple())
                {
                    let evidence = observation.evidence();
                    if !conflicts.contains(&evidence) {
                        conflicts.push(evidence);
                    }
                }
            }
            if profile_conflict {
                conflicts.extend(same_name_profiles.iter().map(NormalizedProfile::evidence));
            }
        }

        candidates.push(DiscoveryCandidate {
            candidate_id,
            runtime_session_id,
            inventory_generation,
            compose_project_name: representative.name.clone(),
            working_directory: representative.working_directory.clone(),
            config_files: representative.config_files.clone(),
            container_count: identity_observations
                .iter()
                .fold(0_u32, |count, observation| {
                    count.saturating_add(observation.container_count)
                }),
            classification,
            conflicts,
            ignored: false,
        });
    }

    candidates.sort_by(|left, right| {
        (
            &left.compose_project_name,
            &left.working_directory,
            &left.config_files,
            &left.candidate_id,
        )
            .cmp(&(
                &right.compose_project_name,
                &right.working_directory,
                &right.config_files,
                &right.candidate_id,
            ))
    });
    candidates
}

fn normalize_observation(group: &ComposeObservationGroup) -> NormalizedObservation {
    let name = group.compose_project_name.trim().to_owned();
    let valid_name = ComposeProjectName::try_from(name.as_str()).is_ok();
    let working_directory = group
        .working_directory
        .as_deref()
        .and_then(|value| normalize_absolute(Path::new(value.trim())));
    let config_files = normalize_files(
        working_directory.as_deref(),
        group.config_files.iter().map(String::as_str),
    );
    NormalizedObservation {
        name,
        valid_name,
        working_directory,
        config_files,
        container_count: u32::try_from(group.container_ids.len()).unwrap_or(u32::MAX),
    }
}

fn normalize_profile(profile: &ProjectProfile) -> NormalizedProfile {
    let working_directory = profile
        .working_directory()
        .to_str()
        .and_then(|value| normalize_absolute(Path::new(value)));
    let paths_are_utf8 = profile
        .compose_files()
        .iter()
        .all(|path| path.to_str().is_some());
    let config_files = if working_directory.is_some() && paths_are_utf8 {
        normalize_files(
            working_directory.as_deref(),
            profile
                .compose_files()
                .iter()
                .filter_map(|path| path.to_str()),
        )
    } else {
        Vec::new()
    };
    let comparable = working_directory.is_some() && paths_are_utf8 && !config_files.is_empty();
    NormalizedProfile {
        profile_id: profile.id().clone(),
        working_directory,
        config_files,
        comparable,
    }
}

fn normalize_files<'a>(
    working_directory: Option<&str>,
    files: impl Iterator<Item = &'a str>,
) -> Vec<String> {
    let mut normalized = Vec::new();
    for file in files {
        let trimmed = file.trim();
        if trimmed.is_empty() {
            continue;
        }
        let path = Path::new(trimmed);
        let absolute = if path.is_absolute() {
            normalize_absolute(path)
        } else {
            working_directory
                .and_then(|directory| normalize_absolute(&Path::new(directory).join(path)))
        };
        if let Some(absolute) = absolute {
            if !normalized.contains(&absolute) {
                normalized.push(absolute);
            }
        }
    }
    normalized
}

fn normalize_absolute(path: &Path) -> Option<String> {
    if !path.is_absolute() {
        return None;
    }
    let mut normalized = PathBuf::from("/");
    for component in path.components() {
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(value) => normalized.push(value),
            Component::Prefix(_) => return None,
        }
    }
    normalized.to_str().map(str::to_owned)
}

fn candidate_id(
    session: &RuntimeSessionId,
    name: &str,
    working_directory: Option<&str>,
) -> CandidateId {
    let mut encoded = b"colui-candidate-v1".to_vec();
    encoded.extend_from_slice(session.as_uuid().as_bytes());
    append_string(&mut encoded, name);
    match working_directory {
        Some(directory) => {
            encoded.push(1);
            append_string(&mut encoded, directory);
        }
        None => encoded.push(0),
    }
    CandidateId(hex(&sha256(&encoded)))
}

fn append_string(encoded: &mut Vec<u8>, value: &str) {
    let length = u32::try_from(value.len()).unwrap_or(u32::MAX);
    encoded.extend_from_slice(&length.to_be_bytes());
    encoded.extend_from_slice(value.as_bytes());
}

fn hex(bytes: &[u8; 32]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(64);
    for byte in bytes {
        value.push(DIGITS[(byte >> 4) as usize] as char);
        value.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    value
}

fn sha256(input: &[u8]) -> [u8; 32] {
    const INITIAL: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let bit_length = (input.len() as u64).wrapping_mul(8);
    let mut padded = input.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_length.to_be_bytes());
    let mut hash = INITIAL;
    for chunk in padded.as_chunks::<64>().0 {
        let mut words = [0_u32; 64];
        for (index, bytes) in chunk.as_chunks::<4>().0.iter().enumerate() {
            words[index] = u32::from_be_bytes(*bytes);
        }
        for index in 16..64 {
            let s0 = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let s1 = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(s0)
                .wrapping_add(words[index - 7])
                .wrapping_add(s1);
        }
        let mut state = hash;
        for index in 0..64 {
            let sum1 =
                state[4].rotate_right(6) ^ state[4].rotate_right(11) ^ state[4].rotate_right(25);
            let choice = (state[4] & state[5]) ^ (!state[4] & state[6]);
            let temp1 = state[7]
                .wrapping_add(sum1)
                .wrapping_add(choice)
                .wrapping_add(K[index])
                .wrapping_add(words[index]);
            let sum0 =
                state[0].rotate_right(2) ^ state[0].rotate_right(13) ^ state[0].rotate_right(22);
            let majority = (state[0] & state[1]) ^ (state[0] & state[2]) ^ (state[1] & state[2]);
            let temp2 = sum0.wrapping_add(majority);
            state = [
                temp1.wrapping_add(temp2),
                state[0],
                state[1],
                state[2],
                state[3].wrapping_add(temp1),
                state[4],
                state[5],
                state[6],
            ];
        }
        for index in 0..8 {
            hash[index] = hash[index].wrapping_add(state[index]);
        }
    }
    let mut output = [0_u8; 32];
    for (index, word) in hash.iter().enumerate() {
        output[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    output
}
