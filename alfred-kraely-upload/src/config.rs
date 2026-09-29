use crate::Result;
use std::{env, time::Duration};

#[derive(Debug)]
pub struct Config {
    pub host: String,
    pub directory: String,
    pub max_bytes: u64,
    pub timeout: Duration,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Self::from_lookup(|name| env::var(name).ok())
    }

    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self> {
        let setting = |key, default: &str| {
            get(key)
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| default.into())
                .trim()
                .to_owned()
        };
        let host = setting("KRAELY_HOST", "");
        if host.is_empty() {
            return Err("Set an SSH host in Configure Workflow before uploading.".into());
        }
        validate_host(&host)?;
        let directory = setting("KRAELY_REMOTE_DIR", "/home/ubuntu/screenshot/");
        if !directory.starts_with('/') || directory.chars().any(char::is_control) {
            return Err(
                "Remote folder must be an absolute path, such as /home/ubuntu/screenshot/.".into(),
            );
        }
        let max_mb: f64 = setting("KRAELY_MAX_SIZE_MB", "20")
            .parse()
            .map_err(|_| "Max file size must be a positive number in MB.".to_owned())?;
        if !max_mb.is_finite() || !(0.0..=1_048_576.0).contains(&max_mb) || max_mb == 0.0 {
            return Err("Max file size must be greater than 0 and at most 1048576 MB.".into());
        }
        let seconds: u64 = setting("KRAELY_TIMEOUT", "60")
            .parse()
            .map_err(|_| "Timeout must be between 1 and 600 seconds.".to_owned())?;
        if !(1..=600).contains(&seconds) {
            return Err("Timeout must be between 1 and 600 seconds.".into());
        }
        Ok(Self {
            host,
            directory,
            max_bytes: (max_mb * 1024.0 * 1024.0).ceil() as u64,
            timeout: Duration::from_secs(seconds),
        })
    }
}

fn validate_host(value: &str) -> Result<()> {
    let error = "SSH host must be an alias or hostname, optionally user@host. Set the port and key in ~/.ssh/config.";
    let plain = |s: &str| {
        !s.is_empty()
            && !s.starts_with('-')
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    };
    let host = if let Some((user, host)) = value.split_once('@') {
        if !plain(user) {
            return Err(error.into());
        }
        host
    } else {
        value
    };
    let ipv6 = host
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .is_some_and(|s| s.parse::<std::net::Ipv6Addr>().is_ok());
    if !plain(host) && !ipv6 {
        return Err(error.into());
    }
    Ok(())
}
