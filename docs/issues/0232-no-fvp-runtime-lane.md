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
3. **[OPEN — last blocker to green] the talker creates its participant before
   the network interface is up.** `ddsi_udp_create_conn: failed to bind to
   ANY:0` → `dds_create_participant returned -1`, always at ~15 ms. Adding a
   static IP to the example (`CONFIG_NET_CONFIG_MY_IPV4_ADDR="10.0.2.15"` +
   netmask/gw matching the SLIRP `10.0.2.0/24` subnet, `NEED_IPV4=y`) did
   NOT fix it — the bind still fails at 15 ms, i.e. the participant is created
   before `net_config` has brought the interface up and applied the address.
   The ASI image published because its boot sequence explicitly WAITS for the
   interface ("Waiting interface 1 to be up") before opening the DDS session;
   the talker example's typed carrier opens it immediately at boot. So the fix
   is two-part: (a) an IP config (static or `CONFIG_NET_DHCPV4=y`), AND (b) the
   example must block on the interface being up (or on
   `CONFIG_NET_CONFIG_INIT_TIMEOUT` with `NET_CONFIG_NEED_IPV4`) before the
   first `nros::init`/participant create. This is net-stack init ordering,
   distinct from every wall above.

## Remaining fix direction

- Give the talker example a network config (static addr matching the SLIRP
  `10.0.2.0/24` subnet, or `CONFIG_NET_DHCPV4=y`) AND make it wait for the
  interface to be up before the first participant create (finding #3 above —
  a static IP alone did not suffice; bind still raced net_config at 15 ms).
  Then the existing `fvp_runtime.rs` publish assertion passes.
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
