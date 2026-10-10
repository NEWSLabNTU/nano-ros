# Phase 487 — the C API's dispatched entities are arena handles; its publisher keeps its storage

**Status (2026-10-10). Opened.** Answers issue
[1335](../issues/1335-entity-storage-split-timer-arena-vs-caller-buffer.md)'s
last open question (its question 3: does the C API follow the C++ entity
model?). Implements the C half of
[RFC-0096](../design/0096-cpp-freestanding-core-and-porting-layer.md) D9 and
follows [phase-456](archived/phase-456-cpp-api-is-a-handle-over-the-rust-arena.md), which did the C++ half.

## Decision taken when this phase was opened (2026-10-10, maintainer)

**C follows the C++ rule.** Upstream's rule is "an entity the executor
dispatches is owned by the executor", and C++ already applies it:

| entity | C++ today (phase-456) | C after this phase |
| --- | --- | --- |
| subscription, service server, service client, timer, action server/client | a handle into the executor arena | the same: the struct holds a handle, not the entity |
| publisher | caller storage (RFC-0096 D5 item 4: nothing dispatches it) | unchanged: caller storage |

The C user still DECLARES the struct (`nros_subscription_t sub;`), which is the
`rcl` / `rclc` idiom: upstream's `rcl_subscription_t` is also user-declared and
holds a pointer to an implementation. What shrinks is the struct's CONTENT.
Caller-DECLARED is kept; caller-STORAGE goes, except for the publisher.

Rejected alternatives, recorded so they are not reopened by accident:

- **Keep the C shape and close 1335 as `wontfix`.** Then C stays a third shape
  beside the arena and the C++ handle, and a dispatched C subscription keeps
  three copies of its identity (below).
- **Make the publisher an arena handle too.** A C publisher would then need an
  executor to exist, and the arena would have to be sized for publishers.
  Phase-456 refused the same move for the C++ `Publisher`.

## Where the C API stands (read 2026-10-10, `origin/main`)

`packages/api/nros-c/include/nros/nros_generated.h`:

- `nros_subscription_t` holds the topic, type name and type hash as inline
  byte arrays plus their lengths, the `callback` and `context`, the node ref,
  the QoS, the scheduling context, a `handle_id` plus `_executor` (an arena
  handle), AND `uint64_t _opaque[SUBSCRIPTION_OPAQUE_U64S]` (caller storage for
  the RMW subscriber). After `nros_executor_add_subscription*`, the arena's
  `SubBufferedRawCEntry` holds the subscriber, the callback and the context,
  so the struct's copies of all three are unused on that path. This is the
  "three copies" shape phase-456 removed from C++.
- `nros_service_t` and `nros_client_t` have the same layout: names, callback,
  context, QoS, an `_internal` record and `_opaque[...]`.
- `nros_timer_t` is close to the target already (`handle_id`, `_executor`),
  but still carries `period_ns`, `last_call_time_ns`, `callback` and
  `context`, which the arena's timer entry also holds.
- `nros_publisher_t` holds the names and `_opaque[PUBLISHER_OPAQUE_U64S]`.
  Unchanged by this phase.

**The complication this phase must settle in W1: C has POLLING forms too.**
`nros_subscription_init_polling*`, `nros_service_init_polling` and
`nros_client_init_polling` create an entity the caller drives itself, with no
executor. A polled entity has no arena slot, so it needs its storage. Two
shapes are possible:

1. **Separate polled types** (`nros_polling_subscription_t`, …) that keep
   `_opaque`, and dispatched types that become handles. This matches the C++
   and Rust split (phase-483 W2 named Rust's polled constructors
   `create_polling_*`) and gives each type one meaning.
2. **One type whose storage is used only by the polled form.** Smaller diff,
   but it keeps exactly the dead storage this phase exists to remove.

The recommendation is shape 1, decided in W1 against the measured costs below.

## Work items

### W0 — measure before moving anything

The bytes do not disappear: they move from the C struct into the executor
arena. A before/after that counts the arena growth without subtracting what
the struct gave up reads as a regression; issues 1145 and 1171 record that
trap for the executor backing.

- `sizeof` of each C entity type, on the three toolchain arms
  `check-cpp-capability-layout` already measures for C++.
- `just mem-report --baseline` on one real C image with a subscription, a
  service and a timer (for example `examples/native/c/listener` plus a
  FreeRTOS twin), before and after. The acceptance number is the NET change.

### W1 — subscriptions

- Decide the polling shape (above) and write it into RFC-0096 D9 as the C
  amendment.
- The dispatched `nros_subscription_t` keeps `{state, handle_id, _executor}`
  plus whatever the C API must still answer without the arena (W1 lists it,
  for example `nros_subscription_get_topic_name`, read through the handle if
  it can be).
- `nros_executor_add_subscription*` takes the callback and the context and
  owns them. The struct no longer stores either.

### W2 — services and clients

The same change for `nros_service_t` and `nros_client_t`.

### W3 — timers

`nros_timer_t` drops its copies of the period, the last-call time, the
callback and the context.

### W4 — actions

Check the C action server and client against the same rule. On the C++ side
actions were already furthest along (issue 1335's measured correction).

### W5 — consumers, docs and ledger

- Every C example, template, workspace and test bin that reaches the removed
  fields. Sweep with `git grep -nE "\.(callback|context|_opaque|period_ns)\b" -- '*.c' '*.h'`
  and record the result in the commit.
- The C++ FFI mirrors these structs (`check-ffi-struct-mirrors`). Run it, and
  regenerate the cbindgen headers through `cargo run -p nros-cbindgen-headers`.
- The book's C chapters, the api-parity ledger's C rows, and a changelog
  fragment. This is an ABI change for every C user.
- Close issue 1335.

## Acceptance

- Every C entity the executor dispatches is an arena handle in its struct, and
  a C publisher holds its own storage, so C and C++ follow one rule.
- `just mem-report --baseline` on the W0 images shows the NET change, with the
  arena growth and the struct shrinkage both stated.
- `just ci gate`, `just check c`, `just check cpp` and the C fixtures of tier 1
  are green, and the C cells of tier 2 run.
