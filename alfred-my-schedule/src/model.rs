use crate::Result;
use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, env, path::PathBuf};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Calendar {
    pub id: String,
    pub title: String,
    pub source: String,
    pub color: String,
    pub writable: bool,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Attendee {
    pub name: String,
    pub email: String,
    pub status: i64,
    pub is_self: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Event {
    pub id: String,
    #[serde(default)]
    pub uid: String,
    #[serde(default)]
    pub item_id: String,
    pub calendar_id: String,
    pub title: String,
    pub start: i64,
    pub end: i64,
    #[serde(default)]
    pub all_day: bool,
    #[serde(default)]
    pub recurring: bool,
    #[serde(default)]
    pub status: i64,
    #[serde(default = "no_status")]
    pub self_status: i64,
    #[serde(default)]
    pub availability: i64,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub attendees: Vec<Attendee>,
}
fn no_status() -> i64 {
    -1
}
impl Event {
    pub fn excluded(&self) -> bool {
        self.status == 3 || self.self_status == 3
    }
    pub fn response(&self) -> &'static str {
        match self.self_status {
            1 => "RSVP pending",
            2 => "Accepted",
            3 => "Declined",
            4 => "Tentative",
            _ => "",
        }
    }
    pub fn icon(&self) -> &'static str {
        match self.self_status {
            1 => "icons/pending.png",
            2 => "icons/accepted.png",
            3 => "icons/declined.png",
            4 => "icons/tentative.png",
            _ => "icons/event.png",
        }
    }
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Snapshot {
    pub calendars: Vec<Calendar>,
    pub events: Vec<Event>,
    pub default_calendar: String,
    pub fetched: i64,
    pub from: i64,
    pub until: i64,
}
impl Snapshot {
    pub fn calendar(&self, id: &str) -> Option<&Calendar> {
        self.calendars.iter().find(|c| c.id == id)
    }
    pub fn event(&self, value: &serde_json::Value) -> Result<&Event> {
        self.events
            .iter()
            .find(|e| {
                Some(e.id.as_str()) == value["id"].as_str()
                    && Some(e.start) == value["start"].as_i64()
            })
            .ok_or_else(|| "This event is no longer available. Refresh your schedule.".into())
    }
}
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Preferences {
    pub hidden: BTreeSet<String>,
}
#[derive(Clone)]
pub struct Config {
    pub tz: Tz,
    pub days: i64,
    pub cache: PathBuf,
    pub data: PathBuf,
    pub keyword: String,
    pub meet_browser: String,
    pub show_declined: bool,
    pub show_all_day: bool,
    pub work_start: u32,
    pub work_end: u32,
    pub minimum_free: i64,
}
impl Config {
    pub fn load() -> Result<Self> {
        let home = PathBuf::from(env::var("HOME")?);
        let zone = env::var("SCHEDULE_TIMEZONE")
            .ok()
            .filter(|s| !s.is_empty() && s != "auto")
            .or_else(|| iana_time_zone::get_timezone().ok())
            .unwrap_or_else(|| "UTC".into());
        let integer = |name: &str, default: i64, min: i64, max: i64| {
            env::var(name)
                .ok()
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(default)
                .clamp(min, max)
        };
        let config = Self {
            tz: zone.parse().map_err(|_| "Use an IANA timezone such as Asia/Jakarta in Workflow Configuration.")?,
            days: integer("SCHEDULE_DAYS", 90, 1, 365),
            cache: env::var_os("alfred_workflow_cache").map(PathBuf::from).unwrap_or_else(|| home.join("Library/Caches/com.runningwithcrayons.Alfred/Workflow Data/com.ariestwn.my-schedule-rust")),
            data: env::var_os("alfred_workflow_data").map(PathBuf::from).unwrap_or_else(|| home.join("Library/Application Support/Alfred/Workflow Data/com.ariestwn.my-schedule-rust")),
            keyword: env::var("SCHEDULE_KEYWORD").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "schedule".into()),
            meet_browser: env::var("SCHEDULE_MEET_BROWSER").unwrap_or_default(),
            show_declined: env::var("SCHEDULE_SHOW_DECLINED").as_deref() == Ok("1"),
            show_all_day: env::var("SCHEDULE_SHOW_ALL_DAY").as_deref() != Ok("0"),
            work_start: integer("SCHEDULE_WORK_START", 9, 0, 23) as u32,
            work_end: integer("SCHEDULE_WORK_END", 17, 1, 24) as u32,
            minimum_free: integer("SCHEDULE_MINIMUM_FREE", 30, 5, 480),
        };
        if config.work_end <= config.work_start {
            return Err("Workday end must be after workday start.".into());
        }
        Ok(config)
    }
    pub fn local(&self, timestamp: i64) -> DateTime<Tz> {
        self.tz
            .timestamp_opt(timestamp, 0)
            .single()
            .expect("validated timestamp")
    }
    pub fn midnight(&self, date: NaiveDate) -> Result<i64> {
        self.tz
            .from_local_datetime(&date.and_hms_opt(0, 0, 0).ok_or("Invalid date")?)
            .earliest()
            .map(|d| d.timestamp())
            .ok_or_else(|| "This date has no local midnight in your timezone.".into())
    }
    pub fn today(&self, now: DateTime<Utc>) -> NaiveDate {
        now.with_timezone(&self.tz).date_naive()
    }
    pub fn next_monday(&self, date: NaiveDate) -> NaiveDate {
        date + Duration::days(7 - i64::from(date.weekday().num_days_from_monday()))
    }
}
