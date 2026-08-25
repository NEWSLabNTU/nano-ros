# Phase 382 — one parameter store, caller-owned and alloc-free

**Status (2026-08-25). NOT STARTED — designed, scoped out of issue 0793.**
Issue 0793 found two disjoint C parameter stores and asked which to delete. The
answer is neither: conciliate them into one API that keeps what each does well.

**Implements.** RFC-0045 (boot config), and amends RFC-0036's parameter row.
Closes the store half of issue 0793.

## The situation, measured

Two stores, and the consumer counts invert the obvious reading:

| | legacy `nros_param_server_t` | executor-owned (`nros_params::ParameterServer`) |
| --- | --- | --- |
| in-tree consumers | **3** — `nros::ParameterServer` (C++), `examples/native/c/parameters`, `custom-transport-loopback` | **0** |
| C entry points | all 10 `rcl_interfaces` types incl. arrays | 4 scalars |
| visible to `ros2 param` | **no** | yes |
| storage | **caller-owned** (`*mut nros_parameter_t` into a caller array) | `[Option<ParameterEntry>; MAX_PARAMETERS]`, build-time const |
| needs `alloc` | no | **yes** |

So the documented, exampled, C++-wrapped path is the invisible one, and the path
`ros2 param` can see has no users, no example, and a narrower C surface.

Two facts make conciliation cheap rather than a rewrite:

* **`nros_params::ParameterValue` already covers all ten types**, arrays
  included. The 4-scalar limit is in the C *entry points*, not the store — so
  adopting it loses no capability.
* **The store is already a fixed array**, not a heap structure. The `alloc::`
  uses in `nros-params` are edge conversions (`impl ParameterVariant for
  String`/`Vec<T>`), never storage.

## Why `param-services` requires `alloc` today — and why that is fixable

Not the store. Two separate reasons, both about placement rather than dynamic
sizing:

1. The executor holds `params: Option<alloc::boxed::Box<ParamState>>`.
2. The service handlers return `Box<Response>` **because the response types
   contain ~32 KB of heapless arrays** and a value that size cannot go on an
   RTOS task stack (`parameter_services.rs:313,346,383,453` each say so).

Reason 2 is the interesting one: it is a *stack* problem, and the answer to a
stack problem on a device with no heap is caller-owned storage — the same answer
the parameter table itself wants. So both reasons dissolve into one mechanism.

## The mechanism already exists

phase-271 moved six executor tables into caller-owned storage:
`ExecutorSizing { cbs, sc, arena }` names the capacities, `carve()` splits a
caller-supplied backing into typed sub-slices, `ExecutorSlices<'s>` is what the
executor borrows, and a `#[repr(C)] ExecutorStorage<CBS, SC, ARENA>` reference
layout is unit-tested against the carve. Issue 0563 added a seventh table
(remaps) the same way.

**The parameter table and the service response scratch become the eighth and
ninth.** That yields, without inventing anything:

* caller-chosen capacity — a field on `ExecutorSizing`, not `NROS_MAX_PARAMETERS`
* caller-chosen placement — it is the caller's backing buffer, so it can live in
  `.sram2` or wherever the board wants
* alloc-free — no `Box` for the table, and the 32 KB response scratch has a home
  that is neither stack nor heap
* `ros2 param` visibility — it IS the executor's store, which the
  `rcl_interfaces` servers already read

## Work items

**W1 — `ParameterServer` borrows its table.** `[Option<ParameterEntry>;
MAX_PARAMETERS]` → `&'s mut [Option<ParameterEntry>]`. `MAX_PARAMETERS` survives
as the default sizing, not as the only sizing.

**W2 — carve the table and the response scratch.** Add both to
`ExecutorSizing`/`ExecutorStorage`/`ExecutorSlices`/`carve`, with the layout
unit test extended — that test is what keeps the const-fn carve and the
`#[repr(C)]` reference honest.

**W3 — drop the two `alloc` reasons.** `ParamState` inline rather than `Box`ed;
handlers write into the carved scratch rather than returning `Box<Response>`.
Then delete the `compile_error!` in `nros-node/src/lib.rs:250` and prove
`param-services` builds without `alloc`.

**W4 — one C surface.** The rich `nros_param_*` family (all ten types) becomes
the API, operating on the executor's store. `nros_executor_declare_param_*`
becomes deprecated aliases or is deleted. `nros_param_server_set_callback` —
the accept/reject veto, today installed where no service reads it — lands on the
path a remote `ros2 param set` actually takes, which is what issue 0793 wanted.

**W5 — the consumers.** `nros::ParameterServer` (C++), both C examples, and the
C++ `ComponentNode` parameter surface, which today has `declare`/`get` and **no
setter at all**.

**W6 — the ledger.** 32 `gap` and 22 `rename` rows in
`docs/reference/api-parity-ledger/param.json`; several assert the split this
phase removes. `just check-api-parity` must stay green.

## Acceptance

* A parameter declared through the one C API is visible to `ros2 param list` and
  settable by `ros2 param set`, in a live interop cell.
* The accept/reject callback fires for a remote set.
* `param-services` builds with `--no-default-features` and no `alloc`.
* A caller can place the parameter table in a named linker section, and there is
  an example that does — otherwise the capability is claimed and untested, which
  is how the legacy store's own placement property ended up with no in-tree
  consumer exercising it.

## What this deliberately does not do

Domain and locator in the baked boot config (issue 0794) are a different
producer question. And the `EnvRung` asymmetry noted there — the namespace has
two ladder rungs where the domain has three — should be settled in RFC-0045, not
here.
