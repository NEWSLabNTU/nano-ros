/* Issue 1569 — BUILDLESS sizes header. NOT a platform's fallback any more.
 *
 * This file used to be what every NuttX C/C++ image was sized from
 * (`nros_config_generated_nuttx.h`, dispatched under `NROS_PLATFORM_NUTTX`),
 * because the per-build header was on no NuttX include path. It was a
 * hand-kept UPPER BOUND, and it kept failing to be one: #167, #464, #954 and
 * issue 1568 each found a value below the build, the last as a live 36-byte
 * publisher overrun. It could not even be made right by hand — one file stood
 * for two architectures that measure differently, and `nros_subscription_t`
 * carries a field after its opaque array, so any value but the build's own
 * moves that field in C relative to the Rust that writes it.
 *
 * NuttX now compiles against the per-build header like every other platform
 * (the FFI build script takes it from the `nros-c` / `nros-cpp` `links`
 * channels, ordered after both writers). The stub reaches this file ONLY under
 * `NROS_CONFIG_BUILDLESS`, which the header-only checks that compile with no
 * build at all define (API parity, capability layout, the minimal-libcpp and
 * compat-shim probes). No build system may define it —
 * `check-config-fallback-macros` refuses one that does — so no image can be
 * sized from these numbers, and they need no relation to any build's: they
 * must only be self-consistent (the asserts below) and define every macro
 * generated code reads (the same gate). The values are the last snapshot's,
 * kept as they were. */
/* Phase 159 (Path C) — NuttX fallback.
 *
 * #167 — this fallback is a snapshot and MUST be a safe UPPER BOUND: when the
 * per-build `<nros/nros_cpp_config_generated.h>` mirror does not reach a TU that
 * emits `Node::GlobalStorageHolder<>::storage` (ODR-picked across the nros-cpp
 * library + entry), the C++ side falls back to these sizes. The old 79304 was
 * STALE — current codegen needs 80712 (rv-virt) / 79952 (arm-virt), so
 * `nros_cpp_init`'s Rust `open_in` wrote past the buffer and smashed a saved
 * return address (rv-virt boot panic EPC=0x4; arm's smaller overflow survived).
 * Keep this comfortably above the largest per-build value.
 */
#ifndef NROS_CPP_CONFIG_GENERATED_BUILDLESS_H
#define NROS_CPP_CONFIG_GENERATED_BUILDLESS_H
/* #464 postscript — this was 98304, i.e. `NROS_EXECUTOR_SIZE + 8`, which is the
 * PRE-issue-0436 overhead. The generator's invariant is
 *
 *     storage_bytes = EXECUTOR_SIZE + CPP_CONTEXT_OVERHEAD   (nros-build-helpers/src/cpp.rs)
 *
 * and 0436 raised that constant from 8 to 16 when it added the 8-byte handle
 * `tag` — the field whose entire job is turning a mixed-up executor handle into
 * a clean error instead of memory corruption. The snapshot kept the old delta,
 * so if this fallback ever fired the C++ side allocated 8 bytes too few and the
 * bytes it ran past were that tag. Same shape as the C twin's stale
 * EXECUTOR_OPAQUE_U64S: a hand-maintained number left behind by a fix that
 * moved the thing it was derived from. Asserted below. */
#define NROS_CPP_EXECUTOR_STORAGE_SIZE 98312
/* issue 0796 — was 80. `CppActionServer` gained the accepted-goal callback
 * slot (one function pointer), so every per-build value rose by one pointer
 * width: the host generator now emits 128 where it emitted 120. Raised by 8 to
 * keep this snapshot the UPPER BOUND its header comment requires. */
#define NROS_CPP_ACTION_SERVER_STORAGE_SIZE 88
#define NROS_CPP_ACTION_CLIENT_STORAGE_SIZE 48
#define NROS_EXECUTOR_SIZE 98296
#define NROS_GUARD_CONDITION_SIZE 24
/* Issue 1568 — MEASURED BELOW THE BUILD and raised: the realtime-c
 * qemu-armv7a-nuttx fixture built at this commit wrote a per-build header with
 * NROS_PUBLISHER_SIZE 596, NROS_SUBSCRIBER_SIZE 620, NROS_SERVICE_SERVER_SIZE
 * 568, raw subscription / service-server 209 / 199 u64s and PUBLISHER_OPAQUE_U64S
 * 75 — every one ABOVE this file's value, so each buffer sized from here was
 * short (the ctrl component's publisher buffer was 564 bytes for a 600-byte
 * write, overrunning into the telem instance the linker placed after it). The
 * values below are that measurement rounded up with margin, still hand-kept:
 * issue 1569 is the exact fix (the per-build header on the NuttX path). */
#define NROS_PUBLISHER_SIZE 640
#define NROS_SUBSCRIBER_SIZE 672
#define NROS_SERVICE_CLIENT_SIZE 4632
#define NROS_SERVICE_SERVER_SIZE 608
#define NROS_CPP_RAW_SUBSCRIPTION_OPAQUE_U64S 224
#define NROS_CPP_RAW_SERVICE_SERVER_OPAQUE_U64S 216
#define NROS_CPP_RAW_SERVICE_CLIENT_OPAQUE_U64S 707
/* #464 postscript — was 786, which is BELOW a real per-build value: a host
 * probe of the same type measures 799. This file's contract is "MUST be a safe
 * UPPER BOUND … above the largest per-build value", and 786 is not. Raised with
 * margin; an over-sized bound costs static RAM, an under-sized one is the
 * overflow #167 describes.
 *
 * Stated plainly: this is NOT verified against a 32-bit NuttX build, where the
 * true size is probably smaller than the host's. It is raised because the
 * contract is an upper bound over ALL per-build values and 786 demonstrably
 * failed that for one of them. */
#define NROS_CPP_RAW_ACTION_SERVER_OPAQUE_U64S 832
#define NROS_CPP_RAW_ACTION_CLIENT_OPAQUE_U64S 2193

/* RFC-0090 / issue 1115 — the codegen-version anchors, mirroring the per-build
 * `nros_cpp_config_generated.h` (`nros-build-helpers/src/cpp.rs`), which emits
 * the same pair from `nros_core::codegen_version`.
 *
 * A generated artifact reads these from the C header (see
 * `rosidl-codegen/packs/_codegen_version.jinja` — one header for both
 * languages, and it is the C one), so the C twin of this file is what unbroke
 * the NuttX build. They are here because this fallback stands in for a
 * per-build header that DOES define them, and a fallback that carries a
 * strict subset of its twin is the drift issue 1115 is: identical token
 * sequences, so a TU that reaches both headers sees a benign redefinition.
 *
 * EXACT values, not upper bounds — a version range has no bound to be safe on.
 * Gated by `check-config-fallback-macros`. */
#define NROS_CODEGEN_VERSION 9
#define NROS_CODEGEN_VERSION_MIN 2

/* Issue 0464 — the generator computes
 *   NROS_CPP_EXECUTOR_STORAGE_SIZE = NROS_EXECUTOR_SIZE + CPP_CONTEXT_OVERHEAD
 * with the overhead currently 16 (`nros-build-helpers/src/cpp.rs`). A snapshot
 * cannot observe the type it describes, but it CAN refuse to encode a
 * relationship the generator no longer holds — which is exactly how the old
 * `+ 8` survived issue 0436's bump.
 *
 * If the overhead changes again, this assertion fires at the including TU and
 * names the file to update, instead of the next reader discovering it from a
 * smashed handle tag. */
#define NROS__CPP_CONTEXT_OVERHEAD 16
#if (defined(__STDC_VERSION__) && __STDC_VERSION__ >= 201112L)
_Static_assert(NROS_CPP_EXECUTOR_STORAGE_SIZE >= NROS_EXECUTOR_SIZE + NROS__CPP_CONTEXT_OVERHEAD,
               "NROS_CPP_EXECUTOR_STORAGE_SIZE must cover NROS_EXECUTOR_SIZE + CppContext overhead");
#elif defined(__cplusplus) && __cplusplus >= 201103L
static_assert(NROS_CPP_EXECUTOR_STORAGE_SIZE >= NROS_EXECUTOR_SIZE + NROS__CPP_CONTEXT_OVERHEAD,
              "NROS_CPP_EXECUTOR_STORAGE_SIZE must cover NROS_EXECUTOR_SIZE + CppContext overhead");
#endif
#endif
