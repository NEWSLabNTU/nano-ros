---
id: 311
title: "A C/C++ workspace listener receives fine but is invisible to ROS 2 discovery — `ros2 topic info` reports `Subscription count: 0`"
status: open
type: bug
severity: medium
area: rmw, interop
related: [issue-0309, issue-0306]
---

## Finding (2026-07-28, while strengthening the QoS proofs for issue 0309)

Running the `ws-qos-c` pair (talker entry + listener entry, two processes on one
zenoh router) and asking a stock `rmw_zenoh_cpp` peer what it sees:

```
Publisher count: 1

Node name: qos_talker
Endpoint type: PUBLISHER
QoS profile:
  Reliability: RELIABLE
  History (Depth): KEEP_LAST (10)
  Durability: TRANSIENT_LOCAL
  ...

Subscription count: 0
```

The listener was **alive and receiving** at that moment — the test had already
observed three `Received:` lines from it, and the process had a 30 s spin
budget. So delivery works and discovery does not: the subscription carries no
liveliness token ROS 2 counts, while the publisher in the sibling process
carries one that reports its full profile correctly.

The Rust path does not have this gap: `ws-qos-rust`'s entry shows
`Subscription count: 1` with its declared `TRANSIENT_LOCAL` profile, asserted in
`qos_override_e2e`.

## Not obviously a missing token

`nros-rmw-zenoh`'s shim declares `LivelinessToken` on BOTH sides
(`shim/publisher.rs` and `shim/subscriber.rs`), so "subscriptions don't declare
tokens" is too simple an explanation. Candidates, unverified:

- the C/C++ subscription path (`nros-cpp` → `nros_cpp_subscription_create`)
  takes a different route into the shim than the Rust one and skips the
  declaration;
- the token IS declared but with a keyexpr shape `ros2 topic info` does not
  match to the topic (the same class as issue #141, where the pub-direction
  keyexprs turned out identical and the bug was elsewhere);
- the token is declared and then dropped early (lifetime tied to a temporary).

Deciding between these needs a zenoh-side look at the declared liveliness
keyexprs for the two processes — `zenohd`'s admin space, or a `z_sub` on the
liveliness prefix — rather than more reading.

## Impact

- **Interop tooling is wrong about our subscriptions.** `ros2 topic info`,
  `ros2 node info`, and anything else driven by discovery under-report a
  nano-ros C/C++ listener. A user checking whether their node is connected sees
  nothing.
- **QoS compatibility checking on the peer side cannot see our profile.** A ROS 2
  publisher that would refuse an incompatible subscriber has nothing to refuse
  against.
- Delivery itself is unaffected — which is exactly why this went unnoticed.

## Test-side consequence, recorded

`workspace_features_e2e`'s QoS cells (c / cpp / mixed) assert the advertised
profile of the PUBLISHER only, with a comment pointing here. Adding the
subscription assertion is a one-line change (`"SUBSCRIPTION"` into the block
list) once this is fixed — the assertion is already written and is exercised
against the Rust path in `qos_override_e2e`.

## Direction

1. Determine which of the three candidates above is the mechanism, by observing
   the liveliness keyexprs both processes actually declare.
2. Fix, and turn the subscription assertion back on in the QoS cells.
3. If the cause is a divergence between the C/C++ and Rust subscription paths
   into the shim, that seam deserves the same single-sourcing treatment issues
   0303/0307 applied elsewhere — two paths into one shim is how this class of
   bug keeps arriving.
