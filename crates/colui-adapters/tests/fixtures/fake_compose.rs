use std::{env, fs, io::{self, Write}, process::{Command, Stdio}, thread, time::Duration};

#[cfg(unix)] unsafe extern "C" { fn signal(signal: i32, handler: usize) -> usize; }
#[cfg(unix)] const SIGTERM: i32 = 15;
#[cfg(unix)] fn ignore_term() { unsafe { signal(SIGTERM, 1); } }

fn main() {
    let get = |key: &str| env::var(key).unwrap_or_default();
    if let Ok(path) = env::var("FAKE_ARGS_FILE") { fs::write(path, env::args().skip(1).collect::<Vec<_>>().join("\n") + "\n").unwrap(); }
    if let Ok(path) = env::var("FAKE_CWD_FILE") { fs::write(path, env::current_dir().unwrap().to_string_lossy().as_bytes()).unwrap(); }
    if let Ok(path) = env::var("FAKE_ENV_FILE") { fs::write(path, env::vars().filter(|(k, _)| k.starts_with("FAKE_") || k == "SELECTED_ENV").map(|(k,v)| format!("{k}={v}\n")).collect::<String>()).unwrap(); }
    if get("FAKE_EXEC_MODE") == "signal-aware" { ignore_term(); }
    if get("FAKE_EXEC_MODE") == "spawn-child" || get("FAKE_EXEC_MODE") == "signal-aware" {
        let child = Command::new(env::current_exe().unwrap()).env("FAKE_EXEC_MODE", "sleep").env("FAKE_SLEEP_MS", "10000").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
        if let Ok(path) = env::var("FAKE_CHILD_PID_FILE") { fs::write(path, child.id().to_string()).unwrap(); }
    }
    let write_bytes = |stream: &mut dyn Write, count: usize| { let mut left = count; let buf = [b'Z'; 8192]; while left > 0 { let n = left.min(buf.len()); stream.write_all(&buf[..n]).unwrap(); left -= n; } stream.flush().unwrap(); };
    write_bytes(&mut io::stdout(), get("FAKE_STDOUT_BYTES").parse().unwrap_or(0));
    if get("FAKE_INVALID_UTF8") == "1" { io::stdout().write_all(&[0xff, 0xfe]).unwrap(); }
    write_bytes(&mut io::stderr(), get("FAKE_STDERR_BYTES").parse().unwrap_or(0));
    if get("FAKE_SLEEP_MS").parse::<u64>().unwrap_or(0) > 0 { thread::sleep(Duration::from_millis(get("FAKE_SLEEP_MS").parse().unwrap())); }
    std::process::exit(get("FAKE_EXIT_CODE").parse().unwrap_or(if get("FAKE_EXEC_MODE") == "failure" { 1 } else { 0 }));
}
