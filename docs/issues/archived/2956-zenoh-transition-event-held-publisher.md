---
id: 2956
title: "On zenoh, each managed node holds a publisher for transition_event, because a put needs a key the publisher does not keep"
status: resolved
resolved: 2026-11-28
resolved_in: "the change that adds this line"
type: tech-debt
area: [zenoh, lifecycle, memory]
severity: low
found: 2026-11-24
related: [rfc-0128, phase-554]
---

## What happens

On zenoh, every managed node holds one zenoh publisher for
`/<node>/transition_event`. RFC-0128 D7 originally asked for an on-demand put
instead, so a node would hold no publisher.

## Why it is not a put

A zenoh put takes the topic's full data key (`<domain>/<topic>/<type>/<hash>`)
on every call. The zenoh publisher keeps only the topic name, and the type
strings it is created from are not `'static`, so it cannot rebuild the key at
publish time. Measured on the native lifecycle-node image (gdb, 2026-11-24):

| What | Size |
| --- | --- |
| A held zenoh publisher (`publisher_entry_t`) | 168 B static per managed node, plus zenoh-pico's key copy |
| Keeping the key in every zenoh publisher (or an enum of that size) | about +256 B on every publisher |
| A C table of put targets (`z_declare_keyexpr`) | a per-node table about the size of the publisher slot, plus its own knob |

Neither alternative saves memory, so the user ruled to keep the publisher
(option c) and amend RFC-0128 D7 (phase-554 W4).

## When to revisit

If zenoh-pico gains a put that does not need a held key (for example, a put
against a declared key id that costs less than a publisher), or if the topic's
type strings become `'static` at the shim, so the key can be rebuilt on the
stack at publish time. Either makes the put cheaper than the publisher slot,
and the saving is 168 B per managed node.

## Measured again: the heap side (2026-11-28, host 139)

The table above counted the publisher's static slot and "zenoh-pico's key
copy". The heap behind a declared zenoh-pico publisher is larger than that.
Net heap (glibc `mallinfo2().uordblks`) retained across each
`z_declare_publisher` in `zpico_declare_publisher_ex`, on the native
`linux_rust_lifecycle` image under gdb, main fa6b48e1f1:

| Publisher | Net heap retained |
| --- | --- |
| `/chatter` | 1,424 B |
| `/talker/transition_event` | 1,760 B |
| `/parameter_events` | 1,680 B |
| `/rosout` | 1,600 B |

A held zenoh publisher is therefore about 1.6–1.8 KB of heap on top of its
168 B slot. `_z_publisher_t` holds a declared key resource
(`_z_declared_keyexpr_t`) and a write-filter interest (`_z_write_filter_t`),
each with its own copies of the key. A bare `z_put` on a view key declares
neither.

This changes the premise of the ruling above ("neither alternative saves
memory"). A put-based `transition_event` would save about 1.9 KB per managed
node.

**What it would cost:**
- a new RMW `Session` method and C vtable slot, a one-shot put to a topic;
- XRCE and Cyclone keep the held publisher, since DDS needs a writer;
- the publisher-pool derivation drops one publisher per managed node;
- each put builds the key (about 150 B) on the stack.

The second revisit condition holds where the call is made:
`TransitionEvent`'s type name and hash are compile-time constants in
nros-node, and the node knows its own name. The put would keep the
attachment and the liveliness token, so the graph still lists the publisher.
No zenoh-pico change is needed, because `z_put` exists.

`/rosout` and `/parameter_events` are held the same way and gain from the
same put.

## Ruled again (2026-11-28, user)

The user reversed option (c) on the heap measurement above. On zenoh, all
three always-held writers (`~/transition_event`, `/rosout` and
`/parameter_events`) become a bare put on a key built on the stack:

- one new RMW `Session` method and a C vtable slot carry the put, with the
  topic handed back on every call; the full ABI path goes with it (the header
  edit, the regenerated bindings, `rmw-abi-shape` and the parity maps);
- XRCE and Cyclone keep a held writer for each;
- each writer keeps its rmw_zenoh attachment and its liveliness token, so the
  graph still lists it as a publisher;
- the publisher-pool derivation drops these writers' slots;
- acceptance includes a measured before/after heap number for the native
  lifecycle image.

RFC-0128 D7's 2026-11-24 amendment ("`transition_event` stays an ordinary
publisher on zenoh") is amended again by the change that lands this.

## Resolved (2026-11-28)

Done as ruled. On zenoh the three writers are created with
`TopicInfo::on_demand` and published through `Session::publish_on_demand`
(C ABI: `rmw_publisher_options_t::on_demand` and the `publish_on_demand`
vtable slot). Such a publisher declares no zenoh publisher and keeps its
liveliness token, gid and sequence number; each sample is a `zpico_put`, a
`z_put` on a view of the data key built for the call. XRCE and Cyclone leave
the slot NULL and keep a held writer. `entity.max_publishers` no longer
counts these writers; their tokens stay in the liveliness pool.

Measured, native `linux_rust_lifecycle` image, glibc heap in use at the first
spin (three runs each, identical):

| Build | Heap in use |
| --- | --- |
| Before (held publishers) | 319,856 B |
| After (on-demand puts) | 314,800 B |
| Saved | 5,056 B |

That matches the three publishers measured one by one above
(1,760 + 1,680 + 1,600 = 5,040 B). Live against stock ROS 2 Humble
(`rmw_zenoh_cpp`): `ros2 topic info -v /talker/transition_event` lists one
publisher on node `talker`, `ros2 topic list` lists `/rosout` and
`/parameter_events`, and `ros2 topic echo /talker/transition_event` printed
the `deactivate` event that `ros2 lifecycle set /talker deactivate` caused.

