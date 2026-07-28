---
id: 331
title: "RMW ABI seams: create_session carries zenoh's whatami as an undocumented uint8_t mode, and set_custom_transport bypasses the RFC-0054 generated type with no layout assert"
status: open
type: bug
severity: medium
area: core
related: [issue-0242, rfc-0054]
---

## Finding (audit 2026-07-28, P2)

Two C7/C3 items on the RMW ABI surface — the seam RFC-0054 made the SSoT.

### 1. A backend-specific parameter in the vtable signature

`packages/core/nros-rmw-abi/include/nros/rmw_vtable.h:51` — `create_session`
takes a `uint8_t mode` carrying `SessionMode::{Client,Peer}`, i.e. **zenoh's
`whatami`**. There is no `rmw.h` counterpart: `rmw_init_options_t` carries
domain_id, enclave, security_options and discovery_options, and no session mode.
Unlike every neighbouring slot, this parameter has no doc comment and no
legal-value list.

Fix: fold it into backend-private config behind the locator, or introduce an
`rmw_init_options`-shaped `nros_rmw_session_options_t` with a documented rationale
for the divergence.

### 2. The custom-transport export bypasses its own generated type

`packages/core/nros-rmw-cffi/src/lib.rs:1008` exports

```rust
pub unsafe extern "C" fn nros_rmw_cffi_set_custom_transport(
    ops: *const nros_rmw::NrosTransportOps,   // hand-written Rust type
) -> NrosRmwRet
```

while the header declares `nros_rmw_ret_t
nros_rmw_cffi_set_custom_transport(const nros_transport_ops_t *ops)`
(`rmw_transport.h:153`). Under RFC-0054 the header is the ABI SSoT and Rust is
supposed to consume the **committed bindgen output**; this export consumes the
hand-written mirror instead, so the two can drift silently and the C caller's
struct layout is only accidentally correct.

The header's own doc inverts the SSoT — it describes `nros_transport_ops_t` as a
"`#[repr(C)]` mirror of the Rust-side `NrosTransportOps`", which is backwards
post-RFC-0054.

`tests/c_stubs/abi_layout_check.c` has `_Static_assert`s for the qos, options,
entities and vtable structs but **none for `nros_transport_ops_t`**, so nothing
catches a drift.

*Correction to the audit lane's original claim:* the generated mirror is **not**
dead repo-wide — `packages/core/nros-c/src/transport.rs` and
`packages/core/nros-rmw/src/custom_transport.rs` do use it. It is bypassed at
this one export, which is narrower but still an SSoT hole.

Fix: switch the export to the generated type; add the missing size/offset
`_Static_assert` and a name-parity entry; correct the header doc's direction.

## Related, not filed separately

Further `rmw.h` concept gaps found while reading the vtable belong to open issue
**0242** (RMW parity gaps) rather than a new issue: no `rmw_wait`/waitset analogue
(covered by the differently-shaped `has_data`/`has_request` polling pair, with no
note explaining the reshape), no
`rmw_{publisher,subscription}_get_actual_qos`, no
`rmw_serialize`/`rmw_deserialize`/`rmw_get_serialization_format`, no graph
introspection (`rmw_get_topic_names_and_types`, `rmw_count_publishers`), no
`rmw_publisher_wait_for_all_acked`, no content filters. Also asymmetric:
`create_service`/`create_client` take a `qos` argument but
`nros_rmw_service_t`/`nros_rmw_client_t` (`rmw_entity.h:332,347`) carry no `qos`
field, unlike their publisher/subscription peers, so a backend cannot read back
the profile it was created with. → append to 0242.
