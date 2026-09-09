---
id: 1250
title: "Every FreeRTOS image fails to compile: `freertos_run_tiers.c` reads and
  writes `ctx->stack_bytes`, a field `nros_freertos_tier_ctx_t` never had"
status: resolved
type: bug
area: boards, freertos
severity: high
found: 2026-09-09
related: [1146, 1187, 1232]
resolved_in: "(this commit)"
---

## Problem

`packages/boards/nros-board-freertos/c/freertos_run_tiers.c` does not compile,
at `43fcbd964`, on the pinned `arm-none-eabi-gcc 13.2`:

```
freertos_run_tiers.c: In function 'freertos_tier_task':
freertos_run_tiers.c:359:79: error: 'nros_freertos_tier_ctx_t' has no member named 'stack_bytes'
  359 |     if (nros_cpp_executor_derive_min_stack_headroom(ctx->executor_storage, ctx->stack_bytes)
freertos_run_tiers.c: In function 'freertos_spawn_next_tier':
freertos_run_tiers.c:483:8: error: 'nros_freertos_tier_ctx_t' has no member named 'stack_bytes'
  483 |     ctx->stack_bytes = (size_t)stack_words * 4u;
```

`a6aa0f721` ("feat(cpp): derive a default stack-headroom bound, so the setter
has a caller") added BOTH accesses — the write at spawn and the read in the tier
task — and did not add the field to the per-tier context struct. The `nros_tier_spec_t`
mirror above it has a `stack_bytes`, which is the field the commit's own comment
distinguishes itself from ("The EFFECTIVE stack, not the declared one"), so the
name resolves in the reader's head and not in the compiler's.

This is board glue compiled into **every** FreeRTOS image, C and C++ alike, so
nothing under `examples/qemu-arm-freertos/` links.

## Why it stayed invisible

The same reason issue 1187 did, and the reason 1187 could not be settled
without tripping over this one: no merge-gating lane builds FreeRTOS.
`check-cpp` / `check-c` are HOST lanes; the cross build is reached only by
`build-all` / `workspace-fixtures-build.sh`, i.e. `schedule` and
`workflow_dispatch`. CLAUDE.md's "a red CI lane answers one of two questions"
applies exactly: the FreeRTOS nightly was already red for 1187, so a second
fault behind it was a first-error-only report away from invisible.

## Fix

Declare the field. It is written once at spawn (`stack_words * 4`, the
EFFECTIVE stack the task was created with — not the spec's declared
`stack_bytes`, which may be 0 and then gets the 256 KiB default) and read once
in the tier task to derive the headroom bound.

The class was checked and is one site: the Zephyr sibling
(`packages/boards/nros-board-zephyr/c/zephyr_run_tiers.c`) makes the same two
`nros_cpp_executor_derive_min_stack_headroom` calls and deliberately passes the
shim's slot size (`nros_zephyr_tier_stack_size()` / `nros_zephyr_main_stack_size()`)
rather than any `ctx->` field — see issue 1232 for why. No other board reaches
that entry point.

## Verification

`bash scripts/build/fixtures-build.sh freertos cpp zenoh` and
`… freertos c zenoh`, with `NROS_CMAKE_EXTRA_DEFS` carrying
`cmake/toolchain/arm-freertos-armcm3.cmake` — the same acceptance issue 1187
names, since this defect sits directly in front of it.

Both GREEN at `43fcbd964` + this fix, on
`~/.nros/sdk/arm-none-eabi-gcc/13.2-nros1` (`Arm GNU Toolchain 13.2.rel1`):
zero `error:` lines across the whole log, six `cpp_*` images and the C role
images linked. That build is also what settles issue 1187 — see its
Resolution section.
