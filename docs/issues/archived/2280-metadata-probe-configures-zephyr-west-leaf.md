---
id: 2280
title: "`nros configure` ran the host CMake metadata probe on a Zephyr west leaf, which printed Zephyr's \"BOARD is not being defined\""
status: resolved
type: bug
severity: low
area: [cli, zephyr, build]
related: [0286, 0641, 1469, 2166]
found: 2026-11-05
---

## What

The first `nros configure` (formerly `nros sync`) of a C or C++ Zephyr leaf
prints a CMake error and goes on:

```
$ source ~/.nros-box/workspaces/zephyr/3.7/env.sh
$ cd examples/zephyr/c/talker && nros configure
...
Loading Zephyr default modules (Zephyr base).
CMake Error at .../zephyr/cmake/modules/extensions.cmake:3352 (message):
  BOARD is not being defined on the CMake command-line, in the environment or
  by the app.
Call Stack (most recent call first):
  .../zephyr/cmake/modules/boards.cmake:61 (zephyr_check_cache)
  ...
  examples/zephyr/c/talker/CMakeLists.txt:12 (find_package)
```

Without `ZEPHYR_BASE` in the environment the same step prints `Could not find
a package configuration file provided by "Zephyr"` instead. Found by phase-524
W4b on a `nros new --board nucleo-f767zi-zephyr` package; measured here on
`examples/zephyr/c/talker` (2026-11-05, box tree, origin/main 5c2b018a0c).

The error is not fatal: configure ends `sync: done.`, and a second run prints
only the negative-cache line
`no producer for nros_zephyr_talker_c::talker (probe failed at this source last
sync; unchanged; last error: probe project configure failed: ...)`.

## Why

`metadata_refresh::refresh_stale_sidecars` hands every C/C++ component to the
host CMake probe (`metadata_probe_cmake`), which `add_subdirectory`s the
component's own `CMakeLists.txt` into one generated host project. A Zephyr
leaf's `CMakeLists.txt` is a west application: it calls
`find_package(Zephyr)`, and Zephyr's boilerplate refuses a configure with no
`BOARD`. `metadata_probe_cmake.rs`'s own header already says such a package
"must be reported as un-probeable rather than attempted", and RFC-0120 lists
"calling the metadata probe" for such a leaf as a rejected alternative ("The
probe is a host CMake project that cannot configure a leaf whose CMakeLists
needs `find_package(Zephyr)`. The census can."). Nothing applied that rule.

Two costs beyond the noise: a full CMake configure per first sync for an
answer known in advance, and, in a batch with other components, the failed
configure marks EVERY component of the batch unprobeable ("A configure failure
is the whole project").

## Not the fix

Passing the image's board to the probe: the probe is a host build by
construction (RFC-0120 D1, layer 0 "never enters `find_package(Zephyr)`"), and
a configured Zephyr board makes it a target build.

## Resolution

`cpp_probe_options` (`orchestration/metadata_refresh.rs`) now refuses a
package whose `CMakeLists.txt` calls `find_package(Zephyr)` before anything is
configured, with the reason in the ledger:

```
sync: source metadata — no producer for nros_zephyr_talker_c::talker (a west
application (its CMakeLists.txt calls find_package(Zephyr)), which a host CMake
probe cannot configure; RFC-0120 sizes such a leaf from its census or its
declared entities)
```

The answer is the one the failed configure reached (no sidecar, the component
in `unsupported`), now without running CMake, without the error, and without
taking the batch's other components down with it. The predicate is one
function, `builder::west_app::cmakelists_finds_zephyr`, which `cmd::build`'s
`zephyr_leaf_application` (the "this leaf is its own west application" test)
now calls too, rather than spelling the same line scan a second time.

## Verified

- `nros configure --force` on `examples/zephyr/c/talker` with `ZEPHYR_BASE`
  set, `metadata/talker.json.unprobeable` removed first: no `CMake Error`, the
  ledger line above, no marker written.
- Test `a_west_application_is_not_handed_to_the_host_cmake_probe`: a
  `find_package(Zephyr)` CMakeLists is refused for C and C++; an ament one, and
  one naming Zephyr only in a comment, still probe. Mutation (guard disabled):
  the test fails.
