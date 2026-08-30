---
id: 349
title: "The issue-0332 vtable completeness gate refuses the xrce backend outright — three OPTIONAL capability slots are on the required list, so nros_rmw_xrce_register() returns INVALID_ARGUMENT"
status: resolved
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
(`packages/rmw/cffi/src/lib.rs:806`), rejecting a vtable with any
missing slot rather than panicking mid-spin. Sound intent. But
`first_missing_vtable_slot` (`lib.rs:739`) puts three **optional capability**
slots on the required list:

```
register_publisher_event,
register_subscription_event,
assert_publisher_liveliness,
```

The xrce vtable (`packages/rmw/xrce/nros-rmw-xrce/src/vtable.c:56-58`) sets all
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

## Resolution (2026-07-29)

The three slots are off the required list, and the change that MAKES that safe
is at the point of use: `register_publisher_event`,
`register_subscription_event` and `assert_publisher_liveliness` no longer
`.expect()` their slot — they return `TransportError::Unsupported` when the
backend did not provide one. That is the difference between an optional slot and
a missing required one, and it is the refinement `first_missing_vtable_slot`'s
own doc had deferred.

`assert_liveliness`' dispatch site had documented the intended contract all
along — *"NULL function pointer = backend doesn't support manual liveliness"* —
directly above an `.expect()` that panicked on exactly that. The code
contradicted its own comment.

### The sweep

Every remaining `.expect("rmw vtable: …")` in `nros-rmw-cffi` was enumerated:
18 sites, corresponding 1:1 to the 17 required slots. No other optional slot is
dispatched through an `.expect()`, so this was the whole class.

Sweep command:

```
rg -n 'expect\("rmw vtable' packages/rmw/cffi/src/lib.rs
```

### The gate

That 1:1 correspondence is the real invariant, and it had already broken in both
directions across two issues, so it is now enforced:
`scripts/check-rmw-required-slots.sh` (`just check rmw-required-slots`, wired
into `check-fast`) extracts both sets and fails if they differ —

- expect-ed but not required → registers cleanly, panics mid-spin (issue 0332);
- required but not expect-ed → refuses working backends (this issue).

Mutation-tested in both directions.

### Coverage

- `nros-rmw-xrce-cffi::register_smoke` passes — the end-to-end proof, since xrce
  is the backend that was being refused.
- `register_accepts_vtable_without_optional_capability_slots` asserts the gate
  does not over-bite; `register_still_rejects_a_missing_required_slot` and the
  pre-existing `register_rejects_incomplete_vtable` assert it still bites.
  Keeping both directions is deliberate — dropping either turns the gate into a
  one-way ratchet.
- The "accepts" test asserts on `first_missing_vtable_slot` rather than calling
  `nros_rmw_cffi_register_named`: the registry is a process global with no
  removal, so a successful registration leaks a second backend into every other
  test in the binary and turns single-backend resolution `Ambiguous`
  (`typed_struct_roundtrip` went red). Noted in the test.

### Not done

Whether anything upstream treats `Unsupported` from these three as fatal was not
exercised end to end — the runtime gates liveliness by `liveliness_kind` before
calling, so the path should be unreachable for a backend that never advertises
it, but that is reasoning, not a test.

