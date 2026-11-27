# Phase 592 — Stack-floor walk precision

**Status (2026-11-27). OPEN: RFC-0143 ruled 2026-11-27; W2 and W3 run in
parallel (Q2), nothing started.** Implements
[RFC-0143](../design/0143-stack-walk-precision.md). Closes
[issue 2550](../issues/2550-stack-walk-order-dependent-under-cycles.md) and
[issue 2658](../issues/2658-stack-floor-walk-over-estimates-graph-and-param-images.md).
Follows [phase-557](phase-557-stack-floor-measured-frames.md), whose W7f
(field-sensitive binding) stopped at its checkpoint and reopens here as W3,
alongside W2 (ruled 2026-11-27, RFC-0143 Q2).

## Baseline

Recorded before W1, so each item's effect is a measured delta:

- [issue 2998](../issues/archived/2998-stack-floor-aarch64-c-untyped-mabi-lp64.md)
  (aarch64 C typed 4 of 387 TUs, because clang rejects GCC's `-mabi=lp64`)
  lands on its own, before W2/W3; its fix is measured on the qemu_cortex_a53
  fixture (387 of 387 typed, floors 20-23 % lower), so the baseline is taken
  after it.

- the reachable SCC size, and the 8-order and 64-order walks, of the two
  issue-2658 images (Nucleo Rust action server; realtime-c `[image.freertos]`
  OPEN and TASK_HIGH) and of every `[[observed]]` row in
  `config/stack-floors.toml`.

## Work items

### W1 — Report the SCC

- **Owner:** unassigned.
- **Status:** OPEN
- `report.json` and the walk's summary line carry, per root, the size of the
  largest SCC the root reaches and its frame sum, so every later item's
  effect is visible in an ordinary build.
- **Acceptance:** the baseline above, read off ordinary builds' reports.

### W2 — Rust `dyn` sites bind to live vtables (RFC-0143 D1)

- **Owner:** unassigned.
- **Status:** OPEN
- Read vtables and their slot functions from the IR, and their referrers
  from the linked objects' relocations; bind a `dyn` site in slot S to slot S
  of the vtables live code references.
- **Acceptance:** a walker self-test where a trait with two implementations,
  one never made into a trait object, binds to one; the baseline images
  re-measured (SCC size, walk) and recorded in issue 2658.

### W3 — C sites bind through their field or parameter (RFC-0143 D2)

- **Owner:** unassigned.
- **Status:** OPEN
- In parallel with W2 (RFC-0143 Q2, ruled 2026-11-27); it does not wait on
  W2's measurement. Nested initialisers, the return-value channel, and the parameter channel; a cast,
  `memcpy` or escape keeps the coarse binding.
- **Acceptance:** the share of C sites binding through a trusted channel, on
  the baseline images, against W7f's 37-43 %; the walks re-measured.

### W4 — The exact search under a budget (RFC-0143 D3)

- **Owner:** unassigned.
- **Status:** OPEN
- SCC condensation; exact DAG depths outside SCCs; `deepest_simple` inside
  them, under a node budget per root; a spent budget refuses on an image
  with an `[[observed]]` row and warns on an unmeasured one, reporting the
  budget and the SCC size (Q3, ruled);
  true recursion refused by name. `WALK_ORDERS` retires.
- **Acceptance:** issue 2550's four-node self-test reads 121 B (the true
  depth), and the same image gives one depth for any callee order; every
  `[[observed]]` row's walk finishes within budget, at or above its measured
  peak.

### W5 — Per-task roots (issue 2658)

- **Owner:** unassigned.
- **Status:** OPEN
- Root each sched task's role at its own `__nros_entry_setup_sched_task_<i>`
  callbacks instead of the shared runner entry, once W2-W4 make the shared
  edges narrow enough for it to matter.
- **Acceptance:** realtime-c and realtime-rust TASK_HIGH floors re-measured,
  each below the shared-root floor or with a reason it is not.

### W6 — Wording and rows

- **Owner:** unassigned.
- **Status:** OPEN
- Docs, warnings and `report.json` drop "heuristic floor, order-dependent";
  `[[observed]]` rows' `walk_floor` re-measured under W4 (a floor never drops
  only because the walk order changed; it may drop because the walk is now
  exact, and that is recorded with the measurement).
- **Acceptance:** `git grep -n 'order-dependent'` over docs and scripts
  finds only history; issues 2550 and 2658 resolved.
