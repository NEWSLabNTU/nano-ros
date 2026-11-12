---
id: 2480
title: "A leaf migrated to a workspace keeps its old `.leaf-selection`, so every configure of its images reads stale and the Zephyr promise check refuses `c/talker` xrce on every rebuild"
status: open
type: bug
severity: high
area: [cli, zephyr, cmake, build]
found: 2026-11-13
related: [2405]
---

## What

`just build-test-fixtures lane=tier1` at 8cf84ff5b9f5c6eae307ae3bb166d86bf750110b fails its zephyr stage on
one image, `zephyr/native_sim/native/64/c/talker/xrce`
(`build/zephyr-workspace-builds/3.7/build-c-talker-xrce`):

```
-- nano-ros: nros configure .../examples/zephyr/c/talker --image zephyr_xrce: configured (0 s)
-- nano-ros: image `zephyr_xrce`'s configure is stale (leaf output holds image `zephyr_zenoh`): this configure reads no seed from it and composes its own answer ...
...
CMake Error at cmake/NanoRosPromise.cmake:411 (message):
  nros: this image's entity inventory changed after this configure's readers
  ran, and no `nros configure` seeded them: they read the answer an earlier
  configure composed, which can be smaller than this one.
```

The make driver then stops scheduling, so 19 of the 26 Zephyr images are not
built, and `build-test-fixtures` never reaches its body
(`build-compile-check-fixtures`, the cmake-configure rows). In the following
`just ci` (tier 1, run with `NROS_SKIP_FIXTURE_CHECK=1`) that is 45 of the 47
reds: missing compile-check and cmake fixtures, and STALE west and Zephyr
fixtures.

It does not go away. A second `just build-test-fixtures lane=tier1` failed the
same way, and so does running the remedy the error names:

```
$ nros configure examples/zephyr/c/talker --image zephyr_xrce   # twice
configure: nros_zephyr_talker_c_bringup:zephyr_xrce: leaf output holds image `zephyr_zenoh`
configure: nros_zephyr_talker_c_bringup:zephyr_xrce: leaf output holds image `zephyr_zenoh`
```

## Why

`examples/zephyr/c/talker/build/nros/configured/.leaf-selection` holds
`zephyr_zenoh`, written 2026-11-10 by a configure from before 3eefb44f93771090a223ea5c4d1a5b49711b6042
(phase-531 W6) made `c/talker` a workspace (bringup under `src/`, no
top-level `system.toml`).

- `configure_stamp::currency` reads `leaf_selection(project)` for every project
  and calls the stamp stale when the holder is not the image key.
- `cmd/configure.rs` rewrites the marker only when `is_leaf(project)` (a
  top-level `system.toml`), and `leaf_settings` only for a leaf with a selection
  package. A migrated project is neither, so nothing rewrites or removes it.
- The image key is now qualified (`nros_zephyr_talker_c_bringup:zephyr_xrce`),
  so no image of the project can ever equal a bare `zephyr_zenoh`.

So the stamp is stale for good. The Zephyr hook's readers run unseeded, and
whenever the late composer's entity inventory differs from the fragment the
readers loaded, `_nros_promise_check_unseeded` stops the configure. Its remedy
(`nros configure ... --image zephyr_xrce`, then `west build --cmake`) cannot
help, because that configure is also stale.

A fresh clone has no `.leaf-selection`, so CI does not see this. Any checkout
that configured a leaf before its migration does. This worktree has 57
`examples/*/*/*` projects with a `.leaf-selection` and no top-level
`system.toml`, including all seven Zephyr talkers and clients. Only
`c/talker` xrce failed in this run. The other images either matched their
fragment or take a road with no promise check.

## Fix direction (not done here)

Treat the marker as relevant only where it can be written: ignore it, or
retire it, when the project is not a leaf (the same predicate `configure.rs`
uses before writing it). Compare it with the bare image id too, so a qualified
key and the marker spell the same thing. A gitignored residue should not wedge
a build that a fresh clone passes (issue 1366's shape).

## Evidence

- Fixture logs: `tmp/build-test-fixtures-20261112-193434-2334108/zephyr.log`,
  `tmp/build-test-fixtures-20261112-222337-3583499/zephyr.log`,
  `build/zephyr-fixtures/build-c-talker-xrce.log`.
- `.leaf-selection` mtime 2026-11-10 03:40 +0800; `examples/zephyr/c/talker`
  migrated in 3eefb44f93771090a223ea5c4d1a5b49711b6042 (2026-11-10 01:09 UTC).
