---
id: 2229
title: "Zephyr board facts asked NROS_IMAGE of the application dir, not NROS_IMAGE_SYSTEM_DIR, so every west row with an external bringup stopped at configure"
status: resolved
type: bug
area: [build, zephyr, ci]
severity: high
found: 2026-11-03
resolved: 2026-11-03
related: [2199, 2211, 0941, 2191, phase-466]
---

## Measured

run-matrix 41589064854 (`main` at 55a467ea51, 2026-11-03): tier 1 and tier 2
both named themselves `NO VERDICT: stopped in the build`. In `just build`,
3 of the 5 west compile-check rows failed at configure, each with:

```
CMake Error at cmake/NanoRosBoardFacts.cmake:209 (message):
  nano-ros: board facts for .../zephyr_self_pkg/sibling/caller
  --deploy zephyr could not be resolved (`nros dev image board-facts`):
  Error: reading .../zephyr_self_pkg/sibling/caller/Cargo.toml:
  No such file or directory (os error 2)
```

The failing rows are exactly the three whose `west_extra` carries
`-DNROS_IMAGE_SYSTEM_DIR=<bringup> -DNROS_IMAGE=<id>`:
`west_bringup_zephyr`, `west_bringup_zephyr_cyclone_user_config`,
`zephyr_self_pkg_sibling`. A generated west application
(`builder/west_app.rs`) states the same pair, so `nros build` of a Zephyr
workspace image takes the same road.

## Cause

Issue 2199 (`b46ba47d6d`, 2026-11-01) began forwarding `NROS_IMAGE` as
`--deploy`. `nros_resolve_board_facts` still resolved FROM
`APPLICATION_SOURCE_DIR`, which in these rows holds no `system.toml` and no
`Cargo.toml` — the image is declared in the dir `NROS_IMAGE_SYSTEM_DIR` names.
Before 2199 no deploy was named, so the verb answered "nothing to carry" and
the configure went on without board facts; with a deploy named it is no
longer a "nothing to carry" case, falls to the cargo-leaf arm, and reads a
manifest nobody wrote.

## Fix

- `cmake/NanoRosBoardFacts.cmake`: `NROS_IMAGE_SYSTEM_DIR` (absolute against
  `APPLICATION_SOURCE_DIR`, as `nros_image_kconfig.cmake` resolves it) is the
  first candidate to resolve from — it and `NROS_IMAGE` are one statement.
- `nros dev image board-facts`: a `--deploy` asked of a dir with neither a
  `system.toml` nor a `Cargo.toml` is refused naming the image and where to
  look, instead of as a failed read.

Verified: `NROS_ZEPHYR_FIXTURE_FILTER='west_bringup_zephyr|zephyr_self_pkg_sibling'
bash scripts/build/west-fixtures.sh` configures and builds all three rows
(board facts from the bringup dir, 3 values each).
