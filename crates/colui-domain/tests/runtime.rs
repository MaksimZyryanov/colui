use colui_domain::{DaemonFingerprint, DockerEndpoint, MismatchDetails};

#[test]
fn endpoint_and_fingerprint_round_trip_without_runtime_dependencies() {
    let endpoint = DockerEndpoint::try_from("unix:///var/run/docker.sock").unwrap();
    let fingerprint = DaemonFingerprint::new("id", "20.10.17", "linux", "aarch64");
    assert_eq!(endpoint.as_str(), "unix:///var/run/docker.sock");
    assert_eq!(fingerprint.server_version(), "20.10.17");
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
