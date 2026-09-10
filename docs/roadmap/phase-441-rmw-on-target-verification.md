# Phase 441 — the one "on-target" live-peer cell runs on host sockets

**Status (2026-09-10). W2 ANSWERED (yes — see below); W1 and W3–W5 not started.
Analysis below; work items W1–W5 proposed.**

Phase-433 closed with twenty of twenty Runtime interop cells carrying a live
verdict, and named its own remainder:

> ### W7 — one on-target cell that is not QoS
>
> An RTOS image against a stock peer, on a platform that is not Zephyr. This is
> the largest job here and the one with the weakest current argument, so it goes
> last and may reasonably become its own phase.

This is that phase. The argument is stronger than phase-433 could make it,
because writing it down required looking at what the existing on-target cell
actually exercises — and the answer is less than its name suggests.

## What we have, measured

`interop::CELLS` has exactly one non-Linux runnable row:

```rust
ic("zephyr-qos-rust-zenoh",
   c(ZephyrNativeSim, Rust, Zenoh, Qos, Interop, Runtime), …)
```

plus one carved sibling (`zephyr-qos-cpp-cyclone-CARVED`). Every other row is
`PlatformId::Linux`. So the live-peer coverage of the RTOS ports is: **one
platform, one language, one RMW, one workload.**

`FreertosMps2`, `FreertosPosix`, `NuttxArm`, `NuttxRiscv`, `ThreadxLinux`,
`ThreadxRiscv64`, `Esp32Qemu`, `QemuBaremetal`, `Fvp`, `Px4` and
`ZephyrQemuCortexM` have **no live-peer cell at all**. Whatever those ports do
against a stock ROS 2 node is unobserved.

## The part that changes the argument

`ZephyrNativeSim` is documented in `matrix.rs` as *"Zephyr native_sim (NSOS host
sockets)"*, and `ZephyrQemuCortexM`'s doc comment spells out the difference:

> phase-337 W2 added this because "Zephyr" previously meant exactly one config:
> `native_sim/native/64`, where sockets are OFFLOADED to the host and the
> pointer width is the host's. That is a board, not a platform — and the
> difference is not academic. Bringing this witness up cost five real defects (a
> 32-bit `size_t`/`uintptr_t` header conflict, an atomics feature gated on an
> arch list, a staticlib with no allocator or panic handler off native_sim, a
> duplicated cmake feature string, and a board with no entropy device), every
> one of them invisible to native_sim.

So the one cell we call on-target verification **never puts an RTOS network
stack in the path**. Its sockets are the host's, its pointers are the host's,
its libc is the host's. It proves that our zenoh-pico code talks to
`rmw_zenoh_cpp`; it does not prove that it does so from a device.

That is not an argument that the cell is worthless — it caught issue #141, where
`ros2 topic echo` looked dead against a healthy publisher. It is an argument
that the phrase "on-target" is currently doing work the artifact does not
support, and that the five-defect precedent above is the best available estimate
of what a real second witness would find.

## Why the crossing is affordable, and where it is not

The existing cell already uses the mechanism this phase needs. It bakes a router
locator into the image (`CONFIG_NROS_ZENOH_LOCATOR`, the allocator's
`port_of(ZephyrNativeSim, Rust, Qos)`) and connects by **TCP to a known
endpoint** — not by multicast discovery. `scripts/qemu/setup-network.sh` already
records the peer-side half of that shape:

```
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/0.0.0.0:7447"];scouting/multicast/enabled=false'
```

That matters because of how a QEMU guest reaches the host.
`scripts/qemu/launch-mps2-an385.sh` offers two modes:

| mode | reaches the host | needs root |
| --- | --- | --- |
| `--slirp` (user-mode NAT, gateway `10.0.2.2`) | unicast only | no |
| `--tap` | full, multicast included | **yes** — `ip tuntap add` |

CLAUDE.md forbids `sudo` outright, so **TAP cannot be the default path for this
phase**; it is a maintainer-provisioned option at most. Everything W1–W4 propose
must therefore work over slirp, which means unicast-to-a-known-endpoint — which
is exactly what zenoh-pico client mode and the baked locator already do.

**This is the axis on which the backends differ, and it decides the ordering.**
Cyclone's SPDP discovery is multicast by default; making it work over slirp
means a configured unicast peer list, which is a different thing to verify and
may not be worth doing first. zenoh-pico needs no such change. So zenoh goes
first, and whether Cyclone-on-QEMU is reachable at all over slirp is itself a
finding this phase should record rather than assume. **W2 has now measured it:
yes, with four settings and a `hostfwd` — see W2 below.**

## One constraint that is already written down

`ZephyrQemuCortexM`'s doc comment: *"Cells here are C/C++ only: the pinned
`zephyr-lang-rust` cannot compile for any board whose devicetree has gpio nodes
(issue 0432)."* So the natural first target — the same QoS workload, moved from
native_sim to the Cortex-M board — **cannot be the Rust image**. It is a C or
C++ cell, which also means it exercises a different half of our API surface than
the existing row. That is a feature, not a problem, but it must be planned for
rather than discovered.

## Work items

### W1 — the same workload, one board over

Move the proven cell to `ZephyrQemuCortexM`, in C or C++ (issue 0432 forbids
Rust there), over slirp, against a stock `rmw_zenoh_cpp` peer with multicast
scouting disabled.

This is deliberately the smallest possible step: same RMW, same workload, same
peer, same crossing mechanism, one axis moved — from host sockets to Zephyr's
own IP stack over `eth_smsc911x`, and from 64-bit host pointers to 32-bit.

**Acceptance.** A row in `interop::CELLS` with `Tier::Runtime`, an
`interop::assert_test_bound` tripwire, a recorded `pass` in
`.config/interop-verdicts.toml` written by `--record` from a real junit, and
membership in the `live-peer.yml` lane that follows from that ledger entry.

**Expected finding, stated in advance so it is falsifiable:** phase-337 W2 found
five defects moving a non-interop workload across this same boundary. If W1
finds none, that is itself worth writing down — it would mean the earlier five
were about the build, not the wire.

### W2 — is Cyclone reachable from a QEMU guest at all?

A measurement, not a cell. Determine whether `rmw_cyclonedds_cpp` and our
embedded Cyclone can discover each other across slirp with a configured unicast
peer, and what it costs. The pinned Cyclone is 0.10.5 (the version ROS ships —
never bump it off that, see issue 0507).

**Acceptance.** A written answer either way. If yes, a W1-shaped cell follows.
If no, a recorded reason in the same place the declined RMW symbols carry
theirs, so the absence stops reading as an oversight — the failure mode issues
1137, 1164 and 1231 each demonstrated.

#### ANSWER (2026-09-10): yes, and it costs four settings and a QEMU flag

Two CycloneDDS 0.10.5 participants — the pinned tree at `67ff7518`, built for
the host — discovered each other and delivered a sample across a user-mode NAT
of slirp's shape, in both directions, in **16 ms**. The four settings below are
each NECESSARY: removing any one of them was measured and produced no discovery
at all.

**Guest side** (the image; this is the shape `kEmbeddedCycloneConfig` /
`CONFIG_NROS_CYCLONE_CONFIG_XML` / the bringup's `rmw/cyclonedds.xml` would
carry):

```xml
<CycloneDDS><Domain Id="42">
  <General>
    <AllowMulticast>false</AllowMulticast>
    <ExternalNetworkAddress>127.0.0.1</ExternalNetworkAddress>
  </General>
  <Discovery>
    <ParticipantIndex>0</ParticipantIndex>
    <Peers><Peer Address="10.0.2.2:17912"/></Peers>
  </Discovery>
</Domain></CycloneDDS>
```

**Host side** (the `ros2` peer's `CYCLONEDDS_URI`):

```xml
<CycloneDDS><Domain Id="42">
  <General>
    <AllowMulticast>false</AllowMulticast>
    <ExternalNetworkAddress>10.0.2.2</ExternalNetworkAddress>
  </General>
  <Discovery>
    <ParticipantIndex>1</ParticipantIndex>
    <Peers><Peer Address="127.0.0.1:17910"/></Peers>
  </Discovery>
</Domain></CycloneDDS>
```

**QEMU side** — the guest's two unicast ports forwarded in:

```
-netdev user,id=n0,hostfwd=udp::17910-10.0.2.15:17910,hostfwd=udp::17911-10.0.2.15:17911
```

The ports are not free parameters; they are DDSI 2.1 §9.6.1 arithmetic, and the
pinned tree exposes every constant under `Discovery/Ports` (`Base` 7400,
`DomainGain` 250, `ParticipantGain` 2, `UnicastMetaOffset` 10,
`UnicastDataOffset` 11). Domain 42 with participant index 0 is therefore meta
`17910` / data `17911`, and index 1 is `17912` / `17913`. `10.0.2.15` is the
guest's own address and must match what the board's `Config` bakes
(`Config::qemu_slirp()` uses `10.0.2.10` on mps2-an385 — the `hostfwd` target
follows the board, not this example).

#### Why each part is load-bearing

| variant | change from the working config | result |
| --- | --- | --- |
| V1 | (the config above), guest publishes | **sample delivered**, 16 ms |
| V5 | roles swapped, host publishes | **sample delivered** |
| V1b | host advertises its real LAN address instead of `10.0.2.2` | **sample delivered** |
| V2 | no `hostfwd` | no discovery |
| V3 | guest does not rewrite its advertised address | no discovery |
| V4 | host pinned to loopback (the issue-1009 isolation shape) | no discovery |
| V6 | `AllowMulticast=false` + `Peers` and nothing else | no discovery |
| V7 | V1 plus a matching `<Discovery><Tag>` on both sides | **sample delivered** |
| V8 | V1 with MISMATCHED tags | no discovery |

Two of those are worth spelling out.

**V3 — the guest's advertised locator is the crux.** SPDP carries the sender's
own unicast locators, and a guest behind NAT advertises `10.0.2.15`, which the
host cannot dial. `<General><ExternalNetworkAddress>` is the documented NAT knob
(*"allows explicitly overruling the network address Cyclone DDS advertises in
the discovery protocol … to allow Cyclone DDS to communicate across a Network
Address Translation (NAT) device"*), and with it the host's trace shows the
guest arriving at the forwarded address:

```
SPDP ST0 110910c:… NEW (pasta-jerry-aeon/0.10.5/Linux/Linux)
   (data udp/127.0.0.1:17911@2 meta udp/127.0.0.1:17910@2)
```

while the guest addresses the host through the gateway alias:

```
setcover: all_addrs udp/10.0.2.2:17912@2
```

**V4 — the loopback pin and the NAT rewrite are mutually exclusive**, which is
the one finding that lands on this tree rather than on Cyclone, and is filed as
[issue 1251](../issues/1251-cyclone-slirp-needs-nat-profile.md).
`ExternalNetworkAddress` is refused outright when the only selected interface is
loopback (`q_init.c:398`, *"external network address specification only
supported if there is a unique non-loopback interface"*), so a host pinned the
way `dds_isolation` pins it cannot rewrite what it advertises — and `127.0.0.1`
means the guest's own loopback inside the guest. Worse, it fails SILENTLY:
both sides log the other's SPDP and neither matches, because Cyclone treats an
advertised locator equal to its own external locator as "same machine, use the
real interface address" (`q_ddsi_discovery.c:208-221`), so the guest ends up
addressing `10.0.2.15` — itself.

`<Discovery><Tag>` (V7/V8) is the isolation mechanism that survives the NAT: a
domain-id extension both peers must match, independent of interface selection.
It has to be a build-time constant on the guest, because the guest's config is
baked into the image.

#### The experiment, and what it does not model

`libslirp` needs a guest, and a guest needs an image, so the decisive shape
(RTOS image on QEMU, host `rmw_cyclonedds_cpp` peer) was not run. What ran is
the same constraint applied to real Cyclone participants without root:

- the "guest" is a process in its own network namespace whose only path out is
  **pasta** (passt), a user-mode NAT configured to slirp's numbering:
  `pasta --config-net -a 10.0.2.15 -n 24 -g 10.0.2.2 --map-host-loopback 10.0.2.2 -u 17910,17911`
  — outbound unicast works, `10.0.2.2` maps to the host's loopback, inbound
  arrives only on forwarded ports, multicast does not cross;
- both peers are `HelloworldPublisher`/`HelloworldSubscriber` from the pinned
  submodule, built from `third-party/dds/cyclonedds` at the pin, so the wire
  behaviour is the version ROS ships and not an approximation of it;
- the verdict is the example's own: the publisher blocks until a reader is
  matched, the subscriber blocks until a sample arrives, and a 25 s timeout is
  the negative.

Measured on the QEMU side separately, because the recipe depends on it: QEMU
11.0.3 accepts `hostfwd=udp::P-10.0.2.15:P` on `-netdev user` and binds those
UDP ports on the host (`ss -lunp` shows `0.0.0.0:17910` and `0.0.0.0:17911`
owned by `qemu-system-arm`). And a Cyclone participant binds its unicast
sockets to `0.0.0.0`, not to the selected interface's address — so it receives
whatever address slirp hands it, whichever of the host's addresses that turns
out to be.

**Not modelled, and each could still bite:**

1. **The RTOS IP stack.** lwIP (FreeRTOS/mps3-an536), NSOS (native_sim), NetX
   Duo (ThreadX) — none of them were in the path. The guest here ran Linux's
   stack. Everything above is about ADDRESSING, which is where the NAT question
   lives, but "does lwIP deliver a 1.2 KB SPDP datagram through slirp's virtual
   LAN9118" is a separate question this does not answer.
2. **The embedded Cyclone build.** ddsrt's FreeRTOS/ThreadX ports, the 32-bit
   pointer width, the composed-config path in `session.cpp`. The config strings
   above compose the same way there (`cyclone_config.hpp` verifies later tokens
   win), but that is read, not run.
3. **`rmw_cyclonedds_cpp`.** The host peer here was plain Cyclone. ROS's RMW is
   the same library with its own QoS and topic-name mapping; discovery is
   Cyclone's, so the crossing carries, but a `ros2 topic echo` was not run.
4. **libslirp vs pasta.** Two implementations of one NAT shape. The properties
   the experiment leans on (gateway-to-loopback mapping, inbound only via
   forwarded ports, no multicast) are documented for both, and the QEMU probe
   above confirms the forwarding half on QEMU itself — but the DDS run was on
   pasta.

#### What this makes affordable

A W1-shaped Cyclone cell is now a configuration job with a known shape rather
than an open question. What it still needs, in order:

1. **A slirp-configured Cyclone image.** The FreeRTOS and ThreadX arms of
   `kEmbeddedCycloneConfig` bake `AllowMulticast=spdp` and no `Peers`; a slirp
   guest needs `false` plus the two Discovery settings. That is a bringup-XML
   or Kconfig-blob change, composed over the baseline — no code change.
2. **`hostfwd` in the launch path.** `scripts/qemu/launch-mps2-an385.sh` has no
   way to express it today (`--slirp` emits a bare `-nic user,model=lan9118`),
   and the port numbers follow from the image's domain id.
3. **A NAT-shaped isolation profile** on the host side — issue 1251.
4. **One host port block per concurrent cell.** slirp binds the forwarded ports
   on the HOST, so two cells on one machine collide unless their domain ids
   differ. The tree already bakes distinct Cyclone domains (50–58) for parallel
   SPDP, and the port arithmetic turns that straight into distinct host ports.

Ordering advice unchanged from the top of this phase: zenoh still goes first,
because it needs none of the four. But "Cyclone cannot cross slirp" is not a
reason to skip it, and this section is here so the next reader does not assume
it was one.

### W3 — a second RTOS, not a second Zephyr board

FreeRTOS on MPS2-AN385 (lwIP) is the strongest candidate: it is a different
kernel, a different IP stack, and it already has QEMU networking. NuttX and
ThreadX are the alternatives.

Pick ONE. The value of this phase is a second *kernel* witness, and picking one
and finishing it beats three half-configured lanes — the shape phase-433 W2 hit
when it ran cells before their fixtures could build.

### W4 — say what the lane covers, in the lane

`live-peer.yml` derives its membership from the verdict ledger, so an on-target
cell joins it automatically once recorded. What does NOT follow automatically is
the runner: these cells need QEMU, and the ledger has no way to say so.

**Acceptance.** Either the lane provisions QEMU, or on-target cells are split
into a sibling lane that does — with the split visible in the stage reporter
added for issue 1158, so "this lane could not run the on-target cells" and "the
on-target cells regressed" stay distinguishable.

### W5 — retire the phrase, or earn it

Whatever W1–W3 land, correct the places that currently call the native_sim cell
on-target verification. `interop.rs`'s section header reads `── Zephyr on-target
QoS interop ──`; phase-433's coverage map inherits the same reading.

**Acceptance.** Every surviving use of "on-target" names a coordinate whose
sockets are not the host's, or says which board it means.

## What this phase does NOT promise

**Not real hardware.** Everything above is QEMU. A QEMU MPS2-AN385 is a genuine
second witness for the IP stack, the pointer width and the libc, and it is not a
witness for timing, for a real PHY, or for anything an FVP or a board would
catch. Saying "on-target" about QEMU is the same overreach this phase exists to
fix one level down; the honest phrase is "on a target-shaped emulator".

**Not every platform.** Ten platforms have no live-peer cell. This phase proposes
covering two (one Zephyr board, one non-Zephyr kernel) and argues that the
marginal value of the third is much lower than the first — the first two answer
"does anything about our RMW depend on host sockets?", and after that the
question is per-port rather than per-class.

**Not the QoS workload's completeness.** W1 moves an existing cell to a new
coordinate. It does not widen what that cell checks, and a QoS cell is not a
substitute for pubsub, service, action or graph coverage on the same board.

**Not a claim about `Px4` or `Fvp`.** Both are in `PlatformId` and neither has a
QEMU networking story in this tree. They are out of scope until one exists.
