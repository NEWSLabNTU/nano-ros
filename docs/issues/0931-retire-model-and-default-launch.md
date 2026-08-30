---
id: 931
title: "`nano_ros_entry` has eleven arguments; four have no users and three restate the bringup"
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

`LAUNCH` needed a second correction, and the fix goes the other way from the
first. Counting `LAUNCH` values across `examples/` gave 26 `default` and 37
naming a real launch file, which read as "LAUNCH carries real information for
the majority". **Every one of those 37 is in a GENERATED CMakeLists under a
`/build` directory** — tool output, not authored input. Filtering to files a
human wrote:

```
for f in $(grep -rlE '^\s*LAUNCH\s' examples --include=CMakeLists.txt | grep -v /build); do
    grep -hE '^\s*LAUNCH\s+\S+' "$f"
done | sort | uniq -c
      9 LAUNCH default
```

Nine hand-written entries, all `default`, none naming a file. **No user in this
repo has ever specified a launch file.** The first count conflated generated
output with authored input by filtering on file extension instead of on who
wrote the file.

So `LAUNCH` is ceremony on the ENTIRE authored surface, not on a subset.

## Proposed

**1. Retire `MODEL`.** No deprecation window is needed for a parameter with no
users; keep the argument accepted for one release ONLY so an out-of-tree caller
gets a `FATAL_ERROR` naming `LAUNCH` rather than "unknown argument". Delete
`_NRA_MODEL`'s resolution branch, which also deletes the
`$ENV{NROS_MODEL_DIR}` / `${CMAKE_BINARY_DIR}/nros/` fallback chain — a second
way of locating a generated artifact that exists only to serve `MODEL`.

**2. Drop `LAUNCH` from the authored surface.** `BRINGUP` alone means "this
bringup's default launch", which is what all nine hand-written entries spell
out. The generated CMakeLists can keep passing an explicit launch selection
through a non-user-facing spelling — a generator SHOULD be explicit — but a
human never writes it.

## The whole argument surface, measured

`nano_ros_entry` parses eleven keywords. Hand-written users in the tree
(generated CMakeLists excluded):

| arg | purpose | authored uses | already known elsewhere? |
| --- | --- | ---: | --- |
| `NAME` | CMake target name | 6 | no — local |
| `SOURCES` | the entry's own TU(s) | 6 | no — local |
| `BRINGUP` | bringup package dir (the SSoT) | 6 | — it IS the SSoT |
| `PANIC` | panic policy | 6 | no — build policy |
| `TYPED` | real-executor seam vs descriptor | 6 | no |
| `BOARD` | board key; gates non-native DEPLOY | 6 | partly: model has `extra.target` |
| `DEPLOY` | which deploy target(s) to build | 6 | YES: model has `extra.deploy_name` |
| `LANG` | c / cpp / rust | 3 | YES: inferred from SOURCES at line 220 |
| `LAUNCH` | which launch file | 9, all `default` | — see above |
| `MODEL` | the resolved artifact | **0** | superseded by BRINGUP |
| `LOCATOR` | transport locator | **0** | model has `rmw:` |
| `ARGS` | k=v to the generated main | **0** | — |
| `LAUNCH_ARGS` | k=v launch bindings | **0** | — |

A resolved model already carries what `DEPLOY`, `BOARD` and `LOCATOR` restate:

```yaml
deploy:
  /talker:
    domain: 0
    rmw: zenoh
    extra:
      deploy_name: robot1
      target: x86_64-unknown-linux-gnu
```

**Why cmake asks anyway, and it is not an oversight.** The DEPLOY gate at
`NanoRosEntry.cmake:159` runs BEFORE the model is resolved (~line 270), so it
cannot consult it. That is an ORDERING constraint, not a missing fact —
reordering the gate after resolution is what makes `DEPLOY`/`BOARD` derivable.
Worth stating because "the model already knows" invites deleting the argument
without moving the gate, which would fail on the embedded path.

`LANG` is already inferred when omitted (line 220), so its three authored uses
are redundant today, with no code change needed to drop them.

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
