/*
 * nros_rtos_run_components.c — the C-ABI single-executor entry, shared by
 * EVERY RTOS board.
 *
 * phase-432 W3.1. This is the C twin of `nros::board::<Rtos>Board::
 * run_components` in `<nros/main.hpp>`, and it exists so a C-only consumer —
 * a certified C compiler, MISRA-style, no C++ runtime — can boot a nano-ros
 * image without a C++ toolchain in the build.
 *
 * ## Why ONE file and ONE symbol, where `run_tiers` has three
 *
 * `run_tiers` is genuinely per-RTOS: it spawns a task per tier, and a
 * FreeRTOS task, a Zephyr `k_thread` and a NuttX pthread are three different
 * things. `run_components` is the SINGLE-EXECUTOR path — open, set up, spin,
 * shut down, all on the caller's thread — and there is nothing in it that
 * differs by kernel except the per-tick yield. Three copies of it would have
 * been three chances for the copies to drift, which is the defect class this
 * phase exists to remove, and the C++ side already agrees: `run_components`
 * is written ONCE per board there only because each board is a class, while
 * the loop under it (`detail::component_spin_loop`) is one function.
 *
 * So the symbol is `nros_board_rtos_run_components`, not one name per board.
 * `native` keeps its own (`nros_board_native_run_components_named`, Rust)
 * because it has a genuinely different signature: a host process resolves its
 * locator and domain at run time, so it takes neither.
 *
 * ## Why this is new code rather than a re-declaration
 *
 * The phase's first assessment said declaring `nros_board_<rtos>_run_components`
 * was not new capability, because `nros_board_freertos_run_tiers` is 666 lines
 * of C. That inference does not hold, and the correction is worth keeping here
 * rather than only in the roadmap: `run_tiers` and `run_components` are two
 * different architectures. The tiers path IS C underneath a four-line C++
 * veneer; `FreertosBoard::run_components` is fully implemented in the C++
 * header with no C function beneath it, and the one C-ABI `run_components`
 * that does exist — native's — is Rust. This is also the DOMINANT embedded
 * path: `run_tiers` is reached only when a plan declares tiers.
 *
 * ## What made it possible
 *
 * The C++ body uses no C++ feature a C function lacks: no exceptions, no RAII,
 * and its `template <typename Setup>` is only ever instantiated with a plain
 * function pointer at every generated call site. What it DID use, and what
 * blocked this for two attempts, is `nros::ok()` — the spin loop's exit
 * condition, reached through `detail::component_spin_loop`. That was
 * `Node::global_initialized()`, a C++ template static emitted by the header
 * and COMDAT-collapsed across TUs, so it could not be reached from C and could
 * not be relocated either.
 *
 * It is `nros_cpp_context_is_live(storage)` now: the context's own tag is the
 * flag, `nros_cpp_init*` stamps it and `nros_cpp_fini` clears it. That change
 * also removed a duplicate — the C++ `initialized` bool was a SECOND answer
 * maintained by hand at four sites, and a direct `nros_cpp_fini` (which
 * `freertos_run_tiers.c` does) tore the context down without touching it, so
 * `ok()` kept saying yes over a dropped executor.
 *
 * `nros_board_network_wait()` is the other thing that had to move: it was a
 * weak symbol in the C++ header, so a pure C entry failed at link. It lives in
 * `<nros/main.h>` now — one definition, both languages reach it.
 *
 * ## Why the spin loop is written again here rather than called
 *
 * `detail::component_spin_loop` cannot be called from C for the same reason
 * `ok()` could not: it is a header-emitted C++ inline. This is the one place
 * the two languages restate each other, and it is deliberately the SMALLEST
 * possible restatement — the loop and its two exit conditions — rather than a
 * per-board copy of the whole entry.
 *
 * ## Scope
 *
 * The benefit is RMW-CONDITIONAL and this file does not pretend otherwise: it
 * drops the C++ toolchain requirement for zenoh and XRCE, and NOT for
 * cyclonedds or uORB, whose RMW libraries are themselves C++. The acceptance
 * fixture is pinned to zenoh for exactly that reason.
 */

#include <stddef.h>
#include <stdint.h>
#include <string.h>

/* ---- nros-cpp CFFI (Rust) ----
 *
 * Declared rather than included, matching `freertos_run_tiers.c`: this file is
 * compiled by `build.rs` before cmake runs, so the generated
 * `nros_cpp_ffi.h` is not on the include path. Signatures must match it
 * exactly — `check-ffi-struct-mirrors` covers the structs, and these are
 * scalar-only. */
extern int nros_cpp_init(const char* locator, uint8_t domain_id, const char* node_name,
                         const char* namespace_, void* storage);
extern int nros_cpp_fini(void* storage);
extern int nros_cpp_spin_once(void* handle, int32_t timeout_ms);
extern int nros_cpp_spin_for(void* handle, uint32_t duration_ms, int32_t poll_ms);

/* phase-432 W3.1 — the exit condition, and the whole reason this file can
 * exist. `true` while the context at `storage` holds a live executor. */
extern _Bool nros_cpp_context_is_live(const void* storage);

/* Weak no-op in <nros/main.h>; a board with a slower link overrides it. */
extern void nros_board_network_wait(void);

/* RFC-0034 — the sole sanctioned allocation seam (wraps the FreeRTOS heap).
 * Direct pvPortMalloc/vPortFree are forbidden (check-no-direct-kernel-alloc). */
extern void* nros_platform_alloc(size_t size);
extern void nros_platform_dealloc(void* ptr);

/* ---- Executor storage sizing ----
 *
 * Same rule and the same hazard as `freertos_run_tiers.c`, deliberately spelled
 * the same way: prefer the REAL per-build size when the generated header is
 * visible to this compile, because the hardcoded fallback silently went 32
 * bytes short on Zephyr once and the symptom was heap corruption (issue
 * #245). */
#if defined(__has_include)
#if __has_include(<nros/nros_cpp_config_generated.h>)
#include <nros/nros_cpp_config_generated.h>
#endif
#endif
#ifdef NROS_CPP_EXECUTOR_STORAGE_SIZE
#define NROS_RTOS_COMPONENT_STORAGE_BYTES ((NROS_CPP_EXECUTOR_STORAGE_SIZE + 7u) & ~7u)
#elif defined(__ZEPHYR__)
/* Zephyr's `run_tiers` sibling carries 96 KiB rather than 80 — kept, not
 * averaged: a fallback that is too small is issue #245's heap corruption, and
 * the extra 16 KiB is only reached when the generated header is invisible. */
#define NROS_RTOS_COMPONENT_STORAGE_BYTES 98304u
#else
#define NROS_RTOS_COMPONENT_STORAGE_BYTES 81920u
#endif

/* Mirrors `nros_c_entry_setup_fn` in <nros/main.h>. Declared locally for the
 * same reason the CFFI externs are. */
typedef int32_t (*nros_c_component_setup_fn)(void* executor);

/* NROS_CPP_RET_* — the three this file can return itself. */
#define NROS_RUN_COMPONENTS_RET_ERROR (-1)
#define NROS_RUN_COMPONENTS_RET_INVALID_ARGUMENT (-3)

/* ---- The bounded spin budget ----
 *
 * `$NROS_ENTRY_SPIN_MS` is the bounded external-observer test path, and the
 * C++ sibling reads it behind `#if defined(NROS_CPP_STD) || __STDC_HOSTED__`.
 * The same guard applies here and for the same reason: a freestanding FreeRTOS
 * firmware image has no environment to read, while the POSIX simulator board
 * does and the test lane uses it. Where there is no environment the bound is
 * simply absent and the loop is unbounded, which is what firmware wants. */
#if defined(NROS_CPP_STD) || (__STDC_HOSTED__ + 0)
#include <stdlib.h>
static uint32_t nros_freertos_entry_spin_bound_ms(void) {
    const char* env = getenv("NROS_ENTRY_SPIN_MS");
    if (env == NULL || env[0] == '\0') {
        return 0u;
    }
    /* Hand-parsed rather than `strtoul`, matching `detail::entry_parse_u32`:
     * the C++ sibling cannot use the C library here either, and two parsers
     * that disagree about a malformed value would be a difference nobody
     * reads. Non-digits end the number, as there. */
    uint32_t v = 0u;
    for (const char* s = env; *s >= '0' && *s <= '9'; ++s) {
        v = v * 10u + (uint32_t)(*s - '0');
    }
    return v;
}
#else
static uint32_t nros_freertos_entry_spin_bound_ms(void) {
    return 0u;
}
#endif

/* The per-tick cooperative yield, mirroring `detail::entry_tick_yield` in
 * `<nros/main.hpp>` one for one — same condition, same body, same emptiness
 * elsewhere. Zephyr is cooperatively scheduled, so each tick must release the
 * CPU to the network stack and the peer threads; the preemptive kernels do not
 * need it and the C++ path does not do it there either. */
#ifdef __ZEPHYR__
#include <zephyr/kernel.h>
#endif
static void nros_rtos_entry_tick_yield(void) {
#ifdef __ZEPHYR__
    k_yield();
#endif
}

/*
 * The C-ABI single-executor RTOS entry.
 *
 * `locator` and `domain_id` are the compile-time `NROS_ENTRY_LOCATOR` /
 * `NROS_ENTRY_DOMAIN_ID` the generated entry bakes (one ladder, in
 * `<nros/entry_config.h>`, shared with the C++ entry). `session_name` sets the
 * primary session / node name visible to `ros2 node list`; NULL or empty falls
 * back to `"node"`, the same default the C++ overloads apply.
 *
 * Sequence, identical to `FreertosBoard::run_components`: network wait, init,
 * `setup(executor)`, spin, shutdown. Returns 0 on a graceful exit, else the
 * first non-zero `setup` or spin code.
 *
 * Argument order matches every `nros_board_<rtos>_run_tiers` so the two entry
 * points read the same way at a call site.
 */
int32_t nros_board_rtos_run_components(const char* locator, uint8_t domain_id,
                                       const char* session_name,
                                       nros_c_component_setup_fn setup) {
    /* A NULL setup registers nothing, so the image would boot into a spin loop
     * over an empty executor and look like a working node that publishes
     * nothing. The C++ sibling cannot express this — a callable is required by
     * its signature — so refusing it is what keeps the C entry from having a
     * shape the C++ one does not. */
    if (setup == NULL) {
        return NROS_RUN_COMPONENTS_RET_INVALID_ARGUMENT;
    }

    nros_board_network_wait();

    const char* sn = (session_name != NULL && session_name[0] != '\0') ? session_name : "node";

    void* storage = nros_platform_alloc(NROS_RTOS_COMPONENT_STORAGE_BYTES);
    if (storage == NULL) {
        return NROS_RUN_COMPONENTS_RET_ERROR;
    }
    memset(storage, 0, NROS_RTOS_COMPONENT_STORAGE_BYTES);

    int rc = nros_cpp_init(locator, domain_id, sn, NULL, storage);
    if (rc != 0) {
        nros_platform_dealloc(storage);
        return (int32_t)rc;
    }

    int32_t out = 0;
    int32_t setup_rc = setup(storage);
    if (setup_rc != 0) {
        out = setup_rc;
    } else {
        const uint32_t bound_ms = nros_freertos_entry_spin_bound_ms();
        if (bound_ms != 0u) {
            /* Issue 0329 — the bounded path is the shared wall-clock budgeted
             * spin, so it forwards to the single `nros_cpp_spin_for` CFFI
             * rather than being a hand-rolled budget loop. Same call the C++
             * sibling reaches through `nros::spin(bound_ms)`. */
            out = (int32_t)nros_cpp_spin_for(storage, bound_ms, 10);
        } else {
            /* Unbounded (production): run until the context stops being live,
             * yielding per tick exactly where the C++ sibling does.
             *
             * `nros_rtos_entry_tick_yield()` mirrors `detail::entry_tick_yield`
             * one for one, INCLUDING that it is empty off Zephyr. That is not
             * an oversight in either: FreeRTOS and NuttX are preemptive and
             * `nros_cpp_spin_once`'s 10 ms blocking wait is the pacing, while
             * Zephyr is cooperatively scheduled and a tick that never yields
             * starves the network stack and the peer threads (the class behind
             * issues 0129/0139). Adding a `taskYIELD()` off Zephyr would make
             * the C entry behave differently from the C++ one on the same
             * board, which is what this phase exists to stop. */
            for (;;) {
                if (!nros_cpp_context_is_live(storage)) {
                    break;
                }
                int32_t last = (int32_t)nros_cpp_spin_once(storage, 10);
                if (last != 0) {
                    out = last;
                    break;
                }
                nros_rtos_entry_tick_yield();
            }
        }
    }

    /* Shutdown runs on every exit path past init, including the failing ones —
     * the C++ sibling calls `nros::shutdown()` before returning a non-zero
     * setup code, and a C entry that leaked the session instead would differ
     * exactly where an image is already in trouble. */
    nros_cpp_fini(storage);
    nros_platform_dealloc(storage);
    return out;
}
