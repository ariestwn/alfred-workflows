use crate::{
    config::{self, Config, Provider},
    prompt, Result,
};
use serde_json::Value;
use std::{
    fs::{self, File},
    io::{Read, Write},
    os::unix::process::CommandExt,
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const MAX_OUTPUT_BYTES: u64 = 2 * 1024 * 1024;
const OPENCODE_AGENT: &str = "quickai";

pub fn command(config: &Config, dir: &Path, system: &str) -> Result<Command> {
    let mut cmd = Command::new(&config.executable);
    cmd.current_dir(dir)
        .env("PATH", config::search_path()?)
        .env("NO_COLOR", "1")
        // OpenCode resolves its project and config from PWD, which Command does not update.
        .env("PWD", dir);
    match config.provider {
        Provider::Codex => {
            cmd.args([
                "exec",
                "--skip-git-repo-check",
                "--ephemeral",
                "--ignore-user-config",
                "--sandbox",
                "read-only",
                "--color",
                "never",
                "-c",
                "approval_policy=\"never\"",
                "-c",
                "features.shell_tool=false",
                "-c",
                "web_search=\"disabled\"",
                "-c",
                "project_doc_max_bytes=0",
            ]);
            cmd.arg("-c").arg(format!(
                "developer_instructions={}",
                serde_json::to_string(system).unwrap()
            ));
            cmd.arg("--output-last-message").arg(dir.join("answer.txt"));
            if !config.model.is_empty() {
                cmd.arg("--model").arg(&config.model);
            }
            if !config.effort.is_empty() {
                cmd.arg("-c")
                    .arg(format!("model_reasoning_effort=\"{}\"", config.effort));
            }
            cmd.arg("-");
        }
        Provider::Claude => {
            cmd.args([
                "--print",
                "--output-format",
                "json",
                "--no-session-persistence",
                "--tools",
                "",
                "--strict-mcp-config",
                "--mcp-config",
                "{\"mcpServers\":{}}",
                "--disable-slash-commands",
                "--permission-mode",
                "dontAsk",
                "--settings",
                "{\"disableAllHooks\":true}",
                "--system-prompt",
                system,
            ]);
            if !config.model.is_empty() {
                cmd.arg("--model").arg(&config.model);
            }
            if !config.effort.is_empty() {
                cmd.arg("--effort").arg(&config.effort);
            }
        }
        Provider::OpenCode => {
            cmd.args(["run", "--format", "json", "--agent", OPENCODE_AGENT]);
            if !config.model.is_empty() {
                cmd.arg("--model").arg(&config.model);
            }
        }
    }
    // A separate group lets timeout/error cleanup also stop child processes.
    cmd.process_group(0);
    Ok(cmd)
}

struct ProcessGroup(Child);
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        // SAFETY: the child was spawned into its own process group with PGID = child PID.
        unsafe {
            libc::kill(-(self.0.id() as i32), libc::SIGKILL);
        }
        let _ = self.0.wait();
    }
}

pub fn transform(config: &Config, system: &str, input: &str) -> Result<String> {
    let dir = tempfile::Builder::new()
        .prefix("alfred-quickai-")
        .tempdir()
        .map_err(|e| format!("Cannot create temporary working directory: {e}"))?;
    // OpenCode treats each working directory as a project, so run it from a stable project
    // directory keyed by the system prompt. The per-request temp directory still holds the
    // input, output, and diagnostics.
    let cwd = match config.provider {
        Provider::OpenCode => {
            let project = config
                .opencode_dir
                .join(format!("{:016x}", stable_hash(system)));
            fs::create_dir_all(&project).map_err(io_error)?;
            fs::write(project.join("opencode.json"), opencode_settings(system)).map_err(io_error)?;
            project
        }
        _ => dir.path().to_path_buf(),
    };
    // Files avoid pipe deadlocks, even when a CLI emits lots of diagnostics or never reads stdin.
    let input_path = dir.path().join("input.txt");
    let mut input_file = File::create(&input_path).map_err(io_error)?;
    match config.provider {
        Provider::Codex => write!(
            input_file,
            "{system}\n\nEdit this input text:\n{}",
            serde_json::to_string(input).unwrap()
        ),
        Provider::Claude | Provider::OpenCode => write!(
            input_file,
            "Edit this input text (JSON string):\n{}",
            serde_json::to_string(input).unwrap()
        ),
    }
    .map_err(io_error)?;
    drop(input_file);
    let stdout_path = dir.path().join("stdout.txt");
    let stderr_path = dir.path().join("stderr.txt");
    let mut cmd = command(config, &cwd, system)?;
    cmd.stdin(File::open(input_path).map_err(io_error)?)
        .stdout(Stdio::from(File::create(&stdout_path).map_err(io_error)?))
        .stderr(Stdio::from(File::create(&stderr_path).map_err(io_error)?));
    let mut child = ProcessGroup(cmd.spawn().map_err(|e| format!(
        "Cannot start {}: {e}. Install/sign in to the CLI, or set its path in Configure Workflow.", config.provider.name()))?);
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.0.try_wait().map_err(io_error)? {
            break status;
        }
        if started.elapsed() >= config.timeout {
            return Err(format!(
                "{} timed out after {} seconds. Retry or increase Timeout in Configure Workflow.",
                config.provider.name(),
                config.timeout.as_secs()
            ));
        }
        for path in [&stdout_path, &stderr_path, &cwd.join("answer.txt")] {
            if fs::metadata(path)
                .map(|m| m.len() > MAX_OUTPUT_BYTES)
                .unwrap_or(false)
            {
                return Err("CLI output exceeded the 2 MB limit.".into());
            }
        }
        thread::sleep(Duration::from_millis(40));
    };
    if !status.success() {
        let stderr = read_limited(&stderr_path).unwrap_or_default();
        let stdout = read_limited(&stdout_path).unwrap_or_default();
        match config.provider {
            Provider::Claude => {
                for stream in [&stdout, &stderr] {
                    if serde_json::from_str::<Value>(stream).is_ok() {
                        claude_result(stream)?;
                    }
                }
            }
            Provider::OpenCode => {
                let parsed = opencode_parse(&stdout);
                discard_session(config, &cwd, &parsed.session);
                if !parsed.error.trim().is_empty() {
                    return Err(format!("OpenCode: {}", concise(&parsed.error)));
                }
            }
            Provider::Codex => {}
        }
        let detail = if stderr.trim().is_empty() {
            &stdout
        } else {
            &stderr
        };
        return Err(format!(
            "{} failed ({status}): {}",
            config.provider.name(),
            concise(detail)
        ));
    }
    let raw = match config.provider {
        Provider::Codex => read_limited(&cwd.join("answer.txt")).map_err(|_| {
            "Codex did not write a final answer. Check CLI authentication and model settings."
                .to_owned()
        })?,
        Provider::Claude => claude_result(&read_limited(&stdout_path)?)?,
        Provider::OpenCode => {
            let parsed = opencode_parse(&read_limited(&stdout_path)?);
            discard_session(config, &cwd, &parsed.session);
            if !parsed.error.trim().is_empty() {
                return Err(format!("OpenCode: {}", concise(&parsed.error)));
            }
            if parsed.text.trim().is_empty() {
                return Err(
                    "OpenCode returned no final text. Check CLI authentication and model settings."
                        .into(),
                );
            }
            parsed.text
        }
    };
    prompt::clean_output(&raw, input)
}

fn io_error(e: std::io::Error) -> String {
    format!("CLI I/O error: {e}")
}

fn read_limited(path: &Path) -> Result<String> {
    let file = File::open(path).map_err(io_error)?;
    let mut bytes = Vec::new();
    file.take(MAX_OUTPUT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() as u64 > MAX_OUTPUT_BYTES {
        return Err("CLI output exceeded the 2 MB limit.".into());
    }
    String::from_utf8(bytes).map_err(|_| "CLI returned text that is not valid UTF-8.".into())
}

fn concise(text: &str) -> String {
    let text: String = text
        .chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .take(600)
        .collect();
    if text.trim().is_empty() {
        "No details returned; check CLI login and model availability.".into()
    } else {
        text.trim().to_owned()
    }
}

pub struct OpenCodeOutput {
    pub text: String,
    pub error: String,
    pub session: String,
}

/// Parse `opencode run --format json` newline-delimited events. Only the final step's
/// text becomes the rewrite; reasoning parts and diagnostics are ignored.
pub fn opencode_parse(raw: &str) -> OpenCodeOutput {
    let mut out = OpenCodeOutput {
        text: String::new(),
        error: String::new(),
        session: String::new(),
    };
    for line in raw.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if let Some(id) = value.get("sessionID").and_then(Value::as_str) {
            if !id.is_empty() {
                out.session = id.to_owned();
            }
        }
        match value.get("type").and_then(Value::as_str) {
            Some("step_start") => out.text.clear(),
            Some("text") => {
                let part = value.get("part");
                if part.and_then(|p| p.get("type")).and_then(Value::as_str) == Some("text") {
                    if let Some(text) = part.and_then(|p| p.get("text")).and_then(Value::as_str) {
                        out.text.push_str(text);
                    }
                }
            }
            Some("error") => {
                out.error = value
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .unwrap_or("OpenCode request failed.")
                    .to_owned();
            }
            _ => {}
        }
    }
    out
}

/// A private OpenCode config: the agent supplies the instructions and every tool is denied,
/// so the model only rewrites the supplied text.
fn opencode_settings(system: &str) -> String {
    let deny = serde_json::json!({"action": "*", "resource": "*", "effect": "deny"});
    serde_json::json!({
        "$schema": "https://opencode.ai/config.json",
        "permissions": [deny.clone()],
        "agents": {
            (OPENCODE_AGENT): {
                "mode": "primary",
                "description": "QuickAI editor",
                "system": system,
                "permissions": [deny],
            }
        }
    })
    .to_string()
}

/// Best effort: edited text should not linger in OpenCode's session history.
fn discard_session(config: &Config, dir: &Path, session: &str) {
    if session.is_empty() {
        return;
    }
    let mut cmd = Command::new(&config.executable);
    cmd.args(["session", "delete", session])
        .current_dir(dir)
        .env("PWD", dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Ok(path) = config::search_path() {
        cmd.env("PATH", path);
    }
    let Ok(mut child) = cmd.spawn() else {
        return;
    };
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) | Err(_) => break,
            Ok(None) => {}
        }
        if started.elapsed() >= Duration::from_secs(5) {
            let _ = child.kill();
            let _ = child.wait();
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
}

/// A deterministic FNV-1a hash so the same prompt always maps to the same project directory.
fn stable_hash(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

pub fn claude_result(raw: &str) -> Result<String> {
    let value: Value = serde_json::from_str(raw).map_err(|_| {
        "Claude returned invalid JSON. Update Claude Code and try again.".to_owned()
    })?;
    if value.get("is_error").and_then(Value::as_bool) == Some(true)
        || value
            .get("subtype")
            .and_then(Value::as_str)
            .is_some_and(|s| s != "success")
    {
        let detail = value
            .get("result")
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
            .map(str::to_owned)
            .or_else(|| value.get("errors").map(Value::to_string))
            .or_else(|| value.get("error").map(Value::to_string))
            .or_else(|| {
                value
                    .get("terminal_reason")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| "Request failed".into());
        return Err(format!("Claude: {}", concise(&detail)));
    }
    value
        .get("result")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| "Claude returned no final text.".into())
}
