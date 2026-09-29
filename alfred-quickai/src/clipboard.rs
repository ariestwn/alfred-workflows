//! Clipboard capture is bracketed around Alfred's native Cmd+C action. No key
//! simulation or Accessibility permission is required by this Rust executable.
use crate::Result;
use serde_json::{json, Value};
use std::{
    fs,
    path::Path,
    thread,
    time::{Duration, Instant},
};

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use std::{
        ffi::{c_char, c_void, CStr, CString},
        mem,
    };

    type Id = *mut c_void;
    type Sel = *const c_void;
    #[link(name = "AppKit", kind = "framework")]
    extern "C" {}
    #[link(name = "objc")]
    extern "C" {
        fn objc_getClass(name: *const c_char) -> Id;
        fn sel_registerName(name: *const c_char) -> Sel;
        fn objc_msgSend();
    }

    // Each call site specifies the Cocoa method's exact ABI, including BOOL as i8.
    unsafe fn send<R>(object: Id, name: &str) -> R {
        let selector = CString::new(name).unwrap();
        let call: unsafe extern "C" fn(Id, Sel) -> R = mem::transmute(objc_msgSend as *const ());
        call(object, sel_registerName(selector.as_ptr()))
    }
    unsafe fn send1<A, R>(object: Id, name: &str, arg: A) -> R {
        let selector = CString::new(name).unwrap();
        let call: unsafe extern "C" fn(Id, Sel, A) -> R = mem::transmute(objc_msgSend as *const ());
        call(object, sel_registerName(selector.as_ptr()), arg)
    }
    unsafe fn send2<A, B, R>(object: Id, name: &str, a: A, b: B) -> R {
        let selector = CString::new(name).unwrap();
        let call: unsafe extern "C" fn(Id, Sel, A, B) -> R =
            mem::transmute(objc_msgSend as *const ());
        call(object, sel_registerName(selector.as_ptr()), a, b)
    }
    unsafe fn class(name: &str) -> Id {
        objc_getClass(CString::new(name).unwrap().as_ptr())
    }
    unsafe fn nsstring(value: &str) -> Result<Id> {
        let value = CString::new(value).map_err(|_| "Clipboard type contains a NUL character.")?;
        Ok(send1(
            class("NSString"),
            "stringWithUTF8String:",
            value.as_ptr(),
        ))
    }
    unsafe fn string(value: Id) -> Option<String> {
        if value.is_null() {
            return None;
        }
        let bytes: *const c_char = send(value, "UTF8String");
        if bytes.is_null() {
            return None;
        }
        CStr::from_ptr(bytes).to_str().ok().map(str::to_owned)
    }
    struct Pool(Id);
    impl Pool {
        fn new() -> Self {
            unsafe { Self(send(class("NSAutoreleasePool"), "new")) }
        }
    }
    impl Drop for Pool {
        fn drop(&mut self) {
            unsafe {
                send::<()>(self.0, "drain");
            }
        }
    }
    unsafe fn board() -> Id {
        send(class("NSPasteboard"), "generalPasteboard")
    }

    pub fn frontmost_pid() -> Option<i32> {
        let _pool = Pool::new();
        unsafe {
            let workspace: Id = send(class("NSWorkspace"), "sharedWorkspace");
            let app: Id = send(workspace, "frontmostApplication");
            if app.is_null() {
                return None;
            }
            let bundle: Id = send(app, "bundleIdentifier");
            if string(bundle).is_some_and(|s| s.starts_with("com.runningwithcrayons.Alfred")) {
                return None;
            }
            let pid: i32 = send(app, "processIdentifier");
            (pid > 0).then_some(pid)
        }
    }
    pub fn change_count() -> i64 {
        let _pool = Pool::new();
        unsafe { send::<isize>(board(), "changeCount") as i64 }
    }
    pub fn text() -> Result<String> {
        let _pool = Pool::new();
        unsafe {
            let value: Id = send1(
                board(),
                "stringForType:",
                nsstring("public.utf8-plain-text")?,
            );
            string(value).ok_or_else(|| "The copied selection does not contain plain text.".into())
        }
    }
    pub fn snapshot() -> Result<Value> {
        let _pool = Pool::new();
        unsafe {
            let pasteboard = board();
            let count: isize = send(pasteboard, "changeCount");
            let items: Id = send(pasteboard, "pasteboardItems");
            let length: usize = if items.is_null() {
                0
            } else {
                send(items, "count")
            };
            let mut saved = Vec::new();
            let mut size = 0usize;
            for i in 0..length {
                let item: Id = send1(items, "objectAtIndex:", i);
                let types: Id = send(item, "types");
                let mut representations = serde_json::Map::new();
                for j in 0..send::<usize>(types, "count") {
                    let kind: Id = send1(types, "objectAtIndex:", j);
                    let key = string(kind).ok_or("Cannot save a clipboard type.")?;
                    let data: Id = send1(item, "dataForType:", kind);
                    if data.is_null() {
                        return Err(
                            "Cannot save the existing clipboard. Copy it elsewhere, then retry."
                                .into(),
                        );
                    }
                    let len: usize = send(data, "length");
                    size = size
                        .checked_add(len)
                        .ok_or("Clipboard is too large to save.")?;
                    if size > 32 * 1024 * 1024 {
                        return Err(
                            "Clipboard is over 32 MB. Copy a smaller item, then retry.".into()
                        );
                    }
                    let bytes: *const u8 = send(data, "bytes");
                    let bytes = if len == 0 {
                        &[]
                    } else {
                        if bytes.is_null() {
                            return Err("Cannot save clipboard data.".into());
                        }
                        std::slice::from_raw_parts(bytes, len)
                    };
                    representations.insert(key, json!(bytes));
                }
                saved.push(Value::Object(representations));
            }
            if send::<isize>(pasteboard, "changeCount") != count {
                return Err("Clipboard changed while preparing capture. Try again.".into());
            }
            Ok(json!({"count":count, "items":saved}))
        }
    }
    pub fn restore(saved: &Value, expected_count: i64) -> Result<()> {
        let _pool = Pool::new();
        unsafe {
            let items: Id = send(class("NSMutableArray"), "array");
            for representations in saved["items"]
                .as_array()
                .ok_or("Invalid clipboard snapshot.")?
            {
                let item: Id = send(class("NSPasteboardItem"), "new");
                let item: Id = send(item, "autorelease");
                for (kind, bytes) in representations
                    .as_object()
                    .ok_or("Invalid clipboard item.")?
                {
                    let bytes: Vec<u8> = serde_json::from_value(bytes.clone())
                        .map_err(|_| "Invalid clipboard bytes.")?;
                    let data: Id = send2(
                        class("NSData"),
                        "dataWithBytes:length:",
                        bytes.as_ptr().cast::<c_void>(),
                        bytes.len(),
                    );
                    if send2::<_, _, i8>(item, "setData:forType:", data, nsstring(kind)?) == 0 {
                        return Err("Could not restore clipboard data.".into());
                    }
                }
                send1::<_, ()>(items, "addObject:", item);
            }
            let pasteboard = board();
            // A copy the user made in the meantime takes precedence over our snapshot.
            if send::<isize>(pasteboard, "changeCount") as i64 != expected_count {
                return Ok(());
            }
            send::<isize>(pasteboard, "clearContents");
            if send::<usize>(items, "count") > 0
                && send1::<_, i8>(pasteboard, "writeObjects:", items) == 0
            {
                return Err("Could not restore the previous clipboard.".into());
            }
            Ok(())
        }
    }
}

#[cfg(target_os = "macos")]
pub use mac::frontmost_pid;
#[cfg(not(target_os = "macos"))]
pub fn frontmost_pid() -> Option<i32> {
    None
}

pub fn target_after_alfred_closes() -> Option<i32> {
    for _ in 0..15 {
        thread::sleep(Duration::from_millis(100));
        if let Some(pid) = frontmost_pid() {
            return Some(pid);
        }
    }
    None
}

#[cfg(target_os = "macos")]
pub fn begin() -> Result<String> {
    let pid = target_after_alfred_closes()
        .ok_or("Could not return to the source app. Close Alfred and try again.")?;
    let mut saved = mac::snapshot()?;
    saved["pid"] = json!(pid);
    let directory = std::env::temp_dir().join("com.ariestwn.quickai-capture");
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    // Remove abandoned captures from canceled workflow runs after five minutes.
    for entry in fs::read_dir(&directory)
        .map_err(|e| e.to_string())?
        .flatten()
    {
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with("selection-")
            && entry
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|m| m.elapsed().ok())
                .is_some_and(|age| age > Duration::from_secs(300))
        {
            let _ = fs::remove_file(entry.path());
        }
    }
    let mut file = tempfile::Builder::new()
        .prefix("selection-")
        .tempfile_in(directory)
        .map_err(|e| e.to_string())?;
    serde_json::to_writer(&mut file, &saved).map_err(|e| e.to_string())?;
    let (_, path) = file.keep().map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

#[cfg(target_os = "macos")]
pub fn finish(path: &str) -> Result<(String, i32)> {
    let path = Path::new(path);
    if path.parent()
        != Some(
            std::env::temp_dir()
                .join("com.ariestwn.quickai-capture")
                .as_path(),
        )
        || !path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("selection-"))
    {
        return Err("Invalid selection capture. Please run the command again.".into());
    }
    let bytes =
        fs::read(path).map_err(|_| "Selection capture expired. Please run the command again.")?;
    fs::remove_file(path).map_err(|e| e.to_string())?;
    let saved: Value = serde_json::from_slice(&bytes).map_err(|_| "Invalid selection capture.")?;
    let baseline = saved["count"]
        .as_i64()
        .ok_or("Missing clipboard change count.")?;
    let pid = saved["pid"].as_i64().ok_or("Missing source application.")? as i32;
    let started = Instant::now();
    loop {
        let changed = mac::change_count();
        if changed != baseline {
            let text = mac::text();
            let source_unchanged = frontmost_pid() == Some(pid);
            mac::restore(&saved, changed)?;
            if !source_unchanged {
                return Err("The source app changed during selection capture. Try again.".into());
            }
            let text = text?;
            crate::prompt::validate_input(&text)?;
            return Ok((text, pid));
        }
        if started.elapsed() > Duration::from_millis(1500) {
            return Err("No text was copied. Select text in the source app and try again. Alfred needs Accessibility permission to send Copy.".into());
        }
        thread::sleep(Duration::from_millis(40));
    }
}

#[cfg(not(target_os = "macos"))]
pub fn begin() -> Result<String> {
    Err("Selection capture requires macOS.".into())
}
#[cfg(not(target_os = "macos"))]
pub fn finish(_: &str) -> Result<(String, i32)> {
    Err("Selection capture requires macOS.".into())
}

pub fn matches_original(original: &str, pid: i32, captured: &Result<(String, i32)>) -> bool {
    matches!(captured, Ok((text, current_pid)) if *current_pid == pid && text == original)
}
