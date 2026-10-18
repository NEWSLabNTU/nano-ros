# Phase 441 — the one "on-target" live-peer cell runs on host sockets

**Status (2026-10-18). W1–W5 and W7 DONE; REVISED — W6 and W8 are open, and they
are the homed issues' work.** The two cells this phase was opened for have met a live peer:
`zephyr-cortex-m-pubsub-c-zenoh` and `freertos-mps2-pubsub-c-zenoh-n2r` each carry a
recorded `pass` in `.config/interop-verdicts.toml` (2026-10-11, rmw_zenoh_cpp on
Humble, 8.7 s and 5.7 s), and the scheduled `live-peer` lane has run both every night
since: run 39001846062 (2026-10-17) is `live-peer board — VERDICT: cells ran and passed`
with both tests `PASS`. So W1's and W3's "a live verdict is still owed" paragraphs
below are history, kept because they state the prediction the run answered: **zero
defects on the wire** across both crossings (host sockets → Zephyr's stack over
`eth_smsc911x`, and → FreeRTOS + lwIP), i.e. phase-337 W2's five were build defects.

What the phase did NOT do is the thing its homed issues need: a live peer for an RTOS
image on **Cyclone**. Every board cell above is zenoh. W2 established that Cyclone
crosses slirp and what it costs; W6 builds that cell, and it is the in-tree witness
issues 0997 and 0917 have never had (both are reproduced only in the island's repo,
over a root-provisioned tap). W7 re-measures 1281, which passed in tier 2 on
2026-10-16. W8 takes 0814's measure-then-decide. See "Revision 2026-10-18" below.

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

*(As of W1, `ZephyrQemuCortexM` has one — `zephyr-cortex-m-pubsub-c-zenoh`. The
count above is the pre-W1 measurement and is left as written, because it is the
argument the phase was opened on. Ten platforms remain.)*

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

~~`ZephyrQemuCortexM`'s doc comment: *"Cells here are C/C++ only: the pinned
`zephyr-lang-rust` cannot compile for any board whose devicetree has gpio nodes
(issue 0432)."* So the natural first target — the same QoS workload, moved from
native_sim to the Cortex-M board — **cannot be the Rust image**.~~

**REFUTED 2026-09-10 (W1).** Issue 0432 was RESOLVED 2026-08-12 by phase-346
W2/W3; the Rust leaf has built since and `zephyr_cortex_m_rust_zenoh_pubsub_e2e`
runs it. The quoted doc comment was stale when this phase quoted it, and so were
two more copies of the same sentence — see "W1 as built" below. All three are
corrected. W1 still landed as a C cell, but for a different and smaller reason:
C is what the west lane already builds at that coordinate.

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

### W1 as built (2026-09-10) — two amendments and a gate defect

The cell landed as `zephyr-cortex-m-pubsub-c-zenoh` in `interop::CELLS`
(`Tier::Runtime`, `RosEdition(Zenoh)`, `NanoToRos`, `ZephyrWestLeaves`), run by
`tests/pubsub_zephyr_cortex_m_ros2_interop_e2e.rs` and aimable with
`just zephyr test-ros2-cortex-m`. Two things in the plan above were wrong and
one thing in the tree was.

**Amendment 1 — the C/C++-only constraint no longer exists.** The section "One
constraint that is already written down" quotes `ZephyrQemuCortexM`'s doc
comment on issue 0432. **0432 was RESOLVED 2026-08-12 by phase-346 W2/W3**;
`build-cortex-m-rust-talker-zenoh` has built since and
`zephyr_cortex_m_rust_zenoh_pubsub_e2e` runs it. Three places still asserted the
constraint (`matrix.rs`'s `ZephyrQemuCortexM` doc, `binaries/mod.rs`'s
`build_zephyr_cortex_m_example` doc — which says "there is no rust arm" in a
function the Rust cell calls with `"rust"` — and this phase doc); all three are
corrected. The sentence outlived its fact by four weeks and was still being read
as a design constraint, which is precisely the W5 problem one series over.

**Amendment 2 — the workload moves, because the board cannot hold the
workload still.** W1 says "same RMW, same workload, same peer, same crossing
mechanism, one axis moved". The workload could not stay `Qos`: there is no QoS
workspace entry for any board but `native_sim/native/64`, in ANY language
(`examples/workspaces/features/src/` holds `zephyr_rust_{lifecycle,params,qos}_entry`,
all native_sim). A QoS cell on `mps2_an385` therefore means authoring an entry
package, a `[[workspace_fixture]]` row, a west build name and a port bake —
four new artifacts, none of which is the axis W1 exists to move, and all of
which would ship unbuilt on a host with no Zephyr SDK. So the cell reuses the
`build-cortex-m-c-talker-zenoh` leaf the west lane ALREADY builds at exactly
this coordinate (locator `tcp/10.0.2.2:10700` = `port_of(ZephyrQemuCortexM, C,
Pubsub)`) and adds a live peer and nothing else. **`Pubsub`, not `Qos`.** The
axis W1 is about — host sockets → Zephyr's in-kernel stack over `eth_smsc911x`,
64-bit host pointers → 32-bit ARMv7-M — moves exactly as planned.

The test asserts TWO things, and the first is what makes the coordinate a
witness rather than a second spelling of native_sim: `IPv4 address: 10.0.2.15`
from Zephyr's own `net_config` (only the in-kernel stack driving a real
ethernet controller prints it), then `ros2 topic echo --once /chatter` receiving
a sample. The second is the interop claim and the first alone would not do —
the existing baked pubsub cell asserts the image PRINTS `Publishing:`, which the
C talker does whenever `nros_cpp_publish_raw` returns 0, a local return code.
Nothing in the tree had observed a sample LEAVE this board.

**The gate defect — G5's reach was narrower than its rule.**
`matrix_fixture_coverage.rs`'s `interop_bindings_g5_cells_are_lane_narrowable`
rejected the new cell with "produced by NO fixtures.toml row, so no lane can
narrow it", and its doc comment declared that verdict deliberate: "a Runtime
cell that could only be backed by a west row would therefore still fail here —
correctly, because nothing could narrow it". That was true when G5 was written
and had already stopped being true: **issue 0713** put
`require_west_leaf_in_lane(build_name, …)` at the head of
`require_prebuilt_binary_fresh_zephyr`, so every zephyr resolver narrows by
build-dir NAME against `fixtures::lane::west_leaves()` — the same
`examples/fixtures.toml`, read through a different subcommand. G5 was checking
only the PATH-attribution table (`manifest_rows()`), which by construction omits
west rows. Its diagnosis was one the resolver contradicts. G5 now counts all
three backing mechanisms (path rows, workspace rows, west leaves), with a
vacuity precondition on each table. The 0196 shape, in the direction that reads
as correctness.

**Defect count across the board crossing: ZERO in nano-ros itself.** The
expected-finding paragraph above is therefore answered, with a caveat that
weakens it: the crossing was NOT run live here — this host has no `/opt/ros`,
no Zephyr SDK and no west workspace, so the cell reaches its `require_ros2()`
skip and stops. What IS established is that the cell is bound and
coordinate-correct (all 10 `matrix_fixture_coverage` gates, including the
repaired G5), that its runner is named (`check-interop-cell-runners`: 23/23,
ledger still empty), that its nextest group resolves and actually holds both
sharers of the baked port (`cargo nextest show-config test-groups`), and that
the skip fires for the right reason. The five phase-337 W2 defects were BUILD
defects — a header conflict, an arch-gated feature, a missing allocator, a
duplicated cmake string, a missing entropy device — and this cell adds no build,
which is consistent with finding none. **A live verdict is still owed**: the
`.config/interop-verdicts.toml` `pass` and the `live-peer.yml` membership that
W1's acceptance names need a host with ROS 2, a Zephyr SDK and the west lane.
Until that runs, "zero defects on the wire" is unmeasured, not established.

**Incidental, and pre-existing:** `ci_lane::tests::documented_lane_table_is_live`
was already red before this work — four of its six gated numbers had drifted.
It is excluded from `just ci gate` (`test-unit` passes `--exclude nros-tests`),
so it only fires in a sweep. Recomputed and corrected in the same commit,
because adding an interop cell moves one of those numbers (tier 2's cell count
13→12: `cells()` is a greedy set cover, so a cell covering more singles at once
finishes the cover in FEWER picks).

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
[issue 1251](../issues/archived/1251-cyclone-slirp-needs-nat-profile.md).
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
3. **A NAT-shaped isolation profile** on the host side — issue 1251, LANDED
   2026-10-06 as `nros_tests::dds_isolation::CycloneSlirpPair` (tag-isolated,
   measured through libslirp by `tests/cyclone_slirp_pair.rs`).
4. **One host port block per concurrent cell.** slirp binds the forwarded ports
   on the HOST, so two cells on one machine collide unless their domain ids
   differ. The tree already bakes distinct Cyclone domains (50–58) for parallel
   SPDP, and the port arithmetic turns that straight into distinct host ports.

Ordering advice unchanged from the top of this phase: zenoh still goes first,
because it needs none of the four. But "Cyclone cannot cross slirp" is not a
reason to skip it, and this section is here so the next reader does not assume
it was one.

### W3 — a second RTOS, not a second Zephyr board

**Status: the cell is LANDED and has NEVER MET A PEER. Kernel taken: FreeRTOS on
MPS2-AN385.** The declaration, the tripwire, the test and the runner exist; the
verdict does not, and `.config/interop-verdicts.toml` carries no entry for it —
which is what "never run" is supposed to look like here, not an oversight.

FreeRTOS on MPS2-AN385 (lwIP) was the strongest candidate: it is a different
kernel, a different IP stack, and it already has QEMU networking. NuttX and
ThreadX were the alternatives; nothing was found that argued against the first
choice, so no switch was made.

The cell is `freertos-mps2-pubsub-c-zenoh-n2r` — `(FreertosMps2, C, Zenoh,
EntryPubsub, Interop, Runtime)`, peer `RosEdition(Zenoh)`, direction
`NanoToRos`, test `pubsub_freertos_ros2_interop_e2e`, build channel
`FreertosMps2Fixtures` (`just freertos build-fixtures`), runner
`just freertos test-ros2`. It reuses `entry_e2e`'s `workspace-c-freertos` image
and therefore its baked port, so `.config/nextest.toml` puts it in
`matrix-consumers-serial` — placed ABOVE the `binary(~freertos)` →
`qemu-emulated` override, because that substring match would otherwise claim it
first and two different groups do not serialize against each other (the
phase-373 W1 defect, one binary over).

Five axes move at once, and the C is not an accident: the existing on-target row
runs the Rust API, so this one exercises the C ABI, the half of the surface an
RTOS consumer is most likely to be using.

| axis | `zephyr-qos-rust-zenoh` | this cell |
| --- | --- | --- |
| kernel | Zephyr | FreeRTOS |
| IP stack | host (NSOS offload) | lwIP over emulated LAN9118 |
| pointers | 64-bit host | 32-bit thumbv7m |
| libc | host glibc | newlib-nano |
| API | Rust | C |

**Measured while landing it, on a host with no ROS.**

*The mechanism half is alive.* An MPS2-AN385 FreeRTOS image boots under
`qemu-system-arm`, brings up lwIP over the emulated LAN9118, opens a zenoh-pico
session over **slirp** to a router on the host's gateway address, and publishes
— 23 samples in 45 s, with `[INFO] Publishing: 'Hello World: N'` climbing and no
kernel fault. So the kernel, the IP stack, the cross toolchain and the emulator
are not the open question; only the peer is. (Caveat, stated because RFC-0075
exists: the router in that measurement was the SDK store's upstream `zenohd`
1.7.2, **not** ROS's `rmw_zenohd`, so it says nothing whatever about
`rmw_zenoh_cpp` pairing. It answers "does a FreeRTOS guest reach a host router
through slirp", and only that.)

*The cell needs a ROS host for BOTH halves, not just the peer — and that is
new information.* The fixture the cell resolves cannot be BUILT here at all:

```
$ bash scripts/build/workspace-fixtures-build.sh freertos c
  -> workspace-c-freertos (c) examples/workspaces/c
     nros build demo_bringup:freertos …
Error: 2 <depend> name(s) resolve to nothing:
  example_interfaces — declared by …/service_server_pkg/package.xml, …
  std_msgs           — declared by …/talker_pkg/package.xml, …
```

Neither name is a package in that workspace, neither is in
`packages/interfaces/` (which carries only `diagnostic-msgs`,
`lifecycle-msgs`, `rcl-interfaces`, `rosgraph-msgs`), and neither is a
`[prereq.*]` with `role = "package"` in `nros-sdk-index.toml`. They resolve
through ament or not at all. That is a property of the workspace-fixture lane
generally, not of this board — it is why `live-peer.yml` builds its fixtures on
a ROS runner — but it sharpens W4 below from a scheduling question into a
provisioning one.

*Defects exposed by the second-kernel crossing so far: **zero**, and the number
is not yet meaningful.* W1 states the falsifiable prediction (phase-337 W2 found
five defects moving a workload across a comparable boundary). It cannot be
evaluated from this side of a peer: nothing in this cell has been run against
`rmw_zenoh_cpp`, so "no defects" here means "no observations", not "no bugs".
Do not read the green `check-interop-*` gates as evidence — every one of them is
a statement about the declaration, which is exactly the class
`check-interop-verdicts` exists to separate from a result.

**What is left, and it is one thing:** run
`just freertos test-ros2` on a host with ROS 2 + `rmw_zenoh_cpp`,
`arm-none-eabi-gcc`, `qemu-system-arm` and the FreeRTOS + lwIP submodules, then
`check-interop-verdicts.py --record freertos-mps2-pubsub-c-zenoh-n2r --junit …`.
Until that happens the cell is honestly unproven and says so.

### W4 — say what the lane covers, in the lane

`live-peer.yml` derives its membership from the verdict ledger, so an on-target
cell joins it automatically once recorded. What does NOT follow automatically is
the runner: these cells need QEMU, and the ledger has no way to say so.

**Acceptance.** Either the lane provisions QEMU, or on-target cells are split
into a sibling lane that does — with the split visible in the stage reporter
added for issue 1158, so "this lane could not run the on-target cells" and "the
on-target cells regressed" stay distinguishable.

**LANDED 2026-09-10 — SPLIT, and the split is DERIVED.**

*Which option, and why.* A sibling job (`board`) in the same workflow, gated on
the ledger actually containing a row that needs one. Provisioning the toolchain
in `regression` was rejected on this lane's own stated reason: it exists
separately from `just ci tier1` because "verification must not be hostage to
everything else being green", and putting a Zephyr SDK, a west update and an
emulator in front of nineteen host cells that touch none of them re-creates
exactly that hostage relationship — every provisioning flake would cost the
whole lane its verdict. The other half of the trade is the cost of a split, and
both halves of it are paid down rather than accepted: the second lane does not
exist when there is nothing for it to run (`if: needs.membership.outputs
.has_board == 'true'`), and it reports through the SAME stage reporter, so the
run list carries two named answers instead of one job with two meanings.

*The split is computed, never recorded.* A cell's runner follows from its
PLATFORM coordinate, which `interop::CELLS` already states — so the ledger gains
no `needs_qemu` field (a second source that drifts the moment a cell moves
board). `check-interop-cell-runners.py` now parses the platform beside the tier;
`check-interop-verdicts.py --runner host|board` narrows any listing by it, and
`--scopes setup|build` / `--narrowing` derive what the board job must provision
and which fixture leaves it must build. Two authored maps back that, and both
fail CLOSED: an unmapped board is an error naming the platform, an unmapped cell
is an error naming the cell, and every scope token is checked against
`scripts/build/scope.sh` while every narrowing value is checked against
`examples/fixtures.toml`.

*The lane runs `just linux test-live-peer-regression host|board`.* Three things
that were wrong there had to be fixed for the acceptance to be satisfiable at
all:

* **The board row was already in the host lane.** `zephyr-qos-rust-zenoh` has
  had a recorded PASS since the ledger's first entry, and the host container
  builds no Zephyr image — so that cell resolved no fixture, skipped, and was
  counted green. Not a future hazard: today's state.
* **A regression could not be reported as one.** The loop classified `100` as
  "tests ran and failed", but it calls `_test-focused`, which swallows nextest's
  100 and answers 1 — so every real failure would have been reported as a
  harness fault. It now reads the junit (`_count-real-failures`) rather than the
  exit code alone.
* **A skipped membership read as a pass.** `--assert-ran` checks that every
  in-scope cell with a recorded PASS produced a NON-SKIP result, over the junits
  of the whole run; a membership that only skipped is exit 2, "this lane could
  not run". Because that exposure is only useful if the absence can be supplied,
  both jobs now provision the declared system closure — neither CI image bakes
  `ros-humble-rmw-zenoh-cpp`, so without it every zenoh cell skips.

*The "could not run" case is named, three ways.* `lane-stage.py` gained the
fourth axis it needed: a cells STEP failing does not say whether the cells
produced results, so the workflow forwards the recipe's own answer
(`NROS_LANE_CELLS_RAN`) and the reporter says `NO VERDICT: the cells could not
run` instead of `VERDICT: cells ran and FAILED`. `stage-board`'s job NAME
distinguishes the three non-verdicts a skipped board job can mean — the ledger
could not be split, there was nothing to run, or the job never started — so a
grey tick never reads as "fine". Both jobs are in `LANES`, and `--selftest`
cross-checks each against the YAML in both directions.

*Left deliberately.* The board job runs on a GitHub-hosted runner in
`nano-ros-zephyr-ci` (the image nightly's Zephyr line uses daily) rather than on
the self-hosted `nros-qemu, nros-sdk-zephyr` fleet: the fleet has the toolchains
but its ROS story is unverified, and `NROS_SELF_HOSTED_READY` gating would make
this lane's verdict depend on a fleet variable. When W1's Cortex-M cell lands,
its `qemu` scope is already derived and provisioned; whether the SDK's own QEMU
suffices there is W1's measurement, not an assumption made here. A board cell on
a NON-Zephyr kernel (W3) will ask for a scope this image cannot provision, and
the setup step FAILS naming it rather than running a green over cells it never
built.

*What W3 adds to this.* The FreeRTOS/MPS2 cell is exactly the non-Zephyr case in the
last paragraph: it needs `arm-none-eabi-gcc`, `qemu-system-arm` and the FreeRTOS + lwIP
submodules, none of which `nano-ros-zephyr-ci` provisions. Written before this split
landed, W3 had recorded the consequence under the OLD lane — the cell would be selected,
skip, and read as green. The split and `--assert-ran` retire that: until a scope provides
those four, the board job fails naming the scope, a NO VERDICT and never a pass. Providing
them is the one item W3 leaves open here.

### W5 — retire the phrase, or earn it

Whatever W1–W3 land, correct the places that currently call the native_sim cell
on-target verification. `interop.rs`'s section header reads `── Zephyr on-target
QoS interop ──`; phase-433's coverage map inherits the same reading.

**Acceptance.** Every surviving use of "on-target" names a coordinate whose
sockets are not the host's, or says which board it means.

**LANDED 2026-09-10 — 169 uses examined, 20 changed.**

The sweep covered every non-archived `on-target` / `on target` / `on_target` in
the tree (243 including `archived/`, which is a record and was left alone). The
uses split into four kinds, and only one of them is the overclaim:

1. **Not the phrase.** `non-target`, `json-target-spec`, `corrosion-target`,
   "depends on target" — the CMake and cargo senses, which is most of the
   non-hyphenated half. Untouched.
2. **A real target.** `emulator.rs`'s baremetal MPS2 cells, RFC-0074's cadence
   "taken from an on-target guest clock" on the FreeRTOS mps2-an385 QEMU lane,
   phase-372's "on-target smoke when hardware is available". These name a
   coordinate whose sockets are not the host's. Left.
3. **A feature, not a coordinate.** RFC-0052's `on-target contract monitors` —
   `nros-diagnostics`, `executor/monitor.rs`, the parity ledger's `why` text,
   the generated messages' `STAMP_OFFSET` comments, phase-296 W3b. The phrase
   there is the RFC's own vocabulary for "checked in the image rather than by an
   external monitor node subscribing to `/statistics`", it is true on every
   board the executor runs on, and it makes no claim about a test's reach.
   24 of the 64 non-phase-441 hyphenated hits, left — rewriting an RFC's
   vocabulary is the mass rewrite this item was told not to do.
4. **The overclaim: `native_sim` called on-target.** 20 lines, all in the
   phase-276 Zephyr entry family and its interop sibling. Fixed.

The replacement word was already in the tree: `entry_e2e.rs` says **in-image**
three lines from where it said on-target, and that is exactly the distinction
those sentences are drawing (the nano-ros application vs the test harness and
the ROS peer). So the fix is one word, not a rephrasing:

| where | was |
| --- | --- |
| `interop::CELLS` section header | `── Zephyr on-target QoS interop ──` → `native_sim`, plus why |
| `entry_e2e.rs` (8) | on-target seeding / endpoints / store / listener → in-image |
| `qos_zephyr_ros2_interop_e2e.rs`, `interop_e2e.rs`, `zephyr.rs` | "both nodes on-target" → in-image; "the on-target zephyr-image QoS interop" → "the zephyr native_sim image" |
| `examples/fixtures.toml` (2), the qos + safety entry crates (3) | the on-target pair / safe_listener → in-image |
| `just/zephyr-dev.just` | "a publisher ON TARGET" → "in a Zephyr NATIVE_SIM image" |
| `executor/spin.rs` | "Measured on-target" → "Measured on that native_sim image" (the same comment already said native_sim two lines up) |
| phase-433's coverage map | "One on-target cell exists" → "One non-Linux cell exists", with the correction noted in place |

Left deliberately, beyond kinds 1–3: `spin.rs`'s "a growing count is the
on-target signal that a tier is not keeping up" (the signal available inside any
image, contrasted with an external observer differencing timestamps);
phase-433's W7 heading, which describes the cell this phase is about to build
and is quoted verbatim above; `docs/issues/README.md`, which is generated.
W4's own new code uses the phrase twice, both times to say why it is not the
vocabulary of the runner split.

## Revision 2026-10-18 — the remainder is Cyclone, and three homed issues

Read against `main` at `6486216ab7`. What changed since 2026-09-10:

- **W1 and W3 met a peer** (status above). Their acceptance is met.
- **W2's four prerequisites, re-read.** (1) The FreeRTOS and ThreadX arms of
  `kEmbeddedCycloneConfig` still bake `AllowMulticast=spdp` with no `Peers` — open.
  (2) `hostfwd`: the PORTS are now derived in the harness
  (`dds_isolation::CycloneSlirpPair::qemu_hostfwd`, issue 1251), but the launch path a
  cell uses still has to carry them — open. (3) The NAT-shaped host profile LANDED
  (issue 1251, `CycloneSlirpPair`, measured through libslirp by
  `tests/cyclone_slirp_pair.rs`). (4) One port block per concurrent cell: follows from
  the baked domain, as written.
- **The board for the Cyclone cell exists and already delivers in-image.**
  `workspace-cpp-mps3-an536-freertos` (Cortex-R52, FreeRTOS, lwIP over LAN9118, C++,
  CycloneDDS) is a `fixtures.toml` row consumed by `tests/freertos_qemu.rs`, which
  asserts in-image delivery. That is the same kernel, IP stack and NIC as the island
  image issues 0917 and 0997 were found on.
- **1281's test passed** in tier 2 (run 38933437082, 2026-10-16, newslab-118,
  `c_riscv_nuttx_talker_delivers_cross_process` 3.5 s). One pass is not a closure
  under a 28–79 s intermittent drop, which is why W7 is a re-measurement.

### W6 — a Cyclone RTOS image meets a stock peer over slirp

The mps3-an536 FreeRTOS C++ Cyclone entry against a stock `rmw_cyclonedds_cpp`
peer, over slirp, with W2's recipe: the guest composes `AllowMulticast=false`,
`ExternalNetworkAddress`, `ParticipantIndex` and one `Peers` entry over the baked
baseline (a bringup-XML change, no code), the host peer takes
`CycloneSlirpPair`'s profile, and QEMU forwards the two unicast ports the domain
implies.

Run it for at least 60 s, not to the first sample. That is what makes it the
witness for **issue 0997**: the island's participant announced SPDP once and every
peer expired its lease ~10 s later, so a cell that stops at the first delivery would
pass on exactly the image that defect lives in. Then a second leg with one sample
larger than a datagram (8 fragments, the island's trajectory shape) is the witness
for **issue 0917**.

**Acceptance.** An `interop::CELLS` row (`Tier::Runtime`, `Cyclone` peer,
`assert_test_bound`), a recorded verdict in `.config/interop-verdicts.toml` written
by `--record` from a real junit, and the `live-peer` board job building and running
it (its scope must provision the an536 toolchain; until it does, the board job
fails naming the scope, per W4). Whatever the 60 s and fragmented legs show is
recorded in 0997 and 0917 — a pass narrows both to the tap/hardware path, a failure
is their first in-tree reproduction. Either is the result.

### W7 — re-measure 1281, then close or narrow it

Run `c_riscv_nuttx_talker_delivers_cross_process` solo, 10 times, on freshly built
fixtures, with `ZENOHD_LOG=debug` on the router as the issue asks. 10 of 10 closes
1281 with the run recorded; any drop gets the router log the issue says it needs to
tell a lease expiry from a close.

**DONE 2026-10-18.** 10 of 10 PASS, 3.48–3.53 s each, on fixtures built fresh in
the same session on newslab-118. 1281 is resolved as not reproduced, with the run
recorded in the issue.

### W8 — 0814: measure the copy count, then pick an option

0814's own section "Options, none chosen" says the decision needs the copy-count
measurement (memcpy'd bytes on the publish path: `publish_streamed` vs `publish_raw`
vs `try_loan`) on the cells that already exist, not a delivery lane. Take that
measurement, choose among the issue's three options, and meet its "Acceptance for
this half". An embedded lending lane is only built if the chosen option keeps the
publish half.

**Measurement DONE 2026-10-18; the choice is open.** On native zenoh at this pin
(512 B, 200 publishes each, `memcpy` interposed): `publish_raw` copies the payload
once (`_z_slice_copy`, from `z_bytes_copy_from_buf`), `publish_streamed` also once
(its 1 KiB chunk is copied by `z_bytes_writer_write_all`), and `try_loan` + `commit`
**zero** times (`z_bytes_from_static_buf`). So the loan is the only zero-copy publish
on zenoh, which argues against withdrawing it (0814's option 3), and
`publish_streamed`'s saving is RAM, not copies. Details and caveats are in 0814.
What remains is the maintainer's choice of option, then the issue's acceptance.

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

## Issues homed here (survey 2026-09-11)

Every open issue was checked for a home phase; these had none, or were mentioned
here only in passing. A mention is not an owner — an issue with no work item is
an issue nobody is accountable for. Each row is a work item: the issue holds the
evidence, the item is *close it*.

| issue | why it belongs here |
| --- | --- |
| [#1139](../issues/archived/1139-cyclone-ros2-pubsub-e2e-flakes-under-the-parallel-gate.md) | `nros_rmw_cyclonedds_ros2_pubsub_e2e` fails under the 21-way parallel run. Today it is a row in phase-444's verdict table reading 'open | Acceptance, still unmet' — a status, not an owner; its acceptance needs a live peer, which is this phase |
| [#1251](../issues/archived/1251-cyclone-slirp-needs-nat-profile.md) | a Cyclone peer for a QEMU guest cannot be loopback-pinned. It is W2's own residue and appears only as prerequisite #3 of a prose list |
| [#0917](../issues/0917-an536-fragmented-sample-never-syncs.md) | re-homed 2026-10-12 from phase-358 (archived). Interrupt-driven RX landed, but the an536 FreeRTOS Cyclone target still never syncs a fragmented sample. Getting an on-target verdict on it is this phase's job |
| [#0997](../issues/0997-island-announces-spdp-once-then-lease-expires.md) | re-homed 2026-10-12 from phase-414 (archived). On FreeRTOS + Cyclone the timed-event tree empties itself, so the SPDP resend is scheduled and never lands. The root cause is still unknown, and only an on-target run can show it |
| [#0814](../issues/0814-lending-never-exercised-on-hardware.md) | re-homed 2026-10-13 from phase-433 (archived). The zero-copy lending surface is enabled only by a posix test crate, so no embedded target has run it. An on-target cell with lending on closes it. |

## Issues homed here (2026-10-17)

Homed when every open issue was given a phase. Each issue has a `**Home:**` line pointing here.

| issue | why it belongs here |
| --- | --- |
| [#1281](../issues/archived/1281-riscv-nuttx-c-talker-router-drops-session.md) | The NuttX RISC-V C talker's router drops its session after 28-79 s. This needs an on-target verdict. Re-homed from phase-414, which is archived. |
