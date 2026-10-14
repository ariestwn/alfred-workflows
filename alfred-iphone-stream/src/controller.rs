use crate::state;
use std::{
    fs::{self, File, OpenOptions},
    os::fd::AsRawFd,
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub struct Controller {
    pub dir: PathBuf,
    daemon: PathBuf,
    idam: PathBuf,
}

struct Lock(File);
impl Drop for Lock {
    fn drop(&mut self) {
        unsafe {
            libc::flock(self.0.as_raw_fd(), libc::LOCK_UN);
        }
    }
}

impl Controller {
    pub fn from_env() -> Result<Self, String> {
        let root = std::env::current_exe()
            .map_err(|e| e.to_string())?
            .parent()
            .ok_or("Lokasi workflow tidak ditemukan")?
            .to_path_buf();
        let dir = match std::env::var_os("IPHONE_STREAM_STATE_DIR") {
            Some(dir) => PathBuf::from(dir),
            None => PathBuf::from(std::env::var_os("alfred_workflow_data").unwrap_or(
                PathBuf::from(std::env::var_os("HOME").ok_or("HOME tidak tersedia")?)
                    .join("Library/Application Support/Alfred/Workflow Data/com.ariestwn.iphone-stream").into_os_string()
            )),
        };
        Ok(Self {
            dir,
            daemon: root.join("iPhone Stream Helper.app/Contents/MacOS/iphone-stream-daemon"),
            idam: root.join("idam-helper"),
        })
    }

    fn lock(&self) -> Result<Lock, String> {
        fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.dir.join("command.lock"))
            .map_err(|e| e.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
                return Ok(Lock(file));
            }
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::EWOULDBLOCK) {
                return Err(error.to_string());
            }
            if Instant::now() >= deadline {
                return Err("Perintah lain masih berjalan. Coba lagi sebentar.".into());
            }
            thread::sleep(Duration::from_millis(50));
        }
    }

    pub fn running_pid(&self) -> Option<i32> {
        self.owned_pid("daemon.pid")
    }

    fn owned_pid(&self, file: &str) -> Option<i32> {
        let pid = fs::read_to_string(self.dir.join(file))
            .ok()?
            .trim()
            .parse::<i32>()
            .ok()?;
        if pid <= 1 {
            return None;
        }
        // Never signal an unrelated process if a stale PID has been reused.
        let actual = process_path(pid)?;
        (fs::canonicalize(actual).ok()? == fs::canonicalize(&self.daemon).ok()?).then_some(pid)
    }

    fn start(&self) -> Result<String, String> {
        if self.running_pid().is_some() {
            return Ok("iPhone Stream sudah aktif".into());
        }
        if !self.daemon.is_file() {
            return Err("Helper belum tersedia. Jalankan build.sh terlebih dahulu.".into());
        }
        state::write_volume(&self.dir, state::read_volume(&self.dir))?;
        let _ = fs::remove_file(self.dir.join("daemon.pid"));
        // Each start has a fresh diagnostic log, so an older error isn't shown.
        let log = File::create(self.dir.join("daemon.log")).map_err(|e| e.to_string())?;
        let launch_id = format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos()
        );
        let _ = fs::remove_file(self.dir.join("startup.json"));
        let _ = fs::remove_file(self.dir.join("startup.pid"));
        // LaunchServices makes the app responsible for its own microphone access.
        // Directly spawning its executable incorrectly attributes access to Alfred.
        let mut command = self.launch_command(&launch_id)?;
        command
            .stdin(Stdio::null())
            .stdout(log.try_clone().map_err(|e| e.to_string())?)
            .stderr(log);
        let mut child = command.spawn().map_err(|e| e.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if self.running_pid().is_some() {
                return Ok("iPhone Stream aktif".into());
            }
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                if !status.success() {
                    let log = fs::read_to_string(self.dir.join("daemon.log")).unwrap_or_default();
                    return Err(format!(
                        "Aplikasi helper tidak dapat dibuka ({status}): {}",
                        log.trim()
                    ));
                }
            }
            if let Some(report) = self.startup_report(&launch_id) {
                if report["phase"] == "failed" {
                    return Err(report["message"]
                        .as_str()
                        .unwrap_or("Helper gagal memulai audio.")
                        .into());
                }
                if report["phase"] == "starting" && self.owned_pid("startup.pid").is_none() {
                    let log = fs::read_to_string(self.dir.join("daemon.log")).unwrap_or_default();
                    return Err(format!("Helper berhenti saat startup: {}. Periksa log dan izin Microphone untuk iPhone Stream Helper.", log.lines().last().unwrap_or("tanpa pesan kesalahan")));
                }
            }
            if Instant::now() >= deadline {
                if let Some(pid) = self.owned_pid("startup.pid") {
                    unsafe {
                        libc::kill(pid, libc::SIGTERM);
                    }
                }
                let _ = child.kill();
                let _ = child.wait();
                let _ = fs::remove_file(self.dir.join("daemon.pid"));
                return Err(
                    "Audio belum aktif. Periksa izin Microphone dan Audio MIDI Setup.".into(),
                );
            }
            thread::sleep(Duration::from_millis(100));
        }
    }

    fn launch_command(&self, launch_id: &str) -> Result<Command, String> {
        let app = self
            .daemon
            .ancestors()
            .nth(3)
            .ok_or("Lokasi aplikasi helper tidak valid")?;
        let mut command = Command::new("/usr/bin/open");
        command
            .args(["-g", "-n"])
            .arg(app)
            .arg("--args")
            .arg("--state-dir")
            .arg(&self.dir)
            .arg("--launch-id")
            .arg(launch_id);
        Ok(command)
    }

    fn startup_report(&self, launch_id: &str) -> Option<serde_json::Value> {
        let report: serde_json::Value =
            serde_json::from_slice(&fs::read(self.dir.join("startup.json")).ok()?).ok()?;
        (report["launchID"].as_str() == Some(launch_id)).then_some(report)
    }

    fn stop(&self) -> Result<String, String> {
        if let Some(pid) = self.running_pid() {
            if unsafe { libc::kill(pid, libc::SIGTERM) } != 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
            let deadline = Instant::now() + Duration::from_secs(3);
            while self.running_pid().is_some() {
                if Instant::now() >= deadline {
                    return Err("Helper belum berhenti. Periksa log.".into());
                }
                thread::sleep(Duration::from_millis(50));
            }
        }
        let _ = fs::remove_file(self.dir.join("daemon.pid"));
        Ok("iPhone Stream berhenti".into())
    }

    fn idam(&self, args: &[&str]) -> Result<serde_json::Value, String> {
        let mut child = Command::new(&self.idam)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Gagal menjalankan IDAM helper: {e}"))?;
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            if child.try_wait().map_err(|e| e.to_string())?.is_some() {
                break;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(
                    "iPhone tidak merespons dalam 8 detik. Periksa kabel dan Trust This Computer."
                        .into(),
                );
            }
            thread::sleep(Duration::from_millis(50));
        }
        let out = child.wait_with_output().map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
        }
        let value: serde_json::Value = serde_json::from_slice(&out.stdout)
            .map_err(|e| format!("Respons IDAM tidak valid: {e}"))?;
        if value["ok"] != true || !value["enabled"].is_boolean() {
            return Err("Respons IDAM tidak valid".into());
        }
        Ok(value)
    }

    pub fn action(&self, command: &str) -> Result<String, String> {
        let _lock = self.lock()?;
        match command {
            "toggle" => {
                if self.running_pid().is_some() {
                    self.stop()
                } else {
                    self.start()
                }
            }
            "start" => self.start(),
            "stop" => self.stop(),
            "volume-up" | "volume-down" => {
                if self.running_pid().is_none() {
                    return Err("Stream belum aktif. Jalankan Toggle iPhone Stream.".into());
                }
                let delta = if command == "volume-up" { 0.1 } else { -0.1 };
                let volume = state::clamp(state::read_volume(&self.dir) + delta);
                state::write_volume(&self.dir, volume)?;
                Ok(format!("Volume iPhone: {:.0}%", volume * 100.0))
            }
            "idam-toggle" => {
                let enabled = self.idam(&["get"])?["enabled"].as_bool().unwrap();
                // Stop audio before disabling its underlying device.
                if enabled {
                    self.stop()?;
                }
                self.idam(&["set", if enabled { "false" } else { "true" }])?;
                Ok(if enabled { "Audio MIDI Mode dinonaktifkan" } else { "Audio MIDI Mode diminta aktif. Tunggu iPhone terhubung ulang, lalu mulai stream." }.into())
            }
            "audio-midi" => {
                let status = Command::new("/usr/bin/open")
                    .args(["-b", "com.apple.audio.AudioMIDISetup"])
                    .status()
                    .map_err(|e| e.to_string())?;
                if !status.success() {
                    return Err("Audio MIDI Setup tidak dapat dibuka".into());
                }
                Ok("Audio MIDI Setup dibuka".into())
            }
            "log" => {
                let path = self.dir.join("daemon.log");
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .map_err(|e| e.to_string())?;
                let status = Command::new("/usr/bin/open")
                    .arg(path)
                    .status()
                    .map_err(|e| e.to_string())?;
                if !status.success() {
                    return Err("Log tidak dapat dibuka".into());
                }
                Ok("Log iPhone Stream dibuka".into())
            }
            _ => Err(format!("Perintah tidak dikenal: {command}")),
        }
    }
}

#[cfg(target_os = "macos")]
fn process_path(pid: i32) -> Option<PathBuf> {
    unsafe extern "C" {
        fn proc_pidpath(pid: i32, buffer: *mut libc::c_void, buffersize: u32) -> i32;
    }
    let mut buffer = vec![0u8; 4096];
    let size = unsafe { proc_pidpath(pid, buffer.as_mut_ptr().cast(), buffer.len() as u32) };
    if size <= 0 {
        return None;
    }
    use std::os::unix::ffi::OsStrExt;
    let end = buffer.iter().position(|b| *b == 0).unwrap_or(size as usize);
    Some(PathBuf::from(std::ffi::OsStr::from_bytes(&buffer[..end])))
}

#[cfg(not(target_os = "macos"))]
fn process_path(pid: i32) -> Option<PathBuf> {
    fs::read_link(format!("/proc/{pid}/exe")).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn controller(dir: &std::path::Path) -> Controller {
        Controller {
            dir: dir.into(),
            daemon: dir.join("missing-daemon"),
            idam: dir.join("missing-idam"),
        }
    }
    #[test]
    fn stale_pid_does_not_signal_current_process() {
        let dir = tempfile::tempdir().unwrap();
        let c = controller(dir.path());
        fs::write(
            dir.path().join("daemon.pid"),
            std::process::id().to_string(),
        )
        .unwrap();
        assert_eq!(c.running_pid(), None);
        assert!(c.action("stop").is_ok());
        assert!(!dir.path().join("daemon.pid").exists());
    }
    #[test]
    fn stopped_volume_and_missing_helpers_have_no_side_effects() {
        let dir = tempfile::tempdir().unwrap();
        let c = controller(dir.path());
        assert!(c.action("volume-down").unwrap_err().contains("belum aktif"));
        assert!(!dir.path().join("state.json").exists());
        assert!(c
            .action("start")
            .unwrap_err()
            .contains("Helper belum tersedia"));
        assert!(c.action("idam-toggle").is_err());
    }
    #[test]
    fn actions_are_serialized() {
        let dir = tempfile::tempdir().unwrap();
        let c = controller(dir.path());
        let lock = c.lock().unwrap();
        assert!(c.lock().is_err());
        drop(lock);
        assert!(c.lock().is_ok());
    }

    fn fake_idam(c: &Controller, script: &str) {
        use std::os::unix::fs::PermissionsExt;
        fs::write(&c.idam, script).unwrap();
        fs::set_permissions(&c.idam, fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn idam_enable_sends_correct_command_and_rejects_bad_output() {
        let dir = tempfile::tempdir().unwrap();
        let c = controller(dir.path());
        fake_idam(&c, "#!/bin/sh\nif [ \"$1\" = get ]; then\nprintf '%s\\n' '{\"ok\":true,\"enabled\":false}'\nelif [ \"$1 $2\" = 'set true' ]; then\nprintf '%s\\n' '{\"ok\":true,\"enabled\":true}'\nelse\nexit 2\nfi\n");
        assert!(c.action("idam-toggle").unwrap().contains("diminta aktif"));
        fake_idam(
            &c,
            "#!/bin/sh\nprintf '%s\\n' '{\"ok\":true,\"enabled\":\"false\"}'\n",
        );
        assert!(c.action("idam-toggle").unwrap_err().contains("tidak valid"));
        fake_idam(&c, "#!/bin/sh\necho 'device not paired' >&2\nexit 4\n");
        assert_eq!(c.action("idam-toggle").unwrap_err(), "device not paired");
    }

    #[test]
    fn startup_reports_ignore_old_launches_and_partial_json() {
        let dir = tempfile::tempdir().unwrap();
        let c = controller(dir.path());
        let file = dir.path().join("startup.json");
        assert!(c.startup_report("new").is_none());
        fs::write(&file, "partial").unwrap();
        assert!(c.startup_report("new").is_none());
        fs::write(
            &file,
            r#"{"launchID":"old","phase":"failed","message":"old error"}"#,
        )
        .unwrap();
        assert!(c.startup_report("new").is_none());
        fs::write(
            &file,
            r#"{"launchID":"new","phase":"failed","message":"permission denied"}"#,
        )
        .unwrap();
        assert_eq!(
            c.startup_report("new").unwrap()["message"],
            "permission denied"
        );
    }

    #[test]
    fn launch_services_receives_app_path_and_state_as_separate_arguments() {
        let dir = tempfile::tempdir().unwrap();
        let mut c = controller(dir.path());
        c.daemon = dir
            .path()
            .join("iPhone Stream Helper.app/Contents/MacOS/iphone-stream-daemon");
        c.dir = dir.path().join("data with spaces");
        let command = c.launch_command("attempt-1").unwrap();
        assert_eq!(command.get_program(), "/usr/bin/open");
        let args: Vec<_> = command
            .get_args()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            vec![
                "-g".into(),
                "-n".into(),
                dir.path()
                    .join("iPhone Stream Helper.app")
                    .to_string_lossy()
                    .into_owned(),
                "--args".into(),
                "--state-dir".into(),
                c.dir.to_string_lossy().into_owned(),
                "--launch-id".into(),
                "attempt-1".into()
            ]
        );
    }

    #[test]
    fn owned_process_allows_volume_and_stops_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let c = controller(dir.path());
        // A copied sleep executable emulates an owned daemon without audio/TCC.
        fs::copy("/bin/sleep", &c.daemon).unwrap();
        let mut child = Command::new(&c.daemon).arg("20").spawn().unwrap();
        fs::write(dir.path().join("daemon.pid"), child.id().to_string()).unwrap();
        assert_eq!(c.running_pid(), Some(child.id() as i32));
        assert_eq!(c.action("volume-down").unwrap(), "Volume iPhone: 90%");
        assert_eq!(c.action("volume-up").unwrap(), "Volume iPhone: 100%");
        assert_eq!(c.action("volume-up").unwrap(), "Volume iPhone: 100%");
        assert!(c.action("stop").is_ok());
        child.wait().unwrap();
        assert_eq!(c.running_pid(), None);
    }
}
