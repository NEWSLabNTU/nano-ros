# Phase 492 — The C API drops `nros_`: upstream names, `rclc_` for the rest

**Status (2026-10-12). In progress — W1, W2, W3, W5, W6, W7 and W8 landed, W4 part 1 landed; W4 part 2 (C components onto the C API) open.** Implements the
[RFC-0089](../design/0089-ros2-api-adoption-and-the-compile-or-conform-rule.md)
amendment "`nros_` is phased out of the C API; ours-only names take `rclc_`"
(2026-10-12). The C counterpart of the C++ `nros::` → `rclcpp::` flip
(phase-427/428). Runs beside [phase-491](archived/phase-491-c-parameter-server.md),
whose parameter surface already follows the rule.

## Why

A ported rclc file should compile unchanged, and a nano-ros C user should
learn one vocabulary. Today the C user API is mostly `nros_`: 468 ours-only
items, 151 with an upstream counterpart, of which 77 already carry upstream's
name natively and at least six also keep an `nros_` twin
(`scripts/api-parity.py --lang c`, rclc + rcl humble, at `d457eed61e`).

## Work items

**W1 — The rename map and its gates.**
Generate the map from the parity report and the ledger, one row per public C
name in the `nros-c` headers: upstream spelling (ADOPT / ADOPT-BOUNDED), a
distinct `rclc_` spelling (a `differs` row that does not conform), or
`rclc_<name>` (ours-only). Commit it as a reviewed table. Add a gate that
counts `nros_` names in the public C headers, as a ratchet that ends at zero,
and extend the parity tripwire to ours-only header names under
`include/rclc/`.
*Acceptance:* every one of the 619 items has exactly one target name; no target
collides with a name in `docs/reference/api-surface/rclc.json` unless the row is
an adoption; the 18 `differs` rows each carry a conform-or-rename decision.

**W2 — Upstream header layout.**
Declare the surface under `include/{rcl,rcl_action,rcl_lifecycle,rclc,rclc_lifecycle,rclc_parameter}/`,
using upstream's file names where upstream has the file. Ours-only families get
`include/rclc/<family>.h`. Move the return-code mapping and the documented
refusals out of `rcl_compat.h` into `<rcl/types.h>` and the family headers.
Every umbrella keeps the `#error` against upstream's real `rcl` headers.
*Acceptance:* `packages/api/nros-c/tests/compile` builds a TU that includes only
upstream paths; a TU that includes both ours and a stub of upstream's
`<rcl/types.h>` fails to compile with the named message.

**W3 — Rename the symbols, types and constants.**
In `nros-c`: `#[no_mangle]` names, the cbindgen configuration, opaque-size
constants, `nros_ret_t`/`NROS_RET_*` → `rcl_ret_t`/`RCL_RET_*` (our values),
`NROS_PARAMETER_*` and other user-visible constants. The `nros_` twins of
already-adopted names are deleted. Build knobs (`NROS_MAX_*`, `CONFIG_NROS_*`)
do not change.
*Acceptance:* the W1 ratchet reads zero for `packages/api/nros-c/include`;
`check-ffi-struct-mirrors`, `check-abi-bindings` and the opaque-size asserts are
green.

**W4 — C components stop calling `nros_cpp_*`.**
A C component gets an `rcl_node_t *` and an `rclc_executor_t *` from its
context and uses the C API, so `nros_cpp_*` becomes C++-private. The codegen
that emits C entries (`tests_c.rs`, `builder/west_app.rs`, the entry emitters
calling `nros_app_main` and `nros_boot_config_*`) emits the new names.
*Acceptance:* no C component calls `nros_cpp_*` — `git grep 'nros_cpp_' --
'examples/**/*.c'` is empty — and `component.h` declares no C++-FFI type or call
beyond the opaque node handle and the bind seam its macro uses. (As first
written this said "outside `nros-cpp`'s own tests", which also counted the
board runners that HOST the executor — `nros_cpp_init` / `_spin_once` / the tier
runners. Those are the runtime's host seam, not a component, and stay.)
`c_param_talker_pkg` and the other C component packages build and pass their
cells.

**W5 — Migrate every caller** (after W7 and W8).
Examples (about 160 files), `packages/testing` (about 70), `packages/cli` (about
80), the book, `docs/reference/c-api-cmake.md`, the C getting-started page
(which states that the drop-in is one-directional), and CLAUDE.md's Naming
line ("nros — code shorthand (crates, Rust/C idents)" no longer covers C API
identifiers). No forwarders.
*Acceptance:* `just ci gate` green; `just ci matrix` green (the change touches
codegen); `git grep -E '\bnros_[a-z_0-9]+\(' -- '*.c'` finds only seam
implementations.

**W6 — Ledger.**
Re-key the C rows of `docs/reference/api-parity-ledger/*.json` to the new
names. Every ours-only row keeps `verdict: extension` and says the name is ours
inside upstream's prefix.
*Acceptance:* `just check api-parity` green with `--require-disposition`.

**W7 — Messages in rosidl's names and shape.**
The C pack of `rosidl-codegen` emits RFC-0089's "Messages take rosidl's C names
AND its member shape": rosidl struct, header and constant names;
`rosidl_runtime_c__String` and `__Sequence` members; `__Storage` +
`__init_in`; no allocating rosidl functions; the RFC-0033 `heap` arm removed
from the C emitter. The runtime types (`rosidl_runtime_c__String`, the primitive
sequences) ship once in `nros-c` under `include/rosidl_runtime_c/`. Lands
before W5 so every example moves once.
*Acceptance:* a TU written against micro-ROS's manual-buffer idiom for
`std_msgs/String` and `sensor_msgs/LaserScan` compiles unchanged; a TU calling
`std_msgs__msg__String__init` fails to compile with the message naming
`__init_in`; deserialising a string longer than its `capacity` returns an error
and writes nothing past it; a counting allocator sees zero allocations across
`__init_in`, serialise and deserialise; the RFC-0033 cross-language capacity
tests still pass for Rust and C++.

**W8 — Type support through the descriptor.**
RFC-0089 "`ROSIDL_GET_*_TYPE_SUPPORT` adopted as a pass-through": the
descriptor gains `serialize` / `deserialize`; the C pack emits
`<pkg>__<sub>__<Name>__nros_type`; `<rosidl_runtime_c/*_type_support_struct.h>`
define the three macros; publishers and subscriptions keep the descriptor;
`rcl_publish` / `rcl_take` exist and serialise through it into the carved
transmit buffer; `rclc_executor_add_subscription_with_context` loses its extra
argument; the typed helpers retire. Lands with W7.
*Acceptance:* the micro-ROS-style talker and listener in the RFC compile and
exchange messages with a ROS 2 peer unchanged; each refused line in the RFC is a
must-not-compile verdict row (`*-verdict`, issue 1620); a counting allocator
sees zero allocations across publish and take.

## Out of scope

* The port seams (RMW, platform, board C ABIs), CMake function and target
  names, and build knobs (RFC-0089 amendment, "What does NOT move").
* The C++ API, already on `rclcpp::`.

## Open regressions

* [issue 1876](../issues/1876-nuttx-arm-c-images-never-reach-readiness.md):
  NuttX-ARM C images fail `rclc_subscription_init_default` with
  `RCL_RET_INVALID_ARGUMENT` after the rename, so four tier-2 cells never reach
  readiness.

## Progress

### 2026-10-12 — W1, W3, W5, W6: the rename

- **W1.** `scripts/gen-c-rename-map.py` -> `docs/reference/c-api-rename-map.tsv`.
  It has one row per public `nros_`/`NROS_` name, classified as:
  - adopt (43), alias (10), ours (599), ret (18);
  - kept: seam (60), hook/entry (46), build (107).

  `check-c-api-nros-names` (fast line) holds every public name to the map. The
  count left to rename is a ratchet, now at **0**. The map is a RECORD of the
  rename: regenerating it on the renamed tree yields only the kept names.
- **W3/W5/W6.** 670 names renamed across Rust, headers, examples, codegen,
  tests, scripts, book, `docs/reference` and the ledger, with no forwarders.
  Decision records keep their wording. `NROS_CODEGEN_VERSION`/`_MIN` -> 13.
  Seven gates whose patterns named the old spellings now read the new ones.
- Measured deviations from the plan:
  - the five lifecycle `differs` verbs became
    `rclc_lifecycle_state_machine_*` (their rclc spelling is upstream's with a
    different contract);
  - the entry and boot conventions (`nros_app_main`, `NROS_APP_*`,
    `nros_app_config_t`, boot config, tier task memory) KEEP `nros_`, as link
    contracts with board startup code, like the port seams;
  - the C component API's names (`nros_c_*`) became `rclc_c_*` mechanically
    and are reworked by W4.

### 2026-10-12 — W2: upstream include paths

- 51 headers at upstream's paths (`<rcl/*.h>`, `<rclc/*.h>`,
  `<rcl_action/*.h>`, `<rcl_lifecycle/*.h>`, `<rclc_lifecycle/rclc_lifecycle.h>`),
  taken from the recorded rclc+rcl surface. Each brings in the whole C API
  and `<nros/rcl_compat.h>`. They use their own include guards, so the refusal
  beside real rcl headers still fires. Probe:
  `packages/api/nros-c/tests/compile/upstream_include_paths.c` in `just check c`.
- Deviation: `<nros/*.h>` is NOT retired. Those files are where the
  declarations, the entry hooks and the build facts live; the upstream paths
  are the public entry points over them, and one umbrella keeps any
  declaration from having two spellings. Moving the examples' own includes to
  the upstream paths is cosmetic and is left for W5's tail.

### 2026-10-12 — W4 (part 1): one executor object for both bindings

- Design. A C component is hosted by the generated entry, which runs on the
  C++ binding's context (`nros_cpp_init`, scheduling, monitors, remaps, QoS
  overrides — all host services). A view `rclc_executor_t` over that context
  was rejected: the struct carries the executor's inline backing (`_opaque`,
  ~96 KiB at default sizing), so a view per component would waste that much
  `.bss` each on an RTOS part. Instead the C++ context EMBEDS a real
  `rclc_executor_t` and opens its executor in that struct's `_opaque`, exactly
  where `rclc_executor_init` puts one (`nros_node::ExecutorInlineStorage`).
  One executor object, whichever language hosts it.
- `nros-c`: `rclc_executor_t._host` (NULL for `rclc_executor_init`; set by
  `host_executor_init` for a host-owned executor, whose `fini` then drops
  nothing). `get_executor` takes the executor, not its `_opaque`.
  `bind_host_node` fills an executor-bound `rcl_node_t` for a node the host
  created.
- `nros-cpp`: `CppContext { tag, executor: &'static mut Executor, domain_id,
  diag, c: rclc_executor_t }`; `c.in_dispatch` is the one reentrancy guard for
  both bindings. Seam `nros_cpp_node_bind_c(node, rcl_node_t*,
  rclc_executor_t**)`, which the component macro will call. Size estimate in
  `nros-build-helpers/src/cpp.rs` adds the C header (128 + 8 per handle).
- Test: `a_c_component_gets_the_hosts_executor_and_node` (census lane) — the
  C face sees the host's registrations, and its `fini` leaves the host running.
- Also fixed: W3 left the C pack's paste prefix `nros_cdr_{{ … }}` unrenamed,
  so generated C called `nros_cdr_*`, which no longer exists.
- Next (part 2, after W7/W8, so each component is ported once onto the new
  message API): `RCLC_C_COMPONENT`'s `configure` takes `(rcl_node_t *,
  rclc_executor_t *, StructT *)`; the 37 component sources move to the C API;
  `component.h` stops declaring `nros_cpp_*`.

### 2026-10-12 — W7, W8: messages in rosidl's names and shape, type support through the descriptor

- **W7 (C pack).** Struct `<pkg>__msg__<Name>` (`_Request`/`_Response`,
  `_Goal`/`_Result`/`_Feedback`) in `<pkg>/<kind>/<snake>.h`. The file name
  follows rosidl's `convert_camel_case_to_lower_case_underscore`
  (`utils::to_rosidl_snake_case`; `UInt8` -> `u_int8`). The CMake output
  predictor in `NanoRosCodegenCore.cmake` uses the same two regexes. Guards are
  `<PKG>__<KIND>__<SNAKE>_H_` and constants `<Msg>__<CONST>`.
  - Members use rosidl's shape: `rosidl_runtime_c__String` and the `__Sequence`
    types, with fixed arrays and nested messages inline.
  - Each type gets a generated `__Storage` sized by RFC-0033's capacities, and
    `__init_in(&msg, &storage)` wires every string and sequence to it,
    recursing into nested messages, sequence elements and array elements.
  - Deserialize reads into the caller's `capacity` and returns -1 before writing
    a byte of a field that does not fit. `.msg` bounds are checked in both
    directions.
  - rosidl's allocating `__init` / `__fini` / `__create` / `__destroy` and the
    `__Sequence__*` functions are function-like macros that expand to an
    undeclared identifier naming `__init_in`. The String runtime's allocating
    functions get the same treatment.
  - The C `heap` arm is gone: a `heap` entry only sizes `__Storage`.
  - `view` keeps `<Msg>__View` and `__deserialize_view`. Its non-view members
    read through caller-wired storage, and the view is not zeroed.
  - Message, service and action share one set of payload macros
    (`packs/c/_field.jinja`).
- **Runtime types.** `nros-c/include/rosidl_runtime_c/` holds `string.h`,
  `primitives_sequence.h` and the three `*_type_support_struct.h`, each under
  upstream's include guard, so they are the one definition site.
  `rclc_parameter.h` now includes them instead of defining its own copies.
- **W8.**
  - `rclc_message_type_t` gains `serialize` and `deserialize`. The service
    descriptor gains request/response codec slots and the action descriptor
    goal/result/feedback slots.
  - The pack emits `<pkg>__<sub>__<Name>__nros_type` with type-erased codec
    functions (real functions, never casts).
  - `ROSIDL_GET_{MSG,SRV,ACTION}_TYPE_SUPPORT(P, S, N)` expands to
    `(&P##__##S##__##N##__nros_type)`.
  - Publishers and subscriptions keep their descriptor (`type_support`).
  - `rcl_publish(&pub, &msg, NULL)` serialises through the descriptor into a
    stack buffer bounded at build time. The buffer is 256 B when the type's
    bound fits, and otherwise `MESSAGE_BUFFER_SIZE`, the image's message
    buffer. It retries in that larger class when the sample does not fit the
    smaller one.
  - `rclc_executor_add_subscription_with_context` takes rclc's six arguments.
    The deserializer and the receive hint come from the descriptor.
  - `rmw_{publisher,subscription}_allocation_t` are declared incomplete.
  - `rclc_executor_add_subscription_typed{,_sized}` and every per-message typed
    helper are retired. nros-node's erased deserializer now takes
    `(buf, len, msg)`.
- **Tests.**
  - `check c`:
    - `rosidl_manual_buffer_idiom.c`: the micro-ROS idiom for `std_msgs/String`
      and `sensor_msgs/LaserScan` compiles unchanged with `-Werror`.
    - `rosidl_talker_listener.c`: a talker and listener using
      `ROSIDL_GET_MSG_TYPE_SUPPORT`, `rcl_publish` and `_with_context`.
    - Every C golden compiles with `-Werror`.
    - `rosidl_message_no_alloc` (run): a string or sequence past its capacity
      is refused and the guard bytes are untouched. A counting allocator sees 0
      allocations across `__init_in`, serialize, deserialize and `rcl_publish`,
      and a control shows the counter is live. `rcl_publish` puts the
      serialiser's exact bytes on the stub backend. A non-NULL allocation, a
      codec-less descriptor and an oversized message are refused.
  - The must-not-compile verdicts are four `cxx-syntax-verdict` rows
    (`c_rosidl_*`, consumer `tests/c_rosidl_refusals.rs`). The builder now
    compiles a `.c` snippet as C11. The rows cover:
    - a `rosidl_message_type_support_t` declaration;
    - `->typesupport_identifier`;
    - a stack `rmw_publisher_allocation_t`;
    - `std_msgs__msg__String__init`, whose diagnostic names `__init_in`.
  - The fingerprint corpus gained stock Humble `String`, `Header`, `Time` and
    `LaserScan` under their real package names. Its C goldens are now laid out
    the way rosidl lays them out.
  - The `#[ignore]`d C heap compile check retired with the C heap arm.
- **Runtime, on this host:**
  - Native C fixtures (zenoh, cyclonedds) and C++/Rust peers built.
  - `native_api` C/interop cells and `native_example_pubsub` (c zenoh, c
    cyclone): 15/15.
  - `interop_e2e` cyclone cells against ROS 2 Humble `rmw_cyclonedds_cpp`
    (C talker to ROS 2, ROS 2 to C listener, C service server): 3/3.
  - `parameters_roundtrip` 2/2, `c_rosidl_refusals` 4/4.
- `repr-memory-agreement` is retargeted. It used to compile the C pack's
  header as a stand-in for the C++ header. After W7 the C pack is rosidl's
  shape, its memory partner is `rosidl_generator_c`, and C and C++ nodes
  exchange CDR rather than memory. The gate now pairs the memory that IS
  shared: the C++ pack's `.hpp` structs against their Rust `repr(C)` glue.
  - Both sides are compiled for armv7a (`arm-none-eabi-g++` with the nros-cpp
    include set, against a two-macro stand-in for the config header), then the
    symbol sizes are compared: size, alignment, every field offset, and every
    container member offset.
  - Pairs come from a name map derived from the corpus. Every other Rust struct
    must be a container a pair embeds, or a srv/action `_t_view`, for which the
    C++ pack emits no type.
  - Result: 28 pairs, 274 probes, 3 storage arms. The self-test also checks the
    pairing rule. A mutation (HeapString `size`/`capacity` swapped) is reported
    at both affected fields.
  - The first standalone compile of the C++ pack's headers found two
    pre-existing C++ pack defects, both now fixed:
    - a nested message of a hyphenated package was spelled `pkg-name::msg::T`;
    - service and action headers did not include `heap_sequence.hpp` /
      `heap_string.hpp`.
- Codegen version 14 (MIN 14). `.config/codegen-version-surface.txt` was
  re-baselined. Ledger: the two typed rows were deleted. `c:publish` is
  `adopt-bounded`, its envelope being that `allocation` must be NULL.
  `c:polling_subscription_take` and `c:message_serialize_fn_t` were added.
- Deviations, measured or decided:
  - **`rcl_take` is not declared.** `rcl_subscription_t` is executor-owned
    (phase-487 W1) and has no subscriber of its own to take from, so rcl's name
    with this contract would compile and differ. The analog is
    `rclc_polling_subscription_take(&sub, &msg, NULL)`.
  - **Action envelopes are not emitted.** The C pack emits Goal, Result and
    Feedback. `_SendGoal_Request` and the other envelopes are assembled by the
    action runtime.
  - **`wstring` maps to `rosidl_runtime_c__String`**, not `U16String`. The wire
    behaviour is unchanged from before.
  - **Generated service helpers are kept, renamed** (`<Srv>__service_init`,
    `__executor_add_service`, `__service_handler_t`, the client half). Their
    handler structs carry the payloads' `__Storage`. rclc's
    `rclc_executor_add_service` stays refused, as before.
  - **The talker/listener probe is not literal micro-ROS code.** It uses
    nano-ros's `rclc_support_init` / `rclc_executor_init` and the
    `ON_NEW_DATA` enumerator. rclc's support/executor init
    signatures and its bare `ON_NEW_DATA` are a separate question, outside W8.
  - **The type-check on a subscription's `msg` argument is gone with the
    retired macros.** The macros checked its type; rclc's `void *` does not.
    `typed_subscription_storage_mismatch_probe.c` was deleted, matching rclc.
  - **Hand-built descriptors in examples still compile with NULL codec slots.**
    `rcl_publish` refuses them with `RCL_RET_UNSUPPORTED`.
  - **A fixture-environment trap.** A linked worktree inherits
    `nano_ros_ROOT` pointing at the main checkout. That makes `find_package`
    configure fixture build dirs against the MAIN checkout's CMake, which
    generated the old file names. Issue 1280's class, but through
    `nano_ros_ROOT`, which the re-root helper does not cover. The fixtures here
    were built with `nano_ros_ROOT` set to the worktree.


### 2026-10-12 — W1.b: a name upstream has takes upstream's contract

- Found while designing W4 part 2: W1's map adopted every parity
  `systematic` row, and `systematic` is a SPELLING match. Sixteen functions
  and callback types landed on rcl/rclc names over a different arity or
  callback contract, which RFC-0089 rule 2 forbids — among them the very names
  `<nros/rcl_compat.h>` §4 and `tests/compile/node_timer_accessors.c` said were
  refused (`rcl_timer_init`, `rcl_node_get_domain_id`).
- Conformed (rcl's signature, so the name stays): the six `rcl_*_fini` of
  publisher / subscription / service / client / action client / action server
  take `(entity, node)`; `rcl_clock_init(type, &clock, &allocator)`;
  `rclc_support_init(&support, argc, argv, &allocator)` (`--ros-args` is
  refused — no C ROS-argument parser); `rclc_executor_init(&executor,
  &support.context, n, &allocator)`; `rcl_action_client_init(client, node,
  type_support, name, options)`; `rcl_node_get_domain_id` writes `size_t`,
  `rcl_timer_get_time_since_last_call` `int64_t`, `rcl_clock_get_now` the
  nanosecond count; `rclc_executor_remove_*` take `const`. New types:
  `rcl_allocator_t` + `rcl_get_default_allocator()` (accepted, never allocates;
  a caller-supplied allocator is refused) and `rcl_context_t`, the
  `context` member of `rclc_support_t`.
- Distinct `rclc_` names where the contract differs:
  `rclc_timer_init_with_context` / `rclc_timer_callback_with_context_t` (and
  NO witness argument: the support was a validity check the init never read,
  and a hosted C component has none), `rclc_clock_get_now_time`,
  `rclc_node_resolve_name`, `rclc_action_server_init_with_callbacks`,
  `rclc_action_publish_feedback_raw`, `rclc_action_server_goal_exists`,
  `rclc_{subscription,service}_raw_callback_t`,
  `rclc_executor_attach_{client,action_client,action_server}`,
  `rclc_{publisher,subscription}_get_*_count`, `rclc_support_init_with_locator`.
  `rclc_subscription_callback_with_context_t` is the definition (identical to
  rclc's); its alias is gone.
- The baked-RMW rung (`<nros/baked_rmw.h>`, issue 1531) covers both support
  inits: `rclc_support_init` forwards to the seam `nros_c_support_init_baked`,
  `rclc_support_init_with_locator` to `rclc_support_init_rmw`; the lane probe
  pins both.
- Class fix: `check-c-api-upstream-signatures` (fast line) compares every name
  the C headers share with humble rcl / rcl_action / rcl_lifecycle / rclc /
  rclc_lifecycle / rclc_parameter against a committed snapshot
  (`docs/reference/upstream-c-signatures.tsv`). A function conforms when an
  upstream call site compiles against ours unchanged; a callback type only
  when identical. Ruled exceptions live in
  `.config/c-api-upstream-signature-deviations.txt` (the executor trigger
  family), and a stale ruling fails. The rename generator no longer adopts a
  `systematic` row without a measured conformance (`SIGNATURE_CONFORMED`).
- Ledger rows are re-keyed in W6 against `just check api-parity`.

### 2026-10-12 — W4 (part 2): C components on the C API

- `RCLC_C_COMPONENT(StructT, configure_fn)`: `configure_fn(rcl_node_t *node,
  rclc_executor_t *executor, StructT *self)`. The generated `configure` seam
  keeps its host signature and binds the C handles itself
  (`nros_cpp_node_bind_c` into a static `rcl_node_t` beside the instance), so
  no entry emitter changed. `component.h` now declares only the opaque node
  handle and that seam — the C mirrors of the C++ FFI types
  (`nros_cpp_qos_t`, `rclc_c_qos_default()`, the `RCLC_C_*` enums, the
  `NROS_C_*_STORAGE_SIZE` buffers, the `nros_cpp_*` prototypes) are gone.
- Every C component moved: talkers / listeners (typed:
  `rclc_executor_add_subscription_with_context`), QoS (`rclc_qos_t` +
  `*_init_with_qos`), services (the generated typed handler and client
  helpers), actions (`rclc_action_server_init_with_callbacks` +
  `rclc_action_execute` / `rclc_action_succeed`; the async client), the
  parameter talker (`rclc_parameter_server_t` over the host's store, never
  added to the executor), the safety listener (new
  `rclc_executor_add_subscription_raw_validated`, the callback-style sibling of
  `rclc_polling_subscription_take_validated`), the Zephyr examples (generated
  bindings instead of hand-rolled CDR), the templates and the `nros new --lang
  c` scaffold (now declares `std_msgs`). The mixed param probe's C half takes an
  `rcl_node_t`; its C++ caller binds one with `nros_cpp_node_bind_c`.
- Found and fixed (`nros-node`): the executor's spin TOOK the reply of a
  service client registered without a response callback and dropped it, so the
  polling API (`rclc_client_take_response`) never saw one. A C component that
  polls a client from a timer — the shape every C service-client component had
  — got no reply while the server answered every request. A callback-less
  client's reply is now left for the poll.
- Gates: `check-ffi-struct-mirrors` family 1 moves to the mirror that is left
  (`qos.hpp`'s `struct nros_cpp_qos_t`, which nothing had compared — mutation
  tested); `check-qos-profile-ssot` drops the `component.h` subject; the issue
  1535 short-buffer probe retires with the buffer.

### 2026-10-13 — enum values take upstream's spelling

- W1's map classified every constant without asking upstream ("API constant
  with no upstream spelling"), so three enums that already carried upstream's
  TYPE name kept `RCLC_` values a ported file cannot write:
  - `rcl_clock_type_t`: `RCL_CLOCK_UNINITIALIZED`, `RCL_ROS_TIME`,
    `RCL_SYSTEM_TIME`, `RCL_STEADY_TIME` (were `RCLC_CLOCK_*`);
  - `rclc_executor_semantics_t`: `RCLCPP_EXECUTOR`, `LET` (were
    `RCLC_SEMANTICS_*`);
  - `rclc_executor_handle_invocation_t`: `ON_NEW_DATA`, `ALWAYS` (were
    `RCLC_EXECUTOR_*`).

  Same numbers; the eight map rows are now `adopt`. `RCLC_CLOCK_STATE_*`
  stays (rcl has no clock-state enum).
- Class closed by a rule, not the map: `check-c-api-upstream-signatures` now
  also compares ENUM typedefs. Upstream's values must be a prefix of ours
  (names and numbers), so `rclc_parameter_type_t`, which appends six kinds after
  rclc's four, conforms. The snapshot
  (`docs/reference/upstream-c-signatures.tsv`) gained the 10 upstream enums from
  the same humble checkouts. Negative control: main's header fails on the
  three enums above.
- Not adopted: rcl_action's `GOAL_STATE_*`. Upstream spells them as macros
  over an `int8_t` typedef (`rcl_action_goal_state_t`), and ours is the enum
  `rclc_goal_status_t`, so the type names differ and this rule does not reach
  them.
- Also: `declared_qos.h` no longer says a C component hands `nros_cpp_qos_t`
  (W4 part 2 moved components onto `rclc_qos_t`).

### 2026-10-13 — the public headers are freestanding

- Ruling (maintainer, 2026-10-13): a user includes the C API only at
  upstream's paths; `<nros/*.h>` is internal (to be renamed into `rclc` later),
  and the public headers must be freestanding so a part with no libc can
  include them.
- Measured: all 51 upstream-path headers failed `-ffreestanding -nostdinc`.
  Exactly two hosted includes reached the umbrella: `log.h`'s `<stdio.h>`, for
  an inline `vsnprintf` in `rclc_log_emit_fmt_at`, and `view.h`'s
  `<string.h>`, for a bit-cast `memcpy`. (`check.h`'s `<stdio.h>` was already
  gated on `__STDC_HOSTED__`.)
- `rclc_log_emit_fmt_at` moved into `c-stubs/log_fmt.c` beside
  `rclc_log_emit_fmt`, sharing one truncating renderer. That was the follow-up
  its own comment named. `view.h` bit-casts with an inline byte copy.
- Eight example components called `memset` and had received `<string.h>`
  through a generated message header via `view.h`; they include it now.
- Gate `check-c-api-freestanding-headers` (fast line): every tracked public
  header (57, everything outside `nros/`) compiles on its own with
  `-ffreestanding -nostdinc`, with a negative control. Against main's headers
  it fails 51 of 57.

### 2026-10-13 — user code includes the API only at upstream's paths

- Scope chosen by the maintainer: "includes only, now". The entry and boot
  names a C app writes (`nros_app_main`, `NROS_APP_MAIN_REGISTER`,
  `NROS_ENTRY_*`, `NROS_LOCATOR`) keep `nros_` until the `nros/*.h` -> `rclc`
  rename and move together with it. The C++ side is not in this step.
- `<nros/nros.h>` (the umbrella every upstream-path header includes) now also
  includes `<nros/component.h>`, so `RCLC_C_COMPONENT` arrives with
  `<rclc/rclc.h>`.
- 80 C example sources dropped `<nros/{app_main,check,component,log}.h>`. A
  file that had no upstream-path include left gets `<rclc/rclc.h>`. The book's
  C snippets and the `nros new` C component scaffold were changed the same way.
- Gate `check-c-user-includes` (fast line): no example C source and no ```c
  block in `book/src/` outside `porting/` includes a header from
  `packages/api/nros-c/include/nros/`. The set is derived from that directory,
  so port-seam headers stay legal. Main's tree has 141 violations.
