---
id: 2808
title: "The FVP install hint links Arm's Ecosystem FVPs page, which does not list the AEMv8-R model"
status: resolved
type: bug
severity: low
area: [zephyr, boards, cli, docs]
related: [issue-2802]
found: 2026-11-21
resolved_in: "fvp evidence renewal, 2026-11-21"
---

## What

Issue 2802 made the Arm FVP user-provided and pointed every install hint
(`nros doctor`, `nros setup`, `resolve-fvp-bin.sh`, the FVP lanes' unmet text,
`book-internals/src/platform/arm-fvp.md`) at
`https://developer.arm.com/downloads/-/arm-ecosystem-fvps`. That page lists
the Ecosystem FVPs (Corstone and the reference designs). The AEMv8-R model is
an architecture model, listed on Arm's Architecture FVPs page; its release
archives sit under `.../cdn-downloads/permalink/FVPs-Architecture/`.

Found by following the internals page as a user while renewing the
`fvp-aemv8r-smp` row's evidence. The same walk found three more places the
page and the tools disagreed:

1. `nros setup board fvp-aemv8r-smp` requires `--zephyr-workspace <dir>`
   (clap refuses the bare form, rc 2), and it patches that Zephyr tree; the
   page showed the bare form as a report-only command.
2. `nros setup fvp-aemv8r-smp` plans the board's default RMW, zenoh; the
   page's image is Cyclone DDS, which needs `--rmw cyclonedds`.
3. Step 5's `FVP_BaseR_AEMv8R --version` only works with the model on `PATH`;
   `ARMFVP_BIN_PATH`, the preferred setting, does not put it there.
4. The libatomic1 note said `nros setup --system` names it; it lists it only
   while it is missing, and `nros doctor`'s FVP line does not probe it.

## Resolution

Every hint links the Architecture FVPs page. The internals page gives the
direct FM 11.31 archive link, the `--zephyr-workspace` form, the
`--rmw cyclonedds` note, a `--version` check through `ARMFVP_BIN_PATH`, and a
hand check for `libatomic.so.1`.

`scripts/mdbook-board-status.py --self-test` pinned the FVP row's
`last_run` (2026-09-06) as literals, so renewing the row broke
`check-board-registry`; it now reads the dates from the row.
