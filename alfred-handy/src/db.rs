//! Small owning wrapper around macOS' system SQLite. No sqlite CLI or new DBs.
use crate::Result;
use serde_json::{Map, Value};
use std::{
    ffi::{CStr, CString},
    os::raw::{c_char, c_int, c_void},
    path::Path,
    ptr,
};

#[link(name = "sqlite3")]
extern "C" {
    fn sqlite3_open_v2(
        path: *const c_char,
        db: *mut *mut c_void,
        flags: c_int,
        vfs: *const c_char,
    ) -> c_int;
    fn sqlite3_close(db: *mut c_void) -> c_int;
    fn sqlite3_errmsg(db: *mut c_void) -> *const c_char;
    fn sqlite3_busy_timeout(db: *mut c_void, ms: c_int) -> c_int;
    fn sqlite3_prepare_v2(
        db: *mut c_void,
        sql: *const c_char,
        size: c_int,
        stmt: *mut *mut c_void,
        tail: *mut *const c_char,
    ) -> c_int;
    fn sqlite3_finalize(stmt: *mut c_void) -> c_int;
    fn sqlite3_bind_text(
        stmt: *mut c_void,
        index: c_int,
        text: *const c_char,
        len: c_int,
        free: Option<unsafe extern "C" fn(*mut c_void)>,
    ) -> c_int;
    fn sqlite3_step(stmt: *mut c_void) -> c_int;
    fn sqlite3_column_count(stmt: *mut c_void) -> c_int;
    fn sqlite3_column_name(stmt: *mut c_void, index: c_int) -> *const c_char;
    fn sqlite3_column_type(stmt: *mut c_void, index: c_int) -> c_int;
    fn sqlite3_column_int64(stmt: *mut c_void, index: c_int) -> i64;
    fn sqlite3_column_text(stmt: *mut c_void, index: c_int) -> *const u8;
    fn sqlite3_column_bytes(stmt: *mut c_void, index: c_int) -> c_int;
}

pub struct Db(*mut c_void);
struct Statement(*mut c_void);
impl Drop for Db {
    fn drop(&mut self) {
        unsafe {
            sqlite3_close(self.0);
        }
    }
}
impl Drop for Statement {
    fn drop(&mut self) {
        unsafe {
            sqlite3_finalize(self.0);
        }
    }
}

impl Db {
    pub fn open(path: &Path, write: bool) -> Result<Self> {
        Self::open_flags(path, if write { 2 } else { 1 })
    }
    fn open_flags(path: &Path, flags: i32) -> Result<Self> {
        use std::os::unix::ffi::OsStrExt;
        let path = CString::new(path.as_os_str().as_bytes())?;
        let mut raw = ptr::null_mut();
        let code = unsafe { sqlite3_open_v2(path.as_ptr(), &mut raw, flags, ptr::null()) };
        let db = Self(raw);
        if code != 0 {
            return Err(db.error().into());
        }
        unsafe {
            sqlite3_busy_timeout(db.0, 1500);
        }
        Ok(db)
    }
    fn error(&self) -> String {
        if self.0.is_null() {
            return "Could not allocate SQLite connection".into();
        }
        unsafe {
            CStr::from_ptr(sqlite3_errmsg(self.0))
                .to_string_lossy()
                .into_owned()
        }
    }
    pub fn query(&self, sql: &str, args: &[String]) -> Result<Vec<Value>> {
        let sql = CString::new(sql)?;
        // Bindings outlive the statement: SQLITE_STATIC is safe here.
        let bindings: Vec<CString> = args
            .iter()
            .map(|s| CString::new(s.as_bytes()))
            .collect::<std::result::Result<_, _>>()?;
        let mut raw = ptr::null_mut();
        let code =
            unsafe { sqlite3_prepare_v2(self.0, sql.as_ptr(), -1, &mut raw, ptr::null_mut()) };
        let stmt = Statement(raw);
        if code != 0 {
            return Err(self.error().into());
        }
        for (i, text) in bindings.iter().enumerate() {
            let code =
                unsafe { sqlite3_bind_text(stmt.0, (i + 1) as c_int, text.as_ptr(), -1, None) };
            if code != 0 {
                return Err(self.error().into());
            }
        }
        let mut rows = Vec::new();
        loop {
            match unsafe { sqlite3_step(stmt.0) } {
                101 => return Ok(rows),
                100 => {
                    let mut row = Map::new();
                    for i in 0..unsafe { sqlite3_column_count(stmt.0) } {
                        let name = unsafe { CStr::from_ptr(sqlite3_column_name(stmt.0, i)) }
                            .to_string_lossy()
                            .into_owned();
                        let value = match unsafe { sqlite3_column_type(stmt.0, i) } {
                            5 => Value::Null,
                            1 => unsafe { sqlite3_column_int64(stmt.0, i) }.into(),
                            _ => {
                                let data = unsafe { sqlite3_column_text(stmt.0, i) };
                                let len = unsafe { sqlite3_column_bytes(stmt.0, i) } as usize;
                                if data.is_null() {
                                    Value::String(String::new())
                                } else {
                                    String::from_utf8_lossy(unsafe {
                                        std::slice::from_raw_parts(data, len)
                                    })
                                    .into_owned()
                                    .into()
                                }
                            }
                        };
                        row.insert(name, value);
                    }
                    rows.push(Value::Object(row));
                }
                _ => return Err(self.error().into()),
            }
        }
    }
    pub fn entry(&self, id: Option<i64>) -> Result<Value> {
        let rows = if let Some(id) = id {
            self.query(
                "SELECT * FROM transcription_history WHERE id = ?",
                &[id.to_string()],
            )?
        } else {
            self.query(
                "SELECT * FROM transcription_history ORDER BY timestamp DESC, id DESC LIMIT 1",
                &[],
            )?
        };
        rows.into_iter()
            .next()
            .ok_or_else(|| "No transcript found. Record something with Handy first.".into())
    }
    pub fn history(
        &self,
        query: &str,
        saved: bool,
        page: usize,
        size: usize,
    ) -> Result<Vec<Value>> {
        let tokens: Vec<String> = query.split_whitespace().map(str::to_owned).collect();
        let mut sql = String::from("SELECT id, title, timestamp, saved, COALESCE(post_processed_text, transcription_text) AS display_text FROM transcription_history WHERE 1=1");
        if saved {
            sql.push_str(" AND saved != 0");
        }
        for _ in &tokens {
            sql.push_str(" AND instr(lower(title || ' ' || transcription_text || ' ' || COALESCE(post_processed_text,'')),lower(?)) > 0");
        }
        sql.push_str(" ORDER BY timestamp DESC, id DESC LIMIT ? OFFSET ?");
        let mut args = tokens;
        args.push((size + 1).to_string());
        args.push(page.saturating_mul(size).to_string());
        self.query(&sql, &args)
    }
    #[cfg(test)]
    pub fn fixture(path: &Path) -> Self {
        let db = Self::open_flags(path, 6).unwrap();
        db.query("CREATE TABLE transcription_history (id INTEGER PRIMARY KEY, file_name TEXT, timestamp INTEGER, saved INTEGER DEFAULT 0, title TEXT, transcription_text TEXT, post_processed_text TEXT)", &[]).unwrap();
        db
    }
}
