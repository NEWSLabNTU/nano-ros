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
