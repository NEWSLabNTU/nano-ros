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
2. **Configuration in files, not in code.** Nothing hardware- or
   RMW-specific is hardcoded in hand-written code or hand-edited Cargo
   manifests. Transport choice (Ethernet / serial / CAN), hardware
   params (IP, baudrate), RMW choice, and locator are declared once in
   a config file (`nros.toml`); `nros build` generates every downstream
   artifact from it — board Cargo features, `Config` values, RMW deps +
   `SessionSpec`s, and any RTOS-native config fragment. Changing
   "ethernet → serial" or "zenoh → cyclonedds" is a config edit, never
   a code/manifest edit. Bridge mode = 2+ (transport, RMW) entries.
3. **Don't reinvent peripheral drivers.** `embedded-hal` /
   `embedded-nal` / `embedded-io` already standardise the peripheral +
   network + byte-stream trait surfaces; nano-ros driver crates consume
   those rather than define a competing `Peripheral` trait. nano-ros's
   own surface is the transport ⟷ RMW *binding* (point 2), not a driver
   framework.
4. **Default-first UX.** Standard ROS users run with defaults and tune a
   backend via one config (`CYCLONEDDS_URI`, `ZENOH_LOCATOR`). nano-ros
   keeps that shape: default build boots zero-config; customisation is
   the single `nros.toml`; runtime values still env-overridable.
5. **Bounded essential variation.** Target triple, toolchain, boot entry
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

## Parallel work groups

The seven items factor into **four groups that can progress
concurrently**, mirroring the Phase 126 group structure. Each group
owns distinct crates/files so day-to-day work rarely collides; the only
hard cross-group edges are listed below.

| Group | Items | Owns | Can start | Blocks on |
|---|---|---|---|---|
| **A — Board layer** | 173.1, 173.4 | `nros-board-common`, `nros-board-cffi` (new), every `nros-board-*` | **now** | nothing |
| **B — Generator core** | 173.2, 173.3 | `nros-cli-core/orchestration/generate.rs`, templates, drift-gate script | **now** (scaffolding + non-BoardRun arms) | A for the `EntryKind::BoardRun*` wire-up only |
| **C — Config schema + cooperation** | 173.5, 173.7 | `nros.toml`/`nros-plan.json` schema, `SessionSpec` wiring, RTOS-fragment emitters | **now** (schema design) | B for the emit wiring (PlatformProfile fields) |
| **D — UX + fixtures + gates** | 173.6, tests | examples, `orchestration_e2e` cases, config-diff + grep gates | tracks A/B/C; lands last | A+B+C for end-to-end gates |

**Dependency edges (only these four):**

```
A (173.1 Board::run) ──▶ B (173.2 EntryKind::BoardRun emits <board>::run)
A (173.1 Board trait) ──▶ A (173.4 nros_board_export! wraps it)   [intra-A]
B (173.2 PlatformProfile{net_stack}) ──▶ C (173.7 emit path per NetStack)
A+B+C ──▶ D (end-to-end gates)
```

Everything else is independent. Concretely, day-1 parallel starts:

- **A**: define `Board` super-trait + the one `run<B>` in
  `nros-board-common`; migrate boards. Land `nros/board.h` +
  `nros_board_export!` once the trait exists (same group, no external
  wait).
- **B**: stand up `PlatformProfile` + the enums (`EntryKind`,
  `Toolchain`, `LinkKind`, `NetStack`) and convert the
  `HostedMain` / `ZephyrStaticlib` arms + the cargo-config / toolchain /
  deps readers — none of which need A. Wire the `BoardRun` arm to
  `<board>::run` after A lands the uniform signature.
- **C**: design the `nros.toml` `[[transport]]` schema + the
  `SessionSpec` mapping (pure data + serde) immediately; bolt the
  per-`NetStack` fragment emitters on once B exposes the field.
- **D**: write the config-diff + grep gates against the schema from C
  as soon as it stabilises; flip them on when the generators land.

Merge order: **A and B land first (independently), then C, then D.**
A and B never block each other except at the single `BoardRun`
wire-up, which is a one-line generator change once A is in.

## Work items

### 173.1 — One `Board` trait + one generic `run` — **DONE (Group A)**

*Group A · landed on `phase-173A-board-entry-unification`.*


Added to `nros-board-common` (`src/board_init.rs`):

```rust
/// Everything the generic entry driver needs from a concrete board.
/// Blanket-implemented for any type that already carries the three
/// split traits, so existing `BoardInit + BoardPrint + BoardExit`
/// impls satisfy it for free.
pub trait Board: BoardInit + BoardPrint + BoardExit {}
impl<T: BoardInit + BoardPrint + BoardExit> Board for T {}

/// The one **direct-exec** board entry driver.
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

**Discovered during implementation — `run` is a 2-shape, not 1-shape,
unification.** The original proposal ("six per-platform `run`s collapse
to one") over-promised: FreeRTOS and ThreadX have genuine *kernel-spawn*
semantics — they allocate an app task, hand the closure to it, and start
the scheduler; the closure runs in *task* context and scheduler-start
never returns, so the result is consumed inside the task, not in `run`.
That kernel bring-up is the bounded essential variation (factor 5). So:

- **Direct-exec families** (bare-metal `mps2-an385`, `stm32f4`; esp-hal
  `esp32`) — the closure runs on the boot stack; control falls through
  to `exit_*`. These route through `nros_board_common::run`.
  - `nros-board-mps2-an385` migrated: `Mps2An385` ZST with the three
    impls delegating to the existing free `init_hardware` / `exit_*` /
    `hprintln!`; `run` tail-calls `nros_board_common::run`.
  - `nros-board-esp32-qemu` migrated: `Esp32Qemu` ZST whose
    `init_hardware` folds in `register_log_writer`, `println` →
    `esp_println`, `exit_*` → the ESP32 no-exit spin loop; `run`
    tail-calls the common driver. `logging_smoke_esp32_qemu` test
    re-verified green with the new `nros: application complete` banner.
  - `nros-board-stm32f4` migrated: `Stm32F4` ZST whose `init_hardware`
    takes the PAC + core peripherals internally (dropping the unused
    `SYST`), `println` → `defmt` via `Display2Format`, `exit_*` → the
    `wfi` idle loop; `run` tail-calls the common driver. Checks clean on
    `thumbv7em-none-eabihf`.
- **Kernel-spawn families** (`nros-board-{freertos,threadx}`) — keep
  their own task-spawning `run` body, but converge on the `Board`
  super-trait + `B::Config`. ThreadX `run<B,C,F,E>` collapsed to
  `run<B,F,E>` over `B::Config` (both overlays updated, build clean).
  FreeRTOS keeps its `BoardInit<Config = Config>`-pinned bound (its body
  reads `config.mac/ip` directly, so it's tied to the crate-local
  `Config`; the three-trait bound there is already equivalent to
  `Board`). The *callsite* is unified even though the *body* differs.
- **`nros-board-nuttx` stays on its own `run_generic`** — on inspection
  it is *not* a clean direct-exec fit: overlays impl `BoardInit` only
  (no `BoardPrint`/`BoardExit`), and the body has an essential 5 s
  virtio-net warm-up sleep + `std::process::exit` rather than
  `BoardExit`. That is genuine essential variation (factor 5), same
  category as the kernel-spawn families — left as-is by design.

Net: one direct-exec `run` + the `Board` super-trait; every board (both
shapes) exposes the identical `<board>::run(cfg, closure) -> !`
*callsite*, which is what 173.2's generator depends on.

### 173.2 — `PlatformProfile` descriptor + `EntryKind` in the generator

*Group B · can start now (scaffolding + non-BoardRun arms) · the `EntryKind::BoardRun` arm waits on 173.1.*


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
    net_stack: NetStack,                     // who owns NIC + IP bring-up (173.7)
}

// RtosOwned   — RTOS brings up NIC + IP (Zephyr/NuttX/esp-idf); generator
//               emits an additive RTOS-config fragment from nros.toml.
// NanoRosOwned — board crate owns the stack (smoltcp/lwIP/NetX, bare-metal,
//               esp-hal); nros.toml values flow into the board `Config`.
enum NetStack { RtosOwned, NanoRosOwned }

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

*Group B · after 173.1 + 173.2 (asserts profile ⟷ `Board` impl).*


Mirror `check-platform-abi-mirror`: a test asserting every
`PlatformProfile` row names a board crate that exists AND implements
`Board` (compile-time check via a generated `const _: fn() = || { … }`
witness, or a runtime path-existence + `cargo metadata` check). Catches
"added a profile row, forgot the board impl" and vice-versa.

### 173.4 — board C ABI (`nros/board.h` + `nros_board_export!`) — **DONE (Group A)**

*Group A · landed on `phase-173A-board-entry-unification`.*


The features layer crosses into C/C++ via `nros/platform.h`; the
workflow layer does the same so the *one* `Board` impl serves Rust and
C/C++ apps alike. Mirrors the platform pattern: new `nros-board-cffi`
crate (sibling of `nros-platform-cffi`) with an `unsafe extern "C"`
block + a `nros_board_export!` macro + the `<nros/board.h>` header.

**As-built differs from the proposal in two deliberate ways:**

1. **Opaque `cfg` pointer, not a fixed `nros_board_config_t`.** The
   proposal baked `{ zenoh_locator, domain_id }` into the ABI struct —
   too narrow: boards carry peripheral / IP / baud / RMW-binding config
   (factor 2, Phase 173.5). Instead `cfg` is `const void *`, an opaque
   pointer the board casts back to its concrete `BoardInit::Config`.
   The generic ABI never inspects it; board crates ship their own C
   constructor (`nros_board_<name>_config_from_toml`). This keeps
   hardware config *out* of the generic header — exactly the
   "no hardcoded hardware config in code" principle.
2. **Five primitives, not one.** The ABI mirrors the full `Board`
   surface so the drift gate has real coverage and a C runtime can call
   any single primitive: `nros_board_run` (full entry driver, routed via
   `BoardEntry::run` so it serves both direct-exec and kernel-spawn),
   `nros_board_init_hardware`, `nros_board_println`,
   `nros_board_exit_success`, `nros_board_exit_failure`.

Header (`packages/boards/nros-board-cffi/include/nros/board.h`):

```c
/* Returns 0 on success, non-zero on error. `user` is threaded through. */
typedef int32_t (*nros_board_app_fn)(void *user);

/* Full entry driver (direct-exec OR kernel-spawn, via BoardEntry). noreturn. */
void nros_board_run(const void *cfg, nros_board_app_fn app, void *user);

void    nros_board_init_hardware(const void *cfg);
void    nros_board_println(const uint8_t *msg, size_t len);
void    nros_board_exit_success(void);   /* noreturn */
void    nros_board_exit_failure(void);   /* noreturn */
```

Rust side — `nros_board_export!($ty)` monomorphises all five symbols
over a `BoardEntry` impl (`cfg` read out as `<$ty>::Config`), exactly
like `nros_platform_export!`. Two compile-link tests exercise it:
`tests/export_compiles.rs` (direct-exec board via the `DirectExec`
marker) and `tests/export_kernel_spawn.rs` (a board impl'ing
`BoardEntry` directly).

- `scripts/check-board-abi-mirror.sh` (sibling of the platform gate,
  wired into `just check`) keeps `nros/board.h` ⟷ extern block ⟷ macro
  emission in lock-step. Currently clean: 5 symbols.
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

*Group C · schema design can start now · `SessionSpec`/feature emit waits on 173.2.*


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

**Core principle: nothing hardware- or RMW-specific is hardcoded in
hand-written code or hand-edited Cargo manifests. It is declared once
in a config file, and the generator emits every platform artifact from
it.** Today these decisions are scattered + hardcoded:

- transport choice → a hand-set Cargo `feature` on the board dep,
- IP / baudrate → hand-edited `Config::from_toml` fields or Rust
  defaults baked in the board crate,
- RMW choice → a hand-added Cargo `path` dep + feature,
- locator → a hand-set env var or constant.

A user changing "ethernet → serial" or "zenoh → cyclonedds" or
"192.168.1.5 → DHCP" edits Rust/Cargo today. That is the anti-pattern
this phase removes.

**The nano-ros config file is the single authority.** `nros.toml`
declares the hardware + RMW shape; `nros build` generates everything
downstream — no hand-edited code or manifest:

```toml
# nros.toml — the ONE place a user touches
[[transport]]
kind   = "ethernet"          # ethernet | serial | can
ip     = "10.0.2.50/24"      # or "dhcp"
rmw    = "zenoh"             # which RMW rides this transport
locator = "tcp/10.0.2.2:7447"

[[transport]]                # second entry ⇒ bridge mode
kind    = "serial"
device  = "UART0"
baudrate = 115200
rmw     = "cyclonedds"
```

From this the generator emits:

- the board crate's transport Cargo **features** (`ethernet`, `serial`)
  — user never hand-sets them,
- the per-transport **`Config` values** (ip / baudrate / locator) into
  the generated package (a generated `config.toml` or build.rs consts)
  — user never hand-edits `Config::from_toml`,
- the **RMW backend deps** + the `SessionSpec`s that
  `Executor::open_multi` (Phase 128.F) consumes — user never hand-adds
  an `nros-rmw-*` dep,
- where an RTOS needs its native config touched (Zephyr Kconfig /
  devicetree, NuttX defconfig), the generator emits the **fragment**
  too — the user still only edits `nros.toml`, not the RTOS config.

What stays in the board crate is only **board-intrinsic, non-user
wiring**: the chip's UART pin mux, the fixed MAC base address — facts
about the silicon, not user choices. User-tunable values
(transport/IP/baudrate/RMW/locator) all live in `nros.toml`.

### 173.6 — UX: default-first, like standard ROS

*Group D · tracks A/B/C; the zero-config + single-file behaviours land with 173.5.*


Standard ROS 2: users run with defaults; customise a backend via one
config (`CYCLONEDDS_URI`, `ZENOH_CONFIG`/`ZENOH_LOCATOR`). nano-ros
keeps the same default-first shape, with `nros.toml` as the single
customisation file:

- **Default build just works.** A board's default transport + the
  single linked RMW → zero-config boot. No `nros.toml` required for the
  common single-transport case; the generator falls back to board
  defaults.
- **Customisation is one file.** Transport, hardware params, RMW,
  locator — all in `nros.toml`. No Rust edit, no Cargo-feature edit,
  no RTOS-Kconfig edit. The generator translates.
- **Runtime values still overridable at runtime.** The generated
  config seeds the values, but `ZENOH_LOCATOR` / `ROS_DOMAIN_ID` env
  (hosted) or a runtime `config.toml` (MCU) can still override without
  a rebuild — same as stock ROS env overrides. Config file picks the
  baseline; env tunes per-run.

Net: the nano-ros-specific cognitive load is one file, and that file
never leaks into hand-written code. "Configuration in files, not in
code" is the invariant 173 enforces — the generator is the only thing
that turns config into Cargo features / `Config` values / `SessionSpec`s
/ RTOS fragments.

### 173.7 — how `nros.toml` cooperates with each RTOS (and bare-metal)

*Group C · after 173.2 (`NetStack` field) + 173.5 (the values to emit).*


The hard part of "config in files" is that **each RTOS already owns a
chunk of config** — kernel tick/heap/scheduler, driver enables, the IP
stack. nano-ros must not duplicate or fight that. The rule:

> nano-ros config never sets **kernel** params (tick / heap /
> scheduler). It sets **transport selection + IP / baudrate / RMW /
> locator**. *Where* those land depends on **who owns the network
> stack** on that target.

Three ownership models, captured as a `net_stack` field on
`PlatformProfile` (`RtosOwned` | `NanoRosOwned`):

| Platform | Kernel cfg owner | Net-stack owner | `nros.toml` →  generator emits |
|---|---|---|---|
| **posix** | OS | OS | nothing compile-time; values seed `ExecutorConfig::from_env` at runtime |
| **Zephyr** | Zephyr Kconfig/DT | **Zephyr** (`CONFIG_NET_*`, DT NIC) | a `prj.conf` + DT-overlay **fragment** (net-enable + static-IP/DHCP from `nros.toml`), appended to the board base config — never replacing it |
| **NuttX** | NuttX defconfig | **NuttX** (`CONFIG_NET_*`) | a `defconfig` **fragment** (net + IP) merged into the board defconfig |
| **FreeRTOS** | `FreeRTOSConfig.h` (kernel only) | **nano-ros board** (bundled lwIP) | IP/locator flow into the generated board `Config`; `FreeRTOSConfig.h` untouched |
| **ThreadX** | tx kernel cfg | **nano-ros board** (bundled NetX) | same — values into board `Config`; kernel cfg untouched |
| **esp32 (esp-hal)** | none (bare-metal) | **nano-ros board** (smoltcp) | values straight into board `Config` |
| **bare-metal (mps2 / stm32f4)** | none | **nano-ros board** (smoltcp) | values straight into board `Config` |

Two emit paths, picked by `net_stack`:

- **`RtosOwned`** (Zephyr, NuttX, esp-idf): the RTOS brings up the NIC +
  IP stack; nano-ros rides it via BSD sockets. The generator translates
  the `nros.toml` transport/IP into the RTOS's *own* config language as
  an **additive fragment** (`prj.conf` lines, a DT overlay, a defconfig
  patch) and the user still only edits `nros.toml`. nano-ros emits the
  *net* knobs only; the kernel base config is the RTOS's, untouched.
- **`NanoRosOwned`** (FreeRTOS+lwIP, ThreadX+NetX, all bare-metal +
  esp-hal): nano-ros's board crate owns the stack (smoltcp / lwIP /
  NetX, compiled by the board build.rs). The `nros.toml` values flow
  straight into the generated board `Config` — no RTOS config touched
  because the kernel config there is kernel-only (no net section).

**Bare-metal is the simplest case, not a special one.** No RTOS, no
kernel config, no net-stack owner but nano-ros itself → `nros.toml` is
the *only* config and feeds the board `Config` directly. It's just
`NanoRosOwned` with an empty kernel-config side.

**Serial / CAN follow the same split.** Serial baudrate on an
RtosOwned target (NuttX `CONFIG_UART0_BAUD`) → defconfig fragment; on a
NanoRosOwned target (bare-metal `stm32f4-usart`) → board `Config`. CAN
bitrate likewise. The transport *kind* always comes from `nros.toml`;
the *value* lands wherever that platform's stack reads it.

**What nano-ros never emits**, on any target: kernel tick rate, heap
size, scheduler policy, stack sizes, non-net driver enables. Those stay
the RTOS's (or the board crate's intrinsic) concern. If a user needs to
tune them, they edit the RTOS config directly — that's outside the
nano-ros transport⟷RMW surface by design.

## Acceptance criteria

- [x] One direct-exec `pub fn run<B: Board, …>` + the `Board` super-trait
      in `nros-board-common`. (Revised from "all six collapse to one":
      kernel-spawn families FreeRTOS/ThreadX keep their task-spawning
      `run` body but converge on `Board` + `B::Config` — ThreadX's
      redundant `C` param collapsed. Direct-exec `mps2-an385` + `esp32`
      migrated through the common `run`; `stm32f4` is a mechanical
      follow-up. `nuttx` stays on `run_generic` by design — `BoardInit`-
      only + 5 s warm-up + `process::exit` is essential variation.)
- [ ] Every board crate exposes `<board>::run(cfg, closure) -> !` with
      the identical *callsite* signature (body differs by family).
- [ ] Generator's six per-platform functions collapse to
      `PlatformProfile` lookups + a 3-arm `EntryKind` match in
      `main.rs.jinja`.
- [ ] Adding **esp32-s3** to the generator is a single `PlatformProfile`
      row + `impl Board for Esp32S3` + the (genuinely new) board/platform
      crate — **zero** edits to the six former match-arm functions.
- [ ] `orchestration_e2e` suite stays green across all existing platforms
      (posix / freertos / nuttx / zephyr / threadx / esp32-c3 / bare-metal).
- [x] Drift gate fails when a `PlatformProfile` row lacks a `Board` impl.
      (`check-profile-board-mirror.sh` now asserts each concrete board
      crate impls `BoardInit`+`BoardPrint`+`BoardExit`; verified red when
      an impl is removed.)
- [x] `nros/board.h` + `nros_board_export!` land in the new
      `nros-board-cffi` crate; `check-board-abi-mirror` keeps header ⟷
      extern block ⟷ macro in sync (clean: 5 symbols), wired into
      `just check`. Macro proven via `tests/export_compiles.rs`.
- [x] C-consumer ABI proof landed: `tests/board_consumer.c` includes
      `<nros/board.h>`, defines an `nros_board_app_fn`, and calls
      `nros_board_run` + each primitive; `tests/c_abi.rs` compiles it
      under `-Werror -std=c11`. Pairs with `export_compiles.rs` (Rust
      *emits*) to cover both ABI directions.
- [x] **Kernel-spawn design gap closed.** The earlier limitation (macro
      `nros_board_run` was direct-exec only) is fixed: `nros-board-common`
      now has a `BoardEntry: Board` trait abstracting the full
      boot→app→exit flow, plus a `DirectExec` marker whose blanket impl
      routes direct-exec boards through `nros_board_common::run`.
      Kernel-spawn boards impl `BoardEntry` directly (delegating to their
      family `run`). The macro's `nros_board_run` now calls
      `<B as BoardEntry>::run` — family-agnostic. Proven by two test
      binaries: `export_compiles.rs` (direct-exec via `DirectExec`) and
      `export_kernel_spawn.rs` (custom `BoardEntry`). A full hosted-RTOS
      C app that links + boots still needs a board crate to *invoke* the
      macro + an example project (Group D), but the ABI no longer blocks
      it.
- [x] Default build (board default transport + single RMW) boots with
      **no `nros.toml`** — zero-config common case (no `[[transport]]` ⇒
      board defaults; `orchestration_e2e` 12/12 across all platforms).
- [x] `nros.toml` is the single authority: declaring transport + IP +
      baudrate + RMW + locator there, the generator emits the board
      Cargo feature(s), the `Config` values (locator const), the RMW
      dep(s), and the `SessionSpec`(s). No hand-edited Rust / Cargo
      feature / RTOS Kconfig needed (Group C 173.5/173.7; tests below).
- [x] Changing one `nros.toml` line (`ethernet`→`serial`) re-generates a
      working build with zero code/manifest edits — verified by
      `one_transport_line_change_reflows_only_the_board_feature` (diffs
      two generated manifests; only the transport feature token differs).
- [x] A bridge `nros.toml` with 2 (transport, RMW) entries generates a
      package that opens both via `Executor::open_multi`
      (`bridge_two_transports_emit_open_multi_and_session_specs`).
- [x] Grep gate: generated packages contain no hand-authored
      transport/IP/baudrate/RMW constants — every such value traces to
      `nros.toml` or a board-intrinsic default (which lives in the board
      crate's `Config::default`, not the generated package). Verified by
      `zero_config_package_hardcodes_no_network_constants`: a no-transport
      generated `main.rs`/`build.rs` has no IPv4 literal + no
      `tcp/`/`serial/` locator.
- [ ] At least one transport-bridge driver crate (`*-smoltcp`) is
      reworked to sit on `embedded-nal` (or documented why it can't),
      proving the "consume the ecosystem, don't reinvent" direction.
- [x] `NetStack::RtosOwned` path verified (Zephyr): `nros.toml` IP +
      transport produce an additive `prj.conf` net fragment with the
      configured IP, and the board's base kernel config is unmodified
      (`generator_emits_no_kernel_params_in_net_fragment` + the
      `zephyr_fragment_*` unit tests). A booting hardware run is still
      open.
- [x] `NetStack::NanoRosOwned` path: `nros.toml` IP/baudrate land in the
      generated board `Config` via the `BoardTransportConfig` trait
      (`set_ipv4` / `set_baudrate`, impl'd by mps2-an385 / esp32 / stm32 /
      freertos / threadx-{linux,riscv64}). The generator emits
      `apply_transport_config` (gated NanoRosOwned + board entry +
      static-ip/baud) and the board entry calls it on `Config::default()`
      before `run`; no RTOS kernel config is emitted. Verified by the
      extended `declared_serial_transport_selects_board_feature` test +
      `orchestration_e2e` 12/12. A booting hardware run is still open.
- [x] Negative gate: the generator never emits kernel params (tick /
      heap / scheduler / stack size / non-net driver enables) — the
      Zephyr net fragment is asserted net-only by
      `generator_emits_no_kernel_params_in_net_fragment`.

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
