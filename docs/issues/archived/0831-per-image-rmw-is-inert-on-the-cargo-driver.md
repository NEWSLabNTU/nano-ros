---
id: 831
title: "`[image.<id>].rmw` configures nothing on the cargo driver — and a
  workspace fixture row's `rmw` does not either, so two tier-2 coordinates
  test zenoh while claiming cyclonedds and XRCE"
status: resolved
type: bug
area: build
related: [rfc-0065, phase-383, issue-0828, issue-0482, issue-0270, issue-0517]
resolved: 2026-08-28
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

## Fix

Two halves, because naming the backend was not enough to get only it.

**1. The facade reads the image.** `[image.<id>].rmw` selected nothing because
the backend comes from the `<entry>_nros_selection` facade, which read
`[system] rmw`. But the facade is already keyed per ENTRY, and an image NAMES an
entry (`package_name(image_id)`), so per-entry is per-image and the only thing
missing was the lookup. `facade::image_rmw` matches FORWARD —
`package_name(id) == entry_name` — never by stripping `_entry`, because that
function replaces `-`, `.` and `/` with `_` and the reverse is ambiguous.
`[image_defaults]` is folded first; no image naming the entry falls back to
`[system] rmw`, which is what hand-written entries have always used.

The refusal that stood in for this is gone.

**2. The board's default RMW is carved out.** Naming `rmw-cyclonedds` while
`nros-board-linux`'s `default = ["rmw-zenoh"]` still applied got BOTH — cargo
unions features and cannot subtract a default (issue 0270). The image then
carried two backends and the runtime refused to choose:

```
[ERROR] nros: cannot select an RMW backend — more than one RMW backend is
registered and no $NROS_RMW selector was set
```

Honest, and not a working image. So the facade and the generated entry both
declare the board `default-features = false`, and the facade RE-SUPPLIES the
board's defaults minus any `rmw-*`. Re-supply is not optional: boards do not put
the same things there — `nros-board-esp32-qemu` and `nros-board-mps2-an385`
default to `["ethernet", "rmw-zenoh"]`, and the NuttX boards to
`["image-runtime"]`, which carries two lang items. A blanket
`default-features = false` would have dropped `ethernet` and the panic handler
along with the backend. `crate_default_features` reads the list from the board
crate, which is the authority on what it declares.

Both declarations have to be silent about the RMW; one is not enough, because
the union restores it.

## Measured after the fix

`nm` on the three rust-workspace native images, same command as above:

| image | `dds_` | `uxr_` | zenoh `_z_` | size |
| --- | --- | --- | --- | --- |
| `native` (zenoh) | 0 | 0 | 777 | 8.5M |
| `native_cyclonedds` | 328 | 0 | **0** | 7.3M |
| `native_xrce` | 0 | 232 | **0** | 6.5M |

Exactly one backend each, and each one the declared one. Both new images run and
select their backend (they then fail for want of a peer — a Cyclone participant,
an XRCE Agent — which is the expected standalone behaviour, not a selection
failure).

Mutation-checked: deleting `rmw = "cyclonedds"` from the image and re-running
`nros sync` reverts the facade to `features = ["rmw-zenoh"]`, so the wiring is
load-bearing rather than incidental.

## The fixture rows

`examples/workspaces/rust` gained `[image.native_cyclonedds]` and
`[image.native_xrce]`, each declaring its backend, and the two rows now name
them. That is what makes `linux,rust,cyclonedds` and `linux,rust,xrce` real
coordinates instead of two claims about a zenoh binary. No coverage was dropped:
the rows kept their RMW and gained an image that honours it.

## The claim is now checked

Per this issue's own prescription — *add a runtime assertion rather than
trusting the coordinate; the artifact knows*:
`packages/testing/nros-tests/tests/rmw_coordinate_truth.rs` asserts, for every
`[[workspace_fixture]]` row naming zenoh/cyclonedds/xrce, that the row's own
binary links that backend's C namespace and (outside a declared bridge) no
other. 93 artifacts checked on the native lane.

Three false-positive classes were found by RUNNING it, and all three are
narrowings the gate needed rather than exemptions it was given:

* a **bridge** links two backends by declaration (`from = "zenoh:zen"`,
  `to = "cyclonedds:dds"`), so exclusivity does not apply — read from the
  bringup, not kept as a list of row ids;
* a **multi-row leaf** shares one `target/` across rows with different RMWs
  (issue 0517), so no single binary there can satisfy both;
* a **rename** leaves an orphan beside the new binary (issue 0215's class).

Naming the row's binary from the manifest removes all three at once.

The gate carries its own negative control on the normal path
(`the_symbol_counter_reads_local_and_global_text_symbols`), because the symbol
counter was wrong twice while this was being written — first matching only
GLOBAL `T` symbols, which called the bridge's 350 local `t dds_` entries an
absent backend; and a substring match would have counted `U` imports and `d`
data, making the gate unfailable.

## Still open, tracked elsewhere

The two bridge workspaces remain unmigrated: a bridge needs two backends in one
image, which a single `[image.<id>].rmw` cannot express. That is a per-image
backend LIST, not this bug, and the gate is correct to exempt them meanwhile.

## Sweep

```sh
# the facade reads the image, and both declarations carve out the default
grep -n 'image_rmw\|crate_default_features' packages/cli/nros-cli-core/src/orchestration/facade.rs
grep -n 'default-features = false' packages/cli/nros-cli-core/src/builder/entry.rs

# the claim, checked
cargo nextest run -p nros-tests -E 'binary(rmw_coordinate_truth)'
```
