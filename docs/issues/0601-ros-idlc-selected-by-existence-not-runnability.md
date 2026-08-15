---
id: 601
title: "`find_package(CycloneDDS)` selects ROS's `idlc`, which cannot run without ROS's library path — cold cyclone fixtures die at code=127"
status: open
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
