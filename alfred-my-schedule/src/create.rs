use crate::{links, model::*, view, Result};
use chrono::{DateTime, Duration, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use serde_json::{json, Value};

pub fn parse_time(text: &str, config: &Config, now: DateTime<Utc>) -> Result<i64> {
    if let Ok(time) = DateTime::parse_from_rfc3339(text) {
        return Ok(time.timestamp());
    }
    let (day, clock) = text
        .trim()
        .rsplit_once(' ')
        .ok_or("Use a start such as tomorrow 14:00 or 2026-09-20 14:00.")?;
    let date = match day.to_lowercase().as_str() {
        "today" => config.today(now),
        "tomorrow" => config.today(now) + Duration::days(1),
        _ => NaiveDate::parse_from_str(day, "%Y-%m-%d")
            .map_err(|_| "Use today, tomorrow, or YYYY-MM-DD for the event date.")?,
    };
    let clock = NaiveTime::parse_from_str(clock, "%H:%M")
        .map_err(|_| "Use a 24-hour time such as 14:00.")?;
    match config.tz.from_local_datetime(&NaiveDateTime::new(date,clock)) {
        LocalResult::Single(time) => Ok(time.timestamp()),
        LocalResult::Ambiguous(_,_) => Err("This time happens twice during a daylight-saving change. Use an ISO timestamp with an explicit offset.".into()),
        LocalResult::None => Err("This local time does not exist during the daylight-saving change.".into()),
    }
}
pub fn preview(
    query: &str,
    snapshot: &Snapshot,
    config: &Config,
    now: DateTime<Utc>,
) -> Result<Value> {
    if query.trim().is_empty() {
        let mut example = view::info(
            "Create an event",
            "Title | start | minutes | calendar (optional)",
        );
        example["autocomplete"] = json!("Focus time | tomorrow 14:00 | 60");
        return Ok(view::output(vec![
            example,
            view::info(
                "Example: Focus time | tomorrow 14:00 | 60",
                "Select the preview to save. Extra fields: | location | URL | notes",
            ),
        ]));
    }
    let fields: Vec<_> = query.split('|').map(str::trim).collect();
    if fields.len() < 3 {
        return Ok(view::output(vec![view::info(
            "Title | start | minutes",
            "Example: Focus time | tomorrow 14:00 | 60",
        )]));
    }
    if fields.len() > 7 {
        return Err("Use at most seven fields: title | start | minutes | calendar | location | URL | notes.".into());
    }
    let title = fields[0];
    if title.is_empty() || title.chars().count() > 500 {
        return Err("Give the event a title of 1–500 characters.".into());
    }
    let start = parse_time(fields[1], config, now)?;
    if start < now.timestamp() {
        return Err("Choose a start time in the future.".into());
    }
    let minutes: i64 = fields[2]
        .parse()
        .map_err(|_| "Duration must be a number of minutes, such as 30 or 60.")?;
    if !(1..=10080).contains(&minutes) {
        return Err("Duration must be between 1 minute and 7 days.".into());
    }
    let end = start.checked_add(minutes * 60).ok_or("Date out of range")?;
    let chosen = fields.get(3).copied().unwrap_or_default();
    let location = fields.get(4).copied().unwrap_or_default();
    let url = fields.get(5).copied().unwrap_or_default();
    let notes = fields.get(6).copied().unwrap_or_default();
    if !url.is_empty() && links::host(url).is_none() {
        return Err("Use an HTTPS URL for the event link.".into());
    }
    let mut calendars: Vec<_> = snapshot
        .calendars
        .iter()
        .filter(|c| {
            c.writable
                && (chosen.is_empty() || c.title.eq_ignore_ascii_case(chosen) || c.id == chosen)
        })
        .collect();
    calendars.sort_by_key(|c| (c.id != snapshot.default_calendar, &c.source, &c.title));
    let mut items = vec![];
    for calendar in calendars {
        let request = json!({"op":"create","title":title,"start":start,"end":end,"calendar_id":calendar.id,"location":location,"url":url,"notes":notes,"reminder":-1});
        let mut item = view::run(
            &format!("Create “{title}” in {}", calendar.title),
            &format!(
                "{}–{} · {} · ↩ Save event",
                config.local(start).format("%a %-d %b %H:%M"),
                config.local(end).format("%H:%M"),
                calendar.source
            ),
            request,
        );
        item["text"] = json!({"largetype":format!("{title}\n{}–{}\n{} · {}\n{location}\n{url}\n{notes}",config.local(start),config.local(end),calendar.title,calendar.source)});
        items.push(item);
    }
    if items.is_empty() {
        items.push(view::info(
            "No matching writable calendar",
            "Leave the calendar field empty to choose, or use its exact name.",
        ));
    }
    Ok(view::output(items))
}
