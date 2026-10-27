//! rcl's error state — `rcl_get_error_string` and its family (phase-515 C5).
//!
//! rcl keeps a THREAD-LOCAL error state that every failing call fills with a
//! message, file and line; a caller reads it back with
//! `rcl_get_error_string().str`. The C API here returns an `rcl_ret_t` from
//! every entry point, and until C5 that code was all a C user ever got.
//!
//! ADOPT-BOUNDED. rcl's names and signatures, with two stated differences:
//!
//! * ONE record for the process, under a lock, where rcl keeps one per
//!   thread. A failure on another thread overwrites it, so the message read
//!   after a failing call is that call's only when nothing else failed in
//!   between. The pointer `rcl_get_error_state` returns names the shared
//!   record and is valid until the next set or reset by ANY thread.
//! * SMALLER buffers: `RCUTILS_ERROR_MESSAGE_MAX_LENGTH` is 256 here (rcutils:
//!   1024), so `rcl_get_error_string()` returns 256 bytes by value rather
//!   than a kilobyte onto a task stack measured in kilobytes. A longer
//!   message or file name is truncated.
//!
//! The record is filled where the C API refuses an argument or a state —
//! `validate_not_null!` and `validate_state!`, 214 sites when this landed —
//! with the argument's or object's name and the Rust source location, and by
//! a caller through `rcl_set_error_state` / `RCL_SET_ERROR_MSG`.

use core::{
    cell::UnsafeCell,
    ffi::{CStr, c_char},
};

/// rcutils' `RCUTILS_ERROR_STATE_MESSAGE_MAX_LENGTH` (768 upstream).
pub const RCUTILS_ERROR_STATE_MESSAGE_MAX_LENGTH: usize = 160;
/// rcutils' `RCUTILS_ERROR_STATE_FILE_MAX_LENGTH` (229 upstream).
pub const RCUTILS_ERROR_STATE_FILE_MAX_LENGTH: usize = 69;
/// rcutils' `RCUTILS_ERROR_STATE_LINE_NUMBER_STR_MAX_LENGTH`.
pub const RCUTILS_ERROR_STATE_LINE_NUMBER_STR_MAX_LENGTH: usize = 20;
/// rcutils' `RCUTILS_ERROR_FORMATTING_CHARACTERS` (`", at "` + `":"`).
pub const RCUTILS_ERROR_FORMATTING_CHARACTERS: usize = 6;
/// rcutils' `RCUTILS_ERROR_MESSAGE_MAX_LENGTH` (1024 upstream): the formatted
/// `"<message>, at <file>:<line>"` with its NUL, which is what the four
/// lengths above add up to, as upstream's do.
pub const RCUTILS_ERROR_MESSAGE_MAX_LENGTH: usize = RCUTILS_ERROR_STATE_MESSAGE_MAX_LENGTH
    + RCUTILS_ERROR_STATE_FILE_MAX_LENGTH
    + RCUTILS_ERROR_STATE_LINE_NUMBER_STR_MAX_LENGTH
    + RCUTILS_ERROR_FORMATTING_CHARACTERS
    + 1;

/// rcl's `rcl_error_state_t` (rcutils' `rcutils_error_state_t`): the message,
/// the file and the line of the last error set.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct rcl_error_state_t {
    /// NUL-terminated message, truncated to fit.
    pub message: [c_char; RCUTILS_ERROR_STATE_MESSAGE_MAX_LENGTH],
    /// NUL-terminated file name, truncated to fit.
    pub file: [c_char; RCUTILS_ERROR_STATE_FILE_MAX_LENGTH],
    /// Line number in `file`.
    pub line_number: u64,
}

/// rcl's `rcl_error_string_t` (rcutils' `rcutils_error_string_t`):
/// `"<message>, at <file>:<line>"`, returned by value.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct rcl_error_string_t {
    /// NUL-terminated formatted error.
    pub str: [c_char; RCUTILS_ERROR_MESSAGE_MAX_LENGTH],
}

const EMPTY_STATE: rcl_error_state_t = rcl_error_state_t {
    message: [0; RCUTILS_ERROR_STATE_MESSAGE_MAX_LENGTH],
    file: [0; RCUTILS_ERROR_STATE_FILE_MAX_LENGTH],
    line_number: 0,
};

struct Record {
    state: rcl_error_state_t,
    set: bool,
}

/// The one record. Written only under `LOCK`; `rcl_get_error_state` hands out
/// a pointer into it, which is the envelope the module doc states.
struct Shared(UnsafeCell<Record>);
// SAFETY: every write goes through `LOCK`; the one unlocked read is the
// pointer `rcl_get_error_state` returns, whose validity the C contract bounds.
unsafe impl Sync for Shared {}

static RECORD: Shared = Shared(UnsafeCell::new(Record {
    state: EMPTY_STATE,
    set: false,
}));
static LOCK: nros_rmw::sync::Mutex<()> = nros_rmw::sync::Mutex::new(());

fn copy_truncated(dst: &mut [c_char], src: &[u8]) {
    let n = src.len().min(dst.len() - 1);
    for (d, s) in dst[..n].iter_mut().zip(src) {
        *d = *s as c_char;
    }
    dst[n] = 0;
}

fn set(message: &[u8], file: &[u8], line: u64) {
    LOCK.with(|()| {
        // SAFETY: under LOCK, the only writer.
        let rec = unsafe { &mut *RECORD.0.get() };
        copy_truncated(&mut rec.state.message, message);
        copy_truncated(&mut rec.state.file, file);
        rec.state.line_number = line;
        rec.set = true;
    })
}

/// Record a refusal from inside the C API: the static message, file and line
/// of the `validate_*!` site.
pub(crate) fn set_static(message: &'static str, file: &'static str, line: u32) {
    set(message.as_bytes(), file.as_bytes(), u64::from(line));
}

fn cstr_bytes<'a>(p: *const c_char) -> &'a [u8] {
    if p.is_null() {
        return b"";
    }
    // SAFETY: the C caller passes a NUL-terminated string, as for rcutils.
    unsafe { CStr::from_ptr(p) }.to_bytes()
}

/// rcl's `rcl_set_error_state(error_string, file, line_number)`: record an
/// error, replacing any previous one. A NULL string or file records an empty
/// one.
///
/// # Safety
/// `error_string` and `file` must each be NULL or NUL-terminated.
///
/// ADOPT-BOUNDED: one process-wide record, where rcl keeps one per thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rcl_set_error_state(
    error_string: *const c_char,
    file: *const c_char,
    line_number: usize,
) {
    set(
        cstr_bytes(error_string),
        cstr_bytes(file),
        line_number as u64,
    );
}

/// rcl's `rcl_error_is_set()`: true once an error is recorded and not reset.
///
/// ADOPT-BOUNDED: one process-wide record, where rcl keeps one per thread.
#[unsafe(no_mangle)]
pub extern "C" fn rcl_error_is_set() -> bool {
    // SAFETY: a single bool read under LOCK.
    LOCK.with(|()| unsafe { (*RECORD.0.get()).set })
}

/// rcl's `rcl_get_error_state()`: the recorded error, or NULL when none is set.
///
/// ADOPT-BOUNDED: the pointer names the PROCESS-wide record and is valid until
/// the next `rcl_set_error_state` / `rcl_reset_error` on any thread (rcl's is
/// per thread).
#[unsafe(no_mangle)]
pub extern "C" fn rcl_get_error_state() -> *const rcl_error_state_t {
    if !rcl_error_is_set() {
        return core::ptr::null();
    }
    // SAFETY: a pointer into the static; reading through it is the caller's,
    // bounded as documented above.
    unsafe { core::ptr::addr_of!((*RECORD.0.get()).state) }
}

struct Writer<'a> {
    out: &'a mut [c_char],
    at: usize,
}

impl Writer<'_> {
    fn push(&mut self, bytes: &[u8]) {
        for b in bytes {
            if self.at + 1 >= self.out.len() {
                return;
            }
            self.out[self.at] = *b as c_char;
            self.at += 1;
        }
    }
}

fn until_nul(s: &[c_char]) -> impl Iterator<Item = u8> + '_ {
    s.iter().take_while(|c| **c != 0).map(|c| *c as u8)
}

/// rcl's `rcl_get_error_string()`: `"<message>, at <file>:<line>"`, or
/// `"error not set"` when none is (rcutils' text), by value.
///
/// ADOPT-BOUNDED: `RCUTILS_ERROR_MESSAGE_MAX_LENGTH` bytes, 256 here (1024
/// upstream), and the process-wide record (rcl's is per thread).
#[unsafe(no_mangle)]
pub extern "C" fn rcl_get_error_string() -> rcl_error_string_t {
    let mut out = rcl_error_string_t {
        str: [0; RCUTILS_ERROR_MESSAGE_MAX_LENGTH],
    };
    LOCK.with(|()| {
        // SAFETY: under LOCK.
        let rec = unsafe { &*RECORD.0.get() };
        let mut w = Writer {
            out: &mut out.str,
            at: 0,
        };
        if !rec.set {
            w.push(b"error not set");
            return;
        }
        for b in until_nul(&rec.state.message) {
            w.push(&[b]);
        }
        w.push(b", at ");
        for b in until_nul(&rec.state.file) {
            w.push(&[b]);
        }
        w.push(b":");
        let mut digits = [0u8; 20];
        let mut n = 0;
        let mut line = rec.state.line_number;
        loop {
            digits[n] = b'0' + (line % 10) as u8;
            n += 1;
            line /= 10;
            if line == 0 {
                break;
            }
        }
        for i in (0..n).rev() {
            w.push(&[digits[i]]);
        }
    });
    out
}

/// rcl's `rcl_reset_error()`: forget the recorded error.
///
/// ADOPT-BOUNDED: one process-wide record, where rcl keeps one per thread.
#[unsafe(no_mangle)]
pub extern "C" fn rcl_reset_error() {
    LOCK.with(|()| {
        // SAFETY: under LOCK, the only writer.
        let rec = unsafe { &mut *RECORD.0.get() };
        rec.state = EMPTY_STATE;
        rec.set = false;
    })
}
