# Phase 173 — Unified platform-entry abstraction

**Goal.** Collapse the per-platform sprawl in the board-entry layer and
the orchestration generator so that **adding a new platform that shares
an existing CPU-arch + boot convention is data-only** — one descriptor
row plus one `impl Board` — with code changes reserved for genuinely new
boot/link shapes. Three cross-cutting factors shape the design:

1. **Language parity.** nano-ros ships Rust *and* C/C++ surfaces. The
   features layer already crosses the language boundary through a C ABI
   (`nros/platform.h`); the workflow (board entry) layer must do the
   same — a Rust-only generic `run<B>` strands every C/C++ app. The
   abstraction therefore lands a board **C ABI**, mirroring the platform
   pattern, so the one `Board` impl serves Rust *and* C/C++ entry.
2. **Lean on the embedded ecosystem; don't reinvent peripherals.**
   `embedded-hal` / `embedded-nal` / `embedded-io` already standardise
   the peripheral + network + byte-stream trait surfaces. nano-ros's
   driver crates should consume those, not define a competing
   `Peripheral` trait. nano-ros's *own* config concern is narrower: the
   **transport ⟷ RMW binding** — which transport(s) (Ethernet / serial /
   CAN) a build uses and which RMW rides each, with bridge mode being
   2+ (transport, RMW) pairs. Per-transport hardware params (IP,
   baudrate, pins) stay in the platform's native config (board
   `config.toml`, RTOS Kconfig/devicetree), not a nano-ros invention.
3. **Default-first UX.** Standard ROS users run with defaults and tune a
   backend via one env var (`CYCLONEDDS_URI`, `ZENOH_LOCATOR`). nano-ros
   moves the transport + RMW *shape* choice to compile time (a no_std
   MCU can't carry every backend), but the default build must just boot
   with zero config, and customisation must be one small declarative
   surface (transport + RMW), with runtime values still env/`config.toml`
   overridable.
4. **Bounded essential variation.** Target triple, toolchain, boot entry
   attribute, and link glue genuinely differ per platform and are kept
   behind small finite enums — not abstracted away, just centralised.

**Status.** Proposed.

**Priority.** P2. Pure consolidation; unblocks cheap esp32-s3 / stm32f4 /
additional Cortex-M boards. Not blocking any open milestone, but the
platform count (7) has crossed the threshold where the match-arm
duplication costs more than the abstraction.

**Depends on.** Phase 126 (archived; per-platform generator support landed for
posix / freertos / nuttx / zephyr / threadx / esp32-c3 / bare-metal).

## Motivation

Three layers participate in "boot a generated nano-ros binary on
platform X". Their current maturity is asymmetric:

| Layer | Crate(s) | Role | State |
|---|---|---|---|
| **Features** | `nros-platform-api` + `nros-platform-*` + `nros-platform-cffi` | clock / net / timer / log / … | **Unified + drift-gated.** ~19 capability traits; every Rust platform crate is the same 4 `nros_platform_export_*!` macro calls; `nros/platform.h` ABI; `check-platform-abi-mirror` keeps emission ⟷ header in sync. |
| **Workflow** | `nros-board-*` | `init_hardware` + entry/`run()` | **~60% unified.** `BoardInit`/`BoardPrint`/`BoardExit` traits exist, but three families (`nros-board-{freertos,nuttx,threadx}`) each redeclare a divergent generic `run` (`run<B,F,E>(Config…)` vs `run_generic<B,…>(B::Config…)` vs `run<B,C,…>(C…)`), and esp32 + bare-metal hand-roll their own `run` bypassing the traits. |
| **Codegen** | `nros-cli-core` orchestration generator | emit Cargo.toml / `.cargo/config.toml` / `rust-toolchain.toml` / `main.rs` / build.rs link glue | **Per-platform match arms** across ~6 functions (`render_platform_dependencies`, `render_cargo_config`, `render_rust_toolchain`, `render_platform_link_directives`, `generated_default_features`, `platform_feature`). New platform = edit all six. |

The **features** layer is the reference for how the other two should
look: a typed contract, a uniform impl pattern, a drift gate. This
phase brings the **workflow** and **codegen** layers up to that bar.

What stays genuinely platform-specific (cannot and should not be
abstracted away):

- target triple + ABI rustflags (cortex-a7 neon-vfpv4 vs riscv32imc vs
  xtensa-esp32s3),
- toolchain (stable vs pinned nightly vs `+esp`),
- the boot **entry attribute** — `fn main` (hosted) / `#[cortex_m_rt::entry]`
  / `_start` (FreeRTOS) / `#[esp_hal::main]` (esp-hal) / `staticlib`+CMake
  (Zephyr). This is ~5 finite shapes and lives in the *caller*
  (generated `main.rs`), not in `run()`.

The win is turning the *accidental* sprawl (board-dep names, target
strings, rustflags lists, build-std sets, patch entries, three
copy-pasted `run` fns) into data, while keeping the *essential*
variation behind a small finite enum.

## Work items

### 173.1 — One `Board` trait + one generic `run`

Add to `nros-board-common`:

```rust
/// Everything the generic entry driver needs from a concrete board.
/// Blanket-implemented for any type that already carries the three
/// split traits, so existing `BoardInit + BoardPrint + BoardExit`
/// impls satisfy it for free.
pub trait Board: BoardInit + BoardPrint + BoardExit {}
impl<T: BoardInit + BoardPrint + BoardExit> Board for T {}

/// The single kernel-family entry driver. Replaces the three
/// per-family `run` / `run_generic` / `run<B,C,…>` declarations.
pub fn run<B, F, E>(cfg: B::Config, f: F) -> !
where
    B: Board,
    F: FnOnce(&B::Config) -> Result<(), E>,
    E: core::fmt::Debug,
{
    B::init_hardware(&cfg);
    match f(&cfg) {
        Ok(()) => { B::println(format_args!("nros: application complete")); B::exit_success() }
        Err(e) => { B::println(format_args!("nros: application error: {e:?}")); B::exit_failure() }
    }
}
```

Then:

- `nros-board-{freertos,nuttx,threadx}` — delete their bespoke
  `run` / `run_generic`; re-export `nros_board_common::run`. Keep only
  the kernel-level glue that legitimately differs (their `init_hardware`
  helpers feed `BoardInit`).
- `nros-board-esp32-qemu`, `nros-board-mps2-an385` — stop hand-rolling
  `run`; add `impl BoardInit/BoardPrint/BoardExit` (init_hardware =
  esp-hal init + log-writer reg / cortex-m init) and route through the
  common `run`.
- Normalise the config generic on `B::Config` everywhere (the ThreadX
  `<B,C,…>` extra param + the FreeRTOS crate-local `Config` both
  collapse to `B::Config`). `B::Config: nros_platform::BoardConfig`
  already gives `zenoh_locator()` + `domain_id()` so the generated
  `main.rs` builds `ExecutorConfig` uniformly.

Net: every board exposes the identical `<board>::run(cfg, closure) -> !`.

### 173.2 — `PlatformProfile` descriptor + `EntryKind` in the generator

Replace the six match-arm functions with one lookup table:

```rust
struct PlatformProfile {
    board_dep: &'static str,                 // workspace-relative crate path
    extra_deps: &'static [&'static str],     // esp-hal / esp-backtrace / panic-semihosting …
    nros_platform_feature: &'static str,     // platform-bare-metal / platform-nuttx / …
    target: &'static str,                    // rustc triple
    toolchain: Toolchain,                    // Stable | Nightly(&str) | Esp
    rustflags: &'static [&'static str],
    build_std: &'static [&'static str],      // [] ⇒ prebuilt target, no -Z build-std
    patches: &'static [(&'static str, &'static str)], // crate → workspace-relative path
    link_kind: LinkKind,                     // None | NuttxStaging | …
    entry_kind: EntryKind,
}

// HostedMain  — Rust `fn main` (posix).
// BoardRun    — Rust `<board>::run(cfg, closure)` (RTOS/esp/bare-metal).
// BoardRunC   — C/C++ RTOS app entry calls `nros_board_run` (173.4).
// ZephyrStaticlib — Rust staticlib + CMake `rust_cargo_application()`.
enum EntryKind { HostedMain, BoardRun, BoardRunC, ZephyrStaticlib }
enum Toolchain { Stable, Nightly(&'static str), Esp }
enum LinkKind { None, NuttxStaging }

fn profile(board: &str, target: &str) -> Option<PlatformProfile> { /* table */ }
```

- `render_cargo_config`, `render_rust_toolchain`, `render_platform_dependencies`,
  `render_platform_link_directives`, `generated_default_features` all
  become thin readers of `PlatformProfile`.
- `main.rs.jinja` branches only on `EntryKind` (3 shapes), not on N
  per-platform `#[cfg(feature = "platform-X")]` blocks. `EntryKind::BoardRun`
  emits the uniform `<board>::run(cfg, closure)` — valid because 173.1 made
  every board expose it.
- `LinkKind::NuttxStaging` keeps the one piece of genuinely dynamic build.rs
  logic (preprocess `dramboot.ld`, group-link the 14 staging archives);
  everything else is static data.

### 173.3 — drift gate

Mirror `check-platform-abi-mirror`: a test asserting every
`PlatformProfile` row names a board crate that exists AND implements
`Board` (compile-time check via a generated `const _: fn() = || { … }`
witness, or a runtime path-existence + `cargo metadata` check). Catches
"added a profile row, forgot the board impl" and vice-versa.

### 173.4 — board C ABI (`nros/board.h` + `nros_board_export!`)

The features layer crosses into C/C++ via `nros/platform.h`; the
workflow layer must do the same so the *one* `Board` impl serves Rust
and C/C++ apps alike. Mirror the platform pattern exactly.

Header (`nros-board-cffi/include/nros/board.h`, new):

```c
typedef struct {
    const char *zenoh_locator;   // nullptr ⇒ board default
    uint32_t    domain_id;
} nros_board_config_t;

/* User application body. Returns 0 on success, non-zero on error.
   `user` is an opaque pointer the caller threads through. */
typedef int32_t (*nros_board_app_fn)(const nros_board_config_t *cfg, void *user);

/* Init hardware (board::init_hardware), run `app`, then exit per the
   board's BoardExit. Never returns on bare-metal/RTOS targets; on
   hosted boards returns the app's status. Symmetric with the Rust
   `nros_board_common::run`. */
void nros_board_run(const nros_board_config_t *cfg,
                    nros_board_app_fn app, void *user);
```

Rust side — an export macro that monomorphises the C shim over a
`Board` impl, exactly like `nros_platform_export!`:

```rust
// nros-board-cffi
#[macro_export]
macro_rules! nros_board_export {
    ($board:ty) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_board_run(
            cfg: *const $crate::NrosBoardConfig,
            app: $crate::NrosBoardAppFn,
            user: *mut core::ffi::c_void,
        ) {
            // marshal cfg → <$board as Board>::Config, then call the
            // common generic `run::<$board>(config, |c| app(c, user))`.
        }
    };
}
```

- A `check-board-abi-mirror` gate (sibling of the platform one) keeps
  `nros/board.h` ⟷ the macro emission in lock-step.
- **Scope:** C/C++ targets the *hosted-RTOS* entry kinds only
  (FreeRTOS/NuttX/Zephyr/ThreadX/posix). Bare-metal C/C++ (esp32,
  stm32f4, mps2 bare-metal) stays out of scope — `nros-c`/`nros-cpp`
  assume hosted startup/heap/libc per the Phase 118 coverage matrix.
  `EntryKind` for those C/C++ cells is `BoardRunC` (the RTOS app entry
  calls `nros_board_run`); the existing 5 deliberately-empty bare-metal
  C/C++ cells stay empty.
- The orchestration generator's *mixed-language* mode (Phase 126 M6 components)
  is unaffected: C/C++ **components** are static archives linked into
  the Rust generated package, whose entry stays Rust `run::<B>`. The
  board C ABI is for **standalone** C/C++ apps (the
  `examples/<rtos>/{c,cpp}/` tree), not the generator's entry.

### 173.5 — transport ⟷ RMW binding config (NOT a new peripheral framework)

**Don't reinvent the peripheral layer.** The Rust embedded ecosystem
already standardises it, and nano-ros should consume those traits in
its driver crates rather than define a competing `Peripheral` trait:

- **`embedded-hal`** — the de-facto SPI / I2C / GPIO / UART peripheral
  traits; vendor HALs (esp-hal, stm32-hal, embassy-stm32) impl them.
- **`embedded-nal`** (+ `embedded-nal-async`) — `TcpClientStack` /
  `UdpClientStack` network abstraction.
- **`embedded-io`** — `Read`/`Write`/`Seek` for byte streams (serial).

A peripheral *driver* (LoRa radio, IMU, a second NIC, a CAN
controller) is an `embedded-hal`/`embedded-io` consumer authored
**outside** nano-ros's concern. nano-ros's transport-bridge crates
(`packages/drivers/*` — `lan9118-smoltcp`, `openeth-smoltcp`,
`stm32f4-usart`, `nros-smoltcp`) should, where practical, sit on top
of `embedded-nal`/`embedded-io` so any conformant driver drops in.
**173 does NOT add a `Peripheral`/`BaseBoard` trait** — that earlier
sketch was a reinvention; it's dropped.

**What nano-ros config actually owns: transport ⟷ RMW cooperation.**
The board layer *already* does compile-time transport selection — the
`ethernet` / `serial` / `wifi` Cargo features on board crates, with
`#[cfg(feature = …)]` `Config` fields (`ip_addr`, `baudrate`,
`zenoh_locator`). The nano-ros config surface is the *binding*:

- **which transport(s)** a build uses (ethernet / serial / CAN), and
- **which RMW rides each transport**, and
- for **bridge mode**, the 2+ (transport, RMW) pairs and their topology.

That binding is the nano-ros-specific decision. It maps onto the
existing `nros-bridge` `Executor::open_multi(&[SessionSpec])` surface
(Phase 128.F) — a `SessionSpec` already names an RMW + a locator;
extend the orchestration `nros.toml` / `nros-plan.json` to declare the
(transport, RMW) pairs the generator turns into `SessionSpec`s + the
matching board transport features.

**Hardware params stay in the platform's native config — not a
nano-ros invention.** IP address, serial baudrate, pin mux, CAN
bitrate belong to the board/RTOS config the platform already has:

- bare-metal / esp-hal boards: `Config::from_toml` (the existing
  `config.toml` with `ip_addr` / `baudrate` / `zenoh_locator` fields),
- RTOS targets: the RTOS's own Kconfig / devicetree (Zephyr),
  `defconfig` (NuttX), FreeRTOSConfig.h — already where these live.

nano-ros does not duplicate these knobs; it references the resulting
locator string (`tcp/10.0.0.1:7448`, `serial/UART_0#baudrate=115200`),
which already encodes transport + params.

### 173.6 — UX: default-first, like standard ROS

Standard ROS 2: users run with defaults; customise a backend via one
env var (`CYCLONEDDS_URI`, `ZENOH_CONFIG`/`ZENOH_LOCATOR`). nano-ros
shifts the *transport + RMW* choice to **compile time** (it must — a
no_std MCU build can't carry every backend), but the UX target is the
same default-first shape:

- **Default build just works.** A board's default transport feature
  (`ethernet` on most) + the single linked RMW → zero-config boot. No
  `nros.toml` required for the common single-transport case.
- **Customisation is one declarative surface.** When a user needs a
  non-default transport, a second transport (bridge), or a specific
  RMW, they say so once in `nros.toml` (the orchestration config) —
  *transport + RMW only*, not hardware params. The generator turns
  that into board features + `SessionSpec`s.
- **Runtime knobs stay runtime.** Locator / IP / domain id remain
  env-overridable (`ZENOH_LOCATOR`, `ROS_DOMAIN_ID`) exactly like
  stock ROS, read through `ExecutorConfig::from_env` on hosted targets
  and `Config::from_toml` on MCU targets. Compile-time config picks the
  *shape*; runtime config tunes the *values*.

This keeps the nano-ros-specific cognitive load to one question —
"which transport(s) talk which RMW?" — and delegates everything else
to mechanisms users (or the RTOS) already know.

## Acceptance criteria

- [ ] One `pub fn run<B: Board, …>` in `nros-board-common`; the three
      family-crate `run`/`run_generic` declarations deleted; esp32 +
      bare-metal route through it.
- [ ] Every board crate exposes `<board>::run(cfg, closure) -> !` with
      the identical signature.
- [ ] Generator's six per-platform functions collapse to
      `PlatformProfile` lookups + a 3-arm `EntryKind` match in
      `main.rs.jinja`.
- [ ] Adding **esp32-s3** to the generator is a single `PlatformProfile`
      row + `impl Board for Esp32S3` + the (genuinely new) board/platform
      crate — **zero** edits to the six former match-arm functions.
- [ ] `orchestration_e2e` suite stays green across all existing platforms
      (posix / freertos / nuttx / zephyr / threadx / esp32-c3 / bare-metal).
- [ ] Drift gate fails when a `PlatformProfile` row lacks a `Board` impl.
- [ ] `nros/board.h` + `nros_board_export!` land; a standalone C app and
      a standalone C++ app on one hosted RTOS (e.g. NuttX or Zephyr) boot
      through `nros_board_run` against the same `Board` impl the Rust
      path uses. `check-board-abi-mirror` keeps header ⟷ macro in sync.
- [ ] Default build (board default transport + single RMW) boots with
      **no `nros.toml`** — the zero-config common case.
- [ ] `nros.toml` declares a (transport, RMW) binding; the generator
      turns it into the board transport feature(s) + `SessionSpec`(s).
      A bridge build with 2 (transport, RMW) pairs opens via
      `Executor::open_multi`.
- [ ] At least one transport-bridge driver crate (`*-smoltcp`) is
      reworked to sit on `embedded-nal` (or documented why it can't),
      proving the "consume the ecosystem, don't reinvent" direction.
- [ ] Hardware params (IP / baudrate) are sourced from board
      `config.toml` / RTOS Kconfig — nano-ros adds **no** new param
      knob; it only consumes the resulting locator string.
## Notes

- The features layer (`nros-platform-api` + export macros + cffi ABI +
  `check-platform-abi-mirror`) is **not** touched — it's the template
  this phase copies, not a thing to change.
- `EntryKind` is deliberately closed at ~3 today. A brand-new RTOS with
  an unseen boot convention adds a variant + one `main.rs.jinja` branch —
  that's the irreducible code cost, and it's bounded.
- `BoardConfig` (`nros_platform::board::BoardConfig`, `zenoh_locator()` /
  `domain_id()`) already exists; 173.1 just makes the generic `run` +
  generated `main.rs` consume it instead of poking board-specific fields.
- **Symmetry is the through-line.** The one new ABI copies the platform
  layer's proven shape: a C header (`nros/board.h`) + an export macro
  (`nros_board_export!`) + a drift gate (`check-board-abi-mirror`),
  exactly as `nros/platform.h` + `nros_platform_export!` +
  `check-platform-abi-mirror`. No new cross-language mechanism is
  invented.
- **No competing peripheral trait.** The earlier `Peripheral` /
  `BaseBoard` sketch is dropped — `embedded-hal` / `embedded-nal` /
  `embedded-io` own that surface. nano-ros consumes them in its driver
  crates; it does not redefine them.
- **Layer roles after 173:**
  - *Peripheral driver* — an `embedded-hal`/`embedded-io`/`embedded-nal`
    consumer (vendor HAL or community crate). Outside nano-ros's trait
    surface entirely.
  - *Transport-bridge crate* (`packages/drivers/*-smoltcp`) — adapts a
    transport to the RMW; should ride `embedded-nal`/`embedded-io` where
    practical.
  - *Platform crate* — system features (clock/net/timer/log) via
    capability traits → `nros/platform.h` ABI. (unchanged)
  - *Board crate* — chip boot (`init_hardware`) + the compile-time
    transport feature set; drives the workflow via the single
    `run<B: Board>` / `nros_board_run`. Hardware params via its own
    `config.toml` / RTOS Kconfig.
  - *Generator* — `PlatformProfile` data + `EntryKind`; reads the
    `nros.toml` transport⟷RMW binding into board features +
    `SessionSpec`s; emits Rust or C/C++ entry funnelling into the
    board ABI.
