# The example tree's layout

<!-- Moved from examples/README.md, which keeps only what a user copies from. -->

This page is the contributor half of [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md):
why the tree has the shape it has, the layout classes, and how the test
harness covers what the README's matrix lists. Read it before adding,
moving or deleting an example; [Creating Examples](creating-examples.md) is
the how-to.

## Tree history

**`stm32f4/` left this tree in phase-337 W7.a.** Its board crates carried
**zero** Runtime cells — no CI lane could boot them (the hardware is not in
the rack and QEMU models no STM32 MAC), so an in-tree example set read as a
support promise nobody verified. The same board, reached through RFC-0064's
customization ladder, is
[`book/src/extending/add-a-board.md`](../../../book/src/extending/add-a-board.md).
Cortex-M stays witnessed here by `mps2-an385-baremetal` and
`mps2-an385-freertos`, both of which a lane actually boots.

**There is no per-RMW directory level for the standard example set.** The path
is `<platform>/<language>/<example>`, never
`<platform>/<language>/<rmw>/<example>`. One example serves every backend, and
the RMW is selected at build/test time by the mechanisms the README's copy-out section describes. Phase 168 collapsed the old `rust/{zenoh,xrce,dds}/` trees for this
reason; a build recipe that reaches for `<lang>/<rmw>/<role>` is reaching for
something that no longer exists (issue 0314 — a stale `cpp/xrce` reference sat
in `just zephyr build-xrce` for months, and the abandoned directories lingered
on disk as untracked `generated/` output that read like real examples).

**No `<rmw>/` path remains.** The last one was
`zephyr/{rust,cpp}/cyclonedds/talker-aemv8r`, a *board* variant (aemv8r) that
happened to sit under a backend name; it left with the FVP code nothing ran
(issue 0537, phase-350 W3), and that board's coverage is now an `[image.fvp]`
row in `workspaces/realtime-cpp/`. Verify with
`git ls-files ':(glob)examples/*/*/{zenoh,xrce,cyclonedds,uorb}/*'` — it returns
nothing.

px4 had two, and neither named an RMW at all. `px4/rust/xrce/` is now
`px4/rust/companion/` — the axis is **where the code runs** (beside PX4, not in
firmware); the RMW is pinned by `uxrce_dds_client` and was never a choice. And
`px4/cpp/uorb/` held only `nros-register-check`, which is a link/registration
gate rather than an example, so it moved to
`packages/testing/nros-px4-register-check/` and the path level went with it.

If you are adding a `talker`/`listener`/`service-*`/`action-*` example, it goes
at `<platform>/<language>/<role>`. No exceptions.

## Layout classes

An example's layout is decided by **two** questions, and only the second has
ever been written down. That is why the first keeps getting re-derived from
whichever tree shows it most vividly, and re-attributed to that tree's platform.
(Survey and work items:
[phase-470](../../../docs/roadmap/archived/phase-470-example-layout-unification.md) (done); its gaps:
[phase-477](../../../docs/roadmap/archived/phase-477-example-gaps-and-unreported-lanes.md) (done, 2026-10-13).)

### Question 1 — who owns the link?

For **C and C++** the answer is cmake on every tree, the bare-metal one
included. **The link belongs to whoever owns the STARTUP**, and on
`mps2-an385-baremetal` the startup is Rust (`cortex-m-rt`'s reset vector, the
CMSDK Timer0 clock, the LAN9118/smoltcp bring-up), so the board exports a C
start, `nros_board_mps2_an385_start`, that runs that bring-up and then the
image's entry. The generated entry's `main` calls it, the Rust half arrives as
a Corrosion staticlib, and CMake owns the final link exactly as on an RTOS
board (RFC-0130, phase-553 W4). The `mps2-an385-baremetal/{c,cpp}/talker`
witnesses are one-package workspaces whose package is an ordinary component
(phase-553 W5), and the other bare-metal C/C++ roles are `baremetal_<role>`
images of `workspaces/{c,cpp}` (issue 2555); every such row says
`builder = "cmake"`. No C or C++ example is a cargo package; recount it the same
mechanical way:

```sh
git ls-files 'examples/*/c/*/Cargo.toml' 'examples/*/cpp/*/Cargo.toml' \
  | xargs -r -n1 dirname
```

One hit (git's `*` crosses `/`): `px4/cpp/bridge/ffi`, which is not an example
image at all — px4 is a foreign build integration (issue 1516) and that crate is
the FFI half **PX4's own** build links.

For **Rust it varies by platform**, and that variation is the whole of
what gets called "the Zephyr layout":

| | Rust leaf files | trees |
| --- | --- | --- |
| **cargo owns the link** | `Cargo.toml` + `src/main.rs` (plus `src/lib.rs` where the node logic is shared) | `linux`, `mps2-an385-baremetal`, `mps2-an385-freertos`, `qemu-armv7a-nuttx`, `threadx-linux` |
| **cmake owns the link** | `CMakeLists.txt` + `src/lib.rs` + `src/app_main.rs`; cargo emits a **staticlib**, and cmake links it into an image whose startup is C | `zephyr/rust/*`, `rv-virt-threadx/rust/*` |

The predicate is mechanical, so recount rather than trusting the lists above —
a Rust leaf holding a `CMakeLists.txt` is in the second row:

```sh
git ls-files ':(glob)examples/*/rust/*/CMakeLists.txt' | xargs -n1 dirname
```

**`rv-virt-threadx/rust/*` is why this is written down.** It carries the shape
everyone calls Zephyr's, for an unrelated cause — ThreadX RISC-V64 uses the C
startup path and Cyclone needs cmake-time C descriptors, so the link goes
through `nros_threadx_rv64_rust_app` — and it has **no `prj.conf` at all**. Both
its RMWs build that way (`builder = "cmake"` on its zenoh *and* its cyclonedds
`fixtures.toml` rows since phase-369 W2, which retired the cargo row and
`src/main.rs` with it). Meanwhile `threadx-linux/rust/*` sits in the *other*
row: same platform family, opposite answer, correctly so. **Zephyr is this row
plus `prj.conf` + `prj-<rmw>.conf`** — the conf files are what make it Zephyr,
not the staticlib.

Do not read one family's boot glue into the other's, either: the exported symbol
differs. `nros::zephyr_component_main!` exports `rust_main()`, the
zephyr-lang-rust convention that `rust_cargo_application()` consumes; the
ThreadX RV64 board's `app_main!` exports `app_main()`.

### Question 2 — is it a leaf or a workspace?

A **leaf** is `<platform>/<language>/<example>/` — one standalone copy-out
package ([RFC-0026](../../../docs/design/0026-example-directory-layout.md)), its
deployment in its own `system.toml` — except class 3a below, which owns its own
`main` and has none.

A **workspace** is a directory of packages with no root build file: `src/<pkg>/`
plus a bringup that declares the `[image.*]` rows, and the entry package for
each image is **generated**
([RFC-0098](../../../docs/design/0098-generated-leaf-build-config.md) D9, as amended
by phase-445 W5).

### The classes

| class | shape | how to recognise it | members |
| --- | --- | --- | --- |
| **1** | workspace, generated entry | `.colcon_workspace` + `src/*_bringup/system.toml`; no `*_entry` package claims the image | every workspace under `workspaces/`, for every non-Zephyr `[image.*]` row |
| **1z** | workspace, Zephyr image | the image row builds through `west`; its application is GENERATED (phase-470 W5) unless a hand-written `src/*_entry` package still claims it | every Zephyr `[image.*]` row. None in-tree is hand-written since phase-477 W1 — [issue 1288](../../../docs/issues/archived/1288-zephyr-rust-workspace-entries-not-generated.md); count them with the command below, not from this table |
| **1b** | workspace with **no bringup** | `.colcon_workspace`, no `*_bringup` — builds every package in dependency order, colcon's default | `templates/local-msg-package` (a `system.toml` beside a *package*) and `templates/workspace-shadowing` (none at all) |
| **3** | leaf, cargo owns the link, node-class | leaf has `Cargo.toml`, no `CMakeLists.txt`, and a `system.toml` | most Rust leaves (question 1, first row) |
| **3a** | leaf, cargo owns the link, **application-shaped** | `[package.metadata.nros.application]` in `Cargo.toml`; the leaf owns `main` and its executor; **no `system.toml`**, never built through `nros build` | 14 `linux/rust/*` leaves — the RTIC, custom-transport, serial, lifecycle and logging demos. They show what the node-class shape cannot express (the imperative executor API, a user-supplied transport, an RTIC app), so they are a class, not a backlog |
| **4** | leaf, cmake owns the link | leaf has `CMakeLists.txt` (`+ prj*.conf` on Zephyr) | every C and C++ leaf; plus `zephyr/rust/*` and `rv-virt-threadx/rust/*` |
| **X** | foreign-build integration | no `system.toml` and nothing for `nros build` to generate; a *foreign* build consumes the tree | `examples/px4/` — see below |

**1 and 1z are properties of an IMAGE, not of a directory.** The same workspace
is usually both: `workspaces/rust/` declares 17 `[image.*]` rows, 15 of them
class 1 and two — `zephyr`, `zephyr_robot1` — class 1z. So "which class is this
workspace?" has no answer; "which class is this image?" does.

Count the Zephyr half with:

```sh
git ls-files ':(glob)examples/workspaces/*/src/*entry*/CMakeLists.txt'   # the 1z packages
grep -rn 'entry *=' examples/workspaces/*/src/*_bringup/system.toml      # the rows that name one
```

### Two classes that deliberately do NOT exist

A name makes a shape look intentional, so these two are recorded as *absent*
rather than left unnamed for the next survey to invent:

- **"class 2", hand-written entries.** Not a class — it is class 1z before
  issue 1288. The generator reaches west for Rust and C/C++ since phase-470 W5,
  and phase-477 W1 migrated the last three hand-written entry packages (two
  bringups sharing one generated directory, a board-package preamble, a
  precedence rule with no test) — recorded in issue 1288, now resolved.
  A shape that exists because something is unfinished must not get a number
  that makes it look like a design.
- **"class 3r", Rust-only leaf families.** Not a class either. It looks like a
  missing platform port on `mps2-an385-baremetal`, and
  it is not: the bare-metal heap has existed since RFC-0034 D6 landed, and the
  C/C++ cmake seam (`cmake/platform/nano-ros-baremetal.cmake`,
  `cmake/board/nano-ros-board-mps2-an385-baremetal.cmake`) is complete — with
  **zero live consumers**. The C road was built and never driven, so this is
  feature wiring, and it dissolves into classes 3 and 4. (The
  [intentionally empty cells](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md#intentionally-empty-cells) row for
  `mps2-an385-baremetal` C/C++ used to state the old cause;
  [issue 1512](../../../docs/issues/archived/1512-c-api-does-not-reach-bare-metal.md) measured
  all of this, and the family now has six C and six C++ leaves, rooted in cargo;
  all their pairs (pubsub, service, action) run end to end under QEMU — not in that cmake seam, which still has no
  consumer and is recorded as not pursued.)

### `examples/px4/` — class X, a foreign-build integration

PX4 matches neither canonical shape, and it is **not** an outlier awaiting
migration. Unifying it would produce a firmware tree PX4's build cannot find.
Three departures, each with a cause:

1. **`src/modules/<name>/{CMakeLists.txt,Kconfig}` is PX4's layout, not ours.**
   PX4 consumes `examples/px4/cpp/firmware/` and `examples/px4/cpp/bridge/` via
   `EXTERNAL_MODULES_LOCATION` (`just px4 build-sitl-example`,
   `just px4 build-bridge-example`), which mandates that directory shape and the
   `Kconfig` beside the module. They are copy-**into**-PX4 sources, not nano-ros
   applications: no `system.toml`, and nothing for `nros build` to generate.
2. **The sub-directory axis is the TRANSPORT CASE, not the language.** PX4 is
   integrated on its two native messaging surfaces — in-firmware uORB modules
   (C++) and an XRCE-DDS companion (Rust) — so `cpp/` and `rust/` name *which
   surface*, and the language follows from that rather than the other way round.
   A reader expecting the usual language level will go looking for a `cpp/`
   companion and a `rust/` firmware module; **neither can exist.**
3. **No RMW axis at all.** In-firmware is uORB-only (the Rust uORB backend was
   retired in phase-115.K.4); the companion speaks XRCE-DDS to
   `uxrce_dds_client`. The `<rmw>` coordinate every other example carries is not
   a free choice here.

`rust/companion/{offboard-companion,px4-probe,px4-stub}/` are the three that
*could* move — ordinary host cargo bins with a `package.xml` and no
`system.toml`, so `px4/rust/<example>` would be well-formed. **Recorded as a
decision so it stops being a recurring question: they stay.** The move would
delete the one directory level that records the transport case, to buy
uniformity with leaves that do not share PX4's constraints.

Detail and prerequisites: [`examples/px4/README.md`](../../../examples/px4/README.md). The measurement
behind this section is
[issue 1516](../../../docs/issues/archived/1516-px4-is-a-foreign-build-integration.md).


## How the matrix is tested

Interop (nano ↔ real ROS 2) and bridge cells are **not** in `matrix::CELLS` and
have **no `fixtures.toml` row** — their nano side comes off the native-example /
zephyr-west-leaves lane and their peer is an ephemeral ROS 2 node / XRCE Agent /
router. They live in `packages/testing/nros-tests/src/interop.rs`
(`interop::CELLS`) with their peer + direction + build channel + test, gated by
`matrix_fixture_coverage.rs` G1–G4 so a cell cannot silently disagree with the
fixture its test runs. `ros_editions_e2e` (docker per-edition) is the edition
axis below, not one of these cells.

Edition (`ros-{humble,iron,jazzy}`; RFC-0056) is **orthogonal** to the table
above — every cell is edition-parametric via the `ros-<edition>` cargo feature
and regenerated per edition, so it is a PER-RUN global (`NROS_ROS_EDITION`,
default `jazzy`), not a per-cell column. `just ros_editions ci <distro>` runs the
whole set against one edition.

The zenoh ROS 2 interop lanes need `rmw_zenoh_cpp` in the edition's install,
and the harness PROBES for it rather than assuming (`nros-tests::ros_env`): a
host or image without it reports the cell unmet. Humble ships it as
`ros-humble-rmw-zenoh-cpp` (checked 2026-11-07), as does Jazzy. This section
used to say humble and iron ship none, and the probes followed the claim to
Jazzy (issue 2259). cyclonedds/xrce interop is edition-agnostic.

### Gap themes

Gap themes — see `docs/roadmap/archived/phase-118-example-matrix-coverage.md`
for the plan that fills these:

- **CycloneDDS matrix-fill** — Phase 175 replaced the old
  `nros-rmw-cyclonedds-staticlib` idea with CMake/Corrosion fixture
  paths: native Rust talker/listener link and exchange user data, and
  FreeRTOS plus ThreadX RISC-V64 Cyclone fixture build/link coverage is
  wired. FreeRTOS Rust Cyclone also boots and exchanges user data.
  ThreadX RISC-V64 runtime still needs participant-init diagnosis
  (Phase 177.22). dust-DDS retired in Phase 169 — there is no `dds`
  column anymore.
- **XRCE absent on every embedded platform except Zephyr** — Phase 115.K.2 header-only backend needs a Rust adapter for bare-metal targets.

### Before filling an intentionally empty cell

These cells are *deliberately blank* in the matrix above and will not be
filled without a separate scoping phase. New contributors should not
spin up examples here without first lifting the underlying constraint.

If you believe one of these cells should be filled, please open an issue
referencing the gating phase before adding directories — the lint in
Phase 118.I blocks untriaged retired RMW roots.

The README's [coverage matrix](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md#coverage-matrix)
lists the empty cells and why each is empty. Their history: the uORB cell
follows phase-115.K.4 (the Rust uORB backend deleted) and phase-277 W7 (the
README-only placeholder retired); bare-metal Cyclone is the Phase 171.C gate
decision; the NuttX Cyclone cell lost FreeRTOS to Phase 175's lwIP wiring;
ThreadX RISC-V64 Cyclone service/action was de-scoped in Phase 275 W4 and
waits on Phase 177.22; `zephyr/rust/service-client-async` was dropped by
Phase 212.M-F.5 and its tokio sibling by RFC-0113.

## Further history

- [`docs/roadmap/archived/phase-118-example-matrix-coverage.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/roadmap/archived/phase-118-example-matrix-coverage.md) — coverage-gap fill plan
- [`docs/roadmap/archived/phase-131-examples-tree-revision.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/roadmap/archived/phase-131-examples-tree-revision.md) — the tree's restructuring history
