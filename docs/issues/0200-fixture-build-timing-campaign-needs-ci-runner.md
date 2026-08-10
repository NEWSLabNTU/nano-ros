---
id: 200
title: "fixture-build timing campaign — blocked on a big-disk CI runner (phase-226 validation residue)"
status: open
type: task
area: build
related: [phase-226, phase-340, phase-343]
---

## Summary

Phase 226 (fixture build orchestration audit) landed all its scheduler and
cache work, but three 226.F validation measurements could never run on the
maintainer host: a timed clean `just native build-fixtures` alone consumed
~52 GiB of per-RMW-variant cargo target dirs at 25 min (still incomplete,
host at 3.4T/3.6T) and was killed to protect the partition. See the
archived phase doc's 226.F section and `tmp/phase226/results.md`
(2026-06-13).

## UPDATE 2026-08-10 — re-derive the blocker before buying the runner

The ≥200 GiB requirement below is a consequence of the ~21:1 artifact
duplication, and three landed items have moved that number since it was written:

| landed | effect |
| --- | --- |
| phase-343 W1 (`7db7e72b5`) | 425 leaked sizes-probe dirs, **63.1 GiB** recovered, deduplicating 81:1 |
| phase-340 B3 (`a2e40b12f`, `bc7a286b3`) | `linux` **26796 MB → 5226 MB**; nuttx, freertos and both threadx platforms then joined the shared groups |
| phase-340 item 5 (in flight) | measured prize `linux` 46.1 GiB → ~7.0 GiB, −84.9 % |

Nobody has re-run this issue's arithmetic against that tree. **The first action
here is a re-derivation, not a procurement**: if the full matrix now fits the
maintainer host, the campaign is "run `just build-test-fixtures` twice and read
the joblog", which is what the Context section below already says it mostly is.

This makes the issue a validation item of the phase-340 / phase-334 program
rather than an independent blocked task — it should be run **after** item 5
lands, since item 5 changes both of the things being measured (bytes and
wall-clock) and a pre-item-5 number would be obsolete on arrival.

Also note the follow-up candidate at the end of this issue — "shared Corrosion
`--target-dir` per (triple, feature-set, profile) role group" — **is** phase-340
W2, whose mechanism was decided by measurement on 2026-08-08. It is no longer an
open question this issue needs to carry.

## What to measure (needs a runner with ≥200 GiB scratch)

1. Representative timings for direct platform fixture builds: native,
   qemu, zephyr, freertos, nuttx (clean + warm, `NROS_BUILD_JOBS=8`).
2. Representative `just build-test-fixtures` timing through BOTH the
   fifo-jobserver path (`build-all-jobserver.sh`) and the ordinary-make
   fallback path.
3. CPU utilization under `NROS_BUILD_JOBS=8` and a high-core default run
   (oversubscription vs idle-tail behavior of the make graph).

On a bounded-disk host the full matrix must be built per-platform with
prunes in between, which serializes exactly the wall-clock the campaign is
meant to characterize — hence the runner requirement, not a workaround.

## Context

- The make-driver joblog (`build/fixture-make-driver/…/fixtures.joblog` /
  `tmp/build-test-fixtures-latest`) already records per-leaf start/end/
  duration — the campaign is mostly "run it twice on big iron and read the
  joblog".
- Warm per-platform timings from Wave 13 (native 340 s, qemu 55 s,
  freertos 88 s, nuttx 22 s, zephyr-rs 7 s, zephyr-ccpp 22 s,
  `NROS_BUILD_JOBS=8`) are the only numbers on record; no clean-build or
  jobserver-vs-fallback comparison exists.
- Follow-up candidate identified by 226.E if numbers justify it: shared
  Corrosion `--target-dir` per (triple, feature-set, profile) role group —
  removes the ~200 structurally-non-cacheable staticlib recompiles per
  cell that sccache cannot cover.
