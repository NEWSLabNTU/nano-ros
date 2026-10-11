---
id: 1817
title: "`nros sync` printed `done.` and exited 0 over a deploy-bound leaf whose own `Cargo.toml` cargo cannot resolve"
status: resolved
type: bug
severity: low
area: [cli, orchestration]
related: [0288, 0426, 1305]
found: 2026-10-11
---

## Measured

While measuring issue 1305's board switch, the mps2 bare-metal talker's board
dependency was renamed to `nros-board-mps2-an385-freertos`, keeping
`features = ["board-entry"]`. That board crate has no `board-entry` feature, so
the manifest cannot resolve on any target. `nros sync` reported:

```
sync: source metadata — no producer for qemu_bsp_talker::talker (deploy-bound probe failed: metadata-mode harness failed (exit 101) for component 'talker': error: failed to select a version for `nros-board-mps2-an385-freertos`.)
...
sync: done.
```

and exited 0. The cause (`… does not have that feature`) was not in the line.
It surfaced only when `nros build` failed.

## Why

[Issue 0288](0288-deploy-bound-examples-have-no-metadata-producer.md) made a
deploy-bound probe failure best-effort. The probe compiles target code for the
HOST, and a board the host cannot build is not the user's mistake, so the
component falls back to the SystemModel bound and the failure is negative-cached.
That is right for a host-build wall. It was also applied to a failure that has
nothing to do with the host: a manifest cargo cannot resolve fails on every
target, so the degrade only moved the error from `nros sync` to the build.
[Issue 0426](0426-metadata-probe-compiles-target-only-deps-on-host.md) folded the
first diagnostic line into the report, and for a resolver error that line names
the crate but not the cause.

## Fix

When a deploy-bound probe fails, sync now asks cargo whether the component's own
manifest resolves (`metadata_build::component_resolution_error`). It runs
`cargo metadata --locked` in the probe's own resolution context: the same cwd and
the same `[patch]` rows. If the manifest does not resolve, sync stops with
cargo's whole error, minus progress lines, and writes no negative-cache marker.
Re-checking costs no compile. If the manifest does resolve, the 0288 degrade is
unchanged. A failure only the harness hits never reads as the leaf's fault.
`--locked` keeps the check from writing a lockfile, and a lock it would have had
to update counts as undecided, never as the leaf's fault.

Verified both ways with the rebuilt CLI on a copy of
`examples/mps2-an385-baremetal/rust/talker`:

- With the bad feature, `nros sync` exits 1 and prints
  `… depends on nros-board-mps2-an385-freertos with feature board-entry but … does not have that feature.`
- Unmodified, it prints `source metadata — 1 rebuilt`, then `sync: done.`, and
  exits 0.

Unit test: `component_resolution_error_names_an_unresolvable_manifest_only`
(path dependencies only, so no registry is reached and nothing compiles).
