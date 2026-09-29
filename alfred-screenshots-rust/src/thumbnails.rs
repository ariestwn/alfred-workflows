use crate::{catalog::Entry, Config, Result};
use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
    time::{Duration, SystemTime},
};

pub struct Preview {
    pub path: PathBuf,
    pub cached: bool,
    pub failed: bool,
}

pub fn prepare(config: &Config, entries: &[Entry]) -> Result<Vec<Preview>> {
    let folder = config.cache.join("thumbnails-v1");
    fs::create_dir_all(&folder).map_err(|e| format!("Cannot create thumbnail cache: {e}"))?;
    // Private screenshot previews stay in a private cache, never beside source files.
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    prune(&folder);
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<Option<Preview>>> =
        Mutex::new((0..entries.len()).map(|_| None).collect());
    std::thread::scope(|scope| {
        // Fixed concurrency bounds simultaneous image decoders, including on large-core Macs.
        for _ in 0..4.min(entries.len()) {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(entry) = entries.get(i) else { break };
                let path = folder.join(format!("{}.png", entry.key(config.pixels)));
                let failure = folder.join(format!("{}.failed", entry.key(config.pixels)));
                let cached = path.metadata().is_ok_and(|m| m.len() > 0);
                let recent_failure = failure
                    .metadata()
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.elapsed().ok())
                    .is_some_and(|age| age < Duration::from_secs(300));
                let result = if cached {
                    Ok(())
                } else if recent_failure {
                    Err("Preview unavailable".to_owned())
                } else {
                    tempfile::Builder::new()
                        .prefix(".preview-")
                        .suffix(".png")
                        .tempfile_in(&folder)
                        .map_err(|e| e.to_string())
                        .and_then(|temp| {
                            native::render(&entry.path, temp.path(), config.pixels)?;
                            temp.persist(&path).map_err(|e| e.to_string())?;
                            Ok(())
                        })
                };
                let failed = result.is_err();
                if failed && !recent_failure {
                    let _ = fs::write(&failure, b"");
                }
                results.lock().unwrap()[i] = Some(Preview {
                    path: if failed {
                        PathBuf::from("icon.png")
                    } else {
                        path
                    },
                    cached,
                    failed,
                });
            });
        }
    });
    Ok(results
        .into_inner()
        .unwrap()
        .into_iter()
        .map(Option::unwrap)
        .collect())
}

fn prune(folder: &std::path::Path) {
    let marker = folder.join(".last-cleanup");
    let due = marker
        .metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_none_or(|age| age > Duration::from_secs(86400));
    if !due {
        return;
    }
    // Keep at most 512 MB of previews; also expire previews older than thirty days.
    let Ok(files) = fs::read_dir(folder) else {
        return;
    };
    let mut cached = Vec::new();
    let mut bytes = 0;
    for file in files.flatten() {
        if !file.file_type().is_ok_and(|t| t.is_file()) {
            continue;
        }
        let path = file.path();
        if !matches!(
            path.extension().and_then(|s| s.to_str()),
            Some("png" | "failed")
        ) {
            continue;
        }
        let Ok(meta) = file.metadata() else { continue };
        let time = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        bytes += meta.len();
        cached.push((time, meta.len(), path));
    }
    cached.sort_unstable();
    for (time, size, path) in cached {
        if bytes <= 512 * 1024 * 1024
            && time.elapsed().unwrap_or_default() < Duration::from_secs(30 * 86400)
        {
            break;
        }
        if fs::remove_file(path).is_ok() {
            bytes = bytes.saturating_sub(size);
        }
    }
    let _ = fs::write(marker, b"");
}

#[cfg(target_os = "macos")]
mod native {
    use crate::Result;
    use std::{ffi::c_void, os::unix::ffi::OsStrExt, path::Path, ptr};
    type Ref = *const c_void;
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(value: Ref);
        fn CFURLCreateFromFileSystemRepresentation(
            allocator: Ref,
            buffer: *const u8,
            length: isize,
            directory: u8,
        ) -> Ref;
        fn CFNumberCreate(allocator: Ref, kind: isize, value: *const c_void) -> Ref;
        fn CFStringCreateWithCString(allocator: Ref, string: *const u8, encoding: u32) -> Ref;
        fn CFDictionaryCreate(
            allocator: Ref,
            keys: *const Ref,
            values: *const Ref,
            count: isize,
            key_callbacks: Ref,
            value_callbacks: Ref,
        ) -> Ref;
        static kCFBooleanTrue: Ref;
        static kCFBooleanFalse: Ref;
    }
    #[link(name = "ImageIO", kind = "framework")]
    extern "C" {
        fn CGImageSourceCreateWithURL(url: Ref, options: Ref) -> Ref;
        fn CGImageSourceCreateThumbnailAtIndex(source: Ref, index: usize, options: Ref) -> Ref;
        fn CGImageDestinationCreateWithURL(url: Ref, kind: Ref, count: usize, options: Ref) -> Ref;
        fn CGImageDestinationAddImage(destination: Ref, image: Ref, options: Ref);
        fn CGImageDestinationFinalize(destination: Ref) -> bool;
        static kCGImageSourceCreateThumbnailFromImageAlways: Ref;
        static kCGImageSourceThumbnailMaxPixelSize: Ref;
        static kCGImageSourceCreateThumbnailWithTransform: Ref;
        static kCGImageSourceShouldCache: Ref;
    }
    struct Owned(Ref);
    impl Owned {
        fn new(value: Ref) -> Result<Self> {
            if value.is_null() {
                Err("Image cannot be decoded.".into())
            } else {
                Ok(Self(value))
            }
        }
    }
    impl Drop for Owned {
        fn drop(&mut self) {
            unsafe { CFRelease(self.0) }
        }
    }
    fn url(path: &Path) -> Result<Owned> {
        let bytes = path.as_os_str().as_bytes();
        unsafe {
            Owned::new(CFURLCreateFromFileSystemRepresentation(
                ptr::null(),
                bytes.as_ptr(),
                bytes.len() as isize,
                0,
            ))
        }
    }
    pub fn render(source: &Path, destination: &Path, pixels: u32) -> Result<()> {
        // Each worker owns every CF object until finalization. Null dictionary callbacks
        // borrow the keys/values, which all outlive the dictionary and decoding calls.
        unsafe {
            let input = url(source)?;
            let output = url(destination)?;
            let size = pixels as i32;
            let number = Owned::new(CFNumberCreate(ptr::null(), 3, &size as *const i32 as Ref))?;
            let keys = [
                kCGImageSourceCreateThumbnailFromImageAlways,
                kCGImageSourceThumbnailMaxPixelSize,
                kCGImageSourceCreateThumbnailWithTransform,
                kCGImageSourceShouldCache,
            ];
            let values = [kCFBooleanTrue, number.0, kCFBooleanTrue, kCFBooleanFalse];
            let options = Owned::new(CFDictionaryCreate(
                ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                keys.len() as isize,
                ptr::null(),
                ptr::null(),
            ))?;
            let source = Owned::new(CGImageSourceCreateWithURL(input.0, options.0))?;
            let thumbnail =
                Owned::new(CGImageSourceCreateThumbnailAtIndex(source.0, 0, options.0))?;
            let kind = Owned::new(CFStringCreateWithCString(
                ptr::null(),
                c"public.png".as_ptr().cast(),
                0x08000100,
            ))?;
            let output = Owned::new(CGImageDestinationCreateWithURL(
                output.0,
                kind.0,
                1,
                ptr::null(),
            ))?;
            CGImageDestinationAddImage(output.0, thumbnail.0, ptr::null());
            if !CGImageDestinationFinalize(output.0) {
                return Err("Could not write thumbnail.".into());
            }
            Ok(())
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod native {
    pub fn render(_: &std::path::Path, _: &std::path::Path, _: u32) -> crate::Result<()> {
        Err("Native previews require macOS.".into())
    }
}
