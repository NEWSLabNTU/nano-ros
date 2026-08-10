---
id: 490
title: "`nros-rmw-cffi`'s `rerun-if-changed` named a path that does not exist, so
  every Rust fixture recompiled its whole chain on every build"
status: resolved  # fixed + gated 2026-08-10 (phase-340 P2)
type: bug
area: build
related: [phase-340, phase-321, issue-0196, issue-0445]
---

## Symptom

Every Rust fixture in the repo was permanently non-fresh. A `cargo build` in a
freshly built fixture leaf recompiled `nros-rmw-cffi`, `nros`, `nros-node`,
`zpico-sys`, `nros-rmw-zenoh`, both board crates and the leaf itself — every
time, forever. Nothing failed; the build simply never converged.

Downstream, that made `check-fixtures-stale` print

```
WARNING: N rust fixture(s) were STALE and have now been rebuilt by cargo:
```

on every run for every row it probed — the warning that teaches its readers to
ignore the warning.

## Cause

`packages/rmw/cffi/build.rs`:

```rust
println!("cargo:rerun-if-changed=../nros-rmw-abi/include/nros");
```

There is no `packages/rmw/nros-rmw-abi`. The headers are at
`packages/core/nros-rmw-abi/include/nros`. The line was correct when the crate
was `packages/core/nros-rmw-cffi`; **phase-321 W2.e (`12c365774`) moved the RMW
shim crates from `packages/core/` to `packages/rmw/` and the relative path came
along unchanged.**

Cargo treats a MISSING `rerun-if-changed` input as permanently dirty. Its own
fingerprint log says so exactly:

```
dirty: FsStatusOutdated(StaleItem(MissingFile {
    path: ".../packages/rmw/cffi/../nros-rmw-abi/include/nros" }))
```

and since `nros-rmw-cffi` sits under every nano-ros image, the whole chain above
it went with it.

## Why it survived

It has no failing symptom. The build succeeds; only its INCREMENTALITY is
destroyed, and the one place that reports incrementality — the staleness probe —
reports it as a warning that self-heals. Found by phase-340 P2 while checking
whether a profile change had broken the probe: the probe reported seven rows
stale immediately after building them, and the answer was in
`CARGO_LOG=cargo::core::compiler::fingerprint=info`.

## Fix

Point it at `../../core/nros-rmw-abi/include/nros`. Verified by measurement:
with the path fixed, a single row probes FRESH on a second and third pass, where
before it was dirty on every pass (see issue 0491 for the *other* cause, which
this measurement then exposed — a row is only stable in isolation).

## Gate

`scripts/check-build-rs-rerun-paths.py` (`check-fast`): every static
`cargo:rerun-if-changed=<path>` in a tracked `build.rs` must name a path that
exists. Interpolated paths are skipped (they cannot be checked without running
the script), and `src/**` build helpers are excluded because their relative
paths resolve against the CONSUMER crate — checking them would produce four
false positives and teach people to add exemptions. Self-tests its checker in
both directions on every run.

The sweep over all 57 tracked build scripts found exactly this one.
