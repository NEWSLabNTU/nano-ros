---
id: 645
title: "The provider scan pruned `build` and `target` by EXACT name, so it walked every `build-*` root — 55 % of the directories it visited"
status: resolved
type: performance
area: cli, build
related: [issue-0641, phase-340]
---

## How it was found

Issue 0641 removed the subprocess cost from `nros sync` and left the question
"what is the remaining 0.26 s". Re-profiling answered it — after the probe was
gone, **I/O was co-dominant with subprocesses**:

```
 33.69 %  wait4      84 calls
 32.73 %  statx   34,172 calls   29,296 ENOENT   (86 % failing)
  7.07 %  getdents64 13,946
  6.37 %  openat  11,858
```

Tracing the failing `statx` paths gave the shape immediately:

```
7113  NROS_IGNORE
7113  COLCON_IGNORE
7113  AMENT_IGNORE
6636  package.xml
```

**7113 directories walked**, four `stat`s each, in a workspace with ~19 source
packages. 3923 of them (55 %) were build output.

## Cause

`provider_scan::PRUNED_DIRS` exists for exactly this and names `"build"` and
`"target"` — matched with `PRUNED_DIRS.contains(&name)`, i.e. **exact equality**.
Almost none of this tree's build roots are called `build` or `target`:

```
examples/workspaces/mixed/build-workspace-fixtures/
examples/workspaces/mixed/build-workspace-fixtures-freertos/
…/target-<coord>/            (phase-340's shared cargo groups)
```

So the list was right about what to skip and wrong about how to name it, and the
walk descended into every one — including their `_deps/`, `cargo/`,
`CMakeFiles/` and staged `src/` copies.

The staged copies are the reason this was invisible: they contain REAL
`package.xml` files, so the scan found packages rather than junk. They were
duplicates of the source tree it had already scanned.

## Fix (2026-08-16)

`PRUNED_DIR_PREFIXES = ["build-", "target-"]` beside the exact list, behind one
`is_pruned_dir()` predicate.

### Measured, `examples/workspaces/mixed`

| | before | after |
| --- | --- | --- |
| directories walked | 7113 | **1590** |
| `statx` | 34,172 (29,296 ENOENT) | **12,080** (7,204) |
| `getdents64` | 13,946 | **2,902** |
| `statx` share of time | 32.7 % | **9.4 %** |

End to end, with issue 0641:

| | original | after 0641 | after this |
| --- | --- | --- | --- |
| 22-workspace sync loop | 7.0 s | 5.1 s | **3.6 s** |
| `regenerate-bindings.sh` | 12.8 s | 10.9 s | **9.4 s** |

`regenerate-bindings.sh` runs at the head of every fixture build.

## The cost of the rule, stated

A real package directory may not be named `build-*` or `target-*`. That is
already the convention (`build/` and `target/` are pruned outright, and
`examples/**/target-*/` is globally gitignored), and two unit tests pin both
directions — the roots that cost the walk are pruned, and `builder_pkg`,
`buildings`, `targeting_pkg`, `targets` are not.

The alternative — dropping an ignore marker into each build root as it is
created — was rejected: it needs every creator to remember, which is the failure
mode markers already have here.

## What is left

`wait4` is now 55 % of a much smaller total: ~85 subprocesses per sync, and
~726 `execve`s (the PATH search for each). That is the next thing to look at if
sync needs to get faster again; this issue did not touch it.
