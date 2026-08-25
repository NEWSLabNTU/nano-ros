---
rfc: 0081
title: "A CAN link for zenoh-rs: the host half of RFC-0080"
status: Draft
since: 2026-08
last-reviewed: 2026-08-25
implements-tracked-by: [phase-378]
supersedes: []
superseded-by: null
---

# RFC-0081 — A CAN link for zenoh-rs

> RFC-0080 §8 closes with: *"The host side is a second implementation. A CAN
> link in zenoh-pico covers MCU-class peers. A Linux host running zenoh-rs needs
> a matching link in Rust, sharing this wire format. That work is real and is not
> scoped here."* This RFC scopes it. **[OPEN]** marks unresolved points.

## 1. Position

**The link belongs in zenoh-rs as `io/zenoh-links/zenoh-link-can`, shaped for
upstream.**

zenoh-rs has no out-of-tree link plugin path. `LinkKind` in
`io/zenoh-link/src/lib.rs` is a closed enum, and
`LinkManagerBuilderMulticast::make()` matches exactly one variant:

```rust
match link_kind {
    #[cfg(feature = "transport_udp")]
    LinkKind::Udp => Ok(std::sync::Arc::new(LinkManagerMulticastUdp)),
    _ => bail!("Multicast not supported for link {link_kind:?}"),
}
```

Any CAN link must therefore patch `zenoh-link` itself. Given that the patch is
unavoidable, the choice is whether to carry it as a private fork delta or to
shape it as upstream would want it. We shape it for upstream: a new crate beside
`zenoh-link-udp`, a `transport_can` feature that defaults off, and no change to
any framework interface. The pico link is already upstreamable on its own
merits (RFC-0080 §1); this is the same argument in the other repository.

Consequences we want:

* the delta against `eclipse-zenoh/zenoh` is one crate plus a handful of
  `#[cfg(feature = "transport_can")]` arms, so rebasing across zenoh releases is
  mechanical;
* every zenoh-rs consumer gets it, `rmw_zenoh_cpp` included;
* nano-ros carries no Rust transport code, exactly as it carries no C transport
  code.

## 2. The wire is already decided

RFC-0080 settled the frame format and it is not reopened here. Restated
normatively, because this implementation must reproduce it byte for byte:

| | |
| --- | --- |
| socket | `socket(PF_CAN, SOCK_RAW, CAN_RAW)`, bound to the interface index |
| identifier | `frame.can_id` = this peer's `id`, written raw, **no `CAN_EFF_FLAG`** |
| byte 0 | the true datagram length |
| bytes 1..N | the datagram |
| frame length | `len + 1`, rounded up to the next of {12, 16, 20, 24, 32, 48, 64} when in FD mode and `len + 1 > 8` |
| flags | `CANFD_BRS` whenever the socket is in FD mode |
| write size | `CANFD_MTU` (72) in FD mode, `CAN_MTU` (16) in classic |
| MTU | 63 in FD mode, 7 in classic |
| filter | one `can_filter { can_id: match, can_mask: mask }` |
| receive | drop frames whose sender equals our own `id`; drop when `mask != 0 && (sender & mask) != match`; drop when `frame.len < 1`; drop when `data[0] > frame.len - 1` |

Classic mode is wire-compatible despite the sender always populating a
`canfd_frame`: the first 16 bytes of `struct canfd_frame` overlay
`struct can_frame` such that `len` lands on `can_dlc` and `flags` — zero in
classic mode — lands on `__pad`.

### 2.1 Extended identifiers are out of scope, and we fail loudly

The pico sender writes `frame.can_id = sock->_id` with no `CAN_EFF_FLAG`, so
only 11-bit identifiers are actually expressible; a configured `id` above
`0x7FF` silently becomes a different identifier on the wire. The Rust link
**rejects `id`, `match` and `mask` above `CAN_SFF_MASK` at open time** rather
than reproducing that. This is not a wire change — every configuration that
works today keeps working — it only turns a silent misdelivery into a startup
error.

**[OPEN]** 29-bit identifiers are the natural home for a priority-major layout
(§4.2), and adopting them is a change both implementations must make together.

## 3. Three things reading zenoh-rs changed

### 3.1 Whether CAN can attach to a router is version-dependent

**On zenoh 1.10**, multicast-group faces exist only in the `peer` hat.
`hat/peer/mod.rs:152` defines `multicast_groups()`; `hat/router/` and
`hat/client/` never mention `mcast_groups`. A runtime in `mode: "router"` —
which is what `rmw_zenohd` is — builds the face in `gateway.rs:357` and then
never routes anything to it. Nothing errors, logs, or warns; the symptom is
silence.

**On zenoh 1.8**, which is what the ROS packages actually ship (§4.7), the
router hat does route to multicast groups:
`hat/router/pubsub.rs:1211` inserts the group into the data route exactly as the
peer hats do, and the hats are `router`, `linkstate_peer`, `p2p_peer` and
`client`. So a CAN endpoint on `ZENOH_ROUTER_CONFIG_URI` works there.

**Prefer the session anyway.** §3.2 applies to both hats, and it is far worse on
a router: a peer session forwards only what it publishes, while a router
forwards the whole graph's traffic onto the bus. The CAN endpoint therefore
belongs in `ZENOH_SESSION_CONFIG_URI` with `mode: "peer"`, and a ROS 2
application session holds two links — TCP to the local `rmw_zenohd` for the
other applications on the host, and CAN for the island.

Recording the difference because it is a behaviour change between two zenoh
minors, and a design that assumed either one would be wrong on the other.

### 3.2 A multicast face receives every route, unfiltered

```rust
// zenoh/src/net/routing/hat/peer/pubsub.rs:305 — HACK(regions)
for group in self.multicast_groups(tables) {
    route.insert(group.id, || { ... });
}
```

There is no subscriber matching. Every `Put` the session routes crosses the CAN
bus whether any island peer subscribes or not. `pubsub.rs:410` does the
same for `DeclareSubscriber`, and it sends `res.expr().to_string()` — the full
topic name as text, not an interned numeric id.

On a bus whose usable payload is 1.41 Mbit/s (RFC-0080 §8) this, not the link,
is the dominant bandwidth risk. It is also **upstream behaviour we inherit**,
not something the link can fix from below without a layering violation.

**Position: measure it, do not pre-solve it.** Phase-378 W6 captures `candump`
under the real ROS application and reports what actually crosses. Only then is
it clear whether the answer is an egress interceptor, an upstream fix in the
peer hat, or nothing at all because the application's publication set is small.

### 3.3 zenoh-rs can do what zenoh-pico structurally cannot

RFC-0080 §4.2 names the blocker: *"zenoh's link is priority-blind, and that is
the blocker. `_z_f_link_write(self, ptr, len, socket)` has no priority
argument."*

In Rust the priority is in hand at the moment of the write:

```rust
// io/zenoh-transport/src/multicast/link.rs, tx_task
Some((mut batch, priority)) => {
    link.send_batch(&mut batch).await?;
```

`LinkMulticastTrait::write_all(&self, buffer: &[u8])` is what discards it. Since
a CAN identifier **is** the arbitration priority, plumbing that argument through
converts zenoh QoS into real bus priority — the thing RFC-0080 §4.2 said
allocation alone could not deliver.

That is a genuine capability, and it is deliberately **not** in the first
deliverable. It changes a framework trait, which makes it a much harder upstream
argument than "a new link crate beside the existing ones". It is a second,
separately justified PR. See §4.2 and phase-378 W7.

## 4. Design

### 4.1 Crate layout

```
io/zenoh-links/zenoh-link-can/
  Cargo.toml
  src/lib.rs         locator prefix, config keys, CanLocatorInspector, endpoint accessors
  src/frame.rs       the wire format of §2, with no I/O in it
  src/multicast.rs   LinkMulticastCan, LinkManagerMulticastCan
  src/sys.rs         the SocketCAN binding (Linux only)
```

`frame.rs` holds no socket and no `async`, so every claim in §2 is a unit test
that runs anywhere — no `vcan0`, no root, no CI capability. That split is the
main reason the port is cheap to trust.

### 4.2 Endpoint grammar

Unchanged from RFC-0080, because the two ends must agree:

```
can/<device>#bitrate=500000;dbitrate=2000000;id=0x100;match=0;mask=0
```

`bitrate` and `dbitrate` are advisory on Linux — rates are set out of band with
`ip link set` and a virtual interface has none — but `dbitrate=0` still selects
classic CAN, and the link reports the mode it actually obtained rather than the
one requested.

`id` is a real-time decision, not a name: a **lower identifier wins
arbitration**, priority is **per peer** today, and the defaults are a starting
point rather than an allocation. All of RFC-0080 §4.2's warnings apply
unchanged and belong in the crate documentation, not only here.

### 4.3 Addressing, and where the two implementations legitimately differ

zenoh-pico reports the sender as four little-endian bytes in an `addr` slice.
zenoh-rs's trait is

```rust
async fn read<'a>(&'a self, buffer: &mut [u8]) -> ZResult<(usize, Cow<'a, Locator>)>;
```

so the Rust link reports `can/<device>#id=0x101` instead. **This is not a wire
difference.** In both implementations the peer address is derived locally from
`frame.can_id` and never transmitted; only the multicast transport's own
`Join`/`Frame` messages cross the bus, and those are identical. `get_src()` is
this peer's identifier locator, `get_dst()` is the group locator that
`open_transport_multicast` keys the transport by.

### 4.4 Async strategy

`tokio::io::unix::AsyncFd` over a non-blocking raw fd, with `libc` for the
`PF_CAN` specifics. Both crates are already workspace dependencies
(`zenoh-link-vsock` takes `libc`, `zenoh-link-udp` takes both), so the link adds
no new dependency to zenoh.

The `socketcan` crate was considered and rejected: it would be a new upstream
dependency, it pulls a netlink stack we do not need because bit rates are set
out of band, and the parts we do need are three `setsockopt` calls.

### 4.5 MTU and mode negotiation

`CAN_RAW_FD_FRAMES` is requested only when `dbitrate != 0`. If the interface
refuses it we fall back to classic framing rather than failing, and the link's
MTU becomes 7 rather than 63 — matching RFC-0080's rule that the MTU tracks the
mode obtained, not the mode requested. Declaring 63 on a classic interface would
truncate every frame.

**A 7-byte MTU is legal but close to useless**, since zenoh's per-fragment
overhead is about 16 bytes (RFC-0080 §8) — larger than the MTU itself. The link
therefore **warns loudly** when it falls back, because the failure mode is
otherwise a session that merely appears to hang.

### 4.6 What changes outside the crate

| file | change |
| --- | --- |
| `Cargo.toml` | workspace member, `zenoh-link-can` dependency entry |
| `io/zenoh-link/Cargo.toml` | optional dependency, `transport_can` feature |
| `io/zenoh-link/src/lib.rs` | `LinkKind::Can`, prefix arm, inspector field, `LinkManagerBuilderMulticast` arm |
| `io/zenoh-transport/Cargo.toml` | `transport_can` feature pass-through |
| `zenoh/Cargo.toml` | `transport_can` feature pass-through |
| `DEFAULT_CONFIG.json5` | documentation of the endpoint form |

`transport_can` defaults **off** and is Linux-only. Nothing about a build that
does not ask for it changes.

### 4.7 The delivery chain

`rmw_zenoh_cpp` does not consume zenoh-rs directly. The chain is

```
zenoh (Rust, this RFC)  →  zenoh-c  →  zenoh-cpp  →  rmw_zenoh_cpp
```

`zenoh-c` turned out to be nearly free, which an earlier draft of this section
overstated. Checked against zenoh-c 1.8.0 and 1.10.0:

* its `Cargo.toml` already carries one line per transport, so
  `transport_can = ["zenoh/transport_can"]` beside `transport_vsock` is the
  whole feature change;
* **no CMake change is needed** — `ZENOHC_CARGO_FLAGS` passes arbitrary cargo
  flags through, so `-DZENOHC_CARGO_FLAGS="--features=transport_can"` suffices;
* it pins zenoh **by git branch**, not crates.io
  (`zenoh = { version = "1.8.0", git = "...", branch = "release/1.8.0" }`), so
  repointing it at a fork is four lines.

**The version is what costs.** The installed ROS stack determines which zenoh
this work must target:

| | version |
| --- | --- |
| `ros-humble-rmw-zenoh-cpp` | 0.1.9 |
| `ros-humble-zenoh-cpp-vendor` → `libzenohc.so` | **1.8.0** |

`rmw_zenoh_cpp` is compiled against those headers, so the link must be built on
zenoh **1.8.0** for the resulting `libzenohc.so` to be substitutable. A feature
addition does not change the C ABI — `transport_can` adds no C API — so a 1.8.0
`libzenohc.so` built with the feature on should drop in beneath the stock
`rmw_zenoh_cpp` without rebuilding the ROS packages. That is the hypothesis
phase-378 W6 tests.

## 5. Testability

The property RFC-0080 relied on holds here too: **the whole link runs on one
laptop with no hardware.**

```sh
sudo modprobe vcan
sudo ip link add dev vcan0 type vcan
sudo ip link set up vcan0
```

Four tiers, cheapest first:

1. **Frame unit tests** — `frame.rs` against §2. No socket, no root, runs in CI.
2. **Golden-frame tests** — byte-exact buffers taken from the pico encoder, so
   an interop regression fails in a unit test rather than on a bus. This is the
   test that makes the two implementations one wire format instead of two.
3. **Link E2E on `vcan0`** — two Rust zenoh sessions, pub/sub, a payload well
   above the MTU so the transport's own fragmentation drives the link.
4. **Interop E2E on `vcan0`** — a zenoh-pico peer against a zenoh-rs peer, which
   is the claim that actually matters, with `candump` as the record.

Tiers 3 and 4 need `vcan0` and therefore root to set up. They **skip with a
clear message** rather than fail when it is absent, so a developer without it
still gets tiers 1 and 2.

## 6. What this does not solve

**Bus flooding (§3.2) is measured, not fixed.** The link is not the right layer
for it.

**Per-message priority (§3.3) is not in this deliverable.** It is phase-378 W7
and a separate upstream PR.

**Extended identifiers (§2.1) stay rejected** until both implementations adopt
them together.

**No hardware.** RFC-0080's W6 — MR-CANHUBK344 across a real transceiver — is
still the only thing that turns the analytic airtime model into a measurement,
and it covers both implementations at once.

## Exploration log

**2026-08-25 — the router is the wrong place, and only reading the hats showed
it.** The obvious reading of "run a ROS 2 app over CAN" is to give the router a
CAN endpoint, because that is where a bridge belongs. Grepping `mcast_groups`
returns exactly two files, both under `hat/peer/`. The face is created for a
router perfectly happily and simply never appears in a route. Recorded because
nothing errors, logs or warns — the symptom is silence.

**2026-08-25 — the multicast face is unconditional.** Found while checking
whether declaration traffic would fit the bus. `route.insert(group.id, ...)`
sits outside every matching test, under a `HACK(regions)` comment. It makes the
bandwidth question a property of the host's whole publication set rather than of
the island's subscriptions, which is the opposite of what the RFC-0080 §8
bandwidth analysis assumed.

**2026-08-25 — Rust has the priority that C does not.** RFC-0080 §4.2 states the
priority-blind write as a fact about zenoh. It is a fact about zenoh-pico. In
zenoh-rs `pipeline.pull()` yields `(batch, priority)` and the information is
discarded one call later, at the trait boundary. The blocker is a trait
signature, not an architecture.
