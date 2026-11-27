---
id: 3004
title: "A `ZEPHYR_SDK_INSTALL_DIR` one level above the SDK root lets CMake take another SDK from its registry, silently"
status: resolved
type: bug
severity: medium
area: [zephyr, toolchain, cmake, diagnostics]
found: 2026-11-27
resolved: 2026-11-27
resolved_in: "fix-zephyr-sdk-nested-root"
reporter: "nros NXP 175 (B43 run, relayed via nros 243)"
---

## What happened

Reported by NXP 175. In its B43 run at nano-ros 0e4afce2fa,
`ZEPHYR_SDK_INSTALL_DIR` was set to `~/.nros/sdk/zephyr-sdk/0.16.9`. The
nros store unpacks the SDK tarball without stripping its top-level
directory, so the SDK root is one level deeper:
`~/.nros/sdk/zephyr-sdk/0.16.9/zephyr-sdk-0.16.9/` (the directory holding
`sdk_version`). Zephyr's `FindZephyr-sdk.cmake` passes the variable to
`find_package` only as a HINT. A hint that names no SDK root is not an
error: the search went on to the CMake package registry and configured
with SDK 0.16.8, with no message.

## The class

nros's own exports already name the root: `nros locate tool zephyr-sdk`,
`nros sdk-path`, `scripts/lib/zephyr-sdk.sh store-dir`, the `just` FVP
recipes and the book's `export` lines all print `.../0.16.9/zephyr-sdk-0.16.9`.
The gap was in every place that READS an operator's value. Each accepted the
root or a parent holding `zephyr-sdk-<pinned>`, and passed over any other
value:
- `nros build` (`builder::zephyr::resolve_toolchain`) fell to the store;
- `scripts/build/zephyr-toolchain.sh` (`nros_zephyr_sdk_env`) fell to the
  CMake registry;
- `runner-doctor.sh` reported the parent spelling only.

A plain `west build` was not reached by any of them, so CMake decided
alone.

## Fix

One rule, spelled in the three languages that read the variable:
`builder::zephyr::callers_sdk` (Rust), `nros_zephyr_sdk_callers`
(`scripts/lib/zephyr-sdk.sh`), and `nros_zephyr_sdk_dir_check`
(`zephyr/cmake/nros_zephyr_sdk_dir.cmake`).
1. The value is an SDK root (it holds `sdk_version`, or is named
   `zephyr-sdk-*`): use it.
2. `<value>/zephyr-sdk-<pinned>` exists: use that.
3. Exactly one `zephyr-sdk-*` child holds `sdk_version`: use that. This is
   the store's `<version>/` directory.
4. Anything else is REFUSED, naming what was found and the directory to set
   (`nros locate tool zephyr-sdk`). It is never passed over.

The CMake half runs in the module's `module_ext_root` hook
(`zephyr/modules/modules.cmake`), after module discovery and before
`dts.cmake` looks the SDK up. So it reaches a plain `west build`, not only
`nros build`. A resolved value replaces the one Zephyr reads (cache first,
then environment, as `zephyr_get` orders them), with a status line naming
both paths.

One behaviour changed on purpose: a parent directory that holds exactly one
SDK at another version now names that SDK. It used to join the pinned
version, miss, and fall to the store. The caller's SDK wins at any version
under the root spelling, so it does under this one too.

## Tests

A fake store laid out both ways (`<v>/zephyr-sdk-<v>/sdk_version`), in each
language:
- `builder::zephyr::a_callers_sdk_one_level_up_resolves_to_its_root_or_is_refused`;
- `scripts/check-zephyr-sdk-env.sh`: the version directory and the root
  resolve, while two levels up and a missing path are refused. Against
  main's scripts, those cases fail 5 checks (each answered rc 0 with the
  registry's SDK);
- `just check zephyr-sdk-dir` (`tests/cmake-zephyr-sdk-dir-tests.sh`): the
  module hook under `cmake -P`, with the same five cases;
- `runner-doctor.sh --self-test`: one level above the root names that root.

Not run here: a Zephyr configure. Zephyr builds are blocked on host 139.
