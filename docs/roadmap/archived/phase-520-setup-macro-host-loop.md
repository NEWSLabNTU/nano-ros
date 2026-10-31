# Phase 520 — `nros::setup!`: model-driven assembly for a host that owns its loop

**Status (2026-10-31). RETIRED — W1, W1b, W2 and W3 landed; W4 not started;
the surface is withdrawn and phase-521 deletes it.** The maintainer ruled on
2026-10-31 that the real-time promise (stated response-time, jitter and
priority bounds) is a precondition of support. A host-driven loop — RFC-0112's
Tier 0: tokio, an Embassy task, Qt/GLib, a hand-written PX4 work item — decides
when callbacks run, so nano-ros cannot state those bounds there, and Tier 0 is
withdrawn. `nros::setup!` exists to serve that shape, so it goes with it. The
ruling is RFC-0113 "rt-unified-wait" (D1 the support rule, D7 the retired
surface, D10 `setup!`), drafted on 2026-10-31 and landing with phase-521;
phase-521 W1 deletes the macro, the `nros_setup` / `nros_deploy` / `nros_boot`
emit, the W3 deferral helpers, the `listener-host-loop` example and cell, the
tests and the book text, together with `spin_async*`, `set_ready_notifier`,
`ready_fd` and the public `Executor::next_deadline_us`. PX4 stays supported as
a Tier 1 framework (a generated mapping of tiers onto PX4 work queues), not
through `setup!`.

Issue 2150 is closed `wontfix` with this ruling
([archived](../../issues/archived/2150-nros-setup-macro-host-loop.md)). What
follows is the record of what was built and measured, kept because phase-521's
deletion has to cover every item of it; nothing below is a plan.

Implemented [issue 2150](../../issues/archived/2150-nros-setup-macro-host-loop.md)
under [RFC-0112](../../design/0112-framework-support-tiers.md) D2 (the Tier 0
host-driven contract) and D4 (recipes). The
[issue 2134](../../issues/archived/2134-embassy-macro-branch-untested.md) demotion
landed first (2026-10-30: `nros::main!`'s Embassy arm is deleted and the book
shows the hand-driven shape), so `setup!` was what gave an Embassy recipe back
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

### W1b — the board's start-up without its loop — DONE

Answers the question W1's hand run raised. No new board trait was needed:
every OwnedSpin board's `BoardEntry::run_with_deploy{,_sized}` already does
the start-up (backend registration, log sinks, hardware, network, the
executor sized as `nros::main!` sizes it) and THEN calls its closure. Linux
never spins after the closure, and an RTOS board spins only if the closure
returns. So a closure that keeps the host's loop is the board's start-up
without its loop. When the image's board resolves to an OwnedSpin
`BoardEntry`, `setup!` also emits:

```rust
pub fn nros_boot(
    host: impl FnOnce(&mut dyn NodeDispatchRuntime) -> Result<(), RuntimeError>,
) -> Result<(), RuntimeError>;   // run_with_deploy{,_sized}(&nros_deploy(), |ctx| host(ctx.runtime))
pub fn nros_stop_requested() -> bool;  // the board's termination guard
```

```rust
nros::setup!();
fn main() {
    nros_boot(|rt| {
        nros_setup(&mut *rt)?;
        while !nros_stop_requested() { let _ = rt.spin_once(10); /* host work */ }
        Ok(())
    }).expect("boot");
}
```

- The error is fixed to `RuntimeError`, `nros_setup`'s: a generic `E` cannot
  be inferred for a closure whose loop never returns (measured, `E0283`).
- `nros_stop_requested` exists because the board installs a SIGTERM guard,
  and a host loop that ignores it never exits (measured: the hand-run listener
  had to be killed). Returning lets the board close the session (issue 1732).
- No `nros_boot` for a framework board (RTIC, Embassy, Zephyr, esp-hal: no
  `BoardEntry` to call) or a board the macro cannot resolve. Such a host does
  its own start-up and uses `nros_setup` alone.
- Measured by hand on 2026-10-30: the linux listener as the program above, with
  no logger, backend call or config of its own, received every message from
  the stock talker through `rmw_zenohd`, logged through the board's sink, opened
  with the derived sizing (6144-byte arena) and, on SIGTERM, printed `closing
  the RMW session` and exited cleanly.

### W2 — a delivering host-loop example and cell — DONE

A `examples/linux/rust/` leaf whose `main` is a plain `std` loop over
`nros_setup`, with a `fixtures.toml` row and a `matrix::CELLS` cell that asserts
delivery. Proves the function is complete, not just that it compiles.

- Example: `examples/linux/rust/listener-host-loop`. Its `lib.rs` is the
  `listener` example's node, unchanged; its `main.rs` is the W1b program
  (`nros_boot` → `nros_setup` → `while !nros_stop_requested() { spin_once(10) }`).
  One image (`linux`, zenoh): who drives the loop is not an RMW or edition
  axis.
- Row: `native-rust-listener-host-loop` in `examples/fixtures.toml`
  (coordinate `linux,rust,zenoh`).
- Cell: `cell(Linux, Rust, Zenoh, HostLoop, Example, Runtime)` — a new
  `Workload::HostLoop`, its own value on `TypedCarrier`'s reasoning (the
  subject is the `setup!` road; every `Pubsub` example takes `nros::main!`).
  Consumer: `linux_example_pubsub_e2e::native_example_host_loop_delivers`,
  which pairs it with the stock rust talker through a unique `rmw_zenohd`,
  asserts ≥3 `I heard:` lines (`checker::assert_delivery`), then sends ONE
  SIGTERM and asserts the process exits 0 on its own within 10 s.
- Measured 2026-10-31: `cargo nextest run -p nros-tests --test
  linux_example_pubsub_e2e -E 'test(native_example_host_loop_delivers)'`
  PASS in 3.4 s on fresh fixtures (`fixtures-build.sh linux rust --id
  native-rust-listener-host-loop` + `--id native-rust-talker`). Negative
  control: the same image with the `nros_setup(&mut *rt)?` call removed FAILS
  it (`saw 0` deliveries in 25 s), so the delivery assertion is load-bearing.
- Tier 1's 1-wise workload axis took the new workload: 25 → 26 cells at the
  same 11 coordinates (`ci_lane` table re-measured); tier 3 254 → 255.

### W3 — deferred dispatch inside `setup!` (issue 2146) — DONE

A `[[component]]` with `dispatch = "deferred"` is installed deferred by the
emitted `nros_setup`, and the host gets the drain, so a Tier 0 host runs that
component's callbacks in a task of its own.

- Built on issue 2146's executor queue (`set_deferred` / `run_deferred`),
  NOT on the `register_dispatch` slot registry this item first named: that
  registry has no consumer (issue 2161), and the queue is what a hand-written
  host already uses. The two emitters therefore agree, which 2161 asked for.
- The component is deferred as a WHOLE, with no handle named: the executor
  grew `callback_mark()` (an opaque snapshot of the occupied callback slots)
  and `set_deferred_since(mark, bool)` (defer every slot occupied since;
  timers skipped and not counted). `nros_setup` brackets the component's
  `register(runtime)?` with the two, through
  `nros::__macro::{callback_mark, defer_registered_since}`. Measured by unit
  test: a slot registered before the mark stays inline, a timer among the new
  ones is skipped, the deferred subscription is queued by the spin and run by
  the drain.
- The host's calls, emitted beside `nros_setup` (always, `#[allow(dead_code)]`,
  so a host that defers by hand has them too): `nros_run_deferred(rt) -> usize`,
  `nros_has_deferred(rt) -> bool`, `nros_set_defer_notifier(rt, fn, ctx) ->
  bool`, and `NROS_DEFERRED_COMPONENTS: &[&str]`. They reach the executor
  through the lent `&mut dyn NodeDispatchRuntime`'s `executor_handle()`, so
  a host inside `nros_boot`'s closure needs nothing else.
- Which component a row names: `pkg` is a ROS package name, and the leaf's own
  component has TWO names — the cargo crate (`CARGO_PKG_NAME`, what the
  self-bringup ident is spelled from) and `package.xml`'s `<name>`. The first
  image built reported `Deferred components: []` because
  `native_listener_host_loop` was matched against `native_rs_listener_host_loop`;
  a row without `pkg`, or naming either of the leaf's names, now matches, and
  `package.xml` is a tracked rebuild input of `setup!`.
- `nros::main!` ignores the key (expansion test `main_does_not_defer_a_deferred_component`):
  its board loop is the only task, so a deferral would queue callbacks nothing
  drains. RTIC's own queue remains issue 2161.
- Acceptance: the W2 example is now the deferred one — `listener-host-loop`
  declares `dispatch = "deferred"` and its loop is `spin_once(10)` then
  `nros_run_deferred`; the `HostLoop` cell asserts the 3 deliveries AND the
  drain's marker line (`output::HOST_LOOP_DEFERRED_DRAIN_MARKER`). Measured
  by hand 2026-10-31 through `rmw_zenohd` with the stock talker: `Deferred
  components: ["native_rs_listener_host_loop"]`, `Deferred callbacks ran from
  the host loop: 1`, 6 `I heard:` lines in 6 s, clean exit on SIGTERM.
- Expansion tests: mark / register / defer order and the package literal,
  the leaf's own component by either name, inline / absent / foreign rows
  left verbatim with an empty `NROS_DEFERRED_COMPONENTS`.

### W4 — the Embassy recipe on `setup!` — NOT STARTED, RETIRED

Withdrawn with the surface (RFC-0113 D10). The Embassy chapter keeps the
hand-driven shape issue 2134 left it with, and whatever recipe Embassy gets
next is phase-521's to state under the Tier 1 rule.

As planned before the ruling:

Rewrite the book's Embassy chapter around `setup!` and a host loop
(2143/2144's notifier once they land; `spin_some` + timer until then), with a
cell on `embassy-executor`'s std arch — the recipe and cell issue 2134's
resolution left waiting on 2150.

## Open questions (moot since the ruling; kept as record)

- Executor backing: the host opens the `Executor`, so it chooses the backing.
  Should `setup!` also emit the `.bss` `EXECUTOR_BACKING` sizing it derived,
  so a no-alloc host does not have to re-derive it?
- Multi-tier: refused in W1. A host that wants tiers either keeps
  `nros::main!` or gets a `nros_setup_tier(i, runtime)` per tier later.
- Whether `nros::main!` should itself be rebuilt as "`setup` + the board's
  loop", so the two cannot drift. W1 shares the token streams, which already
  prevents drift in the body.
