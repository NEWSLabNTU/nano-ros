---
id: 1887
title: "`just native check` renders only non-native leaves, so native leaves fail at manifest parse on an unsynced checkout"
status: resolved
type: bug
area: [build, testing, examples]
severity: medium
found: 2026-10-14
resolved: 2026-10-14
related: [1882, 1381]
---

## Measured

`just ci gate` in a fresh linked worktree on `main` (the tree after
`1f07376670`, issue 1882's fix), after `just setup-worktree`,
`just setup-cli` and `just generate-bindings`. Step 4 (`check::build`) fails
in `native-check`, rc=2, on five leaves:

```
`cargo metadata` exited with an error: error: failed to load manifest for
dependency `native_rs_async_action_client_nros_selection`
  failed to read `.../examples/native/rust/action-client-async/build/nros/selection/Cargo.toml`
  No such file or directory (os error 2)
```

The same error appears for `native_rs_action_client`, `native_rs_action_server`,
`native_rtic_action_client` and `native_rtic_action_server`. Those are
`examples/native/rust/{action-client,action-client-async,action-client-rtic,action-server,action-server-rtic}`.

Run again after those five leaves were synced by hand, the gate then failed
on `custom-msg` (`native_rs_custom_msg_nros_selection`). After the fix,
it then failed on `logging`: `no matching package named nros-log`. The
`make` fan-out stops at the first failing unit, so each run shows only one
member of the class.

## Cause

Issue 1882 made `scripts/build/lint-example-leaf.sh` run `nros sync` before
linting, but only for a non-`native` leaf with a `system.toml`. Since
phase-445 W6, no example tracks a `.cargo/` (`check-example-cargo-dirs`).
So every example leaf depends on something sync renders:

- 21 native leaves name their `<leaf>_nros_selection` crate.
- `examples/native/rust/logging` has no `system.toml` at all, and it
  resolves `nros-log` only through the `[patch.crates-io]` row that sync
  writes.

On a host where a fixture build had already synced the leaves, the gate
passed. On a checkout where nothing had, it failed.

## Fix

`lint-example-leaf.sh` syncs every leaf that `nros sync` can read (one with
a `system.toml` or a `package.xml`). The only tracked example Cargo leaf
with neither is `examples/px4/cpp/bridge/ffi`, which the recipe already
excludes.

A native leaf gets no `--config`. Sync already wires
`build/<image>/nros-cargo.toml` into the leaf's own `.cargo/config.toml` as
an `include` (issue 1381), and cargo joins `rustflags` across config files,
so passing the file a second time would read it twice. Board leaves keep
issue 1882's measured `--config` path.

**Verified:** with `logging/.cargo` removed and `custom-msg` never synced,
`just native check` exits 0.
