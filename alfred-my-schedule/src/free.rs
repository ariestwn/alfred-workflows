use crate::{model::*, view, Result};
use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc};
use serde_json::Value;

pub fn slots(
    snapshot: &Snapshot,
    prefs: &Preferences,
    config: &Config,
    date: NaiveDate,
    now: DateTime<Utc>,
) -> Result<Vec<(i64, i64)>> {
    let local_time = |hour: u32| -> Result<i64> {
        let date_time = if hour == 24 {
            (date + Duration::days(1)).and_hms_opt(0, 0, 0)
        } else {
            date.and_hms_opt(hour, 0, 0)
        }
        .ok_or("Invalid work hours")?;
        config
            .tz
            .from_local_datetime(&date_time)
            .single()
            .map(|d| d.timestamp())
            .ok_or_else(|| {
                "Work hours overlap a daylight-saving transition; choose another date.".into()
            })
    };
    let start = local_time(config.work_start)?.max(now.timestamp());
    let end = local_time(config.work_end)?;
    if start >= end {
        return Ok(vec![]);
    }
    // All-day busy events still block availability when hidden from the agenda.
    let mut busy: Vec<_> = snapshot
        .events
        .iter()
        .filter(|e| {
            !prefs.hidden.contains(&e.calendar_id)
                && !e.excluded()
                && e.availability != 1
                && e.start < end
                && e.end > start
        })
        .map(|e| (e.start.max(start), e.end.min(end)))
        .collect();
    busy.sort_unstable();
    let mut cursor = start;
    let mut free = vec![];
    for (a, b) in busy {
        if a - cursor >= config.minimum_free * 60 {
            free.push((cursor, a));
        }
        cursor = cursor.max(b);
    }
    if end - cursor >= config.minimum_free * 60 {
        free.push((cursor, end));
    }
    Ok(free)
}
pub fn preview(
    query: &str,
    snapshot: &Snapshot,
    prefs: &Preferences,
    config: &Config,
    now: DateTime<Utc>,
) -> Result<Value> {
    let today = config.today(now);
    let query = query.trim().to_lowercase();
    let (date, days) = match query.as_str() {
        "" | "today" => (today, 1),
        "tomorrow" => (today + Duration::days(1), 1),
        "week" => (today, 7),
        _ => (
            NaiveDate::parse_from_str(&query, "%Y-%m-%d")
                .map_err(|_| "Use sfree today, tomorrow, week, or YYYY-MM-DD.")?,
            1,
        ),
    };
    if date < today || date + Duration::days(days) > today + Duration::days(config.days) {
        return Err("Choose a date inside your configured look-ahead range.".into());
    }
    let mut all = vec![];
    let mut items = vec![];
    for offset in 0..days {
        let date = date + Duration::days(offset);
        if days > 1 && date.weekday().num_days_from_monday() >= 5 {
            continue;
        }
        let slots = slots(snapshot, prefs, config, date, now)?;
        let times = slots
            .iter()
            .map(|(a, b)| {
                format!(
                    "{}–{}",
                    config.local(*a).format("%H:%M"),
                    config.local(*b).format("%H:%M")
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let line = format!(
            "{}: {} ({})",
            date.format("%a %-d %b %Y"),
            if times.is_empty() {
                "No free slots"
            } else {
                &times
            },
            config.tz
        );
        items.push(view::copy(
            &date.format("%A %-d %b").to_string(),
            &line,
            &line,
        ));
        all.push(line);
    }
    let summary = format!("Availability\n{}", all.join("\n"));
    items.insert(
        0,
        view::copy(
            "Copy availability",
            &format!(
                "Enabled calendars · {:02}:00–{:02}:00 · minimum {} minutes",
                config.work_start, config.work_end, config.minimum_free
            ),
            &summary,
        ),
    );
    Ok(view::output(items))
}
