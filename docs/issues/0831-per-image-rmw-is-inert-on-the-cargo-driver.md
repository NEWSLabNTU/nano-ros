---
id: 831
title: "`[image.<id>].rmw` configures nothing on the cargo driver — and a
  workspace fixture row's `rmw` does not either, so two tier-2 coordinates
  test zenoh while claiming cyclonedds and XRCE"
status: open
type: bug
area: build
related: [rfc-0065, phase-383, issue-0828, issue-0482]
---

## Problem

RFC-0065 D6 makes `[image.<id>]` the buildable unit and its declaration the
SSoT. One of its keys does not reach the build.

* **cmake driver** — `image.rmw` becomes `-DNROS_RMW=<rmw>` and selects the
  backend. Correct.
* **cargo driver** — `image.rmw` reaches exactly one thing: `coordinate()`,
  which names the build DIRECTORY. The backend comes from the
  `<entry>_nros_selection` facade `nros sync` generates from the bringup's
  `[system] rmw`, and nothing in that chain consults the image.

So `[image.native_cyclonedds] rmw = "cyclonedds"` produces
`build/posix-cyclonedds/native_cyclonedds_entry` containing a **zenoh** binary.
A directory named for a backend it does not contain reads as coverage.

## The same hole one layer up, and it is live

`examples/fixtures.toml` has the identical shape, and it is not hypothetical:

```
[[workspace_fixture]]
id = "workspace-rust-native-cyclonedds"
platform = "linux"
lang = "rust"
rmw = "cyclonedds"
dir = "examples/workspaces/rust"
target_dir = "target-fixtures-cyclonedds"
```

Measured on the artifact that row builds:

```
strings target-fixtures-cyclonedds/nros-relwithdebinfo/native_entry | grep -ci cyclone   # 0
strings target-fixtures-cyclonedds/nros-relwithdebinfo/native_entry | grep -ci zenoh     # 1916
```

A row's `rmw` is read in exactly three places — `cmake_defs()` (cmake rows
only), `row_coord()`, and the label printer. **For a cargo row it is
coordinate metadata, not build configuration.** `workspace-fixtures-build.sh`
never mentions it.

`linux,rust,cyclonedds` is one of tier 2's fourteen coordinates. Tier 2
believes it covers rust-on-cyclonedds; it builds and runs zenoh. Same for
`workspace-rust-native-xrce`. This predates phase-383 — the migration to
`nros build` only made it visible, because an image declaring `rmw` put the
claim somewhere a reader looks.

## Mitigation in place

`nros build` now REFUSES a cargo image whose `rmw` differs from the bringup's
`[system] rmw`, naming both and pointing here. A loud refusal is strictly better
than a directory that lies, and it costs nothing today: no shipped image
declares a divergent rmw.

The fixture rows are deliberately NOT changed. Relabelling them `zenoh` would
silently drop two coordinates' worth of claimed coverage, and deleting them
would drop it outright — which of those is right is a coverage decision, not a
mechanical one.

## Fix

The backend is selected by the facade, so per-image RMW means a facade per
image rather than per entry name. `nros sync` generates
`generated/nros-selection/<entry>/`; an image is already the thing that names
an entry (`<image>_entry`), so the natural shape is for sync to read the
`[image.*]` table and emit one facade per image, with `[system] rmw` as the
default when an image declares none.

That also closes the fixture hole without touching the rows: once the image
carries the RMW, a row naming `[image.native_cyclonedds]` gets a cyclonedds
binary, and `row_coord()` stops being a claim nobody checks.

**Until then, add a runtime assertion rather than trusting the coordinate.**
The interop and matrix cells that name an RMW should assert the backend the
binary actually linked — the artifact knows, and `strings` proved it in one
command here.

## Sweep

```sh
grep -rn 'NROS_RMW\|image.rmw\|row_coord' scripts/build packages/cli/nros-cli-core/src/cmd/build.rs
grep -n 'rmw' scripts/build/workspace-fixtures-build.sh      # no hits: the driver never reads it
```
