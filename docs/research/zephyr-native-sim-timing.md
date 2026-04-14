# Zephyr native_sim Multi-Instance Issues

## Problem

When running `just zephyr talker` and `just zephyr listener` simultaneously,
`z_declare_publisher` fails with `-128` (Z_EGENERIC).

## Root Cause: Duplicate Zenoh Session IDs

**Confirmed via Wireshark packet capture** (zenoh dissector on the zeth-br bridge).

Zephyr native_sim uses a deterministic test entropy source
(`CONFIG_TEST_RANDOM_GENERATOR`). Without a unique `--seed` per instance,
both the listener and talker generate the **same Zenoh session ID (ZID)**.

The zenohd router sees the second connection's `OpenSyn` as a second link
for an already-open session. With the default `max_links=1`, it responds
with `Close(reason=4)` (`_Z_CLOSE_MAX_LINKS`) and immediately sends TCP FIN.

**Packet trace evidence:**
```
# Listener session (192.0.2.3) — succeeds:
  4  0.051  192.0.2.3 → 192.0.2.2  Zenoh  InitSyn   Zid: 5387bee10000000015d2aa08
  6  0.052  192.0.2.2 → 192.0.2.3  Zenoh  InitAck
  8  0.102  192.0.2.3 → 192.0.2.2  Zenoh  OpenSyn
  9  0.103  192.0.2.2 → 192.0.2.3  Zenoh  OpenAck   ← success

# Talker session (192.0.2.1) — rejected:
 23  4.050  192.0.2.1 → 192.0.2.2  Zenoh  InitSyn   Zid: 5387bee10000000015d2aa08  ← SAME ZID!
 25  4.051  192.0.2.2 → 192.0.2.1  Zenoh  InitAck
 27  4.101  192.0.2.1 → 192.0.2.2  Zenoh  OpenSyn
 28  4.101  192.0.2.2 → 192.0.2.1  Zenoh  Close     Reason: 4 (MAX_LINKS)  ← REJECTED
 29  4.101  192.0.2.2 → 192.0.2.1  TCP    [FIN, ACK]
```

The TCP connection then closes. zenoh-pico's session reports "opened
successfully" (it checks the `Open` exchange, not the subsequent `Close`
from the router's background processing), but any subsequent send fails
with `-100` (`_Z_ERR_TRANSPORT_TX_FAILED`), which propagates to
`z_declare_publisher` as `-128` (`_Z_ERR_GENERIC`).

## Fix: Unique `--seed` per Instance

The `--seed` flag controls native_sim's entropy source. Each instance must
have a different seed to produce a unique ZID.

**Automated tests** (`just zephyr test`): The test runner in
`packages/testing/nros-tests/src/zephyr.rs` generates unique seeds using
`SystemTime::now().subsec_nanos()` + an atomic counter.

**Manual recipes** (`just zephyr talker/listener/...`): Fixed seeds assigned
per recipe (talker=1000, listener=2000, service-server=3000, etc.).

**Three-terminal workflow:**
```
Terminal 1: just zephyr zenohd
Terminal 2: just zephyr listener
Terminal 3: just zephyr talker
```

## Debugging Methodology

This issue was diagnosed using:

1. **tshark + zenoh-dissector** — captured TCP traffic on `zeth-br` bridge,
   decoded zenoh frames, identified the `Close(MAX_LINKS)` from the router
2. **zenohd `RUST_LOG=zenoh_transport=debug`** — confirmed only one session
   was accepted
3. **printf in zpico.c** — confirmed background TX failure (`ret=-100`)
   before the publisher declaration

## Additional Notes

### `CONFIG_NATIVE_SIM_SLOWDOWN_TO_REAL_TIME`

This is `default y` for non-test builds (`!TEST`) in Zephyr's Kconfig.
It causes `k_sleep()` to map to real `nanosleep()`. The automated tests
use `CONFIG_TEST=y` which disables SLOWDOWN. This is unrelated to the
ZID collision but was investigated as a potential cause.

## Second Issue: `zeth0` TAP Contention Across Simultaneous Instances

**Observed 2026-04-14 while investigating 4 consistently-failing Rust E2E
tests** (`test_native_server_zephyr_client`, `test_native_to_zephyr_e2e`,
`test_zephyr_action_e2e`, `test_zephyr_talker_to_listener_e2e`).

### Symptom

Running two `native_sim` binaries at the same time (`just zephyr test`
flow: listener then talker) produces this boot sequence on *both*
processes:

```
[00:00:00.000] <err> eth_posix: Cannot create zeth0 (0)
[00:00:00.000] <inf> net_config: IPv4 address: 192.0.2.1   ← same IP on BOTH
[00:00:00.000] <inf> zpico_zephyr: Network interface up (waited 0 ms)
[00:00:00.204] <err> rustapp: Error: Transport(ConnectionFailed)
```

Key evidence:
- The `eth_posix` driver fails to create the TAP device because another
  instance already holds `zeth0`.
- Both processes then fall back to reporting the *same* static IPv4
  (192.0.2.1 — whichever one the prj.conf encodes — **even the listener,
  which is configured for 192.0.2.3**, reports 192.0.2.1 because its
  driver never actually bound to the backing bridge).
- `net_if_is_up()` still returns true (the virtual interface exists in
  Zephyr's netstack; only the host-side TAP is dead), so
  `zpico_zephyr_wait_network()` is useless against this failure.
- The TCP `connect()` to `192.0.2.2:7456` then fails because neither
  process has working L2 connectivity.

### Why the C++ tests appear to pass

The C++ E2E tests hit exactly the same boot errors (verified via
`--no-capture` on `test_zephyr_cpp_talker_to_listener_e2e` — both
processes show `Init failed: -100`), but the assertions in
`packages/testing/nros-tests/tests/zephyr.rs` are written so that a
process that boots but fails to publish prints a `WARNING:` and
returns `ok`. The Rust tests (`test_zephyr_talker_to_listener_e2e` et
al.) have stricter assertions that `panic!("Talker failed to create
publisher")`, so the same underlying failure surfaces as a hard fail.

In other words: **the 4 Rust failures and the corresponding C++
"passes" are the same bug. The Rust tests are correct; the C++ tests
are hiding it.**

### What this is *not*

- Not a `nros-platform-zephyr` issue — the ported platform crate is
  exercised by all 23 passing tests (talker→native, server→native,
  bidirectional, smokes, XRCE, C/C++ smokes).
- Not a ZID collision (this issue fires during boot; ZID collision
  fires at ~4 s during session open).
- Not a network-readiness timing issue — `zpico_zephyr_wait_network`
  sees `net_if_is_up == true` at t=0 ms.

### Paths forward

1. **Unique TAP device names per instance.** Zephyr's `eth_posix`
   driver reads `CONFIG_ETH_NATIVE_POSIX_DRV_NAME` at build time. We
   could generate a unique name per example (`zeth-talker`,
   `zeth-listener`, …) and attach all of them to the `zeth-br` bridge
   in `scripts/zephyr/setup-network.sh`. Cleanest fix; works for any
   number of simultaneous instances.
2. **Serialize the Zephyr nextest group.** Add
   `[[nextest.test-groups]]` with `max-threads = 1` for the zephyr
   integration test binary, so two native_sim processes never run at
   once. Quick to ship but regresses wall-clock test time and doesn't
   help anyone running two examples manually.
3. **Tighten the C++ assertions to match the Rust ones** — not a fix,
   but makes the state of the suite honest so the TAP contention bug
   stops being invisible.

Recommendation: **option 1 + option 3** together. Option 1 actually
fixes the bug; option 3 prevents future drift where one language's
tests hide failures from the other's.

### What *did* ship while chasing this

`nros::platform::zephyr::wait_for_network(timeout_ms)` was added to the
public surface of the `nros` crate so Rust Zephyr examples can mirror
the C++ `zpico_zephyr_wait_network()` convention. It is **not** the fix
for this particular bug (the TAP is already "up" from `net_if_is_up`'s
perspective), but it is the right shape of helper to keep for any
future case where the interface *does* come up asynchronously, and it
matches the pattern every C/C++ Zephyr example already follows. All 7
Rust examples were updated to call it. Harmless no-op today; cheap
insurance against drivers where the wait actually matters.
