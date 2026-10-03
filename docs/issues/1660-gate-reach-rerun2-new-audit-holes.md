---
id: 1660
title: "Gate re-run 2026-10-03: the four holes the first re-run's NEW audits found were never assigned to a class issue, and still stand"
status: open
type: tech-debt
area: testing, build
severity: low
found: 2026-10-03
related: [phase-472, 1614, 1615, 1616, 1617, 1618, 1636]
---

## What

The 2026-10-01 re-run ([findings](../development/audit-findings-2026-10-01-rerun.md),
"New audits") recorded four holes in gates the 2026-09-28 audit had only
triaged. Issues 1614–1617 took the RE-RUN holes by class. These four were
listed in the doc but filed under no issue, so no PR reached them. The
2026-10-03 final re-run (every recorded mutation, on `main` after #1559,
#1563, #1583, #1595, #1607 and the 1636 PR) still finds each one passing,
while its positive control fails.

| class | gate · facet | mutation | rc | control | control rc |
| --- | --- | --- | ---: | --- | ---: |
| W5 | `check-board-vocabulary.py` · system-toml-board | `examples/rv-virt-threadx/rust/talker/system.toml`: board = "rerun-nonexistent-board" | 0 | the same edit in the `c/talker` leaf | 1 |
| W7 | `check-build-type-spelling.py` · new-board-ament_cargo | new `packages/boards/nros-board-rerun/package.xml` with `<build_type>ament_cargo</build_type>` | 0 | the same file declaring `<nano_ros_provides kind="board">` | 1 |
| W6 | `check-cmake-generated-source-owners.py` · two-targets | a raw `add_custom_command(OUTPUT x.c)` consumed by two `add_library` targets (`zephyr/cmake/` or `cmake/`) | 0 | the same shape through `nros_rmw_cyclonedds_idlc_compile` | 1 |
| W6 | `check-xrce-config-manifest.py` · hand-value | `set(UXR_CONFIG_SERIAL_TRANSPORT_MTU 512)` in `nros-rmw-xrce/CMakeLists.txt` | 0 | `set(UCLIENT_SERIAL_TRANSPORT_MTU 512)` | 1 |

The classes are read off the controls:
- **board-vocabulary**: reads the C leaf's `system.toml`, not the Rust one's.
  The population stops short.
- **build-type-spelling**: judges a board package only when it declares the
  provider marker. The population is the authored marker, not the kind.
- **cmake-generated-source-owners**: sees the helper's spelling of a generated
  source, not a raw `add_custom_command(OUTPUT …)`.
- **xrce-config-manifest**: knows the `UCLIENT_*` spelling and not
  `UXR_CONFIG_*`.

Two re-run rows are CONTROLS that cannot fail, and they are not holes:
- `default-gates-run-somewhere/removed`: every gate has more than one
  placement, as the 2026-09-28 row recorded;
- `cmake-generated-source-owners/cmake-dir`: the control for the hole above.

## What closing needs

Move each gate onto its class helper (`file_kinds` / `harvest` / `per_item`),
then re-run the four mutations. Phase-472's acceptance is unmet until this
table is empty.
