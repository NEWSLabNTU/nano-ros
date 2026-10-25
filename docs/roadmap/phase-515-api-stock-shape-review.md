# Phase 515 — Re-audit the recorded API differences against stock ROS 2 shape

**Status (2026-10-25). Open; W0.a landed (#2627), D1–D4 decided, C1 landed.** Opened from a review of the
`api-parity` report on main `1a30bcbb22`. The work items below are candidates:
each starts by confirming its reason against the code, because W0 found that
the ledger's prose has drifted. The four decisions (D1–D4) are taken;
see Decisions.

**Prior:** [phase-379](archived/phase-379-api-parity-with-ros2-client-libraries.md)
(the measurement and the ledger),
[phase-417](archived/phase-417-ros2-api-adoption.md) (RFC-0089 adoption),
[phase-482](archived/phase-482-rclcpp-drop-in-residue.md),
[phase-483](archived/phase-483-rclcpp-rclrs-shape-only.md) (`rclcpp::` and
rclrs shape only), [phase-492](archived/phase-492-c-api-rcl-names.md) (C on rcl
names, descriptors gain a codec),
[phase-506](archived/phase-506-api-crates-carry-only-the-api.md).
**Implements:** [RFC-0089](../design/0089-ros2-api-adoption-and-the-compile-or-conform-rule.md):
our constraints first, then upstream's name and shape, invention only where
upstream has nothing.

---

## Why a phase

The ledger gate is green: every item that differs from rclc, rclcpp or rclrs
carries a verdict and a reason. That proves every difference is *explained*;
it does not prove the explanation still holds. RFC-0089 allows a difference
only when an RTOS constraint forces it. Reading the reasons on 2026-10-24
turned up three kinds that do not meet that bar:

* a **preference** recorded as a reason ("an overload trait over one caller
  shape", "one constructor resolves it with no types");
* a **limitation of our own code** that has since been fixed or could be;
* **stale prose** that describes code which no longer exists.

The report as measured (Humble; Jazzy is within a few rows; rclrs 0.7.0):

| surface | same | same name, other shape | theirs only | ours only | open `gap` | open `rename` |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| C · rclc + rcl | 143 | 13 | 328 | 461 | 0 | 5 |
| C++ · rclcpp | 205 | 45 | 577 | 442 | 11 | 5 |
| Rust · rclrs | 109 | 29 | 467 | 1508 | 1 | 33 |

Most theirs-only rows have a reason that holds: wait sets (the executor drives
entities from a fixed arena, RFC-0002), heap-built objects, the async runtime
and rclrs workers, runtime-typed messages, argv/env/YAML configuration,
content filters and typed loans. This phase does not reopen those. It works
the rows below.

The browsable view is the three pages `scripts/gen-api-comparison.py` renders
(one per language, every row with its reason); regenerate after each item.

## Four open API issues get a home here

Each was left without a phase when its finding phase closed.

| issue | severity | item |
| --- | --- | --- |
| [2030](../issues/archived/2030-c-callback-tier-cancel-reply-slot.md) — the C callback tier never takes a cancel reply (resolved 2026-10-24) | medium | C7 |
| [2024](../issues/2024-parameter-event-handler.md) — no `ParameterEventHandler` | low | P6 |
| [1942](../issues/1942-rclrs-service-info-no-counterpart.md) — no `ServiceInfo` for a service callback | low | R7 |
| [2029](../issues/archived/2029-action-server-cancel-all-before.md) — cancel-all / cancel-before answer `UNKNOWN_GOAL_ID` (resolved 2026-10-24) | low | X1 |

## Decisions

Taken 2026-10-25 (maintainer). Items that depend on one say so.

* **D1 — hosted-only stock items: NO. Freestanding only.** Every stock-shaped
  item must work on every target; nothing is added behind a `<chrono>`,
  `<string>`/`<vector>` or Rust `std` probe. Std-shaped items stay `declined`
  and a hosted port edits those call sites. Effect: P3 shrinks to the bounded
  `rclcpp::Parameter` / `ParameterValue` record (works freestanding); its
  vector-taking `get_parameters` / `set_parameters` forms stay declined. P5
  loses `Duration::to_chrono` (declined); `Clock::sleep_for` / `sleep_until`
  take rclcpp's own `Duration` / `Time`, not `<chrono>` types, so they remain
  candidates under RFC-0021. The `make_shared` family stays declined. The
  existing `Rate(std::chrono::duration)` overload (phase-442 W5, behind
  `NROS_CPP_HAS_STD_CHRONO`) predates this ruling; whether it stays is
  phase-442 W8's call, not this phase's.
* **D2 — C timer callback: rclc's contract ONLY, and migrate.** The timer
  callback becomes rclc's `rcl_timer_callback_t`, `(rcl_timer_t *, int64_t)`,
  registered with `rclc_timer_init_default`; `rclc_timer_init_with_context`
  is removed and its 34 in-tree call sites move. Measured before deciding: of
  the 34, 23 pass `self` and embed the timer in that same struct
  (`&self->timer`), 8 pass a sibling field of the struct holding the timer
  (`&app.talker_ctx` beside `&app.timer`), 3 pass NULL, and the executor calls back with the caller's own `rcl_timer_t *`
  (`executor.rs`, `c_callback(timer_ptr, c_context)`), never a copy. So every
  in-tree context is recoverable from the timer pointer by the embedded-struct
  idiom (`offsetof`), and no RTOS capability is lost; a timer kept apart from
  its state would need a lookup, and none exists in the tree. Decides C1.
  Open inside C1: rclc's init takes a `support`, and a C component hosted by
  the generated entry has none (`rcl_compat.h` section 4); and the
  `rclc_timer_init_with_info` flavour also carries a context.
* **D3 — Rust's primary model: already settled, no decision here.**
  phase-483 (decisions 3 and 4, 2026-10-09) made `nros` rclrs-shaped, with
  `use nros as rclrs;` reading as rclrs and ONE `nros::Node` type, the
  component model an extension on top. This phase listed D3 by mistake. R1–R5
  are remaining gaps against that ruling and need no further decision.
* **D4 — Rust facade internals: a SEALED FFI TRAIT.** Methods that exist only
  for nros-c / nros-cpp move off `Executor`'s inherent impl into a sealed
  extension trait in `nros-node` (working name `nros_node::ffi::ExecutorFfi`)
  that the shims import and `nros` never re-exports. Both shims already
  depend on `nros-node` directly. Not `#[doc(hidden)]`, which would leave them
  callable and semver-visible. Decides R9.

## Work items

### W0 — the ledger's reasons match the code

Spot-checking eight candidate rows against the source found three whose reason
no longer describes the code, and two candidates that are already done:

| row | ledger says | the code says |
| --- | --- | --- |
| `c:timer_init_default` | declined: "`rcl_timer_init` IS the default form" | `rclc/detail/rcl_compat.h` §4 refuses it over the CALLBACK CONTRACT (`last_call_time` vs `context`), a real reason the row never states |
| `c:executor_add_subscription` | divergence: an extra deserialiser argument, because the descriptor has no slot | phase-492 gave the descriptor a codec and the argument is gone; the context-less form is refused because adapting it "needs a trampoline that remembers the original function pointer" (`nros_generated.h`, `rclc_executor_add_subscription_with_context`) |
| `rust:Publisher::publish` | divergence: "`publish(&self, msg: &M, buf: &mut [u8])`", caller supplies the buffer | `EmbeddedPublisher::publish(&self, msg: &M)` uses an internal `DEFAULT_TX_BUF`; only `&M` vs rclrs's by-value message still differs |
| `cpp:Rate`, `cpp:WallRate`, free `spin_until_future_complete` | (correct) | shipped, `adopt-bounded` — not candidates |
| `cpp:KeepLast::KeepLast` | declined: tag type not needed | `constexpr QoS KeepLast(size_t)` in `qos.hpp` makes `rclcpp::QoS(rclcpp::KeepLast(10))` compile; only the constructor row is theirs-only — not a candidate |

`check-ledger-orphan-refs` validates cited FILES. Issue 1012 fixed 23 rows
citing renamed symbols but left its gate undone, because a heuristic cannot
tell a historical mention from a stale one. Nothing checked a quoted
SIGNATURE, which is the drift above.

**Do:** re-read every `declined` and `divergence` row in the families the
review marked (async helpers, parameter objects, per-process config, callback
groups, timers, logging, accessors), and correct the prose where the code
moved. Decide whether a signature quoted in a reason can be checked against the
extracted surface the report already holds.

**Done when** each reviewed row's reason matches the code at the commit it is
reviewed against, and the rows in the table above are corrected.

**W0.a — LANDED 2026-10-25: quoted signatures are checked.** `--check` now reads
every call-shaped span in a `why` and, when the extraction knows the name (both
sides, all languages, both editions, defaulted parameters and a Rust `self`
allowed for), requires that argument count to be accepted. A quote describing a
retired shape is listed in the row's `retired_quotes` (SCHEMA.md), so tense is
stated rather than guessed, and a listed quote the prose dropped is itself red.
First run: 35 findings, 33 real, 2 false (Jazzy-only `Rate` constructors, fixed
by indexing every edition; an unqualified `from_baked` matching a different
type, fixed by qualifying the quote). Corrected, with a dated note in each row:

| row(s) | was | now |
| --- | --- | --- |
| the table above | as listed | rewritten from the code |
| `c:{publisher,subscription}_get_*_count` + 3 C++/Rust twins | C shipped as `rcl_*_get_*_count(…, executor, out_count)` "on upstream's name" | shipped as `rclc_*`; neither `rcl_` name exists |
| `c:init_options_*` | `rclc_support_init(support, locator, domain_id)` | rclc's `(support, argc, argv, allocator)` since phase-492; values from build/env |
| `c:node_ref_t` | we keep `rcl_publisher_fini(&pub)` | rcl's `(pub, node)`; the identity check stays |
| six `c:*_clock_{init,fini}` | `rcl_clock_init(clock, type)`, no allocator | rcl's `(type, &clock, &allocator)` |
| `c:publisher_init` | five-argument `_with_options` | six, with `qos` |
| `cpp:init`, `cpp:remove_ros_arguments` | native `nros::init(...)` | `rclcpp::init_in` / `rclcpp::init` (phase-483) |
| `cpp:SerializedMessage` | `Subscription::take_serialized` | `PollSubscription::take_serialized` (phase-456 W2b) |
| `rust:ParameterBuilder::new` | `(&mut ParameterServer, name)` | `(&mut ParameterServer, NodeKey, name)` |
| `c:executor_add_{subscription,service}_raw` | cite `rclc_executor_add_subscription_typed` + per-message macro | `rclc_executor_add_subscription_with_context` (found by hand; names, not arity) |

**W0.b — OPEN: retired NAMES.** The arity check judges only names the
extraction knows, so a quote of a deleted function passes. Measured over the C
prefixes alone (`rclc_`, `rcl_`, `nros_` in backticks, resolved against both
surfaces plus every token in our installed headers): **100 distinct names in
259 references** resolve nowhere. Some are history (deprecated aliases, dated
CORRECTED notes), some are not C symbols (crate names, Rust functions, macros),
and some are stale current-tense claims like the `_typed` pair above, which is
1012's tense problem again. W0.a's escape field generalises to it (a
`retired_names` list beside `retired_quotes`), and the sweep is that list plus a
prose fix for each stale one. Not started; C++ and Rust names are not measured.

### C — rclc / rcl

**C1 — `rclc_timer_init_default(timer, support, timeout_ns, callback)`.
LANDED 2026-10-25.** rclc's and rcl's timer surface, with their signatures and
callback contract (the upstream-signature gate measures all four names):
`rcl_timer_callback_t` `(timer, int64_t last_call_time)`,
`rclc_timer_init_default` / `_default2`, `rcl_timer_init` / `_init2`.
Removed: `rclc_timer_init_with_context`, `_with_context2`,
`rclc_timer_init_on_clock`, `_on_clock2`, `rclc_timer_callback_with_context_t`
and the `context` field of `rcl_timer_t`; `rclc_timer_init_with_info` and its
callback lost their context too. All 34 call sites moved (23 components,
8 `main.c` examples, 3 tests), plus the `nros new` C scaffold, the book and
the nros-c guide. A component's callback reaches its state with
`RCLC_CONTAINER_OF(timer, my_t, timer)` (new, `<rclc/timer.h>`); a `main.c`
reads its file-scope `app` as an rclc program does.

`last_call_time` comes from the dispatcher: `TimerCallInfo` gained
`since_last_call_ns`, the `elapsed_us` count at the moment the activation is
due, i.e. what `rcl_timer_get_time_since_last_call` reports, so it survives
`rcl_timer_exchange_period` and `rcl_timer_reset`. Resolved in-item: `support`
(rclc) and `context` (rcl) may be NULL, since neither is read and a hosted C
component has no support; a non-default allocator is refused. All three are
the rows' `adopt-bounded` envelope. `timer_clock_source.c` pins 10 ms after a
reset reading exactly 10 ms and `RCLC_CONTAINER_OF` reaching a timer embedded
mid-struct.

**C2 — context-less `rclc_executor_add_subscription(exec, sub, msg, cb, invocation)`.**
Refused because a `void (*)(const void *)` callback would need a trampoline that
remembers the original pointer, which RFC-0019/0020 call "state in the
wrapper". The pointer would live in the executor's own arena entry, as C1's
contract tag would, not in a wrapper. Confirm that reading of RFC-0019/0020;
if it holds, adopt at rclc's arity.

**C3 — typed `rclc_executor_add_service` / `rclc_executor_add_client`.**
Only `rclc_executor_add_service_raw` (CDR bytes) exists. Since phase-492 the
descriptor carries the codec the typed subscription uses, so the typed forms
with caller-owned request/response messages are the same pattern. Adopt.

**C4 — `rclc_action_server_init_default` + `rclc_executor_add_action_server`.**
Refused because our action callbacks are bound at init (`rclc/action.h`:
"closing this needs the `rclc_executor_add_action_server` binding site").
Phase-417 stage 6 moved subscription and service callbacks to registration;
actions were not moved. Move them, then adopt both names.

**C5 — `rcl_get_error_string` / `rcl_error_state_t`.** Declined because there
is no per-thread error slot. A bounded last-error record (one per executor, or
one global under the platform lock) gives a C user the message instead of only
the code. Adopt-bounded; the envelope is "last error on this executor".

**C6 — `rcl_*_get_options` (publisher, subscription, client, service, action
client, action server).** Declined because nothing retains the options. Keep a
copy on the handle; price the bytes per entity before deciding.

**C7 — issue 2030.** The C callback tier has no cancel-reply slot, so the fifth
`rclc_action_cancel_goal` is refused. Medium severity; it is a correctness gap,
not a shape choice. **Resolved 2026-10-24:** the arena entry has a cancel-response
slot, C registers it, and `rclc_action_client_set_cancel_response_callback` is new.
The C++ half (taking replies in the spin) is issue 2034, **resolved 2026-10-25**: the C++ client registers the slot and the spin feeds its `async_cancel_*` futures and callbacks.

**C8 — open renames (5):** `rcl_publish_serialized_message` (ours
`rclc_publish_raw`), `rcl_service_server_is_available`,
`client_server_available`, `executor_handle_invocation_t`,
`action_server_get_active_goal_count`. Land each, or re-verdict it with a
reason.

### P — rclcpp

**P1 — `Executor::spin_until_future_complete(future, timeout)`.** The free
function ships (`adopt-bounded`); the executor member does not. Declined under
RFC-0021, but the member drives the executor exactly as the free function
does. Adopt-bounded with the same envelope.

**P2 — `Executor::add_node`, `remove_node`, `spin_node_some`,
`spin_node_once`, `spin_all`.** Declined because nodes are created by the
executor. The executor can accept its own node (no-op) and refuse a foreign one
loudly; `spin_all` maps onto the bounded spin. Adopt-bounded.

**P3 — `rclcpp::Parameter`, `ParameterValue`, `Node::get_parameters` /
`set_parameters` over vectors.** Declined because the value object owns
`std::string`/`std::vector`. Rust's `nros_params::Parameter` is already a
bounded inline record (C has typed get/set on `rclc_parameter_server_t` and no
record type). A bounded `rclcpp::Parameter` works freestanding. *D1: freestanding only* --
adopt the bounded record; the vector forms stay declined.

**P4 — `GenericPublisher` / `GenericSubscription`.** Declined as runtime-typed.
They move serialized bytes plus a type NAME, and our raw entities move bytes.
The open question is the type HASH: zenoh's key expression carries it
(RIHS01), and nothing resolves a hash from a name at runtime. Investigate;
if no hash source exists, make it REFUSE-LOUD (RFC-0089 already uses
`create_generic_subscription` as its example) rather than absent.

**P5 — `Duration::to_chrono`, `Clock::sleep_for`, `Clock::sleep_until`.**
*D1: `to_chrono` stays declined.* `sleep_for` / `sleep_until` take rclcpp's
`Duration` / `Time` and remain candidates; they must satisfy RFC-0021 (drive the executor, or not block the transport).

**P6 — `ParameterEventHandler` (11 `gap` rows), issue 2024.** We publish
`/parameter_events`; nothing subscribes to another node's. A bounded handler
table over a typed `rcl_interfaces/msg/ParameterEvent` subscription.

**P7 — two polling subscription types.** `PollSubscription` (20 members) and
`PollingSubscription` (7) both ship. Confirm each has a distinct contract, or
fold one into the other.

**P8 — open renames (5):** `rclcpp_action::create_server` /
`create_client` (ours `Node::create_action_server` / `_client`),
`create_subscription`, `create_server`, `Client::service_is_ready`.

### R — rclrs

**R1 — `IntoPrimitiveOptions`: `"topic".keep_last(10).reliable()` (28 rows).**
Declined because QoS is compile-time and the builder "buys nothing with one
caller shape". That is a preference. The builder produces a plain value, works
in `no_std`, and is the usual rclrs create-site spelling. Adopt.

**R2 — `IntoNodeOptions`: `create_node("name")` (9 rows).** `NodeOptions` is
already `Copy` with three fields; `From<&str>` plus an `impl Into` parameter
closes it. Adopt.

**R3 — `ToLogParams`: `log_info!(logger.once(), …)`, `.throttle(d)`,
`.only_if(b)` (14 rows).** Declined as an overload trait. The modifiers need no
allocation. `nros-log/src/macros.rs` names three ledger rows this would
reverse; reverse them. Adopt.

**R4 — `create_timer_repeating` / `_oneshot` / `_inert`.** The ledger row
records that rclrs 0.7.0 converged on our three timer modes; ours are
`create_timer_in_group` / `_on_clock` / `_with_info` plus a mode set on the
entity. Match rclrs's verbs.

**R5 — `Publisher::publish` by value.** After W0's correction the only
difference is `&M` against rclrs's owned-or-borrowed message
(`MessageCow`). Accept both through one trait if it costs nothing.

**R6 — `Context::ok`.** Declined because our `Context` has no shutdown state.
Low priority; add only if a shutdown state exists for it to report.

**R7 — `ServiceInfo`, issue 1942.** rclrs hands a service callback the request
id and timestamps; ours passes the request alone.

**R8 — open renames (33).** `UndeclaredParameters` → `Parameters`,
`GoalId` → `GoalUuid`, `CancelReturnCode`, `QoSProfile::{deadline,lifespan,
liveliness_lease}_duration`, `ValueConversionError`, `SetParameterResult`,
`ActionClientCore::is_server_ready`, and their members. Land each, or
re-verdict it with a reason.

**R9 — internals in the public facade. *D4: sealed FFI trait.*** `Executor` has 276 public
members; at least 66 are entry points the C/C++ layers call
(`add_arena_subscription_c_callback`, `from_session_ptr`, `*_on`, `*_raw`).
`ActionServerCore`, `ActionClientCore` and the raw handles add about 75 more.
They are public so the shims can reach them, not so a Rust user can.

**R10 — three parameter receivers.** `ParameterServer` (55 rows), `Executor`
(~29) and `Node` each carry parameter verbs; rclrs keeps them on the node.
Pick one user-facing receiver and keep the others internal.

### X — cross-language

**X1 — issue 2029.** An action server answers the zero-id `CancelGoal` forms
(cancel-all, cancel-before-time) with `UNKNOWN_GOAL_ID`. Behaviour, in the
shared action core, so it lands once for all three languages. **Resolved
2026-10-24** in `ActionServerCore::try_handle_cancel`; goals now carry an
acceptance stamp.

## Acceptance

For each item: the ledger row moves (to `same`, to a `systematic`/`arity-only`
pair, or to a corrected verdict with a reason the code supports),
`just check api-parity` stays green, and the adopted name has a test that
calls it with upstream's signature. A refusal carries its RFC-0089
disposition, and REFUSE-LOUD is preferred over absent where a user will reach
for the name.

The phase is done when W0's review is complete, every item is landed or
re-verdicted with a reason that names a constraint, and the four issues above
are closed or re-homed.
