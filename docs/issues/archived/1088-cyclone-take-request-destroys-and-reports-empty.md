---
id: 1088
title: "cyclone `take_request` consumes a request, destroys it, and reports an
  empty queue — 33rd request onward is lost with `taken = false`"
status: resolved
type: bug
area: rmw
resolved_in: "fix(#1088): a saturated Cyclone service server says WOULD_BLOCK, and the runtime keeps the kind"
related: [phase-428, phase-444, rfc-0089, issue-1291]
---

## Resolution (2026-09-11, phase-444 W1)

Both halves of the Fix are done. A regression test covers both, each checked
against a negative control.

- **Loss.** `bd9948e23d` reserves the correlation slot BEFORE the destructive
  take. With every slot held, nothing is taken off the reader.
- **Observability.** `service_take_request` now maps only `NO_DATA` to
  `taken = false` + OK. `WOULD_BLOCK` reaches the caller, as the XRCE sibling
  has always done. No new ABI code was needed: `NROS_RMW_RET_WOULD_BLOCK`
  already meant "resource momentarily unavailable, retry"
  (`<nros/rmw_ret.h>`). The `take_request` slot doc in `<nros/rmw_vtable.h>`
  now states the contract: `taken = false` means nothing was pending, and a
  held request with no free slot is `WOULD_BLOCK`, with nothing consumed.
  `generated.rs` is regenerated.
- **The runtime keeps the kind.** Every `take_request` consumer in nros-node
  used to rewrite every error to `ServiceReplyFailed` /
  `ServiceRequestFailed`. That covered the executor's raw service dispatch
  (`arena.rs`), the four action-server takes (`action_core.rs`) and
  `RawServiceServer::take_request_raw` (`handles.rs`). Each now passes
  `WouldBlock` through, so `SpinOnceResult.service_errors` counts a
  saturated server and `first_error` names it. The C polling take
  (`nros_service_take_request_raw`) returns `NROS_RET_TRY_AGAIN`, distinct
  from the `0` that means empty. The C++ raw FFI take returns
  `NROS_CPP_RET_TRY_AGAIN`, distinct from `OK` with `len = 0`.
- **Test.** `tests/service_request_slots_exhausted.cpp`: 5 clients × 7
  requests, 35 against the 32 slots. It takes 32 without answering, then
  asserts the next take is `WOULD_BLOCK` while `has_request` stays true. It
  then answers one request at a time and asserts that all 35 replies reach the
  client that asked, each checked by value. Negative control: with the
  adapter's old `NO_DATA || WOULD_BLOCK` fold put back, it fails `rc=9`,
  `EXHAUSTION NOT OBSERVABLE: 32 slots held, has_request=true, and
  take_request reported an EMPTY queue`.
- **Found on the way:** issue 1291. Five clients in one process shared one
  reply id, so client 1 accepted client 0's reply as its own. It is fixed in
  the same change. Without that fix this test cannot pass. With the old id put
  back it fails `rc=18`.

Not changed: the typed C++ wrapper `Service::take_request` already returned
`ErrorCode::TryAgain` for an EMPTY queue (`len == 0`). A saturated server
therefore reads the same as an empty one at that layer, though not at the raw
FFI below it. Separating them needs a new `ErrorCode` value, which is a
public C++ API change, so it is left out of this fix.

## Problem

`cyclonedds/src/service.cpp:826-853`. `take_typed_wire` is a **destructive**
`dds_takecdr` + `ddsi_serdata_unref` (`:535`, `:551`). Only *after* consuming
the sample does it look for a free correlation slot, and when there is none it
returns `WOULD_BLOCK` (`:853`) — which the adapter collapses to
`*taken = false; return OK` (`:879-881`).

`kRequestSlots = 32` (`:241`), freed only in `send_response` (`:983`). So with
32 requests outstanding, every further request is taken off the wire,
destroyed, and reported to the caller as "nothing there".

Upstream (`rmw.h:2348, 2353`): `taken == false` with `RMW_RET_OK` means
**nothing was consumed**.

## The sibling gets it right

`xrce/src/service.c:293` returns `WOULD_BLOCK` **without popping the ring**, and
`:335-338` maps only `NO_DATA` to OK. Two implementations of one contract
disagreeing about both the loss and the reporting.

## Impact

Silently wrong. A busy server stops answering and looks idle; the client sees
timeouts. Nothing counts the drop.

## Fix

Reserve the correlation slot **before** the destructive take, or peek before
taking. Report exhaustion as a distinct condition — `WOULD_BLOCK` reaching the
caller, or a counted-and-logged drop — never as an empty queue.

## Status — 2026-09-10: half fixed

`bd9948e23d` reserves the correlation slot BEFORE the destructive take, so a request is
no longer consumed and lost. The second half of the Fix above is NOT done: the
`service_take_request` adapter still maps `WOULD_BLOCK` to `taken = false` with `OK`, so
a saturated server reads as an empty queue. That is now contract-correct about
consumption — the sample stays on the reader — but not about observability. No
regression test was added. Carried as phase-444 W1.
