---
id: 2461
title: "The FreeRTOS stack-floor CHECK ignores [image.<id>] stack_floor_image, so a renamed FreeRTOS project would miss its template's measured row"
status: open
type: limitation
severity: low
area: [memory, freertos, build]
found: 2026-11-12
related: [issue-2400]
---

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
