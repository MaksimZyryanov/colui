use colui_domain::{
    ContainerDetails, ContainerId, ContainerInstance, ContainerState, DaemonFingerprint,
    DockerEndpoint, MismatchDetails, RuntimeSessionState,
};
use std::collections::BTreeMap;

#[test]
fn endpoint_and_fingerprint_round_trip_without_runtime_dependencies() {
    let endpoint = DockerEndpoint::try_from("unix:///var/run/docker.sock").unwrap();
    let fingerprint = DaemonFingerprint::new("id", "20.10.17", "linux", "aarch64");
    assert_eq!(endpoint.as_str(), "unix:///var/run/docker.sock");
    assert_eq!(fingerprint.server_version(), "20.10.17");
}

#[test]
fn endpoint_rejects_empty_values() {
    assert!(DockerEndpoint::try_from("").is_err());
    assert!(DockerEndpoint::try_from("   ").is_err());
}

#[test]
fn endpoint_normalizes_surrounding_whitespace() {
    let endpoint = DockerEndpoint::try_from("  unix:///var/run/docker.sock  ").unwrap();
    assert_eq!(endpoint.as_str(), "unix:///var/run/docker.sock");
}

#[test]
fn fingerprint_serde_preserves_canonical_trimmed_values() {
    let fingerprint: DaemonFingerprint = serde_json::from_str(
        r#"{"daemon_id":" id ","server_version":" 20.10.17 ","os_type":" linux ","architecture":" arm64 "}"#,
    )
    .unwrap();

    assert_eq!(fingerprint.daemon_id, "id");
    assert_eq!(fingerprint.server_version, "20.10.17");
    assert_eq!(fingerprint.os_type, "linux");
    assert_eq!(fingerprint.architecture, "arm64");
}

#[test]
fn fingerprint_versions_are_canonical_exact_values() {
    assert_ne!(
        DaemonFingerprint::new("id", "20.10.17", "linux", "arm64"),
        DaemonFingerprint::new("id", "20.10.17-ce", "linux", "arm64"),
    );
}

#[test]
fn mismatch_keeps_both_observed_fingerprints() {
    let mismatch = MismatchDetails::new(
        DockerEndpoint::try_from("unix:///var/run/docker.sock").unwrap(),
        DaemonFingerprint::new("api", "20.10.17", "linux", "arm64"),
        DaemonFingerprint::new("cli", "20.10.17", "linux", "arm64"),
    );
    assert_ne!(mismatch.api_fingerprint(), mismatch.cli_fingerprint());
}

#[test]
fn session_state_and_container_details_round_trip() {
    let instance = ContainerInstance {
        id: ContainerId("container-1".to_owned()),
        name: "web".to_owned(),
        image: "nginx:latest".to_owned(),
        state: ContainerState::Running,
        status_text: "Up".to_owned(),
        service_name: Some("web".to_owned()),
        published_ports: Vec::new(),
    };
    let details = ContainerDetails {
        instance,
        labels: BTreeMap::from([(String::from("com.example.service"), String::from("web"))]),
    };
    let state = RuntimeSessionState::Disconnected;

    assert_eq!(
        serde_json::to_value(&details).unwrap()["labels"]["com.example.service"],
        "web"
    );
    assert_eq!(
        serde_json::from_value::<RuntimeSessionState>(serde_json::to_value(state).unwrap())
            .unwrap(),
        RuntimeSessionState::Disconnected
    );
}
