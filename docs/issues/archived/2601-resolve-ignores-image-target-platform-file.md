---
id: 2601
title: "`nros configure` resolves every model for `posix`, so an image's `<stem>.system.<target>.yaml` is never read"
status: resolved
type: bug
area: [cli, orchestration, launch]
severity: high
found: 2026-11-15
resolved: 2026-11-15
resolved_in: "the change that filed it"
related: [phase-478]
---

**Home: [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md), I14.**
Reported from the Autoware Safety Island (`safety_island_bringup`), reproduced
on main.

## What happens

`nros configure` (and `nros sync`) runs `nros-launch-resolve` once per
resolved model and never passes `--target`, so the resolver takes its
default, `posix`. The resolver reads the platform file
`<stem>.system.<target>.yaml` beside the launch file for that target only, so
an image whose board is Zephyr resolves as if it ran on posix, and a
`<stem>.system.zephyr.yaml` is never read.

This has a deadline. rlm v0.3.22 moved the supervision terms
(`supervision: { tick, switch }`) from the contract to the target's platform
file. The island's `[image.zephyr|qemu|native_sim]` all set
`board = "zephyr"`, yet resolve as posix, so the terms are not found and
`error[supervised-fit]` fires. The island works around it by keeping the
deprecated contract spelling, which rlm refuses at its next release.

## Resolution (2026-11-15)

`cmd/ws.rs` `resolve_system_models` now passes `--target` for each model: the
scheduling target of the images that resolve to it (`model_sched_targets`).
An image's target follows the rule a `[tasks.<name>.<rtos>]` key already
uses, the board's platform (`PlatformKind::sched_task_rtos_key`, through the
board catalog; the entry board keys when no catalog loads).
`[image_defaults] board` and `[[model]]` variants are followed, so a variant
takes the target of the image that binds it.

When images on two targets share one model, the model cannot be resolved for
both. It is resolved for the default and the build says so, naming the
images per target and asking for a `[[model]]` variant per target.

Measured on a scratch workspace: with a `system.system.zephyr.yaml` beside the
launch file, the Zephyr image's model now records it in `meta.inputs`, and
the default (linux) image's model does not.

Tests: `model_sched_target_tests` (a model takes its images' target; a
default board and a variant are followed; two targets on one model are
reported).
