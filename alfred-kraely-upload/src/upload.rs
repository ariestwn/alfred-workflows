use crate::{config::Config, Result};
use std::{
    fs::{self, File},
    io::Read,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const MAX_DIAGNOSTICS: u64 = 256 * 1024;

#[derive(Debug)]
pub struct Upload {
    pub local: PathBuf,
    pub remote: String,
}

impl Upload {
    pub fn prepare(config: &Config, selected: &Path) -> Result<Self> {
        let filename = selected
            .file_name()
            .and_then(|s| s.to_str())
            .filter(|s| !s.is_empty() && !s.chars().any(char::is_control))
            .ok_or(
                "Select one file with a valid filename (no line breaks or control characters).",
            )?;
        let metadata =
            fs::metadata(selected).map_err(|e| format!("Cannot read selected file: {e}"))?;
        if !metadata.is_file() {
            return Err(
                "Select a single file. Folders and other special files cannot be uploaded.".into(),
            );
        }
        if metadata.len() > config.max_bytes {
            return Err(format!(
                "File is too large: {:.2} MB (limit {:.2} MB).",
                mb(metadata.len()),
                mb(config.max_bytes)
            ));
        }
        // Absolute source paths cannot be mistaken for scp options or remote hosts.
        let local = std::path::absolute(selected)
            .map_err(|e| format!("Cannot resolve selected file: {e}"))?;
        File::open(&local).map_err(|e| format!("Cannot open selected file: {e}"))?;
        Ok(Self {
            local,
            remote: format!("{}/{}", config.directory.trim_end_matches('/'), filename),
        })
    }
}

fn mb(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

pub fn execute(config: &Config, upload: &Upload) -> Result<()> {
    let executable = PathBuf::from("/usr/bin/scp");
    // Tests can simulate uploads; distributed release builds always use Apple's scp.
    #[cfg(debug_assertions)]
    let executable = std::env::var_os("KRAELY_TEST_SCP")
        .map(PathBuf::from)
        .unwrap_or(executable);
    execute_with(config, upload, &executable)
}

pub fn command(config: &Config, upload: &Upload, executable: &Path) -> Command {
    let mut cmd = Command::new(executable);
    // OpenSSH 9+ uses SFTP: remote names are literal protocol paths, not shell code.
    // Keep host verification on and never prompt from a background Alfred action.
    cmd.args([
        "-o",
        "BatchMode=yes",
        "-o",
        "ConnectTimeout=10",
        "-o",
        "StrictHostKeyChecking=yes",
        "--",
    ])
    .arg(&upload.local)
    // Address the existing folder, so a same-named remote directory fails
    // instead of silently receiving a nested file with the wrong copied path.
    .arg(format!(
        "{}:{}/",
        config.host,
        config.directory.trim_end_matches('/')
    ))
    .env("SSH_ASKPASS_REQUIRE", "never")
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .process_group(0);
    cmd
}

struct ProcessGroup(Child);
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        // This child owns a separate group, including its ssh subprocess.
        unsafe {
            libc::kill(-(self.0.id() as i32), libc::SIGKILL);
        }
        let _ = self.0.wait();
    }
}

pub fn execute_with(config: &Config, upload: &Upload, executable: &Path) -> Result<()> {
    let diagnostics =
        tempfile::NamedTempFile::new().map_err(|e| format!("Cannot create upload log: {e}"))?;
    let mut cmd = command(config, upload, executable);
    cmd.stderr(diagnostics.reopen().map_err(|e| e.to_string())?);
    let mut child = ProcessGroup(cmd.spawn().map_err(|e| format!("Cannot start scp: {e}"))?);
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.0.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        if started.elapsed() >= config.timeout {
            return Err(format!(
                "Upload timed out after {} seconds. Check the connection or increase Timeout.",
                config.timeout.as_secs()
            ));
        }
        if diagnostics
            .as_file()
            .metadata()
            .is_ok_and(|m| m.len() > MAX_DIAGNOSTICS)
        {
            return Err("Upload stopped: SSH returned too many diagnostic messages.".into());
        }
        thread::sleep(Duration::from_millis(30));
    };
    if status.success() {
        return Ok(());
    }
    let mut bytes = Vec::new();
    diagnostics
        .reopen()
        .map_err(|e| e.to_string())?
        .take(MAX_DIAGNOSTICS)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    Err(failure_message(&String::from_utf8_lossy(&bytes)))
}

fn failure_message(stderr: &str) -> String {
    let lower = stderr.to_lowercase();
    if lower.contains("could not resolve hostname") {
        return "SSH host was not found. Check SSH host in Configure Workflow and ~/.ssh/config."
            .into();
    }
    if lower.contains("host key verification failed")
        || lower.contains("remote host identification has changed")
    {
        return "SSH host verification failed. Connect from Terminal and verify this server’s host key first.".into();
    }
    if lower.contains("permission denied") && lower.contains("publickey") {
        return "SSH authentication failed. Make sure key-based login works without prompting in Terminal.".into();
    }
    let detail: String = stderr
        .lines()
        .map(str::trim)
        .find(|s| !s.is_empty())
        .unwrap_or("Check the SSH connection and that the remote folder exists and is writable.")
        .chars()
        .filter(|c| !c.is_control())
        .take(220)
        .collect();
    format!("Upload failed: {detail}")
}
