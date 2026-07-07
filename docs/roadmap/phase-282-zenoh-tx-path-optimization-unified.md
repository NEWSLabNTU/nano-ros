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

- [ ] W1.a In the vendored zenoh-pico fork: split locking into the existing tx
  mutex (guards wbuf/batch state — held only to STEAL the pending batch:
  swap `ztc->_wbuf` with a pre-allocated spare, reset `_batch_count`) and a new
  **link-write mutex** (guards the actual `_z_link_send_wbuf` call). ALL wire
  writers — batch flush, immediate n_msg sends, t_msg keepalives, express
  sends, fragments — take the link-write mutex for the socket write, so
  concurrent whole-frame writes cannot interleave. Publishers append under the
  tx mutex while a send is in flight on the link mutex.
- [ ] W1.b Gate the split behind a fork-local `Z_FEATURE_TX_SPLIT_LOCK` (struct
  fields for the spare wbuf are gated → flips in the SHARED generated config
  per the #135 rule; Zephyr gets it via `zephyr_compile_definitions`). Default
  OFF until W3 validates.
- [ ] W1.c Correctness pass: frame-header/SN handling on the stolen buffer
  (a stolen batch is a complete frame; the fresh wbuf re-prepares on next
  append), fragmentation path, `Z_FEATURE_BATCH_TX_MUTEX=1` interaction,
  single-threaded platforms compile the split out.
- [ ] W1.d Target: ctrl (100 Hz) ≥ ~90 msg/s at the 100 ms socket timeout with
  batch+thread+split on (vs 25.8 today); telem stays ≈ ideal. Re-measure both
  timeouts on the W1 harness.

### W2 — Zephyr streaming benchmark (the promotion-relevant number)

- [ ] W2.a A minimal Zephyr bench leaf: tight-loop Int32/byte publisher
  (`stress-zenoh` talker semantics) as a west app in the zephyr-fixture-leaves
  driver, plus the native `stress-zenoh` listener as the sink. One leaf, no
  per-language variants needed (the tx path under test is the shared shim).
- [ ] W2.b Record msgs/s + tx-side elapsed: knob off / batch+thread /
  batch+thread+split(W1), at 100 ms and 5 ms socket timeouts. This is the
  number that decides whether the batch knob is promoted into any default
  config or example (`prj-zenoh.conf`) — per phase-279's "measure before
  promoting".

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
