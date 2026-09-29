use crate::{models, text, Result};
use serde_json::{json, Value};
use std::{
    env,
    fs::{self, OpenOptions},
    io::Write,
    os::fd::AsRawFd,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub struct Paths {
    pub data: PathBuf,
    pub hub: PathBuf,
    pub binary: PathBuf,
    pub trash: PathBuf,
}
impl Paths {
    pub fn from_env() -> Result<Self> {
        let home = PathBuf::from(env::var_os("HOME").ok_or("HOME is not set")?);
        let expand = |s: String| {
            if let Some(rest) = s.strip_prefix("~/") {
                home.join(rest)
            } else {
                PathBuf::from(s)
            }
        };
        let setting = |key: &str| env::var(key).ok().filter(|s| !s.trim().is_empty());
        Ok(Self {
            data: setting("HANDY_DATA_DIR")
                .map(&expand)
                .unwrap_or(home.join("Library/Application Support/com.pais.handy")),
            hub: setting("HANDY_HF_CACHE")
                .or_else(|| setting("HF_HUB_CACHE"))
                .map(&expand)
                .or_else(|| setting("HF_HOME").map(|p| expand(p).join("hub")))
                .unwrap_or(home.join(".cache/huggingface/hub")),
            binary: setting("HANDY_BINARY")
                .map(&expand)
                .unwrap_or(PathBuf::from(
                    "/Applications/Handy.app/Contents/MacOS/handy",
                )),
            trash: home.join(".Trash"),
        })
    }
    pub fn db(&self) -> PathBuf {
        self.data.join("history.db")
    }
    pub fn read_store(&self) -> Result<Value> {
        let path = self.data.join("settings_store.json");
        if !path.is_file() {
            return Err("Handy settings were not found. Open Handy and finish setup, or set its data folder in Workflow Configuration.".into());
        }
        let store: Value = serde_json::from_slice(&fs::read(path)?)?;
        if !store["settings"].is_object() {
            return Err(
                "Handy's settings file has an unsupported format; it was left unchanged.".into(),
            );
        }
        Ok(store)
    }
    pub fn recording(&self, entry: &Value) -> Result<PathBuf> {
        let name = text(entry, "file_name");
        let path = Path::new(name);
        if name.is_empty()
            || path.components().count() != 1
            || !matches!(
                path.components().next(),
                Some(std::path::Component::Normal(_))
            )
        {
            return Err("Invalid recording filename in Handy history".into());
        }
        let path = self.data.join("recordings").join(path);
        if path.is_symlink() {
            return Err("Recording is a symlink; refusing to modify its target".into());
        }
        Ok(path)
    }
    fn app(&self) -> Result<&Path> {
        self.binary
            .ancestors()
            .find(|p| p.extension().is_some_and(|e| e == "app"))
            .ok_or_else(|| {
                "Set Handy Binary to the executable inside Handy.app in Workflow Configuration."
                    .into()
            })
    }
    fn running(&self) -> Result<bool> {
        let output = Command::new("/bin/ps").args(["-axo", "comm="]).output()?;
        if !output.status.success() {
            return Err("Could not check whether Handy is running".into());
        }
        let binary = self.binary.canonicalize()?;
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| Path::new(line.trim()).canonicalize().ok().as_ref() == Some(&binary)))
    }
    fn launch(&self) -> Result<()> {
        let status = Command::new("/usr/bin/open")
            .arg("-a")
            .arg(self.app()?)
            .args(["--args", "--start-hidden"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        if !status.success() {
            return Err(
                "Settings were saved, but Handy could not be opened. Open Handy manually.".into(),
            );
        }
        Ok(())
    }
    fn quit(&self) -> Result<()> {
        // Arguments are passed as data, never interpolated into AppleScript.
        let script = "on run argv\ntell application (item 1 of argv) to quit\nend run";
        let mut child = Command::new("/usr/bin/osascript")
            .args(["-e", script])
            .arg(self.app()?)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let deadline = Instant::now() + Duration::from_secs(12);
        loop {
            if let Some(status) = child.try_wait()? {
                if !status.success() {
                    return Err("Could not quit Handy. Allow Alfred to control Handy in macOS Automation settings, or quit Handy yourself and retry.".into());
                }
                break;
            }
            if Instant::now() >= deadline {
                child.kill()?;
                child.wait()?;
                return Err("Handy did not answer the quit request. Finish recording, quit Handy, and retry; settings were left unchanged.".into());
            }
            thread::sleep(Duration::from_millis(100));
        }
        while self.running()? {
            if Instant::now() >= deadline {
                return Err(
                    "Handy is still running. Quit it and retry; settings were left unchanged."
                        .into(),
                );
            }
            thread::sleep(Duration::from_millis(100));
        }
        Ok(())
    }
    pub fn toggle(&self, flag: &str) -> Result<String> {
        if !self.running()? {
            self.launch()?;
            return Ok("Handy opened. Run the recording command again when it is ready.".into());
        }
        let mut child = Command::new(&self.binary)
            .arg(flag)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let deadline = Instant::now() + Duration::from_secs(6);
        loop {
            if let Some(status) = child.try_wait()? {
                return if status.success() {
                    Ok("Recording command sent to Handy".into())
                } else {
                    Err("Handy could not run the recording command".into())
                };
            }
            if Instant::now() >= deadline {
                return Err("Handy has not acknowledged the command yet; check its recording indicator before retrying.".into());
            }
            thread::sleep(Duration::from_millis(50));
        }
    }
    pub fn apply(&self, action: &Value) -> Result<String> {
        // Validate before requesting a quit. The process is never force-killed.
        let current = self.read_store()?;
        self.validate_selection(action, &current)?;
        let mut candidate = current.clone();
        if !mutate(&mut candidate, action)? {
            return Ok("Already set — no changes needed".into());
        }
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.data.join(".alfred-handy.lock"))?;
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err("Another Handy settings change is in progress. Try again shortly.".into());
        }
        let was_running = self.running()?;
        if was_running {
            self.quit()?;
        }
        // Read AFTER exit: Handy may flush its in-memory settings as it quits.
        let result = (|| {
            let mut store = self.read_store()?;
            self.validate_selection(action, &store)?;
            mutate(&mut store, action)?;
            atomic_store(&self.data.join("settings_store.json"), &store)
        })();
        if let Err(error) = result {
            if was_running {
                let _ = self.launch();
            }
            return Err(error);
        }
        self.launch()?;
        Ok("Settings applied — Handy reopened".into())
    }
    fn validate_selection(&self, action: &Value, store: &Value) -> Result<()> {
        match text(action, "op") {
            "model" => {
                if !models::downloaded(&self.hub, &self.data.join("models"))?
                    .iter()
                    .any(|m| m["id"] == action["value"])
                {
                    return Err(
                        "That model is no longer downloaded. Refresh the model list.".into(),
                    );
                }
            }
            "language" => {
                if action["model"] != store["settings"]["selected_model"] {
                    return Err("The active model changed. Open the language list again.".into());
                }
                if !models::languages(text(&store["settings"], "selected_model"))
                    .iter()
                    .any(|l| l["code"] == action["value"])
                {
                    return Err("This language is not available for the active model.".into());
                }
            }
            _ => (),
        }
        Ok(())
    }
}
pub fn atomic_store(path: &Path, store: &Value) -> Result<()> {
    let parent = path.parent().ok_or("Missing settings directory")?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut file, store)?;
    file.write_all(b"\n")?;
    file.as_file().sync_all()?;
    file.persist(path)?;
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}
pub fn mutate(store: &mut Value, action: &Value) -> Result<bool> {
    let settings = store["settings"]
        .as_object_mut()
        .ok_or("Invalid settings object")?;
    let value = text(action, "value").trim();
    match text(action, "op") {
        "model" | "language" => {
            if value.is_empty() {
                return Err("A selection is required".into());
            }
            let key = if text(action, "op") == "model" {
                "selected_model"
            } else {
                "selected_language"
            };
            if settings.get(key).and_then(Value::as_str) == Some(value) {
                return Ok(false);
            }
            settings.insert(key.into(), value.into());
            if key == "selected_model" {
                settings.insert("selected_language".into(), "auto".into());
            }
        }
        "word-add" | "word-remove" => {
            if value.is_empty()
                || value.chars().count() > 256
                || value.chars().any(char::is_control)
            {
                return Err("Enter a word or phrase, up to 256 characters, on one line.".into());
            }
            let words = settings
                .entry("custom_words")
                .or_insert(json!([]))
                .as_array_mut()
                .ok_or("Handy's dictionary is not a list")?;
            if text(action, "op") == "word-add" {
                if words.iter().any(|w| {
                    w.as_str()
                        .is_some_and(|w| w.to_lowercase() == value.to_lowercase())
                }) {
                    return Ok(false);
                }
                words.push(value.into());
            } else {
                let before = words.len();
                words.retain(|w| w.as_str() != Some(value));
                if words.len() == before {
                    return Ok(false);
                }
            }
        }
        _ => return Err("Unknown settings action".into()),
    }
    Ok(true)
}
