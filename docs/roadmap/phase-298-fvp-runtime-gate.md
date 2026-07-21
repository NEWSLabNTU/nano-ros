# Phase 298 — FVP cyclone-on-Zephyr runtime gate (resolves issue 0232)

Turn the cyclone-on-Zephyr-FVP runtime path from a **skip-only false green**
into a real **maintainer pre-release regression gate**. The existing runtime
tests (`fvp_runtime.rs` / `fvp_runtime_rust.rs`) assert the talker's
`Publishing:` line but ALWAYS `skip!` (the Arm FVP is license-walled, absent on
every CI/dev host), so cyclone-on-Zephyr-hardware regressions shipped invisible
— walls #4/#5/#8/#9 (phase-292 W2) and the descriptor-codegen bugs were all
found by the external ASI consumer, not by nano-ros CI. Issue 0232 tracks the
gap; this phase closes it.

Implements the fix direction recorded on `docs/issues/0232-no-fvp-runtime-lane.md`.

## Design (brainstormed 2026-07-21)

Four decisions frame the gate:

- **Execution context — maintainer pre-release gate.** The FVP model is
  license-walled and cannot live in shared CI, so the lane is NOT an
  every-push gate. It is an authoritative check a maintainer (who has the
  model installed) runs before bumping the ASI pin or cutting a release. It
  must `skip!` cleanly for everyone without the model (skip, never false-fail).
- **Target — `ws-entry`** (`examples/workspaces/ws-realtime-cpp-fvp/src/fvp_entry`).
  The in-tree mirror of ASI's EXACT consumption shape:
  `nano_ros_use_board(fvp-aemv8r-smp)` + `find_package(nano_ros)` +
  `nano_ros_add_executable(BOARD zephyr MODEL … TYPED DEPLOY zephyr)` driving
  `ZephyrBoard::run_tiers`. It gets the board-crate ethernet fix for free and
  exercises the model/tiers path ASI runs, so it catches the regressions that
  actually reach ASI. (The legacy `talker-aemv8r` examples are single-node,
  `find_package(Zephyr)+-b` build-only images with no ethernet device — the
  wrong target; their runtime tests are retired here, W4.)
- **Assertion — self-contained publish.** ws-entry, on the FVP alone under
  SLIRP `userNetworking` (no host peer), must reach its publish loop:
  `/ctrl` + `/telem` `Publishing:` on UART. This proves participant creation +
  descriptor registration + publish — the exact chain the walls broke — with no
  tap0, no root, no host ROS 2. (The full guest→host wire path is already
  covered separately by the ASI demo closed-loop, which this session validated.)
- **Harness — Rust test driven by `west fvp run`.** A `nros_tests` test keeps
  the `skip!`→junit semantics and follows the Zephyr `west fvp` convention
  (the nano-ros `scripts/west_commands/fvp.py` extension), rather than a bespoke
  direct-FVP recipe.

### Why ws-entry does not yet publish (the two remaining blockers)

The board-crate ethernet fix (phase-292 W2, commit `b1876bbd2`) landed the eth
device; ws-entry now boots → eth up → binds a routable IP → creates the Cyclone
participant, then fails. Root causes, both proven on the model:

1. **Thread pool too small.** `zephyr/nros_platform_zephyr_shims.c` hardcodes
   `NROS_ZEPHYR_MAX_THREADS 8` (a static `K_THREAD_STACK_ARRAY` pool). Cyclone's
   worker set — recv, tev, gcreq, lease, dq, listen, threadmon — exhausts it:
   `create_thread: tev: ddsrt_thread_create failed` / `tid … is in use!` →
   kernel panic. (Note: `run_tiers`' per-tier threads use a SEPARATE pool
   `nros_tier_stacks[NROS_ZEPHYR_MAX_TIERS]`, so this is a Cyclone-RMW budget,
   not a tiers one.)
2. **No routable IP.** With eth up but no address, `getifaddrs` (the
   `link_stubs.c` net_if walk) finds none → Cyclone binds loopback → the native
   stack rejects it. A DHCP attempt got a wrong SLIRP address; a static IP in
   the SLIRP subnet binds cleanly.

## Work items

### W1 — make ws-entry publish (the two blockers)
- [ ] W1.1 `NROS_ZEPHYR_MAX_THREADS`: make it Kconfig-driven
  (`CONFIG_NROS_ZEPHYR_MAX_THREADS`, default 8 for back-compat; the shim's
  `#ifndef` fallback stays 8). Set it to **16** in the cyclone snippet conf
  `zephyr/snippets/nros-cyclonedds/cyclonedds.conf` — every cyclone-on-Zephyr
  consumer needs the headroom, the same place wall #9's mutex-pool bump lives.
  (16 covers Cyclone's ~7–8 workers + margin.)
- [ ] W1.2 ws-entry routable IP: static `172.20.51.15/24` (the FVP SLIRP
  subnet, gateway `.2`) + `CONFIG_NET_CONFIG_{SETTINGS,AUTO_INIT,NEED_IPV4}` in
  `examples/workspaces/ws-realtime-cpp-fvp/src/fvp_entry/prj.conf`. NOT DHCP
  (SLIRP handed a non-bindable address). A comment records the SLIRP-subnet
  dependency.
- [ ] W1.3 Validate on the model: ws-entry reaches `/ctrl` + `/telem`
  `Publishing:` under `cache_state_modelled=0` + SLIRP, no crash. This is the
  gating proof for the rest of the phase.

### W2 — provision `west fvp`
- [ ] W2.1 Register nano-ros's `scripts/west-commands.yml` (which declares the
  `fvp` command → `scripts/west_commands/fvp.py`) in the workspace so
  `west fvp run` resolves when nano-ros is consumed as a MODULE (the
  "unknown command fvp" gap — it self-registers only when nano-ros is the
  manifest repo). Wire via the west manifest `self.west-commands` /
  `scripts/zephyr/setup.sh`, whichever the workspace setup owns. Follows the
  Zephyr west-extension convention (no manual `west config`).
- [ ] W2.2 Confirm `west fvp run -d build-fvp-ws-entry` delegates to
  `west build -t run` with `ARMFVP_EXTRA_FLAGS="-C cache_state_modelled=0"`
  honored (last-wins over the board default `=1`; the aarch64
  `armfvp.cmake` appends env flags after the board flags).

### W3 — the gate (recipe + test + verb)
- [ ] W3.1 `just zephyr run-fvp-ws-entry`: `west fvp run -d build-fvp-ws-entry`
  with the env wiring (workspace `cd`, pinned make/ninja, `ZEPHYR_SDK_*`,
  `ARMFVP_EXTRA_FLAGS=-C cache_state_modelled=0`). Skips cleanly when
  west/workspace/SDK/ELF absent — same shape as `run-fvp-aemv8r-cyclonedds`.
- [ ] W3.2 `fvp_runtime_ws.rs` (`nros_tests`): drives `just zephyr
  run-fvp-ws-entry`, waits for BOTH `/ctrl` and `/telem` `Publishing:` (the
  `run_tiers` two-tier proof) within a ~180 s budget, `skip!`s on the four
  preconditions (FVP resolvable via `resolve-fvp-bin.sh`, `west` on PATH,
  workspace set up, `build-fvp-ws-entry/zephyr/zephyr.elf` prebuilt).
  `ManagedProcess` kills the FVP group on timeout/panic.
- [ ] W3.3 `just zephyr verify-fvp-runtime` maintainer verb:
  `build-fvp-ws-entry` then run the `fvp_runtime_ws` test (or the recipe +
  assertion). One command the maintainer runs before an ASI pin bump / release.

### W4 — retire the false-green legacy tests
- [ ] W4.1 `fvp_runtime.rs` / `fvp_runtime_rust.rs` (the talker publish/boot
  tests): the talkers are build-only single-node images with no ethernet device
  and can never publish, so these were pure false green. Delete them, or
  downgrade to boot-banner-only build proofs clearly labelled as such. Keep the
  `build-fvp-aemv8r-cyclonedds[-rust]` BUILD lanes (they still prove the codegen
  + link path).

## Acceptance
- `just zephyr verify-fvp-runtime` on a host with the FVP installed builds
  ws-entry and asserts it publishes `/ctrl` + `/telem` within the timeout.
- The same verb / the `fvp_runtime_ws` test `skip!`s cleanly (not fails) on a
  host without the model.
- No remaining runtime test can pass without actually exercising Cyclone
  participant-create + publish on the model (no false green).

## Out of scope
- Automated CI execution (license-walled model; maintainer-run by design).
- Two-sided guest→host wire verification (covered by the ASI demo closed-loop).
- The AVH/Corellium cloud-device path (possible future automation, separate).
