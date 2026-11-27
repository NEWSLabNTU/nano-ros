---
id: 3007
title: "check-max-frame keys a baseline row by function alone, so a new image on another target reads as a frame that grew"
status: resolved
resolved: 2026-11-27
resolved_in: "this change: check-max-frame keys rows by (function, image), schema 3"
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

## Resolution (2026-11-27)

`config/max-frame-baseline.json` is schema 3: one row per (function, image),
where the image is the report's `image_key` (the `seen_in` spelling). A
report is judged against its own image's row:

- growth over the same image's row fails, as before;
- a function the baseline knows, on an image it has not measured, is a NOTE
  when at or under that function's largest row on any image, and a FAIL above
  it, so a new image cannot ship a frame bigger than any accepted one;
- a function no row names fails, as before.

A schema-2 row reads as the row of its `seen_in` image, so an old baseline
still loads. `--write-baseline` merges by (function, image) and never drops
another image's row.

The migration ran at the tip after B48 (#5013) re-baselined the nros12
frames. Every row B48 wrote stays as written; a pre-B48 row came back only
where B48 moved its function's `seen_in` and that (function, image) key was
absent. That restored exactly the two rows in the table above, beside B48's
rows for the same functions:

| Function | Restored | Kept from B48 |
| --- | --- | --- |
| `out_of_line<..create_entity closure #3..>` | 4536 B, freertos action_bringup | 4544 B, zephyr sntp-resync talker |
| `install_in_with_launch_params<talker_pkg::Talker,..>` | 7960 B, nuttx demo_bringup | 8048 B, zephyr sntp-resync talker |

Read back, and also unchanged from B48: `app_task_entry_bare` 4224 B and
`open_executor` 5512 B. 57 rows at the tip, 59 written. The gate's self-test
holds the cases above (37 checks).
