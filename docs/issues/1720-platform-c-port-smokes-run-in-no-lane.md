---
id: 1720
title: "The platform C-port smokes (`just {threadx_linux,freertos,zephyr} test-c-port`) run in NO lane — the only runtime test of ThreadX's `nros_platform_alloc` saying NO is reachable by hand alone"
status: open
type: tech-debt
severity: low
area: [testing, ci, platform]
related: [1717, 1040, 1226]
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
