//! Bounded execution of an executable selected by the trusted host.
use crate::process::Process;
use std::{
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const OUTPUT_LIMIT: usize = 8192;

pub struct ToolOutput {
    pub exit_code: u32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

fn capture(pipe: impl Read) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    pipe.take((OUTPUT_LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("PROCESS_OUTPUT_FAILED: {error}"))?;
    if bytes.len() > OUTPUT_LIMIT {
        return Err("PROCESS_OUTPUT_LIMIT".into());
    }
    Ok(bytes)
}

#[allow(clippy::too_many_arguments)] // Explicit execution inputs; no shared mutable runner configuration.
pub fn run(
    executable: &Path,
    directory: &Path,
    temporary: &Path,
    environment: &std::collections::HashMap<String, String>,
    args: &[String],
    stdin: &[u8],
    deadline: Instant,
    check: impl Fn() -> Result<(), String>,
) -> Result<ToolOutput, String> {
    check()?;
    let mut command = Command::new(executable);
    command
        .args(args)
        .current_dir(directory)
        .env_clear()
        .envs(environment)
        .env("TEMP", temporary)
        .env("TMP", temporary)
        .env("TMPDIR", temporary)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(system_root) = std::env::var_os("SystemRoot") {
        command.env("SystemRoot", system_root);
    }
    let mut process = Process::spawn(&mut command)?;
    let mut input = process.child.stdin.take().expect("piped stdin");
    let stdout = process.child.stdout.take().expect("piped stdout");
    let stderr = process.child.stderr.take().expect("piped stderr");
    thread::scope(move |scope| {
        let mut process = process;
        let input = scope.spawn(move || input.write_all(stdin));
        let stdout = scope.spawn(|| capture(stdout));
        let stderr = scope.spawn(|| capture(stderr));
        let status = loop {
            if let Err(error) = check() {
                break Err(error);
            }
            if Instant::now() >= deadline {
                break Err("PROCESS_TIMEOUT".into());
            }
            if let Some(status) = process
                .child
                .try_wait()
                .map_err(|error| format!("PROCESS_WAIT_FAILED: {error}"))
                .transpose()
            {
                break status;
            }
            thread::sleep(Duration::from_millis(25));
        };
        // Stop descendants holding pipe handles even when the direct child exited.
        process.force_stop()?;
        let status = status?;
        input
            .join()
            .expect("stdin writer")
            .map_err(|error| format!("PROCESS_INPUT_FAILED: {error}"))?;
        Ok(ToolOutput {
            exit_code: status.code().expect("Windows exit code") as u32,
            stdout: stdout.join().expect("stdout reader")?,
            stderr: stderr.join().expect("stderr reader")?,
        })
    })
}
