//! Minimal macOS Accessibility bindings: read the focused selection without modifying the clipboard.
#[cfg(target_os = "macos")]
mod mac {
    use std::{
        ffi::{c_void, CStr},
        ptr,
    };

    type Ref = *const c_void;
    const UTF8: u32 = 0x08000100;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
        fn AXUIElementCreateSystemWide() -> Ref;
        fn AXUIElementCopyAttributeValue(element: Ref, attribute: Ref, value: *mut Ref) -> i32;
        fn AXUIElementGetPid(element: Ref, pid: *mut i32) -> i32;
        fn AXUIElementSetMessagingTimeout(element: Ref, timeout: f32) -> i32;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(value: Ref);
        fn CFGetTypeID(value: Ref) -> usize;
        fn CFStringGetTypeID() -> usize;
        fn CFStringCreateWithBytes(
            allocator: Ref,
            bytes: *const u8,
            length: isize,
            encoding: u32,
            external: bool,
        ) -> Ref;
        fn CFStringGetLength(value: Ref) -> isize;
        fn CFStringGetMaximumSizeForEncoding(length: isize, encoding: u32) -> isize;
        fn CFStringGetCString(value: Ref, buffer: *mut i8, size: isize, encoding: u32) -> bool;
    }
    extern "C" {
        fn proc_name(pid: i32, buffer: *mut c_void, size: u32) -> i32;
    }

    struct Owned(Ref);
    impl Drop for Owned {
        fn drop(&mut self) {
            unsafe {
                CFRelease(self.0);
            }
        }
    }

    fn attribute(element: Ref, name: &str) -> Option<Owned> {
        // SAFETY: valid, retained CF/AX objects; attribute names are UTF-8 byte slices.
        unsafe {
            let key = CFStringCreateWithBytes(
                ptr::null(),
                name.as_ptr(),
                name.len() as isize,
                UTF8,
                false,
            );
            if key.is_null() {
                return None;
            }
            let key = Owned(key);
            let mut value = ptr::null();
            if AXUIElementCopyAttributeValue(element, key.0, &mut value) != 0 || value.is_null() {
                return None;
            }
            Some(Owned(value))
        }
    }

    fn string(value: Ref) -> Option<String> {
        unsafe {
            if CFGetTypeID(value) != CFStringGetTypeID() {
                return None;
            }
            let capacity = CFStringGetMaximumSizeForEncoding(CFStringGetLength(value), UTF8) + 1;
            if !(1..=1_000_000).contains(&capacity) {
                return None;
            }
            let mut buffer = vec![0u8; capacity as usize];
            if !CFStringGetCString(value, buffer.as_mut_ptr().cast(), capacity, UTF8) {
                return None;
            }
            CStr::from_ptr(buffer.as_ptr().cast())
                .to_str()
                .ok()
                .map(str::to_owned)
        }
    }

    pub fn snapshot() -> Option<super::Snapshot> {
        unsafe {
            if !AXIsProcessTrusted() {
                return None;
            }
            let system = AXUIElementCreateSystemWide();
            if system.is_null() {
                return None;
            }
            let system = Owned(system);
            AXUIElementSetMessagingTimeout(system.0, 0.3);
            let app = attribute(system.0, "AXFocusedApplication")?;
            let mut pid = 0;
            if AXUIElementGetPid(app.0, &mut pid) != 0 || pid <= 0 {
                return None;
            }
            let mut name = [0u8; 1024];
            let length = proc_name(pid, name.as_mut_ptr().cast(), name.len() as u32);
            if length > 0 && String::from_utf8_lossy(&name[..length as usize]).starts_with("Alfred")
            {
                return None;
            }
            let text = attribute(app.0, "AXFocusedUIElement")
                .and_then(|element| attribute(element.0, "AXSelectedText"))
                .and_then(|value| string(value.0));
            Some(super::Snapshot { pid, text })
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub pid: i32,
    pub text: Option<String>,
}

pub fn snapshot() -> Option<Snapshot> {
    #[cfg(target_os = "macos")]
    {
        mac::snapshot()
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

pub fn after_alfred_closes() -> Option<Snapshot> {
    for _ in 0..4 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if let Some(snapshot) = snapshot() {
            return Some(snapshot);
        }
    }
    None
}

#[cfg(test)]
pub fn paste_target_unchanged(before: &Option<Snapshot>, after: &Option<Snapshot>) -> bool {
    matches!((before, after), (Some(a), Some(b)) if a.pid == b.pid && a.text.is_some() && a.text == b.text)
}
