---
rfc: 0135
title: "Board packages: one line switches the board, and the package carries everything"
status: Draft
area: boards
since: 2026-11
last-reviewed: 2026-11
implements-tracked-by: [phase-577]
amends: [0064, 0087, 0114, 0115]
supersedes: []
superseded-by: null
---

# RFC-0135 — Board packages

**Status:** Draft (2026-11-18). The direction and the seven items below were
approved by the user on 2026-11-18. The decisions D1–D9 are proposed and await
the user's review of this text. Tracked by phase-577.

## Summary

The goal is plug and play in the sense Arduino made familiar:

- **Switching boards takes one line.** An image names its board once, and that
  name is all that changes.
- **One package carries everything a board needs:** to build, link and
  configure an image, and to flash it, run it and watch its output.
- **An installed board package is found from anywhere,** without the user
  editing a search path.
- **A wrong choice is refused before anything builds,** with every reason it
  cannot work.

Most of the machinery exists (survey of main, 2026-11-18):

- **Board definition.** The `nros-board.toml` schema already has
  `[board.run]`, `[board.flash]` and `[board.console]`.
- **CLI verbs.** `nros run`, `nros flash` and `nros monitor` exist, and their
  runners are packages (RFC-0119).
- **Package shape.** A board is already a package with a `package.xml` that
  announces it (RFC-0064 D1, RFC-0087 D3).
- **Out-of-tree discovery.** Out-of-tree board packages are found through
  `$NROS_EXTRA_BOARD_PATH` and the workspace.

This RFC fills six gaps (D3–D8) and fixes the vocabulary (D1). It amends
RFC-0087 D8, which kept discovery source-time only.

## Terminology (D1)

User ruling, 2026-11-18. Each term names one thing, and only that thing.

| Term | Means | Is not |
| --- | --- | --- |
| **board package** | the self-contained, installable unit: a ROS package (`package.xml`) that provides one or more boards, and carries their board definitions, any bring-up code, examples and flash configuration | a crate; a status |
| **board definition** | one `nros-board.toml` entry: the facts about one board key (platform, target, toolchain, run, flash, capabilities, network) | the package around it |
| **board support** | a board's support **status** only: the word RFC-0114's registry generates ("Verified in emulator (daily)", "Builds", and so on) | the package; the code |
| **BSP** | the vendor's SDK layer (for example TI's Processor SDK RTOS, NXP's MCUXpresso SDK) that a board package wraps and names, and never ships | a nano-ros package |

"Board crate" stays meaningful only for the optional Rust crate inside a board
package. "Board descriptor" is retired in favour of "board definition". The
sweep is its own work item (phase-577 W9), not a precondition.

## Motivation

Five gaps, measured on main on 2026-11-18:

1. **Discovery stops at source trees.** Boards are found in
   `packages/boards`, in `$NROS_EXTRA_BOARD_PATH`, and in the user's workspace
   packages. An installed package is invisible: RFC-0087 D8 decided "discovery
   stays source-time", because nano-ros builds RTOS images with no install
   step. That reason holds for the user's own packages. It does not hold for a
   board package someone else published, which is installed exactly once and
   reused from every project.
2. **`nros board list` does not list boards.** It walks
   `packages/boards/nros-board-*` directories that hold a `Cargo.toml`. It
   misses crate-less boards (`linux`, `zephyr`), extra-root boards, workspace
   boards and installed ones. RFC-0133 already plans the fix.
3. **A board package cannot bring its own architecture profile.** `[arch.*]`
   profiles live in platform descriptors (`nros-platform.toml`). A board
   definition has no `[arch]` table, and RFC-0064 step 8, which would have
   given it one, was dropped on 2026-11-17. A board on a new core (for
   example a hard-float Cortex-R5F) therefore needs an in-tree edit
   (phase-575 W1).
4. **No version compatibility.** No board package declares which nano-ros
   versions it works with. The first sign of a mismatch is a build error deep
   in a dependency.
5. **No single pre-build check.** The checks are scattered: netstack
   resolution, the framework check, toolchain preflight, the `nros check`
   workspace lints, and build-script capability errors. Each reports one
   failure, in its own words, at a different stage. Nothing compares an
   image's needs with a board's definition as a whole, and there is no
   declarative check of RMW, transport or memory.

Two smaller gaps:

- **Flash methods.** Only two board definitions state `[board.flash]`, both
  `method = "west"`. No runner package exists for OpenOCD, probe-rs or
  picotool.
- **Examples.** These are organised by platform, not by board. Five boards have
  none at all.

## Design

### D2 — What a board package contains

```
my-board-package/
  package.xml               identity, nano-ros version range (D6), announcements
  nros-board.toml           one or more board definitions
  nros-platform.toml        optional: the package's own [arch.*] profiles (D4)
  src/ + Cargo.toml         optional: bring-up code (the board crate)
  flash/                    optional: OpenOCD scripts, probe configs (D3)
  examples/<name>/          at least one example per board key (D7)
```

- One package may provide several board keys. This is RFC-0115 D3's
  family package, for example one FreeRTOS package with a board definition
  per SoC.
- **The package never contains the BSP, and nano-ros never fetches it.** The
  board package names the BSP's root through its own variable. An outside
  board package gets no row in `nros-sdk-index.toml` (no `[gated.*]` entry):
  an index row is nano-ros's promise to provision and price a thing, and a
  third-party BSP under its vendor's licence is not ours to fetch. This
  follows RFC-0064's 2026-11-17 ruling. A missing BSP root is one of D8's
  reasons, reported with the rest.
- **Switching boards stays one line.** It is the image's `board =` in
  `system.toml`, with `nros build --board <key>` as a per-invocation override.
  Everything else follows from the board definition.

### D3 — Flash, run and monitor from the board definition

The existing tables become the whole contract:

- `[board.run]`: host, or an emulator (`qemu:<machine>`, `renode:<script>`,
  `fvp:<model>`).
- `[board.flash]`: the method, plus the method's arguments. Paths in the
  arguments are relative to the board package's directory.
- `[board.console]`: how to read the image's output (a serial device and baud
  rate, RTT, or semihosting).

Each method is a runner package (RFC-0119), as `qemu`, `renode`, `fvp`,
`west` and `serial` already are. This RFC adds three:

| Method | Runner package | Covers |
| --- | --- | --- |
| `openocd` | `nros_runner_openocd` | most Cortex-M and Cortex-R debug probes; arguments name the interface and target scripts |
| `probe-rs` | `nros_runner_probe_rs` | CMSIS-DAP, ST-Link and J-Link probes, with RTT for the console |
| `picotool` | `nros_runner_picotool` | RP2040 and RP2350 over USB, no probe |

`west flash` stays the method for Zephyr board definitions. A board package
may also ship its own runner package, for a vendor flasher (for example TI's
UniFlash), because runner discovery already works for any announced
`kind="runner"`.

`nros flash` and `nros monitor` therefore need no new design. What changes is
that every board package states these tables, and D8 checks that the named
method's runner is installed.

### D4 — A board package's own architecture profiles

A board package may ship a `nros-platform.toml` beside its board definitions,
holding `[arch.*]` profiles. It is read through the platform search path that
already reaches extra board roots and workspace package directories, and D5
extends that path to installed packages.

- **Additions only.** A profile name that collides with one already loaded is
  refused, with both sources named. A board package cannot silently change
  an in-tree profile.
- **Why not `[arch]` inside `nros-board.toml`:** that is RFC-0064 step 8,
  dropped on 2026-11-17 because the facts belong with the toolchain and
  platform files. This keeps them there, and only lets the file travel with
  the package.
- **Which profiles belong in a board package.** A profile that describes a
  CORE stays in-tree, so every board on that core reuses it. Phase-575 W1's
  hard-float `cortex-r5f` (Cortex-R5 with VFPv3-D16 and the AAPCS-VFP ABI) is
  one: it is the twin of the in-tree soft-float `cortex-r5`. A board package
  ships only a variant specific to its board, for example a different
  `-mfpu`. The collision refusal then stops a board package from redefining
  a core profile.

### D5 — Discovery through the ament index (amends RFC-0087 D8)

An installed board package registers itself in the ament index, and `nros`
reads the index from `$AMENT_PREFIX_PATH`.

- **Marker.** The package installs
  `share/ament_index/resource_index/nano_ros_boards/<package>`, and its
  contents under `share/<package>/`. This is the same index that ROS 2
  message packages already use, and that nano-ros already reads for them.
- **Roots, in precedence order:**
  1. the workspace's own packages;
  2. `$NROS_EXTRA_BOARD_PATH`;
  3. the in-tree `packages/boards`;
  4. installed packages, from `$AMENT_PREFIX_PATH`.

  A key found in more than one root resolves to the first and reports the
  shadowing (RFC-0087 D6's rule), so a checked-out copy always wins over an
  installed one.
- **Installation.** Any ament route installs a board package: `colcon build`
  plus `install`, a Debian package built with bloom, or copying into a prefix.
  A board package needs no compile step to install, because its build
  happens per image.
- **What D8 of RFC-0087 keeps.** The user's own packages stay source-time, and
  nothing requires sourcing a `setup.sh`. The amendment is narrow: installed
  board packages (and, through D4, their profiles) become one more root.
- **Listing.** `nros board list` shows every key from every root, with its
  package, root, platform and support status (RFC-0133 D1, RFC-0114). This
  is the Boards Manager view.
- **Not in this RFC:** a remote package index from which `nros` downloads
  board packages. apt and the ROS buildfarm already distribute ament
  packages. A remote index of our own is a later decision.

### D6 — Supported nano-ros version range

A board package declares the nano-ros versions it supports in `package.xml`:

```xml
<export>
  <nano_ros_provides kind="board" name="ti-j784s4-r5f-freertos"/>
  <nano_ros_requires nano_ros="&gt;=0.9, &lt;0.11"/>
</export>
```

- The range uses Cargo's requirement syntax, checked against the running
  nano-ros's version.
- Outside the range, the board is still listed, marked incompatible, and an
  image that selects it is refused by D8 with both versions named.
- A package without the tag is accepted with a warning, so today's packages
  keep working. In-tree board packages are exempt: they always match their
  own tree.

### D7 — One example per board key

- Every board key a package provides has at least one example under the
  package's `examples/`, using the template's talker/listener pair unless the
  board needs something else.
- `nros board check <package>` (today's `nros check <board-package>`) builds
  each example, and runs it where `[board.run]` names a host or an emulator.
- For in-tree boards this is RFC-0115 D3's "one worked configuration per book
  board", extended to every in-tree board key. The five in-tree boards with no
  example gain one.

### D8 — The pre-build compatibility check

Before any build step, `nros build` (and `nros check --board <key>` on its
own) compares the image with the board definition and reports every mismatch
at once:

```
error[NROS-....]: image `talker` cannot build for board `rpi-pico-w-zephyr`:
  - rmw `cyclonedds` needs threads; the board's capabilities say threads = false
  - transport `udp` is not among the board's links [serial, wifi]
  - 3 publishers + 2 subscriptions need ~148 KiB; the board's RAM is 264 KiB minus 180 KiB reserved
  - board package `pico-boards` supports nano-ros >=0.7, <0.8; this is 0.9.2
```

It checks, from facts that already exist:

| Need | Source |
| --- | --- |
| platform, framework, entry type | board definition; `image_framework` |
| RMW and its capability needs (threads, heap, atomics) | board `[board.capabilities]`; RMW provider facts |
| transports and links | image network configuration (RFC-0109); `[board.net]` |
| memory | the sizing descriptor (RFC-0100) against a board memory budget, which the board definition gains |
| toolchain present; the BSP root the package names is set and exists | today's preflight; the board package's own variable |
| nano-ros version | D6 |
| flash method's runner installed | D3 |

- **Each existing check stays the authority for its own fact.** D8 calls them
  and collects their verdicts; it adds no second copy of any rule.
- **Error codes.** Each reason carries a diagnostic code (RFC-0129).
- **What is new in data:**
  - a memory budget in the board definition;
  - each RMW provider's capability needs, stated as facts.

## Alternatives considered

- **Keep discovery source-time only, and document `$NROS_EXTRA_BOARD_PATH`.**
  This works today, but every project must point at every board it uses,
  which is the opposite of plug and play. Rejected.
- **A nano-ros board registry served over HTTP**, like Arduino's
  `package_index.json`. It duplicates what apt and the ROS buildfarm already
  provide for ament packages. Deferred, not rejected.
- **`[arch]` inside the board definition.** Rejected by RFC-0064's 2026-11-17
  ruling. D4 gets the same reach without moving the facts.
- **The version range in `nros-board.toml`.** It is a property of the package
  (one range for every board it provides), so it belongs in `package.xml`,
  where installation tools can also read it.

## Phased plan

Tracked by phase-577. The first consumer is phase-575 W2 (an out-of-tree TI
R5F board package with a vendor SDK), built to this shape rather than a second
time.

## Changelog

- 2026-11-18: drafted from the user's approved brief.
