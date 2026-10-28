---
id: 2128
title: "nros-params' `!verdict.is_ok()` passes stable clippy and fails the pinned nightly's, so only the scheduled feature sweep saw it"
status: open
type: bug
area: [ci, lint]
severity: low
found: 2026-10-29
related: [phase-466, 2104]
---

**Home: [phase-466](../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)**, since 2026-10-29.

## Measured

`host-tests`' per-feature clippy job, run 40625461704 (2026-10-28):

```
FAIL nros-c [ros-humble,rmw-cffi,alloc,panic-platform,platform-nuttx] --target armv7a-nuttx-eabihf
      error: this boolean expression can be simplified
      error: could not compile `nros-params` (lib) due to 4 previous errors
FAIL nros-cpp [... same ...]
```

All four errors are `if !verdict.is_ok() { return verdict; }` in
`packages/core/nros-params/src/server.rs`, added on 2026-10-27 by the
issue-2104 change (`48319f54cb`). The job passed on 10-26 and 10-27.

Reproduced on a target with a prebuilt std. The pinned nightly flags it and
stable 1.99 does not:

```
cargo +nightly-2026-04-11 clippy -p nros-params --features alloc --target thumbv7m-none-eabi -- -D warnings   # 4 errors
cargo clippy -p nros-params --all-features --all-targets -- -D warnings                                       # clean
```

NuttX is reached only because `check-feature-sweep.py` builds its
`build-std` boards with the pinned nightly (`build_std_args`). The lint fires
on any target under that toolchain. Every merge-gating clippy runs stable, so
nothing before the schedule saw it.

## Fix

The four sites are `self.check_apply(...)?;` (and
`Self::check_set_existing(...)?;`). `is_err()` alone then trips
`question_mark`, which is why the `?` form is used. Both toolchains are
clean, and `cargo test -p nros-params --all-features` passes (101 tests).

Not fixed: the gap itself. A lint only the nightly reports reaches `main`
whenever stable does not have it. Running a nightly clippy on the PR lane
would close that gap, at a cost this issue does not measure.

## What would close it

A scheduled `host-tests` run whose per-feature clippy job passes.
