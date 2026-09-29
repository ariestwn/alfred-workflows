use crate::{links, model::*, Result};
use chrono::{DateTime, Datelike, Duration, NaiveDate, Timelike, Utc};
use serde_json::{json, Value};

pub fn output(items: Vec<Value>) -> Value {
    json!({"items":items,"skipknowledge":true,"rerun":5})
}
pub fn info(title: &str, subtitle: &str) -> Value {
    json!({"uid":format!("info:{title}"),"title":title,"subtitle":subtitle,"valid":false,"icon":{"path":"icons/info.png"}})
}
pub fn run(title: &str, subtitle: &str, action: Value) -> Value {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    action.to_string().hash(&mut hash);
    json!({"uid":format!("action:{:x}",hash.finish()),"title":title,"subtitle":subtitle,"arg":action.to_string(),"valid":true,"variables":{"SCHEDULE_ACTION":"run"}})
}
pub fn copy(title: &str, subtitle: &str, value: &str) -> Value {
    json!({"uid":format!("copy:{title}"),"title":title,"subtitle":subtitle,"arg":value,"text":{"copy":value,"largetype":value},"valid":true,"variables":{"SCHEDULE_ACTION":"copy"}})
}
pub fn reference(event: &Event, op: &str) -> Value {
    json!({"op":op,"id":event.id,"start":event.start})
}
pub fn visible<'a>(snapshot: &'a Snapshot, prefs: &Preferences, config: &Config) -> Vec<&'a Event> {
    snapshot
        .events
        .iter()
        .filter(|e| {
            !prefs.hidden.contains(&e.calendar_id)
                && e.status != 3
                && (config.show_declined || e.self_status != 3)
                && (config.show_all_day || !e.all_day)
        })
        .collect()
}
fn timing(event: &Event, config: &Config) -> String {
    let start = config.local(event.start);
    let end = config.local(event.end);
    if event.all_day {
        let last = config.local(event.end.saturating_sub(1));
        if last.date_naive() > start.date_naive() {
            format!(
                "{}–{} · All day",
                start.format("%-d %b"),
                last.format("%-d %b")
            )
        } else {
            format!("{} · All day", start.format("%a %-d %b"))
        }
    } else if start.date_naive() != end.date_naive() {
        format!(
            "{}–{}",
            start.format("%a %-d %b %H:%M"),
            end.format("%a %-d %b %H:%M")
        )
    } else {
        format!(
            "{} · {}–{}",
            start.format("%a %-d %b"),
            start.format("%H:%M"),
            end.format("%H:%M")
        )
    }
}
pub fn details(event: &Event, snapshot: &Snapshot, config: &Config) -> String {
    let calendar = snapshot
        .calendar(&event.calendar_id)
        .map(|c| format!("{} · {}", c.title, c.source))
        .unwrap_or_default();
    let mut lines = vec![
        event.title.clone(),
        format!("{} ({})", timing(event, config), config.tz),
        calendar,
    ];
    if !event.response().is_empty() {
        lines.push(event.response().into());
    }
    if !event.location.is_empty() {
        lines.push(format!("Location: {}", event.location));
    }
    if !event.url.is_empty() {
        lines.push(format!("URL: {}", event.url));
    }
    if !event.attendees.is_empty() {
        lines.push(format!("Attendees: {}", attendees(event)));
    }
    if !event.notes.is_empty() {
        lines.push(format!("\n{}", event.notes));
    }
    lines.join("\n")
}
pub fn attendees(event: &Event) -> String {
    event
        .attendees
        .iter()
        .map(|p| {
            let email = p.email.strip_prefix("mailto:").unwrap_or(&p.email);
            if p.name.is_empty() || p.name == email {
                email.to_string()
            } else {
                format!("{} <{}>", p.name, email)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}
pub fn event_row(event: &Event, snapshot: &Snapshot, config: &Config) -> Value {
    let calendar = snapshot
        .calendar(&event.calendar_id)
        .map(|c| c.title.as_str())
        .unwrap_or("Calendar");
    let conference = links::conference(event);
    let mut labels = vec![timing(event, config), calendar.to_string()];
    if !event.response().is_empty() {
        labels.push(event.response().into());
    }
    if !event.attendees.is_empty() {
        labels.push(format!("{} attendees", event.attendees.len()));
    }
    if event.recurring {
        labels.push("↻".into());
    }
    if let Some((_, provider)) = &conference {
        labels.push(provider.to_string());
    }
    let detail = details(event, snapshot, config);
    let mut item = run(&event.title, &labels.join(" · "), reference(event, "open"));
    item["uid"] = json!(format!(
        "{}:{}:{}",
        event.calendar_id, event.id, event.start
    ));
    item["icon"] = json!({"path":event.icon()});
    item["text"] = json!({"copy":detail,"largetype":detail});
    item["mods"] = json!({
        "cmd":{"valid":conference.is_some(),"subtitle":conference.map(|(_,p)| format!("Join {p}")).unwrap_or_else(|| "No supported conference link in this event".into()),"arg":reference(event,"join").to_string(),"variables":{"SCHEDULE_ACTION":"run"}},
        "alt":{"valid":true,"subtitle":"Event actions: join, copy, manage in Calendar…","arg":"","variables":{"SCHEDULE_ACTION":"menu","SCHEDULE_EVENT":reference(event,"open").to_string()}},
        "shift":{"valid":true,"subtitle":"Copy event details","arg":detail,"variables":{"SCHEDULE_ACTION":"copy"}},
        "cmd+shift":{"valid":true,"subtitle":"Copy event title","arg":event.title,"variables":{"SCHEDULE_ACTION":"copy"}},
        "ctrl":{"valid":true,"subtitle":"Copy attendees","arg":attendees(event),"variables":{"SCHEDULE_ACTION":"copy"}}
    });
    item
}
pub fn section(date: NaiveDate, today: NaiveDate, config: &Config) -> String {
    let next_week = config.next_monday(today);
    if date <= today {
        "Today".into()
    } else if date < next_week {
        if date == today + Duration::days(1) {
            "Tomorrow".into()
        } else {
            date.format("%A %-d %b").to_string()
        }
    } else if date < next_week + Duration::days(7) {
        "Next week".into()
    } else if date.year() == today.year() && date.month() == today.month() {
        format!("Rest of {}", date.format("%B"))
    } else {
        date.format("%B %Y").to_string()
    }
}
pub fn query_range<'a>(
    query: &'a str,
    config: &Config,
    now: DateTime<Utc>,
) -> Result<(i64, i64, &'a str)> {
    let today = config.today(now);
    let trimmed = query.trim();
    let (first, rest) = trimmed
        .split_once(char::is_whitespace)
        .unwrap_or((trimmed, ""));
    let (start, days, search) = match first.to_lowercase().as_str() {
        "today" => (today, 1, rest),
        "tomorrow" => (today + Duration::days(1), 1, rest),
        "week" => (
            today,
            7 - i64::from(today.weekday().num_days_from_monday()),
            rest,
        ),
        "month" => {
            let next = if today.month() == 12 {
                NaiveDate::from_ymd_opt(today.year() + 1, 1, 1)
            } else {
                NaiveDate::from_ymd_opt(today.year(), today.month() + 1, 1)
            }
            .unwrap();
            (today, (next - today).num_days(), rest)
        }
        _ => {
            if let Ok(date) = NaiveDate::parse_from_str(first, "%Y-%m-%d") {
                (date, 1, rest)
            } else {
                (today, config.days, trimmed)
            }
        }
    };
    Ok((
        config.midnight(start)?,
        config.midnight(start + Duration::days(days))?,
        search.trim(),
    ))
}
pub fn agenda(
    snapshot: &Snapshot,
    prefs: &Preferences,
    config: &Config,
    query: &str,
    now: DateTime<Utc>,
    next_only: bool,
) -> Result<Value> {
    let today = config.today(now);
    let all = visible(snapshot, prefs, config);
    let upcoming: Vec<_> = all
        .iter()
        .copied()
        .filter(|e| e.end > now.timestamp())
        .collect();
    let next = upcoming
        .iter()
        .copied()
        .find(|e| !e.all_day && !e.excluded());
    if next_only {
        return Ok(output(match next {
            Some(e) => {
                let mut row = event_row(e, snapshot, config);
                if links::conference(e).is_some() {
                    row["arg"] = json!(reference(e, "join").to_string());
                }
                vec![row]
            }
            None => vec![info(
                "No upcoming meetings",
                "Open schedule for all-day events and calendar settings.",
            )],
        }));
    }
    let today_end = config.midnight(today + Duration::days(1))?;
    let today_count = upcoming
        .iter()
        .filter(|e| e.start < today_end && !e.excluded())
        .count();
    let greeting = match now.with_timezone(&config.tz).hour() {
        0..=11 => "Good morning!",
        12..=17 => "Good afternoon!",
        _ => "Good evening!",
    };
    let message = match today_count {
        0 => "Nothing else scheduled for today. Enjoy your free time!".into(),
        1 => "1 event remaining today".into(),
        n => format!("{n} events remaining today"),
    };
    let mut summary = info(greeting, &message);
    summary["mods"] = json!({"alt":{"valid":true,"subtitle":"Choose calendars","arg":json!({"op":"calendars"}).to_string(),"variables":{"SCHEDULE_ACTION":"run"}}});
    let mut items = vec![summary];
    if let Some(event) = next {
        let mut row = event_row(event, snapshot, config);
        row["uid"] = json!(format!("next:{}:{}", event.id, event.start));
        row["title"] = json!(format!(
            "{} · {}",
            if event.start <= now.timestamp() {
                "Now"
            } else {
                "Next up"
            },
            event.title
        ));
        items.push(row);
    }
    let (start, end, search) = query_range(query, config, now)?;
    if start < config.midnight(today)?
        || end > config.midnight(today + Duration::days(config.days))?
    {
        items.push(info("Date outside your schedule range", &format!("Choose a date within the next {} days, or extend Look ahead in Workflow Configuration.",config.days)));
        return Ok(output(items));
    }
    let words: Vec<_> = search.split_whitespace().map(str::to_lowercase).collect();
    let filtered: Vec<_> = upcoming
        .into_iter()
        .filter(|e| {
            e.start < end
                && e.end > start
                && words
                    .iter()
                    .all(|word| e.title.to_lowercase().contains(word))
        })
        .collect();
    let mut previous = String::new();
    for event in filtered.iter().take(250) {
        let group = section(config.local(event.start).date_naive(), today, config);
        if group != previous {
            items.push(info(&group, ""));
            previous = group;
        }
        items.push(event_row(event, snapshot, config));
    }
    if filtered.is_empty() {
        items.push(info(
            if search.is_empty() {
                "No upcoming events in this range"
            } else {
                "No matching event titles"
            },
            "Use scal to choose calendars, or try another date or title.",
        ));
    } else if filtered.len() > 250 {
        items.push(info(
            &format!("Showing 250 of {} events", filtered.len()),
            &format!(
                "Narrow by title or date, for example: {} tomorrow",
                config.keyword
            ),
        ));
    }
    let enabled = snapshot
        .calendars
        .iter()
        .filter(|c| !prefs.hidden.contains(&c.id))
        .count();
    items.push(run(
        &format!("Calendars · {enabled} enabled"),
        "Choose which calendars appear in My Schedule",
        json!({"op":"calendars"}),
    ));
    items.push(run(
        "Create an event…",
        "Set a title, time, duration, and calendar",
        json!({"op":"new"}),
    ));
    items.push(run(
        "Copy availability…",
        "Find free time within your work hours",
        json!({"op":"availability"}),
    ));
    items.push(run(
        "Refresh schedule",
        "Read the latest events from Calendar",
        json!({"op":"refresh"}),
    ));
    Ok(output(items))
}
pub fn calendars(snapshot: &Snapshot, prefs: &Preferences, query: &str) -> Value {
    let mut items = vec![run(
        "Back to My Schedule",
        "Return to your agenda",
        json!({"op":"agenda"}),
    )];
    let mut calendars: Vec<_> = snapshot
        .calendars
        .iter()
        .filter(|c| {
            format!("{} {}", c.title, c.source)
                .to_lowercase()
                .contains(&query.to_lowercase())
        })
        .collect();
    calendars.sort_by_key(|c| (&c.source, &c.title, &c.id));
    for calendar in calendars {
        let enabled = !prefs.hidden.contains(&calendar.id);
        let mut row = run(
            &format!("{} {}", if enabled { "✓" } else { "○" }, calendar.title),
            &format!(
                "{} · {} · ↩ {}",
                calendar.source,
                if calendar.writable {
                    "Writable"
                } else {
                    "Read only"
                },
                if enabled {
                    "Hide calendar"
                } else {
                    "Show calendar"
                }
            ),
            json!({"op":"toggle","id":calendar.id}),
        );
        row["icon"] =
            json!({"path":if enabled { "icons/accepted.png" } else { "icons/event.png" }});
        items.push(row);
    }
    if snapshot.calendars.is_empty() {
        items.push(info(
            "No calendars found",
            "Add your Google, iCloud, or Exchange account in the macOS Calendar app.",
        ));
    }
    items.push(run(
        "Show all calendars",
        "Enable all synced calendars",
        json!({"op":"all-calendars"}),
    ));
    output(items)
}
pub fn menu(event: &Event, snapshot: &Snapshot, config: &Config, query: &str) -> Value {
    let mut items = vec![info(&event.title, &timing(event, config))];
    if let Some((url, provider)) = links::conference(event) {
        items.push(run(
            &format!("Join {provider}"),
            &url,
            reference(event, "join"),
        ));
        items.push(copy("Copy conference link", provider, &url));
    }
    items.push(run(
        "Open in Calendar",
        "Edit, manage invitations, or delete this event in Calendar",
        reference(event, "open"),
    ));
    items.push(copy(
        "Copy event details",
        "Title, time, calendar, attendees, and notes",
        &details(event, snapshot, config),
    ));
    items.push(copy("Copy event title", &event.title, &event.title));
    items.push(copy(
        "Copy attendees",
        "Names and email addresses",
        &attendees(event),
    ));
    // Composing email and sending invitations remain explicit actions in Calendar.
    items.retain(|item| {
        item["title"]
            .as_str()
            .unwrap_or_default()
            .to_lowercase()
            .contains(&query.to_lowercase())
    });
    output(items)
}
