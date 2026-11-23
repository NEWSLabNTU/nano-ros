---
id: 2906
title: "A Zephyr build dir configured before a setting was retired refuses to configure, and only a hand cache edit gets it out"
status: resolved
type: bug
area: [zephyr, build, ux]
severity: medium
found: 2026-11-23
resolved: 2026-11-23
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

## Resolution

Fixed 2026-11-23 by direction 1, drop with a warning. The module's hook runs
before Zephyr's `kconfig.cmake`, so at that point a `CONFIG_<SYM>` cache entry
is this configure's command line and a `CLI_CONFIG_<SYM>` alone is an earlier
configure's. `_nros_image_kconfig_drop_stale_cli`
(`zephyr/cmake/nros_image_kconfig.cmake`) removes a cache-only
`CLI_CONFIG_<SYM>` for every retired symbol (the registry's
`NROS_KNOB_RETIRED_KCONFIG`, the list the refusal reads) and every knob symbol
(the sibling refusal that sends a knob to system.toml had the same wedge). It
warns with the value, where it came from and what to set instead. A value this
configure passes is still refused. The remedy runs inside the configure, so
it reaches every road that re-configures a kept build dir (`just
reconfigure-stale`, `cmake <dir>`, an incremental `west build`, a persistent
runner workspace) with no hand cache edit. Measured on the probe below; the
end-to-end check is a re-configure of a dir wedged by phase-581 W6.

Test: `tests/zephyr-transient-overrides-tests.sh` case H configures a probe
build dir with a retired symbol as an older tree would, then re-configures
under the current module (no refusal, warning, entry gone), then passes the
symbol live (refused). Making the helper a no-op fails four of H's checks.

### Second sighting and the end-to-end check (host 142, 2026-11-23)

An agent worktree that ran `just build-test-fixtures tier1` at an older base
and then moved to current main got 120 refusals, and its zephyr stage
returned rc=2. 23 of the 28 dirs under `build/zephyr-workspace-builds/3.7/`
carried `CLI_CONFIG_NROS_ZENOH_LOCATOR`. `just reconfigure-stale` could not
help: `scripts/nros-reconfigure-stale.sh` re-runs a bare `cmake <dir>`, which
keeps the cache, so it reported the dirs as unrepairable. By hand,
`cmake -U CLI_CONFIG_<X> <dir>` cleared one dir.

With this fix's module in place, a bare `cmake <dir>` on two of those dirs:

- `build-rust-talker-zenoh`: rc=0. It warned "dropped
  CLI_CONFIG_NROS_ZENOH_LOCATOR=…" and "dropped CLI_CONFIG_NROS_PARAM_STORE=n",
  then `Configuring done`, and both entries were gone from `CMakeCache.txt`.
- `build-c-talker-xrce`: rc=0. It dropped `CLI_CONFIG_NROS_PARAM_STORE=n`.
  `CLI_CONFIG_NROS_DOMAIN_ID` and `CLI_CONFIG_NROS_XRCE_AGENT_PORT`, which are
  not retired, stayed and applied.
- A live `-DCONFIG_NROS_ZENOH_LOCATOR=…` on the same dir was still refused
  (rc=1).

One residue is out of scope here: a refused live `-DCONFIG_<retired>` stays in
the cache as `CONFIG_<SYM>:UNINITIALIZED`, because the configure aborts before
`kconfig.cmake` moves it. Every later bare configure therefore reads it as
live and refuses again, until the user runs `cmake -U CONFIG_<SYM> <dir>`.
That user did pass the setting once, so the refusal is honest, but it does not
name that remedy.
