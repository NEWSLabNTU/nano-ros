# Phase 394 — CAN unicast for zenoh-pico: nano-ros talks ROS 2 services over CAN

**Status (2026-08-27). PROPOSED — nothing started.**

Implements [RFC-0083](../design/0083-can-unicast-over-isotp.md), the zenoh-pico
half. Ends with a nano-ros node and a ROS 2 node exchanging a **service call**
across a CAN bus.

**Depends on** phase-393, which must reach its W5 gate first: if a service does
not complete between two Linux peers, nothing here can work either.

---

## 1. Shape

ISO-TP comes from the platform, never from zenoh-pico. That is the whole design.

```
zenoh-pico:  third-party/isotp-c/                     vendored, MIT, pinned
             include/zenoh-pico/system/link/isotp.h   the platform contract
             src/link/config/isotp.c                  endpoint parsing
             src/link/unicast/isotp.c                 capabilities, lifecycle
             src/system/unix/network.c                CAN_ISOTP socket
             src/system/zephyr/network.c              native isotp_bind/send/recv
             src/system/<other>/network.c             the three vendored hooks
             CMakeLists.txt                           Z_FEATURE_LINK_ISOTP, default 0
```

## 2. Waves

Ordered so the vendored library is proven on a platform with a reference
implementation to disagree with, before it is trusted on one without.

| | What | Proves | State |
| --- | --- | --- | --- |
| **W0** | Vendor `SimonCahill/isotp-c` at a pinned commit, with its MIT licence; wire into the build | the dependency is present, attributed and reproducible | |
| **W1** | `unix` platform: implement the link over the **kernel** `CAN_ISOTP` socket | the pico link works where the kernel is the reference implementation | |
| **W2** | `src/link/unicast/isotp.c` + config; register in `_z_open_link` **only** | pico connects out and never touches the accept path | |
| **W3** | zenoh-pico ↔ zenoh-rs over `vcan0`: session, pub/sub, and a **query** | the two implementations agree on the wire | |
| **W4** | `unix` platform on the **vendored library** instead of the kernel socket | the vendored ISO-TP is conformant against the kernel as reference | |
| **W5** | Zephyr platform, using Zephyr's native `isotp_bind`/`isotp_send`/`isotp_recv` | the island's real platform | |
| **W6** | **nano-ros node ↔ ROS 2 node: a service call over CAN** | the reason this phase exists | |
| **W7** | Extend the demo container to show it | the artifact reviewers can run | |

**W4 is the interesting one.** Implementing `unix` twice — once on the kernel,
once on the vendored library — makes the kernel the oracle for the library on
the one platform where both exist. Any divergence is a library bug found on a
laptop instead of on a board.

## 3. Acceptance criteria

**W0.**
* The vendored tree records upstream URL, commit and licence, and the MIT
  `LICENSE` file is present verbatim.
* No GPL or AGPL code enters the tree. `devcoons/iso15765-canbus` (AGPL-3.0) and
  `altelch/iso-tp` (GPL-3.0) are the near misses; the survey in RFC-0083 §3
  records why each was rejected so nobody re-litigates it.
* The library builds for at least one MCU target with no allocator.

**W1.**
* A pico peer opens an ISO-TP link on `vcan0` and exchanges PDUs with a
  zenoh-rs peer.
* `_z_link_get_socket` returns the real descriptor — unlike the multicast CAN
  link, which cannot and which is what broke RFC-0080's unicast attempt.

**W2.**
* The link is registered in `_z_open_link` and **not** in `_z_listen_link`.
  A test or a comment states why: pico dials out, and `_zp_unicast_accept_task`
  is a listen-side path it must never enter.
* An endpoint with `tx_id == rx_id` is refused.
* With `Z_FEATURE_LINK_ISOTP=0` the image is byte-identical to today's.

**W3.**
* A pico peer and a zenoh-rs peer establish a unicast session over `vcan0`.
* Pub/sub works both directions with a payload past the MTU.
* **A query completes** — a pico `z_get` against a zenoh-rs queryable, or the
  reverse. This is the first evidence the ceiling is gone.

**W4.**
* The same W3 tests pass with the vendored library substituted for the kernel
  socket on `unix`.
* Any behavioural difference from the kernel is characterised and written down
  rather than worked around silently.
* `N_As` is addressed explicitly: if the platform's send is asynchronous, either
  the hook blocks until transmit-confirm or a timer is added. RFC-0083 §3 notes
  no portable library supplies this.

**W5.**
* The Zephyr port uses Zephyr's own ISO-TP, not the vendored library.
* Zephyr's implementation is `[EXPERIMENTAL]` in its Kconfig; its conformance
  and implementation test suites are run, and the result recorded, before the
  island depends on it for services.

**W6.** The gate.
* A nano-ros node and a ROS 2 node exchange a **service call** over CAN, in both
  roles: nano-ros as client, and nano-ros as server.
* No router and no TCP endpoint anywhere in the path.
* `candump` capture recorded.

**W7.**
* `docker/can-demo/` gains a unicast mode showing the service call.
* The README stops saying services do not work over CAN, and says instead which
  link carries which semantics.

## 4. Test method

**Tier 1 — `vcan0`, one laptop.** Everything through W4. The kernel and the
vendored library on the same interface, compared directly.

**Tier 2 — `native_sim`.** Zephyr's ISO-TP against a Linux peer, as phase-377
did for the multicast link.

**Tier 3 — hardware.** MR-CANHUBK344 to a Linux host. Still untouched, and now
carrying more weight: ISO-TP flow control has timing behaviour that `vcan`, with
no bit rate at all, cannot exercise even in principle.

## 5. Risks

**`vcan` cannot test flow control honestly.** `STmin`, `BS` and the `N_Bs`/`N_Cr`
timers exist to pace a real bus. On a zero-latency virtual interface they are
nearly no-ops, so a conformance bug can survive every Tier 1 test. This is the
strongest argument yet for hardware, and it should be said plainly rather than
discovered later.

**Zephyr's ISO-TP is marked experimental.** W5 runs its own test suites rather
than assuming; if they are thin, that is a finding the island needs before it
depends on services.

**Two implementations of `unix` is deliberate duplication.** It is a testing
oracle, not an accident, and the phase should keep both rather than delete the
kernel path once the library works — the day the library regresses, the oracle
is what finds it.
