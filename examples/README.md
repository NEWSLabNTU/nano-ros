# nano-ros Examples

Copy-out templates for users porting nano-ros to a new platform and language.

**Non-example binaries live elsewhere** — see [Where else to look](#where-else-to-look).

## Tree shape

```
examples/
├── <platform>/<language>/<example>/           # canonical
├── workspaces/<language-or-mix>/              # Node + Bringup + Entry workspaces
└── templates/<name>/                          # multi-platform recipes (Pattern A workspace, etc.)
```

- **Platform** (10): `native`, `px4`, `mps2-an385-baremetal`, `mps2-an385-freertos`, `qemu-armv7a-nuttx`, `esp32-c3-baremetal`, `rv-virt-nuttx`, `rv-virt-threadx`, `threadx-linux`, `zephyr`
- **Language**: `c`, `cpp`, `rust`
- **Example** (cases): `talker`, `listener`, `service-{server,client}`, `action-{server,client}`, `custom-msg`, plus variant suffixes: `-rtic`, `-rtic-mixed`, `-async`, `-serial`, `-aemv8r`, etc.

> **`stm32f4/` left this tree in phase-337 W7.a.** Its board crates carried
> **zero** Runtime cells — no CI lane could boot them (the hardware is not in
> the rack and QEMU models no STM32 MAC), so an in-tree example set read as a
> support promise nobody verified. The same board, reached through RFC-0064's
> customization ladder, is
> [`book/src/porting/stm32f4-out-of-tree.md`](../book/src/porting/stm32f4-out-of-tree.md).
> Cortex-M stays witnessed here by `mps2-an385-baremetal` and
> `mps2-an385-freertos`, both of which a lane actually boots.

**There is no per-RMW directory level for the standard example set.** The path
is `<platform>/<language>/<example>`, never
`<platform>/<language>/<rmw>/<example>`. One example serves every backend, and
the RMW is selected at build/test time by the mechanisms in [Building](#building)
below. Phase 168 collapsed the old `rust/{zenoh,xrce,dds}/` trees for this
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

Each example is a standalone Cargo + CMake package — no walk-up in the manifests, no workspace coupling. The tested copy-out contract (phase-277 W6):

- **Rust** — manifests declare nano-ros crates registry-style (`nros = { version = "*" }`) and carry Rust-toolchain facts only. Which board the leaf deploys to is stated once, in `system.toml` beside the manifest (`[image.<id>] board`, RFC-0098 D3); `nros sync` turns that choice into the generated build settings under `build/<image>/`, and `nros build` reads them. Copy the directory anywhere, point it at your checkout, and build:

  ```bash
  cp -r examples/native/rust/talker ~/my-talker && cd ~/my-talker
  export NROS_REPO_DIR=/path/to/nano-ros
  nros sync     # generated/ message crates + build/<image>/nros-cargo.toml
  nros build    # or: nros build native
  ```

  Driving cargo yourself is supported — hand it the file `nros sync` wrote. Run it from the directory *above* the package: the leaf's own `.cargo/` still exists and cargo would read it a second time (phase-445 W6 deletes that directory, after which the working directory stops mattering).

  ```bash
  cd ~
  cargo build --manifest-path my-talker/Cargo.toml \
              --config my-talker/build/native/nros-cargo.toml
  ```

  One caveat for a single-package **Rust** leaf: it is its own entry, so it still names its board crate in `[dependencies]`, and RFC-0098 D6's generated board dependency reaches only a workspace entry. Switching such a leaf to another board means editing `[image.*] board` **and** that dependency together — leave them disagreeing and `nros sync` reports success while the build fails inside your own crate ([issue 1305](../docs/issues/1305-single-package-board-crate-dep-not-generated.md)). A C/C++ leaf and a workspace really are one line.

- **C / C++** — every CMakeLists resolves the nano-ros root through one guard: `-DNANO_ROS_ROOT=<path>` cache var, else the `NROS_REPO_DIR` env var (exported by `activate.sh`), else the in-repo relative walk-up. That flag says where the checkout is and nothing else — the board, the RMW, the domain and the network identity come from the leaf's `system.toml`, not from `-D` flags and not from the retired `package.xml` `<nano_ros deploy= board= rmw=/>` tuple. A single-package C/C++ leaf needs no `nros sync`: its message bindings are a CMake-time output. Copy the directory anywhere, then:

  ```bash
  cp -r examples/native/c/talker ~/my-c-talker && cd ~/my-c-talker
  cmake -S . -B build -DNANO_ROS_ROOT=/path/to/nano-ros   # or: export NROS_REPO_DIR=…
  cmake --build build
  ```

  (`nros build` is the workspace verb; it does not yet resolve a single-package C/C++ leaf — [issue 1296](../docs/issues/1296-nros-build-c-leaf-bringup-name-mismatch.md).)

- Every canonical leaf ships its own `README.md` with the copy-out build/run steps and the `system.toml` that carries its deployment — the instructions travel with the directory. They are generated by [`scripts/docs/gen-example-readmes.py`](../scripts/docs/gen-example-readmes.py) (`--force` re-renders the generated pages after a template change; hand-written pages are never overwritten) and gated by `example_shape::every_canonical_leaf_has_readme`.

- Prefer vendoring the checkout into your own workspace instead? See [`templates/multi-package-workspace/`](templates/multi-package-workspace/), which documents the path-dep Pattern A layout.

Embedded targets additionally need their SDK env vars (`*_DIR`, `FREERTOS_PORT`, …) — `source activate.sh` in the nano-ros checkout provides them. The RMW is part of the deployment, so it is stated with the rest of it: `[system] rmw = "<backend>"` in the leaf's (or bringup's) `system.toml`, in one place for every language. Supported backend names are `zenoh`, `xrce`, `cyclonedds`, and `uorb`; the legacy dust-DDS `dds` backend was retired in Phase 169. Zephyr keeps its own front-end on top — the `prj-<backend>.conf` Kconfig overlay a west build selects.

## Layout classes

An example's layout is decided by **two** questions, and only the second has
ever been written down. That is why the first keeps getting re-derived from
whichever tree shows it most vividly, and re-attributed to that tree's platform.
(Survey and work items:
[phase-470](../docs/roadmap/archived/phase-470-example-layout-unification.md) (done); open gaps:
[phase-477](../docs/roadmap/phase-477-example-gaps-and-unreported-lanes.md).)

### Question 1 — who owns the link?

For **C and C++** the answer is cmake on every tree but one, because cmake is the
only C build here. The exception is a board with no RTOS, and it is this same
rule taken one step further rather than a hole in it: **the link belongs to
whoever owns the STARTUP**, and on `mps2-an385-baremetal` nothing owns it in C —
the reset vector is `cortex-m-rt`, the clock is
`nros_platform_mps2_an385::clock`, and the LAN9118/smoltcp bring-up is
`nros_board_mps2_an385::init_hardware`, none of them with a C entry point
(`nros-platform-mps2-an385` holds zero `.c` files, where every RTOS port holds a
`platform.c`). So every `mps2-an385-baremetal/{c,cpp}/*` leaf is cargo-rooted:
`src/main.rs` boots the board and calls `app_main()`, `build.rs` compiles the
leaf's `src/*.c` (or `src/talker.cpp`) with `cc`, and `nano_ros_add_executable`
is not available on that platform. Issue 1512; `c/talker/README.md` carries the
table. Their `[[fixture]]` rows say `builder = "cargo"`, and the fixture driver
and staleness probe pick each row's lane by that declared fact, never by
`lang`. Recount it the same mechanical way — a C or C++ leaf that is a cargo
package:

```sh
git ls-files 'examples/*/c/*/Cargo.toml' 'examples/*/cpp/*/Cargo.toml' \
  | xargs -r -n1 dirname
```

Eight hits, and seven are images: the six `mps2-an385-baremetal/c/*` roles and
`mps2-an385-baremetal/cpp/talker` above. The eighth is `px4/cpp/bridge/ffi`,
which is not an example image at all — px4 is a foreign build integration
(issue 1516) and that crate is the FFI half **PX4's own** build links. None has a
`CMakeLists.txt`, so the count is the same either way; the distinction is whose
build owns the image, which is the question this section asks.

For **Rust it varies by platform**, and that variation is the whole of
what gets called "the Zephyr layout":

| | Rust leaf files | trees |
| --- | --- | --- |
| **cargo owns the link** | `Cargo.toml` + `src/main.rs` (plus `src/lib.rs` where the node logic is shared) | `native`, `mps2-an385-baremetal`, `mps2-an385-freertos`, `esp32-c3-baremetal`, `qemu-armv7a-nuttx`, `threadx-linux` |
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
package ([RFC-0026](../docs/design/0026-example-directory-layout.md)), its
deployment in its own `system.toml` — except class 3a below, which owns its own
`main` and has none.

A **workspace** is a directory of packages with no root build file: `src/<pkg>/`
plus a bringup that declares the `[image.*]` rows, and the entry package for
each image is **generated**
([RFC-0098](../docs/design/0098-generated-leaf-build-config.md) D9, as amended
by phase-445 W5).

### The classes

| class | shape | how to recognise it | members |
| --- | --- | --- | --- |
| **1** | workspace, generated entry | `.colcon_workspace` + `src/*_bringup/system.toml`; no `*_entry` package claims the image | every workspace under `workspaces/`, for every non-Zephyr `[image.*]` row |
| **1z** | workspace, Zephyr image | the image row builds through `west`; its application is GENERATED (phase-470 W5) unless a hand-written `src/*_entry` package still claims it | every Zephyr `[image.*]` row. The few still hand-written are each blocked for a recorded reason — [issue 1288](../docs/issues/1288-zephyr-rust-workspace-entries-not-generated.md); count them with the command below, not from this table |
| **1b** | workspace with **no bringup** | `.colcon_workspace`, no `*_bringup` — builds every package in dependency order, colcon's default | `templates/local-msg-package` (a `system.toml` beside a *package*) and `templates/workspace-shadowing` (none at all) |
| **3** | leaf, cargo owns the link, node-class | leaf has `Cargo.toml`, no `CMakeLists.txt`, and a `system.toml` | most Rust leaves (question 1, first row) **and** the cargo-rooted bare-metal C/C++ leaves |
| **3a** | leaf, cargo owns the link, **application-shaped** | `[package.metadata.nros.application]` in `Cargo.toml`; the leaf owns `main` and its executor; **no `system.toml`**, never built through `nros build` | 16 `native/rust/*` leaves — the RTIC, async, custom-transport, serial, lifecycle and logging demos. They show what the node-class shape cannot express (the imperative executor API, a user-supplied transport, an RTIC app), so they are a class, not a backlog |
| **4** | leaf, cmake owns the link | leaf has `CMakeLists.txt` (`+ prj*.conf` on Zephyr) | every C and C++ leaf on a platform whose startup is C — all but the bare-metal ones; plus `zephyr/rust/*` and `rv-virt-threadx/rust/*` |
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
  and the entry packages still hand-written are each blocked by something OTHER
  than the generator (two bringups sharing one generated directory, a board this
  host cannot build, a precedence rule with no test) — recorded in issue 1288.
  A shape that exists because something is unfinished must not get a number
  that makes it look like a design.
- **"class 3r", Rust-only leaf families.** Not a class either. It looks like a
  missing platform port on `mps2-an385-baremetal` and `esp32-c3-baremetal`, and
  it is not: the bare-metal heap has existed since RFC-0034 D6 landed, and the
  C/C++ cmake seam (`cmake/platform/nano-ros-baremetal.cmake`,
  `cmake/board/nano-ros-board-mps2-an385-baremetal.cmake`) is complete — with
  **zero live consumers**. The C road was built and never driven, so this is
  feature wiring, and it dissolves into classes 3 and 4. (The
  [Intentionally empty cells](#intentionally-empty-cells) row for
  `mps2-an385-baremetal` C/C++ used to state the old cause;
  [issue 1512](../docs/issues/1512-c-api-does-not-reach-bare-metal.md) measured
  all of this, and the family now has six C leaves and a C++ talker — rooted in
  cargo, not in that cmake seam, which still has no consumer.)

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

Detail and prerequisites: [`px4/README.md`](px4/README.md). The measurement
behind this section is
[issue 1516](../docs/issues/archived/1516-px4-is-a-foreign-build-integration.md).

## Coverage matrix

Cell content: `<count>` of `talker|listener|service-{server,client}|action-{server,client}` cases present (max 6). `+` suffix indicates extras (custom-msg, parameters, lifecycle, RTIC variants, custom-transport, serial, embassy, async, etc.).

| Platform                  | Language | zenoh | xrce | cyclonedds | uorb |
|---------------------------|----------|-------|------|------------|------|
| `native`                  | c        | 6+    | 6    | 6          | –    |
| `native`                  | cpp      | 6+    | –    | 6          | –    |
| `native`                  | rust     | 6+    | 6+   | 6          | –    |
| `px4`                     | cpp      | –     | –    | –          | – ²  |
| `px4`                     | rust     | –     | companion+stub | – | –    |
| `mps2-an385-baremetal`      | c        | 6 ³   | –    | –          | –    |
| `mps2-an385-baremetal`      | cpp      | 1 ³   | –    | –          | –    |
| `mps2-an385-baremetal`      | rust     | 6+rtic+serial | – | –     | –    |
| `mps2-an385-freertos`       | c        | 6     | –    | –          | –    |
| `mps2-an385-freertos`       | cpp      | 6     | –    | –          | –    |
| `mps2-an385-freertos`       | rust     | 6     | –    | –          | –    |
| `qemu-armv7a-nuttx`          | c        | 6     | –    | –          | –    |
| `qemu-armv7a-nuttx`          | cpp      | 6     | –    | –          | –    |
| `qemu-armv7a-nuttx`          | rust     | 6     | –    | –          | –    |
| `esp32-c3-baremetal`    | rust     | 2     | –    | –          | –    |
| `rv-virt-nuttx`        | c        | 1     | –    | –          | –    |
| `rv-virt-threadx`    | c        | 6 ¹   | –    | –          | –    |
| `rv-virt-threadx`    | cpp      | 6 ¹   | –    | –          | –    |
| `rv-virt-threadx`    | rust     | 6 ¹   | –    | –          | –    |
| `threadx-linux`           | c        | 6     | –    | 6          | –    |
| `threadx-linux`           | cpp      | 6     | –    | 2 (pub/sub) | –   |
| `threadx-linux`           | rust     | 6     | –    | – (intentionally empty, see below) | – |
| `zephyr`                  | c        | 6     | 6    | 6          | –    |
| `zephyr`                  | cpp      | 6     | 6    | 6+aemv8r   | –    |
| `zephyr`                  | rust     | 6     | 6    | 6+aemv8r   | –    |

² px4's in-firmware uORB surface is `packages/testing/nros-px4-register-check/`,
not an example: it is a link/registration gate whose build IS the assertion, so
there is nothing to copy out. phase-316 W3.1 moved it out of this tree.

¹ `rv-virt-threadx` action lanes are build-only: the action
example fixtures compile, but the action runtime lanes were
deliberately dropped from the run matrix in 182.5 (pub/sub + service
remain runtime-tested).

³ `mps2-an385-baremetal` C/C++ leaves are cargo-rooted (see "Question 1"
above) and build-only: each `[[fixture]]` row builds in the `baremetal` lane and
the image boots in QEMU as far as the session open, but the C/C++ entry dials
`NROS_ENTRY_LOCATOR`'s empty bottom rung — the backend's own default, which the
guest cannot reach — instead of its `system.toml` locator. Baking that is what
the runtime lane needs (issue 1512). `cpp/` has the talker only.

`rv-virt-nuttx` currently ships only `c/talker`, built by the separate
`build-riscv-c` recipe in `just/nuttx.just` (its own riscv toolchain/board
lane — not the `qemu-armv7a-nuttx` build path above).

### Interop & bridge coverage (issue 0352 / phase-324)

Interop (nano ↔ real ROS 2) and bridge cells are **not** in `matrix::CELLS` and
have **no `fixtures.toml` row** — their nano side comes off the native-example /
zephyr-west-leaves lane and their peer is an ephemeral ROS 2 node / XRCE Agent /
router. They live in `packages/testing/nros-tests/src/interop.rs`
(`interop::CELLS`) with their peer + direction + build channel + test, gated by
`matrix_fixture_coverage.rs` G1–G4 so a cell cannot silently disagree with the
fixture its test runs. `ros_editions_e2e` (docker per-edition) is the edition
axis below, not one of these cells.

### ROS edition axis (issue 0327)

Edition (`ros-{humble,iron,jazzy}`; RFC-0056) is **orthogonal** to the table
above — every cell is edition-parametric via the `ros-<edition>` cargo feature
and regenerated per edition, so it is a PER-RUN global (`NROS_ROS_EDITION`,
default `jazzy`), not a per-cell column. `just ros_editions ci <distro>` runs the
whole set against one edition.

Carve-out: **humble and iron ship no `rmw_zenoh_cpp` apt package**, so the zenoh
ROS 2 interop lanes run **only on jazzy** — the two humble/iron zenoh-interop
cells are permanently skipped (was recorded only as a code comment in
`nros-tests::ros_env`; surfaced here per issue 0327). cyclonedds/xrce interop is
edition-agnostic.

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

### Intentionally empty cells

These cells are *deliberately blank* in the matrix above and will not be
filled without a separate scoping phase. New contributors should not
spin up examples here without first lifting the underlying constraint.

| Cell                                                   | Why empty                                                                                                                                                                                                                                                          | Lift requires                                                                                                                                                                  |
|--------------------------------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `mps2-an385-baremetal/cpp/*` beyond `talker`, and a RUNTIME lane for any of its C/C++ leaves | **The cause this row used to give was stale in all five of its terms** (issue 1512). It read "`nros-c` / `nros-cpp` … assume a hosted RTOS for startup, heap, libc, RNG, and clock": the heap is `nros-platform-mps2-an385`'s 128 KB `FreeListHeap` in `.bss`, libc and RNG are `nros-baremetal-common`, the clock is the `cffi-export` `nros_platform_clock_ns`, and startup is `cortex-m-rt`. What was missing was feature wiring, and it landed: six C roles and `cpp/talker` build in the `baremetal` fixture lane (`builder = "cargo"` rows) and boot in QEMU to the session open. What is *actually* missing is narrower: every one of them dials `NROS_ENTRY_LOCATOR`'s empty bottom rung instead of its `system.toml` locator, so there is no runtime lane (the `matrix::CELLS` cells are `BuildOnly`); and the C++ roles beyond `talker` are unwritten — mechanical now that `nros generate cpp` has a standalone (`package.xml`) front door. Note the SHAPE: the link root here is Rust (`src/main.rs` boots the board, `build.rs` compiles the C/C++), because a board with no RTOS has no C startup and no C-reachable board init — `nano_ros_add_executable` is not available on this platform and that is a property of the board, not of the C API. | issue 1512: bake the entry locator into the C/C++ compile of a cargo-rooted image (an entry-codegen job — `<nros/entry_config.h>` stays the one producer), then the five remaining `cpp/` roles. |
| `esp32-c3-baremetal/{c,cpp}/*`                       | An AUTHORED decision, not a missing port (issue 1512). `PlatformKind::Esp32::cmake_deploy()` returns `None`, so `nros ws leaf-system` derives no `NANO_ROS_PLATFORM` for a C/C++ leaf here: this tree is the no-IDF / pure-Rust HAL path (`esp-hal`), and C/C++ on the same silicon is the ESP-IDF component road (phase-139), which belongs under a hypothetical `esp32-idf/` tree. A `cmake/board/nano-ros-board-esp32-c3-baremetal.cmake` overlay does exist and says so in its own header ("kept for symmetry with the other board overlays"); it is for a non-IDF C parent build, not for a leaf here. | Nothing to lift on this tree. Lifting the sibling means deciding whether ESP-IDF-hosted C/C++ examples deserve their own platform dir. |
| `px4/{c,rust}/*` (px4 has no example-tree uORB cell)   | PX4 integration is uORB-only (the platform's native pub/sub), and Phase 115.K.4 collapsed `nros-rmw-uorb` to a single C++ port (the legacy Rust crate was deleted). `packages/testing/nros-px4-register-check/` is the canonical surface (the former `examples/px4/rust/uorb/` README-only placeholder was retired in phase-277 W7). | Won't lift: C is not on the PX4 module API, and the Rust uORB backend was retired in Phase 115.K.4 (see `docs/roadmap/phase-115-runtime-transport-vtable.md`). No C/Rust PX4 examples are planned.                  |
| `cyclonedds` on bare-metal (`mps2-an385-baremetal`, `esp32-c3-baremetal`) | Cyclone DDS requires a hosted runtime — BSD sockets, threads, heap, libc. Pure Cortex-M / esp-hal bare-metal targets have none, so the C++ Cyclone stack cannot run (Phase 171.C.gate decision). | Won't lift on bare-metal. Cyclone DDS is the hosted-platform DDS backend; embedded targets use the zenoh-pico or XRCE backends instead. |
| `cyclonedds` on NuttX QEMU (`qemu-armv7a-nuttx` × all langs) | Deferred-upstream: a Cyclone DDS NuttX socket-shim port is an upstream-scale effort not attempted in nano-ros. FreeRTOS is no longer in this bucket; Phase 175 added FreeRTOS/lwIP Cyclone fixture wiring. | An upstream Cyclone DDS NuttX port (socket shim + config + heap budget), then a nano-ros example cell. |
| pure-cargo `cyclonedds` Rust binaries on `native` / `threadx-linux` | Still intentionally unsupported: `nros-rmw-cyclonedds-sys` exposes only the C register shim, so a plain Cargo build has no way to build+link the C++ Cyclone lib + `libddsc`. Native Rust Cyclone now uses the Phase 175 CMake/Corrosion path instead. | Use the CMake/Corrosion fixture path for Cyclone-backed Rust examples, or scope a new staticlib crate separately. |
| `cyclonedds` service/action on `rv-virt-threadx` (`c`/`cpp`) — talker+listener only | De-scoped (Phase 275 W4): Cyclone on ThreadX RISC-V64 is *experimental* and its runtime still needs participant-init diagnosis (Phase 177.22). Only talker/listener are wired, exercised by the AF_UNIX two-QEMU pub/sub e2e (`test_threadx_riscv64_cyclonedds_two_qemu_pubsub`). Service/action would need bidirectional RTPS discovery over that L2 tunnel, unproven on this port; the zenoh RMW covers the full 6-role set here. | Land the Phase 177.22 participant-init fix, then a two-QEMU Cyclone request/response e2e before adding `service-*`/`action-*` cyclone fixture rows. |
| `zephyr/rust/service-client-async` | Dropped 2026-06-02 per Phase 212.M-F.5 — the Embassy-driven async client has no `Node` / `ExecutableNode` analogue today. The native tokio sibling (`examples/native/rust/service-client-async/`) remains as the async-client reference. | Decide on an async executable-node trait (deferred until L-Wave / runtime authors pick the path), then re-introduce the example. |

If you believe one of these cells should be filled, please open an issue
referencing the gating phase before adding directories — the lint in
Phase 118.I blocks untriaged retired RMW roots.

## Sibling categories

### Cross-RMW bridges live in `workspaces/`, not a category of their own

There is no `bridges/` category. It held two hand-written time-triggered
gateways (`tt-zenoh-to-xrce`, `tt-zenoh-to-cyclonedds`) that no fixture row,
lane or `system.toml` ever built, and phase-477 deleted them. A
bridge is a `[[bridge]]` row in a workspace's `system.toml`:
[`workspaces/bridge-cyclonedds/`](workspaces/bridge-cyclonedds/) and
[`workspaces/bridge-xrce/`](workspaces/bridge-xrce/) below, both built and
e2e-tested. See
[`book/src/user-guide/cross-backend-bridges.md`](../book/src/user-guide/cross-backend-bridges.md)
for the model and the build knobs.

### `workspaces/` — product-shaped multi-package workspaces

Workspaces that follow the book's Node package + Bringup package workflow —
[class 1](#the-classes), with the Zephyr rows of ten of them still class 1z. A
workspace is a directory of packages — no root `Cargo.toml`, no root
`CMakeLists.txt`, and no entry package to write: the entry is generated per
`[image.*]` (RFC-0098 D9). Build one with

```bash
cd <ws>
nros sync
nros build              # every [image.*]; or: nros build <image-id>
```

Everything generated lands under `build/`, `dist/` and `log/`. Zephyr stays the
carve-out: `west build` is still its build verb, with `nros sync` before it.

**A FEATURE is a node package and a CONFIGURATION is a fixture axis — never a
new directory** (RFC-0066, phase-331). The per-capability `ws-<topic>-<lang>`
directories this list used to enumerate are **gone**: QoS, params, lifecycle,
custom-msg and remap folded into `features/`, and the three `ws-safety-*` into
`safety/`. Don't reintroduce one. Naming rules and the workspace classes →
[`workspaces/README-layout.md`](workspaces/README-layout.md).

Every workspace has its own README; one line each. Recount with
`ls -d examples/workspaces/*/` rather than trusting this list:

| Workspace | What it shows |
| --- | --- |
| `rust/` | base starter: Rust Node pkgs + the Node / Bringup / Entry split in pure Rust |
| `c/` | base starter: C Node pkgs |
| `cpp/` | base starter: C++ Node pkgs |
| `mixed/` | the language SEAM: one entry, components from C, C++ and Rust |
| `features/` | the capability demos in one workspace — params, lifecycle, QoS, custom messages, remap — native only |
| `safety/` | E2E message integrity in one workspace (auto CRC-32 + seq, validated subscription reports faults) |
| `bridge-cyclonedds/` | declarative `[[bridge]]`: `/chatter` zenoh → cyclonedds in one process, no user bridge code |
| `bridge-xrce/` | the same declarative bridge, XRCE variant (zenoh → XRCE Agent → DDS) |
| `launch/` | advanced launch composition — topology lives in the launch XML (launch v1) |
| `managed/` | the lifecycle shape `features/` cannot hold: a C++ node that manages ITSELF |
| `realtime-c/` | two nodes on two scheduling tiers (`/ctrl` 10 ms high, `/telem` 100 ms low) from config |
| `realtime-cpp/` | the C++ base of the same two-tier scheduling differentiator (RFC-0015 §4.2) |
| `realtime-cpp-subnode-portable/` | the identical `SubNode` under tiers renamed `fast`/`bulk` — tier names are deployment-owned |
| `realtime-rust/` | the Rust projection of the scheduling-tiers differentiator |
| `derived-tiers-cpp/` | four C++ components and **no authored tiers** — the Safety-Island-shaped sizing fixture |
| `sizing/` | a node the SystemModel cannot count (six timers, no subscription) — the executor-sizing showcase |

### `templates/` — multi-platform copy-out recipes

Patterns that span platforms (multi-package workspace layouts, mixed C / C++ / Rust packages sharing one nano-ros install, etc.).

- `templates/multi-package-workspace/` — Pattern A workspace (C talker, C++ listener, Rust publisher under one nano-ros install). It declares **no** workspace root by either tracked spelling and has no bringup: it is the path-dep pattern, not a colcon workspace, so it is outside the classes above.
- `templates/local-msg-package/` and `templates/workspace-shadowing/` are the two **[class 1b](#the-classes)** trees — a `.colcon_workspace` with no bringup. `nros build` builds every package in dependency order, colcon's default (RFC-0098 D9 as amended by phase-445 W5). `workspace-shadowing` carries no `system.toml` at all; `local-msg-package` carries one beside a *package*.

## Where else to look

Test / bench / smoke binaries are NOT under `examples/`. They live with the integration-test crate so the example tree stays a clean copy-out surface.

- **`packages/testing/nros-bench/`** — perf, fairness, stress, large-msg
  - `executor-fairness`, `stress-{zenoh,xrce}`, `large-msg-{xrce,baremetal}`, `wcet-cycles-qemu`
- **`packages/testing/nros-smoke/`** — driver / board bringup (no nros API)
  - `stm32f4-smoltcp-echo`, `esp32-hello-world`
- **`packages/testing/nros-tests/bins/`** — fixture binaries that integration tests build & invoke
  - `cdr-roundtrip-qemu`, `lan9118-qemu`
- **`packages/testing/nros-px4-register-check/`** — PX4 module whose *build* is the assertion
  - links `nros-rmw-uorb` inside a real `px4_add_module()` context against real
    PX4 headers; run via `just px4 build-sitl-cpp`. Was `examples/px4/cpp/uorb/`
    until phase-316 W3.1 — it produces nothing to copy out and nothing to run.

Each is a standalone Cargo package with an empty `[workspace]` table (they nest under the `nros-tests` workspace member).

## Consumption profile per platform

Each `examples/<plat>/` tree maps to one of the seven consumption
profiles from [`book/src/concepts/board-integration.md`](../book/src/concepts/board-integration.md).
The mapping tells you which guide to read when porting one of these
examples to your own board.

| `examples/<plat>/` | Profile | Guide |
|---|---|---|
| `native/` | Host native (Linux) | `nros sync` + `nros build` — no integration shell needed. |
| `mps2-an385-baremetal/` | Cargo-first bare-metal | [Generic board crate](../book/src/concepts/board-integration.md#generic-board-crate) (`nros-board-baremetal-cortex-m`) |
| `mps2-an385-freertos/` | Cargo-first FreeRTOS | [Generic board crate](../book/src/concepts/board-integration.md#generic-board-crate) (`nros-board-freertos`); reference overlay `nros-board-mps2-an385-freertos`. For STM32 / NXP / Espressif FreeRTOS, write a [vendor overlay](../book/src/porting/vendor-overlay.md). |
| `qemu-armv7a-nuttx/` | NuttX native shell | [NuttX integration shell](../book/src/getting-started/integration-nuttx.md) — `apps/external/nano-ros/`. |
| `esp32-c3-baremetal/` | Cargo-first bare-metal | Bare-metal `esp-hal` path; same generic-crate flow as `mps2-an385-baremetal`. |
| `rv-virt-threadx/` | Cargo-first ThreadX | [Generic board crate](../book/src/concepts/board-integration.md#generic-board-crate) (`nros-board-threadx`); reference overlay `nros-board-threadx-qemu-riscv64`. For Renesas Synergy / STM32 X-CUBE-AZRTOS / NXP MCUXpresso ThreadX, write a [vendor overlay](../book/src/porting/vendor-overlay.md). |
| `threadx-linux/` | Linux sim (CI) | Same as `rv-virt-threadx` but with NSOS host-kernel sockets shim. |
| `zephyr/` | Zephyr native shell | [Zephyr integration shell](../book/src/getting-started/integration-zephyr.md) — `projects:` entry in your `west.yml`. |
| `px4/` | PX4 native shell | [PX4 integration shell](../book/src/getting-started/integration-px4.md) — `EXTERNAL_MODULES_LOCATION`. |

When in doubt, read [Board Integration](../book/src/concepts/board-integration.md)
first — it explains why each profile exists and which one fits your
project's build system.

## Quick start

Each block assumes a zenoh router running on `tcp/127.0.0.1:7447` — `ros2 run rmw_zenoh_cpp rmw_zenohd` (contributor shortcut inside a checkout: `just native zenohd`). C/C++ examples resolve the nano-ros root through the `-DNANO_ROS_ROOT` / `NROS_REPO_DIR` guard described in the copy-out contract above; no install step required.

### Native Rust + zenoh

```bash
# terminal 2
cd examples/native/rust/talker && nros sync && nros build
./build/native/target/debug/talker

# terminal 3
cd examples/native/rust/listener && nros sync && nros build
./build/native/target/debug/listener
```

### QEMU bare-metal Cortex-M3 (MPS2-AN385)

```bash
just setup qemu
just qemu build
just qemu talker      # spawns QEMU + nros-rs-talker
```

### Zephyr (native_sim) C + Cyclone DDS

```bash
just setup zephyr
source ~/nano-ros-workspace/env.sh
west build -b native_sim/native/64 nano-ros/examples/zephyr/c/talker \
  -- -DCONF_FILE="prj.conf;prj-cyclonedds.conf"
./build/zephyr/zephyr.exe
```

## ROS 2 interoperability

nano-ros pubs/subs are rmw_zenoh-compatible. Quickest round-trip:

```bash
# terminal 1
ros2 run rmw_zenoh_cpp rmw_zenohd

# terminal 2
cd examples/native/rust/talker && nros sync && nros build
./build/native/target/debug/talker

# terminal 3
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
```

For DDS-side interop (cyclonedds), see `docs/reference/rmw_zenoh_interop.md`.

## See also

- [`CLAUDE.md`](../CLAUDE.md) — development guidelines, "Examples = Standalone Projects" section
- [`docs/guides/zephyr-setup.md`](../docs/guides/zephyr-setup.md) — Zephyr workspace bootstrap
- [`docs/reference/rmw_zenoh_interop.md`](../docs/reference/rmw_zenoh_interop.md) — ROS 2 wire protocol
- [`docs/roadmap/archived/phase-118-example-matrix-coverage.md`](../docs/roadmap/archived/phase-118-example-matrix-coverage.md) — coverage-gap fill plan
- [`docs/roadmap/archived/phase-131-examples-tree-revision.md`](../docs/roadmap/archived/phase-131-examples-tree-revision.md) — this tree's restructuring history
