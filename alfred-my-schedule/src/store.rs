use crate::{model::*, Result};
use chrono::{Duration, Utc};
use serde_json::{json, Value};
use std::{
    ffi::{CStr, CString},
    fs,
    io::Write,
    os::{raw::c_char, unix::fs::PermissionsExt},
    path::Path,
};

extern "C" {
    fn schedule_call(input: *const c_char) -> *mut c_char;
    fn schedule_free(value: *mut c_char);
}
pub fn native(request: Value) -> Result<Value> {
    let input = CString::new(serde_json::to_string(&request)?)?;
    // The bridge returns a strdup allocation; copy its bytes before freeing it.
    let output = unsafe {
        let ptr = schedule_call(input.as_ptr());
        if ptr.is_null() {
            return Err("EventKit returned no response.".into());
        }
        let bytes = CStr::from_ptr(ptr).to_bytes().to_vec();
        schedule_free(ptr);
        bytes
    };
    let value: Value = serde_json::from_slice(&output)?;
    if let Some(message) = value["error"].as_str() {
        return Err(message.into());
    }
    Ok(value)
}
pub fn atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or("Invalid storage directory")?;
    fs::create_dir_all(parent)?;
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(path)?;
    Ok(())
}
pub fn preferences(config: &Config) -> Result<Preferences> {
    match fs::read(config.data.join("preferences.json")) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Preferences::default()),
        Err(e) => Err(e.into()),
    }
}
pub fn save_preferences(config: &Config, preferences: &Preferences) -> Result<()> {
    atomic(
        &config.data.join("preferences.json"),
        &serde_json::to_vec(preferences)?,
    )
}
pub fn snapshot(config: &Config, now: chrono::DateTime<Utc>, refresh: bool) -> Result<Snapshot> {
    // Recheck authorization before using cached private event data.
    if native(json!({"op":"status"}))?["status"] != 3 {
        let _ = fs::remove_file(config.cache.join("events.json"));
        return Err("Calendar access required".into());
    }
    let from = config.midnight(config.today(now))?;
    let until = config.midnight(config.today(now) + Duration::days(config.days))?;
    let path = config.cache.join("events.json");
    if !refresh {
        if let Ok(bytes) = fs::read(&path) {
            if let Ok(snapshot) = serde_json::from_slice::<Snapshot>(&bytes) {
                if (0..20).contains(&(now.timestamp() - snapshot.fetched))
                    && snapshot.from == from
                    && snapshot.until == until
                {
                    return Ok(snapshot);
                }
            }
        }
    }
    let value = native(json!({"op":"snapshot", "start":from, "end":until}))?;
    let mut snapshot: Snapshot = serde_json::from_value(value)?;
    snapshot.events.retain(|e| {
        chrono::DateTime::from_timestamp(e.start, 0).is_some()
            && chrono::DateTime::from_timestamp(e.end, 0).is_some()
            && e.end >= e.start
    });
    snapshot.events.sort_by(|a, b| {
        (a.start, !a.all_day, &a.title, &a.id).cmp(&(b.start, !b.all_day, &b.title, &b.id))
    });
    snapshot
        .events
        .dedup_by(|a, b| a.id == b.id && a.start == b.start && a.calendar_id == b.calendar_id);
    snapshot.fetched = now.timestamp();
    snapshot.from = from;
    snapshot.until = until;
    atomic(&path, &serde_json::to_vec(&snapshot)?)?;
    Ok(snapshot)
}
