---
id: 365
title: "FreeRTOS board build can't find `nros/app_config.h`: build.rs still uses the pre-phase-321 `core/nros-c/include` path after nros-c moved to packages/api"
status: open
type: bug
severity: medium
area: freertos
related: [phase-321, issue-0361]
---

## Finding (2026-07-31, surfaced building freertos fixtures for #361/#356)

`just freertos build-fixtures` fails compiling the board's generated config TU:

```
error: failed to run custom build command for
  `nros-board-mps2-an385-freertos v0.4.0`
  cargo:warning=…/build/…/nros_app_config_def.c:21:10:
    fatal error: nros/app_config.h: No such file or directory
```

The board's `build.rs` emits `nros_app_config_def.c` (a small TU that
`#include <nros/app_config.h>` and defines the app-config struct) and compiles it
with `arm-none-eabi-gcc`. The `-I` for the nros-c headers is stale.

## Root cause

`phase-321 W2.e` (`7e3e15b4d refactor: move the façade and language bindings into
packages/api/`) moved `nros-c` from `packages/core/nros-c` to
`packages/api/nros-c`. The header is now at
`packages/api/nros-c/include/nros/app_config.h`.

`packages/boards/nros-board-mps2-an385-freertos/build.rs:107` still joins the OLD
path:

```rust
    .join("core/nros-c/include"),   // ← stale; nros-c is now under packages/api
```

so the emitted `-I …/packages/core/nros-c/include` points at a directory that no
longer exists and `<nros/app_config.h>` is not found. The build.rs's OWN doc
comments were updated to the new location (line 241 cites
`packages/api/nros-c/include/nros/zephyr/app_config.h`), but the code path (107)
was not — a half-applied move.

**Missed site, not a class.** The sibling `nros-board-threadx-linux/build.rs:53`
already uses the correct `packages/api/nros-c/include`, so phase-321 updated that
board and missed the freertos one. A grep for `core/nros-c/include` across
`packages/boards/*/build.rs` returns ONLY the freertos board; nuttx/esp32/etc. do
not emit this TU. So the fix is a single site.

## Fix

`packages/boards/nros-board-mps2-an385-freertos/build.rs:107`:
`core/nros-c/include` → `packages/api/nros-c/include` (match the threadx-linux
board's `workspace_root.join("packages/api/nros-c/include")` spelling). Then
`just freertos build-fixtures` should compile the config TU.

## Secondary (separate)

The crate is `nros-board-mps2-an385-freertos v0.4.0` while the workspace is
`0.5.0`. Several board crates sit at `0.4.x` (`git grep '^version = "0.4'
packages/boards/*/Cargo.toml`). Whether these are a real version-lockstep drift
or an intentional board-crate exemption is a separate question from the include
path — noted here, not conflated with the fix above.

## Impact

- `just freertos build-fixtures` cannot build the freertos workspace fixtures →
  the tier-2 `freertos,*` coordinates cannot build. This is the NEXT blocker after
  the #361/#356 codegen fix cleared "no nodes on board".
- Latent since phase-321 W2.e because full freertos fixture builds are rarely run
  locally (per-platform toolchains).

## Repro

```
source ./activate.sh && source /opt/ros/humble/setup.bash
just freertos build-fixtures
# … fatal error: nros/app_config.h: No such file or directory
```
