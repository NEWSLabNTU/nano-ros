---
id: 1546
title: "The SDK store is enumerated newest-version-first in two places — pending a ruling on whether issue 0500's ordering survives"
status: resolved
type: bug
area: [build, sdk]
severity: medium
found: 2026-09-28
resolved_in: "fix/1546-sdk-store-pin (2026-09-29)"
related: [phase-472, 0500, 0625, 1563]
---

## What happens

VERIFIED present on `main`:

- `cmake/toolchain/NanoRosCrossToolchain.cmake:181-182` —
  `file(GLOB _vers … "${_store}/*")` + `list(SORT … NATURAL ORDER DESCENDING)`,
  taking the first hit.
- `scripts/build/riscv64-toolchain.sh:43,70` — `for ver in $(ls -1 "$store" | sort -Vr)`.

`check-sdk-store-not-enumerated` states that the store is "constructed from the
pin and never enumerated", but matches only a literal `/sdk/<tool>/*`. The
cmake site is the shared helper that REPLACED the site the gate's docstring
cites, so the fix moved the defect out of the regex's reach. The gate's docstring
also promises a self-test "below" that does not exist.

## What is NOT established — a ruling is needed

`riscv64-toolchain.sh` cites *"the 0500 rule"* for its newest-first order, and
CLAUDE.md documents 0500 as newest-first enumeration. It also says that ordering
was RETIRED with the globbed prefix in phase-365 W3a. So these two sites are
either live violations citing a retired rule, or a sanctioned exception the gate
should know about. That is a design ruling, not a fact the audit can establish.

## Fix

Rule on it. If pinned: construct both sites from the pin. Either way the gate
matches any store-rooted glob or `ls` followed by a sort, and gains its self-test
(phase-472 W7/W9).

## Resolution

`status: resolved`. **Ruling (option A): the pin is constructed; the store is
never enumerated.** 0500's newest-first ordering does not survive — it was the
remedy for a STALE copy shadowing the pin, and in a store shared between
checkouts it produces the mirror image (a sibling checkout's NEWER copy
shadowing it). Every consumer now builds `<store>/<tool>/<pinned version>`
from `nros-sdk-index.toml`; another version in the store is never used, and
when only others are present the resolver NAMES them and the remedy
(`nros setup --tool <tool>`) before falling through to its documented next
rung (PATH for the cross toolchains).

**Seven sites, not two.** The widened gate found five more of the same class:

| Site | Now |
| --- | --- |
| `cmake/toolchain/NanoRosCrossToolchain.cmake` (store rung) | pinned only; unsorted listing recorded for a NOTICE in the report |
| `scripts/build/riscv64-toolchain.sh` (`nros_riscv64_prefix`/`_bindir`) | pinned only; stderr message when only others exist |
| `nros_build_paths::riscv64::store_bin` | pinned only; `cargo:warning` when only others exist |
| `packages/rmw/cyclonedds/.../NrosRmwCycloneddsTypeSupport.cmake` (idlc hints) | pinned + legacy flat `<store>/cyclonedds/bin` (two constructed candidates) |
| `nros-tests` `threadx_riscv64::riscv64_gcc` | pinned only |
| `nros-tests` `ros_env::host_xrce_agent_bin` | pinned only |
| `nros-tests` `zephyr::sdk_qemu_xilinx_aarch64` store arm | `[tool.zephyr-sdk]` then `[tool.zephyr-sdk-1-0-1]`, each constructed with its `subdir` |

**One pin reader per language**, since the build systems cannot call each
other: `nros_sdk_pin()` in the new `cmake/NanoRosSdkPin.cmake` (the former
`_nros_ct_pinned`, moved so the cyclonedds module shares it),
`nros_sdk_pinned_version` in the new `scripts/lib/sdk-pin.sh` (awk, so it
works before the CLI is built and on a python without `tomllib`),
`nros_build_paths::sdk_pinned_version()`, and `nros_tests::sdk_pin()`. The CLI
(`nros sdk-path`) was not reused for these because the cross-toolchain family's
store root is `NROS_SDK_STORE`, which the CLI does not read.

**The gate.** `check-sdk-store-not-enumerated` keeps its literal
`/sdk/<tool>/*` shape and adds a windowed one: an enumeration primitive (cmake
`file(GLOB`, shell `ls`/`find`/glob loop, Rust `read_dir(`) with a store
reference within 8 code lines above and a sort/pick within 8 below, over
`cmake/ scripts/ just/ zephyr/ packages/**/{cmake,*.cmake,*.rs,*.sh}`. The
phase-431 exemption (`sdk_store::installed_versions`) is keyed on the FUNCTION,
not the file. Its self-test (7 bad / 5 good, the bad ones being the shapes that
shipped) runs on every invocation through the same `scan_text`, and the script
left `.config/gate-selftest-baseline.txt`.

### Measured

- Gate mutation: with all seven files byte-identical to `origin/main`, the OLD
  gate prints OK (rc=0) and the NEW gate lists all 8 lines (rc=1); on the fix
  it is OK (rc=0).
- Real configure, `project(C)` with `arm-freertos-armcm3.cmake`, scratch
  `NROS_SDK_STORE` holding `13.2-nros5` (the pin) and `99.0-decoy`: new →
  `via SDK store — …/13.2-nros5/…`; origin/main's toolchain file → `…/99.0-decoy/…`.
  Decoy only → `via system PATH` plus the NOTICE naming `99.0-decoy`.
- riscv64 shell helper, scratch store `14.2-nros1` + `99.0-decoy`: new picks
  `14.2-nros1`; origin/main picks `99.0-decoy`. Decoy only: message fires,
  PATH rung used.
- cyclonedds idlc hints (`cmake -P`, scratch `NROS_HOME`): pin present → hint is
  the pinned bin; decoy only → empty hints + STATUS naming the decoy.
- Unit tests: `nros-build-paths` (pin parser, 2 new) and `nros-tests`
  (`sdk_pin`, 2 new, including the real index's four pins) pass.

### NOT verified

- No fixture/QEMU/riscv64/Zephyr/Cyclone image was built or run against the
  change; the Rust build-script path (`nros_build_paths::riscv64::tool`) was
  unit-tested for the parser only, not exercised inside a real board build.
- Behaviour change on hosts whose store lacks the pin: THIS host holds
  `arm-none-eabi-gcc` `13.2-nros1`/`13.2-nros4` against a `13.2-nros5` pin, so
  it now leaves the store rung and reaches PATH — where `activate.sh` has put
  those same unpinned copies. That second route is issue 1563, filed, not fixed.
