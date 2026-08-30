---
id: 330
title: "Backend facts in RMW-agnostic layers: the zenoh locator default is hardcoded in 4 places (two of them backend-blind), BoardConfig::zenoh_locator names a backend in a core API, and the façade macro hardcodes two register() calls"
status: resolved
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

#### Resolved (part 1)

`nros_rmw_zenoh::DEFAULT_LOCATOR` is now the SSoT. Every agnostic layer supplies
NOTHING and the backend substitutes:

- the "absent" spelling is the **empty string** (`""`) — already the value
  `ExecutorConfig::try_resolve`'s embedded path and `<nros/main.hpp>`'s own
  `NROS_ENTRY_LOCATOR` fallback used, and the only spelling the C/C++ macro edges
  can express (a `#define` expanding to a *value*, not a nullable pointer);
- `nros_rmw_zenoh::shim::session::{normalize_locator, effective_client_locator}`
  collapse `None` **and** `""` to "absent" and apply the default — inside
  `ZenohSession::new`, so both the Rust `Rmw::open` path and the cffi vtable path
  are covered. This matters because `zpico_init_with_config` inserts any non-NULL
  locator as the zenoh CONNECT endpoint, so a bare `""` would be configured as a
  real (broken) endpoint;
- deleted: `nros-node/src/executor/types.rs`'s `DEFAULT_LOCATOR` (+ its env-cache
  and `try_resolve` uses), `nros/src/init.rs`'s separate env ladder,
  `nros-cpp/include/nros/node.hpp`'s hosted local-router default (a null locator
  now flows through `nros_cpp_init` → `None` → the resolver's empty bottom rung);
- `nros-c/include/nros/app_main.h`'s `NROS_ENTRY_LOCATOR` fallback is now `""`.

Precedence is unchanged (hosted env > explicit arg > baked macro > **backend
default** — only the last rung moved).

#### A FIFTH site, and proof the duplication had already drifted

`nros-c/src/support.rs:193` held an *XRCE* default, `"127.0.0.1:2019"`,
substituted in the RMW-blind C layer whenever the caller passed a NULL locator —
and applied to zenoh / dds / cyclonedds builds too, not just xrce. Its port had
already **drifted** from the xrce backend's own `XRCE_DEFAULT_AGENT_PORT` = 2018
(`nros-rmw-xrce/src/internal.h`), which the backend self-applies at
`nros-rmw-xrce/src/session.c`. That is this issue's thesis demonstrated: a
backend fact restated in an agnostic layer does not stay in sync. The
substitution is deleted; the xrce backend's own default applies, and its
"absent locator" test now accepts `""` as well as NULL.

Regression coverage: `nros-rmw-zenoh` `tests/zenoh_integration.rs`
`client_session_with_absent_locator_dials_backend_default` starts a router on the
DEFAULT port and opens a client session with no locator and no
`NROS_LOCATOR`/`ZENOH_LOCATOR` — every other hosted test pins a locator, so
nothing else exercised this rung. Mutation-checked: removing the substitution
turns it red (`ConnectionFailed`).

Parts 2 and 3 are also resolved (below).

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

#### Resolved (part 2)

`BoardConfig::locator()` is the required method; `zenoh_locator()` survives as a
defaulted `#[deprecated]` alias delegating to it, so out-of-tree board crates
keep compiling. Every in-tree `impl` and caller moved to the new name, and each
board's inherent `with_zenoh_locator()` builder gained a `with_locator()`, the
old name kept as a deprecated delegating wrapper.

Two things the audit did not anticipate:

- `nros-board-common`'s **`ThreadxConfig`** trait carries the same
  `zenoh_locator()` accessor. Renamed the same way — but the alias cannot
  preserve behaviour for an out-of-tree overlay that *overrides* the old name,
  because a defaulted method's override is simply no longer consulted. Called
  out in that file's docs.
- The public struct FIELDS (`pub zenoh_locator: &'static str`) across the board
  crates are deliberately NOT renamed — a much larger blast radius through
  struct literals, and the board layer is where naming a backend is legitimate.
  Remaining work, not a leak of the core API.

Verified on the lanes CI actually runs (`check-workspace`, embedded thumbv7em
clippy, nightly rustfmt) plus per-crate builds of the out-of-workspace board
crates. Five board families (esp32s3, both nuttx, threadx-qemu-riscv64,
orin-spe, freertos) could not be compile-verified locally for want of their
SDKs/toolchains; their edits are the identical builder-pair pattern verified
compiling elsewhere.

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

#### Resolved (part 3)

Investigation changed the shape of the fix. The two `register()` calls were
**redundant as registration** — `nros_app_register_backends()`, called a dozen
lines above them, already registers. They were load-bearing only as force-link
anchors: on a Rust-only image the Zephyr module emits a WEAK
`nros_rmw_<x>_register` and calls it if it resolves, and the strong definition
is the backend crate's `#[no_mangle]` export, which staticlib DCE drops unless
the crate being compiled into the staticlib references it.

So the fix is not "make the facade register generically" — it is "move the
anchor to the layer that legitimately names a backend". `nros` itself cannot
host it (it has no backend deps by design); the app crate can, and does, via a
new name-free `nros::force_link_backend!(<crate>)`. The facade macro now names
no backend at all.

Two things that fell out:

- The cyclonedds-only example carried **inert `rmw-zenoh = []` / `rmw-xrce = []`
  feature rows** whose only purpose was to satisfy the facade macro's per-backend
  `cfg` checks under check-cfg (issue #216). Direct evidence the leak imposed a
  cost on consumers. Both rows are deleted, and that example still builds.
- `$backend:path` does not work as the macro's fragment: a `path` fragment may
  not be followed by `::`, so `$backend::register()` fails to parse at the CALL
  site with a misleading "expected an operator". It takes an `ident`.

**A missing anchor is SILENT** — mutation-verified: deleting it from
`examples/zephyr/rust/talker` still builds AND links, `nros_rmw_zenoh_register`
simply vanishes from `librustapp.a`. No gate existed because, while the facade
emitted the anchor, it could not go missing; moving it to the app crate makes it
possible, so `just check rmw-force-link-anchor`
(`scripts/check-rmw-force-link-anchor.sh`, wired into `check-fast`) now requires
an anchor from any Zephyr Rust example whose `rmw-*` feature forwards to a real
backend dep. Mutation-tested in both directions.

Verification: all six zenoh examples build; `rust/service-client` builds against
xrce; the FVP cyclonedds Rust example builds with its inert rows gone; `nm`
confirms `nros_rmw_zenoh_register` present in `librustapp.a` and `zephyr.exe`
with the anchor and ABSENT without it; and the talker runs against a real
zenohd, reaching the readiness marker and publishing.

## Checked and NOT findings

The 177 `cfg(feature = "rmw-…"/"platform-…")` hits in `packages/core` were
triaged file-by-file: the `rmw-cffi` / `rmw-lending` / `platform-*` gates in
`nros`/`nros-node`/`nros-platform` are capability- or seam-gated, not
backend-gated. The cyclonedds hits in `nros-node/executor/{node,spin,action}.rs`
all route through the sanctioned `rmw_type_registry::register_type` seam and name
the backend only in comments. The `ros_edition` axis has zero footprint in
`packages/core` (it lives entirely in cmake/CLI/codegen), so it introduces no new
C5 exposure.
