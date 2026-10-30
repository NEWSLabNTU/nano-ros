# Phase 520 — `nros::setup!`: model-driven assembly for a host that owns its loop

**Status (2026-10-30). W1 landed; W2–W4 open.** Implements
[issue 2150](../issues/2150-nros-setup-macro-host-loop.md) under
[RFC-0112](../design/0112-framework-support-tiers.md) D2 (the Tier 0
host-driven contract) and D4 (recipes). The
[issue 2134](../issues/archived/2134-embassy-macro-branch-untested.md) demotion
landed first (2026-10-30: `nros::main!`'s Embassy arm is deleted and the book
shows the hand-driven shape), so `setup!` is what gives an Embassy recipe back
the `system.toml` assembly that arm used to emit.

---

## Why

`nros::main!` emits the assembly (components, run plan, parameters, lifecycle,
contract monitors) and the loop (a `main` for the board's entry shape, and the
spin inside it) as one unit. A Tier 0 host (Embassy, tokio, GLib, an Arduino
`loop()`, a PX4 work item) owns its loop, so today it must hand-write the
assembly the model already describes. Every board's `BoardEntry::run` already
takes that assembly as a closure over `&mut RuntimeCtx`, and the closure body
names no board. `setup!` emits that closure as a function.

## Work items

### W1 — `nros::setup!` emits `nros_setup` — DONE

`nros::setup!()` takes `nros::main!`'s model arguments (`launch =`,
`args =`, the deprecated `model =`) and emits:

```rust
pub fn nros_setup(
    runtime: &mut dyn NodeDispatchRuntime,
) -> Result<(), RuntimeError>;           // the board closure's body, nothing else
pub fn nros_deploy() -> DeployOverlay;   // the image's locator / domain / transport / rmw
```

No `main`, no spin, no panic handler, no board call. The host opens the
`Executor`, wraps it (`ExecutorNodeRuntime::from_executor`), calls
`nros_setup(&mut rt)`, then drives `spin_once`/`spin_some` from its own loop.
The host never has to name the signature's types: `&mut rt` coerces to
`&mut dyn NodeDispatchRuntime`, and the results are matched or `expect`ed.
Re-exporting them from `nros` would widen the public API (`check
api-parity`), which W1 does not need.

- The board is NEVER CALLED: the image still names one (RFC-0098 D3 is the
  leaf reader's rule), but only its overlay facts are read, a board the
  in-tree table does not know is no error, and its framework is ignored,
  because the host is the framework. Whether an Embassy recipe's image names
  an out-of-tree board or a new host-only board word is W4's question.
- Refused, each naming why: `spin =`, `panic =`, `before_boot =`,
  `custom_tasks =` (all belong to whoever owns `main`), and a multi-tier image
  (spawned tier executors need a board's tier runner; see open questions).
- Acceptance: macro expansion tests over a throwaway leaf (the
  `leaf_axis_tests` harness) for the emitted shape and each refusal.
- Measured by hand on 2026-10-30, not yet a cell (that is W2):
  `examples/linux/rust/listener` with its `main.rs` replaced by `setup!` plus a
  `std` loop over `spin_once(10 ms)` built with `nros build linux` and received
  every message from the stock talker through `rmw_zenohd`.
- Found doing it: a host on a nano-ros board still wants that board's boot
  steps WITHOUT its loop. On `linux` that is `register_linked_rmw()` (public)
  and the log sinks (`init_default`, reachable only inside `boot_hosted`).
  The hand-run host had to install its own `log` sink to see the callbacks
  print. See the open question below.

### W2 — a delivering host-loop example and cell

A `examples/linux/rust/` leaf whose `main` is a plain `std` loop over
`nros_setup`, with a `fixtures.toml` row and a `matrix::CELLS` cell that asserts
delivery. Proves the function is complete, not just that it compiles.

### W3 — deferred dispatch inside `setup!` (issue 2146)

Emit the `register_dispatch` slots for `dispatch = "deferred"` components, and
give the host the drain call, so a Tier 0 host can run callbacks in its own
task. Closes 2146.

### W4 — the Embassy recipe on `setup!`

Rewrite the book's Embassy chapter around `setup!` and a host loop
(2143/2144's notifier once they land; `spin_some` + timer until then), with a
cell on `embassy-executor`'s std arch — the recipe and cell issue 2134's
resolution left waiting on 2150.

## Open questions

- Board boot without the loop: should a board expose `host_init()` (backend
  registration, log sinks, `init_hardware`) for a host that calls
  `nros_setup`, or should `setup!` emit an `nros_init()` that calls it when the
  image names an in-tree board? W2 needs one of the two, or its example
  carries board boot steps by hand.

- Executor backing: the host opens the `Executor`, so it chooses the backing.
  Should `setup!` also emit the `.bss` `EXECUTOR_BACKING` sizing it derived,
  so a no-alloc host does not have to re-derive it?
- Multi-tier: refused in W1. A host that wants tiers either keeps
  `nros::main!` or gets a `nros_setup_tier(i, runtime)` per tier later.
- Whether `nros::main!` should itself be rebuilt as "`setup` + the board's
  loop", so the two cannot drift. W1 shares the token streams, which already
  prevents drift in the body.
