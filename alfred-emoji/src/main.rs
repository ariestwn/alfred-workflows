mod catalog;
mod jev;
#[cfg(test)]
mod tests;

use catalog::{Catalog, Emoji};
use jev::Jev;
use serde_json::{json, Value};
use std::{env, fs, path::PathBuf};

type Result<T> = std::result::Result<T, String>;
const BUNDLE: &str = "com.ariestwn.emoji";
const AI_LIMIT: usize = 24;
const PAGE_SIZE: usize = 30;

fn item(e: &Emoji) -> Value {
    let mut words = vec![e.name];
    words.extend(&e.keywords);
    json!({
        "uid": e.glyph,
        "title": e.title(),
        "subtitle": format!("{} · {}", e.group, e.subgroup),
        "arg": e.glyph,
        "match": words.join(" "),
        "icon": { "path": e.icon() },
        "variables": { "EMOJI_KIND": "emoji" },
        "text": { "copy": e.glyph, "largetype": e.glyph },
        "mods": {
            "cmd": { "arg": e.glyph, "subtitle": "Copy without pasting" },
            "alt": { "arg": e.name, "subtitle": format!("Copy name: {}", e.name) },
        },
    })
}

fn notice(title: &str, subtitle: &str) -> Value {
    json!({ "title": title, "subtitle": subtitle, "valid": false, "icon": { "path": "icon.png" } })
}

fn cache_path(query: &str, model: &str) -> Option<PathBuf> {
    let dir = env::var_os("alfred_workflow_cache").map(PathBuf::from).or_else(|| {
        env::var_os("HOME").map(|h| {
            PathBuf::from(h)
                .join("Library/Caches/com.runningwithcrayons.Alfred/Workflow Data")
                .join(BUNDLE)
        })
    })?;
    let key = format!("v2\0{model}\0{}", query.trim().to_lowercase());
    let hash = key.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100000001b3));
    Some(dir.join("jev-v1").join(format!("{hash:016x}.json")))
}

fn semantic(catalog: &Catalog, query: &str, jev: &Jev) -> Result<Vec<usize>> {
    let path = cache_path(query, &jev.model);
    if let Some(glyphs) = path
        .as_ref()
        .and_then(|p| fs::read(p).ok())
        .and_then(|b| serde_json::from_slice::<Vec<String>>(&b).ok())
    {
        return Ok(glyphs.iter().filter_map(|g| catalog.find(g)).collect());
    }
    let found: Vec<usize> = jev.search(catalog, query, AI_LIMIT)?.into_iter().map(|(i, _)| i).collect();
    if let Some(p) = path {
        let glyphs: Vec<&str> = found.iter().map(|&i| catalog.emoji[i].glyph).collect();
        let _ = fs::create_dir_all(p.parent().unwrap());
        let _ = fs::write(p, serde_json::to_vec(&glyphs).unwrap());
    }
    Ok(found)
}

/// Exact name hits first, then Jev's ranking, then remaining keyword hits.
pub fn merge(keyword: &[(usize, bool)], ai: &[usize]) -> Vec<usize> {
    let mut out: Vec<usize> = keyword.iter().filter(|k| k.1).map(|k| k.0).collect();
    for &i in ai.iter().chain(keyword.iter().map(|k| &k.0)) {
        if !out.contains(&i) {
            out.push(i);
        }
    }
    out
}

fn results(catalog: &Catalog, query: &str) -> Vec<Value> {
    let query = query.trim();
    if query.is_empty() {
        return catalog.emoji.iter().map(item).collect();
    }
    let keyword = catalog.keyword_search(query);
    let key = env::var("TYPESAFE_API_KEY").unwrap_or_default().trim().to_string();
    let mut problem = None;
    let ai = if key.is_empty() {
        Vec::new()
    } else {
        let model = env::var("JEV_MODEL").ok().filter(|m| !m.trim().is_empty()).unwrap_or("jev-latest".into());
        semantic(catalog, query, &Jev { key, model }).unwrap_or_else(|e| {
            problem = Some(e);
            Vec::new()
        })
    };
    let mut items: Vec<Value> = merge(&keyword, &ai).into_iter().map(|i| item(&catalog.emoji[i])).collect();
    if let Some(e) = problem {
        items.push(notice("AI search unavailable", &e));
    } else if items.is_empty() {
        let hint = if env::var("TYPESAFE_API_KEY").unwrap_or_default().trim().is_empty() {
            "Add a TypeSafe API key in Configure Workflow to search by meaning"
        } else {
            "Try describing it differently"
        };
        items.push(notice(&format!("No emoji for “{query}”"), hint));
    }
    items
}

fn nav(title: &str, icon: &str, query: &str, page: usize) -> Value {
    let vars = json!({ "EMOJI_PAGE": page.to_string(), "EMOJI_KIND": "page" });
    let off = json!({ "valid": false, "subtitle": title });
    json!({
        "uid": format!("page-{icon}"),
        "title": title,
        "subtitle": format!("Page {}", page + 1),
        "arg": query,
        "icon": { "path": format!("{icon}.png") },
        "variables": vars,
        "mods": { "cmd": off, "alt": off },
    })
}

/// Keeps the grid at four rows: PAGE_SIZE items plus up to two page tiles.
pub fn paginate(items: Vec<Value>, page: usize, query: &str) -> Vec<Value> {
    let pages = items.len().div_ceil(PAGE_SIZE).max(1);
    let page = page.min(pages - 1);
    let mut out: Vec<Value> = items.into_iter().skip(page * PAGE_SIZE).take(PAGE_SIZE).collect();
    if page > 0 {
        out.push(nav("Previous", "previous", query, page - 1));
    }
    if page + 1 < pages {
        out.push(nav("Next", "next", query, page + 1));
    }
    out
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let query = args.join(" ");
    let page = env::var("EMOJI_PAGE").ok().and_then(|p| p.parse().ok()).unwrap_or(0);
    let catalog = Catalog::load();
    let items = paginate(results(&catalog, &query), page, query.trim());
    println!("{}", json!({ "items": items }));
}
