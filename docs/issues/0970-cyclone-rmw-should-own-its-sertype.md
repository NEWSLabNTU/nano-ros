---
id: 970
title: "The Cyclone backend borrows Cyclone's generated sertype instead of registering its own, and that — not an upstream gap — is what forces the CDR round trip on publish"
status: open
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
>     try_recv_raw : wire → dds_take (typed buf)
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
