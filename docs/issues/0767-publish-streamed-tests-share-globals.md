---
id: 767
title: "`publish_streamed`'s two tests share process globals and run in parallel,
  so a bare `cargo test` fails ~1 in 5 — invisible to nextest, which gives each
  test its own process"
status: open
type: bug
area: testing
related: [issue-0673]
---

## Problem

`packages/rmw/cffi/tests/publish_streamed.rs` has two tests — the native-slot
path and the fallback path — and they share three process globals:
`NATIVE_CALLS`, `FALLBACK_CALLS` and `NATIVE_RECORD`. The native test asserts

```rust
assert_eq!(FALLBACK_CALLS.load(Ordering::SeqCst), 0,
           "native slot must not fall through to publish_raw");
```

which is only true if the fallback test has not run yet. `cargo test` runs the
tests of one binary on parallel THREADS in one process, so the two race.

Measured 2026-08-23, `cargo test -p nros-rmw-cffi --features alloc --test
publish_streamed`, five runs: **ok, ok, ok, FAILED (both cases), ok**. Five more
runs on an unrelated working tree: **FAILED, ok, FAILED, ok, ok**. So roughly
1 in 3 at the binary level, in either direction.

## Why it has never been seen

`just check` and `just test-all` run **nextest**, which executes each test in its
own PROCESS. Two processes cannot share an `AtomicUsize`, so the race cannot
occur there and the suite is honestly green. The failure is reachable only by a
bare `cargo test`, which is what someone does when iterating on one crate.

That asymmetry is worth recording on its own: a test that depends on
process-global state is correct under nextest and wrong under cargo, and the
gate everybody runs is the one that cannot see it.

## Not a phase-376 regression

Found while converting these vtable literals to `..EMPTY_VTABLE` (phase 376 W4).
The first reading was that the refactor broke it — a single `git stash` run
passed, which looked like proof. It was luck: five runs on the stashed tree
failed once too. The lesson is the one CLAUDE.md already records for QEMU reds,
in reverse — a single green is not evidence when the failure is intermittent.

## Fix

Any of:

* give each test its own counters (a struct per test rather than statics), or
* serialise the two with a shared mutex, or
* mark the file `#[serial]` if the crate gains that dependency.

The first is best: the globals exist only because the C stub callbacks have no
context pointer to hang state on — but `stub_publish_streamed` DOES receive a
`void *ctx`, so the state can travel through it like the runtime's own
`process_raw_in_place` trampoline does.
