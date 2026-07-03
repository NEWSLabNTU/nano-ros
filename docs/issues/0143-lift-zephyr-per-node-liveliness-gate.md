---
id: 143
title: "Lift the Zephyr per-node-liveliness gate now that the #139 socket-timeout root cause is fixed"
status: open
type: tech-debt
area: rmw-zenoh
related: [phase-276, issue-0139]
---

## Summary

Resolving #129, per-node NN liveliness declares were gated OFF on Zephyr
(`#[cfg(feature = "platform-zephyr")]` early-return in
`nros-rmw-zenoh/src/shim/session.rs::ensure_node_liveliness`; the #104
primary token stays). The observed "deadlock" was later root-caused (#139)
to the zenoh-pico Zephyr 5000 ms socket timeout starving ALL tx under
Zephyr's per-fd zsock serialization — the liveliness declare was merely the
first tx to hit the window. With the timeout now 100 ms (fork commit
`d53df344`), the gate is very likely treating a symptom that no longer
exists, and it costs graph fidelity: multi-node Zephyr images advertise only
the primary node, so per-node `ros2 node list` / lifecycle discovery of
secondary nodes is degraded.

## Plan

1. Revert the gate (drop the `platform-zephyr` early-return + the paired
   `cfg_attr(allow(dead_code))` shims).
2. Rebuild the zephyr west-lane workspace images (purge the build dirs'
   `zpico-sys` cargo fingerprints — header/feature changes are otherwise
   missed) and rerun the seven zephyr entry e2es.
3. If boots stay ~3 s and all e2es stay green, land; if the wedge
   reappears, capture a `thread apply all bt` (the #139 gdb recipe) before
   re-gating — it would mean a second, distinct tx-path defect.
