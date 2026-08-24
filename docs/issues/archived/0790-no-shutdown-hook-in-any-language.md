---
id: 790
title: "No shutdown hook in any language — a node that must park an actuator or
  release a bus on the way down has nowhere to do it"
status: resolved
type: bug
area: api, core
related: [rfc-0002, rfc-0036, phase-379, issue-0460]
---

## Problem

rclcpp lets a node register work to run when the context shuts down, in two
ordered phases:

* `Context::add_pre_shutdown_callback` — runs BEFORE entities are torn down.
* `Context::add_on_shutdown_callback` / `rclcpp::on_shutdown` — runs after.
* Both return a handle (`PreShutdownCallbackHandle`, `OnShutdownCallbackHandle`)
  so a callback can be removed.

nano-ros has **none of it** — not the free function, not the context methods,
not the pre-shutdown phase, in C, C++ or Rust. Phase 379's `init` stage recorded
four `gap` rows for it (`cpp:on_shutdown`, `cpp:ShutdownCallbackHandle`,
`cpp:OnShutdownCallbackHandle`, `cpp:PreShutdownCallbackHandle`).

`Executor::shutdown` exists and stops the spin loop; nothing runs on the way out.

## Why it matters more here than on a desktop

On a desktop, a process that exits without ordered cleanup leaks nothing the OS
will not reclaim. On a device there is no OS to reclaim it:

* an actuator holds its last commanded position
* a CAN or SPI peripheral stays claimed
* a DMA channel stays armed
* a watchdog stays fed by a task that is no longer doing useful work

The pre-shutdown phase is the load-bearing half, and it is the one with no
workaround: a node has to release hardware *while its entities still work*, so
it can publish a final state or answer a last request. After teardown it cannot.

Nothing about `no_std` prevents this. A fixed-capacity array of
`(fn_ptr, void* ctx)` sized by a Kconfig knob is the same shape as every other
static table in the tree, and the ordering guarantee is a loop in a known
direction.

## Evidence

`scripts/api-parity.py --topic init`, and the four `gap` rows in
`docs/reference/api-parity-ledger/init.json`. The `why` on `cpp:on_shutdown`
carries the argument.

## Direction

Not decided here; phase 379 W3 owns it. Points worth settling when it is taken
up:

* **Capacity and where it is configured.** A static array needs a bound; the
  parameter-services precedent (issue 0460 — queryable slots are a measurable
  cost) says the default should be small and the knob explicit.
* **Which object owns the list.** rclcpp hangs it on the `Context`, which we do
  not have (the init stage records the collapse into `nros_support_t`). The
  executor is the natural owner here since it is what `shutdown` is called on.
* **Whether both phases are needed.** The pre-shutdown phase is the one with a
  real use; shipping only the post-teardown one would look like the feature and
  not be it.
* **What runs it on an abnormal stop.** A watchdog reset or a panic does not go
  through `shutdown`, so the hook is a clean-stop facility and the docs should
  say so rather than implying a guarantee it cannot make.

## Resolution (2026-08-25)

Both phases exist, in all three languages, owned by the `Executor` — rclcpp
hangs them on `Context` and we have none (phase-379's init stage records the
collapse), and the executor is what `shutdown` / `close` / `fini` is called on.

**Ordering.** `Executor::close()` runs the pre-shutdown list, THEN closes the
session, THEN runs the on-shutdown list. `Executor::drop` does the same around
its teardown — the C API's `nros_executor_fini` never calls `close()`, so
without that half the whole facility would have been inert for every C entry.
Each phase table is emptied before its first callback runs, so "exactly once"
is a property of the table (a second `close()`, or the drop after one, finds
nothing) and no `&mut` into it is live while foreign code runs.

**Capacity.** `NROS_EXECUTOR_MAX_SHUTDOWN_CBS` (env or
`CONFIG_NROS_EXECUTOR_MAX_SHUTDOWN_CBS`), **default 2, per phase** — issue
0460's precedent: a static slot is a cost every image pays, including the ones
that register nothing. Overflow is reported (`NodeError::ShutdownCallbacksFull`
/ `NROS_RET_FULL`), never silently dropped.

**Handle.** A `u32` packing a phase tag (high half) and the SLOT INDEX (low
half). rclcpp's handle owns a `shared_ptr` to the callback; there is no
allocator here and the callbacks live in a fixed array, so the index is what
identifies one. The phase is packed in rather than left implicit because a slot
index means nothing without the table it indexes and there are two tables — a
handle from one phase removes nothing from the other. C++ additionally gets
distinct `PreShutdownCallbackHandle` / `OnShutdownCallbackHandle` types, as in
rclcpp, so the same mistake is a compile error there.

**Surfaces.**

* Rust — `Executor::{add,remove}_{pre,on}_shutdown_callback`,
  `shutdown_callback_count`, `ShutdownPhase`, `ShutdownCallbackHandle`,
  `ShutdownCallbackFn`.
* C — `nros_executor_{add,remove}_{pre,on}_shutdown_callback`,
  `nros_shutdown_callback_t`, `nros_shutdown_callback_handle_t`,
  `NROS_SHUTDOWN_CALLBACK_HANDLE_INVALID`.
* C++ — the same four verbs on `nros::Executor`, plus the free
  `nros::on_shutdown` / `nros::pre_shutdown` (+ removals) over the global
  executor. `nros::on_shutdown` is the `rclcpp::on_shutdown` mirror.

**What it still does not promise.** A watchdog reset, a hard fault or a panic
does not go through `close()`, so the hooks do not run then, and every doc
comment on the surface says so. Hardware that must be safe across an abnormal
stop needs a hardware answer, not a callback.

**Ledger.** `cpp:on_shutdown` re-verdicted `gap` -> `divergence` (the name
corresponds now; the signature differs because there is no allocator for
`std::function`). `cpp:ShutdownCallbackHandle`, `cpp:OnShutdownCallbackHandle`
and `cpp:PreShutdownCallbackHandle` now report `same` and their rows are gone.
The new surface is classified in `exec.json` (executor methods and the type
vocabulary) and `init.json` (the context-shaped free functions).
