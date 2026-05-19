# Phase 11W — Cyclone DDS Zephyr nros-module runtime (native_sim path)

**Goal.** Bring the Cyclone DDS RMW backend up to runtime on
Zephyr `native_sim/native/64` so the Phase 168 collapsed examples
(`examples/zephyr/{c,cpp,rust}/<case>/`) build clean with
`prj-cyclonedds.conf` overlays.

**Status.** Not Started — gaps catalogued below during the
Phase 168.X.fvp investigation.

**Priority.** P2 — gates the cyclonedds runtime row of the
Phase 168 collapse matrix (~18 native_sim binaries: 6 cases ×
3 languages). The aemv8r FVP reference path
(`examples/zephyr/cpp/cyclonedds/talker-aemv8r/`) is unaffected
and continues to validate the RMW backend end-to-end on Phase 117
acceptance terms.

**Depends on.** Phase 117 (Cyclone DDS RMW — COMPLETE),
Phase 168.X.fvp (llext-edk gen-expr fix — COMPLETE),
Phase 169.5 (`nros-rmw-cyclonedds-sys` Rust shim — COMPLETE).

---

## Background

Phase 117.14 explicitly deferred cyclonedds-on-Zephyr-nros-module:

> "The example builds against the existing Zephyr DDS backend
> (`CONFIG_NROS_RMW_DDS=y` — dust-dds) since the Zephyr nros
> module hasn't yet been extended to ship a Cyclone build path;
> once that lands the user flips the Kconfig symbol with no
> source changes."

Phase 168.X gap 2.B then dropped the `NROS_CPP_API` requirement
from `NROS_RMW_CYCLONEDDS` in `zephyr/Kconfig` and added the
C-API + Rust strong-stub emission paths. Phase 168.X.fvp landed
the llext-edk gen-expr fix that unblocked the configure phase on
host-gcc / native_sim. The remaining gaps are Cyclone DDS C
source compile issues that surface once configure succeeds.

aemv8r doesn't hit any of these because its toolchain
(`zephyr-sdk/aarch64-zephyr-elf-gcc`) is built with relaxed
language defaults vs `host-gcc`'s strict `-std=c11` flow on
native_sim.

---

## Gap 1 — `struct ip_mreqn` redefinition (partially landed)

**Symptom.**

```
zephyr/include/zephyr/net/socket.h:1252:8: error: redefinition of 'struct ip_mreqn'
note: previous definition is here:
zephyr/cyclonedds-config/zephyr_ipv4_compat.h:29:8: 'struct ip_mreqn'
```

`zephyr/cyclonedds-config/zephyr_ipv4_compat.h` is force-included
on every C/CXX TU via `zephyr_compile_options(SHELL:-include …)`
in the `CONFIG_NROS_RMW_CYCLONEDDS` branch of
`zephyr/CMakeLists.txt`. The shim was written against Zephyr
≤3.5 (no `ip_mreqn` definition). Zephyr 3.7 LTS's
`<zephyr/net/socket.h>` ships its own `struct ip_mreqn`, so the
shim's redefinition collides.

**Resolution (landed during Phase 168.X.fvp investigation).**

`zephyr/cyclonedds-config/zephyr_ipv4_compat.h` now `#include`s
`<zephyr/net/socket.h>` first (pulling Zephyr's `ip_mreqn`) and
keeps only `struct ip_mreq` (which Zephyr 3.7 doesn't ship).
Compile-path: green past this point.

## Gap 2 — Cyclone DDS atomics use `asm volatile` under `-std=c11`

**Symptom.**

```
third-party/dds/cyclonedds/src/ddsrt/include/dds/ddsrt/atomics/gcc.h:292:3:
  error: 'asm' undeclared (first use in this function)
  292 |   asm volatile ("" ::: "memory");
      |   ^~~
```

Three occurrences in
`third-party/dds/cyclonedds/src/ddsrt/include/dds/ddsrt/atomics/gcc.h`
(`ddsrt_atomic_fence_acq`, `ddsrt_atomic_fence_rel`, plus one
seq-cst). All emit `asm volatile ("" ::: "memory")` — GCC's
classic compiler-only memory fence.

Zephyr 3.7 host-gcc passes `-std=c11` by default. Strict ISO C11
reserves the `asm` keyword to implementations — GCC honors
strict mode by accepting only `__asm__`. The
`zephyr-sdk/x86_64-zephyr-elf-gcc` toolchain compiled for
aemv8r doesn't hit this because its default flow includes
`-fgnu-keywords` (or equivalent) so `asm` resolves.

**Fix sketch (minimum-touch).**

Inside `zephyr/CMakeLists.txt :: CONFIG_NROS_RMW_CYCLONEDDS`
branch, before `zephyr_library_sources(${_cdds_ddsrt_top} …)`:

```cmake
# Phase 11W — Cyclone DDS uses `asm volatile` (GCC extension)
# in ddsrt/atomics/gcc.h. Zephyr's default -std=c11 rejects the
# unprefixed `asm` keyword on strict toolchains (host-gcc on
# native_sim). Re-enable GCC keyword recognition for Cyclone's
# C TUs only.
zephyr_compile_options($<$<COMPILE_LANGUAGE:C>:-fgnu-keywords>)
```

Affects every C TU under the cyclonedds branch (acceptable —
none of Cyclone's own code uses ISO-strict `asm` reservation).
Doesn't leak into example user code (each example sets its own
compile options via its `CMakeLists.txt`).

**Alternative (upstream):** patch the three call sites in
`ddsrt/atomics/gcc.h` to use `__asm__` instead of `asm`. Upstream
PR; backport burden each Cyclone bump.

## Gap 3 — Possibly-more cyclonedds TU compile issues (unknown)

The Phase 168.X.fvp investigation stopped at Gap 2; further TU
compile failures may exist further into the cyclonedds source
tree (~1268 ninja steps total, last seen successful step was
~955 before Gap 1 fired). Known categories that have hit
embedded targets in past Cyclone bring-ups:

- **`fcntl.h` / `unistd.h` posix-only symbol references** —
  some `ddsrt/src/*/posix/*.c` TUs reference functions Zephyr
  declares but doesn't fully implement. Drop or stub per the
  existing pattern at
  `zephyr/CMakeLists.txt :: list(REMOVE_ITEM _cdds_ddsrt_posix …)`.
- **`sa_data` vs `data` in `struct sockaddr`** — Zephyr renames
  this field. Phase 117 already drops `ddsi_vnet.c` for this
  reason; other TUs may need the same treatment.
- **`pthread_setname_np`, `getifaddrs`, IGMP join helpers** —
  none of these have direct Zephyr equivalents on native_sim.
  Stub or implement in `zephyr/cyclonedds-zephyr/*.c`.

Resolve incrementally: apply Gap 2 fix, retest, drop/stub each
new failure until link succeeds.

## Gap 4 — Cyclone DDS runtime on native_sim NSOS

Even if all compile issues clear, runtime success depends on
Zephyr's `NET_SOCKETS_OFFLOAD` (NSOS) layer forwarding the
specific BSD socket calls Cyclone's RTPS engine makes. Known
unknowns:

- **`setsockopt(IP_ADD_MEMBERSHIP)`** — Cyclone calls this for
  SPDP multicast joins. The Phase 97.4 NSOS IPPROTO_IP patch
  (`scripts/zephyr/native-sim-ipproto-ip-patch.sh`) was written
  for dust-dds and should already cover the common path; verify
  Cyclone's exact `ip_mreq`-shaped argument lands on the same
  NSOS forwarding slot.
- **`recvmsg` / `MSG_DONTWAIT`** — Cyclone's RX thread uses
  these. NSOS's recvmsg path may not forward all flags.
- **Multicast loopback** — required for in-process pub/sub
  smoke tests on a single host.

Runtime validation: extend `phase_118_collapse` smokes to launch
the cyclonedds binary under timeout and assert at least one
log line emitted before the timeout (matches the existing
zenoh / xrce smoke pattern). Deferred until compile-path is
green.

---

## Work items

- [ ] **11W.1 — Gap 2 `-fgnu-keywords` patch.** Add the one-line
       `zephyr_compile_options` inside `CONFIG_NROS_RMW_CYCLONEDDS`
       branch. Retest `west build -b native_sim/native/64 …
       prj-cyclonedds.conf` on `examples/zephyr/c/talker/`;
       confirm builds advance past `ddsrt/src/*.c`.
- [ ] **11W.2 — Gap 3 iterative TU fixes.** Each new compile
       failure → drop / stub per existing pattern. Land via
       additional entries in the
       `list(REMOVE_ITEM _cdds_ddsrt_posix …)` or new files
       under `zephyr/cyclonedds-zephyr/`.
- [ ] **11W.3 — Verify link.** All three languages
       (`examples/zephyr/{c,cpp,rust}/<case>/`) × at least one
       case (`talker`) link clean. Capture artefact size +
       compare against the aemv8r equivalent.
- [ ] **11W.4 — Runtime smoke.** Extend
       `phase_118_collapse::test_zephyr_{rust,cmake}_case_rmw_variant_exists`
       with cyclonedds rows. Add a separate ctest that boots the
       binary under timeout and asserts a log line.
- [ ] **11W.5 — Just / build-fixtures.** Add cyclonedds entries
       to `just/zephyr.just :: build-fixtures` (12 cells: 6 cases
       × 2 languages — Rust async-service-client excluded; cpp
       cyclonedds-aemv8r already present).

## Acceptance

- [ ] `examples/zephyr/c/<case>/` cyclonedds builds + links on
       `native_sim/native/64`.
- [ ] `examples/zephyr/cpp/<case>/` cyclonedds builds + links.
- [ ] `examples/zephyr/rust/<case>/` cyclonedds builds + links
       (uses Phase 169.5 `nros-rmw-cyclonedds-sys` shim).
- [ ] cyclonedds smoke binary on native_sim emits at least one
       Cyclone DDS log line before timeout (process-level
       smoke).
- [ ] No regression on aemv8r cyclonedds path
       (`examples/zephyr/cpp/cyclonedds/talker-aemv8r/`).
- [ ] No regression on Phase 168 zenoh + xrce collapse (37 / 37
       smokes still pass).

## Files (when 11W lands)

- `zephyr/CMakeLists.txt` — `zephyr_compile_options(-fgnu-keywords)`
  + any additional `list(REMOVE_ITEM …)` from Gap 3 iteration.
- `zephyr/cyclonedds-zephyr/*.c` — new stub TUs for
  Gap 3-uncovered missing symbols.
- `zephyr/cyclonedds-config/dds/config.h` — possible
  `DDSRT_HAVE_*` toggles for missing Zephyr-side support.
- `just/zephyr.just :: build-fixtures` — 12+ cyclonedds entries.
- `packages/testing/nros-tests/tests/phase_118_collapse.rs` —
  cyclonedds smoke rows.

## Notes

- The Phase 117 cyclonedds backend itself is COMPLETE and
  validated on POSIX + FVP/aemv8r. 11W is strictly about the
  Zephyr nros-module's compile-path integration — making the
  same `libnros_rmw_cyclonedds.a` link cleanly under
  `host-gcc/native_sim/native/64` build context.
- Possible alternative if 11W stalls on Gap 4 (NSOS limits):
  declare native_sim cyclonedds **unsupported** and pin runtime
  validation to FVP/aemv8r + POSIX (Phase 117.12 ros2 E2E). The
  collapse-shape contribution is still useful — users get a
  drop-in cyclonedds-overlay shape across all three languages,
  even if native_sim only validates configure + link.
- Phase 117's "follow-ups (post-117)" list is **separate**: 11X
  autoware, 11Y Phase 108 events, 11Z zero-copy sertype. 11W
  here is yet another post-117 follow-up.
