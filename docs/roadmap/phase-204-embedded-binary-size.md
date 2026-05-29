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
- [ ] Build + measure a `stm32f4` (or qemu-bare-metal) talker over **serial**
      (`zpico-serial` / zenoh-pico serial / XRCE serial) — no smoltcp. Expect
      ~24 KB text + ~50 KB bss dropped (the IP stack + socket buffers). This is the
      configuration that matches micro-ROS's default and is the honest size story.
- [ ] Document the serial number alongside the ethernet one in the book; make
      serial the recommended size-critical transport.
- [ ] **Acceptance:** a measured serial talker, flash + RAM, in the book.

### 204.2 — Right-size the smoltcp socket pool
- [ ] `MAX_UDP_SOCKETS` defaults to **4**, but a zenoh/XRCE client needs **0..=1**
      (RTPS/Cyclone needs 3). The 4× 8 KB UDP socket buffers (32 KB) + the TCP
      buffer count are oversized for brokered clients — drop to what the active RMW
      needs (a backend-derived default, not a fixed 4).
- [ ] **Acceptance:** zenoh/XRCE bare-metal `.bss` drops by ~24–36 KB; the smoltcp
      multicast/socket tests still pass.

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

### 204.8 — `--gc-sections` on the Rust embedded link path (biggest cross-cut)
- [ ] Add `-C link-arg=-Wl,--gc-sections` to embedded Rust targets' link args
      (`.cargo/config.toml` per embedded triple, or a shared rustflags). C deps are
      already `-ffunction-sections -fdata-sections` (manifest/board build.rs) but
      the cargo/rustc final link never `--gc-sections` — only the CMake-board path
      does. This dead-strips the always-compiled-unreferenced C (204.7's IP link,
      vendor TUs).
- [ ] **Acceptance:** measured `.text` drop on a serial-only + on an ethernet build.

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
