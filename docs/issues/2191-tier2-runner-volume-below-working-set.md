---
id: 2191
title: "Tier 2 fills the runner volume in check::default: one full working set does not fit beside the unpruned host targets"
status: open
type: bug
severity: high
area: [ci, testing]
related: [1353, 1094, 1158, 2113, 2179, 2180, phase-466]
found: 2026-11-01
---

**Home:** [phase-466](../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)
— a scheduled lane with no verdict.

## What

run-matrix 41205983383 (`main` at 37d6466751, 2026-10-31) was the first tier-2
run after issues 2179 and 2180 were fixed. Its fixture build PASSED. `just ci
matrix` then died at step 1 (`check::default`): `check-build` failed 10 of 28
gates, every one on `No space left on device (os error 28)` (size probes,
`example-lint` builds, `target/` archives, one `ld` killed by SIGBUS). The
coverage job named itself `NO VERDICT: preflight failed before any cell`.

`runner-doctor` measured 137 GiB free on the checkout's volume at job start.

## What the job's own sweep measured

From `runner-sweep.sh` at the end of the same job (before its eviction):

| area | size | reclaimed by |
| --- | --- | --- |
| `fixtures` (build caches, per coordinate) | 178.1 GiB, budget 60, high-water 327.4 | evicted LRU to the budget |
| `cargo-targets` (host `target/`, `packages/cli/target/`) | 127.5 GiB | NOTHING: `cargo-sweep` is not installed on the runner |
| unbudgeted | 129.7 GiB | nothing (reported only) |
| — of which `examples/` leaf builds | 94.8 GiB | |
| — `build/metadata-probe` | 8.6 GiB | |
| — `packages/testing` | 8.2 GiB | |
| sccache | 30.0 GiB | self-capped |

After eviction: 118 GiB free.

The fixture tree was 4 days old (cold reset at 7 days), so `examples/` is not
residue from retired paths — it is the current leaf builds of a few days of
lanes.

## Why it is structural, not a one-off

- The run grew the fixture area from at most 60 GiB (the previous sweep's
  target) to 178 GiB: a full tier-2 build adds ~118 GiB of fixture caches plus
  leaf builds under `examples/`.
- The sweep then evicts back to 60 GiB, i.e. it drops ~118 GiB of the WARM set,
  so the next run rebuilds it and grows by the same amount again.
- The next run starts with ~118 GiB free, below that growth. Every full tier-2
  run on this volume therefore ends in ENOSPC, unless something else frees
  space between runs.
- NOT MEASURED: why the runs of 10-28..10-30 reached their cells on the same
  box. The likely reading is that 2179/2180 stopped each nightly's build
  part-way, so the warm set was rebuilt in pieces; nothing here proves it.

Arithmetic: fixtures working set ~178 + leaf builds ~95 + host targets ~128 +
sccache 30 + Zephyr SDK (budget 14) ≈ 445 GiB, against 137 GiB free at the
start of a run whose areas held roughly 60 + 128 + 130 + 30 already. The
volume's total size is not in any log, so this is the sum of what the sweep
reported, not a measured capacity.

## The levers

1. **Prune the host target dirs.** 127.5 GiB, reported and never touched,
   because `cargo-sweep` is absent from the runner image (it is in no
   `nros-sdk-index.toml` row, so the image generator never installs it).
   Issue 1094 measured `--time 21` freeing ~41 GiB on a comparable tree.
   **Code half landed:** `[tool.cargo-sweep]` (0.8.0, source build; upstream
   attaches no release assets) is in the index, `just setup base` runs
   `nros setup --tool cargo-sweep`, and `runner-sweep.sh` finds it at the
   store's pinned path when it is not on PATH (the sweep step runs without
   `activate.sh`). What is left is the operator provisioning it on the runner:
   `nros setup --tool cargo-sweep`, or `scripts/ci/runner-provision.sh`.
2. **Give `examples/` leaf builds an area.** 94.8 GiB that the sweep reports
   and its own warning says to budget ("Give the largest entry an area"). They
   are fixture outputs (rebuildable, re-staled by the probe), so they belong
   with the fixture caches.
3. **Size the fixtures budget to a lane's working set.** 60 GiB is a third of
   what one tier-2 build writes, so every sweep discards the warm set. A budget
   below the working set buys a rebuild per run, not disk.
4. **A bigger volume** for this runner (operator).

1 and 4 are operator-side. 2 and 3 are in `scripts/ci/runner-sweep.sh`, but
they only move the problem: with 1 undone, the volume still cannot hold the
working set warm.

## Acceptance

A tier-2 run on `main` reaches its cells (`tier 2 — VERDICT: …` in the
coverage job name), and the sweep at its end reports the host target dirs
pruned or budgeted.
