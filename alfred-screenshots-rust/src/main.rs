mod catalog;
mod thumbnails;

use serde_json::{json, Value};
use std::{env, path::PathBuf, time::Instant};

type Result<T> = std::result::Result<T, String>;
const BUNDLE: &str = "com.ariestwn.screenshots-rust";

#[derive(Clone)]
pub struct Config {
    folder: PathBuf,
    cache: PathBuf,
    page_size: usize,
    pixels: u32,
    page: usize,
}

impl Config {
    fn from_env() -> Result<Self> {
        let home = env::var_os("HOME").ok_or("HOME is not set.")?;
        let home = PathBuf::from(home);
        let folder = env::var("SHOTS_FOLDER").unwrap_or_else(|_| "~/Pictures/Screenshot".into());
        let folder = if folder == "~" {
            home.clone()
        } else if let Some(rest) = folder.strip_prefix("~/") {
            home.join(rest)
        } else {
            PathBuf::from(folder)
        };
        let folder = folder.canonicalize().map_err(|e| {
            format!("Cannot open screenshots folder: {e}. Choose a folder in Configure Workflow.")
        })?;
        if !folder.is_dir() {
            return Err("Screenshots Folder must be a directory.".into());
        }
        let cache = env::var_os("alfred_workflow_cache")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                home.join("Library/Caches/com.runningwithcrayons.Alfred/Workflow Data")
                    .join(BUNDLE)
            });
        Ok(Self {
            folder,
            cache,
            page_size: number("SHOTS_PAGE_SIZE", 48, 12, 120)?,
            pixels: number("SHOTS_PIXELS", 384, 160, 768)? as u32,
            page: number("SHOTS_PAGE", 0, 0, 1_000_000)?,
        })
    }
}

fn number(key: &str, default: usize, min: usize, max: usize) -> Result<usize> {
    match env::var(key).ok().filter(|s| !s.is_empty()) {
        None => Ok(default),
        Some(s) => s
            .parse()
            .ok()
            .filter(|v| (min..=max).contains(v))
            .ok_or_else(|| format!("{key} must be between {min} and {max}.")),
    }
}

fn navigation(page: usize, label: &str, query: &str, icon: &str) -> Value {
    json!({"uid":format!("page-{page}"), "title":label,
        "subtitle":"Press Return to browse this page", "arg":query, "valid":true,
        "icon":{"path":icon}, "variables":{"SHOTS_KIND":"page", "SHOTS_PAGE":page.to_string(), "SHOTS_QUERY":query}})
}

fn browse(config: &Config, query: &str) -> Result<Value> {
    let start = Instant::now();
    let page = catalog::scan(config, query)?;
    let scan_ms = start.elapsed().as_secs_f64() * 1000.;
    let start = Instant::now();
    let previews = thumbnails::prepare(config, &page.entries)?;
    let thumb_ms = start.elapsed().as_secs_f64() * 1000.;
    let mut items = Vec::with_capacity(page.entries.len() + 3);
    for (entry, preview) in page.entries.iter().zip(&previews) {
        let subtitle = format!(
            "{} · {} · ↩ Preview · ⌘↩ Copy image · ⌥↩ Reveal",
            entry.date,
            size(entry.bytes),
        );
        items.push(json!({"uid":entry.key(config.pixels), "type":"file:skipcheck",
            "title":entry.name, "subtitle":subtitle, "arg":entry.path,
            "icon":{"path":preview.path}, "quicklookurl":entry.path,
            "variables":{"SHOTS_KIND":"file", "SHOTS_PAGE":page.number.to_string(), "SHOTS_QUERY":query},
            "text":{"copy":entry.path, "largetype":entry.name}}));
    }
    let preselect = items
        .first()
        .and_then(|v| v["uid"].as_str())
        .unwrap_or("")
        .to_owned();
    if page.number > 0 {
        items.push(navigation(
            page.number - 1,
            "← Previous page",
            query,
            "previous.png",
        ));
    }
    if page.number + 1 < page.pages {
        items.push(navigation(
            page.number + 1,
            "Next page →",
            query,
            "next.png",
        ));
    }
    let title = if page.matched == 0 {
        "No matching screenshots".to_owned()
    } else {
        format!(
            "Page {} / {} · {} images",
            page.number + 1,
            page.pages,
            page.matched
        )
    };
    let keyword = env::var("SHOTS_KEYWORD").unwrap_or_else(|_| "shots".into());
    items.push(json!({"uid":"info", "title":title,
        "subtitle":format!("Search the whole folder: {keyword} <filename or date> · {} unreadable files skipped", page.skipped),
        "valid":false,"icon":{"path":"icon.png"}}));
    if env::var_os("SHOTS_STATS").is_some() {
        eprintln!(
            "{}",
            json!({"scanned_images":page.total,"matched":page.matched,
            "page_images":page.entries.len(),"page":page.number + 1,"pages":page.pages,
            "cached":previews.iter().filter(|p| p.cached).count(),
            "generated":previews.iter().filter(|p| !p.cached && !p.failed).count(),
            "failed_previews":previews.iter().filter(|p| p.failed).count(),
            "scan_ms":scan_ms,"thumbnail_ms":thumb_ms})
        );
    }
    Ok(
        json!({"items":items,"preselect":preselect,"variables":{"SHOTS_QUERY":query,"SHOTS_PAGE":page.number.to_string()}}),
    )
}

fn size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / 1048576.)
    } else {
        format!("{} KB", bytes.div_ceil(1024))
    }
}

fn main() {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "browse".into());
    if command == "--help" || command == "-h" {
        println!("Screenshots Rust\n\nshots browse [query]\nshots catalog [query]\n\nSettings: SHOTS_FOLDER, SHOTS_PAGE_SIZE (48), SHOTS_PIXELS (384), SHOTS_PAGE (0).\nSet SHOTS_STATS=1 for timing and counts on stderr. No images are uploaded.");
        return;
    }
    let query = args.next().unwrap_or_default();
    if args.next().is_some() {
        eprintln!("Pass the query as one argument.");
        std::process::exit(2);
    }
    let result = Config::from_env().and_then(|c| match command.as_str() {
        "browse" => browse(&c, &query),
        "catalog" => catalog::scan(&c, &query).map(|p| json!({"matched":p.matched,"total":p.total,
            "page":p.number,"pages":p.pages,"entries":p.entries.iter().map(|e| &e.path).collect::<Vec<_>>()})),
        _ => Err("Unknown command. Use --help.".into()),
    });
    match result {
        Ok(value) => println!("{value}"),
        Err(error) => {
            eprintln!("Screenshots Rust: {error}");
            // Alfred still needs valid grid JSON for a readable failure state.
            println!(
                "{}",
                json!({"items":[{"title":"Cannot load screenshots", "subtitle":error,
                "valid":false,"icon":{"path":"icon.png"}}]})
            );
            if command != "browse" {
                std::process::exit(1);
            }
        }
    }
}

#[cfg(test)]
mod tests;
