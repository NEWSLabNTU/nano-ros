---
id: 330
title: "Backend facts in RMW-agnostic layers: the zenoh locator default is hardcoded in 4 places (two of them backend-blind), BoardConfig::zenoh_locator names a backend in a core API, and the façade macro hardcodes two register() calls"
status: open
type: bug
severity: medium
area: core
related: [issue-0225, rfc-0001, rfc-0031]
---

## Finding (audit 2026-07-28, P2)

New instances of the #225 class (a concrete backend named in a layer that must
stay agnostic), this time in APIs and defaults rather than a module name.

### 1. The zenoh-router default lives in four layers

`tcp/127.0.0.1:7447` is hardcoded independently at:

- `packages/core/nros-cpp/include/nros/node.hpp:717` and `:767`
- `packages/core/nros-c/include/nros/app_main.h:61`
- `packages/core/nros-node/src/executor/types.rs:160` (`DEFAULT_LOCATOR` — the
  **agnostic core**)
- `packages/core/nros/src/init.rs:128`

Two of those layers are RMW-blind by design, and the value is a zenoh fact. It is
also an I4 duplicated-SSoT: four places to change, and a cyclone/xrce build
carries a zenoh default it never uses.

Fix: one default owned by the registration seam / backend, consumed through the
RFC-0045 ladder; delete the other three.

### 2. A backend name in a core public trait

`packages/core/nros-platform/src/board/config.rs:36` — the core platform crate's
public `BoardConfig` trait declares `fn zenoh_locator(&self) -> &str`, with
`with_zenoh_locator()` builders mirrored across the board crates. Unlike #225
(which was a module *name*), this is the published API surface.

The same leak appears in a public C header's contract text:
`packages/core/nros-c/include/nros/init.h:7` ("manages the middleware session
(zenoh-pico)").

Fix: rename to `locator()` — matching the vtable's own `locator` parameter and
`ExecutorConfig::new` — keeping a deprecated alias for the board crates.

### 3. The façade macro hardcodes two of three backends

`packages/core/nros/src/lib.rs:385,389` — `zephyr_component_main!` emits
`::nros_rmw_zenoh::register()` and `::nros_rmw_xrce_cffi::register()`, naming two
concrete backends while cyclonedds is handled asymmetrically through the C hook.
RFC-0001 states only the RMW backend crates know about specific transport
protocols; the façade is not one of them.

Fix: emit a generic force-link anchor plus the `nros_app_register_backends` call
from the board/platform gate that already owns backend selection, so the façade
stays name-free. (Note the force-link constraint from archived issues 0155/0163 —
the anchor must keep a *direct* reference or rustc's staticlib DCE drops the
backend's `#[no_mangle]` export.)

## Checked and NOT findings

The 177 `cfg(feature = "rmw-…"/"platform-…")` hits in `packages/core` were
triaged file-by-file: the `rmw-cffi` / `rmw-lending` / `platform-*` gates in
`nros`/`nros-node`/`nros-platform` are capability- or seam-gated, not
backend-gated. The cyclonedds hits in `nros-node/executor/{node,spin,action}.rs`
all route through the sanctioned `rmw_type_registry::register_type` seam and name
the backend only in comments. The `ros_edition` axis has zero footprint in
`packages/core` (it lives entirely in cmake/CLI/codegen), so it introduces no new
C5 exposure.
