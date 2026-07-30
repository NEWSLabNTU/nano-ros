//! Phase 212.K.7.8 — multi-thread registry race test.
//!
//! Exercises the global type registry under contention: N threads
//! simultaneously call [`nros_rmw_cyclonedds::register::<M>()`] on
//! the *same* `M`. The expected invariants are:
//!
//! 1. The C++ bridge stub fires **exactly once** —
//!    `BUILD_COUNTER == 1` — because the second waiter on the
//!    registry mutex finds the cache populated and returns the
//!    cached pointer instead of re-building.
//! 2. Every thread receives the *same* descriptor pointer (cache
//!    coherence under contention).
//! 3. No panic, no deadlock: every thread joins.
//!
//! Gating:
//!
//! * `#[cfg(feature = "std")]` — uses [`std::thread`]; the default
//!   `no_std` build skips this test.
//! * `#[cfg(feature = "bridge-stub")]` — exercises the C++ bridge
//!   via the in-crate stub (`src/bridge.rs::test_stub`) so the test
//!   never links `libddsc`. Run via:
//!
//!   ```text
//!   cargo test -p nros-rmw-cyclonedds \
//!     --no-default-features --features bridge-stub,std \
//!     --test registry_race
//!   ```

#![cfg(all(feature = "std", feature = "bridge-stub"))]

use core::sync::atomic::Ordering;
use std::{
    sync::{Arc, Barrier, Mutex},
    thread,
};

/// Serialises the three sub-tests in this file: each one assumes
/// exclusive ownership of the process-global registry + the
/// `BUILD_COUNTER` atomic, and Cargo runs tests within a single
/// test binary in parallel by default. Acquired at the top of every
/// `#[test]` fn; released on drop. Poisoning is ignored (one
/// failing test must not cascade-fail the others).
static SUBTEST_LOCK: Mutex<()> = Mutex::new(());

use nros_rmw_cyclonedds::{
    bridge::test_stub::BUILD_COUNTER, global, register, sync::RegistryMutexExt,
};
use nros_serdes::schema::{Field, FieldType, Message};

/// `Send` newtype around the descriptor pointer the registry returns.
/// The raw pointer is `!Send` by default; the registry itself
/// guarantees the value never aliases per-thread state (it points
/// into the stub's static backing buffer here, and into Cyclone's
/// ddsrt-allocated descriptor table in production). Safe to ship
/// across thread joins for assertion purposes.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
struct SendPtr(*const core::ffi::c_void);
unsafe impl Send for SendPtr {}
unsafe impl Sync for SendPtr {}

impl SendPtr {
    fn is_null(self) -> bool {
        self.0.is_null()
    }
}

// Shared fixture — the race test repeatedly registers this exact
// type from every spawned thread.
struct RaceMsg;
impl Message for RaceMsg {
    const TYPE_NAME: &'static str = "test_msgs/msg/RaceMsg\0";
    const FIELDS: &'static [Field] = &[Field {
        name: "x\0",
        ty: FieldType::Int32,
        offset: 0,
    }];
}

// Reset shared state between race sub-tests. The global registry is
// process-wide, so cargo's per-test parallelism could see leftover
// entries from `registry_smoke` (different `Message` impls — disjoint
// keys, so it doesn't break correctness here, but resetting keeps
// `BUILD_COUNTER` assertions tight). `clear_for_test` is gated behind
// `bridge-stub` per K.7.6.b — only callable from this test cfg.
fn reset_for_race() {
    global().with(|r| r.clear_for_test());
    BUILD_COUNTER.store(0, Ordering::SeqCst);
}

#[test]
fn register_same_type_from_many_threads_builds_once() {
    let _serial = SUBTEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset_for_race();

    const THREADS: usize = 16;
    let barrier = Arc::new(Barrier::new(THREADS));
    let mut handles = Vec::with_capacity(THREADS);

    for _ in 0..THREADS {
        let b = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            // Synchronise every thread's `register` call to maximise
            // contention on the registry mutex. The barrier release
            // is the closest we can get to a single-instruction
            // simultaneous entry.
            b.wait();
            SendPtr(register::<RaceMsg>().expect("registration must succeed under race"))
        }));
    }

    let mut ptrs = Vec::with_capacity(THREADS);
    for h in handles {
        ptrs.push(h.join().expect("thread must not panic"));
    }

    // Invariant 1: exactly one bridge call across the contending
    // threads — the registry mutex serialises the build, and the
    // second-and-later waiters find the cached pointer.
    assert_eq!(
        BUILD_COUNTER.load(Ordering::SeqCst),
        1,
        "C++ bridge must be invoked once per type across all racing threads"
    );

    // Invariant 2: every thread got the same pointer back.
    let first = ptrs[0];
    assert!(!first.is_null(), "registered pointer must be non-NULL");
    for (i, p) in ptrs.iter().enumerate() {
        assert_eq!(
            *p, first,
            "thread {i} saw a different descriptor pointer — cache coherence violated",
        );
    }
}

#[test]
fn register_distinct_types_concurrently_each_builds_once() {
    let _serial = SUBTEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // Variant of the race test: 8 threads register A, 8 threads
    // register B. We must see exactly two bridge calls, and each
    // type's pointer must be stable across the threads that asked
    // for it. This catches a registry-mutex implementation that
    // accidentally serialises all builds under one slot.
    reset_for_race();

    struct A;
    impl Message for A {
        const TYPE_NAME: &'static str = "test_msgs/msg/RaceA\0";
        const FIELDS: &'static [Field] = &[Field {
            name: "a\0",
            ty: FieldType::Int32,
            offset: 0,
        }];
    }
    struct B;
    impl Message for B {
        const TYPE_NAME: &'static str = "test_msgs/msg/RaceB\0";
        const FIELDS: &'static [Field] = &[Field {
            name: "b\0",
            ty: FieldType::Uint64,
            offset: 0,
        }];
    }

    const HALF: usize = 8;
    let barrier = Arc::new(Barrier::new(HALF * 2));
    let mut handles_a = Vec::with_capacity(HALF);
    let mut handles_b = Vec::with_capacity(HALF);

    for _ in 0..HALF {
        let b = Arc::clone(&barrier);
        handles_a.push(thread::spawn(move || {
            b.wait();
            SendPtr(register::<A>().expect("A must register"))
        }));
    }
    for _ in 0..HALF {
        let b = Arc::clone(&barrier);
        handles_b.push(thread::spawn(move || {
            b.wait();
            SendPtr(register::<B>().expect("B must register"))
        }));
    }

    let ptrs_a: Vec<_> = handles_a.into_iter().map(|h| h.join().unwrap()).collect();
    let ptrs_b: Vec<_> = handles_b.into_iter().map(|h| h.join().unwrap()).collect();

    assert_eq!(
        BUILD_COUNTER.load(Ordering::SeqCst),
        2,
        "two distinct types must each trigger exactly one bridge build",
    );

    let first_a = ptrs_a[0];
    let first_b = ptrs_b[0];
    assert!(!first_a.is_null());
    assert!(!first_b.is_null());
    assert_ne!(
        first_a, first_b,
        "distinct types must map to distinct descriptor pointers",
    );
    for p in &ptrs_a {
        assert_eq!(*p, first_a);
    }
    for p in &ptrs_b {
        assert_eq!(*p, first_b);
    }
}

#[test]
fn repeated_register_after_cache_clear_rebuilds_once() {
    let _serial = SUBTEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // The `clear_for_test` hook (gated behind `bridge-stub`) must
    // genuinely empty the cache: a follow-up `register` must hit
    // the bridge again exactly once, not zero times (stale ptr) and
    // not twice (re-entrant build).
    reset_for_race();

    let p1 = register::<RaceMsg>().expect("first build");
    assert_eq!(BUILD_COUNTER.load(Ordering::SeqCst), 1);

    let p2 = register::<RaceMsg>().expect("cache hit");
    assert_eq!(p1, p2);
    assert_eq!(BUILD_COUNTER.load(Ordering::SeqCst), 1, "cache hit");

    // Clear → next call must rebuild.
    global().with(|r| r.clear_for_test());
    let _p3 = register::<RaceMsg>().expect("rebuild after clear");
    assert_eq!(
        BUILD_COUNTER.load(Ordering::SeqCst),
        2,
        "clear_for_test must force a fresh build on next register",
    );
}
