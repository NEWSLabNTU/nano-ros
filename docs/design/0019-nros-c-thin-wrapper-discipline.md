---
rfc: 0019
title: "nros-c Thin-Wrapper Discipline (Phase 122.3.a)"
status: Stable
area: runtime-api
since: 2026-05
last-reviewed: 2026-11-14
implements-tracked-by: [phase-83, phase-122, phase-270, phase-317]
supersedes: [0020]
superseded-by: null
---

# nros-c Thin-Wrapper Discipline (Phase 122.3.a)

**Status:** Stable. The rule is in force; the opaque-only struct target of
2026-05-13 was not pursued (ruled 2026-11-14, below).
**Date:** 2026-05-13; ruled and merged with RFC-0020 2026-11-14 (phase-555 W3).
**Owner:** Phase 122 (Unify C / C++ / Rust API code paths), then phase-83's
audit method, now the periodic audit's category C1.

> **Ruling 2026-11-14 (phase-555 W3).** The principle stands: the Rust API is
> the source of truth, and C and C++ delegate, with no logic or state machine
> in the wrapper. RFC-0089 and RFC-0105 cite it as governing. The target this
> RFC first set, every public C struct holding only `state` and `_opaque`, is
> dropped: it was never built
> ([phase 122](../roadmap/archived/phase-122-unify-api-paths.md) left
> 122.3.b unchecked), later phases added fields to these structs
> (`type_support` by phase-492 W8, `options` on `rcl_publisher_t` by
> phase-515), and users reach the structs through rclc's functions, not their
> fields (RFC-0089). Mirrored registration fields are accepted; see "The
> rule". The C names moved to rclc's (RFC-0089), so `nros_<entity>_t` in the
> 2026-05-13 audit below is now `rcl_<entity>_t`.
>
> [RFC-0020](archived/0020-thin-wrapper-audit.md), the phase-83 audit and its
> checklist, is merged here: the checklist is "Compliance checklist" below,
> and the audit method is category C1 of
> [codebase-audit-checklist.md](../development/codebase-audit-checklist.md).

> **Build/link contract.** The C ABI nros-c wraps resolves through one canonical
> `<nros/platform.h>` (`nros-platform-api`); board capabilities + the RMW backend
> link are config/manifest-driven — see
> [RFC-0042](0042-platform-build-determinism.md) +
> [c-api-cmake.md](../reference/c-api-cmake.md) "Board capabilities & deterministic
> build".

## Why this exists

Phase 122 establishes the rule: **the Rust API is the source of truth.
C / C++ wrappers are thin shims that delegate.** On 2026-05-13 nros-c
followed this unevenly — some entities are thin wrappers (publisher,
executor, support), others duplicate Rust-side bookkeeping in
C-visible struct fields (subscription, service, action, timer,
guard).

This doc recorded that audit and first set a per-entity refactor target
(an opaque-only struct). The target is withdrawn; "The rule" below is what
stands.

## Audit (2026-05-13, kept as history)

For each entity, three categories:

- **Opaque-thin (✅).** Public C struct holds the Rust entity inline
  via `_opaque: [u64; <ENTITY>_OPAQUE_U64S]` (size probed by
  `nros-sizes-build`). All ops cast `_opaque` to the Rust type +
  delegate. The C struct IS the Rust value.

- **Field-mirror (❌).** Public C struct exposes parallel storage for
  what Rust already tracks: `topic_name: [u8; N]`, `type_name`,
  `callback`, `context`, `qos`, `handle_id`, etc. C-side validation
  duplicates Rust-side validation. The real entity lives elsewhere
  (executor arena); C struct is a "registration packet".

- **Mixed (⚠️).** Combines `_opaque` for the Rust entity with
  field-mirror metadata.

| Entity | Pattern | C struct fields beyond `state` + `_opaque` |
|---|---|---|
| `nros_publisher_t` | ✅ opaque-thin | topic_name + type_name + type_hash + node + `_opaque` |
| `nros_support_t` (RmwSession) | ✅ opaque-thin | `_opaque` only |
| `nros_executor_t` | ✅ opaque-thin | `_opaque` only |
| `nros_node_t` | ✅ opaque-thin-ish | name + namespace + support* |
| `nros_subscription_t` | ❌ field-mirror | topic_name + type_name + type_hash + callback + context + node + qos + handle_id |
| `nros_service_t` (server) | ❌ field-mirror | service_name + type_name + type_hash + callback + context + node + `_internal` |
| `nros_client_t` | ❌ field-mirror | service_name + type_name + type_hash + response_callback + context + node + `_internal` |
| `nros_action_server_t` | ❌ field-mirror | action_name + type_name + type_hash + goal_callback + cancel_callback + accepted_callback + context + node + `_internal` |
| `nros_action_client_t` | ❌ field-mirror | action_name + type_name + type_hash + 3 callbacks + context + node + `_internal` |
| `nros_timer_t` | ❌ field-mirror | period_ns + last_call_time_ns + callback + context + `_internal` |
| `nros_guard_condition_t` | ⚠️ mixed | callback + context + `_guard_opaque` for `GuardCondition` |

Score: 4 of 11 entities follow the discipline.

## Why the split exists historically

The pattern divergence reflects when each entity was added:

- **Opaque-thin entities** (publisher, support, executor) are
  *self-contained Rust values*. The C struct allocates inline
  storage; the Rust value lives there for its lifetime.
- **Field-mirror entities** (subscription, service, action, timer,
  guard) are *executor-registered handlers*. The actual entity (the
  Rust `RawSubscription` / `SrvEntry` / etc.) lives in the
  executor's arena. The C struct only carries the *registration
  packet* — topic name, type info, callback, etc. — consumed at
  `nros_executor_register_*` time.

The split made sense when only the L2 (callback) path existed —
field-mirror was the cheapest way to ship a callback registration
through the C ABI. Phase 122 introduces L1 (primitive polling),
which puts the entity inside the C struct. Field-mirror doesn't
extend to L1 cleanly.

## The rule

1. **The wrapper holds no logic and no state machine.** Goal lifecycle,
   request and reply tracking, pending tables, counters and anything else
   the Rust side tracks and changes after registration live in Rust. The C
   struct never mirrors them.

2. **Mirrored registration fields are accepted.** A C struct may carry the
   registration packet as fields: name, type name, type hash, QoS, callback,
   context, the type support and the arena handle id. They are written once,
   at init or registration, and read by the wrapper; nothing in C changes
   them afterwards. This is what `rcl_subscription_t` and its siblings hold
   today (`packages/api/nros-c/src/subscription.rs`). The 2026-05-13 target
   that every struct hold only `state` + `_opaque` is not pursued: users
   reach the structs through functions, so a layout change costs a rebuild
   (see "ABI compatibility"), which is the only risk the target guarded
   against.

3. **C entry points do boundary conversion + delegate.**
   - `*const c_char` to `&str` via `CStr::from_ptr` + `to_str`.
   - Null-pointer checks at the boundary.
   - Result mapping (`Result<T, NodeError>` to `rcl_ret_t`).
   - No business logic in C.

4. **No duplicated validation.** What Rust validates (topic names, QoS,
   bounds) is validated in Rust; the wrapper trusts it. Checking the C
   struct's own `state` at the boundary (`validate_state!`) is boundary
   validation, not duplication.

5. **Getters read the registration field or delegate.** A getter may return
   a mirrored registration field; anything that changes after registration
   is read from the Rust side.

## Compliance checklist

Merged from RFC-0020 (phase-83). A `nros-c` or `nros-cpp` change is
thin-wrapper-compliant if **none** of the following appear in the added
code. Apply it as a review gate; the periodic re-run is the audit's
category C1.

- [ ] No `[u8; N]` arrays or `heapless::Vec` fields in the C/C++ FFI
      struct that mirror data the arena already owns (`active_goals`,
      `pending_*`, `completed_results`, etc.). Reserve such fields for the
      registration packet (rule 2) or inline storage of the RMW handle,
      never lifecycle state.
- [ ] No counters that duplicate arena accessors (e.g.
      `active_goal_count`, `pending_count`). Always forward the getter
      to an arena call like `handle.active_goal_count(executor)`.
- [ ] Callbacks are trampolines that translate ABI + call through to the
      user's callback. No FSM transitions inside trampolines; no slot
      reservation beyond what the arena requires.
- [ ] Lifecycle ops (`succeed` / `abort` / `cancel` / `complete` /
      `publish_feedback`) contain zero C-side state mutation. They
      extract the goal identity, look up the arena handle, and delegate.
- [ ] No `static mut BLOCKING_*` flags and no `zpico_get`-style condvar
      waits. Blocking convenience wrappers take a user-supplied executor
      and drive it (RFC-0021).
- [ ] No magic bytes / offsets for CDR framing. Import from `nros::cdr`
      (or `nros_serdes::...` inside `nros-node` internals).
- [ ] No key-expression / topic-string construction. Use
      `TopicInfo` / `ServiceInfo` / `ActionInfo` builders from
      `nros-rmw`.
- [ ] No direct calls to `zpico_*` / `uxr_*` / other transport SDKs.
      Go through `Session` / `Publisher` / `Subscriber` /
      `ActionServerRawHandle`.

If a change needs to break any of these, its description justifies the
exception with a link to the governing phase doc. Otherwise the logic moves
into `nros-node` first.

The phase-83 run found five violations across both crates (a pending-goal
array and auto-accept trampoline in the C++ action server, goal arrays and
a goal counter mirrored in the C action server, scattered CDR header bytes,
and `static mut BLOCKING_ACCEPTED` flags in both action clients); all five
are resolved. The findings table, with the commits that closed each, is in
[archived RFC-0020](archived/0020-thin-wrapper-audit.md).

## ABI compatibility

A C struct's layout is a build-time fact: growing the registration packet
changes struct sizes and field offsets, and consumers recompile. There is
no silent breakage, because a C consumer that reaches a struct through the
functions does not depend on its layout, and the compiler catches a
mismatch at build time. Until a first stable release, breaking changes are
welcome (phase-115's RMW backend rename established this).
