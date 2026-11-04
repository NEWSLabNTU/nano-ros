# Phase 535 — Example pairs into the language workspaces, single-node witnesses, and cadence-backed CI

**Status (2026-11-04). OPEN — not started; W1 and W2 (the gate and the
measurement) first.**
Implements [RFC-0122](../design/0122-behaviour-bringups-road-witnesses-cadence-backed-ci.md)
D1 to D9, with its amendments to RFC-0026, RFC-0051, RFC-0061, RFC-0066,
RFC-0114 and RFC-0121 D8.

Depends on:

- [phase-531](phase-531-workspace-first-project-layout.md) W2 to W5: the
  bringup rule, launch generation and the one build road. This phase replaces
  phase-531 W6 for the pairs under `examples/`, and W6 still migrates the
  witnesses, `packages/testing/` and the templates in place;
- phase-531 W7 step 3 (the selection crate retires) for Rust workspaces with
  more than one bringup. W0 below checks this rather than assuming it;
- issue 2251 (merged): the west lane reads isolation values from
  `nros_tests::alloc`.

## Why

275 example fixture rows build 275 separate projects whose node code the
language workspaces already hold. On the CMake RTOS roads, each one builds its
own kernel, network stack, RMW and `nros-c`. Separately, two support promises
have no CI job: `cadence = "merge"` for linux, zephyr native_sim and
threadx-linux, and tier 3. RFC-0122 has the measurements and the workflow
survey.

## Work items

### W0 — Can a workspace hold several bringups?

- Build a Rust workspace with two bringups on the phase-531 tree and check
  that each bringup's images build with their own selection. Today
  `generate_facade_crates` (`packages/cli/nros-cli-core/src/cmd/ws.rs`)
  declines to generate facades for it.
- Reproduce RFC-0066's C/C++ open question: with three bringups, does one
  bringup's capability set still overwrite another's for the whole configure?

**Acceptance:** both answered with a build. If either fails and is not fixed
here, D1 uses the fallback (image rows in `demo_bringup`) for that language,
and RFC-0122 records it.

### W1 — The coverage ratchet and the image-row check

- The D5 gate: derive (platform family, language, build driver, project
  shape, launch origin) per runtime cell from the manifest. Fail when a tuple
  in the floor has no cell. The floor is a committed snapshot of the tree at
  adoption, which may only grow.
- The image-row check: every `[image.*]` in a bringup resolves to a known
  board, an existing or generated launch file, and a matrix cell.
- Both are static, on the fast line.

**Acceptance:**
- A mutation that deletes the last witness of a tuple fails the ratchet,
  naming the tuple.
- A mutation that adds an image with an unknown board fails the row check.
- Both mutations are recorded in the commit.

### W2 — Grouping, the resolver, and the allocator lookup

- Find out whether the fixture builder groups `[[workspace_fixture]]` rows
  that share (dir, bringup, board, rmw) into one configure. If not, make it.
- `nros_tests::fixtures::workspace_image`: (cell, role) → (workspace,
  bringup, image id). This is the only place that knows the mapping (D4).
- `zephyr-fixture-leaves.sh`: workspace west rows map their image id to a
  workload for the `cell-isolation` lookup (D7).

**Acceptance:**
- One grouped configure builds two images of one bringup, and `ninja -t
  query` shows one runtime library feeding both.
- The resolver is unit-tested on every pair cell in `matrix::CELLS`.

### W3 — Pilot: `mps2-an385-freertos`

- Move its 18 pair rows into `{rust,c,cpp}/src/{pubsub,service,action}_bringup`
  as `freertos_*` images. Keep `examples/mps2-an385-freertos/<lang>/talker/` as
  the witnesses.
- Measure with `scripts/dev/measure-fixture-build.sh`, cold, before and after,
  as RFC-0066 did. Record it in `docs/roadmap/data/phase-535-w3-pilot.md`.

**Acceptance:**
- `just test freertos` is green, with the same rstest case names as before.
- The ratchet is green.
- The measured saving is recorded, whatever it is.
- A saving under 10 % stops the phase for a maintainer decision before W4.

### W4 — The merge boards: linux, zephyr native_sim, threadx-linux

Same move as W3, all three RMWs. `demo_bringup`'s `linux_service_*` and
`linux_action_*` images move into the behaviour bringups, so each image exists
once.

**Acceptance:** `just ci tier1` is green, and
`zephyr::example_e2e` keeps its 27 case names. The cross-Agent XRCE repro of
issue 2221 (two Agents, two cells) hears 0 foreign samples.

### W5 — The other platforms

nuttx (arm, riscv), threadx-riscv64 (with its cyclonedds rows) and
mps2-an385 bare metal (with the `-rtic` and `-rtic-mixed` variants per D2).
One PR per platform directory, each rebuilding its own lane.

**Acceptance:** `just test <platform>` is green for each; `just ci matrix`
(tier 2) is green at the end.

### W6 — Variants, configurations and feature demos

- The 22 variants become node packages.
- The 7 configurations become image rows with knobs.
- The 13 feature demos become node packages in `features` or the language
  workspace.
- `talker-xrce` becomes an `rmw` row.

All per RFC-0122 D2.

**Acceptance:** each row's consumer test is green; the ratchet floor is unchanged.

### W7 — Retire the old leaves, add `road_witness_e2e`

- Delete every pair, variant, configuration and feature leaf moved by W3 to
  W6. Keep the 22 witnesses and `custom-platform`.
- `road_witness_e2e`: each witness talker against the workspace listener of the
  same (platform, language, rmw).
- Sweep every hard-coded leaf path from the 2026-11-04 survey:
  - `nightly.yml`: path filters 197-203, the 10 Zephyr leaves 819-829, `c/talker`
    at 1036 and 1142;
  - `just/ci.just` 229-231, and `justfile` `rust-rtos-link-check`;
  - `board-support.toml` `link_check_example`;
  - `just/zephyr-dev.just`, `just/zephyr-ci.just` 238;
  - the per-platform `just/*.just` recipes;
  - `justfile:360`.

  Most of these name `rust/talker` and keep working. Each one is checked, not
  assumed.

**Acceptance:**
- `git ls-files 'examples/*/*/*/'` lists only witnesses, `custom-platform`
  and templates.
- `just ci full` is green with a lower fixture-configure count.
- The book's example pointers resolve.

### W8 — CI follows the cadences

1. `merge-runtime` on `merge_group`, **advisory**: one hosted shard per merge
   board, running the behaviour bringups (zenoh, three languages) plus that
   board's witnesses, then their pair cells.
2. After a week, if p95 is at most 20 min per shard, it joins `CI`. Otherwise
   the three boards move to `cadence = "daily"` and `support-status.md`
   stops saying "every change" (RFC-0122 D9).
3. Tier 1 full moves to hosted shards, daily.
4. The Zephyr nightly goes from 20 hard-coded build jobs to 6 (2 lines × 3
   languages), with native_sim runtime.
5. `release-gate`: `ci full` on dispatch; `release-nros.yml` requires a green
   run on the tagged commit.
6. Stale headers, fixed against the workflows they describe:
   - `nightly.yml:25` (no PR trigger)
   - `post-submit.yml:38-40`
   - `queue.yml:95-99`
   - `docs/development/multi-agent-ci-workflow.md` 36-44
   - `support-status.md` tier-2 trigger wording

**Acceptance:**
- Every `cadence` in `board-support.toml` names a job that ran in the last
  window. A gate reads both files and refuses a cadence with no job.
- A release cannot be cut without a green tier 3 on its commit.

### W9 — Docs

- RFC-0122 flips to Stable.
- `examples/README.md`, `examples/workspaces/README-layout.md` (the new
  bringups and the image-id vocabulary), the book's first chapters (witness
  paths), and `book/src/reference/support-status.md`.
- One pointer line in CLAUDE.md.

## Acceptance (phase)

- The ratchet floor from W1 holds at every merge of this phase.
- Rstest case names in the pair consumers are unchanged.
- The measured fixture-build time of `lane=all` is recorded before W3 and
  after W7.
- Every RFC-0114 cadence has a job (W8), or has been lowered.
