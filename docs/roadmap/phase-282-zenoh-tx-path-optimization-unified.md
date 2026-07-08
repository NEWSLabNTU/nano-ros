# Phase 282 — Zenoh tx-path optimization: the remaining levers, unified across platforms and languages

Status: **W1–W4 done — 2026-07-08; #145 RESOLVED** (remaining: promotion decision — options below, deliberately not decided; successor axis = issue 0148) · Continues [phase-279](archived/phase-279-zephyr-tx-throughput-ceiling.md)
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
- [x] **W1.d follow-up (2026-07-08) — the "generation-limited" hypothesis is
  WRONG; it is still (mild) TX drop, and W2.c roughly doubled ctrl.** Ran the
  published-vs-delivered discriminator the W1.d note asked for
  (`tests/w1d_native_tier_generation_probe.rs`, `#[ignore]`): the ctrl node
  publishes a monotonic counter, so the delivered Int32 *values* encode the
  published sequence — `max_value/window` = publish rate, `count/window` =
  deliver rate. Native ws-realtime-rust, batch+split, 15 s window, 3 runs, rock
  stable:

  | | publish rate | deliver rate | delivered/published |
  | --- | --- | --- | --- |
  | /ctrl (100 Hz tier) | **99.5/s** | **79.2/s** | **80%** |

  Two corrections to the W1.d text above:
  1. **The ctrl timer fires at ~99.5/s** — the tier is NOT generation-limited.
     The "executor timer under-fires / native_sim scheduling" suspects are ruled
     out: generation is at line rate.
  2. **Delivered is ~79/s, ~2.3× the 34.4/s W1.d recorded.** The gain lines up
     with the **W2.c overflow-steal fix (fork `ef065b9c`)**, which landed AFTER
     the W1.d measurement — the old cap was the overflow flush sending inline
     under the caller's mutex; parking the finalized batch in the spare wbuf
     removed it. So there is no separate "generation" axis to chase; the residual
     is a ~20% tx drop on the split-lock path (candidate: batch/flush-cadence
     coalescing vs the 10 ms tier, or spare-drain backpressure under sustained
     100 Hz). Re-measuring the W1.d tier table with the ef065b9c fork is the
     honest next step before any promotion decision.

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

- [x] W3.a LANDED 2026-07-07 — Rust: `QosSettings` gains a `tx_express: bool`
  field + const `.tx_express(bool)` setter (the profile is the uniform
  carrier, like reliability/depth), and all three publisher builders
  (`PublisherBuilder` / `TypedPublisherBuilder` / `GenericPublisherBuilder`)
  gain a `.tx_express(bool)` convenience. The cffi boundary ORs the two
  surfaces: `NrosRmwQos.tx_express = topic.tx_express || qos.tx_express`
  (direct-RMW users keep `TopicInfo::with_tx_express`). It is a transport
  hint, not a DDS policy — excluded from RxO/backend-compat validation.
  NOTE: declarative plan-level `qos_overrides` do NOT carry it yet — that
  needs the CLI schema + both run-plan emitters to move in lockstep
  (macro-vs-CLI sync rule); deferred until a deploy lane needs it.
- [x] W3.b LANDED — C: `nros_qos_t` gains `tx_express: u8` (ABI append, all
  profile statics 0), mapped in `to_qos_settings`; `nros_generated.h`
  regenerated.
- [x] W3.c LANDED — C++: `nros_cpp_qos_t` gains `tx_express` (cbindgen
  `nros_cpp_ffi.h` regenerated + the qos.hpp fallback mirror), `nros::QoS`
  gains constexpr `.tx_express(bool)` / `.tx_express()`, and EVERY
  `ffi_qos` fill site (13 across publisher/subscription/service/client/
  action/component headers) sets the field — they're stack structs, an
  unset field is garbage.
- [x] W3.d MEASURED — express-vs-batched in the same batching image
  (batch+split, 200 msgs @5 ms pacing, listener counts inter-arrival gaps;
  `TX_EXPRESS=1` env on the native talker, `-DSTRESS_EXPRESS=1` on the
  Zephyr bench app):

  | lane | batched | express |
  | --- | --- | --- |
  | native | 200/200, **21 gaps >25 ms, max 50 ms** (flush-cadence bursts) | 200/200, **0 gaps >25 ms, max 5 ms** (continuous) |
  | Zephyr | 200/200, 36 gaps (coalesced; talker 4 s) | window-paced ~8/s (each put pays the socket window itself) |

  The Zephyr express row is the expected trade: express = immediate send =
  back on the zsock per-window budget. Express is for LOW-RATE latency-
  critical topics (documented in the book page + QoS docs).

### W4 — Knob completeness + tuning docs

- [x] W4.a LANDED — `ZPICO_TX_BATCH_FLUSH_MS` front-ends: Kconfig
  `CONFIG_NROS_ZENOH_TX_BATCH_FLUSH_MS` (int, default 50, depends on
  TX_BATCH) forwarded by `nros_rmw_zenoh.cmake`; cargo lanes read the env in
  `nros-zpico-build` (`ShimConfig.tx_batch_flush_ms` → `-D` on the shim,
  only when batching); C/C++ cmake lanes pass `-DZPICO_TX_BATCH_FLUSH_MS`
  via `NROS_CMAKE_EXTRA_DEFS` (documented in the book page).
- [x] W4.b LANDED — `zpico_set_flush_task_config(priority, stack_bytes)`
  (call before open; FreeRTOS applies name/priority/stack, POSIX-like stack
  size only — mirrors `zpico_set_task_config`); the flush `_z_task_init` now
  takes the attr when configured. ThreadX/single-threaded builds have no
  flush thread and the call is a documented no-op.
- [x] W4.c LANDED — book page `user-guide/tx-tuning.md` ("TX Throughput &
  Latency Tuning"): decision tree, measured tables (streaming 9→136→181,
  tiers 8.6→34→43, express gap table), knob matrix across all three build
  lanes, pitfalls (deep-ring listeners, express-on-Zephyr, never flush from
  tier threads). Cross-linked from platform-implementation-notes (which also
  gained the phase-282 split-lock/express summary).
- [x] W4.d Default-off regression sweep — PARTIAL, honestly scoped. Green:
  `just check` (full clippy matrix incl. every touched crate), full fixture
  rebuild across all lanes (native/qemu/freertos/threadx/nuttx/zephyr, after
  the tx_express struct change invalidated every input signature), targeted
  default-off runs (native paced 500/500; zephyr knob-off streaming ≈8.9
  unchanged), and 1122/1259 of `just test-all`. NOT green: ~115 `test-all`
  failures that PRE-EXIST this phase's diff — verified by stash-baseline
  (e.g. `realtime_subnode_cpp_e2e` fails identically with the W3/W4 changes
  stashed and the fixture rebuilt from the pre-diff tree). They are machine
  env debt + latent breaks exposed by the first full fixture re-configure in
  months: missing `build/cyclonedds/bin/idlc` install (bridge/descriptor
  lanes), missing zenoh-pico-arm + idf-fixtures builds (emulator/esp lanes),
  a CLI/test flag mismatch (`--nros-toml` rejected — the phase-280 "stale
  nros install" blocker, still reproducing after `just setup-cli`), ROS 2
  interop lanes, and a deterministic `realtime_subnode_cpp` tier-ratio
  failure (ctrl=6 telem=5 — predates this diff; possibly phase-281's
  ws-realtime-cpp change or older). Needs its own env-resync/housekeeping
  pass — follow-up work, not silently absorbed here. Also fixed in this
  phase (found by the sweep): 5 stale `target_link_libraries` component
  names in freertos/threadx C++ example CMakeLists — phase-277 W5.B renamed
  the register NAMEs but missed these link lines; never caught because
  incremental fixture builds skip re-configure.

### Promotion options (documented for the maintainer decision — knobs stay default-OFF until decided)

All combinations keep knob-off builds byte-identical to pre-279 behavior;
"promote" = flip a default in one front-end layer, no mechanism change.

| option | what flips | wins | costs / risks | fit |
| --- | --- | --- | --- | --- |
| **A. status quo** (all off, docs only) | nothing | zero risk; tuning page carries the knowledge | every high-rate user must find the page; zephyr demos stay window-bound (~9 msg/s) | conservative default |
| **B. zephyr-only batch** (`CONFIG_NROS_ZENOH_TX_BATCH` default y) | one Kconfig default | 15× streaming, 4× tiers on the ONLY platform with the per-fd ceiling; other platforms untouched | +≤50 ms publish latency on non-express topics (FLUSH_MS-bounded); flush thread = 1 extra task + stack | targeted; the measured-pain platform |
| **C. zephyr batch+split** (B + `TX_SPLIT_LOCK` default y) | two Kconfig defaults | 20× streaming AND publisher never blocks on the socket (the only variant where a tight loop completes) | fork-only feature (`Z_FEATURE_TX_SPLIT_LOCK` upstream-divergence grows); spare wbuf doubles tx buffer RAM per session | best perf/latency if fork divergence is acceptable |
| **D. everywhere-on** (env defaults flip in nros-zpico-build too) | all lanes | uniform behavior story | native/POSIX gain little (no fd ceiling — 4.3× only in tight-loop blasts); latency cost paid platform-wide; ThreadX has no flush thread (spin-driven only) | not recommended — benefit is zephyr-shaped |

Notes for the decision: (1) `tx_express` per-publisher escape is uniform in
all three languages, so control-tier latency has an out under any option;
(2) timer-paced LOW-rate systems measurably LOSE under batching (phase-279
W3 negative result) — options B/C rely on such systems tolerating the
+FLUSH_MS latency or setting express, worth an explicit release-note line;
(3) RAM: split lock allocates a second wbuf (Z_BATCH_UNICAST_SIZE, zephyr
default 2 KB) per session.

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
