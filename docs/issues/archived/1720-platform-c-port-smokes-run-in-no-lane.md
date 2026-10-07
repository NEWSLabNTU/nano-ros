---
id: 1720
title: "The platform C-port smokes (`just {threadx_linux,freertos,zephyr} test-c-port`) run in NO lane — the only runtime test of ThreadX's `nros_platform_alloc` saying NO is reachable by hand alone"
status: resolved
type: tech-debt
severity: low
area: [testing, ci, platform]
related: [1717, 1040, 1226]
resolved_in: "branch issue-1720-c-port-smokes"
found: 2026-10-06
---

## What

`tests/threadx-c-smoke/`, `tests/freertos-c-smoke/` and `tests/zephyr-c-smoke/`
boot the REAL kernel (ThreadX linux port, FreeRTOS POSIX port, Zephyr
native_sim) over the port's `platform.c` and probe clock/alloc/sleep/mutex/
timer. They are reached only by `test-c-port` recipes in `[group("debug")]`
(`just/threadx-linux.just`, `just/freertos.just`, `just/zephyr-dev.just`), and
`git grep test-c-port -- .github just justfile scripts` finds no caller: no
workflow, no `ci` recipe, no gate.

Issue 1717 is what that costs. Its regression proof — the ThreadX smoke now
exhausts its 256 KiB pool in 16 KiB chunks and requires `nros_platform_alloc`
to return NULL (before the fix the 14th request hung until `timeout 15`
killed it, rc 124; after, NULL in 0.25 s) — lives in the one place nothing
runs. The fast-line `check-allocator-never-waits` holds the SPELLING; only the
smoke holds the BEHAVIOUR.

## Why it is not simply added to a lane

* The ThreadX kernel is a submodule the gate tier does not check out
  (`just setup-worktree` omits it), so it cannot go on the `ci gate` line.
* "No compilation inside tests" rules out a nros-tests test that runs cmake;
  the build belongs in `build-test-fixtures`, which means a `fixtures.toml`
  row — and every row needs a `matrix::CELLS` cell
  (`fixture_rows_all_modeled_by_matrix`), whose axes (platform × lang × rmw ×
  kind) do not describe an RMW-less port smoke.

## Fix direction

Either a fixture row + cell kind for "port smoke" (RMW-less) so the tier-1
`threadx-linux` / `freertos` lanes build and run them, or have each platform's
`ci` recipe call its `test-c-port` (cheap: the ThreadX smoke builds in seconds
and runs in 0.25 s). Same "a gate that works is not a gate that runs" shape as
issues 1040/1226.

## Resolution

Each smoke now runs in the cheapest lane that already provisions its
precondition. Each runs as its OWN step with `!cancelled()`, so an earlier red
does not withhold its verdict, and with `NROS_CHECK_SKIP_STRICT=1`: the lane
provides the precondition, so a missing one is a failure there.

| smoke | lane | event |
| --- | --- | --- |
| `just threadx_linux test-c-port` | `nightly.yml` `platform` job, `threadx_linux` cell (after `just setup threadx_linux` checks the kernel out) | `schedule` 07:00, `workflow_dispatch` |
| `just freertos test-c-port` (heap_4 + heap_3) | `nightly.yml` `platform` job, `freertos` cell | `schedule` 07:00, `workflow_dispatch` |
| `just zephyr test-c-port` | `nightly.yml` `zephyr-dual-line-summary` (3.7 workspace) | `schedule` 05:00, `workflow_dispatch` |

They are not in the gate tier, for two reasons:

* None of those kernels or workspaces is checked out there
  (`just setup-worktree`).
* `check-lane-contracts` forbids an affordability lane from resolving what its
  job does not provide.

There is no fixture row either. The smokes have no RMW, so a `matrix::CELLS`
coordinate cannot describe them.

**A missing precondition is a reported skip.** Each `test-c-port` checks its
kernel or west workspace first. If it is missing, the recipe calls
`nros_check_unverified <platform>-c-port-smoke "<what is missing> — <remedy>"`.
That records the skip in the ledger, or FAILS under
`NROS_CHECK_SKIP_STRICT=1`. Both outcomes were verified by hiding
`third-party/freertos/kernel/include`.

**The gate.** `check-default-gates-run-somewhere` has a third R1/R2 scope.
The recipes in scope are every `<module>::test-c-port`, read from
`just --summary`. Reading `just`'s own list means an `import`ed recipe like
`zephyr::test-c-port` counts, and a new platform's smoke joins automatically.
Each one must:

* be reached by a literal `just <module> test-c-port` in some workflow `run:`
  block;
* not be sequenced behind another `just` command in that block.

A `${{ matrix.plat }}` template credits nothing.

The self-test plants the negative controls on every run:

* a literal placement is credited;
* a templated one is refused;
* an unplaced smoke is reported;
* a shadowed one is detected.

Against the pre-change `nightly.yml`, the gate reports all three smokes as
running in NO workflow.

**What running them found.** Two of the three smokes were broken on `main`,
which is what this issue said their absence costs:

* **The FreeRTOS smoke did not link.** It forced heap_3 without telling the
  port, so `xPortGetFreeHeapSize` was undefined. Fixed with issue 1719, which
  also builds the smoke once per heap.
* **The Zephyr smoke did not link either.** phase-391 W3 moved
  `nros_platform_alloc` onto the Rust rlsf arena (`nros_zephyr_heap_*`), and
  this C-only build has no Rust half. `tests/zephyr-c-smoke/src/heap_stand_in.c`
  supplies those symbols over a `k_heap`, so the smoke again tests the PORT's
  `platform.c` funnel. The arena itself is tested on the Rust side.

All three PASS locally, each including issue 1719's realloc probe. The Zephyr
smoke ran on a 3.7 workspace, native_sim/native/64.
