use crate::runtime::bollard_fingerprint;
use bollard::container::{InspectContainerOptions, ListContainersOptions};
use bollard::models::{ContainerInspectResponse, ContainerSummary};
use bollard::Docker;
use colui_app::RuntimeFuture;
use colui_domain::{
    AppError, AppErrorCode, ContainerDetails, ContainerId, ContainerInstance, ContainerState,
    DaemonFingerprint, PortBinding,
};
use std::collections::BTreeMap;

pub trait DockerControl: Send + Sync {
    fn info(&self) -> RuntimeFuture<'_, DaemonFingerprint>;
    fn list(&self) -> RuntimeFuture<'_, Vec<ContainerInstance>>;
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
    fn list(&self) -> RuntimeFuture<'_, Vec<ContainerInstance>> {
        Box::pin(async move {
            self.docker
                .list_containers(Some(ListContainersOptions::<String> {
                    all: true,
                    ..Default::default()
                }))
                .await
                .map_err(api_error)
                .map(|items| items.into_iter().map(summary).collect())
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
            .and_then(|v| v.get("com.docker.compose.service").cloned()),
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
                .and_then(|v| v.get("com.docker.compose.service").cloned()),
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
    use super::inspect_ports;

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
