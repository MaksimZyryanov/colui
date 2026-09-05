use bollard::{container::LogsOptions, Docker, API_DEFAULT_VERSION};
use futures_util::StreamExt;
use std::io::{Read, Write};
use std::time::Duration;

#[tokio::test]
async fn bollard_unterminated_tty_limitation_requires_raw_log_transport() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        let request = String::from_utf8(request).unwrap();
        let line = request.lines().next().unwrap();
        assert!(line.starts_with("GET /containers/abc/logs?"));
        for option in [
            "stdout=true",
            "stderr=true",
            "follow=false",
            "timestamps=false",
            "since=0",
            "tail=4096",
        ] {
            assert!(line.contains(option), "{line}");
        }
        // Bollard also elides until=0; the raw transport sends the explicit fixed option.
        assert!(!line.contains("until="));
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/vnd.docker.raw-stream\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc").unwrap();
    });
    let docker =
        Docker::connect_with_http(&format!("http://{address}"), 5, API_DEFAULT_VERSION).unwrap();
    let mut logs = docker.logs(
        "abc",
        Some(LogsOptions::<String> {
            stdout: true,
            stderr: true,
            follow: false,
            timestamps: false,
            since: 0,
            until: 0,
            tail: "4096".into(),
        }),
    );
    let first = logs.next().await;
    server.join().unwrap();
    // Keep this dependency regression visible until Bollard can replace the raw transport.
    assert!(first.expect("decoder result").is_err());
}
