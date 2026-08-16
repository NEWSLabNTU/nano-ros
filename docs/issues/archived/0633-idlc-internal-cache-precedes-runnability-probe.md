---
id: 633
title: The cached `NROS_RMW_CYCLONEDDS_IDLC` is reused if it EXISTS, so issue 0601's
  runnability probe never runs in a build dir that already resolved a broken `idlc`
status: resolved
type: bug
area: build
related: [0601, 0500, phase-353, phase-365]
---

## Symptom

On a host with ROS installed but incomplete, a cold cyclone leaf build dies:

```
/opt/ros/humble/bin/idlc: error while loading shared libraries:
libiceoryx_binding_c.so: cannot open shared object file: No such file or directory
FAILED: [code=127] cyclonedds-ts/_genroot/builtin_interfaces/msg/Time.c …
```

That is issue 0601's symptom, and 0601 is closed. `763e463cd` prefers the
SDK-provisioned `idlc` over whatever is first on `PATH`, and probes the resolved
binary with `-h` so a tool that cannot load fails at CONFIGURE time with a
legible message.

It does not help here, and re-running cmake does not help either.

## Why the fix cannot take effect

`NrosRmwCycloneddsTypeSupport.cmake` caches the resolved path:

```cmake
if(NOT NROS_RMW_CYCLONEDDS_IDLC OR NOT EXISTS "${NROS_RMW_CYCLONEDDS_IDLC}")
    …resolve: imported target, then IDLC_EXECUTABLE, then _nros_find_idlc()…
endif()
```

Both the SDK preference and the `-h` probe live inside that block. The gate on
entering it is `NOT EXISTS`. A build dir configured before the fix holds

```
NROS_RMW_CYCLONEDDS_IDLC:INTERNAL=/opt/ros/humble/bin/idlc
```

and that file **exists** — it just cannot load its own libraries. So the block
is skipped, the probe never runs, the preference never applies, and the stale
answer is reused forever.

The reuse gate asks whether the tool EXISTS. The property that matters is
whether it RUNS. That is verbatim the class 0601 named:

> Selection is by EXISTENCE; the property that matters is RUNNABILITY.

0601 fixed it at the point of SELECTION and left it standing at the point of
REUSE, which is the same defect one line up. It is the fourth instance of the
shape in this area, after `probe_key`, `_adapter_bin_and_env`, and 0601 itself.

## Measured, not inferred

On this host, 14 leaf build dirs under `examples/native/{c,cpp}/*/build-cyclonedds`
had the ROS path baked in, and a working `idlc` was present the whole time at
`~/.nros/sdk/cyclonedds/0.10.5-nros1/bin/idlc`.

**A correction, because the first version of this issue got it wrong.** It
claimed `libiceoryx_binding_c.so` was "absent from that install entirely", on
the strength of this:

```
LD_LIBRARY_PATH=/opt/ros/humble/lib /opt/ros/humble/bin/idlc -h  -> 127
```

That probed the wrong directory. The library is present, one level down:

```
/opt/ros/humble/lib/x86_64-linux-gnu/libiceoryx_binding_c.so
```

and with ROS's own arch lib dir on the loader path — which `activate.sh`
provides — ROS's `idlc` runs fine (`rc=0`). So the tool is not intrinsically
broken here, and `_nros_idlc_runs` already derives exactly that directory
(`<prefix>/lib/${CMAKE_LIBRARY_ARCHITECTURE}`) when its first attempt fails.

That makes the cache the whole of the problem rather than one contributor to
it: 0601's fallback was already sufficient to rescue this host, and never got
the chance to run.

What is NOT determined is why the fixture lane saw `code=127` at all, given the
lane was launched from a shell that had sourced `activate.sh`. Something between
that shell and the `idlc` invocation does not carry `LD_LIBRARY_PATH`, and this
issue does not establish what. It is recorded as an open question rather than
guessed at; the defect below reproduces and is fixed independently of it.

A plain reconfigure did **not** dislodge the cached path:

```
cmake -S examples/native/c/talker -B …/build-cyclonedds     -> 33 ROS idlc refs remain
cmake … -DIDLC_EXECUTABLE=<sdk idlc>                        -> 33 remain (third fallback, never reached)
cmake … -DNROS_RMW_CYCLONEDDS_IDLC=<sdk idlc>               -> 0 remain
```

Only overriding the cache entry itself worked. `IDLC_EXECUTABLE` is consulted
two levels inside the block the cache short-circuits, so the documented escape
hatch is unreachable in exactly the situation that needs it.

## This is also why 0601's guard was never seen firing

0601 recorded, honestly, that the guard "has not been observed firing
end-to-end", and attributed it to build dirs configured before the change. That
explanation was incomplete: a RECONFIGURE would not have fired it either. The
cache, not the configure timing, is what holds the old answer.

## Fixed 2026-08-16

Two changes in `NrosRmwCycloneddsTypeSupport.cmake`, one per cache:

* **The reuse gate asks whether the tool RUNS.** `NOT EXISTS` became a
  `_nros_idlc_runs()` probe of the cached path, so a tool that is present and
  unusable triggers re-resolution instead of being reused forever.
* **Every rung is probed, and `find_program`'s cache is dropped first.**
  Candidates — imported target, `IDLC_EXECUTABLE`, then a fresh search — are
  tried in order and the first that RUNS wins. `unset(_idlc_found CACHE)`
  precedes the search, because `find_program` answers from its cache without
  searching, so a provisioning run that installs a working tool would otherwise
  change nothing.

The selection block gained the same treatment: a cached `IDLC_EXECUTABLE` that
cannot run is dropped and searched for again before the `FATAL_ERROR` fires,
rather than turning a recoverable state into a hard stop.

And it prints the choice — `nano-ros: idlc <path> via <origin>` — because 0500's
lesson is that a path which reports success either way is how the wrong answer
wins silently, and a sticky cache is a second way to win silently: the
reconfigures that changed nothing still printed `Configuring done`.

### Proven by reproduction, not by reading

0601 could never observe its own guard firing. This one was reproduced: both
caches were poisoned by EDITING `CMakeCache.txt` — not by passing `-D`, which
would prove something weaker, since the value would arrive fresh at the very
configure under test, where the whole point is a value already sitting there
from an earlier run. The stub is a file that exists and cannot execute.

A reconfigure with NO flags then recovered on its own:

```
-- nano-ros: cached idlc …/fakeprefix/bin/idlc no longer runs
   (exit Permission denied); re-resolving (issue 0633)
-- nano-ros: idlc /home/aeon/.nros/sdk/cyclonedds/0.10.5-nros1/bin/idlc
   via search (SDK store, then PATH)
```

Afterwards both caches point at the SDK tool — `_idlc_found` as well as
`NROS_RMW_CYCLONEDDS_IDLC`, which is the half a one-cache fix would have missed
— and `build.ninja` holds 0 references to the unusable one.

The first attempt at this proof used ROS's `idlc` as the "broken" tool and
reported NOT RECOVERED. That was the instrument, not the fix: under
`activate.sh` that binary runs, so the gate correctly reused it. Chasing that
is what turned up the arch-subdirectory correction above.

## Fix shape as originally proposed (superseded by the above)

Gate the reuse on runnability rather than existence — `_nros_idlc_runs()`
already exists and is what the resolution path calls:

```cmake
set(_reuse OFF)
if(NROS_RMW_CYCLONEDDS_IDLC AND EXISTS "${NROS_RMW_CYCLONEDDS_IDLC}")
    _nros_idlc_runs("${NROS_RMW_CYCLONEDDS_IDLC}" _why _env)
    if(NOT _why)
        set(_reuse ON)
    endif()
endif()
if(NOT _reuse)
    …resolve…
endif()
```

Cost is one `execute_process` per configure of a cyclone leaf, against a tool
that must run thousands of times in the build that follows.

Two things to keep while doing it:

* **Say which `idlc` was chosen and why**, in the configure output. 0500's
  lesson is that a provisioning path which "prints success either way" is how
  the wrong prefix wins silently, and this cache is a second way to win
  silently — the reconfigures above printed `Configuring done` / `Generating
  done` while changing nothing.
* **`find_program`'s own cache is a second copy of the same problem.** The
  stale dirs also held `_idlc_found:FILEPATH=/opt/ros/humble/bin/idlc`, so even
  a forced re-entry into the block would return the ROS hit without searching.
  Whatever fix lands must invalidate that too, or it fixes one of two caches
  and reports success.

## Workaround

```
cmake -S <leaf> -B <leaf>/build-cyclonedds \
      -DNROS_RMW_CYCLONEDDS_IDLC="$HOME/.nros/sdk/cyclonedds/<ver>/bin/idlc"
```

or delete the build dir. Note the deletion costs a full cyclone rebuild per
leaf, which is why the reconfigure path is worth fixing.

## Provenance

Found 2026-08-16 while restoring the native fixture lane after a rebase, on the
same host as 0601. `just build-test-fixtures lane=native` failed at
`fixture-linux-c-cyclonedds` with `code=127` even though 0601 was resolved and
its fix was present in the tree.
