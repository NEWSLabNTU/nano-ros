# Phase 279 — Zephyr tx throughput ceiling: measure, then batch-mode fix

Status: **In progress — 2026-07-06 · W1 (measurement) done, W2/W3 pending** ·
Implements issue #145 · Related [[issue-0139]] (lease-death variant of the same
zsock serialization).

> **Goal.** Lift the Zephyr tx ceiling of ~1 send per socket-recv window off the
> `Z_CONFIG_SOCKET_TIMEOUT` band-aid. **Measure the ceiling first** (establish a
> reproducible baseline + the number any fix must beat), THEN land an opt-in
> batch-mode flush that coalesces N puts into one send per window — throughput
> scales with messages-per-flush instead of 1/window, at a bounded, opt-in
> latency cost.

## Why

Zephyr's zsock holds a per-fd `fdtable` mutex for the whole blocking recv, so the
zenoh-pico read task owns the session socket for a full `Z_CONFIG_SOCKET_TIMEOUT`
window between inbound packets; every tx (`z_publisher_put`, declare, keepalive,
query reply) queues behind it. Net: **total image tx ≈ 1 send per recv window**.
Measured in 276-W2: a 100 Hz + 10 Hz tier pair throttled to ~5 msg/s each
(~10 msg/s total) at the 100 ms default.

The shipped mitigation — `Z_CONFIG_SOCKET_TIMEOUT` (5000 default; the ws-realtime
zephyr entry sets 5 ms → ~200 windows/s) — trades read-task wake rate for tx
budget and adds up to one window of tx LATENCY per message. Fine for native_sim
demos; questionable for real boards (power) and for anything past a few hundred
Hz. It is a band-aid, not a fix.

## Approach

Two of the three #145 directions are heavy (a dedicated second tx link needs
zenoh-pico multi-link plumbing; an upstream zsock mode that releases the fd lock
while parked in poll is the biggest lever but the hardest to land). The tractable
in-tree lever is **batch mode**, and it is already available:

- `Z_FEATURE_BATCHING = 1` is compiled in (`zenoh-pico/config.h:129`).
- The API exists: `zp_batch_start` / `zp_batch_flush` / `zp_batch_stop`
  (`api/primitives.h`). Between start/stop, `z_put`/`z_publisher_put` are queued
  instead of sent; a flush ships the whole queue in one socket send.

So `N` puts between flushes → one send/window carrying `N` messages → the ceiling
becomes `N`-msgs/window, decoupled from the window rate.

**Measure before building.** #145 explicitly says "pick after measuring." W1
stands up a reproducible native_sim throughput harness and records the baseline
at the default (100 ms) and 5 ms socket timeouts, single- and multi-tier. That
baseline (a) confirms the ~1-send/window model, (b) is the number W3 must beat,
and (c) tells us whether batching is even the right lever before we spend W2.
(native_sim is a relative baseline only — the absolute hardware number stays a
board-measurement follow-up, noted in the issue.)

**Opt-in, latency-aware.** Batching adds up to one flush-period of latency, so it
must be off by default and escape-hatched for control tiers (design below).

## Design (explored 2026-07-06) — uniform across ALL platforms

The mechanism lives entirely in the SHARED zpico shim + zenoh-pico, so the
migration is uniform by construction — native, Zephyr, FreeRTOS, NuttX, ThreadX,
and the bare-metal (smoltcp/serial) arms all route publishes through
`zpico_publish → z_publisher_put` and their executors all call
`zpico_spin_once`. Platform differences are confined to how the knob is passed.

**zenoh-pico batching mechanics (verified in the vendored source):**

- `zp_batch_start/flush/stop` toggle per-TRANSPORT state
  (`transport.c::_z_transport_start_batching`); while active, each n_msg tx
  appends to the transport `_wbuf` instead of sending
  (`tx.c::_z_transport_tx_flush_or_incr_batch`).
- **Thread-safe**: every append/flush takes the transport tx mutex
  (`Z_FEATURE_BATCH_TX_MUTEX=0` default = lock per op). Puts from app/executor
  threads, flush from the spin thread, and keepalives from the lease task all
  serialize correctly. No new locking needed in the shim.
- **Bounded memory**: a put that overflows the batch buffer auto-flushes first
  (`_z_transport_tx_batch_overflow`) — worst case is the existing wbuf size.
- **Bounded staleness**: transport messages FLUSH pending batch data before
  sending (`_z_transport_tx_send_t_msg_inner`), so the lease keepalive (~3.3 s)
  is a hard upper bound on batch sit-time even if an app stops spinning.
- **Native per-message bypass**: an "express" n_msg is sent immediately even
  while batching (`_z_transport_tx_get_express_status` → flush+send).
  `z_publisher_options_t.is_express` / `z_publisher_put_options_t.is_express`
  exist in the vendored zenoh-pico — a per-PUBLISHER escape is already in the
  protocol; we only need to surface the flag.

**Architecture:**

1. **Mechanism (zpico shim, shared)** — `zp_batch_start` after `z_open` when
   batching is enabled; a guarded `zp_batch_flush` at the TOP of
   `zpico_spin_once` (all six platform arms pass through the function entry;
   the multi-threaded arms only wait afterwards, so the flush lands before the
   sleep); `zp_batch_stop` in `zpico_close`. Flush cadence = the executor spin
   period the app already tunes. Zero-cost when the knob is off (one bool test).
2. **Opt-in knob (per-image, compile-time, default OFF)** — `ZPICO_TX_BATCH`,
   following the exact plumbing every other `ZPICO_*` knob uses:
   - Rust builds: `ZPICO_TX_BATCH` env → `nros-zpico-build` `shim_config_from_env`
     → `-D` on zpico.c (per-example `.cargo/config.toml [env]`, same as
     `ZPICO_MAX_*`).
   - C/C++ cmake builds (threadx/freertos/nuttx): `defines_kv` /
     `NROS_CMAKE_EXTRA_DEFS` → same `-D`.
   - Zephyr: `CONFIG_NROS_ZENOH_TX_BATCH` Kconfig forwarded by
     `zephyr/cmake/nros_rmw_zenoh.cmake` (mirrors
     `CONFIG_NROS_ZENOH_SOCKET_TIMEOUT_MS` → `Z_CONFIG_SOCKET_TIMEOUT`).
   One knob name, one semantic, six platform front-ends onto the same define.
   (A runtime session-property switch stays a possible follow-up; compile-time
   matches the precedent that embedded latency/RAM budgets are build decisions.)
3. **Control-tier escape (per-publisher, uniform)** — surface
   `is_express` through `NrosRmwQos` (the Phase 231 `rx_buffer_hint` pattern) →
   `zpico_declare_publisher` declares the zenoh publisher with
   `is_express = true` → its puts bypass batching natively. Control-tier topics
   opt out per-publisher; no custom flush plumbing.
4. **Service/query latency guard** — queryable REPLIES and `z_get` requests
   must not gain spin-period latency: declare/send them express by default (or
   flush immediately after) so service RTT is unchanged; only plain pubs ride
   the batch.

## Waves

### W1 — Measure the ceiling (baseline harness) — DONE 2026-07-06
- [x] W1.a Harness = `tests/w1_zephyr_tx_throughput_measure.rs` (`#[ignore]`,
  run `--ignored`): boots the ws-realtime-rust native_sim (ctrl 100 Hz +
  telem 10 Hz over ONE zenoh session), drains both `int32-sink` observers over a
  fixed 20 s window, prints per-tier + total msg/s. Reuses the existing
  `realtime_tiers_zephyr_entry_e2e` fixture (no new bench leaf needed).
- [x] W1.b **Baseline (native_sim, 20 s window)**:

  | `CONFIG_NROS_ZENOH_SOCKET_TIMEOUT_MS` | ctrl (ideal 100/s) | telem (ideal 10/s) | TOTAL |
  | --- | --- | --- | --- |
  | 100 (default) | 4.2 msg/s | 4.3 msg/s | **8.6 msg/s** |
  | 5 (ws-realtime mitigation) | 33.4 msg/s | 5.5 msg/s | **39 msg/s** |

- [x] W1.c **Gate — batching confirmed as the right lever.** At 100 ms the two
  tiers CONVERGE to ~4.3 msg/s each: a 100 Hz and a 10 Hz publisher get the SAME
  throughput because both fight for one shared send/window budget (matches the
  276-W2 "~5 msg/s each" observation). Total tracks the send-window rate
  (5 ms→39, 100 ms→8.6), NOT the ideal 110/s. So the cap is sends-per-second on
  the shared socket — exactly what coalescing N puts into one send removes.
  **W3 target:** at 100 ms, batching should let one send/window carry both a
  ctrl and a telem put → total approaching the ideal ~110/s (vs 8.6 today),
  decoupled from the window. (native_sim = relative baseline; absolute board
  numbers remain a hardware follow-up.)

### W2 — Opt-in batch-mode flush (uniform: mechanism in the shared shim)

Status: W2.a / W2.b / W2.d(code) landed 2026-07-06; W2.c (per-publisher express
via NrosRmwQos) pending — multi-layer ABI plumb, its own change. Found during
impl: the generated generic config pinned `Z_FEATURE_BATCHING 0` (the vendored
default 1 only applies to Zephyr's vendored-config build), and the flag gates
transport-struct FIELDS — so the knob flips it in the SHARED generated header
(issue-0135 every-TU rule), keeping knob-off builds byte-identical. Validated:
knob-off + knob-on compile clean; batch-ON native pubsub smoke delivers 7/7.

- [x] W2.a `zpico.c`: `g_tx_batching` gate + `zp_batch_start` after the session
  opens (`zpico_open`, when `ZPICO_TX_BATCH` is on), `zp_batch_stop` in
  `zpico_close`, and a guarded `zp_batch_flush` at the TOP of `zpico_spin_once`
  (before every platform arm — the multi-threaded arms must flush BEFORE their
  wake-primitive wait). One code path for all six platform arms.
- [x] W2.b Knob plumbing, one define six front-ends: `ZPICO_TX_BATCH` env knob in
  `nros-zpico-build::shim_config_from_env` (Rust builds, per-example
  `.cargo/config.toml [env]` like `ZPICO_MAX_*`); available to C/C++ cmake lanes
  via `defines_kv`/`NROS_CMAKE_EXTRA_DEFS`; zephyr Kconfig
  `CONFIG_NROS_ZENOH_TX_BATCH` forwarded in `zephyr/cmake/nros_rmw_zenoh.cmake`
  (mirror of the SOCKET_TIMEOUT forward). Default OFF everywhere.
- [ ] W2.c Per-publisher express escape: `is_express` on
  `zpico_declare_publisher` (new arg or options struct), surfaced through
  `NrosRmwQos` (rx_buffer_hint pattern) up to the Rust/C/C++ publisher QoS so
  control-tier topics bypass the batch natively.
- [x] W2.d (code) Service/query latency guard — gets + replies `is_express` under the knob; the batch=ON service e2e assertion moves to W3.c. Original item:: queryable replies + `z_get` requests
  send express (or flush-after) so service RTT gains no spin-period latency
  when batching is on. Add/extend a service e2e assertion under batch=ON.
- [x] W2.e Document the knob next to `Z_CONFIG_SOCKET_TIMEOUT` in
  platform-implementation-notes (applies to every platform; biggest win on
  Zephyr where the send budget is the recv window).

### W3 — Re-measure, validate, document
- [ ] W3.a Re-run `w1_zephyr_tx_throughput_measure` (--ignored) with
  `CONFIG_NROS_ZENOH_TX_BATCH=y` at BOTH 100 ms and 5 ms socket timeouts.
  Target: total ≈ the ideal ~110 msg/s at 100 ms (vs 8.6 baseline), i.e. the
  ceiling decouples from the window rate.
- [ ] W3.b No-regression sweep with batching DEFAULT-OFF: the existing zephyr /
  threadx / freertos / nuttx e2e lanes stay green (knob unset = today's
  behavior, byte-identical config).
- [ ] W3.c Batch=ON spot-checks beyond zephyr: native pubsub + one RTOS lane
  (threadx-riscv64 rust, fixtures already green) to prove the uniform shim path
  behaves on a second platform; service e2e under batch=ON (W2.d guard).
- [ ] W3.d Record before/after numbers in this doc +
  platform-implementation-notes; flip #145 resolved (residual = second-link /
  upstream-zsock levers only if the control-tier case still wants sub-window
  latency at high rates).

## Out of scope (future levers, if batching is insufficient)
- Dedicated second tx socket (zenoh-pico multi-link / a second publisher
  session) — cleanest semantics, needs link plumbing.
- Upstream Zephyr zsock/NSOS mode releasing the fd lock during poll — biggest
  lever, hardest to land.
