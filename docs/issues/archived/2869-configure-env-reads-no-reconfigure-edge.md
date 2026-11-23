---
id: 2869
title: "Configure-time environment reads that are not knobs (cargo profile, interface search path, platform cffi include, stack-floor switch) reach a build only on a configure: a build after the variable moved used the old value silently"
status: resolved
type: bug
area: [cmake, zephyr, build]
severity: medium
found: 2026-11-23
resolved: 2026-11-23
related: [2810, 0991, 2318, 2803]
---

## What failed

Issue 2810 added a refusal for registry knobs on the Zephyr road. It records
each knob's environment at configure, and a build whose live environment
differs stops, naming the knob and `cmake "<build-dir>"`. The refusal is the
user's ruling, not an automatic re-configure.

The same missing edge exists for environment variables a configure reads that
are NOT registry knobs. cmake re-reads a variable only on a configure, and
ninja re-runs cmake only for a file newer than `build.ninja` (issue 0991). So
a value exported, changed or unset after the configure was ignored by every
build that followed: no error, and the old value was used.

## Classification (sweep of `$ENV{` and `ENV{` in `cmake/`, `zephyr/`, 2026-11-23)

**Changes what is built, so it is now refused (fixed here):**

| variable | read at | what a stale value does |
|---|---|---|
| `NROS_CARGO_PROFILE` | `NanoRosCargoProfile.cmake` | every crate builds at the old profile (optimisation level, debug assertions) |
| `NROS_INTERFACE_SEARCH_PATH` | `NanoRosBundledInterfaces`, `NanoRosCodegenCore` (2), `NanoRosGenerateInterfaces`, `find/_NrosFindRosMsgPackage` (2) | the configure's package resolution and ament witness keep the old roots, while build-time codegen reads the live variable: the two disagree |
| `NROS_PLATFORM_CFFI_INCLUDE` | `zephyr/cmake/nros_cargo_build.cmake` | baked into the Zephyr cargo command, so a moved header dir never reaches zpico-sys |
| `NROS_STACK_FLOOR` | `NanoRosStackFloor.cmake` | the stack-floor check stays on or off as configured, whatever the build's environment says (issue 2803's "a later change is seen" held only on a configure) |

**Not refused, with the reason:**

- **Handshakes `nros build` sends on every configure** (and, for the raise, on
  the build too): `NROS_STACK_FLOOR_RAISE` and
  `NROS_RESOLVED_ENTITY_RECORD_SHA256`. No operator sets them.
- **Already watched:** `NROS_EXECUTOR_ARENA_BYTES` is a registry knob (issue
  2810's record).
- **Tool, SDK and store locations,** which cmake pins at the first configure
  by design, the way it pins `CMAKE_C_COMPILER` (issue 0391):
  - `HOME`, `CARGO_HOME`, `RUSTUP_TOOLCHAIN`;
  - `NROS_STORE`, `NROS_SDK_STORE`, `NROS_HOME`, `NANO_ROS_ROOT`, `NROS_CLI`;
  - `ZEPHYR_BASE`, `ZEPHYR_SDK_INSTALL_DIR`, `TI_ARM_CLANG_ROOT`, `IDLC_EXECUTABLE`;
  - the board SDK dirs `NUTTX_FFI_CRATE_DIR`, `VIRTIO_DRIVER_DIR`,
    `NSOS_NETX_DIR`, `THREADX_BOARD_DIR`.

  A checkout's shells agree on these (`activate.sh`), and issue 1280 re-roots
  a foreign checkout's value.
- **The ROS install:** `AMENT_PREFIX_PATH`. It is cmake's own `find_package`
  model, read at configure like `CMAKE_PREFIX_PATH`, and it differs between
  shells by overlay order. A refusal on it would fire on most builds.
  Moving the packages themselves is caught by the ament witness (issue 1780).
- **Zephyr's own:** `BOARD` and `SNIPPET`. West owns them.
- **Mode switches and cache locations:** `NROS_VERBOSE`, `NROS_GEN_CACHE_DIR`,
  `NROS_LEAF_CENSUS_HOST`, `NROS_ALLOW_UNSYNCED_MODEL`. None changes image
  content.
- **Writes, not reads:** `THREADX_PORT`, `THREADX_EXTRA_INCLUDES`,
  `THREADX_CONFIG_DIR`, `NETX_CONFIG_DIR`, `RUSTFLAGS`,
  `NROS_RMW_CYCLONEDDS_SCRIPTS_DIR`.

## A second defect, found on the way: `NROS_CARGO_PROFILE` was read on the FIRST configure only

`nros_resolve_cargo_profile` wrote its answer into the cache with `FORCE`,
then read that cache entry as an explicit `-DNROS_CARGO_PROFILE` on every
later configure. A changed `NROS_CARGO_PROFILE` (or `CMAKE_BUILD_TYPE`)
therefore never reached an existing build dir, not even after a
re-configure. A refusal telling the user to re-configure would have been
wrong.

Fixed with issue 2803's pattern: `_NROS_CARGO_PROFILE_AUTO` remembers what
the function wrote. A cache value equal to it is decided again (environment,
then build type), and any other value is an explicit `-D` and wins.

A build dir configured before this change has no `_NROS_CARGO_PROFILE_AUTO`,
so its cached profile still reads as explicit until its cache entry is
removed. That is the conservative direction: a real `-D` is never
overridden.

## Fix

**The helper.** One helper, `nros_configure_env_watch(<name>)`
(`cmake/NanoRosKnobEnv.cmake`), extends issue 2810's record and check, so
there is one refusal and one spelling:
- A file calls it BEFORE its first read of the variable, and before
  anything in the configure sets it, so the record holds the operator's
  value.
- It records that value, marks the name as watched, and installs the
  build-time check.
- On a HOST configure, a deferred pass orders every buildable target after
  the check, so a refused build compiles nothing.
- A Zephyr build is left to its own ordering (the cargo build, issue 2810),
  so that `pristine` and `flash` keep working.

**The host record holds the watched names only.** A host configure also reads
registry knobs (the shared cargo dir key). Their environment reaches cargo
at build time, where each build script's `rerun-if-env-changed` sees a change.
Only `nros_knob_env_check_target`, which the Zephyr module calls, asks for
the knob comparison.

**The refusal names the configured value:**
`NROS_CARGO_PROFILE: this build dir was configured with dev; the environment now says release`.

**The gate** (`check-knob-env-refusal`):
- Every file that reads a watched variable must call
  `nros_configure_env_watch(<name>)`. 7 read sites of 4 variables are covered.
- It runs a negative control on the real read site of `NROS_STACK_FLOOR`
  (`NanoRosStackFloor.cmake`) and of `NROS_INTERFACE_SEARCH_PATH`
  (`NanoRosBundledInterfaces.cmake`), covering four cases:
  - a change refuses, naming both values;
  - the refused build built nothing;
  - unset refuses;
  - a registry knob the host project also read does not refuse.
- Mutations:
  - dropping one site's watch fails the gate, naming the file;
  - dropping the host ordering fails it ("the refused build still built a
    target").

## Measured (negative controls on real builds, 2026-11-23, x86_64)

**Host road**, `examples/linux/c/talker` image `linux` (Unix Makefiles),
configured by `nros build` with `NROS_CARGO_PROFILE=dev`. Each row is a
plain `cmake --build`:

| environment at build | result |
|---|---|
| unchanged | passes |
| `NROS_LOG_MAX_LEVEL=warn` (a registry knob) | passes |
| `NROS_CARGO_PROFILE=release` | refused: `configured with dev; the environment now says release` |
| `NROS_CARGO_PROFILE` unset | refused: `... now says unset` |
| `NROS_STACK_FLOOR=OFF` | refused: `configured with it unset; ... now says OFF` |
| `NROS_INTERFACE_SEARCH_PATH=/tmp` | refused: `configured with it unset; ... now says /tmp` |

The executable's mtime did not move across the four refused builds.

- `NROS_CARGO_PROFILE=release cmake <dir>` re-configured to `release`
  (`NROS_CARGO_PROFILE:STRING=release`), and the build passed. The second
  defect above is fixed.
- `cmake -DNROS_CARGO_PROFILE=dev <dir>` keeps `dev`, and the variable is not
  watched then: builds with `NROS_CARGO_PROFILE=release` or `other` pass.

**Zephyr road**, `examples/zephyr/c/talker` image `zephyr_zenoh`
(native_sim):

| environment at build | result |
|---|---|
| unchanged | passes |
| `NROS_PLATFORM_CFFI_INCLUDE=/tmp/elsewhere` | refused: `configured with it unset; the environment now says /tmp/elsewhere`; `zephyr.exe` untouched |
| `NROS_MEMORY_HEAP_BYTES=999999` | refused as before (issue 2810's knob check is unchanged) |
