# Phase 487 — the C API's dispatched entities are arena handles; its publisher keeps its storage

**Status (2026-10-11). Done; archived.** W1 is #1920; W2 to W5 follow it in one pull request. Issue 1335 is resolved. Answers issue
[1335](../../issues/archived/1335-entity-storage-split-timer-arena-vs-caller-buffer.md)'s
last open question (its question 3: does the C API follow the C++ entity
model?). Implements the C half of
[RFC-0096](../../design/0096-cpp-freestanding-core-and-porting-layer.md) D9 and
follows [phase-456](phase-456-cpp-api-is-a-handle-over-the-rust-arena.md), which did the C++ half.

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

**Status: done 2026-10-11.** `sizeof`, read from the symbol size of a
`char[sizeof(T)]` per type (`nm -S`), so one probe TU measures a cross target
without running it. Host arm: `nros-c` with the shipped C features
(`std,rmw-cffi,platform-posix,ros-humble`). Cortex-M3 arm: the same crate for
`thumbv7m-none-eabi` with `rmw-cffi,ros-humble`, whose build script emits the
per-target opaque sizes. Before:

| type | x86_64 | thumbv7m |
| --- | ---: | ---: |
| `nros_subscription_t` | 2 536 | 2 368 |
| `nros_service_t` | 2 368 | 2 312 |
| `nros_client_t` | 2 376 | 2 312 |
| `nros_timer_t` | 72 | 48 |
| `nros_action_server_t` | 7 592 | 7 312 |
| `nros_action_client_t` | 6 296 | 6 136 |
| `nros_publisher_t` (unchanged by design) | 1 312 | 1 264 |
| `nros_executor_t` | 90 928 | 89 240 |

**The arena does not grow, which removes the trap W0 was written against.**
`nros_executor_t`'s arena is a fixed array (`EXECUTOR_OPAQUE_U64S`), and on
the dispatched road it ALREADY held each entity: `nros_executor_add_*`
creates the subscriber inside it, so the struct's `_opaque` was a second,
unused copy. The NET change for a dispatched entity is the struct shrinkage
alone, and the per-W rows below state it. The `mem-report` half of W0 is
taken in W5 on the listener images, where the entities are `.bss` statics.

### W1 — subscriptions

- Decide the polling shape (above) and write it into RFC-0096 D9 as the C
  amendment.
- The dispatched `nros_subscription_t` keeps `{state, handle_id, _executor}`
  plus whatever the C API must still answer without the arena (W1 lists it,
  for example `nros_subscription_get_topic_name`, read through the handle if
  it can be).
- `nros_executor_add_subscription*` takes the callback and the context and
  owns them. The struct no longer stores either.

**Status: done 2026-10-11.** Shape 1. The dispatched `nros_subscription_t`
keeps the arena handle plus the registration description (topic, type, hash,
QoS, node, scheduling context): rclc's order is init first and
`rclc_executor_add_*` second, so the description has nowhere else to wait.
`callback`, `context` and `_opaque` are gone.

- `nros_subscription_init_with_qos` / `_with_options` lost `callback` and
  `context` (now rclc's `rclc_subscription_init` shape plus our name).
- `nros_executor_add_subscription(exec, sub, invocation)` is deleted: it
  dispatched the stored callback, and nothing in the tree called it.
  `nros_executor_add_subscription_raw` registers through the shared
  `register_subscription_entry` with its own callback; `_in_group` gained
  `callback` and `context`.
- New `nros_polling_subscription_t` (state, node, `_opaque`) with
  `nros_polling_subscription_{init,init_with_qos,take_serialized,
  take_sequence,take_validated,borrow,release,set_wake_callback,
  get_actual_qos,fini}` and `nros_get_zero_initialized_polling_subscription`.
  The polled init reads the names into locals: the polled struct keeps the
  subscriber, not the description.
- RFC-0096 D9 gained "The C half", which records the rule and shape 1.

| type | x86_64 | thumbv7m |
| --- | ---: | ---: |
| `nros_subscription_t` | 2 536 → **744** | 2 368 → **700** |
| `nros_polling_subscription_t` (new) | 1 792 | 1 672 |

Consumers: `examples/native/c/safety-listener` (the one polled C example) and
four C test TUs; the api-parity ledger renamed eight rows and recorded the
removed name as rclc's theirs-only `executor_add_subscription`. Sweep:
`git grep -nE "nros_subscription_(init_polling|take_|borrow|release|set_wake)|nros_executor_add_subscription\(" -- ':!docs'`
returns nothing outside history.

### W2 — services and clients

The same change for `nros_service_t` and `nros_client_t`.

**Status: done 2026-10-11.** Shape 1 again, plus the one thing only the
client had.

- `nros_service_t` and `nros_client_t` lost `callback` / `response_callback`,
  `context` and `_opaque`, and their `POLLING` states.
- `nros_service_init_with_qos` / `_with_options` lost `callback` and
  `context`. The two-argument `nros_executor_add_service` is deleted;
  `nros_executor_add_service_raw` holds the registration body and takes the
  callback.
- **The client's arena entry held a trampoline whose context was the
  `nros_client_t`**, reading `response_callback` back out of the struct at
  dispatch, so `nros_client_call` could swap a one-shot capture in. That is
  RFC-0096 D9's back-reference, and issue 1631's use-after-free when a reply
  outlives the struct. The entry now holds the user's pair:
  `nros_executor_add_client` registers it empty,
  `nros_client_set_response_callback` writes it (the client must be added
  first, else `NROS_RET_NOT_INIT`; every in-tree caller already was), and
  `nros_client_call` swaps through the same slot.
- New `nros_polling_service_t` / `nros_polling_client_t` with
  `nros_polling_{service,client}_*`: init, the `_raw` take/send pair,
  set_wake_callback, fini, the two granted-QoS getters each, and a
  zero-initialiser.
- **Codegen: the typed service binding moved to registration.**
  `<Srv>_service_init(service, node, name)` no longer takes the handler; the
  new `<Srv>_executor_add_service(executor, service, &handler)` binds the
  trampoline through `nros_executor_add_service_raw`, which is where rclc
  binds its handler. A version-10 or -11 C tree with a service no longer
  compiles, so the codegen version and its minimum are 12.

| type | x86_64 | thumbv7m |
| --- | ---: | ---: |
| `nros_service_t` | 2 368 → **736** | 2 312 → **700** |
| `nros_client_t` | 2 376 → **744** | 2 312 → **704** |
| `nros_polling_service_t` (new) | 1 632 | 1 616 |
| `nros_polling_client_t` (new) | 1 632 | 1 616 |

Consumers: the six `service-server` examples (native, FreeRTOS, bare-metal
mps2, NuttX, ThreadX riscv64, ThreadX Linux), five C test TUs, the codegen
heap compile-check stub, the golden corpus, `rcl_compat.h` and the book. The
`typed_service_storage_mismatch_probe` still fails on both of its mistakes,
the second one now at `_executor_add_service` (checked in its log).

### W3 — timers

`nros_timer_t` drops its copies of the period, the last-call time, the
callback and the context.

**Status: done 2026-10-11, and smaller than written.** The item assumed
the period, callback and context were copies of the arena's. Read against the
code they are not: rclc's `rclc_timer_init_default(timer, support, period,
callback)` takes them at INIT, before any executor exists, and
`nros_executor_remove_timer` returns a timer that can be added again, so the
struct is the only place those inputs can wait. They are the timer's
description, the same role a subscription's topic plays. `nros_timer_init`
keeps its rclc arity, which `<nros/rcl_compat.h>` already records as a
decision.

The one real copy was `last_call_time_ns`: written by init and
`rcl_timer_reset`, read by nothing, and stale for any timer the executor
ran (issue 1049, whose fix already moved every reader to the arena). It is
removed.

| type | x86_64 | thumbv7m |
| --- | ---: | ---: |
| `nros_timer_t` | 72 → **64** | 48 → **40** |

### W4 — actions

Check the C action server and client against the same rule. On the C++ side
actions were already furthest along (issue 1335's measured correction).

**Status: done 2026-10-11.** C follows the C++ rule for actions too, and the
C++ rule for actions is NOT "the arena owns the callbacks": phase-456 W3b-ii
measured and refused that move for the C++ action server, because the arena's
trampolines translate the callback ABI (an `int32_t` goal verdict against the
core's `i8`, a two- against a three-argument cancel callback) and reframe the
goal payload. So the dispatched action struct keeps one callback record, read
by the trampolines with the struct as context, beside its arena handle.

- **Polled forms split**, as W1 and W2: `nros_polling_action_server_t` /
  `nros_polling_action_client_t`, with the `_raw` families, the wake
  callbacks, init and fini renamed to `nros_polling_action_{server,client}_*`.
  The server's active-goal count, goal-exists and expire-goals had a polled
  arm each; they got polled twins.
- **The server held its callback record twice.** `ActionServerInternal`
  carried `c_goal_callback`, `c_cancel_callback`, `c_accepted_callback`,
  `c_context` and a `server_ptr` back to the struct containing it, and the
  trampolines took `&_internal` as context. It is `{handle, executor_ptr}`
  now; the trampolines take the server and read its one record. The layout
  mirror in `nros::sizes` and the buildless `NROS_ACTION_SERVER_INTERNAL_SIZE`
  follow (96 → 72).

| type | x86_64 | thumbv7m |
| --- | ---: | ---: |
| `nros_action_server_t` | 7 592 → **824** | 7 312 → **752** |
| `nros_action_client_t` | 6 296 → **736** | 6 136 → **692** |
| `nros_polling_action_server_t` (new) | 6 744 | 6 560 |
| `nros_polling_action_client_t` (new) | 5 576 | 5 456 |
| `nros_executor_t` (control) | 90 928, unchanged | 89 240, unchanged |

Consumers: the `action-raw-goal-probe` test program (the one polled C action
in the tree), `action.h`, the book's C API page, three comments.

### W5 — consumers, docs and ledger

- Every C example, template, workspace and test bin that reaches the removed
  fields. Sweep with `git grep -nE "\.(callback|context|_opaque|period_ns)\b" -- '*.c' '*.h'`
  and record the result in the commit.
- The C++ FFI mirrors these structs (`check-ffi-struct-mirrors`). Run it, and
  regenerate the cbindgen headers through `cargo run -p nros-cbindgen-headers`.
- The book's C chapters, the api-parity ledger's C rows, and a changelog
  fragment. This is an ABI change for every C user.
- Close issue 1335.

**Status: done 2026-10-11.** Each work item swept its own consumers in the
same commit, so W5 is the image measurement, the runtime cells and the
closure.

- **Consumers.** The item's sweep, `git grep -nE "\.(callback|context|_opaque|period_ns)\b" -- '*.c' '*.h'`,
  widened with `response_callback` and `last_call_time_ns`, hits only the
  zenoh-pico shim's own tables and a C++ header comment; no C entity consumer
  is left. Every example, test program and C test TU that used a changed name
  was ported in its W's commit.
- **FFI mirrors, headers, ledger, book, changelog.** `check-ffi-struct-mirrors`
  and the cbindgen staleness check pass (both on the fast line); the ledger,
  book pages and four `changelog.d` fragments moved with each W.
- **The image, measured.** `just mem-report --baseline` on
  `examples/mps2-an385-freertos/c/listener`, the FreeRTOS C listener: the
  BEFORE image is the one this checkout built on 2026-10-10 from an earlier
  `main`, kept before the rebuild; the AFTER image is this phase's. Section
  RAM −1 696 bytes, and the only symbol that moved is `app` (the `.bss` struct
  holding the support, node, subscription and executor): 92 952 → 91 264,
  −1 688. The subscription's `sizeof` fell 1 668 on Cortex-M3; the other
  20 bytes are from commits between the two `main`s, not from this phase.
  **No arena or heap symbol grew** (`ucHeap`, `g_sessions` and every zenoh
  shim pool read `=`), which is W0's finding measured on a real image: the
  executor arena already held the subscriber, so nothing moved INTO it. On the
  native listener `app` is 92 920 bytes, the 744-byte subscription beside a
  90 928-byte executor.
- **Runtime cells**, on fixtures rebuilt from this tree:
  `native_example_reqresp` C service and action cells on zenoh and Cyclone;
  `test_c_action_communication`; the C Cyclone client-callback test (the W2
  setter path); `action_raw_goal_e2e` (the one polled C action);
  `test_c_safety_listener_validates_crc` (the one polled C subscription); and
  the FreeRTOS QEMU `test_rtos_{pubsub,service,action}_e2e` C cells.
  All pass.
- Issue 1335 is resolved and archived.

Two things found and left as they are, because neither is this phase's:
`c_heap_compile_check`'s three `#[ignore]`d cases fail on a missing
`nros/nros_config_generated.h` include path before reaching any code this
phase touched, and the C workspace's FreeRTOS build trees were configured
with an older pinned ARM toolchain (`13.2-nros1`) than the store now resolves
(`13.2-nros5`), which `NanoRosCrossToolchain.cmake` refuses by design.

## Acceptance

- Every C entity the executor dispatches is an arena handle in its struct, and
  a C publisher holds its own storage, so C and C++ follow one rule.
- `just mem-report --baseline` on the W0 images shows the NET change, with the
  arena growth and the struct shrinkage both stated.
- `just ci gate`, `just check c`, `just check cpp` and the C fixtures of tier 1
  are green, and the C cells of tier 2 run.

**Met 2026-10-11.** Every dispatched C entity is an arena handle in its
struct and the publisher keeps its storage (W1–W4). The image measurement
above states the net change with the arena side included. `just ci gate`,
`just check c` and `just check cpp` pass; the tier-1 C fixtures
(native zenoh and Cyclone) and the FreeRTOS tier-2 C cells run green.
