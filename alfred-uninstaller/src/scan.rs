use crate::{brew, mac, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    env, fs,
    os::unix::fs::MetadataExt,
    path::{Component, Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub struct Config {
    pub home: PathBuf,
    pub roots: Vec<PathBuf>,
    pub libraries: Vec<PathBuf>,
    pub cache: PathBuf,
    pub brews: Vec<PathBuf>,
}
impl Config {
    pub fn from_env() -> Result<Self> {
        let home = fs::canonicalize(env::var_os("HOME").ok_or("HOME is missing")?)?;
        let mut roots = vec![PathBuf::from("/Applications"), home.join("Applications")];
        if let Ok(extra) = env::var("UNINSTALL_APP_FOLDERS") {
            for line in extra.lines().map(str::trim).filter(|s| !s.is_empty()) {
                let path = line
                    .strip_prefix("~/")
                    .map(|s| home.join(s))
                    .unwrap_or_else(|| PathBuf::from(line));
                if path.is_absolute() {
                    roots.push(path);
                }
            }
        }
        roots = roots
            .into_iter()
            .filter_map(|p| p.canonicalize().ok())
            .collect();
        roots.sort();
        roots.dedup();
        let cache = env::var_os("alfred_workflow_cache")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("Library/Caches/com.ariestwn.uninstaller-rust"));
        Ok(Self {
            libraries: vec![home.join("Library"), PathBuf::from("/Library")],
            home,
            roots,
            cache,
            brews: brew::executables(),
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct App {
    pub path: PathBuf,
    pub id: String,
    pub name: String,
    pub names: Vec<String>,
}

pub fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && !name
            .chars()
            .any(|c| c.is_control() || c == '/' || c == '\\')
        && name != "."
        && name != ".."
}
fn safe_id(id: &str) -> bool {
    safe_name(id)
        && id.contains('.')
        && !id.starts_with('.')
        && !id.ends_with('.')
        && !id.contains("..")
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'.' || c == b'-')
}
fn protected(app: &App) -> bool {
    let id = app.id.to_ascii_lowercase();
    id.starts_with("com.apple.")
        || id.starts_with("com.runningwithcrayons.alfred")
        || id == "com.ariestwn.uninstaller-rust"
        || app.path.starts_with("/System")
}
pub fn read_app(path: &Path) -> Result<App> {
    if path
        .extension()
        .is_none_or(|s| !s.eq_ignore_ascii_case("app"))
        || !fs::symlink_metadata(path)?.is_dir()
    {
        return Err("Choose an application bundle, not a file or symbolic link".into());
    }
    let path = path.canonicalize()?;
    if path
        .ancestors()
        .skip(1)
        .any(|p| p.extension().is_some_and(|s| s.eq_ignore_ascii_case("app")))
    {
        return Err("Nested helper apps must be removed with their parent app".into());
    }
    let fields = mac::plist_strings(
        &path.join("Contents/Info.plist"),
        &[
            "CFBundleIdentifier",
            "CFBundleName",
            "CFBundleDisplayName",
            "CFBundleExecutable",
            "CFBundlePackageType",
        ],
    )?;
    let id = fields
        .get("CFBundleIdentifier")
        .filter(|s| safe_id(s))
        .ok_or("This app has no valid bundle identifier")?
        .clone();
    if fields
        .get("CFBundlePackageType")
        .is_some_and(|v| v != "APPL")
    {
        return Err("This bundle is not an application".into());
    }
    let exe = fields
        .get("CFBundleExecutable")
        .filter(|s| safe_name(s))
        .ok_or("This app has no valid executable name")?;
    let exe_path = path.join("Contents/MacOS").join(exe).canonicalize()?;
    if !exe_path.starts_with(&path) || !exe_path.is_file() {
        return Err("App executable is outside its bundle".into());
    }
    let stem = path
        .file_stem()
        .and_then(|v| v.to_str())
        .ok_or("Unsupported app filename")?
        .to_owned();
    let name = fields
        .get("CFBundleDisplayName")
        .or_else(|| fields.get("CFBundleName"))
        .filter(|s| safe_name(s))
        .cloned()
        .unwrap_or(stem.clone());
    let mut names = vec![stem, name.clone()];
    if let Some(n) = fields.get("CFBundleName").filter(|s| safe_name(s)) {
        names.push(n.clone());
    }
    names.retain(|n| safe_name(n));
    names.sort();
    names.dedup();
    Ok(App {
        path,
        id,
        name,
        names,
    })
}

pub fn discover(config: &Config) -> (Vec<App>, Vec<String>) {
    let mut apps = Vec::new();
    let mut warnings = Vec::new();
    let mut seen = HashSet::new();
    let mut pending: Vec<_> = config.roots.iter().map(|p| (p.clone(), 0usize)).collect();
    while let Some((dir, depth)) = pending.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) => {
                warnings.push(format!("Cannot read {}: {e}", dir.display()));
                continue;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    warnings.push(format!("Cannot finish reading {}: {e}", dir.display()));
                    continue;
                }
            };
            let path = entry.path();
            let kind = match entry.file_type() {
                Ok(k) => k,
                Err(_) => continue,
            };
            if !kind.is_dir() || entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            if path
                .extension()
                .is_some_and(|s| s.eq_ignore_ascii_case("app"))
            {
                if let Ok(app) = read_app(&path) {
                    if seen.insert(app.path.clone()) {
                        apps.push(app);
                    }
                }
            } else if depth < 3 {
                pending.push((path, depth + 1));
            }
        }
    }
    apps.sort_by_key(|a| (a.name.to_lowercase(), a.path.clone()));
    (apps, warnings)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    dev: u64,
    ino: u64,
    mode: u32,
}
impl Identity {
    pub fn read(path: &Path) -> Result<Self> {
        let m = fs::symlink_metadata(path)?;
        if m.file_type().is_symlink() {
            return Err("Symbolic links are excluded".into());
        }
        if !m.is_dir() && !m.is_file() {
            return Err("Special files are excluded".into());
        }
        Ok(Self {
            dev: m.dev(),
            ino: m.ino(),
            mode: m.mode() & u32::from(libc::S_IFMT),
        })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Candidate {
    pub path: PathBuf,
    pub identity: Identity,
    pub reason: String,
    pub strong: bool,
    pub selected: bool,
    pub bytes: u64,
    pub partial: bool,
    pub result: Option<String>,
    pub trashed_to: Option<PathBuf>,
    #[serde(default)]
    pub removed_by_brew: bool,
}
impl Candidate {
    pub fn validate(&self) -> Result<()> {
        if self.path.canonicalize()? != self.path || Identity::read(&self.path)? != self.identity {
            return Err("This item changed since the scan. Start a fresh review.".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Scan {
    pub app: App,
    pub candidates: Vec<Candidate>,
    pub warnings: Vec<String>,
    pub duplicate: bool,
    #[serde(default)]
    pub homebrew: brew::Status,
}

pub fn allowed_app(config: &Config, app: &App) -> bool {
    !protected(app)
        && config
            .roots
            .iter()
            .any(|root| app.path.starts_with(root) && &app.path != root)
}

fn byhost_match(filename: &str, id: &str) -> bool {
    filename
        .strip_prefix(&format!("{id}."))
        .and_then(|s| s.strip_suffix(".plist"))
        .is_some_and(|s| {
            s.len() == 36
                && s.bytes().enumerate().all(|(i, b)| {
                    if [8, 13, 18, 23].contains(&i) {
                        b == b'-'
                    } else {
                        b.is_ascii_hexdigit()
                    }
                })
        })
}

/// Only complete identifiers and explicit suffixes match; never vendor prefixes or substrings.
fn match_entry(
    folder: &str,
    filename: &str,
    app: &App,
    name_matches: bool,
) -> Option<(bool, String)> {
    let file = filename.to_lowercase();
    let id = app.id.to_ascii_lowercase();
    let exact = match folder {
        "Preferences" => file == format!("{id}.plist"),
        "Preferences/ByHost" => byhost_match(&file, &id),
        "Saved Application State" => file == format!("{id}.savedstate"),
        "Cookies" => file == format!("{id}.binarycookies"),
        "HTTPStorages" => file == id || file == format!("{id}.binarycookies"),
        _ => file == id,
    };
    if exact {
        return Some((true, "Bundle identifier".into()));
    }
    if name_matches && ["Application Support", "Logs"].contains(&folder) {
        let generic = [
            "app",
            "application",
            "applications",
            "helper",
            "electron",
            "main",
            "data",
            "cache",
            "caches",
            "logs",
            "shared",
            "support",
            "resources",
        ];
        if app.names.iter().any(|name| {
            name.chars().count() >= 3
                && !generic.contains(&name.to_lowercase().as_str())
                && file == name.to_lowercase()
        }) {
            return Some((false, "App name".into()));
        }
    }
    None
}

pub fn scan(config: &Config, path: &Path) -> Result<Scan> {
    let app = read_app(path)?;
    if !allowed_app(config, &app) {
        return Err("This app is protected or outside the configured application folders".into());
    }
    let homebrew = brew::detect(config, &app);
    let (apps, mut warnings) = discover(config);
    let duplicate = apps
        .iter()
        .any(|a| a.path != app.path && a.id.eq_ignore_ascii_case(&app.id));
    let name_matches = !apps.iter().any(|a| {
        a.path != app.path
            && a.names
                .iter()
                .any(|n| app.names.iter().any(|own| n.eq_ignore_ascii_case(own)))
    });
    let mut candidates = vec![candidate(
        app.path.clone(),
        "Application bundle".into(),
        true,
    )?];
    if duplicate {
        warnings.push(
            "Another installed copy uses this bundle identifier. Shared app data is excluded."
                .into(),
        );
    } else {
        for library in &config.libraries {
            for folder in [
                "Application Support",
                "Caches",
                "Logs",
                "Preferences",
                "Preferences/ByHost",
                "HTTPStorages",
                "WebKit",
                "Cookies",
                "Saved Application State",
                "Containers",
            ] {
                let base = library.join(folder);
                let entries = match fs::read_dir(&base) {
                    Ok(v) => v,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(e) => {
                        warnings.push(format!(
                            "Cannot inspect {}: {e}",
                            short_path(&base, &config.home)
                        ));
                        continue;
                    }
                };
                for entry in entries {
                    let entry = match entry {
                        Ok(v) => v,
                        Err(e) => {
                            warnings.push(format!(
                                "Cannot finish inspecting {}: {e}",
                                short_path(&base, &config.home)
                            ));
                            continue;
                        }
                    };
                    let Some((strong, reason)) = match_entry(
                        folder,
                        &entry.file_name().to_string_lossy(),
                        &app,
                        name_matches,
                    ) else {
                        continue;
                    };
                    let path = entry.path();
                    // Reject symbolic-link ancestors as well as symbolic-link entries.
                    if path.canonicalize().ok().as_ref() != Some(&path) {
                        warnings.push(format!(
                            "Skipped linked or inaccessible item: {}",
                            short_path(&path, &config.home)
                        ));
                        continue;
                    }
                    match candidate(path.clone(), reason, strong) {
                        Ok(c) => candidates.push(c),
                        Err(e) => warnings.push(format!(
                            "Cannot inspect {}: {e}",
                            short_path(&path, &config.home)
                        )),
                    }
                }
            }
        }
    }
    // Parent directories cover their contents, even if configuration roots overlap.
    candidates.sort_by_key(|c| (c.path.components().count(), c.path.clone()));
    let mut unique: Vec<Candidate> = Vec::new();
    for c in candidates {
        if !unique.iter().any(|parent| c.path.starts_with(&parent.path)) {
            unique.push(c);
        }
    }
    unique.sort_by_key(|c| (c.path != app.path, c.path.clone()));
    let deadline = Instant::now() + Duration::from_secs(6);
    for c in &mut unique {
        (c.bytes, c.partial) = measure(&c.path, deadline);
    }
    warnings.sort();
    warnings.dedup();
    Ok(Scan {
        app,
        candidates: unique,
        warnings,
        duplicate,
        homebrew,
    })
}

fn candidate(path: PathBuf, reason: String, strong: bool) -> Result<Candidate> {
    Ok(Candidate {
        identity: Identity::read(&path)?,
        path,
        reason,
        strong,
        selected: true,
        bytes: 0,
        partial: false,
        result: None,
        trashed_to: None,
        removed_by_brew: false,
    })
}
pub fn measure(path: &Path, deadline: Instant) -> (u64, bool) {
    let mut pending = vec![path.to_path_buf()];
    let mut total = 0u64;
    let mut partial = false;
    let mut count = 0;
    let mut seen = HashSet::new();
    while let Some(path) = pending.pop() {
        count += 1;
        if count > 250_000 || Instant::now() >= deadline {
            return (total, true);
        }
        let m = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(_) => {
                partial = true;
                continue;
            }
        };
        if m.file_type().is_symlink() {
            continue;
        }
        if m.is_dir() {
            match fs::read_dir(path) {
                Ok(entries) => {
                    for entry in entries {
                        match entry {
                            Ok(e) => pending.push(e.path()),
                            Err(_) => partial = true,
                        }
                    }
                }
                Err(_) => partial = true,
            }
        } else if m.is_file() && seen.insert((m.dev(), m.ino())) {
            total = total.saturating_add(m.len());
        }
    }
    (total, partial)
}
pub fn short_path(path: &Path, home: &Path) -> String {
    path.strip_prefix(home)
        .map(|p| format!("~/{}", p.display()))
        .unwrap_or_else(|_| path.display().to_string())
}
pub fn size(bytes: u64, partial: bool) -> String {
    let units = ["bytes", "kB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut index = 0;
    while value >= 1000. && index < units.len() - 1 {
        value /= 1000.;
        index += 1;
    }
    let prefix = if partial { "≥ " } else { "" };
    if index == 0 {
        format!("{prefix}{bytes} bytes")
    } else {
        format!("{prefix}{value:.1} {}", units[index])
    }
}

pub fn valid_absolute(path: &Path) -> bool {
    path.is_absolute()
        && !path
            .components()
            .any(|p| matches!(p, Component::ParentDir | Component::CurDir))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> App {
        App {
            path: "/Applications/zoom.us.app".into(),
            id: "us.zoom.xos".into(),
            name: "zoom.us".into(),
            names: vec!["zoom.us".into()],
        }
    }
    #[test]
    fn matches_screenshot_without_catching_other_apps() {
        for (folder, name) in [
            ("Caches", "us.zoom.xos"),
            ("HTTPStorages", "us.zoom.xos.binarycookies"),
            ("WebKit", "us.zoom.xos"),
            ("Preferences", "us.zoom.xos.plist"),
        ] {
            assert_eq!(
                match_entry(folder, name, &app(), true).map(|v| v.0),
                Some(true)
            );
        }
        assert_eq!(
            match_entry("Application Support", "zoom.us", &app(), true).map(|v| v.0),
            Some(false)
        );
        for name in [
            "us.zoom",
            "us.zoom.xos.helper",
            "not.us.zoom.xos",
            "zoom",
            "us.zoom.xos.old",
        ] {
            assert!(match_entry("Caches", name, &app(), true).is_none());
        }
        assert!(match_entry("Application Support", "zoom.us", &app(), false).is_none());
    }
    #[test]
    fn byhost_requires_full_uuid_and_exact_domain() {
        assert!(byhost_match(
            "us.zoom.xos.12345678-1234-abcd-5678-123456789abc.plist",
            "us.zoom.xos"
        ));
        assert!(!byhost_match("us.zoom.xos.helper.plist", "us.zoom.xos"));
        assert!(!byhost_match(
            "us.zoom.xos.123456781234abcd5678123456789abc.plist",
            "us.zoom.xos"
        ));
    }
    #[test]
    fn names_cannot_escape_library_roots() {
        for name in ["..", "/tmp/data", "a/b", "a\\b", "foo\nbar", ""] {
            assert!(!safe_name(name));
        }
        for id in ["..", "com.x/../../", "com..x", ".com.app"] {
            assert!(!safe_id(id));
        }
        assert!(safe_id("com.example.App-2"));
    }
    #[test]
    fn size_does_not_follow_links_or_count_hardlinks_twice() {
        let t = tempfile::tempdir().unwrap();
        fs::write(t.path().join("a"), b"12345").unwrap();
        fs::hard_link(t.path().join("a"), t.path().join("b")).unwrap();
        std::os::unix::fs::symlink("/Applications", t.path().join("outside")).unwrap();
        assert_eq!(
            measure(t.path(), Instant::now() + Duration::from_secs(1)),
            (5, false)
        );
        assert_eq!(measure(t.path(), Instant::now()), (0, true));
    }
}
