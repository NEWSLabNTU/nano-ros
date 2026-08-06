---
id: 444
title: The FreeRTOS Rust action image boots and brings up the network, then never reaches application setup
status: resolved
type: bug
area: rmw
related: [issue-0442, issue-0350, phase-338]
---

## Symptom

`rtos_e2e::test_rtos_action_e2e` on `Freertos` / `Rust`:

```
freertos E2E failed — readiness pattern 'Application setup complete' not observed.
Output so far (truncated):
  ========================================
    nros FreeRTOS Platform
  ========================================
  Initializing LAN9118 + lwIP...
    MAC: 02:00:00:00:00:00
    IP:  192.0.3.10
  Network ready.
```

The image boots, the platform banner prints, LAN9118 + lwIP come up and the
interface gets its address. Then nothing. It stalls between "Network ready" and
application setup — so the platform is fine and the application never runs (or
blocks during registration / session open).

Fails SOLO, 3 of 3 retries, so it is not the QEMU-under-load flake class.

## This was hidden, and that is the interesting part

The cell has not been RUNNING. It reported `[SKIPPED] … STALE` against
`zpico-sys/c/include/zpico.h` — issue 0442, where one arm of the cmake freshness
probe did not apply the regenerated-header exemption its sibling did. Fixing
0442 made the cell execute, and it failed immediately.

So a probe defect was masking a runtime defect: the lane looked like a staleness
problem for as long as the fixture was never launched. That is issue 0350's
class exactly — a coordinate that never runs is indistinguishable from one that
cannot.

## Scope

`Freertos` + `Rust` only. On the same board and network, in the same run:

| cell | result |
| --- | --- |
| `Freertos::C` | pass |
| `Freertos::Cpp` | pass |
| `Freertos::Rust` | **fails at readiness** |
| `Nuttx::{Rust,C,Cpp}` | pass |
| `ThreadxLinux::{Rust,C,Cpp}` | pass |

8 of 9 action Runtime cells pass; this is the one.

## What is known and not known

* The entry wiring LOOKS intact: `src/main.rs` carries `nros::main!()` with
  `[package.metadata.nros.entry] deploy = "freertos"`, and `src/lib.rs` has the
  `register` body.
* The last commit to touch the package is `ab486a8db` (phase-338 W2, "collapse
  the 18 `-entry` packages into their node packages") — the same commit that
  dropped the NuttX boards' static link args (issue 0440). That makes it a
  natural suspect, but suspicion is not evidence and this has NOT been bisected.
* Not attributed to a branch. It was found on `phase-339-nuttx-export-snapshot`,
  whose changes are NuttX-only plus the 0442 probe fix; none of them touch the
  FreeRTOS runtime. A main comparison is the next step and has not been run.

## Next step

Run the cell on `main` with freshly built fixtures. If it fails there too this
is a main regression to bisect around `ab486a8db`; if it passes, the difference
is in this branch and the assumption above is wrong.

## Root cause (2026-08-06) — two faults, both from the phase-338 W2 collapse

Reproduced on `main` with freshly built fixtures, so not branch-specific. Also
not action-specific: `pubsub` and `service` fail identically. The scope table
above was drawn from one action run and is wrong about that.

Every FreeRTOS image needs two things the `-entry` packages used to supply and
the collapsed node packages did not.

**Fault 1 — a network the firmware could not route on.** The Rust images kept
`locator = "tcp/10.0.2.2:7447"` with no `ip` / `gateway` in their deploy block,
so lwIP came up on the board's STATIC default (192.0.3.10 / gw 192.0.3.1 — the
boot banner prints it) while the harness launched them on slirp's default
10.0.2.0/24. Nothing answers that gateway's ARP, and 7447 is a port the harness
never serves anyway (it serves `zenohd_port_for(variant, Rust)` =
7800/7810/7820). gdb on a stalled image:

    _z_open_link <- _z_new_transport <- _z_open <- z_open <- zpico_open
    <- create_session_trampoline <- CffiSession::open_with_vtable
    <- app_task_entry_runtime

A blocking TCP connect that never returns is exactly a boot that stops after
"Network ready." with no error line.

**Fault 2 — one ZID for two peers.** Fixing only the network gets the LISTENER
to "Application setup complete" and leaves the TALKER hung. The FreeRTOS
platform PRNG is seeded from `(ip, mac)` precisely because zenoh-pico's ZID
comes off it; `freertos_c_entry.c` says so: "Unseeded, two QEMU instances derive
the same ZID; the router keeps ONE peer (max_links=1) and rejects the second
connection." Both Rust images booted the same address pair, so whichever
connected second was rejected. The C/C++ rows have always split this via
`NROS_ENTRY_IP_LAST` 10/11; `Config::listener()` (ip .11, mac ..01) stopped
being selected when the `-entry` packages were collapsed.

`ab486a8db` was named a suspect above. It is the cause of both faults — but that
was established by reading the boot banner against the launcher and by gdb, not
by the bisection the issue called for.

## Resolution

Fixed upstream by `07faa2383` ("the freertos collapse dropped ip/gateway and the
platform dep"), which restores the DEFAULT-slirp plan in each Rust package's
`[package.metadata.nros.deploy.freertos]` — `ip` 10.0.2.15 / 10.0.2.16 split per
pair, `gateway = "10.0.2.2"`, and the per-variant router port — so the images
match the plain launcher `rtos_e2e` already hands them.

I had independently fixed the same two faults the other way (move the Rust lane
onto the C/C++ board-net plan and unify the launcher). Both work; upstream's had
landed, so it stands and mine was dropped. Recorded because the two approaches
disagree about something real: whether the Rust lane should keep a SEPARATE
network plan from C/C++ on the same board. `rtos_e2e::start_process` still
carries the carve-out and its own comment calls unifying the plans follow-up
work. That is still open, now with the observation that a lane whose firmware
config and launcher are maintained apart is a lane where they can silently stop
matching — which is this issue.

## Verification

`rtos_e2e` FreeRTOS, all nine cells on the rebased tree: **9 passed**,
Rust/C/C++ x pubsub/service/action. Before: Rust 0 of 3 (all three stalled at
"Network ready."), C/C++ 6 of 6.

The two C++ cells that read STALE mid-investigation were a separate, CORRECT
verdict — `nros-cpp/include/nros/client.hpp` had genuinely been edited and the
C++ fixture build was failing the C/C++ sizes split-brain guard on incremental
state. Wiping the six `build-zenoh/` dirs and rebuilding cleared it: the
documented "core-crate change => wipe workspace build dirs" hazard, not a new
defect.
