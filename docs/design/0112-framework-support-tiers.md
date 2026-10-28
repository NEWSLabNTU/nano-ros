---
rfc: 0112
title: "Frameworks are supported through one host-driven contract; codegen is admitted, not assumed"
status: Draft
since: 2026-10
last-reviewed: 2026-10
implements-tracked-by: []
amends: [RFC-0111]
supersedes: []
superseded-by: null
---

# RFC-0112 — Framework support tiers

**Status:** Draft (2026-10-29). Written before any code. It sets the policy for
which application frameworks nano-ros supports and how. No phase yet.

## Summary

New frameworks will keep arriving: async runtimes, embedded schedulers, GUI
event loops, autopilot work queues and vendor SDK entry points. nano-ros cannot
grow a branch in `nros::main!` for each one. Every such branch rots unless it
has a board, a CI cell and someone who owns it, and Embassy's branch already
has none of the three (issue 2134).

So this RFC sets three tiers:

- **Tier 0** is one host-driven contract that any framework can use with no
  nano-ros code specific to that framework. It is a public executor surface,
  documented and kept stable. Each integration is a copy-out recipe.
- **Tier 1** is framework codegen in `nros::main!`. It is admitted case by case
  against four criteria, and it loses its place when it stops meeting them.
- **Tier 2** is out-of-tree. A third-party board crate supplies its own entry,
  and nano-ros promises only the Tier 0 contract.

RFC-0111's `framework` field applies to Tier 1 only.

## What "framework" means

A framework is whatever owns the image's control loop. That covers three
things: who runs `main`, who decides when work runs, and how work is
prioritised. The executor has to fit inside it. Today the word covers three
integration shapes that differ in who holds control:

| Shape | Who owns the loop | What nano-ros supplies | Examples |
|---|---|---|---|
| **A: nano-ros owns it** (`owned-spin`) | nano-ros: `nros::main!` emits `main()` and spins | everything | every RTOS image and every Linux example |
| **B: a host drives it** | the host's loop or task | a library surface the host calls | tokio, async-std, Embassy (async task), Arduino `loop()`, PX4 work queue, ArduPilot scheduler, Qt, GLib, an epoll app |
| **C: the framework owns scheduling** | the framework, including priorities and the reset vector | generated glue that maps callbacks onto framework tasks | RTIC |

Shape C is the only shape that needs generated code. RTIC fixes task
priorities and checks shared resources at compile time, and a callback must be
handed to an RTIC task (Deferred dispatch). The `#[rtic::app]` module, the task
slots and `RticBoardEntry` all have to be emitted. A Shape B host needs no
generated code. It needs a stable way to drive the executor, to wake it and to
know when to call it next.

`zephyr` and `esp32` are not frameworks in this sense. They are the platform's
own entry shape (`main` on a Zephyr thread, `app_main` under ESP-IDF), fixed by
the board. They sit in `nros_orchestration_ir::FRAMEWORKS` only because the
macro dispatches on that list (D5).

## What it costs today

Measured on main, 2026-10-29:

| Framework | Code in nano-ros | In-tree board | CI cell |
|---|---|---|---|
| `owned-spin` | the baseline | all of them | all of them |
| `rtic` | ~313 macro lines, `RticBoardEntry`, ~414 lines of board `rtic.rs` | `mps2-an385` | 4 run-plan tests and 8 examples |
| `embassy` | ~136 macro lines, `EmbassyBoardEntry` | none (removed by phase-337 W7.a) | none |
| `zephyr`, `esp32` | macro branches | yes | yes (esp32 dormant, issue 1525) |

`main_macro.rs` is 6,777 lines. The framework branches are a minority of it,
but each one is a whole entry shape that only its own CI cell can check.

## Decisions

### D1 — Three tiers, and each has a different promise

| Tier | What nano-ros ships | What it promises | What proves it |
|---|---|---|---|
| **0: host-driven contract** | public executor API (D2) and recipes (D4) | the API's behaviour, on every platform | one CI cell per recipe, which tests the contract and not the framework |
| **1: in-macro framework** | an `nros::main!` branch, a board-entry trait, an in-tree board | that the framework boots and delivers on the named board | its own CI cell on that board |
| **2: out-of-tree** | nothing specific to the framework | the Tier 0 contract, plus the out-of-tree route for crate metadata (issue 0415) | the integrator's own CI |

A request to support a new framework is answered with Tier 0 first. Tier 1 is
only for a framework that Tier 0 cannot serve (D3, criterion 3).

### D2 — The Tier 0 contract

The contract has four parts. Each row records what exists on main today and
what is missing:

| Part | What a host needs | On main today | Gap |
|---|---|---|---|
| **Drive** | run ready work without giving up the thread | `spin_once(timeout)`, `spin_some(max)` (`spin_some(ZERO)` drains without blocking) | none, apart from stability (D2.1) |
| **Wake in** | make a parked or later spin notice new work, from a thread, an ISR or a signal handler | `wake()` (any context, `AtomicBool`); `wake_handle()` (needs `alloc`); `signal_fd()` (Linux, `signal-fd-wake` feature) | none for waking. But `signal_fd()` is a WRITE end for signal handlers, not a readable readiness fd, so a GLib or Qt loop cannot watch it. That is the next row's gap |
| **Readiness out** | learn that work arrived, so the host can schedule a spin | per-entity `register_waker` on the RMW traits, used by the C polling FFI (`c_waker.rs`) | **missing at executor level.** No single "this executor has ready work" notifier exists: no `Waker`, no callback, no readable fd. `spin_async()` wakes ITSELF on every poll, after a `spin_once(1 ms)` that can block the async executor's thread. Under Embassy or tokio that is a busy loop that also blocks |
| **Time out** | know when the executor next needs to run, so the host can sleep exactly that long | internally, `next_wake_bound_us` is the minimum over timers, backend, routes and wake sources (RFC-0106). Publicly, only `route_next_due_us()` and the per-timer `timer_time_until_next_call_ns(id)` | **missing.** `next_wake_bound_us` is `pub(crate)` |
| **Defer** | run a callback in the framework's own task instead of inline | `register_dispatch_slot` / `dispatch_callback`, as used by the RTIC emit | **coupled to the macro.** The slot ABI is the per-node `__nros_node_<pkg>_on_callback` that `nros::node!` emits, so a hand-written host cannot use it without the macro |

**D2.1 — Stability.** The Drive, Wake-in and (once added) Time-out and
Readiness-out functions become the documented integration surface, under a
book chapter of their own. A breaking change to them is a `.breaking.md`
changelog entry and a recipe update in the same change.

**D2.2 — Closing the gaps.** This is work for a phase to pick up. The shape is
recorded here so that it is decided once:

- **Time out:** `Executor::next_deadline_us() -> Option<u64>`, a public wrapper
  over `next_wake_bound_us` with no caller budget. `None` means nothing is
  scheduled, so the host waits on readiness alone.
- **Readiness out:** one executor-level hook,
  `set_ready_notifier(fn(*mut c_void), ctx)`, that the executor calls when any
  RMW entity's waker fires. It is built on the existing per-entity
  `register_waker` and has the same rules: callable from any context, must not
  block and must not re-enter. On top of it:
  - a `Waker`-based `spin_async()` that returns `Pending` until notified, which
    replaces the self-waking loop;
  - an optional readable fd on Linux for GLib, Qt and epoll loops.
- **Defer:** a hand-callable form of the dispatch ring (enqueue in the RMW
  callback, drain in the host's task) that does not depend on the
  macro-emitted node ABI. Until it exists, deferred dispatch stays a Tier 1
  feature.

### D3 — Tier 1 admission: four criteria, all required

A framework gets an `nros::main!` branch only if:

1. **an in-tree board** boots it;
2. **a CI cell** on that board boots the image and delivers messages (a cell
   that only builds is not enough);
3. **Tier 0 cannot serve it.** The framework must need compile-time task,
   priority or resource wiring that a recipe cannot express. RTIC needs this.
   A framework that only needs "call spin from my task" does not;
4. **a named owner** answers its reds.

The criteria are re-checked whenever a framework's board or CI cell goes away.
A branch that fails them is demoted to a Tier 0 recipe, or deleted. It is not
kept as a seam that nothing exercises. Today:

- **`rtic`** meets all four. It stays in Tier 1.
- **`embassy`** fails 1, 2 and arguably 3. An async task can drive the
  executor once D2.2's notifier exists. Whether to demote it is a maintainer
  decision, recorded as issue 2134.

### D4 — Recipes, not branches

A Tier 0 integration is a copy-out example leaf (RFC-0026), not an
`nros::main!` branch. It is about 30–80 lines of user code that open the
executor, register nodes and hand the drive, wake and timing calls to the host.
The first recipes are the ones with an existing user or an in-tree precedent:

- **tokio:** `examples/linux/rust/service-client-async` is already most of
  this.
- **A plain C `while` loop:** `spin_some(ZERO)` plus a sleep until the next
  deadline (C has no `nros::main!`, so this is how C images that already have
  a main loop integrate today).
- **GLib or epoll:** after the readiness fd exists.
- **Embassy:** if issue 2134 demotes it.

Each recipe gets one fixture row and one runtime cell, keyed on the CONTRACT
coordinate rather than a new framework coordinate. A recipe that breaks shows
that the contract broke.

### D5 — The framework vocabulary separates frameworks from platform entry shapes

`FRAMEWORKS` keeps only Tier 1 values that a user may choose. Today those are
`owned-spin` and `rtic`, plus `embassy` until issue 2134 is ruled.

`zephyr` and `esp32` become platform entry shapes that the board descriptor
derives. A user never writes them, and RFC-0111's `framework` field refuses
them. The macro still dispatches on them, but it reads them from the board and
not from the image field.

## Effect on RFC-0111

RFC-0111 is narrowed, not withdrawn:

- `[image.<id>] framework` stays and takes Tier 1 values only (D5).
- The board descriptor's `frameworks` list and `entry_types` map are generic
  machinery for a choice that exactly one board has today. RFC-0111 keeps them,
  but their schema is limited to Tier 1 names. A Tier 0 recipe never appears in
  a board descriptor.
- RFC-0111's first open question, the default for a multi-framework board,
  becomes small. With `zephyr` and `esp32` out of the vocabulary, every board
  except `mps2-an385-baremetal` has exactly one framework.

## Alternatives considered

- **A macro branch per framework, on demand.** This is the current trajectory.
  The cost grows linearly, and the branches go unexercised, as Embassy's
  already has. Rejected.
- **A plugin interface for `nros::main!`, with each framework's codegen in its
  own crate.** It moves the code but not the obligation: each plugin still
  needs a board and a cell before anyone can trust it. It also adds a
  proc-macro ABI that nano-ros would have to keep stable. Rejected for now. It
  could be revisited if Tier 1 grows past three or four frameworks.
- **No framework support at all, only `owned-spin`.** This rules out RTIC,
  which has a real user and a CI cell, and Shape C cannot be reached any other
  way. Rejected.

## Open questions

- **Readiness from an ISR on bare metal.** On an MCU the RMW's waker can fire
  in interrupt context. Can the notifier callback run there for every backend
  (zenoh-pico over smoltcp, XRCE over serial)? Or must it be "set a flag and
  pend an interrupt"? This needs measuring per backend before D2.2 is
  implemented.
- **Is `wake_handle()`'s `alloc` requirement needed?** `wake()` covers no-alloc
  callers that hold `&Executor`. A host task usually does not, so a
  `'static` no-alloc handle may be needed.
- **Does a C or C++ host get the same contract?** The C API has the
  per-entity wake callbacks already. The executor-level notifier should cross
  the FFI in the same change, so the contract is not Rust-only.
