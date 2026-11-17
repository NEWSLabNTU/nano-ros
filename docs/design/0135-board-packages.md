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

This RFC fills the gaps (D2–D9), sets out the user flow, and fixes the
vocabulary (D1). It amends
RFC-0087 D8, which kept discovery source-time only.

## Terminology (D1)

User ruling, 2026-11-18. Each term names one thing, and only that thing.

| Term | Means | Is not |
| --- | --- | --- |
| **board package** | the self-contained, installable unit: a ROS package (`package.xml`) that provides one or more boards, and carries their board definitions, any bring-up code, examples and flash configuration | a crate; a status |
| **board definition** | one `[[board]]` entry in a `nros-board.toml`: the facts about one board key (platform, target, toolchain, provisioning, run, flash, console, capabilities, network). A package's `nros-board.toml` holds one board definition per board key. | the file; the package around it |
| **board key** | the name an image selects a board by (`board = "<key>"`), unique across every root; one board definition answers to it | a package name |
| **platform** | the RTOS or OS family a board definition builds for (`zephyr`, `freertos`, `nuttx`, `threadx`, `linux`, bare metal) | a board |
| **board support** | a board's support **status** only: the word RFC-0114's registry generates ("Verified in emulator (daily)", "Builds", and so on) | the package; the code |
| **BSP** | the vendor's SDK layer (for example TI's Processor SDK RTOS, NXP's MCUXpresso SDK) that a board package wraps and names, and never ships | a nano-ros package; **not the board package, despite what the letters expand to** |

Two rules for prose, because the terms collide when read aloud:

- **Never expand BSP.** "Board support package" reads as either "board
  package" or "board support". Write "the vendor SDK (BSP)" on first use and
  "vendor SDK" after.
- **Write "support status" in sentences.** "Board support" is the column and
  heading word only. "The board's support status is Builds", never "the
  board's board support is Builds".

"Board crate" stays meaningful only for the optional Rust crate inside a board
package. "Board descriptor" is retired in favour of "board definition". The
sweep is its own work item (phase-577 W9), not a precondition.

## Motivation

Six gaps, measured on main on 2026-11-18:

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

6. **`nros setup` cannot serve an outside board.** It resolves a key only
   through the `[board.*]` rows of `nros-sdk-index.toml`. For an unknown key
   it tells the user to add a row there, which means editing nano-ros. D2
   gives an outside package no index row, so as things stand every
   out-of-tree or installed board would fail at `nros setup`, before any
   check runs. (Book-side review, 2026-11-18.)

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
  cmake/nano-ros-board-<key>.cmake
                            optional: the board's CMake module, for C/C++ images
  flash/                    optional: OpenOCD scripts, probe configs (D3)
  examples/<name>/          at least one example per board key (D7)
```

- **`package.xml`**, in the shape this RFC asks for:

  ```xml
  <package format="3">
    <name>ti_j784s4_r5f_boards</name>
    <version>0.1.0</version>
    <exec_depend>nros_runner_openocd</exec_depend>
    <export>
      <build_type>nros_cargo</build_type>
      <nano_ros_provides kind="board" name="ti-j784s4-r5f-freertos"/>
      <nano_ros_requires nano_ros="&gt;=0.9, &lt;0.11"/>
    </export>
  </package>
  ```

  The `exec_depend` on its flash method's runner package lets apt or rosdep
  bring the runner with the board.
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
- **What nano-ros provisions for an outside board, and how `nros setup`
  finds it.** The line runs between the toolchain and tools, which nano-ros
  provisions, and the BSP, which it never fetches. The board definition
  already has the field for the first: `[board.provisioning] tools`, which
  names `nros-sdk-index.toml` packages by index name (the field was called
  `gated` until 2026-09-06). `nros setup <key>` therefore resolves the key
  through the board catalog, across all four roots of D5. It takes the
  package list from the in-tree index row where one exists, and otherwise
  from the board definition's `[board.provisioning] tools` (for example the
  Arm GNU toolchain, plus OpenOCD for the flash method). An unknown key is
  refused with near matches and the roots searched, never with "add an index
  row".
- **The board's CMake module travels with the package.** A FreeRTOS, NuttX
  or bare-metal C/C++ image includes the board's CMake module (its port,
  config directory, linker script and startup). Today the platform files
  build that path inside the nano-ros tree (`cmake/board/nano-ros-board-<key>.cmake`,
  four hand-built sites), so an outside board package cannot supply one
  without patching nano-ros. Found building phase-577 W8 on 2026-11-18. The
  rule:
  - a board package may ship `cmake/nano-ros-board-<key>.cmake`, and may
    refer to its own files through `CMAKE_CURRENT_LIST_DIR`;
  - the CLI knows the resolved board definition's directory, so it passes
    `-DNROS_BOARD_MODULE=<that file>` on the configure command it emits (the
    cmake road's carrier, per the canonical build path; never an environment
    variable);
  - one helper, `nros_board_module_path()`, replaces the four hand-built
    paths. It takes `NROS_BOARD_MODULE` when set and falls back to the
    in-tree file, so in-tree boards are unchanged;
  - a configure run by hand for an outside board, without the variable, is
    refused with a message that names `NROS_BOARD_MODULE` and the key,
    rather than failing on a missing file.
- **Every road resolves an outside board key the same way.** Found building
  phase-577 W8 (2026-11-18), each fixed as a class:
  - **Board key to family.** The entry pipeline's compiled-in board-key
    table refused any outside key. One function now answers "which platform
    family is this key": the table first, then the board catalog, where an
    outside board's family is its definition's `platform`. A key that
    neither knows keeps the table's error.
  - **The package's own directory in `cargo_config`.** `${board_dir}`
    resolves to the board package's directory, beside `${workspace}` (the
    nano-ros tree). The catalog resolves it when it reads the definition, so
    every consumer sees the resolved path. An outside board names its own
    `FREERTOS_CONFIG_DIR` and linker directory this way.
  - **nano-ros's shared files from a package's CMake module.** The module's
    own `CMAKE_CURRENT_LIST_DIR` is inside the package, so the cache variable
    `NROS_SOURCE_DIR` names the nano-ros tree for the family files it reuses.
- **How a board crate joins the cargo graph.** These rules settle the Rust
  road for a board package with a crate:
  - **`crate_path` is relative to the board definition's own directory, for
    every root, and defaults to that directory.** Today it is joined to the
    nano-ros root, with `packages/boards/<crate>` as the default. Measured
    on 2026-11-18, all 8 in-tree definitions that name a board crate sit in
    that crate's own directory, so the new default resolves every one of
    them identically and needs no migration. One rule for every root is what
    RFC-0064 D1 asks for: ours and a user's board differ only in which root
    found them. A root-dependent meaning would break that.
  - **The board crate names nano-ros crates by registry name**
    (`nros-board-freertos = "0.6"`, `nros-platform = "0.6"`). The generated
    entry's `[patch.crates-io]` rows (RFC-0098, RFC-0104) resolve them to the
    sources in use, which is the mechanism leaves already use. A path
    dependency from an outside crate into the nano-ros tree cannot be spelled
    portably. The board crate itself is the entry's path dependency.
  - **`nros::main!` learns the board type from the crate's own metadata.**
    The board crate states `[package.metadata.nros.board] type =
    "::my_board::MyBoard"`, as it can today. The CLI reads it and writes
    `NROS_BOARD_TYPE` into the generated entry's `nros-cargo.toml` `[env]`.
    That is the cargo road's carrier, and the CLI owns the file. This
    replaces the route that needed an entry `build.rs` calling
    `emit_board_framework()`, which a generated entry does not have. The
    type is stated once, by the crate, and is not repeated in the board
    definition.
  - **Acceptance includes a rebuild check:** changing the board's `type` (or
    switching board) rebuilds the entry, rather than linking a stale
    expansion.
  - **The compiled-in key tables become a fallback, then go.** The macro's
    `BOARD_PATHS` is the Rust twin of the entry pipeline's board-key table.
    While in-tree board crates do not yet state `type`, the macro keeps it
    as a fallback after `NROS_BOARD_TYPE`. Once every in-tree crate states
    its type, both tables can be retired (phase-577).
- **An outside board owns its whole RTOS configuration.** An outside FreeRTOS
  board ships its complete `FreeRTOSConfig.h`, `lwipopts.h` and `arch/cc.h`
  (RFC-0064's third level). In-tree boards include the family's shared
  configuration by a relative path, and the cargo road's C builds see only
  `FREERTOS_CONFIG_DIR`, so an outside package has no include path to reach
  the family's files. A family-configuration include seam would remove the
  copies; that is an issue homed in phase-577, not a precondition.
- **Scaffolding.** `nros board new` already scaffolds a board package. It
  emits D2's shape: `package.xml` with a version range and the runner
  `exec_depend`, a board definition with run, flash and console tables, the
  ament marker's install rule (D5) and one example (D7).
- **A new project names its board.** `nros new <name> --board <key>` writes
  the image's `board =`. The scaffold writes nothing platform-specific; the
  board definition brings that. Where today's `nros new` writes
  platform-specific files, that is a defect this RFC's phase removes, because
  otherwise "one line switches the board" holds only within one platform.
- **Switching boards stays one line.** It is the image's `board =` in
  `system.toml`, with `nros build --board <key>` as a per-invocation override.
  Everything else follows from the board definition.

### D3 — Flash, run and monitor from the board definition

The existing tables become the whole contract:

- `[board.run]`: host, or an emulator (`qemu:<machine>`, `renode:<script>`,
  `fvp:<model>`).
- `[board.flash]`: the method, plus the method's arguments. Paths in the
  arguments are relative to the board package's directory.
- `[board.console]`: how to read the image's output (serial, RTT, or
  semihosting). For serial it states the baud rate and the USB vendor and
  product ids, never a device path: a path belongs to one machine and cannot
  ship in a package. The port comes from `nros bench scan` (recorded in
  `$NROS_HOME/bench.toml`, RFC-0118 D6) or from `--port`.

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
that every board package states these tables, and D8 checks two things that
are easy to confuse:

- **the runner package** for the method is installed;
- **the host tool** it drives (`openocd`, `probe-rs`, `picotool`) is present.
  The remedy is `nros setup <key>`, when the board definition lists the tool
  under `[board.provisioning] tools`.

Two more rules:

- **Probe access on Linux.** "Permission denied" on a probe's device node is a
  named diagnostic whose remedy installs the probe's udev rules. A board
  package may ship those rules under `flash/udev/`. Installing them needs root,
  so nano-ros prints the command and never runs it.
- **The verbs are named once.** Hardware is `nros flash --monitor`: write,
  then watch the console. An emulator or the host is `nros run`, because there
  is nothing to write. The book states each in one sentence.

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
  Installing compiles nothing, because a board package's code is built per
  image, for that image's target. An optional board crate is installed as
  source. This needs one thing from the colcon side: for a package that
  announces `kind="board"`, the `nros_cargo` build type's install step copies
  the package and writes the marker, and does not build the crate. The
  phase records that as its own item.
- **What D8 of RFC-0087 keeps.** The user's own packages stay source-time, and
  nothing requires sourcing a `setup.sh`. The amendment is narrow: installed
  board packages (and, through D4, their profiles) become one more root.
- **Listing.** `nros board list` shows every key from every root, with its
  package, root, platform and support status (RFC-0133 D1, RFC-0114). Two
  marker columns flag an incompatible version range (D6) and a shadowed copy.
  Its footer says how to get more board packages (the package naming
  convention for apt and rosdep, and the book's board page), because unlike
  Arduino's Boards Manager it lists only what is already found.
- **One board's page.** `nros board show <key>` answers the reader's next
  question, "what does this one need?". It shows:
  - the package, its root, its version range and whether it is compatible;
  - the support status;
  - the provisioning packages, and whether they are installed;
  - the BSP variable, and whether it is set;
  - the flash method, with its runner and tool, and whether each is
    installed;
  - the examples.

  `nros board show` exists today for registry evidence (RFC-0114), so this
  extends it.
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
- `nros check <board-package>`, which already dispatches by package kind
  (RFC-0117 D2, RFC-0119 D10), also builds each example, and runs it where
  `[board.run]` names a host or an emulator. It stays the one spelling.
- For in-tree boards this is RFC-0115 D3's "one worked configuration per book
  board", extended to every in-tree board key. The five in-tree boards with no
  example gain one.

### D8 — The pre-build compatibility check

Before any build step, `nros build` (and `nros check --board <key>` on its
own) compares the image with the board definition and reports every mismatch
at once:

```
error[NROS-....]: image `talker` cannot build for board `acme-m7-freertos`
  (board package `acme_boards` 0.4.1, installed in /opt/ros/humble):
  - rmw `cyclonedds` needs threads; the board's capabilities say threads = false
      fix: choose rmw `zenoh` for this image, or a board with threads
  - transport `udp` is not among the board's links [serial]
      fix: set transport = "serial" in [image.talker.network]
  - the image needs ~148 KiB of RAM; the board has 84 KiB free
      fix: lower the entity counts, or choose a board with more RAM
  - the vendor SDK root ACME_SDK_ROOT is not set
      fix: export ACME_SDK_ROOT=<path to the vendor SDK>
  - toolchain `arm-gnu-toolchain` is not installed
      fix: nros setup acme-m7-freertos
  - `acme_boards` supports nano-ros >=0.7, <0.8; this is 0.9.2
      fix: install an `acme_boards` release that supports 0.9
note: a workspace copy of `acme_boards` would shadow this installed one; none is present
```

Rules for the output:

- **Every reason carries a remedy line**, not only the fault.
- **Numbers are given as the comparison the reader needs.** Memory is
  "needs N, has M free", never a subtraction left to the reader.
- **The board's origin is named**: package, version and root, with any
  shadowing. Otherwise a user who edits one copy and builds another debugs
  the wrong file.
- **A key found in no root** lists near matches and the roots searched. It is
  the most common mistake when switching boards.

It checks, from facts that already exist:

| Need | Source |
| --- | --- |
| platform, framework, entry type | board definition; `image_framework` |
| RMW and its capability needs (threads, heap, atomics) | board `[board.capabilities]`; RMW provider facts |
| transports and links | image network configuration (RFC-0109); `[board.net]` |
| memory | the sizing descriptor (RFC-0100) against a board memory budget, which the board definition gains |
| toolchain and tools present, with `nros setup <key>` as the remedy | today's preflight; `[board.provisioning] tools` |
| the BSP root the package names is set and exists | the board package's own variable |
| every library the image uses can build for this board | D9 |
| nano-ros version | D6 |
| flash method's runner package and host tool installed | D3 |

- **Each existing check stays the authority for its own fact.** D8 calls them
  and collects their verdicts; it adds no second copy of any rule.
- **Error codes.** Each reason carries a diagnostic code (RFC-0129).
- **What is new in data:**
  - a memory budget in the board definition;
  - each RMW provider's capability needs, stated as facts.

### D9 — What a library needs from a board

A library package (for example Eigen, or a vendor driver library) declares
what it needs from a board, in the same capability vocabulary that D8 uses
for an RMW: hosted C++ (a standard library with `<string>` and friends),
heap, threads, atomics, and a minimum RAM. This is Arduino's
`architectures=`, stated as needs rather than as a list of names.

- D8 checks every library the image uses against the board definition's
  `[board.capabilities]`, before any build. Today, Eigen on a board built with
  `-nostdinc++` (ThreadX RISC-V) fails mid-configure, in
  `nano_ros_hosted_cxx()`. Under D9 it is refused before the build, with the
  reason.
- Adding a library never edits a board package.
- Switching board re-checks every library the image uses.

## User flow

What a reader types, in order. The book's board pages are written from this
sequence (phase-577 W13).

1. **See what is available.** `nros board list`, then `nros board show <key>`
   for the board they want.
2. **Install the board package** if it is not listed: from apt or rosdep by
   its package name, or with `colcon build` into a prefix that the shell has
   sourced.
3. **Create a project for it.** `nros new <name> --board <key>`.
4. **Provision.** `nros setup <key>` installs the toolchain and tools the
   board definition lists. The reader sets the vendor SDK variable, if the
   board names one.
5. **Build.** `nros build`. D8 runs first, and any mismatch is listed with its
   remedy.
6. **Run it.** On hardware, `nros flash --monitor`. On an emulator or the
   host, `nros run`.
7. **Switch board.** Change the one `board =` line, or pass
   `nros build --board <other>`. D8 reports anything that no longer fits.
8. **Add a library.** Add the dependency. D9's check runs at the next build
   and again on every board switch.

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
- 2026-11-18: D2 gains the board's own CMake module, found by phase-577
  W8's owner building the first board package.
- 2026-11-18: D2 settles the Rust road for a board crate:
  - `crate_path` is relative to the definition, for every root;
  - registry names plus patch rows;
  - `NROS_BOARD_TYPE` comes from the crate's metadata, carried in the entry's
    `nros-cargo.toml` `[env]`.

  Asked by W8's owner.
- 2026-11-18: D2 gains key-to-family resolution through the catalog,
  `${board_dir}`, `NROS_SOURCE_DIR`, and the rule that an outside board owns
  its RTOS configuration, all found building W8's first C image.
- 2026-11-18: book-side review folded in. The changes:
  - `nros setup` resolves through the board catalog, using
    `[board.provisioning] tools`;
  - D1 gains its prose rules, "board key" and "platform";
  - console ports come from the bench, not the package;
  - D3 checks the host tool as well as the runner package;
  - D8 output carries a remedy per reason;
  - D9 covers library needs;
  - a User flow section is added;
  - `nros check` stays the one verb.
