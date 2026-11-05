# TX Throughput & Latency Tuning

<!-- type: how-to · level: 3 -->

*(zenoh-pico RMW — Zephyr, FreeRTOS, NuttX, ThreadX, POSIX)*

Every nano-ros language front-end (Rust, C, C++) publishes through the same
shared transport path (the zpico shim → zenoh-pico), so the knobs on this page
apply uniformly to all of them. They matter most on **Zephyr**, where the
kernel's socket layer serializes send/recv per file descriptor: the zenoh-pico
read task holds the session socket for a full receive window
(`CONFIG_NROS_ZENOH_SOCKET_TIMEOUT_MS`, default 100 ms), so **without tuning,
total image tx throughput is capped at roughly one socket send per window —
about 9 msg/s at the default**, regardless of how many publishers the image
runs or how fast they publish.

## Platform defaults

Per-platform defaults for these knobs live in each platform
package's `nros-platform.toml` (`[knobs.zenoh.tx]`), resolved through the
fixed ladder `builtin < platform < board < env/Kconfig` — an explicit
build-time setting (including an explicit `0` / `n`) always wins.

| platform | batch | split_lock | flush_ms | rationale |
| --- | --- | --- | --- | --- |
| **Zephyr** | **on** | **on** | 50 | the per-fd tx ceiling above; measured 15–20× streaming. Relies on nano-ros's zenoh-pico fork, in which declarations bypass the batch. `zephyr/Kconfig`'s `NROS_ZENOH_TX_*` defaults mirror this (drift-tested) — flip them per-app in `prj.conf` to opt out |
| everything else | off | off | 50 | no per-fd ceiling (POSIX), no flush thread (ThreadX/bare-metal), or simply unmeasured (FreeRTOS/NuttX — flip is one line in their platform toml after a bench run) |

With batching on, **non-express topics pay up to `flush_ms` of publish
latency** — declare low-rate latency-critical publishers `tx_express`
(below) to bypass the batch. Timer-paced low-rate systems that want the
old behavior image-wide: set `CONFIG_NROS_ZENOH_TX_BATCH=n` (Zephyr) /
`NROS_ZENOH_TX_BATCH=0` (cargo/cmake lanes).

## The decision tree

```text
Is total publish rate comfortably under ~1 msg per socket window?
├── yes → no tuning needed (the defaults are fine)
└── no
    ├── Lots of small/medium messages, latency budget ≥ one flush period?
    │   └── TX batching: CONFIG_NROS_ZENOH_TX_BATCH=y  (or NROS_ZENOH_TX_BATCH=1)
    │       └── still tx-bound under a tight-loop / bursty publisher?
    │           └── add CONFIG_NROS_ZENOH_TX_SPLIT_LOCK=y (NROS_ZENOH_TX_SPLIT_LOCK=1)
    ├── A specific topic needs per-sample latency < the flush period?
    │   └── declare THAT publisher express (tx_express QoS) — it bypasses
    │       the batch; keep the rest batched
    └── Everything is latency-critical at high rate?
        └── lower CONFIG_NROS_ZENOH_SOCKET_TIMEOUT_MS (e.g. 5 ms) — more
            send windows per second at the cost of read-task wakeups; batching
            still composes on top
```

## Measured effect (Zephyr native_sim, 64 B messages, 100 ms socket timeout)

> **Provenance.** Single-run measurements on one development host,
> on `native_sim` — a host-scheduled simulator, so the
> absolute msg/s figures move with host load; the *ratios* between
> rows are the durable result. Re-measure on freshly rebuilt fixtures
> on a quiet machine before citing any absolute number.

Streaming (tight-loop publisher, 5000 messages, deep-ring listener):

| configuration | talker completes? | throughput | vs baseline |
| --- | --- | --- | --- |
| defaults | no (~33 s window) | ~9 msg/s | 1× |
| `TX_BATCH` | no | ~136 msg/s | ~15× |
| `TX_BATCH` + `TX_SPLIT_LOCK` | **yes (27.7 s)** | **~181 msg/s** | **~20×** |

Timer-paced tiers (100 Hz control + 10 Hz telemetry, one session,
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
| tx batching | `CONFIG_NROS_ZENOH_TX_BATCH=y` | `NROS_ZENOH_TX_BATCH=1` | `-DZPICO_TX_BATCH=1` via `NROS_CMAKE_EXTRA_DEFS` | off |
| flush cadence | `CONFIG_NROS_ZENOH_TX_BATCH_FLUSH_MS` | `NROS_ZENOH_TX_BATCH_FLUSH_MS` | `-DZPICO_TX_BATCH_FLUSH_MS=<ms>` | 50 ms |
| split tx locking | `CONFIG_NROS_ZENOH_TX_SPLIT_LOCK=y` | `NROS_ZENOH_TX_SPLIT_LOCK=1` | `-DZ_FEATURE_TX_SPLIT_LOCK=1` (shared config) | off |
| per-publisher express | `tx_express` in the QoS profile (all languages, below) | same | same | off |

On Zephyr the Kconfig column names the symbol the build ends up with; you
state it in `system.toml`, never in `prj.conf`: a registry knob
as `[image.<id>.knobs.zenoh.tx] batch = true` or
`[image.<id>.knobs.zenoh] socket_timeout_ms = 5`, and any other option as an
image `env` row without the `CONFIG_` prefix.

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
overhead measurably hurts (a measured negative result).

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
let qos = nros::QoSProfile::RELIABLE.tx_express(true);
let pub_ = node.create_publisher_with_qos::<Twist>("/cmd_vel", qos)?;
```

C:

```c
rclc_qos_t qos = RCLC_QOS_DEFAULT;
qos.tx_express = 1;
rclc_publisher_init_with_qos(&pub, &node, "/cmd_vel", ts, &qos);
```

C++:

```cpp
auto pub = node.create_publisher<Twist>("/cmd_vel", rclcpp::QoS().tx_express(true));
```

## Pitfalls

- **Benchmark listeners need a deep rx ring.** A batched publisher delivers a
  whole wire batch as one callback burst; the per-subscriber SPSC ring depth
  is compile-time (`NROS_ZENOH_SUBSCRIBER_RING_DEPTH`, default 4). A full ring
  keeps the newest samples and evicts older ones, counted by
  `nros_rmw_zenoh::ring_full_drops_total()` — build measurement sinks with e.g.
  `NROS_ZENOH_SUBSCRIBER_RING_DEPTH=1024` or the numbers undercount by >10×.
- **Express ≠ fast on Zephyr.** Express bypasses the batch, so each sample
  pays the socket-window wait itself. Reserve it for topics whose *rate* is
  low but whose *latency* matters.
- **Don't flush from your own timers/executor threads.** The dedicated flush
  thread exists because flushing from tier threads stalls the very timers
  that generate the puts (measured slower than no batching).

The benchmarks behind these defaults live in the tree: see
[Internals → zpico-sys build architecture](../../internals/rmw/zpico-build.html#tx-tuning-benchmarks).

## Transport buffers and limits

Beyond throughput, every transport has fixed-size buffers that are sized at
build time. They are static memory, not heap: an embedded image has no
runtime configuration, and a buffer that is too small shows up as a dropped
or truncated message rather than as an allocation failure.

Most entity counts and receive slots are **derived** for each image from what
its components declare, so you do not set them; which ones, and how the
derivation works, is in
[Static Memory → Sizing the pools](../contracts/measure-static-memory.md#sizing-the-pools).
The wire buffers in this section are not derived. They depend on your network
and your largest message, so they stay yours.

Set a knob in one of three places, highest precedence first:

- the environment of one build — a value you export wins over the build's
  generated `[env]` table, which never uses `force`:

  ```bash
  NROS_ZENOH_FRAG_MAX_SIZE=1400 \
  NROS_ZENOH_BATCH_UNICAST_SIZE=1024 \
      nros build
  ```

- the image, in the workspace's `system.toml`, keyed as the knob registry
  keys it:

  ```toml
  [image.<id>.knobs.zenoh.wire]
  frag_max_size = 1400
  batch_unicast_size = 1024
  get_reply_buf_size = 1024
  ```

- the board descriptor's `[knobs]`, for a budget that belongs to the hardware
  and every image built for it.

`nros config explain` reports where each resolved value came from. The full
list of names and defaults is the
[Environment Variables Reference](../../reference/environment-variables.md#buffer-tuning).

### Zenoh-pico wire buffers

These control fragmentation and reassembly. The defaults depend on the
platform: POSIX hosts, NuttX (POSIX-compatible) and ThreadX (NetX Duo BSD
sockets) get the large defaults; bare-metal, Zephyr and FreeRTOS get the small
ones, and a serial-only image gets a 1,500-byte unicast batch.

| Variable | Image key (`[image.<id>.knobs.zenoh.wire]`) | POSIX-like | Embedded | Serial |
| --- | --- | --- | --- | --- |
| `NROS_ZENOH_FRAG_MAX_SIZE` | `frag_max_size` | 65535 | 2048 | 2048 |
| `NROS_ZENOH_BATCH_UNICAST_SIZE` | `batch_unicast_size` | 65535 | 1024 | 1500 |
| `NROS_ZENOH_BATCH_MULTICAST_SIZE` | `batch_multicast_size` | 8192 | 1024 | 1024 |

- `NROS_ZENOH_FRAG_MAX_SIZE` limits the largest message your node can
  **receive**. A message larger than this is dropped silently.
- `NROS_ZENOH_BATCH_UNICAST_SIZE` limits the largest message your node can
  **send** without fragmentation; a larger one is split into fragments.
- All batch sizes are capped at 65535 (the protocol's 16-bit limit).
- On an MTU-constrained network (Ethernet 1500, Zephyr's default of about
  1400), keep the batch sizes below the network MTU to avoid IP fragmentation.

### Zenoh-pico service buffers

| Variable | Default | What it holds |
| --- | --- | --- |
| `NROS_ZENOH_SERVICE_BUFFER_SIZE` | 1024 | per-service-server receive buffer |
| `NROS_ZENOH_GET_REPLY_BUF_SIZE` | 4096 | service-client reply buffer |
| `NROS_ZENOH_GET_POLL_INTERVAL_MS` | 10 | service-call polling interval (ms) |
| `NROS_ZENOH_MAX_PENDING_GETS` | 4 | concurrent in-flight service calls; each holds one reply buffer |

Size a buffer to the CDR-serialized size of the largest message the entity
receives. CDR adds alignment padding per field, typically 4–8 bytes;
variable-length fields are often smaller on the wire than their in-memory
bound. The per-subscriber receive slots are derived from the message types
the image subscribes to; see
[Static Memory](../contracts/measure-static-memory.md#sizing-the-pools).

### Zenoh session constants

These are fixed in the generated zenoh-pico configuration and have no knob:

| Constant | Value | Meaning |
| --- | --- | --- |
| `Z_TRANSPORT_LEASE` | 60,000 ms | transport lease; matches the lease the ROS 2 zenoh router announces |
| `Z_TRANSPORT_LEASE_EXPIRE_FACTOR` | 3 | lease expiry multiplier |

A peer that dies with its socket closed is detected at once; the lease only
bounds detection of a peer that vanishes without closing its connection.

### smoltcp (bare metal)

A bare-metal image on the smoltcp TCP/IP stack sizes its socket pools at
build time. The defaults suit a brokered client (zenoh or XRCE), which
multiplexes everything over one session: one TCP and one UDP socket, or four
UDP sockets with the `nros-smoltcp/rtps` feature. The knobs are
`NROS_SMOLTCP_MAX_SOCKETS`, `NROS_SMOLTCP_MAX_UDP_SOCKETS`,
`NROS_SMOLTCP_BUFFER_SIZE` (per-socket staging buffer, 2048),
`NROS_SMOLTCP_CONNECT_TIMEOUT_MS` (30,000) and
`NROS_SMOLTCP_SOCKET_TIMEOUT_MS` (10,000), keyed `net.*` in an image's or
board's `[knobs]` (`[image.<id>.knobs.net] max_sockets = 1`).

### XRCE-DDS

The XRCE backend's transport buffers are set by `NROS_XRCE_*` knobs. The
plain `XRCE_*` names are the C `#define`s those knobs produce; exporting one
has no effect and reports no error.

- `NROS_XRCE_TRANSPORT_MTU` is the UDP/TCP transport MTU (4096; 512 on
  Zephyr) and the default of `NROS_XRCE_CUSTOM_TRANSPORT_MTU`.
- The per-session stream buffers are `MTU × NROS_XRCE_STREAM_HISTORY`. The
  history depth defaults to 16 and must be at least 4.
- `NROS_XRCE_BUFFER_SIZE` (1024) is the receive ceiling: a sample larger than
  it is refused on take, whatever buffer the subscription asked for.

The entity caps derive from the image's declarations. The session timeouts
(entity creation, service reply) are compile-time constants with no knob. Both
are listed in the
[Environment Variables Reference](../../reference/environment-variables.md#xrce-dds-nros_xrce_).

### When a transport limit is hit

| Symptom | Knob |
| --- | --- |
| Large messages never arrive | raise `NROS_ZENOH_FRAG_MAX_SIZE`: the message exceeds the reassembly limit |
| A subscription receives truncated data, or a large-type subscription fails to create | the receive slot is too small for the serialized message; see [Static Memory](../contracts/measure-static-memory.md#sizing-the-pools) for `NROS_SUBSCRIBER_BUFFER_SIZE` and the large class |
| Service calls time out with large replies | raise `NROS_ZENOH_GET_REPLY_BUF_SIZE` |
| Service-call latency is too high | lower `NROS_ZENOH_GET_POLL_INTERVAL_MS`, at a higher CPU cost |
| Several concurrent service calls fail | raise `NROS_ZENOH_MAX_PENDING_GETS` |
| An entity create fails at run time | a pool is full; the [Static Pool Inventory](../../reference/static-pool-inventory.md) names the knob that sizes it |
| smoltcp connect times out | raise `NROS_SMOLTCP_CONNECT_TIMEOUT_MS` on a slow network, and check the router is reachable at the image's `locator` (`NROS_LOCATOR` on a native host) |
| XRCE entity creation times out | check `NROS_XRCE_STREAM_HISTORY` is at least 4, and that the agent is running and reachable |

### Compared with Cyclone DDS on Zephyr

For reference, a published port of Autoware actuation to Zephyr with Cyclone
DDS ([actuation_porting](https://github.com/oguzkaganozt/actuation_porting))
uses these transport settings:

| Cyclone DDS setting | Value | zenoh-pico counterpart | Notes |
| --- | --- | --- | --- |
| `ReceiveBufferSize` | 16 KB | none | zenoh-pico uses per-entity buffers instead of a shared pool |
| `ReceiveBufferChunkSize` | 2 KB | `NROS_SUBSCRIBER_BUFFER_SIZE` | per entity in zenoh-pico, shared chunks in Cyclone DDS |
| `MaxMessageSize` | 1400 B | `NROS_ZENOH_BATCH_UNICAST_SIZE` | an MTU-aware limit to avoid IP fragmentation |
| `AllowMulticast` | SPDP only | the `link-udp-multicast` feature | off by default in both |

That port sizes Zephyr's network stack for Cyclone DDS's dynamic allocation
(`CONFIG_NET_PKT_RX_COUNT=32`, `CONFIG_NET_PKT_TX_COUNT=32`,
`CONFIG_NET_BUF_RX_COUNT=256`, `CONFIG_NET_BUF_TX_COUNT=256`,
`CONFIG_NET_BUF_DATA_SIZE=1600`, `CONFIG_NET_MAX_CONN=200`) and gives Cyclone
DDS a 1 MB heap. The zenoh-pico wire and entity buffers on this page are sized
at build time instead. The zenoh-pico session's own working set still comes
from a heap; on bare metal that heap is sized by `NROS_HEAP_SIZE` (see
[Configuration](../build/configure-an-image.md#binary-size-knobs-embedded)).

## Design background

- [RFC-0049: hierarchical platform and board config](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0049-hierarchical-platform-board-config.md)
- [RFC-0103: one owner per fact](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0103-one-owner-per-fact.md) — the `NROS_<ID>` knob spelling and the knob registry
