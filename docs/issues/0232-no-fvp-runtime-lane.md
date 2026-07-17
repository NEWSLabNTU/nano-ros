---
id: 232
title: "No FVP runtime lane — cyclone-on-Zephyr-hardware regressions invisible (build smoke only)"
status: open
type: tech-debt
area: testing
related: [phase-292]
---

## Summary

`just zephyr build-fvp-aemv8r-cyclonedds` is a BUILD smoke; nothing ever
RAN on the FVP model until phase-292 W2. Result: walls #4/#5/#8/#9
(snippet conf never merged on 3.7, loopback getifaddrs, missing
descriptor codegen, mutex-pool exhaustion) all shipped invisible and were
found by the ASI consumer.

## Fix direction

Add a `just zephyr run-fvp-talker` recipe: skip cleanly when
`FVP_BaseR_AEMv8R` is not on PATH / `ARMFVP_BIN_PATH` unset (license-gated
download), else boot the talker image single-shot
(`cache_state_modelled=0`, per-UART out_file, timeout) and assert the
uart reaches `dds_create_participant returned <positive>` — that single
gate would have caught every wall above. Wire into the zephyr CI group
behind the model-present check like the SDK-gated lanes.

Ops note for whoever writes it: with `cache_state_modelled=1` (Zephyr's
default board.cmake flag) the model fast-forwards idle time but crawls
~1000x under busy code — runtime lanes must pass `=0`.
