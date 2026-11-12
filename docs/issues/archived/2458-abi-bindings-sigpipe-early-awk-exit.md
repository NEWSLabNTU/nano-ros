---
id: 2458
title: "`check abi-bindings` can exit 141: an awk that exits early SIGPIPEs bash's line-by-line printf under pipefail"
status: resolved
type: bug
area: [ci, scripts]
severity: low
found: 2026-11-12
related: [phase-535]
---

**Home: [phase-535](../../roadmap/phase-535-example-reorg-and-cadence-ci.md)** (found while gating the W6 bare-metal C/C++ slice, PR #3579).

## What fails

`just ci gate` stopped at `check::fast` with one red:

```text
FAIL nros-gates::gates fast::check::abi-bindings
error: recipe `abi-bindings` failed on line 24 with exit code 141
```

The same gate had passed a minute earlier in a standalone `just check fast` on
the same tree. 141 is SIGPIPE.

## Why

`scripts/gen-abi-bindings.sh` runs under `set -euo pipefail` and read the
bindgen version with:

```sh
have="$(printf '%s\n' "$bg_version" | awk '/^bindgen /{print $2; exit}')"
```

`bg_version` is two lines (`bindgen 0.72.1`, `Clang: … clang version 14…`).
Bash's builtin `printf` writes each line with its own `write(2)` (measured with
`strace -e write`: `"a\n"` then `"b\n"`). When awk exits after line 1 before the
second write, that write gets SIGPIPE. `pipefail` turns this into the
pipeline's status, and `set -e` then stops the script at the assignment. It is
a scheduling race, so it shows up under load (the 32-way gate) and not when run
alone.

## Fix

The awk no longer exits early (`/^bindgen / && !v {v = $2} END {print v}`), so
it drains its input. Sweep for the same shape (an early-`exit` awk fed by a
multi-line `printf '%s\n'`) under `scripts/`, which found this one site only:

```sh
git grep -nE "printf '%s\\\\n' \"\\\$[a-z_]+\" \| awk '[^']*exit" -- scripts
```
