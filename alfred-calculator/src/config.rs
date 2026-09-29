use std::{env, path::PathBuf};

#[derive(Clone)]
pub struct Config {
    pub comma: bool,
    pub grouping: bool,
    pub precision: usize,
    pub work_hours: u32,
    pub ppi: u32,
    pub timezone: String,
    pub cache: PathBuf,
    pub network: bool,
    pub favorites: Vec<String>,
}
impl Config {
    pub fn from_env() -> Self {
        let value = |key: &str| env::var(key).ok().filter(|v| !v.trim().is_empty());
        let number =
            |key: &str, default: u32| value(key).and_then(|s| s.parse().ok()).unwrap_or(default);
        let home = PathBuf::from(env::var_os("HOME").unwrap_or_default());
        let comma = match value("CALC_DECIMAL").as_deref() {
            Some("comma") => true,
            Some("dot") => false,
            _ => system_decimal_comma(),
        };
        Self {
            comma,
            grouping: value("CALC_GROUPING").as_deref() != Some("0"),
            precision: number("CALC_PRECISION", 12).min(30) as usize,
            work_hours: number("CALC_WORK_HOURS", 8).clamp(1, 24),
            ppi: number("CALC_PPI", 96).clamp(1, 9600),
            timezone: value("CALC_TIMEZONE")
                .filter(|s| s != "auto")
                .or_else(|| iana_time_zone::get_timezone().ok())
                .unwrap_or_else(|| "UTC".into()),
            cache: value("CALC_CACHE_DIR")
                .or_else(|| value("alfred_workflow_cache"))
                .map(PathBuf::from)
                .unwrap_or(home.join("Library/Caches/com.ariestwn.calculator-rust")),
            network: value("CALC_OFFLINE").as_deref() != Some("1"),
            favorites: value("CALC_FAVORITES")
                .unwrap_or_else(|| "IDR,USD,EUR,GBP,JPY,SGD,AUD,BTC,ETH".into())
                .split(',')
                .map(|s| s.trim().to_uppercase())
                .filter(|s| !s.is_empty())
                .collect(),
        }
    }
    /// Format the leading decimal token without converting it through f64.
    pub fn format(&self, input: &str) -> String {
        let (approx, input) = input
            .strip_prefix("approx. ")
            .map(|s| ("≈ ", s))
            .unwrap_or(("", input));
        let (sign, input) = input
            .strip_prefix('-')
            .map(|s| ("−", s))
            .unwrap_or(("", input));
        let n = input
            .find(|c: char| !c.is_ascii_digit() && c != '.')
            .unwrap_or(input.len());
        let (number, suffix) = input.split_at(n);
        if number.is_empty() {
            return format!("{approx}{sign}{input}");
        }
        let (integer, fraction) = number.split_once('.').unwrap_or((number, ""));
        let mut formatted = String::new();
        for (i, c) in integer.chars().enumerate() {
            if self.grouping && i > 0 && (integer.len() - i) % 3 == 0 {
                formatted.push(if self.comma { '.' } else { ',' });
            }
            formatted.push(c);
        }
        if number.contains('.') {
            formatted.push(if self.comma { ',' } else { '.' });
            formatted.push_str(fraction);
        }
        format!("{approx}{sign}{formatted}{suffix}")
    }
}

#[cfg(target_os = "macos")]
fn system_decimal_comma() -> bool {
    use std::{ffi::c_void, ptr};
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFLocaleCopyCurrent() -> *const c_void;
        fn CFNumberFormatterCreate(
            a: *const c_void,
            locale: *const c_void,
            style: isize,
        ) -> *const c_void;
        fn CFNumberFormatterCopyProperty(f: *const c_void, key: *const c_void) -> *const c_void;
        fn CFStringGetCString(s: *const c_void, buf: *mut i8, len: isize, encoding: u32) -> bool;
        fn CFRelease(value: *const c_void);
        static kCFNumberFormatterDecimalSeparator: *const c_void;
    }
    unsafe {
        let locale = CFLocaleCopyCurrent();
        if locale.is_null() {
            return false;
        }
        let formatter = CFNumberFormatterCreate(ptr::null(), locale, 1);
        CFRelease(locale);
        if formatter.is_null() {
            return false;
        }
        let sep = CFNumberFormatterCopyProperty(formatter, kCFNumberFormatterDecimalSeparator);
        CFRelease(formatter);
        if sep.is_null() {
            return false;
        }
        let mut buf = [0i8; 16];
        let ok = CFStringGetCString(sep, buf.as_mut_ptr(), 16, 0x08000100);
        CFRelease(sep);
        ok && buf[0] == b',' as i8
    }
}
#[cfg(not(target_os = "macos"))]
fn system_decimal_comma() -> bool {
    false
}
