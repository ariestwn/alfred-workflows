use crate::{
    brew, mac,
    scan::{self, Config, Scan},
    Result,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Review,
    Confirm,
    Running,
    Finished,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Panel {
    #[default]
    Files,
    Actions,
    Details,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Session {
    pub created: u64,
    pub revision: u64,
    pub phase: Phase,
    pub sort: String,
    #[serde(default)]
    pub panel: Panel,
    #[serde(default)]
    pub filter: String,
    pub scan: Scan,
    #[serde(default)]
    pub homebrew_result: Option<brew::Outcome>,
    #[serde(default)]
    pub finder_error: Option<String>,
}
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn token_ok(token: &str) -> bool {
    token.len() == 32 && token.bytes().all(|b| b.is_ascii_hexdigit())
}

pub struct Store {
    root: PathBuf,
    _lock: File,
}
impl Store {
    pub fn open(config: &Config) -> Result<Self> {
        if !scan::valid_absolute(&config.cache) {
            return Err("The workflow cache must be an absolute path".into());
        }
        fs::create_dir_all(&config.cache)?;
        let root = config.cache.join("reviews");
        match fs::create_dir(&root) {
            Ok(()) => fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
        let metadata = fs::symlink_metadata(&root)?;
        if !metadata.is_dir()
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.mode() & 0o077 != 0
        {
            return Err(
                "The workflow review cache must be a private directory owned by you".into(),
            );
        }
        let root = root.canonicalize()?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(root.join(".lock"))?;
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(Self { root, _lock: lock })
    }
    fn path(&self, token: &str) -> Result<PathBuf> {
        if !token_ok(token) {
            return Err("This review is invalid. Start again with uninstall.".into());
        }
        Ok(self.root.join(format!("{token}.json")))
    }
    pub fn load(&self, token: &str) -> Result<Session> {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(self.path(token)?)?;
        let metadata = file.metadata()?;
        if !metadata.is_file()
            || metadata.len() > 4 * 1024 * 1024
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.mode() & 0o077 != 0
        {
            return Err("Invalid review cache file".into());
        }
        let session: Session = serde_json::from_reader(file)?;
        if now().saturating_sub(session.created) > 3600 {
            return Err("This review expired. Start again with uninstall.".into());
        }
        Ok(session)
    }
    pub fn save(&self, token: &str, session: &Session) -> Result<()> {
        let mut file = tempfile::NamedTempFile::new_in(&self.root)?;
        serde_json::to_writer(&mut file, session)?;
        file.flush()?;
        file.as_file().sync_all()?;
        file.persist(self.path(token)?)?;
        File::open(&self.root)?.sync_all()?;
        Ok(())
    }
    pub fn create(&self, scan: Scan) -> Result<String> {
        // Expired reviews contain only paths and selection state; remove our own JSON files.
        for entry in fs::read_dir(&self.root)?.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|s| s == "json")
                && path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .is_some_and(token_ok)
                && fs::symlink_metadata(&path)
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|m| m.elapsed().ok())
                    .is_some_and(|age| age.as_secs() > 3600)
            {
                let _ = fs::remove_file(path);
            }
        }
        let mut random = [0u8; 16];
        File::open("/dev/urandom")?.read_exact(&mut random)?;
        let token: String = random.iter().map(|b| format!("{b:02x}")).collect();
        self.save(
            &token,
            &Session {
                created: now(),
                revision: 0,
                phase: Phase::Review,
                sort: "size".into(),
                panel: Panel::Files,
                filter: String::new(),
                scan,
                homebrew_result: None,
                finder_error: None,
            },
        )?;
        Ok(token)
    }
}

pub fn change(session: &mut Session, op: &str, index: Option<usize>) -> Result<()> {
    if !matches!(session.phase, Phase::Review | Phase::Confirm) {
        return Err("This review has already been used. Start a fresh review.".into());
    }
    match op {
        "toggle" => {
            let c = session
                .scan
                .candidates
                .get_mut(index.ok_or("Missing file index")?)
                .ok_or("Invalid file index")?;
            c.selected = !c.selected;
            session.phase = Phase::Review;
        }
        "all" => {
            for c in &mut session.scan.candidates {
                c.selected = true;
            }
            session.phase = Phase::Review;
        }
        "none" => {
            for c in &mut session.scan.candidates {
                c.selected = false;
            }
            session.phase = Phase::Review;
        }
        "sort" => {
            session.sort = if session.sort == "path" {
                "size"
            } else {
                "path"
            }
            .into();
            session.phase = Phase::Review;
        }
        "files" | "actions" | "details" => {}
        _ => return Err("Unknown review action".into()),
    }
    session.phase = Phase::Review;
    session.panel = match op {
        "actions" => Panel::Actions,
        "details" => Panel::Details,
        _ => Panel::Files,
    };
    session.revision += 1;
    Ok(())
}

pub fn preflight(config: &Config, session: &Session, revision: u64) -> Result<()> {
    if !matches!(session.phase, Phase::Review | Phase::Confirm)
        || session.panel != Panel::Files
        || session.revision != revision
    {
        return Err(
            "The selection changed or was already used. Reload the file list before uninstalling."
                .into(),
        );
    }
    if !session.scan.candidates.iter().any(|c| c.selected) {
        return Err("No files are selected".into());
    }
    // Rebuild the allowlist from the current app, never trust stored paths alone.
    let fresh = scan::scan(config, &session.scan.app.path)?;
    if fresh.app.id != session.scan.app.id {
        return Err("The application changed. Start a fresh review.".into());
    }
    // Always validate the app, including when only its data was selected.
    let app = session
        .scan
        .candidates
        .iter()
        .find(|c| c.path == session.scan.app.path)
        .ok_or("Application is missing from the review")?;
    app.validate()?;
    if app.selected {
        if let Some(error) = &fresh.homebrew.error {
            return Err(error.clone().into());
        }
        if !session.scan.homebrew.checked || fresh.homebrew != session.scan.homebrew {
            return Err(
                "Homebrew ownership or its uninstall plan changed. Start a fresh review.".into(),
            );
        }
    }
    let (apps, warnings) = scan::discover(config);
    if !warnings.is_empty() {
        return Err("Cannot recheck all configured application folders. Check their permissions and try again.".into());
    }
    let mut running_paths: Vec<_> = apps
        .iter()
        .filter(|a| a.id.eq_ignore_ascii_case(&fresh.app.id))
        .map(|a| a.path.clone())
        .collect();
    running_paths.push(fresh.app.path.clone());
    if app.selected {
        if let Some(cask) = &fresh.homebrew.owner {
            running_paths.extend(cask.apps.clone());
        }
    }
    if mac::running_in(&running_paths)? {
        return Err(format!(
            "Quit {} and its helper processes, then try again. Nothing was moved.",
            fresh.app.name
        )
        .into());
    }
    for selected in session.scan.candidates.iter().filter(|c| c.selected) {
        if !fresh
            .candidates
            .iter()
            .any(|c| c.path == selected.path && c.identity == selected.identity)
        {
            return Err(format!(
                "{} changed or is no longer associated with this app. Start a fresh review.",
                selected.path.display()
            )
            .into());
        }
        selected.validate()?;
    }
    Ok(())
}

/// The caller must hold the store lock and pass preflight first.
pub fn execute<F>(store: &Store, token: &str, session: &mut Session, mut trash: F) -> Result<()>
where
    F: FnMut(&[PathBuf]) -> mac::TrashOutcome,
{
    if !matches!(session.phase, Phase::Review | Phase::Confirm) || session.panel != Panel::Files {
        return Err("Open the file list before uninstalling".into());
    }
    session.phase = Phase::Running;
    store.save(token, session)?;
    let app_index = session
        .scan
        .candidates
        .iter()
        .position(|c| c.path == session.scan.app.path)
        .ok_or("Application is missing from the review")?;
    let cask = session
        .scan
        .homebrew
        .owner
        .clone()
        .filter(|_| session.scan.candidates[app_index].selected);
    if let Some(cask) = &cask {
        session.homebrew_result = Some(brew::Outcome { success:false, message:format!("Homebrew uninstall started for {}. If interrupted, check Homebrew before trying again.",cask.full_token) });
        store.save(token, session)?;
        let result = session.scan.candidates[app_index]
            .validate()
            .and_then(|_| brew::uninstall(cask));
        match result {
            Ok(()) => {
                session.homebrew_result = Some(brew::Outcome {
                    success: true,
                    message: format!(
                        "Uninstalled {} through Homebrew; its installed cask record is gone.",
                        cask.full_token
                    ),
                })
            }
            Err(e) => {
                session.homebrew_result = Some(brew::Outcome { success:false, message:format!("Homebrew uninstall did not finish: {e}. It may have made partial changes. Selected leftover cleanup was not started.") });
                for c in &mut session.scan.candidates {
                    if c.selected {
                        c.result = Some(
                            "Homebrew did not finish; no additional file cleanup attempted".into(),
                        );
                    }
                }
                session.phase = Phase::Finished;
                return store.save(token, session);
            }
        }
        store.save(token, session)?;
    }
    let mut pending = Vec::new();
    for index in 0..session.scan.candidates.len() {
        if !session.scan.candidates[index].selected {
            continue;
        }
        let path = session.scan.candidates[index].path.clone();
        if cask.is_some()
            && fs::symlink_metadata(&path).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
        {
            session.scan.candidates[index].removed_by_brew = true;
            session.scan.candidates[index].result =
                Some("Removed by Homebrew's uninstall routine".into());
            store.save(token, session)?;
            continue;
        }
        // Validate the whole batch before Finder receives any files.
        if let Err(e) = session.scan.candidates[index].validate() {
            session.finder_error = Some(format!("{} changed before removal: {e}", path.display()));
            for c in &mut session.scan.candidates {
                if c.selected && !c.removed_by_brew {
                    c.result = Some(
                        "Not attempted: a selected file changed before Finder could start".into(),
                    );
                }
            }
            session.phase = Phase::Finished;
            return store.save(token, session);
        }
        pending.push(index);
    }
    if !pending.is_empty() {
        for &index in &pending {
            session.scan.candidates[index].result = Some(
                "Sent to Finder; check its authorization window and Trash if interrupted".into(),
            );
        }
        store.save(token, session)?;
        let paths = pending
            .iter()
            .map(|&i| session.scan.candidates[i].path.clone())
            .collect::<Vec<_>>();
        let outcome = trash(&paths);
        session.finder_error = outcome.error;
        let destinations: Vec<_> = outcome
            .destinations
            .into_iter()
            .filter_map(|p| scan::Identity::read(&p).ok().map(|identity| (p, identity)))
            .collect();
        for &index in &pending {
            let c = &mut session.scan.candidates[index];
            let source_absent = fs::symlink_metadata(&c.path)
                .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound);
            let destination = destinations
                .iter()
                .find(|(_, identity)| *identity == c.identity);
            if source_absent {
                if let Some((path, _)) = destination {
                    c.trashed_to = Some(path.clone());
                    c.result = Some("Moved to Trash by Finder".into());
                } else {
                    c.result = Some("No longer at its original path; Trash destination could not be verified. Check Finder.".into());
                }
            } else {
                c.result = Some(format!(
                    "Not moved: {}",
                    session
                        .finder_error
                        .as_deref()
                        .unwrap_or("Finder did not move this item; check its authorization window")
                ));
            }
        }
        store.save(token, session)?;
    }
    session.phase = Phase::Finished;
    store.save(token, session)
}
