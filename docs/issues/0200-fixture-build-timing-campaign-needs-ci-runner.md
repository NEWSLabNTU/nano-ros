---
id: 200
title: "fixture-build timing campaign — blocked on a big-disk CI runner (phase-226 validation residue)"
status: open
type: task
area: build
related: [phase-226]
---

## Summary

Phase 226 (fixture build orchestration audit) landed all its scheduler and
cache work, but three 226.F validation measurements could never run on the
maintainer host: a timed clean `just native build-fixtures` alone consumed
~52 GiB of per-RMW-variant cargo target dirs at 25 min (still incomplete,
host at 3.4T/3.6T) and was killed to protect the partition. See the
archived phase doc's 226.F section and `tmp/phase226/results.md`
(2026-06-13).

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
