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
- [ ] Re-measure + document serial vs ethernet in the book once the lean
      registration (204.1.L below) lands.

#### 204.1.L — Lean (entity-gated) vtable registration

**The real footprint lever.** `RustBackendAdapter::<R>::VTABLE` is a `const`
wiring *every* entity trampoline (`create_subscriber`, `create_service_server`,
`create_service_client`, …). The explicit `register()` call references that const,
so `--gc-sections` keeps all of it — and each trampoline transitively pins the
backend's per-entity static buffers (`nros-rmw-zenoh` `SUBSCRIBER_BUFFERS` 34.5 KB,
`g_pending_gets` 16.4 KB, `SERVICE_BUFFERS` 10.4 KB). A publish-only node pays for
subscriber + service + client machinery it never calls. We cannot drop `register()`
(it is the bare-metal linkage anchor, see above) — so instead **make the vtable
install only the slots the node opts into**, leaving the rest as cheap
`RET_UNSUPPORTED` stubs that reference nothing, so gc collects the real trampolines
and the backend buffers behind them.

**Mechanism — `cfg`-selected vtable slots in `nros-rmw-cffi`.**

- New `nros-rmw-cffi` features (all in its own `default`, so standalone cffi users
  keep full behaviour): `entity-subscriber`, `entity-service-server`,
  `entity-service-client`. `publisher` stays ungated (cheap, no big buffer; a node
  with neither pub nor sub is pointless).
- `RustBackendAdapter::<R>::VTABLE` selects each gated required slot via a
  `const`-fn / `cfg!`: real trampoline when the feature is on, else an
  `unsupported_*` stub of the matching signature returning `NROS_RMW_RET_UNSUPPORTED`
  (and `0` / NULL for the `has_data`/`has_request` probes). The `Option` slots that
  already default to `None` need no change. Gated subscriber slots:
  `create_subscriber`, `destroy_subscriber`, `try_recv_raw`, `has_data`,
  `register_subscriber_event`. Service-server: `create_service_server`,
  `destroy_service_server`, `try_recv_request`, `has_request`, `send_reply`.
  Service-client: `create_service_client`, `destroy_service_client`, `call_raw`,
  `send_request_raw`, `try_recv_reply_raw`.
- Because the real trampoline is the *only* edge into the backend's
  `Session::create_subscriber` (and thus its buffers), stubbing the slot makes the
  whole chain dead → gc strips it. No change needed in the backend crate.

**Feature plumbing — non-breaking (zero churn on the ~70 existing examples).**

- `nros-node`: `rmw-cffi` keeps forwarding **all** entity features
  (`nros-rmw-cffi/entity-subscriber` + `…service-server` + `…service-client`) →
  every example currently listing `rmw-cffi` is byte-for-byte unchanged. Add a
  sibling `rmw-cffi-lean = ["dep:nros-rmw-cffi"]` that forwards **none**, plus
  granular `rmw-entity-{subscriber,service-server,service-client}` passthroughs.
- `nros`: mirror — `rmw-cffi` (full, default path) + `rmw-cffi-lean` +
  `rmw-entity-*`.
- A size-critical example swaps `features=["rmw-cffi", …]` →
  `features=["rmw-cffi-lean", …]` (+ any `rmw-entity-*` it does need). Cargo
  feature unification is per-build, so one example going lean does not affect others.

**Acceptance.** A pub-only `qemu-arm-baremetal/rust/talker` built with
`rmw-cffi-lean` links the backend (zenoh > 0) **and** drops `SUBSCRIBER_BUFFERS` +
`SERVICE_BUFFERS` + `g_pending_gets` (verify `nm`: those symbols absent, `_z_/zp_`
present) → expect ≈ −55 KB bss / −20–40 KB text vs the full build, while a
sub-or-service example on plain `rmw-cffi` is unchanged. Re-measure serial +
ethernet talkers after.

- [ ] **204.1.L.1** — `nros-rmw-cffi` `entity-*` features + `unsupported_*` slot
      stubs + `cfg`-selected `VTABLE`. Standalone `cargo test -p nros-rmw-cffi`
      green (full features).
- [ ] **204.1.L.2** — `nros-node` + `nros` feature plumbing (`rmw-cffi` = full
      forward, `rmw-cffi-lean`, `rmw-entity-*`). Workspace `cargo check` green;
      existing examples unchanged.
- [ ] **204.1.L.3** — switch `qemu-arm-baremetal/rust/talker` (pub-only) to
      `rmw-cffi-lean`; verify backend linked + buffers dropped (`nm`) + e2e green
      (rebuild under the `nros-fast-release` fixture profile — see the stale-binary
      lesson above). Measure.
- [ ] **204.1.L.4** — roll to the other pub-only / sub-only bare-metal examples;
      book write-up with the before/after table.

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

### 204.3 — Size-tuned embedded release profile — [~] profile landed + quantified
- [x] **`[profile.size]` added (2026-05-30)** to `examples/stm32f4/rust/talker`:
      `inherits="release"` (keeps `lto="fat"` + `codegen-units=1`) but
      `opt-level="z"` (size) instead of `3` (speed) + `strip=true` + `debug=false`.
      `panic` is already `abort` on the thumbv7em target. Build with
      `cargo build --profile size`.
- [x] **Quantified `z` vs `3`** (stm32f4 talker, thumbv7em-none-eabihf, same
      lto/cu, with 204.2 + 204.8 already applied):

      | profile | opt | text | data | bss |
      |---|---|---|---|---|
      | `release` | `3` (speed) | 75.6 KB | 13.7 KB | 51.8 KB |
      | `size`    | `z` (size)  | **59.9 KB** | 13.7 KB | 51.8 KB |

      **text −14.3 KB (−18.9 %)**; data/bss unchanged (opt-level doesn't touch
      static buffers — those are 204.2/204.5/204.6 territory). `z` trades some
      speed for ~19 % less flash on the hot networking + zenoh-pico code.
- [ ] **Remainder:** roll `[profile.size]` into the other size-critical examples
      + add a `just <plat> build --size` knob that passes `--profile size` (each
      example needs the profile defined; `nros new` should scaffold it — ties into
      204.15's `optimize="size"`).
- [x] **Acceptance:** measured text delta documented (−18.9 %); a size profile
      exists (`cargo build --profile size`).

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
- [ ] **Remainder:** `nros new` scaffolds `NROS_LINK_IP=0` for serial boards
      (lives in nros-cli); resolve the cffi register-path confound (204.1) so the
      serial *absolute* number reflects the shed IP stack.

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
