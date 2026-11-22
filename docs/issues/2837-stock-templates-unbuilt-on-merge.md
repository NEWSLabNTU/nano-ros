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
user's ruling, #4616) leaves the image about 33 KB over, so something else in
it has grown. The fix belongs to nros 175's batch beside issue 2772. This
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
link overflow, and passes once 175's fix lands.
