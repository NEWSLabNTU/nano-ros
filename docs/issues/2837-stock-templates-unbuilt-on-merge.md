---
id: 2837
title: "No check builds what `nros new` makes for each stock template and board: a stock Rust action-server template on nucleo-f767zi-zephyr fails to link, unseen"
status: open
type: test-gap
severity: medium
area: [ci, templates, cli, zephyr]
related: [issue-2783, issue-2772, issue-1453, phase-557]
found: 2026-11-22
---

**Home: [phase-557](../roadmap/phase-557-stack-floor-measured-frames.md)**,
whose `config/stack-floors.toml` names the template × board images and whose
copy-out shard is the merge-queue check this extends.

## What happens

`nros new <name> --template action-server --lang rust --rmw zenoh --board
nucleo-f767zi-zephyr`, then `nros build`, fails on `main`:

```text
region `RAM' overflowed by 77440 bytes
```

The stamped main stack is 77,728 B. Sizing it from the measured peak (the
user's ruling, #4617) leaves the image about 33 KB over, so something else in
it has grown. The fix is issue 2772's, nros 139's #4843. This
issue is about why nothing noticed: a user's first `nros build` of a stock
template was the first build it got.

## What builds templates today

- `check-template-copy-out` (`scripts/check-template-copy-out.sh`) on the
  merge queue (gate.yml, issue 1453) builds:
  - every copy-out project under `examples/templates/`;
  - one scaffold, `nros new … --template talker`, on `SCAFFOLD_BOARDS`
    (`linux`, `mps2-an385-baremetal`, `runner-package`).
- Its `zephyr` part runs nightly (`template-copy-out-zephyr`), but it drops
  the scaffolds (`[ "$part" = zephyr ] && scaffold_boards=()`), so no
  Zephyr scaffold is built at all.
- `config/stack-floors.toml` names six template × board images (talker and
  action-server, Rust and C, on nucleo-f767zi-zephyr, mps2-an385-zephyr and
  mps2-an385-freertos). Only the stack-floor twin lane builds them
  (`just zephyr stack-floor-twin`), and no workflow runs it.

So of the six `--template` roles × the boards a getting-started page or a
floor row names, a merge builds one role on two boards.

## Wanted

- Every stock template × every board that `config/stack-floors.toml` or a
  getting-started chapter names is scaffolded with `nros new` and built with
  `nros build`, as the copy-out check builds its scaffolds today: on the
  merge queue where the toolchain is provisioned, else on the nightly
  `zephyr` part. It must not be dropped there.
- A combination that cannot build is listed with its reason, not skipped
  quietly.
- The set is derived from the template list and the floor table, so a new
  template or a new floor row is built without an edit to the check.

## Acceptance

The check fails on today's `action-server/rust/zenoh@nucleo-f767zi-zephyr`
link overflow, and passes once #4843 (issue 2772) lands.

## Design and price (2026-11-27, nros 118)

Read-only study. Nothing was built for it: host 118 was quiet for the
release's evidence lanes.

**The set, derived, in three parts:**

1. **Every template image in the floor table.** That is each `[[observed]]`
   `image = "<role>/<lang>/<rmw>@<board>"` row in `config/stack-floors.toml`.
   Today there are six: talker and action-server in Rust on
   nucleo-f767zi-zephyr and mps2-an385-zephyr, and talker in C on
   nucleo-f767zi-zephyr and mps2-an385-freertos. This part carries the
   acceptance case (`action-server/rust/zenoh@nucleo-f767zi-zephyr`). A new
   floor row is built with no edit to the check.
2. **Every board a book chapter scaffolds.** Each `nros new … --board <b>` in
   `book/src`, with `--template talker`, as `SCAFFOLD_BOARDS` does today but
   read instead of listed. Measured on main there are 11 such boards, plus
   `my-board`, a chapter's placeholder, which is refused with that reason.
3. **Every role × language on the host board** (`linux`): 6 roles (talker,
   listener, server, client, action-server, action-client) × Rust, C and C++.
   That is 18 host images, so every template compiles at least once wherever
   it is used.

**Refused, never skipped.** A combination that cannot build goes in
`config/template-scaffold-refusals.toml`, each with its reason and an issue.
The check prints it as NOT VERIFIED through the skip ledger, as it does for
copy-outs.

**One reader.** `scripts/lib/template_scaffolds.py` returns the set, with a
self-test against a synthetic floor table and book tree.
`check-template-copy-out.sh` takes its scaffolds from it, and the `zephyr`
part stops dropping them (`[ "$part" = zephyr ] && scaffold_boards=()` goes):
each part builds the scaffolds whose board belongs to it.

**Price** (from existing runs; nothing new measured):

| Where | Added | Basis | Cap |
| --- | --- | --- | --- |
| Nightly `template-copy-out [zephyr]` | 4 Zephyr floor images, plus the Zephyr chapter boards not already among them, about 5 to 6 images: **~20-25 min** | that job built 5 Zephyr projects in 21 min on 2026-11-26 (05:16 to 05:37, job 139706142308), about 4 min each | 85 min; the job ran 30-37 min, so about 60 min |
| Nightly, other boards (FreeRTOS, NuttX, ThreadX chapter boards and the FreeRTOS floor image) | about 7 images; **unmeasured**, likely 2-4 min each | — | to be measured on the first run |
| Merge queue (`template-copy-out [n/3]`, 50 min) | the 18 host role × language images | **not priced**: the merge-queue copy-out shards were skipped by path on the runs read | — |

**Recommendation.** All three parts go to the nightly (schedule plus
dispatch) first, as the manager expected. Part 3 moves to the merge queue
only after one nightly measures it under the shards' 50 min cap. The merge
queue keeps today's scaffolds until then.

**Acceptance stays as written:** the nightly fails on
`action-server/rust/zenoh@nucleo-f767zi-zephyr` until the fix for issue 2772
(the action-server RAM overflow, nros 139's #4843) lands. Both must land
together, or the nightly is red by construction until then. So ship the
check with that image in the refusal list, naming issue 2772 and saying it
waits on #4843, and delete the entry when #4843 lands; the refusal list's own
gate fails once the image builds.

**Approved 2026-11-27 (manager):** nightly first; part 3 moves to the merge
queue only after a nightly measures it under the shards' 50 min cap.
