---
id: 969
title: "The Cyclone RMW deserializes every received sample and re-serializes it, so `try_recv_raw` costs a decode, an encode and two heap allocations per take"
status: open
area: [rmw, memory]
severity: high
related: [0958, 0781, 0896, phase-391, phase-403, rfc-0035, 0038]
---

# We take CDR off the wire, decode it, encode it again, and hand back the second copy

## What the code does

`nros-rmw-cyclonedds`'s `subscription_take` (`src/subscriber.cpp:134`) runs this
sequence for every sample:

```cpp
void* sample = ddsrt_calloc(1, state->desc->m_size);   // typed sample, heap
dds_return_t taken = dds_take(state->reader, samples, si, 1, 1);
// ... Cyclone deserializes wire CDR into `sample`, allocating for every
//     variable-length member as it goes ...
dds_ostream_t os;
dds_ostream_init(&os, 0, 1 /*xcdr1*/);                 // starts empty, grows by realloc
bool ok = dds_stream_write_sample(&os, sample, state->st->as_sertype());
// ... re-serializes the typed sample back to CDR ...
uint32_t paylen = os.m_index;
uint32_t total = paylen + 4;
if (buf_len < total) { /* NROS_RMW_RET_BUFFER_TOO_SMALL */ }
```

The caller asked for bytes. The bytes were already there. We decoded them,
allocated a typed struct plus one block per variable-length member, allocated and
grew an output stream, encoded the struct back to bytes, copied those into the
caller's buffer, and freed everything.

The publish direction is the mirror image: CDR bytes → `dds_stream_read_sample` →
typed buffer → `dds_write` → Cyclone serializes again.

## This is deliberate, and the reason it was accepted no longer holds

`src/sertype_min.hpp:6-29` states the tradeoff outright:

> Cyclone's `dds_writecdr` / `dds_takecdr` raw-CDR API needs a real
> `ddsi_sertype *` linked to a `ddsi_domaingv`, which our backend can't get to
> without reaching into Cyclone's private struct layout. We sidestep that path
> entirely: […]
> **Cost: a 2× CDR roundtrip per publish + per recv.** Acceptable for an in-tree
> smoke; low-throughput control loops on Cortex-A/R safety MCUs run well under the
> headroom. A future zero-copy fast path can replace this once Cyclone exposes
> `dds_writer_lookup_serdatatype` upstream.

Two things are wrong with that reasoning as it applies to **receive**.

**The stated blocker is transmit-only.** `dds_takecdr` takes no sertype:

```c
dds_return_t dds_takecdr(dds_entity_t reader_or_condition,
                         struct ddsi_serdata **buf, uint32_t maxs,
                         dds_sample_info_t *si, uint32_t mask);
```

(`third-party/dds/cyclonedds/src/core/ddsc/include/dds/dds.h:3783`.) The reader
already owns its sertype from `dds_create_topic(desc)`. `dds_writer_lookup_serdatatype`
is what the *publish* path would need, to build a serdata from raw bytes. Receive
never needed it. The blocker was written once and applied to both directions.

**"Acceptable for an in-tree smoke" is no longer the deployment.** The
autoware-safety-island an536 lane is a real consumer running a control loop over
this backend, and it is the lane that motivated issues 0896, 0917 and 0958.

## What the reference implementation does

`ros2/rmw_cyclonedds`'s `rmw_take_ser_int` (`rmw_cyclonedds_cpp/src/rmw_node.cpp:3572`)
is the shape we should have:

```cpp
while (dds_takecdr(sub->enth, &d, 1, &info, DDS_ANY_STATE) == 1) {
  size_t size = ddsi_serdata_size(d);
  rmw_serialized_message_resize(serialized_message, size);
  ddsi_serdata_to_ser(d, 0, size, serialized_message->buffer);
  serialized_message->buffer_length = size;
  ddsi_serdata_unref(d);
```

One `memcpy` out of the serdata. No typed sample, no member allocations, no
ostream, no re-serialization. `ddsi_serdata_to_ser` on Cyclone's default sertype
reads the CDR the serdata is already holding.

## Three costs, not one

**Throughput and jitter.** Per take: two-plus heap allocations, a full decode, a
full encode. On a Cortex-R safety MCU this sits directly in the receive path of a
control loop.

**Real-time bound.** [phase 391](../roadmap/phase-391-allocation-unification-and-tier-model.md) argues
the heap holds infrastructure while payload buffers stay static, and derives a
Robson bound from that. Every Cyclone take allocates a *payload-sized* block, and
the ostream's growth-by-realloc allocates a second one whose size depends on the
sample. The bound as stated does not describe a Cyclone image.

**Correctness of the bytes, not only their cost.** `dds_ostream_init(&os, 0, 1)`
requests **XCDR1 in native byte order**. A ROS 2 publisher emits XCDR2, and a
big-endian peer emits big-endian. So the caller does not receive the wire
representation — it receives a re-encoding, with an encapsulation header we
synthesized. This is why the sizing work has to reason about
`MAX_SERIALIZED_SIZE_XCDR1` for this backend while the wire carries XCDR2
(see [#0964](0964-two-different-sizes-for-the-same-type.md)). Taking the serdata's
own bytes removes the discrepancy rather than documenting it.

## Direction

Rewrite `subscription_take` to the `dds_takecdr` + `ddsi_serdata_to_ser` shape:
take the serdata, read `ddsi_serdata_size`, refuse with
`NROS_RMW_RET_BUFFER_TOO_SMALL` when the caller's buffer is smaller, otherwise one
`memcpy` into it, then `ddsi_serdata_unref`. The caller-owns-the-buffer contract
costs exactly one copy; everything above that copy is removable today, inside this
backend, with no change to Cyclone.

`subscription_take_multi` (`src/subscriber.cpp:281`) has the same body and gets
the same treatment. The request path in `src/service.cpp:657` mirrors it and
should be checked, not assumed.

**Not in scope here:** the publish direction, which genuinely needs the sertype we
do not own — that is [#0970](archived/0970-cyclone-rmw-should-own-its-sertype.md), and it
subsumes this fix if it lands first.

**Also not in scope:** filling the ABI's `take_loaned_message` slot for this
backend. Upstream returns `RMW_RET_UNSUPPORTED` for it without shared-memory
support, and even its SHM path ends in a `memcpy`; see the amendment to
[design 0038](../design/0038-zero-copy-data-transport.md). Removing the round trip
is the whole of the available win on the buffered path.

## Verification — measured

**Allocation count: 9.93 per message → 2.00.** `data_roundtrip` gained
`NROS_ROUNDTRIP_ITERS`; two runs at different counts under valgrind cancel
session and entity setup and give the per-message cost as a slope. Same harness,
this backend against `origin/main`'s:

| | allocs @1 | allocs @200 | per message |
| --- | ---: | ---: | ---: |
| before | 984 | 2,960 | **9.93** |
| after | 968 | 1,366 | **2.00** |

The remaining 2 are one serdata object and its payload buffer — on the loopback
path Cyclone hands the same serdata to the local reader by reference, so one
message costs one serdata.

The before figure is not an integer, and that is information: the old path's
ostream grew by `realloc`, so its allocation count depended on the sample. Part
of what the round trip cost varied with the message, which is the property a
real-time budget most dislikes. The after figure is exactly 2.00 across 199
messages.

**Correcting this section as first written:** it said the
[phase 394](../roadmap/phase-394-memory-campaign-ledger.md) ledger could report
allocation count per take. It cannot — that instrument reads static RAM out of an
ELF symbol table. Runtime allocation needed an instrument and did not have one.

**Byte-identity: confirmed, with a qualification that matters for sizing.**
`ros2_pubsub_e2e` now prints what a real ROS 2 Humble peer over stock
`rmw_cyclonedds` actually delivers (the payload was widened to 16 characters
first, because the old one came to exactly 24 bytes of CDR — already 4-aligned,
so it could not tell wire bytes from a padded re-encode):

```
WIRE=len:28 hdr:00010000 cdr:25
```

The backend adds nothing — `get_size` returns exactly what `from_ser` was handed.
But **transparent is not unpadded**: the three extra bytes are the RTPS
submessage's 4-byte alignment applied by the SENDER, and the encapsulation
options read `0000` rather than `0003`, so the pad length is not recoverable from
the header either. Two consequences survive this issue rather than being removed
by it — a deserialiser must tolerate trailing bytes (nros-serdes reads by
position, so it does), and a receive buffer cut to a type's exact
`MAX_SERIALIZED_SIZE` can be up to 3 bytes short of what a remote peer delivers.
That belongs to [#0964](0964-two-different-sizes-for-the-same-type.md).

## The third site was CHECKED 2026-09-03 — still unconverted, and the reason is 0976

This issue said `src/service.cpp:657` "mirrors it and should be checked, not
assumed". Checked. It is NOT converted:

* `subscriber.cpp` uses `dds_takecdr` in 8 places — both `subscription_take` and
  `subscription_take_multi` carry the fix.
* `service.cpp`'s `take_typed_wire` (now line 671) still runs the full
  `dds_take` -> `dds_ostream_init(&os, 0, 1 /*xcdr1*/)` ->
  `dds_stream_write_sample` round trip this issue exists to remove. It is reached
  from the request path (line 1022) and the reply path (line 1412).

**And the interesting part: converting it would REMOVE adapters, not conflict
with them.** The first read of this is that the five action adapters
([#0976](0976-service-action-adapters-tested-only-against-ourselves.md)) block the
change, because they reshape bytes the typed path produces. The direction matters:

* `strip_goal_id_len_at` and `strip_nested_cdr_at` correct bytes WE generate.
  A raw `dds_takecdr` returns the PEER's bytes, which a conforming ROS 2 peer
  already emits correctly — so on receive there is nothing to correct.
* `take_fibonacci_get_result_response_wire` exists because
  `dds_stream_read_sample` CRASHES on that type (phase 171.0.b). Taking the
  serdata never calls the stream reader, so the crash path is not on the route.

So the receive half of 0976's adapter set looks like it falls out of this fix
rather than standing in its way. That is a claim about a byte-exact path in an
action protocol, and it is NOT verified here — it needs the Cyclone action E2E
fixtures built and `ros2_pubsub_e2e`'s witness extended to the action types, which
is a fixture build this session did not have room for.

**DONE 2026-09-03 — the third site is converted, and the prediction held.**

`take_typed_wire` now runs `dds_takecdr` -> `ddsi_serdata_size` ->
`ddsi_serdata_to_ser` into the caller's buffer, the same shape `subscriber.cpp`
already had. The typed sample, the `dds_ostream_t`, the re-encode and the two
heap allocations are gone from the request and reply paths.

The prediction above was that converting would REMOVE a receive adapter rather
than conflict with it. It did: `take_fibonacci_get_result_response_wire` existed
because `dds_stream_read_sample` crashes on that type (phase 171.0.b), and there
is no stream read on this path any more, so it is deleted. `write_fibonacci_get_
result_response` stays — it is on the WRITE side, which still decodes, and that
is issue 0970's half.

Acceptance, on fixtures rebuilt against the converted path:

| check | result |
| --- | --- |
| backend ctest suite | 23/23 |
| `ros2_action_e2e`, both directions, real ROS 2 peer | 2 passed |
| `test_native_cyclonedds_rust_action` (nros to nros) | passed |

One red appeared in the first suite run — `ros2_pubsub_e2e`, a lane this change
does not touch. It passed solo (11.4 s) and the whole suite passed 23/23 on
re-run, which is this repo's own guidance applied: retest a QEMU/e2e red SOLO
before believing it.

**This was only checkable because `ros2_action_e2e` exists** (issue 0976). Before
that witness, converting this path would have changed the action wire format with
nothing in the tree able to tell.

**Not measured, and not expected to move:** delivery rate at the fragment sizes
in [#0917](0917-an536-fragmented-sample-never-syncs.md). That cliff is the
LAN9118's RX FIFO capacity and has nothing to do with serialisation. What should
move on that lane is per-message CPU and allocation, so the rate below the cliff
and the jitter — an an536 measurement still owed.

## The third site, mapped — 2026-09-03

Read end to end and written down rather than converted. Everything below is
either quoted from the tree or marked as unverified; the one assumption that
decides whether the conversion is a half-hour job or a redesign is named at the
bottom.

### The conversion, concretely

`take_typed_wire` (`src/service.cpp:671`) mirrors what `subscription_take`
(`src/subscriber.cpp:236-268`) already does:

```cpp
struct ddsi_serdata* d = nullptr;
dds_sample_info_t si[1];
for (;;) {                                   // skip invalid_data, as the
    dds_return_t taken =                     // converted subscriber does
        dds_takecdr(reader, &d, 1, si, DDS_ANY_STATE);
    if (taken < 0)  return wire_status(NROS_RMW_RET_ERROR);
    if (taken == 0) return wire_status(NROS_RMW_RET_NO_DATA);
    if (si[0].valid_data) break;
    ddsi_serdata_unref(d);
    d = nullptr;
}
const uint32_t total = ddsi_serdata_size(d);  // counts the 4-byte CDRHeader
if (out_cap < total) { ddsi_serdata_unref(d); return wire_status(NROS_RMW_RET_BUFFER_TOO_SMALL); }
ddsi_serdata_to_ser(d, 0, total, out_buf);    // header + payload, wire form
ddsi_serdata_unref(d);
return static_cast<int32_t>(total);
```

That deletes, in one move:

* the `dds_ostream_init(&os, 0, 1 /*xcdr1*/)` + `dds_stream_write_sample` pair
  (`:689-691`) — the re-encode this issue is named for, and the reason the path
  emits **XCDR1 native-endian** rather than the peer's own representation. That
  is a correctness question, not only a cost one: the endianness bytes at
  `:701-707` are written from the HOST's `__BYTE_ORDER__`, so the bytes handed
  to `split_wire_header` are re-labelled rather than passed through;
* the `_SendGoal_*` / `_GetResult_Request_` memcpy fallback (`:692-714`), which
  exists only because `dds_stream_write_sample` returns false for those types;
* the `Fibonacci_GetResult_Response_` special case (`:682-687` calling
  `take_fibonacci_get_result_response_wire` at `:503`), which exists only
  because `dds_stream_read_sample` **crashes** on that type. A serdata take
  never enters either function.

### Two things that check out

**`split_wire_header` is transparent to this.** It already takes *wire* CDR —
encap, then the 8-byte request header, then user fields (`:585-604`) — and it is
called on `take_typed_wire`'s output at `:1022` (server request) and `:1412`
(client reply). A serdata take hands it the peer's bytes in the same shape the
reserialiser was reconstructing, so the strip logic is unaffected.

**The write-side half of the adapter hypothesis is now measured.** The
2026-09-03 section above guessed that converting this path would DELETE 0976's
adapters rather than have to preserve them. For the write side that is no longer
a guess: instrumenting both branches of `write_typed` and running the Rust, C
and C++ action clients against a stock ROS 2 server shows `strip_goal_id_len_at`
and `strip_nested_cdr_at` declining in all three with identical counts.

### The assumption that has to be checked first

`subscriber.cpp` creates its topic with `dds_create_topic_sertype`
(`subscriber.cpp:164`). `service.cpp` creates its with `dds_create_topic(desc)`
(`:946-947`, `:1221-1222`) and allocates a `SertypeMin` alongside (`:976-977`,
`:1248-1249`).

**Whether `dds_takecdr` yields usable serdata on a reader whose topic came from
a descriptor rather than a sertype is NOT verified here.** It is the kind of
thing that ought to work — serdata is Cyclone's internal representation either
way, and this issue's own argument for the receive path is that `dds_takecdr`
takes no sertype — but "ought to" is what this file exists to distrust. 0970's
step 4, checking this path's request/reply handling against upstream's
`cdds_request_wrapper_t`, is recorded as undone and is the same question from
the other side.

If it holds, the conversion is the block above plus deleting three helpers. If
it does not, the topics must move to `dds_create_topic_sertype` first — which is
0970's service half, a larger change, and the reason `sertype_min.{hpp,cpp}`
still exists.

### Why it was not converted here

Blast radius is services AND actions, and the direction with no automated test
is exactly the one the deleted helpers serve:
`take_fibonacci_get_result_response_wire` fires only when nano-ros is the CLIENT
taking a result (`:1412`), and the action witness (`ros2_action_e2e.rs`) runs
the server direction only. A rewrite of this path with no test that would catch
a regression is the shape this repo keeps retracting.

### Order for whoever takes it

1. Answer the `dds_takecdr`-on-a-desc-topic question — a throwaway assertion in
   `tests/` is enough, and it decides which of the two changes this is.
2. Land the client-direction witness 0976 asks for (nros action client against a
   stock `ros2` action server), so the three deletions have something watching
   them.
3. Convert, delete the three helpers, and re-run the allocation harness
   (`tests/data_roundtrip.cpp`, `NROS_ROUNDTRIP_ITERS`) — the message path went
   9.93 → 2.00 allocations/message and this path still carries three sites, one
   per request and two per reply.
