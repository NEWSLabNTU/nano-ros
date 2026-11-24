---
id: 2957
title: "A knob or Kconfig change can drift the island-shaped Zephyr snapshot unseen: the merge queue never configures that image live"
status: open
type: tech-debt
severity: medium
area: [zephyr, testing, build]
related: [2886, 2780, phase-567]
found: 2026-11-25
---

## What happens

The merge queue checks knob delivery for the island-shaped Zephyr image over a
committed snapshot of its configured build dir
(`tests/fixtures/knob-delivery/island-shaped-zephyr`, phase-567 W4). Only the
daily nightly job `check / template-copy-out [zephyr]` configures the image
live and compares (`just check island-shaped-zephyr`). So a PR that changes
what a Zephyr configure resolves for that image merges green, and the snapshot
goes stale until the next scheduled nightly turns red.

This happened twice:
- issue 2886: the stock node interfaces (phase-556 W13) moved
  `NROS_DERIVED_MAX_SUBSCRIBERS` 3 -> 4;
- nightly run 45422889905, job 138546921094, main `730dfbebeb`: #4569
  (issue 2780) made the parameter-service heap arm a derive sentinel, so the
  island image's heap went from Kconfig to derived:

```text
live     NROS_KNOB_SOURCE_NROS_MEMORY_HEAP_BYTES:INTERNAL=derived
snapshot NROS_KNOB_SOURCE_NROS_MEMORY_HEAP_BYTES:INTERNAL=kconfig
snapshot NROS_RESOLVED_NROS_MEMORY_HEAP_BYTES:INTERNAL=114688
live     NROS_RESOLVED_NROS_MEMORY_HEAP_BYTES:INTERNAL=163840
```

Both changes were intended. Each one only needed
`just check island-shaped-zephyr-regen` in the same PR.

## Why it matters

- A red nightly blocks a release cut (2886 was a release blocker, and this
  drift landed in the nros12 cut).
- Between the merge and the nightly, the merge queue checks knob delivery
  against stale values, so the issue 2618 coverage it claims is weaker than
  it looks.
- Finding the cause takes a live Zephyr build and a bisect over a day of
  merges.

## Direction

Either of these would close it. They are not exclusive.

1. Run `just check island-shaped-zephyr` on PRs whose diff touches the paths
   that decide a Zephyr configure's resolved knobs. At least these:
   `zephyr/Kconfig`, `zephyr/cmake/**` (the resolver, `nros_heap_floor.cmake`),
   `packages/tooling/nros-zephyr-build/knobs.toml`,
   `packages/platform/*/nros-knobs.toml`, the CLI's entity-inventory and knob
   code, `examples/workspaces/island-shaped/**` and
   `scripts/check-knob-delivery.py`. This needs a west workspace on the
   lane. The 2026-11-13 ruling keeps Zephyr builds daily, so it is a ruling
   question, scoped to path-filtered PRs.
2. A cheap queue check that needs no west workspace: when a PR touches those
   paths and leaves `tests/fixtures/knob-delivery/island-shaped-zephyr`
   untouched, it fails and asks for the regen. A trailer could mark a PR that
   was checked live and resolves nothing new, for example
   `Island-Snapshot: checked`. The trailer must be backed by the command's
   output in the PR, not by assertion.

Option 2 is the smaller change. Its path list must stay in step with the
resolver. A self-test can keep it there: every file that a configure's
`NROS_RESOLVED_*` line can come from is on the list.

## Acceptance

- A PR like #4569 (a Kconfig arm becomes a derive sentinel) cannot merge
  with the snapshot unchanged unless it states that the snapshot was checked
  live.
- `scripts/check-default-gates-run-somewhere.py` lists the new check.
