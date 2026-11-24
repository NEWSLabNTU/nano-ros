---
id: 2956
title: "On zenoh, each managed node holds a publisher for transition_event, because a put needs a key the publisher does not keep"
status: open
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
