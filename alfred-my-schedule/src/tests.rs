use super::*;
use chrono::{Duration, NaiveDate, TimeZone};

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 13, 10, 0, 0).unwrap()
}
fn config(temp: &tempfile::TempDir) -> Config {
    Config {
        tz: chrono_tz::Asia::Jakarta,
        days: 90,
        cache: temp.path().join("cache"),
        data: temp.path().join("data"),
        keyword: "schedule".into(),
        meet_browser: String::new(),
        show_declined: false,
        show_all_day: true,
        work_start: 9,
        work_end: 17,
        minimum_free: 30,
    }
}
fn event(id: &str, title: &str, start: &str, minutes: i64) -> Event {
    let start = chrono::DateTime::parse_from_rfc3339(start)
        .unwrap()
        .timestamp();
    serde_json::from_value(
        json!({"id":id,"calendar_id":"work","title":title,"start":start,"end":start+minutes*60,
        "url":"https://meet.google.com/abc-defg-hij","self_status":2}),
    )
    .unwrap()
}
fn snapshot() -> Snapshot {
    Snapshot {
        calendars: vec![
            Calendar {
                id: "work".into(),
                title: "Work".into(),
                source: "Google".into(),
                writable: true,
                ..Default::default()
            },
            Calendar {
                id: "personal".into(),
                title: "Work".into(),
                source: "iCloud".into(),
                writable: true,
                ..Default::default()
            },
        ],
        events: vec![
            event(
                "weekly",
                "Weekly product sync",
                "2026-09-14T16:00:00+07:00",
                60,
            ),
            event(
                "ads",
                "Report Ads Kitabisa ORG",
                "2026-09-16T10:00:00+07:00",
                60,
            ),
            event(
                "weekly",
                "Weekly product sync",
                "2026-09-21T16:00:00+07:00",
                60,
            ),
        ],
        default_calendar: "work".into(),
        ..Default::default()
    }
}
#[test]
fn calendar_links_select_the_local_item_and_exact_timed_occurrence() {
    let mut e = event(
        "remote-event-id",
        "Recurring meeting",
        "2026-09-23T03:00:00+07:00",
        60,
    );
    e.item_id = "local-item-id".into();
    e.uid = "external-uid".into();
    assert_eq!(
        actions::calendar_event_url(&e, &chrono_tz::Asia::Jakarta).unwrap(),
        "ical://ekevent/local-item-id?method=show&options=more"
    );
    e.recurring = true;
    assert_eq!(
        actions::calendar_event_url(&e, &chrono_tz::Asia::Jakarta).unwrap(),
        "ical://ekevent/20260922T200000Z/local-item-id?method=show&options=more"
    );
    e.start += 7 * 86400;
    assert_eq!(
        actions::calendar_event_url(&e, &chrono_tz::America::New_York).unwrap(),
        "ical://ekevent/20260929T200000Z/local-item-id?method=show&options=more"
    );
}

#[test]
fn calendar_links_preserve_all_day_dates_and_encode_identifiers() {
    let mut e = event("remote", "All day", "2026-09-23T00:00:00+07:00", 1440);
    e.recurring = true;
    e.all_day = true;
    e.item_id = "id/with ?#%&".into();
    assert_eq!(
        actions::calendar_event_url(&e, &chrono_tz::Asia::Jakarta).unwrap(),
        "ical://ekevent/20260923T000000Z/id%2Fwith%20%3F%23%25%26?method=show&options=more"
    );
    e.start = i64::MAX;
    assert!(actions::calendar_event_url(&e, &Utc).is_err());
    e.item_id.clear();
    assert!(actions::calendar_event_url(&e, &Utc).is_err());
}

#[test]
fn agenda_has_summary_dynamic_sections_and_filters_titles() {
    let temp = tempfile::tempdir().unwrap();
    let c = config(&temp);
    let s = snapshot();
    let all = view::agenda(&s, &Preferences::default(), &c, "", now(), false).unwrap();
    let titles: Vec<_> = all["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["title"].as_str())
        .collect();
    assert!(titles.contains(&"Good afternoon!"));
    assert!(titles.contains(&"Next week"));
    assert!(titles.contains(&"Rest of September"));
    assert_eq!(all["items"][0]["valid"], false);
    // Every row, including section labels and footer actions, retains selection on rerun.
    let ids: std::collections::BTreeSet<_> = all["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["uid"].as_str().expect("stable row ID"))
        .collect();
    assert_eq!(ids.len(), all["items"].as_array().unwrap().len());
    assert!(all["items"][0]["subtitle"]
        .as_str()
        .unwrap()
        .contains("Nothing else"));
    let search = view::agenda(&s, &Preferences::default(), &c, "REPORT org", now(), false).unwrap();
    let rows: Vec<_> = search["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["title"] == "Report Ads Kitabisa ORG")
        .collect();
    assert_eq!(rows.len(), 1);
    assert!(!search["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["title"] == "Weekly product sync"));
    let tomorrow = view::agenda(&s, &Preferences::default(), &c, "tomorrow", now(), false).unwrap();
    assert!(!tomorrow.to_string().contains("Report Ads Kitabisa"));
}
#[test]
fn month_and_year_group_boundaries_follow_calendar_weeks() {
    let temp = tempfile::tempdir().unwrap();
    let c = config(&temp);
    let day = |y, m, d| NaiveDate::from_ymd_opt(y, m, d).unwrap();
    assert_eq!(
        view::section(day(2026, 9, 14), day(2026, 9, 13), &c),
        "Next week"
    );
    assert_eq!(
        view::section(day(2026, 9, 15), day(2026, 9, 14), &c),
        "Tomorrow"
    );
    assert_eq!(
        view::section(day(2026, 9, 16), day(2026, 9, 14), &c),
        "Wednesday 16 Sep"
    );
    assert_eq!(
        view::section(day(2026, 10, 12), day(2026, 9, 13), &c),
        "October 2026"
    );
    assert_eq!(
        view::section(day(2027, 1, 3), day(2026, 12, 27), &c),
        "Next week"
    );
    assert_eq!(
        view::section(day(2027, 1, 20), day(2026, 12, 27), &c),
        "January 2027"
    );
}
#[test]
fn expired_declined_cancelled_and_disabled_calendars_do_not_become_next() {
    let temp = tempfile::tempdir().unwrap();
    let c = config(&temp);
    let mut s = snapshot();
    s.events[0].self_status = 3;
    s.events[1].status = 3;
    let row = view::agenda(&s, &Preferences::default(), &c, "", now(), true).unwrap();
    let action: Value = serde_json::from_str(row["items"][0]["arg"].as_str().unwrap()).unwrap();
    assert_eq!(action["start"], s.events[2].start);
    assert_eq!(action["op"], "join");
    let mut prefs = Preferences::default();
    prefs.hidden.insert("work".into());
    let empty = view::agenda(&s, &prefs, &c, "", now(), true).unwrap();
    assert_eq!(empty["items"][0]["title"], "No upcoming meetings");
    let expired = view::agenda(
        &snapshot(),
        &Preferences::default(),
        &c,
        "",
        now() + Duration::days(20),
        true,
    )
    .unwrap();
    assert_eq!(expired["items"][0]["valid"], false);
}
#[test]
fn recurring_events_have_distinct_actions_and_clipboard_payloads() {
    let temp = tempfile::tempdir().unwrap();
    let c = config(&temp);
    let s = snapshot();
    let a = view::event_row(&s.events[0], &s, &c);
    let b = view::event_row(&s.events[2], &s, &c);
    assert_ne!(a["uid"], b["uid"]);
    assert_ne!(a["arg"], b["arg"]);
    assert_eq!(a["mods"]["alt"]["arg"], "");
    assert_eq!(a["mods"]["alt"]["variables"]["SCHEDULE_ACTION"], "menu");
    assert_eq!(a["mods"]["shift"]["variables"]["SCHEDULE_ACTION"], "copy");
    assert_eq!(a["mods"]["cmd+shift"]["arg"], "Weekly product sync");
    assert!(a["text"]["copy"].as_str().unwrap().contains("16:00–17:00"));
    assert_eq!(
        s.event(&view::reference(&s.events[2], "open"))
            .unwrap()
            .start,
        s.events[2].start
    );
}
#[test]
fn conference_links_are_detected_in_all_fields_and_hosts_are_validated() {
    let mut e = snapshot().events.remove(0);
    for (url, label) in [
        ("https://us02web.zoom.us/j/123?pwd=secret", "Zoom"),
        ("https://meet.google.com/abc-defg-hij", "Google Meet"),
        (
            "https://teams.microsoft.com/l/meetup-join/test",
            "Microsoft Teams",
        ),
        ("https://app.slack.com/huddle/work/call", "Slack Huddle"),
        ("https://example.webex.com/meet/room", "Webex"),
        ("https://facetime.apple.com/join#v=1", "FaceTime"),
        ("https://join.skype.com/room", "Skype"),
        ("https://bluejeans.com/123", "BlueJeans"),
        ("https://chime.aws/123", "Amazon Chime"),
        ("https://whereby.com/room", "Whereby"),
        ("https://meet.jit.si/room", "Jitsi"),
        ("https://around.co/r/room", "Around"),
        ("https://chorus.ai/meet/room", "Chorus"),
        ("https://riverside.fm/studio/room", "Riverside"),
        ("https://streamyard.com/room", "StreamYard"),
    ] {
        e.url.clear();
        e.location.clear();
        e.notes = format!("Join here: <{url}>.");
        assert_eq!(links::conference(&e), Some((url.to_string(), label)));
        e.notes.clear();
        e.location = url.into();
        assert_eq!(links::conference(&e).unwrap().1, label);
    }
    for url in [
        "https://zoom.us.evil.test/j/123",
        "https://evilzoom.us/j/123",
        "https://zoom.us@evil.test/j/123",
        "javascript:alert(1)",
        "file:///tmp/a",
        "https://evil.test/?next=https://zoom.us/j/123",
        "https://zoom.us\\@evil.test/x",
    ] {
        e.url = url.into();
        e.location.clear();
        e.notes.clear();
        assert!(links::conference(&e).is_none(), "{url}");
    }
}
#[test]
fn availability_merges_overlap_and_includes_hidden_all_day_busy_events() {
    let temp = tempfile::tempdir().unwrap();
    let mut c = config(&temp);
    let mut s = snapshot();
    s.events = vec![
        event("a", "A", "2026-09-14T10:00:00+07:00", 90),
        event("b", "B", "2026-09-14T11:00:00+07:00", 120),
        event("c", "C", "2026-09-14T14:00:00+07:00", 60),
    ];
    let date = NaiveDate::from_ymd_opt(2026, 9, 14).unwrap();
    let slots = free::slots(&s, &Preferences::default(), &c, date, now()).unwrap();
    let times: Vec<_> = slots
        .iter()
        .map(|(a, b)| {
            (
                c.local(*a).format("%H:%M").to_string(),
                c.local(*b).format("%H:%M").to_string(),
            )
        })
        .collect();
    assert_eq!(
        times,
        vec![
            ("09:00".into(), "10:00".into()),
            ("13:00".into(), "14:00".into()),
            ("15:00".into(), "17:00".into())
        ]
    );
    let mut all_day = event("all", "Busy day", "2026-09-14T00:00:00+07:00", 1440);
    all_day.all_day = true;
    s.events.push(all_day);
    c.show_all_day = false;
    assert!(free::slots(&s, &Preferences::default(), &c, date, now())
        .unwrap()
        .is_empty());
    s.events.last_mut().unwrap().availability = 1;
    assert_eq!(
        free::slots(&s, &Preferences::default(), &c, date, now()).unwrap(),
        slots
    );
    s.events.last_mut().unwrap().availability = 0;
    s.events.last_mut().unwrap().self_status = 3;
    assert_eq!(
        free::slots(&s, &Preferences::default(), &c, date, now()).unwrap(),
        slots
    );
}
#[test]
fn create_preview_is_explicit_and_dst_invalid_times_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let mut c = config(&temp);
    let s = snapshot();
    let result=create::preview("Focus time | tomorrow 14:00 | 60 | Work | Office | https://meet.google.com/abc-defg-hij | Notes",&s,&c,now()).unwrap();
    assert_eq!(result["items"].as_array().unwrap().len(), 2);
    let action: Value = serde_json::from_str(result["items"][0]["arg"].as_str().unwrap()).unwrap();
    assert_eq!(action["title"], "Focus time");
    assert_eq!(action["calendar_id"], "work");
    assert_eq!(action["location"], "Office");
    assert_eq!(
        action["end"].as_i64().unwrap() - action["start"].as_i64().unwrap(),
        3600
    );
    assert!(!c.data.exists());
    for query in [
        "Test | tomorrow 14:00 | 0",
        "Test | tomorrow 25:00 | 60",
        "Test | tomorrow 14:00 | -1",
        "Test | tomorrow 14:00 | 60 | Work | Home | javascript:alert(1)",
    ] {
        assert!(create::preview(query, &s, &c, now()).is_err(), "{query}");
    }
    c.tz = chrono_tz::America::New_York;
    assert!(create::parse_time("2026-03-08 02:30", &c, now()).is_err());
    assert!(create::parse_time("2026-11-01 01:30", &c, now()).is_err());
    assert!(create::parse_time("2026-11-01T01:30:00-04:00", &c, now()).is_ok());
}
#[test]
fn calendar_selection_uses_identifiers_and_persists_privately() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let c = config(&temp);
    let s = snapshot();
    let mut prefs = Preferences::default();
    prefs.hidden.insert("personal".into());
    store::save_preferences(&c, &prefs).unwrap();
    assert_eq!(store::preferences(&c).unwrap().hidden, prefs.hidden);
    assert_eq!(
        std::fs::metadata(c.data.join("preferences.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let list = view::calendars(&s, &prefs, "Work");
    assert_eq!(list["items"][1]["title"], "✓ Work");
    assert_eq!(list["items"][2]["title"], "○ Work");
    let a: Value = serde_json::from_str(list["items"][1]["arg"].as_str().unwrap()).unwrap();
    let b: Value = serde_json::from_str(list["items"][2]["arg"].as_str().unwrap()).unwrap();
    assert_ne!(a["id"], b["id"]);
}
#[test]
fn thousands_of_events_are_bounded_and_can_be_searched() {
    let temp = tempfile::tempdir().unwrap();
    let c = config(&temp);
    let mut s = snapshot();
    s.events = (0..10000)
        .map(|i| {
            let mut e = s.events[0].clone();
            e.id = i.to_string();
            e.title = format!("Meeting {i}");
            e
        })
        .collect();
    let result = view::agenda(&s, &Preferences::default(), &c, "", now(), false).unwrap();
    assert!(result["items"].as_array().unwrap().len() < 265);
    assert!(result.to_string().contains("Showing 250 of 10000"));
    let search = view::agenda(&s, &Preferences::default(), &c, "9999", now(), false).unwrap();
    assert!(search.to_string().contains("Meeting 9999"));
}
