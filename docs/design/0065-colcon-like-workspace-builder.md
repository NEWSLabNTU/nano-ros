---
rfc: 0065
title: "A colcon-like builder: `nros build`, and the entry stops being hand-written"
status: Draft
since: 2026-08
last-reviewed: 2026-08
implements-tracked-by: []   # phase doc follows this RFC's approval
supersedes: []
superseded-by: null
---

# RFC-0065 — A colcon-like builder: `nros build`, and the entry stops being hand-written

**Amends:** [RFC-0024](0024-multi-node-workspace-layout.md) §2.4 + §9 (the
`nros build` rejection — see *Reconciliation* below), RFC-0048 (ament/CMake
integration + presets), RFC-0026 (examples are standalone copy-out projects),
[RFC-0070](0070-build-cache-layout.md) R1 (`install/` → `dist/`).
**Relates to:** [RFC-0063](0063-system-model-is-a-build-artifact.md) — that RFC
says the resolved model belongs in `build/`; this one decides who *owns*
`build/`.

## Problem

A nano-ros workspace user maintains build-system files that carry no decision of
theirs, and then repeats a six-command ritual to use them.

**The root file.** `examples/workspaces/c/CMakeLists.txt` is ~70 lines doing four
jobs — mapping board to toolchain before `project()`, listing packages by hand,
filtering entries by platform by hand, and promoting `NUTTX_DIR` out of a cmake
scope. **Every one of those four is derivable, and none is user intent.** Nine
workspace roots and eight root `Cargo.toml` files carry variations of it. A
workspace that gains a package but forgets the `SUBDIRS` line simply does not
build it — silently, because an absent subdir is not an error.

**The ritual.** `examples/workspaces/rust/README.md` tells a user to type:

```bash
nros setup native
nros sync
nros codegen-system --bringup demo_bringup
nros check --bringup src/demo_bringup
nros check --workspace .
cargo build -p native_entry
```

`colcon build` is one command. The root file is what a *maintainer* feels; the
ritual is what a *user* feels. Both are this RFC's problem.

**The entry packages.** A workspace carries one entry package per (system,
board) pairing — `examples/workspaces/rust` has fifteen. Measured, they contain
almost nothing:

| what | measurement |
| --- | --- |
| every Rust entry source | ≤ 6 non-comment lines |
| every **embedded** C/C++ entry | **zero source files** — `CMakeLists.txt` + `package.xml` only |
| native C/C++ entry source | **2 lines** |

```c
/* examples/workspaces/c/src/native_entry/src/main.c — the whole file */
#include <nros/main.h>
NROS_MAIN_C(nros_board_native, "demo_bringup:system.launch.xml");
```

That is `(board, launch)`. The C path has already reduced the entry to a
declaration; the Rust path has not.

## What we are NOT

Worth stating plainly, because the analogy misleads if taken too far. `colcon
build` walks a package tree and builds each package into its own artifact,
producing `build/` + `install/` with per-package shared libraries composed at
runtime.

nano-ros reads the **launch tree** and performs **one whole-system bake per
(bringup, board)**. There is no per-package shared library, no `install/` to
source, no runtime composition. Packages are inputs to that bake, not
independently deployable units.

The bake's product is a **set of artifacts, usually of size one** — see D8. A
bare-metal or host target yields a single executable; a production Zephyr target
yields the application plus its bootloader, sharing a signing key and partition
table. Treating the singleton as the general case is what would need undoing
later, so it is not assumed here.

So the borrowing is the FRONT of colcon (discover packages by walking the tree;
require no hand-maintained root manifest; put output under `build/`), not the
back.

### Why not reuse colcon itself

colcon is a real candidate — it is pip-installable without ROS, and
`packages/cli/colcon-cargo-ros2/` already registers fourteen
`ros.nros.<lang>.<platform>` build tasks. It is rejected for four reasons, the
first decisive:

1. **Per-package isolation fragments the cargo tree.** colcon gives each package
   its own cmake configure. RFC-0070 measured the consequence: of 182.3 GiB in
   cmake binary dirs, **151.7 GiB (83.2 %) was corrosion's own cargo tree**, and
   the 80.6 GB that phase-340 actually saved came from **one invocation over
   many packages**. colcon's model is the shape that measurement rejects.
   Avoiding it under colcon means making a single package build everything — at
   which point colcon contributes only a tree-walk we already own.
2. **We already have the parts it would give us.** Discovery is
   `nros-pkg-index`'s `package.xml` walk; ordering is `nros ws order` (phase-348
   W4), already called by `nano_ros_workspace(ORDER_FROM_DEPENDS)`.
3. **No target concept.** The existing `task/nros/build.py` shows the strain in
   its own comment: it reads `NANO_ROS_RMW` from an env var because *"the
   standalone per-package colcon task has no system config in scope"*.
4. **A second implementation to keep in sync.** Discovery and ordering in Python
   beside the Rust ones is the "second spelling instead of a shared helper"
   class this repository names as its recurring bug source.

colcon keeps the role RFC-0024 §2.2 already gave it: an **outer** seam for
Autoware-style integration, never the inner loop. A nano-ros workspace and a
ROS 2 workspace are separate trees on different OS and ROS layers; they are not
built together.

## Reconciliation with RFC-0024 §2.4

RFC-0024 constraint 4 reads *"nros never a build verb. No `nros build` / `nros
test` / `nros flash`"*, and §9 rejects the verb as *"re-creates colcon's
wrapping anti-pattern; hides cargo/cmake diagnostics."* Phase-222 deleted the
verb; `nros doctor` still warns on it.

**That objection is to wrapping the compiler, not to deriving the root.** This
RFC's builder does not wrap:

* **Stage 5 is `exec`, not a pipe.** The native tool replaces the process. A
  rustc error is byte-identical to `cargo build`'s, because nothing is capturing
  it. This is also precisely what colcon cannot do — its per-package task model
  must capture output in order to attribute it.
* **The generated root is a real build tree.** `build/<coord>/CMakeLists.txt` is
  an ordinary cmake project; `cmake --build build/<coord>` works by hand
  forever after. RFC-0024 §2.3 ("cargo / cmake stay user-facing") holds
  literally.

So §2.4 is amended in scope, not overturned in principle: **`nros build` writes
the root and hands off; it does not own the build.** `nros test` and `nros
flash` remain rejected — they have no derivation to perform, so they would be
pure wrapping.

## Decision

### D1 — `nros build` is a five-stage pipeline that ends in `exec`

```
nros build [<image>] [--board <board>] [-- <native build args>]

  1. DISCOVER   walk package.xml → pkg index → topological order
  2. RESOLVE    the image: argument > sole declaration > list and fail
  3. PREFLIGHT  toolchains / SDKs / sources present?
  4. GENERATE   msg bindings + system model + the ROOT BUILD FILE → build/<coord>/
  5. EXEC       cmake --build / cargo build / west build — stderr untouched
```

Stages 1–3 exist today (`nros-pkg-index`, `nros ws order`, `nros setup`, `nros
doctor`, `nros sync`). Stage 4's root emitter and the driver that sequences them
are the new code.

### D2 — Provisioning asks once, and never in CI

Stage 3 auto-fetches what the index can fetch, **after prompting with the
download size**. `--yes` skips the prompt. **A non-TTY behaves as verify-only**:
it never fetches and never blocks, failing instead with the exact `nros setup`
line. License-gated packages are never auto-fetched in any mode — they fail
naming the package and the manual step.

### D3 — The driver is chosen by the board, not by the language mix

| board | driver | stage 4 emits |
| --- | --- | --- |
| pure-Rust package set | **cargo** | a synthesized `[workspace] members` root |
| any C/C++ package in the set | **cmake** | `CMakeLists.txt` calling `nano_ros_workspace(…)` |
| zephyr | **west** | nothing — sets env, `exec west build -b <board>` |
| esp32 | **idf.py** | nothing — same shape |

Mixed is not a fourth case. RFC-0024 §6.3 already settled it: *"cargo can be
consumed as a cmake target (via Corrosion); cmake cannot be consumed as a cargo
target. So when the graph crosses languages, cmake wins."* One cmake configure
per workspace is what preserves the single corrosion cargo tree.

**The rule that covers every exception:** *stage 4 emits a root only where a root
would otherwise be hand-written.* west and ESP-IDF apps keep their own files
because those are Kconfig overlays — user intent, not derivable. A copy-out
example (RFC-0026) keeps its root because it must build with plain cargo/cmake
and no `nros` at all, and its output stays in the native `target/` / `build/`
beside the source per RFC-0070 R1's amendment. Neither needs a special case.

### D4 — Entry × board → image. The entry is generated.

An entry package does not *have* a `main`; it *gives* one to a board.

| board | who owns `main` | what the entry supplies |
| --- | --- | --- |
| native / Linux | the entry — real `fn main` | `<LinuxBoard as BoardEntry>::run(…)` |
| Zephyr | Zephyr's C `main` | a **staticlib** exporting `rust_main` |
| FreeRTOS (C) | board startup | generated TU carrying `nros_app_main` |
| ESP-IDF | idf's `app_main` | the component registration |

So entry and board are **independent axes that get paired**, not one implying
the other. The C/C++ side already encodes this: `nano_ros_entry` takes `DEPLOY
<board1> [<board2> …]`, a **list** of supported boards, gated by
`if("${NANO_ROS_BOARD}" IN_LIST _NRA_DEPLOY)`. phase-372 W2 made one C++ leaf
serve two boards with `BOARD ${NANO_ROS_BOARD}`.

**Therefore the entry is derivable from `(launch, args, board)` and is
generated into `build/<coord>/`.** A workspace contains no entry packages.

What is *not* derivable, and where each thing goes:

| not derivable | home |
| --- | --- |
| RTOS config overlays — `prj.conf`, `prj-<rmw>.conf`, `boards/*.overlay`, `sdkconfig.defaults` | the **bringup package**, under `boards/<board>/` |
| panic handler, `[profile.release]`, a custom spin loop (RFC-0024 §11.8) | `nros eject`, below |

Overlays are indexed by board × RMW (`prj-zenoh.conf`, `prj-cyclonedds.conf`),
never by entry — evidence they were never entry property. They move to the
bringup package, which already owns `system.toml` and `launch/`, matching the
nav2/Autoware convention of a bringup package carrying `config/`.

### D5 — `nros eject` is the escape hatch, and it is load-bearing

`nros eject <image>` writes the generated entry into `src/<name>_entry/` as a
real, owned package; the builder afterwards treats it as an ordinary input. This
is the only home for a hand-written panic handler, profile override, or custom
spin loop, so **it requires a test proving an ejected entry still builds** — a
decorative eject would silently delete capability.

### D6 — `[deploy.<id>]` becomes `[image.<id>]`

The table already holds the triple the builder needs:

```toml
[image.robot1]                            [image.freertos]
kind   = "self"                           kind  = "embedded"
target = "x86_64-unknown-linux-gnu"       board = "mps2-an385-freertos"
launch = "multihost.launch.xml"
nodes  = ["/talker"]
```

`kind` stops being two species under one name and becomes one axis: whether the
image carries a **partition** of the nodes (`self`) or **all** of them
(`embedded`).

This also retires an orphan class. Today `[deploy.robot1]` and `[deploy.robot2]`
are declared and **no entry points at them** — `native_entry_robot1` declares
`deploy = "native"` and selects its machine through a *second* mechanism, a
launch argument (`args = [("host", "robot1")]`). Once the entry is synthesized
*from* the image declaration, the second mechanism disappears.

Migration carries a deprecation window in which both `[deploy.*]` and
`[image.*]` parse, with `[deploy.*]` warning.

### D7 — Vocabulary: there is no `--target`

"target" already means four things here — the Rust triple
(`DeployTargetMetadata.target`), cargo's `--target`, a cmake build target, and
the old `[deploy.<id>]` id. A fifth meaning on our own flag would collide with
the `--target` users pass through to cargo. **Board** and **image** are the two
nouns; `--target` is not a flag `nros build` defines.

```
nros build                    # sole image, or list and fail
nros build robot1             # one declared image
nros build --board <b>        # every image on that board
nros eject robot1             # materialize the entry package
```

### D8 — Output layout

Build trees live at `build/<coordinate>/`, coordinate in RFC-0070 R2's existing
vocabulary (platform, rmw, feature-sig) — **never a new ad-hoc suffix**. One
configure per coordinate, so switching board or RMW selects a different tree
rather than thrashing one.

Finished artifacts land in **`dist/<image>/`**, and that directory holds a
**set** plus a manifest naming which member is flashable:

```
dist/zephyr-nrf52840/          dist/native/
  manifest.toml                  manifest.toml
  app.signed.hex                 demo          # a set of one — same shape
  mcuboot.hex
  merged.hex
```

The singleton is not a special case; it is a set of one. This falls out of
Zephyr **sysbuild**, whose most common configuration is "build my app and an
MCUboot bootloader, same key, same partition table" — so on any board with a
bootloader the product is ≥2 artifacts with config that must stay in sync. A
`dist/` layout assuming one file would have to be re-cut the first time anyone
ships a signed image.

`dist/` is deliberately not `install/`: that name promises an environment to
source, which nano-ros will never have, and a ROS user's first move would be a
`source dist/setup.bash` that cannot exist. This amends RFC-0070 R1, which
names `install/` for workspace scope.

### D9 — A board is a (machine, platform) pair, and the registry resolves it

Two platforms on one SoC are **two boards**. The tree already encodes this and
already paid for it: `PlatformId` in `matrix.rs` enumerates `Linux`,
`ZephyrNativeSim`, `ZephyrQemuCortexM`, `FreertosMps2`, `FreertosPosix`,
`QemuBaremetal`, `NuttxArm`, `NuttxRiscv`, `ThreadxLinux`, `ThreadxRiscv64`,
`Esp32Qemu`, `Fvp` — (machine, platform) pairs under a name that says
"platform". mps2-an385 alone appears three times, as `QemuBaremetal`,
`FreertosMps2` and `ZephyrQemuCortexM`. `matrix.rs` records the cost of learning
this:

> phase-337 W2 added this because "Zephyr" previously meant exactly one config:
> `native_sim/native/64` … **That is a board, not a platform** — and the
> difference is not academic.

Bringing that one witness up surfaced **five real defects**, every one invisible
to native_sim. micro-ROS reached the same conclusion independently: its entry
point is `create_firmware_ws.sh [RTOS] [Platform]`, with per-pair config
folders (`config/freertos/crazyflie21`).

Board **identity** therefore splits per platform always. Board **crate** count
does not have to follow: `nros-board-zephyr` serves three boards and
`nros-board-nuttx-qemu` two, because Zephyr and NuttX own their own board
abstractions and our crate is a shim over theirs, while baremetal and FreeRTOS
have no such tree so we carry it. Identity is user-facing; crate count is
implementation.

**One vocabulary at the surface.** `[image.*].board` today mixes seven nano-ros
keys with one raw Zephyr string (`native_sim/native/64`) — the same conflation
phase-337 W2 removed from `PlatformId`, still live one layer up. The user always
writes a nano-ros board id; `board-support.toml` carries the framework's own
board string for platforms that have one, and the builder resolves it. Nothing
authored in a bringup is a framework's private vocabulary.

### D10 — Overlays reach the framework through its own external-config knob

nano-ros knows **no overlay filenames**. Per platform it knows one thing: which
knob receives an external config directory.

| framework | knob |
| --- | --- |
| Zephyr / west | `APPLICATION_CONFIG_DIR`, plus `EXTRA_CONF_FILE` / `EXTRA_DTC_OVERLAY_FILE` for absolute paths |
| ESP-IDF | `SDKCONFIG_DEFAULTS` (a list; env var or CMake var) |
| NuttX | `CONFIG_APPS_DIR` |
| FreeRTOS / ThreadX / baremetal | none — the board crate owns the config header |

Zephyr's `APPLICATION_CONFIG_DIR` is documented as taking *all* configuration
files from the named directory, so pointing it at
`src/<bringup>/boards/<board>/` preserves Zephyr's own `boards/<board>.conf`
auto-discovery with no copying.

**A path whitelist is explicitly rejected.** Zephyr's overlay surface is open
and grows with releases — `.conf`, `.overlay`, `dts/bindings/`, `snippets/`,
`Kconfig` — so a whitelist would silently drop a file the user added. That is
the silent-drop class this repository keeps paying for; the framework, not
nano-ros, decides what a file in its config directory means.

Note the split by **author**, which is what makes this small: the board's
*baseline* config is nano-ros's and already ships in the board crate
(`nros-board-freertos-posix/config/FreeRTOSConfig.h`, the NuttX `defconfig`,
`cmake/zephyr/<board>.conf`). The overlay is a user **delta** on top, and today
only Zephyr has a non-empty one. The other platforms' knobs stay unused until
something needs them.

### D11 — A custom board is a board crate, not an overlay

Vendor SDKs generate per-board **source**, not just config: MCUXpresso Config
Tools emit `pin_mux.c/h` and `clock_config.c/h` into a project `board/` folder
("if you don't structure this correctly, your project won't work correctly with
the Config tools"), and note that pin-mux files are *"always customized for each
application"*. STM32CubeMX has the same shape via `.ioc`. An out-of-tree NuttX
board needs `Kconfig` + `defconfig` + `Make.defs` + `Makefile`.

None of that is configuration, so none of it belongs in
`src/<bringup>/boards/<board>/`. **A custom board is authored as a board crate**,
exactly like the in-tree ones, and the authoring story belongs to
[RFC-0012](0012-board-bsp-integration-architecture.md), not here. This RFC states the
boundary explicitly because an overlay directory otherwise reads as covering it,
and someone will try.

## What the user authors

```
src/
  talker_pkg/            # Node pkg — code
  listener_pkg/          # Node pkg — code
  demo_bringup/          # Bringup pkg — the system declaration
    package.xml
    system.toml          #   components + [image.*]
    launch/*.launch.xml  #   topology
    boards/<board>/      #   prj.conf, prj-<rmw>.conf, *.overlay, sdkconfig.defaults
```

Three authored things: **node code, the launch tree, the per-board overlay.** No
entry packages, no root `CMakeLists.txt`, no root `[workspace] members`.

For `examples/workspaces/rust` that is 15 entry packages plus one 19-member root
manifest — 16 files — becoming derived.

## Error handling

Three classes, three behaviours:

* **Missing prerequisite** — fails in stage 3 with the exact `nros setup` line,
  never mid-compile.
* **Declaration error** — a dependency cycle, an unknown image, an image naming
  a board with no overlay — fails in stages 1–2 naming the file and package,
  before anything is generated.
* **Compile error** — the native tool's, reaching the terminal unmodified,
  because stage 5 is an `exec`.

## Consequences

**The builder owns `build/`.** RFC-0063 moves the resolved SystemModel there and
leaves the layout open; this RFC answers it — `build/` is a builder output tree,
so the model lands in it for the same reason object files do.

**`generated/` and `metadata/` follow.** Already gitignored-in-source
(phase-330 W3.a); once a builder owns `build/`, leaving derived msg crates under
`src/` has no defence. Sequencing matters: the msg-crate redirects in each leaf's
`.cargo/config.toml` are RELATIVE paths, and issue 0378 is the live reminder
that a wrong redirect resolves to a stranger's crate on crates.io.

**Fixture rows become invocations.** `examples/fixtures.toml` declares itself the
SSoT for per-fixture build options; a builder taking `(image, board)` is exactly
what a row already describes, so the row becomes an invocation rather than a
description of one.

**Two duplicate-declaration hazards must be resolved, not inherited.**
`freertos_entry/Cargo.toml` carries `[package.metadata.nros.deploy.freertos]`
with the same `rmw`/`domain_id`/`locator` facts as the bringup's block, with no
stated precedence; and the machine axis is spelled both as `nodes = [...]` and
as a launch argument. The builder must resolve each in one documented order.

## Validation cases

**Issue 0798** — `examples/workspaces/c`'s root routes `s32z270-freertos` to an
entry hardcoding `mps2-an385-freertos`, so all three arms of `_nra_board_active`
are false and the image links without its platform glue or locator. It is
latent only because the sole s32z270 fixture row targets the C++ workspace. A
generated entry cannot disagree with the board it was generated for, so this
class ceases to exist — making 0798 the sharpest acceptance test for D4.

**Phase-331** folded 18 themed workspaces into four, moving node packages
between them. Every move edited a hand-maintained `SUBDIRS` list or `[workspace]
members`. If a tree-walking builder would have made that fold `git mv` plus a
bringup edit, the design holds.

## Fit against real frameworks

Checked 2026-08-25 against upstream documentation rather than against our own
ports, because our ports are the thing under test.

**Fits.** Every framework we target already has a first-class knob for "config
lives outside the source tree" — Zephyr's `APPLICATION_CONFIG_DIR`, ESP-IDF's
`SDKCONFIG_DEFAULTS` list, NuttX's `CONFIG_APPS_DIR`. So D10 uses upstream
features, not workarounds, and the single-bringup design does not fight the
per-framework principle.

**Precedent.** micro-ROS keys on `[RTOS] [Platform]` and configures firmware in
a pre-build step before handing off — the same pairing and the same
generate-then-handoff shape. It differs in creating **one firmware workspace per
target**, where this RFC keeps one workspace spanning targets. That is the
harder road, taken deliberately: a nano-ros system spans boards (the multi-host
case), and per-target workspaces cannot express it.

**Does not fit, and is answered above.** Zephyr sysbuild makes the product a set
(D8). Vendor-generated board init is per-(board, application) source, not config
(D11).

**Unresolved.** Nothing found yet contradicts D1–D11, but only three frameworks
were checked in depth. ThreadX and the FreeRTOS vendor distributions
(MCUXpresso, STM32Cube) were checked only for their board-init shape.

## Open questions

- **Deprecation window length for `[deploy.*]` → `[image.*]`**, and whether
  `nros doctor` warns or `nros build` does.
- **Whether the repo's own nine workspace roots migrate** as part of this work,
  or only new user workspaces get the builder. Migrating them is the real proof
  it works; not migrating them leaves two shapes in one tree.
- **How `nros eject` names what it writes** when several images share a launch
  file but differ in args.
- **Whether sysbuild config is authored or derived.** D8 accepts a set of
  artifacts, but says nothing about who declares that a board wants MCUboot.
  Candidates: a key in `[image.<id>]`, or the presence of sysbuild files in the
  board's overlay dir. The second is more framework-respecting and less
  explicit.
- **How `dist/manifest.toml` is consumed.** Flash and run paths must read it
  rather than glob, or the set degenerates back into a convention.

## Non-goals

Per-package shared libraries, an `install/` tree to source, or runtime
composition of independently built packages — colcon's model, explicitly not
nano-ros's. `nros test` and `nros flash`: rejected by RFC-0024 §9 and still
rejected here, because neither has a derivation to perform.

## Changelog

- **2026-08-02** — created as Draft; problem statement + the "front of colcon,
  not the back" framing; four open questions.
- **2026-08-25 (b)** — framework-fit research pass. `dist/` becomes a SET plus a
  manifest (D8) because Zephyr sysbuild's standard case is app + MCUboot with
  shared key and partition table; "one unified image" retired from the framing.
  Adds D9 (board = (machine, platform), registry-resolved, one surface
  vocabulary), D10 (overlays reach a framework through its own
  external-config knob; a path whitelist is rejected), D11 (a custom board is a
  board crate — vendor tools generate board SOURCE, which is RFC-0012's
  territory). New *Fit against real frameworks* section.
- **2026-08-25** — rewritten with the decisions D1–D8. Adds the RFC-0024 §2.4
  reconciliation (exec, not wrap), the colcon-reuse rejection with the RFC-0070
  measurement, the entry×board pairing and entry synthesis (D4/D5), the
  `[deploy.*]` → `[image.*]` rename (D6), `dist/` over `install/` (D8). Closes
  the original open questions on driver, `install/`, incremental layout and
  target declaration. Files issue 0798 as the validation case.
