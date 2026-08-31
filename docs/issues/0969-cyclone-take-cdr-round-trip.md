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
do not own — that is [#0970](0970-cyclone-rmw-should-own-its-sertype.md), and it
subsumes this fix if it lands first.

**Also not in scope:** filling the ABI's `take_loaned_message` slot for this
backend. Upstream returns `RMW_RET_UNSUPPORTED` for it without shared-memory
support, and even its SHM path ends in a `memcpy`; see the amendment to
[design 0038](../design/0038-zero-copy-data-transport.md). Removing the round trip
is the whole of the available win on the buffered path.

## Verification

Before/after on the an536 lane: allocation count per take (the rlsf ledger from
[phase 394](../roadmap/phase-394-memory-campaign-ledger.md) can report it), and delivery
rate at the fragment sizes characterized in
[#0917](0917-an536-fragmented-sample-never-syncs.md). The bytes the caller sees should
become byte-identical to the wire payload, encapsulation header included — that
equality is the sharpest test that the round trip is gone.
