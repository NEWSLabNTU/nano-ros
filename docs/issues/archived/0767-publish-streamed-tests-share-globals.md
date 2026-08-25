---
id: 767
title: "`publish_streamed`'s two tests share process globals and run in parallel,
  so a bare `cargo test` fails ~1 in 5 — invisible to nextest, which gives each
  test its own process"
status: resolved
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

## A second instance, same crate (2026-08-23)

`tests/rust_adapter.rs::rust_backend_adapter_routes_every_slot` fails the same
way under `cargo test`: `PUBLISH_HITS` reads 2 where it asserts 1, because
another test in the binary drove the same static. Five runs of that test ALONE:
5/5 green. `cargo nextest run -p nros-rmw-cffi --features alloc`: 33/33 green.

So this is not one test's mistake but a crate-wide pattern — the C stub
callbacks have no context to hang state on, so every test file reaches for
`static AtomicUsize`. The fix should be applied to the pattern rather than to
the two files that have been caught: the vtable stubs that matter already take a
`void *ctx` (`process_raw_in_place`, `publish_streamed`), and the ones that do
not could take one the same way the runtime's own trampolines do.

Until then, the honest statement is: **this crate's tests are correct under
nextest and racy under `cargo test`**, and `just check` only ever runs the
former.


## Resolved (2026-08-25) — serialized, measured 3/20 → 0/20

Reproduced first, because the issue's own lesson is that a single green proves
nothing about an intermittent failure. Six runs passed, which would have been
enough to call it fixed by someone else and close this. Twenty runs:

    BEFORE   20 runs: 17 pass, 3 fail
    AFTER    20 runs: 20 pass, 0 fail

### Fix

A `TEST_LOCK: Mutex<()>` beside the four statics it protects, taken at the top of
both tests. Serializing rather than splitting the file or giving each test its
own counters, because the assertion this is protecting is inherently
process-wide:

```rust
assert_eq!(FALLBACK_CALLS.load(Ordering::SeqCst), 0,
           "native slot must not fall through to publish_raw");
```

That is a claim about the whole process, and it is only checkable when nothing
else in the process is publishing.

**This is NOT the fix this issue proposed**, and the difference is worth stating
rather than glossing. The proposal was to give each test its own counters, hung
on the `void *ctx` the stub receives. That is viable and keeps the two tests
running in PARALLEL, which serializing gives up — and the resulting assertion
("MY publisher did not fall through") is arguably the more precise claim, not a
weaker one.

I did not take it because the plumbing is not as ready as the one-line
suggestion reads: `stub_create_publisher` sets `backend_data` itself to a
sentinel (`0xa5a5`), the two `static` vtables are shared per-PATH rather than
per-test, and both deliberately share one `publish_raw` — which is the very thing
the native test is checking is not reached. Threading per-test state through that
means repurposing `backend_data` and touching every stub, for a test that runs in
microseconds and whose parallelism buys nothing.

So: lock now, measured; the ctx refactor stays available if someone later wants
these tests parallel or finds the process-wide assertion too coarse.

The guard ignores mutex poisoning (`unwrap_or_else(|e| e.into_inner())`). Without
that, a panicking test poisons the lock and the OTHER test then fails with a
poison error rather than its own assertion — one real failure reported as two,
with the cause hidden.

### The asymmetry is the durable part

`just check` and `just test-all` run **nextest**, which gives each test its own
PROCESS; two processes cannot share an `AtomicUsize`, so the race is unreachable
there and the suite is honestly green. It is reachable only by the bare
`cargo test` someone runs while iterating on one crate.

So the gate everybody runs is the one that cannot see this class, and the
developer loop is the one that can. A test depending on process-global state is
correct under nextest and wrong under cargo — worth remembering the next time a
crate-local `cargo test` disagrees with CI.
