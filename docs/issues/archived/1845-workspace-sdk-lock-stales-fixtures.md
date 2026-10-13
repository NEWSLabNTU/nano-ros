---
id: 1845
title: "A workspace's `nros-sdk.lock` is untracked and unignored, so each entry build re-stales its sibling workspace fixtures"
status: resolved
type: bug
severity: medium
area: [tooling, tests]
related: [1262]
found: 2026-10-12
---

## Measured

phase-485 / RFC-0104 D4 has `nros build` write `<ws>/nros-sdk.lock`. In the
in-repo example workspaces that file is neither tracked nor gitignored. Only
the checkout-root `/nros-sdk.lock` is ignored (`.gitignore`, the
`/nros-sdk.lock` line). After one fixture build:

```
?? examples/workspaces/c/nros-sdk.lock
?? examples/workspaces/cpp/nros-sdk.lock
?? examples/workspaces/mixed/nros-sdk.lock
?? examples/workspaces/features/nros-sdk.lock
... (9 workspaces)
```

`scripts/build/source-manifest.sh`'s `nros_source_manifest` hashes
`git ls-files --cached --others --exclude-standard`, so an untracked,
unignored lock is an input to every workspace fixture's signature
(`.nros-workspace-fixture.<id>.inputsig`). Each entry build adds its own
`[cargo."build/…"]` section to the lock, which changes the signature of
every sibling fixture built before it.

Effect, measured on `scripts/build/workspace-fixtures-build.sh linux
{rust,c,cpp,mixed}` followed by the `workspace_features_e2e` /
`workspace_metadata` / `roundtrip_xprocess_e2e` tests: 23 of 35 tests read
STALE fixtures that had been built minutes earlier. A second build pass
leaves the locks byte-identical (md5 checked), and the signatures then
settle, so a sweep needs two passes to be green.

## Fix direction

Decide what the lock IS for an in-repo workspace:

- a build output: ignore `examples/**/nros-sdk.lock` (and the templates'),
  so it leaves the source manifest; or
- a recorded input: track it and keep its content stable across entry
  builds (one write per workspace, not one section per entry build).

Either way, the manifest must not see a file that the build it fingerprints
writes. Sweep: `git status --short -- '*nros-sdk.lock'` after a fixture
build must print nothing.

## Resolution (2026-10-13)

The lock is a build artifact, which is what RFC-0104 D4 already says: "In a
checkout it is a build artifact (gitignored, as today)". The ignore rule was
narrower than that statement, since `/nros-sdk.lock` matches only the
checkout root. It is now `nros-sdk.lock`, at any depth, so no workspace's lock
reaches `nros_source_manifest` any more.

This was also the live-peer host job's last red. Run 38321018350 failed 4
cells (`rust_multi_node_entry_per_node_graph_nodes` and its cyclonedds
variant, `test_ros2_param_set_reconfigures_live_read`,
`xrce_set_parameters_naming_25_declared_lands`) on `Workspace fixture … is
stale`, minutes after the job built those fixtures.

Measured locally, removing the lock first each time:

1. build `workspace-rust-native`;
2. build its sibling `workspace-rust-native-xrce`;
3. run `rust_multi_node_entry_per_node_graph_nodes`.

With the old root-only rule the test FAILS
(`Workspace fixture workspace-rust-native is stale`). With the fix it PASSES.

Gate: `just check sdk-lock-ignored`. It asks `git check-ignore` for
`<dir>/nros-sdk.lock` in every directory that holds a `system.toml` or
`.colcon_workspace`, and self-tests the old rule on a throwaway repository.
Restoring the old rule in this checkout turns it red, naming the lock paths.

Found in the same sweep: `packages/testing/nros-tests/bins/contract-monitor-cpp`
was the one cmake leaf under `bins/` with no `.gitignore`, so its
`build-zenoh/` sat untracked and unignored. It now has its siblings' file.
