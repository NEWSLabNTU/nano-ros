---
id: 2192
title: "A west fixture built on one Zephyr line is reused, and passes, as the other line's; a `zephyr/` module edit stales none of them"
status: open
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

## Not done

Not fixed in issue 2189's change. A fix keys the west-fixture root by Zephyr
line (as the leaf builds already are) and adds the line plus the `zephyr/`
module tree to the signature, then checks that the test-side locator moves
in the same commit (#393).
