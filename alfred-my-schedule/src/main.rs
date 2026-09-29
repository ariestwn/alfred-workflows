mod actions;
mod create;
mod free;
mod links;
mod model;
mod store;
#[cfg(test)]
mod tests;
mod view;

use chrono::Utc;
use model::*;
use serde_json::{json, Value};
use std::{env, fs};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn permission() -> Value {
    view::output(vec![
        view::run(
            "Allow calendar access",
            "Press Return, then allow Full Access so My Schedule can read your calendars.",
            json!({"op":"authorize"}),
        ),
        view::run(
            "Open Calendar privacy settings",
            "Enable Calendar access for Alfred / My Schedule if previously denied.",
            json!({"op":"privacy"}),
        ),
        view::run(
            "Open Calendar",
            "Check that your calendar accounts are added and syncing.",
            json!({"op":"calendar-app"}),
        ),
    ])
}
fn execute() -> Result<()> {
    let mut args: Vec<String> = env::args().skip(1).collect();
    let command = if args.is_empty() {
        "agenda".to_string()
    } else {
        args.remove(0)
    };
    let fixture = if let Some(index) = args.iter().position(|v| v == "--fixture") {
        if index + 1 >= args.len() {
            return Err("Missing fixture path".into());
        }
        let path = args.remove(index + 1);
        args.remove(index);
        Some(path)
    } else {
        None
    };
    args.retain(|v| v != "--");
    let query = args.join(" ");
    if query.len() > 16384 {
        return Err("Input is too long.".into());
    }
    if command == "status" {
        println!("{}", store::native(json!({"op":"status"}))?);
        return Ok(());
    }
    let config = Config::load()?;
    if command == "act" {
        if fixture.is_some() {
            return Err("Actions cannot run against a fixture.".into());
        }
        actions::perform(&serde_json::from_str(&query)?, &config)?;
        return Ok(());
    }
    let now = Utc::now();
    let snapshot = if let Some(path) = fixture {
        serde_json::from_slice::<Snapshot>(&fs::read(path)?)?
    } else {
        if store::native(json!({"op":"status"}))?["status"] != 3 {
            println!("{}", permission());
            return Ok(());
        }
        store::snapshot(&config, now, false)?
    };
    let prefs = store::preferences(&config)?;
    let output = match command.as_str() {
        "agenda" => view::agenda(&snapshot, &prefs, &config, &query, now, false)?,
        "next" => view::agenda(&snapshot, &prefs, &config, &query, now, true)?,
        "calendars" => view::calendars(&snapshot, &prefs, &query),
        "menu" => {
            let reference: Value =
                serde_json::from_str(&env::var("SCHEDULE_EVENT").unwrap_or_default())
                    .map_err(|_| "Open event actions with Option-Return from your schedule.")?;
            view::menu(snapshot.event(&reference)?, &snapshot, &config, &query)
        }
        "new" => create::preview(&query, &snapshot, &config, now)?,
        "free" => free::preview(&query, &snapshot, &prefs, &config, now)?,
        _ => return Err("Unknown command.".into()),
    };
    println!("{output}");
    Ok(())
}
fn main() {
    if let Err(error) = execute() {
        let message = error.to_string();
        if env::args().nth(1).as_deref() == Some("act") {
            eprintln!("My Schedule: {message}");
            let _ = store::native(json!({"op":"alert","message":message}));
            std::process::exit(1);
        }
        println!(
            "{}",
            view::output(vec![view::info("My Schedule", &message)])
        );
    }
}
