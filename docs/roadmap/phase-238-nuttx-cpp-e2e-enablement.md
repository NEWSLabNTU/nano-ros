# Phase 238 — NuttX C/C++ E2E enablement (bootable-ELF wiring)

**Goal.** Run the NuttX C and C++ example E2E tests (pub/sub, service, action) in
QEMU ARM virt, the same way the NuttX **Rust** examples already do. The compile
blocker that gated this is gone; what remains is producing a bootable ELF from the
C/C++ build.

**Status.** 238.A + 238.B + 238.C (C++ pub/sub pair, E2E observability, C path)
**DONE** 2026-06-12 — `nuttx_{cpp,c}_talker` + `nuttx_{cpp,c}_listener` boot as
bootable kernel ELFs in QEMU ARM virt, connect to a host zenoh router over
slirp, and exchange `/chatter` `std_msgs/msg/Int32`: talker prints
`Published: N`, listener prints `Waiting for messages` + `Received: N` with
matching values (C and C++). `rtos_e2e` `(Nuttx, {Cpp,C}, Pubsub)` now
satisfiable. Service/action (both languages) remain deferred (interpreter has
no callback bodies — see §238.B "Deferred"). Off the critical path (NuttX is a
secondary platform).

**Depends on.** `nros-board-nuttx-qemu-arm` (kernel staging + link), the NuttX
submodule (`nuttx-12.13.0-4`), `cmake/NanoRosNodeRegister.cmake`, the rtos_e2e
harness.

## Background / what was verified

The 6 NuttX C++ build tests in `nuttx_qemu.rs` are `#[ignore]`'d with reason
"CMake build blocked by upstream libc missing `_SC_HOST_NAME_MAX`". **That blocker
is resolved** in the current submodule:

- `_SC_HOST_NAME_MAX` is defined (`third-party/nuttx/nuttx/include/unistd.h:170`),
  `HOST_NAME_MAX 32` (`limits.h:329`).
- All 6 NuttX C++ examples now **compile clean** — each produces its component lib
  `libnuttx_cpp_<name>_<name>_component.a` with no error
  (`just nuttx build-fixtures` + a direct `cmake --build` both confirmed).

The z_open hang and the `tcp_update_timer`/`z_clock_t` issues are likewise long
resolved (Apr 2026, see the `project_nuttx_investigation` memory).

## The actual gap

The C/C++ NuttX examples are **component-lib only** — `NanoRosNodeRegister` emits a
`STATIC` `<pkg>_<name>_component` library and stops there (Phase 194.3c scoped the
C path as "build-coverage, no e2e"). There is **no executable / bootable-ELF
target** for C/C++ NuttX. The rtos_e2e cases
(`test_rtos_*_e2e::platform_*_Nuttx::lang_2_C` / `lang_3_Cpp`) and the
`build_nuttx_{c,cpp}_*` resolvers expect a bootable ELF at
`examples/qemu-arm-nuttx/<lang>/<name>/build-zenoh/nuttx_<lang>_<name>`, which the
build never produces → `require_prebuilt_binary` fails.

Contrast the **Rust** path (works): the example deps `nros-board-nuttx-qemu-arm`,
whose build.rs links the NuttX kernel staging, and `cargo build` emits a complete
bootable ELF at `target/armv7a-nuttx-eabihf/release/nuttx-rs-<name>`.

## Root cause (precise — found 2026-06-12)

The bootable-ELF mechanism **already exists and is complete**, but is **orphaned**
for the current example shape:

- `cmake/board/nano-ros-board-nuttx-qemu-arm.cmake::nros_board_link_app(target)`
  calls `nros_nuttx_build_example(...)` (in `nros-c/cmake/nros-nuttx.cmake`), which
  runs `cargo build` on `nros-nuttx-ffi` with `APP_MAIN_CPP` / `APP_FFI_LIBS_FILE`
  set → the NuttX kernel relink (the SSoT link in
  `nros-board-common::nuttx_ffi_build.rs`: `dramboot.ld` + `staging/` start-group +
  vectortab + libgcc, `--entry=__start -nostartfiles -nodefaultlibs`) → a `*_build`
  **ALL** target that copies the ELF to `build-zenoh/nuttx_<lang>_<name>`
  (`nros-nuttx.cmake:288`).
- **Nothing calls `nros_board_link_app` for the examples.** `nano_ros_node_register`
  only emits the `<pkg>_<name>_component` static lib + records component JSON;
  `nano_ros_deploy` only records deploy JSON; `nano_ros_entry` / `_nros_metadata_emit`
  don't call the board link either. So the `*_build` target is never created and the
  ELF is never produced.
- The C/C++ NuttX examples are the **Phase 212.L.9 "declarative Component pkg shape"**
  (their CMakeLists literally say "No add_executable"). That migration dropped the
  per-example bootable-ELF target; the **Rust** examples kept producing ELFs via
  cargo, so only the **C/C++** rtos_e2e cases lost their binaries — matching 194.3c
  ("C path is build-coverage, no e2e"). NuttX C/C++ E2E has been non-functional
  since 212.L.9.

The fix is **not** writing a new link — it is two pieces, both verified by
prototyping (2026-06-12):

**(1) Carrier** — `nano_ros_node_register` must (for `"nuttx" IN_LIST DEPLOY`,
non-Rust, when `COMMAND nros_platform_link_app`) create a *separate* carrier
`add_executable(${PROJECT_NAME} ${SOURCES})` (named after the package = the
fixture's binary name, so the ELF lands at `build-zenoh/${PROJECT_NAME}`; the
component lib stays as build-coverage), link the same iface libs + `NanoRos*`, and
call `nros_platform_link_app(${PROJECT_NAME})`. **Verified:** this creates the
`${PROJECT_NAME}_build` ALL target and the cargo `nros-nuttx-ffi` build runs.

**(2) Entry** — the cargo link then fails `undefined reference to 'app_main'`. The
212.L.9 examples are **declarative class shape**: `Talker.cpp` only does
`NROS_NODE_REGISTER(nuttx_cpp_talker::Talker, …)` — it has **no `app_main` /
`nros_app_main`**. (Rust's `nros::node!()` macro emits the entry; the C++
`NROS_NODE_REGISTER` does not.) The entry is generated by **`nros codegen-system`**
(→ `system_main` resolving the registered components — see `node_pkg.hpp:509` +
`NanoRosEntry.cmake`). So the carrier must also compile a generated entry source as
`APP_MAIN_CPP` (or `APP_EXTRA_SOURCES`) — i.e. drive `nros codegen-system` for the
single-node example to produce the `nros_app_main` that runs `Talker`, then feed it
into the carrier link.

Concretely: extend the carrier wiring (1) to invoke the Entry codegen for the
registered node(s) and pass the generated `system_main`/entry source into
`nros_nuttx_build_example` as the `MAIN_SOURCE`, with the class source(s) as extra
sources. Then `cmake --build` produces `build-zenoh/nuttx_<lang>_<name>` for both C
and C++, and the rtos_e2e Nuttx×{C,Cpp} cases boot. (Until both land, the carrier
must NOT be added unconditionally — its `_build` ALL target fails the fixture build
without the entry; gate the whole thing on the entry being generated.)

## Approaches

### A — cmake executable target (recommended; mirrors the Rust board crate)
Add a NuttX bootable-ELF target to the C/C++ example build: an `add_executable`
(or a custom link command) that links
- the example's `<pkg>_<name>_component` static lib,
- `nros-c` / `nros-cpp` (already corrosion-built),
- the NuttX kernel staging: `third-party/nuttx/nuttx/staging/libc.a` (+ the kernel
  objects the Rust board link pulls in),
- the NuttX linker script,

producing `build-zenoh/nuttx_<lang>_<name>`. The exact link line is the SSoT in the
board crate's `nuttx_platform_build` / `nros-board-common` — extract it into a
reusable cmake fragment (or a small `link-nuttx-elf.sh` the cmake invokes) so the
C/C++ link matches the Rust one byte-for-byte (same staging, same `.ld`, same
`arm-none-eabi-gcc` flags). Self-contained per example; no apps-tree.

### B — NuttX apps-tree integration (`stage-external-apps.sh`)
Restructure each C/C++ example as a NuttX external app (Make.defs + Kconfig + an
NSH-registered `main`), stage them with `stage-external-apps.sh`, and rebuild the
kernel (`build-nuttx.sh`) with the apps enabled — one kernel ELF carries all apps
as NSH commands. More idiomatic NuttX, but needs the examples reshaped as apps and
the rtos_e2e harness changed to "boot the shared kernel, run `<name>` via NSH"
instead of per-example boot. Larger test-harness change.

**Recommendation: A** — keeps the per-example bootable-ELF model the harness +
resolvers already assume, reuses the proven Rust kernel-staging link, and needs no
harness change.

## Work items (Approach A)

1. **Extract the kernel-ELF link recipe** from the Rust board path
   (`nros-board-common::nuttx_platform_build` + the example's effective rustc link
   args) into a reusable form: staging libs, kernel objects, linker script, flags.
2. **Add the executable target** to the C/C++ NuttX example cmake (via
   `NanoRosNodeRegister` NuttX branch or a sibling `nano_ros_nuttx_executable`):
   link component + nros-c/cpp + staging → `build-zenoh/nuttx_<lang>_<name>`.
3. **Wire it into the fixture build** (`just nuttx build-fixtures` / the cpp/c
   fixture leaves) so the ELF is produced alongside the component lib.
4. **Un-ignore** the 6 `nuttx_qemu.rs` C++ `*_builds` markers (now they resolve a
   real ELF) and confirm the rtos_e2e `Nuttx` × `{C,Cpp}` cases boot + pass in QEMU
   (pub/sub, service, action — expect ~90–140 s each, like the Rust/C cases).
5. **Regression:** the Rust NuttX E2E + the C/C++ component build stay green.

## Acceptance

- `test_rtos_{pubsub,service,action}_e2e::platform_*_Nuttx::lang_{2_C,3_Cpp}` boot
  the example in QEMU and exchange data over zenoh (matching the Rust cases).
- The 6 C++ `*_builds` `#[ignore]` markers removed; the ELF builds in CI.
- NuttX Rust E2E unaffected.

## Notes

- The link is the finicky part — a NuttX bootable ELF is sensitive to the exact
  staging libs / linker script / toolchain flags; mismatches manifest as a silent
  QEMU reboot loop (cf. the Phase 177.8.c rust cross-CGU miscompile). Build the
  link from the Rust path's known-good inputs, don't hand-roll.

## 238.A — what landed (C++ pub/sub pair), and what's deferred

**Approach taken.** Not Approach A's "new cmake link" — the bootable-ELF link
already exists (`nros_board_link_app` → `nros_nuttx_build_example` → cargo
`nros-nuttx-ffi`). The gap was purely that nothing *called* it for the
declarative C++ examples. 238.A wires the carrier + a C++ board adapter:

1. **`::nros::board::NuttxBoard`** (`packages/core/nros-cpp/include/nros/main.hpp`)
   — sibling to `NativeBoard`/`ZephyrBoard`, sharing the exact same
   `detail::EntryNodeRuntime` ops + arena. NuttX network is up at kernel boot
   (`nsh_initialize()` runs netinit before the app), so — like Zephyr — no
   explicit network wait is needed. Adds a `run(locator, lambda)` overload: the
   QEMU slirp guest must *dial* the host router (`tcp/10.0.2.2:<port>`), so the
   locator is baked rather than discovered. `emit_cpp.rs` maps the `"nuttx"`
   board key → `NuttxBoard` (with a unit test).
2. **Carrier** — `nano_ros_node_register` grew a NuttX branch (gated:
   `LANGUAGE CPP` + `nuttx IN_LIST DEPLOY` + `nros_platform_link_app` defined).
   It `configure_file`s `cmake/templates/nuttx_entry_main.cpp.in` into a
   single-node entry TU (`nros_app_main` → `NuttxBoard::run` → the one
   `__nros_component_<pkg>_register`, emitted `void app_main(void)` via
   `NROS_APP_MAIN_REGISTER_VOID`), creates `add_executable(<PROJECT_NAME> …)`
   (so the ELF lands at `build-zenoh/<PROJECT_NAME>`), and calls
   `nros_platform_link_app`. The Component static lib stays as build-coverage.
3. **`NROS_PKG_NAME` plumbing** — `nros_board_link_app` now ferries the
   carrier's `COMPILE_DEFINITIONS` into `nros_nuttx_build_example`'s
   `COMPILE_DEFS` (→ `APP_COMPILE_DEFS` → the cc-rs build), so the class TU
   (compiled as `APP_EXTRA_SOURCES`) sees `NROS_PKG_NAME` and its
   `NROS_NODE_REGISTER` macro emits the symbol the entry calls.

**Proof (manual).** Boot both ELFs in QEMU ARM virt (slirp,
`-netdev user,id=net0 -device virtio-net-device,netdev=net0`) against
`build/zenohd/zenohd -l tcp/0.0.0.0:7447`. zenohd `RUST_LOG=debug` shows:
two `whatami: client` transports; talker registers resource
`0/chatter/std_msgs/msg/Int32`; listener `Declare subscriber … 0/chatter/…`.
Both reach `init -> 0` / `register -> 0; spinning` (with the opt-in
`NROS_NUTTX_ENTRY_DEBUG` template flag). The pair connects + the pub/sub
topology routes — the proven `Variant::Pubsub` exchange.

**Un-ignored.** The six `nuttx_qemu.rs` `*_builds` markers (the carrier now
produces a real `build-zenoh/nuttx_cpp_<name>` ELF for every C++ example —
build coverage; service/action build + boot + *register* but do not execute).

## 238.B — pub/sub E2E observability (DONE 2026-06-12)

The shared `EntryNodeRuntime` (`nros-cpp/include/nros/main.hpp`) was silent: it
never drained a subscription unless a `Reads` callback-effect was recorded (the
declarative `Listener.cpp` records none), and printed nothing. Three changes
make the `Nuttx × Cpp × Pubsub` E2E observable + green:

1. **Auto-`reads`** — `do_create_entity` infers `reads` for any subscription
   declared *with* a callback (`d->callback_id` present), so the canonical
   declarative listener drains without a separate `record_callback_effect`.
2. **Readiness banner** — `spin()` prints `"Waiting for messages"` once when the
   topology has a draining subscription (publisher-only entries print nothing).
3. **Per-sample lines** — the drain prints `"Received: <v>"` (decoding the
   synthesized `std_msgs/Int32`), and `fire_publisher` prints `"Published: <v>"`
   — symmetric to the native imperative examples + the strings rtos_e2e greps.

**Proof.** Boot the pair (45 s) against host `zenohd`: talker prints
`Published: 0..43`, listener prints `Waiting for messages` then
`Received: 0,1,2,3…` — **received values equal published values** (the decode +
routing are correct). The earlier `Received: 0` was a cold-boot overlap timing
fluke, not a data-path bug. `rtos_e2e` `(Nuttx, Cpp, Pubsub)` (rtos_e2e.rs:370)
now satisfies its `"Waiting for messages"` + `"Received"` criteria once the
fixtures rebuild with these prints. The prints are unconditional in the shared
runtime (every Entry consumer) — they match the native examples' demo output;
native phase235 greps its *external* observer, so it is unaffected.

## 238.C — C path bootable ELF (DONE 2026-06-12)

The C examples (`examples/qemu-arm-nuttx/c/*`) are component-lib only, and the C
entry adapter (`nros-c/c-stubs/main_board.c`) is a no-op sleep-spin — no live
pub/sub even on native. Rather than port the ~400-line `EntryNodeRuntime`
interpreter to C, the C node is driven by the **proven C++ runtime** (the
C/C++ `NodeContext` / ops / descriptor structs are ABI-identical — see
`nros-c/include/nros/node_pkg.h` vs `nros-cpp/.../node_pkg.hpp`). Three changes:

1. **Mixed C/C++ cargo build** (`nros-board-common/src/nuttx_ffi_build.rs`).
   cc-rs compiles one `cc::Build` with a single language; a `.c` node under a
   `.cpp` entry would be forced to C++, mangling the C node's C-linkage
   `__nros_component_<pkg>_register`. Refactored to compile `.c` sources in a
   separate C `cc::Build` (`app_c` archive) and `.cpp/.cc/.cxx` in a C++ one
   (`app_cpp`); a shared `configure` closure applies the include/define/flag set
   to both. Each source keeps its native linkage; both archives link into the
   kernel ELF.
2. **Carrier accepts `LANGUAGE C`** (`NanoRosNodeRegister.cmake`). The generated
   entry stays C++ (it drives the header-only C++ `EntryNodeRuntime`); the C
   node is added as an extra source (compiled as C by (1)). Its C-linkage
   register symbol matches the entry's `extern "C"` decl.
3. **C-style Publishes binding** (`nros-cpp/.../main.hpp`,
   `do_record_effect`). The C declarative API records
   `record_callback_effect(timer, Publishes, pub)` — the "callback" IS the timer
   entity (no separate declared callback). `find_timer_for_callback` only
   matched a timer bound to a callback, so the C publisher never got a period.
   Added a fallback: if no timer-by-callback, treat `callback_id` as a Timer
   entity id. (C++ examples bind a named callback, so they match the primary
   path unchanged.)

**Proof.** `nuttx_c_talker` + `nuttx_c_listener` boot, exchange `/chatter`
`std_msgs/msg/Int32`: talker `Published: 0..36`, listener `Waiting for messages`
+ `Received: 0..36` (matching values). Cross-tested both directions
(C↔C++) — the C node publishes to a C++ subscriber and receives from a C++
publisher. `rtos_e2e` `(Nuttx, C, Pubsub)` (rtos_e2e.rs:357) now satisfiable.
NB: a stale cargo build (the `nuttx_ffi_build` change not yet recompiled into
the FFI build.rs on the first pass) silently produced a non-receiving listener;
a clean rebuild fixed it — `just nuttx build-fixtures` builds from scratch.

### Deferred (follow-ups)

- **Service / action (C and C++).** The `EntryNodeRuntime` interpreter only synthesizes
  timer-driven `std_msgs/Int32` pub/sub; services/actions are *recorded* (no
  hard error) but never executed. True service/action E2E needs the imperative
  entry shape (hand-written `nros_app_main` with real logic, like the native
  C/C++ examples), not the declarative carrier. (The C path uses the same C++
  interpreter via the mixed build, so it inherits the same limit.)
