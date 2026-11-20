---
id: 2759
title: "Cyclone's TypeLookup reply descriptor stores a 1-byte enum as 4 bytes on arm-none-eabi (unverified at run time)"
status: resolved  # not a live defect: the reply sample is zeroed; measured on QEMU mps3-an536
type: bug
severity: low
area: [rmw, third-party]
related: [issue-2755, issue-2758]
found: 2026-11-20
resolved: 2026-11-20
resolved_in: "docs(issue 2759): the TypeLookup reply enum is latent, the sample is zeroed"
---

## Finding (kept)

`DDS_Builtin_TypeLookup_Reply_ops` in Cyclone's
`src/core/ddsi/src/ddsi_xt_typelookup.c:178` describes `remoteEx`
(`DDS_RPC_RemoteExceptionCode`) as a 4-byte enum. On arm-none-eabi
(`Tag_ABI_enum_size: small`) the C enum is 1 byte at offset 24 of the 28-byte
`DDS_RPC_ReplyHeader`, so the serializer reads `remoteEx | padding << 8` and
the deserializer writes bytes 24..27. Both stay inside the struct.

## Why it does not bite

Cyclone at the pin on main (05b3c9eda5) zeroes the sample on both paths
before the field is touched:

- send: `create_tl_reply_msg` (`src/core/ddsi/src/ddsi_typelookup.c:230`)
  does `memset (reply, 0, sizeof (*reply))` and only then sets
  `remoteEx` (line 235). It is the only place a reply sample is built.
- receive: `ddsi_tl_handle_reply` (`ddsi_typelookup.c:389`) memsets the
  sample before `ddsi_serdata_to_sample`, and nothing reads `remoteEx`
  after deserialising.

So the tail padding is always zero when the 4-byte read happens.

## Measured

Image: fixture `workspace-cpp-mps3-an536-freertos-slirp` (FreeRTOS + lwIP +
Cyclone, type discovery on, `Tag_ABI_enum_size: small`), QEMU mps3-an536,
user networking with the guest's two unicast ports forwarded, a pcap from
QEMU's `filter-dump` on the guest NIC.

Peers on the host, Cyclone 0.10.5 from ROS 2 Humble:

- A creates a topic from an idlc descriptor that carries type information
  and writes on it;
- B discovers A's writer from the DCPSPublication builtin topic and calls
  `dds_get_typeobj` on its type id. That writes a TypeLookup request on B's
  builtin request writer, which goes to every matched request reader: A's
  and the image's.

nano-ros's own endpoints advertise no type information (its sertype has
none), so no peer ever asks the image about its own types; but the image
answers any request it receives, with an empty type list when it does not
know the id, and that reply carries `remoteEx`.

| run | image's reply on the wire | B's trace |
|---|---|---|
| as built | DATA from writer `0x000301c3`, `remoteEx` = `00 00 00 00` | `tl-reply-add-types wr <image>:301c3 seqnr 1 ntypeids 0`, next to A's `ntypeids 1` |
| control: bytes 25..27 of the header set to `0xff` before `remoteEx` (debug patch, not kept) | request received (frame from B's `0x000300c3`), no reply sent | only A's reply |

The control shows the run can see the failure: with non-zero padding the
serializer reads `0xffffff00`, refuses it against the max of 5, and the
image sends nothing, so the peer never gets a reply from it.

## Left as is

The descriptor still assumes an int-sized enum. That is upstream idlc's
convention and holds wherever the sample is zeroed. Any other idlc-generated
descriptor with an enum member would carry the same assumption; nano-ros
compiles none (ROS IDL has no enums), so there is nothing else in an image to
fix today.
