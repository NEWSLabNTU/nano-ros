---
id: 2343
title: "A tree built before issue 2287 keeps its `build/<key>/cmake` root beside the new `build/<key>-<bringup>/cmake`, and the entity census finds the entry's binary in both"
status: resolved
type: bug
area: [build, cmake, cli]
severity: high
found: 2026-11-07
related: [issue-2287]
resolved: 2026-11-07
resolved_in: "fix-issue-2343 branch: a build retires the pre-2287 cmake root of its configuration"
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

## Resolution

Fixed in 2287's own code, not in the census: the build that writes a root at
the bringup-keyed coordinate retires the legacy root of the same
configuration. `retire_legacy_cmake_root` (`cmd/build.rs`) runs right after
`cmake_root::write` in the `Driver::CMake` arm, on `Planning::Generate` only,
so `nros plan`, `nros image-facts` and the preflight still write and delete
nothing. Before 2287 every bringup on one configuration shared the root, so
whichever builds first retires it. It prints one line:

```
nros build: removed the stale pre-issue-2287 build dir build/posix-zenoh-linux/ (cmake roots are per bringup now, at build/posix-zenoh-linux-service_bringup/; the old one held a second copy of each entry's binary)
```

It deletes only what it can positively identify, and only under `build/`:

- `build/<cmake_configuration_key>` must be a real directory (not a symlink),
  the key one plain path component, and not the root just written;
- its `CMakeLists.txt` must be a regular file that opens with
  `builder::cmake_root::GENERATED_HEADER` (now a constant, the first line of
  every generated root) and names a bringup (`SYSTEM <b>`). A package-mode root
  (`build/<pkg>/`) names none, a hand-made project carries no header;
- the dir name must not end in `-<b>`, which could make it the current root of
  another configuration.

It removes the root's own files, `cmake/`, `nros-image-env.toml` and the
`CMakeLists.txt` (last, so an interrupted retire is retried), and the dir
itself only if that left it empty: a west build dir can sit under the same
name. A failure warns and the build goes on.

Other readers of build roots, checked once the legacy root is gone:

- the entity census search (`locate_entry_binary`) scans `build/*/cmake/<entry>`
  and now finds one. A tree that still holds both roots before its first new
  build refuses as before, naming both, until that build runs; the nightly
  always builds first;
- `nros image-facts`, `nros run`'s artifact lookup, the fixture-row check
  (`every_generated_workspace_fixture_row_names_its_cmake_build_dir`) and the
  nros-tests locators all take the path from `cmake_coordinate` through
  `ResolvedBuild::cmake_build_dir`, and never read the legacy name;
- `nros ws clean` removes the whole build root, both layouts with it.

Test: `a_build_retires_the_pre_2287_cmake_root_beside_its_own`
(`tests/build_verb_pipeline.rs`) builds a tree with the new root and the
entry's binary in it, an unrelated `build/unrelated/cmake/<entry>`, and a
legacy-named dir that is a hand-made cmake project. A build leaves that dir
alone. It then writes a real pre-2287 root for another bringup (header,
`SYSTEM other_bringup`, `cmake/CMakeCache.txt`, the stale binary, the
image-env file, and a nested west dir), asserts two roots hold the entry, and
builds. The legacy root's files are gone, one root holds the entry, the west
dir and the unrelated dir are untouched, and an emptied legacy dir is
removed. It failed before the fix ("the legacy binary dir is gone").
