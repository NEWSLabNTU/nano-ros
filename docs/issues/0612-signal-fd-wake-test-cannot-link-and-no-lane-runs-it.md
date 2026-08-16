---
id: 612
title: "`tests/signal_fd_wake.rs` cannot link in any configuration, and no lane
  enables the feature — so `signal-fd-wake` has never been runtime-tested"
status: open
type: bug
area: testing
related: [issue-0577, issue-0196, phase-359]
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
