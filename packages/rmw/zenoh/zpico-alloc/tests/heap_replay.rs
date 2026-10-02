//! Issue 1370 — what a recorded image's traffic needs from the arena, and why
//! that number is not a bound.
//!
//! The engine is the `heap_replay` example's, included by path so the tool an
//! operator runs and the guard below are one copy.
//!
//! Needs the arena's counters, so it runs only with `--features stats`; the
//! `just check node-std-tests` lane names that feature, because `--workspace`
//! unifies nothing that turns it on (no workspace member depends on it).
#![cfg(feature = "stats")]

#[path = "../examples/heap_replay/engine.rs"]
mod engine;

use engine::*;
use zpico_alloc::Exhaustion;

const ZEPHYR_C_TALKER: &str = include_str!("data/zephyr-native-sim-c-talker.trace");

/// The image's own `nros_zephyr_heap_peak()` at the end of the recorded run
/// (the header of the trace file).
const ZEPHYR_C_TALKER_IMAGE_PEAK: usize = 15_504;

/// The shipped `CONFIG_NROS_ZEPHYR_HEAP_SIZE` default.
const ZEPHYR_DEFAULT_ARENA: usize = 65_536;

#[test]
fn the_parser_pairs_every_free_with_its_allocation() {
    let t = parse(
        "# watched: x\n\
         A 100 0x1000\n\
         A 200 0x2000\n\
         F 0x1000\n\
         A 300 0x1000\n\
         R 0x2000 400 0x3000\n\
         F 0x9999\n\
         A 50 0x0\n\
         F 0x3000\n",
    )
    .unwrap();
    assert_eq!(
        t.events,
        vec![
            Event::Alloc { size: 100, id: 0 },
            Event::Alloc { size: 200, id: 1 },
            Event::Free { id: 0 },
            Event::Alloc { size: 300, id: 2 },
            Event::Realloc {
                old: 1,
                size: 400,
                id: 3
            },
            Event::Free { id: 3 },
        ]
    );
    assert_eq!(t.unmatched_frees, 1, "the untraced 0x9999");
    assert_eq!(t.refused_in_image, 1, "the NULL return");
    let s = shape(&t);
    assert_eq!((s.min_request, s.max_request), (100, 400));
    assert_eq!(s.peak_requested, 700, "300 + 400 live together");
    assert!(
        parse("Q 1 2\n").is_err(),
        "an unknown event is an error, not a skip"
    );
}

/// The replay IS the image's allocator: replaying the recorded traffic into
/// the shipped arena reproduces, to the byte, the peak the running image
/// reported about itself. If this drifts, either the allocator changed or the
/// recorder lost events, and every number below stops meaning anything.
#[test]
fn a_replay_reproduces_the_images_own_peak() {
    let t = parse(ZEPHYR_C_TALKER).unwrap();
    assert_eq!(t.unmatched_frees, 0);
    assert_eq!(t.refused_in_image, 0);
    let o = replay::<ZEPHYR_DEFAULT_ARENA>(&t);
    assert_eq!(
        o.refusal, None,
        "the shipped default serves its own traffic"
    );
    assert_eq!(o.peak, ZEPHYR_C_TALKER_IMAGE_PEAK);
}

/// The measured external fragmentation of REAL traffic is small: the smallest
/// arena from which every larger candidate serves the whole run is within 10 %
/// of the live peak (measured 16,896 against 15,504 = 1.09x), and just below
/// it the refusal is the arena running out, not fragmentation.
#[test]
fn real_traffic_needs_little_beyond_its_peak() {
    let t = parse(ZEPHYR_C_TALKER).unwrap();
    let safe = smallest_safe(|c| (c.1)(&t)).expect("some candidate serves it");
    assert!(
        safe.arena * 100 <= safe.peak * 110,
        "smallest safe arena {} vs peak {}",
        safe.arena,
        safe.peak
    );
    let below = largest_refusing(|c| (c.1)(&t)).expect("a 1 KiB arena cannot");
    assert_eq!(
        below.refusal.unwrap().verdict,
        Exhaustion::TooSmall,
        "{below:?}"
    );
}

/// …and that is a fact about THIS traffic, not a bound. The same size range at
/// the same live peak, under a Robson-style stress pattern, is refused by an
/// arena four times the peak — refused as FRAGMENTED, with the bytes free.
/// No "peak plus a margin" sizing rule survives traffic shaped like this, which
/// is why the `FRAGMENTED` verdict on the exhaustion path is the operative
/// guard rather than an arena size derived from a bound (issue 1370).
#[test]
fn the_same_range_under_stress_defeats_any_peak_margin() {
    let t = parse(ZEPHYR_C_TALKER).unwrap();
    let real = replay::<ZEPHYR_DEFAULT_ARENA>(&t);
    let (lo, hi) = real.spread;
    assert!(lo > 0 && hi > lo, "{real:?}");
    let budget = real.peak;
    // 4 x 15,504 = 62,016 < the 65,536 the image ships with.
    let o = adversary::<{ 4 * ZEPHYR_C_TALKER_IMAGE_PEAK }>(lo, hi, budget);
    let r = o
        .refusal
        .expect("the stress pattern must defeat a 4x arena");
    assert_eq!(r.verdict, Exhaustion::Fragmented, "{r:?}");
    assert!(r.shape.free_total >= r.size, "the bytes were free: {r:?}");
    // The sticky peak may include the ONE probe block that crossed the budget
    // and was handed straight back; nothing beyond that was ever live.
    assert!(
        o.peak <= budget + Exhaustion::reachable_payload(hi),
        "and live never exceeded the budget: {o:?}"
    );
}
