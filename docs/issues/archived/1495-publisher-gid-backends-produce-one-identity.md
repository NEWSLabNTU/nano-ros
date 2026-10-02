---
id: 1495
title: "The publisher GID is one TYPE on every backend and one VALUE on none —
  a take's gid and `get_gid_for_publisher`'s gid are never produced from the
  same source"
status: resolved
resolved: 2026-10-03
type: bug
area: rmw
severity: medium
related: [rfc-0089, phase-444, phase-467]
found: 2026-09-25
---

> **RESOLVED 2026-10-03.** Both halves landed together and each was observed
> by a live peer. The "what is true today" table below is the state when
> filed; it was already partly stale by then — PR #1321 had filled zenoh's
> `get_gid_for_publisher` slot (Rust-adapter trampoline +
> `ZenohPublisher::get_gid`), so on zenoh the take gid and `get_gid()` were
> already the same BYTES. What remained was what those bytes MEANT, and all of
> Cyclone.
>
> * **zenoh.** The gid is XXH3-128 of the entity's liveliness keyexpr
>   (`shim/entity_gid.rs`, a port of `rmw_zenoh_cpp` 0.1.9's
>   `simplified_xxhash3.cpp`, vectors produced by compiling upstream's own
>   file) — upstream's derivation, not merely "derived from the ZenohId and
>   entity id" (the keyexpr carries both). The wire stays 16 bytes. Getting a
>   peer to AGREE took a second change the issue did not anticipate: a peer
>   parses our token and RE-SERIALISES it before hashing, and our QoS field
>   (`1:2:1,10`) did not survive that round trip (upstream writes `::,10`,
>   eliding its defaults, depth default 42). Measured: with the hash alone,
>   `ros2 topic info --verbose` printed a gid for our publisher that matched
>   none of our samples. Tokens now carry the canonical form
>   (parse-equivalent for every peer). Our graph's endpoint gids are derived
>   the same way, so they match a stock peer's own.
> * **Cyclone.** The ABI already had the channel — `take_with_info` — and
>   nothing dispatched it. Cyclone fills it (writer GUID via the publication
>   handle, cached per subscription) and `nros-rmw-cffi` now calls it.
> * **Acceptance.** `graph_interop`'s
>   `zenoh_publisher_gid_is_the_one_a_stock_peer_reads_and_reports` (our
>   `get_gid` == the `publisher_gid` a stock `rmw_zenoh_cpp` subscriber read
>   via `rcl_take` == the GID `ros2 topic info --verbose` prints; and the take
>   side against a stock publisher) and
>   `cyclone_take_gid_is_the_publishers_graph_gid` (our take reports the GUID
>   ros2 prints for a stock publisher; our `get_gid` is the GUID ros2 prints
>   for us). Both fail with their half reverted. Recorded in
>   `.config/interop-verdicts.toml`.
> * **Acceptance item 2 as written could not be met, and the reason is
>   upstream's:** a Humble `rmw_cyclonedds_cpp` subscriber reports the 8-byte
>   publication HANDLE as `message_info.publisher_gid` (`rmw_node.cpp`
>   1.3.4; measured `1d5ff9fccbaee4f7` + zeros against GUID `01.10.0f.46.…`).
>   The graph GID is the shared identity, and that is what the Cyclone case
>   compares.
> * **Item 3 (stable across restart):** exactly as stable as the session's
>   zenoh id — stable when `session_zid` is configured, fresh per run by
>   default. That is upstream's behaviour too; distinct between two publishers
>   in one process by construction (distinct entity ids).

# What is true today

`MessageInfo::publisher_gid` and the `rmw_gid_t` that `get_gid_for_publisher`
writes are now the same width — 24 bytes, upstream's `RMW_GID_STORAGE_SIZE`
(the phase-467 RMW gap-closure design study's Q1(a), the widening that closed
the type half). They are still not the same value, and **no backend produces
both**:

| backend | `get_gid_for_publisher` | `MessageInfo::publisher_gid` |
| --- | --- | --- |
| Cyclone | the DDS writer GUID, 16 bytes zero-padded to 24 (`packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/vtable.cpp:146-164`, registered at `:427`) | **never written** — `set_publisher_gid` has two callers, both zenoh, and a pure C/C++ backend writes no `MessageInfo` at all (`packages/rmw/cffi/src/lib.rs:875-879`), so the callback sees `None` |
| zenoh | **NULL slot** (`packages/rmw/cffi/src/lib.rs:271`) | `RmwAttachment::generate_gid()` — a counter times a constant, XORed with the address of a stack local (`packages/rmw/zenoh/nros-rmw-zenoh/src/shim/mod.rs:363-376`), zero-extended into the 24-byte field |
| uORB | **NULL slot** (`packages/rmw/uorb/nros-rmw-uorb/src/vtable.cpp:121`) | not filled |

So there is no image in which the two can be compared, and an equality test
between them would be answering a question no backend has been asked. That is
tolerable only while it is WRITTEN DOWN, which is what this issue and the doc
comments added with the widening are for — `nros-core`'s
`MessageInfo::publisher_gid` accessor and `rmw_entity.h`'s `rmw_gid_t` comment
both say it, and `cpp:Publisher::get_gid` in the api-parity ledger says it
again.

Two separate defects hide behind that:

* **zenoh's gid means nothing.** A stack address XORed with a counter is not an
  identity: it is not stable across a restart, it is not derived from anything
  a peer knows us by, and two publishers in two processes can collide. A stock
  `rmw_zenoh_cpp` peer READS this value off our attachment.
* **Cyclone's `MessageInfo` is silent.** Cyclone has the sample's publication
  handle on the receive path and does not put it anywhere — and because a pure
  C/C++ backend writes no `MessageInfo` at all, closing this means giving the
  C-backend take path a metadata channel, not just an assignment.

# What closing it means

Each backend derives BOTH answers from one source.

* **zenoh.** The publisher already has a stable identity in its liveliness
  token — a `ZenohId` (16 bytes) plus a `u32` entity id
  (`packages/rmw/zenoh/nros-rmw-zenoh/src/zpico.rs:148`,
  `shim/mod.rs:596`). Deriving the attachment gid from that pair makes the
  value mean something, and filling `get_gid_for_publisher` from the same bytes
  makes the two answers agree.
* **Cyclone.** Fill `MessageInfo::publisher_gid` from the sample's publication
  handle, which the receive path already holds.

# Why it is not part of the widening

**The zenoh half is WIRE-OBSERVABLE.** The attachment gid is the one value in
this area that a stock ROS 2 peer looks at, and the width is fixed: the
attachment carries a VLE length byte of 16 and `rmw_zenoh_cpp`'s reader rejects
any other length (`shim/mod.rs:93-96`, `:429-431`). Changing what those 16 bytes
CONTAIN cannot be accepted on a reading of the code — it needs a run against a
live peer, which is the router-and-peer lane **phase-444 W2** still owes.

The widening has no such exposure: it moves a `nros-core` Rust field that is
not `repr(C)`, appears in no C or C++ header, and leaves every byte on the wire
where it was. Splitting the two is what let the width close without waiting on
a lane that does not exist yet.

# Acceptance

Not a gate and not a grep — an interop run:

1. a nano-ros zenoh publisher and a stock `rmw_zenoh_cpp` subscriber on one
   router; the peer's reported publisher gid equals the first 16 bytes of what
   `get_gid_for_publisher` reports for that publisher;
2. the same for Cyclone, against `ros2 topic info --verbose`
   (`publisher_gid_identifies_us_to_the_peer` in
   `packages/testing/nros-tests/tests/advertised_state_interop.rs` already does
   this half for the vtable slot — extend it to assert the take side agrees);
3. the gid is stable across a restart of the same node and distinct between two
   publishers in one process.

Until (1) and (2) pass together, the ledger's `cpp:Publisher::get_gid` row and
the two doc comments keep saying the two gids are not comparable.
