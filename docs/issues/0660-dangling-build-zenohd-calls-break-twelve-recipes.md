---
id: 660
title: "phase-362 W4 deleted the `build-zenohd` recipe and left twelve callers, so twelve `just` recipes fail on invocation"
status: open
type: bug
severity: high
area: build, testing
related: [phase-362, issue-0374]
---

## Symptom

```
$ just native test-rmw
error: Justfile does not contain recipe `build-zenohd`
$ just native test-c
error: Justfile does not contain recipe `build-zenohd`
$ just native test-large-msg
error: Justfile does not contain recipe `build-zenohd`
```

The recipe dies before doing anything. Twelve call sites across three files:

```
just/native.just         9   (test, test-large-msg, test-rmw, test-ros2,
                              test-ros2-params, test-ros2-lifecycle,
                              test-native-api, test-c, test-cpp)
just/qemu-baremetal.just 1
just/zephyr-dev.just     2
```

## Cause

phase-362 W4 retired the vendored router — correctly, and that is what the phase
is for. Its own note records what it removed: *"Submodule, `just/zenohd.just`,
…"*. The callers were not removed with it, and `just` resolves a recipe
reference only when the recipe RUNS, so nothing failed at parse time.

## Why no gate caught it

* `just check` never invokes these recipes;
* `just ci` runs `test-all`, not the per-family `native test-*` recipes, so tier 1
  is green with all twelve broken;
* `check-doc-refs` covers docs, not justfile recipe references.

The class — "a deleted recipe leaves live callers" — has no gate at all, which is
the part worth fixing rather than just the twelve lines. `just --summary` knows
every recipe name and every `just <name>` inside a recipe body is greppable, so
the check is cheap.

## What the callers should become

Not a mechanical deletion. Each site called `build-zenohd` to guarantee a router
before an interop test, and under RFC-0075 the router now comes from the ROS
installation (`ros2 run rmw_zenoh_cpp rmw_zenohd`). So each site is either:

* **dropped**, where `ZenohRouter` already starts the ROS router on demand
  (phase-362 W1 pointed the interop lanes at it), or
* **replaced** by whatever preflight the new router needs — and if that is
  nothing, the line goes.

`ZenohRouter` is the only sanctioned spawner (`check-zenohd-spawn-sites`, issue
0573), so the answer is probably "drop", but it should be decided per lane rather
than assumed: `qemu-baremetal.just` and `zephyr-dev.just` are not the interop
lanes W1 converted.

## Not yet checked

Whether any of the twelve recipes has been run since W4 landed. If none has, the
breakage is invisible rather than newly introduced — and that is itself the
argument for the gate, since these are the recipes a developer reaches for by
hand (`just native test-c`) rather than the ones CI drives.
