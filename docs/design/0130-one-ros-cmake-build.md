---
rfc: 0130
title: "One C/C++ build method: a ROS 2 CMake package, on every board"
status: Draft
since: 2026-11
last-reviewed: 2026-11
implements-tracked-by: [phase-553]
amends: [0065, 0024]
supersedes: []
superseded-by: null
---

# RFC-0130 — One C/C++ build method: a ROS 2 CMake package, on every board

**Status:** Draft (2026-11-12). Direction ACCEPTED by the maintainer
(2026-11-12, "Accept the unified ROS-CMake build"); the RFC stays Draft until
D6's work items land and dep-chain and the stack floor are measured on the new
road. The work is [phase-553](../roadmap/phase-553-one-ros-cmake-build.md). Amends RFC-0065 D3 and RFC-0024 §6.3 for boards whose startup is Rust.

## Ruling (2026-11-12)

> "Accept the unified ROS-CMake build."

One C/C++ build method on every board: an ordinary ROS 2
`ament_cmake(_auto)`-style package. On a board whose link root is Rust (bare
metal mps2-an385), CMake owns the final link, with the Rust runtime and the
board's startup as a Corrosion staticlib. Required, from the measurements in
D4/D5:

1. ONE libc and heap owner in a C-rooted image: newlib owns libc, and the Rust
   bare-metal crates stop exporting `snprintf`/`vsnprintf`/`strlen`/`memcpy`/
   `malloc` and their kin in that configuration (feature-gated), so C `printf`
   formats.
2. C++ static constructors run at startup (`__libc_init_array` from
   cortex-m-rt's `pre_init` or an equivalent).
3. No `-lgcc`, as measured.

It retires the cargo-entry design (never built), `nros new`'s refusal of C/C++
on bare metal, and, as a later migration, the ten cargo-crate bare-metal C/C++
workspaces.

## Direction (maintainer, 2026-11-12)

> "I would present one unified build method for users, not adding many ways to
> confuse them. My idea is to follow ROS CMake project convention."
>
> "I prefer to let users author pure CMake projects without writing Cargo and
> boilerplates on user side."

## D1 — The user writes a ROS 2 package; nothing else

A C/C++ node package on ANY board, bare metal included, is a `package.xml`
plus a `CMakeLists.txt` written as a ROS 2 user writes one. Two spellings are
accepted, both already implemented (`cmake/NanoRosAmentSurface.cmake`,
`cmake/NanoRosNodeRegister.cmake`):

Stock ROS 2 (the porting spelling):

```cmake
cmake_minimum_required(VERSION 3.22)
project(talker_pkg LANGUAGES CXX)
find_package(ament_cmake_auto REQUIRED)
ament_auto_find_build_dependencies()
ament_auto_add_library(talker_lib STATIC src/Talker.cpp)
rclcpp_components_register_node(talker_lib PLUGIN "talker_pkg::Talker" EXECUTABLE talker)
ament_auto_package()
```

nano-ros native (what the examples use today):

```cmake
find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)
nano_ros_auto_add_library(talker_lib STATIC src/Talker.cpp)
nros_components_register_node(talker_lib PLUGIN talker_pkg::Talker EXECUTABLE talker SHAPE configure)
```

No `Cargo.toml`, no `build.rs`, no `main`, no board name in the package. The
board, RMW and network are the bringup's `system.toml`. (Open: whether the
book teaches one spelling or both; this RFC only requires that neither needs
anything board-specific.)

## D2 — CMake owns the final link on every board where a C/C++ package is in the image

RFC-0065 D3's rule ("any C/C++ package in the set → cmake") now holds on
boards whose startup is Rust too. What changes is HOW the Rust half enters on
such a board: the nros runtime AND the board's startup (reset handler, vector
table, linker script, hardware/network bring-up) arrive as ONE Rust staticlib
through Corrosion, and the C toolchain links the image. The generated
`entry.c`/`.cpp` (the same C/C++ entry pack every RTOS uses) owns `main`.

Boot on a Rust-startup board:
`Reset` (cortex-m-rt, from the staticlib) → C `main` (generated entry) →
`nros_board_<board>_start(app)` (C-ABI, Rust: log sinks, `init_hardware`,
network) → `app` → `nros_board_rtos_run_components_in(...)` (the shared,
kernel-agnostic C runner in `nros-board-common/c`) → `__nros_entry_setup` →
each component's `__nros_c_component_<pkg>_create/_configure` (or the C++
component install).

RFC-0024 §6.3's "cmake cannot be consumed as a cargo target" is unchanged and
is the reason for this direction: the cargo-rooted road would have had to
re-implement the user's CMakeLists (or ignore it).

## D3 — What retires

- The ten `examples/mps2-an385-baremetal/{c,cpp}/{listener,service-*,action-*}`
  one-package workspaces whose package is a hand-written cargo crate
  (`Cargo.toml` + `build.rs` + `src/main.rs`, issue 1512 shape, moved in by
  phase-535 W6): they become ordinary CMake component packages in the
  language workspaces' bringups, like every RTOS.
- The two bare-metal C/C++ talker witnesses: same, no cargo.
- `nros-cc-flags`' leaf-only modules (`app_main`, `entry_config::bake`,
  `backends`) lose their consumers once no leaf compiles C through cargo
  (to be measured; `backends` has a cmake twin already).
- `nros new --lang c|cpp` stops refusing bare metal (issue 2376's ruling is
  superseded by this direction).
- The cargo-entry design (a generated cargo entry compiling component
  sources by convention) is not built.

## D4 — What the board/platform crates need (measured 2026-11-12)

Probe (2026-11-12, kept out of the tree): A `staticlib` crate depending on
`nros-board-mps2-an385`, `nros-c`, `nros-cpp` (`platform-mps2-an385`,
`rmw-cffi`, `alloc`) and `nros-rmw-zenoh`, exporting one C-ABI
`nros_board_start(app)` that wraps `run_bare(Config::from_image(), app)`.

1. **Startup + linker script from a staticlib: works.** The C toolchain links
   `main.o` + the staticlib with `-nostartfiles -nostdlib -Tlink.x` and three
   `-L` dirs (cortex-m-rt's `link.x`, the board's `memory.x`, the PAC's
   `device.x`, all build-script outputs). `Reset`/`__RESET_VECTOR` come from
   the archive; cortex-m-rt's `Reset` calls the C `main`. Booted in QEMU
   mps2-an385: board banner, LAN9118 up, IP 10.0.2.10, clean exit.
2. **A C component publishes.** Adding `Talker.c` (`RCLC_C_COMPONENT`,
   `-DNROS_PKG_NAME=<pkg>`), `nros interface generate c` bindings,
   `nros_rtos_run_components.c` and a backend-registration TU: the image boots,
   opens the zenoh session to a router at 10.0.2.2:10300 and publishes every
   second. Text 438 KB, bss 547 KB.
3. **Needed in the crates:** a C-ABI board start (`nros_board_<board>_start`)
   in the board crate (or a thin board-seam staticlib crate, so `nros-c` never
   names a board, RFC-0064); the Corrosion staticlib must carry cortex-m-rt and
   the PAC so their link scripts are reachable, and cmake must learn the three
   `-L` dirs from the cargo build (an `OUT_DIR`-published path, the same
   problem `DEP_*` metadata solves); a BoardFamily row in `nros-entry-lower`
   whose runner is `nros_board_rtos_run_components_in` (needed on this road
   too).

## D5 — Open risks (measured where marked)

- **libc ownership (measured, real).** `nros-baremetal-common` exports
  `snprintf`, `vsnprintf`, `strlen`, `memcpy`, and `nros-platform-mps2-an385`
  exports `malloc`. In the C link they win over newlib: the component's
  `snprintf` produced an empty string and `RCLC_LOG_INFO` printed `%d`
  unformatted. A C-rooted image needs ONE libc owner; the ruling picks
  newlib, with the Rust shims feature-gated off in that configuration (D6
  item 1). The cargo-rooted
  leaves have the same shims; whether their printf output is right was not
  re-measured here.
- **Two heaps (measured, latent).** newlib's `_sbrk` needs `end`; the probe
  set `end = __sheap`, the region the Rust allocator may also use. One heap
  owner is required (same decision as libc).
- **`-lgcc` (measured).** Linking libgcc pulls ARM unwind code needing
  `__exidx_start/__exidx_end` and `abort`; the probe links without it. C++
  components need a measured libgcc/libstdc++ subset (`-fno-exceptions
  -fno-rtti`, no `.init_array` run by cortex-m-rt — static constructors do not
  run, issue 1512 already recorded this).
- **Executor storage size.** The generated entry must take
  `NROS_CPP_EXECUTOR_STORAGE_SIZE` from the build (the probe's hand size was
  refused: 83,832 bytes needed); the RTOS entries already do this.
- **dep-chain**: its cells are Rust talkers resolved through cargo; this road
  adds no cell there. A C bare-metal image's cargo half is the Corrosion
  staticlib, whose dep chain is the same crates. Not measured.
- **Size measurement** (`mem-report`, sizing descriptor): reads the ELF and the
  model, both unchanged in kind. Not measured.
- **Fixture/test roads**: the ten moved rows and the two talkers change from
  `builder = "cargo"` to the cmake road; `test_qemu_baremetal_capi_e2e` and the
  dep-chain witnesses (Rust talkers only) need re-pointing.

## D7 — NuttX (decided 2026-11-12)

Maintainer ruling 2026-11-12: "Accept" on the three recommendations below
(Q-N1, Q-N2, Q-N3), with one addition for the guest road: the app names its
project in its own `Make.defs` (`NROS_PROJECTS += …`) and declares its
entities. The matching issue 2236 rulings (Q10, Q16) are folded into
[RFC-0126](0126-nuttx-builds-on-nuttx-infrastructure.md) and
[phase-550](../roadmap/phase-550-nuttx-native-build-infrastructure.md).

NuttX already has three roads (RFC-0126): the CMake project leaf, the
workspace (`nros build`), and the `apps/external` make app. How they fit "one
C/C++ build method" is open; what is known:

- Today the leaf and workspace roads ARE this RFC's road: a C/C++ package is
  a nano-ros CMake package, and the image links against NuttX's exported make
  tree (the per-arch `make export` snapshot, `nros_nuttx_include_root` in
  `cmake/platform/nano-ros-nuttx.cmake`). That is the road every NuttX C/C++
  fixture row takes today, on qemu-armv7a and rv-virt (not re-measured for
  this RFC).
- NuttX's own upstream CMake flow (`nuttx_add_application`) is not used.
  Whether it builds and boots qemu-armv7a at our pins is unmeasured.
- The `apps/external` make app is NuttX's own app framework: a user who keeps
  an existing NuttX apps tree adds nano-ros as one more app, with NuttX's
  Makefile and Kconfig, not a ROS package.

**Q-N1. Does a NuttX C/C++ package build through the same ROS-CMake road, and
on what?** Decided (recommendation accepted): yes, the same package unchanged, built by the
nano-ros CMake road against the exported make tree, as the leaf and workspace
roads do today. NuttX's upstream CMake flow becomes an option only after it
is measured booting both boards and a lane builds it (RFC-0126 D6). The
user's package does not change whichever kernel build sits underneath, so
this can be decided later without moving any user file.

**Q-N2. Where does the `apps/` make app stand?** Decided (recommendation
accepted): a GUEST
road, not a peer. It is how nano-ros enters someone else's NuttX apps tree,
written the NuttX way, and it is documented as that. The book's one C/C++
build method stays the ROS-CMake package, and the make app is not a second
way to build a nano-ros package. This keeps RFC-0126's
make-road fixes (D1 to D3) in scope as guest-road fixes. The guest app names its project in its own `Make.defs`
(`NROS_PROJECTS += …`) and declares its entities (ruled 2026-11-12).

**Q-N3. The NuttX CMake shell (issue 2236 Q16).** Decided: Q-N1 is answered as
recommended, so deleting the unreachable shell does not conflict with this RFC:
the ROS-CMake road on NuttX is the nano-ros CMake road, not NuttX's.

## D6 — Work, in order

1. **libc and heap ownership** (first). A cargo feature on the bare-metal
   platform crates (`nros-baremetal-common`, `nros-platform-mps2-an385`) that
   stops them exporting the C library symbols and the C allocator when a C
   libc owns them; the Rust allocator and newlib's `_sbrk` get one heap
   region. Acceptance: the probe's component prints a formatted
   `Hello World: N` and `RCLC_LOG_INFO` expands its arguments.
2. **C++ static constructors** run before `main` on the C-rooted image.
3. **The board's C-ABI start** and the Corrosion staticlib that carries the
   board, cortex-m-rt and the PAC, with the three link-script directories
   handed to cmake.
4. **`nros-entry-lower` family row** for the bare-metal board, runner
   `nros_board_rtos_run_components_in`, so the C/C++ entry pack renders for it.
5. **The road:** `nros build` routes a C/C++ image on this board to cmake, and
   `nros new --lang c|cpp` stops refusing it.
6. **Migration:** the two bare-metal C/C++ talker witnesses as components;
   later the ten cargo-crate workspaces.
7. **Measure before Stable:** dep-chain, the stack floor, `mem-report` on a
   migrated image.

## Versus the cargo-entry design (not built)

| | CMake-rooted (this RFC) | cargo-entry draft |
| --- | --- | --- |
| user writes | ROS 2 CMake package | ROS 2 CMake package, but its CMakeLists is IGNORED (sources by convention) |
| methods across boards | one | two (cmake on RTOS, cargo on bare metal) |
| new machinery | board C-ABI start, Corrosion staticlib link scripts, libc/heap owner | entry fork, generated build.rs re-implementing a C build |
| measured | boots + publishes | not built |
