---
id: 1664
title: "S32DS still carries `--allow-multiple-definition`: the only remaining use, and it cannot be measured on a host with no S32DS project"
status: open
type: tech-debt
area: build, s32ds
severity: low
found: 2026-10-03
related: [1645, 1636, 1618]
---

## What

`integrations/s32ds/makefile.defs` adds `-Wl,--allow-multiple-definition` to
every S32DS link. It is the last row in `scripts/allow-multiple-def-allowlist.txt`.
Issue 1645 removed the Zephyr row, which was measured and fixed.

The S32DS use is UNVERIFIED in both directions. The generated `nros-libs.mk`
links `corrosion/*.a`, which can hold `libnros_c.a` beside `libnros_cpp.a`.
On NuttX that pairing measured 392 duplicates, 297 of them `nros_*` plus
`REGISTRY`, and it was fixed by linking `libnros_cpp.a` alone (issue 1636).
The same fix probably applies here.

## Why it was not done in 1645

S32DS 3.6.10 is installed at `~/NXP/S32DS.3.6.10`, but no S32DS project
exists on the host. A project needs a `.cproject` plus RTD, FreeRTOS and
lwIP, made by the IDE's project wizard. The CDT-generated makefiles own the
link line, so nothing short of a real project shows which archives reach the
link and what the flag masks. Changing `nros-libs.mk` without that link would
be the unmeasured edit the allowlist exists to stop.

## What closing needs

- Create an S32DS project (the README's flow) on a host that has the RTD
  packages.
- Link it with the flag removed and read the duplicate list.
- Emit one runtime archive into `nros-libs.mk` (the NuttX fix), relink
  without the flag, and drop the allowlist row.
