use colui_adapters::runtime::{
    bollard_fingerprint, build_cli_environment, parse_cli_fingerprint, resolve_endpoint,
    EndpointPreference,
};
use colui_domain::AppErrorCode;
use std::collections::BTreeMap;

fn inherited_fixture_env() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("DOCKER_HOST".into(), "unix:///inherited".into()),
        ("DOCKER_CONTEXT".into(), "desktop-linux".into()),
        ("DOCKER_CONFIG".into(), "/tmp/docker-config".into()),
        ("DOCKER_TLS_VERIFY".into(), "1".into()),
        ("COMPOSE_FILE".into(), "compose.yml".into()),
        ("DOCKER_BUILDKIT".into(), "1".into()),
        ("PATH".into(), "/usr/bin".into()),
    ])
}

#[test]
fn endpoint_resolution_prefers_explicit_then_docker_host_then_socket() {
    assert_eq!(
        resolve_endpoint(Some("unix:///explicit"), Some("unix:///env"))
            .unwrap()
            .as_str(),
        "unix:///explicit"
    );
    assert_eq!(
        resolve_endpoint(None, Some("unix:///env"))
            .unwrap()
            .as_str(),
        "unix:///env"
    );
    assert_eq!(
        resolve_endpoint(None, None).unwrap().as_str(),
        "unix:///var/run/docker.sock"
    );
}

#[test]
fn endpoint_preference_wraps_explicit_endpoint() {
    let preference = EndpointPreference::try_from("unix:///explicit").unwrap();
    assert_eq!(preference.as_str(), "unix:///explicit");
}

#[test]
fn cli_environment_clears_context_and_compose_selection() {
    let env = build_cli_environment("unix:///resolved", inherited_fixture_env());
    assert_eq!(env.get("DOCKER_HOST").unwrap(), "unix:///resolved");
    assert!(!env.contains_key("DOCKER_CONTEXT"));
    assert!(!env.contains_key("COMPOSE_FILE"));
    assert!(!env.contains_key("DOCKER_TLS_VERIFY"));
    assert_eq!(env.get("DOCKER_CONFIG").unwrap(), "/tmp/docker-config");
    assert_eq!(env.get("DOCKER_BUILDKIT").unwrap(), "1");
    assert_eq!(env.get("PATH").unwrap(), "/usr/bin");
}

#[test]
fn cli_info_parser_extracts_exact_server_fingerprint() {
    let output = "ID: abc\nServer Version: 20.10.17\nOSType: linux\nArchitecture: aarch64\n";
    assert_eq!(
        parse_cli_fingerprint(output).unwrap().server_version(),
        "20.10.17"
    );
}

#[test]
fn cli_info_parser_rejects_missing_or_ambiguous_server_fields() {
    let missing = "ID: abc\nOSType: linux\nArchitecture: aarch64\n";
    assert!(parse_cli_fingerprint(missing).is_err());

    let ambiguous = "ID: abc\nServer Version: 20.10.17\nServer Version: 20.10.18\nOSType: linux\nArchitecture: aarch64\n";
    assert!(parse_cli_fingerprint(ambiguous).is_err());
}

#[test]
fn bollard_info_maps_four_fingerprint_fields() {
    let info: bollard::models::SystemInfo = serde_json::from_str(
        r#"{"ID":"abc","ServerVersion":"20.10.17","OSType":"linux","Architecture":"aarch64"}"#,
    )
    .unwrap();
    let fingerprint = bollard_fingerprint(&info).unwrap();
    assert_eq!(fingerprint.server_version(), "20.10.17");
    assert_eq!(fingerprint.daemon_id, "abc");
    assert_eq!(fingerprint.os_type, "linux");
    assert_eq!(fingerprint.architecture, "aarch64");
}

#[test]
fn bollard_info_rejects_incomplete_fingerprint_fields() {
    let info: bollard::models::SystemInfo =
        serde_json::from_str(r#"{"ID":"abc","ServerVersion":"20.10.17","OSType":"linux"}"#)
            .unwrap();
    let error = bollard_fingerprint(&info).unwrap_err();
    assert_eq!(error.code, AppErrorCode::RuntimeConnectionFailed);
}
