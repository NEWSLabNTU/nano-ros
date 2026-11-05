# Tune throughput and latency

<!-- type: how-to · level: 3 -->

**Needs first:** [Configure zenoh](configure-zenoh.md)

Use this page when a zenoh image publishes more than its transport sends, a
latency-critical topic arrives late, or large messages and service replies
do not arrive at all.

Every language front end, Rust, C and C++, publishes through the same
zenoh-pico transmit path, so these settings apply to all three. They matter
most on Zephyr, whose socket layer serializes sends and receives on one
socket: the zenoh-pico read task holds the session socket for a whole
receive window (`NROS_ZENOH_SOCKET_TIMEOUT_MS`, 100 ms by default), so an
untuned image sends about one batch per window, about 9 messages a second,
however many publishers it has. All of these are build-time settings; an
embedded image has no run-time configuration.

## Before you start

- A zenoh image that builds, and a measure of its publish rate and the
  latency each topic can take.
- The platform's defaults. Zephyr builds with batching and split locking on;
  every other platform builds with both off.

## Steps

1. Read the values your platform resolves, and the rung that set each one
   (`builtin`, `platform`, `board` or `env`):

   ```bash
   nros config explain --platform zephyr
   ```

   ```text
   knob                     value      set by
   ----                     -----      ------
   zenoh.tx.batch           true       platform
   zenoh.tx.split_lock      true       platform
   zenoh.tx.flush_ms        50         platform
   ```

2. Turn on transmit batching for an image that publishes many small or
   medium messages and can wait one flush period. Publishes then collect in
   the transport's write buffer and leave as one socket send per flush:

   ```toml
   [image.posix.knobs.zenoh.tx]
   batch = true
   ```

   A flush thread sends the batch every `flush_ms`; ThreadX has no
   background tasks, so there the executor's spin sends it. Service requests
   and replies never wait in the batch. Do not batch a timer-paced image
   that publishes less than once per flush period: there is nothing to
   combine, and the flushes make it slower.

Timer-paced sched tasks (100 Hz control + 10 Hz telemetry, one session,
harness): defaults 8.6 msg/s total → batch + flush thread 34.1 → + split lock
43.2, with the 10 Hz sched task at its ideal rate. Delivery integrity is validated
in every configuration.

   ```toml
   [image.posix.knobs.zenoh.tx]
   batch = true
   split_lock = true
   ```

   On Zephyr `native_sim`, 64-byte messages from a tight loop went from about
   9 a second untuned to about 136 with batching and about 181 with both.
   Those are single runs on one development host; the ratios hold, the
   absolute numbers move with the host.

4. Bound the latency batching adds. A non-express sample waits up to
   `flush_ms`; lower it toward 10 to 20 ms when your budget needs fresher
   samples, at the cost of more, smaller sends:

   ```toml
   [image.posix.knobs.zenoh.tx]
   batch = true
   flush_ms = 20
   ```

5. Mark a low-rate, latency-critical publisher express, such as a control
   setpoint or an emergency stop. Its samples skip the batch and leave at
   once, while the rest of the image stays batched. Express is a transport
   hint, not a DDS QoS policy: subscriptions and backends that do not batch
   ignore it.

<!-- tabs -->

### Rust

```rust
let pub_ = node.publisher("/cmd_vel").typed::<Twist>().tx_express(true).build()?;
```

### C

```c
rclc_qos_t qos = RCLC_QOS_DEFAULT;
qos.tx_express = 1;
rclc_publisher_init_with_qos(&pub, &node, type_info, "/cmd_vel", &qos);
```

### C++

```cpp
auto pub = node.create_publisher<Twist>("/cmd_vel", rclcpp::QoS().tx_express(true));
```

<!-- /tabs -->

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
  thread exists because flushing from sched task threads stalls the very timers
  that generate the puts (measured slower than no batching).

6. If every topic is latency-critical at a high rate, shorten the socket
   receive window, `NROS_ZENOH_SOCKET_TIMEOUT_MS` (on Zephyr
   `CONFIG_NROS_ZENOH_SOCKET_TIMEOUT_MS`), to 5 ms or so. That gives
   more send windows a second at the cost of more read-task wake-ups, and
   batching still works on top.

### Set the wire buffers

The wire buffers decide the largest message an image can send and receive.
They depend on your network and your largest message, so the build does not
derive them. Set them per image:

```toml
[image.posix.knobs.zenoh.wire]
frag_max_size = 1400
batch_unicast_size = 1024
get_reply_buf_size = 1024
```

or for one build, in its environment, where an exported value wins:

```bash
NROS_ZENOH_FRAG_MAX_SIZE=1400 NROS_ZENOH_BATCH_UNICAST_SIZE=1024 nros build
```

or in the board descriptor's `[knobs]`, when the budget belongs to the
hardware. `frag_max_size` is the largest message the image can receive; a
larger one is dropped. `batch_unicast_size` is the largest it sends without
fragmenting. On a network with a 1500-byte MTU, or Zephyr's default of about
1400, keep the batch sizes below the MTU. A serial image gets a 1500-byte
unicast batch by default.

## Example

A Linux host image that streams telemetry batched every 20 ms and sends its
command topic express:

```toml
# src/host_node_bringup/system.toml
[system]
name = "host_node"
rmw = "zenoh"
ros_edition = "humble"
domain_id = 0

[[component]]
pkg = "host_node"
class = "host_node::Talker"
name = "host_node"

[image.posix]
board = "linux"

[image.posix.knobs.zenoh.tx]
batch = true
split_lock = true
flush_ms = 20
```

with the command publisher built `tx_express(true)` as in step 5. Build it
with `nros build posix`.

## If it doesn't work

| What you see | What to change |
| --- | --- |
| Large messages never arrive | raise `frag_max_size`: the message exceeds the reassembly limit |
| A subscription receives truncated data, or a large-type subscription fails to create | the receive slot is too small; see [Measure static memory](../contracts/measure-static-memory.md#sizing-the-pools) |
| Service calls with large replies time out | raise `get_reply_buf_size` |
| Several concurrent service calls fail | raise `NROS_ZENOH_MAX_PENDING_GETS` |
| An entity create fails at run time | a pool is full; the [Static pool inventory](../../reference/static-pool-inventory.md) names the knob that sizes it |
| A benchmark listener counts far fewer messages than were sent | its receive ring is too shallow for a batch arriving at once; build the listener with `NROS_ZENOH_SUBSCRIBER_RING_DEPTH=1024` |
| Batching made a timer-paced image slower | turn `batch` off: at one message per flush there is nothing to combine |

Do not flush from your own timers or executor threads. Flushing from a tier
thread stalls the timers that produce the samples, and measured slower than
no batching.

## See also

- [Environment variables](../../reference/environment-variables.md#zenoh-pico-transmit-path):
  every transmit knob and its Zephyr, cargo and CMake spelling, and the
  [buffer knobs](../../reference/environment-variables.md#buffer-tuning) of
  zenoh-pico, smoltcp and XRCE-DDS.
- [CLI reference](../../reference/cli.md#nros-config): `nros config explain`.
- [Use XRCE-DDS](use-xrce-dds.md#steps): sizing an XRCE-DDS session.
- [Static memory and pools](../../background/static-memory-and-pools.md):
  why these buffers are fixed at build time.

## Design background

- [RFC-0049: hierarchical platform and board config](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0049-hierarchical-platform-board-config.md)
- [RFC-0103: one owner per fact](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0103-one-owner-per-fact.md)
