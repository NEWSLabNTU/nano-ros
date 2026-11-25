---
id: 2957
title: "A knob or Kconfig change can drift the island-shaped Zephyr snapshot unseen: the merge queue never configures that image live"
status: open
type: tech-debt
severity: medium
area: [zephyr, testing, build]
related: [2886, 2780, phase-567, phase-590]
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

## Pricing (measured 2026-11-25)

The snapshot has two halves, and they need different answers.

- **Census half** (`nros/entity_inventory.cmake`, `nros/message_bound_knobs.cmake`,
  the `NROS_DERIVED_*` counts). Issue 2886's drift was entirely in this half.
  The Zephyr image's configure takes these counts from the workspace census,
  and the census's own build dir
  (`build/posix-zenoh-linux-island_bringup-linux/cmake`) states the same
  lines. They were measured equal against the live Zephyr build dir, and
  `nros configure --census island_bringup:freertos` produces it in 46 s on 118.
  `check-workspace-all` already runs that census on every pull request, inside
  `island-shaped-workspace`. Diffing the two files there costs nothing extra
  and needs no west workspace and no path list.
- **Resolver half** (the CMakeCache `NROS_RESOLVED_*` / `NROS_KNOB_SOURCE_*`
  lines and the `build.ninja` defines). #4569's drift was in this half. It
  needs the Zephyr Kconfig, so only a west workspace answers it.

What would trigger a check on the resolver half. Counted over the 600 pull
requests merged from 2026-11-17 to 2026-11-25 (178 of them docs-only):

| Trigger set | Files | PRs hit |
| --- | --- | --- |
| The path list under Direction above (authored) | ~8 globs | 43 (7 %) |
| Tracked inputs on the configure's RERUN_CMAKE edge (derived) | 120 | 112 (19 %) |
| That edge plus the census, metadata-probe and CLI closures | ~770 | 294 (49 %) |

Over the same window only two pull requests changed the snapshot: #4688 (for
2886) and #4823 (for 2780). The authored list happens to catch both causes,
#4283 and #4569. It catches #4283 only through the island workspace's own
files, though, so the next CLI-side change could miss it. The RERUN_CMAKE edge
is the smallest set that is right by construction.

**Option 1, a live check on the pull request.**
- On a hosted runner, nightly job 138546921094 took about 12.6 min: container
  start 74 s, CLI build 118 s, west workspace and SDK 238 s (with an
  SDK-store cache hit), and 324 s for `island-shaped-zephyr` itself.
- At the 19 % trigger rate that is about 14 runs and 3 hosted hours a day.
- A configure-only variant saves only the image build: on 118, the check took
  132 s, of which 39 s was the image build. It would also need a new
  stop-after-host-configure mode in `nros build`.
- On a self-hosted runner that keeps a west workspace, the check is the 132 s
  measured on 118.
- Either way, a Zephyr configure on the pull request lane needs a ruling,
  because the 2026-11-13 ruling keeps Zephyr builds daily.

**Option 2, an input-diff gate.**
- Buildless, about 1 s, on every pull request. Needs no ruling for the lane.
- The cost moves to authors: 19 % of PRs edit an input, and each one has to
  run `just check island-shaped-zephyr` (about 2.2 min on a host with a west
  workspace) or regenerate the snapshot.
- An author on a host without a west workspace needs someone else's host.
  An on-demand `workflow_dispatch` job that runs the live check on a branch
  and prints the trailer would close that gap, at option 1's hosted price,
  paid only when asked for.

### Implemented (draft PR)

1. Census half, step D of `island-shaped-workspace`:
   `knob-delivery-fixture.py diff-derived` compares the census build dir's
   derived counts with the snapshot. It runs on every PR at no extra cost.
2. Resolver half, option 2: the regen writes `inputs.txt` into the snapshot.
   That is the tracked inputs of the live configure's RERUN_CMAKE edge, so the
   list follows the resolver by construction, which is the self-test this
   issue asked for. `just check island-snapshot-inputs` refuses a branch that:
   - edits a listed file,
   - leaves the snapshot untouched, and
   - carries no `Island-Snapshot: checked <digest>` trailer.

   `island-shaped-zephyr` prints that trailer after a live pass. The digest is
   over the listed files' blobs, so an input edited after the check voids the
   trailer. The gate passes on merge_group, push, schedule and dispatch, using
   changelog-audit.py's event and base logic.

Known gap: a change to the CLI alone that moves a resolved knob without
moving a derived count is still nightly-only. The CLI binary is on the edge,
but it is build output, so the list carries no CLI source. Adding the CLI's
closure would take the trigger rate to about 49 %.

## Acceptance

- A PR like #4569 (a Kconfig arm becomes a derive sentinel) cannot merge
  with the snapshot unchanged unless it states that the snapshot was checked
  live.
- `scripts/check-default-gates-run-somewhere.py` lists the new check.
