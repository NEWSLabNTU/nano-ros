---
id: 1888
title: "`native-check` on `build-serial` makes `just ci gate` red on a host that has not built fixtures: it needs bindings and synced native leaves, and once it collided with a jobserver pool already holding the tree"
status: open
type: bug
severity: medium
area: [build, ci, testing]
related: [1882, 0952, 1881]
found: 2026-10-14
---

## Measured

Issue 1882's fix (`1f07376670`, `12bc8bb8fc`) made `native-check` a gate on
`build-serial`. `just ci gate` runs that list in step 4 (`check::build`), so
the local push tier now runs the native-example clippy fan-out. CLAUDE.md
says that tier needs "no fixture build, no SDK, no QEMU and no cross
toolchain". On this host (main checkout, branch based on `00e67d0079`) it
went red three ways, one after the other.

1. **Missing bindings.** The first `just ci gate` failed at
   `native-check` (rc=1, 1950 ms):

   ```
   native check: 5 example(s) are missing their generated message bindings:
     examples/mps2-an385-baremetal/cpp/{action-client,action-server,listener,service-client,service-server}
   Run 'just generate-bindings' (or 'just build-test-fixtures') first
   ```

   The CI `check-build` job runs `just generate-bindings` before
   `just check build`. `just ci gate` has no such step.

2. **The subtree guard.** After `just generate-bindings` (rc=0), the second
   `just ci gate` failed at `native-check` again (rc=1, 2335 ms into
   `check::build`):

   ```
   subtree-guard: a 'jobserver-pool' build is already running (pgid 2070739, 167 process(es)).
     Holding tree: /home/aeon/repos/nano-ros
   ```

   The guard key is `<tree>/jobserver-pool`, one per checkout, whatever the
   pool's label. So two pool users in one tree cannot run at the same time.
   On `build-serial`, `source-gates` and `borrowed-e2e` both run
   `compile-check-fixtures.sh`, which can take the pool
   (`nros_pool_run compile-check`). That makes them the likely holder. The
   group had exited before it could be inspected, so this is **not
   confirmed**.

3. **Unsynced native leaves.** Run alone, `just check native-check` failed
   with rc=2 after 116 s. 20 of the 22 `examples/native/rust/*` leaves fail
   at `cargo metadata`:

   ```
   failed to load manifest for dependency `native_rs_lifecycle_node_nros_selection`
     failed to read `examples/native/rust/lifecycle-node/build/nros/selection/Cargo.toml`
   ```

   `scripts/build/lint-example-leaf.sh` runs `nros sync` only when
   `platform != native`. A native leaf's `build/nros/selection/` is written
   by that same sync, so the lint works only where an earlier native fixture
   build left it behind. The 1882 measurement ("green, 20m53s cold in the
   ros2 box") was on such a host.

## Effect

`just ci gate` is the tier CLAUDE.md tells every contributor to run before
every push. On a host that has not run `just generate-bindings` and a native
fixture build in this checkout, it is now red for reasons unrelated to the
change being pushed. The nightly `check / build tier` job runs the same
parallel lane, so finding 2 can reach it too.

## What would close it

- Make the lane satisfy its own preconditions, or stop `ci gate` from
  reaching the gate:
  - **Sync.** `lint-example-leaf.sh` syncs every leaf that has a
    `system.toml`, native included. Today it lints native leaves without
    syncing them.
  - **Bindings.** Either the `native-check` gate produces the bindings it
    asserts, or `check-lane-contracts` records that `ci gate` lacks a
    producer for them. It accepted the lane without one.
- **Pool.** Either the pool guard is keyed per label, or every `build-serial`
  gate that takes the pool is serialised against the others. Then one
  `just ci gate` on a fresh worktree should show all three findings gone.
