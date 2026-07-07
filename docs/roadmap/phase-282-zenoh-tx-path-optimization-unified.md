# Phase 282 — Zenoh tx-path optimization: the remaining levers, unified across platforms and languages

Status: **Planned — 2026-07-07** · Continues [phase-279](archived/phase-279-zephyr-tx-throughput-ceiling.md)
(measure → batch → flush thread, 4× landed) · Implements the residual of issue
#145 · Related [[issue-0135]] (shared-generated-config ABI rule), [[issue-0139]].

> **Goal.** Close the remaining gap between the phase-279 mitigation (34 msg/s
> total at the 100 ms Zephyr default) and line-rate tiers (~110 msg/s), and give
> every language the same latency/throughput controls — one mechanism in the
> shared layers, one knob-set, one QoS surface, measured on every platform. No
> per-platform or per-language forks of the design.

## Where phase-279 left the system

| lever | state |
| --- | --- |
| `ZPICO_TX_BATCH` + dedicated flush thread | landed, opt-in, 4× @100 ms; telem (10 Hz) ≈ ideal |
| per-publisher express escape | plumbed `TopicInfo::tx_express` → `NrosRmwQos` → `zpico_declare_publisher_ex` → `z_publisher_options_t.is_express`; **not yet surfaced in the Rust builder / C / C++ QoS APIs** |
| streaming validation | native only (stress-zenoh tight-loop: 4.3× tx-side, integrity across overflow-flushes); **no Zephyr streaming bench** |
| residual ceiling | 100 Hz tier at 25–44 of 100: puts still block on the transport tx mutex while a flush-send is in flight — zenoh-pico holds that mutex across the entire socket write |

## Unification principles (apply to every wave)

1. **Mechanism lives in the shared layers only** — the vendored zenoh-pico fork
   and the zpico shim, which every platform (native, Zephyr, FreeRTOS, NuttX,
   ThreadX, bare-metal smoltcp/serial) and every language front-end (Rust
   direct, C via nros-c, C++ via nros-cpp — all through the single-runtime
   umbrella) already route through. A lever that cannot be expressed there does
   not ship.
2. **One knob name, per-platform front-ends** — the `ZPICO_TX_BATCH` /
   `ZPICO_TX_BATCH_FLUSH_MS` pattern: env → `nros-zpico-build` for cargo/
   corrosion lanes, `defines_kv`/`NROS_CMAKE_EXTRA_DEFS` for C/C++ cmake lanes,
   `CONFIG_NROS_ZENOH_*` Kconfig forwards on Zephyr. Every new knob in this
   phase follows it.
3. **One QoS surface, three languages** — a capability exposed to Rust must be
   exposed to C and C++ in the same change (the RFC-0031 QoS model; the
   rx_buffer_hint / qos_overrides precedent). No Rust-only knobs.
4. **Config-header ABI rule (issue #135)** — any `Z_FEATURE_*` that gates
   struct fields flips ONLY in the shared generated config so every TU agrees;
   fixture rebuilds follow config changes.
5. **Measure before and after on the same harness** — `w1_zephyr_tx_throughput_
   measure` (tiers) + the W2 streaming bench below; a lever that does not move
   the number is reverted or parked, as phase-279 W3 demonstrated.

## Waves

### W1 — Fork surgery: release the tx mutex during the link write (the big lever)

The root of the residual: `_z_transport_tx_flush_buffer` (and every n_msg/t_msg
send) performs the socket write while HOLDING the transport tx mutex, so
publishers cannot append to the batch during the up-to-one-window fd wait, and
BLOCK-congestion puts stall their (tier) thread for the duration.

- [x] W1.a LANDED 2026-07-07. In the vendored zenoh-pico fork: split locking into the existing tx
  mutex (guards wbuf/batch state — held only to STEAL the pending batch:
  swap `ztc->_wbuf` with a pre-allocated spare, reset `_batch_count`) and a new
  **link-write mutex** (guards the actual `_z_link_send_wbuf` call). ALL wire
  writers — batch flush, immediate n_msg sends, t_msg keepalives, express
  sends, fragments — take the link-write mutex for the socket write, so
  concurrent whole-frame writes cannot interleave. Publishers append under the
  tx mutex while a send is in flight on the link mutex.
- [x] W1.b Gate landed: fork-local `Z_FEATURE_TX_SPLIT_LOCK` (struct
  fields for the spare wbuf are gated → flips in the SHARED generated config
  per the #135 rule; Zephyr gets it via `zephyr_compile_definitions`). Default
  OFF until W3 validates.
- [x] W1.c Correctness validated: native pubsub 5/5 under batch+split; streaming 5000 tight-loop puts in 4 ms with 718/718 received VALID (zero frame corruption through the steal path — notably MORE delivered than batch-only's 269). Original checklist: frame-header/SN handling on the stolen buffer
  (a stolen batch is a complete frame; the fresh wbuf re-prepares on next
  append), fragmentation path, `Z_FEATURE_BATCH_TX_MUTEX=1` interaction,
  single-threaded platforms compile the split out.
- [x] W1.d MEASURED — target missed, and the miss is diagnostic. @100 ms with
  batch+thread+split: **43.2 total (ctrl 34.4, telem 8.9 ≈ ideal)** — +27% over
  batch+thread (34.1), 5× baseline (8.6), but ctrl ≪ the ~90 target. The
  decisive cross-check: ctrl NEVER exceeds ~44/s in ANY configuration — 33.4 at
  5 ms no-batch (200 windows/s), 43.6 at 5 ms batch+thread, 34.4 at 100 ms
  batch+split — i.e. a ~40/s ctrl-tier ceiling INDEPENDENT of socket timing.
  With the tx path unblocked (appends never wait on a send now), the residual
  bottleneck has MOVED OFF the tx path: the 100 Hz tier is generation-limited
  (executor timer fire-once-late semantics under stall/jitter + native_sim
  scheduling of the 1 ms-spin tier thread are the suspects). Follow-up
  discriminators: instrument published-count at the talker side vs delivered;
  measure on hardware; investigate executor timer catch-up. This is a NEW,
  separate axis from #145's zsock serialization — the tx levers (batch + flush
  thread + split lock) now deliver telem at ideal and total at 5× baseline.

### W2 — Zephyr streaming benchmark (the promotion-relevant number)

- [x] W2.a LANDED 2026-07-07: `packages/testing/nros-bench/stress-zenoh-zephyr`
  — a west app publishing a tight loop over zpico DIRECTLY (bypasses the
  executor: the number isolates the shared transport tx path), payload/summary
  format identical to the native `stress-zenoh` talker so the native listener
  is the counting + integrity sink. Build/measure procedure in its README
  (manual west invocations; leaves-driver integration deferred until the knob
  is promotion-ready).
- [x] W2.b MEASURED, then RE-MEASURED after two artifacts were found and
  fixed (see W2.c). Final honest numbers (100 ms socket timeout, deep-ring
  listener, 64 B):

  | variant | talker (5000 msgs) | delivered | msgs/s | vs off |
  | --- | --- | --- | --- | --- |
  | knob off | not in ~33 s | 298/298 valid | ~8.9 | 1× (each put pays a full recv window) |
  | batch+thread | not in ~33 s | 4499/4499 valid | ~136 | ~15× |
  | batch+thread+split | finished, 27.7 s | 5000/5000 valid | ~181 | ~20× |

  The original W2.b run reported batch/split at ~22.5/s — that number was a
  MEASUREMENT ARTIFACT: the native listener's per-subscriber SPSC ring depth
  is compile-time (`ZPICO_SUBSCRIBER_RING_DEPTH`, default 4) and a batched
  publisher delivers a whole wire batch as one callback burst, so the 4-slot
  ring dropped the burst tail (drop-newest by design) and kept ~4 per batch.
  Bench listeners MUST build with `ZPICO_SUBSCRIBER_RING_DEPTH=1024` (README
  updated). The real finding under the artifact still held: the overflow
  flush on the publisher's thread was the cap → W2.c.
- [x] W2.c LANDED 2026-07-07 (fork ef065b9c): overflow steal. Under
  `Z_FEATURE_TX_SPLIT_LOCK`, `_z_transport_tx_batch_overflow` no longer sends
  inline under the caller's tx mutex — it PARKS the finalized batch in the
  spare wbuf (`_spare_pending`) and returns; every flush path (flush thread /
  t_msg / express / cadence) drains the parked spare first (older SNs first,
  wire order == SN order). At most one batch parks: a second overflow drains
  the pending one inline — natural backpressure. Locking rule that makes it
  sound: EVERY spare access happens under `_mutex_link_tx` (lock order
  strictly tx → link, link taken before tx released). First cut had a race —
  the flush-thread steal sent the spare after releasing tx while a concurrent
  overflow swapped into that same spare mid-send, corrupting the stream
  (symptom: 8/5000 delivered, session death) — fixed by the every-spare-
  access-under-link rule. Validation: native paced 2000/2000 valid, native
  default-off 500/500, Zephyr table above (only variant that completes AND
  delivers 100%).

### W3 — Language-uniform QoS surface for `tx_express`

The plumbing exists end-to-end (RMW → C shim → wire); expose it identically in
all three language APIs:

- [ ] W3.a Rust: the publication/publisher builder gains `.tx_express(bool)`
  (forwards to `TopicInfo::with_tx_express`); declarative `nros::node!` QoS
  metadata accepts `tx_express` where reliability/depth already live.
- [ ] W3.b C: `nros_qos_t` gains a `tx_express` field (nros-c `qos.rs` →
  `NrosRmwQos.tx_express`), default 0; ABI-append or reserved-byte carve like
  the cffi struct.
- [ ] W3.c C++: `nros::Qos` mirrors the C field (cbindgen FFI header is
  canonical — no hand-written redeclaration drift).
- [ ] W3.d One cross-language e2e: an express publisher and a batched publisher
  in the same batching image; assert the express topic's latency is not
  flush-cadence-quantized while the batched topic still coalesces. Run on
  native + one RTOS lane.

### W4 — Knob completeness + tuning docs

- [ ] W4.a `ZPICO_TX_BATCH_FLUSH_MS` front-ends: Kconfig
  (`CONFIG_NROS_ZENOH_TX_BATCH_FLUSH_MS`) on Zephyr; documented env/defines_kv
  for the other lanes (the define already exists in the shim).
- [ ] W4.b Flush-thread stack/priority attrs where a platform needs them (the
  zenoh-pico `z_task_attr_t` slot — mirrors `zpico_set_task_config` for
  read/lease tasks); ThreadX stays spin-driven (documented exception, same
  reason its read/lease tasks are disabled).
- [ ] W4.c Book page: "tx throughput & latency tuning" — the socket-timeout /
  batch / flush-cadence / express decision tree, with the measured tables from
  phase-279 + this phase; cross-linked from platform-implementation-notes.
- [ ] W4.d Default-off regression sweep across ALL platform e2e lanes (knob
  unset = byte-identical config) before closing.

### Out of scope (parked unless W1 misses its target)

- Dedicated second tx link (zenoh-pico multi-link plumbing or a second
  publisher-only session).
- Upstream Zephyr zsock change (release the per-fd lock while parked in poll) —
  the biggest lever, hardest to land; file upstream if W1+batch still caps
  real workloads on hardware.
- Hardware (real-board) absolute numbers — native_sim/QEMU stay the relative
  baseline for this phase.

## Exit criteria

#145 closes when: the W2 Zephyr streaming bench + the W1.d tier target are met
(or the misses are explained and parked with upstream issues filed), the QoS
surface is uniform across Rust/C/C++, and the tuning docs let a user pick the
right knobs without reading zpico.c.
