use colui_domain::DockerEndpoint;
use std::collections::BTreeMap;

pub type EndpointPreference = DockerEndpoint;

const DEFAULT_ENDPOINT: &str = "unix:///var/run/docker.sock";
const CLEARED_VARIABLES: &[&str] = &[
    "DOCKER_CONTEXT",
    "DOCKER_TLS_VERIFY",
    "DOCKER_CERT_PATH",
    "DOCKER_API_VERSION",
    "DOCKER_CLI_EXPERIMENTAL",
    "COMPOSE_FILE",
    "COMPOSE_PROJECT_NAME",
    "COMPOSE_PROFILES",
    "COMPOSE_PROJECT_DIRECTORY",
    "COMPOSE_PATH_SEPARATOR",
    "COMPOSE_ENV_FILES",
    "COMPOSE_DISABLE_ENV_FILE",
    "COMPOSE_PARALLEL_LIMIT",
    "COMPOSE_MENU",
];

pub fn resolve_endpoint(
    explicit: Option<&str>,
    docker_host: Option<&str>,
    context_host: Option<&str>,
) -> Result<DockerEndpoint, &'static str> {
    explicit
        .or(docker_host)
        .or(context_host)
        .unwrap_or(DEFAULT_ENDPOINT)
        .try_into()
}

pub fn build_cli_environment(
    endpoint: &str,
    inherited: BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut environment = inherited;
    environment.insert("DOCKER_HOST".to_owned(), endpoint.to_owned());
    for variable in CLEARED_VARIABLES {
        environment.remove(*variable);
    }
    environment
}
