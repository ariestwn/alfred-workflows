use crate::{Config, Result};
use std::{
    cmp::{Ordering, Reverse},
    collections::BinaryHeap,
    fs,
    os::unix::fs::MetadataExt,
    path::PathBuf,
    time::UNIX_EPOCH,
};

#[derive(Clone, Debug)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub date: String,
    pub bytes: u64,
    pub modified: u128,
    pub changed: (i64, i64),
}
impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.modified == other.modified && self.path == other.path
    }
}
impl Eq for Entry {}
impl Ord for Entry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.modified
            .cmp(&other.modified)
            .then_with(|| self.path.cmp(&other.path))
    }
}
impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Entry {
    pub fn key(&self, pixels: u32) -> String {
        // FNV-1a is stable between runs. File identity and both timestamps invalidate previews.
        let value = format!(
            "v1\0{}\0{}\0{}\0{:?}\0{}",
            self.path.display(),
            self.modified,
            self.bytes,
            self.changed,
            pixels
        );
        let hash = value.bytes().fold(0xcbf29ce484222325u64, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
        });
        format!("{hash:016x}")
    }
}

pub struct Page {
    pub entries: Vec<Entry>,
    pub total: usize,
    pub matched: usize,
    pub skipped: usize,
    pub number: usize,
    pub pages: usize,
}

fn is_image(path: &std::path::Path) -> bool {
    path.extension().and_then(|s| s.to_str()).is_some_and(|s| {
        matches!(
            s.to_ascii_lowercase().as_str(),
            "png" | "jpg" | "jpeg" | "webp" | "heic" | "heif" | "tif" | "tiff" | "gif" | "bmp"
        )
    })
}

pub fn scan(config: &Config, query: &str) -> Result<Page> {
    let files =
        fs::read_dir(&config.folder).map_err(|e| format!("Cannot read screenshots folder: {e}"))?;
    let tokens: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let keep = (config.page + 1).saturating_mul(config.page_size);
    let mut newest: BinaryHeap<Reverse<Entry>> = BinaryHeap::new();
    let (mut total, mut matched, mut skipped) = (0usize, 0usize, 0usize);
    for file in files {
        let Ok(file) = file else {
            skipped += 1;
            continue;
        };
        let Some(name) = file.file_name().to_str().map(str::to_owned) else {
            skipped += 1;
            continue;
        };
        if name.starts_with('.') || !is_image(&file.path()) {
            continue;
        }
        // Do not follow links or descend into folders; only this configured folder is browsed.
        if !file.file_type().is_ok_and(|t| t.is_file()) {
            continue;
        }
        let Ok(meta) = file.metadata() else {
            skipped += 1;
            continue;
        };
        let Ok(modified) = meta
            .modified()
            .and_then(|t| t.duration_since(UNIX_EPOCH).map_err(std::io::Error::other))
        else {
            skipped += 1;
            continue;
        };
        total += 1;
        let date = date_string(modified.as_secs() as i64);
        let haystack = format!("{} {}", name.to_lowercase(), date);
        if !tokens.iter().all(|token| haystack.contains(token)) {
            continue;
        }
        matched += 1;
        let entry = Entry {
            path: file.path(),
            name,
            date,
            bytes: meta.len(),
            modified: modified.as_nanos(),
            changed: (meta.ctime(), meta.ctime_nsec()),
        };
        if newest.len() < keep {
            newest.push(Reverse(entry));
        } else if newest.peek().is_some_and(|oldest| entry > oldest.0) {
            newest.pop();
            newest.push(Reverse(entry));
        }
    }
    let pages = matched.div_ceil(config.page_size).max(1);
    let number = config.page.min(pages - 1);
    let mut retained: Vec<Entry> = newest.into_iter().map(|entry| entry.0).collect();
    retained.sort_unstable_by(|a, b| b.cmp(a));
    let entries = retained
        .into_iter()
        .skip(number * config.page_size)
        .take(config.page_size)
        .collect();
    Ok(Page {
        entries,
        total,
        matched,
        skipped,
        number,
        pages,
    })
}

pub fn date_string(seconds: i64) -> String {
    // localtime_r writes to caller-owned storage, including when tests run concurrently.
    unsafe {
        let seconds = seconds as libc::time_t;
        let mut local: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&seconds, &mut local).is_null() {
            return String::new();
        }
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}",
            local.tm_year + 1900,
            local.tm_mon + 1,
            local.tm_mday,
            local.tm_hour,
            local.tm_min
        )
    }
}
