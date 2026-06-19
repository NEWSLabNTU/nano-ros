---
id: 87
title: Zephyr 3.7 native_sim fixtures need the downloaded SDK; 4.4 uses host gcc — gate is version-keyed, not board-keyed
status: open
type: enhancement
area: zephyr
related: [phase-258]
---

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
