---
id: 2607
title: "A Zephyr workspace missing a module that west.yml lists fails deep in configure, as a devicetree 'No such file', instead of being refused up front"
status: resolved
type: diagnosability
severity: low
area: [zephyr, build, provisioning]
related: [phase-524, 2249, 2242, 1275, 1282]
found: 2026-11-15
resolved: 2026-11-15
resolved_in: "phase-524 W2"
---

**Home: [phase-524](../../roadmap/phase-524-emulator-twins.md)**, whose W2 put
`hal_stm32` back in the allowlist and whose Nucleo-F767ZI twin is the first
board to need it. Found measuring issue 2375.

## What happens

`west.yml` lists `hal_stm32` again (phase-524 W2, for the Nucleo-F767ZI
twin). A workspace provisioned before that, such as the store workspace
`~/.nros/workspaces/zephyr/3.7` on the host where this was found, has no
`modules/hal/stm32`: `west update` adds nothing on its own, and
`just zephyr setup` is what makes an existing workspace follow the manifest
(issue 2249).

`NROS_ZEPHYR_FIXTURE_FILTER=build-nucleo-f767zi-c-talker-serial just zephyr
build-fixtures` against that workspace runs the full `nros build` front half,
then fails in Zephyr's devicetree step:

```
zephyr/boards/st/nucleo_f767zi/nucleo_f767zi.dts:9:10: fatal error: st/f7/stm32f767zitx-pinctrl.dtsi: No such file or directory
```

Nothing in that line names the missing module, the manifest, or the remedy,
and it reads like a broken board port. The same workspace builds every other
board, so its other preflights pass.

A second shape fails with the same line, and it is less visible. In the main
checkout's workspace, `modules/hal/stm32` is PRESENT, at the manifest's
revision (f1317150). The workspace's own manifest copy, though, still names
no `hal_stm32`: that is `zephyr-workspace/nano-ros/west.yml`, the
`[manifest] path` west reads. So `west list` omits the module, Zephyr's
module discovery never adds it, and the build fails exactly as above. A
check of "is the directory there" would pass this workspace.

The same tree builds the leaf once the workspace's manifest copy matches
the repo's `west.yml`, so that `west list` names `hal_stm32`.

## Expected

Before the first configure, compare the modules WEST REPORTS for the
workspace (`west list`, which reads the workspace's own manifest copy) with
the ones the checkout's manifest (`west.yml` / `west-4.4.yml`) lists. Both
shapes above fail that one test, and a directory check catches only the
first. Refuse a workspace
missing any, naming the module and `just zephyr setup`. The same check belongs
in the provisioning preflight (`check-tier-preconditions` / `just zephyr
doctor`), so the report comes before a build is started at all.

## Notes

- Additive staleness only: a module the workspace has and the manifest no
  longer lists is harmless (issue 1282: `west update` never prunes).
- The allowlist is derived from `[zephyr_module.*]` in `nros-sdk-index.toml`
  (phase-447 F1). A check that reads that table, not the YAML, has one source.

## Resolution

`scripts/zephyr/check-workspace-modules.py` asks the question the Expected
section names. It runs `west list -f "{name} {cloned}"` in the workspace, so
the answer comes from the workspace's own manifest copy. It compares that with
the modules `[zephyr_module.*]` in `nros-sdk-index.toml` lists for the
workspace's line. That is the same table `check-zephyr-module-allowlist`
derives the allowlist from, read through that gate's `LINE_TO_MANIFEST`, so
there is one source. A module is missing if west does not list it, or lists it
uncloned, so both shapes above fail the one test.

Whether a missing module is fatal depends on the build. The row's
`needed_by` names the boards that use it. `just zephyr build-fixtures` passes
the boards of the rows it is about to build (`--boards`). A missing module
whose `needed_by` meets that set refuses the build before any `west build`,
naming the module and `just zephyr setup`. One no board in the build needs is
a WARNING, and the build goes on, so a workspace without `hal_stm32` still
builds every native_sim and mps2 leaf. `just zephyr doctor` runs the check
with no board set and reports `[MISSING]` for any absent module. The
script's `--self-test` runs in `check-zephyr-module-allowlist`.

Measured on the store workspace this was found in (no `hal_stm32`, no
`hal_rpi_pico`): the Nucleo-F767ZI serial talker leaf now stops before
configure, naming `hal_stm32` and `just zephyr setup`, and a native_sim and
mps2 build prints the warning and exits 0.

Two limits are left. A module whose row names no board in `needed_by` can
only warn during a build, since nothing says which build needs it; `doctor`
still reports it. And the check is on the `just zephyr build-fixtures` and
`doctor` roads; a bare `nros build` of a Zephyr image still meets the
devicetree line.
