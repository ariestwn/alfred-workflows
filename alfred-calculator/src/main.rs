macro_rules! re {
    ($pattern:literal) => {{
        static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
        RE.get_or_init(|| regex::Regex::new($pattern).expect("valid regex"))
    }};
}
mod config;
mod currency;
mod dates;
mod math;
mod percent;
mod rates;
#[cfg(test)]
mod tests;

use chrono::Utc;
use config::Config;
use math::Math;
use rates::Rates;
use serde_json::{json, Value};
use std::io::{self, Write};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Debug)]
pub struct Answer {
    pub value: String,
    pub raw: String,
    pub detail: String,
    pub rounded: Option<String>,
}
impl Answer {
    fn item(&self, question: &str, note: &str) -> Value {
        let combined = format!("{question} = {}", self.value);
        let shortcuts = if self.rounded.is_some() {
            "↩ Copy · ⇧↩ Round · ⌘↩ Raw · ⌥↩ Paste"
        } else {
            "↩ Copy · ⌘↩ Raw · ⌥↩ Paste"
        };
        let subtitle = [self.detail.as_str(), note, shortcuts]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
        let mut item = json!({"title":self.value,"subtitle":subtitle,"valid":true,"arg":self.value,
        "text":{"copy":self.value,"largetype":combined},"variables":{"CALC_ACTION":"copy"},
        "mods":{
            "cmd":{"valid":true,"subtitle":format!("Copy unformatted: {}",self.raw),"arg":self.raw},
            "alt":{"valid":true,"subtitle":"Paste formatted answer into the active app","arg":self.value,"variables":{"CALC_ACTION":"paste"}}
        }});
        if let Some(rounded) = &self.rounded {
            item["mods"]["shift"] = json!({
                "valid": true,
                "subtitle": format!("Copy rounded: {rounded}"),
                "arg": rounded,
                "variables": {"CALC_ACTION": "copy"}
            });
        }
        item
    }
}
pub fn info(title: &str, subtitle: &str) -> Value {
    json!({"title":title,"subtitle":subtitle,"valid":false})
}
fn examples() -> Value {
    let pairs = [
        ("Math and percentages", "52% of 900"),
        ("Units", "10ft in m"),
        ("Fiat and crypto", "100 usd in idr"),
        ("City time conversion", "5pm ldn in sf"),
        ("Calendar dates", "monday in 3 weeks"),
        ("Days remaining", "days until 31 Mar"),
        ("Days elapsed", "days since 21 Sep"),
        ("Tip and total", "15% tip on 42"),
        ("Readable durations", "145 mins to timespan"),
        ("Work schedule", "workhours in 2023"),
        ("Design dimensions", "2 inches in px at 72 ppi"),
        ("Compound growth", "123 at 7% after 3 years"),
    ];
    json!({"items":pairs.into_iter().map(|(title,q)|json!({"title":title,"subtitle":q,"autocomplete":q,"valid":false})).collect::<Vec<_>>(),"skipknowledge":true})
}
fn filter(query: &str, fx: bool, config: &Config, now: chrono::DateTime<Utc>) -> Value {
    if query.trim().is_empty() && !fx {
        return examples();
    }
    if query.len() > 2048 {
        return json!({"items":[info("Expression too long","Use up to 2,048 bytes") ]});
    }
    let rates = Rates::new(config);
    let math = Math::new(config, &rates);
    let result: Result<Vec<Value>> = (|| {
        if fx {
            return currency::browse(query, config, &math, &rates);
        }
        let answers = match dates::evaluate(query, config, now)? {
            Some(a) => a,
            None => math.evaluate(query)?,
        };
        let (_, notes) = rates.status();
        Ok(answers.into_iter().map(|a| a.item(query, &notes)).collect())
    })();
    let (pending, notes) = rates.status();
    let mut items = result.unwrap_or_else(|error| {
        vec![info(
            if pending {
                "Fetching exchange rates…"
            } else {
                "Keep typing a calculation"
            },
            &error.to_string(),
        )]
    });
    if !notes.is_empty() {
        if notes.contains("ExchangeRate-API") {
            items.push(json!({"title":"Rates by ExchangeRate-API","subtitle":"Daily fiat reference rates · Open provider website","arg":"https://www.exchangerate-api.com","valid":true,"variables":{"CALC_ACTION":"url"}}));
        }
        if notes.contains("Coinbase") {
            items.push(json!({"title":"Crypto rates by Coinbase","subtitle":"Indicative prices · Open provider documentation","arg":"https://docs.cdp.coinbase.com/coinbase-app/track-apis/exchange-rates","valid":true,"variables":{"CALC_ACTION":"url"}}));
        }
    }
    let mut result = json!({"items":items,"skipknowledge":true});
    if pending {
        result["rerun"] = 0.5.into();
    }
    result
}
fn main() {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "filter".into());
    if command == "refresh" {
        let provider = args.next().unwrap_or_default();
        let cache = args.next().unwrap_or_default();
        if cache.is_empty() {
            eprintln!("A cache directory is required");
            std::process::exit(2);
        }
        if let Err(error) = rates::refresh(&provider, std::path::Path::new(&cache)) {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    let mut args: Vec<_> = args.collect();
    if args.first().is_some_and(|a| a == "--") {
        args.remove(0);
    }
    let query = args.join(" ");
    let config = Config::from_env();
    let value = match command.as_str() {
        "filter" => filter(&query, false, &config, Utc::now()),
        "currency" => filter(&query, true, &config, Utc::now()),
        _ => {
            json!({"items":[info("Usage","calculator filter -- EXPRESSION | calculator currency -- AMOUNT CURRENCY")]})
        }
    };
    let _ = writeln!(io::stdout().lock(), "{value}");
}
