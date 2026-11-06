---
id: 2258
title: "`nros setup` rewrites a pinned project's `nros-sdk.lock` from a newer fetched index, so the semester pin does not reproduce a seat"
status: resolved
type: bug
area: cli, setup, lock
purpose: [education, industrial]
related: [phase-523, rfc-0116]
found: 2026-11-05
resolved_in: "issue-2257 branch: a release reads its shipped index (RFC-0097 D5 amended)"
---

**Home: [phase-523](../../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round one (2026-11-05); round two counts what is left.

Found by phase-523 W3 adoption probe round one, 2026-11-05, on release `nros-0.6.0-nros6` (2026-10-31, 318 commits behind `main` at the time) in a pristine `ubuntu:24.04` container, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n.

## Measured

- E10, installation.md "Pinning a version" (seat 2): `nros setup mps2-an385-baremetal --rmw zenoh` printed `locked in …/sensor_node/nros-sdk.lock` and rewrote the committed lock to QEMU `11.0.0-nros8`. The next `nros build` → ``Error: 1 dependency drift(s) from the pin set (RFC-0104 D9): tool `qemu` is recorded as 11.0.0-nros8, the index pins 11.0.0-nros7``.
- I11/E9: the same drift error after following the CLI's own `nros setup --tool qemu` advice (issue 2257). Workaround: `nros update --tool qemu`, after which the lock names a QEMU that is not installed.

The education brief's task 4 ("a seat that installs in week 10 gets what week 1 got") fails: the pin the course commits is overwritten by whatever `main`'s index says on install day. Same code on `main` (`setup.rs` "locked in", `orchestration/drift.rs`).

## Next

A committed lock is an input to `nros setup`, never an output: setup provisions what the lock records, and moving it is `nros update`'s job only.

## Resolution (2026-11-06)

Root cause shared with issues 2256–2258: `nros setup` on a release read
`main`'s fetched index, while every other reader (`qemu_bin::for_sdk`, the
build's source preflight, `nros locate`, the drift check) read the release's
own SDK root, about 20 sites. The maintainer ruled that a release uses its own
index, which amends RFC-0097 D5. `setup::release_reads_shipped` makes a
released `nros` answer the store rung with its shipped
`share/nros/nros-sdk-index.toml`, the same file the SDK root carries, and
fetch nothing unless `$NROS_INDEX_URL` names a mirror. A binary with no shipped
copy keeps the fetch path. Unit-tested; round two re-measures it on the
release that carries the change.

The lock was rewritten because setup's pins (fetched) differed from the lock
written by the release (shipped). A project pinned with `nros pin` runs that
release's CLI, whose shipped pins are the lock's, so setup re-records the same
pins and the drift check stays quiet. Moving a pin stays `nros update`'s job.
