---
id: 2374
title: "A Zephyr build dir caches the CONF_FILE list it discovered, so deleting a board fragment makes every later configure fail, reconfigure included"
status: resolved
type: bug
severity: medium
area: [zephyr, build, fixtures]
related: [2157, 1288, 0882, phase-489, phase-544]
found: 2026-11-09
resolved_in: "the nano-ros Zephyr module re-runs Zephyr's own CONF_FILE / DTC_OVERLAY_FILE discovery on every configure when the cached value is one discovery wrote (zephyr/cmake/nros_discovered_config.cmake)"
---

**Home:** [phase-489](../../roadmap/phase-489-ci-build-speedup.md) (W6: warm build
trees for the fixture lanes). Found while verifying PR #3349 (phase-544 W2).

## What

An incremental Zephyr image build fails at configure when a Kconfig fragment
that Zephyr DISCOVERED (not one anybody named) has been deleted since the
build dir was first configured:

```
CMake Error at .../zephyr/3.7/zephyr/cmake/modules/kconfig.cmake:321 (message):
  File not found:
  .../examples/workspaces/realtime-c/src/demo_bringup/boards/native_sim_native_64/boards/native_sim_native_64.conf
```

Measured 2026-11-09 in a checkout whose
`build/zephyr-workspace-builds/3.7/build-ws-c-realtime-entry-zenoh` and
`build-ws-mixed-entry-zenoh` were configured before issue 2157 (commit
9a8e18a5cf) deleted the leaf `boards/native_sim_native_64.conf` copies. Both
caches hold

```
CONF_FILE:STRING=<bringup>/boards/native_sim_native_64/prj.conf;<bringup>/boards/native_sim_native_64/boards/native_sim_native_64.conf
```

while a FRESH configure of the same row, same tree, caches only `prj.conf` and
links (`build-w3349-ws-c-realtime-entry-zenoh`, PR #3349's verification).

## Why

Zephyr 3.7 `cmake/modules/configuration_files.cmake`:

```cmake
zephyr_get(CONF_FILE SYSBUILD LOCAL)
if(NOT DEFINED CONF_FILE)
  zephyr_file(CONF_FILES ${APPLICATION_CONFIG_DIR} KCONF CONF_FILE NAMES "prj.conf" ...)
  zephyr_file(CONF_FILES ${APPLICATION_CONFIG_DIR}/boards KCONF CONF_FILE ...)
  ...
endif()
set(CONF_FILE ${CONF_FILE} CACHE STRING "...")
```

Discovery runs only while `CONF_FILE` is undefined, and its RESULT is then
written to the cache. On every later configure the cached list is defined, so
discovery never runs again: a fragment added to `APPLICATION_CONFIG_DIR` is
ignored, and a fragment deleted from it is a hard error. `nros build`'s west
road leaves `CONF_FILE` unset on purpose (issue 1288: so Zephyr discovers the
image's `APPLICATION_CONFIG_DIR`), which is exactly the case that caches.

## The escape hatch does not recover it

CLAUDE.md's "re-configure, do not wipe" (`cmake <build-dir>`) reads the same
cache, so it fails the same way (measured, exit 1, same `File not found`).
`west build -p auto` is not pristine either, because the source dir did not
change. So the only recovery today is a pristine build, i.e. the wipe the rule
exists to avoid, and `just reconfigure-stale` would report the dir as wedged.

## Scope

- Local incremental builds and any persistent build tree (phase-489 W6's
  subject). CI's run-matrix lanes are not hit today, because
  `actions/checkout`'s `git clean -ffdx` deletes `build/` between runs.
- Every road that leaves `CONF_FILE` unset: `nros build <bringup>:<image>`
  and the fixture lane's retargeted rows, which run it.
- The ADD direction is silent: a new `boards/<board>.conf` is never merged
  into an existing build dir.

## Fix directions (not decided)

1. `nros build` states the list: compute the discovered fragments itself and
   pass `-DCONF_FILE=...` on every configure, so the cache is always
   overwritten. Loses nothing issue 1288 wanted if the list is computed by the
   same rule (`prj.conf` + `boards/<board>.conf` qualifiers), but that is a
   second spelling of Zephyr's discovery.
2. `nros build` drops the cached value when it was discovered: pass
   `-UCONF_FILE` on a re-configure of a dir whose cached `CONF_FILE` names a
   path that no longer exists (or always, when the image names no `conf`).
   Keeps Zephyr's own discovery the only spelling.
3. A staleness probe (`scripts/lib/ninja_stale_refs.py`, issue 1406) learns
   `CONF_FILE:STRING` entries, so `just reconfigure-stale` reports the dir
   with a remedy instead of a configure error.

## Evidence kept

The two stale dirs above were left in place, unmodified apart from the failed
`cmake` reconfigure.

## Resolution (2026-11-09)

Ruling: direction 2, Zephyr's own discovery re-run. It is placed in the
nano-ros Zephyr MODULE, not in `nros build`, so every road gets it: `nros
build`, a plain `west build`, the fixture lane, and the `cmake <dir>` escape
hatch.

- `zephyr/cmake/nros_discovered_config.cmake`, from the module's pre-Kconfig
  hook (`zephyr/modules/modules.cmake`, which runs before Zephyr's
  `configuration_files.cmake`). It drops a cached `CONF_FILE` /
  `DTC_OVERLAY_FILE` that DISCOVERY wrote, so discovery runs again on every
  configure. A chosen value is left alone, so it still suppresses discovery
  exactly as Zephyr documents. Chosen means: a `-D` on this configure (the
  entry is `UNINITIALIZED`), a cache edit, the environment, sysbuild, a
  snippet, or a `set()` in the application.
- The module's `zephyr/CMakeLists.txt` records what discovery produced (marker
  `NROS_ZEPHYR_DISCOVERED_<var>`). It also names the config dir and its
  `boards/` and `socs/` as configure inputs, so adding or deleting a fragment
  re-runs cmake from a plain `ninja`.
- A dir configured before the fix has no marker. Its value counts as
  discovered when every entry is an absolute path inside the cached
  `APPLICATION_CONFIG_DIR`, which is the shape `zephyr_file(CONF_FILES ...)`
  writes. That is how the two dirs above recover.

Why the other two lost:

- Direction 1 (an explicit `-DCONF_FILE`) is a second spelling of Zephyr's
  discovery, which CLAUDE.md's "one helper, not a second spelling" rule rules
  out. It would also turn the image's list into a CHOSEN value, which stops
  `boards/<board>.conf` from merging at all (the reason issue 1288 left it
  unset). And it covers only `nros build`, not `west build` or `cmake <dir>`.
- Direction 3 (a stale-ref probe) only reports. The dir still needs a wipe,
  the add direction stays silent, and a probe cannot see a fragment that
  exists but was never merged.

Measured 2026-11-09, Zephyr 3.7, `native_sim/native/64`, on a scratch copy of
`examples/zephyr/c/talker` built with `nros build zephyr_zenoh`. The leaf is
its own `APPLICATION_CONFIG_DIR`.

- Add: `boards/native_sim_native_64.conf` with `CONFIG_ASSERT=y`, then a plain
  `ninja -C <dir>`. Cmake re-ran, `CONF_FILE` gained the fragment, and
  `.config` gained `CONFIG_ASSERT=y`.
- Delete: removing the fragment, then a plain `ninja`, re-ran cmake and
  dropped it. `CONFIG_ASSERT` left `.config`. A further `cmake <dir>` and the
  next `nros build` both exit 0.
- The two evidence dirs, A/B on copies of their `CMakeCache.txt`:
  - With the old module, `cmake <copy>` still fails at `kconfig.cmake:321 File
    not found`.
  - With this module, it rediscovers (`CONF_FILE` is `prj.conf` only) and gets
    past Kconfig. The copy then stops at issue 1379's two-checkout guard,
    which is an artifact of the A/B, since the application comes from another
    checkout. The dirs themselves recover on their next `cmake <dir>` once
    their checkout carries this change.

Gate `just check zephyr-discovered-config`
(`tests/cmake-zephyr-discovered-config-tests.sh`, 20 assertions over 11 cases)
runs the hook around a stand-in for Zephyr's discovery rule, so it needs no
SDK. It was mutation-tested 8 ways, and each mutation fails at least one case:

- hook a no-op;
- no `-D` type check;
- the shape test always true;
- the shape test always false;
- no directory watch;
- no `<pending>` branch;
- no marker record;
- a `set()` not treated as a choice.
