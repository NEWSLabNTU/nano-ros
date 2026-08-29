---
id: 890
title: "The liveliness DISCOVERY wildcards still pinned `0/11` in the entity-id
  field, so `wait_for_service` and LivelinessChanged matched only a session's
  eleventh entity"
status: resolved
type: bug
area: rmw
related: [issue-0292, phase-381]
---

## Problem

`rmw_zenoh_cpp`'s liveliness grammar puts a node id and an ENTITY id in chunks
3 and 4:

```
@ros2_lv/<domain>/<zid>/<node_id>/<entity_id>/<kind>/%/<ns>/<node>/…
```

Our declarations got that right — `0/<entity_id>`, from a per-session counter.
Our two DISCOVERY wildcards did not:

```
declared:  @ros2_lv/<domain>/<zid>/0/<entity_id>/MP/%/<ns>/<node>/<topic>/…
wildcard:  @ros2_lv/<domain>/*   /0/11         /MP/%/*/*/<topic>/…
```

A `*` matches one chunk; `0` and `11` are literal. So the query matched an
entity only when its id was exactly 11. `entity_counter` starts at 1 and
increments, so that is a session's ELEVENTH entity and nothing else.

Two callers depend on those wildcards:

* `Client::wait_for_service` — the rclcpp-style discovery a client does before
  its first request;
* `ZenohSubscriber`'s LivelinessChanged emulation, which polls for publishers
  matching its (topic, type).

Both could only ever see an eleventh entity.

## This is issue 0292's residue

0292 measured this exact literal on the DECLARATION side: every entity of an
action server carried the hardcoded `PROTO_VERSION_TOPIC = "0/11"`, so
`rmw_zenoh_cpp`'s graph cache — which keys on `(zid, node_id, entity_id)` —
deduped an action server's five entities to one and `ros2 action list` was
empty. The fix threaded a per-session `entity_counter` through **the four
entity keyexpr builders**.

It did not touch the two WILDCARD builders, which kept the constant. On that
side the same literal stops being a collision and becomes a filter — a
different symptom, same root, which is why it survived a fix that named it.

Textbook "fix the CLASS, not the reported site" (CLAUDE.md): the sweep for the
constant was never run, and the constant's own NAME hid it. `PROTO_VERSION_TOPIC`
reads like a protocol version, so a wildcard carrying "the protocol version"
looks correct. It is a node id and an entity id.

## Fix

The constant is gone. `ENTITY_ANY = "*/*"` replaces it in both wildcard
builders, and the name now says what the field is.

Wildcarding BOTH chunks, not `0/*`: our nodes use node id 0, but the peers worth
discovering are native `rmw_zenoh_cpp` ones, and their node id is not ours to
assume.

Four format doc-comments that said `0/11` where the code emits `0/<entity_id>`
are corrected too — they are what made the wildcard look right.

## Verified — by regression, not by reading

`publisher_wildcard_matches_any_entity_id` and
`service_server_wildcard_matches_any_entity_id` build a declaration with several
ids and require the wildcard to match, under a chunk-wise matcher with zenoh's
single-`*`-is-one-chunk rule. They assert the PROPERTY, not the spelling, so a
future re-pin fails whatever literal it uses.

Both ids include **11 deliberately** — it is the one value the broken code did
match, so a test that only tried 11 would have passed against the bug.

Restoring the old constant turns both red with the full keyexpr pair in the
message; restoring the fix turns them green. 72/72 in the crate.

## NOT verified here

Live interop. These tests prove our wildcard matches our own declarations under
zenoh's matching rule; they do not prove a native `rmw_zenoh_cpp` publisher is
discovered, which needs a router and a real ROS 2 node. That is phase-381's
acceptance work, and this fix is a precondition for it rather than a
substitute.
