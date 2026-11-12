# Phase-555 W2, slice `test-ci`: verdicts and decision brief, 2026-11-13

The test-ci slice of [phase-555](../phase-555-design-doc-review.md) W2. It
holds one verdict per RFC in the area, and the area's one decision brief. Every
fact is cited to the file and line it was read from on 2026-11-13 (branch
`phase-555-design-doc-review`, PR #3655, as of its W1 commits). Mechanical fixes were made in
the RFCs themselves, in the same commit as this file; nothing here changes a
design until it is ruled.

Inputs: `python3 scripts/docs/rfc-inventory.py` (126 RFCs, 352 pairs), its
`slices/test-ci.md` (two tier-2 pairs, both on RFC-0051), and RFC-0122 weighed
against RFC-0061 and RFC-0066 at the manager's request. The maintainer ruling
relayed with this slice: linux, Zephyr native_sim and threadx-linux runtime
tests run at `cadence = "daily"`, and there is no merge-time runtime lane (the
`merge-runtime` job is dispatch-only). That is the RFC-0122 D9 fallback.

## Verdicts

| RFC | `area:` | Verdict | Mechanical fixes made |
| --- | --- | --- | --- |
| 0051 Test-matrix architecture | `test-ci`, confirmed | **keep** | yes, see below |
| 0058 Multi-edition ROS test harness | `test-ci`, confirmed | **keep** (status: brief item 4; lane: item 5) | yes |
| 0061 Fixture freshness and the tier ladder | `test-ci`, confirmed | **amend** (brief items 1 to 3) | yes |
| 0066 Example and fixture consolidation | `test-ci`, confirmed | **keep** (status: brief item 4) | yes |

### RFC-0051: keep

The design is what the code does. Its text had fallen behind the code in
places, and those are fixed.

- The matrix is one declaration, with two lists: `matrix::CELLS` and
  `interop::CELLS` (`packages/testing/nros-tests/src/matrix.rs:896`,
  `src/interop.rs`). The ⊆⊇ coverage gate with G1 to G4 is
  `tests/matrix_fixture_coverage.rs:582-706`. The §7 `config` axis is in the
  code (`matrix.rs:711`, `Cell::config` at `matrix.rs:807`), and so is
  `Tier::Gated` (`matrix.rs:780`).
- **Pair 0051-0114 (tier 2):** RFC-0114 D12 did take the cell rows
  over. `CELLS = CELLS_GENERATED`, from `matrix_cells.rs`, which
  `scripts/gen-board-support.py` writes (`matrix.rs:892-897`,
  `matrix_cells.rs:1`). Its narrowing also applied: fixture rows stay authored
  (phase-522 W2, archived; RFC-0114 changelog 2026-11-01). So 0114 amends §1,
  and neither RFC said so. The note is added to 0051. The frontmatter
  `amends` link is W3's job.
- **Pair 0051-0061 (tier 2):** this pair is complementary, not a conflict. The
  evidence row is 0061's axis table, which names RFC-0051 §7a as the source of
  `config`. 0051 owns the cell taxonomy and 0061 owns the cover over it. Each
  cites the other, and neither restates the other's rules.
- **Fixed:** the allocator formula in §3 now matches `src/alloc.rs:38-117`
  (the draft's `workload*40 + lang*10 + rmw` was never shipped).
  `matrix-gen` → `tests/matrix_fixture_coverage.rs` plus the `cell-isolation`
  binary (`src/bin/cell-isolation.rs`, issue 2251). `StandardChecker` → the
  `checker` module's `ready_marker` / `delivery_marker` / `assert_delivery`
  (`src/checker.rs:31,42,122`). `matrix::sched_dims::CELLS` →
  `matrix::SCHED_CELLS` (`matrix.rs:1730`), and the dims added since.
  `ws-realtime` → `realtime-{c,cpp,rust}`. The §7b registry is named:
  `.config/feature-lanes.toml` with `scripts/feature-lanes.py`. A cross-ref to
  RFC-0114 is added. The esp32 lines in §1 and §4 are left as written
  (ESP32 is dormant, issue 1525).

### RFC-0058: keep

Phase-309 shipped it and is archived as complete. The design holds: one
provider and two backends, codegen and interop axes, and a lane that is opt-in.

- `RosEnv`, `HostRosEnv` and `DockerRosEnv` are in
  `packages/testing/nros-tests/src/ros_env.rs` (lines 94, 262 and 417), and
  `for_edition` is at line 1167. The image is `docker/ros-editions/Dockerfile`,
  the recipes are `just/ros-editions.just` (`image`, `image-check`, `ci`), and
  `test-all` deselects the tests (`justfile:2851-2854`).
- **Fixed:** the trait sketch now carries an as-built note (`shell`, `run`,
  `run_text` and `spawn`, with peers and bridges spelled as shell strings, and
  `generate` an inherent method on the docker backend). The retired recipe name `ros-edition-image` →
  `just ros_editions image`. The "zenoh 1.7.2 source overlay" → apt
  `rmw_zenoh_cpp` (`Dockerfile:18-21`, where the pin is left unbuilt), with
  RFC-0075 cited. The implementation section notes that phase-309 is archived.
- Not fixed, and outside this slice: two code comments in `ros_env.rs`
  (lines 759 and 903) still say zpico is "pinned 1.7.2". The pin is 1.10.1
  (`packages/rmw/zenoh/zpico-sys/zenoh-pico/version.txt`), and CLAUDE.md's
  pitfall line, which says 1.8.0, is stale too.

### RFC-0061: amend

Both proposals shipped. The fingerprint is `nros codegen-fingerprint`
(`nros-cli-core/src/cmd/mod.rs:171`) and `scripts/build/resolve-fingerprint.sh`.
The golden test is `packages/cli/rosidl-codegen/tests/codegen_golden.rs`. The
covers are computed in `nros-tests/src/ci_lane.rs`, and `nightly.yml` has its
`lane` and `lane-coverage` jobs (`nightly.yml:81`, `:677`).

What does not hold is the ladder's "when" column against the cadence ruling:

- The table says tier 1 runs on "every commit, pre-push". `support-status.md:48`
  says the same ("`ci` (every change)").
- In fact, tier 1's runtime half runs daily in `run-matrix.yml`'s `tier1` job
  (`run-matrix.yml:23`, cron `0 6`; `host-tests.yml:13-17` says why it moved
  there). The PR and merge-queue lanes run only the compile gate subset and
  `test-unit`.
- The 2026-11-04 amendment defers to RFC-0122 D8. D8 names a `merge-runtime`
  job and a `release-gate` job, and neither exists in `.github/workflows/` on
  `origin/main` (`be32b60ade`).
- **Fixed:** frontmatter gained `supersedes` / `superseded-by`. Tier 0 is now
  spelled `just check fast`, with the current `just ci <rung>` names and their
  forwarders (`justfile:3767-3785`). A note says the committed SystemModel and
  `nros::main!(model = …)` are now a build artifact and `launch =`. A note says
  the signature is now `v4` (issue 2441; `workspace-fixture-signature.sh:28`).

### RFC-0066: keep

Its two rules are live, and CLAUDE.md quotes them: a feature is a node
package, and a configuration is a fixture axis.

- `examples/workspaces/features/` exists, and the language workspaces carry
  cyclonedds and xrce rows as well as zenoh: about 64 cyclonedds and 40 xrce
  `[[workspace_fixture]]` rows, counted from `examples/fixtures.toml`. Before
  this RFC there were 2 non-zenoh rows of 86.
- The one-system-per-workspace limit its open question names is still in the
  code (`nros-cli-core/src/cmd/ws.rs:3466-3470`). RFC-0121 W7 is the place
  that retires it, so nothing here is for test-ci to rule.
- **`area:` check:** half of this RFC is example layout, which RFC-0026,
  RFC-0121 and RFC-0122 (all `workspace-system-model`) now own. RFC-0122
  already superseded its "Standalone examples" paragraph. The other half is
  the fixture-axis rule, which no other RFC states. `test-ci` stays, as the
  owner of that rule.
- **Fixed:** an outcome note above the measurements maps the `ws-*` names to
  today's directories. It also records where phase-331's result differed from
  the plan: safety became 1 workspace, not 0; realtime became 3 + 1; the phase
  ended at 15 directories, not 12 or 13. It points at RFC-0122.

## Decision brief: test-ci (for ruling)

### Settled by the cadence ruling (confirm as a group)

| # | Question | Answer the ruling gives | Measured fact | Recommendation |
| --- | --- | --- | --- | --- |
| 1 | Does RFC-0061's ladder still promise tier 1 "every commit"? | **No.** The rungs say what a run covers, and RFC-0122 D8 as ruled says how often CI runs it. No runtime rung gates a merge. | Tier 1's runtime half is on cron `0 6` (`run-matrix.yml:23`). The PR and queue lanes are compile, source gates and `test-unit` (`gate.yml:1093`). `support-status.md:48` still says "every change". | **Amend 0061.** Split the "when" column into "a developer runs it locally when" and "CI runs it" (daily for tier 1 runtime, the gate on every PR and queue entry). State that no runtime rung gates a merge. The book row follows. |
| 2 | RFC-0122 D8 names `merge-runtime` (on `merge_group`) and `release-gate`. Do 0061's 2026-11-04 amendment and RFC-0114's cadence binding keep pointing at them? | **No.** D9's fallback is taken: the three boards move to `daily`, and `merge-runtime` is at most dispatch-only. | Neither job exists in `.github/workflows/` on `origin/main` (`be32b60ade`). RFC-0114 lines 135-137 cite `merge-runtime` as the job behind `merge`. | **RFC-0122's reviewer (`workspace-system-model`) amends D8/D9** to record the ruling, and 0061's 2026-11-04 amendment then points at the amended row. In flight: PR #3594 (phase-535 W8) records the ruling in RFC-0122 D9 and adds `merge-runtime.yml` as `workflow_dispatch`-only, never part of `CI`. |

### Open: the ruling does not decide these

| # | Question | Options | Measured facts | Recommendation |
| --- | --- | --- | --- | --- |
| 3 | When the three boards move to `daily`, what says which boards make up tier 1's pool? | (a) A separate registry field for tier-1 membership (for example `tier1 = true`), and `cadence` says only how often. (b) Derive the pool from `runs = "host"`. (c) Keep `cadence = "merge"` on the three boards, with "merge" meaning "the tier-1 pool". | `ci_lane.rs:861-920` asserts that the tier-1 pool equals the set of boards with `cadence == "merge"`, read from the registry. Those are exactly linux, zephyr (host, i.e. native_sim) and threadx-linux (`board-support.toml:113,132,233`). Lowering them to `daily` with no other change empties that set, and the assertion fails. (b) is wrong: `nros-board-freertos-posix` is `runs = "host"`, `cadence = "daily"`, and not in the pool. (c) keeps a word whose rendered meaning is "every merge" (`board-support-levels.md:5`) for a run that is daily. | **(a).** Frequency and tier membership are two facts. The book's "Verified on host (daily)" then renders true. This is joint with `platform-board-rtos` (RFC-0114 owns the field set). It lands in the same change that lowers the cadences, or that change goes red. **In flight, option (d):** PR #3594 (phase-535 W8), which lowers the three cadences, derives the pool from the existing fields instead: `runs = "host"` AND `breadth = "witness"` AND an automated cadence. That separates freertos-posix (not a witness) without a new field, and its `ci_lane.rs` test is updated in the same change (a mutant dropping the witness term goes red). If (d) is accepted, this item closes when #3594 merges. |
| 4 | RFC-0058, 0061 and 0066 are `Draft`, and their phases are archived (309, 318/410/510, 331). Do they move to `Stable`? | Stable now; or stay Draft until W1's status normalization. | Each design is in the tree (the verdicts above). 0061 is still amended about once a month, and `docs/design/README.md` defines Stable as a living document that can be amended. | **Stable for all three.** RFC-0061 takes item 1's amendment in the same change. |
| 5 | Multi-edition interop: `support-status.md:93-94` lists Iron and Jazzy as "Supported", and no workflow runs `just ros_editions ci`. Do we add a lane or lower the claim? | (a) A weekly scheduled workflow (with dispatch) on a hosted runner, running `just ros_editions ci jazzy` (and `iron`). (b) Lower the claim to "codegen-checked" (the committed `fixtures/ros-editions/` golden files). (c) Leave it as it is. | `git grep ros_editions .github` finds nothing. The harness and its five test files exist (`tests/ros_editions_*.rs`). The image build is apt only, "a few minutes" (`just/ros-editions.just:10-11`). RFC-0122 already lists this as an open question. | **(a)**, and **(b)** until (a) is green. This is RFC-0122 D9's rule ("a promise with no job is lowered"), applied to the edition axis. (c) leaves a claim with nothing behind it. |

### What a ruling unblocks

- Items 1, 2 and 4: an RFC-0061 amendment, the status flips, and the
  `support-status.md` tier-1 row, as one change.
- Item 3: the registry field, `ci_lane.rs`'s pool check and the cadence
  lowering, as one change, owned jointly with the platform-board-rtos slice.
- Item 5: a new workflow, or a one-line book change.
- In phase-555 W3: write `amends: [0051]` on RFC-0114 (and the reverse link),
  and `amends: [0026, 0051, 0061, 0066, 0114, 0121]` is already on RFC-0122.
  Add the reverse `amended-by` mentions if W1's vocabulary gains that field.
