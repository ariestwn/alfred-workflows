//! Small, typed bridges to macOS. No AppleScript, shell interpolation, or runtime dependencies.
use crate::Result;
use std::{
    collections::HashMap,
    ffi::{c_char, c_void, CStr, CString},
    fs,
    path::{Path, PathBuf},
    ptr,
};

type Obj = *mut c_void;
type Sel = *mut c_void;
type CF = *const c_void;
const UTF8: u32 = 0x0800_0100;

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFDataCreate(alloc: CF, bytes: *const u8, len: isize) -> CF;
    fn CFPropertyListCreateWithData(
        alloc: CF,
        data: CF,
        options: usize,
        format: *mut isize,
        error: *mut CF,
    ) -> CF;
    fn CFStringCreateWithBytes(
        alloc: CF,
        bytes: *const u8,
        len: isize,
        encoding: u32,
        external: bool,
    ) -> CF;
    fn CFDictionaryGetValue(dict: CF, key: CF) -> CF;
    fn CFGetTypeID(value: CF) -> usize;
    fn CFDictionaryGetTypeID() -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFStringGetLength(value: CF) -> isize;
    fn CFStringGetMaximumSizeForEncoding(length: isize, encoding: u32) -> isize;
    fn CFStringGetCString(value: CF, buffer: *mut c_char, size: isize, encoding: u32) -> bool;
    fn CFRelease(value: CF);
}

/// Reads both binary and XML Info.plist files without spawning plutil for every app.
pub fn plist_strings(path: &Path, keys: &[&str]) -> Result<HashMap<String, String>> {
    if fs::metadata(path)?.len() > 4 * 1024 * 1024 {
        return Err("App metadata is too large".into());
    }
    let bytes = fs::read(path)?;
    unsafe {
        let data = CFDataCreate(ptr::null(), bytes.as_ptr(), bytes.len() as isize);
        if data.is_null() {
            return Err("Cannot read app metadata".into());
        }
        let plist =
            CFPropertyListCreateWithData(ptr::null(), data, 0, ptr::null_mut(), ptr::null_mut());
        CFRelease(data);
        if plist.is_null() {
            return Err("Invalid app metadata".into());
        }
        if CFGetTypeID(plist) != CFDictionaryGetTypeID() {
            CFRelease(plist);
            return Err("App metadata is not a dictionary".into());
        }
        let mut values = HashMap::new();
        for key in keys {
            let cfkey =
                CFStringCreateWithBytes(ptr::null(), key.as_ptr(), key.len() as isize, UTF8, false);
            let value = CFDictionaryGetValue(plist, cfkey);
            if !value.is_null() && CFGetTypeID(value) == CFStringGetTypeID() {
                let size = CFStringGetMaximumSizeForEncoding(CFStringGetLength(value), UTF8) + 1;
                let mut buf = vec![0u8; size as usize];
                if CFStringGetCString(value, buf.as_mut_ptr().cast(), size, UTF8) {
                    values.insert(
                        (*key).to_owned(),
                        CStr::from_ptr(buf.as_ptr().cast())
                            .to_string_lossy()
                            .into_owned(),
                    );
                }
            }
            CFRelease(cfkey);
        }
        CFRelease(plist);
        Ok(values)
    }
}

#[link(name = "Foundation", kind = "framework")]
extern "C" {}
#[link(name = "objc")]
extern "C" {
    fn objc_getClass(name: *const c_char) -> Obj;
    fn sel_registerName(name: *const c_char) -> Sel;
    fn objc_msgSend();
}

unsafe fn class(name: &str) -> Obj {
    objc_getClass(CString::new(name).unwrap().as_ptr())
}
unsafe fn sel(name: &str) -> Sel {
    sel_registerName(CString::new(name).unwrap().as_ptr())
}
unsafe fn send0(object: Obj, name: &str) -> Obj {
    let f: unsafe extern "C" fn(Obj, Sel) -> Obj = std::mem::transmute(objc_msgSend as *const ());
    f(object, sel(name))
}
unsafe fn send1(object: Obj, name: &str, arg: Obj) -> Obj {
    let f: unsafe extern "C" fn(Obj, Sel, Obj) -> Obj =
        std::mem::transmute(objc_msgSend as *const ());
    f(object, sel(name), arg)
}
unsafe fn string(value: &str) -> Obj {
    let f: unsafe extern "C" fn(Obj, Sel, *const u8, usize, usize) -> Obj =
        std::mem::transmute(objc_msgSend as *const ());
    let value = f(
        send0(class("NSString"), "alloc"),
        sel("initWithBytes:length:encoding:"),
        value.as_ptr(),
        value.len(),
        4,
    );
    send0(value, "autorelease")
}
unsafe fn rust_string(value: Obj) -> String {
    let bytes = send0(value, "UTF8String");
    if bytes.is_null() {
        String::new()
    } else {
        CStr::from_ptr(bytes.cast()).to_string_lossy().into_owned()
    }
}
struct Pool(Obj);
impl Pool {
    unsafe fn new() -> Self {
        Self(send0(send0(class("NSAutoreleasePool"), "alloc"), "init"))
    }
}
impl Drop for Pool {
    fn drop(&mut self) {
        unsafe {
            send0(self.0, "drain");
        }
    }
}

/// Finder can move part of a batch before an error or cancellation.
#[derive(Debug, Default)]
pub struct TrashOutcome {
    pub destinations: Vec<PathBuf>,
    pub error: Option<String>,
}

const DIRECT_OBJECT: u32 = u32::from_be_bytes(*b"----");
const FINDER_SEND_OPTIONS: usize = 0x03 | 0x30 | 0x40; // wait, allow UI, bring authentication forward

unsafe fn integer(object: Obj, selector: &str) -> isize {
    let f: unsafe extern "C" fn(Obj, Sel) -> isize = std::mem::transmute(objc_msgSend as *const ());
    f(object, sel(selector))
}

fn finder_error(code: isize, description: &str) -> String {
    match code {
        -128 => "Cancelled in Finder. Files that already moved are listed below.".into(),
        -1743 => "Allow Alfred to control Finder in System Settings → Privacy & Security → Automation, then start a new scan.".into(),
        -1712 => "Finder did not reply in time. Check its authorization window and Trash before starting a new scan; the operation may still finish.".into(),
        _ => format!("Finder could not finish: {description} (code {code})"),
    }
}

unsafe fn error_description(error: Obj) -> String {
    if error.is_null() {
        return "macOS returned no error details".into();
    }
    let description = rust_string(send0(error, "localizedDescription"));
    let domain = rust_string(send0(error, "domain"));
    let code = integer(error, "code");
    let info = send0(error, "userInfo");
    let underlying = send1(info, "objectForKey:", string("NSUnderlyingError"));
    let detail = if underlying.is_null() {
        format!("{description} · {domain}")
    } else {
        format!(
            "{description} · {domain} · {} ({})",
            rust_string(send0(underlying, "domain")),
            integer(underlying, "code")
        )
    };
    finder_error(code, &detail)
}

unsafe fn file_url(path: &Path) -> Result<Obj> {
    let path = path
        .to_str()
        .ok_or("The path cannot be represented as UTF-8")?;
    if !Path::new(path).is_absolute() || path.contains('\0') {
        return Err("Finder requires an absolute file path".into());
    }
    let url = send1(class("NSURL"), "fileURLWithPath:", string(path));
    if url.is_null() {
        return Err("Cannot create the file URL".into());
    }
    Ok(url)
}

unsafe fn bookmark(url: Obj) -> Result<Obj> {
    let mut error: Obj = ptr::null_mut();
    let f: unsafe extern "C" fn(Obj, Sel, usize, Obj, Obj, *mut Obj) -> Obj =
        std::mem::transmute(objc_msgSend as *const ());
    let data = f(
        url,
        sel("bookmarkDataWithOptions:includingResourceValuesForKeys:relativeToURL:error:"),
        1 << 29, // NSURLBookmarkCreationWithoutImplicitSecurityScope: tracking only
        ptr::null_mut(),
        ptr::null_mut(),
        &mut error,
    );
    if data.is_null() {
        return Err(error_description(error).into());
    }
    Ok(data)
}

unsafe fn resolve_bookmark(data: Obj) -> Option<PathBuf> {
    let mut stale = 0i8;
    let mut error: Obj = ptr::null_mut();
    let f: unsafe extern "C" fn(Obj, Sel, Obj, usize, Obj, *mut i8, *mut Obj) -> Obj =
        std::mem::transmute(objc_msgSend as *const ());
    // Never prompt or mount volumes while reconciling a result.
    let url = f(
        class("NSURL"),
        sel("URLByResolvingBookmarkData:options:relativeToURL:bookmarkDataIsStale:error:"),
        data,
        (1 << 8) | (1 << 9) | (1 << 15),
        ptr::null_mut(),
        &mut stale,
        &mut error,
    );
    if url.is_null() {
        return None;
    }
    let path = rust_string(send0(url, "path"));
    (!path.is_empty()).then(|| PathBuf::from(path))
}

unsafe fn trash_event(paths: &[PathBuf]) -> Result<Obj> {
    let descriptor = class("NSAppleEventDescriptor");
    let target = send1(
        descriptor,
        "descriptorWithBundleIdentifier:",
        string("com.apple.finder"),
    );
    let event_fn: unsafe extern "C" fn(Obj, Sel, u32, u32, Obj, i16, i32) -> Obj =
        std::mem::transmute(objc_msgSend as *const ());
    // Finder.sdef: 'core'/'delo' moves items to Trash. 'fndr'/'empt' is never sent.
    let event = event_fn(
        descriptor,
        sel("appleEventWithEventClass:eventID:targetDescriptor:returnID:transactionID:"),
        u32::from_be_bytes(*b"core"),
        u32::from_be_bytes(*b"delo"),
        target,
        -1,
        0,
    );
    let list = send0(descriptor, "listDescriptor");
    if event.is_null() || list.is_null() {
        return Err("Cannot construct the Finder request".into());
    }
    let insert: unsafe extern "C" fn(Obj, Sel, Obj, isize) =
        std::mem::transmute(objc_msgSend as *const ());
    for (index, path) in paths.iter().enumerate() {
        let file = send1(descriptor, "descriptorWithFileURL:", file_url(path)?);
        if file.is_null() {
            return Err("Cannot encode a file for Finder".into());
        }
        insert(
            list,
            sel("insertDescriptor:atIndex:"),
            file,
            index as isize + 1,
        );
    }
    let set: unsafe extern "C" fn(Obj, Sel, Obj, u32) =
        std::mem::transmute(objc_msgSend as *const ());
    set(
        event,
        sel("setParamDescriptor:forKeyword:"),
        list,
        DIRECT_OBJECT,
    );
    Ok(event)
}

/// Sends one native Apple event. Finder owns all authentication; no passwords, sudo,
/// shell commands, permanent deletion, or automatic retries are involved.
pub fn trash(paths: &[PathBuf]) -> TrashOutcome {
    if paths.is_empty() {
        return TrashOutcome::default();
    }
    unsafe {
        let _pool = Pool::new();
        let prepared = (|| -> Result<_> {
            let bookmarks = paths
                .iter()
                .map(|p| bookmark(file_url(p)?))
                .collect::<Result<Vec<_>>>()?;
            Ok((trash_event(paths)?, bookmarks))
        })();
        let (event, bookmarks) = match prepared {
            Ok(prepared) => prepared,
            Err(e) => {
                return TrashOutcome {
                    error: Some(format!("Finder request was not sent: {e}")),
                    ..Default::default()
                }
            }
        };
        let mut error: Obj = ptr::null_mut();
        let send: unsafe extern "C" fn(Obj, Sel, usize, f64, *mut Obj) -> Obj =
            std::mem::transmute(objc_msgSend as *const ());
        let reply = send(
            event,
            sel("sendEventWithOptions:timeout:error:"),
            FINDER_SEND_OPTIONS,
            300.0,
            &mut error,
        );
        let mut outcome = TrashOutcome::default();
        if reply.is_null() {
            outcome.error = Some(error_description(error));
        } else {
            let param: unsafe extern "C" fn(Obj, Sel, u32) -> Obj =
                std::mem::transmute(objc_msgSend as *const ());
            let number = param(
                reply,
                sel("paramDescriptorForKeyword:"),
                u32::from_be_bytes(*b"errn"),
            );
            let value: unsafe extern "C" fn(Obj, Sel) -> i32 =
                std::mem::transmute(objc_msgSend as *const ());
            let code = value(number, sel("int32Value"));
            if code != 0 {
                let description = param(
                    reply,
                    sel("paramDescriptorForKeyword:"),
                    u32::from_be_bytes(*b"errs"),
                );
                outcome.error = Some(finder_error(
                    code as isize,
                    &rust_string(send0(description, "stringValue")),
                ));
            }
        }
        // Bookmarks follow renames, including Finder's name collision handling.
        // Resolve even on cancellation/error so partial moves aren't hidden.
        for (original, data) in paths.iter().zip(bookmarks) {
            if let Some(destination) = resolve_bookmark(data).filter(|p| p != original) {
                outcome.destinations.push(destination);
            }
        }
        outcome
    }
}

#[link(name = "proc")]
extern "C" {
    fn proc_listallpids(buffer: *mut c_void, buffersize: i32) -> i32;
    fn proc_pidpath(pid: i32, buffer: *mut c_void, buffersize: u32) -> i32;
}

/// Includes helpers with executables inside the app bundle; never sends quit/kill signals.
pub fn running_in(apps: &[PathBuf]) -> Result<bool> {
    unsafe {
        let count = proc_listallpids(ptr::null_mut(), 0);
        if count <= 0 {
            return Err("Cannot check running apps. Try again.".into());
        }
        let mut pids = vec![0i32; count as usize + 1024];
        let n = proc_listallpids(pids.as_mut_ptr().cast(), (pids.len() * 4) as i32);
        if n <= 0 || n as usize >= pids.len() {
            return Err("Cannot finish checking running apps. Try again.".into());
        }
        for pid in pids.iter().take(n as usize) {
            let mut buf = [0u8; 4096];
            if proc_pidpath(*pid, buf.as_mut_ptr().cast(), buf.len() as u32) > 0 {
                let path = PathBuf::from(
                    CStr::from_ptr(buf.as_ptr().cast())
                        .to_string_lossy()
                        .into_owned(),
                );
                if apps.iter().any(|app| path.starts_with(app)) {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finder_request_encodes_paths_as_data_and_uses_only_trash_command() {
        let temp = tempfile::tempdir().unwrap();
        let paths =
            ["Quotes \" & $(hello).app", "Café\nNew line.app"].map(|name| temp.path().join(name));
        for p in &paths {
            fs::write(p, "fixture").unwrap();
        }
        unsafe {
            let _pool = Pool::new();
            let event = trash_event(&paths).unwrap();
            let code: unsafe extern "C" fn(Obj, Sel) -> u32 =
                std::mem::transmute(objc_msgSend as *const ());
            assert_eq!(code(event, sel("eventClass")), u32::from_be_bytes(*b"core"));
            assert_eq!(code(event, sel("eventID")), u32::from_be_bytes(*b"delo"));
            let param: unsafe extern "C" fn(Obj, Sel, u32) -> Obj =
                std::mem::transmute(objc_msgSend as *const ());
            let list = param(event, sel("paramDescriptorForKeyword:"), DIRECT_OBJECT);
            assert_eq!(integer(list, "numberOfItems"), 2);
            let at: unsafe extern "C" fn(Obj, Sel, isize) -> Obj =
                std::mem::transmute(objc_msgSend as *const ());
            for (i, path) in paths.iter().enumerate() {
                let desc = at(list, sel("descriptorAtIndex:"), i as isize + 1);
                let url = send0(desc, "fileURLValue");
                // NSURL may normalize Unicode; it must still identify the exact file.
                assert_eq!(
                    crate::scan::Identity::read(&PathBuf::from(rust_string(send0(url, "path"))))
                        .unwrap(),
                    crate::scan::Identity::read(path).unwrap()
                );
            }
        }
    }

    #[test]
    fn bookmarks_follow_renames_without_sending_finder_events() {
        let temp = tempfile::tempdir().unwrap();
        let original = temp.path().join("Café \"quoted\" file");
        let destination = temp.path().join("renamed after collision");
        fs::write(&original, "fixture").unwrap();
        unsafe {
            let _pool = Pool::new();
            let data = bookmark(file_url(&original).unwrap()).unwrap();
            fs::rename(&original, &destination).unwrap();
            assert_eq!(
                resolve_bookmark(data).unwrap().canonicalize().unwrap(),
                destination.canonicalize().unwrap()
            );
        }
    }

    #[test]
    fn finder_errors_distinguish_cancellation_automation_and_timeout() {
        assert!(finder_error(-128, "cancel").contains("Cancelled"));
        assert!(finder_error(-1743, "denied").contains("Automation"));
        assert!(finder_error(-1712, "timeout").contains("may still finish"));
        assert!(finder_error(-5000, "access denied").contains("-5000"));
    }
}
