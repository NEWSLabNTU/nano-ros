# Phase 476 — freestanding spellings for the gated C++ overloads

**Status (2026-10-02). Opened.** Carries the half of
[phase-456](archived/phase-456-cpp-api-is-a-handle-over-the-rust-arena.md) W6
that was measured unreachable when that phase closed. Same design as 456 —
[RFC-0096 D9 revision 3](../design/0096-cpp-freestanding-core-and-porting-layer.md)
— plus [RFC-0089](../design/0089-ros2-api-adoption-and-the-compile-or-conform-rule.md)'s ported-source
ergonomics, which is the surface these overloads exist to serve.

## The goal, and why it was blocked

phase-456 W6 made both C++ capability gates STRUCTURAL: a row in either baseline
is now a hard failure rather than a ratchet. Its acceptance asked for more —

> zero `NROS_CPP_HAS_*`, zero `NROS_CPP_STD`, zero `NROS_CPP_NODE_HOSTED`

— and that count could not be forced to zero, because every remaining use gates
SURFACE that ported code calls. Deleting the surface breaks the ported corpus;
moving it somewhere the gate does not look reaches zero by not looking, which
W6 itself names as worse than a ratchet.

So this phase does not delete gated surface. It gives each gated family a
**freestanding spelling** first, so the hosted overload becomes a convenience
over a freestanding one rather than the only way to say the thing — and only
then retires the gate.

## Where the count stands — re-measured 2026-10-02

Occurrences over `packages/api/nros-cpp/include`, word-bounded
(`git grep -ohw <macro> -- packages/api/nros-cpp/include | wc -l`). Word bounds
matter: they exclude the `NROS_CPP_STD_DETECT_HPP` include guard and the
`-DNROS_CPP_STD` flag named in prose, which is why phase-456 W6's table read 59
for a count this method reads as 57 at the same base.

| macro | uses | what still needs it |
| --- | --- | --- |
| `NROS_CPP_HAS_SHARED_PTR` | 10 | `Timer` aliases `std::shared_ptr` (`timer.hpp:59`, the ONE alias gate); `Node::SharedPtr` inside `NROS_CPP_NODE_HOSTED`; the rest are the definition site, the hosted conjunction and prose |
| `NROS_CPP_HAS_STD_STRING` | 9 | `get_logger(const std::string&)`; `FixedString`/`HeapString`'s `std::string` interop |
| `NROS_CPP_HAS_STD_CHRONO` | 8 | `create_wall_timer` / `create_timer`'s duration overloads; `Rate`'s `std::chrono` constructor |
| `NROS_CPP_HAS_STD_FUNCTION` | 6 | `detail::WallTimer`'s type-erasure cell; the `NROS_CPP_NODE_HOSTED` conjunction |
| `NROS_CPP_HAS_STD_VECTOR` | 4 | the `NROS_CPP_NODE_HOSTED` conjunction |
| `NROS_CPP_HAS_STD_SSTREAM` | 4 | the `RCLCPP_*_STREAM` family |
| `NROS_CPP_NODE_HOSTED` | 21 | derived from four of the above |
| `NROS_CPP_STD` | 58 | the consumer-facing opt-in, which nothing that ships defines |

Every `NROS_CPP_HAS_*` row and `NROS_CPP_NODE_HOSTED` is unchanged since
phase-456 measured them on 2026-09-25. `NROS_CPP_STD` moved by ONE: `c848ea5c99`
(2026-10-01, issues 1256/1564) gated a `<cstdio>` include in `log.hpp:35` the
same way `main.hpp:45` already does. That is a legitimate hosted-only include,
not a regression against this goal — but it is the reason the count has to be
re-measured at the start of every work item rather than carried.

## What is ALREADY freestanding — measured before writing the work items

The first draft of this phase listed "introduce a freestanding duration type" as
W1. Reading the headers refuted it: `nros::Duration` exists
(`nros/duration.hpp:69`), and two of the three chrono-gated families already use
it as their primary spelling, with the `std::chrono` overload as conversion sugar:

* `Rate` — `Rate(double)` and `Rate(::nros::Duration)` are freestanding; only
  `Rate(std::chrono::duration<…>)` is gated (`nros.hpp:1325`).
* `rclcpp::create_timer(node, clock, period, cb)` — the `::nros::Duration`
  overload is the real one; the chrono overload (`nros.hpp:921`) converts and
  delegates to it.

The one chrono-gated site with NO freestanding twin is
`Node::create_wall_timer(period, cb)` (`nros.hpp:660`, declared `node.hpp:913`)
— and it is hosted for TWO reasons, not one: its chrono parameter and its
`std::shared_ptr<Timer>` return. A `Duration` overload alone cannot free it; it
needs a freestanding timer handle to return. So the chrono work is not its own
item — it is the last step of the timer item.

## Work items

* **W1 [cpp] — string spellings.** `get_logger(const std::string&)` and the
  `FixedString`/`HeapString` interop. `nros::FixedString` exists
  (`fixed_string.hpp:39`); the work is making it, or a `const char*`, the primary
  spelling at every gated signature. *Acceptance:* `NROS_CPP_HAS_STD_STRING`
  gates only interop conversions, never a signature that is the only way to call
  something.

* **W2 [core, cpp] — timers: callback captured in the arena, a freestanding
  handle, and the duration overload.** phase-456 W1 put a subscription's capture
  in the arena; timers never got that treatment, so `Timer` still needs
  `std::function` (`detail::WallTimer`'s type-erasure cell) and aliases
  `std::shared_ptr` (`timer.hpp:59`, the one remaining alias gate). This is core
  Rust work and the largest item here. Its last step is
  `create_wall_timer(::nros::Duration, cb)` returning the freestanding handle, at
  which point the chrono overload becomes sugar like `Rate`'s and
  `create_timer`'s already are. *Acceptance:* `timer.hpp` names
  `NROS_CPP_HAS_SHARED_PTR` and `NROS_CPP_HAS_STD_FUNCTION` zero times, and
  `NROS_CPP_HAS_STD_CHRONO` gates only conversion overloads.

  The arena now has a removal path (`9768795b1d`, issue 1496 resolution 2) — a
  bounded one, 8 released regions, coalescing, first-fit — so unlike phase-456,
  a timer that moves into the arena can be destroyed for real. Wire it; do not
  repeat 456's "no removal path" reasoning, which that commit retired.

* **W3 [cpp] — `Node::SharedPtr` and the `NROS_CPP_NODE_HOSTED` block.** The
  whole block is spelled in `std::string` / `std::vector` / `std::function`, so
  it is sequenced after W1 and W2. *Acceptance:* `NROS_CPP_NODE_HOSTED` is
  derived from nothing, and is deleted.

* **W4 [cpp] — the `RCLCPP_*_STREAM` family.** Stream logging needs a
  formatter. Decide between a freestanding minimal formatter and declaring the
  family hosted-only by design — and if the latter, the decision is written
  here and the gate's refusal message names it, because "hosted-only on
  purpose" and "not yet ported" read the same in a count.

* **W5 [ci] — the gates assert zero.** Only after W1–W4: both gates assert the
  constant phase-456 W6 could not, with the mutation that reintroduces a `std`
  type in a public signature still in their selftests. `NROS_CPP_STD` stays as
  the consumer opt-in if any surface remains hosted by design (W4); the
  acceptance then reads "zero outside the documented hosted-only family", with
  that family enumerated rather than counted.

## What this phase must not do

Reach zero by deleting surface the ported corpus calls, or by moving it out of
`packages/api/nros-cpp/include` where the gate looks. Each work item names the
freestanding spelling it ADDS before the gate it RETIRES.
