---
id: 1521
title: "The example-portability ratchet reached zero, then took 11 regressions
  that no merge-gating lane could report"
status: open
type: bug
area: examples, ci, testing
severity: medium
found: 2026-09-28
related: [1226, 1509, 1512, phase-338, phase-437, phase-470]
---

## What this is

`nros-tests::example_portability copies_within_a_group_are_identical` **fails on
`main` today**, with 11 divergences:

```
rust/listener       [A-scheduled]: mps2-an385-freertos differs from esp32-c3-baremetal
rust/listener       [A-scheduled]: native              differs from esp32-c3-baremetal
rust/listener       [A-scheduled]: qemu-armv7a-nuttx   differs from esp32-c3-baremetal
rust/listener       [A-scheduled]: rv-virt-threadx     differs from esp32-c3-baremetal
rust/listener       [A-scheduled]: threadx-linux       differs from esp32-c3-baremetal
rust/service-client [A-scheduled]: native              differs from mps2-an385-freertos
rust/talker         [A-scheduled]: mps2-an385-freertos differs from esp32-c3-baremetal
rust/talker         [A-scheduled]: native              differs from esp32-c3-baremetal
rust/talker         [A-scheduled]: qemu-armv7a-nuttx   differs from esp32-c3-baremetal
rust/talker         [A-scheduled]: rv-virt-threadx     differs from esp32-c3-baremetal
rust/talker         [A-scheduled]: threadx-linux       differs from esp32-c3-baremetal
```

**It was green, and this is measured rather than inferred.** `KNOWN_DIVERGENCE`
is an empty list whose comment reads *"Baseline recorded 2026-08-04 by walking
the tree; every entry names the wave that removes it"* — the phase-338 ratchet
counted 31 → 28 → 22 → 18 → 6 → 4 → 0. The commit that removed the last entry is
`fix(#449): the examples follow the ROS demos — delete the NROS_SUB_TYPE switch`,
2026-08-06. Running the test in a worktree at that commit:

```
PASS [0.008s] (1/1) nros-tests::example_portability copies_within_a_group_are_identical
Summary [0.019s] 1 test run: 1 passed, 1474 skipped
```

So the ratchet genuinely reached zero and these 11 are **regressions that landed
over the following seven weeks**, unreported.

## Why nothing reported them — the part worth fixing

**No merge-gating lane runs this test.**

- `just test-unit` — the lane `just ci gate` uses and the one `merge_group` runs
  — passes `--workspace --exclude nros-tests`. The test lives in
  `packages/testing/nros-tests/tests/example_portability.rs`, i.e. in exactly the
  excluded crate.
- The required `CI` context on a `pull_request` is `check-fast` +
  `check-submodule-commits-reachable` + `check-compile-smoke` + `check-cli-tests`
  + `check-workspace-all`. None of them reads this test.
- No `just` recipe and no workflow names `example_portability` anywhere.
- The only workflow that reaches it, `.github/workflows/host-tests.yml`, fires on
  `push` / `schedule` / `workflow_dispatch` — i.e. **after** the merge, never
  before it. Of its last 8 push runs on `main`: 2 `failure`, 6 `cancelled`
  (superseded by the next merge, which arrives faster than the lane finishes).

That combination is both of this repo's documented signal-loss shapes at once:
issue 1226's *"a gate that WORKS is not a gate that RUNS"*, and CLAUDE.md's *"a
red CI lane answers one of two questions and they look identical"* — a lane that
is perpetually red-or-cancelled has no capacity to report an twelfth divergence
differently from the eleventh.

**Note the test needs no fixtures.** It reads `examples/**/src/` and finishes in
0.008 s. It is a source invariant wearing a test's clothes, parked in the crate
whose lane requires a fixture build — which is the whole reason no affordable
lane runs it.

## The 11, by cause

**Ten of eleven: a node's logic file names its board crate.**
`examples/esp32-c3-baremetal/rust/{talker,listener}/src/lib.rs` log through

```rust
nros_board_esp32_qemu::nros_log::log_info!(
    nros_board_esp32_qemu::nros_log::get_logger("talker"),
    "Publishing: '{}'", msg.data
);
```

where every other platform's copy of the same node is

```rust
log::info!("Publishing: '{}'", msg.data);
```

This is also an **issue 1509 violation one level up** — a node package is
supposed to document what it DOES, not which platform carries it, and here the
platform is not in a comment but in the code. The crate name is additionally
stale: `nros_board_esp32_qemu` outlived the phase-437 W6 rename of
`examples/qemu-esp32-baremetal` → `examples/esp32-c3-baremetal`.

**One of eleven: an unconverged wait loop.**
`examples/native/rust/service-client/src/lib.rs` has

```rust
if let Ok(false) = ctx.service_is_ready_for_name("/add_two_ints") {
    log::info!("service not available, waiting again...");
    return;
}
```

which `examples/mps2-an385-freertos/rust/service-client` does not. That is a real
behavioural difference, not a spelling one, and it is the phase-338 W3 work the
test's own doc-comment predicts ("un-split packages are compared whole, and their
divergence shows up as the W3 work it is").

## What to do

Three things, and the third is the one that matters:

1. **Converge the ten.** The esp32 leaves should use the same `log::info!` their
   siblings use, or the group's baseline should be the one that is right and the
   others should move — decide by which shape the C/C++ copies and the book
   teach, not by majority.
2. **Resolve the service-client difference**: converge it, or record it as a
   `KNOWN_DIVERGENCE` entry naming the wave that removes it. The list's contract
   is that silence is not an option; it says so in the failure text.
3. **Give the test a lane that gates.** It is buildless and 0.008 s, so the
   obvious home is the fast line as a `check-*` gate rather than a test inside
   the fixture-bound crate — but that is a decision, not a foregone conclusion,
   and whoever takes it should check `check-lane-contracts` (an affordability
   tier may only resolve artifacts the lane itself builds; this one resolves
   none) and `check-default-gates-run-somewhere`. **Without step 3, steps 1 and 2
   are undone by the next unwatched merge**, which is what the last seven weeks
   measured.

A fourth, separable: `host-tests.yml` being red-or-cancelled on nearly every push
is its own signal-loss problem and is not diagnosed here. Its failing job is
`nros-tests integration (host)`; this issue does not establish that the
portability test is why it fails, only that the lane cannot be trusted to say.

## A second instance, one day later (2026-09-28)

This issue argued the class from one example. A second arrived within a day of
filing, which settles whether the class is worth a lane.

`nros-tests::example_shape zephyr_leaf_buildrs_uses_shared_bake` is **red on
`main`**:

```
thread 'zephyr_leaf_buildrs_uses_shared_bake' panicked at
packages/testing/nros-tests/tests/example_shape.rs:1040:5:
expected >=13 zephyr rust leaf build.rs, walked only 12 — layout moved?
```

Cause: phase-470 W5.a deleted one hand-written Zephyr entry package (the
migration that item exists to do) and the test's floor stayed at 13. Nothing
wrong with the test — a floor is exactly how this repo keeps a walk from going
vacuous, and it did its job. What failed is that **nobody heard it**: it lives in
`nros-tests`, `test-unit` passes `--exclude nros-tests`, so `just ci gate` was
honestly green on the PR that broke it and stayed green on `main` afterwards.

Two things this adds to the argument above:

- **The interval is not the point.** The first instance took seven weeks to
  notice; this one took a day, and only because the next agent in the same
  campaign happened to run the excluded crate's tests by hand. Neither was
  reported by a lane. A class that is invisible reports at the speed of whoever
  stumbles over it, which is not a schedule.
- **The two failures are opposite in kind, and that matters for the fix.** The
  first is a source invariant drifting (11 divergences). This one is a
  **deliberate, correct change** tripping a floor that had to move with it. A
  lane that gates would have made this a one-line edit inside the PR that caused
  it, instead of a red on `main` for a day. That is the ordinary case for these
  tests, not the exceptional one.

Fixed in phase-470 W5.b1 (floor lowered to 9, accounting for all four deletions,
with the reason written in). The class is not fixed, which is what acceptance
below is about.

## Acceptance

- The test passes on `main`, with `KNOWN_DIVERGENCE` carrying only entries that
  name the wave that removes them.
- A lane that runs on `pull_request` or `merge_group` fails when a twelfth
  divergence is introduced — demonstrated by introducing one, not by reading the
  lane list.
