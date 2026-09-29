mod config;
mod hud;
mod upload;

use config::Config;
use serde_json::{json, Value};
use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
};

type Result<T> = std::result::Result<T, String>;

const HELP: &str = "Upload to Remote for Alfred\n\nUsage: kraely-upload [--workflow] -- FILE\n\nUploads one regular file over SSH and prints its absolute remote path.\nAlfred copies the path after success. No clipboard changes on failure.\n\nSettings: KRAELY_HOST (required), KRAELY_REMOTE_DIR, KRAELY_MAX_SIZE_MB,\nKRAELY_TIMEOUT, KRAELY_FEEDBACK (hud, notification, off).\n";

fn main() {
    let args: Vec<OsString> = env::args_os().skip(1).collect();
    if args.first().is_some_and(|a| a == "--help" || a == "-h") {
        print!("{HELP}");
        return;
    }
    if args.first().is_some_and(|a| a == "--hud") {
        let result = args
            .get(1)
            .ok_or_else(|| "Missing HUD directory.".to_owned())
            .and_then(|s| hud::serve(Path::new(s)));
        if let Err(error) = result {
            eprintln!("Upload to Remote HUD: {error}");
            std::process::exit(1);
        }
        return;
    }
    if args.first().is_some_and(|a| a == "--feedback") {
        let native = hud::finish();
        println!(
            "{}",
            json!({"alfredworkflow":{"variables":{
                "kraely_native_notification": if native { "1" } else { "0" }
            }}})
        );
        return;
    }
    let workflow = args
        .iter()
        .take_while(|a| *a != "--")
        .any(|a| a == "--workflow");
    let mut hud_path = None;
    let result = run(&args, workflow, &mut hud_path);
    if let Some(path) = &hud_path {
        hud::handoff(path);
    }
    if workflow {
        println!("{}", envelope(&result, hud_path.as_deref()));
    } else {
        match result {
            Ok(path) => println!("{path}"),
            Err(message) => {
                eprintln!("Upload to Remote: {message}");
                std::process::exit(1);
            }
        }
    }
}

fn run(args: &[OsString], workflow: bool, hud_path: &mut Option<PathBuf>) -> Result<String> {
    let separator = args
        .iter()
        .position(|s| s == "--")
        .ok_or("Select one file through Alfred Universal Actions, or pass -- FILE.")?;
    if args[..separator].iter().any(|s| s != "--workflow") {
        return Err("Unknown option. Use -- before the selected file path.".into());
    }
    let selected = &args[separator + 1..];
    if selected.len() != 1 || selected[0].is_empty() {
        return Err("Select exactly one file to upload.".into());
    }
    let config = Config::from_env()?;
    let upload = upload::Upload::prepare(&config, Path::new(&selected[0]))?;
    if workflow {
        *hud_path = hud::start(config.timeout);
    }
    upload::execute(&config, &upload)?;
    Ok(upload.remote)
}

fn envelope(result: &Result<String>, hud_path: Option<&Path>) -> Value {
    let (arg, ok, message) = match result {
        Ok(path) => (path.as_str(), "1", format!("Remote path copied: {path}")),
        Err(error) => ("", "0", error.clone()),
    };
    json!({"alfredworkflow":{"arg":arg,"variables":{
        "kraely_ok":ok, "kraely_message":message,
        "kraely_hud":hud_path.map(|p|p.to_string_lossy()).unwrap_or_default()
    }}})
}

#[cfg(test)]
mod tests;
