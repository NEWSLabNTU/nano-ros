---
id: 3007
title: "check-max-frame keys a baseline row by function alone, so a new image on another target reads as a frame that grew"
status: open
type: tech-debt
severity: low
area: [ci, testing]
found: 2026-11-27
related: [2815, 2846]
---

## What happens

`config/max-frame-baseline.json` holds one row per function, with the crate
hashes stripped and no target. The size a row records is the largest any build
had, and `seen_in` names one such build. A generic function is
monomorphised and laid out per image, so the same row can be measured
differently on two targets with no code change.

The nros12 tier-2 nightly at 0e4afce2fa failed on exactly that. Commit
d0e05a9159 (issue 2846) added a new lane image,
`workspace-rust-zephyr-mps2-talker-clock-sntp-resync` (mps2/an385, Zephyr
3.7), after the baseline was written. The image measured two frames above
rows taken on other images, and the gate reported them as growth:

| Function | Row (`seen_in`) | New image |
| --- | --- | --- |
| `out_of_line<..create_entity closure #3..>` | 4536 B (freertos action_bringup) | 4544 B |
| `install_in_with_launch_params<talker_pkg::Talker,..>` | 7960 B (nuttx demo_bringup) | 8048 B |

Per commit on FreeRTOS, from the baseline to 0e4afce2fa, neither function
moved. The rows were raised by a measured re-baseline with that reason.

## Why it matters

"Grown from N B" names the wrong cause, so the next person goes hunting for a
commit that does not exist. It cost one release blocker's attribution pass.

## Fix direction

Key a row by function and target (triple, or triple and board), or record the
size per `seen_in` and compare a report only against its own target's rows. A
new target then reads as "not yet measured on this target", which is a
separate, visible outcome.
