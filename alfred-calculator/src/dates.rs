use crate::{config::Config, Answer, Result};
use chrono::{
    DateTime, Datelike, Duration, FixedOffset, LocalResult, NaiveDate, NaiveDateTime, NaiveTime,
    Offset, TimeZone, Utc,
};
use chrono_tz::Tz;
use std::{collections::BTreeMap, sync::OnceLock};

#[derive(Clone)]
pub enum Zone {
    Named(Tz),
    Fixed(FixedOffset, String),
}
impl Zone {
    pub fn parse(input: &str) -> Result<Self> {
        let key = input
            .to_lowercase()
            .replace('_', " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if matches!(key.as_str(), "cst" | "ist" | "bst") {
            return Err(
                "That abbreviation can mean multiple time zones. Use a city or IANA name.".into(),
            );
        }
        let fixed = match key.as_str() {
            "z" | "utc" | "gmt" => Some(0),
            "pst" => Some(-8 * 3600),
            "pdt" => Some(-7 * 3600),
            "est" => Some(-5 * 3600),
            "edt" => Some(-4 * 3600),
            _ => None,
        };
        if let Some(seconds) = fixed {
            return Ok(Self::Fixed(
                FixedOffset::east_opt(seconds).unwrap(),
                input.to_uppercase(),
            ));
        }
        if let Some(c) = re!(r"(?i)^(?:utc|gmt)([+-])(\d{1,2})(?::?(\d{2}))?$").captures(input) {
            let hours: i32 = c[2].parse()?;
            let minutes: i32 = c
                .get(3)
                .map(|m| m.as_str().parse())
                .transpose()?
                .unwrap_or(0);
            if hours > 23 || minutes > 59 {
                return Err("Invalid UTC offset".into());
            }
            let seconds = (hours * 3600 + minutes * 60) * if &c[1] == "-" { -1 } else { 1 };
            return Ok(Self::Fixed(
                FixedOffset::east_opt(seconds).ok_or("Invalid UTC offset")?,
                input.to_uppercase(),
            ));
        }
        static PLACES: OnceLock<BTreeMap<String, String>> = OnceLock::new();
        let places = PLACES.get_or_init(|| {
            serde_json::from_str(include_str!("../data/places.json")).expect("place aliases")
        });
        let value = places.get(&key).map(String::as_str).unwrap_or(input);
        if let Ok(tz) = value.parse::<Tz>() {
            return Ok(Self::Named(tz));
        }
        let mut candidates: Vec<Tz> = chrono_tz::TZ_VARIANTS
            .iter()
            .copied()
            .filter(|t| {
                let name = t.name().to_lowercase().replace('_', " ");
                name == key || name.rsplit('/').next() == Some(&key)
            })
            .collect();
        candidates.sort_by_key(|z| z.name());
        match candidates.as_slice(){[tz]=>Ok(Self::Named(*tz)),[]=>Err(format!("Unknown place “{input}”. Try a city, airport alias, or IANA zone such as Asia/Jakarta.").into()),_=>Err("Place name is ambiguous; use its full IANA timezone name.".into())}
    }
    pub fn name(&self) -> String {
        match self {
            Self::Named(tz) => tz.name().replace('_', " "),
            Self::Fixed(_, label) => label.clone(),
        }
    }
    fn at(&self, time: DateTime<Utc>) -> DateTime<FixedOffset> {
        match self {
            Self::Named(tz) => time.with_timezone(tz).fixed_offset(),
            Self::Fixed(offset, _) => time.with_timezone(offset),
        }
    }
    fn local(&self, time: NaiveDateTime) -> Result<Vec<DateTime<Utc>>> {
        let result = match self {
            Self::Named(tz) => tz.from_local_datetime(&time).map(|d| d.with_timezone(&Utc)),
            Self::Fixed(offset, _) => offset
                .from_local_datetime(&time)
                .map(|d| d.with_timezone(&Utc)),
        };
        match result {LocalResult::Single(d)=>Ok(vec![d]),LocalResult::Ambiguous(a,b)=>Ok(vec![a,b]),LocalResult::None=>Err("That local time does not exist during the daylight-saving transition. Choose a different time.".into())}
    }
    fn abbreviation(&self, time: DateTime<Utc>) -> String {
        match self {
            Self::Named(tz) => time.with_timezone(tz).format("%Z").to_string(),
            Self::Fixed(_, label) => label.clone(),
        }
    }
}
fn date_answer(date: NaiveDate) -> Answer {
    Answer {
        value: date.format("%A, %-d %B %Y").to_string(),
        raw: date.to_string(),
        detail: String::new(),
        rounded: None,
    }
}
fn time_answer(time: DateTime<Utc>, zone: &Zone, detail: &str) -> Answer {
    let local = zone.at(time);
    Answer {
        value: format!(
            "{} · {}",
            local.format("%a %-d %b %Y, %H:%M"),
            zone.abbreviation(time)
        ),
        raw: local.to_rfc3339(),
        rounded: None,
        detail: if detail.is_empty() {
            zone.name()
        } else {
            format!("{} · {detail}", zone.name())
        },
    }
}
pub fn parse_date(
    input: &str,
    today: NaiveDate,
    next_occurrence: bool,
) -> Result<Option<NaiveDate>> {
    let s = input.trim().to_lowercase();
    match s.as_str() {
        "today" => return Ok(Some(today)),
        "tomorrow" => return Ok(today.succ_opt()),
        "yesterday" => return Ok(today.pred_opt()),
        _ => (),
    }
    if let Ok(date) = NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
        return Ok(Some(date));
    }
    let weekdays = [
        "monday",
        "tuesday",
        "wednesday",
        "thursday",
        "friday",
        "saturday",
        "sunday",
    ];
    if let Some(index) = weekdays.iter().position(|d| *d == s) {
        let delta =
            (index as i64 - i64::from(today.weekday().num_days_from_monday())).rem_euclid(7);
        return Ok(today.checked_add_signed(Duration::days(delta)));
    }
    let cleaned = re!(r"(\d)(?:st|nd|rd|th)\b")
        .replace_all(&s, "$1")
        .replace(',', "");
    let explicit_year = re!(r"\b\d{4}\b").is_match(&cleaned);
    let candidate = if explicit_year {
        cleaned.clone()
    } else {
        format!("{cleaned} {}", today.year())
    };
    for format in ["%B %d %Y", "%b %d %Y", "%d %B %Y", "%d %b %Y"] {
        if let Ok(mut date) = NaiveDate::parse_from_str(&candidate, format) {
            if next_occurrence && !explicit_year && date < today {
                date = NaiveDate::from_ymd_opt(today.year() + 1, date.month(), date.day())
                    .ok_or("That date does not exist next year; include a year.")?;
            }
            return Ok(Some(date));
        }
    }
    Ok(None)
}
fn since_date(input: &str, today: NaiveDate) -> Result<NaiveDate> {
    let input = input.split_whitespace().collect::<Vec<_>>().join(" ");
    let yearless = re!(r"(?i)^(?:\d{1,2}(?:st|nd|rd|th)? [a-z]+|[a-z]+ \d{1,2}(?:st|nd|rd|th)?)$")
        .is_match(&input);
    if yearless {
        // Eight years covers the leap-day gap across a non-leap century.
        for year in (today.year() - 8..=today.year()).rev() {
            if let Some(date) = parse_date(&format!("{input} {year}"), today, false)? {
                if date <= today {
                    return Ok(date);
                }
            }
        }
    } else if let Some(date) = parse_date(&input, today, false)? {
        if date > today {
            return Err("Start date is in the future. Use days until DATE instead.".into());
        }
        return Ok(date);
    }
    Err("Use a date like 21 Sep, 21 September 2025, or 2025-09-21".into())
}

fn clock(input: &str) -> Option<NaiveTime> {
    let s = input.trim().to_uppercase().replace(' ', "");
    if let Some(c) = re!(r"^(\d{1,2})(AM|PM)$").captures(&s) {
        let hour: u32 = c[1].parse().ok()?;
        if !(1..=12).contains(&hour) {
            return None;
        }
        return NaiveTime::from_hms_opt(hour % 12 + if &c[2] == "PM" { 12 } else { 0 }, 0, 0);
    }
    for format in ["%I:%M%p", "%I%p", "%H:%M:%S", "%H:%M"] {
        if let Ok(time) = NaiveTime::parse_from_str(&s, format) {
            return Some(time);
        }
    }
    None
}
fn duration(value: i64, unit: &str) -> Result<Duration> {
    let multiplier = match unit.trim_end_matches('s') {
        "second" => 1,
        "minute" => 60,
        "hour" => 3600,
        "day" => 86400,
        "week" => 604800,
        _ => return Err("Use seconds, minutes, hours, days, or weeks".into()),
    };
    Duration::try_seconds(
        value
            .checked_mul(multiplier)
            .ok_or("Duration is too large")?,
    )
    .ok_or_else(|| "Duration is too large".into())
}

pub fn evaluate(query: &str, config: &Config, now: DateTime<Utc>) -> Result<Option<Vec<Answer>>> {
    let local = Zone::parse(&config.timezone)?;
    let today = local.at(now).date_naive();
    let s = query.trim();
    let lower = s.to_lowercase();
    if matches!(lower.as_str(), "now" | "time") {
        return Ok(Some(vec![time_answer(now, &local, "")]));
    }
    if matches!(lower.as_str(), "today" | "tomorrow" | "yesterday") {
        return Ok(Some(vec![date_answer(
            parse_date(s, today, false)?.ok_or("Date out of range")?,
        )]));
    }
    if let Some(c) = re!(r"(?i)^(?:time\s+)?diff\s+(.+)$").captures(s) {
        let zone = Zone::parse(&c[1])?;
        let diff = i64::from(
            zone.at(now).offset().fix().local_minus_utc()
                - local.at(now).offset().fix().local_minus_utc(),
        );
        let hours = diff.abs() / 3600;
        let minutes = (diff.abs() % 3600) / 60;
        let value = if diff == 0 {
            "Same local time".into()
        } else {
            format!(
                "{hours}h{} {}",
                if minutes > 0 {
                    format!(" {minutes}m")
                } else {
                    String::new()
                },
                if diff > 0 { "ahead" } else { "behind" }
            )
        };
        return Ok(Some(vec![
            Answer {
                value,
                raw: (diff as f64 / 3600.0).to_string(),
                rounded: None,
                detail: format!("{} vs {}", zone.name(), local.name()),
            },
            time_answer(now, &zone, ""),
        ]));
    }
    if let Some(c) =
        re!(r"(?i)^time in ([+-]?\d+) (seconds?|minutes?|hours?|days?|weeks?)(?: in (.+))?$")
            .captures(s)
    {
        let zone = if let Some(place) = c.get(3) {
            Zone::parse(place.as_str())?
        } else {
            local.clone()
        };
        let target = now
            .checked_add_signed(duration(c[1].parse()?, &c[2])?)
            .ok_or("Time out of range")?;
        return Ok(Some(vec![time_answer(target, &zone, "")]));
    }
    if let Some(place) = lower.strip_prefix("time in ") {
        return Ok(Some(vec![time_answer(now, &Zone::parse(place)?, "")]));
    }
    if let Some(c) = re!(r"(?i)^work(hours|days) in (\d{4})$").captures(s) {
        let year: i32 = c[2].parse()?;
        let mut date = NaiveDate::from_ymd_opt(year, 1, 1).ok_or("Invalid year")?;
        let end = NaiveDate::from_ymd_opt(year + 1, 1, 1).ok_or("Invalid year")?;
        let mut count = 0u32;
        while date < end {
            if date.weekday().num_days_from_monday() < 5 {
                count += 1;
            }
            date = date.succ_opt().ok_or("Date out of range")?;
        }
        let n = if &c[1].to_lowercase() == "hours" {
            count * config.work_hours
        } else {
            count
        };
        return Ok(Some(vec![Answer {
            value: format!(
                "{} work{}",
                config.format(&n.to_string()),
                c[1].to_lowercase()
            ),
            raw: n.to_string(),
            rounded: None,
            detail: format!("Mon–Fri · {}h/day · holidays included", config.work_hours),
        }]));
    }
    if let Some(c) = re!(r"(?i)^days?\s+since\b\s*(.*?)\s*(?:\s+to\s+today)?$").captures(s) {
        let start = since_date(&c[1], today)?;
        let n = (today - start).num_days();
        return Ok(Some(vec![Answer {
            value: format!("{n} {}", if n == 1 { "day" } else { "days" }),
            raw: n.to_string(),
            rounded: None,
            detail: format!("{start} → {today}"),
        }]));
    }
    if let Some(target) = lower.strip_prefix("days until ") {
        let target = if target == "end of quarter" {
            let month = ((today.month() - 1) / 3 + 1) * 3;
            let next = if month == 12 {
                NaiveDate::from_ymd_opt(today.year() + 1, 1, 1)
            } else {
                NaiveDate::from_ymd_opt(today.year(), month + 1, 1)
            };
            next.and_then(|d| d.pred_opt()).ok_or("Date out of range")?
        } else {
            parse_date(target, today, true)?.ok_or("Use a date like 31 Mar or 2027-03-31")?
        };
        let n = (target - today).num_days();
        return Ok(Some(vec![Answer {
            value: format!("{n} days"),
            raw: n.to_string(),
            rounded: None,
            detail: format!("{} → {}", today, target),
        }]));
    }
    if let Some(c) =
        re!(r"(?i)^(monday|tuesday|wednesday|thursday|friday|saturday|sunday) in (\d+) weeks?$")
            .captures(s)
    {
        let from = today
            .checked_add_signed(duration(c[2].parse()?, "weeks")?)
            .ok_or("Date out of range")?;
        let date = parse_date(&c[1], from, false)?.ok_or("Invalid weekday")?;
        return Ok(Some(vec![date_answer(date)]));
    }
    if let Some(c) =
        re!(r"(?i)^(.+?)\s*([+-])\s*(\d+)(?:\s+(days?|weeks?|hours?|minutes?|seconds?))?$")
            .captures(s)
    {
        let base = c[1].trim();
        let n: i64 = c[3].parse()?;
        let n = if &c[2] == "-" { -n } else { n };
        if let Some(date) = parse_date(base, today, false)? {
            let unit = c
                .get(4)
                .map(|m| m.as_str())
                .unwrap_or("days")
                .to_lowercase();
            if unit.starts_with("day") || unit.starts_with("week") {
                let date = date
                    .checked_add_signed(duration(n, &unit)?)
                    .ok_or("Date out of range")?;
                return Ok(Some(vec![date_answer(date)]));
            }
            let instant = local.local(date.and_hms_opt(0, 0, 0).unwrap())?[0]
                .checked_add_signed(duration(n, &unit)?)
                .ok_or("Date out of range")?;
            return Ok(Some(vec![time_answer(instant, &local, "")]));
        }
        if let Some(time) = clock(base) {
            let unit = c
                .get(4)
                .map(|m| m.as_str())
                .unwrap_or("hours")
                .to_lowercase();
            let results = local
                .local(today.and_time(time))?
                .into_iter()
                .map(|instant| {
                    let target = instant
                        .checked_add_signed(duration(n, &unit)?)
                        .ok_or("Time out of range")?;
                    Ok(time_answer(target, &local, ""))
                })
                .collect::<Result<Vec<_>>>()?;
            return Ok(Some(results));
        }
    }
    // Split conversion at the final connector, so multi-word city names survive.
    let conversion = re!(r"(?i)^(.+)\s+(?:in|to)\s+(.+)$").captures(s);
    let (base, target) = conversion
        .as_ref()
        .map(|c| (c[1].trim(), Some(c[2].trim())))
        .unwrap_or((s, None));
    if let Ok(instant) = DateTime::parse_from_rfc3339(base) {
        let target = target
            .map(Zone::parse)
            .transpose()?
            .unwrap_or(local.clone());
        return Ok(Some(vec![time_answer(
            instant.with_timezone(&Utc),
            &target,
            "",
        )]));
    }
    // 5pm ldn in sf; also accept an explicit ISO date to disambiguate DST.
    if let Some(c) = re!(
        r"(?i)^(?:(\d{4}-\d{2}-\d{2})\s+)?(\d{1,2}(?::\d{2}(?::\d{2})?)?\s*(?:am|pm)?)(?:\s+(.+))?$"
    )
    .captures(base)
    {
        if let Some(time) = clock(c[2].trim()) {
            let source = if let Some(place) = c.get(3) {
                Zone::parse(place.as_str())?
            } else {
                local.clone()
            };
            let day = if let Some(date) = c.get(1) {
                NaiveDate::parse_from_str(date.as_str(), "%Y-%m-%d")?
            } else {
                source.at(now).date_naive()
            };
            let target = target
                .map(Zone::parse)
                .transpose()?
                .unwrap_or(source.clone());
            let instants = source.local(day.and_time(time))?;
            let ambiguous = instants.len() > 1;
            let results = instants
                .into_iter()
                .map(|instant| {
                    time_answer(
                        instant,
                        &target,
                        &format!(
                            "{} {} in {}{}",
                            day,
                            time.format("%H:%M"),
                            source.name(),
                            if ambiguous {
                                format!(
                                    " · ambiguous clock time ({})",
                                    source.abbreviation(instant)
                                )
                            } else {
                                String::new()
                            }
                        ),
                    )
                })
                .collect();
            return Ok(Some(results));
        }
    }
    if let Some(date) = parse_date(s, today, false)? {
        return Ok(Some(vec![date_answer(date)]));
    }
    if re!(r"^\d{4}-\d{2}-\d{2}$").is_match(s) {
        return Err("Invalid calendar date".into());
    }
    Ok(None)
}
