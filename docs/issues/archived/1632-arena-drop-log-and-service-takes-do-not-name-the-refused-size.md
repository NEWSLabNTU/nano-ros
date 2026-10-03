---
id: 1632
title: "The Rust arena's drop log and the service/client takes still cannot name a
  refused sample's size — issue 1612 carried it to the C++ subscription take only"
status: resolved
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

## Resolution

Fixed 2026-10-03 on `fix/1632-refused-size-everywhere`.

**ABI (RFC-0054, additive — no slot, no layout change).** `rmw_entity.h`,
`rmw_ret.h` and `rmw_vtable.h` widen issue 1612's rule from the subscription
`take` to `take_request` and `take_response`: on `NROS_RMW_RET_BUFFER_TOO_SMALL`
the span's `len` is the size the payload needed, or
`NROS_RMW_TAKE_LEN_UNKNOWN`, which the caller pre-sets. Bindings regenerated
(doc comments only).

**Rust.** `ServiceTrait::refused_request_len` / `ClientTrait::refused_response_len`
(default `None`). `CffiService` / `CffiClient` pre-set `len` to UNKNOWN, record
the backend's answer (a `len` that would have fit reads as unknown), and treat
an over-long success `len` as the size needed. `rust_adapter` carries a Rust
backend's size into the span through ONE helper, `refused_ret`, for all three
take slots (1612's `refused_take_ret` now delegates to it).

**Backends.**

| backend | request | reply | measured, before -> after |
| --- | --- | --- | --- |
| XRCE | staged ring entry length | reply slot length | `service_refused_len.c`: len 0 -> 300 (both) |
| Cyclone | user payload (or wire size less the 16-byte request header) | same | `service_refused_len.cpp`: len 0 -> 24 / 0 -> 12 |
| zenoh | ring entry length | `zpico_get_reply_len` (new, read-only) | `zenoh_integration.rs`: `Some(64)`, `Some(200)` |
| uORB | — | — | services are an UNSUPPORTED stub; nothing to report |

**Found on the way (zenoh):** a reply bigger than the client's buffer came back
from `zpico_get_check` as `ZPICO_ERR_FULL` and fell through to issue 0465's
pool-exhaustion mapping, so it surfaced as `InvalidConfig` with no size, and the
handle stayed pending to be refused again on every poll. Measured with the new
arm disabled: `Err(InvalidConfig)`. Now it is `BufferTooSmall`, the answered
request is retired like any answered one, and the size is reported.

**Drop log.** `nros-node`'s `report_dropped_take` prints
`<needed>-byte sample, <cap>-byte buffer` (`?` when unknown) and its stale doc
("the backend cannot report how big the sample was") is corrected. The format
and the rule for WHICH refusal carries a size (only `BufferTooSmall`;
`MessageTooLarge` is the backend's staging buffer) are `nros_rmw::RefusedLen`,
which `nros-cpp`'s drop log now uses too — one rule, two consumers. The five
buffered drain sites go through one `take_reporting_drop`.

Tests per layer: cffi `refused_service_len.rs` (size / legacy-unknown /
fitting-len, both sides), `rust_adapter.rs` (Rust backend -> `span.len`, both
sides), XRCE and Cyclone ctests (each with a negative control against the
unfixed source), the zenoh router-backed integration test, nros-node
`dropped_take_log_tests`, nros-cpp's four drop-log cells unchanged and green.
Gates: `check-abi-bindings`, `check-rmw-abi-shape`, `check-rmw-api-parity`,
`check-ffi-struct-mirrors`, `api-parity --check --require-disposition` OK.

**Not done:** nothing CONSUMES `refused_request_len` / `refused_response_len`
in a log yet — the arena's service and client dispatch propagate a refused take
as an error with no drop log of their own (the subscription has one because of
issue 0757). The sizes are now available to whichever layer adds that line. The
C++ log still cannot name the topic (out of scope, as the issue said).

Sweep: `git grep -n 'BUFFER_TOO_SMALL' -- 'packages/rmw/*.c' 'packages/rmw/*.cpp'`
and `git grep -n 'impl.*\(ServiceTrait\|ClientTrait\) for' -- packages`.
