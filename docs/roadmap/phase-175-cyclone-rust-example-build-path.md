# Phase 175 - Cyclone DDS build path for pure-Rust examples

**Goal.** Give the pure-cargo Rust examples a way to actually link the
Cyclone DDS RMW backend. Phase 169 retired the pure-Rust dust-dds
backend and made Cyclone the canonical DDS RMW; the examples were
migrated from the deleted `nros-rmw-dds` crate onto
`nros-rmw-cyclonedds-sys`, but Cyclone cannot be linked by a plain
`cargo build` on any target. This phase designs and lands the build
path(s) that make `--features rmw-cyclonedds` link end-to-end.

**Status.** **175.A build path landed (2026-05-21)** for the native
Rust talker + listener. **175.B FreeRTOS compile/link path is partially
landed (2026-05-22)** for the FreeRTOS C/C++ talker fixtures and the
Rust talker fixture: the pinned Cyclone tree builds as a Cortex-M3
static `libddsc.a`, installs into `build/cyclonedds-freertos-install`,
and FreeRTOS C/C++/Rust talkers link against `NANO_ROS_RMW=cyclonedds`.
FreeRTOS Rust CycloneDDS talker boot/publish and local pub/sub data
exchange are verified under QEMU. **ThreadX ddsrt compile surface is
experimental (2026-05-23):** the nano-ros Cyclone fork has a new
NetX Duo-backed ThreadX port, and the RISC-V64 `ddsc` static-library
probe builds. **ThreadX C/C++/Rust talker fixture link checks now pass
(2026-05-24)** with `NANO_ROS_RMW=cyclonedds`; the Rust fixture uses the
same CMake/Corrosion staticlib `app_main()` path as FreeRTOS.

The CMake/Corrosion glue at `examples/native/rust/{talker,listener}/CMakeLists.txt`
now links the Cyclone backend into a pure-Rust example end-to-end:

- `find_package(CycloneDDS)` + `add_subdirectory(packages/dds/nros-rmw-cyclonedds)`
  builds the C++ backend (defines `nros_rmw_cyclonedds_register`);
- `nros_rmw_cyclonedds_generate_from_msg(... std_msgs msg/Int32.msg)`
  (host `idlc` at `build/cyclonedds/bin/idlc` + `scripts/cyclonedds/`)
  emits the C `dds_topic_descriptor_t` + a static-init register TU,
  compiled into `<bin>_cyc_types` STATIC and **whole-archived** into the
  bin so `find_descriptor("std_msgs::msg::dds_::Int32_")` resolves;
- `corrosion_import_crate(... NO_DEFAULT_FEATURES FEATURES rmw-cyclonedds)`
  + `corrosion_link_libraries(<bin> nros_rmw_cyclonedds)`;
- ddsc is linked via `corrosion_add_target_local_rustflags` link-args
  (`$<TARGET_FILE:CycloneDDS::ddsc>` + rpath) — corrosion mangles the
  namespaced imported SHARED target into a bogus `-lCycloneDDS::ddsc`,
  so the resolved `.so` path is passed directly. whole-archive on the
  descriptor lib is also done this way (corrosion_link_libraries rejects
  raw `-Wl,` flags).

**Verified:** both bins build + link + boot; talker creates the writer,
listener the reader, and the two **fully match at the RTPS level**
(`writer_add_connection(wr …103 prd …104)`) over loopback — same topic
`rt/chatter`, type `std_msgs::msg::dds_::Int32_`, compatible QoS. The
backend's in-process `nros_rmw_cyclonedds_data_roundtrip` test passes.

**Runtime fix landed 2026-05-21:** two separate native Cyclone
processes now exchange user data. Root cause: Cyclone is a poll-only
backend (`set_wake_callback = NULL`) and the executor therefore passes
the full per-iteration timeout into `session_drive_io(timeout_ms)` to
pace wall-clock timer accumulation. The Zephyr path already honored
this via `k_msleep`, but hosted/POSIX returned immediately, so
`spin_blocking` busy-spun and the 1 Hz timer did not fire at the
expected wall-clock cadence. POSIX now sleeps with
`std::this_thread::sleep_for(timeout_ms)`. Verified talker logs
`Published: 0/1` and listener logs `Received: 0/1` on loopback. The
`rmw-cyclonedds` feature is still NOT wired into any pure-cargo fixture
matrix; use this CMake/Corrosion path.

**Priority.** P2. Does not block `just ci` / `just test-all` (DDS is a
non-default example feature). Blocks DDS coverage for Rust examples.

**Depends on.**

- Phase 117: `nros-rmw-cyclonedds` C++ CMake backend + `NANO_ROS_RMW=cyclonedds`.
- Phase 11W: Cyclone DDS on Zephyr native_sim (the only working
  embedded Cyclone path; still in progress on `main`).
- Phase 169: dust-dds retirement; Cyclone as canonical DDS RMW.

## Background — why pure-cargo can't link Cyclone

`nros-rmw-cyclonedds-sys` is an `rlib`-only shim. Its `register()`
declares `extern "C" fn nros_rmw_cyclonedds_register()` and calls it;
that symbol is **defined in C++** in `packages/dds/nros-rmw-cyclonedds/`
(`src/vtable.cpp`), compiled by a CMake project that
`find_package(CycloneDDS)` and links `CycloneDDS::ddsc`. A plain
`cargo build --features rmw-cyclonedds` of an example only pulls in the
Rust shim — the C++ backend is never compiled or linked — so the link
fails with:

```
rust-lld: error: undefined symbol: nros_rmw_cyclonedds_register
```

This is confirmed on **native/posix as well as embedded** — it is not a
cross-compilation quirk; it is structural. dust-dds had no such problem
because it was pure Rust and linked entirely inside cargo.

The backend links today only when the build is **CMake-driven**:

- Native C / C++ examples: root `CMakeLists.txt` `NANO_ROS_RMW=cyclonedds`
  branch `add_subdirectory(packages/dds/nros-rmw-cyclonedds)` and links
  the static lib into `NanoRos` with `--whole-archive`.
- Zephyr Rust/C/C++: `zephyr/CMakeLists.txt` compiles the Cyclone DDS
  sources + the `nros-rmw-cyclonedds` C++ glue directly into the app
  library, against Zephyr's POSIX layer (Phase 11W).

## Work items

### 175.A — Native Rust example Cyclone CMake/Corrosion path

**Achievable build-glue.** Cyclone builds for Linux (`just cyclonedds
setup` installs it under `build/install/`). Give the native Rust
examples a CMakeLists.txt that:

- [x] Pulls in the Cyclone C++ backend (`add_subdirectory(packages/dds/nros-rmw-cyclonedds)`
  with `CMAKE_PREFIX_PATH` → the Cyclone install),
- [x] Imports the Rust example bin via `corrosion_import_crate(... FEATURES rmw-cyclonedds NO_DEFAULT_FEATURES)`.
- [x] Links the backend into the bin with
  `corrosion_link_libraries(<bin> -Wl,--whole-archive nros_rmw_cyclonedds -Wl,--no-whole-archive CycloneDDS::ddsc)`
  so `nros_rmw_cyclonedds_register` resolves and its
  `.linkm2_RMW_INIT_ENTRIES` section entry survives dead-strip.
- [x] Adds native Rust `talker` / `listener` CMake entry points.
- [x] Drives native Rust Cyclone fixtures through CMake/Corrosion
  instead of the pure-Cargo fixture loop.

**Files / recipe work:**

- [x] Add `examples/native/rust/talker/CMakeLists.txt`.
- [x] Add `examples/native/rust/listener/CMakeLists.txt`.
- [x] Add a `just native build-fixtures` arm that drives the Cyclone
  variant via CMake instead of `cargo build`.

**Acceptance criteria:**

- [x] Native Rust talker builds and links with Cyclone DDS.
- [x] Native Rust listener builds and links with Cyclone DDS.
- [x] Native Rust talker boots and creates the Cyclone writer.
- [x] Native Rust listener boots and creates the Cyclone reader.
- [x] Native Rust talker/listener exchange user data over loopback.
- [x] Native Rust Cyclone examples are interop-tested against the C/C++
  Cyclone examples.

### 175.B — FreeRTOS / ThreadX Cyclone (ddsrt RTOS port)

**Not build-glue — a port.** Cyclone DDS abstracts its OS dependencies
(sockets, threads, time, sync) behind `ddsrt`. FreeRTOS can use
Cyclone's upstream FreeRTOS/lwIP port with board compatibility shims;
ThreadX still has no upstream `ddsrt` port. Standing up a new ThreadX
port is a weeks-scale effort comparable to the Zephyr Cyclone bring-up
(Phase 11W, still ongoing on `main`), and likely a research-grade
undertaking on bare-metal thumbv7m / riscv64 where there is no hosted
socket stack.

**Work items:**

- [x] Inventory the pinned Cyclone `ddsrt` RTOS surface.
- [x] Scope the embedded networking split.
  - [x] FreeRTOS: upstream `ddsrt` has `WITH_FREERTOS` plus `WITH_LWIP`
    hooks and should use lwIP sockets first.
  - [x] ThreadX: no upstream `ddsrt` files exist; the port must be new and
    backed by NetX Duo sockets.
- [x] Add a FreeRTOS Cyclone cross-build probe using
  `WITH_FREERTOS=ON`, `WITH_LWIP=ON`, the MPS2 toolchain, and the
  checked-out FreeRTOS/lwIP trees.
- [x] Resolve the first FreeRTOS probe blocker: Cyclone's lwIP
  `ddsrt_getifaddrs` path expects `netif_list`, but the MPS2
  `lwipopts.h` previously set `LWIP_SINGLE_NETIF=1`, which hides
  `netif_list` and `struct netif::next`.
- [x] Design the ThreadX `ddsrt` port API mapping.
- [x] Add an experimental ThreadX `ddsrt` port using ThreadX kernel
  primitives and NetX Duo BSD sockets.
- [x] Add a ThreadX Cyclone cross-build probe using `WITH_THREADX=ON`,
  the RISC-V64 ThreadX toolchain, and the checked-out ThreadX/NetX Duo
  trees.
- [x] Implement the first embedded `ddsrt` link surface by using
  Cyclone's upstream FreeRTOS/lwIP ddsrt port plus nano-ros MPS2
  compatibility shims for TLS, wall-clock, hostname, FreeRTOS trace
  API exposure, and bare-metal linker TLS placement.
- [x] Add embedded Cyclone link wiring for the FreeRTOS C/C++ talker
  fixtures.
- [x] Add embedded Cyclone link wiring for the FreeRTOS Rust talker
  fixture using a CMake/Corrosion staticlib entry point.
- [x] Re-enable the relevant `rmw-cyclonedds` fixture matrix cell for
  the first FreeRTOS Rust talker build.

**Files / recipe work:**

- [x] FreeRTOS probe/link surface that consumes Cyclone's upstream
  FreeRTOS/lwIP `ddsrt` port.
- [x] ThreadX `ddsrt` port.
- [x] ThreadX RISC-V64 `ddsc` static-library probe.
- [x] Embedded Cyclone link wiring for the first FreeRTOS C/C++/Rust
  talker fixtures.
- [x] FreeRTOS Rust Cyclone boot/run recipe or E2E fixture.
- [x] FreeRTOS Rust Cyclone data-exchange fixture.

**Acceptance criteria:**

- [x] Embedded networking story is scoped enough to estimate.
- [x] At least one RTOS can build a Rust Cyclone DDS example.
- [x] At least one RTOS can boot a Rust Cyclone DDS example.
- [x] At least one RTOS Rust Cyclone DDS example exchanges user data.
- [x] Fixture recipes build RTOS Rust Cyclone cells without pure-Cargo
  link failures.
- [x] FreeRTOS C talker links with `NANO_ROS_RMW=cyclonedds`.
- [x] FreeRTOS C++ talker links with `NANO_ROS_RMW=cyclonedds`.
- [x] FreeRTOS Rust talker links with `NANO_ROS_RMW=cyclonedds`.
- [x] ThreadX RISC-V64 Cyclone `ddsc` builds against ThreadX + NetX Duo.
- [x] ThreadX C/C++/Rust talker fixtures link with
  `NANO_ROS_RMW=cyclonedds`.

**Verified 2026-05-23:**

```bash
cmake --build examples/qemu-arm-freertos/rust/talker/build-cyclonedds --target freertos_rust_talker_cyclonedds
timeout 180s cargo test -p nros-tests --test freertos_qemu test_freertos_rust_talker_cyclonedds_boot -- --nocapture
timeout 180s cargo test -p nros-tests --test freertos_qemu test_freertos_rust_cyclonedds_local_pubsub_e2e -- --nocapture
```

The focused E2E boots `freertos_rust_talker_cyclonedds` under QEMU,
opens the CycloneDDS-backed executor, declares the `/chatter`
publisher, and reaches `Published: 0`. The local pub/sub E2E also
declares a CycloneDDS subscriber in the same FreeRTOS process and
verifies `Loopback received: 0`, covering writer/reader matching,
sample delivery, CDR decode, and executor dispatch on the RTOS path.

**Verified 2026-05-24:**

```bash
just cyclonedds threadx-cross-probe
cmake --build examples/qemu-riscv64-threadx/c/talker/build-cyclonedds --parallel 4
cmake --build examples/qemu-riscv64-threadx/cpp/talker/build-cyclonedds --parallel 4
cmake --build examples/qemu-riscv64-threadx/rust/talker/build-cyclonedds --parallel 4
```

The ThreadX Rust fixture is CMake-owned, imports
`qemu-riscv64-threadx-talker` as a staticlib with
`FEATURES rmw-cyclonedds`, generates `std_msgs/Int32` CycloneDDS
descriptors with `idlc`, exports Rust `app_main()`, and links the
RISC-V64 ThreadX executable through `nros_platform_link_app`.

**Probe:**

```bash
just cyclonedds ddsrt-port-inventory
just cyclonedds freertos-cross-probe
just cyclonedds threadx-cross-probe
```

The inventory probe is read-only and verifies the upstream FreeRTOS/lwIP
ddsrt files exist while recording that upstream Cyclone has no
ThreadX/NetX Duo port; nano-ros now carries the experimental port in
this tree.

The FreeRTOS cross-build probe configures the pinned Cyclone tree with
`WITH_FREERTOS=ON`, `WITH_LWIP=ON`, `BUILD_SHARED_LIBS=OFF`, the
MPS2 ARM toolchain, and the checked-out FreeRTOS/lwIP headers. It uses
`CMAKE_TRY_COMPILE_TARGET_TYPE=STATIC_LIBRARY` so CMake feature checks
do not require hosted executables and disables Cyclone's optional
FreeRTOS rusage path because the MPS2 config does not enable the trace
facility/runtime-stats APIs it needs. The current result is:

- configure passes;
- `ddsc` compiles and installs as a static ARM Cortex-M3 library and
  reaches the FreeRTOS/lwIP `ddsrt` sources;
- the first port/config blocker was fixed by setting
  `LWIP_SINGLE_NETIF=0` in the MPS2 board config, keeping lwIP's
  linked-list netif model visible to
  `src/ddsrt/src/ifaddrs/lwip/ifaddrs.c`;
- the probe carries a temporary `-D__int64_t_defined=1` ARM-newlib guard
  so Cyclone's `PRIu64`/`PRIu32` format macros are visible and the build
  can get through the current compile probe.

The FreeRTOS C/C++ talker link path additionally requires:

- `cmake/board/nano-ros-board-mps2-an385-freertos.cmake` exposes
  `configUSE_TRACE_FACILITY=1` to FreeRTOS so Cyclone's `ddsrt_gettid`
  can resolve `vTaskGetInfo`;
- `packages/core/nros-platform-freertos/src/cyclonedds_compat.c`
  provides the bare-metal symbols Cyclone expects from hosted C/ARM
  runtimes (`__aeabi_read_tp`, `clock_gettime`, `gethostname`);
- the MPS2 linker script keeps Cyclone's TLS sections adjacent and
  defines `__tls_base`;
- the Cyclone wrapper avoids 64-bit libatomic dependencies on FreeRTOS
  for service request sequence counters.

The FreeRTOS Rust talker uses a CMake/Corrosion staticlib shape rather
than pure cargo:

- `examples/qemu-arm-freertos/rust/talker/src/lib.rs` owns the shared
  app body;
- the existing `src/main.rs::_start()` still drives the pure-cargo
  zenoh path through the Rust board crate;
- the Cyclone path imports the crate as a `staticlib` and exports
  `app_main()` so the checked-in C startup creates the FreeRTOS task,
  initializes networking, and then enters Rust;
- `just freertos build-fixtures` regenerates Rust message crates before
  cargo builds and adds the `freertos/rust/talker` Cyclone CMake cell
  when `build/cyclonedds-freertos-install/lib/libddsc.a` exists.

**ThreadX verified 2026-05-23:**

```bash
just cyclonedds threadx-cross-probe

cmake -S examples/qemu-riscv64-threadx/c/talker \
  -B examples/qemu-riscv64-threadx/c/talker/build-cyclonedds \
  -DNROS_RMW=cyclonedds \
  -DCMAKE_PREFIX_PATH="$PWD/build/cyclonedds-threadx-rv64-install" \
  -DCMAKE_TOOLCHAIN_FILE="$PWD/cmake/toolchain/riscv64-threadx.cmake" \
  -DTHREADX_DIR="$PWD/third-party/threadx/kernel" \
  -DNETX_DIR="$PWD/third-party/threadx/netxduo" \
  -DTHREADX_CONFIG_DIR="$PWD/packages/boards/nros-board-threadx-qemu-riscv64/config" \
  -DNETX_CONFIG_DIR="$PWD/packages/boards/nros-board-threadx-qemu-riscv64/config" \
  -DCMAKE_BUILD_TYPE=Release \
  -DIDLC_EXECUTABLE="$PWD/build/install/bin/idlc" \
  -DNROS_RMW_CYCLONEDDS_MSG_TO_IDL="$PWD/scripts/cyclonedds/msg_to_cyclone_idl.py"
cmake --build examples/qemu-riscv64-threadx/c/talker/build-cyclonedds --parallel 4

cmake -S examples/qemu-riscv64-threadx/cpp/talker \
  -B examples/qemu-riscv64-threadx/cpp/talker/build-cyclonedds \
  -DNROS_RMW=cyclonedds \
  -DCMAKE_PREFIX_PATH="$PWD/build/cyclonedds-threadx-rv64-install" \
  -DCMAKE_TOOLCHAIN_FILE="$PWD/cmake/toolchain/riscv64-threadx.cmake" \
  -DTHREADX_DIR="$PWD/third-party/threadx/kernel" \
  -DNETX_DIR="$PWD/third-party/threadx/netxduo" \
  -DTHREADX_CONFIG_DIR="$PWD/packages/boards/nros-board-threadx-qemu-riscv64/config" \
  -DNETX_CONFIG_DIR="$PWD/packages/boards/nros-board-threadx-qemu-riscv64/config" \
  -DCMAKE_BUILD_TYPE=Release \
  -DIDLC_EXECUTABLE="$PWD/build/install/bin/idlc" \
  -DNROS_RMW_CYCLONEDDS_MSG_TO_IDL="$PWD/scripts/cyclonedds/msg_to_cyclone_idl.py"
cmake --build examples/qemu-riscv64-threadx/cpp/talker/build-cyclonedds --parallel 4
```

The ThreadX probe builds Cyclone DDS `ddsc` as a RISC-V64 static
library using `WITH_THREADX=ON`, the nano-ros
`riscv64-threadx.cmake` toolchain, ThreadX kernel headers, NetX Duo BSD
headers, and the QEMU RISC-V64 board config. The probe disables
Cyclone's `ENABLE_LTO` path so `rust-lld` can consume real ELF objects
instead of GCC LTO-slim archive members. The C/C++/Rust talker fixture
links also validate the ThreadX C++ compatibility headers, the
freestanding picolibc/POSIX weak stubs, the Generic-target Cyclone
archive ordering fix, and the Rust CMake/Corrosion `app_main()`
staticlib path. Runtime RTPS over QEMU is still open.

### ThreadX ddsrt port mapping

ThreadX has no upstream Cyclone `ddsrt` implementation. The nano-ros
ThreadX port is a new
`src/ddsrt/src/{sync,time,threads,sockets}/threadx` surface wired by
`WITH_THREADX=ON`, not a fork of the POSIX port. Current mapping:

- time: `tx_time_get()` plus the configured ThreadX tick rate for
  monotonic time, with an explicit wall-clock hook for APIs that need
  real time;
- sleep: `tx_thread_sleep()` with millisecond/tick rounding matching the
  FreeRTOS `pdMS_TO_TICKS` behavior;
- mutex/cond/semaphore: `TX_MUTEX`, `TX_SEMAPHORE`, and either
  `tx_semaphore_get` timeouts or a small condition-variable adapter;
- threads: `tx_thread_create()` with caller-provided stack storage or a
  nano-ros heap-backed allocation wrapper, plus deterministic priority
  mapping from normalized nano-ros priorities;
- sockets/ifaddrs: NetX Duo BSD sockets if enabled, otherwise direct
  NetX Duo UDP/TCP wrappers; interface enumeration must expose at least
  IPv4 address/netmask/broadcast for Cyclone participant discovery;
- TLS/errno/hostname: explicit shims matching the FreeRTOS compatibility
  layer, because bare-metal ThreadX toolchains do not provide hosted
  libc process/thread state.

## Notes

- The migrated examples keep `rmw-cyclonedds = ["dep:nros-rmw-cyclonedds-sys"]`
  in `Cargo.toml` so the manifest resolves and the intent is recorded.
  Pure-cargo fixture loops must stay zenoh-only; Cyclone fixture cells
  need a CMake/Corrosion path that can link the C++ backend.
- Native, FreeRTOS, and ThreadX now have CMake/Corrosion Cyclone Rust
  fixture cells. ThreadX is verified at build/link level for the talker;
  runtime RTPS over QEMU remains future work. NuttX and other
  pure-cargo RTOS fixture loops remain zenoh-only.
- Do NOT re-introduce a `for rmw in ... cyclonedds` / `... dds` arm into
  those pure-cargo loops without first landing 175.A.
