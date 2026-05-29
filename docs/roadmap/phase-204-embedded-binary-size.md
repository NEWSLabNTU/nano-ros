# Phase 204 — Embedded binary-size reduction

**Goal.** Shrink nano-ros embedded flash + RAM toward the micro-ROS footprint,
and document an apples-to-apples size story. The ROS/RMW layer is already
competitive; the size is dominated by what we statically **link for networking**
(a full TCP/IP stack + buffers + heap) on the ethernet/lwIP paths.

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

## Acceptance (phase)

- [ ] An honest size table in the book: per (transport, backend, platform) flash +
      RAM, with the micro-ROS comparison + the structural explanation (IP stack
      link vs serial/RTOS-stack-reuse).
- [ ] A documented size-minimal recipe (serial + XRCE + size profile + tuned
      pools) and its measured footprint.

## Notes

- The single biggest, cheapest win is **204.2** (socket-pool right-sizing — a
  config default, no architecture change) for RAM, and **204.1** (serial) for the
  honest comparison.
- Don't chase the ROS/RMW layer — it's already at the micro-ROS XRCE class; the
  cost is networking + buffers + heap + the speed-tuned profile.
- micro-ROS's structural advantages we can't fully match without the same trade:
  offloading discovery to an Agent (XRCE does this; zenoh-pico peer mode doesn't)
  and serial-default. The path to parity is **XRCE + serial + static pools**.
