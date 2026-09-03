---
id: 970
title: "The Cyclone backend borrows Cyclone's generated sertype instead of registering its own, and that — not an upstream gap — is what forces the CDR round trip on publish"
status: resolved
area: [rmw]
severity: medium
related: [0969, 0958, 0896, phase-391, 0038]
---

# The blocker we recorded was "Cyclone hasn't exposed this yet". It was "we don't own the sertype".

## The claim on file

`packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/sertype_min.hpp:6-29`:

> Cyclone's `dds_writecdr` / `dds_takecdr` raw-CDR API needs a real
> `ddsi_sertype *` linked to a `ddsi_domaingv`, which our backend can't get to
> without reaching into Cyclone's private struct layout. We sidestep that path
> entirely:
>
>     publish_raw  : CDR bytes →  dds_stream_read_sample → typed buf
>                    → dds_write (Cyclone re-serialises) → wire
>     take_serialized : wire → dds_take (typed buf)
>                    → dds_stream_write_sample → CDR bytes → caller
>
> […] Cost: a 2× CDR roundtrip per publish + per recv. […] A future zero-copy fast
> path can replace this once Cyclone exposes `dds_writer_lookup_serdatatype`
> upstream.

`SertypeMin` is therefore not a sertype the domain knows about. It is a
hand-populated `ddsi_sertype_default` used purely as an argument to the public
cdrstream helpers (`dds_stream_read_sample` / `dds_stream_write_sample`), with
`serpool` and every other gv-derived field zeroed. The topic itself is created
from the generated `dds_topic_descriptor_t`, so the reader and writer use
**Cyclone's** sertype, and Cyclone's sertype deals in typed C structs. Hence the
round trip in both directions.

## What the reference implementation does instead

`ros2/rmw_cyclonedds` never uses Cyclone's generated descriptor. It builds its own
sertype and registers it:

```cpp
tp = dds_create_topic_sertype(pp, name, &sertype, nullptr, nullptr, nullptr);
```

(`rmw_cyclonedds_cpp/src/rmw_node.cpp:1993`.) Its serdata is the bytes
(`src/serdata.hpp:57-60`):

```cpp
size_t m_size {0};
/* first two bytes of data is CDR encoding
   second two bytes are encoding options */
std::unique_ptr<byte[]> m_data {nullptr};
```

Wire CDR arrives, is kept as CDR, and is only decoded if some caller asks for a
typed message (`serdata_rmw_to_sample_impl`). Publishing goes the other way with
no typed struct at all — `serdata_rmw_from_serialized_message(...)` then
`dds_forwardcdr(pub->enth, d)` (`rmw_node.cpp:2098`, `:2146`).

`dds_writer_lookup_serdatatype` never appears in that file. It is not needed,
because when you register the sertype you already hold the pointer that a serdata
has to be constructed against. The API we were waiting on is the API for
recovering a sertype you don't own; owning it makes the question moot.

`dds_create_topic_sertype` is public, and present in our vendored Cyclone
(`third-party/dds/cyclonedds/src/core/ddsc/include/dds/dds.h:1393`, guarded by the
`DDS_HAS_CREATE_TOPIC_SERTYPE` feature macro at `:1351`).

## Why this is filed separately from #0969

[#0969](0969-cyclone-take-cdr-round-trip.md) removes the **receive** round trip
with `dds_takecdr`, which needs no sertype at all and is landable now, in that one
function. This issue is the larger change: an nros sertype and serdata, which
removes the **publish** round trip too and makes #0969's receive fix fall out for
free. It carries real risk that #0969 does not — the sertype's op table is a
Cyclone-internal contract, keys and instance handling have to be right, and
`service.cpp`'s request-header wrapping (`cdds_request_wrapper_t` upstream) is
entangled with it.

So: #0969 first, this after, and this subsumes it. Recorded now so the retracted
blocker doesn't get re-derived from the comment a third time.

## Direction

1. Land #0969, so receive stops round-tripping regardless of what happens here.
2. Prototype an `nros_sertype` / `nros_serdata` pair modelled on
   `rmw_cyclonedds_cpp/src/serdata.{hpp,cpp}` — serdata holds CDR, `to_ser` is a
   `memcpy`, `to_sample` is only implemented if something actually needs a typed
   struct (nothing in nano-ros does; the whole point is that our callers speak
   bytes).
3. Switch topic creation to `dds_create_topic_sertype`, publish via
   `dds_forwardcdr`, delete `SertypeMin`.
4. Check `service.cpp` request/reply header handling against the upstream
   `cdds_request_wrapper_t` treatment before assuming step 3 covers it.

## What this does not buy

Not zero copy. The buffered take contract has the caller own the buffer, so one
`memcpy` out of the serdata remains, and upstream's loaned-message path is
`RMW_RET_UNSUPPORTED` without shared memory — see the amendment to
[design 0038](../design/0038-zero-copy-data-transport.md). What it buys is the
removal of a decode and an encode, and every heap allocation that goes with them,
from both directions of a control loop's data path.

## Resolved 2026-09-03 — the service half, and the round trip is gone

Steps 1-4 of the direction above are done. The message half had already landed
(`publisher.cpp` / `subscriber.cpp` on `create_nros_sertype` +
`dds_create_topic_sertype`); this is `service.cpp`.

* Its four topic creations moved from `dds_create_topic(pp, desc, ...)` — which
  registers CYCLONE's sertype, the one that deals in typed C structs — to
  `dds_create_topic_sertype` with ours.
* `write_typed` is three lines: `const NrosCdrBlob blob{wire_cdr, wire_len};
  dds_write(writer, &blob)`. The `dds_istream_t`, the `dds_stream_read_sample`
  and the per-publish `ddsrt_calloc` are gone, and `SertypeMin` is unused on this
  path.
* Step 4's worry — the request-header wrapping — turned out to need nothing. The
  16-byte header is INLINE in the CDR nano-ros builds, not a DDS-level wrapper,
  so a blob sertype carries it like any other bytes. Upstream's
  `cdds_request_wrapper_t` solves a problem this protocol does not have.

**Three more adapters fell out**, which is what issue 0969 predicted and the
reason it said this change subsumes it. Each existed to work around the typed
round trip, not the wire: the `_SendGoal_*` / `_GetResult_Request_` memcpy
branch, `write_fibonacci_get_result_response` (which hand-built the generated C
layout because `dds_stream_read_sample` CRASHES on that type — phase 171.0.b),
and `type_contains`, its only caller. With no stream read there is nothing to
work around.

**The measurable result**, from the allocation ledger the gate maintains:

    cyclonedds steady-state sites   2 -> 1

The survivor is `serdata_alloc`, the one copy a caller-owns-the-buffer contract
cannot avoid.

**Read that as what it is: a count of allocation SOURCE SITES, not of runtime
allocations.** This section first went on to say per-message allocation was
"otherwise gone" from the service and action data path. The ledger does not
support that claim and the runtime measurement contradicts it — see
[#0969](../0969-cyclone-take-cdr-round-trip.md), where the allocation COUNT comes
out unchanged before and after, and the BYTES cross over at ~6 KB rather than
falling. A site disappearing from a ledger is not a call disappearing at runtime.

**What the change does remove at runtime, measured** (bench in
`tests/codec_bench.cpp`, numbers in 0969): the decode+encode pair, worth a
**~46 ns floor per message** at any size and 176 ns at a 16 KB payload, against
a `memcpy` of 0.8-76 ns over the same range. That is the win this migration
actually buys, and unlike the byte curve it holds at every payload size.

`check-rmw-alloc-sites` is what caught the leftovers: it failed with "DECLARED
names a steady-state site that is gone", which is how `write_fibonacci_get_result_
response` was found to be dead rather than merely unused-looking. A ledger that
fails when reality shrinks is worth as much as one that fails when it grows.

Acceptance:

| check | result |
| --- | --- |
| backend ctest (incl. `ros2_srv_e2e` vs stock ROS 2) | 23/23 |
| `ros2_action_e2e`, both directions, real ROS 2 peer | 2 passed |
| `test_native_cyclonedds_rust_action` (nros to nros) | passed |
| `just check fast` | 188 gates |

**Keys were the stated risk and did not bite**: `create_nros_sertype` refuses a
keyed descriptor, and every service request/reply descriptor in this tree is
`m_nkeys = 0u`. Checking that first is what made this mechanical rather than
speculative.

None of it was checkable before `ros2_action_e2e` (issue 0976). This change alters
what goes on the wire for every service and action; before that witness, nothing
in the tree could have told.