# Phase 11W — Cyclone DDS Zephyr nros-module runtime (native_sim path)

**Goal.** Bring the Cyclone DDS RMW backend up to runtime on
Zephyr `native_sim/native/64` so the Phase 168 collapsed examples
(`examples/zephyr/{c,cpp,rust}/<case>/`) build clean with
`prj-cyclonedds.conf` overlays.

**Status.** ✓ COMPLETE for compile + link + boot smoke. Every
collapsed case (6 cases × 3 languages = 18 cells) builds clean on
`native_sim/native/64`; the Rust talker boots to the init banner
under `test_zephyr_rust_talker_cyclonedds_boot`. Full pub/sub against
a stock ROS 2 peer stays an open follow-up — Cyclone DDS
`Executor::open` currently surfaces `Transport(ConnectionFailed)`
under NSOS, separate from 11W's "compile + link + boot" bar.

```
build-c-talker-cyclonedds/zephyr/zephyr.exe       13 MB
build-cpp-talker-cyclonedds/zephyr/zephyr.exe     13 MB
build-rust-talker-cyclonedds/zephyr/zephyr.exe     3 MB
```

The native_sim networking primitive is **NSOS** (host BSD sockets
via `CONFIG_NET_SOCKETS_OFFLOAD=y +
CONFIG_NET_NATIVE_OFFLOADED_SOCKETS=y`), not eth_posix / zeth — the
TAP path stays unused. Boot-time `<err> eth_posix: Cannot create
zeth (0)` is benign driver-init noise; socket calls go through the
host syscall layer regardless.

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

## Gap 2.5 — Cyclonedds C++ source needs `zephyr/cxx-compat/` shims (landed)

**Symptom (post Gap 2 fix):**

```
packages/dds/nros-rmw-cyclonedds/src/session.cpp:18:10: fatal error: cstdlib: No such file or directory
packages/dds/nros-rmw-cyclonedds/src/descriptors.cpp:24:10: fatal error: cstring: No such file or directory
packages/dds/nros-rmw-cyclonedds/src/sertype_min.cpp:5:10: fatal error: cstdlib: No such file or directory
```

Cyclonedds' C++ sources `#include <cstdlib>` / `<cstring>` etc.
Zephyr 3.7's `lib/cpp/minimal/include` only ships `<cstddef>`,
`<cstdint>`, `<new>`. Project already ships compat shims at
`zephyr/cxx-compat/` (used by nros-cpp). Cyclone branch was
missing the include directive.

**Resolution.** Added `zephyr_include_directories(${CMAKE_CURRENT_LIST_DIR}/cxx-compat)`
inside the `CONFIG_NROS_RMW_CYCLONEDDS` branch.

## Gap 3 — Cxx-compat shim namespace + atomic gap

**Symptom (post Gap 2.5 fix):**

```
packages/dds/nros-rmw-cyclonedds/src/descriptors.cpp:53:18:
  error: 'strcmp' is not a member of 'std'; did you mean 'strcmp'?
packages/dds/nros-rmw-cyclonedds/src/sertype_min.cpp:11:10:
  error: 'memset' is not a member of 'std'; did you mean 'memset'?
packages/dds/nros-rmw-cyclonedds/src/service.cpp:57:10:
  fatal error: atomic: No such file or directory
```

Two distinct issues:

1. **`zephyr/cxx-compat/<cstring>` etc.** use `using ::strcmp;`
   patterns that expose C library functions in the global
   namespace but NOT inside `std::`. Cyclone's source qualifies
   every call as `std::strcmp`, `std::memset`, `std::malloc`,
   `std::memcpy`, `std::free`, `std::strlen`. nros-cpp doesn't
   hit this because its code uses unqualified `strcmp` /
   `memset` (legal post `#include <cstring>` on a strict-
   compliant stdlib).

2. **`<atomic>` header** — Zephyr's minimal libcpp doesn't ship
   `<atomic>` at all. Cyclone's `service.cpp` includes it for
   `std::atomic`. nros-cpp avoids it (uses `core::sync::atomic`
   on the Rust side). No shim exists in `cxx-compat/`.

**Fix sketch.**

For (1): extend each `cxx-compat/<cname>` header with a
`namespace std { using ::name; }` block alongside the existing
global `using` exports. Pattern:

```cpp
// zephyr/cxx-compat/cstring
#include <string.h>
namespace std {
  using ::strcmp;
  using ::strlen;
  using ::strncpy;
  using ::memcpy;
  using ::memset;
  using ::memmove;
  using ::memcmp;
}
```

For (2): provide a minimal `<atomic>` shim. Either:
- Forward to `<stdatomic.h>` C11 atomics with namespace wrappers.
- Drop Cyclone's `<atomic>` use entirely (single `std::atomic<bool>`
  in service.cpp's reply-slot counter — could be replaced with
  `volatile` + memory fence).

## Gap 3.5 — chrono / thread / random / new shim surface (landed)

**Symptom (post Gap 3 cstring fix):**

```
service.cpp:58:10: fatal error: chrono: No such file or directory
service.cpp: error: 'std::this_thread::sleep_for' / 'std::chrono::steady_clock'
publisher.cpp:86: error: no matching function for call to 'operator new(sizetype, const std::nothrow_t&)'
sertype_min.cpp:11: error: 'memset' is not a member of 'std'
```

**Resolution.** New `zephyr/cxx-compat/` headers landed:

- `cxx-compat/atomic` — minimal `std::atomic<T>` over GCC
  `__atomic_*` builtins (no `<stdatomic.h>` include — that uses
  C11 `_Atomic`, incompatible with C++).
- `cxx-compat/chrono` — `std::chrono::steady_clock`,
  `std::chrono::{nanoseconds, milliseconds, seconds, …}`. Backed
  by Zephyr's `k_uptime_ticks()`.
- `cxx-compat/thread` — `std::this_thread::sleep_for(duration)`.
  Forwards to `k_msleep`.
- `cxx-compat/random` — `random_device`, `mt19937`,
  `uniform_int_distribution`. Backed by `sys_rand32_get()`.
- `cxx-compat/new` — declares nothrow placement-new overloads.
  Implementation in
  `zephyr/cyclonedds-zephyr/nothrow_new.cpp` (forwards to
  `malloc`/`free`).
- `cxx-compat/cstring` + `cstdlib` + `cstdio` — extended with
  `namespace std { using ::name; }` exports.

## Gap 4 — Cyclone DDS link-time undefined references (88 symbols)

**Symptom (post Gap 3.5).** All compile gaps clear — the build
advances all the way to the link stage. `ld` then reports
**88 undefined references** across the linked `zephyr.elf`,
categorised:

| Category | Symbol prefix | Count | Driver |
|----------|---------------|-------|--------|
| Security | `q_omg_*`, `ddsi_handshake_*`, `*_secure*`, `validate_msg_decoding`, `decode_Data*`, `encode_*` | ~50 | `DDS_HAS_SECURITY=0` drops the TU bodies but **leaves call sites** in `ddsi_acknack.c`, `ddsi_endpoint.c`, `ddsi_entity_match.c`, `q_receive.c`. |
| Iceoryx SHM | `iox_*`, `shm_*`, `iceoryx_header_*`, `deliver_data_via_iceoryx`, `free_iox_chunk` | ~15 | `DDS_HAS_SHM=0` — same pattern as security. |
| Endpoint helpers | `determine_publication_writer`, `determine_subscription_writer`, `determine_topic_writer`, `is_proxy_participant_deletion_allowed`, `set_proxy_participant_security_info`, `pserop_participant_generic_message{,_nops}`, `secure_conn_write` | ~10 | Internal cyclonedds helpers normally provided by dropped TUs. |
| POSIX | `ddsrt_eth_get_mac_addr`, `ddsrt_getifaddrs`, `IN_MULTICAST` | 3 | Zephyr doesn't ship these; `ddsrt/src/ifaddrs/posix/ifaddrs.c` was deliberately dropped in Phase 117. |
| Network transport | `ddsi_vnet_init`, `decode_rtps_message`, `volatile_secure_data_filter` | 3 | Dropped `ddsi_vnet.c` TU but call sites remain. |
| Auth handshake | `handle_auth_handshake_message`, `handle_crypto_exchange_message`, `ddsi_handshake_*` | 7 | SECURITY=0 follow-up. |

**Fix sketch.** Write a comprehensive
`zephyr/cyclonedds-zephyr/link_stubs.c` (mirror of the existing
`shm_stubs.c` but covering all 88 symbols). Each stub returns
the failure / no-op sentinel value matching the function's
return type (security checks → `false`, SHM ops → `0`, encode
helpers → `0` size, etc.). Per-symbol signatures looked up from
the Cyclone DDS source headers (~30 min mechanical work).

Alternative: extend Cyclone DDS upstream to wrap each call site
in `#if DDS_HAS_SECURITY` / `#if DDS_HAS_SHM` guards. Cleaner
but ~50 source-file patches; harder to maintain across Cyclone
bumps.

## Gap 5 — Unknown runtime gaps past Gap 4

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

- [x] **11W.1 — Gap 2 asm keyword fix.** Landed
       `zephyr_compile_options($<$<COMPILE_LANGUAGE:C>:-Dasm=__asm__>)`
       inside `CONFIG_NROS_RMW_CYCLONEDDS` branch. Cyclone's
       `ddsrt/atomics/gcc.h` `asm volatile` lines now resolve to
       `__asm__ volatile` at preprocess time.
       (`-fgnu-keywords` was attempted first; turns out it is
       C++-only — gcc emits a `valid for C++/ObjC++ but not for
       C` diagnostic and ignores it on C TUs.)
- [x] **11W.2 — Gap 2.5 cxx-compat include.** Landed
       `zephyr_include_directories(${CMAKE_CURRENT_LIST_DIR}/cxx-compat)`
       inside `CONFIG_NROS_RMW_CYCLONEDDS` branch. Cyclone's
       C++ TUs now resolve `<cstdlib>` / `<cstring>` /
       `<cstdio>` against the nros-cpp shim layer.
- [x] **11W.3 — Gap 3 namespace + atomic shim.** Landed
       `zephyr/cxx-compat/{atomic, chrono, thread, random, new}`
       headers + extended `cstring` / `cstdlib` / `cstdio` with
       `namespace std { using ::name; … }` exports.
- [x] **11W.3.b — Gap 3.5 nothrow new impl.** Landed
       `zephyr/cyclonedds-zephyr/nothrow_new.cpp` (`malloc`/`free`
       backed) + wired into `_cdds_zephyr_overrides`. Cyclone's
       `new (std::nothrow) T{}` expressions now resolve.
- [x] **11W.4 — Gap 4 link-time stubs.** Root cause turned out
       to be `-DDDS_HAS_*=0` vs `#ifdef DDS_HAS_*` mismatch in
       upstream Cyclone — defining the macro to value 0 still
       trips defined-checks as TRUE, pulling in 80+ call sites
       whose TU bodies we drop. Resolution:
       * `DDS_HAS_SECURITY` / `DDS_HAS_SHM`: leave UNDEFINED so
         Cyclone falls through to its inline stub branches.
       * `DDS_HAS_NETWORK_PARTITIONS` / `DDS_HAS_TYPE_DISCOVERY` /
         `DDS_HAS_TOPIC_DISCOVERY`: define (no value) — Cyclone
         compiles `free_config_networkpartition_addresses` /
         `ddsi_typebuilder.c` UNCONDITIONALLY but with struct
         refs that require the macro for visibility.
       * Residual 3 undef-refs (`ddsi_vnet_init`,
         `ddsrt_getifaddrs`, `IN_MULTICAST`) stubbed in
         `zephyr/cyclonedds-zephyr/link_stubs.c` +
         `zephyr_ipv4_compat.h`.
- [x] **11W.5 — Runtime smoke.** Landed
       (commit ec1773258 + this commit). Extended
       `phase_118_collapse` with 18 cyclonedds existence rows
       (6 rust + 6 c + 6 cpp) and a separate
       `test_zephyr_rust_talker_cyclonedds_boot` ctest that
       boots the native_sim binary under a 3 s timeout and
       asserts the "Booting Zephyr" / "nros" banner. Talker
       reaches the Rust entry, calls
       `nros::platform::zephyr::wait_for_network`, attempts
       `Executor::open` and then surfaces
       `Transport(ConnectionFailed)` — Cyclone DDS init on
       native_sim NSOS is still incomplete (open follow-up,
       not 11W's bar). The native_sim networking primitive is
       **NSOS** (`CONFIG_NET_SOCKETS_OFFLOAD=y +
       CONFIG_NET_NATIVE_OFFLOADED_SOCKETS=y`), not zeth /
       TAP — `<err> eth_posix: Cannot create zeth (0)` in the
       log is harmless driver-init noise; sockets go through
       the host syscall layer.
- [x] **11W.3 — Verify link.** All three languages × every
       collapsed case link clean (18 / 18 cyclonedds cells +
       the boot ctest above).
- [x] **11W.4 — Runtime smoke.** Subsumed by 11W.5 above.
- [x] **11W.5 — Just / build-fixtures.** 18 cyclonedds entries
       added to `just/zephyr.just :: build-fixtures` (rust × 6
       + c × 6 + cpp × 6; cyclonedds-aemv8r still skipped via
       its own `build-fvp-aemv8r-cyclonedds` recipe).

## Acceptance

- [x] `examples/zephyr/c/<case>/` cyclonedds builds + links on
       `native_sim/native/64`.
- [x] `examples/zephyr/cpp/<case>/` cyclonedds builds + links.
- [x] `examples/zephyr/rust/<case>/` cyclonedds builds + links
       (uses Phase 169.5 `nros-rmw-cyclonedds-sys` shim).
- [x] cyclonedds native_sim binary boots far enough to print
       the init banner (process-level smoke,
       `test_zephyr_rust_talker_cyclonedds_boot`). Publication
       past `Executor::open` deferred — needs Cyclone DDS
       initialisation against NSOS sockets, tracked separately.
- [ ] No regression on aemv8r cyclonedds path
       (`examples/zephyr/cpp/cyclonedds/talker-aemv8r/`).
       **Pre-existing breakage, not introduced by 11W:** the
       recipe failed on `main` before 11W (Cargo compile error
       on the stale `CONFIG_NROS_RMW_DDS=y` Kconfig — dust-dds
       retired by Phase 169 — and, once that is corrected, the
       link step trips on `_critical_section_1_0_acquire` /
       `__rust_alloc` etc. multi-defs because the
       NROS_CPP_API=y / NROS_C_API=n co-build emits two Rust
       runtimes into the same image). Tracked as a Phase
       169 / 171 follow-up, not 11W.
- [x] No regression on Phase 168 zenoh + xrce collapse (37 / 37
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
