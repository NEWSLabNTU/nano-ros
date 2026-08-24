# Phase 378 — A CAN link for zenoh-rs

**Status (2026-08-25). PROPOSED — nothing started.**

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
| **W0** | Crate skeleton; `frame.rs` encoding/decoding per RFC-0081 §2; unit tests for DLC steps, length prefix, MTU refusal, own-id and mask filtering | the wire format is executable, with no socket and no root | |
| **W1** | Golden-frame tests: byte-exact buffers produced by the zenoh-pico encoder | the two implementations are one wire format, checked in CI | |
| **W2** | `sys.rs` SocketCAN binding + `multicast.rs` link and manager | a Rust process opens a CAN link and moves datagrams | |
| **W3** | `LinkKind::Can`, `LinkManagerBuilderMulticast` arm, `transport_can` feature through `zenoh-link` / `zenoh-transport` / `zenoh` | a zenoh session accepts a `can/...` endpoint | |
| **W4** | E2E on `vcan0`: two zenoh-rs peers, pub/sub, payload well above the MTU | the transport's own fragmentation drives the link, end to end | |
| **W5** | E2E interop on `vcan0`: zenoh-pico peer against zenoh-rs peer, `candump` capture | the claim that actually matters | |
| **W6** | zenoh-c feature pass-through; ROS 2 app with `RMW_IMPLEMENTATION=rmw_zenoh_cpp` and a CAN endpoint in `ZENOH_SESSION_CONFIG_URI`; `candump` under real load | the delivery chain works, and RFC-0081 §3.2 becomes a number | |
| **W7** | Per-message priority: priority reaches the link write, identifier laid out priority-major | RFC-0080 §4.2's blocker is removed on the Rust side | |

W1 is the gate that matters for interop. W4 is the gate that matters for the
link being real. **W7 is a separate upstream PR** and must not be bundled into
the others — it changes a framework trait, which is a different argument to make.

### Why golden frames before the socket

Phase-377 learned the wire format by implementing it and then had to change
`_cap._transport` after the fact. Here the format is already known, so the
cheapest place to be wrong is a unit test. W1 costs an afternoon and removes
"the two ends disagree about DLC rounding" from every later wave's diagnosis.

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

**W4.**
* Two zenoh-rs peers on `vcan0`, distinct identifiers, exchange a payload of at
  least 189 bytes — matching phase-377's figure so the two phases are
  comparable — and the subscriber receives it byte-identical.
* The test skips with an explanatory message when `vcan0` is absent; it never
  fails for that reason.
* `candump` shows traffic on both identifiers.
* A payload of at least 4 KiB also arrives intact, so fragmentation is exercised
  well past a single batch.

**W5.**
* A zenoh-pico peer publishes and a zenoh-rs peer receives, and the reverse.
* The `candump` capture is committed as the record, as phase-377 did.
* Frames-per-message matches phase-377's measured 47.3 payload bytes per frame
  within one frame, or the discrepancy is explained.

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
