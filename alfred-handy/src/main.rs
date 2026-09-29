mod app;
mod db;
mod models;
#[cfg(test)]
mod tests;

use app::Paths;
use db::Db;
use serde_json::{json, Value};
use std::{
    env, fs,
    io::{self, Write},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or("")
}
fn item(title: impl Into<String>, subtitle: impl Into<String>, action: Value) -> Value {
    json!({"title":title.into(),"subtitle":subtitle.into(),"arg":action.to_string(),"valid":true})
}
fn info(title: impl Into<String>, subtitle: impl Into<String>) -> Value {
    json!({"title":title.into(),"subtitle":subtitle.into(),"valid":false})
}
fn browse(query: &str) -> Value {
    json!({"op":"browse","query":query})
}
fn matches(query: &str, haystack: &str) -> bool {
    let haystack = haystack.to_lowercase();
    query
        .split_whitespace()
        .all(|q| haystack.contains(&q.to_lowercase()))
}
fn compact(s: &str, n: usize) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(n)
        .collect()
}
fn output(route: &str, arg: &str, message: &str) -> Value {
    json!({"alfredworkflow":{"arg":arg,"variables":{"HANDY_ROUTE":route,"HANDY_MESSAGE":message}}})
}
fn transcript(entry: &Value, raw: bool) -> &str {
    if raw {
        text(entry, "transcription_text")
    } else {
        entry["post_processed_text"]
            .as_str()
            .unwrap_or_else(|| text(entry, "transcription_text"))
    }
}
fn id(action: &Value) -> Result<i64> {
    action["id"]
        .as_i64()
        .filter(|id| *id > 0)
        .ok_or_else(|| "Invalid transcript ID".into())
}
fn date(timestamp: i64) -> String {
    let time = timestamp as libc::time_t;
    let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
    if unsafe { libc::localtime_r(&time, &mut tm) }.is_null() {
        return timestamp.to_string();
    }
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min
    )
}

fn filter(paths: &Paths, full_query: &str) -> Result<Value> {
    let (mode, query) = full_query.split_once(' ').unwrap_or((full_query, ""));
    let query = query.trim();
    let mut items = Vec::new();
    match mode {
        "history" | "saved" => {
            if !paths.db().is_file() {
                return Ok(
                    json!({"items":[info("No transcriptions yet", "Open Handy and record your first transcript")]}),
                );
            }
            let db = Db::open(&paths.db(), false)?;
            let page = if env::var("HANDY_PAGE_QUERY").ok().as_deref() == Some(full_query.trim()) {
                env::var("HANDY_PAGE")
                    .ok()
                    .and_then(|p| p.parse::<usize>().ok())
                    .unwrap_or(0)
                    .min(1_000_000)
            } else {
                0
            };
            let size = env::var("HANDY_PAGE_SIZE")
                .ok()
                .and_then(|n| n.parse::<usize>().ok())
                .unwrap_or(40)
                .clamp(10, 100);
            let rows = db.history(query, mode == "saved", page, size)?;
            if page > 0 {
                items.push(item(
                    "← Previous page",
                    format!("Page {page}"),
                    json!({"op":"browse","query":full_query,"page":page-1}),
                ));
            }
            for row in rows.iter().take(size) {
                let id = row["id"].as_i64().ok_or("Invalid history ID")?;
                let title = compact(text(row, "title"), 100);
                let title = if title.is_empty() {
                    "Untitled transcript".to_string()
                } else {
                    title
                };
                let mut entry = item(
                    format!(
                        "{}{}",
                        if row["saved"].as_i64().unwrap_or(0) != 0 {
                            "★ "
                        } else {
                            ""
                        },
                        title
                    ),
                    format!(
                        "{} · {}",
                        date(row["timestamp"].as_i64().unwrap_or(0)),
                        compact(text(row, "display_text"), 150)
                    ),
                    json!({"op":"copy","id":id}),
                );
                entry["uid"] = format!("transcript-{id}").into();
                entry["text"] = json!({"copy":row["display_text"],"largetype":row["display_text"]});
                entry["action"] = json!({"text":[row["display_text"]]});
                entry["mods"] = json!({
                    "cmd":{"subtitle":"Paste transcript into the active app","arg":json!({"op":"paste","id":id}).to_string()},
                    "alt":{"subtitle":"Reveal recording in Finder","arg":json!({"op":"reveal","id":id}).to_string()},
                    "ctrl":{"subtitle":"View transcript, save, or delete…","arg":browse(&format!("entry {id}")).to_string()}
                });
                items.push(entry);
            }
            if rows.len() > size {
                items.push(item(
                    "Next page →",
                    format!("Page {} · More matching transcripts", page + 2),
                    json!({"op":"browse","query":full_query,"page":page+1}),
                ));
            }
            if items.is_empty() {
                items.push(info(
                    "No matching transcripts",
                    if query.is_empty() {
                        "Record with Handy first, or browse all history"
                    } else {
                        "Try another word, phrase, or title"
                    },
                ));
            }
        }
        "entry" | "delete" => {
            let id: i64 = query
                .parse()
                .map_err(|_| "Choose a transcript from history")?;
            let entry = Db::open(&paths.db(), false)?.entry(Some(id))?;
            items.push(info(
                compact(text(&entry, "title"), 100),
                date(entry["timestamp"].as_i64().unwrap_or(0)),
            ));
            if mode == "delete" {
                items.push(item(
                    "Keep transcript",
                    "Return to transcript actions",
                    browse(&format!("entry {id}")),
                ));
                items.push(item(
                    "Delete transcript and recording",
                    "A transcript backup and audio are placed in Trash first",
                    json!({"op":"delete","id":id}),
                ));
            } else {
                for (title, op, subtitle) in [
                    (
                        "View full transcript",
                        "view",
                        "Show original and post-processed text",
                    ),
                    (
                        "Copy transcript",
                        "copy",
                        "Use post-processed text when available",
                    ),
                    ("Paste transcript", "paste", "Paste into the active app"),
                    (
                        "Copy original transcript",
                        "raw",
                        "Copy text before post-processing",
                    ),
                    (
                        if entry["saved"].as_i64().unwrap_or(0) != 0 {
                            "Remove from Saved"
                        } else {
                            "Save transcript"
                        },
                        "save",
                        "Toggle the saved flag in Handy",
                    ),
                    (
                        "Reveal recording",
                        "reveal",
                        "Show the audio file in Finder",
                    ),
                ] {
                    items.push(item(title, subtitle, json!({"op":op,"id":id})));
                }
                items.push(item(
                    "Delete transcript…",
                    "Review before removing this transcript",
                    browse(&format!("delete {id}")),
                ));
            }
        }
        "models" => {
            let store = paths.read_store()?;
            let current = text(&store["settings"], "selected_model");
            for model in models::downloaded(&paths.hub, &paths.data.join("models"))? {
                if !matches(
                    query,
                    &format!(
                        "{} {} {}",
                        text(&model, "name"),
                        text(&model, "id"),
                        text(&model, "description")
                    ),
                ) {
                    continue;
                }
                let active = text(&model, "id") == current;
                items.push(item(
                    format!("{}{}", if active { "✓ " } else { "" }, text(&model, "name")),
                    if active {
                        "Active model".to_string()
                    } else {
                        format!("Apply and reopen Handy · {}", text(&model, "description"))
                    },
                    json!({"op":"model","value":model["id"]}),
                ));
            }
            if items.is_empty() {
                items.push(info(
                    "No downloaded models match",
                    "Download a model in Handy, then reopen this list",
                ));
            }
        }
        "languages" => {
            let store = paths.read_store()?;
            let settings = &store["settings"];
            let languages = models::languages(text(settings, "selected_model"));
            if languages.is_empty() {
                items.push(info(
                    "This model chooses its own language",
                    "Select another model to choose a transcription language",
                ));
            }
            for lang in languages {
                let label = if text(&lang, "code") == "auto" {
                    "Auto (detect)".into()
                } else {
                    format!("{} · {}", text(&lang, "native"), text(&lang, "label"))
                };
                if !matches(query, &format!("{label} {}", text(&lang, "code"))) {
                    continue;
                }
                let active = lang["code"] == settings["selected_language"];
                items.push(item(format!("{}{}",if active {"✓ "}else{""},label),if active {"Active language"} else {"Apply and reopen Handy"},
                    json!({"op":"language","value":lang["code"],"model":settings["selected_model"]})));
            }
            if items.is_empty() {
                items.push(info(
                    "No matching languages",
                    "Search by name or language code",
                ));
            }
        }
        "dictionary" | "add" => {
            let store = paths.read_store()?;
            let words = store["settings"]["custom_words"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let exists = words.iter().any(|w| {
                w.as_str()
                    .is_some_and(|w| w.to_lowercase() == query.to_lowercase())
            });
            if !query.is_empty() && !exists {
                items.push(item(
                    format!("Add “{query}”"),
                    "Add to dictionary and reopen Handy",
                    json!({"op":"word-add","value":query}),
                ));
            }
            if mode == "dictionary" || exists {
                for word in words
                    .iter()
                    .filter_map(Value::as_str)
                    .filter(|w| matches(query, w))
                {
                    let mut row = item(
                        word,
                        "Enter: copy · ⌘↵: remove word and reopen Handy",
                        json!({"op":"word-copy","value":word}),
                    );
                    row["mods"] = json!({"cmd":{"subtitle":"Remove word and reopen Handy","arg":json!({"op":"word-remove","value":word}).to_string()}});
                    items.push(row);
                }
            }
            if items.is_empty() {
                items.push(info(
                    "Type a word or phrase to add",
                    "Custom words help Handy recognize names and specialized vocabulary",
                ));
            }
        }
        _ => {
            let definitions = [
                (
                    "Toggle Recording",
                    "Start or stop Handy transcription",
                    json!({"op":"toggle"}),
                ),
                (
                    "Copy Last Transcript",
                    "Copy your most recent transcription",
                    json!({"op":"copy"}),
                ),
                (
                    "Paste Last Transcript",
                    "Paste your most recent transcription into the active app",
                    json!({"op":"paste"}),
                ),
                (
                    "Search Transcripts",
                    "Browse history · Enter: copy · ⌘↵: paste · ⌃↵: actions",
                    browse("history "),
                ),
                (
                    "Saved Transcripts",
                    "Browse transcripts marked as saved",
                    browse("saved "),
                ),
                (
                    "Add Dictionary Word",
                    "Add a word or phrase",
                    browse("add "),
                ),
                (
                    "Manage Dictionary",
                    "Search, add, and remove custom words",
                    browse("dictionary "),
                ),
                (
                    "Select Model",
                    "Choose an already downloaded transcription model",
                    browse("models "),
                ),
                (
                    "Select Language",
                    "Choose a language supported by the active model",
                    browse("languages "),
                ),
                (
                    "Open Recordings Folder",
                    "Open Handy’s audio recordings in Finder",
                    json!({"op":"recordings"}),
                ),
                (
                    "Cancel Recording",
                    "Cancel the current Handy operation",
                    json!({"op":"cancel"}),
                ),
            ];
            for (title, subtitle, action) in definitions {
                if matches(full_query, &format!("{title} {subtitle}")) {
                    items.push(item(title, subtitle, action));
                }
            }
            if items.is_empty() {
                items.push(item(
                    "Search transcripts",
                    format!("Search history for “{full_query}”"),
                    browse(&format!("history {full_query}")),
                ));
            }
        }
    }
    Ok(json!({"items":items,"skipknowledge":true}))
}

fn execute(paths: &Paths, action: &Value) -> Result<Value> {
    let op = text(action, "op");
    match op {
        "browse" => {
            let query = text(action, "query");
            let mut result = output("browse", query, "");
            result["alfredworkflow"]["variables"]["HANDY_PAGE"] =
                action["page"].as_u64().unwrap_or(0).to_string().into();
            result["alfredworkflow"]["variables"]["HANDY_PAGE_QUERY"] = query.trim().into();
            Ok(result)
        }
        "toggle" | "cancel" => Ok(output(
            "notify",
            "",
            &paths.toggle(if op == "cancel" {
                "--cancel"
            } else {
                "--toggle-transcription"
            })?,
        )),
        "model" | "language" | "word-add" | "word-remove" => {
            Ok(output("notify", "", &paths.apply(action)?))
        }
        "word-copy" => Ok(output("copy", text(action, "value"), "Word copied")),
        "recordings" => {
            let path = paths.data.join("recordings");
            if !path.is_dir() {
                return Err(
                    "The recordings folder does not exist yet. Make a recording in Handy first."
                        .into(),
                );
            }
            Ok(output("open", &path.to_string_lossy(), ""))
        }
        "copy" | "paste" | "raw" | "view" | "reveal" => {
            let selected_id = if action.get("id").is_some() {
                Some(id(action)?)
            } else {
                None
            };
            let entry = Db::open(&paths.db(), false)?.entry(selected_id)?;
            match op {
                "view" => {
                    let mut content = format!(
                        "{}\n{}\n\n",
                        text(&entry, "title"),
                        date(entry["timestamp"].as_i64().unwrap_or(0))
                    );
                    if let Some(processed) = entry["post_processed_text"].as_str() {
                        content.push_str(&format!("POST-PROCESSED\n{processed}\n\n"));
                    }
                    content.push_str(&format!("ORIGINAL\n{}", text(&entry, "transcription_text")));
                    Ok(output("view", &content, ""))
                }
                "reveal" => {
                    let path = paths.recording(&entry)?;
                    if !path.is_file() {
                        return Err(
                            "Recording is no longer on disk. The transcript is still available."
                                .into(),
                        );
                    }
                    Ok(output("reveal", &path.to_string_lossy(), ""))
                }
                _ => Ok(output(
                    if op == "paste" { "paste" } else { "copy" },
                    transcript(&entry, op == "raw"),
                    "Transcript copied",
                )),
            }
        }
        "save" => {
            let db = Db::open(&paths.db(), true)?;
            let id = id(action)?;
            db.entry(Some(id))?;
            db.query("UPDATE transcription_history SET saved = CASE WHEN saved = 0 THEN 1 ELSE 0 END WHERE id = ?", &[id.to_string()])?;
            let saved = db.entry(Some(id))?["saved"].as_i64().unwrap_or(0) != 0;
            Ok(output(
                "notify",
                "",
                if saved {
                    "Transcript saved"
                } else {
                    "Transcript removed from Saved"
                },
            ))
        }
        "delete" => delete(paths, id(action)?),
        _ => Err("Unknown Handy action".into()),
    }
}
fn delete(paths: &Paths, id: i64) -> Result<Value> {
    let db = Db::open(&paths.db(), true)?;
    db.query("BEGIN IMMEDIATE", &[])?;
    let entry = db.entry(Some(id))?;
    let audio = paths.recording(&entry)?;
    fs::create_dir_all(&paths.trash)?;
    let backup = tempfile::Builder::new()
        .prefix(&format!("Handy Transcript {id} "))
        .tempdir_in(&paths.trash)?;
    app::atomic_store(&backup.path().join("transcript.json"), &entry)?;
    if audio.exists() {
        if !audio.is_file() {
            return Err("Recording path is not a regular file; nothing was deleted".into());
        }
        let copy = backup
            .path()
            .join(format!("audio-{}", text(&entry, "file_name")));
        fs::copy(&audio, &copy)?;
        fs::File::open(copy)?.sync_all()?;
    }
    fs::File::open(backup.path())?.sync_all()?;
    // Keep the recovery files before committing any destructive changes.
    let _backup_path = backup.keep();
    db.query(
        "DELETE FROM transcription_history WHERE id = ?",
        &[id.to_string()],
    )?;
    db.query("COMMIT", &[])?;
    let message = if audio.exists() && fs::remove_file(&audio).is_err() {
        "Transcript deleted and backed up in Trash; the original audio could not be removed"
    } else {
        "Transcript deleted — transcript backup and available audio are in Trash"
    };
    Ok(output("notify", "", message))
}

fn main() {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "filter".into());
    let mut remaining: Vec<_> = args.collect();
    if remaining.first().is_some_and(|arg| arg == "--") {
        remaining.remove(0);
    } else if remaining.get(1).is_some_and(|arg| arg == "--") {
        remaining.remove(1);
    }
    let query = remaining.join(" ");
    let result = Paths::from_env().and_then(|paths| match command.as_str() {
        "filter" => filter(&paths,&query),
        "action" => execute(&paths,&serde_json::from_str::<Value>(&query)?),
        "help" | "--help" => Ok(json!({"usage":"handy filter [history|saved|models|languages|dictionary|add] [query] OR handy action <JSON>","keyword":"handy"})),
        _ => Err("Unknown command".into())
    });
    let value = result.unwrap_or_else(|e| {
        if command == "filter" {
            json!({"items":[info("Handy needs attention",e.to_string())]})
        } else {
            output("error", "", &e.to_string())
        }
    });
    // Broken stdout pipes are normal when Alfred cancels a previous search.
    let _ = writeln!(io::stdout().lock(), "{value}");
}
