# Phase 378 — A CAN link for zenoh-rs

**Status (2026-08-25). W0-W6 DONE. W7 IMPLEMENTED AND BLOCKED — the mechanism
works and cannot be switched on.**

**Two ROS 2 nodes talk to each other over CAN with no router and no TCP at all**,
and a ROS 2 node publishes to a zenoh-pico peer over the same bus — with the
stock `rmw_zenoh_cpp` binary and no ROS rebuild, only a substituted
`libzenohc.so`. The RFC-0081 §3.2 bus question is
now a measurement, and the answer is worse than the RFC allowed for: the
mitigation it predicted does not exist.

**A zenoh-pico peer and a zenoh-rs peer exchange pub/sub across `vcan0`, both
directions, including a 189-byte payload over a 63-byte MTU.** That is the claim
the whole phase exists to make. Getting there needed one change on the pico
side, described below, and none to the wire format.

Two zenoh-rs peers exchange pub/sub across `vcan0` with 189-byte and 4 KiB
payloads over a 63-byte MTU, every message byte-identical, so zenoh's own
fragmentation is driving the link. **One frame carries 47.25 payload bytes**,
against the 47.3 phase-377 measured with two zenoh-pico peers — the two
implementations agree to within 0.1%.

The link exists, registers, and builds: `cargo test -p zenoh-link-can` passes
41 tests with no `vcan0` and no root, `cargo check -p zenoh --features
transport_can` succeeds, default builds are unchanged, and clippy is clean. The
golden frames are byte-exact against the zenoh-pico encoder, so the two
implementations are pinned to one wire format before any socket was involved.

**The work now targets zenoh 1.8.0, not 1.10.0.** The installed ROS packages —
`ros-humble-rmw-zenoh-cpp` 0.1.9 and `ros-humble-zenoh-cpp-vendor` 0.1.9 — ship
`libzenohc.so` at **zenoh-c 1.8.0**, and `rmw_zenoh_cpp` is compiled against
those headers, so the link has to be built on the zenoh the ROS side actually
uses. Branch `feat/can-link-1.8`, cut from `release/1.8.0`; `feat/can-link`
keeps the 1.10.0 line for the eventual upstream PR.

The port cost almost nothing, which is itself a result: `LinkMulticastTrait`,
`LinkKind` and `LinkManagerBuilderMulticast::make` are **identical** across the
two minors. Two conflicts, both version-pin noise in `Cargo.toml` files. All 42
unit tests, W4 and W5 pass unchanged on 1.8.0.

Implements [RFC-0081](../design/0081-can-link-for-zenoh-rs.md), which is the
host half of [RFC-0080](../design/0080-can-link-for-zenoh-pico.md). Phase-377
delivered the MCU end: two zenoh-pico peers exchange pub/sub over `vcan0` with a
189-byte payload across a 63-byte MTU. RFC-0080 §8 left the Linux end unscoped
and this phase closes it.

**Related:** phase-377 (the pico link, and the wire format this must reproduce),
RFC-0079 (identifier allocation is the same problem in a second address space).

---

## 1. Shape

The link is **self-contained in a fork of `eclipse-zenoh/zenoh`, shaped as an
upstream PR**. nano-ros gains no Rust transport code.

```
zenoh:   io/zenoh-links/zenoh-link-can/src/frame.rs      ← the wire format, no I/O
         io/zenoh-links/zenoh-link-can/src/sys.rs        ← SocketCAN, Linux only
         io/zenoh-links/zenoh-link-can/src/multicast.rs  ← LinkMulticastCan + manager
         io/zenoh-links/zenoh-link-can/src/lib.rs        ← prefix, config keys, inspector
         io/zenoh-link/src/lib.rs                        ← LinkKind::Can registration
         zenoh/Cargo.toml, io/zenoh-transport/Cargo.toml ← transport_can pass-through

zenoh-c: Cargo.toml + CMake                              ← transport_can pass-through
```

## 2. Waves

Ordered so each wave is provable by the one before it, and so **the wire format
is pinned by tests before any socket exists**.

| | What | Proves | State |
| --- | --- | --- | --- |
| **W0** | Crate skeleton; `frame.rs` encoding/decoding per RFC-0081 §2; unit tests for DLC steps, length prefix, MTU refusal, own-id and mask filtering | the wire format is executable, with no socket and no root | **done, 41 tests pass** |
| **W1** | Golden-frame tests: byte-exact buffers produced by the zenoh-pico encoder | the two implementations are one wire format, checked in CI | **done** |
| **W2** | `sys.rs` SocketCAN binding + `multicast.rs` link and manager | a Rust process opens a CAN link and moves datagrams | **done** |
| **W3** | `LinkKind::Can`, `LinkManagerBuilderMulticast` arm, `transport_can` feature through `zenoh-link` / `zenoh-transport` / `zenoh` | a zenoh session accepts a `can/...` endpoint | **done** |
| **W4** | E2E on `vcan0`: two zenoh-rs peers, pub/sub, payload well above the MTU | the transport's own fragmentation drives the link, end to end | **done, passes** |
| **W5** | E2E interop on `vcan0`: zenoh-pico peer against zenoh-rs peer, `candump` capture | the claim that actually matters | **done, passes** |
| **W6** | zenoh-c feature pass-through; ROS 2 app with `RMW_IMPLEMENTATION=rmw_zenoh_cpp` and a CAN endpoint in `ZENOH_SESSION_CONFIG_URI`; `candump` under real load | the delivery chain works, and RFC-0081 §3.2 becomes a number | **done, passes** |
| **W7** | Per-message priority: priority reaches the link write, identifier laid out priority-major | RFC-0080 §4.2's blocker is removed on the Rust side | **built, blocked below the feature** |

W1 is the gate that matters for interop. W4 is the gate that matters for the
link being real. **W7 is a separate upstream PR** and must not be bundled into
the others — it changes a framework trait, which is a different argument to make.

### Why golden frames before the socket

Phase-377 learned the wire format by implementing it and then had to change
`_cap._transport` after the fact. Here the format is already known, so the
cheapest place to be wrong is a unit test. W1 costs an afternoon and removes
"the two ends disagree about DLC rounding" from every later wave's diagnosis.

### What W0-W3 actually produced

```
io/zenoh-links/zenoh-link-can/src/frame.rs      the wire format, no I/O, 24 tests
io/zenoh-links/zenoh-link-can/src/lib.rs        endpoint grammar and inspector, 17 tests
io/zenoh-links/zenoh-link-can/src/sys.rs        SocketCAN over tokio's AsyncFd
io/zenoh-links/zenoh-link-can/src/multicast.rs  LinkMulticastCan + manager
io/zenoh-transport/tests/multicast_can.rs       W4, three #[ignore]d tests
```

Three findings from building it, none of which were visible from the RFC:

**A `Locator` cannot carry config.** `From<EndPoint> for Locator` truncates at
the `#` separator (`locator.rs:81`), silently. The peer address the link reports
through `read()` is a `Locator`, so an identifier written as config would vanish
and every peer would look alike to the multicast transport. Peer and group
locators therefore carry their identifiers in the locator **metadata**. The user
facing endpoint grammar is unchanged — it is still `can/vcan0#id=0x100` — because
that is an `EndPoint`, which does keep its config.

**`InterceptorLink` is not optional.** Adding `LinkAuthId::Can` breaks two
deliberately wildcard-free matches in `zenoh` (`interceptor/mod.rs:84`,
`api/info.rs:338`), so `zenoh-config` needs an `InterceptorLink::Can` variant for
`zenoh` to compile at all. A side benefit: `"can"` is now a valid ACL
`link_protocols` value, which is exactly the lever RFC-0081 §3.2 would need if
the bus turns out to flood.

**Two deviations from zenoh-pico, both deliberate.** Identifiers above `0x7FF`
are refused rather than silently truncated on the wire, and a malformed config
value is an error rather than a quiet fall back to the default. Neither breaks a
configuration that works today; both turn a silent priority misconfiguration
into a startup error.

### W4 results (2026-08-25)

Three tests in `io/zenoh-transport/tests/multicast_can.rs`, all passing on
`vcan0`. Peers take distinct identifiers — `0x100`/`0x101` — because a peer
drops frames carrying its own identifier, so two peers sharing one `id` would
each discard everything the other sent.

| payload | messages | result |
| ---: | ---: | --- |
| 189 B, Data priority | 100 | 100/100, 0 corrupt |
| 189 B, RealTime priority | 100 | 100/100, 0 corrupt |
| 4 KiB | 100 | 100/100, 0 corrupt |

`candump` for the 189-byte runs, 200 messages:

| | frames |
| --- | ---: |
| `[64]` full fragments on `0x100` | 600 |
| `[48]` final fragments on `0x100` | 200 |
| `[20]` Join on `0x100` / `0x101` | 4 / 4 |
| `[03]` close on `0x100` / `0x101` | 2 / 2 |

**Exactly 4 frames per message**, so 189 / 4 = **47.25 payload bytes per frame**
and 63 − 47.25 = **15.75 bytes per frame not carrying payload** — phase-377's
"about 16 bytes" of per-fragment overhead, arrived at independently.

The final fragment declares 33 bytes in a 48-byte frame: 14 bytes of DLC
padding, visible on the wire, which is exactly the case the length prefix exists
for. **The `Join` message is 18 bytes** and needs one 20-byte frame, so
RFC-0081's concern that it might not fit a 63-byte MTU is settled — it fits with
room to spare, though only because multicast `is_qos` defaults false.

### The measurement that was not one, and the defect underneath it

The test first reported 50-59 of 100 messages arriving. That was **not** loss:
the assertion loop, copied from the UDP multicast test, stops at
`count != 0`, so the figure was whatever had arrived by the time it stopped
looking. Replacing it with a settle loop gave 100/100.

That fix exposed a real defect underneath. 100 messages of 4 KiB is 7 100
frames, and a virtual bus delivers them as fast as memory allows. The kernel
dropped the overflow before the link saw it: **31% of messages lost, no error
reported anywhere**, `ip -s link` showing zero drops because the loss is above
the interface. An 8 MiB receive buffer took it to 100%.

A real bus cannot do this — 2 Mbit/s of CAN FD is under 2 800 frames per second,
and the reader keeps up comfortably — so the default is unchanged and the knob
is opt-in: `so_rcvbuf`, spelled as the TCP and UDP links spell it. The kernel
clamps the request to `net.core.rmem_max` without saying so, so the link reads
the value back and warns when it fell short.

Worth carrying into W6: the failure mode is silence. Nothing in zenoh, the
kernel, or `candump` reported a problem; only counting the messages that should
have arrived did.

### W5 results (2026-08-25)

A zenoh-pico peer built from the vendored tree with `Z_FEATURE_LINK_CAN=1`,
against a zenoh-rs peer built from the fork, on one `vcan0`. Real sessions with
16-byte zids, key expressions and declarations — not transport-level harnesses.

| direction | payload | result |
| --- | ---: | --- |
| zenoh-rs → zenoh-pico | 31 B | 11/11 |
| zenoh-pico → zenoh-rs | 31 B | 11/11 |
| zenoh-pico → zenoh-rs | 189 B, fragmented | 10/10, every payload exactly 189 bytes |

**The fragmented run is the strong result.** 40 data frames for 10 messages —
30 × `[64]` plus 10 × `[48]` — is **exactly 4 frames per message**, which is the
same frame count zenoh-rs produces for the same payload, and the same 47.25
payload bytes per frame that phase-377 measured at 47.3. Two independent
implementations fragmenting a 189-byte message into the same four frames is
better evidence that the wire format is one format than any amount of code
reading.

### The interop blocker, and why it is not the link

The first attempt failed. The pico side logged:

```
[INFO ::_z_multicast_handle_join_inner] Couldn't accept peer because distant node is incompatible config wise.
[ERROR ::_zp_multicast_process_messages] Dropping message due to processing error: -101
```

and the zenoh-rs side logged **nothing at all** — it had no reason to; its frames
were being sent and acknowledged by the bus.

Decoding both `Join` frames out of `candump` by hand settles where the fault is.
zenoh-pico's, on `0x200`:

```
1B E7 09 F1 <16-byte zid> 0A 00 08 0A 00 00 27 01
```

zenoh-rs's, on `0x100`:

```
21 E7 09 F1 <16-byte zid> 0A 3F 00 0A <sn varints> 27 01
```

Byte 0 is the length prefix, `E7` the Join header, `09` the protocol version,
`F1` the whatami-and-zid-length byte, `0A` the resolution byte. **Every field
matches except one:** the two bytes after the resolution byte are the batch
size — pico sends `00 08`, which is 2048, and zenoh-rs sends `3F 00`, which is
**63**.

`src/transport/multicast/rx.c` rejects on exact inequality:

```c
if ((msg->_seq_num_res != Z_SN_RESOLUTION) || (msg->_req_id_res != Z_REQ_RESOLUTION) ||
    (msg->_batch_size != Z_BATCH_MULTICAST_SIZE)) {
```

`Z_BATCH_MULTICAST_SIZE` is a compile-time constant defaulting to 2048, and pico
advertises it **regardless of the MTU of the link underneath**. zenoh-rs
advertises `min(configured batch size, link MTU)`, which on CAN FD is 63.

Rebuilding zenoh-pico with `-DBATCH_MULTICAST_SIZE=63` made both directions work
immediately, with no change to the wire format or to either link.

**This is worth carrying back to phase-377.** A zenoh-pico peer on a CAN bus
advertising a 2048-byte batch is not merely an interop inconvenience — it is
incoherent on its own terms, because the link beneath it cannot carry 63 bytes
in one frame let alone 2048. The advertised batch should be derived from the
link MTU rather than fixed at compile time. Until it is, **every zenoh-pico
image that is meant to speak CAN must set `BATCH_MULTICAST_SIZE` to the CAN
MTU**, and that belongs in the island's build configuration rather than in a
README somewhere.

Note also the follow-on error, `Invalid zid length received`, which appears once
after the rejection. It is a consequence of the dropped association, not a
second fault; chasing it first would have wasted the afternoon.

### The 1.8.0 port (2026-08-25)

| check | 1.10.0 | 1.8.0 |
| --- | --- | --- |
| `cargo test -p zenoh-link-can` | 42 pass | 42 pass |
| W4, 189 B and 4 KiB | 100/100, 0 corrupt | 100/100, 0 corrupt |
| W5, zenoh-rs → zenoh-pico | works | works |
| W5, zenoh-pico → zenoh-rs, 189 B fragmented | works | works, payloads exactly 189 B |
| clippy, default build | clean | clean |

**One behaviour differs between the two minors, and it matters.** RFC-0081 §3.1
found that on 1.10 multicast-group faces exist only in the `peer` hat, so a
router builds the face and silently never routes to it. **On 1.8 the router hat
does route to multicast groups** — `hat/router/pubsub.rs:1211` inserts the group
into the data route exactly as the peer hats do — so a CAN endpoint on
`ZENOH_ROUTER_CONFIG_URI` would work on the version ROS ships.

Prefer the session anyway. The unfiltered-route problem (§3.2) applies to both
hats and is far worse on a router: a peer session forwards only what it
publishes, a router forwards the whole graph. But the original instinct to put
CAN on the router was **not wrong on 1.8** — it is wrong on 1.10, and a design
that assumed either version would be wrong on the other.

### W7 results (2026-08-25): the mechanism works, and cannot be switched on

**What was built.** `LinkMulticastTrait` gains a defaulted
`write_all_with_priority`, so no other link changes and no signature breaks. The
multicast TX task passes the priority it already holds — `pipeline.pull()`
returns `(batch, priority)` and it was being discarded one call later, which is
the whole of RFC-0080 §4.2's "the link is priority-blind" on the Rust side.
`Join`, `KeepAlive` and `Close` go out at `Control` so session traffic cannot
lose the bus to bulk data.

The identifier splits into a class field of `prio_bits` bits at the top and a
peer field below. Two properties matter and both are unit-tested:

* **Class dominates arbitration**, because it sits at the most significant end.
  The highest-numbered peer at `Control` outranks the lowest-numbered peer at
  `Background`.
* **Nothing is inverted.** zenoh numbers `Control` 0 and `Background` 7, CAN
  gives the bus to the lowest identifier, so the two orderings already agree.

Peer identity is recovered by masking the class off. Without that, one peer
transmitting at eight priorities would look like eight peers, and would hear its
own frames. `prio_bits` defaults to 0 — the wire zenoh-pico speaks — and the
interop was re-run to confirm the default is unchanged.

**Why it cannot be switched on.** Mapping priority onto the wire only does
anything if the transport keeps one queue per priority, because otherwise every
batch reports queue index 0 and every frame goes out in class 0. That was the
first observation: with QoS off, `candump` showed 806 frames on `0x00A` and
nothing else.

Turning multicast QoS on makes `Join` carry eight `PrioritySn` instead of one.
`Join` is written as a **single un-fragmented datagram**. Measured, in a test
that needs no bus:

| | bytes |
| --- | ---: |
| `Join` without QoS | **33** |
| `Join` with per-priority SNs | **99** |
| CAN FD MTU | **63** |

The 33 is corroborated: it is exactly the `Join` length observed on the wire in
W5. So enabling QoS kills the transmit task before the session starts, and the
bus stays silent — which is what the first attempt did, with a 60-second timeout
and zero frames captured.

**This is a protocol limit, not a link limit.** 99 bytes does not fit a CAN FD
frame under any framing: the frame is 64 bytes. The ways out are all above the
link — fragmenting `Join`, or a compact QoS extension — and none is in this
phase's reach. RFC-0081 §3.3 called W7 "a genuine capability"; it is, and it is
also unreachable on CAN FD as the protocol stands today.

`join_with_qos_does_not_fit_one_can_frame` records both numbers and **fails if
the `Join` ever shrinks enough to fit**, which is the day this becomes usable.
That is deliberately a failing-open test rather than a comment.

### W6 results (2026-08-25)

`ros2 run demo_nodes_cpp talker` under `RMW_IMPLEMENTATION=rmw_zenoh_cpp`, with a
CAN listen endpoint in its session config, publishing to a zenoh-pico peer on
`vcan0`. The stock `rmw_zenoh_cpp` and `rmw_zenohd` binaries, unmodified.

| topic | messages the island received over CAN |
| --- | ---: |
| `0/chatter/std_msgs::msg::dds_::String_/…` | 17 |
| `0/rosout/rcl_interfaces::msg::dds_::Log_/…` | 17 |
| `0/parameter_events/…ParameterEvent_/…` | 5 |

**No ROS rebuild was needed.** `librmw_zenoh_cpp.so` and `rmw_zenohd` carry
`libzenohc.so` as a plain `DT_NEEDED` with **no RPATH and no RUNPATH**, and the
vendored library has **no `DT_SONAME`**, so prepending a directory to
`LD_LIBRARY_PATH` after sourcing `setup.bash` substitutes it wholesale. Adding a
cargo feature changes no C API, so the ABI is unchanged.

The build recipe, with the trap in it:

```sh
# zenoh-c 1.8.0, matching ros-humble-zenoh-cpp-vendor 0.1.9
# Cargo.toml: transport_can = ["zenoh/transport_can"], plus a
#   [patch."https://github.com/eclipse-zenoh/zenoh.git"] pointing at the fork.
# THE TRAP: build-resources/opaque-types/ has its OWN manifest and is handed the
#   parent's Cargo.lock. Without the same patch there, the two disagree about
#   where zenoh comes from, the size-probe build yields nothing, and the failure
#   surfaces much later and unrecognisably as
#   "no sigatures found for building generic z_take_from_loaned".
cargo build --release --features unstable,shared-memory,transport_can
```

The feature set must match what the vendored library was built with —
`/opt/ros/humble/opt/zenoh_cpp_vendor/include/zenoh_configure.h` lists it, and
`unstable` and `shared_memory` are the two that move struct layouts. Transport
features do not.

### ROS to ROS over CAN, with a control

The run above proves ROS → zenoh-pico. It does **not** prove two ROS nodes
talking to each other over CAN, because both attach to the same local
`rmw_zenohd` over TCP and would have used that path regardless. Tested
separately, with the TCP path removed entirely: no router process, `connect`
emptied, and a CAN endpoint as the **only** `listen` endpoint.

| | talker published | listener heard |
| --- | ---: | ---: |
| both peers in one band | 19 | **19** |
| **control:** peers in disjoint bands (`match`/`mask`) | 19 | **0** |

The control is what makes the first row mean anything. Both processes ran
normally in both cases and both kept transmitting — 248 frames from the talker,
46 from the isolated listener — so the silence is the CAN link's identifier
filter separating them, not a crash or a misconfiguration. If any other path
existed, the listener would still have heard.

**And a third sighting of §3.2**: the talker emitted **248 frames in both runs**,
identical, whether or not anything on the bus could hear it.

### More than two peers, and what limits "many" (2026-08-26)

Every test up to here had been a pair, which cannot exercise peer identity: with
two peers, "the other one" is unambiguous however the address is derived.

**Four zenoh peers on one bus.** Each tracks the other three, told apart only by
the identifier in the frames they send. One publisher, three subscribers, all
three receive **100/100 intact**, and the publisher hears nothing of itself. The
test asserts the peer count **exactly**, not at-least — see the footgun below.

**Three ROS 2 nodes on one bus**, CAN-only, no router: talker published 17, both
listeners heard **17/17**.

Convergence takes one `join_interval` (2.5 s), not the instant a peer opens: a
peer learns about those already present only when the next periodic `Join` comes
round, so immediately after startup the peer counts are a staircase by open
order. That is expected, and worth knowing before someone reads it as a bug.

#### What actually limits the number of peers

Measured, three idle peers over a 20-second window: **24 frames, 8 per peer** —
exactly one `Join` per peer per 2.5 s.

| | per peer |
| --- | ---: |
| steady-state discovery | **0.4 frames/s** = 0.013% of a 500k/2M CAN FD bus |
| one-time declaration burst, per ROS node | ~170 frames ≈ 57 ms of airtime |

**Discovery is not the limit.** At 2 793 frames/s of bus capacity, per-peer
keepalive alone would not saturate until thousands of peers. Twenty nodes all
starting at once cost about a second of solid bus, once.

The real limits, in the order they bite:

1. **Data volume, made worse by §3.2.** Every peer's *entire* publication set
   crosses the bus whether anything subscribes or not. Bus load is the sum of
   what all peers publish, and there is no filtering. This is the binding
   constraint and the only one that scales badly.
2. **Every peer decodes every frame.** With `mask=0` each node's CPU sees all
   bus traffic. On an MCU that matters well before bandwidth does.
3. **Physical bus limits.** Transceiver loading caps a segment at tens of nodes,
   independently of any of this.
4. **Identifier space is not a limit.** 11 bits give 2 048 addresses with
   `prio_bits=0`, 256 with `prio_bits=3`.

#### The practical scheme for many peers: bands

`match`/`mask` partition one physical bus into **independent zenoh networks**.
This was proved as the negative control for the ROS-to-ROS test: two peers in
disjoint bands, both transmitting, neither hearing the other.

That gives a real deployment pattern. Put each functional group in its own
identifier band; a node then decodes only its own group's traffic (limit 2), and
groups do not merge into one flat zenoh network (limit 1). It does **not** save
bus bandwidth — the frames are still on the wire — so it addresses processing
and blast radius, not airtime.

Combined with the rule that a lower identifier wins arbitration, a band layout
is also a priority layout, which is RFC-0079's problem in this address space and
should be allocated rather than authored ad hoc.

#### A footgun worth writing down

The `ros2` CLI spawns a **background daemon that inherits
`ZENOH_SESSION_CONFIG_URI`**. One `ros2 node list` left a daemon sitting on the
CAN bus for half an hour after the command returned, emitting `Join`s under the
identifier from a throwaway config. It was found only because the four-peer test
asserts an exact peer count and reported four remotes where three were expected.

On a vehicle bus this is a silent extra participant holding an identifier — and
therefore an arbitration priority — that nobody allocated.

### What does NOT work over CAN, and why it is not the link's fault

Tested after the ROS-to-ROS result, because "two nodes exchange a topic" is a
much smaller claim than "ROS 2 works over CAN".

| ROS 2 feature | over CAN | evidence |
| --- | --- | --- |
| Topics (pub/sub) | **works** | 19/19, plus a negative control |
| Services | **does not** | `add_two_ints_client`: "service not available" forever |
| Actions | **does not** | built on services |
| Parameters | **does not** | built on services |
| Graph introspection | **does not** | `ros2 node list` empty; `ros2 topic list` shows only the CLI's own topics, not `/chatter` |

The cause is one line of routing, and it is architectural:

```sh
$ for f in zenoh/src/net/routing/hat/*/*.rs; do grep -c mcast_groups $f; done
hat/linkstate_peer/pubsub.rs: 1
hat/p2p_peer/pubsub.rs:       2
hat/router/pubsub.rs:         1
```

`mcast_groups` appears **only in `pubsub.rs`**, in every hat. `queries.rs`,
`token.rs` and `interests.rs` never mention multicast. **A zenoh multicast
transport carries pushed data and nothing else** — no queries, no liveliness
tokens, no interests. rmw_zenoh builds the ROS graph from liveliness tokens and
resolves services through queries, so both die.

This is not a CAN limitation and no CAN link can fix it. It is what
`Z_LINK_CAP_TRANSPORT_MULTICAST` costs, and RFC-0080 chose multicast because
unicast could not complete a handshake on a datagram medium. The choice was
right and this is its price.

**For the safety island specifically**, the traffic that matters — stop
commands, MRM state, gear and mode reports, subscribed telemetry — is all
pub/sub, and pub/sub works. Anything reaching for a service, an action or a
parameter across the CAN segment does not, and that is a design constraint to
plan around rather than a bug to wait on.

### The §3.2 measurement, and the mitigation that is not there

Two runs, identical in every way except the island peer's subscription:

| island subscribes to | messages it received | frames on the bus |
| --- | ---: | ---: |
| `**` | 39 | **233** |
| `island/**` (matches nothing) | 0 | **233** |

**Byte-identical frame counts.** The island paid the full bus cost for traffic it
had not asked for and could not use. That is RFC-0081 §3.2 stated as a number:
bus load is the total output of whatever session holds the CAN link, and the
island's subscriptions do not enter into it.

At 336 µs per full CAN FD frame (RFC-0080's model), 226 frames over the talker's
17 seconds is **0.45% bus load** — trivial, because a 1 Hz talker is trivial.
The number that matters is the ratio, and it was 0% useful.

**The fix RFC-0081 predicted does not exist.** An ACL naming
`link_protocols: ["can"]` — which parses only because this link registers an
`InterceptorLink` variant — loads cleanly and changes nothing. Reading why:

```rust
/// zenoh/src/net/routing/interceptor/access_control.rs:849
fn new_transport_multicast(&self, _transport: &TransportMulticast) -> Option<EgressInterceptor> {
    tracing::debug!("Transport Multicast is disabled in interceptor");
    None
}
```

`access_control.rs`, `downsampling.rs`, `low_pass.rs` and `qos_overwrite.rs` all
return `None` for both directions on a multicast transport. **No interceptor
runs on a multicast face at all**, so none of the config-level levers —
filtering, downsampling a bulk topic, QoS rewriting — is available to a CAN link.

That is worth stating plainly because downsampling in particular looked like the
answer to RFC-0080 §8's Odometry problem, and it is not.

What remains:

* **Bound the publication set by topology.** Put the CAN link on a purpose-built
  bridge node that publishes only what should cross, not on a general-purpose
  application session. Available today; a design decision, not a fix.
* **Fix it upstream.** Either make the route to a multicast group respect
  subscriptions, or enable interceptors on multicast faces. The second is much
  the smaller change and would give `link_protocols: ["can"]` the meaning it
  already appears to have.

### What W6 looked like beforehand

Reading zenoh-c 1.8.0 and 1.10.0 shrank this wave considerably:

* `transport_can = ["zenoh/transport_can"]` beside `transport_vsock` is the
  entire feature change;
* **no CMake change** — `ZENOHC_CARGO_FLAGS` already passes cargo flags through;
* zenoh-c pins zenoh **by git branch**, not crates.io, so repointing it at the
  fork is four lines.

And because a cargo feature adds no C API, a 1.8.0 `libzenohc.so` built with
`transport_can` should be substitutable beneath the **stock** `rmw_zenoh_cpp`
with no ROS rebuild at all. That is the W6 hypothesis and it is cheap to test.

The earlier claim that this wave was blocked on cloning repositories was wrong
in its reasoning even though the conclusion held at the time: the blocker was
never zenoh-c, it was that the fork was on 1.10.0 while the ROS stack was on
1.6.2. Reinstalling the ROS packages moved that to 1.8.0 and porting the link
closed the gap.



## 3. Acceptance criteria

**W0.**
* `cargo test -p zenoh-link-can` passes with no `vcan0` present and no root.
* A test asserts every FD DLC step boundary: payloads of 7, 8, 11, 12, 15, 16,
  19, 20, 23, 24, 31, 32, 47, 48, 62, 63 encode to frame lengths of
  8, 9, 12, 12, 16, 16, 20, 20, 24, 24, 32, 32, 48, 48, 64, 64.
* A test asserts a 64-byte payload is refused in FD mode and an 8-byte payload
  is refused in classic mode.
* A test asserts `decode(encode(x)) == x` for every length 0..=63.
* A test asserts a frame whose `data[0]` exceeds `frame.len - 1` is rejected.

**W1.**
* At least six golden buffers, covering an empty datagram, a sub-8 datagram, a
  datagram that lands exactly on a DLC step, one that must round up, a
  full-MTU datagram, and a classic-mode datagram.
* Each golden buffer is annotated with the zenoh-pico expression that produced
  it, so a future reader can regenerate it.
* Byte-exact equality, including `flags` and the trailing padding bytes.

**W2.**
* Opening a link on a missing interface returns an error naming the interface.
* `id`, `match` or `mask` above `CAN_SFF_MASK` is rejected at open with a
  message that says why (RFC-0081 §2.1).
* An `id` outside its own `match`/`mask` band is rejected at open.
* A classic-mode fallback emits a warning that names the MTU it fell back to.
* `get_src()`, `get_dst()` and the `read()` locator are all parseable `can/...`
  locators.

**W3.**
* `cargo build --no-default-features` and a default build are both unaffected.
* `cargo build -p zenoh --features transport_can` succeeds.
* `cargo clippy --all-targets --features transport_can -- -D warnings` is clean.
* A session configured with a `can/...` listen endpoint opens a multicast
  transport rather than erroring with "Multicast not supported for link".

**W4.** All met.
* ~~Two zenoh-rs peers on `vcan0`, distinct identifiers, exchange a payload of at
  least 189 bytes — matching phase-377's figure so the two phases are
  comparable — and the subscriber receives it byte-identical.~~ 100/100, 0 corrupt.
* ~~The test skips with an explanatory message when `vcan0` is absent; it never
  fails for that reason.~~ Verified by running it with no interface present.
* ~~`candump` shows traffic on both identifiers.~~ 808 frames on `0x100`,
  8 on `0x101`.
* ~~A payload of at least 4 KiB also arrives intact, so fragmentation is exercised
  well past a single batch.~~ 100/100, 0 corrupt, with `so_rcvbuf` raised.

**W5.** All met.
* ~~A zenoh-pico peer publishes and a zenoh-rs peer receives, and the reverse.~~
  11/11 both directions.
* ~~The `candump` capture is committed as the record, as phase-377 did.~~ Frame
  counts recorded above, following phase-377's practice of recording the numbers
  rather than the raw log.
* ~~Frames-per-message matches phase-377's measured 47.3 payload bytes per frame
  within one frame, or the discrepancy is explained.~~ 47.25, from an identical
  4-frames-per-message split — not within one frame, identical.

**W6.** All met, plus a ROS-to-ROS case the criteria did not ask for.
* `zenoh-c` builds with the feature and `rmw_zenoh_cpp` links against it.
* A ROS 2 application with `RMW_IMPLEMENTATION=rmw_zenoh_cpp` and a CAN endpoint
  in its **session** config — not its router config, RFC-0081 §3.1 — publishes a
  topic that a zenoh-pico peer on the bus receives.
* The same application still reaches other ROS 2 applications on the host over
  its TCP link to `rmw_zenohd`.
* A `candump` capture under real application load reports **what fraction of bus
  traffic is data the island subscribes to** versus data forwarded to the
  multicast face unconditionally. This is the RFC-0081 §3.2 measurement and it
  is the deliverable of the wave, not a side note.

**W7.** Two of three met; the third is blocked twice over.
* ~~Priority reaches `LinkMulticastTrait` without changing any other link's
  behaviour — a defaulted trait method, not a breaking signature change.~~
  `write_all_with_priority`, defaulted to `write_all`.
* ~~An identifier layout in which class dominates from the MSB.~~ `prio_bits`,
  default 0.
* **A measurement showing an urgent message overtaking a bulk burst from the
  same peer.** NOT MET, and not by an oversight — see below. `vcan` does not
  arbitrate, so it could never have shown this; and the feature cannot be
  enabled at all, so there is nothing to measure even on hardware yet.

## 4. Test method

Same tiering as phase-377, adapted.

**Tier 0 — unit.** `frame.rs`, no socket. Runs everywhere including CI.

**Tier 1 — golden.** Byte-exact pico-sourced buffers. Runs everywhere.

**Tier 2 — `vcan`, one laptop.** The workhorse.

```sh
sudo modprobe vcan
sudo ip link add dev vcan0 type vcan
sudo ip link set up vcan0
candump -td vcan0
```

**Tier 3 — interop.** zenoh-pico peer against zenoh-rs peer on the same `vcan0`.

**Tier 4 — the system.** MR-CANHUBK344 across a transceiver to a Linux host.
Shared with phase-377 W6; one hardware session validates both ends.

## 4b. Feature completeness

What is done, what is blocked, and what is merely untested. Blocked items have
been diagnosed to a specific line; untested ones have not been attempted.

**Blocked above the link, all diagnosed:**

* **Queries and liveliness do not route to a multicast face**, so ROS services,
  actions, parameters and graph introspection do not work over CAN. Needs
  routing changes in `queries.rs` / `token.rs`.
* **Routes to a multicast group ignore subscriptions** (§3.2), so bus cost is
  the publisher's whole output. No interceptor runs on a multicast face either,
  so there is no configuration lever.
* **W7 priority mapping cannot be switched on**: `Join` with per-priority SNs is
  99 bytes against a 63-byte MTU.

**Implemented but never exercised:**

* ~~**Classic CAN**, MTU 7.~~ **Removed.** It is refused at parse (`dbitrate=0`)
  and at open (an interface that will not negotiate FD), and a classic frame on
  the bus is now treated as another device's traffic rather than decoded. A
  7-byte MTU against ~16 bytes of per-fragment overhead is not a slow link, it
  is a non-functional one, and there was no point chasing a below-MTU job.
* ~~**More than two peers on one bus.**~~ **Done**: four zenoh peers and three
  ROS 2 nodes, above.
* **Bus-off and interface-down recovery.** The read path fails the transport on
  error; whether a session recovers when a controller returns has not been
  tested, and on a vehicle bus it will happen.
* **Real hardware.** Tier 4 is untouched in both phases. Every latency and
  bandwidth figure so far is either analytic or measured on `vcan`, which has no
  bit rate and no arbitration.
* **Reliability semantics.** The link declares itself best-effort and the
  multicast transport does not retransmit, so a ROS `RELIABLE` topic is not
  reliable end to end across a CAN segment. On `vcan` delivery was 100%, which
  proves nothing about a loaded bus.

**Delivery hygiene:**

* `feat/can-link` (the 1.10.0 line for upstream) does **not** carry W4-W7; only
  `feat/can-link-1.8` is current.
* The zenoh-c patch and the built `libzenohc.so` exist only in a scratchpad.
* No upstream PR has been opened.

## 5. Risks

**The bus floods before the link does.** RFC-0081 §3.2: a multicast face is
inserted into every route with no subscriber matching, so the host's whole
publication set crosses CAN. W6 measures it. If it is bad, the fix is upstream
in `hat/peer/pubsub.rs` or an egress interceptor — not in the link.

**A classic-mode fallback is a silent cliff.** MTU 7 against ~16 bytes of
per-fragment overhead does not merely slow down, it cannot make progress. W2's
warning is the mitigation and it is an acceptance criterion for that reason.

**Identifier defaults are an unallocated priority ordering.** Inherited verbatim
from RFC-0080 §4.2 and not improved by this phase. Two peers that both take the
default `0x100` are misconfigured in a way nothing detects.

**The zenoh-c hop is a second repository.** W6 is the first wave that cannot be
done entirely in the zenoh fork, and it is where schedule risk concentrates.
