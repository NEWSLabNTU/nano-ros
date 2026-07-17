---
id: 232
title: "No exercised FVP runtime lane — cyclone-on-Zephyr-hardware regressions invisible (skip-only gate)"
status: open
type: tech-debt
area: testing
related: [phase-292]
---

## Summary

The cyclone-on-Zephyr-FVP path has a runtime test (`fvp_runtime.rs` asserts
the talker's `Publishing:` line, `fvp_runtime_rust.rs` asserts only the boot
banner) — but it was NEVER exercised: the ARM FVP is license-gated, so on
every CI/dev host the test `skip!`s. That is a FALSE green. Result: walls
#4/#5/#8/#9 (snippet conf never merged on 3.7, loopback getifaddrs,
missing/mis-named descriptor codegen, mutex-pool exhaustion) all shipped
invisible and were found by the ASI consumer.

## Findings from actually running it (2026-07-18, FVP now on hand)

Booting the cpp/cyclonedds talker on `FVP_BaseR_AEMv8R` 11.31.28 uncovered a
STACK of gaps behind the false green, peeled in order:

1. **[FIXED] Two descriptor-codegen bugs** in `scripts/cyclonedds/
   msg_to_cyclone_idl.py`, both exposed by the phase-292 W2 module-path
   descriptor codegen (the ASI consumer used clean absolute paths + real
   package names so never hit them): (a) an absolute `--interface` arg
   skipped `.resolve()` while `pkg_dir.resolve()` followed the west-workspace
   `nano-ros` symlink → `relative_to` ValueError; (b) the scratch dir copied
   the caller's `package.xml` so rosidl_adapter named the IDL module after
   the caller — an example bundling `std_msgs/String` under its own pkg then
   emitted the descriptor as `<example>_msg_dds__String__desc` while the
   register TU used `std_msgs_...` → link undefined-reference. (commit on
   main; talker now generates + links clean.)
2. **[MITIGATED] cpp talker built with NO cyclone RMW conf.** Unlike the rust
   sibling / the `build-fvp-ws-entry` workspace lane, the cpp talker uses
   `find_package(Zephyr)` + `-b` directly (the module-verb path), NOT
   `nano_ros_use_board`, so wall #4's snippet-conf merge never ran → mutex=32,
   worker stacks=2048, no waitset socketpair → **SMP-4 crash inside
   `dds_create_participant`**. Adding `nano_ros_use_board` to the example
   breaks its `nros_generate_interfaces` module verb, so the fix is
   recipe-level: `build-fvp-aemv8r-cyclonedds` now layers the snippet conf via
   `-DEXTRA_CONF_FILE=zephyr/snippets/nros-cyclonedds/cyclonedds.conf`
   (mutex=1024, stacks=32K/4K, socketpair). No more SMP-4 crash.
3. **[OPEN — last blocker to green] the talker cannot create a participant on
   the FVP's default SLIRP networking.** `ddsi_udp_create_conn: failed to
   bind to ANY:0` → `dds_create_participant returned -1`. The example ships no
   IP config; under `bp.hostbridge.userNetworking=1` (what `armfvp.cmake`
   auto-adds for `CONFIG_ETH_SMSC91X`) the guest has no routable address, so
   the getifaddrs walk (wall #5) finds none and bind fails. The ASI image
   published because its tap profile assigned a static `192.168.10.x`
   (`CONFIG_NET_CONFIG_MY_IPV4_ADDR`). The talker example needs the same — a
   static IP (or `CONFIG_NET_DHCPV4=y`, since SLIRP runs a DHCP server) — for
   the runtime lane to reach `Publishing:`.

## Remaining fix direction

- Give the talker example a network config so it gets a routable IP on the
  FVP (static addr + the smsc overlay, or DHCPv4 for SLIRP). Then the
  existing `fvp_runtime.rs` publish assertion passes.
- Raise `fvp_runtime_rust.rs` from the boot-banner pattern to
  `nros_tests::output::TALKER_LOG_PREFIX` (`"Publishing:"`) — parity with the
  cpp lane (the rust talker already logs `Publishing:`).
- The run recipes delegate to `west fvp run`, which needs the `fvp` west
  extension registered in the workspace (absent on this host — the recipe
  errored "unknown command fvp"); a direct-FVP fallback or documenting the
  extension provisioning would make the lane host-portable.
- Speed: `west fvp run` inherits the board.cmake `cache_state_modelled=1`
  (aarch64) which is ~1000x slower under busy code; the runtime lane must
  override via `ARMFVP_EXTRA_FLAGS="-C cache_state_modelled=0"` (appended
  last by `armfvp.cmake`, FVP takes last-wins).

Only when the talker actually publishes on the model is the lane a real
regression gate — until then it stays a skip-only false green.
