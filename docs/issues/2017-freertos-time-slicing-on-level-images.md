---
id: 2017
title: "FreeRTOS level images keep configUSE_TIME_SLICING at the kernel default"
status: open
type: enhancement
area: [freertos, executor]
severity: low
found: 2026-10-24
related: [phase-509, rfc-0108, 2018]
---

## Measured

The phase-509 P10 FreeRTOS item asks for derived stacks and for
`configUSE_TIME_SLICING = 0` when the image has levels. The derived stacks
landed (`NROS_LEVEL_STACK_BYTES`, `tier_task_memory_freertos.h`).

Time slicing did not. No `FreeRTOSConfig.h` in the tree sets
`configUSE_TIME_SLICING`, so the kernel default of 1 applies. The setting is
kernel-wide and is compiled into the kernel library, not into the generated
entry. No build input currently tells the kernel build that an image has
levels.

Levels hold distinct priorities, so slicing only matters between a level and
a non-nros task at the same priority, such as the lwIP or zenoh-pico read
tasks. Every worker parks each pass, so a level still yields. With slicing
on, though, a level can be sliced against an equal-priority task in the
middle of a callback, and RFC-0108 D6 asks the kernel not to do that.

## Closes when

A level image's FreeRTOS build sets `configUSE_TIME_SLICING 0`, or the board
config states why the default is kept. That needs a per-image input the
kernel build can read. The FreeRTOS realtime rows must stay green.
