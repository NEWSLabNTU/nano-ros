---
id: 660
title: "phase-362 W4 deleted the `build-zenohd` recipe and left twelve callers, so twelve `just` recipes fail on invocation"
status: resolved
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

## Fixed (2026-08-17)

**The twelve callers are gone.** All twelve had the identical shape — a
`just build-zenohd` line immediately before running tests — and all twelve exist
to guarantee a router. `ZenohRouter` now resolves `rmw_zenohd` from `/opt/ros`
itself (`process::ros_zenohd_path`, phase-362 W1) and errors with a named remedy
when it is absent, so the line is simply dropped: nothing replaces it.

**`tests/zephyr/run-c.sh` was the one live consumer of the deleted path** and
would have kept failing after the recipe fix — it hard-coded
`$PROJECT_ROOT/build/zenohd/zenohd`. It now resolves the ROS router the same way
the Rust fixture does (`NROS_RMW_ZENOHD`, else `$ROS_DISTRO`, else the newest
`/opt/ros/*`), verified to find
`/opt/ros/humble/lib/rmw_zenoh_cpp/rmw_zenohd` on this host.

Two things there were NOT mechanical substitutions, and both would have been
silent:

* `rmw_zenohd` **ignores its argv**, so `--listen tcp/127.0.0.1:7556
  --no-multicast-scouting` had to become `ZENOH_CONFIG_OVERRIDE` entries —
  `;`-separated, `=` where the CLI used `:`. The translation is the one
  `fixtures::zenohd_router::router_command` documents and verified against the
  installed binary.
* the availability probe was `"$ZENOHD" --version`, which with an argv-ignoring
  binary **starts a router** rather than reporting a version. It is an
  executable-file test now.

`status_events_matrix.rs`'s skip message named `build/zenohd/zenohd` as a place
it looked; corrected to say the vendored router is gone and name the remedy.

## The gate, and what it found

`scripts/check-just-recipe-refs.py`, wired into `just check`: every
`just <recipe>` in a recipe body must name a recipe that exists.

It parses recipe DEFINITIONS rather than `just --summary`, which was the first
attempt and wrong — `--summary` omits `[private]` and `_`-prefixed recipes, and
bodies call those constantly (`just _count-real-failures`), so it reported a
dozen live recipes as missing. Interpolated targets (`just {{x}}`) are skipped
deliberately: the name is not known until run time, and guessing produces false
positives on the one shape a human cannot check either.

Mutation-tested both ways: reintroducing one `just build-zenohd` is caught by
file and line; making the namespace lookup return nothing fails with "parsed NO
recipes … a gate with an empty expectation passes forever" rather than passing.

**A second clause came out of the fix.** With the `build-zenohd` error gone,
`just native test-rmw` reached cargo and said `no test target named 'rmw'`. The
first error had been masking a second, older one, so the gate also checks
`-p <pkg> --test <target>` against `<pkg>/tests/<target>.rs`. Two dead targets:

| recipe | target | deleted by |
| --- | --- | --- |
| `native test-rmw` | `rmw` | `6e56ce202` (phase-115.L.7/L.8) |
| `native test-dds-ros2` | `dds_ros2_interop` | `ad5454d11` (phase-169.3) |

`ad5454d11`'s own message says it deleted the tests and "strip[ped] Cargo
wiring" — the recipe was the piece it missed. Both recipes were dead far longer
than the router calls, and both are deleted: a recipe that cannot run looks like
coverage and is not. The only reference to either is an archived phase doc.

## Verified

`just native test-c` runs again — 11 tests, 7 passed, 4 `[SKIPPED]`. The four
are stale-fixture skips, not failures: these recipes use bare `cargo nextest`,
which counts `nros_tests::skip!` panics as FAILURES because only `just
test-all`'s junit rewrite converts them (CLAUDE.md records this trap). Checked
rather than reported as reds.

`just check` green, including the new gate: 235 root recipes, 19 modules, every
`just <recipe>` and every `--test` target resolves.

## Worth keeping

The class had no gate because each half is invisible in the same way: `just`
resolves a recipe name only when the recipe RUNS, and cargo resolves `--test`
only then too. Both are cheap to check statically, and neither was, which is why
two deletions from different phases could sit broken for months behind a tier-1
green.
