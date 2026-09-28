---
id: 1563
title: "activate.sh puts EVERY store version's bin dir on PATH, in readdir order, so the PATH rung can still pick an unpinned store copy"
status: open
type: bug
area: [build, sdk]
severity: low
found: 2026-09-29
related: [1546, 0500, 0625, 1117]
---

## What happens

Issue 1546 made every store CONSUMER construct `<store>/<tool>/<pin>` from
`nros-sdk-index.toml` instead of listing the store newest-first. The store
rung of the cross-toolchain resolver (`NanoRosCrossToolchain.cmake`,
`scripts/build/riscv64-toolchain.sh`, `nros_build_paths::riscv64`) no longer
uses a version the tree does not pin.

But the rung BELOW it is `PATH`, and `activate.sh` / `activate.fish` build
that PATH from the store too: `_nros_bin_dirs "$_nros_sdk" 3` is a `find
-mindepth 3 -maxdepth 3 -type d -name bin`, and every hit that holds a `*-gcc`
or a `scripts/sdk-path-tools.txt` tool is PREPENDED. So every provisioned
version of a tool is on PATH, and which one `command -v` finds is decided by
the filesystem's readdir order — not by the pin, and not even by version.

MEASURED on this host, 2026-09-29 (`~/.nros/sdk/arm-none-eabi-gcc` holds
`13.2-nros1` and `13.2-nros4`; the index pins `13.2-nros5`):

```
$ source ./activate.sh; command -v arm-none-eabi-gcc
/home/aeon/.nros/sdk/arm-none-eabi-gcc/13.2-nros4/bin/arm-none-eabi-gcc
```

After 1546 a configure here correctly reports that the pin is absent and names
`13.2-nros1, 13.2-nros4` as present-but-unused — then its PATH rung resolves a
bare `arm-none-eabi-gcc`, which is one of exactly those copies. The line says
`via system PATH` and the notice says the store copies are not used, both
true of the resolver and misleading about the result.

## What is NOT established

- Whether any build outcome differs: `13.2-nros4` and `13.2-nros5` both report
  GCC 13.2.1. Not measured.
- Whether `activate.fish` enumerates identically (it reads the same
  `scripts/sdk-path-tools.txt`; its directory walk was not read for this).

## Fix (proposed, not ruled)

Put only the PINNED version's bin dir on PATH per tool —
`nros_sdk_pinned_version` (`scripts/lib/sdk-pin.sh`) for the version, which
needs no CLI and so works in `activate.sh` — and keep the legacy flat
`<tool>/bin` as the one other constructed candidate, as
`sdk_store::tool_dir_candidates` does. `check-sdk-store-not-enumerated` does
not flag this site because nothing is SORTED; the defect is that a PATH is an
ordered list built from an unordered enumeration, which the gate's windowed
shape does not model.
