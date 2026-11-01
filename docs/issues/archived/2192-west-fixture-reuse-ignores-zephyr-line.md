---
id: 2192
title: "A west fixture built on one Zephyr line is reused, and passes, as the other line's; a `zephyr/` module edit stales none of them"
status: resolved
resolved_in: "2026-11-01"
type: bug
area: [zephyr, testing, build]
severity: medium
found: 2026-11-01
related: [2189, 0509, 0196]
---

## What was measured

Found while fixing issue 2189, on the two `zephyr_self_pkg` rows.

1. `NROS_ZEPHYR_VERSION=3.7 … west-fixtures.sh` built both rows, then
   `NROS_ZEPHYR_VERSION=4.4 … west-fixtures.sh` printed
   `west fixtures: 2/2 ok (2 reused, 0 built)`. The 4.4 sibling row had
   failed Kconfig on 4.4 one run earlier, and no 4.4 build ran in between.
2. An edit to `zephyr/Kconfig` (the nano-ros module, which every west
   fixture's configure reads) was followed by `2 reused` on both lines.

Dropping `build/compile-check-fixtures/<id>/.inputsig` forced a real build,
which printed `Zephyr version: 4.4.0` and `0 reused, 2 built`.

## Cause

- `out_root` is `nros_build_dir "$NROS_KIND_WEST_FIXTURES"`, and the row's
  build dir is `$out_root/<id>`. Neither has the Zephyr line in it, so the
  3.7 and 4.4 builds of a row are one directory.
- The reuse test (phase-353 W2, issue 0509) compares
  `compile-check-signature.sh <record>`, which hashes the manifest record,
  the row's own source dir, the CLI's codegen fingerprint, the launch
  resolver's identity and a dependency closure read from the compile-check
  dir. The Zephyr line, the Zephyr workspace and `zephyr/` (module Kconfig,
  cmake, snippets) are none of these.
- The test side reads the same signature and `.compile-ok`, whose stamp
  records the CLI hash and the builder, not the line. So a 4.4 test run
  reads a 3.7 bake as a fresh 4.4 fixture (issue 0196's shape: build side
  and test side agree, and both watch less than the build reads).

The `zephyr-workspace-builds/<ver>/<row>` leaves do not have this problem:
their root is keyed by version.

## Resolution

- `scripts/build/build-root.sh` gains the one derivation:
  `nros_west_fixture_dir <id>` is `west-fixtures/<line>/<id>`, and
  `nros_compile_check_stamp_dir <builder> <id>` puts a west row's `.inputsig`
  under `compile-check-fixtures/zephyr-<line>/<id>` (other builders are
  unchanged). The line is `nros_zephyr_ws_version`'s, never a second spelling
  of its default. The builder, `compile-check-signature.sh` and the staleness
  probe `compile-check-stale.sh` all ask it, so the two lines' fixtures are
  both current at once instead of thrashing one directory.
- `compile-check-signature.sh` hashes, for a west row, the line plus the
  `zephyr/` and `cmake/` trees the configure reads.
- `require_west_fixture` reads `build_dir(WEST_FIXTURES, [line, id])` through
  the new `nros_tests::zephyr::zephyr_line`, which `zephyr_build_root` now uses
  too. `build_root_derivation.sh` pins both shell paths per line and the Rust
  twin's spelling.

Not covered: the provisioned Zephyr workspace's own patches
(`scripts/zephyr/patches/<line>.sh`) are still not in the signature; the
line is, and a workspace re-provision with new patches needs a rebuild by
hand.

## Measured

- An edit to `zephyr/cmake/nros_system_generate.cmake` rebuilt the four
  affected west fixtures on both lines (`0 reused, 4 built`), where before it
  reused them.
- Two consecutive 3.7 runs: the second reused all four.
- After a 3.7 build, a 4.4 run built into `west-fixtures/4.4/` and left
  `west-fixtures/3.7/` in place.
- With both lines' fixtures on disk and no rebuild between,
  `nros-tests::{zephyr_self_pkg,cli_bringup_zephyr}` passed 3/3 with
  `NROS_ZEPHYR_VERSION=3.7` and 3/3 with `4.4`.
