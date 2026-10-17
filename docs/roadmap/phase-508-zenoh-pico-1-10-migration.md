# Phase 508 — move the zenoh-pico fork onto 1.10

**Status (2026-10-17). Opened to give issue 0910 a home; not started.** The
issue has had no home since
[phase-355](archived/phase-355-dependency-and-fork-debt.md) was archived on
2026-10-12.

## Why this phase exists

[Issue 0910](../issues/0910-zenoh-pico-1-10-migration.md) has the measurements.
Our fork's pin is far behind upstream, and upstream has since restructured the
link transports. It now ships its own Zephyr UART backend and serial protocol,
generates `config.h` from `config.h.in`, and has knobs our config generator
does not emit. Most of our Zephyr serial code now has an upstream equivalent,
so the migration is mostly deletion. The system ABI our shims implement
(`_z_mutex_*`, `_z_task_*`) is unchanged.

The wire protocol is stable across 1.x (issue 0291), so the move does not change
interop with the `rmw_zenohd` a ROS install ships.

## Work items

- [ ] Rebase the fork's `nano-ros` patch line onto upstream 1.10, keeping the
      patches as individual commits (CLAUDE.md, vendored-fork workflow). Drop
      each patch that upstream now carries, and record each drop.
- [ ] Generate `config.h` the way upstream does, and close the gap between the
      knobs upstream defines and the ones our generator emits.
- [ ] Move Zephyr serial onto upstream's `uart_zephyr.c` and serial protocol,
      then delete ours.
- [ ] Bump the superproject pin forward only, after the fork branch is pushed.

## Acceptance

Issue 0910 is resolved. Tier 2's zenoh cells and the Zephyr serial cells pass
on the new pin, and an image interoperates with the host's `rmw_zenohd`.

## Issues homed here (2026-10-17)

| issue | why it belongs here |
| --- | --- |
| [#0910](../issues/0910-zenoh-pico-1-10-migration.md) | The migration itself. Phase-355 was archived without it, and nothing else fits. |
