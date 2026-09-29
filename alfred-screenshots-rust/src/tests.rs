use super::*;
use std::{
    fs::{self, FileTimes},
    time::{Duration, UNIX_EPOCH},
};

fn config(root: &std::path::Path) -> Config {
    let folder = root.join("images");
    std::fs::create_dir_all(&folder).unwrap();
    Config {
        folder,
        cache: root.join("cache"),
        page_size: 12,
        pixels: 384,
        page: 0,
    }
}
fn file(c: &Config, name: &str, seconds: u64) {
    let f = fs::File::create(c.folder.join(name)).unwrap();
    f.set_times(FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(seconds)))
        .unwrap();
}

#[test]
fn pages_are_newest_first_without_omissions_or_duplicates() {
    let tmp = tempfile::tempdir().unwrap();
    let mut c = config(tmp.path());
    for i in 0..35 {
        file(&c, &format!("Shot-{i}.PNG"), 1_700_000_000 + i);
    }
    let mut paths = Vec::new();
    for page in 0..3 {
        c.page = page;
        let result = catalog::scan(&c, "").unwrap();
        assert_eq!(result.matched, 35);
        assert_eq!(result.pages, 3);
        assert!(result.entries.len() <= 12);
        paths.extend(result.entries.iter().map(|e| e.name.clone()));
    }
    assert_eq!(
        paths,
        (0..35)
            .rev()
            .map(|i| format!("Shot-{i}.PNG"))
            .collect::<Vec<_>>()
    );
}

#[test]
fn search_covers_all_files_before_paginating_and_preserves_literal_paths() {
    let tmp = tempfile::tempdir().unwrap();
    let c = config(tmp.path());
    for i in 0..40 {
        file(&c, &format!("unrelated-{i}.png"), 1_700_000_000 + i);
    }
    let name = "old 日本語 'quoted' $(touch nope).png";
    file(&c, name, 1_600_000_000);
    let result = catalog::scan(&c, "日本語 OLD").unwrap();
    assert_eq!(result.matched, 1);
    assert_eq!(result.entries[0].path, c.folder.join(name));
    assert_eq!(catalog::scan(&c, "no such screenshot").unwrap().matched, 0);
}

#[test]
fn scan_skips_hidden_files_folders_links_and_non_images() {
    let tmp = tempfile::tempdir().unwrap();
    let c = config(tmp.path());
    for name in [
        "one.png",
        "two.JPEG",
        "three.webp",
        ".hidden.png",
        "video.mov",
    ] {
        file(&c, name, 1_700_000_000);
    }
    fs::create_dir(c.folder.join("directory.png")).unwrap();
    std::os::unix::fs::symlink(c.folder.join("one.png"), c.folder.join("linked.png")).unwrap();
    let result = catalog::scan(&c, "").unwrap();
    assert_eq!(result.total, 3);
    assert_eq!(result.entries.len(), 3);
}

#[test]
fn shrinking_folders_clamp_to_the_last_page() {
    let tmp = tempfile::tempdir().unwrap();
    let mut c = config(tmp.path());
    for i in 0..15 {
        file(&c, &format!("{i}.png"), 1_700_000_000 + i);
    }
    c.page = 100;
    let result = catalog::scan(&c, "").unwrap();
    assert_eq!(result.number, 1);
    assert_eq!(result.entries.len(), 3);
    for entry in fs::read_dir(&c.folder).unwrap() {
        fs::remove_file(entry.unwrap().path()).unwrap();
    }
    let empty = catalog::scan(&c, "").unwrap();
    assert_eq!(empty.number, 0);
    assert_eq!(empty.pages, 1);
    assert!(empty.entries.is_empty());
}

#[test]
fn malformed_images_have_a_cached_fallback_without_hiding_the_original() {
    let tmp = tempfile::tempdir().unwrap();
    let c = config(tmp.path());
    file(&c, "corrupt.png", 1_700_000_000);
    let page = catalog::scan(&c, "").unwrap();
    let previews = thumbnails::prepare(&c, &page.entries).unwrap();
    assert!(previews[0].failed);
    assert_eq!(previews[0].path, PathBuf::from("icon.png"));
    let output = browse(&c, "").unwrap();
    assert_eq!(
        output["items"][0]["arg"],
        c.folder.join("corrupt.png").to_str().unwrap()
    );
}

#[test]
fn replacing_files_and_changing_preview_size_invalidate_the_cache() {
    let tmp = tempfile::tempdir().unwrap();
    let c = config(tmp.path());
    file(&c, "shot.png", 1_700_000_000);
    let first = catalog::scan(&c, "").unwrap().entries.remove(0);
    fs::write(&first.path, b"replaced data").unwrap();
    let second = catalog::scan(&c, "").unwrap().entries.remove(0);
    assert_ne!(first.key(384), second.key(384));
    assert_ne!(first.key(384), first.key(512));
}

#[cfg(target_os = "macos")]
#[test]
fn native_thumbnails_are_small_cached_and_only_generated_for_the_current_page() {
    let tmp = tempfile::tempdir().unwrap();
    let c = config(tmp.path());
    // A tiny uncompressed BMP, generated locally, exercises actual ImageIO decoding.
    let mut bmp = vec![0u8; 70];
    bmp[0..2].copy_from_slice(b"BM");
    bmp[2..6].copy_from_slice(&70u32.to_le_bytes());
    bmp[10..14].copy_from_slice(&54u32.to_le_bytes());
    bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
    bmp[18..22].copy_from_slice(&2i32.to_le_bytes());
    bmp[22..26].copy_from_slice(&2i32.to_le_bytes());
    bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
    bmp[28..30].copy_from_slice(&24u16.to_le_bytes());
    bmp[54..].fill(127);
    for i in 0..30 {
        fs::write(c.folder.join(format!("{i}.bmp")), &bmp).unwrap();
    }
    let page = catalog::scan(&c, "").unwrap();
    let first = thumbnails::prepare(&c, &page.entries).unwrap();
    assert_eq!(first.len(), 12);
    assert!(first.iter().all(|p| !p.failed && !p.cached));
    for preview in &first {
        let bytes = fs::read(&preview.path).unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        assert!(u32::from_be_bytes(bytes[16..20].try_into().unwrap()) <= c.pixels);
        assert!(u32::from_be_bytes(bytes[20..24].try_into().unwrap()) <= c.pixels);
    }
    let second = thumbnails::prepare(&c, &page.entries).unwrap();
    assert!(second.iter().all(|p| p.cached && !p.failed));
    let count = fs::read_dir(c.cache.join("thumbnails-v1"))
        .unwrap()
        .flatten()
        .filter(|f| f.path().extension().is_some_and(|s| s == "png"))
        .count();
    assert_eq!(count, 12);
}
