---
id: 612
title: "`tests/signal_fd_wake.rs` cannot link in any configuration, and no lane
  enables the feature — so `signal-fd-wake` has never been runtime-tested"
status: resolved
type: bug
area: testing
related: [issue-0577, issue-0196, issue-0652, issue-0667, phase-359]
---

## Symptom

`nros-node`'s `signal-fd-wake` feature ships a dedicated integration test,
`packages/core/nros-node/tests/signal_fd_wake.rs`, which its own header says to
run as:

```bash
cargo test -p nros-node --features "signal-fd-wake,rmw-cffi" --test signal_fd_wake
```

That command does not work, and no variation of it does.

## Two independent causes, both pre-existing

**1. It cannot LINK.** `nros-node` has no platform provider in its
dev-dependencies (only `nros-ghost-types`). The test's feature set pulls in
`NodeWake`, whose inner gate is `all(feature = "alloc", feature = "rmw-cffi")`,
and that calls the platform ABI directly. So the link fails on symbols the crate
never provides:

```
rust-lld: error: undefined symbol: nros_platform_wake_init
rust-lld: error: undefined symbol: nros_platform_wake_wait_ms
rust-lld: error: undefined symbol: nros_platform_wake_signal
rust-lld: error: undefined symbol: nros_platform_wake_storage_size
...
```

Other `nros-node` test binaries in the same feature set link fine because
nothing in them REFERENCES those symbols — the wake path is dead-code-eliminated.
Turning `signal-fd-wake` on is what makes them live.

**2. Nothing runs it.** `grep -rn "signal-fd-wake" just/ .github/ scripts/`
returns exactly one hit, and it is a comment. No `just` recipe, CI job or script
enables the feature, so no lane would have noticed cause 1.

Until phase-359 W10 the documented command also tripped a
`compile_error!("`signal-fd-wake` needs OS services: add \"std\"")`, because it
does not pass `std` — a third way the same command failed.

## Why it matters

This is issue 0577's class: a test that no lane runs, and which has apparently
never passed. The phase-359 doc names the class explicitly ("expect untested
code … budget for that rather than treating each as a surprise"), and this is
the budgeted instance.

It surfaced because W10 ported the forwarder's worker from `std::thread` to a
platform task. That port is therefore COMPILE-verified only; the runtime
behaviour — a signal handler writing the eventfd and the executor waking — has
no passing test before or after.

## Fix sketch

Two halves, and the first is the one that decides the second:

* **Give the test a platform.** Add a platform provider to `nros-node`'s
  `[target.'cfg(not(target_os = "none"))'.dev-dependencies]` (the POSIX C port
  is the natural one — it already supplies `nros_platform_wake_*` and now
  `nros_platform_task_*`). Check what feature unification that pulls into other
  test binaries before committing to it; the dev-dep comment in that manifest
  already records one round of pain here.
* **Give it a lane.** A feature with no lane rots regardless of whether it
  links. Either add it to a `check-*` lane or, if the capability is not worth a
  lane, say so and delete the test rather than leaving it as apparent coverage.

Do not "fix" this by loosening the test's `#![cfg]` until it compiles — that
would restore the appearance of coverage without the substance, which is what
the current state already provides.

## Update 2026-08-16 — cause 1 fixed, and a THIRD cause it was hiding

**Cause 1 (cannot link) is fixed.** `nros-node` gains
`nros-platform-cffi = { features = ["posix-c-port"] }` in its target-scoped
dev-dependency table, and the test carries `use nros_platform_cffi as _;`.

That second line is load-bearing and not optional: rustc drops a dev-dependency
no code references, taking its build script's `cargo:rustc-link-lib` with it, so
the undefined symbols return looking exactly as they did before the fix. Same
lesson as issue 0619, one crate over.

**With the link fixed, the test runs — and both cases silently SKIPPED and
reported PASS:**

```
nros: cannot select an RMW backend — no RMW backend is registered
[SKIPPED] Executor::open failed — no transport. …
test signal_fd_wake_unblocks_spin_once ... ok
test result: ok. 2 passed; 0 failed
```

`eprintln!` + `return` — the shape CLAUDE.md prohibits outright. Both `Err`
branches now `panic!` with the diagnosis, so the suite states the truth.

### Cause 3 — the gating makes the wake path unreachable by any invocation

Not "no transport is available". The two cfgs are mutually exclusive:

| | gate |
| --- | --- |
| `NodeWake` (the code under test) | `all(feature = "alloc", feature = "rmw-cffi")` |
| `nros_node::mock` (the only session a bare `cargo test` can open) | `all(test, not(feature = "rmw-cffi"))` |

The feature set that makes the wake path live is exactly the one that removes
the session, and `nros-node` registers no cffi backend of its own. So no
invocation both compiles the code under test and opens a session. The
documented command in the test header cannot work even now that it links.

This is why the runtime behaviour has never been verified, and it is a
structural gap rather than a missing dependency — worth separating from cause 1,
because fixing the link makes the test *runnable* without making it *meaningful*.

### Deliberately NOT done: giving it a lane

Cause 2 (nothing runs it) stays open on purpose. Adding a lane now would only
add a red, since cause 3 means the test cannot pass. The ordering has to be:
resolve the gating, then add the lane. Adding the lane first would put pressure
on whoever hits the red to reach for the fix this issue already warns against —
loosening the `#![cfg]` until it compiles, restoring the appearance of coverage
without the substance.

### Also worth noting for the class

The assertions are upper-bound only (`elapsed < TRIGGER_DELAY_MS + 100`). A
`spin_once` that never blocks at all passes them. Whoever resolves cause 3
should add the lower bound (`elapsed >= TRIGGER_DELAY_MS`) — otherwise the test
still cannot distinguish "woke on the eventfd" from "never waited".

## Resolution 2026-08-18 — cause 3 fixed by moving the test, cause 2 by giving it a lane

The 2026-08-16 update fixed cause 1 (cannot link) and left the two that decide
whether this is coverage. Both are closed now, in the order that update
prescribed: resolve the gating first, then add the lane.

### Cause 3 — the test moved to where a session exists

The gating was never going to resolve inside `nros-node`. `NodeWake` is gated
`all(alloc, rmw-cffi)`; `nros_node::mock` is gated `all(test, not(rmw-cffi))`;
the crate registers no cffi backend of its own. Any fix confined to that crate
is either a second mock behind the same feature or the `#![cfg]` loosening this
issue warns against.

What was missing is not a cfg but a **registered backend and a router**, and a
crate that has both already exists. The test is now
`packages/testing/nros-tests/tests/signal_fd_wake.rs`, behind a new
`signal-fd-wake-test` feature (`trigger-test` + `nros-node/signal-fd-wake`):
`zenohd_unique` supplies the session, `use nros_rmw_zenoh as _;` supplies the
backend, and `nros-tests`'s `lib.rs` already force-links the platform C port.
The `#![cfg]` is NARROWER than before, not wider — `nros-node`'s own test file
is deleted, not relaxed.

`nros-node` keeps its `nros-platform-cffi` dev-dependency: `src/lib.rs` carries
a `#[cfg(test)] extern crate nros_platform_cffi as _;` for the crate's own unit
tests (254 of them, `just check node-std-tests`). Its comment now says that is
what the entry is for.

### Cause 2 — the lane

`just check required-features-tests`, issue 0652's lane, which landed
independently and in parallel with this work. This target joins it: 18 tests
became 20, green.

### The assertions were upper-bound only, and it mattered

Both cases now assert `elapsed >= TRIGGER_DELAY_MS` as well as the upper bound,
per this issue's closing note. Mutation-checked by replacing
`spin_once(1000ms)` with `spin_once(0)`:

```
spin_once returned after 58.979µs, before the eventfd write at +30 ms —
it did not block, so this run proves nothing about the wake path
```

Without the lower bound that run passes, which is the whole objection.

### What running it found

**`signal-fd-wake` was broken, not merely untested.** The first real execution
failed with `NotInitialized`, from `nros_platform_task_init` returning
`NROS_PLATFORM_RET_INVALID` (-7): the worker asks for an 8192-byte stack and
glibc's `PTHREAD_STACK_MIN` on x86_64 is 16384, so the POSIX port refused it.
`Executor::signal_fd()` had therefore been dead on every Linux host since
phase-359 W10 moved the worker to a platform task — and three other ports had
the same defect in its quieter form (forwarding a below-floor request to a task
that overflows later). → **issue 0667**, where the fix lives: `stack_bytes` is a
FLOOR, and each port raises it to its own minimum.

That is the return on this issue. A test that no lane runs is not weaker
coverage than one that runs; it is a claim of coverage over a capability that
did not work.
