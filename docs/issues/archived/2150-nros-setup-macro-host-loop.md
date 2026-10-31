---
id: 2150
title: "`nros::main!` emits assembly and loop as one `main`, so a host that owns its own loop loses the `system.toml`-driven assembly entirely"
status: wontfix  # the host-driven shape is withdrawn (RFC-0113 D1/D10)
resolved: 2026-10-31
type: enhancement
area: [macros, core, executor]
severity: low
found: 2026-10-29
related: [2134, 2143, 2144, 2145, 2146, 2161, phase-520, phase-521]
---

**Home: [phase-520](../../roadmap/archived/phase-520-setup-macro-host-loop.md)**
(retired 2026-10-31).

## What was asked

`nros::main!` emitted the assembly (components, run plan, parameters,
lifecycle, contract monitors) and the loop (a `main` for the board's entry
shape, and the spin inside it) as one unit, so an RFC-0112 Tier 0 host that
owns its loop (Embassy, tokio, GLib, an Arduino `loop()`, a PX4 work item) had
to hand-write the assembly the model already describes. The ask was a
setup-only macro, `nros::setup!`, emitting that assembly as a function.

## What was built

Phase-520 W1, W1b, W2 and W3 landed it: `nros::setup!` emits `nros_setup`,
`nros_deploy`, `nros_boot` and `nros_stop_requested`; the
`examples/linux/rust/listener-host-loop` leaf and its `HostLoop` cell deliver
through `rmw_zenohd` and drain a `dispatch = "deferred"` component from the
host loop (issue 2146's queue, via `Executor::callback_mark` /
`set_deferred_since`). W4, the Embassy recipe, was not started.

## Why it is closed without finishing

Maintainer ruling, 2026-10-31: the real-time promise is a precondition of
support. A host-driven loop decides when callbacks run, so nano-ros cannot
state response-time, jitter or priority bounds there; RFC-0112's Tier 0 is
withdrawn, and `nros::setup!`, which exists to serve that shape, is withdrawn
with it. Written as RFC-0113 "rt-unified-wait" (D1 the support rule, D7 the
retired surface, D10 `setup!`), drafted the same day and landing with
phase-521, whose W1 deletes the macro, its emit, the W3 helpers, the example,
the cell, the tests and the book text, together with `spin_async*`,
`set_ready_notifier`, `ready_fd` and the public `Executor::next_deadline_us`.

PX4 stays supported, as a Tier 1 framework (a generated mapping of tiers onto
PX4 work queues), not through `setup!`. The Embassy chapter keeps the
hand-driven shape issue 2134 left it with.

Kept for the deletion, not for the design: the phase doc's W1–W3 bullets list
every artifact that must go, so phase-521 W1 can be checked against them.
