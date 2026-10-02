---
id: 1632
title: "The Rust arena's drop log and the service/client takes still cannot name a
  refused sample's size — issue 1612 carried it to the C++ subscription take only"
status: open
type: enhancement
area: [rmw, core, diagnostics]
severity: low
found: 2026-10-02
related: [1612, 0757, 1425]
---

## What

Issue 1612 made a too-small subscription `take` carry the size the sample
needed: `rmw_mut_byte_span_t.len` on `NROS_RMW_RET_BUFFER_TOO_SMALL`
(`NROS_RMW_TAKE_LEN_UNKNOWN` = 0 when unknown), surfaced in Rust as
`nros_rmw::Subscription::refused_sample_len()`, filled by zenoh, XRCE,
Cyclone, uORB and the cffi Rust adapter. The C++ drop log
(`nros-cpp/src/subscription.rs`) now prints both sizes.

Two siblings of the same drop still cannot:

1. **`nros-node`'s arena dispatch** (`executor/arena.rs`, `report_dropped_take`,
   issue 0757). Its doc comment still says "no required-length out-param, so
   the backend cannot report how big the sample was" — no longer true. The
   subscriber handle is in reach at the drop site, so this is a call to
   `refused_sample_len()` plus a format change. Left out of 1612 only because
   `nros-node` was being edited concurrently by another session; it is the
   same one-line consumer the C++ path got, and the comment should move with it.
2. **`take_request` / `take_response`** (service server / client). Both use the
   same span and can return `BUFFER_TOO_SMALL` (XRCE `service.c`, Cyclone
   `service.cpp`, zenoh `shim/service.rs`), but the `rmw_entity.h` rule was
   scoped to the subscription `take` slot, and the `Service`/`Client` traits have
   no accessor. Extending it is the same additive shape (caller pre-sets 0,
   backend writes the size, a trait query with a `None` default).

The C++ log also still cannot name the TOPIC (the storage the take is reached
from does not carry it) — unchanged by 1612, and out of this issue's scope.

## Closing looks like

`report_dropped_take` prints `<needed>-byte sample, <cap>-byte buffer` (`?`
when unknown) with a test like `nros-cpp`'s; the span rule in `rmw_entity.h`
widened to the two service takes, implemented in each backend, with a
per-backend test as 1612's.
