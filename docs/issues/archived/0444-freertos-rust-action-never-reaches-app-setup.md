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

## Root cause (2026-08-06) — two faults, both from the same carve-out

Reproduced on `main` with freshly built fixtures, so it is not branch-specific.
It is also not action-specific: `pubsub` and `service` fail identically. The
issue's scope table was drawn from one action run and was wrong about that.

The `rtos_e2e` FreeRTOS dispatch carved the Rust lane out of the board-net
launcher on the premise that the Rust images "keep the historical DEFAULT-slirp
plan (guest 10.0.2.15, host 10.0.2.2)". That premise had stopped being true —
the board crate brings up the STATIC plan for every lane, which the boot banner
says out loud (`IP: 192.0.3.10`) in the very output pasted at the top of this
issue.

**Fault 1 — an unroutable network.** The Rust images got `-nic user,model=
lan9118` (slirp's default 10.0.2.0/24) while their lwIP was configured
192.0.3.10 / gw 192.0.3.1. Nothing answers that gateway's ARP, and the baked
locator was `tcp/10.0.2.2:7447` — an address the firmware cannot route to AND a
port the harness never serves (it serves `zenohd_port_for(variant, Rust)` =
7800/7810/7820). gdb on the stalled image put it in `_z_open_link` under
`zpico_open` ← `CffiSession::open_with_vtable` ← `app_task_entry_runtime`: a
blocking TCP connect that never returns, which is exactly a boot that stops
after "Network ready." with no error line.

Fixed by giving every FreeRTOS image the board-net launcher and baking
`tcp/192.0.3.1:<per-variant port>` in each Rust package's
`[package.metadata.nros.deploy.freertos]`, matching the C/C++ rows in
`examples/fixtures.toml`.

**Fault 2 — one ZID for two peers.** With the network fixed, the LISTENER
reached "Application setup complete" and the TALKER still hung. The FreeRTOS
platform PRNG is seeded from `(ip, mac)` precisely because zenoh-pico's ZID
comes off it; `freertos_c_entry.c` says so in as many words ("Unseeded, two QEMU
instances derive the same ZID; the router keeps ONE peer (max_links=1) and
rejects the second connection"). Both Rust images booted 192.0.3.10 /
02:00:00:00:00:00, so whichever connected second was rejected. The C/C++ rows
have always split this via `NROS_ENTRY_IP_LAST` 10/11; the Rust lane had no
equivalent set, and `Config::listener()` (ip .11, mac ..01) stopped being
selected when phase-338 W2 collapsed the `-entry` packages into the node
packages.

Fixed by setting `ip = "192.0.3.11"` on the second image of each pair
(listener / service-client / action-client) via the `DeployOverlay`.

`ab486a8db` was named as a suspect above. It is implicated in fault 2 only, and
was never bisected — the two faults were found by reading the boot banner
against the launcher, and by gdb, not by bisection.

## Verification

`rtos_e2e` FreeRTOS, all nine cells: **9 passed**, Rust/C/C++ × pubsub/service/
action. Before: Rust 0 of 3 (all three stalled at "Network ready."), C/C++ 6 of
6.

The two C++ cells that read STALE mid-investigation were a separate, correct
verdict — `nros-cpp/include/nros/client.hpp` had genuinely been edited and the
C++ fixture build was failing the C/C++ sizes split-brain guard on incremental
state. Wiping the six `build-zenoh/` dirs and rebuilding cleared it, which is
the documented "core-crate change ⇒ wipe workspace build dirs" hazard, not a new
defect.
