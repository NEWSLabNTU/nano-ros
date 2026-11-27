---
id: 2653
title: "The FreeRTOS C tutorial's stack walk types 73 of its 242 indirect C call sites; the other 169 are each charged every function C can hold, so the walk is twice the stack the tutorial ships"
status: open
type: tech-debt
severity: medium
area: [memory, tooling, freertos, build]
related: [issue-2400, issue-2402, issue-2550, phase-557, phase-567]
found: 2026-11-16
---

## What

The cmake road now writes `compile_commands.json` when the stack floor is
checked, so the walk can type an indirect C call by clang's function type
instead of assuming it may reach any address-taken C function. On the FreeRTOS
C talker template (`nros new --template talker --lang c --rmw zenoh`, board
`mps2-an385-freertos`) that typed **73 of 242** sites and moved the OPEN walk
from 147,060 B to **130,424 B** (floor 143,472 B).

The template ships a **65,536 B** app stack, and the image runs on it. So the
walk is still about twice the stack. The tutorial is UNMEASURED (no
`[[observed]]` row), so its build warns and never refuses.

## Why it is not fixed by raising the stack

Ruled 2026-11-16: sizing the tutorial's stack from this walk would cost every
user RAM to satisfy a measuring tool. The walk is instead held by a
`[[drift]]` row in `config/stack-floors.toml`. The drift check fails when the
walk moves out of its band, and nothing sizes or refuses a build from that
row. The row names this issue as the one that decides the image's stack.

## What is left

Find out why 169 sites stay untyped, and type them. Not yet measured: which
translation units hold them. The likely candidates are C sources compiled by a
cargo build script (zenoh-pico through `zpico-sys`, the platform shims) rather
than by cmake, which `compile_commands.json` cannot list. Check that first:
`scripts/nros-stack-floor.py` prints the typed/untyped split, and it does not
list the sites themselves.

This is the precision work the stack-floor design (issue 2400, phase-557)
needs anyway. Whether that design keeps per-image rows or moves to per-layer
stack shares is still undecided (a question with the user), so how much this
work is worth depends on that ruling. A walk that over-counts by 2x cannot size a stack, measured or
not.

## Acceptance

- The untyped sites are listed with the TU each comes from.
- Either they are typed, or each remaining one has a reason.
- The `[[drift]]` row is re-measured. A fall is only a note under issue 2400
  Ruling (C), so the drift check will not prompt this re-measure. If the walk comes within reach of the shipped stack, the
  row becomes an `[[observed]]` row with a measured peak.

## Progress (2026-11-23): deferred behind #4693

The likely home of the 169 untyped sites, C compiled by cargo build scripts
(zenoh-pico through `zpico-sys`, the platform shims), is what #4693 (issue
2658) types: it routes every build-script C compile through one wrapper that
writes `compile_commands.json`. On the FreeRTOS C talker drift row it moved the
walk -21 % and re-records that row. So this issue waits for #4693 to land, then
continues on top of it: a walker listing of the remaining untyped sites by TU
(the first acceptance item), typing or a reason for each, and a re-measure of
the `[[drift]]` row. Ruled with the manager; not started before then.

## Progress (2026-11-27): listed by TU; none is in the image

Its blocker landed (5c5c854bac types the C cargo build scripts compile), so
the listing exists now:

- **Listed by TU.** The walk records the object each function's machine
  sites were disassembled from (`Graph.site_objects`), prints the untyped
  sites by object after the bind line, and writes them all to
  `report.json` `untyped_sites` (`[{object, sites, functions}]`). It counts
  only functions the image holds: an input's function the link dropped is
  on no path, and is reported as such.
- **Measured** on a fresh `nros new --template talker --lang c --rmw zenoh`
  project, board `mps2-an385-freertos`, main 0e4afce2fa, host 133:
  - C typed: 149 of 149 linked TUs;
  - untyped indirect sites: 35 (from 169), and **all 35 are in functions
    the link dropped** (zenoh-pico's generic containers `lru_cache`,
    `sortedmap`, `fifo`/`fifo_mt`, `lifo`, `ring`/`ring_mt`,
    `cancellation`, plus four `nros_cpp` wake-callback setters and two
    encoding/serialization helpers), checked against the ELF's symbols.
    No indirect site in the image is untyped.
  - Walk 52,308 B (the `[[drift]]` row re-recorded from 52,164 B), floor
    57,544 B, under the 65,536 B app stack; an upper bound through a
    type-bound edge (`CffiSession::create_subscription -> drop_entry`).
  - Run under QEMU with the router up: app task stack peak **11,584** of
    65,536 B.

**Still open: the third acceptance item, which is a decision.** The walk is
now within reach of the shipped stack, so the acceptance says the row
becomes `[[observed]]` with that peak. But this walk is an upper bound, and
an `[[observed]]` row with `upper_bound` is stamped from its peak (issue
2783's ruling): `nros new` would give the C tutorial an app stack of about
12.8 KB (11,584 B + 10 %) instead of 64 KiB. That changes what every new C
FreeRTOS project ships, so it waits on a ruling; the row stays `[[drift]]`.
