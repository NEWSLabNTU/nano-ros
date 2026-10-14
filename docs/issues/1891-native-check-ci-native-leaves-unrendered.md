---
id: 1891
title: "`native-check` on CI: the native example leaves are linted without `nros sync`, so their generated selection package and patch rows do not exist"
status: open
type: bug
area: [ci, build, examples]
severity: medium
found: 2026-10-14
related: [1882, 1305]
---

## Measured

Scheduled gate
[38498943417](https://github.com/NEWSLabNTU/nano-ros/actions/runs/38498943417)
on main at `cad05d2ea` (the first scheduled run after issue 1882's fix put
`native-check` on `build-serial`). In "check / build tier + no-std
(nightly/manual only)", `check-build` failed 1 of 26 gates, `native-check`
(rc=1, about 57 min). It is the only failing gate.

```
error: no matching package named `nros-log` found
location searched: crates.io index
required by package `native-rs-logging v0.1.0 (…/examples/native/rust/logging)`
```

In the same gate, 21 `cargo metadata` errors appeared, one per native Rust
leaf. For example:

```
error: failed to load manifest for dependency `native_rs_action_client_nros_selection`
  failed to read `…/examples/native/rust/action-client/build/nros/selection/Cargo.toml`
```

The leaves include `rtic-*`, `serial-listener` and the `action-client`
variants.

## Cause

`scripts/build/lint-example-leaf.sh` renders a leaf before linting it, but only
under a platform condition:

```bash
if [ "$platform" != "native" ] && [ -f "$dir/system.toml" ]; then
    … (cd "$dir" && nros sync) …
```

Issue 1882's resolution added the render for the board-image leaves
(`mps2-an385-baremetal`, `esp32-c3-baremetal`). Since issue 1305, though, the
native leaves also depend on a generated `<leaf>_nros_selection` package and
on sync's `[patch]` rows (that is where the registry-named `nros-log` gets its
path). The native branch never renders, so on a fresh CI checkout neither
exists. 1882's green measurement came from a distrobox where those leaves had
already been synced, so they were not fresh.

## What it is not

- Not issue 1882 reopened as the same defect. 1882 was "`native-check` never
  runs", and it now runs. This is the first thing that run found.
- Not a runner or disk problem. `nros sync failed` never appears (0
  occurrences), because the native branch does not call sync at all.

## What would close it

1. Render native leaves too. Drop the `"$platform" != "native"` half of the
   condition, or give the native branch its own `nros sync`. Then check that
   `nros sync` succeeds for a native leaf in the build-tier job's environment.
   Leaves with message dependencies need ament packages there.
2. Acceptance: a scheduled or manually dispatched gate where `check-build`
   passes `native-check` from a fresh checkout. The way 1882 measured it does
   not count, because the leaves were already synced.
