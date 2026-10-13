---
id: 1865
title: "`just qemu test-rtic-main-e2e` could not pass: no build path, a router on a port the image never dials, and an RTIC dispatch that reaches no component"
status: resolved
resolved: 2026-10-13
type: bug
severity: high
area: [testing, rmw, build]
related: [1305, 1581, 0488, 1025, phase-289, phase-216, phase-295]
found: 2026-10-13
---

## Reported

From issue 1305 phase F, red before and after that change: the image baked
`tcp/10.0.2.2:7450` while the recipe's router listened on 10200. With a router
on 7450, neither the old image nor the new one exited within 60 s.

## Measured: four faults, each hidden by the one before

1. **No build path.** `build-rtic-main-e2e` ran a bare `cargo build` in the
   leaf. That could not have worked since phase-445 W6 / issue 1305: the leaf
   has no `.cargo/`, and it depends on `build/nros/selection`, which only
   `nros sync` writes. It failed with `failed to read …/build/nros/selection/Cargo.toml`
   before compiling anything. This is a second build path, the 0488 shape.
2. **Port carrier mismatch.** The leaf's `system.toml` stated no locator, so the
   image baked the board default (`nros-board-mps2-an385` `Config`,
   `tcp/10.0.2.2:7450`). The recipe hardcoded 10200, a literal it was given in
   295-W4 (`733dfd9edd`) without the image being given one. The gate that
   exists for exactly this (`check-image-locator-bake`, issue 1581) exempted
   the row in `NO_ROUTER`, on the claim that "baremetal_run_plan_runtime.rs
   runs it with no zenohd". That test runs only `qemu-baremetal-main-e2e`; this
   row's sole consumer is the recipe, which starts a router.
3. **The callback the image exits on reaches no component.** With a router on
   the right port, the session opens and the image spins forever.
   `gdb` on the live QEMU showed `ExecutorNodeRuntime::dispatch_callback`
   reached exactly once, with its loop over `self.components` EMPTY.
   phase-289 (`c2227f5270`) moved the RTIC entry onto `<pkg>::register(&mut RuntimeCtx)`.
   That seam (`install_node_typed*` → `register_node_borrowed`) ENROLLS a
   component in the executor and never pushes it to the runtime's
   `components`. `dispatch_callback` scanned `components` alone. So every
   signaled callback the RTIC/Embassy dispatch task forwarded, including this
   image's `__nros_e2e` exit, was dropped. The two populations are disjoint by
   construction. The stats fold (`observed_callback_counts`, issue #140) had
   already learned to read both; the dispatch had not.
4. **A hang read as a PASS.** `tests/run-test.sh` had no deadline, so the
   recipe waited forever. Killed by a signal, QEMU exits 0, so killing it by
   hand printed `[PASS] qemu-rtic-main-e2e 0 passed, 0 failed (1511s)`.

## Fix (each at the class)

- `ExecutorNodeRuntime::dispatch_callback` also walks the executor's enrolled
  components (`enrolled_component_states`), through `CellPrefix`, the
  non-generic `repr(C)` prefix (header + slot) of every `ComponentCell<..>`.
  Its offsets are asserted at compile time against two monomorphizations.
- The image's `system.toml` states `locator = "tcp/10.0.2.2:10530"`, a new
  allocator aux slot, `alloc::BAREMETAL_RTIC_RUN_PLAN_PORT`. It joins the
  injectivity test, so it cannot collide with the 10200 RTIC pair.
- **One reader for the router port:** `scripts/lib/image_locator.py` reads
  `[image.<id>] locator` over `[image_defaults]` over `[system]`, the overlay
  the build bakes. The recipe takes its listen port from it and fails, rather
  than falling back, when the image states none. `check-image-locator-bake`
  resolves image locators through the same function.
  `nros_tests::fixtures::baked_locator` now also reads single-package leaf
  rows, and pins the leaf's stated port to the allocator constant.
- `check-image-locator-bake` **rule 5**: a `just` recipe that starts a router
  and runs a fixture row's artifact must take the port from `image_locator.py`,
  not a literal, and must not run a `NO_ROUTER` row. The false `NO_ROUTER`
  entry is gone, and the surviving entry names the test that really runs it.
  The rule has self-test negative controls.
- `build-rtic-main-e2e` builds the ROW (`fixtures-build.sh baremetal rust --id baremetal-rtic-run-plan-e2e`,
  a new row id). The test side finds the artifact through
  `nros_fixture_row_artifact_dir_by_id` (issue 1025's rule).
- `tests/run-test.sh` takes `--timeout`, defaulting to 120 s for `--qemu`,
  through the one deadline spelling (`scripts/lib/deadline.sh`). A deadline
  status is a FAIL that says so. The recipe passes `--timeout 60`; the image
  exits about 5 s after boot.

## Verified (2026-10-13, worktree on `7d41ce9a89`, solo)

- `just qemu test-rtic-main-e2e` gives PASS, then PASS again (rc=0 twice). The
  image's strings carry `tcp/10.0.2.2:10530`.
- **Mutation, dispatch:** `enrolled_component_states().take(0)` makes the recipe
  fail with `did not exit within 60s (deadline, rc=124)`. Before the
  `run-test.sh` deadline, the same mutation hung for 1511 s and printed PASS
  when killed. `component_dispatch::signaled_callback_reaches_an_installed_component`
  goes 3/3 PASS with the fix and FAIL with the mutation (rc=100).
- **Mutation, port:** restoring the literal `tcp/127.0.0.1:10200` in the recipe
  fails the recipe (`RMW session open failed — ConnectionFailed`, rc=1), and
  `check-image-locator-bake` fails with rule 5 (rc=1, rc=0 restored).
- `nros-tests --lib` `alloc::*` and `baked_locator::*`: 7/7 PASS.
