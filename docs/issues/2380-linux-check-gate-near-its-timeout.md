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

**Home: [phase-543](../roadmap/phase-543-nextest-gates.md)**. A gate whose verdict depends on how loaded the host is: the class that phase's resource groups bound, next to issue [2373](2373-nros-node-yielding-park-test-flakes-under-load.md).

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
