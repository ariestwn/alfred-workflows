use std::{env, path::PathBuf, time::Duration};

use crate::Result;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Provider {
    Codex,
    Claude,
    OpenCode,
}

impl Provider {
    pub fn name(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::OpenCode => "opencode",
        }
    }
}

#[derive(Debug)]
pub struct Config {
    pub provider: Provider,
    pub executable: PathBuf,
    pub model: String,
    pub effort: String,
    pub output: String,
    pub timeout: Duration,
    pub global_prompt: String,
    pub opencode_dir: PathBuf,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Self::load(|key| env::var(key).unwrap_or_default())
    }

    pub fn load(get: impl Fn(&str) -> String) -> Result<Self> {
        let provider = match get("QUICKAI_PROVIDER").trim().to_lowercase().as_str() {
            "" | "codex" => Provider::Codex,
            "claude" => Provider::Claude,
            "opencode" => Provider::OpenCode,
            _ => {
                return Err(
                    "Provider must be codex, claude, or opencode. Open Configure Workflow.".into(),
                )
            }
        };
        let prefix = format!("QUICKAI_{}", provider.name().to_uppercase());
        let model = get(&format!("{prefix}_MODEL")).trim().to_owned();
        let effort = get(&format!("{prefix}_EFFORT")).trim().to_lowercase();
        let effort = if effort == "default" {
            String::new()
        } else {
            effort
        };
        let valid = match provider {
            Provider::Codex => [
                "", "none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra",
            ]
            .as_slice(),
            Provider::Claude => ["", "low", "medium", "high", "xhigh", "max"].as_slice(),
            // DeepSeek models have no reasoning variants; a variant can be appended to the model.
            Provider::OpenCode => [""].as_slice(),
        };
        if !valid.contains(&effort.as_str()) {
            return Err(format!(
                "Unsupported {} effort: {effort}. Open Configure Workflow.",
                provider.name()
            ));
        }
        let output = match get("QUICKAI_OUTPUT").trim() {
            "" | "paste" => "paste",
            "copy" => "copy",
            "both" => "both",
            _ => return Err("Output must be paste, copy, or both.".into()),
        }
        .to_owned();
        let timeout = get("QUICKAI_TIMEOUT");
        let seconds = if timeout.trim().is_empty() {
            120
        } else {
            timeout
                .trim()
                .parse::<u64>()
                .map_err(|_| "Timeout must be a whole number of seconds.")?
        };
        if !(1..=600).contains(&seconds) {
            return Err("Timeout must be between 1 and 600 seconds.".into());
        }
        let binary = get(&format!("{prefix}_PATH"));
        let executable = if binary.trim().is_empty() {
            PathBuf::from(provider.name())
        } else if let Some(relative) = binary.trim().strip_prefix("~/") {
            PathBuf::from(
                env::var_os("HOME").ok_or("HOME is unavailable; use an absolute CLI path.")?,
            )
            .join(relative)
        } else {
            PathBuf::from(binary.trim())
        };
        Ok(Self {
            provider,
            executable,
            model,
            effort,
            output,
            timeout: Duration::from_secs(seconds),
            global_prompt: get("QUICKAI_GLOBAL_PROMPT"),
            opencode_dir: opencode_dir(&get),
        })
    }
}

/// OpenCode treats every working directory as a project. Keep its project and
/// private config in one cache location instead of the per-request temp directory.
fn opencode_dir(get: &impl Fn(&str) -> String) -> PathBuf {
    let configured = get("QUICKAI_OPENCODE_DIR").trim().to_owned();
    if let Some(relative) = configured.strip_prefix("~/") {
        if let Some(home) = env::var_os("HOME") {
            return PathBuf::from(home).join(relative);
        }
    } else if !configured.is_empty() {
        return PathBuf::from(configured);
    }
    env::var_os("HOME")
        .map(|home| PathBuf::from(home).join("Library/Caches/com.ariestwn.quickai/opencode"))
        .unwrap_or_else(|| PathBuf::from("quickai-opencode"))
}

/// GUI apps don't inherit interactive shell setup. Never source shell dotfiles.
pub fn search_path() -> Result<std::ffi::OsString> {
    let mut paths = Vec::new();
    if let Some(home) = env::var_os("HOME") {
        let home = PathBuf::from(home);
        paths.extend([
            home.join(".local/bin"),
            home.join(".opencode/bin"),
            home.join(".npm-global/bin"),
            home.join(".volta/bin"),
        ]);
    }
    paths.extend(
        [
            "/opt/homebrew/bin",
            "/usr/local/bin",
            "/usr/bin",
            "/bin",
            "/usr/sbin",
            "/sbin",
        ]
        .map(PathBuf::from),
    );
    if let Some(current) = env::var_os("PATH") {
        paths.extend(env::split_paths(&current));
    }
    env::join_paths(paths).map_err(|e| format!("Cannot construct CLI search path: {e}"))
}
