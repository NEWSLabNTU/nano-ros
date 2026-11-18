---
id: 2488
title: "The zenoh CAN links (CAN FD multicast, ISO-TP unicast) have never run on CAN hardware, and no phase tracks it"
status: open
type: limitation
severity: medium
area: [rmw-zenoh, can, testing]
related: [rfc-0080, rfc-0081, rfc-0083, phase-377, phase-394, 1822, 2522, phase-555]
found: 2026-11-13
---

**Found by** the phase-555 W2 review of RFC-0080 to RFC-0083, item B13 in
`docs/roadmap/data/phase-555-w2-rmw-wire-brief.md`.

## Measured

- The CAN links of RFC-0080 (zenoh-pico, CAN FD multicast `id`/`match`/`mask`),
  RFC-0081 (zenoh-rs) and RFC-0083 (unicast over ISO-TP) are implemented and
  tested on `vcan0` only. Their phases (377, 378, 387, 393 and 394) are all
  archived.
- The hardware step was planned twice and done neither time:
  - phase-377 W6 (CAN on a board) was never run;
  - phase-394's "Tier 3 hardware" (`docs/roadmap/archived/phase-394-*.md`,
    around line 699) is listed as future work.
- No open phase or issue carries it. The one board phase that could,
  phase-512 (the MR-CANHUBK344 safety island), runs that board over SERIAL
  only. The only CAN item phase-512 holds is issue 1822, the zenoh-rs
  per-message priority.
- Upstream review is under way: zenoh#2757, zenoh-pico#1298 and zenoh-c#1322
  (`phase-394-*.md`, around lines 708-712). Hardware evidence would help
  those PRs too.

## What is untested on real silicon

- **CAN FD bit timing and data-phase switching.** `vcan0` has no bit timing.
- **Arbitration under bus load, and error frames.** A real bus can lose
  frames; `vcan0` cannot.
- **The classic-CAN refusal.** The pico link refuses a classic interface at
  open (`src/link/multicast/can.c:123-133` in the fork); ISO-TP is the classic
  route.
- **Zephyr's CAN driver path** (`nros-platform-zephyr/src/can_start.c`) on a
  real controller.

## Next

Decide whether CAN hardware validation is in scope:

- **If yes:** home it in a phase. phase-512 is the natural one, since its
  board has CAN FD transceivers. Name the board and the acceptance: a
  talker/listener pair over CAN FD multicast, and a service over ISO-TP,
  between the board and a Linux host with a USB-CAN adapter.
- **If no:** record "vcan-validated only" in RFC-0080 and RFC-0083's status
  lines (brief item B13).

## Status (2026-11-17): needs a scope decision

Still true: no phase homes it, and the RFC-0080 / RFC-0083 status lines
carry no "vcan-validated only" stamp (grep is empty). In scope means homing
it in phase-512 with a board and an acceptance. Out of scope means stamping
both RFCs, so the claim matches what was measured.

**Suggested owner: a user / maintainer decision.**

## Merged from issue 2522 (2026-11-18)

Issue 2522 was filed for the same gap a day later, from the same phase-555
review. It is archived as a duplicate of this one. What it added:

- Besides `vcan0` on Linux, the links were also tested on Zephyr's own
  simulated CAN; neither has bit timing or real bus errors.
- phase-394 says twice that tier 3 (MR-CANHUBK344 to a Linux host) is "the
  only thing that closes" its open behaviour questions; phase-377's status
  line ends "Remaining: W6 (hardware)."
- Closing it in scope means a work item that runs the board over CAN
  against a Linux peer for BOTH links (multicast and ISO-TP unicast), with
  the result recorded in RFC-0080 and RFC-0083.
