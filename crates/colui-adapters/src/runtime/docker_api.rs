use crate::runtime::bollard_fingerprint;
use bollard::container::{
    InspectContainerOptions, ListContainersOptions, RestartContainerOptions, StartContainerOptions,
    StopContainerOptions,
};
use bollard::models::{ContainerInspectResponse, ContainerSummary};
use bollard::Docker;
use colui_app::{container_logs_error, ContainerLogs, LogByteRing};
use colui_app::{container_operation_error, ContainerAction, RuntimeFuture};
use colui_domain::{
    AppError, AppErrorCode, ComposeContainerMetadata, ContainerDetails, ContainerId,
    ContainerInstance, ContainerObservation, ContainerState, DaemonFingerprint, PortBinding,
};
use colui_domain::{DockerEndpoint, Timestamp};
use http_body_util::{BodyExt, Empty};
use hyper::body::Bytes;
use std::collections::BTreeMap;
use std::time::Duration;

pub trait DockerControl: Send + Sync {
    fn info(&self) -> RuntimeFuture<'_, DaemonFingerprint>;
    fn list(&self) -> RuntimeFuture<'_, Vec<ContainerObservation>>;
    fn inspect(&self, id: &ContainerId) -> RuntimeFuture<'_, ContainerDetails>;
    fn action(&self, id: ContainerId, action: ContainerAction) -> RuntimeFuture<'_, ()>;
    fn logs(&self, id: ContainerId) -> RuntimeFuture<'_, ContainerLogs>;
}

pub struct DockerApiAdapter {
    docker: Docker,
    endpoint: DockerEndpoint,
}

impl DockerApiAdapter {
    pub fn new(docker: Docker, endpoint: DockerEndpoint) -> Self {
        Self { docker, endpoint }
    }
    pub fn docker(&self) -> &Docker {
        &self.docker
    }
}

impl DockerControl for DockerApiAdapter {
    fn logs(&self, id: ContainerId) -> RuntimeFuture<'_, ContainerLogs> {
        Box::pin(async move {
            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            let read = async {
                // Only opaque Docker IDs may become path segments.
                if id.0.is_empty() || !id.0.bytes().all(|b| b.is_ascii_alphanumeric()) {
                    return Err(container_logs_error(&id, false));
                }
                let path = format!("/v1.47/containers/{}/logs?stdout=true&stderr=true&follow=false&timestamps=false&since=0&until=0&tail=4096", id.0);
                let builder = hyper_util::client::legacy::Client::builder(
                    hyper_util::rt::TokioExecutor::new(),
                );
                let endpoint = self.endpoint.as_str();
                let response = if let Some(socket) = endpoint.strip_prefix("unix://") {
                    let client = builder.build::<_, Empty<Bytes>>(hyperlocal::UnixConnector);
                    client.get(hyperlocal::Uri::new(socket, &path).into()).await
                } else {
                    let endpoint = endpoint
                        .strip_prefix("tcp://")
                        .map(|e| format!("http://{e}"))
                        .unwrap_or_else(|| endpoint.to_owned());
                    let uri = format!("{}{path}", endpoint.trim_end_matches('/'))
                        .parse()
                        .map_err(|_| container_logs_error(&id, false))?;
                    let client = builder.build_http::<Empty<Bytes>>();
                    client.get(uri).await
                }
                .map_err(|_| container_logs_error(&id, true))?;
                if !response.status().is_success() {
                    return Err(container_logs_error(
                        &id,
                        response.status().is_server_error(),
                    ));
                }
                // API >= 1.42 distinguishes raw TTY from multiplexed logs by media type.
                // Bypass Bollard's line/full-frame decoder so limits apply before buffering.
                let multiplexed = match response
                    .headers()
                    .get(hyper::header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .map(|v| v.split(';').next().unwrap_or("").trim())
                {
                    Some("application/vnd.docker.raw-stream") => false,
                    Some("application/vnd.docker.multiplexed-stream") => true,
                    _ => return Err(container_logs_error(&id, false)),
                };
                let mut body = response.into_body();
                let mut ring = LogByteRing::default();
                let mut transferred = 0_usize;
                let mut header = [0_u8; 8];
                let mut header_len = 0;
                let mut remaining = 0_usize;
                while let Some(frame) = body.frame().await {
                    let frame = frame.map_err(|_| container_logs_error(&id, true))?;
                    let Ok(data) = frame.into_data() else {
                        continue;
                    };
                    transferred = transferred.saturating_add(data.len());
                    if transferred >= 8 * 1024 * 1024 {
                        return Err(container_logs_error(&id, true));
                    }
                    if !multiplexed {
                        ring.push(&data);
                        continue;
                    }
                    let mut bytes = data.as_ref();
                    while !bytes.is_empty() {
                        if remaining != 0 {
                            let length = remaining.min(bytes.len());
                            ring.push(&bytes[..length]);
                            remaining -= length;
                            bytes = &bytes[length..];
                        } else {
                            let length = (8 - header_len).min(bytes.len());
                            header[header_len..header_len + length]
                                .copy_from_slice(&bytes[..length]);
                            header_len += length;
                            bytes = &bytes[length..];
                            if header_len == 8 {
                                if !matches!(header[0], 1 | 2) || header[1..4] != [0, 0, 0] {
                                    return Err(container_logs_error(&id, false));
                                }
                                remaining =
                                    u32::from_be_bytes(header[4..8].try_into().unwrap()) as usize;
                                header_len = 0;
                            }
                        }
                    }
                }
                if header_len != 0 || remaining != 0 {
                    return Err(container_logs_error(&id, true));
                }
                Ok(ring.finish(id.clone(), Timestamp(chrono::Utc::now().to_rfc3339())))
            };
            tokio::time::timeout_at(deadline, read)
                .await
                .map_err(|_| container_logs_error(&id, true))?
        })
    }
    fn action(&self, id: ContainerId, action: ContainerAction) -> RuntimeFuture<'_, ()> {
        Box::pin(async move {
            let result = match action {
                ContainerAction::Start => {
                    self.docker
                        .start_container(&id.0, None::<StartContainerOptions<String>>)
                        .await
                }
                ContainerAction::Stop => {
                    self.docker
                        .stop_container(&id.0, None::<StopContainerOptions>)
                        .await
                }
                ContainerAction::Restart => {
                    self.docker
                        .restart_container(&id.0, None::<RestartContainerOptions>)
                        .await
                }
            };
            result.map_err(|_| container_operation_error(&id, "container operation failed"))
        })
    }
    fn info(&self) -> RuntimeFuture<'_, DaemonFingerprint> {
        Box::pin(async move {
            self.docker
                .info()
                .await
                .map_err(api_error)
                .and_then(|i| bollard_fingerprint(&i))
        })
    }
    fn list(&self) -> RuntimeFuture<'_, Vec<ContainerObservation>> {
        Box::pin(async move {
            self.docker
                .list_containers(Some(ListContainersOptions::<String> {
                    all: true,
                    ..Default::default()
                }))
                .await
                .map_err(api_error)
                .map(|items| items.into_iter().map(normalize_container_summary).collect())
        })
    }
    fn inspect(&self, id: &ContainerId) -> RuntimeFuture<'_, ContainerDetails> {
        let id = id.0.clone();
        Box::pin(async move {
            self.docker
                .inspect_container(&id, Some(InspectContainerOptions { size: false }))
                .await
                .map_err(api_error)
                .map(inspected)
        })
    }
}

fn api_error(_: bollard::errors::Error) -> AppError {
    AppError::new(
        AppErrorCode::RuntimeUnavailable,
        "runtime_request",
        None,
        "Runtime request failed",
    )
}
fn state(value: Option<&str>) -> ContainerState {
    match value {
        Some("running") => ContainerState::Running,
        Some("created") | Some("exited") | Some("dead") | Some("paused") => ContainerState::Stopped,
        _ => ContainerState::Unknown,
    }
}

const COMPOSE_PROJECT_LABEL: &str = "com.docker.compose.project";
const COMPOSE_SERVICE_LABEL: &str = "com.docker.compose.service";
const COMPOSE_WORKING_DIR_LABEL: &str = "com.docker.compose.project.working_dir";
const COMPOSE_CONFIG_FILES_LABEL: &str = "com.docker.compose.project.config_files";

/// Maps one Docker list entry into a runtime-free observation.
///
/// Only the four official Compose labels are retained; the container instance
/// itself stays label-free so grouping stays a coordinator concern.
pub fn normalize_container_summary(value: ContainerSummary) -> ContainerObservation {
    let compose = compose_metadata(value.labels.as_ref());
    ContainerObservation::new(summary(value), compose)
}

fn compose_metadata(
    labels: Option<&std::collections::HashMap<String, String>>,
) -> Option<ComposeContainerMetadata> {
    let labels = labels?;
    // Only a non-blank project label marks a container as Compose-managed.
    let project = labels
        .get(COMPOSE_PROJECT_LABEL)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())?;
    Some(ComposeContainerMetadata {
        project: project.to_owned(),
        service: labels.get(COMPOSE_SERVICE_LABEL).cloned(),
        working_directory: labels.get(COMPOSE_WORKING_DIR_LABEL).cloned(),
        config_files: labels
            .get(COMPOSE_CONFIG_FILES_LABEL)
            .map(|value| config_files(value))
            .unwrap_or_default(),
    })
}

/// Splits the Compose `config-files` label, preserving Docker-provided order.
fn config_files(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_owned)
        .collect()
}

fn summary(value: ContainerSummary) -> ContainerInstance {
    ContainerInstance {
        id: ContainerId(value.id.unwrap_or_default()),
        name: value
            .names
            .and_then(|mut v| v.pop())
            .unwrap_or_default()
            .trim_start_matches('/')
            .into(),
        image: value.image.unwrap_or_default(),
        state: state(value.state.as_deref()),
        status_text: value.status.unwrap_or_default(),
        service_name: value
            .labels
            .as_ref()
            .and_then(|v| v.get(COMPOSE_SERVICE_LABEL).cloned()),
        published_ports: value
            .ports
            .unwrap_or_default()
            .into_iter()
            .map(|p| PortBinding {
                host_ip: p.ip,
                host_port: p.public_port,
                container_port: p.private_port,
                protocol: p.typ.map(|value| value.to_string()).unwrap_or_default(),
            })
            .collect(),
    }
}
fn inspected(value: ContainerInspectResponse) -> ContainerDetails {
    let config = value.config.unwrap_or_default();
    let state_value = value
        .state
        .as_ref()
        .and_then(|v| v.status.as_ref())
        .map(|v| v.as_ref());
    ContainerDetails {
        instance: ContainerInstance {
            id: ContainerId(value.id.unwrap_or_default()),
            name: value
                .name
                .unwrap_or_default()
                .trim_start_matches('/')
                .into(),
            image: config.image.unwrap_or_default(),
            state: state(state_value),
            status_text: value
                .state
                .and_then(|v| v.status)
                .map(|v| v.to_string())
                .unwrap_or_default(),
            service_name: config
                .labels
                .as_ref()
                .and_then(|v| v.get(COMPOSE_SERVICE_LABEL).cloned()),
            published_ports: inspect_ports(value.network_settings.as_ref()),
        },
        labels: config
            .labels
            .unwrap_or_default()
            .into_iter()
            .collect::<BTreeMap<_, _>>(),
    }
}

fn inspect_ports(settings: Option<&bollard::models::NetworkSettings>) -> Vec<PortBinding> {
    let Some(ports) = settings.and_then(|settings| settings.ports.as_ref()) else {
        return vec![];
    };
    ports
        .iter()
        .flat_map(|(container, bindings)| {
            let Some((port, protocol)) = container.rsplit_once('/') else {
                return vec![];
            };
            let Ok(container_port) = port.parse::<u16>() else {
                return vec![];
            };
            if bindings.as_ref().is_none_or(Vec::is_empty) {
                return vec![PortBinding {
                    host_ip: None,
                    host_port: None,
                    container_port,
                    protocol: protocol.to_ascii_lowercase(),
                }];
            }
            bindings
                .as_deref()
                .unwrap_or_default()
                .iter()
                .map(move |binding| PortBinding {
                    host_ip: binding.host_ip.clone(),
                    host_port: binding.host_port.as_deref().and_then(|p| p.parse().ok()),
                    container_port,
                    protocol: protocol.to_ascii_lowercase(),
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{api_error, config_files, inspect_ports, normalize_container_summary};
    use bollard::models::ContainerSummary;
    use colui_domain::AppErrorCode;

    fn summary_json(labels: &str) -> ContainerSummary {
        serde_json::from_str(&format!(
            r#"{{"Id":"abc123","Names":["/checkout-web-1"],"Image":"checkout:latest","State":"running","Status":"Up 2 hours","Labels":{labels},"Ports":[]}}"#
        ))
        .unwrap()
    }

    #[test]
    fn docker_errors_are_sanitized_at_the_adapter_boundary() {
        let secret = "token=super-secret";
        let socket = "/Users/max/.docker/run/docker.sock";
        let endpoint = "tcp://admin:password@private.example:2375";
        let daemon_text = format!("{secret} {socket} {endpoint} {}", "x".repeat(10_000));
        let error = api_error(bollard::errors::Error::DockerResponseServerError {
            status_code: 500,
            message: daemon_text.clone(),
        });
        let projected = serde_json::to_string(&error).unwrap();

        assert_eq!(error.code, AppErrorCode::RuntimeUnavailable);
        assert!(!projected.contains(secret));
        assert!(!projected.contains(socket));
        assert!(!projected.contains(endpoint));
        assert!(!projected.contains(&daemon_text));
        assert!(error
            .details
            .as_ref()
            .is_none_or(|details| details.len() <= 256));
        assert_ne!(error.operation, "docker_api");
    }

    #[test]
    fn normalization_extracts_only_the_four_official_compose_labels() {
        let observation = normalize_container_summary(summary_json(
            r#"{"com.docker.compose.project":"checkout","com.docker.compose.service":"web","com.docker.compose.project.working_dir":"/workspace","com.docker.compose.project.config_files":"/workspace/compose.yml,/workspace/compose.override.yml","com.docker.compose.oneoff":"False","custom.label":"ignored"}"#,
        ));
        let compose = observation.compose.unwrap();
        assert_eq!(compose.project, "checkout");
        assert_eq!(compose.service.as_deref(), Some("web"));
        assert_eq!(compose.working_directory.as_deref(), Some("/workspace"));
        assert_eq!(
            compose.config_files,
            vec![
                "/workspace/compose.yml".to_owned(),
                "/workspace/compose.override.yml".to_owned()
            ]
        );
    }

    #[test]
    fn normalization_keeps_instance_free_of_labels_and_maps_core_fields() {
        let observation = normalize_container_summary(summary_json(
            r#"{"com.docker.compose.project":"checkout","com.docker.compose.service":"web"}"#,
        ));
        assert_eq!(observation.instance.id.0, "abc123");
        assert_eq!(observation.instance.name, "checkout-web-1");
        assert_eq!(observation.instance.image, "checkout:latest");
        assert_eq!(observation.instance.status_text, "Up 2 hours");
        assert_eq!(
            observation.instance.state,
            colui_domain::ContainerState::Running
        );
        assert_eq!(observation.instance.service_name.as_deref(), Some("web"));
    }

    #[test]
    fn project_label_without_optional_labels_yields_empty_metadata_fields() {
        let compose = normalize_container_summary(summary_json(
            r#"{"com.docker.compose.project":"checkout"}"#,
        ))
        .compose
        .unwrap();
        assert_eq!(compose.project, "checkout");
        assert!(compose.service.is_none());
        assert!(compose.working_directory.is_none());
        assert!(compose.config_files.is_empty());
    }

    #[test]
    fn absent_or_empty_labels_produce_standalone_observation() {
        assert!(normalize_container_summary(summary_json("{}"))
            .compose
            .is_none());
        assert!(normalize_container_summary(summary_json("null"))
            .compose
            .is_none());
    }

    #[test]
    fn blank_project_label_is_not_a_valid_compose_association() {
        assert!(normalize_container_summary(summary_json(
            r#"{"com.docker.compose.project":"  ","com.docker.compose.service":"web"}"#
        ))
        .compose
        .is_none());
    }

    #[test]
    fn config_files_preserve_docker_order_and_drop_blank_entries() {
        assert_eq!(
            config_files("/b/second.yml, /a/first.yml ,,"),
            vec!["/b/second.yml".to_owned(), "/a/first.yml".to_owned()]
        );
        assert!(config_files("").is_empty());
    }

    #[test]
    fn inspect_ports_preserves_partial_and_unpublished_entries() {
        let settings = serde_json::from_str(r#"{"Ports":{"80/TCP":[{"HostIp":"::"},{"HostPort":"8080"},{}],"53/udp":null,"443/tcp":[]}}"#).unwrap();
        let ports = inspect_ports(Some(&settings));
        assert_eq!(ports.len(), 5);
        assert!(ports.iter().any(|p| p.host_ip.as_deref() == Some("::")
            && p.host_port.is_none()
            && p.protocol == "tcp"));
        assert!(ports
            .iter()
            .any(|p| p.host_ip.is_none() && p.host_port == Some(8080)));
        for service in [53, 443] {
            assert!(ports.iter().any(|p| p.container_port == service
                && p.host_ip.is_none()
                && p.host_port.is_none()));
        }
    }

    #[test]
    fn inspect_ports_maps_multiple_hosts_and_protocols() {
        let settings: bollard::models::NetworkSettings = serde_json::from_str(
            r#"{"Ports":{"80/tcp":[{"HostIp":"127.0.0.1","HostPort":"8080"},{"HostIp":"0.0.0.0","HostPort":"18080"}],"53/udp":[{"HostIp":"127.0.0.1","HostPort":"5353"}]}}"#,
        ).unwrap();
        let ports = inspect_ports(Some(&settings));
        assert_eq!(ports.len(), 3);
        assert!(ports.iter().any(|port| {
            port.container_port == 80
                && port.host_port == Some(8080)
                && port.host_ip.as_deref() == Some("127.0.0.1")
                && port.protocol == "tcp"
        }));
        assert!(ports.iter().any(|port| {
            port.container_port == 80
                && port.host_port == Some(18080)
                && port.host_ip.as_deref() == Some("0.0.0.0")
                && port.protocol == "tcp"
        }));
        assert!(ports.iter().any(|port| port.container_port == 53
            && port.host_port == Some(5353)
            && port.protocol == "udp"));
    }
}
