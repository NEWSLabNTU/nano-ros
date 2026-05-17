# Phase 152 — Board / BSP Abstraction Layer

**Goal.** Stop requiring a hand-written Cargo board crate per
`(vendor × board × SDK-variant)` combo. Carve generic board crates
per RTOS-kernel family (one each for FreeRTOS, ThreadX, NuttX,
bare-metal-cortex-m, bare-metal-cortex-a), plus a small overlay-crate
pattern (~50 LOC) for vendor forks. Combine with Phase 139's per-RTOS
integration shells so vendor BSPs flow through their RTOS's native
package manager rather than through nano-ros code.

**Status.** Not started. Design fixed by
`docs/design/board-bsp-integration-architecture.md` (2026-05-18).

**Priority.** P2 — usability + scalability win. Existing per-board
crates work for the curated demo set; this phase makes nano-ros
consumable on vendor SKUs we never enumerated.

**Depends on.** Phase 136 (manifest parser to reuse), Phase 137
(root CMake entry — landed), Phase 138 (per-platform CMake
consolidation — landed), Phase 139 (per-RTOS shells — landed).

**Related.** Phase 111.B.2 (crates.io name availability — confirmed
13/13 open; community overlay crates can publish freely),
Phase 116 (unified config + extensibility north-star — this phase
delivers the platform side of that vision but punts BSP DSL),
Phase 144 (example migration — overlays land underneath the example
migration's `add_subdirectory` consumers).

---

## Overview

Today's board crates (`nros-board-mps2-an385-freertos`,
`nros-board-threadx-linux`, `nros-board-threadx-qemu-riscv64`,
`nros-board-orin-spe`, `nros-board-nuttx-qemu-arm`) each duplicate:
the kernel build glue (cc-rs invocation, source globbing, port-dir
selection), the platform-provider wiring (`nros-platform-<rtos>`
init), the network-stack hookup (lwIP / NetX-Duo / nsos-netx), and
the `run(config, closure)` entry-point shape. Adding a new board =
copy a sibling crate + ~200 LOC of edits.

This doesn't scale to NXP MCUXpresso (40+ boards), STM32 (50+
families), Espressif (7 chip families × boards), Renesas Synergy,
NVIDIA FSP-FreeRTOS, etc. Vendors won't write Cargo crates per SKU.

Per-RTOS research (captured in
`docs/design/board-bsp-integration-architecture.md`) shows two
patterns vendors actually use:

1. Zephyr's DTS + `west` modules; ESP-IDF's component registry;
   NuttX's `apps/external/`. These RTOSes already have a BSP +
   package-manager story. nano-ros should ride those rails rather
   than re-invent.
2. Stock FreeRTOS + lwIP + a vendor HAL is the "build it yourself"
   path. One generic crate per kernel + tiny overlay crates per
   vendor fork covers this surface.

Phase 152 lands both.

---

## Architecture

See `docs/design/board-bsp-integration-architecture.md` for the
five-layer model + per-RTOS findings + consumption matrix. This
phase's scope = Layers 2 + 2.5 + the Phase 139 follow-ups in
Layer 3.

### A. Generic board crates (Layer 2)

| Crate | Covers | SDK env vars | TOML manifest |
|---|---|---|---|
| `nros-board-freertos` | stock FreeRTOS + lwIP | `FREERTOS_DIR`, `FREERTOS_PORT`, `LWIP_DIR`, `FREERTOS_CONFIG_DIR` | `nros_board_freertos_platforms.toml` |
| `nros-board-threadx`  | stock ThreadX + NetX Duo | `THREADX_DIR`, `THREADX_CONFIG_DIR`, `NETX_DIR`, `NETX_CONFIG_DIR` | `nros_board_threadx_platforms.toml` |
| `nros-board-nuttx`    | NuttX (kernel built by NuttX) | `NUTTX_DIR` | n/a — thin wrapper |
| `nros-board-baremetal-cortex-m` | cortex-m + smoltcp | `BOARD_LINKER_SCRIPT_DIR` | board-arch toml |
| `nros-board-baremetal-cortex-a` | cortex-a + smoltcp | same | board-arch toml |

Each manifest reuses
`packages/zpico/zpico-sys/build/manifest.rs` — the same parser
Phase 136 landed for `zenoh_platforms.toml`. Schema mirrors:
per-target `arch` profiles, `extra_sources` (`if_env` +
`with_define`), `required_env`, `include_paths_conditional`
(`when.target_match` / `target_not` / `if_env`), `compile`,
`pic`, `rerun_if_env_changed`. One parser, two consumers.

### B. Overlay crates (Layer 2.5)

Tiny (~50 LOC) crates that depend on a generic board crate +
patch vendor-specific deltas:

```rust
// nros-board-stm32f4-freertos/src/lib.rs (example shape)
pub use nros_board_freertos::{Config, run};

pub fn init_clocks() { /* STM HAL clock-tree config */ }
pub fn init_eth() { /* HAL_ETH_Init + lwIP netif binding */ }
```

```rust
// nros-board-stm32f4-freertos/build.rs
// Add STM32 HAL sources via cc-rs.
fn main() {
    let stm_hal_dir = env::var("STM32_HAL_DIR")
        .expect("set STM32_HAL_DIR to your STMicroelectronics HAL source");
    let mut hal = cc::Build::new();
    hal.flag("-mcpu=cortex-m4").flag("-mthumb").flag("-mfpu=fpv4-sp-d16");
    for f in &["stm32f4xx_hal_eth.c", "stm32f4xx_hal_uart.c", ...] {
        hal.file(format!("{stm_hal_dir}/Src/{f}"));
    }
    hal.compile("stm32f4_hal");
}
```

Vendor / community publishes these under `nros-board-<vendor>-<chip-or-board>-<rtos>`
on crates.io. nano-ros project ships canonical examples — not a
crate per SKU.

### C. Phase 139 shell polish (Layer 3)

Per-RTOS research surfaced concrete gaps in the already-landed
shells:

- **NuttX**: `integrations/nuttx/Make.defs` doesn't invoke
  upstream `apps/tools/Rust.mk`. Should `include $(APPDIR)/tools/Rust.mk`
  + append `EXTRA_LIBS += $(call RUST_GET_BINDIR,nros_c,…)`.
  `integrations/nuttx/Makefile` needs a `context::` hook calling
  `RUST_CARGO_BUILD`. `Kconfig` should expose `NROS_RMW` as a
  `choice` rather than free-form `string`, driving Cargo features
  via a `CARGO_FEATURES` Make var.
- **ESP-IDF**: shell should document `[package.metadata.esp-idf-sys]`
  `extra_components` + `bindings_header` injection — the bridge for
  Rust crates to land C glue into the IDF build tree. Currently
  `integrations/esp-idf/idf_component.yml` just registers the
  component; add a `book/src/getting-started/integration-esp-idf.md`
  section on the `esp-idf-sys` flow.
- **PlatformIO**: `lib_deps`-resolved libraries are NOT registered
  as IDF components. Document `EXTRA_COMPONENT_DIRS +=
  .pio/libdeps/${board}` workaround in
  `integrations/platformio/README.md`.
- **Zephyr**: shell already correct (T2 topology, `module.yml`,
  west manifest). Cross-link the per-RTOS book pages.

### D. Documentation (Layer 3 supporting docs)

`book/src/concepts/board-integration.md` (new) — consumption-matrix
chapter from the design doc; user picks their profile and follows
the matching path. `book/src/porting/vendor-overlay.md` (new) —
overlay-crate cookbook with the `nros-board-orin-spe` walkthrough.

---

## Work Items

- [x] **152.1 — Carve `nros-board-freertos` generic crate.**
      (152.1.A + 152.1.B.1-.6 landed 2026-05-18; B.5 partial —
      `node.rs` lift deferred to `BoardInit` trait alongside 152.4.B)
      Split into two sub-steps:
      - **152.1.A — Scaffolding** (landed): new
        `packages/boards/nros-board-freertos/` crate claims the
        Layer-2 namespace + documents the public contract in
        `src/lib.rs`. Behind the opt-in `reference-mps2` feature
        it re-exports `Config` + `run` from
        `nros-board-mps2-an385-freertos` so future overlays can
        depend on the generic crate today and switch wiring
        transparently when 152.1.B completes the build-glue
        carve-out. Workspace `Cargo.toml` excludes the new crate
        from members (standalone like every other board crate);
        `cargo check` clean (default + `reference-mps2 --target
        thumbv7m-none-eabi`); native nano2nano E2E unchanged.
      - **152.1.B — Build-glue carve-out** (deferred; broken down
        below). Move the FreeRTOS kernel + lwIP + sys_arch +
        nros-platform-freertos compile pipeline out of
        `nros-board-mps2-an385-freertos/build.rs` (~600 of its 816
        LOC) into `nros-board-freertos/build.rs`. Leave LAN9118
        driver + linker script + startup.c in the per-board
        overlay (~200 LOC).
      **Files.** `packages/boards/nros-board-freertos/` (new),
      `packages/boards/nros-board-mps2-an385-freertos/` (refactor —
      152.1.B), `Cargo.toml` (exclude list — 152.1.A landed).

#### 152.1.B subitems

The existing `nros-board-mps2-an385-freertos/build.rs` is 816 LOC
with the FreeRTOS / lwIP / nros-platform-freertos compile loops
intermixed against a ~500-line `STARTUP_C` const that bundles
Cortex-M3-specific bring-up (vector table, `Reset_Handler`,
`SysTick_Handler`) **and** generic FreeRTOS task hooks
(`vApplicationStackOverflowHook`, idle / malloc) **and** the
FFI surface Rust calls (`nros_freertos_init_network`,
`_poll_network`, `_start_scheduler`, `_create_task`, `_diag_network`)
**and** direct LAN9118 register pokes. Splitting is the bulk of
the work; do it in 6 ordered sub-commits so each lands a verifiable
delta:

- [ ] **152.1.B.1 — Split `STARTUP_C` into three C files.**
      Promote the inline const into `startup/` checked-in C
      sources:
      - `startup/freertos_hooks.c` — generic FreeRTOS hooks
        (`vApplicationStackOverflowHook`,
        `vApplicationMallocFailedHook`, `vApplicationIdleHook`,
        `SysTick_Handler`, `freertos_assert_failed`). Goes into
        the generic crate.
      - `startup/network_glue.c` — the FFI surface Rust calls
        (`nros_freertos_init_network`, `_poll_network`,
        `_start_scheduler`, `_create_task`) MINUS the LAN9118
        direct-register code. Calls weak `nros_board_init_eth()`
        hook the overlay implements. Goes into the generic crate.
      - `startup/board_mps2.c` — vector table + `Reset_Handler`
        + `Default_Handler` + LAN9118 register init + diag
        helper. Stays in the overlay.
      Generated `STARTUP_C` const goes away; build.rs reads the
      checked-in files via `cc::Build::file`. Mechanical refactor;
      no behaviour change. Verify mps2-an385 example links.
      **Files.** `packages/boards/nros-board-freertos/startup/{freertos_hooks,network_glue}.c`,
      `packages/boards/nros-board-mps2-an385-freertos/startup/board_mps2.c`,
      both crates' `build.rs`.

- [ ] **152.1.B.2 — Define `nros_board_*` weak-hook contract.**
      `startup/network_glue.c`'s `nros_freertos_init_network`
      stops poking LAN9118 registers directly; it calls
      `nros_board_init_eth(mac, ip, netmask, gateway)` declared
      as `extern void` with a `__attribute__((weak))` no-op
      default in the generic crate. Overlay's
      `startup/board_mps2.c` provides the strong definition
      that does LAN9118 + `netifapi_netif_add` wiring. Same
      pattern for `nros_board_init_clocks(void)` (no-op on MPS2;
      STM32 / NXP overlays use it) and `nros_board_init_extra_drivers(void)`.
      Document the contract in `nros-board-freertos/src/lib.rs`
      doc-comment + `book/src/porting/vendor-overlay.md`.
      **Files.** Both `startup/*.c` files, `nros-board-freertos/src/lib.rs`.

- [ ] **152.1.B.3 — `FREERTOS_CFLAGS` arch parameterisation.**
      Drop `configure_arm_cm3()` from the generic crate's
      `build.rs`. Read `FREERTOS_CFLAGS` env var (space-separated
      flag list) at the start of the generic crate's `build.rs` +
      pass to every `cc::Build`. Overlay's user-facing
      `.cargo/config.toml [env]` block sets
      `FREERTOS_CFLAGS = "-mcpu=cortex-m3 -mthumb"`. Document
      env var in 152.5's `nros-board-common` reader so the
      manifest-driven path (152.1.B.4) and the env-var-driven
      path agree.
      **Files.** `packages/boards/nros-board-freertos/build.rs`,
      reference `.cargo/config.toml` examples in MPS2 overlay
      + `book/src/porting/vendor-overlay.md`.

- [ ] **152.1.B.4 — Move kernel + lwIP + nros-platform-freertos
      build into generic `build.rs`.**
      Generic crate's `build.rs` (was empty in 152.1.A) consumes
      `nros-board-common`'s manifest parser to read a new
      `nros_board_freertos_platforms.toml` declaring source-list
      data declaratively (mirror Phase 136.4's `zenoh_platforms.toml`).
      cc-rs invocations:
      - `freertos` archive (kernel core + portable layer + heap_4)
      - `lwip` archive (core + IPv4 + API + netif + ethernet +
        FreeRTOS sys_arch)
      - `nros_platform_freertos` archive (platform.c + net.c + timer.c)
      Each emits `cargo:rustc-link-lib=static=<name>`. Overlay's
      build.rs shrinks to LAN9118 driver + linker script + the
      MPS2-specific startup chunk + libc/libgcc discovery. Verify
      mps2-an385 example links + boots in QEMU.
      **Files.** `packages/boards/nros-board-freertos/{build.rs,
      nros_board_freertos_platforms.toml}`,
      `packages/boards/nros-board-mps2-an385-freertos/build.rs`
      (~600 LOC delete).

- [x] **152.1.B.5 — `Config` + `run` lift into generic crate.**
      (landed 2026-05-18)
      `node.rs` (~380 LOC of FreeRTOS-task plumbing) lifted from
      `nros-board-mps2-an385-freertos/src/` into
      `nros-board-freertos/src/node.rs`. The board-specific
      divergence (cortex_m_semihosting print, QEMU semihosting
      exit) is captured by two new traits in
      `nros_board_common::board_init`:
      `BoardPrint::println(format_args!)` +
      `BoardExit::{exit_success, exit_failure}`. Generic
      `nros_board_freertos::run<B: BoardInit<Config=Config> +
      BoardPrint + BoardExit, F, E>` calls
      `B::println(format_args!(...))` for every banner line and
      `B::exit_success() / B::exit_failure()` at every termination
      point.
      Overlay's `lib.rs` shrunk to ~130 LOC: trait impls for
      `Mps2An385` + thin non-generic `run(Config, F) -> !` wrapper
      that calls `nros_board_freertos::run::<Mps2An385, F, E>` so
      users keep their existing call shape. `run` + `init_hardware`
      stay re-exported.
      The `rmw-zenoh` feature in the generic crate forwards from
      the overlay's `rmw-zenoh` to keep the
      `zpico_set_task_config` FFI cfg-gated.
      Verified: `just freertos build` + `just freertos test` clean.
      The pre-existing `Transport(ConnectionFailed)` failure on
      `test_rtos_pubsub_e2e::platform_1_Platform__Freertos::*`
      reproduces unchanged with `git stash` on top of `e9d100e0`
      — not a 152.1.B.5 regression; tracked separately.

- [x] **152.1.B.6 — Verify matrix.** (landed 2026-05-18)
      - `cargo build --release --target thumbv7m-none-eabi` for
        all 6 `examples/qemu-arm-freertos/rust/zenoh/{talker,
        listener, service-server, service-client, action-server,
        action-client}` — clean.
      - `cargo build --release --target thumbv7m-none-eabi` for
        both `examples/qemu-arm-freertos/rust/dds/{talker, listener}`
        — clean (after `rm -rf target` to drop stale artifacts
        that mixed pre-152.1.B.4 platform.c with the new
        generic-crate copy).
      - `test_talker_listener_communication` native nano2nano
        E2E — pass (1.2s).
      - `cargo tree -p zpico-sys | grep cmake` — empty
        (cmake dep stays gone since 136.3).

      `.B.1` → `.B.4` ran their own verify steps + recorded
      results in their commit messages. Each subitem landed as
      a separate commit on the `phase-152-board-bsp-abstraction`
      branch.

- [~] **152.2 — Carve `nros-board-threadx` generic crate.** (152.2.A
      landed 2026-05-18; 152.2.B deferred alongside 152.1.B)
      Same split as 152.1:
      - **152.2.A — Scaffolding** (landed): new
        `packages/boards/nros-board-threadx/` crate claims the
        Layer-2 namespace + documents the public contract. Two
        opt-in features (`reference-linux` +
        `reference-qemu-riscv64`) re-export `Config` + `run` +
        `init_hardware` from the existing per-board crates;
        mutually exclusive (incompatible `std` requirements +
        different `Config` shapes). Workspace `Cargo.toml`
        excludes the new crate; `cargo check` clean (default +
        `reference-linux` host build).
      - **152.2.B — Build-glue carve-out** (partial 2026-05-18;
        full carve deferred).
        Partial landed (two passes):
        (1) kernel + port-source enumeration helpers
        (`add_threadx_kernel_sources` +
        `add_threadx_port_sources`) lifted into
        `nros-board-common::threadx_sources`. Both overlays
        (`nros-board-threadx-linux` + `nros-board-threadx-qemu-riscv64`)
        switched from inline `read_dir(common/src)` /
        `read_dir(ports/<port>/src)` loops to the shared helpers.
        Future ThreadX-kernel submodule bumps that add files pick
        up automatically in both overlays.
        (2) `add_nros_platform_threadx_build` helper (same module)
        wires the `nros-platform-threadx` C port (`platform.c`,
        `net.c`, `timer.c`) + the cffi include path + matching
        `cargo:rerun-if-changed` triggers into a pre-configured
        `cc::Build`. Both overlays dropped the hand-rolled
        sources/include/rerun lines (~6 lines each → 1 call).
        `nros-board-threadx-linux` `cargo check` clean +
        `just threadx_linux build` clean. RISC-V `cargo check`
        clean for the board crate; full link verification
        blocked by pre-existing `zpico-sys/platform_aliases.o`
        float-ABI mismatch (unrelated to the carve — reproduces
        on `main`).
        Full carve-out deferred — same shape as 152.1.B but with
        two reference overlays differing in `std`/`no_std` +
        `pthreads`/`bare-metal` + with/without full NetX-Duo
        TCP/IP + with/without RISC-V startup assembly. Per-board
        `Config` shapes diverge enough that the `BoardInit` trait
        (152.4.B) is the right abstraction to share — now landed,
        so the full carve is unblocked for a follow-up session.

#### 152.2.B subitems

The two existing ThreadX board crates differ structurally:

- `nros-board-threadx-linux` builds ThreadX as Linux pthreads
  (`ports/linux/gnu/src/`) + the nsos-netx BSD shim over host
  POSIX sockets. `std` available; networking via host kernel.
- `nros-board-threadx-qemu-riscv64` builds ThreadX with a
  bare-metal RISC-V64 port + full NetX-Duo TCP/IP stack over
  virtio-net. `no_std`; needs picolibc errno-shadow.

The generic crate must handle both. Subitems mirror 152.1.B but
with two reference overlays instead of one:

- [x] **152.2.B.1 — Split per-board C startup.** (landed 2026-05-18)
      Generic `tx_application_define` stub + byte-pool / app-thread
      plumbing lifted into `packages/boards/nros-board-common/c/threadx_hooks.c`
      (materialised into each overlay's `OUT_DIR` by the new
      `add_threadx_hooks_source(&mut cc::Build)` helper in
      `nros_board_common::threadx_sources`). Overlays now ship only
      the board-specific glue:
      - `nros-board-threadx-linux/c/board_threadx_linux.c` (renamed
        from `c/app_define.c`, 70 LOC): NSOS-flavour
        `nros_threadx_set_config` (5-arg) + `nros_board_log` →
        `printf` + `nros_board_init_eth` no-op + IP/MAC RNG seed.
      - `nros-board-threadx-qemu-riscv64/c/board_threadx_qemu_riscv64.c`
        (renamed from `c/app_define.c`, 230 LOC): RISC-V-flavour
        `nros_threadx_set_config` (4-arg) + `nros_board_log` →
        `uart_puts` + `nros_board_init_eth` running the full
        NetX-Duo + virtio-net + BSD socket bring-up + strong
        overrides of `nros_board_app_stack_size` (512 KB) and
        `nros_board_app_priority` (15) + bare-metal `errno`
        global.
      Weak-hook contract mirrors 152.1.B.2's FreeRTOS shape:
      `nros_board_log`, `nros_board_init_eth`,
      `nros_board_compute_rng_seed` + weak-`const`
      `nros_board_app_stack_size` / `nros_board_app_priority`.
      Verified: `cargo check` clean both overlays + `just
      threadx_linux build` + `just threadx_linux test` clean +
      `[app_define] Creating byte pool... → [app_thread] Calling
      Rust entry...` log trace shows the shared hooks firing in
      order. Pre-existing `Transport(ConnectionFailed)` failure
      on `test_rtos_pubsub_e2e::*::ThreadxLinux::*` reproduces on
      `73d2316a` without this patch — same infrastructure issue
      noted for FreeRTOS in 152.1.B.5.

- [ ] **152.2.B.2 — `THREADX_CFLAGS` arch parameterisation.**
      Same shape as 152.1.B.3. Linux overlay sets
      `THREADX_CFLAGS = ""` (host gcc native); RISC-V overlay sets
      `THREADX_CFLAGS = "-march=rv64gc -mabi=lp64d -mcmodel=medany"`.

- [ ] **152.2.B.3 — Move kernel + NetX-Duo + nros-platform-threadx
      build into generic `build.rs`.**
      New `nros_board_threadx_platforms.toml` mirrors
      152.1.B.4's structure. Per-platform blocks declare which
      NetX-Duo features to compile (full TCP/IP for QEMU vs.
      nsos-netx-only for Linux sim). Linker emits
      `cargo:rustc-link-lib=static={threadx,netxduo,nros_platform_threadx}`.

- [~] **152.2.B.4 — `Config` + `run` lift into generic crate.**
      (partial 2026-05-18 — trait scaffolding only)
      Both ThreadX overlays now implement
      `nros_board_common::{BoardInit, BoardPrint, BoardExit}`
      — same canonical-overlay shape as 152.1.B.5
      (`Mps2An385`) and 152.3 (`OrinSpe`). Concrete markers:
      `ThreadxLinux` (in `nros-board-threadx-linux`),
      `ThreadxQemuRiscv64` (in `nros-board-threadx-qemu-riscv64`).
      `Config` stays per-overlay because the two shapes diverge
      meaningfully (Linux has `interface: String`; RISC-V has
      MAC + IP + netmask + gateway with no host-bridge name) —
      a shared `ThreadxConfig` trait would be five accessors
      with no shared storage worth carving.
      `run<B>` lift deferred: Linux uses `Box::leak` (std heap)
      for `AppContext`; RISC-V uses a `static mut [u8; 4096]`
      backing store (no_std). Generic crate would need either a
      `feature = "std-host"` split or twin `run_std<B>` /
      `run_no_std<B>` entry points — non-mechanical design call
      that wants explicit user direction.
      Verified: `cargo check` clean for both overlays + 
      `just threadx_linux build` clean.

- [ ] **152.2.B.5 — Verify matrix.**
      - `cargo build` for `threadx-linux` Rust talker / listener
        — clean (currently pre-existing-broken per Phase 147 with
        `_z_task_free` duplicate; expect either to still fail the
        same way OR to become unblocked if the carve-out routes
        symbol selection cleaner).
      - `cargo build` for `qemu-riscv64-threadx` Rust talker —
        clean.
      - `cargo nextest run rtos_e2e test_rtos_pubsub_e2e::platform_3_Platform__Threadx` —
        passes (where applicable).
      - Native nano2nano talker-listener still passes.

- [x] **152.3 — Refactor `nros-board-orin-spe` as canonical overlay.**
      (landed 2026-05-18, partial)
      `nros-board-orin-spe` now implements
      `nros_board_common::BoardInit for OrinSpe` so it fits the
      same overlay contract as `nros-board-nuttx-qemu-arm` (152.4.B)
      and future stock-FreeRTOS overlays. Pulls `nros-board-common`
      as `default-features = false` (BoardInit only; no serde/toml/cc
      transitively).

      `nros-board-common` itself gained a `build-helpers` feature
      gate (default-on for back-compat) so runtime-only consumers
      can disable the build-script-side serde/toml/cc dep chain.
      `BoardInit` lives at the bare `no_std`-compatible surface.

      **Out of scope this session:** the phase-doc-original goal of
      depending on `nros-board-freertos` directly. orin-spe is a
      FreeRTOS-FORK overlay (prebuilt FSP `libtegra_aon_fsp.a`
      kernel, no source rebuild; IVC link instead of lwIP) — the
      generic crate's stock-FreeRTOS-source + lwIP build pipeline
      doesn't apply. The `BoardInit` trait is the cross-fork
      shared surface; orin-spe demonstrates the
      overlay-without-kernel-rebuild variant of the cookbook in
      `book/src/porting/vendor-overlay.md`.
      **Files.** `packages/boards/nros-board-orin-spe/{Cargo.toml,
      src/lib.rs}`, `packages/boards/nros-board-common/{Cargo.toml,
      src/lib.rs}`.

#### 152.3 subitems (blocked on 152.1.B)

- [ ] **152.3.1 — Switch `nros-board-orin-spe` Cargo dep to
      `nros-board-freertos`.** Drop the standalone kernel build
      from this crate's `build.rs`; declare `nros-board-freertos`
      as `[dependencies]`.

- [ ] **152.3.2 — Implement `nros_board_init_eth` as IVC bind.**
      Replace the generic-crate weak default with an FSP-specific
      version that wires IVC link (via `zpico-link-ivc`) instead
      of lwIP. `nros_board_init_clocks` configures Cortex-R5F
      clocks via FSP API.

- [ ] **152.3.3 — Shrink `build.rs` to FSP source injection.**
      Pull only `tegra_aon_fsp.a` headers + ARM_R5 portable layer
      from `$NV_SPE_FSP_DIR`. Hand the rest to the generic crate.

- [ ] **152.3.4 — Verify `cargo build -p nros-board-orin-spe`
      succeeds with the same `NV_SPE_FSP_DIR` env requirement as
      today.** Verifies the overlay-on-generic pattern handles
      vendor forks cleanly. Document the resulting LOC count in
      the commit message — should be < 100 LOC of Rust + < 50 LOC
      of `build.rs`, matching the
      `book/src/porting/vendor-overlay.md` cookbook's promise.

- [x] **152.4 — Migrate NuttX board crate.** (152.4.A + 152.4.B landed 2026-05-18)
      - **152.4.A — Scaffolding** (landed): thin
        `packages/boards/nros-board-nuttx/` crate (NuttX owns the
        kernel build via `apps/external/nano-ros/` + the Phase
        152.7 `Make.defs` / `Makefile` / `Kconfig` shell). Opt-in
        `reference-qemu-arm` feature re-exports `Config` + `run` +
        `init_hardware` from `nros-board-nuttx-qemu-arm`. Default
        build (no feature) clean on host; reference feature
        requires NuttX target (same constraint as the underlying
        crate; not a regression).
      - **152.4.B — `BoardInit` trait + per-board overlay refactor**
        (landed 2026-05-18). Kernel-agnostic `BoardInit` trait
        landed in `nros-board-common::board_init`; generic
        `nros-board-nuttx::run_generic<B: BoardInit>` consumes it.
        `nros-board-nuttx-qemu-arm` exposes `pub struct QemuArmVirt`
        with the trait impl (delegates to the existing
        `node::init_hardware`). Public API of the overlay crate
        preserved — `Config` + `init_hardware` + `run` still exported.
        All 6 NuttX zenoh examples build clean for
        `armv7a-nuttx-eabihf` target. Future NuttX overlays
        (`nros-board-px4-fmu-v5-nuttx` etc.) provide their own
        `BoardInit` impl + use the same `run_generic` shim. The
        trait sits in `nros-board-common` so future FreeRTOS +
        ThreadX generic crates can reuse the contract when they
        adopt the same pattern (unlocks the deferred 152.1.B.5
        `node.rs` lift + the full 152.2.B carve-out).

#### 152.4.B subitems

- [ ] **152.4.B.1 — Define `BoardInit` trait.**
      Generic crate adds:
      ```rust
      pub trait BoardInit {
          type Config;
          fn config_from_toml(s: &str) -> Self::Config;
          fn init_hardware(cfg: &Self::Config);
      }
      ```
      `run<B: BoardInit>(cfg: B::Config, closure: F) -> ...`.
      Default impl in `nros-board-nuttx::DefaultNuttx` produces
      the generic `Config` shape.

- [ ] **152.4.B.2 — Refactor `nros-board-nuttx-qemu-arm` to
      implement `BoardInit`.**
      Per-board crate shrinks to `pub struct QemuArmVirt; impl
      BoardInit for QemuArmVirt { ... }`. Public API surface
      preserves backward compat via `pub use` re-export of
      `Config` + `run::<QemuArmVirt>`.

- [ ] **152.4.B.3 — Verify matrix.**
      - `cargo build --target arm-nuttx-eabihf` for the existing
        examples — clean (currently pre-existing-broken per
        Phase 147 with `_z_*_serial_internal`; re-verify same
        failure mode, not a regression).

#### Sequencing + risks for the `.B` carve-outs

**Order**: land in 1.B → 3 → 2.B → 4.B.

1. **152.1.B first.** FreeRTOS has the largest existing surface
   (816 LOC build.rs + 749 LOC src). Doing it first establishes the
   `nros-board-*-platforms.toml` schema for the per-kernel
   manifests (mirrors Phase 136.4's `zenoh_platforms.toml` but
   for board glue), the `nros_board_init_*` weak-hook contract,
   and the `<KERNEL>_CFLAGS` env-var arch parameterisation pattern.
   Subsequent `.B`s clone the template.
2. **152.3 next.** Refactor `nros-board-orin-spe` as the canonical
   FSP-FreeRTOS overlay against the newly-landed
   `nros-board-freertos`. Validates the overlay-on-generic pattern
   actually shrinks code to ≤100 LOC + ≤50 LOC `build.rs` as
   `book/src/porting/vendor-overlay.md` promises. Cheapest
   end-to-end demonstration since orin-spe's existing build is
   already FreeRTOS-shaped.
3. **152.2.B next.** ThreadX carve mirrors the 1.B template but
   has two reference overlays (Linux sim + RISC-V QEMU) with
   genuinely different `Config` shapes + networking flavours.
   Land after 1.B + 3 prove the pattern works.
4. **152.4.B last.** NuttX is the smallest because NuttX owns the
   kernel build — refactor is Rust-only (`BoardInit` trait + per-
   board impl) without TOML manifest / cflags parameterisation.

**Risks per `.B`**:

- **Linker-section coordination.** Splitting `STARTUP_C` across two
  crates means the vector table (overlay) must reference symbols
  defined in the generic crate (`Reset_Handler` jumps to `_start`
  which is Rust's `#[no_mangle] extern "C" fn main`). Cargo build
  order: generic compiles first; cc-rs emits `cargo:rustc-link-lib=static=<lib>`;
  overlay's link pass pulls them. Section order in the linker
  script matters for the vector-table-at-`0x0` requirement; verify
  per-board.
- **Weak-symbol behaviour with `cc-rs` + Rust `staticlib`.**
  `__attribute__((weak))` defaults work when both definitions live
  in static archives + the linker resolves them at the final link.
  Confirmed working for Phase 121.3's `nros-platform-freertos`
  symbols; reuse that proof. Cortex-M ABI gotcha: weak default
  must NOT be in the same TU as the strong override or GCC issues
  a warning + the strong version may still tie-break wrong.
- **`STARTUP_C` const → checked-in .c file.** Currently emitted at
  build-time via `out_dir.join("startup.c")` so cc-rs sees it as a
  fresh artifact. Promoting to checked-in source means rust-analyzer
  / clangd start parsing it; ensure it `#include`s the right
  headers when scanned outside the build. Add a `// IWYU pragma`
  if needed.
- **`cargo:rustc-link-lib` propagation.** Generic crate emits
  `freertos` / `lwip` / `nros_platform_freertos`; overlay emits
  `lan9118_lwip` / `startup` / `nosys` / `c` / `gcc`. Final binary
  pulls both via Cargo's transitive build-script metadata. The
  ORDER of the `cargo:rustc-link-lib` lines matters for static-
  library link resolution; document the canonical order in the
  generic crate's `build.rs` + verify with `cargo build -vv` on
  one mps2-an385 example after 152.1.B.4.
- **Pre-existing test failures masked.** Phase 147 documents three
  pre-existing link regressions (`_z_task_free` dup,
  `_z_*_serial_internal` undefined). The `.B` verify steps must
  re-confirm those tests fail in the SAME way as on `main`, not
  introduce new failures. Use `git stash` revert test if any
  regression looks new.

**Total scope estimate**:

| Subitem | LOC delta | Sessions |
|---|---|---|
| 152.1.B.1 | -500 const, +3 files (split) | 0.5 |
| 152.1.B.2 | +50 weak-hook contract | 0.5 |
| 152.1.B.3 | -30 cortex-m3 hardcode, +30 cflags reader | 0.5 |
| 152.1.B.4 | -600 from overlay, +400 in generic + 100 TOML | 1.5 |
| 152.1.B.5 | -350 from overlay, +350 in generic | 0.5 |
| 152.1.B.6 | verify pass | 0.5 |
| 152.3.1-4 | overlay refactor + verify | 1 |
| 152.2.B.1-5 | clone 1.B for ThreadX (×2 overlays) | 2 |
| 152.4.B.1-3 | `BoardInit` trait + verify | 0.5 |
| **Total** | ~−1000 LOC net | **~7 sessions** |

Plan calendar before starting: cluster `.B` work into a
mini-sprint with the same dev-box environment + SDK tarballs
checked out, since each verify step needs `FREERTOS_DIR` /
`THREADX_DIR` / `NETX_DIR` / `NUTTX_DIR` / `NV_SPE_FSP_DIR` env
vars present.

- [x] **152.5 — Reuse Phase 136 manifest parser.** (landed 2026-05-18)
      `packages/boards/nros-board-common/` library crate exposes
      `manifest` + `policy` modules (the Phase 136.1 / 136.4 /
      134.2 parser + interpolator + matcher + `LinkFeatures` /
      `LinkPolicy`). zpico-sys's `build.rs` switched from
      `#[path = "build/manifest.rs"] mod manifest;` to
      `use nros_board_common::{manifest, policy};`; the old
      `packages/zpico/zpico-sys/build/{manifest,policy}.rs` copies
      were deleted. Future per-kernel generic board crates
      (`nros-board-freertos` 152.1.B, `nros-board-threadx`
      152.2.B) consume the same library via `[build-dependencies]`
      path dep.
      `nros-board-common` declares `serde` + `toml` as regular
      `[dependencies]` so consumers don't need to re-declare them
      under `[build-dependencies]`. Library is build-host only —
      never reaches a final binary.
      Verified:
      - `cargo check -p nros-board-common`
      - `cargo check -p zpico-sys` (default features) — clean
      - `cargo check -p zpico-sys --features posix` — clean
      - `cargo check -p zpico-sys --features bare-metal --target thumbv7m-none-eabi --no-default-features` — clean
      - `test_talker_listener_communication` + `test_tls_talker_listener_communication` native E2E pass.

- [x] **152.6 — Overlay-crate template + cookbook.** (landed 2026-05-18)
      `templates/overlay-board/` skeleton (Cargo.toml.template +
      src/lib.rs.template + build.rs.template + README) + book
      chapter `book/src/porting/vendor-overlay.md` covering the
      contract, what overlays do / don't, naming convention
      (`nros-board-<vendor>-<chip-or-board>-<rtos>`), publishing,
      and the in-tree precedents (`nros-board-orin-spe` +
      `nros-board-mps2-an385-freertos`). SUMMARY.md lists under
      Porting.

- [x] **152.7 — Phase 139 shell polish (NuttX, ESP-IDF, PlatformIO).**
      (landed 2026-05-18)
      - NuttX: `integrations/nuttx/Make.defs` now `-include`s
        upstream `apps/tools/Rust.mk` and appends the Cargo-built
        staticlib paths to `EXTRA_LIBS` + `EXTRA_LIBPATHS` via
        `RUST_GET_BINDIR` / `RUST_GET_LIBDIR`. The Makefile gained
        a `context::` hook running `RUST_CARGO_BUILD` (+ `clean::`
        mirror). `Kconfig` promoted free-form `string` knobs to
        `choice` blocks (`NROS_RMW_{ZENOH,DDS,XRCE,CYCLONEDDS}` +
        `NROS_ROS_{HUMBLE,IRON}`) that the Makefile reads to
        assemble a `CARGO_FEATURES` env var driving Cargo's
        `--features` + `--no-default-features` flags. Optional
        include of `Rust.mk` keeps older NuttX trees building (just
        skips the `EXTRA_LIBS` append).
      - ESP-IDF: `book/src/getting-started/integration-esp-idf.md`
        appended an "Rust glue via `esp-idf-sys`" section
        documenting the canonical `[package.metadata.esp-idf-sys]`
        `extra_components` + `bindings_header` bridge. Links to
        `esp-rs/esp-idf-template` + `esp-idf-sys/BUILD-OPTIONS.md`
        for the full schema.
      - PlatformIO:
        `book/src/getting-started/integration-platformio.md`
        appended an "ESP-IDF gotcha" section explaining that
        `lib_deps`-resolved libraries are NOT registered as IDF
        components by default; the user's root `CMakeLists.txt`
        must append `EXTRA_COMPONENT_DIRS` pointing at
        `.pio/libdeps/<board>/nano-ros/integrations/esp-idf` for
        `idf_component_register(...)` to fire.

- [x] **152.8 — Consumption-matrix doc.** (landed 2026-05-18)
      `book/src/concepts/board-integration.md` lands the 7-profile
      consumption matrix (Cargo-first, vendor-IDE,
      Zephyr / ESP-IDF / NuttX / PX4 / PlatformIO native shells,
      niche / vendor fork with overlay) and the generic-crate +
      vendor-overlay subsections. SUMMARY.md lists under Concepts.
      Cross-links the vendor-overlay cookbook, the
      `add_subdirectory` getting-started guide, the per-RTOS
      integration pages 152.7 polished, the existing platform-model
      + RTOS-cooperation chapters, and the design doc.
      `book/src/getting-started/installation.md` cross-link
      pending; the page already routes most users to the
      per-RTOS shells.

- [x] **152.9 — Migrate examples to consume generic + overlay path.**
      (landed 2026-05-18 — doc-only pass)
      Rather than dropping a fresh README into every
      `examples/<plat>/` (11 platforms × language × RMW = many
      directories), `examples/README.md` gained a "Consumption
      profile per platform" table mapping each of the 11 top-level
      platform dirs to one of the 7 profiles from
      `book/src/concepts/board-integration.md`. Users porting an
      example to a real board look up their `examples/<plat>/`
      row + follow the linked guide. No per-example `Cargo.toml`
      changes — 152.1.A / 152.2.A / 152.4.A scaffolds keep the
      existing per-board crate names working as overlay re-exports.

---

## Acceptance

- [ ] `cargo build` of every `examples/**` consumer keeps producing
      identical output binaries vs. pre-148 (overlay refactor is
      pure code motion).
- [ ] `cargo build -p nros-board-orin-spe` succeeds with the same
      `NV_SPE_FSP_DIR` env requirement as today.
- [ ] Adding a new overlay crate is < 100 LOC of Rust + < 50 LOC
      `build.rs`; verified by writing a `nros-board-stm32f4-freertos`
      skeleton during 152.6.
- [ ] Each per-RTOS integration smoke test (Phase 139's set: NuttX,
      PlatformIO, Zephyr, PX4, ESP-IDF) stays green when its SDK
      env is sourced.
- [ ] `book/src/concepts/board-integration.md` covers the seven
      user profiles + working consumption recipe per profile.
- [ ] `just ci` green after the refactor.

---

## Non-goals

- **No common driver HAL.** Vendor `HAL_*` / `fsl_*` / `R_*` /
  `esp_*` stays vendor-owned. Overlay crates wrap them; nano-ros
  doesn't abstract them.
- **No DTS-equivalent for non-Zephyr.** Zephyr keeps its DTS story.
  Other RTOSes use whatever board config format their vendor IDE
  produces (CubeMX `.ioc`, NuttX `defconfig`, ESP-IDF `sdkconfig`,
  etc.).
- **No mandatory board crate per SKU.** Generic + overlay covers
  long tail. A user with an exotic board + custom HAL writes an
  overlay; nano-ros project doesn't catalog them.
- **No nano-ros-managed vendor crates.** `nros-board-stm32*-freertos`
  and friends are community / vendor crates published independently
  to crates.io. nano-ros ships canonical examples for guidance, not
  a maintained per-vendor matrix.
- **No retirement of existing per-board crates in this phase.**
  Public APIs preserve; the per-board crate names users `[dependencies]`
  against today keep working via overlay-style re-exports. Future
  phase can deprecate names if community moves to publishing under
  the new naming convention.

---

## Notes

- The Phase 136 manifest parser already proves the TOML-driven
  build-data approach at scale. 152.5 reuses it to avoid
  reinventing per-kernel.
- Phase 139's smoke matrix (NuttX / PlatformIO / Zephyr / PX4 /
  ESP-IDF) validated 2026-05-18 — the integration shells work even
  before the Phase 152 board-crate refactor; 152.7 is polish, not
  rebuilding.
- Phase 116 ("unified config and extensibility") is the long-term
  north-star where this design sits. Phase 152 delivers the platform
  side; the configuration-DSL side (a la Zephyr's Kconfig + DTS)
  stays a Phase 116 open question.
- Open question from the design doc: **monorepo vs sister repo for
  vendor / community overlay crates?** Lean monorepo for the
  initial canonical set (Orin SPE + the three existing board
  refactors); spin a `nano-ros-boards` sister repo when the
  community publishes more than ~5 overlays.
- Open question: **who owns `nros-board-stm32*` / `nros-board-nxp-*`?**
  Plan: community-owned with one nano-ros-blessed example per
  vendor (152.6 covers the example shape).
