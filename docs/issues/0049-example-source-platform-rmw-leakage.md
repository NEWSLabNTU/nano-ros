---
id: 49
title: Example source leaks platform/RMW selection + low-level boilerplate — should be agnostic application logic only
status: open
type: tech-debt
area: examples
related: [phase-244, phase-245, rfc-0024, rfc-0032]
---

## The bar

An example's **source** (`src/*.rs`, `src/*.{c,cpp,h,hpp}`) must contain **only the
platform-agnostic + RMW-agnostic application logic** — a talker demonstrates "create
a publisher, publish on a timer", nothing else. Platform selection (FreeRTOS /
NuttX / ThreadX / Zephyr / esp32 / native / bare-metal), RMW selection (zenoh /
cyclonedds / xrce), and low-level boilerplate (no_std, panic handlers, linker/alloc
wiring, board/HW init, transport bring-up, `Executor::open` plumbing) belong in
**build + config files** (`Cargo.toml` features, `.cargo/config.toml`,
`[package.metadata.nros.deploy.*]`, CMake, `prj.conf`/Kconfig, launch xml) or the
**board / `nros::main!()` / `nros::node!()` macro layers** — never in source.

## Finding (2026-06-13 audit, 16 agents, 200 example pkgs)

**86 clean (43%) · 33 minor (16%) · 81 major (41%).**

The macro/board layer **already exists and works** — proven by the clean Entry
packages and all declarative C/C++ components. The major-tier examples are older
hand-wired variants that predate it, or platform-collapsed shapes (Zephyr 168.4,
esp32 bare-metal) that never adopted it.

**Reference-clean target shape:**
- Rust → `nros::main!()` + `nros::Node` (source is ~one line): `phase216-rtic-e2e`,
  all `stm32f4/rust/*-rtic`/`*-embassy` entries, `workspaces/rust/src/*_entry`.
- C/C++ → declarative `NROS_NODE_REGISTER` / `NROS_C_COMPONENT`: all
  `qemu-arm-freertos/{c,cpp}`, `threadx-linux/cpp`, `qemu-riscv-nuttx/c`,
  `zephyr/cpp/talker-typed`.

**Worst offenders:** `qemu-riscv64-threadx` (20/20 major — manual `Executor::open`
+ spin loops everywhere); `zephyr` C/C++ 168.4 (RMW `#if CONFIG_NROS_RMW_*` +
`<zephyr/kernel.h>` + `k_sleep` in `main.cpp`); `qemu-esp32-baremetal` (densest,
~14 leaks each); `qemu-arm-baremetal/rust` (13/15 — no_std + panic + RMW register
+ net + bare RTIC plumbing).

## Cross-cutting leak patterns

| # | Pattern | ~examples | Correct destination |
|---|---------|-----------|---------------------|
| P1 | Manual session/executor wiring (`Executor::open`, `nros_support_init`+`nros_executor_init`, explicit `spin_once` loops) | 50+ | `nros::main!()` expansion / generated `nros_system_main()` / board crate |
| P2 | `#![no_std]` (+`#![no_main]`) in example source | 60+ | injected by `nros::node!()`/`nros::main!()`; node libs std-agnostic |
| P3 | RMW backend hardcoded in source (`nros_rmw_zenoh::register()`, `.rmw("zenoh")`, `register_rmw()`) | 30+ | linkme auto-registration / macro+board; selection via `Cargo.toml [features]` |
| P4 | RMW selection via `#if`/`#[cfg(feature=rmw-*)]` branches (`compile_error!` guards, `#if defined(CONFIG_NROS_RMW_*)`, `ACTIVE_RMW_NAME`) | 25+ | guard belongs in framework/macro crate; example calls `nros::init()` unconditionally |
| P5 | Panic/backtrace/bootloader/entry boilerplate (`panic_semihosting`, `esp_backtrace`, `esp_app_desc!()`, `#[entry]`, `no_mangle extern "C" fn main`) | 25+ | board-crate default deps / `.cargo/config.toml` / macro expansion |
| P6 | Hardcoded network + locator (`Config{mac,ip,…}`, `const LOCATOR`, `tcp/127.0.0.1:PORT`) | 25+ | `[package.metadata.nros.deploy.<target>]` / board defaults (cf. #48 `run_with_deploy`) |
| P7 | Platform headers/APIs in app source (`zephyr/kernel.h`, `nros_platform_zephyr_wait_network`, `k_sleep`, `esp_println`, PX4 `PX4_INFO`/`rmw_vtable.h`) | 20+ | board/platform-abstraction crate; agnostic `nros::log!`/timer API |
| P8 | RTIC device/dispatcher plumbing (`#[rtic::app(device=…, dispatchers=[…])]`, `Mono::start`, `enable_wfi_idle`) | 7 | new `nros_rtic_app!()` macro |
| P9 | Custom-transport raw FFI callbacks in example (`cb_open/close/write/read`, `set_custom_transport`) | 3 | reusable transport library crate |
| P10 | Manual type registration (`nros_rmw_cyclonedds::register::<…>()`, `nros_action_type_t{}` literals) | 10 | RosAction trait bridge / generated code auto-registers |

**Not leaks (do not "fix"):** `extern crate nros_platform_cffi as _;` (link-forcing);
`NROS_APP_MAIN_REGISTER_POSIX()` (standard C entry macro); `build.rs` linker/Kconfig
bridges; `serial-talker` env-var config; rclcpp-compat ROS 2 idioms.

## Cleanup

Work items + parallel clusters + wave ordering →
[phase-244](../roadmap/phase-244-example-source-cleanliness.md). The dirtiest
group (qemu-riscv64-threadx, cluster C1) is a re-architecture carved into
[phase-245](../roadmap/phase-245-riscv64-threadx-example-port.md). Closes when
every example is `clean` or `minor` (residual `minor` = node-lib `#![no_std]`,
the E4-confirmed accepted minor — proc-macros cannot inject crate-level attrs).
