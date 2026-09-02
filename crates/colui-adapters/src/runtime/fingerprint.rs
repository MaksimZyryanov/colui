use bollard::models::SystemInfo;
use colui_domain::{AppError, AppErrorCode, DaemonFingerprint};
use std::collections::BTreeMap;

pub fn bollard_fingerprint(info: &SystemInfo) -> Result<DaemonFingerprint, AppError> {
    let required = |value: &Option<String>, field: &str| {
        value
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| {
                AppError::new(
                    AppErrorCode::RuntimeConnectionFailed,
                    "bollard_fingerprint",
                    None,
                    "Docker API fingerprint field missing",
                )
                .with_details(field)
            })
    };

    Ok(DaemonFingerprint::new(
        required(&info.id, "ID")?,
        required(&info.server_version, "ServerVersion")?,
        required(&info.os_type, "OSType")?,
        required(&info.architecture, "Architecture")?,
    ))
}

pub fn parse_cli_fingerprint(output: &str) -> Result<DaemonFingerprint, &'static str> {
    let mut fields = BTreeMap::<&str, &str>::new();
    for line in output.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        let canonical_key = match key {
            "ID" => "daemon_id",
            "Server Version" => "server_version",
            "OSType" => "os_type",
            "Architecture" => "architecture",
            _ => continue,
        };
        if value.is_empty() || fields.insert(canonical_key, value).is_some() {
            return Err("docker info fingerprint fields must be present exactly once");
        }
    }

    let get = |key| {
        fields
            .get(key)
            .copied()
            .ok_or("docker info fingerprint field missing")
    };
    Ok(DaemonFingerprint::new(
        get("daemon_id")?,
        get("server_version")?,
        get("os_type")?,
        get("architecture")?,
    ))
}
