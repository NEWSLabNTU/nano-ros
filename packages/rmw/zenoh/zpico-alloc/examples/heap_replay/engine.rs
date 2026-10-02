//! Issue 1370 — replay a recorded allocation trace into the REAL arena.
//!
//! A trace is what `scripts/heap-trace/gdb_heap_trace.py` writes off a running
//! image (one `A`/`R`/`F` event per line). Replaying it into a
//! [`FreeListHeap`] of a chosen size answers the question the arena's own
//! counters cannot: not "how many bytes were live at once" (`peak`) but "how
//! small an arena still serves THIS sequence" — the difference between the two
//! is the external fragmentation the traffic actually caused.
//!
//! Shared by the `heap_replay` example (the operator's tool) and
//! `tests/heap_replay.rs` (the guard) through `#[path]`, so there is one copy
//! of the replay, not a mirror of it.
//!
//! The arena is the real `FreeListHeap` — slab fast-path, rlsf parameters and
//! all — so a replay measures the shipped allocator, not a model of it. Its
//! size is a const generic, which is why the candidate sizes are a fixed
//! table ([`ARENAS`]) rather than a runtime binary search.
//!
//! Host-word caveat: rlsf's block header and granularity scale with the
//! pointer width (16 B granularity on a 32-bit target, 32 B on 64-bit), so a
//! host replay of a Cortex-M trace slightly OVER-states that target's arena.
//! The native_sim images are 64-bit, so their replays are exact in this
//! respect.

#![allow(dead_code)]

use std::collections::HashMap;

use zpico_alloc::{Exhaustion, FreeListHeap, FreeShape};

/// One funnel call, with the pointer replaced by a stable allocation id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Alloc { size: usize, id: usize },
    Realloc { old: usize, size: usize, id: usize },
    Free { id: usize },
}

/// A parsed trace.
#[derive(Debug, Default)]
pub struct Trace {
    pub events: Vec<Event>,
    /// Number of allocation ids handed out.
    pub ids: usize,
    /// `A`/`R` lines whose recorded return was NULL: the IMAGE refused them.
    /// They are not replayed — the image did not get the block either.
    pub refused_in_image: usize,
    /// `F` lines naming a pointer no traced `A` returned (the trace started
    /// after the block was handed out). Skipped; the replay cannot free a block
    /// it never allocated, so a non-zero count means the replay's live set is
    /// LARGER than the image's was, i.e. the measurement is conservative.
    pub unmatched_frees: usize,
}

/// Parse the text `gdb_heap_trace.py` writes. `#` lines are comments.
pub fn parse(text: &str) -> Result<Trace, String> {
    let mut t = Trace::default();
    let mut live: HashMap<u64, usize> = HashMap::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let f: Vec<&str> = line.split_whitespace().collect();
        let num = |s: &str| -> Result<u64, String> {
            let r = match s.strip_prefix("0x") {
                Some(h) => u64::from_str_radix(h, 16),
                None => s.parse(),
            };
            r.map_err(|e| format!("line {}: `{s}`: {e}", n + 1))
        };
        match (f.first().copied(), f.len()) {
            (Some("A"), 3) => {
                let size = num(f[1])? as usize;
                let ptr = num(f[2])?;
                if ptr == 0 {
                    t.refused_in_image += 1;
                    continue;
                }
                let id = t.ids;
                t.ids += 1;
                live.insert(ptr, id);
                t.events.push(Event::Alloc { size, id });
            }
            (Some("R"), 4) => {
                let old_ptr = num(f[1])?;
                let size = num(f[2])? as usize;
                let ptr = num(f[3])?;
                if ptr == 0 {
                    t.refused_in_image += 1;
                    continue;
                }
                let id = t.ids;
                t.ids += 1;
                match live.remove(&old_ptr) {
                    Some(old) => t.events.push(Event::Realloc { old, size, id }),
                    // realloc(NULL, n) or an untraced block: an allocation.
                    None => t.events.push(Event::Alloc { size, id }),
                }
                live.insert(ptr, id);
            }
            (Some("F"), 2) => {
                let ptr = num(f[1])?;
                if ptr == 0 {
                    continue;
                }
                match live.remove(&ptr) {
                    Some(id) => t.events.push(Event::Free { id }),
                    None => t.unmatched_frees += 1,
                }
            }
            _ => return Err(format!("line {}: not a trace event: `{line}`", n + 1)),
        }
    }
    Ok(t)
}

/// The facts a trace states about its own traffic, independent of any arena.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TraceShape {
    pub requests: usize,
    pub min_request: usize,
    pub max_request: usize,
    /// Peak of the REQUESTED bytes live at once (no header, no rounding).
    pub peak_requested: usize,
}

pub fn shape(t: &Trace) -> TraceShape {
    let mut s = TraceShape {
        min_request: usize::MAX,
        ..Default::default()
    };
    let mut sizes = vec![0usize; t.ids];
    let mut live = 0usize;
    for e in &t.events {
        let (size, id, old) = match *e {
            Event::Alloc { size, id } => (size, id, None),
            Event::Realloc { old, size, id } => (size, id, Some(old)),
            Event::Free { id } => {
                live -= sizes[id];
                continue;
            }
        };
        if let Some(old) = old {
            live -= sizes[old];
        }
        sizes[id] = size;
        live += size;
        s.requests += 1;
        s.min_request = s.min_request.min(size);
        s.max_request = s.max_request.max(size);
        s.peak_requested = s.peak_requested.max(live);
    }
    if s.requests == 0 {
        s.min_request = 0;
    }
    s
}

/// A request the replay arena refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refusal {
    /// Index into `Trace::events`.
    pub at: usize,
    pub size: usize,
    pub verdict: Exhaustion,
    pub shape: FreeShape,
}

/// What one replay into one arena size measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    pub arena: usize,
    pub refusal: Option<Refusal>,
    /// The arena's own sticky peak (usable bytes charged, slab slots at 64).
    pub peak: usize,
    /// The arena's own `request_spread()`: the TLSF path's min/max request.
    pub spread: (usize, usize),
}

/// Replay `t` into a fresh `FreeListHeap<N>`, stopping at the first refusal.
pub fn replay<const N: usize>(t: &Trace) -> Outcome {
    let heap: Box<FreeListHeap<N>> = Box::new(FreeListHeap::new());
    let mut ptrs: Vec<*mut core::ffi::c_void> = vec![core::ptr::null_mut(); t.ids];
    let mut refusal = None;
    for (at, e) in t.events.iter().enumerate() {
        let (size, id, p) = match *e {
            Event::Alloc { size, id } => (size, id, heap.alloc(size)),
            Event::Realloc { old, size, id } => (size, id, heap.realloc(ptrs[old], size)),
            Event::Free { id } => {
                heap.free(ptrs[id]);
                ptrs[id] = core::ptr::null_mut();
                continue;
            }
        };
        if p.is_null() {
            let shape = heap.free_shape();
            refusal = Some(Refusal {
                at,
                size,
                verdict: Exhaustion::classify(size, shape),
                shape,
            });
            break;
        }
        ptrs[id] = p;
    }
    Outcome {
        arena: N,
        refusal,
        peak: heap.peak(),
        spread: heap.request_spread(),
    }
}

/// Robson-style adversary, ADAPTIVE (it reads the arena's addresses).
///
/// Stage k allocates blocks of `m * 2^k` (the last stage exactly `max`) until
/// the arena's live USABLE bytes (`used()`, the unit `peak()` reports) would
/// exceed `budget`, then frees every block but
/// the first in each address window of the NEXT stage's size — so the bytes
/// handed back sit in holes the next, larger request cannot use. Live bytes
/// never exceed `budget`; what grows is the arena needed to keep serving.
///
/// This is A stress pattern, not the optimal adversary: the arena it needs is a
/// LOWER bound on TLSF's worst case for `[min, max]` at `budget` live, which is
/// the direction a sizing argument needs (it proves "peak + small margin" is
/// NOT safe for arbitrary traffic in that range).
pub fn adversary<const N: usize>(min: usize, max: usize, budget: usize) -> Outcome {
    let heap: Box<FreeListHeap<N>> = Box::new(FreeListHeap::new());
    let mut stages = Vec::new();
    let mut s = min.max(1);
    while s < max {
        stages.push(s);
        s *= 2;
    }
    stages.push(max);
    let mut live: Vec<usize> = Vec::new(); // addresses
    let mut at = 0usize;
    for (k, &s) in stages.iter().enumerate() {
        loop {
            let p = heap.alloc(s);
            at += 1;
            if p.is_null() {
                let shape = heap.free_shape();
                let refusal = Refusal {
                    at,
                    size: s,
                    verdict: Exhaustion::classify(s, shape),
                    shape,
                };
                return Outcome {
                    arena: N,
                    refusal: Some(refusal),
                    peak: heap.peak(),
                    spread: heap.request_spread(),
                };
            }
            // The budget is in the arena's own unit — USABLE bytes charged,
            // what `peak()` reports and what a replay is compared against — so
            // the granule a 3-byte request really costs is counted as live
            // rather than read as fragmentation.
            if heap.used() > budget {
                heap.free(p);
                break;
            }
            live.push(p as usize);
        }
        let Some(&next) = stages.get(k + 1) else {
            break;
        };
        live.sort_unstable();
        let mut kept = Vec::with_capacity(live.len());
        let mut last_window = usize::MAX;
        for addr in live.drain(..) {
            let window = addr / next;
            if window != last_window {
                last_window = window;
                kept.push(addr);
            } else {
                heap.free(addr as *mut core::ffi::c_void);
            }
        }
        live = kept;
    }
    Outcome {
        arena: N,
        refusal: None,
        peak: heap.peak(),
        spread: heap.request_spread(),
    }
}

/// One candidate arena: its size, and the replay and adversary monomorphised
/// at that size.
pub type Candidate = (
    usize,
    fn(&Trace) -> Outcome,
    fn(usize, usize, usize) -> Outcome,
);

macro_rules! arenas {
    ($($n:literal),* $(,)?) => {
        /// Candidate arena sizes, ascending: 512 B steps to 32 KiB, 2 KiB to
        /// 128 KiB, 8 KiB to 256 KiB.
        pub const ARENAS: &[Candidate] = &[$(($n, replay::<$n>, adversary::<$n>)),*];
    };
}

arenas!(
    1024, 1536, 2048, 2560, 3072, 3584, 4096, 4608, 5120, 5632, 6144, 6656, 7168, 7680, 8192, 8704,
    9216, 9728, 10240, 10752, 11264, 11776, 12288, 12800, 13312, 13824, 14336, 14848, 15360, 15872,
    16384, 16896, 17408, 17920, 18432, 18944, 19456, 19968, 20480, 20992, 21504, 22016, 22528,
    23040, 23552, 24064, 24576, 25088, 25600, 26112, 26624, 27136, 27648, 28160, 28672, 29184,
    29696, 30208, 30720, 31232, 31744, 32256, 32768, 34816, 36864, 38912, 40960, 43008, 45056,
    47104, 49152, 51200, 53248, 55296, 57344, 59392, 61440, 63488, 65536, 67584, 69632, 71680,
    73728, 75776, 77824, 79872, 81920, 83968, 86016, 88064, 90112, 92160, 94208, 96256, 98304,
    100352, 102400, 104448, 106496, 108544, 110592, 112640, 114688, 116736, 118784, 120832, 122880,
    124928, 126976, 129024, 131072, 139264, 147456, 155648, 163840, 172032, 180224, 188416, 196608,
    204800, 212992, 221184, 229376, 237568, 245760, 253952, 262144,
);

/// The smallest candidate arena from which EVERY larger candidate also
/// succeeds — the size a sizing statement can be made at, since a smaller
/// arena that happens to succeed between two that fail is luck, not a margin.
/// `None` when even the largest candidate refuses.
pub fn smallest_safe<F: Fn(&Candidate) -> Outcome>(run: F) -> Option<Outcome> {
    let mut safe = None;
    for c in ARENAS.iter().rev() {
        let o = run(c);
        if o.refusal.is_some() {
            break;
        }
        safe = Some(o);
    }
    safe
}

/// The largest candidate that REFUSES, with its refusal — the evidence for
/// which exhaustion stands just below the safe size.
pub fn largest_refusing<F: Fn(&Candidate) -> Outcome>(run: F) -> Option<Outcome> {
    ARENAS.iter().rev().map(run).find(|o| o.refusal.is_some())
}
