# Phase 441 — the one "on-target" live-peer cell runs on host sockets

**Status (2026-09-09). Not started. Analysis below; work items W1–W5 proposed.**

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
finding this phase should record rather than assume.

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

*The lane runs `just native test-live-peer-regression host|board`.* Three things
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
