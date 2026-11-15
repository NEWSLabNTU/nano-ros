---
id: 2607
title: "A Zephyr workspace missing a module that west.yml lists fails deep in configure, as a devicetree 'No such file', instead of being refused up front"
status: open
type: diagnosability
severity: low
area: [zephyr, build, provisioning]
related: [phase-524, 2249, 2242, 1275, 1282]
found: 2026-11-15
---

**Home: [phase-524](../roadmap/phase-524-emulator-twins.md)**, whose W2 put
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
