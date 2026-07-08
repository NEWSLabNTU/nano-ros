---
id: 161
title: "zephyr-native-cyclonedds nextest group serialized (max-threads 1) — restore per-role-set Cyclone domain bake to re-parallelize"
status: resolved
type: tech-debt
area: testing
related: [issue-0157, phase-177]
---

## RESOLVED (2026-07-08) — knob split-brain fixed; group parallel again (8/8 ×3, ~6 s vs ~23 s)

The 177.37 bake machinery was never lost — `zephyr-fixture-leaves.sh` still
passes `-DCONFIG_NROS_DOMAIN_ID=$((50 + lang_idx*3 + variant_idx))` for every
cyclonedds row. TWO later regressions defeated it:

1. **Kconfig knob split-brain (C/C++ images)**: phase-180.C introduced a
   separate `CONFIG_NROS_CYCLONE_DOMAIN_ID` (default 0) that the cyclone
   backend actually consumes, and pinned it to 0 in the `nros-cyclonedds`
   snippet + every `prj-cyclonedds.conf` (20 files) + the nros CLI's generated
   conf. The driver's `NROS_DOMAIN_ID` bake became a no-op for the backend.
   Fix: `NROS_CYCLONE_DOMAIN_ID` now **defaults to `NROS_DOMAIN_ID`** (set it
   explicitly only when DDS and ROS domains must intentionally differ) and all
   `=0` pins are gone. This was also a user-facing footgun: setting
   `CONFIG_NROS_DOMAIN_ID=5` on a cyclone image silently kept DDS on domain 0.
2. **Rust macro dropped the domain (Rust images)**: the example `build.rs`
   bakes `CONFIG_NROS_DOMAIN_ID` into the `NROS_DOMAIN_ID` rustc-env (phase
   225.P) and its comment promises `nros::zephyr_component_main!` reads it —
   but the phase-277 macro rework dropped that consumption, so every Rust
   cyclone image ran `domain=0` regardless of the bake (confirmed in the boot
   log: `session_open: domain=0`). Fix: the macro reads the baked value again
   (panics on a non-numeric bake) and sets `ExecutorConfig::domain_id`.

With both fixed the images bake domains 50–58 (pairs share, sets differ),
`max-threads` is back to 4, and `phase_118_collapse` runs **8/8 across three
consecutive parallel runs in ~6.1 s** (vs ~23 s serialized).

Fallout discovered by the full-sweep rebuild (tracked separately in **#163**):
pure-Rust zephyr images carry NO zenoh/xrce backend at all — #155's strong
RUST-API register stub made every `rs-*-zenoh` image a hard LINK error, which
broke the whole `build-fixtures` sweep. Interim: the stub's extern is now
`__attribute__((weak))` + null-guarded, so images link and a backend-less
image still fails loudly at `Executor::open`.

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
