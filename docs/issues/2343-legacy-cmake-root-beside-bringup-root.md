---
id: 2343
title: "A tree built before issue 2287 keeps its `build/<key>/cmake` root beside the new `build/<key>-<bringup>/cmake`, and the entity census finds the entry's binary in both"
status: open
type: bug
area: [build, cmake, cli]
severity: high
found: 2026-11-07
related: [issue-2287]
---

## Symptom

The tier-2 nightly (run 42341355560, job 128309517818, step
`just build tier2-nightly`). `nros build service_bringup:linux_service_server`
succeeded, then the entity census refused:

```
entry `linux_service_server_entry` names a binary in 2 image roots, so which one
to census cannot be decided here:
  examples/workspaces/cpp/build/posix-zenoh-linux/cmake/linux_service_server_entry
  examples/workspaces/cpp/build/posix-zenoh-linux-service_bringup/cmake/linux_service_server_entry
Name it with `--binary` or `--build-dir`.
```

## Cause

Issue 2287 (commit fd6f9594d3, "the cmake build dir names the bringup") moved
a CMake workspace image's build dir from `build/<key>/cmake` to
`build/<key>-<bringup>/cmake`, where `<key>` is `cmake_configuration_key`
(platform, rmw, board, `-cfg<hash>`) and the new name is `cmake_coordinate`.
Nothing removes the old root. The runner's fixture tree persists across runs,
so the pre-2287 `build/posix-zenoh-linux/` still holds a `cmake/` dir with the
entry's binary from the last build before the change. The census searches
every `build/*/cmake/<entry>` (`locate_entry_binary`,
`packages/cli/nros-cli-core/src/cmd/entity_census.rs`, the issue-1419 search)
and refuses an ambiguity, so it finds two.

The 2287 resolution said "the old dir is left in place; a `rm -rf build/`
reclaims it". A persistent tree never runs that.

## Code site

`packages/cli/nros-cli-core/src/cmd/build.rs`: the `Driver::CMake` arm of
`plan_for`, which writes the root at `build/<cmake_coordinate>/` via
`builder::cmake_root::write` and never looks at `build/<cmake_configuration_key>/`.
