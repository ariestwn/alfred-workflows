//! A short-lived native HUD process. Only status messages cross the private
//! state file; selected text and provider output never go to the HUD.
use crate::{prompt::Action, Result};
use serde_json::{json, Value};
use std::{
    env, fs,
    os::unix::{
        fs::{MetadataExt, PermissionsExt},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[cfg(target_os = "macos")]
#[path = "hud_mac.rs"]
mod mac;

const PREFIX: &str = "quickai-hud-";

#[derive(Clone, Debug, PartialEq)]
pub struct Notice {
    pub phase: &'static str,
    pub title: String,
    pub detail: String,
}

impl Notice {
    fn from_value(value: &Value) -> Result<Self> {
        let phase = match value["phase"].as_str() {
            Some("working") => "working",
            Some("success") => "success",
            Some("error") => "error",
            _ => return Err("Invalid HUD status.".into()),
        };
        Ok(Self {
            phase,
            title: value["title"]
                .as_str()
                .unwrap_or("QuickAI")
                .chars()
                .take(80)
                .collect(),
            detail: value["detail"]
                .as_str()
                .unwrap_or_default()
                .chars()
                .take(220)
                .collect(),
        })
    }

    fn value(&self, owner: u32, expires: u64) -> Value {
        json!({"phase":self.phase, "title":self.title, "detail":self.detail,
            "owner":owner, "expires":expires})
    }

    pub fn stopped() -> Self {
        Self {
            phase: "error",
            title: "QuickAI stopped".into(),
            detail: "The request was interrupted. Run the command again.".into(),
        }
    }
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn mode() -> String {
    env::var("QUICKAI_FEEDBACK").unwrap_or_else(|_| "hud".into())
}

fn valid_directory(path: &Path) -> bool {
    path.parent() == Some(env::temp_dir().as_path())
        && path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with(PREFIX))
        && fs::symlink_metadata(path).is_ok_and(|m| {
            m.is_dir() && m.uid() == unsafe { libc::geteuid() } && m.mode() & 0o077 == 0
        })
}

fn write_state(directory: &Path, value: &Value) -> Result<()> {
    if !valid_directory(directory) {
        return Err("Invalid HUD directory.".into());
    }
    let mut file = tempfile::NamedTempFile::new_in(directory).map_err(|e| e.to_string())?;
    serde_json::to_writer(&mut file, value).map_err(|e| e.to_string())?;
    file.persist(directory.join("state.json"))
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn read_state(directory: &Path) -> Result<Value> {
    let file = fs::read(directory.join("state.json")).map_err(|e| e.to_string())?;
    if file.len() > 8192 {
        return Err("Invalid HUD state size.".into());
    }
    serde_json::from_slice(&file).map_err(|e| e.to_string())
}

fn alive(pid: u32) -> bool {
    pid > 0 && pid <= i32::MAX as u32 && unsafe { libc::kill(pid as i32, 0) == 0 }
}

fn helper_alive(directory: &Path) -> bool {
    fs::read_to_string(directory.join("ready"))
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .is_some_and(alive)
}

fn launch(notice: &Notice, owner: u32, expires: u64) -> Result<PathBuf> {
    // Clean up files left only by killed/crashed helpers, never active sessions.
    if let Ok(entries) = fs::read_dir(env::temp_dir()) {
        for entry in entries.flatten() {
            let path = entry.path();
            if valid_directory(&path)
                && !helper_alive(&path)
                && entry
                    .metadata()
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.elapsed().ok())
                    .is_some_and(|t| t > Duration::from_secs(900))
            {
                let _ = fs::remove_dir_all(path);
            }
        }
    }
    let directory = tempfile::Builder::new()
        .prefix(PREFIX)
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .map_err(|e| e.to_string())?;
    write_state(directory.path(), &notice.value(owner, expires))?;
    let mut child = Command::new(env::current_exe().map_err(|e| e.to_string())?)
        .arg("--hud")
        .arg(directory.path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|e| e.to_string())?;
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(1500) {
        if helper_alive(directory.path()) {
            let path = directory.keep();
            // Reap if this launcher lives long enough; short Alfred helpers exit
            // normally, at which point launchd adopts the HUD process.
            thread::spawn(move || {
                let _ = child.wait();
            });
            return Ok(path);
        }
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            return Err("HUD could not open a window.".into());
        }
        thread::sleep(Duration::from_millis(25));
    }
    let _ = child.kill();
    let _ = child.wait();
    Err("HUD startup timed out.".into())
}

pub fn start(action: &str) -> Option<PathBuf> {
    if !matches!(mode().as_str(), "" | "hud") {
        return None;
    }
    let title = match Action::parse(action).ok() {
        Some(Action::Quickfix) => "Fixing text…",
        Some(Action::ImproveWriting) => "Improving writing…",
        Some(Action::GrammarCheck) => "Checking grammar…",
        None => "Working…",
    };
    let timeout = env::var("QUICKAI_TIMEOUT")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(120)
        .clamp(1, 600);
    match launch(
        &Notice {
            phase: "working",
            title: title.into(),
            detail: "QuickAI".into(),
        },
        std::process::id(),
        now_ms() + (timeout + 15) * 1000,
    ) {
        Ok(path) => Some(path),
        Err(error) => {
            eprintln!("QuickAI HUD: {error}");
            None
        }
    }
}

/// The Rust request process ends before Alfred verifies and pastes. Give that
/// handoff a bounded grace period instead of treating the parent exit as a crash.
pub fn handoff(directory: &Path) {
    if let Ok(mut state) = read_state(directory) {
        state["owner"] = json!(0);
        state["expires"] = json!(now_ms() + 15_000);
        let _ = write_state(directory, &state);
    }
}

pub fn completion(ok: bool, output: &str, message: &str) -> Notice {
    if !ok {
        return Notice {
            phase: "error",
            title: "QuickAI couldn’t finish".into(),
            detail: message
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(220)
                .collect(),
        };
    }
    Notice {
        phase: "success",
        title: "QuickAI · Done".into(),
        detail: match output {
            "copy" => "Text copied. Paste with ⌘V.",
            "both" => "Paste sent. Text also saved to the clipboard.",
            _ => "Paste sent to your app.",
        }
        .into(),
    }
}

/// Returns true only when Alfred should display its native notification fallback.
pub fn finish() -> bool {
    match mode().as_str() {
        "off" => return false,
        "notification" => return true,
        _ => {}
    }
    let notice = completion(
        env::var("quickai_ok").as_deref() == Ok("1"),
        &env::var("quickai_output").unwrap_or_default(),
        &env::var("quickai_message").unwrap_or_else(|_| "Please try again.".into()),
    );
    let state = notice.value(0, now_ms() + 10_000);
    if let Ok(path) = env::var("quickai_hud") {
        let directory = Path::new(&path);
        if valid_directory(directory)
            && helper_alive(directory)
            && write_state(directory, &state).is_ok()
        {
            return false;
        }
    }
    // Includes failures that happen before capture has produced any input.
    if let Err(error) = launch(&notice, 0, now_ms() + 10_000) {
        eprintln!("QuickAI HUD: {error}");
        return true;
    }
    false
}

pub fn serve(directory: &Path) -> Result<()> {
    if !valid_directory(directory) {
        return Err("Invalid HUD directory.".into());
    }
    #[cfg(target_os = "macos")]
    let result = mac::show(directory);
    #[cfg(not(target_os = "macos"))]
    let result = Err("HUD requires macOS.".into());
    let _ = fs::remove_dir_all(directory);
    result
}

pub fn interrupted(value: &Value, now: u64) -> bool {
    value["phase"] == "working"
        && (now >= value["expires"].as_u64().unwrap_or(0)
            || value["owner"]
                .as_u64()
                .is_some_and(|p| p != 0 && !alive(p as u32)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn completion_reports_copy_fallback_and_errors_without_pretending_to_paste() {
        assert!(completion(true, "copy", "").detail.contains("⌘V"));
        assert_eq!(completion(false, "paste", "Not logged in").phase, "error");
        assert_eq!(
            completion(false, "paste", "Not logged in").detail,
            "Not logged in"
        );
    }
    #[test]
    fn handoff_has_bounded_lifetime_and_atomic_state_updates() {
        let directory = tempfile::Builder::new()
            .prefix(PREFIX)
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let notice = Notice {
            phase: "working",
            title: "Working…".into(),
            detail: "QuickAI".into(),
        };
        write_state(
            directory.path(),
            &notice.value(std::process::id(), now_ms() + 5000),
        )
        .unwrap();
        handoff(directory.path());
        let state = read_state(directory.path()).unwrap();
        assert_eq!(state["owner"], 0);
        assert!(!interrupted(&state, now_ms()));
        assert!(interrupted(&state, now_ms() + 16_000));
        let final_state = completion(true, "copy", "").value(0, now_ms());
        write_state(directory.path(), &final_state).unwrap();
        assert_eq!(
            Notice::from_value(&read_state(directory.path()).unwrap())
                .unwrap()
                .phase,
            "success"
        );
        assert!(!interrupted(&final_state, now_ms() + 16_000));
    }
    #[test]
    fn rejects_paths_outside_private_hud_directories() {
        let directory = tempfile::tempdir().unwrap();
        assert!(write_state(directory.path(), &json!({})).is_err());
        assert!(serve(Path::new("/tmp")).is_err());
    }
}
