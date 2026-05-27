# Phase 184 - nano-ros consumable by a full-C++ app on the FVP / newlib profile

**Goal.** A downstream Zephyr application that links a full C++ stack
(Autoware control + Eigen) against nano-ros's `nros-cpp` API + CycloneDDS
RMW builds clean — and eventually boots — on the **Autoware safety-island**
profile: board `fvp_baser_aemv8r_smp`, Zephyr 3.7 LTS, `CONFIG_NEWLIB_LIBC=y`
+ `CONFIG_GLIBCXX_LIBCPP=y`. Phase 180 made nano-ros a consumable Zephyr
module and proved it on `native_sim`; Phase 184 closes the gaps that only
surface on the FVP + newlib + full-libstdc++ + real-downstream-app profile.

**Status.** In progress (2026-05-27). Surfaced by the autoware-safety-island
(ASI) west-pin bump `70ab6227d → be4c51364` (610 commits). 184.A landed
(cxx-compat passthrough guard). 184.B–184.E open.

**Priority.** P2 — unblocks the Autoware safety-island actuation bring-up
(Phase 117). No new external consumers blocked beyond ASI today.

**Depends on.** Phase 180 (version-spanning consumable Zephyr module —
the module foundation, copy-out examples, snippets, patch story). Phase 117
(Cyclone DDS RMW + safety-island). Builds directly on 180.A's
`force-include scoping` + `net_ip_mreq guard`, which covered `native_sim`
but not the FVP/newlib profile.

## Overview

Phase 180 verified the consumable-module story end-to-end on `native_sim`
(zenoh + cyclonedds, 3.7 + 4.4). ASI is the first *real* downstream consumer
on a different profile, and it exposed three classes of gap plus a missing
verification lane:

1. **Defining C++ compat shims collide with a full libstdc++.** nano-ros
   ships `zephyr/cxx-compat/` shims and adds the dir to the **global** app
   include path (`zephyr_include_directories`). The benign re-export shims
   (`<cstdlib>`/`<cstdio>`/`<cstring>` — `using ::name;`) are fine on top of
   a real header, but the Phase 11W.3 *defining* shims (`<atomic>`,
   `<chrono>`, `<thread>`, `<random>`) re-`#define` `std::atomic<>`,
   `std::atomic_thread_fence`, `std::chrono::*`, `std::this_thread::*`, etc.
   On a profile whose SDK ships a full libstdc++ (the aarch64-zephyr-elf
   newlib SDK), a consumer that pulls the real `<atomic>` (transitively via
   `<memory>`) and the shim hits a hard redefinition error.

2. **`net.c` multicast join assumes `struct ip_mreqn`.**
   `nros-platform-zephyr/src/net.c` selects the `ip_mreqn` setsockopt path
   whenever `IP_ADD_MEMBERSHIP` is defined. On the FVP newlib profile
   `IP_ADD_MEMBERSHIP` is provided (newlib `<netinet/in.h>`) together with
   `struct ip_mreq` but **not** the Linux-extension `struct ip_mreqn`, so the
   `ip_mreqn` storage is incomplete and the TU fails to compile.
   `zephyr/cyclonedds-config/zephyr_ipv4_compat.h` already assumes "Zephyr
   ≥3.7 defines `struct ip_mreqn` in `<zephyr/net/socket.h>`" — that
   assumption does not hold for this profile.

3. **The consuming app's own C++ TUs lack `std::` C-library names.** The
   Autoware/Eigen translation units use `std::rand`, `std::exit`,
   `std::abort`, `std::strtod`, … . They do not carry nano-ros's
   `cxx-compat/` dir (it is wired for nano-ros's own targets), and the
   newlib `<cstdlib>` reachable in those TUs under the actuation build flags
   does not surface the full set into `std`. This is partly a downstream
   porting concern, but nano-ros should offer a clean, documented path
   rather than leaving every consumer to rediscover it.

4. **No CI lane for this profile.** Phase 180 CI builds the example matrix
   on `native_sim` (3.7 + 4.4). Nothing builds a full-C++ downstream app on
   `fvp_baser_aemv8r_smp` + newlib + libstdc++, so 184.A/184.B-class
   regressions land silently.

Out of scope: the Autoware control stack's own picolibc/newlib portability
(it is ASI-vendored). 184.C delivers the nano-ros-side enablement + docs;
fixing every Autoware TU is ASI's work, tracked in the ASI repo.

## Architecture

The fixes stay on the nano-ros side of the module boundary:

- **cxx-compat shims become conditional.** Each *defining* shim probes a
  libstdc++-internal header it does not shadow (`<bits/atomic_base.h>`,
  `<bits/chrono.h>`, `<bits/std_thread.h>`, `<bits/random.h>`) and, when
  present, `#include_next`s the real header and defines nothing. Targets
  without libstdc++ (picolibc / minimal-libc, e.g. the native_sim profiles
  Phase 180 proved) keep the existing minimal shim — the probe is false, so
  it is a no-op for them.

- **`net.c` multicast join uses the portable `struct ip_mreq`** (or probes
  `ip_mreqn` separately) so it compiles wherever `IP_ADD_MEMBERSHIP` exists,
  regardless of whether the Linux `ip_mreqn` extension is available. The
  existing `zephyr_ipv4_compat.h` is the home for the probe.

- **A public, opt-in std-C-library compat surface** for downstream C++ apps
  on newlib, or — if that is rejected — a documented consumer recipe
  (force-include / Kconfig). Decided in 184.C.

- **An FVP-profile consumer smoke** added to the Zephyr CI cluster, mirroring
  ASI's shape (nros-cpp + CycloneDDS, full libstdc++) at minimal size.

## Work Items

### 184.A — cxx-compat shims defer to a real libstdc++

**Files.** `zephyr/cxx-compat/{atomic,chrono,thread,random}`.

- [x] Guard each defining shim with `#if __has_include(<bits/...>)` →
      `#include_next` the real header; else keep the minimal shim
      (commit `fix(zephyr): cxx-compat shims defer to real libstdc++ when present`)
- [ ] Verify the FVP actuation build advances past the `atomic_thread_fence`
      redefinition (confirmed locally against the ASI bump; re-confirm here)
- [ ] Reconcile with 180.A's "force-include scoping" claim — document why the
      global `zephyr_include_directories(cxx-compat)` still needs per-shim
      guarding, or scope the dir so defining shims never reach consumer TUs
- [ ] Decide whether the benign re-export shims should use the same guard for
      consistency (they do not collide today)

### 184.B — portable multicast join on newlib (`ip_mreq` vs `ip_mreqn`)

**Files.** `packages/core/nros-platform-zephyr/src/net.c`,
`zephyr/cyclonedds-config/zephyr_ipv4_compat.h`.

- [ ] Reproduce: `IP_ADD_MEMBERSHIP` defined + `struct ip_mreqn` incomplete on
      `fvp_baser_aemv8r_smp` + `CONFIG_NEWLIB_LIBC=y` (root: newlib provides
      `ip_mreq`, not the Linux `ip_mreqn`)
- [ ] Switch the join/leave path to portable `struct ip_mreq`
      (`imr_multiaddr` + `imr_interface`), or add a separate `ip_mreqn`
      availability probe alongside the existing `IP_ADD_MEMBERSHIP` gate
- [ ] Fix `zephyr_ipv4_compat.h`'s "≥3.7 defines `ip_mreqn`" assumption for the
      newlib profile
- [ ] Confirm the existing native_sim NSOS dual-`net_ip_mreq`/`net_ip_mreqn`
      path is unaffected

### 184.C — downstream C++ app std C-library names on newlib

**Files.** `zephyr/cxx-compat/` (possible public opt-in header),
`book/src/getting-started/integration-zephyr.md`.

- [ ] Decide the nano-ros role: (a) export an opt-in `<nros/...>` std-C-lib
      compat header a consumer force-includes; (b) document the consumer-side
      recipe only (force-include / newlib `<cstdlib>` config); (c) nothing —
      pure consumer concern. Record the decision + rationale
- [ ] Implement the chosen option (header and/or docs)
- [ ] Note: the re-export must be valid under the actuation build flags — the
      naive `#include <stdlib.h>` + `using ::name;` failed in-TU during the ASI
      spike (the in-TU `<stdlib.h>` returned no decls); understand why before
      shipping a recommendation

### 184.D — FVP / full-C++ consumer smoke in CI

**Files.** `examples/` or `packages/testing/nros-smoke/` (new minimal FVP
C++ smoke), `just/zephyr.just`, `.github/workflows/`.

- [ ] Add a minimal `nros-cpp` + CycloneDDS C++ smoke targeting
      `fvp_baser_aemv8r_smp` with `CONFIG_NEWLIB_LIBC=y` + `CONFIG_GLIBCXX_LIBCPP=y`
- [ ] Wire it into the Zephyr CI cluster so 184.A/184.B regressions are caught
- [ ] Keep it build-only if FVP run-time is licence/SDK-gated (mirror Phase
      180 Twister `build_only`)

### 184.E — RMW migration guidance for downstream consumers

**Files.** `book/src/getting-started/integration-zephyr.md`,
`docs/reference/` (RMW backends).

- [ ] Document the dust-dds retirement (Phase 169): `CONFIG_NROS_RMW_DDS`
      removed; consumers move to `CONFIG_NROS_RMW_CYCLONEDDS` (or zenoh/xrce)
- [ ] Note the Cyclone-vs-zenoh transport implication (Cyclone RTPS/UDP pulls
      no mbedtls; a TCP-`NET_TCP_ISN_RFC6528` consumer must disable it)

## Acceptance

- The ASI `actuation_module` (or an equivalent in-tree FVP full-C++ smoke)
  compiles every nano-ros + CycloneDDS TU clean on `fvp_baser_aemv8r_smp` +
  newlib + libstdc++ — no cxx-compat redefinition, no `ip_mreqn` incomplete.
- 184.D CI lane builds that profile and stays green.
- A documented consumer story (184.C/184.E) for std-C-lib names + RMW choice.
- native_sim (Phase 180) profiles remain green — the guards are no-ops there.

## Notes

- Consumer = `autoware-safety-island` (ASI), `github.com/NEWSLabNTU/autoware-safety-island`,
  branch `nano-ros`. Consumes nano-ros via a west `import: false` leaf at
  `modules/nros` + `nros_generate_interfaces()` + the `nros-codegen-c` host
  build. FVP target `fvp_baser_aemv8r_smp`.
- This phase is maintained on a worktree branch (`phase-184-asi-fvp-integration`)
  kept rebased on `main`; ASI re-pins to the merged commits once they land on
  `main` and are pushed.
- The Autoware/Eigen `std::exit`/`std::rand` failures (184.C trigger) and the
  `net.c` `ip_mreqn` issue (184.B) are pre-existing relative to the bump —
  `net.c` was unchanged across the 610-commit range; they surfaced only
  because the 184.A fix let the build reach them.
- 184.A first landed as `fix/cxx-compat-libstdcpp-passthrough` and is
  cherry-picked onto this branch.
