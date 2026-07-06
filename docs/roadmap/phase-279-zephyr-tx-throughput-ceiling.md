# Phase 279 — Zephyr tx throughput ceiling: measure, then batch-mode fix

Status: **Planned — 2026-07-06** · Implements issue #145 · Related [[issue-0139]]
(lease-death variant of the same zsock serialization).

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
must be off by default and escape-hatched for control tiers:
- A config knob (`CONFIG_NROS_ZENOH_BATCH` / a build define), default OFF → zero
  behavior change when unset.
- A per-publish "flush now" path so a control-tier publisher (or any latency-
  sensitive put) still sends immediately; only telemetry tiers ride the batch.
- Flush cadence driven by `zpico_spin_once` (one flush per spin), so the batch
  window tracks the executor period the app already tunes.

## Waves

### W1 — Measure the ceiling (baseline harness)
- [ ] W1.a Reproducible native_sim (or QEMU) throughput fixture: a multi-tier
  publisher (mirror the 276-W2 100 Hz + 10 Hz pair) + a sink that counts
  received msgs/s over a fixed window. Extend `nros-bench/stress-zenoh` if it
  fits; else a new `nros-bench` leaf.
- [ ] W1.b Record msg/s at `Z_CONFIG_SOCKET_TIMEOUT` = 100 ms and 5 ms, single-
  and two-tier. Confirm the ~1-send/window model (throughput ≈ windows/s) and
  capture the numbers in this doc + platform-implementation-notes.
- [ ] W1.c Decision gate: confirm batching is the right lever (vs the ceiling
  being dominated by something else). If not, re-scope before W2.

### W2 — Opt-in batch-mode flush
- [ ] W2.a `zpico.c`: `zp_batch_start(session)` after `zpico_open` when the batch
  config is on; `zp_batch_stop` in `zpico_close`.
- [ ] W2.b `zpico_batch_flush()` → `zp_batch_flush(session)`, called once per
  `zpico_spin_once` (multi-threaded read-drive path). Gate on the config.
- [ ] W2.c Immediate-send escape: a `zpico_publish_now` (or a flag on publish)
  that flushes right after the put, for control-tier / latency-sensitive topics.
  Surface through the Rust/C publisher API (QoS or a per-publisher option).
- [ ] W2.d Config knob (`CONFIG_NROS_ZENOH_BATCH`, default OFF) threaded through
  the zephyr build + the zenoh_platforms.toml define set; documented next to
  `Z_CONFIG_SOCKET_TIMEOUT`.

### W3 — Re-measure, validate, document
- [ ] W3.a Re-run the W1 harness with batch mode ON; confirm throughput scales
  with puts-per-flush (target: ≫ the W1 baseline at the same socket timeout).
- [ ] W3.b Assert the immediate-send escape keeps control-tier latency bounded
  (no regression vs batch-off).
- [ ] W3.c Update platform-implementation-notes "Zephyr zsock per-fd
  serialization" with the batch-mode knob + the before/after numbers; flip #145
  to resolved (or note residual = the second-link / upstream-zsock levers as
  future work if batching doesn't cover the control-tier case).

## Out of scope (future levers, if batching is insufficient)
- Dedicated second tx socket (zenoh-pico multi-link / a second publisher
  session) — cleanest semantics, needs link plumbing.
- Upstream Zephyr zsock/NSOS mode releasing the fd lock during poll — biggest
  lever, hardest to land.
