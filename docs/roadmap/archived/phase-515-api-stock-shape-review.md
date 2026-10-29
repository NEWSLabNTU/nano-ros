# Phase 515 — Re-audit the recorded API differences against stock ROS 2 shape

**Status (2026-10-29). Archived: every item is landed or re-verdicted, and every
homed issue is closed.** W0.a and W0.b landed; C1–C8, P1–P8, R1–R5 and R7–R10,
and X1 landed or were re-verdicted with a reason. **R6 stays declined**:
`Context::ok` is added only once `Context` has a shutdown state to report, and
it has none. `api-parity --by-topic` reports 0 open decisions in every stage
and language. The one ledger `gap` P3's re-verdict found,
`cpp:ParameterCallbackHandle`, was closed after the archive by
[issue 2114](../../issues/archived/2114-cpp-parameter-callback-handle-name-taken.md)
(2026-10-29): the name is upstream's event-handler handle again, and the
on-set token is `rclcpp::OnSetParametersCallbackToken`. W0.c (2026-10-29)
extended the retired-name check to C++ and Rust names. W0.d (in progress,
2026-10-29) holds code comments to the same rule, one area at a time.

Opened from a review of the
`api-parity` report on main `1a30bcbb22`. The work items below are candidates:
each starts by confirming its reason against the code, because W0 found that
the ledger's prose has drifted. The four decisions (D1–D4) are taken;
see Decisions.

**Prior:** [phase-379](phase-379-api-parity-with-ros2-client-libraries.md)
(the measurement and the ledger),
[phase-417](phase-417-ros2-api-adoption.md) (RFC-0089 adoption),
[phase-482](phase-482-rclcpp-drop-in-residue.md),
[phase-483](phase-483-rclcpp-rclrs-shape-only.md) (`rclcpp::` and
rclrs shape only), [phase-492](phase-492-c-api-rcl-names.md) (C on rcl
names, descriptors gain a codec),
[phase-506](phase-506-api-crates-carry-only-the-api.md).
**Implements:** [RFC-0089](../../design/0089-ros2-api-adoption-and-the-compile-or-conform-rule.md):
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

## Open API issues homed here

Each was left without a phase when its finding phase closed.

| issue | severity | item |
| --- | --- | --- |
| [2030](../../issues/archived/2030-c-callback-tier-cancel-reply-slot.md) — the C callback tier never takes a cancel reply (resolved 2026-10-24) | medium | C7 |
| [2024](../../issues/archived/2024-parameter-event-handler.md) — no `ParameterEventHandler` (**resolved 2026-10-25**, hosted C++) | low | P6 |
| [1942](../../issues/archived/1942-rclrs-service-info-no-counterpart.md) — no `ServiceInfo` for a service callback (**resolved 2026-10-26**) | low | R7 |
| [2080](../../issues/archived/2080-cpp-parameter-event-from-event-helpers.md) — `ParameterEventHandler::get_parameter(s)_from_event` (P6's residue; **resolved 2026-10-26**) | low | P6 |
| [2081](../../issues/archived/2081-c-abi-service-request-info.md) — C-ABI backends report no `ServiceInfo` (R7's residue; **resolved 2026-10-26**) | low | R7 |
| [2029](../../issues/archived/2029-action-server-cancel-all-before.md) — cancel-all / cancel-before answer `UNKNOWN_GOAL_ID` (resolved 2026-10-24) | low | X1 |

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
**LANDED 2026-10-27.** `--check` now resolves every backticked `rclc_` / `rcl_` /
`nros_` span against both extractions plus every CODE identifier (comments
stripped) in tracked sources under `packages/` and the upstream surfaces; a
span ending `*`, `{` or `_` is a prefix that must begin a live name. History is
listed per row in `retired_names` (SCHEMA.md), which is itself checked both
ways (named in the prose, and not live). Stripping comments mattered: a stale
`nros_executor_spin_some` survived in one test's doc comment, so a token scan
of the raw sources passed the very mutation this exists for. The sweep moved
~190 rows: about 60 stale current-tense claims corrected to the live name
(mostly the pre-phase-487 `nros_*` polling spellings, the pre-phase-492
`nros_*_get_zero_initialized` / `nros_node_init` family, and the `nros_executor_*`
verbs), the rest history listed in `retired_names`. Three upstream names live in
rcl headers the extraction does not read (`rcl/remap.h`,
`rcl/logging_rosout.h`, `rcl_lifecycle`) and are allowed by a named set in the
script. **Still not measured:** C++ and Rust names without a C prefix, and stale
names in CODE comments (the generated header still documents
`nros_lifecycle_register_on_*`).

**W0.c — LANDED 2026-10-29: C++ and Rust names.** `--check` now judges every
CODE-SHAPED backticked span, not only the C-prefixed ones: a `::` path (each
segment must exist), or an identifier that holds `_` or a capital or is called.
A lone lowercase word, a file name, a commit id and names from rcutils, Zephyr,
`std` or a `*_msgs` package are not judged. The known set grew to match what
rows cite: `examples/`, `scripts/` (NAME tokens only), `just/`, `cmake/`,
tracked file stems, and the ledger's own fields. An unqualified name also
resolves through the prefix a C key drops (`cdr_write_string`). Another
project's name goes in a new `foreign_names` list (SCHEMA.md), and a row
carrying `retired` is history throughout and is skipped.

Measured first: 313 findings in 245 rows. They came from 140 names. Three
reach fixes came out of measuring, each of which had hidden real names:

- The comment stripper did not know string literals. A `"/**"` glob in
  `ros_args.rs` swallowed 840 lines of code.
- Whole-line `#` comments in `.just`/`.toml`/`.cmake` counted as code.
- A string literal in Python is not evidence. The checks keep tables of
  retired spellings (`TYPE_SYNONYMS = {"NodeCtx": ...}`), and counting their
  keys hid every row still calling the node handle `NodeCtx`.

The sweep corrected the prose of 59 rows:

| was | now |
| --- | --- |
| `nros::NodeCtx` / `NodeCtx::get_*` named as the shipped node handle (16 rows) | `nros::Node` / `Node::` (phase-483 W2) |
| `NROS_LIFECYCLE_{STATE,TRANSITION,RET}_*`, `nros_executor_lifecycle_*`, `NROS_RET_*` | the `RCLC_` / `rclc_` names (phase-492) |
| `NROS_C_GOAL_*` / `NROS_C_CANCEL_*` in `component.h` | `RCLC_GOAL_*` / `RCLC_CANCEL_*` in `nros_generated.h`; `server.hpp`'s comments named the never-shipped `RCLC_C_*` spelling and are corrected too |
| `NROS_COMPONENT_SLOT_BYTES` and three sibling knobs | `NROS_RUNTIME_*` |
| "no `PUBLICATION_MATCHED` event anywhere in the tree" (5 rows) | issue 1972 added it; noted, verdict unchanged |
| `rcl_action_server_t`'s polling state | moved to `rclc_polling_action_server_t` (phase-487 W4) |
| C reads one of four boot-config fields | issue 0794 gave C all four |
| `init_with_launch*` / `ContextSource::Launch` | deleted by issue 1931 |
| `nros::host_env`, `DeserializeBorrowed`, `nros_log::nros_error!`, `owned_entities_`, `NOT_FOUND`, `ACTUALQOS` | the live name, or a dated note |

The rest is history listed in `retired_names` (152 entries) or another
project's name in `foreign_names` (64). CLAUDE.md's issue-0589 entry named the
removed `nros_error!` macro and now names `log_error!`.

**Still not measured, and the next step: CODE comments.** The same resolver
over comments in tracked sources under `packages/` finds 2,311 references to
1,414 names. 238 of those references (95 names) are C-prefixed and sit in the
C/C++ API alone, 60 of them in `nros_generated.h`. A comment has no
`retired_names` list beside it, so a gate needs either a ratchet baseline or an
inline history marker first, and that is a design decision rather than a sweep.

**W0.d — IN PROGRESS: code comments.** Maintainer ruling 2026-10-29: an INLINE
marker, not a ratchet baseline. A comment names history by striking it,
~~`old_name`~~, which rustdoc renders struck and a reader of plain source reads
as "gone". `just check comment-names` (`scripts/check/check-comment-names.py`,
fast lane, 1.7 s) judges every backtick name in a comment of a tracked C, C++
or Rust source in its SCOPE:

- an unstruck name must resolve through the ledger check's index (code with
  its comments stripped, plus what a tracked shell script defines, plus
  `zephyr/` and `integrations/`);
- a struck name must NOT exist here — the marker states tense, so a live name
  under it is a finding;
- another project's name is listed once in
  `scripts/check/comment-foreign-names.txt`, grouped by project; a listed name
  that resolves here or that no comment quotes any more is a finding;
- `CONFIG_NROS_*` must be a `config` in a tracked Kconfig; any other
  `CONFIG_*` is the RTOS's;
- a placeholder (`Node::create_X`, `foo_pkg`) is not judged.

Measured on main `d7851e77ae` with packages/ and examples/ in scope: 2,365
references to 1,430 names. The scope widens one area per pull request; it is a
list of directories already swept, not a baseline, and nothing inside it is
exempt.

| Pull request | Scope added | Findings | Struck | Foreign names | Renamed or reworded |
| --- | --- | --- | --- | --- | --- |
| 1 | `examples/`, `packages/{drivers,platform,tooling,verification}` | 286 | 88 | 91 | the rest |
| 2 | `packages/boards` | 190 | 53 | 70 | the rest |
| 3 | `packages/cli` | 276 | 136 | 50 | the rest |
| 4 | `packages/testing` | 290 | 163 | 56 | the rest |

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
**LANDED 2026-10-27.** The reading holds: RFC-0020's checklist asks that a
callback be a trampoline that translates the ABI and calls through, and forbids
state in the wrapper; the arena entry has owned `callback` and `context` since
phase-487 W1. `rclc_executor_add_subscription(executor, sub, msg, cb,
invocation)` is `_with_context` with a fixed trampoline as the callback and the
user's `rclc_subscription_callback_t` in the entry's `context` slot. RFC-0089's
"stays refused" carries a dated amendment. Test:
`tests/run/context_less_subscription.c`. C3's typed service and client use
context-less callbacks too, so the same reading applies there.

**C3 — typed `rclc_executor_add_service` / `rclc_executor_add_client`.**
Only `rclc_executor_add_service_raw` (CDR bytes) exists. Since phase-492 the
descriptor carries the codec the typed subscription uses, so the typed forms
with caller-owned request/response messages are the same pattern. Adopt.
**LANDED 2026-10-27.** `rclc_executor_add_service`, `_with_request_id` and
`_with_context`, and `rclc_executor_add_client`, at rclc's signatures with
rclc's callback typedefs. Each is the raw registration with a fixed
trampoline; what the trampoline needs (the caller's messages, the
descriptor's codec halves, the callback) is copied into the arena entry as a
capture, so the C structs keep nothing but the descriptor pointer
(`rcl_service_t` / `rcl_client_t` gained `type_support`, as the subscription
did in phase-492 W8). The client half needed a capture on the client entry,
which only services and subscriptions had: nros-node's
`register_service_client_raw_capturing`. Envelope: a request or reply that does
not deserialise is dropped (no reply, no callback), and a descriptor with no
codec is `RCL_RET_UNSUPPORTED`. Test: `packages/api/nros-c/tests/run/typed_service_client.c`.

**C4 — `rclc_action_server_init_default` + `rclc_executor_add_action_server`.**
Refused because our action callbacks are bound at init (`rclc/action.h`:
"closing this needs the `rclc_executor_add_action_server` binding site").
Phase-417 stage 6 moved subscription and service callbacks to registration;
actions were not moved. Move them, then adopt both names.
**Scoped 2026-10-27 (maintainer): the full rclc model**, not just the binding
site. rclc's `rclc_executor_add_action_server(executor, server, handles_number,
ros_goal_request, ros_goal_request_size, goal_cb, cancel_cb, context)` hands each
goal out as an `rclc_action_goal_handle_t` whose `ros_goal_request` is an
element of the caller's `<Action>_SendGoal_Request` array, and the user answers
through `rclc_action_send_result(goal_handle, status, &get_result_response)` and
`rclc_action_publish_feedback(goal_handle, &feedback_message)`. Three PRs:
- **C4a — the envelopes.** Codegen emits `<Action>_SendGoal_Request`,
  `_GetResult_Response` and `_FeedbackMessage`; the goal id is
  `unique_identifier_msgs__msg__UUID`, defined by `<rclc/detail/uuid.h>` and by
  the generated header under one `NROS_DEFINED_<name>` guard (every generated C
  struct now carries one). The descriptor (`rclc_action_type_t`) gains each
  envelope's `sizeof` and payload `offsetof`, emitted by the compiler that laid
  the envelope out: `GetResult_Response`'s `result` follows an `int8`, so its
  offset depends on the Result type's alignment. Codegen version 21.
  **LANDED 2026-10-27** (test `generated_c_action_envelopes_and_uuid_guard_syntax_check`,
  both include orders, mutation-checked against an unguarded UUID).
- **C4b — the runtime.** `rclc_action_server_t` / `rclc_action_goal_handle_t`,
  `rclc_action_server_init_default`, the eight-argument
  `rclc_executor_add_action_server`, `rclc_action_send_result`,
  `rclc_action_publish_feedback`, over the existing raw registration.
  **LANDED 2026-10-27**, with one change of plan: the goal-handle pool lives
  INLINE in `rcl_action_server_t` (`_rclc`, `NROS_MAX_CONCURRENT_GOALS` handles),
  not as an arena capture -- `rclc_action_send_result` and
  `rclc_action_publish_feedback` take a goal handle alone, so the handle must
  reach its server and the server its pool without the executor. Two trampolines
  installed as the server's goal/cancel callbacks run rclc's bookkeeping. Bounds
  (stated on the declarations, ledger rows adopt-bounded): a goal with every
  handle held is rejected (rclc queues it); an accepted goal is EXECUTING at once,
  so a result may be sent before the client asks for it. New return codes
  `RCL_RET_ACTION_GOAL_ACCEPTED`/`_REJECTED` and
  `RCLC_RET_ACTION_WAIT_RESULT_REQUEST`, plus `rcl_action_goal_state_t` and the
  `GOAL_STATE_*` constants. Test `packages/api/nros-c/tests/run/rclc_action_server.c`
  (`just check c`).
- **C4c — migration.** The C action examples move to rclc's shape; the
  init-bound `rclc_action_server_init_with_callbacks` stays one release.
  **LANDED 2026-10-27**: the six `c/action-server` mains (linux, the four RTOS
  copies, bare-metal mps2) register with `rclc_executor_add_action_server` and
  run accepted goals between `rclc_executor_spin_some` calls; the three C
  COMPONENTS (zephyr, `workspaces/c`, `workspaces/mixed`) have no main loop, so
  a 20 ms timer runs the accepted goal. A component has no `rclc_support_t`, so
  `rclc_action_server_init_default` now accepts a NULL `support`, exactly as
  `rclc_timer_init_default` does. Remaining `rclc_action_server_init_with_callbacks`
  users are tests and the C++ wrapper, which keep the init-bound path.

**C5 — `rcl_get_error_string` / `rcl_error_state_t`.** Declined because there
is no per-thread error slot. A bounded last-error record (one per executor, or
one global under the platform lock) gives a C user the message instead of only
the code. Adopt-bounded; the envelope is "last error on this executor".
**Decided 2026-10-27 (maintainer): global, at rcl's names.** **LANDED
2026-10-27:** `rcl_error_state_t` / `rcl_error_string_t` with rcutils' members,
`rcl_set_error_state`, `rcl_error_is_set`, `rcl_get_error_state`,
`rcl_get_error_string`, `rcl_reset_error`, and `RCL_SET_ERROR_MSG` /
`RCL_CHECK_FOR_NULL_WITH_MSG` / `RCL_CHECK_ARGUMENT_FOR_NULL` in
`<rcl/error_handling.h>`. One process-wide record behind `nros_rmw::sync::Mutex`,
filled by every `validate_not_null!` / `validate_state!` refusal (214 sites;
the message names the argument) and by the caller. Envelope: process-wide, not
per thread; `RCUTILS_ERROR_MESSAGE_MAX_LENGTH` 256 (rcutils 1024).
`RCL_SET_ERROR_MSG_WITH_FORMAT_STRING` is not provided (it needs a formatter
on every target). Refusals OTHER than those two macros still return a code
alone. Test `packages/api/nros-c/tests/run/rcl_error_state.c` (`just check c`).

**C6 — `rcl_*_get_options` (publisher, subscription, client, service, action
client, action server).** Declined because nothing retains the options. Keep a
copy on the handle; price the bytes per entity before deciding.
**Decided 2026-10-28 (maintainer): adopt, all six.** **LANDED 2026-10-28:**
each handle holds the options it was initialised with, and
`rcl_{publisher,subscription,service,client,action_client,action_server}_get_options`
returns a pointer to that copy: all zero after an init without options, NULL for
a NULL or unusable handle. Where the handle already copied `sched_context_id`,
the options struct REPLACES that field, and the executor reads
`options.sched_context`. Envelope: our options carry the scheduling context
(and the subscription's message-info flag; the publisher's carries reserved
bytes only), not rcl's `qos` / `allocator`, so a ported `->qos` does not
compile. Handle size and the padding the options leave, measured with
`sizeof`/`offsetof` (x86_64 / cortex-m3); the options struct replaces a
`sched_context_id` the handle already held, so padding is the growth:

| entity | handle | padding |
| --- | --- | --- |
| subscription | 752 / 704 | 7 / 3 after the options |
| service | 744 / 704 | 3 / 3 |
| client | 752 / 708 | 3 / 3 |
| action client | 784 / 716 | 7 / 3 |
| publisher | 1336 / 1320 | 4 / 3 after `topic_name` (new field) |
| action server | 1144 / 992 | -- |

Test `packages/api/nros-c/tests/run/entity_get_options.c` (`just check c`).

**C7 — issue 2030.** The C callback tier has no cancel-reply slot, so the fifth
`rclc_action_cancel_goal` is refused. Medium severity; it is a correctness gap,
not a shape choice. **Resolved 2026-10-24:** the arena entry has a cancel-response
slot, C registers it, and `rclc_action_client_set_cancel_response_callback` is new.
The C++ half (taking replies in the spin) is issue 2034, **resolved 2026-10-25**: the C++ client registers the slot and the spin feeds its `async_cancel_*` futures and callbacks.

**C8 — open renames (5):** `rcl_publish_serialized_message` (ours
`rclc_publish_raw`), `rcl_service_server_is_available`,
`client_server_available`, `executor_handle_invocation_t`,
`action_server_get_active_goal_count`. Land each, or re-verdict it with a
reason. **`rcl_service_server_is_available` LANDED 2026-10-27:** rcl's
name and three arguments (`RCL_RET_UNSUPPORTED` when the backend cannot tell);
`rclc_client_server_available` and `rclc_client_service_is_ready` keep their
shapes one release. That supersedes the 2026-09-03 target of reshaping
`rclc_client_service_is_ready` in place, an ABI break C cannot alias. The
other two, `executor_handle_invocation_t` and `action_server_get_active_goal_count`,
are already `landed` records. Open: `rcl_publish_serialized_message`.

**C8 `rcl_publish_serialized_message` LANDED 2026-10-27** as the ledger's settled
word swap, not upstream's type: C `rclc_publish_serialized`, C++
`Publisher::publish_serialized`, Rust `EmbeddedPublisher::publish_serialized`
and `EmbeddedRawPublisher::publish_serialized[_with_attachment]`, matching the
receive side's `take_serialized`. Every old name stays one release as a
deprecated forwarder (C: a `static inline` with the deprecated attribute). Not
renamed: the RMW trait's `publish_raw` and the `nros_cpp_publish_raw` FFI
symbol — seams no user writes, and the generated C++ exports name the symbol.

### P — rclcpp

**P1 — `Executor::spin_until_future_complete(future, timeout)`.** The free
function ships (`adopt-bounded`); the executor member does not. Declined under
RFC-0021, but the member drives the executor exactly as the free function
does. Adopt-bounded with the same envelope.
**LANDED 2026-10-27** with P2: `rclcpp::Executor::spin_until_future_complete(future,
int32_t timeout_ms = -1)` and the same member on
`executors::SingleThreadedExecutor`, the free function's loop on that
executor (10 ms slices, the future re-checked after each). An explicit executor
stops on the termination request, since `rclcpp::ok()` describes only the
process executor. Test: `executor_node_members_runtime.cpp`.

**P2 — `Executor::add_node`, `remove_node`, `spin_node_some`,
`spin_node_once`, `spin_all`.** Declined because nodes are created by the
executor. The executor can accept its own node (no-op) and refuse a foreign one
loudly; `spin_all` maps onto the bounded spin. Adopt-bounded.
**LANDED 2026-10-27.** On `rclcpp::Executor` and
`executors::SingleThreadedExecutor`: `add_node(node, notify)` checks the
attachment construction made (own node: no-op; foreign or uninitialised:
abort naming it); `spin_node_some(node)` / `spin_node_once(node, timeout_ms)`
are that check plus `spin_some` / `spin_once`, and drain every node on the
executor (the arena is indexed by handle, not node); `spin_all(max_ms)` is
`spin_some`'s drain, which already re-polls until a pass does nothing, with
zero refused as upstream throws for it. `remove_node` is REFUSE-LOUD
(`ros2_refuse_remove_node_probe.cpp`): arena registration is one-way.

**P3 — `rclcpp::Parameter`, `ParameterValue`, `Node::get_parameters` /
`set_parameters` over vectors.** Declined because the value object owns
`std::string`/`std::vector`. Rust's `nros_params::Parameter` is already a
bounded inline record (C has typed get/set on `rclc_parameter_server_t` and no
record type). A bounded `rclcpp::Parameter` works freestanding. *D1: freestanding only* --
adopt the bounded record; the vector forms stay declined.
**LANDED 2026-10-27.** The bounded record is issue 2056's
`rclcpp::Parameter` / `ParameterValue` (scalars on every target), so P3's
remaining work was the LEDGER: `cpp:Parameter::get_value` and
`get_parameter_value` still read `declined`/`absent` ("C++ does not have
ParameterValue") while both shipped and correlated `same`, and `--check` was
green. New check `misdeclared_absences` (the mirror of
`misdeclared_refusals`: an `absent` row on a key our side declares) found 11
such rows. 8 deleted or re-verdicted to their measured shape
(`QoS::get_rmw_qos_profile` and `rclcpp_action::Client::async_send_goal` are
`same:ret`, adopt-bounded); `Parameter::get_value` is adopt-bounded (a
wrong-type read answers zero); `Server::execute` is a named collision; and
`ParameterCallbackHandle` is a `gap` -- the name is our on-set token, not
upstream's event-handler handle (issue 2114). The vector-taking
`get_parameters` / `set_parameters` stay declined (D1).

**P4 — `GenericPublisher` / `GenericSubscription`.** Declined as runtime-typed.
They move serialized bytes plus a type NAME, and our raw entities move bytes.
The open question is the type HASH: zenoh's key expression carries it
(RIHS01), and nothing resolves a hash from a name at runtime. Investigate;
if no hash source exists, make it REFUSE-LOUD (RFC-0089 already uses
`create_generic_subscription` as its example) rather than absent.
**LANDED 2026-10-27** (maintainer choice "Refuse-loud + hash overload").
Investigated: each generated type states `M::TYPE_HASH`, but nothing maps a
name to a hash at run time, so rclcpp's name-only spellings are REFUSE-LOUD
(`NROS_RCLCPP_REFUSE_GENERIC_BY_NAME`, probe
`ros2_refuse_generic_by_name_probe.cpp`). Beside them, the C++ twin of Rust's
`create_generic_publisher(topic, type_name, type_hash)`:
`Node::create_generic_publisher(topic, type, type_hash, qos)` returns a
`rclcpp::GenericPublisher` (`publish_serialized(data, len)`), and
`create_generic_subscription(topic, type, type_hash, qos, callback)` registers
`callback(const uint8_t*, size_t)` in the arena and returns a
`GenericSubscription::SharedPtr` handle (`rclcpp/generic.hpp`). Test:
`generic_pubsub_runtime.cpp` (the stub backend now records each entity's type
name and hash).

**P5 — `Duration::to_chrono`, `Clock::sleep_for`, `Clock::sleep_until`.**
*D1: `to_chrono` stays declined.* `sleep_for` / `sleep_until` take rclcpp's
`Duration` / `Time` and remain candidates; they must satisfy RFC-0021 (drive the executor, or not block the transport).
**Decided 2026-10-27 (maintainer): spin while sleeping. LANDED 2026-10-27:**
`Clock::sleep_until(until[, context])` / `sleep_for(rel_time[, context])` make
one call into the new `nros_cpp_spin_until_clock`, which spins the context's
executor until THIS clock reaches the deadline (Rust-side, as `Rate::sleep`'s
`nros_cpp_spin_for`), so callbacks run during the wait and a ROS-time clock on
`/clock` wakes on simulated time. `false` without sleeping before init, for a
deadline on another clock type (upstream throws), from inside a callback, or on
a termination signal. No `std::chrono` overload (D1). Test
`packages/api/nros-cpp/tests/compile/clock_sleep_runtime.cpp` (`just check cpp`).

**P6 — `ParameterEventHandler` (11 `gap` rows), issue 2024.** We publish
`/parameter_events`; nothing subscribes to another node's. A bounded handler
table over a typed `rcl_interfaces/msg/ParameterEvent` subscription.
**Resolved 2026-10-25 (hosted C++):** `rclcpp::ParameterEventHandler` and a
hosted `rclcpp::Parameter`; freestanding refuses the handler (issue 2024's
Still open). The two static `get_parameter(s)_from_event` helpers followed in
[issue 2080](../../issues/archived/2080-cpp-parameter-event-from-event-helpers.md)
(2026-10-26): templates over the caller's generated event, hosted like the
handler.

**P7 — two polling subscription types.** `PollSubscription` (20 members) and
`PollingSubscription` (7) both ship. Confirm each has a distinct contract, or
fold one into the other. **Resolved 2026-10-27 — distinct, both stay.**
`PollSubscription<M>` is CONSUMING: each `take*()` yields a sample once and it
carries the whole take family (`take`, `take_serialized`, `take_validated`,
`take_sequence`, `try_borrow`). `PollingSubscription<M>` is RETAINED: it HOLDS a
`PollSubscription<M>` (`sub_`) plus the newest value, so `take_data()` answers
repeatably (issue 0278, the analog of `autoware_utils::InterProcessPollingSubscriber`).
One composes the other, so there is nothing to fold; `polling_subscription.hpp`
states the choice under "WHICH ONE DO I WANT".

**P8 — open renames (5):** `rclcpp_action::create_server` /
`create_client` (ours `Node::create_action_server` / `_client`),
`create_subscription`, `create_server`, `Client::service_is_ready`. **Re-read 2026-10-27:** `create_subscription` and
`Client::service_is_ready` are `landed` records (where our old name went), not
open. `rclcpp_action::create_server` / `create_client` shipped with issue 2015,
so `cpp:create_server` is `adopt-bounded` (hosted) like `cpp:create_client`,
and the member factories `Node::create_action_{server,client}` are
`extension`s kept beside them.

### R — rclrs

**R1 — `IntoPrimitiveOptions`: `"topic".keep_last(10).reliable()` (28 rows).**
Declined because QoS is compile-time and the builder "buys nothing with one
caller shape". That is a preference. The builder produces a plain value, works
in `no_std`, and is the usual rclrs create-site spelling. Adopt.
**LANDED 2026-10-27.** `nros_node::{PrimitiveOptions, IntoPrimitiveOptions}`
(re-exported from `nros` and its prelude) with all 25 of rclrs's builder
steps, implemented for any borrowed string (rclrs's own `&T: Borrow<str>`
blanket, so `&String` still names an entity) and for `PrimitiveOptions`.
`Node::create_publisher` / `create_subscription` / `create_service` /
`create_client` take `impl IntoPrimitiveOptions`; a bare name keeps the kind's
default profile and a step overrides only the policy it names. Envelope: the
duration steps take `core::time::Duration` (the record is whole milliseconds,
`0` = infinite) where rclrs takes `QoSDuration`. The mock session now records
the profile each entity was requested with (`MockSession::created_qos`). Tests:
`primitive_options::tests`, `create_sites_take_rclrs_primitive_options`.

**R2 — `IntoNodeOptions`: `create_node("name")` (9 rows).** `NodeOptions` is
already `Copy` with three fields; `From<&str>` plus an `impl Into` parameter
closes it. Adopt.
**LANDED 2026-10-27.** `nros_node::IntoNodeOptions` (re-exported as
`nros::IntoNodeOptions`, and in the prelude) for `&str` and `NodeOptions`, with
rclrs's `into_node_options` and `namespace`. `NodeOptions` moved to `nros-node`
so `Executor::create_node` can take it (`nros` re-exports it unchanged); both
`Executor::create_node` and `NodeContext::create_node` take
`impl IntoNodeOptions`. On the executor the options' defaults mean the
executor's own namespace and session, as a bare name always did. rclrs's six
other builder steps are build-time here (RFC-0045) and got `declined` rows;
`rust:IntoNodeOptions` is `adopt-bounded` with its envelope on the trait.
Test: `create_node_takes_rclrs_into_node_options`.

**R3 — `ToLogParams`: `log_info!(logger.once(), …)`, `.throttle(d)`,
`.only_if(b)` (14 rows).** Declined as an overload trait. The modifiers need no
allocation. `nros-log/src/macros.rs` names three ledger rows this would
reverse; reverse them. Adopt.
**LANDED 2026-10-27.** `nros_log::{ToLogParams, LogParams, LogOccurrence,
ThrottleClock}` with rclrs's members; the five level macros, `log!` and
`nros_trace!` take a `ToLogParams` first argument (a `&Logger` still works).
The three rows' objections each had an answer: the `once` / throttle state is
a static the level macro declares per call site (rclrs's arrangement, and
constant-folded away for a plain call); `ThrottleClock` is `SteadyTime` only;
`throttle` / `throttle_clock` exist only under `cfg(nros_log_clock)`, so a
clockless `.throttle(d)` is still a compile error. Bounded to `&Logger` and
`LogParams` (no node or name impls). Tests: `tests/log_params.rs`,
`tests/log_params_throttle.rs` (`platform-clock`, in
`required-features-tests`).

**R4 — `create_timer_repeating` / `_oneshot` / `_inert`.** The ledger row
records that rclrs 0.7.0 converged on our three timer modes; ours are
`create_timer_in_group` / `_on_clock` / `_with_info` plus a mode set on the
entity. Match rclrs's verbs.
**LANDED 2026-10-27.** `Node::create_timer_repeating` / `_oneshot` /
`_inert(period, callback)`, `period` a `TimerDuration` or a
`core::time::Duration` (new `From` impl). Each returns the timer's `HandleId`
(the timer is in the executor's static table, so `cancel_timer` /
`reset_timer` take the id), not an rclrs `Timer`; an inert timer is created
unstarted and repeats once `reset_timer` starts it. The existing
`create_timer_in_group` / `_on_clock` / `_with_info` stay: they carry a group,
a clock or call info, which rclrs's three do not. The book's introduction uses
`create_timer_repeating`. Test: `node_creates_rclrs_repeating_oneshot_and_inert_timers`.

**R5 — `Publisher::publish` by value.** After W0's correction the only
difference is `&M` against rclrs's owned-or-borrowed message
(`MessageCow`). Accept both through one trait if it costs nothing.
**LANDED 2026-10-27.** `EmbeddedPublisher::publish(impl core::borrow::Borrow<M>)`:
`publish(&msg)` and `publish(msg)` both compile and put the same bytes on the
wire. It costs nothing — both serialise at once into the stack buffer — and no
`MessageCow` type is added, since `Borrow` is the one trait. Test:
`publish_takes_the_message_borrowed_or_owned`.

**R6 — `Context::ok`.** Declined because our `Context` has no shutdown state.
Low priority; add only if a shutdown state exists for it to report.

**R7 — `ServiceInfo`, issue 1942.** rclrs hands a service callback the request
id and timestamps; ours passes the request alone. **Resolved 2026-10-26:**
`nros::ServiceInfo` / `nros::RequestId` (rclrs's field names, every field an
`Option`, `nros::Time` timestamps), reached through
`Node::create_service_with_info` and `node.service(name).service_info()`; seam
`ServiceTrait::request_info`, re-exported there as `ServiceRequestInfo` so it
never shares a name with the seam's service descriptor again. zenoh reports
`request_id` + `source_timestamp` from the query attachment.
`rust:ServiceInfo` is `divergence` / `adopt-bounded`. **Residue resolved
2026-10-26
([issue 2081](../../issues/archived/2081-c-abi-service-request-info.md)):** the
vtable's appended `take_request_with_info` slot carries an
`rmw_service_info_t`; Cyclone DDS reports `request_id` + `source_timestamp`
(stock `rmw_cyclonedds_cpp`'s layout), XRCE-DDS reports `request_id` (the
Agent's `SampleIdentity`). The C and C++ callbacks get the id too:
`rclc_executor_add_service_raw_with_request_id` and rclcpp's three-argument
handler's request header. Residue: XRCE's slot was measured against a live
Agent and a stock `rmw_fastrtps_cpp` client on 2026-10-27 ([issue
2088](../../issues/archived/2088-xrce-service-request-id-unmeasured.md)), and the typed C service path's request-id flavour
landed with C3 as rclc's `rclc_executor_add_service_with_request_id` ([issue
2089](../../issues/archived/2089-c-typed-service-request-id.md), resolved
2026-10-27).

**R8 — open renames (33).** `UndeclaredParameters` → `Parameters`,
`GoalId` → `GoalUuid`, `CancelReturnCode`, `QoSProfile::{deadline,lifespan,
liveliness_lease}_duration`, `ValueConversionError`, `SetParameterResult`,
`ActionClientCore::is_server_ready`, and their members. Land each, or
re-verdict it with a reason. Decided 2026-10-27 (maintainer): each lands as rclrs's name
with the old one kept one release as a `#[deprecated]` alias and every in-tree
caller moved. **Parameter family LANDED 2026-10-27:** `UndeclaredParameters` →
`Parameters` (with rclrs's generic `get::<T>` / `set`), `ValueConversionError` →
`RmwParameterConversionError`. `ParameterBuilder`'s `ParameterError` vs rclrs's
`DeclarationError` is not a rename (ours already has a `DeclarationError`, and
`ParameterError` also serves get/set), so it is worked separately. **Also LANDED 2026-10-27:** rclrs's
`QoSProfile::{deadline,lifespan,liveliness_lease}_duration(Duration)` builders
(stored as the shared record's whole milliseconds, sub-millisecond rounding up
so it never reads as infinite); and the five `rust:nros_{debug..fatal}` rows,
whose `blocked-needs-decision` was stale -- phase-417 had already shipped
rclrs's `log_*!` and removed the `nros_*!` forwarders. **Action-core names LANDED 2026-10-27:** `GoalId` →
`GoalUuid`, `GoalStatus` → `GoalStatusCode`, `CancelReturnCode` →
`CancelResponseCode` (rclrs 0.7.0's names), the old ones deprecated aliases
re-exported from `nros_core`, `nros_node`, `nros` and its prelude; every
in-tree Rust caller moved (C and C++ keep their own names). rclrs's `GoalStatus`
is the `{code, goal_id, stamp}` record, ours `GoalStatusStamped`; the old
`GoalStatus` alias names the enum until it is removed. **`ParameterBuilder` terminals LANDED 2026-10-27:**
`mandatory` / `optional` / `read_only` return rclrs's `DeclarationError`
(new variant `NoValueAvailable`; `From<DeclarationError> for ParameterError`
keeps `?` callers compiling). **`SetParameterResult` collapse LANDED 2026-10-27
(maintainer go-ahead the same day):** every store write returns
`Result<(), ParameterError>` (rclrs has one spelling); `ParameterError` gained
`Undeclared`; the enum is `#[deprecated]` for one release. 204 uses in 12 files
rewritten; the wire reasons and C/C++ return codes are unchanged per verdict. (`ActionClientCore::is_server_ready` closed
2026-10-27 by R9 stage 2: it left the Rust surface, so Rust ships the one
spelling `ActionClient::action_server_is_ready` and its row was deleted.)

**R9 — internals in the public facade. *D4: sealed FFI trait.*** `Executor` has 276 public
members; at least 66 are entry points the C/C++ layers call
(`add_arena_subscription_c_callback`, `from_session_ptr`, `*_on`, `*_raw`).
`ActionServerCore`, `ActionClientCore` and the raw handles add about 75 more.
They are public so the shims can reach them, not so a Rust user can.

**Stage 1 (`Executor`) — LANDED 2026-10-26.** Measured on main `1a30bcbb22`+C1:
307 `pub fn`s on `Executor`'s inherent impls; by caller, 110 only nros-c/nros-cpp call, 74
nothing outside nros-node calls, 123 `nros` or another crate calls. "Only the
shims call it" is not the cut line. About 45 of the 110 are recorded in the ledger as Rust
features (`set_parameter_on`, `wait_for_publishers`, `cancel_timer`,
`is_spinning`, …) that no Rust caller happens to use yet. The cut line, taken
2026-10-26 (maintainer), is **C-shaped signature, plus shim plumbing**:

* C-shaped: a raw pointer, `extern "C"` callback, `c_void` or `unsafe fn` in
  the signature (`add_arena_subscription_c_*`, `from_session_ptr*`,
  `open_with_session_{in,slot,handle_slot}`, the `release_*` / `detach_*` /
  `retarget_*` raw paths, `register_service*_raw*`, `register_timer_c_capturing*`,
  `subscription_handle*`, `service_{server,client}_handle`, `wake_raw_ptr`,
  `set_park_primitive`, the shutdown-hook pair);
* plumbing: shim-only, and the method's own doc or row says it serves the FFI
  (`enter_spin_loop` / `exit_spin_loop`, `exec_lock`, `node_session_mut`,
  `register_action_{client,server}_raw`, `install_wake_signal_on_primary`,
  `set_entry_owner`, `set_primary_identity`, `session_io_failures`,
  `timer_elapsed_us` / `timer_is_ready` / `exchange_timer_period_us`);
* four C-shaped methods the `nros` runtime calls internally
  (`register_service{,_client}_raw_sized_on`, `service_client_entry_mut`,
  `action_client_core_mut`): `nros` imports the trait and does not re-export it.

63 methods in all. Each stays an inherent method, now `pub(crate)`, so nros-node's
own call sites are untouched. `nros_node::executor::ffi::ExecutorFfi` is a
sealed trait that forwards to them, and inside the crate the inherent method
wins resolution. The shims import it (`use … ExecutorFfi as _`). Outside
nros-node an inaccessible inherent method does not shadow the trait method.
44 `rust:Executor::*` ledger rows went with them, since the extractor counts a
trait impl as the trait's surface, not the type's.

**Not moved, and why:** entry points that generated code, boards or tests call
(`dispatch_callback` and `register_dispatch_slot` from `nros-macros` expansion,
`register_wake_source` from boards, `open_in` / `open_threaded`,
`set_violation_sink`, `spin_band_once`). Plumbing that no doc ties to the FFI
(`age_table`, `monitor_table`, launch-remap setters). Rust features with a
Rust-shaped signature. R4 and R10 re-home those.

**Stage 2 (action cores) — LANDED 2026-10-27.** `ActionServerCore`,
`ActionClientCore`, `ActionServerRawHandle` and `ActionClientRawHandle` carry
62 `pub fn`s (25 + 25 + 11 + 1; "about 75" was an estimate). None is C-shaped by
stage 1's signature test, so that test would move nothing. The cut line here is
the TYPE: all four are the untyped machinery the shims drive (phase-506 W1
already moved them to `nros::internals`, RFC-0071 D12). By caller, 42 are called
only from nros-c/nros-cpp, 10 by nothing outside nros-node, and the rest also
from the `nros` runtime; no crate outside those three calls any of them (the
hits in `examples/` and `nros-tests` are the typed `ActionServer` handle's
same-named methods). So every `pub fn` on the four types moved, constructors
included. `ActionServerRawHandle::invalid` / `is_invalid` stay `const fn`
inherently; their trait forwarders cannot be `const`, and no caller outside
nros-node needs a const context.

Same mechanism as stage 1: each method is `pub(crate)`, and one sealed trait
per type in `nros_node::executor::ffi` forwards to it (`ActionServerCoreFfi`,
`ActionClientCoreFfi`, `ActionServerRawHandleFfi`, `ActionClientRawHandleFfi`).
nros-c, nros-cpp and `nros`'s `node_runtime` import the ones they use. The
types stay public and nameable, because the sizes probe and the shims' storage
structs name them. The ledger extracts one member row for these types,
`rust:ActionClientCore::is_server_ready`; it was deleted, which also closes
that R8 rename.

**R10 — three parameter receivers.** `ParameterServer` (55 rows), `Executor`
(~29) and `Node` each carry parameter verbs; rclrs keeps them on the node.
Pick one user-facing receiver and keep the others internal.
**Decided 2026-10-27 (maintainer): the node, like rclrs.** **Stage 1 LANDED
2026-10-27:** `Node` carries the per-node parameter surface -- rclrs's
`declare_parameter::<T>(name)` (the `ParameterBuilder`) and
`use_undeclared_parameters()`, plus the rclcpp-shaped verbs the executor's bare
forms carried (`get_parameter`, `set_parameter`, `set_parameters_atomically`,
`undeclare_parameter`, `allow_undeclared_parameters`, `describe_parameter`,
`get_parameter_type`, `list_parameters`, `declare_parameter_with_descriptor`,
the description / read-only / range setters, the on-set callback pair). Each
forwards to the executor's `_on(node_id, ...)` form: the STORE stays the
executor's one table (phase-426 W1), only the receiver moved. The 17 bare
primary-node `Executor` forms are `#[deprecated]` for one release. The builder
path now creates the store on first use, as `declare_parameter_on` always did.
`DeclarativeNode`'s hidden `declare_parameter(id, name, type)` became
`declare_parameter_with_id`, because an inherent method shadows a trait one.
Executor-wide verbs (`register_parameter_services`, `params`,
`install_argv_params`, `flush_parameter_events`) stay on the executor.
**Stage 1b LANDED 2026-10-27:** the 26 node-id-keyed parameter forms
(`declare_parameter_on`, `get_parameter_on`, `parameter_on`, the `_on_ref`
pair, `argv_param_overrides_on`, ...) are crate-private inherent methods behind
the sealed `ExecutorFfi`, as R9 stage 1 did; the C and C++ shims import the
trait. The seven with no `Node` verb yet got one first
(`argv_param_overrides`, `for_each_argv_param_override`,
`set_parameter_local`, `enable_parameter_events` / `disable_parameter_events`,
`delete_parameters_on_not_set`, `set_parameters_atomically_with`), so no Rust
capability left with them. Test: `the_node_carries_the_whole_parameter_surface`.
**Stage 2 LANDED 2026-10-27** (maintainer choice "seal every method"): every
`ParameterServer` method (59) is a crate-private inherent method behind the
sealed `nros_params::ParameterServerOps` (`nros-params/src/ops.rs`), which
`nros` never re-exports; nros-node re-exports it for the shims as
`nros_node::executor::ffi::ParameterServerOps`. The type stays public
(`Executor::params()`, a standalone caller-owned store). Callers that import
the trait: nros-node, `nros`'s declarative node, both shims, and
`heap-free-poc-mps2` (which now depends on `nros-params` directly, since it
exercises a standalone store without alloc).

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
re-verdicted with a reason that names a constraint, and the issues above
are closed or re-homed.
