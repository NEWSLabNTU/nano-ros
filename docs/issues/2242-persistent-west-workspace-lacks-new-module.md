---
id: 2242
title: "`just zephyr setup` skips `west update` on a present workspace, so a module newly allowlisted never reaches a persistent runner"
status: open
type: bug
area: [zephyr, ci, setup]
severity: medium
found: 2026-11-04
related: [1279, 1275, 1282]
---

## Measured

The scheduled tier-2 nightly on main, run 41777336351 (2026-11-04 05:07 UTC),
failed job 126400635166 (`tier 2 nightly (pairwise cover)`, runner
`nano-ros-runner-newslab-118-server`) at step `just build tier2-nightly`:

    == zephyr == FAILED (rc=2)

Every attempt at the one new leaf, `build-nucleo-f767zi-c-talker-serial` (the
Nucleo-F767ZI row that phase-524 W4c added, #3079), stops at configure:

    /home/runner/.nros/workspaces/zephyr/3.7/zephyr/boards/st/nucleo_f767zi/nucleo_f767zi.dts:9:10: fatal error: st/f7/stm32f767zitx-pinctrl.dtsi: No such file or directory

That `.dtsi` ships in the `hal_stm32` module. Phase-524 W2 put `hal_stm32`
back on the 3.7 line (`[zephyr_module.hal_stm32] lines = ["3.7"]`, mirrored
into `west.yml`). The runner's persistent workspace was provisioned before
that, and the setup step of the same job never fetched the module:

    === just setup zephyr ===
    Zephyr workspace already present at /home/runner/.nros/workspaces/zephyr/3.7
    To reinstall, run: just zephyr setup --force

`just/zephyr-setup.just` treats a present `$WORKSPACE/zephyr` as done, so
`scripts/zephyr/setup.sh`, whose "workspace exists" branch would run
`west update`, is never reached. A workspace that existed before a module
joined the allowlist never gains it on its own.

## What it is not

- **Not the allowlist.** `west.yml` on main names `hal_stm32`, and the row
  builds where the workspace was fetched after W2 (the phase-524 W4c
  measurement).
- **Not the reverse direction.** `west update` never prunes, so a module
  leaving the allowlist is harmless here. Only additions are lost.
- **Not issue 1279.** 1279 made SDK registration run whatever the workspace
  state. This is the workspace's own content, which that fix deliberately left
  behind the presence check.

## What would close it

Two defensible sites:

1. **Always update a present workspace.** `west update` on a current
   workspace is incremental and fetches only what the manifest gained. The
   cost to price is a full project walk on every setup.
2. **Update when the manifest moved.** Keep the skip, but run `west update`
   when the manifest's project set is not a subset of the workspace's.
   `west list` against `west.yml`, or a stamp of the manifest's hash written
   after the last update, would answer that.

For this runner, `just zephyr setup --force` (or a bare `west update` in
`/home/runner/.nros/workspaces/zephyr/3.7`) unblocks tonight's lane. It does
not close the class.

Acceptance: a tier-2 nightly on a runner whose workspace predates a module
addition builds the module's consumer without `--force`.
