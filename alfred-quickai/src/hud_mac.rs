//! AppKit is confined to the HUD subprocess's main thread. No event taps,
//! simulated keys, activation calls, or application-wide keyboard hooks.
use super::{interrupted, now_ms, read_state, Notice};
use crate::Result;
use std::{
    cell::Cell,
    ffi::{c_char, c_void, CString},
    fs, mem,
    path::Path,
    ptr,
    time::{Duration, Instant},
};

type Id = *mut c_void;
type Sel = *const c_void;
const PAD_X: f64 = 20.;
const PAD_Y: f64 = 15.;
const ICON: f64 = 18.;
const GAP: f64 = 10.;
const LINE: f64 = 18.;
const RADIUS: f64 = 12.;
const STACK_STEP: f64 = 94.;
const RESIZE_SECONDS: f64 = 0.22;
#[derive(Clone, Copy, Default)]
#[repr(C)]
struct Point {
    x: f64,
    y: f64,
}
#[derive(Clone, Copy, Default)]
#[repr(C)]
struct Size {
    width: f64,
    height: f64,
}
#[derive(Clone, Copy, Default)]
#[repr(C)]
struct Rect {
    origin: Point,
    size: Size,
}
#[repr(C)]
struct Insets {
    top: f64,
    left: f64,
    bottom: f64,
    right: f64,
}
fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect {
        origin: Point { x, y },
        size: Size { width, height },
    }
}

#[link(name = "AppKit", kind = "framework")]
extern "C" {
    static NSDefaultRunLoopMode: Id;
    static NSAccessibilityAnnouncementRequestedNotification: Id;
    static NSAccessibilityAnnouncementKey: Id;
    static NSAccessibilityPriorityKey: Id;
    fn NSAccessibilityPostNotificationWithUserInfo(element: Id, notification: Id, info: Id);
}
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGColorSpaceCreateDeviceRGB() -> Id;
    fn CGColorSpaceRelease(space: Id);
    fn CGBitmapContextCreate(
        data: Id,
        width: usize,
        height: usize,
        bits: usize,
        row_bytes: usize,
        space: Id,
        info: u32,
    ) -> Id;
    fn CGContextRelease(context: Id);
    fn CGContextScaleCTM(context: Id, x: f64, y: f64);
    fn CGContextClearRect(context: Id, rect: Rect);
    fn CGPathCreateWithRoundedRect(rect: Rect, radius_x: f64, radius_y: f64, transform: Id) -> Id;
    fn CGPathRelease(path: Id);
    fn CGContextAddPath(context: Id, path: Id);
    fn CGContextSetRGBFillColor(context: Id, red: f64, green: f64, blue: f64, alpha: f64);
    fn CGContextFillPath(context: Id);
    fn CGBitmapContextCreateImage(context: Id) -> Id;
    fn CGImageRelease(image: Id);
}
#[link(name = "QuartzCore", kind = "framework")]
extern "C" {}
#[link(name = "objc")]
extern "C" {
    fn objc_getClass(name: *const c_char) -> Id;
    fn sel_registerName(name: *const c_char) -> Sel;
    fn objc_msgSend();
    #[cfg(target_arch = "x86_64")]
    fn objc_msgSend_stret();
    fn objc_allocateClassPair(superclass: Id, name: *const c_char, extra: usize) -> Id;
    fn objc_registerClassPair(class: Id);
    fn class_addMethod(
        class: Id,
        selector: Sel,
        implementation: *const c_void,
        types: *const c_char,
    ) -> i8;
}

unsafe fn selector(name: &str) -> Sel {
    sel_registerName(CString::new(name).unwrap().as_ptr())
}
unsafe fn class(name: &str) -> Id {
    objc_getClass(CString::new(name).unwrap().as_ptr())
}
unsafe fn send<R>(object: Id, name: &str) -> R {
    let call: unsafe extern "C" fn(Id, Sel) -> R = mem::transmute(objc_msgSend as *const ());
    call(object, selector(name))
}
unsafe fn send1<A, R>(object: Id, name: &str, a: A) -> R {
    let call: unsafe extern "C" fn(Id, Sel, A) -> R = mem::transmute(objc_msgSend as *const ());
    call(object, selector(name), a)
}
unsafe fn send2<A, B, R>(object: Id, name: &str, a: A, b: B) -> R {
    let call: unsafe extern "C" fn(Id, Sel, A, B) -> R = mem::transmute(objc_msgSend as *const ());
    call(object, selector(name), a, b)
}
unsafe fn send4<A, B, C, D, R>(object: Id, name: &str, a: A, b: B, c: C, d: D) -> R {
    let call: unsafe extern "C" fn(Id, Sel, A, B, C, D) -> R =
        mem::transmute(objc_msgSend as *const ());
    call(object, selector(name), a, b, c, d)
}
// NSRect uses stret on Intel, but the regular dispatch ABI on Apple Silicon.
unsafe fn frame(object: Id, name: &str) -> Rect {
    #[cfg(target_arch = "x86_64")]
    {
        let mut value = mem::MaybeUninit::<Rect>::uninit();
        let call: unsafe extern "C" fn(*mut Rect, Id, Sel) =
            mem::transmute(objc_msgSend_stret as *const ());
        call(value.as_mut_ptr(), object, selector(name));
        value.assume_init()
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        send(object, name)
    }
}
unsafe fn nsstring(text: &str) -> Id {
    let text = CString::new(text.replace('\0', "")).unwrap();
    send1(class("NSString"), "stringWithUTF8String:", text.as_ptr())
}
struct Pool(Id);
impl Pool {
    unsafe fn new() -> Self {
        Self(send(class("NSAutoreleasePool"), "new"))
    }
}
impl Drop for Pool {
    fn drop(&mut self) {
        unsafe {
            send::<()>(self.0, "drain");
        }
    }
}

unsafe extern "C" fn never_key(_: Id, _: Sel) -> i8 {
    0
}
unsafe fn panel_class() -> Result<Id> {
    let name = CString::new("QuickAIStatusPanel").unwrap();
    let panel = objc_allocateClassPair(class("NSPanel"), name.as_ptr(), 0);
    if panel.is_null() {
        return Err("Cannot create the HUD panel class.".into());
    }
    #[cfg(target_arch = "aarch64")]
    let encoding = c"B@:";
    #[cfg(not(target_arch = "aarch64"))]
    let encoding = c"c@:";
    for method in ["canBecomeKeyWindow", "canBecomeMainWindow"] {
        if class_addMethod(
            panel,
            selector(method),
            never_key as *const c_void,
            encoding.as_ptr(),
        ) == 0
        {
            return Err("Cannot disable HUD keyboard focus.".into());
        }
    }
    objc_registerClassPair(panel);
    Ok(panel)
}

unsafe fn label(parent: Id, area: Rect, size: f64, weight: f64, secondary: bool) -> Id {
    let view: Id = send1(
        class("NSTextField"),
        if secondary {
            "wrappingLabelWithString:"
        } else {
            "labelWithString:"
        },
        nsstring(""),
    );
    send1::<_, ()>(view, "setFrame:", area);
    let font: Id = send2(class("NSFont"), "systemFontOfSize:weight:", size, weight);
    send1::<_, ()>(view, "setFont:", font);
    let color: Id = send(
        class("NSColor"),
        if secondary {
            "secondaryLabelColor"
        } else {
            "labelColor"
        },
    );
    send1::<_, ()>(view, "setTextColor:", color);
    send1::<_, ()>(
        view,
        "setLineBreakMode:",
        if secondary { 0isize } else { 4isize },
    );
    send1::<_, ()>(
        view,
        "setMaximumNumberOfLines:",
        if secondary { 2isize } else { 1isize },
    );
    send1::<_, ()>(view, "setSelectable:", 0i8);
    send1::<_, ()>(parent, "addSubview:", view);
    view
}

/// The blur is composited by WindowServer, outside the normal CALayer clip.
/// Mask its alpha explicitly; NSVisualEffectView also applies this to the
/// window shadow when it is the content view. Cap insets preserve the radius
/// when the content-sized popup changes width or height.
unsafe fn rounded_material_mask() -> Result<Id> {
    let side = RADIUS * 2. + 1.;
    let pixels = (side * 2.) as usize;
    let space = CGColorSpaceCreateDeviceRGB();
    if space.is_null() {
        return Err("Cannot create the HUD mask color space.".into());
    }
    let context = CGBitmapContextCreate(ptr::null_mut(), pixels, pixels, 8, pixels * 4, space, 1);
    CGColorSpaceRelease(space);
    if context.is_null() {
        return Err("Cannot create the HUD mask context.".into());
    }
    CGContextScaleCTM(context, 2., 2.);
    let area = rect(0., 0., side, side);
    CGContextClearRect(context, area);
    let path = CGPathCreateWithRoundedRect(area, RADIUS, RADIUS, ptr::null_mut());
    if path.is_null() {
        CGContextRelease(context);
        return Err("Cannot create the HUD mask shape.".into());
    }
    CGContextAddPath(context, path);
    CGContextSetRGBFillColor(context, 1., 1., 1., 1.);
    CGContextFillPath(context);
    CGPathRelease(path);
    let bitmap = CGBitmapContextCreateImage(context);
    CGContextRelease(context);
    if bitmap.is_null() {
        return Err("Cannot render the HUD mask.".into());
    }
    let image: Id = send2(
        send::<Id>(class("NSImage"), "alloc"),
        "initWithCGImage:size:",
        bitmap,
        Size {
            width: side,
            height: side,
        },
    );
    CGImageRelease(bitmap);
    if image.is_null() {
        return Err("Cannot load the HUD mask.".into());
    }
    send1::<_, ()>(
        image,
        "setCapInsets:",
        Insets {
            top: RADIUS,
            left: RADIUS,
            bottom: RADIUS,
            right: RADIUS,
        },
    );
    send1::<_, ()>(image, "setResizingMode:", 1isize); // Stretch only the 1pt center.
    Ok(send(image, "autorelease"))
}

unsafe fn screen_frame() -> Rect {
    let screens: Id = send(class("NSScreen"), "screens");
    let pointer: Point = send(class("NSEvent"), "mouseLocation");
    for i in 0..send::<usize>(screens, "count") {
        let screen: Id = send1(screens, "objectAtIndex:", i);
        let bounds = frame(screen, "frame");
        if pointer.x >= bounds.origin.x
            && pointer.y >= bounds.origin.y
            && pointer.x < bounds.origin.x + bounds.size.width
            && pointer.y < bounds.origin.y + bounds.size.height
        {
            return frame(screen, "visibleFrame");
        }
    }
    frame(send(class("NSScreen"), "mainScreen"), "visibleFrame")
}

unsafe fn announce(app: Id, notice: &Notice) {
    let info: Id = send(class("NSMutableDictionary"), "dictionary");
    send2::<_, _, ()>(
        info,
        "setObject:forKey:",
        nsstring(&format!("{}. {}", notice.title, notice.detail)),
        NSAccessibilityAnnouncementKey,
    );
    let priority: Id = send1(class("NSNumber"), "numberWithInteger:", 10isize);
    send2::<_, _, ()>(
        info,
        "setObject:forKey:",
        priority,
        NSAccessibilityPriorityKey,
    );
    NSAccessibilityPostNotificationWithUserInfo(
        app,
        NSAccessibilityAnnouncementRequestedNotification,
        info,
    );
}

unsafe fn reduce_motion() -> bool {
    // Exercise both paths in the native smoke test without changing macOS
    // preferences. This override does not exist in the installed release build.
    #[cfg(debug_assertions)]
    if let Ok(value) = std::env::var("QUICKAI_HUD_TEST_REDUCE_MOTION") {
        return value == "1";
    }
    let workspace: Id = send(class("NSWorkspace"), "sharedWorkspace");
    send::<i8>(workspace, "accessibilityDisplayShouldReduceMotion") != 0
}

struct Window {
    panel: Id,
    app: Id,
    title: Id,
    detail: Id,
    spinner: Id,
    symbol: Id,
    bounds: Rect,
    resize_until: Cell<Option<Instant>>,
}
impl Window {
    unsafe fn new() -> Result<Self> {
        let app: Id = send(class("NSApplication"), "sharedApplication");
        // Accessory: no Dock icon or menu bar. Never activate the app.
        if send1::<_, i8>(app, "setActivationPolicy:", 1isize) == 0 {
            return Err("Cannot create a background HUD.".into());
        }
        send::<()>(app, "finishLaunching");
        let bounds = screen_frame();
        let panel: Id = send4(
            send::<Id>(panel_class()?, "alloc"),
            "initWithContentRect:styleMask:backing:defer:",
            rect(0., 0., 240., PAD_Y * 2. + LINE),
            1usize << 7,
            2usize,
            0i8,
        );
        if panel.is_null() {
            return Err("Cannot create the HUD window.".into());
        }
        send1::<_, ()>(panel, "setReleasedWhenClosed:", 0i8);
        send1::<_, ()>(panel, "setTitle:", nsstring("QuickAI Status"));
        send1::<_, ()>(panel, "setHidesOnDeactivate:", 0i8);
        send1::<_, ()>(panel, "setBecomesKeyOnlyIfNeeded:", 1i8);
        send1::<_, ()>(panel, "setIgnoresMouseEvents:", 1i8);
        send1::<_, ()>(panel, "setLevel:", 3isize); // NSFloatingWindowLevel.
        send1::<_, ()>(
            panel,
            "setCollectionBehavior:",
            (1 | (1 << 3) | (1 << 6) | (1 << 8)) as usize,
        );
        send1::<_, ()>(panel, "setOpaque:", 0i8);
        let clear: Id = send(class("NSColor"), "clearColor");
        send1::<_, ()>(panel, "setBackgroundColor:", clear);
        send1::<_, ()>(panel, "setHasShadow:", 1i8);
        let background: Id = send1(
            send::<Id>(class("NSVisualEffectView"), "alloc"),
            "initWithFrame:",
            rect(0., 0., 240., PAD_Y * 2. + LINE),
        );
        send1::<_, ()>(background, "setMaterial:", 13isize); // Native HUD material.
        send1::<_, ()>(background, "setState:", 1isize); // Active even while the app is inactive.
        send1::<_, ()>(background, "setBlendingMode:", 0isize);
        send1::<_, ()>(background, "setWantsLayer:", 1i8);
        let layer: Id = send(background, "layer");
        send1::<_, ()>(layer, "setCornerRadius:", RADIUS);
        send1::<_, ()>(layer, "setMasksToBounds:", 1i8);
        send1::<_, ()>(background, "setMaskImage:", rounded_material_mask()?);
        send1::<_, ()>(panel, "setContentView:", background);
        let title = label(background, Rect::default(), 13., 0.23, false);
        let detail = label(background, Rect::default(), 12., 0., true);
        let symbol = label(background, Rect::default(), 18., 0.23, false);
        send1::<_, ()>(symbol, "setAlignment:", 1isize);
        let spinner: Id = send1(
            send::<Id>(class("NSProgressIndicator"), "alloc"),
            "initWithFrame:",
            rect(PAD_X, PAD_Y, ICON, ICON),
        );
        send1::<_, ()>(spinner, "setStyle:", 1isize);
        send1::<_, ()>(spinner, "setIndeterminate:", 1i8);
        send1::<_, ()>(spinner, "setDisplayedWhenStopped:", 0i8);
        send1::<_, ()>(background, "addSubview:", spinner);
        send::<()>(background, "release");
        send::<()>(spinner, "release");
        Ok(Self {
            panel,
            app,
            title,
            detail,
            spinner,
            symbol,
            bounds,
            resize_until: Cell::new(None),
        })
    }

    unsafe fn update(&self, notice: &Notice) {
        // Progress and completion need one concise line. Errors retain a
        // second, wrapping explanation so their actionable detail stays visible.
        let text = if notice.phase == "success" {
            &notice.detail
        } else {
            &notice.title
        };
        send1::<_, ()>(self.title, "setStringValue:", nsstring(text));
        send1::<_, ()>(self.detail, "setStringValue:", nsstring(&notice.detail));
        let expanded = notice.phase == "error" && !notice.detail.is_empty();
        send1::<_, ()>(self.detail, "setHidden:", (!expanded) as i8);
        let title_text: Id = send(self.title, "attributedStringValue");
        let title_size: Size = send(title_text, "size");
        let detail_text: Id = send(self.detail, "attributedStringValue");
        let detail_size: Size = send(detail_text, "size");
        let text_width = title_size
            .width
            .max(if expanded { detail_size.width } else { 0. })
            .ceil()
            .clamp(80., 290.);
        // NSTextField has a 2pt drawing inset at each horizontal edge.
        let field_width = text_width + 4.;
        let detail_height = if expanded {
            let cell: Id = send(self.detail, "cell");
            let size: Size = send1(cell, "cellSizeForBounds:", rect(0., 0., field_width, 100.));
            size.height.ceil().clamp(15., 30.)
        } else {
            0.
        };
        let content_height = LINE + if expanded { 3. + detail_height } else { 0. };
        let size = Size {
            width: PAD_X * 2. + ICON + GAP + text_width,
            height: PAD_Y * 2. + content_height,
        };
        let current = frame(self.panel, "frame");
        let reduce = reduce_motion();
        let animated = send::<i8>(self.panel, "isVisible") != 0 && !reduce;
        let target = rect(
            current.origin.x + (current.size.width - size.width) / 2.,
            current.origin.y,
            size.width,
            size.height,
        );
        // Native window resizing keeps the blur's rounded mask and the text at
        // their original scale. Resize and content fade share one timing curve.
        // The animator proxy retargets from the live frame if status changes
        // again before an earlier resize finishes.
        if animated {
            send::<()>(class("NSAnimationContext"), "beginGrouping");
            let context: Id = send(class("NSAnimationContext"), "currentContext");
            send1::<_, ()>(context, "setDuration:", RESIZE_SECONDS);
            let timing: Id = send4(
                class("CAMediaTimingFunction"),
                "functionWithControlPoints::::",
                0.645f32,
                0.045f32,
                0.355f32,
                1f32,
            );
            send1::<_, ()>(context, "setTimingFunction:", timing);
            let proxy: Id = send(self.panel, "animator");
            send2::<_, _, ()>(proxy, "setFrame:display:", target, 1i8);
            self.resize_until.set(Some(
                Instant::now() + Duration::from_secs_f64(RESIZE_SECONDS),
            ));
        } else {
            self.resize_until.set(None);
            send2::<_, _, ()>(self.panel, "setFrame:display:", target, 1i8);
        }
        let text_x = PAD_X + ICON + GAP - 2.;
        send1::<_, ()>(
            self.title,
            "setFrame:",
            rect(text_x, size.height - PAD_Y - LINE, field_width, LINE),
        );
        send1::<_, ()>(
            self.detail,
            "setFrame:",
            rect(text_x, PAD_Y, field_width, detail_height),
        );
        send1::<_, ()>(
            self.spinner,
            "setFrame:",
            rect(PAD_X, (size.height - ICON) / 2., ICON, ICON),
        );
        send1::<_, ()>(
            self.symbol,
            "setFrame:",
            rect(PAD_X - 2., (size.height - 22.) / 2., ICON + 4., 22.),
        );
        if animated {
            // Fade the replacement text in without scaling its glyphs while
            // the native panel expands to accommodate the new message.
            for view in [self.title, self.detail] {
                send1::<_, ()>(view, "setAlphaValue:", 0f64);
                send1::<_, ()>(send::<Id>(view, "animator"), "setAlphaValue:", 1f64);
            }
            send::<()>(class("NSAnimationContext"), "endGrouping");
        } else {
            for view in [self.title, self.detail] {
                let layer: Id = send(view, "layer");
                send::<()>(layer, "removeAllAnimations");
                send1::<_, ()>(view, "setAlphaValue:", 1f64);
            }
        }
        send::<()>(self.panel, "invalidateShadow");
        let animate = notice.phase == "working" && !reduce;
        send1::<_, ()>(self.spinner, "setHidden:", (!animate) as i8);
        send1::<_, ()>(self.symbol, "setHidden:", animate as i8);
        send1::<_, ()>(
            self.spinner,
            if animate {
                "startAnimation:"
            } else {
                "stopAnimation:"
            },
            ptr::null_mut::<c_void>(),
        );
        send1::<_, ()>(
            self.symbol,
            "setStringValue:",
            nsstring(match notice.phase {
                "success" => "✓",
                "error" => "!",
                _ => "…",
            }),
        );
        let color: Id = send(
            class("NSColor"),
            match notice.phase {
                "success" => "systemGreenColor",
                "error" => "systemOrangeColor",
                _ => "labelColor",
            },
        );
        send1::<_, ()>(self.symbol, "setTextColor:", color);
        announce(self.app, notice);
    }

    unsafe fn position(&self, slot: usize) {
        // Repositioning during a native frame animation would cancel it or
        // move its center. Resume ordinary stack placement once it settles.
        if self.resizing() {
            return;
        }
        let size = frame(self.panel, "frame").size;
        let max_slot = ((self.bounds.size.height - 120.) / STACK_STEP).max(0.) as usize;
        send1::<_, ()>(
            self.panel,
            "setFrameOrigin:",
            Point {
                x: self.bounds.origin.x + (self.bounds.size.width - size.width) / 2.,
                y: self.bounds.origin.y + 32. + STACK_STEP * slot.min(max_slot) as f64,
            },
        );
    }

    fn resizing(&self) -> bool {
        self.resize_until
            .get()
            .is_some_and(|end| Instant::now() < end)
    }

    unsafe fn pump(&self) {
        let until: Id = send1(
            class("NSDate"),
            "dateWithTimeIntervalSinceNow:",
            if self.resizing() { 1. / 120. } else { 0.05f64 },
        );
        let event: Id = send4(
            self.app,
            "nextEventMatchingMask:untilDate:inMode:dequeue:",
            usize::MAX,
            until,
            NSDefaultRunLoopMode,
            1i8,
        );
        if !event.is_null() {
            send1::<_, ()>(self.app, "sendEvent:", event);
        }
        send::<()>(self.app, "updateWindows");
    }

    #[cfg(debug_assertions)]
    unsafe fn trace(&self, phase: &str) {
        use std::io::Write;
        let Ok(path) = std::env::var("QUICKAI_HUD_TEST_TRACE") else {
            return;
        };
        let current = frame(self.panel, "frame");
        let value = serde_json::json!({"time": now_ms(), "phase": phase,
            "x": current.origin.x, "y": current.origin.y,
            "width": current.size.width, "height": current.size.height,
            "resizing": self.resizing(), "reduce_motion": reduce_motion()});
        if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "{value}");
        }
    }

    /// Debug builds can render only our own view for visual QA. This does not
    /// capture the desktop or require Screen Recording access.
    #[cfg(debug_assertions)]
    unsafe fn snapshot(&self, phase: &str) {
        let Ok(directory) = std::env::var("QUICKAI_HUD_SNAPSHOT_DIR") else {
            return;
        };
        let view: Id = send(self.panel, "contentView");
        let bounds = frame(view, "bounds");
        let bitmap: Id = send1(view, "bitmapImageRepForCachingDisplayInRect:", bounds);
        if bitmap.is_null() {
            return;
        }
        send2::<_, _, ()>(view, "cacheDisplayInRect:toBitmapImageRep:", bounds, bitmap);
        let data: Id = send2(
            bitmap,
            "representationUsingType:properties:",
            4usize,
            send::<Id>(class("NSDictionary"), "dictionary"),
        );
        let path = Path::new(&directory).join(format!("{phase}.png"));
        send2::<_, _, i8>(
            data,
            "writeToFile:atomically:",
            nsstring(&path.to_string_lossy()),
            1i8,
        );
    }
}
impl Drop for Window {
    fn drop(&mut self) {
        unsafe {
            send::<()>(self.panel, "close");
            send::<()>(self.panel, "release");
        }
    }
}

fn slot(directory: &Path) -> usize {
    // Keep simultaneous requests separate. Stable creation times preserve order
    // while their status files are atomically replaced.
    let Some(parent) = directory.parent() else {
        return 0;
    };
    let Ok(entries) = fs::read_dir(parent) else {
        return 0;
    };
    let mut sessions: Vec<_> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !super::valid_directory(&path) || !super::helper_alive(&path) {
                return None;
            }
            Some((entry.metadata().ok()?.created().ok()?, path))
        })
        .collect();
    sessions.sort();
    sessions
        .iter()
        .position(|(_, path)| path == directory)
        .unwrap_or(0)
}

pub fn show(directory: &Path) -> Result<()> {
    unsafe {
        let _pool = Pool::new();
        let window = Window::new()?;
        let mut notice = Notice::from_value(&read_state(directory)?)?;
        window.update(&notice);
        window.position(0);
        send::<()>(window.panel, "orderFrontRegardless");
        window.pump();
        #[cfg(debug_assertions)]
        window.snapshot(notice.phase);
        #[cfg(debug_assertions)]
        let mut snapshot_pending = false;
        // Refuse to run a HUD that could receive user input. This is also a
        // startup handshake: only a visible, inert window reports itself ready.
        if send::<i8>(window.panel, "canBecomeKeyWindow") != 0
            || send::<i8>(window.panel, "canBecomeMainWindow") != 0
            || send::<i8>(window.panel, "isKeyWindow") != 0
            || send::<i8>(window.panel, "ignoresMouseEvents") == 0
            || send::<i8>(window.panel, "isVisible") == 0
        {
            return Err("HUD could not be shown without keyboard focus.".into());
        }
        fs::write(directory.join("ready"), std::process::id().to_string())
            .map_err(|e| e.to_string())?;
        let mut final_since = if notice.phase == "working" {
            None
        } else {
            Some(Instant::now())
        };
        let mut position_at = Instant::now() - Duration::from_secs(1);
        loop {
            let _iteration = Pool::new();
            let Ok(mut state) = read_state(directory) else {
                break;
            };
            // The owner can exit just after writing its handoff. Re-read before
            // interpreting an old snapshot as an interrupted request.
            if interrupted(&state, now_ms()) {
                state = read_state(directory)?;
            }
            let next = if interrupted(&state, now_ms()) {
                Notice::stopped()
            } else {
                Notice::from_value(&state)?
            };
            if next != notice {
                notice = next;
                window.update(&notice);
                window.position(slot(directory));
                #[cfg(debug_assertions)]
                {
                    snapshot_pending = true;
                }
                final_since = if notice.phase == "working" {
                    None
                } else {
                    Some(Instant::now())
                };
            }
            let seconds = if notice.phase == "error" { 7 } else { 3 };
            if final_since.is_some_and(|t| t.elapsed() >= Duration::from_secs(seconds)) {
                break;
            }
            if position_at.elapsed() >= Duration::from_millis(500) {
                window.position(slot(directory));
                position_at = Instant::now();
            }
            window.pump();
            #[cfg(debug_assertions)]
            {
                window.trace(notice.phase);
                if snapshot_pending && !window.resizing() {
                    window.snapshot(notice.phase);
                    snapshot_pending = false;
                }
            }
        }
        Ok(())
    }
}
