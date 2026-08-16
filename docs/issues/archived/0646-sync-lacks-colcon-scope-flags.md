---
id: 646
title: "`nros sync` had no way to scope the provider scan, so every sync re-walked the whole nano-ros checkout"
status: resolved
type: enhancement
area: cli, build
related: [issue-0641, issue-0645]
---

## Context

Issues 0641 and 0645 made `nros sync` cheaper. Profiling what was left showed
the remaining walk is almost entirely NOT the workspace:

```
nros sync examples/workspaces/mixed
  directories walked  : 1590
  the workspace itself:   20
  the nano-ros tree   : 1570
```

`default_search_path` puts the nano-ros checkout in front of every workspace as
a provider underlay — that is deliberate and load-bearing (49 provisions:
boards, platforms, RMWs). But it is rediscovered on every sync, and
`regenerate-bindings.sh` runs 22 of them at the head of a fixture build.

## Why the obvious fix is wrong

The 49 provisions live in three subtrees — `config/`, `packages/boards/`,
`packages/rmw/` — so narrowing the underlay scan to those would remove almost
all of the cost. `packages/cli/CLAUDE.md` forbids it:

> **`nros` is a generic tool** — it must not learn the nano-ros directory
> layout.

A caller CAN know the layout, though. So the scope becomes an argument rather
than a constant, which is also how colcon does it (`--base-paths`, `--paths`).

## What landed

Three flags on `nros sync`, deliberately sharing `nros ws providers`' vocabulary
so the index sync WRITES stays the index that command READS —
`provider_search_path` exists precisely because two spellings of the root list
make every read "built for other roots":

| flag | effect |
| --- | --- |
| `--base-paths PATH...` | REPLACES the search path (colcon's flag, repeatable, order = search order) |
| `--nano-ros-root PATH` | replaces only the underlay root, keeping the workspace |
| `--no-provider-index` | skip the scan and the index entirely |

Precedence is most-explicit-first, in `provider_roots_for_sync`.

`--no-provider-index` is the one with real leverage, and it is honest about what
it is: the index is a CACHE for later commands, not an input to this sync, so a
caller that will not read it can decline the work. Same shape as the existing
`--no-metadata`.

### Measured

```
nros sync <ws>                     19 provider(s), 483 package(s) scanned
nros sync <ws> --base-paths <ws>    0 provider(s),  18 package(s) scanned
nros sync <ws> --no-provider-index  index skipped

directories walked, --no-provider-index:  1590 -> 0
wall clock:                              0.194 s -> 0.175 s
```

The wall-clock gain is small, and that is the useful part of the measurement:
after 0645 the scan is no longer the bottleneck. `wait4` is **62 %** of a
`--no-provider-index` sync — ~85 subprocesses per run. These flags remove the
directory walk completely and the process is still dominated by process
spawning, which is where the next work is.

## The sharp edge, stated

Dropping a root that DOES hold a provider makes its boards unresolvable, and
nothing fails until something asks for that board. `--base-paths` is an override
for a caller that knows the tree — a build script looping over workspaces — not
a tuning knob. Naming a root with no providers is fine (the default workspace
root is often exactly that).

## Rejected on measurement

Enumerating packages through the git index instead of walking
(`git ls-files -- '*package.xml'`), the same substitution
`scripts/build/source-manifest.sh` documents. Implemented, with an equivalence
test that immediately earned its keep — git has no notion of ancestry, so it
found the fixture packages under `nros-rmw-cyclonedds/tests/types/` that the
walk's stop-at-a-package rule hides. That was fixable, and all 147 tests passed.
It was then **slower**: `statx` 12,080 -> 16,282, the 22-workspace loop
3.6 s -> 4.2 s. Re-applying the walk's ancestry rules per candidate costs more
than the walk it replaces. Reverted; recorded here so it is not tried twice.

## How many syncs does a workspace actually need? One. It gets up to 22.

Measured with a transparent counting shim on `$NROS_CLI` (the documented
override in `nros_cli_bin`, so it intercepts every site) across a clean
`just build-test-fixtures lane=native`:

```
185 invocations, 69 distinct targets
```

so **116 of 185 (63 %) are repeats**. The distribution:

```
 42 target(s) x1        4 target(s) x6
 12 target(s) x2        1 target(s) x7
  1 target(s) x3        3 target(s) x8
  2 target(s) x4        2 target(s) x10
                        1 target(s) x11
                        1 target(s) x22
```

The cause is checkable against the manifest — repeats track `fixtures.toml`
ROWS, because `fixtures-build.sh:342` syncs `$dir` once per row and many rows
share one workspace directory:

| workspace | rows | syncs |
| --- | --- | --- |
| `features` | 24 | 22 |
| `rust` | 16 | 11 |
| `cpp` | 13 | 10 |
| `c` | 13 | 10 |
| `mixed` | 10 | 8 |
| `safety` | 7 | 7 |

(Counts fall slightly below row counts because the native lane filters some
rows out.)

`nros sync` is per-WORKSPACE and idempotent — its outputs are the generated msg
crates, the patch config and the resolved models, none of which vary by the
platform/rmw coordinate that distinguishes one row from another. So the loop is
asking the same question up to 22 times.

**Not fixed here**, because it is a change to the build script's loop rather
than to the CLI, and it needs one thing checked first that this measurement does
not establish: whether anything BETWEEN rows invalidates a sync (a row-specific
env var, a `generated/` wipe by `nros_codegen_stamp_check_or_wipe`). If nothing
does, deduplicating by `dir` removes ~116 invocations per build — far more than
any per-sync optimisation in #0641, #0645 or this issue.

Two measurement notes worth keeping, since both cost a build:

* the first shim logged only the positional argument, so 83 cwd-based
  invocations recorded as `.`; a shim must resolve `$PWD` to attribute them;
* the second run failed at `rc=2` — self-inflicted, and exactly the treadmill
  CLAUDE.md documents: uncommitted CLI edits made the in-tree `nros` stale
  mid-build. Build the CLI BEFORE instrumenting a build that uses it.
