---
rfc: 0035
title: "RMW vtable ABI — frozen slot table + stability policy"
status: Draft
since: 2026-06
last-reviewed: 2026-06
implements-tracked-by: []
supersedes: []
superseded-by: null
---

# RFC-0035 — RMW vtable ABI: frozen slot table + stability policy

## Summary

`nros_rmw_vtable_t` is the C function-pointer table every RMW backend
implements and the runtime calls. It is the project's primary cross-language,
cross-compilation-unit ABI: three backends (zenoh-pico, XRCE-DDS, Cyclone DDS)
populate it and `nros-rmw-cffi` consumes it. The table has grown to **35 slots**
across Phases 104/108/110/124/130/231/301, but its layout is frozen only by convention —
there is no `abi_version` field and no written stability contract. This RFC
**records the current slot table as the canonical ABI**, defines the
append-only-to-tail evolution rule, the per-slot NULL/fallback contract, and the
`abi_version` field to add (mirroring the already-versioned `NrosTransportOps`).
RFC-0006 motivated the C-ABI-is-canonical stance; this RFC is the concrete,
enumerated freeze that RFC-0006 left to follow-up.

## Motivation / problem

- **One checkout = one ABI** (CLAUDE.md) is asserted but unenforced: a backend
  built against an older slot count linked against a newer runtime (or vice
  versa) has no version field to reject, so a layout skew is silent UB.
- The sibling transport vtable `NrosTransportOps`
  (`nros-rmw-abi/include/nros/rmw_transport.h:84`) already carries
  `abi_version: u32` + `NROS_TRANSPORT_OPS_ABI_VERSION_V1 = 1` and a
  registration check; the RMW vtable does not, despite being the more
  load-bearing interface.
- `NROS_RMW_RET_INCOMPATIBLE_ABI = -14`
  (`nros-rmw-abi/include/nros/rmw_ret.h:99`) is defined but never returned.
- NULL-slot semantics (runtime fallback vs `RET_UNSUPPORTED`) are decided
  per-slot in code with no contract a backend author can rely on.

Constraints: `no_std`, C ABI, zero allocation in registration, three live
backends that must not break.

## Design

### Canonical header

`packages/core/nros-rmw-abi/include/nros/rmw_vtable.h` is the **source of
truth** (hand-written canonical C, per RFC-0006 R2). The `#[repr(C)]`
`NrosRmwVtable` in `nros-rmw-cffi/src/lib.rs` must match it field-for-field and
in order. This RFC records the contract; the header records the exact
signatures.

### Slot table (frozen order)

Slots are grouped by entity but the **wire order is the struct field order** —
never reorder. Required slots must be non-NULL; optional slots are
`Option<fn>` / nullable in C.

Phase-301 (issues 0240/0241) renamed the entity/slot vocabulary to upstream
rmw's terms — `open`/`close` → `create_session`/`destroy_session`,
`subscriber` → `subscription`, `service_server` → `service`,
`service_client` → `client` — deleted the deprecated blocking `call_raw`
slot, and moved the transport hints (`tx_express`, `rx_buffer_hint`) out of
`nros_rmw_qos_t` into NULLable trailing `nros_rmw_publisher_options_t` /
`nros_rmw_subscription_options_t` params on `create_publisher` /
`create_subscription`.

| # | slot | kind | NULL behaviour |
|---|------|------|----------------|
| 1–3 | `create_session`, `destroy_session`, `drive_io` | session, required | — |
| 4–6 | `create_publisher`, `destroy_publisher`, `publish_raw` | pub, required | — |
| 7–10 | `create_subscription`, `destroy_subscription`, `try_recv_raw`, `has_data` | sub, required | — |
| 11–15 | `create_service`, `destroy_service`, `try_recv_request`, `has_request`, `send_reply` | service, required | — |
| 16–17 | `create_client`, `destroy_client` | client, required | — |
| 18–19 | `send_request_raw`, `try_recv_reply_raw` | async request/reply (P130.4; required since phase-301 — the blocking `call_raw` slot is deleted) | — |
| 20–21 | `register_subscription_event`, `register_publisher_event` | QoS events (P108), required | backend returns `RET_UNSUPPORTED` for unsupported kinds |
| 22 | `assert_publisher_liveliness` | liveliness (P108.B), required | — |
| 23 | `next_deadline_ms` | deadline (P110), optional | runtime uses its own timeout math |
| 24 | `set_wake_callback` | wake (P124.B), optional | executor uses condvar/poll fallback |
| 25–27 | `pub_loan`, `pub_commit`, `pub_discard` | zero-copy loan (P124.A), optional | runtime stages into an arena buffer |
| 28–29 | `sub_borrow`, `sub_release` | zero-copy borrow (P124.A), optional | runtime copies via `try_recv_raw` |
| 30 | `service_server_available` | probe (P124.C), optional | surfaces `RET_UNSUPPORTED` |
| 31 | `try_recv_sequence` | burst-take (P124.D), optional | runtime loops `try_recv_raw` |
| 32 | `publish_streamed` | streamed publish (P124.E), optional | runtime stages then `publish_raw` |
| 33 | `ping_session` | connectivity probe (P124.F), optional | surfaces `RET_UNSUPPORTED` |
| 34–35 | `subscription_supports_in_place`, `process_raw_in_place` | in-place take (P231 / RFC-0038), optional | runtime uses the buffered (copying) path |

**NULL-slot contract (normative):** every optional slot is in exactly one of two
classes — **fallback** (the runtime emits a correct, possibly slower
implementation when NULL) or **unsupported-surfacing** (NULL makes the runtime
return `NROS_RMW_RET_UNSUPPORTED` to the caller). The table column above is the
contract; a new optional slot MUST declare its class in the header doc-comment.

### Return codes

The negative `nros_rmw_ret_t` space (`rmw_ret.h`) is part of the ABI: `OK=0`,
`ERROR=-1` … `CONNECTION_FAILED=-18`. New codes append at the tail (next: `-19`).
`NROS_RMW_RET_INCOMPATIBLE_ABI=-14` becomes live (see versioning).

### Registration ABI

Backends register via `nros_rmw_cffi_register_named(name, vtable)`
(`rmw_vtable.h:389`); `nros_rmw_cffi_register(vtable)` is the deprecated
single-arg form. Static backends use the `nros_rmw_register_backend!` macro
(`linkme` slice in the `.nros_rmw_init` section), walked by
`nros_rmw_cffi_walk_init_section()` from `Executor::open`. Lookup:
`nros_rmw_cffi_lookup(name)`. Registry is a fixed `NROS_RMW_MAX_BACKENDS` array,
no heap.

### Versioning (the change this RFC mandates)

Add, at **struct offset 0**, mirroring `NrosTransportOps`:

```c
uint32_t abi_version;   /* MUST equal NROS_RMW_VTABLE_ABI_VERSION_V1 */
uint32_t _reserved;     /* zero; future flags */
```

`#define NROS_RMW_VTABLE_ABI_VERSION_V1 ((uint32_t)1)`. Registration
(`register_named`) returns `NROS_RMW_RET_INCOMPATIBLE_ABI` when `abi_version`
mismatches. Because this prepends fields, it is itself a one-time breaking
change — landed in a single commit across the runtime + all three backends, then
the layout is frozen.

### Evolution rule (normative)

1. **Append only.** New slots go at the tail; never reorder or repurpose.
2. New slots are **optional** with a declared NULL class, OR the addition bumps
   `NROS_RMW_VTABLE_ABI_VERSION_V*` and updates every backend in the same commit.
3. Removing/changing a slot signature is a major bump — disallowed without an
   RFC superseding this one.

## Alternatives considered

- **Leave it convention-only** (status quo). Rejected — silent UB on skew; the
  transport vtable already proved the `abi_version` pattern is cheap and worth it.
- **Promote RFC-0006 instead of a new RFC.** RFC-0006 is a broad design-review
  note across RMW *and* platform interfaces; the enumerated RMW ABI freeze
  deserves its own stable, citable contract. RFC-0035 references 0006 for
  rationale.
- **Trailing version field.** Rejected — offset-0 matches `NrosTransportOps` and
  lets registration validate before reading any fn pointer.

## Open questions

1. Should `_reserved` carry a capability bitmask (which optional slots are
   populated) to let the runtime skip per-slot NULL checks? Proposed: defer;
   the NULL checks are cheap.
2. Do the four `*-sys`/DDS backends that bypass the cffi shim (Cyclone via its
   own register path) need the same `abi_version` gate? Proposed: yes, any
   `nros_rmw_cffi_register_named` caller is gated.

## Changelog

- 2026-07 — phase-301 (issues 0240/0241): slot vocabulary aligned with
  upstream rmw (`create_session`/`destroy_session`, `subscription`,
  `service`, `client`), blocking `call_raw` slot deleted
  (`send_request_raw` + `try_recv_reply_raw` now required for services),
  transport hints moved to per-create options structs. Table now 35 slots
  (incl. the P231 in-place pair).
- 2026-06 — created (Draft). Records the 34-slot ABI as canonical; defines the
  append-only rule, NULL-slot contract, and the `abi_version` field to add.
  Grounded in `nros-rmw-abi/include/nros/rmw_vtable.h` + `rmw_ret.h`; rationale
  from RFC-0006.
