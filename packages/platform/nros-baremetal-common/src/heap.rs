//! Issue 1640 — the bare-metal TLSF arenas REPORT a refusal.
//!
//! `zpico_alloc::FreeListHeap` is the platform arena on four ports. Issue 1370
//! made the FRAGMENTED / TOO SMALL verdict the operative guard for external
//! fragmentation, and issues 1425 / 1036 made an exhausted heap reach the boot
//! record and the fatal hook — on Zephyr only, whose `nros_platform_alloc` is C.
//! MPS2-AN385, STM32F4 and ESP32-QEMU called the arena and returned what it
//! returned: a NULL with no size, no verdict and no record, on exactly the
//! boards where the console is least likely to be wired.
//!
//! [`alloc_or_report`] / [`realloc_or_report`] are the ONE refusal path those
//! three share, in the Zephyr report's vocabulary:
//!
//! 1. the boot record's `failed_alloc_size` (`nros_boot_report_note_heap_alloc_failed`,
//!    exported by `nros-node` in every build), BEFORE anything is printed —
//!    the record is what a console-less board is read through;
//! 2. one `HEAP EXHAUSTED (<verdict>)` line at ERROR through the port's
//!    `PlatformLog`, with the request, the arena, the free bytes and the
//!    largest free block, and the knob to raise — or the warning that raising
//!    it only postpones a fragmentation;
//! 3. the port's fatal hook (`PlatformPanic`) when the image asked for it:
//!    `NROS_HEAP_EXHAUSTION_IS_FATAL`, default ON exactly when the boot report
//!    is (`NROS_BOOT_REPORT`), the rule `nros-node` applies to its arena's twin
//!    knob and Zephyr's Kconfig applies to `CONFIG_NROS_HEAP_EXHAUSTION_IS_FATAL`.

use core::{ffi::c_void, fmt};

use nros_platform_api::{PlatformLog, PlatformPanic};
use zpico_alloc::{Exhaustion, FreeListHeap, FreeShape};

unsafe extern "C" {
    /// `nros-node`'s boot-record writer — exported in both the enabled and the
    /// disabled build, so a port can call it unconditionally.
    fn nros_boot_report_note_heap_alloc_failed(size: usize);
}

/// `nros_log::Severity::Error.as_u8()`.
const SEVERITY_ERROR: u8 = 4;

/// Whether a refusal halts through the fatal hook. Resolved by this crate's
/// `build.rs` from `NROS_HEAP_EXHAUSTION_IS_FATAL` / `NROS_BOOT_REPORT`.
pub const HEAP_EXHAUSTION_IS_FATAL: bool = cfg!(nros_heap_exhaustion_fatal);

/// One refused request, as the report states it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Refusal {
    /// Bytes requested.
    pub size: usize,
    /// The arena's capacity.
    pub capacity: usize,
    /// The free memory's shape at the refusal.
    pub shape: FreeShape,
    /// Which exhaustion it is (issue 1370).
    pub verdict: Exhaustion,
    /// The knob a TOO SMALL refusal names (e.g. `NROS_HEAP_SIZE`).
    pub knob: &'static str,
}

impl Refusal {
    /// Classify a refusal of `size` bytes by `heap`. O(free blocks): a
    /// diagnostic path, never the allocation path.
    pub fn of<const N: usize>(heap: &FreeListHeap<N>, size: usize, knob: &'static str) -> Self {
        let shape = heap.free_shape();
        Self {
            size,
            capacity: N,
            shape,
            verdict: Exhaustion::classify(size, shape),
            knob,
        }
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "HEAP EXHAUSTED ({}): request {} bytes, arena {} bytes, free {} bytes, \
             largest free block {} bytes -- ",
            self.verdict.as_str(),
            self.size,
            self.capacity,
            self.shape.free_total,
            self.shape.largest_free,
        )?;
        match self.verdict {
            Exhaustion::Fragmented => write!(
                f,
                "the bytes are free and no hole the size-class search reaches holds the \
                 request; a larger {} only postpones this (issue 1370)",
                self.knob
            ),
            Exhaustion::TooSmall => write!(f, "raise {} once you know what asked", self.knob),
        }
    }
}

/// A `core::fmt::Write` over a fixed buffer that truncates rather than fails:
/// this runs on the path that just failed to allocate.
struct Line {
    buf: [u8; 256],
    len: usize,
}

impl fmt::Write for Line {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let room = self.buf.len() - self.len;
        let n = s.len().min(room);
        self.buf[self.len..self.len + n].copy_from_slice(&s.as_bytes()[..n]);
        self.len += n;
        Ok(())
    }
}

/// Render `refusal` into a stack line, no allocator. Exposed for the tests.
pub fn render(refusal: &Refusal, out: &mut [u8; 256]) -> usize {
    use fmt::Write as _;
    let mut line = Line {
        buf: [0; 256],
        len: 0,
    };
    let _ = write!(line, "{refusal}");
    out.copy_from_slice(&line.buf);
    line.len
}

/// The refusal path: record, report, and halt if the image asked for it.
#[cold]
fn report<P: PlatformLog + PlatformPanic, const N: usize>(
    heap: &FreeListHeap<N>,
    size: usize,
    knob: &'static str,
) {
    // The record FIRST — on the board class this exists for, the log line
    // reaches nobody and the record is what survives the halt.
    // SAFETY: `nros-node` exports this symbol in every build (issue 1425).
    unsafe { nros_boot_report_note_heap_alloc_failed(size) };
    let refusal = Refusal::of(heap, size, knob);
    let mut line = [0u8; 256];
    let n = render(&refusal, &mut line);
    P::write(SEVERITY_ERROR, b"nros", &line[..n]);
    if HEAP_EXHAUSTION_IS_FATAL {
        const MSG: &[u8] = b"platform heap exhausted (see the HEAP EXHAUSTED line above, and the \
                             boot report's failed_alloc_size)";
        P::panic(MSG.as_ptr(), MSG.len());
    }
}

/// `heap.alloc(size)`, with a refusal reported (issue 1640).
pub fn alloc_or_report<P: PlatformLog + PlatformPanic, const N: usize>(
    heap: &FreeListHeap<N>,
    size: usize,
    knob: &'static str,
) -> *mut c_void {
    let p = heap.alloc(size);
    if p.is_null() && size != 0 {
        report::<P, N>(heap, size, knob);
    }
    p
}

/// `heap.realloc(ptr, size)`, with a refusal reported: a grow that cannot be
/// served is the same exhaustion as an alloc, and the caller keeps `ptr`.
pub fn realloc_or_report<P: PlatformLog + PlatformPanic, const N: usize>(
    heap: &FreeListHeap<N>,
    ptr: *mut c_void,
    size: usize,
    knob: &'static str,
) -> *mut c_void {
    let p = heap.realloc(ptr, size);
    if p.is_null() && size != 0 {
        report::<P, N>(heap, size, knob);
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(r: &Refusal) -> &'static str {
        let mut out = [0u8; 256];
        let n = render(r, &mut out);
        let s = core::str::from_utf8(&out[..n]).unwrap();
        // Leak for the test's convenience: the assertion needs a `&str`.
        Box::leak(s.to_owned().into_boxed_str())
    }

    extern crate std;
    use std::{borrow::ToOwned, boxed::Box};

    #[test]
    fn a_too_small_refusal_names_the_knob_and_every_number() {
        let r = Refusal {
            size: 4096,
            capacity: 16384,
            shape: FreeShape {
                largest_free: 1024,
                free_total: 1500,
                free_blocks: 2,
            },
            verdict: Exhaustion::TooSmall,
            knob: "NROS_HEAP_SIZE",
        };
        let l = line(&r);
        for want in [
            "HEAP EXHAUSTED (TOO SMALL)",
            "request 4096 bytes",
            "arena 16384 bytes",
            "free 1500 bytes",
            "largest free block 1024 bytes",
            "raise NROS_HEAP_SIZE",
        ] {
            assert!(l.contains(want), "missing {want:?}: {l}");
        }
    }

    #[test]
    fn a_fragmented_refusal_says_a_bigger_arena_only_postpones_it() {
        let r = Refusal {
            size: 4096,
            capacity: 16384,
            shape: FreeShape {
                largest_free: 2048,
                free_total: 9000,
                free_blocks: 6,
            },
            verdict: Exhaustion::Fragmented,
            knob: "NROS_HEAP_SIZE",
        };
        let l = line(&r);
        assert!(l.starts_with("HEAP EXHAUSTED (FRAGMENTED)"), "{l}");
        assert!(l.contains("only postpones"), "{l}");
        assert!(
            l.len() < 256,
            "the line must fit the stack buffer whole: {l}"
        );
    }

    #[test]
    fn the_verdict_comes_from_the_arena_itself() {
        static HEAP: FreeListHeap<4096> = FreeListHeap::new();
        // Fill it, then ask for more than is left: TOO SMALL.
        let mut held = std::vec::Vec::new();
        loop {
            let p = HEAP.alloc(256);
            if p.is_null() {
                break;
            }
            held.push(p);
        }
        let r = Refusal::of(&HEAP, 1024, "NROS_HEAP_SIZE");
        assert_eq!(r.verdict, Exhaustion::TooSmall, "{r:?}");
        assert_eq!(r.capacity, 4096);
        // Free every other block: the bytes are there, the holes are not.
        for p in held.iter().step_by(2) {
            HEAP.free(*p);
        }
        let r = Refusal::of(&HEAP, 1024, "NROS_HEAP_SIZE");
        assert_eq!(r.verdict, Exhaustion::Fragmented, "{r:?}");
        assert!(r.shape.free_total >= 1024, "{r:?}");
    }
}
