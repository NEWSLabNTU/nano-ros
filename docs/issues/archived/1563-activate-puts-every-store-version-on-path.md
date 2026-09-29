---
id: 1563
title: "activate.sh puts EVERY store version's bin dir on PATH, in readdir order, so the PATH rung can still pick an unpinned store copy"
status: resolved
resolved: 2026-09-29
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

## Resolution (2026-09-29, phase-472 F3)

Both activation files CONSTRUCT one bin dir per tool from the pin, never
walk versions: `_nros_store_tool_bin <tool>` (activate.sh; its fish mirror
takes the checkout root, since fish functions cannot see file-scope locals)
returns `<store>/<tool>/<pin>/bin` from `nros_sdk_pinned_version`
(`scripts/lib/sdk-pin.sh`, the one shell pin reader — fish calls it through
bash), else the legacy unversioned `<tool>/bin` (as
`sdk_store::tool_dir_candidates` does), else nothing. The store root honours
`NROS_SDK_STORE` before `NROS_HOME/sdk`, as `NanoRosCrossToolchain.cmake`
does. The `play_launch_parser` fallback, which took "whichever version is
present", uses the same helper. What is enumerated now is the TOOL list (a
tool's name is not a version choice).

A pin that is not in the store is NOT replaced by another version: activation
prints one notice naming the tool, the pin and what IS present (the
NanoRosCrossToolchain shape), suppressed by `NROS_QUIET_ACTIVATE=1`.

**Measured on a scratch store** (`tmp/nh/sdk`, `NROS_HOME` pointed at it;
arm-none-eabi-gcc `13.2-nros1` + `13.2-nros4`, pin `13.2-nros5`; genromfs
`0.4.0-old` + the pin; sccache flat `bin/`), bash, zsh and fish:

| | before | after |
| --- | --- | --- |
| `arm-none-eabi-gcc` | store `13.2-nros4` | `/usr/bin` (the documented PATH rung) + notice naming 1/4 as present, 5 absent |
| `genromfs` | `0.4.0-old` | the pin `0.5.7-nros1` |
| `sccache` (flat layout) | flat | flat |

With `13.2-nros5` added, `NROS_SDK_STORE` pointing at the store: the pinned
dir wins in bash and fish.

**Gates.** `check-sdk-store-not-enumerated` gained shape 3, a VERSION-LEVEL
walk (`$store/*/*`, or a store root handed to a walker at version depth), and
now scans `*.fish`: new gate on the old activation files rc=1, old gate rc=0.
`check-activate-shells` builds its versioned fixture AT THE PINS and adds a
stale version beside them, which must not reach PATH in bash or fish (rc=1 on
the old files).

**Not verified:** a real build against the real store. On this host the
arm-none-eabi-gcc pin is absent from `~/.nros/sdk`, so after this change a
sourced shell takes `/usr/bin/arm-none-eabi-gcc` (or none) and says so — the
intended behaviour, but a behaviour change for anyone relying on the stale
copy.


**Caught before merge:** the stranded-pin notice appended to an accumulator
nothing had initialised, so `activate.sh` sourced under `set -u` (as a CI or
harness script does) aborted at that line on exactly this host — the pin-absent
path. Now `${_nros_store_stranded:-}`, and `check-activate-shells` sources a
stranded store in every shell plus bash `-u`, asserting the end of the file is
reached and the pin is NAMED (the unfixed line: rc=1, both rows fail).
