---
id: 2802
title: "The Arm FVP is user-provided: nano-ros stops provisioning and distributing it"
status: resolved
type: chore
area: [zephyr, boards, cli, tooling]
severity: low
found: 2026-11-21
related: [1786, 0232]
resolved: 2026-11-22
resolved_in: "chore(fvp): the Arm FVP is user-provided (issue 2802)"
---

## Decision

nano-ros no longer provisions or distributes the Arm FVP
(`FVP_BaseR_AEMv8R`, the AEMv8-R Fast Model the `fvp-aemv8r-smp` board runs
on). Users install it from Arm and point nano-ros at it with
`ARMFVP_BIN_PATH` (or `ARM_FVP_DIR`, or `PATH`).

Since 2026-09-06 the index carried a `[tool.arm-fvp]` row with Arm's download
URL and digest, `[board.fvp-aemv8r-smp]` listed it as a package, and
`nros setup board fvp-aemv8r-smp` installed it as a board tool. That made
nano-ros a redistributor of the model, which it should not be.

## What changes

1. Index: `[tool.arm-fvp]` is removed, and `[board.fvp-aemv8r-smp]` lists no
   package. `[prereq.libatomic1]` stays: it is the model's own runtime
   dependency, and `nros setup --system` names it.
2. `scripts/zephyr/resolve-fvp-bin.sh` keeps its lookup order
   (`ARMFVP_BIN_PATH`, `ARM_FVP_DIR`, `PATH`) and drops the SDK store
   fallback. `scripts/installers/arm-fvp-installer.sh` is deleted: it only
   symlinked an existing install into a store landing nothing reads any more.
3. Every "`nros setup --tool arm-fvp`" hint (just recipes, the nextest unmet
   text, the board registry, the book) says instead: install the AEMv8-R FVP
   from Arm, then set `ARMFVP_BIN_PATH` (or `ARM_FVP_DIR`); the model needs
   `libatomic1`.
4. `nros setup board fvp-aemv8r-smp` (and `nros setup fvp-aemv8r-smp`) says
   the FVP is user-provided and reports whether it is found and through which
   setting. It never downloads and never fails over the model: a downstream
   bootstrap runs this command on hosts with and without it.
5. `nros doctor` reports whether the FVP is found and through which setting
   (`ARMFVP_BIN_PATH`, `ARM_FVP_DIR` or `PATH`); a miss warns and is not
   counted. With `--board`, only for a board whose `[board.run] emulator` is an
   FVP.
6. The internals book's `platform/arm-fvp.md` gives the user-install steps
   and the configure-cache trap: a build dir configured before the model was
   found caches `ARMFVP-NOTFOUND` and must be re-configured.

Unchanged: the board-support row stays "Boots, manual", and the FVP lanes keep
skipping (named: failing) with the new hint when the model is not found.

## Downstream

The safety island's CI downloads the model from Arm itself and exports
`ARMFVP_BIN_PATH`; it never used the index row. Its local bootstrap runs
`nros setup board fvp-aemv8r-smp`, which keeps exiting 0.

## Coverage

- `nros-cli-core` `cmd::arm_fvp` tests: the status names each setting that
  found the model, and a miss warns with the user-install hint.
- `nros-cli` `tests/fvp_user_provided.rs`: `nros setup board fvp-aemv8r-smp
  --dry-run` exits 0 without the model, and names `ARMFVP_BIN_PATH` with it.
