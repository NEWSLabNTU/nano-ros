# Phase 190 - nano-ros consumable by a full-C++ app on the FVP / newlib profile

**Goal.** A downstream Zephyr application that links a full C++ stack
(Autoware control + Eigen) against nano-ros's `nros-cpp` API + CycloneDDS
RMW builds clean — and eventually boots — on the **Autoware safety-island**
profile: board `fvp_baser_aemv8r_smp`, Zephyr 3.7 LTS, `CONFIG_NEWLIB_LIBC=y`
+ `CONFIG_GLIBCXX_LIBCPP=y`. Phase 180 made nano-ros a consumable Zephyr
module and proved it on `native_sim`; Phase 190 closes the gaps that only
surface on the FVP + newlib + full-libstdc++ + real-downstream-app profile.

**Status.** In progress (2026-05-28). Surfaced by the autoware-safety-island
(ASI) west-pin bump `70ab6227d → be4c51364` (610 commits). 190.A landed
(cxx-compat passthrough guard) + 190.B landed (libc-gated multicast struct,
`net.c.obj` verified) + 190.C landed (re-export shims `cstdlib`/`cstdio`/
`cstring`/`utility`/`cstdarg`/`cstddef`/`cstdint` defer to real libstdc++).
**Deep-validated end-to-end: the full ASI actuation_module COMPILES + LINKS to
`zephyr.elf` against bumped nano-ros** (190.A–G; 52 MB, 0 undefined refs).
Runtime (boot + DDS) needs the ARM FVP simulator, deferred. 190.D landed the
full-libstdc++ FVP build guard (overlay + recipe; CI-job wiring is a follow-up
since no FVP CI lane exists). 190.E (RMW migration docs) open; 190.F optional
RMW-gate-out open.

**Realignment — Zephyr 3.7 floor (2026-05-29, rebased onto `6777a0551`).**
Phase 199 (archived) set a hard **Zephyr 3.7 floor**: support is bounded by
`zephyr-lang-rust` (born after 3.7.0), so the Rust staticlib cannot link below
3.7 — the doc names ASI explicitly ("must be ≥ 3.7"). **This moots the 3.5.99
premise** under which 190.A–H were first validated: that number was the
Corellium AVH *default sample*, not a supported target. Consequences:
- **190.F obsolete** — its `KERNEL_VERSION < 0x030600` flat-`net_if_addr`
  branch only mattered ≤ 3.5; at the 3.7 floor `unicast[i].ipv4` is always
  present, and main's net-wait (relocated to the platform layer in 200.1)
  already uses it unconditionally. Dropped in the rebase.
- **190.H solved by `nros setup`** — Phase 187 landed `nros setup`; the
  `nros-sdk-index.toml` Cyclone host-tools entry builds `idlc`
  (`-DBUILD_IDLC=ON`). ASI runs `nros setup` instead of hand-building idlc.
- **190.A/B/C/G still valid** on 3.7 — they key off libc/libstdc++ presence
  (full-libstdc++, newlib `ip_mreq`, weak POSIX stubs), not the Zephyr version.
- **Re-validation owed:** the COMPILE+LINK milestone was on 3.5.0; redo on a
  real Zephyr 3.7 (`NROS_ZEPHYR_VERSION=3.7`). ARM-virtual-platform runtime =
  build `fvp_baser_aemv8r` on 3.7, load onto the Corellium FVP (the 3.5.0
  sample is just replaced).

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
   `fvp_baser_aemv8r_smp` + newlib + libstdc++, so 190.A/190.B-class
   regressions land silently.

Out of scope: the Autoware control stack's own picolibc/newlib portability
(it is ASI-vendored). 190.C delivers the nano-ros-side enablement + docs;
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
  (force-include / Kconfig). Decided in 190.C.

- **An FVP-profile consumer smoke** added to the Zephyr CI cluster, mirroring
  ASI's shape (nros-cpp + CycloneDDS, full libstdc++) at minimal size.

## Work Items

### 190.A — cxx-compat shims defer to a real libstdc++

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

### 190.B — portable multicast join on newlib (`ip_mreq` vs `ip_mreqn`)

**Files.** `packages/core/nros-platform-zephyr/src/net.c`,
`zephyr/cyclonedds-config/zephyr_ipv4_compat.h`.

- [x] Reproduce: `IP_ADD_MEMBERSHIP` defined + `struct ip_mreqn` incomplete on
      `fvp_baser_aemv8r_smp` + `CONFIG_NEWLIB_LIBC=y` (root: newlib provides
      `ip_mreq`, not the Linux `ip_mreqn`)
- [x] Switch the join/leave path to a libc-gated membership struct
      (`nros_mcast_membership_t`: `struct ip_mreq` + `imr_interface` on
      `CONFIG_NEWLIB_LIBC`, `struct ip_mreqn` + `imr_address` otherwise);
      Zephyr's IP_ADD_MEMBERSHIP accepts both the 8B/12B forms. Verified:
      `net.c.obj` compiles clean on the FVP newlib profile (was an `ip_mreqn`
      incomplete-type error)
- [x] `zephyr_ipv4_compat.h` is Cyclone-TU-only (force-included on Cyclone DDS
      TUs, not net.c); its Cyclone TUs compiled clean on the FVP build, so its
      `ip_mreq`/`ip_mreqn` handling is unaffected. Refreshed its stale "≥3.7
      defines `ip_mreqn`" comment to note the newlib nuance
- [x] native_sim path unaffected by construction — only the
      `CONFIG_NEWLIB_LIBC` branch changed; minimal/picolibc targets keep the
      existing `ip_mreqn` + `imr_address` path the Phase 180 native_sim runs proved

### 190.C — downstream C++ app std C-library names on newlib

**Files.** `zephyr/cxx-compat/{cstdlib,cstdio,cstring}`.

Root cause (resolved): NOT a missing consumer header — the same shim-vs-real-
libstdc++ issue as 190.A. The cxx-compat dir is on every consumer TU's include
path; a TU's `#include <stdlib.h>` resolves through libstdc++'s `<stdlib.h>`
wrapper → `<cstdlib>` → the cxx-compat `cstdlib` shim. The shim's
`#include <stdlib.h>` then hits the wrapper's already-set include guard, so the
C declarations are absent and `using ::abort;`/`::rand;`/… fail
("'abort' has not been declared in '::'"). That is exactly why the spike's
naive force-include also failed. So the consumer needs no header at all once
the shim is transparent.

- [x] Decision: option (c)+fix — nano-ros makes the cxx-compat dir fully
      transparent on full-libstdc++ profiles; no public opt-in header, no
      consumer force-include. Consistent with 190.A
- [x] Guard the re-export shims (`cstdlib`, `cstdio`, `cstring`) with
      `#if __has_include(<bits/c++config.h>)` → `#include_next` the real header;
      keep the minimal shim for picolibc / minimal-libcpp (probe absent)
- [x] Verified: `std::rand`/`std::exit`/`std::abs`/`std::memcpy` compile under
      the exact autoware-TU flags (`compile_commands.json`) with cxx-compat on
      the path — previously every `using ::name` errored. Fixes both the
      Eigen `std::rand` and the Autoware `std::exit` classes with no autoware
      source edits
- [x] Guard the remaining cxx-compat shims the same way: `utility` (defining —
      `std::remove_reference`/`std::move`, collided with libstdc++ `<utility>`/
      `<type_traits>`) + the `cstdarg`/`cstddef`/`cstdint` re-exports. The whole
      cxx-compat dir is now transparent on full-libstdc++ profiles
- [x] Deep-validate: full ASI actuation build now compiles the entire
      autoware/Eigen/Cyclone C++ stack ([116/123], past all CXX TUs). Remaining
      blocker is unrelated (zpico-zephyr net_if API, 190.F)

### 190.D — FVP / full-C++ consumer smoke in CI

**Files.** `examples/zephyr/cpp/cyclonedds/talker-aemv8r/full-libcpp.conf`
(new overlay), `just/zephyr.just` (`build-fvp-aemv8r-cyclonedds-full-libcpp`),
`.github/workflows/` (CI-job wiring — follow-up).

Insight: the existing `talker-aemv8r` example + its `build-fvp-aemv8r-cyclonedds`
recipe build on the FVP board with Zephyr's **minimal libcpp**, which never
touches the cxx-compat-vs-real-libstdc++ passthrough (190.A/C) — so it could
never catch the FVP-consumer regressions. Building the SAME example with the
full-libstdc++ profile (`CONFIG_NEWLIB_LIBC` + `CONFIG_GLIBCXX_LIBCPP`, what a
real downstream C++ app uses) does: nros-cpp + Cyclone TUs pull
`<memory>`/`<atomic>`/`<utility>`/… (190.A/C), net.c multicast (190.B),
zpico net_if (190.F), Cyclone ddsrt POSIX (190.G).

- [x] Add the `full-libcpp.conf` overlay (`CONFIG_NEWLIB_LIBC` +
      `CONFIG_GLIBCXX_LIBCPP`) + the `build-fvp-aemv8r-cyclonedds-full-libcpp`
      recipe (build-only; self-skips without workspace/SDK like its sibling)
- [x] Validated by analogy: the ASI `actuation_module` — the identical profile
      (fvp_baser_aemv8r + newlib + GLIBCXX_LIBCPP + Cyclone + nros-cpp), a strict
      superset of this guard — COMPILES + LINKS with 190.A–G. The guard is the
      lightweight in-tree equivalent
- [ ] CI-job wiring: neither this nor the pre-existing minimal
      `build-fvp-aemv8r-cyclonedds` is in any aggregate (`build-fixtures` /
      `ci`) or GitHub workflow — there is no FVP CI lane yet. Adding one (build
      the guard against nano-ros's own Zephyr where the example `prj.conf` is
      valid) is the remaining step. NB: the example `prj.conf` targets
      nano-ros's Zephyr; it does NOT build against ASI's older 3.5.99 pin
      (`POSIX_THREAD_THREADS_MAX` undefined, `NET_TCP_ISN_RFC6528`→mbedtls) —
      an orthogonal example-vs-ASI-Zephyr-version mismatch, not a 190 fix gap

### 190.E — RMW migration guidance for downstream consumers

**Files.** `book/src/getting-started/integration-zephyr.md`,
`docs/reference/` (RMW backends).

- [ ] Document the dust-dds retirement (Phase 169): `CONFIG_NROS_RMW_DDS`
      removed; consumers move to `CONFIG_NROS_RMW_CYCLONEDDS` (or zenoh/xrce)
- [ ] Note the Cyclone-vs-zenoh transport implication (Cyclone RTPS/UDP pulls
      no mbedtls; a TCP-`NET_TCP_ISN_RFC6528` consumer must disable it)

### 190.F — zpico-zephyr net_if IPv4 API version-spanning (3.7 vs 4.x)

**Files.** `packages/zpico/zpico-zephyr/src/zpico_zephyr.c`.

`nros_platform_net_wait_ready` reads `ipv4->unicast[i].ipv4.is_used` — the
`struct net_if_addr_ipv4` wrapper form, which Zephyr added in 4.x. On the ASI
Zephyr 3.7.0 LTS pin `unicast[]` is `struct net_if_addr` directly (no `.ipv4`
sub-struct, `struct net_if_addr_ipv4` does not exist), so the TU fails to
compile (`'struct net_if_addr' has no member named 'ipv4'`). Surfaced only
after 190.A–C let the build reach the nros library TUs. Note this is the
zenoh-pico glue, compiled even for a Cyclone-only build.

- [x] Reproduce: `unicast[i].ipv4` on `fvp_baser_aemv8r_smp` + the ASI pin
      (`net_if.h`: `struct net_if_addr unicast[NET_IF_MAX_IPV4_ADDR]`; the pin
      reports `KERNEL_VERSION_NUMBER 0x30563` = 3.5.99, pre the 3.6 wrapper)
- [x] Version-gate the unicast access at the wrapper's 3.6 introduction
      (`KERNEL_VERSION_NUMBER >= 0x030600` → `.ipv4`, else flat `net_if_addr`).
      `KERNEL_VERSION_NUMBER` comes via a robust dual include
      (`__has_include(<zephyr/version.h>)` 4.x layout, else bare `<version.h>`
      generated layout, else fall back to flat). Verified: `zpico_zephyr.c`
      compiles on the FVP profile; the build now reaches the final link
- [ ] (Optional) gate zpico-zephyr out of the build when the selected RMW is
      not zenoh, so a Cyclone-only consumer never compiles the zenoh glue
- [~] FVP build now compiles every TU; final link blocked on Cyclone ddsrt
      POSIX symbols → 190.G

### 190.G — Cyclone ddsrt POSIX link gaps on the FVP profile

**Files.** ASI `prj_actuation.conf` (POSIX Kconfig), possibly
`packages/dds/nros-rmw-cyclonedds` / `zephyr/cyclonedds-zephyr/` (ddsrt
stubs), Cyclone `src/ddsrt/src/{sockets,threads}/posix/`.

After 190.A–F the FVP actuation build compiles every TU and reaches the final
link, which then fails on undefined references from Cyclone's POSIX ddsrt
backend: `recvmsg` (`ddsrt/src/sockets/posix/socket.c`),
`pthread_attr_setscope` / `pthread_attr_setinheritsched` / `pthread_sigmask`
(`ddsrt/src/threads/posix/threads.c`), and newlib's `_open` syscall stub. The
FVP Zephyr POSIX layer does not provide these out of the box. This is the
Cyclone-on-Zephyr bring-up for the FVP aarch64-r profile (cf. Phase 177
embedded-Cyclone for FreeRTOS/ThreadX, Phase 180 native_sim).

- [x] Triage: all are nano-ros-side (Kconfig can't help — the symbols don't
      exist in this Zephyr). `recvmsg`: Cyclone's own single-iovec shim is
      gated `#if LWIP_SOCKET`, off on Zephyr. `pthread_attr_setscope`/
      `setinheritsched`/`pthread_sigmask`: absent from Zephyr 3.5.99's pthread.
      `_open`: Zephyr's libc-hooks gates `_open`/`_read`/`_write`
      `#ifndef CONFIG_POSIX_API`, but this profile sets `CONFIG_POSIX_API=y`, so
      they're compiled out while the SDK libc.a's `_open_r` still needs `_open`
- [x] Provide them as **weak** symbols in a new
      `zephyr/cyclonedds-zephyr/posix_compat_zephyr.c` (wired into the module
      source list): `recvmsg` → `recvfrom` single-iovec (identical to Cyclone's
      lwIP shim, matching its `msg_iovlen==1` assert), pthread attr/sigmask
      no-ops (Zephyr threads are system-scope, no signal delivery), `_open`
      `-1/ENOSYS` (Cyclone has `DDSRT_HAVE_FILESYSTEM=0`). Weak so a 4.x with
      real impls wins
- [x] Links `zephyr.elf` on `fvp_baser_aemv8r_smp` (52 MB, RAM 3.55 MB / 128 MB,
      0 undefined refs) — full ASI actuation_module now COMPILES + LINKS against
      bumped nano-ros
- [ ] Runtime validation (boot + DDS data plane) needs the ARM FVP simulator
      (licence/SDK-gated) — deferred, not exercisable in this environment

### 190.H — host `idlc` provisioning for the Zephyr consumer (post-Phase-186)

**Files.** ASI `build.sh` / `scripts/bootstrap-nano-ros-shim.sh` (consumer
side), `zephyr/CMakeLists.txt` (the `IDLC_EXECUTABLE` default).

Surfaced re-bumping ASI past Phase 186 (CycloneDDS self-provisioning in CMake).
The embedded Zephyr Cyclone build now runs `idlc` to generate the graph types
(`nros_rmw_cyclonedds_idlc_compile` over `rmw_dds_common_graph.idl`). There is
no `CycloneDDS::ddsc` target on embedded, so `zephyr/CMakeLists.txt` pre-sets
`IDLC_EXECUTABLE` to `${NROS_REPO_DIR}/build/install/bin/idlc` (a host idlc
built by `just cyclonedds setup`), overridable via `-DIDLC_EXECUTABLE`. A leaf
consumer (ASI) runs neither, so the build fails:
`/.../build/install/bin/idlc: not found` (exit 127). (Also a softer warning:
`msg_to_cyclone_idl.py not found` — only bites if `generate_from_msg()` is used.)

- [x] Diagnose: post-186 graph-types idlc step needs a host idlc; ASI supplies
      none → falls back to the empty `build/install/bin/idlc`
- [ ] ASI: build a host idlc from `modules/nros/third-party/dds/cyclonedds`
      (mirror the `nros-codegen` host build in `bootstrap-nano-ros-shim.sh`) and
      pass `-DIDLC_EXECUTABLE=<path>` (+ `-DNROS_RMW_CYCLONEDDS_MSG_TO_IDL` /
      `NROS_RMW_CYCLONEDDS_SCRIPTS_DIR` if msg→IDL codegen is added) from
      `build.sh`, alongside the existing `-D_NANO_ROS_CODEGEN_TOOL`
- [ ] Re-verify the ASI actuation build compiles + links on the bumped pin
- [ ] (nano-ros) consider documenting the `-DIDLC_EXECUTABLE` contract for
      downstream Zephyr consumers in the integration guide (190.E neighbour)

### 190.I — collapse the Zephyr board Cargo crates (version-spanning)

**Files.** `packages/boards/nros-board-fvp-aemv8r-smp`,
`packages/boards/nros-board-s32z270dc2-r52`, `examples/zephyr/.../*-aemv8r/`,
`integrations/zephyr/`, `docs/reference/zephyr-armv8r-setup.md`.

**Decision (2026-05-28).** Per `docs/design/board-bsp-integration-architecture.md`
(feature/phase-172): *"Generic board crate not needed for Zephyr; Zephyr owns
the board contract via DTS. Layer 2 collapses into Layer 3."* The two remaining
Zephyr board **Cargo** crates (`nros-board-fvp-aemv8r-smp`,
`nros-board-s32z270dc2-r52`) are the legacy Phase-117 per-board model and couple
a board to a pinned Zephyr — a new Zephyr forces a crate rebuild. Retire them:
the board is a Zephyr **DTS** board (vendor/upstream-owned), consumed via the
west module; the single version-coupled touchpoint becomes
`nros-platform-zephyr` (gated by `KERNEL_VERSION_NUMBER`, the 190.B/190.F
pattern). New Zephyr ⇒ at most a version gate in `nros-platform-zephyr`, no
board-crate churn.

- [ ] Move each crate's `boards/*.conf` + `*.overlay` to the example/west-module
      consumption path (DTS overlay + `prj.conf`), delete the Cargo crate +
      its `src/{config,node,lib}.rs`
- [ ] Drop the two crates from the workspace `Cargo.toml`; update
      `book/src/concepts/platform-model.md` + `zephyr-armv8r-setup.md`
- [ ] Confirm the FVP / s32z examples still build via the west module on ≥2
      Zephyr versions (3.5.x FVP + 4.x), proving the version decoupling
- [ ] Note: this is phase-172 (board-BSP) territory; tracked here because the
      FVP bump surfaced the coupling. Coordinate if 172 lands first.
- [ ] **Reinforced by phase-172 (`7b6ab0a1b`, largely landed).** 172 retired the
      per-package `system nros.toml` triple/board reader and moved board/triple
      into the root `nros.toml [system].target` (board is config-driven via
      `Config::from_toml`, *not* a `profile()` fn). And `nros deploy zephyr-mod`
      already drives a real `west` cross-build (the Layer-3 path) that consumes
      Zephyr via the module, no board Cargo crate. So `nros-board-fvp-aemv8r-smp`
      is even more clearly vestigial — the collapse aligns with 172's deploy
      model, not just the board-BSP doc.

### 190.J — FVP run/debug: standalone `fvp/` scripts + doc

**Files.** `fvp/start-fvp-vpn.sh`, `fvp/stop-fvp-vpn.sh` (in the ASI repo),
`docs/reference/zephyr-armv8r-setup.md` §6.

**Decision (2026-05-28).** Keep the FVP run/debug flow as standalone helper
scripts + a doc recipe, NOT wired into the `nros` CLI yet (revisit when the
Phase-172 `nros run`/deploy model lands). FVP simulator stays BYO/licence-gated
(Phase 187).

- [x] Route-safe VPN scripts (`--route-nopull` + single device-subnet route;
      host default route untouched) — validated against Corellium AVH:
      `fvp_baser_aemv8r`, Zephyr 3.5.x, device `10.11.1.8`, gdb `:4000` +
      console `:2000` reachable, default route intact
- [x] Documented the run recipe (VPN + `gdb-multiarch`/`lldb` + `nc` console +
      image-load) in `zephyr-armv8r-setup.md` §6
- [ ] (later) fold into Phase-172 `nros deploy fvp-aemv8r` when that lands.
      **Phase-172 update (`7b6ab0a1b`):** `nros deploy zephyr-mod` already does a
      real `west` cross-build + boot — but **native_sim only**; it builds+boots,
      it does NOT load onto real hardware / an FVP. The gdb-`:4000`-load +
      console-`:2000` flow here is exactly that missing half. A future
      `nros deploy fvp-aemv8r` = 172's deploy build + this load/monitor glue.
- [ ] **W.5 caveat for FVP data-plane validation.** Phase-172's orchestration
      component model is declarative-only until W.5 (callbacks wired as noop,
      `ComponentPublisher` zero-sized → a deployed *orchestration* binary emits
      no data). So a Published/Received demo on the FVP must use **direct-mode**
      (hand-written `main()` via the entry lib, which carries real logic) or
      wait for W.5 — the deploy/transport path itself is not the blocker.

## Acceptance

- The ASI `actuation_module` (or an equivalent in-tree FVP full-C++ smoke)
  compiles every nano-ros + CycloneDDS TU clean on `fvp_baser_aemv8r_smp` +
  newlib + libstdc++ — no cxx-compat redefinition, no `ip_mreqn` incomplete.
- 190.D CI lane builds that profile and stays green.
- A documented consumer story (190.C/190.E) for std-C-lib names + RMW choice.
- native_sim (Phase 180) profiles remain green — the guards are no-ops there.

## Notes

- Consumer = `autoware-safety-island` (ASI), `github.com/NEWSLabNTU/autoware-safety-island`,
  branch `nano-ros`. Consumes nano-ros via a west `import: false` leaf at
  `modules/nros` + `nros_generate_interfaces()` + the `nros-codegen-c` host
  build. FVP target `fvp_baser_aemv8r_smp`.
- This phase is maintained on a worktree branch (`phase-190-asi-fvp-integration`)
  kept rebased on `main`; ASI re-pins to the merged commits once they land on
  `main` and are pushed.
- The Autoware/Eigen `std::exit`/`std::rand` failures (190.C trigger) and the
  `net.c` `ip_mreqn` issue (190.B) are pre-existing relative to the bump —
  `net.c` was unchanged across the 610-commit range; they surfaced only
  because the 190.A fix let the build reach them.
- 190.A first landed as `fix/cxx-compat-libstdcpp-passthrough` and is
  cherry-picked onto this branch.
