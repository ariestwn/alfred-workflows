use crate::{config::Config, Result};
use chrono::{TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    process::{Command, Stdio},
    sync::{Arc, Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

pub const CRYPTO: &[&str] = &[
    "BTC", "ETH", "SOL", "DOGE", "LTC", "BCH", "XRP", "ADA", "DOT", "AVAX", "LINK", "USDT", "USDC",
    "BNB", "XLM", "UNI", "ATOM", "NEAR",
];
pub fn names() -> &'static BTreeMap<String, String> {
    static NAMES: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    NAMES.get_or_init(|| {
        serde_json::from_str(include_str!("../data/currencies.json")).expect("currency metadata")
    })
}
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub fetched: u64,
    pub as_of: u64,
    pub rates: BTreeMap<String, f64>,
}
#[derive(Default)]
struct State {
    loaded: BTreeMap<String, Snapshot>,
    pub pending: bool,
    pub notes: Vec<String>,
}
#[derive(Clone)]
pub struct Rates {
    config: Config,
    state: Arc<Mutex<State>>,
}
impl Rates {
    pub fn new(config: &Config) -> Self {
        Self {
            config: config.clone(),
            state: Arc::new(Mutex::new(State::default())),
        }
    }
    pub fn status(&self) -> (bool, String) {
        let s = self.state.lock().unwrap();
        (s.pending, s.notes.join(" · "))
    }
    pub fn get(&self, code: &str) -> Result<f64> {
        if code == "USD" {
            return Ok(1.0);
        }
        let provider = if CRYPTO.contains(&code) {
            "crypto"
        } else {
            "fiat"
        };
        let ttl = if provider == "crypto" { 300 } else { 43200 };
        let mut state = self.state.lock().unwrap();
        if !state.loaded.contains_key(provider) {
            let path = self.config.cache.join(format!("{provider}.json"));
            let cached = fs::read(path)
                .ok()
                .and_then(|b| serde_json::from_slice::<Snapshot>(&b).ok())
                .filter(valid_snapshot);
            if cached
                .as_ref()
                .is_none_or(|s| now().saturating_sub(s.fetched) > ttl)
            {
                state.pending |= schedule(&self.config, provider)?;
            }
            if let Some(cached) = cached {
                let age = now().saturating_sub(cached.fetched);
                let source = if provider == "crypto" {
                    "Coinbase"
                } else {
                    "ExchangeRate-API"
                };
                let timestamp = Utc
                    .timestamp_opt(cached.as_of as i64, 0)
                    .single()
                    .map(|d| d.format("%Y-%m-%d %H:%M UTC").to_string())
                    .unwrap_or_default();
                let note = format!(
                    "{}{} · {}",
                    if age > ttl { "Cached / stale · " } else { "" },
                    source,
                    timestamp
                );
                state.notes.push(note);
                state.loaded.insert(provider.into(), cached);
            } else {
                return Err(if state.pending {
                    "Fetching exchange rates…"
                } else {
                    "Exchange rates unavailable. Check your connection; retry in a minute."
                }
                .into());
            }
        }
        state.loaded[provider]
            .rates
            .get(code)
            .copied()
            .ok_or_else(|| format!("{code} is not available from the rate provider").into())
    }
}
impl fend_core::ExchangeRateFnV2 for Rates {
    fn relative_to_base_currency(
        &self,
        currency: &str,
        _options: &fend_core::ExchangeRateFnV2Options,
    ) -> std::result::Result<f64, Box<dyn std::error::Error + Send + Sync>> {
        self.get(currency).map_err(|e| e.to_string().into())
    }
}
fn valid_snapshot(s: &Snapshot) -> bool {
    s.fetched > 0
        && s.as_of > 0
        && s.fetched <= now() + 300
        && s.rates.get("USD").is_some_and(|r| (*r - 1.0).abs() < 1e-9)
        && s.rates.values().all(|r| r.is_finite() && *r > 0.0)
}
fn age(path: &Path) -> u64 {
    path.metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .map(|d| d.as_secs())
        .unwrap_or(u64::MAX)
}
fn schedule(config: &Config, provider: &str) -> Result<bool> {
    if !config.network {
        return Ok(false);
    }
    fs::create_dir_all(&config.cache)?;
    if age(&config.cache.join(format!("{provider}.error"))) < 60 {
        return Ok(false);
    }
    let lock = config.cache.join(format!("{provider}.lock"));
    if lock.exists() {
        if age(&lock) < 20 {
            return Ok(true);
        }
        let _ = fs::remove_file(&lock);
    }
    match OpenOptions::new().create_new(true).write(true).open(&lock) {
        Ok(mut f) => {
            writeln!(f, "{}", now())?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Ok(true),
        Err(e) => return Err(e.into()),
    }
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args(["refresh", provider])
        .arg(&config.cache)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    if let Err(error) = command.spawn() {
        let _ = fs::remove_file(lock);
        return Err(error.into());
    }
    Ok(true)
}
pub fn refresh(provider: &str, cache: &Path) -> Result<()> {
    let url = match provider {
        "fiat" => "https://open.er-api.com/v6/latest/USD",
        "crypto" => "https://api.coinbase.com/v2/exchange-rates?currency=USD",
        _ => return Err("Unknown rate provider".into()),
    };
    fs::create_dir_all(cache)?;
    let result = (|| {
        let download = tempfile::NamedTempFile::new_in(cache)?;
        let output = Command::new("/usr/bin/curl")
            .args([
                "--proto",
                "=https",
                "--tlsv1.2",
                "--fail",
                "--silent",
                "--show-error",
                "--connect-timeout",
                "3",
                "--max-time",
                "8",
                "--max-filesize",
                "2097152",
                "--output",
            ])
            .arg(download.path())
            .arg(url)
            .output()?;
        if !output.status.success() {
            return Err("Could not refresh exchange rates".into());
        }
        let data: Value = serde_json::from_slice(&fs::read(download.path())?)?;
        let snapshot = parse(provider, &data, now())?;
        let mut file = tempfile::NamedTempFile::new_in(cache)?;
        serde_json::to_writer(&mut file, &snapshot)?;
        file.as_file().sync_all()?;
        file.persist(cache.join(format!("{provider}.json")))?;
        let _ = fs::remove_file(cache.join(format!("{provider}.error")));
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::write(cache.join(format!("{provider}.error")), now().to_string());
    }
    let _ = fs::remove_file(cache.join(format!("{provider}.lock")));
    result
}
pub fn parse(provider: &str, data: &Value, fetched: u64) -> Result<Snapshot> {
    let (rates, as_of) = if provider == "fiat" {
        if data["result"] != "success" || data["base_code"] != "USD" {
            return Err("Invalid fiat rate response".into());
        }
        (
            &data["rates"],
            data["time_last_update_unix"]
                .as_u64()
                .ok_or("Missing rate date")?,
        )
    } else {
        if data["data"]["currency"] != "USD" {
            return Err("Invalid crypto rate base".into());
        }
        (&data["data"]["rates"], fetched)
    };
    let mut parsed = BTreeMap::new();
    for (code, rate) in rates.as_object().ok_or("Missing exchange rates")? {
        let rate = rate
            .as_f64()
            .or_else(|| rate.as_str().and_then(|s| s.parse().ok()))
            .ok_or("Invalid exchange rate")?;
        if !rate.is_finite() || rate <= 0.0 {
            return Err("Exchange rates must be positive and finite".into());
        }
        parsed.insert(code.clone(), rate);
    }
    let snapshot = Snapshot {
        fetched,
        as_of,
        rates: parsed,
    };
    if !valid_snapshot(&snapshot) {
        return Err("Invalid rate snapshot".into());
    }
    Ok(snapshot)
}
