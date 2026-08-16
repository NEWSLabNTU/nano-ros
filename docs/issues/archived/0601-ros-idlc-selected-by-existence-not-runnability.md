---
id: 601
title: "`find_package(CycloneDDS)` selects ROS's `idlc`, which cannot run without ROS's library path — cold cyclone fixtures die at code=127"
status: resolved
type: bug
area: build
related: [issue-0500, phase-353]
---

## Symptom

A COLD `just build-test-fixtures lane=native` dies on the first cyclonedds leaf:

```
/opt/ros/humble/bin/idlc: error while loading shared libraries:
libiceoryx_binding_c.so: cannot open shared object file: No such file or directory
FAILED: [code=127] cyclonedds-ts/_genroot/builtin_interfaces/msg/Time.c …
make[1]: *** [… fixture-linux-c-cyclonedds] Error 127
```

`code=127` is "command not found", which reads like a missing tool. The tool is
present; it cannot LOAD. `idlc` links `libiceoryx_binding_c.so`, which lives in
ROS's `lib/` and is only on the loader path when `setup.bash` has been sourced
**into the build's own environment** — not merely into the shell that launched
it.

## Why it is invisible until a cold build

`build-test-fixtures lane=native` succeeded repeatedly on this host earlier the
same day. Those runs never invoked `idlc`, because the cyclone leaves were warm.
Anything that makes them cold — `just setup-cli` (which stales every workspace
fixture by design), a wiped build dir, a fresh clone — exposes it immediately.
So the lane looks healthy right up until the first person needs it to rebuild.

## Root cause, and it is a class

`find_package(CycloneDDS)` resolves to the first prefix that satisfies it, and
on a host with ROS installed that is `/opt/ros/humble`. The in-tree
`build/cyclonedds` provisioned by `just cyclonedds setup` loses, even though it
is the one this build controls.

Selection is by EXISTENCE; the property that matters is RUNNABILITY. This is
the third instance of that shape found in one session (phase-353 W2):

1. `nros_sizes_build::probe_key` keyed on the request, not the artifact —
   fixed, phase-353 W4.
2. `msg_to_cyclone_idl.py::_adapter_bin_and_env` picked
   `/opt/ros/humble/lib/rosidl_adapter` because the DIRECTORY existed, while
   `import rosidl_adapter` failed under the build's env — fixed by asking a
   subprocess whether the import actually works, then falling through to the
   vendored tree.
3. This one, unfixed.

It is also the same failure mode CLAUDE.md already records for issue 0500 —
"prefixes are enumerated newest-version-first precisely because `find_package`
takes the FIRST that resolves, and both provisioning paths print success either
way."

## Fix shape (not implemented)

Prefer the tool the build provisioned, and prove it runs before committing to
it. Concretely, one of:

* Put `build/cyclonedds` ahead of ROS on `CMAKE_PREFIX_PATH` for this build, the
  way issue 0500's prefix ordering does for Corrosion; or
* keep the discovery but VERIFY the resolved `idlc` executes (a `--version`
  probe) and fall back when it does not — the shape used for the rosidl adapter
  above; or
* propagate ROS's `LD_LIBRARY_PATH` into the build env when ROS's `idlc` is the
  one selected, so the choice is at least coherent.

The second is the one that generalises, and it makes the diagnostic honest: the
build says which `idlc` it chose and why, instead of failing as `code=127` at
the first `.idl`.

Whichever is chosen, print the resolved path — issue 0500's lesson is that a
provisioning path which "prints success either way" is how the wrong prefix wins
silently.

## Provenance

Found 2026-08-15 under phase-353 W2 while restoring the native fixture lane on a
host where the cyclone leaves had gone cold. `just cyclonedds setup` completes
successfully and does NOT resolve the problem, because the ROS copy still wins
discovery.

## Partly fixed 2026-08-15 — the guard is in, and what is NOT yet proven

Two runnability checks landed, both at the point where the tool is chosen:

* `NrosRmwCycloneddsTypeSupport.cmake::_nros_find_idlc` — `find_program` is what
  actually picks ROS's idlc off `PATH` for the native leaves. The resolved path
  is now probed and, when it cannot run, the build stops at CONFIGURE time with
  a message naming the binary, the loader error and the remedy, instead of
  `FAILED: [code=127]` on the first `.idl` far away in the ninja.
* `ProvideCycloneDDS.cmake` — the same check for the `find_package` branch,
  which is a different route to the same tool. (The fixture lane passes
  `-DCMAKE_DISABLE_FIND_PACKAGE_CycloneDDS=ON`, so this branch is inert there;
  it matters for a bare cmake build.)

**The probe flag is `-h`, not `--version`, and that distinction was measured
rather than assumed:**

```
working idlc:  -h -> 0     --version -> 1
broken  idlc:  -h -> 127   --version -> 127
```

`--version` is not a supported option, so probing with it would have rejected
every healthy install and silently forced source-provisioning everywhere. That
was the first version of this fix.

### Not demonstrated, and stated rather than implied

**The guard has not been observed firing end-to-end.** A cmake block only runs
on a CONFIGURE, and every `examples/**/build-cyclonedds` on this host was
configured before the change, so each keeps `/opt/ros/humble/bin/idlc` baked
into its `build.ninja` and still fails with `code=127`. Wiping one leaf's build
dir moved the failure to the next leaf, which is consistent with that reading
but is not proof the new message appears. A bare `cmake -S … -B …` of one leaf
does not exercise it either: without the fixture lane's extra defs the cyclone
typesupport path is not reached at all (no `idlc` in the generated ninja).

**This does not make the lane build here.** There is no working idlc on this
host: `just cyclonedds setup` completes but installs no
`build/cyclonedds/bin/idlc`, and ROS's copy cannot load. The fix converts a
misleading late failure into a legible early one; it does not conjure a tool.
Restoring the lane needs either ROS's environment propagated into the build, or
`-DIDLC_EXECUTABLE=<a working idlc>`.

### Still open

The preferred shape in "Fix shape" above — prefer the tool the build
provisioned, via prefix ordering — is NOT implemented, because this host has no
provisioned idlc to prefer. It remains the better answer for a host that does.

## Resolved 2026-08-16 — the preferred shape is now implemented

The "Still open" note above said prefix ordering was not implemented *because
this host had no provisioned idlc to prefer*. That premise no longer holds — the
SDK store has one:

```
$ env -i ~/.nros/sdk/cyclonedds/0.10.5-nros1/bin/idlc -h ; echo $?
0
$ env -i /opt/ros/humble/bin/idlc -h
/opt/ros/humble/bin/idlc: error while loading shared libraries:
libiceoryx_binding_c.so: cannot open shared object file: No such file or directory
127
```

It was looked for under `build/cyclonedds/bin` (where `just cyclonedds setup`
does not put it) rather than `$NROS_HOME/sdk/cyclonedds/<version>/bin` (where
`nros setup --tool cyclonedds` does). An interactive shell hides the whole thing,
because `LD_LIBRARY_PATH` there makes ROS's copy load — the failure only appears
in a clean environment, which is what the build actually gets.

**`_nros_cyclonedds_sdk_bins()` puts the SDK store on `_nros_find_idlc`'s HINTS,
newest version first.** HINTS rather than PATHS because HINTS are searched
BEFORE the system PATH, and preferring the tool this build provisioned is the
entire point. Newest-first for issue 0500's reason: the store accumulates, and a
provisioning run that installs a new version while an old one keeps winning is
the worst shape a setup step can have — it reports success and changes nothing.

### Measured, both directions

Clean env, ROS on PATH, dummy `CycloneDDS::ddsc`:

```
with the fix:     SELECTED idlc: ~/.nros/sdk/cyclonedds/0.10.5-nros1/bin/idlc
hints removed:    nano-ros: idlc at /opt/ros/humble/bin/idlc needs its own prefix libs;
                  running it with LD_LIBRARY_PATH=/opt/ros/humble/lib:… (issue 0601)
                  SELECTED idlc: /opt/ros/humble/bin/idlc
```

The second line also confirms the earlier partial fix works: when ROS's copy is
all there is, the derived `LD_LIBRARY_PATH` rescues it instead of dying
`code=127` mid-ninja. So the two halves now cover both cases — prefer the
provisioned tool, and make the ROS fallback actually run.

### What is still not demonstrated

The guard firing on a host with NO working idlc at all is still unobserved, for
the reason already recorded: it needs a configure, and it needs an environment
without either copy. That gap is unchanged and is not what this issue was about.
