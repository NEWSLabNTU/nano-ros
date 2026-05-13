# Phase 123 — Build distribution + C++ API revision

**Goal.** Reduce time-to-first-publish for an rclcpp engineer trying
nano-ros. Two threads: how the library reaches the user (source vs
SDK) and how the C++ API looks once they have it.

**Status.** In flight (branch `phase-123-build-and-api-revision`).

**Priority.** P1 — gating the migration-guide chapter.

**Depends on.** Phase 122 (closed). Builds on the install layout
introduced in 119.3 + the two-layer API surface frozen in 122.

## Why now

Walk-through (see session log 2026-05-13) of "rclcpp engineer
tries nano-ros":

- `cmake -S . -B build && cmake --install` works end-to-end on
  POSIX, but ships ~284 MB of artefacts (all variants) and
  requires a Rust nightly toolchain on the user machine.
- A new project needs `find_package(NanoRos)` + two
  `nros_generate_interfaces` calls + `target_link_libraries` —
  acceptable.
- The runtime C++ API has 10+ friction points vs rclcpp: silent
  error logging, mandatory `NROS_APP_MAIN_REGISTER_POSIX()`,
  manual `nros::init(locator, domain_id)`, out-param
  `create_node` / `create_publisher`, raw `void*` timer
  callbacks, hand-rolled `while (ok()) spin_once(100)` loop, no
  QoS in `create_publisher`.

These are answerable. The phase splits into two work streams.

## Stream A — Build distribution (source vs SDK)

### Current state

* **Source build only.** The user runs CMake against the repo and
  installs into a prefix. Build needs a Rust nightly toolchain
  (the rlibs that the C/C++ static libraries wrap) plus a host C
  toolchain.
* **Install size at default config.** ~284 MB total across
  zenoh + xrce + dds + cyclonedds variants, both C and C++ APIs,
  several embedded variants (`*_threadx_linux`,
  `*_freertos_armcm3`). Per-variant `libnros_*_*.a` ranges
  22–28 MB.
* **Cross-targeting.** Already wired for ARM Cortex-M
  (mps2-an385), ARM Cortex-A (NuttX QEMU), RISC-V (ThreadX
  rv64), Xtensa ESP32, x86_64 ThreadX. Each lives behind a
  `just <module> install` recipe — driven from the source tree.

### Open questions

1. **SDK package or source-only?**
   - SDK: prebuilt `nano-ros-sdk-<api>-<rmw>-<target>.tar.zst`
     archives published on GitHub releases. User downloads the
     archive matching their host + target, `find_package` picks
     up the unpacked layout. No Rust toolchain on the user box.
   - Source: present workflow. User installs Rust nightly,
     `git clone`, `cmake --install`.
2. **Target / arch matrix the SDK must cover.**
   - Host (where binaries link): `x86_64-linux-gnu`,
     `aarch64-linux-gnu`, `x86_64-apple-darwin`,
     `aarch64-apple-darwin`. Possibly `x86_64-windows-msvc`
     later.
   - Embedded targets (where binaries run): `thumbv7m-none-eabi`,
     `thumbv7em-none-eabihf`, `riscv32imc-unknown-none-elf`,
     `riscv64imac-unknown-none-elf`, `xtensa-esp32-none-elf`,
     `aarch64-unknown-nuttx`, `armv7a-none-eabihf`.
   - Each archive = (host, target, rmw, api). Cartesian product
     is large; pick the small useful subset.
3. **What goes in an SDK archive?**
   - `include/` headers (cbindgen + Doxyfile output included for
     offline reference).
   - `lib/libnros_{c,cpp}_<rmw>[_<plat>].a` — the static
     archives the user links.
   - `lib/cmake/NanoRos/` — `NanoRosConfig.cmake` +
     `NanoRos*Targets.cmake` that point at the prebuilt
     archives (no source paths).
   - `bin/nros-codegen` — Rust-compiled codegen tool for the
     host. Architecture-specific binary.
   - `share/nano-ros/interfaces/` — bundled
     `package.xml` + `.msg` sources for `std_msgs`,
     `builtin_interfaces`, `geometry_msgs`,
     `action_msgs`, `example_interfaces` so the user can
     `nros_generate_interfaces(std_msgs ...)` without a
     separate ROS install. Already shipped today.
4. **Versioning + reproducibility.**
   - SDK archives tagged with the upstream git SHA + the
     Rust toolchain channel. `nros --sdk-version` prints them.
   - Reproducible builds via fixed nightly + locked Cargo.lock
     (already locked).
5. **Source path still supported.**
   - Contributors and embedded-target users that need a custom
     RTOS port keep the source build. SDK is the "I want to
     try nano-ros from rclcpp" path.

### Work items

- **123.A.1 — Audit binary content + redistribution.** Confirm
  `libnros_{c,cpp}_*.a` are self-contained (no
  source-path-baked-in symbols, no rustc rmeta leaking) and
  that the install layout is path-independent. Spot-check
  with `objdump --info`. Document any rust-runtime symbols
  the user must already have (libgcc / libstdc++).
- **123.A.2 — Pick the host / target shipping matrix.**
  Decision doc: which (host, rmw, api) the SDK ships first,
  which is on-demand, which stays source-only.
- **123.A.3 — Build CI matrix.** GitHub Actions job builds the
  per-(host, target) archives from a tagged commit, signs them,
  publishes via GitHub release. Reusable across all rows of
  the matrix.
- **123.A.4 — `nros-sdk` install/unpack helper.** Small CLI
  (Rust or bash) that:
    - downloads the right archive for the host + target,
    - unpacks to `~/.local/nano-ros-sdk/<rmw>/`,
    - prints the `CMAKE_PREFIX_PATH` to add.
- **123.A.5 — Doc.** Update
  `book/src/getting-started/installation.md` with the SDK
  path as the recommended entry; demote source-build to
  "for contributors / RTOS porters".

### Open design points

- **Static vs dynamic libraries.** Currently static-only
  (each binary embeds zenoh-pico, no shared `.so`). Static
  is the right default for embedded; for posix-only users a
  `.so` build would cut binary size dramatically. Decide
  whether to ship both.
- **Archive format.** `tar.zst` for size, `tar.gz` for
  compatibility. Probably ship both; CLI picks the right one.

## Stream B — C++ API revision

### Friction inventory (rclcpp → nros)

| # | Friction | Concrete cost to user | Proposed fix |
|---|---|---|---|
| B.1 | `NROS_TRY_LOG` silent unless user `#define`s it | First-run silent failures | Default the macro to `fprintf(stderr, ...)`. Opt-OUT via `#define NROS_TRY_LOG(...) (void)0` for embedded. |
| B.2 | `NROS_APP_MAIN_REGISTER_POSIX()` boilerplate | Bottom-of-file magic; "why?" | Provide a default `main()` in a header-only optional include; user opts in by including `<nros/posix_main.hpp>` and defining `nros_app_main`. Today's macro stays for embedded `_start` injection. |
| B.3 | Hardcoded `tcp/127.0.0.1:7447` fallback in `nros::init` | Production code reads `getenv` itself | Make `nros::init()` (no args) read `$NROS_LOCATOR` / `$ROS_DOMAIN_ID` itself. Match rclcpp's "init from environment" mental model. |
| B.4 | Out-param `create_*` style | `nros::Node n; nros::create_node(n, "name");` vs `auto n = rclcpp::Node::make_shared("name")` | Keep out-param for zero-alloc. Add a `nros::Node::make("name")` value-return convenience that constructs into an `aligned_storage` slot. Same for `Publisher<M>::make(node, "/topic")`. |
| B.5 | Manual spin loop | `while (nros::ok()) nros::spin_once(100);` | Add `nros::spin(node, options)` blocking entry (mirror of `rclcpp::spin`). Internally drives the existing loop. |
| B.6 | Timer takes `void*` + C fn pointer | Hand-roll a context struct, cast | Add a `node.create_timer(period, [&]() { ... })` overload that captures into a typed callback box; falls back to the C-pointer form on `NROS_CPP_NO_STD` builds. |
| B.7 | No QoS argument in `create_publisher` | Defaults baked in; user has to set after | Add overload `create_publisher(pub, "/topic", QoS::reliable().keep_last(10))`. Already supported by FFI — just surface it. |
| B.8 | Generated header naming `std_msgs.hpp` (flat) | Inconsistent with rclcpp `<std_msgs/msg/int32.hpp>` | Codegen emits per-message headers `std_msgs/msg/int32.hpp` + an umbrella `std_msgs.hpp` that includes them. Migration is the cargo subcommand's job. |
| B.9 | No `RCLCPP_INFO` logging macro | Users mix `std::printf` / `fprintf` | Add `NROS_INFO(...)`, `NROS_ERROR(...)`, etc. that route through `NROS_TRY_LOG`'s sink. |
| B.10 | `Result` vs exception | rclcpp throws; users forget to check | Document loudly; the new `NROS_TRY` family + auto-`NROS_TRY_LOG` covers this. |

### Work items

- **123.B.1 — Default `NROS_TRY_LOG` to stderr.** One-line
  header change; backward-compat opt-out.
- **123.B.2 — `nros::spin(node)` blocking entry.** Mirror of
  `rclcpp::spin`. Wraps `while (nros::ok()) nros::spin_once`.
- **123.B.3 — Env-aware `nros::init()`.** No-arg overload reads
  `$NROS_LOCATOR` / `$ROS_DOMAIN_ID`. Existing two-arg form
  stays.
- **123.B.4 — `Publisher<M>::make` / `Node::make` convenience.**
  Value-returning factories that construct into an
  `aligned_storage` slot. Original out-param API stays for
  zero-alloc users.
- **123.B.5 — Lambda-capable timer + subscription callbacks.**
  Header-only template that boxes a `std::function`-style
  capture (under `NROS_CPP_STD`); falls back to C fn pointer
  otherwise.
- **123.B.6 — `create_publisher` QoS overload.** Already
  available through the FFI; surface it.
- **123.B.7 — `NROS_INFO` / `NROS_ERROR` macros.** Wrap a
  printf-format call through the configurable sink.
- **123.B.8 — Per-message codegen headers.** Update
  `nros-codegen-cpp` to emit `std_msgs/msg/int32.hpp` etc.
  alongside the existing flat `std_msgs.hpp`. Maintain
  backward compat.

## Stream order

Likely cadence: B.1 + B.3 + B.6 first (one-line fixes), then
B.2 + B.5 (medium), then B.4 + B.7 + B.8 (refactors). Stream A
audit (A.1) can run in parallel; the SDK packaging itself
(A.3 / A.4) needs B.* to stabilise so it ships a non-moving
target.

## Acceptance criteria

1. A fresh rclcpp engineer can install the SDK (no Rust
   toolchain on their box), copy-paste the migration-guide
   snippet, and have a publisher running in under 10 minutes.
2. The migration-guide snippet is ≤ 30 lines of C++ for a
   1 Hz pub/sub pair.
3. SDK + source build coexist; source build is the only path
   for new RTOS ports.

## Notes

- `package.xml` stays. Required for codegen; aligns with ROS
  convention. Future colcon-like tooling can build on it.
- Stream B changes are additive — existing examples don't
  need a sweep. The migration-guide chapter (separate phase)
  consumes the new ergonomics.
