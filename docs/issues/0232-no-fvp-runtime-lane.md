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
3. **[OPEN — last blocker to green] the talker example is a BUILD-ONLY image
   with no working network; retrofitting it into a runtime image is a
   mini-project, not a conf tweak.** `ddsi_udp_create_conn: failed to bind to
   ANY:0` → `dds_create_participant returned -1`. (An earlier note here blamed
   carrier-vs-net_config init ordering — that was WRONG; the `session_open` at
   `00:00:00.000` is just fast-sim timestamp clustering under
   `cache_state_modelled=0`. The entry is a plain `int main()`, runs after all
   SYS_INIT.) The real chain, peeled on the FVP:
   - The generated devicetree has `ethernet@9a000000 { status = "disabled" }`
     — the example has **NO ethernet device at all**. It uses
     `find_package(Zephyr)+-b` (not `nano_ros_use_board`) and ships no DTS
     overlay, and the nros board crate's overlay only enables the UARTs (its
     ethernet block is a stub: "users override at the example-app level"). No
     device → no `net_if` → `bind(INADDR_ANY)` → EADDRNOTAVAIL.
   - Adding an app overlay (`boards/<board>.overlay`, auto-discovered) that
     enables `&eth`/`&phy`/`&mdio` (exactly ASI's overlay) got the node
     `status = "okay"` but then **crashed the smsc91x driver at init**
     (`smsc_select_bank` → `sys_write16` to a null register base via
     `mdio_smsc_write`, Data Abort FAR=0x0e, before any banner). The bare
     overlay is insufficient — the ASI image carried more (net L2 / driver /
     mdio DTS wiring beyond the three `status=okay` lines) that the talker
     lacks. Diagnosing the driver's null base is a further DTS layer.
   Retrofitting the build-only talker into a working networked FVP image thus
   means recreating ASI's full network setup piece by piece (eth device DTS +
   driver config + IP + net-up), each step revealing the next — a mini-project.
   **Better target:** point the runtime lane at `build-fvp-ws-entry`
   (`examples/workspaces/ws-realtime-cpp-fvp/src/fvp_entry`, phase-292 W1.a),
   which uses the FULL canonical ASI-shaped consumption
   (`nano_ros_use_board(fvp-aemv8r-smp)` + `find_package(nano_ros)` + a
   `nano_ros_add_executable(BOARD zephyr LAUNCH …)`) — the exact shape ASI
   runs, PROVEN to publish end-to-end on this model (the closed-loop demo).
   Give ws-entry a run recipe + a `Publishing:`/participant assertion instead
   of retrofitting the legacy talker.

## Remaining fix direction

- Retarget the runtime lane at `build-fvp-ws-entry` (finding #3): the legacy
  cpp/rust talkers are build-only images with no ethernet device — enabling it
  naively crashes the smsc91x driver. `ws-realtime-cpp-fvp/src/fvp_entry`
  already uses the full canonical `nano_ros_use_board` consumption (ASI's exact
  shape, proven to publish on the model), so it should have the working network
  wiring. Add a `run-fvp-ws-entry` recipe + a `fvp_runtime_ws.rs` test asserting
  participant-create / `Publishing:`, and retire the talker runtime tests
  (`fvp_runtime.rs` / `fvp_runtime_rust.rs`) or downgrade them to boot-banner
  build proofs. That avoids reconstructing ASI's network setup inside a legacy
  build-only example.
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
