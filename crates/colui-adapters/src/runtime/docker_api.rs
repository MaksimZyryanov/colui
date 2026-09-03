use crate::runtime::bollard_fingerprint;
use bollard::container::{InspectContainerOptions, ListContainersOptions};
use bollard::models::{ContainerInspectResponse, ContainerSummary};
use bollard::Docker;
use colui_app::RuntimeFuture;
use colui_domain::{
    AppError, AppErrorCode, ComposeContainerMetadata, ContainerDetails, ContainerId,
    ContainerInstance, ContainerObservation, ContainerState, DaemonFingerprint, PortBinding,
};
use std::collections::BTreeMap;

pub trait DockerControl: Send + Sync {
    fn info(&self) -> RuntimeFuture<'_, DaemonFingerprint>;
    fn list(&self) -> RuntimeFuture<'_, Vec<ContainerObservation>>;
    fn inspect(&self, id: &ContainerId) -> RuntimeFuture<'_, ContainerDetails>;
}

pub struct DockerApiAdapter {
    docker: Docker,
}

impl DockerApiAdapter {
    pub fn new(docker: Docker) -> Self {
        Self { docker }
    }
    pub fn docker(&self) -> &Docker {
        &self.docker
    }
}

impl DockerControl for DockerApiAdapter {
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

fn api_error(error: bollard::errors::Error) -> AppError {
    AppError::new(
        AppErrorCode::RuntimeUnavailable,
        "docker_api",
        None,
        "Docker API request failed",
    )
    .with_details(error.to_string())
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
const COMPOSE_WORKING_DIR_LABEL: &str = "com.docker.compose.working_dir";
const COMPOSE_CONFIG_FILES_LABEL: &str = "com.docker.compose.config-files";

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
            .filter_map(|p| {
                Some(PortBinding {
                    host_ip: p.ip.unwrap_or_default(),
                    host_port: p.public_port?,
                    container_port: p.private_port,
                    protocol: p.typ.map(|value| value.to_string()).unwrap_or_default(),
                })
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
            bindings
                .as_deref()
                .unwrap_or_default()
                .iter()
                .filter_map(move |binding| {
                    Some(PortBinding {
                        host_ip: binding.host_ip.clone().unwrap_or_default(),
                        host_port: binding.host_port.as_deref()?.parse().ok()?,
                        container_port,
                        protocol: protocol.to_owned(),
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{config_files, inspect_ports, normalize_container_summary};
    use bollard::models::ContainerSummary;

    fn summary_json(labels: &str) -> ContainerSummary {
        serde_json::from_str(&format!(
            r#"{{"Id":"abc123","Names":["/checkout-web-1"],"Image":"checkout:latest","State":"running","Status":"Up 2 hours","Labels":{labels},"Ports":[]}}"#
        ))
        .unwrap()
    }

    #[test]
    fn normalization_extracts_only_the_four_official_compose_labels() {
        let observation = normalize_container_summary(summary_json(
            r#"{"com.docker.compose.project":"checkout","com.docker.compose.service":"web","com.docker.compose.working_dir":"/workspace","com.docker.compose.config-files":"/workspace/compose.yml,/workspace/compose.override.yml","com.docker.compose.oneoff":"False","custom.label":"ignored"}"#,
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
    fn inspect_ports_maps_multiple_hosts_and_protocols() {
        let settings: bollard::models::NetworkSettings = serde_json::from_str(
            r#"{"Ports":{"80/tcp":[{"HostIp":"127.0.0.1","HostPort":"8080"},{"HostIp":"0.0.0.0","HostPort":"18080"}],"53/udp":[{"HostIp":"127.0.0.1","HostPort":"5353"}]}}"#,
        ).unwrap();
        let ports = inspect_ports(Some(&settings));
        assert_eq!(ports.len(), 3);
        assert!(ports.iter().any(|port| {
            port.container_port == 80
                && port.host_port == 8080
                && port.host_ip == "127.0.0.1"
                && port.protocol == "tcp"
        }));
        assert!(ports.iter().any(|port| {
            port.container_port == 80
                && port.host_port == 18080
                && port.host_ip == "0.0.0.0"
                && port.protocol == "tcp"
        }));
        assert!(ports.iter().any(|port| port.container_port == 53
            && port.host_port == 5353
            && port.protocol == "udp"));
    }
}
