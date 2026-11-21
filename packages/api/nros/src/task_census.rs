//! The boot task census of a Linux process: every thread with its scheduling
//! policy and real-time priority, as the RTOS runners print theirs (FreeRTOS,
//! NuttX, ThreadX):
//!
//! ```text
//! nros: task census name=`<comm>` prio=<rt_priority> policy=<FIFO|RR|OTHER|...>
//! nros: task census done tasks=<n>
//! ```
//!
//! Read from `/proc/self/task/<tid>/{comm,stat}`, through the C library and
//! not `std`, for the reason [`mod@crate::env`] gives: the edge is "a C library is
//! linked", which on this tree is `target_os = "linux"`. ONE reader serves both
//! Linux runners, the Rust board's (`nros-board-linux`) and the C/C++ one
//! (`nros-cpp`), which do not share a board crate.
//!
//! `prio` is the real-time priority (`stat` field 40). A thread on a
//! non-real-time policy has 0 there and sits outside every band of the
//! priority plan by construction. The Linux plan states
//! `reserved.foreign = []`, and this is what a test checks it against.

use core::{
    ffi::{c_char, c_int, c_void},
    fmt,
};

/// glibc's `struct dirent64`, the same on every glibc Linux architecture.
#[repr(C)]
struct Dirent64 {
    d_ino: u64,
    d_off: i64,
    d_reclen: u16,
    d_type: u8,
    d_name: [c_char; 256],
}

unsafe extern "C" {
    fn opendir(name: *const c_char) -> *mut c_void;
    fn readdir64(dir: *mut c_void) -> *mut Dirent64;
    fn closedir(dir: *mut c_void) -> c_int;
    fn open(path: *const c_char, flags: c_int, ...) -> c_int;
    fn read(fd: c_int, buf: *mut c_void, count: usize) -> isize;
    fn close(fd: c_int) -> c_int;
}

const O_RDONLY: c_int = 0;
// `man 5 proc`, /proc/[pid]/stat: field (39) processor, (40) rt_priority,
// (41) policy. Both
// follow `comm`, which may hold spaces and parentheses, so the fields are
// counted after the LAST ')'; the first one after it is field 3.
const RT_PRIORITY_FIELD: usize = 40;
/// The most threads reported; a census of a nano-ros image is a handful.
const MAX_THREADS: usize = 64;

/// A fixed buffer that formats a NUL-terminated path.
struct PathBuf {
    buf: [u8; 64],
    len: usize,
}

impl fmt::Write for PathBuf {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let b = s.as_bytes();
        // Keep one byte for the NUL.
        if self.len + b.len() >= self.buf.len() {
            return Err(fmt::Error);
        }
        self.buf[self.len..self.len + b.len()].copy_from_slice(b);
        self.len += b.len();
        Ok(())
    }
}

fn path(args: fmt::Arguments<'_>) -> Option<PathBuf> {
    let mut p = PathBuf {
        buf: [0; 64],
        len: 0,
    };
    fmt::write(&mut p, args).ok()?;
    p.buf[p.len] = 0;
    Some(p)
}

/// Read a small file whole into `buf`; the bytes read.
fn read_file<'a>(p: &PathBuf, buf: &'a mut [u8]) -> Option<&'a [u8]> {
    // SAFETY: `p.buf` is NUL-terminated by `path`.
    let fd = unsafe { open(p.buf.as_ptr() as *const c_char, O_RDONLY) };
    if fd < 0 {
        return None;
    }
    let mut len = 0usize;
    while len < buf.len() {
        // SAFETY: `buf[len..]` is writable for its length.
        let n = unsafe { read(fd, buf[len..].as_mut_ptr() as *mut c_void, buf.len() - len) };
        if n <= 0 {
            break;
        }
        len += n as usize;
    }
    // SAFETY: `fd` was opened above.
    unsafe { close(fd) };
    Some(&buf[..len])
}

fn parse_u32(s: &[u8]) -> Option<u32> {
    core::str::from_utf8(s).ok()?.parse().ok()
}

fn policy_name(policy: u32) -> &'static str {
    match policy {
        0 => "OTHER",
        1 => "FIFO",
        2 => "RR",
        3 => "BATCH",
        5 => "IDLE",
        6 => "DEADLINE",
        _ => "?",
    }
}

/// Print the census through `println`, one call per line, ending with the
/// `done` line. A thread that exits between the listing and the read is
/// skipped; past `MAX_THREADS` (64) the rest are not reported.
pub fn report(mut println: impl FnMut(fmt::Arguments<'_>)) {
    let mut tids = [0u32; MAX_THREADS];
    let mut n = 0usize;
    // SAFETY: a NUL-terminated literal.
    let dir = unsafe { opendir(c"/proc/self/task".as_ptr()) };
    if dir.is_null() {
        println(format_args!(
            "nros: task census unavailable: /proc/self/task is not readable"
        ));
        return;
    }
    loop {
        // SAFETY: `dir` is open; the entry lives until the next call.
        let ent = unsafe { readdir64(dir) };
        if ent.is_null() {
            break;
        }
        // SAFETY: `d_name` is NUL-terminated by the C library.
        let name = unsafe { core::ffi::CStr::from_ptr((*ent).d_name.as_ptr()) };
        if let Some(tid) = parse_u32(name.to_bytes())
            && n < MAX_THREADS
        {
            tids[n] = tid;
            n += 1;
        }
    }
    // SAFETY: opened above.
    unsafe { closedir(dir) };
    tids[..n].sort_unstable();

    let mut count = 0usize;
    for &tid in &tids[..n] {
        let (Some(stat_path), Some(comm_path)) = (
            path(format_args!("/proc/self/task/{tid}/stat")),
            path(format_args!("/proc/self/task/{tid}/comm")),
        ) else {
            continue;
        };
        let mut stat_buf = [0u8; 1024];
        let mut comm_buf = [0u8; 32];
        let (Some(stat), Some(comm)) = (
            read_file(&stat_path, &mut stat_buf),
            read_file(&comm_path, &mut comm_buf),
        ) else {
            continue;
        };
        let Some(close_paren) = stat.iter().rposition(|&b| b == b')') else {
            continue;
        };
        let mut fields = stat[close_paren + 1..]
            .split(|b| b.is_ascii_whitespace())
            .filter(|f| !f.is_empty());
        // Field 3 is the first; advance to 40, and the policy (41) follows.
        let prio = fields.nth(RT_PRIORITY_FIELD - 3).and_then(parse_u32);
        let policy = fields.next().and_then(parse_u32);
        let (Some(prio), Some(policy)) = (prio, policy) else {
            continue;
        };
        let comm = core::str::from_utf8(comm).unwrap_or("?").trim_end();
        println(format_args!(
            "nros: task census name=`{comm}` prio={prio} policy={}",
            policy_name(policy)
        ));
        count += 1;
    }
    println(format_args!("nros: task census done tasks={count}"));
}
