---
id: 1231
title: "Cyclone's `publisher_assert_liveliness` is `nullptr` with no recorded reason, while zenoh implements it"
status: resolved
type: bug
area: rmw
severity: low
found: 2026-09-08
related: [1164, 0800]
resolved_in: (this commit)
---

# A NULL slot that never said why

`packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/vtable.cpp` carried

```cpp
constexpr rmw_ret_t (*kAssertPublisherLiveliness)(
    const rmw_publisher_t *) = nullptr;
```

in the same block as the two `*_event_init` hooks, under one comment that
explained only those: `*_event_init` is the ABI's promise that the BACKEND has
a safe context and will call you, and this backend has none. That argument says
nothing about manual liveliness, which needs no callback at all. zenoh
implements the slot (`nros-rmw-zenoh/src/shim/publisher.rs`), so it was a
per-backend gap and not an ABI-wide decline — and an unexplained `nullptr` is
indistinguishable from an oversight.

## Resolution: implemented, because both open questions measured "yes"

The file named two things as NOT established. Both were established before
anything was written:

**1. Does `dds_assert_liveliness` do something useful under our writer setup?**
Yes, measured in the pinned fork (0.10.5, `67ff7518`), not read off upstream
docs:

* `dds_assert_liveliness(writer)` (`ddsc/src/dds_entity.c:1558`) pins the
  entity and, for `DDS_KIND_WRITER`, calls `write_hb_liveliness`
  (`ddsi/src/q_transmit.c:724`).
* That renews `wr->lease` when the writer's kind is `MANUAL_BY_TOPIC` (or the
  participant's `minl_man` lease for `MANUAL_BY_PARTICIPANT`), then sends a
  Heartbeat.
* The lease exists to be renewed: it is registered at writer creation for any
  non-AUTOMATIC kind with a finite duration (`ddsi_endpoint.c:1006`), with **no
  dependency on a matched reader**, and expiry runs
  `ddsi_writer_set_notalive(wr, true)` → `DDS_LIVELINESS_LOST_STATUS`
  (`q_lease.c:297`, `ddsi_endpoint.c:728`).

So the effect is observable locally, through this backend's own
`publisher_take_event`, with no peer process and no reader.

**2. Is there a consumer path that reaches the slot?** Yes, all three language
surfaces: Rust `Publisher<M>::assert_liveliness()`
(`nros-node/src/executor/handles.rs`), C `rcl_publisher_assert_liveliness`
(`nros-c/src/publisher.rs:502`), C++ `Publisher<M>::assert_liveliness()`
(`nros-cpp/include/nros/publisher.hpp:290`). Each lands on
`CffiPublisher::assert_liveliness` (`rmw/cffi/src/lib.rs:3695`), which on a
NULL slot returns `TransportError::Unsupported`. So a Cyclone application
asking for `MANUAL_BY_TOPIC` got `Unsupported`, not a silent no-op.

What was **not** established, and is not claimed: whether any nano-ros consumer
actually wants `MANUAL_BY_TOPIC` on Cyclone. Nothing in the tree asks for it.
That is an argument about demand, not about correctness.

## What landed

* `publisher_assert_liveliness` in `publisher.cpp` — `dds_assert_liveliness` on
  the writer, **gated on the publisher's liveliness kind** (`MANUAL_BY_TOPIC` /
  `MANUAL_BY_NODE`, which `make_dds_qos` already folds together because Cyclone
  has no BY_NODE). The kind is captured in `PubState` at create, the way the
  zenoh shim keeps `liveliness_kind`.
* The gate is not fastidiousness. For AUTOMATIC / NONE / SYSTEM_DEFAULT there
  is no lease to renew, so the call reduces to an unsolicited Heartbeat;
  `rmw_vtable.h` documents the slot as a no-op returning OK for those kinds and
  zenoh implements exactly that. And the 0.10.5 writer branch **leaks on its
  failure paths** — a failed `dds_entity_lock` returns with the entity still
  pinned, a failed `write_hb_liveliness` with it pinned AND locked, wedging
  every later operation on that writer. That is vendored code on its own patch
  line, so the rule available here is the narrow one: never make the call for a
  kind it cannot help.
* The shared comment is split. The `*_event_init` note now says in its own text
  that it covers those two constants and nothing else, and the vtable row for
  this slot carries its own reason.
* `tests/assert_liveliness.cpp` (ctest `nros_rmw_cyclonedds_assert_liveliness`)
  provokes the lease rather than asserting a return code: 500 ms lease, held
  open by assertions every 100 ms for 2 s (no `LIVELINESS_LOST` may appear),
  then left alone (one MUST appear). The second half is the first half's
  negative control. It also checks the documented no-op on an AUTOMATIC
  publisher and `INVALID_ARGUMENT` on an uninitialised one.

## Proof the test is about the lease

Mutating the implementation to `return NROS_RMW_RET_OK;` without touching the
writer, rebuilding that target only, and re-running:

```
FAIL: LIVELINESS_LOST after 500 ms of assertions at 100 ms against a 500 ms
      lease — the assertion is not reaching the writer
```

— failing at exactly the lease boundary the stub cannot renew. Restored, the
test passes in 2.41 s, three runs for three passes.

## Not fixed here

* `check-rmw-cyclonedds` cannot go fully green on a host without ROS: six
  service/mangling tests are `Not Run` because their `.msg` → IDL step needs
  `rosidl_adapter`. Pre-existing, unrelated to this slot.
* `book/src/design/rmw-vs-upstream.md` still says an unsupported backend's
  `assert_liveliness()` default is `Ok(())`. True of the Rust trait default,
  false through the C ABI, where a NULL slot is `Unsupported` (issue 0349's
  deliberate choice). Left alone — it is a separate claim in a separate file.
