# Phase 11W — Cyclone DDS Zephyr nros-module runtime (native_sim path)

**Goal.** Bring the Cyclone DDS RMW backend up to runtime on
Zephyr `native_sim/native/64` so the Phase 168 collapsed examples
(`examples/zephyr/{c,cpp,rust}/<case>/`) build clean with
`prj-cyclonedds.conf` overlays.

**Status.** ✓ COMPLETE for compile + link + boot smoke **+ true
talker→listener pub/sub discovery (11W.12)**. Every collapsed case
(6 cases × 3 languages = 18 cells) builds clean on
`native_sim/native/64`; the Rust talker publishes std_msgs/Int32 at
1 Hz and a separate Rust listener now receives those samples over
Cyclone DDS SPDP multicast discovery — `test_zephyr_rust_cyclonedds_pubsub_e2e`
asserts `Received`. Wire-compat against a stock ROS 2 /
`rmw_cyclonedds_cpp` peer stays an open follow-up (Phase 117.X interop
track).

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

- **11W.8 status (2026-05-20):** Cyclonedds talker now boots
  through `Executor::open` → `dds_create_participant` →
  `ddsi_config_init` (full default-config walk, ~80 elements) →
  UDP transport factory. Stuck at `ddsi_udp_create_conn`'s
  `ddsrt_bind`. Direct probe (via `zsock_bind` from
  `vtable.cpp` register fn) confirms the failure is **not
  Cyclone-specific**: NSOS rejects `bind(AF_INET, 127.0.0.1, *)`
  with `errno=2` (ENOENT) while accepting `bind(AF_INET,
  0.0.0.0, *)`. Linux host bind of 127.0.0.1 succeeds outside
  Zephyr (Python sanity check), so this is a Zephyr NSOS path
  issue — possibly in `nsos_adapt_bind`'s `sockaddr_storage`
  zero-init, possibly in the family / port byte-order
  translation. Diagnosing further needs printk inside
  `zephyr-workspace/zephyr/drivers/net/nsos_adapt.c` — blocked
  by sandbox policy.
- **11W.8 options for resolution:**
  - **A.** Authorize Zephyr SDK edits via a workspace patch
    script (mirror the cyclonedds-zephyr-*-patch.sh pattern), add
    printk inside `nsos_adapt_bind` to capture the host-side
    errno before NSOS_MID translation, then root-cause from there.
  - **B.** Coerce Cyclone DDS to bind sockets to `0.0.0.0` (which
    NSOS accepts) while continuing to **advertise** 127.0.0.1 in
    the participant locator. Cyclone's XML config has
    `General/Interfaces/NetworkInterface` with an `autodetermine`
    attribute and an explicit `address` field — set the synthetic
    ifaddrs entry to `0.0.0.0` but configure Cyclone's locator
    advertisement to 127.0.0.1.
  - **C.** Skip native_sim runtime validation entirely and pin
    cyclonedds Zephyr coverage to FVP/aemv8r (see existing note
    above). Phase 11W's "compile + link + boot smoke" deliverable
    has already landed (Phase 11W.5 / .6 / .7).

- **11W.8 resolution (2026-05-20):** Took path **A** — the bind
  failure was NOT an NSOS bug. Root cause: the talker's NSOS board
  overlay (`boards/native_sim_native_64.conf`) is **not auto-applied
  when `-DCONF_FILE` is set explicitly**, so the cyclonedds build
  fell back to the zeth/TAP `eth_posix` driver. With no TAP device,
  `socket()` returned a native-stack fd whose `bind(127.0.0.1)`
  failed. Forcing the NSOS Kconfig directly in `prj-cyclonedds.conf`
  (`CONFIG_NET_SOCKETS_OFFLOAD=y +
  CONFIG_NET_NATIVE_OFFLOADED_SOCKETS=y + CONFIG_ETH_NATIVE_POSIX=n`)
  routes sockets through host BSD sockets, and bind works.

  With NSOS active the participant init then surfaced a cascade of
  NSOS feature gaps, each now patched:

  1. **`getsockname` missing entirely** — NSOS never populated the
     `socket_op_vtable.getsockname` slot, so Cyclone read back port 0
     after binding to an ephemeral port. Added top+bottom-half
     impl via `scripts/zephyr/nsos-getsockname-patch.sh`.
  2. **`getsockopt(SO_*BUF)` reports 0** — NSOS succeeds but returns
     a zero buffer size; Cyclone's min-size check then errored.
     Tolerate `actsize == 0` (`cyclonedds-zephyr-udp-rcvbuf-patch.sh`).
  3. **`IP_MULTICAST_{IF,TTL,LOOP}` setsockopt fail** — best-effort
     on Zephyr NSOS (struct-shape / size mismatches from upstream
     POSIX). Same patch makes the multicast TX-option block
     non-fatal.

  Init now drives all the way through `find_own_ip`, unicast +
  multicast socket creation, transmit-connection setup, and reaches
  the **SPDP multicast group join** (`joinleave_spdp_defmcip` →
  `add_locator_to_addrset`), where it still `abort()`s. This is the
  multicast-discovery wall: the synthetic loopback interface
  (127.0.0.1, from `link_stubs.c`'s `ddsrt_getifaddrs`) drives
  Cyclone down the ASM multicast-join path, and the
  `add_locator_to_addrset` / mreq handling does not survive the
  NSOS multicast plumbing (the existing nano-ros NSOS
  `IP_ADD_MEMBERSHIP` handler expects `struct ip_mreqn` (12 B) while
  Cyclone passes `struct ip_mreq` (8 B), among other shape gaps).

  **Remaining 11W.8 options for the multicast wall:**
  - Align the NSOS `IP_ADD_MEMBERSHIP` / `IP_MULTICAST_*` struct
    shapes with what Cyclone's `ddsi_udp.c` actually passes
    (`ip_mreq` 8 B, 1-byte TTL/LOOP), then debug
    `add_locator_to_addrset`.
  - Configure Cyclone for **unicast-only discovery**
    (`General/AllowMulticast=false` + `Discovery/Peers`) so the
    multicast join path is never taken — works for in-host
    talker↔listener but does not auto-discover LAN ROS 2 peers.
  - Accept native_sim cyclonedds as "init-progresses-far" and pin
    full runtime to FVP/aemv8r (path C).

  Landed this iteration (all via reproducible workspace patches,
  applied by `just zephyr setup`): NSOS getsockname, SO_*BUF
  actsize==0 tolerance, multicast TX best-effort, NSOS-forcing +
  arena bump in `prj-cyclonedds.conf`. The bind / getsockname /
  sockbuf gaps are fully resolved; only the multicast-RX join
  remains.

- **11W.8 continued (2026-05-20, second pass) — participant init now
  fully completes.** Pushed past the multicast-join wall and a
  cascade of further NSOS/Zephyr gaps; the Cyclone DDS participant
  now initialises end-to-end on native_sim. Fixes (each a
  reproducible workspace patch, wired into `just zephyr setup`):

  4. **pthread mutex/cond pool exhaustion** — Cyclone creates many
     mutexes during init; the 32-entry pool was exhausted and
     `pthread_mutex_lock` on the failed-init mutex aborted. Bumped
     `CONFIG_MAX_PTHREAD_MUTEX_COUNT` / `_COND_COUNT` to 256
     (`prj-cyclonedds.conf`).
  5. **Multicast join non-fatal** — `joinleave_spdp_defmcip` now
     returns success on Zephyr when the ASM join fails, so init
     continues unicast-only (`cyclonedds-zephyr-mcjoin-patch.sh`).
  6. **`sigprocmask` → `pthread_sigmask`** — Cyclone blocks signals
     around `pthread_create`; Zephyr asserts on `sigprocmask` in a
     multi-threaded context. Redirected via `#define` in the
     threads patch.
  7. **`pthread_create` EINVAL** — Cyclone worker threads (gc, recv,
     lease, tev, …) are created with no explicit stack attr; Zephyr
     needs the dynamic-thread stack pool. Enabled
     `CONFIG_DYNAMIC_THREAD` + `_ALLOC` + `THREAD_STACK_INFO` +
     `THREAD_MONITOR` + `DYNAMIC_THREAD_STACK_SIZE=32768`.
  8. **Socket-waitset self-pipe** — Cyclone's `make_pipe` uses
     `pipe(2)`, whose fds the NSOS socket-poll waitset can't watch,
     so `os_sockWaitsetTrigger` failed. Replaced (under `__ZEPHYR__`)
     with a loopback TCP socket pair — the same technique upstream
     uses on Windows (`cyclonedds-zephyr-sockwaitset-patch.sh`,
     needs `CONFIG_NET_TCP=y`).

  Init now drives all the way to **`create_publisher`**, which fails
  at `find_descriptor` → returns NULL for
  `std_msgs::msg::dds_::Int32_`. **This is the final, distinct
  blocker and a different problem class** — the Cyclone DDS C type
  descriptor for the message type is neither generated nor
  registered on the Rust + cyclonedds + Zephyr path:
  - The descriptor self-registration uses
    `__attribute__((constructor))`
    (`cmake/NrosRmwCycloneddsTypeSupport.cmake`), which does not run
    on Zephyr bare-metal (`target_os = "none"`, no `.init_array`
    invocation) — the same root cause as the RMW-register gap fixed
    in 11W.6.
  - More fundamentally, the Rust example's codegen produces Rust
    message types, not the Cyclone DDS C `dds_topic_descriptor_t`
    the backend's `find_descriptor` needs. Wiring Cyclone C
    type-support generation + explicit (non-constructor)
    registration into the Rust + cyclonedds Zephyr build is a
    codegen-pipeline task, tracked as **Phase 11W.9**.

  **Net 11W.8 result:** every NSOS / Zephyr-runtime gap in the
  Cyclone DDS participant-init path is resolved; the participant
  boots and initialises fully on native_sim. The remaining work is
  type-support codegen, not runtime/transport.

- **11W.9 (2026-05-20) — cyclonedds Rust talker PUBLISHES on
  native_sim.** Resolved the type-descriptor gap. The Cyclone DDS C
  `dds_topic_descriptor_t` for `std_msgs/Int32` is now generated +
  compiled into the talker and the publisher creates successfully;
  the talker emits `Published: N` in a loop.

  - `examples/zephyr/rust/talker/CMakeLists.txt` (cyclonedds branch)
    generates the descriptor from `std_msgs/Int32.msg` via
    `nros_rmw_cyclonedds_generate_from_msg`, pointing
    `IDLC_EXECUTABLE` at the host-built
    `build/cyclonedds/bin/idlc` and `PKG_DIR` at
    `/opt/ros/humble/share/std_msgs`, then adds the generated `.c`
    to the app.
  - `NrosRmwCycloneddsTypeSupport.cmake` guard relaxed: accept a
    pre-set `IDLC_EXECUTABLE` (embedded direct-compile build) in
    addition to the `CycloneDDS::ddsc` imported target (POSIX
    find_package build).
  - **The `__attribute__((constructor))` self-registration DOES run
    on native_sim** (it links as a host binary, so the host C
    runtime executes constructors) — Task "explicit registration"
    proved unnecessary. (Real Cortex-M targets may still need an
    explicit path; revisit when running cyclonedds on hardware.)

  **Open follow-up — Phase 11W.10 (runtime stability):** after
  ~16k publishes the process `abort()`s. Investigated 2026-05-20;
  two distinct layers, the second being the real blocker:

  1. **Main-thread publish free-run + writer-history OOM.** On the
     no_std Zephyr path the executor's timer delta falls back to
     crediting `spin_once`'s `timeout_ms` every call (no
     `clock_us_fn` set in the cyclonedds `ExecutorConfig`), and the
     poll-only cyclonedds `session_drive_io` returns instantly, so
     the 1 Hz timer fires hundreds of times/second. With a reliable
     writer and no reader, the writer-history cache grows until the
     4 MB heap is exhausted → bare `ddsrt_malloc` `abort()`.
     *Mitigable* — pacing `session_drive_io` (k_msleep timeout_ms)
     and/or setting `clock_us_fn` fixes the delta; bounding writer
     QoS avoids the OOM. But pacing the main thread exposes layer 2.

  2. **NSOS UDP recv busy-spin (the real blocker).** The Cyclone
     recv thread spins logging
     `UDP recvmsg sock N: ret 0 retcode -1` millions of times: NSOS
     `recvmsg` on a UDP socket returns 0 (not `-1`/`EWOULDBLOCK`)
     while the socket-poll waitset keeps reporting the fd readable,
     so `ddsi_udp_conn_read` treats it as an error and the recv
     loop never blocks. On single-core native_sim this starves the
     publish thread. Reproduces on both the unicast (sock 4) and
     multicast (sock 5) recv sockets, and persists with multicast
     fully disabled (`AllowMulticast=false`), so it is **not** the
     failed multicast join — it is a Zephyr NSOS `recvmsg`/`poll`
     semantics bug for UDP (poll signals readable, recvmsg yields
     0, condition never clears). Fixing it needs NSOS-level work in
     `zephyr/drivers/net/nsos_{sockets,adapt}.c` (return
     `EWOULDBLOCK` for no-data, or make `poll` not over-report) —
     beyond the in-repo cyclonedds patches and partly sandbox-gated.

  The `6864b2550` state (talker publishes ~16k visible
  `Published: N` lines, then OOM) is kept as the demonstrable
  milestone; the experimental `drive_io`-pacing / `AllowMulticast`
  /off changes were reverted because pacing the main thread just
  surfaces the layer-2 recv-spin (no visible publishes). 11W.10
  proper is gated on the NSOS UDP-recv fix.

- **11W.10 RESOLVED (2026-05-20) — clean 1 Hz publishing.** Root
  cause of layer 2 found: `nsos_recvmsg` in
  `zephyr/drivers/net/nsos_sockets.c` was an
  `errno = ENOTSUP; return -1;` stub — NSOS never implemented
  recvmsg, but Cyclone's UDP read uses it, so every receive failed
  (ENOTSUP → DDS_RETCODE_ERROR, the `retcode -1` in the spam) and
  the recv thread busy-spun. Two fixes:
  1. **Implement NSOS recvmsg** (`nsos-recvmsg-patch.sh`): delegate
     the single-iovec form (Cyclone uses one iovec + msg_name) to
     the existing `nsos_recvfrom` path, reusing its poll/block +
     sockaddr translation. The recv thread now blocks for data
     instead of spinning.
  2. **Pace `session_drive_io`** (k_msleep `timeout_ms` on Zephyr):
     the executor's "wait for events" primitive now actually waits,
     so the no_std timer-delta credit matches wall-clock, the 1 Hz
     timer fires once per second, the native_sim clock advances, and
     the writer-history cache no longer grows unbounded.

  Result: the talker publishes at a steady 1 Hz indefinitely —
  `Published: 0` @ 1.1 s, `Published: 12` @ 14.3 s, zero recvmsg
  spam, no abort. `test_zephyr_rust_talker_cyclonedds_boot` still
  passes. **cyclonedds Rust talker now runs stably on Zephyr
  native_sim end to end.**

- **11W.11 (2026-05-20) — discovery: mreq fixed, blocked on real
  interface.** Pursued true talker↔listener discovery. The SPDP
  multicast join (`IP_ADD_MEMBERSHIP`) was failing with two causes:
  1. **mreq struct size (FIXED, `nsos-mcjoin-mreq-patch.sh`).** The
     NSOS `IP_ADD_MEMBERSHIP` handler (from
     `native-sim-ipproto-ip-patch.sh`) hard-coded
     `optlen != sizeof(struct ip_mreqn)` (12 B) → EINVAL. Cyclone
     passes `struct ip_mreq` (8 B, via the `zephyr_ipv4_compat.h`
     shim). Both share the same first 8 bytes (multiaddr +
     interface-IP); the patch reads the two leading `in_addr`s and
     accepts either size. Confirmed the 8-byte mreq now reaches the
     host setsockopt (`mreqsz=8`).
  2. **Loopback interface can't join multicast (OPEN).** With the
     mreq fix the host `setsockopt(IP_ADD_MEMBERSHIP,
     group=239.255.0.1, interface=127.0.0.1)` still returns -1:
     Linux can't join an arbitrary multicast group on the loopback
     interface. The synthetic `ddsrt_getifaddrs` returns `127.0.0.1`
     (`link_stubs.c`) — fine for unicast bind, not for a multicast
     join. ddsi_ownip rejects `0.0.0.0` (unspecified), so a real,
     multicast-capable host interface address is required.

  **Path to discovery (Phase 11W.12):** implement a real
  `ddsrt_getifaddrs` backed by an NSOS `getifaddrs` (host
  `getifaddrs()` trampoline — a new NSOS feature, like getsockname /
  recvmsg) so Cyclone selects the host's actual primary interface;
  the multicast join then lands on a real interface and two
  native_sim processes discover each other. Alternative: unicast
  peer discovery (`Discovery/Peers` + NSOS `getaddrinfo`), needing
  deterministic per-participant ports and a working `getaddrinfo`.

  Current state: the mc-join-best-effort patch (11W.8) keeps the
  failed join non-fatal, so talker (publishes) and listener
  (subscribes) each run cleanly; they just don't discover each
  other yet. The mreq optlen fix lands regardless — a correct NSOS
  bug fix independent of the interface question.

  **Remaining for full E2E:** the build hardcodes
  `/opt/ros/humble/share/std_msgs` + `build/cyclonedds/bin/idlc`
  (acceptable: the cyclonedds Zephyr path already needs ROS +
  Cyclone host tools for codegen, but should be generalised to
  resolve via the project's setup paths). Generalising descriptor
  generation across all collapsed C/C++/Rust cyclonedds examples
  (not just the talker) is the broader 11W.9 follow-up.

  **Phase 11W.12 — RESOLVED (discovery works).** Talker→listener
  pub/sub now completes on native_sim NSOS. The full multicast path
  needed three more pieces beyond the 11W.8 mreq optlen fix:

  1. **Real multicast interface** — `scripts/zephyr/nsos-getifaddrs-patch.sh`
     adds a host `getifaddrs()` trampoline (`nsos_adapt_getifaddrs`
     → `struct nsos_mid_ifaddr`) that returns the first UP +
     MULTICAST + non-loopback AF_INET host interface.
     `link_stubs.c`'s `ddsrt_getifaddrs` now reports that real
     address (e.g. 192.168.x.x) instead of 127.0.0.1, so the join
     lands on a multicast-capable interface and `ddsi_ownip` accepts
     it.
  2. **Host-side IPPROTO_IP forwarder** — the guest half
     (`native-sim-ipproto-ip-patch.sh`, `nsos_sockets.c`) marshalled
     `IP_ADD_MEMBERSHIP` / `IP_MULTICAST_*` but the *bottom* half
     (`nsos_adapt.c`) had no `case NSOS_MID_IPPROTO_IP`, so the
     midplane returned `-NSOS_MID_EOPNOTSUPP` and the join never
     reached the host kernel. `scripts/zephyr/nsos-adapt-ipproto-ip-patch.sh`
     adds that handler (reconstructs host `struct ip_mreq` /
     `in_addr` / int and calls the host `setsockopt`). With it the
     join succeeds (no more `error in join`).
  3. **Distinct GUID prefix per process** — native_sim's test
     entropy ("not safe - entropy source") is deterministic, so two
     copies of the same binary generated *identical* Cyclone
     participant GUID prefixes (same participant handle, same thread
     IDs across processes). Each then treated the other's SPDP
     announcement as its own and discovery never matched. Passing a
     unique `--seed` per process fixes it; `ZephyrProcess::start`
     already injects a time-+counter seed per spawn, so the E2E test
     gets distinct GUIDs for free.

  Verified: `test_zephyr_rust_cyclonedds_pubsub_e2e` (listener +
  talker, distinct seeds) — listener logs `Received[N]:` for the
  talker's published sequence. Manual 30 s run delivered 24/24
  samples. Patches are wired into `just zephyr setup` (idempotent)
  after the mcjoin-mreq patch.

  **C / C++ parity.** The C and C++ cyclonedds overlays
  (`examples/zephyr/{c,cpp}/{talker,listener}/prj-cyclonedds.conf`)
  only carried the Phase-117 link-time config — they lacked the 11W.8
  runtime knobs, so the talker hit `CODE_UNREACHABLE` in picolibc
  libc-hooks (16 KiB malloc arena) and `Cannot create zeth` (no NSOS
  forcing) before ever publishing. Brought to parity with the Rust
  overlay (16 MiB `COMMON_LIBC_MALLOC_ARENA_SIZE`, NSOS offload
  forcing, NET_TCP, bigger pthread pools, diagnostics) and added the
  Cyclone C descriptor generation to the C and C++ CMake (the C/C++
  codegen emits the message types but not Cyclone's descriptor; the
  backend's `find_descriptor` needs the C `dds_topic_descriptor_t`).
  The C app links the C++ cyclonedds backend via the module (overlay
  keeps `CONFIG_CPP=y`). `test_zephyr_{c,cpp}_cyclonedds_pubsub_e2e`
  both pass — listener receives the talker's samples in all three
  languages (Rust + C + C++).

  **Services (Rust).** `service-server`/`service-client` got the same
  overlay parity + Cyclone C descriptor generation, but from
  `example_interfaces/srv/AddTwoInts.srv` (the converter emits the
  `_Request_` + `_Response_` structs). Surfaced + fixed a backend bug:
  `service_type_name` concatenated `<base> + _Request_`, but the nros
  codegen emits `SERVICE_NAME` with a trailing underscore
  (`<pkg>::srv::dds_::<Svc>_`, mirroring the message `<Type>_`
  convention), so the lookup became `<Svc>__Request_` (double `_`) and
  missed the registered `<Svc>_Request_`. `service_type_name` now
  strips one trailing `_` from the base, matching both the registered
  descriptor and stock `rmw_cyclonedds_cpp` (no-op when the base has no
  trailing `_`, so the backend's own roundtrip tests still pass). With
  it `test_zephyr_rust_cyclonedds_service_e2e` passes — client gets
  `Response: sum=` over the request/response roundtrip.

  **Services (C++).** Same overlay parity + srv descriptor generation;
  `test_zephyr_cpp_cyclonedds_service_e2e` passes (client logs 4/4
  `[OK]` calls). The backend `service_type_name` fix covers C++ too.

  **Services (C) — open (data-plane bug, not timing).** With the C
  overlay + srv descriptor generation, both C endpoints create cleanly
  and discovery completes — gating the client on
  `nros_client_wait_for_service` returns OK immediately (server visible)
  — yet `nros_client_call` still times out (`NROS_RET_TIMEOUT`, -2) and
  the server never logs handling the request. So this is *not* a
  discovery-timing issue (the wait-for-service gate did not help); the
  request→reply roundtrip itself fails for the C path while the C++ and
  Rust paths (identical backend `service_*` slots) succeed. C client and
  C server are self-consistent on topic (`/add_two_ints`) and type
  (`example_interfaces::srv::dds_::AddTwoInts_` → backend strips the
  trailing `_`), so the divergence is somewhere in the C-codegen request
  serialization / `nros_client_call` capture path vs the C++ `fut.wait`
  path. Needs focused backend tracing — deferred. Tracked alongside
  all-language **action** examples (actions also need `.action`
  decomposition in the IDL converter — `msg_to_cyclone_idl.py` handles
  only `.msg`/`.srv`).

  Investigation leads (for the next pass):
  - Generated type strings are *identical* across C and C++
    (`example_interfaces::srv::dds_::AddTwoInts_` + `_Request_` /
    `_Response_`), so it is not a type-name divergence.
  - `nros_executor_register_service` (nros-c) routes straight to
    nros-node's `register_service_raw_sized{,_on}` with the same
    `service_name` / `type_name` / `type_hash` the Rust and C++ paths
    use — no registration divergence found. The fault is therefore most
    likely in nros-c's *runtime* request take / reply send during
    `nros_executor_spin_period`, not in setup.
  - Localization method: run a cross-language pair (C++ client → C
    server, then C client → C++ server) on domain 0 — the working C++
    endpoint isolates whether the C *server* receive or the C *client*
    send is at fault. This was blocked in one pass by flaky
    background-process execution under native_sim (0-byte captured
    output for `&`-launched processes); re-run when the host is stable.

  Follow-ups: upstream the NSOS host patches (getifaddrs +
  adapt-side IPPROTO_IP, alongside the earlier getsockname /
  recvmsg) to Zephyr; the determinism gotcha is native_sim-specific
  (real hardware / separate hosts differ naturally).
- Phase 117's "follow-ups (post-117)" list is **separate**: 11X
  autoware, 11Y Phase 108 events, 11Z zero-copy sertype. 11W
  here is yet another post-117 follow-up.
