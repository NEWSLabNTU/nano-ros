# Phase 81: Fix Zephyr native_sim Multi-Instance E2E Tests

**Goal**: Make `just zephyr test` pass cleanly on all 27 tests by fixing the root cause that is currently masked in C++ suites and hard-failing in Rust suites.

**Status**: Not Started
**Priority**: Medium
**Depends on**: Phase 79 (nros-platform-zephyr landed in 79.16)

## Overview

### Problem

Four Zephyr E2E tests consistently fail on `just zephyr test`:

- `test_native_server_zephyr_client`
- `test_native_to_zephyr_e2e`
- `test_zephyr_action_e2e`
- `test_zephyr_talker_to_listener_e2e`

All four involve a Rust `native_sim` process that either (a) is the second Zephyr instance launched in the test, or (b) runs alongside another Zephyr instance. All four surface as:

```
[00:00:00.000] <err>   eth_posix: Cannot create zeth0 (0)
[00:00:00.000] <inf>   net_config: IPv4 address: 192.0.2.1    ← same IP on BOTH instances
[00:00:00.000] <inf>   zpico_zephyr: Network interface up (waited 0 ms)
[00:00:00.204] <err>   rustapp: Error: Transport(ConnectionFailed)
```

The test then panics with "Talker failed to create publisher" or similar.

### Root cause

Zephyr's `eth_posix` driver on `native_sim` unconditionally uses the TAP device name `zeth0` (from `CONFIG_ETH_NATIVE_POSIX_DRV_NAME`, default `"zeth0"`). When two `native_sim` processes launch simultaneously, only the first can open the TAP fd. The second logs `Cannot create zeth0 (0)` and falls back to a dead virtual interface — `net_if_is_up()` still returns true because the netif object exists, but no packets flow through the host-side bridge. Both processes then report the static IPv4 from their `prj.conf` (often the *same* address because the second process got garbage), and the TCP connect to the zenohd router at `192.0.2.2:7456` fails because neither process has L2 connectivity.

This is unrelated to:

- `nros-platform-zephyr` — the platform crate is exercised by all 23 passing tests.
- Zenoh session ID collisions (documented separately in `docs/research/zephyr-native-sim-timing.md`; those fire at ~4 s during session negotiation, not at boot).
- Network readiness timing — `wait_for_network()` sees `net_if_is_up == true` at t=0 ms.

### Why the C++ suites currently "pass"

`test_zephyr_cpp_talker_to_listener_e2e` hits **exactly the same boot errors** (verified 2026-04-14 via `cargo nextest --no-capture`): both C++ processes log `Cannot create zeth0 (0)`, both report `192.0.2.1` even though the listener is configured for `192.0.2.3`, and both log `Init failed: -100` during `nros::init()`. The test reports `ok` because its assertion chain degrades through:

```rust
if !talker_published {
    eprintln!("WARNING: Talker started but didn't publish");
    eprintln!("This may be due to zenoh-pico interest message limitations");
    return; // ← test passes silently
}
```

The Rust assertions in `packages/testing/nros-tests/tests/zephyr.rs:161` `panic!("Talker failed to create publisher")` instead, which is the correct behavior. So the visible red-vs-green difference between Rust and C++ suites is **pure assertion strictness**, not a real language-specific bug.

## Work Items

- [ ] 81.1 — Reproduce and confirm the TAP contention hypothesis
  - [ ] 81.1.1 — Capture on `zeth-br` during a failing test run with `tshark` to verify neither failing instance emits any frames to `192.0.2.2`
  - [ ] 81.1.2 — Confirm via `ls /sys/class/net/` inside the failing `native_sim` process (or `ip link show zeth0`) that only one `zeth0` is attached to the bridge at the moment of failure
  - [ ] 81.1.3 — Document exact reproduction steps in `docs/research/zephyr-native-sim-timing.md`

- [ ] 81.2 — Per-example unique TAP device names
  - [ ] 81.2.1 — Add `CONFIG_ETH_NATIVE_POSIX_DRV_NAME` override to every Zephyr example `prj.conf` (rust/zenoh/*, cpp/zenoh/*, rust/xrce/*, c/xrce/*). Use the example name as the suffix: `"zeth-talker"`, `"zeth-listener"`, `"zeth-service-client"`, etc. 22 files total.
  - [ ] 81.2.2 — Update `scripts/zephyr/setup-network.sh` to attach the full set of per-example TAP names to the `zeth-br` bridge. Keep the existing `zeth0` entry for manual `just zephyr talker` runs that don't set the Kconfig override.
  - [ ] 81.2.3 — Handle the bridge side robustly: the setup script runs before any Zephyr process, so the TAP devices don't exist yet. Either pre-create persistent TAPs (`ip tuntap add dev zeth-talker mode tap`) or have the setup be idempotent and run at test time.
  - [ ] 81.2.4 — Verify `just zephyr test` shows 27/27 passing.
  - [ ] 81.2.5 — Verify manual workflow still works: `just zephyr zenohd` + `just zephyr talker` + `just zephyr listener` in three terminals.

- [ ] 81.3 — Tighten C++ test assertions to match Rust strictness
  - [ ] 81.3.1 — Remove the `WARNING: Talker started but didn't publish ... return` soft-pass path in `test_zephyr_cpp_talker_to_listener_e2e`
  - [ ] 81.3.2 — Audit `test_zephyr_cpp_*_e2e` for similar degraded-success paths and remove them
  - [ ] 81.3.3 — Before shipping 81.2, confirm these newly-strict C++ tests also fail on the current (unfixed) tree, proving they were masking the same bug
  - [ ] 81.3.4 — After shipping 81.2, verify they pass

- [ ] 81.4 — Guard against regression via nextest grouping
  - [ ] 81.4.1 — Even with unique TAP names, the `zeth-br` bridge has a finite port budget. Add a `[[nextest.test-groups]]` entry capping simultaneous Zephyr native_sim instances to a sane number (e.g. 4) so a future test explosion doesn't starve the bridge.

## Acceptance Criteria

- [ ] `just zephyr test` reports 27 passing, 0 failing, 0 skipped
- [ ] Manual three-terminal workflow (`zenohd` + `talker` + `listener`) still works
- [ ] C++ tests have no `WARNING:` soft-pass paths remaining — any suite failure prints a hard panic
- [ ] `docs/research/zephyr-native-sim-timing.md` updated with the fix and a note that the "Second Issue" section is resolved

## Notes

### Why not serialize tests via `max-threads = 1`?

Tempting as a one-line fix, but it has two drawbacks:

1. It regresses wall-clock test time (currently ~300 s for 27 tests; serializing would roughly double it because several tests deliberately launch two instances in parallel).
2. It doesn't help anyone running two examples manually with `just zephyr talker` + `just zephyr listener` — the `zeth0` contention would still bite in that workflow.

Per-instance TAP names fix it at the actual source.

### Why the research doc is the source of truth for the diagnosis

`docs/research/zephyr-native-sim-timing.md` (§ "Second Issue: zeth0 TAP Contention Across Simultaneous Instances") was extended during the Phase 79.16 Zephyr port investigation with the full evidence chain: `--no-capture` output from the "passing" C++ test showing the same `Cannot create zeth0 (0)` boot error, the asymmetric IPv4 assignments, and the `zpico_zephyr_wait_network()` returning 0 ms on a dead interface. Re-confirm before starting 81.2 in case upstream Zephyr has changed behavior.

### What already shipped (Phase 79.16)

`nros::platform::zephyr::wait_for_network(timeout_ms)` was added to the `nros` public surface and wired into all 7 Rust Zephyr examples. This is **not** the fix for Phase 81 (the wait sees `net_if_is_up == true` at t=0, so it's a no-op against the TAP contention), but it is the right helper shape for any future driver where the wait genuinely matters and it keeps the Rust examples symmetric with the C/C++ ones that already call `zpico_zephyr_wait_network()`. Leave it in place.
