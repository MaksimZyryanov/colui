#![cfg(feature = "docker-tests")]

#[tokio::test]
async fn docker_fixture_skips_when_daemon_is_unavailable() {
    let result = tokio::process::Command::new("docker")
        .arg("info")
        .output()
        .await;
    if result
        .as_ref()
        .map(|output| output.status.success())
        .unwrap_or(false)
    {
        eprintln!("docker-tests fixture hook: daemon available");
    } else {
        eprintln!("skipping real Docker fixture: daemon unavailable");
    }
}
