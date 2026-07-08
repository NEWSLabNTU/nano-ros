---
id: 161
title: "zephyr-native-cyclonedds nextest group serialized (max-threads 1) — restore per-role-set Cyclone domain bake to re-parallelize"
status: open
type: tech-debt
area: testing
related: [issue-0157, phase-177]
---

## Summary

The `zephyr-native-cyclonedds` nextest group runs `max-threads = 1` since
#157: every zephyr+cyclonedds fixture image currently bakes
`NROS_CYCLONE_DOMAIN_ID=0`, so concurrent test pairs share one SPDP multicast
port (native_sim NSOS forwards no `SO_REUSEADDR`) and cross-talk — the rust
pubsub/service tests flaked nondeterministically under the parallel group
(the "177.39 class" group-load flakes were this).

Phase 177.37 established the fix pattern: bake a DISTINCT Cyclone domain per
role-set (per test pair), so parallel pairs discover only their own peer. That
bake is not in the current fixture rows.

## Work

- Add per-role-set `NROS_CYCLONE_DOMAIN_ID` values to the zephyr cyclonedds
  fixture rows (talker/listener pair, service pair, boot singletons), mirroring
  177.37.
- Restore `max-threads = 4` on the `zephyr-native-cyclonedds` group in
  `.config/nextest.toml` (the serialization comment there points here).
- Prove: 3 consecutive parallel full-family runs of `phase_118_collapse` 8/8.

## Cost of the status quo

Correctness is covered (serialized group is deterministic 8/8); this is
wall-clock only — the family takes ~23 s serialized vs ~8 s parallel.

## References

`.config/nextest.toml` (`[test-groups.zephyr-native-cyclonedds]` comment),
archived issue 0157, phase-177 (177.37/177.39).
