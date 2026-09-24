---
id: 1479
title: "Issue 1435's gate compares two keys of one board ZST, so it is blind to the same defect with one key — and `lane=all` reported it as a multihost-entry failure"
status: resolved
type: bug
area: codegen, build, testing
severity: low
found: 2026-09-24
resolved_in: "issue-1479 (2026-09-24)"
related: [1435, 1449, 1381, 1285, 0415, 0196, phase-445, phase-456]
---

## What

A `just build-test-fixtures lane=all` run on 2026-09-23 died in its **zephyr**
module — which `lane=all` builds first, so the one failure starved the whole
fixture corpus:

```
error[E0277]: the trait bound `ZephyrBoard: nros::__macro_support::nros_platform::board::entry::BoardEntry` is not satisfied
error: could not compile `zephyr_entry_robot1` (lib) due to 1 previous error
```

It was reported as a failure of the **multihost** workspace entry
(`examples/workspaces/rust`, image `zephyr_robot1`). It is not: the same log
carries **five** failed targets and they are **two** defects, neither of which
has anything to do with multihost.

| fixture | first error | defect |
| --- | --- | --- |
| 64 `ws-rs-mh-robot1-entry-zenoh` | `ZephyrBoard: BoardEntry` E0277 | issue 1435 |
| 59 `ws-rs-entry-zenoh` | `ZephyrBoard: BoardEntry` E0277 | issue 1435 |
| 56 `cortex-m-cpp-talker-zenoh` | `no method named wake_raw_ptr` E0599 | issue 1449 |
| 55 `cortex-m-c-talker-zenoh` | `no method named wake_raw_ptr` E0599 | issue 1449 |
| 68 `ws-c-realtime-entry-smp` | `no method named wake_raw_ptr` E0599 | issue 1449 |

Fixture 59 is the SINGLE-host Rust workspace entry. It fails identically. The
multihost entry is not the variable; the board key is.

## Why

`examples/workspaces/rust/src/demo_bringup/system.toml` gives both Zephyr
images `board = "native_sim/native/64"` — the zephyr descriptor's second name
(`packages/boards/zephyr/nros-board.toml`: `names = ["zephyr",
"native_sim/native/64"]`). phase-445 W5 added that spelling to
`nros_orchestration_ir::BOARD_PATHS` and not to `framework_for_board_key`, so
it resolved to `None`, which every consumer reads as `owned-spin`, which makes
`nros::main!` emit `<ZephyrBoard as BoardEntry>::run` — a trait
`nros-board-zephyr` does not implement, because Zephyr owns `main` and the
macro's `Framework::Zephyr` arm emits a `rust_main` staticlib export instead.
That is issue **1435**, exactly.

## Both defects were already fixed before this was triaged

Not asserted — measured, three independent ways.

- `8fc6d0019 fix(#1435)` (2026-09-21) adds the `native_sim/native/64` arm.
  `469a50b53 fix(#1449)` (2026-09-22) gates the wake-handle body on `alloc`.
  Both are ancestors of `work/phase-456-rebased`; **neither** is an ancestor of
  `backup/phase-456-prerebase` (`c0b994dbe`, 2026-09-23 14:40:56 +0800).
- The build log's directory is `build-test-fixtures-20260923-144216-312535` —
  **80 seconds** after that pre-rebase tip was committed.
- The log compiles `ros-launch-manifest-model … tag=v0.1.35`. The current tree
  pins **v0.1.40**, moved by `e4b12c040` on 2026-09-23 17:55 UTC, i.e. *after*
  the build. A tree-independent fingerprint, and it agrees.

So the build ran against the pre-rebase tree, and `4bddece8c`'s rebase onto
main's 362 commits carried both fixes in. This is CLAUDE.md's "a test result is
only about the tree its FIXTURES were built from" (issues 0859–0862) reaching
a *build* rather than a run. The phase-456 diff touches no board, entry or
platform file; it did not cause either failure.

## What was actually wrong: the gate's reach

Issue 1435 shipped a gate,
`nros_orchestration_ir`'s `every_key_of_one_board_zst_wants_one_framework`. It
groups `BOARD_PATHS` by board ZST and asserts the keys of one ZST agree on a
framework. That is **agreement, not correctness** — issue 0196's shape, a gate
whose reach is narrower than the rule it enforces. It cannot see:

- a ZST named by exactly **one** key (`threadx-linux` today): nothing to
  compare against, so any value passes;
- **both** keys of a shared ZST wrong the same way. Had phase-445 W5 *renamed*
  the zephyr key instead of adding a second spelling, the table would have had
  one key, `None`, `owned-spin` — and the gate would have been green on a
  table that cannot compile a Zephyr entry.

Measured. With the `"zephyr" | "native_sim/native/64" => "zephyr"` arm deleted
outright, `every_key_of_one_board_zst_wants_one_framework` **passes**.

## Fix

A sibling of issue 1381's
`the_links_std_column_agrees_with_the_descriptors_entry_kind` in
`packages/cli/nros-cli-core/tests/board_key_table.rs`, over the same catalog
and the same key set:
**`the_framework_agrees_with_the_descriptors_entry_kind`**.

`entry_kind` in `nros-board.toml` is an *independent* authority on the entry
shape — authored per board, not derived from the key — so it answers both
cases the agreement gate cannot. The assertion is a biconditional on `zephyr`
alone (`entry_kind = "zephyr-staticlib"` ⟺ framework `zephyr`) and deliberately
claims nothing more: `board-run` covers `owned-spin`, `rtic`, `embassy` and
`esp32` at once, so it does not name a framework and a test pretending it did
would assert something the data does not hold. A missing row is resolved the
way consumers resolve it (`unwrap_or("owned-spin")`) rather than skipped —
a missing row is the defect. Two meaningfulness floors, because a scan that
matched no key, or no *zephyr* key, would pass having checked nothing.

It runs on `check-cli-tests` (`cargo test --manifest-path packages/cli/Cargo.toml
--workspace`), which is in the required PR `CI` context — so it gates a merge,
not just a nightly (issue 1226's shape).

### Negative controls, both confirmed failing before the change

| mutation to `framework_for_board_key` | 1435's gate | this gate |
| --- | --- | --- |
| drop the `native_sim/native/64` arm (the pre-1435 table, i.e. the exact tree that produced the E0277) | fails | **fails**, naming the key and the remedy |
| delete the zephyr arm entirely (the one-key / both-wrong case) | **passes** | **fails** |

## Verification

The gate:

```
cd packages/cli && cargo test --locked --offline -p nros-cli-core --test board_key_table
```

9 passed. The build this issue is about cannot be run from an agent worktree
(no submodules, no west workspace, no Zephyr SDK); in the main checkout it is

```
source ./activate.sh
just build-test-fixtures lane=all
```

and the two entries to watch are `zephyr-fixture-59-build-ws-rs-entry-zenoh`
and `zephyr-fixture-64-build-ws-rs-mh-robot1-entry-zenoh`.

## What this issue does not answer

Whether `lane=all` is now green *end to end*. Five targets failed in that run
and all five reduce to 1435 and 1449, both fixed — but the failures were
concurrent and `make` stopped scheduling after them, so any defect in the
targets that never started is unobserved, not absent. The next `lane=all` is
the measurement.
