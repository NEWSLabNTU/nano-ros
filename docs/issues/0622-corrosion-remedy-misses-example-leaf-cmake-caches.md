---
id: 622
title: "The legacy-Corrosion FATAL_ERROR's remedy clears only the WORKSPACE build
  trees, so following it verbatim leaves the build failing identically"
status: open
type: bug
area: build
related: [issue-0616, issue-0500, issue-0493, phase-364]
---

## Symptom

`just build-test-fixtures lane=native` dies at configure:

```
CMake Error at cmake/NanoRosCorrosion.cmake:176 (message):
  nano-ros: Corrosion 0.5.0 shares ONE cargo target-dir across workspace roots.

    resolved: 0.5.0 via SDK store — ~/.nros/sdk/corrosion/0.5.1-nros1/lib/cmake/Corrosion

  Fix: provision the pinned copy and clear the trees that cached the old
  topology in their CMakeCache —

      nros setup --tool corrosion
      rm -rf examples/workspaces/*/build-workspace-fixtures*
```

Doing exactly that does not fix it. The next run fails with the same error, from
the same line, naming the same stale prefix.

## Why

The remedy clears the WORKSPACE fixture trees. The stale resolution is also
cached in every EXAMPLE LEAF build directory, and those are not mentioned:

```
$ grep -rl "corrosion/0.5" examples/*/*/*/build*/CMakeCache.txt | wc -l
62
```

`nros setup --tool corrosion` reports `present 0.6.1-nros1 (skip)` — the pinned
copy was already installed and was never the problem. What keeps 0.5 alive is
the 62 leaf caches, each holding the resolved path from a configure that ran
before the pin existed. A `CMakeCache.txt` is authoritative for the next
configure of that tree, so the guidance is not merely incomplete: it produces an
identical failure and reads like the fix did not work.

Clearing them resolves it:

```
$ grep -rl "corrosion/0.5" examples/*/*/*/build*/CMakeCache.txt | xargs rm -f
$ cmake -B build-zenoh -DNROS_RMW=zenoh          # in examples/native/c/talker
$ grep -oE "corrosion/[0-9.]+[^/]*" build-zenoh/CMakeCache.txt | sort -u
corrosion/0.6.1-nros1
```

## Why the fix belongs in the message, not in a runbook

This is issue 0500's shape one layer up. 0500 is about the SDK store
ACCUMULATING so a stale prefix shadows the pin, and its lesson was "read the
configure's `nano-ros: Corrosion <ver> via <origin>` line — never infer the
version from having run the installer". The same applies to the remedy: having
run the two commands says nothing about whether every cache that recorded the
old topology is gone.

The message is the only place a reader is looking at the moment they need this.
An incomplete remedy at that moment is worse than no remedy, because it converts
"I do not know what to do" into "I did what it said and it is still broken".

## Fix

Extend the remedy to the leaf caches, e.g.

```
    nros setup --tool corrosion
    rm -rf examples/workspaces/*/build-workspace-fixtures*
    grep -rl 'corrosion/0\.[0-5]' examples/*/*/*/build*/CMakeCache.txt | xargs -r rm -f
```

Deleting just the `CMakeCache.txt` is enough — the whole tree need not go, which
matters because there are 62 of them and a full rebuild of the C/C++ example
leaves is expensive.

Worth considering alongside: the gate could TELL the user which trees are stale
rather than hand them a glob, since it already knows the resolved path it
rejected and could scan for caches naming a `0.[0-5]` prefix. That turns the
remedy from "run this and hope" into a list.

## Provenance

Found on 2026-08-16 while running tier 1 for phase-364, on a host where
`0.6.1-nros1` and `0.5.1-nros1` both sit in the SDK store. The gate itself is
correct and caught a real topology hazard — this is only about the instructions
it prints.
