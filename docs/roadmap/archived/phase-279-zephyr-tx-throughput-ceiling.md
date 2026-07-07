# Phase 279 — Zephyr tx throughput ceiling: measure, then batch-mode fix

Status: **Done — 2026-07-07 · W1-W4 + W2.c + W3.c(native) landed (batch +
dedicated flush thread, 4× @100 ms; express escape plumbed; native streaming
4.3× tx-side). Residual levers continue in
[phase-282](phase-282-zenoh-tx-path-optimization-unified.md)** ·
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
- [x] W2.c Per-publisher express escape — LANDED 2026-07-07: `TopicInfo::
  tx_express` (+ `with_tx_express` builder) → `NrosRmwQos.tx_express` (u8 carved
  from `_reserved0`, layout-identical ABI) → cffi `create_publisher` fill + the
  rust-adapter TopicInfo rebuild → `zpico_declare_publisher_ex(keyexpr,
  is_express)` (old fn = `_ex(key, 0)`) → `z_publisher_options_t.is_express`.
  Express puts bypass batching inside zenoh-pico (wire EXPRESS flag; harmless
  without batching). Follows the Phase 231 rx_buffer_hint pattern exactly.
  Remaining surface: expose on the nros-node publication builder / C-C++ QoS
  structs when a consumer needs it (TopicInfo is the RMW-level contract).
- [x] W2.d (code) Service/query latency guard — gets + replies `is_express` under the knob; the batch=ON service e2e assertion moves to W3.c. Original item:: queryable replies + `z_get` requests
  send express (or flush-after) so service RTT gains no spin-period latency
  when batching is on. Add/extend a service e2e assertion under batch=ON.
- [x] W2.e Document the knob next to `Z_CONFIG_SOCKET_TIMEOUT` in
  platform-implementation-notes (applies to every platform; biggest win on
  Zephyr where the send budget is the recv window).

### W3 — Re-measure, validate, document — MEASURED 2026-07-06: negative result for timer-paced workloads
- [x] W3.a Re-measured (`w1_zephyr_tx_throughput_measure`, 100 ms window, 20 s):

  | variant @100 ms | ctrl (ideal 100/s) | telem (ideal 10/s) | TOTAL |
  | --- | --- | --- | --- |
  | no batch (baseline) | 4.2 | 4.3 | 8.6 |
  | batch, flush-per-spin | 3.6 | 1.1 | **4.7 (worse)** |
  | batch + BLOCK-congestion pubs | 4.2 | 0.7 | **4.9 (worse)** |
  | batch + 50 ms rate-limited flush | 7.1 | 2.0 | **9.2 (≈ baseline)** |

  **Why batching cannot lift THIS workload class** (three mechanisms, verified
  by the deltas above):
  1. zenoh-pico's `zp_batch_flush` holds the transport tx mutex across the
     ENTIRE socket send — including the up-to-one-window wait on Zephyr's
     per-fd lock — so puts cannot append while a flush is in flight; the batch
     only accumulates between flushes.
  2. The flush runs on an executor/tier thread (`zpico_spin_once`), so each
     flush stalls the very timers that generate the puts for up to a window.
     Eager (per-spin) flushing also COMPETES with puts for send-window slots —
     that is the 8.6 → 4.7 halving.
  3. Timer-paced tiers (10 ms / 100 ms periods) produce ≤1 put per flush
     interval — there is nothing to coalesce. Batching pays only when MANY puts
     accumulate per interval (tight-loop / high-rate streaming publishers).

  The landed shape (rate-limited flush, `ZPICO_TX_BATCH_FLUSH_MS`, default
  50 ms) is the best of the three and ≈ baseline on this workload; the knob
  stays default-OFF and is honestly labelled: a potential win for high-rate
  streaming publishers, NOT a fix for the timer-paced tier ceiling.
- [x] W3.b Default-off regression: canonical conf restored (5 ms, no batch),
  realtime fixture rebuilt, `realtime_tiers_zephyr_entry_schedules_high_and_low`
  green. Native pubsub smoke green both knob states (W2).
- [x] W3.c Streaming benchmark (native) — MEASURED 2026-07-07 with the existing
  `nros-bench/stress-zenoh` tight-loop talker (5000 × 64 B puts,
  `PUBLISH_INTERVAL_MS=0`, shared zenohd):

  | | tx-side elapsed (5000 puts) | integrity |
  | --- | --- | --- |
  | knob off | 13 ms | — |
  | `ZPICO_TX_BATCH=1` | **3 ms (4.3×)** | 269/269 received valid |

  Streaming mechanics validated: appends replace per-put sends (4.3× faster
  put path), payload integrity holds across batch-overflow auto-flush
  boundaries (5000 × 76 B through a ~64 KB wbuf = many overflows), no
  corruption, bounded memory. The rx-side cap (~300 of 5000 received in both
  variants) is the subscriber-side best-effort blast limit — a separate,
  pre-existing axis, not #145. Residual: the promotion-relevant ZEPHYR
  streaming benchmark (a tight-loop publisher on the zsock-ceiling platform)
  needs a new zephyr bench leaf — future work alongside the fork-surgery
  lever.
- [x] W3.d Numbers + mechanism recorded here and in
  platform-implementation-notes.

### W4 — Dedicated tx-flush thread (lever (a)) — LANDED 2026-07-07, 4× lift

Moved the batch flush OFF the executor/tier threads: under `ZPICO_TX_BATCH` on
multi-threaded platforms (except ThreadX, which deliberately runs no background
tasks), `zpico_open` spawns a zenoh-pico `_z_task` that loops
`zp_batch_flush` + `z_sleep_ms(ZPICO_TX_BATCH_FLUSH_MS)`. The flush thread
absorbs the fd-window waits; tier threads only ever append to the batch. The
spin-entry flush remains only for the arms WITHOUT the thread (ThreadX +
single-threaded).

Re-measured (same W1 harness):

  | config | @100 ms | @5 ms |
  | --- | --- | --- |
  | no batch (baseline) | 8.6 | 39 |
  | batch, flush on tier threads (best variant) | 9.2 | — |
  | **batch + dedicated flush thread** | **34.1 (ctrl 25.8, telem 8.4)** | **52.5 (ctrl 43.6, telem 8.9)** |

  The 10 Hz telem tier reaches ≈ ideal at BOTH timeouts; total is 4× baseline
  at the 100 ms default. Residual gap (ctrl 25-44 of 100): puts still BLOCK on
  the transport tx mutex whenever the flush thread is mid-send (zenoh-pico
  holds the tx mutex across the whole socket write). Closing that needs fork
  surgery — release the tx mutex during the link write (swap/steal the wbuf
  under the mutex, send outside it, with a link-write mutex against concurrent
  t_msg/express writers) — or the second-link / upstream-zsock levers. #145
  stays open for that residual, with the opt-in batch+thread mitigation landed
  and documented.

## Out of scope (future levers, if batching is insufficient)
- Dedicated second tx socket (zenoh-pico multi-link / a second publisher
  session) — cleanest semantics, needs link plumbing.
- Upstream Zephyr zsock/NSOS mode releasing the fd lock during poll — biggest
  lever, hardest to land.
