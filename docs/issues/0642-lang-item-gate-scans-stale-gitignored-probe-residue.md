---
id: 642
title: "`check-archive-lang-items` fails the fixture build on 16-day-old
  gitignored probe residue, and takes ~25 min doing it"
status: open
type: bug
area: build, testing
related: [issue-0616, issue-0436, phase-366]
---

## Symptom

`just build-test-fixtures lane=tier2` fails AFTER every one of its eight
platform families reports OK:

```
== zephyr == OK        == nuttx == OK      == qemu == OK
== threadx_linux == OK == esp32 == OK      == freertos == OK
== threadx_riscv64 == OK                   == native == OK
...
check-archive-lang-items: examples/workspaces/c/build/nros-metadata/metadata-probe-cmake/build/c_talker_pkg/CMakeFiles/c_talker_pkg.dir/link.txt links 2 archives that each define the global allocator:
    ../nano_ros/packages/api/nros-cpp/libnros_cpp.a
    ../nano_ros/packages/api/nros-c/libnros_c.a
error: recipe `build-test-fixtures` failed with exit code 1
```

Every flagged path is under `build/nros-metadata/metadata-probe-cmake/`.

## Two problems, one scan

**1. It fails on stale, gitignored, throwaway output.**

```
$ ls -la .../c_talker_pkg.dir/link.txt
Jul 31 22:47                       # 16 days old
$ git check-ignore -v .../link.txt
examples/workspaces/c/.gitignore:2:/build/
```

The gate landed on 2026-08-16 (`fb5ef2521`, "one allocator per LINK LINE", and
`133aaa61a`). The files it fails on are metadata-PROBE residue from a July run —
gitignored build output, regenerated on demand, not part of any shipped image.
Deleting `examples/workspaces/*/build/nros-metadata` makes the gate pass with no
source change, which is the tell.

**The rule is right and worth keeping.** One allocator per link line is exactly
what #0616 and #0436 are about. What is wrong is the SCOPE: a probe's link line
is not an image's link line, and a link line from sixteen days ago is not
evidence about this tree.

**2. It takes ~25 minutes, almost all of it in `find`.**

The scan runs unbounded finds over directories that contain build output:

```
find examples -name link.txt -path '*CMakeFiles*' -type f     # ~22 min
find packages -name link.txt -path '*CMakeFiles*' -type f
find build    -name link.txt -path '*CMakeFiles*' -type f
```

Measured mid-run: 22 minutes elapsed against **15 seconds of CPU** — pure I/O,
walking every object file in every leaf build tree. That cost is paid by every
`build-test-fixtures` invocation, on top of the build it is checking.

## Suggested fix

* **Scope the scan to link lines that belong to a real artifact.** Skip
  `*/nros-metadata/*` (probes), or take the link.txt set from the fixture
  manifest rather than a filesystem sweep.
* **Ignore what git ignores**, or at minimum do not fail on it. `git ls-files`
  / `git status --porcelain` know what is tracked; a `find` does not.
* **Bound the walk.** `-prune` on `target/`, `build-*/`, `.git/`, or drive it
  from the coordinates the lane just built — the same argument that made fixture
  builds lane-scoped in #0393.

A freshness rule would also work: a link line older than the build it is meant
to describe is not evidence about it. That is the same reasoning as the
staleness probe in #0445.

## Not

* Not a real duplicate-allocator defect in this tree. The eight fixture families
  all built and linked green in the same run; only the July probe residue is
  flagged, and it is regenerated from scratch when a probe actually runs.
* Not caused by the #0623/#0626/#0634 work in flight when it surfaced — those
  touch priorities, a typedef and a `-L` flag, none of which appear on a
  probe's link line.

## Found by

A tier-2 sweep. The lane's eight families were green and the run still failed at
the end, which is the most expensive place to discover a scoping problem.
