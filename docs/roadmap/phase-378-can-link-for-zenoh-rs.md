# Phase 378 — A CAN link for zenoh-rs

**Status (2026-08-25). W0-W5 DONE. W6-W7 not started.**

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
| **W6** | zenoh-c feature pass-through; ROS 2 app with `RMW_IMPLEMENTATION=rmw_zenoh_cpp` and a CAN endpoint in `ZENOH_SESSION_CONFIG_URI`; `candump` under real load | the delivery chain works, and RFC-0081 §3.2 becomes a number | unblocked, not started |
| **W7** | Per-message priority: priority reaches the link write, identifier laid out priority-major | RFC-0080 §4.2's blocker is removed on the Rust side | not started |

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

### What W6 now looks like

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

**W6.**
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

**W7.**
* Priority reaches `LinkMulticastTrait` without changing any other link's
  behaviour — a defaulted trait method, not a breaking signature change.
* An identifier layout in which class dominates from the MSB.
* A measurement showing an urgent message overtaking a bulk burst **from the
  same peer**, which is precisely what phase-377 §3b could not achieve.

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
