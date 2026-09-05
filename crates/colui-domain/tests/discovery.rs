use colui_domain::{
    classify_candidates, ComposeObservationGroup, ComposeProjectName, ContainerId,
    DiscoveryClassification, DiscoveryConflictSource, DisplayName, ProfileDraft, ProfileId,
    ProjectProfile, RegistrationOrigin, RuntimeSessionId,
};
use std::convert::TryFrom;
use std::path::PathBuf;
use uuid::Uuid;

fn session(value: u128) -> RuntimeSessionId {
    RuntimeSessionId::new(Uuid::from_u128(value))
}

fn group(
    name: &str,
    working_directory: Option<&str>,
    files: &[&str],
    containers: usize,
) -> ComposeObservationGroup {
    ComposeObservationGroup {
        compose_project_name: name.to_owned(),
        working_directory: working_directory.map(str::to_owned),
        config_files: files.iter().map(|value| (*value).to_owned()).collect(),
        container_ids: (0..containers)
            .map(|index| ContainerId(format!("container-{index}")))
            .collect(),
    }
}

fn profile(
    id: u128,
    name: &str,
    working_directory: PathBuf,
    files: Vec<PathBuf>,
) -> ProjectProfile {
    ProjectProfile::from_draft(
        ProfileId::new(Uuid::from_u128(id)),
        ProfileDraft {
            display_name: DisplayName::try_from(name).unwrap(),
            compose_project_name: ComposeProjectName::try_from(name).unwrap(),
            working_directory,
            compose_files: files,
            environment_files: Vec::new(),
            registration_origin: RegistrationOrigin::Manual,
        },
    )
    .unwrap()
}

#[test]
fn classifies_complete_and_incomplete_observations() {
    struct Case {
        name: &'static str,
        group: ComposeObservationGroup,
        want: DiscoveryClassification,
        want_directory: Option<&'static str>,
        want_files: &'static [&'static str],
    }

    let cases = [
        Case {
            name: "complete metadata",
            group: group(
                "shop",
                Some(" /srv/./team/../shop "),
                &[" compose.yml ", "./compose.yml", "extras/../dev.yml"],
                2,
            ),
            want: DiscoveryClassification::NewUnambiguous,
            want_directory: Some("/srv/shop"),
            want_files: &["/srv/shop/compose.yml", "/srv/shop/dev.yml"],
        },
        Case {
            name: "invalid Compose grammar",
            group: group("Shop", Some("/srv/shop"), &["compose.yml"], 1),
            want: DiscoveryClassification::IncompleteMetadata,
            want_directory: Some("/srv/shop"),
            want_files: &["/srv/shop/compose.yml"],
        },
        Case {
            name: "missing working directory",
            group: group("shop", None, &["compose.yml"], 1),
            want: DiscoveryClassification::IncompleteMetadata,
            want_directory: None,
            want_files: &[],
        },
        Case {
            name: "relative working directory",
            group: group("shop", Some("srv/shop"), &["compose.yml"], 1),
            want: DiscoveryClassification::IncompleteMetadata,
            want_directory: None,
            want_files: &[],
        },
        Case {
            name: "missing config files",
            group: group("shop", Some("/srv/shop"), &[" ", ""], 1),
            want: DiscoveryClassification::IncompleteMetadata,
            want_directory: Some("/srv/shop"),
            want_files: &[],
        },
    ];

    for case in cases {
        let candidates = classify_candidates(session(1), 7, &[case.group], &[]);
        assert_eq!(candidates.len(), 1, "{}", case.name);
        let candidate = &candidates[0];
        assert_eq!(candidate.classification, case.want, "{}", case.name);
        assert_eq!(
            candidate.working_directory.as_deref(),
            case.want_directory,
            "{}",
            case.name
        );
        assert_eq!(
            candidate
                .config_files
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            case.want_files,
            "{}",
            case.name
        );
        assert_eq!(candidate.inventory_generation, 7);
        assert_eq!(
            candidate.container_count,
            if case.name == "complete metadata" {
                2
            } else {
                1
            }
        );
        assert!(!candidate.ignored);
    }
}

#[test]
fn classifies_registered_tuple_and_profile_name_conflicts() {
    let observation = group("shop", Some("/srv/shop"), &["compose.yml", "dev.yml"], 1);
    let matching = profile(
        10,
        "shop",
        PathBuf::from("/srv/shop"),
        vec![PathBuf::from("compose.yml"), PathBuf::from("./dev.yml")],
    );
    let conflicting = profile(
        11,
        "shop",
        PathBuf::from("/opt/shop"),
        vec![PathBuf::from("compose.yml")],
    );

    let registered = classify_candidates(
        session(1),
        1,
        std::slice::from_ref(&observation),
        std::slice::from_ref(&matching),
    );
    assert_eq!(
        registered[0].classification,
        DiscoveryClassification::AlreadyRegistered
    );
    assert!(registered[0].conflicts.is_empty());

    for profiles in [
        vec![conflicting.clone()],
        vec![matching.clone(), conflicting.clone()],
    ] {
        let candidates =
            classify_candidates(session(1), 1, std::slice::from_ref(&observation), &profiles);
        assert_eq!(
            candidates[0].classification,
            DiscoveryClassification::NameConflict
        );
        assert!(candidates[0].conflicts.iter().any(|evidence| {
            evidence.source == DiscoveryConflictSource::RegisteredProfile
                && evidence.profile_id.as_ref() == Some(conflicting.id())
        }));
    }

    let duplicate_matches = classify_candidates(
        session(1),
        1,
        &[observation],
        &[
            matching.clone(),
            profile(
                12,
                "shop",
                PathBuf::from("/srv/shop"),
                vec![PathBuf::from("compose.yml"), PathBuf::from("dev.yml")],
            ),
        ],
    );
    assert_eq!(
        duplicate_matches[0].classification,
        DiscoveryClassification::NameConflict
    );
    assert_eq!(duplicate_matches[0].conflicts.len(), 2);
}

#[test]
fn complete_and_incomplete_same_name_observations_block_registration() {
    let candidates = classify_candidates(
        session(1),
        1,
        &[
            group("shop", Some("/srv/shop"), &["compose.yml"], 1),
            group("shop", None, &["compose.yml"], 1),
        ],
        &[],
    );

    assert_eq!(candidates.len(), 2);
    assert_eq!(
        candidates[0].classification,
        DiscoveryClassification::IncompleteMetadata
    );
    assert_eq!(
        candidates[1].classification,
        DiscoveryClassification::NameConflict
    );
    assert!(candidates[1]
        .conflicts
        .iter()
        .any(
            |evidence| evidence.source == DiscoveryConflictSource::RuntimeObservation
                && evidence.working_directory.is_none()
        ));
}

#[test]
fn changed_config_files_keep_identity_but_collapse_to_conflict() {
    let candidates = classify_candidates(
        session(1),
        9,
        &[
            group("shop", Some("/srv/shop"), &["compose.yml"], 1),
            group("shop", Some("/srv/shop"), &["compose.yml", "dev.yml"], 2),
        ],
        &[],
    );

    assert_eq!(candidates.len(), 1);
    assert_eq!(
        candidates[0].classification,
        DiscoveryClassification::NameConflict
    );
    assert_eq!(candidates[0].container_count, 3);
    assert_eq!(candidates[0].conflicts.len(), 2);
    assert_eq!(candidates[0].config_files, vec!["/srv/shop/compose.yml"]);
}

#[test]
fn candidate_order_and_ids_are_deterministic_but_session_scoped() {
    let observations = [
        group("zeta", Some("/srv/zeta"), &["compose.yml"], 1),
        group("alpha", Some("/srv/alpha"), &["compose.yml"], 1),
    ];
    let first = classify_candidates(session(1), 4, &observations, &[]);
    let reordered = classify_candidates(
        session(1),
        4,
        &[observations[1].clone(), observations[0].clone()],
        &[],
    );
    let reset = classify_candidates(session(2), 4, &observations, &[]);

    assert_eq!(first, reordered);
    assert_eq!(
        first
            .iter()
            .map(|candidate| candidate.compose_project_name.as_str())
            .collect::<Vec<_>>(),
        ["alpha", "zeta"]
    );
    assert_eq!(
        first[0].candidate_id.as_ref(),
        "7ad8cc94c149a7d92dbfd3b0771ada23541f615f7e155a59da5cb9cbcaee0a9b"
    );
    assert_eq!(first[0].candidate_id.as_ref().len(), 64);
    assert!(first[0]
        .candidate_id
        .as_ref()
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
    assert_ne!(first[0].candidate_id, reset[0].candidate_id);
}

#[cfg(unix)]
#[test]
fn non_utf8_and_relative_profile_paths_are_name_conflicts() {
    use std::os::unix::ffi::OsStringExt;

    let observation = group("shop", Some("/srv/shop"), &["compose.yml"], 1);
    let relative = profile(
        20,
        "shop",
        PathBuf::from("srv/shop"),
        vec![PathBuf::from("compose.yml")],
    );
    let non_utf8 = profile(
        21,
        "shop",
        PathBuf::from(std::ffi::OsString::from_vec(vec![
            b'/', b's', b'r', b'v', b'/', 0xff,
        ])),
        vec![PathBuf::from("compose.yml")],
    );

    for registered in [relative, non_utf8] {
        let candidates = classify_candidates(
            session(1),
            1,
            std::slice::from_ref(&observation),
            &[registered],
        );
        assert_eq!(
            candidates[0].classification,
            DiscoveryClassification::NameConflict
        );
        assert_eq!(
            candidates[0].conflicts[0].source,
            DiscoveryConflictSource::RegisteredProfile
        );
        assert!(candidates[0].conflicts[0].working_directory.is_none());
    }
}
