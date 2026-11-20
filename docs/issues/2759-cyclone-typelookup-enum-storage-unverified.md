---
id: 2759
title: "Cyclone's TypeLookup reply descriptor stores a 1-byte enum as 4 bytes on arm-none-eabi (unverified at run time)"
status: open
type: bug
severity: low
area: [rmw, third-party]
related: [issue-2755, issue-2758]
found: 2026-11-20
---

## Measured

Found by the issue 2758 sweep, same class as issue 2755. Cyclone at the
submodule pin on main (05b3c9ed, which carries the 2755 fix), ARMv8-R FreeRTOS Cyclone image
(`Tag_ABI_enum_size: small`).

Cyclone's serializer (`ddsi_cdrstream.c`) takes the storage size of an enum
member from the descriptor's opcode (`DDS_OP_TYPE_SZ`), and the descriptors
idlc writes assume a C enum is `int`-sized. One such descriptor is compiled
into every image that has type discovery: `DDS_Builtin_TypeLookup_Reply_ops`
in `src/core/ddsi/src/ddsi_xt_typelookup.c`. Its `ReplyHeader` entry is

```
DDS_OP_ADR | DDS_OP_TYPE_ENU | (2 << DDS_OP_FLAG_SZ_SHIFT), offsetof (DDS_RPC_ReplyHeader, remoteEx), 5u,
```

so `remoteEx` is read and written as 4 bytes (`(const uint32_t *) addr` in
`dds_stream_write_enum_arrBO`; `*((uint32_t *) discaddr) = 0` in
`skip_union_default`). In C it is `DDS_RPC_RemoteExceptionCode`, an unfixed
enum with values 0..5.

The built object (`ddsi_xt_typelookup.c.obj` from `libddsc.a`) carries the
opcode word `0x010c0080` at offset 0x28 of `.rodata.DDS_Builtin_TypeLookup_Reply_ops`
(ENU, size flag 2 = 4 bytes, field offset 0x18). A probe of the same struct
with `arm-none-eabi-gcc -mcpu=cortex-r52`:

| quantity | value |
|---|---|
| `sizeof(DDS_RPC_RemoteExceptionCode)` | 1 |
| `offsetof(DDS_RPC_ReplyHeader, remoteEx)` | 24 |
| `sizeof(DDS_RPC_ReplyHeader)` | 28 |
| bytes the serializer reads/writes at offset 24 | 4 |

The 4-byte access stays inside the struct (bytes 25..27 are tail padding),
so it does not write outside the object. On serialize the value read is
`remoteEx | (padding << 8)`, which is only right when the padding is zero.

## Not verified

- Whether the reply sample is zero-initialised before `remoteEx` is set (if
  it is calloc'd or memset, the value is right and this is latent).
- Whether any peer sends a TypeLookup request to a nano-ros Cyclone image, and
  what a non-zero padding byte does then (`dds_stream_write_enum_arrBO`
  checks the value against the max of 5, so the likely symptom is a refused
  reply and a peer that cannot resolve our types).
- Other idlc-generated descriptors with enum members: this was the only
  `DDS_OP_TYPE_ENU` found in Cyclone's own sources outside the serializer,
  the type builder and idlc. Our generated message types have no enums (ROS
  IDL has none). A user IDL with an enum would hit the same assumption.

`ddsi_typebuilder.c` also sizes an enum as `uint32_t`, but it lays out its own
samples, so it is consistent with itself.

## Direction

Measure first: a TypeLookup round trip against a Cyclone peer with type
discovery on, on an ARM EABI image. If it bites, the fix belongs in the
Cyclone fork beside 2755's (`05b3c9ed`): either a size flag from the target's
`sizeof(enum)` in the descriptor, or a width pin on the enum in the generated
header.
