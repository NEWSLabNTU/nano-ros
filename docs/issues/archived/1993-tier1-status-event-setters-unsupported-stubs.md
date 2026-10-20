---
id: 1993
title: "The five Tier-1 C/C++ status-event setters (liveliness, deadline, message-lost) are UNSUPPORTED stubs on every backend, though Cyclone, zenoh and Rust all carry the events"
status: resolved
type: bug
severity: medium
area: [api, rmw]
related: [1972, 1978, 1164, phase-108]
found: 2026-10-20
resolved_in: 2026-10-21
---

## What happens

Every Tier-1 status-event setter in the C and C++ APIs returns
`UNSUPPORTED` unconditionally. The arguments are ignored: nothing is
stored, no backend is asked. This happens on every backend, including the
two that generate all five events.

### C (`packages/api/nros-c/src/event.rs`, on `origin/main`)

| Setter | Line | Returns |
|---|---|---|
| `rclc_subscription_set_liveliness_changed_callback` | 83 | `RCL_RET_UNSUPPORTED` (`:90`) |
| `rclc_subscription_set_requested_deadline_missed_callback` | 103 | `RCL_RET_UNSUPPORTED` (`:109`) |
| `rclc_subscription_set_message_lost_callback` | 118 | `RCL_RET_UNSUPPORTED` (`:123`) |
| `rclc_publisher_set_liveliness_lost_callback` | 147 | `RCL_RET_UNSUPPORTED` (`:152`) |
| `rclc_publisher_set_offered_deadline_missed_callback` | 163 | `RCL_RET_UNSUPPORTED` (`:169`) |

Every parameter is `_`-prefixed. The doc comment says "Returns
`NROS_RMW_RET_UNSUPPORTED` until the active backend wires up
liveliness-event detection (Phase 109+) … the event fires once the backend
gains support". Both backend halves have since shipped (see below), and
nothing would make this event fire.

### C++ (`packages/api/nros-cpp`, on `origin/main`)

| Wrapper | FFI entry point | Returns |
|---|---|---|
| `PollingSubscription::on_liveliness_changed` (`include/rclcpp/polling_subscription.hpp:433`) | `nros_cpp_subscription_set_liveliness_changed` (`src/subscription.rs:1195`) | `NROS_CPP_RET_UNSUPPORTED` (`:1200`) |
| `PollingSubscription::on_requested_deadline_missed` (`:440`) | `nros_cpp_subscription_set_requested_deadline_missed` (`:1204`) | `NROS_CPP_RET_UNSUPPORTED` (`:1210`) |
| `PollingSubscription::on_message_lost` (`:448`) | `nros_cpp_subscription_set_message_lost` (`:1214`) | `NROS_CPP_RET_UNSUPPORTED` (`:1219`) |
| `Publisher::on_liveliness_lost` (`include/rclcpp/publisher.hpp:460`) | `nros_cpp_publisher_set_liveliness_lost` (`src/publisher.rs:476`) | `NROS_CPP_RET_UNSUPPORTED` (`:481`) |
| `Publisher::on_offered_deadline_missed` (`:467`) | `nros_cpp_publisher_set_offered_deadline_missed` (`:485`) | `NROS_CPP_RET_UNSUPPORTED` (`:491`) |

The section headers read `// Phase 108 — … status events (stub: …
NROS_CPP_RET_UNSUPPORTED)` (`src/publisher.rs:457`, `src/subscription.rs:1154`).
`include/rclcpp/event.hpp` says the same: "`ErrorCode::Unsupported` until
backend wiring lands per-phase (109+)".

The line numbers are `origin/main` at `e8400d5a92`. PR #2451 (issue 1972,
branch `feat/1972-matched-events`) moves them by about 8 lines and does not
touch the bodies. Its own module doc in `event.rs` says so: "The five Tier-1
setters … still return `RCL_RET_UNSUPPORTED` at this layer".

### Below the API, every layer carries all five

- **Rust API, wired.** `nros-node`'s `EmbeddedPublisher::on_liveliness_lost` /
  `on_offered_deadline_missed` (`packages/core/nros-node/src/executor/handles.rs:353`,
  `:374`) and `Subscription::on_liveliness_changed` /
  `on_requested_deadline_missed` / `on_message_lost` (`:1453`, `:1463`,
  `:1488`) call `register_pub_event` / `register_sub_event` with the real
  `EventKind`. `packages/testing/nros-tests/bins/qos-event-probe` calls
  them.
- **cffi seam, wired.** `PUB_EVENT_KINDS` / `SUB_EVENT_KINDS` list all five
  (`packages/rmw/cffi/src/lib.rs`, around line 709). The single
  `backend_event_type_is_supported` helper decides support from them.
- **Cyclone DDS, wired.** `cyclone_event_type_is_supported` answers `true`
  for all five (`packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/vtable.cpp`).
  `publisher_take_event` / `subscription_take_event` serve them (issue 1164).
- **zenoh-pico, wired.** `shim::events::{publisher,subscription}_event_supported`
  (`packages/rmw/zenoh/nros-rmw-zenoh/src/shim/events.rs`) lists
  `OfferedDeadlineMissed`, `LivelinessLost`, `MessageLost`,
  `RequestedDeadlineMissed`, and `LivelinessChanged` off bare metal.
- XRCE-DDS and uORB carry none of them. For those two, `UNSUPPORTED` is the
  correct answer.

So only the C and C++ layer is missing. The shared backend path is
complete, and Rust users already get the events.

No C or C++ test or example calls any of the ten entry points
(`git grep` over `examples/`, `packages/testing/`, `packages/api/*/tests/`).
That is why nothing noticed.

## Why it matters

- **The setter refuses quietly.** It returns `UNSUPPORTED`, which is also
  the documented "this backend can't" answer. A C/C++ app on Cyclone or
  zenoh reads it as a backend limitation and drops its liveliness
  failover or deadline alarm, when the backend could have delivered.
  RFC-0089 Part I covers this shape: an answer that is wrong but looks
  legitimate.
- **The API parity ledger overclaims.** In `docs/reference/api-parity-ledger/pubsub.json`,
  the rows `c:publisher_set_liveliness_lost_callback`,
  `c:publisher_set_offered_deadline_missed_callback`,
  `c:subscription_set_liveliness_changed_callback`,
  `c:subscription_set_message_lost_callback`,
  `c:subscription_set_requested_deadline_missed_callback`,
  `c:event_publisher_count_cb_t`, `c:subscription_info_callback_t`, and the
  `cpp:Publisher::*` event rows that `provide` `nros::Publisher::on_liveliness_lost`
  / `on_offered_deadline_missed` all carry the same `divergence` text. It
  says the callbacks are "registered on the entity … the callback is a
  fn-ptr in the entity's own static storage. The set of events is the
  same." Nothing is registered and nothing is stored. The verdict
  (`divergence`, named per-event callbacks vs. rclcpp's
  `QOSEventHandler`) is right about the SHAPE once this is fixed. The
  "registered / stored" sentences are false today. This PR leaves those
  rows alone. They all carry the same reason text, so edit them together
  when the fix lands, and re-run `just check api-parity`.
- **The book overclaimed, now corrected in this PR.** The C and C++
  sections of `book/src/concepts/status-events.md` said "Each returns
  `NROS_RMW_RET_UNSUPPORTED` if the active backend doesn't generate that
  event". That was wrong twice over: the return is `RCL_RET_UNSUPPORTED`,
  and it comes back regardless of backend. The C++ section also showed
  `sub.on_liveliness_changed([&](…){…})` with
  `std::chrono::milliseconds`, and claimed "`std::function` overloads
  available with `NROS_CPP_STD`". No such overloads exist; the only
  overloads take a C function pointer plus `user_context`. The section
  now shows the real shape and points here.
- `book/src/reference/rmw-feature-matrix.md` row "Status events (deadline /
  liveliness / lost)" reads `wired` for Cyclone and zenoh. That is TRUE at
  the backend level the page describes, so it needs no change.

## Fix sketch

Use one mechanism for C and C++, the one PR #2451 added for the four
issue-1972 setters: `nros_c::status_event_slots`, the static table of
`(owner, kind, generation, entity, cb, ctx)` that the backend reaches
through a generation-tagged pointer. The C++ half does not need a separate
fix. Its matched setters already call `nros_c::status_event_slots::register`
/ `release_owner` / `rekey`. The five Tier-1 setters become the same
`publisher_set` / `subscription_set` (C) and `publisher_set_slot` /
`subscription_set_slot` (C++) calls with a different `EventKind`. Four
things that path does not do yet:

1. **Pass the deadline through.** Both helpers hand the backend
   `register_event_callback(handle, kind, 0, tramp, tag)`, a hardcoded
   `0`. `*_deadline_missed` takes a `deadline_ms`. Add a `deadline_ms`
   parameter to the helpers, `0` for the non-deadline kinds, as
   `nros-node` does. Cyclone takes the deadline from the QoS profile and
   ignores the argument (the book says so). zenoh's clock check uses it.
2. **Handle the two payload shapes in the trampoline.** Today its
   `match` dispatches only the matched and incompatible-type payloads and
   ends in `_ => {}`, so a Tier-1 event would be dropped without a trace.
   Add arms for `EventPayload::LivelinessChanged(LivelinessChangedStatus)`
   and the four `CountStatus` kinds. Const-assert, next to each,
   that `rclc_liveliness_changed_status_t` / `rclc_count_status_t` and
   `nros_cpp_liveliness_changed_status_t` / `nros_cpp_count_status_t` /
   `nros_cpp_pub_count_status_t` are layout-identical to the `nros_rmw`
   struct, the same way the matched structs are.
3. **Size the table for nine kinds.** `MAX_SLOTS = 16`, chosen for "four
   event kinds per entity". With all nine, one publisher can hold 4 slots
   and one subscription 5. Either size it from a knob in the registry
   (`packages/tooling/nros-zephyr-build/knobs.toml`, floored at the pool
   per the `check-c-array-pool-floors` rule) or state the bound in the
   doc comment of every setter that can return `RCLC_RET_FULL`. Do not
   enlarge the constant by hand without a reason. Each slot costs
   `6 x usize`, and the table is process-wide.
4. **Keep `NOT_INIT` for an unattached subscription.** As with the
   matched pair, a C subscription not yet added to an executor has no
   subscriber to ask, so the setter returns `RCL_RET_NOT_INIT`. Document
   that on the three subscription setters.

Then update the stale doc comments (`event.rs` module doc and the five
setter docs, `event.hpp`, the two `// Phase 108 … (stub …)` headers, the
`polling_subscription.hpp` "Returns `Unsupported` until …" note), the
ledger rows listed above, and the "Not wired yet (issue 1993)" note this
PR adds to `book/src/concepts/status-events.md`.

## Acceptance

- [x] All ten entry points (5 C, 5 C++) register through
      `status_event_slots`. On a backend whose `supports_event(kind)` is
      true they return `OK`. On XRCE/uORB they return `UNSUPPORTED`,
      decided by the backend and never by a stub.
- [x] `deadline_ms` reaches `register_event_callback`. A test asserts
      that the zenoh backend sees the value the caller passed.
- [x] The trampoline dispatches `LivelinessChanged` and the four
      `CountStatus` kinds, with layout const-asserts for every C/C++
      payload struct.
- [x] The slot-table bound is either derived from a knob or documented
      per setter, and one publisher plus one subscription with ALL their
      kinds registered fit in the default.
- [x] Each setter has a test: a C test and a C++ test per event kind,
      against a stub vtable as `packages/rmw/cffi/tests/` uses, that
      fires the event and observes the user callback with the right
      entity, status and context. Plus a test that a late fire after
      `*_fini` is a no-op (the generation check).
- [x] No `RCL_RET_UNSUPPORTED` / `NROS_CPP_RET_UNSUPPORTED` stub left in
      `event.rs` / `nros-cpp/src/{publisher,subscription}.rs`. The ledger
      `why` text and the book no longer say "stub" or "registered … stored"
      where that is false. `just check api-parity` is green.

## Resolution

**One mechanism, C and C++.** All ten entry points now go through
`nros_c::status_event_slots`, the table issue 1972 added: the C setters via
`publisher_set` / `subscription_set` (`packages/api/nros-c/src/event.rs`), the
C++ FFI via `publisher_set_slot` / `subscription_set_slot`
(`packages/api/nros-cpp/src/{publisher,subscription}.rs`). No
`RCL_RET_UNSUPPORTED` / `NROS_CPP_RET_UNSUPPORTED` stub is left.

1. **Deadline.** The four helpers take `deadline_ms` and hand it to
   `register_event_callback`; `0` for every kind but the two deadline ones.
2. **Support is the backend's answer.** Each helper asks the entity's
   `supports_event(kind)` first (the cffi route answers through
   `backend_event_type_is_supported`), then the backend's registration may
   still refuse. XRCE-DDS and uORB therefore answer `UNSUPPORTED`; Cyclone
   DDS and zenoh answer `OK` (zenoh `LivelinessChanged` is refused on bare
   metal, by the shim).
3. **Trampoline.** The `_ => {}` arm is gone: the match over `EventPayload`
   is exhaustive, with an arm for `LivelinessChanged` and one for the four
   `CountStatus` kinds. A payload whose kind differs from the slot's
   registered kind is now refused (the callback's shape is chosen by the
   registered kind). Size, alignment and per-field offset const-asserts sit
   beside `rclc_liveliness_changed_status_t`, `rclc_count_status_t`,
   `nros_cpp_liveliness_changed_status_t`, `nros_cpp_count_status_t` and
   `nros_cpp_pub_count_status_t`.
4. **Bound documented, not a knob.** `MAX_SLOTS` stays 16, now `pub`, with
   `const` asserts that one publisher (4 kinds) plus one subscription (5)
   fit and that the index fits the tag's 8 bits. The bound is stated on the
   module and on every setter: `RCLC_RET_FULL` / `NROS_CPP_RET_FULL` /
   `ErrorCode::Full` on exhaustion.
5. **`NOT_INIT`.** The three C subscription setters document that an
   unattached subscription (not yet added to an executor) answers
   `RCL_RET_NOT_INIT`.

**A leak closed on the way.** `rclc_executor_remove_subscription` dropped the
executor-owned subscriber the slots are keyed on without releasing them, and
`rcl_subscription_fini` could no longer find the handle afterwards: each
remove/re-add cycle leaked registrations from the process-wide table (already
true for the 1972 kinds). It now calls `release_owner` before the release.

**Tests.**

- `packages/api/nros-c/tests/run/status_event_setters.c` (`just check c`):
  against the stub backend, which now fills `publisher_event_init`,
  `subscription_event_init` and `event_type_is_supported`, every C setter
  returns `OK`, the stub sees 250 / 500 ms for the deadline kinds and 0 for
  the rest, a fire reaches the user callback with the right entity, status
  and context, an unattached subscription is `NOT_INIT`, a backend denying
  events makes all five `UNSUPPORTED`, a remove releases the slots, and a
  late fire after `rcl_*_fini` is a no-op.
- `packages/api/nros-cpp/tests/compile/status_event_setters_runtime.cpp`
  (`just check cpp`): the same for the five C++ entry points, plus a
  `nros_cpp_publisher_relocate` that moves the callback's storage argument.
- `status_event_slots` unit test: the Tier-1 shapes dispatch, a mismatched
  kind is refused, and all nine kinds of one publisher plus one subscription
  fit.
- `packages/rmw/zenoh/nros-rmw-zenoh/tests/status_events_matrix.rs` (live,
  needs a router): with no deadline in the QoS, a publisher registered at
  100 ms misses after a 300 ms silence and one registered at 10 s does not,
  so zenoh uses the value the registration carries.

**Ledgers and book.** The shared "the callback is a fn-ptr in the entity's own
static storage" sentence in `pubsub.json` (18 rows) and `qos.json` (10 rows)
now says where the triple lives and which backends carry the events; the
matched rows' "unlike the five Tier-1 setters" now say "like". The "Not wired
yet" note in `book/src/concepts/status-events.md` became the returns,
deadline and lifetime contract.
