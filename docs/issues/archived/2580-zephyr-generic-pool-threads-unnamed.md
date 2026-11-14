---
id: 2580
title: "Zephyr: threads from the generic pool were unnamed, so every scheduling capture showed `unknown`"
status: resolved
type: bug
area: [zephyr, platform]
severity: low
found: 2026-08-27
related: [0852]
resolved: 2026-08-31
resolved_in: "b5a43fff80 fix(zephyr): honour attr->name, so nano-ros threads are not anonymous"
---

GitHub: NEWSLabNTU/nano-ros#5

Filed from the GitHub issue on 2026-11-15, already fixed: the commit landed
four days after the report and no repo issue was opened for it.

## What was reported

On Zephyr the threads nano-ros spawns through its generic pool were unnamed,
so thread analyzer and CTF captures (`thread_create` / `thread_info` /
`thread_switched_in`) showed them as `unknown`; only stack base told them
apart. `nros_zephyr_task_create` in `zephyr/nros_platform_zephyr_shims.c`
took no name, while the tier path called `k_thread_name_set`. On an ASI
controller image with one declared tier (run on the calling thread) every
nano-ros thread was anonymous. Seen at pin 589a9d0, Zephyr v3.7.0,
`fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp`, `CONFIG_POSIX_API`.

## Resolution

`b5a43fff80` (2026-08-31): `nros_zephyr_task_create_prio` takes a `name` and
applies it with `pthread_setname_np` after a successful create, best effort
(it self-gates on `CONFIG_THREAD_NAME`), and the platform port passes
`attr->name` through (`packages/platform/nros-platform-zephyr/src/platform.c`).
Issue 0852's shape one field over: a scheduling attribute dropped one layer
down.
