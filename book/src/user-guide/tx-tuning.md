# TX Throughput & Latency Tuning

*(zenoh-pico RMW — Zephyr, FreeRTOS, NuttX, ThreadX, POSIX; phases 279/282, issue #145)*

Every nano-ros language front-end (Rust, C, C++) publishes through the same
shared transport path (the zpico shim → zenoh-pico), so the knobs on this page
apply uniformly to all of them. They matter most on **Zephyr**, where the
kernel's socket layer serializes send/recv per file descriptor: the zenoh-pico
read task holds the session socket for a full receive window
(`CONFIG_NROS_ZENOH_SOCKET_TIMEOUT_MS`, default 100 ms), so **without tuning,
total image tx throughput is capped at roughly one socket send per window —
about 9 msg/s at the default**, regardless of how many publishers the image
runs or how fast they publish.

## Platform defaults (phase-290 / RFC-0049)

Since phase-290, per-platform defaults for these knobs live in each platform
package's `nros-platform.toml` (`[knobs.zenoh.tx]`), resolved through the
fixed ladder `builtin < platform < board < env/Kconfig` — an explicit
build-time setting (including an explicit `0` / `n`) always wins.

| platform | batch | split_lock | flush_ms | rationale |
| --- | --- | --- | --- | --- |
| **Zephyr** | **on** | **on** | 50 | the per-fd tx ceiling above; measured 15–20× streaming (phase-282). Requires the issue-0213 fork fix (declarations bypass the batch). `zephyr/Kconfig`'s `NROS_ZENOH_TX_*` defaults mirror this (drift-tested) — flip them per-app in `prj.conf` to opt out |
| everything else | off | off | 50 | no per-fd ceiling (POSIX), no flush thread (ThreadX/bare-metal), or simply unmeasured (FreeRTOS/NuttX — flip is one line in their platform toml after a bench run) |

With batching on, **non-express topics pay up to `flush_ms` of publish
latency** — declare low-rate latency-critical publishers `tx_express`
(below) to bypass the batch. Timer-paced low-rate systems that want the
old behavior image-wide: set `CONFIG_NROS_ZENOH_TX_BATCH=n` (Zephyr) /
`ZPICO_TX_BATCH=0` (cargo/cmake lanes).

## The decision tree

```text
Is total publish rate comfortably under ~1 msg per socket window?
├── yes → no tuning needed (the defaults are fine)
└── no
    ├── Lots of small/medium messages, latency budget ≥ one flush period?
    │   └── TX batching: CONFIG_NROS_ZENOH_TX_BATCH=y  (or ZPICO_TX_BATCH=1)
    │       └── still tx-bound under a tight-loop / bursty publisher?
    │           └── add CONFIG_NROS_ZENOH_TX_SPLIT_LOCK=y (ZPICO_TX_SPLIT_LOCK=1)
    ├── A specific topic needs per-sample latency < the flush period?
    │   └── declare THAT publisher express (tx_express QoS) — it bypasses
    │       the batch; keep the rest batched
    └── Everything is latency-critical at high rate?
        └── lower CONFIG_NROS_ZENOH_SOCKET_TIMEOUT_MS (e.g. 5 ms) — more
            send windows per second at the cost of read-task wakeups; batching
            still composes on top
```

## Measured effect (Zephyr native_sim, 64 B messages, 100 ms socket timeout)

Streaming (tight-loop publisher, 5000 messages, deep-ring listener):

| configuration | talker completes? | throughput | vs baseline |
| --- | --- | --- | --- |
| defaults | no (~33 s window) | ~9 msg/s | 1× |
| `TX_BATCH` | no | ~136 msg/s | ~15× |
| `TX_BATCH` + `TX_SPLIT_LOCK` | **yes (27.7 s)** | **~181 msg/s** | **~20×** |

Timer-paced tiers (100 Hz control + 10 Hz telemetry, one session, phase-279
harness): defaults 8.6 msg/s total → batch + flush thread 34.1 → + split lock
43.2, with the 10 Hz tier at its ideal rate. Delivery integrity is validated
in every configuration.

Express-vs-batched (same batching image, 200 messages at 5 ms pacing, native):
the batched topic arrives in flush-cadence bursts (21 inter-arrival gaps of
~50 ms), the express topic arrives continuously (zero gaps > 25 ms, max gap
5 ms). On Zephyr an express publisher sends each sample immediately and is
therefore paced by the socket window instead — use express for **low-rate,
latency-sensitive** topics (control setpoints, e-stop), never for streams.

## The knobs

| knob | Zephyr (Kconfig) | cargo lanes (env at build) | C/C++ cmake lanes | default |
| --- | --- | --- | --- | --- |
| socket recv window | `CONFIG_NROS_ZENOH_SOCKET_TIMEOUT_MS` | — (posix doesn't serialize per-fd) | — | 100 ms |
| tx batching | `CONFIG_NROS_ZENOH_TX_BATCH=y` | `ZPICO_TX_BATCH=1` | `-DZPICO_TX_BATCH=1` via `NROS_CMAKE_EXTRA_DEFS` | off |
| flush cadence | `CONFIG_NROS_ZENOH_TX_BATCH_FLUSH_MS` | `ZPICO_TX_BATCH_FLUSH_MS` | `-DZPICO_TX_BATCH_FLUSH_MS=<ms>` | 50 ms |
| split tx locking | `CONFIG_NROS_ZENOH_TX_SPLIT_LOCK=y` | `ZPICO_TX_SPLIT_LOCK=1` | `-DZ_FEATURE_TX_SPLIT_LOCK=1` (shared config) | off |
| per-publisher express | `tx_express` in the QoS profile (all languages, below) | same | same | off |

All of these are **compile-time** knobs (embedded images have no runtime
config), and all default **off** — an untuned build is byte-identical to
pre-279 behavior.

### What each one does

**TX batching** (`TX_BATCH`): publishes append to the transport write buffer
and ship as one socket send per flush instead of one send per put. Throughput
then scales with messages-per-flush. A dedicated flush thread (spawned
automatically on multi-threaded platforms; ThreadX stays spin-driven because
it runs no background tasks) flushes every `TX_BATCH_FLUSH_MS`. Batching adds
up to one flush period of publish latency; service requests/replies always
bypass the batch. **Do not enable for timer-paced low-rate systems** — with
≤1 message per flush period there is nothing to coalesce and the flush
overhead measurably hurts (phase-279 negative result).

**Flush cadence** (`TX_BATCH_FLUSH_MS`): bounds the extra latency batching
adds. Lower = fresher samples, more (smaller) sends. 50 ms is a good default
for telemetry; drop toward 10–20 ms if your latency budget demands it.

**Split tx locking** (`TX_SPLIT_LOCK`, requires `TX_BATCH`): the flush
*steals* the pending batch (buffer swap) and performs the socket write under a
separate link mutex, so publishers keep appending while the send is in
flight; a batch-overflow parks the full buffer instead of blocking the
publishing thread. Wire order still equals SN order. This is what lets a
tight-loop publisher actually saturate the path (~181 msg/s above). Fork-local
`Z_FEATURE_TX_SPLIT_LOCK`; it gates transport-struct fields, so it must be set
in the **shared** generated zenoh config (the build front-ends above do this
for you — never define it for a single translation unit).

**Flush-thread attributes**: platforms that need explicit task priorities /
stack sizes can call `zpico_set_flush_task_config(priority, stack_bytes)`
before session open (mirrors `zpico_set_task_config` for the read/lease
tasks; FreeRTOS applies name/priority/stack, POSIX-like platforms stack size
only).

## Per-publisher express (`tx_express`)

Marks one publisher's samples "express": they carry the zenoh EXPRESS wire
flag and bypass tx batching — sent immediately even in a batching image. A
transport hint, not a DDS policy: no RxO matching, ignored on subscriptions
and by non-batching backends.

Rust:

```rust
// builder
let pub_ = node.publisher("/cmd_vel").typed::<Twist>().tx_express(true).build()?;
// or via the QoS profile
let qos = nros::QosSettings::RELIABLE.tx_express(true);
let pub_ = node.create_publisher_with_qos::<Twist>("/cmd_vel", qos)?;
```

C:

```c
nros_qos_t qos = NROS_QOS_DEFAULT;
qos.tx_express = 1;
nros_publisher_init_with_qos(&pub, &node, "/cmd_vel", ts, &qos);
```

C++:

```cpp
auto pub = node.create_publisher<Twist>("/cmd_vel", nros::QoS().tx_express(true));
```

## Pitfalls

- **Benchmark listeners need a deep rx ring.** A batched publisher delivers a
  whole wire batch as one callback burst; the per-subscriber SPSC ring depth
  is compile-time (`ZPICO_SUBSCRIBER_RING_DEPTH`, default 4, drop-newest) —
  build measurement sinks with e.g. `ZPICO_SUBSCRIBER_RING_DEPTH=1024` or the
  numbers undercount by >10×.
- **Express ≠ fast on Zephyr.** Express bypasses the batch, so each sample
  pays the socket-window wait itself. Reserve it for topics whose *rate* is
  low but whose *latency* matters.
- **Don't flush from your own timers/executor threads.** The dedicated flush
  thread exists because flushing from tier threads stalls the very timers
  that generate the puts (phase-279 measured it slower than no batching).

Benchmarks and procedures: `packages/testing/nros-bench/stress-zenoh-zephyr/`
(streaming) and `packages/testing/nros-tests/tests/w1_zephyr_tx_throughput_measure.rs`
(timer tiers). Background: `docs/roadmap/archived/phase-279-*.md`,
`docs/roadmap/phase-282-*.md`, and
[platform implementation notes](../../../docs/reference/platform-implementation-notes.md).
