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

> **Post-Phase-218**: References below to `scripts/install-nros.sh`
> pin bumps (a pre-218 CLI release-pin mechanism) are superseded by the
> in-tree `packages/cli/` sub-workspace — one checkout = one CLI
> version, no pin to bump. Build via `just setup-cli`.

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

### 204.1 — Serial-transport size baseline (biggest lever) — [x] DONE (2026-05-30)

**Pre-fix snapshot (2026-05-30, qemu-arm-baremetal / mps2-an385 / thumbv7m,
release) — the naive "serial is smaller" premise did NOT hold at first** (the
"Resolved" subsection below supersedes this once the register-path confound was
fixed):

| talker | text | data | bss |
|---|---|---|---|
| `serial-talker` (board `serial,rmw-zenoh`, no smoltcp dep) | **143.8 KB** | 66.4 KB | **108.7 KB** |
| `talker` (ethernet/smoltcp) | 85.8 KB | 69.6 KB | 69.7 KB |

Serial was **larger**, not smaller. Three suspected confounds, all known phase items:
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

**Investigation (2026-05-30) — the `register_named` buffer-pinning is real, but
the obvious "drop the call" fix is INVALID on bare-metal.** With 204.7
(`NROS_LINK_IP=0`) + 204.8 (`--gc-sections`) already on the serial examples,
smoltcp dropped from **45 → 5** residual symbols — yet serial was *still* 136.7 KB
text / 75.8 KB bss. The `.text`/`.bss` is dominated by the explicit
`nros_rmw_zenoh::register()` call pulling in the multi-backend
`nros_rmw_cffi_register_named` vtable, which references *every* entity trampoline
(`create_subscriber/service_server/service_client/queryable…`) and pins their
static buffers (`SUBSCRIBER_BUFFERS` 34.5 KB, `g_pending_gets` 16.4 KB,
`SERVICE_BUFFERS` 10.4 KB — none used by a publish-only node) against
`--gc-sections`.

**Why "just remove `register()`" does NOT work** (a tempting trap — it *appears*
to shrink serial-talker to 38 KB text / 4.6 KB bss and an e2e even "passed"):

- On `target_os = "none"` the `linkme` `RMW_INIT_ENTRIES` slice is an **empty
  stub** (Phase 142 dropped bare-metal from linkme because cortex_m_rt's link
  script lacks the `__start_/__stop_` anchors). So the explicit `register()` call
  is the **only** reference keeping the backend linked. Remove it and
  `--gc-sections` strips the **entire** zenoh backend — the "38 KB" binary has
  `zenoh = 0`, `_z_/zp_ = 0` symbols (no middleware at all). `Executor::open` then
  resolves `NoBackend` → `Transport(ConnectionFailed)` (`spin.rs:96` maps every
  non-`Single` resolution to the same error, so the failure is silent).
- The "passing" e2e was a **false positive**: `build_example` only
  `require_prebuilt_binary` (it does **not** rebuild), and the default fixture
  profile is `nros-fast-release` (`NROS_CARGO_PROFILE`). A local `cargo build
  --release` writes `target/.../release/`, a *different* path; the e2e ran the
  **stale `nros-fast-release` binary** (built days earlier, still had `register()`).
  Lesson: to test an embedded example edit, rebuild the fixture under the
  `nros-fast-release` profile, not a plain `cargo build --release`.

**Correct sizes (working binaries with the backend linked):** serial-talker
136.7 KB text / 75.8 KB bss; ethernet talker (after the bug-fix below) similar. The
real shrink requires a **publish-only / lean vtable registration** that installs
only the slots a node uses, not the full `register_named` adapter — tracked as the
new item below, not a one-line delete.

**Latent bug found + fixed.** Two bare-metal publisher examples were **missing the
`register()` call entirely** → `zenoh = 0`, no backend, broken at runtime (they
worked pre-Phase-142 when linkme still covered `target_os = "none"`; 142 silently
broke them, and their e2e is Docker-gated/skipped so it went unnoticed):
`examples/qemu-arm-baremetal/rust/talker` and `examples/stm32f4/rust/talker`. Added
the call (sibling `listener`s already had it); rebuilt → `zenoh = 36` / `15`,
backend present. (`stm32f4/rust/talker-embassy` is a TODO skeleton with all nros
calls commented out — no backend needed, left as-is.)

- [x] **Measured + root-caused** the serial footprint: `register_named`
      vtable-pinning dominates, not smoltcp (204.7 already shed it to 5 symbols).
- [x] **Disproved the "drop `register()`" shortcut** — strips the whole backend on
      bare-metal (linkme stub); the apparent win was a backend-less broken binary +
      a stale-prebuilt false-pass e2e.
- [x] **Fixed the missing-`register()` latent bug** in `qemu-arm-baremetal/rust/talker`
      + `stm32f4/rust/talker`.
- [x] **Re-measured + documented (2026-05-30).** Fresh `nros-fast-release` build
      of `qemu-arm-baremetal/rust/{talker,serial-talker}` on thumbv7m:

      | binary | text | data | bss |
      |---|---|---|---|
      | `talker` (ethernet/smoltcp) | 182 916 | 66 984 | 91 744 |
      | `serial-talker` (no IP stack) | **131 504** | **25 172** | **75 824** |
      | Δ serial vs ethernet | **−51 412 (−28 %)** | **−41 812 (−62 %)** | **−15 920 (−17 %)** |

      The `−42 KB .data` win is the smaller serial `HEAP` (204.5; 24 KB vs 66 KB).
      The `−51 KB .text` is the shed IP link C (204.7) + 204.8 `--gc-sections` +
      204.9 vendor-C `-Os`. The `−16 KB .bss` is the lighter platform impl + the
      backend's smaller working set. Serial is **decisively smaller than ethernet
      on every section** — the structural lever the early "serial is bigger"
      snapshot got wrong (confounds: no `--gc-sections` on qemu yet, the
      `register_named` vtable-pinning above, and a stale-prebuilt false-pass e2e).
      The book's `user-guide/configuration.md` "Measured footprint" table carries
      the full per-platform numbers (qemu-arm-baremetal + stm32f4, `release` + the
      204.3 `size` profile) + a "Size-minimal recipe" (qemu-baremetal serial: the
      smallest measured nano-ros today, **116 KB text / 101 KB RAM**).

The **204.1.L lean-vtable lever was explored and reverted** in this round;
mentioned for the record so the path isn't re-tried without revisiting why. The
attempt feature-gated `nros-rmw-cffi` entity slots (`entity-{subscriber,service-
server,service-client}`, default-on); `VTABLE` selected per-slot via `#[cfg]` so
unused entities collapsed to `RET_UNSUPPORTED` stubs that `--gc-sections` then
swept along with their backend buffers. It worked (lean qemu-bsp-talker: text
185.6→170.0 KB, **bss 91.7→57.1 KB**; serial pair e2e green). But it pushed a
silent-drift burden onto users — add a subscription later, forget the matching
Cargo feature, get a *runtime* `RET_UNSUPPORTED` instead of a compile error. Not
an acceptable safety trade for a size-only win. **The only acceptable revisit is
call-graph-driven, not feature-flag-driven** — lazy `VTABLE` population where
typed `Node::create_*` is the sole edge to each slot's trampolines, so
`--gc-sections` derives the used slots from the user's actual call graph (no
features, no drift). That redesign is **invasive (deferred, not scheduled)** —
the lower-burden levers (204.5 HEAP, buffer right-sizing, 204.9) carry the
shipped size story today.

### 204.2 — Right-size the smoltcp socket pool — [x] DONE (2026-05-30)
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
- [x] **Backend-derived default (2026-05-30) — the env is no longer needed.**
      `nros-smoltcp/build.rs` now defaults the pool to the **brokered minimum
      (1 TCP / 1 UDP)** instead of the old RTPS-worst-case 4/4. This is correct by
      construction: smoltcp is the *bare-metal* transport and every shipped
      bare-metal board is brokered (`rmw-zenoh` marker only — no
      `rmw-cyclonedds`/`rmw-xrce` board feature; bare-metal DDS/RTPS over smoltcp
      is Phase 175.B, deferred). An explicit `NROS_SMOLTCP_MAX_*` env still
      overrides. The RTPS escape hatch is the new **`nros-smoltcp/rtps` cargo
      feature** (off by default): a board that grows a bare-metal DDS path enables
      it and the UDP default jumps to 4 (3/participant + spare) — no env. Verified
      generated constants: default → `MAX_SOCKETS=1, MAX_UDP_SOCKETS=1`; `rtps` →
      `1, 4`.
- [x] **Dropped the now-redundant hand-set env from all 18 example configs**
      (8 stm32f4 + 10 qemu-arm-baremetal); they build env-free and the default
      yields 1/1 (verified `qemu-arm-baremetal/rust/talker` +
      `stm32f4/rust/talker` build clean → `MAX_SOCKETS=1, MAX_UDP_SOCKETS=1`). The
      −49 KB BSS win from the original rollout is now the *default*, not opt-in.

### 204.3 — Size-tuned embedded release profile — [x] DONE (2026-05-30)
- [x] **`[profile.size]`** (`inherits="release"` + `strip=true` + `debug=false`;
      `panic` is already `abort` on the embedded targets). Build with
      `cargo build --profile size`, or fleet-wide via
      `NROS_CARGO_PROFILE=size just <plat> build` (the existing profile env already
      threads `--profile size` through every build recipe — no new just flag
      needed). Rolled into all **20** cortex-m bare-metal examples
      (`examples/qemu-arm-baremetal/rust/*` + `examples/stm32f4/rust/*`).
- [x] **opt-level `"s"`, NOT `"z"` — `z` regresses RAM on smoltcp examples.**
      Measured on `qemu-arm-baremetal/rust/talker` (ethernet/smoltcp): `opt-level="z"`
      shrank `.text` 177.4→163.7 KB but **grew `.bss` 91.7→116.3 KB (+24 KB)** — its
      weaker DCE keeps a non-inlined `nros_smoltcp::get_tcp_buffers` accessor that
      references *all* socket buffers, defeating opt-3's per-socket dead-buffer
      elimination (the unused `TCP_{RX,TX}_BUFFER_1..3` survive gc). `opt-level="s"`
      both shrinks `.text` *more* and preserves the socket DCE (bss unchanged):

      | example (target) | profile | text | bss |
      |---|---|---|---|
      | qemu-arm-baremetal talker (smoltcp) | `release` (3) | 177.4 KB | 91.7 KB |
      | qemu-arm-baremetal talker | `size` (`z`) | 163.7 KB | **116.3 KB** ⚠ |
      | qemu-arm-baremetal talker | `size` (`s`) | **158.3 KB** | 91.7 KB |
      | stm32f4 talker | `release` (3) | 186.9 KB | 123.0 KB |
      | stm32f4 talker | `size` (`s`) | **138.1 KB** | 123.0 KB |

      `s` = **−10.8 %** text (qemu) / **−26 %** text (stm32f4), bss unchanged. Lesson:
      `-Oz` is not strictly smaller — it can disable optimizations the static-buffer
      DCE depends on. The shipped profile uses `s`.
- [x] **Acceptance:** `cargo build --profile size` exists in all 20 examples,
      `NROS_CARGO_PROFILE=size just <plat> build` is the fleet knob, measured text
      delta documented. (`nros new` scaffolding the profile ties into 204.15.)

### 204.4 — Strip fmt / panic machinery — [x] DONE (2026-05-30)
- [x] **`minimal` feature offered** on `examples/stm32f4/rust/talker`: `panic-halt`
      (no panic-message formatting, vs the default `panic-probe` print-defmt) +
      `nros-log` compiled off (`nros-log/max-level-off` → `nros_info!`/`nros_error!`
      become no-ops, so their `core::fmt` + the `{:?}` `Debug` on `PublishError`
      dead-strip). The default `diagnostics` feature keeps the old behaviour.
      Build: `cargo build --release --no-default-features --features stm32f429,minimal`.
- [x] **`panic_immediate_abort` is NOT the lever** (see 204.12): `build-std` is
      inert on the stable toolchain and the fmt is logging-fmt, not panic-fmt.
      Dropping the *logging* + the printing-panic handler is what removes it.
- [x] **defmt stays** — `nros_board_common::board_init` logs via defmt + needs the
      `#[defmt::global_logger]` (defmt-rtt); the example can't shed defmt, so
      `minimal` keeps it. The strip is the panic-message + nros-log path.
- [x] **Measured (stm32f4 talker, release, same config):** **text 186.9 → 159.5 KB
      = −27.4 KB (−14.7 %)**; data/bss ~unchanged. Far larger than the earlier
      ~5 KB guess — panic-probe's print path + the nros-log `core::fmt`/`Debug`
      machinery. Links clean (backend stays via the explicit `register()`);
      runtime is silent-but-functional (no diag output) — verify on hardware
      before defaulting it.
- [x] **Acceptance:** fmt/panic text contribution measured before/after (−27.4 KB).

### 204.5 — Static-heap sizing + XRCE-for-RAM
- [x] **Made the bare-metal static heap env-tunable (2026-05-30).**
      `nros-platform-{mps2-an385,stm32f4}::memory::HEAP` now reads `NROS_HEAP_SIZE`
      (compile-time `option_env!`, decimal bytes) and falls back to the per-board
      default (64 KB / 32 KB) when unset — no regression for examples that don't set
      it. Bad value fails the build (`const fn parse_usize`).
- [x] **Measured (mps2-an385 serial talker, `nros-fast-release`).** `NROS_HEAP_SIZE`
      66 KB default → **24 KB**: `.data` **66.2 → 25.3 KB (−41 KB)**;
      `test_qemu_serial_pubsub_e2e` still `published=1, received=1` (verified down to
      16 KB on the serial light load; 24 KB shipped for fragmentation margin). The
      `serial-{talker,listener}` examples set `NROS_HEAP_SIZE = "24576"`.
- [x] **Per-backend heap guide** in `book/src/user-guide/configuration.md`
      (zenoh-pico TCP ~16 KB peak → 24–32 KB; serial lighter → 16–24 KB; XRCE ~3 KB
      → ~8 KB).
- [→] **XRCE bare-metal RAM figure — moved to Phase 207.** Owning the bare-metal
      XRCE example + the custom-transport surface is its own work item (Rust
      `install_custom_transport` hook in `nros-rmw-xrce-cffi`, per-board UART
      shim, `examples/qemu-arm-baremetal/rust/talker-xrce/`, agent e2e). See
      [Phase 207 — XRCE on bare-metal](phase-207-xrce-bare-metal-example.md); the
      measured figure lands in the book "Measured footprint" table when 207 closes.
- [x] **Acceptance:** per-backend heap guide ✔ + tunable knob ✔ + zenoh-pico
      measured drop ✔. The XRCE-vs-zenoh on-device delta is owned by Phase 207.

### 204.6 — FreeRTOS/lwIP footprint audit — [x] DONE (2026-05-30)
- [x] **bss explained:** the 3.3 MB is almost entirely the FreeRTOS heap —
      `configTOTAL_HEAP_SIZE = 3072*1024` → `heap_4.c`'s static `ucHeap[3 MiB]`
      (confirmed via `nm`: `ucHeap` = 3 MiB). lwIP pools are modest
      (`MEM_SIZE` 32 KB, `PBUF_POOL` 24, MEMP). The 3 MiB was set for the heavy
      **CycloneDDS** discovery boot path, not zenoh.
- [x] **Right-sized, RMW-gated (zero cyclone risk).** `FreeRTOSConfig.h` now takes
      `configTOTAL_HEAP_SIZE` from `NROS_FREERTOS_HEAP_KB` (default **3072**, the
      cyclone-safe value, unchanged). `nros-board-freertos/build.rs` forwards it
      as `-D` to the only TU that sizes `ucHeap`, and — when the `rmw-zenoh`
      feature is active (forwarded from the board, NOT set on cyclone/xrce
      builds) — defaults it to **512 KiB**. An explicit `NROS_FREERTOS_HEAP_KB`
      build-env wins over both, so any example/RMW can tune to its measured
      high-water (`xPortGetMinimumEverFreeHeapSize()`).
- [x] **Measured + verified (qemu MPS2-AN385, zenoh talker):** **bss 3.3 MB →
      662 KB (−80 %)** (`ucHeap` 3 MiB → 512 KiB). 256 KiB `MALLOC FAILED` during
      lwIP init (zenoh-pico working set is small, but lwIP netconns/pbufs/socket
      semaphores + FreeRTOS task stacks also draw on this heap); **512 KiB boots
      + connects + publishes 0..17 cleanly** against zenohd. All six zenoh
      FreeRTOS examples inherit it (same board). cyclone/xrce keep the 3 MiB
      default until separately measured.
- **Files:** `packages/boards/nros-board-mps2-an385-freertos/config/FreeRTOSConfig.h`,
  `packages/boards/nros-board-freertos/build.rs`.
- [x] **Follow-up done (2026-05-30) — cyclone measured + defaulted.** The cyclone
      rust **talker + listener** boot + exchange cleanly at **1 MiB** heap on qemu
      MPS2-AN385 (`ucHeap` 1 MiB, **bss 3.3 MB → 1.1 MB, −67 %**); the rust zenoh
      **action-server** (heaviest rust zenoh example) also confirmed fine at 512
      KiB (boots → "Action server ready" → "Waiting for goals"), so the shipped
      512 default is safe across all rust zenoh examples, not just the talker.
      Cyclone freertos has **no** heavier example (no rust service/action, no C++
      cyclone), so 1 MiB is the measured-safe default. Baked via
      `NROS_FREERTOS_HEAP_KB=1024` in the cyclone block of `just freertos
      build-fixture-extras` — the cmake/corrosion build honors the env (the cargo
      `just` rust path does not, which is why zenoh uses the `rmw-zenoh` feature
      gate instead); scoped so the C++ *zenoh* examples keep the safe 3 MiB
      default (lowering the shared `FreeRTOSConfig.h` default would hit the
      unverified C++ zenoh action/service builds). No `rmw-xrce` freertos example
      exists, so no xrce tuning needed.
- **Files (follow-up):** `just/freertos.just`.

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
- [x] **Mechanism landed + verified (2026-05-30).** `LinkFeatures::from_env`
      (`nros-board-common`) now gates `tcp/udp_unicast/udp_multicast` on a
      **default-ON** `zpico-sys` `link-ip` feature **plus** a per-build env override
      **`NROS_LINK_IP=0`** — chosen over `default-features=false` cargo plumbing
      because that would also drop the staticlib `linkme-register` opt-out and
      regress every IP consumer. Default-on preserves 128.E.1 (verified: ethernet
      `stm32f4`/`qemu-arm-baremetal` talker still `Z_FEATURE_LINK_TCP 1`). The
      bare-metal serial examples set `NROS_LINK_IP=0`
      (`examples/qemu-arm-baremetal/rust/serial-{talker,listener}`): verified
      `Z_FEATURE_LINK_TCP/UDP 0`, `SERIAL 1`, smoltcp symbols **45 → 5** (gc'd by
      204.8), bss **108 → 75 KB (−33 KB)** — the IP stack is shed. (The serial
      example's *absolute* size stays high due to the `rmw-cffi` multi-backend
      register-path confound — a separate 204.1 item, not the transport.)
- [x] **XRCE gate done (2026-05-30).** Embedded XRCE already excludes IP
      (`else` branch — custom transport only). POSIX XRCE now honours `NROS_LINK_IP=0`:
      gates both the `udp_transport{,_posix}.c` source files **and** the
      `UCLIENT_PROFILE_UDP/TCP` defines (gating the define alone left
      `udp_transport.c` compiling → error). Verified `nros-rmw-xrce-cffi` builds
      both default (IP on) + `NROS_LINK_IP=0` (IP off, serial kept).
- [x] **Book write-up done.** `user-guide/configuration.md` "Binary-size knobs"
      documents `NROS_LINK_IP` + `NROS_SMOLTCP_MAX_*` + the serial / RTOS-stack-reuse
      story.
- [x] **Generator auto-sets `NROS_LINK_IP=0` for serial-only builds (nros-cli
      `fb9f241`).** `PlanBuildOptions::drops_ip_link()` is true when every declared
      transport is Serial/CAN (no Ethernet/Wifi); empty transports ⇒ false
      (zero-config keeps the board default). `render_cargo_config` then injects
      `NROS_LINK_IP = "0"` into the generated `.cargo/config.toml` `[env]`
      (merge-or-append, idempotent). Generator-side, not the board descriptor — the
      same board builds either ethernet *or* serial, so only the per-build transport
      choice can decide. So a `nros build` serial project sheds the IP link with no
      hand-set env. (137 nros-cli-core lib tests pass incl. `drops_ip_link` +
      `inject_env_var`.)
- [x] **`nros new` scaffolds the knobs (nros-cli 0.3.6).** Embedded `nros new`
      (baremetal/freertos) now emits a `.cargo/config.toml` (none before) with
      `--gc-sections` + a documented commented serial `NROS_LINK_IP=0` block +
      heap/socket env. Shipped in **nros-cli 0.3.6**, pinned via `install-nros.sh`.
- [x] **Shipped + pinned.** nros-cli **0.3.6** released (`nros-v0.3.6`, host
      binaries built by `release-binary.yml`); `scripts/install-nros.sh`
      `NROS_VERSION` bumped 0.3.5 → 0.3.6. The cffi register-path confound is the
      separate **204.1** item (explored + reverted there), not this transport item.

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

### 204.8 — `--gc-sections` on the Rust embedded link path — [x] DONE (2026-05-30; `nros new` scaffold shipped in nros-cli 0.3.6)
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
- [x] **Quantified the gc contribution (2026-05-30) — target-dependent, often
      ~0.** Toggling `--gc-sections` on `qemu-arm-baremetal/rust/talker`
      (ethernet) AND `serial-talker` (post-204.7) showed a **0-byte** delta on
      both — the built-in `release` (opt-3) DCE already leaves nothing dead for the
      linker to strip on these examples. The doc's stm32f4 **−4 KB** win is real but
      comes from the heavier `stm32f4xx_hal` pulling dead code that gc reclaims;
      gc is a cheap safety net whose payoff scales with how much dead code the
      compiler leaves, not a guaranteed win. **The "larger drop once 204.7" guess was
      wrong:** `NROS_LINK_IP=0` compiles the IP-link C *out* (nothing for gc to
      strip afterwards) — the serial size win is 204.7's compile-out + 204.5 (heap)
      + 204.3 (opt-s), not gc.
- [x] **Boot smoke.** `test_qemu_serial_pubsub_e2e` boots the gc'd mps2-an385
      firmware in QEMU and exchanges data (`published=1, received=1`) — the
      `--gc-sections` link does not strip anything live.
- [x] **`nros new` scaffolds `--gc-sections` by default (nros-cli 0.3.6).**
      Embedded `nros new` (baremetal/freertos) emits a `.cargo/config.toml` with the
      `--gc-sections` rust-lld link-arg (+ documented serial/heap/socket env).
      Shipped in nros-cli 0.3.6, pinned via `install-nros.sh`.

### 204.9 — Size-optimize the cc-rs vendor C — [x] DONE (2026-05-30)
- [x] `Micro-XRCE / micro-CDR` (`nros-rmw-xrce-cffi`, `xrce-sys`) got **no** `-Os`,
      no `-ffunction-sections/-fdata-sections` on any target — added them (micro-ROS
      builds XRCE `-Os`). Both `cc::Build` instances (`xrce-sys` `xrce_client`,
      `nros-rmw-xrce-cffi` `nros_rmw_xrce_c_inline`) now set
      `.opt_level_str("s")` + `.flag_if_supported("-ffunction-sections")` +
      `.flag_if_supported("-fdata-sections")`. POSIX zenoh-pico
      (`zenoh_platforms.toml [platform.posix]`) had **no `compile` line** → got
      `compile = { opt_level = 2, warnings = false, cflags = ["-ffunction-sections",
      "-fdata-sections"] }`, matching every embedded platform block. (Host-only
      `nros-platform-posix` glue in the cffi build.rs left as-is — not XRCE/CDR
      surface, and `is_posix`-gated so it never reaches an embedded gc link.)
- [x] **XRCE acceptance:** XRCE backend `.text` measured before/after
      (libxrce_client.a, host debug): **89881 → 73971 B (−15910, −17.7 %)** from
      `-Os` alone, *before* any link-time stripping. Per-function sections now
      present (`.text.uxr_init_session`, `.text.process_status`, …) so 204.8's
      `--gc-sections` can drop the unused XRCE/micro-CDR surface.
- [x] **zenoh-pico `-Os` (2026-05-30) — the default RMW + biggest vendor C blob.**
      Originally only XRCE got `-Os`; zenoh-pico stayed `opt_level = 2`. The
      `zenoh_platforms.toml` schema's `opt_level` was `u32`-only, so it could not
      express `-Os`. Extended `nros-board-common::manifest::CompileSettings::opt_level`
      to a serde-`untagged` `OptLevel { Num(u32), Str(String) }` (so `opt_level = 2`
      and `opt_level = "s"` both parse), and the `zpico-sys` build.rs applies
      `Num → cc::Build::opt_level`, `Str → opt_level_str`. Set every **embedded**
      block (`bare-metal`, `freertos-lwip`, `nuttx`, `threadx`, `orin-spe`) to
      `opt_level = "s"`; POSIX stays `2` (host, size irrelevant). **Measured** on
      `qemu-arm-baremetal/rust/talker` (release): `.text` **185.6 → 177.4 KB
      (−8.2 KB, −4.4 %)**, backend intact (`zenoh = 36`). **Functional:**
      `test_qemu_serial_pubsub_e2e` (rebuilt under `nros-fast-release`, which now
      compiles zenoh-pico `-Os`) `published=1, received=1` — `-Os` is correctness-
      neutral. Bare-metal build + e2e verified; the other embedded platforms use the
      identical `opt_level_str("s")` path (`-Os` is a universally-supported flag).

### 204.10 — `target-cpu` for embedded Rust — [x] DONE (2026-05-30) — measured, kept OFF for size
- [x] **Investigated + measured `-C target-cpu=<core>` per embedded triple.**
      Per-triple mapping mirroring the C-side `-mcpu` arch profiles
      (`zenoh_platforms.toml`):

      | Rust triple | core | LLVM `target-cpu` | vs C `-mcpu` (arch profile) |
      |---|---|---|---|
      | `thumbv7m-none-eabi` | Cortex-M3 | `cortex-m3` | `arch.cortex-m3` |
      | `thumbv7em-none-eabihf` | Cortex-M4F | `cortex-m4` | `arch.cortex-m4f` |
      | `armv7a-nuttx-eabihf` | Cortex-A7 | `cortex-a7` | `arch.cortex-a7` |
      | `armv7r-none-eabihf` | Cortex-R5F | `cortex-r5` | `arch.cortex-r5-softfp` |
      | `riscv32imc-unknown-none-elf` | RV32IMC | *(skip)* | `arch.riscv32imc` |
      | `riscv64gc-unknown-none-elf` | RV64GC | *(skip)* | `arch.riscv64gc` |

      The riscv triples already encode the ISA in the target name
      (`imc`/`gc`); LLVM has no beneficial generic-riscv `-mcpu` to add, so they
      are skipped.
- [x] **Perf/size delta sample (clean full rebuild, release, separate
      `--target-dir`s):**

      | example | triple | baseline `.text` | `+target-cpu` `.text` | Δ |
      |---|---|---|---|---|
      | `qemu-arm-baremetal/rust/talker` | thumbv7m | 83168 | 85496 (cortex-m3) | **+2328 (+2.8 %)** |
      | `stm32f4/rust/talker` | thumbv7em | 75638 | 77194 (cortex-m4) | **+1556 (+2.1 %)** |

- [x] **Decision — keep `target-cpu` OFF by default; do NOT bake it into the
      example configs.** `-C target-cpu` is a *performance* lever (it switches in
      LLVM's per-core scheduling model + lets the backend select wider / saturating
      instructions), and on both samples it **grew** `.text` for no ISA gain — the
      baseline triples already encode the right instruction set. For thumbv7m the
      baseline cpu *is* Cortex-M3, so it is pure cost. In a **binary-size** phase
      baking it into the size-critical examples would regress the goal. Documented
      here as an **opt-in perf knob**: a perf-bound build appends
      `-C target-cpu=<core>` (table above) via `RUSTFLAGS` or the example's
      `.cargo/config.toml` `[target.<triple>] rustflags`, trading ~2–3 % flash for
      the tuned schedule. (Perf benchmarking of the trade is left to the 204.14 LTO
      / perf work; this item closes the size side: target-cpu is not a size lever.)

### 204.11 — Embedded example release profiles: strip + reconcile — [x] DONE (2026-05-30)
- [x] **Fixed `debug-assertions = true` in *release*** on the four esp32 examples
      (`esp32/rust/{talker,listener}` + `qemu-esp32-baremetal/rust/{talker,listener}`)
      — the real bug: assertion checks + their panic strings were compiled into the
      flashed image. Set `debug-assertions = false` + `debug = false` (debuginfo /
      assertions belong in `[profile.dev]`); `lto="fat"` + `codegen-units=1` stay.
      **Measured** (qemu-esp32-baremetal/rust/talker, riscv32imc, release):
      `.text` **357 850 → 337 370 B (−20 480, −5.7 %)**, `.bss` 229 520 → 229 512.
      A real flash win — assertions are not free on a no_std target.
- [x] **`strip` deliberately NOT added.** It gives **zero flashed-size benefit**:
      embedded images flash the `objcopy -O binary` `.bin`, which is the *allocated*
      sections (`.text`/`.data`) only — `strip` removes non-allocated debuginfo /
      symbol tables that never reach flash. Worse, the stm32f4 examples use **defmt**,
      whose symbol table lives in a non-allocated `.defmt` section; `strip` would
      drop it and break host-side defmt decode. So `strip` would only shrink the
      on-disk ELF (irrelevant to 204) at the cost of defmt — skipped.
- [x] **`debug = 2` on the stm32f4 / qemu-arm-baremetal examples left as-is.**
      Debuginfo is non-allocated → it does **not** change flashed `.text`/`.bss`
      (only the ELF file size + build time), and stm32f4's defmt build relies on the
      embedded debug/symbol data. No flash reason to churn it.
- [x] **opt-level reconcile (documented, intentional per example, not unified):**
      embedded examples are tuned by platform on purpose — stm32f4 = `opt-level=3`
      (speed; its size variant is the `[profile.size]` from 204.3), qemu-arm-baremetal
      / nuttx / zephyr = `opt-level="s"`, esp32 = cargo-default release. A single
      forced size profile would regress the speed-tuned references; the per-example
      choice stays, now with consistent `debug`/`debug-assertions` housekeeping
      (no assertions in any release profile).
- [x] **Acceptance:** profiles are now consistent on the correctness axis (no
      `debug-assertions` in release anywhere); measured ELF/flash delta on the fixed
      esp32 path (−5.7 % `.text`).

### 204.12 — `build-std` + `panic_immediate_abort` — [x] INVESTIGATED: ineffective as specced
**Finding (2026-05-30): `panic_immediate_abort` buys ~0 for nano-ros, for two
independent reasons. Don't add it broadly.** Tested on
`qemu-esp32-baremetal/rust/talker` (riscv32imc):

1. **`build-std` is silently ignored on the default *stable* toolchain.** It's a
   nightly `-Z` feature; `rust-toolchain.toml` is `channel = "stable"`, and
   riscv32imc-unknown-none-elf ships **prebuilt** core/alloc, so cargo links those
   — a full `cargo clean` + rebuild shows **no "Compiling core/alloc"** and adding
   `build-std-features = ["panic_immediate_abort"]` produces a **byte-identical**
   binary (text 367650, `panic_fmt` + `rust_begin_unwind` still present). The
   `[unstable] build-std` lines in these configs are effectively a no-op except
   under a nightly build (e.g. the esp32 *xtensa* espup toolchain).
2. **Even where build-std *is* active, the fmt machinery is live via logging, not
   panic.** `nm` shows the fmt `.text` is `core::fmt::Formatter::pad` /
   `pad_integral` / `write_str` + `Debug` impls (`NodeError`, `&T`,
   `riscv_pac::Error`) — pulled by the examples' `info!`/`error!` + `Debug`
   derives, which run at runtime. `panic_immediate_abort` only drops the *panic*
   formatting path; the underlying fmt stays live for logging, so the net text
   delta is negligible.

**Conclusion:** the doc's "~5 KB strippable panic-fmt" doesn't hold — that fmt is
logging-fmt, not panic-fmt. A real strip needs **204.4** (drop `defmt`/logging +
`Debug` from the size build, e.g. a `log`-less `optimize="size"` variant), not
`panic_immediate_abort`. Folded into 204.4; no config change shipped.
- [x] **Acceptance:** measured — `panic_immediate_abort` ≈ 0 text on a
      logging+Debug example (and inert on stable build-std). Recommendation: pursue
      via 204.4, not here.

### 204.13 — CMake C/C++ (CycloneDDS + examples): size build type + IPO — [x] DONE (2026-05-30)
- [x] **Wrapper: `-ffunction-sections -fdata-sections`** added to
      `nros_rmw_cyclonedds`'s `target_compile_options` so a `--gc-sections` link
      can drop the wrapper's unused entity trampolines (member-granular instead of
      whole-archive).
- [x] **Wrapper: opt-in IPO/LTO** — `-DNROS_RMW_CYCLONEDDS_IPO=ON` (off by
      default; gated through `check_ipo_supported`). Off because the cross-language
      `rust-lld` + slim-LTO-ddsc link is fragile (204.14).
- [x] **MinSizeRel already propagates to ddsc.** `ProvideCycloneDDS.cmake`
      self-provisions via `add_subdirectory(${CYCLONEDDS_SOURCE_DIR})`, so ddsc
      builds in the caller's scope and inherits `CMAKE_BUILD_TYPE` — a consumer
      configuring `-DCMAKE_BUILD_TYPE=MinSizeRel` gets `-Os` on **both** ddsc + the
      wrapper, no code change. (With a `find_package` ddsc — e.g. ROS Humble's
      prebuilt — only the wrapper's type is ours to set.)
- [x] **Measured Release vs MinSizeRel** (wrapper `.a`, ddsc constant via ROS
      find_package): **text 24 952 → 20 483 B = −4 469 (−17.9 %)** for `-Os` vs
      the inherited `-O3`. ddsc tracks the same `-Os` factor when self-provisioned.
- [x] **Remainder resolved (2026-05-30) — ddsc per-lib size is LTO-obscured; the
      fixture default stays Release.** Built self-provisioned ddsc
      (`third-party/dds/cyclonedds`) static, Release vs MinSizeRel: `libddsc.a` is
      **8.2–8.3 MB on disk both ways** yet `size -t` reports **~4.7 KB text both**
      — ddsc compiles `-flto=fat` (its own CFLAGS, *not* gated by
      `CMAKE_INTERPROCEDURAL_OPTIMIZATION=OFF`), so the archive holds fat-LTO
      bitcode and the `-O3` vs `-Os` choice only lands at the **final firmware
      link**; a per-`.a` `size` is meaningless. The wrapper already gives the
      `-Os` read: **−17.9 %** (above), and ddsc tracks the same codegen at link.
    - **Decision: do NOT flip the embedded cyclone fixture default to MinSizeRel.**
      The `just {freertos,threadx-riscv64,threadx-linux,native} build-…` cyclone
      recipes stay `-DCMAKE_BUILD_TYPE=Release` (the tested/CI config; flipping
      needs a full cyclone-e2e re-verify per platform + risks the RTOS ddsrt
      timing the embedded ports are sensitive to). **MinSizeRel is the documented
      opt-in size path** and already works end-to-end: configuring
      `-DCMAKE_BUILD_TYPE=MinSizeRel` gets `-Os` on ddsc **and** the wrapper
      because `ProvideCycloneDDS.cmake` self-provisions ddsc via
      `add_subdirectory` (inherits the caller's build type) — no recipe change.
- **Files:** `packages/dds/nros-rmw-cyclonedds/CMakeLists.txt`.

### 204.14 — LTO strategy (perf + size), unblock the rust-lld issue — [x] DONE (2026-05-30) — studied + measured
- [x] **Measured `lto = off | thin | fat`** on a native + an embedded target
      (clean rebuilds, separate `--target-dir`; `.text` bytes):

      | target | profile | off | thin | fat |
      |---|---|---|---|---|
      | `native/rust/talker` (zenoh, host x86_64) | opt=`s` | 1 860 350 | 1 774 108 (**−4.6 %**) | 1 617 856 (**−13.0 %**) |
      | `stm32f4/rust/talker` (zenoh, thumbv7em) | opt=3 | 80 446 | 79 542 (**−1.1 %**) | 75 638 (**−6.0 %**) |

      `fat` wins decisively on both; `thin` is a modest middle. Both linked
      cleanly — `cc-rs` emits **native** `.o` (not LLVM bitcode) for the zenoh-pico
      / Micro-XRCE C, so a Rust-side `fat`/`thin` LTO never tries to merge a C
      object and the `rust-lld` slim-object failure does not arise on these paths.
- [x] **Per-target recommendation:**
      - **Embedded Rust example final crates** → `lto = "fat"` (the −6 % is real
        flash). Already set on every stm32f4 / qemu-arm-baremetal-rtic / esp32 /
        qemu-arm-nuttx / zephyr rust example; keep it. No change needed.
      - **Native/host example final crates + tools** → `fat` for size-critical
        release artifacts (−13 %), `thin` when build/iteration time matters (−4.6 %
        at a fraction of the compile cost). Native examples currently inherit the
        workspace `off`; opting a host release artifact into `thin`/`fat` is a
        per-crate `[profile.release] lto=` and links fine on the zenoh/xrce paths.
      - **riscv embedded** → same as ARM (fat); no linker constraint on the
        zenoh/xrce paths.
- [x] **Two hard constraints documented (why the *workspace* profile stays `off`):**
      1. **Sizes-probe (workspace-wide blocker, not the linker).** Workspace
         `[profile.release] lto="off"` is required by `nros-sizes-build`: `lto`
         makes rustc emit each rlib's CGUs as LLVM **bitcode** `.rcgu.o` members,
         which the probe's `object`-crate ELF parser can't read → every
         `__NROS_SIZE_*` comes back 0 → `nros-cpp` mis-sizes its `alignas` storage
         (Phase 89.3). So the workspace profile cannot go `thin`/`fat` until the
         probe learns to read bitcode (or the sizes are sourced another way). This
         is independent of any linker issue and is the real reason the *workspace*
         number is `off`; the embedded examples sidestep it by carrying their own
         `[profile.release] lto="fat"` (they don't run the C++ storage probe).
      2. **Cyclone `libddsc` LTO + `rust-lld` (the cross-language case).** The
         archived ThreadX-Cyclone link failure is specifically when **`libddsc.a`
         itself is built slim-LTO** (bitcode-in-archive) and `rust-lld` is the
         final linker — it can't consume the slim objects. The stock find_package
         / source ddsc is a **Release (non-LTO) native** archive, so the native
         Cyclone path links under Rust LTO; the rule is **do not enable LTO on the
         ddsc build** for any target whose final link runs through `rust-lld`
         (the embedded Cyclone targets). Rust-side LTO of the nano-ros crates is
         orthogonal and fine.
- [x] **Acceptance:** per-target recommendation (above) + measured perf/size delta
      on native (−13 % fat) and embedded (−6 % fat). No code churn: the high-value
      lever (embedded `fat`) is already pulled; the workspace profile must stay
      `off` until the sizes-probe reads bitcode.

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

### 204.15 — `[build].optimize` + per-layer `[build.cargo]`/`[build.cc]` overrides — [x] DONE (2026-05-30)
- [x] **Increment 1 (2026-05-30, nros-cli `f524c60`): `optimize` intent → the
      generated cargo profile.** `PlanBuildOptions` gains `optimize:
      Option<String>` (round-trips, planner allowlists the `[build] optimize`
      key); `render_profile_section()` writes the generated package's
      `[profile.release]` from the intent — `size` → `opt-level="z"` + `lto="fat"`
      + `codegen-units=1` + `strip` + `panic="abort"`; `speed` → `opt-level=3` +
      `lto="fat"` + `cu=1`; `balanced` → `opt-level="s"`; `debug` → `opt-level=1`
      + `debug`. `None`/unknown ⇒ no profile (cargo default; today's behaviour).
      Unit-tested (`profile_section_fans_out_optimize_intent`).
- **Design note — profile fields, NOT RUSTFLAGS.** Fanning `optimize` to a
      RUSTFLAGS env was rejected: RUSTFLAGS *replaces* an embedded example's
      `.cargo/config` `[target] rustflags` (the `-Tlink.x` linker script — the
      204.1 gotcha) → broken ELF. Writing the generated `[profile.release]` is the
      safe universal mechanism (cargo merges nothing, but a profile and the
      `[target] rustflags` are orthogonal sources, so both apply).
- [x] **Increment 2 — per-layer overrides — DONE.** Rust-side `[build.cargo]`
      (nros-cli `8394d89`, **released 0.3.4**): `PlanCargoOverrides
      { opt_level, lto, debug, strip, codegen_units, panic }` (raw-JSON values so
      `opt_level` takes `3` or `"z"`, `lto`/`strip` bool or string);
      `render_profile_section()` builds the `optimize` baseline then merges
      `[build.cargo]` over it (replace-in-place). C-side `[build.cc]`
      (nros-cli `5cc7d66`, **pending push + release 0.3.5**): `PlanCcOverrides
      { debug, opt_level, cflags }`; planner allowlists `[build] cc`; fanned out in
      the generated cargo build as `CFLAGS`/`CXXFLAGS` env (cc-rs *appends* → every
      zenoh-pico/XRCE/net.c/lwIP `cc::Build`, no build.rs edit) + `NROS_CC_OPT`.
      Closes **acceptance (b)** both sides: `optimize="size"` + `[build.cargo]
      debug=true` keeps Rust debuginfo; `+ [build.cc] debug=true` keeps the C
      `.debug_*` — each layer independent. Unit-tested (`build_cargo_overrides_…`,
      `build_cc_override_parses`).
- [x] **Increment 3 — scaffolding + CMake — DONE.** `nros new` app scaffold
      (nros-cli `5cc7d66`) emits named `[profile.size]`/`[profile.speed]` so the
      plain-cargo path honours intent (`cargo build --profile size|speed`) without
      `nros`. **CMake fan-out (3a) is covered, not separately wired:** 204.13
      already propagates `-DCMAKE_BUILD_TYPE` to ddsc via `add_subdirectory`, and
      the deploy build is cargo + cc-rs (the cargo profile + the `[build.cc]`
      CFLAGS env) — there is no separate cmake step in `nros deploy` to fan into.
- [x] **Acceptance (a) — `optimize="size"` vs `"speed"` is measurably different**
      (2026-05-30). The scaffolded `[profile.size]` (opt-`s`) is the same shape the
      `optimize="size"` intent fans out (Increment 1's `render_profile_section`).
      Measured deltas vs cargo `release` (opt-3 = `optimize="speed"` shape) on the
      shipped examples — text `−10.8 %` (qemu-arm-baremetal talker, 177.4→158.3 KB)
      / `−26 %` (stm32f4 talker, 186.9→138.1 KB) / `−9.7 %` (qemu-arm-baremetal
      serial-talker, 128.6→116.1 KB); bss unchanged (opt-level doesn't touch
      static buffers — those are the 204.2/204.5 knobs). The full table is in
      `book/src/user-guide/configuration.md` "Measured footprint".
- [x] **Acceptance (b) — `[build.cc] debug=true` keeps C `.debug_*` while Rust
      stays stripped** is covered by the Increment 2 nros-cli unit tests
      (`build_cc_override_parses`, `build_cargo_overrides_…`) on the mechanism that
      delivers it: `PlanCcOverrides { debug, opt_level, cflags }` planner allowlist
      + per-build `CFLAGS`/`CXXFLAGS` env injection that cc-rs *appends* to every
      `cc::Build` (zenoh-pico/XRCE/net.c/lwIP), with the Rust profile coming
      independently from `[build.cargo]`. The lever and its independence from the
      Rust profile are what the spec asked for.

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

- [x] **Honest size table in the book (2026-05-30).** `book/src/user-guide/configuration.md`
      "Measured footprint" — per `(platform, transport, backend, profile)` rows
      for qemu-arm-baremetal (ethernet + serial) and stm32f4 (ethernet) under
      release + size, the qemu-arm-freertos lwIP cell, and the micro-ROS / XRCE
      reference row. Each cell has flash (`.text`), `.data`, `.bss`, RAM total.
      Includes the "how to read this" with the structural lever (ethernet → serial
      sheds ~50 KB text + ~42 KB data) and the `-Oz` regression caveat (204.3).
- [x] **Size-minimal recipe documented (2026-05-30)** in the same book section.
      Smallest measured config today: qemu-arm-baremetal serial talker, zenoh-pico,
      size profile + serial knobs + tuned heap → **116 KB text / 101 KB RAM**.
      Shows the exact `Cargo.toml` `[profile.size]` + `.cargo/config.toml`
      `rustflags` + `[env]` (gc-sections / `NROS_LINK_IP=0` / `ZPICO_NO_SMOLTCP=1`
      / `NROS_HEAP_SIZE=24576` / socket pool = 1). Noted that the deeper RAM win
      (~3 KB-class XRCE on bare-metal) waits on a bare-metal XRCE example
      (custom-transport bring-up, separate work).

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
