---
id: 1761
title: "Four Zephyr native_sim XRCE cells (C/C++ pubsub and action) fail the heap-headroom gate: the listener and action client keep 10-14 KiB of a 64 KiB arena, floor 24 KiB"
status: resolved
type: bug
area: [zephyr, xrce, testing]
severity: medium
found: 2026-10-09
related: [1424, 1033, 1757, phase-481]
---

## Measured

Distrobox `ros2`, Zephyr 3.7, native_sim/native/64, solo runs
(`cargo nextest run -p nros-tests --test zephyr -j1 --retries 0 <case>`), with
the issue-1424 platform heap gate on. Delivery succeeds in every one of them; the
gate then refuses the receiving side:

| cell | refused image | headroom | peak / arena |
|---|---|---|---|
| `example_e2e::case_20_xrce_c_pubsub_e2e` | c/listener (xrce) | 13,968 B | 52,080 / 66,048 |
| `example_e2e::case_21_xrce_cpp_pubsub_e2e` | cpp/listener (xrce) | 13,968 B | 52,080 / 66,048 |
| `example_e2e::case_26_xrce_c_action_e2e` | c/action-client (xrce) | 10,432 B | 55,616 / 66,048 |
| `example_e2e::case_27_xrce_cpp_action_e2e` | cpp/action-client (xrce) | 10,432 B | 55,616 / 66,048 |

The sending side passes (talker 47,248 B spare; action server 33,616 B). The
gate's own advice is `CONFIG_NROS_ZEPHYR_HEAP_SIZE >= 76656` (listener) and
`>= 80192` (action client).

## Not caused by phase-481 W3 -- measured both ways

The same four cells give the SAME numbers, byte for byte, on `origin/main`
`ea333ea179` built with the pre-phase-481 conf state (the module's image hook
bypassed, so `prj.conf;prj-xrce.conf;<nsos>` merges exactly what it did before
W1). The W3 images' `.config` is identical to that build's (phase-481 W3
results). So this predates W3, and predates W1's hook too.

Why nobody saw it: issue 1424's gate landed 2026-10-02 and was run on two zenoh
pubsub cells only ("cells other than the two pubsub cells above were not run with
the gate"); from 2026-10-07 to W3 no Zephyr XRCE image built at all (issue 1757).

## Resolution

Resolved 2026-10-09 in phase-481 W3, the way issue 1424 asks: each of the four
leaves' `[image.zephyr_xrce]` gains `NROS_ZEPHYR_HEAP_SIZE = "98304"` (the knob
phase-481 moved into `system.toml`), with the dump recorded beside it. All four
cells then pass solo; e.g. `case_26_xrce_c_action_e2e`:

    HEAP HEADROOM: ok -- 43200 bytes spare (peak 55616 of 98816, floor 24576).

96 KiB rather than the gate's bare minimum (76,656 / 80,192) because one dump
cannot show run-to-run variance and the floor exists for exactly that margin.

Not answered here: whether a C/C++ XRCE listener should need ~52 KiB at all
when its session caps are already stated at their minimum (issue 1033). That is
a footprint question, not a correctness one, and the gate now measures it on
every run.
