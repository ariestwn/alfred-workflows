use crate::{
    brew,
    scan::{self, Config},
    session::{Panel, Phase, Session},
    Result,
};
use serde_json::{json, Value};
use std::{cmp::Reverse, path::Path};

pub fn output(value: &Value) {
    println!("{}", serde_json::to_string(value).unwrap());
}
pub fn message(title: &str, subtitle: &str) -> Value {
    json!({"title":title,"subtitle":subtitle,"valid":false})
}
pub fn action_output(token: &str, error: &str, query: &str) -> Value {
    json!({"alfredworkflow":{"arg":query,"variables":{"UNINSTALL_SESSION":token,"UNINSTALL_ERROR":error,"UNINSTALL_ROUTE":""}}})
}
fn request(op: &str, token: &str, query: &str) -> String {
    json!({"op":op,"token":token,"query":query}).to_string()
}
fn menu_row(title: &str, subtitle: &str, op: &str, token: &str, query: &str) -> Value {
    json!({"uid":format!("{token}-{op}"),"title":title,"subtitle":subtitle,
        "arg":request(op,token,query),"valid":true})
}
fn reveal(path: &Path) -> Value {
    json!({"valid":true,"arg":path,"subtitle":"Reveal in Finder","variables":{"UNINSTALL_ROUTE":"reveal"}})
}

pub fn list(config: &Config, query: &str) -> Value {
    let (apps, warnings) = scan::discover(config);
    let words: Vec<_> = query.split_whitespace().map(str::to_lowercase).collect();
    let mut items = Vec::new();
    for app in apps.into_iter().filter(|a| scan::allowed_app(config, a)) {
        let haystack = format!("{} {} {}", app.name, app.id, app.path.display()).to_lowercase();
        if !words.iter().all(|w| haystack.contains(w)) {
            continue;
        }
        items.push(json!({"uid":app.path,"title":app.name,
            "subtitle":scan::short_path(&app.path,&config.home),
            "arg":json!({"op":"scan","path":app.path}).to_string(),"valid":true,
            "icon":{"type":"fileicon","path":app.path},"quicklookurl":app.path,
            "text":{"copy":app.path},"mods":{"alt":reveal(&app.path)}}));
    }
    if items.is_empty() {
        items.push(message(
            "No matching applications",
            "Search /Applications and ~/Applications, or add a folder in Configure Workflow.",
        ));
    }
    for warning in warnings {
        items.push(message("Some apps could not be listed", &warning));
    }
    json!({"items":items,"variables":{"UNINSTALL_ROUTE":""}})
}

pub fn view(config: &Config, token: &str, s: &Session, query: &str, error: &str) -> Result<Value> {
    let items = if matches!(s.phase, Phase::Running | Phase::Finished) {
        report(config, s, error)
    } else {
        match s.panel {
            Panel::Files => files(config, token, s, query, error),
            Panel::Actions => actions(token, s),
            Panel::Details => details(config, token, s),
        }
    };
    // Stable row IDs identify files across refreshes; skipknowledge keeps our sort order.
    Ok(json!({"items":items,"skipknowledge":true,
        "variables":{"UNINSTALL_SESSION":token,"UNINSTALL_ROUTE":"","UNINSTALL_ERROR":""}}))
}

fn files(config: &Config, token: &str, s: &Session, query: &str, error: &str) -> Vec<Value> {
    let selected: Vec<_> = s.scan.candidates.iter().filter(|c| c.selected).collect();
    let total = selected.iter().fold(0u64, |n, c| n.saturating_add(c.bytes));
    let summary = format!(
        "{} of {} files selected · {}",
        selected.len(),
        s.scan.candidates.len(),
        scan::size(total, selected.iter().any(|c| c.partial))
    );
    let app_selected = selected.iter().any(|c| c.path == s.scan.app.path);
    let cask = s.scan.homebrew.owner.as_ref().filter(|_| app_selected);
    let homebrew_blocked =
        app_selected && (!s.scan.homebrew.checked || s.scan.homebrew.error.is_some());
    let valid = !selected.is_empty() && !homebrew_blocked;
    let title = if selected.is_empty() {
        format!("Select files to remove · {}", s.scan.app.name)
    } else if !app_selected {
        format!("Remove selected files · keep {}", s.scan.app.name)
    } else if cask.is_some() {
        format!("Uninstall {} · Homebrew", s.scan.app.name)
    } else {
        format!("Uninstall {}", s.scan.app.name)
    };
    let extra_apps = cask.map_or(0, |c| {
        c.apps.iter().filter(|p| **p != s.scan.app.path).count()
    });
    let subtitle = if !error.is_empty() {
        error.to_owned()
    } else if homebrew_blocked {
        format!("{summary} · Homebrew check failed · ⌃↩ Actions")
    } else if extra_apps > 0 {
        format!(
            "{summary} · Cask includes {extra_apps} other {} · ⌃↩ Actions",
            if extra_apps == 1 { "app" } else { "apps" }
        )
    } else {
        format!("{summary} · ⌘↩ Toggle · ⌃↩ Actions")
    };
    let uninstall =
        json!({"op":"uninstall","token":token,"revision":s.revision,"query":query}).to_string();
    let actions = json!({"valid":true,"arg":request("actions",token,query),"subtitle":"Actions · selection, sorting, and scan details"});
    let all_selected = selected.len() == s.scan.candidates.len();
    let mut items = vec![json!({"uid":format!("{token}-summary"),"title":title,
    "subtitle":subtitle,"valid":valid,"arg":uninstall,
    "icon":{"type":"fileicon","path":s.scan.app.path},
    "mods":{
        "cmd":{"valid":true,"arg":request(if all_selected {"none"} else {"all"},token,query),
            "subtitle":if all_selected {"Deselect all files"} else {"Select all files"}},
            "ctrl":actions,
            "alt":{"valid":false}
    }})];
    let q = query.to_lowercase();
    let mut candidates: Vec<_> = s
        .scan
        .candidates
        .iter()
        .enumerate()
        .filter(|(_, c)| c.path.to_string_lossy().to_lowercase().contains(&q))
        .collect();
    if s.sort == "size" {
        candidates.sort_by_key(|(_, c)| (Reverse(c.bytes), &c.path));
    } else {
        candidates.sort_by_key(|(_, c)| &c.path);
    }
    if candidates.is_empty() {
        items.push(message(
            "No files match this filter",
            "Clear the filter to see all files. The total includes hidden selections.",
        ));
    }
    for (index, c) in candidates {
        let filename = c.path.file_name().unwrap_or_default().to_string_lossy();
        items.push(json!({"uid":format!("{token}-file-{index}"),
            "title":format!("{} {}",if c.selected {"☑"} else {"☐"},filename),
            "subtitle":format!("{} · {}",scan::short_path(c.path.parent().unwrap_or(Path::new("/")),&config.home),scan::size(c.bytes,c.partial)),
            "valid":valid,"arg":uninstall,"icon":{"type":"fileicon","path":c.path},
            "text":{"copy":c.path,"largetype":format!("{}\n{}",c.path.display(),c.reason)},"quicklookurl":c.path,
            "mods":{
                "cmd":{"valid":true,"arg":json!({"op":"toggle","token":token,"index":index,"query":query}).to_string(),
                    "subtitle":format!("{} {}",if c.selected {"Deselect"} else {"Select"},filename)},
                "ctrl":actions,
                "alt":reveal(&c.path)
            }}));
    }
    items
}

fn actions(token: &str, s: &Session) -> Vec<Value> {
    let mut back = menu_row("Back to files", &s.scan.app.name, "files", token, &s.filter);
    back["icon"] = json!({"path":"back.png"});
    let mut all = menu_row(
        "Select all files",
        "Include every file in the list",
        "all",
        token,
        &s.filter,
    );
    all["icon"] = json!({"path":"checked.png"});
    let mut none = menu_row(
        "Deselect all files",
        "Clear the selection",
        "none",
        token,
        &s.filter,
    );
    none["icon"] = json!({"path":"unchecked.png"});
    let sort = menu_row(
        if s.sort == "size" {
            "Sort by path"
        } else {
            "Sort by size"
        },
        if s.sort == "size" {
            "Currently largest first"
        } else {
            "Largest first"
        },
        "sort",
        token,
        &s.filter,
    );
    let detail_subtitle = if s.scan.homebrew.error.is_some() {
        "Homebrew ownership could not be checked".to_owned()
    } else if !s.scan.warnings.is_empty() {
        format!(
            "{} scan notices · file associations and Homebrew",
            s.scan.warnings.len()
        )
    } else {
        "File associations and Homebrew".to_owned()
    };
    let detail = menu_row(
        "Scan details",
        &detail_subtitle,
        "details",
        token,
        &s.filter,
    );
    vec![back, all, none, sort, detail]
}

fn details(config: &Config, token: &str, s: &Session) -> Vec<Value> {
    let mut back = menu_row("Back to files", &s.scan.app.name, "files", token, &s.filter);
    back["icon"] = json!({"path":"back.png"});
    let mut items = vec![back];
    if let Some(error) = &s.scan.homebrew.error {
        items.push(message("Homebrew check failed", error));
    } else if !s.scan.homebrew.checked {
        items.push(message(
            "Scan this app again",
            "This saved scan predates Homebrew detection.",
        ));
    } else if let Some(cask) = &s.scan.homebrew.owner {
        items.push(message(
            &format!("Homebrew · {}", cask.full_token),
            "Selecting the app uninstalls the entire cask, including its installed components.",
        ));
        for effect in brew::effects(cask) {
            items.push(message("Included in cask uninstall", &effect));
        }
    } else {
        items.push(message(
            "No installed Homebrew cask found",
            "Selected files will be moved to Trash.",
        ));
    }
    for warning in &s.scan.warnings {
        let mut item = message("Scan notice", warning);
        item["icon"] = json!({"path":"warning.png"});
        items.push(item);
    }
    for c in &s.scan.candidates {
        let mut item = message(
            &c.path.file_name().unwrap_or_default().to_string_lossy(),
            &format!("{} · {}", c.reason, scan::short_path(&c.path, &config.home)),
        );
        item["icon"] = json!({"type":"fileicon","path":c.path});
        item["mods"] = json!({"alt":reveal(&c.path)});
        item["text"] = json!({"copy":c.path,"largetype":c.reason});
        items.push(item);
    }
    items
}

fn report(config: &Config, s: &Session, error: &str) -> Vec<Value> {
    let selected: Vec<_> = s.scan.candidates.iter().filter(|c| c.selected).collect();
    let removed = selected
        .iter()
        .filter(|c| c.trashed_to.is_some() || c.removed_by_brew)
        .count();
    let title = if s.phase == Phase::Running {
        "Operation was interrupted · check each item".to_owned()
    } else if s.homebrew_result.as_ref().is_some_and(|r| !r.success) {
        "Homebrew uninstall did not finish".to_owned()
    } else {
        format!("{removed} of {} selected items removed", selected.len())
    };
    let open_trash = json!({"valid":true,"arg":config.home.join(".Trash"),
        "subtitle":"Open Trash in Finder","variables":{"UNINSTALL_ROUTE":"trash"}});
    let mut summary = open_trash.clone();
    summary["title"] = json!(title);
    summary["subtitle"] = json!(if !error.is_empty() {
        error
    } else if let Some(error) = &s.finder_error {
        error
    } else {
        "↩ Open Trash · ↩ or ⌥↩ on a file to reveal it"
    });
    summary["mods"] = json!({"alt":open_trash,"cmd":{"valid":false},"ctrl":{"valid":false}});
    let mut items = vec![summary];
    if let Some(outcome) = &s.homebrew_result {
        items.push(message("Homebrew uninstall result", &outcome.message));
    }
    for c in selected {
        let path = c.trashed_to.as_deref().unwrap_or(&c.path);
        let mut item = reveal(path);
        // Missing destinations may have been restored or emptied since the report was saved.
        // Let Finder handle permission errors: inability to stat a path doesn't mean it's gone.
        let available = !c.removed_by_brew && path.try_exists().unwrap_or(true);
        item["valid"] = json!(available);
        let mut alt = item.clone();
        let result = c.result.as_deref().unwrap_or("Not attempted");
        let subtitle = if available {
            format!("{result} · ↩ Reveal in Finder")
        } else {
            format!("{result} · No file to reveal")
        };
        alt["subtitle"] = json!(if available {
            "Reveal in Finder"
        } else {
            "No file to reveal"
        });
        item["title"] = json!(c.path.file_name().unwrap_or_default().to_string_lossy());
        item["subtitle"] = json!(subtitle);
        item["icon"] = json!({"type":"fileicon","path":path});
        item["quicklookurl"] = json!(path);
        item["text"] = json!({"copy":path});
        item["mods"] = json!({"alt":alt,"cmd":{"valid":false},"ctrl":{"valid":false}});
        items.push(item);
    }
    items
}
