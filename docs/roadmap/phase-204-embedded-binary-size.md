# Phase 204 — Embedded binary-size reduction

**Goal.** Shrink nano-ros embedded flash + RAM toward the micro-ROS footprint
(perf too), and document an apples-to-apples size story. Three fronts: (a) the
**networking stack** we statically link on the ethernet path (the dominant cost;
the ROS/RMW layer is already competitive); (b) **transport optionality** — make
the IP stack droppable (serial-only) + reuse the RTOS stack, mostly already built
(see below); (c) **compiler + linker options across every layer** (core Rust, RMW
C, platform C/C++) — several size/perf levers are not pulled today.

**Status.** Proposed (2026-05-29). Investigation done (figures below); work items
ranked by impact, not yet started.

**Priority.** P2 — footprint matters for the smallest MCUs + for the micro-ROS
comparison, but no capability is blocked.

## Investigation — measured footprint

Built release ELFs, `size`/`nm` on the artifacts:

| Build | text | data | bss | notes |
|---|---|---|---|---|
| `stm32f4/rust/talker` (zenoh, **bare-metal ethernet/smoltcp**) | **79.7 KB** | 16.4 KB | **100.9 KB** | a real embedded ROS node |
| `qemu-arm-freertos/rust/talker` (zenoh, **lwIP**) | **240.6 KB** | 10.7 KB | **3.3 MB** | FreeRTOS kernel + lwIP + heap pools |
| `nros-bench/wcet-cycles-qemu` (executor only, no net/RMW) | **16.9 KB** | 0 | 12 B | the nros-core floor |

**`.text` breakdown of the 79.7 KB stm32f4 talker** (`nm --size-sort`):
- **smoltcp (full Rust TCP/IP stack) ≈ 24 KB** — `Interface::poll` 13.2 KB, `dispatch_ip` 4 KB, `smoltcp_network_poll` 4.3 KB, wire/tcp/checksum/neighbor ~3 KB.
- **`nros_board_common::board_init::run` 14 KB** — board + clock + net bringup.
- **fmt / panic / Debug machinery ≈ 5 KB** — `core::fmt::pad`, `<&T as Debug>::fmt`, `char::escape_debug`, `str::slice_error_fail_rt` (pulled in by `defmt`/`panic-probe` + `Debug`).
- **HAL + compiler-builtins ≈ 6 KB** — `stm32f4xx_hal` rcc/pll, mem/div intrinsics.
- **nros core + zenoh-pico: modest** — `timer_try_process` 840 B etc. **The ROS/RMW layer is *not* the bloat.**

**`.bss` of the stm32f4 talker (~100 KB):**
- `HEAP` **33.3 KB** (static heap; zenoh-pico uses `alloc`).
- smoltcp socket buffers **~50 KB** — `SOCKET_{RX,TX}_BUFFERS` + `UDP_SOCKET_{RX,TX}_BUFFERS` 8 KB each (×4 = 32 KB) + `TCP_{RX,TX}_BUFFER_0..3` 2 KB each (×8 = 16 KB).

## Investigation — micro-ROS comparison

- micro-ROS's **default embedded transport is serial/UART with NO IP stack** — there is no smoltcp-equivalent cost at all. Its UDP transport **reuses the RTOS's lwIP/Zephyr-net** (a thin socket-glue file); micro-ROS never bundles its own stack.
- **Micro XRCE-DDS Client: < 75 KB flash, ~3 KB RAM** (pub+sub, 512 B msgs). Stays small by: **static-only memory** (`RMW_UXRCE_ALLOW_DYNAMIC_ALLOCATIONS=OFF`), **compile-time-sized fixed pools** (`RMW_UXRCE_MAX_{NODES,PUBLISHERS,…}`, `UCLIENT_*_TRANSPORT_MTU`), and **offloading DDS discovery + RTPS to the host Agent** (no DDS stack on-device). Build is `-Os` + a `colcon.meta` that tunes the pools.
- **zenoh-pico** (our default RMW): stripped ~15 KB, full ~50–80 KB flash + **~12 KB RAM** — flash comparable to XRCE, but **~4× the RAM** (peer middleware, not a thin client).

**Conclusion.** nano-ros is *not* bloated in the ROS/RMW layer (core floor 17 KB; zenoh-pico ≈ micro-ROS XRCE flash). The gap is structural: (1) our bare-metal-ethernet path **statically links smoltcp + ~80 KB of buffers/heap** where micro-ROS's default serial path links no IP stack; (2) **zenoh-pico's ~12 KB RAM vs XRCE's ~3 KB**; (3) our embedded example is tuned for **speed** (`opt-level=3`), not size; (4) we ship 4× the socket buffers a brokered client needs.

## Work Items (ranked by impact)

### 204.1 — Serial-transport size baseline (biggest lever)

**Measured (2026-05-30, qemu-arm-baremetal / mps2-an385 / thumbv7m, release,
no extra flags) — the naive premise does NOT hold yet:**

| talker | text | data | bss |
|---|---|---|---|
| `serial-talker` (board `serial,rmw-zenoh`, no smoltcp dep) | **143.8 KB** | 66.4 KB | **108.7 KB** |
| `talker` (ethernet/smoltcp) | 85.8 KB | 69.6 KB | 69.7 KB |

Serial is **larger**, not smaller. Three confounds, all already-known phase items:
- **smoltcp is still linked in the serial build** — `nm` shows **45 smoltcp
  symbols** (vs 136 ethernet), i.e. the IP link/stack is pulled in even on a
  `default-features=false, features=["serial"]` board build. This is the
  **204.7** gap (IP link compiled unconditionally) reaching the smoltcp Rust side
  too, and with **no `--gc-sections`** on the qemu examples (**204.8** only landed
  on stm32f4) the dead code is not stripped.
- The serial example builds the heavier **`rmw-cffi` multi-backend register path**
  (`nros_rmw_cffi_register_named` alone is **5.2 KB** `.text`), where the ethernet
  `talker` uses a leaner direct-zenoh register — so the two are not an
  apples-to-apples transport swap.
- ⚠️ **gc-sections measurement gotcha:** setting `RUSTFLAGS=-Clink-arg=--gc-sections`
  in the env **replaces** the example's `.cargo/config.toml` `[target] rustflags`
  (cargo does not merge), dropping `-Tlink.x` → the linker gc's the vector table +
  entry → a **0-size broken ELF**. Append via `cargo rustc -- -Clink-arg=…` or edit
  the config; never via env on these examples.

**Conclusion / re-sequencing.** A clean serial baseline is **blocked on 204.7 +
204.8** (confirms this phase's own Note). The honest "serial sheds the IP stack"
number can only be measured once (a) the serial board build stops linking smoltcp
/ the zenoh IP-link C (204.7) and (b) `--gc-sections` strips the residue (204.8),
and ideally (c) the serial example uses the same register path as the ethernet
one. Until then "serial = smaller" is not true on the shipped examples.

- [x] **Measured the current (pre-204.7/.8) serial vs ethernet baseline** — serial
      is larger; root-caused to 204.7 (smoltcp/IP-link still linked) + 204.8 (no
      gc) + the cffi register-path confound (above).
- [ ] Re-measure the serial floor **after** 204.7 + 204.8 land on the serial
      example; expect the predicted ~24 KB text + IP-buffer bss drop then.
- [ ] Document the (post-204.7/.8) serial number alongside ethernet in the book;
      make serial the recommended size-critical transport.
- [ ] **Acceptance:** a measured serial talker, flash + RAM, in the book.

### 204.2 — Right-size the smoltcp socket pool — [~] landed on stm32f4 talker
- [x] **Proven (2026-05-29).** The socket counts are env-tunable
      (`NROS_SMOLTCP_MAX_SOCKETS` default 4, `NROS_SMOLTCP_MAX_UDP_SOCKETS` default
      2; buffers = `SOCKET_BUFFER_SIZE` 2048 × {RX,TX} × count on both the bridge +
      smoltcp sides). A zenoh-pico `tcp/`-locator client needs **1 TCP + ≤1 UDP**
      (defaults are sized for DDS RTPS, 3 UDP/participant). Set
      `NROS_SMOLTCP_MAX_SOCKETS=1` + `NROS_SMOLTCP_MAX_UDP_SOCKETS=1` in
      `examples/stm32f4/rust/talker/.cargo/config.toml` → **bss 100.9 → 51.8 KB
      (−49 KB, −49 %)** + data −2.7 KB.
- [x] **Rolled out (2026-05-30).** `NROS_SMOLTCP_MAX_SOCKETS=1 /
      MAX_UDP_SOCKETS=1` set in **17** smoltcp examples (qemu-arm-baremetal
      non-serial + stm32f4 — zenoh multiplexes pub/sub/service/action over one
      session, so 1 TCP suffices for any entity count). FreeRTOS/ThreadX/esp32 use
      RTOS stacks (no smoltcp) → env is a no-op, left unset. Verified building
      (qemu-arm-baremetal talker bss 20 KB).
- [ ] **Remainder:** make it a **backend-derived default** (the generator/board
      sets it from the active RMW) so RTPS keeps 3 + brokered clients get 1
      automatically, instead of a hand-set env. Smoltcp multicast/socket tests pass.

### 204.3 — Size-tuned embedded release profile
- [ ] The embedded examples override the workspace `opt-level="s"` to
      **`opt-level=3`** (speed). Add a size profile (`opt-level="z"`, `lto="fat"`,
      `codegen-units=1`, `panic="abort"`, `strip=true`) and use it for the
      size-critical examples / a `just <plat> build --size` knob. Quantify `z` vs
      `3` on the talker.
- [ ] **Acceptance:** measured text delta for `z` vs `3` documented; a size build
      profile exists.

### 204.4 — Strip fmt / panic machinery
- [ ] ~5 KB is `core::fmt`/`Debug`/`escape_debug`/`slice_error_fail_rt`, pulled in
      by `defmt`/`panic-probe` formatting + `Debug` derives. Offer a
      minimal-panic build: `panic-halt` (no formatting) + `build-std` with
      `panic_immediate_abort` + audit `Debug` in the embedded hot path.
- [ ] **Acceptance:** the fmt/panic text contribution measured before/after.

### 204.5 — Static-heap sizing + XRCE-for-RAM
- [ ] `nros_platform_stm32f4::memory::HEAP` is 33 KB static. Document/tune the heap
      to the RMW's real need (zenoh-pico ~12 KB working set; XRCE ~3 KB). Offer
      XRCE as the RAM-minimal backend (static pools, discovery offloaded to agent —
      the micro-ROS model), measured against zenoh-pico.
- [ ] **Acceptance:** a documented heap-size guide per backend; an XRCE bare-metal
      RAM figure vs zenoh-pico.

### 204.6 — FreeRTOS/lwIP footprint audit
- [ ] The FreeRTOS talker is **240 KB text + 3.3 MB bss** — the bss (lwIP pools +
      FreeRTOS heaps) is suspiciously large. Audit the lwIP/FreeRTOS heap + pool
      config (`lwipopts.h`, `configTOTAL_HEAP_SIZE`) for default-oversize.
- [ ] **Acceptance:** the bss explained + reduced to a documented budget.

## Transport / IP-stack optionality (architecture)

**Most of "make the IP stack optional, switch to serial, reuse the RTOS stack"
already exists** — confirmed by an architecture audit:

- **smoltcp is an optional dep gated behind the `ethernet` feature** on bare-metal
  boards (`nros-board-{stm32f4,mps2-an385}/Cargo.toml`: `ethernet = ["dep:nros-smoltcp",
  "dep:<mac>", "dep:smoltcp"]`, `serial = ["dep:zpico-serial", "dep:<uart>"]`). A
  `default-features = false, features = ["serial"]` build links **no** smoltcp Rust
  driver — real `examples/qemu-arm-baremetal/rust/serial-{talker,listener}` prove it.
- **RTOS-stack reuse is already the default + only path for every hosted RTOS:**
  FreeRTOS→lwIP, Zephyr→`zsock`, ThreadX→NetX, POSIX/ESP-IDF→sockets, each via a
  per-platform `nros-platform-<rtos>/src/net.c` implementing the canonical
  `nros_platform_{tcp,udp}_*` C ABI — none bundle smoltcp. **Bare-metal is the only
  place smoltcp is mandatory** (no RTOS stack to reuse).
- All backends plug behind **one C ABI seam** (`nros-platform-api` traits /
  `platform_net.h`); zenoh-pico's `_z_*` symbols forward to it via the alias TU.
  **Serial is a separate byte-stream link** (`zpico-serial` implements
  `_z_*_serial_*`), not a pseudo-socket; zenoh-pico picks at session-open from the
  locator (`tcp/` `udp4/` `serial/`).

**The gap (work item 204.7):** zenoh-pico's TCP/UDP link C is compiled
**unconditionally** — `nros-board-common/src/policy.rs` `LinkFeatures::from_env()`
hardcodes `tcp/udp = true`, so `Z_FEATURE_LINK_TCP/UDP` are always on even in a
serial-only build (the IP link `.c` is built, just unreferenced). Combined with the
**missing `--gc-sections` on the Rust link path** (204.8), that dead code is *not*
stripped. The Cargo-feature gating model already exists (`raweth`/`tls`/`ivc`/`custom`
flow from `CARGO_FEATURE_*` into the link-feature flags) — only the `tcp/udp`
`from_env()` defaults + a board feature need wiring; the XRCE backend
(`UCLIENT_PROFILE_{UDP,TCP,SERIAL}`) needs the same gate.

### 204.7 — IP stack truly optional (serial-only drops the IP link layer)
- [ ] Drive `LinkFeatures.{tcp,udp_unicast,udp_multicast}` from a cargo feature
      (default on; serial-only board → off) instead of the hardcoded `from_env()`
      `true`, so `Z_FEATURE_LINK_TCP/UDP=0` and zenoh-pico's IP link C is not
      compiled in a serial-only build. Same gate for XRCE's `UCLIENT_PROFILE_*`.
- [ ] Document the existing serial / RTOS-stack-reuse story (above) in the book.
- [ ] **Acceptance:** a serial-only bare-metal build's `.text` shows no zenoh-pico
      TCP/UDP link symbols (vs today's compiled-but-dead); pairs with 204.8.

## Compiler + linker options — cross-layer inventory

Audit of every build layer (Rust profiles + cc-rs C + CMake C/C++ + linker). The
levers below are **not currently pulled**; C deps are largely arch-tuned +
section-split, but the **Rust link path and several vendor-C cc-rs builds are not**.

| Layer | how built | opt | sections | flto/LTO | target-cpu |
|---|---|---|---|---|---|
| Core Rust (`release`) | cargo | `s` | rustc dflt | **off** | **none** |
| Embedded Rust examples (override) | cargo | **`3` (speed)** | rustc dflt | fat | **none** |
| zenoh-pico C (embedded) | cc-rs/manifest | `2` | **yes** | no | per-arch ✓ |
| zenoh-pico C (**POSIX**) | cc-rs | **none** | **none** | no | host |
| Micro-XRCE / micro-CDR C (**all**) | cc-rs | **none** | **none** | no | **none** |
| CycloneDDS C++ + RMW wrapper | CMake `Release` | `3` | **none** | **no IPO** | host |
| lwIP / FreeRTOS / NetX (board) | cc-rs | `2` | yes | no | per-arch ✓ |
| Final link — Rust embedded | rustc | — | — | — | **no `--gc-sections`** |
| Final link — C/C++ board (CMake) | CMake | — | — | — | `--gc-sections` ✓ |

### 204.8 — `--gc-sections` on the Rust embedded link path — [~] landed on stm32f4 talker
- [x] **Proven (2026-05-29).** Added `-C link-arg=--gc-sections` to
      `examples/stm32f4/rust/talker/.cargo/config.toml` → **text 79.7 → 75.6 KB
      (−4.0 KB)**. **Note:** the link uses **`rust-lld` directly** (no gcc driver),
      so the arg is **`--gc-sections`, NOT the `-Wl,`-prefixed driver form**
      (`-Wl,--gc-sections` → `rust-lld: error: unknown argument`). C deps are already
      `-ffunction-sections -fdata-sections` (manifest) + rustc emits per-fn
      sections; `cortex-m-rt`'s `link.x` `KEEP`s the vector table so gc is safe.
- [x] **Rolled out (2026-05-30).** `--gc-sections` added to **all 35** real
      embedded Rust example `.cargo/config.toml` (all rust-lld). Builds verified
      clean on **3 families**: bare-metal (qemu-arm-baremetal, stm32f4), esp32
      (riscv32imc). FreeRTOS/ThreadX/NuttX are SDK-gated (flag applied — standard +
      their `-T*.ld`/`linkall.x` KEEP essentials — but not built locally; CI
      confirms).
- [ ] **Remainder:** `nros new` scaffold default; larger drop expected once 204.7
      (serial-only no IP link C) lands; boot smoke on a target.

### 204.9 — Size-optimize the cc-rs vendor C
- [ ] `Micro-XRCE / micro-CDR` (`nros-rmw-xrce-cffi`, `xrce-sys`) get **no** `-Os`,
      no `-ffunction-sections/-fdata-sections` on any target — add them (micro-ROS
      builds XRCE `-Os`). POSIX zenoh-pico (`zenoh_platforms.toml [platform.posix]`)
      has **no `compile` line** → no opt/sections; add `opt_level + sections`.
- [ ] **Acceptance:** XRCE backend `.text` measured before/after; sections present
      so 204.8 can strip.

### 204.10 — `target-cpu` for embedded Rust
- [ ] Set `-C target-cpu=<core>` (cortex-m3 / cortex-m4 / cortex-r5 / the rv32/rv64
      cores) per embedded triple — Rust currently codegens for the baseline triple
      while the C side is already `-mcpu`-tuned. Perf + some size.
- [ ] **Acceptance:** documented per-triple `target-cpu`; a perf/size delta sample.

### 204.11 — Embedded example release profiles: strip + reconcile
- [ ] Embedded example profiles keep `debug=2` (full debuginfo) + no `strip`; one
      (`qemu-esp32-baremetal/listener`) sets `debug-assertions=true` in release.
      Add `strip=true`, drop debuginfo, fix debug-assertions; reconcile the
      `opt-level=3` (speed) overrides with the workspace `"s"` (204.3) — a single
      size profile vs an explicit speed profile, chosen per example.
- [ ] **Acceptance:** consistent embedded profiles; ELF artifact size + flash text.

### 204.12 — `build-std` + `panic_immediate_abort`
- [ ] esp32 / orin-spe / nuttx already `build-std = ["core","alloc"]` but **without**
      `build-std-features = ["panic_immediate_abort"]`, so core/alloc are rebuilt
      but the panic-fmt machinery (~5 KB, 204.4) is not stripped. Add the feature
      where `build-std` is already on; evaluate enabling `build-std` + the feature
      for the other bare-metal triples.
- [ ] **Acceptance:** fmt/panic `.text` contribution before/after.

### 204.13 — CMake C/C++ (CycloneDDS + examples): size build type + IPO
- [ ] CMake libs build `-DCMAKE_BUILD_TYPE=Release` (`-O3`), never `MinSizeRel`
      (`-Os`); no `CMAKE_INTERPROCEDURAL_OPTIMIZATION`. For size-critical
      consumption offer `MinSizeRel` + IPO/LTO; the CycloneDDS RMW wrapper
      (`packages/dds/nros-rmw-cyclonedds/CMakeLists.txt`) sets no opt/section flags
      (inherits BUILD_TYPE) — add `-ffunction-sections -fdata-sections`.
- [ ] **Acceptance:** Cyclone path size measured `Release` vs `MinSizeRel`+IPO.

### 204.14 — LTO strategy (perf + size), unblock the rust-lld issue
- [ ] Workspace `lto="off"` (cross-crate inlining left on the table) because cross
      `libddsc.a` slim-LTO objects are unlinkable by `rust-lld` (the archived ThreadX
      Cyclone link issue). Study per-target `lto="thin"` where the linker supports
      it (native/host first), gated on the cross-language link constraint.
- [ ] **Acceptance:** a per-target LTO recommendation + a measured perf/size delta
      on at least the native + one embedded target.

## End-user compiler-option UX (204.15)

**Problem.** The size/speed levers (204.3/.8/.9/.10/.12/.13/.14) live in five places —
`Cargo.toml [profile.*]`, `.cargo/config.toml` rustflags, the cc-rs manifest, the
CMake build type, `[unstable] build-std`. An end user today hand-edits all five.
Per-example `nros.toml` carries only *runtime* config ([node]/[[transport]]); the
root deploy `[build]` carries `profile`/`features`/`cfg`. There is **no single
intent knob**, and no fan-out across the Rust/RMW-C/platform-C layers.

**Design — a global intent + per-toolchain-layer overrides, all in `nros.toml`.**
Two tiers: `optimize` sets a coherent baseline across every layer; per-layer
tables (`[build.cargo]` Rust, `[build.cc]` the gcc/clang C+C++ compiler) refine
*one* layer without disturbing the others.

```toml
# root nros.toml (deploy mode) — or [build] in a direct-mode project
[build]
optimize = "size"          # size | speed | balanced (default) | debug — the baseline

# Per-layer overrides (refine one toolchain; merge over the `optimize` baseline):
[build.cargo]              # Rust / rustc / cargo profile
opt_level = "z"            # 0|1|2|3|s|z   lto = off|thin|fat   debug = bool
strip = true               #               codegen_units = N
rustflags = ["-Ctarget-cpu=cortex-m4"]   # appended verbatim

[build.cc]                # the gcc/clang C+C++ compiler — RMW C (zenoh-pico/XRCE),
debug = true               # platform C (net.c/lwIP/NetX), AND cmake-built C/C++
opt_level = "s"            # cflags = ["-fno-plt"]  (appended)
```

`nros build` / `nros deploy` is the **single fan-out point**. `optimize` maps to:

| `optimize` | cargo profile | RUSTFLAGS | cc (RMW/platform C) | CMake (Cyclone/C++) | build-std |
|---|---|---|---|---|---|
| `size` | opt=`z`, lto=fat, cu=1, panic=abort, strip | `-Ctarget-cpu=<core>` + `-Clink-arg=-Wl,--gc-sections` | `-Os -ffunction-sections -fdata-sections` | `MinSizeRel` + IPO | `panic_immediate_abort` |
| `speed` | opt=3, lto=thin/fat, cu=1 | `-Ctarget-cpu=<core>` | `-O3` | `Release` | (target-dep) |
| `balanced` | opt=`s`/2 | `-Ctarget-cpu=<core>` | `-O2` | `RelWithDebInfo` | — |
| `debug` | opt=0/1, debug | — | `-Og -g` | `Debug` | — |

`<core>` derives from the board/triple (closes the Rust target-cpu gap, 204.10).

**Per-layer mechanism (how each override reaches its toolchain):**
- **`[build.cargo]`** → the generated cargo profile fields (`opt-level`/`lto`/`debug`/
  `strip`/`codegen-units`) + appended `RUSTFLAGS`. nros owns the profile it builds
  with, so this is a direct write.
- **`[build.cc]`** → `cc-rs` auto-appends `TARGET_<triple>_CFLAGS` / `CFLAGS`, so
  nros exports those from `[build.cc]` (e.g. `debug=true` → `-g`, `cflags` verbatim)
  — applied to **every** `cc::Build` (zenoh-pico, Micro-XRCE, net.c, lwIP) **without
  touching any build.rs**. The `opt_level` override needs the build scripts to honour
  an `NROS_CC_OPT` env (today the zenoh manifest hardcodes `.opt_level(2)`); a small
  build-script change reads it (204.9 territory). Debug/extra-cflags work **today**
  via the env append.
- **cmake C/C++** (Cyclone, C++ examples) → `[build.cc]` lowers to `-DCMAKE_C_FLAGS`/
  `-DCMAKE_CXX_FLAGS` (+ `-DCMAKE_BUILD_TYPE` from `optimize`, `-DCMAKE_INTERPROCEDURAL_OPTIMIZATION`).

**Precedence (lowest→highest):** `optimize` baseline → `[build.<layer>]` field →
an explicit per-deploy `[deploy.<name>.build…]` override.

**The motivating case — debug symbols on *one* layer.** Debugging a C driver
(e.g. the smoltcp MAC or lwIP glue) while keeping Rust release-stripped:
```toml
[build]
optimize = "size"          # everything opt-z + stripped
[build.cc]
debug = true               # but the C layer keeps -g → gdb the driver; Rust untouched
```
→ C compiled `-Os -g` (symbols), Rust stays `opt-z`/stripped. Symmetric: `[build.cargo]
debug = true` keeps Rust debuginfo while C stays stripped.

**Plain `cargo build` / `cmake` (copy-out example, no `nros`).** `nros new`
scaffolds named cargo profiles (`[profile.size]`/`[profile.speed]`) + a per-target
`.cargo/config.toml` (`target-cpu` + `--gc-sections`), so `cargo build --profile
size` works without `nros`; CMake examples gain `-DNROS_OPTIMIZE=size`. Per-layer
C overrides on the bare path are the standard `CFLAGS`/`<target>_CFLAGS` env (the
same vars nros sets) — documented, not nano-ros-specific.

### 204.15 — `[build].optimize` + per-layer `[build.cargo]`/`[build.cc]` overrides
- [ ] Add `optimize` + the per-layer override tables to the build config schema;
      `nros build`/`deploy` fans `optimize` out (cargo profile + RUSTFLAGS + cc env
      + CMake type + build-std) and merges the per-layer tables over it
      (precedence above). Wire `[build.cc]` debug/cflags via `TARGET_*_CFLAGS` env
      (works without build.rs edits) + `NROS_CC_OPT` for opt-level override.
- [ ] `nros new` scaffolds named `size`/`speed` cargo profiles + a target
      `.cargo/config.toml` so the plain-cargo path honours intent without `nros`.
- [ ] **Acceptance:** (a) `optimize="size"` vs `"speed"` → measurably different
      flash on a hosted + embedded target, no cross-layer hand-edits; (b) the
      motivating case — `optimize="size"` + `[build.cc] debug=true` → C objects
      carry `.debug_*` sections while the Rust crate stays stripped (verified by
      `readelf`/`nm`).

## End-user workflow (simulated)

**Persona A — size-critical STM32F4 over serial.** Wants the smallest flash.
```toml
# my_robot/nros.toml
[build]
optimize  = "size"
[node]
domain_id = 0
[[transport]]
kind    = "serial"          # no IP stack (204.7 drops smoltcp + TCP/UDP link C)
locator = "serial/dev/ttyACM0"
```
```
$ nros build              # one command; fans out:
  · cargo --profile size  → opt-z, lto=fat, panic=abort, strip
  · RUSTFLAGS             → -Ctarget-cpu=cortex-m4 -Clink-arg=-Wl,--gc-sections
  · serial-only           → smoltcp + zenoh IP link C not compiled (204.7) +
                            dead code gc'd (204.8)
  · XRCE/zenoh C          → -Os + sections (204.9)
$ nros build --size-report   # prints text/data/bss + the per-section breakdown
  text 28 KB  data 4 KB  bss 18 KB   (vs 80 KB / 16 KB / 100 KB ethernet+speed)
```

**Persona B — perf-critical native bridge.** Wants throughput, size irrelevant.
```toml
[build]
optimize = "speed"          # opt-3, lto=thin, target-cpu=native
```
`$ nros build` → release-fast profile; no gc-sections size cost paid; C at `-O3`.

**Persona C — default.** No `[build].optimize` → `balanced` (opt-`s`/`2`,
target-cpu, no aggressive strip) — sensible middle, today's behaviour but
target-cpu-tuned.

**Persona D — bare toolchain, no `nros`.** Copies out the example, builds with
plain cargo:
```
$ cargo build --profile size --no-default-features --features serial,rmw-xrce
```
The scaffolded `[profile.size]` + `.cargo/config.toml` (target-cpu + gc-sections)
give the same result without `nros`.

**Persona E — debug one layer, ship the rest small.** Hunting a bug in the C lwIP
glue on an otherwise size-optimized build:
```toml
[build]
optimize = "size"           # opt-z + stripped everywhere
[build.cc]
debug = true                # C layer keeps -g → gdb the glue; Rust stays stripped
```
`$ nros build` → C objects carry `.debug_*`; the Rust crate is opt-z/stripped.
Symmetric `[build.cargo] debug=true` debugs the Rust side instead.

**Escape hatch — power user overrides one layer's lever:**
```toml
[build]
optimize = "size"
[build.cargo]
lto = "off"                 # e.g. to dodge the rust-lld cross-LTO link issue (204.14)
```

## Acceptance (phase)

- [ ] An honest size table in the book: per (transport, backend, platform) flash +
      RAM, with the micro-ROS comparison + the structural explanation (IP stack
      link vs serial/RTOS-stack-reuse).
- [ ] A documented size-minimal recipe (serial + XRCE + size profile + tuned
      pools) and its measured footprint.

## Notes

- Cheapest/biggest wins: **204.8** (`--gc-sections` on the Rust link — strips
  dead code repo-wide, one rustflag) + **204.2** (socket-pool right-sizing, a
  config default) for size/RAM; **204.1** (serial) for the honest comparison.
  204.8 + 204.7 together make a serial-only build actually shed the IP stack.
- Don't chase the ROS/RMW layer — it's already at the micro-ROS XRCE class; the
  cost is networking + buffers + heap + the speed-tuned profile.
- micro-ROS's structural advantages we can't fully match without the same trade:
  offloading discovery to an Agent (XRCE does this; zenoh-pico peer mode doesn't)
  and serial-default. The path to parity is **XRCE + serial + static pools**.
