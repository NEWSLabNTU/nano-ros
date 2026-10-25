//! A bringup's own RMW session configuration, baked into the image as ONE
//! const table -- RFC-0109 D8, phase-514 W3.
//!
//! The user writes their middleware's own file (`rmw/zenoh.json5`, the
//! document `rmw_zenoh` reads from `ZENOH_SESSION_CONFIG_URI`). The build
//! lowers it ON THE HOST into the backend's session properties
//! (`nros-zenoh-session-config`), and the generated entry hands the result to
//! the runtime once, before the board runner opens any session
//! ([`set_session_properties`], `nros_cpp_set_session_properties`) -- the same
//! shape as the launch remap table ([`super::launch_remap`]). Every
//! [`crate::Executor::open`] that is not given properties of its own passes
//! these to the backend as `RmwConfig::properties`.
//!
//! The table describes the image's DEFAULT session. A second session
//! ([`crate::SessionSpec`], a bridge's other domain) is not given it.

use core::ffi::{CStr, c_char};

use portable_atomic::{AtomicPtr, AtomicUsize, Ordering};

/// One session property, as the generated entry emits it: two NUL-terminated
/// strings in `.rodata`. `#[repr(C)]` so the C table
/// (`nros_cpp_session_property_t`) is this struct.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SessionProperty {
    /// The backend's property name (`listen`, `connect`, `mode`, ...).
    pub key: *const c_char,
    /// Its value.
    pub value: *const c_char,
}

// SAFETY: a `SessionProperty` only ever points at `'static`, immutable,
// NUL-terminated strings (the constructor below, and the C ABI's contract on
// `nros_cpp_set_session_properties`); nothing writes through the pointers.
unsafe impl Sync for SessionProperty {}
// SAFETY: as above.
unsafe impl Send for SessionProperty {}

impl SessionProperty {
    /// A property over `'static` C strings -- the Rust spelling of one row of
    /// the generated table.
    pub const fn new(key: &'static CStr, value: &'static CStr) -> Self {
        Self {
            key: key.as_ptr(),
            value: value.as_ptr(),
        }
    }

    /// Both fields as `str`, or `None` when one is NULL or not UTF-8 --
    /// [`set_session_properties`] refuses such a table, so a stored row reads.
    pub fn strs(&self) -> Option<(&'static str, &'static str)> {
        let s = |p: *const c_char| -> Option<&'static str> {
            if p.is_null() {
                None
            } else {
                // SAFETY: the type's invariant -- every non-NULL pointer is a
                // `'static` NUL-terminated string.
                unsafe { CStr::from_ptr(p) }.to_str().ok()
            }
        };
        Some((s(self.key)?, s(self.value)?))
    }
}

/// Why [`set_session_properties`] refused a table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BadSessionProperties {
    /// The row at this index has a NULL or non-UTF-8 string.
    Row(usize),
    /// More rows than one session open can carry ([`MAX_SESSION_PROPERTIES`]).
    TooMany(usize),
}

impl core::fmt::Display for BadSessionProperties {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Row(i) => write!(
                f,
                "session property row {i} has a NULL key or value, or a string that is not UTF-8"
            ),
            Self::TooMany(n) => write!(
                f,
                "{n} session properties; one session open carries at most {MAX_SESSION_PROPERTIES}"
            ),
        }
    }
}

/// The most rows one session open hands the backend. The zenoh shim's own
/// cap is lower on an embedded build and refuses a longer list there, naming
/// it; this one bounds the stack copy [`with_session_properties`] makes.
pub const MAX_SESSION_PROPERTIES: usize = 16;

static TABLE: AtomicPtr<SessionProperty> = AtomicPtr::new(core::ptr::null_mut());
static LEN: AtomicUsize = AtomicUsize::new(0);

/// Install the image's baked session properties. Call it ONCE, at boot,
/// before any executor opens a session -- the generated entry calls it before
/// the board runner. A row that does not read refuses the whole table and
/// installs nothing.
pub fn set_session_properties(
    table: &'static [SessionProperty],
) -> Result<(), BadSessionProperties> {
    if table.len() > MAX_SESSION_PROPERTIES {
        return Err(BadSessionProperties::TooMany(table.len()));
    }
    if let Some(i) = table.iter().position(|r| r.strs().is_none()) {
        return Err(BadSessionProperties::Row(i));
    }
    // Boot-only, single writer: hide the old table, swap the pointer, then
    // publish the length, so a reader never pairs a pointer with a longer
    // length than its table.
    LEN.store(0, Ordering::Release);
    TABLE.store(table.as_ptr() as *mut SessionProperty, Ordering::Release);
    LEN.store(table.len(), Ordering::Release);
    Ok(())
}

/// The installed table (empty until [`set_session_properties`]).
pub fn session_properties() -> &'static [SessionProperty] {
    let len = LEN.load(Ordering::Acquire);
    let ptr = TABLE.load(Ordering::Acquire);
    if len == 0 || ptr.is_null() {
        return &[];
    }
    // SAFETY: `ptr`/`len` came from one `&'static [SessionProperty]` in
    // `set_session_properties` (the length is published after the pointer).
    unsafe { core::slice::from_raw_parts(ptr, len) }
}

/// Run `f` over the properties a session open hands its backend: `explicit`
/// when the caller supplied any, else the installed table. Every road's open
/// goes through here (`Executor::open`, the C API's `open_session`), so the
/// table cannot reach one road and miss another.
pub fn with_session_properties<R>(
    explicit: &[(&str, &str)],
    f: impl FnOnce(&[(&str, &str)]) -> R,
) -> R {
    if !explicit.is_empty() {
        return f(explicit);
    }
    let mut rows: heapless::Vec<(&'static str, &'static str), MAX_SESSION_PROPERTIES> =
        heapless::Vec::new();
    for r in session_properties() {
        // `set_session_properties` checked every row reads and the count fits.
        if let Some(kv) = r.strs() {
            let _ = rows.push(kv);
        }
    }
    f(&rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::string::{String, ToString};

    #[test]
    fn a_table_installs_and_an_explicit_list_wins() {
        static T: [SessionProperty; 2] = [
            SessionProperty::new(c"mode", c"client"),
            SessionProperty::new(c"connect", c"tcp/10.0.2.2:7447"),
        ];
        set_session_properties(&T).unwrap();
        let got = with_session_properties(&[], |p| {
            p.iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect::<std::vec::Vec<_>>()
        });
        let want: [(String, String); 2] = [
            ("mode".into(), "client".into()),
            ("connect".into(), "tcp/10.0.2.2:7447".into()),
        ];
        assert_eq!(got, want);
        let explicit = [("listen", "tcp/0.0.0.0:1")];
        assert_eq!(with_session_properties(&explicit, |p| p.len()), 1);

        static BAD: [SessionProperty; 1] = [SessionProperty {
            key: c"mode".as_ptr(),
            value: core::ptr::null(),
        }];
        assert_eq!(
            set_session_properties(&BAD),
            Err(BadSessionProperties::Row(0))
        );
        // A refused table leaves the installed one in place.
        assert_eq!(session_properties().len(), 2);
    }
}
