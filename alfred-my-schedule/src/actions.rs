use crate::{links, model::*, store, Result};
use chrono::{Local, TimeZone, Utc};
use serde_json::{json, Value};
use std::{fs, io::Write, os::unix::fs::OpenOptionsExt, process::Command};

fn command(program: &str, args: &[&str]) -> Result<()> {
    let output = Command::new(program).args(args).output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr)
            .trim()
            .to_string()
            .into());
    }
    Ok(())
}
fn trigger(name: &str, query: &str) -> Result<()> {
    command(
        "/usr/bin/osascript",
        &[
            "-e",
            r#"on run argv
tell application id "com.runningwithcrayons.Alfred" to run trigger (item 1 of argv) in workflow "com.ariestwn.my-schedule-rust" with argument (item 2 of argv)
end run"#,
            name,
            query,
        ],
    )
}
// Calendar's URL handler accepts the local item identifier, with a date for recurrence.
// Timed occurrences use UTC; all-day occurrences use the Mac's local calendar date.
pub(crate) fn calendar_event_url<T: TimeZone>(event: &Event, local_zone: &T) -> Result<String> {
    use std::fmt::Write as _;
    if event.item_id.is_empty() {
        return Err("This event has no local Calendar identifier. Refresh your schedule.".into());
    }
    let mut identifier = String::with_capacity(event.item_id.len());
    for byte in event.item_id.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            identifier.push(char::from(byte));
        } else {
            write!(identifier, "%{byte:02X}")?;
        }
    }
    let date = if event.recurring {
        let start =
            chrono::DateTime::from_timestamp(event.start, 0).ok_or("Invalid event start time")?;
        let wall_time = if event.all_day {
            start.with_timezone(local_zone).naive_local()
        } else {
            start.naive_utc()
        };
        format!("/{}", wall_time.format("%Y%m%dT%H%M%SZ"))
    } else {
        String::new()
    };
    Ok(format!(
        "ical://ekevent{date}/{identifier}?method=show&options=more"
    ))
}
fn open_event(event: &Event) -> Result<()> {
    let url = calendar_event_url(event, &Local)?;
    // Launch Services starts Calendar when needed; no Calendar AppleScript session.
    command("/usr/bin/open", &["-b", "com.apple.iCal", &url])
}
fn installed(bundle: &str) -> bool {
    store::native(json!({"op":"app-installed","bundle":bundle}))
        .ok()
        .and_then(|v| v["installed"].as_bool())
        .unwrap_or(false)
}
pub fn perform(action: &Value, config: &Config) -> Result<()> {
    let op = action["op"].as_str().ok_or("Invalid action")?;
    match op {
        "authorize" => {
            store::native(json!({"op":"authorize"}))?;
            trigger("agenda", "")?;
        }
        "privacy" => command(
            "/usr/bin/open",
            &["x-apple.systempreferences:com.apple.preference.security?Privacy_Calendars"],
        )?,
        "calendar-app" => command("/usr/bin/open", &["-b", "com.apple.iCal"])?,
        "agenda" | "calendars" | "new" => trigger(op, "")?,
        "availability" => trigger("free", "")?,
        "refresh" => {
            store::snapshot(config, Utc::now(), true)?;
            trigger("agenda", "")?;
        }
        "toggle" | "all-calendars" => {
            let mut prefs = store::preferences(config)?;
            if op == "toggle" {
                let id = action["id"]
                    .as_str()
                    .ok_or("No calendar selected")?
                    .to_string();
                if !prefs.hidden.remove(&id) {
                    prefs.hidden.insert(id);
                }
            } else {
                prefs.hidden.clear();
            }
            store::save_preferences(config, &prefs)?;
            trigger("calendars", "")?;
        }
        "open" | "join" => {
            let snapshot = store::snapshot(config, Utc::now(), true)?;
            let event = snapshot.event(action)?;
            if op == "open" {
                open_event(event)?;
            } else {
                let (url, provider) = links::conference(event)
                    .ok_or("This event no longer has a supported conference link.")?;
                if provider == "Google Meet" && !config.meet_browser.is_empty() {
                    if !installed(&config.meet_browser) {
                        return Err("The configured Google Meet browser is not installed. Choose another in Workflow Configuration.".into());
                    }
                    command("/usr/bin/open", &["-b", &config.meet_browser, &url])?;
                } else if provider == "Microsoft Teams" && installed("com.microsoft.teams2") {
                    command("/usr/bin/open", &[&url.replacen("https:", "msteams:", 1)])?;
                } else if provider == "Zoom" && installed("us.zoom.xos") {
                    if let Some(rest) = url.split_once("/j/").map(|(_, s)| s) {
                        let (id, query) = rest.split_once('?').unwrap_or((rest, ""));
                        if !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) {
                            command(
                                "/usr/bin/open",
                                &[&format!("zoommtg://zoom.us/join?confno={id}&{query}")],
                            )?;
                        } else {
                            command("/usr/bin/open", &[&url])?;
                        }
                    } else {
                        command("/usr/bin/open", &[&url])?;
                    }
                } else {
                    command("/usr/bin/open", &[&url])?;
                }
            }
        }
        "create" => {
            // A repeated action for the same exact preview must not create duplicates.
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            action.to_string().hash(&mut hasher);
            let marker = config
                .data
                .join(format!("created-{:x}.json", hasher.finish()));
            if action["start"]
                .as_i64()
                .is_none_or(|s| s < Utc::now().timestamp())
            {
                return Err("The start time has passed. Create a new preview.".into());
            }
            fs::create_dir_all(&config.data)?;
            let mut reservation = fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&marker)
                .map_err(|e| if e.kind() == std::io::ErrorKind::AlreadyExists { "This event preview was already submitted. Check Calendar before trying again.".to_string() } else { e.to_string() })?;
            // The preview is committed only when Return is pressed on its calendar row.
            let result = match store::native(action.clone()) {
                Ok(result) => result,
                Err(error) => {
                    let _ = fs::remove_file(&marker);
                    return Err(error);
                }
            };
            reservation.write_all(&serde_json::to_vec(
                &json!({"saved":Utc::now().timestamp()}),
            )?)?;
            let _ = fs::remove_file(config.cache.join("events.json"));
            let event: Event = serde_json::from_value(result["event"].clone())?;
            open_event(&event)?;
        }
        _ => return Err("Unknown schedule action".into()),
    }
    // Keep stdout empty so private event details do not enter Alfred's action logs.
    Ok(())
}
