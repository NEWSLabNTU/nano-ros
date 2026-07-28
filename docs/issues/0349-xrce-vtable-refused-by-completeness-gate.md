---
id: 349
title: "The issue-0332 vtable completeness gate refuses the xrce backend outright — three OPTIONAL capability slots are on the required list, so nros_rmw_xrce_register() returns INVALID_ARGUMENT"
status: open
type: bug
severity: high
area: core
related: [issue-0332, rfc-0035, rfc-0054]
---

## Finding (2026-07-28, found while fixing issue 0331)

`cargo test -p nros-rmw-xrce-cffi --test register_smoke` fails on a clean
checkout of `main`:

```
test register_resolves_and_returns ... FAILED
register returned unexpected error: RegisterError(-4)
```

`-4` is `NROS_RMW_RET_INVALID_ARGUMENT`. Verified pre-existing by stashing all
local work and re-running against HEAD — it is not a regression from 0330/0331.

## Cause

Issue 0332 added a completeness check to `nros_rmw_cffi_register_named`
(`packages/core/nros-rmw-cffi/src/lib.rs:806`), rejecting a vtable with any
missing slot rather than panicking mid-spin. Sound intent. But
`first_missing_vtable_slot` (`lib.rs:739`) puts three **optional capability**
slots on the required list:

```
register_publisher_event,
register_subscription_event,
assert_publisher_liveliness,
```

The xrce vtable (`packages/xrce/nros-rmw-xrce/src/vtable.c:56-58`) sets all
three to `NULL` — deliberately and explicitly, alongside ~14 other `NULL`
capability slots (`pub_loan`, `sub_borrow`, `next_deadline_ms`,
`service_server_available`, …) that the gate correctly does NOT require.

So the gate does not merely warn: **the xrce backend cannot register at all.**

## Why this is severity: high

The three slots are QoS-event and liveliness *capabilities*, not core message
transport. RFC-0035's model is that a backend advertises what it supports and
the runtime degrades or reports `INCOMPATIBLE_QOS` — not that a backend without
liveliness is structurally invalid. A backend that can publish, subscribe,
serve and call is a working backend.

Note the vtable slots are `Option<fn>` precisely *because* C nullability
encodes "not provided" (RFC-0054). Requiring a slot whose type says it is
optional is the contradiction here.

## What is NOT yet known

- Whether this breaks xrce end-to-end in practice or only the register path.
  The embedded xrce fixtures may reach registration through a path that ignores
  the return code — worth checking, because "the return is dropped" would be a
  second defect, not a mitigation.
- Whether cyclonedds and the metadata backend fill all three (they register
  successfully today, so presumably yes, or they never hit this path).

## Fix

Split the slot list in two:

1. **Required** — the ~17 slots without which the runtime cannot function
   (`create_session`, `publish_raw`, `try_recv_raw`, `drive_io`, the create /
   destroy pairs, `send_reply`, `has_request`, `try_recv_request`).
2. **Optional capability** — `register_publisher_event`,
   `register_subscription_event`, `assert_publisher_liveliness`, and the other
   already-optional slots.

Then confirm every call site of the three guards on `is_none()` before
dispatch, rather than assuming registration validated them — that assumption is
what makes moving them safe or unsafe, and it must be checked, not presumed.

## Acceptance

- `cargo test -p nros-rmw-xrce-cffi --test register_smoke` passes.
- A regression test asserts that a vtable with the three capability slots NULL
  registers successfully, AND that one missing a genuinely required slot is
  still refused — mutation-checked in both directions, so the gate is not
  simply weakened into uselessness.
- Every dispatch site of the three slots is confirmed to handle `None`.
