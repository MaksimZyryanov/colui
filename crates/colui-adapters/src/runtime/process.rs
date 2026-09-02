use colui_app::{ComposeInvocation, ComposeProcessResult, ComposeRunner, RuntimeFuture};
use colui_domain::{AppError, AppErrorCode};
use std::io;
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, Command};

const LIMIT: usize = 64 * 1024;

#[derive(Clone, Debug)]
pub struct TerminationConfig {
    pub grace_period: Duration,
}

#[derive(Clone, Debug)]
pub struct ComposeProcessRunner {
    pub termination: TerminationConfig,
}

impl ComposeProcessRunner {
    pub fn new(termination: TerminationConfig) -> Self {
        Self { termination }
    }
    pub fn default() -> Self {
        Self::new(TerminationConfig {
            grace_period: Duration::from_secs(5),
        })
    }
    pub async fn run(
        &self,
        invocation: ComposeInvocation,
    ) -> Result<ComposeProcessResult, AppError> {
        self.execute(invocation).await
    }

    async fn execute(
        &self,
        invocation: ComposeInvocation,
    ) -> Result<ComposeProcessResult, AppError> {
        #[cfg(not(unix))]
        return Err(AppError::new(
            AppErrorCode::ComposeFailed,
            "compose_spawn",
            None,
            "process groups unsupported on this target",
        ));

        let started = Instant::now();
        let mut command = Command::new(&invocation.executable);
        command
            .args(&invocation.args)
            .current_dir(&invocation.working_directory)
            .env_clear()
            .envs(&invocation.environment)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        unsafe {
            command.pre_exec(|| {
                if libc::setpgid(0, 0) != 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child = command.spawn().map_err(|e| {
            AppError::new(
                AppErrorCode::ComposeFailed,
                "compose_spawn",
                None,
                "failed to spawn compose",
            )
            .with_details(e.to_string())
        })?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let out_task = tokio::spawn(drain(stdout));
        let err_task = tokio::spawn(drain(stderr));
        let mut timed_out = false;
        let status = tokio::time::sleep_until(tokio::time::Instant::from_std(invocation.deadline));
        tokio::pin!(status);
        let lifecycle: Result<(), AppError> = tokio::select! {
            result = child.wait() => { result.map(|_| ()).map_err(|e| wait_error(e, false)) }
            _ = &mut status => {
                timed_out = true;
                terminate_group(&mut child, self.termination.grace_period).await
            }
        };
        let output = out_task
            .await
            .map_err(|e| drain_error(e.to_string()))
            .and_then(|result| result.map_err(|e| drain_error(e.to_string())));
        let error = err_task
            .await
            .map_err(|e| drain_error(e.to_string()))
            .and_then(|result| result.map_err(|e| drain_error(e.to_string())));
        lifecycle?;
        let output = output?;
        let error = error?;
        let duration = started.elapsed();
        if timed_out {
            Ok(ComposeProcessResult::from_timeout(
                decode(output.0),
                decode(error.0),
                output.1,
                error.1,
                duration,
            ))
        } else {
            let code = child
                .try_wait()
                .map_err(|e| wait_error(e, false))?
                .and_then(|s| s.code())
                .unwrap_or(-1);
            if code != 0 {
                return Err(AppError::new(
                    AppErrorCode::ComposeFailed,
                    "compose_run",
                    None,
                    "compose exited unsuccessfully",
                )
                .with_details(format!("exit code {code}")));
            }
            Ok(ComposeProcessResult {
                exit_code: Some(code),
                stdout: decode(output.0),
                stderr: decode(error.0),
                stdout_truncated: output.1,
                stderr_truncated: error.1,
                timed_out: false,
                duration,
            })
        }
    }
}

impl Default for ComposeProcessRunner {
    fn default() -> Self {
        Self::default()
    }
}
impl ComposeRunner for ComposeProcessRunner {
    fn invoke(&self, invocation: ComposeInvocation) -> RuntimeFuture<'_, ComposeProcessResult> {
        Box::pin(self.execute(invocation))
    }
}

fn decode(bytes: Vec<u8>) -> String {
    String::from_utf8_lossy(&bytes).into_owned()
}
fn drain_error(message: String) -> AppError {
    AppError::new(
        AppErrorCode::ComposeFailed,
        "compose_output",
        None,
        "failed to drain compose output",
    )
    .with_details(message)
}
fn wait_error(error: io::Error, timeout: bool) -> AppError {
    AppError::new(
        if timeout {
            AppErrorCode::OperationTimeout
        } else {
            AppErrorCode::ComposeFailed
        },
        "compose_wait",
        None,
        "failed to wait for compose",
    )
    .with_details(error.to_string())
}

async fn drain<R: AsyncRead + Unpin>(mut reader: R) -> io::Result<(Vec<u8>, bool)> {
    let mut ring = Vec::with_capacity(LIMIT);
    let mut buf = [0u8; 8192];
    let mut truncated = false;
    loop {
        let count = reader.read(&mut buf).await?;
        if count == 0 {
            break;
        }
        if count >= LIMIT {
            ring.clear();
            ring.extend_from_slice(&buf[count - LIMIT..count]);
            truncated = true;
            continue;
        }
        let overflow = ring.len() + count > LIMIT;
        if overflow {
            let remove = ring.len() + count - LIMIT;
            ring.drain(..remove);
            truncated = true;
        }
        ring.extend_from_slice(&buf[..count]);
    }
    Ok((ring, truncated))
}

#[cfg(unix)]
async fn terminate_group(child: &mut Child, grace: Duration) -> Result<(), AppError> {
    let mut diagnostic = None;
    if let Err(error) = signal_group(child, libc::SIGTERM) {
        diagnostic = Some(error);
    }
    let grace_result = tokio::time::timeout(grace, child.wait()).await;
    if let Ok(Err(error)) = grace_result {
        diagnostic = Some(wait_error(error, true));
    }
    if let Err(error) = signal_group(child, libc::SIGKILL) {
        diagnostic.get_or_insert(error);
    }
    let reap_deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < reap_deadline => {
                tokio::time::sleep(Duration::from_millis(10)).await
            }
            Ok(None) => {
                child
                    .wait()
                    .await
                    .map_err(|error| wait_error(error, true))?;
                break;
            }
            Err(_error) => {
                let diagnostic_error = wait_error(_error, true);
                let fallback = child.wait().await.map_err(|error| wait_error(error, true));
                diagnostic.get_or_insert(diagnostic_error);
                if let Err(error) = fallback {
                    diagnostic.get_or_insert(error);
                }
                break;
            }
        }
    }
    diagnostic.map_or(Ok(()), Err)
}

#[cfg(not(unix))]
async fn terminate_group(_child: &mut Child, _grace: Duration) -> Result<(), AppError> {
    Err(AppError::new(
        AppErrorCode::ComposeFailed,
        "compose_terminate",
        None,
        "process groups unsupported on this target",
    ))
}

#[cfg(unix)]
fn signal_group(child: &Child, signal: i32) -> Result<(), AppError> {
    let Some(pid) = child.id() else {
        return Ok(());
    };
    let pid = pid as i32;
    let result = unsafe { libc::kill(-pid, signal) };
    if result == 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        return Ok(());
    }
    Err(AppError::new(
        AppErrorCode::OperationTimeout,
        "compose_terminate",
        None,
        "failed to signal compose process group",
    )
    .with_details(error.to_string()))
}
