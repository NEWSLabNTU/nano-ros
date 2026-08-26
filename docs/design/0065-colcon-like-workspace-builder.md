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

**Amends:** [RFC-0026](0026-example-directory-layout.md) (a copied-out
*workspace* now requires `nros build`; single-package leaves are unchanged),
[RFC-0024](0024-multi-node-workspace-layout.md) §2.4 + §9 (the
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
  2. RESOLVE    the image: argument > [system] default_images > list and fail
  3. PREFLIGHT  toolchains / SDKs / sources present?
  4. GENERATE   msg bindings + system model + the ROOT BUILD FILE → build/<coord>/
  5. EXEC       cmake --build / cargo build / west build — stderr untouched
```

Stages 1–3 exist today (`nros-pkg-index`, `nros ws order`, `nros setup`, `nros
doctor`, `nros sync`). Stage 4's root emitter and the driver that sequences them
are the new code.

**Bare `nros build` never guesses and never surprises.** `[system]
default_images` names the default set; absent it, a workspace with several
images lists them and fails. `--all` builds every image.

```
$ nros build
error: this workspace declares 8 images and no default.

  native   zephyr    freertos    nuttx
  robot1   robot2    esp32-qemu  threadx-linux

  build one:   nros build native
  build all:   nros build --all
  or declare:  [system] default_images = ["native"]
```

PlatformIO's `default_envs` is the model, with one deliberate divergence: `pio
run` builds **every** environment when none is named, and that is wrong here.
`examples/workspaces/rust` declares eight images across three cross toolchains,
so an accidental bare invocation would be a very expensive way to learn the
default. One declaration per multi-image workspace is the cheaper trade.

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
| panic handler, `[profile.release]` | **declarations on the image** — D5 |
| a genuinely unforeseen hand-written entry | `nros materialize` — D5, shape-stamped |
| third-party code that must be linked | a **support package** — D12 |

Overlays are indexed by board × RMW (`prj-zenoh.conf`, `prj-cyclonedds.conf`),
never by entry — evidence they were never entry property. They move to the
bringup package, which already owns `system.toml` and `launch/`, matching the
nav2/Autoware convention of a bringup package carrying `config/`.

### D5 — Declarative escapes first; materialising is the last resort

**An earlier draft of this RFC made `nros eject` the primary escape hatch. The
closest prior art ran that experiment and abandoned it.** Expo shipped `eject`,
found it was *"a one-way door for most projects"*, and replaced it with
Continuous Native Generation — where `eject` is now *"historical vocabulary"*.
Their stated reason is the one that applies to us directly: *"If you modify the
generated directories manually then you risk losing your changes the next time
you run `prebuild --clean`. **Instead, use config plugins.**"*

So the escapes that are **known** become declarations on the image, and never
leave generation:

```toml
[image.freertos]
kind  = "embedded"
board = "mps2-an385-freertos"
panic = "halt"                 # RFC-0077 policy, forwarded to nros::main!

[image.esp32-qemu]
kind    = "embedded"
board   = "esp32-qemu"
panic   = "own"                # a support package carries the handler — D12
profile = { opt-level = "z", lto = true }
```

**`panic` is not a new vocabulary — it is the existing one, forwarded.**
`nros::main!` already takes `panic = "platform" | "halt" | "own"`
([RFC-0077](0077-image-runtime-is-the-images-choice.md), phase-366 W7.a): a
**policy**, not a crate name. Routing to `nros_platform_panic`, parking the
core, or declaring that this image supplies its own `#[panic_handler]`. An
earlier draft of this RFC wrote `panic = "semihosting"` / `"esp-backtrace"`,
which is a category error — those are the crates an entry pulls in *under* the
`own` policy, and inventing a second spelling for a shipped key is precisely the
"second spelling instead of a shared helper" defect this repository keeps
paying for.

The composition matters: under D4 the entry is generated, so `panic = "own"`
means *something else in the image* must carry the handler — and D12's support
package is exactly that slot. The two escapes meet rather than duplicating.

RFC-0077's own title, "The image runtime is the image's choice", is independent
support for D6: the runtime policy was already understood as a property of the
**image**, before this RFC named the table.

That covers every escape the survey found (D4): the panic policy and the
per-board build profile. The custom spin loop already has its own supported
seam, RFC-0024 §11.8.

`nros materialize <image>` remains for the genuinely unforeseen. It writes the
generated entry into `src/<name>_entry/` as a real, owned package, and the
builder afterwards uses that package rather than regenerating it.

**What materialising does and does not freeze — this is narrower than it
looks.** A materialised entry is a one-line `nros::main!(launch = "…")`, and
that macro reads the launch XML **at expansion time** (its tracked inputs are
"launch.xml, every `package.xml` the pkg-index walked"). The RMW and capability
selection likewise flows through the `*_nros_selection` facade that `nros sync`
regenerates. So adding a node to a launch file reaches a materialised entry on
the next compile: **the derivation stays live.**

What freezes is the *shell* — `#![no_std]` / `#![no_main]`, the panic handler,
board boilerplate like `esp_app_desc!()`, `[profile.release]`, and
`[[bin]]`-vs-`[lib]`. If nano-ros later changes what an entry for a given board
must *look like*, a materialised one silently keeps the old shape. That is issue
0798's class one layer up — a hardcoded entry while the system around it moved.

So materialising needs a **shape stamp, not a second mode**: the materialised
package records the generator version and the board shape it was cut for, and
`nros build` warns when that shape has since moved. It also **requires a test
proving a materialised entry still builds** — a decorative escape silently
deletes capability.

> **Considered and rejected: a third "vendored" mode** (committed generated
> output with a freshness gate, the `nros-rmw-cffi/src/generated.rs` +
> `check-abi-bindings` pattern). It was proposed for the safety-critical case,
> on the reasoning that ISO 26262 / IEC 61508 treat code generators like
> compilers ("the procedure applies to compilers and code generators", "each new
> version of an off-line support tool shall be qualified") and therefore want the
> emitted code under configuration control. But materialising **already** places
> the entry package under version control, and — per the paragraph above — the
> part a safety argument actually cares about is not frozen by it. A third mode
> would add a gate, a command and a doc section to cover a gap the second mode
> does not leave.

### D5.1 — `[image]` is a base section

Following PlatformIO's `[env]` / `[env:NAME]` split, a bare `[image]` table
carries keys shared by every image, and `[image.<id>]` overrides them. Without
it, an eight-image workspace repeats its RMW, edition and profile eight times —
and eight copies of one fact is how they start disagreeing.

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
nros materialize robot1       # own the generated entry (one-way)
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

**Who declares that a board wants a bootloader: Zephyr already decided.** An
application asks for one with a `sysbuild.conf` carrying
`SB_CONFIG_BOOTLOADER_MCUBOOT=y`, optionally with a `sysbuild/mcuboot.conf`
merge fragment. So the presence of that file in the board's config directory
**is** the declaration, and `nros build` passes `--sysbuild` when it is there.
We invent no key of our own.

**Verified in Zephyr's source, because its documentation does not say.** The
docs describe `APPLICATION_CONFIG_DIR` as covering Kconfig fragments and
devicetree overlays and are silent on sysbuild, so this was read from
`share/sysbuild/cmake/modules/sysbuild_kconfig.cmake` (Zephyr v3.7.0,
`36940db938a`):

```cmake
zephyr_get(APPLICATION_CONFIG_DIR)        # sysbuild honours it, and caches it
                                          # "in order for the setting to
                                          #  propagate to images"
zephyr_get(SB_APPLICATION_CONFIG_DIR)     # sysbuild-specific override
if(DEFINED SB_APPLICATION_CONFIG_DIR)
  set(APPLICATION_CONFIG_DIR ${SB_APPLICATION_CONFIG_DIR})
endif()
set_ifndef(APPLICATION_CONFIG_DIR ${APP_DIR})
...
# "sysbuild.conf is an optional file, because sysbuild is an opt-in feature."
zephyr_file(CONF_FILES ${APPLICATION_CONFIG_DIR} KCONF SB_CONF_FILE
            NAMES "sysbuild.conf" SUFFIX ${FILE_SUFFIX})
```

Three facts follow, all favourable:

* `sysbuild.conf` **is** looked up in `APPLICATION_CONFIG_DIR`, so D10's
  external config directory carries the sysbuild declaration with no extra
  machinery;
* one `APPLICATION_CONFIG_DIR` reaches **both** sysbuild and the images beneath
  it, because sysbuild forces it into the cache;
* presence-based detection is Zephyr's own model, not our inference — the file
  is optional precisely because sysbuild is opt-in.

`SB_APPLICATION_CONFIG_DIR` and `SB_CONF_FILE` remain as escape hatches if
sysbuild config ever needs to live apart from the app's.

**`manifest.toml` must be consumed, never globbed, and must be complete.**
ESP-IDF's `flasher_args.json` is the precedent — "project flash information in
JSON format, used by `idf.py` and other tools" — alongside `flash_project_args`,
consumable directly as `esptool @build/flash_project_args`. It also supplies the
cautionary tale: a filed bug reads *"flasher_args.json is missing entry for
`bootloader` when built with secure boot v2"*. The manifest silently fell behind
the artifacts when a feature was added. So the invariant is a **gate**: every
file in `dist/<image>/` is named by that image's manifest, and an unnamed
artifact fails the build rather than being flashed by a glob that guessed.

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

### D12 — Third-party code enters through a package, and force-linking is a keyword

Three shapes, and they do not want the same answer:

| what | where | works today |
| --- | --- | --- |
| an app-level library a node uses (sensor lib, CAN stack) | a **node package** dependency — flows transitively into the image | yes, unchanged |
| an SoC/board-level vendor layer (NXP RTD/MCAL) | a **board crate** (D11) | yes |
| code with **no referencing symbol** — vectors, driver init tables, a `.a` needing whole-archive | a **support package**, below | new |

The third row is the one the design must answer, because dead-code elimination
drops it and — under D4 — there is no entry `CMakeLists.txt` for a user to add a
link line to.

**The code lives in an ordinary package; force-linking is a declared keyword the
builder implements.**

```cmake
# src/rtd_mcal_pkg/CMakeLists.txt
nano_ros_support_library(rtd_mcal
    SRCS     generated/*.c
    INCLUDES include
    WHOLE_ARCHIVE)
```

**Why a keyword and not a flag the user writes.** We whole-archive today through
a raw `-Wl,--whole-archive,$<TARGET_FILE:…>` (`NanoRosLink.cmake`), and issue
0475 records what that costs: CMake cannot see a file inside a flag string, so
the construct carries **no rebuild edge** — a backend edit relinked the old
object, and the only cure was `rm -rf` on the build dir. Asking users to
hand-write it is asking them to reproduce a defect we have already paid for.
Behind the keyword the builder emits the flag **and** the `LINK_DEPENDS` that
gives it an edge.

Owning the spelling also buys a migration for free. Our CMake floor is **3.22**,
so the portable `$<LINK_LIBRARY:WHOLE_ARCHIVE,…>` generator expression (3.24+,
which CMake's own docs say projects should prefer "instead of manual
implementations") is out of reach today. When the floor rises, the keyword's
implementation changes and **no user file does**.

ESP-IDF reached the same conclusion independently:
`idf_component_register(SRCS … WHOLE_ARCHIVE)` is a declared keyword on the
component, not a flag its author writes. Zephyr, which has no equivalent, has an
open issue asking for a way into its whole-archive group and a bug report titled
*"Linking static library with use of `zephyr_link_libraries` may end with
undefined symbol error"*.

**A vendor tool's output is committed, never invoked by us.** NXP RTD is
configured in EB tresos Studio / S32CT, which generates MCAL C from `.xdm`
config *before* the build, across GCC/IAR/DIAB/GHS. `nros build` does not run
it. The `.xdm` is user intent and the generated C is their tool's artifact;
both live in the support package and both are committed. This is the same
per-(board, application) generated-source shape D11 found in MCUXpresso Config
Tools, so the two decisions share one answer: **a package, authored by the user,
holding what a vendor tool emitted.**

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

## Users this must serve

Three personas, walked end to end. Each found something the decisions above did
not cover.

### Newcomer — clones, builds, runs, reads

The six-command ritual becomes one, and there is no root manifest to understand
before anything compiles. That is the whole win and it is real.

**The loss is pedagogical, not technical.** Today a newcomer opens
`src/native_entry/src/main.rs`, sees one line — `nros::main!(launch =
"demo_bringup:system.launch.xml")` — and has the model. Under D4 that file is in
`build/<coord>/` instead. Mitigation is documentation, not design: the generated
entry is a readable file and the book should point at it by path.

**One tested contract changes meaning and must say so.** RFC-0026 makes each
example a standalone copy-out template with "its own `Cargo.toml` +
`.cargo/config.toml` + `CMakeLists.txt`, no workspace walk-up", and the contract
is CI-checked (`just zephyr check-copy-out`). Under D4 a copied-out **workspace**
has neither a root manifest nor entry packages, so `cargo build -p native_entry`
stops resolving and `nros build` becomes required. **Single-package leaves are
unaffected** — they keep their own root per D3, and still build with plain
cargo/cmake. RFC-0065 amends RFC-0026 to state that split explicitly, because a
CI-checked contract must not quietly change what it promises.

### Production — reproducible, auditable, offline

**Generated output must be deterministic, and nothing above said so.** A
reproducible build needs bit-identical output across machines, and generated
files depending on host state is the classic breaker — timestamps, absolute
paths, unstable ordering. The generated root and entry must carry none of them.
There is in-tree precedent for the discipline: issue 0320 made model paths
content-addressed and added `check-no-absolute-model-paths`, and the same gate
shape applies here.

**`dist/manifest.toml` is the natural SBOM anchor.** Current guidance is that
the authoritative SBOM should be immutable and linked to the exact artifact
shipped, and generated from the final artifact rather than from a manifest
alone. Ours should therefore carry the generator version and input hashes
alongside the artifact list D8 already requires.

**Offline is not yet stated as a guarantee.** D2 gives verify-only behaviour on
a non-TTY, which covers CI *by inference from how the build was invoked*. A
production build wants a property it can state — see Open questions.

### Safety-critical — everything in the safety argument under configuration control

ISO 26262 and IEC 61508 treat a code generator the way they treat a compiler:
tool qualification "applies to compilers and code generators as much as to
modelling, test and analysis tools", and "each new version of an off-line
support tool shall be qualified".

The conclusion this persona forced was **not** a new mode — see the rejected
"vendored" note under D5. `nros materialize` already places the entry package
under version control, and the derivation it appears to freeze is in fact still
live. What the persona did buy is the **shape stamp** in D5: without it, a
materialised entry drifts from the generator silently, which is exactly the
thing a safety argument cannot tolerate.

Note also that this repository's `examples/workspaces/safety/` is about **E2E
message integrity** (a CRC on `/chatter`), not certification. The
safety-critical persona here is the downstream consumer — the autoware safety
island work — not that workspace.

## Related work

Surveyed 2026-08-25. Each row changed or confirmed a decision; none is cited
decoratively.

| system | what it does | effect here |
| --- | --- | --- |
| **PlatformIO** | `[env]` base + `[env:NAME]` sections + `default_envs`; `pio run` builds all when none named | shape of D6/D5.1/D1. Adopted the base section and `default_images`; **rejected** build-all-by-default (eight images over three cross toolchains is too expensive an accident) |
| **Expo** (prebuild / CNG) | shipped `eject`, found it a one-way door, replaced it with always-generate + config plugins; `eject` is now "historical vocabulary" | **reversed D5.** Declarative escapes first; materialising is last resort and marked one-way |
| **micro-ROS** | `create_firmware_ws.sh [RTOS] [Platform]`, per-pair config dirs, `configure_firmware.sh` pre-build step | confirms D9's pairing and the generate-then-handoff shape; differs by making one workspace per target |
| **Yocto** multiconfig | many MACHINEs from one tree; "separate TMPDIR for the different multiconfigs is strongly recommended" | confirms D8's per-coordinate build tree, and that one-tree-many-boards is a solved shape |
| **Buildroot** | one defconfig per board; `BR2_EXTERNAL` limited to one at a time | the pole we are not choosing. Efficient "when the product definition is stable"; nano-ros systems span boards |
| **ESP-IDF** | `flasher_args.json` + `flash_project_args`; `SDKCONFIG_DEFAULTS` list | precedent for D8's manifest, including its failure mode (manifest fell behind artifacts under secure boot v2) and D10's knob |
| **Zephyr** sysbuild | `sysbuild.conf` with `SB_CONFIG_BOOTLOADER_MCUBOOT=y` | the bootloader declaration is Zephyr's own file, so D8 adds no key of ours |
| **ESP-IDF** components | `idf_component_register(SRCS … WHOLE_ARCHIVE)` — force-linking is a declared keyword, not an author-written flag | shape of D12 |
| **CMake ≥ 3.24** | `$<LINK_LIBRARY:WHOLE_ARCHIVE,…>`; docs prefer it "instead of manual implementations" | D12's future implementation. Out of reach at our 3.22 floor, which is why the keyword must own the spelling |
| **NXP RTD / EB tresos** | `.xdm` config → generated MCAL C, produced *before* the build, across GCC/IAR/DIAB/GHS | D12's "vendor tool output is committed, never invoked by us"; same shape as MCUXpresso in D11 |
| **ISO 26262 / IEC 61508** | tool qualification "applies to compilers and code generators"; "each new version … shall be qualified" | motivated the rejected third D5 mode, and kept the shape stamp that replaced it |
| **Arduino CLI** | FQBN `VENDOR:ARCHITECTURE:BOARD_ID[:MENU_ID=OPTION_ID]` | a board id can carry **options**, which D9's opaque key cannot. Not adopted — no in-tree board needs it yet, and it is additive later |

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

Four of the original set were closed by evidence already in the tree; what
remains is one scope call and one narrowed decision.

- **Do the repo's own nine workspace roots migrate** as part of this work, or
  only new user workspaces get the builder? Migrating them is the real proof it
  works; not migrating leaves two shapes in one tree. Yocto multiconfig settles
  that one-tree-many-boards is feasible, so this is purely sequencing and risk —
  and it roughly doubles or halves the campaign.

- **Does `--offline` get its own flag, or reuse what exists?** Two in-tree
  precedents narrow this. `nros setup --check` is already the "verify, name what
  is missing, fetch nothing" spelling, and `scripts/bin/cargo` records a ruling
  on the vocabulary (issue 0676): *"`--offline` is NOT in this list,
  deliberately. It restricts cargo to the local cache; it does NOT pin
  resolution … `--frozen` stays: it means `--locked --offline` by definition."*
  So the recommendation is to reuse `--check` semantics in stage 3 rather than
  mint a third word — leaving only whether production also wants a single
  top-level flag that states the guarantee.

### Closed since the first draft

- ~~What `panic` accepts~~ — it is the **existing** RFC-0077 policy enum
  (`platform` / `halt` / `own`), forwarded rather than redefined. See D5.
- ~~Deprecation window for `[deploy.*]` → `[image.*]`~~ — phase-222 already
  ran this migration and its pattern is reused verbatim: a `--help` suffix
  naming the replacement and the removal version, a one-line stderr warning on
  every invocation that still delegates, `NROS_SUPPRESS_DEPRECATION=1` for CI
  lanes, `nros doctor` flagging it in config files, integration tests covering
  all three surfaces, and deletion at the next minor version — **a version
  boundary, not a time period**.
- ~~How `nros materialize` names what it writes~~ — D6 made the **image** the
  named unit, so `nros materialize robot1` writes `src/robot1_entry/`. The
  question presupposed a launch-file-derived name, which D6 superseded.
- ~~Does `APPLICATION_CONFIG_DIR` reach `sysbuild.conf`~~ — yes; read from
  Zephyr v3.7.0's `sysbuild_kconfig.cmake`. See D8.

## Non-goals

Per-package shared libraries, an `install/` tree to source, or runtime
composition of independently built packages — colcon's model, explicitly not
nano-ros's. `nros test` and `nros flash`: rejected by RFC-0024 §9 and still
rejected here, because neither has a derivation to perform.

## Changelog

- **2026-08-02** — created as Draft; problem statement + the "front of colcon,
  not the back" framing; four open questions.
- **2026-08-26 (b)** — open-question audit. Closed three from evidence already
  in the tree: `panic` is the **existing** RFC-0077 policy enum and D5's
  `"semihosting"` example was a category error (corrected); the deprecation
  window reuses phase-222's shipped pattern, keyed to a version boundary; and
  materialize naming was already answered by D6 making the image the named unit.
  Narrowed the offline question against `nros setup --check` and issue 0676's
  `--offline`-vs-`--frozen` ruling. One scope call left open.
- **2026-08-26** — persona pass (newcomer / production / safety-critical).
  **Third D5 mode proposed and rejected**: materialising already puts the entry
  package under version control, and `nros::main!` derives at EXPANSION time, so
  the derivation is not frozen — only the shell is. Replaced by a **shape
  stamp** so that drift warns instead of passing silently (issue 0798's class).
  Adds **D12** — third-party code enters through a package, and force-linking is
  a declared keyword rather than a user-written `-Wl,--whole-archive`, because
  that construct has no rebuild edge (issue 0475) and our 3.22 CMake floor rules
  out the portable genex. Amends RFC-0026 for copied-out workspaces; records the
  determinism requirement production needs; adds the offline question.
- **2026-08-25 (d)** — closed the sysbuild open question by reading Zephyr
  v3.7.0's `sysbuild_kconfig.cmake` rather than its docs: `sysbuild.conf` IS
  resolved through `APPLICATION_CONFIG_DIR`, sysbuild caches that variable so
  one setting reaches the images too, and presence-based opt-in is Zephyr's own
  model. D10's sysbuild half is now evidence, not assumption.
- **2026-08-25 (c)** — related-work pass; two decisions reconsidered. **D5
  reversed**: Expo shipped `eject`, found it a one-way door, and replaced it
  with always-generate + declarative plugins, so known escapes (`panic`,
  `profile`) become image keys and `nros materialize` is the marked-one-way last
  resort. **D1 refined**: `[system] default_images` after PlatformIO's
  `default_envs`, but list-and-fail rather than its build-everything default.
  Adds D5.1 (`[image]` base section). Answers the sysbuild-declaration question
  from Zephyr's own `sysbuild.conf`, and the manifest question from ESP-IDF's
  `flasher_args.json` — including a completeness gate, since that manifest is on
  record falling behind its artifacts. New *Related work* table.
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
