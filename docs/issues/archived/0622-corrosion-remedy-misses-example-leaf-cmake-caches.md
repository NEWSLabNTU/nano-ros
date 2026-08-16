---
id: 622
title: "The legacy-Corrosion FATAL_ERROR's remedy clears only the WORKSPACE build
  trees, so following it verbatim leaves the build failing identically"
status: resolved
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

## Resolution 2026-08-16

The `Fix` above had already landed in the **`FATAL_ERROR`** arm by the time this
was picked up — the grep line and the 0622 citation are both in it. What had not
happened is the rest of the class:

**The `WARNING` arm still said "remove its build dir".** That arm is the one a
reader actually reaches: the fatal branch was downgraded to opt-in
(`NROS_STRICT_CORROSION`) hours after landing, because as a hard failure it took
out four fixture families. So the incomplete remedy this issue was filed about
was still live — it had moved, not gone. Fixing the reported site and not the
class, one arm apart.

Both arms now share one report, and it is the *"worth considering alongside"*
from this issue rather than the glob: `_nros_corrosion_stale_caches()` scans the
workspace trees **and** the example leaves and prints what it found —

```
  12 CMakeCache.txt in this checkout still name a legacy prefix. A cache
  is authoritative for the next configure of its tree, so these must go —
  deleting the CMakeCache.txt is enough, the trees themselves need not:
    examples/native/c/talker/build-0622probe1/CMakeCache.txt
    …
    … and 2 more
```

and, when it finds none:

```
  No CMakeCache.txt in this checkout names a legacy prefix, so a stale
  cache is NOT what is pinning this resolution — look at the resolution
  path itself (an `add_subdirectory` import never consults the SDK prefixes).
```

That negative branch matters as much as the positive one, and it is what the
module's own comment already measured: the 155-vs-28 split was *not* stale
caches, it was an `add_subdirectory` import bypassing `_nros_corrosion_prefixes`.
A remedy that only knows how to say "clear your caches" sends that reader to
delete 62 files and come back no better off. Now the message distinguishes the
two.

Verified by exercising all three paths against real files (no caches / 12 caches
listed / truncation at 10), not by reading the code.

### One defect found by running it

The first cut passed several arguments to `set()`, which makes a **list**, and
`message()` renders a list joined by `;` — so the remedy printed `;` between
every line, mid-sentence. Fixed to a single quoted string. Worth recording
because it is invisible in the source and obvious in the output.

### Still open, and NOT this issue

The underlying `add_subdirectory` resolution path still bypasses the newest-first
ordering; the module's comment says the fatal arm can be promoted back once that
is fixed. This issue was only ever about the instructions.
