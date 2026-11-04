use std::io::{BufReader, Read};
use std::os::windows::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

use base64::Engine;
use wait_timeout::ChildExt;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const PRELUDE: &str = concat!(
    "$ProgressPreference='SilentlyContinue'; ",
    "$ErrorActionPreference='Continue'; ",
    "try { [Console]::OutputEncoding = New-Object System.Text.UTF8Encoding $false } catch {}; ",
    "try { $OutputEncoding = [System.Text.Encoding]::UTF8 } catch {}"
);

#[allow(dead_code)]
pub struct PsOutput {
    pub stdout: String,
    pub stderr: String,
    pub code: Option<i32>,
    pub timed_out: bool,
}

fn encoded(script: &str) -> String {
    let full = format!("{PRELUDE}; {script}");
    let utf16: Vec<u16> = full.encode_utf16().collect();
    let mut bytes = Vec::with_capacity(utf16.len() * 2);
    for unit in utf16 {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

pub fn spawn(script: &str) -> std::io::Result<Child> {
    Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-EncodedCommand",
            &encoded(script),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
}

/// Runs a PowerShell script and captures its output, killing it on timeout.
pub fn run(script: &str, timeout: Duration) -> PsOutput {
    let mut child = match spawn(script) {
        Ok(child) => child,
        Err(error) => {
            return PsOutput {
                stdout: String::new(),
                stderr: error.to_string(),
                code: None,
                timed_out: false,
            }
        }
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let out_handle = thread::spawn(move || {
        let mut buffer = String::new();
        if let Some(stream) = stdout {
            let mut reader = BufReader::new(stream);
            let _ = reader.read_to_string(&mut buffer);
        }
        buffer
    });
    let err_handle = thread::spawn(move || {
        let mut buffer = String::new();
        if let Some(stream) = stderr {
            let mut reader = BufReader::new(stream);
            let _ = reader.read_to_string(&mut buffer);
        }
        buffer
    });

    let mut timed_out = false;
    let status = match child.wait_timeout(timeout) {
        Ok(Some(status)) => Some(status),
        Ok(None) => {
            timed_out = true;
            let _ = child.kill();
            child.wait().ok()
        }
        Err(_) => None,
    };

    let result = PsOutput {
        stdout: out_handle.join().unwrap_or_default(),
        stderr: err_handle.join().unwrap_or_default(),
        code: status.and_then(|status| status.code()),
        timed_out,
    };

    if result.timed_out {
        crate::logging::log("powershell timed out");
    } else if result.code.unwrap_or(0) != 0 && !result.stderr.trim().is_empty() {
        let message = result.stderr.trim();
        let short: String = message.chars().take(300).collect();
        crate::logging::log(&format!("powershell failed: {short}"));
    }

    result
}


