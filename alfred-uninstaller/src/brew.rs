//! Homebrew owns cask removal. Detect from installed metadata, never from a guessed app name.
use crate::{
    scan::{self, App, Config},
    Result,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashSet,
    fs,
    io::{Read, Seek, SeekFrom},
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    pub checked: bool,
    pub owner: Option<Cask>,
    pub error: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cask {
    pub brew: PathBuf,
    pub token: String,
    pub full_token: String,
    pub version: Value,
    pub apps: Vec<PathBuf>,
    pub artifacts: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Outcome {
    pub success: bool,
    pub message: String,
}

pub fn executables() -> Vec<PathBuf> {
    if let Some(path) = std::env::var_os("UNINSTALL_BREW_PATH").filter(|p| !p.is_empty()) {
        return vec![PathBuf::from(path)];
    }
    let mut paths = vec![
        PathBuf::from("/opt/homebrew/bin/brew"),
        PathBuf::from("/usr/local/bin/brew"),
    ];
    if let Some(path) = std::env::var_os("PATH") {
        paths.extend(
            std::env::split_paths(&path)
                .filter(|p| p.is_absolute())
                .map(|p| p.join("brew")),
        );
    }
    let mut seen = HashSet::new();
    paths.retain(|p| p.is_file() && p.canonicalize().is_ok_and(|p| seen.insert(p)));
    paths
}

fn command(brew: &Path, args: &[&str], timeout: Duration) -> Result<String> {
    if !brew.is_absolute()
        || !brew.is_file()
        || fs::metadata(brew)?.permissions().mode() & 0o111 == 0
    {
        return Err(
            "Homebrew executable is unavailable. Check Homebrew path in Configure Workflow.".into(),
        );
    }
    let mut stdout = tempfile::tempfile()?;
    let mut stderr = tempfile::tempfile()?;
    let mut child = Command::new(brew)
        .args(args)
        .env("HOMEBREW_NO_AUTO_UPDATE", "1")
        .env("HOMEBREW_NO_ANALYTICS", "1")
        .env("HOMEBREW_NO_AUTOREMOVE", "1")
        .env("HOMEBREW_NO_INSTALL_CLEANUP", "1")
        .env("HOMEBREW_NO_ENV_HINTS", "1")
        .env("NONINTERACTIVE", "1")
        .stdin(Stdio::null())
        .stdout(stdout.try_clone()?)
        .stderr(stderr.try_clone()?)
        .process_group(0)
        .spawn()?;
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() > timeout
            || stdout.metadata()?.len() + stderr.metadata()?.len() > 8 * 1024 * 1024
        {
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            let _ = child.wait();
            return Err("Homebrew timed out or produced too much output. Check its state in Terminal before trying again.".into());
        }
        thread::sleep(Duration::from_millis(30));
    };
    stdout.seek(SeekFrom::Start(0))?;
    stderr.seek(SeekFrom::Start(0))?;
    let mut out = String::new();
    stdout.take(8 * 1024 * 1024).read_to_string(&mut out)?;
    if !status.success() {
        let mut err = String::new();
        stderr.take(64 * 1024).read_to_string(&mut err)?;
        let text = if err.trim().is_empty() { &out } else { &err };
        let detail: String = text
            .chars()
            .filter(|c| !c.is_control() || *c == '\n')
            .take(1200)
            .collect();
        return Err(format!("Homebrew exited with {status}: {}", detail.trim()).into());
    }
    Ok(out)
}

fn safe_token(token: &str) -> bool {
    !token.is_empty()
        && token.len() < 200
        && token.as_bytes()[0].is_ascii_alphanumeric()
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-+_.@".contains(&b))
        && token != "."
        && token != ".."
}
fn full_token_valid(token: &str) -> bool {
    matches!(token.split('/').count(), 1 | 3) && token.split('/').all(safe_token)
}
fn json_file(path: &Path) -> Result<Value> {
    if fs::metadata(path)?.len() > 4 * 1024 * 1024 {
        return Err("Homebrew metadata is too large".into());
    }
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
fn installed(value: &Value) -> bool {
    value.as_str().is_some_and(|s| !s.is_empty()) || value.as_array().is_some_and(|v| !v.is_empty())
}
fn appdir(root: &Path, home: &Path) -> Result<PathBuf> {
    let path = root.join(".metadata/config.json");
    if !path.exists() {
        return Ok(PathBuf::from("/Applications"));
    }
    let config = json_file(&path)?;
    for layer in ["explicit", "env", "default"] {
        if let Some(value) = config[layer]["appdir"].as_str() {
            let path = expand(value, home);
            if scan::valid_absolute(&path) {
                return Ok(path);
            }
            return Err("Invalid app directory in Homebrew's install record".into());
        }
    }
    Ok(PathBuf::from("/Applications"))
}
fn expand(value: &str, home: &Path) -> PathBuf {
    value
        .strip_prefix("~/")
        .map(|p| home.join(p))
        .unwrap_or_else(|| PathBuf::from(value))
}

/// Receipts describe the installed version; brew info can describe a newer release.
fn artifacts_for(root: &Path, cask: &Value) -> Result<Value> {
    let receipt = root.join(".metadata/INSTALL_RECEIPT.json");
    if receipt.exists() {
        let data = json_file(&receipt)?;
        if data["uninstall_artifacts"].is_array() {
            return Ok(data["uninstall_artifacts"].clone());
        }
    }
    let versions: Vec<&str> = cask["installed"]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_else(|| cask["installed"].as_str().into_iter().collect());
    for version in versions {
        if !scan::safe_name(version) {
            return Err("Invalid installed Homebrew version".into());
        }
        let folder = root.join(".metadata").join(version);
        if !folder.exists() {
            continue;
        }
        let mut timestamps = fs::read_dir(&folder)?.collect::<std::io::Result<Vec<_>>>()?;
        timestamps.sort_by_key(|e| std::cmp::Reverse(e.file_name()));
        for entry in timestamps {
            let json = entry.path().join("Casks").join(format!(
                "{}.json",
                cask["token"].as_str().unwrap_or_default()
            ));
            if json.exists() {
                let data = json_file(&json)?;
                if data["artifacts"].is_array() {
                    return Ok(data["artifacts"].clone());
                }
            }
        }
    }
    // Older/custom installations may only retain Ruby definitions. `brew info` evaluates those.
    if !cask["artifacts"].is_array() {
        return Err("Homebrew returned no cask artifact metadata".into());
    }
    Ok(cask["artifacts"].clone())
}

fn app_targets(artifacts: &Value, directory: &Path, home: &Path) -> Vec<PathBuf> {
    let mut targets = Vec::new();
    for artifact in artifacts.as_array().into_iter().flatten() {
        let Some(app) = artifact["app"].as_array() else {
            continue;
        };
        let Some(source) = app.first().and_then(Value::as_str) else {
            continue;
        };
        let target = artifact["target"]
            .as_str()
            .or_else(|| app.iter().find_map(|v| v["target"].as_str()));
        let path = if let Some(target) = target {
            expand(target, home)
        } else {
            PathBuf::from(Path::new(source).file_name().unwrap_or_default())
        };
        let path = if path.is_absolute() {
            path
        } else {
            directory.join(path)
        };
        if scan::valid_absolute(&path)
            && path
                .extension()
                .is_some_and(|s| s.eq_ignore_ascii_case("app"))
        {
            targets.push(path);
        }
    }
    targets.sort();
    targets.dedup();
    targets
}
fn strings(value: &Value) -> Vec<&str> {
    if let Some(s) = value.as_str() {
        vec![s]
    } else {
        value
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect()
    }
}
fn hook_matches(artifacts: &Value, app: &App, home: &Path) -> bool {
    artifacts.as_array().into_iter().flatten().any(|a| {
        a["uninstall"].as_array().into_iter().flatten().any(|hook| {
            strings(&hook["quit"])
                .iter()
                .any(|id| id.eq_ignore_ascii_case(&app.id))
                || ["delete", "trash"].iter().any(|key| {
                    strings(&hook[*key])
                        .iter()
                        .any(|p| expand(p, home) == app.path)
                })
        })
    })
}

fn owners(config: &Config, app: &App) -> Result<Vec<Cask>> {
    let mut owners = Vec::new();
    for brew in &config.brews {
        let info: Value = serde_json::from_str(&command(
            brew,
            &["info", "--json=v2", "--cask", "--installed"],
            Duration::from_secs(30),
        )?)?;
        let casks = info["casks"]
            .as_array()
            .ok_or("Homebrew returned an invalid installed-cask inventory")?;
        if casks.is_empty() {
            continue;
        }
        let room = PathBuf::from(command(brew, &["--caskroom"], Duration::from_secs(10))?.trim());
        if !scan::valid_absolute(&room) || !room.is_dir() {
            return Err("Homebrew returned an invalid Caskroom path".into());
        }
        for cask in casks.iter().filter(|c| installed(&c["installed"])) {
            let token = cask["token"]
                .as_str()
                .filter(|s| safe_token(s))
                .ok_or("Homebrew returned an invalid cask token")?;
            let full_token = cask["full_token"].as_str().unwrap_or(token);
            if !full_token_valid(full_token) || full_token.rsplit('/').next() != Some(token) {
                return Err("Homebrew returned an invalid full cask token".into());
            }
            let root = room.join(token);
            let artifacts = artifacts_for(&root, cask)?;
            let apps = app_targets(&artifacts, &appdir(&root, &config.home)?, &config.home);
            if apps
                .iter()
                .any(|p| p == &app.path || p.canonicalize().ok().as_ref() == Some(&app.path))
                || hook_matches(&artifacts, app, &config.home)
            {
                owners.push(Cask {
                    brew: brew.clone(),
                    token: token.into(),
                    full_token: full_token.into(),
                    version: cask["installed"].clone(),
                    apps,
                    artifacts,
                });
            }
        }
    }
    Ok(owners)
}
pub fn detect(config: &Config, app: &App) -> Status {
    match owners(config,app) {
        Ok(mut owners) if owners.len() <= 1 => Status { checked:true, owner:owners.pop(), error:None },
        Ok(_) => Status { checked:true, owner:None, error:Some("Multiple Homebrew installations claim this app. Resolve that in Homebrew before removing it.".into()) },
        Err(e) => Status { checked:true, owner:None, error:Some(format!("Cannot verify Homebrew ownership: {e}")) },
    }
}

pub fn uninstall(cask: &Cask) -> Result<()> {
    if !safe_token(&cask.token)
        || !full_token_valid(&cask.full_token)
        || cask.full_token.rsplit('/').next() != Some(cask.token.as_str())
    {
        return Err("Invalid Homebrew uninstall target".into());
    }
    command(
        &cask.brew,
        &["uninstall", "--cask", "--", &cask.full_token],
        Duration::from_secs(180),
    )?;
    // A success exit alone is insufficient: confirm the installed record is gone.
    let remaining = command(&cask.brew, &["list", "--cask"], Duration::from_secs(30))?;
    if remaining
        .lines()
        .any(|line| line.trim().rsplit('/').next() == Some(&cask.token))
    {
        return Err(
            "Homebrew still lists the cask as installed. Further file cleanup stopped.".into(),
        );
    }
    Ok(())
}

pub fn effects(cask: &Cask) -> Vec<String> {
    let mut rows: Vec<String> = cask.apps.iter().map(|p| p.display().to_string()).collect();
    let mut kinds = HashSet::new();
    for artifact in cask
        .artifacts
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
    {
        for key in artifact
            .keys()
            .filter(|s| !["app", "target", "zap"].contains(&s.as_str()))
        {
            kinds.insert(key.replace('_', " "));
        }
    }
    let mut kinds: Vec<_> = kinds.into_iter().collect();
    kinds.sort();
    if !kinds.is_empty() {
        rows.push(format!(
            "Cask components and routines: {}",
            kinds.join(", ")
        ));
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn targets_respect_custom_appdir_and_renames() {
        let artifacts = json!([{"app":["nested/Original.app",{"target":"Renamed.app"}]},{"app":["Other.app"],"target":"/other/Other.app"}]);
        assert_eq!(
            app_targets(&artifacts, Path::new("/custom"), Path::new("/home")),
            vec![
                PathBuf::from("/custom/Renamed.app"),
                PathBuf::from("/other/Other.app")
            ]
        );
    }
    #[test]
    fn tokens_cannot_inject_commands_or_flags() {
        for token in ["--force", "../x", "x;touch hi", "x$(whoami)", "a\nb", "a/b"] {
            assert!(!safe_token(token));
        }
        assert!(full_token_valid("user/tap/app@nightly"));
        assert!(!full_token_valid("user/../app"));
    }
    #[test]
    fn hung_command_is_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("brew");
        fs::write(&script, "#!/bin/sh\n/bin/sleep 10\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let started = Instant::now();
        assert!(command(&script, &[], Duration::from_millis(50))
            .unwrap_err()
            .to_string()
            .contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(3));
    }
}
