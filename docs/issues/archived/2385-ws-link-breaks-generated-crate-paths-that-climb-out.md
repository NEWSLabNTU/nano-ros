---
id: 2385
title: "An entry's `_ws` link breaks a workspace generated crate whose path climbs to nano-ros, so `nros configure` in examples/workspaces/features fails on a cold tree"
status: resolved
type: bug
severity: high
area: [build, cli, fixtures, zephyr]
related: [2322, phase-523]
found: 2026-11-10
---

**Home: [phase-523](../../roadmap/phase-523-education-labs-adoption-probes.md)**,
where issue 2322 ([open again](../2322-image-not-reproducible-across-paths.md))
was fixed. Found by phase-531 W6 while it was building Zephyr fixtures.

## Measured

On a cold worktree at `a877dac37d`, `just zephyr build-fixtures` (any
`NROS_ZEPHYR_FIXTURE_FILTER`) stops in "features workspace-entry prep". At
that step, `nros configure` in `examples/workspaces/features` takes the census
of `demo_bringup:zephyr_rust_qos`. To do so it builds the host image
`demo_bringup:linux_rust_qos`, and the host image's `cargo update` fails:

```
error: no matching package named `nros-core` found
location searched: <checkout>/examples/workspaces/features/build/packages/core/nros-core
required by package `std_msgs v0.0.0 (<checkout>/examples/workspaces/features/build/posix-zenoh/linux_rust_qos_entry/_ws/generated/std_msgs)`
    ... which satisfies path dependency `std_msgs` of package `rust_qos_listener_pkg v0.1.0 (.../linux_rust_qos_entry/_ws/src/rust_qos_listener_pkg)`
```

The configure then refuses: `census missing: no census at
.../build/nros/models/demo_bringup/rust_qos_model.census.json`.

Good and bad:

| commit | result |
| --- | --- |
| `b5d2596d30` | builds (phase-531 batch 8, cold worktree, 2026-11-09 15:28Z) |
| `a877dac37d` | fails as above |

`69c5bcf04c` (issue 2322, PR #3352) is in that range, and it is the commit
that adds the link.

## Cause

Issue 2322 gave every generated entry `_ws -> ../../..` and routed each
dependency path into the workspace through it. cargo normalises a path
dependency **lexically**, and that is the point of the fix: `_ws/src/<pkg>`
then hashes relative to the root.

The workspace's own generated crates keep paths relative to where they really
live. `examples/workspaces/features/generated/std_msgs/Cargo.toml` says:

```toml
nros-core = { path = "../../../../../packages/core/nros-core", default-features = false }
```

From `features/generated/std_msgs` that path reaches the checkout root, which
is correct. cargo, though, reaches the crate as
`build/posix-zenoh/linux_rust_qos_entry/_ws/generated/std_msgs` and applies
the five `..` lexically to that path, without resolving the link. Five levels
up lands in `features/build/`, so it looks for `features/build/packages/core/nros-core`.

So a crate reached through `_ws` breaks whenever its own relative paths leave
the workspace: an in-tree workspace's generated message crates pointing at
nano-ros' `packages/`. Every in-tree workspace whose generated crates
path-depend on the checkout is exposed. A copied-out project whose deps resolve
outside the workspace by absolute path or registry would not be.

## Impact

- `just zephyr build-fixtures` cannot run on a cold tree, and neither can
  anything behind it. A warm tree whose census already exists may not notice.
- It blocked phase-531 W6's verification of #3358, which then skipped the
  features prep locally to build the six `build-ws-rust-*-cyclonedds` rows.

## Not done here

No fix. Options for the owner: emit generated-crate paths through the same
`_ws` spelling, rewrite them to absolute paths at link time, or resolve the
link before cargo sees it. All three trade against 2322's byte-identity goal,
so the choice belongs to 2322's owner.

## Resolution (2026-11-10)

Reverted #3352 (69c5bcf04c). Main is back to the pre-`_ws` layout, so a
workspace generated crate's relative climb to nano-ros resolves from its real
directory again. Issue 2322 reopens; its redo must keep every relative path a
routed crate carries inside the workspace, verified by a cold
`just zephyr build-fixtures`.
