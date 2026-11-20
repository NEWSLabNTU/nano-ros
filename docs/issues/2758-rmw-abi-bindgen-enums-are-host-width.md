---
id: 2758
title: "The RMW ABI's Rust bindings give every C enum the host's 4-byte width; on arm-none-eabi the C enum is 1 byte"
status: open
type: bug
severity: medium
area: rmw
related: [issue-0238, issue-1330, issue-2755, rfc-0054]
found: 2026-11-20
---

## Measured

nano-ros main at 0dc5331b28 (the sweep ran on c7884b827d, the issue 2755
branch; the lines below are unchanged on main).

`scripts/gen-abi-bindings.sh` runs bindgen on the HOST with
`--default-enum-style moduleconsts`, so every unfixed C enum in
`packages/core/nros-rmw-abi/include/nros/*.h` becomes
`pub type Type = core::ffi::c_uint;` in `packages/rmw/cffi/src/generated.rs`.
That is 4 bytes on every target. On arm-none-eabi the AAPCS default is
variable-size enums (`readelf -A`: `Tag_ABI_enum_size: small`), so the same
enum is 1 byte in C. Same class as issue 2755 (Cyclone's plist code read
1-byte enum fields as 4-byte `unsigned`), here on our own ABI.

Probe: the rmw ABI headers compiled with
`arm-none-eabi-gcc -mcpu=cortex-r52 -mfloat-abi=hard -mfpu=fp-armv8`, and
`generated.rs` compiled with `rustc --target armv8r-none-eabihf`, each
exporting `sizeof` / `offsetof` as symbol sizes, compared with
`nm --print-size` (the method of `check-repr-memory-agreement`).

| quantity | C (arm-none-eabi) | Rust `generated.rs` (armv8r-none-eabihf) |
|---|---|---|
| `sizeof(rmw_qos_compatibility_type_t)` | 1 | 4 |
| `sizeof(rmw_endpoint_type_t)` | 1 | 4 |
| `sizeof(rmw_network_flow_endpoint_t)` | 60 | 68 |
| `offsetof(rmw_network_flow_endpoint_t, transport_port)` | 2 | 8 |
| `offsetof(rmw_network_flow_endpoint_t, flow_label)` | 4 | 12 |
| `offsetof(rmw_network_flow_endpoint_t, dscp)` | 8 | 16 |
| `offsetof(rmw_network_flow_endpoint_t, internet_address)` | 9 | 17 |
| `offsetof(rmw_topic_endpoint_info_t, endpoint_type)` | 12 | 12 |
| `offsetof(rmw_topic_endpoint_info_t, endpoint_gid)` | 16 | 16 |

Every other enum in those headers (`rmw_event_type_t`, `rmw_feature_t`,
`rmw_log_severity_t`, `rmw_liveliness_kind_t`, `nros_rmw_qos_clash_t`,
`rmw_transport_protocol_t`, `rmw_internet_protocol_t`,
`nros_rmw_session_mode_t`) is also 1 byte in C there and `c_uint` in Rust.

`rustc` itself is not the problem: `c-enum-min-bits` is 8 for every ARM
target in `config/rust-targets.txt` and for the NuttX ARM targets, so a
`#[repr(C)]` enum would agree with C. The width is lost because bindgen ran
on x86_64 and wrote the host's answer into a committed, target-independent
file.

## Where it bites

1. **Out-parameter overrun (live for any C caller).**
   `rmw_qos_profile_check_compatible` and `nros_rmw_qos_incompatibility_mask`
   (`rmw_entity.h:536-555`) are declared in C with
   `rmw_qos_compatibility_type_t *compatibility` and implemented in Rust
   (`packages/rmw/cffi/src/lib.rs:6249`, `:6283`) as
   `*mut rmw_qos_compatibility_type_t::Type`, i.e. `*mut c_uint`. The ARMv8-R
   FreeRTOS Cyclone build of the issue 2755 branch has the function in
   `libnros_cpp.a`; its disassembly stores the verdict with a word store:

   ```
   134:  ldr  r1, [fp, #40]   @ compatibility
   144:  str  r0, [r1]        @ 4-byte store
   ```

   A C caller that does what upstream ROS code does,
   `rmw_qos_compatibility_type_t c; rmw_qos_profile_check_compatible(p, s, &c, NULL, 0);`,
   gets 3 bytes past `c` overwritten, and an unaligned word store if `c`
   sits at an odd address (a fault on cores without unaligned access, e.g.
   Cortex-M0). No in-tree C or C++ caller exists today; the function is
   public upstream API, so the first ported caller is the trigger.

2. **Struct layout mismatch (latent).** `rmw_network_flow_endpoint_t` is 60
   bytes in C and 68 in Rust, with every field after the two enums at a
   different offset. Nothing produces or consumes it yet; the first slot
   that streams flow endpoints breaks.

3. **Padding read (masked).** `rmw_topic_endpoint_info_t.endpoint_type`
   sits at offset 12 on both sides, so the layout agrees, but Rust reads 4
   bytes where C wrote 1 (`lib.rs:2146`,
   `info.endpoint_type == RMW_ENDPOINT_PUBLISHER`). It works today only
   because the one C producer, Cyclone's `graph_query.cpp:485`, memsets the
   struct first. A backend that fills the struct field by field makes every
   endpoint read as "not a publisher" whenever the 3 padding bytes are not
   zero.

By-value enum arguments and returns across the vtable (`kind`, `feature`,
`severity`) are not affected: AAPCS extends a sub-word integer argument or
result to a full register, so a 1-byte enum and a `c_uint` carry the same
value. Only memory (pointers and struct fields) disagrees.

## Why nothing caught it

- `packages/rmw/cffi/tests/c_stubs/abi_layout_check.c:37` pins
  `sizeof(rmw_event_type_t) == 4` HOST-side only, and its comment says "On
  ARM EABI the same enum is 1 byte on BOTH sides". That was true while the
  Rust mirror was `#[repr(C)]` (issue 0238's fix); RFC-0054 replaced the
  mirror with bindgen output and the claim went stale.
- `just check abi-bindings` compares the committed bindings with a fresh host
  generation, which reproduces the same host width.
- `check-ffi-struct-mirrors` compares C/C++ declarations textually (field
  names, enumerators) and never measures a width on a target.
- `check-repr-memory-agreement` does cross-compile both sides for ARM and
  compare `sizeof`/`offsetof`, but only for generated message types, not for
  the rmw ABI.

## Direction

Pin the width in the header, as issue 1330 did for `rcl_log_severity_t`
(`packages/api/nros-c/include/rclc/log.h`): give each rmw ABI enum a
`..._FORCE_INT_WIDTH_ = 0x7fffffff` enumerator, so it is `int`-sized on every
ABI and bindgen's `c_uint` is right everywhere, and add a
`_Static_assert(sizeof(T) == sizeof(int))` next to each enum in the header
itself, so any ARM C TU that includes the header proves it. Values stay
upstream's; only the storage width changes. The alternative, switching these
fields and the out-parameter to fixed-width integer typedefs, changes the
upstream-parity spelling of public types and is the bigger edit.

`-fno-short-enums` is not a fix, for the reason `log.h` gives: every
consumer, in tree and out, would need it.

## Acceptance

- The probe above reports equal C and Rust `sizeof`/`offsetof` for every
  rmw ABI enum and every struct that holds one, on an ARM EABI target.
- `abi_layout_check.c`'s host-only pin and its stale comment are replaced by
  the in-header asserts.
- A gate fails when a `typedef enum` is added to an ABI header without a
  width pin (or the cross-target probe covers the rmw ABI).
