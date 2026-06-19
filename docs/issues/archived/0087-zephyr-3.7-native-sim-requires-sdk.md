---
id: 87
title: Zephyr 3.7 native_sim fixtures need the downloaded SDK; 4.4 uses host gcc — gate is version-keyed, not board-keyed
status: resolved
type: enhancement
area: zephyr
related: [phase-258]
resolved_in: "scripts/build/zephyr-fixture-run-one.sh + just/zephyr-{ci,dev}.just — board-keyed ZEPHYR_TOOLCHAIN_VARIANT=host"
---

## Resolution (2026-06-20)

Made the host-toolchain export **board-keyed** instead of version-keyed. The
fixture matrix's per-leaf runner (`scripts/build/zephyr-fixture-run-one.sh`)
now sets `ZEPHYR_TOOLCHAIN_VARIANT=host` whenever the leaf's `board` is
`native_sim*` (any line, 3.7 + 4.4), respecting a caller-set variant. Real
embedded boards (FVP cortex-a/r, cyclonedds targets) leave the variant unset →
Zephyr still locates the downloaded SDK as before.

The old version-keyed export in `build-fixtures` (`just/zephyr-ci.just`, gated on
`ZEPHYR_VENV_BIN` ⇒ 4.4 only) was removed; the 4.4 venv-on-PATH stays (it's the
Python 3.12 `find_package(Python3>=3.12)` requirement, genuinely version-specific).
The same board-keyed export was applied to the other native_sim build paths that
carried the version-keyed gate (or no host export at all): `build-logging-smoke`
(pulled into `build-fixtures`) and `build-one`/`ci-both` (`just/zephyr-dev.just`),
plus the west bringup fixtures (`scripts/build/west-fixtures.sh`) — there it stays
board-keyed because the `west_board_import` FVP entry must remain SDK-gated.

Verified live (3.7, in-tree SDK present but NOT referenced):
- `build-rs-talker-zenoh` (pristine): `-- Found toolchain: host (gcc/ld)` (no
  "trying to locate Zephyr SDK" fallback); produced `zephyr.exe`.
- `build-c-talker-zenoh` (pristine): configure picked host too —
  `CMAKE_C_COMPILER=/usr/bin/gcc`, `ZEPHYR_TOOLCHAIN_VARIANT:INTERNAL=host`, and
  **0** `zephyr-sdk`/`pokysdk`/`*-zephyr-elf` references in `CMakeCache.txt`.
  This is the airtight SDK-free proof: with the variant set to `host`,
  `FindZephyr-sdk.cmake` guards out the entire `find_package(Zephyr-sdk)` block,
  so neither the in-tree SDK dir nor the `~/.cmake` package registry is consulted.

Net: the 3.7 `native_sim` **Rust/zenoh** fixture subset is SDK-free, matching 4.4
— a networked host (or the CI image) builds the host-side zephyr tests without the
~GB SDK download.

Scope boundary: the **C/C++** native_sim host-gcc builds do NOT yet complete —
they hit the `nros_config_generated.h` stub error tracked by
[[0088-c-consumer-config-generated-stub]] ("Zephyr: NOT fixed — separate, deeper
integration race"). That is orthogonal to the toolchain gate this issue closes;
the host-gcc path merely *exposes* it on 3.7 now. Likewise the fully-SDK-free
host still needs rust-std targets + clang builtin headers — see
[[0086-zephyr-fixture-rustup-target-race]].

## Observation (2026-06-19)

`native_sim` Zephyr builds can use the **host gcc** toolchain
(`ZEPHYR_TOOLCHAIN_VARIANT=host`) — no Zephyr SDK needed. But the fixture-build
path only exports the host toolchain for the **4.4** line:

`just/zephyr.just:25`
```
ZEPHYR_VENV_BIN := if NROS_ZEPHYR_VERSION == "4.4" { ZEPHYR_WORKSPACE / ".venv312/bin" } else { "" }
```

`just/zephyr-ci.just:47-51` (build-fixtures)
```
venvbin="{{ZEPHYR_VENV_BIN}}"
if [ -n "$venvbin" ]; then
    export PATH="$(realpath "$venvbin"):$PATH"
    export ZEPHYR_TOOLCHAIN_VARIANT=host     # <- only when venvbin set => only 4.4
fi
```

So for the **default 3.7 line**, `ZEPHYR_VENV_BIN` is empty → no
`ZEPHYR_TOOLCHAIN_VARIANT=host` → Zephyr falls back to locating the **SDK**
(`zephyr-sdk-0.16.8`), even for `native_sim` which the host gcc could compile.

## Why it matters

The SDK is a ~GB external download ("provisioned outside the index"; CI bakes it at
`/opt/zephyr-sdk` and runs `just zephyr setup --skip-sdk`). A host without the
pre-baked SDK (or without network) can't build **any** 3.7 zephyr fixture — even the
host-runnable `native_sim` subset — purely because the host-toolchain export is
version-gated rather than board-gated.

## Proposed change

Decouple `ZEPHYR_TOOLCHAIN_VARIANT=host` from the version/venv gate: set it for
**native_sim targets on any line** (3.7 + 4.4). Real embedded boards (FVP /
cortex-a/r, cyclonedds) still need the SDK and stay SDK-gated
(`resolve-fvp-bin.sh` etc.). Net: the `native_sim` zenoh fixture subset becomes
SDK-free on 3.7 too, matching 4.4 — so a networked host (or the CI image) can run
the host-side zephyr tests without the SDK download.

(Note: even with this, a *fully* SDK-free host still needs the rust-std targets +
clang builtin headers present — see [[0086-zephyr-fixture-rustup-target-race]] for
the rustup-race half; the clang `stddef.h` resource-dir is a host-provisioning
prerequisite, not a nano-ros bug.)

## Impact

Enhancement — widens host-runnable zephyr coverage. Orthogonal to phase-258
(surfaced while running test-all for it).
