# Size an image from its contract

<!-- type: how-to · level: 2 -->

**Needs first:** [the contract walkthrough](../../tutorial/tell-the-build.md#write-the-contract) ·
[Project layout](../../tutorial/grow-your-workspace.md)

Write a contract that describes every node of an image when you want the build
to size that image's static pools from what it creates, instead of from the
backend's defaults.

Every pool in a nano-ros image is a fixed array: a slot costs its bytes whether
an entity uses it or not. The build cannot count your entities from the code,
because the crates that hold the pools compile before your components do. So it
reads the count from the contract, once per image, before anything is compiled,
and writes the answers where those crates read them. The steps are the same for
Rust, C and C++ images.

## Before you start

- An image that builds and runs with no contract, so you have a baseline.
- Every node the image launches, and every publisher, subscription, service,
  client, action and timer each one creates. A count over only some of them is
  refused, not guessed at (see step 3).

## Steps

1. **Build without a contract and note the resolve line.**

   ```bash
   nros build linux
   ```

   With nothing to count from, the build keeps the configured defaults:

   ```text
   nros build:   resolved → <workspace>/build/<bringup>__linux/resolved.toml (entity counts not derived; configured defaults kept)
   ```

   If you can, measure this image's static RAM now
   ([Measure static memory](measure-static-memory.md)), so you can show the
   saving afterwards.

2. **Write the contract.** Put it beside the launch file, as
   `<bringup>/launch/<stem>.contract.yaml` for `<stem>.launch.xml`, or beside
   `system.toml` for a bringup with no `launch/`
   ([Contracts for a bringup with no launch file](contracts-for-a-bringup-with-no-launch-file.md)).
   Declare each node's endpoints under `nodes:`, keyed by their local names; a
   timer is a `paths:` entry whose trigger is `{ timer: { rate_hz: N } }`. Wire
   each endpoint to its absolute name under `topics:`, `services:` or
   `actions:`, with its type. To size the receive queues too, declare each
   endpoint's depth ([Declare QoS in a contract](declare-qos-in-a-contract.md)).

3. **Rebuild and read the resolve line.**

   ```bash
   nros build linux
   ```

   ```text
   nros build:   resolved → <workspace>/build/<bringup>__linux/resolved.toml (max_cbs <n>, nodes <n>, digest <digest>)
   ```

   `max_cbs` is the executor's callback table. A publisher claims no callback
   slot, so it is smaller than the number of entities you declared; that is
   expected. If the line still says `entity counts not derived`, the contract
   does not describe some component: see
   [Fix a contract refusal](fix-a-contract-refusal.md).

4. **Read what was derived.** Open the `resolved.toml` the line names.
   `[executor]` holds the callback, node and action counts, `[pools]` the RMW
   session's publisher, subscriber, queryable and liveliness counts, and
   `[provenance]` says where each number came from.

5. **Measure again.** Rebuild with the memory report, as in step 1, and compare
   the two images symbol by symbol
   ([Showing a saving](measure-static-memory.md#showing-a-saving)).

## Example

The Rust listener in
[`examples/linux/rust/listener`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/linux/rust/listener)
runs one subscription. Its `system.contract.yaml` declares that subscription
with `KEEP_LAST(1)`:

```yaml
version: 1

nodes:
  listener:
    sub:
      chatter:
        qos:
          history: keep_last
          depth: 1

topics:
  /chatter:
    type: std_msgs/msg/String
    sub: [listener/chatter]
```

With it, the zenoh backend sizes the subscriber's payload pools for the one
sample the image keeps instead of the four its ring would hold by default. The
image's static RAM falls from 273,802 to 172,298 bytes on Linux.

## Which knobs the build derives

The pools that dominate static RAM are derived per image from what the
contract declares. The build writes the answers into its generated cargo
`[env]` table (on Zephyr, into the Kconfig defaults). Stating one of them by
hand is an override, not a requirement.

| Knob | Derived from |
| --- | --- |
| `NROS_ZENOH_MAX_PUBLISHERS`, `NROS_ZENOH_MAX_SUBSCRIBERS` | the declared entity inventory, floored at 1: each sizes a fixed C array, where zero is not a smaller pool |
| `NROS_ZENOH_MAX_QUERYABLES` | the declared service servers, plus the parameter and lifecycle services the image's nodes register; with no readable declaration it stays at its undeclared budget (8 embedded, 32 hosted) |
| `NROS_ZENOH_MAX_LIVELINESS` | one per node name plus the session's own, one per publisher, subscriber, service server and client, and the parameter and lifecycle families' entities |
| `NROS_EXECUTOR_MAX_CBS`, `NROS_EXECUTOR_ACTION_CLIENTS`, `NROS_RMW_SUBSCRIBER_SLOTS` | the declared entity inventory |
| `NROS_SUBSCRIBER_BUFFER_SIZE` | the largest *small*-class message type the image subscribes to |
| `NROS_ZENOH_SUBSCRIBER_LARGE_SIZE` | the largest *large*-class type; stated only when the large count is non-zero |
| `NROS_ZENOH_MAX_LARGE_SUBSCRIBERS` | how many subscribed types fall in the large class. Zero is an answer, not a floor: the pool becomes zero bytes |
| `NROS_SUBSCRIPTION_BUFFER_SIZE` | the executor's take buffer, from the same message bounds |
| `NROS_EXECUTOR_ARENA_SIZE` | the same inventory, in the executor's own build; the variable is the override |

The transport's wire buffers (`NROS_ZENOH_FRAG_MAX_SIZE`, the batch sizes,
`NROS_ZENOH_SERVICE_BUFFER_SIZE`, `NROS_ZENOH_GET_REPLY_BUF_SIZE`,
`NROS_ZENOH_MAX_PENDING_GETS`), the smoltcp socket pools and the parameter
store limits are not derived: they are yours. They are covered in
[Tune throughput and latency](../rmw/tune-throughput-and-latency.md#transport-buffers-and-limits).

**Precedence.** A value you state wins over a derived one: an exported
environment variable first, then the image's `[image.<id>.knobs]`, then the
board descriptor's `[knobs]`. The derived value comes next, and the crate's
own default applies only when nothing is stated and nothing can be derived.

## If it doesn't work

- **The derived buffer size is larger than any message you send.** The
  message-bound sizes (`NROS_SUBSCRIBER_BUFFER_SIZE`, the large class,
  `NROS_SUBSCRIPTION_BUFFER_SIZE`) come from every type in the interface
  closure the image links, not only the types it subscribes to, so they are an
  upper bound: never too small, sometimes larger than you need. The build names
  the type that set the size. When that type is not one of yours, state your
  own value or trim the package's `<depend>` list.
- **Nothing message-bound is derived, and the build names an unbounded type.**
  One unbounded type refuses the whole message-bound derivation, and the stated
  or default values stand. Bound the field in the `.msg` (`string<=64`) or cap
  it `inline` in `nros-codegen.toml`; one cap on a declaring type such as
  `"std_msgs/Header.frame_id"` reaches every message that nests it.
- **Entity creation fails at boot after you trimmed the contract.** Each slot
  is exactly what the contract declares, with no headroom, so a contract that
  leaves out an endpoint the code creates makes a pool one short. On an image
  for a board other than the host, the census check catches this before the
  build: see [Fix a contract refusal](fix-a-contract-refusal.md).

## See also

- [Static pool inventory](../../reference/static-pool-inventory.md): every
  sizing knob, its default and what it costs at that default.
- [Measure static memory](measure-static-memory.md): what your image
  actually reserved.
- [Contract format reference](https://github.com/NEWSLabNTU/ros-launch-manifest/blob/v0.3.12/docs/format-reference.md):
  every contract key, generated from the grammar this release pins.
- [Static memory and pools](../../background/static-memory-and-pools.md): why
  nano-ros pools are fixed-size.
