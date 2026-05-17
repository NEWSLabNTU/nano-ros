# Phase 134 — zenoh-pico Build-Flag Canonical Header

**Goal.** Eliminate the linker-time class where `libnros_rmw_zenoh.a`
ships `_z_f_link_*` wrappers compiled under one `Z_FEATURE_LINK_*`
value while the underlying transport impls compile under a different
value, leaving the archive internally inconsistent. The root fix is
structural: make `zenoh_config.h` the **single source of truth** for
every `Z_FEATURE_LINK_*` flag and stop overriding it from `build.rs`
via `.define(…)` calls. After the change there is no path through
`build.rs` where two compile units can disagree about a flag because
both paths read the same header.

This phase is the minimal, ship-fast fix. The larger "unify the two
build paths and eliminate CMake from `zpico-sys`" effort is deferred
to Phase 136.

**Status.** Not started.

**Priority.** P1 — blocks ~86 of 148 post-Phase-131 ci failures
(`native_api` 58, `rmw_interop` 40, `c_xrce_api` 10) all caused by
this one half-defined-archive class.

**Depends on.** Phase 128 (which introduced the gate mismatch by
deleting "inert link-tcp / udp-unicast" but leaving the multicast
side half-wired across the cc-rs / CMake split).

**Related.** Phase 133 (post-131 ci sweep — same failure inventory),
Phase 131 (forced clean install that surfaced the latent bug),
Phase 136 (structural unify of the two build paths — supersedes the
need for 134's `.define()` deletions but does not replace 134's
header-as-source-of-truth contract).

---

## Overview

`packages/zpico/zpico-sys/build.rs` has two compile paths that build
zenoh-pico for the C shim layer:

1. **cc-rs path** (`build_c_shim`, `build_zenoh_pico_threadx`,
   `build_zenoh_pico_freertos`, …) — used for every embedded target
   (threadx, freertos, nuttx, bare-metal, esp-idf, Orin SPE).
2. **CMake path** (`build_zenoh_pico_native`) — used for the POSIX
   path. Delegates to upstream `zenoh-pico/CMakeLists.txt`.

Today every flag has up to four expressions: a `LinkFeatures` field,
a per-path `build.define(…)` literal in cc-rs, a separate
`cmake_cfg.define(…)` literal in CMake, and a `#define …` line
written into `zenoh_config.h` by `generate_config_header`. Drift
between any two of those four is silent until the linker fails.

After this phase:

- `zenoh_config.h` is canonical. Every `Z_FEATURE_LINK_*` flag
  appears there once, derived from `LinkFeatures`.
- Both compile paths force-include the header (`-include
  <out_dir>/zenoh_config.h`).
- Every `build.define("Z_FEATURE_LINK_*", …)` and every
  `cmake_cfg.define("Z_FEATURE_LINK_*", …)` in `build.rs` is
  deleted.
- Platform-invariant overrides (e.g. SPE has no Ethernet, so TCP /
  UDP must be 0 regardless of `LinkFeatures`) live in a per-platform
  policy table, not as inline literals.

---

## Architecture

### A. Where the flags get set today

```
build.rs::generate_config_header   →  zenoh_config.h  (declarative)
build.rs::build_c_shim             →  build.define(...)  (literals)
build.rs::build_zenoh_pico_threadx →  build.define(...)  (literals)
build.rs::build_zenoh_pico_freertos →  build.define(...) (literals)
build.rs::build_zenoh_pico_native  →  cmake_cfg.define(...) (literals + reliance on CMake default)
build.rs Orin-SPE block (line ~1925) → build.define(...) (literals)
```

Four sources of truth + one CMake-default fallthrough = guaranteed
divergence given enough changes. Phase 128 was the change.

### B. Single source of truth

```
LinkFeatures + per-platform LinkPolicy  →  zenoh_config.h  →  every compile unit
```

`build.rs` writes the header once. Both paths `-include` it.
Per-path `build.define(…)` / `cmake_cfg.define(…)` calls for
`Z_FEATURE_LINK_*` are deleted entirely. Other defines unrelated to
link features (`ZENOH_GENERIC`, `ZENOH_LINUX`, `ZPICO_SMOLTCP`,
`Z_FEATURE_MULTI_THREAD`, …) stay where they are — they are not the
bug class. The CMake path keeps the few non-`Z_FEATURE_LINK_*`
defines it needs to drive upstream's CMake (`BUILD_SHARED_LIBS=OFF`,
buffer sizes, …) untouched.

For platform invariants (Orin SPE has no Ethernet; bare-metal
serial-only boards have no IVC) introduce a `LinkPolicy` struct that
masks `LinkFeatures` before `generate_config_header` writes the
header. Policies are platform-specific data, not inline literals
scattered through `build.rs`. The Orin SPE block becomes:

```rust
// Before: ten literal build.define("Z_FEATURE_LINK_*", "0") calls
// After:
let link = LinkFeatures::from_env().apply(LinkPolicy::orin_spe());
generate_config_header(&out_dir, &link, &buf_config);
// remaining ZENOH_GENERIC / ZENOH_ORIN_SPE / Z_FEATURE_MULTI_THREAD
// defines stay — they are not link-feature gates.
```

---

## Work Items

- [x] **134.1 — Audit `Z_FEATURE_LINK_*` sites.** Done; see
      "Audit" subsection below for the full pre-fix table. Smoking
      gun confirmed: `build_zenoh_pico_native` (the POSIX CMake path)
      sets `Z_FEATURE_LINK_SERIAL=0`, `_IVC=env`, `_CUSTOM=env`,
      `_TLS=conditional` only — it omits `_TCP`, `_UDP_UNICAST`,
      `_UDP_MULTICAST`, so upstream's CMake default (=1) wins. Then
      `build.rs:1271-1280` deletes `src/system/unix/network.c` from
      the CMake-built copy because "platform symbols and networking
      are provided by zpico-platform-shim via nros-platform", but
      `packages/zpico/zpico-sys/c/zpico/platform_aliases.c` only
      supplies `_z_*_tcp` / `_z_*_udp_unicast` aliases, not the
      multicast equivalents. Net effect, confirmed via `nm
      build/install/lib/libnros_rmw_zenoh.a`:
      ```
      T _z_f_link_{open,close,free,listen,read,read_exact,
                   write,write_all}_udp_multicast
      U _z_read_udp_multicast
      U _z_read_exact_udp_multicast
      ```
      Every link-time consumer of the multicast wrappers fails.

- [x] **134.2 — `LinkPolicy` landed.** `enum PolicyChoice { Force(bool), Follow }`
      + `struct LinkPolicy { tcp, udp_unicast, udp_multicast, serial,
      raweth, tls, ivc, custom: PolicyChoice }` + constructors
      `LinkPolicy::passthrough()`, `::posix()`, `::orin_spe()` +
      `LinkFeatures::apply(&LinkPolicy) -> Self` mask method
      (build.rs:32-178). Dispatch picks per-platform policy at line
      ~605 ahead of `generate_config_header`.
- [x] **134.3 — `zenoh_generic_config.h` canonical.** `generate_config_header`
      now runs once before the if-else dispatch (was inside each
      arm). The CMake path adds `.define("ZENOH_GENERIC", "1")` +
      `.cflag(-I<out_dir>/zenoh-config)` so upstream
      `zenoh-pico/config.h` routes its `#ifdef ZENOH_GENERIC` branch
      into our header. cc-rs paths already had `ZENOH_GENERIC`; they
      now rely on the header for `Z_FEATURE_LINK_*` instead of
      duplicating literals.
- [x] **134.4 — All `Z_FEATURE_LINK_*` literals deleted.** Removed
      from `build_c_shim` (line ~1670), `build_zenoh_pico_embedded`
      (~1820), `build_zenoh_pico_orin_spe` (~1999),
      `build_zenoh_pico_freertos` (~2168), `build_zenoh_pico_nuttx`
      (~2314), `build_zenoh_pico_threadx` (~2557), and the CMake
      path's `cmake_cfg.define(…)` (~1409). `grep
      'build\.define(.Z_FEATURE_LINK' build.rs` returns only the
      doc-comment references. SPE invariants now live in
      `LinkPolicy::orin_spe()`. `Z_FEATURE_MATCHING` (previously
      forced on by the CMake path) bumped to `1` inside
      `generate_config_header` so every path keeps cross-network
      routing working.
- [x] **134.5 — `nm` regression script + `just` recipe.**
      `scripts/check-zenoh-archive-symbols.sh` walks
      `build/install/lib/libnros_rmw_zenoh.a` and asserts wrapper /
      impl parity for every transport in `{tcp, udp_unicast,
      udp_multicast, serial, ivc}`. SIGPIPE / `set -o pipefail` race
      avoided by tempfile-buffering `nm` output and grepping the
      file. Recipe `just check-zenoh-archive` wires it in.
- [x] **134.6 — E2E tests landed.** See "Acceptance / E2E" below.

---

## Acceptance / E2E

- [x] **E2E.1 — Symbol parity gate green.**
      `packages/testing/nros-tests/tests/zenoh_archive_symbols.rs`
      wraps `scripts/check-zenoh-archive-symbols.sh` as a cargo-
      runnable test. Walks `build/install/lib/libnros_rmw_zenoh.a`
      and asserts no `U / T` mismatch for any transport's wrappers
      and impls. Passes today; FAILS (no skip) if the contract
      regresses.
      ```
      $ cargo test -p nros-tests --test zenoh_archive_symbols
      test zenoh_archive_wrapper_impl_parity ... ok
      ```
- [x] **E2E.2 — Native C / C++ link errors gone.** `cargo test -p
      nros-tests --test native_api` no longer fails with
      `undefined reference to '_z_read_udp_multicast'` or any
      sibling. Binaries now link and execute past `main`. The
      remaining `native_api` failures are a different defect
      (`duplicate #[distributed_slice] with name
      "RMW_INIT_ENTRIES"` at runtime — Phase 133 territory) and not
      tracked by Phase 134.
- [x] **E2E.4 — Canonical header content gate green.**
      `packages/testing/nros-tests/tests/zenoh_header_parity.rs`
      finds the generated
      `<OUT_DIR>/zenoh-config/zenoh_generic_config.h`, parses every
      `Z_FEATURE_LINK_*` plus `Z_FEATURE_INTEREST` /
      `Z_FEATURE_MATCHING`, and asserts the values match
      `LinkPolicy::posix()` exactly (TCP/UDP/MC/SERIAL=1,
      BT/WS/SERIAL_USB/IVC/CUSTOM/TLS/RAWETH=0, INTEREST=MATCHING=1).
      Pre-134 the CMake path bypassed this header entirely; the
      gate locks the post-134 contract.
      ```
      $ cargo test -p nros-tests --test zenoh_header_parity
      test posix_canonical_header_matches_link_policy ... ok
      ```
- [ ] **E2E.3 — Flag-drift cross-product (deferred).** Programmatic
      16-combination `LinkFeatures` matrix gated behind a
      `link-flag-matrix` feature. Cycle time ~5 min. Deferred to a
      Phase 134 follow-up — the structural drift the gate catches
      is the same one E2E.1 + E2E.4 already cover; the cross-
      product is belt-and-braces.
- [ ] **E2E.5 — `just ci` regression accounting (deferred).** Full
      `just ci` re-run + document the fail-count drop in this doc.
      `native_api` (~58), `rmw_interop` (~40), and `c_xrce_api`
      (~10) now LINK; the remaining test failures are downstream
      (`distributed_slice` duplicate registration, Phase 133). Full
      number-crunching deferred until Phase 133 lands so the delta
      attributable to Phase 134 is isolatable.

---

## Audit (134.1 — pre-fix state)

Eight functions in `packages/zpico/zpico-sys/build.rs` set
`Z_FEATURE_LINK_*` at compile time. Columns marked `link.X` derive
from `LinkFeatures::from_env()`; `dflt(N)` means upstream's CMake
default applies; `=N` means hardcoded literal.

| Function | TCP | UDP_UNI | UDP_MC | SERIAL | RAWETH | IVC | CUSTOM | TLS | WS | BT | SERIAL_USB |
|---|---|---|---|---|---|---|---|---|---|---|---|
| `generate_config_header` (writes `zenoh_generic_config.h`) | link | link | link | link | link | link | link | link | 0 | 0 | 0 |
| `build_zenoh_pico_native` (POSIX, CMake) | **dflt(1)** | **dflt(1)** | **dflt(1)** | 0 | dflt(0) | env | env | conditional | dflt(0) | dflt(0) | dflt(0) |
| `build_c_shim` (cc-rs shim TU) | link | link | link | link | dflt | link | link | link | dflt | dflt | dflt |
| `build_zenoh_pico_embedded` (cc-rs) | link | link | link | link | dflt | link | link | link | 0 | 0 | dflt |
| `build_zenoh_pico_orin_spe` (cc-rs) | **0** | **0** | **0** | **0** | dflt | link | link | **0** | 0 | 0 | dflt |
| `build_zenoh_pico_freertos` (cc-rs) | link | link | link | link | dflt | link | link | dflt | 0 | 0 | dflt |
| `build_zenoh_pico_nuttx` (cc-rs) | link | link | link | link | dflt | link | link | dflt | 0 | 0 | dflt |
| `build_zenoh_pico_threadx` (cc-rs) | link | link | link | link | dflt | link | link | dflt | 0 | 0 | dflt |

Observations:

1. The CMake path is the only divergent one: it skips
   `_TCP/_UDP_UNICAST/_UDP_MULTICAST` so upstream's CMake default of
   1 wins regardless of `LinkFeatures`. Every other path threads
   through `LinkFeatures`.
2. Every path EXCEPT the CMake one is already shaped right — flags
   match the canonical header for every variable derived from
   `LinkFeatures`. The CMake path's deletion of
   `src/system/unix/network.c` (line 1271-1280) is what turns the
   wrappers-without-impls case into a hard linker failure.
3. The Orin SPE path hardcodes a slew of `=0` invariants
   (`_TCP=_UDP_UNICAST=_UDP_MULTICAST=_SERIAL=_TLS=0`) because the
   SPE has no Ethernet. These are platform invariants, not
   `LinkFeatures` opt-outs — they belong in a per-platform policy
   table (see 134.2 `LinkPolicy::orin_spe()`), not as scattered
   literals.
4. `LinkFeatures::from_env()` (build.rs:48) currently forces `tcp =
   udp_unicast = udp_multicast = serial = true` — the comment at
   line 35-43 explains the locator string selects the actual
   transport at runtime. The POSIX CMake path's missing multicast
   impl in `platform_aliases.c` is therefore a real bug, not a
   feature gate working as intended.

## Notes

- The `Z_FEATURE_LINK_UDP_MULTICAST` mismatch is the smoking gun, but
  every link feature has the same shape. The audit (134.1) catches
  any sibling already festering.
- Do **not** "fix" by setting `Z_FEATURE_LINK_UDP_MULTICAST=0`
  everywhere without checking whether the runtime needs multicast.
  rmw_zenoh discovery uses multicast on LAN by default. Per-platform
  `LinkPolicy` is the correct shape.
- Phase 131's parallel-build pressure did not introduce the bug — it
  surfaced because Phase 131 forced a clean install where the bug
  had previously been masked by stale archives with both halves
  defined.
- This phase intentionally keeps the two build paths split. Phase
  136 unifies them. 134 makes the canonical-header contract hold
  even with the split present, so 136 can be a pure refactor
  afterward.
