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
3. **[OPEN — last blocker to green; CONF-ONLY RULED OUT] the typed carrier
   opens the DDS session at t≈0, before net_config runs at all.**
   `ddsi_udp_create_conn: failed to bind to ANY:0` → `dds_create_participant
   returned -1`. Cyclone's `session_open` logs at `00:00:00.000` — the FIRST
   app output, with NO `net_config: Initializing network` line before it. So
   the participant is created before the net stack has any interface with an
   IPv4 address (Zephyr's `bind(INADDR_ANY)` needs one → EADDRNOTAVAIL).
   Two conf attempts were cleanly REFUTED: (a) a static IP alone, and (b) the
   full net_config blocking set (`AUTO_INIT=y` + `NEED_IPV4=y` +
   `MY_IPV4_ADDR=10.0.2.15` + `INIT_TIMEOUT=30`) — both still bind-fail at
   t≈0 because the carrier beats net_config's SYS_INIT. This is NOT a conf
   problem. The ASI image publishes because its app owns
   `network_config.cpp`, which adds the IP and BLOCKS on a net-mgmt
   `L4_CONNECTED` semaphore BEFORE `run_components` runs — its `session_open`
   lands at `00:00:00.157`, after the iface is up. The fix must give the
   Zephyr typed carrier (`ZephyrBoard::run_components`) — or the example — an
   equivalent "wait for network up" step before the first entity create.
   Investigate the carrier's init ordering vs net_config's APPLICATION-level
   SYS_INIT; a carrier-side wait would fix every cyclone-on-Zephyr example at
   once. Net-stack init ordering, distinct from every wall above and from a
   conf tweak.

## Remaining fix direction

- Make the DDS session open AFTER the network is up. Conf-only is RULED OUT
  (finding #3): the typed carrier creates the participant at t≈0, before
  net_config's SYS_INIT — a static IP and the full net_config blocking set
  both still bind-fail. The carrier (`ZephyrBoard::run_components`) or the
  example needs a "wait for L4/iface up" step before the first entity create,
  the way ASI's `network_config.cpp` blocks on an `L4_CONNECTED` semaphore
  before `run_components`. A carrier-side wait fixes every cyclone-on-Zephyr
  example at once. Plus an IP config (static SLIRP `10.0.2.15` or
  `CONFIG_NET_DHCPV4=y`). Then the existing `fvp_runtime.rs` publish assertion
  passes.
- ~~Raise `fvp_runtime_rust.rs` from the boot-banner pattern to
  `nros_tests::output::TALKER_LOG_PREFIX` (`"Publishing:"`)~~ — **DONE** (the
  assertion + boot-banner sanity check + rename `..._boots` → `..._publishes`;
  parity with the cpp lane).
- The run recipes delegate to `west fvp run`, which needs the `fvp` west
  extension registered in the workspace (absent on this host — the recipe
  errored "unknown command fvp"); a direct-FVP fallback or documenting the
  extension provisioning would make the lane host-portable.
- ~~Speed: override `cache_state_modelled=1` via
  `ARMFVP_EXTRA_FLAGS="-C cache_state_modelled=0"`~~ — **DONE** (both cyclone
  FVP build recipes export it before `west build`, so it bakes into the
  configure-time `run_armfvp` target; scoped to AEMv8-R, NOT the S32Z recipe
  whose model would reject the flag).

Remaining (model-host, the active phase-292 session): the talker net-config
+ iface-up wait (finding #3), and the `west fvp` extension provisioning /
direct-FVP fallback (bullet 1 above). The assertion + speed halves are
landed.

Only when the talker actually publishes on the model is the lane a real
regression gate — until then it stays a skip-only false green.
