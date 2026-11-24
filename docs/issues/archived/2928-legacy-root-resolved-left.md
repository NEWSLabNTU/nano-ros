---
id: 2928
title: "Retiring an nros11 per-bringup cmake root leaves its resolved/ directory behind"
status: resolved
type: bug
area: [cli, build]
severity: low
found: 2026-11-24
resolved: 2026-11-24
---

## What happens

0.6.0-nros11 configured a C or C++ image in one cmake root per bringup,
`build/<coord>-<bringup>/`. 0.6.0-nros12 configures one root per image,
`build/<coord>-<bringup>-<image>/` (issue 2702), and `nros build` retires the
older root (`retire_legacy_cmake_root` in
`packages/cli/nros-cli-core/src/cmd/build.rs`, issue 2343).

The retire removes the root's `cmake/`, its image env file and its
`CMakeLists.txt`, then removes the directory only if it is empty ("anything
else under this name is not the root's"). But nros11 also wrote the shared
configure's resolve inside that root: `resolved/<bringup>__union/` with
`resolved.toml` and `coordinate.toml` (`coordinate_resolve_dir`). So the
directory is never empty, and an almost-empty old root stays beside the new
one.

Measured on 2026-11-24, with linux C and C++ projects made and built by
installed 0.6.0-nros11, then rebuilt in place with the CLI of #4738 (a
checkout build):

```text
build/posix-zenoh-linux-up_c_node_bringup/              # kept, 20 KB
build/posix-zenoh-linux-up_c_node_bringup/resolved/up_c_node_bringup__union/resolved.toml
build/posix-zenoh-linux-up_c_node_bringup/resolved/up_c_node_bringup__union/coordinate.toml
build/posix-zenoh-linux-up_c_node_bringup-linux/        # the new root
```

The configure itself is removed, so the space lost is small. The nros12
release page says `nros build` removes the old directory, and a user who
lists `build/` still sees it.

## What would close it

`retire_cmake_root` also removes `resolved/` when every entry under it is a
`<bringup>__union` resolve of the root's own `SYSTEM`, then removes the
directory as now. A test plants an nros11-shaped root (generated header,
`SYSTEM`, `cmake/`, `resolved/<bringup>__union/`) and asserts the whole
directory goes, while a root holding anything else keeps that.

Not an nros12 blocker: the leftover is a few KB and nothing reads it.

## Related

- `packages/cli/nros-cli-core/src/cmd/build.rs`: `retire_legacy_cmake_root`,
  `retire_cmake_root`, `coordinate_resolve_dir`
- `book/src/migrating/releases/0.6.0-nros12.md`, "A build directory from the
  previous release"

## Resolution

Fixed 2026-11-24 as directed. `retire_cmake_root` now also removes the
retired root's `resolved/` when `holds_only_own_union_resolve` says it holds
nothing but the root's own shared-configure resolve: a real directory whose
every entry is a real directory named `<SYSTEM>__union` (spelled by
`resolve::dir_name_for`, as `coordinate_resolve_dir` writes it) containing
regular files only. Then the directory goes as before, if empty. A
`resolved/` with anything else in it (another bringup's resolve, a symlink, a
nested dir) is kept whole, and so is the directory. A file of the user's
beside the root still keeps the directory.

Test: `an_nros11_root_goes_with_its_own_union_resolve` plants the measured
nros11 shape (generated header, `SYSTEM`, `cmake/`,
`resolved/<bringup>__union/{resolved,coordinate}.toml`) and asserts the whole
directory goes. A second root with another bringup's resolve beside its own
keeps `resolved/` whole. A third with a stray file keeps the directory but
loses its own resolve.
