use crate::{text, Result};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

fn catalog() -> Vec<Value> {
    serde_json::from_str(include_str!("../data/catalog.json")).expect("embedded catalog")
}
fn legacy() -> Vec<Value> {
    serde_json::from_str(include_str!("../data/legacy-models.json")).expect("embedded registry")
}
fn entries(dir: &Path) -> Result<Vec<PathBuf>> {
    match fs::read_dir(dir) {
        Ok(entries) => entries
            .map(|e| e.map(|e| e.path()).map_err(Into::into))
            .collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(e.into()),
    }
}
pub fn capabilities(id: &str) -> Option<Value> {
    let repo = id.rsplit_once('/').map(|(repo, _)| repo).unwrap_or(id);
    if let Some(entry) = catalog().into_iter().find(|e| text(e, "id") == repo) {
        let languages = entry["languages"].as_array().cloned().unwrap_or_default();
        return Some(
            json!({"name":entry["name"],"select":languages.len()>1,"languages":languages}),
        );
    }
    legacy().into_iter().find(|e| text(e, "id") == id).map(|e| json!({
        "name": e["name"], "select": e["supportsLanguageSelection"], "languages": e["supportedLanguages"]
    }))
}
pub fn languages(model: &str) -> Vec<Value> {
    let caps = capabilities(model);
    if caps.as_ref().is_some_and(|c| c["select"] == false) {
        return vec![];
    }
    let all: Vec<Value> =
        serde_json::from_str(include_str!("../data/languages.json")).expect("embedded languages");
    let allowed = caps.as_ref().and_then(|c| c["languages"].as_array());
    all.into_iter()
        .filter(|lang| {
            text(lang, "code") == "auto" || allowed.is_none_or(|a| a.contains(&lang["code"]))
        })
        .collect()
}
pub fn downloaded(hub: &Path, model_dir: &Path) -> Result<Vec<Value>> {
    let catalog = catalog();
    let legacy = legacy();
    let mut models = Vec::new();
    let mut seen = HashSet::new();
    for repo_dir in entries(hub)? {
        let folder = repo_dir.file_name().unwrap_or_default().to_string_lossy();
        let Some(repo) = folder
            .strip_prefix("models--")
            .map(|s| s.replace("--", "/"))
        else {
            continue;
        };
        let meta = catalog.iter().find(|e| text(e, "id") == repo);
        let mut snapshots = entries(&repo_dir.join("snapshots"))?;
        let primary = fs::read_to_string(repo_dir.join("refs/main")).unwrap_or_default();
        snapshots.sort_by_key(|p| {
            (
                p.file_name().unwrap_or_default() != primary.trim(),
                std::cmp::Reverse(
                    p.metadata()
                        .and_then(|m| m.modified())
                        .unwrap_or(SystemTime::UNIX_EPOCH),
                ),
            )
        });
        for snapshot in snapshots {
            let files: Vec<_> = entries(&snapshot)?
                .into_iter()
                .filter(|p| {
                    p.extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("gguf"))
                        && p.is_file()
                })
                .collect();
            if files.is_empty() {
                continue;
            }
            for path in files {
                let filename = path.file_name().unwrap().to_string_lossy();
                let id = format!("{repo}/{filename}");
                if !seen.insert(id.clone()) {
                    continue;
                }
                let fallback = repo
                    .rsplit('/')
                    .next()
                    .unwrap_or(&repo)
                    .trim_end_matches("-gguf")
                    .replace('-', " ");
                let name = meta.map(|e| text(e, "name")).unwrap_or(&fallback);
                let suffix = filename
                    .trim_end_matches(".gguf")
                    .rsplit('-')
                    .next()
                    .unwrap_or("");
                let name = if suffix.starts_with('Q') || ["F16", "F32", "BF16"].contains(&suffix) {
                    format!("{name} ({suffix})")
                } else {
                    name.to_string()
                };
                models.push(json!({"id":id,"name":name,"path":path,"description":meta.map(|e|text(e,"description")).unwrap_or("Downloaded GGUF model")}));
            }
            break;
        }
    }
    for path in entries(model_dir)? {
        let filename = path.file_name().unwrap_or_default().to_string_lossy();
        let meta = legacy.iter().find(|e| text(e, "filename") == filename);
        if meta.is_none()
            && !path.is_dir()
            && !path.extension().is_some_and(|e| e == "bin" || e == "gguf")
        {
            continue;
        }
        if !path.exists() {
            continue;
        }
        let fallback = path.file_stem().unwrap_or_default().to_string_lossy();
        let id = meta.map(|e| text(e, "id")).unwrap_or(&fallback);
        if !seen.insert(id.to_string()) {
            continue;
        }
        models.push(
            json!({"id":id,"name":meta.map(|e|text(e,"name")).unwrap_or(&fallback),"path":path,
            "description":meta.map(|e|text(e,"description")).unwrap_or("Custom model")}),
        );
    }
    models.sort_by_cached_key(|m| text(m, "name").to_lowercase());
    Ok(models)
}
