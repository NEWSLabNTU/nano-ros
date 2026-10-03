---
id: 1638
title: "A Zephyr leaf's CMAKE_MAKE_PROGRAM comes from an inherited PATH entry in ANOTHER checkout, so a worktree build caches the parent checkout's ninja"
status: resolved
type: bug
area: [build, zephyr, multi-session]
severity: low
found: 2026-10-02
resolved_in: 2026-10-03
related: [issue-1280, issue-1596, issue-1387]
---

## What happened (measured while verifying issue 1596)

A worktree's first `just zephyr build-fixtures` (filter `build-c-talker-zenoh`)
wrote the leaf into the worktree's own build root — correct since 1596 — and
`check-zephyr-workspace-foreign-checkout` then reported it RED:

```
1 cached value(s) across 1 of 1 build dir(s) name ANOTHER checkout
  CMAKE_MAKE_PROGRAM=/home/aeon/repos/nano-ros/third-party/ninja/ninja
```

The parent shell's `PATH` starts with `/home/aeon/repos/nano-ros/third-party/make`
and `…/third-party/ninja` (the MAIN checkout's untracked tool dirs). The leaf
runner puts `$(nros sdk-path ninja)/bin` first on `NROS_ZEPHYR_TOOL_PATH`, but
`nros sdk-path ninja` printed `~/.nros/sdk/ninja/1.13.2`, a directory that does
not exist on this host, so the prepend was inert and PATH resolution fell
through to the other checkout's ninja.

Rebuilt with those PATH entries removed (`CLEAN_PATH=1` in the 1596 driver),
the cache read `/usr/bin/ninja` and the gate went green. So the gate was right:
it is an issue-1280 crossing through `PATH`, which `reroot-checkout-path` does
not cover (it re-roots named variables, not PATH entries).

## Two defects

1. `sdk-path ninja` names an unprovisioned store dir and nothing says so — the
   prepend silently does nothing.
2. Inherited PATH entries inside a DIFFERENT checkout are not re-rooted or
   dropped, so any tool resolved by name can come from the parent checkout.

## Direction

Have the Zephyr runner refuse (or warn) when the SDK path it prepends does not
exist, and extend issue 1280's rule to PATH: an entry inside another nano-ros
checkout is re-rooted or dropped for recipe subprocesses.

## Resolution

**Both defects fixed, plus the sibling the first fix exposed.**

1. **Inherited PATH entries go through issue 1280's rule, per entry.**
   `nros_reroot_checkout_pathlist` in `scripts/lib/checkout-paths.sh` (the one
   home of the rule) walks a `:`-list: an entry outside any checkout, or inside
   this one, is kept; one inside ANOTHER checkout is re-rooted here when this
   checkout has that directory, and DROPPED when it does not — a missing PATH
   directory is skipped silently by every lookup, so keeping either spelling is
   the crossing. The justfile's `export PATH` goes through it
   (`scripts/lib/reroot-checkout-pathlist.sh`, the sibling of
   `reroot-checkout-path.sh`), so every recipe and every tool it resolves by
   name sees the re-rooted list. The owning-checkout walk is inline, forking
   only for owned entries, because `just` evaluates it on every invocation.
2. **A missing store pin is said, not silently skipped.**
   `nros_sdk_tool_path_prefix` (`scripts/lib/sdk-tool-path.sh`) returns only the
   store `bin` dirs that EXIST and, for each pin that is absent, prints the
   `nros setup --tool` command and the binary PATH resolves instead. The
   Zephyr runner (`zephyr-ci.just`'s build-fixtures, `zephyr-dev.just`) uses it.
3. **The workspace-fixture make pool puts its pinned make first for its
   children** (`scripts/build/workspace-fixtures-build.sh`), as
   `fixture-make-driver.sh` and `build-all-jobserver.sh` already did. Dropping
   the parent checkout's `third-party/make` (a make 4.4) from a worktree's PATH
   exposed it: a cmake Makefiles leaf configured with the system `gmake` 4.3 is
   a CLIENT of the pool's fifo jobserver and dies with `gmake[1]: *** internal
   error: invalid --jobserver-auth string 'fifo:/tmp/GMfifo…'` — measured on
   this host while building FreeRTOS fixtures for issue 1658 with the
   parent-checkout entries removed.

### Measured

- `just --evaluate PATH` from this worktree with the parent shell's PATH:
  `<main>/third-party/make` and `<main>/third-party/ninja` are gone,
  `<main>/packages/cli/target/release` and `<main>/scripts/bin` come out
  re-rooted onto the worktree, every out-of-tree entry is unchanged.
- `NROS_ZEPHYR_FIXTURE_FILTER=build-c-talker-zenoh just zephyr build-fixtures`
  from that same inherited environment (first entries
  `/home/aeon/repos/nano-ros/third-party/{make,ninja}`): the lane prints
  `note: the pinned ninja is not provisioned (no ~/.nros/sdk/ninja/1.13.2/bin)
  … resolves ninja by PATH instead: /usr/bin/ninja`, the leaf builds
  (`zephyr.exe`), and its cache reads `CMAKE_MAKE_PROGRAM:FILEPATH=/usr/bin/ninja`
  — where the issue measured `/home/aeon/repos/nano-ros/third-party/ninja/ninja`.
  `just check zephyr-workspace-foreign-checkout`: `ok — 2 subject(s) examined …
  none names another checkout`.
- `check-inherited-checkout-paths` gained a PATH probe (re-root, drop, keep).
  Negative control: with the old `export PATH` line it reports both "a PATH
  entry naming another checkout was not re-rooted" and "… was not dropped".

### Not measured

- The workspace-pool fix (3) on a FRESH configure: this worktree's two
  affected caches had already been repaired by `cmake -DCMAKE_MAKE_PROGRAM=…`
  re-configure while diagnosing it. A cache configured BEFORE this fix with the
  system `gmake` keeps it — `CMAKE_MAKE_PROGRAM` is cached — and needs that
  same re-configure (never a wipe).
- The advisory `foreign-checkout-root.sh` still reports only named variables;
  a crossing that exists only on PATH is now repaired silently rather than
  announced.
