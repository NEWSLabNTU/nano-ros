---
id: 2906
title: "A Zephyr build dir configured before a setting was retired refuses to configure, and only a hand cache edit gets it out"
status: open
type: bug
area: [zephyr, build, ux]
severity: medium
found: 2026-11-23
related: [2847]
---

## What happens

On 2026-11-23, re-configuring an existing Zephyr fixture build dir
(`/tmp/nros-build-aeon/zephyr/build-ws-c-realtime-entry-smp`, row
`workspace-zephyr-c-realtime-smp`) on current main failed:

```
CMake Error at zephyr/cmake/nros_image_kconfig.cmake:180 (message):
  nano-ros: a retired nano-ros setting is set:

    -DCONFIG_NROS_ZENOH_LOCATOR: CONFIG_NROS_ZENOH_LOCATOR is retired: set CONFIG_NROS_LOCATOR in a guest's conf, or `locator` under [system] or [image.<id>] in a system.toml project
    -DCONFIG_NROS_PARAM_STORE: CONFIG_NROS_PARAM_STORE is retired: set CONFIG_NROS_PARAMS_STORE instead
```

The same dir had built cleanly that morning, from main as it was then. The
current command line passed neither setting. Both came from the build dir's
`CMakeCache.txt`:

```
CLI_CONFIG_NROS_PARAM_STORE:INTERNAL=n
CLI_CONFIG_NROS_ZENOH_LOCATOR:INTERNAL="tcp/10.0.2.2:7591"
```

Zephyr records every `-DCONFIG_<X>` it is ever given as a `CLI_CONFIG_<X>`
cache entry and re-applies it on every later configure of that dir. A value
an older tree passed therefore outlives the tree. The retired-setting
refusal (`_nros_image_kconfig_refuse_retired`, RFC-0140 D4) reads those
entries on purpose, so it refuses a value that nothing current passes.

## Why it matters

Every user with an existing Zephyr build dir who upgrades past a retirement
hits this on their first incremental build. The message tells them to set
the new spelling, which they may already do, and it shows the setting as
`-DCONFIG_...`, as if their command passed it. Nothing says the value lives
in the build dir's cache or how to clear it.

The only way out found here was to delete the two `CLI_CONFIG_*` lines from
`CMakeCache.txt` by hand. After that the dir configured and built. Two
remedies were not verified:
- `cmake -N -U CLI_CONFIG_<X> <dir>` changed nothing, since `-N` does not
  write the cache;
- `cmake -U CLI_CONFIG_<X> <dir>`, which would re-run the configure, was not
  tried.

Wiping the dir also works, but that is the antipattern CLAUDE.md names.

The same refusal treats `NROS_OVERRIDE_<ID>` differently: since RFC-0103 D2
those overrides are consumed and removed from the cache on every configure,
so a stale one cannot outlive its configure. `CLI_CONFIG_*` is Zephyr's own
mechanism and gets no such treatment.

## Directions (not decided)

1. Tell a stale cache entry apart from one this configure was given, and
   for a cache-only entry either drop it with a warning naming the new
   spelling, or refuse with the exact remedy (the `cmake -U` command for
   that entry, once verified).
2. At the least, say in the refusal whether the value came from the command
   line, a conf file or the build dir's cache, and print the remedy for each.

## Closes when

A Zephyr build dir configured before a setting was retired either configures
on the next incremental build or is refused with a message that names where
the value lives and one command that clears it. A test on a probe project
covers the cache-only case, beside `tests/zephyr-transient-overrides-tests.sh`.
