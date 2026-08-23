---
id: 331
title: "RMW ABI seams: create_session carries zenoh's whatami as an undocumented uint8_t mode, and set_custom_transport bypasses the RFC-0054 generated type with no layout assert"
status: resolved
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

`packages/rmw/cffi/src/lib.rs:1008` exports

```rust
pub unsafe extern "C" fn nros_rmw_cffi_set_custom_transport(
    ops: *const nros_rmw::NrosTransportOps,   // hand-written Rust type
) -> NrosRmwRet
```

while the header declares `rmw_ret_t
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
`rmw_service_t`/`rmw_client_t` (`rmw_entity.h:332,347`) carry no `qos`
field, unlike their publisher/subscription peers, so a backend cannot read back
the profile it was created with. → append to 0242.

## Resolution (2026-07-28)

### Part 2 — `set_custom_transport` SSoT bypass: fixed

`nros_rmw_cffi_set_custom_transport` now takes
`generated::nros_transport_ops_t` — the committed bindgen output of the header
that RFC-0054 makes the SSoT — instead of the hand-written
`nros_rmw::NrosTransportOps`. The two are still bridged by a `transmute_copy`
(the Rust-side setter takes the Rust type), but the bridge is guarded by a
`const _` block asserting equal size AND alignment, so a drift that used to be
silent is now a build failure.

Taking the generated type also exposed a latent hole the old signature hid: the
generated struct's fn slots are `Option<fn>` because C pointers are nullable,
while `NrosTransportOps`' are plain `fn`. The two are layout-identical via the
null-pointer optimization, so a C caller passing a NULL callback produced an
invalid `fn` that was UB the moment the runtime called it. The export now
rejects a NULL `open`/`close`/`write`/`read` with
`NROS_RMW_RET_INVALID_ARGUMENT` before the copy, and refusing does not clobber
a previous install.

Gates: `_Static_assert`s for `nros_transport_ops_t`'s size and alignment added
to `nros-rmw-cffi/tests/c_stubs/abi_layout_check.c`, expressed as
`2 * sizeof(uint32_t) + 5 * sizeof(void*)` so they hold on 32-bit targets too.
New test `null_callback_slot_rejected` in `tests/set_custom_transport.rs`;
mutation-checked (removing the guard makes it FAIL).

The header's own doc had the SSoT relationship written **backwards** — it
described the C declaration as a "`#[repr(C)]` mirror of the Rust-side
`NrosTransportOps`", which is exactly inverted post-RFC-0054. Corrected.

### Part 1 — the undocumented `uint8_t mode`: documented, not restructured

The legal values are now specified: `nros_rmw_session_mode_t` in
`rmw_vtable.h` declares `NROS_RMW_SESSION_MODE_CLIENT = 0` and
`NROS_RMW_SESSION_MODE_PEER = 1`, and the `create_session` slot documents the
parameter, states that a backend with no client/peer distinction must IGNORE it
rather than reject it, and records WHY the seam diverges from
`rmw_init_options_t`. The Rust boundary in `nros-rmw-cffi` now maps
`SessionMode` onto those named constants instead of restating bare `0u8`/`1u8`.

The structural half — folding the mode into backend-private config behind the
locator, so the agnostic vtable stops carrying a backend-shaped field — is NOT
done. It is the same class as issue 0330 (backend facts in agnostic layers) and
is better done alongside that issue's part 3 than as an isolated ABI break.

