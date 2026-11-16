---
id: 2642
title: "A FreeRTOS workspace fixture was looked up at the ambient profile, so a built binary read as MISSING"
status: resolved
type: bug
area: [testing, fixtures, freertos]
severity: medium
found: 2026-11-16
resolved: 2026-11-16
resolved_in: "fix(nros-tests): a workspace fixture is resolved at its platform's profile, at the chokepoint"
related: [0608, phase-489]
---

## Measured

Nightly 43808375964 (main ffff1e331f), job FreertosMps2. One of the three
`realtime_sched_tasks` rows, freertos/rust:

    freertos/rust: Test fixture binary MISSING for an in-lane coordinate:
    examples/workspaces/realtime-rust/target-fixtures/freertos/thumbv7m-none-eabi/nros-relwithdebinfo/freertos_realtime_entry

The fixture had built. The same job's build step for that row
(`workspace-rust-freertos-realtime`, 155 s) ended with:

    Finished `nros-minsizerel` profile [optimized] target(s) in 2m 20s
    artifact: .../target-fixtures/freertos/thumbv7m-none-eabi/nros-minsizerel/freertos_realtime_entry

So the binary sat one directory over from where the test looked. It was not a
wrong name and not a quiet build failure.

## Cause

`nros build` writes a FreeRTOS image at the platform's profile carve-out,
`nros_cargo_profile::platform_profile("freertos")` = `nros-minsizerel` (the
QEMU timing floor). Every WORKSPACE resolver in `nros-tests` built its path
from `cargo_target_profile_dir()`, the AMBIENT profile (`nros-relwithdebinfo`
in that lane), and handed it to `require_prebuilt_workspace_binary` unchanged.

This is issue 0608's class one funnel over. 0608 moved the carve-out into
`require_prebuilt_row_binary_in`, the chokepoint for group rows, with the note
that fixing it at call sites is the #328 shape. The workspace chokepoint was
never given it. The two FreeRTOS workspace resolvers in `binaries/mod.rs`
(`build_freertos_workspace_rust_realtime_entry` and
`freertos_bridge_pair_image`) both asked for the ambient directory.

## Fix

`require_prebuilt_workspace_binary` looks the row up by id
(`lane::attribute_workspace_id`) and rewrites the path's profile component to
the row's platform profile (`path_at_platform_profile`, the platform-keyed core
of `rel_at_row_profile`). Only the component that is the ambient profile moves,
and a platform with no carve-out is untouched.

Every workspace resolver gets it at once, and no call site names a profile.

Test: `a_workspace_row_is_resolved_at_its_platforms_profile`, on the real row
that failed. It also checks that the chokepoint calls the rewrite: with that
call removed, the test fails (measured).
