# Phase 134 — zenoh-pico UDP Multicast Feature-Gate Alignment

**Goal.** Eliminate the linker-time mismatch where
`libnros_rmw_zenoh.a` ships
`_z_f_link_{open,close,read,read_exact}_udp_multicast` wrappers
compiled with `Z_FEATURE_LINK_UDP_MULTICAST=1` but **omits** the
underlying `_z_read_udp_multicast` / `_z_read_exact_udp_multicast`
transport fns (compiled with `=0`). The archive is internally
inconsistent and any C consumer that pulls multicast in transitively
fails to link.

**Status.** Not started.

**Priority.** P1 — blocks every C / C++ native example link path
(~86 of 148 post-Phase-131 ci failures: native_api 58, rmw_interop
40, c_xrce_api 10 are all this single root cause).

**Depends on.** Phase 128 (which introduced the gate mismatch by
deleting "inert link-tcp / udp-unicast" but leaving the multicast
side half-wired).

**Related.** Phase 133 (post-131 ci sweep), Phase 131 (the trigger
that first hit it under clean CI on this machine).

---

## Overview

`packages/zpico/zpico-sys/build.rs` has **two** code paths that
compile zenoh-pico for the C shim layer:

1. **Direct cc-rs build** for embedded / threadx / freertos /
   nuttx / bare-metal builds (functions `build_c_shim`,
   `build_zenoh_pico_threadx`, …).
2. **CMake build via `build_zenoh_pico_native`** for the POSIX path,
   driven from a separate CMake configure step that produces
   `build/cmake-zenoh/cargo/nros-rmw-zenoh-staticlib_*/.../out/zenoh-pico-build/…`.

Phase 128's audit only flipped the multicast `Z_FEATURE_LINK_UDP_*`
flags in one path. The CMake build still uses an older flag set,
so:

```
$ nm build/install/lib/libnros_rmw_zenoh.a | grep _z_read_udp_multicast
                 U _z_read_udp_multicast          # ← undefined, expected to come from network.c
                 U _z_read_exact_udp_multicast    # ← undefined

$ nm build/install/lib/libnros_rmw_zenoh.a | grep _z_f_link_.*udp_multicast
0000000000000000 T _z_f_link_open_udp_multicast   # ← defined
0000000000000000 T _z_f_link_read_udp_multicast   # ← defined, calls the missing _z_read_*
0000000000000000 T _z_f_link_read_exact_udp_multicast
...
```

Source locations of the gate:

- Defines: `packages/zpico/zpico-sys/zenoh-pico/src/system/unix/network.c`
  lines 809 (`_z_read_udp_multicast`) and 856 (`_z_read_exact_udp_multicast`),
  both inside `#if Z_FEATURE_LINK_UDP_MULTICAST == 1`.
- Callers: `packages/zpico/zpico-sys/zenoh-pico/src/link/multicast/udp.c`
  lines 176 + 182, inside the same `#if`.

Both files MUST be compiled with the same value of
`Z_FEATURE_LINK_UDP_MULTICAST`. Today they aren't — the CMake path
sets the wrappers' file ON and the system file OFF (or vice versa).

---

## Architecture

### A. Where the flags get set

```
build.rs::generate_config_header
  ├── writes zenoh_config.h with #define Z_FEATURE_LINK_UDP_MULTICAST {0|1}
  └── used by the cc-rs path (build_c_shim, build_zenoh_pico_threadx, …)

build.rs::build_zenoh_pico_native
  ├── invokes CMake on third-party zenoh-pico/CMakeLists.txt
  └── passes -DZ_FEATURE_LINK_UDP_MULTICAST=… via build.define("Z_FEATURE_LINK_UDP_MULTICAST", …)
```

Today the two paths can disagree because:

- `build.rs:1716–1717` and `:299–300` write the cc-rs flag from
  `link.udp_multicast`.
- `build.rs:1927` hard-codes `build.define("Z_FEATURE_LINK_UDP_MULTICAST", "0")`
  for one specific subpath. This is the gate that flips `network.c`
  off while the rest of the source is compiled with the source-of-
  truth value.

### B. What the fix looks like

Single source of truth for the flag, plumbed identically into:
1. the cc-rs `build.define(…)` call(s)
2. the CMake `build.define(…)` call(s) used by `build_zenoh_pico_native`
3. the generated `zenoh_config.h` header

After the change, `nm libnros_rmw_zenoh.a` shows either both
wrapper-and-impl symbols **defined** (multicast on) or neither
(multicast off). No half-states.

---

## Work Items

- [ ] 134.1 — Audit every `Z_FEATURE_LINK_*` site in `build.rs`. Build a
      table: caller path → flag-source variable → which compile unit
      sees the value. Confirm the disagreement on
      `Z_FEATURE_LINK_UDP_MULTICAST` (and check the same shape for
      `_TCP`, `_UDP_UNICAST`, `_SERIAL`, `_WS`).
      **Files.** `packages/zpico/zpico-sys/build.rs`.

- [ ] 134.2 — Introduce a single
      `fn zenoh_link_flags(link: &LinkFeatures) -> Vec<(&'static str, &'static str)>`
      that returns the canonical `(name, "0"|"1")` list. Both the
      cc-rs and CMake paths consume it.
      **Files.** `packages/zpico/zpico-sys/build.rs`.

- [ ] 134.3 — Replace every ad-hoc `build.define("Z_FEATURE_LINK_*", …)`
      and every `zenoh_config.h` write with the helper from 134.2.
      **Files.** `packages/zpico/zpico-sys/build.rs`.

- [ ] 134.4 — Add a build-time invariant check: after `cargo build -p
      nros-rmw-zenoh-staticlib`, run `nm build/install/lib/libnros_rmw_zenoh.a`
      and assert that for every `_z_f_link_*_udp_multicast` symbol
      defined, the matching `_z_*_udp_multicast` impl is also defined
      (no `U`). Wire into `just doctor` or a new `just check-zenoh-archive`
      recipe. Catches the regression class for good.
      **Files.** `scripts/check-zenoh-archive-symbols.sh`, `justfile`.

- [ ] 134.5 — Re-run `just ci`. Expected: ~86 of the 148 fails
      (native_api, rmw_interop, c_xrce_api categories) drop to PASS
      assuming local env has zenohd / cmake. Surviving fails should
      be the `[SKIPPED]` precondition panics (XRCE agent + ROS 2 +
      cross-toolchain).

---

## Acceptance

- [ ] `nm build/install/lib/libnros_rmw_zenoh.a | grep _z_read_udp_multicast`
      returns either zero matches OR matches with `T` (defined), never `U`.
- [ ] `just check-zenoh-archive` passes (new recipe from 134.4).
- [ ] `just ci` no longer reports the
      `undefined reference to '_z_read_*_udp_multicast'` linker class.
- [ ] `just ci` test-all FAIL count drops by ≥85.

---

## Notes

- The `Z_FEATURE_LINK_UDP_MULTICAST` symmetry is the smoking gun, but
  the audit (134.1) should look at every link-feature pair. The
  Phase 128 cleanup deleted whole feature dirs; survivors may have
  the same shape bug.
- Do not "fix" by setting `Z_FEATURE_LINK_UDP_MULTICAST=0` everywhere
  without checking whether the runtime needs multicast (rmw_zenoh
  discovery uses multicast on LAN by default). Set the flag to match
  what the runtime actually requires per `LinkFeatures`.
- Phase 131's parallel-build pressure didn't introduce the bug —
  it surfaced because Phase 131 forced a clean install where the
  bug had previously been masked by stale archives from earlier
  builds that happened to have both halves defined.
