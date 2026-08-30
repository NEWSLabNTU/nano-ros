---
id: 931
title: "`MODEL` has no users and should be retired; `LAUNCH default` is ceremony on 26 entries"
status: open
area: build, api
severity: low
found: 2026-08-30
related: [phase-330, phase-392, RFC-0063, RFC-0065]
---

# `nano_ros_entry`'s MODEL parameter is dead, and LAUNCH is half-ceremony

## Measured

`MODEL` names a resolved SystemModel directly. phase-330 W4.a made models build
artifacts and demoted it to "expert override, deprecated"; `NanoRosEntry.cmake`
enforces that `LAUNCH` and `MODEL` are mutually exclusive.

**In-tree users of `MODEL`: zero.**

```
grep -rn '^\s*MODEL ' --include=CMakeLists.txt --include='*.cmake' . | grep -v '^./cmake/'
```

returns nothing but the internal forward at `NanoRosEntry.cmake:348`. Not "few" —
none. Every entry in the tree is launch-addressed.

`LAUNCH` is a different story, and the first draft of this issue had it wrong.
Counted across `examples/`:

| value | entries |
| --- | ---: |
| `LAUNCH default` | 26 |
| a NAMED launch file (`multihost.launch.xml`, `service_server.launch.xml`, …) | 37 |

So `LAUNCH` is **not** ceremony in general — the majority carry real
information, namely which launch file inside the bringup package this entry
addresses. Only the 26 `default` cases are noise.

## Proposed

**1. Retire `MODEL`.** No deprecation window is needed for a parameter with no
users; keep the argument accepted for one release ONLY so an out-of-tree caller
gets a `FATAL_ERROR` naming `LAUNCH` rather than "unknown argument". Delete
`_NRA_MODEL`'s resolution branch, which also deletes the
`$ENV{NROS_MODEL_DIR}` / `${CMAKE_BINARY_DIR}/nros/` fallback chain — a second
way of locating a generated artifact that exists only to serve `MODEL`.

**2. Make `LAUNCH` optional, defaulting to `default`.** `BRINGUP` alone then
means "the bringup's default launch", which is what 26 entries spell out today.
The 37 that name a file keep naming it. This is the whole user-facing win:
one fewer line on the simplest entry, and nothing lost on the complex one.

## What to be careful about

**The guard at `NanoRosEntry.cmake:131`** rejects an entry with no `LAUNCH`, no
`MODEL` and no `SOURCES`. Defaulting `LAUNCH` changes that from an error into a
launch-addressed entry, so a genuinely empty entry (a typo, a half-written
CMakeLists) would stop being caught. The default must therefore be conditional
on `BRINGUP` being present — `BRINGUP` without `LAUNCH` means `default`;
neither means the same error as today.

**`nano_ros_entry` is public API consumed by out-of-tree workspaces.** The
book's user-facing flow and `examples/templates/` both spell entries, so this is
a documented-surface change, not an internal rename. Both need updating in the
same commit or the templates teach the retired spelling.

## Not a blocker for, but adjacent to

phase-392 W5.c reads entity facts off `_NRA_MODEL`. That read is currently
believed broken for a reason not yet established (see phase-392's W5.g section,
where an earlier diagnosis was retracted). Retiring `MODEL` does not fix it —
the LAUNCH path populates `_NRA_MODEL` too — but it removes one of the three
ways that variable can be set, which makes the remaining failure easier to
isolate.
