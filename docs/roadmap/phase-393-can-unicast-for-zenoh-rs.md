# Phase 393 — CAN unicast for zenoh-rs: ROS services over a CAN bus

**Status (2026-08-27). W0-W5 DONE — the gate is passed.**

**A ROS 2 service call completes over a CAN bus.**

```
[add_two_ints_server]: Incoming request
[add_two_ints_client]: Result of add_two_ints: 5
```

No router, no TCP: each session has exactly one active endpoint and it is
`isotp/vcan0`. 1141 CAN frames across the pair, including 13 flow-control
frames and visible first frames (`10 19` = FF of 25 bytes), so the kernel's
ISO-TP segmentation is genuinely carrying it. W6 and W7 remain.

Implements [RFC-0083](../design/0083-can-unicast-over-isotp.md), the zenoh-rs
half. Delivers what the multicast link cannot: **ROS 2 services, actions,
parameters and graph introspection across a CAN bus.**

**Depends on** phase-378 (the multicast link and the measurement that exposed the
ceiling). **Blocks** phase-394, which does the same for zenoh-pico.

---

## 1. Shape

A second link crate, unicast, beside the multicast one. Both stay.

```
zenoh:  io/zenoh-links/zenoh-link-isotp/src/lib.rs       prefix, config, inspector
        io/zenoh-links/zenoh-link-isotp/src/sys.rs       CAN_ISOTP sockets, Linux
        io/zenoh-links/zenoh-link-isotp/src/unicast.rs   LinkUnicastIsotp + managers
        io/zenoh-link/src/lib.rs                         LinkKind::Isotp registration
        io/zenoh-link-commons/src/unicast.rs             LinkAuthId::Isotp
        zenoh/Cargo.toml                                 transport_isotp, default off
```

## 2. Waves

Ordered so the claim that justifies the whole phase — a ROS **service** working
over CAN — is reached as early as the machinery allows, and so the pieces that
can be tested without a bus are tested without one.

| | What | Proves | State |
| --- | --- | --- | --- |
| **W0** | Crate skeleton; endpoint grammar; identifier-pair validation; unit tests with no socket | the configuration surface is right, and testable anywhere | **done** |
| **W1** | `sys.rs`: `CAN_ISOTP` socket open/bind, options, `sockaddr_can.tp` | a Linux process moves ISO-TP PDUs | **done** |
| **W2** | `LinkUnicastTrait` + connect and listen managers, on the `zenoh-link-serial` pattern | a unicast link exists on a medium with no `accept()` | **done** |
| **W3** | `LinkKind::Isotp`, `LinkAuthId::Isotp`, `transport_isotp` feature plumbing | a zenoh session accepts an `isotp/...` endpoint | **done** |
| **W4** | E2E on `vcan0`: two zenoh-rs peers, a **unicast** transport, payload past the MTU | the link carries a zenoh session at all | **done** |
| **W5** | **`ros2 run demo_nodes_cpp add_two_ints_client` succeeds over CAN** | the reason this phase exists | **done** |
| **W6** | Graph introspection, actions, parameters; negative control on the multicast link | the full ROS surface, and that the contrast is real | |
| **W7** | `prio_classes`: one ISO-TP socket per priority, identifiers priority-major | per-message bus arbitration, which W7 of phase-378 could not reach | |

**W5 is the gate.** Everything before it is machinery; if a service call does not
complete, the premise of RFC-0083 is wrong and the rest should not be built.

### Results (2026-08-27)

| wave | outcome |
| --- | --- |
| W0 | 14 unit tests, no socket, no root |
| W1 | `CAN_ISOTP` socket; `can-isotp` autoloads unprivileged, so no modprobe prerequisite |
| W2 | link + listener, no `accept()` anywhere |
| W3 | `LinkKind::Isotp` registered; default and `--no-default-features` builds unchanged |
| W4 | **unicast** transport over `vcan0`, MTU 4095, asserted rather than inferred |
| W5 | **`Result of add_two_ints: 5`** over CAN |

**Three ABI traps**, found by transcribing the kernel header rather than working
from memory, all now handled in code:

* `sockaddr_can.can_addr.tp` declares **`rx_id` before `tx_id`**. Declaring the
  struct locally in the intuitive order swaps them silently and yields a link
  that opens cleanly and never communicates. `libc`'s struct is used and the
  fields are set by name.
* `optlen` is compared for **equality**, not as a minimum, so a layout drift
  would surface as a runtime `EINVAL` on correct configuration. `const` size
  assertions make it a compile error.
* Every socket option must be set **before** `bind`; `isotp_setsockopt` returns
  `EISCONN` afterwards.

**The port to the ROS revision cost more than the multicast one did**, and the
difference was predicted. `LinkMulticastTrait` is byte-identical between
`release/1.8.0` and `main`, so the multicast link moved untouched. The unicast
trait is not: at `2687c5135` the I/O methods carry no priority argument, and
there is no `supports_priorities`, no `get_fd` and no
`get_locators_noloopback`. Four mechanical differences in one file, noted in
the file itself so the divergence is legible rather than rediscovered by
diffing. Branch `feat/can-unicast-isotp-ros`; `feat/can-unicast-isotp` keeps the
`main` line for the eventual PR.

**Also confirmed empirically:** clippy's MSRV lint caught `is_none_or`, stable
since 1.82 against zenoh's 1.75 floor — the same gate upstream CI enforces.

## 3. Acceptance criteria

**W0.**
* `cargo test -p zenoh-link-isotp` passes with no CAN interface and no root.
* `tx_id` equal to `rx_id` is refused: ISO-TP needs a directed pair, and a peer
  addressing itself is a configuration error rather than a quiet loop.
* Identifiers above the 11-bit range are refused unless 29-bit addressing is
  explicitly requested.
* The grammar is documented in the crate, including that only **normal**
  addressing is supported and why (RFC-0083 §3).

**W1.**
* Opening on a missing interface names the interface.
* A kernel without `can-isotp` produces an error that says so and names the
  module, rather than a bare `EPROTONOSUPPORT`.
* The MTU the link reports matches what the socket will actually accept.

**W2.**
* `new_listener` binds the pair and yields a link on first contact, with no
  `accept()` call anywhere.
* Closing a link closes its socket; a second listener on the same pair is
  refused rather than silently stealing traffic.

**W3.**
* Default build and `--no-default-features` build unchanged.
* `cargo clippy --all-targets --features transport_isotp -- -D warnings` clean.
* The crate compiles on non-Linux, reporting the platform as unsupported at
  open — matching how `zenoh-link-can` handles it.

**W4.**
* Two peers on `vcan0` establish a **unicast** transport — asserted by checking
  the transport kind, not inferred from traffic.
* A payload of at least 8 KiB arrives intact, exercising zenoh fragmentation
  above ISO-TP segmentation.
* The test skips with an explanation when `vcan0` is absent; it never fails for
  that reason.

**W5.** The gate.
* `add_two_ints_server` and `add_two_ints_client` under
  `RMW_IMPLEMENTATION=rmw_zenoh_cpp`, with an `isotp/...` endpoint as the only
  transport, produce `Result of add_two_ints: 5`.
* No router, no TCP endpoint in either session config.
* `candump` shows the exchange, and the capture is recorded.

**W6.**
* `ros2 node list` and `ros2 topic list` show the remote node and its topics —
  the graph queries that return empty over the multicast link.
* An action and a parameter get/set both complete.
* **Negative control:** the same service call over the *multicast* CAN link still
  fails. Without it, W5 proves only that services work somewhere.

**W7.**
* `prio_classes=8` opens eight sockets and selects by batch priority.
* Identifier layout is priority-major, so class dominates arbitration.
* `prio_classes=1`, the default, uses exactly one pair and changes nothing.

## 4. Test method

**Tier 0 — unit.** Grammar and address validation, no socket.

**Tier 1 — `vcan0`.** Two zenoh-rs peers; then two ROS 2 nodes.

**Tier 2 — the contrast.** The same ROS operations over the multicast link,
which must still fail for services and graph. The phase's claim is comparative.

**Tier 3 — hardware.** Still untouched, here as everywhere. Every timing figure
in this phase will be `vcan`-derived and therefore says nothing about a real bus.

## 5. Risks

**The kernel module may be absent.** `can-isotp` is mainline since 5.10 but not
loaded by default on every distribution. W1's error message is the mitigation,
and the README must state it as a prerequisite the way the vcan one does.

**A lost consecutive frame aborts a whole PDU.** With a 4095-byte MTU that is a
large unit of loss, and `is_reliable = false` means nothing below zenoh will
recover it. Worth measuring before choosing a default MTU; the maximum is not
automatically the right one.

**Identifier pairs scale with peers.** This is a link, not a bus. A design that
quietly assumes bus economics will run out of identifiers, and unlike the
multicast link there is no band to share.
