# Phase 377 — A CAN link for zenoh-pico

**Status (2026-08-23). W0-W5 DONE. The link works, and the workload fits.**

Two zenoh-pico peers exchange pub/sub over `vcan0` with a 189-byte payload
across a 63-byte MTU, so zenoh's own fragmentation is driving the link. 24/24
zenoh-pico tests pass; the ASI Zephyr image is unchanged with the link off.
W5 says a 100BASE-T1-sized workload fits CAN FD with room to spare — see
"W5 results" below. Remaining: W6 (hardware).

**Superseded status.** W0-W2 and W4 DONE; W3 BLOCKED on a transport-capability
choice. The link is implemented, compiles for both platforms, and passes its
link-level test on `vcan0` — CAN FD negotiated, MTU 63, every DLC boundary, both
directions, over-MTU refusal. A zenoh *session* does not come up: the RFC's
`Z_LINK_CAP_TRANSPORT_UNICAST` obliges the listen side through
`_zp_unicast_accept_task`, which needs a socket and an `accept()` that a
datagram medium does not have. See [RFC-0080 §"UNICAST cannot complete a
two-peer handshake"](../design/0080-can-link-for-zenoh-pico.md). The fix is
`MULTICAST`, which is what a CAN bus actually is.

**Original status.** PROPOSED — nothing started. Opened from the Autoware
safety island bring-up: the MR-CANHUBK344's Ethernet is one 100BASE-T1 port on a
GMAC with no RGMII, so it is capped at 100 Mbit and needs a media converter to
reach any normal host — while the same board carries six CAN FD ports unused.

**Implements:** [RFC-0080](../design/0080-can-link-for-zenoh-pico.md).
**Related:** `_Z_LINK_TYPE_IVC` (phase-100.4, the datagram-link template),
`_Z_LINK_TYPE_CUSTOM` (phase-115.B, rejected for this — see RFC §3).

---

## 1. Shape

The link is **self-contained in the vendored zenoh-pico tree**. nano-ros gains
exactly one Kconfig entry and no transport code. See RFC-0080 §1 and §5.

```
zenoh-pico:  include/zenoh-pico/system/link/can.h    ← platform contract
             src/link/config/can.c                    ← endpoint parsing
             src/link/unicast/can.c                   ← capabilities, lifecycle
             src/system/unix/network.c                ← SocketCAN
             src/system/zephyr/network.c              ← zephyr/drivers/can.h
             CMakeLists.txt                           ← Z_FEATURE_LINK_CAN

nano-ros:    zephyr/Kconfig                           ← NROS_ZENOH_LINK_CAN
```

## 2. Waves

Ordered so that each wave is provable by the one before it, and so that the
**test loop exists before the target port**.

| | What | Proves | State |
| --- | --- | --- | --- |
| **W0** | Settle the wire format: DLC/length scheme, endpoint grammar, identifier plan (RFC §4). Header comment first, code after. | the wire is decided, not discovered | **done** |
| **W1** | `can.h` contract + `link/config/can.c` + `link/unicast/can.c`. | the generic half compiles and declares itself correctly | **done** |
| **W2** | `src/system/unix/network.c` SocketCAN binding. | a Linux zenoh-pico peer opens a CAN link | **done** |
| **W2.5** | `tests/z_can_link_test.c` against `vcan0`. | the frame codec is right — DLC steps, MTU, both directions | **done, passes** |
| **W3.a** | Change `_cap._transport` to `MULTICAST`: `read` returns the sender identifier, the rx filter widens to a peer mask, the endpoint grammar becomes own-id + mask. | a datagram link can host a session at all | **done** |
| **W3** | **Tier-1 harness**: two zenoh-pico peers, pub/sub across `vcan0`, `candump` capture. | the wire format works, fragmentation works, end to end | **done, passes** |
| **W4** | `src/system/zephyr/network.c` binding + `NROS_ZENOH_LINK_CAN` Kconfig + the full Kconfig→cargo→define chain. | the Zephyr port compiles; island unchanged with it off | **done** |
| **W5** | Bandwidth + latency measurement on the tier-1 harness at real island message rates. | the estimate in RFC §8 is replaced by a number | **done** |
| **W6** | Hardware: MR-CANHUBK344 CAN ↔ USB-CAN dongle ↔ Linux peer. | real bit rates, real errors, real timing | |

W0 is a decision, not code. W3 is the gate that matters — after it, the wire
format is validated and everything else is porting.

### Why `unix` before `zephyr`

The SocketCAN binding is the same code that talks to a virtual `vcan0`. Writing
it first unlocks the whole test loop with no board, no probe and no target
toolchain in the path. Doing Zephyr first would mean debugging a new wire format
and a new platform binding simultaneously, on hardware.

## 3. Test method

**Tier 0 — loopback.** `can_loopback.c` inside one Zephyr image. Link framing,
MTU handling, fragmentation, as a unit test.

**Tier 1 — vcan, one laptop.** The workhorse.

```sh
sudo modprobe vcan
sudo ip link add dev vcan0 type vcan
sudo ip link set up vcan0
sudo ip link property add dev vcan0 altname zcan0   # native_sim's DT expects zcan0
```

Two peers on `vcan0`; `candump vcan0` observes every frame. `native_sim` reaches
it through Zephyr's `CAN_NATIVE_LINUX` driver (`ARCH_POSIX`), and
`native_sim.dts` already carries the node and documents the `altname` step. Both
that driver and the loopback driver support CAN FD via `CONFIG_CAN_FD_MODE`.

**Tier 2 — dongle.** CANable / PCAN-USB / Kvaser to a real board.

**Tier 3 — the system.** Board ↔ Orin.

**Not QEMU.** Zephyr has no CTU CAN FD or QEMU CAN driver; `drivers/can/`
contains nothing matching. QEMU emulates controllers Zephyr cannot drive.
`native_sim` + `vcan` is simpler and strictly more capable here. Recorded
because QEMU is the obvious first guess.

## 3b. W5 results (2026-08-23)

**Method, and its limit.** `vcan` has no bit-rate simulation — frames cross
memory — so a throughput number measured on it would describe the software, not
a bus. What it measures honestly is **frames per message**; wire time is then
arithmetic. Frame counts came from `candump`, differencing a 14-message run
against a 4-message one so the handshake cancels.

Measured, publishing through a zenoh session over the link:

| payload | frames/msg | payload bytes per frame |
| ---: | ---: | ---: |
| 20 B | 1.4 | 14.3 |
| 100 B | 2.4 | 41.7 |
| 200 B | 4.4 | 45.5 |
| 240 B | 5.4 | 44.4 |

Above ~100 B the cost is linear: **one frame per 47.3 payload bytes** against a
63-byte MTU, so zenoh's per-fragment overhead is about 16 bytes and the link
runs at **75% of MTU**. That is the number worth carrying forward; the earlier
estimate assumed the full 63.

**Wire time**, CAN FD with an 11-bit identifier, 500 kbit/s arbitration and
2 Mbit/s data, BRS, 10% allowed for stuff bits: a full frame is **336 µs**,
giving 2,793 frames/s and **1.41 Mbit/s of usable payload**.

| message | bytes | frames | wire time |
| --- | ---: | ---: | ---: |
| MRM stop / operate request | 32 | 0.9 | **0.32 ms** |
| GearCommand, ControlModeReport | 40 | 1.1 | 0.37 ms |
| MrmState / behaviour status | 48 | 1.3 | 0.43 ms |
| VelocityReport | 56 | 1.4 | 0.49 ms |
| `nav_msgs/Odometry` | 718 | 15.5 | 5.20 ms |

**Bus load** for the island's 14 publishers:

| scenario | load |
| --- | ---: |
| Odometry alone at 50 Hz | 27% |
| Odometry 50 Hz + 13 others at 20 Hz | **37%** |
| everything at 10 Hz | 10% |

So the workload fits with roughly 2.7x headroom at the busiest realistic rate.
RFC §8's "0.5–1 Mbit/s against 1–1.5 available, without much margin" was
pessimistic: the margin is real, but it comes with Odometry costing 15 frames.

**The latency property that matters.** CAN arbitrates **per frame**, so a
high-priority identifier preempts a long burst at the next frame boundary rather
than waiting for it to finish. A stop command queued behind a 15-frame Odometry
burst waits **0.34 ms**, not 5.2 ms. That is the bounded worst case a safety
island wants, and it is a property of the medium rather than of anything this
link does — provided the stop command is given a lower (higher-priority)
identifier than the bulk traffic.

**Caveats.** The airtime model is analytic, not a wire measurement: it assumes
11-bit identifiers, BRS, and 10% stuff bits, and it ignores arbitration lost to
other traffic on a shared bus. W6 on hardware is what turns it into a
measurement.

**Found while measuring:** zenoh-pico's own `examples/unix/c11/z_pub.c` has
`char buf[256]` and `sprintf`s the payload into it, so any `-v` argument beyond
~240 bytes aborts the publisher with a glibc buffer-overflow trap. Upstream
example bug, unrelated to the link; it is why the table above stops at 240 B
and Odometry is extrapolated from the fit.

## 4. Acceptance

* W3: two zenoh-pico peers exchange pub/sub over `vcan0`, including a payload
  larger than the link MTU (proving zenoh's fragmentation drives the link
  correctly), with a `candump` capture attached to the phase.
* W4: the four-node island runs as `native_sim` over `vcan0` against a Linux
  peer, with no Ethernet configured.
* W5: measured throughput and per-message latency at island message rates,
  recorded against RFC §8's 0.5–1 Mbit/s estimate.
* No nano-ros file outside `zephyr/Kconfig` changed.

## 5. Risks

**The wire format is the irreversible part.** Once a peer ships, the DLC/length
scheme and identifier grammar are a compatibility surface. W0 exists to stop
that being decided by whoever writes the first `memcpy`.

**Bandwidth has little margin.** ~1–1.5 Mbit/s usable against an estimated
0.5–1 Mbit/s need. If W5 says otherwise, the transport decision reopens — better
at W5 than after hardware is bought.

**The host side is unscoped.** zenoh-rs needs a matching link for a real Orin
deployment (RFC §8). Tier 1 defers it, and W6 does not require it, but a
production system does.

**Bus sharing is unresolved** (RFC §4.3). If the island shares a bus with
vehicle traffic, identifier priority and fragmentation burst length interact
with everything else on it. Needs a measurement on a loaded bus, not a guess.
