---
id: 2461
title: "The FreeRTOS stack-floor CHECK ignores [image.<id>] stack_floor_image, so a renamed FreeRTOS project would miss its template's measured row"
status: resolved
type: limitation
severity: low
area: [memory, freertos, build]
found: 2026-11-12
resolved: 2026-11-13
resolved_in: "nros build passes -DNROS_IMAGE_STACK_FLOOR_IMAGE on every CMake configure and nros_stack_floor_check keys by it (phase-557 W2b)"
related: [issue-2400, phase-557]
---

> **Resolved (2026-11-13), phase-557 W2b.** `nros build` passes the image's
> `stack_floor_image` stamp (or an empty value) to every CMake configure as
> `-DNROS_IMAGE_STACK_FLOOR_IMAGE`. `nros_stack_floor_check` keys the FreeRTOS
> check by it, as the Zephyr carrier does. An empty value falls back to
> `<bringup or entry>@<board>`. Passing it on every configure keeps a stale
> stamp from outliving a `system.toml` edit in the cache.

## What

Issue 2400 key (c) (user ruling 2026-11-12) has `nros new` stamp
`[image.<id>] stack_floor_image = "<template>/<lang>/<rmw>@<board>"`. That way a renamed
project still finds the allowance row its template was measured under. The Zephyr carrier (W4)
reads the stamp: `nros dev image facts --cmake` emits `NROS_IMAGE_STACK_FLOOR_IMAGE`, and
`_nros_stack_floor_image_key` prefers it.

The FreeRTOS carrier (W2, `nros_stack_floor_check` in `cmake/NanoRosStackFloor.cmake`) does not.
Its configure runs no image-facts query, so it always keys by `<bringup or entry>@<board>`.

## Why it is not a bug today

No `config/stack-floors.toml` row names a FreeRTOS board. Every FreeRTOS image is therefore
unmeasured whatever its key, and an unmeasured image only warns.

## When it becomes one

As soon as a FreeRTOS `<template>/…@<board>` row is added, a `nros new` FreeRTOS project will
key by its bringup name, miss the row, and be treated as UNMEASURED. That means warn-only where
the row says it must be enforced.

## Fix

Have the FreeRTOS configure read the image facts the way the Zephyr one does (or pass the stamp
through `nros build`). Then call `_nros_stack_floor_image_key` with the stated key. Do this
before, or in the same change as, the first FreeRTOS row.
