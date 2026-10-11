# Phase 491 — The C parameter server: one rclc-shaped family over the executor store

**Status (2026-10-11). Opened.** Nothing landed yet. Implements
[RFC-0105](../design/0105-c-parameter-server.md). Closes issues
[0793](../issues/0793-c-params-declared-via-legacy-store-invisible-to-ros2.md) and
[0865](../issues/0865-no-example-registers-parameter-services.md), and lands the C
half of [1812](../issues/1812-param-services-require-alloc.md).

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
quota; range enforcement confirmed on the service path. C++ and Rust callers
keep their semantics.
*Acceptance:* unit tests for each row of RFC-0105's callback table, the
re-entrancy refusal, the quota, and read-only/range on both paths.

**W3 — `/parameter_events` emitter.**
In `nros-node` behind `param-services`: one publisher per node with the
`parameter_events` QoS, serialising one-parameter events straight from the store
on add, set and delete. No `ParameterEvent` struct is built.
*Acceptance:* a host test decodes the CDR of each event kind; `mem-report`
shows no new heap use.

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
`RCL_RET_NOT_INIT`).

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

**W7 — Issue 1812: heap-free services.**
Carve the remaining boxed service state, as issue 1812 lists. Tracked there;
named here because RFC-0105 D5 holds end to end only after it lands.

## Out of scope

* Moving the rest of the C API to `rcl_*`/`rclc_*` primary names
  (RFC-0105 §Open questions).
* `/parameter_events` for the C++ and Rust surfaces. W3 puts the emitter where
  they can reach it; turning it on there is a separate item.
