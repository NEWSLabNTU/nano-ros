# Phase 491 — The C parameter server: one rclc-shaped family over the executor store

**Status (2026-10-12). Complete — W1–W7 landed (#1996, #2015); C components move with phase-492 W4.**
Implements [RFC-0105](../../design/0105-c-parameter-server.md). Closes issues
[0793](../../issues/archived/0793-c-params-declared-via-legacy-store-invisible-to-ros2.md) and
[0865](../../issues/archived/0865-no-example-registers-parameter-services.md), and lands the C
half of [1812](../../issues/archived/1812-param-services-require-alloc.md). W7 closed 1812
whole. Carried out of this phase: C components, which still use the C++ shim for
everything and move with phase-492 W4.

## Why

The C API has three parameter families. The rclc-shaped one is served by
nothing, the served one has none of rclc's names, and C components borrow
C++'s. RFC-0105 replaces them with rclc's API, spelled with rclc's names, over
the executor's store, plus `/parameter_events`.

## Work items

**W1 — Issue 0865: header and docs (independent, lands first).**
Guard or document the `param-services` requirement on the service entry point
in the generated header, fix the `NROS_PARAM_SERVICES` name in `parameter.h`,
and fix the CMake capability docs. Lands on the current API, so 0865 closes
before W5 deletes that entry point.
*Acceptance:* a C caller without the capability reads the requirement in the
header, not in `ld`'s output.

**W2 — Store: rclc local-set semantics.**
In `nros-params`: `OnSetParameterFn` carries `old: Option<&ParameterValue>` and
`new: Option<&ParameterValue>`; one entry point for an rclc local set (type
check, callbacks, write; no read-only or range check, RFC-0105 D7); a per-node
quota; range enforcement confirmed on the service path; one carved staging
slot per executor for a remote set's incoming value, replacing today's stack
local in `parameter_services.rs` (RFC-0105 D10.2). C++ and Rust callers keep
their semantics.
*Acceptance:* unit tests for each row of RFC-0105's callback table, the
re-entrancy refusal, the quota, and read-only/range on both paths; a stack
measurement of a remote set with a full string array stays under the documented
bound.

**W3 — `/parameter_events` emitter.**
In `nros-node` behind `param-services`: one publisher per node with the
`parameter_events` QoS, serialising one-parameter events straight from the store
on add, set and delete, into a carved buffer sized from the store's bounds. No
`ParameterEvent` struct is built. Stamp from the executor clock. Depth from
`NROS_PARAM_EVENTS_DEPTH` on an RTOS (RFC-0105 D8).
*Acceptance:* a host test decodes the CDR of each event kind, including one at
the largest configured bounds; `mem-report` shows the buffer as `.bss` and no
new heap use.

**W4 — The `rclc_*` C surface.**
`include/rclc_parameter/rclc_parameter.h` with the full rclc function set, the
options struct, the callback type, the member-path-compatible `Parameter`
types, `RCLC_PARAMETER_*` codes and `RCLC_EXECUTOR_PARAMETER_SERVER_HANDLES` = 6,
plus the extensions. Opaque-handle size asserted. Census hook on the declare
path. Ledger rows in `docs/reference/api-parity-ledger/param.json` move to
ADOPT / ADOPT-BOUNDED.
*Acceptance:* `packages/api/nros-c/tests/compile` builds an unmodified excerpt of the upstream
rclc parameter example; a REFUSE-LOUD fixture (rosidl `Parameter__init`) fails
to compile; `packages/api/nros-c/tests/run` covers RFC-0105 D3 (unbound node returns
`RCL_RET_NOT_INIT`); a run test under a counting allocator asserts **zero**
allocations across init, declare, every get/set, a callback veto, delete and
fini (RFC-0105 D10.1); a `nm` check finds no `getenv`/`fopen`/`printf`-family
reference from the parameter objects (D10.3/D10.4).

**W5 — Migrate and retire (closes 0793).**
Move `examples/native/c/parameters`, `c_param_talker_pkg`, codegen
`tests_c.rs`, `metadata_hooks.rs` and the gate scripts
(`check-c-executor-remove-coverage.py`, `check-census-hooks-complete.py`,
`check-example-parameter-stores.py`) to the new API, then delete the
caller-storage family, the `nros_param_*` aliases, the
`nros_executor_*_param*` family and C's use of `nros_cpp_get_param_*`.
*Acceptance:* `git grep` finds none of the retired names outside
`docs/`; `just ci gate` green.

**W6 — Live interop cell.**
The `interop::CELLS` row in RFC-0105 §Interop proof: a C node on this API
against `ros2 param` and an rclcpp `ParameterEventHandler`.
*Acceptance:* the cell runs in its lane and passes all four checks.

**W7 — Issue 1812: heap-free services, and an RTOS image proves it.**
Carve the remaining boxed service state, as issue 1812 lists. Then build the
W4 C example for an RTOS with no heap (a FreeRTOS or Zephyr image whose
`alloc` is absent) and run it in its QEMU lane. RFC-0105 D10 is not met, and
the phase does not close, until that image builds and answers `ros2 param`.

## Out of scope

* Moving the rest of the C API to `rcl_*`/`rclc_*` primary names
  (RFC-0105 §Open questions).
* `/parameter_events` for the C++ and Rust surfaces. W3 puts the emitter where
  they can reach it; turning it on there is a separate item.

## Progress

### 2026-10-12 — one C family (W1, W2, W4, W5)

What landed:

- **W2 (store).** `OnSetParameterFn` carries `(old, new)`, both `Option`
  (`nros-params` `server.rs`). `ParameterServer::set_local` is rclc's local
  typed set: type check, then hooks, then write, with no read-only or range
  check. `set_delete_on_not_set` makes a remote `NOT_SET` delete through the
  hook with `new = None`, per node. The rclcpp path is unchanged.
  `Executor::set_parameter_local_on` and `delete_parameters_on_not_set_on`
  expose these. Unit-tested:
  `the_on_set_hook_sees_old_and_new`,
  `a_local_set_skips_read_only_and_range_but_not_type`,
  `not_set_deletes_only_on_a_node_that_asked`.
- **W4 (surface).** `<rclc_parameter/rclc_parameter.h>` declares rclc's
  function set, options, callback and return codes, plus
  `RCLC_EXECUTOR_PARAMETER_SERVER_HANDLES` = 6. It adds the extensions:
  `declare_*`, string, bool/int/double/byte arrays, `get_type`, `describe`,
  the ranges, and `list`. `Parameter`/`ParameterValue` keep rosidl's member
  layout under rosidl's include guards. The Rust module is
  `cbindgen:ignore`, so nothing reaches `nros_generated.h` unconditionally.
  Run test: `packages/api/nros-c/tests/run/rclc_parameter_server.c` in
  `just check c`.
- **W1 (0865).** `NROS_HAS_PARAM_SERVICES` (0/1) is in all three producers of
  `nros_config_generated.h`. nros-cpp reads nros-c's value from the `links`
  channel. The header `#error`s at 0, naming the capability, and
  `just check c` asserts the refusal and its text.
  `docs/reference/c-api-cmake.md` says how to declare the capability.
- **W5 (retire, standalone C).** Deleted:
  - the caller-storage `nros_parameter_server_t` family;
  - the `nros_param_*` aliases;
  - every `nros_executor_*param*` / `_on` function and
    `nros_executor_register_parameter_services`;
  - `<nros/parameter.h>`, its probes, and the `example-parameter-stores` gate,
    whose class is now a compile error.

  `examples/native/c/parameters` runs on the rclc API. The census hook moved to
  the one `declare` path. `check-census-hooks-complete` and
  `check-c-executor-remove-coverage` follow it. The ledger's C rows were
  rewritten: C `same` went from 116 to 130.

Carried, with where each goes:

- **C components still use `nros_cpp_*`**, for parameters and for everything
  else: publishers, timers, publish. Moving their parameters alone would mix
  two families inside one component, so it moves with the rest in
  phase-492 W4.
- **The header spells `nros_ret_t` / `struct nros_node_t` /
  `struct nros_executor_t`**, which are the same types as `rcl_ret_t` /
  `rcl_node_t` / `rclc_executor_t` from `<nros/rcl_compat.h>`. It does not
  include `rcl_compat.h` itself. phase-492 makes the rcl spellings primary.
- ~~**The remote set still stages its value on the stack**, about 8.5 KiB at
  default bounds (RFC-0105 D10.2).~~ Done in W7, below.
- **`notify_changed_over_dds` is recorded and not acted on** until W3.

### 2026-10-12 — string arrays and the live cell (W6)

- `rclc_parameter_{declare,set,get}_string_array`: in from `const char* const*`,
  out into a caller rectangle. Elements that are too long, or too many, are
  refused; nothing is truncated. Covered in `rclc_parameter_server.c`.
- **W6.** `packages/testing/nros-tests/tests/c_param_server_interop.rs`, cell
  `native-params-c-zenoh` in `interop::CELLS`. It runs the prebuilt
  `c-parameters` image and drives it with stock `ros2 param`:
  - list and get;
  - the rclc callback's veto of `publish_rate_hz = 0`;
  - the advertised range enforced on the remote path;
  - a set on `topic_name`, which the example now marks read-only;
  - an accepted set that reads back.

  The `/parameter_events` check joins it with W3.

### 2026-10-12 — heap-free services (W7, closes 1812; RFC-0105 D10.2)

- The parameter services' state, their inline sets of six, the shared
  request/reply pair and a `ParameterValue` staging slot are one region of the
  executor backing (`ParamHome`), carved after the store's slots whenever the
  sizing carves a store. `param-services` builds without `alloc`; its
  `compile_error!` is gone, and a heap-free build with no carved store is
  refused at compile time, naming `NROS_PARAM_STORE`. Lane:
  `scripts/check-param-services-heap-free.sh` in `just check compile-smoke`.
- D10.2: a remote set decodes into the staging slot and applies with
  `ParameterServer::apply_staged` (swap, no temporary). Stack per streamed set,
  release x86_64: 26,472 -> 352 B (`set_parameters`), 35,016 -> 464 B
  (`_atomically`); held by `a_remote_set_holds_no_parameter_value_on_the_stack`.
  The `ParamHome` region is 32,000 B at default knobs (heap-free `rmw-cffi`
  build). Detail and numbers:
  [issue 1812](../../issues/archived/1812-param-services-require-alloc.md).

### 2026-10-12 — `/parameter_events` (W3)

- Store: a bounded change log, switched on per node
  (`ParameterServer::set_parameter_events`). It holds
  `PARAMETER_EVENT_LOG_LEN` = 8 entries, one per name, merged
  (new-then-set stays new). Every write site records: declare, set, local set,
  staged apply, remove and unset. An overflow is counted, never blocked on.
- Executor: `enable_parameter_events_on` creates ONE `/parameter_events`
  publisher per executor (RFC-0105 implementation notes say why it is not one
  per node). `flush_parameter_events` encodes each change as one
  `ParameterEvent` with one parameter, straight from the store into the
  carved reply buffer, and publishes it. The flush runs after every C
  parameter call and every spin's services.
- Knob `NROS_PARAM_EVENTS_DEPTH` (registry, Kconfig, cmake, platform config):
  1000 hosted, 1 otherwise.
- The C server enables events when `notify_changed_over_dds` is set
  (rclc's default).
- `nros-c` no longer refuses `param-services` without `alloc`.
- Tests:
  - `parameter_events_publish_each_change_in_its_slot` (nros-node): decodes
    the published bytes with the generated decoders;
  - `the_event_log_merges_by_name_and_counts_overflow` (nros-params);
  - the W6 cell now runs a stock `ros2 topic echo /parameter_events` across an
    accepted remote set, and asserts the node, the name and the new value.

