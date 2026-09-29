mod clipboard;
mod config;
mod hud;
mod prompt;
mod runner;
mod selection;

use config::Config;
use prompt::Action;
use serde_json::{json, Value};
use std::{
    env,
    io::{self, Read, Write},
    time::Instant,
};

type Result<T> = std::result::Result<T, String>;

const HELP: &str = "QuickAI for Alfred\n\nUsage: alfred-quickai <quickfix|improve-writing|grammar-check> [OPTIONS] [-- TEXT]\n\n  --stdin      Read text from standard input\n  --workflow   Emit Alfred workflow JSON (internal)\n  --help       Show this help\n\nWithout text, read the macOS selection. The CLI writes rewritten text to stdout;\nAlfred handles copy/paste. Configure QUICKAI_PROVIDER, QUICKAI_CODEX_MODEL,\nQUICKAI_CODEX_EFFORT, QUICKAI_CLAUDE_MODEL, QUICKAI_CLAUDE_EFFORT,\nQUICKAI_OPENCODE_MODEL, QUICKAI_OUTPUT, QUICKAI_TIMEOUT, and optional\nQUICKAI_*_PATH variables.\n";

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        print!("{HELP}");
        return;
    }
    if args[0] == "--hud" {
        if let Some(path) = args.get(1) {
            if let Err(error) = hud::serve(std::path::Path::new(path)) {
                eprintln!("QuickAI HUD: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if args[0] == "--feedback" {
        let native = hud::finish();
        println!(
            "{}",
            json!({"alfredworkflow":{"variables":{
                "quickai_native_notification":if native { "1" } else { "0" }
            }}})
        );
        return;
    }
    if args[0] == "--capture-start" || args[0] == "--verify-paste" {
        let value = capture_helper(&args);
        println!("{value}");
        return;
    }
    // Only options before `--` count; selected prose can contain any flag-like text.
    let workflow = args
        .iter()
        .take_while(|a| a.as_str() != "--")
        .any(|a| a == "--workflow");
    let hud_path = if workflow { hud::start(&args[0]) } else { None };
    let result = run(&args, workflow);
    if let Some(path) = &hud_path {
        hud::handoff(path);
    }
    match result {
        Ok(rewrite) => {
            if workflow {
                let mut value = envelope(Some(&rewrite.text), &rewrite.output, &rewrite.message);
                let variables = &mut value["alfredworkflow"]["variables"];
                variables["quickai_verify"] =
                    json!(if rewrite.output == "copy" { "0" } else { "1" });
                variables["quickai_original"] = json!(rewrite.original);
                variables["quickai_target_pid"] = json!(rewrite.target.unwrap_or(0).to_string());
                variables["quickai_hud"] = json!(hud_path
                    .as_ref()
                    .map(|p| p.to_string_lossy())
                    .unwrap_or_default());
                println!("{value}");
            } else {
                let _ = io::stdout().write_all(rewrite.text.as_bytes());
            }
        }
        Err(message) => {
            if workflow {
                let mut value = envelope(None, "copy", &message);
                value["alfredworkflow"]["variables"]["quickai_hud"] = json!(hud_path
                    .as_ref()
                    .map(|p| p.to_string_lossy())
                    .unwrap_or_default());
                println!("{value}");
            } else {
                eprintln!("QuickAI: {message}");
                std::process::exit(1);
            }
        }
    }
}

struct Rewrite {
    text: String,
    output: String,
    message: String,
    original: String,
    target: Option<i32>,
}

fn capture_helper(args: &[String]) -> Value {
    let text = args
        .iter()
        .position(|a| a == "--")
        .map(|i| args[i + 1..].join(" "))
        .unwrap_or_default();
    if args[0] == "--capture-start" {
        return match clipboard::begin() {
            Ok(path) => json!({"alfredworkflow":{"arg":text,"variables":{
                "quickai_capture":path,"quickai_capture_ready":"1"
            }}}),
            Err(message) => {
                let mut value = envelope(None, "copy", &message);
                value["alfredworkflow"]["arg"] = json!(text);
                value["alfredworkflow"]["variables"]["quickai_capture_ready"] = json!("0");
                value
            }
        };
    }
    let original = env::var("quickai_original").unwrap_or_default();
    let target = env::var("quickai_target_pid")
        .unwrap_or_default()
        .parse::<i32>()
        .unwrap_or(0);
    let captured = env::var("quickai_capture")
        .map_err(|_| "No capture was prepared.".into())
        .and_then(|path| clipboard::finish(&path));
    let unchanged = clipboard::matches_original(&original, target, &captured);
    let output = if unchanged {
        env::var("quickai_output").unwrap_or_else(|_| "paste".into())
    } else {
        "copy".into()
    };
    let message = if unchanged {
        env::var("quickai_message").unwrap_or_else(|_| "Done".into())
    } else {
        "Result copied — the original selection changed or could not be confirmed. Paste with ⌘V."
            .into()
    };
    envelope(Some(&text), &output, &message)
}

fn run(args: &[String], workflow: bool) -> Result<Rewrite> {
    let action = Action::parse(&args[0])?;
    let mut stdin = false;
    let mut capture = false;
    let mut text = None;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--workflow" => {}
            "--stdin" => stdin = true,
            "--capture" => capture = true,
            "--" => {
                text = Some(args[index + 1..].join(" "));
                break;
            }
            other => return Err(format!("Unknown option: {other}. Put input text after --.")),
        }
        index += 1;
    }
    if stdin && text.is_some() {
        return Err("Use either --stdin or -- TEXT, not both.".into());
    }
    // Finish capture (and restore the clipboard) even when provider settings are invalid.
    let captured = if capture {
        let path = env::var("quickai_capture")
            .map_err(|_| "Missing selection capture. Run the keyword again.")?;
        Some(clipboard::finish(&path)?)
    } else {
        None
    };
    if let Some((input, _)) = &captured {
        text = Some(input.clone());
    }
    let config = Config::from_env()?;
    if stdin {
        let mut bytes = Vec::new();
        io::stdin()
            .take((prompt::MAX_INPUT_CHARS * 4 + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("Cannot read input: {e}"))?;
        text =
            Some(String::from_utf8(bytes).map_err(|_| "Input is too long or is not valid UTF-8.")?);
    }
    // Alfred passes an empty argument for a keyword invoked without text.
    let needs_selection = !stdin && text.as_ref().is_none_or(|s| s.is_empty());
    let before = if needs_selection {
        selection::after_alfred_closes()
    } else {
        None
    };
    let target = captured
        .as_ref()
        .map(|(_, pid)| *pid)
        .or_else(|| before.as_ref().map(|s| s.pid))
        .or_else(|| {
            if workflow {
                clipboard::target_after_alfred_closes()
            } else {
                None
            }
        });
    let input = if needs_selection {
        before.as_ref().and_then(|s| s.text.clone()).ok_or(
            "Cannot read selected text. Use an Alfred Universal Action/hotkey, type text after the keyword, or enable Accessibility for Alfred.")?
    } else {
        text.unwrap_or_default()
    };
    prompt::validate_input(&input)?;
    let custom = env::var(action.prompt_variable()).unwrap_or_default();
    let system = prompt::system_prompt(action, &custom, &config.global_prompt);
    let started = Instant::now();
    let result = runner::transform(&config, &system, &input)?;
    let output = config.output.clone();
    let model = if config.model.is_empty() {
        "default model"
    } else {
        &config.model
    };
    let message = format!(
        "{} · {} / {} · {:.1}s",
        action.title(),
        config.provider.name(),
        model,
        started.elapsed().as_secs_f32()
    );
    Ok(Rewrite {
        text: result,
        output,
        message,
        original: input,
        target,
    })
}

fn envelope(text: Option<&str>, output: &str, message: &str) -> Value {
    json!({"alfredworkflow": {"arg": text.unwrap_or(""), "variables": {
        "quickai_ok": if text.is_some() { "1" } else { "0" },
        "quickai_output": output,
        "quickai_message": message,
    }}})
}

#[cfg(test)]
mod tests;
