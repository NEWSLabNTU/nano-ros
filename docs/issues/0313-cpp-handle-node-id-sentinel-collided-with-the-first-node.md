---
id: 313
title: "`nros_cpp_node_t.node_id` used `0` as both \"no node\" and the FIRST node's id, so a single-node C/C++ entry read as node-less at eight call sites"
status: resolved
type: bug
severity: medium
area: rmw, runtime
related: [issue-0312]
---

## Finding (2026-07-28, while investigating issue 0312)

`NodeId` is an INDEX into the executor's node table, and that table starts empty
(`nodes: heapless::Vec::new()`) — so the FIRST node any entry creates is
`NodeId(0)`. The C/C++ handle stored it raw:

```rust
out.node_id = node_id.raw();     // 0 for the first node
...
if node_ref.node_id != 0 { /* use the node */ } else { /* "no node" */ }
```

`0` therefore meant both "this handle has no registered node" (a zero-initialised
`nros_cpp_node_t`) and "this handle's node is the first one". Eight call sites
across `subscription.rs`, `publisher.rs`, `service.rs`, `timer.rs` and
`action.rs` read the field raw, so **a single-node C or C++ entry — the common
shape — had its only node treated as absent at every one of them**, falling back
to the executor's own primary session/name instead of the node's.

Phase 268 had already fixed the adjacent half of this: `nros_cpp_node_create`
used to leave the field at `0` unconditionally, and its commit notes the same
downstream symptom ("the subscription register fell back to the session's
name"). It made the field carry a real id but left `0` overloaded, so the fix
worked for every node except the first.

## Fix

The field is now stored BIASED BY ONE: `0` = no node, `n` = `NodeId(n - 1)`,
via `encode_node_id` / `decode_node_id` (pure `u8`, so they hold without the
`rmw-cffi` feature and are unit-testable) and the `store_node_id` /
`node_id_opt` wrappers. All eight raw comparisons now go through the decoder.

Nothing outside `nros-cpp`'s own Rust reads the field — no C++ header, no
example — so the encoding change is internal; the struct layout is unchanged.

## What this did NOT fix

**Issue 0312 is still open.** This was the leading hypothesis for it: the arena
subscription path resolves node identity from `node_id`, and a lost identity
means no `with_node_name`, which means `create_subscription` skips the
liveliness token `ros2 topic info` counts. The mechanism is real and the
sentinel collision was real — but after the fix, with the `ws-qos-c` build
directory wiped and rebuilt from scratch, a stock `rmw_zenoh_cpp` peer STILL
reports `Subscription count: 0` for the listener.

So the sentinel bug was a genuine latent defect sitting next to 0312, not its
cause. Recorded separately rather than folded into 0312 precisely because it
does not close it.

## Verification

`node_id_zero_survives_the_handle_round_trip` pins the property that broke: a
zero-initialised handle decodes to `None`, `encode(0)` does not collide with the
sentinel, and `decode(encode(n)) == n`. Behaviour-level coverage is absent — no
test observed the wrong fallback before, which is how the first-node case
survived phase 268's fix. That gap is issue 0309's subject.
