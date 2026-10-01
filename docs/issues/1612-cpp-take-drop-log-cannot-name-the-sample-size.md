---
id: 1612
title: "A BufferTooSmall drop on the C++ take path cannot name the sample's size:
  the RMW take ABI returns bytes-or-error with no required-length out-param"
status: open
type: enhancement
area: [rmw, cpp, diagnostics]
severity: low
found: 2026-10-01
related: [1425, 0757, phase-460]
---

## What

Issue 1425's acceptance asked for the first too-small drop on a C++
subscription to log "topic and both sizes". phase-460 W7 (`f74ebdade`) counts
the drop, pushes the total into the boot record and logs the BUFFER size, and
recorded why it stops there: the RMW C ABI's take returns "non-negative = bytes
produced, negative = error code", so `TransportError::BufferTooSmall` carries no
length, and no layer below the C++ wrapper ever learns how large the dropped
sample was. `nros-node`'s arena dispatch (`arena.rs`, issue 0757) has the same
gap for the same reason. The topic is absent for a different reason: it lives on
the C++ `Subscription<M>`, not in the storage the take path is reached from.

1425 was closed on the faults it named (exhaustion and the drop both reach the
record); this is the part that needs an ABI change.

## What would fix it

A required-length out-param (or a negative return that encodes it) on the RMW
take slot, filled by each backend that knows the wire size before it copies --
zenoh-pico's sample length, XRCE's payload length, Cyclone's serdata size --
then carried by `TransportError::BufferTooSmall { needed }`. A header change,
so RFC-0054 applies: regenerate the bindings, and every backend's slot moves in
the same change.
