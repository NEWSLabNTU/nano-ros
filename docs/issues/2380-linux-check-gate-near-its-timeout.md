---
id: 2380
title: "The build gate `linux-check` takes 62 to 87 % of the gate harness's 2400 s timeout, so a loaded host turns it red"
status: open
type: test
severity: medium
area: [testing, gates, ci]
related: [phase-543, 2373, 2274]
found: 2026-11-09
---

**Home: [phase-543](../roadmap/phase-543-nextest-gates.md)**. A gate whose verdict depends on how loaded the host is: the class that phase's resource groups bound, next to issue [2373](archived/2373-nros-node-yielding-park-test-flakes-under-load.md).

## What

`just ci gate` step 7 (`check::build`) went red on one unit, with no
failure in the code:

```
TIMEOUT [2400.051s] (63/63) nros-gates::gates build::check::linux-check
error: recipe `linux-check` was terminated on line 4882 by signal 15
```

`build::check::linux-check` is the `linux-check` recipe in
`just/check/lanes.just`, which runs `just linux check`: clippy over every
native example leaf, each rendered by `nros configure` first (issue 1892).

The cap is the gate harness's default,
`packages/testing/nros-gates/.config/nextest.toml`:
`slow-timeout = { period = "120s", terminate-after = 20 }`, which is
2400 s. No override gives this gate a longer budget.

## Measured

On `arm-a100` (aarch64), all on phase-530 branches with no change to the
example leaves:

| run | result | time | host load average |
| --- | --- | --- | --- |
| `ci gate`, W3 branch | pass | 1,493 s | not recorded |
| `ci gate`, W3 branch | pass | 2,088 s | about 90 to 100 |
| `ci gate`, W4 stacked on W3 | **timeout** | 2,400 s | about 90 |
| `just check linux-check` alone, W4 rebased on main | pass | 2,080 s | 90 at start, falling to 27 |

So a passing run uses 62 to 87 % of the budget. The run that timed out had
reached the thumbv7m leaves and was still compiling; nothing was hung.

## What would close it

One of these:

- **Split the gate.** One unit per platform family (or per leaf group), so
  each has a budget it uses a fraction of, and nextest can schedule them.
- **Raise its budget, with the reason.** A per-test override in the
  gates' `nextest.toml` for `build::check::linux-check`, sized from these
  measurements, with a comment naming this issue.
- **Serialise it.** Put it in a test-group that keeps it off slots the
  other build units hold, so its time stops depending on what runs beside it.

Acceptance: `build::check::linux-check` passes inside its budget in a
full `just ci gate` run at a load average of about 90, with the choice
named here.

## Profile (2026-11-10, newslab-241, x86_64, 32 cores)

Per leaf, all 32 under the lane's own jobserver pool, timed by a wrapper
around `lint-example-leaf.sh`:

| state | whole lane | slowest leaf | sum of leaf walls |
| --- | --- | --- | --- |
| cold (fresh lint dir), load 4 at start | 82 s | 82.5 s (`mps2-an385-baremetal/c/service-server`) | 2,055 s |
| warm | 2 s | 2.2 s | 51 s |
| cold, inside a full `just ci gate` (other gates beside it) | 831 s | | |

The 14 `mps2-an385-baremetal` C/C++ leaves are the slowest (75-83 s cold),
then the `linux/rust` leaves (40-78 s); `logging` is 11 s. No single leaf or
repeated rebuild dominates. The cost is that each of the 32 leaves is its own
cargo root and compiles the nros stack cold in its own target dir. Issue 0616
rules out sharing a target dir across roots, so this work cannot be pooled.
The lane's wall time is therefore that real work divided by whatever share of
the host it gets. On a loaded host that share shrinks, and the single
2400 s test runs out.

## Fix: one nextest unit per leaf

`check::linux-check` is now a `[group('units')]` gate with one unit per leaf
(`build::check::linux-check#linux-rust-talker`, …). Each unit has its own
2400 s budget; a leaf's cold time on an idle host is 11-83 s. Each unit is
weighted `[group('cpu:8')]` (`threads-required = 8`), so nextest schedules the
units beside the other gates as parallel builds instead of starting all 32
at once.

The leaf list, the fan-out and the leaf `target/` assertion moved from the
`just linux check` recipe body into `scripts/build/lint-examples.sh`, which
both the recipe and the gate call. `just linux check` still lints every leaf
under one jobserver pool. Each unit also runs the `target/` assertion after
its leaf (1 s).

Acceptance still needs a full `just ci gate` at a load average of about 90.
