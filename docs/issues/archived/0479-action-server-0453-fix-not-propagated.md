---
id: 479
title: "issue 0453's action-server fix landed on the native cells only — 8 embedded copies still ignore the goal payload"
status: resolved
resolved_in: 46a8fe1d3
type: bug
area: examples
related: [issue-0453, issue-0450, phase-287]
---

## Resolution (2026-08-10) — option 1, and the risk did not survive contact

Propagated. `46a8fe1d3` copied native over all eight embedded copies;
`example_portability` is **6/6**, `copies_within_a_group_are_identical`
included, so no `KNOWN_DIVERGENCE` entry was needed.

The compile fear below was unfounded and worth recording as such: the embedded
C copies **already** had `server_context_t* ctx = (server_context_t*)context;`
in `execute` — the `(void)context;` sighting was a different callback. The diff
against native was exactly 0453's four hunks with nothing platform-specific,
and the four copies per language were byte-identical to each other. So it was
one edit replicated, not eight judgment calls.

C++ needed only the loop bound (`i <= goal.order`, feedback tick at
`i == goal.order`), so an order-N goal now yields N+1 elements like its Rust
and C siblings.

**The open question in "The class" is answered: yes.** 0450's Rust `State`
(`MAX_ORDER`, the accepted-order slot, `for i in 0..=order`) is present in all
four embedded Rust copies — checked per copy, not assumed. The Rust copies
still differ from native, but only in the per-platform entry wrapper, which is
what the portability groups already model.

Acceptance is a fixture rebuild per embedded family proving each still
compiles; the freertos family was rebuilt for this issue.

## Symptom

`nros-tests::example_portability copies_within_a_group_are_identical` fails, and
it is one of the few `ci-matrix` failures that is NOT a QEMU-under-load flake —
it reproduces solo in 0.4 s:

```
c/action-server   [A-scheduled]: qemu-arm-nuttx differs from native
cpp/action-server [A-scheduled]: qemu-arm-nuttx / qemu-riscv64-threadx /
                                 threadx-linux differs from native
    Either make them identical, or add a KNOWN_DIVERGENCE entry
    naming the wave that will — silence is not an option.
```

## Cause

`5f4eda8a4` ("fix(#453): every native action cell now proves the goal payload
was delivered") rewrote the action servers so the result is computed from the
order the client actually requested, instead of a hard-coded 10. It landed on
the **native** cells only. Every embedded copy still has the old body, whose
output is independent of its input — which is precisely the defect 0453 was
filed about, still live on 8 of 10 cells.

Two distinct divergences:

**1. The loop bound (C++ only).** Native now runs `i <= goal.order`, the ROS 2
`action_tutorials` convention the Rust and C servers already followed. The four
embedded C++ copies still run `i < goal.order` with `i == goal.order - 1` for
the feedback tick, so an order-N goal yields N elements where every sibling
yields N+1.

* `examples/qemu-arm-freertos/cpp/action-server/src/main.cpp`
* `examples/qemu-arm-nuttx/cpp/action-server/src/main.cpp`
* `examples/qemu-riscv64-threadx/cpp/action-server/src/main.cpp`
* `examples/threadx-linux/cpp/action-server/src/main.cpp`

**2. The accepted-order plumbing (C).** Native gained an `accepted_order` slot
on the server context, set in `goal_callback` and read when computing the
result. The embedded C copies never added it — their `execute` still ignores the
request. The C loop bound already matches (`i <= order`), so this one is subtle:
the bound is right and the *input* is still dropped.

* `examples/qemu-arm-freertos/c/action-server/src/main.c`
* `examples/qemu-arm-nuttx/c/action-server/src/main.c`
* `examples/qemu-riscv64-threadx/c/action-server/src/main.c`
* `examples/threadx-linux/c/action-server/src/main.c`

## Why this was not just propagated

The mechanical copy is NOT obviously safe. The nuttx C copy carries
`(void)context;` where native now casts that same parameter to
`server_context_t*` — so the embedded cells may surface the callback context
differently, and a naive paste could fail to compile on four platforms. That
needs a fixture rebuild per platform to confirm, which is why it is filed rather
than guessed at.

Decide deliberately between:

1. **Propagate**, and rebuild the four embedded families to prove each compiles
   and that the action e2e cells still deliver.
2. **`KNOWN_DIVERGENCE`**, if the embedded cells genuinely cannot carry
   per-goal state — naming the wave that will fix it, which is what the test
   demands instead of silence.

Option 1 is almost certainly right: the Rust and C servers already follow the
convention, so the embedded C++ cells are the outlier, not a platform limit.

## The class

This is CLAUDE.md's "fix the CLASS, not the reported site" with the sites being
copies of one file across a platform axis. `example_portability` exists to catch
exactly this and did — it just cannot catch it until someone runs the sweep.
Worth checking whether 0453's sibling fixes (0450's Rust `State`) reached every
copy either.

## Reproduce

```sh
source ./activate.sh
cargo nextest run -p nros-tests --test example_portability \
    copies_within_a_group_are_identical
```

0.4 s, deterministic.
