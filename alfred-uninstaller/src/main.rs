mod brew;
mod mac;
mod scan;
mod session;
#[cfg(test)]
mod tests;
mod ui;

use serde_json::{json, Value};
use std::{env, path::Path};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn action(config: &scan::Config, input: &str) -> Result<(String, String)> {
    if input.len() > 16_384 {
        return Err("Action is too large".into());
    }
    let request: Value = serde_json::from_str(input)?;
    let op = request["op"].as_str().ok_or("Missing action")?;
    if op == "scan" {
        let path = Path::new(request["path"].as_str().ok_or("Missing application path")?);
        let scan = scan::scan(config, path)?;
        return Ok((session::Store::open(config)?.create(scan)?, String::new()));
    }
    let token = request["token"].as_str().ok_or("Missing review token")?;
    let store = session::Store::open(config)?;
    let mut state = store.load(token)?;
    if matches!(op, "uninstall" | "trash") {
        session::preflight(
            config,
            &state,
            request["revision"]
                .as_u64()
                .ok_or("Missing selection revision")?,
        )?;
        session::execute(&store, token, &mut state, mac::trash)?;
    } else {
        state.filter = request["query"].as_str().unwrap_or("").to_owned();
        session::change(
            &mut state,
            op,
            request["index"].as_u64().and_then(|n| n.try_into().ok()),
        )?;
        store.save(token, &state)?;
    }
    let query = if state.panel == session::Panel::Files && state.phase == session::Phase::Review {
        state.filter
    } else {
        String::new()
    };
    Ok((token.to_owned(), query))
}

fn edit_action(config: &scan::Config, input: &str) -> Result<(String, String)> {
    let request: Value = serde_json::from_str(input)?;
    if !matches!(
        request["op"].as_str(),
        Some("toggle" | "all" | "none" | "sort" | "files" | "actions" | "details")
    ) {
        return Err("This shortcut can only change the selection or open Actions".into());
    }
    action(config, input)
}

fn run(args: &[String]) -> Result<Value> {
    let config = scan::Config::from_env()?;
    let query = args.get(2).map(String::as_str).unwrap_or("");
    match args.get(1).map(String::as_str).unwrap_or("list") {
        "list" => Ok(ui::list(&config, query)),
        "scan" => Ok(serde_json::to_value(scan::scan(
            &config,
            Path::new(query),
        )?)?),
        "start" => match action(&config, &json!({"op":"scan","path":query}).to_string()) {
            Ok((token, query)) => Ok(ui::action_output(&token, "", &query)),
            Err(e) => Ok(ui::action_output("", &e.to_string(), "")),
        },
        command @ ("action" | "edit") => match if command == "edit" {
            edit_action(&config, query)
        } else {
            action(&config, query)
        } {
            Ok((token, query)) => Ok(ui::action_output(&token, "", &query)),
            Err(e) => {
                eprintln!("{e}");
                let request: Value = serde_json::from_str(query).unwrap_or(Value::Null);
                let token = request["token"]
                    .as_str()
                    .filter(|t| session::token_ok(t))
                    .unwrap_or("");
                Ok(ui::action_output(
                    token,
                    &e.to_string(),
                    request["query"].as_str().unwrap_or(""),
                ))
            }
        },
        "view" => {
            let token = env::var("UNINSTALL_SESSION").unwrap_or_default();
            let error = env::var("UNINSTALL_ERROR").unwrap_or_default();
            if token.is_empty() {
                return Ok(
                    json!({"items":[ui::message("Start a new app review",if error.is_empty() {"Type uninstall followed by an app name."} else {&error})]}),
                );
            }
            let state = session::Store::open(&config)?.load(&token)?;
            ui::view(&config, &token, &state, query, &error)
        }
        "--version" => Ok(json!({"name":"Uninstaller Rust","version":env!("CARGO_PKG_VERSION")})),
        _ => Err(
            "Usage: uninstaller list [query] | scan <app.app> | view [filter] | action <JSON>"
                .into(),
        ),
    }
}
fn main() {
    let args: Vec<_> = env::args().collect();
    match run(&args) {
        Ok(value) => ui::output(&value),
        Err(e) => {
            eprintln!("{e}");
            ui::output(
                &json!({"items":[ui::message("Uninstaller could not continue",&e.to_string())]}),
            );
            std::process::exit(1);
        }
    }
}
