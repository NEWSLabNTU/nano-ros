---
id: 1638
title: "A Zephyr leaf's CMAKE_MAKE_PROGRAM comes from an inherited PATH entry in ANOTHER checkout, so a worktree build caches the parent checkout's ninja"
status: open
type: bug
area: [build, zephyr, multi-session]
severity: low
found: 2026-10-02
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
