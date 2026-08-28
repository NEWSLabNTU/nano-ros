---
id: 859
title: "`rust/action-server` diverges from its native copy on all four RTOS
  platforms — one copy of a portability group was edited alone"
status: open
type: bug
area: examples, testing
related: [phase-338, phase-394]
---

## Symptom

`nros-tests::example_portability copies_within_a_group_are_identical` fails on
the tier-2 lane. Four of a group's five copies disagree with `native`:

```
rust/action-server [A-scheduled]: qemu-arm-freertos    differs from native
rust/action-server [A-scheduled]: qemu-arm-nuttx       differs from native
rust/action-server [A-scheduled]: qemu-riscv64-threadx differs from native
rust/action-server [A-scheduled]: threadx-linux        differs from native
```

The gate names the remedy itself: make them identical, or record a
`KNOWN_DIVERGENCE` naming the wave that will — "silence is not an option".

## Why the shape matters

Every non-native copy differs and `native` is the odd one out, so this is one
edit to `native` that was not propagated, not four independent drifts. The
example is `A-scheduled`, so the divergence is in a body the scheduling model
is supposed to keep uniform across platforms.

`examples/native/rust/action-server` and its siblings were last touched by
`f714e6a01` (phase-394, "action CANCEL over CAN, and fix a cancel the server
lied about") — a cancel-path fix, which is exactly the kind of change that
should have landed in all five copies. That is the first place to diff, not a
confirmed cause.

## Repro

    source ./activate.sh
    just build-test-fixtures lane=tier2
    cargo nextest run -E 'test(copies_within_a_group_are_identical)'

## Fix direction

Diff `native` against any one sibling and decide which side is right. If the
platforms are deliberately behind, the `KNOWN_DIVERGENCE` entry must name the
wave that closes it — an unexplained entry re-opens this issue under a
different number.

## Provenance

Found by the first full tier-2 run in some time (2026-08-28). Pre-existing on
main; not related to the QEMU/lan9118 or clippy work landing alongside it.
