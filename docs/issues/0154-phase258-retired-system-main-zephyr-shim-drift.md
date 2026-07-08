---
id: 154
title: "phase-258 retired the codegen-system system_main.c emit but the Zephyr shim path + its fixtures/tests still require it"
status: open
type: bug
area: zephyr
related: [issue-0152]
---

## Summary

`nros codegen-system` stopped emitting `system_main.c` in phase-258 (Track 2
follow-up — the CLI's own tests now ASSERT its absence:
`codegen_system.rs:1494/1565/1730`). Three consumers were never migrated and
have been silently broken against any post-258 CLI:

1. **`zephyr/cmake/nros_system_generate.cmake`** — hard-fails when
   `system_main.c` is missing ("verb may be unimplemented in this CLI
   build") and `target_sources(app PRIVATE system_main.c)` is the ONLY main
   the shim path attaches; the fixture app
   (`multi_pkg_workspace_zephyr/zephyr_app`) has no `src/` of its own.
2. **`scripts/build/west-fixtures.sh`** — bake-success check requires
   `nros-system/system_main.c` → all west/self-pkg fixtures stamp MISSING
   ("shim regressed?"), so `zephyr_self_pkg` + the west-bringup lane skip-fail.
3. **Tests** — `zephyr_self_pkg.rs` and `self_bringup.rs`
   (`nros_codegen_system_self_bringup_bakes_system_main`) assert the file.

Manual repro: `nros codegen-system --workspace <fixture> --bringup
demo_bringup --target zephyr-zenoh --out <dir>` bakes
`system_config.h/system_config.cmake/Cargo.toml/nros-plan.json` — no
`system_main.c`.

## Direction (needs a design decision, not a local patch)

Post-258 the entry main comes from `nros codegen entry` / `nano_ros_entry`
(run_tiers / sched-context emitters), not the retired C-baker. The Zephyr
shim path (`nros_system_generate`) predates the Entry-pkg architecture —
either migrate it to consume the entry-codegen output (attach the emitted
`*_nros_main_generated.c*` instead of `system_main.c`), or retire the shim
path + its fixtures in favor of the workspace-Entry shape and rewrite the
three tests accordingly. Cross-links the phase-258 track owner's intent —
don't guess it in a drive-by fix.

## Repro

```
cargo nextest run -p nros-tests --test zephyr_self_pkg --test self_bringup
bash scripts/build/west-fixtures.sh   # all bakes stamp MISSING
```
