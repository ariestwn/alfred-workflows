use crate::{
    brew, mac,
    scan::{self, Config},
    session::{self, Panel, Phase, Session, Store},
    ui,
};
use serde_json::json;
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
};

struct Fixture {
    _temp: tempfile::TempDir,
    config: Config,
    app: PathBuf,
}

/// Simulates Finder processing a batch using the existing per-file fixture helpers.
fn execute_items<F>(
    store: &Store,
    token: &str,
    state: &mut Session,
    mut move_item: F,
) -> crate::Result<()>
where
    F: FnMut(&Path) -> crate::Result<PathBuf>,
{
    session::execute(store, token, state, |paths| {
        let mut outcome = mac::TrashOutcome::default();
        for path in paths {
            match move_item(path) {
                Ok(destination) => outcome.destinations.push(destination),
                Err(e) => {
                    outcome.error = Some(e.to_string());
                    break;
                }
            }
        }
        outcome
    })
}

#[test]
fn finished_report_opens_trash_and_reveals_moved_and_remaining_files() {
    let f = Fixture::new();
    let prefs = f.file("Preferences/us.zoom.xos.plist");
    let mut s = f.review();
    let trash = f.config.home.join(".Trash");
    fs::create_dir(&trash).unwrap();
    let destination = trash.join("zoom.us 2.app");
    fs::rename(&f.app, &destination).unwrap();
    s.scan.candidates[0].trashed_to = Some(destination.clone());
    s.scan.candidates[0].result = Some("Moved to Trash by Finder".into());
    s.phase = Phase::Finished;

    let report = ui::view(&f.config, "token", &s, "", "").unwrap();
    let summary = &report["items"][0];
    assert_eq!(summary["title"], "1 of 2 selected items removed");
    assert_eq!(summary["arg"], json!(trash));
    assert_eq!(summary["variables"]["UNINSTALL_ROUTE"], "trash");
    assert_eq!(summary["valid"], true);
    assert_eq!(summary["mods"]["alt"]["arg"], summary["arg"]);
    assert_eq!(summary["mods"]["alt"]["variables"], summary["variables"]);
    assert_eq!(summary["mods"]["alt"]["valid"], true);
    for (item, path) in report["items"].as_array().unwrap()[1..]
        .iter()
        .zip([destination, prefs])
    {
        assert_eq!(item["valid"], true);
        assert_eq!(item["arg"], json!(path));
        assert_eq!(item["variables"]["UNINSTALL_ROUTE"], "reveal");
        assert_eq!(item["mods"]["alt"]["arg"], item["arg"]);
        assert_eq!(item["mods"]["alt"]["variables"], item["variables"]);
        assert_eq!(item["mods"]["alt"]["valid"], true);
        assert_eq!(item["mods"]["cmd"]["valid"], false);
        assert_eq!(item["mods"]["ctrl"]["valid"], false);
    }
}

#[test]
fn finished_report_explains_when_there_is_no_file_to_reveal() {
    for removed_by_brew in [false, true] {
        let f = Fixture::new();
        let mut s = f.review();
        s.phase = Phase::Finished;
        let app = &mut s.scan.candidates[0];
        app.removed_by_brew = removed_by_brew;
        if !removed_by_brew {
            app.trashed_to = Some(f.config.home.join(".Trash/missing.app"));
        }
        let report = ui::view(&f.config, "token", &s, "", "").unwrap();
        let item = &report["items"][1];
        assert_eq!(item["valid"], false);
        assert_eq!(item["mods"]["alt"]["valid"], false);
        assert!(item["subtitle"]
            .as_str()
            .unwrap()
            .contains("No file to reveal"));
        assert_eq!(report["items"][0]["valid"], true);
    }
}

#[test]
fn finder_receives_one_batch_and_cancellation_preserves_verified_partial_moves() {
    let f = Fixture::new();
    let prefs = f.file("Preferences/us.zoom.xos.plist");
    let keep = f.file("Application Support/zoom.us/data");
    let store = Store::open(&f.config).unwrap();
    let token = store.create(f.review().scan).unwrap();
    let mut s = store.load(&token).unwrap();
    let keep_index = s
        .scan
        .candidates
        .iter()
        .position(|c| c.path == keep.parent().unwrap())
        .unwrap();
    session::change(&mut s, "toggle", Some(keep_index)).unwrap();
    session::preflight(&f.config, &s, s.revision).unwrap();
    let mut calls = 0;
    let destination = f.config.home.join("Finder Trash.app");
    session::execute(&store, &token, &mut s, |paths| {
        calls += 1;
        assert_eq!(paths, [f.app.clone(), prefs.clone()]);
        fs::rename(&f.app, &destination).unwrap();
        mac::TrashOutcome {
            destinations: vec![destination.clone()],
            error: Some("Cancelled in Finder".into()),
        }
    })
    .unwrap();
    assert_eq!(calls, 1);
    assert!(keep.exists() && prefs.exists());
    assert_eq!(s.scan.candidates[0].trashed_to, Some(destination));
    let report = ui::view(&f.config, &token, &s, "", "").unwrap();
    assert_eq!(report["items"][0]["title"], "1 of 2 selected items removed");
    assert_eq!(report["items"][0]["subtitle"], "Cancelled in Finder");
    assert!(session::execute(&store, &token, &mut s, |_| panic!("must not retry")).is_err());
}

#[test]
fn finder_cancellation_without_moves_and_unverified_destinations_are_not_successes() {
    for partial in [false, true] {
        let f = Fixture::new();
        let store = Store::open(&f.config).unwrap();
        let token = store.create(f.review().scan).unwrap();
        let mut s = store.load(&token).unwrap();
        session::execute(&store, &token, &mut s, |_| {
            if partial {
                fs::rename(&f.app, f.config.home.join("unknown.app")).unwrap();
            }
            let unrelated = f.config.home.join("unrelated.txt");
            fs::write(&unrelated, "unrelated file").unwrap();
            mac::TrashOutcome {
                destinations: vec![unrelated],
                error: Some("Finder did not finish".into()),
            }
        })
        .unwrap();
        assert!(s.scan.candidates[0].trashed_to.is_none());
        assert!(s.scan.candidates[0]
            .result
            .as_ref()
            .unwrap()
            .contains(if partial {
                "could not be verified"
            } else {
                "Not moved"
            }));
    }
}

#[test]
fn changed_file_blocks_the_entire_finder_batch() {
    let f = Fixture::new();
    let prefs = f.file("Preferences/us.zoom.xos.plist");
    let store = Store::open(&f.config).unwrap();
    let token = store.create(f.review().scan).unwrap();
    let mut s = store.load(&token).unwrap();
    fs::rename(&prefs, prefs.with_extension("old")).unwrap();
    fs::write(&prefs, "replaced").unwrap();
    session::execute(&store, &token, &mut s, |_| {
        panic!("no Finder request with stale paths")
    })
    .unwrap();
    assert!(f.app.exists() && prefs.exists());
    assert!(s
        .finder_error
        .as_ref()
        .unwrap()
        .contains("changed before removal"));
}
fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
fn make_app(root: &Path, name: &str, id: &str) -> PathBuf {
    let path = root.join(format!("{name}.app"));
    fs::create_dir_all(path.join("Contents/MacOS")).unwrap();
    fs::write(path.join("Contents/MacOS/main"), "fixture executable").unwrap();
    fs::write(path.join("Contents/Info.plist"), format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><plist version=\"1.0\"><dict><key>CFBundleIdentifier</key><string>{}</string><key>CFBundleName</key><string>{}</string><key>CFBundleExecutable</key><string>main</string><key>CFBundlePackageType</key><string>APPL</string></dict></plist>",xml(id),xml(name))).unwrap();
    path
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().canonicalize().unwrap();
        let root = home.join("Applications");
        fs::create_dir(&root).unwrap();
        let app = make_app(&root, "zoom.us", "us.zoom.xos");
        Self {
            config: Config {
                roots: vec![root],
                libraries: vec![home.join("Library")],
                cache: home.join("cache"),
                brews: Vec::new(),
                home,
            },
            app,
            _temp: temp,
        }
    }
    fn file(&self, path: &str) -> PathBuf {
        let path = self.config.home.join("Library").join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"sample").unwrap();
        path
    }
    fn review(&self) -> Session {
        Session {
            created: session::now(),
            revision: 0,
            phase: Phase::Review,
            sort: "size".into(),
            panel: Panel::Files,
            filter: String::new(),
            scan: scan::scan(&self.config, &self.app).unwrap(),
            homebrew_result: None,
            finder_error: None,
        }
    }
}

#[test]
fn native_metadata_accepts_xml_binary_and_unicode() {
    let f = Fixture::new();
    let path = make_app(
        &f.config.roots[0],
        "Notes & Café $(echo hello)",
        "com.example.notes",
    );
    assert_eq!(
        scan::read_app(&path).unwrap().name,
        "Notes & Café $(echo hello)"
    );
    assert!(Command::new("/usr/bin/plutil")
        .args(["-convert", "binary1"])
        .arg(path.join("Contents/Info.plist"))
        .status()
        .unwrap()
        .success());
    assert_eq!(scan::read_app(&path).unwrap().id, "com.example.notes");
    fs::write(path.join("Contents/Info.plist"), b"invalid plist").unwrap();
    assert!(scan::read_app(&path).is_err());
}

#[test]
fn zoom_fixture_matches_all_screenshot_paths_and_selects_every_file() {
    let f = Fixture::new();
    for path in [
        "Application Support/zoom.us/data",
        "Caches/us.zoom.xos/data",
        "HTTPStorages/us.zoom.xos/data",
        "HTTPStorages/us.zoom.xos.binarycookies",
        "Logs/zoom.us/log",
        "Preferences/us.zoom.xos.plist",
        "WebKit/us.zoom.xos/data",
        "Caches/us.zoom.xos.helper/data",
        "Application Support/Zoom/data",
        "Group Containers/us.zoom.xos/data",
    ] {
        f.file(path);
    }
    let s = f.review();
    assert_eq!(s.scan.candidates.len(), 8);
    assert_eq!(s.scan.candidates.iter().filter(|c| c.selected).count(), 8);
    assert_eq!(s.scan.candidates[0].path, f.app);
    assert!(s
        .scan
        .candidates
        .iter()
        .all(|c| !c.path.to_string_lossy().contains(".helper")));
}

#[test]
fn duplicate_install_excludes_shared_data() {
    let f = Fixture::new();
    f.file("Preferences/us.zoom.xos.plist");
    make_app(&f.config.roots[0], "Zoom Copy", "us.zoom.xos");
    let s = f.review();
    assert!(s.scan.duplicate);
    assert_eq!(s.scan.candidates.len(), 1);
}

#[test]
fn ambiguous_app_name_does_not_match_support_data() {
    let f = Fixture::new();
    f.file("Application Support/zoom.us/data");
    let extra = f.config.roots[0].join("Other");
    fs::create_dir(&extra).unwrap();
    make_app(&extra, "zoom.us", "com.other.zoom");
    assert_eq!(f.review().scan.candidates.len(), 1);
}

#[test]
fn symlink_apps_and_library_entries_are_excluded() {
    let f = Fixture::new();
    symlink(&f.app, f.config.roots[0].join("linked.app")).unwrap();
    assert!(scan::read_app(&f.config.roots[0].join("linked.app")).is_err());
    let outside = f.config.home.join("outside");
    fs::write(&outside, "keep me").unwrap();
    fs::create_dir_all(f.config.home.join("Library/Caches")).unwrap();
    symlink(&outside, f.config.home.join("Library/Caches/us.zoom.xos")).unwrap();
    assert_eq!(f.review().scan.candidates.len(), 1);
}

#[test]
fn protected_nested_and_out_of_scope_apps_cannot_be_scanned() {
    let f = Fixture::new();
    let system = make_app(&f.config.roots[0], "Fake System", "com.apple.fake");
    let alfred = make_app(
        &f.config.roots[0],
        "Alfred",
        "com.runningwithcrayons.Alfred",
    );
    let outside = make_app(&f.config.home, "Outside", "com.example.outside");
    let helper = make_app(&f.app.join("Contents"), "Helper", "com.example.helper");
    for p in [system, alfred, outside, helper] {
        assert!(scan::scan(&f.config, &p).is_err());
    }
}

#[test]
fn stale_selection_and_replaced_entries_are_rejected() {
    let f = Fixture::new();
    let prefs = f.file("Preferences/us.zoom.xos.plist");
    let mut s = f.review();
    let rev = s.revision;
    session::change(&mut s, "toggle", Some(1)).unwrap();
    assert!(session::preflight(&f.config, &s, rev).is_err());
    session::change(&mut s, "all", None).unwrap();
    fs::rename(&prefs, prefs.with_extension("backup")).unwrap();
    fs::write(&prefs, "replacement").unwrap();
    assert!(session::preflight(&f.config, &s, s.revision).is_err());
    assert!(f.app.exists());
    assert!(prefs.exists());
}

#[test]
fn tampered_paths_and_swapped_ancestors_cannot_reach_trash() {
    let f = Fixture::new();
    let prefs = f.file("Preferences/us.zoom.xos.plist");
    let mut s = f.review();
    let unrelated = f.config.home.join("personal.txt");
    fs::write(&unrelated, "do not touch").unwrap();
    s.scan.candidates[1].path = unrelated.clone();
    s.scan.candidates[1].identity = scan::Identity::read(&unrelated).unwrap();
    assert!(session::preflight(&f.config, &s, s.revision).is_err());
    let s = f.review();
    let parent = prefs.parent().unwrap();
    let backup = parent.with_extension("old");
    fs::rename(parent, &backup).unwrap();
    symlink(&backup, parent).unwrap();
    assert!(session::preflight(&f.config, &s, s.revision).is_err());
}

#[test]
fn cached_reviews_are_private_and_expire() {
    let f = Fixture::new();
    let store = Store::open(&f.config).unwrap();
    let token = store.create(f.review().scan).unwrap();
    assert!(session::token_ok(&token));
    let mut s = store.load(&token).unwrap();
    s.created = session::now() - 3601;
    store.save(&token, &s).unwrap();
    assert!(store.load(&token).is_err());
    assert!(store.load("../../other").is_err());
    drop(store);
    fs::set_permissions(
        f.config.cache.join("reviews"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    assert!(Store::open(&f.config).is_err());
}

#[test]
fn executes_only_selected_files_and_does_not_allow_replay() {
    let f = Fixture::new();
    let prefs = f.file("Preferences/us.zoom.xos.plist");
    let support = f.file("Application Support/zoom.us/data");
    let store = Store::open(&f.config).unwrap();
    let token = store.create(f.review().scan).unwrap();
    let mut s = store.load(&token).unwrap();
    let support_index = s
        .scan
        .candidates
        .iter()
        .position(|c| c.path == support.parent().unwrap())
        .unwrap();
    session::change(&mut s, "toggle", Some(support_index)).unwrap();
    session::preflight(&f.config, &s, s.revision).unwrap();
    let trash = f.config.home.join("Test Trash");
    fs::create_dir(&trash).unwrap();
    let mut moved = Vec::new();
    execute_items(&store, &token, &mut s, |p| {
        moved.push(p.to_owned());
        let dest = trash.join(p.file_name().unwrap());
        fs::rename(p, &dest)?;
        Ok(dest)
    })
    .unwrap();
    assert_eq!(moved, vec![f.app.clone(), prefs.clone()]);
    assert!(support.exists());
    assert!(!f.app.exists());
    assert!(!prefs.exists());
    assert_eq!(store.load(&token).unwrap().phase, Phase::Finished);
    assert!(execute_items(&store, &token, &mut s, |_| panic!("must not replay")).is_err());
}

#[test]
fn failure_to_move_app_preserves_its_data_and_reports_partial_result() {
    let f = Fixture::new();
    let prefs = f.file("Preferences/us.zoom.xos.plist");
    let store = Store::open(&f.config).unwrap();
    let token = store.create(f.review().scan).unwrap();
    let mut s = store.load(&token).unwrap();
    let mut calls = 0;
    execute_items(&store, &token, &mut s, |_| {
        calls += 1;
        Err("Permission denied".into())
    })
    .unwrap();
    assert_eq!(calls, 1);
    assert!(prefs.exists());
    assert!(f.app.exists());
    assert!(s.scan.candidates[1]
        .result
        .as_ref()
        .unwrap()
        .contains("Not moved"));
}

#[test]
fn running_app_process_blocks_preflight() {
    let f = Fixture::new();
    fs::copy("/bin/sleep", f.app.join("Contents/MacOS/main")).unwrap();
    let mut child = Command::new(f.app.join("Contents/MacOS/main"))
        .arg("30")
        .spawn()
        .unwrap();
    let s = f.review();
    let result = session::preflight(&f.config, &s, s.revision);
    let _ = child.kill();
    let _ = child.wait();
    assert!(result.unwrap_err().to_string().contains("Quit"));
}

#[test]
fn clean_file_list_uninstalls_on_return_and_toggles_on_command_return() {
    let f = Fixture::new();
    f.file("Preferences/us.zoom.xos.plist");
    let s = f.review();
    let view = ui::view(&f.config, "token", &s, "no match", "").unwrap();
    assert!(view["items"][0]["subtitle"]
        .as_str()
        .unwrap()
        .contains("2 of 2"));
    let view = ui::view(&f.config, "token", &s, "", "").unwrap();
    assert_eq!(view["items"].as_array().unwrap().len(), 3);
    assert_eq!(view["items"][0]["title"], "Uninstall zoom.us");
    let primary: serde_json::Value =
        serde_json::from_str(view["items"][0]["arg"].as_str().unwrap()).unwrap();
    assert_eq!(primary["op"], "uninstall");
    assert_eq!(primary["revision"], s.revision);
    for row in &view["items"].as_array().unwrap()[1..] {
        assert_eq!(row["arg"], view["items"][0]["arg"]);
        assert_eq!(row["icon"]["type"], "fileicon");
        assert!(row["title"].as_str().unwrap().starts_with("☑ "));
        let toggle: serde_json::Value =
            serde_json::from_str(row["mods"]["cmd"]["arg"].as_str().unwrap()).unwrap();
        assert_eq!(toggle["op"], "toggle");
        assert_eq!(row["mods"]["alt"]["variables"]["UNINSTALL_ROUTE"], "reveal");
    }
    assert_eq!(view["skipknowledge"], true);
}

#[test]
fn file_list_sorts_largest_first_even_when_data_is_larger_than_app() {
    let f = Fixture::new();
    let support = f.file("Application Support/zoom.us/data");
    fs::write(support, vec![b'x'; 10000]).unwrap();
    let s = f.review();
    let view = ui::view(&f.config, "token", &s, "", "").unwrap();
    assert_eq!(view["items"][1]["title"], "☑ zoom.us");
    assert_eq!(view["items"][2]["title"], "☑ zoom.us.app");
}

#[test]
fn toggling_and_actions_preserve_filter_and_can_reselect_from_empty() {
    let f = Fixture::new();
    f.file("Preferences/us.zoom.xos.plist");
    let token = Store::open(&f.config)
        .unwrap()
        .create(f.review().scan)
        .unwrap();
    let state = Store::open(&f.config).unwrap().load(&token).unwrap();
    assert_eq!(state.sort, "size");
    let view = ui::view(&f.config, &token, &state, "plist", "").unwrap();
    let uid = view["items"][1]["uid"].clone();
    let (_, query) = crate::action(
        &f.config,
        view["items"][1]["mods"]["cmd"]["arg"].as_str().unwrap(),
    )
    .unwrap();
    assert_eq!(query, "plist");
    let state = Store::open(&f.config).unwrap().load(&token).unwrap();
    let view = ui::view(&f.config, &token, &state, &query, "").unwrap();
    assert_eq!(view["items"][1]["title"], "☐ us.zoom.xos.plist");
    assert_eq!(view["items"][1]["uid"], uid);
    let (_, query) = crate::action(
        &f.config,
        view["items"][0]["mods"]["ctrl"]["arg"].as_str().unwrap(),
    )
    .unwrap();
    assert_eq!(query, "");
    let state = Store::open(&f.config).unwrap().load(&token).unwrap();
    assert_eq!(state.panel, Panel::Actions);
    assert!(session::preflight(&f.config, &state, state.revision).is_err());
    let menu = ui::view(&f.config, &token, &state, "", "").unwrap();
    let (_, query) = crate::action(&f.config, menu["items"][2]["arg"].as_str().unwrap()).unwrap();
    assert_eq!(query, "plist");
    let state = Store::open(&f.config).unwrap().load(&token).unwrap();
    assert!(state.scan.candidates.iter().all(|c| !c.selected));
    let view = ui::view(&f.config, &token, &state, &query, "").unwrap();
    assert_eq!(view["items"][0]["valid"], false);
    assert_eq!(view["items"][1]["valid"], false);
    assert_eq!(view["items"][1]["mods"]["cmd"]["valid"], true);
    crate::action(
        &f.config,
        view["items"][1]["mods"]["cmd"]["arg"].as_str().unwrap(),
    )
    .unwrap();
    let state = Store::open(&f.config).unwrap().load(&token).unwrap();
    assert_eq!(
        state.scan.candidates.iter().filter(|c| c.selected).count(),
        1
    );
}

#[test]
fn modifier_route_rejects_uninstall_even_if_given_the_default_argument() {
    let f = Fixture::new();
    let token = Store::open(&f.config)
        .unwrap()
        .create(f.review().scan)
        .unwrap();
    for op in ["uninstall", "trash", "scan"] {
        assert!(crate::edit_action(
            &f.config,
            &json!({"op":op,"token":token,"revision":0,"path":f.app}).to_string()
        )
        .is_err());
    }
    assert!(f.app.exists());
    assert_eq!(
        Store::open(&f.config).unwrap().load(&token).unwrap().phase,
        Phase::Review
    );
    crate::edit_action(&f.config, &json!({"op":"none","token":token}).to_string()).unwrap();
    assert!(Store::open(&f.config)
        .unwrap()
        .load(&token)
        .unwrap()
        .scan
        .candidates
        .iter()
        .all(|c| !c.selected));
}

#[test]
fn scan_notices_stay_in_details_and_action_errors_stay_visible() {
    let f = Fixture::new();
    let mut s = f.review();
    let warning = "Cannot inspect ~/Library/Cookies: Operation not permitted";
    s.scan.warnings.push(warning.into());
    let view = ui::view(&f.config, "token", &s, "", "").unwrap();
    assert_eq!(view["items"].as_array().unwrap().len(), 2);
    assert!(!view.to_string().contains(warning));
    let view = ui::view(
        &f.config,
        "token",
        &s,
        "",
        "Quit zoom.us before uninstalling",
    )
    .unwrap();
    assert_eq!(
        view["items"][0]["subtitle"],
        "Quit zoom.us before uninstalling"
    );
    session::change(&mut s, "details", None).unwrap();
    let view = ui::view(&f.config, "token", &s, "", "").unwrap();
    assert!(view.to_string().contains(warning));
}

#[test]
#[ignore = "Moves only a generated fixture through the real macOS Trash, then restores it"]
fn native_trash_roundtrip() {
    let temp = tempfile::Builder::new()
        .prefix("alfred-uninstaller-trash-test-")
        .tempdir()
        .unwrap();
    let path = temp.path().join(format!(
        "alfred-uninstaller-fixture-{}.txt",
        std::process::id()
    ));
    fs::write(&path, "temporary uninstaller verification fixture").unwrap();
    let outcome = mac::trash(std::slice::from_ref(&path));
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    let destination = outcome
        .destinations
        .into_iter()
        .next()
        .expect("Finder must return a verified destination");
    assert!(!path.exists());
    assert_eq!(
        fs::read_to_string(&destination).unwrap(),
        "temporary uninstaller verification fixture"
    );
    fs::rename(&destination, &path).unwrap();
}

impl Fixture {
    fn with_brew(mut self) -> Self {
        let root = self.config.home.join("fake-homebrew");
        fs::create_dir_all(root.join("Caskroom")).unwrap();
        let executable = root.join("brew");
        fs::write(&executable, r#"#!/bin/sh
fixture_root=$(/usr/bin/dirname "$0")
printf 'COMMAND:%s\n' "$1" >> "$fixture_root/commands"
case "$1" in
 info)
  if test -f "$fixture_root/info-fail"; then echo 'inventory unavailable' >&2; exit 2; fi
  /bin/cat "$fixture_root/info.json" ;;
 --caskroom) printf '%s/Caskroom\n' "$fixture_root" ;;
 uninstall)
  printf '%s\n' "$@" > "$fixture_root/uninstall-args"
  printf '%s\n' "$HOMEBREW_NO_AUTOREMOVE" "$HOMEBREW_NO_AUTO_UPDATE" "$NONINTERACTIVE" > "$fixture_root/flags"
  if test -f "$fixture_root/uninstall-fail"; then echo 'uninstall failed' >&2; exit 9; fi
  app_path=$(/bin/cat "$fixture_root/app-path")
  if ! test -f "$fixture_root/leave-app"; then /bin/mv "$app_path" "$fixture_root/removed.app"; fi
  if test -f "$fixture_root/hook-path"; then
   hook_path=$(/bin/cat "$fixture_root/hook-path")
   /bin/mv "$hook_path" "$fixture_root/removed-by-hook"
  fi
  if ! test -f "$fixture_root/still-installed"; then printf '{"casks":[]}' > "$fixture_root/info.json"; fi ;;
 list) if test -f "$fixture_root/still-installed"; then printf 'zoom-cask\n'; fi ;;
 *) echo 'unexpected command' >&2; exit 7 ;;
esac
"#).unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(root.join("app-path"), self.app.to_str().unwrap()).unwrap();
        fs::write(root.join("info.json"),json!({"casks":[{"token":"zoom-cask","full_token":"zoom-cask","installed":"1.0","artifacts":[{"app":["zoom.us.app"],"target":self.app}]}]}).to_string()).unwrap();
        self.config.brews = vec![executable];
        self
    }
    fn brew_file(&self, name: &str) -> PathBuf {
        self.config.home.join("fake-homebrew").join(name)
    }
    fn prepare_brew(&self) -> (Store, String, Session) {
        let store = Store::open(&self.config).unwrap();
        let token = store.create(self.review().scan).unwrap();
        let s = store.load(&token).unwrap();
        session::preflight(&self.config, &s, s.revision).unwrap();
        (store, token, s)
    }
}

#[test]
fn homebrew_uninstalls_cask_before_cleaning_only_selected_leftovers() {
    let f = Fixture::new().with_brew();
    let prefs = f.file("Preferences/us.zoom.xos.plist");
    let keep = f.file("Application Support/zoom.us/data");
    let (store, token, mut s) = f.prepare_brew();
    let keep_index = s
        .scan
        .candidates
        .iter()
        .position(|c| c.path == keep.parent().unwrap())
        .unwrap();
    session::change(&mut s, "toggle", Some(keep_index)).unwrap();
    session::preflight(&f.config, &s, s.revision).unwrap();
    let mut paths = Vec::new();
    execute_items(&store, &token, &mut s, |p| {
        assert!(!f.app.exists(), "Homebrew must remove the app first");
        paths.push(p.to_owned());
        let dest = f.config.home.join("trashed-pref");
        fs::rename(p, &dest)?;
        Ok(dest)
    })
    .unwrap();
    assert_eq!(paths, vec![prefs]);
    assert!(keep.exists());
    assert!(s.scan.candidates[0].removed_by_brew);
    assert!(s.homebrew_result.as_ref().unwrap().success);
    assert_eq!(
        fs::read_to_string(f.brew_file("uninstall-args")).unwrap(),
        "uninstall\n--cask\n--\nzoom-cask\n"
    );
    assert_eq!(
        fs::read_to_string(f.brew_file("flags")).unwrap(),
        "1\n1\n1\n"
    );
    let report = ui::view(&f.config, &token, &s, "", "").unwrap();
    assert_eq!(report["items"][0]["title"], "2 of 2 selected items removed");
}

#[test]
fn homebrew_failure_does_not_fall_back_to_trash_or_clean_data() {
    let f = Fixture::new().with_brew();
    let prefs = f.file("Preferences/us.zoom.xos.plist");
    let (store, token, mut s) = f.prepare_brew();
    fs::write(f.brew_file("uninstall-fail"), "").unwrap();
    execute_items(&store, &token, &mut s, |_| {
        panic!("No Trash calls after Homebrew failure")
    })
    .unwrap();
    assert!(f.app.exists());
    assert!(prefs.exists());
    assert!(!s.homebrew_result.unwrap().success);
    assert_eq!(store.load(&token).unwrap().phase, Phase::Finished);
}

#[test]
fn homebrew_success_must_also_remove_installed_record() {
    let f = Fixture::new().with_brew();
    let prefs = f.file("Preferences/us.zoom.xos.plist");
    let (store, token, mut s) = f.prepare_brew();
    fs::write(f.brew_file("still-installed"), "").unwrap();
    execute_items(&store, &token, &mut s, |_| {
        panic!("No further cleanup while still registered")
    })
    .unwrap();
    assert!(prefs.exists());
    assert!(!s.homebrew_result.as_ref().unwrap().success);
    assert!(s.homebrew_result.unwrap().message.contains("still lists"));
}

#[test]
fn keeping_the_app_also_keeps_its_homebrew_registration() {
    let f = Fixture::new().with_brew();
    f.file("Preferences/us.zoom.xos.plist");
    let store = Store::open(&f.config).unwrap();
    let token = store.create(f.review().scan).unwrap();
    let mut s = store.load(&token).unwrap();
    session::change(&mut s, "toggle", Some(0)).unwrap();
    session::preflight(&f.config, &s, s.revision).unwrap();
    execute_items(&store, &token, &mut s, |p| {
        let target = f.config.home.join("trash");
        fs::rename(p, &target)?;
        Ok(target)
    })
    .unwrap();
    assert!(f.app.exists());
    assert!(!f.brew_file("uninstall-args").exists());
    assert!(s.homebrew_result.is_none());
}

#[test]
fn cask_hooks_removing_selected_files_are_not_reported_as_trash_failures() {
    let f = Fixture::new().with_brew();
    let prefs = f.file("Preferences/us.zoom.xos.plist");
    fs::write(f.brew_file("hook-path"), prefs.to_str().unwrap()).unwrap();
    let (store, token, mut s) = f.prepare_brew();
    execute_items(&store, &token, &mut s, |_| {
        panic!("Both selected entries were already removed by Homebrew")
    })
    .unwrap();
    assert!(s.scan.candidates.iter().all(|c| c.removed_by_brew));
}

#[test]
fn cask_success_with_remaining_app_uses_trash_for_the_leftover_bundle() {
    let f = Fixture::new().with_brew();
    fs::write(f.brew_file("leave-app"), "").unwrap();
    let (store, token, mut s) = f.prepare_brew();
    let mut moved = Vec::new();
    execute_items(&store, &token, &mut s, |p| {
        moved.push(p.to_owned());
        let target = f.config.home.join("trash.app");
        fs::rename(p, &target)?;
        Ok(target)
    })
    .unwrap();
    assert_eq!(moved, vec![f.app.clone()]);
    assert!(s.homebrew_result.unwrap().success);
}

#[test]
fn ownership_changes_and_inventory_failures_block_app_removal() {
    let f = Fixture::new().with_brew();
    let s = f.review();
    fs::write(f.brew_file("info.json"), "{\"casks\":[]}").unwrap();
    assert!(session::preflight(&f.config, &s, s.revision)
        .unwrap_err()
        .to_string()
        .contains("changed"));
    fs::write(f.brew_file("info-fail"), "").unwrap();
    let s = f.review();
    assert!(s.scan.homebrew.error.is_some());
    let view = ui::view(&f.config, "token", &s, "", "").unwrap();
    assert_eq!(view["items"][0]["valid"], false);
}

#[test]
fn installed_receipt_and_appdir_win_over_the_current_cask_definition() {
    let f = Fixture::new().with_brew();
    let metadata = f.brew_file("Caskroom/zoom-cask/.metadata");
    fs::create_dir_all(&metadata).unwrap();
    fs::write(
        metadata.join("INSTALL_RECEIPT.json"),
        json!({"uninstall_artifacts":[{"app":["Original.app",{"target":"zoom.us.app"}]}]})
            .to_string(),
    )
    .unwrap();
    fs::write(
        metadata.join("config.json"),
        json!({"default":{"appdir":"/Applications"},"explicit":{"appdir":f.config.roots[0]}})
            .to_string(),
    )
    .unwrap();
    fs::write(f.brew_file("info.json"),json!({"casks":[{"token":"zoom-cask","installed":"1.0","artifacts":[{"app":["NewName.app"],"target":"/Applications/NewName.app"}]}]}).to_string()).unwrap();
    let s = f.review();
    let cask = s.scan.homebrew.owner.unwrap();
    assert_eq!(cask.apps, vec![f.app.clone()]);
    assert_eq!(cask.artifacts[0]["app"][0], "Original.app");
}

#[test]
fn same_named_app_at_another_path_does_not_match_a_cask() {
    let f = Fixture::new().with_brew();
    fs::write(f.brew_file("info.json"),json!({"casks":[{"token":"zoom-cask","installed":"1.0","artifacts":[{"app":["zoom.us.app"],"target":"/somewhere-else/zoom.us.app"}]}]}).to_string()).unwrap();
    let status = brew::detect(&f.config, &scan::read_app(&f.app).unwrap());
    assert!(status.owner.is_none());
    assert!(status.error.is_none());
}

#[test]
fn multi_app_cask_scope_is_shown_in_header_and_details() {
    let f = Fixture::new().with_brew();
    let extra = make_app(&f.config.roots[0], "Second App", "com.example.second");
    fs::write(f.brew_file("info.json"),json!({"casks":[{"token":"zoom-cask","installed":"1.0","artifacts":[{"app":["zoom.us.app"],"target":f.app},{"app":["Second App.app"],"target":extra},{"binary":["tool"],"target":"/opt/homebrew/bin/tool"}]}]}).to_string()).unwrap();
    let mut s = f.review();
    let v = ui::view(&f.config, "token", &s, "", "").unwrap();
    assert!(v["items"][0]["title"]
        .as_str()
        .unwrap()
        .contains("Homebrew"));
    assert!(v["items"][0]["subtitle"]
        .as_str()
        .unwrap()
        .contains("1 other app"));
    session::change(&mut s, "details", None).unwrap();
    let v = ui::view(&f.config, "token", &s, "", "").unwrap();
    let text = v.to_string();
    assert!(text.contains("Second App.app"));
    assert!(text.contains("uninstalls the entire cask"));
    assert!(text.contains("binary"));
}
