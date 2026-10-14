---
id: 1886
title: "`just check build` refuses one of `native-check` / `source-gates`: every jobserver pool ran under ONE subtree-guard name, so two pools in one lane fan-out collided although their artifacts are disjoint"
status: resolved
type: bug
severity: high
area: [build, gates]
related: [1882, 0762, 1157, 0993, 1403]
found: 2026-10-14
resolved: 2026-10-14
resolved_in: "branch fix/1886-pool-guard-siblings (this commit)"
---

## Measured

`just check build` (step 4 of `just ci gate`) runs the `build-serial` gates
through `scripts/build/run-gates-parallel.sh`, which fans them out with
`xargs -P`. Two of them run a jobserver pool:

- `native-check` -> `just native check` -> `nros_pool_run check-examples`
- `source-gates` -> `scripts/build/compile-check-fixtures.sh` ->
  `nros_pool_run compile-check`

`jobserver-pool.sh` wrapped every pool in `nros_guard_exec "jobserver-pool"`, a
constant. `subtree-guard.sh` keeps one lock per (tree, name) and refuses while
the lock's launcher is alive. A trace of the guard during `just check build`
shows both pools reaching it 0.42 s apart:

```
1791944605.551748355  native-check  jobserver-pool-check-examples
1791944605.971599450  source-gates  jobserver-pool-compile-check
```

Before the fix, the same two gates through the runner on their own
(`NROS_GATE_LANE=build-serial run-gates-parallel.sh <list of the two>`) gave
rc=1:

```
===== FAIL (native-check, rc=1, 1712ms) =====
subtree-guard: a 'jobserver-pool' build is already running (pgid 673898, 61 process(es)).
check-build (parallel): 1 of 2 gate(s) FAILED
```

Each gate passed on its own. #1882 (1f07376670, 12bc8bb8fc) put
`native-check` on `build-serial`, and that is what made the pair co-occur.

## Do the two pools share artifacts? No, measured

Each pool was run alone after a marker file. The list is every file newer than
the marker in the worktree (`.git` excluded). `compile-check` ran with
`NROS_COMPILE_CHECK_REBUILD=1` and `source-gates`' own lanes
(`cxx-syntax,cxx-syntax-verdict,cxx-compile-verdict`), so that every row
really wrote:

| pool | writes |
| --- | --- |
| `check-examples` (`native-check`) | `build/example-lint/<leaf>/` (12,230 files), untracked leaf `Cargo.lock`s under `examples/native/rust/*`; the `nros sync` output (`<leaf>/build/`, `.cargo/`, `generated/`) of the 27 board-image leaves it lints (`examples/mps2-an385-baremetal/**`, `examples/esp32-c3-baremetal/rust/{talker,listener}`) |
| `compile-check` (`source-gates`) | `build/compile-check-fixtures/<id>/` (87 files), `target/.rustc_info.json`, `target/debug/libnros_{c,cpp}.d` |

The intersection is empty (`comm -12`). Both also write `build/jobserver-pool/`,
but under `<label>-<pid>.mk`, so the names are disjoint by construction.
`compile-check`'s C++ lanes run no `nros sync` and read only `packages/*`
headers and the generated config headers. `check-examples` writes nothing under
`target/` (its cargo runs use `--target-dir build/example-lint/<leaf>`).

So the refusal protected nothing. The lock's SCOPE was wrong: it was keyed on
"some pool is running in this tree", while what it protects is one pool's
artifact set.

## Fix

`jobserver-pool.sh` now guards each pool under `nros_pool_guard_name <label>`,
which gives `jobserver-pool-<label>`. That is one lock per artifact set:

- Two pools with DIFFERENT labels in one tree run concurrently. This is the
  1886 case.
- A pool whose label is already held by a live launcher still REFUSES. A second
  `just native check`, or any foreign build of the same set, is what the guard
  exists for (issue 0762).
- The lock format is unchanged (`<launcher> <pgid> <tree>`), and only the file
  name changes. `runner-sweep.sh` enumerates `*/*.pgid` and reads fields 1 and
  2, and `nros_guard_reap_lock` reads fields 1 to 3. Both are untouched.

The other cross-label pairs were also checked for shared writes, so that
narrowing does not drop a refusal that was protecting something:

- `check-examples` vs `build-all-extras`: the leaves `check-examples` syncs
  (mps2-an385-baremetal, esp32-c3-baremetal) are exactly the platforms
  `build-all-extras` excludes, and their target dirs differ
  (`build/example-lint/` vs `nros_example_build_target_dir`).
- `px4-companion` builds into `examples/px4/**/target-xrce`. `check-examples`
  excludes `/px4/`.
- `no-std` runs `cargo check` in the workspace `target/`, which cargo's own
  build-directory lock serialises.

## Class sweep

The guard names are `jobserver-pool-<label>` (five labels: `check-examples`,
`compile-check`, `no-std`, `px4-companion`, `build-all-extras`), `fixtures`,
`workspace-fixtures` and `native-examples`. Every gate that reached
`nros_guard_exec` was recorded by a temporary trace in the guard (the caller's
`just check <gate>` ancestor), over full runs:

- **`build-serial`** (26 gates: borrowed-e2e, c, census-hooks-complete,
  cli-tests, cpp, cpp-ffi-tests, dist-runtime-deps,
  embedded-feature-unification, excluded-crate-tests, launch-resolve-builds,
  mem-report, native-check, node-std-tests, nros-log-riscv32,
  required-features-tests, rmw-cyclonedds, rmw-uorb, rmw-xrce, scaffold-builds,
  sched-dim-arms, source-gates, staticlib-symbols, template-copy-out,
  test-targets, workspace-all, workspace-features): only `native-check`
  (`check-examples`) and `source-gates` (`compile-check`) reach a guard.
  `borrowed-e2e` runs `compile-check-fixtures.sh` with `NROS_FIXTURE_ID` (one
  row, no pool, so no guard).
- **fast lane** (447 gates, `check-gate-lists.py --list fast-serial`, traced
  `just check fast` rc=0): only `subtree-guard` reaches the guard, and it does
  so under its own `NROS_GUARD_LOCK_DIR` tempdir (`guardtest`,
  `jobserver-pool-sibling-{a,b}`), so it never shares a lock with a real
  build.
- **`ci gate`'s other steps** (`cli-fresh`, `launch-resolve-fresh`, `fast`,
  `build`, `test-unit`, `test-lane-contracts`) run SERIALLY through
  `run-lane-steps.sh`. `test-lane-contracts`' `compile-check` pool never
  overlaps `source-gates`' pool.
- `no-std` is lane-exempt and runs after `check fast` in `just ci`, as a just
  dependency (serial).

No two gates in one fan-out now reach the same guard name. If two ever do, the
guard refuses loudly, and that is the right default for two runs over one
artifact set.

## Regression gate

`packages/testing/nros-tests/tests/subtree_guard.sh` (gate `subtree-guard`,
fast lane) has a fifth path. It drives the real `nros_pool_run`, with pinned
make replaced by a stub that holds the guard for 8 s:

- a pool labelled `sibling-b` must START while `sibling-a` holds its guard;
- a second `sibling-a` must REFUSE with the guard's message.

Mutation: with the call reverted to `nros_guard_exec "jobserver-pool"`, the
gate gives rc=1 (`FAIL: a pool with a DIFFERENT label was refused while
'sibling-a' ran — the issue 1886 collision`). With the fix, rc=0.

## Verification

- Pair harness (the two gates through `run-gates-parallel.sh`,
  `NROS_GATE_LANE=build-serial`): before the fix rc=1 with the refusal above.
  After the fix rc=0, with `busy 29s over wall 14s`, so the two pools really
  overlapped.
- `just check build`: rc=0 (21 ran, 5 skipped for provisioning reasons).
- Foreign refusal is kept. With a live outside process recorded in
  `jobserver-pool-compile-check.pgid`, `just check source-gates` gives rc=1:
  `subtree-guard: a 'jobserver-pool-compile-check' build is already running`.
- `just ci gate`: steps 1-4 were green. `test-unit` failed only
  `client_session_with_absent_locator_dials_backend_default`, while port 7447
  was held by another session's `rmw_zenohd`. Once the port was free,
  `just test-unit` ran on its own and gave rc=0 (1983 passed).
  `just test-lane-contracts` gave rc=0.

## Not fixed here

On a fresh worktree, `native-check` fails ON ITS OWN until every example leaf
it lints has been through `nros sync`. The 21 `examples/native/rust/*` leaves
that depend on `<leaf>_nros_selection` need `build/nros/selection`, and
`examples/native/rust/logging` needs the `.cargo/config.toml` that patches
`nros-log`. `lint-example-leaf.sh` syncs only NON-native leaves (#1882), and
the recipe's precondition message points at `just generate-bindings`, which
does neither. This was provisioned by hand (`nros sync` per leaf) to measure
this issue. It is a separate defect, which needs its own issue id.
